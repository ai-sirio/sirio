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
    /// With `true`, `env` is the session's whole environment and the host's own
    /// variables are not added to it. A host without `term.env_exact` ignores
    /// this field and extends its own environment with `env`, so a client sends
    /// `true` only after the capability is in the host's `Welcome`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub env_exact: bool,
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

/// Encode bytes using the standard padded Base64 alphabet used on the wire.
pub fn b64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(ALPHABET[(first >> 2) as usize] as char);
        output.push(ALPHABET[((first & 0x03) << 4 | second >> 4) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[((second & 0x0f) << 2 | third >> 6) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(third & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

/// Decode standard padded Base64, rejecting non-alphabet bytes and malformed
/// or non-canonical padding.
pub fn b64_decode(encoded: &str) -> Option<Vec<u8>> {
    let bytes = encoded.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut decoded = Vec::with_capacity(bytes.len() / 4 * 3);
    let chunks = bytes.as_chunks::<4>().0;
    for (index, chunk) in chunks.iter().enumerate() {
        let last = index + 1 == chunks.len();
        let first = alphabet_value(chunk[0])?;
        let second = alphabet_value(chunk[1])?;
        match (chunk[2], chunk[3]) {
            (b'=', b'=') if last && second & 0x0f == 0 => {
                decoded.push((first << 2) | (second >> 4));
            }
            (third, b'=') if last => {
                let third = alphabet_value(third)?;
                if third & 0x03 != 0 {
                    return None;
                }
                decoded.push((first << 2) | (second >> 4));
                decoded.push((second << 4) | (third >> 2));
            }
            (third, fourth) => {
                let third = alphabet_value(third)?;
                let fourth = alphabet_value(fourth)?;
                decoded.push((first << 2) | (second >> 4));
                decoded.push((second << 4) | (third >> 2));
                decoded.push((third << 6) | fourth);
            }
        }
    }
    Some(decoded)
}

fn alphabet_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
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

    #[test]
    fn env_exact_false_is_not_serialized() {
        let mut params = CreateParams {
            session_id: None,
            cwd: "/".into(),
            command: TermCommand::Shell,
            env: Default::default(),
            size: TermSize { cols: 80, rows: 24 },
            origin: Origin::Control,
            parent: None,
            prelude: None,
            env_exact: false,
        };
        let unset = serde_json::to_value(&params).unwrap();
        assert!(unset.get("env_exact").is_none(), "{unset}");
        assert!(!serde_json::from_value::<CreateParams>(unset).unwrap().env_exact);

        params.env_exact = true;
        let set = serde_json::to_value(&params).unwrap();
        assert_eq!(set["env_exact"], true);
    }

    #[test]
    fn base64_round_trips_empty_and_each_tail_length() {
        for bytes in [b"".as_slice(), b"f", b"fo", b"foo", b"foob", b"fooba"] {
            let encoded = b64_encode(bytes);
            assert_eq!(b64_decode(&encoded).as_deref(), Some(bytes));
        }
        assert_eq!(b64_encode(b""), "");
        assert_eq!(b64_encode(b"f"), "Zg==");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
        assert_eq!(b64_encode(b"foo"), "Zm9v");
    }

    #[test]
    fn base64_rejects_invalid_characters_and_padding() {
        for invalid in ["?AAA", "AA A", "AA\nA", "=AAA", "A===", "AA=A"] {
            assert_eq!(b64_decode(invalid), None, "{invalid:?}");
        }
        for invalid in ["Zg", "Zg=", "Zg===", "Zm=8", "Zm9="] {
            assert_eq!(b64_decode(invalid), None, "{invalid:?}");
        }
    }
}
