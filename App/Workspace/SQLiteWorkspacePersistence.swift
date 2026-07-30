import Foundation
import GRDB
import TillerCore
import TillerPersistence

final class SQLiteWorkspacePersistence: WorkspaceLayoutPersistence, @unchecked Sendable {
    private let database: AppDatabase
    private let now: @Sendable () -> Date
    private let recoveryDirectory: URL?

    init(database: AppDatabase, now: @escaping @Sendable () -> Date = Date.init,
         recoveryDirectory: URL? = nil) {
        self.database = database
        self.now = now
        self.recoveryDirectory = recoveryDirectory
    }

    func commitStructural(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot,
                          tabs: [WorkspaceTab], terminalContents: [TerminalContentRecordValue]) async throws {
        guard case .success = snapshot.materialize() else {
            throw WorkspacePersistenceError.invalidSnapshot
        }
        let tabIDs = Set(snapshot.groups.flatMap(\.tabIDs))
        let suppliedTabs = Dictionary(tabs.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        guard suppliedTabs.count == tabs.count, Set(suppliedTabs.keys) == tabIDs else {
            throw WorkspacePersistenceError.invalidSnapshot
        }
        let suppliedContents = Dictionary(terminalContents.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        guard suppliedContents.count == terminalContents.count else {
            throw WorkspacePersistenceError.invalidSnapshot
        }
        for tab in tabs {
            guard case .terminal(let contentID) = tab.content else { continue }
            guard let content = suppliedContents[contentID] else {
                throw WorkspacePersistenceError.missingTerminalContent(contentID)
            }
            guard content.worktreeID == worktreeID else {
                throw WorkspacePersistenceError.contentBelongsToAnotherWorktree(contentID)
            }
        }

        let payload = try snapshot.canonicalPayload()
        let payloadString = String(decoding: payload, as: UTF8.self)
        let checksum = SHA256Hex.digest(payload)
        let timestamp = now()

        try database.write { db in
            if let existing = try WorkspaceLayoutRecord.fetchOne(db, key: worktreeID.uuidString),
               existing.revision >= revision {
                return
            }
            try WorkspaceTabRecord
                .filter(Column("worktreeId") == worktreeID.uuidString)
                .deleteAll(db)
            try TerminalContentRecord
                .filter(Column("worktreeId") == worktreeID.uuidString)
                .deleteAll(db)

            for tab in tabs {
                let viewStateJSON = String(decoding: try JSONEncoder().encode(tab.viewState), as: UTF8.self)
                try WorkspaceTabRecord(
                    id: tab.id.rawValue.uuidString, worktreeId: worktreeID.uuidString,
                    title: tab.title, titleIsAutoNamed: tab.titleIsAutoNamed,
                    contentKind: tab.content.kind.rawValue,
                    contentId: tab.content.contentIdentifierString,
                    viewStateJSON: viewStateJSON, viewStateVersion: 1,
                    createdAt: timestamp).insert(db)
            }
            for content in terminalContents {
                let launchKind: String
                let agentID: String?
                switch content.launchKind {
                case .shell:
                    launchKind = "shell"
                    agentID = nil
                case .agent(let id):
                    launchKind = "agent"
                    agentID = id
                }
                try TerminalContentRecord(
                    id: content.id.rawValue.uuidString, worktreeId: worktreeID.uuidString,
                    launchKind: launchKind, agentId: agentID,
                    commandJSON: content.commandJSON, createdAt: timestamp).insert(db)
            }
            try WorkspaceLayoutRecord(
                worktreeId: worktreeID.uuidString, schemaVersion: snapshot.schemaVersion,
                revision: revision, payload: payloadString, checksum: checksum,
                updatedAt: timestamp).save(db)
        }
    }

    func restore(worktreeID: UUID) async -> RestoredWorkspace {
        do {
            let record = try database.read({
                try WorkspaceLayoutRecord.fetchOne($0, key: worktreeID.uuidString)
            })
            if let sidecar = try validNewerSidecar(worktreeID: worktreeID, storedRevision: record?.revision ?? 0) {
                let importedRecord = WorkspaceLayoutRecord(
                    worktreeId: worktreeID.uuidString, schemaVersion: sidecar.schemaVersion,
                    revision: sidecar.revision, payload: sidecar.payload,
                    checksum: sidecar.checksum, updatedAt: now())
                try saveLayoutRecord(importedRecord)
                guard case .success(let snapshot) = WorkspaceSnapshotUpgrader.upgrade(
                    sidecar.payloadData, from: sidecar.schemaVersion) else {
                    throw WorkspaceRecoverySidecarError.invalidChecksum
                }
                let restored = try restore(snapshot: snapshot, worktreeID: worktreeID, record: importedRecord)
                return RestoredWorkspace(
                    layout: restored.layout, tabs: restored.tabs, revision: restored.revision,
                    diagnostics: [.importedRecoverySidecar(revision: sidecar.revision)] + restored.diagnostics)
            }
            guard let record else {
                return RestoredWorkspace(layout: .empty(), tabs: [:], revision: 0, diagnostics: [])
            }
            let payload = Data(record.payload.utf8)
            guard SHA256Hex.digest(payload) == record.checksum else {
                return try salvage(worktreeID: worktreeID, record: record, reason: "checksum")
            }
            switch WorkspaceSnapshotUpgrader.upgrade(payload, from: record.schemaVersion) {
            case .failure(.unsupportedFutureVersion(let version)):
                return RestoredWorkspace(
                    layout: .empty(), tabs: [:], revision: record.revision,
                    diagnostics: [.futureSchemaVersion(version)])
            case .failure:
                return try salvage(worktreeID: worktreeID, record: record, reason: "decode")
            case .success(let snapshot):
                return try restore(snapshot: snapshot, worktreeID: worktreeID, record: record)
            }
        } catch {
            return RestoredWorkspace(layout: .empty(), tabs: [:], revision: 0, diagnostics: [])
        }
    }

    func checkpoint(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) async {
        guard case .success = snapshot.materialize(),
              let payload = try? snapshot.canonicalPayload() else { return }
        let timestamp = now()
        do {
            try database.write { db in
                guard let existing = try WorkspaceLayoutRecord.fetchOne(db, key: worktreeID.uuidString),
                      existing.revision < revision else { return }
                try WorkspaceLayoutRecord(
                    worktreeId: worktreeID.uuidString, schemaVersion: snapshot.schemaVersion,
                    revision: revision, payload: String(decoding: payload, as: UTF8.self),
                    checksum: SHA256Hex.digest(payload), updatedAt: timestamp).save(db)
            }
        } catch {
            // Optimistic checkpoints are deliberately best-effort. The queue
            // owns retry/dirty state and calls flush before quit.
        }
    }

    func flush(worktreeID: UUID) async throws {}

    func writeRecoverySidecar(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) throws {
        try WorkspaceRecoverySidecar.write(worktreeID: worktreeID, revision: revision, snapshot: snapshot)
    }

    func purge(worktreeID: UUID) async throws {
        try database.write { db in
            try WorkspaceLayoutQuarantineRecord
                .filter(Column("worktreeId") == worktreeID.uuidString).deleteAll(db)
            try WorkspaceTabRecord
                .filter(Column("worktreeId") == worktreeID.uuidString).deleteAll(db)
            try TerminalContentRecord
                .filter(Column("worktreeId") == worktreeID.uuidString).deleteAll(db)
            try WorkspaceLayoutRecord.deleteOne(db, key: worktreeID.uuidString)
        }
    }

    private func restore(snapshot: WorkspaceSnapshot, worktreeID: UUID,
                         record: WorkspaceLayoutRecord) throws -> RestoredWorkspace {
        let rows = try database.read { db in
            try WorkspaceTabRecord
                .filter(Column("worktreeId") == worktreeID.uuidString)
                .fetchAll(db)
        }
        let contentRows = try database.read { db in
            try TerminalContentRecord
                .filter(Column("worktreeId") == worktreeID.uuidString)
                .fetchAll(db)
        }
        let rowsByID = Dictionary(uniqueKeysWithValues: rows.compactMap { row -> (WorkspaceTabID, WorkspaceTabRecord)? in
            guard let tabID = UUID(uuidString: row.id) else { return nil }
            return (WorkspaceTabID(tabID), row)
        })
        let tabsByID = Dictionary(uniqueKeysWithValues: rows.compactMap { row -> (WorkspaceTabID, WorkspaceTab)? in
            guard let tabID = UUID(uuidString: row.id),
                  let tab = makeTab(row: row, worktreeID: worktreeID, contents: contentRows) else { return nil }
            return (WorkspaceTabID(tabID), tab)
        })
        var diagnostics: [RestoreDiagnostic] = []
        var missing = Set<WorkspaceTabID>()
        for group in snapshot.groups {
            for tabID in group.tabIDs where tabsByID[tabID] == nil {
                missing.insert(tabID)
                if rowsByID[tabID] != nil {
                    diagnostics.append(.unavailableContent(tabID))
                } else {
                    diagnostics.append(.removedMissingTab(tabID))
                }
            }
        }

        let result = repairedLayout(snapshot: snapshot, tabs: tabsByID, missing: missing)
        diagnostics.append(contentsOf: result.diagnostics)
        let finalSnapshot = WorkspaceSnapshot(layout: result.layout)
        let finalRevision: Int
        if !missing.isEmpty || result.didRepairSelection {
            finalRevision = record.revision + 1
            try saveLayout(snapshot: finalSnapshot, worktreeID: worktreeID, revision: finalRevision)
        } else {
            finalRevision = record.revision
        }
        let referencedIDs = Set(snapshot.groups.flatMap(\.tabIDs))
        let referencedTabs = tabsByID.filter { referencedIDs.contains($0.key) }
        return RestoredWorkspace(layout: result.layout, tabs: referencedTabs,
                                 revision: finalRevision, diagnostics: deduplicate(diagnostics))
    }

    private func makeTab(row: WorkspaceTabRecord, worktreeID: UUID,
                         contents: [TerminalContentRecord]) -> WorkspaceTab? {
        let tabID = UUID(uuidString: row.id).map(WorkspaceTabID.init)
        guard let tabID else { return nil }
        let viewState = row.viewStateJSON.flatMap { data in
            try? JSONDecoder().decode(WorkspaceTabViewState.self, from: Data(data.utf8))
        } ?? .empty
        let content: WorkspaceContentRef
        switch row.contentKind {
        case WorkspaceContentKind.terminal.rawValue:
            guard let contentUUID = UUID(uuidString: row.contentId),
                  contents.contains(where: { UUID(uuidString: $0.id) == contentUUID && $0.worktreeId == worktreeID.uuidString }) else {
                return nil
            }
            content = .terminal(TerminalContentID(contentUUID))
        case WorkspaceContentKind.chat.rawValue:
            content = .chat(ChatContentID(row.contentId))
        case WorkspaceContentKind.document.rawValue:
            content = .document(
                DocumentID.make(worktreeID: worktreeID, fileURL: URL(fileURLWithPath: row.contentId)),
                editor: viewState.editorMode ?? .code)
        default:
            return nil
        }
        return WorkspaceTab(id: tabID, title: row.title, titleIsAutoNamed: row.titleIsAutoNamed,
                            content: content, viewState: viewState)
    }

    private func repairedLayout(snapshot: WorkspaceSnapshot, tabs: [WorkspaceTabID: WorkspaceTab],
                                missing: Set<WorkspaceTabID>)
        -> (layout: WorkspaceLayout, diagnostics: [RestoreDiagnostic], didRepairSelection: Bool) {
        var groups: [PaneGroupID: PaneGroup] = [:]
        var diagnostics: [RestoreDiagnostic] = []
        var repairedSelection = false
        for groupSnapshot in snapshot.groups {
            let retained = groupSnapshot.tabIDs.compactMap { tabs[$0] }
            let active = retained.contains { $0.id == groupSnapshot.activeTabID } ? groupSnapshot.activeTabID : retained.first?.id
            if active != groupSnapshot.activeTabID {
                repairedSelection = true
                diagnostics.append(.repairedSelection(groupSnapshot.id))
            }
            groups[groupSnapshot.id] = PaneGroup(id: groupSnapshot.id, tabs: retained, activeTabID: active)
        }

        func collapse(_ node: LayoutNode) -> LayoutNode? {
            switch node {
            case .group(let id):
                guard let group = groups[id], !group.tabs.isEmpty || snapshot.root == .group(id) else { return nil }
                return .group(id)
            case .split(let id, let axis, let fraction, let first, let second):
                let left = collapse(first)
                let right = collapse(second)
                switch (left, right) {
                case (nil, nil): return nil
                case (let child?, nil), (nil, let child?): return child
                case (let left?, let right?): return .split(id: id, axis: axis, fraction: fraction, first: left, second: right)
                }
            }
        }

        let root: LayoutNode
        if let collapsed = collapse(snapshot.root) {
            root = collapsed
        } else if let first = snapshot.groups.first {
            groups[first.id] = PaneGroup(id: first.id, tabs: [], activeTabID: nil)
            root = .group(first.id)
        } else {
            let id = PaneGroupID()
            root = .group(id)
            groups[id] = PaneGroup(id: id, tabs: [], activeTabID: nil)
        }
        let ordered: [PaneGroupID] = {
            func ids(_ node: LayoutNode) -> [PaneGroupID] {
                switch node {
                case .group(let id): return [id]
                case .split(_, _, _, let first, let second): return ids(first) + ids(second)
                }
            }
            return ids(root)
        }()
        let activeGroup = groups[snapshot.activeGroupID] != nil && ordered.contains(snapshot.activeGroupID)
            ? snapshot.activeGroupID : ordered[0]
        if activeGroup != snapshot.activeGroupID {
            repairedSelection = true
            diagnostics.append(.repairedSelection(snapshot.activeGroupID))
        }
        let liveGroups = Dictionary(uniqueKeysWithValues: ordered.compactMap { id in groups[id].map { (id, $0) } })
        let layout = try! WorkspaceLayout.make(root: root, groups: liveGroups, activeGroupID: activeGroup).get()
        return (layout, diagnostics, repairedSelection)
    }

    private func saveLayout(snapshot: WorkspaceSnapshot, worktreeID: UUID, revision: Int) throws {
        let payload = try snapshot.canonicalPayload()
        try database.write { db in
            try WorkspaceLayoutRecord(
                worktreeId: worktreeID.uuidString, schemaVersion: snapshot.schemaVersion,
                revision: revision, payload: String(decoding: payload, as: UTF8.self),
                checksum: SHA256Hex.digest(payload), updatedAt: now()).save(db)
        }
    }

    private func saveLayoutRecord(_ record: WorkspaceLayoutRecord) throws {
        try database.write { db in try record.save(db) }
    }

    private func validNewerSidecar(worktreeID: UUID, storedRevision: Int)
        throws -> WorkspaceRecoverySidecarEnvelope? {
        do {
            guard let sidecar = try WorkspaceRecoverySidecar.read(
                worktreeID: worktreeID, directory: recoveryDirectory),
                  sidecar.revision > storedRevision else { return nil }
            guard case .success(let snapshot) = WorkspaceSnapshotUpgrader.upgrade(
                sidecar.payloadData, from: sidecar.schemaVersion) else {
                try WorkspaceRecoverySidecar.quarantine(worktreeID: worktreeID, directory: recoveryDirectory)
                return nil
            }
            let tabIDs = Set(snapshot.groups.flatMap(\.tabIDs))
            let valid = try database.read { db in
                let rows = try WorkspaceTabRecord
                    .filter(Column("worktreeId") == worktreeID.uuidString).fetchAll(db)
                let contents = try TerminalContentRecord
                    .filter(Column("worktreeId") == worktreeID.uuidString).fetchAll(db)
                let validRows = rows.compactMap { row -> WorkspaceTabID? in
                    guard let id = UUID(uuidString: row.id), makeTab(row: row, worktreeID: worktreeID, contents: contents) != nil else { return nil }
                    return WorkspaceTabID(id)
                }
                return Set(validRows) == tabIDs
            }
            guard valid else {
                try WorkspaceRecoverySidecar.quarantine(worktreeID: worktreeID, directory: recoveryDirectory)
                return nil
            }
            return sidecar
        } catch WorkspaceRecoverySidecarError.invalidChecksum {
            try WorkspaceRecoverySidecar.quarantine(worktreeID: worktreeID, directory: recoveryDirectory)
            return nil
        } catch DecodingError.dataCorrupted, DecodingError.keyNotFound,
                DecodingError.typeMismatch, DecodingError.valueNotFound {
            try WorkspaceRecoverySidecar.quarantine(worktreeID: worktreeID, directory: recoveryDirectory)
            return nil
        }
    }

    private func salvage(worktreeID: UUID, record: WorkspaceLayoutRecord,
                         reason: String) throws -> RestoredWorkspace {
        let layout = WorkspaceLayout.empty()
        let snapshot = WorkspaceSnapshot(layout: layout)
        let payload = try snapshot.canonicalPayload()
        let quarantine = WorkspaceLayoutQuarantineRecord(
            id: UUID().uuidString, worktreeId: worktreeID.uuidString,
            payload: record.payload, reason: reason, createdAt: now())
        try database.write { db in
            try quarantine.insert(db)
            try db.execute(sql: """
                DELETE FROM workspaceLayoutQuarantine
                WHERE worktreeId = ? AND id NOT IN (
                    SELECT id FROM workspaceLayoutQuarantine
                    WHERE worktreeId = ? ORDER BY createdAt DESC, id DESC LIMIT 3
                )
                """, arguments: [worktreeID.uuidString, worktreeID.uuidString])
            try WorkspaceLayoutRecord(
                worktreeId: worktreeID.uuidString, schemaVersion: snapshot.schemaVersion,
                revision: record.revision + 1,
                payload: String(decoding: payload, as: UTF8.self),
                checksum: SHA256Hex.digest(payload), updatedAt: now()).save(db)
        }
        return RestoredWorkspace(
            layout: layout, tabs: [:], revision: record.revision + 1,
            diagnostics: [.quarantinedSnapshot(reason: reason)])
    }

    private func deduplicate(_ diagnostics: [RestoreDiagnostic]) -> [RestoreDiagnostic] {
        var seen = Set<RestoreDiagnostic>()
        return diagnostics.filter { seen.insert($0).inserted }
    }
}
