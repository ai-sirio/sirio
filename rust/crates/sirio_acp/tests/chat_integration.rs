use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use sirio_acp::{AcpError, AgentCommand, ChatSession, ChatSessionConfig, ChatStatus};
use sirio_persistence::{AppDatabase, ProjectRecord, TabRecord, WorktreeRecord};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/acp_fixture.py");

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "sirio-chat-session-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("create temp directory");
        Self(path)
    }

    fn db_path(&self) -> PathBuf {
        self.0.join("sirio.sqlite")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn seed_chat_tab(path: &Path, tab_id: &str, worktree_id: &str) {
    let db = AppDatabase::open(path).expect("open chat database");
    db.save_project(&ProjectRecord::new("project-1", "fixture", "/tmp/fixture"))
        .expect("save project");
    db.save_worktree(&WorktreeRecord::new(
        worktree_id,
        "project-1",
        "main",
        "/tmp/fixture",
    ))
    .expect("save worktree");
    let mut tabs = db.tabs_of_worktree(worktree_id).expect("load chat tabs");
    if !tabs.iter().any(|tab| tab.id == tab_id) {
        tabs.push(TabRecord::new(tab_id, worktree_id, "Chat", "chat"));
    }
    db.save_tabs(worktree_id, &tabs).expect("save chat tab");
}

fn wait_for<F>(session: &ChatSession, mut predicate: F) -> sirio_acp::ChatSnapshot
where
    F: FnMut(&sirio_acp::ChatSnapshot) -> bool,
{
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = session.read().expect("read chat snapshot");
        if predicate(&snapshot) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "chat did not reach the expected state: {snapshot:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn chat_session_streams_tool_permission_stops_and_restores() {
    let dir = TempDir::new();
    let database_path = dir.db_path();
    seed_chat_tab(&database_path, "chat-normal", "worktree-1");
    seed_chat_tab(&database_path, "chat-cancel", "worktree-1");

    let mut session = ChatSession::launch(ChatSessionConfig::new(
        "chat-normal",
        "worktree-1",
        AgentCommand::new("python3").args([FIXTURE, "normal"]),
        std::env::temp_dir(),
        &database_path,
    ))
    .expect("launch chat session");

    session.send("exercise the chat door").expect("send turn");
    let streaming = wait_for(&session, |snapshot| {
        snapshot.status == ChatStatus::Streaming
            && snapshot
                .transcript
                .turns
                .iter()
                .flat_map(|turn| turn.entries.iter())
                .any(|entry| matches!(entry, sirio_persistence::ChatEntry::AssistantMessage { text } if text == "first "))
    });
    assert_eq!(streaming.status, ChatStatus::Streaming);
    let pending = wait_for(&session, |snapshot| {
        snapshot.transcript.turns.iter().any(|turn| {
            turn.entries.iter().any(|entry| {
                matches!(
                    entry,
                    sirio_persistence::ChatEntry::Permission {
                        request_id: 1,
                        outcome: sirio_persistence::ChatPermissionOutcome::Pending,
                        ..
                    }
                )
            })
        })
    });
    assert_eq!(pending.status, ChatStatus::Streaming);
    assert!(pending.transcript.turns[0].entries.iter().any(|entry| {
        matches!(entry, sirio_persistence::ChatEntry::ToolCall { id, .. } if id == "tool-1")
    }));
    session
        .respond_permission(1, "deny")
        .expect("resolve permission");

    let completed = wait_for(&session, |snapshot| {
        snapshot.status == ChatStatus::Completed
    });
    assert!(completed.transcript.turns.iter().any(|turn| {
        turn.entries.iter().any(|entry| {
            matches!(
                entry,
                sirio_persistence::ChatEntry::Permission {
                    outcome: sirio_persistence::ChatPermissionOutcome::Selected { option_id, .. },
                    ..
                } if option_id == "deny"
            )
        })
    }));
    assert!(completed.transcript.turns.iter().any(|turn| {
        turn.entries.iter().any(|entry| {
            matches!(entry, sirio_persistence::ChatEntry::ToolCall { status, .. } if status == "Completed")
        })
    }));
    session.shutdown().expect("shutdown normal chat");

    let restored = ChatSession::restore(&database_path, "chat-normal", "worktree-1")
        .expect("restore chat transcript after relaunch");
    let restored_snapshot = restored.read().expect("read restored transcript");
    assert_eq!(restored_snapshot.status, ChatStatus::Completed);
    assert_eq!(restored_snapshot.transcript, completed.transcript);

    let mut cancellable = ChatSession::launch(ChatSessionConfig::new(
        "chat-cancel",
        "worktree-1",
        AgentCommand::new("python3").args([FIXTURE, "cancel"]),
        std::env::temp_dir(),
        &database_path,
    ))
    .expect("launch cancellable chat");
    cancellable
        .send("stop this turn")
        .expect("send cancellable turn");
    wait_for(&cancellable, |snapshot| {
        snapshot.status == ChatStatus::Streaming
            && snapshot.transcript.turns.iter().any(|turn| {
                turn.entries.iter().any(|entry| {
                    matches!(entry, sirio_persistence::ChatEntry::AssistantMessage { text } if text == "partial")
                })
            })
    });
    cancellable.stop().expect("stop in-flight turn");
    let stopped = wait_for(&cancellable, |snapshot| {
        snapshot.status == ChatStatus::Stopped
    });
    assert_ne!(stopped.status, ChatStatus::Completed);
    cancellable.shutdown().expect("shutdown stopped chat");
}

#[test]
fn chat_session_reports_launch_failures_without_a_fake_transcript() {
    let dir = TempDir::new();
    let database_path = dir.db_path();
    seed_chat_tab(&database_path, "chat-failure", "worktree-1");

    let error = ChatSession::launch(ChatSessionConfig::new(
        "chat-failure",
        "worktree-1",
        AgentCommand::new("definitely-not-an-agent"),
        std::env::temp_dir(),
        &database_path,
    ))
    .expect_err("missing agent must fail launch");
    assert!(
        error.downcast_ref::<AcpError>().is_some(),
        "launch error should retain its ACP type: {error:?}"
    );
    assert!(
        format!("{error:#}").contains("definitely-not-an-agent"),
        "launch error should name the attempted program: {error:#}"
    );
}

#[test]
fn chat_session_sends_queued_text_after_a_normal_turn() {
    let dir = TempDir::new();
    let database_path = dir.db_path();
    seed_chat_tab(&database_path, "chat-queue", "worktree-1");

    let mut session = ChatSession::launch(ChatSessionConfig::new(
        "chat-queue",
        "worktree-1",
        AgentCommand::new("python3").args([FIXTURE, "queue"]),
        std::env::temp_dir(),
        &database_path,
    ))
    .expect("launch queue chat");
    session.send("first prompt").expect("send first turn");
    session.compose("second prompt").expect("queue second turn");

    let completed = wait_for(&session, |snapshot| {
        snapshot.status == ChatStatus::Completed && snapshot.transcript.turns.len() == 2
    });
    assert!(completed.queued_text.is_empty());
    assert!(matches!(
        &completed.transcript.turns[0].entries[0],
        sirio_persistence::ChatEntry::UserMessage { text, .. } if text == "first prompt"
    ));
    assert!(matches!(
        &completed.transcript.turns[1].entries[0],
        sirio_persistence::ChatEntry::UserMessage { text, .. } if text == "second prompt"
    ));
    session.shutdown().expect("shutdown queue chat");
}

#[test]
fn chat_session_folds_plan_approval_and_persists_it() {
    let dir = TempDir::new();
    let database_path = dir.db_path();
    seed_chat_tab(&database_path, "chat-plan", "worktree-1");

    let mut session = ChatSession::launch(ChatSessionConfig::new(
        "chat-plan",
        "worktree-1",
        AgentCommand::new("python3").args([FIXTURE, "plan"]),
        std::env::temp_dir(),
        &database_path,
    ))
    .expect("launch plan chat session");

    session.send("plan this").expect("send the prompt");

    // The plan card is up with its approval pending; answer the approval so
    // the turn can end and the plan advance.
    wait_for(&session, |snapshot| {
        snapshot
            .transcript
            .turns
            .iter()
            .flat_map(|turn| &turn.entries)
            .any(|entry| {
                matches!(
                    entry,
                    sirio_persistence::ChatEntry::Permission {
                        outcome: sirio_persistence::ChatPermissionOutcome::Pending,
                        ..
                    }
                )
            })
    });
    session
        .respond_permission(1, "approve")
        .expect("approve the plan");

    // The plan entry advances after the approval; the permission records
    // the selected option.
    let snapshot = wait_for(&session, |snapshot| {
        snapshot.status == ChatStatus::Completed
            && snapshot.transcript.turns.iter().any(|turn| {
                turn.entries
                    .iter()
                    .any(|entry| matches!(entry, sirio_persistence::ChatEntry::Plan { .. }))
            })
    });
    let plan = snapshot
        .transcript
        .turns
        .iter()
        .flat_map(|turn| turn.entries.iter())
        .find_map(|entry| match entry {
            sirio_persistence::ChatEntry::Plan { entries } => Some(entries),
            _ => None,
        })
        .expect("the plan card was persisted");
    assert_eq!(plan.len(), 2);
    assert_eq!(plan[0].status, "completed");
    assert_eq!(plan[1].status, "in_progress");
    let permission = snapshot
        .transcript
        .turns
        .iter()
        .flat_map(|turn| turn.entries.iter())
        .find_map(|entry| match entry {
            sirio_persistence::ChatEntry::Permission { title, outcome, .. } => {
                Some((title, outcome))
            }
            _ => None,
        })
        .expect("the approval was persisted");
    assert_eq!(permission.0, "Exit plan mode");
    assert!(matches!(
        permission.1,
        sirio_persistence::ChatPermissionOutcome::Selected { option_id, .. }
            if option_id == "approve"
    ));

    session.shutdown().expect("shut down the plan session");

    // The restored transcript still shows the plan card.
    let restored = ChatSession::restore(&database_path, "chat-plan", "worktree-1")
        .expect("restore the plan transcript");
    let snapshot = restored.read().expect("read restored snapshot");
    assert!(
        snapshot.transcript.turns.iter().any(|turn| turn
            .entries
            .iter()
            .any(|entry| matches!(entry, sirio_persistence::ChatEntry::Plan { .. }))),
        "the restored transcript keeps the plan card"
    );
}

#[test]
fn chat_session_answers_a_question_with_text_and_can_cancel_one() {
    let dir = TempDir::new();
    let database_path = dir.db_path();
    seed_chat_tab(&database_path, "chat-question", "worktree-1");

    // Text answer: the typed text rides the selected-option channel and the
    // card records it as the label.
    let mut session = ChatSession::launch(ChatSessionConfig::new(
        "chat-question",
        "worktree-1",
        AgentCommand::new("python3").args([FIXTURE, "question"]),
        std::env::temp_dir(),
        &database_path,
    ))
    .expect("launch question chat session");
    session.send("which color?").expect("send the prompt");
    wait_for(&session, |snapshot| {
        snapshot
            .transcript
            .turns
            .iter()
            .flat_map(|turn| &turn.entries)
            .any(|entry| {
                matches!(
                    entry,
                    sirio_persistence::ChatEntry::Permission {
                        outcome: sirio_persistence::ChatPermissionOutcome::Pending,
                        ..
                    }
                )
            })
    });
    session
        .respond_permission_text(1, "Blue")
        .expect("the typed answer should be accepted");
    let snapshot = wait_for(&session, |snapshot| {
        snapshot.status == ChatStatus::Completed
    });
    let answered = snapshot
        .transcript
        .turns
        .iter()
        .flat_map(|turn| turn.entries.iter())
        .find_map(|entry| match entry {
            sirio_persistence::ChatEntry::Permission { outcome, .. } => Some(outcome),
            _ => None,
        })
        .expect("the answered card was persisted");
    assert!(matches!(
        answered,
        sirio_persistence::ChatPermissionOutcome::Selected { option_id, label }
            if option_id == "Blue" && label == "Blue"
    ));
    session.shutdown().expect("shut down the question session");

    // Cancel: withdrawing the permission records Cancelled and the turn
    // still completes.
    seed_chat_tab(&database_path, "chat-cancel", "worktree-1");
    let mut session = ChatSession::launch(ChatSessionConfig::new(
        "chat-cancel",
        "worktree-1",
        AgentCommand::new("python3").args([FIXTURE, "cancel_permission_direct"]),
        std::env::temp_dir(),
        &database_path,
    ))
    .expect("launch cancel chat session");
    session
        .send("wait for the withdrawal")
        .expect("send the prompt");
    wait_for(&session, |snapshot| {
        snapshot
            .transcript
            .turns
            .iter()
            .flat_map(|turn| &turn.entries)
            .any(|entry| {
                matches!(
                    entry,
                    sirio_persistence::ChatEntry::Permission {
                        outcome: sirio_persistence::ChatPermissionOutcome::Pending,
                        ..
                    }
                )
            })
    });
    session
        .cancel_permission(1)
        .expect("withdraw the permission");
    let snapshot = wait_for(&session, |snapshot| {
        snapshot.status == ChatStatus::Completed
    });
    let cancelled = snapshot
        .transcript
        .turns
        .iter()
        .flat_map(|turn| turn.entries.iter())
        .find_map(|entry| match entry {
            sirio_persistence::ChatEntry::Permission { outcome, .. } => Some(outcome),
            _ => None,
        })
        .expect("the cancelled card was persisted");
    assert!(matches!(
        cancelled,
        sirio_persistence::ChatPermissionOutcome::Cancelled
    ));
    session.shutdown().expect("shut down the cancel session");
}

#[test]
fn chat_session_expires_a_permission_left_open_by_a_dead_transport() {
    let dir = TempDir::new();
    let database_path = dir.db_path();
    seed_chat_tab(&database_path, "chat-death", "worktree-1");

    let mut session = ChatSession::launch(ChatSessionConfig::new(
        "chat-death",
        "worktree-1",
        AgentCommand::new("python3").args([FIXTURE, "question_death"]),
        std::env::temp_dir(),
        &database_path,
    ))
    .expect("launch death chat session");
    session.send("which color?").expect("send the prompt");

    // The fixture dies while the question is open. Wait for the terminal
    // transport state, then inspect the same settled snapshot: this avoids
    // depending on whether the short-lived Pending callback is observable
    // before run_connection processes EOF under workspace load.
    let snapshot = wait_for(&session, |snapshot| snapshot.status == ChatStatus::Error);
    assert!(snapshot.transcript.turns.iter().any(|turn| {
        turn.entries.iter().any(|entry| {
            matches!(
                entry,
                sirio_persistence::ChatEntry::Permission {
                    outcome: sirio_persistence::ChatPermissionOutcome::Expired,
                    ..
                }
            )
        })
    }));
    assert!(snapshot.transcript.turns.iter().any(|turn| {
        turn.entries
            .iter()
            .any(|entry| matches!(entry, sirio_persistence::ChatEntry::Error { .. }))
    }));
    assert_eq!(snapshot.status, ChatStatus::Error);
    session.shutdown().expect("shut down the dead session");
}
