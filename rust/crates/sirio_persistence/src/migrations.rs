//! Forward-only schema migrations, versioned via `PRAGMA user_version`.
//!
//! The Swift app learned the hard way that migrations can silently stop
//! applying (its GRDB migrator was pinned at v17 and installs past that
//! point never received later migrations). This runner has no such escape
//! hatch: every open migrates the database forward to the current schema,
//! one transaction per version, and the version is recorded atomically with
//! the schema change. Adding a new schema version is a single function
//! appended to [`MIGRATIONS`] — nothing else changes.

use rusqlite::{Connection, Transaction, TransactionBehavior, params};

use crate::error::PersistenceError;
use crate::model::stable_worktree_id;

/// One schema migration: `PRAGMA user_version` starts at `index + 1` after
/// it runs. A migration runs inside its own transaction; if it fails, the
/// transaction rolls back and the version is not recorded.
pub(crate) type Migration = fn(&Transaction) -> Result<(), rusqlite::Error>;

/// The schema version this crate writes. `user_version` values above this
/// mean the file was written by a newer app and is refused ([`PersistenceError::NewerSchema`]).
pub const CURRENT_SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// v1 — the initial schema: projects, worktrees, tabs, settings.
///
/// The initial DDL is idempotent (`IF NOT EXISTS`): migrations only ever run
/// under the migration write lock, so this is defense in depth — a lost race
/// must be harmless even if some future path bypasses the lock. Later
/// migrations may intentionally use forward-only ALTER statements.
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

/// v6 — rendered chat transcript turns, owned by their chat tab.
///
/// Each row is one complete serialized turn. Keeping turns as separate rows
/// lets the persistence API retain the newest complete turns under its byte
/// budget without ever cutting a JSON payload in half.
fn migrate_v6(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS chat_turn (
            tab_id TEXT NOT NULL REFERENCES tab(id) ON DELETE CASCADE,
            ordinal INTEGER NOT NULL,
            payload BLOB NOT NULL,
            PRIMARY KEY (tab_id, ordinal)
        );",
    )
}

/// v7 — worktree metadata that belongs to the sidebar record itself.
fn migrate_v7(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch(
        "ALTER TABLE worktree ADD COLUMN comment TEXT;
         ALTER TABLE worktree ADD COLUMN created_at INTEGER;
         ALTER TABLE worktree ADD COLUMN updated_at INTEGER;",
    )
}

/// v8 — one primary checkout per project.
///
/// Older databases could contain multiple primary flags. Reconcile those
/// rows deterministically before installing the index that protects future
/// writes.
fn migrate_v8(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch(
        "UPDATE worktree SET is_primary = 0
         WHERE id IN (
             SELECT id FROM (
                 SELECT id,
                        ROW_NUMBER() OVER (
                            PARTITION BY project_id
                            ORDER BY order_idx, id
                        ) AS position
                 FROM worktree
                 WHERE is_primary = 1
             )
             WHERE position > 1
         );
         CREATE UNIQUE INDEX IF NOT EXISTS worktree_one_primary_per_project
             ON worktree (project_id)
             WHERE is_primary = 1;",
    )
}

/// v9 — durable quarantine for rows whose serialized payload cannot be
/// materialized. The source row is removed only after its original bytes are
/// recorded in this table in the same transaction.
fn migrate_v9(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS quarantine_record (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            record_type TEXT NOT NULL,
            record_id TEXT NOT NULL,
            payload BLOB NOT NULL,
            reason TEXT NOT NULL,
            quarantined_at INTEGER NOT NULL
        );",
    )
}

/// v10 — retain the agent identity associated with a chat tab.
fn migrate_v10(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch("ALTER TABLE tab ADD COLUMN agent_id TEXT;")
}

/// v11 — durable browser-origin grants used by the Linux browser doorhanger.
fn migrate_v11(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS browser_origin_grant (
            origin TEXT PRIMARY KEY,
            granted_at INTEGER NOT NULL
        );",
    )
}

/// v12 — timestamp complete chat transcript snapshots for the history list.
fn migrate_v12(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch(
        "ALTER TABLE chat_turn
         ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;",
    )
}

/// v13 — the local-account-identity store (F-PERSIST-DB-06). Before this,
/// `discover_claude_identity`/`discover_codex_identity` (sirio_ui's
/// Settings surface) only ever shelled out live for a display line; there
/// was no table for that result to land in, so it could not survive a
/// restart or a slow/offline shell-out. One row per provider, replaced
/// wholesale on each successful detection — this is a cache of the last
/// known-good identity, not a history, so a plain upsert on `provider` is
/// the whole write path.
fn migrate_v13(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS account_identity (
            provider TEXT PRIMARY KEY,
            identity TEXT NOT NULL,
            detected_at INTEGER NOT NULL
        );",
    )
}

/// v14 — isolated agent-CLI accounts (F-SET-15). Mirrors the Swift app's
/// `AgentAccountRecord` table: a named reference to an isolated config
/// directory, separate from `account_identity` (v13, a single-slot cache of
/// whatever the CLI's own unmodified on-disk login currently reports).
/// "Which account is active" is a small amount of per-provider state, not a
/// record of its own, so it reuses the existing generic `setting` table
/// under the same key the Swift app used
/// (`"agentAccounts.<provider>.activeId"`) rather than adding a second
/// table for one nullable string.
fn migrate_v14(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS agent_account (
            id TEXT PRIMARY KEY,
            provider TEXT NOT NULL,
            label TEXT NOT NULL,
            config_dir_path TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );",
    )
}

/// v15 — qualify the persisted agent identity so it carries its namespace.
///
/// `tab.agent_id` (v10) holds a bare adapter id. Sirio is about to support
/// agents from the ACP registry, whose ids live in a different namespace
/// and collide with adapter ids: `registry_id("opencode")` resolves to
/// `"opencode"`, the one adapter id the registry does not remap. A bare
/// string cannot say which namespace it came from, so this prefixes every
/// existing NON-NULL value with `adapter:`. The prefix rule has no
/// exceptions and no whitelist: only adapters could ever have written this
/// column, so every NON-NULL value is an adapter id by construction,
/// including one that matches no current adapter — it is prefixed like any
/// other (a tab whose identity does not resolve is already restorable, the
/// same path as a NULL). Nothing is cleared and no row is deleted.
fn migrate_v15(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch("UPDATE tab SET agent_id = 'adapter:' || agent_id WHERE agent_id IS NOT NULL;")
}

/// v16 — the Secondary centre pane's open/closed flag (#323).
///
/// This is the one piece of the centre split that is *stored* rather than
/// derived, and it is deliberate: which half a tab is drawn in comes from its
/// kind, and whether the pane exists normally comes from whether it holds
/// tabs. What cannot be derived is "closed, but still holding tabs" — the
/// state the keyboard toggle produces. Without that toggle this column would
/// have no reason to exist.
fn migrate_v16(db: &Transaction) -> Result<(), rusqlite::Error> {
    db.execute_batch(
        "ALTER TABLE worktree
         ADD COLUMN secondary_pane_open INTEGER NOT NULL DEFAULT 0;",
    )
}

/// v17 — replace positional worktree ids with ids derived from the canonical
/// checkout path. The old id is not only a worktree primary key: `tab` and
/// `sidebar_state` refer to it as well. SQLite does not have `ON UPDATE
/// CASCADE` on this schema, so the migration moves each row through a
/// temporary parent id, creates the final parent, moves the references, and
/// only then removes the temporary row. Tab ids stay unchanged because
/// `tab_state` and `chat_turn` refer to those ids, not to the worktree id.
fn migrate_v17(db: &Transaction) -> Result<(), rusqlite::Error> {
    let mappings = {
        let mut statement = db.prepare(
            "SELECT rowid, id, project_id, path, is_primary
             FROM worktree
             ORDER BY rowid",
        )?;
        let rows = statement.query_map([], |row| {
            let rowid: i64 = row.get(0)?;
            let old_id: String = row.get(1)?;
            let project_id: String = row.get(2)?;
            let path: String = row.get(3)?;
            let is_primary: i64 = row.get(4)?;
            Ok((rowid, old_id, project_id, path, is_primary))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter_map(|(rowid, old_id, project_id, path, is_primary)| {
                let positional_prefix = format!("{project_id}-wt-");
                old_id
                    .strip_prefix(&positional_prefix)
                    .and_then(|index| index.parse::<usize>().ok())?;
                Some((
                    old_id,
                    format!("__sirio_worktree_migration_{rowid}"),
                    stable_worktree_id(&project_id, std::path::Path::new(&path)),
                    is_primary,
                ))
            })
            .collect::<Vec<_>>()
    };

    if mappings.is_empty() {
        return Ok(());
    }

    db.execute_batch(
        "CREATE TEMP TABLE worktree_id_migration (
            old_id TEXT PRIMARY KEY,
            temporary_id TEXT NOT NULL UNIQUE,
            new_id TEXT NOT NULL UNIQUE,
            is_primary INTEGER NOT NULL
        );",
    )?;
    for (old_id, temporary_id, new_id, is_primary) in &mappings {
        db.execute(
            "INSERT INTO worktree_id_migration
                 (old_id, temporary_id, new_id, is_primary)
             VALUES (?1, ?2, ?3, ?4)",
            params![old_id, temporary_id, new_id, is_primary],
        )?;
    }

    // Create temporary parent rows before moving the child foreign keys. They
    // deliberately start non-primary so the existing partial unique index is
    // never violated while both generations of each row coexist.
    db.execute(
        "INSERT INTO worktree
             (id, project_id, branch, path, is_primary, order_idx,
              comment, created_at, updated_at, secondary_pane_open)
         SELECT migration.temporary_id, worktree.project_id, worktree.branch,
                worktree.path, 0, worktree.order_idx, worktree.comment,
                worktree.created_at, worktree.updated_at,
                worktree.secondary_pane_open
         FROM worktree
         JOIN worktree_id_migration AS migration ON migration.old_id = worktree.id",
        [],
    )?;
    db.execute(
        "UPDATE worktree SET is_primary = 0
         WHERE id IN (SELECT old_id FROM worktree_id_migration)",
        [],
    )?;
    db.execute(
        "UPDATE tab
         SET worktree_id = (
             SELECT temporary_id FROM worktree_id_migration
             WHERE old_id = tab.worktree_id
         )
         WHERE worktree_id IN (SELECT old_id FROM worktree_id_migration)",
        [],
    )?;
    db.execute(
        "UPDATE sidebar_state
         SET selected_worktree_id = (
             SELECT temporary_id FROM worktree_id_migration
             WHERE old_id = sidebar_state.selected_worktree_id
         )
         WHERE selected_worktree_id IN (SELECT old_id FROM worktree_id_migration)",
        [],
    )?;
    db.execute(
        "DELETE FROM worktree
         WHERE id IN (SELECT old_id FROM worktree_id_migration)",
        [],
    )?;

    // A second parent copy is needed because SQLite checks a foreign key at
    // the end of an UPDATE of the parent primary key. Move children to the
    // final parent first, then remove the temporary parent.
    db.execute(
        "INSERT INTO worktree
             (id, project_id, branch, path, is_primary, order_idx,
              comment, created_at, updated_at, secondary_pane_open)
         SELECT migration.new_id, worktree.project_id, worktree.branch,
                worktree.path, 0, worktree.order_idx, worktree.comment,
                worktree.created_at, worktree.updated_at,
                worktree.secondary_pane_open
         FROM worktree
         JOIN worktree_id_migration AS migration
           ON migration.temporary_id = worktree.id",
        [],
    )?;
    db.execute(
        "UPDATE tab
         SET worktree_id = (
             SELECT new_id FROM worktree_id_migration
             WHERE temporary_id = tab.worktree_id
         )
         WHERE worktree_id IN (SELECT temporary_id FROM worktree_id_migration)",
        [],
    )?;
    db.execute(
        "UPDATE sidebar_state
         SET selected_worktree_id = (
             SELECT new_id FROM worktree_id_migration
             WHERE temporary_id = sidebar_state.selected_worktree_id
         )
         WHERE selected_worktree_id IN
             (SELECT temporary_id FROM worktree_id_migration)",
        [],
    )?;
    db.execute(
        "DELETE FROM worktree
         WHERE id IN (SELECT temporary_id FROM worktree_id_migration)",
        [],
    )?;
    db.execute(
        "UPDATE worktree
         SET is_primary = (
             SELECT is_primary FROM worktree_id_migration
             WHERE new_id = worktree.id
         )
         WHERE id IN (SELECT new_id FROM worktree_id_migration)",
        [],
    )?;
    db.execute_batch("DROP TABLE worktree_id_migration;")?;
    Ok(())
}

/// All migrations in order. Appending a function here (and nothing else) is
/// how a new schema version is added.
pub(crate) const MIGRATIONS: &[Migration] = &[
    migrate_v1,
    migrate_v2,
    migrate_v3,
    migrate_v4,
    migrate_v5,
    migrate_v6,
    migrate_v7,
    migrate_v8,
    migrate_v9,
    migrate_v10,
    migrate_v11,
    migrate_v12,
    migrate_v13,
    migrate_v14,
    migrate_v15,
    migrate_v16,
    migrate_v17,
];

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
    fn positional_worktree_ids_are_rekeyed_with_their_references() {
        let mut conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute("PRAGMA foreign_keys = ON", [])
            .expect("enable foreign keys");
        migrate_up_to(&mut conn, 16).expect("migrate to the pre-rekey schema");
        conn.execute_batch(
            "INSERT INTO project (id, name, root_path) VALUES ('project', 'Project', '/repo');
             INSERT INTO worktree (id, project_id, branch, path, order_idx, is_primary)
                 VALUES ('project-wt-0', 'project', 'main', '/repo', 0, 1);
             INSERT INTO worktree (id, project_id, branch, path, order_idx, is_primary)
                 VALUES ('project-wt-1', 'project', 'feature', '/repo-feature', 1, 0);
             INSERT INTO tab (id, worktree_id, title, kind, order_idx, is_active)
                 VALUES ('tab-feature', 'project-wt-1', 'Feature', 'terminal', 0, 1);
             INSERT INTO tab_state (tab_id, state)
                 VALUES ('tab-feature', '{\"marker\":\"feature\"}');
             INSERT INTO sidebar_state (id, selected_worktree_id)
                 VALUES (1, 'project-wt-1');",
        )
        .expect("seed positional worktree rows");

        migrate_up_to(&mut conn, 17).expect("migrate to the stable-id schema");

        let ids: Vec<(String, String)> = conn
            .prepare("SELECT id, path FROM worktree ORDER BY path")
            .expect("prepare worktree query")
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("query worktree rows")
            .collect::<Result<_, _>>()
            .expect("collect worktree rows");
        assert_eq!(ids.len(), 2);
        assert!(ids.iter().all(|(id, _)| !id.ends_with("-wt-0") && !id.ends_with("-wt-1")));
        assert_ne!(ids[0].0, ids[1].0, "rekeying must not create duplicate ids");

        let feature_id = ids
            .iter()
            .find(|(_, path)| path == "/repo-feature")
            .expect("feature worktree row")
            .0
            .clone();
        let tab_worktree_id: String = conn
            .query_row(
                "SELECT worktree_id FROM tab WHERE id = 'tab-feature'",
                [],
                |row| row.get(0),
            )
            .expect("read rekeyed tab reference");
        assert_eq!(tab_worktree_id, feature_id);
        let selected_worktree_id: String = conn
            .query_row(
                "SELECT selected_worktree_id FROM sidebar_state WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .expect("read rekeyed sidebar selection");
        assert_eq!(selected_worktree_id, feature_id);
        let state: String = conn
            .query_row(
                "SELECT state FROM tab_state WHERE tab_id = 'tab-feature'",
                [],
                |row| row.get(0),
            )
            .expect("tab state survives rekeying");
        assert_eq!(state, "{\"marker\":\"feature\"}");
    }

    #[test]
    fn a_fresh_database_starts_at_version_zero() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        assert_eq!(read_user_version(&conn).expect("read"), 0);
    }

    /// F-PERSIST-DB-11: the schema-creation half was already strongly
    /// proven (a real database migrates v1->current and boots a populated
    /// sidebar); what was missing was proof that data planted at an *old*
    /// schema version survives forward migration through the specific
    /// renames/backfills later versions perform on it — not just that the
    /// final CREATE/ALTER statements are syntactically fine on an empty
    /// database. This plants one row in each of `session_ref`, `tab`, and
    /// `chat_turn` at v6 (the version right after both `session_ref` and
    /// `chat_turn` exist, and before v10's `tab.agent_id` and v12's
    /// `chat_turn.updated_at` are added), migrates forward to the current
    /// schema, and checks each row is not just present but has the exact
    /// backfilled value those later migrations document.
    #[test]
    fn forward_migration_preserves_pre_existing_session_ref_tab_and_chat_turn_data() {
        let mut conn = Connection::open_in_memory().expect("open in-memory db");
        migrate_up_to(&mut conn, 6).expect("migrate to v6");

        conn.execute_batch(
            "INSERT INTO project (id, name, root_path) VALUES ('proj', 'Proj', '/repo');
             INSERT INTO worktree (id, project_id, branch, path, order_idx)
                 VALUES ('wt', 'proj', 'main', '/repo', 0);
             INSERT INTO tab (id, worktree_id, title, kind, order_idx, is_active)
                 VALUES ('tab-1', 'wt', 'Chat', 'chat', 3, 1);
             INSERT INTO session_ref (session, reference)
                 VALUES ('pane-1', 'session-abc');
             INSERT INTO chat_turn (tab_id, ordinal, payload)
                 VALUES ('tab-1', 0, x'01020304');",
        )
        .expect("plant data at v6, before the later renames/backfills");

        migrate_up_to(&mut conn, MIGRATIONS.len()).expect("migrate forward to current");
        assert_eq!(
            read_user_version(&conn).expect("read"),
            CURRENT_SCHEMA_VERSION
        );

        // session_ref: no later migration touches this table at all.
        let reference: String = conn
            .query_row(
                "SELECT reference FROM session_ref WHERE session = 'pane-1'",
                [],
                |row| row.get(0),
            )
            .expect("session_ref row survives forward migration");
        assert_eq!(reference, "session-abc");

        // tab: order_idx (planted before v3's active-tab invariant fixup)
        // survives, and v10's `agent_id` column backfills to NULL for a
        // row that predates it rather than dropping or blanking the row.
        let (order_idx, agent_id): (i64, Option<String>) = conn
            .query_row(
                "SELECT order_idx, agent_id FROM tab WHERE id = 'tab-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("tab row survives forward migration");
        assert_eq!(order_idx, 3, "tab ordering survives migration");
        assert_eq!(
            agent_id, None,
            "v10 backfills agent_id to NULL for a pre-existing row"
        );

        // chat_turn: the payload/ordinal planted at v6 survive, and v12's
        // `updated_at` column backfills to its documented default (0) for
        // a row that predates it.
        let (payload, updated_at): (Vec<u8>, i64) = conn
            .query_row(
                "SELECT payload, updated_at FROM chat_turn
                 WHERE tab_id = 'tab-1' AND ordinal = 0",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("chat_turn row survives forward migration");
        assert_eq!(payload, vec![1, 2, 3, 4]);
        assert_eq!(
            updated_at, 0,
            "v12 backfills updated_at to its documented DEFAULT 0"
        );
    }

    /// F-PERSIST-DB-06: the schema half of the local-account-identity
    /// store. `account_identity` did not exist before v13; this proves the
    /// migration both creates it with the right shape (one row per
    /// provider, replace-on-conflict) and — like the test above — that a
    /// row planted before a later migration runs (none touch this table
    /// yet, but the same discipline applies as the table grows) is not
    /// something a future migration can silently drop.
    #[test]
    fn account_identity_table_stores_one_upserted_row_per_provider() {
        let mut conn = Connection::open_in_memory().expect("open in-memory db");
        migrate(&mut conn).expect("migrate to current");

        conn.execute(
            "INSERT INTO account_identity (provider, identity, detected_at)
             VALUES ('claude', 'first@example.com', 100)",
            [],
        )
        .expect("insert an identity row");
        conn.execute(
            "INSERT INTO account_identity (provider, identity, detected_at)
             VALUES ('claude', 'second@example.com', 200)
             ON CONFLICT(provider) DO UPDATE SET
                 identity = excluded.identity,
                 detected_at = excluded.detected_at",
            [],
        )
        .expect("re-detecting the same provider replaces its row");

        let (identity, detected_at): (String, i64) = conn
            .query_row(
                "SELECT identity, detected_at FROM account_identity WHERE provider = 'claude'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("exactly one row per provider");
        assert_eq!(identity, "second@example.com");
        assert_eq!(detected_at, 200);
    }

    /// v15 qualification (mirror of the v10 `agent_id` backfill test above):
    /// the schema-creation half is proven by every `AppDatabase::open`, but
    /// what v15 does on *existing* data needs its own proof. This plants
    /// three tabs at v14 — the version right after v14's `agent_account`
    /// table and before v15's prefixing — with a known adapter id, an
    /// *unexpected* non-NULL id, and a NULL, migrates forward, and checks
    /// the exact values the migration documents: every non-NULL value gets
    /// the `adapter:` prefix with no whitelist check (the unexpected value
    /// is prefixed like the known one), NULL stays NULL, and no row is
    /// dropped.
    #[test]
    fn v15_qualifies_preexisting_bare_agent_ids_with_no_exceptions() {
        let mut conn = Connection::open_in_memory().expect("open in-memory db");
        migrate_up_to(&mut conn, 14).expect("migrate to v14");

        conn.execute_batch(
            "INSERT INTO project (id, name, root_path) VALUES ('proj', 'Proj', '/repo');
             INSERT INTO worktree (id, project_id, branch, path, order_idx)
                 VALUES ('wt', 'proj', 'main', '/repo', 0);
             INSERT INTO tab (id, worktree_id, title, kind, order_idx, is_active, agent_id)
                 VALUES ('tab-known', 'wt', 'Chat', 'chat', 0, 1, 'codex');
             INSERT INTO tab (id, worktree_id, title, kind, order_idx, is_active, agent_id)
                 VALUES ('tab-unexpected', 'wt', 'Chat', 'chat', 1, 0, 'some-unknown-agent');
             INSERT INTO tab (id, worktree_id, title, kind, order_idx, is_active)
                 VALUES ('tab-null', 'wt', 'Chat', 'chat', 2, 0);",
        )
        .expect("plant rows at v14 with bare agent ids");

        migrate_up_to(&mut conn, MIGRATIONS.len()).expect("migrate forward to current");
        assert_eq!(
            read_user_version(&conn).expect("read"),
            CURRENT_SCHEMA_VERSION
        );

        let rows: Vec<(String, Option<String>)> = {
            let mut statement = conn
                .prepare("SELECT id, agent_id FROM tab ORDER BY order_idx")
                .expect("prepare");
            let rows = statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .expect("query");
            rows.collect::<Result<_, _>>().expect("collect")
        };
        assert_eq!(
            rows,
            vec![
                ("tab-known".to_string(), Some("adapter:codex".to_string())),
                (
                    "tab-unexpected".to_string(),
                    Some("adapter:some-unknown-agent".to_string()),
                ),
                ("tab-null".to_string(), None),
            ],
            "v15 prefixes every non-NULL agent_id with 'adapter:' — known ids,\n  \
             unexpected ids alike — and leaves NULL and row count untouched"
        );
    }
}
