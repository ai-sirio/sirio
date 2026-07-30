import Testing
@testable import TillerCore

@Suite struct WorkspaceLayoutEngineLocalTests {
    @Test func activatingATabDoesNotChangeTopology() throws {
        let (layout, tabB) = Fixtures.twoTabsInOneGroup()
        let before = layout.root
        let result = WorkspaceLayoutEngine.apply(.activateTab(tabB), to: layout)
        let transition = try #require(try result.get())
        #expect(transition.layout.root == before)
        #expect(transition.delta.insertedSplits.isEmpty && transition.delta.removedGroups.isEmpty)
        #expect(transition.delta.isStructural == false)
        #expect(transition.focusIntent == .focusTab(tabB))
    }

    @Test func insertingATabRejectsDuplicateContentOwnership() {
        let (layout, contentID) = Fixtures.singleTerminalTab()
        let clone = WorkspaceTab(
            id: WorkspaceTabID(), title: "clone", titleIsAutoNamed: true,
            content: .terminal(contentID))
        let result = WorkspaceLayoutEngine.apply(
            .insertTab(clone, into: layout.activeGroupID, index: nil, activate: true), to: layout)
        #expect(result == .failure(
            .duplicateContentOwnership(contentID.rawValue.uuidString)))
    }

    @Test func reorderingWithinAGroupEmitsReorderNotMove() throws {
        let (layout, tabB) = Fixtures.twoTabsInOneGroup()
        let groupID = layout.activeGroupID
        let result = WorkspaceLayoutEngine.apply(
            .moveTab(tabB, to: .group(groupID, index: 0)), to: layout)
        let transition = try #require(try result.get())
        #expect(transition.layout.group(groupID)?.tabs.map(\.id) == [tabB, layout.group(groupID)!.tabs[0].id])
        #expect(transition.delta.reorderedGroups == [groupID])
        #expect(transition.delta.movedTabs.isEmpty)
        #expect(transition.delta.isStructural)
        #expect(transition.layout.root == layout.root)
    }

    @Test func renameClearsAutoNamedFlagWithoutTouchingTopology() throws {
        let (layout, tabB) = Fixtures.twoTabsInOneGroup()
        let before = layout.root
        let result = WorkspaceLayoutEngine.apply(
            .renameTab(tabB, title: "Renamed", isAutoNamed: false), to: layout)
        let transition = try #require(try result.get())
        #expect(transition.layout.root == before)
        #expect(transition.layout.tab(tabB)?.title == "Renamed")
        #expect(transition.layout.tab(tabB)?.titleIsAutoNamed == false)
        #expect(transition.delta.isStructural == false)
        #expect(transition.focusIntent == .none)
    }

    @Test func updateViewStateIsNonStructural() throws {
        let (layout, tabB) = Fixtures.twoTabsInOneGroup()
        var state = WorkspaceTabViewState.empty
        state.chatComposerDraft = "draft"
        let result = WorkspaceLayoutEngine.apply(
            .updateViewState(tabB, state), to: layout)
        let transition = try #require(try result.get())
        #expect(transition.layout.tab(tabB)?.viewState == state)
        #expect(transition.layout.root == layout.root)
        #expect(transition.delta.isStructural == false)
        #expect(transition.focusIntent == .none)
    }

    @Test func acceptanceFixtureHasThePlannedEnvelope() {
        let layout = Fixtures.acceptanceEnvelope()
        #expect(layout.groups.count == 16)
        #expect(layout.allTabs.count == 64)
        #expect(Set(layout.allTabs.map { $0.content.kind }) == Set([
            .terminal, .chat, .document]))
        #expect(layout.allTabs.contains {
            if case .document(_, editor: .markdown) = $0.content { return true }
            return false
        })
        #expect(layout.allTabs.contains {
            if case .document(_, editor: .code) = $0.content { return true }
            return false
        })
    }
}
