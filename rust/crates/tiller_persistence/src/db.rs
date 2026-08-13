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

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use crate::MAX_DATABASE_BYTES;
use crate::error::PersistenceError;
use crate::migrations::{CURRENT_SCHEMA_VERSION, migrate};
use crate::model::{
    AppSettings, AppearanceMode, FileIconTheme, ProjectRecord, SidebarState, TabRecord,
    TabStateRecord, WorktreeRecord, settings_keys,
};

/// Durable storage for what Tiller must remember across launches.
#[derive(Debug)]
pub struct AppDatabase {
    conn: Connection,
    /// The database file; `None` for an in-memory database.
    path: Option<PathBuf>,
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
        // A concurrent opener (another Tiller process, or a CLI invoked by an
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
        let transaction = self.conn.unchecked_transaction()?;
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
            "SELECT id, project_id, branch, path, is_primary, order_idx
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
            "SELECT id, project_id, branch, path, is_primary, order_idx
             FROM worktree
             WHERE project_id = ?1
             ORDER BY order_idx, id",
        )?;
        let rows = statement.query_map([project_id], map_worktree)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Upserts a single worktree.
    pub fn save_worktree(&self, worktree: &WorktreeRecord) -> Result<(), PersistenceError> {
        self.conn.execute(
            "INSERT INTO worktree (id, project_id, branch, path, is_primary, order_idx)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                 project_id = excluded.project_id,
                 branch = excluded.branch,
                 path = excluded.path,
                 is_primary = excluded.is_primary,
                 order_idx = excluded.order_idx",
            params![
                worktree.id,
                worktree.project_id,
                worktree.branch,
                worktree.path,
                worktree.is_primary,
                worktree.order_idx,
            ],
        )?;
        Ok(())
    }

    /// Replaces the entire worktree list in one transaction, assigning
    /// `order_idx` from the slice order. Tabs of worktrees not in the list
    /// are cascade-deleted.
    pub fn save_worktrees(&self, worktrees: &[WorktreeRecord]) -> Result<(), PersistenceError> {
        let transaction = self.conn.unchecked_transaction()?;
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

    // ------------------------------------------------------------------
    // Tabs
    // ------------------------------------------------------------------

    /// All tabs, ordered by worktree then position.
    pub fn tabs(&self) -> Result<Vec<TabRecord>, PersistenceError> {
        let mut statement = self.conn.prepare(
            "SELECT id, worktree_id, title, kind, order_idx, is_active
             FROM tab
             ORDER BY worktree_id, order_idx, id",
        )?;
        let rows = statement.query_map([], map_tab)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The tabs of one worktree, in order.
    pub fn tabs_of_worktree(&self, worktree_id: &str) -> Result<Vec<TabRecord>, PersistenceError> {
        let mut statement = self.conn.prepare(
            "SELECT id, worktree_id, title, kind, order_idx, is_active
             FROM tab
             WHERE worktree_id = ?1
             ORDER BY order_idx, id",
        )?;
        let rows = statement.query_map([worktree_id], map_tab)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
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
        let transaction = self.conn.unchecked_transaction()?;
        if tab.is_active {
            transaction.execute(
                "UPDATE tab SET is_active = 0
                 WHERE worktree_id = ?1 AND is_active = 1 AND id != ?2",
                params![tab.worktree_id, tab.id],
            )?;
        }
        transaction.execute(
            "INSERT INTO tab (id, worktree_id, title, kind, order_idx, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                 worktree_id = excluded.worktree_id,
                 title = excluded.title,
                 kind = excluded.kind,
                 order_idx = excluded.order_idx,
                 is_active = excluded.is_active",
            params![
                tab.id,
                tab.worktree_id,
                tab.title,
                tab.kind,
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
    pub fn save_tabs(&self, worktree_id: &str, tabs: &[TabRecord]) -> Result<(), PersistenceError> {
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute("DELETE FROM tab WHERE worktree_id = ?1", [worktree_id])?;
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
            insert_tab(&transaction, &tab)?;
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
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            "DELETE FROM tab_state
             WHERE tab_id IN (SELECT id FROM tab WHERE worktree_id = ?1)",
            [worktree_id],
        )?;
        for state in states {
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
            "SELECT tab_state.tab_id, tab_state.state
             FROM tab_state
             JOIN tab ON tab.id = tab_state.tab_id
             WHERE tab.worktree_id = ?1
             ORDER BY tab.order_idx, tab.id",
        )?;
        let rows = statement.query_map([worktree_id], |row| {
            Ok(TabStateRecord {
                tab_id: row.get(0)?,
                state: row.get(1)?,
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
        if let Some(value) = self.setting_value(settings_keys::FILE_ICON_THEME)? {
            defaults.file_icon_theme =
                FileIconTheme::parse(&value).unwrap_or(FileIconTheme::SfSymbols);
        }
        if let Some(value) = self.setting_value(settings_keys::CONTROL_SOCKET_ENABLED)? {
            defaults.control_socket_enabled = match value.as_str() {
                "true" => true,
                "false" => false,
                _ => true, // unparseable → default (enabled), like Swift
            };
        }

        Ok(defaults)
    }

    /// Persists all five settings in one transaction, under the exact Swift
    /// key names.
    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), PersistenceError> {
        let transaction = self.conn.unchecked_transaction()?;
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
            settings_keys::FILE_ICON_THEME,
            settings.file_icon_theme.raw(),
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
        let transaction = self.conn.unchecked_transaction()?;
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
    path.with_file_name(format!(".{file_name}.tiller-open.lock"))
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
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
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
    })
}

fn map_tab(row: &rusqlite::Row) -> rusqlite::Result<TabRecord> {
    Ok(TabRecord {
        id: row.get(0)?,
        worktree_id: row.get(1)?,
        title: row.get(2)?,
        kind: row.get(3)?,
        order_idx: row.get(4)?,
        is_active: row.get(5)?,
    })
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

fn insert_worktree(tx: &rusqlite::Transaction, worktree: &WorktreeRecord) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO worktree (id, project_id, branch, path, is_primary, order_idx)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            worktree.id,
            worktree.project_id,
            worktree.branch,
            worktree.path,
            worktree.is_primary,
            worktree.order_idx,
        ],
    )?;
    Ok(())
}

fn insert_tab(tx: &rusqlite::Transaction, tab: &TabRecord) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO tab (id, worktree_id, title, kind, order_idx, is_active)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            tab.id,
            tab.worktree_id,
            tab.title,
            tab.kind,
            tab.order_idx,
            tab.is_active,
        ],
    )?;
    Ok(())
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
