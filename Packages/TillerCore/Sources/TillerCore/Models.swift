import Foundation

public struct Project: Identifiable, Hashable, Sendable {
    public let id: UUID
    public var name: String
    public var rootPath: String
    public var colorHex: String?
    public var displayName: String?
    public var iconKind: IconKind
    public var iconValue: String?
    public var avatarImage: Data?
    public var defaultWorktreeBase: String?
    public var worktreeLocationOverride: String?

    public init(
        id: UUID, name: String, rootPath: String, colorHex: String? = nil,
        displayName: String? = nil, iconKind: IconKind = .icon, iconValue: String? = nil,
        avatarImage: Data? = nil, defaultWorktreeBase: String? = nil,
        worktreeLocationOverride: String? = nil
    ) {
        self.id = id; self.name = name; self.rootPath = rootPath; self.colorHex = colorHex
        self.displayName = displayName; self.iconKind = iconKind; self.iconValue = iconValue
        self.avatarImage = avatarImage; self.defaultWorktreeBase = defaultWorktreeBase
        self.worktreeLocationOverride = worktreeLocationOverride
    }
}

/// How a project's sidebar glyph is rendered: a cached avatar image, an SF
/// Symbol, or a literal emoji character. Default is `.icon` with no value,
/// which falls back to the hash-derived folder glyph.
public enum IconKind: String, Codable, Sendable {
    case avatar, icon, emoji
}

public struct Worktree: Identifiable, Hashable, Sendable {
    public let id: UUID
    public let projectId: UUID
    public var branch: String
    public var path: String
    public var comment: String?
    public var commentUpdatedAt: Date?
    public var isPrimary: Bool

    public init(
        id: UUID, projectId: UUID, branch: String, path: String,
        comment: String? = nil, commentUpdatedAt: Date? = nil, isPrimary: Bool = false
    ) {
        self.id = id; self.projectId = projectId; self.branch = branch; self.path = path
        self.comment = comment; self.commentUpdatedAt = commentUpdatedAt; self.isPrimary = isPrimary
    }
}


/// Ciclo di vita di un agente Tiller (badge sidebar + notifiche native).
/// Hooks (tillerctl notify) own the lifecycle; process exit only clears via onClose.
public enum AgentStatus: String, Sendable {
    case running, needsInput = "needs-input", done, error

    /// Etichetta umana per i titoli delle notifiche native (inglese, come da spec).
    public var humanLabel: String {
        switch self {
        case .running: "running"
        case .needsInput: "needs input"
        case .done: "finished"
        case .error: "failed"
        }
    }

    public static let priorityOrder: [AgentStatus] = [.error, .needsInput, .running, .done]

    /// Highest-priority status in the collection, or nil if empty.
    /// Priority: error > needs-input > running > done.
    public static func highestPriority(in statuses: some Collection<AgentStatus>) -> AgentStatus? {
        for wanted in priorityOrder where statuses.contains(wanted) {
            return wanted
        }
        return nil
    }
}

/// Payload for a native macOS notification. Built by AgentActivityModel
/// using agent + worktree context, and posted by AgentNotifier.
public struct NotificationPayload: Sendable {
    public let paneId: UUID
    public let worktreeId: UUID
    public let title: String    // e.g. "Claude Code — needs input"
    public let body: String     // e.g. "feature/login · myapp"

    public init(paneId: UUID, worktreeId: UUID, title: String, body: String) {
        self.paneId = paneId
        self.worktreeId = worktreeId
        self.title = title
        self.body = body
    }
}

/// An agent-status transition: the old status (nil = first known status for
/// this pane) and the new status. Returned by AgentActivityModel methods so
/// the caller can decide whether to post a notification.
public struct AgentTransition: Sendable, Equatable {
    public let paneId: UUID
    public let old: AgentStatus?
    public let new: AgentStatus

    public init(paneId: UUID, old: AgentStatus?, new: AgentStatus) {
        self.paneId = paneId
        self.old = old
        self.new = new
    }
}
