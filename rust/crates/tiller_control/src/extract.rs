//! Extracts an agent-native session reference from hook payloads, ported
//! from `TillerControl/AgentSessionExtractor.swift`. Pure functions — no
//! I/O — so `tillerctl` and tests share the same logic.

use serde_json::Value;

/// Keys agents use for their session identity, in priority order.
const SESSION_KEYS: &[&str] = &[
    "session_id",
    "session-id",
    "sessionId",
    "thread_id",
    "thread-id",
    "rollout_path",
    "rollout-path",
];

/// Parses a single JSON object (e.g. Claude Code hook stdin) and returns the
/// first known session key with a non-empty string value.
pub fn session_ref_from_json(data: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(data).ok()?;
    ref_in(&value)
}

/// Scans positional arguments (e.g. the Codex `notify` JSON payload) for the
/// first parseable JSON object containing a known session key.
pub fn session_ref_from_payload_arguments(args: &[String]) -> Option<String> {
    args.iter()
        .find_map(|arg| session_ref_from_json(arg.as_bytes()))
}

fn ref_in(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    for key in SESSION_KEYS {
        let Some(raw) = object.get(*key).and_then(|v| v.as_str()) else {
            continue;
        };
        if raw.is_empty() {
            continue;
        }
        // Rollout paths carry the id in the filename.
        if key.starts_with("rollout") {
            return rollout_id_from_path(raw);
        }
        return Some(raw.to_string());
    }
    None
}

/// `…/rollout-2026-07-08T12-00-00-<uuid>.jsonl` → `<uuid>` (lowercase).
fn rollout_id_from_path(path: &str) -> Option<String> {
    let name = path.rsplit('/').next()?;
    let stem = name.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;
    let parts: Vec<&str> = stem.split('-').collect();
    if parts.len() < 5 {
        return None;
    }
    let candidate = parts[parts.len() - 5..].join("-");
    // UUID-shaped (8-4-4-4-12 hex groups), like the Swift original's check.
    let groups: Vec<&str> = candidate.split('-').collect();
    let lengths = [8usize, 4, 4, 4, 12];
    if groups.len() == 5
        && groups.iter().zip(lengths).all(|(group, length)| {
            group.len() == length && group.bytes().all(|b| b.is_ascii_hexdigit())
        })
    {
        return Some(candidate.to_lowercase());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_session_id_from_claude_hook_payload() {
        let payload = br#"{"session_id": "abc-123", "transcript_path": "/x/y.jsonl"}"#;
        assert_eq!(session_ref_from_json(payload).as_deref(), Some("abc-123"));
    }

    #[test]
    fn extracts_thread_id_and_snake_case_keys() {
        assert_eq!(
            session_ref_from_json(br#"{"thread_id": "t-9"}"#).as_deref(),
            Some("t-9")
        );
        assert_eq!(
            session_ref_from_json(br#"{"session-id": "s-1"}"#).as_deref(),
            Some("s-1")
        );
    }

    #[test]
    fn extracts_uuid_from_rollout_path() {
        let uuid = "a1b2c3d4-e5f6-4a5b-9c8d-1e2f3a4b5c6d";
        let path = format!("/tmp/rollout-2026-07-08T12-00-00-{uuid}.jsonl");
        assert_eq!(rollout_id_from_path(&path).as_deref(), Some(uuid));
    }

    #[test]
    fn ignores_non_json_and_unknown_keys() {
        assert_eq!(session_ref_from_json(b"not json"), None);
        assert_eq!(session_ref_from_json(br#"{"other": "x"}"#), None);
        assert_eq!(
            session_ref_from_payload_arguments(&["plain".to_string()]),
            None
        );
    }

    #[test]
    fn scans_payload_arguments_in_order() {
        let args = vec![
            "not-json".to_string(),
            r#"{"session_id": "found"}"#.to_string(),
        ];
        assert_eq!(
            session_ref_from_payload_arguments(&args).as_deref(),
            Some("found")
        );
    }
}
