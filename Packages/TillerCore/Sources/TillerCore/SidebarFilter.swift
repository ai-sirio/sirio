import Foundation

/// Pure sidebar filter: a project row stays visible when the query matches
/// its name or any of its worktree branches (case-insensitive substring).
public enum SidebarFilter {
    public static func matches(projectName: String, worktreeBranches: [String], query: String) -> Bool {
        let q = query.trimmingCharacters(in: .whitespaces)
        guard !q.isEmpty else { return true }
        if projectName.localizedCaseInsensitiveContains(q) { return true }
        return worktreeBranches.contains { $0.localizedCaseInsensitiveContains(q) }
    }
}
