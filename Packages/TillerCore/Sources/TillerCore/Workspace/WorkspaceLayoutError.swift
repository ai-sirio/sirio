import Foundation

public enum WorkspaceLayoutError: Error, Equatable, Sendable {
    case emptyGroupRegistry
    case orphanGroup(PaneGroupID)
    case unresolvedGroupLeaf(PaneGroupID)
    case duplicateID(String)
    case emptyNonRootGroup(PaneGroupID)
    case unknownActiveGroup(PaneGroupID)
    case activeTabNotInGroup(PaneGroupID)
    case invalidFraction(SplitID, Double)
    case duplicateContentOwnership(String)
    case unknownGroup(PaneGroupID)
    case unknownTab(WorkspaceTabID)
    case illegalSplitOfSoleTab(PaneGroupID)
    case staleRevision(expected: Int, actual: Int)
}
