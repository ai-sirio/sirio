import AppKit
import Foundation
import Observation
import Testing
import TillerCore
import TillerWorkspace
@testable import Tiller

@Suite(.serialized)
@MainActor
struct WorkspaceCoordinatorTests {
    @Test func preparationRunsOutsideTheWorktreeGate() async {
        let preparation = PreparationGate(blocked: true)
        let persistence = CoordinatorPersistence()
        let adapter = CoordinatorAdapter(preparation: preparation)
        let coordinator = makeCoordinator(persistence: persistence, adapter: adapter)
        let first = fixtureWorktree(number: 1)
        let second = fixtureWorktree(number: 2)
        await coordinator.restore(worktree: first)
        await coordinator.restore(worktree: second)

        let pending = Task { await coordinator.requestSplit(
            anchor: anchor(in: first), placement: .right, choice: .newTerminal, in: first) }
        await preparation.waitUntilStarted()
        await coordinator.handle(.activateGroup(anchor(in: second)), in: second)

        #expect(coordinator.revisions[second.id] == 1)
        await preparation.release()
        await pending.value
    }

    @Test func aStaleAnchorDisposesTheCandidateAndChangesNothing() async {
        let preparation = PreparationGate(blocked: true)
        let persistence = CoordinatorPersistence()
        let adapter = CoordinatorAdapter(preparation: preparation)
        let coordinator = makeCoordinator(persistence: persistence, adapter: adapter)
        let worktree = fixtureWorktree(number: 1)
        await coordinator.restore(worktree: worktree)
        let before = coordinator.layouts[worktree.id]

        let pending = Task { await coordinator.requestSplit(
            anchor: anchor(in: worktree), placement: .right, choice: .newTerminal, in: worktree) }
        await preparation.waitUntilStarted()
        await coordinator.handle(.activateGroup(anchor(in: worktree)), in: worktree)
        await preparation.release()
        await pending.value

        #expect(coordinator.layouts[worktree.id] == before)
        #expect(await persistence.commitCount == 0)
        #expect(await adapter.disposeCount == 1)
    }

    @Test func cancelledContentChoiceCreatesNoTabGroupSplitRowOrFocusChange() async {
        let persistence = CoordinatorPersistence()
        let adapter = CoordinatorAdapter(prepareError: CancellationError())
        let coordinator = makeCoordinator(persistence: persistence, adapter: adapter)
        let worktree = fixtureWorktree(number: 1)
        await coordinator.restore(worktree: worktree)
        let before = coordinator.layouts[worktree.id]

        await coordinator.requestSplit(
            anchor: anchor(in: worktree), placement: .right, choice: .newTerminal, in: worktree)

        #expect(coordinator.layouts[worktree.id] == before)
        #expect(coordinator.lastFocusIntent == .none)
        #expect(await persistence.commitCount == 0)
    }

    @Test func aFailedStructuralCommitLeavesUiOnTheLastDurableRevision() async {
        let persistence = CoordinatorPersistence(structuralError: TestFailure.failed)
        let adapter = CoordinatorAdapter()
        let coordinator = makeCoordinator(persistence: persistence, adapter: adapter)
        let worktree = fixtureWorktree(number: 1)
        await coordinator.restore(worktree: worktree)
        let before = coordinator.layouts[worktree.id]

        await coordinator.requestSplit(
            anchor: anchor(in: worktree), placement: .right, choice: .newTerminal, in: worktree)

        #expect(coordinator.layouts[worktree.id] == before)
        #expect(coordinator.revisions[worktree.id] == 0)
        #expect(coordinator.lastRecoverableError != nil)
        #expect(coordinator.registry.liveHostCount == 0)
    }

    @Test func resourcesArePublishedOnlyAfterTheDurableCommit() async {
        let persistence = CoordinatorPersistence()
        let adapter = CoordinatorAdapter(persistence: persistence)
        let coordinator = makeCoordinator(persistence: persistence, adapter: adapter)
        let worktree = fixtureWorktree(number: 1)
        await coordinator.restore(worktree: worktree)
        #expect(coordinator.layouts[worktree.id]?.group(anchor(in: worktree)) != nil)

        await coordinator.requestSplit(
            anchor: anchor(in: worktree), placement: .right, choice: .newTerminal, in: worktree)

        let events = await persistence.events
        #expect(await adapter.prepareCount == 1)
        #expect(events.first == "commit")
        #expect(coordinator.registry.liveHostCount == 1)
    }

    @Test func twoWorktreesCommitConcurrentlyWithIndependentGates() async {
        let persistence = CoordinatorPersistence(blockFirstCommit: true)
        let adapter = CoordinatorAdapter()
        let coordinator = makeCoordinator(persistence: persistence, adapter: adapter)
        let first = fixtureWorktree(number: 1)
        let second = fixtureWorktree(number: 2)
        await coordinator.restore(worktree: first)
        await coordinator.restore(worktree: second)

        let firstCommit = Task { await coordinator.requestSplit(
            anchor: anchor(in: first), placement: .right, choice: .newTerminal, in: first) }
        await persistence.waitUntilFirstCommitStarted()
        let secondCommit = Task { await coordinator.requestSplit(
            anchor: anchor(in: second), placement: .right, choice: .newTerminal, in: second) }
        await secondCommit.value

        #expect(coordinator.layouts[second.id]?.orderedGroupIDs.count == 2)
        await persistence.releaseFirstCommit()
        await firstCommit.value
    }

    @Test func aFailedTeardownLeavesADurableCleanupObligationRetriedOnLaunch() async {
        let persistence = CoordinatorPersistence(purgeFailures: 1)
        let coordinator = makeCoordinator(persistence: persistence, adapter: CoordinatorAdapter())
        let worktree = fixtureWorktree(number: 1)

        await coordinator.purge(worktree: worktree)
        #expect(coordinator.pendingCleanupObligations.contains(worktree.id))
        await coordinator.restore(worktree: worktree)

        #expect(!coordinator.pendingCleanupObligations.contains(worktree.id))
        #expect(await persistence.purgeCount == 2)
    }

    @Test func quitCheckpointsWithoutApplyingCloseSemantics() async {
        let persistence = CoordinatorPersistence()
        let adapter = CoordinatorAdapter()
        let coordinator = makeCoordinator(persistence: persistence, adapter: adapter)
        let worktree = fixtureWorktree(number: 1)
        await coordinator.restore(worktree: worktree)
        await coordinator.requestSplit(
            anchor: anchor(in: worktree), placement: .right, choice: .newTerminal, in: worktree)
        let closeCountBeforeQuit = await adapter.closeCount

        await coordinator.checkpointOnQuit()

        #expect(await adapter.checkpointCount == 2)
        #expect(await adapter.closeCount == closeCountBeforeQuit)
        #expect(await persistence.checkpointCount == 1)
        #expect(await persistence.flushCount == 1)
    }

    @Test func containerRemovalPurgesTillerArtifactsButNeverSourceFiles() async {
        let persistence = CoordinatorPersistence()
        let coordinator = makeCoordinator(persistence: persistence, adapter: CoordinatorAdapter())
        let worktree = fixtureWorktree(number: 1)

        await coordinator.purge(worktree: worktree)

        #expect(await persistence.purgeCount == 1)
        #expect(await persistence.sourceFileDeletionCount == 0)
    }

    @Test func movingATabPreservesTabIdContentIdGenerationHostAndViewState() async {
        let tabID = WorkspaceTabID(UUID(uuidString: "00000000-0000-4000-8000-000000000010")!)
        let contentID = TerminalContentID(UUID(uuidString: "00000000-0000-4000-8000-000000000011")!)
        var viewState = WorkspaceTabViewState.empty
        viewState.followsTail = true
        viewState.terminalViewportAnchor = 0.75
        let tab = WorkspaceTab(
            id: tabID, title: "kept", titleIsAutoNamed: false,
            content: .terminal(contentID), viewState: viewState)
        let persistence = CoordinatorPersistence(restored: layoutWith(tab: tab))
        let adapter = CoordinatorAdapter()
        let coordinator = makeCoordinator(persistence: persistence, adapter: adapter)
        let worktree = fixtureWorktree(number: 1)
        await coordinator.restore(worktree: worktree)
        let generation = ResourceGenerationID()
        let host = adapter.host(for: tabID)
        coordinator.registry.adopt(host, tab: tab, generation: generation)

        await coordinator.requestSplit(
            anchor: anchor(in: worktree), placement: .right,
            choice: .moveExistingTab(tabID), in: worktree)

        let moved = coordinator.layouts[worktree.id]?.tab(tabID)
        #expect(moved == tab)
        #expect(coordinator.registry.host(for: tabID) === host)
        #expect(coordinator.registry.generation(for: tabID) == generation)
        #expect(await adapter.prepareCount == 0)
        #expect(await adapter.closeCount == 0)
    }

    private func makeCoordinator(persistence: CoordinatorPersistence,
                                 adapter: CoordinatorAdapter) -> WorkspaceCoordinator {
        WorkspaceCoordinator(
            persistence: persistence,
            registry: WorkspaceContentRegistry(),
            adapters: [.terminal: adapter])
    }

    private func fixtureWorktree(number: Int) -> Worktree {
        Worktree(
            id: UUID(uuidString: "00000000-0000-4000-8000-00000000010\(number)")!,
            projectId: UUID(uuidString: "00000000-0000-4000-8000-000000000001")!,
            branch: "branch-\(number)", path: "/tmp/worktree-\(number)")
    }

    private func anchor(in worktree: Worktree) -> PaneGroupID {
        WorkspaceLayout.empty(groupID: PaneGroupID(worktree.id)).activeGroupID
    }

    private func layoutWith(tab: WorkspaceTab) -> WorkspaceLayout {
        let group = PaneGroup(id: PaneGroupID(), tabs: [tab], activeTabID: tab.id)
        return try! WorkspaceLayout.make(
            root: .group(group.id), groups: [group.id: group], activeGroupID: group.id).get()
    }
}

private enum TestFailure: Error { case failed }

private actor PreparationGate {
    private let blocked: Bool
    private var started = false
    private var released = false
    private var waiters: [CheckedContinuation<Void, Never>] = []
    private var releaseWaiters: [CheckedContinuation<Void, Never>] = []

    init(blocked: Bool) { self.blocked = blocked }

    func waitUntilStarted() async {
        if started { return }
        await withCheckedContinuation { waiters.append($0) }
    }

    func markStarted() {
        started = true
        let continuations = waiters
        waiters.removeAll()
        continuations.forEach { $0.resume() }
    }

    func release() {
        released = true
        let continuations = releaseWaiters
        releaseWaiters.removeAll()
        continuations.forEach { $0.resume() }
    }

    func waitIfBlocked() async {
        guard blocked, !released else { return }
        await withCheckedContinuation { releaseWaiters.append($0) }
    }
}

private actor CoordinatorPersistence: WorkspaceLayoutPersistence {
    private var restored: [UUID: RestoredWorkspace] = [:]
    private let structuralError: Error?
    private let blockFirstCommit: Bool
    private var firstCommitStarted = false
    private var firstCommitReleased = false
    private var firstCommitWaiters: [CheckedContinuation<Void, Never>] = []
    private var firstCommitReleaseWaiters: [CheckedContinuation<Void, Never>] = []
    private var purgeFailures: Int
    private(set) var structuralCommits = 0
    private(set) var commitCount = 0
    private(set) var checkpointCount = 0
    private(set) var flushCount = 0
    private(set) var purgeCount = 0
    private(set) var sourceFileDeletionCount = 0
    private(set) var events: [String] = []

    init(restored: WorkspaceLayout? = nil, structuralError: Error? = nil,
         blockFirstCommit: Bool = false, purgeFailures: Int = 0) {
        self.structuralError = structuralError
        self.blockFirstCommit = blockFirstCommit
        self.purgeFailures = purgeFailures
        if let restored {
            let id = UUID(uuidString: "00000000-0000-4000-8000-000000000101")!
            self.restored[id] = RestoredWorkspace(
                layout: restored, tabs: Dictionary(uniqueKeysWithValues: restored.allTabs.map { ($0.id, $0) }),
                revision: 0, diagnostics: [])
        }
    }

    func restore(worktreeID: UUID) async -> RestoredWorkspace {
        if let stored = restored[worktreeID] { return stored }
        let groupID = PaneGroupID(worktreeID)
        let tabID = WorkspaceTabID(worktreeID)
        let tab = WorkspaceTab(
            id: tabID, title: "restored", titleIsAutoNamed: true,
            content: .terminal(TerminalContentID(worktreeID)))
        let group = PaneGroup(id: groupID, tabs: [tab], activeTabID: tabID)
        let layout = try! WorkspaceLayout.make(
            root: .group(groupID), groups: [groupID: group], activeGroupID: groupID).get()
        return RestoredWorkspace(
            layout: layout, tabs: [tabID: tab], revision: 0, diagnostics: [])
    }

    func commitStructural(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot,
                          tabs: [WorkspaceTab], terminalContents: [TerminalContentRecordValue]) async throws {
        commitCount += 1
        if blockFirstCommit, !firstCommitStarted {
            firstCommitStarted = true
            events.append("commit")
            let continuations = firstCommitWaiters
            firstCommitWaiters.removeAll()
            continuations.forEach { $0.resume() }
            if !firstCommitReleased {
                await withCheckedContinuation { firstCommitReleaseWaiters.append($0) }
            }
        }
        if let structuralError { throw structuralError }
        structuralCommits += 1
        events.append("commit")
    }

    func checkpoint(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) async {
        checkpointCount += 1
        events.append("checkpoint")
    }

    func flush(worktreeID: UUID) async throws { flushCount += 1 }
    nonisolated func writeRecoverySidecar(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) throws {}

    func purge(worktreeID: UUID) async throws {
        purgeCount += 1
        if purgeFailures > 0 {
            purgeFailures -= 1
            throw TestFailure.failed
        }
    }

    func waitUntilFirstCommitStarted() async {
        if firstCommitStarted { return }
        await withCheckedContinuation { firstCommitWaiters.append($0) }
    }

    func releaseFirstCommit() {
        firstCommitReleased = true
        let continuations = firstCommitReleaseWaiters
        firstCommitReleaseWaiters.removeAll()
        continuations.forEach { $0.resume() }
    }
}

@MainActor
private final class CoordinatorAdapter: WorkspaceContentAdapter {
    let kind: WorkspaceContentKind = .terminal
    private let preparation: PreparationGate
    private let prepareError: Error?
    private weak var persistence: CoordinatorPersistence?
    private(set) var prepareCount = 0
    private(set) var disposeCount = 0
    private(set) var closeCount = 0
    private(set) var checkpointCount = 0
    private var hosts: [WorkspaceTabID: CoordinatorHost] = [:]

    init(preparation: PreparationGate = PreparationGate(blocked: false),
         prepareError: Error? = nil, persistence: CoordinatorPersistence? = nil) {
        self.preparation = preparation
        self.prepareError = prepareError
        self.persistence = persistence
    }

    func prepare(request: ContentRequest, worktree: Worktree) async throws -> PreparedContent {
        prepareCount += 1
        await preparation.markStarted()
        await preparation.waitIfBlocked()
        if let prepareError { throw prepareError }
        let tab = WorkspaceTab(id: WorkspaceTabID(), title: "candidate", titleIsAutoNamed: true,
                              content: .terminal(TerminalContentID()))
        return PreparedContent(tab: tab, generationID: ResourceGenerationID(), opaqueToken: NSObject())
    }

    func hydrate(tab: WorkspaceTab, worktree: Worktree) async {}

    func makeHost(tab: WorkspaceTab, worktree: Worktree) -> WorkspaceContentHost {
        let host = CoordinatorHost(tabID: tab.id)
        hosts[tab.id] = host
        return host
    }

    func host(for tabID: WorkspaceTabID) -> CoordinatorHost {
        hosts[tabID] ?? CoordinatorHost(tabID: tabID)
    }

    func checkpoint(tab: WorkspaceTab) async { checkpointCount += 1 }
    func retry(tab: WorkspaceTab, worktree: Worktree) async -> ResourceGenerationID { ResourceGenerationID() }
    func close(tab: WorkspaceTab) async { closeCount += 1 }
    func dispose(prepared: PreparedContent) async { disposeCount += 1 }
}

@MainActor
private final class CoordinatorHost: WorkspaceContentHost {
    let tabID: WorkspaceTabID
    let viewController = NSViewController()
    init(tabID: WorkspaceTabID) { self.tabID = tabID }
    func setVisible(_ isVisible: Bool) {}
    func fulfill(_ intent: FocusIntent) -> Bool { true }
}
