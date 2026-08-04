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

    static func effective(rawValue: String, isGitRepository: Bool) -> RightPanelMode {
        let saved = rawValue == "diff" ? .status : RightPanelMode(rawValue: rawValue) ?? .files
        return saved.requiresGit && !isGitRepository ? .files : saved
    }
}
