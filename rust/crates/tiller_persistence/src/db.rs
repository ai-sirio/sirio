//! The [`AppDatabase`]: owns the SQLite connection, runs the migrations on
//! open, and exposes the persisted records.
//!
//! Writes are atomic per logical change: every save runs in a transaction,
//! so a crash mid-save cannot leave a half-written layout. The connection is
//! not `Sync`; callers that need multi-threaded access should wrap the
//! database in a `Mutex` (or keep it on one thread, as the GPUI app does).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::PersistenceError;
use crate::migrations::migrate;
use crate::model::{
    AppSettings, AppearanceMode, FileIconTheme, ProjectRecord, SidebarState, TabRecord,
    WorktreeRecord, settings_keys,
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
        Self::initialize(&mut conn)?;
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

    fn open_connection(path: &Path) -> Result<Connection, PersistenceError> {
        let mut conn = Connection::open(path)
            .map_err(|error| classify_open_error(PersistenceError::Sqlite(error), path))?;
        Self::initialize(&mut conn).map_err(|error| classify_open_error(error, path))?;
        Ok(conn)
    }

    /// Per-connection pragmas plus the forward migration.
    fn initialize(conn: &mut Connection) -> Result<(), PersistenceError> {
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
        migrate(conn)?;
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
            if ffi_error.code == rusqlite::ErrorCode::NotADatabase =>
        {
            PersistenceError::Corrupt {
                path: path.to_path_buf(),
                message: "file is not a database".to_string(),
            }
        }
        other => other,
    }
}
