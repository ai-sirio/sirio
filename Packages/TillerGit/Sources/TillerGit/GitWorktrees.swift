import Foundation

public struct GitWorktreeInfo: Equatable, Sendable {
    public let path: String
    public let branch: String?
    public let isMain: Bool
}

public enum GitWorktrees {
    /// Parses `git worktree list --porcelain`: blocks separated by blank
    /// lines, first block is the main worktree.
    public static func list(repoPath: String) async throws -> [GitWorktreeInfo] {
        let output = try await GitRunner.run(["worktree", "list", "--porcelain"], in: repoPath)
        var result: [GitWorktreeInfo] = []
        var path: String?, branch: String?
        var isFirst = true
        for line in output.split(separator: "\n", omittingEmptySubsequences: false) {
            if line.hasPrefix("worktree ") {
                path = String(line.dropFirst("worktree ".count))
            } else if line.hasPrefix("branch refs/heads/") {
                branch = String(line.dropFirst("branch refs/heads/".count))
            } else if line.isEmpty, let p = path {
                result.append(GitWorktreeInfo(path: p, branch: branch, isMain: isFirst))
                isFirst = false
                path = nil; branch = nil
            }
        }
        if let p = path {
            result.append(GitWorktreeInfo(path: p, branch: branch, isMain: isFirst))
        }
        return result
    }

    public static func add(repoPath: String, branch: String, at path: String, base: String? = nil) async throws {
        var arguments = ["worktree", "add", "-b", branch, path]
        if let base { arguments.append(base) }
        _ = try await GitRunner.run(arguments, in: repoPath)
    }

    public static func remove(repoPath: String, path: String) async throws {
        _ = try await GitRunner.run(["worktree", "remove", "--force", path], in: repoPath)
    }
}
