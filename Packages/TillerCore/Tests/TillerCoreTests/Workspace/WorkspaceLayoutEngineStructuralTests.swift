import Testing
@testable import TillerCore

@Suite struct WorkspaceLayoutEngineStructuralTests {
    @Test func splitCreatesExactlyOneGroupAndOneSplitAtHalf() throws {
        let (layout, _) = Fixtures.singleTerminalTab()
        let newGroup = PaneGroupID()
        let newSplit = SplitID()
        let fresh = Fixtures.freshTerminalTab()
        let result = WorkspaceLayoutEngine.apply(
            .splitGroup(anchor: layout.activeGroupID, placement: .right,
                        newGroup: newGroup, newSplit: newSplit,
                        content: .newTab(fresh)),
            to: layout)
        let transition = try #require(try result.get())

        #expect(transition.delta.insertedGroups == [newGroup])
        #expect(transition.delta.insertedSplits == [newSplit])
        #expect(transition.layout.groups.count == 2)
        if case .split(let splitID, let axis, let fraction, .group(let first),
                       .group(let second)) = transition.layout.root {
            #expect(splitID == newSplit)
            #expect(axis == .horizontal)
            #expect(fraction == 0.5)
            #expect(first == layout.activeGroupID)
            #expect(second == newGroup)
        } else {
            Issue.record("root should be a two-leaf split")
        }
        #expect(transition.layout.activeGroupID == newGroup)
        #expect(transition.focusIntent == .focusTab(fresh.id))
    }

    @Test func splitDownPlacesTheNewGroupSecondOnTheVerticalAxis() throws {
        let (layout, _) = Fixtures.singleTerminalTab()
        let newGroup = PaneGroupID()
        let newSplit = SplitID()
        let fresh = Fixtures.freshTerminalTab()
        let result = WorkspaceLayoutEngine.apply(
            .splitGroup(anchor: layout.activeGroupID, placement: .down,
                        newGroup: newGroup, newSplit: newSplit,
                        content: .newTab(fresh)),
            to: layout)
        let transition = try #require(try result.get())

        guard case .split(let splitID, let axis, let fraction, .group(let first),
                          .group(let second)) = transition.layout.root else {
            Issue.record("root should be a vertical two-leaf split")
            return
        }
        #expect(splitID == newSplit)
        #expect(axis == .vertical)
        #expect(fraction == 0.5)
        #expect(first == layout.activeGroupID)
        #expect(second == newGroup)
    }

    @Test func movingTheLastTabOutOfANonRootGroupCollapsesItAndItsParent() throws {
        let sourceGroup = PaneGroupID()
        let destinationGroup = PaneGroupID()
        let splitID = SplitID()
        let movedTab = Fixtures.freshTerminalTab()
        let siblingTab = Fixtures.freshTerminalTab()
        let layout = makeLayout(
            root: .split(id: splitID, axis: .horizontal, fraction: 0.3,
                         first: .group(sourceGroup), second: .group(destinationGroup)),
            groups: [
                sourceGroup: PaneGroup(id: sourceGroup, tabs: [movedTab],
                                        activeTabID: movedTab.id),
                destinationGroup: PaneGroup(id: destinationGroup, tabs: [siblingTab],
                                             activeTabID: siblingTab.id)
            ],
            activeGroupID: sourceGroup)

        let result = WorkspaceLayoutEngine.apply(
            .moveTab(movedTab.id, to: .group(destinationGroup, index: 1)), to: layout)
        let transition = try #require(try result.get())

        #expect(transition.layout.root == .group(destinationGroup))
        #expect(transition.layout.groups.count == 1)
        #expect(transition.layout.group(destinationGroup)?.tabs.map(\.id)
                == [siblingTab.id, movedTab.id])
        #expect(transition.delta.movedTabs == [movedTab.id])
        #expect(transition.delta.removedGroups == [sourceGroup])
        #expect(transition.delta.collapsedSplits == [splitID])
        #expect(transition.layout.activeGroupID == destinationGroup)
        #expect(transition.focusIntent == .focusTab(movedTab.id))
        #expect(WorkspaceLayoutInvariants.validate(transition.layout) == nil)
    }

    @Test func closingTheLastRootTabLeavesTheValidEmptyRootGroup() throws {
        let (layout, _) = Fixtures.singleTerminalTab()
        let tabID = layout.allTabs[0].id
        let result = WorkspaceLayoutEngine.apply(.closeTab(tabID), to: layout)
        let transition = try #require(try result.get())

        #expect(transition.layout.root == .group(layout.activeGroupID))
        #expect(transition.layout.groups.count == 1)
        #expect(transition.layout.group(layout.activeGroupID)?.tabs.isEmpty == true)
        #expect(transition.layout.group(layout.activeGroupID)?.activeTabID == nil)
        #expect(transition.delta.removedTabs == [tabID])
        #expect(transition.delta.removedGroups.isEmpty)
        #expect(WorkspaceLayoutInvariants.validate(transition.layout) == nil)
    }

    @Test func splittingASoleTabAwayFromItsOwnGroupIsRejected() {
        let (layout, _) = Fixtures.singleTerminalTab()
        let tabID = layout.allTabs[0].id
        let result = WorkspaceLayoutEngine.apply(
            .splitGroup(anchor: layout.activeGroupID, placement: .right,
                        newGroup: PaneGroupID(), newSplit: SplitID(),
                        content: .existingTab(tabID)),
            to: layout)
        #expect(result == .failure(.illegalSplitOfSoleTab(layout.activeGroupID)))
    }

    @Test func moveToUnknownGroupLeavesTheLayoutUntouched() {
        let (layout, _) = Fixtures.singleTerminalTab()
        let unknownGroup = PaneGroupID()
        let tabID = layout.allTabs[0].id
        let before = layout
        let result = WorkspaceLayoutEngine.apply(
            .moveTab(tabID, to: .group(unknownGroup, index: 0)), to: layout)

        #expect(result == .failure(.unknownGroup(unknownGroup)))
        #expect(layout == before)
    }

    @Test func closeRepairsSelectionDeterministicallyToTheNeighbourOnTheLeft() throws {
        let groupID = PaneGroupID()
        let tabs = [
            Fixtures.freshTerminalTab(),
            Fixtures.freshTerminalTab(),
            Fixtures.freshTerminalTab()
        ]
        let layout = makeLayout(
            root: .group(groupID),
            groups: [groupID: PaneGroup(id: groupID, tabs: tabs, activeTabID: tabs[1].id)],
            activeGroupID: groupID)
        let result = WorkspaceLayoutEngine.apply(.closeTab(tabs[1].id), to: layout)
        let transition = try #require(try result.get())

        #expect(transition.layout.group(groupID)?.tabs.map(\.id)
                == [tabs[0].id, tabs[2].id])
        #expect(transition.layout.group(groupID)?.activeTabID == tabs[0].id)
        #expect(transition.layout.activeGroupID == groupID)
        #expect(transition.focusIntent == .focusTab(tabs[0].id))
    }

    @Test func everySuccessfulCommandProducesAValidLayout() {
        let (initialLayout, tabB) = Fixtures.twoTabsInOneGroup()
        let rootGroup = initialLayout.activeGroupID
        let inserted = Fixtures.freshTerminalTab()
        let splitTab = Fixtures.freshTerminalTab()
        let splitGroup = PaneGroupID()
        let splitID = SplitID()
        let moveGroup = PaneGroupID()
        let moveSplit = SplitID()
        var layout = initialLayout
        let commands: [WorkspaceLayoutCommand] = [
            .activateTab(tabB),
            .activateGroup(rootGroup),
            .renameTab(tabB, title: "renamed", isAutoNamed: false),
            .updateViewState(tabB, .empty),
            .insertTab(inserted, into: rootGroup, index: nil, activate: true),
            .moveTab(inserted.id, to: .group(rootGroup, index: 0)),
            .splitGroup(anchor: rootGroup, placement: .right,
                        newGroup: splitGroup, newSplit: splitID,
                        content: .newTab(splitTab)),
            .setPreferredFraction(splitID, 0.6),
            .moveTab(tabB, to: .newSplit(anchor: rootGroup, placement: .down,
                                         newGroup: moveGroup, newSplit: moveSplit)),
            .closeTab(inserted.id)
        ]

        for command in commands {
            guard case .success(let transition) = WorkspaceLayoutEngine.apply(command, to: layout) else {
                Issue.record("fixture command should succeed: \(String(describing: command))")
                return
            }
            layout = transition.layout
            #expect(WorkspaceLayoutInvariants.validate(layout) == nil)
        }
    }

    /// The sibling promoted by a collapse must land in place of its parent
    /// split without disturbing the enclosing split's fraction. Every other
    /// collapse test here collapses at the root, where there is no enclosing
    /// split to disturb — so none of them can catch a regression that rebuilds
    /// the grandparent with a default fraction instead of its own.
    @Test func collapsingANestedSplitPreservesTheGrandparentFraction() throws {
        let outerSplit = SplitID()
        let innerSplit = SplitID()
        let leftGroup = PaneGroupID()
        let sourceGroup = PaneGroupID()
        let siblingGroup = PaneGroupID()
        let leftTab = Fixtures.freshTerminalTab()
        let movedTab = Fixtures.freshTerminalTab()
        let siblingTab = Fixtures.freshTerminalTab()
        let layout = makeLayout(
            root: .split(id: outerSplit, axis: .horizontal, fraction: 0.75,
                         first: .group(leftGroup),
                         second: .split(id: innerSplit, axis: .vertical, fraction: 0.4,
                                        first: .group(sourceGroup),
                                        second: .group(siblingGroup))),
            groups: [
                leftGroup: PaneGroup(id: leftGroup, tabs: [leftTab], activeTabID: leftTab.id),
                sourceGroup: PaneGroup(id: sourceGroup, tabs: [movedTab],
                                       activeTabID: movedTab.id),
                siblingGroup: PaneGroup(id: siblingGroup, tabs: [siblingTab],
                                        activeTabID: siblingTab.id)
            ],
            activeGroupID: sourceGroup)

        let result = WorkspaceLayoutEngine.apply(
            .moveTab(movedTab.id, to: .group(leftGroup, index: 1)), to: layout)
        let transition = try #require(try result.get())

        #expect(transition.delta.removedGroups == [sourceGroup])
        #expect(transition.delta.collapsedSplits == [innerSplit])
        #expect(transition.layout.root == .split(
            id: outerSplit, axis: .horizontal, fraction: 0.75,
            first: .group(leftGroup), second: .group(siblingGroup)))
        #expect(transition.layout.preferredFraction(for: outerSplit) == 0.75)
        #expect(transition.layout.groups.count == 2)
        #expect(WorkspaceLayoutInvariants.validate(transition.layout) == nil)
    }

    private func makeLayout(root: LayoutNode,
                            groups: [PaneGroupID: PaneGroup],
                            activeGroupID: PaneGroupID) -> WorkspaceLayout {
        guard case .success(let layout) = WorkspaceLayout.make(
            root: root, groups: groups, activeGroupID: activeGroupID) else {
            preconditionFailure("test fixture must be valid")
        }
        return layout
    }
}
