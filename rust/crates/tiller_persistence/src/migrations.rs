//! Forward-only schema migrations, versioned via `PRAGMA user_version`.
//!
//! The Swift app learned the hard way that migrations can silently stop
//! applying (its GRDB migrator was pinned at v17 and installs past that
//! point never received later migrations). This runner has no such escape
//! hatch: every open migrates the database forward to the current schema,
//! one transaction per version, and the version is recorded atomically with
//! the schema change. Adding a new schema version is a single function
//! appended to [`MIGRATIONS`] — nothing else changes.

use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::error::PersistenceError;

/// One schema migration: `PRAGMA user_version` starts at `index + 1` after
/// it runs. A migration runs inside its own transaction; if it fails, the
/// transaction rolls back and the version is not recorded.
pub(crate) type Migration = fn(&Transaction) -> Result<(), rusqlite::Error>;

/// The schema version this crate writes. `user_version` values above this
/// mean the file was written by a newer app and is refused ([`PersistenceError::NewerSchema`]).
pub const CURRENT_SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// v1 — the initial schema: projects, worktrees, tabs, settings.
///
/// Every statement is idempotent (`IF NOT EXISTS`): migrations only ever
/// run under the migration write lock, so this is defense in depth — a lost
/// race must be harmless even if some future path bypasses the lock.
fn migrate_v1(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS project (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            root_path TEXT NOT NULL,
            color_hex TEXT,
            display_name TEXT,
            icon_kind TEXT NOT NULL DEFAULT 'icon',
            icon_value TEXT,
            avatar_image BLOB,
            default_worktree_base TEXT,
            worktree_location_override TEXT,
            order_idx INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS worktree (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
            branch TEXT NOT NULL,
            path TEXT NOT NULL,
            is_primary INTEGER NOT NULL DEFAULT 0,
            order_idx INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS tab (
            id TEXT PRIMARY KEY,
            worktree_id TEXT NOT NULL REFERENCES worktree(id) ON DELETE CASCADE,
            title TEXT NOT NULL,
            kind TEXT NOT NULL,
            order_idx INTEGER NOT NULL,
            is_active INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS setting (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )
}

/// v2 — sidebar UI state: the selected worktree (singleton row) and the
/// expanded projects (child table, cascade-deleted with their project).
fn migrate_v2(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS sidebar_state (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            selected_worktree_id TEXT
        );
        CREATE TABLE IF NOT EXISTS sidebar_expanded_project (
            project_id TEXT PRIMARY KEY REFERENCES project(id) ON DELETE CASCADE
        );",
    )
}

/// v3 — enforce the single-active-tab invariant in the schema itself.
///
/// `is_active` used to be a plain flag: `save_tabs` always normalized it, but
/// the single-tab `save_tab` path could leave two active tabs for one
/// worktree, making "which tab is active" ambiguous on restore. v3 first
/// brings any existing database that already violates the invariant back to
/// one active tab per worktree — keeping the first in tab order
/// (`order_idx`, then `id`), the same rule `save_tabs` applies — then pins
/// the invariant with a partial unique index that SQLite itself enforces:
/// any future write path that tries to persist a second active tab per
/// worktree fails loudly instead of silently writing an ambiguous state.
fn migrate_v3(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch(
        "UPDATE tab SET is_active = 0
         WHERE id IN (
             SELECT id FROM (
                 SELECT id,
                        ROW_NUMBER() OVER (
                            PARTITION BY worktree_id
                            ORDER BY order_idx, id
                        ) AS position
                 FROM tab
                 WHERE is_active = 1
             )
             WHERE position > 1
         );
         CREATE UNIQUE INDEX IF NOT EXISTS tab_one_active_per_worktree
             ON tab (worktree_id)
             WHERE is_active = 1;",
    )
}

/// v4 — opaque per-tab surface state. The application owns the JSON payload
/// (pane tree history and, once the terminal seam is available, scrollback),
/// while SQLite owns its lifetime alongside the tab row.
fn migrate_v4(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS tab_state (
            tab_id TEXT PRIMARY KEY REFERENCES tab(id) ON DELETE CASCADE,
            state TEXT NOT NULL
        );",
    )
}

/// v5 — stable agent session references reported by control hooks. The pane
/// id is the key agents use when they reconnect after a process restart.
fn migrate_v5(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS session_ref (
            session TEXT PRIMARY KEY,
            reference TEXT NOT NULL
        );",
    )
}

/// All migrations in order. Appending a function here (and nothing else) is
/// how a new schema version is added.
pub(crate) const MIGRATIONS: &[Migration] =
    &[migrate_v1, migrate_v2, migrate_v3, migrate_v4, migrate_v5];

/// Migrates `conn` forward to [`CURRENT_SCHEMA_VERSION`]. Databases already
/// current, or older, are handled; a database from a *newer* schema version
/// is refused. Safe against concurrent openers from other processes (see
/// [`migrate_up_to`]).
pub(crate) fn migrate(conn: &mut Connection) -> Result<(), PersistenceError> {
    migrate_up_to(conn, MIGRATIONS.len())
}

/// Like [`migrate`], but stops after version `up_to` (inclusive). Used by
/// tests to produce a database at an older schema version, and by callers
/// that need a staged migration.
///
/// Cross-process safety: the version check happens twice. The first read is
/// a cheap, lock-free fast path — an already-migrated database (the common
/// case) is handled without taking any write lock, so concurrent readers of
/// a current database never contend. When migrations are actually pending,
/// the whole remainder runs inside a single `BEGIN IMMEDIATE` transaction:
/// the write lock is taken before the version is re-read, so two processes
/// opening the same fresh database serialize here — the loser blocks (the
/// connection's `busy_timeout`) and, once the winner commits, finds the
/// schema already current and skips every migration instead of racing the
/// CREATE statements.
pub fn migrate_up_to(conn: &mut Connection, up_to: usize) -> Result<(), PersistenceError> {
    let up_to = up_to.min(MIGRATIONS.len());
    let current = read_user_version(conn)?;
    if current > MIGRATIONS.len() as i64 {
        return Err(PersistenceError::NewerSchema {
            version: current,
            supported: MIGRATIONS.len() as i64,
        });
    }
    if current >= up_to as i64 {
        return Ok(());
    }

    // Migrations are pending. Take the write lock up front (not the deferred
    // transaction the per-version loop used before, which let two processes
    // read the same old version and race the CREATE statements), then re-read
    // the version under the lock: a concurrent opener may have migrated and
    // committed while we were waiting for it.
    let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_user_version(&transaction)?;
    if current > MIGRATIONS.len() as i64 {
        return Err(PersistenceError::NewerSchema {
            version: current,
            supported: MIGRATIONS.len() as i64,
        });
    }
    for (index, migration) in MIGRATIONS.iter().enumerate() {
        let version = (index + 1) as i64;
        if version <= current || version > up_to as i64 {
            continue;
        }
        migration(&transaction)?;
    }
    // The version is recorded inside the migration's transaction, so a
    // crash mid-migration leaves the old version and the old schema.
    transaction.pragma_update(None, "user_version", up_to as i64)?;
    transaction.commit()?;
    Ok(())
}

/// Reads `PRAGMA user_version` (0 for a fresh database).
fn read_user_version(conn: &Connection) -> Result<i64, PersistenceError> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(PersistenceError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_version_is_the_migration_count() {
        assert_eq!(CURRENT_SCHEMA_VERSION, MIGRATIONS.len() as i64);
    }

    #[test]
    fn a_fresh_database_starts_at_version_zero() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        assert_eq!(read_user_version(&conn).expect("read"), 0);
    }
}
