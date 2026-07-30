import Foundation

public struct WorkspaceSnapshot: Codable, Equatable, Sendable {
    public static let currentSchemaVersion = 1

    public let schemaVersion: Int
    public let root: LayoutNode
    public let groups: [PaneGroupSnapshot]
    public let activeGroupID: PaneGroupID

    public init(layout: WorkspaceLayout) {
        schemaVersion = Self.currentSchemaVersion
        root = layout.root
        groups = layout.orderedGroupIDs.compactMap { groupID in
            guard let group = layout.group(groupID) else { return nil }
            return PaneGroupSnapshot(
                id: group.id,
                tabIDs: group.tabs.map(\.id),
                activeTabID: group.activeTabID)
        }
        activeGroupID = layout.activeGroupID
    }

    /// Encodes only the durable workspace structure in its canonical form.
    public func canonicalPayload() throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return try encoder.encode(self)
    }

    public static func decode(_ payload: Data) -> Result<WorkspaceSnapshot, WorkspaceLayoutError> {
        do {
            let snapshot = try JSONDecoder().decode(Self.self, from: payload)
            switch snapshot.materialize() {
            case .success:
                return .success(snapshot)
            case .failure(let error):
                return .failure(error)
            }
        } catch {
            return .failure(.emptyGroupRegistry)
        }
    }

    /// Rebuilds a layout through `WorkspaceLayout.make`, so invalid structure is
    /// rejected before a caller can observe a partially coherent layout.
    public func materialize() -> Result<WorkspaceLayout, WorkspaceLayoutError> {
        var materializedGroups: [PaneGroupID: PaneGroup] = [:]
        for groupSnapshot in groups {
            guard materializedGroups[groupSnapshot.id] == nil else {
                return .failure(.duplicateID(
                    "group:\(groupSnapshot.id.rawValue.uuidString)"))
            }

            let tabs = groupSnapshot.tabIDs.map { tabID in
                WorkspaceTab(
                    id: tabID,
                    title: "",
                    titleIsAutoNamed: true,
                    content: .terminal(TerminalContentID(tabID.rawValue)))
            }
            materializedGroups[groupSnapshot.id] = PaneGroup(
                id: groupSnapshot.id,
                tabs: tabs,
                activeTabID: groupSnapshot.activeTabID)
        }

        return WorkspaceLayout.make(
            root: root,
            groups: materializedGroups,
            activeGroupID: activeGroupID)
    }
}

public struct PaneGroupSnapshot: Codable, Equatable, Sendable {
    public let id: PaneGroupID
    public let tabIDs: [WorkspaceTabID]
    public let activeTabID: WorkspaceTabID?

    public init(id: PaneGroupID, tabIDs: [WorkspaceTabID], activeTabID: WorkspaceTabID?) {
        self.id = id
        self.tabIDs = tabIDs
        self.activeTabID = activeTabID
    }
}

public enum WorkspaceSnapshotUpgrader {
    public static func upgrade(
        _ payload: Data,
        from version: Int
    ) -> Result<WorkspaceSnapshot, WorkspaceSnapshotUpgradeError> {
        guard version <= WorkspaceSnapshot.currentSchemaVersion else {
            return .failure(.unsupportedFutureVersion(version))
        }

        switch WorkspaceSnapshot.decode(payload) {
        case .success(let snapshot):
            if snapshot.schemaVersion > WorkspaceSnapshot.currentSchemaVersion {
                return .failure(.unsupportedFutureVersion(snapshot.schemaVersion))
            }
            guard snapshot.schemaVersion == WorkspaceSnapshot.currentSchemaVersion else {
                return .failure(.malformed(
                    "no upgrader for schema version \(snapshot.schemaVersion)"))
            }
            return .success(snapshot)
        case .failure(let error):
            return .failure(.malformed("invalid snapshot: \(String(describing: error))"))
        }
    }
}

public enum WorkspaceSnapshotUpgradeError: Error, Equatable, Sendable {
    case unsupportedFutureVersion(Int)
    case malformed(String)
}
