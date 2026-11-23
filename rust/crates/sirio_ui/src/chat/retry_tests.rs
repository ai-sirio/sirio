//! Retry on an error row (spec §5.3), against the fake `claude` and the fake ACP agent.
//!
//! The ways it can fail, written before the code:
//! - a failed turn's Retry relaunches (the agent loses its conversation) instead of resending;
//! - the resend differs from the first send: the mention, the image or the text is lost;
//! - the queue drains after a failure, so its next item replaces the prompt Retry resends;
//! - a double click, or a click on a row that is gone, sends the prompt twice;
//! - a stray `TurnEnded` (a turn the CLI started) releases the hold or forgets the prompt;
//! - an agent that died idle is made to resend a prompt that was answered long ago;
//! - a resume that the CLI refused still sends the prompt, into a conversation without context;
//! - the composer's own draft is overwritten by the refill, or the refill drops the mentions;
//! - a prompt sent after a failure leaves the old row's Retry resending the wrong text.
use super::tests::{
    CHAT_FIXTURE, TempDir, pump_chat_until, pump_chat_until_within, refresh_frame,
};
use super::*;
use gpui::{Modifiers, TestAppContext, VisualTestContext};
use std::time::Duration;

const CLAUDE_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sirio_chat/tests/fixtures/claude_fixture.py"
);

const HELD_LINE: &str = "Waiting: press Retry or send a message.";

/// Where the fake `claude` writes what it heard: every prompt, and the `--resume` of every launch.
struct Logs {
    users: PathBuf,
    launches: PathBuf,
}

impl Logs {
    fn in_dir(dir: &Path) -> Self {
        Self {
            users: dir.join("users.jsonl"),
            launches: dir.join("launches.jsonl"),
        }
    }

    /// The `content` of each prompt the CLI heard, in order.
    fn heard(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(&self.users)
            .unwrap_or_default()
            .lines()
            .map(|line| {
                let message: serde_json::Value =
                    serde_json::from_str(line).expect("a user line is JSON");
                message["message"]["content"].clone()
            })
            .collect()
    }

    /// The `--resume` of every process launched, `None` for a fresh session.
    fn resumes(&self) -> Vec<Option<String>> {
        std::fs::read_to_string(&self.launches)
            .unwrap_or_default()
            .lines()
            .map(|line| {
                let launch: serde_json::Value =
                    serde_json::from_str(line).expect("a launch line is JSON");
                launch["resume"].as_str().map(str::to_string)
            })
            .collect()
    }
}

fn claude_launch(mode: &str, logs: &Logs) -> LaunchSpec {
    LaunchSpec::Claude(sirio_chat::ClaudeLaunch::fixture(
        "python3",
        vec![
            CLAUDE_FIXTURE.to_string(),
            mode.to_string(),
            format!("--fixture-user-log={}", logs.users.display()),
            format!("--fixture-launch-log={}", logs.launches.display()),
        ],
    ))
}

fn acp_launch() -> LaunchSpec {
    LaunchSpec::Acp(AgentCommand::new("python3").arg(CHAT_FIXTURE).arg("plain"))
}

fn open_window<'a>(
    cx: &'a mut TestAppContext,
    build: impl FnOnce(&mut Context<Chat>) -> Chat + 'static,
) -> (Entity<Chat>, &'a mut VisualTestContext) {
    cx.update(Theme::init);
    cx.update(crate::ely::init);
    cx.update(init);
    cx.executor().allow_parking();
    cx.add_window_view(|_, cx| build(cx))
}

/// A chat on the fake `claude`, connected, whose CLI hears every prompt and answers none
/// (`session_state`, a prompt that starts with `silent`): the test applies the CLI's events
/// itself. The CLI names its session (`fixture-session-1`) when it starts.
fn silent_chat<'a>(
    cx: &'a mut TestAppContext,
    cwd: &Path,
    logs: &Logs,
) -> (Entity<Chat>, &'a mut VisualTestContext) {
    let (launch, cwd) = (claude_launch("session_state", logs), cwd.to_path_buf());
    let (chat, cx) = open_window(cx, move |cx| Chat::from_test_command(launch, cwd, cx));
    pump_chat_until_within(cx, &chat, 3000, |chat| {
        chat.client.is_some() && chat.agent_session_id().is_some()
    });
    (chat, cx)
}

/// A chat that is not connected (its client is gone, as after a loss) on `launch`.
fn offline_chat<'a>(
    cx: &'a mut TestAppContext,
    launch: LaunchSpec,
) -> (Entity<Chat>, &'a mut VisualTestContext) {
    open_window(cx, move |cx| Chat::new(Some(launch), std::env::temp_dir(), cx))
}

fn event(chat: &Entity<Chat>, cx: &mut VisualTestContext, event: sirio_chat::ChatEvent) {
    chat.update(cx, |chat, cx| chat.handle_event(event, cx));
}

fn turn_failed() -> sirio_chat::ChatEvent {
    sirio_chat::ChatEvent::TurnFailed {
        message: "API Error: 529 overloaded".into(),
        stop_reason: "EndTurn".into(),
    }
}

fn turn_ended() -> sirio_chat::ChatEvent {
    sirio_chat::ChatEvent::TurnEnded {
        stop_reason: "EndTurn".into(),
    }
}

fn connection_lost() -> sirio_chat::ChatEvent {
    sirio_chat::ChatEvent::TransportError("ACP transport closed unexpectedly".into())
}

/// Types `text` in the composer and presses Send: a turn when idle, a queued item mid-turn.
fn send(chat: &Entity<Chat>, cx: &mut VisualTestContext, text: &str) {
    chat.update(cx, |chat, cx| {
        chat.set_composer_text(text.to_string(), cx);
        chat.send(cx);
    });
}

/// Gives what is in flight real time to arrive, for a test that says nothing arrived. The
/// virtual clock moves a little: a streaming chat redraws on every tick of it.
fn settle(cx: &mut VisualTestContext, rounds: usize) {
    for _ in 0..rounds {
        cx.cx.executor().advance_clock(Duration::from_millis(10));
        std::thread::sleep(Duration::from_millis(10));
        cx.cx.run_until_parked();
    }
}

fn wait_for_prompts(chat: &Entity<Chat>, cx: &mut VisualTestContext, logs: &Logs, count: usize) {
    pump_chat_until_within(cx, chat, 3000, |_| logs.heard().len() >= count);
}

fn error_row(chat: &Entity<Chat>, cx: &VisualTestContext) -> usize {
    chat.read_with(&cx.cx, |chat, _| {
        chat.entries
            .iter()
            .rposition(|entry| matches!(entry, Entry::Error { .. }))
            .expect("an error row is in the transcript")
    })
}

/// The `errorKind` and `retry` the socket reports for the newest error row.
fn readout(chat: &Entity<Chat>, cx: &VisualTestContext) -> (String, String) {
    let snapshot = chat.read_with(&cx.cx, |chat, _| chat.control_snapshot());
    let row = snapshot
        .transcript
        .iter()
        .rev()
        .find(|row| row.get("kind").is_some_and(|kind| kind == "error"))
        .expect("an error row is in the readout");
    (row["errorKind"].clone(), row["retry"].clone())
}

fn user_texts(chat: &Entity<Chat>, cx: &VisualTestContext) -> Vec<String> {
    chat.read_with(&cx.cx, |chat, _| {
        chat.entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::User { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    })
}

fn queue_of(chat: &Entity<Chat>, cx: &VisualTestContext) -> Vec<String> {
    chat.read_with(&cx.cx, |chat, _| chat.queue.iter().cloned().collect())
}

fn notices(chat: &Entity<Chat>, cx: &VisualTestContext) -> Vec<String> {
    chat.read_with(&cx.cx, |chat, _| {
        chat.entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Notice {
                    text,
                    kind: NoticeKind::Info,
                } => Some(text.clone()),
                _ => None,
            })
            .collect()
    })
}

fn retry_row(chat: &Entity<Chat>, cx: &mut VisualTestContext, index: usize) {
    chat.update(cx, |chat, cx| chat.retry(index, cx));
}

fn png() -> ImageAttachment {
    ImageAttachment {
        mime_type: "image/png".into(),
        base64_data: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAX+XDSwAAAABJRU5ErkJggg=="
            .into(),
    }
}

/// A failed turn's Retry sends the very same prompt again, on the same process: the text, the
/// mention and the image the first send carried, as the CLI hears them.
#[gpui::test]
async fn retry_on_a_failed_turn_resends_the_same_prompt(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    let generation = chat.read_with(&cx.cx, |chat, _| chat.connection_generation);

    chat.update(cx, |chat, cx| {
        chat.set_composer_text("silent read @notes.md and say hi".to_string(), cx);
        chat.accepted_mentions.push("notes.md".to_string());
        chat.attachments.push(png());
        chat.send(cx);
    });
    wait_for_prompts(&chat, cx, &logs, 1);
    event(&chat, cx, turn_failed());
    assert_eq!(
        readout(&chat, cx),
        ("turnFailed".to_string(), "resend".to_string())
    );
    assert!(!chat.read_with(&cx.cx, |chat, _| chat.streaming));

    let row = error_row(&chat, cx);
    retry_row(&chat, cx, row);
    wait_for_prompts(&chat, cx, &logs, 2);

    let heard = logs.heard();
    assert_eq!(heard[0], heard[1], "the same blocks both times");
    let blocks = heard[0].as_array().expect("content is a list of blocks");
    assert_eq!(blocks.len(), 2, "a text block and an image block: {blocks:?}");
    assert!(
        blocks[0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("[@notes.md](file://")),
        "the mention rides in the text block: {blocks:?}"
    );
    assert_eq!(blocks[1]["type"], "image");
    chat.read_with(&cx.cx, |chat, _| {
        assert!(
            !chat
                .entries
                .iter()
                .any(|entry| matches!(entry, Entry::Error { .. })),
            "Retry removes the error row"
        );
        assert_eq!(
            chat.connection_generation, generation,
            "the same process: nothing was relaunched"
        );
        assert!(chat.streaming, "the resent turn runs");
    });
    let users = user_texts(&chat, cx);
    assert_eq!(users.len(), 2, "a second user row: {users:?}");
    assert_eq!(users[0], users[1]);
}

/// What the user sees and presses: the line under the held queue, and the row's own Retry
/// button, which resends the prompt and takes the line away.
#[gpui::test]
async fn the_drawn_retry_button_resends_and_the_drawn_line_says_the_queue_waits(
    cx: &mut TestAppContext,
) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent first");
    wait_for_prompts(&chat, cx, &logs, 1);
    send(&chat, cx, "silent second");
    refresh_frame(cx);
    assert!(
        cx.debug_bounds("queue-held-line").is_none(),
        "no failure yet: the queue is not held"
    );

    event(&chat, cx, turn_failed());
    refresh_frame(cx);
    assert!(
        cx.debug_bounds("queue-held-line").is_some(),
        "the line under the held queue is drawn"
    );
    let retry = cx
        .debug_bounds("chat-retry")
        .expect("a failed turn's row offers Retry");
    cx.simulate_click(retry.center(), Modifiers::none());
    cx.run_until_parked();

    wait_for_prompts(&chat, cx, &logs, 2);
    let heard = logs.heard();
    assert_eq!(heard[0], heard[1], "the click sent the same prompt again");
    refresh_frame(cx);
    assert!(
        cx.debug_bounds("queue-held-line").is_none(),
        "the line goes once a prompt is on its way"
    );
}

/// The two clicks of a double click: the second one reaches a row that is gone.
#[gpui::test]
async fn a_second_retry_on_a_removed_row_does_nothing(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent once");
    wait_for_prompts(&chat, cx, &logs, 1);
    event(&chat, cx, turn_failed());

    let row = error_row(&chat, cx);
    retry_row(&chat, cx, row);
    wait_for_prompts(&chat, cx, &logs, 2);
    let rows = chat.read_with(&cx.cx, |chat, _| chat.entries.len());
    let generation = chat.read_with(&cx.cx, |chat, _| chat.connection_generation);

    retry_row(&chat, cx, row);
    settle(cx, 30);

    assert_eq!(logs.heard().len(), 2, "no third prompt");
    chat.read_with(&cx.cx, |chat, _| {
        assert_eq!(chat.entries.len(), rows, "the transcript did not change");
        assert_eq!(chat.connection_generation, generation, "no relaunch");
        assert!(!chat.connecting);
    });
}

/// Without this the next queued item would replace the prompt Retry resends.
#[gpui::test]
async fn a_failed_turn_holds_the_queue_and_shows_why(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent first");
    wait_for_prompts(&chat, cx, &logs, 1);
    send(&chat, cx, "silent second");
    send(&chat, cx, "silent third");
    assert_eq!(queue_of(&chat, cx), ["silent second", "silent third"]);
    let before = chat.read_with(&cx.cx, |chat, _| chat.control_snapshot());
    assert!(!before.queue_held);
    assert_eq!(before.queue_held_line, None);

    event(&chat, cx, turn_failed());
    settle(cx, 30);

    let held = chat.read_with(&cx.cx, |chat, _| chat.control_snapshot());
    assert!(held.queue_held);
    assert_eq!(held.queue_held_line.as_deref(), Some(HELD_LINE));
    assert_eq!(held.queued, ["silent second", "silent third"], "nothing left the queue");
    assert_eq!(logs.heard().len(), 1, "and nothing was sent");
    assert_eq!(user_texts(&chat, cx), ["silent first"]);

    // The held items stay removable, and Clear all still empties the queue.
    chat.update(cx, |chat, cx| chat.remove_queued_entry(0, cx));
    assert_eq!(queue_of(&chat, cx), ["silent third"]);
    chat.update(cx, |chat, cx| {
        chat.queue.push_back("silent fourth".into());
        cx.notify();
    });

    let row = error_row(&chat, cx);
    retry_row(&chat, cx, row);
    wait_for_prompts(&chat, cx, &logs, 2);
    assert_eq!(
        queue_of(&chat, cx),
        ["silent third", "silent fourth"],
        "the resend runs alone: the queue waits for its end"
    );

    event(&chat, cx, turn_ended());
    wait_for_prompts(&chat, cx, &logs, 3);
    let released = chat.read_with(&cx.cx, |chat, _| chat.control_snapshot());
    assert!(!released.queue_held);
    assert_eq!(released.queue_held_line, None);
    assert_eq!(released.queued, ["silent fourth"], "the next item went out, one only");
    assert_eq!(
        user_texts(&chat, cx),
        ["silent first", "silent first", "silent third"]
    );
}

/// Clear all empties a held queue, and the line goes with it.
#[gpui::test]
async fn clearing_a_held_queue_takes_the_line_away(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent first");
    send(&chat, cx, "silent second");
    event(&chat, cx, turn_failed());
    assert_eq!(
        chat.read_with(&cx.cx, |chat, _| chat.control_snapshot().queue_held_line),
        Some(HELD_LINE.to_string())
    );

    chat.update(cx, |chat, cx| chat.clear_queue(cx));

    let snapshot = chat.read_with(&cx.cx, |chat, _| chat.control_snapshot());
    assert!(snapshot.queued.is_empty());
    assert_eq!(snapshot.queue_held_line, None, "an empty queue shows no line");
}

/// A turn the CLI started ends with a `TurnEnded` of its own: it is not the end of the failed
/// turn, so it neither releases the queue nor takes the prompt away from Retry.
#[gpui::test]
async fn a_stray_turn_end_after_a_failure_changes_nothing(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent first");
    wait_for_prompts(&chat, cx, &logs, 1);
    send(&chat, cx, "silent second");
    event(&chat, cx, turn_failed());

    event(&chat, cx, turn_ended());
    settle(cx, 30);

    let snapshot = chat.read_with(&cx.cx, |chat, _| chat.control_snapshot());
    assert!(snapshot.queue_held, "still held");
    assert_eq!(snapshot.queued, ["silent second"]);
    assert_eq!(logs.heard().len(), 1, "the queue was not drained");
    assert_eq!(readout(&chat, cx).1, "resend", "Retry still has its prompt");

    let row = error_row(&chat, cx);
    retry_row(&chat, cx, row);
    wait_for_prompts(&chat, cx, &logs, 2);
    let heard = logs.heard();
    assert_eq!(heard[0], heard[1]);
}

/// An agent that dies between turns has nothing to resend: its Restart agent only relaunches,
/// though the launch could resume and the session is known.
#[gpui::test]
async fn an_idle_death_has_nothing_to_resend(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent hello");
    wait_for_prompts(&chat, cx, &logs, 1);
    event(&chat, cx, turn_ended());

    event(&chat, cx, connection_lost());

    assert_eq!(
        readout(&chat, cx),
        ("disconnected".to_string(), "relaunch".to_string())
    );
    assert!(
        chat.read_with(&cx.cx, |chat, _| chat.agent_session_id().is_some()),
        "the session is known: it is the missing prompt that rules out a resend"
    );
}

/// Stop is the user's own end of the turn: a later death does not send it again.
#[gpui::test]
async fn a_stopped_turn_leaves_nothing_to_resend(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent stop me");
    wait_for_prompts(&chat, cx, &logs, 1);
    chat.update(cx, |chat, cx| chat.cancel_turn(cx));

    event(&chat, cx, connection_lost());

    assert_eq!(readout(&chat, cx).1, "relaunch");
}

/// A prompt sent after a failure makes the old row stale: its Retry would resend the new one.
#[gpui::test]
async fn a_prompt_sent_after_a_failure_retires_the_old_rows_retry(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent first");
    wait_for_prompts(&chat, cx, &logs, 1);
    event(&chat, cx, turn_failed());
    let stale = error_row(&chat, cx);

    send(&chat, cx, "silent second");
    wait_for_prompts(&chat, cx, &logs, 2);

    assert_eq!(readout(&chat, cx).1, "none", "the old row has no Retry now");
    retry_row(&chat, cx, stale);
    settle(cx, 30);
    assert_eq!(logs.heard().len(), 2, "the second prompt was not sent again");
    assert_eq!(user_texts(&chat, cx), ["silent first", "silent second"]);
}

/// A loss in mid-turn, the session known and the launch able to resume: Retry relaunches with
/// `--resume` of it and sends the prompt again. The resume is Retry's own: a later plain
/// relaunch starts fresh, as it does for a chat that was not restored.
#[gpui::test]
async fn a_lost_turn_resumes_the_session_and_sends_its_prompt_again(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent mid-turn");
    wait_for_prompts(&chat, cx, &logs, 1);
    event(
        &chat,
        cx,
        sirio_chat::ChatEvent::TransportError("the pipe broke".into()),
    );
    assert_eq!(
        readout(&chat, cx),
        ("connection".to_string(), "resume".to_string())
    );

    let row = error_row(&chat, cx);
    retry_row(&chat, cx, row);
    wait_for_prompts(&chat, cx, &logs, 2);

    assert_eq!(
        logs.resumes(),
        [None, Some("fixture-session-1".to_string())],
        "the second process resumed the first one's session"
    );
    let heard = logs.heard();
    assert_eq!(heard[0], heard[1], "the same prompt");
    assert_eq!(user_texts(&chat, cx), ["silent mid-turn", "silent mid-turn"]);
    assert!(
        notices(&chat, cx).is_empty(),
        "a resumed conversation says nothing: {:?}",
        notices(&chat, cx)
    );

    pump_chat_until(cx, &chat, |chat| chat.client.is_some() && !chat.connecting);
    chat.update(cx, |chat, cx| chat.start_connection(cx));
    pump_chat_until_within(cx, &chat, 3000, |_| logs.resumes().len() >= 3);
    assert_eq!(
        logs.resumes()[2],
        None,
        "a plain relaunch of a chat that was not restored does not resume"
    );
}

/// The CLI refused the session id: the resend would go to a conversation without context, so
/// the prompt goes back to the composer instead.
#[gpui::test]
async fn a_refused_resume_puts_the_prompt_back_in_the_composer(cx: &mut TestAppContext) {
    refused_resume("refuse_resume", cx);
}

/// The same, from a CLI that prints its `init` before it answers the handshake: the fresh
/// session has named itself by the time the chat is connected, so only its id tells.
#[gpui::test]
async fn a_refusal_named_at_startup_still_puts_the_prompt_back(cx: &mut TestAppContext) {
    refused_resume("refuse_resume_early_init", cx);
}

fn refused_resume(mode: &str, cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let launch = claude_launch(mode, &logs);
    let (chat, cx) = offline_chat(cx, launch);
    chat.update(cx, |chat, cx| {
        chat.agent_session_id = Some("session-that-is-gone".to_string());
        chat.submit_turn("write the report".into(), Vec::new(), Vec::new(), cx);
    });
    event(&chat, cx, connection_lost());
    assert_eq!(readout(&chat, cx).1, "resume");

    let row = error_row(&chat, cx);
    retry_row(&chat, cx, row);
    pump_chat_until_within(cx, &chat, 3000, |chat| {
        chat.client.is_some() && !chat.connecting
    });

    assert_eq!(
        logs.resumes().first(),
        Some(&Some("session-that-is-gone".to_string())),
        "the resume was tried"
    );
    assert!(logs.heard().is_empty(), "nothing was sent: {:?}", logs.heard());
    assert_eq!(chat.read_with(&cx.cx, |chat, _| chat.draft_text()), "write the report");
    assert_eq!(
        notices(&chat, cx),
        [retry::RESTARTED_MESSAGE_BACK],
        "the first notice"
    );
    assert_eq!(user_texts(&chat, cx), ["write the report"], "one user row: the first send");
}

/// An ACP agent cannot resume. Retry relaunches and gives the prompt back: the raw draft with
/// its mention, and the image.
#[gpui::test]
async fn a_lost_acp_turn_gives_its_prompt_back_in_the_composer(cx: &mut TestAppContext) {
    let (chat, cx) = offline_chat(cx, acp_launch());
    chat.update(cx, |chat, cx| {
        chat.submit_prompt(
            retry::LastPrompt {
                text: "look at please".into(),
                mention_paths: vec!["src/a.rs".into()],
                images: vec![png()],
                draft: "look at @src/a.rs please".into(),
                mentions: vec!["src/a.rs".into()],
            },
            cx,
        );
    });
    event(&chat, cx, connection_lost());
    assert_eq!(readout(&chat, cx).1, "composer");

    let row = error_row(&chat, cx);
    retry_row(&chat, cx, row);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some() && !chat.connecting);

    chat.read_with(&cx.cx, |chat, _| {
        assert_eq!(chat.draft_text(), "look at @src/a.rs please");
        assert_eq!(chat.accepted_mentions, ["src/a.rs"]);
        assert_eq!(chat.attachments, [png()]);
        assert!(!chat.streaming, "nothing was sent");
        assert!(
            !chat
                .entries
                .iter()
                .any(|entry| matches!(entry, Entry::Error { .. })),
            "the recovered error row is gone"
        );
    });
    assert_eq!(notices(&chat, cx), [retry::RESTARTED_MESSAGE_BACK]);
    assert_eq!(user_texts(&chat, cx), ["look at please"], "no second user row");
    assert_eq!(
        readout_retry_after_refill(&chat, cx),
        "relaunch",
        "the refill used the prompt up: a death now has nothing to resend"
    );
}

/// What the agent's next death would offer: the prompt was handed back, not kept for a resend.
fn readout_retry_after_refill(chat: &Entity<Chat>, cx: &mut VisualTestContext) -> String {
    event(chat, cx, connection_lost());
    readout(chat, cx).1
}

/// Whatever the user typed while the agent was down stays where it is.
#[gpui::test]
async fn a_composer_with_a_draft_is_not_overwritten(cx: &mut TestAppContext) {
    let (chat, cx) = offline_chat(cx, acp_launch());
    chat.update(cx, |chat, cx| {
        chat.submit_turn("the lost prompt".into(), Vec::new(), Vec::new(), cx);
        chat.set_composer_text("something else I was typing".to_string(), cx);
    });
    event(&chat, cx, connection_lost());

    let row = error_row(&chat, cx);
    retry_row(&chat, cx, row);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some() && !chat.connecting);

    chat.read_with(&cx.cx, |chat, _| {
        assert_eq!(chat.draft_text(), "something else I was typing");
        assert!(chat.accepted_mentions.is_empty());
        assert!(chat.attachments.is_empty());
    });
    assert_eq!(
        notices(&chat, cx),
        [retry::RESTARTED],
        "the second notice only"
    );
}

/// The agent died after a failed turn, and Restart agent handed the prompt back: the failed turn's
/// row must not stay a Retry that restarts an agent that is up.
#[gpui::test]
async fn handing_the_prompt_back_retires_the_failed_turns_retry(cx: &mut TestAppContext) {
    let (chat, cx) = offline_chat(cx, acp_launch());
    chat.update(cx, |chat, cx| {
        chat.submit_turn("the failed prompt".into(), Vec::new(), Vec::new(), cx);
    });
    event(&chat, cx, turn_failed());
    event(&chat, cx, connection_lost());
    // The newest error row is the lost connection's; the failed turn's comes before it.
    let lost = error_row(&chat, cx);
    retry_row(&chat, cx, lost);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some() && !chat.connecting);

    assert_eq!(notices(&chat, cx), [retry::RESTARTED_MESSAGE_BACK]);
    assert_eq!(
        readout(&chat, cx),
        ("turnFailed".to_string(), "none".to_string()),
        "the failed turn's row is what is left, and it has no Retry"
    );
}

/// A new conversation drops the old one's prompt and hold: a resend must never reach it.
#[gpui::test]
async fn a_new_conversation_forgets_the_failed_prompt(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent first");
    wait_for_prompts(&chat, cx, &logs, 1);
    send(&chat, cx, "silent second");
    event(&chat, cx, turn_failed());

    chat.update(cx, |chat, cx| chat.new_conversation(cx));
    pump_chat_until_within(cx, &chat, 3000, |chat| {
        chat.client.is_some() && !chat.connecting
    });

    let snapshot = chat.read_with(&cx.cx, |chat, _| chat.control_snapshot());
    assert!(!snapshot.queue_held, "the hold belonged to the old conversation");
    event(&chat, cx, connection_lost());
    assert_eq!(readout(&chat, cx).1, "relaunch", "no prompt to resend");
}
