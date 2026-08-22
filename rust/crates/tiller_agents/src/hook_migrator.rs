//! Safe migration of stale Claude hook command paths.

use std::path::Path;

use serde_json::Value;

use crate::shell_quote::shell_quote;

/// Repairs only the leading executable path in Claude's worktree-local hook
/// commands. Everything else in the JSON document remains data owned by the
/// user and is preserved.
pub struct ClaudeHookMigrator;

impl ClaudeHookMigrator {
    /// Returns serialized settings when at least one stale `tillerctl` path
    /// changed. Invalid JSON and already-current settings are no-ops.
    pub fn rewritten_settings(data: impl AsRef<[u8]>, tillerctl_path: &str) -> Option<Vec<u8>> {
        let mut root: Value = serde_json::from_slice(data.as_ref()).ok()?;
        let hooks = root.get_mut("hooks")?.as_object_mut()?;
        let mut changed = false;

        for entries in hooks.values_mut() {
            let Some(entries) = entries.as_array_mut() else {
                continue;
            };
            for entry in entries {
                let Some(inner_hooks) = entry.get_mut("hooks").and_then(Value::as_array_mut) else {
                    continue;
                };
                for hook in inner_hooks {
                    let Some(command) = hook.get("command").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(rewritten) = rewrite_command(command, tillerctl_path) else {
                        continue;
                    };
                    hook["command"] = Value::String(rewritten);
                    changed = true;
                }
            }
        }

        changed
            .then(|| serde_json::to_vec_pretty(&root).ok())
            .flatten()
    }

    /// Rewrites a settings file atomically and reports whether it changed.
    /// Missing, malformed, and already-current files are no-ops.
    pub fn migrate_file(path: &Path, tillerctl_path: &str) -> bool {
        let Ok(data) = std::fs::read(path) else {
            return false;
        };
        let Some(rewritten) = Self::rewritten_settings(data, tillerctl_path) else {
            return false;
        };

        let temporary = path.with_extension(format!(
            "json.tmp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ));
        if std::fs::write(&temporary, rewritten).is_err() {
            return false;
        }
        if std::fs::rename(&temporary, path).is_err() {
            let _ = std::fs::remove_file(temporary);
            return false;
        }
        true
    }
}

fn rewrite_command(command: &str, tillerctl_path: &str) -> Option<String> {
    let rest = command.strip_prefix('\'')?;
    let closing = rest.find('\'')?;
    let old_path = &rest[..closing];
    if !old_path.ends_with("/tillerctl") || old_path == tillerctl_path {
        return None;
    }
    Some(format!(
        "{}{}",
        shell_quote(tillerctl_path),
        &rest[closing + 1..]
    ))
}

#[cfg(test)]
mod tests {
    use super::rewrite_command;

    /// The migrator's contract is spelled in the POSIX form the Swift
    /// original wrote — it runs unchanged on unix.
    #[cfg(not(windows))]
    #[test]
    fn only_a_quoted_tillerctl_path_is_rewritten() {
        assert_eq!(
            rewrite_command("'/old/tillerctl' notify --session pane", "/new/tillerctl"),
            Some("'/new/tillerctl' notify --session pane".to_string())
        );
        assert_eq!(
            rewrite_command("/usr/bin/echo done", "/new/tillerctl"),
            None
        );
        assert_eq!(
            rewrite_command("'/new/tillerctl' notify", "/new/tillerctl"),
            None
        );
    }

    /// On Windows the rewritten hook command is cmd.exe-quoted, so the
    /// expected output is the double-quote form. Windows paths end in
    /// `tillerctl.exe`, which `ends_with("/tillerctl")` cannot match — the
    /// migrator only ever rewrites POSIX-style stale paths, which is exactly
    /// the kind of file this migrator exists to fix.
    #[cfg(windows)]
    #[test]
    fn only_a_quoted_tillerctl_path_is_rewritten() {
        assert_eq!(
            rewrite_command("'/old/tillerctl' notify --session pane", "/new/tillerctl"),
            Some("\"/new/tillerctl\" notify --session pane".to_string())
        );
    }
}
