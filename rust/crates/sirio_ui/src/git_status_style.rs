//! The one place a git status becomes a colour.
//!
//! F-CHG-06. This module exists because the port had already made the same
//! mistake twice, in the same shape, and the second time it was invisible.
//!
//! The Swift original resolves a git status to a colour in exactly one
//! function, `GitStatusStyle.color` (`App/RightPanel/GitPanelTypes.swift:36`),
//! and its own doc comment records *why* it is factored out: "the file
//! explorer and the changes list both need it and had drifted into two
//! copies." The Rust port re-created that drift — `right_panel.rs` resolved a
//! file's marker dot one way and `changes.rs`'s `status_color` resolved the
//! same file's row another way, disagreeing on the single case that
//! distinguishes them.
//!
//! That case is a file that is **staged and then modified again**. Swift's
//! precedence is conflicted > untracked > staged > modified, so staged wins
//! and the file is green in both views. `changes.rs` tested
//! `has_worktree_changes()` *before* `is_staged()`, so the same file came out
//! amber there and green in the Files tree — the two panels visibly
//! contradicting each other about one file, on screen, at the same time.
//!
//! Reordering the branches would have fixed today's symptom and left the
//! mechanism in place. Having one resolver is what makes the next drift
//! impossible rather than merely absent, which is the part of Swift's design
//! worth porting.

use gpui::Rgba;
use sirio_git::{DirectoryGitStatus, StatusEntry};
use sirio_theme::Theme;

/// Map a resolved status to its theme colour. The only mapping in the crate.
pub fn status_color(status: DirectoryGitStatus, theme: Theme) -> Rgba {
    match status {
        DirectoryGitStatus::Conflicted => theme.git_conflict,
        DirectoryGitStatus::Staged => theme.git_staged,
        DirectoryGitStatus::Changed => theme.git_modified,
        DirectoryGitStatus::Untracked => theme.git_untracked,
    }
}

/// The colour a single file's row or marker takes, resolved through
/// [`DirectoryGitStatus::for_file`] so every view agrees on precedence.
///
/// Returns `theme.title` — the neutral, unmarked colour — for an entry
/// carrying no status at all. That fallback is deliberate and is *not* what
/// `for_file` would give: `for_file` ends in `Changed`, so delegating to it
/// unconditionally would paint an unmarked row amber. The Changes list is not
/// expected to hold such an entry, but it rendered them neutral before this
/// module existed, and a shared resolver should not quietly change an
/// unrelated case on its way to fixing the one it was written for.
pub fn entry_color(entry: &StatusEntry, theme: Theme) -> Rgba {
    if !entry.is_conflicted()
        && !entry.is_untracked()
        && !entry.is_staged()
        && !entry.has_worktree_changes()
    {
        return theme.title;
    }
    status_color(DirectoryGitStatus::for_file(entry), theme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sirio_git::StatusKind;
    use std::path::PathBuf;

    fn entry(index: Option<StatusKind>, worktree: Option<StatusKind>) -> StatusEntry {
        StatusEntry {
            path: PathBuf::from("both.txt"),
            original_path: None,
            index_status: index,
            worktree_status: worktree,
        }
    }

    /// The control that makes every assertion below capable of failing. If the
    /// theme ever gave two of these the same value, the precedence tests would
    /// pass while proving nothing — the exact way F-CHG-06's original test
    /// (`a_staged_and_modified_file_appears_in_both_sections`) stayed green
    /// through the bug by asserting presence and never colour.
    #[test]
    fn the_four_status_colours_are_mutually_distinct() {
        let theme = Theme::dark();
        let all = [
            theme.git_conflict,
            theme.git_untracked,
            theme.git_staged,
            theme.git_modified,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(
                    (a.r, a.g, a.b, a.a),
                    (b.r, b.g, b.b, b.a),
                    "two git status colours coincide; the precedence tests \
                     below would pass vacuously"
                );
            }
        }
    }

    /// F-CHG-06's decisive case: staged, then modified again. Swift's
    /// `GitStatusStyle.color` puts `isStaged` ahead of the modified fallback,
    /// so this file is green. `changes.rs` used to test
    /// `has_worktree_changes()` first and render it amber, disagreeing with
    /// the Files tree about one file on screen at the same time.
    #[test]
    fn a_staged_and_further_modified_file_reads_as_staged() {
        let theme = Theme::dark();
        let both = entry(Some(StatusKind::Modified), Some(StatusKind::Modified));

        assert!(both.is_staged() && both.has_worktree_changes());
        assert_eq!(entry_color(&both, theme), theme.git_staged);
        assert_ne!(entry_color(&both, theme), theme.git_modified);
    }

    #[test]
    fn precedence_is_conflicted_then_untracked_then_staged_then_modified() {
        let theme = Theme::dark();

        // Conflicted outranks everything, including a staged index entry.
        let conflicted = entry(Some(StatusKind::Unmerged), Some(StatusKind::Modified));
        assert_eq!(entry_color(&conflicted, theme), theme.git_conflict);

        // Untracked outranks staged.
        let untracked = entry(Some(StatusKind::Untracked), None);
        assert_eq!(entry_color(&untracked, theme), theme.git_untracked);

        // Staged alone.
        let staged = entry(Some(StatusKind::Added), None);
        assert_eq!(entry_color(&staged, theme), theme.git_staged);

        // Modified alone is the fallback.
        let modified = entry(None, Some(StatusKind::Modified));
        assert_eq!(entry_color(&modified, theme), theme.git_modified);
    }

    /// The fallback `for_file` alone would not give: an entry carrying no
    /// status stays neutral rather than becoming amber. Pins the one case
    /// where `entry_color` deliberately does not delegate.
    #[test]
    fn an_entry_with_no_status_stays_neutral() {
        let theme = Theme::dark();
        assert_eq!(entry_color(&entry(None, None), theme), theme.title);
    }

    /// The drift guard. Both doors into this module must agree for the same
    /// file — this is the assertion that fails if someone reintroduces a
    /// second precedence chain in either view.
    #[test]
    fn both_entry_points_agree_for_every_representable_state() {
        let theme = Theme::dark();
        let states = [
            Some(StatusKind::Modified),
            Some(StatusKind::Added),
            Some(StatusKind::Deleted),
            Some(StatusKind::Renamed),
            Some(StatusKind::Unmerged),
            Some(StatusKind::Untracked),
            None,
        ];

        for index in states {
            for worktree in states {
                let e = entry(index, worktree);
                if entry_color(&e, theme) == theme.title {
                    continue; // the deliberate no-status carve-out
                }
                assert_eq!(
                    entry_color(&e, theme),
                    status_color(DirectoryGitStatus::for_file(&e), theme),
                    "row colour and marker colour disagree for {index:?}/{worktree:?}"
                );
            }
        }
    }
}
