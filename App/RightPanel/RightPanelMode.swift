import Foundation

/// UI route for the worktree tools panel. Persistence stores rawValue only.
enum RightPanelMode: String, CaseIterable, Identifiable {
    case files
    case status

    var id: String { rawValue }

    var title: String {
        switch self {
        case .files: "Files"
        case .status: "Changes"
        }
    }

    var systemImage: String {
        switch self {
        case .files: "folder"
        case .status: "arrow.triangle.branch"
        }
    }

    var requiresGit: Bool { self != .files }

    /// The modes worth offering. A git-only mode outside a repository can never
    /// be entered — `effective` routes it straight back to `.files` — so the
    /// picker must not show it at all, the way the sidebar hides "New Worktree…"
    /// for the same projects.
    static func available(isGitRepository: Bool) -> [RightPanelMode] {
        allCases.filter { isGitRepository || !$0.requiresGit }
    }

    static func effective(rawValue: String, isGitRepository: Bool) -> RightPanelMode {
        let saved = rawValue == "diff" ? .status : RightPanelMode(rawValue: rawValue) ?? .files
        return saved.requiresGit && !isGitRepository ? .files : saved
    }
}
