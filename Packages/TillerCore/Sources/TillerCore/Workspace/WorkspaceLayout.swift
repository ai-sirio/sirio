import Foundation

public enum WorkspaceSplitAxis: String, Sendable, Codable {
    /// Children sit side by side: `first` is left, `second` is right.
    case horizontal
    /// Children stack: `first` is above, `second` is below.
    case vertical
}

public indirect enum LayoutNode: Equatable, Sendable, Codable {
    case group(PaneGroupID)
    case split(id: SplitID, axis: WorkspaceSplitAxis, fraction: Double,
               first: LayoutNode, second: LayoutNode)
}

public struct PaneGroup: Identifiable, Equatable, Sendable, Codable {
    public let id: PaneGroupID
    public var tabs: [WorkspaceTab]
    public var activeTabID: WorkspaceTabID?

    public init(id: PaneGroupID, tabs: [WorkspaceTab], activeTabID: WorkspaceTabID?) {
        self.id = id
        self.tabs = tabs
        self.activeTabID = activeTabID
    }
}

public struct WorkspaceLayout: Equatable, Sendable {
    public private(set) var root: LayoutNode
    public private(set) var groups: [PaneGroupID: PaneGroup]
    public private(set) var activeGroupID: PaneGroupID

    internal init(root: LayoutNode, groups: [PaneGroupID: PaneGroup], activeGroupID: PaneGroupID) {
        self.root = root
        self.groups = groups
        self.activeGroupID = activeGroupID
    }

    /// The only public way to build a layout. Rejects any value that would
    /// violate the invariants from #2.
    public static func make(root: LayoutNode, groups: [PaneGroupID: PaneGroup],
                            activeGroupID: PaneGroupID)
        -> Result<WorkspaceLayout, WorkspaceLayoutError> {
        guard let error = WorkspaceLayoutInvariants.validate(
            root: root, groups: groups, activeGroupID: activeGroupID
        ) else {
            return .success(WorkspaceLayout(root: root, groups: groups, activeGroupID: activeGroupID))
        }
        return .failure(error)
    }

    /// Valid empty layout for a worktree with no content.
    public static func empty(groupID: PaneGroupID = PaneGroupID()) -> WorkspaceLayout {
        let groups = [groupID: PaneGroup(id: groupID, tabs: [], activeTabID: nil)]
        guard case .success(let layout) = make(
            root: .group(groupID), groups: groups, activeGroupID: groupID
        ) else {
            preconditionFailure("the empty workspace layout must be valid")
        }
        return layout
    }

    // Read-only queries used by renderer/persistence.
    public func group(_ id: PaneGroupID) -> PaneGroup? {
        groups[id]
    }

    public func groupContaining(tab id: WorkspaceTabID) -> PaneGroupID? {
        orderedGroupIDs.first { groupID in
            groups[groupID]?.tabs.contains { $0.id == id } == true
        }
    }

    public func tab(_ id: WorkspaceTabID) -> WorkspaceTab? {
        guard let groupID = groupContaining(tab: id) else { return nil }
        return groups[groupID]?.tabs.first { $0.id == id }
    }

    public var orderedGroupIDs: [PaneGroupID] {
        Self.orderedGroupIDs(in: root)
    }

    public var allTabs: [WorkspaceTab] {
        orderedGroupIDs.flatMap { groups[$0]?.tabs ?? [] }
    }

    public func splitIDs() -> [SplitID] {
        Self.splitIDs(in: root)
    }

    public func preferredFraction(for split: SplitID) -> Double? {
        Self.preferredFraction(for: split, in: root)
    }

    private static func orderedGroupIDs(in node: LayoutNode) -> [PaneGroupID] {
        switch node {
        case .group(let id):
            [id]
        case .split(_, _, _, let first, let second):
            orderedGroupIDs(in: first) + orderedGroupIDs(in: second)
        }
    }

    private static func splitIDs(in node: LayoutNode) -> [SplitID] {
        switch node {
        case .group:
            []
        case .split(let id, _, _, let first, let second):
            [id] + splitIDs(in: first) + splitIDs(in: second)
        }
    }

    private static func preferredFraction(for split: SplitID, in node: LayoutNode) -> Double? {
        switch node {
        case .group:
            return nil
        case .split(let id, _, let fraction, let first, let second):
            if id == split { return fraction }
            return preferredFraction(for: split, in: first) ?? preferredFraction(for: split, in: second)
        }
    }
}
