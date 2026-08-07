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
    public var orderIdx: Int

    public init(
        id: String, name: String, rootPath: String, createdAt: Date, colorHex: String? = nil,
        displayName: String? = nil, iconKind: String = "icon", iconValue: String? = nil,
        avatarImage: Data? = nil, defaultWorktreeBase: String? = nil,
        worktreeLocationOverride: String? = nil, orderIdx: Int = 0
    ) {
        self.id = id; self.name = name; self.rootPath = rootPath; self.createdAt = createdAt
        self.colorHex = colorHex
        self.displayName = displayName; self.iconKind = iconKind; self.iconValue = iconValue
        self.avatarImage = avatarImage; self.defaultWorktreeBase = defaultWorktreeBase
        self.worktreeLocationOverride = worktreeLocationOverride
        self.orderIdx = orderIdx
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
    public var orderIdx: Int

    public init(id: String, projectId: String, branch: String, path: String, createdAt: Date,
                comment: String? = nil, commentUpdatedAt: Date? = nil, isPrimary: Bool = false,
                orderIdx: Int = 0) {
        self.id = id; self.projectId = projectId; self.branch = branch
        self.path = path; self.createdAt = createdAt
        self.comment = comment; self.commentUpdatedAt = commentUpdatedAt; self.isPrimary = isPrimary
        self.orderIdx = orderIdx
    }
}

public struct PaneScrollbackRecord: Codable, FetchableRecord, PersistableRecord, Sendable {
    public static let databaseTableName = "paneScrollback"
    public var terminalContentId: String
    public var worktreeId: String
    public var data: Data
    public var updatedAt: Date

    public init(paneId: String, worktreeId: String, data: Data, updatedAt: Date) {
        self.terminalContentId = paneId; self.worktreeId = worktreeId
        self.data = data; self.updatedAt = updatedAt
    }

    public init(terminalContentId: String, worktreeId: String, data: Data, updatedAt: Date) {
        self.terminalContentId = terminalContentId; self.worktreeId = worktreeId
        self.data = data; self.updatedAt = updatedAt
    }

    @available(*, deprecated, renamed: "terminalContentId")
    public var paneId: String { terminalContentId }
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
    public var kind: String
    public var filePath: String?
    public var chatAgentId: String?
    /// Chat session this tab renders. NULL for non-chat tabs and for legacy
    /// chat tabs that predate v15 and had no session to adopt.
    public var chatSessionId: String?
    public var titleIsAutoNamed: Bool

    public init(id: String, worktreeId: String, title: String, orderIdx: Int,
                isActive: Bool, treeJSON: String, updatedAt: Date,
                kind: String = "terminal", filePath: String? = nil,
                chatAgentId: String? = nil, titleIsAutoNamed: Bool = true,
                chatSessionId: String? = nil) {
        self.id = id; self.worktreeId = worktreeId; self.title = title
        self.orderIdx = orderIdx; self.isActive = isActive
        self.treeJSON = treeJSON; self.updatedAt = updatedAt
        self.kind = kind; self.filePath = filePath; self.chatAgentId = chatAgentId
        self.titleIsAutoNamed = titleIsAutoNamed
        self.chatSessionId = chatSessionId
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
    public var terminalContentId: String
    public var worktreeId: String
    public var agentId: String
    public var sessionRef: String
    public var capturedAt: Date

    public init(paneId: String, worktreeId: String, agentId: String,
                sessionRef: String, capturedAt: Date) {
        self.terminalContentId = paneId; self.worktreeId = worktreeId; self.agentId = agentId
        self.sessionRef = sessionRef; self.capturedAt = capturedAt
    }

    public init(terminalContentId: String, worktreeId: String, agentId: String,
                sessionRef: String, capturedAt: Date) {
        self.terminalContentId = terminalContentId; self.worktreeId = worktreeId; self.agentId = agentId
        self.sessionRef = sessionRef; self.capturedAt = capturedAt
    }

    @available(*, deprecated, renamed: "terminalContentId")
    public var paneId: String { terminalContentId }
}

public struct WorkspaceLayoutRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "workspaceLayout"
    public var worktreeId: String
    public var schemaVersion: Int
    public var revision: Int
    public var payload: String
    public var checksum: String
    public var updatedAt: Date

    public init(worktreeId: String, schemaVersion: Int, revision: Int, payload: String,
                checksum: String, updatedAt: Date) {
        self.worktreeId = worktreeId; self.schemaVersion = schemaVersion
        self.revision = revision; self.payload = payload; self.checksum = checksum
        self.updatedAt = updatedAt
    }
}

public struct WorkspaceTabRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "workspaceTab"
    public var id: String
    public var worktreeId: String
    public var title: String
    public var titleIsAutoNamed: Bool
    public var contentKind: String
    public var contentId: String
    public var viewStateJSON: String?
    public var viewStateVersion: Int
    public var createdAt: Date

    public init(id: String, worktreeId: String, title: String, titleIsAutoNamed: Bool,
                contentKind: String, contentId: String, viewStateJSON: String?,
                viewStateVersion: Int, createdAt: Date) {
        self.id = id; self.worktreeId = worktreeId; self.title = title
        self.titleIsAutoNamed = titleIsAutoNamed; self.contentKind = contentKind
        self.contentId = contentId; self.viewStateJSON = viewStateJSON
        self.viewStateVersion = viewStateVersion; self.createdAt = createdAt
    }
}

public struct TerminalContentRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "terminalContent"
    public var id: String
    public var worktreeId: String
    public var launchKind: String
    public var agentId: String?
    public var commandJSON: String?
    public var createdAt: Date

    public init(id: String, worktreeId: String, launchKind: String, agentId: String?,
                commandJSON: String?, createdAt: Date) {
        self.id = id; self.worktreeId = worktreeId; self.launchKind = launchKind
        self.agentId = agentId; self.commandJSON = commandJSON; self.createdAt = createdAt
    }
}

public struct BrowserContentRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "browserContent"
    public var id: String
    public var worktreeId: String
    public var url: String
    public var title: String?
    public var createdAt: Date

    public init(id: String, worktreeId: String, url: String, title: String?, createdAt: Date) {
        self.id = id
        self.worktreeId = worktreeId
        self.url = url
        self.title = title
        self.createdAt = createdAt
    }
}

public struct WorkspaceLayoutQuarantineRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "workspaceLayoutQuarantine"
    public var id: String
    public var worktreeId: String
    public var payload: String
    public var reason: String
    public var createdAt: Date

    public init(id: String, worktreeId: String, payload: String, reason: String, createdAt: Date) {
        self.id = id; self.worktreeId = worktreeId; self.payload = payload
        self.reason = reason; self.createdAt = createdAt
    }
}

/// One chat conversation bound to a worktree + agent. `acpSessionId` is the
/// agent-side session reference used for `session/load` resume.
public struct ChatSessionRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "chatSession"
    public var id: String
    public var worktreeId: String
    public var agentId: String
    public var acpSessionId: String?
    public var createdAt: Date
    public var lastActivityAt: Date
    /// Last known context-window usage, so a remounted worktree can show the
    /// ring immediately instead of waiting for the next live `usage_update`.
    public var contextUsageUsed: Int?
    public var contextUsageSize: Int?
    public var permissionMode: String?
    public var selectedModel: String?
    public var selectedEffort: String?
    public var transportKind: String
    /// Mirror of the tab's auto-generated title, so history rows stay
    /// readable after the tab is closed. NULL until auto-naming runs.
    public var title: String?

    public init(id: String, worktreeId: String, agentId: String,
                acpSessionId: String? = nil, createdAt: Date, lastActivityAt: Date,
                contextUsageUsed: Int? = nil, contextUsageSize: Int? = nil,
                permissionMode: String? = nil, selectedModel: String? = nil,
                selectedEffort: String? = nil, transportKind: String = "acp",
                title: String? = nil) {
        self.id = id; self.worktreeId = worktreeId; self.agentId = agentId
        self.acpSessionId = acpSessionId
        self.createdAt = createdAt; self.lastActivityAt = lastActivityAt
        self.contextUsageUsed = contextUsageUsed; self.contextUsageSize = contextUsageSize
        self.permissionMode = permissionMode; self.selectedModel = selectedModel
        self.selectedEffort = selectedEffort; self.transportKind = transportKind
        self.title = title
    }
}

/// One transcript entry. `payload` is an opaque encoded blob owned by the
/// caller (TillerACP encodes TranscriptItem); `kind` is denormalized for
/// future queries.
public struct ChatItemRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "chatItem"
    public var sessionId: String
    public var ordinal: Int
    public var kind: String
    public var payload: Data

    public init(sessionId: String, ordinal: Int, kind: String, payload: Data) {
        self.sessionId = sessionId; self.ordinal = ordinal
        self.kind = kind; self.payload = payload
    }
}
