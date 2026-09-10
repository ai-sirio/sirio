//! The control protocol: request/response types, line framing, the rows
//! convention for list results, and the default socket path. Ported from
//! `SirioControl/ControlProtocol.swift` and `ControlRows.swift`.
//!
//! The wire format is one JSON object per line (NDJSON). Requests are
//! `{ "id", "method", "params" }`; responses are
//! `{ "id", "ok", "result"?, "error"? }` where `result`/`error` are omitted
//! when nil, exactly as Swift's synthesized Codable does. Keys are sorted,
//! matching the Swift encoder's `.sortedKeys`.

use std::collections::BTreeMap;
#[allow(unused_imports)]
#[cfg(any(test, not(target_os = "macos")))]
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A request from a client to the running Sirio.
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

/// The default control socket path: `$SIRIO_SOCKET` if set, otherwise the
/// platform's private runtime location. Linux uses `$XDG_RUNTIME_DIR` and
/// falls back to the XDG state directory when no runtime directory exists.
/// Windows uses `%LOCALAPPDATA%` and never consults `HOME`/`XDG_*`, which
/// differ between PowerShell/Explorer and Git Bash and would otherwise fork
/// the pipe name per shell (#370).
pub fn default_socket_path(environment: &BTreeMap<String, String>) -> String {
    // `TILLER_SOCKET` is still honoured: agent hooks and shells started before
    // the rebrand carry it in their environment, and the socket is how they
    // reach the app at all. The new name wins when both are set.
    if let Some(override_path) = environment
        .get("SIRIO_SOCKET")
        .or_else(|| environment.get("TILLER_SOCKET"))
    {
        return override_path.clone();
    }

    #[cfg(target_os = "macos")]
    {
        let home = environment
            .get("HOME")
            .cloned()
            .unwrap_or_else(|| "/".to_string());
        return format!("{home}/Library/Application Support/Sirio/control.sock");
    }

    #[cfg(target_os = "windows")]
    {
        windows_default_socket_path(environment)
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let root = absolute_environment_path(environment, "XDG_RUNTIME_DIR")
            .unwrap_or_else(|| xdg_state_home(environment));
        root.join("Sirio")
            .join("control.sock")
            .to_string_lossy()
            .into_owned()
    }
}

/// Windows default, extracted pure so the precedence is testable on any
/// platform: `%LOCALAPPDATA%\Sirio\control.sock`, falling back to `%TEMP%`
/// / `%TMP%` and finally to a `HOME`-independent constant. `HOME` and every
/// `XDG_*` variable are ignored entirely: Git Bash exports a POSIX `HOME`
/// (`/c/Users/...`) while native launches have none, so consulting either
/// would put the app and `sirioctl` on different pipes (#370).
#[cfg(any(target_os = "windows", test))]
fn windows_default_socket_path(environment: &BTreeMap<String, String>) -> String {
    for key in ["LOCALAPPDATA", "TEMP", "TMP"] {
        if let Some(root) = environment
            .get(key)
            .map(Path::new)
            .filter(|path| windows_absolute(path))
            .map(Path::to_path_buf)
        {
            return root
                .join("Sirio")
                .join("control.sock")
                .to_string_lossy()
                .into_owned();
        }
    }
    "Sirio/control.sock".to_string()
}

/// Absolute-path check that also recognises drive-letter paths (`C:\...`)
/// when this helper runs on non-Windows test hosts, where `Path::is_absolute`
/// only knows POSIX roots and would otherwise misclassify every Windows
/// sample as relative.
#[cfg(any(target_os = "windows", test))]
fn windows_absolute(path: &Path) -> bool {
    if path.is_absolute() {
        return true;
    }
    let text = path.to_string_lossy();
    let bytes = text.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        return true;
    }
    text.starts_with(r"\\")
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn absolute_environment_path(environment: &BTreeMap<String, String>, key: &str) -> Option<PathBuf> {
    environment
        .get(key)
        .map(Path::new)
        .filter(|path| path.is_absolute())
        .map(Path::to_path_buf)
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn xdg_state_home(environment: &BTreeMap<String, String>) -> PathBuf {
    absolute_environment_path(environment, "XDG_STATE_HOME").unwrap_or_else(|| {
        let home =
            absolute_environment_path(environment, "HOME").unwrap_or_else(|| PathBuf::from("/tmp"));
        home.join(".local/state")
    })
}
/// Builders for the control methods in scope, mirroring
/// `SirioctlRequestBuilder` from the Swift package. Every request gets a
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

    pub fn session_restore() -> ControlRequest {
        request("session.restore", BTreeMap::new())
    }

    pub fn panel_create(worktree: Option<&str>, command: Option<&str>) -> ControlRequest {
        let mut params = BTreeMap::new();
        if let Some(worktree) = worktree {
            params.insert("worktree".to_string(), worktree.to_string());
        }
        if let Some(command) = command {
            params.insert("cmd".to_string(), command.to_string());
        }
        request("panel.create", params)
    }

    pub fn panel_split(from: &str, direction: &str, command: Option<&str>) -> ControlRequest {
        let mut params = BTreeMap::from([
            ("from".to_string(), from.to_string()),
            ("direction".to_string(), direction.to_string()),
        ]);
        if let Some(command) = command {
            params.insert("cmd".to_string(), command.to_string());
        }
        request("panel.split", params)
    }

    pub fn panel_list(worktree: Option<&str>) -> ControlRequest {
        let mut params = BTreeMap::new();
        if let Some(worktree) = worktree {
            params.insert("worktree".to_string(), worktree.to_string());
        }
        request("panel.list", params)
    }

    pub fn panel_write(id: &str, input: &str) -> ControlRequest {
        request(
            "panel.write",
            BTreeMap::from([
                ("id".to_string(), id.to_string()),
                ("input".to_string(), input.to_string()),
            ]),
        )
    }

    pub fn panel_key(id: &str, key: &str) -> ControlRequest {
        request(
            "panel.key",
            BTreeMap::from([
                ("id".to_string(), id.to_string()),
                ("key".to_string(), key.to_string()),
            ]),
        )
    }

    pub fn panel_read(id: &str) -> ControlRequest {
        request(
            "panel.read",
            BTreeMap::from([("id".to_string(), id.to_string())]),
        )
    }

    pub fn panel_state(id: &str) -> ControlRequest {
        request(
            "panel.state",
            BTreeMap::from([("id".to_string(), id.to_string())]),
        )
    }

    pub fn panel_scrollback(id: &str, max_bytes: Option<usize>) -> ControlRequest {
        let mut params = BTreeMap::from([("id".to_string(), id.to_string())]);
        if let Some(max_bytes) = max_bytes {
            params.insert("maxBytes".to_string(), max_bytes.to_string());
        }
        request("panel.scrollback", params)
    }

    pub fn panel_wait(id: &str, timeout_ms: Option<u64>) -> ControlRequest {
        let mut params = BTreeMap::from([("id".to_string(), id.to_string())]);
        if let Some(timeout_ms) = timeout_ms {
            params.insert("timeoutMs".to_string(), timeout_ms.to_string());
        }
        request("panel.wait", params)
    }

    pub fn panel_focus(id: &str) -> ControlRequest {
        request(
            "panel.focus",
            BTreeMap::from([("id".to_string(), id.to_string())]),
        )
    }

    pub fn panel_close(id: &str) -> ControlRequest {
        request(
            "panel.close",
            BTreeMap::from([("id".to_string(), id.to_string())]),
        )
    }

    pub fn pane_split(direction: &str) -> ControlRequest {
        request(
            "pane.split",
            BTreeMap::from([("direction".to_string(), direction.to_string())]),
        )
    }

    pub fn pane_focus(direction: &str) -> ControlRequest {
        request(
            "pane.focus",
            BTreeMap::from([("direction".to_string(), direction.to_string())]),
        )
    }

    pub fn pane_close() -> ControlRequest {
        request("pane.close", BTreeMap::new())
    }

    pub fn tab_cycle(forward: bool) -> ControlRequest {
        request(
            "tab.cycle",
            BTreeMap::from([(
                "direction".to_string(),
                if forward { "forward" } else { "backward" }.to_string(),
            )]),
        )
    }

    pub fn tab_select(position: usize) -> ControlRequest {
        request(
            "tab.select",
            BTreeMap::from([("index".to_string(), position.to_string())]),
        )
    }

    pub fn changes_open(worktree: Option<&str>) -> ControlRequest {
        let mut params = BTreeMap::new();
        if let Some(worktree) = worktree {
            params.insert("worktree".to_string(), worktree.to_string());
        }
        request("surface.changes.open", params)
    }

    pub fn changes_read() -> ControlRequest {
        request("surface.changes.read", BTreeMap::new())
    }

    fn changes_path_action(method: &str, path: &str, worktree: Option<&str>) -> ControlRequest {
        let mut params = BTreeMap::from([("path".to_string(), path.to_string())]);
        if let Some(worktree) = worktree {
            params.insert("worktree".to_string(), worktree.to_string());
        }
        request(method, params)
    }

    fn changes_all_action(method: &str, worktree: Option<&str>) -> ControlRequest {
        let mut params = BTreeMap::new();
        if let Some(worktree) = worktree {
            params.insert("worktree".to_string(), worktree.to_string());
        }
        request(method, params)
    }

    pub fn changes_stage(path: &str, worktree: Option<&str>) -> ControlRequest {
        changes_path_action("surface.changes.stage", path, worktree)
    }

    pub fn changes_unstage(path: &str, worktree: Option<&str>) -> ControlRequest {
        changes_path_action("surface.changes.unstage", path, worktree)
    }

    pub fn changes_discard(path: &str, worktree: Option<&str>) -> ControlRequest {
        changes_path_action("surface.changes.discard", path, worktree)
    }

    pub fn changes_stage_all(worktree: Option<&str>) -> ControlRequest {
        changes_all_action("surface.changes.stage_all", worktree)
    }

    pub fn changes_discard_all(worktree: Option<&str>) -> ControlRequest {
        changes_all_action("surface.changes.discard_all", worktree)
    }

    pub fn settings_open(section: Option<&str>) -> ControlRequest {
        let mut params = BTreeMap::new();
        if let Some(section) = section {
            params.insert("section".to_string(), section.to_string());
        }
        request("surface.settings.open", params)
    }

    pub fn settings_select(section: &str) -> ControlRequest {
        request(
            "surface.settings.select",
            BTreeMap::from([("section".to_string(), section.to_string())]),
        )
    }

    pub fn settings_read() -> ControlRequest {
        request("surface.settings.read", BTreeMap::new())
    }

    /// Opens the non-drawing chat surface for a worktree.
    pub fn chat_open(worktree: Option<&str>) -> ControlRequest {
        let mut params = BTreeMap::new();
        if let Some(worktree) = worktree {
            params.insert("worktree".to_string(), worktree.to_string());
        }
        request("surface.chat.open", params)
    }

    /// Submits text to a chat surface without waiting for the final response.
    pub fn chat_send(surface_id: &str, text: &str) -> ControlRequest {
        request(
            "surface.chat.send",
            BTreeMap::from([
                ("surfaceId".to_string(), surface_id.to_string()),
                ("text".to_string(), text.to_string()),
            ]),
        )
    }

    /// Changes the draft or queued text on a chat surface.
    pub fn chat_compose(surface_id: &str, text: &str) -> ControlRequest {
        request(
            "surface.chat.compose",
            BTreeMap::from([
                ("surfaceId".to_string(), surface_id.to_string()),
                ("text".to_string(), text.to_string()),
            ]),
        )
    }

    /// Resolves a permission card exposed by a live chat turn.
    pub fn chat_permission(surface_id: &str, request_id: u64, option_id: &str) -> ControlRequest {
        request(
            "surface.chat.permission",
            BTreeMap::from([
                ("surfaceId".to_string(), surface_id.to_string()),
                ("requestId".to_string(), request_id.to_string()),
                ("optionId".to_string(), option_id.to_string()),
            ]),
        )
    }

    /// Stops the in-flight turn, leaving readback to distinguish `stopped`
    /// from a normal `completed` outcome.
    pub fn chat_stop(surface_id: &str) -> ControlRequest {
        request(
            "surface.chat.stop",
            BTreeMap::from([("surfaceId".to_string(), surface_id.to_string())]),
        )
    }

    /// Reads the complete observable state of a chat surface.
    pub fn chat_read(surface_id: &str) -> ControlRequest {
        request(
            "surface.chat.read",
            BTreeMap::from([("surfaceId".to_string(), surface_id.to_string())]),
        )
    }

    pub fn system_ping() -> ControlRequest {
        request("system.ping", BTreeMap::new())
    }

    pub fn system_quit() -> ControlRequest {
        request("system.quit", BTreeMap::new())
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

    pub fn project_list() -> ControlRequest {
        request("project.list", BTreeMap::new())
    }

    pub fn project_add(path: &str) -> ControlRequest {
        request(
            "project.add",
            BTreeMap::from([(String::from("path"), path.to_string())]),
        )
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

    /// Associates runtime-only worktree metadata with an optional pane/session
    /// and comment. The control state is rebuilt on launch, so these values do
    /// not survive an application restart.
    pub fn worktree_set(
        worktree: &str,
        comment: Option<&str>,
        session: Option<&str>,
    ) -> ControlRequest {
        let mut params = BTreeMap::from([("worktree".to_string(), worktree.to_string())]);
        if let Some(comment) = comment {
            params.insert("comment".to_string(), comment.to_string());
        }
        if let Some(session) = session {
            params.insert("session".to_string(), session.to_string());
        }
        request("worktree.set", params)
    }

    pub fn notification_list() -> ControlRequest {
        request("notification.list", BTreeMap::new())
    }

    pub fn notification_clear() -> ControlRequest {
        request("notification.clear", BTreeMap::new())
    }
}

#[cfg(test)]
mod tests {
    use super::request;
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    use super::default_socket_path;
    #[cfg(any(target_os = "windows", test))]
    use super::windows_default_socket_path;
    use std::collections::BTreeMap;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_socket_prefers_runtime_dir_and_falls_back_to_state_dir() {
        let runtime = BTreeMap::from([
            ("HOME".to_string(), "/home/alice".to_string()),
            ("XDG_RUNTIME_DIR".to_string(), "/run/user/1000".to_string()),
        ]);
        assert_eq!(
            default_socket_path(&runtime),
            "/run/user/1000/Sirio/control.sock"
        );

        let fallback = BTreeMap::from([("HOME".to_string(), "/home/alice".to_string())]);
        assert_eq!(
            default_socket_path(&fallback),
            "/home/alice/.local/state/Sirio/control.sock"
        );
    }

    #[test]
    fn windows_default_ignores_home_and_xdg_and_uses_localappdata() {
        // Native PowerShell/Explorer (no HOME) and Git Bash (POSIX HOME)
        // must land on the same pipe when LOCALAPPDATA agrees (#370).
        let native = BTreeMap::from([(
            "LOCALAPPDATA".to_string(),
            r"C:\Users\alice\AppData\Local".to_string(),
        )]);
        let bash = BTreeMap::from([
            (
                "LOCALAPPDATA".to_string(),
                r"C:\Users\alice\AppData\Local".to_string(),
            ),
            ("HOME".to_string(), "/c/Users/alice".to_string()),
            (
                "XDG_RUNTIME_DIR".to_string(),
                "/run/user/1000".to_string(),
            ),
            ("XDG_STATE_HOME".to_string(), "/c/Users/alice/.local/state".to_string()),
        ]);
        let expected = r"C:\Users\alice\AppData\Local\Sirio\control.sock".replace(r"\", "/");
        let native_path = windows_default_socket_path(&native).replace(r"\", "/");
        let bash_path = windows_default_socket_path(&bash).replace(r"\", "/");
        assert_eq!(native_path, expected);
        assert_eq!(
            bash_path, expected,
            "HOME/XDG_* must not fork the Windows pipe name"
        );
    }

    #[test]
    fn windows_default_falls_back_without_home() {
        let temp = BTreeMap::from([(
            "TEMP".to_string(),
            r"C:\Users\alice\AppData\Local\Temp".to_string(),
        )]);
        let fallback = windows_default_socket_path(&temp).replace(r"\", "/");
        assert_eq!(
            fallback,
            "C:/Users/alice/AppData/Local/Temp/Sirio/control.sock"
        );

        let empty: BTreeMap<String, String> = BTreeMap::new();
        assert_eq!(windows_default_socket_path(&empty), "Sirio/control.sock");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_socket_path_is_stable_across_shell_environments() {
        let native = BTreeMap::from([(
            "LOCALAPPDATA".to_string(),
            r"C:\Users\alice\AppData\Local".to_string(),
        )]);
        let bash = BTreeMap::from([
            (
                "LOCALAPPDATA".to_string(),
                r"C:\Users\alice\AppData\Local".to_string(),
            ),
            ("HOME".to_string(), "/c/Users/alice".to_string()),
        ]);
        assert_eq!(default_socket_path(&native), default_socket_path(&bash));

        let override_env = BTreeMap::from([
            (
                "LOCALAPPDATA".to_string(),
                r"C:\Users\alice\AppData\Local".to_string(),
            ),
            (
                "SIRIO_SOCKET".to_string(),
                r"\\.\pipe\custom".to_string(),
            ),
        ]);
        assert_eq!(default_socket_path(&override_env), r"\\.\pipe\custom");
    }

    #[test]
    fn pane_request_builders_use_the_wire_method_names_and_parameters() {
        let create = request::panel_create(Some("worktree-1"), Some("printf ready"));
        assert_eq!(create.method, "panel.create");
        assert_eq!(
            create.params.get("worktree").map(String::as_str),
            Some("worktree-1")
        );
        assert_eq!(
            create.params.get("cmd").map(String::as_str),
            Some("printf ready")
        );

        let split = request::panel_split("pane-1", "right", None);
        assert_eq!(split.method, "panel.split");
        assert_eq!(split.params.get("from").map(String::as_str), Some("pane-1"));
        assert_eq!(
            split.params.get("direction").map(String::as_str),
            Some("right")
        );
        assert!(!split.params.contains_key("cmd"));

        let wait = request::panel_wait("pane-1", Some(2500));
        assert_eq!(wait.method, "panel.wait");
        assert_eq!(
            wait.params.get("timeoutMs").map(String::as_str),
            Some("2500")
        );

        assert_eq!(
            request::panel_write("pane-1", "hello").method,
            "panel.write"
        );
        assert_eq!(request::panel_key("pane-1", "enter").method, "panel.key");
        assert_eq!(request::panel_read("pane-1").method, "panel.read");
        assert_eq!(request::panel_state("pane-1").method, "panel.state");
        let scrollback = request::panel_scrollback("pane-1", Some(4096));
        assert_eq!(scrollback.method, "panel.scrollback");
        assert_eq!(
            scrollback.params.get("maxBytes").map(String::as_str),
            Some("4096")
        );
        assert_eq!(request::panel_focus("pane-1").method, "panel.focus");
        assert_eq!(request::panel_close("pane-1").method, "panel.close");

        let split = request::pane_split("right");
        assert_eq!(split.method, "pane.split");
        assert_eq!(
            split.params.get("direction").map(String::as_str),
            Some("right")
        );

        let focus = request::pane_focus("below");
        assert_eq!(focus.method, "pane.focus");
        assert_eq!(
            focus.params.get("direction").map(String::as_str),
            Some("below")
        );

        assert_eq!(request::pane_close().method, "pane.close");
        assert_eq!(request::tab_cycle(true).method, "tab.cycle");
        assert_eq!(
            request::tab_cycle(false)
                .params
                .get("direction")
                .map(String::as_str),
            Some("backward")
        );
        let select = request::tab_select(9);
        assert_eq!(select.method, "tab.select");
        assert_eq!(select.params.get("index").map(String::as_str), Some("9"));

        assert_eq!(
            request::changes_open(Some("wt-1")).method,
            "surface.changes.open"
        );
        assert_eq!(
            request::changes_open(Some("wt-1"))
                .params
                .get("worktree")
                .map(String::as_str),
            Some("wt-1")
        );
        assert_eq!(request::changes_read().method, "surface.changes.read");
        let stage = request::changes_stage("f.txt", Some("wt-1"));
        assert_eq!(stage.method, "surface.changes.stage");
        assert_eq!(stage.params.get("path").map(String::as_str), Some("f.txt"));
        assert_eq!(
            stage.params.get("worktree").map(String::as_str),
            Some("wt-1")
        );
        assert_eq!(
            request::changes_unstage("f.txt", None).method,
            "surface.changes.unstage"
        );
        assert_eq!(
            request::changes_discard("f.txt", None).method,
            "surface.changes.discard"
        );
        assert_eq!(
            request::changes_stage_all(None).method,
            "surface.changes.stage_all"
        );
        assert_eq!(
            request::changes_discard_all(Some("wt-1"))
                .params
                .get("worktree")
                .map(String::as_str),
            Some("wt-1")
        );
        assert_eq!(
            request::settings_open(Some("Agents")).method,
            "surface.settings.open"
        );
        assert_eq!(
            request::settings_select("Agents").method,
            "surface.settings.select"
        );
        assert_eq!(request::settings_read().method, "surface.settings.read");
    }

    #[test]
    fn automation_request_builders_use_the_documented_wire_methods() {
        let set = request::worktree_set("wt-1", Some("agent pane"), Some("pane-1"));
        assert_eq!(set.method, "worktree.set");
        assert_eq!(set.params.get("worktree").map(String::as_str), Some("wt-1"));
        assert_eq!(
            set.params.get("comment").map(String::as_str),
            Some("agent pane")
        );
        assert_eq!(
            set.params.get("session").map(String::as_str),
            Some("pane-1")
        );

        let clear = request::worktree_set("/tmp/wt", None, None);
        assert_eq!(clear.method, "worktree.set");
        assert_eq!(clear.params.len(), 1);
    }

    #[test]
    fn project_request_builders_use_resource_method_names() {
        assert_eq!(request::project_list().method, "project.list");

        let add = request::project_add("/tmp/sirio-fixture");
        assert_eq!(add.method, "project.add");
        assert_eq!(
            add.params.get("path").map(String::as_str),
            Some("/tmp/sirio-fixture")
        );
    }

    #[test]
    fn chat_request_builders_use_observable_surface_methods() {
        let open = request::chat_open(Some("wt-1"));
        assert_eq!(open.method, "surface.chat.open");
        assert_eq!(
            open.params.get("worktree").map(String::as_str),
            Some("wt-1")
        );

        let send = request::chat_send("chat-1", "hello");
        assert_eq!(send.method, "surface.chat.send");
        assert_eq!(
            send.params.get("surfaceId").map(String::as_str),
            Some("chat-1")
        );
        assert_eq!(send.params.get("text").map(String::as_str), Some("hello"));

        let compose = request::chat_compose("chat-1", "draft");
        assert_eq!(compose.method, "surface.chat.compose");
        assert_eq!(
            compose.params.get("surfaceId").map(String::as_str),
            Some("chat-1")
        );

        let permission = request::chat_permission("chat-1", 7, "deny");
        assert_eq!(permission.method, "surface.chat.permission");
        assert_eq!(
            permission.params.get("requestId").map(String::as_str),
            Some("7")
        );
        assert_eq!(
            permission.params.get("optionId").map(String::as_str),
            Some("deny")
        );

        assert_eq!(request::chat_stop("chat-1").method, "surface.chat.stop");
        assert_eq!(request::chat_read("chat-1").method, "surface.chat.read");
    }
}
