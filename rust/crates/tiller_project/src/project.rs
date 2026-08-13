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
    /// Optional sidebar colour.
    pub color: Option<String>,
    /// Optional user-facing name override.
    pub display_name: Option<String>,
    /// Optional icon/avatar key.
    pub icon: Option<String>,
    /// Optional avatar bytes owned by the persistence layer.
    pub avatar: Option<Vec<u8>>,
    /// Explicit base branch for newly-created worktrees.
    pub default_worktree_base: Option<String>,
    /// Explicit parent directory for newly-created worktrees.
    pub location_override: Option<PathBuf>,
}

impl Project {
    /// Returns a new project with the given fields.
    pub fn new(id: ProjectId, name: impl Into<String>, root_path: PathBuf, is_git: bool) -> Self {
        Self {
            id,
            name: name.into(),
            root_path,
            is_git,
            color: None,
            display_name: None,
            icon: None,
            avatar: None,
            default_worktree_base: None,
            location_override: None,
        }
    }

    pub fn effective_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }
}
