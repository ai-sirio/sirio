//! The real OpenCode v2 service against the watch, with a local fake model API.
//!
//! One private service (private HOME, private XDG_*, a free port set with
//! `opencode service set port` BEFORE its first start, so the default port 49374, the
//! user's own service, is never contacted), one session in a private directory, one prompt
//! that makes the fake model ask for a `shell` tool. The `Watch` must report that session
//! Busy, then Waiting on the permission ask (answered through the REST path), then Idle or
//! Error. A rename of an event in a future OpenCode turns this test red.
//!
//! The test prints `SKIP:` and passes when `opencode` (v2) or `python3` is missing. The
//! binary is `~/.opencode/bin/opencode`, or the path in `SIRIO_LIVE_OPENCODE`; it is run
//! through a private `bin/` of symlinks on PATH. Nothing of the user's `~/.config/opencode`,
//! `~/.local/state/opencode` or home is read or written. At the end the private service is
//! stopped by the pid in its own registration file, and the test checks that nothing
//! listens on the private port any more.
//!
//! It spends no model credit (the model is a local script) but runs the real CLI, so it is
//! `#[ignore]`d and runs only on request:
//!
//!     cargo nextest run -p sirio_opencode_watch --test service_live --run-ignored only
//!
//! The scratch root is under `std::env::temp_dir()`, which must be on disk and OUTSIDE your
//! home: the test panics otherwise, before the CLI starts. A failed test keeps its scratch root
//! and prints its path. A binary that is a script mentioning `mise`, or in `~/.local/bin`, is
//! refused.

#![cfg(unix)]

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sirio_opencode_watch::{Endpoint, OcState, Watch, WatchEvent, discover};

/// OpenCode's default service port: the user's real service. Never used, never contacted.
const REAL_SERVICE_PORT: u16 = 49374;
const STEP_TIMEOUT: Duration = Duration::from_secs(60);

const FAKE_CHAT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sirio_term_session/tests/live/fake_chat.py"
);

#[test]
#[ignore = "live: runs the real CLI against a local fake model API; run with --run-ignored only"]
fn service_events_match_task_2() {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        println!("SKIP: HOME unset");
        return;
    };
    let real_home = home.canonicalize().unwrap_or_else(|_| home.clone());
    let base = scratch_base(&real_home);
    let Some(opencode) = find_cli(&home) else {
        println!("SKIP: opencode not found (set SIRIO_LIVE_OPENCODE to its path)");
        return;
    };
    refuse_wrapper(&opencode, &real_home);
    if python().arg("--version").output().is_err() {
        println!("SKIP: python3 not found");
        return;
    }
    println!("opencode: {}", opencode.display());
    let mut live = Live::new(&base, &opencode);
    match live.version_is_v2() {
        Some(true) => {}
        Some(false) => {
            println!("SKIP: {} is not OpenCode v2", opencode.display());
            return;
        }
        None => panic!("`opencode --version` crashed or exited non-zero: {}", opencode.display()),
    }
    live.configure();
    live.start_fake_model();
    live.start_service();

    // The watch finds the private service the way the app does: its registration file.
    let state = live.state_home();
    let home = live.home();
    let events: Arc<Mutex<Vec<WatchEvent>>> = Arc::default();
    let sink = events.clone();
    let watch = Watch::start(
        move || {
            discover(&home, &|name| {
                (name == "XDG_STATE_HOME").then(|| state.display().to_string())
            })
        },
        move |event| sink.lock().unwrap().push(event),
        vec![live.work()],
    );
    wait_for("the watch to be up", &events, |events| {
        events.iter().any(|event| matches!(event, WatchEvent::Sessions(_)))
    });

    let endpoint = live.endpoint();
    let session = live.create_session(&endpoint);
    live.post(
        &endpoint,
        &format!("/api/session/{session}/prompt"),
        r#"{"text":"please run it"}"#,
    );
    wait_for("the session to be Busy", &events, |events| {
        states_of(events, &session).contains(&OcState::Busy)
    });
    wait_for("the session to be Waiting", &events, |events| {
        states_of(events, &session).contains(&OcState::Waiting)
    });

    // Answer the ask: the id comes from the session's own permission list.
    let asked = live.get(&endpoint, &format!("/api/session/{session}/permission"));
    let request = asked["data"][0]["id"].as_str().expect("a permission request id").to_string();
    live.post(
        &endpoint,
        &format!("/api/session/{session}/permission/{request}/reply"),
        r#"{"decision":"once"}"#,
    );
    wait_for("the session to end its turn", &events, |events| {
        let states = states_of(events, &session);
        states.iter().rposition(|s| matches!(s, OcState::Idle | OcState::Error))
            > states.iter().position(|s| *s == OcState::Waiting)
    });

    watch.stop();
    let states = states_of(&events.lock().unwrap(), &session);
    println!("opencode session states seen by the watch: {states:?}");
    let busy = states.iter().position(|s| *s == OcState::Busy).expect("Busy");
    let waiting = states.iter().position(|s| *s == OcState::Waiting).expect("Waiting");
    let end = states
        .iter()
        .rposition(|s| matches!(s, OcState::Idle | OcState::Error))
        .expect("Idle or Error");
    assert!(busy < waiting && waiting < end, "order Busy < Waiting < end, saw {states:?}");
    // `permission.replied` takes the session back to Busy until the execution ends.
    assert!(
        states[waiting..end].contains(&OcState::Busy),
        "Busy again after the answer, saw {states:?}"
    );
    assert_eq!(
        states[end],
        OcState::Idle,
        "the approved shell call ends the turn normally, saw {states:?}"
    );
    live.finish();
}

/// The scratch parent: `TMPDIR`, which must be outside the user's home (agents read
/// instruction files from every ancestor of the directory they run in).
fn scratch_base(real_home: &Path) -> PathBuf {
    let base = std::env::temp_dir().canonicalize().expect("a TMPDIR that exists");
    assert!(
        !base.starts_with(real_home),
        "set TMPDIR to an on-disk directory outside your home, e.g. /var/tmp/sirio-live \
         (TMPDIR is {}, inside {})",
        base.display(),
        real_home.display()
    );
    base
}

/// Panics before anything runs when `path` is a script that mentions `mise` in its first
/// 4 KiB (a wrapper that would edit the user's mise config) or lives in `~/.local/bin`.
fn refuse_wrapper(path: &Path, real_home: &Path) {
    use std::io::Read;
    let mut head = Vec::new();
    if let Ok(file) = std::fs::File::open(path) {
        let _ = file.take(4096).read_to_end(&mut head);
    }
    let mentions_mise = head.windows(4).any(|window| window == b"mise");
    let local_bin = real_home.join(".local/bin");
    let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let wrapper = (head.starts_with(b"#!") && mentions_mise)
        || path.starts_with(&local_bin)
        || target.starts_with(&local_bin);
    assert!(!wrapper, "refusing a mise wrapper: {}", path.display());
}

/// python3 with nothing of the user's environment: the fake API needs none.
fn python() -> Command {
    let mut command = Command::new("python3");
    command.env_clear().env("PATH", "/usr/bin:/bin");
    command
}

/// `kill`, by absolute path, with an empty environment.
fn kill_command() -> Command {
    let path = ["/usr/bin/kill", "/bin/kill"].into_iter().find(|p| Path::new(p).exists()).unwrap_or("/bin/kill");
    let mut command = Command::new(path);
    command.env_clear().stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    command
}

/// The pids whose environment has exactly `HOME=<home>` (Linux, where `/proc` shows them).
/// `None` when `/proc` is not there.
fn processes_with_home(home: &Path) -> Option<Vec<u32>> {
    let needle = format!("HOME={}", home.display());
    let mut found = Vec::new();
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        if let Ok(environ) = std::fs::read(entry.path().join("environ"))
            && environ.split(|b| *b == 0).any(|var| var == needle.as_bytes())
        {
            found.push(pid);
        }
    }
    Some(found)
}

/// The states the watch reported for `session`, one per change, in order.
fn states_of(events: &[WatchEvent], session: &str) -> Vec<OcState> {
    let mut states: Vec<OcState> = Vec::new();
    for event in events {
        if let WatchEvent::Sessions(sessions) = event
            && let Some(row) = sessions.iter().find(|row| row.id == session)
            && states.last() != Some(&row.state)
        {
            states.push(row.state);
        }
    }
    states
}

fn wait_for(what: &str, events: &Arc<Mutex<Vec<WatchEvent>>>, done: impl Fn(&[WatchEvent]) -> bool) {
    let deadline = Instant::now() + STEP_TIMEOUT;
    loop {
        if done(&events.lock().unwrap()) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; the watch said {:?}",
            events.lock().unwrap()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn find_cli(home: &Path) -> Option<PathBuf> {
    let path = match std::env::var_os("SIRIO_LIVE_OPENCODE") {
        Some(path) => PathBuf::from(path),
        None => home.join(".opencode/bin/opencode"),
    };
    path.is_file().then_some(path)
}

/// Two different free ports: both listeners are held until both are chosen.
fn free_ports() -> (u16, u16) {
    loop {
        let first = TcpListener::bind("127.0.0.1:0").unwrap();
        let second = TcpListener::bind("127.0.0.1:0").unwrap();
        let (a, b) = (first.local_addr().unwrap().port(), second.local_addr().unwrap().port());
        if a != REAL_SERVICE_PORT && b != REAL_SERVICE_PORT {
            return (a, b);
        }
    }
}

fn listening(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

/// Everything the run owns: the scratch tree, the fake model, the private service. Dropping
/// it stops them, so a failed assertion leaves nothing running.
struct Live {
    root: PathBuf,
    port: u16,
    chat_port: u16,
    fake: Option<Child>,
    service_started: bool,
    agent: ureq::Agent,
}

impl Live {
    fn new(base: &Path, opencode: &Path) -> Self {
        let root = base.join(format!("sirio-live-oc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["home", "bin", "work", "tmp", "xdg/config", "xdg/data", "xdg/state", "xdg/cache", "xdg/runtime"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        // The path is the symlink's own: OpenCode may start itself again by it.
        std::os::unix::fs::symlink(opencode, root.join("bin/opencode")).unwrap();
        let root = root.canonicalize().unwrap();
        let agent = ureq::Agent::config_builder()
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(20)))
            .build()
            .new_agent();
        let (port, chat_port) = free_ports();
        Self {
            root,
            port,
            chat_port,
            fake: None,
            service_started: false,
            agent,
        }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }
    fn work(&self) -> PathBuf {
        self.root.join("work")
    }
    fn state_home(&self) -> PathBuf {
        self.root.join("xdg/state")
    }

    /// An `opencode` command in the private environment and nothing else: the user's
    /// variables never reach it.
    fn command(&self) -> Command {
        let mut command = Command::new(self.root.join("bin/opencode"));
        command
            .env_clear()
            .env("HOME", self.home())
            .env("XDG_CONFIG_HOME", self.root.join("xdg/config"))
            .env("XDG_DATA_HOME", self.root.join("xdg/data"))
            .env("XDG_STATE_HOME", self.state_home())
            .env("XDG_CACHE_HOME", self.root.join("xdg/cache"))
            .env("XDG_RUNTIME_DIR", self.root.join("xdg/runtime"))
            .env("PATH", format!("{}:/usr/bin:/bin", self.root.join("bin").display()))
            .env("LANG", "C.UTF-8")
            .env("TMPDIR", self.root.join("tmp"))
            // No self-update and no model catalogue fetch; a dead proxy keeps anything else
            // off the network (the fake model and the service itself are on 127.0.0.1).
            .env("OPENCODE_DISABLE_AUTOUPDATE", "1")
            .env("OPENCODE_DISABLE_MODELS_FETCH", "1")
            .env("HTTPS_PROXY", "http://127.0.0.1:9")
            .env("HTTP_PROXY", "http://127.0.0.1:9")
            .env("NO_PROXY", "127.0.0.1,localhost")
            .current_dir(self.work())
            .stdin(Stdio::null());
        command
    }

    /// `Some(true)` for a clean `--version` run naming 2.x, `Some(false)` for a clean run
    /// naming something else, `None` for a crash or a non-zero exit.
    fn version_is_v2(&self) -> Option<bool> {
        let output = self.command().arg("--version").output().ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        Some(text.contains(" v2.") || text.trim_start().starts_with("2."))
    }

    /// The provider config and, before the service ever starts, its private port.
    fn configure(&mut self) {
        let config = self.root.join("xdg/config/opencode");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(
            config.join("opencode.json"),
            format!(
                r#"{{"$schema":"https://opencode.ai/config.json",
"provider":{{"fake":{{"npm":"@ai-sdk/openai-compatible","name":"Fake","options":{{"baseURL":"http://127.0.0.1:{}/v1","apiKey":"x"}},"models":{{"m":{{"name":"m"}}}}}}}},
"model":"fake/m","small_model":"fake/m","autoupdate":false,"share":"disabled",
"permission":{{"shell":"ask","bash":"ask","edit":"ask"}}}}"#,
                self.chat_port
            ),
        )
        .unwrap();
        let set = self
            .command()
            .args(["service", "set", "port", &self.port.to_string()])
            .output()
            .expect("run opencode service set port");
        assert!(set.status.success(), "opencode service set port failed");
        // The service must not start before its private port is what it will read.
        let saved = std::fs::read_to_string(config.join("service.json")).unwrap_or_default();
        assert!(
            saved.contains(&self.port.to_string()),
            "the private service.json does not hold port {}; not starting a service",
            self.port
        );
    }

    fn start_fake_model(&mut self) {
        self.fake = Some(
            python()
                .args(["-I", FAKE_CHAT, &self.chat_port.to_string(), "2"])
                .current_dir(&self.root)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("start the fake model"),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while !listening(self.chat_port) {
            assert!(Instant::now() < deadline, "the fake model did not come up");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn start_service(&mut self) {
        self.service_started = true;
        let mut start = self
            .command()
            .args(["service", "start"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("run opencode service start");
        let deadline = Instant::now() + STEP_TIMEOUT;
        loop {
            match start.try_wait().unwrap() {
                Some(status) => {
                    assert!(status.success(), "opencode service start failed");
                    break;
                }
                None if Instant::now() > deadline => {
                    let _ = start.kill();
                    panic!("opencode service start did not return");
                }
                None => std::thread::sleep(Duration::from_millis(100)),
            }
        }
        // The registration must name the private port, or this is not our service.
        let endpoint = self.endpoint();
        assert!(
            endpoint.base_url.ends_with(&format!(":{}", self.port)),
            "the registration names {}, not the private port {}",
            endpoint.base_url,
            self.port
        );
    }

    fn endpoint(&self) -> Endpoint {
        let state = self.state_home();
        discover(&self.home(), &|name| {
            (name == "XDG_STATE_HOME").then(|| state.display().to_string())
        })
        .expect("the private service's registration")
    }

    fn authorization(endpoint: &Endpoint) -> String {
        let (user, password) = endpoint.auth.as_ref().expect("a password");
        format!("Basic {}", base64(&format!("{user}:{password}")))
    }

    fn create_session(&self, endpoint: &Endpoint) -> String {
        let body = format!(r#"{{"location":{{"directory":{:?}}}}}"#, self.work().display().to_string());
        let created = self.post(endpoint, "/api/session", &body);
        created["data"]["id"].as_str().expect("a session id").to_string()
    }

    fn post(&self, endpoint: &Endpoint, path: &str, body: &str) -> serde_json::Value {
        let mut response = self
            .agent
            .post(format!("{}{path}", endpoint.base_url))
            .header("Authorization", Self::authorization(endpoint))
            .header("Content-Type", "application/json")
            .send(body)
            .unwrap_or_else(|error| panic!("POST {path}: {error}"));
        let text = response.body_mut().read_to_string().unwrap_or_default();
        serde_json::from_str(&text).unwrap_or(serde_json::Value::Null)
    }

    fn get(&self, endpoint: &Endpoint, path: &str) -> serde_json::Value {
        let mut response = self
            .agent
            .get(format!("{}{path}", endpoint.base_url))
            .header("Authorization", Self::authorization(endpoint))
            .call()
            .unwrap_or_else(|error| panic!("GET {path}: {error}"));
        serde_json::from_str(&response.body_mut().read_to_string().unwrap()).unwrap()
    }

    /// Stops what the run started and checks nothing is left on the private port.
    fn finish(&mut self) {
        if let Err(problem) = self.cleanup() {
            panic!("{problem}");
        }
        assert!(!listening(self.port), "something still listens on the private port {}", self.port);
        assert!(!listening(self.chat_port), "the fake model is still up");
        println!("private service stopped, ports {} and {} free", self.port, self.chat_port);
    }

    /// Kills the fake model, stops the private service by the pid of its own registration
    /// (after checking that process carries the private HOME), then everything else that
    /// carries the private HOME, so the service's workers do not outlive the run.
    fn cleanup(&mut self) -> Result<(), String> {
        let mut problem = None;
        if let Some(mut fake) = self.fake.take() {
            let _ = fake.kill();
            let _ = fake.wait();
        }
        if self.service_started {
            self.service_started = false;
            match self.registered_pid() {
                Some(pid) if pid <= 1 => problem = Some(format!("refusing to signal registered pid {pid}")),
                Some(pid) => match processes_with_home(&self.home()) {
                    Some(owned) if !owned.contains(&pid) => {
                        problem = Some(format!(
                            "registered pid {pid} does not carry the private HOME; not signalling it"
                        ));
                    }
                    // Without /proc the registration's private port is the only evidence.
                    _ => {
                        let _ = kill_command().arg(pid.to_string()).status();
                    }
                },
                None => {}
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            while listening(self.port) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        for _ in 0..2 {
            for pid in processes_with_home(&self.home()).unwrap_or_default() {
                if pid > 1 {
                    let _ = kill_command().args(["-KILL", &pid.to_string()]).status();
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        problem.map_or(Ok(()), Err)
    }

    fn registered_pid(&self) -> Option<u32> {
        let text = std::fs::read_to_string(self.root.join("xdg/state/opencode/service.json")).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        let url = value["url"].as_str()?;
        url.ends_with(&format!(":{}", self.port)).then_some(())?;
        value["pid"].as_u64().map(|pid| pid as u32)
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        if let Err(problem) = self.cleanup() {
            eprintln!("{problem}");
        }
        if std::thread::panicking() {
            eprintln!("the scratch root is kept for inspection: {}", self.root.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

/// Base64 (standard alphabet, padded), for the Basic credentials.
fn base64(text: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in text.as_bytes().chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, byte)| n | u32::from(*byte) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
