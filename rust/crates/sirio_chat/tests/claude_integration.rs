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

fn drain_until_turn_end(events: &sirio_chat::EventStream) -> Vec<ChatEvent> {
    let mut seen = Vec::new();
    loop {
        let event = next_event(events);
        let ended = matches!(event, ChatEvent::TurnEnded { .. });
        seen.push(event);
        if ended {
            return seen;
        }
    }
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
