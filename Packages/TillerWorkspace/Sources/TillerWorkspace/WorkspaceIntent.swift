import TillerCore

public enum WorkspaceIntentDestination: Sendable, Equatable {
    case group(PaneGroupID, index: Int)
    case edgeSplit(anchor: PaneGroupID, placement: SplitPlacementSide)
}

public enum WorkspaceIntent: Sendable, Equatable {
    case requestSplit(anchor: PaneGroupID, placement: SplitPlacementSide)
    case requestNewTab(into: PaneGroupID)
    case requestClose(WorkspaceTabID)
    case requestMove(WorkspaceTabID, to: WorkspaceIntentDestination)
    case activateTab(WorkspaceTabID)
    case activateGroup(PaneGroupID)
    case setPreferredFraction(SplitID, Double)
    case retryContent(WorkspaceTabID)
}

@MainActor
public protocol WorkspaceIntentSink: AnyObject {
    func send(_ intent: WorkspaceIntent)
}
