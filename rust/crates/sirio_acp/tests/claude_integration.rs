//! The native Claude client against a deterministic fake `claude`.
//!
//! Test for test, this mirrors `acp_integration.rs`: the two transports owe
//! the surface the same behaviour, so they are held to the same list.

use std::path::PathBuf;
use std::time::Duration;

use futures::future::Either;
use sirio_acp::{AcpEvent, ClaudeClient, ClaudeLaunch};

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
    let timer = async_io::Timer::after(Duration::from_secs(5));
    futures::executor::block_on(async move {
        futures::pin_mut!(receive, timer);
        match futures::future::select(receive, timer).await {
            Either::Left((event, _)) => event.expect("event stream should remain open"),
            Either::Right((_, _)) => panic!("fixture did not emit an event within five seconds"),
        }
    })
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
fn the_session_id_becomes_readable_and_is_announced_once() {
    let (mut client, events) = launch("normal");
    assert_eq!(client.session_id(), None, "no id before the first turn");
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
    assert_eq!(client.session_id().as_deref(), Some("fixture-session-1"));
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
    client.shutdown().expect("clean shutdown");
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
    let (mut client, events) = ClaudeClient::launch_with_timeouts(
        fixture_launch("chatty_slow_turn"),
        std::env::temp_dir(),
        Duration::from_secs(10),
        Duration::from_millis(400),
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
    client.shutdown().expect("clean shutdown");
}
