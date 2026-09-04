//! The [`AppDatabase`]: owns the SQLite connection, runs the migrations on
//! open, and exposes the persisted records.
//!
//! Writes are atomic per logical change: every save runs in a transaction,
//! so a crash mid-save cannot leave a half-written layout. The connection is
//! not `Sync`; callers that need multi-threaded access should wrap the
//! database in a `Mutex` (or keep it on one thread, as the GPUI app does).

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use crate::MAX_DATABASE_BYTES;
use crate::agent_ref::AgentRef;
use crate::error::PersistenceError;
use crate::migrations::{CURRENT_SCHEMA_VERSION, migrate};
use crate::model::{
    AgentAccountRecord, AppSettings, AppearanceMode, ChatSessionSummary, ChatTranscript, ChatTurn,
    BaseColor, MAX_CHAT_TRANSCRIPT_BYTES, ProjectRecord, QuarantinedRecord, SidebarState,
    TabRecord, TabStateRecord, WorktreeRecord, settings_keys,
};

/// Durable storage for what Sirio must remember across launches.
#[derive(Debug)]
pub struct AppDatabase {
    conn: Connection,
    /// The database file; `None` for an in-memory database.
    path: Option<PathBuf>,
}

/// Opens a transaction that is going to write, as `BEGIN IMMEDIATE`.
///
/// Every write path in this file goes through here, and none of them may use
/// rusqlite's `unchecked_transaction`, which is `BEGIN DEFERRED`. A deferred
/// transaction takes no lock until its first statement, so one that reads
/// before it writes — `save_tabs` reads the worktree's tab ids first, and it
/// is not alone — holds a read snapshot by the time it asks to write. SQLite
/// refuses that read→write upgrade *without running the busy handler*: waiting
/// while already holding a read lock the peer may need is a deadlock, so it
/// returns `SQLITE_BUSY` immediately. The connection's five-second
/// `busy_timeout` is never consulted on that path, which is exactly how two
/// writer processes managed to fail with "database is locked" despite it.
///
/// `IMMEDIATE` takes the write lock up front, before any read is held. There
/// is then nothing for a peer to be blocked behind, so waiting is safe, the
/// busy handler runs, and the second writer queues instead of failing. The
/// cost is that writers serialize from the `BEGIN` rather than from their
/// first write — which is what we want, since these transactions are short
/// and the alternative is a spurious error.
fn write_transaction(conn: &Connection) -> Result<rusqlite::Transaction<'_>, PersistenceError> {
    Ok(rusqlite::Transaction::new_unchecked(
        conn,
        rusqlite::TransactionBehavior::Immediate,
    )?)
}

impl AppDatabase {
    /// Opens (creating if missing) the database at `path`, migrating it
    /// forward to the current schema. The parent directory must exist.
    ///
    /// A corrupt or unreadable file returns [`PersistenceError::Corrupt`] /
    /// [`PersistenceError::Sqlite`] — it is never panicked on and never
    /// silently discarded: the file is left untouched.
    pub fn open(path: &Path) -> Result<Self, PersistenceError> {
        let _initial_open_lock = acquire_initial_open_lock(path)?;
        validate_existing_file(path)?;
        let conn = Self::open_connection(path)?;
        Ok(Self {
            conn,
            path: Some(path.to_path_buf()),
        })
    }

    /// An in-memory database, migrated to the current schema. Tests use real
    /// files (the failure modes under test only exist on disk), but this is
    /// handy for fixtures.
    pub fn in_memory() -> Result<Self, PersistenceError> {
        let mut conn = Connection::open_in_memory().map_err(|error| {
            classify_open_error(PersistenceError::Sqlite(error), Path::new(":memory:"))
        })?;
        Self::initialize(&mut conn, Path::new(":memory:"))
            .map_err(|error| classify_open_error(error, Path::new(":memory:")))?;
        Ok(Self { conn, path: None })
    }

    /// The database file, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The schema version this database is at after the migrations ran.
    pub fn schema_version(&self) -> Result<i64, PersistenceError> {
        self.conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(PersistenceError::from)
    }

    /// The logical page limit installed on this connection.
    pub fn database_limit_bytes(&self) -> Result<u64, PersistenceError> {
        let page_size: i64 = self
            .conn
            .query_row("PRAGMA page_size", [], |row| row.get(0))?;
        let max_pages: i64 = self
            .conn
            .query_row("PRAGMA max_page_count", [], |row| row.get(0))?;
        let page_size = u64::try_from(page_size).map_err(|_| {
            corrupt(
                self.path.as_deref().unwrap_or(Path::new(":memory:")),
                "database page size is negative",
            )
        })?;
        let max_pages = u64::try_from(max_pages).map_err(|_| {
            corrupt(
                self.path.as_deref().unwrap_or(Path::new(":memory:")),
                "database max page count is negative",
            )
        })?;
        Ok(page_size.saturating_mul(max_pages))
    }

    fn open_connection(path: &Path) -> Result<Connection, PersistenceError> {
        let mut conn = Connection::open(path)
            .map_err(|error| classify_open_error(PersistenceError::Sqlite(error), path))?;
        Self::initialize(&mut conn, path).map_err(|error| classify_open_error(error, path))?;
        Ok(conn)
    }

    /// Per-connection pragmas plus the forward migration.
    fn initialize(conn: &mut Connection, path: &Path) -> Result<(), PersistenceError> {
        // A concurrent opener (another Sirio process, or a CLI invoked by an
        // agent hook) may hold the migration write lock when we arrive;
        // wait for it instead of failing with SQLITE_BUSY. 5s dwarfs any
        // real migration; a writer stuck longer than that is hung, not slow.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        // WAL makes each transaction crash-atomic and lets readers proceed
        // during writes. Persistent in the file; setting it again on open is
        // a no-op.
        let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        install_database_limit(conn, path)?;
        migrate(conn)?;
        verify_integrity(conn, path)?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Projects
    // ------------------------------------------------------------------

    /// All projects, in sidebar order.
    pub fn projects(&self) -> Result<Vec<ProjectRecord>, PersistenceError> {
        let mut statement = self.conn.prepare(
            "SELECT id, name, root_path, order_idx, color_hex, display_name, icon_kind,
                    icon_value, avatar_image, default_worktree_base, worktree_location_override
             FROM project
             ORDER BY order_idx, id",
        )?;
        let rows = statement.query_map([], map_project)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Upserts a single project.
    pub fn save_project(&self, project: &ProjectRecord) -> Result<(), PersistenceError> {
        self.conn.execute(
            "INSERT INTO project (id, name, root_path, order_idx, color_hex, display_name,
                                  icon_kind, icon_value, avatar_image, default_worktree_base,
                                  worktree_location_override)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 root_path = excluded.root_path,
                 order_idx = excluded.order_idx,
                 color_hex = excluded.color_hex,
                 display_name = excluded.display_name,
                 icon_kind = excluded.icon_kind,
                 icon_value = excluded.icon_value,
                 avatar_image = excluded.avatar_image,
                 default_worktree_base = excluded.default_worktree_base,
                 worktree_location_override = excluded.worktree_location_override",
            params![
                project.id,
                project.name,
                project.root_path,
                project.order_idx,
                project.color_hex,
                project.display_name,
                project.icon_kind,
                project.icon_value,
                project.avatar_image,
                project.default_worktree_base,
                project.worktree_location_override,
            ],
        )?;
        Ok(())
    }

    /// Replaces the entire project list in one transaction, assigning
    /// `order_idx` from the slice order. Worktrees and tabs of projects not
    /// in the list are cascade-deleted.
    pub fn save_projects(&self, projects: &[ProjectRecord]) -> Result<(), PersistenceError> {
        let transaction = write_transaction(&self.conn)?;
        transaction.execute("DELETE FROM project", [])?;
        for (index, project) in projects.iter().enumerate() {
            let mut project = project.clone();
            project.order_idx = index as i64;
            insert_project(&transaction, &project)?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Removes a project and everything under it (worktrees, tabs,
    /// sidebar-expansion rows), atomically.
    pub fn remove_project(&self, id: &str) -> Result<(), PersistenceError> {
        self.conn
            .execute("DELETE FROM project WHERE id = ?1", [id])?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Worktrees
    // ------------------------------------------------------------------

    /// All worktrees, ordered by project then position.
    pub fn worktrees(&self) -> Result<Vec<WorktreeRecord>, PersistenceError> {
        let mut statement = self.conn.prepare(
            "SELECT id, project_id, branch, path, is_primary, order_idx,
                    comment, created_at, updated_at, secondary_pane_open
             FROM worktree
             ORDER BY project_id, order_idx, id",
        )?;
        let rows = statement.query_map([], map_worktree)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The worktrees of one project, in order.
    pub fn worktrees_of_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<WorktreeRecord>, PersistenceError> {
        let mut statement = self.conn.prepare(
            "SELECT id, project_id, branch, path, is_primary, order_idx,
                    comment, created_at, updated_at, secondary_pane_open
             FROM worktree
             WHERE project_id = ?1
             ORDER BY order_idx, id",
        )?;
        let rows = statement.query_map([project_id], map_worktree)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Upserts a single worktree.
    pub fn save_worktree(&self, worktree: &WorktreeRecord) -> Result<(), PersistenceError> {
        let transaction = write_transaction(&self.conn)?;
        clear_other_primaries(&transaction, worktree)?;
        transaction.execute(
            "INSERT INTO worktree
                (id, project_id, branch, path, is_primary, order_idx,
                 comment, created_at, updated_at, secondary_pane_open)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                 project_id = excluded.project_id,
                 branch = excluded.branch,
                 path = excluded.path,
                 is_primary = excluded.is_primary,
                 order_idx = excluded.order_idx,
                 comment = excluded.comment,
                 created_at = excluded.created_at,
                 updated_at = excluded.updated_at,
                 secondary_pane_open = excluded.secondary_pane_open",
            params![
                worktree.id,
                worktree.project_id,
                worktree.branch,
                worktree.path,
                worktree.is_primary,
                worktree.order_idx,
                worktree.comment,
                worktree.created_at,
                worktree.updated_at,
                worktree.secondary_pane_open,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Replaces the entire worktree list in one transaction, assigning
    /// `order_idx` from the slice order. Tabs of worktrees not in the list
    /// are cascade-deleted.
    pub fn save_worktrees(&self, worktrees: &[WorktreeRecord]) -> Result<(), PersistenceError> {
        let transaction = write_transaction(&self.conn)?;
        transaction.execute("DELETE FROM worktree", [])?;
        for (index, worktree) in worktrees.iter().enumerate() {
            let mut worktree = worktree.clone();
            worktree.order_idx = index as i64;
            insert_worktree(&transaction, &worktree)?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Removes a worktree and its tabs, atomically.
    pub fn remove_worktree(&self, id: &str) -> Result<(), PersistenceError> {
        self.conn
            .execute("DELETE FROM worktree WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Finds one worktree by exact stored path equality. No canonicalization
    /// or normalization is performed, so callers can distinguish path
    /// spellings that the store recorded separately.
    pub fn worktree_by_path(&self, path: &str) -> Result<Option<WorktreeRecord>, PersistenceError> {
        self.conn
            .query_row(
                "SELECT id, project_id, branch, path, is_primary, order_idx,
                        comment, created_at, updated_at, secondary_pane_open
                 FROM worktree
                 WHERE path = ?1
                 ORDER BY project_id, order_idx, id
                 LIMIT 1",
                [path],
                map_worktree,
            )
            .optional()
            .map_err(PersistenceError::from)
    }

    // ------------------------------------------------------------------
    // Tabs
    // ------------------------------------------------------------------

    /// All tabs, ordered by worktree then position.
    pub fn tabs(&self) -> Result<Vec<TabRecord>, PersistenceError> {
        read_tabs(
            &self.conn,
            "SELECT rowid, id, worktree_id, title, kind, agent_id, order_idx, is_active
             FROM tab
             ORDER BY worktree_id, order_idx, id",
            [],
        )
    }

    /// The tabs of one worktree, in order.
    pub fn tabs_of_worktree(&self, worktree_id: &str) -> Result<Vec<TabRecord>, PersistenceError> {
        read_tabs(
            &self.conn,
            "SELECT rowid, id, worktree_id, title, kind, agent_id, order_idx, is_active
             FROM tab
             WHERE worktree_id = ?1
             ORDER BY order_idx, id",
            [worktree_id],
        )
    }

    /// Upserts a single tab.
    ///
    /// Activating a tab atomically deactivates every other tab of the same
    /// worktree in the same transaction, so the schema's
    /// `tab_one_active_per_worktree` partial unique index never sees an
    /// intermediate state with two active tabs. A caller that forgets the
    /// invariant gets a constraint error, not a silently ambiguous restore
    /// state.
    pub fn save_tab(&self, tab: &TabRecord) -> Result<(), PersistenceError> {
        let transaction = write_transaction(&self.conn)?;
        if tab.is_active {
            transaction.execute(
                "UPDATE tab SET is_active = 0
                 WHERE worktree_id = ?1 AND is_active = 1 AND id != ?2",
                params![tab.worktree_id, tab.id],
            )?;
        }
        // agent_id is stored in its qualified `adapter:<id>`/`registry:<id>`
        // form; encode once here at the write boundary.
        let agent_id = tab.agent_id.as_ref().map(AgentRef::to_db_string);
        transaction.execute(
            "INSERT INTO tab (id, worktree_id, title, kind, agent_id, order_idx, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                 worktree_id = excluded.worktree_id,
                 title = excluded.title,
                 kind = excluded.kind,
                 agent_id = excluded.agent_id,
                 order_idx = excluded.order_idx,
                 is_active = excluded.is_active",
            params![
                tab.id,
                tab.worktree_id,
                tab.title,
                tab.kind,
                agent_id,
                tab.order_idx,
                tab.is_active,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Replaces the tabs of one worktree in a single transaction, assigning
    /// `order_idx` from the slice order and normalizing the active flag: at
    /// most one tab per worktree is active, the first marked active wins.
    ///
    /// Tabs that are still present in `tabs` are updated in place (upsert on
    /// `id`) rather than deleted-and-reinserted: `chat_turn.tab_id` has
    /// `ON DELETE CASCADE` to `tab(id)`, so a delete/insert pair — even for
    /// the *same* tab id within the same transaction — wipes any persisted
    /// chat transcript for that tab. Only tabs no longer present in `tabs`
    /// are deleted, which cascades their transcripts away deliberately.
    pub fn save_tabs(&self, worktree_id: &str, tabs: &[TabRecord]) -> Result<(), PersistenceError> {
        let transaction = write_transaction(&self.conn)?;
        let keep_ids: std::collections::HashSet<&str> =
            tabs.iter().map(|t| t.id.as_str()).collect();
        let stale_ids: Vec<String> = {
            let mut statement = transaction.prepare("SELECT id FROM tab WHERE worktree_id = ?1")?;
            let mut rows = statement.query([worktree_id])?;
            let mut stale = Vec::new();
            while let Some(row) = rows.next()? {
                let id: String = row.get(0)?;
                if !keep_ids.contains(id.as_str()) {
                    stale.push(id);
                }
            }
            stale
        };
        for id in &stale_ids {
            transaction.execute("DELETE FROM tab WHERE id = ?1", [id])?;
        }
        // Clear every existing row's `is_active` flag for this worktree
        // before upserting the new set. `tab_one_active_per_worktree` is a
        // partial unique index checked immediately per statement (SQLite
        // partial indexes cannot be DEFERRABLE), so upserting the new active
        // tab while a *different*, not-yet-updated row still carries the old
        // `is_active = 1` transiently violates it — e.g. when the active tab
        // moves from a later position in `tabs` to an earlier one, the
        // now-active row is upserted before the now-inactive row's flag is
        // cleared. Clearing first removes every stale active flag up front,
        // so each upsert below only ever sets a flag, never races another
        // row still holding one.
        transaction.execute(
            "UPDATE tab SET is_active = 0 WHERE worktree_id = ?1",
            [worktree_id],
        )?;
        let mut active_seen = false;
        for (index, tab) in tabs.iter().enumerate() {
            let mut tab = tab.clone();
            tab.order_idx = index as i64;
            if tab.is_active {
                if active_seen {
                    tab.is_active = false;
                } else {
                    active_seen = true;
                }
            }
            upsert_tab(&transaction, &tab)?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Removes a single tab.
    pub fn remove_tab(&self, id: &str) -> Result<(), PersistenceError> {
        self.conn.execute("DELETE FROM tab WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Replaces opaque state for the tabs of one worktree. The state rows are
    /// foreign-keyed to `tab`, so replacing tabs also removes stale state.
    pub fn save_tab_states(
        &self,
        worktree_id: &str,
        states: &[TabStateRecord],
    ) -> Result<(), PersistenceError> {
        let transaction = write_transaction(&self.conn)?;
        transaction.execute(
            "DELETE FROM tab_state
             WHERE tab_id IN (SELECT id FROM tab WHERE worktree_id = ?1)",
            [worktree_id],
        )?;
        for state in states {
            serde_json::from_str::<serde_json::Value>(&state.state)?;
            transaction.execute(
                "INSERT INTO tab_state (tab_id, state) VALUES (?1, ?2)",
                rusqlite::params![state.tab_id, state.state],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Loads opaque state in the same order as the worktree's tabs.
    pub fn tab_states_of_worktree(
        &self,
        worktree_id: &str,
    ) -> Result<Vec<TabStateRecord>, PersistenceError> {
        let mut statement = self.conn.prepare(
            "SELECT tab_state.rowid, tab_state.tab_id, tab_state.state
             FROM tab_state
             JOIN tab ON tab.id = tab_state.tab_id
             WHERE tab.worktree_id = ?1
             ORDER BY tab.order_idx, tab.id",
        )?;
        let mut rows = statement.query([worktree_id])?;
        let mut valid = Vec::new();
        let mut corrupt = Vec::new();
        while let Some(row) = rows.next()? {
            let rowid: i64 = row.get(0)?;
            let tab_id: rusqlite::Result<String> = row.get(1);
            let state: rusqlite::Result<String> = row.get(2);
            match (tab_id, state) {
                (Ok(tab_id), Ok(state)) => {
                    if serde_json::from_str::<serde_json::Value>(&state).is_ok() {
                        valid.push(TabStateRecord { tab_id, state });
                    } else {
                        corrupt.push(CorruptRow {
                            rowid,
                            record_id: tab_id,
                            payload: state.into_bytes(),
                            reason: "tab state is not valid JSON".to_string(),
                        });
                    }
                }
                (tab_id, state) => {
                    let payload = format!("tab_id={tab_id:?}; state={state:?}").into_bytes();
                    let record_id = tab_id.unwrap_or_else(|_| format!("rowid:{rowid}"));
                    corrupt.push(CorruptRow {
                        rowid,
                        record_id,
                        payload,
                        reason: "tab state row has an invalid SQLite value".to_string(),
                    });
                }
            }
        }
        drop(rows);
        drop(statement);
        quarantine_rows(&self.conn, "tab_state", &corrupt)?;
        Ok(valid)
    }

    // ------------------------------------------------------------------
    // Chat transcripts
    // ------------------------------------------------------------------

    /// Replaces one chat tab's rendered transcript atomically.
    ///
    /// Turns are serialized independently and retained newest-first until
    /// their payload bytes reach [`MAX_CHAT_TRANSCRIPT_BYTES`]. A turn is
    /// either kept whole or omitted; no JSON entry is sliced. The foreign key
    /// makes an unknown tab fail the transaction, and deleting a tab removes
    /// its transcript rows automatically.
    pub fn save_chat_transcript(
        &self,
        transcript: &ChatTranscript,
    ) -> Result<(), PersistenceError> {
        let mut retained = Vec::new();
        let mut retained_bytes = 0usize;
        for turn in transcript.turns.iter().rev() {
            let payload = serde_json::to_vec(turn)?;
            if payload.len() > MAX_CHAT_TRANSCRIPT_BYTES {
                continue;
            }
            if retained_bytes + payload.len() > MAX_CHAT_TRANSCRIPT_BYTES {
                break;
            }
            retained_bytes += payload.len();
            retained.push(payload);
        }
        retained.reverse();
        let updated_at = unix_millis();

        let transaction = write_transaction(&self.conn)?;
        transaction.execute(
            "DELETE FROM chat_turn WHERE tab_id = ?1",
            [&transcript.tab_id],
        )?;
        for (index, payload) in retained.into_iter().enumerate() {
            transaction.execute(
                "INSERT INTO chat_turn (tab_id, ordinal, payload, updated_at)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![transcript.tab_id, index as i64, payload, updated_at],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Lists saved chat tabs for one worktree, newest transcript first.
    pub fn chat_sessions(
        &self,
        worktree_id: &str,
    ) -> Result<Vec<ChatSessionSummary>, PersistenceError> {
        let mut statement = self.conn.prepare(
            "SELECT tab.id, tab.title, tab.agent_id,
                    COUNT(chat_turn.ordinal), MAX(chat_turn.updated_at), tab.order_idx
             FROM tab
             JOIN chat_turn ON chat_turn.tab_id = tab.id
             WHERE tab.worktree_id = ?1 AND tab.kind = 'chat'
             GROUP BY tab.id, tab.title, tab.agent_id, tab.order_idx
             ORDER BY MAX(chat_turn.updated_at) DESC, tab.order_idx, tab.id",
        )?;
        let rows = statement.query_map([worktree_id], |row| {
            let turn_count = row.get::<_, i64>(3)?;
            // The column holds the qualified `adapter:<id>`/`registry:<id>`
            // form; the single decode point is AgentRef::from_db, applied
            // here at the DB boundary so no higher layer re-parses it.
            let agent_id = row
                .get::<_, Option<String>>(2)?
                .map(|s| AgentRef::from_db(&s));
            Ok(ChatSessionSummary {
                tab_id: row.get(0)?,
                title: row.get(1)?,
                agent_id,
                turn_count: usize::try_from(turn_count).unwrap_or(0),
                last_activity: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Deletes one chat transcript while preserving its shell tab record.
    pub fn delete_chat_session(&self, tab_id: &str) -> Result<bool, PersistenceError> {
        Ok(self
            .conn
            .execute("DELETE FROM chat_turn WHERE tab_id = ?1", [tab_id])?
            > 0)
    }

    /// Loads one chat tab's rendered transcript in display order.
    pub fn load_chat_transcript(
        &self,
        tab_id: &str,
    ) -> Result<Option<ChatTranscript>, PersistenceError> {
        let mut statement = self.conn.prepare(
            "SELECT rowid, ordinal, payload
             FROM chat_turn
             WHERE tab_id = ?1
             ORDER BY ordinal",
        )?;
        let mut rows = statement.query([tab_id])?;
        let mut turns = Vec::new();
        let mut corrupt = Vec::new();
        while let Some(row) = rows.next()? {
            let rowid: i64 = row.get(0)?;
            let ordinal: i64 = row.get(1)?;
            let payload: rusqlite::Result<Vec<u8>> = row.get(2);
            match payload {
                Ok(payload) => match serde_json::from_slice::<ChatTurn>(&payload) {
                    Ok(turn) => turns.push(turn),
                    Err(error) => corrupt.push(CorruptRow {
                        rowid,
                        record_id: format!("{tab_id}:{ordinal}"),
                        payload,
                        reason: format!("chat turn JSON could not be decoded: {error}"),
                    }),
                },
                Err(error) => corrupt.push(CorruptRow {
                    rowid,
                    record_id: format!("{tab_id}:{ordinal}"),
                    payload: format!("SQLite payload conversion failed: {error}").into_bytes(),
                    reason: "chat turn row has an invalid SQLite value".to_string(),
                }),
            }
        }
        drop(rows);
        drop(statement);
        quarantine_rows(&self.conn, "chat_turn", &corrupt)?;
        if turns.is_empty() {
            return Ok(None);
        }
        Ok(Some(ChatTranscript {
            tab_id: tab_id.to_string(),
            turns,
        }))
    }

    /// Returns rows preserved after a malformed serialized record was
    /// removed from active state.
    pub fn quarantined_records(&self) -> Result<Vec<QuarantinedRecord>, PersistenceError> {
        let mut statement = self.conn.prepare(
            "SELECT id, record_type, record_id, payload, reason, quarantined_at
             FROM quarantine_record
             ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(QuarantinedRecord {
                id: row.get(0)?,
                record_type: row.get(1)?,
                record_id: row.get(2)?,
                payload: row.get(3)?,
                reason: row.get(4)?,
                quarantined_at: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // ------------------------------------------------------------------
    // Agent session references
    // ------------------------------------------------------------------

    /// Loads the pane-to-agent-session associations used by control hooks.
    pub fn session_refs(&self) -> Result<BTreeMap<String, String>, PersistenceError> {
        let mut statement = self
            .conn
            .prepare("SELECT session, reference FROM session_ref ORDER BY session")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows
            .collect::<Result<Vec<(String, String)>, _>>()?
            .into_iter()
            .collect())
    }

    /// Upserts one pane-to-agent-session association.
    pub fn save_session_ref(&self, session: &str, reference: &str) -> Result<(), PersistenceError> {
        self.conn.execute(
            "INSERT INTO session_ref (session, reference) VALUES (?1, ?2)
             ON CONFLICT(session) DO UPDATE SET reference = excluded.reference",
            params![session, reference],
        )?;
        Ok(())
    }

    /// Removes the pane-to-agent-session association, if present.
    pub fn delete_session_ref(&self, session: &str) -> Result<(), PersistenceError> {
        self.conn
            .execute("DELETE FROM session_ref WHERE session = ?1", [session])?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Browser permissions
    // ------------------------------------------------------------------

    /// Loads browser origins that the user allowed through the GPUI
    /// doorhanger. Origins are returned deterministically for settings and
    /// relaunch snapshots.
    pub fn browser_origin_grants(&self) -> Result<Vec<String>, PersistenceError> {
        let mut statement = self.conn.prepare(
            "SELECT origin
             FROM browser_origin_grant
             ORDER BY origin",
        )?;
        let rows = statement.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Persists one allowed browser origin. The primary key makes repeated
    /// Allow decisions idempotent.
    pub fn save_browser_origin_grant(&self, origin: &str) -> Result<(), PersistenceError> {
        self.conn.execute(
            "INSERT INTO browser_origin_grant (origin, granted_at)
             VALUES (?1, ?2)
             ON CONFLICT(origin) DO UPDATE SET granted_at = excluded.granted_at",
            params![origin, unix_timestamp_millis()],
        )?;
        Ok(())
    }

    /// Revokes one browser origin and reports whether a grant was removed.
    pub fn revoke_browser_origin(&self, origin: &str) -> Result<bool, PersistenceError> {
        Ok(self.conn.execute(
            "DELETE FROM browser_origin_grant WHERE origin = ?1",
            [origin],
        )? > 0)
    }

    /// Revokes all browser-origin grants and reports how many were removed.
    pub fn revoke_all_browser_origins(&self) -> Result<usize, PersistenceError> {
        Ok(self.conn.execute("DELETE FROM browser_origin_grant", [])?)
    }

    // ------------------------------------------------------------------
    // Account identity (F-PERSIST-DB-06)
    // ------------------------------------------------------------------

    /// Returns the last known-good identity detected for a provider
    /// (`"claude"`, `"codex"`, …), and when it was detected, if one has ever
    /// been saved. One row per provider — see migration v13's own comment:
    /// this is a cache of the last successful shell-out, not a history.
    pub fn account_identity(
        &self,
        provider: &str,
    ) -> Result<Option<(String, i64)>, PersistenceError> {
        Ok(self
            .conn
            .query_row(
                "SELECT identity, detected_at FROM account_identity WHERE provider = ?1",
                [provider],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?)
    }

    /// Upserts the identity detected for a provider, stamped with the
    /// current time. Replaces any previous row for the same provider —
    /// there is exactly one live identity per provider by design.
    pub fn save_account_identity(
        &self,
        provider: &str,
        identity: &str,
    ) -> Result<(), PersistenceError> {
        self.conn.execute(
            "INSERT INTO account_identity (provider, identity, detected_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(provider) DO UPDATE SET
                identity = excluded.identity,
                detected_at = excluded.detected_at",
            params![provider, identity, unix_timestamp_millis()],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Isolated agent accounts (F-SET-15)
    // ------------------------------------------------------------------

    /// All isolated accounts stored for one provider (`"claude"` |
    /// `"codex"`), oldest first — matches the Swift store's own
    /// `sorted { $0.createdAt < $1.createdAt }`.
    pub fn agent_accounts(
        &self,
        provider: &str,
    ) -> Result<Vec<AgentAccountRecord>, PersistenceError> {
        let mut statement = self.conn.prepare(
            "SELECT id, provider, label, config_dir_path, created_at
             FROM agent_account
             WHERE provider = ?1
             ORDER BY created_at ASC, id ASC",
        )?;
        let rows = statement.query_map([provider], |row| {
            Ok(AgentAccountRecord {
                id: row.get(0)?,
                provider: row.get(1)?,
                label: row.get(2)?,
                config_dir_path: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Inserts one isolated account. Ids are caller-supplied and unique by
    /// construction (a fresh UUID), so this is a plain insert, not an
    /// upsert — unlike `account_identity`, this is a list, not a cache.
    pub fn save_agent_account(&self, record: &AgentAccountRecord) -> Result<(), PersistenceError> {
        self.conn.execute(
            "INSERT INTO agent_account (id, provider, label, config_dir_path, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                record.id,
                record.provider,
                record.label,
                record.config_dir_path,
                record.created_at,
            ],
        )?;
        Ok(())
    }

    /// Removes one isolated account and reports whether a row was deleted.
    pub fn delete_agent_account(&self, id: &str) -> Result<bool, PersistenceError> {
        Ok(self
            .conn
            .execute("DELETE FROM agent_account WHERE id = ?1", [id])?
            > 0)
    }

    /// The currently selected account id for a provider, or `None` for
    /// "System default" (the CLI's own unmodified on-disk login). Stored
    /// under the exact UserDefaults key the Swift app used
    /// (`"agentAccounts.<provider>.activeId"`) in the same generic
    /// `setting` table `AppSettings` already uses, since this is one
    /// nullable string per provider, not a record warranting its own table.
    pub fn active_agent_account_id(
        &self,
        provider: &str,
    ) -> Result<Option<String>, PersistenceError> {
        self.setting_value(&active_agent_account_key(provider))
    }

    /// Sets (or, with `None`, clears back to "System default") the active
    /// account id for a provider.
    pub fn set_active_agent_account_id(
        &self,
        provider: &str,
        id: Option<&str>,
    ) -> Result<(), PersistenceError> {
        let transaction = write_transaction(&self.conn)?;
        let key = active_agent_account_key(provider);
        match id {
            Some(id) => set_setting(&transaction, &key, id)?,
            None => {
                transaction.execute("DELETE FROM setting WHERE key = ?1", [&key])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Settings
    // ------------------------------------------------------------------

    /// Loads the settings, applying the Swift defaults for keys that were
    /// never written and clamping font sizes into the Swift ranges. An
    /// unparseable stored value falls back to its default.
    pub fn settings(&self) -> Result<AppSettings, PersistenceError> {
        let mut defaults = AppSettings::default();

        if let Some(value) = self.setting_value(settings_keys::APPEARANCE_THEME)? {
            defaults.appearance = AppearanceMode::parse(&value).unwrap_or(AppearanceMode::System);
        }
        if let Some(value) = self.setting_value(settings_keys::UI_FONT_SIZE)? {
            defaults.ui_font_size =
                clamp_setting(&value, crate::model::settings_ranges::UI_FONT_SIZE, 13);
        }
        if let Some(value) = self.setting_value(settings_keys::TERMINAL_FONT_SIZE)? {
            defaults.terminal_font_size = clamp_setting(
                &value,
                crate::model::settings_ranges::TERMINAL_FONT_SIZE,
                13,
            );
        }
        if let Some(value) = self.setting_value(settings_keys::BASE_COLOR)? {
            defaults.base_color = BaseColor::parse(&value).unwrap_or(BaseColor::Neutral);
        }
        if let Some(value) = self.setting_value(settings_keys::CONTROL_SOCKET_ENABLED)? {
            defaults.control_socket_enabled = parse_bool_setting(&value, true);
        }
        if let Some(value) = self.setting_value(settings_keys::UPDATES_ENABLED)? {
            defaults.updates_enabled = parse_bool_setting(&value, true);
        }
        if let Some(value) = self.setting_value(settings_keys::RESUME_AGENT_SESSIONS)? {
            defaults.resume_agent_sessions = parse_bool_setting(&value, true);
        }
        if let Some(value) = self.setting_value(settings_keys::AUTO_NAMING)? {
            defaults.auto_naming = parse_bool_setting(&value, false);
        }
        if let Some(value) = self.setting_value(settings_keys::LIMIT_CHAT_HISTORY)? {
            defaults.limit_chat_history = parse_bool_setting(&value, true);
        }
        if let Some(value) = self.setting_value(settings_keys::CHAT_RETENTION)? {
            defaults.chat_retention =
                clamp_setting(&value, crate::model::settings_ranges::CHAT_RETENTION, 100);
        }
        if let Some(value) = self.setting_value(settings_keys::LIMIT_MOUNTED_WORKTREES)? {
            defaults.limit_mounted_worktrees = parse_bool_setting(&value, false);
        }
        if let Some(value) = self.setting_value(settings_keys::MOUNTED_WORKTREES)? {
            defaults.mounted_worktrees =
                clamp_setting(&value, crate::model::settings_ranges::MOUNTED_WORKTREES, 6);
        }
        if let Some(value) = self.setting_value(settings_keys::SUMMARIZER_AGENT)?
            && matches!(
                value.as_str(),
                "claude" | "codex" | "opencode" | "pi" | "omp"
            )
        {
            defaults.summarizer_agent = value;
        }
        if let Some(value) = self.setting_value(settings_keys::CLAUDE_SHOW_IN_BAR)? {
            defaults.claude_show_in_bar = parse_bool_setting(&value, true);
        }
        if let Some(value) = self.setting_value(settings_keys::CODEX_SHOW_IN_BAR)? {
            defaults.codex_show_in_bar = parse_bool_setting(&value, true);
        }
        if let Some(value) = self.setting_value(settings_keys::OPENCODE_SHOW_IN_BAR)? {
            defaults.opencode_show_in_bar = parse_bool_setting(&value, false);
        }
        if let Some(value) = self.setting_value(settings_keys::OLLAMA_SHOW_IN_BAR)? {
            defaults.ollama_show_in_bar = parse_bool_setting(&value, false);
        }
        if let Some(value) = self.setting_value(settings_keys::REFRESH_INTERVAL_MIN)? {
            defaults.refresh_interval_min = clamp_setting(
                &value,
                crate::model::settings_ranges::REFRESH_INTERVAL_MIN,
                5,
            );
        }
        if let Some(value) = self.setting_value(settings_keys::OPENCODE_WORKSPACE_ID_OVERRIDE)? {
            defaults.opencode_workspace_id_override = value;
        }
        if let Some(value) = self.setting_value(settings_keys::TRANSLUCENCY)? {
            defaults.translucency = parse_bool_setting(&value, false);
        }
        if let Some(value) = self.setting_value(settings_keys::SIDEBAR_WIDTH)? {
            defaults.sidebar_width =
                clamp_setting(&value, crate::model::settings_ranges::SIDEBAR_WIDTH, 325);
        }
        if let Some(value) = self.setting_value(settings_keys::RIGHT_PANEL_WIDTH)? {
            defaults.right_panel_width = clamp_setting(
                &value,
                crate::model::settings_ranges::RIGHT_PANEL_WIDTH,
                405,
            );
        }
        if let Some(value) = self.setting_value(settings_keys::CENTER_SPLIT_RATIO)? {
            defaults.center_split_ratio = clamp_setting(
                &value,
                crate::model::settings_ranges::CENTER_SPLIT_RATIO,
                500,
            );
        }

        Ok(defaults)
    }

    /// Persists the complete settings contract in one transaction.
    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), PersistenceError> {
        let transaction = write_transaction(&self.conn)?;
        set_setting(
            &transaction,
            settings_keys::APPEARANCE_THEME,
            settings.appearance.raw(),
        )?;
        set_setting(
            &transaction,
            settings_keys::UI_FONT_SIZE,
            &settings.ui_font_size.to_string(),
        )?;
        set_setting(
            &transaction,
            settings_keys::TERMINAL_FONT_SIZE,
            &settings.terminal_font_size.to_string(),
        )?;
        set_setting(
            &transaction,
            settings_keys::BASE_COLOR,
            settings.base_color.raw(),
        )?;
        set_setting(
            &transaction,
            settings_keys::CONTROL_SOCKET_ENABLED,
            if settings.control_socket_enabled {
                "true"
            } else {
                "false"
            },
        )?;
        set_setting(
            &transaction,
            settings_keys::UPDATES_ENABLED,
            if settings.updates_enabled {
                "true"
            } else {
                "false"
            },
        )?;
        set_setting(
            &transaction,
            settings_keys::RESUME_AGENT_SESSIONS,
            if settings.resume_agent_sessions {
                "true"
            } else {
                "false"
            },
        )?;
        set_setting(
            &transaction,
            settings_keys::AUTO_NAMING,
            if settings.auto_naming {
                "true"
            } else {
                "false"
            },
        )?;
        set_setting(
            &transaction,
            settings_keys::LIMIT_CHAT_HISTORY,
            if settings.limit_chat_history {
                "true"
            } else {
                "false"
            },
        )?;
        set_setting(
            &transaction,
            settings_keys::CHAT_RETENTION,
            &settings.chat_retention.to_string(),
        )?;
        set_setting(
            &transaction,
            settings_keys::LIMIT_MOUNTED_WORKTREES,
            if settings.limit_mounted_worktrees {
                "true"
            } else {
                "false"
            },
        )?;
        set_setting(
            &transaction,
            settings_keys::MOUNTED_WORKTREES,
            &settings.mounted_worktrees.to_string(),
        )?;
        set_setting(
            &transaction,
            settings_keys::SUMMARIZER_AGENT,
            &settings.summarizer_agent,
        )?;
        set_setting(
            &transaction,
            settings_keys::CLAUDE_SHOW_IN_BAR,
            if settings.claude_show_in_bar {
                "true"
            } else {
                "false"
            },
        )?;
        set_setting(
            &transaction,
            settings_keys::CODEX_SHOW_IN_BAR,
            if settings.codex_show_in_bar {
                "true"
            } else {
                "false"
            },
        )?;
        set_setting(
            &transaction,
            settings_keys::OPENCODE_SHOW_IN_BAR,
            if settings.opencode_show_in_bar {
                "true"
            } else {
                "false"
            },
        )?;
        set_setting(
            &transaction,
            settings_keys::OLLAMA_SHOW_IN_BAR,
            if settings.ollama_show_in_bar {
                "true"
            } else {
                "false"
            },
        )?;
        set_setting(
            &transaction,
            settings_keys::REFRESH_INTERVAL_MIN,
            &settings.refresh_interval_min.to_string(),
        )?;
        set_setting(
            &transaction,
            settings_keys::OPENCODE_WORKSPACE_ID_OVERRIDE,
            &settings.opencode_workspace_id_override,
        )?;
        set_setting(
            &transaction,
            settings_keys::TRANSLUCENCY,
            if settings.translucency {
                "true"
            } else {
                "false"
            },
        )?;
        set_setting(
            &transaction,
            settings_keys::SIDEBAR_WIDTH,
            &settings.sidebar_width.to_string(),
        )?;
        set_setting(
            &transaction,
            settings_keys::RIGHT_PANEL_WIDTH,
            &settings.right_panel_width.to_string(),
        )?;
        set_setting(
            &transaction,
            settings_keys::CENTER_SPLIT_RATIO,
            &settings.center_split_ratio.to_string(),
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn setting_value(&self, key: &str) -> Result<Option<String>, PersistenceError> {
        self.conn
            .query_row("SELECT value FROM setting WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(PersistenceError::from)
    }

    // ------------------------------------------------------------------
    // Sidebar state
    // ------------------------------------------------------------------

    /// Loads the sidebar state; a database that never saved one yields the
    /// default (nothing expanded, nothing selected).
    pub fn sidebar_state(&self) -> Result<SidebarState, PersistenceError> {
        let selected_worktree_id = self
            .conn
            .query_row(
                "SELECT selected_worktree_id FROM sidebar_state WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?
            .flatten();

        let mut statement = self
            .conn
            .prepare("SELECT project_id FROM sidebar_expanded_project ORDER BY project_id")?;
        let expanded = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SidebarState {
            expanded_project_ids: expanded,
            selected_worktree_id,
        })
    }

    /// Persists the sidebar state atomically: the singleton row plus the
    /// expanded-project set are replaced together.
    pub fn save_sidebar_state(&self, state: &SidebarState) -> Result<(), PersistenceError> {
        let transaction = write_transaction(&self.conn)?;
        transaction.execute(
            "INSERT INTO sidebar_state (id, selected_worktree_id) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET selected_worktree_id = excluded.selected_worktree_id",
            [&state.selected_worktree_id],
        )?;
        transaction.execute("DELETE FROM sidebar_expanded_project", [])?;
        for project_id in &state.expanded_project_ids {
            transaction.execute(
                "INSERT INTO sidebar_expanded_project (project_id) VALUES (?1)",
                [project_id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
}

fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

/// Serializes the tiny window in which SQLite has created a new zero-byte
/// file but has not committed the first schema yet. The lock is only created
/// for a previously missing path, so a user-provided empty file is still
/// rejected by validate_existing_file.
struct InitialOpenLock {
    path: PathBuf,
    _file: File,
}

impl Drop for InitialOpenLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_initial_open_lock(path: &Path) -> Result<Option<InitialOpenLock>, PersistenceError> {
    let lock_path = initial_open_lock_path(path);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);

    loop {
        match fs::metadata(path) {
            Ok(metadata) if metadata.len() == 0 => {
                // A creator that won the lock may already have created the
                // SQLite file, so wait for it to finish. Without the lock,
                // this is an existing empty file and must be rejected.
                if !lock_path.exists() {
                    return Ok(None);
                }
            }
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&lock_path)
                {
                    Ok(file) => {
                        return Ok(Some(InitialOpenLock {
                            path: lock_path,
                            _file: file,
                        }));
                    }
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(PersistenceError::Io(error)),
                }
            }
            Err(error) => return Err(PersistenceError::Io(error)),
        }

        if std::time::Instant::now() >= deadline {
            return Err(PersistenceError::Io(std::io::Error::new(
                ErrorKind::TimedOut,
                format!(
                    "timed out waiting for SQLite initialization lock {}",
                    lock_path.display()
                ),
            )));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn initial_open_lock_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| std::borrow::Cow::Borrowed("database"));
    path.with_file_name(format!(".{file_name}.sirio-open.lock"))
}

/// Validates an existing file before opening it read-write. A missing path is
/// a new database; an existing empty file is almost certainly a truncated
/// write and must not be mistaken for one.
fn validate_existing_file(path: &Path) -> Result<(), PersistenceError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(PersistenceError::Io(error)),
    };
    if metadata.len() == 0 {
        return Err(corrupt(path, "existing database file is empty"));
    }

    let mut file = File::open(path).map_err(PersistenceError::Io)?;
    let mut header = [0_u8; 100];
    if let Err(error) = file.read_exact(&mut header) {
        return Err(if error.kind() == ErrorKind::UnexpectedEof {
            corrupt(
                path,
                "database file is truncated before its complete header",
            )
        } else {
            PersistenceError::Io(error)
        });
    }
    if &header[..16] != b"SQLite format 3\0" {
        return Err(corrupt(path, "file does not have a SQLite header"));
    }

    let page_size = decode_page_size(&header);
    if !is_valid_page_size(page_size) {
        return Err(corrupt(
            path,
            "database header contains an invalid page size",
        ));
    }
    if metadata.len() % page_size as u64 != 0 {
        return Err(corrupt(path, "database file length is not page-aligned"));
    }

    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| classify_open_error(PersistenceError::Sqlite(error), path))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| classify_open_error(PersistenceError::Sqlite(error), path))?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(PersistenceError::NewerSchema {
            version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    verify_integrity(&conn, path)?;
    let bytes = logical_database_bytes(&conn, path)?;
    if bytes > MAX_DATABASE_BYTES {
        return Err(PersistenceError::DatabaseTooLarge {
            path: path.to_path_buf(),
            bytes,
            max_bytes: MAX_DATABASE_BYTES,
        });
    }
    Ok(())
}

fn decode_page_size(header: &[u8; 100]) -> u32 {
    let raw = u16::from_be_bytes([header[16], header[17]]);
    if raw == 1 { 65_536 } else { raw as u32 }
}

fn is_valid_page_size(page_size: u32) -> bool {
    (512..=65_536).contains(&page_size) && page_size.is_power_of_two()
}

fn logical_database_bytes(conn: &Connection, path: &Path) -> Result<u64, PersistenceError> {
    let page_size: i64 = conn.query_row("PRAGMA page_size", [], |row| row.get(0))?;
    let page_count: i64 = conn.query_row("PRAGMA page_count", [], |row| row.get(0))?;
    let page_size =
        u64::try_from(page_size).map_err(|_| corrupt(path, "database page size is negative"))?;
    let page_count =
        u64::try_from(page_count).map_err(|_| corrupt(path, "database page count is negative"))?;
    if page_size == 0 {
        return Err(corrupt(path, "database page size is zero"));
    }
    Ok(page_size.saturating_mul(page_count))
}

fn install_database_limit(conn: &Connection, path: &Path) -> Result<(), PersistenceError> {
    let page_size: i64 = conn.query_row("PRAGMA page_size", [], |row| row.get(0))?;
    let page_size =
        u64::try_from(page_size).map_err(|_| corrupt(path, "database page size is negative"))?;
    let bytes = logical_database_bytes(conn, path)?;
    if bytes > MAX_DATABASE_BYTES {
        return Err(PersistenceError::DatabaseTooLarge {
            path: path.to_path_buf(),
            bytes,
            max_bytes: MAX_DATABASE_BYTES,
        });
    }
    let max_pages = (MAX_DATABASE_BYTES / page_size).max(1);
    conn.pragma_update(None, "max_page_count", max_pages as i64)?;
    Ok(())
}

fn verify_integrity(conn: &Connection, path: &Path) -> Result<(), PersistenceError> {
    let result: String = conn.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    if result.eq_ignore_ascii_case("ok") {
        Ok(())
    } else {
        Err(corrupt(
            path,
            &format!("SQLite quick_check returned {result}"),
        ))
    }
}

fn corrupt(path: &Path, message: &str) -> PersistenceError {
    PersistenceError::Corrupt {
        path: path.to_path_buf(),
        message: message.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Row mapping helpers
// ---------------------------------------------------------------------------

fn map_project(row: &rusqlite::Row) -> rusqlite::Result<ProjectRecord> {
    Ok(ProjectRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        root_path: row.get(2)?,
        order_idx: row.get(3)?,
        color_hex: row.get(4)?,
        display_name: row.get(5)?,
        icon_kind: row.get(6)?,
        icon_value: row.get(7)?,
        avatar_image: row.get(8)?,
        default_worktree_base: row.get(9)?,
        worktree_location_override: row.get(10)?,
    })
}

fn map_worktree(row: &rusqlite::Row) -> rusqlite::Result<WorktreeRecord> {
    Ok(WorktreeRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        branch: row.get(2)?,
        path: row.get(3)?,
        is_primary: row.get(4)?,
        order_idx: row.get(5)?,
        comment: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        secondary_pane_open: row.get(9)?,
    })
}

fn map_tab(row: &rusqlite::Row, offset: usize) -> rusqlite::Result<TabRecord> {
    // The column holds the qualified `adapter:<id>`/`registry:<id>` form;
    // the single decode point is AgentRef::from_db, applied here at the DB
    // boundary so no higher layer re-parses it.
    let agent_id = row
        .get::<_, Option<String>>(offset + 4)?
        .map(|s| AgentRef::from_db(&s));
    Ok(TabRecord {
        id: row.get(offset)?,
        worktree_id: row.get(offset + 1)?,
        title: row.get(offset + 2)?,
        kind: row.get(offset + 3)?,
        agent_id,
        order_idx: row.get(offset + 5)?,
        is_active: row.get(offset + 6)?,
    })
}

struct CorruptRow {
    rowid: i64,
    record_id: String,
    payload: Vec<u8>,
    reason: String,
}

fn read_tabs<P: rusqlite::Params>(
    conn: &rusqlite::Connection,
    sql: &str,
    parameters: P,
) -> Result<Vec<TabRecord>, PersistenceError> {
    let mut statement = conn.prepare(sql)?;
    let mut rows = statement.query(parameters)?;
    let mut valid = Vec::new();
    let mut corrupt = Vec::new();
    while let Some(row) = rows.next()? {
        let rowid: i64 = row.get(0)?;
        match map_tab(row, 1) {
            Ok(tab) => valid.push(tab),
            Err(error) => {
                let record_id = row
                    .get::<_, String>(1)
                    .unwrap_or_else(|_| format!("rowid:{rowid}"));
                corrupt.push(CorruptRow {
                    rowid,
                    record_id,
                    payload: format!("tab row conversion failed: {error}").into_bytes(),
                    reason: "tab row has an invalid SQLite value".to_string(),
                });
            }
        }
    }
    drop(rows);
    drop(statement);
    quarantine_rows(conn, "tab", &corrupt)?;
    Ok(valid)
}

fn quarantine_rows(
    conn: &rusqlite::Connection,
    record_type: &str,
    rows: &[CorruptRow],
) -> Result<(), PersistenceError> {
    if rows.is_empty() {
        return Ok(());
    }
    let transaction = write_transaction(conn)?;
    for row in rows {
        transaction.execute(
            "INSERT INTO quarantine_record
                (record_type, record_id, payload, reason, quarantined_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                record_type,
                row.record_id,
                row.payload,
                row.reason,
                unix_timestamp_millis(),
            ],
        )?;
        match record_type {
            "tab" => transaction.execute("DELETE FROM tab WHERE rowid = ?1", [row.rowid])?,
            "tab_state" => {
                transaction.execute("DELETE FROM tab_state WHERE rowid = ?1", [row.rowid])?
            }
            "chat_turn" => {
                transaction.execute("DELETE FROM chat_turn WHERE rowid = ?1", [row.rowid])?
            }
            other => panic!("unknown quarantine source {other}"),
        };
    }
    transaction.commit()?;
    Ok(())
}

fn unix_timestamp_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

fn insert_project(tx: &rusqlite::Transaction, project: &ProjectRecord) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO project (id, name, root_path, order_idx, color_hex, display_name,
                              icon_kind, icon_value, avatar_image, default_worktree_base,
                              worktree_location_override)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            project.id,
            project.name,
            project.root_path,
            project.order_idx,
            project.color_hex,
            project.display_name,
            project.icon_kind,
            project.icon_value,
            project.avatar_image,
            project.default_worktree_base,
            project.worktree_location_override,
        ],
    )?;
    Ok(())
}

fn clear_other_primaries(
    tx: &rusqlite::Transaction,
    worktree: &WorktreeRecord,
) -> rusqlite::Result<()> {
    if worktree.is_primary {
        tx.execute(
            "UPDATE worktree SET is_primary = 0
             WHERE project_id = ?1 AND id != ?2 AND is_primary = 1",
            params![worktree.project_id, worktree.id],
        )?;
    }
    Ok(())
}

fn insert_worktree(tx: &rusqlite::Transaction, worktree: &WorktreeRecord) -> rusqlite::Result<()> {
    clear_other_primaries(tx, worktree)?;
    tx.execute(
        "INSERT INTO worktree
            (id, project_id, branch, path, is_primary, order_idx,
             comment, created_at, updated_at, secondary_pane_open)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            worktree.id,
            worktree.project_id,
            worktree.branch,
            worktree.path,
            worktree.is_primary,
            worktree.order_idx,
            worktree.comment,
            worktree.created_at,
            worktree.updated_at,
            worktree.secondary_pane_open,
        ],
    )?;
    Ok(())
}

/// Inserts a tab row, or updates it in place if `id` already exists.
/// Deliberately never deletes: deleting and reinserting a `tab` row —
/// even within the same transaction — cascades away `chat_turn` rows via
/// `ON DELETE CASCADE`, silently wiping persisted chat transcripts.
fn upsert_tab(tx: &rusqlite::Transaction, tab: &TabRecord) -> rusqlite::Result<()> {
    // agent_id is stored in its qualified `adapter:<id>`/`registry:<id>`
    // form; encode once here at the write boundary.
    let agent_id = tab.agent_id.as_ref().map(AgentRef::to_db_string);
    tx.execute(
        "INSERT INTO tab (id, worktree_id, title, kind, agent_id, order_idx, is_active)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
             worktree_id = excluded.worktree_id,
             title = excluded.title,
             kind = excluded.kind,
             agent_id = excluded.agent_id,
             order_idx = excluded.order_idx,
             is_active = excluded.is_active",
        params![
            tab.id,
            tab.worktree_id,
            tab.title,
            tab.kind,
            agent_id,
            tab.order_idx,
            tab.is_active,
        ],
    )?;
    Ok(())
}

/// The exact UserDefaults key format the Swift `AgentAccountStore` used:
/// `"agentAccounts.<provider>.activeId"`.
fn active_agent_account_key(provider: &str) -> String {
    format!("agentAccounts.{provider}.activeId")
}

fn set_setting(tx: &rusqlite::Transaction, key: &str, value: &str) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO setting (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

fn clamp_setting(value: &str, range: std::ops::RangeInclusive<i64>, default: i64) -> i64 {
    value
        .parse::<i64>()
        .map(|parsed| parsed.clamp(*range.start(), *range.end()))
        .unwrap_or(default)
}

fn parse_bool_setting(value: &str, default: bool) -> bool {
    match value {
        "true" => true,
        "false" => false,
        _ => default,
    }
}

/// Maps an open-time SQLite failure onto [`PersistenceError::Corrupt`] when
/// SQLite says the file is not a database.
fn classify_open_error(error: PersistenceError, path: &Path) -> PersistenceError {
    match error {
        PersistenceError::Sqlite(rusqlite::Error::SqliteFailure(ffi_error, _))
            if matches!(
                ffi_error.code,
                rusqlite::ErrorCode::NotADatabase | rusqlite::ErrorCode::DatabaseCorrupt
            ) =>
        {
            PersistenceError::Corrupt {
                path: path.to_path_buf(),
                message: if ffi_error.code == rusqlite::ErrorCode::NotADatabase {
                    "file is not a database".to_string()
                } else {
                    "SQLite reported a corrupt database".to_string()
                },
            }
        }
        other => other,
    }
}

#[cfg(test)]
mod save_tabs_tests {
    use super::*;
    use crate::model::{ChatEntry, ProjectRecord, WorktreeRecord};

    /// Regression for F-CHAT-34: `save_tabs` used to delete+reinsert every
    /// tab row of the worktree on each autosave. `tab(id)` is the parent of
    /// `chat_turn(tab_id)` with `ON DELETE CASCADE`, so even reinserting the
    /// same tab id inside the same transaction cascaded away its persisted
    /// transcript. An ordinary autosave triggered by e.g. `tab.select` must
    /// not wipe a chat session that was already saved.
    #[test]
    fn resaving_the_same_tabs_does_not_wipe_chat_transcripts() {
        let db = AppDatabase::in_memory().expect("open");
        db.save_project(&ProjectRecord::new("project", "Project", "/tmp/project"))
            .expect("project");
        db.save_worktree(&WorktreeRecord::new(
            "worktree",
            "project",
            "main",
            "/tmp/project",
        ))
        .expect("worktree");
        let chat = TabRecord::new("chat-1", "worktree", "Chat", "chat");
        db.save_tabs("worktree", std::slice::from_ref(&chat))
            .expect("initial tabs");
        db.save_chat_transcript(&ChatTranscript {
            tab_id: "chat-1".into(),
            turns: vec![ChatTurn {
                entries: vec![ChatEntry::UserMessage {
                    text: "hello".into(),
                    at: None,
                }],
            }],
        })
        .expect("save transcript");

        // Simulate an ordinary autosave (e.g. `tab.select`) that re-saves
        // the same worktree's tabs with no structural change.
        db.save_tabs("worktree", std::slice::from_ref(&chat))
            .expect("re-save tabs");

        let sessions = db.chat_sessions("worktree").expect("list sessions");
        let session = sessions
            .iter()
            .find(|s| s.tab_id == "chat-1")
            .expect("chat session must survive an ordinary tab autosave");
        assert_eq!(session.turn_count, 1);
    }
}

#[cfg(test)]
mod agent_account_tests {
    use super::*;
    use crate::model::AgentAccountRecord;

    /// F-SET-15: a provider starts with zero isolated accounts and a `None`
    /// active id ("System default"). Saving one makes it listable and
    /// selectable; selecting it persists across a fresh handle on the same
    /// file, the same durability guarantee every other setting in this file
    /// gets.
    #[test]
    fn saved_accounts_are_listed_and_selection_survives_a_reopen() {
        let dir = std::env::temp_dir().join(format!(
            "sirio-agent-account-test-{}-{}",
            std::process::id(),
            unix_timestamp_millis()
        ));
        std::fs::create_dir_all(&dir).expect("create test dir");
        let path = dir.join("accounts.sqlite");

        {
            let db = AppDatabase::open(&path).expect("open");
            assert_eq!(
                db.agent_accounts("claude").expect("list"),
                Vec::new(),
                "a fresh database has no isolated accounts for any provider"
            );
            assert_eq!(
                db.active_agent_account_id("claude").expect("active"),
                None,
                "no account selected yet means System default"
            );

            let work = AgentAccountRecord {
                id: "acct-work".into(),
                provider: "claude".into(),
                label: "Work".into(),
                config_dir_path: "/tmp/sirio-agent-accounts/claude/acct-work".into(),
                created_at: 1,
            };
            db.save_agent_account(&work).expect("save account");
            db.set_active_agent_account_id("claude", Some("acct-work"))
                .expect("select account");
        }

        // A fresh handle on the same file — proves this round-trips through
        // SQLite, not just an in-process cache.
        let reopened = AppDatabase::open(&path).expect("reopen");
        let accounts = reopened
            .agent_accounts("claude")
            .expect("list after reopen");
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].label, "Work");
        assert_eq!(
            reopened
                .active_agent_account_id("claude")
                .expect("active after reopen"),
            Some("acct-work".to_string()),
            "the selected account must survive a reopen, same as every other setting"
        );
        assert_eq!(
            reopened.agent_accounts("codex").expect("codex list"),
            Vec::new(),
            "accounts are scoped per provider"
        );

        // Clearing back to System default removes the setting row entirely
        // rather than leaving a stale/empty-string sentinel behind.
        reopened
            .set_active_agent_account_id("claude", None)
            .expect("clear selection");
        assert_eq!(
            reopened
                .active_agent_account_id("claude")
                .expect("active after clear"),
            None,
            "clearing selection must return to System default"
        );

        drop(reopened);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
