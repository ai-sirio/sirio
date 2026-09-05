//! Integration tests against real temporary database files. The failure
//! modes under test — migration on open, corruption, atomicity — only exist
//! on disk, so no test here uses an in-memory database.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sirio_persistence::{
    AgentRef, AppDatabase, AppSettings, AppearanceMode, BaseColor, CURRENT_SCHEMA_VERSION, ChatEntry,
    ChatPermissionOption, ChatPermissionOutcome, ChatToolLocation, ChatTranscript, ChatTurn,
    MAX_DATABASE_BYTES, PersistenceError, ProjectRecord, SidebarState, TabRecord,
    TabStateRecord, WorktreeRecord, migrate_up_to,
};

/// A throwaway directory, removed on drop. Canonicalized so paths match what
/// SQLite reports.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "sirio-persistence-test-{}-{unique}",
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
        comment: None,
        created_at: None,
        updated_at: None,
        secondary_pane_open: false,
    }
}

fn sample_tab(id: &str, worktree_id: &str, title: &str, kind: &str) -> TabRecord {
    TabRecord {
        id: id.to_string(),
        worktree_id: worktree_id.to_string(),
        title: title.to_string(),
        kind: kind.to_string(),
        agent_id: None,
        order_idx: 0,
        is_active: false,
    }
}

#[test]
fn worktree_metadata_and_exact_path_survive_a_relaunch() {
    let dir = TempDir::new();
    let path = dir.db_path("worktree-metadata");
    let mut expected = sample_worktree("wt-1", "proj-1", "main");
    expected.comment = Some("release checkout".into());
    expected.created_at = Some(1_700_000_000_123);
    expected.updated_at = Some(1_700_000_100_456);
    {
        let db = AppDatabase::open(&path).expect("open");
        db.save_project(&sample_project("proj-1", "sirio"))
            .expect("project");
        db.save_worktree(&expected).expect("worktree");
    }
    let db = AppDatabase::open(&path).expect("reopen");
    assert_eq!(
        db.worktree_by_path(&expected.path).expect("lookup"),
        Some(expected.clone())
    );
    assert_eq!(db.worktrees().expect("worktrees"), vec![expected]);
}

#[test]
fn a_duplicate_primary_is_reconciled_and_future_writes_are_rejected() {
    let dir = TempDir::new();
    let path = dir.db_path("primary-invariant");
    {
        let mut conn = rusqlite::Connection::open(&path).expect("open raw database");
        conn.execute("PRAGMA foreign_keys = ON", [])
            .expect("pragmas");
        migrate_up_to(&mut conn, 6).expect("migrate to pre-index schema");
        conn.execute(
            "INSERT INTO project (id, name, root_path, icon_kind, order_idx)
             VALUES ('proj-1', 'sirio', '/tmp/sirio', 'icon', 0)",
            [],
        )
        .expect("project");
        for (id, path, order) in [
            ("wt-first", "/tmp/sirio", 0),
            ("wt-second", "/tmp/sirio-second", 1),
        ] {
            conn.execute(
                "INSERT INTO worktree (id, project_id, branch, path, is_primary, order_idx)
                 VALUES (?1, 'proj-1', ?2, ?3, 1, ?4)",
                rusqlite::params![id, id, path, order],
            )
            .expect("duplicate primary fixture");
        }
    }

    let db = AppDatabase::open(&path).expect("open reconciles duplicate primary");
    let worktrees = db.worktrees_of_project("proj-1").expect("worktrees");
    assert!(worktrees[0].is_primary);
    assert!(!worktrees[1].is_primary);

    let conn = rusqlite::Connection::open(&path).expect("reopen raw database");
    let result = conn.execute(
        "INSERT INTO worktree (id, project_id, branch, path, is_primary, order_idx)
         VALUES ('wt-third', 'proj-1', 'third', '/tmp/sirio-third', 1, 2)",
        [],
    );
    assert!(
        result.is_err(),
        "the primary index must reject a second primary"
    );
}

#[test]
fn exact_path_lookup_does_not_normalize_nearby_paths() {
    let dir = TempDir::new();
    let path = dir.db_path("exact-path");
    let db = AppDatabase::open(&path).expect("open");
    db.save_project(&sample_project("proj-1", "sirio"))
        .expect("project");
    let worktree = sample_worktree("wt-1", "proj-1", "main");
    db.save_worktree(&worktree).expect("worktree");
    assert_eq!(
        db.worktree_by_path(&worktree.path).expect("exact lookup"),
        Some(worktree)
    );
    assert_eq!(
        db.worktree_by_path("/Users/me/./main")
            .expect("nearby lookup"),
        None
    );
}

#[test]
fn current_schema_contains_named_persistence_migrations() {
    let dir = TempDir::new();
    let path = dir.db_path("named-migrations");
    let db = AppDatabase::open(&path).expect("open");
    assert_eq!(
        db.schema_version().expect("schema version"),
        CURRENT_SCHEMA_VERSION
    );
    drop(db);

    let conn = rusqlite::Connection::open(&path).expect("raw open");
    for table in [
        "session_ref",
        "chat_turn",
        "quarantine_record",
        "browser_origin_grant",
    ] {
        let present: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("table lookup");
        assert_eq!(present, 1, "named migration must create {table}");
    }
    let worktree_columns: Vec<String> = conn
        .prepare("PRAGMA table_info(worktree)")
        .expect("worktree schema")
        .query_map([], |row| row.get(1))
        .expect("worktree columns")
        .collect::<Result<Vec<_>, _>>()
        .expect("read worktree columns");
    for column in ["comment", "created_at", "updated_at"] {
        assert!(
            worktree_columns.iter().any(|value| value == column),
            "metadata migration must create {column}"
        );
    }
    let primary_index: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master
             WHERE type = 'index' AND name = 'worktree_one_primary_per_project'",
            [],
            |row| row.get(0),
        )
        .expect("primary index lookup");
    assert_eq!(primary_index, 1);
}

#[test]
fn tab_agent_identity_round_trips_through_a_real_database_file() {
    let dir = TempDir::new();
    let path = dir.db_path("tab-agent-identity");

    {
        let db = AppDatabase::open(&path).expect("open");
        db.save_project(&sample_project("proj-1", "sirio"))
            .expect("project");
        db.save_worktree(&sample_worktree("wt-1", "proj-1", "main"))
            .expect("worktree");
        let tab = TabRecord::new("tab-1", "wt-1", "Codex chat", "chat")
            .with_agent_id(AgentRef::adapter("codex"));
        db.save_tab(&tab).expect("tab");
    }

    let db = AppDatabase::open(&path).expect("reopen");
    let tabs = db.tabs_of_worktree("wt-1").expect("tabs");
    assert_eq!(tabs.len(), 1);
    assert_eq!(
        tabs[0].agent_id.as_ref().and_then(AgentRef::adapter_id),
        Some("codex")
    );
}

#[test]
fn v9_tab_rows_upgrade_to_current_with_no_recorded_agent_identity() {
    let dir = TempDir::new();
    let path = dir.db_path("v9-tab-agent-identity");

    {
        let mut conn = rusqlite::Connection::open(&path).expect("open raw database");
        conn.execute("PRAGMA foreign_keys = ON", [])
            .expect("pragmas");
        migrate_up_to(&mut conn, 9).expect("migrate to v9");
        conn.execute(
            "INSERT INTO project (id, name, root_path, icon_kind, order_idx)
             VALUES ('proj-1', 'sirio', '/Users/me/sirio', 'icon', 0)",
            [],
        )
        .expect("project");
        conn.execute(
            "INSERT INTO worktree (id, project_id, branch, path, order_idx)
             VALUES ('wt-1', 'proj-1', 'main', '/Users/me/sirio', 0)",
            [],
        )
        .expect("worktree");
        conn.execute(
            "INSERT INTO tab (id, worktree_id, title, kind, order_idx, is_active)
             VALUES ('tab-1', 'wt-1', 'Legacy chat', 'chat', 0, 1)",
            [],
        )
        .expect("tab");
    }

    let db = AppDatabase::open(&path).expect("upgrade v9 database");
    assert_eq!(
        db.schema_version().expect("schema version"),
        CURRENT_SCHEMA_VERSION
    );
    let tabs = db.tabs_of_worktree("wt-1").expect("legacy tabs");
    assert_eq!(tabs.len(), 1);
    assert_eq!(tabs[0].agent_id, None);
}

#[test]
fn a_corrupt_tab_is_skipped_and_quarantined_while_valid_tabs_survive() {
    let dir = TempDir::new();
    let path = dir.db_path("quarantine-tab");
    {
        let db = AppDatabase::open(&path).expect("open");
        db.save_project(&sample_project("proj-1", "sirio"))
            .expect("project");
        db.save_worktree(&sample_worktree("wt-1", "proj-1", "main"))
            .expect("worktree");
        db.save_tabs(
            "wt-1",
            &[
                sample_tab("tab-bad", "wt-1", "Bad", "terminal"),
                sample_tab("tab-good", "wt-1", "Good", "chat"),
            ],
        )
        .expect("tabs");
    }
    let conn = rusqlite::Connection::open(&path).expect("raw open");
    conn.execute(
        "UPDATE tab SET order_idx = 'not-an-integer' WHERE id = 'tab-bad'",
        [],
    )
    .expect("corrupt one tab");
    drop(conn);

    let db = AppDatabase::open(&path).expect("reopen");
    let tabs = db.tabs_of_worktree("wt-1").expect("load valid tabs");
    assert_eq!(tabs.len(), 1);
    assert_eq!(tabs[0].id, "tab-good");
    assert!(
        db.quarantined_records()
            .expect("quarantine")
            .iter()
            .any(|row| row.record_type == "tab" && row.record_id == "tab-bad")
    );
}

#[test]
fn a_corrupt_tab_state_is_quarantined_without_poisoning_siblings() {
    let dir = TempDir::new();
    let path = dir.db_path("quarantine-state");
    {
        let db = AppDatabase::open(&path).expect("open");
        db.save_project(&sample_project("proj-1", "sirio"))
            .expect("project");
        db.save_worktree(&sample_worktree("wt-1", "proj-1", "main"))
            .expect("worktree");
        db.save_tabs(
            "wt-1",
            &[
                sample_tab("tab-1", "wt-1", "Terminal", "terminal"),
                sample_tab("tab-2", "wt-1", "Chat", "chat"),
            ],
        )
        .expect("tabs");
        db.save_tab_states(
            "wt-1",
            &[
                TabStateRecord::new("tab-1", r#"{"ok":true}"#),
                TabStateRecord::new("tab-2", r#"{"ok":false}"#),
            ],
        )
        .expect("states");
    }
    let conn = rusqlite::Connection::open(&path).expect("raw open");
    conn.execute(
        "UPDATE tab_state SET state = 'not json' WHERE tab_id = 'tab-1'",
        [],
    )
    .expect("corrupt one state");
    drop(conn);

    let db = AppDatabase::open(&path).expect("reopen");
    let states = db.tab_states_of_worktree("wt-1").expect("load states");
    assert_eq!(
        states,
        vec![TabStateRecord::new("tab-2", r#"{"ok":false}"#)]
    );
    assert!(
        db.quarantined_records()
            .expect("quarantine")
            .iter()
            .any(|row| row.record_type == "tab_state" && row.record_id == "tab-1")
    );
}

#[test]
fn a_corrupt_chat_turn_is_quarantined_without_losing_other_turns() {
    let dir = TempDir::new();
    let path = dir.db_path("quarantine-chat");
    {
        let db = AppDatabase::open(&path).expect("open");
        db.save_project(&sample_project("proj-1", "sirio"))
            .expect("project");
        db.save_worktree(&sample_worktree("wt-1", "proj-1", "main"))
            .expect("worktree");
        db.save_tabs("wt-1", &[sample_tab("tab-chat", "wt-1", "Chat", "chat")])
            .expect("tab");
        db.save_chat_transcript(&ChatTranscript {
            tab_id: "tab-chat".into(),
            turns: vec![
                ChatTurn {
                    entries: vec![ChatEntry::UserMessage {
                        text: "first".into(),
                        at: None,
                    }],
                },
                ChatTurn {
                    entries: vec![ChatEntry::UserMessage {
                        text: "second".into(),
                        at: None,
                    }],
                },
            ],
        })
        .expect("transcript");
    }
    let conn = rusqlite::Connection::open(&path).expect("raw open");
    conn.execute(
        "UPDATE chat_turn SET payload = X'6E6F742D6A736F6E' WHERE ordinal = 0",
        [],
    )
    .expect("corrupt one turn");
    drop(conn);

    let db = AppDatabase::open(&path).expect("reopen");
    let transcript = db
        .load_chat_transcript("tab-chat")
        .expect("load transcript")
        .expect("valid sibling survives");
    assert_eq!(transcript.turns.len(), 1);
    assert!(matches!(
        transcript.turns[0].entries[0],
        ChatEntry::UserMessage { ref text, .. } if text == "second"
    ));
    assert!(
        db.quarantined_records()
            .expect("quarantine")
            .iter()
            .any(|row| row.record_type == "chat_turn")
    );
}

#[test]
fn round_trips_projects_worktrees_tabs_settings_and_sidebar() {
    let dir = TempDir::new();
    let path = dir.db_path("roundtrip");

    let settings = AppSettings {
        appearance: AppearanceMode::Dark,
        ui_font_size: 16,
        terminal_font_size: 14,
        control_socket_enabled: false,
        ..AppSettings::default()
    };
    let state = SidebarState {
        expanded_project_ids: vec!["proj-1".to_string()],
        selected_worktree_id: Some("wt-2".to_string()),
    };

    {
        let db = AppDatabase::open(&path).expect("open fresh database");

        let projects = vec![
            sample_project("proj-1", "sirio"),
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
    assert_eq!(
        db.schema_version().expect("version"),
        CURRENT_SCHEMA_VERSION
    );

    let projects = db.projects().expect("load projects");
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0].name, "sirio");
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
fn base_colour_survives_a_relaunch() {
    let dir = TempDir::new();
    let path = dir.db_path("base-colour");

    {
        let db = AppDatabase::open(&path).expect("open");
        let mut settings = db.settings().expect("load defaults");
        assert_eq!(
            settings.base_color,
            BaseColor::Neutral,
            "an install that never wrote the key reads as neutral"
        );
        settings.base_color = BaseColor::Slate;
        db.save_settings(&settings).expect("save settings");
    }
    // Drop closes the connection; reopen simulates a relaunch.
    let db = AppDatabase::open(&path).expect("reopen");
    assert_eq!(
        db.settings().expect("load settings").base_color,
        BaseColor::Slate
    );
}

#[test]
fn chat_transcript_survives_process_relaunch_with_tool_and_permission_outcome() {
    let dir = TempDir::new();
    let path = dir.db_path("chat-relaunch");

    let write = std::process::Command::new(std::env::current_exe().expect("test binary path"))
        .args(["--exact", "chat_relaunch_helper", "--nocapture"])
        .env("SIRIO_PERSISTENCE_CHAT_HELPER", &path)
        .env("SIRIO_PERSISTENCE_CHAT_MODE", "write")
        .output()
        .expect("spawn transcript writer");
    assert!(
        write.status.success(),
        "writer failed: {}",
        String::from_utf8_lossy(&write.stderr)
    );

    let read = std::process::Command::new(std::env::current_exe().expect("test binary path"))
        .args(["--exact", "chat_relaunch_helper", "--nocapture"])
        .env("SIRIO_PERSISTENCE_CHAT_HELPER", &path)
        .env("SIRIO_PERSISTENCE_CHAT_MODE", "read")
        .output()
        .expect("spawn transcript reader");
    assert!(
        read.status.success(),
        "reader failed: {}",
        String::from_utf8_lossy(&read.stderr)
    );
    assert!(
        String::from_utf8_lossy(&read.stdout).contains("CHAT_OK"),
        "reader must prove the restored transcript contents: {}",
        String::from_utf8_lossy(&read.stdout)
    );
}

#[test]
fn chat_relaunch_helper() {
    let Ok(database_path) = std::env::var("SIRIO_PERSISTENCE_CHAT_HELPER") else {
        return;
    };
    let mode = std::env::var("SIRIO_PERSISTENCE_CHAT_MODE").expect("chat helper mode");
    let path = Path::new(&database_path);

    match mode.as_str() {
        "write" => {
            let db = AppDatabase::open(path).expect("open writer database");
            db.save_project(&sample_project("proj-chat", "chat"))
                .expect("save project");
            db.save_worktree(&sample_worktree("wt-chat", "proj-chat", "main"))
                .expect("save worktree");
            db.save_tabs(
                "wt-chat",
                &[sample_tab("tab-chat", "wt-chat", "Chat", "chat")],
            )
            .expect("save chat tab");
            db.save_chat_transcript(&sample_chat_transcript())
                .expect("save transcript");
        }
        "read" => {
            let db = AppDatabase::open(path).expect("reopen reader database");
            assert_eq!(
                db.load_chat_transcript("tab-chat")
                    .expect("load transcript"),
                Some(sample_chat_transcript())
            );
            println!("CHAT_OK");
        }
        other => panic!("unknown chat helper mode {other}"),
    }
}

fn sample_chat_transcript() -> ChatTranscript {
    ChatTranscript {
        tab_id: "tab-chat".into(),
        turns: vec![
            ChatTurn {
                entries: vec![
                    ChatEntry::UserMessage {
                        text: "Inspect the project".into(),
                        at: None,
                    },
                    ChatEntry::AssistantMessage {
                        text: "I will inspect it now.".into(),
                    },
                    ChatEntry::ToolCall {
                        id: "tool-1".into(),
                        title: "Read file".into(),
                        status: "Completed".into(),
                        kind: Some("Read".into()),
                        locations: vec![ChatToolLocation {
                            path: "src/main.rs".into(),
                            line: Some(42),
                        }],
                        duration_ms: None,
                    },
                ],
            },
            ChatTurn {
                entries: vec![
                    ChatEntry::UserMessage {
                        text: "Can I apply the change?".into(),
                        at: None,
                    },
                    ChatEntry::Permission {
                        request_id: 7,
                        title: String::new(),
                        options: vec![ChatPermissionOption {
                            id: "allow-once".into(),
                            name: "Allow once".into(),
                            kind: "allow_once".into(),
                        }],
                        outcome: ChatPermissionOutcome::Selected {
                            option_id: "allow-once".into(),
                            label: "Allow once".into(),
                        },
                    },
                    ChatEntry::AssistantMessage {
                        text: "The change was applied.".into(),
                    },
                ],
            },
        ],
    }
}

fn sample_turn(text: &str) -> ChatTurn {
    ChatTurn {
        entries: vec![ChatEntry::UserMessage { text: text.into(), at: None }],
    }
}

#[test]
fn chat_sessions_list_saved_tabs_by_activity_and_delete_only_transcript() {
    let dir = TempDir::new();
    let db = AppDatabase::open(&dir.db_path("chat-history")).expect("open");
    db.save_project(&sample_project("project", "Project"))
        .expect("project");
    db.save_worktree(&sample_worktree("worktree", "project", "main"))
        .expect("worktree");
    let mut first = sample_tab("chat-1", "worktree", "First chat", "chat");
    first.agent_id = Some(AgentRef::adapter("codex"));
    let second = sample_tab("chat-2", "worktree", "Second chat", "chat");
    db.save_tabs("worktree", &[first, second]).expect("tabs");
    db.save_chat_transcript(&ChatTranscript {
        tab_id: "chat-1".into(),
        turns: vec![sample_turn("one")],
    })
    .expect("first");
    std::thread::sleep(std::time::Duration::from_millis(2));
    db.save_chat_transcript(&ChatTranscript {
        tab_id: "chat-2".into(),
        turns: vec![sample_turn("two"), sample_turn("three")],
    })
    .expect("second");

    let sessions = db.chat_sessions("worktree").expect("list");
    assert_eq!(
        sessions
            .iter()
            .map(|session| session.tab_id.as_str())
            .collect::<Vec<_>>(),
        ["chat-2", "chat-1"]
    );
    assert_eq!(sessions[0].title, "Second chat");
    assert_eq!(sessions[0].turn_count, 2);
    assert_eq!(
        sessions[1].agent_id.as_ref().and_then(AgentRef::adapter_id),
        Some("codex")
    );
    assert!(sessions[0].last_activity >= sessions[1].last_activity);

    assert!(db.delete_chat_session("chat-2").expect("delete"));
    assert!(
        db.chat_sessions("worktree")
            .expect("list after delete")
            .iter()
            .all(|session| session.tab_id != "chat-2")
    );
    assert!(
        db.tabs_of_worktree("worktree")
            .expect("tabs after delete")
            .iter()
            .any(|tab| tab.id == "chat-2")
    );
}

#[test]
fn tab_state_round_trips_and_is_removed_with_replaced_tabs() {
    let dir = TempDir::new();
    let path = dir.db_path("tab-state");
    let db = AppDatabase::open(&path).expect("open database");
    db.save_project(&sample_project("proj-1", "sirio"))
        .expect("save project");
    db.save_worktree(&sample_worktree("wt-1", "proj-1", "main"))
        .expect("save worktree");
    db.save_tabs(
        "wt-1",
        &[sample_tab("tab-1", "wt-1", "Terminal", "terminal")],
    )
    .expect("save tab");

    let state = TabStateRecord::new("tab-1", r#"{"events":["split"]}"#);
    db.save_tab_states("wt-1", std::slice::from_ref(&state))
        .expect("save tab state");
    assert_eq!(
        db.tab_states_of_worktree("wt-1").expect("load state"),
        vec![state]
    );

    db.save_tabs("wt-1", &[sample_tab("tab-2", "wt-1", "Chat", "chat")])
        .expect("replace tabs");
    assert!(
        db.tab_states_of_worktree("wt-1")
            .expect("load replaced states")
            .is_empty(),
        "tab state must follow the tab row and not survive replacement"
    );
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
             VALUES ('proj-1', 'sirio', '/Users/me/sirio', 'icon', 0)",
            [],
        )
        .expect("seed project");
        conn.execute(
            "INSERT INTO worktree (id, project_id, branch, path, order_idx)
             VALUES ('wt-1', 'proj-1', 'main', '/Users/me/sirio', 0)",
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

    // Opening with the real database migrates v1 to the current schema.
    let db = AppDatabase::open(&path).expect("open migrates forward");
    assert_eq!(
        db.schema_version().expect("version"),
        CURRENT_SCHEMA_VERSION
    );

    let projects = db.projects().expect("load projects");
    assert_eq!(projects.len(), 1, "project survived the migration");
    assert_eq!(projects[0].id, "proj-1");
    assert_eq!(projects[0].name, "sirio");

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

    // The v1 rows above remain usable after the additive chat migration.
    db.save_tabs(
        "wt-1",
        &[
            sample_tab("tab-1", "wt-1", "Terminal", "terminal"),
            sample_tab("tab-chat", "wt-1", "Chat", "chat"),
        ],
    )
    .expect("save migrated tabs");
    db.save_chat_transcript(&sample_chat_transcript())
        .expect("save transcript after old-db migration");
    assert_eq!(
        db.load_chat_transcript("tab-chat")
            .expect("load migrated transcript"),
        Some(sample_chat_transcript())
    );
}

#[test]
fn chat_transcript_retains_newest_complete_turns_under_one_megabyte() {
    let dir = TempDir::new();
    let db = AppDatabase::open(&dir.db_path("chat-bound")).expect("open");
    db.save_project(&sample_project("proj-chat", "chat"))
        .expect("save project");
    db.save_worktree(&sample_worktree("wt-chat", "proj-chat", "main"))
        .expect("save worktree");
    db.save_tabs(
        "wt-chat",
        &[sample_tab("tab-chat", "wt-chat", "Chat", "chat")],
    )
    .expect("save chat tab");

    let turns = (0..20)
        .map(|index| ChatTurn {
            entries: vec![ChatEntry::AssistantMessage {
                text: format!("marker-{index}-{}", "x".repeat(100_000)),
            }],
        })
        .collect();
    db.save_chat_transcript(&ChatTranscript {
        tab_id: "tab-chat".into(),
        turns,
    })
    .expect("save bounded transcript");

    let restored = db
        .load_chat_transcript("tab-chat")
        .expect("load bounded transcript")
        .expect("some newest turns fit");
    assert!(restored.turns.len() < 20, "old turns must be pruned");
    let first_text = match &restored.turns[0].entries[0] {
        ChatEntry::AssistantMessage { text } => text,
        entry => panic!("unexpected restored entry: {entry:?}"),
    };
    let last_text = match &restored.turns.last().expect("last turn").entries[0] {
        ChatEntry::AssistantMessage { text } => text,
        entry => panic!("unexpected restored entry: {entry:?}"),
    };
    assert!(!first_text.starts_with("marker-0-"));
    assert!(last_text.starts_with("marker-19-"));
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
fn an_existing_empty_file_is_corrupt_not_a_fresh_store() {
    let dir = TempDir::new();
    let path = dir.db_path("empty-existing");
    std::fs::File::create(&path).expect("create empty file");

    let error = AppDatabase::open(&path).expect_err("empty existing file must fail");
    assert!(
        matches!(error, PersistenceError::Corrupt { .. }),
        "empty database error: {error:?}"
    );
    assert_eq!(
        std::fs::metadata(&path).expect("metadata").len(),
        0,
        "rejected empty file must not be initialized"
    );
}

#[test]
fn a_database_truncated_after_close_is_corrupt_and_untouched() {
    let dir = TempDir::new();
    let path = dir.db_path("truncated");
    {
        let db = AppDatabase::open(&path).expect("seed database");
        db.save_project(&sample_project("proj-1", "sirio"))
            .expect("seed row");
    }
    let original_len = std::fs::metadata(&path).expect("metadata").len();
    let page_size = {
        let connection = rusqlite::Connection::open(&path).expect("open seeded database");
        connection
            .query_row("PRAGMA page_size", [], |row| row.get::<_, u64>(0))
            .expect("read SQLite page size")
    };
    assert!(
        original_len > page_size,
        "seed database must contain more than one page"
    );
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open for truncation");
    file.set_len(original_len - page_size)
        .expect("truncate database");
    let truncated_len = std::fs::metadata(&path).expect("metadata").len();

    let error = AppDatabase::open(&path).expect_err("truncated file must fail");
    assert!(
        matches!(error, PersistenceError::Corrupt { .. }),
        "truncated database error: {error:?}"
    );
    assert_eq!(
        std::fs::metadata(&path).expect("metadata").len(),
        truncated_len,
        "rejected truncated file must not be rewritten"
    );
}

#[test]
fn each_database_connection_installs_a_logical_size_limit() {
    let dir = TempDir::new();
    let path = dir.db_path("size-limit");
    let db = AppDatabase::open(&path).expect("open");
    assert_eq!(
        db.database_limit_bytes().expect("database limit"),
        MAX_DATABASE_BYTES
    );
}

#[test]
fn session_references_upsert_load_and_delete() {
    let dir = TempDir::new();
    let db = AppDatabase::open(&dir.db_path("session-ref-lifecycle")).expect("open");

    db.save_session_ref("pane-1", "session-a")
        .expect("insert session reference");
    db.save_session_ref("pane-1", "session-b")
        .expect("replace session reference");
    let references = db.session_refs().expect("load session references");
    assert_eq!(
        references.get("pane-1").map(String::as_str),
        Some("session-b")
    );

    db.delete_session_ref("pane-1")
        .expect("delete session reference");
    assert!(
        !db.session_refs()
            .expect("load after delete")
            .contains_key("pane-1")
    );
}

#[test]
fn account_identity_upserts_one_row_per_provider_and_survives_a_relaunch() {
    // F-PERSIST-DB-06: discover_claude_identity/discover_codex_identity had
    // nowhere to persist a successful shell-out, so the display line could
    // not survive a restart or a slow/offline CLI. Same upsert contract as
    // save_session_ref: an unknown provider reads back None, a saved
    // identity replaces wholesale rather than accumulating history, and the
    // row survives closing and reopening the database file.
    let dir = TempDir::new();
    let path = dir.db_path("account-identity-lifecycle");

    {
        let db = AppDatabase::open(&path).expect("open writer database");
        assert_eq!(
            db.account_identity("claude")
                .expect("query unknown provider"),
            None,
            "no identity has been saved for claude yet"
        );

        db.save_account_identity("claude", "signed in as dev@example.com")
            .expect("insert claude identity");
        let (identity, first_detected_at) = db
            .account_identity("claude")
            .expect("load claude identity")
            .expect("a row exists after saving");
        assert_eq!(identity, "signed in as dev@example.com");

        db.save_account_identity("claude", "signed in as dev2@example.com")
            .expect("replace claude identity");
        let (identity, replaced_detected_at) = db
            .account_identity("claude")
            .expect("load replaced claude identity")
            .expect("still exactly one row for claude");
        assert_eq!(
            identity, "signed in as dev2@example.com",
            "a second save replaces the row wholesale rather than appending history"
        );
        assert!(
            replaced_detected_at >= first_detected_at,
            "detected_at advances on replace"
        );

        db.save_account_identity("codex", "signed in as ops@example.com")
            .expect("insert codex identity");
        assert_eq!(
            db.account_identity("claude")
                .expect("claude row")
                .map(|(identity, _)| identity),
            Some("signed in as dev2@example.com".to_string()),
            "a different provider's row does not disturb claude's"
        );
    }

    let db = AppDatabase::open(&path).expect("reopen database after relaunch");
    assert_eq!(
        db.account_identity("claude")
            .expect("claude survives relaunch")
            .map(|(identity, _)| identity),
        Some("signed in as dev2@example.com".to_string())
    );
    assert_eq!(
        db.account_identity("codex")
            .expect("codex survives relaunch")
            .map(|(identity, _)| identity),
        Some("signed in as ops@example.com".to_string())
    );
}

#[test]
fn browser_origin_grants_survive_relaunch_and_revoke() {
    let dir = TempDir::new();
    let path = dir.db_path("browser-origins");

    {
        let db = AppDatabase::open(&path).expect("open database");
        db.save_browser_origin_grant("https://agent.example")
            .expect("save first origin");
        db.save_browser_origin_grant("https://docs.example")
            .expect("save second origin");
        db.save_browser_origin_grant("https://agent.example")
            .expect("repeated allow is idempotent");
        assert_eq!(
            db.browser_origin_grants().expect("load origins"),
            vec![
                "https://agent.example".to_owned(),
                "https://docs.example".to_owned()
            ]
        );
    }

    let db = AppDatabase::open(&path).expect("reopen database");
    assert_eq!(
        db.browser_origin_grants().expect("load after relaunch"),
        vec![
            "https://agent.example".to_owned(),
            "https://docs.example".to_owned()
        ]
    );
    assert!(
        db.revoke_browser_origin("https://agent.example")
            .expect("revoke one origin")
    );
    assert!(
        !db.revoke_browser_origin("https://missing.example")
            .expect("missing revoke is harmless")
    );
    assert_eq!(db.revoke_all_browser_origins().expect("revoke all"), 1);
    assert!(
        db.browser_origin_grants()
            .expect("load after revoke")
            .is_empty()
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
fn linux_settings_survive_a_database_relaunch_with_contract_clamps() {
    let dir = TempDir::new();
    let path = dir.db_path("linux-settings-relaunch");
    {
        let db = AppDatabase::open(&path).expect("open writer database");
        db.save_settings(&AppSettings {
            updates_enabled: false,
            resume_agent_sessions: false,
            auto_naming: true,
            limit_chat_history: false,
            chat_retention: 999,
            limit_mounted_worktrees: true,
            mounted_worktrees: 1,
            summarizer_agent: "opencode".into(),
            claude_show_in_bar: false,
            codex_show_in_bar: false,
            opencode_show_in_bar: true,
            ollama_show_in_bar: true,
            refresh_interval_min: 99,
            opencode_workspace_id_override: "wrk_relaunch".into(),
            translucency: true,
            ..AppSettings::default()
        })
        .expect("save Linux settings");
    }

    let db = AppDatabase::open(&path).expect("reopen database after relaunch");
    let settings = db.settings().expect("load Linux settings");
    assert!(!settings.updates_enabled);
    assert!(!settings.resume_agent_sessions);
    assert!(settings.auto_naming);
    assert!(!settings.limit_chat_history);
    assert_eq!(settings.chat_retention, 500);
    assert!(settings.limit_mounted_worktrees);
    assert_eq!(settings.mounted_worktrees, 2);
    assert_eq!(settings.summarizer_agent, "opencode");
    assert!(!settings.claude_show_in_bar);
    assert!(!settings.codex_show_in_bar);
    assert!(settings.opencode_show_in_bar);
    // F-SET-13: the Ollama Cloud toggle rides the same contract — a saved
    // `true` survives the relaunch (the default is false).
    assert!(settings.ollama_show_in_bar);
    assert_eq!(settings.refresh_interval_min, 60);
    // F-SET-12: the workspace-ID override survives the relaunch verbatim —
    // free text, no clamp, and clearing it (saving "") must also survive.
    assert_eq!(settings.opencode_workspace_id_override, "wrk_relaunch");
    // F-SET-20: a saved `true` survives the relaunch (the default is false).
    assert!(settings.translucency);
    db.save_settings(&AppSettings {
        opencode_workspace_id_override: String::new(),
        ..settings
    })
    .expect("save the cleared override");
    assert_eq!(
        db.settings()
            .expect("reload")
            .opencode_workspace_id_override,
        "",
        "a cleared override persists as empty, not as the stale value"
    );
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
            control_socket_enabled: true,
            ..AppSettings::default()
        })
        .expect("save");
    }

    // Hand-corrupt one key to prove unparseable values fall back.
    {
        let conn = rusqlite::Connection::open(&path).expect("open raw");
        conn.execute(
            "UPDATE setting SET value = 'banana' WHERE key = 'appearance.baseColor'",
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
        settings.base_color,
        BaseColor::Neutral,
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
    db.save_projects(&[sample_project("proj-1", "sirio")])
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
    db.save_projects(&[sample_project("proj-1", "sirio")])
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

/// F-CORE-WSP-08 regression: moving the active tab from a later array
/// position to an earlier one, across two `save_tabs` calls, used to fail
/// the whole write with `UNIQUE constraint failed: tab.worktree_id`. The
/// upsert loop processed tabs in array order, so upserting the
/// newly-active earlier tab happened before the still-active-in-the-database
/// later tab's row had its flag cleared — a transient two-actives state the
/// partial unique index (checked immediately, not deferrable) rejected.
#[test]
fn save_tabs_moving_the_active_flag_to_an_earlier_tab_does_not_violate_the_unique_index() {
    let dir = TempDir::new();
    let db = AppDatabase::open(&dir.db_path("active-earlier")).expect("open");
    db.save_projects(&[sample_project("proj-1", "sirio")])
        .expect("seed project");
    db.save_worktrees(&[sample_worktree("wt-1", "proj-1", "main")])
        .expect("seed worktree");

    let mut a = sample_tab("tab-a", "wt-1", "Chat", "chat");
    a.is_active = false;
    let mut b = sample_tab("tab-b", "wt-1", "Terminal", "terminal");
    b.is_active = true;
    db.save_tabs("wt-1", &[a.clone(), b.clone()])
        .expect("first save: b (later position) active");

    a.is_active = true;
    b.is_active = false;
    db.save_tabs("wt-1", &[a, b])
        .expect("second save: a (earlier position) becomes active");

    let tabs = db.tabs_of_worktree("wt-1").expect("load tabs");
    assert!(tabs[0].is_active, "tab-a is now the sole active tab");
    assert!(!tabs[1].is_active, "tab-b lost its active flag");
}

#[test]
fn save_tab_keeps_a_single_active_tab_per_worktree() {
    let dir = TempDir::new();
    let db = AppDatabase::open(&dir.db_path("single-active")).expect("open");
    db.save_projects(&[sample_project("proj-1", "sirio")])
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
             VALUES ('proj-1', 'sirio', '/Users/me/sirio', 'icon', 0)",
            [],
        )
        .expect("seed project");
        conn.execute(
            "INSERT INTO worktree (id, project_id, branch, path, order_idx)
             VALUES ('wt-1', 'proj-1', 'main', '/Users/me/sirio', 0)",
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
    assert_eq!(
        db.schema_version().expect("version"),
        CURRENT_SCHEMA_VERSION
    );

    let tabs = db.tabs_of_worktree("wt-1").expect("load tabs");
    assert_eq!(tabs.len(), 2, "rows survive the migration");
    let active: Vec<_> = tabs.iter().filter(|tab| tab.is_active).collect();
    assert_eq!(
        active.len(),
        1,
        "exactly one active tab after reconciliation"
    );
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
        db.save_projects(&[sample_project("proj-1", "sirio")])
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
        sample_project("proj-1", "sirio"),
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
        db.save_project(&sample_project("proj-1", "sirio"))
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
/// Triggered by the `SIRIO_PERSISTENCE_HELPER` environment variable, so the
/// parent can re-execute this test binary as the child (`current_exe`). When
/// the variable is absent this is a no-op test.
#[test]
fn helper_process() {
    let Ok(database_path) = std::env::var("SIRIO_PERSISTENCE_HELPER") else {
        return;
    };
    let go_file = std::env::var("SIRIO_PERSISTENCE_GO").expect("go file env");
    let ready_file = std::env::var("SIRIO_PERSISTENCE_READY").expect("ready file env");

    std::fs::write(&ready_file, "ready")
        .unwrap_or_else(|error| helper_fail(3, &format!("cannot write ready file: {error}")));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !Path::new(&go_file).exists() {
        if std::time::Instant::now() > deadline {
            helper_fail(2, "go file never appeared");
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    std::fs::write(format!("{ready_file}.started"), "started")
        .unwrap_or_else(|error| helper_fail(7, &format!("cannot write started file: {error}")));

    let db = match AppDatabase::open(Path::new(&database_path)) {
        Ok(db) => db,
        Err(error) => helper_fail(1, &format!("open failed: {error:?}")),
    };
    let version = db
        .schema_version()
        .unwrap_or_else(|error| helper_fail(4, &format!("schema version unreadable: {error:?}")));
    if version != sirio_persistence::CURRENT_SCHEMA_VERSION {
        helper_fail(
            5,
            &format!(
                "schema version {version}, expected {}",
                sirio_persistence::CURRENT_SCHEMA_VERSION
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

/// A real writer child used by the concurrent-writer evidence test. Each
/// child quits after saving disjoint records to the same SQLite file.
#[test]
fn writer_process() {
    let Ok(database_path) = std::env::var("SIRIO_PERSISTENCE_WRITER") else {
        return;
    };
    let go_file = std::env::var("SIRIO_PERSISTENCE_WRITER_GO").expect("writer go file env");
    let ready_file =
        std::env::var("SIRIO_PERSISTENCE_WRITER_READY").expect("writer ready file env");
    let writer_id = std::env::var("SIRIO_PERSISTENCE_WRITER_ID").expect("writer id env");

    std::fs::write(&ready_file, "ready").unwrap_or_else(|error| {
        helper_fail(7, &format!("cannot write writer ready file: {error}"))
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !Path::new(&go_file).exists() {
        if std::time::Instant::now() > deadline {
            helper_fail(8, "writer go file never appeared");
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    let db = match AppDatabase::open(Path::new(&database_path)) {
        Ok(db) => db,
        Err(error) => helper_fail(9, &format!("writer open failed: {error:?}")),
    };
    for index in 0..20 {
        let project_id = format!("writer-{writer_id}-project-{index}");
        let worktree_id = format!("writer-{writer_id}-worktree-{index}");
        let tab_id = format!("writer-{writer_id}-tab-{index}");
        db.save_project(&sample_project(&project_id, &project_id))
            .unwrap_or_else(|error| helper_fail(10, &format!("save project failed: {error:?}")));
        db.save_worktree(&sample_worktree(&worktree_id, &project_id, "main"))
            .unwrap_or_else(|error| helper_fail(11, &format!("save worktree failed: {error:?}")));
        db.save_tabs(
            &worktree_id,
            &[sample_tab(&tab_id, &worktree_id, "Terminal", "terminal")],
        )
        .unwrap_or_else(|error| helper_fail(12, &format!("save tab failed: {error:?}")));
    }
    println!("OK");
    std::process::exit(0);
}

fn helper_fail(code: i32, message: &str) -> ! {
    eprintln!("helper failed: {message}");
    std::process::exit(code);
}

/// Polls `ready` until it returns true or `deadline` passes.
fn wait_until(deadline: std::time::Instant, what: &str, mut ready: impl FnMut() -> bool) -> bool {
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
fn spawn_helper(dir: &Path, database_path: &Path, ready: &Path, go: &Path) -> std::process::Child {
    std::process::Command::new(std::env::current_exe().expect("test binary path"))
        .args(["--exact", "helper_process", "--nocapture"])
        .env("SIRIO_PERSISTENCE_HELPER", database_path)
        .env("SIRIO_PERSISTENCE_GO", go)
        .env("SIRIO_PERSISTENCE_READY", ready)
        .current_dir(dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn helper process")
}

fn spawn_writer(
    dir: &Path,
    database_path: &Path,
    ready: &Path,
    go: &Path,
    writer_id: &str,
) -> std::process::Child {
    std::process::Command::new(std::env::current_exe().expect("test binary path"))
        .args(["--exact", "writer_process", "--nocapture"])
        .env("SIRIO_PERSISTENCE_WRITER", database_path)
        .env("SIRIO_PERSISTENCE_WRITER_GO", go)
        .env("SIRIO_PERSISTENCE_WRITER_READY", ready)
        .env("SIRIO_PERSISTENCE_WRITER_ID", writer_id)
        .current_dir(dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn writer process")
}

/// Waits for `children` to exit (bounded), asserting every one exited 0 with
/// `OK` on stdout.
fn assert_all_helpers_succeeded(children: Vec<std::process::Child>, deadline: std::time::Instant) {
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
    let started_a = dir.0.join("ready-a.started");
    let started_b = dir.0.join("ready-b.started");
    assert!(wait_until(deadline, "both migration attempts", || {
        started_a.exists() && started_b.exists()
    }));
    held_lock.rollback().expect("release the lock");
    drop(conn);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    assert_all_helpers_succeeded(vec![child_a, child_b], deadline);

    // The schema is complete and correct after both processes raced.
    assert_eq!(
        db_schema_version(&database_path),
        sirio_persistence::CURRENT_SCHEMA_VERSION
    );
    let db = AppDatabase::open(&database_path).expect("reopen for verification");
    assert_eq!(
        db.schema_version().expect("version"),
        sirio_persistence::CURRENT_SCHEMA_VERSION
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
        db.save_project(&sample_project("proj-1", "sirio"))
            .expect("seed project");
    }
    assert_eq!(
        db_schema_version(&database_path),
        sirio_persistence::CURRENT_SCHEMA_VERSION
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

/// Two real app instances write disjoint records to one store and then quit.
/// WAL plus the busy timeout must preserve every record and leave a database
/// that passes integrity checking; the package does not promise merging two
/// updates to the same row.
#[test]
fn concurrent_writers_save_disjoint_records_and_exit() {
    let dir = TempDir::new();
    let database_path = dir.db_path("concurrent-writers");
    let go = dir.0.join("writer-go");
    let ready_a = dir.0.join("writer-ready-a");
    let ready_b = dir.0.join("writer-ready-b");
    let child_a = spawn_writer(&dir.0, &database_path, &ready_a, &go, "a");
    let child_b = spawn_writer(&dir.0, &database_path, &ready_b, &go, "b");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    assert!(
        wait_until(deadline, "both writer ready files", || {
            ready_a.exists() && ready_b.exists()
        }),
        "writer children never signalled ready"
    );
    std::fs::write(&go, "go").expect("release writer children");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    assert_all_helpers_succeeded(vec![child_a, child_b], deadline);

    let db = AppDatabase::open(&database_path).expect("reopen after both writers quit");
    assert_eq!(db.projects().expect("projects").len(), 40);
    assert_eq!(db.worktrees().expect("worktrees").len(), 40);
    assert_eq!(db.tabs().expect("tabs").len(), 40);

    let conn = rusqlite::Connection::open(&database_path).expect("raw integrity connection");
    let check: String = conn
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .expect("quick check");
    assert_eq!(check, "ok", "concurrent writers left a valid SQLite store");
}

/// A write that reads first must *wait* for a peer holding the write lock,
/// not fail as busy.
///
/// `save_tabs` reads the worktree's existing tab ids before replacing them, so
/// under `BEGIN DEFERRED` its writes are a read→write *upgrade*: the SELECT
/// takes a read snapshot, and the UPDATE behind it then asks for a write lock
/// the connection did not hold when the transaction opened. SQLite refuses
/// that upgrade without ever running the busy handler — waiting while already
/// holding a read lock can deadlock — so `SQLITE_BUSY` comes back at once and
/// the connection's five-second `busy_timeout` is never consulted. That is why
/// concurrent writers failed here about one run in five, and why the two saves
/// beside it in the same loop never did: `save_project` and `save_worktree`
/// both open with a write, which is the path where the timeout works.
///
/// The peer holds the lock far longer than a refusal takes to surface, so the
/// outcome is a decision about behaviour rather than a race. The elapsed
/// assertion is the positive control: it proves the contention this test
/// claims to create actually happened, so a pass cannot come from a peer that
/// quietly failed to take the lock at all.
#[test]
fn a_contended_write_waits_for_the_peer_instead_of_failing() {
    const HELD_FOR: std::time::Duration = std::time::Duration::from_millis(750);

    let dir = TempDir::new();
    let database_path = dir.db_path("contended-write");
    let db = AppDatabase::open(&database_path).expect("open the database under test");
    db.save_project(&sample_project("p", "p"))
        .expect("seed a project");
    db.save_worktree(&sample_worktree("w", "p", "main"))
        .expect("seed a worktree for the tab to hang off");

    // A second connection takes the write lock and keeps it for HELD_FOR.
    let peer = rusqlite::Connection::open(&database_path).expect("peer connection");
    peer.busy_timeout(std::time::Duration::from_secs(5))
        .expect("peer busy timeout");
    peer.execute_batch("BEGIN IMMEDIATE")
        .expect("peer takes the write lock");
    let peer_release = std::thread::spawn(move || {
        std::thread::sleep(HELD_FOR);
        peer.execute_batch("COMMIT")
            .expect("peer releases the write lock");
    });

    let started = std::time::Instant::now();
    let outcome = db.save_tabs("w", &[sample_tab("t", "w", "Terminal", "terminal")]);
    let waited = started.elapsed();
    peer_release.join().expect("peer thread");

    outcome.expect("a contended write must wait for the peer, not fail as busy");
    assert!(
        waited >= HELD_FOR / 2,
        "the write returned after {waited:?}, so it never contended with the peer \
         that held the lock for {HELD_FOR:?} — a pass here would prove nothing"
    );
    assert_eq!(
        db.tabs().expect("tabs").len(),
        1,
        "the write that waited its turn still landed"
    );
}

/// No write path may open a deferred transaction.
///
/// The test above proves one call site waits under contention. This one keeps
/// the other ten honest, because the defect is invisible at the call site: the
/// tempting `conn.unchecked_transaction()` compiles, reads like the obvious
/// thing, and only misbehaves when a second writer happens to hold the lock at
/// the moment a read-then-write transaction tries to upgrade. Route writes
/// through `write_transaction`, which is `BEGIN IMMEDIATE`.
///
/// The needle is assembled at runtime so this test does not match itself.
#[test]
fn no_write_path_opens_a_deferred_transaction() {
    let source = include_str!("../src/db.rs");
    let needle = format!("{}_{}(", "unchecked", "transaction");
    let offenders: Vec<&str> = source
        .lines()
        .filter(|line| line.contains(&needle))
        .collect();
    assert!(
        offenders.is_empty(),
        "db.rs opens a deferred transaction, which cannot wait on a peer's \
         write lock — use write_transaction instead:\n{}",
        offenders.join("\n")
    );
}

/// The panel widths are Linux-rewrite-only keys with no Swift antecedent.
/// They round-trip, clamp into their ranges, and fall back to the drawn
/// geometry (325 / 405) when never written — which is what makes wiring them
/// visually a no-op on first launch.
#[test]
fn panel_widths_round_trip_and_clamp_into_their_ranges() {
    let dir = TempDir::new();
    let path = dir.db_path("panel-widths");

    {
        let db = AppDatabase::open(&path).expect("open");
        let fresh = db.settings().expect("load defaults");
        assert_eq!(fresh.sidebar_width, 325, "matches the geometry drawn today");
        assert_eq!(
            fresh.right_panel_width, 405,
            "matches the geometry drawn today"
        );

        db.save_settings(&AppSettings {
            sidebar_width: 900,     // above the 220...480 range
            right_panel_width: 100, // below the 220...640 range
            ..AppSettings::default()
        })
        .expect("save");
    }

    let db = AppDatabase::open(&path).expect("reopen");
    let settings = db.settings().expect("load");
    assert_eq!(settings.sidebar_width, 480, "clamped to the upper bound");
    assert_eq!(
        settings.right_panel_width, 220,
        "clamped to the lower bound"
    );

    // The sidebar's own floor, which nothing on this side pinned before. It
    // moved from 160 to 220 when the panel's rows turned out not to fit
    // below that, so a width written under the old floor has to come back
    // raised rather than reopening a sidebar its own content cannot use.
    db.save_settings(&AppSettings {
        sidebar_width: 160,
        ..settings
    })
    .expect("save a width below the floor");
    let settings = db.settings().expect("reload");
    assert_eq!(
        settings.sidebar_width, 220,
        "a width stored under the floor is raised to it"
    );

    db.save_settings(&AppSettings {
        sidebar_width: 300,
        right_panel_width: 500,
        ..settings
    })
    .expect("save in-range values");
    let settings = db.settings().expect("reload");
    assert_eq!(
        settings.sidebar_width, 300,
        "an in-range width survives verbatim"
    );
    assert_eq!(settings.right_panel_width, 500);
}

#[test]
fn the_centre_split_ratio_survives_a_relaunch_and_clamps_a_corrupt_value() {
    let dir = TempDir::new();
    let path = dir.db_path("centre-split-ratio");
    {
        let db = AppDatabase::open(&path).expect("open");
        db.save_settings(&AppSettings {
            center_split_ratio: 640,
            ..AppSettings::default()
        })
        .expect("save the dragged ratio");
    }

    assert_eq!(
        AppDatabase::open(&path)
            .expect("reopen")
            .settings()
            .expect("load")
            .center_split_ratio,
        640,
        "a ratio inside the range comes back verbatim"
    );

    // Hand-edit the row past the safety net: the range exists to stop a value
    // like this driving a pane to zero width.
    {
        let conn = rusqlite::Connection::open(&path).expect("open raw");
        conn.execute(
            "UPDATE setting SET value = '10000' WHERE key = 'appearance.centerSplitRatio'",
            [],
        )
        .expect("corrupt the ratio");
    }

    assert_eq!(
        AppDatabase::open(&path)
            .expect("reopen")
            .settings()
            .expect("load")
            .center_split_ratio,
        900,
        "clamped to the upper bound, not honoured"
    );
}

#[test]
fn a_worktrees_secondary_pane_flag_survives_a_relaunch_and_defaults_closed() {
    let dir = TempDir::new();
    let path = dir.db_path("secondary-pane-flag");
    {
        let db = AppDatabase::open(&path).expect("open");
        db.save_project(&sample_project("p1", "Sirio"))
            .expect("save project");
        db.save_worktree(&WorktreeRecord {
            secondary_pane_open: true,
            ..sample_worktree("w1", "p1", "main")
        })
        .expect("save the worktree with its pane open");
        db.save_worktree(&sample_worktree("w2", "p1", "feature"))
            .expect("save a worktree that never opened one");
    }

    let db = AppDatabase::open(&path).expect("reopen");
    let worktrees = db.worktrees().expect("load worktrees");
    let flag = |id: &str| {
        worktrees
            .iter()
            .find(|worktree| worktree.id == id)
            .expect("worktree present")
            .secondary_pane_open
    };
    assert!(flag("w1"), "an open pane reopens open");
    assert!(
        !flag("w2"),
        "a worktree that never opened one stays closed"
    );
}
