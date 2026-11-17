//! The server against a real `Bridge`, driven by a `tungstenite` client over loopback.

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sirio_ide::{
    Bridge, BridgeEvent, BridgeOptions, ConnId, Diagnostic, DiffAnswer, DiffId, FileDiagnostics,
    Gone, Notification, Pos16, ReadOutcome, Refusal, Severity, bind,
};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::HeaderValue;
use tungstenite::{HandshakeError, Message, WebSocket};

const TOKEN: &str = "0123456789abcdef0123456789abcdef";

struct Harness {
    bridge: Bridge,
    port: u16,
    events: Receiver<BridgeEvent>,
}

fn harness(options: BridgeOptions) -> Harness {
    let listener = bind(None).expect("bind");
    let (tx, events) = mpsc::channel();
    let bridge = Bridge::start(listener, TOKEN.to_string(), options, move |e| {
        let _ = tx.send(e);
    })
    .expect("start");
    let port = bridge.port();
    Harness { bridge, port, events }
}

fn default_harness() -> Harness {
    harness(BridgeOptions::default())
}

fn request(port: u16, token: Option<&str>, offer: bool) -> tungstenite::handshake::client::Request {
    let mut req = format!("ws://127.0.0.1:{port}/").into_client_request().unwrap();
    if let Some(t) = token {
        req.headers_mut()
            .insert("x-claude-code-ide-authorization", HeaderValue::from_str(t).unwrap());
    }
    if offer {
        req.headers_mut().insert("Sec-WebSocket-Protocol", HeaderValue::from_static("mcp"));
        req.headers_mut().insert(
            "Sec-WebSocket-Extensions",
            HeaderValue::from_static("permessage-deflate"),
        );
    }
    req
}

type Handshake = Result<
    (WebSocket<TcpStream>, tungstenite::handshake::client::Response),
    HandshakeError<tungstenite::ClientHandshake<TcpStream>>,
>;

#[allow(clippy::result_large_err)]
fn try_connect(port: u16, token: Option<&str>, offer: bool) -> Handshake {
    let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    tungstenite::client(request(port, token, offer), stream)
}

struct Client(WebSocket<TcpStream>);

fn connect(port: u16) -> Client {
    let (ws, _) = try_connect(port, Some(TOKEN), true).expect("handshake");
    Client(ws)
}

impl Client {
    fn send(&mut self, v: Value) {
        self.0.send(Message::text(v.to_string())).unwrap();
    }

    fn request(&mut self, id: u64, method: &str, params: Value) {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
    }

    /// The next text frame as JSON, or `None` when nothing arrives within `wait` (or the socket closed).
    fn recv(&mut self, wait: Duration) -> Option<Value> {
        self.0.get_ref().set_read_timeout(Some(wait)).unwrap();
        loop {
            match self.0.read() {
                Ok(Message::Text(t)) => return Some(serde_json::from_str(t.as_str()).unwrap()),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    }

    fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.request(id, method, params);
        let v = self.recv(Duration::from_secs(5)).expect("a reply");
        assert_eq!(v["id"], json!(id));
        v
    }

    /// `initialize`, the initialized notification and `ide_connected`; returns the connection.
    fn handshake(&mut self, h: &Harness, pid: u32) -> ConnId {
        self.call(1, "initialize", json!({"protocolVersion":"2025-11-25"}));
        self.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        self.send(json!({"jsonrpc":"2.0","method":"ide_connected","params":{"pid":pid}}));
        match h.events.recv_timeout(Duration::from_secs(5)).unwrap() {
            BridgeEvent::Connected { conn, pid: p } => {
                assert_eq!(p, pid);
                conn
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

fn status_of(r: Handshake) -> u16 {
    match r {
        Err(HandshakeError::Failure(tungstenite::Error::Http(resp))) => resp.status().as_u16(),
        Ok(_) => panic!("handshake succeeded"),
        Err(e) => panic!("unexpected failure {e:?}"),
    }
}

#[test]
fn missing_token_is_refused_before_json_rpc() {
    let h = default_harness();
    assert_eq!(status_of(try_connect(h.port, None, true)), 401);
    assert!(h.events.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn wrong_token_is_refused() {
    let h = default_harness();
    let wrong = "ffffffffffffffffffffffffffffffff";
    assert_eq!(status_of(try_connect(h.port, Some(wrong), true)), 401);
    assert_eq!(status_of(try_connect(h.port, Some("short"), true)), 401);
    assert!(h.events.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn offered_mcp_subprotocol_is_echoed() {
    let h = default_harness();
    let (_ws, resp) = try_connect(h.port, Some(TOKEN), true).expect("handshake");
    assert_eq!(resp.headers().get("sec-websocket-protocol").unwrap(), "mcp");
    assert!(resp.headers().get("sec-websocket-extensions").is_none());
}

#[test]
fn initialize_echoes_protocol_version_and_names_sirio() {
    let h = default_harness();
    let mut c = connect(h.port);
    let v = c.call(7, "initialize", json!({"protocolVersion":"2025-11-25","capabilities":{}}));
    assert_eq!(v["jsonrpc"], "2.0");
    assert_eq!(v["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(v["result"]["serverInfo"]["name"], "sirio");
    assert!(v["result"]["capabilities"]["tools"].is_object());
}

#[test]
fn tools_list_is_exactly_the_four_tools() {
    let h = default_harness();
    let mut c = connect(h.port);
    let v = c.call(2, "tools/list", json!({}));
    let mut names: Vec<&str> = v["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["closeAllDiffTabs", "close_tab", "getDiagnostics", "openDiff"]);
    for t in v["result"]["tools"].as_array().unwrap() {
        assert_eq!(t["inputSchema"]["type"], "object");
    }
}

#[test]
fn unknown_method_is_32601_and_connection_survives() {
    let h = default_harness();
    let mut c = connect(h.port);
    let v = c.call(3, "resources/list", json!({}));
    assert_eq!(v["error"]["code"], -32601);
    let v = c.call(4, "ping", json!({}));
    assert_eq!(v["result"], json!({}));
}

#[test]
fn unknown_tool_is_32601() {
    let h = default_harness();
    let mut c = connect(h.port);
    let v = c.call(3, "tools/call", json!({"name":"nope","arguments":{}}));
    assert_eq!(v["error"]["code"], -32601);
}

#[test]
fn malformed_json_is_32700_and_connection_survives() {
    let h = default_harness();
    let mut c = connect(h.port);
    c.0.send(Message::text("{not json")).unwrap();
    let v = c.recv(Duration::from_secs(5)).unwrap();
    assert_eq!(v["error"]["code"], -32700);
    assert!(v["id"].is_null());
    let v = c.call(5, "ping", json!({}));
    assert_eq!(v["result"], json!({}));
}

#[test]
fn ide_connected_emits_connected_with_pid() {
    let h = default_harness();
    let mut c = connect(h.port);
    let conn = c.handshake(&h, 558674);
    drop(c);
    match h.events.recv_timeout(Duration::from_secs(5)).unwrap() {
        BridgeEvent::Disconnected { conn: gone } => assert_eq!(gone, conn),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn notifications_reach_only_their_connection() {
    let h = default_harness();
    let mut a = connect(h.port);
    let conn_a = a.handshake(&h, 1);
    let mut b = connect(h.port);
    let _conn_b = b.handshake(&h, 2);

    h.bridge.notify(
        conn_a,
        Notification::SelectionChanged {
            path: PathBuf::from("/work/src/lib.rs"),
            text: "fn main".to_string(),
            start: Pos16 { line: 3, character: 0 },
            end: Pos16 { line: 3, character: 7 },
        },
    );
    h.bridge.notify(
        conn_a,
        Notification::SelectionChanged {
            path: PathBuf::from("/work/src/lib.rs"),
            text: String::new(),
            start: Pos16 { line: 1, character: 2 },
            end: Pos16 { line: 1, character: 2 },
        },
    );
    h.bridge.notify(
        conn_a,
        Notification::AtMentioned {
            path: PathBuf::from("/work/src/lib.rs"),
            line_start: 4,
            line_end: 9,
        },
    );

    let n = a.recv(Duration::from_secs(5)).unwrap();
    assert_eq!(n["method"], "selection_changed");
    assert!(n.get("id").is_none());
    let p = &n["params"];
    assert_eq!(p["text"], "fn main");
    assert_eq!(p["filePath"], "/work/src/lib.rs");
    assert_eq!(p["fileUrl"], "file:///work/src/lib.rs");
    assert_eq!(p["selection"]["start"], json!({"line":3,"character":0}));
    assert_eq!(p["selection"]["end"], json!({"line":3,"character":7}));
    assert_eq!(p["selection"]["isEmpty"], false);

    let n = a.recv(Duration::from_secs(5)).unwrap();
    assert_eq!(n["params"]["selection"]["isEmpty"], true);

    let n = a.recv(Duration::from_secs(5)).unwrap();
    assert_eq!(n["method"], "at_mentioned");
    assert_eq!(
        n["params"],
        json!({"filePath":"/work/src/lib.rs","lineStart":4,"lineEnd":9})
    );

    assert!(b.recv(Duration::from_millis(200)).is_none());
}

fn warning(message: &str) -> Diagnostic {
    Diagnostic {
        message: message.to_string(),
        severity: Severity::Warning,
        start: Pos16 { line: 2, character: 4 },
        end: Pos16 { line: 2, character: 9 },
        source: Some("rust-analyzer".to_string()),
        code: Some("E0001".to_string()),
    }
}

fn diagnostics_event(h: &Harness) -> (sirio_ide::DiagId, ConnId, Option<PathBuf>) {
    loop {
        match h.events.recv_timeout(Duration::from_secs(5)).unwrap() {
            BridgeEvent::Diagnostics { id, conn, path } => return (id, conn, path),
            _ => continue,
        }
    }
}

#[test]
fn get_diagnostics_answered_by_app() {
    let h = default_harness();
    let mut c = connect(h.port);
    let conn = c.handshake(&h, 1);
    c.request(
        10,
        "tools/call",
        json!({"name":"getDiagnostics","arguments":{"uri":"file:///work/src/lib.rs"}}),
    );
    let (id, from, path) = diagnostics_event(&h);
    assert_eq!(from, conn);
    assert_eq!(path, Some(PathBuf::from("/work/src/lib.rs")));
    h.bridge.answer_diagnostics(
        &id,
        vec![FileDiagnostics {
            path: PathBuf::from("/work/src/lib.rs"),
            diagnostics: vec![warning("unused variable")],
        }],
    );
    let v = c.recv(Duration::from_secs(5)).unwrap();
    assert_eq!(v["id"], 10);
    let content = v["result"]["content"].as_array().unwrap();
    assert_eq!(content.len(), 1);
    assert_eq!(content[0]["type"], "text");
    let files: Value = serde_json::from_str(content[0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(files[0]["uri"], "file:///work/src/lib.rs");
    let d = &files[0]["diagnostics"][0];
    assert_eq!(d["message"], "unused variable");
    assert_eq!(d["severity"], "Warning");
    assert_eq!(d["range"]["start"], json!({"line":2,"character":4}));
    assert_eq!(d["range"]["end"], json!({"line":2,"character":9}));
    assert_eq!(d["source"], "rust-analyzer");
    assert_eq!(d["code"], "E0001");
}

#[test]
fn get_diagnostics_without_uri_asks_the_app_for_every_file() {
    let h = default_harness();
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    c.request(11, "tools/call", json!({"name":"getDiagnostics","arguments":{}}));
    let (id, _, path) = diagnostics_event(&h);
    assert_eq!(path, None);
    h.bridge.answer_diagnostics(
        &id,
        vec![
            FileDiagnostics { path: PathBuf::from("/w/a.rs"), diagnostics: vec![warning("a")] },
            FileDiagnostics { path: PathBuf::from("/w/b.rs"), diagnostics: vec![warning("b")] },
        ],
    );
    let v = c.recv(Duration::from_secs(5)).unwrap();
    let content = v["result"]["content"].as_array().unwrap();
    assert_eq!(content.len(), 1);
    let files: Value = serde_json::from_str(content[0]["text"].as_str().unwrap()).unwrap();
    let uris: Vec<&str> = files.as_array().unwrap().iter().map(|f| f["uri"].as_str().unwrap()).collect();
    assert_eq!(uris, ["file:///w/a.rs", "file:///w/b.rs"]);
}

#[test]
fn get_diagnostics_with_a_non_file_uri_is_32602_and_no_event() {
    let h = default_harness();
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    let v = c.call(
        12,
        "tools/call",
        json!({"name":"getDiagnostics","arguments":{"uri":"https://example.com/x.rs"}}),
    );
    assert_eq!(v["error"]["code"], -32602);
    assert!(h.events.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn get_diagnostics_past_deadline_is_an_error() {
    let h = harness(BridgeOptions { diagnostics_deadline: Duration::from_millis(200) });
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    let asked = Instant::now();
    c.request(13, "tools/call", json!({"name":"getDiagnostics","arguments":{}}));
    let (id, _, _) = diagnostics_event(&h);
    let v = c.recv(Duration::from_secs(5)).expect("the deadline error");
    assert!(asked.elapsed() >= Duration::from_millis(200));
    assert_eq!(v["id"], 13);
    assert_eq!(v["error"]["code"], -32000);

    // A late answer is dropped: the next frame is the ping's reply, not a second answer to 13.
    h.bridge.answer_diagnostics(&id, vec![]);
    let v = c.call(14, "ping", json!({}));
    assert_eq!(v["result"], json!({}));
}

#[test]
fn slow_diagnostics_do_not_block_ping() {
    let h = harness(BridgeOptions { diagnostics_deadline: Duration::from_secs(30) });
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    c.request(20, "tools/call", json!({"name":"getDiagnostics","arguments":{}}));
    let (id, _, _) = diagnostics_event(&h);
    let v = c.call(21, "ping", json!({}));
    assert_eq!(v["result"], json!({}));
    h.bridge.answer_diagnostics(&id, vec![]);
    let v = c.recv(Duration::from_secs(5)).unwrap();
    assert_eq!(v["id"], 20);
    assert!(v["result"]["content"].is_array());
}

#[test]
fn per_file_answer_echoes_the_request_uri() {
    let h = default_harness();
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    let uri = "file:///work/my dir/%41 b.rs";
    c.request(15, "tools/call", json!({"name":"getDiagnostics","arguments":{"uri":uri}}));
    let (id, _, path) = diagnostics_event(&h);
    assert_eq!(path, Some(PathBuf::from("/work/my dir/%41 b.rs")));
    h.bridge.answer_diagnostics(
        &id,
        vec![FileDiagnostics {
            // Not the path the uri names: only the echo can produce the request's spelling.
            path: PathBuf::from("/elsewhere/other.rs"),
            diagnostics: vec![warning("w")],
        }],
    );
    let v = c.recv(Duration::from_secs(5)).unwrap();
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    let files: Value = serde_json::from_str(text).unwrap();
    assert_eq!(files[0]["uri"], uri);
}

#[test]
fn shutdown_is_not_held_by_a_silent_peer() {
    let h = default_harness();
    let port = h.port;
    let _silent = TcpStream::connect(("127.0.0.1", port)).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let started = Instant::now();
    h.bridge.shutdown();
    assert!(started.elapsed() < Duration::from_secs(4));
    assert_eq!(bind(Some(port)).unwrap().local_addr().unwrap().port(), port);
}

#[test]
fn shutdown_closes_every_connection_and_the_port() {
    let h = default_harness();
    let port = h.port;
    let mut a = connect(port);
    a.handshake(&h, 1);
    let mut b = connect(port);
    b.handshake(&h, 2);
    h.bridge.shutdown();
    for c in [&mut a, &mut b] {
        // Reads until the close frame or EOF; a ping reply would be a bug.
        assert!(c.recv(Duration::from_secs(5)).is_none());
        assert!(c.0.read().is_err());
    }
    assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
}

// ---- openDiff, close_tab, closeAllDiffTabs ----

/// A private directory, removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let dir = std::env::temp_dir().join(format!("sirio-ide-srv-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Dir(dir)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn open_diff_params(old: &Path, new_contents: &str, tab: &str) -> Value {
    json!({"name":"openDiff","arguments":{
        "old_file_path": old.to_string_lossy(),
        "new_file_path": old.to_string_lossy(),
        "new_file_contents": new_contents,
        "tab_name": tab,
    }})
}

struct Opened {
    id: DiffId,
    conn: ConnId,
    original: ReadOutcome,
    canonical: Option<PathBuf>,
    tab_name: String,
    contents: String,
    replaces: Option<DiffId>,
}

fn next_open_diff(h: &Harness) -> Opened {
    match h.events.recv_timeout(Duration::from_secs(5)).expect("an OpenDiff event") {
        BridgeEvent::OpenDiff {
            id, conn, original, canonical, tab_name, contents, replaces, ..
        } => Opened { id, conn, original, canonical, tab_name, contents, replaces },
        other => panic!("unexpected {other:?}"),
    }
}

fn texts(v: &Value) -> Vec<String> {
    v["result"]["content"]
        .as_array()
        .unwrap_or_else(|| panic!("no content in {v}"))
        .iter()
        .map(|c| {
            assert_eq!(c["type"], "text");
            c["text"].as_str().unwrap().to_string()
        })
        .collect()
}

fn tab(file: &str, n: &str) -> String {
    format!("\u{273b} [Claude Code] {file} ({n}) \u{29c9}")
}

#[test]
fn open_diff_emits_event_and_waits() {
    let h = default_harness();
    let dir = Dir::new("open");
    let file = dir.0.join("a.txt");
    std::fs::write(&file, "alpha\n").unwrap();
    let mut c = connect(h.port);
    let conn = c.handshake(&h, 1);
    let name = tab("a.txt", "abc123");
    c.request(5, "tools/call", open_diff_params(&file, "beta\n", &name));
    let opened = next_open_diff(&h);
    assert_eq!(opened.conn, conn);
    assert_eq!(opened.original, ReadOutcome::Contents(b"alpha\n".to_vec()));
    assert_eq!(opened.canonical, std::fs::canonicalize(&file).ok());
    assert_eq!(opened.tab_name, name);
    assert_eq!(opened.contents, "beta\n");
    assert_eq!(opened.replaces, None);
    assert!(c.recv(Duration::from_millis(300)).is_none(), "openDiff answered by itself");
}

#[test]
fn open_diff_of_a_missing_file_reports_it_missing() {
    let h = default_harness();
    let dir = Dir::new("openmissing");
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    c.request(5, "tools/call", open_diff_params(&dir.0.join("new.txt"), "x", "t"));
    let opened = next_open_diff(&h);
    assert_eq!(opened.original, ReadOutcome::Missing);
    assert_eq!(opened.canonical, None);
}

#[test]
fn open_diff_of_a_directory_is_refused_but_still_opens() {
    let h = default_harness();
    let dir = Dir::new("opendir");
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    c.request(5, "tools/call", open_diff_params(&dir.0, "x", "t"));
    assert_eq!(next_open_diff(&h).original, ReadOutcome::Refused(Refusal::NotRegular));
}

#[test]
fn open_diff_without_a_tab_name_is_32602_and_no_event() {
    let h = default_harness();
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    let v = c.call(5, "tools/call", json!({"name":"openDiff","arguments":{"old_file_path":"/x"}}));
    assert_eq!(v["error"]["code"], -32602);
    assert!(h.events.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn saved_answer_is_file_saved_then_text() {
    let h = default_harness();
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    c.request(5, "tools/call", open_diff_params(Path::new("/nonexistent/a"), "x", "t"));
    let opened = next_open_diff(&h);
    h.bridge.answer_diff(&opened.id, DiffAnswer::Saved("edited\nno newline".into())).unwrap();
    let v = c.recv(Duration::from_secs(5)).unwrap();
    assert_eq!(v["id"], json!(5));
    assert_eq!(v["result"]["content"], json!([
        {"type":"text","text":"FILE_SAVED"},
        {"type":"text","text":"edited\nno newline"},
    ]));
}

#[test]
fn rejected_answer_is_diff_rejected_then_tab_name() {
    let h = default_harness();
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    let name = tab("a.txt", "ffffff");
    c.request(5, "tools/call", open_diff_params(Path::new("/nonexistent/a"), "x", &name));
    let opened = next_open_diff(&h);
    h.bridge.answer_diff(&opened.id, DiffAnswer::Rejected).unwrap();
    let v = c.recv(Duration::from_secs(5)).unwrap();
    assert_eq!(texts(&v), ["DIFF_REJECTED".to_string(), name]);
}

#[test]
fn close_tab_returns_tab_closed_and_withdraws_without_decision() {
    let h = default_harness();
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    c.request(5, "tools/call", open_diff_params(Path::new("/nonexistent/a"), "x", "t1"));
    let opened = next_open_diff(&h);
    c.request(6, "tools/call", json!({"name":"close_tab","arguments":{"tab_name":"t1"}}));
    let mut frames = [c.recv(Duration::from_secs(5)).unwrap(), c.recv(Duration::from_secs(5)).unwrap()];
    frames.sort_by_key(|f| f["id"].as_u64());
    assert_eq!(frames[0]["id"], json!(5));
    assert_eq!(frames[0]["error"]["code"], -32000);
    assert!(frames[0].get("result").is_none());
    assert_eq!(frames[1]["id"], json!(6));
    assert_eq!(frames[1]["result"]["content"], json!([{"type":"text","text":"TAB_CLOSED"}]));
    match h.events.recv_timeout(Duration::from_secs(5)).unwrap() {
        BridgeEvent::DiffWithdrawn { id } => assert_eq!(id, opened.id),
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(h.bridge.answer_diff(&opened.id, DiffAnswer::Rejected), Err(Gone));
}

#[test]
fn close_tab_after_answer_is_harmless() {
    let h = default_harness();
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    c.request(5, "tools/call", open_diff_params(Path::new("/nonexistent/a"), "x", "t1"));
    let opened = next_open_diff(&h);
    h.bridge.answer_diff(&opened.id, DiffAnswer::Saved("y".into())).unwrap();
    assert_eq!(c.recv(Duration::from_secs(5)).unwrap()["id"], json!(5));
    for id in [6, 7] {
        let v = c.call(id, "tools/call", json!({"name":"close_tab","arguments":{"tab_name":"t1"}}));
        assert_eq!(v["result"]["content"], json!([{"type":"text","text":"TAB_CLOSED"}]));
    }
    assert!(h.events.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn close_all_counts_its_own_connection_only() {
    let h = default_harness();
    let mut a = connect(h.port);
    a.handshake(&h, 1);
    let mut b = connect(h.port);
    b.handshake(&h, 2);
    a.request(5, "tools/call", open_diff_params(Path::new("/nonexistent/a"), "x", "t1"));
    let a1 = next_open_diff(&h);
    a.request(6, "tools/call", open_diff_params(Path::new("/nonexistent/a"), "x", "t2"));
    let a2 = next_open_diff(&h);
    b.request(5, "tools/call", open_diff_params(Path::new("/nonexistent/a"), "x", "t1"));
    let b1 = next_open_diff(&h);
    a.request(7, "tools/call", json!({"name":"closeAllDiffTabs","arguments":{}}));
    let mut frames: Vec<Value> = (0..3).map(|_| a.recv(Duration::from_secs(5)).unwrap()).collect();
    frames.sort_by_key(|f| f["id"].as_u64());
    assert_eq!(frames[0]["error"]["code"], -32000);
    assert_eq!(frames[1]["error"]["code"], -32000);
    assert_eq!(frames[2]["result"]["content"], json!([{"type":"text","text":"CLOSED_2_DIFF_TABS"}]));
    // One event carries both: the user's comments on them are delivered together.
    let withdrawn = match h.events.recv_timeout(Duration::from_secs(5)).unwrap() {
        BridgeEvent::DiffsWithdrawn { ids } => ids,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(withdrawn.len(), 2);
    assert!(withdrawn.contains(&a1.id) && withdrawn.contains(&a2.id));
    // B's proposal is untouched and still answerable.
    assert!(b.recv(Duration::from_millis(200)).is_none());
    h.bridge.answer_diff(&b1.id, DiffAnswer::Rejected).unwrap();
    assert_eq!(texts(&b.recv(Duration::from_secs(5)).unwrap())[0], "DIFF_REJECTED");
}

#[test]
fn superseded_by_same_tab_name_gets_error() {
    let h = default_harness();
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    c.request(5, "tools/call", open_diff_params(Path::new("/nonexistent/a"), "one", "t1"));
    let first = next_open_diff(&h);
    c.request(6, "tools/call", open_diff_params(Path::new("/nonexistent/a"), "two", "t1"));
    let second = next_open_diff(&h);
    assert_eq!(second.replaces, Some(first.id.clone()));
    let v = c.recv(Duration::from_secs(5)).unwrap();
    assert_eq!(v["id"], json!(5));
    assert_eq!(v["error"]["code"], -32000);
    assert_eq!(h.bridge.answer_diff(&first.id, DiffAnswer::Rejected), Err(Gone));
    h.bridge.answer_diff(&second.id, DiffAnswer::Saved("two".into())).unwrap();
    assert_eq!(c.recv(Duration::from_secs(5)).unwrap()["id"], json!(6));
}

#[test]
fn answer_after_withdrawal_is_gone() {
    let h = default_harness();
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    c.request(5, "tools/call", open_diff_params(Path::new("/nonexistent/a"), "x", "t1"));
    let opened = next_open_diff(&h);
    c.request(6, "tools/call", json!({"name":"close_tab","arguments":{"tab_name":"t1"}}));
    assert_eq!(c.recv(Duration::from_secs(5)).unwrap()["id"], json!(5));
    assert_eq!(c.recv(Duration::from_secs(5)).unwrap()["id"], json!(6));
    assert_eq!(h.bridge.answer_diff(&opened.id, DiffAnswer::Saved("x".into())), Err(Gone));
}

#[test]
fn second_answer_is_gone() {
    let h = default_harness();
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    c.request(5, "tools/call", open_diff_params(Path::new("/nonexistent/a"), "x", "t1"));
    let opened = next_open_diff(&h);
    assert_eq!(h.bridge.answer_diff(&opened.id, DiffAnswer::Rejected), Ok(()));
    assert_eq!(h.bridge.answer_diff(&opened.id, DiffAnswer::Saved("x".into())), Err(Gone));
}

#[test]
fn disconnect_drops_pending_and_emits_disconnected() {
    let h = default_harness();
    let mut announced = connect(h.port);
    let conn_a = announced.handshake(&h, 1);
    let mut silent = connect(h.port);
    announced.request(5, "tools/call", open_diff_params(Path::new("/nonexistent/a"), "x", "t1"));
    let a = next_open_diff(&h);
    assert_eq!(a.conn, conn_a);
    silent.request(5, "tools/call", open_diff_params(Path::new("/nonexistent/a"), "x", "t1"));
    let s = next_open_diff(&h);
    assert_ne!(s.conn, conn_a);
    drop(announced);
    drop(silent);
    let mut gone = Vec::new();
    for _ in 0..2 {
        match h.events.recv_timeout(Duration::from_secs(5)).unwrap() {
            BridgeEvent::Disconnected { conn } => gone.push(conn),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(gone.contains(&conn_a) && gone.contains(&s.conn));
    assert_eq!(h.bridge.answer_diff(&a.id, DiffAnswer::Rejected), Err(Gone));
    assert_eq!(h.bridge.answer_diff(&s.id, DiffAnswer::Rejected), Err(Gone));
}

/// Tiny deterministic generator, so the sequences are the same on every run.
struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// With no `answer_diff` call, nothing Sirio does may put a decision in an `openDiff` result.
/// The two clients live across sequences and are only now and then dropped or replaced (each
/// accept costs up to one acceptor poll), but every sequence ends by reading what its requests
/// were answered with.
#[test]
fn never_decides_without_answer() {
    let h = default_harness();
    let forbidden = ["FILE_SAVED", "DIFF_REJECTED", "TAB_CLOSED"];
    // Per slot: the client (if connected) and the ids of its openDiff requests.
    let mut slots: Vec<Option<(Client, Vec<u64>)>> = vec![None, None];
    let mut next_id = 10u64;
    let (mut answers_checked, mut withdrawals_checked) = (0u32, 0u32);
    for seq in 0..200u64 {
        let mut rng = Xorshift(0x9E37_79B9_7F4A_7C15 ^ (seq + 1).wrapping_mul(0xD1B5_4A32_D192_ED03));
        let mut checked: Vec<Value> = Vec::new();
        for _ in 0..(4 + rng.below(8)) {
            let s = rng.below(2) as usize;
            let op = rng.below(24);
            if slots[s].is_none() || op == 22 {
                // Reconnect: the old connection ends with whatever it still had pending.
                if let Some((mut old, ids)) = slots[s].take() {
                    checked.extend(drain(&mut old, &ids));
                }
                slots[s] = Some((connect(h.port), Vec::new()));
                continue;
            }
            if op == 23 {
                slots[s] = None; // disconnect with requests unanswered
                continue;
            }
            let (client, ids) = slots[s].as_mut().unwrap();
            next_id += 1;
            let tab_name = format!("tab{}", rng.below(3));
            match op {
                0..=9 => {
                    ids.push(next_id);
                    client.request(next_id, "tools/call",
                        open_diff_params(Path::new("/nonexistent/a"), "x", &tab_name));
                }
                10..=16 => client.request(next_id, "tools/call",
                    json!({"name":"close_tab","arguments":{"tab_name":tab_name}})),
                _ => client.request(next_id, "tools/call",
                    json!({"name":"closeAllDiffTabs","arguments":{}})),
            }
        }
        // The ids stay for the client's whole life: a proposal opened in one sequence is
        // closed, replaced or swept in a later one, and that frame must be checked too.
        for (client, ids) in slots.iter_mut().flatten() {
            checked.extend(drain(client, ids));
        }
        for frame in checked {
            answers_checked += 1;
            if frame["error"]["code"] == -32000 {
                withdrawals_checked += 1;
            }
            let text = frame.to_string();
            for word in forbidden {
                assert!(!text.contains(word), "sequence {seq}: an openDiff frame decided: {text}");
            }
        }
    }
    assert!(withdrawals_checked > 0, "the test never saw a withdrawn openDiff");
    println!("never_decides: {answers_checked} openDiff answer frames checked, {withdrawals_checked} of them -32000, {next_id} requests sent in all");
}

/// The frames answering `ids`, read up to a `ping` sent behind them (answers are in order, so
/// everything the server had to say about earlier requests precedes the ping's reply).
fn drain(client: &mut Client, ids: &[u64]) -> Vec<Value> {
    const PING: u64 = 9_000_000;
    client.request(PING, "ping", json!({}));
    let mut out = Vec::new();
    while let Some(frame) = client.recv(Duration::from_secs(5)) {
        if frame["id"] == json!(PING) {
            return out;
        }
        if frame["id"].as_u64().is_some_and(|id| ids.contains(&id)) {
            out.push(frame);
        }
    }
    panic!("no ping reply");
}

#[test]
fn ids_are_not_reused_across_bridges() {
    let first = default_harness();
    let mut a = connect(first.port);
    a.handshake(&first, 1);
    a.request(5, "tools/call", open_diff_params(Path::new("/nonexistent/a"), "x", "t1"));
    let old = next_open_diff(&first);
    first.bridge.shutdown();

    let second = default_harness();
    let mut b = connect(second.port);
    b.handshake(&second, 1);
    b.request(5, "tools/call", open_diff_params(Path::new("/nonexistent/a"), "x", "t1"));
    let new = next_open_diff(&second);
    assert_ne!(old.id, new.id);
    assert_ne!(old.conn, new.conn);
    assert_eq!(second.bridge.answer_diff(&old.id, DiffAnswer::Rejected), Err(Gone));
    assert!(b.recv(Duration::from_millis(200)).is_none(), "the stale id answered the new proposal");
    assert_eq!(second.bridge.answer_diff(&new.id, DiffAnswer::Rejected), Ok(()));
}

#[test]
fn relative_paths_are_refused() {
    let h = default_harness();
    let mut c = connect(h.port);
    c.handshake(&h, 1);
    for (old, new) in [("rel/a.txt", "/abs/a.txt"), ("/abs/a.txt", "rel/a.txt")] {
        let v = c.call(5, "tools/call", json!({"name":"openDiff","arguments":{
            "old_file_path": old, "new_file_path": new,
            "new_file_contents": "x", "tab_name": "t"}}));
        assert_eq!(v["error"]["code"], -32602);
    }
    assert!(h.events.recv_timeout(Duration::from_millis(200)).is_err());
}
