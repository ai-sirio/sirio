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
//! - **Application settings** under the exact Swift UserDefaults key names,
//!   with the exact Swift defaults ([`AppSettings`], [`settings_keys`]).
//! - **Sidebar UI state**: expanded projects and the selected worktree
//!   ([`SidebarState`]).
//!
//! # Schema versioning
//!
//! The schema is versioned with `PRAGMA user_version`. Every
//! [`AppDatabase::open`] migrates the file forward, so an existing database
//! is upgraded on open with its rows intact and a crash mid-migration leaves
//! the old version. Pending migrations run inside one `BEGIN IMMEDIATE`
//! transaction with a busy timeout, so two processes opening the same fresh
//! database serialize instead of racing the schema creation; the DDL is
//! idempotent as defense in depth. Adding a schema
//! version is one function appended to [`crate::migrations::MIGRATIONS`] —
//! nothing else changes (the Swift app's migrator was frozen at v17 once;
//! this runner has no such escape hatch).
//!
//! # Failure behavior
//!
//! A corrupt or unreadable database is an error ([`PersistenceError::Corrupt`]
//! or [`PersistenceError::Sqlite`]), never a panic and never a silent reset:
//! the file is left untouched. A database from a *newer* schema version is
//! refused with [`PersistenceError::NewerSchema`]. All writes run in
//! transactions, so a crash mid-save cannot leave a half-written layout.

mod db;
mod error;
mod migrations;
mod model;

pub use db::AppDatabase;
pub use error::PersistenceError;
pub use migrations::{CURRENT_SCHEMA_VERSION, migrate_up_to};
pub use model::{
    AppSettings, AppearanceMode, FileIconTheme, ProjectRecord, SidebarState, TabRecord,
    WorktreeRecord, settings_keys, settings_ranges,
};
