import AppKit
import Foundation
import Testing
import TillerControl
import TillerCore
import TillerTerminal
@testable import Tiller

@Suite(.serialized)
@MainActor
struct AppModelControlTests {
    @Test func renameTabDisablesAutoNaming() {
        let model = makeModel()
        let worktree = makeWorktree(path: "/tmp/rename-tab")
        let tab = LegacyWorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: UUID()))
        model.worktrees = [worktree.projectId: [worktree]]
        model.workspaceCoordinator.setLegacyTabs([tab], for: worktree.id)

        #expect(tab.titleIsAutoNamed == true)
        model.renameTab(tab.id, in: worktree.id, to: "My custom name")

        let updated = model.workspaceTabs(for: worktree.id).first { $0.id == tab.id }!
        #expect(updated.title == "My custom name")
        #expect(updated.titleIsAutoNamed == false)
    }

    @Test func applyAutoTitleLeavesProvenanceUntouched() {
        let model = makeModel()
        let worktree = makeWorktree(path: "/tmp/apply-auto-title")
        let tab = LegacyWorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: UUID()))
        model.worktrees = [worktree.projectId: [worktree]]
        model.workspaceCoordinator.setLegacyTabs([tab], for: worktree.id)

        model.applyAutoTitle(tab.id, in: worktree.id, title: "Fix login bug")

        let updated = model.workspaceTabs(for: worktree.id).first { $0.id == tab.id }!
        #expect(updated.title == "Fix login bug")
        #expect(updated.titleIsAutoNamed == true)
    }

    @Test func createReturnsOnlyAfterRegistrationWithoutChangingSelection() async {
        let registry = PaneRegistry()
        let createdPaneId = UUID()
        let activation = ActivationCounter()
        let model = makeModel(
            registry: registry,
            paneId: createdPaneId,
            timeoutMs: 1_000,
            activation: activation
        )
        let target = makeWorktree(path: "/tmp/target")
        let selected = makeWorktree(path: "/tmp/selected")
        model.worktrees = [target.projectId: [target], selected.projectId: [selected]]
        await model.workspaceCoordinator.restore(worktree: target)
        await model.workspaceCoordinator.restore(worktree: selected)
        let targetPaneId = UUID(), selectedPaneId = UUID()
        let targetTab = LegacyWorkspaceTab(
            id: UUID(), title: "Target active", tree: .leaf(id: targetPaneId)
        )
        let selectedTab = LegacyWorkspaceTab(
            id: UUID(), title: "Selected", tree: .leaf(id: selectedPaneId)
        )
        model.workspaceCoordinator.setLegacyTabs([targetTab], for: target.id)
        model.workspaceCoordinator.setLegacyTabs([selectedTab], for: selected.id)
        model.workspaceCoordinator.setLegacyActiveTabID(targetTab.id, for: target.id)
        model.workspaceCoordinator.setLegacyActiveTabID(selectedTab.id, for: selected.id)
        model.selectedWorktree = selected
        let priorOpenIds = model.openWorktreeIds
        let response = ResponseBox()

        let requestTask = Task {
            let value = await model.handleControl(request(
                "panel.create", ["worktree": target.path, "cmd": "printf ready"]
            ))
            await response.set(value)
            return value
        }
        await poll {
            model.workspaceTabs(for: target.id).flatMap(\.leafIds).contains(createdPaneId)
        }

        #expect(await response.value == nil)
        #expect(model.selectedWorktree?.id == selected.id)
        #expect(model.workspaceActiveTabID(for: selected.id) == selectedTab.id)
        #expect(model.workspaceActiveTabID(for: target.id) == targetTab.id)
        #expect(model.workspaceTabs(for: target.id).flatMap(\.leafIds).contains(createdPaneId))
        await registry.register(
            paneId: createdPaneId,
            pty: PtyProcess { _ in },
            scrollback: ScrollbackBuffer()
        )

        let result = await requestTask.value
        #expect(result.ok)
        #expect(result.result == ["id": createdPaneId.uuidString])
        #expect(model.selectedWorktree?.id == selected.id)
        #expect(model.workspaceActiveTabID(for: selected.id) == selectedTab.id)
        #expect(model.workspaceActiveTabID(for: target.id) == targetTab.id)
        #expect(Set(model.openWorktreeIds) == Set(priorOpenIds + [target.id]))
        #expect(activation.count == 0)
    }

    @Test func createTimeoutRollsBackTabCommandMountAndLateRegistration() async {
        let registry = PaneRegistry()
        let createdPaneId = UUID()
        let activation = ActivationCounter()
        let model = makeModel(
            registry: registry,
            paneId: createdPaneId,
            timeoutMs: 20,
            activation: activation
        )
        let target = makeWorktree(path: "/tmp/target-timeout")
        model.worktrees = [target.projectId: [target]]
        await model.workspaceCoordinator.restore(worktree: target)

        let result = await model.handleControl(request(
            "panel.create", ["worktree": target.id.uuidString, "cmd": "sleep 10"]
        ))

        #expect(result.ok == false)
        #expect(model.workspaceTabs(for: target.id).isEmpty)
        #expect(model.paneCommands[createdPaneId] == nil)
        #expect(model.openWorktreeIds.contains(target.id) == false)
        await registry.register(
            paneId: createdPaneId,
            pty: PtyProcess { _ in },
            scrollback: ScrollbackBuffer()
        )
        #expect(await registry.isRegistered(paneId: createdPaneId) == false)
        #expect(activation.count == 0)
    }

    @Test func splitTimeoutRollsBackOnlyNewLeafAndCommand() async {
        let registry = PaneRegistry()
        let createdPaneId = UUID()
        let model = makeModel(
            registry: registry,
            paneId: createdPaneId,
            timeoutMs: 20,
            activation: ActivationCounter()
        )
        let worktree = makeWorktree(path: "/tmp/split-timeout")
        let sourcePaneId = UUID()
        let tab = LegacyWorkspaceTab(id: UUID(), title: "Source", tree: .leaf(id: sourcePaneId))
        model.worktrees = [worktree.projectId: [worktree]]
        model.workspaceCoordinator.setLegacyTabs([tab], for: worktree.id)
        model.workspaceCoordinator.setLegacyActiveTabID(tab.id, for: worktree.id)
        model.openWorktreeIds = [worktree.id]

        let result = await model.handleControl(request(
            "panel.split",
            ["from": sourcePaneId.uuidString, "direction": "left", "cmd": "false"]
        ))

        #expect(result.ok == false)
        #expect(model.workspaceTabs(for: worktree.id).first?.terminalTree == .leaf(id: sourcePaneId))
        #expect(model.paneCommands[createdPaneId] == nil)
        await registry.register(
            paneId: createdPaneId,
            pty: PtyProcess { _ in },
            scrollback: ScrollbackBuffer()
        )
        #expect(await registry.isRegistered(paneId: createdPaneId) == false)
    }

    @Test func listUsesExplicitWorktreeSelector() async {
        let model = makeModel()
        let requested = makeWorktree(path: "/tmp/list-requested")
        let selected = makeWorktree(path: "/tmp/list-selected")
        let requestedPaneId = UUID(), selectedPaneId = UUID()
        let requestedTab = LegacyWorkspaceTab(
            id: UUID(), title: "Requested", tree: .leaf(id: requestedPaneId)
        )
        let selectedTab = LegacyWorkspaceTab(
            id: UUID(), title: "Selected", tree: .leaf(id: selectedPaneId)
        )
        model.worktrees = [requested.projectId: [requested], selected.projectId: [selected]]
        model.workspaceCoordinator.setLegacyTabs([requestedTab], for: requested.id)
        model.workspaceCoordinator.setLegacyTabs([selectedTab], for: selected.id)
        model.workspaceCoordinator.setLegacyActiveTabID(requestedTab.id, for: requested.id)
        model.workspaceCoordinator.setLegacyActiveTabID(selectedTab.id, for: selected.id)
        model.selectedWorktree = selected

        let result = await model.handleControl(request(
            "panel.list", ["worktree": requested.path]
        ))
        let rows = result.result?["panels"].flatMap(ControlRows.decode)

        #expect(result.ok)
        #expect(rows?.map { $0["id"] } == [requestedPaneId.uuidString])
        #expect(model.selectedWorktree?.id == selected.id)
    }

    @Test func closeRejectsUnknownPaneAndRemovesKnownPaneBeforeReturning() async {
        let registry = PaneRegistry()
        let model = makeModel(registry: registry)
        let worktree = makeWorktree(path: "/tmp/close")
        let paneId = UUID()
        let tab = LegacyWorkspaceTab(id: UUID(), title: "Close", tree: .leaf(id: paneId))
        model.worktrees = [worktree.projectId: [worktree]]
        model.workspaceCoordinator.setLegacyTabs([tab], for: worktree.id)
        model.workspaceCoordinator.setLegacyActiveTabID(tab.id, for: worktree.id)

        let unknown = await model.handleControl(request(
            "panel.close", ["id": UUID().uuidString]
        ))
        #expect(unknown.ok == false)

        await registry.register(
            paneId: paneId,
            pty: PtyProcess { _ in },
            scrollback: ScrollbackBuffer()
        )
        let known = await model.handleControl(request(
            "panel.close", ["id": paneId.uuidString]
        ))

        #expect(known.ok)
        #expect(model.workspaceTabContaining(paneId: paneId) == nil)
        #expect(await registry.isRegistered(paneId: paneId) == false)
        #expect(await registry.write(paneId: paneId, data: Data("stale".utf8)) == false)
    }

    @Test func capabilitiesAdvertiseCanonicalPanelsWithoutLegacyMethods() async {
        let model = makeModel()
        let result = await model.handleControl(request("system.capabilities"))
        let rows = result.result?["methods"].flatMap(ControlRows.decode) ?? []
        let methods = Set(rows.compactMap { $0["method"] })
        let canonical: Set<String> = [
            "panel.create", "panel.split", "panel.list", "panel.write", "panel.key",
            "panel.read", "panel.wait", "panel.focus", "panel.close",
        ]
        let legacy: Set<String> = [
            "surface.list", "pane.surfaces", "surface.focus", "surface.split",
            "surface.send_text", "surface.send_key", "surface.close",
        ]

        #expect(canonical.isSubset(of: methods))
        #expect(methods.isDisjoint(with: legacy))
    }

    @Test func activationCallbackIsInvokedOnlyByFocus() async {
        let activation = ActivationCounter()
        let model = makeModel(activation: activation)
        let owning = makeWorktree(path: "/tmp/focus-owning")
        let selected = makeWorktree(path: "/tmp/focus-selected")
        let paneId = UUID()
        let owningTab = LegacyWorkspaceTab(id: UUID(), title: "Owning", tree: .leaf(id: paneId))
        model.worktrees = [owning.projectId: [owning], selected.projectId: [selected]]
        model.workspaceCoordinator.setLegacyTabs([owningTab], for: owning.id)
        model.selectedWorktree = selected

        let controller = NSViewController()
        let focusView = AppModelFocusableView()
        controller.view = focusView
        model.workspacePaneCache(for: owning.id).controllers[paneId] = controller
        let host = NSView(frame: NSRect(x: 0, y: 0, width: 100, height: 100))
        host.addSubview(focusView)
        let window = NSWindow(
            contentRect: host.bounds,
            styleMask: .borderless,
            backing: .buffered,
            defer: false
        )
        window.contentView = host

        let listed = await model.handleControl(request(
            "panel.list", ["worktree": owning.id.uuidString]
        ))
        #expect(listed.ok)
        #expect(activation.count == 0)

        let focused = await model.handleControl(request(
            "panel.focus", ["id": paneId.uuidString]
        ))
        #expect(focused.ok)
        #expect(activation.count == 1)
        #expect(model.selectedWorktree?.id == owning.id)
        #expect(model.workspaceActiveTabID(for: owning.id) == owningTab.id)
        #expect(window.firstResponder === focusView)
    }

    @Test func focusFailsWhenPaneNeverAttaches() async {
        let activation = ActivationCounter()
        let model = makeModel(timeoutMs: 20, activation: activation)
        let worktree = makeWorktree(path: "/tmp/focus-unattached")
        let paneId = UUID()
        let tab = LegacyWorkspaceTab(id: UUID(), title: "Hidden", tree: .leaf(id: paneId))
        model.worktrees = [worktree.projectId: [worktree]]
        model.workspaceCoordinator.setLegacyTabs([tab], for: worktree.id)

        let result = await model.handleControl(request(
            "panel.focus", ["id": paneId.uuidString]
        ))

        #expect(result.ok == false)
        #expect(result.error == "panel could not be focused")
        #expect(model.selectedWorktree?.id == worktree.id)
        #expect(model.workspaceActiveTabID(for: worktree.id) == tab.id)
        #expect(activation.count == 1)
    }

    @Test func focusRejectsUnknownInvalidAndMissingPanelIds() async {
        let model = makeModel()
        let unknown = await model.handleControl(request(
            "panel.focus", ["id": UUID().uuidString]
        ))
        let invalid = await model.handleControl(request(
            "panel.focus", ["id": "not-a-uuid"]
        ))
        let missing = await model.handleControl(request("panel.focus"))

        #expect(unknown.error == "unknown panel")
        #expect(invalid.error == "unknown panel")
        #expect(missing.error == "unknown panel")
    }

    /// Asserts the cancellation SEMANTICS, not a stopwatch. The cancelled and
    /// timed-out paths used to return the same error, leaving elapsed time as
    /// the only discriminator — so this test was really a race against AppKit
    /// activation cost, and it lost at both a 250ms and a 500ms bound. Now the
    /// cancelled path has its own error, which is deterministic. The timing
    /// bound stays only as a generous guard against an outright hang: it must
    /// be well under the 3s timeout to prove the wait was short-circuited.
    @Test func cancelledFocusReturnsPromptly() async {
        let model = makeModel(timeoutMs: 3_000)
        let worktree = makeWorktree(path: "/tmp/focus-cancelled")
        let paneId = UUID()
        let tab = LegacyWorkspaceTab(id: UUID(), title: "Hidden", tree: .leaf(id: paneId))
        model.worktrees = [worktree.projectId: [worktree]]
        model.workspaceCoordinator.setLegacyTabs([tab], for: worktree.id)

        let task = Task {
            await model.handleControl(request("panel.focus", ["id": paneId.uuidString]))
        }
        await Task.yield()
        let clock = ContinuousClock()
        let start = clock.now
        task.cancel()
        let result = await task.value
        let elapsed = start.duration(to: clock.now)

        #expect(result.error == "panel focus cancelled")
        #expect(elapsed < .milliseconds(2_000))
    }

    @Test func createFailsAndRollsBackWhenPersistenceFails() async {
        let registry = PaneRegistry()
        let paneId = UUID()
        let model = makeModel(
            registry: registry,
            paneId: paneId,
            controlTabPersister: { _, _, _ in throw ControlPersistenceFailure.failed }
        )
        let worktree = makeWorktree(path: "/tmp/persist-failure")
        model.worktrees = [worktree.projectId: [worktree]]
        await model.workspaceCoordinator.restore(worktree: worktree)

        let task = Task { await model.handleControl(self.request(
            "panel.create", ["worktree": worktree.id.uuidString]
        )) }
        try? await Task.sleep(for: .milliseconds(20))
        await registry.register(
            paneId: paneId, pty: PtyProcess { _ in }, scrollback: ScrollbackBuffer()
        )
        let result = await task.value

        #expect(result.ok == false)
        #expect(model.workspaceTabContaining(paneId: paneId) == nil)
        #expect(model.openWorktreeIds.contains(worktree.id) == false)
        #expect(await registry.isRegistered(paneId: paneId) == false)
    }

    @Test func createResponseWaitsForOrderedPersistence() async {
        let registry = PaneRegistry()
        let paneId = UUID()
        let gate = PersistenceGate()
        let response = ResponseBox()
        let model = makeModel(
            registry: registry,
            paneId: paneId,
            timeoutMs: 1_000,
            controlTabPersister: { _, _, _ in await gate.wait() }
        )
        let worktree = makeWorktree(path: "/tmp/persist-order")
        model.worktrees = [worktree.projectId: [worktree]]
        await model.workspaceCoordinator.restore(worktree: worktree)

        let task = Task {
            let value = await model.handleControl(self.request(
                "panel.create", ["worktree": worktree.id.uuidString]
            ))
            await response.set(value)
            return value
        }
        try? await Task.sleep(for: .milliseconds(20))
        await registry.register(
            paneId: paneId, pty: PtyProcess { _ in }, scrollback: ScrollbackBuffer()
        )
        await poll { await gate.started }

        #expect(await gate.started)
        #expect(await response.value == nil)
        await gate.release()
        #expect(await task.value.ok)
    }

    @Test func concurrentCreateTimeoutNeverPersistsPendingPane() async {
        let registry = PaneRegistry()
        let successfulPaneId = UUID()
        let failedPaneId = UUID()
        var generatedPaneIds = [successfulPaneId, failedPaneId]
        let recorder = ControlPersistenceRecorder()
        let workspaceCoordinator = WorkspaceCoordinator(
            persistence: ControlWorkspacePersistence(),
            registry: WorkspaceContentRegistry(),
            adapters: [.terminal: TerminalContentAdapter()]
        )
        let model = AppModel(
            paneRegistry: registry,
            registrationTimeoutMs: 1_000,
            paneIdGenerator: { generatedPaneIds.removeFirst() },
            activateApplication: {},
            controlTabPersister: { _, tabs, _ in
                await recorder.record(tabs.flatMap(\.leafIds))
            },
            workspaceCoordinator: workspaceCoordinator
        )
        let worktree = makeWorktree(path: "/tmp/concurrent-persistence")
        model.worktrees = [worktree.projectId: [worktree]]
        await workspaceCoordinator.restore(worktree: worktree)

        let successfulTask = Task {
            await model.handleControl(request(
                "panel.create", ["worktree": worktree.id.uuidString]
            ))
        }
        try? await Task.sleep(for: .milliseconds(10))
        let failedTask = Task {
            await model.handleControl(request(
                "panel.create", ["worktree": worktree.id.uuidString]
            ))
        }
        try? await Task.sleep(for: .milliseconds(20))
        await registry.register(
            paneId: successfulPaneId,
            pty: PtyProcess { _ in },
            scrollback: ScrollbackBuffer()
        )

        let successful = await successfulTask.value
        let failed = await failedTask.value
        let snapshots = await recorder.snapshots

        #expect(successful.ok)
        #expect(failed.error == "panel did not register before timeout")
        #expect(snapshots.allSatisfy { !$0.contains(failedPaneId) })
    }

    private func makeModel(
        registry: PaneRegistry = PaneRegistry(),
        paneId: UUID = UUID(),
        timeoutMs: Int = 100,
        activation: ActivationCounter = ActivationCounter(),
        controlTabPersister: AppModel.ControlTabPersister? = nil
    ) -> AppModel {
        let workspaceCoordinator = WorkspaceCoordinator(
            persistence: ControlWorkspacePersistence(),
            registry: WorkspaceContentRegistry(),
            adapters: [.terminal: TerminalContentAdapter()]
        )
        return AppModel(
            paneRegistry: registry,
            registrationTimeoutMs: timeoutMs,
            paneIdGenerator: { paneId },
            activateApplication: { activation.count += 1 },
            controlTabPersister: controlTabPersister,
            workspaceCoordinator: workspaceCoordinator
        )
    }

    private func makeWorktree(path: String) -> Worktree {
        Worktree(
            id: UUID(),
            projectId: UUID(),
            branch: "main",
            path: path
        )
    }

    private func request(
        _ method: String,
        _ params: [String: String] = [:]
    ) -> ControlRequest {
        ControlRequest(id: UUID().uuidString, method: method, params: params)
    }
}

private actor ControlWorkspacePersistence: WorkspaceLayoutPersistence {
    func restore(worktreeID: UUID) async -> RestoredWorkspace {
        RestoredWorkspace(
            layout: .empty(groupID: PaneGroupID(worktreeID)),
            tabs: [:], revision: 0, diagnostics: [])
    }

    func commitStructural(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot,
                          tabs: [WorkspaceTab],
                          terminalContents: [TerminalContentRecordValue]) async throws {}

    func checkpoint(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) async {}
    func flush(worktreeID: UUID) async throws {}
    nonisolated func writeRecoverySidecar(
        worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) throws {}
    func purge(worktreeID: UUID) async throws {}
}

@MainActor
private final class ActivationCounter {
    var count = 0
}

private final class AppModelFocusableView: NSView {
    override var acceptsFirstResponder: Bool { true }
}

private actor ResponseBox {
    private(set) var value: ControlResponse?
    func set(_ value: ControlResponse) { self.value = value }
}

private enum ControlPersistenceFailure: Error { case failed }

private actor ControlPersistenceRecorder {
    private(set) var snapshots: [[UUID]] = []

    func record(_ paneIds: [UUID]) {
        snapshots.append(paneIds)
    }
}

/// Polls `condition` until it's true or `timeout` elapses, instead of
/// guessing a fixed sleep duration for async state to settle.
@MainActor
private func poll(
    timeout: Duration = .milliseconds(1_000),
    interval: Duration = .milliseconds(5),
    _ condition: () async -> Bool
) async {
    let deadline = ContinuousClock.now + timeout
    while ContinuousClock.now < deadline {
        if await condition() { return }
        try? await Task.sleep(for: interval)
    }
}

private actor PersistenceGate {
    private(set) var started = false
    private var continuation: CheckedContinuation<Void, Never>?

    func wait() async {
        started = true
        await withCheckedContinuation { continuation = $0 }
    }

    func release() {
        continuation?.resume()
        continuation = nil
    }
}
