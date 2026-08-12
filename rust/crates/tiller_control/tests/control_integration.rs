//! Integration tests against a REAL unix socket in a temp directory
//! (set via $TILLER_SOCKET). Every trap only exists on a real socket: an
//! idle client, a mid-request disconnect, an oversized line, concurrent
//! clients, and a stale socket file.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tiller_control::protocol::rows;
use tiller_control::{
    ControlHandler, ControlRequest, ControlResponse, ControlServer, protocol::request, round_trip,
};

struct TempDir(PathBuf);
impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        // Deliberately short, and under /tmp rather than $TMPDIR: a unix
        // socket path is capped at 104 bytes by sun_path, and the obvious
        // name under macOS's per-user $TMPDIR (/private/var/folders/../T/)
        // already spends ~55 of them. The descriptive version of this name
        // pushed bind() past the limit once the test counter reached two
        // digits, so it passed alone and failed in the suite.
        let _ = tag;
        let path = std::path::PathBuf::from(format!(
            "/tmp/tc{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(std::fs::canonicalize(&path).expect("canonicalize"))
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A handler that answers every in-scope method, logging the requests it
/// saw, exactly as the app's control layer would.
struct TestHandler {
    seen: Mutex<Vec<ControlRequest>>,
}

impl TestHandler {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            seen: Mutex::new(Vec::new()),
        })
    }

    fn requests(&self) -> Vec<ControlRequest> {
        self.seen.lock().expect("lock").clone()
    }
}

impl ControlHandler for TestHandler {
    fn handle(&self, request: &ControlRequest) -> ControlResponse {
        self.seen.lock().expect("lock").push(request.clone());
        match request.method.as_str() {
            "system.ping" => {
                let mut result = BTreeMap::new();
                result.insert("pong".to_string(), "true".to_string());
                ControlResponse::success(&request.id, result)
            }
            "system.capabilities" => {
                let methods = [
                    "system.ping",
                    "system.capabilities",
                    "system.identify",
                    "workspace.list",
                    "workspace.create",
                    "workspace.select",
                    "workspace.current",
                    "workspace.close",
                    "notify",
                    "session.ref",
                    "notification.create",
                    "notification.list",
                    "notification.clear",
                ];
                let method_rows: Vec<BTreeMap<String, String>> = methods
                    .iter()
                    .map(|method| {
                        let mut row = BTreeMap::new();
                        row.insert("method".to_string(), method.to_string());
                        row
                    })
                    .collect();
                let mut result = BTreeMap::new();
                result.insert("methods".to_string(), rows::encode(&method_rows));
                result.insert("socketEnabled".to_string(), "true".to_string());
                ControlResponse::success(&request.id, result)
            }
            "system.identify" => {
                let mut result = BTreeMap::new();
                result.insert("project".to_string(), "tiller".to_string());
                result.insert("branch".to_string(), "main".to_string());
                result.insert("path".to_string(), "/Users/me/tiller".to_string());
                result.insert("workspaceId".to_string(), "wt-1".to_string());
                result.insert("surfaceId".to_string(), "pane-1".to_string());
                ControlResponse::success(&request.id, result)
            }
            "workspace.list" => {
                let mut row = BTreeMap::new();
                row.insert("id".to_string(), "wt-1".to_string());
                row.insert("project".to_string(), "tiller".to_string());
                row.insert("branch".to_string(), "main".to_string());
                row.insert("path".to_string(), "/Users/me/tiller".to_string());
                row.insert("selected".to_string(), "true".to_string());
                let mut result = BTreeMap::new();
                result.insert("workspaces".to_string(), rows::encode(&[row]));
                ControlResponse::success(&request.id, result)
            }
            "workspace.current" => {
                let mut result = BTreeMap::new();
                result.insert("id".to_string(), "wt-1".to_string());
                result.insert("branch".to_string(), "main".to_string());
                result.insert("path".to_string(), "/Users/me/tiller".to_string());
                result.insert("project".to_string(), "tiller".to_string());
                ControlResponse::success(&request.id, result)
            }
            "workspace.create" => {
                let mut result = BTreeMap::new();
                result.insert("id".to_string(), "wt-new".to_string());
                ControlResponse::success(&request.id, result)
            }
            "workspace.select"
            | "workspace.close"
            | "notify"
            | "session.ref"
            | "notification.create"
            | "notification.clear" => ControlResponse::success(&request.id, BTreeMap::new()),
            "notification.list" => {
                let mut result = BTreeMap::new();
                result.insert("notifications".to_string(), rows::encode(&[]));
                ControlResponse::success(&request.id, result)
            }
            other => ControlResponse::failure(&request.id, format!("unknown method: {other}")),
        }
    }
}

/// A test server bound to a socket under a temp dir (via $TILLER_SOCKET).
struct TestServer {
    server: ControlServer,
    /// Kept alive so the socket's parent directory outlives the server.
    _dir: TempDir,
    socket_path: PathBuf,
}

impl TestServer {
    fn start() -> (Self, Arc<TestHandler>) {
        let dir = TempDir::new("sock");
        let socket_path = dir.path().join("control.sock");
        let handler = TestHandler::new();
        let server = ControlServer::new(socket_path.clone(), handler.clone());
        server.start().expect("server starts");
        (
            Self {
                server,
                _dir: dir,
                socket_path,
            },
            handler,
        )
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.server.stop();
    }
}

/// A line reader over a stream with a persistent buffer: a read may deliver
/// several response lines, and the leftovers must not be lost between calls.
struct LineReader {
    buffer: Vec<u8>,
}

impl LineReader {
    fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Reads exactly one `\n`-terminated line (without the newline). The
    /// stream carries a read timeout so a server that never answers turns
    /// into an error instead of a hang.
    fn read_line(&mut self, stream: &mut UnixStream) -> Vec<u8> {
        let mut chunk = [0u8; 4096];
        loop {
            if let Some(newline) = self.buffer.iter().position(|b| *b == b'\n') {
                return self.buffer.drain(..=newline).collect();
            }
            let n = stream.read(&mut chunk).expect("read");
            assert!(n > 0, "server closed the connection before a line");
            self.buffer.extend_from_slice(&chunk[..n]);
        }
    }
}

// ---------------------------------------------------------------------------
// The traps
// ---------------------------------------------------------------------------

#[test]
fn idle_client_does_not_block_other_clients() {
    let (server, _) = TestServer::start();

    // Client A connects and sends nothing.
    let idle = UnixStream::connect(&server.socket_path).expect("idle client connects");
    std::thread::sleep(Duration::from_millis(50));

    // Client B must still get a prompt response.
    let request = request::system_ping();
    let started = std::time::Instant::now();
    let response = round_trip(&server.socket_path, &request, Duration::from_secs(5))
        .expect("round trip while an idle client is connected");
    assert!(response.ok);
    assert_eq!(response.id, request.id);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "idle client blocked the server: {:?}",
        started.elapsed()
    );

    drop(idle);
}

#[test]
fn disconnect_mid_request_does_not_take_the_server_down() {
    let (server, _) = TestServer::start();

    // Send half a request line, then disconnect.
    let request = request::system_ping();
    let mut line = tiller_control::encode_line(&request).expect("encode");
    line.truncate(10);
    {
        let mut stream = UnixStream::connect(&server.socket_path).expect("connect");
        stream.write_all(&line).expect("partial write");
    } // dropped mid-request

    std::thread::sleep(Duration::from_millis(50));

    // The server must still serve a complete request.
    let response = round_trip(&server.socket_path, &request, Duration::from_secs(5))
        .expect("server alive after mid-request disconnect");
    assert!(response.ok);
}

#[test]
fn oversized_request_line_is_rejected_not_buffered() {
    let (server, _) = TestServer::start();

    // A line larger than the 1 MiB cap, without a newline. The server may
    // reject and close mid-write (the client sees EPIPE) — that is the
    // expected wire behavior, so write errors are tolerated.
    let huge = vec![b'x'; tiller_control::server::MAX_BUFFER_BYTES + 64 * 1024];
    let mut stream = UnixStream::connect(&server.socket_path).expect("connect");
    let _ = stream.write_all(&huge);

    let mut reader = LineReader::new();
    let response_line = reader.read_line(&mut stream);
    let response: tiller_control::ControlResponse =
        tiller_control::decode_response(&response_line[..response_line.len() - 1])
            .expect("rejection response decodes");
    assert!(!response.ok);
    assert_eq!(response.error.as_deref(), Some("request line too large"));
    assert_eq!(response.id, "?");

    // The server must still serve other clients.
    let request = request::system_ping();
    let response =
        round_trip(&server.socket_path, &request, Duration::from_secs(5)).expect("still serving");
    assert!(response.ok);
}

#[test]
fn concurrent_clients_are_all_served() {
    let (server, _) = TestServer::start();

    let mut handles = Vec::new();
    for client in 0..8 {
        let socket_path = server.socket_path.clone();
        handles.push(std::thread::spawn(move || {
            for round in 0..5 {
                let request = request::system_ping();
                let response =
                    round_trip(&socket_path, &request, Duration::from_secs(5)).expect("round trip");
                assert!(response.ok);
                assert_eq!(response.id, request.id, "client {client} round {round}");
            }
        }));
    }
    for handle in handles {
        handle.join().expect("client thread");
    }
}

#[test]
fn stale_socket_file_is_replaced_cleanly() {
    let dir = TempDir::new("stale");
    let socket_path = dir.path().join("control.sock");

    // Simulate a crashed previous run: a socket file with no live server.
    {
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).expect("bind stale");
        drop(listener); // leaves the socket file behind, nothing listening
    }
    assert!(socket_path.exists(), "stale file present");

    // Starting the server must replace it, not fail forever.
    let handler = TestHandler::new();
    let server = ControlServer::new(socket_path.clone(), handler);
    server.start().expect("stale socket replaced");

    let response = round_trip(
        &socket_path,
        &request::system_ping(),
        Duration::from_secs(5),
    )
    .expect("serving on the replaced socket");
    assert!(response.ok);
}

#[test]
fn live_socket_is_not_stolen() {
    let dir = TempDir::new("live");
    let socket_path = dir.path().join("control.sock");

    // A live server owns the path; a second server must refuse, not steal.
    let handler = TestHandler::new();
    let first = ControlServer::new(socket_path.clone(), handler);
    first.start().expect("first server");

    let second = ControlServer::new(socket_path.clone(), TestHandler::new());
    let error = second
        .start()
        .expect_err("second server must refuse the live socket");
    assert!(
        matches!(error, tiller_control::ServerError::AlreadyRunning { .. }),
        "got {error:?}"
    );

    // The first server still answers.
    let response = round_trip(
        &socket_path,
        &request::system_ping(),
        Duration::from_secs(5),
    )
    .expect("first server alive");
    assert!(response.ok);
}

// ---------------------------------------------------------------------------
// Protocol behavior
// ---------------------------------------------------------------------------

#[test]
fn malformed_request_gets_an_error_and_the_connection_keeps_serving() {
    let (server, _) = TestServer::start();

    let mut stream = UnixStream::connect(&server.socket_path).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("timeout");
    stream
        .write_all(b"this is not json\n")
        .expect("write garbage");

    let mut reader = LineReader::new();
    let response_line = reader.read_line(&mut stream);
    let response: tiller_control::ControlResponse =
        tiller_control::decode_response(&response_line[..response_line.len() - 1])
            .expect("error response decodes");
    assert!(!response.ok);
    assert_eq!(response.id, "?");
    assert!(
        response
            .error
            .unwrap_or_default()
            .starts_with("malformed request")
    );

    // Same connection keeps serving after the malformed line.
    let request = request::system_ping();
    let encoded = tiller_control::encode_line(&request).expect("encode");
    stream.write_all(&encoded).expect("write valid request");
    let response_line = reader.read_line(&mut stream);
    let response: tiller_control::ControlResponse =
        tiller_control::decode_response(&response_line[..response_line.len() - 1])
            .expect("valid response decodes");
    assert!(response.ok);
    assert_eq!(response.id, request.id);
}

#[test]
fn multiple_requests_on_one_connection_are_answered_in_order() {
    let (server, _) = TestServer::start();

    let mut stream = UnixStream::connect(&server.socket_path).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("timeout");
    let first = request::system_ping();
    let second = request::system_ping();
    let mut payload = tiller_control::encode_line(&first).expect("encode");
    payload.extend(tiller_control::encode_line(&second).expect("encode"));
    // Send both at once (one read, two requests).
    stream.write_all(&payload).expect("write both");

    let mut reader = LineReader::new();
    let line = reader.read_line(&mut stream);
    let response: tiller_control::ControlResponse =
        tiller_control::decode_response(&line[..line.len() - 1]).expect("decodes");
    assert_eq!(
        response.id, first.id,
        "first response is for the first request"
    );

    let line = reader.read_line(&mut stream);
    let response: tiller_control::ControlResponse =
        tiller_control::decode_response(&line[..line.len() - 1]).expect("decodes");
    assert_eq!(
        response.id, second.id,
        "second response is for the second request"
    );
}

#[test]
fn request_split_across_reads_is_assembled() {
    let (server, _) = TestServer::start();

    let mut stream = UnixStream::connect(&server.socket_path).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("timeout");
    let request = request::system_ping();
    let encoded = tiller_control::encode_line(&request).expect("encode");
    let split = encoded.len() / 2;
    stream.write_all(&encoded[..split]).expect("first half");
    std::thread::sleep(Duration::from_millis(30));
    stream.write_all(&encoded[split..]).expect("second half");

    let mut reader = LineReader::new();
    let line = reader.read_line(&mut stream);
    let response: tiller_control::ControlResponse =
        tiller_control::decode_response(&line[..line.len() - 1]).expect("decodes");
    assert!(response.ok);
    assert_eq!(response.id, request.id);
}

#[test]
fn handler_receives_exact_methods_and_params() {
    let (server, handler) = TestServer::start();

    // notify with an agent session ref.
    let notify = request::notify("pane-1", "needs-input", Some("sess-9"));
    round_trip(&server.socket_path, &notify, Duration::from_secs(5)).expect("notify");

    // notify without a session ref must omit the param.
    let notify_bare = request::notify("pane-2", "running", None);
    round_trip(&server.socket_path, &notify_bare, Duration::from_secs(5)).expect("notify bare");

    let session_ref = request::session_ref("pane-1", "sess-9");
    round_trip(&server.socket_path, &session_ref, Duration::from_secs(5)).expect("session.ref");

    let identify = request::system_identify(Some("wt-1"), Some("pane-1"));
    let response =
        round_trip(&server.socket_path, &identify, Duration::from_secs(5)).expect("identify");
    assert_eq!(
        response
            .result
            .as_ref()
            .and_then(|r| r.get("branch"))
            .map(String::as_str),
        Some("main")
    );

    let seen = handler.requests();
    assert_eq!(seen.len(), 4);
    assert_eq!(seen[0].method, "notify");
    assert_eq!(
        seen[0].params.get("session").map(String::as_str),
        Some("pane-1")
    );
    assert_eq!(
        seen[0].params.get("status").map(String::as_str),
        Some("needs-input")
    );
    assert_eq!(
        seen[0].params.get("agentSession").map(String::as_str),
        Some("sess-9")
    );
    assert!(
        !seen[1].params.contains_key("agentSession"),
        "omitted when empty"
    );
    assert_eq!(seen[2].method, "session.ref");
    assert_eq!(
        seen[2].params.get("ref").map(String::as_str),
        Some("sess-9")
    );
    assert_eq!(seen[3].method, "system.identify");
}

#[test]
fn unknown_methods_fail_without_killing_the_server() {
    let (server, _) = TestServer::start();

    let mut request = request::system_ping();
    request.method = "no.such.method".to_string();
    let response = round_trip(&server.socket_path, &request, Duration::from_secs(5))
        .expect("unknown method answered");
    assert!(!response.ok);
    assert_eq!(response.id, request.id);
    assert_eq!(
        response.error.as_deref(),
        Some("unknown method: no.such.method")
    );

    let ping = request::system_ping();
    let response =
        round_trip(&server.socket_path, &ping, Duration::from_secs(5)).expect("still serving");
    assert!(response.ok);
}

#[test]
fn stop_removes_the_socket_file_and_stops_accepting() {
    let dir = TempDir::new("stop");
    let socket_path = dir.path().join("control.sock");
    let server = ControlServer::new(socket_path.clone(), TestHandler::new());
    server.start().expect("start");
    assert!(socket_path.exists());

    server.stop();
    assert!(!socket_path.exists(), "socket file removed on stop");

    // Connecting now must fail (nothing listening).
    let error = UnixStream::connect(&socket_path).expect_err("no listener after stop");
    let _ = error;
    // And the path can be reused by a fresh server.
    let again = ControlServer::new(socket_path.clone(), TestHandler::new());
    again.start().expect("restart on the same path");
    let response = round_trip(
        &socket_path,
        &request::system_ping(),
        Duration::from_secs(5),
    )
    .expect("restarted server serves");
    assert!(response.ok);
}

// ---------------------------------------------------------------------------
// The tillerctl binary over a real socket
// ---------------------------------------------------------------------------

/// Runs the tillerctl binary against the server, returning stdout.
fn tillerctl(socket_path: &Path, args: &[&str]) -> std::process::Output {
    let binary = env!("CARGO_BIN_EXE_tillerctl");
    Command::new(binary)
        .args(args)
        .env("TILLER_SOCKET", socket_path)
        .output()
        .expect("tillerctl runs")
}

#[test]
fn tillerctl_ping_prints_pong() {
    let (server, _) = TestServer::start();
    let output = tillerctl(&server.socket_path, &["ping"]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "pong");
}

#[test]
fn tillerctl_capabilities_lists_methods() {
    let (server, _) = TestServer::start();
    let output = tillerctl(&server.socket_path, &["capabilities"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("system.ping"), "got: {stdout}");
    assert!(stdout.contains("workspace.list"), "got: {stdout}");
    assert!(stdout.contains("notify"), "got: {stdout}");
}

#[test]
fn tillerctl_list_workspaces_prints_tab_separated_rows() {
    let (server, _) = TestServer::start();
    let output = tillerctl(&server.socket_path, &["list-workspaces"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let row = stdout.trim();
    assert!(
        row.starts_with("wt-1\ttiller\tmain\t/Users/me/tiller\ttrue"),
        "got: {row}"
    );
}

#[test]
fn tillerctl_identify_reads_environment_context() {
    let (server, _) = TestServer::start();
    let binary = env!("CARGO_BIN_EXE_tillerctl");
    let output = Command::new(binary)
        .args(["identify"])
        .env("TILLER_SOCKET", &server.socket_path)
        .env("TILLER_WORKTREE_ID", "wt-1")
        .env("TILLER_PANE_ID", "pane-1")
        .output()
        .expect("tillerctl runs");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "tiller\tmain\t/Users/me/tiller\twt-1\tpane-1",
        "project, branch, path, workspaceId, surfaceId"
    );
}

#[test]
fn tillerctl_notify_sends_agent_status() {
    let (server, handler) = TestServer::start();
    let output = tillerctl(
        &server.socket_path,
        &[
            "notify",
            "--session",
            "pane-1",
            "--status",
            "needs-input",
            "--agent-session",
            "sess-7",
        ],
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let seen = handler.requests();
    assert_eq!(seen.last().expect("notify seen").method, "notify");
    let params = &seen.last().expect("notify seen").params;
    assert_eq!(
        params.get("status").map(String::as_str),
        Some("needs-input")
    );
    assert_eq!(
        params.get("agentSession").map(String::as_str),
        Some("sess-7")
    );
}

#[test]
fn tillerctl_notify_title_sends_user_notification() {
    let (server, handler) = TestServer::start();
    let output = tillerctl(
        &server.socket_path,
        &[
            "notify",
            "--title",
            "Build done",
            "--subtitle",
            "tiller",
            "--body",
            "green",
        ],
    );
    assert!(output.status.success());

    let seen = handler.requests();
    assert_eq!(seen.last().expect("seen").method, "notification.create");
    let params = &seen.last().expect("seen").params;
    assert_eq!(params.get("title").map(String::as_str), Some("Build done"));
    assert_eq!(params.get("body").map(String::as_str), Some("green"));
}

#[test]
fn tillerctl_session_ref_round_trips() {
    let (server, handler) = TestServer::start();
    let output = tillerctl(
        &server.socket_path,
        &["session-ref", "--session", "pane-1", "--ref", "sess-42"],
    );
    assert!(output.status.success());
    let seen = handler.requests();
    assert_eq!(seen.last().expect("seen").method, "session.ref");
    assert_eq!(
        seen.last()
            .expect("seen")
            .params
            .get("ref")
            .map(String::as_str),
        Some("sess-42")
    );
}

#[test]
fn tillerctl_fails_cleanly_when_no_server_is_running() {
    let dir = TempDir::new("nohost");
    let dead_path = dir.path().join("dead.sock");
    let output = Command::new(env!("CARGO_BIN_EXE_tillerctl"))
        .args(["ping"])
        .env("TILLER_SOCKET", &dead_path)
        .output()
        .expect("tillerctl runs");
    assert!(!output.status.success(), "must fail when nothing listens");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Tiller control socket is disabled or Tiller is not running"),
        "canonical message, got: {stderr}"
    );
}

#[test]
fn tillerctl_current_and_select_workspace() {
    let (server, _) = TestServer::start();

    let output = tillerctl(&server.socket_path, &["current-workspace"]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "tiller\tmain\t/Users/me/tiller\twt-1"
    );

    let output = tillerctl(
        &server.socket_path,
        &["select-workspace", "--workspace", "wt-1"],
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = tillerctl(
        &server.socket_path,
        &[
            "new-workspace",
            "--project",
            "tiller",
            "--branch",
            "feature/x",
        ],
    );
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "wt-new");
}

// ---------------------------------------------------------------------------
// Socket-mode race and peer credentials
// ---------------------------------------------------------------------------

/// The regression test for the bind/chmod race: the socket must NEVER exist
/// at the umask-derived mode (0755 on a default macOS umask) — not even
/// briefly — because a local attacker can race the window and keep the
/// captured connection authorized forever. The reviewer's version of this
/// loop caught mode 0755 on 488 of 500 startups; with the umask narrowed
/// around the bind, the socket is born 0600 and this must be 0 of 500.
#[test]
fn socket_mode_is_never_permissive_during_startup() {
    use std::os::unix::fs::MetadataExt;

    let dir = TempDir::new("mode");
    let socket_path = dir.path().join("control.sock");

    let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observations = Arc::new(AtomicU64::new(0));
    let violations = Arc::new(AtomicU64::new(0));
    let stat_path = socket_path.clone();

    // A thread stats the socket path as fast as it can while the main
    // thread starts and stops servers. Every successful stat must see
    // mode 0600; a missing file (between stop and the next start) is not
    // a violation.
    let stat_thread = std::thread::spawn({
        let stop_flag = Arc::clone(&stop_flag);
        let observations = Arc::clone(&observations);
        let violations = Arc::clone(&violations);
        move || {
            while !stop_flag.load(Ordering::SeqCst) {
                if let Ok(metadata) = std::fs::symlink_metadata(&stat_path) {
                    observations.fetch_add(1, Ordering::Relaxed);
                    if metadata.mode() & 0o7777 != 0o600 {
                        violations.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
    });

    for _ in 0..500 {
        let server = ControlServer::new(socket_path.clone(), TestHandler::new());
        server.start().expect("start");
        // Give the stat thread a chance to sample inside the startup.
        std::thread::sleep(Duration::from_micros(50));
        server.stop();
    }

    stop_flag.store(true, Ordering::SeqCst);
    stat_thread.join().expect("stat thread");

    let observed = observations.load(Ordering::Relaxed);
    let violated = violations.load(Ordering::Relaxed);
    assert!(
        observed > 0,
        "the stat thread must observe the socket at least once"
    );
    assert_eq!(
        violated, 0,
        "socket mode was observed {violated}/{observed} times as anything but 0600"
    );
}

/// The peer-credential check must accept connections from the owner's uid
/// — every connection in this suite is same-uid and must keep working.
///
/// Honest limitation: a connection from a DIFFERENT uid cannot be
/// fabricated in a test without privilege escalation, so that half of the
/// check (refusing foreign uids via `getpeereid`) is verified by reading
/// `peer_is_owner` rather than by an automated test. The same-uid accept
/// path is exercised here and by every other round trip in this suite.
#[test]
fn same_uid_peer_is_accepted() {
    let (server, _) = TestServer::start();
    let response =
        round_trip(&server.socket_path, &request::system_ping(), Duration::from_secs(5))
            .expect("same-uid connection accepted");
    assert!(response.ok);
}
