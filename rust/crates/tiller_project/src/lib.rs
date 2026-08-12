//! The domain model of what Tiller manages — projects, worktrees and tabs —
//! plus real discovery of that model from disk.
//!
//! This crate is deliberately dependency-free and UI-free: the public API is
//! plain value types (`Clone`, `Debug`, `PartialEq`, no interior mutability)
//! and `std::process`-based git shell-out, so the terminal, the control
//! socket and the GPUI sidebar can all build on it without inheriting a UI
//! framework.
//!
//! # Model
//!
//! - [`Project`] — a root directory the user added (git or not).
//! - [`Worktree`] — a git worktree of a project; the primary checkout is
//!   itself a worktree.
//! - [`Tab`] — a surface open inside a worktree (terminal, agent chat,
//!   browser, editor, diff).
//! - [`Workspace`] — the whole tree plus expansion/selection state and the
//!   pure tree operations the sidebar needs.
//!
//! # Discovery
//!
//! [`discovery`] mirrors the Swift app's `TillerGit` package:
//! [`discovery::is_git_repository`] is the pure gate (a `.git` entry at the
//! project root), [`discovery::discover_worktrees`] shells out to
//! `git worktree list --porcelain` and parses the machine format — primary
//! checkout, linked worktrees, detached HEAD, locked and prunable entries —
//! and [`discovery::current_branch`] reports what branch a checkout is on.
//! [`Workspace::load_project`] ties it together.

mod discovery;
mod error;
mod git;
mod id;
mod project;
mod tab;
mod workspace;
mod worktree;

pub use discovery::{
    DiscoveredProject, DiscoveredWorktree, current_branch, discover_project, discover_worktrees,
    is_git_repository, parse_worktree_list,
};
pub use error::GitError;
pub use id::{ProjectId, TabId, WorktreeId};
pub use project::Project;
pub use tab::{Tab, TabKind};
pub use workspace::{FilteredProject, FilteredTab, FilteredTree, FilteredWorktree, Workspace};
pub use worktree::Worktree;
