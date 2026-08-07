import AppKit
import Foundation
import GRDB
import Testing
import TillerCore
import TillerPersistence
import TillerWorkspace
@testable import Tiller

@Suite(.serialized)
@MainActor
struct WorkspaceResourceLifecycleTests {
    @Test
    func reconcilingAfterActivatingADifferentGroupPreparesNoNewContent() async {
        let layout = makeTwoGroupLayout()
        let persistence = LifecyclePersistence(restored: layout)
        let adapter = LifecycleAdapter()
        let registry = WorkspaceContentRegistry()
        let coordinator = WorkspaceCoordinator(
            persistence: persistence, registry: registry, adapters: [.terminal: adapter])
        let worktree = fixtureWorktree()

        await coordinator.restore(worktree: worktree)
        let tabs = layout.allTabs
        _ = coordinator.host(for: tabs[0].id)
        _ = coordinator.host(for: tabs[1].id)
        let prepareCount = adapter.prepareCount
        let makeHostCount = adapter.makeHostCount
        let liveHostCount = registry.liveHostCount

        let secondGroupID = layout.orderedGroupIDs[1]
        await coordinator.handle(.activateGroup(secondGroupID), in: worktree)
        await coordinator.handle(.activateTab(tabs[1].id), in: worktree)

        #expect(adapter.prepareCount == prepareCount)
        #expect(adapter.makeHostCount == makeHostCount)
        #expect(registry.liveHostCount == liveHostCount)
    }

    @Test
    func oneHundredCreateMoveRemountCloseRetryCyclesReturnCountsToBaseline() async throws {
        let persistence = LifecyclePersistence(restored: makeSingleGroupLayout())
        let adapter = LifecycleAdapter()
        let registry = WorkspaceContentRegistry()
        let coordinator = WorkspaceCoordinator(
            persistence: persistence, registry: registry, adapters: [.terminal: adapter])
        let worktree = fixtureWorktree()
        await coordinator.restore(worktree: worktree)
        let baseline = registry.liveHostCount

        for _ in 0..<100 {
            let groupID = try #require(coordinator.activeOrFirstGroup(for: worktree.id))
            await coordinator.handle(.requestNewTab(into: groupID), in: worktree)
            let tab = try #require(coordinator.layouts[worktree.id]?.allTabs.last)
            let anchor = coordinator.layouts[worktree.id]!.activeGroupID
            await coordinator.handle(
                .requestMove(tab.id, to: .edgeSplit(anchor: anchor, placement: .right)),
                in: worktree)
            await coordinator.handle(.retryContent(tab.id), in: worktree)
            await coordinator.handle(.requestClose(tab.id), in: worktree)
        }

        #expect(registry.liveHostCount == baseline)
        #expect(coordinator.pendingCleanupObligations.isEmpty)
    }

    @Test
    func restoreDecodesAndValidatesOffTheMainActor() async throws {
        let database = try makeDatabase()
        let worktreeID = fixtureWorktree().id
        let layout = WorkspaceLayout.empty(groupID: PaneGroupID(worktreeID))
        let snapshot = WorkspaceSnapshot(layout: layout)
        let payload = try snapshot.canonicalPayload()
        try database.write { db in
            try WorkspaceLayoutRecord(
                worktreeId: worktreeID.uuidString,
                schemaVersion: snapshot.schemaVersion,
                revision: 4,
                payload: String(decoding: payload, as: UTF8.self),
                checksum: "wrong",
                updatedAt: Date()).save(db)
        }

        let observation = ThreadObservation()
        let persistence = SQLiteWorkspacePersistence(
            database: database,
            now: {
                observation.record(isMainThread: Thread.isMainThread)
                return Date(timeIntervalSince1970: 0)
            })

        let restored = await persistence.restore(worktreeID: worktreeID)

        #expect(restored.diagnostics.contains(.quarantinedSnapshot(reason: "checksum")))
        #expect(observation.values.contains(false))
    }

    private func makeTwoGroupLayout() -> WorkspaceLayout {
        let firstGroupID = PaneGroupID()
        let secondGroupID = PaneGroupID()
        let firstTab = WorkspaceTab(
            id: WorkspaceTabID(), title: "First", titleIsAutoNamed: true,
            content: .terminal(TerminalContentID()))
        let secondTab = WorkspaceTab(
            id: WorkspaceTabID(), title: "Second", titleIsAutoNamed: true,
            content: .terminal(TerminalContentID()))
        let groups = [
            firstGroupID: PaneGroup(id: firstGroupID, tabs: [firstTab], activeTabID: firstTab.id),
            secondGroupID: PaneGroup(id: secondGroupID, tabs: [secondTab], activeTabID: secondTab.id)
        ]
        return try! WorkspaceLayout.make(
            root: .split(
                id: SplitID(), axis: .horizontal, fraction: 0.5,
                first: .group(firstGroupID), second: .group(secondGroupID)),
            groups: groups, activeGroupID: firstGroupID).get()
    }

    private func makeSingleGroupLayout() -> WorkspaceLayout {
        let groupID = PaneGroupID()
        let tab = WorkspaceTab(
            id: WorkspaceTabID(), title: "Existing", titleIsAutoNamed: true,
            content: .terminal(TerminalContentID()))
        return try! WorkspaceLayout.make(
            root: .group(groupID),
            groups: [groupID: PaneGroup(id: groupID, tabs: [tab], activeTabID: tab.id)],
            activeGroupID: groupID).get()
    }

    private func fixtureWorktree() -> Worktree {
        Worktree(
            id: UUID(uuidString: "00000000-0000-4000-8000-000000000101")!,
            projectId: UUID(uuidString: "00000000-0000-4000-8000-000000000001")!,
            branch: "main", path: "/tmp/worktree")
    }

    private func makeDatabase() throws -> AppDatabase {
        let database = try AppDatabase.inMemory()
        let worktree = fixtureWorktree()
        try database.write { db in
            try ProjectRecord(
                id: worktree.projectId.uuidString, name: "Project", rootPath: "/tmp/project",
                createdAt: Date()).insert(db)
            try WorktreeRecord(
                id: worktree.id.uuidString, projectId: worktree.projectId.uuidString,
                branch: worktree.branch, path: worktree.path, createdAt: Date()).insert(db)
        }
        return database
    }
}

private actor LifecyclePersistence: WorkspaceLayoutPersistence {
    private let restored: WorkspaceLayout?

    init(restored: WorkspaceLayout? = nil) {
        self.restored = restored
    }

    func restore(worktreeID: UUID) async -> RestoredWorkspace {
        if let restored {
            return RestoredWorkspace(
                layout: restored,
                tabs: Dictionary(uniqueKeysWithValues: restored.allTabs.map { ($0.id, $0) }),
                revision: 0,
                diagnostics: [])
        }
        return RestoredWorkspace(layout: .empty(), tabs: [:], revision: 0, diagnostics: [])
    }

    func commitStructural(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot,
                          tabs: [WorkspaceTab], terminalContents: [TerminalContentRecordValue],
                          browserContents: [BrowserContentRecordValue])
        async throws {}

    func checkpoint(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) async {}
    func flush(worktreeID: UUID) async throws {}
    nonisolated func writeRecoverySidecar(worktreeID: UUID, revision: Int,
                                          snapshot: WorkspaceSnapshot) throws {}
    func purge(worktreeID: UUID) async throws {}
}

@MainActor
private final class LifecycleAdapter: WorkspaceContentAdapter {
    let kind: WorkspaceContentKind = .terminal
    private(set) var prepareCount = 0
    private(set) var makeHostCount = 0

    func prepare(request: ContentRequest, worktree: Worktree) async throws -> PreparedContent {
        prepareCount += 1
        let tab = WorkspaceTab(
            id: WorkspaceTabID(), title: "candidate", titleIsAutoNamed: true,
            content: .terminal(TerminalContentID()))
        return PreparedContent(
            tab: tab, generationID: ResourceGenerationID(), opaqueToken: NSObject())
    }

    func hydrate(tab: WorkspaceTab, worktree: Worktree) async {}

    func makeHost(tab: WorkspaceTab, worktree: Worktree) -> WorkspaceContentHost {
        makeHostCount += 1
        return LifecycleHost(tabID: tab.id)
    }

    func checkpoint(tab: WorkspaceTab) async {}
    func retry(tab: WorkspaceTab, worktree: Worktree) async -> ResourceGenerationID {
        ResourceGenerationID()
    }
    func close(tab: WorkspaceTab) async {}
    func dispose(prepared: PreparedContent) async {}
}

@MainActor
private final class LifecycleHost: WorkspaceContentHost {
    let tabID: WorkspaceTabID
    let viewController = NSViewController()

    init(tabID: WorkspaceTabID) { self.tabID = tabID }
    func setVisible(_ isVisible: Bool) {}
    func fulfill(_ intent: FocusIntent) -> Bool { true }
}

private final class ThreadObservation: @unchecked Sendable {
    private let lock = NSLock()
    private(set) var values: [Bool] = []

    func record(isMainThread: Bool) {
        lock.lock()
        values.append(isMainThread)
        lock.unlock()
    }
}
