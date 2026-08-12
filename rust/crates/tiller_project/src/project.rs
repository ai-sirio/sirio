//! The [`Project`] value type: a root directory the user added to Tiller.

use std::path::PathBuf;

use crate::ProjectId;

/// A root directory the user added to Tiller.
///
/// A project is just a folder on disk — Tiller supports non-git projects
/// too — so `is_git` is derived at discovery time and describes what the
/// project's worktree operations can offer, not a precondition for the
/// project existing.
///
/// Mirrors `TillerCore.Project` in the Swift app, trimmed to what the Rust
/// rewrite needs so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    /// Stable identity within a [`crate::Workspace`].
    pub id: ProjectId,
    /// Display name, derived from the last path component of `root_path`.
    pub name: String,
    /// Absolute path of the project's root directory.
    pub root_path: PathBuf,
    /// Whether `root_path` contains a `.git` entry (repo or linked worktree).
    pub is_git: bool,
}

impl Project {
    /// Returns a new project with the given fields.
    pub fn new(id: ProjectId, name: impl Into<String>, root_path: PathBuf, is_git: bool) -> Self {
        Self {
            id,
            name: name.into(),
            root_path,
            is_git,
        }
    }
}
