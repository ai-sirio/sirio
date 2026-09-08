//! A Finder or Dock launch inherits launchd's minimal PATH
//! (`/usr/bin:/bin:/usr/sbin:/sbin`), not the login shell's. Agent discovery,
//! the ACP servers (npm `#!/usr/bin/env node` shims) and the npm installer all
//! resolve binaries through the process environment, so under that PATH every
//! CLI reads "Not found on PATH" and every ACP server dies at `initialize`.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::{Command, Stdio};

const BEGIN: &str = "SIRIO_PATH_BEGIN";
const END: &str = "SIRIO_PATH_END";

/// Merge the login shell's PATH into this process's environment.
///
/// Must run before any thread exists: `set_var` is only sound single-threaded.
pub(crate) fn adopt_login_shell_path() {
    let (program, args) = sirio_terminal::command_shell_invocation(&format!(
        "printf '%s' {BEGIN}; printenv PATH; printf '%s' {END}"
    ));
    let Some(login) = login_shell_path(&program, &args) else {
        return;
    };
    let current = std::env::var_os("PATH").unwrap_or_default();
    // SAFETY: called from the top of `main`, before any thread exists.
    unsafe { std::env::set_var("PATH", merge_paths(&login, &current)) };
}

fn login_shell_path(program: &str, args: &[String]) -> Option<OsString> {
    // ponytail: no timeout; stdin is closed so the shell cannot block on a read.
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    extract(&stdout)
        .filter(|path| !path.is_empty())
        .map(OsString::from)
}

/// Startup files may print to stdout, so the value is read between markers.
fn extract(stdout: &str) -> Option<&str> {
    let start = stdout.find(BEGIN)? + BEGIN.len();
    let end = stdout[start..].find(END)? + start;
    Some(stdout[start..end].trim_end_matches('\n'))
}

/// Login-shell entries first, so binaries resolve as they do in the user's
/// terminal; anything only the current environment had is kept after them.
fn merge_paths(login: &OsStr, current: &OsStr) -> OsString {
    let mut entries: Vec<PathBuf> = Vec::new();
    for entry in std::env::split_paths(login).chain(std::env::split_paths(current)) {
        if !entry.as_os_str().is_empty() && !entries.contains(&entry) {
            entries.push(entry);
        }
    }
    std::env::join_paths(entries).unwrap_or_else(|_| current.to_os_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_reads_the_path_between_markers_and_ignores_startup_chatter() {
        let stdout = "Welcome!\nSIRIO_PATH_BEGIN/a/bin:/b/bin\nSIRIO_PATH_ENDbye\n";
        assert_eq!(extract(stdout), Some("/a/bin:/b/bin"));
    }

    #[test]
    fn extract_returns_none_without_both_markers() {
        assert_eq!(extract("SIRIO_PATH_BEGIN/a/bin\n"), None);
        assert_eq!(extract("no markers here"), None);
    }

    #[test]
    fn merge_puts_login_entries_first_and_keeps_current_only_entries() {
        let merged = merge_paths(
            OsStr::new("/opt/homebrew/bin:/usr/bin:/bin"),
            OsStr::new("/usr/bin:/bin:/custom/bin"),
        );
        assert_eq!(merged, "/opt/homebrew/bin:/usr/bin:/bin:/custom/bin");
    }

    #[test]
    fn merge_drops_duplicates_and_empty_entries() {
        let merged = merge_paths(OsStr::new("/a::/a:/b"), OsStr::new("/b:"));
        assert_eq!(merged, "/a:/b");
    }

    #[test]
    fn login_shell_path_runs_the_shell_and_reads_the_marked_value() {
        use std::os::unix::fs::PermissionsExt;

        let scratch = std::env::temp_dir().join(format!("sirio-login-path-{}", std::process::id()));
        std::fs::create_dir_all(&scratch).expect("create scratch dir");
        let fake_shell = scratch.join("fake-shell");
        std::fs::write(
            &fake_shell,
            "#!/bin/sh\nprintf 'noise\\n'\nprintf 'SIRIO_PATH_BEGIN/fake/bin:/usr/bin\\nSIRIO_PATH_END'\n",
        )
        .expect("write fake shell");
        std::fs::set_permissions(&fake_shell, std::fs::Permissions::from_mode(0o755))
            .expect("make fake shell executable");

        let path = login_shell_path(
            &fake_shell.to_string_lossy(),
            &["-lc".to_string(), "ignored".to_string()],
        );

        let _ = std::fs::remove_dir_all(&scratch);
        assert_eq!(path.as_deref(), Some(OsStr::new("/fake/bin:/usr/bin")));
    }

    #[test]
    fn login_shell_path_is_none_when_the_shell_cannot_run() {
        assert_eq!(login_shell_path("/definitely/missing/shell", &[]), None);
    }
}
