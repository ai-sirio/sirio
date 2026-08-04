import SwiftUI
import TillerGit

struct PendingGitDiscard: Identifiable {
    enum Kind: Equatable { case changes, untracked }

    let id = UUID()
    let kind: Kind
    let entries: [GitStatusEntry]

    var title: String {
        entries.count == 1
            ? "Discard \(entries[0].path.value)?"
            : "Discard \(entries.count) files?"
    }

    var message: String {
        kind == .untracked
            ? "Untracked files will be permanently deleted."
            : "Unstaged changes will be restored from the Git index."
    }
}

/// Status letter and colour for a changed file. Extracted because the file
/// explorer and the changes list both need it and had drifted into two copies.
enum GitStatusStyle {
    static func symbol(_ entry: GitStatusEntry) -> String {
        if entry.isConflicted { return "U" }
        if entry.isUntracked { return "?" }
        if entry.indexState == .added { return "A" }
        if entry.indexState == .deleted || entry.worktreeState == .deleted { return "D" }
        if entry.indexState == .renamed || entry.worktreeState == .renamed { return "R" }
        return "M"
    }

    static func color(_ entry: GitStatusEntry) -> Color {
        if entry.isConflicted { return AppTheme.gitConflict }
        if entry.isUntracked { return AppTheme.gitUntracked }
        if entry.isStaged { return AppTheme.gitStaged }
        return AppTheme.gitModified
    }
}
