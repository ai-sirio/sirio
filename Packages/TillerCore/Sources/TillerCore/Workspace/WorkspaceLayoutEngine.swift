import Foundation

public enum WorkspaceLayoutEngine {
    public static func apply(
        _ command: WorkspaceLayoutCommand,
        to layout: WorkspaceLayout
    ) -> Result<WorkspaceLayoutTransition, WorkspaceLayoutError> {
        switch command {
        case .activateTab(let tabID):
            return activateTab(tabID, in: layout)
        case .activateGroup(let groupID):
            return activateGroup(groupID, in: layout)
        case .renameTab(let tabID, let title, let isAutoNamed):
            return renameTab(tabID, title: title, isAutoNamed: isAutoNamed, in: layout)
        case .updateViewState(let tabID, let viewState):
            return updateViewState(tabID, viewState, in: layout)
        case .insertTab(let tab, let groupID, let index, let activate):
            return insertTab(tab, into: groupID, index: index, activate: activate, in: layout)
        case .moveTab(let tabID, to: .group(let groupID, let index)):
            return reorderTab(tabID, in: groupID, at: index, in: layout)
        case .moveTab(_, to: .newSplit(let anchor, _, _, _)),
             .splitGroup(anchor: let anchor, placement: _, newGroup: _, newSplit: _, content: _):
            return .failure(.unknownGroup(anchor))
        case .closeTab(let tabID):
            return .failure(.unknownTab(tabID))
        case .setPreferredFraction(let splitID, _):
            return .failure(.invalidFraction(splitID, 0.0))
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
        let activeGroupChanged = layout.activeGroupID == groupID ? nil : groupID
        let delta = WorkspaceLayoutDelta(
            activeGroupChanged: activeGroupChanged,
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
            return .failure(.duplicateID("tab:\(tab.id.rawValue.uuidString)"))
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
        let activeGroupChanged = activate && layout.activeGroupID != groupID ? groupID : nil
        let delta = WorkspaceLayoutDelta(
            insertedTabs: [tab.id],
            activeGroupChanged: activeGroupChanged,
            activeTabChanges: activate ? [groupID: tab.id] : [:],
            isStructural: true)
        return makeTransition(
            root: layout.root, groups: groups,
            activeGroupID: activate ? groupID : layout.activeGroupID,
            delta: delta,
            focusIntent: activate ? .focusTab(tab.id) : .none)
    }

    private static func reorderTab(
        _ tabID: WorkspaceTabID,
        in groupID: PaneGroupID,
        at index: Int,
        in layout: WorkspaceLayout
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
        let reordered = sourceIndex == index ? [] : [groupID]
        return makeTransition(
            root: layout.root, groups: groups, activeGroupID: layout.activeGroupID,
            delta: WorkspaceLayoutDelta(
                movedTabs: [],
                reorderedGroups: reordered,
                isStructural: true),
            focusIntent: .none)
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
