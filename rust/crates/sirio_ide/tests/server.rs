//! The server against a real `Bridge`, driven by a `tungstenite` client over loopback.

use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sirio_ide::{
    Bridge, BridgeEvent, BridgeOptions, ConnId, Diagnostic, FileDiagnostics,
    Notification, Pos16, Severity, bind,
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
