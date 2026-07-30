import Foundation

public enum WorkspaceLayoutEngine {
    public static func apply(
        _ command: WorkspaceLayoutCommand,
        to layout: WorkspaceLayout
    ) -> Result<WorkspaceLayoutTransition, WorkspaceLayoutError> {
        switch command {
        case .activateTab(let tabID):
            activateTab(tabID, in: layout)
        case .activateGroup(let groupID):
            activateGroup(groupID, in: layout)
        case .renameTab(let tabID, let title, let isAutoNamed):
            renameTab(tabID, title: title, isAutoNamed: isAutoNamed, in: layout)
        case .updateViewState(let tabID, let viewState):
            updateViewState(tabID, viewState, in: layout)
        case .insertTab(let tab, let groupID, let index, let activate):
            insertTab(tab, into: groupID, index: index, activate: activate, in: layout)
        case .moveTab(let tabID, to: .group(let groupID, let index)):
            moveTab(tabID, to: groupID, index: index, in: layout)
        case .moveTab(let tabID, to: .newSplit(let anchor, let placement,
                                                let newGroup, let newSplit)):
            moveTabToNewSplit(
                tabID, anchor: anchor, placement: placement,
                newGroup: newGroup, newSplit: newSplit, in: layout)
        case .splitGroup(let anchor, let placement, let newGroup, let newSplit, let content):
            splitGroup(
                anchor: anchor, placement: placement, newGroup: newGroup,
                newSplit: newSplit, content: content, in: layout)
        case .closeTab(let tabID):
            closeTab(tabID, in: layout)
        case .setPreferredFraction(let splitID, let fraction):
            setPreferredFraction(splitID, fraction: fraction, in: layout)
        }
    }

    private static func activateTab(
        _ tabID: WorkspaceTabID,
        in layout: WorkspaceLayout
    ) -> Result<WorkspaceLayoutTransition, WorkspaceLayoutError> {
        guard let groupID = layout.groupContaining(tab: tabID) else {
            return .failure(.unknownTab(tabID))
        }
        var groups = layout.groups
        groups[groupID]?.activeTabID = tabID
        let delta = WorkspaceLayoutDelta(
            activeGroupChanged: layout.activeGroupID == groupID ? nil : groupID,
            activeTabChanges: [groupID: tabID],
            isStructural: false)
        return makeTransition(
            root: layout.root, groups: groups, activeGroupID: groupID,
            delta: delta, focusIntent: .focusTab(tabID))
    }

    private static func activateGroup(
        _ groupID: PaneGroupID,
        in layout: WorkspaceLayout
    ) -> Result<WorkspaceLayoutTransition, WorkspaceLayoutError> {
        guard layout.groups[groupID] != nil else {
            return .failure(.unknownGroup(groupID))
        }
        let delta = WorkspaceLayoutDelta(
            activeGroupChanged: layout.activeGroupID == groupID ? nil : groupID,
            isStructural: false)
        return makeTransition(
            root: layout.root, groups: layout.groups, activeGroupID: groupID,
            delta: delta, focusIntent: .none)
    }

    private static func renameTab(
        _ tabID: WorkspaceTabID,
        title: String,
        isAutoNamed: Bool,
        in layout: WorkspaceLayout
    ) -> Result<WorkspaceLayoutTransition, WorkspaceLayoutError> {
        guard let groupID = layout.groupContaining(tab: tabID),
              var group = layout.groups[groupID],
              let tabIndex = group.tabs.firstIndex(where: { $0.id == tabID }) else {
            return .failure(.unknownTab(tabID))
        }
        group.tabs[tabIndex].title = title
        group.tabs[tabIndex].titleIsAutoNamed = isAutoNamed
        var groups = layout.groups
        groups[groupID] = group
        return makeTransition(
            root: layout.root, groups: groups, activeGroupID: layout.activeGroupID,
            delta: WorkspaceLayoutDelta(isStructural: false), focusIntent: .none)
    }

    private static func updateViewState(
        _ tabID: WorkspaceTabID,
        _ viewState: WorkspaceTabViewState,
        in layout: WorkspaceLayout
    ) -> Result<WorkspaceLayoutTransition, WorkspaceLayoutError> {
        guard let groupID = layout.groupContaining(tab: tabID),
              var group = layout.groups[groupID],
              let tabIndex = group.tabs.firstIndex(where: { $0.id == tabID }) else {
            return .failure(.unknownTab(tabID))
        }
        group.tabs[tabIndex].viewState = viewState
        var groups = layout.groups
        groups[groupID] = group
        return makeTransition(
            root: layout.root, groups: groups, activeGroupID: layout.activeGroupID,
            delta: WorkspaceLayoutDelta(isStructural: false), focusIntent: .none)
    }

    private static func insertTab(
        _ tab: WorkspaceTab,
        into groupID: PaneGroupID,
        index: Int?,
        activate: Bool,
        in layout: WorkspaceLayout
    ) -> Result<WorkspaceLayoutTransition, WorkspaceLayoutError> {
        guard var group = layout.groups[groupID] else {
            return .failure(.unknownGroup(groupID))
        }
        guard !layout.allTabs.contains(where: { $0.id == tab.id }) else {
            return .failure(.duplicateID("tab:" + tab.id.rawValue.uuidString))
        }
        guard !layout.allTabs.contains(where: {
            $0.content.contentIdentifierString == tab.content.contentIdentifierString
        }) else {
            return .failure(.duplicateContentOwnership(tab.content.contentIdentifierString))
        }

        let insertionIndex = index ?? group.tabs.count
        guard (0...group.tabs.count).contains(insertionIndex) else {
            return .failure(.unknownGroup(groupID))
        }
        group.tabs.insert(tab, at: insertionIndex)
        if activate {
            group.activeTabID = tab.id
        }
        var groups = layout.groups
        groups[groupID] = group
        let delta = WorkspaceLayoutDelta(
            insertedTabs: [tab.id],
            activeGroupChanged: activate && layout.activeGroupID != groupID ? groupID : nil,
            activeTabChanges: activate ? [groupID: tab.id] : [:],
            isStructural: true)
        return makeTransition(
            root: layout.root, groups: groups,
            activeGroupID: activate ? groupID : layout.activeGroupID,
            delta: delta, focusIntent: activate ? .focusTab(tab.id) : .none)
    }

    private static func moveTab(
        _ tabID: WorkspaceTabID,
        to destinationGroupID: PaneGroupID,
        index: Int,
        in layout: WorkspaceLayout
    ) -> Result<WorkspaceLayoutTransition, WorkspaceLayoutError> {
        guard let sourceGroupID = layout.groupContaining(tab: tabID) else {
            return .failure(.unknownTab(tabID))
        }
        guard layout.groups[destinationGroupID] != nil else {
            return .failure(.unknownGroup(destinationGroupID))
        }
        if sourceGroupID == destinationGroupID {
            return reorderTab(tabID, in: sourceGroupID, at: index, layout: layout)
        }
        guard var sourceGroup = layout.groups[sourceGroupID],
              var destinationGroup = layout.groups[destinationGroupID],
              let sourceIndex = sourceGroup.tabs.firstIndex(where: { $0.id == tabID }) else {
            return .failure(.unknownTab(tabID))
        }
        guard (0...destinationGroup.tabs.count).contains(index) else {
            return .failure(.unknownGroup(destinationGroupID))
        }

        let movedTab = sourceGroup.tabs.remove(at: sourceIndex)
        let sourceWasActive = sourceGroup.activeTabID == tabID
        if sourceWasActive {
            sourceGroup.activeTabID = repairedSelection(
                in: sourceGroup.tabs, afterRemoving: sourceIndex)
        }
        destinationGroup.tabs.insert(movedTab, at: index)
        destinationGroup.activeTabID = movedTab.id

        var groups = layout.groups
        var root = layout.root
        var removedGroups: [PaneGroupID] = []
        var collapsedSplits: [SplitID] = []
        let activeGroupID = destinationGroupID
        if sourceGroup.tabs.isEmpty {
            guard let removal = removeGroup(sourceGroupID, from: root),
                  let replacementRoot = removal.node else {
                return .failure(.unknownGroup(sourceGroupID))
            }
            root = replacementRoot
            groups.removeValue(forKey: sourceGroupID)
            removedGroups = [sourceGroupID]
            if let collapsedSplit = removal.collapsedSplit {
                collapsedSplits = [collapsedSplit]
            }
        } else {
            groups[sourceGroupID] = sourceGroup
        }
        groups[destinationGroupID] = destinationGroup

        let delta = WorkspaceLayoutDelta(
            movedTabs: [tabID],
            removedGroups: removedGroups,
            collapsedSplits: collapsedSplits,
            activeGroupChanged: layout.activeGroupID == activeGroupID ? nil : activeGroupID,
            activeTabChanges: sourceGroup.tabs.isEmpty
                ? [destinationGroupID: movedTab.id]
                : [sourceGroupID: sourceGroup.activeTabID, destinationGroupID: movedTab.id],
            isStructural: true)
        return makeTransition(
            root: root, groups: groups, activeGroupID: activeGroupID,
            delta: delta, focusIntent: .focusTab(tabID))
    }

    private static func reorderTab(
        _ tabID: WorkspaceTabID,
        in groupID: PaneGroupID,
        at index: Int,
        layout: WorkspaceLayout
    ) -> Result<WorkspaceLayoutTransition, WorkspaceLayoutError> {
        guard var group = layout.groups[groupID] else {
            return .failure(.unknownGroup(groupID))
        }
        guard let sourceIndex = group.tabs.firstIndex(where: { $0.id == tabID }) else {
            return .failure(.unknownTab(tabID))
        }
        guard (0..<group.tabs.count).contains(index) else {
            return .failure(.unknownGroup(groupID))
        }
        let tab = group.tabs.remove(at: sourceIndex)
        group.tabs.insert(tab, at: index)
        var groups = layout.groups
        groups[groupID] = group
        return makeTransition(
            root: layout.root, groups: groups, activeGroupID: layout.activeGroupID,
            delta: WorkspaceLayoutDelta(
                reorderedGroups: sourceIndex == index ? [] : [groupID],
                isStructural: true),
            focusIntent: .none)
    }

    private static func moveTabToNewSplit(
        _ tabID: WorkspaceTabID,
        anchor: PaneGroupID,
        placement: SplitPlacementSide,
        newGroup: PaneGroupID,
        newSplit: SplitID,
        in layout: WorkspaceLayout
    ) -> Result<WorkspaceLayoutTransition, WorkspaceLayoutError> {
        splitGroup(
            anchor: anchor, placement: placement, newGroup: newGroup,
            newSplit: newSplit, content: .existingTab(tabID), in: layout)
    }

    private static func splitGroup(
        anchor: PaneGroupID,
        placement: SplitPlacementSide,
        newGroup newGroupID: PaneGroupID,
        newSplit newSplitID: SplitID,
        content: SplitContentPayload,
        in layout: WorkspaceLayout
    ) -> Result<WorkspaceLayoutTransition, WorkspaceLayoutError> {
        guard var anchorGroup = layout.groups[anchor] else {
            return .failure(.unknownGroup(anchor))
        }
        guard layout.groups[newGroupID] == nil else {
            return .failure(.duplicateID("group:" + newGroupID.rawValue.uuidString))
        }
        guard !layout.splitIDs().contains(newSplitID) else {
            return .failure(.duplicateID("split:" + newSplitID.rawValue.uuidString))
        }

        let tab: WorkspaceTab
        var insertedTabs: [WorkspaceTabID] = []
        var movedTabs: [WorkspaceTabID] = []
        var activeTabChanges: [PaneGroupID: WorkspaceTabID?] = [:]
        switch content {
        case .newTab(let newTab):
            guard !layout.allTabs.contains(where: { $0.id == newTab.id }) else {
                return .failure(.duplicateID("tab:" + newTab.id.rawValue.uuidString))
            }
            guard !layout.allTabs.contains(where: {
                $0.content.contentIdentifierString == newTab.content.contentIdentifierString
            }) else {
                return .failure(.duplicateContentOwnership(
                    newTab.content.contentIdentifierString))
            }
            tab = newTab
            insertedTabs = [newTab.id]
        case .existingTab(let existingTabID):
            guard anchorGroup.tabs.count > 1 else {
                return .failure(.illegalSplitOfSoleTab(anchor))
            }
            guard let sourceIndex = anchorGroup.tabs.firstIndex(
                where: { $0.id == existingTabID }) else {
                return .failure(.unknownTab(existingTabID))
            }
            tab = anchorGroup.tabs.remove(at: sourceIndex)
            if anchorGroup.activeTabID == existingTabID {
                anchorGroup.activeTabID = repairedSelection(
                    in: anchorGroup.tabs, afterRemoving: sourceIndex)
                activeTabChanges[anchor] = anchorGroup.activeTabID
            }
            movedTabs = [existingTabID]
        }

        let newPaneGroup = PaneGroup(id: newGroupID, tabs: [tab], activeTabID: tab.id)
        let first: LayoutNode = placement.anchorIsFirstChild ? .group(anchor) : .group(newGroupID)
        let second: LayoutNode = placement.anchorIsFirstChild ? .group(newGroupID) : .group(anchor)
        let replacement: LayoutNode = .split(
            id: newSplitID, axis: placement.axis, fraction: 0.5,
            first: first, second: second)
        guard let root = replaceGroup(anchor, with: replacement, in: layout.root) else {
            return .failure(.unknownGroup(anchor))
        }

        var groups = layout.groups
        groups[anchor] = anchorGroup
        groups[newGroupID] = newPaneGroup
        activeTabChanges[newGroupID] = tab.id
        let delta = WorkspaceLayoutDelta(
            insertedTabs: insertedTabs,
            movedTabs: movedTabs,
            insertedGroups: [newGroupID],
            insertedSplits: [newSplitID],
            activeGroupChanged: layout.activeGroupID == newGroupID ? nil : newGroupID,
            activeTabChanges: activeTabChanges,
            isStructural: true)
        return makeTransition(
            root: root, groups: groups, activeGroupID: newGroupID,
            delta: delta, focusIntent: .focusTab(tab.id))
    }

    private static func closeTab(
        _ tabID: WorkspaceTabID,
        in layout: WorkspaceLayout
    ) -> Result<WorkspaceLayoutTransition, WorkspaceLayoutError> {
        guard let groupID = layout.groupContaining(tab: tabID),
              var group = layout.groups[groupID],
              let tabIndex = group.tabs.firstIndex(where: { $0.id == tabID }) else {
            return .failure(.unknownTab(tabID))
        }

        let wasActive = group.activeTabID == tabID
        group.tabs.remove(at: tabIndex)
        if wasActive {
            group.activeTabID = repairedSelection(in: group.tabs, afterRemoving: tabIndex)
        }

        var groups = layout.groups
        var root = layout.root
        var activeGroupID = layout.activeGroupID
        var removedGroups: [PaneGroupID] = []
        var collapsedSplits: [SplitID] = []
        if !group.tabs.isEmpty {
            groups[groupID] = group
        } else if case .group(let rootGroupID) = root, rootGroupID == groupID {
            groups[groupID] = group
            activeGroupID = groupID
        } else {
            guard let removal = removeGroup(groupID, from: root),
                  let replacementRoot = removal.node else {
                return .failure(.unknownGroup(groupID))
            }
            root = replacementRoot
            groups.removeValue(forKey: groupID)
            removedGroups = [groupID]
            if let collapsedSplit = removal.collapsedSplit {
                collapsedSplits = [collapsedSplit]
            }
            if activeGroupID == groupID {
                activeGroupID = firstGroup(in: root)
            }
        }

        var activeTabChanges: [PaneGroupID: WorkspaceTabID?] = [:]
        if removedGroups.isEmpty {
            activeTabChanges[groupID] = group.activeTabID
        }
        let focusIntent: FocusIntent
        if wasActive, let activeTabID = groups[activeGroupID]?.activeTabID {
            focusIntent = .focusTab(activeTabID)
        } else {
            focusIntent = .none
        }
        let delta = WorkspaceLayoutDelta(
            removedTabs: [tabID],
            removedGroups: removedGroups,
            collapsedSplits: collapsedSplits,
            activeGroupChanged: layout.activeGroupID == activeGroupID ? nil : activeGroupID,
            activeTabChanges: activeTabChanges,
            isStructural: true)
        return makeTransition(
            root: root, groups: groups, activeGroupID: activeGroupID,
            delta: delta, focusIntent: focusIntent)
    }

    private static func setPreferredFraction(
        _ splitID: SplitID,
        fraction: Double,
        in layout: WorkspaceLayout
    ) -> Result<WorkspaceLayoutTransition, WorkspaceLayoutError> {
        guard fraction.isFinite, 0.0 < fraction, fraction < 1.0 else {
            return .failure(.invalidFraction(splitID, fraction))
        }
        guard let root = replaceSplit(splitID, fraction: fraction, in: layout.root) else {
            return .failure(.invalidFraction(splitID, fraction))
        }
        return makeTransition(
            root: root, groups: layout.groups, activeGroupID: layout.activeGroupID,
            delta: WorkspaceLayoutDelta(
                preferredFractionChanges: [splitID: fraction],
                isStructural: false),
            focusIntent: .focusDivider(splitID))
    }

    private static func repairedSelection(
        in tabs: [WorkspaceTab],
        afterRemoving index: Int
    ) -> WorkspaceTabID? {
        if index > 0, tabs.indices.contains(index - 1) {
            return tabs[index - 1].id
        }
        if tabs.indices.contains(index) {
            return tabs[index].id
        }
        return nil
    }

    private static func replaceGroup(
        _ groupID: PaneGroupID,
        with replacement: LayoutNode,
        in node: LayoutNode
    ) -> LayoutNode? {
        switch node {
        case .group(let id):
            return id == groupID ? replacement : nil
        case .split(let id, let axis, let fraction, let first, let second):
            if let replacedFirst = replaceGroup(groupID, with: replacement, in: first) {
                return .split(
                    id: id, axis: axis, fraction: fraction,
                    first: replacedFirst, second: second)
            }
            if let replacedSecond = replaceGroup(groupID, with: replacement, in: second) {
                return .split(
                    id: id, axis: axis, fraction: fraction,
                    first: first, second: replacedSecond)
            }
            return nil
        }
    }

    private static func replaceSplit(
        _ splitID: SplitID,
        fraction: Double,
        in node: LayoutNode
    ) -> LayoutNode? {
        switch node {
        case .group:
            return nil
        case .split(let id, let axis, let currentFraction, let first, let second):
            if id == splitID {
                return .split(
                    id: id, axis: axis, fraction: fraction,
                    first: first, second: second)
            }
            if let replacedFirst = replaceSplit(splitID, fraction: fraction, in: first) {
                return .split(
                    id: id, axis: axis, fraction: currentFraction,
                    first: replacedFirst, second: second)
            }
            if let replacedSecond = replaceSplit(splitID, fraction: fraction, in: second) {
                return .split(
                    id: id, axis: axis, fraction: currentFraction,
                    first: first, second: replacedSecond)
            }
            return nil
        }
    }

    private struct GroupRemoval {
        let node: LayoutNode?
        let collapsedSplit: SplitID?
    }

    private static func removeGroup(
        _ groupID: PaneGroupID,
        from node: LayoutNode
    ) -> GroupRemoval? {
        switch node {
        case .group(let id):
            return id == groupID ? GroupRemoval(node: nil, collapsedSplit: nil) : nil
        case .split(let id, let axis, let fraction, let first, let second):
            if let removal = removeGroup(groupID, from: first) {
                if let newFirst = removal.node {
                    return GroupRemoval(
                        node: .split(
                            id: id, axis: axis, fraction: fraction,
                            first: newFirst, second: second),
                        collapsedSplit: removal.collapsedSplit)
                }
                return GroupRemoval(node: second, collapsedSplit: id)
            }
            if let removal = removeGroup(groupID, from: second) {
                if let newSecond = removal.node {
                    return GroupRemoval(
                        node: .split(
                            id: id, axis: axis, fraction: fraction,
                            first: first, second: newSecond),
                        collapsedSplit: removal.collapsedSplit)
                }
                return GroupRemoval(node: first, collapsedSplit: id)
            }
            return nil
        }
    }

    private static func firstGroup(in node: LayoutNode) -> PaneGroupID {
        switch node {
        case .group(let id):
            id
        case .split(_, _, _, let first, _):
            firstGroup(in: first)
        }
    }

    private static func makeTransition(
        root: LayoutNode,
        groups: [PaneGroupID: PaneGroup],
        activeGroupID: PaneGroupID,
        delta: WorkspaceLayoutDelta,
        focusIntent: FocusIntent
    ) -> Result<WorkspaceLayoutTransition, WorkspaceLayoutError> {
        switch WorkspaceLayout.make(
            root: root, groups: groups, activeGroupID: activeGroupID
        ) {
        case .success(let layout):
            return .success(WorkspaceLayoutTransition(
                layout: layout, delta: delta, focusIntent: focusIntent))
        case .failure(let error):
            return .failure(error)
        }
    }
}
