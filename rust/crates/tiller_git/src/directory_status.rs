//! Aggregated status markers for directories in the Changes tree.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::StatusEntry;

/// The strongest status found below a directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectoryGitStatus {
    Conflicted,
    Changed,
    Untracked,
}

impl DirectoryGitStatus {
    fn precedence(self) -> u8 {
        match self {
            Self::Conflicted => 2,
            Self::Changed => 1,
            Self::Untracked => 0,
        }
    }

    /// The status one changed path contributes, before any aggregation.
    ///
    /// This is the classification [`DirectoryStatusAggregator::directory_statuses`]
    /// applies to every entry — exposed (rather than duplicated) so a *file*
    /// row in the same tree can be tinted from the same three-valued
    /// vocabulary its ancestors are, and so the two can never drift apart.
    pub fn for_entry(entry: &StatusEntry) -> Self {
        if entry.is_conflicted() {
            Self::Conflicted
        } else if entry.is_untracked() {
            Self::Untracked
        } else {
            Self::Changed
        }
    }

    /// The stable lower-case name of this status, for element ids and any
    /// other machine-readable spelling. Not a user-facing label.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Conflicted => "conflicted",
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
