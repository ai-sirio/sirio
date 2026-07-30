import Testing
@testable import TillerCore

@Suite struct WorkspaceLayoutDeltaTests {
    @Test func onlyOwnershipMutationsAreStructural() {
        #expect(WorkspaceLayoutDelta.isStructuralCommand(
            .insertTab(WorkspaceTab(id: WorkspaceTabID(), title: "new", titleIsAutoNamed: true,
                                    content: .terminal(TerminalContentID())),
                       into: PaneGroupID(), index: nil, activate: true)))
        #expect(WorkspaceLayoutDelta.isStructuralCommand(
            .splitGroup(anchor: PaneGroupID(), placement: .right,
                        newGroup: PaneGroupID(), newSplit: SplitID(),
                        content: .existingTab(WorkspaceTabID()))))
        #expect(WorkspaceLayoutDelta.isStructuralCommand(
            .moveTab(WorkspaceTabID(), to: .group(PaneGroupID(), index: 0))))
        #expect(WorkspaceLayoutDelta.isStructuralCommand(.closeTab(WorkspaceTabID())))

        #expect(!WorkspaceLayoutDelta.isStructuralCommand(.activateTab(WorkspaceTabID())))
        #expect(!WorkspaceLayoutDelta.isStructuralCommand(.activateGroup(PaneGroupID())))
        #expect(!WorkspaceLayoutDelta.isStructuralCommand(
            .setPreferredFraction(SplitID(), 0.4)))
        #expect(!WorkspaceLayoutDelta.isStructuralCommand(
            .updateViewState(WorkspaceTabID(), .empty)))
        #expect(!WorkspaceLayoutDelta.isStructuralCommand(
            .renameTab(WorkspaceTabID(), title: "renamed", isAutoNamed: false)))
    }

    @Test func transitionIsSendableAndCopiesAsAValue() {
        func acceptsSendable<T: Sendable>(_ type: T.Type) -> Bool {
            true
        }

        #expect(acceptsSendable(WorkspaceLayoutTransition.self))

        let transition = WorkspaceLayoutTransition(
            layout: .empty(), delta: WorkspaceLayoutDelta(), focusIntent: .none)
        let copiedTransition = transition
        #expect(copiedTransition == transition)
        #expect(Mirror(reflecting: transition).displayStyle == .struct)
    }
}
