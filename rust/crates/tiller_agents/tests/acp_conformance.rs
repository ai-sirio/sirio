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

use tiller_agents::{AgentAdapter, OhMyPiAdapter, OpenCodeAdapter};

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

#[test]
fn answers_initialize_times_out_for_silent_cli() {
    let executable = std::env::current_exe().expect("test binary path");
    let started = Instant::now();
    let timeout = Duration::from_millis(300);
    let result = answers_initialize_within(
        &executable.to_string_lossy(),
        &[
            "--exact",
            "silent_child_never_answers",
            "--ignored",
            "--format",
            "terse",
        ],
        timeout,
    );
    let elapsed = started.elapsed();

    let Handshake::TimedOut { output } = &result else {
        panic!("expected TimedOut, got {result:?}");
    };
    assert!(!output.is_empty(), "timed-out handshake discarded stdout");
    assert!(
        elapsed < Duration::from_secs(1),
        "timeout took {elapsed:?} — the deadline is not enforced"
    );
}

#[test]
fn answers_initialize_reports_unlaunchable_cli() {
    let missing = std::env::current_exe()
        .expect("test binary path")
        .with_file_name("tiller-acp-missing-executable");
    let result = answers_initialize(&missing.to_string_lossy(), &[]);

    assert_eq!(
        result,
        Handshake::NotLaunched,
        "an unlaunchable CLI must be reported as NotLaunched"
    );
}

#[test]
#[ignore = "spawned as the silent child by the timeout test"]
fn silent_child_never_answers() {
    std::thread::sleep(Duration::from_secs(3));
    std::process::exit(0);
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
