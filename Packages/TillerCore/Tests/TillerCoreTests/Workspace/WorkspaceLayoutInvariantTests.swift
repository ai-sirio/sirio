import Testing
import Foundation
@testable import TillerCore

@Suite struct WorkspaceLayoutInvariantTests {
    private func tab(_ title: String = "t") -> WorkspaceTab {
        WorkspaceTab(id: WorkspaceTabID(), title: title, titleIsAutoNamed: true,
                     content: .terminal(TerminalContentID()))
    }

    @Test func emptyRootGroupIsValid() {
        let layout = WorkspaceLayout.empty()
        #expect(layout.groups.count == 1)
        #expect(layout.group(layout.activeGroupID)?.activeTabID == nil)
    }

    @Test func rejectsOrphanGroupNotPresentInTheTree() {
        let leaf = PaneGroupID(), orphan = PaneGroupID()
        let t = tab()
        let result = WorkspaceLayout.make(
            root: .group(leaf),
            groups: [leaf: PaneGroup(id: leaf, tabs: [t], activeTabID: t.id),
                     orphan: PaneGroup(id: orphan, tabs: [tab()], activeTabID: nil)],
            activeGroupID: leaf)
        #expect(result == .failure(.orphanGroup(orphan)))
    }

    @Test func rejectsEmptyGroupRegistry() {
        let leaf = PaneGroupID()
        let result = WorkspaceLayout.make(root: .group(leaf), groups: [:], activeGroupID: leaf)
        #expect(result == .failure(.emptyGroupRegistry))
    }

    @Test func rejectsEmptyNonRootGroup() {
        let first = PaneGroupID(), second = PaneGroupID()
        let t = tab()
        let split = SplitID()
        let result = WorkspaceLayout.make(
            root: .split(id: split, axis: .horizontal, fraction: 0.5,
                         first: .group(first), second: .group(second)),
            groups: [first: PaneGroup(id: first, tabs: [t], activeTabID: t.id),
                     second: PaneGroup(id: second, tabs: [], activeTabID: nil)],
            activeGroupID: first)
        #expect(result == .failure(.emptyNonRootGroup(second)))
    }

    @Test func rejectsFractionOutsideExclusiveZeroOneRange() {
        let first = PaneGroupID(), second = PaneGroupID()
        let firstTab = tab("first"), secondTab = tab("second")
        let split = SplitID()
        let result = WorkspaceLayout.make(
            root: .split(id: split, axis: .vertical, fraction: 0.0,
                         first: .group(first), second: .group(second)),
            groups: [first: PaneGroup(id: first, tabs: [firstTab], activeTabID: firstTab.id),
                     second: PaneGroup(id: second, tabs: [secondTab], activeTabID: secondTab.id)],
            activeGroupID: first)
        #expect(result == .failure(.invalidFraction(split, 0.0)))
    }

    @Test func rejectsDuplicateTabIDAcrossGroups() {
        let first = PaneGroupID(), second = PaneGroupID()
        let sharedID = WorkspaceTabID()
        let firstTab = WorkspaceTab(id: sharedID, title: "first", titleIsAutoNamed: true,
                                    content: .terminal(TerminalContentID()))
        let secondTab = WorkspaceTab(id: sharedID, title: "second", titleIsAutoNamed: true,
                                     content: .terminal(TerminalContentID()))
        let split = SplitID()
        let result = WorkspaceLayout.make(
            root: .split(id: split, axis: .horizontal, fraction: 0.5,
                         first: .group(first), second: .group(second)),
            groups: [first: PaneGroup(id: first, tabs: [firstTab], activeTabID: sharedID),
                     second: PaneGroup(id: second, tabs: [secondTab], activeTabID: sharedID)],
            activeGroupID: first)
        #expect(result == .failure(.duplicateID("tab:\(sharedID.rawValue.uuidString)")))
    }

    @Test func rejectsTwoTabsOwningTheSameContentIdentity() {
        let first = PaneGroupID(), second = PaneGroupID()
        let contentID = TerminalContentID()
        let firstTab = WorkspaceTab(id: WorkspaceTabID(), title: "first", titleIsAutoNamed: true,
                                    content: .terminal(contentID))
        let secondTab = WorkspaceTab(id: WorkspaceTabID(), title: "second", titleIsAutoNamed: true,
                                     content: .terminal(contentID))
        let split = SplitID()
        let result = WorkspaceLayout.make(
            root: .split(id: split, axis: .horizontal, fraction: 0.5,
                         first: .group(first), second: .group(second)),
            groups: [first: PaneGroup(id: first, tabs: [firstTab], activeTabID: firstTab.id),
                     second: PaneGroup(id: second, tabs: [secondTab], activeTabID: secondTab.id)],
            activeGroupID: first)
        #expect(result == .failure(.duplicateContentOwnership(
            "terminal:\(contentID.rawValue.uuidString)")))
    }

    @Test func rejectsActiveTabThatIsNotInItsGroup() {
        let groupID = PaneGroupID()
        let existing = tab()
        let unknown = WorkspaceTabID()
        let result = WorkspaceLayout.make(
            root: .group(groupID),
            groups: [groupID: PaneGroup(id: groupID, tabs: [existing], activeTabID: unknown)],
            activeGroupID: groupID)
        #expect(result == .failure(.activeTabNotInGroup(groupID)))
    }

    @Test func rejectsUnknownActiveGroup() {
        let groupID = PaneGroupID(), unknown = PaneGroupID()
        let t = tab()
        let result = WorkspaceLayout.make(
            root: .group(groupID),
            groups: [groupID: PaneGroup(id: groupID, tabs: [t], activeTabID: t.id)],
            activeGroupID: unknown)
        #expect(result == .failure(.unknownActiveGroup(unknown)))
    }

    @Test func rejectsNonRootGroupWithoutAnActiveTab() {
        let first = PaneGroupID(), second = PaneGroupID()
        let firstTab = tab("first"), secondTab = tab("second")
        let result = WorkspaceLayout.make(
            root: .split(id: SplitID(), axis: .horizontal, fraction: 0.5,
                         first: .group(first), second: .group(second)),
            groups: [first: PaneGroup(id: first, tabs: [firstTab], activeTabID: nil),
                     second: PaneGroup(id: second, tabs: [secondTab], activeTabID: secondTab.id)],
            activeGroupID: second)
        #expect(result == .failure(.activeTabNotInGroup(first)))
    }

    @Test func rejectsDuplicateSplitID() {
        let first = PaneGroupID(), second = PaneGroupID(), third = PaneGroupID()
        let firstTab = tab("first"), secondTab = tab("second"), thirdTab = tab("third")
        let split = SplitID()
        let result = WorkspaceLayout.make(
            root: .split(id: split, axis: .horizontal, fraction: 0.5,
                         first: .group(first),
                         second: .split(id: split, axis: .vertical, fraction: 0.5,
                                        first: .group(second), second: .group(third))),
            groups: [first: PaneGroup(id: first, tabs: [firstTab], activeTabID: firstTab.id),
                     second: PaneGroup(id: second, tabs: [secondTab], activeTabID: secondTab.id),
                     third: PaneGroup(id: third, tabs: [thirdTab], activeTabID: thirdTab.id)],
            activeGroupID: first)
        #expect(result == .failure(.duplicateID("split:\(split.rawValue.uuidString)")))
    }

    @Test func orderedGroupIDsFollowDepthFirstReadingOrder() {
        let left = PaneGroupID(), middle = PaneGroupID(), right = PaneGroupID()
        let leftTab = tab("left"), middleTab = tab("middle"), rightTab = tab("right")
        let result = WorkspaceLayout.make(
            root: .split(id: SplitID(), axis: .horizontal, fraction: 0.5,
                         first: .group(left),
                         second: .split(id: SplitID(), axis: .vertical, fraction: 0.5,
                                        first: .group(middle), second: .group(right))),
            groups: [left: PaneGroup(id: left, tabs: [leftTab], activeTabID: leftTab.id),
                     middle: PaneGroup(id: middle, tabs: [middleTab], activeTabID: middleTab.id),
                     right: PaneGroup(id: right, tabs: [rightTab], activeTabID: rightTab.id)],
            activeGroupID: left)

        guard case .success(let layout) = result else {
            Issue.record("expected a valid nested layout")
            return
        }
        #expect(layout.orderedGroupIDs == [left, middle, right])
    }
}
