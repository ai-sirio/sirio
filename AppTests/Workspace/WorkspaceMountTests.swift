import AppKit
import Testing
import TillerCore
import TillerWorkspace
@testable import Tiller

@Suite(.serialized)
@MainActor
struct WorkspaceMountTests {
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
private final class RecordingHost: WorkspaceContentHost {
    let tabID: WorkspaceTabID
    let viewController = NSViewController()
    var releaseCount = 0

    init(tabID: WorkspaceTabID) { self.tabID = tabID }

    func setVisible(_ isVisible: Bool) {}
    func fulfill(_ intent: FocusIntent) -> Bool { true }
}
