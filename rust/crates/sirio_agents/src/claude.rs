//! The Claude Code adapter, ported from `SirioAgents/ClaudeCodeAdapter.swift`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::error::PrepareError;
use crate::shell_quote::shell_quote;
use crate::{GlobalHookInstall, write_atomic};

/// Adapter for Anthropic's Claude Code CLI.
///
/// `prepare` writes a project-level `settings.local.json` under
/// `<worktree>/.claude/` with hooks that notify Sirio on lifecycle events.
/// Merge semantics replace ONLY the five hook event arrays (`Stop`,
/// `Notification`, `SessionStart`, `UserPromptSubmit`, `SessionEnd`) while
/// preserving every other key in the file. `install_global_hooks` applies
/// the same merge to the user's own `settings.json` — only ever on the
/// explicit Settings → Install Hooks action.
pub struct ClaudeCodeAdapter;

const SETTINGS_FILE_NAME: &str = "settings.local.json";
/// The user-global settings file under `~/.claude` (or `$CLAUDE_CONFIG_DIR`).
const GLOBAL_SETTINGS_FILE_NAME: &str = "settings.json";

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

    fn skill_markdown(&self) -> Option<&'static str> {
        // F-AGENT-SAFE-01: the bundled Sirio skill, marker included —
        // `install_skill` refuses to write it over a same-path file that
        // was not itself written by Sirio.
        Some(include_str!("../../../../skills/sirio/SKILL.md"))
    }

    fn prepare(
        &self,
        worktree_path: &str,
        pane_id: &str,
        sirioctl_path: &str,
    ) -> Result<(), PrepareError> {
        if let Some(markdown) = self.skill_markdown() {
            crate::install_skill(markdown, self.id(), worktree_path)?;
        }
        let claude_dir = Path::new(worktree_path).join(".claude");
        std::fs::create_dir_all(&claude_dir)?;
        merge_sirio_hooks(
            &claude_dir.join(SETTINGS_FILE_NAME),
            sirio_hooks(sirioctl_path, Some(pane_id)),
        )
    }

    fn install_global_hooks(
        &self,
        home: &Path,
        environment: &BTreeMap<String, String>,
        sirioctl_path: &str,
    ) -> Result<GlobalHookInstall, PrepareError> {
        // `$CLAUDE_CONFIG_DIR` relocates the whole config directory, the
        // same precedence `claude` itself applies (and `sirio_usage` reads).
        let config_dir = environment
            .get("CLAUDE_CONFIG_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude"));
        std::fs::create_dir_all(&config_dir)?;
        let settings_path = config_dir.join(GLOBAL_SETTINGS_FILE_NAME);
        merge_sirio_hooks(&settings_path, sirio_hooks(sirioctl_path, None))?;
        Ok(GlobalHookInstall::Written(settings_path))
    }

    fn command(&self, _worktree_path: &str, _pane_id: &str, _sirioctl_path: &str) -> String {
        // The pane already changes cwd to the worktree.
        "claude".to_string()
    }

    fn resume_command(
        &self,
        _worktree_path: &str,
        _pane_id: &str,
        _sirioctl_path: &str,
        session_ref: &str,
    ) -> Option<String> {
        Some(format!("claude --resume {}", shell_quote(session_ref)))
    }

    fn summarizer_command(&self, prompt: &str) -> Option<String> {
        Some(format!("claude -p {}", shell_quote(prompt)))
    }
}

/// The five hook arrays, each running `sirioctl notify` with the concrete
/// sirioctl path. `session` pins a pane — the worktree-local shape, one
/// file per launch; `None` leaves the pane to sirioctl's own
/// `SIRIO_PANE_ID` resolution — the user-global shape, one file for every
/// pane.
fn sirio_hooks(sirioctl_path: &str, session: Option<&str>) -> Value {
    let session = session
        .map(|pane| format!(" --session {pane}"))
        .unwrap_or_default();
    let command = |status: &str| {
        format!(
            "{} notify{session} --status {status} --stdin-json",
            shell_quote(sirioctl_path)
        )
    };
    let needs_input_cmd = command("needs-input");
    let running_cmd = command("running");
    let done_cmd = command("done");

    json!({
        // A session start (fresh launch or /compact-resume) lands Claude
        // on an idle prompt awaiting the user's first input — it is
        // waiting, not running. UserPromptSubmit flips it to running
        // when the user submits.
        "Stop": [{ "matcher": "", "hooks": [{ "type": "command", "command": needs_input_cmd }] }],
        "Notification": [{ "matcher": "", "hooks": [{ "type": "command", "command": needs_input_cmd }] }],
        "SessionStart": [{ "matcher": "", "hooks": [{ "type": "command", "command": needs_input_cmd }] }],
        "UserPromptSubmit": [{ "matcher": "", "hooks": [{ "type": "command", "command": running_cmd }] }],
        "SessionEnd": [{ "matcher": "", "hooks": [{ "type": "command", "command": done_cmd }] }],
    })
}

/// Merges `hooks` into the settings file at `settings_path`, replacing
/// ONLY those five event keys under `"hooks"` and preserving every other
/// key in the file. Like the Swift original: a file that is missing,
/// unreadable or not a JSON object is replaced wholesale.
fn merge_sirio_hooks(settings_path: &Path, hooks: Value) -> Result<(), PrepareError> {
    let mut root: Value = std::fs::read_to_string(settings_path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(|value| value.is_object())
        .unwrap_or_else(|| json!({}));

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
    write_atomic(settings_path, output.as_bytes())
}
