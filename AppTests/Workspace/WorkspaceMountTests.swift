import AppKit
import Foundation
import SwiftUI
import Testing
import TillerCore
import TillerTerminal
import TillerWorkspace
@testable import Tiller

@Suite(.serialized)
@MainActor
struct WorkspaceMountTests {
    @Test func renderedContentViewReportsOnlyTheMountedSplitBranch() throws {
        let legacyHost = TitlebarAccessoryHost()
        let legacyWindow = renderContentView(
            workspaceEngineEnabled: false,
            titlebarAccessoryHost: legacyHost)

        #expect(legacyHost.updateCount > 0)
        #expect(legacyHost.renderedControlProbes.contains {
            $0.slot == .split && $0.accessibilityIdentifier == "tiller.titlebar.split.legacy"
        })
        #expect(!legacyHost.renderedControlProbes.contains {
            $0.slot == .split && $0.accessibilityIdentifier == "tiller.titlebar.split.workspace"
        })

        let workspaceHost = TitlebarAccessoryHost()
        let workspaceWindow = renderContentView(
            workspaceEngineEnabled: true,
            titlebarAccessoryHost: workspaceHost)

        #expect(workspaceHost.updateCount > 0)
        #expect(workspaceHost.renderedControlProbes.contains {
            $0.slot == .split && $0.accessibilityIdentifier == "tiller.titlebar.split.workspace"
        })
        #expect(!workspaceHost.renderedControlProbes.contains {
            $0.slot == .split && $0.accessibilityIdentifier == "tiller.titlebar.split.legacy"
        })

        let expectedSlots: Set<TitlebarControlSlot> = [.sidebar, .rightPanel, .split, .permissions]
        #expect(Set(legacyHost.renderedControlProbes.map(\.slot)).isSuperset(of: expectedSlots))
        #expect(Set(workspaceHost.renderedControlProbes.map(\.slot)).isSuperset(of: expectedSlots))

        legacyWindow.orderOut(nil)
        workspaceWindow.orderOut(nil)
    }

    @Test func withTheGateOnTheWorkspaceRendererIsMountedInsteadOfTheTabStack() {
        #expect(ContentView.renderPath(gateEnabled: true) == .workspace)
    }

    @Test func withTheGateOffTheLegacyTerminalSplitHostStillRenders() {
        #expect(ContentView.renderPath(gateEnabled: false) == .legacyTerminal)
    }

    @Test func detachingAChatTabKeepsItsControllerAlive() {
        let registry = WorkspaceContentRegistry()
        let tab = WorkspaceTab(
            id: WorkspaceTabID(), title: "Chat", titleIsAutoNamed: true,
            content: .chat(ChatContentID("session-1")))
        let host = RecordingHost(tabID: tab.id)

        registry.adopt(host, tab: tab, generation: ResourceGenerationID())
        let detached = WorkspaceMountPlan.detachedHostIDs(
            from: [tab.id], mountedTabIDs: [])

        #expect(detached == [tab.id])
        #expect(registry.host(for: tab.id) === host)
        #expect(host.releaseCount == 0)
    }

    @Test func switchingWorktreesRegeneratesNoContent() {
        let first = UUID()
        let second = UUID()
        let open = [first, second]

        let firstSelection = WorkspaceMountPlan(
            openWorktreeIDs: open, selectedWorktreeID: first)
        let secondSelection = WorkspaceMountPlan(
            openWorktreeIDs: open, selectedWorktreeID: second)

        #expect(firstSelection.mountedWorktreeIDs == open)
        #expect(secondSelection.mountedWorktreeIDs == open)
        #expect(firstSelection.mountedWorktreeIDs == secondSelection.mountedWorktreeIDs)
    }
}

@MainActor
private func renderContentView(
    workspaceEngineEnabled: Bool,
    titlebarAccessoryHost: TitlebarAccessoryHost
) -> NSWindow {
    let model = AppModel(paneRegistry: PaneRegistry(), registrationTimeoutMs: 100)
    let updater = UpdaterModel()
    let contentView = ContentView(
        model: model,
        updater: updater,
        workspaceEngineEnabled: workspaceEngineEnabled,
        titlebarAccessoryHost: titlebarAccessoryHost)
    let hostingView = NSHostingView(rootView: contentView)
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 560),
                          styleMask: [.titled, .closable, .resizable],
                          backing: .buffered,
                          defer: false)
    window.contentView = hostingView
    window.makeKeyAndOrderFront(nil)
    window.layoutIfNeeded()
    for _ in 0..<20 where titlebarAccessoryHost.updateCount == 0 {
        RunLoop.main.run(until: Date().addingTimeInterval(0.01))
    }
    window.layoutIfNeeded()
    return window
}

@MainActor
private final class RecordingHost: WorkspaceContentHost {
    let tabID: WorkspaceTabID
    let viewController = NSViewController()
    var releaseCount = 0

    init(tabID: WorkspaceTabID) { self.tabID = tabID }

    func setVisible(_ isVisible: Bool) {}
    func fulfill(_ intent: FocusIntent) -> Bool { true }
}
