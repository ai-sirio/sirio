import Foundation

/// Aggregated git state for a directory, derived from the changed files
/// beneath it. Collapsed like VS Code's explorer: staged, modified, deleted
/// and renamed descendants all mark the directory `changed`.
public enum DirectoryGitStatus: Sendable, Equatable {
    case conflicted
    case changed
    case untracked

    /// conflicted > changed > untracked
    var precedence: Int {
        switch self {
        case .conflicted: 2
        case .changed: 1
        case .untracked: 0
        }
    }
}

public enum DirectoryStatusAggregator {
    /// Maps every ancestor directory (relative path, no trailing slash) of a
    /// changed file to its aggregated status. Rename entries contribute the
    /// ancestors of both current and original paths. Root-level files have
    /// no ancestors and contribute nothing.
    public static func directoryStatuses(
        from statusByPath: [String: GitStatusEntry]
    ) -> [String: DirectoryGitStatus] {
        var result: [String: DirectoryGitStatus] = [:]
        for entry in statusByPath.values {
            let status: DirectoryGitStatus =
                entry.isConflicted ? .conflicted
                : entry.isUntracked ? .untracked
                : .changed
            for path in entry.mutationPaths {
                var components = path.value.split(separator: "/").dropLast()
                while !components.isEmpty {
                    let directory = components.joined(separator: "/")
                    if result[directory].map({ $0.precedence < status.precedence }) ?? true {
                        result[directory] = status
                    }
                    components = components.dropLast()
                }
            }
        }
        return result
    }
}
