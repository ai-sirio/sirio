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
const TASKS_API: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/live/fake_tasks_api.py");
const LIMITS_API: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/live/fake_limits_api.py");

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

/// The git the helpers run with: the one on `ISOLATED_PATH`, by absolute path.
fn git() -> Option<PathBuf> {
    ISOLATED_PATH
        .split(':')
        .map(|dir| Path::new(dir).join("git"))
        .find(|path| path.is_file())
}

/// Whether `git` is available, for tests that give the work directory project settings.
pub fn git_available() -> bool {
    git().is_some()
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

/// The `CLAUDE_CONFIG_DIR` of a CLI whose private HOME is `home`.
fn config_dir_of(home: &Path) -> PathBuf {
    home.join(".claude")
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
        .env("CLAUDE_CONFIG_DIR", config_dir_of(home))
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
    for _ in 0..2 {
        for pid in holder_pids(home) {
            let _ = Command::new("/usr/bin/kill")
                .args(["-9", &pid.to_string()])
                .status();
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// How many processes name `home` as their HOME, without killing them. It
/// reads /proc, so it is Linux only, like `kill_holders_of`.
pub fn holders_of(home: &Path) -> usize {
    holder_pids(home).len()
}

fn holder_pids(home: &Path) -> Vec<u32> {
    let needle = format!("HOME={}", home.display());
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let pid = entry.file_name().to_str()?.parse::<u32>().ok()?;
            let environ = std::fs::read(entry.path().join("environ")).ok()?;
            environ
                .split(|b| *b == 0)
                .any(|var| var == needle.as_bytes())
                .then_some(pid)
        })
        .collect()
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
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    /// What a second `claude` command needs to run beside this one.
    cli: CliEnv,
}

/// The environment a `claude` command of this session runs in, kept for `Session::run_claude`.
struct CliEnv {
    claude: PathBuf,
    root: PathBuf,
    home: PathBuf,
    work: PathBuf,
    port: u16,
    session_id: String,
    launch_env: Vec<(String, String)>,
    /// A gateway token, applied to the CLI's command alone as `ANTHROPIC_AUTH_TOKEN`.
    auth_token: Option<String>,
}

impl CliEnv {
    /// The isolated environment plus the fake API's base URL: the one `launch` gives its CLI.
    fn apply(&self, command: &mut Command) {
        isolate(command, &self.home, &self.root);
        command
            .env("ANTHROPIC_API_KEY", FAKE_KEY)
            .env(
                "ANTHROPIC_BASE_URL",
                format!("http://127.0.0.1:{}", self.port),
            )
            .env("DISABLE_AUTOUPDATER", "1")
            .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
            // Claude reads CLAUDE.md files from every ancestor of its directory.
            .env("CLAUDE_CODE_DISABLE_CLAUDE_MDS", "1")
            .env("ENABLE_TOOL_SEARCH", "false")
            .envs(
                self.launch_env
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str())),
            );
        if let Some(token) = &self.auth_token {
            command.env("ANTHROPIC_AUTH_TOKEN", token);
            // Every command that carries the token passes the guard, `run_claude` included.
            assert_local_base_url(command);
        }
    }
}

/// Whether `url` is the fake API on this machine: `http://127.0.0.1:`, a port, then the end
/// or a path. A prefix check alone would let `http://127.0.0.1:1@example.com/` through.
pub fn is_local_base_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://127.0.0.1:") else {
        return false;
    };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && matches!(rest.as_bytes().get(digits), None | Some(b'/'))
}

/// Panics unless the finished `command`'s `ANTHROPIC_BASE_URL` is a local one. Read from the
/// command's own environment, so no later `env` call can point it elsewhere. The message
/// names neither the URL nor anything else of the environment.
fn assert_local_base_url(command: &Command) {
    let local = command
        .get_envs()
        .find(|(name, _)| *name == "ANTHROPIC_BASE_URL")
        .and_then(|(_, value)| value)
        .is_some_and(|url| is_local_base_url(&url.to_string_lossy()));
    assert!(
        local,
        "the CLI's ANTHROPIC_BASE_URL is a 127.0.0.1 address: false"
    );
}

/// What a finished `Session::run_claude` printed.
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Session {
    /// Launches `claude` for a new chat session under `root` (a private HOME and work
    /// directory) with the eliciting MCP server and the fake model API.
    pub fn start(claude: &Path, python: &Path, root: &Path) -> Self {
        Self::launch(claude, python, root, FAKE_API, true, None, None, false, true)
    }

    /// Launches `claude` for a new chat session under `root` against the fake API of the
    /// session-state test (`fake_state_api.py`), with no MCP server.
    pub fn start_state(claude: &Path, python: &Path, root: &Path) -> Self {
        Self::launch(claude, python, root, STATE_API, false, None, None, false, true)
    }

    /// Launches `claude` for a new chat session under `root` against the fake API of the
    /// background-task test (`fake_tasks_api.py`), with no MCP server.
    pub fn start_tasks(claude: &Path, python: &Path, root: &Path) -> Self {
        Self::launch(claude, python, root, TASKS_API, false, None, None, false, true)
    }

    /// Launches `claude` against the rate-limit fake API (`fake_limits_api.py`), which answers
    /// with the status and headers `limits` holds (`{"status": .., "headers": {..}}`, re-read
    /// on every request) and logs method and path of each request to `limits` + `.requests`.
    ///
    /// `auth_token`, when given, is passed to the CLI's command alone as `ANTHROPIC_AUTH_TOKEN`
    /// (the gateway auth under which the CLI emits `rate_limit_event`). It is never printed or
    /// written. Once every variable is applied and before the CLI starts, the command's
    /// `ANTHROPIC_BASE_URL` must start with `http://127.0.0.1:`, or this panics.
    pub fn start_limits(
        claude: &Path,
        python: &Path,
        root: &Path,
        limits: &Path,
        auth_token: Option<&str>,
    ) -> Self {
        Self::launch_broken(
            claude,
            python,
            root,
            LIMITS_API,
            false,
            None,
            None,
            false,
            true,
            None,
            Some((limits, auth_token)),
        )
    }

    /// Launches `claude` against the session-state fake API with `user` as the config dir's
    /// `settings.json` and, when given, `project` as `work/.claude/settings.json` (after a
    /// `git init` in `work/`, so the CLI finds that repository's settings and no other).
    /// The caller checks `git_available` first.
    pub fn start_with_settings(
        claude: &Path,
        python: &Path,
        root: &Path,
        user: &Value,
        project: Option<&Value>,
    ) -> Self {
        Self::launch(
            claude,
            python,
            root,
            STATE_API,
            false,
            Some(user),
            project,
            false,
            true,
        )
    }

    /// Launches `claude` against the session-state fake API with the eliciting MCP server
    /// registered as the user-scope stdio server `probe` in the config dir's `.claude.json`,
    /// the way Sirio launches (no `--mcp-config`). With `fresh: false` the root, home, config
    /// dir and work dir are reused as the last session left them: nothing is wiped or written.
    /// The second session gets another session id (group `8001`).
    pub fn start_with_user_mcp(claude: &Path, python: &Path, root: &Path, fresh: bool) -> Self {
        Self::launch(claude, python, root, STATE_API, false, None, None, true, fresh)
    }

    /// Launches `claude` the way `start_state` does, but set up so that it cannot start. It
    /// never gets a prompt from here. See `BrokenStart`.
    pub fn start_broken(claude: &Path, python: &Path, root: &Path, broken: BrokenStart) -> Self {
        Self::launch_broken(
            claude,
            python,
            root,
            STATE_API,
            false,
            None,
            None,
            false,
            true,
            Some(broken),
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn launch(
        claude: &Path,
        python: &Path,
        root: &Path,
        api_script: &str,
        with_mcp: bool,
        user_settings: Option<&Value>,
        project_settings: Option<&Value>,
        user_mcp: bool,
        fresh: bool,
    ) -> Self {
        Self::launch_broken(
            claude,
            python,
            root,
            api_script,
            with_mcp,
            user_settings,
            project_settings,
            user_mcp,
            fresh,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn launch_broken(
        claude: &Path,
        python: &Path,
        root: &Path,
        api_script: &str,
        with_mcp: bool,
        user_settings: Option<&Value>,
        project_settings: Option<&Value>,
        user_mcp: bool,
        fresh: bool,
        broken: Option<BrokenStart>,
        limits: Option<(&Path, Option<&str>)>,
    ) -> Self {
        if fresh {
            let _ = std::fs::remove_dir_all(root);
        }
        let home = root.join("home");
        let work = root.join("work");
        let config_dir = config_dir_of(&home);
        // Each session group keeps its own logs, so a reused root does not lose the first one.
        let group = if fresh { "8000" } else { "8001" };
        if !fresh {
            assert!(
                config_dir.join(".claude.json").is_file(),
                "a reused root has the config of a previous session"
            );
        }
        for dir in [&home, &work, &config_dir] {
            std::fs::create_dir_all(dir).unwrap();
        }

        let port = free_port();
        let api_log = std::fs::File::create(root.join(format!("api-{group}.log"))).unwrap();
        let mut api_command = Command::new(python);
        api_command
            .args(["-I", api_script, &port.to_string()])
            .args(limits.map(|(file, _)| file))
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

        let mcp_log = root.join("mcp.log");
        if fresh {
            // Claude remembers an approved key by its last 20 characters.
            let key_tail = &FAKE_KEY[FAKE_KEY.len() - 20..];
            let work_text = work.display().to_string();
            let global = format!(
                r#"{{"hasCompletedOnboarding":true,"theme":"dark","hasSeenAutoDefaultNotice":true,"hasResetAutoModeOptInForDefaultOffer":true,"customApiKeyResponses":{{"approved":["{key_tail}"],"rejected":[]}},
"projects":{{{work_text:?}:{{"hasTrustDialogAccepted":true,"hasCompletedProjectOnboarding":true}}}}}}"#
            );
            std::fs::write(home.join(".claude.json"), &global).unwrap();
            let config_json = if user_mcp {
                let mut config: Value = serde_json::from_str(&global).unwrap();
                config["mcpServers"] = serde_json::json!({"probe": {
                    "type": "stdio", "command": python.display().to_string(),
                    "args": ["-I", ELICIT_MCP, mcp_log.display().to_string()]}});
                config.to_string()
            } else {
                global.clone()
            };
            std::fs::write(config_dir.join(".claude.json"), config_json).unwrap();
            let user_text = user_settings.map_or_else(|| "{}".to_string(), Value::to_string);
            std::fs::write(config_dir.join("settings.json"), user_text).unwrap();
            if let Some(project) = project_settings {
                let init = Command::new(git().expect("the caller checked git_available"))
                    .args(["init", "-q"])
                    .current_dir(&work)
                    .env_clear()
                    .env("PATH", ISOLATED_PATH)
                    .env("GIT_CONFIG_NOSYSTEM", "1")
                    .env("HOME", &home)
                    .status()
                    .expect("git init runs");
                assert!(init.success(), "git init failed in the work directory");
                std::fs::create_dir_all(work.join(".claude")).unwrap();
                std::fs::write(work.join(".claude/settings.json"), project.to_string()).unwrap();
            }
            if matches!(broken, Some(BrokenStart::CorruptConfig)) {
                // The only `.claude.json` write of these tests: in the scratch config dir.
                std::fs::write(config_dir.join(".claude.json"), "{ \"broken\": ").unwrap();
            }
        }
        let mcp = serde_json::json!({"mcpServers": {"elic": {
            "type": "stdio", "command": python.display().to_string(),
            "args": ["-I", ELICIT_MCP, mcp_log.display().to_string()]}}});
        let session_id = format!("00000000-0000-4000-{group}-{:012x}", u64::from(port));
        let line = sirio_claude::LaunchLine::chat(sirio_claude::ChatSession::New(&session_id));
        let mut command = Command::new(claude);
        command.args(&line.args).current_dir(&work);
        if with_mcp {
            command.args(["--mcp-config", &mcp.to_string(), "--strict-mcp-config"]);
        }
        let mut launch_env = line.env.clone();
        if let Some(BrokenStart::BadProxy { opt_in }) = broken {
            if !opt_in {
                launch_env.retain(|(name, _)| name != "CLAUDE_CODE_STARTUP_FAILURE_RESULTS");
            }
            // Applied after `isolate`, so it replaces the dead proxy for HTTPS only.
            launch_env.push(("HTTPS_PROXY".to_string(), "not a url".to_string()));
        }
        let cli = CliEnv {
            claude: claude.to_path_buf(),
            root: root.to_path_buf(),
            home: home.clone(),
            work: work.clone(),
            port,
            session_id: session_id.clone(),
            launch_env,
            auth_token: limits.and_then(|(_, token)| token).map(str::to_string),
        };
        cli.apply(&mut command);
        if limits.is_some() {
            assert_local_base_url(&command);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(std::fs::File::create(root.join(format!("stderr-{group}.log"))).unwrap());
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
            stdin: Some(stdin),
            lines,
            cli,
        }
    }

    /// The session uuid this CLI was started with (`--session-id`).
    pub fn session_id(&self) -> &str {
        &self.cli.session_id
    }

    /// The CLI's private HOME.
    pub fn home(&self) -> PathBuf {
        self.cli.home.clone()
    }

    /// The CLI's `CLAUDE_CONFIG_DIR`.
    pub fn config_dir(&self) -> PathBuf {
        config_dir_of(&self.cli.home)
    }

    /// Closes the CLI's stdin and waits up to 20 s for it to exit, leaving the fake API up.
    /// Returns its exit status, or `None` when it had to be killed after the 20 s.
    pub fn close_cli(&mut self) -> Option<std::process::ExitStatus> {
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self._child.0.try_wait().ok().flatten() {
                return Some(status);
            }
            if Instant::now() >= deadline {
                let _ = self._child.0.kill();
                let _ = self._child.0.wait();
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Runs the same `claude` binary as a separate command with `args`, in this session's
    /// environment (HOME, config dir, fake API) and work directory, stdin from /dev/null.
    /// A command still running after `timeout` is killed and the test panics. Its output goes to
    /// `side-<label>.out` / `.err` under the session root, so each call keeps its own.
    pub fn run_claude(&self, label: &str, args: &[&str], timeout: Duration) -> CommandOutput {
        let out_path = self.cli.root.join(format!("side-{label}.out"));
        let err_path = self.cli.root.join(format!("side-{label}.err"));
        let mut command = Command::new(&self.cli.claude);
        command.args(args).current_dir(&self.cli.work);
        self.cli.apply(&mut command);
        command
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(&out_path).unwrap())
            .stderr(std::fs::File::create(&err_path).unwrap());
        let mut child = Reaper(command.spawn().expect("claude launches"));
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.0.try_wait().expect("the command is waited on") {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "`claude {args:?}` did not finish in {timeout:?}"
            );
            std::thread::sleep(Duration::from_millis(50));
        };
        CommandOutput {
            success: status.success(),
            stdout: std::fs::read_to_string(out_path).unwrap_or_default(),
            stderr: std::fs::read_to_string(err_path).unwrap_or_default(),
        }
    }

    /// Like `write`, but `false` on a broken pipe (the CLI already exited) instead of a panic.
    pub fn try_write(&mut self, line: &Value) -> bool {
        let Some(stdin) = self.stdin.as_mut() else {
            return false;
        };
        writeln!(stdin, "{line}").is_ok() && stdin.flush().is_ok()
    }

    pub fn write(&mut self, line: &Value) {
        let stdin = self.stdin.as_mut().expect("the CLI's stdin is still open");
        writeln!(stdin, "{line}").expect("a line is written to claude");
        stdin.flush().expect("stdin is flushed");
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

/// How `Session::start_broken` makes the CLI unable to start.
#[derive(Clone, Copy)]
pub enum BrokenStart {
    /// The config dir's `.claude.json` is cut off in the middle of an object.
    CorruptConfig,
    /// `HTTPS_PROXY` is not a URL. `opt_in: false` withholds
    /// `CLAUDE_CODE_STARTUP_FAILURE_RESULTS`.
    BadProxy { opt_in: bool },
}

/// What `Session::poll` found.
pub enum Polled {
    Line(Value),
    Timeout,
    Closed,
}
