//! One real turn per installed CLI, through the hook config Sirio's adapter
//! writes for it. Each test prepares a scratch worktree with the adapter's own
//! `prepare`, runs the CLI's `say ok` turn, and serves `SIRIO_SOCKET` from a
//! recording listener. The hook's `sirioctl` call must arrive there, naming the
//! test's pane, and nothing else reaches the socket: every pane variable is
//! removed from the child, so a hook that does not name its pane stays silent.
//!
//! Each test prints `SKIP:` and returns when its CLI is not on PATH, when
//! `sirioctl` is not built beside this test binary (`cargo build -p sirio_control
//! --bin sirioctl`), when the installed `claude` lacks the flags that keep the
//! user's config out, or when the CLI exits for want of a login or of credit.
//!
//! Each live test spends a real turn, so each is `#[ignore]`d and runs only on
//! request:
//!
//!     cargo nextest run -p sirio_agents --test hooks_live --run-ignored only
//!
//! The scratch root is under `std::env::temp_dir()`. The tests expect `TMPDIR`
//! to be set, because without it that root is `/tmp`.

#![cfg(unix)]

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sirio_agents::{ALL, AgentAdapter, find_executable_on_path, shell_quote};

/// A turn that outlives this is killed, and its test fails.
const TURN_TIMEOUT: Duration = Duration::from_secs(120);

// libc's `kill`, declared here so this test takes no dependency for one call.
// `SIGKILL` is 9 on every unix Sirio runs on.
unsafe extern "C" {
    safe fn kill(pid: i32, signal: i32) -> i32;
}
const SIGKILL: i32 = 9;

const SIRIOCTL_NOT_BUILT: &str =
    "sirioctl is not built beside this test binary; run cargo build -p sirio_control --bin sirioctl";

/// How much of a CLI's output a failure prints: its last lines, cut short.
const TAIL_LINES: usize = 20;
const TAIL_WIDTH: usize = 200;

/// Phrases a CLI prints when it cannot run a turn for want of a login or of
/// credit, matched as whole phrases and checked only after a failed exit. Only
/// the fact is printed, never the output. Rate limits, overloads and network
/// errors are not here: they fail the test. Where each phrase comes from:
/// - `not logged in`, `please run /login`: claude, printed with no login (run
///   here with an empty HOME, which needs no credential).
/// - `missing bearer or basic authentication`: codex, its 401 for a request with
///   no credential (run here the same way).
/// - `out of usage credits`: claude's organisation message (a string in the
///   installed binary; not triggered, since a live attempt spends).
/// - `requires more credits`: OpenRouter's HTTP 402 text, as opencode printed it
///   in the live run of 2026-10-18.
const NO_ACCESS_PHRASES: &[&str] = &[
    "not logged in",
    "please run /login",
    "missing bearer or basic authentication",
    "out of usage credits",
    "requires more credits",
];

/// The flags that keep the user's global Claude config out of a turn.
/// `--setting-sources project,local` drops the user's settings, while `local`
/// still loads the `.claude/settings.local.json` that `prepare` wrote, so the
/// hooks under test still fire. `--strict-mcp-config` with no `--mcp-config`
/// starts no MCP server. Credentials are not a settings source, so the turn
/// authenticates as the user does.
const CLAUDE_ISOLATION_FLAGS: &[&str] = &["--setting-sources", "--strict-mcp-config"];

#[test]
#[ignore = "live: spends a real turn per CLI; run with --run-ignored only"]
fn claude_fires_its_hooks_for_one_turn() {
    let Some(claude) = find_executable_on_path("claude") else {
        println!("SKIP: claude is not on PATH");
        return;
    };
    let Some(sirioctl) = built_sirioctl() else {
        println!("SKIP: {SIRIOCTL_NOT_BUILT}");
        return;
    };
    let help = claude_help(&claude);
    if let Some(flag) = CLAUDE_ISOLATION_FLAGS.iter().find(|flag| !help.contains(*flag)) {
        println!("SKIP: claude --help has no {flag}, so a turn would load the user's global config");
        return;
    }
    let pane = "pane-hooks-live-claude";
    let scratch = Scratch::new("claude");
    prepare("claude", &scratch, pane, &sirioctl);

    let recorder = Recorder::start(&scratch.socket());
    let mut command = Command::new(claude);
    command.args([
        "-p",
        "say ok",
        "--max-turns",
        "1",
        "--setting-sources",
        "project,local",
        "--strict-mcp-config",
    ]);
    let turn = run_turn(command, &scratch);
    let requests = recorder.finish();
    if !turn_completed("claude", &turn) {
        return;
    }

    // SessionStart reports needs-input too, but before the turn's running. Only a
    // needs-input after the last running can be the Stop hook.
    let statuses = notify_statuses(&requests, pane);
    let stop_fired = statuses
        .iter()
        .rposition(|status| status == "running")
        .is_some_and(|last_running| {
            statuses[last_running + 1..].iter().any(|status| status == "needs-input")
        });
    assert!(
        stop_fired,
        "claude's Stop hook must report needs-input after the last running for {pane}; the \
         socket got {statuses:?} in arrival order from {} request(s)",
        requests.len()
    );
}

/// The text `claude --help` prints, which names the flags the installed version takes.
fn claude_help(claude: &Path) -> String {
    Command::new(claude)
        .arg("--help")
        .output()
        .map(|output| {
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })
        .unwrap_or_default()
}

#[test]
#[ignore = "live: spends a real turn per CLI; run with --run-ignored only"]
fn codex_fires_its_notify_hook_for_one_turn() {
    if find_executable_on_path("codex").is_none() {
        println!("SKIP: codex is not on PATH");
        return;
    }
    let Some(sirioctl) = built_sirioctl() else {
        println!("SKIP: {SIRIOCTL_NOT_BUILT}");
        return;
    };
    let pane = "pane-hooks-live-codex";
    let scratch = Scratch::new("codex");
    prepare("codex", &scratch, pane, &sirioctl);
    // `codex exec` refuses a directory that is not a git repository.
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(scratch.worktree())
        .status();

    // The notify hook rides on the command line the adapter launches Codex with,
    // so the line is built the way `sirio` builds it, `exec` first so that the
    // shell's child is Codex itself.
    let launch = adapter("codex").command(
        &scratch.worktree().to_string_lossy(),
        pane,
        &sirioctl.to_string_lossy(),
    );
    let line = format!("exec {launch} exec {}", shell_quote("say ok"));
    let recorder = Recorder::start(&scratch.socket());
    let mut command = Command::new("sh");
    command.args(["-c", &line]);
    let turn = run_turn(command, &scratch);
    let requests = recorder.finish();
    if !turn_completed("codex", &turn) {
        return;
    }

    let statuses = notify_statuses(&requests, pane);
    assert!(
        statuses.contains(&"needs-input".to_string()),
        "codex's turn must report needs-input for {pane}; the socket got {statuses:?} from \
         {} request(s)",
        requests.len()
    );
}

#[test]
#[ignore = "live: spends a real turn per CLI; run with --run-ignored only"]
fn opencode_reports_its_session_for_one_turn() {
    let Some(opencode) = find_executable_on_path("opencode") else {
        println!("SKIP: opencode is not on PATH");
        return;
    };
    let Some(sirioctl) = built_sirioctl() else {
        println!("SKIP: {SIRIOCTL_NOT_BUILT}");
        return;
    };
    let pane = "pane-hooks-live-opencode";
    let scratch = Scratch::new("opencode");
    prepare("opencode", &scratch, pane, &sirioctl);

    // OpenCode's plugin reports a session reference, not a status: it has no
    // lifecycle hook for one, so the request this turn must produce is `session.ref`.
    let recorder = Recorder::start(&scratch.socket());
    let mut command = Command::new(opencode);
    command.args(["run", "say ok"]);
    let turn = run_turn(command, &scratch);
    let requests = recorder.finish();
    if !turn_completed("opencode", &turn) {
        return;
    }

    let references: Vec<&serde_json::Value> = requests
        .iter()
        .filter(|request| request["method"] == "session.ref" && request["params"]["session"] == pane)
        .collect();
    assert!(
        !references.is_empty(),
        "opencode's turn must report its session reference for {pane}; the socket got {} \
         request(s), methods {:?}",
        requests.len(),
        requests.iter().map(|request| request["method"].to_string()).collect::<Vec<_>>()
    );
}

/// The adapter Sirio launches `id` with, from the same catalogue `sirio` reads.
fn adapter(id: &str) -> &'static dyn AgentAdapter {
    *ALL
        .iter()
        .find(|adapter| adapter.id() == id)
        .expect("the adapter is in the catalogue")
}

/// Writes the hook config for `id` into the scratch worktree, the way a pane
/// launch does, and names `sirioctl` as the hook's binary.
fn prepare(id: &str, scratch: &Scratch, pane: &str, sirioctl: &Path) {
    adapter(id)
        .prepare(
            &scratch.worktree().to_string_lossy(),
            pane,
            &sirioctl.to_string_lossy(),
        )
        .expect("the adapter prepares its worktree");
}

/// The `sirioctl` built beside this test binary, `target/<profile>/sirioctl`.
/// `CARGO_BIN_EXE_sirioctl` is set only for the package that owns the binary.
fn built_sirioctl() -> Option<PathBuf> {
    let test_binary = std::env::current_exe().ok()?;
    let profile = test_binary.parent()?.parent()?;
    let path = profile.join("sirioctl");
    path.is_file().then_some(path)
}

/// The statuses of the `notify` requests that name `pane`, in arrival order.
fn notify_statuses(requests: &[serde_json::Value], pane: &str) -> Vec<String> {
    requests
        .iter()
        .filter(|request| request["method"] == "notify" && request["params"]["session"] == pane)
        .filter_map(|request| request["params"]["status"].as_str().map(str::to_string))
        .collect()
}

/// A scratch directory for one test: its worktree and its recording socket. It
/// sits under `std::env::temp_dir()`, which is `TMPDIR` when that is set, and the
/// test expects `TMPDIR` to be set; it is removed when the test ends.
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("hooks-live-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("wt")).expect("create the scratch worktree");
        Self { root }
    }

    fn worktree(&self) -> PathBuf {
        self.root.join("wt")
    }

    /// Kept short: a unix socket path is limited to about 104 bytes.
    fn socket(&self) -> PathBuf {
        self.root.join("s.sock")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Serves `SIRIO_SOCKET` for one turn. Each request is recorded, and each gets
/// the answer a running Sirio gives a status or session report.
struct Recorder {
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
    stop: Arc<AtomicBool>,
    serving: Option<JoinHandle<()>>,
}

impl Recorder {
    fn start(socket: &Path) -> Self {
        let listener = UnixListener::bind(socket).expect("bind the recording socket");
        listener
            .set_nonblocking(true)
            .expect("make the recording socket pollable");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let serving = std::thread::spawn({
            let requests = requests.clone();
            let stop = stop.clone();
            move || {
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => serve_one(stream, &requests),
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(20));
                        }
                        Err(_) => break,
                    }
                }
            }
        });
        Self {
            requests,
            stop,
            serving: Some(serving),
        }
    }

    /// Stops serving and returns every request received, in arrival order.
    fn finish(mut self) -> Vec<serde_json::Value> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(serving) = self.serving.take() {
            let _ = serving.join();
        }
        std::mem::take(&mut *self.requests.lock().expect("recorded requests"))
    }
}

/// One connection: a line of JSON in, the answer a running Sirio gives, a line out.
fn serve_one(stream: UnixStream, requests: &Mutex<Vec<serde_json::Value>>) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let Ok(request) = serde_json::from_str::<serde_json::Value>(&line) else {
        return;
    };
    let id = request["id"].as_str().unwrap_or_default().to_string();
    requests.lock().expect("recorded requests").push(request);
    let mut stream = reader.into_inner();
    let answer = serde_json::json!({ "id": id, "ok": true, "result": {} });
    let _ = writeln!(stream, "{answer}");
}

/// What one turn left behind: how it ended and what the CLI printed.
struct Turn {
    status: Option<ExitStatus>,
    transcript: String,
}

/// Runs one turn in the worktree. `SIRIO_SOCKET` names the recorder, and the pane
/// variables are removed, so only a hook that names its pane can reach it. The
/// turn is killed at `TURN_TIMEOUT`.
fn run_turn(mut command: Command, scratch: &Scratch) -> Turn {
    let transcript_path = scratch.root.join("turn.txt");
    let transcript = File::create(&transcript_path).expect("create the turn's transcript");
    // Its own process group, so a timeout takes the CLI's children down with it.
    let mut child = command
        .current_dir(scratch.worktree())
        .env("SIRIO_SOCKET", scratch.socket())
        .env_remove("SIRIO_PANE_ID")
        .env_remove("TILLER_PANE_ID")
        .env_remove("SIRIO_WORKTREE_ID")
        .stdin(Stdio::null())
        .stdout(transcript.try_clone().expect("clone the transcript"))
        .stderr(transcript)
        .process_group(0)
        .spawn()
        .expect("start the CLI");
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll the CLI") {
            break Some(status);
        }
        if started.elapsed() > TURN_TIMEOUT {
            let _ = kill(-(child.id() as i32), SIGKILL);
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let transcript = std::fs::read_to_string(&transcript_path).unwrap_or_default();
    Turn { status, transcript }
}

/// Whether the turn ran to a successful exit. A CLI that exited for want of a
/// login or credit is a SKIP; any other failure panics with the tail of its output.
fn turn_completed(name: &str, turn: &Turn) -> bool {
    match turn.status {
        Some(status) if status.success() => true,
        Some(_) if has_no_access(&turn.transcript) => {
            println!("SKIP: {name} exited without a usable login or credit");
            false
        }
        Some(status) => panic!(
            "{name} exited with {status}; last output:\n{}",
            tail(&turn.transcript)
        ),
        None => panic!(
            "{name} did not finish within {TURN_TIMEOUT:?}; last output:\n{}",
            tail(&turn.transcript)
        ),
    }
}

/// Whether the transcript says, as whole phrases, that the CLI cannot run a turn
/// for want of a login or credit.
fn has_no_access(transcript: &str) -> bool {
    NO_ACCESS_PHRASES
        .iter()
        .any(|phrase| has_phrase(transcript, phrase))
}

/// Whether `phrase` (lower case) occurs in `text`, compared lower-cased, with no
/// letter, digit or `_` on either side: `not logged in` is not in `not logged indoors`.
fn has_phrase(text: &str, phrase: &str) -> bool {
    let text = text.to_lowercase();
    text.match_indices(phrase).any(|(start, matched)| {
        let before = text[..start].chars().next_back();
        let after = text[start + matched.len()..].chars().next();
        !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
    })
}

fn is_word_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn tail(transcript: &str) -> String {
    let lines: Vec<&str> = transcript.lines().collect();
    let start = lines.len().saturating_sub(TAIL_LINES);
    lines[start..]
        .iter()
        .map(|line| line.chars().take(TAIL_WIDTH).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Only a login or credit phrase makes a failed turn a SKIP. A rate limit, an
/// overload or a network error is a failure, and a phrase inside a longer word
/// is not the phrase.
#[test]
fn a_login_or_credit_phrase_is_a_skip_and_nothing_else_is() {
    // Printed by claude with no login, and by codex when a request carries no credential.
    assert!(has_no_access("Not logged in · Please run /login"));
    assert!(has_no_access(
        "unexpected status 401 Unauthorized: Missing bearer or basic authentication in header"
    ));
    // Failures, not SKIPs.
    assert!(!has_no_access("API Error: 429 rate limited, retry in 20 seconds"));
    assert!(!has_no_access("API Error: 529 overloaded"));
    assert!(!has_no_access("error: connection refused by api.anthropic.com (network error)"));
    // A phrase inside a longer word is not the phrase.
    assert!(!has_no_access("they are not logged indoors"));
}
