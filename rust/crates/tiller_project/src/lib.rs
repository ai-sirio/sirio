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

mod create;
mod discovery;
mod domain;
mod error;
mod file;
mod file_icon;
mod file_link;
mod git;
mod id;
mod layout;
mod project;
mod settings;
mod skill;
mod tab;
mod ui;
mod workspace;
mod worktree;

pub use create::{ProjectCreationError, create_project};
pub use discovery::{
    DiscoveredProject, DiscoveredWorktree, current_branch, discover_project, discover_worktrees,
    is_git_repository, parse_worktree_list,
};
pub use domain::{
    AutoNamingThrottle, OnceGate, default_project_base, move_item, move_tab, numeric_tab_selection,
    resolve_worktree_defaults,
};
pub use error::GitError;
pub use file::{
    DroppedFile, DroppedPath, FileDropError, MAX_DROPPED_IMAGE_BYTES, classify_file_drop,
    shell_quote_path, terminal_file_drop,
};
pub use file::{FileTreeEntry, FileTreeError, load_file_tree, validate_relative_path};
pub use file_icon::FileIconKey;
pub use file_link::{FileLinkTarget, is_markdown_path, resolve_file_link};
pub use id::{ProjectId, TabId, WorktreeId};
pub use layout::{
    ContentKind, FocusIntent, LayoutCommand, LayoutError, LayoutNode, LayoutTransition,
    LegacyWorkspaceTab, PaneGroup, SnapshotError, SplitAxis, WorkspaceLayout, WorkspaceSnapshot,
    WorkspaceTab, WorkspaceTabViewState, browser_content_id, classify_layout_command,
    document_content_id, terminal_content_id, worktree_content_id,
};
pub use project::Project;
pub use settings::SettingsPolicy;
pub use skill::{SkillInstallCommand, agent_skill_install_command};
pub use tab::{Tab, TabKind};
pub use ui::{AppearanceMode, UpdateEvent, UpdateState};
pub use workspace::{FilteredProject, FilteredTab, FilteredTree, FilteredWorktree, Workspace};
pub use worktree::Worktree;
