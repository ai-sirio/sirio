//! Aggregated status markers for directories in the Changes tree.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::StatusEntry;

/// The strongest status found below a directory — or, via [`Self::for_file`],
/// a single file's own status.
///
/// `Staged` exists only for the latter: [`Self::for_entry`] (directory
/// aggregation) never produces it, by design — see its own doc comment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectoryGitStatus {
    Conflicted,
    Staged,
    Changed,
    Untracked,
}

impl DirectoryGitStatus {
    /// Ordering directory aggregation resolves ties with. `Staged` is
    /// included only for exhaustiveness — [`Self::for_entry`], the only
    /// producer directory aggregation ever calls, never returns it, so this
    /// value is never actually compared in practice.
    fn precedence(self) -> u8 {
        match self {
            Self::Conflicted => 2,
            Self::Staged => 1,
            Self::Changed => 1,
            Self::Untracked => 0,
        }
    }

    /// The status one changed path contributes to *directory* aggregation,
    /// before any aggregation.
    ///
    /// This is the classification [`DirectoryStatusAggregator::directory_statuses`]
    /// applies to every entry. It deliberately collapses staged into
    /// changed — directories are a roll-up (VS Code-style: "staged,
    /// modified, deleted and renamed descendants all mark the directory
    /// changed"), mirroring the Swift `DirectoryStatusAggregator`, which
    /// never emits `.staged` either. Use [`Self::for_file`] for a single
    /// file's own marker, which must not lose that distinction.
    pub fn for_entry(entry: &StatusEntry) -> Self {
        if entry.is_conflicted() {
            Self::Conflicted
        } else if entry.is_untracked() {
            Self::Untracked
        } else {
            Self::Changed
        }
    }

    /// The status a single changed *file* carries on its own row — the
    /// full four-way vocabulary the Changes list already uses. Mirrors the
    /// Swift `GitStatusStyle.color` (`GitPanelTypes.swift`): conflicted >
    /// untracked > staged > modified. Staged wins even when the same file
    /// also carries unstaged changes — the state a two-state marker cannot
    /// represent, and where getting the order backwards is invisible until
    /// someone stages a half-finished file.
    pub fn for_file(entry: &StatusEntry) -> Self {
        if entry.is_conflicted() {
            Self::Conflicted
        } else if entry.is_untracked() {
            Self::Untracked
        } else if entry.is_staged() {
            Self::Staged
        } else {
            Self::Changed
        }
    }

    /// The stable lower-case name of this status, for element ids and any
    /// other machine-readable spelling. Not a user-facing label.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Conflicted => "conflicted",
            Self::Staged => "staged",
            Self::Changed => "changed",
            Self::Untracked => "untracked",
        }
    }
}

/// Namespace for directory aggregation.
pub struct DirectoryStatusAggregator;

impl DirectoryStatusAggregator {
    /// Maps every non-root ancestor of a changed path to its strongest status.
    /// A rename contributes both its current and original path ancestors.
    pub fn directory_statuses(entries: &[StatusEntry]) -> HashMap<PathBuf, DirectoryGitStatus> {
        let mut result = HashMap::new();
        for entry in entries {
            let status = DirectoryGitStatus::for_entry(entry);
            add_ancestors(&mut result, &entry.path, status);
            if let Some(original) = &entry.original_path {
                add_ancestors(&mut result, original, status);
            }
        }
        result
    }

    /// Short alias for callers that do not need the Swift-compatible name.
    pub fn statuses(entries: &[StatusEntry]) -> HashMap<PathBuf, DirectoryGitStatus> {
        Self::directory_statuses(entries)
    }
}

fn add_ancestors(
    result: &mut HashMap<PathBuf, DirectoryGitStatus>,
    path: &Path,
    status: DirectoryGitStatus,
) {
    let mut directory = path.parent();
    while let Some(path) = directory {
        if path.as_os_str().is_empty() {
            break;
        }
        result
            .entry(path.to_path_buf())
            .and_modify(|existing| {
                if existing.precedence() < status.precedence() {
                    *existing = status;
                }
            })
            .or_insert(status);
        directory = path.parent();
    }
}

/// Free-function spelling for directory aggregation.
pub fn directory_statuses(entries: &[StatusEntry]) -> HashMap<PathBuf, DirectoryGitStatus> {
    DirectoryStatusAggregator::directory_statuses(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StatusKind;

    fn entry(index: Option<StatusKind>, worktree: Option<StatusKind>) -> StatusEntry {
        StatusEntry {
            path: PathBuf::from("f"),
            original_path: None,
            index_status: index,
            worktree_status: worktree,
        }
    }

    /// A single changed file's own marker must distinguish "staged" from
    /// "changed" — the Changes list already does this (Swift's
    /// `GitStatusStyle.color` in `GitPanelTypes.swift` checks `isStaged`
    /// before falling back to modified). `for_entry` deliberately does
    /// *not* carry this distinction — directory aggregation collapses
    /// staged into changed on purpose (VS Code-style roll-up, matching
    /// Swift's `DirectoryStatusAggregator` doc comment) — so `for_file` is
    /// the one that must carry the full four-way vocabulary, table-driven
    /// over every combination porcelain v2 can produce, including the one
    /// a two-state marker cannot express at all: staged *and* further
    /// modified in the worktree at once.
    #[test]
    fn for_file_resolves_conflicted_over_untracked_over_staged_over_modified() {
        let cases: Vec<(&str, StatusEntry, DirectoryGitStatus)> = vec![
            (
                "staged add: pure index add, zero worktree component",
                entry(Some(StatusKind::Added), None),
                DirectoryGitStatus::Staged,
            ),
            (
                "staged rename: pure index rename, zero worktree component",
                entry(Some(StatusKind::Renamed), None),
                DirectoryGitStatus::Staged,
            ),
            (
                "staged AND further modified: the case a two-state marker \
                 cannot represent — staged must still win over the \
                 unstaged component",
                entry(Some(StatusKind::Modified), Some(StatusKind::Modified)),
                DirectoryGitStatus::Staged,
            ),
            (
                "unstaged modification only, nothing staged",
                entry(None, Some(StatusKind::Modified)),
                DirectoryGitStatus::Changed,
            ),
            (
                "untracked",
                entry(Some(StatusKind::Untracked), Some(StatusKind::Untracked)),
                DirectoryGitStatus::Untracked,
            ),
            (
                "conflicted beats everything, including staged",
                entry(Some(StatusKind::Unmerged), Some(StatusKind::Unmerged)),
                DirectoryGitStatus::Conflicted,
            ),
        ];
        for (label, entry, expected) in cases {
            assert_eq!(DirectoryGitStatus::for_file(&entry), expected, "{label}");
        }
    }

    /// Directory aggregation stays deliberately narrower than a file's own
    /// marker: it must keep collapsing staged into changed, matching
    /// Swift's `DirectoryStatusAggregator`, which never emits `.staged` —
    /// unaffected by adding the `Staged` variant for file rows.
    #[test]
    fn for_entry_still_collapses_staged_into_changed_for_directory_aggregation() {
        let staged_only = entry(Some(StatusKind::Added), None);
        assert_eq!(
            DirectoryGitStatus::for_entry(&staged_only),
            DirectoryGitStatus::Changed
        );
    }
}
