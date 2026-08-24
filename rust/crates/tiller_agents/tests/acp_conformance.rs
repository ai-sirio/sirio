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
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use tiller_agents::{AgentAdapter, OhMyPiAdapter, OpenCodeAdapter};

/// How long `answers_initialize` will wait for a reply. 30 s is generous on
/// purpose: Node/Bun-hosted CLIs cold-start slowly, so the deadline must not
/// be tight.
const ANSWERS_TIMEOUT: Duration = Duration::from_secs(30);

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false}}}}"#;

fn answers_initialize(program: &str, args: &[&str]) -> Option<String> {
    answers_initialize_within(program, args, ANSWERS_TIMEOUT)
}

/// Spawn `program` with `args`, send it an ACP `initialize` request, and
/// return the first reply line — but never block longer than `timeout`.
/// On timeout the child is killed AND reaped before `None` is returned; the
/// kill+wait also runs on every earlier exit path, closing the un-reaped
/// child gap this helper used to leave.
fn answers_initialize_within(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut answer = None;
    // Only read if the request actually went in; any failure skips straight
    // to the kill+wait below.
    if child
        .stdin
        .as_mut()
        .is_some_and(|stdin| stdin.write_all(format!("{INITIALIZE}\n").as_bytes()).is_ok())
        && let Some(stdout) = child.stdout.take()
    {
        // `child.stdout.take()` hands the pipe to the reader thread while the
        // parent keeps the `Child`, so it can still kill it on timeout.
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut line = String::new();
            if let Ok(read) = BufReader::new(stdout).read_line(&mut line) {
                let _ = tx.send((read > 0).then_some(line));
            }
        });
        // recv_timeout bounds the block; a timed-out or disconnected channel
        // both mean no answer within the deadline.
        answer = match rx.recv_timeout(timeout) {
            Ok(reply) => reply,
            Err(_) => None,
        };
    }
    let _ = child.kill();
    let _ = child.wait();
    answer
}

#[test]
fn answers_initialize_within_times_out_on_a_silent_child() {
    // A child that starts and never speaks: the same never-exits spelling
    // used elsewhere in this repo.
    let (program, args) = if cfg!(windows) {
        ("cmd", vec!["/C", "ping -n 30 127.0.0.1 > nul"])
    } else {
        ("sh", vec!["-c", "sleep 30"])
    };
    let start = Instant::now();
    let answer = answers_initialize_within(program, &args, Duration::from_millis(500));
    let elapsed = start.elapsed();
    assert!(answer.is_none(), "a silent child must time out, not answer");
    assert!(
        elapsed < Duration::from_secs(2),
        "returned after {elapsed:?}; expected it to return at the deadline, not wait the child out"
    );
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
    let response = answers_initialize(&program.to_string_lossy(), claim.args)
        .expect("opencode acp answered nothing on stdout");
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
    let answered = answers_initialize(&program.to_string_lossy(), &["acp"])
        .is_some_and(|response| response.contains("\"protocolVersion\""));
    assert_eq!(
        OhMyPiAdapter.builtin_acp().is_some(),
        answered,
        "the adapter's claim and the binary's behaviour must match"
    );
}
