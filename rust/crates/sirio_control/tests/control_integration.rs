//! Integration tests against a REAL unix socket in a temp directory
//! (set via $SIRIO_SOCKET). Every trap only exists on a real socket: an
//! idle client, a mid-request disconnect, an oversized line, concurrent
//! clients, and a stale socket file.

#[cfg(windows)]
use sirio_control::client::RawStream;
#[cfg(windows)]
use sirio_control::client::connect_raw;
use std::collections::BTreeMap;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sirio_acp::{AgentCommand, ChatSession, ChatSessionConfig, ChatSnapshot};
use sirio_control::protocol::rows;
use sirio_control::{
    ControlHandler, ControlRequest, ControlResponse, ControlServer, PaneError, PaneExitStatus,
    PaneInfo, PaneRegistry, PaneStateSnapshot, ScrollbackSource, protocol::request, round_trip,
};
use sirio_persistence::{AppDatabase, ProjectRecord, TabRecord, WorktreeRecord};

#[cfg(unix)]
type TestStream = UnixStream;
#[cfg(windows)]
type TestStream = RawStream;

fn connect_test_stream(path: &Path) -> std::io::Result<TestStream> {
    #[cfg(unix)]
    {
        UnixStream::connect(path)
    }
    #[cfg(windows)]
    {
        connect_raw(path)
    }
}

struct TempDir(PathBuf);
impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        #[cfg(unix)]
        {
            // Deliberately short, and under /tmp rather than $TMPDIR: a unix
            // socket path is capped at 104 bytes by sun_path, and the obvious
            // name under macOS's per-user $TMPDIR (/private/var/folders/../T/)
            // already spends ~55 of them. The descriptive version of this name
            // pushed bind() past the limit once the test counter reached two
            // digits, so it passed alone and failed in the suite.
            let _ = tag;
            let path = std::path::PathBuf::from(format!("/tmp/tc{}-{unique}", std::process::id()));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(std::fs::canonicalize(&path).expect("canonicalize"))
        }
        #[cfg(windows)]
        {
            // No sun_path-style cap on the pipe namespace; the standard
            // temp directory is fine.
            let path =
                std::env::temp_dir().join(format!("tc{}-{unique}-{tag}", std::process::id()));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }
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
                    "system.quit",
                    "project.list",
                    "project.add",
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
                    "panel.state",
                    "panel.scrollback",
                    "surface.changes.open",
                    "surface.changes.read",
                    "surface.changes.stage",
                    "surface.changes.unstage",
                    "surface.changes.discard",
                    "surface.changes.stage_all",
                    "surface.changes.discard_all",
                    "surface.settings.open",
                    "surface.settings.select",
                    "surface.settings.read",
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
                result.insert("project".to_string(), "sirio".to_string());
                result.insert("branch".to_string(), "main".to_string());
                result.insert("path".to_string(), "/Users/me/sirio".to_string());
                result.insert("workspaceId".to_string(), "wt-1".to_string());
                result.insert("surfaceId".to_string(), "pane-1".to_string());
                ControlResponse::success(&request.id, result)
            }
            "workspace.list" => {
                let mut row = BTreeMap::new();
                row.insert("id".to_string(), "wt-1".to_string());
                row.insert("project".to_string(), "sirio".to_string());
                row.insert("branch".to_string(), "main".to_string());
                row.insert("path".to_string(), "/Users/me/sirio".to_string());
                row.insert("selected".to_string(), "true".to_string());
                let mut result = BTreeMap::new();
                result.insert("workspaces".to_string(), rows::encode(&[row]));
                ControlResponse::success(&request.id, result)
            }
            "project.list" => {
                let row = BTreeMap::from([
                    ("id".to_string(), "project-1".to_string()),
                    ("name".to_string(), "sirio".to_string()),
                    ("path".to_string(), "/Users/me/sirio".to_string()),
                    ("isGit".to_string(), "true".to_string()),
                    ("worktreeCount".to_string(), "1".to_string()),
                    ("empty".to_string(), "false".to_string()),
                ]);
                ControlResponse::success(
                    &request.id,
                    BTreeMap::from([("projects".to_string(), rows::encode(&[row]))]),
                )
            }
            "project.add" => ControlResponse::success(
                &request.id,
                BTreeMap::from([
                    ("added".to_string(), "true".to_string()),
                    ("projectId".to_string(), "project-1".to_string()),
                ]),
            ),
            "workspace.current" => {
                let mut result = BTreeMap::new();
                result.insert("id".to_string(), "wt-1".to_string());
                result.insert("branch".to_string(), "main".to_string());
                result.insert("path".to_string(), "/Users/me/sirio".to_string());
                result.insert("project".to_string(), "sirio".to_string());
                ControlResponse::success(&request.id, result)
            }
            "workspace.create" => {
                let mut result = BTreeMap::new();
                result.insert("id".to_string(), "wt-new".to_string());
                ControlResponse::success(&request.id, result)
            }
            "system.quit"
            | "workspace.select"
            | "workspace.close"
            | "worktree.set"
            | "notify"
            | "session.ref"
            | "notification.create"
            | "notification.clear" => ControlResponse::success(&request.id, BTreeMap::new()),
            "panel.create" => ControlResponse::success(
                &request.id,
                BTreeMap::from([("id".to_string(), "pane-test".to_string())]),
            ),
            "panel.split" => ControlResponse::success(
                &request.id,
                BTreeMap::from([("id".to_string(), "pane-split".to_string())]),
            ),
            "panel.list" => ControlResponse::success(
                &request.id,
                BTreeMap::from([("panels".to_string(), rows::encode(&[]))]),
            ),
            "panel.read" => ControlResponse::success(
                &request.id,
                BTreeMap::from([("data".to_string(), String::new())]),
            ),
            "panel.state" => ControlResponse::success(
                &request.id,
                BTreeMap::from([
                    ("workingDirectory".to_string(), "/tmp".to_string()),
                    ("exitStatus".to_string(), "running".to_string()),
                    ("scrollbackBytes".to_string(), "0".to_string()),
                ]),
            ),
            "panel.scrollback" => ControlResponse::success(
                &request.id,
                BTreeMap::from([
                    ("data".to_string(), String::new()),
                    ("bytes".to_string(), "0".to_string()),
                ]),
            ),
            "surface.changes.open"
            | "surface.changes.read"
            | "surface.changes.stage"
            | "surface.changes.unstage"
            | "surface.changes.discard"
            | "surface.changes.stage_all"
            | "surface.changes.discard_all"
            | "surface.settings.open"
            | "surface.settings.select"
            | "surface.settings.read" => ControlResponse::success(
                &request.id,
                BTreeMap::from([("ready".to_string(), "true".to_string())]),
            ),
            "panel.wait" => ControlResponse::success(
                &request.id,
                BTreeMap::from([("exitCode".to_string(), "0".to_string())]),
            ),
            "panel.write" | "panel.key" | "panel.focus" | "panel.close" => {
                ControlResponse::success(&request.id, BTreeMap::new())
            }
            "pane.split" | "pane.focus" | "pane.close" | "tab.cycle" | "tab.select" => {
                ControlResponse::success(&request.id, BTreeMap::new())
            }
            "notification.list" => {
                let mut result = BTreeMap::new();
                result.insert("notifications".to_string(), rows::encode(&[]));
                ControlResponse::success(&request.id, result)
            }
            other => ControlResponse::failure(&request.id, format!("unknown method: {other}")),
        }
    }
}

/// A test server bound to a socket under a temp dir (via $SIRIO_SOCKET).
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
    fn read_line(&mut self, stream: &mut TestStream) -> Vec<u8> {
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
    let idle = connect_test_stream(&server.socket_path).expect("idle client connects");
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
    let mut line = sirio_control::encode_line(&request).expect("encode");
    line.truncate(10);
    {
        let mut stream = connect_test_stream(&server.socket_path).expect("connect");
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
    let huge = vec![b'x'; sirio_control::server::MAX_BUFFER_BYTES + 64 * 1024];
    let mut stream = connect_test_stream(&server.socket_path).expect("connect");
    let _ = stream.write_all(&huge);

    let mut reader = LineReader::new();
    let response_line = reader.read_line(&mut stream);
    let response: sirio_control::ControlResponse =
        sirio_control::decode_response(&response_line[..response_line.len() - 1])
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
fn oversized_complete_request_line_is_rejected_before_dispatch() {
    let (server, handler) = TestServer::start();
    let padding = "x".repeat(sirio_control::server::MAX_BUFFER_BYTES);
    let request = format!(
        r#"{{"id":"oversized-complete","method":"system.ping","params":{{"padding":"{padding}"}}}}
"#
    );
    assert!(request.len() > sirio_control::server::MAX_BUFFER_BYTES);

    let mut stream = connect_test_stream(&server.socket_path).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    // The server may reject and abort mid-write once it has seen the whole
    // line (client then sees EPIPE/broken pipe) — that is expected wire
    // behavior, so write errors are tolerated.
    let _ = stream.write_all(request.as_bytes());

    let mut reader = LineReader::new();
    let response_line = reader.read_line(&mut stream);
    let response = sirio_control::decode_response(&response_line[..response_line.len() - 1])
        .expect("rejection response decodes");
    assert!(!response.ok);
    assert_eq!(response.error.as_deref(), Some("request line too large"));
    assert!(
        handler
            .requests()
            .iter()
            .all(|request| request.id != "oversized-complete"),
        "oversized complete lines must not reach the handler"
    );
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

    #[cfg(unix)]
    {
        // Simulate a crashed previous run: a socket file with no live
        // server. (On Windows this scenario cannot exist — a named pipe
        // dies with its owning process — so there is nothing to stage.)
        let listener = UnixListener::bind(&socket_path).expect("bind stale");
        drop(listener); // leaves the socket file behind, nothing listening
        assert!(socket_path.exists(), "stale file present");
    }

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
        matches!(error, sirio_control::ServerError::AlreadyRunning { .. }),
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

    let mut stream = connect_test_stream(&server.socket_path).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("timeout");
    stream
        .write_all(b"this is not json\n")
        .expect("write garbage");

    let mut reader = LineReader::new();
    let response_line = reader.read_line(&mut stream);
    let response: sirio_control::ControlResponse =
        sirio_control::decode_response(&response_line[..response_line.len() - 1])
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
    let encoded = sirio_control::encode_line(&request).expect("encode");
    stream.write_all(&encoded).expect("write valid request");
    let response_line = reader.read_line(&mut stream);
    let response: sirio_control::ControlResponse =
        sirio_control::decode_response(&response_line[..response_line.len() - 1])
            .expect("valid response decodes");
    assert!(response.ok);
    assert_eq!(response.id, request.id);
}

#[test]
fn multiple_requests_on_one_connection_are_answered_in_order() {
    let (server, _) = TestServer::start();

    let mut stream = connect_test_stream(&server.socket_path).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("timeout");
    let first = request::system_ping();
    let second = request::system_ping();
    let mut payload = sirio_control::encode_line(&first).expect("encode");
    payload.extend(sirio_control::encode_line(&second).expect("encode"));
    // Send both at once (one read, two requests).
    stream.write_all(&payload).expect("write both");

    let mut reader = LineReader::new();
    let line = reader.read_line(&mut stream);
    let response: sirio_control::ControlResponse =
        sirio_control::decode_response(&line[..line.len() - 1]).expect("decodes");
    assert_eq!(
        response.id, first.id,
        "first response is for the first request"
    );

    let line = reader.read_line(&mut stream);
    let response: sirio_control::ControlResponse =
        sirio_control::decode_response(&line[..line.len() - 1]).expect("decodes");
    assert_eq!(
        response.id, second.id,
        "second response is for the second request"
    );
}

#[test]
fn request_split_across_reads_is_assembled() {
    let (server, _) = TestServer::start();

    let mut stream = connect_test_stream(&server.socket_path).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("timeout");
    let request = request::system_ping();
    let encoded = sirio_control::encode_line(&request).expect("encode");
    let split = encoded.len() / 2;
    stream.write_all(&encoded[..split]).expect("first half");
    std::thread::sleep(Duration::from_millis(30));
    stream.write_all(&encoded[split..]).expect("second half");

    let mut reader = LineReader::new();
    let line = reader.read_line(&mut stream);
    let response: sirio_control::ControlResponse =
        sirio_control::decode_response(&line[..line.len() - 1]).expect("decodes");
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
    #[cfg(unix)]
    assert!(socket_path.exists());

    server.stop();
    #[cfg(unix)]
    assert!(!socket_path.exists(), "socket file removed on stop");

    // Connecting now must fail (nothing listening).
    let error = connect_test_stream(&socket_path).expect_err("no listener after stop");
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

#[test]
fn server_creates_missing_parent_for_platform_default_style_socket() {
    let dir = TempDir::new("parent");
    let socket_path = dir.path().join("runtime/Sirio/control.sock");
    let server = ControlServer::new(socket_path.clone(), TestHandler::new());

    server.start().expect("server creates socket parent");
    #[cfg(unix)]
    assert!(socket_path.exists(), "socket exists below a new parent");
    let response = round_trip(
        &socket_path,
        &request::system_ping(),
        Duration::from_secs(5),
    )
    .expect("server on the newly created path serves");
    assert!(response.ok);
    server.stop();
}

// ---------------------------------------------------------------------------
// The sirioctl binary over a real socket
// ---------------------------------------------------------------------------

/// Runs the sirioctl binary against the server, returning stdout.
fn sirioctl(socket_path: &Path, args: &[&str]) -> std::process::Output {
    let binary = env!("CARGO_BIN_EXE_sirioctl");
    Command::new(binary)
        .args(args)
        .env("SIRIO_SOCKET", socket_path)
        .output()
        .expect("sirioctl runs")
}

#[test]
fn sirioctl_ping_prints_pong() {
    let (server, _) = TestServer::start();
    let output = sirioctl(&server.socket_path, &["ping"]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "pong");
}

#[test]
fn sirioctl_accepts_socket_before_command_as_documented() {
    let (server, _) = TestServer::start();
    let output = Command::new(env!("CARGO_BIN_EXE_sirioctl"))
        .args([
            "--socket",
            server.socket_path.to_str().expect("socket path"),
            "ping",
        ])
        .output()
        .expect("sirioctl runs");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "pong");
}

#[test]
fn sirioctl_rejects_ambiguous_notify_modes() {
    let (server, handler) = TestServer::start();
    let output = sirioctl(
        &server.socket_path,
        &[
            "notify",
            "--session",
            "pane-1",
            "--status",
            "running",
            "--title",
            "done",
        ],
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("ambiguous notify modes"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        handler
            .requests()
            .iter()
            .all(|request| request.method != "notification.create" && request.method != "notify"),
        "ambiguous notify input must not reach the socket"
    );
}

#[test]
fn sirioctl_quit_requests_a_graceful_application_exit() {
    let (server, handler) = TestServer::start();
    let output = sirioctl(&server.socket_path, &["quit"]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        handler
            .requests()
            .last()
            .map(|request| request.method.as_str()),
        Some("system.quit")
    );
}

// Control-owned panes have no PTY backend off unix yet (`spawn_process`
// returns Unsupported there), and these tests are about real process
// groups — genuinely unportable today, not merely inconvenient.
#[cfg(unix)]
#[test]
fn pane_registry_runs_a_real_command_and_returns_output_and_exit_code() {
    let registry = PaneRegistry::new();
    let pane = registry
        .create(
            std::env::current_dir().expect("current directory"),
            Some("printf registry-ready; exit 7"),
            "registry test",
        )
        .expect("pane creates");

    let exit_code = registry
        .wait(&pane.id, Some(Duration::from_secs(5)))
        .expect("pane exits");
    assert_eq!(exit_code, 7);
    assert_eq!(
        registry.read(&pane.id).expect("pane output"),
        b"registry-ready"
    );
}

// Control-owned panes have no PTY backend off unix yet (`spawn_process`
// returns Unsupported there), and these tests are about real process
// groups — genuinely unportable today, not merely inconvenient.
#[cfg(unix)]
#[test]
fn pane_registry_writes_input_to_a_live_command() {
    let registry = PaneRegistry::new();
    let pane = registry
        .create(
            std::env::current_dir().expect("current directory"),
            Some("IFS= read line; printf 'got:%s' \"$line\""),
            "input test",
        )
        .expect("pane creates");

    registry.write(&pane.id, b"hello\r").expect("pane write");
    let exit_code = registry
        .wait(&pane.id, Some(Duration::from_secs(5)))
        .expect("pane exits");
    assert_eq!(exit_code, 0);
    assert!(
        String::from_utf8_lossy(&registry.read(&pane.id).expect("pane output"))
            .contains("got:hello")
    );
}

// Control-owned panes have no PTY backend off unix yet (`spawn_process`
// returns Unsupported there), and these tests are about real process
// groups — genuinely unportable today, not merely inconvenient.
#[cfg(unix)]
#[test]
fn pane_registry_close_terminates_process_group() {
    let dir = TempDir::new("close-group");
    let pid_file = dir.path().join("pids");
    let command = format!(
        "( trap '' TERM HUP; exec sleep 60 ) & child=$!; printf '%s %s' \"$$\" \"$child\" > {}; wait",
        pid_file.display()
    );
    let registry = PaneRegistry::new();
    let pane = registry
        .create(dir.path(), Some(&command), "compound")
        .expect("spawn compound pane");

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let pids = loop {
        if let Ok(contents) = std::fs::read_to_string(&pid_file) {
            let pids = contents
                .split_whitespace()
                .map(|pid| pid.parse::<i32>().expect("numeric pid"))
                .collect::<Vec<_>>();
            if pids.len() == 2 {
                break pids;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "compound pane did not publish both pids"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(pids.iter().all(|pid| process_exists(*pid)));

    registry.close(&pane.id).expect("close compound pane");
    let terminated = wait_for_processes_to_exit(&pids, deadline);
    if !terminated {
        // Keep a red run from leaking a 60-second child into the developer's
        // session when the implementation still kills only the shell.
        unsafe {
            libc::kill(pids[1], libc::SIGKILL);
        }
    }
    assert!(
        terminated,
        "all process-group pids must be gone after close"
    );
}

// Control-owned panes have no PTY backend off unix yet (`spawn_process`
// returns Unsupported there), and these tests are about real process
// groups — genuinely unportable today, not merely inconvenient.
#[cfg(unix)]
#[test]
fn pane_registry_shutdown_for_only_its_worktree() {
    let first_dir = TempDir::new("shutdown-for-first");
    let second_dir = TempDir::new("shutdown-for-second");
    let first_pid_file = first_dir.path().join("pids");
    let second_pid_file = second_dir.path().join("pids");
    let first_command = format!(
        "( trap '' TERM HUP; exec sleep 60 ) & child=$!; printf '%s %s' \"$$\" \"$child\" > {}; wait",
        first_pid_file.display()
    );
    let second_command = format!(
        "( trap '' TERM HUP; exec sleep 60 ) & child=$!; printf '%s %s' \"$$\" \"$child\" > {}; wait",
        second_pid_file.display()
    );
    let registry = PaneRegistry::new();
    let first = registry
        .create(first_dir.path(), Some(&first_command), "first")
        .expect("spawn first pane");
    let second = registry
        .create(second_dir.path(), Some(&second_command), "second")
        .expect("spawn second pane");

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let first_pids = wait_for_pid_file(&first_pid_file, deadline);
    let second_pids = wait_for_pid_file(&second_pid_file, deadline);
    assert!(first_pids.iter().all(|pid| process_exists(*pid)));
    assert!(second_pids.iter().all(|pid| process_exists(*pid)));

    registry
        .shutdown_for(first_dir.path())
        .expect("shutdown first worktree");
    assert!(wait_for_processes_to_exit(&first_pids, deadline));
    assert!(
        registry
            .list_for(second_dir.path())
            .expect("list second worktree")
            .iter()
            .any(|pane| pane.id == second.id),
        "closing one worktree must leave other worktree panes registered"
    );
    assert!(
        second_pids.iter().all(|pid| process_exists(*pid)),
        "closing one worktree must leave other worktree processes alive"
    );

    assert!(registry.list_for(first_dir.path()).unwrap().is_empty());
    assert!(registry.close(&first.id).is_err());
    registry.shutdown();
    assert!(wait_for_processes_to_exit(&second_pids, deadline));
}

// Control-owned panes have no PTY backend off unix yet (`spawn_process`
// returns Unsupported there), and these tests are about real process
// groups — genuinely unportable today, not merely inconvenient.
#[cfg(unix)]
#[test]
fn workspace_close_terminates_process_group_when_worktree_is_missing() {
    let dir = TempDir::new("workspace-close-missing");
    let pid_file = dir.path().join("pids");
    let command = format!(
        "( trap \"\" TERM HUP; exec sleep 60 ) & child=$!; printf '%s %s' \"$$\" \"$child\" > {}; wait",
        pid_file.display()
    );
    let registry = PaneRegistry::new();
    registry
        .create(dir.path(), Some(&command), "workspace-close")
        .expect("spawn workspace pane");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let pids = wait_for_pid_file(&pid_file, deadline);
    assert!(pids.iter().all(|pid| process_exists(*pid)));

    std::fs::remove_dir_all(dir.path()).expect("remove worktree before close");
    registry
        .shutdown_for(dir.path())
        .expect("closing a missing worktree still terminates its panes");
    assert!(
        wait_for_processes_to_exit(&pids, deadline),
        "workspace close must terminate the process group even after the directory disappears"
    );
}

#[test]
fn pane_registry_lists_live_application_panes_for_their_worktree() {
    let registry = PaneRegistry::new();
    let working_directory = std::env::current_dir().expect("current directory");
    let pane = PaneInfo {
        id: "pane-ui-1".to_string(),
        tab: "Terminal".to_string(),
        title: "Terminal".to_string(),
        agent: String::new(),
        active: true,
    };

    registry
        .set_external(working_directory.clone(), vec![pane.clone()])
        .expect("application panes register");

    assert_eq!(
        registry.list_for(&working_directory).expect("list panes"),
        vec![pane]
    );
}

#[test]
fn pane_registry_reports_live_state_and_distinguishes_closed_from_unknown() {
    let registry = PaneRegistry::new();
    let working_directory = std::env::current_dir().expect("current directory");
    let source_calls = Arc::new(AtomicUsize::new(0));
    let source_calls_for_reader = Arc::clone(&source_calls);
    let source: ScrollbackSource = Arc::new(move || {
        source_calls_for_reader.fetch_add(1, Ordering::Relaxed);
        b"old\nnew\n".to_vec()
    });
    let pane = PaneInfo {
        id: "pane-ui-state".to_string(),
        tab: "Terminal".to_string(),
        title: "Terminal".to_string(),
        agent: String::new(),
        active: true,
    };
    registry
        .set_external_state(
            working_directory.clone(),
            vec![(
                pane.clone(),
                PaneStateSnapshot {
                    working_directory: working_directory.clone(),
                    exit_status: Some(PaneExitStatus::Success),
                },
                Some(source),
            )],
        )
        .expect("application pane state registers");

    assert_eq!(source_calls.load(Ordering::Relaxed), 0);
    assert_eq!(
        registry
            .list_for(&working_directory)
            .expect("list panes")
            .len(),
        1
    );
    assert_eq!(
        source_calls.load(Ordering::Relaxed),
        0,
        "listing panes must not consult their scrollback sources"
    );

    let state = registry.state(&pane.id).expect("live state");
    assert_eq!(state.exit_status, Some(PaneExitStatus::Success));
    assert_eq!(source_calls.load(Ordering::Relaxed), 0);
    assert_eq!(
        registry
            .scrollback(&pane.id, Some(4))
            .expect("bounded scrollback"),
        b"new\n".to_vec()
    );
    assert_eq!(source_calls.load(Ordering::Relaxed), 1);
    assert_eq!(registry.read(&pane.id).expect("live read"), b"old\nnew\n");
    assert_eq!(source_calls.load(Ordering::Relaxed), 2);

    registry
        .set_external_state(
            working_directory.clone(),
            vec![(
                pane.clone(),
                PaneStateSnapshot {
                    working_directory: working_directory.clone(),
                    exit_status: Some(PaneExitStatus::Success),
                },
                None,
            )],
        )
        .expect("application pane republishes without a source");
    assert!(
        registry
            .read(&pane.id)
            .expect("read without source")
            .is_empty()
    );
    assert!(
        registry
            .scrollback(&pane.id, None)
            .expect("scrollback without source")
            .is_empty()
    );
    assert_eq!(
        source_calls.load(Ordering::Relaxed),
        2,
        "a republished pane without a source cannot call the old source"
    );

    registry
        .set_external_state(working_directory, Vec::new())
        .expect("application pane closes");
    assert_eq!(
        registry.state(&pane.id),
        Err(PaneError::ClosedPane(pane.id.clone()))
    );
    assert_eq!(
        registry.scrollback("pane-never-seen", None),
        Err(PaneError::UnknownPane("pane-never-seen".to_string()))
    );
}

#[test]
fn pane_registry_read_finds_a_live_application_pane_panel_list_already_reported() {
    // Regression for F-AGENT-API-01: `panel.list` merges control- and
    // renderer-owned panes, but `read()` used to check only the
    // control-owned map, so a renderer-owned pane (e.g. an ACP-bridged
    // OpenCode tab) that `panel.list` had just shown as wired failed
    // `panel.read` moments later with `UnknownPane`.
    let registry = PaneRegistry::new();
    let working_directory = std::env::current_dir().expect("current directory");
    let pane = PaneInfo {
        id: "pane-opencode-1".to_string(),
        tab: "OpenCode".to_string(),
        title: "OpenCode".to_string(),
        agent: "opencode".to_string(),
        active: true,
    };
    registry
        .set_external_state(
            working_directory.clone(),
            vec![(
                pane.clone(),
                PaneStateSnapshot {
                    working_directory: working_directory.clone(),
                    exit_status: None,
                },
                Some(Arc::new(|| b"opencode ready\n".to_vec())),
            )],
        )
        .expect("application pane state registers");

    assert!(
        registry
            .list_for(&working_directory)
            .expect("list panes")
            .iter()
            .any(|listed| listed.id == pane.id),
        "panel.list must report the renderer-owned pane as wired"
    );
    assert_eq!(
        registry.read(&pane.id).expect("panel.read finds the pane"),
        b"opencode ready\n".to_vec()
    );
}

#[cfg(unix)]
#[test]
fn pane_registry_caches_canonical_worktree_paths_for_queries() {
    let registry = PaneRegistry::new();
    let root = std::env::temp_dir().join(format!(
        "sirio-control-canonical-cache-{}",
        std::process::id()
    ));
    let alias = root.with_extension("alias");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&alias);
    std::fs::create_dir_all(&root).expect("create canonical cache directory");
    std::os::unix::fs::symlink(&root, &alias).expect("create canonical cache symlink");

    let pane = PaneInfo {
        id: "pane-canonical-cache".to_string(),
        tab: "Terminal".to_string(),
        title: "Terminal".to_string(),
        agent: String::new(),
        active: true,
    };
    registry
        .set_external(&root, vec![pane.clone()])
        .expect("register canonical directory");
    assert_eq!(
        registry.list_for(&alias).expect("first alias lookup"),
        vec![pane]
    );

    std::fs::remove_file(&alias).expect("remove alias after it is cached");
    assert_eq!(
        registry
            .list_for(&alias)
            .expect("cached alias lookup after filesystem removal")
            .len(),
        1,
        "a repeated query must use the canonicalization cache"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn sirioctl_panel_create_emits_the_panel_method() {
    let (server, handler) = TestServer::start();
    let output = sirioctl(
        &server.socket_path,
        &[
            "panel",
            "create",
            "--worktree",
            "wt-1",
            "--cmd",
            "printf ready",
        ],
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let seen = handler.requests();
    let request = seen.last().expect("panel.create seen");
    assert_eq!(request.method, "panel.create");
    assert_eq!(
        request.params.get("worktree").map(String::as_str),
        Some("wt-1")
    );
    assert_eq!(
        request.params.get("cmd").map(String::as_str),
        Some("printf ready")
    );
}

#[test]
fn sirioctl_exposes_every_panel_subcommand() {
    let (server, handler) = TestServer::start();
    let commands = [
        (
            vec!["panel", "create", "--cmd", "printf ready"],
            "panel.create",
        ),
        (
            vec!["panel", "split", "right", "--from", "pane-test"],
            "panel.split",
        ),
        (vec!["panel", "list"], "panel.list"),
        (
            vec!["panel", "write", "pane-test", "--input", "hello", "--enter"],
            "panel.write",
        ),
        (
            vec!["panel", "key", "pane-test", "--key", "enter"],
            "panel.key",
        ),
        (vec!["panel", "read", "pane-test"], "panel.read"),
        (vec!["panel", "state", "pane-test"], "panel.state"),
        (
            vec!["panel", "scrollback", "pane-test", "--max-bytes", "32"],
            "panel.scrollback",
        ),
        (
            vec!["panel", "wait", "pane-test", "--timeout-ms", "100"],
            "panel.wait",
        ),
        (vec!["panel", "focus", "pane-test"], "panel.focus"),
        (vec!["panel", "close", "pane-test"], "panel.close"),
    ];

    for (args, method) in &commands {
        let output = sirioctl(&server.socket_path, args);
        assert!(
            output.status.success(),
            "{method} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let methods: Vec<_> = handler
        .requests()
        .into_iter()
        .map(|request| request.method)
        .collect();
    assert_eq!(
        methods,
        commands
            .iter()
            .map(|(_, method)| method.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn sirioctl_exposes_terminal_state_and_surface_subcommands() {
    let (server, handler) = TestServer::start();
    let commands = [
        (vec!["panel", "state", "pane-test"], "panel.state"),
        (
            vec!["panel", "scrollback", "pane-test", "--max-bytes", "16"],
            "panel.scrollback",
        ),
        (
            vec!["surface", "changes", "open", "--worktree", "wt-1"],
            "surface.changes.open",
        ),
        (vec!["surface", "changes", "read"], "surface.changes.read"),
        (
            vec!["surface", "settings", "open", "--section", "Agents"],
            "surface.settings.open",
        ),
        (
            vec!["surface", "settings", "select", "Agents"],
            "surface.settings.select",
        ),
        (vec!["surface", "settings", "read"], "surface.settings.read"),
    ];

    for (args, method) in &commands {
        let output = sirioctl(&server.socket_path, args);
        assert!(
            output.status.success(),
            "{method} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let methods: Vec<_> = handler
        .requests()
        .into_iter()
        .map(|request| request.method)
        .collect();
    assert_eq!(
        methods,
        commands
            .iter()
            .map(|(_, method)| method.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn sirioctl_exposes_changes_mutation_subcommands() {
    let (server, handler) = TestServer::start();
    let commands = [
        (
            vec!["surface", "changes", "stage", "f.txt"],
            "surface.changes.stage",
        ),
        (
            vec!["surface", "changes", "unstage", "f.txt"],
            "surface.changes.unstage",
        ),
        (
            vec!["surface", "changes", "discard", "f.txt"],
            "surface.changes.discard",
        ),
        (
            vec!["surface", "changes", "stage-all"],
            "surface.changes.stage_all",
        ),
        (
            vec!["surface", "changes", "discard-all"],
            "surface.changes.discard_all",
        ),
    ];

    for (args, method) in &commands {
        let output = sirioctl(&server.socket_path, args);
        assert!(
            output.status.success(),
            "{method} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let methods: Vec<_> = handler
        .requests()
        .into_iter()
        .map(|request| request.method)
        .collect();
    assert_eq!(
        methods,
        commands
            .iter()
            .map(|(_, method)| method.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn sirioctl_exposes_the_application_pane_and_tab_commands() {
    let (server, handler) = TestServer::start();
    let commands = [
        (vec!["pane", "split", "right"], "pane.split"),
        (vec!["pane", "focus", "left"], "pane.focus"),
        (vec!["pane", "close"], "pane.close"),
        (vec!["tab", "cycle", "backward"], "tab.cycle"),
        (vec!["tab", "select", "3"], "tab.select"),
    ];

    for (args, method) in &commands {
        let output = sirioctl(&server.socket_path, args);
        assert!(
            output.status.success(),
            "{method} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let requests = handler.requests();
    let methods: Vec<_> = requests
        .iter()
        .map(|request| request.method.as_str())
        .collect();
    assert_eq!(
        methods,
        commands
            .iter()
            .map(|(_, method)| *method)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        requests[0].params.get("direction").map(String::as_str),
        Some("right")
    );
    assert_eq!(
        requests[1].params.get("direction").map(String::as_str),
        Some("left")
    );
    assert_eq!(
        requests[3].params.get("direction").map(String::as_str),
        Some("backward")
    );
    assert_eq!(
        requests[4].params.get("index").map(String::as_str),
        Some("3")
    );
}

#[test]
fn sirioctl_capabilities_lists_methods() {
    let (server, _) = TestServer::start();
    let output = sirioctl(&server.socket_path, &["capabilities"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("system.ping"), "got: {stdout}");
    assert!(stdout.contains("workspace.list"), "got: {stdout}");
    assert!(stdout.contains("notify"), "got: {stdout}");
}

#[test]
fn sirioctl_project_list_prints_observable_catalog_rows() {
    let (server, _) = TestServer::start();
    let output = sirioctl(&server.socket_path, &["project", "list"]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "project-1\tsirio\t/Users/me/sirio\ttrue\t1\tfalse"
    );
}

#[test]
fn sirioctl_project_add_sends_the_path_and_reports_identity() {
    let (server, handler) = TestServer::start();
    let output = sirioctl(
        &server.socket_path,
        &["project", "add", "/tmp/sirio-fixture"],
    );
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "true\tproject-1"
    );
    let requests = handler.requests();
    let request = requests
        .iter()
        .find(|request| request.method == "project.add")
        .expect("project.add request");
    assert_eq!(
        request.params.get("path").map(String::as_str),
        Some("/tmp/sirio-fixture")
    );
}

#[test]
fn sirioctl_list_workspaces_prints_tab_separated_rows() {
    let (server, _) = TestServer::start();
    let output = sirioctl(&server.socket_path, &["list-workspaces"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let row = stdout.trim();
    assert!(
        row.starts_with("wt-1\tsirio\tmain\t/Users/me/sirio\ttrue"),
        "got: {row}"
    );
}

#[test]
fn sirioctl_identify_reads_environment_context() {
    let (server, _) = TestServer::start();
    let binary = env!("CARGO_BIN_EXE_sirioctl");
    let output = Command::new(binary)
        .args(["identify"])
        .env("SIRIO_SOCKET", &server.socket_path)
        .env("SIRIO_WORKTREE_ID", "wt-1")
        .env("SIRIO_PANE_ID", "pane-1")
        .output()
        .expect("sirioctl runs");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "sirio\tmain\t/Users/me/sirio\twt-1\tpane-1",
        "project, branch, path, workspaceId, surfaceId"
    );
}

#[test]
fn sirioctl_notify_sends_agent_status() {
    let (server, handler) = TestServer::start();
    let output = sirioctl(
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
fn sirioctl_notify_title_sends_user_notification() {
    let (server, handler) = TestServer::start();
    let output = sirioctl(
        &server.socket_path,
        &[
            "notify",
            "--title",
            "Build done",
            "--subtitle",
            "sirio",
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
fn sirioctl_session_ref_round_trips() {
    let (server, handler) = TestServer::start();
    let output = sirioctl(
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
fn sirioctl_worktree_set_round_trips() {
    let (server, handler) = TestServer::start();
    let output = sirioctl(
        &server.socket_path,
        &[
            "worktree-set",
            "--worktree",
            "wt-1",
            "--comment",
            "agent pane",
            "--session",
            "pane-1",
        ],
    );
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    let request = handler
        .requests()
        .last()
        .cloned()
        .expect("worktree.set request");
    assert_eq!(request.method, "worktree.set");
    assert_eq!(
        request.params.get("worktree").map(String::as_str),
        Some("wt-1")
    );
    assert_eq!(
        request.params.get("comment").map(String::as_str),
        Some("agent pane")
    );
    assert_eq!(
        request.params.get("session").map(String::as_str),
        Some("pane-1")
    );
}

#[test]
fn sirioctl_fails_cleanly_when_no_server_is_running() {
    let dir = TempDir::new("nohost");
    let dead_path = dir.path().join("dead.sock");
    let output = Command::new(env!("CARGO_BIN_EXE_sirioctl"))
        .args(["ping"])
        .env("SIRIO_SOCKET", &dead_path)
        .output()
        .expect("sirioctl runs");
    assert!(!output.status.success(), "must fail when nothing listens");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Sirio control socket is disabled or Sirio is not running"),
        "canonical message, got: {stderr}"
    );
}

#[test]
fn sirioctl_current_and_select_workspace() {
    let (server, _) = TestServer::start();

    let output = sirioctl(&server.socket_path, &["current-workspace"]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "sirio\tmain\t/Users/me/sirio\twt-1"
    );

    let output = sirioctl(
        &server.socket_path,
        &["select-workspace", "--workspace", "wt-1"],
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = sirioctl(
        &server.socket_path,
        &[
            "new-workspace",
            "--project",
            "sirio",
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
// Windows equivalent coverage: `windows_pipe::tests::
// bound_pipe_has_a_restricted_dacl` parses back the created pipe's security
// descriptor and refuses anything but owner+SYSTEM — and unlike unix, the
// DACL is supplied atomically inside CreateNamedPipeW, so there is no
// startup window for an observer thread to race in the first place.
#[cfg(unix)]
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
    let response = round_trip(
        &server.socket_path,
        &request::system_ping(),
        Duration::from_secs(5),
    )
    .expect("same-uid connection accepted");
    assert!(response.ok);
}

// Control-owned panes have no PTY backend off unix yet (`spawn_process`
// returns Unsupported there), and these tests are about real process
// groups — genuinely unportable today, not merely inconvenient.
#[cfg(unix)]
#[test]
fn pane_registry_shutdown_terminates_live_children() {
    let dir = TempDir::new("shutdown");
    let pid_file = dir.path().join("child.pid");
    let command = format!("printf '%s' \"$$\" > {}; exec sleep 60", pid_file.display());
    let registry = PaneRegistry::new();
    registry
        .create(dir.path(), Some(&command), "sleep")
        .expect("spawn child pane");

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let pid = loop {
        if let Ok(pid) = std::fs::read_to_string(&pid_file)
            && let Ok(pid) = pid.parse::<i32>()
        {
            break pid;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "pane child did not publish its pid"
        );
        std::thread::sleep(Duration::from_millis(10));
    };

    registry.shutdown();
    while std::time::Instant::now() < deadline {
        if !process_exists(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("pane child {pid} was still running after registry shutdown");
}

#[cfg(unix)]
fn process_exists(pid: i32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(unix)]
fn wait_for_pid_file(path: &Path, deadline: std::time::Instant) -> Vec<i32> {
    loop {
        if let Ok(contents) = std::fs::read_to_string(path) {
            let pids = contents
                .split_whitespace()
                .map(|pid| pid.parse::<i32>().expect("numeric pid"))
                .collect::<Vec<_>>();
            if pids.len() == 2 {
                return pids;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "compound pane did not publish both pids"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
fn wait_for_processes_to_exit(pids: &[i32], deadline: std::time::Instant) -> bool {
    while std::time::Instant::now() < deadline {
        if pids.iter().all(|pid| !process_exists(*pid)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

// ---------------------------------------------------------------------------
// P53 — a real socket door for the non-drawing chat adapter
// ---------------------------------------------------------------------------

const CHAT_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sirio_acp/tests/fixtures/acp_fixture.py"
);

struct ChatDoorHandler {
    session: Mutex<Option<ChatSession>>,
    database_path: PathBuf,
    tab_id: String,
    worktree_id: String,
}

impl ChatDoorHandler {
    fn new(database_path: PathBuf) -> Arc<Self> {
        let db = AppDatabase::open(&database_path).expect("open chat door database");
        db.save_project(&ProjectRecord::new(
            "project-chat",
            "fixture",
            "/tmp/fixture",
        ))
        .expect("save chat project");
        db.save_worktree(&WorktreeRecord::new(
            "worktree-chat",
            "project-chat",
            "main",
            "/tmp/fixture",
        ))
        .expect("save chat worktree");
        db.save_tabs(
            "worktree-chat",
            &[TabRecord::new("chat-1", "worktree-chat", "Chat", "chat")],
        )
        .expect("save chat tab");

        Arc::new(Self {
            session: Mutex::new(None),
            database_path,
            tab_id: "chat-1".into(),
            worktree_id: "worktree-chat".into(),
        })
    }

    fn launch(&self, mode: &str) -> anyhow::Result<ChatSession> {
        ChatSession::launch(ChatSessionConfig::new(
            &self.tab_id,
            &self.worktree_id,
            AgentCommand::new("python3").args([CHAT_FIXTURE, mode]),
            std::env::temp_dir(),
            &self.database_path,
        ))
    }

    fn open(&self) -> anyhow::Result<()> {
        let mut session = self.session.lock().expect("chat session lock");
        if session.is_none() {
            *session = Some(self.launch("normal")?);
        }
        Ok(())
    }

    fn snapshot(&self) -> anyhow::Result<ChatSnapshot> {
        self.session
            .lock()
            .expect("chat session lock")
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("chat surface is not open"))?
            .read()
    }

    fn send(&self, text: &str) -> anyhow::Result<ChatSnapshot> {
        {
            let session = self.session.lock().expect("chat session lock");
            session
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("chat surface is not open"))?
                .send(text)?;
        }
        self.snapshot()
    }

    fn compose(&self, text: &str) -> anyhow::Result<ChatSnapshot> {
        {
            let session = self.session.lock().expect("chat session lock");
            session
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("chat surface is not open"))?
                .compose(text)?;
        }
        self.snapshot()
    }

    fn permission(&self, request_id: u64, option_id: &str) -> anyhow::Result<ChatSnapshot> {
        {
            let session = self.session.lock().expect("chat session lock");
            session
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("chat surface is not open"))?
                .respond_permission(request_id, option_id)?;
        }
        self.snapshot()
    }

    fn stop(&self) -> anyhow::Result<ChatSnapshot> {
        {
            let session = self.session.lock().expect("chat session lock");
            session
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("chat surface is not open"))?
                .stop()?;
        }
        self.snapshot()
    }

    fn relaunch_restore(&self) -> anyhow::Result<()> {
        let mut session = self.session.lock().expect("chat session lock");
        if let Some(mut live) = session.take() {
            live.shutdown()?;
        }
        *session = Some(ChatSession::restore(
            &self.database_path,
            &self.tab_id,
            &self.worktree_id,
        )?);
        Ok(())
    }

    fn restart_live(&self, mode: &str) -> anyhow::Result<()> {
        let mut session = self.session.lock().expect("chat session lock");
        if let Some(mut old) = session.take() {
            old.shutdown()?;
        }
        *session = Some(self.launch(mode)?);
        Ok(())
    }
}

impl ControlHandler for ChatDoorHandler {
    fn handle(&self, request: &ControlRequest) -> ControlResponse {
        let result: anyhow::Result<ChatSnapshot> = (|| match request.method.as_str() {
            "surface.chat.open" => self.open().and_then(|()| self.snapshot()),
            "surface.chat.send" => {
                let text = request
                    .params
                    .get("text")
                    .ok_or_else(|| anyhow::anyhow!("surface.chat.send requires text"));
                text.and_then(|text| self.send(text))
            }
            "surface.chat.compose" => {
                let text = request
                    .params
                    .get("text")
                    .ok_or_else(|| anyhow::anyhow!("surface.chat.compose requires text"));
                text.and_then(|text| self.compose(text))
            }
            "surface.chat.permission" => {
                let request_id = request
                    .params
                    .get("requestId")
                    .ok_or_else(|| anyhow::anyhow!("surface.chat.permission requires requestId"))?
                    .parse::<u64>()
                    .map_err(|error| anyhow::anyhow!("invalid requestId: {error}"));
                let option_id = request
                    .params
                    .get("optionId")
                    .ok_or_else(|| anyhow::anyhow!("surface.chat.permission requires optionId"));
                request_id
                    .and_then(|request_id| option_id.map(|option_id| (request_id, option_id)))
                    .and_then(|(request_id, option_id)| self.permission(request_id, option_id))
            }
            "surface.chat.stop" => self.stop(),
            "surface.chat.read" => self.snapshot(),
            other => Err(anyhow::anyhow!("unknown method: {other}")),
        })();

        match result {
            Ok(snapshot) => ControlResponse::success(&request.id, snapshot_result(&snapshot)),
            Err(error) => ControlResponse::failure(&request.id, error.to_string()),
        }
    }
}

fn snapshot_result(snapshot: &ChatSnapshot) -> BTreeMap<String, String> {
    let mut result = BTreeMap::from([
        ("surfaceId".into(), snapshot.tab_id.clone()),
        ("status".into(), snapshot.status.as_str().into()),
        ("composerText".into(), snapshot.composer_text.clone()),
        ("queuedText".into(), snapshot.queued_text.clone()),
        (
            "transcript".into(),
            rows::encode(
                &snapshot
                    .transcript
                    .turns
                    .iter()
                    .flat_map(|turn| turn.entries.iter().map(chat_entry_row))
                    .collect::<Vec<_>>(),
            ),
        ),
    ]);
    if let Some(agent_session_id) = &snapshot.agent_session_id {
        result.insert("agentSessionId".into(), agent_session_id.clone());
    }
    if let Some(error) = &snapshot.error {
        result.insert("error".into(), error.clone());
    }
    result
}

fn chat_entry_row(entry: &sirio_persistence::ChatEntry) -> BTreeMap<String, String> {
    let mut row = BTreeMap::new();
    match entry {
        sirio_persistence::ChatEntry::UserMessage { text, .. } => {
            row.insert("kind".into(), "user".into());
            row.insert("text".into(), text.clone());
        }
        sirio_persistence::ChatEntry::AssistantMessage { text } => {
            row.insert("kind".into(), "assistant".into());
            row.insert("text".into(), text.clone());
        }
        sirio_persistence::ChatEntry::Thought { text, .. } => {
            row.insert("kind".into(), "thought".into());
            row.insert("text".into(), text.clone());
        }
        sirio_persistence::ChatEntry::ToolCall {
            id, title, status, ..
        } => {
            row.insert("kind".into(), "tool".into());
            row.insert("id".into(), id.clone());
            row.insert("text".into(), title.clone());
            row.insert("status".into(), status.clone());
        }
        sirio_persistence::ChatEntry::Permission {
            request_id,
            outcome,
            ..
        } => {
            row.insert("kind".into(), "permission".into());
            row.insert("id".into(), request_id.to_string());
            match outcome {
                sirio_persistence::ChatPermissionOutcome::Pending => {
                    row.insert("status".into(), "pending".into());
                }
                sirio_persistence::ChatPermissionOutcome::Selected { option_id, .. } => {
                    row.insert("status".into(), "selected".into());
                    row.insert("optionId".into(), option_id.clone());
                }
                sirio_persistence::ChatPermissionOutcome::Cancelled => {
                    row.insert("status".into(), "cancelled".into());
                }
                sirio_persistence::ChatPermissionOutcome::TimedOut => {
                    row.insert("status".into(), "timed_out".into());
                }
                sirio_persistence::ChatPermissionOutcome::Expired => {
                    row.insert("status".into(), "expired".into());
                }
            }
        }
        sirio_persistence::ChatEntry::Plan { entries } => {
            row.insert("kind".into(), "plan".into());
            row.insert(
                "text".into(),
                entries
                    .iter()
                    .map(|entry| format!("{} · {}", entry.status, entry.content))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        sirio_persistence::ChatEntry::TurnFooter { text } => {
            row.insert("kind".into(), "turn".into());
            row.insert("text".into(), text.clone());
        }
        sirio_persistence::ChatEntry::Error { message, .. } => {
            row.insert("kind".into(), "error".into());
            row.insert("text".into(), message.clone());
        }
    }
    row
}

fn chat_read(socket_path: &Path) -> BTreeMap<String, String> {
    let response = round_trip(
        socket_path,
        &request::chat_read("chat-1"),
        Duration::from_secs(5),
    )
    .expect("chat read round trip");
    assert!(response.ok, "chat read failed: {response:?}");
    response.result.expect("chat read result")
}

fn wait_for_chat_read<F>(socket_path: &Path, mut predicate: F) -> BTreeMap<String, String>
where
    F: FnMut(&BTreeMap<String, String>) -> bool,
{
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let result = chat_read(socket_path);
        if predicate(&result) {
            return result;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "chat read did not reach the expected state: {result:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn chat_door_streams_stops_and_restores_transcript_over_a_real_socket() {
    let dir = TempDir::new("chat-door");
    let database_path = dir.path().join("chat.sqlite");
    let socket_path = dir.path().join("chat.sock");
    let handler = ChatDoorHandler::new(database_path);
    let server = ControlServer::new(socket_path.clone(), handler.clone());
    server.start().expect("chat door server starts");

    let opened = round_trip(
        &socket_path,
        &request::chat_open(Some("worktree-chat")),
        Duration::from_secs(5),
    )
    .expect("open chat over socket");
    assert!(opened.ok, "chat open failed: {opened:?}");
    assert_eq!(
        opened
            .result
            .as_ref()
            .and_then(|result| result.get("status"))
            .map(String::as_str),
        Some("idle")
    );

    let sent = round_trip(
        &socket_path,
        &request::chat_send("chat-1", "exercise the chat door"),
        Duration::from_secs(5),
    )
    .expect("send chat turn over socket");
    assert!(sent.ok, "chat send failed: {sent:?}");

    let first_chunk = wait_for_chat_read(&socket_path, |result| {
        result.get("status").map(String::as_str) == Some("streaming")
            && rows::decode(result.get("transcript").expect("transcript")).is_some_and(|rows| {
                rows.iter().any(|row| {
                    row.get("kind").map(String::as_str) == Some("assistant")
                        && row.get("text").map(String::as_str) == Some("first ")
                })
            })
    });
    assert!(
        rows::decode(first_chunk.get("transcript").expect("transcript"))
            .expect("first transcript rows")
            .iter()
            .all(|row| row.get("kind").is_some())
    );

    let pending = wait_for_chat_read(&socket_path, |result| {
        rows::decode(result.get("transcript").expect("transcript")).is_some_and(|rows| {
            rows.iter().any(|row| {
                row.get("kind").map(String::as_str) == Some("permission")
                    && row.get("status").map(String::as_str) == Some("pending")
            })
        })
    });
    assert_eq!(pending.get("status").map(String::as_str), Some("streaming"));
    assert!(
        rows::decode(pending.get("transcript").expect("transcript"))
            .expect("pending transcript rows")
            .iter()
            .any(|row| row.get("kind").map(String::as_str) == Some("tool"))
    );

    let permission = round_trip(
        &socket_path,
        &request::chat_permission("chat-1", 1, "deny"),
        Duration::from_secs(5),
    )
    .expect("resolve permission over socket");
    assert!(permission.ok, "chat permission failed: {permission:?}");

    let completed = wait_for_chat_read(&socket_path, |result| {
        result.get("status").map(String::as_str) == Some("completed")
            && rows::decode(result.get("transcript").expect("transcript")).is_some_and(|rows| {
                rows.iter().any(|row| {
                    row.get("kind").map(String::as_str) == Some("tool")
                        && row.get("status").map(String::as_str) == Some("Completed")
                }) && rows.iter().any(|row| {
                    row.get("kind").map(String::as_str) == Some("permission")
                        && row.get("status").map(String::as_str) == Some("selected")
                        && row.get("optionId").map(String::as_str) == Some("deny")
                })
            })
    });
    let completed_rows = rows::decode(completed.get("transcript").expect("transcript"))
        .expect("completed transcript rows");
    let assistant_text = completed_rows
        .iter()
        .filter(|row| row.get("kind").map(String::as_str) == Some("assistant"))
        .filter_map(|row| row.get("text"))
        .cloned()
        .collect::<String>();
    assert_eq!(assistant_text, "first streamed denied");

    handler
        .relaunch_restore()
        .expect("relaunch chat adapter from persisted transcript");
    let restored = chat_read(&socket_path);
    assert_eq!(
        restored.get("status").map(String::as_str),
        Some("completed")
    );
    assert_eq!(restored.get("transcript"), completed.get("transcript"));

    handler
        .restart_live("cancel")
        .expect("restart live chat for stop proof");
    round_trip(
        &socket_path,
        &request::chat_send("chat-1", "stop this turn"),
        Duration::from_secs(5),
    )
    .expect("send cancellable turn over socket");
    wait_for_chat_read(&socket_path, |result| {
        result.get("status").map(String::as_str) == Some("streaming")
            && rows::decode(result.get("transcript").expect("transcript")).is_some_and(|rows| {
                rows.iter().any(|row| {
                    row.get("kind").map(String::as_str) == Some("assistant")
                        && row.get("text").map(String::as_str) == Some("partial")
                })
            })
    });
    let stopped_request = round_trip(
        &socket_path,
        &request::chat_stop("chat-1"),
        Duration::from_secs(5),
    )
    .expect("stop in-flight turn over socket");
    assert!(stopped_request.ok, "chat stop failed: {stopped_request:?}");
    let stopped = wait_for_chat_read(&socket_path, |result| {
        result.get("status").map(String::as_str) == Some("stopped")
    });
    assert!(
        rows::decode(stopped.get("transcript").expect("transcript")).is_some_and(|rows| {
            rows.iter().any(|row| {
                row.get("kind").map(String::as_str) == Some("turn")
                    && row.get("text").map(String::as_str) == Some("Cancelled")
            })
        })
    );

    server.stop();
}
