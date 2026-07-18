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
        let targetPaneId = UUID(), selectedPaneId = UUID()
        let targetTab = WorkspaceTab(
            id: UUID(), title: "Target active", tree: .leaf(id: targetPaneId)
        )
        let selectedTab = WorkspaceTab(
            id: UUID(), title: "Selected", tree: .leaf(id: selectedPaneId)
        )
        model.tabs[target.id] = [targetTab]
        model.tabs[selected.id] = [selectedTab]
        model.activeTabId[target.id] = targetTab.id
        model.activeTabId[selected.id] = selectedTab.id
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
        try? await Task.sleep(for: .milliseconds(30))

        #expect(await response.value == nil)
        #expect(model.selectedWorktree?.id == selected.id)
        #expect(model.activeTabId[selected.id] == selectedTab.id)
        #expect(model.activeTabId[target.id] == targetTab.id)
        #expect(model.tabs[target.id]?.flatMap(\.leafIds).contains(createdPaneId) == true)
        await registry.register(
            paneId: createdPaneId,
            pty: PtyProcess { _ in },
            scrollback: ScrollbackBuffer()
        )

        let result = await requestTask.value
        #expect(result.ok)
        #expect(result.result == ["id": createdPaneId.uuidString])
        #expect(model.selectedWorktree?.id == selected.id)
        #expect(model.activeTabId[selected.id] == selectedTab.id)
        #expect(model.activeTabId[target.id] == targetTab.id)
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

        let result = await model.handleControl(request(
            "panel.create", ["worktree": target.id.uuidString, "cmd": "sleep 10"]
        ))

        #expect(result.ok == false)
        #expect(model.tabs[target.id]?.isEmpty != false)
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
        let tab = WorkspaceTab(id: UUID(), title: "Source", tree: .leaf(id: sourcePaneId))
        model.worktrees = [worktree.projectId: [worktree]]
        model.tabs[worktree.id] = [tab]
        model.activeTabId[worktree.id] = tab.id
        model.openWorktreeIds = [worktree.id]

        let result = await model.handleControl(request(
            "panel.split",
            ["from": sourcePaneId.uuidString, "direction": "left", "cmd": "false"]
        ))

        #expect(result.ok == false)
        #expect(model.tabs[worktree.id]?.first?.terminalTree == .leaf(id: sourcePaneId))
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
        let requestedTab = WorkspaceTab(
            id: UUID(), title: "Requested", tree: .leaf(id: requestedPaneId)
        )
        let selectedTab = WorkspaceTab(
            id: UUID(), title: "Selected", tree: .leaf(id: selectedPaneId)
        )
        model.worktrees = [requested.projectId: [requested], selected.projectId: [selected]]
        model.tabs[requested.id] = [requestedTab]
        model.tabs[selected.id] = [selectedTab]
        model.activeTabId[requested.id] = requestedTab.id
        model.activeTabId[selected.id] = selectedTab.id
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
        let tab = WorkspaceTab(id: UUID(), title: "Close", tree: .leaf(id: paneId))
        model.worktrees = [worktree.projectId: [worktree]]
        model.tabs[worktree.id] = [tab]
        model.activeTabId[worktree.id] = tab.id

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
        #expect(model.tabContaining(paneId: paneId) == nil)
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
        let owningTab = WorkspaceTab(id: UUID(), title: "Owning", tree: .leaf(id: paneId))
        model.worktrees = [owning.projectId: [owning], selected.projectId: [selected]]
        model.tabs[owning.id] = [owningTab]
        model.selectedWorktree = selected

        let controller = NSViewController()
        let focusView = AppModelFocusableView()
        controller.view = focusView
        model.paneCache(for: owning.id).controllers[paneId] = controller
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
        #expect(model.activeTabId[owning.id] == owningTab.id)
        #expect(window.firstResponder === focusView)
    }

    private func makeModel(
        registry: PaneRegistry = PaneRegistry(),
        paneId: UUID = UUID(),
        timeoutMs: Int = 100,
        activation: ActivationCounter = ActivationCounter()
    ) -> AppModel {
        AppModel(
            paneRegistry: registry,
            registrationTimeoutMs: timeoutMs,
            paneIdGenerator: { paneId },
            activateApplication: { activation.count += 1 }
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
