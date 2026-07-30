import Foundation

/// Edge the new content is inserted at, relative to the anchor group. Four
/// cases, not two: issue #10's `Move Tab To…` submenu exposes exact
/// destinations (`New Pane Left/Right/Above/Below`), and #3's Edge Preview
/// resolves the nearest of four edges — collapsing left into right (or top
/// into bottom) would silently split on the wrong side of the anchor.
public enum SplitPlacementSide: String, Sendable, Codable, Equatable, CaseIterable {
    case left
    case right
    case above
    case below

    public var axis: WorkspaceSplitAxis {
        switch self {
        case .left, .right: .horizontal
        case .above, .below: .vertical
        }
    }

    /// True when the anchor's existing content stays the split's first
    /// child and the new content becomes second; false when the new
    /// content takes the first-child slot and the anchor is pushed second.
    public var anchorIsFirstChild: Bool {
        switch self {
        case .right, .below: true
        case .left, .above: false
        }
    }
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

