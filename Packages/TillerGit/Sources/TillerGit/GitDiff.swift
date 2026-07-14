import Foundation

public enum GitDiffLineKind: Equatable, Sendable {
    case metadata
    case hunk
    case context
    case addition
    case deletion
}

public struct GitDiffLine: Identifiable, Equatable, Sendable {
    public let id: Int
    public let kind: GitDiffLineKind
    public let oldLineNumber: Int?
    public let newLineNumber: Int?
    public let text: String
}

public struct GitFileDiff: Equatable, Sendable {
    public let path: GitPath
    public let lines: [GitDiffLine]
    public let additions: Int
    public let deletions: Int
    public let isBinary: Bool
    public let isSubmodule: Bool
}

public enum GitDiff {
    public static let outputLimits = GitOutputLimits(
        maxBytes: 5 * 1024 * 1024,
        maxLines: 20_000)

    public static func load(entry: GitStatusEntry, in repoPath: String) async throws -> GitFileDiff {
        let hasHead = try await GitRepository.hasHead(in: repoPath)
        let result: GitCommandResult
        if entry.isUntracked || !hasHead {
            let absolutePath = URL(fileURLWithPath: repoPath, isDirectory: true)
                .appendingPathComponent(entry.path.value).path
            result = try await GitRunner.runCaptured(
                ["diff", "--no-color", "--no-ext-diff", "--no-index", "--unified=3",
                 "--", "/dev/null", absolutePath],
                in: repoPath,
                acceptedExitCodes: [0, 1],
                limits: outputLimits)
        } else {
            result = try await GitRunner.runCaptured(
                ["diff", "--no-color", "--no-ext-diff", "--unified=3", "HEAD", "--"]
                    + entry.mutationPaths.map(\.value),
                in: repoPath,
                limits: outputLimits)
        }
        return try parse(result.stdoutString, path: entry.path)
    }

    public static func parse(_ patch: String, path: GitPath) throws -> GitFileDiff {
        let rawLines = patch.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        var lines: [GitDiffLine] = []
        var oldLine: Int?
        var newLine: Int?
        var additions = 0
        var deletions = 0
        var isBinary = false
        var isSubmodule = false
        var inHunk = false

        for raw in rawLines where !raw.isEmpty {
            let id = lines.count
            if raw.hasPrefix("diff --git ") {
                inHunk = false
                oldLine = nil
                newLine = nil
                lines.append(GitDiffLine(id: id, kind: .metadata,
                                         oldLineNumber: nil, newLineNumber: nil, text: raw))
            } else if raw.hasPrefix("Binary files ") || raw == "GIT binary patch" {
                isBinary = true
                lines.append(GitDiffLine(id: id, kind: .metadata,
                                         oldLineNumber: nil, newLineNumber: nil, text: raw))
            } else if raw.hasPrefix("@@ "), let starts = hunkStarts(raw) {
                inHunk = true
                oldLine = starts.old
                newLine = starts.new
                lines.append(GitDiffLine(id: id, kind: .hunk,
                                         oldLineNumber: nil, newLineNumber: nil, text: raw))
            } else if inHunk, raw.hasPrefix("+") {
                let text = String(raw.dropFirst())
                if text.hasPrefix("Subproject commit ") { isSubmodule = true }
                lines.append(GitDiffLine(id: id, kind: .addition,
                                         oldLineNumber: nil, newLineNumber: newLine, text: text))
                additions += 1
                newLine = newLine.map { $0 + 1 }
            } else if inHunk, raw.hasPrefix("-") {
                let text = String(raw.dropFirst())
                if text.hasPrefix("Subproject commit ") { isSubmodule = true }
                lines.append(GitDiffLine(id: id, kind: .deletion,
                                         oldLineNumber: oldLine, newLineNumber: nil, text: text))
                deletions += 1
                oldLine = oldLine.map { $0 + 1 }
            } else if inHunk, raw.hasPrefix(" ") {
                lines.append(GitDiffLine(id: id, kind: .context,
                                         oldLineNumber: oldLine, newLineNumber: newLine,
                                         text: String(raw.dropFirst())))
                oldLine = oldLine.map { $0 + 1 }
                newLine = newLine.map { $0 + 1 }
            } else {
                lines.append(GitDiffLine(id: id, kind: .metadata,
                                         oldLineNumber: nil, newLineNumber: nil, text: raw))
            }
        }
        return GitFileDiff(path: path, lines: lines, additions: additions,
                           deletions: deletions, isBinary: isBinary, isSubmodule: isSubmodule)
    }

    private static func hunkStarts(_ line: String) -> (old: Int, new: Int)? {
        let fields = line.split(separator: " ")
        guard fields.count >= 3,
              fields[1].first == "-", fields[2].first == "+" else { return nil }

        func start(_ field: Substring) -> Int? {
            let digits = field.dropFirst().split(separator: ",", maxSplits: 1).first
            return digits.flatMap { Int($0) }
        }
        guard let old = start(fields[1]), let new = start(fields[2]) else { return nil }
        return (old, new)
    }
}
