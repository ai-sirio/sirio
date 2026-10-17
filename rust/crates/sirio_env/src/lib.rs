//! The environment Sirio's children run in.
//!
//! A Finder or AppImage launch hands the app its launcher's environment: the
//! AppImage's mount prefixes, its library paths and a PATH that is not the
//! user's. `launch_environment` recovers what the user launched with, from a
//! snapshot the AppImage launcher takes before it changes anything, and
//! `user_environment_from` turns that into the environment every child starts
//! in. Nothing here reads a shell or decides when it runs: `main` does.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Written by `Scripts/appimage/AppRun`: the hex of `env -0`, taken before the
/// launcher changes anything.
pub const LAUNCH_SNAPSHOT_VAR: &str = "SIRIO_LAUNCH_ENV";

/// Belongs to the launcher, so no child is given it.
pub const LAUNCHER_ONLY: [&str; 5] = ["APPIMAGE", "APPDIR", "ARGV0", "OWD", LAUNCH_SNAPSHOT_VAR];

/// Variables the app exports *for* its children (`session::export_pane_host_root`).
/// Every other `set_var` or `remove_var` in `sirio` is for the app alone.
pub const FOR_CHILDREN: [&str; 1] = ["SIRIO_PANE_HOST_HOME"];

static INSTALLED: OnceLock<BTreeMap<String, String>> = OnceLock::new();

/// The environment the user launched with, without the launcher's own variables.
///
/// A snapshot that is absent or does not decode leaves the process environment
/// in charge, so a launch that could not be snapshotted still starts.
pub fn launch_environment(process: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut environment = process
        .get(LAUNCH_SNAPSHOT_VAR)
        .and_then(|hex| decode_snapshot(hex))
        .unwrap_or_else(|| process.clone());
    for name in LAUNCHER_ONLY {
        environment.remove(name);
    }
    environment
}

/// The environment every child of Sirio starts in: the launch environment with
/// `PATH` rebuilt as `cli_dir`, then the login shell's entries, then the
/// launch's own. Empty entries are dropped and the first occurrence wins.
pub fn user_environment_from(
    launch: &BTreeMap<String, String>,
    login_path: Option<&OsStr>,
    cli_dir: &Path,
) -> BTreeMap<String, String> {
    let launch_path = launch.get("PATH").map_or("", String::as_str);
    let login_entries = login_path.into_iter().flat_map(std::env::split_paths);
    let mut entries: Vec<PathBuf> = Vec::new();
    for entry in std::iter::once(cli_dir.to_path_buf())
        .chain(login_entries)
        .chain(std::env::split_paths(launch_path))
    {
        if !entry.as_os_str().is_empty() && !entries.contains(&entry) {
            entries.push(entry);
        }
    }
    let path = std::env::join_paths(&entries)
        .map(|joined| joined.to_string_lossy().into_owned())
        .unwrap_or_else(|_| launch_path.to_string());
    let mut environment = launch.clone();
    environment.insert("PATH".to_string(), path);
    environment
}

/// Records the user environment for this process; the app calls it once, at the
/// end of `main`. A second call is ignored.
pub fn install(mut user: BTreeMap<String, String>) {
    // The app's own decision wins: a variable it exported is in the process,
    // and one it removed must not survive from the launch snapshot.
    for name in FOR_CHILDREN {
        match std::env::var_os(name) {
            Some(value) => {
                user.insert(name.to_string(), value.to_string_lossy().into_owned());
            }
            None => {
                user.remove(name);
            }
        }
    }
    let _ = INSTALLED.set(user);
}

/// The user environment, once `install` has run.
pub fn installed() -> Option<&'static BTreeMap<String, String>> {
    INSTALLED.get()
}

/// The environment for a child that a site starts through something other than
/// `command`: the user environment once installed, else this process's.
pub fn for_child() -> BTreeMap<String, String> {
    installed()
        .cloned()
        .unwrap_or_else(|| std::env::vars().collect())
}

/// `Command::new(program)` in the user environment. This is the only way a site
/// applies it: the clear happens here, so a `.env(..)` the caller adds afterwards
/// (git's `LC_ALL=C`, say) comes after it and survives it.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    if let Some(user) = installed() {
        command.env_clear().envs(user);
    }
    command
}

/// Names in this process's environment that the user environment leaves out.
/// Empty until `install` has run.
pub fn absent_from_user() -> Vec<String> {
    let Some(user) = installed() else {
        return Vec::new();
    };
    std::env::vars()
        .map(|(name, _)| name)
        .filter(|name| !user.contains_key(name))
        .collect()
}

/// Decodes the hex dump of `env -0`: NUL-terminated `NAME=value` entries. A
/// snapshot that does not decode or holds no entry is not a snapshot; an empty
/// environment would start every child with no PATH at all.
fn decode_snapshot(hex: &str) -> Option<BTreeMap<String, String>> {
    let text = String::from_utf8(decode_hex(hex)?).ok()?;
    let environment: BTreeMap<String, String> = text
        .split('\0')
        .filter_map(|entry| entry.split_once('='))
        .filter(|(name, _)| !name.is_empty())
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    (!environment.is_empty()).then_some(environment)
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    let digits = hex.as_bytes();
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    digits
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[high, low]| Some((hex_digit(high)? << 4) | hex_digit(low)?))
        .collect()
}

fn hex_digit(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// What `env -0 | od -An -tx1 -v | tr -d ' \n'` writes for these entries.
    fn snapshot_of(entries: &[&str]) -> String {
        let mut bytes = Vec::new();
        for entry in entries {
            bytes.extend_from_slice(entry.as_bytes());
            bytes.push(0);
        }
        hex(&bytes)
    }

    #[test]
    fn snapshot_absent_falls_back_to_the_process() {
        let process = map(&[
            ("PATH", "/m/usr/bin"),
            ("HOME", "/h"),
            ("APPIMAGE", "/x.AppImage"),
            ("APPDIR", "/tmp/.mount"),
        ]);
        assert_eq!(
            launch_environment(&process),
            map(&[("PATH", "/m/usr/bin"), ("HOME", "/h")])
        );
    }

    #[test]
    fn snapshot_restores_values_the_launcher_changed() {
        let snapshot = snapshot_of(&["PATH=/usr/bin", "HOME=/h"]);
        let process = map(&[
            ("PATH", "/m/usr/bin:/usr/bin"),
            ("LD_LIBRARY_PATH", "/m/lib"),
            ("GTK_THEME", "x"),
            (LAUNCH_SNAPSHOT_VAR, snapshot.as_str()),
        ]);
        assert_eq!(
            launch_environment(&process),
            map(&[("PATH", "/usr/bin"), ("HOME", "/h")])
        );
    }

    #[test]
    fn snapshot_values_may_contain_equals_and_newlines() {
        let snapshot = snapshot_of(&["A=b=c\nd"]);
        let process = map(&[(LAUNCH_SNAPSHOT_VAR, snapshot.as_str())]);
        assert_eq!(launch_environment(&process), map(&[("A", "b=c\nd")]));
    }

    #[test]
    fn malformed_snapshot_falls_back_to_the_process() {
        let non_utf8 = hex(&[b'A', b'=', 0xff, 0]);
        for snapshot in ["abc", "zz", non_utf8.as_str(), ""] {
            let process = map(&[("PATH", "/usr/bin"), (LAUNCH_SNAPSHOT_VAR, snapshot)]);
            assert_eq!(
                launch_environment(&process),
                map(&[("PATH", "/usr/bin")]),
                "snapshot {snapshot:?} must not replace the process environment"
            );
        }
    }

    #[test]
    fn entries_without_equals_are_skipped() {
        let snapshot = snapshot_of(&["BARE", "A=1"]);
        let process = map(&[(LAUNCH_SNAPSHOT_VAR, snapshot.as_str())]);
        assert_eq!(launch_environment(&process), map(&[("A", "1")]));
    }

    #[test]
    fn launcher_only_names_never_reach_children() {
        let snapshot = snapshot_of(&[
            "APPIMAGE=/x.AppImage",
            "APPDIR=/tmp/.mount",
            "ARGV0=sirio",
            "OWD=/home/u",
            "SIRIO_LAUNCH_ENV=stale",
            "HOME=/h",
        ]);
        let process = map(&[(LAUNCH_SNAPSHOT_VAR, snapshot.as_str())]);
        assert_eq!(launch_environment(&process), map(&[("HOME", "/h")]));
    }

    #[test]
    fn launch_environment_is_taken_before_the_app_changes_its_own() {
        // The app removes WAYLAND_DISPLAY from its own environment on X11;
        // the launch snapshot still has it, and children must get it back.
        let snapshot = snapshot_of(&["WAYLAND_DISPLAY=wayland-1"]);
        let process = map(&[(LAUNCH_SNAPSHOT_VAR, snapshot.as_str())]);
        assert!(launch_environment(&process).contains_key("WAYLAND_DISPLAY"));
    }

    #[test]
    fn cli_dir_comes_first_and_only_once() {
        let launch = map(&[("PATH", "/usr/bin:/cli/bin:/bin")]);
        let environment = user_environment_from(&launch, None, Path::new("/cli/bin"));
        assert_eq!(environment["PATH"], "/cli/bin:/usr/bin:/bin");
    }

    #[test]
    fn login_entries_precede_launch_entries() {
        let launch = map(&[("PATH", "/usr/bin:/bin")]);
        let environment = user_environment_from(
            &launch,
            Some(OsStr::new("/opt/homebrew/bin:/usr/bin")),
            Path::new("/cli"),
        );
        assert_eq!(environment["PATH"], "/cli:/opt/homebrew/bin:/usr/bin:/bin");
    }

    #[test]
    fn empty_path_entries_are_dropped() {
        let launch = map(&[("PATH", "/usr/bin::/bin:")]);
        let environment =
            user_environment_from(&launch, Some(OsStr::new("::/opt:")), Path::new("/cli"));
        assert_eq!(environment["PATH"], "/cli:/opt:/usr/bin:/bin");
    }

    #[cfg(unix)]
    #[test]
    fn command_without_install_inherits_the_process() {
        let output = command("/usr/bin/env").output().expect("run env");
        let process_text: String = std::env::vars()
            .map(|(name, value)| format!("{name}={value}\n"))
            .collect();
        // Both sides as the same fragments, sorted: a value that holds a
        // newline splits identically on each side.
        let mut expected: Vec<&str> = process_text.split_inclusive('\n').collect();
        let child_text = String::from_utf8_lossy(&output.stdout).into_owned();
        let mut actual: Vec<&str> = child_text.split_inclusive('\n').collect();
        expected.sort();
        actual.sort();
        assert_eq!(actual, expected);
    }

    #[cfg(unix)]
    #[test]
    fn env_set_after_command_survives_the_clear() {
        install(map(&[("PATH", "/usr/bin:/bin"), ("HOME", "/h")]));
        let output = command("/usr/bin/env")
            .env("LC_ALL", "C")
            .output()
            .expect("run env");
        let text = String::from_utf8(output.stdout).expect("env prints UTF-8 here");
        let user = installed().expect("installed");
        assert!(text.lines().any(|line| line == "LC_ALL=C"));
        for line in text.lines() {
            let name = line.split_once('=').map_or(line, |(name, _)| name);
            assert!(
                name == "LC_ALL" || user.contains_key(name),
                "{line} reached the child from outside the user environment"
            );
        }
    }

    #[test]
    fn for_children_variables_reach_the_user_environment() {
        // SAFETY: a test process of its own under nextest; nothing else reads
        // the environment concurrently.
        unsafe { std::env::set_var("SIRIO_PANE_HOST_HOME", "/h") };
        install(map(&[("PATH", "/usr/bin")]));
        assert_eq!(
            installed()
                .and_then(|user| user.get("SIRIO_PANE_HOST_HOME"))
                .map(String::as_str),
            Some("/h")
        );
    }
}
