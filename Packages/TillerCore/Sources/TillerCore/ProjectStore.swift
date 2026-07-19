import Foundation
import TillerPersistence
import GRDB
import os
 
/// Persistence-only domain store. Git side effects (worktree add/remove on
/// disk) are composed by the caller — this actor never shells out, so every
/// test runs against an in-memory database.
public actor ProjectStore {
    private let database: AppDatabase
    private let logger = Logger(subsystem: "dev.tiller", category: "store")
 
    public init(database: AppDatabase) {
        self.database = database
    }
 
    public func loadAll() throws -> [Project] {
        try database.read { db in
            try ProjectRecord.fetchAll(db).compactMap { record in
                // ids are written by this store as UUID().uuidString;
                // non-parsing rows indicate external corruption and are skipped
                guard let id = UUID(uuidString: record.id) else {
                    logger.warning("loadAll: skipping ProjectRecord with non-UUID id '\(record.id)' — data corruption")
                    return nil
                }
                return Project(
                    id: id, name: record.name, rootPath: record.rootPath, colorHex: record.colorHex,
                    displayName: record.displayName, iconKind: IconKind(rawValue: record.iconKind) ?? .icon,
                    iconValue: record.iconValue, avatarImage: record.avatarImage,
                    defaultWorktreeBase: record.defaultWorktreeBase,
                    worktreeLocationOverride: record.worktreeLocationOverride
                )
            }
        }
    }
 
    public func worktrees(of projectId: UUID) throws -> [Worktree] {
        try database.read { db in
            try WorktreeRecord
                .filter(Column("projectId") == projectId.uuidString)
                .fetchAll(db)
                .compactMap { record in
                    // ids are written by this store as UUID().uuidString;
                    // non-parsing rows indicate external corruption and are skipped
                    guard let id = UUID(uuidString: record.id) else {
                        logger.warning("worktrees(of:): skipping WorktreeRecord with non-UUID id '\(record.id)' — data corruption")
                        return nil
                    }
                    return Worktree(
                        id: id,
                        projectId: projectId,
                        branch: record.branch,
                        path: record.path,
                        comment: record.comment,
                        commentUpdatedAt: record.commentUpdatedAt,
                        isPrimary: record.isPrimary
                    )
                }
        }
    }
 
    public func addProject(name: String, rootPath: String) throws -> Project {
        let project = Project(id: UUID(), name: name, rootPath: rootPath)
        try database.write { db in
            try ProjectRecord(
                id: project.id.uuidString, name: name,
                rootPath: rootPath, createdAt: Date()
            ).insert(db)
        }
        return project
    }
 
    public func removeProject(_ id: UUID) throws {
        try database.write { db in
            _ = try ProjectRecord.deleteOne(db, key: id.uuidString)
        }
    }

    public func setProjectDisplayName(_ id: UUID, displayName: String?) throws {
        try database.write { db in
            try db.execute(
                sql: "UPDATE project SET displayName = ? WHERE id = ?",
                arguments: [displayName, id.uuidString]
            )
        }
    }

    public func setProjectColor(_ id: UUID, colorHex: String?) throws {
        try database.write { db in
            try db.execute(
                sql: "UPDATE project SET colorHex = ? WHERE id = ?",
                arguments: [colorHex, id.uuidString]
            )
        }
    }

    /// Sets kind, value, and avatar image together so switching icon kinds
    /// never leaves a stale value from the previous kind behind.
    public func setProjectIcon(_ id: UUID, kind: IconKind, value: String?, avatarImage: Data?) throws {
        try database.write { db in
            try db.execute(
                sql: "UPDATE project SET iconKind = ?, iconValue = ?, avatarImage = ? WHERE id = ?",
                arguments: [kind.rawValue, value, avatarImage, id.uuidString]
            )
        }
    }

    public func setProjectWorktreeBase(_ id: UUID, branch: String?) throws {
        try database.write { db in
            try db.execute(
                sql: "UPDATE project SET defaultWorktreeBase = ? WHERE id = ?",
                arguments: [branch, id.uuidString]
            )
        }
    }

    public func setProjectWorktreeLocation(_ id: UUID, path: String?) throws {
        try database.write { db in
            try db.execute(
                sql: "UPDATE project SET worktreeLocationOverride = ? WHERE id = ?",
                arguments: [path, id.uuidString]
            )
        }
    }

    public func addWorktree(projectId: UUID, branch: String, path: String) throws -> Worktree {
        let worktree = Worktree(id: UUID(), projectId: projectId, branch: branch, path: path)
        try database.write { db in
            try WorktreeRecord(
                id: worktree.id.uuidString, projectId: projectId.uuidString,
                branch: branch, path: path, createdAt: Date()
            ).insert(db)
        }
        return worktree
    }
 
    public func removeWorktree(_ id: UUID) throws {
        try database.write { db in
            _ = try WorktreeRecord.deleteOne(db, key: id.uuidString)
        }
    }
 
    public func setWorktreeComment(_ id: UUID, comment: String) throws {
        try database.write { db in
            try db.execute(
                sql: "UPDATE worktree SET comment = ?, commentUpdatedAt = ? WHERE id = ?",
                arguments: [comment, Date(), id.uuidString]
            )
        }
    }


    /// Fixes the main-checkout row after an in-app `git init`, whose real
    /// default branch may differ from the "main" fallback stored at add time.
    public func setWorktreeBranch(_ id: UUID, branch: String) throws {
        try database.write { db in
            try db.execute(
                sql: "UPDATE worktree SET branch = ? WHERE id = ?",
                arguments: [branch, id.uuidString]
            )
        }
    }
 
    /// Primary is exclusive within a project: setting true clears siblings.
    public func setWorktreePrimary(_ id: UUID, isPrimary: Bool) throws {
        try database.write { db in
            guard let record = try WorktreeRecord.fetchOne(db, key: id.uuidString) else { return }
            if isPrimary {
                try db.execute(
                    sql: "UPDATE worktree SET isPrimary = 0 WHERE projectId = ?",
                    arguments: [record.projectId]
                )
            }
            try db.execute(
                sql: "UPDATE worktree SET isPrimary = ? WHERE id = ?",
                arguments: [isPrimary, id.uuidString]
            )
        }
    }
 
    public func worktree(byPath path: String) throws -> Worktree? {
        try database.read { db in
            guard let record = try WorktreeRecord
                .filter(Column("path") == path).fetchOne(db),
                let id = UUID(uuidString: record.id),
                let projectId = UUID(uuidString: record.projectId)
            else { return nil }
            return Worktree(
                id: id, projectId: projectId, branch: record.branch, path: record.path,
                comment: record.comment, commentUpdatedAt: record.commentUpdatedAt,
                isPrimary: record.isPrimary
            )
        }
    }

    /// Riscrive l'intera lista tab del worktree (delete + insert): poche
    /// righe, elimina la sincronizzazione incrementale DB↔memoria.
    public func saveTabs(worktreeId: UUID, tabs: [WorkspaceTab], activeTabId: UUID?) throws {
        let encoder = JSONEncoder()
        try database.write { db in
            try db.execute(
                sql: "DELETE FROM terminalTab WHERE worktreeId = ?",
                arguments: [worktreeId.uuidString]
            )
            for (idx, tab) in tabs.enumerated() {
                let record: TerminalTabRecord
                switch tab.content {
                case .terminal(let tree):
                    let treeJSON = String(decoding: try encoder.encode(tree), as: UTF8.self)
                    record = TerminalTabRecord(
                        id: tab.id.uuidString, worktreeId: worktreeId.uuidString,
                        title: tab.title, orderIdx: idx,
                        isActive: tab.id == activeTabId,
                        treeJSON: treeJSON, updatedAt: Date()
                    )
                case .markdown(let fileURL):
                    // treeJSON resta vuoto perché la colonna è notNull dalla v3.
                    record = TerminalTabRecord(
                        id: tab.id.uuidString, worktreeId: worktreeId.uuidString,
                        title: tab.title, orderIdx: idx,
                        isActive: tab.id == activeTabId,
                        treeJSON: "", updatedAt: Date(),
                        kind: "markdown", filePath: fileURL.path
                    )
                case .chat(let agentId):
                    record = TerminalTabRecord(
                        id: tab.id.uuidString, worktreeId: worktreeId.uuidString,
                        title: tab.title, orderIdx: idx,
                        isActive: tab.id == activeTabId,
                        treeJSON: "", updatedAt: Date(),
                        kind: "chat", chatAgentId: agentId
                    )
                }
                try record.insert(db)
            }
        }
    }

    public func loadTabs(of worktreeId: UUID) throws -> (tabs: [WorkspaceTab], activeTabId: UUID?) {
        let decoder = JSONDecoder()
        return try database.read { db in
            let records = try TerminalTabRecord
                .filter(Column("worktreeId") == worktreeId.uuidString)
                .order(Column("orderIdx"))
                .fetchAll(db)
            var tabs: [WorkspaceTab] = []
            var active: UUID?
            for record in records {
                // ids/treeJSON scritti da questo store; righe non parsabili = corruzione esterna
                guard let id = UUID(uuidString: record.id) else {
                    logger.warning("loadTabs: skipping corrupt TerminalTabRecord '\(record.id)'")
                    continue
                }
                switch record.kind {
                case "markdown":
                    guard let path = record.filePath else {
                        logger.warning("loadTabs: markdown tab '\(record.id)' senza filePath — skip")
                        continue
                    }
                    tabs.append(WorkspaceTab(id: id, title: record.title,
                                             content: .markdown(fileURL: URL(fileURLWithPath: path))))
                case "chat":
                    guard let agentId = record.chatAgentId else { continue }
                    tabs.append(WorkspaceTab(
                        id: id, title: record.title,
                        content: .chat(agentId: agentId)))
                default:
                    guard let tree = try? decoder.decode(SplitTree.self, from: Data(record.treeJSON.utf8)) else {
                        logger.warning("loadTabs: skipping corrupt TerminalTabRecord '\(record.id)'")
                        continue
                    }
                    tabs.append(WorkspaceTab(id: id, title: record.title, tree: tree))
                }
                if record.isActive { active = id }
            }
            return (tabs, active)
        }
    }

    // MARK: - Agent session refs (v6)

    /// Upserts the native session reference reported by an agent hook for a
    /// pane. Written live (not at quit) so refs survive a crash.
    public func saveAgentSessionRef(paneId: UUID, worktreeId: UUID,
                                    agentId: String, sessionRef: String) throws {
        try database.write { db in
            try AgentSessionRecord(
                paneId: paneId.uuidString, worktreeId: worktreeId.uuidString,
                agentId: agentId, sessionRef: sessionRef, capturedAt: Date()
            ).save(db)
        }
    }

    public func agentSessionRefs(of worktreeId: UUID) throws -> [AgentSessionRef] {
        try database.read { db in
            try AgentSessionRecord
                .filter(Column("worktreeId") == worktreeId.uuidString)
                .fetchAll(db)
                .compactMap { record in
                    // paneId scritto da questo store come UUID().uuidString;
                    // righe non parsabili = corruzione esterna
                    guard let paneId = UUID(uuidString: record.paneId) else {
                        logger.warning("agentSessionRefs: skipping corrupt AgentSessionRecord '\(record.paneId)'")
                        return nil
                    }
                    return AgentSessionRef(paneId: paneId, agentId: record.agentId,
                                           sessionRef: record.sessionRef)
                }
        }
    }

    public func deleteAgentSessionRef(paneId: UUID) throws {
        try database.write { db in
            _ = try AgentSessionRecord.deleteOne(db, key: paneId.uuidString)
        }
    }
}
