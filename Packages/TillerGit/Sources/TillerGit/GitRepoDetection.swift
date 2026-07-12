import Foundation

public enum GitRepoDetection {
    /// True if `path` contains a `.git` entry. A directory is a normal
    /// repo; a file is a linked worktree/submodule — both count as git.
    public static func isGitRepository(path: String) -> Bool {
        FileManager.default.fileExists(
            atPath: (path as NSString).appendingPathComponent(".git")
        )
    }
}
