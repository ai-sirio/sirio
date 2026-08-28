//! Safe migration of stale Claude hook command paths.

use std::path::Path;

use serde_json::Value;

use crate::shell_quote::shell_quote;

/// Repairs only the leading executable path in Claude's worktree-local hook
/// commands. Everything else in the JSON document remains data owned by the
/// user and is preserved.
pub struct ClaudeHookMigrator;

impl ClaudeHookMigrator {
    /// Returns serialized settings when at least one stale `sirioctl` path
    /// changed. Invalid JSON and already-current settings are no-ops.
    pub fn rewritten_settings(data: impl AsRef<[u8]>, sirioctl_path: &str) -> Option<Vec<u8>> {
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
                    let Some(rewritten) = rewrite_command(command, sirioctl_path) else {
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
    pub fn migrate_file(path: &Path, sirioctl_path: &str) -> bool {
        let Ok(data) = std::fs::read(path) else {
            return false;
        };
        let Some(rewritten) = Self::rewritten_settings(data, sirioctl_path) else {
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

/// Whether a quoted leading token names our control CLI by path.
///
/// A separator is required, so a bare `sirioctl` typed by the user is left
/// alone — only a path we could have written is a migration candidate. The
/// executable name is matched with and without the `.exe` the Windows
/// toolchain emits, because a hook file is on disk from whenever it was
/// written and may name either.
///
/// Which characters separate is per-platform on purpose: `\` is a legal
/// character in a unix file name, so splitting on it there would misread
/// `foo\sirioctl` — one file — as a path ending in `sirioctl`.
fn names_sirioctl(path: &str) -> bool {
    #[cfg(windows)]
    let separators: &[char] = &['/', '\\'];
    #[cfg(not(windows))]
    let separators: &[char] = &['/'];

    let Some(index) = path.rfind(separators) else {
        return false;
    };
    // The pre-rename name stays matched. Worktrees prepared before the rebrand
    // carry hooks pointing at a `tillerctl` that no longer exists, and this
    // rewrite is the only thing that repairs them — stop recognising the old
    // basename and those panes lose their Layer-A notifications silently.
    matches!(
        &path[index + 1..],
        "sirioctl" | "sirioctl.exe" | "tillerctl" | "tillerctl.exe"
    )
}

/// Reads permissively, writes in the current form.
///
/// Reading has to be permissive because the whole point of this migrator is
/// files written by an OLDER build: on Windows those carry the POSIX
/// single-quoted form, since `shell_quote` only learned the cmd form later,
/// and their paths end in `\sirioctl.exe` rather than `/sirioctl`. A parser
/// that accepted only what the CURRENT build emits would have skipped every
/// stale Windows entry — the exact files it exists to repair — and the
/// migration would have been a silent no-op there.
///
/// Writing stays strict: the replacement is quoted by [`shell_quote`], which
/// is the form the shell that will run this hook understands today.
fn rewrite_command(command: &str, sirioctl_path: &str) -> Option<String> {
    // Either quote character opens the token; the same one must close it.
    let (quote, rest) = ['\'', '"']
        .into_iter()
        .find_map(|quote| command.strip_prefix(quote).map(|rest| (quote, rest)))?;
    let closing = rest.find(quote)?;
    let old_path = &rest[..closing];
    if !names_sirioctl(old_path) || old_path == sirioctl_path {
        return None;
    }
    Some(format!(
        "{}{}",
        shell_quote(sirioctl_path),
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
    fn only_a_quoted_sirioctl_path_is_rewritten() {
        assert_eq!(
            rewrite_command("'/old/sirioctl' notify --session pane", "/new/sirioctl"),
            Some("'/new/sirioctl' notify --session pane".to_string())
        );
        assert_eq!(rewrite_command("/usr/bin/echo done", "/new/sirioctl"), None);
        assert_eq!(
            rewrite_command("'/new/sirioctl' notify", "/new/sirioctl"),
            None
        );
    }

    /// Windows arm. The replacement is cmd-quoted because that is what will
    /// run the hook today; what varies between cases is the STALE entry being
    /// read, and all four shapes below are ones a real hook file can hold.
    #[cfg(windows)]
    #[test]
    fn only_a_quoted_sirioctl_path_is_rewritten() {
        let current = r"C:\Users\me\AppData\Local\TillerRust\bin\sirioctl.exe";

        // The shape this migrator will actually meet on Windows: an entry
        // written by a current build — double-quoted, backslashes, `.exe` —
        // whose install path has since moved. Before the parser learned to
        // read its own output, this was skipped and Layer A stayed broken
        // with no way to repair itself.
        assert_eq!(
            rewrite_command(
                &format!("\"{}\" notify --session pane", r"C:\old\bin\sirioctl.exe"),
                current
            ),
            Some(format!("\"{current}\" notify --session pane"))
        );

        // Written by an OLDER Windows build, before `shell_quote` had a cmd
        // form: single quotes and a POSIX-looking path. Still ours, still
        // stale, still repairable.
        assert_eq!(
            rewrite_command("'/old/sirioctl' notify --session pane", current),
            Some(format!("\"{current}\" notify --session pane"))
        );

        // Not ours, and already current: both left alone.
        assert_eq!(
            rewrite_command("C:\\Windows\\System32\\cmd.exe /C echo", current),
            None
        );
        assert_eq!(
            rewrite_command(&format!("\"{current}\" notify"), current),
            None
        );
        // A bare name is a command the user typed, not a path we wrote.
        assert_eq!(rewrite_command("'sirioctl' notify", current), None);
    }
}
