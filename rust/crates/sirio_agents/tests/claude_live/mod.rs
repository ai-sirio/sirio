//! Helpers for the live Claude Code tests in `acp_conformance.rs` (MCP elicitation and session
//! state): find the real binary, give it a private HOME and a fake model API, and drive it over stream-json.
//!
//! Test crates cannot share code, so `Resolve::Mise`, `refuse_wrapper`, `FAKE_KEY` and the
//! config written by `Session::start` are a minimal copy of
//! `sirio_term_session/tests/program_status_live.rs` (`Resolve`, `refuse_wrapper`,
//! `claude_config`). Keep them in step by hand.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::Value;

/// A dummy key for the fake API; Claude wants one that looks like a key.
const FAKE_KEY: &str = "sk-ant-api03-fake-key-for-local-live-test-0000000000";

const FAKE_API: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/live/fake_elicit_api.py");
const ELICIT_MCP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/live/elicit_mcp.py");
const STATE_API: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/live/fake_state_api.py");

/// The real binary of the newest installed mise `claude`, never the `latest` link or the
/// wrapper in `~/.local/bin` (running that under a private HOME reinstalls claude there).
pub fn resolve_claude(home: &Path) -> Option<PathBuf> {
    let installs = home.join(".local/share/mise/installs/claude");
    let mut versions: Vec<(Vec<u64>, PathBuf)> = std::fs::read_dir(installs)
        .ok()?
        .flatten()
        .filter(|entry| {
            entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink())
        })
        .filter_map(|entry| {
            let numbers: Option<Vec<u64>> = entry
                .file_name()
                .to_str()?
                .split('.')
                .map(|part| part.parse().ok())
                .collect();
            Some((numbers?, entry.path().join("claude")))
        })
        .filter(|(_, path)| path.is_file())
        .collect();
    versions.sort();
    versions.pop().map(|(_, path)| path)
}

/// Panics before anything runs when `path` is a wrapper that would edit the user's mise
/// config (a script that mentions `mise`), or lives in `~/.local/bin`.
pub fn refuse_wrapper(path: &Path, real_home: &Path) {
    use std::io::Read;
    let mut head = Vec::new();
    if let Ok(file) = std::fs::File::open(path) {
        let _ = file.take(4096).read_to_end(&mut head);
    }
    let script = head.starts_with(b"#!");
    let mentions_mise = head.windows(4).any(|window| window == b"mise");
    let local_bin = real_home.join(".local/bin");
    let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let wrapper =
        (script && mentions_mise) || path.starts_with(&local_bin) || target.starts_with(&local_bin);
    assert!(!wrapper, "refusing a mise wrapper: {}", path.display());
}

/// The PATH of every process here: nothing of the user's, so no mise shim is ever found.
const ISOLATED_PATH: &str = "/usr/bin:/bin";

/// The python3 the helpers run with: the one on `ISOLATED_PATH`, by absolute path.
pub fn python3() -> Option<PathBuf> {
    ISOLATED_PATH
        .split(':')
        .map(|dir| Path::new(dir).join("python3"))
        .find(|path| path.is_file())
}

/// A scratch directory under the system temp dir, outside `$HOME` and outside any git
/// repository (the CLI would otherwise run git against the user's live worktree). Removed on
/// drop, except while panicking, when it is kept and its path printed.
pub struct Scratch {
    pub root: PathBuf,
}

impl Scratch {
    pub fn new(name: &str, real_home: &Path) -> Self {
        let base = std::env::temp_dir()
            .canonicalize()
            .expect("a TMPDIR that exists");
        assert!(
            !base.starts_with(real_home),
            "set TMPDIR to an on-disk directory outside your home, e.g. /var/tmp/sirio-live \
             (TMPDIR is {}, inside {})",
            base.display(),
            real_home.display()
        );
        assert!(
            !base.ancestors().any(|dir| dir.join(".git").exists()),
            "TMPDIR {} is inside a git repository",
            base.display()
        );
        let root = base.join(format!("sirio-live-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "the scratch root is kept for inspection: {}",
                self.root.display()
            );
        } else {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

/// `claude --version` in the isolated environment, within 30 s.
pub fn version_text(claude: &Path, root: &Path) -> String {
    let home = root.join("version-home");
    std::fs::create_dir_all(&home).unwrap();
    let mut command = Command::new(claude);
    command.arg("--version");
    isolate(&mut command, &home, root);
    let out = root.join("version.out");
    let file = std::fs::File::create(&out).unwrap();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(file)
        .stderr(Stdio::null())
        .spawn()
        .expect("claude --version runs");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match child.try_wait().expect("claude --version is waited on") {
            Some(_) => break,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("claude --version did not finish in 30 s");
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    kill_holders_of(&home);
    std::fs::read_to_string(out)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// The environment of a CLI: private HOME, nothing of the user's, and a dead proxy so nothing
/// reaches past 127.0.0.1 (copied from `Rig::offline` in `program_status_live.rs`).
fn isolate(command: &mut Command, home: &Path, root: &Path) {
    let tmp = root.join("tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    command
        .env_clear()
        .env("PATH", ISOLATED_PATH)
        .env("HOME", home)
        .env("CLAUDE_CONFIG_DIR", home.join(".claude"))
        // The CLI's `/tmp/cc-socks/*.sock` land here, not in the user's temp dir.
        .env("TMPDIR", &tmp)
        .env("LANG", "C.UTF-8")
        .env("TERM", "dumb")
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("NO_PROXY", "127.0.0.1,localhost");
}

/// Kills every process whose environment names `home` as its HOME: a CLI's background
/// helper (a daemon it left behind) belongs to this run alone. Linux only, where `/proc`
/// shows environments. Copied from `program_status_live.rs`, with `kill(1)` for `libc::kill`.
pub fn kill_holders_of(home: &Path) {
    let needle = format!("HOME={}", home.display());
    for _ in 0..2 {
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return;
        };
        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };
            let Ok(environ) = std::fs::read(entry.path().join("environ")) else {
                continue;
            };
            if environ
                .split(|b| *b == 0)
                .any(|var| var == needle.as_bytes())
            {
                let _ = Command::new("/usr/bin/kill")
                    .args(["-9", &pid.to_string()])
                    .status();
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A child process that is killed and waited on when dropped, from the moment it exists.
struct Reaper(Child);

impl Drop for Reaper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Kills the helpers a CLI left under `home` once the CLI itself is gone.
struct HomeSweep(PathBuf);

impl Drop for HomeSweep {
    fn drop(&mut self) {
        kill_holders_of(&self.0);
    }
}

/// One CLI process with its fake model API. Everything is killed on every exit path.
pub struct Session {
    // Field order is drop order: the CLI, then its API, then whatever it left behind.
    _child: Reaper,
    _api: Reaper,
    _sweep: HomeSweep,
    stdin: ChildStdin,
    lines: Receiver<String>,
}

impl Session {
    /// Launches `claude` for a new chat session under `root` (a private HOME and work
    /// directory) with the eliciting MCP server and the fake model API.
    pub fn start(claude: &Path, python: &Path, root: &Path) -> Self {
        Self::launch(claude, python, root, FAKE_API, true)
    }

    /// Launches `claude` for a new chat session under `root` against the fake API of the
    /// session-state test (`fake_state_api.py`), with no MCP server.
    pub fn start_state(claude: &Path, python: &Path, root: &Path) -> Self {
        Self::launch(claude, python, root, STATE_API, false)
    }

    fn launch(claude: &Path, python: &Path, root: &Path, api_script: &str, with_mcp: bool) -> Self {
        let _ = std::fs::remove_dir_all(root);
        let home = root.join("home");
        let work = root.join("work");
        let config_dir = home.join(".claude");
        for dir in [&home, &work, &config_dir] {
            std::fs::create_dir_all(dir).unwrap();
        }

        let port = free_port();
        let api_log = std::fs::File::create(root.join("api.log")).unwrap();
        let mut api_command = Command::new(python);
        api_command
            .args(["-I", api_script, &port.to_string()])
            .env_clear()
            .env("PATH", ISOLATED_PATH)
            .stdin(Stdio::null())
            .stdout(api_log.try_clone().unwrap())
            .stderr(api_log);
        let api = Reaper(api_command.spawn().expect("start the fake model API"));
        let deadline = Instant::now() + Duration::from_secs(10);
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(
                Instant::now() < deadline,
                "the fake model API did not come up"
            );
            std::thread::sleep(Duration::from_millis(50));
        }

        // Claude remembers an approved key by its last 20 characters.
        let key_tail = &FAKE_KEY[FAKE_KEY.len() - 20..];
        let work_text = work.display().to_string();
        let global = format!(
            r#"{{"hasCompletedOnboarding":true,"theme":"dark","hasSeenAutoDefaultNotice":true,"hasResetAutoModeOptInForDefaultOffer":true,"customApiKeyResponses":{{"approved":["{key_tail}"],"rejected":[]}},
"projects":{{{work_text:?}:{{"hasTrustDialogAccepted":true,"hasCompletedProjectOnboarding":true}}}}}}"#
        );
        std::fs::write(home.join(".claude.json"), &global).unwrap();
        std::fs::write(config_dir.join(".claude.json"), &global).unwrap();
        std::fs::write(config_dir.join("settings.json"), "{}").unwrap();

        let mcp_log = root.join("mcp.log");
        let mcp = serde_json::json!({"mcpServers": {"elic": {
            "type": "stdio", "command": python.display().to_string(),
            "args": ["-I", ELICIT_MCP, mcp_log.display().to_string()]}}});
        let session_id = format!("00000000-0000-4000-8000-{:012x}", u64::from(port));
        let line = sirio_claude::LaunchLine::chat(sirio_claude::ChatSession::New(&session_id));
        let mut command = Command::new(claude);
        command.args(&line.args).current_dir(&work);
        if with_mcp {
            command.args(["--mcp-config", &mcp.to_string(), "--strict-mcp-config"]);
        }
        isolate(&mut command, &home, root);
        command
            .env("ANTHROPIC_API_KEY", FAKE_KEY)
            .env("ANTHROPIC_BASE_URL", format!("http://127.0.0.1:{port}"))
            .env("DISABLE_AUTOUPDATER", "1")
            .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
            // Claude reads CLAUDE.md files from every ancestor of its directory.
            .env("CLAUDE_CODE_DISABLE_CLAUDE_MDS", "1")
            .env("ENABLE_TOOL_SEARCH", "false")
            .envs(line.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(std::fs::File::create(root.join("stderr.log")).unwrap());
        let sweep = HomeSweep(home);
        let mut child = Reaper(command.spawn().expect("claude launches"));
        let stdin = child.0.stdin.take().expect("a piped stdin");
        let stdout = child.0.stdout.take().expect("a piped stdout");
        let (tx, lines) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        Self {
            _child: child,
            _api: api,
            _sweep: sweep,
            stdin,
            lines,
        }
    }

    pub fn write(&mut self, line: &Value) {
        writeln!(self.stdin, "{line}").expect("a line is written to claude");
        self.stdin.flush().expect("stdin is flushed");
    }

    /// The next stdout line that parses as JSON, or `None` at `deadline` or when the CLI exits.
    pub fn next(&mut self, deadline: Instant) -> Option<Value> {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let line = self.lines.recv_timeout(remaining).ok()?;
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                return Some(value);
            }
        }
    }

    /// Like `next`, but says why there is no line: the deadline passed (`Timeout`) or the
    /// CLI's stdout closed (`Closed`).
    pub fn poll(&mut self, deadline: Instant) -> Polled {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(remaining) {
                Ok(line) => {
                    if let Ok(value) = serde_json::from_str::<Value>(&line) {
                        return Polled::Line(value);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => return Polled::Timeout,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Polled::Closed,
            }
        }
    }
}

/// What `Session::poll` found.
pub enum Polled {
    Line(Value),
    Timeout,
    Closed,
}
