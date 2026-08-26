//! Durable storage for what Tiller must remember across launches.
//!
//! A pure-Rust, GPUI-free crate (SQLite via `rusqlite` with the bundled
//! engine) mirroring what the Swift app persists through GRDB in
//! `TillerPersistence`:
//!
//! - **Projects** the user added, with their sidebar order and per-project
//!   settings ([`ProjectRecord`]).
//! - **Worktrees** per project ([`WorktreeRecord`]).
//! - **Tabs** open per worktree, with order and the active one ([`TabRecord`]).
//! - **Rendered chat turns** owned by chat tabs ([`ChatTranscript`]).
//! - **Application settings** under the exact Swift UserDefaults key names,
//!   with the exact Swift defaults ([`AppSettings`], [`settings_keys`]).
//! - **Sidebar UI state**: expanded projects and the selected worktree
//!   ([`SidebarState`]).
//! - **Quarantine records** retaining malformed serialized rows
//!   ([`QuarantinedRecord`]).
//! - **Browser origin grants** remembered by the in-app browser doorhanger.
//!
//! # Schema versioning
//!
//! The schema is versioned with `PRAGMA user_version`. Every
//! [`AppDatabase::open`] migrates the file forward, so an existing database
//! is upgraded on open with its rows intact and a crash mid-migration leaves
//! the old version. Pending migrations run inside one `BEGIN IMMEDIATE`
//! transaction with a busy timeout, so two processes opening the same fresh
//! database serialize instead of racing the schema creation; the initial DDL
//! is idempotent as defense in depth. Adding a schema
//! version is one function appended to [`crate::migrations::MIGRATIONS`] —
//! nothing else changes (the Swift app's migrator was frozen at v17 once;
//! this runner has no such escape hatch).
//!
//! # Failure behavior
//!
//! A corrupt or unreadable database is an error ([`PersistenceError::Corrupt`]
//! or [`PersistenceError::Sqlite`]), never a panic and never a silent reset:
//! the file is left untouched. A database from a *newer* schema version is
//! refused with [`PersistenceError::NewerSchema`]. A physically healthy
//! database can still contain a malformed serialized row; those rows are
//! moved to [`QuarantinedRecord`] atomically and omitted from active reads so
//! one bad tab does not erase its siblings. All writes run in transactions,
//! so a crash mid-save cannot leave a half-written layout.

mod agent_ref;
mod db;
mod error;
mod migrations;
mod model;

/// Maximum logical SQLite database size enforced by the package.
pub const MAX_DATABASE_BYTES: u64 = 64 * 1024 * 1024;

pub use agent_ref::AgentRef;
pub use db::AppDatabase;
pub use error::PersistenceError;
pub use migrations::{CURRENT_SCHEMA_VERSION, migrate_up_to};
pub use model::{
    AgentAccountRecord, AppSettings, AppearanceMode, ChatEntry, ChatPermissionOption,
    ChatPermissionOutcome, ChatPlanEntry, ChatSessionSummary, ChatToolLocation, ChatTranscript,
    ChatTurn,
    FileIconTheme, MAX_CHAT_TRANSCRIPT_BYTES, ProjectRecord, QuarantinedRecord, SidebarState,
    TabRecord, TabStateRecord, WorktreeRecord, settings_keys, settings_ranges,
};
