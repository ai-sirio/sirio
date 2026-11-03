//! What the real agent CLIs report in a terminal session, against local fake model APIs.
//!
//! Each test starts a plain shell session of the engine (`Session::spawn`), types the
//! agent's command into it as a person would, gives the agent one or two prompts, and
//! records the `Event::Program` / `Event::Status` sequence the engine emits. A real CLI
//! update that changes what an agent reports turns a test red. Every test prints the
//! sequence it saw, so a failure is legible.
//!
//! The model API is a script in `tests/live/`, started on a free 127.0.0.1 port and killed
//! by the test. Every run has a private HOME, private `XDG_*`, and each CLI's own config
//! directory variable pointing into it; the child's environment is built from nothing, so
//! none of the user's variables reach it. Each CLI is resolved through a private `bin/` of
//! symlinks that is first on the shell's PATH, never through the user's PATH, from the path
//! in the variable named below or from its default:
//!
//! | CLI        | variable                  | default                                          |
//! |------------|---------------------------|--------------------------------------------------|
//! | `claude`   | `SIRIO_LIVE_CLAUDE`       | `~/.local/share/mise/installs/claude/2.1.316/claude` |
//! | `codex`    | `SIRIO_LIVE_CODEX`        | `~/.codex/packages/standalone/current/bin/codex` |
//! | `omp`      | `SIRIO_LIVE_OMP`          | `~/.local/bin/omp`                               |
//! | `pi`       | `SIRIO_LIVE_PI`           | `~/.local/share/mise/installs/pi/1.4.0/pi/pi`    |
//!
//! The defaults are the real binaries, not the `~/.local/bin/claude` and `~/.local/bin/pi`
//! wrappers: those run `mise use -g`, which edits the user's global mise config. A CLI that
//! is not there prints `SKIP:` and the test passes. A binary is only executed: its user
//! configuration directory (`~/.claude`, `~/.codex`, ...) is never read or written.
//!
//! These tests run the real CLIs, so each is `#[ignore]`d and runs only on request:
//!
//!     cargo nextest run -p sirio_term_session --test program_status_live --run-ignored only
//!
//! The scratch root is under `std::env::temp_dir()`; set `TMPDIR` to a directory on disk and
//! outside your home: several agents read instruction files (`CLAUDE.md`, `AGENTS.md`) from
//! every ancestor of the directory they run in, and Claude's are switched off here too.
//! After a run the test kills every process still holding the private HOME in its
//! environment (a CLI's background helper), on Linux, where `/proc` shows them.

#![cfg(unix)]

use std::collections::BTreeMap;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use sirio_term_session::{Event, Format, Session, SpawnSpec};

const FAKE_MESSAGES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/live/fake_messages.py");
const FAKE_RESPONSES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/live/fake_responses.py");
const FAKE_CHAT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/live/fake_chat.py");

/// The most any one wait takes; a CLI's first start is slow (Codex needs 15 s or more).
const START_TIMEOUT: Duration = Duration::from_secs(90);
const TURN_TIMEOUT: Duration = Duration::from_secs(60);
/// A fake model holds each request this long, so a `working` state is there to be seen.
const HOLD: &str = "3";

// ---------------------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------------------

#[test]
#[ignore = "live: runs the real CLI against a local fake model API; run with --run-ignored only"]
fn claude_reports_once_answered() {
    let Some(mut rig) = Rig::new("claude", "SIRIO_LIVE_CLAUDE", ".local/share/mise/installs/claude/2.1.316/claude")
    else {
        return;
    };
    let api = rig.fake(FAKE_MESSAGES);
    rig.claude_config(api, ClaudePermissions::Ask);
    let mut run = rig.start("claude", "claude");

    // Claude reports only once the terminal answers its probe, which the engine does.
    run.wait_status("the startup idle", START_TIMEOUT, |s| s == "idle app=claude-code");
    let turn = run.mark();
    run.prompt("say ok");
    run.wait_status("working", TURN_TIMEOUT, |s| s == "working app=claude-code");
    run.wait_status("done", TURN_TIMEOUT, |s| s == "done app=claude-code");
    run.expect_sequence_since(turn, &["working app=claude-code", "done app=claude-code"]);

    // A tool the settings ask about: the dialog must report `blocked`, with its kind.
    let turn = run.mark();
    run.prompt("run tool");
    run.wait_status("blocked", TURN_TIMEOUT, |s| s.starts_with("blocked kind=permission app=claude-code"));
    run.expect_sequence_since(turn, &["working app=claude-code", "blocked kind=permission app=claude-code"]);
    run.key("\r"); // the dialog's first choice, "Yes"
    run.wait_status("done after the tool", TURN_TIMEOUT, |s| s == "done app=claude-code");
    run.expect_sequence_since(
        turn,
        &["working app=claude-code", "blocked kind=permission app=claude-code", "done app=claude-code"],
    );
    run.finish();
}

#[test]
#[ignore = "live: runs the real CLI against a local fake model API; run with --run-ignored only"]
fn codex_reports_unprompted() {
    let Some(mut rig) = Rig::new("codex", "SIRIO_LIVE_CODEX", ".codex/packages/standalone/current/bin/codex") else {
        return;
    };
    let api = rig.fake(FAKE_RESPONSES);
    rig.codex_config(api);
    let mut run = rig.start("codex", "codex");

    // Codex reports at startup without being asked; a working->idle blip within about 20 ms
    // of it is allowed.
    run.wait_status("the startup idle", START_TIMEOUT, |s| s == "idle app=codex");
    run.settle(Duration::from_secs(2));
    let turn = run.mark();
    run.prompt("say ok");
    run.wait_status("working", TURN_TIMEOUT, |s| s == "working app=codex");
    run.wait_status("done", TURN_TIMEOUT, |s| s == "done app=codex");
    run.expect_sequence_since(turn, &["working app=codex", "done app=codex"]);

    // An escalation request is an approval dialog.
    let turn = run.mark();
    run.prompt("run it");
    run.wait_status("blocked", TURN_TIMEOUT, |s| s.starts_with("blocked kind=permission app=codex"));
    run.expect_sequence_since(turn, &["working app=codex", "blocked kind=permission app=codex"]);
    run.key("\r"); // "Yes, proceed"
    run.wait_status("done after the approval", TURN_TIMEOUT, |s| s == "done app=codex");
    run.finish();
}

#[test]
#[ignore = "live: runs the real CLI against a local fake model API; run with --run-ignored only"]
fn omp_reports_idle() {
    let Some(mut rig) = Rig::new("omp", "SIRIO_LIVE_OMP", ".local/bin/omp") else {
        return;
    };
    let api = rig.fake(FAKE_CHAT);
    rig.omp_config(api);
    let mut run = rig.start("omp", "omp");

    // omp reports at startup and sends no probe.
    run.wait_status("the startup idle", START_TIMEOUT, |s| s == "idle app=omp");
    let turn = run.mark();
    run.prompt("say ok");
    run.wait_status("working", TURN_TIMEOUT, |s| s == "working app=omp");
    run.wait_status("done", TURN_TIMEOUT, |s| s == "done app=omp");
    run.expect_sequence_since(turn, &["working app=omp", "done app=omp"]);
    run.finish();
}

#[test]
#[ignore = "live: runs the real CLI against a local fake model API; run with --run-ignored only"]
fn pi_reports_what_task_2_found() {
    let Some(mut rig) = Rig::new("pi", "SIRIO_LIVE_PI", ".local/share/mise/installs/pi/1.4.0/pi/pi") else {
        return;
    };
    let api = rig.fake(FAKE_CHAT);
    rig.pi_config(api);
    let mut run = rig.start("pi", "pi --provider fake --model m --offline --no-extensions --approve");

    // Pi reports only once the terminal answers its probe.
    run.wait_status("the startup idle", START_TIMEOUT, |s| s == "idle app=pi");
    let turn = run.mark();
    run.prompt("say ok");
    run.wait_status("working", TURN_TIMEOUT, |s| s == "working app=pi");
    run.wait_status("done", TURN_TIMEOUT, |s| s == "done app=pi");
    run.expect_sequence_since(turn, &["working app=pi", "done app=pi"]);
    run.finish();
}

#[test]
#[ignore = "live: runs the real CLI against a local fake model API; run with --run-ignored only"]
fn program_is_the_agent_during_a_tool_run() {
    // Claude: a Bash tool that takes 3 s, allowed without a dialog.
    if let Some(mut rig) = Rig::new("claude-tool", "SIRIO_LIVE_CLAUDE", ".local/share/mise/installs/claude/2.1.316/claude") {
        let api = rig.fake(FAKE_MESSAGES);
        rig.claude_config(api, ClaudePermissions::AllowBash);
        let mut run = rig.start("claude", "claude");
        run.wait_status("the startup idle", START_TIMEOUT, |s| s == "idle app=claude-code");
        tool_run_keeps_the_agent("claude", &mut run, &rig, "run tool", "done app=claude-code");
        run.finish();
    }
    // Codex: a shell command that takes 3 s, inside its sandbox, no approval.
    if let Some(mut rig) = Rig::new("codex-tool", "SIRIO_LIVE_CODEX", ".codex/packages/standalone/current/bin/codex") {
        let api = rig.fake(FAKE_RESPONSES);
        rig.codex_config(api);
        let mut run = rig.start("codex", "codex");
        run.wait_status("the startup idle", START_TIMEOUT, |s| s == "idle app=codex");
        run.settle(Duration::from_secs(2));
        tool_run_keeps_the_agent("codex", &mut run, &rig, "sleep it", "done app=codex");
        run.finish();
    }
}

/// Sends a prompt that makes the agent run a 3 s shell command, and checks the foreground
/// program stays the agent while it runs. If the foreground group does change, the status
/// record must survive it (the host keeps the record when the terminal returns to the
/// agent's group): no clear, and a last report of `done`.
fn tool_run_keeps_the_agent(agent: &str, run: &mut Run, rig: &Rig, prompt: &str, done: &str) {
    let probe = rig.work().join("probe-file");
    let _ = std::fs::remove_file(&probe);
    let turn = run.mark();
    run.prompt(prompt);
    run.wait_status("done", TURN_TIMEOUT, |s| s == done);
    assert!(probe.exists(), "the tool never ran: {} was not created\n{}", probe.display(), run.report());

    let programs: Vec<String> = run.entries_since(turn).iter().filter_map(Seen::program).collect();
    let other: Vec<&String> = programs.iter().filter(|p| *p != &format!("program {agent}")).collect();
    let cleared = run.entries_since(turn).iter().any(|e| e.what == What::Status("clear".into()));
    if other.is_empty() {
        println!("{agent}: the foreground program never left {agent} during the tool run (programs seen: {programs:?})");
    } else {
        println!("{agent}: the foreground group changed during the tool run: {programs:?}; checking the record survived");
        assert!(!cleared, "the status record was cleared during the tool run\n{}", run.report());
        assert_eq!(run.last_status_since(turn).as_deref(), Some(done), "{}", run.report());
    }
}

// ---------------------------------------------------------------------------------------
// The rig: a private HOME, a private bin/, fake model APIs, and the sessions on them
// ---------------------------------------------------------------------------------------

struct Rig {
    root: PathBuf,
    cli: String,
    children: Vec<Child>,
    /// Variables for the CLI, on top of the private HOME and XDG_*.
    extra: BTreeMap<String, String>,
}

impl Rig {
    /// `None` after printing `SKIP:` when the CLI (or python3) is not there.
    fn new(name: &str, var: &str, default: &str) -> Option<Self> {
        let cli = name.split('-').next().unwrap().to_string();
        let path = match std::env::var_os(var) {
            Some(path) => PathBuf::from(path),
            None => PathBuf::from(std::env::var_os("HOME")?).join(default),
        };
        if !path.is_file() {
            println!("SKIP: {cli} not found (set {var} to its path)");
            return None;
        }
        if Command::new("python3").arg("--version").output().is_err() {
            println!("SKIP: python3 not found");
            return None;
        }
        let root = std::env::temp_dir().join(format!("sirio-live-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["home", "bin", "work", "xdg/config", "xdg/data", "xdg/state", "xdg/cache", "xdg/runtime"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let root = root.canonicalize().unwrap();
        std::os::unix::fs::symlink(&path, root.join("bin").join(&cli)).unwrap();
        Some(Self { root, cli, children: Vec::new(), extra: BTreeMap::new() })
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn work(&self) -> PathBuf {
        self.root.join("work")
    }

    /// Starts a fake model API on a free port and returns the port.
    fn fake(&mut self, script: &str) -> u16 {
        let port = free_port();
        let log = std::fs::File::create(self.root.join(format!("fake-{port}.log"))).unwrap();
        let child = Command::new("python3")
            .args(["-I", script, &port.to_string(), HOLD])
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("start the fake model API");
        self.children.push(child);
        let deadline = Instant::now() + Duration::from_secs(10);
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(Instant::now() < deadline, "the fake model API did not come up");
            std::thread::sleep(Duration::from_millis(50));
        }
        port
    }

    /// Writes `text` at `path` under the private HOME.
    fn write(&self, path: &str, text: &str) {
        let file = self.home().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    fn claude_config(&mut self, port: u16, permissions: ClaudePermissions) {
        let work = self.work().display().to_string();
        // Claude remembers an approved key by its last 20 characters.
        let key_tail = &FAKE_KEY[FAKE_KEY.len() - 20..];
        let global = format!(
            r#"{{"hasCompletedOnboarding":true,"theme":"dark","hasSeenAutoDefaultNotice":true,"hasResetAutoModeOptInForDefaultOffer":true,"customApiKeyResponses":{{"approved":["{key_tail}"],"rejected":[]}},
"projects":{{{work:?}:{{"hasTrustDialogAccepted":true,"hasCompletedProjectOnboarding":true}}}}}}"#
        );
        self.write(".claude.json", &global);
        self.write(".claude/.claude.json", &global);
        self.write(".claude/settings.json", &permissions.settings());
        let config_dir = self.home().join(".claude").display().to_string();
        self.env_extra(&[
            ("ANTHROPIC_API_KEY", FAKE_KEY.to_string()),
            ("ANTHROPIC_BASE_URL", format!("http://127.0.0.1:{port}")),
            ("DISABLE_AUTOUPDATER", "1".into()),
            ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1".into()),
            // Claude reads CLAUDE.md files from every ancestor of its directory.
            ("CLAUDE_CODE_DISABLE_CLAUDE_MDS", "1".into()),
            ("CLAUDE_CONFIG_DIR", config_dir),
        ]);
    }

    fn codex_config(&mut self, port: u16) {
        let work = self.work().display().to_string();
        self.write(
            ".codex/config.toml",
            &format!(
                r#"model_provider = "fake"
model = "gpt-fake"
approval_policy = "on-request"
sandbox_mode = "workspace-write"
check_for_update_on_startup = false

[model_providers.fake]
name = "fake"
base_url = "http://127.0.0.1:{port}/v1"
env_key = "FAKE_KEY"
wire_api = "responses"

[tui]
animations = false
screen_reader_detection_done = true

[tui.model_availability_nux]
"gpt-6.1-sol" = 4

[projects.{work:?}]
trust_level = "trusted"
"#
            ),
        );
        let codex_home = self.home().join(".codex").display().to_string();
        self.env_extra(&[("CODEX_HOME", codex_home), ("FAKE_KEY", "x".into())]);
        self.offline();
    }

    /// omp looks for a newer release and Codex syncs its plugins from GitHub on startup: a
    /// dead proxy keeps that off the network, and the fake API on 127.0.0.1 is exempt.
    fn offline(&mut self) {
        self.env_extra(&[
            ("HTTPS_PROXY", "http://127.0.0.1:9".into()),
            ("HTTP_PROXY", "http://127.0.0.1:9".into()),
            ("NO_PROXY", "127.0.0.1,localhost".into()),
        ]);
    }

    fn omp_config(&mut self, port: u16) {
        self.write(
            ".omp/agent/models.yml",
            &format!(
                "providers:\n  fake:\n    baseUrl: http://127.0.0.1:{port}/v1\n    apiKey: x\n    api: openai-completions\n    models:\n      - id: m\n        name: m\n"
            ),
        );
        self.write(".omp/agent/config.yml", "setupVersion: 2\nmodelRoles:\n  default: fake/m\n");
        self.offline();
    }

    fn pi_config(&mut self, port: u16) {
        self.write(
            ".pi/agent/models.json",
            &format!(
                r#"{{"providers":{{"fake":{{"baseUrl":"http://127.0.0.1:{port}/v1","api":"openai-completions","apiKey":"x","models":[{{"id":"m"}}]}}}}}}"#
            ),
        );
        self.write(".pi/agent/auth.json", "{}");
        self.write(".pi/agent/settings.json", r#"{"lastChangelogVersion":"1.4.0"}"#);
        let agent_dir = self.home().join(".pi/agent").display().to_string();
        self.env_extra(&[("PI_CODING_AGENT_DIR", agent_dir)]);
    }

    fn env_extra(&mut self, pairs: &[(&str, String)]) {
        for (key, value) in pairs {
            self.extra.insert((*key).into(), value.clone());
        }
    }

    /// The whole environment of the session's shell: nothing of the user's.
    fn env(&self) -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        let xdg = |name: &str| self.root.join("xdg").join(name).display().to_string();
        env.insert("HOME".into(), self.home().display().to_string());
        env.insert("XDG_CONFIG_HOME".into(), xdg("config"));
        env.insert("XDG_DATA_HOME".into(), xdg("data"));
        env.insert("XDG_STATE_HOME".into(), xdg("state"));
        env.insert("XDG_CACHE_HOME".into(), xdg("cache"));
        env.insert("XDG_RUNTIME_DIR".into(), xdg("runtime"));
        env.insert("PATH".into(), format!("{}:/usr/bin:/bin", self.root.join("bin").display()));
        env.insert("LANG".into(), "C.UTF-8".into());
        env.insert("SHELL".into(), "/bin/sh".into());
        env.extend(self.extra.clone());
        env
    }

    /// Spawns an interactive shell in the private work directory and types `command`.
    fn start(&self, program: &str, command: &str) -> Run {
        let (events, received) = mpsc::channel();
        let session = Session::spawn(
            SpawnSpec {
                id: format!("live-{}", self.cli),
                cwd: self.work(),
                program: Some(("/bin/sh".into(), Vec::new())),
                env: self.env(),
                cols: 120,
                rows: 40,
                prelude: Vec::new(),
            },
            events,
        )
        .expect("spawn the shell session");
        let mut run = Run {
            session,
            received,
            began: Instant::now(),
            seen: Vec::new(),
            last_mark: 0,
        };
        run.settle(Duration::from_millis(800)); // the shell's first prompt
        run.type_line(command);
        run.wait_program(program);
        run
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
        kill_holders_of(&self.home());
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[derive(Clone, Copy)]
enum ClaudePermissions {
    /// A rule makes the Bash tool ask, whatever the permission mode (the default mode of
    /// 2.1.316 is "auto", which lets a tool run without a dialog).
    Ask,
    /// The Bash tool runs without a dialog.
    AllowBash,
}

impl ClaudePermissions {
    fn settings(self) -> String {
        match self {
            Self::Ask => r#"{"permissions":{"ask":["Bash"]}}"#.into(),
            Self::AllowBash => r#"{"permissions":{"allow":["Bash"]}}"#.into(),
        }
    }
}

/// A dummy key for the fake API; Claude wants one that looks like a key.
const FAKE_KEY: &str = "sk-ant-api03-fake-key-for-local-live-test-0000000000";

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// Kills every process whose environment names `home` as its HOME: a CLI's background
/// helper (a daemon it left behind) belongs to this run alone. Linux only, where `/proc`
/// shows environments; elsewhere the helpers are left to the CLI.
fn kill_holders_of(home: &Path) {
    let needle = format!("HOME={}", home.display());
    for _ in 0..2 {
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return;
        };
        for entry in entries.flatten() {
            let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<i32>().ok()) else {
                continue;
            };
            let Ok(environ) = std::fs::read(entry.path().join("environ")) else {
                continue;
            };
            if environ.split(|b| *b == 0).any(|var| var == needle.as_bytes()) {
                unsafe { libc::kill(pid, libc::SIGKILL) };
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

// ---------------------------------------------------------------------------------------
// One session and what it said
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum What {
    /// `idle app=claude-code`, `blocked kind=permission app=codex`, or `clear`.
    Status(String),
    /// `program claude`, or `program none` when the shell has the terminal back.
    Program(String),
    Exited(String),
}

#[derive(Clone, Debug)]
struct Seen {
    at: Duration,
    what: What,
}

impl Seen {
    fn status(&self) -> Option<String> {
        match &self.what {
            What::Status(s) => Some(s.clone()),
            _ => None,
        }
    }

    fn program(&self) -> Option<String> {
        match &self.what {
            What::Program(p) => Some(p.clone()),
            _ => None,
        }
    }
}

struct Run {
    session: Session,
    received: mpsc::Receiver<Event>,
    began: Instant,
    seen: Vec<Seen>,
    /// Where the latest `mark()` was: `wait_status` looks from there.
    last_mark: usize,
}

impl Run {
    /// Takes in what the session sent so far.
    fn pump(&mut self) {
        while let Ok(event) = self.received.try_recv() {
            let what = match event {
                Event::Status(Some(record), _) => {
                    let mut text = format!("{:?}", record.state).to_lowercase();
                    if let Some(kind) = &record.kind {
                        text.push_str(&format!(" kind={kind}"));
                    }
                    if let Some(app) = &record.app {
                        text.push_str(&format!(" app={app}"));
                    }
                    What::Status(text)
                }
                Event::Status(None, _) => What::Status("clear".into()),
                Event::Program(Some(program)) => What::Program(format!("program {}", program.name)),
                Event::Program(None) => What::Program("program none".into()),
                Event::Exited(exit) => What::Exited(format!("{exit:?}")),
                Event::Title(_) | Event::Foreground(_) => continue,
            };
            self.seen.push(Seen { at: self.began.elapsed(), what });
        }
    }

    fn settle(&mut self, wait: Duration) {
        let until = Instant::now() + wait;
        while Instant::now() < until {
            self.pump();
            std::thread::sleep(Duration::from_millis(20));
        }
        self.pump();
    }

    /// A position in the sequence, for "since here".
    fn mark(&mut self) -> usize {
        self.pump();
        self.last_mark = self.seen.len();
        self.last_mark
    }

    fn type_line(&mut self, text: &str) {
        // Typed text, a pause, then Enter: a TUI that sees both at once takes it as a paste.
        self.session.input(text.as_bytes().to_vec()).expect("type");
        self.settle(Duration::from_millis(700));
        self.session.input(b"\r".to_vec()).expect("enter");
    }

    fn prompt(&mut self, text: &str) {
        self.type_line(text);
    }

    fn key(&mut self, bytes: &str) {
        self.session.input(bytes.as_bytes().to_vec()).expect("key");
    }

    fn wait_program(&mut self, name: &str) {
        self.wait_for(&format!("program {name}"), Duration::from_secs(30), 0, |seen| {
            seen.program().is_some_and(|p| p == format!("program {name}"))
        });
    }

    /// Waits for a status report no earlier than the start of the run that `pred` accepts.
    fn wait_status(&mut self, what: &str, timeout: Duration, pred: impl Fn(&str) -> bool) {
        let from = self.last_mark;
        self.wait_for(what, timeout, from, |seen| seen.status().is_some_and(|s| pred(&s)));
    }

    fn wait_for(&mut self, what: &str, timeout: Duration, from: usize, pred: impl Fn(&Seen) -> bool) -> usize {
        let deadline = Instant::now() + timeout;
        loop {
            self.pump();
            if let Some(index) = self.seen.iter().skip(from).position(&pred) {
                return from + index;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}\n{}", self.report());
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn entries_since(&self, mark: usize) -> &[Seen] {
        &self.seen[mark..]
    }

    fn last_status_since(&self, mark: usize) -> Option<String> {
        self.entries_since(mark).iter().rev().find_map(Seen::status)
    }

    /// The statuses since `mark`, one per change, must contain `expected` in order.
    fn expect_sequence_since(&mut self, mark: usize, expected: &[&str]) {
        self.pump();
        let mut statuses: Vec<String> = Vec::new();
        for status in self.entries_since(mark).iter().filter_map(Seen::status) {
            if statuses.last() != Some(&status) {
                statuses.push(status);
            }
        }
        let mut wanted = expected.iter();
        let mut next = wanted.next();
        for status in &statuses {
            if next.is_some_and(|w| w == status) {
                next = wanted.next();
            }
        }
        assert!(next.is_none(), "expected {expected:?} in order, saw {statuses:?}\n{}", self.report());
    }

    /// The whole sequence and the tail of the screen, for a failure message.
    fn report(&self) -> String {
        let mut out = String::from("sequence:\n");
        for seen in &self.seen {
            out.push_str(&format!("  {:>7.2}s {:?}\n", seen.at.as_secs_f32(), seen.what));
        }
        let screen = String::from_utf8_lossy(&self.session.read(Format::Text, None)).into_owned();
        let tail: Vec<&str> = screen.lines().filter(|l| !l.trim().is_empty()).collect();
        out.push_str("screen tail:\n");
        for line in &tail[tail.len().saturating_sub(15)..] {
            out.push_str(&format!("  | {line}\n"));
        }
        out
    }

    /// Prints the sequence the run saw, then ends the session.
    fn finish(&mut self) {
        self.pump();
        println!("{}", self.report());
        self.session.close();
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        self.session.close();
    }
}
