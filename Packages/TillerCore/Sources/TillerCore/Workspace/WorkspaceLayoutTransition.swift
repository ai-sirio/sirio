import Foundation

public enum FocusIntent: Sendable, Equatable {
    case none
    case focusTab(WorkspaceTabID)
    case focusDivider(SplitID)
}

public struct WorkspaceLayoutDelta: Sendable, Equatable {
    public var insertedTabs: [WorkspaceTabID]
    public var removedTabs: [WorkspaceTabID]
    public var movedTabs: [WorkspaceTabID]
    public var reorderedGroups: [PaneGroupID]
    public var insertedGroups: [PaneGroupID]
    public var removedGroups: [PaneGroupID]
    public var insertedSplits: [SplitID]
    public var collapsedSplits: [SplitID]
    public var activeGroupChanged: PaneGroupID?
    public var activeTabChanges: [PaneGroupID: WorkspaceTabID?]
    public var preferredFractionChanges: [SplitID: Double]
    public var isStructural: Bool

    public init(insertedTabs: [WorkspaceTabID] = [],
                removedTabs: [WorkspaceTabID] = [],
                movedTabs: [WorkspaceTabID] = [],
                reorderedGroups: [PaneGroupID] = [],
                insertedGroups: [PaneGroupID] = [],
                removedGroups: [PaneGroupID] = [],
                insertedSplits: [SplitID] = [],
                collapsedSplits: [SplitID] = [],
                activeGroupChanged: PaneGroupID? = nil,
                activeTabChanges: [PaneGroupID: WorkspaceTabID?] = [:],
                preferredFractionChanges: [SplitID: Double] = [:],
                isStructural: Bool = false) {
        self.insertedTabs = insertedTabs
        self.removedTabs = removedTabs
        self.movedTabs = movedTabs
        self.reorderedGroups = reorderedGroups
        self.insertedGroups = insertedGroups
        self.removedGroups = removedGroups
        self.insertedSplits = insertedSplits
        self.collapsedSplits = collapsedSplits
        self.activeGroupChanged = activeGroupChanged
        self.activeTabChanges = activeTabChanges
        self.preferredFractionChanges = preferredFractionChanges
        self.isStructural = isStructural
    }

    /// The one source of truth for the durability-relevant command classes.
    public static func isStructuralCommand(_ command: WorkspaceLayoutCommand) -> Bool {
        switch command {
        case .insertTab, .splitGroup, .moveTab, .closeTab:
            true
        case .activateTab, .activateGroup, .setPreferredFraction,
             .updateViewState, .renameTab:
            false
        }
    }
}

public struct WorkspaceLayoutTransition: Sendable, Equatable {
    public let layout: WorkspaceLayout
    public let delta: WorkspaceLayoutDelta
    public let focusIntent: FocusIntent

    public init(layout: WorkspaceLayout, delta: WorkspaceLayoutDelta,
                focusIntent: FocusIntent) {
        self.layout = layout
        self.delta = delta
        self.focusIntent = focusIntent
    }
}

