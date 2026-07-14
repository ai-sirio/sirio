import Foundation

public enum GitPathError: Error, Equatable {
    case empty
    case nulByte
    case absolutePath(String)
    case parentTraversal(String)
}

extension GitPathError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .empty: "Git path is empty."
        case .nulByte: "Git path contains a NUL byte."
        case .absolutePath(let path): "Git path must be relative: \(path)"
        case .parentTraversal(let path): "Git path escapes the repository: \(path)"
        }
    }
}

public struct GitPath: Hashable, Comparable, Sendable {
    public let value: String

    public init(_ value: String) throws {
        guard !value.isEmpty else { throw GitPathError.empty }
        guard !value.utf8.contains(0) else { throw GitPathError.nulByte }
        guard !value.hasPrefix("/") else { throw GitPathError.absolutePath(value) }
        guard !value.split(separator: "/", omittingEmptySubsequences: false).contains("..") else {
            throw GitPathError.parentTraversal(value)
        }
        self.value = value
    }

    public static func < (lhs: GitPath, rhs: GitPath) -> Bool {
        lhs.value.localizedStandardCompare(rhs.value) == .orderedAscending
    }
}

public enum GitFileState: String, Equatable, Sendable {
    case modified
    case added
    case deleted
    case renamed
    case copied
    case typeChanged
    case unmerged
    case untracked
}

public struct GitStatusEntry: Hashable, Sendable {
    public let path: GitPath
    public let originalPath: GitPath?
    public let indexState: GitFileState?
    public let worktreeState: GitFileState?

    public var isStaged: Bool { indexState != nil && indexState != .untracked }
    public var hasWorktreeChanges: Bool { worktreeState != nil && worktreeState != .untracked }
    public var isUntracked: Bool { indexState == .untracked || worktreeState == .untracked }
    public var isConflicted: Bool { indexState == .unmerged || worktreeState == .unmerged }

    public var mutationPaths: [GitPath] {
        originalPath.map { [path, $0] } ?? [path]
    }
}

public struct GitStatusSnapshot: Equatable, Sendable {
    public let entries: [GitStatusEntry]
    public static let empty = GitStatusSnapshot(entries: [])

    public var staged: [GitStatusEntry] { entries.filter(\.isStaged) }
    public var changes: [GitStatusEntry] {
        entries.filter { $0.hasWorktreeChanges && !$0.isUntracked }
    }
    public var untracked: [GitStatusEntry] { entries.filter(\.isUntracked) }
    public var isClean: Bool { entries.isEmpty }
}

public enum GitStatusParseError: Error, Equatable {
    case malformedRecord
    case missingRenameSource(String)
    case unsupportedStatus(UInt8)
}

extension GitStatusParseError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .malformedRecord: "Git returned a malformed porcelain status record."
        case .missingRenameSource(let path): "Git omitted the rename source for \(path)."
        case .unsupportedStatus(let byte): "Git returned unsupported status byte \(byte)."
        }
    }
}

public enum GitStatus {
    public static func load(in repoPath: String) async throws -> GitStatusSnapshot {
        let result = try await GitRunner.runCaptured(
            ["status", "--porcelain=v1", "-z", "--untracked-files=all"], in: repoPath)
        return try parse(result.stdout)
    }

    public static func parse(_ data: Data) throws -> GitStatusSnapshot {
        let records = data.split(separator: 0, omittingEmptySubsequences: true)
        var index = 0
        var entries: [GitStatusEntry] = []
        while index < records.count {
            let record = records[index]
            guard record.count >= 4, record[record.startIndex + 2] == 0x20 else {
                throw GitStatusParseError.malformedRecord
            }
            let x = record[record.startIndex]
            let y = record[record.startIndex + 1]
            let conflicted = x == 0x55 || y == 0x55
                || [(0x44, 0x44), (0x41, 0x55), (0x55, 0x44),
                    (0x55, 0x41), (0x44, 0x55), (0x41, 0x41)].contains { $0 == (x, y) }
            let current = try GitPath(String(decoding: record.dropFirst(3), as: UTF8.self))
            let renameOrCopy = x == 0x52 || x == 0x43 || y == 0x52 || y == 0x43
            var original: GitPath?
            if renameOrCopy {
                index += 1
                guard index < records.count else {
                    throw GitStatusParseError.missingRenameSource(current.value)
                }
                original = try GitPath(String(decoding: records[index], as: UTF8.self))
            }
            entries.append(GitStatusEntry(
                path: current,
                originalPath: original,
                indexState: try state(for: x, conflicted: conflicted),
                worktreeState: try state(for: y, conflicted: conflicted)))
            index += 1
        }
        return GitStatusSnapshot(entries: entries.sorted { $0.path < $1.path })
    }

    private static func state(for byte: UInt8, conflicted: Bool) throws -> GitFileState? {
        if byte == 0x20 { return conflicted ? .unmerged : nil }
        if conflicted { return .unmerged }
        switch byte {
        case 0x4D: return .modified
        case 0x41: return .added
        case 0x44: return .deleted
        case 0x52: return .renamed
        case 0x43: return .copied
        case 0x54: return .typeChanged
        case 0x3F: return .untracked
        default: throw GitStatusParseError.unsupportedStatus(byte)
        }
    }
}
