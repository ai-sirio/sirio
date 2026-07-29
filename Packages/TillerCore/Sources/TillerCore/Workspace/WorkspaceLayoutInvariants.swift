import Foundation

public enum WorkspaceLayoutInvariants {
    static func validate(root: LayoutNode, groups: [PaneGroupID: PaneGroup],
                         activeGroupID: PaneGroupID) -> WorkspaceLayoutError? {
        guard !groups.isEmpty else { return .emptyGroupRegistry }

        var leafIDs: [PaneGroupID] = []
        var splitIDs: [SplitID] = []
        var fractions: [(SplitID, Double)] = []

        func collect(_ node: LayoutNode) {
            switch node {
            case .group(let id):
                leafIDs.append(id)
            case .split(let id, _, let fraction, let first, let second):
                splitIDs.append(id)
                fractions.append((id, fraction))
                collect(first)
                collect(second)
            }
        }
        collect(root)

        for id in groups.keys.sorted(by: { $0.rawValue.uuidString < $1.rawValue.uuidString })
        where !leafIDs.contains(id) {
            return .orphanGroup(id)
        }
        for id in leafIDs where groups[id] == nil {
            return .unresolvedGroupLeaf(id)
        }

        var seenGroups = Set<PaneGroupID>()
        for id in leafIDs {
            guard seenGroups.insert(id).inserted else {
                return .duplicateID("group:\(id.rawValue.uuidString)")
            }
        }

        var seenSplits = Set<SplitID>()
        for id in splitIDs {
            guard seenSplits.insert(id).inserted else {
                return .duplicateID("split:\(id.rawValue.uuidString)")
            }
        }

        var seenTabs = Set<WorkspaceTabID>()
        var seenContent = Set<String>()
        for groupID in leafIDs {
            guard let group = groups[groupID] else { continue }
            for tab in group.tabs {
                guard seenTabs.insert(tab.id).inserted else {
                    return .duplicateID("tab:\(tab.id.rawValue.uuidString)")
                }
                let contentKey = "\(tab.content.kind.rawValue):\(tab.content.contentIdentifierString)"
                guard seenContent.insert(contentKey).inserted else {
                    return .duplicateContentOwnership(contentKey)
                }
            }
        }

        guard groups[activeGroupID] != nil else {
            return .unknownActiveGroup(activeGroupID)
        }

        let isSoleRootGroup: Bool = {
            if case .group(let id) = root { return id == leafIDs.first && leafIDs.count == 1 }
            return false
        }()

        for groupID in leafIDs {
            guard let group = groups[groupID] else { continue }
            if group.tabs.isEmpty {
                if !isSoleRootGroup {
                    return .emptyNonRootGroup(groupID)
                }
                if group.activeTabID != nil {
                    return .activeTabNotInGroup(groupID)
                }
                continue
            }
            guard let activeTabID = group.activeTabID,
                  group.tabs.contains(where: { $0.id == activeTabID }) else {
                return .activeTabNotInGroup(groupID)
            }
        }

        for (id, fraction) in fractions where !fraction.isFinite || !(0.0 < fraction && fraction < 1.0) {
            return .invalidFraction(id, fraction)
        }
        return nil
    }

    public static func validate(_ layout: WorkspaceLayout) -> WorkspaceLayoutError? {
        validate(root: layout.root, groups: layout.groups, activeGroupID: layout.activeGroupID)
    }
}
