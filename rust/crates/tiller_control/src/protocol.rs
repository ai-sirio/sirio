//! The control protocol: request/response types, line framing, the rows
//! convention for list results, and the default socket path. Ported from
//! `TillerControl/ControlProtocol.swift` and `ControlRows.swift`.
//!
//! The wire format is one JSON object per line (NDJSON). Requests are
//! `{ "id", "method", "params" }`; responses are
//! `{ "id", "ok", "result"?, "error"? }` where `result`/`error` are omitted
//! when nil, exactly as Swift's synthesized Codable does. Keys are sorted,
//! matching the Swift encoder's `.sortedKeys`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A request from a client to the running Tiller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlRequest {
    /// Echoed back in the response.
    pub id: String,
    /// The method name, e.g. "notify" or "workspace.list".
    pub method: String,
    /// Method parameters; every value is a string.
    pub params: BTreeMap<String, String>,
}

/// The response to one request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlResponse {
    pub id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ControlResponse {
    /// A successful response with an (empty) result map.
    pub fn success(id: impl Into<String>, result: BTreeMap<String, String>) -> Self {
        Self {
            id: id.into(),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    /// A failed response carrying an error message.
    pub fn failure(id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ok: false,
            result: None,
            error: Some(error.into()),
        }
    }
}

/// Encodes a value as one JSON line: JSON bytes followed by `\n`.
pub fn encode_line<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let mut data = serde_json::to_vec(value)?;
    data.push(b'\n');
    Ok(data)
}

/// Decodes a request from one JSON line.
pub fn decode_request(line: &[u8]) -> Result<ControlRequest, serde_json::Error> {
    serde_json::from_slice(line)
}

/// Decodes a response from one JSON line.
pub fn decode_response(line: &[u8]) -> Result<ControlResponse, serde_json::Error> {
    serde_json::from_slice(line)
}

/// The result payload is a flat `String -> String` map. List responses embed
/// their rows as one JSON-array string under a single key — this is the
/// shared encoder/decoder for that convention.
pub mod rows {
    use std::collections::BTreeMap;

    /// Encodes rows as a JSON-array string, keys sorted.
    pub fn encode(rows: &[BTreeMap<String, String>]) -> String {
        serde_json::to_string(rows).unwrap_or_else(|_| "[]".to_string())
    }

    /// Decodes a JSON-array string back into rows.
    pub fn decode(json: &str) -> Option<Vec<BTreeMap<String, String>>> {
        serde_json::from_str(json).ok()
    }
}

/// The default control socket path: `$TILLER_SOCKET` if set, else
/// `~/Library/Application Support/Tiller/control.sock`.
pub fn default_socket_path(environment: &BTreeMap<String, String>) -> String {
    if let Some(override_path) = environment.get("TILLER_SOCKET") {
        return override_path.clone();
    }
    let home = environment
        .get("HOME")
        .cloned()
        .unwrap_or_else(|| "/".to_string());
    format!("{home}/Library/Application Support/Tiller/control.sock")
}

/// Builders for the control methods in scope, mirroring
/// `TillerctlRequestBuilder` from the Swift package. Every request gets a
/// fresh random id (Swift uses UUID strings; ids are opaque to the server).
pub mod request {
    use super::ControlRequest;
    use std::collections::BTreeMap;

    fn request(method: &str, params: BTreeMap<String, String>) -> ControlRequest {
        ControlRequest {
            id: fresh_id(),
            method: method.to_string(),
            params,
        }
    }

    fn fresh_id() -> String {
        // Random-ish id: process id + a counter + timestamp. Ids only need to
        // be unique enough to correlate responses; the server echoes them.
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        format!("{}-{n}-{nanos}", std::process::id())
    }

    /// Agent-status update; `agent_session` is included only when non-empty.
    pub fn notify(session: &str, status: &str, agent_session: Option<&str>) -> ControlRequest {
        let mut params = BTreeMap::new();
        params.insert("session".to_string(), session.to_string());
        params.insert("status".to_string(), status.to_string());
        if let Some(ref_value) = agent_session.filter(|r| !r.is_empty()) {
            params.insert("agentSession".to_string(), ref_value.to_string());
        }
        request("notify", params)
    }

    /// User-visible notification (cmux parity).
    pub fn notification_create(title: &str, subtitle: Option<&str>, body: &str) -> ControlRequest {
        let mut params = BTreeMap::new();
        params.insert("title".to_string(), title.to_string());
        if let Some(subtitle) = subtitle {
            params.insert("subtitle".to_string(), subtitle.to_string());
        }
        params.insert("body".to_string(), body.to_string());
        request("notification.create", params)
    }

    /// Report an agent-native session reference for a pane, without touching
    /// the status pipeline.
    pub fn session_ref(session: &str, ref_value: &str) -> ControlRequest {
        let mut params = BTreeMap::new();
        params.insert("session".to_string(), session.to_string());
        params.insert("ref".to_string(), ref_value.to_string());
        request("session.ref", params)
    }

    pub fn system_ping() -> ControlRequest {
        request("system.ping", BTreeMap::new())
    }

    pub fn system_capabilities() -> ControlRequest {
        request("system.capabilities", BTreeMap::new())
    }

    pub fn system_identify(worktree: Option<&str>, pane: Option<&str>) -> ControlRequest {
        let mut params = BTreeMap::new();
        if let Some(worktree) = worktree {
            params.insert("worktree".to_string(), worktree.to_string());
        }
        if let Some(pane) = pane {
            params.insert("pane".to_string(), pane.to_string());
        }
        request("system.identify", params)
    }

    pub fn workspace_list() -> ControlRequest {
        request("workspace.list", BTreeMap::new())
    }

    pub fn workspace_create(project: &str, branch: Option<&str>) -> ControlRequest {
        let mut params = BTreeMap::new();
        params.insert("project".to_string(), project.to_string());
        if let Some(branch) = branch {
            params.insert("branch".to_string(), branch.to_string());
        }
        request("workspace.create", params)
    }

    pub fn workspace_select(workspace: &str) -> ControlRequest {
        let mut params = BTreeMap::new();
        params.insert("workspace".to_string(), workspace.to_string());
        request("workspace.select", params)
    }

    pub fn workspace_current() -> ControlRequest {
        request("workspace.current", BTreeMap::new())
    }

    pub fn workspace_close(workspace: &str) -> ControlRequest {
        let mut params = BTreeMap::new();
        params.insert("workspace".to_string(), workspace.to_string());
        request("workspace.close", params)
    }

    pub fn notification_list() -> ControlRequest {
        request("notification.list", BTreeMap::new())
    }

    pub fn notification_clear() -> ControlRequest {
        request("notification.clear", BTreeMap::new())
    }
}
