import AppKit
import Foundation
import Observation
import Testing
import TillerCore
import TillerTerminal
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
            anchor: anchor(in: first), placement: .right,
            choice: .newTerminal(command: nil), in: first) }
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
            anchor: anchor(in: worktree), placement: .right,
            choice: .newTerminal(command: nil), in: worktree) }
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
            anchor: anchor(in: worktree), placement: .right,
            choice: .newTerminal(command: nil), in: worktree)

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
            anchor: anchor(in: worktree), placement: .right,
            choice: .newTerminal(command: nil), in: worktree)

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
            anchor: anchor(in: worktree), placement: .right,
            choice: .newTerminal(command: nil), in: worktree)

        let events = await persistence.events
        #expect(await adapter.prepareCount == 1)
        #expect(events.first == "commit")
        #expect(coordinator.registry.liveHostCount == 1)
    }

    @Test func requestNewTabOnAFreshEmptyWorktreeInsertsIntoTheSoleGroupWithoutSplitting() async {
        let persistence = CoordinatorPersistence(emptyUntouchedWorktrees: true)
        let adapter = CoordinatorAdapter()
        let coordinator = makeCoordinator(persistence: persistence, adapter: adapter)
        let worktree = fixtureWorktree(number: 3)

        await coordinator.restore(worktree: worktree)
        let before = coordinator.layouts[worktree.id]!
        let groupID = before.activeGroupID

        await coordinator.requestNewTab(
            into: groupID, choice: .newTerminal(command: nil), in: worktree)

        let after = coordinator.layouts[worktree.id]!
        #expect(after.orderedGroupIDs == [groupID])
        #expect(after.splitIDs().isEmpty)
        #expect(after.group(groupID)?.tabs.count == 1)
        #expect(after.group(groupID)?.activeTabID != nil)
        #expect(await persistence.commitCount == 1)
    }

    @Test func requestNewTabAppendsASiblingTabIntoAnExistingNonEmptyGroup() async {
        let persistence = CoordinatorPersistence()
        let adapter = CoordinatorAdapter()
        let coordinator = makeCoordinator(persistence: persistence, adapter: adapter)
        let worktree = fixtureWorktree(number: 1)

        await coordinator.restore(worktree: worktree)
        let before = coordinator.layouts[worktree.id]!
        let groupID = before.activeGroupID
        let existingTabID = before.group(groupID)!.tabs[0].id

        await coordinator.requestNewTab(
            into: groupID, choice: .newTerminal(command: nil), in: worktree)

        let after = coordinator.layouts[worktree.id]!
        #expect(after.orderedGroupIDs == before.orderedGroupIDs)
        #expect(after.splitIDs() == before.splitIDs())
        #expect(after.group(groupID)?.tabs.count == 2)
        #expect(after.group(groupID)?.tabs.contains { $0.id == existingTabID } == true)
    }

    @Test func requestNewTabForAnAgentGoesThroughTheSameAgentTerminalAdapterPathAsSplit() async {
        let persistence = CoordinatorPersistence(emptyUntouchedWorktrees: true)
        let adapter = CoordinatorAdapter()
        let coordinator = makeCoordinator(persistence: persistence, adapter: adapter)
        let worktree = fixtureWorktree(number: 4)

        await coordinator.restore(worktree: worktree)
        await coordinator.requestNewTab(
            into: coordinator.layouts[worktree.id]!.activeGroupID,
            choice: .agentTerminal(agentID: "codex"), in: worktree)

        #expect(adapter.preparedRequests == [.agentTerminal(agentID: "codex")])
        #expect(coordinator.layouts[worktree.id]?.allTabs.count == 1)
        #expect(coordinator.layouts[worktree.id]?.allTabs.first?.content.kind == .terminal)
    }

    @Test func newShellTabWithTheEngineEnabledIsVisibleThroughWorkspaceCoordinatorLayouts() async {
        guard WorkspaceEngineGate.isEnabled else { return }

        let persistence = CoordinatorPersistence(emptyUntouchedWorktrees: true)
        let adapter = CoordinatorAdapter()
        let coordinator = makeCoordinator(persistence: persistence, adapter: adapter)
        let model = AppModel(
            paneRegistry: PaneRegistry(),
            workspaceCoordinator: coordinator)
        let worktree = fixtureWorktree(number: 5)
        model.worktrees = [worktree.projectId: [worktree]]
        await coordinator.restore(worktree: worktree)

        model.newShellTab(in: worktree)
        for _ in 0..<100 where coordinator.layouts[worktree.id]?.allTabs.isEmpty == true {
            try? await Task.sleep(for: .milliseconds(1))
        }

        #expect(coordinator.layouts[worktree.id]?.allTabs.count == 1)
        #expect(coordinator.legacyTabs(for: worktree.id).isEmpty)
    }

    @Test func liveControlPaneIdIsNilBeforeTheHostHydrates() async {
        let persistence = CoordinatorPersistence(emptyUntouchedWorktrees: true)
        let adapter = TerminalContentAdapter()
        let coordinator = makeCoordinator(
            persistence: persistence, adapter: adapter)
        let worktree = fixtureWorktree(number: 6)
        await coordinator.restore(worktree: worktree)

        let prepared = try? await adapter.prepare(
            request: .newTerminal(command: nil), worktree: worktree)
        guard let prepared,
              case .terminal(let contentID) = prepared.tab.content else {
            Issue.record("expected a prepared terminal candidate")
            return
        }
        #expect(coordinator.liveControlPaneId(contentID: contentID, in: worktree.id) == nil)
        await adapter.dispose(prepared: prepared)
    }

    @Test func liveControlPaneIdMatchesTheCurrentGenerationAfterHydration() async {
        let persistence = CoordinatorPersistence(emptyUntouchedWorktrees: true)
        let adapter = TerminalContentAdapter()
        let coordinator = makeCoordinator(persistence: persistence, adapter: adapter)
        let worktree = fixtureWorktree(number: 7)
        await coordinator.restore(worktree: worktree)
        let groupID = coordinator.layouts[worktree.id]!.activeGroupID

        await coordinator.requestNewTab(
            into: groupID, choice: .newTerminal(command: nil), in: worktree)

        let tab = coordinator.layouts[worktree.id]!.allTabs[0]
        guard case .terminal(let contentID) = tab.content else {
            Issue.record("expected a terminal tab")
            return
        }
        #expect(coordinator.terminalContentID(for: tab.id, in: worktree.id) == contentID)
        #expect(coordinator.liveControlPaneId(contentID: contentID, in: worktree.id)
                == adapter.generation(for: tab.id)?.rawValue)
    }

    @Test func liveControlPaneIdIsNilForANonTerminalTab() async {
        let chatID = ChatContentID(UUID().uuidString)
        let tab = WorkspaceTab(
            id: WorkspaceTabID(), title: "chat", titleIsAutoNamed: true,
            content: .chat(chatID))
        let persistence = CoordinatorPersistence(restored: layoutWith(tab: tab))
        let coordinator = WorkspaceCoordinator(
            persistence: persistence,
            registry: WorkspaceContentRegistry(),
            adapters: [.terminal: TerminalContentAdapter()])
        let worktree = fixtureWorktree(number: 1)
        await coordinator.restore(worktree: worktree)

        #expect(coordinator.liveControlPaneId(contentID: TerminalContentID(), in: worktree.id) == nil)
    }

    @Test func liveControlPaneIdIsNilForAContentIdInTheWrongWorktree() async {
        let persistence = CoordinatorPersistence(emptyUntouchedWorktrees: true)
        let coordinator = makeCoordinator(
            persistence: persistence, adapter: TerminalContentAdapter())
        let owner = fixtureWorktree(number: 9)
        let other = fixtureWorktree(number: 0)
        await coordinator.restore(worktree: owner)
        await coordinator.restore(worktree: other)
        let groupID = coordinator.layouts[owner.id]!.activeGroupID

        await coordinator.requestNewTab(
            into: groupID, choice: .newTerminal(command: nil), in: owner)
        guard let tab = coordinator.layouts[owner.id]!.allTabs.first,
              case .terminal(let contentID) = tab.content else {
            Issue.record("expected a terminal tab")
            return
        }

        #expect(coordinator.liveControlPaneId(contentID: contentID, in: other.id) == nil)
    }

    /// `restore()` hydrates a restored terminal's PTY (LC-3), but must never
    /// create its host: a worktree with dozens of restored tabs would
    /// otherwise instantiate a host for every one of them regardless of
    /// visibility. The host for a tab that already existed before this
    /// session is created lazily, the first time something actually asks to
    /// render it — which is exactly what `coordinator.host(for:)` (the
    /// `WorkspaceHostProvider` conformance the renderer calls) does.
    @Test func restoredTabsGetNoHostUntilTheRenderPathAsksForOne() async {
        let tab = WorkspaceTab(
            id: WorkspaceTabID(), title: "restored", titleIsAutoNamed: true,
            content: .terminal(TerminalContentID()))
        let persistence = CoordinatorPersistence(restored: layoutWith(tab: tab))
        let adapter = CoordinatorAdapter(persistence: persistence)
        let coordinator = makeCoordinator(persistence: persistence, adapter: adapter)
        let worktree = fixtureWorktree(number: 1)

        await coordinator.restore(worktree: worktree)
        #expect(coordinator.registry.liveHostCount == 0)

        let firstLookup = coordinator.host(for: tab.id)
        #expect(firstLookup != nil)
        #expect(coordinator.registry.liveHostCount == 1)

        let secondLookup = coordinator.host(for: tab.id)
        #expect(secondLookup === firstLookup)
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
            anchor: anchor(in: first), placement: .right,
            choice: .newTerminal(command: nil), in: first) }
        await persistence.waitUntilFirstCommitStarted()
        let secondCommit = Task { await coordinator.requestSplit(
            anchor: anchor(in: second), placement: .right,
            choice: .newTerminal(command: nil), in: second) }
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
            anchor: anchor(in: worktree), placement: .right,
            choice: .newTerminal(command: nil), in: worktree)
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
                                 adapter: WorkspaceContentAdapter) -> WorkspaceCoordinator {
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

    private let emptyUntouchedWorktrees: Bool

    init(restored: WorkspaceLayout? = nil, structuralError: Error? = nil,
         blockFirstCommit: Bool = false, purgeFailures: Int = 0,
         emptyUntouchedWorktrees: Bool = false) {
        self.structuralError = structuralError
        self.blockFirstCommit = blockFirstCommit
        self.purgeFailures = purgeFailures
        self.emptyUntouchedWorktrees = emptyUntouchedWorktrees
        if let restored {
            let id = UUID(uuidString: "00000000-0000-4000-8000-000000000101")!
            self.restored[id] = RestoredWorkspace(
                layout: restored, tabs: Dictionary(uniqueKeysWithValues: restored.allTabs.map { ($0.id, $0) }),
                revision: 0, diagnostics: [])
        }
    }

    func restore(worktreeID: UUID) async -> RestoredWorkspace {
        if let stored = restored[worktreeID] { return stored }
        if emptyUntouchedWorktrees {
            let layout = WorkspaceLayout.empty(groupID: PaneGroupID(worktreeID))
            return RestoredWorkspace(layout: layout, tabs: [:], revision: 0, diagnostics: [])
        }
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
    private(set) var preparedRequests: [ContentRequest] = []
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
        preparedRequests.append(request)
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
