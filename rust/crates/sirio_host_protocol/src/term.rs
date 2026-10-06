//! The terminal-session messages (SP2 spec §3). Pure types.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `pane-<decimal>` (the app's pane key) or `pane-c<12 hex>` (host-assigned).
pub fn is_valid_session_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("pane-") else { return false };
    if let Some(hex) = rest.strip_prefix('c') {
        hex.len() == 12 && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    } else {
        (1..=18).contains(&rest.len()) && rest.bytes().all(|b| b.is_ascii_digit())
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
pub struct TermSize { pub cols: u16, pub rows: u16 }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TermCommand {
    Shell,
    Program { program: String, #[serde(default)] args: Vec<String> },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Origin { App, Control, #[serde(other)] Other }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct CreateParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub cwd: String,
    pub command: TermCommand,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    pub size: TermSize,
    pub origin: Origin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Base64; fed into the VT and the journal before the child's first byte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prelude: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Created { pub session_id: String, pub pid: u32 }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct AttachParams {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_from: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AttachMode { Snapshot, Resume, #[serde(other)] Other }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Attached { pub stream: u32, pub size: TermSize, pub mode: AttachMode }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct DetachParams { pub stream: u32 }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct ResizeParams { pub session_id: String, pub cols: u16, pub rows: u16 }

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReadFormat { Text, Raw, #[serde(other)] Other }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct ReadParams {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<ReadFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct ReadResult {
    pub format: ReadFormat,
    /// UTF-8 text for `text`; base64 for `raw`.
    pub data: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct KeyParams { pub session_id: String, pub key: String }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct BytesParams {
    pub session_id: String,
    /// Base64.
    pub data: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct WaitParams { pub session_id: String, pub timeout_ms: u64 }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExitStatus { Success, Code { code: i32 }, Unknown, #[serde(other)] Other }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct WaitResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<ExitStatus>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct SessionParams { pub session_id: String }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Foreground { pub running: bool }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct SessionSummary {
    pub session_id: String,
    pub pid: u32,
    pub cwd: String,
    pub title: String,
    pub size: TermSize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<ExitStatus>,
    pub foreground: Foreground,
    pub origin: Origin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub clients: u32,
    /// Seconds since the Unix epoch.
    pub created_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct SessionList { pub sessions: Vec<SessionSummary> }

/// The payload of a `term.sessions` event.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    Created { session_id: String },
    Exited { session_id: String, status: ExitStatus },
    Title { session_id: String, title: String },
    Foreground { session_id: String, running: bool },
    Closed { session_id: String },
    /// Sent on the stream's own connection: the host detached `stream`
    /// because its queue filled; re-attach for a snapshot.
    Lagged { stream: u32 },
    /// Sent on the stream's own connection: the host is about to send this
    /// stream a fresh snapshot (the alternate screen was left).
    Resync { stream: u32 },
    #[serde(other)]
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_have_two_shapes_and_nothing_else() {
        for ok in ["pane-1", "pane-0", "pane-123456789012345678", "pane-c0123456789ab", "pane-cabcdefabcdef"] {
            assert!(is_valid_session_id(ok), "{ok}");
        }
        for bad in [
            "", "pane-", "pane-c", "pane-1234567890123456789", "pane-cABCDEF012345",
            "pane-c0123456789a", "pane-c0123456789abc", "pane--1", "pane-1a", "Pane-1",
            "pane-12-3", "pane-c0123456789ag", "tab-1", "pane- 1",
        ] {
            assert!(!is_valid_session_id(bad), "{bad}");
        }
    }

    #[test]
    fn an_unknown_event_or_mode_degrades_to_other() {
        let e: SessionEvent = serde_json::from_str(r#"{"type":"migrated","session_id":"pane-1"}"#).unwrap();
        assert_eq!(e, SessionEvent::Other);
        let m: AttachMode = serde_json::from_str(r#""replay""#).unwrap();
        assert_eq!(m, AttachMode::Other);
        let o: Origin = serde_json::from_str(r#""remote""#).unwrap();
        assert_eq!(o, Origin::Other);
    }

    #[test]
    fn a_create_without_optional_fields_decodes() {
        let p: CreateParams = serde_json::from_str(
            r#"{"cwd":"/","command":{"type":"shell"},"size":{"cols":80,"rows":24},"origin":"control"}"#,
        )
        .unwrap();
        assert_eq!((p.session_id, p.prelude, p.env.len()), (None, None, 0));
    }
}
