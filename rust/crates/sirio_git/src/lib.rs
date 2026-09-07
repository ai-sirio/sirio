//! The model behind Sirio's Changes panel: git status, unified diffs, line
//! counts, and the stage/unstage/discard mutations.
//!
//! This crate is deliberately dependency-free and UI-free: the public API is
//! plain value types (`Clone`, `Debug`, `PartialEq`, no interior mutability)
//! and `std::process`-based git shell-out, so the terminal, the control
//! socket and the GPUI panels can all build on it without inheriting a UI
//! framework.
//!
//! # Status
//!
//! [`status::status`] runs `git status --porcelain=v2 -z` and parses it into
//! a [`status::StatusSnapshot`]: per path, its index (staged) and worktree
//! states, its rename source, and the derived staged / changes / untracked /
//! conflicted views the panel shows.
//!
//! # Diffs
//!
//! [`diff::diff_entry`] loads the unified diff for one status entry into
//! hunks ([`diff::Hunk`]) of lines ([`diff::DiffLine`]) that carry both line
//! numbers; [`diff::stats`] returns the per-path added/removed counts the
//! panel renders as `-3 +25`.
//!
//! # Mutations
//!
//! [`actions`] stages, unstages and discards a single path or everything at
//! once, mirroring the Swift app's `GitActions`.
//!
//! # Correctness notes
//!
//! - Porcelain v2 with `-z` reports the index and worktree states
//!   independently and keeps paths as raw bytes, so spaces, non-ASCII and
//!   quoted paths parse without a quoting round-trip.
//! - Untracked files and unborn-HEAD checkouts have no HEAD version to diff
//!   against; they are diffed against `/dev/null` and their line counts come
//!   from disk, exactly as in the Swift app.
//! - CRLF content lines have their trailing `\r` stripped by the parser.
//! - Binary files report zero counts with `is_binary` set.

mod actions;
mod branches;
mod clone;
mod diff;
mod directory_status;
mod error;
mod git;
mod graph;
mod ignored;
mod log;
mod remote;
mod side_by_side;
mod status;
mod worktree;

pub use actions::{GitActions, discard, discard_all, stage, stage_all, unstage};
pub use branches::{GitBranches, list_branches};
pub use clone::{GitClone, clone_repository};
pub use diff::{
    DEFAULT_CONTEXT_LINES, DiffLine, DiffOrigin, DiffStat, FileDiff, Hunk,
    WHOLE_FILE_CONTEXT_LINES, commit_diff_entry, commit_files, diff_entry, parse_diff,
    parse_numstat, stats,
};
pub use directory_status::{DirectoryGitStatus, DirectoryStatusAggregator, directory_statuses};
pub use error::{GitActionError, GitError};
pub use git::{GitCancellationToken, GitCommandResult, GitRunner, run_streaming};
pub use graph::{GraphRow, layout};
pub use ignored::{IgnoredPaths, ignored_paths, parse_ignored};
pub use log::{CommitRecord, GitLog, LogFilter, looks_like_hash, parse_log};
pub use remote::{GitRemote, github_owner, project_name};
pub use side_by_side::{
    DiffSideBySideLine, DiffSideBySideRow, GitDiffSideBySide, GitDiffSideBySideLine,
    GitDiffSideBySideRow, side_by_side_rows,
};
pub use status::{
    StatusEntry, StatusKind, StatusParseError, StatusSnapshot, has_head, parse_status, status,
};
pub use worktree::{
    UpstreamBranch, WorktreeError, create_worktree, delete_remote_branch, derive_worktree_path,
    init_repository, remove_worktree, remove_worktree_and_remote_branch, resolve_parent_directory,
    upstream_of,
};
