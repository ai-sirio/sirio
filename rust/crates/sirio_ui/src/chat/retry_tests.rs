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
const HELD_LINE_NO_RETRY: &str = "Waiting: send a message.";

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
    let resent = chat.read_with(&cx.cx, |chat, _| chat.control_snapshot());
    assert!(!resent.queue_held, "the hold lifts at the send");
    assert_eq!(resent.queue_held_line, None, "and the line goes with it");

    event(&chat, cx, turn_ended());
    wait_for_prompts(&chat, cx, &logs, 3);
    assert_eq!(
        queue_of(&chat, cx),
        ["silent fourth"],
        "the resend's end sent the next item, one only"
    );
    assert_eq!(
        user_texts(&chat, cx),
        ["silent first", "silent first", "silent third"]
    );
    assert_eq!(
        logs.heard()[2][0]["text"], "silent third",
        "and it is the CLI that heard it"
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
    assert!(!snapshot.queue_held, "an emptied queue is not held");
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

/// The user's own send is how a held queue moves again: "Send now" on a held, idle queue sends that
/// item (whatever its place) and lifts the hold, and what is left drains at that turn's end.
#[gpui::test]
async fn send_now_on_a_held_queue_sends_it_and_lifts_the_hold(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent first");
    wait_for_prompts(&chat, cx, &logs, 1);
    send(&chat, cx, "silent second");
    send(&chat, cx, "silent third");
    event(&chat, cx, turn_failed());
    assert!(chat.read_with(&cx.cx, |chat, _| chat.queue_held));

    chat.update(cx, |chat, cx| chat.send_queued_entry_now(1, cx));
    wait_for_prompts(&chat, cx, &logs, 2);

    assert_eq!(logs.heard()[1][0]["text"], "silent third", "the entry that was pressed");
    assert_eq!(queue_of(&chat, cx), ["silent second"], "the rest waits");
    assert!(!chat.read_with(&cx.cx, |chat, _| chat.queue_held), "the send lifted the hold");

    event(&chat, cx, turn_ended());
    wait_for_prompts(&chat, cx, &logs, 3);
    assert_eq!(logs.heard()[2][0]["text"], "silent second", "it drained at that turn's end");
    assert!(queue_of(&chat, cx).is_empty());
}

/// A message typed after a failure lifts the hold as Retry does: it goes out alone, and the
/// queue drains at its own end.
#[gpui::test]
async fn a_manual_send_after_a_failure_drains_the_queue_at_its_own_end(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent first");
    wait_for_prompts(&chat, cx, &logs, 1);
    send(&chat, cx, "silent second");
    event(&chat, cx, turn_failed());

    send(&chat, cx, "silent manual");
    wait_for_prompts(&chat, cx, &logs, 2);
    settle(cx, 30);

    assert_eq!(logs.heard().len(), 2, "the queue did not move with the send");
    assert_eq!(queue_of(&chat, cx), ["silent second"]);
    assert!(!chat.read_with(&cx.cx, |chat, _| chat.queue_held));

    event(&chat, cx, turn_ended());
    wait_for_prompts(&chat, cx, &logs, 3);
    assert_eq!(logs.heard()[2][0]["text"], "silent second");
    assert_eq!(
        user_texts(&chat, cx),
        ["silent first", "silent manual", "silent second"]
    );
    assert!(queue_of(&chat, cx).is_empty());
}

/// ACP can answer the prompt that Stop cancelled with an error: that is the late end of the stopped
/// turn. It leaves no error row and no hold, and the agent is not touched; like the late `TurnEnded`
/// of a Stop it lets the queue go on.
#[gpui::test]
async fn a_failure_after_stop_is_the_late_end_of_the_stopped_turn(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    let generation = chat.read_with(&cx.cx, |chat, _| chat.connection_generation);
    send(&chat, cx, "silent stop me");
    wait_for_prompts(&chat, cx, &logs, 1);
    send(&chat, cx, "silent after");
    chat.update(cx, |chat, cx| chat.cancel_turn(cx));

    event(&chat, cx, turn_failed());

    chat.read_with(&cx.cx, |chat, _| {
        assert!(
            !chat
                .entries
                .iter()
                .any(|entry| matches!(entry, Entry::Error { .. })),
            "no error row: {:?}",
            chat.entries
        );
        assert!(!chat.queue_held, "no hold");
        assert!(chat.client.is_some(), "the agent is up");
        assert_eq!(chat.connection_generation, generation, "and not relaunched");
    });
    wait_for_prompts(&chat, cx, &logs, 2);
    assert_eq!(logs.heard()[1][0]["text"], "silent after", "the queue went on");
}

/// A turn's end that is not the prompt's own can be taken as its end (a turn the CLI started while
/// the prompt ran, or the late end of a stopped turn after a quick resend). The prompt's real
/// failure then arrives with no turn streaming, and it is still a failure: an error row with its
/// Retry, and the queue held. Dropping it would leave the footer of an ordinary end.
#[gpui::test]
async fn a_failure_after_a_stray_end_is_still_shown(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent first");
    wait_for_prompts(&chat, cx, &logs, 1);
    event(&chat, cx, turn_ended());
    assert!(!chat.read_with(&cx.cx, |chat, _| chat.streaming), "taken as the prompt's end");

    event(&chat, cx, turn_failed());

    chat.read_with(&cx.cx, |chat, _| {
        let row = chat
            .entries
            .iter()
            .find_map(|entry| match entry {
                Entry::Error {
                    message,
                    retryable,
                    kind,
                } => Some((message.as_str(), *retryable, *kind)),
                _ => None,
            })
            .expect("the failure is in the transcript");
        assert_eq!(row, ("API Error: 529 overloaded", true, ErrorKind::TurnFailed));
        assert!(chat.queue_held, "the queue waits on the user");
        assert!(chat.client.is_some(), "the agent is up and not relaunched");
    });
}

/// A Stop's pending end is settled by the turn end that arrives, a `TurnEnded` as much as a
/// `TurnFailed`. A failure that comes after that end is not the stopped turn's answer: it is shown.
#[gpui::test]
async fn a_stops_end_settles_the_pending_flag_for_a_later_failure(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent stop me");
    wait_for_prompts(&chat, cx, &logs, 1);
    chat.update(cx, |chat, cx| chat.cancel_turn(cx));
    event(&chat, cx, turn_ended());
    chat.read_with(&cx.cx, |chat, _| {
        assert!(
            !chat
                .entries
                .iter()
                .any(|entry| matches!(entry, Entry::Error { .. })),
            "the stopped turn's own end leaves no row"
        );
    });

    event(&chat, cx, turn_failed());

    assert_eq!(
        readout(&chat, cx).0,
        "turnFailed",
        "a failure after the Stop's end is a failed turn"
    );
    chat.read_with(&cx.cx, |chat, _| assert!(chat.queue_held, "and holds the queue"));
}

/// A held queue whose Retry is gone says so: the prompt went back to the composer, and the failed
/// turn's row was retired. The queue stays held.
#[gpui::test]
async fn a_held_queue_with_no_retry_left_says_to_send_a_message(cx: &mut TestAppContext) {
    let (chat, cx) = offline_chat(cx, acp_launch());
    chat.update(cx, |chat, cx| {
        chat.submit_turn("the failed prompt".into(), Vec::new(), Vec::new(), cx);
        chat.queue.push_back("queued behind it".into());
    });
    event(&chat, cx, turn_failed());
    event(&chat, cx, connection_lost());
    let line = |chat: &Entity<Chat>, cx: &VisualTestContext| {
        chat.read_with(&cx.cx, |chat, _| chat.control_snapshot().queue_held_line)
    };
    assert_eq!(line(&chat, cx).as_deref(), Some(HELD_LINE), "a Retry is left");

    let lost = error_row(&chat, cx);
    retry_row(&chat, cx, lost);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some() && !chat.connecting);

    assert_eq!(notices(&chat, cx), [retry::RESTARTED_MESSAGE_BACK]);
    assert_eq!(line(&chat, cx).as_deref(), Some(HELD_LINE_NO_RETRY));
    assert!(chat.read_with(&cx.cx, |chat, _| chat.queue_held), "still held");
    assert_eq!(queue_of(&chat, cx), ["queued behind it"]);
}

/// OK on the failed turn's row takes its Retry away too; the held queue then waits on a message.
#[gpui::test]
async fn an_ok_on_the_failed_row_leaves_the_queue_waiting_on_a_message(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent first");
    wait_for_prompts(&chat, cx, &logs, 1);
    send(&chat, cx, "silent second");
    event(&chat, cx, turn_failed());
    let line = |chat: &Entity<Chat>, cx: &VisualTestContext| {
        chat.read_with(&cx.cx, |chat, _| chat.control_snapshot().queue_held_line)
    };
    assert_eq!(line(&chat, cx).as_deref(), Some(HELD_LINE));

    let row = error_row(&chat, cx);
    chat.update(cx, |chat, cx| chat.dismiss_error(row, cx));

    assert_eq!(line(&chat, cx).as_deref(), Some(HELD_LINE_NO_RETRY));
    assert!(chat.read_with(&cx.cx, |chat, _| chat.queue_held));
}

/// Sign-in is out of band: a plain relaunch of an AuthRequired row connects and sends nothing, so it
/// must not leave the old prompt for a later idle death to resume and resend.
#[gpui::test]
async fn a_plain_relaunch_forgets_the_prompt_it_did_not_resend(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = offline_chat(cx, claude_launch("session_state", &logs));
    chat.update(cx, |chat, cx| {
        chat.agent_session_id = Some("fixture-session-1".to_string());
        chat.submit_turn("silent mid-turn".into(), Vec::new(), Vec::new(), cx);
    });
    event(
        &chat,
        cx,
        sirio_chat::ChatEvent::TransportError("Claude Code requires authentication".into()),
    );
    assert_eq!(
        readout(&chat, cx),
        ("authRequired".to_string(), "relaunch".to_string())
    );

    let row = error_row(&chat, cx);
    retry_row(&chat, cx, row);
    pump_chat_until_within(cx, &chat, 3000, |chat| {
        chat.client.is_some() && !chat.connecting
    });
    assert!(logs.heard().is_empty(), "nothing was resent");

    event(&chat, cx, connection_lost());
    assert_eq!(
        readout(&chat, cx),
        ("disconnected".to_string(), "relaunch".to_string()),
        "an idle death has nothing to resend"
    );
}

/// The socket's `retry` answers what happened: a click that the chat ignores is an error there, not a
/// success an E2E could read.
#[gpui::test]
async fn the_socket_retry_refuses_what_a_click_would_ignore(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent first");
    wait_for_prompts(&chat, cx, &logs, 1);
    event(&chat, cx, turn_failed());
    let row = error_row(&chat, cx);
    let press = |chat: &Entity<Chat>, cx: &mut VisualTestContext, entry: usize| {
        chat.update(cx, |chat, cx| chat.transcript_act(TranscriptAction::Retry(entry), cx))
    };

    // Not an error row.
    assert_eq!(
        press(&chat, cx, 0),
        Err("surface.chat.transcript retry: entry has no Retry".to_string())
    );
    // A turn is running: Retry would send a second prompt.
    chat.update(cx, |chat, _| chat.streaming = true);
    assert_eq!(
        press(&chat, cx, row),
        Err("surface.chat.transcript retry: a turn is running".to_string())
    );
    settle(cx, 30);
    assert_eq!(logs.heard().len(), 1, "nothing was sent");
    // Idle again: the same call is the click, and succeeds.
    chat.update(cx, |chat, _| chat.streaming = false);
    assert_eq!(press(&chat, cx, row), Ok(()));
    wait_for_prompts(&chat, cx, &logs, 2);
}

/// The same for a relaunch that cannot start because one is already connecting.
#[gpui::test]
async fn the_socket_retry_refuses_a_relaunch_while_one_is_connecting(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = offline_chat(cx, claude_launch("session_state", &logs));
    chat.update(cx, |chat, cx| {
        chat.agent_session_id = Some("fixture-session-1".to_string());
        chat.submit_turn("silent mid-turn".into(), Vec::new(), Vec::new(), cx);
    });
    event(&chat, cx, connection_lost());
    let row = error_row(&chat, cx);
    assert_eq!(readout(&chat, cx).1, "resume");

    chat.update(cx, |chat, _| chat.connecting = true);
    let refused = chat.update(cx, |chat, cx| {
        chat.transcript_act(TranscriptAction::Retry(row), cx)
    });

    assert_eq!(
        refused,
        Err("surface.chat.transcript retry: the agent is connecting".to_string())
    );
    assert!(logs.resumes().is_empty(), "nothing was launched");
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

// The usage-limit card (spec §5.4), against the fake `claude`. It is wrong if:
// - the countdown, the Try again button and the "lifted" words do not follow the wall clock: a
//   machine that slept past the reset still counts, because the anchor never moved;
// - the covered error row is drawn, or its Retry is pressable, while the card counts, or the
//   held queue names a Retry that is not there yet;
// - the card's sentence cannot be dragged, or Select All copies the hidden error row as well;
// - the card hides a row that is not the error row of its own turn;
// - a failure of a later turn is hidden by an old card, or the card's numbers do not follow the
//   rows when one before them goes.
// No test sleeps: a counting card is one whose reset is an hour off, and the wall clock passing
// the reset is the card's `resets_at` moved into the past.

fn rejected(resets_in: i64) -> SessionNotice {
    SessionNotice::RateLimited {
        status: "rejected".into(),
        window: Some("five_hour".into()),
        resets_at: Some(chrono::Local::now().timestamp() + resets_in),
        used_percent: None,
        limit_scope: None,
    }
}

fn notice(chat: &Entity<Chat>, cx: &mut VisualTestContext, notice: SessionNotice) {
    chat.update(cx, |chat, cx| chat.apply_notice(notice, cx));
}

/// The card's row and the failed row it covers.
fn card_state(chat: &Entity<Chat>, cx: &VisualTestContext) -> Option<(usize, Option<usize>)> {
    chat.read_with(&cx.cx, |chat, _| {
        chat.limit_card
            .as_ref()
            .map(|card| (card.entry, card.failed_entry))
    })
}

fn rows(chat: &Entity<Chat>, cx: &VisualTestContext) -> Vec<BTreeMap<String, String>> {
    chat.read_with(&cx.cx, |chat, _| chat.control_snapshot().transcript)
}

/// The wall clock passes the card's reset: the machine slept, and the card's own anchor, on the
/// monotonic clock, did not move.
fn pass_the_reset(chat: &Entity<Chat>, cx: &mut VisualTestContext) {
    chat.update(cx, |chat, _| {
        chat.limit_card.as_mut().expect("a card exists").resets_at =
            chrono::Local::now() - chrono::Duration::seconds(1);
    });
}

fn held_line(chat: &Entity<Chat>, cx: &VisualTestContext) -> Option<String> {
    chat.read_with(&cx.cx, |chat, _| chat.control_snapshot().queue_held_line)
}

/// A turn that failed with a limit an hour off, and a prompt waiting behind it.
fn failed_with_limit(
    chat: &Entity<Chat>,
    cx: &mut VisualTestContext,
    logs: &Logs,
    queued: bool,
) -> (usize, usize) {
    send(chat, cx, "silent first");
    wait_for_prompts(chat, cx, logs, 1);
    if queued {
        send(chat, cx, "silent second");
    }
    notice(chat, cx, rejected(3600));
    event(chat, cx, turn_failed());
    refresh_frame(cx);
    let (entry, failed) = card_state(chat, cx).expect("a card exists");
    (entry, failed.expect("the failed row is marked"))
}

/// While the limit lasts the card counts and offers nothing: the covered row is not drawn and
/// says it has no Retry, and the held queue's line does not send the user to a Retry that is
/// not there.
#[gpui::test]
async fn a_counting_card_covers_the_failed_row_and_offers_no_retry(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    let (entry, failed) = failed_with_limit(&chat, cx, &logs, true);

    assert_eq!(failed, entry + 1, "the error row follows the notice it stands behind");
    assert!(cx.debug_bounds("limit-card").is_some(), "the card is drawn");
    assert!(cx.debug_bounds("limit-card-countdown").is_some(), "it counts");
    assert!(cx.debug_bounds("limit-card-retry").is_none(), "no Try again yet");
    assert!(cx.debug_bounds("chat-retry").is_none(), "the covered row's Retry is not drawn");
    assert!(
        cx.debug_bounds(Box::leak(format!("chat-error-message-{failed}").into_boxed_str()))
            .is_none(),
        "the covered error row is not drawn"
    );
    let rows = rows(&chat, cx);
    assert_eq!(rows[entry]["kind"], "limitCard");
    assert_eq!(rows[entry]["phase"], "counting");
    assert!(rows[entry]["resetsAt"].parse::<i64>().is_ok());
    assert_eq!(rows[failed]["kind"], "error", "the error row is still a row");
    assert_eq!(rows[failed]["coveredBy"], "limitCard");
    assert_eq!(rows[failed]["retry"], "none", "the socket refuses it while the card counts");
    assert_eq!(
        held_line(&chat, cx).as_deref(),
        Some(HELD_LINE_NO_RETRY),
        "no Retry to press yet"
    );
}

/// The wall clock passing the reset flips the card to Try again, and the held queue's line is
/// today's again. Pressing it resends the same prompt, and the card then says the limit lifted.
#[gpui::test]
async fn the_card_offers_try_again_once_the_wall_clock_passes_the_reset(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    let (entry, _) = failed_with_limit(&chat, cx, &logs, true);
    assert!(cx.debug_bounds("limit-card-countdown").is_some());

    pass_the_reset(&chat, cx);
    refresh_frame(cx);
    assert!(cx.debug_bounds("limit-card-countdown").is_none(), "no longer counting");
    assert_eq!(rows(&chat, cx)[entry]["phase"], "retry");
    assert_eq!(held_line(&chat, cx).as_deref(), Some(HELD_LINE));
    let retry = cx.debug_bounds("limit-card-retry").expect("Try again is offered");
    cx.simulate_click(retry.center(), Modifiers::none());
    cx.run_until_parked();

    wait_for_prompts(&chat, cx, &logs, 2);
    let heard = logs.heard();
    assert_eq!(heard[0], heard[1], "the same prompt was sent again");
    chat.read_with(&cx.cx, |chat, _| {
        assert!(!chat.entries.iter().any(|e| matches!(e, Entry::Error { .. })));
    });
    refresh_frame(cx);
    assert!(cx.debug_bounds("limit-card-retry").is_none(), "no second Try again");
    assert!(cx.debug_bounds("limit-card-lifted").is_some(), "the card says the limit lifted");
}

/// No turn failed with the limit: the card counts, then says it lifted, with no button.
#[gpui::test]
async fn a_limit_with_no_failed_turn_lifts_with_no_button(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent first");
    wait_for_prompts(&chat, cx, &logs, 1);
    notice(&chat, cx, rejected(3600));
    event(&chat, cx, turn_ended());
    refresh_frame(cx);
    assert!(cx.debug_bounds("limit-card-countdown").is_some());

    pass_the_reset(&chat, cx);
    refresh_frame(cx);
    assert!(cx.debug_bounds("limit-card-lifted").is_some());
    assert!(cx.debug_bounds("limit-card-retry").is_none());
    let (entry, failed) = card_state(&chat, cx).unwrap();
    assert_eq!(failed, None);
    assert_eq!(rows(&chat, cx)[entry]["phase"], "lifted");
}

/// The card's sentence is read through the transcript's selection, as the line it replaces was:
/// a drag over it selects it, and Select All copies it once, with no trace of the hidden row.
#[gpui::test]
async fn the_cards_sentence_is_selected_and_copied_without_the_hidden_row(
    cx: &mut TestAppContext,
) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    let (entry, _) = failed_with_limit(&chat, cx, &logs, false);
    let sentence = chat.read_with(&cx.cx, |chat, _| chat.entries[entry].plain_text());

    let text = cx
        .debug_bounds("limit-card-text")
        .expect("the sentence is drawn in the transcript's selection");
    let y = text.center().y;
    cx.simulate_mouse_down(point(text.left() + px(1.0), y), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(point(text.right() - px(1.0), y), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(point(text.right() - px(1.0), y), MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let dragged = chat.read_with(&cx.cx, |chat, _| chat.selected_transcript_text());
    assert_eq!(dragged.as_deref(), Some(sentence.as_str()), "a drag over the card copies what it shows");

    chat.update(cx, |chat, cx| chat.select_all_transcript(cx));
    let all = chat
        .read_with(&cx.cx, |chat, _| chat.selected_transcript_text())
        .expect("Select All selects");
    assert!(all.contains(&sentence), "the card's text is copied: {all:?}");
    assert!(
        !all.contains("API Error: 529 overloaded"),
        "the covered row adds nothing to the copy: {all:?}"
    );
    assert!(!all.contains("\n\n\n"), "and leaves no gap where it was: {all:?}");
}

/// A row the card does not stand for is never hidden, whatever its numbers say.
#[gpui::test]
async fn a_stale_card_never_hides_a_row_that_is_not_its_error(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    failed_with_limit(&chat, cx, &logs, false);
    assert!(cx.debug_bounds("user-bubble-0").is_some());

    chat.update(cx, |chat, _| {
        chat.limit_card.as_mut().unwrap().failed_entry = Some(0);
    });
    refresh_frame(cx);
    assert!(
        cx.debug_bounds("user-bubble-0").is_some(),
        "the first question is still drawn"
    );
    assert!(!rows(&chat, cx)[0].contains_key("coveredBy"));
}

/// A later turn that fails for another reason is not hidden by an old card.
#[gpui::test]
async fn a_later_failure_is_not_covered_by_an_old_card(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    send(&chat, cx, "silent first");
    wait_for_prompts(&chat, cx, &logs, 1);
    notice(&chat, cx, rejected(3600));
    event(&chat, cx, turn_ended());
    send(&chat, cx, "silent second");
    wait_for_prompts(&chat, cx, &logs, 2);
    event(&chat, cx, turn_failed());
    refresh_frame(cx);
    assert_eq!(card_state(&chat, cx).unwrap().1, None, "not the card's turn");
    assert!(cx.debug_bounds("chat-retry").is_some(), "the later failure is drawn, with Retry");
}

/// A prompt sent while the card counts retires the failed row's Retry: the card must not offer it.
#[gpui::test]
async fn a_new_prompt_retires_the_cards_try_again(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    failed_with_limit(&chat, cx, &logs, false);
    send(&chat, cx, "silent second");
    wait_for_prompts(&chat, cx, &logs, 2);
    assert_eq!(card_state(&chat, cx).unwrap().1, None);
    event(&chat, cx, turn_ended());
    pass_the_reset(&chat, cx);
    refresh_frame(cx);
    assert!(cx.debug_bounds("limit-card-retry").is_none(), "nothing to retry any more");
    assert!(cx.debug_bounds("limit-card-lifted").is_some());
}

/// Other statuses, and a rejection with no reset time, keep today's line.
#[gpui::test]
async fn other_statuses_keep_the_warning_line(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    notice(
        &chat,
        cx,
        SessionNotice::RateLimited {
            status: "allowed_warning".into(),
            window: Some("five_hour".into()),
            resets_at: Some(chrono::Local::now().timestamp() + 3600),
            used_percent: Some(92),
            limit_scope: None,
        },
    );
    notice(
        &chat,
        cx,
        SessionNotice::RateLimited {
            status: "rejected".into(),
            window: None,
            resets_at: None,
            used_percent: None,
            limit_scope: None,
        },
    );
    assert_eq!(card_state(&chat, cx), None);
    let kinds: Vec<String> = rows(&chat, cx).iter().map(|r| r["kind"].clone()).collect();
    assert_eq!(kinds, vec!["warning", "warning"]);
}

/// The socket's `retry` on the card or on the row it covers: refused while the limit lasts, then
/// the same as the error row's, once.
#[gpui::test]
async fn the_socket_retry_presses_the_card_once_the_limit_has_passed(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    let (entry, failed) = failed_with_limit(&chat, cx, &logs, false);
    let press = |chat: &Entity<Chat>, cx: &mut VisualTestContext, at: usize| {
        chat.update(cx, |chat, cx| chat.transcript_act(TranscriptAction::Retry(at), cx))
    };
    assert!(press(&chat, cx, entry).is_err(), "counting: refused");
    assert!(press(&chat, cx, failed).is_err(), "the covered row is the card's: refused too");
    assert_eq!(logs.heard().len(), 1);

    pass_the_reset(&chat, cx);
    assert_eq!(press(&chat, cx, entry), Ok(()));
    wait_for_prompts(&chat, cx, &logs, 2);
    assert!(press(&chat, cx, entry).is_err(), "pressed once: nothing left to retry");
}

/// Rows renumbered under the card: a recovered connection error before it goes, and the card
/// follows its rows; a new conversation drops it.
#[gpui::test]
async fn the_card_follows_renumbered_rows(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    chat.update(cx, |chat, _| {
        chat.push_entry(Entry::Error {
            message: "old".into(),
            retryable: false,
            kind: ErrorKind::Connection,
        })
    });
    let (entry, failed) = failed_with_limit(&chat, cx, &logs, false);
    chat.update(cx, |chat, _| chat.clear_recovered_connection_errors());
    assert_eq!(card_state(&chat, cx), Some((entry - 1, Some(failed - 1))));
    chat.read_with(&cx.cx, |chat, _| {
        let card = chat.limit_card.as_ref().unwrap();
        assert!(matches!(chat.entries[card.entry], Entry::Notice { .. }));
        assert!(matches!(chat.entries[card.failed_entry.unwrap()], Entry::Error { .. }));
    });
    chat.update(cx, |chat, cx| chat.new_conversation(cx));
    assert_eq!(card_state(&chat, cx), None);
}

/// The selection is of the offsets of the text it was made on: a row that comes back into the copy
/// moves every offset below it, so uncovering a row ends the selection, as removing one does.
#[gpui::test]
async fn select_all_ends_when_a_row_stops_being_covered(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    failed_with_limit(&chat, cx, &logs, false);
    chat.update(cx, |chat, cx| chat.select_all_transcript(cx));
    assert!(chat.read_with(&cx.cx, |chat, _| chat.selected_transcript_text()).is_some());

    // A prompt retires the failed row's Retry: the card stops covering it, and it is drawn again.
    send(&chat, cx, "silent second");
    wait_for_prompts(&chat, cx, &logs, 2);
    assert_eq!(card_state(&chat, cx).unwrap().1, None, "the row is uncovered");
    assert!(
        chat.read_with(&cx.cx, |chat, _| chat.transcript_selection.is_none()),
        "the selection was of the text without that row"
    );
}

/// The same when a newer limit replaces the card and its covered row comes back.
#[gpui::test]
async fn select_all_ends_when_a_new_card_replaces_the_one_that_covered_a_row(
    cx: &mut TestAppContext,
) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    failed_with_limit(&chat, cx, &logs, false);
    chat.update(cx, |chat, cx| chat.select_all_transcript(cx));

    notice(&chat, cx, rejected(7200));
    assert!(
        chat.read_with(&cx.cx, |chat, _| chat.transcript_selection.is_none()),
        "the old card's row is drawn again, which moves every offset below it"
    );
}

/// The card owns the loop that wakes the chat at its reset, so a replaced card, a new
/// conversation and a closed chat do not leave it waking every minute until the reset.
#[gpui::test]
async fn the_card_owns_its_wake_loop(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let logs = Logs::in_dir(&dir.0);
    let (chat, cx) = silent_chat(cx, &dir.0, &logs);
    failed_with_limit(&chat, cx, &logs, false);
    assert!(
        chat.read_with(&cx.cx, |chat, _| chat.limit_card.as_ref().unwrap().wake.is_some()),
        "the card holds the task that wakes the chat at its reset"
    );

    // Put a task that notices being dropped where the card keeps its loop.
    let seen_by_task = std::rc::Rc::new(());
    let hold = |chat: &Entity<Chat>, cx: &mut VisualTestContext| {
        let held = seen_by_task.clone();
        chat.update(cx, |chat, cx| {
            chat.limit_card.as_mut().unwrap().wake = Some(cx.spawn(async move |_, _| {
                let _held = held;
                std::future::pending::<()>().await;
            }));
        });
        cx.run_until_parked();
    };
    hold(&chat, cx);
    assert_eq!(std::rc::Rc::strong_count(&seen_by_task), 2, "the task is alive");

    notice(&chat, cx, rejected(7200));
    cx.run_until_parked();
    assert_eq!(std::rc::Rc::strong_count(&seen_by_task), 1, "a replaced card drops its loop");

    hold(&chat, cx);
    assert_eq!(std::rc::Rc::strong_count(&seen_by_task), 2);
    chat.update(cx, |chat, cx| chat.new_conversation(cx));
    cx.run_until_parked();
    assert_eq!(std::rc::Rc::strong_count(&seen_by_task), 1, "a new conversation drops it too");
}
