//! The Claude Code adapter, ported from `TillerAgents/ClaudeCodeAdapter.swift`.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use crate::error::PrepareError;
use crate::shell_quote::shell_quote;

/// Adapter for Anthropic's Claude Code CLI.
///
/// `prepare` writes a project-level `settings.local.json` under
/// `<worktree>/.claude/` with hooks that notify Tiller on lifecycle events.
/// Merge semantics replace ONLY the five hook event arrays (`Stop`,
/// `Notification`, `SessionStart`, `UserPromptSubmit`, `SessionEnd`) while
/// preserving every other key in the file.
pub struct ClaudeCodeAdapter;

const SETTINGS_FILE_NAME: &str = "settings.local.json";

impl super::AgentAdapter for ClaudeCodeAdapter {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn has_native_hooks(&self) -> bool {
        true
    }

    fn prepare(
        &self,
        worktree_path: &str,
        pane_id: &str,
        tillerctl_path: &str,
    ) -> Result<(), PrepareError> {
        let claude_dir = Path::new(worktree_path).join(".claude");
        std::fs::create_dir_all(&claude_dir)?;

        let settings_path = claude_dir.join(SETTINGS_FILE_NAME);

        // Build the five hook arrays with the concrete tillerctl path and
        // pane id.
        let needs_input_cmd = format!(
            "{} notify --session {} --status needs-input --stdin-json",
            shell_quote(tillerctl_path),
            pane_id
        );
        let running_cmd = format!(
            "{} notify --session {} --status running --stdin-json",
            shell_quote(tillerctl_path),
            pane_id
        );
        let done_cmd = format!(
            "{} notify --session {} --status done --stdin-json",
            shell_quote(tillerctl_path),
            pane_id
        );

        let hooks = json!({
            // A session start (fresh launch or /compact-resume) lands Claude
            // on an idle prompt awaiting the user's first input — it is
            // waiting, not running. UserPromptSubmit flips it to running
            // when the user submits.
            "Stop": [{ "matcher": "", "hooks": [{ "type": "command", "command": needs_input_cmd }] }],
            "Notification": [{ "matcher": "", "hooks": [{ "type": "command", "command": needs_input_cmd }] }],
            "SessionStart": [{ "matcher": "", "hooks": [{ "type": "command", "command": needs_input_cmd }] }],
            "UserPromptSubmit": [{ "matcher": "", "hooks": [{ "type": "command", "command": running_cmd }] }],
            "SessionEnd": [{ "matcher": "", "hooks": [{ "type": "command", "command": done_cmd }] }],
        });

        // Merge with the existing file if present. Like the Swift original:
        // a file that is missing, unreadable or not a JSON object is
        // replaced wholesale.
        let mut root: Value = std::fs::read_to_string(&settings_path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .filter(|value| value.is_object())
            .unwrap_or_else(|| json!({}));

        // Replace the five keys under "hooks"; preserve all other hooks keys.
        match root.get_mut("hooks") {
            Some(Value::Object(existing_hooks)) => {
                if let Value::Object(new_hooks) = hooks {
                    for (key, value) in new_hooks {
                        existing_hooks.insert(key, value);
                    }
                }
            }
            _ => {
                root["hooks"] = hooks;
            }
        }

        // serde_json's default map is a BTreeMap, so keys serialize sorted —
        // matching `JSONSerialization`'s `.prettyPrinted, .sortedKeys`.
        let output = serde_json::to_string_pretty(&root)?;
        write_atomic(&settings_path, output.as_bytes())?;
        Ok(())
    }

    fn command(&self, _worktree_path: &str, _pane_id: &str, _tillerctl_path: &str) -> String {
        // The pane already changes cwd to the worktree.
        "claude".to_string()
    }

    fn resume_command(
        &self,
        _worktree_path: &str,
        _pane_id: &str,
        _tillerctl_path: &str,
        session_ref: &str,
    ) -> Option<String> {
        Some(format!("claude --resume {}", shell_quote(session_ref)))
    }

    fn acp_program(&self) -> Option<crate::AcpProgram> {
        // Anthropic's official ACP wrapper, the same package the chat
        // default already launches. `@latest` matches the tree's
        // convention; the ACP registry pins versions for reproducible
        // installs.
        Some(crate::AcpProgram::new(
            "npx",
            &["-y", "@agentclientprotocol/claude-agent-acp@latest"],
        ))
    }
}

/// Writes `contents` to `path` atomically: a unique temp file in the same
/// directory is renamed over the target, so a crash or a concurrent prepare
/// can never leave a torn settings file.
fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), PrepareError> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("json.tmp-{}-{unique}", std::process::id()));
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
