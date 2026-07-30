import Foundation
import Testing
import TillerCore
import TillerWorkspace

@Suite @MainActor
struct WorkspaceFocusTests {
    @Test
    func eachPaneRestoresItsOwnRememberedOperationalFocus() {
        let first = PaneGroupID()
        let second = PaneGroupID()
        let firstTab = WorkspaceTabID()
        let coordinator = WorkspaceFocusCoordinator()

        coordinator.rememberOperationalFocus(.terminalFirstResponder, in: first)
        coordinator.rememberOperationalFocus(.composer, in: second)
        coordinator.rememberAccessibilityFocus(.tab(firstTab), in: first)

        let secondRestore = coordinator.focusPane(second, contentKind: .chat)
        let firstRestore = coordinator.focusPane(first, contentKind: .terminal)

        #expect(secondRestore.operational == .composer)
        #expect(firstRestore.operational == .terminalFirstResponder)
        #expect(firstRestore.accessibility == .tab(firstTab))
        #expect(coordinator.activeGroupID == first)
    }

    @Test
    func voiceOverActivationOfATabLeavesVoiceOverFocusOnTheTab() {
        let group = PaneGroupID()
        let tab = WorkspaceTabID()
        let coordinator = WorkspaceFocusCoordinator()

        let activation = coordinator.activateTabFromVoiceOver(tab, in: group)

        #expect(activation.activeGroupID == group)
        #expect(activation.selectedTabID == tab)
        #expect(activation.accessibilityFocus == .tab(tab))
        #expect(coordinator.accessibilityFocus == .tab(tab))
    }

    @Test
    func withoutMemoryTheContentKindDefaultIsUsed() {
        let coordinator = WorkspaceFocusCoordinator()

        #expect(coordinator.focusPane(PaneGroupID(), contentKind: .terminal).operational
            == .terminalFirstResponder)
        #expect(coordinator.focusPane(PaneGroupID(), contentKind: .chat).operational
            == .composer)
        #expect(coordinator.focusPane(PaneGroupID(), contentKind: .document).operational
            == .editor)
    }

    @Test
    func focusFulfillmentFailureIsReportedAndRetriedOnceAfterAttach() {
        let group = PaneGroupID()
        let coordinator = WorkspaceFocusCoordinator()
        var attempts = 0
        _ = coordinator.focusPane(group, contentKind: .terminal)

        let firstAttempt = coordinator.requestFocus(
            .terminalFirstResponder, in: group
        ) {
            attempts += 1
            return false
        }
        let retry = coordinator.hostAttached()

        #expect(firstAttempt == .pendingRetry)
        #expect(retry == .failed)
        #expect(attempts == 2)
        #expect(coordinator.activeGroupID == group)
        #expect(coordinator.lastFailureAnnouncement == "Focus could not be fulfilled.")
    }

    @Test
    func fullKeyboardAccessReachesEveryPaneControlWithoutAPointer() {
        let firstTab = makeTab(title: "Terminal")
        let secondTab = makeTab(title: "Codex")
        let first = PaneGroup(id: PaneGroupID(), tabs: [firstTab], activeTabID: firstTab.id)
        let second = PaneGroup(id: PaneGroupID(), tabs: [secondTab], activeTabID: secondTab.id)
        let layout = makeLayout(first: first, second: second)

        let controls = WorkspaceFocusCoordinator.keyboardControls(for: layout)

        #expect(controls.allSatisfy { !$0.requiresPointer })
        #expect(controls.contains(.pane(first.id)))
        #expect(controls.contains(.tab(firstTab.id)))
        #expect(controls.contains(.tabAction(secondTab.id, .moveTabTo)))
        #expect(controls.contains(.paneAction(second.id, .splitDownWith)))
        #expect(controls.contains(.dividerAction(.increment)))
        #expect(controls.contains(.dividerAction(.decrement)))
    }
}

private func makeTab(title: String) -> WorkspaceTab {
    WorkspaceTab(
        id: WorkspaceTabID(), title: title, titleIsAutoNamed: false,
        content: .chat(ChatContentID(UUID().uuidString))
    )
}

private func makeLayout(first: PaneGroup, second: PaneGroup) -> WorkspaceLayout {
    let root = LayoutNode.split(
        id: SplitID(), axis: .horizontal, fraction: 0.5,
        first: .group(first.id), second: .group(second.id)
    )
    guard case .success(let layout) = WorkspaceLayout.make(
        root: root,
        groups: [first.id: first, second.id: second],
        activeGroupID: first.id
    ) else {
        preconditionFailure("test fixture must be a valid workspace layout")
    }
    return layout
}
