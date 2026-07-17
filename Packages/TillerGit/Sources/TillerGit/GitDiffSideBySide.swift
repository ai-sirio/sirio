import Foundation

/// One visual row of a side-by-side diff: old file on the left, new file on the right.
public struct GitDiffSideBySideRow: Identifiable, Equatable, Sendable {
    public let id: Int
    public let left: GitDiffLine?
    public let right: GitDiffLine?

    /// Hunk headers span the full width instead of being split into halves.
    public var isHunk: Bool { left?.kind == .hunk }
}

public enum GitDiffSideBySide {
    /// Pairs unified-diff lines into side-by-side rows: context appears on both
    /// sides, each run of deletions is zipped with the following run of
    /// additions, and metadata lines are dropped.
    public static func rows(from lines: [GitDiffLine]) -> [GitDiffSideBySideRow] {
        var rows: [GitDiffSideBySideRow] = []
        var deletions: [GitDiffLine] = []
        var additions: [GitDiffLine] = []

        func flush() {
            for index in 0..<max(deletions.count, additions.count) {
                rows.append(GitDiffSideBySideRow(
                    id: rows.count,
                    left: index < deletions.count ? deletions[index] : nil,
                    right: index < additions.count ? additions[index] : nil))
            }
            deletions = []
            additions = []
        }

        for line in lines {
            switch line.kind {
            case .deletion:
                deletions.append(line)
            case .addition:
                additions.append(line)
            case .context:
                flush()
                rows.append(GitDiffSideBySideRow(id: rows.count, left: line, right: line))
            case .hunk:
                flush()
                rows.append(GitDiffSideBySideRow(id: rows.count, left: line, right: nil))
            case .metadata:
                flush()
            }
        }
        flush()
        return rows
    }
}
