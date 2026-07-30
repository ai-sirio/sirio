import Foundation

public enum SplitPlacementSide: String, Sendable, Codable, Equatable {
    case right
    case down
}

public enum SplitContentPayload: Sendable, Equatable {
    case newTab(WorkspaceTab)
    case existingTab(WorkspaceTabID)
}

public enum MoveDestination: Sendable, Equatable {
    case group(PaneGroupID, index: Int)
    case newSplit(anchor: PaneGroupID, placement: SplitPlacementSide,
                  newGroup: PaneGroupID, newSplit: SplitID)
}

public enum WorkspaceLayoutCommand: Sendable, Equatable {
    case insertTab(WorkspaceTab, into: PaneGroupID, index: Int?, activate: Bool)
    case splitGroup(anchor: PaneGroupID, placement: SplitPlacementSide,
                    newGroup: PaneGroupID, newSplit: SplitID,
                    content: SplitContentPayload)
    case moveTab(WorkspaceTabID, to: MoveDestination)
    case closeTab(WorkspaceTabID)
    case activateTab(WorkspaceTabID)
    case activateGroup(PaneGroupID)
    case setPreferredFraction(SplitID, Double)
    case updateViewState(WorkspaceTabID, WorkspaceTabViewState)
    case renameTab(WorkspaceTabID, title: String, isAutoNamed: Bool)
}

