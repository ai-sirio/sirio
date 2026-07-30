import Foundation
import CoreGraphics
import Testing
import TillerCore
import TillerWorkspace
@testable import Tiller

@Suite(.serialized)
@MainActor
struct SplitContentMenuTests {
    @Test func splitRightAlwaysOpensTheMenuBeforeAnyMutation() {
        var mutations = 0
        let session = SplitContentMenuSession { _ in mutations += 1 }

        session.open()
        #expect(session.isPresented)
        #expect(mutations == 0)

        session.choose(.newTerminal(command: nil))
        #expect(!session.isPresented)
        #expect(mutations == 1)
    }

    @Test func newTerminalIsTheFirstAndDefaultItem() {
        let menu = makeMenu()

        #expect(menu.topLevelLabels.first == "New Terminal")
        #expect(menu.defaultItem == .newTerminal)
    }

    @Test func moveExistingTabExcludesTheTabThatOpenedTheMenu() {
        let opened = WorkspaceTabID()
        let other = WorkspaceTabID()
        let menu = makeMenu(sourceTabID: opened, tabs: [opened, other])

        #expect(menu.moveTabIDs(in: .thisPane) == [other])
        #expect(!menu.allMoveTabIDs.contains(opened))
    }

    @Test func moveExistingTabNeverListsOtherWorktrees() {
        let currentWorktree = UUID()
        let otherWorktree = UUID()
        let currentTab = WorkspaceTabID()
        let otherTab = WorkspaceTabID()
        let menu = SplitContentMenuModel(
            worktreeID: currentWorktree,
            sourceTabID: currentTab,
            layout: layout(tabs: [currentTab]),
            layoutsByWorktree: [
                currentWorktree: layout(tabs: [currentTab]),
                otherWorktree: layout(tabs: [otherTab])
            ],
            groupSize: CGSize(width: 800, height: 600),
            placement: .right,
            installedAgents: [],
            resumedChats: [])

        #expect(menu.allMoveTabIDs == [])
        #expect(!menu.allMoveTabIDs.contains(otherTab))
    }

    @Test func anEmptySubmenuShowsADisabledExplanatoryItem() {
        let menu = makeMenu(installedAgents: [], resumedChats: [])

        #expect(menu.hasDisabledExplanation(in: .newChat))
        #expect(menu.hasDisabledExplanation(in: .resumeChat))
        #expect(menu.hasConfigureAgentsAction)
    }

    @Test func openingAnAlreadyOpenFileMovesItsExistingTab() async {
        let worktree = Worktree(
            id: UUID(), projectId: UUID(), branch: "main", path: "/tmp/worktree")
        let fileURL = URL(fileURLWithPath: "/tmp/worktree/README.md")
        let existingTab = WorkspaceTab(
            id: WorkspaceTabID(), title: "README.md", titleIsAutoNamed: true,
            content: .document(
                DocumentID.makeCanonical(worktreeID: worktree.id, path: fileURL.path),
                editor: .markdown))
        let groupID = PaneGroupID()
        let restoredLayout = layout(groupID: groupID, tabs: [existingTab.id, WorkspaceTabID()])
        let persistence = FakeWorkspacePersistence()
        persistence.restored = RestoredWorkspace(
            layout: restoredLayout,
            tabs: [existingTab.id: existingTab],
            revision: 0,
            diagnostics: [])
        let adapter = DocumentContentAdapter()
        let coordinator = WorkspaceCoordinator(
            persistence: persistence,
            registry: WorkspaceContentRegistry(),
            adapters: [.document: adapter])
        await coordinator.restore(worktree: worktree)

        await coordinator.requestSplit(
            anchor: groupID,
            placement: .right,
            choice: .openFile(fileURL, editor: .markdown),
            in: worktree)

        let result = coordinator.layouts[worktree.id]
        #expect(result?.allTabs.count == 2)
        #expect(result?.groupContaining(tab: existingTab.id) != groupID)
    }

    @Test func anIneligibleSplitIsDisabledWithItsReason() {
        let menu = SplitContentMenuModel(
            worktreeID: UUID(),
            sourceTabID: WorkspaceTabID(),
            layout: layout(tabs: [WorkspaceTabID()]),
            layoutsByWorktree: [:],
            groupSize: CGSize(width: 100, height: 600),
            placement: .right,
            installedAgents: [],
            resumedChats: [])

        #expect(!menu.splitItem.isEnabled)
        #expect(menu.splitItem.disabledReason?.isEmpty == false)
    }

    private func makeMenu(
        sourceTabID: WorkspaceTabID = WorkspaceTabID(),
        tabs: [WorkspaceTabID] = [WorkspaceTabID()],
        installedAgents: [SplitMenuAgent] = [SplitMenuAgent(id: "codex", name: "Codex")],
        resumedChats: [SplitMenuChat] = [SplitMenuChat(id: "session-1", title: "Review")]
    ) -> SplitContentMenuModel {
        SplitContentMenuModel(
            worktreeID: UUID(),
            sourceTabID: sourceTabID,
            layout: layout(tabs: tabs),
            layoutsByWorktree: [:],
            groupSize: CGSize(width: 800, height: 600),
            placement: .right,
            installedAgents: installedAgents,
            resumedChats: resumedChats)
    }

    private func layout(
        groupID: PaneGroupID = PaneGroupID(),
        tabs: [WorkspaceTabID]
    ) -> WorkspaceLayout {
        let workspaceTabs = tabs.map { id in
            WorkspaceTab(
                id: id, title: id.rawValue.uuidString, titleIsAutoNamed: true,
                content: .terminal(TerminalContentID(id.rawValue)))
        }
        return layout(groupID: groupID, tabs: workspaceTabs)
    }

    private func layout(groupID: PaneGroupID, tabs: [WorkspaceTab]) -> WorkspaceLayout {
        let group = PaneGroup(id: groupID, tabs: tabs, activeTabID: tabs.first?.id)
        switch WorkspaceLayout.make(
            root: .group(groupID), groups: [groupID: group], activeGroupID: groupID) {
        case .success(let layout): return layout
        case .failure(let error): Issue.record("invalid fixture layout: \(error)"); return .empty()
        }
    }
}
