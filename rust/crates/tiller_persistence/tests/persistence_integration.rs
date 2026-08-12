//! Integration tests against real temporary database files. The failure
//! modes under test — migration on open, corruption, atomicity — only exist
//! on disk, so no test here uses an in-memory database.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tiller_persistence::{
    AppDatabase, AppSettings, AppearanceMode, FileIconTheme, PersistenceError, ProjectRecord,
    SidebarState, TabRecord, WorktreeRecord, migrate_up_to,
};

/// A throwaway directory, removed on drop. Canonicalized so paths match what
/// SQLite reports.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tiller-persistence-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(std::fs::canonicalize(&path).expect("canonicalize temp dir"))
    }

    fn db_path(&self, name: &str) -> PathBuf {
        self.0.join(format!("{name}.sqlite"))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sample_project(id: &str, name: &str) -> ProjectRecord {
    ProjectRecord {
        id: id.to_string(),
        name: name.to_string(),
        root_path: format!("/Users/me/{name}"),
        order_idx: 0,
        color_hex: Some("#1A6DFF".to_string()),
        display_name: None,
        icon_kind: "icon".to_string(),
        icon_value: Some("folder.fill".to_string()),
        avatar_image: None,
        default_worktree_base: None,
        worktree_location_override: None,
    }
}

fn sample_worktree(id: &str, project_id: &str, branch: &str) -> WorktreeRecord {
    WorktreeRecord {
        id: id.to_string(),
        project_id: project_id.to_string(),
        branch: branch.to_string(),
        path: format!("/Users/me/{branch}"),
        order_idx: 0,
        is_primary: branch == "main",
    }
}

fn sample_tab(id: &str, worktree_id: &str, title: &str, kind: &str) -> TabRecord {
    TabRecord {
        id: id.to_string(),
        worktree_id: worktree_id.to_string(),
        title: title.to_string(),
        kind: kind.to_string(),
        order_idx: 0,
        is_active: false,
    }
}

#[test]
fn round_trips_projects_worktrees_tabs_settings_and_sidebar() {
    let dir = TempDir::new();
    let path = dir.db_path("roundtrip");

    let settings = AppSettings {
        appearance: AppearanceMode::Dark,
        ui_font_size: 16,
        terminal_font_size: 14,
        file_icon_theme: FileIconTheme::Material,
        control_socket_enabled: false,
    };
    let state = SidebarState {
        expanded_project_ids: vec!["proj-1".to_string()],
        selected_worktree_id: Some("wt-2".to_string()),
    };

    {
        let db = AppDatabase::open(&path).expect("open fresh database");

        let projects = vec![
            sample_project("proj-1", "tiller"),
            sample_project("proj-2", "notes"),
        ];
        db.save_projects(&projects).expect("save projects");
        db.save_worktrees(&[
            sample_worktree("wt-1", "proj-1", "main"),
            sample_worktree("wt-2", "proj-1", "feature/login"),
            sample_worktree("wt-3", "proj-2", "main"),
        ])
        .expect("save worktrees");
        db.save_tabs(
            "wt-2",
            &[
                sample_tab("tab-1", "wt-2", "Chat", "chat"),
                sample_tab("tab-2", "wt-2", "Terminal", "terminal"),
            ],
        )
        .expect("save tabs");
        db.save_settings(&settings).expect("save settings");
        db.save_sidebar_state(&state).expect("save sidebar state");
    }
    // Drop closes the connection; reopen simulates a relaunch.

    let db = AppDatabase::open(&path).expect("reopen migrated database");
    assert_eq!(db.schema_version().expect("version"), 3);

    let projects = db.projects().expect("load projects");
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0].name, "tiller");
    assert_eq!(projects[1].name, "notes");
    assert_eq!(projects[0].order_idx, 0);
    assert_eq!(projects[1].order_idx, 1);
    assert_eq!(projects[0].color_hex.as_deref(), Some("#1A6DFF"));
    assert_eq!(projects[0].icon_kind, "icon");

    let worktrees = db.worktrees_of_project("proj-1").expect("load worktrees");
    assert_eq!(worktrees.len(), 2);
    assert_eq!(worktrees[0].branch, "main");
    assert!(worktrees[0].is_primary);
    assert_eq!(worktrees[1].branch, "feature/login");
    assert_eq!(worktrees[1].order_idx, 1);

    let tabs = db.tabs_of_worktree("wt-2").expect("load tabs");
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs[0].title, "Chat");
    assert_eq!(tabs[0].kind, "chat");
    assert_eq!(tabs[1].title, "Terminal");
    assert_eq!(tabs[1].order_idx, 1);

    assert_eq!(db.settings().expect("load settings"), settings);
    assert_eq!(db.sidebar_state().expect("load sidebar state"), state);
}

#[test]
fn an_old_version_database_is_migrated_forward_with_rows_intact() {
    let dir = TempDir::new();
    let path = dir.db_path("old-version");

    // Build a v1 database by hand: open a connection, apply only the v1
    // migration, and seed rows the way a v1 app would have.
    {
        let mut conn = rusqlite::Connection::open(&path).expect("open raw connection");
        conn.execute("PRAGMA foreign_keys = ON", [])
            .expect("pragmas");
        migrate_up_to(&mut conn, 1).expect("migrate to v1");
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("read version"),
            1
        );

        conn.execute(
            "INSERT INTO project (id, name, root_path, icon_kind, order_idx)
             VALUES ('proj-1', 'tiller', '/Users/me/tiller', 'icon', 0)",
            [],
        )
        .expect("seed project");
        conn.execute(
            "INSERT INTO worktree (id, project_id, branch, path, order_idx)
             VALUES ('wt-1', 'proj-1', 'main', '/Users/me/tiller', 0)",
            [],
        )
        .expect("seed worktree");
        conn.execute(
            "INSERT INTO tab (id, worktree_id, title, kind, order_idx, is_active)
             VALUES ('tab-1', 'wt-1', 'Terminal', 'terminal', 0, 1)",
            [],
        )
        .expect("seed tab");
        conn.execute(
            "INSERT INTO setting (key, value) VALUES ('appearance.theme', 'dark')",
            [],
        )
        .expect("seed setting");
    }

    // Opening with the real database migrates v1 -> v3.
    let db = AppDatabase::open(&path).expect("open migrates forward");
    assert_eq!(db.schema_version().expect("version"), 3);

    let projects = db.projects().expect("load projects");
    assert_eq!(projects.len(), 1, "project survived the migration");
    assert_eq!(projects[0].id, "proj-1");
    assert_eq!(projects[0].name, "tiller");

    let worktrees = db.worktrees().expect("load worktrees");
    assert_eq!(worktrees.len(), 1, "worktree survived the migration");
    assert_eq!(worktrees[0].branch, "main");

    let tabs = db.tabs_of_worktree("wt-1").expect("load tabs");
    assert_eq!(tabs.len(), 1, "tab survived the migration");
    assert!(tabs[0].is_active);

    let settings = db.settings().expect("load settings");
    assert_eq!(
        settings.appearance,
        AppearanceMode::Dark,
        "setting survived"
    );

    // The v2 tables exist and are empty until used.
    let state = db.sidebar_state().expect("load sidebar state");
    assert!(state.expanded_project_ids.is_empty());
    assert_eq!(state.selected_worktree_id, None);
}

#[test]
fn a_corrupt_file_returns_an_error_and_is_left_untouched() {
    let dir = TempDir::new();
    let path = dir.db_path("corrupt");

    let garbage = b"this is not a sqlite database, just some bytes that are long enough to not be mistaken for an empty file........";
    std::fs::write(&path, garbage).expect("write garbage");

    let error = AppDatabase::open(&path).expect_err("corrupt file must error, not panic");
    assert!(
        matches!(error, PersistenceError::Corrupt { .. }),
        "expected Corrupt, got {error:?}"
    );

    let after = std::fs::read(&path).expect("file still readable");
    assert_eq!(
        after, garbage,
        "a corrupt open must never modify or discard the user's file"
    );
}

#[test]
fn defaults_are_returned_for_settings_never_written() {
    let dir = TempDir::new();
    let db = AppDatabase::open(&dir.db_path("defaults")).expect("open");

    // No rows were ever written; the Swift defaults must come back.
    assert_eq!(
        db.settings().expect("load settings"),
        AppSettings::default()
    );
    assert_eq!(
        db.sidebar_state().expect("load sidebar state"),
        SidebarState::default()
    );
    assert!(db.projects().expect("load projects").is_empty());
}

#[test]
fn out_of_range_and_unparseable_settings_clamp_and_fall_back() {
    let dir = TempDir::new();
    let path = dir.db_path("clamp");
    {
        let db = AppDatabase::open(&path).expect("open");
        db.save_settings(&AppSettings {
            appearance: AppearanceMode::Light,
            ui_font_size: 99,      // above the 10...20 Swift range
            terminal_font_size: 1, // below the 9...24 Swift range
            file_icon_theme: FileIconTheme::SfSymbols,
            control_socket_enabled: true,
        })
        .expect("save");
    }

    // Hand-corrupt one key to prove unparseable values fall back.
    {
        let conn = rusqlite::Connection::open(&path).expect("open raw");
        conn.execute(
            "UPDATE setting SET value = 'banana' WHERE key = 'appearance.fileIconTheme'",
            [],
        )
        .expect("corrupt a key");
    }

    let db = AppDatabase::open(&path).expect("reopen");
    let settings = db.settings().expect("load");
    assert_eq!(
        settings.ui_font_size, 20,
        "clamped to the Swift upper bound"
    );
    assert_eq!(
        settings.terminal_font_size, 9,
        "clamped to the Swift lower bound"
    );
    assert_eq!(
        settings.file_icon_theme,
        FileIconTheme::SfSymbols,
        "unparseable value falls back to the default"
    );
    assert_eq!(
        settings.appearance,
        AppearanceMode::Light,
        "valid value kept"
    );
}

#[test]
fn a_failed_save_rolls_back_atomically() {
    let dir = TempDir::new();
    let db = AppDatabase::open(&dir.db_path("atomic")).expect("open");
    db.save_projects(&[sample_project("proj-1", "tiller")])
        .expect("seed project");
    db.save_worktrees(&[sample_worktree("wt-1", "proj-1", "main")])
        .expect("seed worktree");

    // save_tabs replaces the worktree's tabs; the second tab violates the
    // foreign key (no such worktree), so the whole transaction must roll
    // back — including the DELETE of the previous tabs.
    let result = db.save_tabs(
        "wt-1",
        &[
            sample_tab("tab-ok", "wt-1", "Terminal", "terminal"),
            sample_tab("tab-fk", "no-such-worktree", "Chat", "chat"),
        ],
    );
    assert!(result.is_err(), "foreign key violation must fail the save");

    let tabs = db.tabs_of_worktree("wt-1").expect("load tabs");
    assert!(
        tabs.is_empty(),
        "a failed save must not leave a half-written layout"
    );
}

#[test]
fn save_tabs_normalizes_the_active_flag() {
    let dir = TempDir::new();
    let db = AppDatabase::open(&dir.db_path("active")).expect("open");
    db.save_projects(&[sample_project("proj-1", "tiller")])
        .expect("seed project");
    db.save_worktrees(&[sample_worktree("wt-1", "proj-1", "main")])
        .expect("seed worktree");

    let mut a = sample_tab("tab-a", "wt-1", "A", "terminal");
    a.is_active = true;
    let mut b = sample_tab("tab-b", "wt-1", "B", "chat");
    b.is_active = true; // second active — must lose
    db.save_tabs("wt-1", &[a, b]).expect("save tabs");

    let tabs = db.tabs_of_worktree("wt-1").expect("load tabs");
    assert!(tabs[0].is_active, "first active tab wins");
    assert!(!tabs[1].is_active, "second active tab is normalized away");
}

#[test]
fn save_tab_keeps_a_single_active_tab_per_worktree() {
    let dir = TempDir::new();
    let db = AppDatabase::open(&dir.db_path("single-active")).expect("open");
    db.save_projects(&[sample_project("proj-1", "tiller")])
        .expect("seed project");
    db.save_worktrees(&[sample_worktree("wt-1", "proj-1", "main")])
        .expect("seed worktree");

    // Activating one tab after another through the single-tab path must not
    // leave two active tabs behind: the invariant is "at most one active
    // per worktree", and this is the path that used to violate it.
    let mut first = sample_tab("tab-first", "wt-1", "Terminal", "terminal");
    first.is_active = true;
    db.save_tab(&first).expect("activate first tab");

    let mut second = sample_tab("tab-second", "wt-1", "Chat", "chat");
    second.is_active = true;
    db.save_tab(&second).expect("activate second tab");

    let tabs = db.tabs_of_worktree("wt-1").expect("load tabs");
    let active: Vec<_> = tabs.iter().filter(|tab| tab.is_active).collect();
    assert_eq!(active.len(), 1, "exactly one active tab remains");
    assert_eq!(active[0].id, "tab-second", "the newly activated tab wins");
}

#[test]
fn a_pre_invariant_database_with_two_active_tabs_is_reconciled_on_open() {
    let dir = TempDir::new();
    let path = dir.db_path("legacy-two-active");

    // Build a v2 database the way the pre-fix app would have: schema v2 and
    // two active tabs for the same worktree (the old save_tab allowed it).
    {
        let mut conn = rusqlite::Connection::open(&path).expect("open raw connection");
        conn.execute("PRAGMA foreign_keys = ON", [])
            .expect("pragmas");
        migrate_up_to(&mut conn, 2).expect("migrate to v2");
        conn.execute(
            "INSERT INTO project (id, name, root_path, icon_kind, order_idx)
             VALUES ('proj-1', 'tiller', '/Users/me/tiller', 'icon', 0)",
            [],
        )
        .expect("seed project");
        conn.execute(
            "INSERT INTO worktree (id, project_id, branch, path, order_idx)
             VALUES ('wt-1', 'proj-1', 'main', '/Users/me/tiller', 0)",
            [],
        )
        .expect("seed worktree");
        conn.execute(
            "INSERT INTO tab (id, worktree_id, title, kind, order_idx, is_active)
             VALUES ('tab-old', 'wt-1', 'Terminal', 'terminal', 0, 1)",
            [],
        )
        .expect("seed first active tab");
        conn.execute(
            "INSERT INTO tab (id, worktree_id, title, kind, order_idx, is_active)
             VALUES ('tab-new', 'wt-1', 'Chat', 'chat', 1, 1)",
            [],
        )
        .expect("seed second active tab");
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("read version"),
            2,
            "fixture is a pre-fix v2 database"
        );
    }

    // Opening must not refuse the database: it migrates forward and
    // reconciles the violation to a single active tab.
    let db = AppDatabase::open(&path).expect("open reconciles legacy database");
    assert_eq!(db.schema_version().expect("version"), 3);

    let tabs = db.tabs_of_worktree("wt-1").expect("load tabs");
    assert_eq!(tabs.len(), 2, "rows survive the migration");
    let active: Vec<_> = tabs.iter().filter(|tab| tab.is_active).collect();
    assert_eq!(active.len(), 1, "exactly one active tab after reconciliation");
    assert_eq!(
        active[0].id, "tab-old",
        "first in tab order wins, the same rule save_tabs applies"
    );
}

#[test]
fn the_unique_index_rejects_a_second_active_tab_at_the_sql_level() {
    let dir = TempDir::new();
    let path = dir.db_path("index-enforced");
    {
        let db = AppDatabase::open(&path).expect("open");
        db.save_projects(&[sample_project("proj-1", "tiller")])
            .expect("seed project");
        db.save_worktrees(&[sample_worktree("wt-1", "proj-1", "main")])
            .expect("seed worktree");
        let mut tab = sample_tab("tab-a", "wt-1", "A", "terminal");
        tab.is_active = true;
        db.save_tab(&tab).expect("activate tab");
    }

    // Bypass the API entirely: a raw INSERT of a second active tab must be
    // rejected by the database itself, not by a check a future caller can
    // forget.
    let conn = rusqlite::Connection::open(&path).expect("open raw");
    let result = conn.execute(
        "INSERT INTO tab (id, worktree_id, title, kind, order_idx, is_active)
         VALUES ('tab-b', 'wt-1', 'B', 'chat', 1, 1)",
        [],
    );
    assert!(
        result.is_err(),
        "a raw second active tab must fail the unique index"
    );
}


#[test]
fn removing_a_project_cascades_to_its_children() {
    let dir = TempDir::new();
    let db = AppDatabase::open(&dir.db_path("cascade")).expect("open");

    db.save_projects(&[
        sample_project("proj-1", "tiller"),
        sample_project("proj-2", "notes"),
    ])
    .expect("save projects");
    db.save_worktrees(&[sample_worktree("wt-1", "proj-1", "main")])
        .expect("save worktrees");
    db.save_tabs(
        "wt-1",
        &[sample_tab("tab-1", "wt-1", "Terminal", "terminal")],
    )
    .expect("save tabs");

    db.remove_project("proj-1").expect("remove project");

    assert_eq!(db.projects().expect("projects").len(), 1);
    assert!(
        db.worktrees_of_project("proj-1")
            .expect("worktrees")
            .is_empty()
    );
    assert!(db.tabs_of_worktree("wt-1").expect("tabs").is_empty());
    assert_eq!(
        db.sidebar_state()
            .expect("sidebar")
            .expanded_project_ids
            .len(),
        0,
        "expansion rows cascade with their project"
    );
}

#[test]
fn a_newer_schema_database_is_refused_not_destroyed() {
    let dir = TempDir::new();
    let path = dir.db_path("newer");
    {
        let db = AppDatabase::open(&path).expect("open");
        db.save_project(&sample_project("proj-1", "tiller"))
            .expect("save");
    }

    // Simulate a newer app having bumped the version beyond what we know.
    {
        let conn = rusqlite::Connection::open(&path).expect("open raw");
        conn.pragma_update(None, "user_version", 999)
            .expect("bump version");
    }

    let error = AppDatabase::open(&path).expect_err("newer schema must be refused");
    assert!(matches!(error, PersistenceError::NewerSchema { .. }));
    assert_eq!(db_schema_version(&path), 999, "the file is left alone");
}

fn db_schema_version(path: &Path) -> i64 {
    let conn = rusqlite::Connection::open(path).expect("open raw");
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read version")
}

/// Runs `AppDatabase::open` in a real child process: writes its ready file,
/// waits for the parent's go file, opens the database, verifies the schema
/// and the partial unique index, prints `OK`, and exits 0. Any failure exits
/// non-zero with the reason on stderr.
///
/// Triggered by the `TILLER_PERSISTENCE_HELPER` environment variable, so the
/// parent can re-execute this test binary as the child (`current_exe`). When
/// the variable is absent this is a no-op test.
#[test]
fn helper_process() {
    let Ok(database_path) = std::env::var("TILLER_PERSISTENCE_HELPER") else {
        return;
    };
    let go_file = std::env::var("TILLER_PERSISTENCE_GO").expect("go file env");
    let ready_file = std::env::var("TILLER_PERSISTENCE_READY").expect("ready file env");

    std::fs::write(&ready_file, "ready").unwrap_or_else(|error| {
        helper_fail(3, &format!("cannot write ready file: {error}"))
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !Path::new(&go_file).exists() {
        if std::time::Instant::now() > deadline {
            helper_fail(2, "go file never appeared");
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    let db = match AppDatabase::open(Path::new(&database_path)) {
        Ok(db) => db,
        Err(error) => helper_fail(1, &format!("open failed: {error:?}")),
    };
    let version = db.schema_version().unwrap_or_else(|error| {
        helper_fail(4, &format!("schema version unreadable: {error:?}"))
    });
    if version != tiller_persistence::CURRENT_SCHEMA_VERSION {
        helper_fail(
            5,
            &format!(
                "schema version {version}, expected {}",
                tiller_persistence::CURRENT_SCHEMA_VERSION
            ),
        );
    }
    // The schema must be complete, not a half-migrated shell.
    let conn = rusqlite::Connection::open(&database_path).expect("reopen raw for verification");
    let tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table'",
            [],
            |row| row.get(0),
        )
        .expect("count tables");
    let index: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master
             WHERE type = 'index' AND name = 'tab_one_active_per_worktree'",
            [],
            |row| row.get(0),
        )
        .expect("count index");
    if tables < 4 || index != 1 {
        helper_fail(
            6,
            &format!("incomplete schema: {tables} tables, index count {index}"),
        );
    }
    println!("OK");
    std::process::exit(0);
}

fn helper_fail(code: i32, message: &str) -> ! {
    eprintln!("helper failed: {message}");
    std::process::exit(code);
}

/// Polls `ready` until it returns true or `deadline` passes.
fn wait_until(
    deadline: std::time::Instant,
    what: &str,
    mut ready: impl FnMut() -> bool,
) -> bool {
    while !ready() {
        if std::time::Instant::now() > deadline {
            eprintln!("timed out waiting for {what}");
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    true
}

/// Spawns one child helper process; returns the child and its output
/// handles. The child writes `ready` first and waits for `go` before
/// opening the database.
fn spawn_helper(
    dir: &Path,
    database_path: &Path,
    ready: &Path,
    go: &Path,
) -> std::process::Child {
    std::process::Command::new(std::env::current_exe().expect("test binary path"))
        .args(["--exact", "helper_process", "--nocapture"])
        .env("TILLER_PERSISTENCE_HELPER", database_path)
        .env("TILLER_PERSISTENCE_GO", go)
        .env("TILLER_PERSISTENCE_READY", ready)
        .current_dir(dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn helper process")
}

/// Waits for `children` to exit (bounded), asserting every one exited 0 with
/// `OK` on stdout.
fn assert_all_helpers_succeeded(
    children: Vec<std::process::Child>,
    deadline: std::time::Instant,
) {
    let mut children = children;
    while !children.is_empty() {
        children.retain_mut(|child| match child.try_wait() {
            Ok(Some(status)) => {
                assert!(
                    status.success(),
                    "helper exited with {status:?}: {}",
                    read_stderr(child)
                );
                false
            }
            Ok(None) => true,
            Err(error) => panic!("waiting for helper: {error}"),
        });
        if !children.is_empty() {
            assert!(
                std::time::Instant::now() < deadline,
                "helpers did not exit within the deadline"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

fn read_stderr(child: &mut std::process::Child) -> String {
    use std::io::Read as _;
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    stderr
}

/// The reviewer's reproduction, made deterministic: two real OS processes
/// open the same brand-new database path concurrently. Both must succeed and
/// the schema must end up complete.
///
/// The opens are forced to overlap rather than merely launched together: the
/// parent holds the SQLite write lock on the schema-less database while both
/// children run their opens, so both are provably inside the first-open
/// critical path at the same time — each one's schema writes block on the
/// parent's lock. On the unfixed code (no busy timeout, deferred migration
/// transactions) the children fail immediately with `database is locked`;
/// with the fix the second opener waits, then finds the schema current.
#[test]
fn concurrent_first_opens_from_two_processes_both_succeed() {
    let dir = TempDir::new();
    let database_path = dir.db_path("concurrent-first");
    let go = dir.0.join("go");

    // The parent is a synchronizer, not an opener: it creates the empty file
    // (no schema, user_version 0 — exactly what two racing first-openers
    // see) and holds the write lock so the children's opens provably
    // overlap.
    let mut conn = rusqlite::Connection::open(&database_path).expect("parent raw open");
    let _: String = conn
        .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
        .expect("parent sets WAL");
    let held_lock = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .expect("parent takes the write lock");

    let ready_a = dir.0.join("ready-a");
    let ready_b = dir.0.join("ready-b");
    let child_a = spawn_helper(&dir.0, &database_path, &ready_a, &go);
    let child_b = spawn_helper(&dir.0, &database_path, &ready_b, &go);

    // Both children are alive and waiting before the go file appears.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    assert!(
        wait_until(deadline, "both ready files", || {
            ready_a.exists() && ready_b.exists()
        }),
        "children never signalled ready"
    );

    std::fs::write(&go, "go").expect("write go file");
    // Hold the write lock well past the children's arrival, so both opens
    // are blocked inside their migration when we release it.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    held_lock.rollback().expect("release the lock");
    drop(conn);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    assert_all_helpers_succeeded(vec![child_a, child_b], deadline);

    // The schema is complete and correct after both processes raced.
    assert_eq!(
        db_schema_version(&database_path),
        tiller_persistence::CURRENT_SCHEMA_VERSION
    );
    let db = AppDatabase::open(&database_path).expect("reopen for verification");
    assert_eq!(
        db.schema_version().expect("version"),
        tiller_persistence::CURRENT_SCHEMA_VERSION
    );
}

/// The read path: two processes opening an already-migrated database at the
/// same time must both succeed — the common case must not pay for the
/// migration lock.
#[test]
fn concurrent_opens_of_an_already_migrated_database_both_succeed() {
    let dir = TempDir::new();
    let database_path = dir.db_path("concurrent-migrated");
    {
        let db = AppDatabase::open(&database_path).expect("seed database");
        db.save_project(&sample_project("proj-1", "tiller"))
            .expect("seed project");
    }
    assert_eq!(
        db_schema_version(&database_path),
        tiller_persistence::CURRENT_SCHEMA_VERSION
    );

    let go = dir.0.join("go");
    let ready_a = dir.0.join("ready-a");
    let ready_b = dir.0.join("ready-b");
    let child_a = spawn_helper(&dir.0, &database_path, &ready_a, &go);
    let child_b = spawn_helper(&dir.0, &database_path, &ready_b, &go);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    assert!(
        wait_until(deadline, "both ready files", || {
            ready_a.exists() && ready_b.exists()
        }),
        "children never signalled ready"
    );
    std::fs::write(&go, "go").expect("write go file");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    assert_all_helpers_succeeded(vec![child_a, child_b], deadline);
}
