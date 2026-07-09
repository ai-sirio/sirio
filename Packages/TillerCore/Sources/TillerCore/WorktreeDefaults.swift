import Foundation

/// Pure resolution rules for where a new worktree's base branch and parent
/// folder come from — no git calls, no filesystem I/O.
public enum WorktreeDefaults {
    /// Base ref for `git worktree add -b <branch> <path> <base>`. An explicit
    /// per-project override wins; otherwise the worktree flagged `isPrimary`;
    /// otherwise nil (git worktree add branches from HEAD).
    public static func resolveBase(project: Project, worktrees: [Worktree]) -> String? {
        project.defaultWorktreeBase ?? worktrees.first(where: \.isPrimary)?.branch
    }

    /// Parent directory for a new worktree's folder. An explicit override
    /// wins; otherwise the project root's sibling directory (today's default).
    public static func resolveParentDirectory(project: Project) -> String {
        project.worktreeLocationOverride ?? (project.rootPath as NSString).deletingLastPathComponent
    }
}
