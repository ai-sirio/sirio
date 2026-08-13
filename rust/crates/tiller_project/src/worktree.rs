//! The [`Worktree`] value type: a git worktree (or primary checkout) of a
//! project.

use std::path::PathBuf;
use std::time::SystemTime;

use crate::{ProjectId, WorktreeId};

/// A git worktree of a project.
///
/// The project's primary checkout is itself a worktree — git models it as
/// the "main" worktree — so `is_primary` distinguishes it from linked
/// worktrees created with `git worktree add`. This matches how the Swift app
/// treats the main checkout: it is the first entry returned by
/// `git worktree list --porcelain`.
///
/// `branch` is the branch label the UI should show. For a checkout on a
/// branch that is the branch name (`main`, `feature/x`); for a detached HEAD
/// the discovery layer substitutes a stable label (the short commit hash,
/// or `main` for the primary checkout, matching the Swift app's fallback).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
    /// Stable identity within a [`crate::Workspace`].
    pub id: WorktreeId,
    /// The project this worktree belongs to.
    pub project_id: ProjectId,
    /// Branch label to display (see type docs for the detached-HEAD case).
    pub branch: String,
    /// Absolute path of the checkout directory.
    pub path: PathBuf,
    /// True for the project's primary checkout (the git main worktree).
    pub is_primary: bool,
    pub comment: Option<String>,
    pub created_at: Option<SystemTime>,
    pub updated_at: Option<SystemTime>,
}

impl Worktree {
    /// Returns a new worktree with the given fields.
    pub fn new(
        id: WorktreeId,
        project_id: ProjectId,
        branch: impl Into<String>,
        path: PathBuf,
        is_primary: bool,
    ) -> Self {
        Self {
            id,
            project_id,
            branch: branch.into(),
            path,
            is_primary,
            comment: None,
            created_at: None,
            updated_at: None,
        }
    }
}
