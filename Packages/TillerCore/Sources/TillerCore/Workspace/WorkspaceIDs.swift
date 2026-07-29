import Foundation

public struct WorkspaceTabID: Hashable, Sendable, Codable {
    public let rawValue: UUID

    public init(_ rawValue: UUID = UUID()) {
        self.rawValue = rawValue
    }
}

public struct PaneGroupID: Hashable, Sendable, Codable {
    public let rawValue: UUID

    public init(_ rawValue: UUID = UUID()) {
        self.rawValue = rawValue
    }
}

public struct SplitID: Hashable, Sendable, Codable {
    public let rawValue: UUID

    public init(_ rawValue: UUID = UUID()) {
        self.rawValue = rawValue
    }
}

public struct TerminalContentID: Hashable, Sendable, Codable {
    public let rawValue: UUID

    public init(_ rawValue: UUID = UUID()) {
        self.rawValue = rawValue
    }
}

public struct ChatContentID: Hashable, Sendable, Codable {
    public let rawValue: String

    public init(_ rawValue: String) {
        self.rawValue = rawValue
    }
}

public struct ResourceGenerationID: Hashable, Sendable {
    public let rawValue: UUID

    public init(_ rawValue: UUID = UUID()) {
        self.rawValue = rawValue
    }
}

public struct DocumentID: Hashable, Sendable, Codable {
    public let worktreeID: UUID
    public let canonicalPath: String

    /// Absolute, standardized, symlink-resolved at open time, worktree-scoped.
    public static func make(worktreeID: UUID, fileURL: URL) -> DocumentID {
        DocumentID(
            worktreeID: worktreeID,
            canonicalPath: fileURL.resolvingSymlinksInPath().standardizedFileURL.path
        )
    }

    private init(worktreeID: UUID, canonicalPath: String) {
        self.worktreeID = worktreeID
        self.canonicalPath = canonicalPath
    }
}
