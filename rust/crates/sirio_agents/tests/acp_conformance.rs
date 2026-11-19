//! Does the CLI on this machine actually answer ACP?
//!
//! This is the test that would have caught the original defect: two
//! adapters denied a capability their CLI had, and nothing in the suite
//! could notice because nothing ever launched them. It SKIPs when the
//! binary is absent — in the style `Scripts/ci-linux.sh` already uses for
//! stages allowed to SKIP — so an unverified claim can never turn into a
//! false green.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use sirio_agents::{AgentAdapter, OhMyPiAdapter, OpenCodeAdapter};

mod claude_live;

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false}}}}"#;
// Cold-starting a CLI can take a few seconds, but a missing handshake must not hang the suite.
const ACP_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, PartialEq, Eq)]
enum Handshake {
    Answered(String),
    Silent,
    NotLaunched,
    TimedOut { output: String },
}

enum ReaderMessage {
    Answered(String),
    Skipped(String),
    End,
}

fn answers_initialize(program: &str, args: &[&str]) -> Handshake {
    answers_initialize_within(program, args, ACP_HANDSHAKE_TIMEOUT)
}

fn answers_initialize_within(program: &str, args: &[&str], timeout: Duration) -> Handshake {
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Handshake::NotLaunched;
    };

    let wrote_initialize = child.stdin.as_mut().is_some_and(|stdin| {
        stdin
            .write_all(format!("{INITIALIZE}\n").as_bytes())
            .is_ok()
    });
    let result = if wrote_initialize {
        child
            .stdout
            .take()
            .map_or(Handshake::Silent, |stdout| read_handshake(stdout, timeout))
    } else {
        Handshake::Silent
    };
    let _ = child.kill();
    // Reap it too: `kill` only signals, and a test binary that leaves a
    // zombie behind for every CLI it probes is a slow leak in the suite.
    let _ = child.wait();
    result
}

fn read_handshake(stdout: std::process::ChildStdout, timeout: Duration) -> Handshake {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            let read = match reader.read_line(&mut line) {
                Ok(read) => read,
                Err(_) => {
                    let _ = tx.send(ReaderMessage::End);
                    return;
                }
            };
            if read == 0 {
                let _ = tx.send(ReaderMessage::End);
                return;
            }
            if line.trim_start().starts_with('{') {
                let _ = tx.send(ReaderMessage::Answered(line));
                return;
            }
            let _ = tx.send(ReaderMessage::Skipped(std::mem::take(&mut line)));
        }
    });

    let deadline = Instant::now() + timeout;
    let mut output = String::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining) {
            Ok(ReaderMessage::Answered(line)) => break Handshake::Answered(line),
            Ok(ReaderMessage::Skipped(line)) => output.push_str(&line),
            Ok(ReaderMessage::End) | Err(RecvTimeoutError::Disconnected) => {
                break Handshake::Silent;
            }
            Err(RecvTimeoutError::Timeout) => break Handshake::TimedOut { output },
        }
    }
}

#[test]
fn opencode_answers_the_acp_handshake_it_claims() {
    let Some(program) = OpenCodeAdapter.availability().executable else {
        eprintln!("SKIP: opencode is not on PATH");
        return;
    };
    let claim = OpenCodeAdapter
        .builtin_acp()
        .expect("opencode claims an in-binary ACP server");
    let response = match answers_initialize(&program.to_string_lossy(), claim.args) {
        Handshake::Answered(response) => response,
        Handshake::Silent => panic!("opencode acp answered nothing on stdout"),
        Handshake::NotLaunched => panic!("opencode acp could not be launched"),
        Handshake::TimedOut { output } => {
            panic!("opencode acp initialize handshake timed out; stdout before timeout: {output:?}")
        }
    };
    assert!(
        response.contains("\"protocolVersion\""),
        "expected an ACP initialize result, got: {response}"
    );
    assert!(
        response.contains("OpenCode"),
        "expected agentInfo naming OpenCode: {response}"
    );
}

#[test]
fn oh_my_pi_is_only_claimed_once_it_answers() {
    let Some(program) = OhMyPiAdapter.availability().executable else {
        eprintln!("SKIP: omp is not on PATH — the claim stays None until it can be checked");
        assert_eq!(
            OhMyPiAdapter.builtin_acp(),
            None,
            "an unverifiable claim must not ship as Some"
        );
        return;
    };
    // omp is present: the ship gate can be settled either way, and the
    // adapter must agree with what the binary actually does.
    let answered = match answers_initialize(&program.to_string_lossy(), &["acp"]) {
        Handshake::Answered(response) => response.contains("\"protocolVersion\""),
        Handshake::Silent => false,
        Handshake::NotLaunched => panic!("omp acp could not be launched"),
        Handshake::TimedOut { output } => {
            panic!("omp acp initialize handshake timed out; stdout before timeout: {output:?}")
        }
    };
    assert_eq!(
        OhMyPiAdapter.builtin_acp().is_some(),
        answered,
        "the adapter's claim and the binary's behaviour must match"
    );
}

/// The native claim's counterpart to `opencode_answers_the_acp_handshake_it_claims`.
///
/// `ClaudeCodeAdapter::native_chat()` says the `claude` on this machine
/// answers a stdio control handshake at or above a named version. That is a
/// claim about an undocumented protocol on a CLI that self-updates, which
/// makes it exactly the kind of claim that rots silently. This is the test
/// that says so on the day it does.
#[test]
fn claude_answers_the_native_handshake_it_claims() {
    use sirio_agents::ClaudeCodeAdapter;

    let claim = ClaudeCodeAdapter
        .native_chat()
        .expect("Claude Code claims a native chat transport");
    let Some(program) = ClaudeCodeAdapter.availability().executable else {
        eprintln!("SKIP: claude is not on PATH");
        return;
    };
    let program = program.to_string_lossy().into_owned();

    // The floor first: a machine with an older claude is a machine whose
    // chats fall back, and the handshake below is not owed there.
    let version_output = std::process::Command::new(&program)
        .arg("--version")
        .output()
        .expect("claude --version runs");
    let version_text = String::from_utf8_lossy(&version_output.stdout)
        .trim()
        .to_string();
    let version = sirio_claude::ClaudeVersion::parse(&version_text)
        .unwrap_or_else(|| panic!("claude reported an unreadable version: {version_text:?}"));
    if !version.meets_floor() {
        eprintln!(
            "SKIP: claude {version} is below the claimed floor {}",
            claim.min_version
        );
        return;
    }

    // `--setting-sources ""` and `--no-session-persistence` keep this inert:
    // it starts no MCP server, runs no hook, and writes no session file on
    // the machine running the suite.
    let line = sirio_claude::LaunchLine::usage_probe();
    let mut args: Vec<&str> = line.args.iter().map(String::as_str).collect();
    args.push("--permission-prompt-tool");
    args.push("stdio");
    // Every chat launch passes it; a CLI that stopped knowing it would
    // refuse to start any chat at all.
    args.push("--allow-dangerously-skip-permissions");
    let handshake = sirio_claude::ControlRequest::initialize("conformance-1").to_string();

    let response = match answers_stdio_handshake(&program, &args, &handshake) {
        Handshake::Answered(response) => response,
        Handshake::Silent => panic!("claude answered nothing on stdout"),
        Handshake::NotLaunched => panic!("claude could not be launched"),
        Handshake::TimedOut { output } => {
            panic!("claude's handshake timed out; stdout before timeout: {output:?}")
        }
    };
    assert!(
        response.contains("\"control_response\""),
        "expected a control response, got: {response}"
    );
    assert!(
        response.contains("\"request_id\":\"conformance-1\""),
        "the response must answer the request it was sent: {response}"
    );
}

/// The session panel's two reads, on the `claude` of this machine.
///
/// The context breakdown and the connector list are control requests of an
/// undocumented protocol on a CLI that self-updates, and everything the panel
/// draws is read from their answers. A CLI that renamed a field would give a
/// panel with no rows and no error, so this says so on the day it does.
#[test]
fn claude_answers_the_session_panel_requests() {
    use sirio_agents::ClaudeCodeAdapter;

    let Some(program) = ClaudeCodeAdapter.availability().executable else {
        eprintln!("SKIP: claude is not on PATH");
        return;
    };
    let program = program.to_string_lossy().into_owned();
    let version_output = Command::new(&program)
        .arg("--version")
        .output()
        .expect("claude --version runs");
    let version_text = String::from_utf8_lossy(&version_output.stdout)
        .trim()
        .to_string();
    let Some(version) = sirio_claude::ClaudeVersion::parse(&version_text) else {
        panic!("claude reported an unreadable version: {version_text:?}");
    };
    if !version.meets_floor() {
        eprintln!("SKIP: claude {version} is below the claimed floor");
        return;
    }

    // Inert, as the handshake test above: no MCP server started, no hook
    // run, no session file written.
    let line = sirio_claude::LaunchLine::usage_probe();
    let mut child = Command::new(&program)
        .args(&line.args)
        .args(["--permission-prompt-tool", "stdio"])
        .arg("--allow-dangerously-skip-permissions")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("claude launches");
    let mut stdin = child.stdin.take().expect("a piped stdin");
    let stdout = child.stdout.take().expect("a piped stdout");
    let (line_tx, line_rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line_tx.send(line).is_err() {
                return;
            }
        }
    });

    for request in [
        sirio_claude::ControlRequest::initialize("panel-init"),
        sirio_claude::ControlRequest::get_context_usage("panel-context"),
        sirio_claude::ControlRequest::mcp_status("panel-mcp"),
    ] {
        writeln!(stdin, "{request}").expect("the request is written");
    }
    stdin.flush().expect("the requests are flushed");

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut context = None;
    let mut mcp = None;
    while context.is_none() || mcp.is_none() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let Ok(line) = line_rx.recv_timeout(remaining) else {
            break;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let Some(envelope) = sirio_claude::ControlEnvelope::parse(&value) else {
            continue;
        };
        match envelope.request_id.as_str() {
            "panel-context" => context = Some(envelope),
            "panel-mcp" => mcp = Some(envelope),
            _ => {}
        }
    }
    let _ = child.kill();
    let _ = child.wait();

    let context = context.expect("claude answered get_context_usage");
    let payload = context
        .payload
        .unwrap_or_else(|| panic!("get_context_usage was refused: {:?}", context.error));
    let breakdown = sirio_claude::ContextBreakdown::parse(&payload)
        .unwrap_or_else(|| panic!("the breakdown could not be read from {payload}"));
    assert!(
        !breakdown.categories.is_empty(),
        "the breakdown names no categories: {payload}"
    );

    let mcp = mcp.expect("claude answered mcp_status");
    let payload = mcp
        .payload
        .unwrap_or_else(|| panic!("mcp_status was refused: {:?}", mcp.error));
    assert!(
        sirio_claude::parse_mcp_status(&payload).is_some(),
        "the connector list could not be read from {payload}"
    );
}

/// The lines the trace waterfall is built from, on the `claude` of this machine.
///
/// The trace reads what the CLI streams as a turn runs: the main thread's model
/// calls (`status: requesting` as the request goes out, `message_start` as the first
/// token comes and names the model), a subagent's start (`task_started` naming the `Agent`
/// call), its own lines (an `assistant` line that carries the `Agent` call's id as its
/// parent) and the CLI's own clock for it (`totalDurationMs` in the `Agent` call's
/// result). None of it is a published contract, and `claude` self-updates: a CLI that
/// renamed one would draw a waterfall with no agents and no error, so this says so on
/// the day it does.
///
/// A second prompt, in the same process, has the model start a `fork` agent in the
/// background (`task_started` with `is_backgrounded: true`). The trace follows such an
/// agent until the CLI says it ended, so this reads on past the turn's `result`: the
/// agent's parented lines keep streaming, then `task_updated` carries a terminal
/// `patch.status` and `task_notification` names the agent's `tool_use_id`. The CLI starts
/// its own turn after that, which is why the read waits for the notification and not for
/// the next `result`.
///
/// It is the one conformance test that spends turns (real Haiku ones, in which a
/// subagent runs `ls`), so it is `#[ignore]`d and runs only on request, like the
/// `hooks_live` tests, and stays off the release gate:
///
///     cargo nextest run -p sirio_agents --test acp_conformance --run-ignored only claude_streams_the_lines_the_trace_reads
///
/// It SKIPs without `claude` and below the claimed floor, like its siblings, and fails when
/// the CLI is not logged in: a check that could not run must not read as one that passed. Its
/// scratch directory is under the target directory (`CARGO_TARGET_TMPDIR`).
#[test]
#[ignore = "spends one real turn"]
fn claude_streams_the_lines_the_trace_reads() {
    use sirio_agents::ClaudeCodeAdapter;
    use sirio_claude::{AgentToolResult, CliMessage, MessageLifecycle};

    let Some(program) = ClaudeCodeAdapter.availability().executable else {
        eprintln!("SKIP: claude is not on PATH");
        return;
    };
    let program = program.to_string_lossy().into_owned();
    let version_output = Command::new(&program)
        .arg("--version")
        .output()
        .expect("claude --version runs");
    let version_text = String::from_utf8_lossy(&version_output.stdout)
        .trim()
        .to_string();
    let Some(version) = sirio_claude::ClaudeVersion::parse(&version_text) else {
        panic!("claude reported an unreadable version: {version_text:?}");
    };
    if !version.meets_floor() {
        eprintln!("SKIP: claude {version} is below the claimed floor");
        return;
    }

    // The probe's flags keep the user's settings out and write no session file; the
    // rest keep this one turn small and its tools to the two it needs. Partial
    // messages are what put `message_start` on the stream. `--strict-mcp-config` with
    // no `--mcp-config` starts no MCP server, and `disableAllHooks` runs none.
    let line = sirio_claude::LaunchLine::usage_probe();
    let scratch = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("trace-live-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("a scratch directory");
    std::fs::write(scratch.join("README.md"), "scratch\n").expect("a file to list");
    let mut child = Command::new(&program)
        .args(&line.args)
        .args([
            "--include-partial-messages",
            "--model",
            "haiku",
            "--strict-mcp-config",
            "--settings",
            r#"{"disableAllHooks":true}"#,
            "--allowedTools",
            "Agent,Bash(ls:*)",
            "--max-turns",
            "12",
        ])
        .current_dir(&scratch)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("claude launches");
    let mut stdin = child.stdin.take().expect("a piped stdin");
    let stdout = child.stdout.take().expect("a piped stdout");
    let (line_tx, line_rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line_tx.send(line).is_err() {
                return;
            }
        }
    });

    let prompt = serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": [{"type": "text", "text":
            "Use the Agent tool exactly once, in the foreground: have the subagent run the \
             shell command ls and report the file names. Do not run anything yourself."}]},
    });
    writeln!(stdin, "{}", sirio_claude::ControlRequest::initialize("trace-init")).expect("initialize");
    writeln!(stdin, "{prompt}").expect("the prompt is written");
    stdin.flush().expect("the prompt is flushed");

    // Until the turn's `result`, the CLI's death or the deadline.
    let deadline = Instant::now() + Duration::from_secs(150);
    let mut messages = Vec::new();
    let mut result = None;
    while result.is_none() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let Ok(line) = line_rx.recv_timeout(remaining) else {
            break;
        };
        let Some(message) = CliMessage::parse(&line) else {
            continue;
        };
        if let CliMessage::Result(payload) = &message {
            result = Some(payload.clone());
        }
        messages.push(message);
    }

    // The background agent's turn, in the same process. What the trace reads of it starts
    // here, so the checks below look only at the lines from `background_from` on. Skipped
    // when the first turn failed: the checks on it say why.
    let background_from = messages.len();
    let mut background_started = None;
    // The `task_id` that `task_started` gave it: `task_updated` names the agent by this only.
    let mut background_task = None;
    let mut background_ended = false;
    if result.as_ref().is_some_and(|result| !result.is_error) {
        let background_prompt = serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": [{"type": "text", "text":
                "Use the Agent tool exactly once, with subagent_type \"fork\", so that it runs \
                 in the background: have it run the shell command ls and report the file \
                 names. Do not run anything yourself."}]},
        });
        writeln!(stdin, "{background_prompt}").expect("the background prompt is written");
        stdin.flush().expect("the background prompt is flushed");

        // Past the turn's `result`, until the notification that names the agent, the CLI's
        // death or the deadline: the CLI's own turn after it is not waited for.
        let deadline = Instant::now() + Duration::from_secs(240);
        while !background_ended {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let Ok(line) = line_rx.recv_timeout(remaining) else {
                break;
            };
            let Some(message) = CliMessage::parse(&line) else {
                continue;
            };
            if let CliMessage::System(system) = &message {
                match system.subtype.as_str() {
                    "task_started"
                        if background_started.is_none() && system.is_backgrounded == Some(true) =>
                    {
                        background_started = system.tool_use_id.clone();
                        background_task = system.task_id.clone();
                    }
                    "task_notification" => {
                        background_ended = background_started.is_some()
                            && system.tool_use_id == background_started;
                    }
                    _ => {}
                }
            }
            messages.push(message);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&scratch);

    let Some(result) = result else {
        panic!(
            "no `result` line within the deadline; the kinds of the {} lines read were {:?}",
            messages.len(),
            messages.iter().map(kind_of).collect::<Vec<_>>()
        );
    };
    if result.is_error {
        let text = result.result.clone().unwrap_or_default().to_lowercase();
        // A logged-out CLI is a machine that cannot answer this check, not one that has
        // nothing to say: it fails, and says what to do, rather than passing for the lines it
        // never streamed.
        assert!(
            !(text.contains("not logged in") || text.contains("please run /login")),
            "claude is not logged in, so the trace's lines were not checked: run `claude /login` \
             and run this test again"
        );
    }

    let main_message_start = messages.iter().position(|message| match message {
        CliMessage::StreamEvent(event) => {
            event.parent_tool_use_id.is_none()
                && matches!(event.lifecycle(), Some(MessageLifecycle::Start { .. }))
        }
        _ => false,
    });
    let Some(main_message_start) = main_message_start else {
        panic!("no `message_start` on the main thread: the trace could not name a model call");
    };

    // A main-thread call starts when its request goes out: `status: requesting` (a
    // subagent's requests send none). `message_start` is the API's first token, seconds
    // later, and a CLI that stopped sending the line would draw every call late.
    let main_request = messages.iter().position(|message| {
        matches!(
            message,
            CliMessage::System(system)
                if system.subtype == "status" && system.status.as_deref() == Some("requesting")
        )
    });
    assert!(
        main_request.is_some_and(|request| request < main_message_start),
        "no `system` line with subtype `status` and status `requesting` ahead of the first \
         main-thread `message_start` (the request was at line {main_request:?}, the first token \
         at line {main_message_start}): the trace could not start a model call when its \
         request goes out"
    );

    let started_agent = messages.iter().find_map(|message| match message {
        CliMessage::System(system)
            if system.subtype == "task_started" && system.tool_use_id.is_some() =>
        {
            system.tool_use_id.clone()
        }
        _ => None,
    });
    let started_agent = started_agent.expect(
        "no `task_started` naming the `Agent` call by `tool_use_id`: the trace could not name a subagent",
    );

    assert!(
        messages.iter().any(|message| matches!(
            message,
            CliMessage::Assistant(assistant)
                if assistant.parent_tool_use_id.as_deref() == Some(started_agent.as_str())
        )),
        "no `assistant` line parented to the `Agent` call {started_agent}: a subagent's calls could not be timed"
    );

    let measured = messages.iter().any(|message| match message {
        CliMessage::User(user) => user
            .tool_use_result
            .as_ref()
            .and_then(AgentToolResult::parse)
            .is_some_and(|totals| totals.total_duration_ms.is_some()),
        _ => false,
    });
    assert!(
        measured,
        "no `Agent` result with `totalDurationMs`: the CLI's own clock for a subagent is gone; \
         the `tool_use_result`s read were {:?}",
        messages
            .iter()
            .filter_map(|message| match message {
                CliMessage::User(user) => user.tool_use_result.as_ref(),
                _ => None,
            })
            .map(|value| value.to_string().chars().take(300).collect::<String>())
            .collect::<Vec<_>>()
    );

    // The background agent: the trace follows it past the turn's `result` until the CLI says
    // it ended, and each of these is a line it reads.
    let background = &messages[background_from..];
    let background_kinds = || background.iter().map(kind_of).collect::<Vec<_>>();
    let Some(background_agent) = background_started else {
        panic!(
            "no `task_started` with `is_backgrounded: true` for the `fork` agent: the trace could \
             not tell a background agent from a foreground one; the kinds of the {} lines read \
             were {:?}",
            background.len(),
            background_kinds()
        );
    };
    let background_result = background
        .iter()
        .position(|message| matches!(message, CliMessage::Result(_)))
        .unwrap_or_else(|| {
            panic!(
                "no `result` for the turn that started the background agent {background_agent}; \
                 the kinds of the lines read were {:?}",
                background_kinds()
            )
        });
    assert!(
        background[background_result..].iter().any(|message| match message {
            CliMessage::Assistant(assistant) => {
                assistant.parent_tool_use_id.as_deref() == Some(background_agent.as_str())
            }
            CliMessage::User(user) => {
                user.parent_tool_use_id.as_deref() == Some(background_agent.as_str())
            }
            _ => false,
        }),
        "no line parented to the background agent {background_agent} after its turn's `result`: \
         the agent's work after the main thread finished could not be traced"
    );
    assert!(
        background_task.is_some(),
        "the `task_started` of the background agent {background_agent} carried no `task_id`: \
         the trace could not tell which agent a `task_updated` ends"
    );
    assert!(
        background.iter().any(|message| matches!(
            message,
            CliMessage::System(system)
                if system.subtype == "task_updated"
                    && system.task_id == background_task
                    && system
                        .patch
                        .as_ref()
                        .and_then(|patch| patch.status.as_deref())
                        .is_some_and(|status| {
                            matches!(status, "completed" | "killed" | "stopped" | "failed")
                        })
        )),
        "no `task_updated` naming task {background_task:?} (the background agent \
         {background_agent}) with a terminal `patch.status`: its end was not announced, so its \
         lane would end only at the notification"
    );
    assert!(
        background_ended,
        "no `task_notification` naming the background agent {background_agent} within the \
         deadline: the trace would keep its turn live for ever; the kinds of the {} lines read \
         were {:?}",
        background.len(),
        background_kinds()
    );
}

/// The kind of a message, for the failure that read none of what it wanted.
fn kind_of(message: &sirio_claude::CliMessage) -> &'static str {
    use sirio_claude::CliMessage;
    match message {
        CliMessage::StreamEvent(_) => "stream_event",
        CliMessage::Assistant(_) => "assistant",
        CliMessage::User(_) => "user",
        CliMessage::System(_) => "system",
        CliMessage::Result(_) => "result",
        _ => "other",
    }
}

/// [`answers_initialize_within`] for the stdio control protocol: same
/// reader, same timeout, a different first line.
fn answers_stdio_handshake(program: &str, args: &[&str], handshake: &str) -> Handshake {
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Handshake::NotLaunched;
    };
    let wrote = child
        .stdin
        .as_mut()
        .is_some_and(|stdin| stdin.write_all(format!("{handshake}\n").as_bytes()).is_ok());
    let result = if wrote {
        child.stdout.take().map_or(Handshake::Silent, |stdout| {
            read_handshake(stdout, ACP_HANDSHAKE_TIMEOUT)
        })
    } else {
        Handshake::Silent
    };
    let _ = child.kill();
    let _ = child.wait();
    result
}
// ---------------------------------------------------------------------------------------
// claude_sends_mcp_elicitation_to_its_host
//
// The ways this can fail, each of which the test below catches:
//  - the CLI never forwards `elicitation` to its host (it answers the MCP server itself, or
//    the subtype was renamed): no request arrives before the deadline;
//  - the line changed shape, so `ElicitationRequest::parse` yields no request, a wrong
//    mode or server, or a schema without the four fields the MCP server sent;
//  - the CLI stops handing the host's accepted `content` on to the MCP server, or the
//    action name `accept` no longer means that;
//  - an error answer stops mapping to `cancel` for the server, or the turn no longer
//    continues to a successful `result` after one;
//  - `interrupt` with an elicitation pending stops sending `control_cancel_request`, or
//    names some other id, which would leave the host's form open for a dead request.

/// What one live run saw.
#[derive(Default)]
struct ElicitationRun {
    request: Option<sirio_claude::ElicitationRequest>,
    tool_result: Option<String>,
    cancelled: Vec<String>,
    result: Option<serde_json::Value>,
    seen: Vec<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Answer {
    Accept,
    Error,
    /// No answer: `interrupt` instead, and stop at the CLI's cancel.
    Interrupt,
}

fn tool_result_text(line: &serde_json::Value) -> Option<String> {
    line.get("message")?
        .get("content")?
        .as_array()?
        .iter()
        .find_map(|block| {
            if block.get("type")?.as_str()? != "tool_result" {
                return None;
            }
            let content = block.get("content")?;
            Some(match content.as_array() {
                Some(parts) => parts
                    .iter()
                    .filter_map(|part| part.get("text")?.as_str())
                    .collect::<String>(),
                None => content.as_str().unwrap_or_default().to_string(),
            })
        })
}

/// One CLI process, one `run form` prompt, the answer `answer`; read on until the run's end
/// (the turn's `result`, or the CLI's cancel) or 30 s. The process is killed on every path.
fn run_elicitation(
    claude: &std::path::Path,
    python: &std::path::Path,
    scratch: &std::path::Path,
    answer: Answer,
) -> ElicitationRun {
    use sirio_claude::{
        CanUseTool, CliMessage, ControlRequest, ElicitationRequest, PermissionResult,
    };

    let name = match answer {
        Answer::Accept => "accept",
        Answer::Error => "error",
        Answer::Interrupt => "interrupt",
    };
    let mut session = claude_live::Session::start(claude, python, &scratch.join(name));
    session.write(&ControlRequest::initialize("elicit-init"));
    session.write(&serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": [{"type": "text", "text": "run form"}]},
        "parent_tool_use_id": null,
        "session_id": "",
    }));

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut run = ElicitationRun::default();
    while let Some(line) = session.next(deadline) {
        let kind = line
            .get("type")
            .and_then(|kind| kind.as_str())
            .unwrap_or("?")
            .to_string();
        if kind != "stream_event" {
            run.seen.push(kind.clone());
        }
        if let Some(CliMessage::ControlCancelRequest(id)) = CliMessage::parse(&line.to_string()) {
            run.cancelled.push(id);
            if answer == Answer::Interrupt {
                break;
            }
        }
        if let Some(call) = CanUseTool::parse(&line) {
            session.write(&PermissionResult::allow(call.input).into_response(&call.request_id));
        } else if let Some(request) = ElicitationRequest::parse(&line) {
            match answer {
                Answer::Accept => {
                    let content = serde_json::json!({"colour": "blue", "count": 2, "ok": true});
                    let content = content.as_object().cloned();
                    session.write(&sirio_claude::elicitation_response(
                        &request.request_id,
                        "accept",
                        content,
                    ));
                }
                Answer::Error => {
                    session.write(&sirio_claude::control_error(
                        &request.request_id,
                        "Sirio does not handle elicitation",
                    ));
                }
                Answer::Interrupt => session.write(&ControlRequest::interrupt("elicit-int")),
            }
            run.request = Some(request);
        } else if kind == "user" {
            if let Some(text) = tool_result_text(&line) {
                run.tool_result = Some(text);
            }
        } else if kind == "result" {
            run.result = Some(line);
            break;
        }
    }
    run
}

/// Whether a `result` line is the CLI's success: the turn ran to its end.
fn result_is_success(result: &serde_json::Value) -> bool {
    result.get("subtype").and_then(|s| s.as_str()) == Some("success")
        && result.get("is_error").and_then(|e| e.as_bool()) == Some(false)
}

/// Proves the real `claude` hands an MCP server's `elicitation/create` to its stream-json
/// host as Sirio's wire code reads it, and acts on the three ways a host can end it.
///
/// A fake model API makes the model call an MCP tool whose server elicits a form; the
/// test plays the host in three separate CLI processes: accept with content, answer with a
/// `control_error`, and `interrupt` while the request is open. It uses no credential and no
/// network beyond 127.0.0.1, but launches a real CLI three times, so it is `#[ignore]`d and
/// stays off CI:
///
///     cargo nextest run -p sirio_agents --test acp_conformance --run-ignored only -E 'test(=claude_sends_mcp_elicitation_to_its_host)'
///
/// It SKIPs without a resolvable mise `claude` binary, below the claimed floor, and without
/// `python3` (looked up on `/usr/bin:/bin`, the only PATH the CLI gets). Its scratch directory is
/// under the system temp dir (outside `$HOME` and any git repo), removed after a pass and kept,
/// with its path printed, after a failure.
#[test]
#[ignore = "launches the real claude three times"]
fn claude_sends_mcp_elicitation_to_its_host() {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        eprintln!("SKIP: HOME is not set");
        return;
    };
    let real_home = home.canonicalize().unwrap_or_else(|_| home.clone());
    let Some(claude) = claude_live::resolve_claude(&home) else {
        eprintln!("SKIP: no mise-installed claude binary");
        return;
    };
    claude_live::refuse_wrapper(&claude, &real_home);
    let Some(python) = claude_live::python3() else {
        eprintln!("SKIP: python3 is not available");
        return;
    };
    let scratch = claude_live::Scratch::new("elicitation", &real_home);
    let version_text = claude_live::version_text(&claude, &scratch.root);
    let Some(version) = sirio_claude::ClaudeVersion::parse(&version_text) else {
        panic!("claude reported an unreadable version: {version_text:?}");
    };
    if !version.meets_floor() {
        eprintln!("SKIP: claude {version} is below the claimed floor");
        return;
    }

    // Run 1: accept with content.
    let run = run_elicitation(&claude, &python, &scratch.root, Answer::Accept);
    let request = run
        .request
        .as_ref()
        .unwrap_or_else(|| panic!("no elicitation reached the host; saw {:?}", run.seen));
    assert_eq!(request.mode, "form");
    assert_eq!(request.server, "elic");
    assert_eq!(request.message, "Pick a colour for the probe");
    let properties = request
        .requested_schema
        .as_ref()
        .and_then(|schema| schema.get("properties"))
        .and_then(|p| p.as_object());
    let mut names: Vec<&str> = properties
        .map(|p| p.keys().map(String::as_str).collect())
        .unwrap_or_default();
    names.sort_unstable();
    assert_eq!(
        names,
        ["colour", "count", "note", "ok"],
        "the schema the server sent"
    );
    if let Some(order) = &request.property_order {
        assert_eq!(
            order,
            &["colour", "count", "note", "ok"],
            "the server's field order"
        );
    }
    let answered = run
        .tool_result
        .as_deref()
        .unwrap_or_else(|| panic!("no tool_result; saw {:?}", run.seen));
    assert!(
        answered.contains(r#""action": "accept""#),
        "the server saw the accept: {answered}"
    );
    assert!(
        answered.contains(r#""colour": "blue""#),
        "the server saw the content: {answered}"
    );
    assert!(
        run.result.as_ref().is_some_and(result_is_success),
        "the turn ends in success; saw {:?}",
        run.seen
    );

    // Run 2: a control_error answer is a cancel for the server, and the turn goes on.
    let run = run_elicitation(&claude, &python, &scratch.root, Answer::Error);
    assert!(
        run.request.is_some(),
        "no elicitation reached the host; saw {:?}",
        run.seen
    );
    let answered = run
        .tool_result
        .as_deref()
        .unwrap_or_else(|| panic!("no tool_result; saw {:?}", run.seen));
    assert!(
        answered.contains(r#""action": "cancel""#),
        "the server saw a cancel: {answered}"
    );
    assert!(
        run.result.as_ref().is_some_and(result_is_success),
        "the turn ends in success; saw {:?}",
        run.seen
    );

    // Run 3: unanswered, then interrupted: the CLI withdraws the very request it sent.
    let run = run_elicitation(&claude, &python, &scratch.root, Answer::Interrupt);
    let request = run
        .request
        .as_ref()
        .unwrap_or_else(|| panic!("no elicitation reached the host; saw {:?}", run.seen));
    assert!(
        run.cancelled.contains(&request.request_id),
        "a control_cancel_request names {}; got {:?}, saw {:?}",
        request.request_id,
        run.cancelled,
        run.seen
    );
}

// Failure list for `claude_reports_its_session_state_under_the_variable`, written before the
// test. It fails when:
// - no `session_state_changed` line arrives under `CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1`
//   (the variable is not honoured, or the line is not read), or the first state of a run is
//   not `running` (a spurious leading `idle`);
// - `running` does not come before the turn's `result`, or the run does not end `idle` after it;
// - the last state before the host answers a `can_use_tool` is not `requires_action`, or
//   it does not come after the turn's `running`;
// - after the answer, the order is not `running`, `result`, `idle`;
// - in the background run, any `idle` comes before the last `result`, there are not exactly
//   two `result`s (the main turn's and the CLI's follow-up turn), or the `task_notification`
//   is not between them;
// - the CLI exits early, or a read runs past 40 s (the run ends and the assertion prints
//   what was seen).

/// One thing the host saw or did, in the order it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
enum StateStep {
    State(String),
    PermissionAsked,
    Answered,
    ResultLine,
    TaskNotification,
    /// The CLI's stdout closed before the run ended.
    CliExited,
}

/// One CLI process, one prompt; reads until the first `idle` after a `result`, the CLI's exit,
/// or 40 s. Every `can_use_tool` is allowed, but only once the latest state seen is
/// `requires_action` (or 4 s passed), so the state of a pending request is observed before it
/// is answered.
fn run_state(
    claude: &std::path::Path,
    python: &std::path::Path,
    scratch: &std::path::Path,
    name: &str,
    prompt: &str,
) -> Vec<StateStep> {
    use claude_live::Polled;
    use sirio_claude::{CanUseTool, CliMessage, ControlRequest, PermissionResult};

    let mut session = claude_live::Session::start_state(claude, python, &scratch.join(name));
    session.write(&ControlRequest::initialize("state-init"));
    session.write(&serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": [{"type": "text", "text": prompt}]},
        "parent_tool_use_id": null,
        "session_id": "",
    }));

    let deadline = Instant::now() + Duration::from_secs(40);
    let mut steps = Vec::new();
    let mut pending: Option<(CanUseTool, Instant)> = None;
    let mut latest_state = String::new();
    let mut seen_result = false;
    while Instant::now() < deadline {
        let poll = (Instant::now() + Duration::from_millis(250)).min(deadline);
        match session.poll(poll) {
            Polled::Line(line) => {
                if let Some(call) = CanUseTool::parse(&line) {
                    steps.push(StateStep::PermissionAsked);
                    pending = Some((call, Instant::now()));
                }
                match CliMessage::parse(&line.to_string()) {
                    Some(CliMessage::System(system)) => match system.subtype.as_str() {
                        "session_state_changed" => {
                            latest_state = system.state.unwrap_or_default();
                            steps.push(StateStep::State(latest_state.clone()));
                            if latest_state == "idle" && seen_result {
                                break;
                            }
                        }
                        "task_notification" => steps.push(StateStep::TaskNotification),
                        _ => {}
                    },
                    Some(CliMessage::Result(_)) => {
                        seen_result = true;
                        steps.push(StateStep::ResultLine);
                    }
                    _ => {}
                }
            }
            Polled::Timeout => {}
            Polled::Closed => {
                steps.push(StateStep::CliExited);
                break;
            }
        }
        if pending.as_ref().is_some_and(|(_, since)| {
            latest_state == "requires_action" || since.elapsed() > Duration::from_secs(4)
        }) {
            let (call, _) = pending.take().unwrap();
            steps.push(StateStep::Answered);
            session.write(&PermissionResult::allow(call.input).into_response(&call.request_id));
        }
    }
    steps
}

/// Proves the real `claude` reports its session state under
/// `CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1`, the env `LaunchLine::chat` sets, the way the
/// chat reads it: `running` before a turn's `result` and `idle` after it, `requires_action`
/// while a permission is open, and no `idle` while a background agent still works.
///
/// A fake model API (`fake_state_api.py`) drives three separate CLI processes: a plain
/// turn, a `Bash` call that needs a permission, and a background `Agent`. It uses no
/// credential and no network beyond 127.0.0.1, but launches a real CLI three times, so it is
/// `#[ignore]`d and stays off CI:
///
///     cargo nextest run -p sirio_agents --test acp_conformance --run-ignored only -E 'test(=claude_reports_its_session_state_under_the_variable)'
///
/// It SKIPs without a resolvable mise `claude` binary, below the claimed floor, and without
/// `python3`.
#[test]
#[ignore = "launches the real claude three times"]
fn claude_reports_its_session_state_under_the_variable() {
    use StateStep::{Answered, PermissionAsked, ResultLine, State, TaskNotification};

    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        eprintln!("SKIP: HOME is not set");
        return;
    };
    let real_home = home.canonicalize().unwrap_or_else(|_| home.clone());
    let Some(claude) = claude_live::resolve_claude(&home) else {
        eprintln!("SKIP: no mise-installed claude binary");
        return;
    };
    claude_live::refuse_wrapper(&claude, &real_home);
    let Some(python) = claude_live::python3() else {
        eprintln!("SKIP: python3 is not available");
        return;
    };
    let scratch = claude_live::Scratch::new("session-state", &real_home);
    let version_text = claude_live::version_text(&claude, &scratch.root);
    let Some(version) = sirio_claude::ClaudeVersion::parse(&version_text) else {
        panic!("claude reported an unreadable version: {version_text:?}");
    };
    if !version.meets_floor() {
        eprintln!("SKIP: claude {version} is below the claimed floor");
        return;
    }
    let state = |name: &str| State(name.to_string());
    let position = |steps: &[StateStep], wanted: &StateStep| steps.iter().position(|s| s == wanted);
    let first_state = |steps: &[StateStep]| {
        steps
            .iter()
            .find(|s| matches!(s, State(_)))
            .cloned()
            .unwrap_or_else(|| panic!("no session_state_changed under the variable; saw {steps:?}"))
    };

    // Run 1: a plain turn is running, result, idle.
    let steps = run_state(&claude, &python, &scratch.root, "plain", "say hello");
    assert_eq!(
        first_state(&steps),
        state("running"),
        "first state: {steps:?}"
    );
    let result =
        position(&steps, &ResultLine).unwrap_or_else(|| panic!("no result; saw {steps:?}"));
    let running = position(&steps, &state("running")).unwrap();
    assert!(
        running < result,
        "running comes before the result: {steps:?}"
    );
    assert_eq!(steps.last(), Some(&state("idle")), "ends idle: {steps:?}");

    // Run 2: a permission is requires_action until it is answered; then running, result, idle.
    let steps = run_state(&claude, &python, &scratch.root, "permission", "run bash");
    assert_eq!(
        first_state(&steps),
        state("running"),
        "first state: {steps:?}"
    );
    position(&steps, &PermissionAsked)
        .unwrap_or_else(|| panic!("no can_use_tool reached the host; saw {steps:?}"));
    let answered = position(&steps, &Answered)
        .unwrap_or_else(|| panic!("the can_use_tool was never answered; saw {steps:?}"));
    let before = &steps[..answered];
    let last_before = before.iter().rposition(|s| matches!(s, State(_)));
    assert_eq!(
        last_before.map(|i| &before[i]),
        Some(&state("requires_action")),
        "the state while the request is open is requires_action: {steps:?}"
    );
    assert!(
        position(&steps, &state("running")).unwrap() < last_before.unwrap(),
        "requires_action comes after the turn's running: {steps:?}"
    );
    let after: Vec<&StateStep> = steps[answered + 1..].iter().collect();
    assert_eq!(
        after,
        [&state("running"), &ResultLine, &state("idle")],
        "after the answer: {steps:?}"
    );

    // Run 3: a background agent holds idle back past the main result, through the CLI's
    // follow-up turn.
    let steps = run_state(&claude, &python, &scratch.root, "background", "run bg");
    assert_eq!(
        first_state(&steps),
        state("running"),
        "first state: {steps:?}"
    );
    let results: Vec<usize> = (0..steps.len())
        .filter(|&i| steps[i] == ResultLine)
        .collect();
    assert_eq!(
        results.len(),
        2,
        "the main result and the follow-up's: {steps:?}"
    );
    let notified = position(&steps, &TaskNotification)
        .unwrap_or_else(|| panic!("no task_notification; saw {steps:?}"));
    assert!(
        results[0] < notified && notified < results[1],
        "the task_notification lies between the two results: {steps:?}"
    );
    assert!(
        !steps[..results[1]].contains(&state("idle")),
        "no idle before the last result: {steps:?}"
    );
    assert_eq!(steps.last(), Some(&state("idle")), "ends idle: {steps:?}");
}

// Failure list for `claude_serves_task_output_and_stops_background_tasks`, written before the
// test. It fails when, under Sirio's own `initialize` (no `perTaskStopAffordance`):
// - the `initialize` answer is not `success` (Sirio's worker fails the handshake on an error);
// - the shell's `task_started` lacks `is_backgrounded:true` / `task_type:"local_bash"`, or no
//   `background_tasks_changed` row lists it while it runs, or one still lists it after its stop
//   (the lines Sirio's task registry is built from); the same for the agent, `local_agent`;
// - the CLI refuses `get_task_output` on a running background shell, or its `total_bytes` does
//   not grow between two reads 2 s apart (the output is not live);
// - `stop_task` on the shell is not answered `success`, or the first terminal `task_updated`
//   for it is not `patch.status:"killed"`, or its `task_notification` lacks `status:"stopped"`;
// - a read after the stop does not end with `[killed]\n` within 5 s (v1 #19: the trailer
//   lands late, so the read is retried, not assumed);
// - 2 s after that, a read shows more bytes or no longer ends `[killed]\n` (the task was only
//   marked killed while the process went on writing);
// - the background agent reports an end before `stop_task` is sent, 2 s after its turn ended
//   (it does not stay busy, so the stop below would prove nothing), or its stop does not give
//   its `killed` / `stopped` lines;
// - the CLI exits early or a wait runs past 40 s (the run ends and the assertion prints
//   what was seen).
//
// Background shells outliving the CLI's group kill: #765, not asserted here.

/// A `task_updated` patch status that ends a task (Sirio's registry skips the others).
fn is_terminal(status: &serde_json::Value) -> bool {
    matches!(
        status.as_str(),
        Some("completed" | "killed" | "failed" | "stopped")
    )
}

/// One CLI process driven over stream-json; every line it prints is kept in `seen`.
struct TaskRun {
    session: claude_live::Session,
    seen: Vec<serde_json::Value>,
    next_id: u32,
}

impl TaskRun {
    /// Reads lines until `deadline`, answering every `can_use_tool` with allow.
    /// Returns when the CLI exits or at `deadline`, or at the first new line `stop` accepts.
    fn pump(&mut self, deadline: Instant, stop: &dyn Fn(&serde_json::Value) -> bool) -> bool {
        use claude_live::Polled;
        loop {
            match self.session.poll(deadline) {
                Polled::Line(line) => {
                    if let Some(call) = sirio_claude::CanUseTool::parse(&line) {
                        self.session.write(
                            &sirio_claude::PermissionResult::allow(call.input)
                                .into_response(&call.request_id),
                        );
                    }
                    let hit = stop(&line);
                    self.seen.push(line);
                    if hit {
                        return true;
                    }
                }
                Polled::Timeout => return false,
                Polled::Closed => panic!("the CLI exited early; saw {:#?}", self.brief()),
            }
        }
    }

    /// The first line from `from` on (already seen, else read within 40 s) that `wanted` accepts.
    fn wait_since(
        &mut self,
        from: usize,
        wanted: impl Fn(&serde_json::Value) -> bool,
    ) -> Option<serde_json::Value> {
        if let Some(line) = self.seen[from..].iter().find(|l| wanted(l)) {
            return Some(line.clone());
        }
        let deadline = Instant::now() + Duration::from_secs(40);
        self.pump(deadline, &wanted)
            .then(|| self.seen.last().unwrap().clone())
    }

    fn pump_for(&mut self, span: Duration) {
        self.pump(Instant::now() + span, &|_| false);
    }

    /// Sends a control request and returns its `control_response` body.
    fn ask(&mut self, build: impl Fn(&str) -> serde_json::Value) -> serde_json::Value {
        self.next_id += 1;
        let id = format!("task-{}", self.next_id);
        let from = self.seen.len();
        self.session.write(&build(&id));
        let line = self
            .wait_since(from, |l| {
                l["type"] == "control_response" && l["response"]["request_id"] == id.as_str()
            })
            .unwrap_or_else(|| panic!("no answer to {id}; saw {:#?}", self.brief()));
        line["response"].clone()
    }

    fn output(&mut self, task_id: &str) -> serde_json::Value {
        let response = self.ask(|id| sirio_claude::ControlRequest::get_task_output(id, task_id));
        assert_eq!(
            response["subtype"],
            "success",
            "get_task_output on {task_id}: {response}; saw {:#?}",
            self.brief()
        );
        response["response"].clone()
    }

    /// The `task_started` line for `tool_use_id`, checked to be a backgrounded `task_type`.
    fn task_started_for(&mut self, tool_use_id: &str, task_type: &str) -> String {
        let line = self
            .wait_since(0, |l| {
                l["subtype"] == "task_started" && l["tool_use_id"] == tool_use_id
            })
            .unwrap_or_else(|| {
                panic!("no task_started for {tool_use_id}; saw {:#?}", self.brief())
            });
        assert_eq!(line["is_backgrounded"], true, "{line}");
        assert_eq!(line["task_type"], task_type, "{line}");
        line["task_id"].as_str().unwrap().to_string()
    }

    /// Whether the latest `background_tasks_changed` lists `task_id`.
    fn registry_lists(&self, task_id: &str) -> bool {
        self.seen
            .iter()
            .rev()
            .find(|l| l["subtype"] == "background_tasks_changed")
            .and_then(|l| l["tasks"].as_array())
            .is_some_and(|rows| rows.iter().any(|row| row["task_id"] == task_id))
    }

    /// Whether `from..` holds a terminal `task_updated` or any `task_notification` for `task_id`.
    fn ended_since(&self, from: usize, task_id: &str) -> bool {
        self.seen[from..].iter().any(|l| {
            l["task_id"] == task_id
                && (l["subtype"] == "task_notification"
                    || (l["subtype"] == "task_updated" && is_terminal(&l["patch"]["status"])))
        })
    }

    fn turn_ended(&mut self, from: usize) {
        self.wait_since(from, |l| l["type"] == "result")
            .unwrap_or_else(|| panic!("the turn never ended; saw {:#?}", self.brief()));
    }

    /// `stop_task`, then both of its lines: `task_updated` killed, `task_notification` stopped.
    fn stop(&mut self, task_id: &str) {
        let from = self.seen.len();
        let response = self.ask(|id| sirio_claude::ControlRequest::stop_task(id, task_id));
        assert_eq!(
            response["subtype"],
            "success",
            "stop_task: {response}; saw {:#?}",
            self.brief()
        );
        let updated = self.wait_since(from, |l| {
            l["subtype"] == "task_updated"
                && l["task_id"] == task_id
                && is_terminal(&l["patch"]["status"])
        });
        assert!(
            updated.is_some_and(|l| l["patch"]["status"] == "killed"),
            "the first terminal task_updated for {task_id} is killed; saw {:#?}",
            self.brief()
        );
        let notified = self.wait_since(from, |l| {
            l["subtype"] == "task_notification" && l["task_id"] == task_id
        });
        assert!(
            notified.is_some_and(|l| l["status"] == "stopped"),
            "task_notification stopped for {task_id}; saw {:#?}",
            self.brief()
        );
        let delisted = self.wait_since(from, |l| {
            l["subtype"] == "background_tasks_changed" && !Self::lists_in(l, task_id)
        });
        assert!(
            delisted.is_some(),
            "background_tasks_changed drops {task_id}; saw {:#?}",
            self.brief()
        );
    }

    fn lists_in(line: &serde_json::Value, task_id: &str) -> bool {
        line["tasks"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row["task_id"] == task_id))
    }

    /// Every line kept, one short string each, for assertion messages and the report.
    fn brief(&self) -> Vec<String> {
        self.seen
            .iter()
            .enumerate()
            .filter(|(_, l)| l["type"] != "stream_event")
            .map(|(i, l)| {
                let kind = match l["subtype"].as_str() {
                    Some(sub) => format!("{}/{sub}", l["type"].as_str().unwrap_or("?")),
                    None => l["type"].as_str().unwrap_or("?").to_string(),
                };
                let detail = ["task_id", "status", "state", "request_id"]
                    .iter()
                    .filter_map(|k| l[k].as_str().map(|v| format!("{k}={v}")))
                    .chain(
                        l["response"]["subtype"]
                            .as_str()
                            .map(|v| format!("response={v}")),
                    )
                    .chain(
                        l["patch"]["status"]
                            .as_str()
                            .map(|v| format!("patch.status={v}")),
                    )
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("{i:3} {kind} {detail}")
            })
            .collect()
    }
}

/// Proves the real `claude` serves `get_task_output` and `stop_task` for background tasks
/// under the handshake Sirio sends (`ControlRequest::initialize`, no `perTaskStopAffordance`):
/// a running shell's output grows between reads; stopping it gives `killed` / `stopped`, and a
/// later read ends `[killed]`; and stopping a background agent gives its own `killed` /
/// `stopped` lines.
///
/// A fake model API (`fake_tasks_api.py`) scripts a background `Bash` that prints a line a
/// second, then in a second turn a background `Agent` that stays busy. It uses no credential and
/// no network beyond 127.0.0.1, but launches a real CLI, so it is `#[ignore]`d and stays off CI:
///
///     cargo nextest run -p sirio_agents --test acp_conformance --run-ignored only -E 'test(=claude_serves_task_output_and_stops_background_tasks)'
///
/// It SKIPs without a resolvable mise `claude` binary, below the claimed floor, and without
/// `python3`.
#[test]
#[ignore = "launches the real claude"]
fn claude_serves_task_output_and_stops_background_tasks() {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        eprintln!("SKIP: HOME is not set");
        return;
    };
    let real_home = home.canonicalize().unwrap_or_else(|_| home.clone());
    let Some(claude) = claude_live::resolve_claude(&home) else {
        eprintln!("SKIP: no mise-installed claude binary");
        return;
    };
    claude_live::refuse_wrapper(&claude, &real_home);
    let Some(python) = claude_live::python3() else {
        eprintln!("SKIP: python3 is not available");
        return;
    };
    let scratch = claude_live::Scratch::new("task-output", &real_home);
    let version_text = claude_live::version_text(&claude, &scratch.root);
    let Some(version) = sirio_claude::ClaudeVersion::parse(&version_text) else {
        panic!("claude reported an unreadable version: {version_text:?}");
    };
    if !version.meets_floor() {
        eprintln!("SKIP: claude {version} is below the claimed floor");
        return;
    }
    eprintln!("claude binary {} reports {version_text}", claude.display());

    let session = claude_live::Session::start_tasks(&claude, &python, &scratch.root.join("tasks"));
    let mut run = TaskRun {
        session,
        seen: Vec::new(),
        next_id: 0,
    };
    // Sirio's own handshake: the builder the native client uses, nothing added.
    let init = run.ask(sirio_claude::ControlRequest::initialize);
    assert_eq!(init["subtype"], "success", "initialize: {init}");
    run.session.write(&serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": [{"type": "text", "text": "run shells"}]},
        "parent_tool_use_id": null,
        "session_id": "",
    }));
    let ticker = run.task_started_for("toolu_s1", "local_bash");
    run.turn_ended(0);
    assert!(
        run.registry_lists(&ticker),
        "the registry lists the running shell"
    );

    // Two reads of the running shell, 2 s apart: the output is live.
    let first = run.output(&ticker);
    run.pump_for(Duration::from_secs(2));
    let second = run.output(&ticker);
    let (before, after) = (
        first["total_bytes"].as_u64().unwrap(),
        second["total_bytes"].as_u64().unwrap(),
    );
    assert!(
        after > before && second["output"].as_str().unwrap().contains("tick"),
        "total_bytes grows between reads: {first} then {second}"
    );

    // Stop the ticker; a read after it ends [killed], retried for 5 s.
    run.stop(&ticker);
    assert!(
        !run.registry_lists(&ticker),
        "the registry dropped the stopped shell"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut killed = serde_json::Value::Null;
    while Instant::now() < deadline {
        killed = run.output(&ticker);
        if killed["output"]
            .as_str()
            .unwrap_or("")
            .ends_with("[killed]\n")
        {
            break;
        }
        run.pump_for(Duration::from_millis(250));
    }
    assert!(
        killed["output"]
            .as_str()
            .unwrap_or("")
            .ends_with("[killed]\n"),
        "a read after the stop ends with [killed]: {killed}"
    );
    // The process is stopped, not only marked: 2 s on, nothing more was written.
    run.pump_for(Duration::from_secs(2));
    let later = run.output(&ticker);
    assert_eq!(
        later["total_bytes"], killed["total_bytes"],
        "the stopped shell wrote nothing more: {killed} then {later}"
    );
    assert!(
        later["output"]
            .as_str()
            .unwrap_or("")
            .ends_with("[killed]\n"),
        "still ends with [killed]: {later}"
    );

    // Turn two: a background agent that stays busy; stop it.
    let from = run.seen.len();
    run.session.write(&serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": [{"type": "text", "text": "run agent"}]},
        "parent_tool_use_id": null,
        "session_id": "",
    }));
    let agent = run.task_started_for("toolu_a1", "local_agent");
    run.turn_ended(from);
    // It stays busy: 2 s after the turn ended nothing has ended it, so the stop below is
    // what ends it, not the turn's end.
    run.pump_for(Duration::from_secs(2));
    assert!(
        !run.ended_since(from, &agent),
        "the agent ended before stop_task; saw {:#?}",
        run.brief()
    );
    assert!(
        run.registry_lists(&agent),
        "the registry lists the running agent"
    );
    run.stop(&agent);
    assert!(
        !run.registry_lists(&agent),
        "the registry dropped the stopped agent"
    );
    run.pump_for(Duration::from_secs(1));

    eprintln!("line order:\n{}", run.brief().join("\n"));
}

// Failure list for `claude_forwards_a_subagents_text_under_sirios_initialize`, written before
// the test. It fails when, under Sirio's own `initialize` (which carries `forwardSubagentText`),
// for a foreground `Agent` call (`run_in_background: false`) and one that omits the field:
// - the `initialize` answer is not `success`;
// - the main agent's `Agent` tool_use is not seen, so there is no parent id to match;
// - no `assistant` line has `parent_tool_use_id` equal to that call's id and a text block
//   "Sub here. Done." within 60 s (the omitted case may forward after the turn's `result`);
// - the forwarded line carries no `message.id`, or the CLI exits before any of these.

/// Sends Sirio's own `initialize` and one prompt, then waits up to 60 s for the subagent's
/// forwarded text. Returns whether the `Agent` call was seen and the text arrived.
fn subagent_text_reaches_stdout(
    claude: &std::path::Path,
    python: &std::path::Path,
    root: &std::path::Path,
    prompt: &str,
) -> (bool, bool) {
    use claude_live::Polled;
    let mut session = claude_live::Session::start_subagent(claude, python, root);
    session.write(&sirio_claude::ControlRequest::initialize("sub-init"));
    session.write(&serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": [{"type": "text", "text": prompt}]},
        "parent_tool_use_id": null,
        "session_id": "",
    }));
    let deadline = Instant::now() + Duration::from_secs(60);
    let (mut initialized, mut agent_id, mut forwarded) = (false, None::<String>, false);
    while !forwarded {
        match session.poll(deadline) {
            Polled::Line(line) => {
                if let Some(call) = sirio_claude::CanUseTool::parse(&line) {
                    session.write(
                        &sirio_claude::PermissionResult::allow(call.input)
                            .into_response(&call.request_id),
                    );
                }
                if line["type"] == "control_response"
                    && line["response"]["request_id"] == "sub-init"
                {
                    initialized = line["response"]["subtype"] == "success";
                }
                if line["type"] != "assistant" {
                    continue;
                }
                let blocks = line["message"]["content"].as_array().cloned().unwrap_or_default();
                match line["parent_tool_use_id"].as_str() {
                    None => {
                        agent_id = blocks
                            .iter()
                            .find(|b| b["type"] == "tool_use" && b["name"] == "Agent")
                            .and_then(|b| b["id"].as_str().map(str::to_string))
                            .or(agent_id);
                    }
                    Some(parent) => {
                        forwarded = agent_id.as_deref() == Some(parent)
                            && line["message"]["id"].is_string()
                            && blocks
                                .iter()
                                .any(|b| b["type"] == "text" && b["text"] == "Sub here. Done.");
                    }
                }
            }
            Polled::Timeout | Polled::Closed => break,
        }
    }
    assert!(initialized, "the initialize answer is success: false");
    (agent_id.is_some(), forwarded)
}

/// Proves the real `claude` forwards a subagent's text on stdout under the handshake Sirio sends
/// (`ControlRequest::initialize`, which carries `forwardSubagentText: true`): an `assistant`
/// line whose `parent_tool_use_id` is the `Agent` call's id and whose content is the subagent's
/// text block. Run for a foreground call (`run_in_background: false`) and one that omits the
/// field, which some versions launch as a background task (its text may follow the `result`).
///
/// A fake model API (`fake_subagent_api.py`) scripts the `Agent` call and the subagent's reply.
/// It uses no credential and no network beyond 127.0.0.1, but launches a real CLI, so it is
/// `#[ignore]`d and stays off CI:
///
///     cargo nextest run -p sirio_agents --test acp_conformance --run-ignored only \
///         -E 'test(=claude_forwards_a_subagents_text_under_sirios_initialize)'
///
/// It SKIPs without a resolvable mise `claude` binary, below the claimed floor, and without
/// `python3`.
#[test]
#[ignore = "launches the real claude"]
fn claude_forwards_a_subagents_text_under_sirios_initialize() {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        eprintln!("SKIP: HOME is not set");
        return;
    };
    let real_home = home.canonicalize().unwrap_or_else(|_| home.clone());
    let Some(claude) = claude_live::resolve_claude(&home) else {
        eprintln!("SKIP: no mise-installed claude binary");
        return;
    };
    claude_live::refuse_wrapper(&claude, &real_home);
    let Some(python) = claude_live::python3() else {
        eprintln!("SKIP: python3 is not available");
        return;
    };
    let scratch = claude_live::Scratch::new("subagent-text", &real_home);
    let version_text = claude_live::version_text(&claude, &scratch.root);
    let Some(version) = sirio_claude::ClaudeVersion::parse(&version_text) else {
        panic!("claude reported an unreadable version");
    };
    if !version.meets_floor() {
        eprintln!("SKIP: claude {version} is below the claimed floor");
        return;
    }
    eprintln!("claude reports {version_text}");

    for (name, prompt) in [("false", "run sub false"), ("omitted", "run sub omitted")] {
        let (called, forwarded) =
            subagent_text_reaches_stdout(&claude, &python, &scratch.root.join(name), prompt);
        assert!(called, "the main agent called Agent ({name}): false");
        assert!(
            forwarded,
            "the subagent's text reached stdout within 60 s ({name}): false"
        );
        eprintln!("prompt {name}: the subagent's text was forwarded");
    }
}

// Failure list for `claude_takes_a_host_rename`, written before the test. It fails when, under
// Sirio's own `initialize` and `rename_session("Café ☕ 名前", Some(<own uuid>))`:
// - the rename is refused in stream-json (the answer is not `success`);
// - `claude agents --json`, run while the session is live, does not list that `sessionId` under
//   `name == "Café ☕ 名前"` (the picker would not show the title);
// - the non-ASCII title comes back mangled anywhere: every line is parsed as JSON and compared
//   as a string, never matched on raw text, because a writer may escape non-ASCII;
// - the transcript `projects/<slug>/<uuid>.jsonl` has no `custom-title` line with that title;
// - `claude -p --resume "Café ☕ 名前"` does not resolve to the same session (its
//   `system/init.session_id` is not the uuid), run after the stream-json CLI has exited;
// - a rename that names a foreign `session_id` is accepted (current CLIs answer the error
//   `session_id is not the current session`);
// - the CLI exits early or any wait runs past its limit (the run ends and prints what was seen).
//
// The listing and the foreign-id error run at 2.1.326 and above only. The 2.1.257 floor
// (FINDINGS Q2, Q3) answers success to a foreign `session_id` and writes to its own current
// session, ignores `source` and `session_id`, never updates `sessions/<pid>.json` (which
// `agents` reads), and writes the title to the transcript only once a turn creates it.

/// Every JSON line of `text` that parses, in order.
fn json_lines(text: &str) -> Vec<serde_json::Value> {
    text.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// The `<uuid>.jsonl` under `<config_dir>/projects/<slug>/`, found without the slug rule.
fn find_transcript(config_dir: &std::path::Path, uuid: &str) -> Option<std::path::PathBuf> {
    let file = format!("{uuid}.jsonl");
    std::fs::read_dir(config_dir.join("projects"))
        .ok()?
        .flatten()
        .map(|slug| slug.path().join(&file))
        .find(|path| path.is_file())
}

/// Sends `request` and returns the `control_response` body for its `request_id` within 30 s,
/// ignoring every other line (the echo `session_title_changed` comes first).
fn ask_session(
    session: &mut claude_live::Session,
    seen: &mut Vec<serde_json::Value>,
    request: serde_json::Value,
) -> serde_json::Value {
    let id = request["request_id"]
        .as_str()
        .expect("a control request has a request_id")
        .to_string();
    session.write(&request);
    let deadline = Instant::now() + Duration::from_secs(30);
    while let Some(line) = session.next(deadline) {
        seen.push(line.clone());
        if line["type"] == "control_response" && line["response"]["request_id"] == id.as_str() {
            return line["response"].clone();
        }
    }
    let brief: Vec<String> = seen
        .iter()
        .map(|line| {
            format!(
                "{}/{}/{}",
                line["type"].as_str().unwrap_or("?"),
                line["subtype"]
                    .as_str()
                    .or(line["response"]["subtype"].as_str())
                    .or(line["request"]["subtype"].as_str())
                    .unwrap_or("-"),
                line["response"]["request_id"]
                    .as_str()
                    .or(line["request_id"].as_str())
                    .unwrap_or("-"),
            )
        })
        .collect();
    panic!("no answer to {id}; saw {brief:#?}");
}

/// Proves the real Claude Code accepts Sirio's `rename_session` (`ControlRequest::rename_session`,
/// `source:"host"` and the session's own id): the session is listed by `agents --json` under the
/// title, the transcript records it, a foreign session id is refused, and after the CLI exits
/// `claude -p --resume "<title>"` resumes the same session. Non-ASCII on purpose.
///
/// `initialize` and `rename_session` come from Sirio's builders. The user line of the turn is
/// hand-written JSON: the real builder (`sirio_chat::claude::prompt::user_message`) is
/// `pub(super)` and `sirio_agents` does not depend on `sirio_chat`.
///
/// It uses the fake API of the session-state test, no credential and nothing beyond 127.0.0.1,
/// but launches a real CLI, so it is `#[ignore]`d and stays off CI. TMPDIR must be on disk
/// outside the home directory:
///
///     TMPDIR=/var/tmp/sirio-live cargo test -p sirio_agents --test acp_conformance claude_takes_a_host_rename -- --ignored --nocapture
///
/// It SKIPs without a resolvable mise `claude` binary, below the claimed floor, and without
/// `python3`.
#[test]
#[ignore = "launches the real claude"]
fn claude_takes_a_host_rename() {
    const TITLE: &str = "Café ☕ 名前";
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        eprintln!("SKIP: HOME is not set");
        return;
    };
    let real_home = home.canonicalize().unwrap_or_else(|_| home.clone());
    let Some(claude) = claude_live::resolve_claude(&home) else {
        eprintln!("SKIP: no mise-installed claude binary");
        return;
    };
    claude_live::refuse_wrapper(&claude, &real_home);
    let Some(python) = claude_live::python3() else {
        eprintln!("SKIP: python3 is not available");
        return;
    };
    let scratch = claude_live::Scratch::new("host-rename", &real_home);
    let version_text = claude_live::version_text(&claude, &scratch.root);
    let Some(version) = sirio_claude::ClaudeVersion::parse(&version_text) else {
        panic!("claude reported an unreadable version: {version_text:?}");
    };
    if !version.meets_floor() {
        eprintln!("SKIP: claude {version} is below the claimed floor");
        return;
    }
    // Only a current CLI lists the title and guards the session id (see the failure list).
    let current = version >= sirio_claude::ClaudeVersion::new(2, 1, 326);
    eprintln!("claude binary {} reports {version_text}", claude.display());

    let mut session =
        claude_live::Session::start_state(&claude, &python, &scratch.root.join("rename"));
    let uuid = session.session_id().to_string();
    let mut seen = Vec::new();

    let init = ask_session(
        &mut session,
        &mut seen,
        sirio_claude::ControlRequest::initialize("rn-init"),
    );
    assert_eq!(init["subtype"], "success", "initialize: {init}");

    // 2. The rename, with the session's own id.
    let renamed = ask_session(
        &mut session,
        &mut seen,
        sirio_claude::ControlRequest::rename_session("rn-1", TITLE, Some(&uuid)),
    );
    assert_eq!(renamed["subtype"], "success", "rename_session: {renamed}");

    // 3. While the process is alive, the listing shows the title under the session id.
    if current {
        let listing = session.run_claude("agents", &["agents", "--json"], Duration::from_secs(20));
        assert!(listing.success, "agents --json failed: {}", listing.stderr);
        let entries: serde_json::Value = serde_json::from_str(listing.stdout.trim())
            .unwrap_or_else(|e| panic!("agents --json is not JSON ({e}): {}", listing.stdout));
        let entry = entries
            .as_array()
            .and_then(|all| all.iter().find(|entry| entry["sessionId"] == uuid.as_str()))
            .unwrap_or_else(|| panic!("agents --json lists no {uuid}: {entries}"));
        assert_eq!(entry["name"], TITLE, "agents entry: {entry}");
    }

    // 4. One turn creates the transcript.
    session.write(&serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": [{"type": "text", "text": "hello"}]},
        "parent_tool_use_id": null,
        "session_id": "",
    }));
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut result = None;
    while let Some(line) = session.next(deadline) {
        seen.push(line.clone());
        if line["type"] == "result" {
            result = Some(line);
            break;
        }
    }
    let result = result.unwrap_or_else(|| panic!("the turn never ended; saw {seen:#?}"));
    assert_eq!(result["subtype"], "success", "the turn failed: {result}");

    // 5. The transcript holds the title in a custom-title line.
    let transcript = find_transcript(&session.config_dir(), &uuid)
        .unwrap_or_else(|| panic!("no {uuid}.jsonl under {}", session.config_dir().display()));
    let text = std::fs::read_to_string(&transcript).unwrap();
    assert!(
        json_lines(&text)
            .iter()
            .any(|line| line["type"] == "custom-title"
                && line["customTitle"] == TITLE
                && line["sessionId"] == uuid.as_str()),
        "no custom-title line {TITLE:?} in {}:\n{text}",
        transcript.display()
    );

    // 6. A foreign session id is refused.
    if current {
        // A fixed uuid that is not the session's own (FINDINGS Q2).
        let foreign = "d5f4c32d-3654-48cf-91e6-5665e2d3ce60";
        let refused = ask_session(
            &mut session,
            &mut seen,
            sirio_claude::ControlRequest::rename_session("rn-2", "x", Some(foreign)),
        );
        assert_eq!(refused["subtype"], "error", "foreign session_id: {refused}");
        assert_eq!(
            refused["error"], "session_id is not the current session",
            "foreign session_id: {refused}"
        );
    }

    // 7. After the CLI exited (stdin closed, process reaped; the fake API still up), the title
    // resolves the same session.
    let exit = session.close_cli();
    assert!(
        exit.is_some_and(|status| status.success()),
        "the CLI did not exit successfully on stdin EOF: {exit:?}"
    );
    let resumed = session.run_claude(
        "resume",
        &[
            "-p",
            "--resume",
            TITLE,
            "hi",
            "--output-format",
            "stream-json",
            "--verbose",
        ],
        Duration::from_secs(30),
    );
    assert!(
        resumed.success,
        "-p --resume failed: {}\n{}",
        resumed.stderr, resumed.stdout
    );
    let lines = json_lines(&resumed.stdout);
    let init = lines
        .iter()
        .find(|line| line["type"] == "system" && line["subtype"] == "init")
        .unwrap_or_else(|| panic!("no system/init in the resumed run: {}", resumed.stdout));
    assert_eq!(
        init["session_id"],
        uuid.as_str(),
        "--resume {TITLE:?} resumed another session: {init}"
    );
}

// Failure list for `claude_lists_permission_rules`, written before the test. It fails when, under
// Sirio's own `initialize` and `list_permission_rules`, sent before any turn:
// - the verb is refused (an error answer) on 2.1.325 or later;
// - the success answer does not parse with `parse_permission_rules`;
// - a rule of the user settings is missing from the answer: the allow, the deny or the ask rule,
//   each with source `userSettings` and editability `persistent`;
// - the project allow rule is missing, source `projectSettings`, editability `persistent` (the
//   project file was not read; `work/` is its own git repository, as a cwd inside another one
//   reads that repository's settings instead, FINDINGS Surprise 1);
// - the CLI does not exit on stdin EOF, or a process of the run outlives it.
//
// Below 2.1.325 only 2.1.257 was probed (FINDINGS Q1): it refuses with `Unsupported control
// request subtype: list_permission_rules`. There either a parsing success or a refusal passes,
// and the test says which one it saw.

/// Proves the real Claude Code answers Sirio's `list_permission_rules` (`ControlRequest::
/// list_permission_rules`, read by `parse_permission_rules`) on a configured config dir: the
/// user rules (allow, deny, ask) and the project rule come back, labelled by source.
///
/// It uses the fake API of the session-state test, no credential and nothing beyond 127.0.0.1,
/// but launches a real CLI, so it is `#[ignore]`d and stays off CI. TMPDIR must be on disk
/// outside the home directory:
///
///     TMPDIR=/var/tmp/sirio-live cargo test -p sirio_agents --test acp_conformance claude_lists_permission_rules -- --ignored --nocapture
///
/// It SKIPs without a resolvable mise `claude` binary, below the claimed floor, without
/// `python3` and without `git`.
#[test]
#[ignore = "launches the real claude"]
fn claude_lists_permission_rules() {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        eprintln!("SKIP: HOME is not set");
        return;
    };
    let real_home = home.canonicalize().unwrap_or_else(|_| home.clone());
    let Some(claude) = claude_live::resolve_claude(&home) else {
        eprintln!("SKIP: no mise-installed claude binary");
        return;
    };
    claude_live::refuse_wrapper(&claude, &real_home);
    let Some(python) = claude_live::python3() else {
        eprintln!("SKIP: python3 is not available");
        return;
    };
    if !claude_live::git_available() {
        eprintln!("SKIP: git is not available");
        return;
    }
    let scratch = claude_live::Scratch::new("permission-rules", &real_home);
    let version_text = claude_live::version_text(&claude, &scratch.root);
    let Some(version) = sirio_claude::ClaudeVersion::parse(&version_text) else {
        panic!("claude reported an unreadable version: {version_text:?}");
    };
    if !version.meets_floor() {
        eprintln!("SKIP: claude {version} is below the claimed floor");
        return;
    }
    let answers = version >= sirio_claude::ClaudeVersion::new(2, 1, 325);
    eprintln!("claude reports {version_text}");

    let user = serde_json::json!({"permissions": {
        "allow": ["Bash(echo hi:*)"],
        "deny": ["Bash(rm -rf /:*)"],
        "ask": ["Bash(git push:*)"],
    }});
    let project = serde_json::json!({"permissions": {"allow": ["Bash(ls:*)"]}});
    let mut session = claude_live::Session::start_with_settings(
        &claude,
        &python,
        &scratch.root.join("rules"),
        &user,
        Some(&project),
    );
    let mut seen = Vec::new();

    let init = ask_session(
        &mut session,
        &mut seen,
        sirio_claude::ControlRequest::initialize("pr-init"),
    );
    assert_eq!(
        init["subtype"], "success",
        "initialize answered success: false"
    );

    let answer = ask_session(
        &mut session,
        &mut seen,
        sirio_claude::ControlRequest::list_permission_rules("pr-1"),
    );
    // Kept with the scratch root after a panic, to see which rule was missing.
    std::fs::write(scratch.root.join("list-answer.json"), answer.to_string()).unwrap();
    let succeeded = answer["subtype"] == "success";
    let parsed = if succeeded {
        sirio_claude::parse_permission_rules(&answer["response"])
    } else {
        None
    };
    if answers {
        assert!(succeeded, "the list answered success: {succeeded}");
        let rules = parsed
            .as_ref()
            .expect("the success answer parses with parse_permission_rules");
        let listed = |behavior: &str, source: &str, rule: &str| {
            rules.rules.iter().any(|row| {
                row.behavior == behavior
                    && row.source == source
                    && row.rule == rule
                    && row.editability == "persistent"
            })
        };
        let user_allow = listed("allow", "userSettings", "Bash(echo hi:*)");
        let user_deny = listed("deny", "userSettings", "Bash(rm -rf /:*)");
        let user_ask = listed("ask", "userSettings", "Bash(git push:*)");
        let project_allow = listed("allow", "projectSettings", "Bash(ls:*)");
        assert!(user_allow, "the user allow rule is listed: {user_allow}");
        assert!(user_deny, "the user deny rule is listed: {user_deny}");
        assert!(user_ask, "the user ask rule is listed: {user_ask}");
        assert!(
            project_allow,
            "the project allow rule is listed: {project_allow}"
        );
        eprintln!("the CLI answered the list with every configured rule");
    } else {
        let refused = !succeeded
            && answer["error"]
                .as_str()
                .is_some_and(sirio_claude::is_refusal);
        assert!(
            parsed.is_some() || refused,
            "the answer is a parsing success or a refusal: false (succeeded: {succeeded})"
        );
        eprintln!(
            "below 2.1.325 the CLI answered: {}",
            if refused {
                "a refusal"
            } else {
                "a success that parses"
            }
        );
    }

    let exit = session.close_cli();
    assert!(
        exit.is_some_and(|status| status.success()),
        "the CLI exited successfully on stdin EOF: false (exit code {:?}, None when killed after 20 s)",
        exit.map(|status| status.code())
    );
    // Nothing of the run may outlive the CLI: poll for up to 2 s, then count.
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut left = claude_live::holders_of(&session.home());
    while left > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        left = claude_live::holders_of(&session.home());
    }
    assert!(
        left == 0,
        "no process of the run outlives the CLI: false ({left} left)"
    );
}

// Failure list for `claude_lists_hooks`, written before the test. On a version at or above
// 2.1.325 it fails when (items 0-5; items 6 and 7 are asserted on every version that
// meets the floor):
// 0. `get_hooks_listing` is refused or answers an error;
// 1. the success does not parse with `parse_hooks_listing`;
// 2. the user `PreToolUse` row is missing, or not from `userSettings`, or its matcher is not
//    `Bash`, or its status message is not the display text, or its command is not the marker's;
// 3. the project `Stop` row is missing or not from `projectSettings` (the file was not read);
// 4. an event is missing from `events` or has an empty summary;
// 5. the answer has no `policy` object, a policy flag is set, safe or bare mode is on, a
//    configured row is disabled, or a settings error is listed;
// 6. either marker exists at the end (the CLI ran a hook, or the listing took a turn);
// 7. a process of the run outlives the CLI, or the CLI does not exit on stdin EOF.
// On a host with managed settings (`/etc/claude-code`) the test SKIPs: they would change the
// policy and the rows. Below 2.1.325 it accepts a parsing success or a refusal.
/// Asks the real CLI for its hook listing before any turn, with a user `PreToolUse` hook and a
/// project `Stop` hook configured, and checks the rows, the events, the policy, and that no hook
/// command ran.
///
/// It uses the fake API of the session-state test, no credential and nothing beyond 127.0.0.1,
/// but launches a real CLI, so it is `#[ignore]`d and stays off CI. TMPDIR must be on disk
/// outside the home directory:
///
///     TMPDIR=/var/tmp/sirio-live cargo test -p sirio_agents --test acp_conformance \
///         claude_lists_hooks -- --ignored --nocapture --test-threads=1
///
/// It SKIPs without a resolvable mise `claude` binary, below the claimed floor, without
/// `python3`, without `git` and on a host with managed settings.
#[test]
#[ignore = "launches the real claude"]
fn claude_lists_hooks() {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        eprintln!("SKIP: HOME is not set");
        return;
    };
    let real_home = home.canonicalize().unwrap_or_else(|_| home.clone());
    let Some(claude) = claude_live::resolve_claude(&home) else {
        eprintln!("SKIP: no mise-installed claude binary");
        return;
    };
    claude_live::refuse_wrapper(&claude, &real_home);
    let Some(python) = claude_live::python3() else {
        eprintln!("SKIP: python3 is not available");
        return;
    };
    if !claude_live::git_available() {
        eprintln!("SKIP: git is not available");
        return;
    }
    if std::path::Path::new("/etc/claude-code").exists() {
        eprintln!("SKIP: managed settings exist on this host");
        return;
    }
    let scratch = claude_live::Scratch::new("hooks", &real_home);
    let version_text = claude_live::version_text(&claude, &scratch.root);
    let Some(version) = sirio_claude::ClaudeVersion::parse(&version_text) else {
        panic!("claude reported an unreadable version: {version_text:?}");
    };
    if !version.meets_floor() {
        eprintln!("SKIP: claude {version} is below the claimed floor");
        return;
    }
    let answers = version >= sirio_claude::ClaudeVersion::new(2, 1, 325);
    eprintln!("claude reports {version_text}");

    let user_marker = scratch.root.join("user-hook.ran");
    let project_marker = scratch.root.join("project-hook.ran");
    let user = serde_json::json!({"hooks": {"PreToolUse": [{
        "matcher": "Bash",
        "hooks": [{
            "type": "command",
            "command": format!("touch {}", user_marker.display()),
            "statusMessage": "Checking bash",
        }],
    }]}});
    let project = serde_json::json!({"hooks": {"Stop": [{
        "hooks": [{
            "type": "command",
            "command": format!("touch {}", project_marker.display()),
        }],
    }]}});
    let mut session = claude_live::Session::start_with_settings(
        &claude,
        &python,
        &scratch.root.join("hooks"),
        &user,
        Some(&project),
    );
    let mut seen = Vec::new();

    let init = ask_session(
        &mut session,
        &mut seen,
        sirio_claude::ControlRequest::initialize("hk-init"),
    );
    assert_eq!(
        init["subtype"], "success",
        "initialize answered success: false"
    );

    let answer = ask_session(
        &mut session,
        &mut seen,
        sirio_claude::ControlRequest::get_hooks_listing("hk-1"),
    );
    // Kept with the scratch root after a panic, to see which row was missing.
    std::fs::write(scratch.root.join("hooks-answer.json"), answer.to_string()).unwrap();
    let succeeded = answer["subtype"] == "success";
    let parsed = if succeeded {
        sirio_claude::parse_hooks_listing(&answer["response"])
    } else {
        None
    };
    if answers {
        assert!(succeeded, "the listing answered success: {succeeded}");
        let listing = parsed
            .as_ref()
            .expect("the success answer parses with parse_hooks_listing");
        let no_errors = listing.errors.is_empty();
        assert!(no_errors, "no settings file was skipped: {no_errors}");
        let user_row = listing.hooks.iter().find(|hook| {
            hook.event == "PreToolUse" && hook.source == "userSettings" && hook.matcher == "Bash"
        });
        let user_listed = user_row.is_some();
        assert!(user_listed, "the user hook is listed: {user_listed}");
        let user_row = user_row.unwrap();
        let status = user_row.display_text == "Checking bash";
        assert!(status, "the user hook is listed with its status message: {status}");
        let names_marker = user_row
            .command_text
            .as_deref()
            .is_some_and(|text| text.contains("user-hook.ran"));
        assert!(names_marker, "the user hook command names its marker: {names_marker}");
        let project_listed = listing
            .hooks
            .iter()
            .any(|hook| hook.event == "Stop" && hook.source == "projectSettings");
        assert!(
            project_listed,
            "the project hook is listed from the project file: {project_listed}"
        );
        for event in ["PreToolUse", "Stop"] {
            let summarized = listing
                .events
                .iter()
                .any(|row| row.name == event && !row.summary.is_empty());
            assert!(
                summarized,
                "an event of the configured hooks is listed with a summary: {summarized}"
            );
        }
        let has_policy = answer["response"]["policy"].is_object();
        assert!(has_policy, "the answer carries a policy: {has_policy}");
        let policy = &listing.policy;
        let clear = !(policy.disabled_by_policy
            || policy.managed_only
            || policy.plugin_only
            || policy.all_disabled
            || policy.unreadable);
        assert!(clear, "every policy flag is false: {clear}");
        let no_mode = listing.safe_mode.is_none() && listing.bare_mode.is_none();
        assert!(no_mode, "neither safe mode nor bare mode is on: {no_mode}");
        // Guard: every disabled row in the probes came with a mode or a policy flag, which
        // fail first.
        let enabled = listing
            .hooks
            .iter()
            .filter(|hook| {
                (hook.event == "PreToolUse" && hook.source == "userSettings")
                    || (hook.event == "Stop" && hook.source == "projectSettings")
            })
            .all(|hook| !hook.disabled);
        assert!(enabled, "the configured hooks are not disabled: {enabled}");
        eprintln!("the CLI listed the configured hooks");
    } else {
        let refused = !succeeded
            && answer["error"]
                .as_str()
                .is_some_and(sirio_claude::is_refusal);
        assert!(
            parsed.is_some() || refused,
            "the answer is a parsing success or a refusal: false (succeeded: {succeeded})"
        );
        eprintln!(
            "below 2.1.325 the CLI answered: {}",
            if refused {
                "a refusal"
            } else {
                "a success that parses"
            }
        );
    }

    let exit = session.close_cli();
    assert!(
        exit.is_some_and(|status| status.success()),
        "the CLI exited successfully on stdin EOF: false \
         (exit code {:?}, None when killed after 20 s)",
        exit.map(|status| status.code())
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut left = claude_live::holders_of(&session.home());
    while left > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        left = claude_live::holders_of(&session.home());
    }
    assert!(
        left == 0,
        "no process of the run outlives the CLI: false ({left} left)"
    );
    let user_ran = user_marker.exists();
    let project_ran = project_marker.exists();
    assert!(!user_ran, "no user hook ran: {}", !user_ran);
    assert!(!project_ran, "no project hook ran: {}", !project_ran);
}

// Failure list for `claude_toggles_a_connector_and_reloads`, written before the test. On a version
// that meets the floor it fails when:
// 0. before the first toggle, `probe` does not read `Pending` or `Connected` (`Failed` fails);
// 1. `mcp_toggle` off for the user-scope server `probe` answers an error;
// 2. `mcp_status` afterwards does not read `Disabled` for it;
// 3. after the first session exits (item 7), the config dir's `.claude.json` is missing or does
//    not parse, or has no `projects[<work>].disabledMcpServers` list, or the list lacks `probe`;
// 4. a `reload_plugins` with the hold, sent before any turn, does not parse with
//    `parse_plugins_reload`, or comes back `held: true`, or (2.1.325 or later) does not carry an
//    explicit `held: false` (a missing key reads as false, so it would hide a hold never received);
// 5. its `commands`, given to `Catalog::replace_commands`, do not yield a non-empty list holding
//    every command the `initialize` answer listed, or that list was empty to begin with;
// 6. on 2.1.325 or later, the `initialize` capabilities lack `reload_plugins_refuse_if_busy_v1`;
// 7. the first session does not exit on stdin EOF, or a process of it outlives it;
// 8. a fresh session on the same folder does not read `probe` as `Disabled` before any toggle;
// 9. `mcp_toggle` on does not answer success;
// 10. `mcp_status` does not reach `Pending` or `Connected` within 15 s (`Failed` or
//     `Disabled` for the whole time fails);
// 11. after the second session exits (item 13), the same list is missing or still has `probe`
//     (the CLI writes `[]` on toggle on);
// 12. a toggle of an unknown server is not an error, or its error is a refusal;
// 13. the second session does not exit on EOF, or leaves a process behind.

/// Proves the real Claude Code honours Sirio's `mcp_toggle` (`ControlRequest::mcp_toggle`) and
/// `reload_plugins` with the hold (`ControlRequest::reload_plugins`): the switch is saved per
/// folder in the config dir and a fresh session still sees it, and the reload answers in a shape
/// `parse_plugins_reload` reads.
///
/// It uses the fake API of the session-state test, no credential and nothing beyond 127.0.0.1,
/// but launches a real CLI, so it is `#[ignore]`d and stays off CI. TMPDIR must be on disk
/// outside the home directory:
///
///     TMPDIR=/var/tmp/sirio-live cargo test -p sirio_agents --test acp_conformance claude_toggles_a_connector_and_reloads -- --ignored --nocapture
///
/// It SKIPs without a resolvable mise `claude` binary, below the claimed floor and without
/// `python3`.
#[test]
#[ignore = "launches the real claude"]
fn claude_toggles_a_connector_and_reloads() {
    use sirio_claude::McpState;

    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        eprintln!("SKIP: HOME is not set");
        return;
    };
    let real_home = home.canonicalize().unwrap_or_else(|_| home.clone());
    let Some(claude) = claude_live::resolve_claude(&home) else {
        eprintln!("SKIP: no mise-installed claude binary");
        return;
    };
    claude_live::refuse_wrapper(&claude, &real_home);
    let Some(python) = claude_live::python3() else {
        eprintln!("SKIP: python3 is not available");
        return;
    };
    let scratch = claude_live::Scratch::new("toggle-reload", &real_home);
    let version_text = claude_live::version_text(&claude, &scratch.root);
    let Some(version) = sirio_claude::ClaudeVersion::parse(&version_text) else {
        panic!("claude reported an unreadable version");
    };
    if !version.meets_floor() {
        eprintln!("SKIP: claude {version} is below the claimed floor");
        return;
    }
    let advertises = version >= sirio_claude::ClaudeVersion::new(2, 1, 325);
    eprintln!("claude reports {version_text}");

    let root = scratch.root.join("mcp");
    let work_key = root.join("work").display().to_string();
    // `projects[<work>].disabledMcpServers` of the config dir's `.claude.json`, read once the
    // CLI has exited and so written its config; `None` when the file, the project or the list
    // is missing or unreadable.
    let disabled_in_file = |session: &claude_live::Session| -> Option<Vec<String>> {
        let text = std::fs::read_to_string(session.config_dir().join(".claude.json")).ok()?;
        let json: serde_json::Value = serde_json::from_str(&text).ok()?;
        let list = json["projects"][work_key.as_str()]["disabledMcpServers"].as_array()?;
        Some(
            list.iter()
                .filter_map(|name| name.as_str().map(str::to_string))
                .collect(),
        )
    };
    // The state `mcp_status` gives `probe`, or `None` when it is not listed.
    let probe_state = |session: &mut claude_live::Session,
                       seen: &mut Vec<serde_json::Value>,
                       id: &str|
     -> Option<McpState> {
        let answer = ask_session(session, seen, sirio_claude::ControlRequest::mcp_status(id));
        let servers = if answer["subtype"] == "success" {
            sirio_claude::parse_mcp_status(&answer["response"])
        } else {
            None
        };
        servers
            .unwrap_or_else(|| panic!("mcp_status answered a list: false"))
            .into_iter()
            .find(|server| server.name == "probe")
            .map(|server| server.status)
    };
    // Closes `session` and waits for nothing of its run to be left.
    let finish = |session: &mut claude_live::Session| {
        let exit = session.close_cli();
        assert!(
            exit.is_some_and(|status| status.success()),
            "the CLI exited successfully on stdin EOF: false (exit code {:?}, None when killed after 20 s)",
            exit.map(|status| status.code())
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut left = claude_live::holders_of(&session.home());
        while left > 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
            left = claude_live::holders_of(&session.home());
        }
        assert!(
            left == 0,
            "no process of the run outlives the CLI: false ({left} left)"
        );
    };

    let mut session = claude_live::Session::start_with_user_mcp(&claude, &python, &root, true);
    let mut seen = Vec::new();
    let init = ask_session(
        &mut session,
        &mut seen,
        sirio_claude::ControlRequest::initialize("tr-init"),
    );
    assert_eq!(
        init["subtype"], "success",
        "initialize answered success: false"
    );
    let mut catalog = sirio_claude::Catalog::from_initialize(&init["response"]);
    let capable = catalog.has_capability("reload_plugins_refuse_if_busy_v1");
    // 6.
    if advertises {
        assert!(
            capable,
            "the initialize answer lists reload_plugins_refuse_if_busy_v1: {capable}"
        );
    }

    // The server is listed and not disabled before any toggle.
    let first = probe_state(&mut session, &mut seen, "tr-s0");
    let listed = first.is_some();
    assert!(listed, "the user-scope server is listed: {listed}");
    let starts_up = matches!(first, Some(McpState::Pending | McpState::Connected));
    assert!(starts_up, "the server starts pending or connected: {starts_up}");

    // 1-3.
    let off = ask_session(
        &mut session,
        &mut seen,
        sirio_claude::ControlRequest::mcp_toggle("tr-off", "probe", false),
    );
    std::fs::write(scratch.root.join("off-answer.json"), off.to_string()).unwrap();
    assert_eq!(off["subtype"], "success", "the toggle off succeeded: false");
    let now = probe_state(&mut session, &mut seen, "tr-s1");
    let disabled = now == Some(McpState::Disabled);
    assert!(disabled, "mcp_status reads the server Disabled: {disabled}");

    // 4, 5.
    let before: Vec<String> = catalog.commands().into_iter().map(|c| c.name).collect();
    let reload = ask_session(
        &mut session,
        &mut seen,
        sirio_claude::ControlRequest::reload_plugins("tr-reload", true, false),
    );
    std::fs::write(scratch.root.join("reload-answer.json"), reload.to_string()).unwrap();
    assert_eq!(
        reload["subtype"], "success",
        "the held reload succeeded: false"
    );
    let parsed = sirio_claude::parse_plugins_reload(&reload["response"]);
    let parses = parsed.is_some();
    assert!(parses, "the reload answer parses: {parses}");
    let held = parsed.is_some_and(|reloaded| reloaded.held);
    assert!(!held, "the reload before any turn was held: {held}");
    if advertises {
        let explicit = reload["response"]["held"] == serde_json::Value::Bool(false);
        assert!(explicit, "the reload answer carries held: false: {explicit}");
    }
    catalog.replace_commands(&reload["response"]["commands"]);
    let after: Vec<String> = catalog.commands().into_iter().map(|c| c.name).collect();
    let had_commands = !before.is_empty();
    assert!(had_commands, "the initialize answer listed commands: {had_commands}");
    let non_empty = !after.is_empty();
    let keeps_all = before.iter().all(|name| after.contains(name));
    assert!(non_empty, "the reload gave commands: {non_empty}");
    assert!(keeps_all, "the reload kept every command: {keeps_all}");

    // 12: before the switch goes back on, so a refusal-shaped answer cannot hide behind it.
    let unknown = ask_session(
        &mut session,
        &mut seen,
        sirio_claude::ControlRequest::mcp_toggle("tr-nosuch", "no-such-server", true),
    );
    let is_error = unknown["subtype"] == "error";
    assert!(is_error, "a toggle of an unknown server is an error: {is_error}");
    let refusal = unknown["error"]
        .as_str()
        .is_some_and(sirio_claude::is_refusal);
    assert!(!refusal, "that error is a refusal: {refusal}");

    // 7, then 3: the exit comes first, so the CLI has written its config.
    finish(&mut session);
    let saved = disabled_in_file(&session).is_some_and(|list| list.iter().any(|n| n == "probe"));
    assert!(saved, "the folder's disabledMcpServers lists it: {saved}");
    drop(session);

    // 8.
    let mut session = claude_live::Session::start_with_user_mcp(&claude, &python, &root, false);
    let mut seen = Vec::new();
    let init = ask_session(
        &mut session,
        &mut seen,
        sirio_claude::ControlRequest::initialize("tr2-init"),
    );
    assert_eq!(
        init["subtype"], "success",
        "initialize answered success: false"
    );
    let state = probe_state(&mut session, &mut seen, "tr2-s0");
    let still_off = state == Some(McpState::Disabled);
    assert!(still_off, "a fresh session reads it Disabled: {still_off}");

    // 9-11.
    let on = ask_session(
        &mut session,
        &mut seen,
        sirio_claude::ControlRequest::mcp_toggle("tr2-on", "probe", true),
    );
    assert_eq!(on["subtype"], "success", "the toggle on succeeded: false");
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut n = 0;
    let left_disabled = loop {
        n += 1;
        let state = probe_state(&mut session, &mut seen, &format!("tr2-s{n}"));
        if matches!(state, Some(McpState::Pending | McpState::Connected)) {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    assert!(
        left_disabled,
        "mcp_status reached pending or connected within 15 s: {left_disabled}"
    );
    // 13.
    finish(&mut session);
    // 11: after the exit, `[]` is on disk (a missing list fails, it is not "not listed").
    let cleared = disabled_in_file(&session).is_some_and(|list| !list.iter().any(|n| n == "probe"));
    assert!(cleared, "the folder's disabledMcpServers no longer lists it: {cleared}");
}

// A7 live proof. Failure list, written before the tests: each of these is a way the live
// proof would be wrong, or would prove nothing.
//   - The variable is missing from the launch, so the proxy case gives no result.
//   - Sirio's parser misses a live field name, so a startup reason or a retry field reads as
//     absent.
//   - The startup result is not stdout line 0, so something else could answer first.
//   - The CLI is left running after it failed to start (checked: stdout reaches EOF, the exit
//     is read, and no process of the run's HOME is left once it is closed).
//   - The CLI reaches past the fake API. This holds by construction, not by an assertion: the
//     isolation (`env_clear`, `ANTHROPIC_BASE_URL` on 127.0.0.1 under `NO_PROXY`, the dead
//     proxy). The early stop in `read_to_eof` adds that a CLI which answers the handshake or
//     opens a session after all is killed at once.
//   - The live `api_retry` shape differs from the fixture's (attempt, error, status, delay).
//   - The answer the CLI sends with no stream carries the id of a streamed message, which
//     would make the fold's rule drop it.
//   - A run that SKIPs is read as a pass: both tests print `claude reports <version>` first.

/// The CLI, the python and the scratch of a live test, or `None` after printing why it SKIPs.
/// The version line `claude reports ...` is printed before the floor check.
fn live_claude(
    name: &str,
    at_least: sirio_claude::ClaudeVersion,
) -> Option<(
    std::path::PathBuf,
    std::path::PathBuf,
    claude_live::Scratch,
)> {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        eprintln!("SKIP: HOME is not set");
        return None;
    };
    let real_home = home.canonicalize().unwrap_or_else(|_| home.clone());
    let Some(claude) = claude_live::resolve_claude(&home) else {
        eprintln!("SKIP: no mise-installed claude binary");
        return None;
    };
    claude_live::refuse_wrapper(&claude, &real_home);
    let Some(python) = claude_live::python3() else {
        eprintln!("SKIP: python3 is not available");
        return None;
    };
    let scratch = claude_live::Scratch::new(name, &real_home);
    let version_text = claude_live::version_text(&claude, &scratch.root);
    let Some(version) = sirio_claude::ClaudeVersion::parse(&version_text) else {
        panic!("claude reported an unreadable version");
    };
    eprintln!("claude reports {version_text}");
    if !version.meets_floor() || version < at_least {
        eprintln!("SKIP: claude {version} is below what this test needs");
        return None;
    }
    Some((claude, python, scratch))
}

/// What `read_to_eof` saw.
struct Startup {
    lines: Vec<serde_json::Value>,
    /// Stdout reached EOF.
    eof: bool,
    /// The CLI answered the handshake or opened a session (a `control_response` or a
    /// `system/init` line): it started after all. Reading stops at once, so the test fails
    /// there and the session's drop kills it.
    ran: bool,
}

/// Reads a CLI that was only sent `initialize` until its stdout closes, 30 s pass, or it
/// shows it started.
fn read_to_eof(session: &mut claude_live::Session) -> Startup {
    use claude_live::Polled;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut lines = Vec::new();
    loop {
        match session.poll(deadline) {
            Polled::Line(line) => {
                let ran = line["type"] == "control_response"
                    || (line["type"] == "system" && line["subtype"] == "init");
                lines.push(line);
                if ran {
                    return Startup { lines, eof: false, ran: true };
                }
            }
            Polled::Timeout => return Startup { lines, eof: false, ran: false },
            Polled::Closed => return Startup { lines, eof: true, ran: false },
        }
    }
}

/// Waits up to 2 s for no process to name the session's HOME, then asserts none does.
fn assert_nothing_left(session: &claude_live::Session) {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut left = claude_live::holders_of(&session.home());
    while left > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        left = claude_live::holders_of(&session.home());
    }
    assert!(left == 0, "no process of the run outlives the CLI: false");
}

/// Proves the real `claude` writes a `result` naming why it could not start, as line 0, and
/// that `CLAUDE_CODE_STARTUP_FAILURE_RESULTS` (the variable `LaunchLine::chat` sets) is what
/// makes the proxy case write one.
///
/// Three launches, each sent `initialize` and no prompt: a corrupt `.claude.json` in the
/// scratch config dir (written by the harness, never by Sirio), an invalid `HTTPS_PROXY` with
/// the variable, and the same without it (the control). It uses the fake API of the
/// session-state test and nothing beyond 127.0.0.1, but launches a real CLI three times, so it
/// is `#[ignore]`d and stays off CI:
///
///     TMPDIR=/var/tmp/sirio-live cargo test -p sirio_agents --test acp_conformance -- --ignored --nocapture --test-threads=1 claude_reports_why_it_could_not_start
///
/// It SKIPs without a resolvable mise `claude` binary, below 2.1.325 and without `python3`.
#[test]
#[ignore = "launches the real claude three times"]
fn claude_reports_why_it_could_not_start() {
    use claude_live::BrokenStart;
    use sirio_claude::{CliMessage, ControlRequest, StartupFailure};

    let Some((claude, python, scratch)) =
        live_claude("startup-reasons", sirio_claude::ClaudeVersion::new(2, 1, 325))
    else {
        return;
    };

    let launch = |name: &str, broken: BrokenStart| {
        let mut session = claude_live::Session::start_broken(
            &claude,
            &python,
            &scratch.root.join(name),
            broken,
        );
        // The CLI may already be gone: its result is read either way.
        let _ = session.try_write(&ControlRequest::initialize("startup-init"));
        let startup = read_to_eof(&mut session);
        assert!(!startup.ran, "the CLI refused to start, it did not answer: false");
        assert!(startup.eof, "stdout reached EOF within 30 s: false");
        let exit = session.close_cli();
        assert_nothing_left(&session);
        (startup.lines, exit)
    };
    let result_of = |line: &serde_json::Value| match CliMessage::parse(&line.to_string()) {
        Some(CliMessage::Result(result)) => Some(result),
        _ => None,
    };

    // 1. A corrupt config file.
    let (lines, exit) = launch("corrupt", BrokenStart::CorruptConfig);
    let first = lines.first().and_then(result_of);
    assert!(first.is_some(), "the first line is a result: false");
    let failure = StartupFailure::from_result(&first.unwrap());
    assert!(failure.is_some(), "the first result is an error: false");
    let failure = failure.unwrap();
    assert_eq!(
        failure.reason.as_deref(),
        Some("config_file_invalid"),
        "the reason is config_file_invalid"
    );
    let global = failure
        .invalid_config_file
        .as_ref()
        .is_some_and(|file| file.is_global);
    assert!(global, "the config file is the global one: {global}");
    let has_errors = !failure.errors.is_empty();
    assert!(has_errors, "the errors are not empty: {has_errors}");
    let failed = exit.is_some_and(|status| !status.success());
    assert!(failed, "the CLI exited with a non-zero status: {failed}");

    // 2. An invalid proxy, with the variable.
    let (lines, exit) = launch("proxy-on", BrokenStart::BadProxy { opt_in: true });
    let first = lines.first().and_then(result_of);
    assert!(first.is_some(), "the first line is a result: false");
    let failure = StartupFailure::from_result(&first.unwrap());
    assert!(failure.is_some(), "the first result is an error: false");
    assert_eq!(
        failure.unwrap().reason.as_deref(),
        Some("proxy_invalid"),
        "the reason is proxy_invalid"
    );
    let failed = exit.is_some_and(|status| !status.success());
    assert!(failed, "the CLI exited with a non-zero status: {failed}");

    // 3. The same without the variable: no result, only a failed exit.
    let (lines, exit) = launch("proxy-off", BrokenStart::BadProxy { opt_in: false });
    let any_result = lines.iter().any(|line| result_of(line).is_some());
    assert!(!any_result, "no result line without the variable: {any_result}");
    let failed = exit.is_some_and(|status| !status.success());
    assert!(failed, "the CLI exited with a non-zero status: {failed}");
}

/// One turn against a CLI: sends `prompt`, then returns every line up to and including the
/// turn's `result`, or panics after 60 s without one.
fn run_turn(session: &mut claude_live::Session, prompt: &str) -> Vec<serde_json::Value> {
    session.write(&serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": [{"type": "text", "text": prompt}]},
        "parent_tool_use_id": null,
        "session_id": "",
    }));
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut lines = Vec::new();
    while let Some(line) = session.next(deadline) {
        let done = line["type"] == "result";
        lines.push(line);
        if done {
            return lines;
        }
    }
    panic!("the turn ended with a result within 60 s: false");
}

/// Proves the live shapes Sirio's notices read: `system/api_retry` (two retries of an
/// overloaded API, with the status, delay and budget) and an answer no stream carried (the CLI
/// falls back to a non-streaming request after a stream broke, and its message id was never
/// named by a `message_start`).
///
/// One CLI, two turns against the session-state fake API (`overload twice`, `fail midway`). It
/// uses no credential and nothing beyond 127.0.0.1, but launches a real CLI, so it is
/// `#[ignore]`d and stays off CI:
///
///     TMPDIR=/var/tmp/sirio-live cargo test -p sirio_agents --test acp_conformance -- --ignored --nocapture --test-threads=1 claude_reports_api_retries_and_unstreamed_answers
///
/// It SKIPs without a resolvable mise `claude` binary, below the claimed floor and without
/// `python3`.
#[test]
#[ignore = "launches the real claude"]
fn claude_reports_api_retries_and_unstreamed_answers() {
    use sirio_claude::{CliMessage, ControlRequest, MessageLifecycle};

    let Some((claude, python, scratch)) =
        live_claude("api-retries", sirio_claude::ClaudeVersion::new(2, 1, 257))
    else {
        return;
    };
    let mut session =
        claude_live::Session::start_state(&claude, &python, &scratch.root.join("retries"));
    session.write(&ControlRequest::initialize("retry-init"));

    let parsed = |lines: &[serde_json::Value]| -> Vec<CliMessage> {
        lines
            .iter()
            .filter_map(|line| CliMessage::parse(&line.to_string()))
            .collect()
    };
    let is_system = |message: &CliMessage, subtype: &str| {
        matches!(message, CliMessage::System(system) if system.subtype == subtype)
    };
    let start_id = |message: &CliMessage| match message {
        CliMessage::StreamEvent(event) => match event.lifecycle() {
            Some(MessageLifecycle::Start { id, .. }) => Some(id),
            _ => None,
        },
        _ => None,
    };

    // Turn 1: two overloads, then the answer.
    let turn = parsed(&run_turn(&mut session, "overload twice"));
    let retries: Vec<(usize, &sirio_claude::SystemPayload)> = turn
        .iter()
        .enumerate()
        .filter_map(|(at, message)| match message {
            CliMessage::System(system) if system.subtype == "api_retry" => Some((at, system)),
            _ => None,
        })
        .collect();
    assert_eq!(retries.len(), 2, "exactly two api_retry lines in the turn");
    let first_start = turn.iter().position(|message| start_id(message).is_some());
    assert!(first_start.is_some(), "the turn's answer is streamed: false");
    for (number, (at, retry)) in retries.iter().enumerate() {
        assert_eq!(retry.attempt, Some(number as u32 + 1), "the attempt counts from 1");
        assert_eq!(retry.error.as_deref(), Some("overloaded"), "the cause is overloaded");
        assert_eq!(retry.error_status, Some(529), "the status is 529");
        let delay = retry.retry_delay_ms.unwrap_or(0);
        assert!(delay > 0, "the retry delay is positive: false");
        let budget = retry.max_retries.unwrap_or(0);
        assert!(budget >= 2, "the retry budget covers both retries: false");
        let requested = turn[..*at].iter().any(|message| {
            matches!(message, CliMessage::System(system)
                if system.subtype == "status" && system.status.as_deref() == Some("requesting"))
        });
        assert!(requested, "a status requesting comes before the retry: {requested}");
        let before_stream = first_start.is_some_and(|start| *at < start);
        assert!(before_stream, "the retry comes before the first message_start: {before_stream}");
    }
    let ended_ok = turn
        .iter()
        .any(|message| matches!(message, CliMessage::Result(result) if !result.is_error));
    assert!(ended_ok, "the turn's result is not an error: {ended_ok}");

    // Turn 2: a stream that breaks, then an answer from a non-streaming request.
    let turn = parsed(&run_turn(&mut session, "fail midway"));
    let retried = turn.iter().any(|message| is_system(message, "api_retry"));
    assert!(!retried, "no api_retry line when a stream breaks: {retried}");
    let streamed: Vec<Option<String>> = turn.iter().filter_map(start_id).collect();
    // A stream started and broke: its message_start and its text arrived first.
    let started = streamed.contains(&Some("msg_m".to_string()));
    assert!(started, "the broken stream's message_start arrived: {started}");
    let partial = turn.iter().any(|message| {
        matches!(message, CliMessage::StreamEvent(event)
            if event.delta() == Some(sirio_claude::Delta::Text("partial-before-error".to_string())))
    });
    assert!(partial, "the broken stream's text delta arrived: {partial}");
    let unstreamed = turn.iter().any(|message| match message {
        CliMessage::Assistant(assistant) => {
            assistant.text() == "ok"
                && assistant.message.id.is_some()
                && !streamed.contains(&assistant.message.id)
        }
        _ => false,
    });
    assert!(
        unstreamed,
        "an assistant answer ok whose id no message_start named: {unstreamed}"
    );
    let ended_ok = turn
        .iter()
        .any(|message| matches!(message, CliMessage::Result(result) if !result.is_error));
    assert!(ended_ok, "the turn's result is not an error: {ended_ok}");

    let exit = session.close_cli();
    let clean = exit.is_some_and(|status| status.success());
    assert!(clean, "the CLI exited successfully on stdin EOF: {clean}");
    assert_nothing_left(&session);
}

/// The lines of a turn that parse as a `CliMessage`.
fn parsed_turn(lines: &[serde_json::Value]) -> Vec<sirio_claude::CliMessage> {
    lines
        .iter()
        .filter_map(|line| sirio_claude::CliMessage::parse(&line.to_string()))
        .collect()
}

/// The requests the fake limits API logged so far (`fake_limits_api.py` writes one line of
/// method and path per request to the limits file's `.requests` sibling). Every line counts,
/// whether or not it parses.
fn logged_requests(limits: &std::path::Path) -> Vec<String> {
    let mut path = limits.as_os_str().to_owned();
    path.push(".requests");
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// Proves the real `claude` answers `/usage` itself: a synthetic assistant line naming the
/// command (`local_command_run`) and a `result` with no turn and `local_command` set, with no
/// request to the model API at all. A positive control (`say hi`) then shows the API logs a
/// request when the CLI does call it, so "no request" cannot be a logger that never writes.
///
/// One CLI against the rate-limit fake API (`fake_limits_api.py`), API-key auth, scratch HOME
/// and nothing beyond 127.0.0.1. It launches a real CLI, so it is `#[ignore]`d and stays off CI:
///
///     TMPDIR=/var/tmp/sirio-live cargo test -p sirio_agents --test acp_conformance -- --ignored --nocapture --test-threads=1 claude_answers_usage_locally
///
/// It SKIPs without a resolvable mise `claude` binary, below 2.1.301 and without `python3`.
#[test]
#[ignore = "launches the real claude"]
fn claude_answers_usage_locally() {
    use sirio_claude::{CliMessage, ControlRequest};

    let Some((claude, python, scratch)) =
        live_claude("usage-local", sirio_claude::ClaudeVersion::new(2, 1, 301))
    else {
        return;
    };
    let limits = scratch.root.join("limits.json");
    std::fs::write(&limits, r#"{"status": 200, "headers": {}}"#).unwrap();
    let mut session = claude_live::Session::start_limits(
        &claude,
        &python,
        &scratch.root.join("usage"),
        &limits,
        None,
    );
    session.write(&ControlRequest::initialize("usage-init"));

    // `/usage`: the marker, the result, and no request.
    let lines = run_turn(&mut session, "/usage");
    let turn = parsed_turn(&lines);
    let marker = turn.iter().position(|message| {
        matches!(message, CliMessage::Assistant(assistant)
            if assistant.parent_tool_use_id.is_none()
                && assistant.local_command_run.as_ref().is_some_and(|run| run.command == "usage"))
    });
    assert!(marker.is_some(), "a top-level assistant line with local_command_run usage");
    let result = lines.iter().find(|line| line["type"] == "result");
    assert!(result.is_some(), "the turn ends with a result");
    let result = result.unwrap();
    assert!(result["num_turns"] == 0, "the result's num_turns is 0: false");
    assert!(
        result["local_command"] == "usage",
        "the result's local_command is usage: false"
    );
    assert!(
        logged_requests(&limits).is_empty(),
        "/usage sent no request to the model API"
    );

    // Positive control: a prompt the CLI does send logs exactly one request.
    let _ = run_turn(&mut session, "say hi");
    let requests = logged_requests(&limits);
    assert!(requests.len() == 1, "one request after say hi: false");
    let logged: serde_json::Value = serde_json::from_str(&requests[0]).unwrap_or_default();
    let path = logged["path"].as_str().unwrap_or_default();
    assert!(
        path.starts_with("/v1/messages"),
        "the request is a /v1/messages call"
    );

    let exit = session.close_cli();
    let clean = exit.is_some_and(|status| status.success());
    assert!(clean, "the CLI exited successfully on stdin EOF: {clean}");
    assert_nothing_left(&session);
}

/// The headers of a fake API answer that carry `status` and the given window fields, written
/// as `anthropic-ratelimit-unified-*` (copied from the A9 probe's `r_q4.py`).
fn unified_headers(
    status: &str,
    claim: &str,
    fields: &[(&str, String)],
) -> serde_json::Map<String, serde_json::Value> {
    let mut headers = serde_json::Map::new();
    headers.insert(
        "anthropic-ratelimit-unified-status".into(),
        status.into(),
    );
    headers.insert(
        "anthropic-ratelimit-unified-representative-claim".into(),
        claim.into(),
    );
    for (name, value) in fields {
        headers.insert(
            format!("anthropic-ratelimit-unified-{}", name.replace('_', "-")),
            value.clone().into(),
        );
    }
    headers
}

/// An unsigned gateway-shaped JWT, built at run time: base64url of `{"alg":"none"}`, a payload
/// with `aud: claude-gateway` and an expiry an hour out, and a dummy signature. Not a
/// credential of anything.
fn gateway_token() -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    fn base64url(bytes: &[u8]) -> String {
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |acc, (at, byte)| acc | u32::from(*byte) << (16 - 8 * at));
            for at in 0..=chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * at) & 63) as usize] as char);
            }
        }
        out
    }
    let exp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let header = serde_json::json!({"alg": "none"}).to_string();
    let payload = serde_json::json!({"aud": "claude-gateway", "exp": exp}).to_string();
    format!("{}.{}.x", base64url(header.as_bytes()), base64url(payload.as_bytes()))
}

/// Proves the real `claude` turns `anthropic-ratelimit-unified-*` response headers into the
/// `rate_limit_event` Sirio reads: an `allowed_warning` on the five-hour window with its
/// utilization and reset, then an overage refusal that carries its `limitScope` and ends the
/// turn with an error result.
///
/// The CLI emits the event only under subscriber or gateway auth, so the token is a fake
/// gateway JWT built at run time and passed to the CLI as `ANTHROPIC_AUTH_TOKEN` alone: it
/// lives only in the CLI's environment and the test's TMPDIR. `CliEnv::apply` refuses to
/// start the CLI unless its base URL is on 127.0.0.1. The CLI runs in a scratch HOME against
/// `fake_limits_api.py`, so nothing leaves the machine. It launches a real CLI, so it is
/// `#[ignore]`d and stays off CI:
///
///     TMPDIR=/var/tmp/sirio-live cargo test -p sirio_agents --test acp_conformance -- --ignored --nocapture --test-threads=1 claude_reports_rate_limits_through_a_gateway
///
/// It SKIPs without a resolvable mise `claude` binary, below 2.1.327 (the env-token gateway was
/// seen only there) and without `python3`. On 2.1.327 or newer a missing event fails.
#[test]
#[ignore = "launches the real claude"]
fn claude_reports_rate_limits_through_a_gateway() {
    use sirio_claude::{CliMessage, ControlRequest};

    let Some((claude, python, scratch)) =
        live_claude("rate-limits", sirio_claude::ClaudeVersion::new(2, 1, 327))
    else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let in_three_hours = now + 3 * 3600;
    let in_four_days = now + 4 * 86400;
    let limits = scratch.root.join("limits.json");
    let write_limits = |status: u16, headers: serde_json::Map<String, serde_json::Value>| {
        std::fs::write(
            &limits,
            serde_json::json!({"status": status, "headers": headers}).to_string(),
        )
        .unwrap();
    };
    // The probe's case `warning-5h-92`.
    write_limits(
        200,
        unified_headers(
            "allowed_warning",
            "five_hour",
            &[
                ("reset", in_three_hours.to_string()),
                ("5h_utilization", "0.92".into()),
                ("5h_reset", in_three_hours.to_string()),
                ("5h_surpassed_threshold", "0.9".into()),
                ("5h_status", "allowed_warning".into()),
                ("7d_utilization", "0.1".into()),
                ("7d_reset", in_four_days.to_string()),
            ],
        ),
    );
    let token = gateway_token();
    let mut session = claude_live::Session::start_limits(
        &claude,
        &python,
        &scratch.root.join("limits"),
        &limits,
        Some(&token),
    );
    session.write(&ControlRequest::initialize("limits-init"));

    // Turn 1: the warning, before the result.
    let lines = run_turn(&mut session, "case warning-5h-92");
    let turn = parsed_turn(&lines);
    let warning = turn.iter().position(|message| {
        matches!(message, CliMessage::RateLimit(push)
            if push.status.as_deref() == Some("allowed_warning"))
    });
    assert!(warning.is_some(), "an allowed_warning rate_limit_event in the turn");
    let CliMessage::RateLimit(push) = &turn[warning.unwrap()] else {
        unreachable!()
    };
    assert!(
        push.rate_limit_type.as_deref() == Some("five_hour"),
        "the window is five_hour: false"
    );
    assert!(push.used_percent == Some(92), "the window is 92 percent used: false");
    assert!(
        push.resets_at == Some(in_three_hours),
        "the event carries the written reset: false"
    );
    // `run_turn` stops at the turn's result, so an event in `turn` came before it.

    // Turn 2: an overage refusal charged to the pooled budget (`rej-overage-orgcap-group_pool`).
    write_limits(
        429,
        unified_headers(
            "rejected",
            "overage",
            &[
                ("reset", in_four_days.to_string()),
                ("overage_status", "rejected".into()),
                ("overage_disabled_reason", "org_spend_cap_reached".into()),
                ("overage_scope", "group_pool".into()),
                ("overage_reset", in_four_days.to_string()),
            ],
        ),
    );
    let lines = run_turn(&mut session, "case rej-overage-orgcap-group_pool");
    let turn = parsed_turn(&lines);
    let rejection = turn.iter().position(|message| {
        matches!(message, CliMessage::RateLimit(push)
            if push.status.as_deref() == Some("rejected"))
    });
    assert!(rejection.is_some(), "a rejected rate_limit_event in the turn");
    let CliMessage::RateLimit(push) = &turn[rejection.unwrap()] else {
        unreachable!()
    };
    assert!(
        push.rate_limit_type.as_deref() == Some("overage"),
        "the window is overage: false"
    );
    assert!(
        push.limit_scope.as_deref() == Some("group_pool"),
        "the scope is group_pool: false"
    );
    let failed = turn
        .iter()
        .any(|message| matches!(message, CliMessage::Result(result) if result.is_error));
    assert!(failed, "the refused turn's result is an error: {failed}");

    // The last turn ended in an error result, and a CLI whose last result is one may exit
    // non-zero: what matters is that it exits on stdin EOF instead of being killed.
    let exit = session.close_cli();
    assert!(exit.is_some(), "the CLI exited on stdin EOF without being killed");
    assert_nothing_left(&session);
}

/// Checks the shape of a real account's `/usage` answer, on the user's own `claude`.
///
/// This is the one live test that touches a real account, so it runs only by hand, by the
/// project owner, and never in CI or by an agent: it SKIPs unless `SIRIO_LIVE_CLAUDE_ACCOUNT=1`.
/// It launches the user's `claude` in their environment, in a fresh directory under TMPDIR,
/// with the launch line of Sirio's usage probe (`--no-session-persistence`,
/// `--setting-sources ""`), so no hook or MCP server of theirs runs and no session file is
/// written. It sends only `/usage` (no model turn, so nothing is spent) and prints no value of
/// the account, only one status line of structure counts:
///
///     SIRIO_LIVE_CLAUDE_ACCOUNT=1 TMPDIR=/var/tmp/sirio-live cargo test -p sirio_agents --test acp_conformance -- --ignored --nocapture --test-threads=1 claude_usage_report_on_account
///
/// What a fresh `/usage` can prove, and this asserts: the marker (`local_command_run` usage),
/// `num_turns` 0 and, when the answer carries a `usage_report`, that `UsageReport::parse`
/// reads it and that every limit row names its kind, group and severity. The status line says
/// whether a report came, whether its limits were rows or unavailable, and how many rows and
/// models were checked, so a pass that checked nothing shows as such.
///
/// The four token counts of a `model_usage` entry are checked only for models the session
/// already holds, which a fresh session does not: its `session.model_usage` is empty, so
/// "models checked" is normally 0 and those keys are not proven against the wire by this test.
/// SKIPs below 2.1.301, where `local_command_run` first appears.
#[test]
#[ignore = "runs the user's own claude against their real account, by hand only"]
fn claude_usage_report_on_account() {
    use sirio_agents::ClaudeCodeAdapter;
    use sirio_claude::{CliMessage, ControlRequest, PlanLimits, UsageReport};

    if std::env::var("SIRIO_LIVE_CLAUDE_ACCOUNT").as_deref() != Ok("1") {
        eprintln!("SKIP: SIRIO_LIVE_CLAUDE_ACCOUNT=1 is not set");
        return;
    }
    let Some(program) = ClaudeCodeAdapter.availability().executable else {
        eprintln!("SKIP: claude is not on PATH");
        return;
    };
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        eprintln!("SKIP: HOME is not set");
        return;
    };
    let real_home = home.canonicalize().unwrap_or(home);
    let scratch = claude_live::Scratch::new("usage-account", &real_home);

    // `claude --version` in the user's own environment, within 30 s.
    let version_path = scratch.root.join("version.out");
    let mut version_child = Command::new(&program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&version_path).unwrap())
        .stderr(Stdio::null())
        .spawn()
        .expect("claude --version runs");
    let version_deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if version_child.try_wait().expect("claude --version is waited on").is_some() {
            break;
        }
        if Instant::now() >= version_deadline {
            let _ = version_child.kill();
            let _ = version_child.wait();
            panic!("claude --version did not finish in 30 s");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let version_text = std::fs::read_to_string(&version_path).unwrap_or_default();
    let version_text = version_text.trim();
    let Some(version) = sirio_claude::ClaudeVersion::parse(version_text) else {
        panic!("claude reported an unreadable version");
    };
    eprintln!("claude reports {version_text}");
    if !version.meets_floor() || version < sirio_claude::ClaudeVersion::new(2, 1, 301) {
        eprintln!("SKIP: claude {version} is below what this test needs");
        return;
    }
    let work = scratch.root.join("work");
    std::fs::create_dir_all(&work).unwrap();

    /// Kills the CLI on every exit path, after the test had its chance to let it finish.
    struct Kill(std::process::Child);
    impl Drop for Kill {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let line = sirio_claude::LaunchLine::usage_probe();
    let mut child = Kill(
        Command::new(&program)
            .args(&line.args)
            .current_dir(&work)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("claude launches"),
    );
    let mut stdin = child.0.stdin.take().expect("a piped stdin");
    let stdout = child.0.stdout.take().expect("a piped stdout");
    let (line_tx, line_rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line_tx.send(line).is_err() {
                return;
            }
        }
    });
    writeln!(stdin, "{}", ControlRequest::initialize("account-init")).expect("initialize is written");
    let usage = serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": [{"type": "text", "text": "/usage"}]},
        "parent_tool_use_id": null,
        "session_id": "",
    });
    writeln!(stdin, "{usage}").expect("/usage is written");
    stdin.flush().expect("stdin is flushed");

    let deadline = Instant::now() + Duration::from_secs(60);
    let mut lines = Vec::new();
    while let Ok(text) = line_rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let done = value["type"] == "result";
        lines.push(value);
        if done {
            break;
        }
    }
    // Close stdin and give the CLI up to 10 s to exit by itself: it may still be writing to
    // the user's own config directory, and only then is it killed.
    drop(stdin);
    let exit_deadline = Instant::now() + Duration::from_secs(10);
    while child.0.try_wait().ok().flatten().is_none() && Instant::now() < exit_deadline {
        std::thread::sleep(Duration::from_millis(50));
    }

    let result = lines.iter().find(|line| line["type"] == "result");
    assert!(result.is_some(), "/usage ends with a result within 60 s");
    assert!(result.unwrap()["num_turns"] == 0, "the result's num_turns is 0: false");
    let turn = parsed_turn(&lines);
    let answer = turn.iter().find_map(|message| match message {
        CliMessage::Assistant(assistant)
            if assistant.parent_tool_use_id.is_none()
                && assistant.local_command_run.as_ref().is_some_and(|run| run.command == "usage") =>
        {
            Some(assistant)
        }
        _ => None,
    });
    assert!(answer.is_some(), "a top-level assistant line with local_command_run usage");

    let mut present = false;
    let mut rows_state = "unavailable";
    let (mut rows_checked, mut models_checked) = (0usize, 0usize);
    if let Some(raw) = &answer.unwrap().usage_report {
        present = true;
        let report = UsageReport::parse(&serde_json::Value::Object(raw.clone()));
        assert!(report.is_some(), "the usage_report is readable");
        let report = report.unwrap();
        if let PlanLimits::Rows(rows) = &report.limits {
            rows_state = "rows";
            for row in rows {
                assert!(!row.kind.is_empty(), "every row has a kind");
                assert!(!row.group.is_empty(), "every row has a group");
                assert!(!row.severity.is_empty(), "every row has a severity");
                rows_checked += 1;
            }
        }
        for (_, tokens) in report.totals.iter().flat_map(|totals| &totals.models) {
            assert!(tokens.input.is_some(), "every model has its input count");
            assert!(tokens.output.is_some(), "every model has its output count");
            assert!(tokens.cache_read.is_some(), "every model has its cache read count");
            assert!(tokens.cache_write.is_some(), "every model has its cache write count");
            models_checked += 1;
        }
    }
    eprintln!(
        "usage_report: {}, limits: {rows_state}, rows checked: {rows_checked}, models checked: {models_checked}",
        if present { "present" } else { "absent" }
    );
}

/// The guard `CliEnv::apply` runs before a CLI that carries the gateway token starts (so at
/// every spawn site that applies it, `launch_broken` included). This test starts no CLI.
///
/// Failure list, written first: a userinfo that hides another host
/// (`http://127.0.0.1:1@example.com/`) passes a prefix check, and so would an empty port.
/// A bare port and a port with a path are the URLs the harness itself builds, so they must
/// pass.
#[test]
fn the_gateway_base_url_guard_checks_the_host() {
    for url in ["http://127.0.0.1:1@example.com/", "http://127.0.0.1:/"] {
        assert!(!claude_live::is_local_base_url(url), "a URL that is not the local API is refused");
    }
    for url in ["http://127.0.0.1:8080", "http://127.0.0.1:8080/v1"] {
        // Guard: holds before the change (the old prefix check also accepts these).
        assert!(claude_live::is_local_base_url(url), "a local API URL is accepted");
    }
}

// Failure list for `claude_serves_mcp_apps_to_sirio`, written before the test. It fails when:
// - the CLI's own MCP `initialize` does not declare `io.modelcontextprotocol/ui` with the
//   `text/html;profile=mcp-app` MIME type under `CLAUDE_CODE_MCP_APPS_HOST=1` (the launch
//   variable is not honoured, or Sirio does not set it);
// - `system/init` lacks any of the four capabilities a widget needs, as the relay in front of
//   the CLI logged them (`mcp_tool_ui_meta_v1`, `mcp_read_resource_v1`,
//   `mcp_call_result_not_for_model_v1`, `mcp_call_check_permissions_v1`);
// - the model's `mcp__apps__show` call does not reach the chat as `Called` and `Returned`, or
//   the result lost its `structuredContent`;
// - `mcp_status` does not carry `show`'s `resourceUri` and `app` visibility, or shows `act` to
//   the model (the `_meta.ui` of a tool is not forwarded);
// - `ui_tool_for` does not find `show`, or `mcp_read_resource` does not return a document
//   whose csp is the one the server declared;
// - the widget's `act` call fails, or the relay does not log exactly one `can_use_tool` ask
//   with a `toolu_mcp_call_` id for `mcp__apps__act` with `{"step":1}`, or the server did not
//   run `act` with `{"step":1}` (the ask is the one Sirio grants, and Sirio denies any ask it
//   did not grant);
// - any permission card reaches the chat, during the turn or in the 2 s after the widget's
//   call (the model's `show` is allowed by the user settings and the widget's call is
//   answered by Sirio alone);
// - the turn does not end within 60 s.

/// Proves the real `claude` and Sirio's MCP Apps plumbing agree on the wire: the host
/// capability reaches the server, a model call to a widget tool reaches the chat with its
/// structured result, `mcp_status` carries the tool's UI, and `mcp_read_resource` and
/// `mcp_call` (with the CLI's own permission ask, in the default permission mode) work through
/// the production client.
///
/// A fake model API (`fake_mcp_apps_api.py`) and a fake MCP server (`mcp_apps.py`) stand in for
/// the network. It uses no credential and no network beyond 127.0.0.1, but launches a real CLI,
/// so it is `#[ignore]`d and stays off CI:
///
///     TMPDIR=/var/tmp/sirio-live cargo test -p sirio_agents --test acp_conformance -- --ignored --exact claude_serves_mcp_apps_to_sirio --test-threads=1 --nocapture
///
/// It SKIPs without a resolvable mise `claude` binary, below the claimed floor, and without
/// `python3`. Its scratch directory is under the system temp dir, removed after a pass and kept
/// after a failure.
#[test]
#[ignore = "launches the real claude"]
fn claude_serves_mcp_apps_to_sirio() {
    use sirio_chat::{ChatEvent, ClaudeClient, ClaudeLaunch, McpToolItem, SessionNotice};

    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        eprintln!("SKIP: HOME is not set");
        return;
    };
    let real_home = home.canonicalize().unwrap_or_else(|_| home.clone());
    let Some(claude) = claude_live::resolve_claude(&home) else {
        eprintln!("SKIP: no mise-installed claude binary");
        return;
    };
    claude_live::refuse_wrapper(&claude, &real_home);
    let Some(python) = claude_live::python3() else {
        eprintln!("SKIP: python3 is not available");
        return;
    };
    let scratch = claude_live::Scratch::new("mcp-apps", &real_home);
    let version_text = claude_live::version_text(&claude, &scratch.root);
    eprintln!("claude reports {version_text}");
    let Some(version) = sirio_claude::ClaudeVersion::parse(&version_text) else {
        panic!("claude reported an unreadable version");
    };
    if !version.meets_floor() {
        eprintln!("SKIP: claude is below the claimed floor");
        return;
    }

    let rig = claude_live::AppsRig::start(&claude, &python, &scratch.root.join("apps"));
    let launch = ClaudeLaunch::fixture(
        std::path::PathBuf::from("/bin/sh"),
        vec![rig.launcher.display().to_string()],
    )
    .mcp_apps_host(true);
    let Ok((mut client, events)) = ClaudeClient::launch(launch, &rig.work) else {
        panic!("the CLI did not complete its handshake");
    };
    if client.prompt("run apps").is_err() {
        panic!("the prompt was not sent");
    }

    // The turn: the model calls `show`; the chat gets its `Called` and `Returned` notices.
    let mut tool_updates = Vec::new();
    let mut permission_cards = 0;
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut ended = false;
    while Instant::now() < deadline && !ended {
        match events.try_recv() {
            Ok(ChatEvent::TurnEnded { .. }) => ended = true,
            Ok(ChatEvent::PermissionRequest { request_id, .. }) => {
                permission_cards += 1;
                let _ = client.cancel_permission(request_id);
            }
            Ok(ChatEvent::OtherSessionUpdate { .. }) => {
                for notice in client.take_notices() {
                    if let SessionNotice::McpTool(update) = notice {
                        tool_updates.push(update);
                    }
                }
            }
            Ok(_) => {}
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    assert!(ended, "the turn ended within 60 s");
    for notice in client.take_notices() {
        if let SessionNotice::McpTool(update) = notice {
            tool_updates.push(update);
        }
    }

    // The server saw the host's MCP Apps capability in the CLI's own `initialize`.
    let log = std::fs::read_to_string(&rig.mcp_log).unwrap_or_default();
    let initializes: Vec<serde_json::Value> = log
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|entry| entry.get("initialize").cloned())
        .collect();
    assert!(!initializes.is_empty(), "the server saw an initialize");
    assert!(
        initializes.iter().any(|params| {
            params["capabilities"]["extensions"]["io.modelcontextprotocol/ui"]["mimeTypes"]
                == serde_json::json!(["text/html;profile=mcp-app"])
        }),
        "the server saw no MCP Apps capability"
    );

    let called = tool_updates.iter().find_map(|update| match &update.item {
        McpToolItem::Called { name, input } => Some((update.id.clone(), name.clone(), input.clone())),
        _ => None,
    });
    let (call_id, name, input) = called.expect("the chat got a Called notice");
    assert!(name == "mcp__apps__show", "the call names the show tool");
    assert!(
        input == serde_json::json!({"text": "hi"}),
        "the call carries the model's input"
    );
    let returned = tool_updates.iter().find_map(|update| match &update.item {
        McpToolItem::Returned { result } if update.id == call_id => Some(result.clone()),
        _ => None,
    });
    let returned = returned.expect("the chat got a Returned notice for the call");
    assert!(
        returned.get("structuredContent").is_some(),
        "the result carries structuredContent"
    );

    // `mcp_status` carries each tool's UI.
    let control = client.mcp_control();
    let Ok(servers) = control.refresh_status() else {
        panic!("mcp_status did not answer");
    };
    let apps = servers
        .iter()
        .find(|server| server.name == "apps")
        .expect("the apps server is listed");
    let ui_of = |tool: &str| {
        apps.tools
            .iter()
            .find(|candidate| candidate.name == tool)
            .and_then(|candidate| candidate.ui.clone())
    };
    let show = ui_of("show").expect("show carries a ui");
    assert!(
        show.resource_uri.as_deref() == Some("ui://apps/show.html"),
        "show names the widget resource"
    );
    assert!(show.for_app, "show is visible to the app");
    let act = ui_of("act").expect("act carries a ui");
    assert!(act.for_app && !act.for_model, "act is app only");
    assert!(
        sirio_claude::mcp::ui_tool_for(&servers, "mcp__apps__show").is_some(),
        "ui_tool_for finds show"
    );

    // The widget's resource, through the CLI, is a document with the declared csp.
    let Ok(contents) = control.read_resource("apps", "ui://apps/show.html") else {
        panic!("mcp_read_resource did not answer");
    };
    let Ok(document) =
        sirio_chat::mcp_apps::document::widget_document(&contents, "ui://apps/show.html")
    else {
        panic!("the resource is not a widget document");
    };
    assert!(
        document.csp.connect == ["https://api.widget.test"],
        "the document carries the declared connect domains"
    );

    // The widget's call: the CLI asks in the default mode, and Sirio answers its own grant.
    let Ok(acted) =
        control.call_tool_after_user_allowed("mcp__apps__act", serde_json::json!({"step": 1}))
    else {
        panic!("the widget's call was not allowed and answered");
    };
    assert!(
        acted.get("content").is_some_and(serde_json::Value::is_array),
        "the call's content is a block array"
    );

    // The widget's call raised no card: the events after the turn are drained for 2 s.
    let drain_until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < drain_until {
        match events.try_recv() {
            Ok(ChatEvent::PermissionRequest { .. }) => permission_cards += 1,
            Ok(_) => {}
            Err(_) if events.is_closed() => break,
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    assert!(permission_cards == 0, "no permission card reached the chat");

    // The wire, as the relay in front of the CLI saw it.
    let relay = jsonl_of(&rig.relay_log);
    let capabilities: Vec<&str> = relay
        .iter()
        .filter_map(|entry| entry.get("init_capabilities")?.as_array())
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .collect();
    for gate in [
        "mcp_tool_ui_meta_v1",
        "mcp_read_resource_v1",
        "mcp_call_result_not_for_model_v1",
        "mcp_call_check_permissions_v1",
    ] {
        assert!(
            capabilities.contains(&gate),
            "system/init lists every capability a widget needs"
        );
    }
    let mcp_call_asks: Vec<&serde_json::Value> = relay
        .iter()
        .filter_map(|entry| entry.get("ask"))
        .filter(|ask| ask["id_prefix"] == "toolu_mcp_call_")
        .collect();
    assert!(
        mcp_call_asks.len() == 1,
        "the CLI sent exactly one mcp_call ask"
    );
    assert!(
        mcp_call_asks[0]["tool_name"] == "mcp__apps__act",
        "the ask names the tool Sirio sent"
    );
    assert!(
        mcp_call_asks[0]["input"] == serde_json::json!({"step": 1}),
        "the ask carries the widget's input"
    );
    assert!(
        jsonl_of(&rig.mcp_log).iter().any(|call| {
            call["call"] == "act" && call["arguments"] == serde_json::json!({"step": 1})
        }),
        "the server ran act with the widget's input"
    );

    if client.shutdown().is_err() {
        panic!("the client did not shut down");
    }
}

/// Every JSON line of a test log.
fn jsonl_of(path: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}
