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
/// It is the one conformance test that spends a turn (a real Haiku one, in which a
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
            "6",
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
