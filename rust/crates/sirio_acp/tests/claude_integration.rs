//! The native Claude client against a deterministic fake `claude`.
//!
//! Test for test, this mirrors `acp_integration.rs`: the two transports owe
//! the surface the same behaviour, so they are held to the same list.

use std::path::PathBuf;
use std::time::Duration;

use futures::future::Either;
use sirio_acp::{AcpEvent, AvailableCommandInfo, ClaudeClient, ClaudeLaunch};
use sirio_claude::FastMode;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/claude_fixture.py"
);

/// The fixture is a python script, so the program is `python3` and the
/// script is its first argument — `ClaudeLaunch` carries the extra
/// arguments a test needs in front of the protocol flags.
fn launch(mode: &str) -> (ClaudeClient, sirio_acp::EventStream) {
    ClaudeClient::launch(fixture_launch(mode), std::env::temp_dir())
        .expect("fixture claude should complete its handshake")
}

fn fixture_launch(mode: &str) -> ClaudeLaunch {
    ClaudeLaunch::fixture(PathBuf::from("python3"), vec![FIXTURE.into(), mode.into()])
}

fn next_event(events: &sirio_acp::EventStream) -> AcpEvent {
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
fn first_signal(events: &sirio_acp::EventStream, prefix: &str, budget: Duration) -> Option<String> {
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
            Some(AcpEvent::OtherSessionUpdate { kind }) if kind.starts_with(prefix) => {
                return Some(kind);
            }
            Some(_) => continue,
            None => return None,
        }
    }
}

fn drain_until_turn_end(events: &sirio_acp::EventStream) -> Vec<AcpEvent> {
    let mut seen = Vec::new();
    loop {
        let event = next_event(events);
        let ended = matches!(event, AcpEvent::TurnEnded { .. });
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
        !modes
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
            AcpEvent::AvailableCommands(commands) => {
                lists.push(commands);
                if lists.len() == 2 {
                    break lists.pop().expect("the second list");
                }
            }
            AcpEvent::OtherSessionUpdate { kind } if kind == "system/commands_changed" => {
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
            AcpEvent::AgentMessageChunk(chunk) => Some(chunk.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "Hello there");
    assert!(seen.iter().any(|event| matches!(
        event,
        AcpEvent::TokenUsageBreakdown {
            input_tokens: 100,
            output_tokens: 20,
            ..
        }
    )));
    assert_eq!(
        seen.last(),
        Some(&AcpEvent::TurnEnded {
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
                == AcpEvent::OtherSessionUpdate {
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
            .any(|event| matches!(event, AcpEvent::AgentMessageChunk(_)))
    );
    client.shutdown().expect("fixture should shut down cleanly");
}

#[test]
fn a_dead_agent_names_its_exit_and_its_last_words() {
    let (_client, events) = launch("death");
    _client.prompt("hello").expect("prompt is accepted");
    let message = loop {
        match next_event(&events) {
            AcpEvent::TransportError(message) => break message,
            AcpEvent::TurnEnded { .. } => panic!("a dead agent must not report a normal turn end"),
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
        if let AcpEvent::OtherSessionUpdate { kind } = next_event(&events) {
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
        if matches!(next_event(&events), AcpEvent::AgentMessageChunk(_)) {
            break;
        }
    }
    client.cancel().expect("cancel is accepted");
    let seen = drain_until_turn_end(&events);
    assert_eq!(
        seen.last(),
        Some(&AcpEvent::TurnEnded {
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
    let (_client, events) = ClaudeClient::launch_with_timeouts(
        fixture_launch("silent_turn"),
        std::env::temp_dir(),
        Duration::from_secs(10),
        Duration::from_millis(400),
        Duration::from_secs(60),
    )
    .expect("handshake succeeds; it is the turn that hangs");
    _client.prompt("hello").expect("prompt is accepted");
    let event = loop {
        match next_event(&events) {
            event @ (AcpEvent::Timeout { .. } | AcpEvent::TransportError(_)) => break event,
            _ => continue,
        }
    };
    assert!(
        matches!(
            event,
            AcpEvent::Timeout {
                operation: sirio_acp::TimeoutOperation::Prompt,
                ..
            }
        ),
        "an idle turn must be reported as a prompt timeout, got {event:?}"
    );
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
            .any(|event| matches!(event, AcpEvent::Timeout { .. })),
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
            AcpEvent::PermissionRequest {
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
            AcpEvent::ToolCallCompleted { status, .. } if status == "Completed"
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
            AcpEvent::PermissionRequest { .. } => saw_request = true,
            AcpEvent::AgentMessageChunk(_) if saw_request => saw_text_after = true,
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
        if let AcpEvent::PermissionRequest { request_id, .. } = next_event(&events) {
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
            AcpEvent::ToolCallCompleted { status, .. } if status == "Failed"
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
        if matches!(next_event(&events), AcpEvent::PermissionRequest { .. }) {
            break;
        }
    }
    // Answer nothing. The deadline the user missed is a refusal, not a
    // protocol fault: the session stays alive and the tool is denied.
    let seen = drain_until_turn_end(&events);
    assert!(seen.iter().any(|event| matches!(
        event,
        AcpEvent::ToolCallCompleted { status, .. } if status == "Failed"
    )));
    client.shutdown().expect("clean shutdown");
}

#[test]
fn a_structured_question_carries_its_prompt_and_its_options() {
    let (mut client, events) = launch("question");
    client.prompt("ask me").expect("prompt is accepted");
    let (request_id, question) = loop {
        if let AcpEvent::PermissionRequest {
            request_id,
            question,
            ..
        } = next_event(&events)
        {
            break (request_id, question.expect("a structured question"));
        }
    };
    assert_eq!(question.header, "Pick a branch");
    assert_eq!(question.prompt, "Which branch should this target?");
    client
        .respond_permission(request_id, "main")
        .expect("the answer reaches the agent");
    let seen = drain_until_turn_end(&events);
    assert!(
        seen.iter()
            .any(|event| matches!(event, AcpEvent::TurnEnded { .. }))
    );
    client.shutdown().expect("clean shutdown");
}

#[test]
fn approving_a_plan_moves_the_session_out_of_plan_mode() {
    let (mut client, events) = launch("exit_plan_mode");
    client.prompt("plan it").expect("prompt is accepted");
    let request_id = loop {
        if let AcpEvent::PermissionRequest {
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
            .any(|event| matches!(event, AcpEvent::AgentMessageChunk(_)))
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
            AcpEvent::ContextUsage(usage) => Some(usage.clone()),
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
            AcpEvent::ContextUsage(usage) => Some(usage.clone()),
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
    assert_eq!(id.len(), 36, "a v4-shaped id, got {id}");
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
