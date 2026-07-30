import Foundation
import Testing
import TillerCore
import TillerWorkspace
@testable import Tiller

@Suite(.serialized)
@MainActor
struct WorkspaceMenuCommandsTests {
    @Test func tabCommandsScopeToTheActivePaneGroupOnly() {
        let first = WorkspaceTabID()
        let second = WorkspaceTabID()
        let otherPaneTab = WorkspaceTabID()
        let activeGroup = PaneGroupID()
        let otherGroup = PaneGroupID()
        let layout = makeLayout(
            activeGroup: activeGroup,
            groups: [
                (activeGroup, [first, second], second),
                (otherGroup, [otherPaneTab], otherPaneTab)
            ])

        let model = WorkspaceMenuModel(layout: layout)

        #expect(model.localTabIDs == [first, second])
        #expect(model.nextTabID == first)
        #expect(model.previousTabID == first)
        #expect(!model.localTabIDs.contains(otherPaneTab))
    }

    @Test func moveEarlierIsDisabledAtTheStartAndNeverWraps() {
        let first = WorkspaceTabID()
        let second = WorkspaceTabID()
        let group = PaneGroupID()
        let layout = makeLayout(activeGroup: group, groups: [(group, [first, second], first)])

        let model = WorkspaceMenuModel(layout: layout)

        #expect(!model.moveEarlier.isEnabled)
        #expect(model.moveEarlierTarget == nil)
        #expect(model.moveLater.isEnabled)
        #expect(model.moveLaterTarget == second)
    }

    @Test func moveTabToPreviewsTheDestinationWithoutMutating() {
        let sourceTab = WorkspaceTabID()
        let destinationTab = WorkspaceTabID()
        let source = PaneGroupID()
        let destination = PaneGroupID()
        let layout = makeLayout(
            activeGroup: source,
            groups: [(source, [sourceTab], sourceTab), (destination, [destinationTab], destinationTab)],
            root: .split(
                id: SplitID(), axis: .horizontal, fraction: 0.5,
                first: .group(source), second: .group(destination)))

        let preview = WorkspaceMenuMovePreview(
            layout: layout,
            tabID: sourceTab,
            destination: .group(destination, index: 1))

        #expect(preview.originalLayout == layout)
        #expect(preview.previewedLayout != nil)
        #expect(preview.originalLayout == layout)
        #expect(preview.previewedLayout?.groupContaining(tab: sourceTab) == destination)
    }

    @Test func escapeFromMoveTabToPreservesTopologyOwnershipSelectionFocus() {
        let tab = WorkspaceTabID()
        let group = PaneGroupID()
        let layout = makeLayout(activeGroup: group, groups: [(group, [tab], tab)])
        let session = WorkspaceMenuMoveSession(layout: layout)

        session.preview(tabID: tab, destination: .newPane(.right))
        session.cancel()

        #expect(session.layout == layout)
        #expect(session.isPreviewing == false)
        #expect(session.cancelledDestination == .newPane(.right))
    }

    @Test func aMoveThatCollapsesItsSourceSaysSoInItsLabel() {
        let sourceTab = WorkspaceTabID()
        let destinationTab = WorkspaceTabID()
        let source = PaneGroupID()
        let destination = PaneGroupID()
        let layout = makeLayout(
            activeGroup: source,
            groups: [(source, [sourceTab], sourceTab), (destination, [destinationTab], destinationTab)],
            root: .split(
                id: SplitID(), axis: .horizontal, fraction: 0.5,
                first: .group(source), second: .group(destination)))

        let model = WorkspaceMenuModel(layout: layout)
        let item = model.moveDestinations.first {
            if case .group(let id, _) = $0.destination { return id == destination }
            return false
        }

        #expect(item?.label.contains("collapses") == true)
    }

    @Test func unmodifiedTabIsNeverIntercepted() {
        #expect(WorkspaceMenuCommandCatalog.allShortcuts.allSatisfy { !$0.modifiers.isEmpty })
        #expect(WorkspaceMenuCommandCatalog.nextTabShortcut ==
                WorkspaceMenuShortcut(key: "tab", modifiers: [.control]))
        #expect(WorkspaceMenuCommandCatalog.previousTabShortcut ==
                WorkspaceMenuShortcut(key: "tab", modifiers: [.control, .shift]))
    }

    @Test func commandsTargetTheFocusedWorkspaceNotTheSelectedWorktree() {
        var firstActions: [WorkspaceMenuAction] = []
        var secondActions: [WorkspaceMenuAction] = []
        let first = WorkspaceMenuTarget(worktreeID: UUID(), send: { firstActions.append($0) })
        let second = WorkspaceMenuTarget(worktreeID: UUID(), send: { secondActions.append($0) })

        second.send(.nextTab)

        #expect(firstActions.isEmpty)
        #expect(secondActions == [.nextTab])
        #expect(first.worktreeID != second.worktreeID)
    }

    @Test func thereIsNoClosePaneCommand() {
        #expect(WorkspaceMenuCommandCatalog.labels.contains("Close Tab"))
        #expect(!WorkspaceMenuCommandCatalog.labels.contains("Close Pane"))
        #expect(WorkspaceMenuCommandCatalog.labels.contains("New Pane Left"))
        #expect(WorkspaceMenuCommandCatalog.labels.contains("New Pane Right"))
        #expect(WorkspaceMenuCommandCatalog.labels.contains("New Pane Above"))
        #expect(WorkspaceMenuCommandCatalog.labels.contains("New Pane Below"))
    }

    private func makeLayout(
        activeGroup: PaneGroupID,
        groups: [(PaneGroupID, [WorkspaceTabID], WorkspaceTabID?)],
        root: LayoutNode? = nil
    ) -> WorkspaceLayout {
        let panes = Dictionary(uniqueKeysWithValues: groups.map { id, tabs, active in
            (id, PaneGroup(
                id: id,
                tabs: tabs.map { WorkspaceTab(
                    id: $0, title: $0.rawValue.uuidString, titleIsAutoNamed: true,
                    content: .terminal(TerminalContentID($0.rawValue))) },
                activeTabID: active))
        })
        let layoutRoot = root ?? {
            guard groups.count > 1 else { return .group(activeGroup) }
            return .split(
                id: SplitID(), axis: .horizontal, fraction: 0.5,
                first: .group(groups[0].0), second: .group(groups[1].0))
        }()
        switch WorkspaceLayout.make(root: layoutRoot, groups: panes, activeGroupID: activeGroup) {
        case .success(let layout): return layout
        case .failure(let error):
            Issue.record("invalid workspace fixture: \(error)")
            return .empty(groupID: activeGroup)
        }
    }
}
