import Testing
import Foundation
@testable import TillerCore

/// Characterization tests for AgentActivityModel — each test encodes the
/// current AppModel agent-status behaviour as extracted into the pure model.
/// All tests pass an explicit `now: Date` for determinism.
///
/// Architecture: Layer A (hook pushes), Layer B (title-derived), process
/// exit fallback, and pane-close teardown. Each layer has its own section.

// MARK: - Layer A: explicit hook push

@Test func notifySetsStatusAndRecordsTimestamp() {
    let model = AgentActivityModel()
    let paneId = UUID()
    let now = Date()

    let t = model.notify(paneId: paneId, status: .needsInput, now: now)

    #expect(t.paneId == paneId)
    #expect(t.old == nil)
    #expect(t.new == .needsInput)
    #expect(model.agentStatus[paneId] == .needsInput)
    #expect(model.lastHookUpdateAt[paneId] == now)
}

@Test func notifyOverwritesPreviousStatus() {
    let model = AgentActivityModel()
    let paneId = UUID()
    let t1 = model.notify(paneId: paneId, status: .running, now: Date())
    let t2 = model.notify(paneId: paneId, status: .done, now: Date())

    #expect(t1.old == nil)
    #expect(t1.new == .running)
    #expect(t2.old == .running)
    #expect(t2.new == .done)
    #expect(model.agentStatus[paneId] == .done)
}

@Test func notifySameStatusReportsCorrectOldAndNew() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.notify(paneId: paneId, status: .done, now: Date())

    let t = model.notify(paneId: paneId, status: .done, now: Date())
    #expect(t.old == .done)
    #expect(t.new == .done)
}

// MARK: - Spawn

@Test func agentSpawnedSetsRunningAndAgentId() {
    let model = AgentActivityModel()
    let paneId = UUID()
    let now = Date()

    model.agentSpawned(paneId: paneId, agentId: "claude", now: now)

    #expect(model.agentStatus[paneId] == .running)
    #expect(model.paneAgents[paneId] == "claude")
    #expect(model.lastHookUpdateAt[paneId] == now)
    #expect(model.titleOwnedPanes.isEmpty)
}

@Test func agentSpawnedReturnsNoTransition() {
    // agentSpawned returns Void — running at spawn is not a notification event.
    let model = AgentActivityModel()
    model.agentSpawned(paneId: UUID(), agentId: "codex", now: Date())
    // Compile-time check: no @discardableResult warning needed.
}

// MARK: - Process exit

@Test func applyExitResultMapsZeroToDone() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.agentSpawned(paneId: paneId, agentId: "codex", now: Date())

    let t = model.applyExitResult(paneId: paneId, exitCode: 0, now: Date())

    #expect(t?.paneId == paneId)
    #expect(t?.old == .running)
    #expect(t?.new == .done)
    #expect(model.agentStatus[paneId] == .done)
}

@Test func applyExitResultMapsNonZeroToError() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.agentSpawned(paneId: paneId, agentId: "codex", now: Date())

    let t = model.applyExitResult(paneId: paneId, exitCode: 1, now: Date())

    #expect(t?.new == .error)
    #expect(model.agentStatus[paneId] == .error)
}

@Test func applyExitResultReturnsNilForUntrackedPane() {
    let model = AgentActivityModel()
    #expect(model.applyExitResult(paneId: UUID(), exitCode: 0, now: Date()) == nil)
}

@Test func applyExitResultReturnsNilAfterPaneClosed() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.agentSpawned(paneId: paneId, agentId: "claude", now: Date())
    model.paneClosed(paneId: paneId)

    #expect(model.applyExitResult(paneId: paneId, exitCode: 0, now: Date()) == nil)
}

// MARK: - Layer B: title change — first-time identification

@Test func handleTitleChangeIdentifiesClaudeFromGlyph() {
    let model = AgentActivityModel()
    let paneId = UUID()

    let t = model.handleTitleChange(paneId: paneId, title: "✳ Fix login bug", now: Date())

    #expect(t != nil)
    #expect(t?.old == nil)
    #expect(t?.new == .needsInput)
    #expect(model.paneAgents[paneId] == "claude")
    #expect(model.titleOwnedPanes.contains(paneId))
}

@Test func handleTitleChangeIdentifiesOpenCodeFromName() {
    let model = AgentActivityModel()
    let paneId = UUID()

    let t = model.handleTitleChange(paneId: paneId, title: "opencode thinking about PR", now: Date())

    #expect(t != nil)
    #expect(t?.new == .running)
    #expect(model.paneAgents[paneId] == "opencode")
    #expect(model.titleOwnedPanes.contains(paneId))
}

@Test func handleTitleChangeReturnsNilWhenNoIdentityFound() {
    let model = AgentActivityModel()
    #expect(model.handleTitleChange(paneId: UUID(), title: "zsh", now: Date()) == nil)
}

// MARK: - Layer B: title change — known pane

@Test func handleTitleChangeUpdatesStatusForKnownAgent() {
    let model = AgentActivityModel()
    let paneId = UUID()
    let spawnTime = Date()
    model.agentSpawned(paneId: paneId, agentId: "claude", now: spawnTime)

    // Title change more than 1.5s after spawn — debounce allows it.
    let t = model.handleTitleChange(paneId: paneId, title: "✳ waiting for input", now: spawnTime.addingTimeInterval(2.0))

    #expect(t?.new == .needsInput)
    #expect(model.agentStatus[paneId] == .needsInput)
}

@Test func handleTitleChangeReturnsNilWhenTitleDoesNotMatch() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.agentSpawned(paneId: paneId, agentId: "claude", now: Date())

    let t = model.handleTitleChange(paneId: paneId, title: "zsh", now: Date())

    // Spawn-owned panes survive a transient nil classification.
    #expect(t == nil)
    #expect(model.agentStatus[paneId] == .running)
    #expect(model.paneAgents[paneId] == "claude")
}

@Test func handleTitleChangeClearsTitleOwnedPaneOnUnmatchedTitle() {
    let model = AgentActivityModel()
    let paneId = UUID()
    // Title-identify first (makes it title-owned).
    model.handleTitleChange(paneId: paneId, title: "opencode ready", now: Date())
    #expect(model.paneAgents[paneId] == "opencode")
    #expect(model.titleOwnedPanes.contains(paneId))

    let t = model.handleTitleChange(paneId: paneId, title: "zsh", now: Date())

    #expect(t == nil)
    #expect(model.agentStatus[paneId] == nil)
    #expect(model.paneAgents[paneId] == nil)
    #expect(model.titleOwnedPanes.isEmpty)
}

// MARK: - Layer B: debounce

@Test func handleTitleChangeSkipsWhenRecentHookIsAuthoritative() {
    let model = AgentActivityModel()
    let paneId = UUID()
    let now = Date()
    model.agentSpawned(paneId: paneId, agentId: "claude", now: now)

    // A title change within the debounce window (1.5s) should be skipped.
    let t = model.handleTitleChange(paneId: paneId, title: "✳ idle", now: now.addingTimeInterval(0.5))

    #expect(t == nil)
    #expect(model.agentStatus[paneId] == .running)
}

@Test func handleTitleChangeAppliesAfterDebounceWindow() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.agentSpawned(paneId: paneId, agentId: "claude", now: Date())

    let t = model.handleTitleChange(paneId: paneId, title: "✳ idle", now: Date().addingTimeInterval(2.0))

    #expect(t?.new == .needsInput)
    #expect(model.agentStatus[paneId] == .needsInput)
}

// MARK: - Layer C: content signal

@Test func applyContentSignalOverridesStaleStatus() {
    let model = AgentActivityModel()
    let paneId = UUID()
    let now = Date()
    model.agentSpawned(paneId: paneId, agentId: "claude", now: now)

    // A hook fired 'running' 200ms ago — well inside Layer B's 1.5s debounce
    // window — but a genuine content match still applies (locked override
    // behavior: Layer C is not gated by shouldApplyTitleSignal).
    let t = model.applyContentSignal(paneId: paneId, status: .needsInput, now: now.addingTimeInterval(0.2))

    #expect(t?.old == .running)
    #expect(t?.new == .needsInput)
    #expect(model.agentStatus[paneId] == .needsInput)
}

@Test func applyContentSignalNoOpsForUnregisteredPane() {
    let model = AgentActivityModel()
    #expect(model.applyContentSignal(paneId: UUID(), status: .needsInput, now: Date()) == nil)
}

@Test func applyContentSignalNoOpsWhenStatusUnchanged() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.agentSpawned(paneId: paneId, agentId: "claude", now: Date())

    let t = model.applyContentSignal(paneId: paneId, status: .running, now: Date())

    #expect(t == nil)
}

@Test func applyContentSignalUpdatesLastHookUpdateAt() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.agentSpawned(paneId: paneId, agentId: "claude", now: Date())
    let signalTime = Date().addingTimeInterval(1)

    model.applyContentSignal(paneId: paneId, status: .needsInput, now: signalTime)

    #expect(model.lastHookUpdateAt[paneId] == signalTime)
}

@Test func subsequentHookNotifyStillOverridesContentSignal() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.agentSpawned(paneId: paneId, agentId: "claude", now: Date())
    model.applyContentSignal(paneId: paneId, status: .needsInput, now: Date().addingTimeInterval(1))

    // Layer A remains ultimate ground truth: a real hook always wins.
    let t = model.notify(paneId: paneId, status: .running, now: Date().addingTimeInterval(2))

    #expect(t.new == .running)
    #expect(model.agentStatus[paneId] == .running)
}

// MARK: - Pane closed

@Test func paneClosedClearsAllState() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.agentSpawned(paneId: paneId, agentId: "claude", now: Date())
    model.handleTitleChange(paneId: UUID(), title: "opencode done", now: Date()) // another pane
    #expect(!model.agentStatus.isEmpty)
    #expect(!model.paneAgents.isEmpty)
    #expect(!model.lastHookUpdateAt.isEmpty)

    model.paneClosed(paneId: paneId)

    #expect(model.agentStatus[paneId] == nil)
    #expect(model.paneAgents[paneId] == nil)
    #expect(model.lastHookUpdateAt[paneId] == nil)
}

@Test func paneClosedIsIdempotent() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.paneClosed(paneId: paneId)
    model.paneClosed(paneId: paneId)
    #expect(model.agentStatus.isEmpty)
    #expect(model.paneAgents.isEmpty)
    #expect(model.lastHookUpdateAt.isEmpty)
    #expect(model.titleOwnedPanes.isEmpty)
}

// MARK: - Queries: statusForWorktree

@Test func statusForWorktreeReturnsHighestPriority() {
    let model = AgentActivityModel()
    let p1 = UUID(); let p2 = UUID(); let p3 = UUID()
    model.agentSpawned(paneId: p1, agentId: "claude", now: Date())        // running
    model.agentSpawned(paneId: p2, agentId: "codex", now: Date())         // running
    model.notify(paneId: p3, status: .needsInput, now: Date())

    #expect(model.statusForWorktree(paneIds: [p1, p2, p3]) == .needsInput)
}

@Test func statusForWorktreeReturnsNilForEmptyCollection() {
    let model = AgentActivityModel()
    #expect(model.statusForWorktree(paneIds: []) == nil)
}

@Test func statusForWorktreeReturnsNilForUntrackedPanes() {
    let model = AgentActivityModel()
    #expect(model.statusForWorktree(paneIds: [UUID(), UUID()]) == nil)
}

@Test func statusForWorktreeErrorOverridesAll() {
    let model = AgentActivityModel()
    let p1 = UUID(); let p2 = UUID()
    model.notify(paneId: p1, status: .error, now: Date())
    model.notify(paneId: p2, status: .running, now: Date())
    #expect(model.statusForWorktree(paneIds: [p1, p2]) == .error)
}

// MARK: - Queries: agentIdForWorktree

@Test func agentIdForWorktreeReturnsAgentWithHighestPriorityStatus() {
    let model = AgentActivityModel()
    let p1 = UUID(); let p2 = UUID()
    model.agentSpawned(paneId: p1, agentId: "claude", now: Date())
    model.agentSpawned(paneId: p2, agentId: "codex", now: Date())

    // Both running; claude's pane appears first in priority scan.
    #expect(model.agentIdForWorktree(paneIds: [p1, p2]) == "claude")
}

@Test func agentIdForWorktreeReturnsNilForEmptyCollection() {
    let model = AgentActivityModel()
    #expect(model.agentIdForWorktree(paneIds: []) == nil)
}

@Test func agentIdForWorktreeReturnsFirstAgentWhenNoStatusMatch() {
    let model = AgentActivityModel()
    let p1 = UUID()
    model.paneAgents[p1] = "opencode"
    #expect(model.agentIdForWorktree(paneIds: [p1]) == "opencode")
}

// MARK: - Queries: runningAgentIds

@Test func runningAgentIdsReturnsOnlyRunningAgents() {
    let model = AgentActivityModel()
    let p1 = UUID(); let p2 = UUID(); let p3 = UUID()
    model.agentSpawned(paneId: p1, agentId: "claude", now: Date())    // running
    model.agentSpawned(paneId: p2, agentId: "codex", now: Date())     // running
    model.notify(paneId: p3, status: .done, now: Date())              // done
    let catalog = ["claude", "codex", "opencode"]

    let result = model.runningAgentIds(paneIds: [p1, p2, p3], catalogIds: catalog)

    #expect(result == ["claude", "codex"])
}

@Test func runningAgentIdsFollowsCatalogOrder() {
    let model = AgentActivityModel()
    let p1 = UUID(); let p2 = UUID()
    model.agentSpawned(paneId: p1, agentId: "codex", now: Date())
    model.agentSpawned(paneId: p2, agentId: "claude", now: Date())
    let catalog = ["claude", "codex", "opencode", "pi", "omp"]

    let result = model.runningAgentIds(paneIds: [p1, p2], catalogIds: catalog)

    #expect(result == ["claude", "codex"])
}

@Test func runningAgentIdsReturnsEmptyForNoRunningAgents() {
    let model = AgentActivityModel()
    #expect(model.runningAgentIds(paneIds: [], catalogIds: ["claude", "codex"]).isEmpty)
}

// MARK: - Notification payload

@Test func buildPayloadConstructsTitleAndBody() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.agentSpawned(paneId: paneId, agentId: "claude", now: Date())

    let payload = model.buildPayload(
        paneId: paneId, status: .done,
        agentDisplayName: "Claude Code",
        worktreeId: UUID(), worktreeBranch: "feature/login",
        projectName: "MyApp", worktreeComment: nil
    )

    #expect(payload?.title == "Claude Code — finished")
    #expect(payload?.body == "feature/login · MyApp")
}

@Test func buildPayloadIncludesComment() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.agentSpawned(paneId: paneId, agentId: "codex", now: Date())

    let payload = model.buildPayload(
        paneId: paneId, status: .needsInput,
        agentDisplayName: "Codex",
        worktreeId: UUID(), worktreeBranch: "fix/bug",
        projectName: nil, worktreeComment: "  WIP  "
    )

    #expect(payload?.title == "Codex — needs input")
    #expect(payload?.body == "fix/bug  ·  WIP")
}

@Test func buildPayloadReturnsNilWhenPaneIsNotTracked() {
    let model = AgentActivityModel()
    #expect(model.buildPayload(
        paneId: UUID(), status: .done,
        agentDisplayName: "Claude Code",
        worktreeId: UUID(), worktreeBranch: "main",
        projectName: nil, worktreeComment: nil
    ) == nil)
}

@Test func buildPayloadReturnsNilAfterPaneClosed() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.agentSpawned(paneId: paneId, agentId: "claude", now: Date())
    model.paneClosed(paneId: paneId)

    #expect(model.buildPayload(
        paneId: paneId, status: .done,
        agentDisplayName: "Claude Code",
        worktreeId: UUID(), worktreeBranch: "main",
        projectName: nil, worktreeComment: nil
    ) == nil)
}

// MARK: - notifyTransition semantics (edge cases)

@Test func notifyRespectsTimestampOverwrite() {
    let model = AgentActivityModel()
    let paneId = UUID()
    let t1 = Date()
    let t2 = t1.addingTimeInterval(10)

    model.notify(paneId: paneId, status: .running, now: t1)
    model.notify(paneId: paneId, status: .needsInput, now: t2)

    #expect(model.lastHookUpdateAt[paneId] == t2)
}
