import Foundation

/// Per-file line counts, cheap enough to fetch for every changed file at once.
/// Binary files report zero counts with `isBinary` set: git emits `-` instead
/// of a number for them.
public struct GitDiffStat: Equatable, Sendable {
    public let additions: Int
    public let deletions: Int
    public let isBinary: Bool

    public init(additions: Int, deletions: Int, isBinary: Bool) {
        self.additions = additions
        self.deletions = deletions
        self.isBinary = isBinary
    }
}

extension GitDiff {
    /// Parses `git diff --numstat -z` output. NUL separation is what makes this
    /// unambiguous: without `-z`, git quotes non-ASCII paths and folds renames
    /// into a `{old => new}` brace form that cannot be split reliably.
    public static func parseNumstat(_ output: String) -> [GitPath: GitDiffStat] {
        let parts = output.split(separator: "\0", omittingEmptySubsequences: false).map(String.init)
        var stats: [GitPath: GitDiffStat] = [:]
        var index = 0

        while index < parts.count {
            let record = parts[index]
            guard !record.isEmpty else {
                index += 1
                continue
            }
            let fields = record.split(separator: "\t", omittingEmptySubsequences: false)
            guard fields.count >= 3 else {
                index += 1
                continue
            }
            let isBinary = fields[0] == "-"
            let additions = Int(fields[0]) ?? 0
            let deletions = Int(fields[1]) ?? 0
            var pathValue = String(fields[2])

            if pathValue.isEmpty {
                // Rename: the destination is two records further on.
                guard index + 2 < parts.count else { break }
                pathValue = parts[index + 2]
                index += 3
            } else {
                index += 1
            }
            guard let path = try? GitPath(pathValue) else { continue }
            stats[path] = GitDiffStat(
                additions: additions, deletions: deletions, isBinary: isBinary)
        }
        return stats
    }

    /// Line counts for every changed file, in one git invocation plus a cheap
    /// read per untracked file. Untracked files do not appear in `--numstat`
    /// against HEAD at all, so their additions are counted from disk.
    public static func stats(
        entries: [GitStatusEntry], in repoPath: String
    ) async throws -> [GitPath: GitDiffStat] {
        var stats: [GitPath: GitDiffStat] = [:]

        if try await GitRepository.hasHead(in: repoPath) {
            let result = try await GitRunner.runCaptured(
                ["diff", "--numstat", "-z", "--no-color", "--no-ext-diff", "HEAD"],
                in: repoPath,
                limits: outputLimits)
            stats = parseNumstat(result.stdoutString)
        }

        let root = URL(fileURLWithPath: repoPath, isDirectory: true)
        for entry in entries where entry.isUntracked {
            let url = root.appendingPathComponent(entry.path.value)
            guard let lines = lineCount(at: url) else { continue }
            stats[entry.path] = GitDiffStat(
                additions: lines, deletions: 0, isBinary: false)
        }
        return stats
    }

    /// Nil past the snapshot size limit, or for anything that is not UTF-8:
    /// a count is a nicety, never worth loading an arbitrarily large file for.
    private static func lineCount(at url: URL) -> Int? {
        guard let data = try? Data(contentsOf: url, options: .mappedIfSafe),
              data.count <= 500_000,
              let text = String(data: data, encoding: .utf8) else { return nil }
        if text.isEmpty { return 0 }
        return text.hasSuffix("\n")
            ? text.split(separator: "\n", omittingEmptySubsequences: false).count - 1
            : text.split(separator: "\n", omittingEmptySubsequences: false).count
    }
}
