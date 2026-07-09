import Foundation
import GRDB

public struct ProjectRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "project"
    public var id: String
    public var name: String
    public var rootPath: String
    public var createdAt: Date
    public var colorHex: String?
    public var displayName: String?
    public var iconKind: String
    public var iconValue: String?
    public var avatarImage: Data?
    public var defaultWorktreeBase: String?
    public var worktreeLocationOverride: String?

    public init(
        id: String, name: String, rootPath: String, createdAt: Date, colorHex: String? = nil,
        displayName: String? = nil, iconKind: String = "icon", iconValue: String? = nil,
        avatarImage: Data? = nil, defaultWorktreeBase: String? = nil,
        worktreeLocationOverride: String? = nil
    ) {
        self.id = id; self.name = name; self.rootPath = rootPath; self.createdAt = createdAt
        self.colorHex = colorHex
        self.displayName = displayName; self.iconKind = iconKind; self.iconValue = iconValue
        self.avatarImage = avatarImage; self.defaultWorktreeBase = defaultWorktreeBase
        self.worktreeLocationOverride = worktreeLocationOverride
    }
}

public struct WorktreeRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "worktree"
    public var id: String
    public var projectId: String
    public var branch: String
    public var path: String
    public var createdAt: Date
    public var comment: String?
    public var commentUpdatedAt: Date?
    public var isPrimary: Bool

    public init(id: String, projectId: String, branch: String, path: String, createdAt: Date,
                comment: String? = nil, commentUpdatedAt: Date? = nil, isPrimary: Bool = false) {
        self.id = id; self.projectId = projectId; self.branch = branch
        self.path = path; self.createdAt = createdAt
        self.comment = comment; self.commentUpdatedAt = commentUpdatedAt; self.isPrimary = isPrimary
    }
}

public struct PaneScrollbackRecord: Codable, FetchableRecord, PersistableRecord, Sendable {
    public static let databaseTableName = "paneScrollback"
    public var paneId: String
    public var worktreeId: String
    public var data: Data
    public var updatedAt: Date

    public init(paneId: String, worktreeId: String, data: Data, updatedAt: Date) {
        self.paneId = paneId; self.worktreeId = worktreeId
        self.data = data; self.updatedAt = updatedAt
    }
}

public struct TerminalTabRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "terminalTab"
    public var id: String
    public var worktreeId: String
    public var title: String
    public var orderIdx: Int
    public var isActive: Bool
    public var treeJSON: String
    public var updatedAt: Date

    public init(id: String, worktreeId: String, title: String, orderIdx: Int,
                isActive: Bool, treeJSON: String, updatedAt: Date) {
        self.id = id; self.worktreeId = worktreeId; self.title = title
        self.orderIdx = orderIdx; self.isActive = isActive
        self.treeJSON = treeJSON; self.updatedAt = updatedAt
    }
}

public struct AgentAccountRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "agentAccount"
    public var id: String
    public var provider: String
    public var configDirPath: String
    public var label: String
    public var orgName: String?
    public var createdAt: Date
    public var lastAuthenticatedAt: Date

    public init(id: String, provider: String, configDirPath: String, label: String,
                orgName: String? = nil, createdAt: Date, lastAuthenticatedAt: Date) {
        self.id = id; self.provider = provider; self.configDirPath = configDirPath
        self.label = label; self.orgName = orgName
        self.createdAt = createdAt; self.lastAuthenticatedAt = lastAuthenticatedAt
    }
}

public struct AgentSessionRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "agentSession"
    public var paneId: String
    public var worktreeId: String
    public var agentId: String
    public var sessionRef: String
    public var capturedAt: Date

    public init(paneId: String, worktreeId: String, agentId: String,
                sessionRef: String, capturedAt: Date) {
        self.paneId = paneId; self.worktreeId = worktreeId; self.agentId = agentId
        self.sessionRef = sessionRef; self.capturedAt = capturedAt
    }
}
