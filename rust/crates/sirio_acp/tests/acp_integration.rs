use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::thread::{self, ThreadId};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use futures::future::Either;
use futures::task::{ArcWake, waker_ref};
use sirio_acp::{AcpClient, AcpEvent, AgentCommand};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/acp_fixture.py");

fn launch_fixture(mode: &str) -> (AcpClient, sirio_acp::EventStream) {
    let cwd = std::env::temp_dir();
    let command = AgentCommand::new("python3").args([FIXTURE, mode]);
    AcpClient::launch(command, cwd).expect("fixture ACP agent should initialize")
}

fn launch_fixture_with_pid_file() -> (AcpClient, sirio_acp::EventStream, PathBuf) {
    let pid_path = std::env::temp_dir().join(format!(
        "sirio-acp-{}-{}.pid",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos()
    ));
    let cwd = std::env::temp_dir();
    let command =
        AgentCommand::new("python3").args([FIXTURE, "normal", pid_path.to_string_lossy().as_ref()]);
    let launched = AcpClient::launch(command, cwd).expect("fixture ACP agent should initialize");
    while !pid_path.exists() {
        thread::sleep(Duration::from_millis(1));
    }
    (launched.0, launched.1, pid_path)
}

#[cfg(not(windows))]
fn process_exists(pid: &str) -> bool {
    ProcessCommand::new("ps")
        .args(["-p", pid, "-o", "pid="])
        .output()
        .map(|output| !String::from_utf8_lossy(&output.stdout).trim().is_empty())
        .unwrap_or(false)
}

#[cfg(windows)]
fn process_exists(pid: &str) -> bool {
    let filter = format!("PID eq {pid}");
    ProcessCommand::new("tasklist")
        .args(["/FI", &filter, "/NH"])
        .output()
        .map(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains(pid)
        })
        .unwrap_or(false)
}

fn next_event(events: &sirio_acp::EventStream) -> AcpEvent {
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

struct WakeProbe {
    threads: Mutex<Vec<ThreadId>>,
}

impl ArcWake for WakeProbe {
    fn wake_by_ref(probe: &Arc<Self>) {
        probe
            .threads
            .lock()
            .expect("wake probe is not poisoned")
            .push(thread::current().id());
    }
}

#[test]
fn event_stream_closes_on_the_client_thread_not_the_acp_worker_thread() {
    let (mut client, events) = launch_fixture("normal");
    let owner = thread::current().id();
    let probe = Arc::new(WakeProbe {
        threads: Mutex::new(Vec::new()),
    });
    let waker = waker_ref(&probe);
    let mut receive = Box::pin(events.recv());
    let mut context = Context::from_waker(&waker);

    assert!(matches!(receive.as_mut().poll(&mut context), Poll::Pending));
    client.shutdown().expect("fixture should shut down cleanly");

    let wake_threads = probe.threads.lock().expect("wake probe is not poisoned");
    assert!(
        wake_threads.iter().all(|thread| *thread == owner),
        "event receiver was woken from a non-owner thread: {wake_threads:?}"
    );
}

#[test]
fn shutdown_reaps_acp_process() {
    let (mut client, _events, pid_path) = launch_fixture_with_pid_file();
    let pid = std::fs::read_to_string(&pid_path).expect("fixture should publish its pid");
    assert!(
        process_exists(&pid),
        "fixture process should be live before shutdown"
    );
    eprintln!(
        "ps ACP fixture count before shutdown: 1 (pid {})",
        pid.trim()
    );

    client.shutdown().expect("fixture should shut down cleanly");
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while process_exists(&pid) && std::time::Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    eprintln!(
        "ps ACP fixture count after shutdown: {}",
        usize::from(process_exists(&pid))
    );
    assert!(
        !process_exists(&pid),
        "fixture process should be reaped after shutdown"
    );
    let _ = std::fs::remove_file(pid_path);
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

#[test]
fn advertises_commands_and_effort_and_echoes_prompt_blocks() {
    let (mut client, events) = launch_fixture("blocks");

    let mut commands = None;
    let mut effort = None;
    while commands.is_none() || effort.is_none() {
        match next_event(&events) {
            AcpEvent::AvailableCommands(list) => commands = Some(list),
            AcpEvent::Effort(option) => effort = Some(option),
            AcpEvent::ModelCatalog(_) => {}
            other => panic!("unexpected startup event: {other:?}"),
        }
    }
    assert_eq!(
        commands.expect("commands arrived"),
        vec![
            sirio_acp::AvailableCommandInfo {
                name: "cr".into(),
                description: "Code review the diff".into(),
            },
            sirio_acp::AvailableCommandInfo {
                name: "research".into(),
                description: "Research a topic".into(),
            },
        ]
    );
    let effort = effort.expect("effort arrived");
    assert_eq!(effort.option_id, "effort");
    assert_eq!(effort.current_value.as_deref(), Some("medium"));
    assert_eq!(effort.choices.len(), 3);

    // A composed prompt goes over the wire as separate blocks: trimmed text,
    // one resource link per mentioned path (relative paths resolved against
    // the session cwd, absolute as-is), and one image block per attachment.
    let cwd = std::env::temp_dir();
    client
        .prompt_content(
            "  hello world  ",
            vec!["sub/notes.md".into(), "/abs/file.png".into()],
            vec![sirio_acp::ImageAttachment {
                mime_type: "image/png".into(),
                base64_data: "AAAA".into(),
            }],
            &cwd,
        )
        .expect("content prompt accepted");

    let mut echo = None;
    while echo.is_none() {
        match next_event(&events) {
            AcpEvent::AgentMessageChunk(text) => echo = Some(text),
            AcpEvent::TurnEnded { .. } => break,
            AcpEvent::TransportError(error) => panic!("content prompt failed: {error}"),
            other => panic!("unexpected prompt event: {other:?}"),
        }
    }
    let echo = echo.expect("the fixture echoes the received blocks");
    // Join through PathBuf (not string interpolation): temp_dir() may carry a
    // trailing slash on some platforms, and the product resolves relative
    // links with PathBuf::join, which normalizes it away.
    let notes_link_target = cwd.join("sub/notes.md");

    // The `file://` spelling is rebuilt here rather than borrowed from the
    // product, deliberately: an expectation computed with the very function
    // under test would agree with itself however wrong that function became.
    // A POSIX path already starts with `/`, so `file://` + path is the
    // three-slash form; a Windows path starts with a drive and separates
    // with `\`, so it needs the third slash and forward separators.
    fn expected_file_uri(path: &std::path::Path) -> String {
        if cfg!(windows) {
            format!("file:///{}", path.display().to_string().replace('\\', "/"))
        } else {
            format!("file://{}", path.display())
        }
    }
    // `/abs/file.png` is absolute on unix but not on Windows, where the
    // product resolves it against the session cwd — mirror that, do not
    // hardcode one platform's answer.
    let png = std::path::Path::new("/abs/file.png");
    let png_target = if png.is_absolute() {
        png.to_path_buf()
    } else {
        cwd.join(png)
    };

    assert_eq!(
        echo,
        format!(
            "text:hello world|link:{}:notes.md|link:{}:file.png|image:image/png",
            expected_file_uri(&notes_link_target),
            expected_file_uri(&png_target)
        )
    );

    client
        .shutdown()
        .expect("blocks fixture should shut down cleanly");
}

#[test]
fn plan_updates_and_approval_flow_as_events() {
    let (mut client, events) = launch_fixture("plan");
    client
        .prompt("plan this")
        .expect("prompt should be accepted");

    let mut saw_first_plan = false;
    let mut saw_approval = false;
    let mut saw_advanced_plan = false;
    let mut ended = false;

    while !ended {
        match next_event(&events) {
            AcpEvent::PlanUpdate { entries } => {
                let statuses = entries
                    .iter()
                    .map(|entry| entry.status.as_str())
                    .collect::<Vec<_>>();
                if statuses == ["pending", "pending"] {
                    assert_eq!(entries[0].content, "Read the design");
                    saw_first_plan = true;
                } else {
                    assert_eq!(statuses, ["completed", "in_progress"]);
                    saw_advanced_plan = true;
                }
            }
            AcpEvent::PermissionRequest {
                request_id,
                title,
                options,
                ..
            } => {
                assert_eq!(title, "Exit plan mode");
                assert_eq!(options.len(), 2);
                assert_eq!(options[0].id, "approve");
                assert!(options[1].kind.contains("Reject"));
                client
                    .respond_permission(request_id, "approve")
                    .expect("approval should be sent");
                saw_approval = true;
            }
            AcpEvent::AgentMessageChunk(text) => assert!(text.contains("approved")),
            AcpEvent::TurnEnded { .. } => ended = true,
            AcpEvent::TransportError(error) => panic!("plan fixture failed: {error}"),
            other => panic!("unexpected plan event: {other:?}"),
        }
    }

    assert!(saw_first_plan, "the initial plan update arrived");
    assert!(saw_approval, "the approval permission arrived");
    assert!(saw_advanced_plan, "the plan advanced after the approval");
    client
        .shutdown()
        .expect("plan fixture should shut down cleanly");
}

#[test]
fn a_pre_rename_agent_still_gets_its_text_input_affordance() {
    // `_tillerTextInput` is a key agents write, not one this app controls, so
    // an agent that predates the Sirio rename keeps declaring it. Reading only
    // the new spelling would drop the text field with no error anywhere.
    let (mut client, events) = launch_fixture("question_legacy");
    client
        .prompt("which color?")
        .expect("prompt should be accepted");

    let mut saw_placeholder = false;
    let mut ended = false;
    while !ended {
        match next_event(&events) {
            AcpEvent::PermissionRequest {
                request_id,
                question,
                ..
            } => {
                let question = question.expect("the structured question is parsed");
                assert_eq!(
                    question
                        .text_input
                        .as_ref()
                        .and_then(|input| input.placeholder.as_deref()),
                    Some("Type a color"),
                    "the pre-rename key must still declare the affordance"
                );
                saw_placeholder = true;
                client
                    .respond_permission(request_id, "Blue")
                    .expect("the typed answer should be sent");
            }
            AcpEvent::AgentMessageChunk(_) => {}
            AcpEvent::TurnEnded { .. } => ended = true,
            AcpEvent::TransportError(error) => panic!("legacy question fixture failed: {error}"),
            other => panic!("unexpected question event: {other:?}"),
        }
    }
    assert!(saw_placeholder, "the legacy text input was never surfaced");
    client
        .shutdown()
        .expect("legacy question fixture should shut down cleanly");
}

#[test]
fn structured_question_carries_title_and_text_input() {
    let (mut client, events) = launch_fixture("question");
    client
        .prompt("which color?")
        .expect("prompt should be accepted");

    let mut answered = false;
    let mut ended = false;
    while !ended {
        match next_event(&events) {
            AcpEvent::PermissionRequest {
                request_id,
                title,
                options,
                question,
                ..
            } => {
                assert_eq!(title, "Ask user question");
                assert!(options.is_empty(), "the question carries no wire options");
                let question = question.expect("the structured question is parsed");
                assert_eq!(question.header, "Which color?");
                assert_eq!(question.prompt, "Which color should the button be?");
                assert_eq!(
                    question
                        .text_input
                        .as_ref()
                        .and_then(|input| input.placeholder.as_deref()),
                    Some("Type a color")
                );
                client
                    .respond_permission(request_id, "Blue")
                    .expect("the typed answer should be sent");
                answered = true;
            }
            AcpEvent::AgentMessageChunk(text) => {
                assert!(text.contains("You chose: Blue"));
            }
            AcpEvent::TurnEnded { .. } => ended = true,
            AcpEvent::TransportError(error) => panic!("question fixture failed: {error}"),
            other => panic!("unexpected question event: {other:?}"),
        }
    }
    assert!(answered, "the question was answered with text");
    client
        .shutdown()
        .expect("question fixture should shut down cleanly");
}

#[test]
fn cancel_permission_withdraws_without_selecting() {
    let (mut client, events) = launch_fixture("cancel_permission_direct");
    client
        .prompt("wait for the withdrawal")
        .expect("prompt should be accepted");

    let mut saw_permission = false;
    let mut ended = false;
    while !ended {
        match next_event(&events) {
            AcpEvent::PermissionRequest { request_id, .. } => {
                client
                    .cancel_permission(request_id)
                    .expect("permission withdrawal should be sent");
                saw_permission = true;
            }
            AcpEvent::TurnEnded { .. } => ended = true,
            AcpEvent::TransportError(error) => panic!("permission withdrawal failed: {error}"),
            other => panic!("unexpected withdrawal event: {other:?}"),
        }
    }
    assert!(saw_permission, "the permission request arrived");
    client
        .shutdown()
        .expect("withdrawal fixture should shut down cleanly");
}

#[cfg(target_os = "linux")]
fn live_thread_count() -> usize {
    std::fs::read_dir("/proc/self/task")
        .expect("/proc/self/task should be readable")
        .count()
}

#[cfg(target_os = "macos")]
fn live_thread_count() -> usize {
    let output = ProcessCommand::new("ps")
        .args(["-M", &std::process::id().to_string()])
        .output()
        .expect("ps -M should run");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .count()
        .saturating_sub(1)
}

#[cfg(not(windows))]
fn drive_turn(client: &AcpClient, events: &sirio_acp::EventStream) {
    client.prompt("count threads").expect("prompt accepted");
    loop {
        match next_event(events) {
            AcpEvent::PermissionRequest { request_id, .. } => client
                .respond_permission(request_id, "deny")
                .expect("permission denial should be sent"),
            AcpEvent::TurnEnded { .. } => break,
            AcpEvent::TransportError(error) => panic!("unexpected transport error: {error}"),
            _ => {}
        }
    }
}

#[cfg(not(windows))]
#[test]
fn completed_prompts_do_not_leave_timeout_threads_behind() {
    let (mut client, events) = launch_fixture("multi");
    drive_turn(&client, &events);
    thread::sleep(Duration::from_millis(300));
    let baseline = live_thread_count();

    const TURNS: usize = 5;
    for _ in 0..TURNS {
        drive_turn(&client, &events);
    }
    thread::sleep(Duration::from_millis(300));
    let after = live_thread_count();

    client.shutdown().expect("fixture should shut down cleanly");
    assert!(
        after <= baseline + 1,
        "{TURNS} completed prompts left {} threads alive (baseline {baseline}, after {after})",
        after.saturating_sub(baseline)
    );
}
