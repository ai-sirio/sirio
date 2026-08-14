use std::time::Duration;

use futures::future::Either;
use tiller_acp::{AcpClient, AcpEvent, AgentCommand};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/acp_fixture.py");

fn launch_fixture(mode: &str) -> (AcpClient, tiller_acp::EventStream) {
    let cwd = std::env::temp_dir();
    let command = AgentCommand::new("python3").args([FIXTURE, mode]);
    AcpClient::launch(command, cwd).expect("fixture ACP agent should initialize")
}

fn next_event(events: &tiller_acp::EventStream) -> AcpEvent {
    let receive = events.recv();
    let timer = async_io::Timer::after(Duration::from_secs(2));
    futures::executor::block_on(async move {
        futures::pin_mut!(receive, timer);
        match futures::future::select(receive, timer).await {
            Either::Left((event, _)) => event.expect("fixture event stream should remain open"),
            Either::Right((_, _)) => panic!("fixture did not emit an event within two seconds"),
        }
    })
}

#[test]
fn streams_chunks_tool_and_permission_denial() {
    let (mut client, events) = launch_fixture("normal");
    client
        .prompt("exercise the fixture")
        .expect("prompt accepted");

    let mut chunks = Vec::new();
    let mut permission = None;
    let mut saw_tool_start = false;
    let mut saw_tool_completion = false;
    let mut turn_ended = false;

    while !turn_ended {
        match next_event(&events) {
            AcpEvent::AgentMessageChunk(text) => chunks.push(text),
            AcpEvent::ToolCallStarted { id, .. } => {
                assert_eq!(id, "tool-1");
                saw_tool_start = true;
            }
            AcpEvent::ToolCallCompleted { id, status, .. } => {
                assert_eq!(id, "tool-1");
                assert_eq!(status, "Completed");
                saw_tool_completion = true;
            }
            AcpEvent::PermissionRequest {
                request_id,
                options,
                ..
            } => {
                assert_eq!(options.len(), 2);
                assert_eq!(options[1].id, "deny");
                client
                    .respond_permission(request_id, "deny")
                    .expect("permission denial should be sent");
                permission = Some(request_id);
            }
            AcpEvent::TurnEnded { stop_reason } => {
                assert_eq!(stop_reason, "EndTurn");
                turn_ended = true;
            }
            AcpEvent::TransportError(error) => panic!("unexpected transport error: {error}"),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    assert_eq!(permission, Some(1));
    assert_eq!(chunks, ["first ", "streamed", " denied"]);
    assert!(saw_tool_start);
    assert!(saw_tool_completion);
    client.shutdown().expect("fixture should shut down cleanly");
}

#[test]
fn malformed_frame_is_rejected_without_hanging_the_session() {
    let (mut client, events) = launch_fixture("malformed");
    client
        .prompt("malform the next frame")
        .expect("prompt accepted");
    let mut recovered = false;
    loop {
        match next_event(&events) {
            AcpEvent::AgentMessageChunk(text) => {
                assert_eq!(text, "after malformed");
                recovered = true;
            }
            AcpEvent::TurnEnded { stop_reason } => {
                assert_eq!(stop_reason, "EndTurn");
                break;
            }
            AcpEvent::TransportError(error) => {
                panic!("malformed frame failed the session: {error}")
            }
            other => panic!("unexpected malformed-frame event: {other:?}"),
        }
    }
    assert!(
        recovered,
        "the session must report an explicit post-rejection outcome"
    );
    client
        .shutdown()
        .expect("malformed-frame fixture should shut down cleanly");
}

#[test]
fn agent_death_is_transport_error() {
    let (mut client, events) = launch_fixture("death");
    client
        .prompt("die during this turn")
        .expect("prompt accepted");
    let mut saw_partial_chunk = false;
    let message = loop {
        match next_event(&events) {
            AcpEvent::AgentMessageChunk(text) => {
                assert_eq!(text, "partial");
                saw_partial_chunk = true;
            }
            AcpEvent::TransportError(message) => break message,
            AcpEvent::Timeout {
                operation,
                duration,
            } => {
                panic!("unexpected {operation:?} timeout after {duration:?}")
            }
            other => panic!("unexpected death event: {other:?}"),
        }
    };
    assert!(
        saw_partial_chunk,
        "agent death must be observed after streamed output"
    );
    assert!(
        message.contains("closed") || message.contains("EOF") || message.contains("end of file"),
        "unexpected death message: {message}"
    );
    let _ = client.shutdown();
}

#[test]
fn cancel_produces_a_terminal_outcome() {
    let (mut client, events) = launch_fixture("cancel");
    client
        .prompt("wait for cancellation")
        .expect("prompt accepted");
    std::thread::sleep(Duration::from_millis(50));
    client.cancel().expect("cancel accepted");

    loop {
        match next_event(&events) {
            AcpEvent::TurnEnded { stop_reason } => {
                assert_eq!(stop_reason, "Cancelled");
                break;
            }
            AcpEvent::TransportError(error) => panic!("cancel failed: {error}"),
            _ => {}
        }
    }
    client
        .shutdown()
        .expect("cancelled fixture should shut down cleanly");
}

#[test]
fn cancel_while_permission_is_pending_produces_a_terminal_outcome() {
    let (mut client, events) = launch_fixture("cancel_permission");
    client
        .prompt("wait for permission cancellation")
        .expect("prompt accepted");

    let mut saw_permission = false;
    loop {
        match next_event(&events) {
            AcpEvent::PermissionRequest { .. } => {
                saw_permission = true;
                client.cancel().expect("cancel accepted");
            }
            AcpEvent::TurnEnded { stop_reason } => {
                assert_eq!(stop_reason, "Cancelled");
                break;
            }
            AcpEvent::TransportError(error) => panic!("permission cancel failed: {error}"),
            _ => {}
        }
    }
    assert!(
        saw_permission,
        "cancellation must cover a pending permission request"
    );
    client
        .shutdown()
        .expect("permission-cancelled fixture should shut down cleanly");
}
