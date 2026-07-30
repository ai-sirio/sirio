import Foundation
import TillerCore

/// Converts the polymorphic v15 `terminalTab` rows into the universal workspace
/// representation. This type deliberately has no persistence or filesystem
/// dependency: the database adapter owns fetching and writing rows, while this
/// function owns only the deterministic value transformation.
enum WorkspaceMigrationV15 {
    struct LegacyTabRow: Sendable, Equatable {
        let id: UUID
        let worktreeId: UUID
        let title: String
        let orderIdx: Int
        let isActive: Bool
        let treeJSON: String
        let kind: String
        let filePath: String?
        let chatAgentId: String?
        let chatSessionId: String?
        let titleIsAutoNamed: Bool
    }

    struct MigrationResult: Sendable {
        let layout: WorkspaceLayout
        let tabs: [WorkspaceTab]
        let terminalContents: [TerminalContentRecordValue]
        let diagnostics: [RestoreDiagnostic]
    }

    static func migrate(
        rows: [LegacyTabRow], worktreeID: UUID,
        freshTabID: () -> WorkspaceTabID,
        freshGroupID: () -> PaneGroupID,
        freshSplitID: () -> SplitID,
        freshChatID: () -> ChatContentID,
        agentDisplayName: (String) -> String?
    ) -> MigrationResult {
        var diagnostics: [RestoreDiagnostic] = []
        var terminalTitleIndex = 0
        var groups: [PaneGroupID: PaneGroup] = [:]
        var terminalContents: [TerminalContentRecordValue] = []

        let scopedRows = rows
            .filter { $0.worktreeId == worktreeID }
            .sorted { lhs, rhs in
                if lhs.orderIdx != rhs.orderIdx { return lhs.orderIdx < rhs.orderIdx }
                return lhs.id.uuidString < rhs.id.uuidString
            }

        struct ParsedRow {
            let row: LegacyTabRow
            let tree: SplitTree?
            let valid: Bool
        }

        var parsed: [ParsedRow] = []
        for row in scopedRows {
            switch row.kind {
            case "terminal":
                guard let tree = decodeTree(row.treeJSON) else {
                    diagnostics.append(.quarantinedSnapshot(reason: "corrupt-tree:\(row.id.uuidString)"))
                    parsed.append(ParsedRow(row: row, tree: nil, valid: false))
                    continue
                }
                parsed.append(ParsedRow(row: row, tree: tree, valid: true))
            case "markdown", "code":
                guard row.filePath != nil else {
                    diagnostics.append(.quarantinedSnapshot(reason: "corrupt-document:\(row.id.uuidString)"))
                    parsed.append(ParsedRow(row: row, tree: nil, valid: false))
                    continue
                }
                parsed.append(ParsedRow(row: row, tree: nil, valid: true))
            case "chat":
                guard row.chatAgentId != nil else {
                    diagnostics.append(.quarantinedSnapshot(reason: "corrupt-chat:\(row.id.uuidString)"))
                    parsed.append(ParsedRow(row: row, tree: nil, valid: false))
                    continue
                }
                parsed.append(ParsedRow(row: row, tree: nil, valid: true))
            default:
                diagnostics.append(.quarantinedSnapshot(reason: "corrupt-kind:\(row.id.uuidString)"))
                parsed.append(ParsedRow(row: row, tree: nil, valid: false))
            }
        }

        let selectedLegacy = parsed.first { $0.valid && $0.row.isActive }
        let activeTerminal = selectedLegacy?.row.kind == "terminal" ? selectedLegacy : nil
        var primaryGroupID: PaneGroupID?
        let root: LayoutNode
        var primaryTabs: [WorkspaceTab] = []
        var selectedTabID: WorkspaceTabID?
        var migratedEntryIDs: [UUID: WorkspaceTabID] = [:]

        func terminalTitle(for row: LegacyTabRow) -> String {
            if let displayName = agentDisplayName(row.title) { return displayName }
            terminalTitleIndex += 1
            return "Terminale \(terminalTitleIndex)"
        }

        func terminalLaunchKind(for row: LegacyTabRow) -> TerminalContentRecordValue.LaunchKind {
            if agentDisplayName(row.title) != nil { return .agent(id: row.title) }
            return .shell
        }

        func makeTerminalTab(
            row: LegacyTabRow, leafID: UUID, tabID: WorkspaceTabID, isPrimaryLeaf: Bool
        ) -> WorkspaceTab {
            let title = isPrimaryLeaf ? row.title : terminalTitle(for: row)
            let contentID = TerminalContentID(leafID)
            terminalContents.append(TerminalContentRecordValue(
                id: contentID, worktreeID: worktreeID,
                launchKind: terminalLaunchKind(for: row), commandJSON: nil))
            return WorkspaceTab(
                id: tabID, title: title, titleIsAutoNamed: isPrimaryLeaf ? row.titleIsAutoNamed : true,
                content: .terminal(contentID))
        }

        func buildTopology(
            _ tree: SplitTree, row: LegacyTabRow, primaryLeaf: inout Bool
        ) -> LayoutNode {
            switch tree {
            case .leaf(let leafID):
                let groupID = freshGroupID()
                let tabID = primaryLeaf ? WorkspaceTabID(row.id) : freshTabID()
                let tab = makeTerminalTab(row: row, leafID: leafID, tabID: tabID, isPrimaryLeaf: primaryLeaf)
                groups[groupID] = PaneGroup(id: groupID, tabs: [tab], activeTabID: tabID)
                if primaryLeaf {
                    primaryGroupID = groupID
                    primaryTabs = [tab]
                    selectedTabID = tabID
                    primaryLeaf = false
                }
                return .group(groupID)
            case .split(let axis, let firstTree, let secondTree):
                let splitID = freshSplitID()
                let first = buildTopology(firstTree, row: row, primaryLeaf: &primaryLeaf)
                let second = buildTopology(secondTree, row: row, primaryLeaf: &primaryLeaf)
                let workspaceAxis: WorkspaceSplitAxis = axis == .horizontal ? .horizontal : .vertical
                return .split(id: splitID, axis: workspaceAxis, fraction: 0.5, first: first, second: second)
            }
        }

        if let activeTerminal, let tree = activeTerminal.tree {
            var primaryLeaf = true
            root = buildTopology(tree, row: activeTerminal.row, primaryLeaf: &primaryLeaf)
            // The first depth-first leaf assigns the primary group.
        } else {
            let groupID = freshGroupID()
            primaryGroupID = groupID
            root = .group(groupID)
            groups[groupID] = PaneGroup(id: groupID, tabs: [], activeTabID: nil)
        }

        let primaryGroup = primaryGroupID!

        if let activeTerminal {
            migratedEntryIDs[activeTerminal.row.id] = selectedTabID!
        }

        let otherRows = parsed.filter { $0.valid && $0.row.id != activeTerminal?.row.id }
        var otherEntries: [(row: LegacyTabRow, tabs: [WorkspaceTab], order: Int)] = []
        for parsedRow in otherRows {
            let row = parsedRow.row
            switch row.kind {
            case "terminal":
                guard let tree = parsedRow.tree else { continue }
                let leaves = tree.leafIds.map { leafID in
                    makeTerminalTab(row: row, leafID: leafID, tabID: freshTabID(), isPrimaryLeaf: false)
                }
                otherEntries.append((row: row, tabs: leaves, order: row.orderIdx))
            case "markdown", "code":
                guard let path = row.filePath else { continue }
                let editor: DocumentEditorKind = row.kind == "markdown" ? .markdown : .code
                let documentID = DocumentID.makeCanonical(worktreeID: worktreeID, path: path)
                let tab = WorkspaceTab(
                    id: WorkspaceTabID(row.id), title: row.title, titleIsAutoNamed: row.titleIsAutoNamed,
                    content: .document(documentID, editor: editor))
                migratedEntryIDs[row.id] = tab.id
                otherEntries.append((row: row, tabs: [tab], order: row.orderIdx))
            case "chat":
                guard row.chatAgentId != nil else { continue }
                let chatID = row.chatSessionId.map(ChatContentID.init) ?? freshChatID()
                let tab = WorkspaceTab(
                    id: WorkspaceTabID(row.id), title: row.title, titleIsAutoNamed: row.titleIsAutoNamed,
                    content: .chat(ChatContentID(chatID.rawValue)))
                migratedEntryIDs[row.id] = tab.id
                otherEntries.append((row: row, tabs: [tab], order: row.orderIdx))
            default:
                continue
            }
        }

        var flattened = otherEntries.sorted { lhs, rhs in
            if lhs.order != rhs.order { return lhs.order < rhs.order }
            return lhs.row.id.uuidString < rhs.row.id.uuidString
        }.flatMap(\.tabs)

        // The database has a unique document identity constraint. Resolve
        // duplicates before materializing the layout, preferring an active row
        // and then the earliest legacy order.
        let documentEntries = otherEntries.flatMap { entry in
            entry.tabs.compactMap { tab -> (tab: WorkspaceTab, row: LegacyTabRow)? in
                guard case .document = tab.content else { return nil }
                return (tab, entry.row)
            }
        }
        var winners: [String: (tab: WorkspaceTab, row: LegacyTabRow)] = [:]
        for entry in documentEntries {
            let key = entry.tab.content.contentIdentifierString
            guard let current = winners[key] else {
                winners[key] = entry
                continue
            }
            let entryWins = (entry.row.isActive && !current.row.isActive)
                || (entry.row.isActive == current.row.isActive && entry.row.orderIdx < current.row.orderIdx)
            let winner = entryWins ? entry : current
            let removed = entryWins ? current.tab : entry.tab
            winners[key] = winner
            diagnostics.append(.quarantinedSnapshot(reason: "duplicate-document:\(removed.id.rawValue.uuidString)"))
        }
        flattened = flattened.filter { tab in
            guard case .document = tab.content else { return true }
            return winners[tab.content.contentIdentifierString]?.tab.id == tab.id
        }

        if !flattened.isEmpty {
            primaryTabs.append(contentsOf: flattened)
        }
        if let primary = groups[primaryGroup] {
            let active = primary.activeTabID ?? selectedTabID ?? primaryTabs.first?.id
            groups[primaryGroup] = PaneGroup(id: primaryGroup, tabs: primaryTabs, activeTabID: active)
        } else {
            let active = selectedTabID ?? primaryTabs.first?.id
            groups[primaryGroup] = PaneGroup(id: primaryGroup, tabs: primaryTabs, activeTabID: active)
        }

        if selectedLegacy?.row.kind != "terminal" {
            selectedTabID = selectedLegacy.flatMap { migratedEntryIDs[$0.row.id] }
                ?? primaryTabs.first?.id
        }
        let orderedGroupIDs = groupIDs(in: root)
        let activeGroupID = selectedTabID.flatMap { tabID in
            orderedGroupIDs.first { groups[$0]?.tabs.contains { $0.id == tabID } == true }
        } ?? primaryGroup
        let finalGroups = groups.mapValues { group in
            if group.id == activeGroupID, group.activeTabID == nil, let first = group.tabs.first {
                return PaneGroup(id: group.id, tabs: group.tabs, activeTabID: first.id)
            }
            return group
        }
        let layout = try! WorkspaceLayout.make(
            root: root, groups: finalGroups, activeGroupID: activeGroupID).get()
        return MigrationResult(
            layout: layout, tabs: layout.allTabs, terminalContents: terminalContents,
            diagnostics: diagnostics)
    }

    private static func decodeTree(_ json: String) -> SplitTree? {
        try? JSONDecoder().decode(SplitTree.self, from: Data(json.utf8))
    }

    private static func groupIDs(in node: LayoutNode) -> [PaneGroupID] {
        switch node {
        case .group(let id): return [id]
        case .split(_, _, _, let first, let second): return groupIDs(in: first) + groupIDs(in: second)
        }
    }
}
