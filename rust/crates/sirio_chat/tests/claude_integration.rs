//! The native Claude client against a deterministic fake `claude`.
//!
//! Test for test, this mirrors `acp_integration.rs`: the two transports owe
//! the surface the same behaviour, so they are held to the same list.

use std::path::PathBuf;
use std::time::Duration;

use futures::future::Either;
use sirio_chat::ThinkingDisplay;
use sirio_chat::{
    AvailableCommandInfo, BackgroundTaskEvent, BackgroundTaskRow, ChatEvent, ClaudeClient,
    ClaudeLaunch, CliState, CloseReason, ElicitationAction, ElicitationPrompt, RequestKind,
    SessionNotice, SpanKind, TraceEvent,
};
use sirio_claude::FastMode;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/claude_fixture.py"
);

/// The fixture is a python script, so the program is `python3` and the
/// script is its first argument — `ClaudeLaunch` carries the extra
/// arguments a test needs in front of the protocol flags.
fn launch(mode: &str) -> (ClaudeClient, sirio_chat::EventStream) {
    ClaudeClient::launch(fixture_launch(mode), std::env::temp_dir())
        .expect("fixture claude should complete its handshake")
}

fn fixture_launch(mode: &str) -> ClaudeLaunch {
    ClaudeLaunch::fixture(PathBuf::from("python3"), vec![FIXTURE.into(), mode.into()])
}

fn next_event(events: &sirio_chat::EventStream) -> ChatEvent {
    let receive = events.recv();
    // A guard against a hang, not a budget: the only thing it must be is
    // longer than this suite's worst scheduling delay. Five seconds was not
    // — a fixture subprocess loses the CPU for longer than that when the
    // whole workspace's tests run at once, and the turn it was driving is
    // fine by the time anyone looks.
    let timer = async_io::Timer::after(Duration::from_secs(60));
    futures::executor::block_on(async move {
        futures::pin_mut!(receive, timer);
        match futures::future::select(receive, timer).await {
            Either::Left((event, _)) => event.expect("event stream should remain open"),
            Either::Right((_, _)) => panic!("fixture did not emit an event within sixty seconds"),
        }
    })
}

/// The first re-read signal whose name starts with `prefix`, on a short
/// budget. It reads the stream once, so a test can say *which* signal
/// arrived rather than draining the stream asking after each in turn.
fn first_signal(
    events: &sirio_chat::EventStream,
    prefix: &str,
    budget: Duration,
) -> Option<String> {
    let deadline = std::time::Instant::now() + budget;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        let receive = events.recv();
        let timer = async_io::Timer::after(remaining);
        let got = futures::executor::block_on(async move {
            futures::pin_mut!(receive, timer);
            match futures::future::select(receive, timer).await {
                Either::Left((event, _)) => event.ok(),
                Either::Right((_, _)) => None,
            }
        });
        match got {
            Some(ChatEvent::OtherSessionUpdate { kind }) if kind.starts_with(prefix) => {
                return Some(kind);
            }
            Some(_) => continue,
            None => return None,
        }
    }
}

/// Everything up to and including the first event `last` accepts.
fn drain_until(
    events: &sirio_chat::EventStream,
    last: impl Fn(&ChatEvent) -> bool,
) -> Vec<ChatEvent> {
    let mut seen = Vec::new();
    loop {
        let event = next_event(events);
        let done = last(&event);
        seen.push(event);
        if done {
            return seen;
        }
    }
}

fn drain_until_turn_end(events: &sirio_chat::EventStream) -> Vec<ChatEvent> {
    drain_until(events, |event| matches!(event, ChatEvent::TurnEnded { .. }))
}

/// A turn is over when it ended or when it failed (`TurnFailed` ends it in place of `TurnEnded`).
fn ends_the_turn(event: &ChatEvent) -> bool {
    matches!(
        event,
        ChatEvent::TurnEnded { .. } | ChatEvent::TurnFailed { .. }
    )
}

#[test]
fn the_handshake_yields_the_catalogues_before_anything_is_typed() {
    let (mut client, _events) = launch("normal");
    let models = client.model_catalog().expect("models from the handshake");
    assert_eq!(models.config_id, "model");
    assert_eq!(
        models
            .options
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["default", "sonnet"]
    );
    assert_eq!(models.selected_id, "default");

    let modes = client.mode_catalog().expect("modes from the handshake");
    assert_eq!(modes.current_id, "default");
    assert!(modes.options.iter().any(|mode| mode.id == "plan"));
    assert!(
        modes
            .options
            .iter()
            .any(|mode| mode.id == "bypassPermissions")
    );

    client.shutdown().expect("fixture should shut down cleanly");
}

#[test]
fn a_command_list_that_changes_mid_session_reaches_the_picker() {
    let (mut client, events) = launch("commands_changed");
    // The handshake publishes its own list first; the change follows it.
    let mut lists: Vec<Vec<AvailableCommandInfo>> = Vec::new();
    let changed = loop {
        match next_event(&events) {
            ChatEvent::AvailableCommands(commands) => {
                lists.push(commands);
                if lists.len() == 2 {
                    break lists.pop().expect("the second list");
                }
            }
            ChatEvent::OtherSessionUpdate { kind } if kind == "system/commands_changed" => {
                panic!("the change was named but the new list was never published");
            }
            _ => {}
        }
    };
    let names: Vec<&str> = changed
        .iter()
        .map(|command| command.name.as_str())
        .collect();
    // `terminal_slash_commands` came with the handshake and is not resent,
    // so the filter it feeds has to survive every later list.
    assert_eq!(names, ["deep-research"]);
    assert_eq!(changed[0].argument_hint.as_deref(), Some("<question>"));

    client.shutdown().expect("fixture should shut down cleanly");
}

#[test]
fn a_turn_streams_its_text_and_ends_with_usage() {
    let (mut client, events) = launch("normal");
    client.prompt("hello").expect("prompt is accepted");
    let seen = drain_until_turn_end(&events);
    let text: String = seen
        .iter()
        .filter_map(|event| match event {
            ChatEvent::AgentMessageChunk(chunk) => Some(chunk.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "Hello there");
    assert!(seen.iter().any(|event| matches!(
        event,
        ChatEvent::TokenUsageBreakdown {
            input_tokens: 100,
            output_tokens: 20,
            ..
        }
    )));
    assert_eq!(
        seen.last(),
        Some(&ChatEvent::TurnEnded {
            stop_reason: "EndTurn".into()
        })
    );
    client.shutdown().expect("fixture should shut down cleanly");
}

#[test]
fn the_session_id_is_named_at_launch_and_the_clis_own_replaces_it() {
    let (mut client, events) = launch("normal");
    // Named before anything is typed — that is what `--session-id` buys.
    let chosen = client
        .session_id()
        .expect("a fresh session is named at launch");
    client.prompt("hello").expect("prompt is accepted");
    let seen = drain_until_turn_end(&events);
    assert_eq!(
        seen.iter()
            .filter(|event| **event
                == ChatEvent::OtherSessionUpdate {
                    kind: "SessionIdentified".into()
                })
            .count(),
        1
    );
    // The CLI stays the authority on which session exists: this fixture
    // opens one under its own name, and that is the id a later `--resume`
    // has to use.
    assert_eq!(client.session_id().as_deref(), Some("fixture-session-1"));
    assert_ne!(chosen, "fixture-session-1");
    assert_eq!(client.claude_version().as_deref(), Some("2.1.273"));
    client.shutdown().expect("fixture should shut down cleanly");
}

#[test]
fn a_logged_out_cli_is_refused_at_the_handshake_with_login_guidance() {
    let error = ClaudeClient::launch(fixture_launch("logged_out"), std::env::temp_dir())
        .expect_err("a logged-out CLI must not yield a usable session");
    let message = format!("{error:#}");
    assert!(
        message.contains("requires authentication"),
        "the error must reach the surface's auth path: {message}"
    );
}

#[test]
fn a_malformed_line_is_skipped_without_hanging_the_session() {
    // The fixture writes `not-json` immediately before its handshake reply.
    let (mut client, events) = launch("malformed");
    client.prompt("hello").expect("prompt is accepted");
    let seen = drain_until_turn_end(&events);
    assert!(
        seen.iter()
            .any(|event| matches!(event, ChatEvent::AgentMessageChunk(_)))
    );
    client.shutdown().expect("fixture should shut down cleanly");
}

#[test]
fn a_dead_agent_names_its_exit_and_its_last_words() {
    let (_client, events) = launch("death");
    _client.prompt("hello").expect("prompt is accepted");
    let message = loop {
        match next_event(&events) {
            ChatEvent::TransportError(message) => break message,
            ChatEvent::TurnEnded { .. } => panic!("a dead agent must not report a normal turn end"),
            _ => continue,
        }
    };
    assert!(
        message.contains("exited with code 3"),
        "the report must name the exit: {message}"
    );
    assert!(
        message.contains("fatal error, going away"),
        "the report must carry the agent's last stderr line: {message}"
    );
}

#[test]
fn a_silent_cli_fails_startup_rather_than_hanging() {
    let started = std::time::Instant::now();
    let error = ClaudeClient::launch_with_timeouts(
        fixture_launch("silent"),
        std::env::temp_dir(),
        Duration::from_millis(400),
        Duration::from_secs(60),
        Duration::from_secs(60),
    )
    .expect_err("a silent CLI must not yield a session");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(format!("{error:#}").to_lowercase().contains("timed out"));
}

#[test]
fn shutdown_reaps_the_process() {
    let pid_path = std::env::temp_dir().join(format!("sirio-claude-{}.pid", std::process::id()));
    let _ = std::fs::remove_file(&pid_path);
    let launch = ClaudeLaunch::fixture(
        PathBuf::from("python3"),
        vec![
            FIXTURE.into(),
            "normal".into(),
            pid_path.to_string_lossy().into_owned(),
        ],
    );
    let (mut client, _events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("fixture handshake");
    while !pid_path.exists() {
        std::thread::sleep(Duration::from_millis(1));
    }
    let pid = std::fs::read_to_string(&pid_path).expect("fixture publishes its pid");
    client.shutdown().expect("fixture should shut down cleanly");
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while process_exists(pid.trim()) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !process_exists(pid.trim()),
        "fixture process should be reaped"
    );
    let _ = std::fs::remove_file(&pid_path);
}

#[cfg(not(windows))]
fn process_exists(pid: &str) -> bool {
    std::process::Command::new("ps")
        .args(["-p", pid, "-o", "pid="])
        .output()
        .map(|output| !String::from_utf8_lossy(&output.stdout).trim().is_empty())
        .unwrap_or(false)
}

#[cfg(windows)]
fn process_exists(pid: &str) -> bool {
    let filter = format!("PID eq {pid}");
    std::process::Command::new("tasklist")
        .args(["/FI", &filter, "/NH"])
        .output()
        .map(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains(pid)
        })
        .unwrap_or(false)
}

#[test]
fn fast_mode_starts_off_and_moves_only_once_the_cli_agrees() {
    let (mut client, events) = launch("normal");
    assert_eq!(
        client.fast_mode(),
        Some(FastMode {
            enabled: false,
            blocked_by: None
        }),
        "the handshake reports it off, and its opt-in notice is not a block"
    );

    client.set_fast_mode(true).expect("the toggle is accepted");
    assert_eq!(
        first_signal(&events, "FastModeUpdate", Duration::from_secs(5)).as_deref(),
        Some("FastModeUpdate(on)"),
        "the change is announced as a re-read signal, the way a mode change is"
    );
    assert_eq!(client.fast_mode().map(|fast| fast.enabled), Some(true));

    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_cli_that_refuses_fast_mode_leaves_the_chip_where_it_was() {
    let (mut client, events) = launch("no_fast_mode");
    client.set_fast_mode(true).expect("the toggle is accepted");
    // Announced, so the chip redraws — but never as a change that happened.
    assert_eq!(
        first_signal(&events, "FastModeUpdate", Duration::from_secs(5)).as_deref(),
        Some("FastModeUpdate(blocked)"),
    );
    assert_eq!(
        client.fast_mode(),
        Some(FastMode {
            enabled: false,
            blocked_by: Some("Unknown setting: fastMode".into())
        }),
        "the chip stays where the session is and carries the CLI's own reason"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn the_thinking_display_is_unset_until_chosen_and_then_reports_the_choice() {
    let (mut client, events) = launch("normal");
    // Nothing reads the session's own value back, so before a choice the
    // only honest answer is that none was made.
    assert_eq!(client.thinking_display(), ThinkingDisplay::default());

    client
        .set_thinking_display(Some("omitted".into()))
        .expect("the choice is accepted");
    assert_eq!(
        first_signal(&events, "ThinkingDisplayUpdate", Duration::from_secs(5)).as_deref(),
        Some("ThinkingDisplayUpdate(omitted)")
    );
    assert_eq!(
        client.thinking_display(),
        ThinkingDisplay {
            chosen: Some("omitted".into()),
            unsupported: false
        }
    );

    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_cli_that_does_not_know_the_thinking_verb_retires_the_control() {
    let (mut client, events) = launch("no_thinking");
    client
        .set_thinking_display(Some("omitted".into()))
        .expect("the choice is accepted");
    assert_eq!(
        first_signal(&events, "ThinkingDisplayUpdate", Duration::from_secs(5)).as_deref(),
        Some("ThinkingDisplayUpdate(unsupported)")
    );
    assert_eq!(
        client.thinking_display(),
        ThinkingDisplay {
            chosen: None,
            unsupported: true
        },
        "the refusal retires the control rather than showing a choice that did not take"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_model_change_reaches_the_cli_and_the_catalogue_follows() {
    let (mut client, _events) = launch("echo_control");
    client
        .set_model("model", "sonnet")
        .expect("model change is accepted");
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while client
        .model_catalog()
        .is_none_or(|catalog| catalog.selected_id != "sonnet")
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        client.model_catalog().expect("a catalogue").selected_id,
        "sonnet"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_mode_change_is_confirmed_by_the_agents_own_status_line() {
    let (mut client, events) = launch("echo_control");
    client.set_mode("plan").expect("mode change is accepted");
    // The CLI echoes the new mode as a status line; that echo, not the
    // request's own success, is what moves the pill.
    let mut seen_update = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !seen_update && std::time::Instant::now() < deadline {
        if let ChatEvent::OtherSessionUpdate { kind } = next_event(&events) {
            seen_update = kind == "CurrentModeUpdate(plan)";
        }
    }
    assert!(seen_update, "the mode change must reach the surface");
    assert_eq!(client.mode_catalog().expect("modes").current_id, "plan");
    client.shutdown().expect("clean shutdown");
}

#[test]
fn cancelling_a_turn_produces_a_terminal_outcome() {
    let (mut client, events) = launch("slow_turn");
    client.prompt("hello").expect("prompt is accepted");
    // Wait until the turn is demonstrably running before interrupting it.
    loop {
        if matches!(next_event(&events), ChatEvent::AgentMessageChunk(_)) {
            break;
        }
    }
    client.cancel().expect("cancel is accepted");
    let seen = drain_until_turn_end(&events);
    assert_eq!(
        seen.last(),
        Some(&ChatEvent::TurnEnded {
            stop_reason: "Cancelled".into()
        }),
        "a cancelled turn must say so, or the footer reads as an ordinary end"
    );
    // No shutdown assertion: `slow_turn` ends its own process the moment it
    // has answered the interrupt, so whether a worker is still there to take
    // a shutdown is a race with nothing to do with what a cancelled turn
    // reports. `shutdown_reaps_the_process` is where that belongs.
}

#[test]
fn a_failed_result_is_a_turn_failure() {
    let (mut client, events) = launch("failed_turns");
    let sentence = "API Error: 529 overloaded";
    client.prompt("overloaded").expect("prompt is accepted");
    let seen = drain_until(&events, ends_the_turn);
    assert!(
        seen.iter().any(|event| matches!(
            event,
            ChatEvent::TurnFailed { message, .. } if message == sentence
        )),
        "the failure must carry the CLI's sentence: {seen:?}"
    );
    assert!(
        !seen
            .iter()
            .any(|event| matches!(event, ChatEvent::AgentMessageChunk(_))),
        "the sentence is the failure's message, not also a reply: {seen:?}"
    );
    // The meter is read before the turn is over, a failed one included: a
    // surface re-reading on the end must not see the previous turn's numbers.
    assert!(
        seen.iter()
            .any(|event| matches!(event, ChatEvent::ContextUsage(_))),
        "the failed turn's end was not held for the context meter: {seen:?}"
    );

    // The failure ends the turn in place of `TurnEnded`, and the client stays:
    // the next turn is answered, and nothing of the first (a late end) is in
    // front of its reply.
    client.prompt("anything else").expect("second prompt accepted");
    let seen = drain_until_turn_end(&events);
    assert!(
        seen.contains(&ChatEvent::AgentMessageChunk("ok".into())),
        "the turn after a failure must answer, with no stray end before it: {seen:?}"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_turn_limit_result_keeps_todays_events() {
    // A resend would hit the same limit, so these two stay what they were: the
    // sentence as a reply, then the turn's end.
    let (mut client, events) = launch("failed_turns");
    for (prompt, sentence) in [
        ("turn limit", "Reached the turn limit"),
        ("budget limit", "Reached the budget limit"),
    ] {
        client.prompt(prompt).expect("prompt is accepted");
        let seen = drain_until(&events, ends_the_turn);
        assert!(
            seen.contains(&ChatEvent::AgentMessageChunk(sentence.into())),
            "{prompt}: the sentence is still drawn as a reply: {seen:?}"
        );
        assert!(
            matches!(seen.last(), Some(ChatEvent::TurnEnded { .. })),
            "{prompt}: the turn still ends with TurnEnded: {seen:?}"
        );
    }
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_stop_is_not_a_failure() {
    // The CLI answers an interrupt with `error_during_execution`, `is_error`, no stop
    // reason and (in this fixture) the words "Interrupted by user". The worker sent the
    // interrupt, so that result closes a stopped turn with `TurnEnded`, as before: no string
    // of the result decides it.
    let (mut client, events) = launch("trace");
    client.prompt("cancel").expect("prompt is accepted");
    loop {
        if matches!(next_event(&events), ChatEvent::AgentMessageChunk(_)) {
            break;
        }
    }
    client.cancel().expect("cancel is accepted");
    let seen = drain_until(&events, ends_the_turn);
    assert!(
        matches!(seen.last(), Some(ChatEvent::TurnEnded { .. })),
        "a stopped turn ends with TurnEnded, not TurnFailed: {seen:?}"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_stop_excuses_only_its_own_turn() {
    let (mut client, events) = launch("failed_turns");
    client.prompt("hold").expect("prompt is accepted");
    loop {
        if matches!(next_event(&events), ChatEvent::AgentMessageChunk(_)) {
            break;
        }
    }
    client.cancel().expect("cancel is accepted");
    let seen = drain_until(&events, ends_the_turn);
    assert!(
        matches!(seen.last(), Some(ChatEvent::TurnEnded { .. })),
        "the turn the worker interrupted ends with TurnEnded: {seen:?}"
    );

    // The same words with no Stop behind them are a failure: the next turn's result is
    // judged on its own, whatever it says.
    client
        .prompt("unasked interrupt")
        .expect("second prompt accepted");
    let seen = drain_until(&events, ends_the_turn);
    assert!(
        seen.iter().any(|event| matches!(
            event,
            ChatEvent::TurnFailed { message, .. } if message == "Interrupted by user"
        )),
        "a result nobody asked for is a failure, even when it reads like a Stop: {seen:?}"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_stop_still_excuses_its_turn_when_the_next_prompt_is_written_first() {
    // The stopped turn's result leaves `queued_turn_count` out (`hold late bare`): the count of
    // prompts pending only goes down by one.
    stop_then_a_prompt_before_the_stopped_result("hold late bare");
}

#[test]
fn a_stopped_result_that_counts_the_prompt_behind_it_excuses_only_its_turn() {
    // The stopped turn's result says the second prompt is queued (`queued_turn_count: 1`), as
    // the real CLI writes it: the count of prompts pending is taken from it.
    stop_then_a_prompt_before_the_stopped_result("hold late");
}

/// Stop frees the composer at once, so the next prompt is written while the CLI winds the
/// stopped turn down: the stopped turn's result comes after it. That result is still the
/// stopped turn's, and the second prompt's own failure is still its own.
fn stop_then_a_prompt_before_the_stopped_result(hold: &str) {
    let (mut client, events) = launch("failed_turns");
    client.prompt(hold).expect("prompt is accepted");
    loop {
        if matches!(next_event(&events), ChatEvent::AgentMessageChunk(_)) {
            break;
        }
    }
    client.cancel().expect("cancel is accepted");
    client.prompt("overloaded").expect("second prompt is accepted");

    let stopped = drain_until(&events, ends_the_turn);
    assert!(
        matches!(stopped.last(), Some(ChatEvent::TurnEnded { .. })),
        "the stopped turn ends with TurnEnded, though a prompt was written after the Stop: {stopped:?}"
    );
    let second = drain_until(&events, ends_the_turn);
    assert!(
        second.iter().any(|event| matches!(
            event,
            ChatEvent::TurnFailed { message, .. } if message == "API Error: 529 overloaded"
        )),
        "the second prompt's own failure is still a failure: {second:?}"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn one_result_for_two_prompts_leaves_no_turn_open() {
    // The CLI folds the second prompt into the running turn and answers both with one result
    // that says nothing is queued. A Stop pressed then interrupts nothing, so the next prompt's
    // failure (a 529, not a Stop's result) is still a failure.
    let (mut client, events) = launch("failed_turns");
    client.prompt("fold first").expect("first prompt is accepted");
    client.prompt("fold second").expect("second prompt is accepted");
    let folded = drain_until(&events, ends_the_turn);
    assert!(
        matches!(folded.last(), Some(ChatEvent::TurnEnded { .. })),
        "the one result ends the turn: {folded:?}"
    );

    client.cancel().expect("cancel is accepted");
    client.prompt("overloaded").expect("third prompt is accepted");
    let third = drain_until(&events, ends_the_turn);
    assert!(
        third.iter().any(|event| matches!(
            event,
            ChatEvent::TurnFailed { message, .. } if message == "API Error: 529 overloaded"
        )),
        "a Stop with nothing open excuses nothing: {third:?}"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_turn_the_cli_started_is_not_a_failed_turn() {
    // The answer to the prompt, then a turn the CLI starts on its own (as after a background
    // agent ends) that fails. No prompt of Sirio's is behind it, so there is nothing to retry:
    // today's events, the sentence as a reply and the turn's end.
    let (mut client, events) = launch("failed_turns");
    client.prompt("then wake").expect("prompt is accepted");
    let answered = drain_until(&events, ends_the_turn);
    assert!(
        matches!(answered.last(), Some(ChatEvent::TurnEnded { .. })),
        "the prompt's own turn ends normally: {answered:?}"
    );
    let woken = drain_until(&events, ends_the_turn);
    assert!(
        woken.contains(&ChatEvent::AgentMessageChunk("API Error: 529 overloaded".into())),
        "the sentence is drawn as a reply: {woken:?}"
    );
    assert!(
        matches!(woken.last(), Some(ChatEvent::TurnEnded { .. })),
        "a turn nobody prompted ends, it does not fail: {woken:?}"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_turn_the_cli_started_with_a_prompt_queued_behind_it_is_not_a_failed_turn() {
    // The CLI starts a turn of its own while the surface is idle (as after a background agent
    // ends), and the user's prompt is written and queued behind it. That turn fails, and its
    // result says the prompt is still queued: it closed no prompt of Sirio's, so it ends with
    // today's events (a Retry would run the prompt the CLI runs next anyway). The prompt's own
    // failure, after it, is still a failure.
    let (mut client, events) = launch("failed_turns");
    client.prompt("wake held").expect("prompt is accepted");
    let answered = drain_until(&events, ends_the_turn);
    assert!(
        matches!(answered.last(), Some(ChatEvent::TurnEnded { .. })),
        "the prompt's own turn ends normally: {answered:?}"
    );
    // The CLI's own turn is under way before the next prompt is written. Its first words can
    // come ahead of the answered turn's end, which waits for the context meter.
    let woke = ChatEvent::AgentMessageChunk("The agent finished. ".into());
    if !answered.contains(&woke) {
        while next_event(&events) != woke {}
    }
    client.prompt("overloaded").expect("second prompt is accepted");

    let woken = drain_until(&events, ends_the_turn);
    assert!(
        woken.contains(&ChatEvent::AgentMessageChunk("API Error: 529 overloaded".into())),
        "the CLI's own turn draws its sentence as a reply: {woken:?}"
    );
    assert!(
        matches!(woken.last(), Some(ChatEvent::TurnEnded { .. })),
        "a turn with the prompt still queued behind it ends, it does not fail: {woken:?}"
    );
    let prompted = drain_until(&events, ends_the_turn);
    assert!(
        prompted.iter().any(|event| matches!(
            event,
            ChatEvent::TurnFailed { message, .. } if message == "API Error: 529 overloaded"
        )),
        "the queued prompt's own failure is still a failure: {prompted:?}"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_thinking_block_stop_ends_the_thought() {
    let (mut client, events) = launch("thinking_blocks");
    client.prompt("think first").expect("prompt is accepted");
    let seen = drain_until_turn_end(&events);
    let thoughts: Vec<&str> = seen
        .iter()
        .filter_map(|event| match event {
            ChatEvent::ThoughtChunk(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(thoughts, ["weighing ", "the options"]);
    // The turn holds two text-block stops, one thinking-block stop, and the stops of two
    // thinking blocks whose messages are over (one ended, one cut off by the next message's
    // start): exactly one thought end, and only the one for a block that is still open.
    let ends: Vec<usize> = seen
        .iter()
        .enumerate()
        .filter(|(_, event)| matches!(event, ChatEvent::ThoughtEnded))
        .map(|(position, _)| position)
        .collect();
    assert_eq!(ends.len(), 1, "one thought end per thinking block: {seen:?}");
    let last_thought = seen
        .iter()
        .rposition(|event| matches!(event, ChatEvent::ThoughtChunk(_)))
        .expect("the thought was streamed");
    let first_reply = seen
        .iter()
        .position(|event| matches!(event, ChatEvent::AgentMessageChunk(_)))
        .expect("the reply was streamed");
    assert!(
        last_thought < ends[0] && ends[0] < first_reply,
        "the thought ends after its last chunk and before the reply starts: {seen:?}"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_turn_that_goes_silent_is_reaped_and_reported() {
    // Not `shutdown_reaps_the_process`'s file: under `cargo test` both are
    // threads of one process.
    let pid_path =
        std::env::temp_dir().join(format!("sirio-claude-silent-{}.pid", std::process::id()));
    let _ = std::fs::remove_file(&pid_path);
    let launch = ClaudeLaunch::fixture(
        PathBuf::from("python3"),
        vec![
            FIXTURE.into(),
            "silent_turn".into(),
            pid_path.to_string_lossy().into_owned(),
        ],
    );
    let (client, events) = ClaudeClient::launch_with_timeouts(
        launch,
        std::env::temp_dir(),
        Duration::from_secs(10),
        Duration::from_millis(400),
        Duration::from_secs(60),
    )
    .expect("handshake succeeds; it is the turn that hangs");
    let pid = std::fs::read_to_string(&pid_path).expect("fixture publishes its pid");
    // Without this, a `ps` that cannot answer would pass the check below.
    assert!(process_exists(pid.trim()), "the fixture runs before the turn");
    client.prompt("hello").expect("prompt is accepted");
    let event = loop {
        match next_event(&events) {
            event @ (ChatEvent::Timeout { .. } | ChatEvent::TransportError(_)) => break event,
            _ => continue,
        }
    };
    assert!(
        matches!(
            event,
            ChatEvent::Timeout {
                operation: sirio_chat::TimeoutOperation::Prompt,
                ..
            }
        ),
        "an idle turn must be reported as a prompt timeout, got {event:?}"
    );
    // The client is still alive: its drop would kill the process too, and
    // then this would pass on a watchdog that only reported.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while process_exists(pid.trim()) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !process_exists(pid.trim()),
        "the watchdog must reap the silent agent, not leave it to the client's drop"
    );
    drop(client);
    let _ = std::fs::remove_file(&pid_path);
}

#[test]
fn a_turn_that_keeps_reporting_is_never_reaped_for_taking_long() {
    // The watchdog measures silence, not duration: an agent that works for
    // longer than the window while still reporting must survive. Killing it
    // loses the whole session, not just the turn.
    //
    // Two margins, and they pull apart rather than together. The premise
    // needs the turn to outlive the window: sixteen reports at 250 ms is 4 s
    // against a 2 s window, so it does, twice over. The assertion needs each
    // gap to stay well inside the window: 250 ms into 2 s is an eighth, so a
    // gap has to stretch eightfold before this measures the scheduler instead
    // of the watchdog. The tight version — 150 ms into 400 ms, barely two
    // window-lengths and a gap over a third of one — went red whenever the
    // whole workspace's suites ran at once.
    let (mut client, events) = ClaudeClient::launch_with_timeouts(
        fixture_launch("chatty_slow_turn"),
        std::env::temp_dir(),
        Duration::from_secs(10),
        Duration::from_secs(2),
        Duration::from_secs(60),
    )
    .expect("handshake succeeds");
    client.prompt("hello").expect("prompt is accepted");
    let seen = drain_until_turn_end(&events);
    assert!(
        !seen
            .iter()
            .any(|event| matches!(event, ChatEvent::Timeout { .. })),
        "a reporting agent must not be reaped: {seen:?}"
    );
    // No shutdown assertion here: `chatty_slow_turn` ends its own process
    // once the turn is done, so whether the worker is still there to take a
    // shutdown is a race with nothing to do with the watchdog. Both clients
    // report a gone worker as an error — `a_dead_agent_names_its_exit_...`
    // is where that belongs.
}

#[test]
fn a_tool_that_needs_permission_pauses_its_row_and_resumes_on_allow() {
    let (mut client, events) = launch("permission");
    client.prompt("delete it").expect("prompt is accepted");
    let (request_id, options) = loop {
        match next_event(&events) {
            ChatEvent::PermissionRequest {
                request_id,
                options,
                title,
                ..
            } => {
                assert_eq!(title, "rm -rf build");
                break (request_id, options);
            }
            _ => continue,
        }
    };
    assert!(options.iter().any(|option| option.id == "allow_once"));
    client
        .respond_permission(request_id, "allow_once")
        .expect("the choice reaches the agent");
    let seen = drain_until_turn_end(&events);
    assert!(
        seen.iter().any(|event| matches!(
            event,
            ChatEvent::ToolCallCompleted { status, .. } if status == "Completed"
        )),
        "an allowed tool must complete: {seen:?}"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn an_open_permission_does_not_freeze_the_stream() {
    // The card waits on a human. While it waits, the agent's other output
    // must keep arriving, or a second request could never be answered.
    let (mut client, events) = launch("permission_then_text");
    client.prompt("go").expect("prompt is accepted");
    let mut saw_request = false;
    let mut saw_text_after = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !saw_text_after && std::time::Instant::now() < deadline {
        match next_event(&events) {
            ChatEvent::PermissionRequest { .. } => saw_request = true,
            ChatEvent::AgentMessageChunk(_) if saw_request => saw_text_after = true,
            _ => {}
        }
    }
    assert!(
        saw_request && saw_text_after,
        "the stream must keep flowing"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_withdrawn_permission_is_answered_as_a_refusal() {
    let (mut client, events) = launch("permission");
    client.prompt("delete it").expect("prompt is accepted");
    let request_id = loop {
        if let ChatEvent::PermissionRequest { request_id, .. } = next_event(&events) {
            break request_id;
        }
    };
    client
        .cancel_permission(request_id)
        .expect("withdrawal reaches the agent");
    let seen = drain_until_turn_end(&events);
    assert!(
        seen.iter().any(|event| matches!(
            event,
            ChatEvent::ToolCallCompleted { status, .. } if status == "Failed"
        )),
        "a refused tool must not read as completed: {seen:?}"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_permission_left_open_expires_into_a_refusal() {
    let (mut client, events) = ClaudeClient::launch_with_timeouts(
        fixture_launch("permission"),
        std::env::temp_dir(),
        Duration::from_secs(10),
        Duration::from_secs(60),
        Duration::from_millis(400),
    )
    .expect("handshake succeeds");
    client.prompt("delete it").expect("prompt is accepted");
    loop {
        if matches!(next_event(&events), ChatEvent::PermissionRequest { .. }) {
            break;
        }
    }
    // Answer nothing. The deadline the user missed is a refusal, not a
    // protocol fault: the session stays alive and the tool is denied.
    let seen = drain_until_turn_end(&events);
    assert!(seen.iter().any(|event| matches!(
        event,
        ChatEvent::ToolCallCompleted { status, .. } if status == "Failed"
    )));
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_turn_and_its_permission_reach_the_trace_ahead_of_the_transcript() {
    let (mut client, events) = launch("permission");
    client.prompt("delete it").expect("prompt is accepted");
    let mut seen = Vec::new();
    let request_id = loop {
        let event = next_event(&events);
        let asked = match &event {
            ChatEvent::PermissionRequest { request_id, .. } => Some(*request_id),
            _ => None,
        };
        seen.push(event);
        if let Some(request_id) = asked {
            break request_id;
        }
    };
    client
        .respond_permission(request_id, "allow_once")
        .expect("the choice reaches the agent");
    seen.extend(drain_until_turn_end(&events));

    let trace: Vec<&TraceEvent> = seen
        .iter()
        .filter_map(|event| match event {
            ChatEvent::Trace(trace) => Some(trace),
            _ => None,
        })
        .collect();
    assert!(
        matches!(trace.first(), Some(TraceEvent::TurnStarted { .. })),
        "the prompt opens the turn: {trace:?}"
    );
    assert!(trace.iter().any(|event| matches!(
        event,
        TraceEvent::SpanStarted { id, kind: SpanKind::Tool, .. } if id == "toolu_perm_1"
    )));
    // The wait is the permission card's own request id, on the tool's span,
    // and the answer ends it.
    assert!(trace.iter().any(|event| matches!(
        event,
        TraceEvent::WaitStarted { span: Some(span), request_id: waited, .. }
            if span == "toolu_perm_1" && *waited == request_id
    )));
    assert!(trace.iter().any(|event| matches!(
        event,
        TraceEvent::WaitEnded { request_id: ended, .. } if *ended == request_id
    )));
    // The trace ends the turn when its `result` is read: ahead of the
    // transcript's own end, which the context meter holds back.
    let trace_end = seen
        .iter()
        .position(|event| matches!(event, ChatEvent::Trace(TraceEvent::TurnEnded { .. })))
        .expect("the result ends the trace's turn");
    let transcript_end = seen
        .iter()
        .position(|event| matches!(event, ChatEvent::TurnEnded { .. }))
        .expect("the transcript's turn ends");
    assert!(trace_end < transcript_end, "{seen:?}");
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_subagents_permission_asks_without_opening_a_tool_row() {
    // The subagent's request names its agent and arrives before the line that
    // holds its call, so its tool id alone cannot say whose it is. Its own
    // lines never reach the transcript: a row opened for it would never get
    // its result and would stay at the top level for the rest of the turn.
    let (mut client, events) = launch("subagent_permission_early");
    client.prompt("delegate it").expect("prompt is accepted");
    let mut seen = Vec::new();
    let request_id = loop {
        let event = next_event(&events);
        let asked = match &event {
            ChatEvent::PermissionRequest { request_id, .. } => Some(*request_id),
            _ => None,
        };
        seen.push(event);
        if let Some(request_id) = asked {
            break request_id;
        }
    };
    client
        .respond_permission(request_id, "allow_once")
        .expect("the choice reaches the agent");
    seen.extend(drain_until_turn_end(&events));

    assert!(
        !seen.iter().any(|event| matches!(
            event,
            ChatEvent::ToolCallStarted { .. }
                | ChatEvent::ToolCallUpdated { .. }
                | ChatEvent::ToolCallCompleted { .. }
        )),
        "a subagent's call is not a row of the transcript: {seen:?}"
    );
    // The card is still asked, and the wait is still in the trace.
    assert!(seen.iter().any(|event| matches!(
        event,
        ChatEvent::Trace(TraceEvent::WaitStarted { request_id: waited, .. })
            if *waited == request_id
    )));
    assert!(seen.iter().any(|event| matches!(
        event,
        ChatEvent::Trace(TraceEvent::WaitEnded { request_id: ended, .. })
            if *ended == request_id
    )));
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_structured_question_carries_its_prompt_and_its_options() {
    let (mut client, events) = launch("question");
    client.prompt("ask me").expect("prompt is accepted");
    let (request_id, options, question) = loop {
        if let ChatEvent::PermissionRequest {
            request_id,
            options,
            question,
            ..
        } = next_event(&events)
        {
            break (request_id, options, question.expect("a structured question"));
        }
    };
    assert_eq!(question.header, "Pick a branch");
    assert_eq!(question.prompt, "Which branch should this target?");
    // Each option says what choosing it means, when the tool input does.
    assert_eq!(
        options
            .iter()
            .map(|option| option.description.as_deref())
            .collect::<Vec<_>>(),
        [Some("The default branch"), None]
    );
    // The native transport takes a typed answer, so it offers the field
    // even though the tool input declared none.
    assert!(
        question.text_input.is_some(),
        "a native question offers free text"
    );
    client
        .respond_permission(request_id, "main")
        .expect("the answer reaches the agent");
    let seen = drain_until_turn_end(&events);
    assert!(
        seen.iter()
            .any(|event| matches!(event, ChatEvent::TurnEnded { .. }))
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn approving_a_plan_moves_the_session_out_of_plan_mode() {
    let (mut client, events) = launch("exit_plan_mode");
    client.prompt("plan it").expect("prompt is accepted");
    let request_id = loop {
        if let ChatEvent::PermissionRequest {
            request_id,
            options,
            ..
        } = next_event(&events)
        {
            assert!(
                options
                    .iter()
                    .any(|option| option.id == "approve_accept_edits")
            );
            break request_id;
        }
    };
    client
        .respond_permission(request_id, "approve_accept_edits")
        .expect("the choice reaches the agent");
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while client
        .mode_catalog()
        .is_none_or(|catalog| catalog.current_id != "acceptEdits")
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        client.mode_catalog().expect("modes").current_id,
        "acceptEdits",
        "approving a plan that still asks for every edit approves nothing"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_dry_run_rewind_reports_its_preview_and_a_real_one_reports_its_result() {
    let (mut client, _events) = launch("rewind");
    let preview = client
        .rewind_files("uuid-1", true)
        .expect("the dry run answers");
    assert!(preview.can_rewind);
    assert_eq!(preview.files_changed, vec!["/repo/a.rs".to_string()]);
    let applied = client
        .rewind_files("uuid-1", false)
        .expect("the rewind answers");
    assert!(applied.can_rewind);
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_cli_that_does_not_know_the_verb_degrades_that_feature_only() {
    // An older-but-above-floor claude, or a newer one that dropped it: the
    // affordance stops working and the session does not notice.
    let (mut client, events) = launch("no_rewind");
    let error = client
        .rewind_files("uuid-1", true)
        .expect_err("an unknown verb is an error, not a fake success");
    assert!(format!("{error:#}").contains("rewind"));
    // The session is still alive and still streaming.
    client.prompt("hello").expect("prompt is accepted");
    let seen = drain_until_turn_end(&events);
    assert!(
        seen.iter()
            .any(|event| matches!(event, ChatEvent::AgentMessageChunk(_)))
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn the_meter_prefers_the_agents_own_count_and_falls_back_to_the_turns_usage() {
    let (mut client, events) = launch("context_usage");
    client.prompt("hello").expect("prompt is accepted");
    let seen = drain_until_turn_end(&events);
    let usage = seen
        .iter()
        .rev()
        .find_map(|event| match event {
            ChatEvent::ContextUsage(usage) => Some(usage.clone()),
            _ => None,
        })
        .expect("a context usage event");
    assert_eq!(usage.used, 48_000);
    assert_eq!(usage.size, 200_000);
    assert_eq!(usage.cost.as_ref().expect("a cost").currency, "USD");
    client.shutdown().expect("clean shutdown");

    // A CLI that does not answer the request still gets a meter, from the
    // per-model usage the result already carried.
    let (mut client, events) = launch("no_context_usage");
    client.prompt("hello").expect("prompt is accepted");
    let seen = drain_until_turn_end(&events);
    let usage = seen
        .iter()
        .rev()
        .find_map(|event| match event {
            ChatEvent::ContextUsage(usage) => Some(usage.clone()),
            _ => None,
        })
        .expect("a fallback context usage event");
    assert_eq!(usage.size, 200_000, "from modelUsage.contextWindow");
    assert!(usage.used > 0);
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_resumed_launch_passes_the_session_id_to_the_cli() {
    let (mut client, _events) = ClaudeClient::launch(
        ClaudeLaunch::fixture(
            PathBuf::from("python3"),
            vec![FIXTURE.into(), "echo_argv".into()],
        )
        .resuming("sess-42"),
        std::env::temp_dir(),
    )
    .expect("handshake succeeds");
    // The `echo_argv` fixture reports its own argv in the handshake's
    // account email field, which is the only string a test can read back
    // through the public API without a new accessor.
    let argv = client.claude_version().unwrap_or_default();
    assert!(argv.contains("--resume sess-42"), "argv was {argv}");
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_fresh_session_is_named_before_the_cli_says_anything() {
    // No turn and no `init` line yet, so nothing has told Sirio an id —
    // and it still has one, because it chose it and passed it as
    // `--session-id`. A tab that dies here is still resumable.
    let (mut client, _events) = launch("normal");
    let id = client
        .session_id()
        .expect("a fresh session is named at launch");
    // Shape, not just length: the name of this test promises v4. The id
    // itself stays out of the message — it is clock and pid, not a
    // secret, but nothing is learned by printing it either.
    let shaped = id.len() == 36 && id.split('-').map(str::len).eq([8, 4, 4, 4, 12]);
    assert!(shaped, "a fresh session must be named with a v4-shaped id");
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_fresh_launch_passes_its_chosen_session_id_and_resumes_nothing() {
    let (mut client, _events) = launch("echo_argv");
    let argv = client.claude_version().unwrap_or_default();
    assert!(argv.contains("--session-id "), "argv was {argv}");
    assert!(!argv.contains("--resume"), "argv was {argv}");
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_refused_resume_starts_a_fresh_session_instead_of_a_dead_tab() {
    // The session file is gone. Losing the conversation is bad; losing the
    // tab is worse, and the transcript on screen came from Sirio's own
    // database either way.
    let (mut client, _events) = ClaudeClient::launch(
        ClaudeLaunch::fixture(
            PathBuf::from("python3"),
            vec![FIXTURE.into(), "refuse_resume".into()],
        )
        .resuming("sess-gone"),
        std::env::temp_dir(),
    )
    .expect("a refused resume must still yield a usable session");
    assert!(
        client.model_catalog().is_some(),
        "the fresh session is usable"
    );
    assert!(
        client.resumed_session_refused(),
        "the surface has to know the id it held is stale"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn the_notices_a_turn_produces_reach_the_client_in_order() {
    let (mut client, events) = launch("notices");
    client.prompt("hello").expect("prompt is accepted");
    // Drain until the turn ends, collecting notices at each signal so the
    // order they were posted in is the order they are read in.
    let mut notices = Vec::new();
    let mut task_updates = Vec::new();
    loop {
        match next_event(&events) {
            ChatEvent::OtherSessionUpdate { kind } if kind == "SessionNotice" => {
                notices.extend(client.take_notices());
            }
            ChatEvent::OtherSessionUpdate { kind }
                if kind.starts_with("BackgroundTasksUpdate") =>
            {
                task_updates.push(kind);
            }
            ChatEvent::TurnEnded { .. } => break,
            _ => {}
        }
    }
    assert_eq!(
        notices,
        vec![
            // The foreground task's notification produced nothing: its
            // `task_started` said `is_backgrounded: false`, and that flag
            // is the only place the difference is ever stated.
            SessionNotice::BackgroundTaskEnded {
                summary: "Background command \"work\" completed (exit code 0)".into()
            },
            SessionNotice::Compacted {
                trigger: Some("auto".into()),
                pre_tokens: Some(43_134),
                post_tokens: Some(11_574),
            },
            SessionNotice::RateLimited {
                status: "rejected".into(),
                window: Some("five_hour".into()),
                resets_at: Some(1_789_990_800),
                used_percent: Some(100),
                // The CLI sends a scope only with an overage denial.
                limit_scope: None,
            },
        ]
    );
    // The client exposes the latest count, which may already be zero by the
    // time these queued events are read. The events themselves retain both
    // transitions in order.
    assert_eq!(
        task_updates,
        ["BackgroundTasksUpdate(1)", "BackgroundTasksUpdate(0)"]
    );
    assert_eq!(client.background_task_count(), 0);
    // Draining is draining: a second read after the turn returns nothing.
    assert!(client.take_notices().is_empty());
}

// The rules are wrong if any of these goes red:
// - a streamed reply or the fixture's `state_text` prints twice;
// - the exhaustion sentence prints twice;
// - the re-asked answer is dropped, with or without `message_stop`;
// - a subagent's text is drawn;
// - a frame with a wrongly typed field loses the whole line, or yields a
//   notice built from garbage;
// - an empty `informational` is drawn;
// - a `no_response: null` reads as "no response".

/// What one `cli_notices` turn produced, in the order it arrived: the notices
/// at the point their signal was read, and the reply text.
#[derive(Debug, PartialEq)]
enum Seen {
    Chunk(String),
    Notices(Vec<SessionNotice>),
    /// The turn failed (`TurnFailed`) with this message instead of ending.
    Failed(String),
}

fn cli_notices_turn(
    client: &mut ClaudeClient,
    events: &sirio_chat::EventStream,
    prompt: &str,
) -> Vec<Seen> {
    client.prompt(prompt).expect("prompt is accepted");
    let mut seen = Vec::new();
    loop {
        match next_event(events) {
            ChatEvent::AgentMessageChunk(text) => seen.push(Seen::Chunk(text)),
            ChatEvent::OtherSessionUpdate { kind } if kind == "SessionNotice" => {
                seen.push(Seen::Notices(client.take_notices()));
            }
            ChatEvent::TurnEnded { .. } => return seen,
            ChatEvent::TurnFailed { message, .. } => {
                seen.push(Seen::Failed(message));
                return seen;
            }
            _ => {}
        }
    }
}

fn notices_of(seen: &[Seen]) -> Vec<SessionNotice> {
    seen.iter()
        .filter_map(|item| match item {
            Seen::Notices(notices) => Some(notices.clone()),
            Seen::Chunk(_) | Seen::Failed(_) => None,
        })
        .flatten()
        .collect()
}

fn reply_of(seen: &[Seen]) -> String {
    seen.iter()
        .filter_map(|item| match item {
            Seen::Chunk(text) => Some(text.as_str()),
            Seen::Notices(_) | Seen::Failed(_) => None,
        })
        .collect()
}

fn retrying(
    attempt: u32,
    delay: u64,
    status: Option<u16>,
    error: &str,
    no_response: bool,
) -> SessionNotice {
    SessionNotice::Retrying {
        attempt,
        max_retries: 10,
        retry_delay_ms: delay,
        error: Some(error.into()),
        error_status: status,
        no_response,
    }
}

#[test]
fn the_cli_notices_reach_the_client_in_order() {
    let (mut client, events) = launch("cli_notices");
    let mut notices = |prompt: &str| notices_of(&cli_notices_turn(&mut client, &events, prompt));

    assert_eq!(
        notices("retry twice"),
        vec![
            retrying(1, 617, Some(529), "overloaded", false),
            retrying(2, 1101, Some(529), "overloaded", false),
        ]
    );
    // The last frame has a string `attempt`: it yields nothing, and the
    // lines before it are not lost with it. A `null` status stays `None`,
    // and a `null` `no_response` is not "no response": only an object is.
    assert_eq!(
        notices("retry kinds"),
        vec![
            retrying(1, 617, Some(500), "server_error", false),
            retrying(2, 617, Some(429), "rate_limit", false),
            retrying(3, 617, None, "unknown", false),
            retrying(4, 617, None, "unknown", true),
            // The status is a string: only that field is lost.
            retrying(5, 617, None, "server_error", false),
        ]
    );
    // `timeout_ms` is a string: the notification stays, without a timeout.
    assert_eq!(
        notices("notify typed"),
        vec![SessionNotice::Notification {
            key: "typed".into(),
            text: "Typed badly".into(),
            priority: Some("low".into()),
            color: None,
            timeout_ms: None,
        }]
    );
    let got = notices("notify");
    let expected: [(&str, &str, &str, Option<&str>); 4] = [
        (
            "stop-hook-error",
            "Stop hook error occurred \u{b7} ctrl+o to see",
            "immediate",
            None,
        ),
        (
            "auto-mode-gate-plan-exit-fallback",
            "Plan mode exit fell back",
            "high",
            Some("warning"),
        ),
        (
            "lsp-server-start-failed:demo",
            "LSP server demo failed to start",
            "low",
            None,
        ),
        (
            "lsp-server-start-failed:demo",
            "LSP server demo failed again",
            "low",
            None,
        ),
    ];
    assert_eq!(got.len(), expected.len(), "one notice per notification");
    for (notice, (key, text, priority, color)) in got.iter().zip(expected) {
        assert_eq!(
            notice,
            &SessionNotice::Notification {
                key: key.into(),
                text: text.into(),
                priority: Some(priority.into()),
                color: color.map(Into::into),
                timeout_ms: Some(120_000),
            }
        );
    }

    let got = notices("refuse");
    assert_eq!(got.len(), 1);
    assert!(matches!(
        &got[0],
        SessionNotice::CliNotice { text, warning: false }
            if text.starts_with("Opus 5.5's safeguards")
    ));

    let got = notices("refuse fallback");
    assert_eq!(got.len(), 1);
    let override_mark = char::from_u32(0x202E).unwrap();
    assert!(matches!(
        &got[0],
        SessionNotice::CliNotice { text, warning: false }
            if text.contains("\n\nDetails:") && text.contains(override_mark)
    ));

    let got = notices("refuse none");
    assert_eq!(got.len(), 1);
    assert!(matches!(
        &got[0],
        SessionNotice::CliNotice { warning: false, .. }
    ));

    // The fourth frame has an empty `content` and yields nothing.
    let got = notices("inform levels");
    let lines: Vec<(&str, bool)> = got
        .iter()
        .map(|notice| match notice {
            SessionNotice::CliNotice { text, warning } => (text.as_str(), *warning),
            _ => panic!("every notice here is a CliNotice"),
        })
        .collect();
    assert_eq!(
        lines,
        [
            ("Started in the background: demo", false),
            ("Try /compact", false),
            (
                "A hook gave no usable answer, so it didn't check this action.",
                true
            ),
        ]
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn an_answer_no_stream_carried_is_drawn_once() {
    let (mut client, events) = launch("cli_notices");
    // (prompt, the reply drawn, the failure the turn ends in: `None` for a turn that ends)
    let cases: [(&str, &str, Option<&str>); 15] = [
        // First of a fresh launch: nothing to run on from.
        ("unstreamed", "ok", None),
        ("midstream", "partial-before-error\n\nok", None),
        ("midstream drop", "partial-before-error\n\nok", None),
        // The exhaustion sentence is the turn's failure, not a reply: nothing is drawn.
        ("retry out", "", Some("API Error: Repeated 529 Overloaded errors.")),
        // The error result of the turn before left no separator behind.
        ("unstreamed", "ok", None),
        ("subagent text", "ok", None),
        ("retry twice", "ok", None),
        ("retry kinds", "ok", None),
        ("notify", "ok", None),
        ("notify short", "ok", None),
        ("notify short again", "ok", None),
        ("refuse fallback", "refused-partialok", None),
        ("refuse none", "refused-partial", None),
        ("inform levels", "ok", None),
        ("anything else", "ok", None),
    ];
    for (prompt, expected, failure) in cases {
        let seen = cli_notices_turn(&mut client, &events, prompt);
        assert_eq!(
            reply_of(&seen),
            expected,
            "the reply text of one turn, drawn once"
        );
        let failed_with = seen.iter().find_map(|item| match item {
            Seen::Failed(message) => Some(message.as_str()),
            _ => None,
        });
        assert_eq!(failed_with, failure, "{prompt}: how the turn ended");
    }
    // Both halves were streamed, so no separator is added, and the notice
    // sits between them.
    let seen = cli_notices_turn(&mut client, &events, "refuse");
    assert_eq!(seen.len(), 3);
    assert_eq!(seen[0], Seen::Chunk("refused-partial".into()));
    assert!(matches!(&seen[1], Seen::Notices(notices) if notices.len() == 1));
    assert_eq!(seen[2], Seen::Chunk("ok".into()));
    client.shutdown().expect("clean shutdown");
}

/// Everything the stream carries up to and including the first re-read signal
/// named `kind`, or `None` when none arrives inside `budget`.
fn events_through_signal(
    events: &sirio_chat::EventStream,
    kind: &str,
    budget: Duration,
) -> Option<Vec<ChatEvent>> {
    let deadline = std::time::Instant::now() + budget;
    let mut seen = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        let receive = events.recv();
        let timer = async_io::Timer::after(remaining);
        let event = futures::executor::block_on(async move {
            futures::pin_mut!(receive, timer);
            match futures::future::select(receive, timer).await {
                Either::Left((event, _)) => event.ok(),
                Either::Right((_, _)) => None,
            }
        })?;
        let hit = matches!(&event, ChatEvent::OtherSessionUpdate { kind: got } if got == kind);
        seen.push(event);
        if hit {
            return Some(seen);
        }
    }
}

#[test]
fn a_turn_leaves_its_context_breakdown_behind() {
    let (mut client, events) = launch("session_panel");
    assert!(client.context_breakdown().is_none(), "nothing before a turn");
    client.prompt("hello").expect("prompt is accepted");
    let seen = drain_until_turn_end(&events);

    let breakdown = client
        .context_breakdown()
        .expect("the turn's answer is kept");
    assert_eq!(breakdown.origins().reducible[0].key, "memory");

    // The signal comes before the turn is over, so a panel reading on
    // `TurnEnded` never sees the previous turn's numbers.
    let signal = seen
        .iter()
        .position(|event| {
            matches!(event, ChatEvent::OtherSessionUpdate { kind } if kind == "ContextBreakdown")
        })
        .expect("a ContextBreakdown signal");
    let ended = seen
        .iter()
        .position(|event| matches!(event, ChatEvent::TurnEnded { .. }))
        .expect("the turn ended");
    assert!(signal < ended, "signal at {signal}, turn ended at {ended}");
    client.shutdown().expect("clean shutdown");
}

#[test]
fn the_breakdown_can_be_asked_for_before_any_turn() {
    let (mut client, events) = launch("session_panel");
    client.refresh_context().expect("the request is written");
    let seen = events_through_signal(&events, "ContextBreakdown", Duration::from_secs(5))
        .expect("a ContextBreakdown signal inside five seconds");
    assert!(client.context_breakdown().is_some());
    assert!(
        !seen
            .iter()
            .any(|event| matches!(event, ChatEvent::TurnEnded { .. })),
        "asking is not a turn"
    );
    // The meter moves too: the window is named by the same answer.
    let usage = seen
        .iter()
        .find_map(|event| match event {
            ChatEvent::ContextUsage(usage) => Some(usage.clone()),
            _ => None,
        })
        .expect("the answer's window reaches the meter");
    assert_eq!((usage.used, usage.size), (64_923, 400_000));
    client.shutdown().expect("clean shutdown");
}

#[test]
fn init_names_the_servers_until_a_status_answers() {
    use sirio_claude::{McpOrigin, McpServer, McpState};

    let (mut client, _events) = launch("session_panel");
    assert_eq!(
        client.mcp_servers(),
        Some(vec![McpServer {
            name: "codegraph".into(),
            status: McpState::Pending,
            origin: McpOrigin::Other(String::new()),
            tools: Vec::new(),
        }])
    );

    let answered = client.refresh_mcp_status().expect("the CLI answers");
    assert_eq!(answered.len(), 2);
    assert_eq!(answered[0].origin, McpOrigin::ClaudeAi);
    assert_eq!(answered[1].origin, McpOrigin::User);
    assert_eq!(client.mcp_servers(), Some(answered));
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_status_answer_is_not_overwritten_by_a_later_init() {
    use sirio_claude::McpOrigin;

    let (mut client, events) = launch("session_panel");
    client.refresh_mcp_status().expect("the CLI answers");
    // A turn makes the CLI print init again, with its unsourced list.
    client.prompt("hello").expect("prompt is accepted");
    drain_until_turn_end(&events);
    let servers = client.mcp_servers().expect("a list");
    assert_eq!(servers.len(), 2, "the status list stands: {servers:?}");
    assert_eq!(servers[0].origin, McpOrigin::ClaudeAi);
    client.shutdown().expect("clean shutdown");
}

#[test]
fn every_init_announces_itself_apart_from_a_status_answer() {
    let (mut client, events) = launch("session_panel");
    client.refresh_mcp_status().expect("the CLI answers");
    client.prompt("hello").expect("prompt is accepted");
    let seen = drain_until_turn_end(&events);
    let names: Vec<&str> = seen
        .iter()
        .filter_map(|event| match event {
            ChatEvent::OtherSessionUpdate { kind } if kind.starts_with("McpServers") => {
                Some(kind.as_str())
            }
            _ => None,
        })
        .collect();
    // The first init, the answer, and a turn's init that the held answer
    // does not hide: a panel re-asks on the init and never on its own answer.
    assert_eq!(names, ["McpServersInit", "McpServers", "McpServersInit"]);
    client.shutdown().expect("clean shutdown");
}

#[test]
fn reconnect_reaches_the_cli_and_a_refusal_carries_its_reason() {
    use sirio_claude::McpState;

    let (mut client, _events) = launch("session_panel");
    client.mcp_reconnect("codegraph").expect("the CLI agrees");
    let servers = client.refresh_mcp_status().expect("the CLI answers");
    let codegraph = servers
        .iter()
        .find(|server| server.name == "codegraph")
        .expect("codegraph is listed");
    assert_eq!(codegraph.status, McpState::Connected);

    let error = client
        .mcp_reconnect("nope")
        .expect_err("an unknown server is refused");
    assert!(
        format!("{error:#}").contains("Server not found: nope"),
        "the CLI's own words reach the caller: {error:#}"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn sign_in_returns_the_url_to_open() {
    let (mut client, _events) = launch("session_panel");
    let start = client
        .mcp_authenticate("claude.ai Notion")
        .expect("the CLI starts the sign-in");
    assert_eq!(start.auth_url.as_deref(), Some("http://127.0.0.1:9/authorize"));
    assert!(start.requires_user_action);
    client.shutdown().expect("clean shutdown");
}

// ---------------------------------------------------------------------------
// MCP elicitation, withdrawal and refusal.
//
// Ways this can fail:
//   - an answer reaches the CLI under the wrong id, with the wrong action, or
//     with content a URL request must not carry;
//   - a form the build cannot draw is shown as a form (or not shown at all)
//     instead of being offered a decline;
//   - a withdrawn request is answered anyway, or its card is never closed;
//   - a withdrawn permission is answered, or closed as the wrong kind;
//   - a request the worker does not handle is left unanswered, so the CLI
//     waits for ever, or is answered under another id;
//   - two open requests cross their answers, or the turn ends on the first;
//   - an unanswered elicitation trips the prompt watchdog before the
//     permission clock, or expires without telling the CLI it was cancelled;
//   - a server's completion notice is lost.
// ---------------------------------------------------------------------------

static LOG_SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn response_log_path() -> PathBuf {
    let n = LOG_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("sirio-elicitation-{}-{n}.log", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

fn elicitation_launch(log: &std::path::Path) -> ClaudeLaunch {
    ClaudeLaunch::fixture(
        PathBuf::from("python3"),
        vec![
            FIXTURE.into(),
            "elicitation".into(),
            format!("--fixture-response-log={}", log.display()),
        ],
    )
}

fn launch_elicitation(log: &std::path::Path) -> (ClaudeClient, sirio_chat::EventStream) {
    ClaudeClient::launch(elicitation_launch(log), std::env::temp_dir())
        .expect("fixture claude should complete its handshake")
}

fn log_lines(log: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// The `response` bodies the host sent for fixture-originated ids, with the id.
fn logged_responses(log: &std::path::Path) -> Vec<(String, serde_json::Value)> {
    log_lines(log)
        .into_iter()
        .filter_map(|line| {
            let response = line.get("response")?;
            let id = response.get("request_id")?.as_str()?.to_string();
            let body = response.get("response").cloned().or_else(|| {
                response
                    .get("error")
                    .map(|e| serde_json::json!({"error": e}))
            })?;
            Some((id, body))
        })
        .collect()
}

fn wait_for_log(log: &std::path::Path, want: impl Fn(&[serde_json::Value]) -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if want(&log_lines(log)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!(
        "the response log never held what the test waits for: {:?}",
        log_lines(log)
    );
}

/// Whether a `TurnEnded` arrives within `budget`, skipping other events.
fn first_turn_end_within(events: &sirio_chat::EventStream, budget: Duration) -> Option<ChatEvent> {
    let deadline = std::time::Instant::now() + budget;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        let receive = events.recv();
        let timer = async_io::Timer::after(remaining);
        let got = futures::executor::block_on(async move {
            futures::pin_mut!(receive, timer);
            match futures::future::select(receive, timer).await {
                Either::Left((event, _)) => event.ok(),
                Either::Right((_, _)) => None,
            }
        });
        match got {
            Some(event @ ChatEvent::TurnEnded { .. }) => return Some(event),
            Some(_) => continue,
            None => return None,
        }
    }
}

fn next_elicitation(events: &sirio_chat::EventStream) -> (u64, String, ElicitationPrompt) {
    loop {
        if let ChatEvent::Elicitation {
            request_id,
            server,
            prompt,
            ..
        } = next_event(events)
        {
            return (request_id, server, prompt);
        }
    }
}

#[test]
fn an_elicitation_form_is_asked_and_its_answer_reaches_the_cli() {
    let log = response_log_path();
    let (client, events) = launch_elicitation(&log);
    client.prompt("form").expect("prompt is accepted");
    let (request_id, server, prompt) = next_elicitation(&events);
    assert_eq!(server, "elic");
    let ElicitationPrompt::Form(fields) = prompt else {
        panic!("a form request must be drawn as a form, got {prompt:?}");
    };
    let names: Vec<_> = fields.iter().map(|field| field.name.as_str()).collect();
    assert_eq!(names, ["colour", "count", "note", "ok"]);
    let mut content = serde_json::Map::new();
    content.insert("colour".into(), "blue".into());
    content.insert("count".into(), 2.into());
    client
        .respond_elicitation(request_id, ElicitationAction::Accept(content))
        .expect("the answer reaches the agent");
    let seen = drain_until_turn_end(&events);
    assert!(matches!(seen.last(), Some(ChatEvent::TurnEnded { .. })));
    assert_eq!(
        logged_responses(&log),
        [(
            "elic-form".to_string(),
            serde_json::json!({"action": "accept", "content": {"colour": "blue", "count": 2}})
        )]
    );
}

#[test]
fn a_url_elicitation_is_accepted_without_content_and_completes() {
    let log = response_log_path();
    let (client, events) = launch_elicitation(&log);
    client.prompt("url").expect("prompt is accepted");
    let (request_id, _server, prompt) = next_elicitation(&events);
    assert!(
        matches!(&prompt, ElicitationPrompt::Url { url, elicitation_id }
            if url == "https://example.com/auth?x=1" && elicitation_id.as_deref() == Some("el-2")),
        "got {prompt:?}"
    );
    client
        .respond_elicitation(
            request_id,
            ElicitationAction::Accept(serde_json::Map::new()),
        )
        .expect("the answer reaches the agent");
    let seen = drain_until_turn_end(&events);
    assert!(
        seen.iter().any(|event| matches!(event,
            ChatEvent::ElicitationCompleted { server, elicitation_id }
                if server == "elic" && elicitation_id == "el-2")),
        "the server's completion notice must reach the surface: {seen:?}"
    );
    assert_eq!(
        logged_responses(&log),
        [(
            "elic-url".to_string(),
            serde_json::json!({"action": "accept"})
        )]
    );
}

#[test]
fn an_unrenderable_form_is_unsupported_and_can_be_declined() {
    let log = response_log_path();
    let (client, events) = launch_elicitation(&log);
    client.prompt("bad schema").expect("prompt is accepted");
    let (request_id, _server, prompt) = next_elicitation(&events);
    assert!(
        matches!(prompt, ElicitationPrompt::Unsupported { .. }),
        "a nested property cannot be a form: {prompt:?}"
    );
    client
        .respond_elicitation(request_id, ElicitationAction::Decline)
        .expect("the answer reaches the agent");
    drain_until_turn_end(&events);
    assert_eq!(
        logged_responses(&log),
        [(
            "elic-bad".to_string(),
            serde_json::json!({"action": "decline"})
        )]
    );
}

#[test]
fn a_withdrawn_elicitation_is_not_answered() {
    let log = response_log_path();
    let (client, events) = launch_elicitation(&log);
    client.prompt("withdraw").expect("prompt is accepted");
    let (request_id, _server, _prompt) = next_elicitation(&events);
    loop {
        if let ChatEvent::RequestClosed {
            request_id: closed,
            kind,
            reason,
        } = next_event(&events)
        {
            assert_eq!(closed, request_id);
            assert_eq!(kind, RequestKind::Elicitation);
            assert_eq!(reason, CloseReason::Withdrawn);
            break;
        }
    }
    drain_until_turn_end(&events);
    client
        .respond_elicitation(
            request_id,
            ElicitationAction::Accept(serde_json::Map::new()),
        )
        .expect("the worker is still there");
    client.prompt("form").expect("prompt is accepted");
    next_elicitation(&events);
    wait_for_log(&log, |lines| {
        lines.iter().any(|line| line["prompt"] == "form")
    });
    assert_eq!(
        logged_responses(&log),
        [],
        "an answer to a withdrawn request must not be sent"
    );
}

#[test]
fn a_withdrawn_permission_closes_and_is_not_answered() {
    let log = response_log_path();
    let (client, events) = launch_elicitation(&log);
    client
        .prompt("withdraw permission")
        .expect("prompt is accepted");
    let request_id = loop {
        if let ChatEvent::PermissionRequest { request_id, .. } = next_event(&events) {
            break request_id;
        }
    };
    loop {
        if let ChatEvent::RequestClosed {
            request_id: closed,
            kind,
            reason,
        } = next_event(&events)
        {
            assert_eq!(closed, request_id);
            assert_eq!(kind, RequestKind::Permission);
            assert_eq!(reason, CloseReason::Withdrawn);
            break;
        }
    }
    drain_until_turn_end(&events);
    client
        .respond_permission(request_id, "allow_once")
        .expect("the worker is still there");
    client.prompt("form").expect("prompt is accepted");
    next_elicitation(&events);
    wait_for_log(&log, |lines| {
        lines.iter().any(|line| line["prompt"] == "form")
    });
    assert_eq!(
        logged_responses(&log),
        [],
        "an answer to a withdrawn card must not be sent"
    );
}

#[test]
fn an_unhandled_request_is_refused_by_name() {
    let log = response_log_path();
    let (client, events) = launch_elicitation(&log);
    client.prompt("unknown").expect("prompt is accepted");
    drain_until_turn_end(&events);
    let responses = logged_responses(&log);
    assert_eq!(responses.len(), 1, "{responses:?}");
    assert_eq!(responses[0].0, "u1");
    let error = responses[0].1["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("future_thing"),
        "the refusal names the request: {error}"
    );
}

#[test]
fn a_request_sent_before_the_handshake_is_answered_is_refused() {
    let log = response_log_path();
    let launch = ClaudeLaunch::fixture(
        PathBuf::from("python3"),
        vec![
            FIXTURE.into(),
            "early_request".into(),
            format!("--fixture-response-log={}", log.display()),
        ],
    );
    let (_client, events) = ClaudeClient::launch(launch, std::env::temp_dir())
        .expect("fixture claude should complete its handshake");
    wait_for_log(&log, |lines| lines.len() >= 2);
    let responses = logged_responses(&log);
    for id in ["early-e", "early-p"] {
        let error = responses
            .iter()
            .find(|(response_id, _)| response_id == id)
            .and_then(|(_, body)| body["error"].as_str())
            .unwrap_or_else(|| panic!("{id} was not refused: {responses:?}"));
        assert!(error.contains("not ready"), "{id}: {error}");
    }
    assert!(
        !std::iter::from_fn(|| events.try_recv().ok()).any(|event| matches!(
            event,
            ChatEvent::Elicitation { .. } | ChatEvent::PermissionRequest { .. }
        )),
        "a refused request is never shown"
    );
}

#[test]
fn two_open_requests_are_answered_independently() {
    let log = response_log_path();
    let (client, events) = launch_elicitation(&log);
    client.prompt("both").expect("prompt is accepted");
    let mut permission = None;
    let mut elicitation = None;
    while permission.is_none() || elicitation.is_none() {
        match next_event(&events) {
            ChatEvent::PermissionRequest { request_id, .. } => permission = Some(request_id),
            ChatEvent::Elicitation { request_id, .. } => elicitation = Some(request_id),
            _ => {}
        }
    }
    let (permission, elicitation) = (permission.unwrap(), elicitation.unwrap());
    assert_ne!(permission, elicitation);
    client
        .respond_elicitation(elicitation, ElicitationAction::Decline)
        .expect("the answer reaches the agent");
    wait_for_log(&log, |lines| {
        lines.iter().any(|l| l.get("response").is_some())
    });
    // The permission is still open, so the turn must not end yet.
    assert!(
        first_turn_end_within(&events, Duration::from_millis(800)).is_none(),
        "the turn ended with a request still open"
    );
    client
        .respond_permission(permission, "allow_once")
        .expect("the choice reaches the agent");
    drain_until_turn_end(&events);
    let responses = logged_responses(&log);
    let ids: Vec<_> = responses.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        ["elic-both", "cli-perm-both"],
        "answers keep their order and ids"
    );
    assert_eq!(responses[0].1["action"], "decline");
    assert_eq!(responses[1].1["behavior"], "allow");
}

#[test]
fn an_unanswered_elicitation_expires_as_cancel_and_holds_off_the_watchdog() {
    let log = response_log_path();
    let (client, events) = ClaudeClient::launch_with_timeouts(
        elicitation_launch(&log),
        std::env::temp_dir(),
        Duration::from_secs(10),
        Duration::from_millis(400),
        Duration::from_secs(3),
    )
    .expect("handshake succeeds");
    client.prompt("silent form").expect("prompt is accepted");
    let (request_id, _server, _prompt) = next_elicitation(&events);
    let started = std::time::Instant::now();
    loop {
        match next_event(&events) {
            ChatEvent::Timeout { .. } => {
                panic!("the prompt watchdog fired while an elicitation was open")
            }
            ChatEvent::RequestClosed {
                request_id: closed,
                kind,
                reason,
            } => {
                assert_eq!(closed, request_id);
                assert_eq!(kind, RequestKind::Elicitation);
                assert_eq!(reason, CloseReason::Expired);
                break;
            }
            _ => {}
        }
    }
    assert!(
        started.elapsed() >= Duration::from_secs(2),
        "expired too early"
    );
    wait_for_log(&log, |lines| {
        lines.iter().any(|l| l.get("response").is_some())
    });
    assert_eq!(
        logged_responses(&log),
        [(
            "elic-silent".to_string(),
            serde_json::json!({"action": "cancel"})
        )]
    );
}

fn gated_session_state_launch(gate: &std::path::Path) -> ClaudeLaunch {
    ClaudeLaunch::fixture(
        PathBuf::from("python3"),
        vec![
            FIXTURE.into(),
            "session_state".into(),
            format!("--fixture-gate={}", gate.display()),
        ],
    )
}

// Failure list: see `message.rs` (the variable, serde, another shape) and
// `events.rs` (an unknown state). This one is the end to end: the fixture
// sends states only when the launch sets the variable, and the idle that
// follows the last result must not be lost behind the first `TurnEnded`.
#[test]
fn the_cli_session_state_reaches_the_client_across_a_background_agent() {
    let dir = std::env::temp_dir().join(format!("sirio-state-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let gate = dir.join("gate");
    let _ = std::fs::remove_file(&gate);
    let _ = std::fs::remove_file(dir.join("gate.2"));
    let (client, events) = ClaudeClient::launch(gated_session_state_launch(&gate), dir.clone())
        .expect("fixture claude should complete its handshake");
    client.prompt("background").expect("prompt is accepted");

    let mut states = Vec::new();
    loop {
        match next_event(&events) {
            ChatEvent::CliState { state } => states.push(state),
            ChatEvent::TurnEnded { .. } => break,
            _ => {}
        }
    }
    assert_eq!(states, [CliState::Running], "at the main result");

    std::fs::write(&gate, "").expect("open the first gate");
    std::fs::write(dir.join("gate.2"), "").expect("open the second gate");
    let mut seen_tasks_drained = false;
    let mut second_turn_ended = false;
    loop {
        match next_event(&events) {
            ChatEvent::OtherSessionUpdate { kind } if kind == "BackgroundTasksUpdate(0)" => {
                seen_tasks_drained = true;
            }
            ChatEvent::TurnEnded { .. } => second_turn_ended = true,
            ChatEvent::CliState { state } => {
                states.push(state);
                if state == CliState::Idle {
                    assert!(seen_tasks_drained, "idle came before the tasks drained");
                    break;
                }
            }
            _ => {}
        }
    }
    assert_eq!(states, [CliState::Running, CliState::Idle]);
    while !second_turn_ended {
        if matches!(next_event(&events), ChatEvent::TurnEnded { .. }) {
            second_turn_ended = true;
        }
    }
    drop(client);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Background tasks: `get_task_output`, `stop_task` and the events over the
// fixture's `background_tasks` mode. Every test launches its own fixture and
// its own request log, so they run side by side.
// ---------------------------------------------------------------------------

const SHELL: &str = "bshell001";

fn background_launch(tag: &str, floor: bool) -> (ClaudeLaunch, PathBuf) {
    let log = std::env::temp_dir().join(format!("sirio-bg-{tag}-{}.requests", std::process::id()));
    let _ = std::fs::remove_file(&log);
    // A gate a failed earlier run left open would answer `slow` at once.
    let _ = std::fs::remove_file(gate_path(&log));
    let mut args = vec![
        FIXTURE.to_string(),
        "background_tasks".to_string(),
        format!("--fixture-request-log={}", log.display()),
        format!("--fixture-gate={}", gate_path(&log).display()),
    ];
    if floor {
        args.push("--fixture-floor".to_string());
    }
    if tag == "events" {
        // Rows with no `task_id` ride along; the events must not name them.
        args.push("--fixture-rowless".to_string());
    }
    (ClaudeLaunch::fixture(PathBuf::from("python3"), args), log)
}

/// The file `slow` holds its `get_task_output` answers for, until it exists.
fn gate_path(log: &std::path::Path) -> PathBuf {
    PathBuf::from(format!("{}.gate", log.display()))
}

/// The control requests the fixture logged for one verb, as it received them.
fn logged_requests(log: &std::path::Path, subtype: &str) -> Vec<serde_json::Value> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| message["request"]["subtype"] == subtype)
        .collect()
}

/// The next event within `budget`, or none: a test that must fail rather
/// than hang reads through this, not through `next_event`.
fn next_event_within(events: &sirio_chat::EventStream, budget: Duration) -> Option<ChatEvent> {
    let receive = events.recv();
    let timer = async_io::Timer::after(budget);
    futures::executor::block_on(async move {
        futures::pin_mut!(receive, timer);
        match futures::future::select(receive, timer).await {
            Either::Left((event, _)) => event.ok(),
            Either::Right((_, _)) => None,
        }
    })
}

/// Every event up to and including the first one `stop` accepts.
fn events_until(
    events: &sirio_chat::EventStream,
    stop: impl Fn(&ChatEvent) -> bool,
) -> Vec<ChatEvent> {
    let mut seen = Vec::new();
    loop {
        let event = next_event(events);
        let done = stop(&event);
        seen.push(event);
        if done {
            return seen;
        }
    }
}

fn ended(event: &ChatEvent, wanted: &str) -> bool {
    matches!(
        event,
        ChatEvent::BackgroundTask(BackgroundTaskEvent::Ended { status, .. }) if status == wanted
    )
}

/// Reads until `done` accepts the answer, or fails after `budget`.
fn read_output_until(
    control: &sirio_chat::TaskControl,
    budget: Duration,
    done: impl Fn(&sirio_chat::TaskOutput) -> bool,
) -> sirio_chat::TaskOutput {
    let deadline = std::time::Instant::now() + budget;
    loop {
        let answer = control.output(SHELL).expect("the CLI answers a read");
        if done(&answer) {
            return answer;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the output never reached the awaited state: {answer:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

// Failure list for the shell's output and its stop (E2E, written first):
//  - the output is read once and never again, so it cannot grow: the second
//    read must be longer than the first, with its ANSI intact;
//  - a stop that does not reach the CLI, or reaches it as a turn's cancel:
//    the task must end, and the log must hold one `stop_task` for its id and
//    no `interrupt`;
//  - the end not showing in the output: the `[killed]` trailer is the CLI's
//    word that the process is gone.
#[test]
fn a_shell_tasks_output_grows_and_its_stop_ends_it() {
    let (launch, log) = background_launch("grows", false);
    let (client, events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("handshake succeeds");
    let control = client.task_control();
    client.prompt("shell go").expect("prompt is accepted");
    drain_until_turn_end(&events);

    let first = read_output_until(&control, Duration::from_secs(10), |out| out.total_bytes > 0);
    let grown = read_output_until(&control, Duration::from_secs(10), |out| {
        out.total_bytes > first.total_bytes
    });
    assert!(
        grown.output.starts_with(&first.output),
        "an output only grows: {first:?} then {grown:?}"
    );
    assert!(
        grown.output.contains("\u{1b}[32mline 1\u{1b}[0m\n"),
        "the CLI's own escapes reach the caller as written: {grown:?}"
    );

    control.stop(SHELL);
    events_until(&events, |event| ended(event, "killed"));
    let last = read_output_until(&control, Duration::from_secs(10), |out| {
        out.output.ends_with("\n[killed]\n")
    });
    assert!(!last.truncated);

    let stops = logged_requests(&log, "stop_task");
    assert_eq!(stops.len(), 1, "one press, one request: {stops:?}");
    assert!(
        logged_requests(&log, "interrupt").is_empty(),
        "a stop is not the turn's cancel"
    );
    drop(client);
    let _ = std::fs::remove_file(&log);
}

// Failure list for the floor (E2E, written first):
//  - the floor's refusal comes back as something `is_refusal` does not know
//    (a wrapped or reworded sentence), so the feature is never retired;
//  - the refusal takes the stop down with it: `stop_task` works on the floor
//    and must still end the task.
#[test]
fn the_floor_refuses_task_output_as_a_refusal_and_still_stops() {
    let (launch, log) = background_launch("floor", true);
    let (client, events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("handshake succeeds");
    let control = client.task_control();
    client.prompt("shell go").expect("prompt is accepted");
    drain_until_turn_end(&events);

    let error = control
        .output(SHELL)
        .expect_err("the floor has no get_task_output");
    assert!(
        sirio_chat::is_refusal(&error),
        "not read as a refusal: {error:?}"
    );

    control.stop(SHELL);
    // Returns only on the end: a stop the floor did not carry out would wait
    // out the event guard instead.
    events_until(&events, |event| ended(event, "killed"));
    drop(client);
    let _ = std::fs::remove_file(&log);
}

// Failure list for the events (E2E, written first):
//  - the rows and the start in the wrong order, or the start lost because the
//    rows already named the task;
//  - the start without the tool call that made it (`toolu_s1`);
//  - a stop that ends the task once, when the CLI says `killed` and then
//    `stopped`: both are `Ended`, the first one being the outcome;
//  - the drain `background_tasks_changed []` not reaching the caller;
//  - a row with no `task_id` reaching the registry as a task keyed `""`.
#[test]
fn background_task_events_follow_the_cli_lines() {
    let (launch, log) = background_launch("events", false);
    let (client, events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("handshake succeeds");
    let control = client.task_control();
    client.prompt("shell go").expect("prompt is accepted");
    let mut tasks: Vec<BackgroundTaskEvent> = Vec::new();
    let mut collect = |seen: Vec<ChatEvent>| {
        tasks.extend(seen.into_iter().filter_map(|event| match event {
            ChatEvent::BackgroundTask(task) => Some(task),
            _ => None,
        }));
    };
    collect(drain_until_turn_end(&events));
    control.stop(SHELL);
    collect(events_until(&events, |event| {
        matches!(
            event,
            ChatEvent::BackgroundTask(BackgroundTaskEvent::Rows(rows)) if rows.is_empty()
        )
    }));
    assert_eq!(
        tasks,
        vec![
            BackgroundTaskEvent::Rows(vec![BackgroundTaskRow {
                task_id: SHELL.into(),
                task_type: Some("local_bash".into()),
                description: "shell40".into(),
            }]),
            BackgroundTaskEvent::Started {
                task_id: SHELL.into(),
                tool_use_id: Some("toolu_s1".into()),
                task_type: Some("local_bash".into()),
                description: "shell40".into(),
            },
            BackgroundTaskEvent::Ended {
                task_id: SHELL.into(),
                status: "killed".into(),
            },
            BackgroundTaskEvent::Ended {
                task_id: SHELL.into(),
                status: "stopped".into(),
            },
            BackgroundTaskEvent::Rows(vec![]),
        ]
    );
    drop(client);
    let _ = std::fs::remove_file(&log);
}

// Failure list for the pending read (E2E, written first):
//  - a second read of a task whose first read is unanswered is written too,
//    so a CLI that stops answering costs a stdin write per tick: it must
//    answer `busy` at once and write nothing;
//  - `busy` only after the first is answered: once it is, the task must be
//    readable again, and that read must be written.
#[test]
fn a_second_read_of_a_pending_task_is_busy_and_not_written() {
    let (launch, log) = background_launch("busy", false);
    let (client, events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("handshake succeeds");
    let control = client.task_control();
    client.prompt("slow go").expect("prompt is accepted");
    drain_until_turn_end(&events);

    // `slow` holds every read's answer until the gate file exists, so the first
    // read stays pending for exactly as long as this test wants, load or not.
    let slow = control.clone();
    let first = std::thread::spawn(move || slow.output(SHELL));
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while logged_requests(&log, "get_task_output").is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "the first read never left"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(control.output(SHELL), Err("busy".to_string()));
    assert_eq!(
        logged_requests(&log, "get_task_output").len(),
        1,
        "a busy answer writes nothing"
    );
    std::fs::write(gate_path(&log), "").expect("open the gate");
    first
        .join()
        .expect("the first read does not panic")
        .expect("the first read is answered");
    control
        .output(SHELL)
        .expect("an answered task can be read again");
    assert_eq!(logged_requests(&log, "get_task_output").len(), 2);
    drop(client);
    let _ = std::fs::remove_file(&log);
    let _ = std::fs::remove_file(gate_path(&log));
}

// Failure list for the watchdog (E2E, written first; R4):
//  - the answers to Sirio's own polls count as the CLI's activity, so a 1 Hz
//    poll holds off the prompt timeout of a turn that hung on the model: the
//    timeout must still fire while the tail is being polled;
//  - the polls stop being written or answered (a regression in the `busy`
//    branch, or `hold` no longer answering): nothing touches the clock then,
//    the timeout fires, and the test would pass without the exclusion. The
//    answers are counted and must number at least three before it fires.
// Not covered here: that the exclusion stays narrow (an unparseable line and
// every other line still touch the clock). That needs a fixture that emits
// such a line on a schedule inside a hold, which `hold` does not.
// The window is the shortest the other timeout tests use (400 ms) and the poll
// gap an eighth of it, so a machine that is slow cannot make the poll
// sparser than the window and let this pass without the exclusion. The wait is
// three windows, so that without the exclusion it fails rather than hangs.
#[test]
fn task_output_answers_do_not_hold_off_the_prompt_timeout() {
    let window = Duration::from_millis(400);
    let (launch, log) = background_launch("hold", false);
    let (client, events) = ClaudeClient::launch_with_timeouts(
        launch,
        std::env::temp_dir(),
        Duration::from_secs(10),
        window,
        Duration::from_secs(60),
    )
    .expect("handshake succeeds");
    let control = client.task_control();
    client.prompt("hold go").expect("prompt is accepted");
    loop {
        let event = next_event(&events);
        if matches!(
            event,
            ChatEvent::BackgroundTask(BackgroundTaskEvent::Started { .. })
        ) {
            break;
        }
        assert!(
            !matches!(event, ChatEvent::Timeout { .. }),
            "the window ran out before the task started: a slow machine, not the watchdog"
        );
    }

    let polling = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let polls = {
        let polling = std::sync::Arc::clone(&polling);
        std::thread::spawn(move || {
            let mut answered = 0;
            while polling.load(std::sync::atomic::Ordering::Relaxed) {
                if control.output(SHELL).is_ok() {
                    answered += 1;
                }
                std::thread::sleep(window / 8);
            }
            answered
        })
    };
    let deadline = std::time::Instant::now() + window * 3;
    let mut timed_out = false;
    while let Some(budget) = deadline
        .checked_duration_since(std::time::Instant::now())
        .filter(|budget| !budget.is_zero())
    {
        if let Some(ChatEvent::Timeout {
            operation: sirio_chat::TimeoutOperation::Prompt,
            ..
        }) = next_event_within(&events, budget)
        {
            timed_out = true;
            break;
        }
    }
    polling.store(false, std::sync::atomic::Ordering::Relaxed);
    let answered = polls.join().expect("the poll thread does not panic");
    assert!(
        answered >= 3,
        "too few polls were answered to prove anything: {answered}"
    );
    assert!(
        timed_out,
        "the prompt timeout must fire while the tail is polled ({answered} reads answered)"
    );
    drop(client);
    let _ = std::fs::remove_file(&log);
}

// ---------------------------------------------------------------------------
// `rename_session`: what a host rename puts on the wire and how the answer
// comes back.
//
// Ways this can fail, written before the code:
//  - the field is not called `title`, so the CLI renames nothing;
//  - `source` is missing, so the CLI treats the rename as its own;
//  - `session_id` is the UI's id, the launch id after `init` renamed the
//    session, or a dead resume id, and a current CLI refuses it;
//  - the answer is never reported;
//  - an error is reported as success;
//  - the fixture's default `success {}` hides a missing echo, so the
//    fixture answers a rename only after it printed the echo itself, and the
//    first test asserts the echo arrived before the answer;
//  - the fold turns the CLI's echo into the answer, so Sirio would report the
//    CLI's sanitised title instead of the one it sent (one-way rule).
// ---------------------------------------------------------------------------

/// A fixture launch that logs its control requests and its launches.
fn rename_launch(tag: &str, mode: &str, extra: &[String]) -> (ClaudeLaunch, PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("sirio-rename-{tag}-{}", std::process::id()));
    let control = PathBuf::from(format!("{}.control", base.display()));
    let launches = PathBuf::from(format!("{}.launch", base.display()));
    let _ = std::fs::remove_file(&control);
    let _ = std::fs::remove_file(&launches);
    let mut args = vec![
        FIXTURE.to_string(),
        mode.to_string(),
        format!("--fixture-control-log={}", control.display()),
        format!("--fixture-launch-log={}", launches.display()),
    ];
    args.extend(extra.iter().cloned());
    (
        ClaudeLaunch::fixture(PathBuf::from("python3"), args),
        control,
        launches,
    )
}

/// The lines of a JSON-lines log. The fixture escapes non-ASCII, so a
/// substring test would miss it: parse.
fn json_lines(log: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn renames_in(control: &std::path::Path) -> Vec<serde_json::Value> {
    json_lines(control)
        .into_iter()
        .filter(|request| request["subtype"] == "rename_session")
        .collect()
}

fn next_rename_answer(events: &sirio_chat::EventStream) -> (String, Option<String>) {
    loop {
        if let ChatEvent::SessionRenamed { title, error } = next_event(events) {
            return (title, error);
        }
    }
}

#[test]
fn rename_session_writes_title_source_and_the_launch_session_id() {
    let (launch, control, launches) = rename_launch("wire", "normal", &[]);
    let (mut client, events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("fixture handshake");
    client
        .rename_session("Café ☕ notes".to_string())
        .expect("the worker is running");
    let seen = events_until(&events, |event| {
        matches!(event, ChatEvent::SessionRenamed { .. })
    });
    let Some(ChatEvent::SessionRenamed { title, error }) = seen.last() else {
        panic!("the last event is the answer: {seen:?}");
    };
    assert_eq!(title, "Café ☕ notes");
    assert_eq!(*error, None);
    assert!(
        seen.iter().any(|event| matches!(
            event,
            ChatEvent::OtherSessionUpdate { kind } if kind == "system/session_title_changed"
        )),
        "the CLI's echo must arrive before the answer: {seen:?}"
    );

    let renames = renames_in(&control);
    assert_eq!(renames.len(), 1, "{renames:?}");
    assert_eq!(renames[0]["title"], "Café ☕ notes");
    assert_eq!(renames[0]["source"], "host");
    let launched = json_lines(&launches);
    assert_eq!(launched.len(), 1, "{launched:?}");
    assert!(launched[0]["session_id"].is_string(), "{launched:?}");
    assert_eq!(renames[0]["session_id"], launched[0]["session_id"]);
    client.shutdown().expect("clean shutdown");
    let _ = std::fs::remove_file(&control);
    let _ = std::fs::remove_file(&launches);
}

#[test]
fn the_answer_carries_the_title_sirio_sent_not_the_cli_echo() {
    let (launch, control, launches) = rename_launch("raw", "normal", &[]);
    let (mut client, events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("fixture handshake");
    client
        .rename_session("  Tab\there  ".to_string())
        .expect("the worker is running");
    let (title, error) = next_rename_answer(&events);
    assert_eq!(title, "  Tab\there  ", "the raw string, not the sanitised echo");
    assert_eq!(error, None);
    client.shutdown().expect("clean shutdown");
    let _ = std::fs::remove_file(&control);
    let _ = std::fs::remove_file(&launches);
}

#[test]
fn rename_after_a_turn_names_the_session_init_reported() {
    let (launch, control, launches) = rename_launch("after-turn", "normal", &[]);
    let (mut client, events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("fixture handshake");
    client.prompt("hello").expect("prompt is accepted");
    drain_until_turn_end(&events);
    client
        .rename_session("After the turn".to_string())
        .expect("the worker is running");
    let (_, error) = next_rename_answer(&events);
    assert_eq!(error, None);

    let renames = renames_in(&control);
    assert_eq!(renames.len(), 1, "{renames:?}");
    assert_eq!(renames[0]["session_id"], "fixture-session-1");
    client.shutdown().expect("clean shutdown");
    let _ = std::fs::remove_file(&control);
    let _ = std::fs::remove_file(&launches);
}

#[test]
fn rename_session_reports_the_cli_error() {
    let (launch, control, launches) = rename_launch(
        "error",
        "normal",
        &["--fixture-rename-error=session_id is not the current session".to_string()],
    );
    let (mut client, events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("fixture handshake");
    client
        .rename_session("Refused".to_string())
        .expect("the worker is running");
    let (title, error) = next_rename_answer(&events);
    assert_eq!(title, "Refused");
    assert_eq!(
        error.as_deref(),
        Some("session_id is not the current session")
    );
    client.shutdown().expect("clean shutdown");
    let _ = std::fs::remove_file(&control);
    let _ = std::fs::remove_file(&launches);
}

#[test]
fn rename_after_refused_resume_names_the_new_session() {
    let (launch, control, launches) = rename_launch("refused", "refuse_resume", &[]);
    let (mut client, events) =
        ClaudeClient::launch(launch.resuming("dead-id"), std::env::temp_dir())
            .expect("a refused resume still yields a usable session");
    assert!(client.resumed_session_refused());
    client
        .rename_session("Fresh start".to_string())
        .expect("the worker is running");
    let (_, error) = next_rename_answer(&events);
    assert_eq!(error, None);

    let renames = renames_in(&control);
    assert_eq!(renames.len(), 1, "{renames:?}");
    let logged = renames[0]["session_id"].as_str().expect("a session id");
    assert_ne!(logged, "dead-id");
    let retry = json_lines(&launches)
        .into_iter()
        .rfind(|line| line["resume"].is_null())
        .expect("the retry launch is logged");
    assert_eq!(retry["session_id"], logged);
    client.shutdown().expect("clean shutdown");
    let _ = std::fs::remove_file(&control);
    let _ = std::fs::remove_file(&launches);
}

// ---------------------------------------------------------------------------
// A5: the CLI's permission detail, and the permission rules.
// ---------------------------------------------------------------------------

/// A scratch file holding `contents`, for a fixture flag that names a path.
fn scratch_json(label: &str, contents: &str) -> PathBuf {
    let n = LOG_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "sirio-a5-{label}-{}-{n}.json",
        std::process::id()
    ));
    std::fs::write(&path, contents).expect("a scratch file");
    path
}

fn launch_with_flags(mode: &str, flags: Vec<String>) -> (ClaudeClient, sirio_chat::EventStream) {
    let mut args = vec![FIXTURE.to_string(), mode.to_string()];
    args.extend(flags);
    ClaudeClient::launch(
        ClaudeLaunch::fixture(PathBuf::from("python3"), args),
        std::env::temp_dir(),
    )
    .expect("fixture claude should complete its handshake")
}

/// The first permission request of a `permission`-mode turn, and the client,
/// with the fixture merging `fields` into its `can_use_tool`.
fn permission_request_with(
    fields: &str,
) -> (
    ClaudeClient,
    sirio_chat::EventStream,
    u64,
    String,
    Vec<sirio_chat::PermissionOption>,
    Option<sirio_chat::PermissionDetail>,
) {
    let path = scratch_json("fields", fields);
    let (mut client, events) = launch_with_flags(
        "permission",
        vec![format!("--fixture-permission-fields={}", path.display())],
    );
    client.prompt("delete it").expect("prompt is accepted");
    loop {
        if let ChatEvent::PermissionRequest {
            request_id,
            title,
            options,
            detail,
            ..
        } = next_event(&events)
        {
            let _ = std::fs::remove_file(&path);
            return (client, events, request_id, title, options, detail);
        }
    }
}

fn finish_permission_turn(
    mut client: ClaudeClient,
    events: &sirio_chat::EventStream,
    request_id: u64,
) {
    client
        .respond_permission(request_id, "reject_once")
        .expect("the choice reaches the agent");
    drain_until_turn_end(events);
    client.shutdown().expect("clean shutdown");
}

#[test]
fn permission_detail_reaches_the_event() {
    let (client, events, request_id, title, _, detail) = permission_request_with(
        r#"{"title":"Run the build","description":"make a file",
            "decision_reason_type":"rule","decision_reason_rule_source":"userSettings",
            "blocked_path":"/w/out.txt","mcp_server":{"name":"probe","source":"project"},
            "default_to_no":true}"#,
    );
    let tool = sirio_claude::message::ToolUse {
        id: "toolu_perm_1".into(),
        name: "Bash".into(),
        input: serde_json::json!({"command": "rm -rf build", "description": "Remove build"}),
    };
    assert_eq!(title, sirio_claude::tools::describe(&tool).title);
    assert_eq!(
        detail,
        Some(sirio_chat::PermissionDetail {
            title: Some("Run the build".into()),
            description: Some("make a file".into()),
            reason: Some("Matches an ask rule in user settings".into()),
            path: Some("/w/out.txt".into()),
            mcp_server: Some(sirio_chat::PermissionMcpServer {
                name: "probe".into(),
                source: "project".into(),
            }),
            default_to_no: true,
        })
    );
    finish_permission_turn(client, &events, request_id);
}

#[test]
fn the_cli_reason_wins_over_its_type() {
    let (client, events, request_id, _, _, detail) = permission_request_with(
        r#"{"decision_reason":"Path is outside allowed working directories",
            "decision_reason_type":"workingDir"}"#,
    );
    assert_eq!(
        detail.and_then(|detail| detail.reason).as_deref(),
        Some("Path is outside allowed working directories")
    );
    finish_permission_turn(client, &events, request_id);
}

#[test]
fn suppression_drops_always_allow() {
    // The mode's own `addRules` suggestion would otherwise earn "Always allow".
    let (client, events, request_id, _, options, _) =
        permission_request_with(r#"{"suppress_always_allow_rule":true}"#);
    assert_eq!(
        options
            .iter()
            .map(|option| option.id.as_str())
            .collect::<Vec<_>>(),
        ["allow_once", "reject_once"]
    );
    finish_permission_turn(client, &events, request_id);
}

#[test]
fn wrongly_typed_fields_degrade_instead_of_hiding_the_card() {
    // A field of the wrong JSON type must read as absent. A parse that
    // refused the whole request would have Sirio deny it unseen.
    let (client, events, request_id, _, options, detail) = permission_request_with(
        r#"{"title":5,"default_to_no":"yes","suppress_always_allow_rule":"true",
            "mcp_server":{"name":"probe"}}"#,
    );
    assert_eq!(detail, None);
    assert_eq!(
        options
            .iter()
            .map(|option| option.id.as_str())
            .collect::<Vec<_>>(),
        ["allow_once", "allow_always", "reject_once"]
    );
    finish_permission_turn(client, &events, request_id);
}

fn rules_client(payload: Option<&str>) -> (ClaudeClient, sirio_chat::EventStream, Option<PathBuf>) {
    let path = payload.map(|payload| scratch_json("rules", payload));
    let flags = path
        .iter()
        .map(|path| format!("--fixture-permission-rules={}", path.display()))
        .collect();
    let (client, events) = launch_with_flags("echo_control", flags);
    (client, events, path)
}

#[test]
fn rules_answer_is_read() {
    let (mut client, _events, path) = rules_client(Some(
        r#"{"state":{"rules":[
              {"behavior":"allow","source":"userSettings","rule":"Bash(echo hi:*)",
               "description":{"prefix":"Any Bash command starting with ","emphasis":"echo hi"},
               "editability":"persistent"},
              {"behavior":"deny","source":"policySettings","rule":"Read(./secret.txt)",
               "editability":"readonly","notInEffect":true},
              {"behavior":"ask","source":"userSettings","editability":"persistent"},
              {"behavior":"ask","source":"userSettings","rule":"Bash(ls)"}],
            "workspaceDirectories":[{"path":"/w/extra","source":"localSettings"}],
            "originalCwd":"/w","managedOnly":true,"withheld":true,
            "errors":[{"file":"/w/.claude/settings.json","path":"permissions.allow",
                       "message":"Expected array, but received string"}]}}"#,
    ));
    let rules = client
        .rules_control()
        .list()
        .expect("the fixture answers with rules");
    assert_eq!(
        rules,
        sirio_chat::PermissionRules {
            rules: vec![
                sirio_chat::PermissionRule {
                    behavior: "allow".into(),
                    source: "userSettings".into(),
                    rule: "Bash(echo hi:*)".into(),
                    description: Some("Any Bash command starting with echo hi".into()),
                    editability: "persistent".into(),
                    not_in_effect: false,
                },
                sirio_chat::PermissionRule {
                    behavior: "deny".into(),
                    source: "policySettings".into(),
                    rule: "Read(./secret.txt)".into(),
                    description: None,
                    editability: "readonly".into(),
                    not_in_effect: true,
                },
                sirio_chat::PermissionRule {
                    behavior: "ask".into(),
                    source: "userSettings".into(),
                    rule: "Bash(ls)".into(),
                    description: None,
                    editability: String::new(),
                    not_in_effect: false,
                },
            ],
            workspace_directories: vec!["/w/extra".into()],
            managed_only: true,
            errors: vec![sirio_chat::SettingsError {
                file: "/w/.claude/settings.json".into(),
                path: "permissions.allow".into(),
                message: "Expected array, but received string".into(),
            }],
            withheld: true,
        }
    );
    client.shutdown().expect("clean shutdown");
    if let Some(path) = path {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn rules_refusal_is_the_clis_own_text() {
    let (mut client, _events, _) = rules_client(None);
    let error = client
        .rules_control()
        .list()
        .expect_err("the floor refuses the request");
    assert_eq!(
        error,
        "Unsupported control request subtype: list_permission_rules"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn rules_success_without_state_is_an_error() {
    let (mut client, _events, path) = rules_client(Some("{}"));
    let error = client
        .rules_control()
        .list()
        .expect_err("a success with no rules is not a list");
    assert_eq!(error, "the answer had no rules");
    // The fixture re-reads the file: a state whose rules are not a list.
    let path = path.expect("a rules file");
    std::fs::write(&path, r#"{"state":{"rules":"none","managedOnly":true}}"#).expect("rewrite");
    let error = client
        .rules_control()
        .list()
        .expect_err("rules that are not a list are not a list");
    assert_eq!(error, "the answer had no rules");
    client.shutdown().expect("clean shutdown");
    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// The hooks listing.
//
// Ways this can fail (covered by the tests below unless marked):
//   - no `hooks` array reads as an empty listing instead of an error;
//   - a CLI error is reworded instead of passed verbatim;
//   - a row of wrong types hides the listing or is dropped;
//   - a row without `event`, or with an empty one, is listed;
//   - a row with neither `displayText` nor `commandText` is listed;
//   - a row with only `commandText` is dropped, or has no display text;
//   - a row with a `statusMessage` and no `commandText` invents a command
//     from the status message;
//   - a fractional timeout is cut to an integer; a `-0.0` timeout is kept;
//   - `policy`, `events`, `errors`, `safeMode` or `bareMode` absent or of the
//     wrong type fails the listing or becomes a value (the defaults rewrite);
//   - `bareMode` as a boolean becomes a mode (`safeMode` shares the reader).
// Not covered here, checked by reading only:
//   - no payload and no error (the worker's `(None, None)` arm): the fixture
//     always writes a `response`;
//   - the 10 s timeout and the worker being gone (the handle's errors are the
//     rules handle's);
//   - ACP having no handle;
//   - the trace, state and background fixture loops answering `{}` to the verb
//     (the main loop is covered by the refusal test; the session-panel E2E's
//     trace, session-state and background-tasks scenarios open the panel, so
//     they send the request to the other three, but none asserts the answer).

fn hooks_client(payload: Option<&str>) -> (ClaudeClient, sirio_chat::EventStream, Option<PathBuf>) {
    let path = payload.map(|payload| scratch_json("hooks", payload));
    let flags = path
        .iter()
        .map(|path| format!("--fixture-hooks={}", path.display()))
        .collect();
    let (client, events) = launch_with_flags("echo_control", flags);
    (client, events, path)
}

fn expected_hook(event: &str, source: &str, label: &str, text: &str) -> sirio_chat::Hook {
    sirio_chat::Hook {
        event: event.into(),
        matcher: String::new(),
        source: source.into(),
        source_label: label.into(),
        plugin_name: None,
        content_label: "Command".into(),
        display_text: text.into(),
        command_text: Some(text.into()),
        disabled: false,
        condition: None,
        timeout: None,
        runs_once: false,
        runs_in_background: false,
    }
}

#[test]
fn hooks_listing_is_read() {
    // The rows with the CLI's full `sourceLabel` come from the probe (run-q1:
    // user, user with a status message, project once/background, http; run-q2:
    // plugin and session); only the user and session rows have shortened
    // commands. The rest are synthetic edge cases.
    let (mut client, _events, path) = hooks_client(Some(
        r#"{"events":[
              {"name":"PreToolUse","summary":"Before tool execution",
               "supportsMatcher":true,"hookCount":2},
              {"summary":"no name"},
              {"name":"Stop"}],
            "hooks":[
              {"event":"PreToolUse","matcher":"Bash","source":"userSettings",
               "sourceLabel":"User settings (~/.claude/settings.json)","type":"command",
               "displayText":"echo user-pre-bash","commandText":"echo user-pre-bash",
               "contentLabel":"Command",
               "editable":{"matcher":"Bash",
                           "config":{"type":"command","command":"echo user-pre-bash"}}},
              {"event":"PreToolUse","matcher":"Bash","source":"userSettings",
               "sourceLabel":"User settings (~/.claude/settings.json)","type":"command",
               "displayText":"Checking bash","commandText":"true","contentLabel":"Command",
               "timeout":7,"statusMessage":"Checking bash"},
              {"event":"Stop","matcher":"","source":"projectSettings",
               "sourceLabel":"Project settings (.claude/settings.json)","type":"command",
               "displayText":"true","commandText":"true","contentLabel":"Command",
               "runsOnce":true,"runsInBackground":true},
              {"event":"SubagentStart","matcher":"","source":"projectSettings",
               "sourceLabel":"Project settings (.claude/settings.json)","type":"http",
               "displayText":"http://127.0.0.1:9/hook","commandText":"http://127.0.0.1:9/hook",
               "contentLabel":"URL","condition":"Bash(ls *)"},
              {"event":"Stop","matcher":"","source":"pluginHook",
               "sourceLabel":"Plugin hooks (~/.claude/plugins/*/hooks/hooks.json)",
               "pluginName":"hkplug@inline","type":"command","displayText":"plug stop",
               "commandText":"true","contentLabel":"Command","statusMessage":"plug stop"},
              {"event":"PreToolUse","matcher":5,"source":"localSettings","sourceLabel":"Local",
               "displayText":"wrong types","commandText":"wrong types","contentLabel":"Command",
               "disabled":"yes","timeout":"7"},
              {"event":"Stop","source":"userSettings","displayText":"half","commandText":"half",
               "timeout":1.5,"disabled":true},
              {"matcher":"Bash","source":"userSettings","displayText":"no event",
               "commandText":"no event"},
              {"event":"Notification","source":"userSettings","commandText":"only the command"},
              {"event":"Notification","source":"policySettings","displayText":"Running a check",
               "statusMessage":"Running a check"},
              {"event":"Stop","source":"userSettings","displayText":"negative","timeout":-3},
              {"event":"PreToolUse","matcher":"Bash","source":"sessionHook",
               "sourceLabel":"Session hooks (this session only)","type":"command",
               "displayText":"echo skill-pre","commandText":"echo skill-pre",
               "contentLabel":"Command"},
              {"event":"Stop","source":"userSettings","sourceLabel":"No text at all"},
              {"event":"","source":"userSettings","displayText":"empty event",
               "commandText":"empty event"},
              {"event":"Stop","source":"userSettings","displayText":"zero","timeout":-0.0}],
            "eventCatalog":[{"name":"Stop"}],
            "safeMode":{"managedHooksStillApply":true,"exitHint":"restart without --safe-mode"},
            "bareMode":true,
            "policy":{"allDisabled":true,"policyUnreadable":true,"policyHookCount":2},
            "errors":[{"file":"/w/.claude/settings.json","path":"hooks.Stop",
                       "message":"Expected array, but received string"},
                      {"message":"no file"}]}"#,
    ));
    let listing = client
        .hooks_control()
        .list()
        .expect("the fixture answers with a listing");

    assert_eq!(
        listing.events,
        vec![
            sirio_chat::HookEvent {
                name: "PreToolUse".into(),
                summary: "Before tool execution".into(),
            },
            sirio_chat::HookEvent {
                name: "Stop".into(),
                summary: String::new(),
            },
        ]
    );

    let user = "User settings (~/.claude/settings.json)";
    let project = "Project settings (.claude/settings.json)";
    let mut plain = expected_hook("PreToolUse", "userSettings", user, "echo user-pre-bash");
    plain.matcher = "Bash".into();
    let mut status = expected_hook("PreToolUse", "userSettings", user, "true");
    status.matcher = "Bash".into();
    status.display_text = "Checking bash".into();
    status.timeout = Some("7".into());
    let mut once = expected_hook("Stop", "projectSettings", project, "true");
    once.runs_once = true;
    once.runs_in_background = true;
    let mut http = expected_hook(
        "SubagentStart",
        "projectSettings",
        project,
        "http://127.0.0.1:9/hook",
    );
    http.content_label = "URL".into();
    http.condition = Some("Bash(ls *)".into());
    let mut plugin = expected_hook(
        "Stop",
        "pluginHook",
        "Plugin hooks (~/.claude/plugins/*/hooks/hooks.json)",
        "true",
    );
    plugin.display_text = "plug stop".into();
    plugin.plugin_name = Some("hkplug@inline".into());
    // Wrong JSON types: each field reads as absent, the row stays.
    let wrong = expected_hook("PreToolUse", "localSettings", "Local", "wrong types");
    let mut half = expected_hook("Stop", "userSettings", "", "half");
    half.content_label = String::new();
    half.timeout = Some("1.5".into());
    half.disabled = true;
    // Only `commandText`: the display text falls back to it.
    let mut only_command = expected_hook("Notification", "userSettings", "", "only the command");
    only_command.content_label = String::new();
    // A status message and no `commandText`: no command is invented.
    let mut message = expected_hook("Notification", "policySettings", "", "Running a check");
    message.content_label = String::new();
    message.command_text = None;
    let mut negative = expected_hook("Stop", "userSettings", "", "negative");
    negative.content_label = String::new();
    negative.command_text = None;
    let mut session = expected_hook(
        "PreToolUse",
        "sessionHook",
        "Session hooks (this session only)",
        "echo skill-pre",
    );
    session.matcher = "Bash".into();
    // A negative zero is not a timeout.
    let mut zero = expected_hook("Stop", "userSettings", "", "zero");
    zero.content_label = String::new();
    zero.command_text = None;
    assert_eq!(
        listing.hooks,
        vec![
            plain,
            status,
            once,
            http,
            plugin,
            wrong,
            half,
            only_command,
            message,
            negative,
            session,
            zero,
        ]
    );

    assert_eq!(
        listing.policy,
        sirio_chat::HooksPolicy {
            disabled_by_policy: false,
            managed_only: false,
            plugin_only: false,
            all_disabled: true,
            unreadable: true,
        }
    );
    assert_eq!(
        listing.safe_mode,
        Some(sirio_chat::HooksMode {
            exit_hint: "restart without --safe-mode".into(),
            managed_hooks_still_apply: true,
        })
    );
    assert_eq!(listing.bare_mode, None);
    assert_eq!(
        listing.errors,
        vec![sirio_chat::SettingsError {
            file: "/w/.claude/settings.json".into(),
            path: "hooks.Stop".into(),
            message: "Expected array, but received string".into(),
        }]
    );

    // The fixture re-reads the file. Every other key of the wrong type, or
    // absent, reads as its default: the listing stays readable.
    let path = path.expect("a hooks file");
    for answer in [
        r#"{"hooks":[],"policy":"x","events":"x","errors":"x","safeMode":"x","bareMode":1}"#,
        r#"{"hooks":[]}"#,
    ] {
        std::fs::write(&path, answer).expect("rewrite");
        let listing = client
            .hooks_control()
            .list()
            .expect("a listing with no other key is still a listing");
        assert!(listing.hooks.is_empty());
        assert!(listing.events.is_empty());
        assert!(listing.errors.is_empty());
        assert_eq!(listing.policy, sirio_chat::HooksPolicy::default());
        assert_eq!(listing.safe_mode, None);
        assert_eq!(listing.bare_mode, None);
    }
    client.shutdown().expect("clean shutdown");
    let _ = std::fs::remove_file(path);
}

#[test]
fn hooks_refusal_is_the_clis_own_text() {
    let (mut client, _events, _) = hooks_client(None);
    let error = client
        .hooks_control()
        .list()
        .expect_err("a CLI without the verb refuses the request");
    assert_eq!(
        error,
        "Unsupported control request subtype: get_hooks_listing"
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn hooks_success_without_hooks_is_an_error() {
    let (mut client, _events, path) = hooks_client(Some("{}"));
    let error = client
        .hooks_control()
        .list()
        .expect_err("a success with no hooks is not a listing");
    assert_eq!(error, "the answer had no hooks");
    // The fixture re-reads the file: hooks that are not a list.
    let path = path.expect("a hooks file");
    std::fs::write(&path, r#"{"hooks":"none","policy":{"allDisabled":true}}"#).expect("rewrite");
    let error = client
        .hooks_control()
        .list()
        .expect_err("hooks that are not a list are not a listing");
    assert_eq!(error, "the answer had no hooks");
    client.shutdown().expect("clean shutdown");
    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// The connector switch and the plugin and skill reloads.
//
// Ways this can fail:
//   - `refuse_if_busy` is sent without the capability, or is missing with it;
//   - `hold_on_cache_impact` is sent on a "Reload anyway", or is missing on
//     the first reload;
//   - a toggle success with no body reads as an error;
//   - a busy answer is told from its text rather than its `error_code`;
//   - a refusal is not `ReloadError::Refused`;
//   - a held answer replaces the slash list;
//   - an applied answer does not replace it;
//   - a floor answer without `held` is not applied;
//   - `reload_skills` does not lead to the new list;
//   - a toggle refusal loses the CLI's sentence;
//   - `mcp_status` after an off toggle does not read `Disabled`.
// ---------------------------------------------------------------------------

/// A `session_panel` fixture with its control log in a directory of its own,
/// so a test reads only what it sent. `finish` removes the directory.
fn reload_session(
    tag: &str,
    extra: &[&str],
) -> (ClaudeClient, sirio_chat::EventStream, PathBuf) {
    let dir = std::env::temp_dir().join(format!("sirio-reload-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory for the control log");
    let log = dir.join("control.log");
    let mut args = vec![
        FIXTURE.to_string(),
        "session_panel".to_string(),
        format!("--fixture-control-log={}", log.display()),
    ];
    args.extend(extra.iter().map(|flag| (*flag).to_string()));
    let (client, events) =
        ClaudeClient::launch(ClaudeLaunch::fixture(PathBuf::from("python3"), args), dir)
            .expect("fixture claude should complete its handshake");
    (client, events, log)
}

/// Shuts the session down and removes its directory, best effort.
fn finish(mut client: ClaudeClient, log: &std::path::Path) {
    client.shutdown().expect("clean shutdown");
    if let Some(dir) = log.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// What the fixture heard for one verb, as the CLI would have read it.
fn control_requests(log: &std::path::Path, subtype: &str) -> Vec<serde_json::Value> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|request| request["subtype"] == subtype)
        .collect()
}

/// Every slash list that reaches the stream until it has been quiet for
/// 400 ms. An applied or held `reload_plugins` publishes its list before it
/// answers, so a drain after the call misses none of that call's lists; a
/// list that follows its answer (`commands_changed`) needs `wait_for_list`.
fn slash_lists(events: &sirio_chat::EventStream) -> Vec<Vec<String>> {
    let mut lists = Vec::new();
    while let Some(event) = next_event_within(events, Duration::from_millis(400)) {
        if let ChatEvent::AvailableCommands(commands) = event {
            lists.push(commands.into_iter().map(|command| command.name).collect());
        }
    }
    lists
}

/// The first slash list `want` accepts, waiting up to 5 s between events.
fn wait_for_list(
    events: &sirio_chat::EventStream,
    want: impl Fn(&[String]) -> bool,
) -> Option<Vec<String>> {
    while let Some(event) = next_event_within(events, Duration::from_secs(5)) {
        if let ChatEvent::AvailableCommands(commands) = event {
            let names: Vec<String> = commands.into_iter().map(|command| command.name).collect();
            if want(&names) {
                return Some(names);
            }
        }
    }
    None
}

fn offers(list: &[String], name: &str) -> bool {
    list.iter().any(|offered| offered == name)
}

#[test]
fn a_toggle_reaches_the_cli_and_the_status_reads_disabled() {
    use sirio_claude::McpState;

    let (client, _events, log) = reload_session("toggle", &[]);
    client
        .mcp_control()
        .toggle("codegraph", false)
        .expect("a success with no body is a success");
    let servers = client.refresh_mcp_status().expect("the CLI answers");
    let codegraph = servers
        .iter()
        .find(|server| server.name == "codegraph")
        .expect("the server is listed");
    assert_eq!(codegraph.status, McpState::Disabled);
    let toggles = control_requests(&log, "mcp_toggle");
    assert_eq!(toggles.len(), 1);
    assert_eq!(toggles[0]["enabled"], false);
    finish(client, &log);
}

#[test]
fn a_refused_toggle_carries_the_refusal() {
    let (client, _events, log) = reload_session("toggle-refused", &["--fixture-toggle=refused"]);
    let error = client
        .mcp_control()
        .toggle("codegraph", false)
        .expect_err("the CLI refuses the verb");
    assert!(sirio_chat::is_refusal(&format!("{error:#}")));
    finish(client, &log);
}

#[test]
fn a_reload_asks_to_refuse_if_busy_only_with_the_capability() {
    let (client, _events, log) = reload_session("busy-flag", &[]);
    client.reload_control().plugins(true).expect("the reload applies");
    let sent = control_requests(&log, "reload_plugins");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["hold_on_cache_impact"], true);
    assert_eq!(sent[0]["refuse_if_busy"], true);
    finish(client, &log);

    let (floor, _events, log) = reload_session("busy-flag-floor", &["--fixture-floor"]);
    floor.reload_control().plugins(true).expect("the reload applies");
    let sent = control_requests(&log, "reload_plugins");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["hold_on_cache_impact"], true);
    assert!(sent[0].get("refuse_if_busy").is_none());
    finish(floor, &log);

    let (again, _events, log) = reload_session("anyway", &[]);
    again.reload_control().plugins(false).expect("the reload applies");
    let sent = control_requests(&log, "reload_plugins");
    assert!(sent[0].get("hold_on_cache_impact").is_none());
    finish(again, &log);
}

#[test]
fn a_floor_reload_without_held_is_applied() {
    let (client, events, log) = reload_session("floor", &["--fixture-floor"]);
    let reloaded = client
        .reload_control()
        .plugins(true)
        .expect("the reload applies");
    assert!(!reloaded.held);
    assert!(
        wait_for_list(&events, |list| offers(list, "pa:hello")).is_some(),
        "an applied reload must replace the slash list"
    );
    finish(client, &log);
}

#[test]
fn a_held_reload_reports_the_impact_and_keeps_the_commands() {
    let (client, events, log) = reload_session("held", &["--fixture-reload=held"]);
    // The handshake's own list is out of the way before the reload asks.
    assert!(!slash_lists(&events).is_empty(), "the handshake lists commands");
    let reload = client.reload_control();
    let held = reload.plugins(true).expect("a held answer is a success");
    assert!(held.held);
    assert!(held.impact.servers_added.len() == 2, "two servers would be added");
    assert!(
        held.impact.servers_added[0] == "plugin:pa:m",
        "the first added name is kept whole"
    );
    assert!(
        held.impact.servers_added[1] == "plugin:pa:\u{202e}x",
        "the second added name keeps its override"
    );
    assert!(
        held.impact.servers_removed == ["old-server"],
        "the removed name is kept"
    );
    assert_eq!(held.impact.lsp_tool_change.as_deref(), Some("may-add"));
    assert!(
        slash_lists(&events).is_empty(),
        "a held reload must publish no slash list"
    );

    let applied = reload.plugins(false).expect("without the hold it applies");
    assert!(!applied.held);
    assert!(
        wait_for_list(&events, |list| offers(list, "pa:hello")).is_some(),
        "the applied reload replaces the slash list"
    );
    finish(client, &log);
}

#[test]
fn a_busy_reload_is_told_by_its_error_code() {
    use sirio_chat::ReloadError;

    let (client, _events, log) = reload_session("busy", &["--fixture-reload=busy"]);
    assert_eq!(
        client.reload_control().plugins(true).err(),
        Some(ReloadError::Busy)
    );
    finish(client, &log);

    let (text, _events, log) = reload_session("busy-text", &["--fixture-reload=busy-text"]);
    let error = text
        .reload_control()
        .plugins(true)
        .expect_err("the same words with no code are a plain failure");
    assert!(matches!(error, ReloadError::Failed(_)));
    finish(text, &log);
}

#[test]
fn a_refused_reload_is_a_refusal() {
    use sirio_chat::ReloadError;

    // The current CLI sends a code with the refusal; the floor sends none.
    for (tag, extra) in [
        ("refused", vec!["--fixture-reload=refused"]),
        ("refused-floor", vec!["--fixture-reload=refused", "--fixture-floor"]),
    ] {
        let (client, _events, log) = reload_session(tag, &extra);
        assert_eq!(
            client.reload_control().plugins(true).err(),
            Some(ReloadError::Refused)
        );
        finish(client, &log);
    }
}

#[test]
fn a_refused_skills_reload_is_a_refusal() {
    use sirio_chat::ReloadError;

    let (client, _events, log) = reload_session("skills-refused", &["--fixture-skills=refused"]);
    let reload = client.reload_control();
    reload.plugins(true).expect("the reload applies");
    assert_eq!(reload.skills().err(), Some(ReloadError::Refused));
    finish(client, &log);
}

#[test]
fn reload_skills_brings_the_new_skill_into_the_slash_list() {
    let (client, events, log) = reload_session("skills", &[]);
    let reload = client.reload_control();
    reload.plugins(true).expect("the reload applies");
    reload.skills().expect("the skills reload");
    let list = wait_for_list(&events, |list| offers(list, "lateskill"))
        .expect("the new skill reaches the slash list");
    assert!(offers(&list, "pa:hello"));
    finish(client, &log);
}

// Failure list for startup failures (E2E, written first). The feature is wrong if:
//  - a reason result reaches the caller as a timeout or an "ACP transport error";
//  - the reason result is ignored because it came before the `initialize` answer;
//  - the opt-in reason never appears because the variable did not reach the child;
//  - a reason on a resume relaunches a fresh session;
//  - a resume without a reason no longer retries;
//  - a refused resume leaves an "ended" footer in the fresh session;
//  - a CLI that exits silently loses its exit code or its last stderr line;
//  - an unknown reason loses its errors;
//  - a config path or backup path is dropped.

/// A fixture launch that cannot start in the way `kind` says, logging each
/// launch. `resume` makes it a resuming launch.
fn startup_launch(kind: &str, resume: Option<&str>, tag: &str) -> (ClaudeLaunch, PathBuf) {
    let launches =
        std::env::temp_dir().join(format!("sirio-startup-{tag}-{}.launch", std::process::id()));
    let _ = std::fs::remove_file(&launches);
    let launch = ClaudeLaunch::fixture(
        PathBuf::from("python3"),
        vec![
            FIXTURE.into(),
            "normal".into(),
            format!("--fixture-startup={kind}"),
            format!("--fixture-launch-log={}", launches.display()),
        ],
    );
    let launch = match resume {
        Some(id) => launch.resuming(id),
        None => launch,
    };
    (launch, launches)
}

/// The text of a launch that must fail with `ChatError::Startup`.
fn startup_error(launch: ClaudeLaunch) -> String {
    use sirio_chat_vocab::ChatError;
    let error = ClaudeClient::launch_with_timeouts(
        launch,
        std::env::temp_dir(),
        Duration::from_secs(20),
        Duration::from_secs(60),
        Duration::from_secs(60),
    )
    .expect_err("a CLI that cannot start must not yield a session");
    assert!(
        matches!(
            error.downcast_ref::<ChatError>(),
            Some(ChatError::Startup(_))
        ),
        "the failure must be a startup failure"
    );
    format!("{error}")
}

#[test]
fn a_startup_reason_is_read_at_the_handshake() {
    let (launch, launches) = startup_launch("config_invalid", None, "config");
    let text = startup_error(launch);
    let rlo = char::from_u32(0x202E).unwrap();
    assert_eq!(
        text,
        format!(
            "Claude Code could not start: a configuration file is not valid JSON.\n\n\
             Configuration error in /fixture/config/.claude.json: JSON Parse error: \
             Expected '}}'\nFix the JSON, or move the file aside.\nbidi {rlo}here\n\n\
             File: /fixture/config/.claude.json\nBackup: /fixture/config/backups/.claude.json.bak"
        )
    );
    let _ = std::fs::remove_file(&launches);
}

#[test]
fn the_cli_is_launched_with_startup_results() {
    if std::env::var_os("CLAUDE_CODE_STARTUP_FAILURE_RESULTS").is_some() {
        println!("SKIP: CLAUDE_CODE_STARTUP_FAILURE_RESULTS is inherited");
        return;
    }
    let (launch, launches) = startup_launch("cwd_unavailable", None, "cwd");
    let text = startup_error(launch);
    assert!(
        text.starts_with("Claude Code could not start: this folder no longer exists or cannot be read.")
    );
    let _ = std::fs::remove_file(&launches);
}

#[test]
fn an_unknown_startup_reason_is_shown_verbatim() {
    let (launch, launches) = startup_launch("future_reason", None, "future");
    let text = startup_error(launch);
    assert!(text.starts_with("Claude Code could not start (some_future_reason)."));
    assert!(text.contains("A reason this build does not know."));
    let _ = std::fs::remove_file(&launches);
}

#[test]
fn a_cli_that_exits_before_the_handshake_names_its_exit() {
    let (launch, launches) = startup_launch("no_result", None, "noresult");
    let text = startup_error(launch);
    assert!(text.starts_with("Claude Code exited before it started (agent exited with code 1)"));
    assert!(text.contains("fixture: last line"));
    let _ = std::fs::remove_file(&launches);
}

#[test]
fn a_startup_reason_on_a_resume_is_final() {
    let (launch, launches) = startup_launch("resume_reason", Some("sess-held"), "held");
    let text = startup_error(launch);
    assert!(text.starts_with(
        "Claude Code could not resume this conversation: it is running as a background session."
    ));
    assert_eq!(
        json_lines(&launches).len(),
        1,
        "a reason must not relaunch a fresh session"
    );
    let _ = std::fs::remove_file(&launches);
}

#[test]
fn a_refused_resume_leaves_no_turn_behind() {
    let (launch, launches) = startup_launch("resume_no_reason", Some("sess-gone"), "gone");
    let (mut client, events) = ClaudeClient::launch(launch, std::env::temp_dir())
        .expect("a refused resume must still yield a usable session");
    assert_eq!(json_lines(&launches).len(), 2, "the resume, then the fresh start");
    // Nothing was asked yet, so nothing may have ended or been said.
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match next_event_within(&events, remaining) {
            Some(ChatEvent::TurnEnded { .. } | ChatEvent::AgentMessageChunk(_)) => {
                panic!("a refused resume must leave no turn behind")
            }
            Some(_) => {}
            None => break,
        }
    }
    client.prompt("hello").expect("prompt is accepted");
    drain_until_turn_end(&events);
    // One prompt, one end: a second one would arrive right behind the first.
    let after = std::time::Instant::now() + Duration::from_millis(500);
    loop {
        let remaining = after.saturating_duration_since(std::time::Instant::now());
        match next_event_within(&events, remaining) {
            Some(ChatEvent::TurnEnded { .. }) => panic!("one prompt must end exactly one turn"),
            Some(_) => {}
            None => break,
        }
    }
    client.shutdown().expect("clean shutdown");
    let _ = std::fs::remove_file(&launches);
}

#[test]
fn a_reasonless_startup_error_shows_its_lines() {
    let (launch, launches) = startup_launch("no_reason", None, "noreason");
    let text = startup_error(launch);
    assert_eq!(
        text,
        "Claude Code could not start.\n\nSomething went wrong before the session."
    );
    let _ = std::fs::remove_file(&launches);
}

// ---------------------------------------------------------------------------
// MCP Apps: tool notices, resource reads, widget calls and the launch variable.
//
// Failure list (written before the code):
// - a notice with the gate shut: the floor, or only two of the three names in
//   `system/init.capabilities`;
// - no notice with the gate open, or `Returned` missing, or one call announced twice;
// - a subagent's `mcp__` call announced;
// - `com.anthropic` meta left in a result, or the result of an errored call not an error;
// - `server_name` on the wire of a resource read, or the read's items not returned;
// - `check_permissions` sent without `mcp_call_check_permissions_v1`, or not sent with it;
// - a stray `toolu_mcp_call_` ask that draws a card or is allowed;
// - the ask of a widget's own call surfacing a card, or being denied;
// - an approval that outlives its call and lets the same ask through a second time;
// - the launch variable set when not asked, or missing when asked.
// ---------------------------------------------------------------------------

fn mcp_apps_flags(extra: &[&str]) -> Vec<String> {
    std::iter::once("--fixture-mcp-apps")
        .chain(extra.iter().copied())
        .map(str::to_string)
        .collect()
}

/// The notices of one `mcp_widget` turn that are MCP tool notices, in order.
fn mcp_tool_notices(tag: &str, extra: &[&str]) -> Vec<sirio_chat::McpToolUpdate> {
    let (launch, control, launches) = rename_launch(tag, "trace", &mcp_apps_flags(extra));
    let (mut client, events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("fixture handshake");
    let seen = cli_notices_turn(&mut client, &events, "mcp_widget");
    client.shutdown().expect("clean shutdown");
    let _ = std::fs::remove_file(&control);
    let _ = std::fs::remove_file(&launches);
    notices_of(&seen)
        .into_iter()
        .filter_map(|notice| match notice {
            SessionNotice::McpTool(update) => Some(update),
            _ => None,
        })
        .collect()
}

#[test]
fn mcp_tool_notices_follow_the_system_init_gate() {
    use sirio_chat::McpToolItem::{Called, Returned};
    let updates = mcp_tool_notices("mcp-notices", &[]);
    let ids: Vec<&str> = updates.iter().map(|update| update.id.as_str()).collect();
    assert_eq!(ids, ["toolu_show_1", "toolu_show_1", "toolu_show_2", "toolu_show_2"]);
    for (call, text) in [(0, "hi"), (2, "again")] {
        let Called { name, input } = &updates[call].item else {
            panic!("a call is announced first");
        };
        assert_eq!(name, "mcp__apps__show");
        assert_eq!(*input, serde_json::json!({"text": text}));
        let Returned { result } = &updates[call + 1].item else {
            panic!("its result follows it");
        };
        assert_eq!(
            result["content"],
            serde_json::json!([{"type": "text", "text": "{\"n\":42}"}])
        );
        assert_eq!(result["structuredContent"], serde_json::json!({"n": 42}));
        // The CLI's own meta stays out, the server's own stays in.
        assert_eq!(result["_meta"], serde_json::json!({"other/key": "v"}));
        assert!(result.get("isError").is_none(), "a success is not marked as an error");
    }

    // Shut: the floor names no capability, and two of the three names are not the gate.
    assert!(mcp_tool_notices("mcp-notices-floor", &["--fixture-floor"]).is_empty());
    let two = "--fixture-mcp-apps-caps=mcp_tool_ui_meta_v1,mcp_read_resource_v1";
    assert!(mcp_tool_notices("mcp-notices-two", &[two]).is_empty());
}

#[test]
fn a_subagents_mcp_call_is_not_announced() {
    let (launch, control, launches) = rename_launch("mcp-sub", "trace", &mcp_apps_flags(&[]));
    let (mut client, events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("fixture handshake");
    let seen = cli_notices_turn(&mut client, &events, "mcp_subagent");
    client.shutdown().expect("clean shutdown");
    let notices = notices_of(&seen);
    assert!(
        notices
            .iter()
            .all(|notice| !matches!(notice, SessionNotice::McpTool(_))),
        "a subagent's call has no row of its own to hang a widget on"
    );
    // The call did reach its card, so the absence above is not a silent fixture.
    assert!(notices.iter().any(|notice| matches!(
        notice,
        SessionNotice::Subagent(update) if matches!(
            &update.item,
            sirio_chat::SubagentItem::ToolStarted { id, .. } if id == "toolu_sub_show"
        )
    )));
    let _ = std::fs::remove_file(&control);
    let _ = std::fs::remove_file(&launches);
}

/// What the fixture heard, as `mcp_call` and `mcp_read_resource` requests.
fn requests_of(control: &std::path::Path, subtype: &str) -> Vec<serde_json::Value> {
    json_lines(control)
        .into_iter()
        .filter(|request| request["subtype"] == subtype)
        .collect()
}

#[test]
fn read_resource_names_the_server() {
    let (launch, control, launches) = rename_launch("mcp-read", "trace", &mcp_apps_flags(&[]));
    let (mut client, _events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("fixture handshake");
    let mcp = client.mcp_control();
    let contents = mcp
        .read_resource("apps", "ui://apps/show.html")
        .expect("the widget document is read");
    let items = contents.as_array().expect("the contents are an array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["mimeType"], "text/html;profile=mcp-app");
    assert!(
        mcp.read_resource("apps", "ui://apps/missing.html")
            .is_err(),
        "a resource that is not there is an error, not an empty list"
    );
    let reads = requests_of(&control, "mcp_read_resource");
    assert_eq!(reads.len(), 2);
    assert_eq!(reads[0]["serverName"], "apps");
    assert_eq!(reads[0]["uri"], "ui://apps/show.html");
    assert!(reads[0].get("server_name").is_none(), "the CLI refuses `server_name`");
    client.shutdown().expect("clean shutdown");
    let _ = std::fs::remove_file(&control);
    let _ = std::fs::remove_file(&launches);
}

/// Every event already queued, without waiting for more.
fn queued_events(events: &sirio_chat::EventStream) -> Vec<ChatEvent> {
    std::iter::from_fn(|| events.try_recv().ok()).collect()
}

fn is_permission_card(event: &ChatEvent) -> bool {
    matches!(event, ChatEvent::PermissionRequest { .. })
}

#[test]
fn a_widget_call_is_checked_and_its_ask_answered_once() {
    let (launch, control, launches) = rename_launch("mcp-call", "trace", &mcp_apps_flags(&[]));
    let (mut client, events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("fixture handshake");
    let answer = client
        .mcp_control()
        .call_tool_after_user_allowed("mcp__apps__act", serde_json::json!({"step": 1}))
        .expect("the call ran");
    assert_eq!(answer["structuredContent"]["n"], 42);
    assert_eq!(answer["structuredContent"]["tool"], "mcp__apps__act");
    let calls = requests_of(&control, "mcp_call");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["tool"], "mcp__apps__act");
    assert_eq!(calls[0]["arguments"], serde_json::json!({"step": 1}));
    assert_eq!(calls[0]["check_permissions"], true);
    assert_eq!(calls[0]["result_not_for_model"], true);
    let asks: Vec<_> = json_lines(&control)
        .into_iter()
        .filter_map(|line| line.get("mcp_call_ask").cloned())
        .collect();
    assert_eq!(asks, [serde_json::json!("allow")]);
    assert!(
        !queued_events(&events).iter().any(is_permission_card),
        "the user already pressed Allow on the widget's own card"
    );
    client.shutdown().expect("clean shutdown");
    let _ = std::fs::remove_file(&control);
    let _ = std::fs::remove_file(&launches);
}

#[test]
fn a_widget_call_asks_for_a_check_only_with_the_capability() {
    let caps = "--fixture-mcp-apps-caps=mcp_tool_ui_meta_v1,mcp_read_resource_v1,\
                mcp_call_result_not_for_model_v1";
    let flags = mcp_apps_flags(&[caps]);
    let (launch, control, launches) = rename_launch("mcp-nocheck", "trace", &flags);
    let (mut client, _events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("fixture handshake");
    client
        .mcp_control()
        .call_tool_after_user_allowed("mcp__apps__act", serde_json::json!({"step": 1}))
        .expect("the call ran");
    let calls = requests_of(&control, "mcp_call");
    assert_eq!(calls.len(), 1);
    assert!(calls[0].get("check_permissions").is_none());
    client.shutdown().expect("clean shutdown");
    let _ = std::fs::remove_file(&control);
    let _ = std::fs::remove_file(&launches);
}

/// The value the fixture logged under `key`, once it has: a stray ask is answered after the
/// turn's own end, on the fixture's schedule.
fn logged_value(control: &std::path::Path, key: &str) -> serde_json::Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(value) = json_lines(control)
            .into_iter()
            .find_map(|line| line.get(key).cloned())
        {
            return value;
        }
        assert!(std::time::Instant::now() < deadline, "the fixture never logged the key");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_stray_mcp_call_ask_is_denied_without_a_card() {
    let flags = mcp_apps_flags(&["--fixture-stray-mcp-ask"]);
    let (launch, control, launches) = rename_launch("mcp-stray", "trace", &flags);
    let (mut client, events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("fixture handshake");
    client.prompt("hello").expect("prompt is accepted");
    let turn = drain_until_turn_end(&events);
    assert_eq!(logged_value(&control, "stray_mcp_ask"), "deny");
    let after = queued_events(&events);
    assert!(
        !turn.iter().chain(after.iter()).any(is_permission_card),
        "an ask nobody approved draws no card"
    );
    client.shutdown().expect("clean shutdown");
    let _ = std::fs::remove_file(&control);
    let _ = std::fs::remove_file(&launches);
}

#[test]
fn an_approval_ends_with_its_calls_answer() {
    // In `bypassPermissions` the CLI asks nothing, so the approval granted for the call is never
    // used. The stray ask that follows carries the very tool and input the call had: were the
    // approval to outlast the call's answer, it would be let through.
    let flags = mcp_apps_flags(&["--fixture-start-bypass", "--fixture-stray-mcp-ask"]);
    let (launch, control, launches) = rename_launch("mcp-outlive", "trace", &flags);
    let (mut client, events) =
        ClaudeClient::launch(launch, std::env::temp_dir()).expect("fixture handshake");
    client
        .mcp_control()
        .call_tool_after_user_allowed("mcp__apps__act", serde_json::json!({"stray": true}))
        .expect("the call ran");
    client.prompt("hello").expect("prompt is accepted");
    drain_until_turn_end(&events);
    assert_eq!(logged_value(&control, "stray_mcp_ask"), "deny");
    client.shutdown().expect("clean shutdown");
    let _ = std::fs::remove_file(&control);
    let _ = std::fs::remove_file(&launches);
}

#[test]
fn the_launch_variable_follows_the_switch() {
    let host_of = |tag: &str, on: Option<bool>| {
        let (launch, control, launches) = rename_launch(tag, "trace", &[]);
        let launch = match on {
            Some(on) => launch.mcp_apps_host(on),
            None => launch,
        };
        let (mut client, _events) =
            ClaudeClient::launch(launch, std::env::temp_dir()).expect("fixture handshake");
        let host = json_lines(&launches)
            .first()
            .map(|line| line["mcp_apps_host"].clone())
            .expect("the fixture logged its launch");
        client.shutdown().expect("clean shutdown");
        let _ = std::fs::remove_file(&control);
        let _ = std::fs::remove_file(&launches);
        host
    };
    assert_eq!(host_of("mcp-host-on", Some(true)), "1");
    assert!(host_of("mcp-host-default", None).is_null());
    assert!(host_of("mcp-host-off", Some(false)).is_null());
}
