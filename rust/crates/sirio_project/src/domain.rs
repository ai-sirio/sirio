use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Deterministic Linux project defaults. An explicit environment override is
/// useful for tests and portable installations; XDG data is the normal base.
pub fn default_project_base() -> PathBuf {
    if let Some(path) = std::env::var_os("TILLER_PROJECTS_DIR").filter(|value| !value.is_empty()) {
        return PathBuf::from(path);
    }
    if let Some(data) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(data).join("Sirio").join("projects");
    }
    // A native Windows launch has no HOME, so XDG's data fallback must
    // resolve through USERPROFILE — otherwise the base silently degrades
    // to a RELATIVE "Sirio/projects" under the current directory.
    home_dir()
        .map(|home| home.join("Sirio").join("projects"))
        .unwrap_or_else(|| PathBuf::from("Sirio").join("projects"))
}

/// The user's home directory, or `None` when the environment offers no
/// source. Windows has no `HOME` in a native GUI launch: `USERPROFILE`
/// (then `HOMEDRIVE`+`HOMEPATH`) takes its place; unix keeps reading
/// `HOME`. Duplicated deliberately — the leaf crates that need it cannot
/// depend on each other; see the copies in `sirio_usage/src/lib.rs` and
/// `sirio/src/main.rs` (+ `sirio/src/session.rs`).
fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERPROFILE")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                let drive = std::env::var_os("HOMEDRIVE").filter(|value| !value.is_empty())?;
                let path = std::env::var_os("HOMEPATH").filter(|value| !value.is_empty())?;
                Some(PathBuf::from(drive).join(path))
            })
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }
}

/// Renders a filesystem path as user-facing text. On Windows the verbatim
/// `\\?\` prefix that `std::fs::canonicalize` always produces is stripped —
/// from the *string only* (the `Path` itself is never rewritten, so fs calls
/// keep the long-path capability; same constraint as `sirio_git::path_arg`) —
/// and then the home directory collapses to `~` with a single platform
/// separator (`~\` on Windows, `~/` elsewhere).
pub fn display_path(path: &Path) -> String {
    display_path_against(path, home_dir().as_deref())
}

/// [`display_path`] minus the tilde collapsing: an absolute, unambiguous
/// string for places where the reader will paste or resolve the path again
/// (clipboard "copy path", command lines). The verbatim prefix is still
/// stripped from the string only.
pub fn display_absolute_path(path: &Path) -> String {
    let mut string = path.to_string_lossy().into_owned();
    #[cfg(windows)]
    if let Some(stripped) = strip_verbatim_prefix(&string) {
        string = stripped;
    }
    string
}

/// [`display_path`] with the home directory injected, so tests can prove the
/// tilde collapsing without mutating process-global environment variables.
pub(crate) fn display_path_against(path: &Path, home: Option<&Path>) -> String {
    let string = display_absolute_path(path);
    let Some(home) = home else {
        return string;
    };
    // After stripping, a verbatim-disk path compares component-for-component
    // against the plain (non-verbatim) home directory.
    Path::new(&string)
        .strip_prefix(home)
        .map(|relative| {
            if relative.as_os_str().is_empty() {
                "~".to_string()
            } else {
                format!(
                    "~{}{}",
                    std::path::MAIN_SEPARATOR,
                    relative.to_string_lossy()
                )
            }
        })
        .unwrap_or_else(|_| string)
}

/// Strips the Windows `\\?\` verbatim prefix from a path string:
/// `\\?\C:\...` becomes `C:\...` and `\\?\UNC\server\share\...` becomes
/// `\\server\share\...`. Returns `None` when the path is not
/// verbatim-prefixed. Display-side twin of the argv-specific copy in
/// `sirio_git`; duplicated deliberately so this dependency-free leaf crate
/// does not depend on `sirio_git`.
#[cfg(windows)]
fn strip_verbatim_prefix(path: &str) -> Option<String> {
    let rest = path.strip_prefix(r"\\?\")?;
    if let Some(unc) = rest.strip_prefix("UNC\\") {
        Some(format!(r"\\{unc}"))
    } else {
        Some(rest.to_string())
    }
}

/// Chooses the branch base and parent directory for a new worktree.
pub fn resolve_worktree_defaults(
    project_root: &Path,
    explicit_base_branch: Option<&str>,
    primary_branch: Option<&str>,
    explicit_location: Option<&Path>,
) -> (String, PathBuf) {
    let branch = explicit_base_branch
        .filter(|branch| !branch.trim().is_empty())
        .or(primary_branch)
        .unwrap_or("HEAD")
        .to_string();
    let parent = explicit_location.map(Path::to_path_buf).unwrap_or_else(|| {
        project_root
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| project_root.to_path_buf())
    });
    (branch, parent)
}

/// Moves a known item before a target, or to the end when no target is given.
/// Unknown ids and no-op moves leave the sequence untouched.
pub fn move_item<T: Eq + Copy>(items: &mut Vec<T>, item: T, before: Option<T>) -> bool {
    let Some(from) = items.iter().position(|candidate| *candidate == item) else {
        return false;
    };
    let target = before.and_then(|target| items.iter().position(|candidate| *candidate == target));
    if before.is_some() && target.is_none() {
        return false;
    }
    let value = items.remove(from);
    let destination = match target {
        Some(target) if target > from => target - 1,
        Some(target) => target,
        None => items.len(),
    };
    if destination == from {
        items.insert(from, value);
        return false;
    }
    items.insert(destination, value);
    true
}

/// Numeric tab selection: 1-based, with 9 selecting the last tab.
pub fn numeric_tab_selection(number: u8, count: usize) -> Option<usize> {
    if count == 0 || number == 0 {
        return None;
    }
    let index = if number == 9 {
        count - 1
    } else {
        usize::from(number - 1)
    };
    (index < count).then_some(index)
}

/// A moving tab index that wraps at either edge.
pub fn move_tab(index: usize, count: usize, direction: i8) -> Option<usize> {
    if count == 0 || index >= count {
        return None;
    }
    Some(if direction < 0 {
        if index == 0 { count - 1 } else { index - 1 }
    } else if index + 1 == count {
        0
    } else {
        index + 1
    })
}

/// Throttles automatic tab naming by both elapsed time and transcript growth.
#[derive(Debug, Default)]
pub struct AutoNamingThrottle {
    last_request: Option<Instant>,
    last_transcript_len: usize,
}

impl AutoNamingThrottle {
    pub const MIN_INTERVAL: Duration = Duration::from_secs(30);
    pub const MIN_GROWTH: usize = 200;

    /// Whether a generated-name request should fire now. The first run is
    /// never throttled — both gates apply only once a request was recorded,
    /// mirroring the reference `AutoNamingThrottle.swift`.
    pub fn should_request(&self, now: Instant, transcript_len: usize) -> bool {
        let Some(last) = self.last_request else {
            return true;
        };
        now.duration_since(last) >= Self::MIN_INTERVAL
            && transcript_len.saturating_sub(self.last_transcript_len) >= Self::MIN_GROWTH
    }

    pub fn record_request(&mut self, now: Instant, transcript_len: usize) {
        self.last_request = Some(now);
        self.last_transcript_len = transcript_len;
    }
}

/// A value-type once gate for restore/setup callbacks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OnceGate(bool);

impl OnceGate {
    pub fn fire(&mut self, action: impl FnOnce()) -> bool {
        if self.0 {
            return false;
        }
        self.0 = true;
        action();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #210: the default base must not mix path separators.
    ///
    /// `default_project_base` built its tail with `join("Sirio/projects")`
    /// -- one string holding a separator. On Windows `PathBuf` keeps that
    /// slash verbatim in the path's *text*, so the value rendered as
    /// `C:\Users\...\Sirio/projects`, and the clone form showed exactly
    /// that to the user at the moment it asks them to confirm where their
    /// repository will land.
    ///
    /// The assertion is on the string, deliberately. `components()` cannot
    /// see this: Rust's Windows path parser accepts `/` as a separator too,
    /// so it normalises the mixed form into the same components as the
    /// correct one, and a `components()`-based test passes on the broken
    /// code. I wrote that test first, and it did.
    ///
    /// Honest about its reach: on unix the two expressions are identical,
    /// so this only has teeth on Windows -- which is where the defect was.
    #[test]
    fn the_default_base_does_not_mix_path_separators() {
        let text = default_project_base().to_string_lossy().into_owned();
        let foreign = if std::path::MAIN_SEPARATOR == '/' {
            '\u{5C}'
        } else {
            '/'
        };
        assert!(
            !text.contains(foreign),
            "the default project base must use only this platform's \
             separator, got {text:?}"
        );
    }

    #[test]
    fn defaults_prefer_explicit_values_then_primary_and_sibling() {
        let (branch, parent) =
            resolve_worktree_defaults(Path::new("/home/me/sirio"), None, Some("main"), None);
        assert_eq!(branch, "main");
        assert_eq!(parent, PathBuf::from("/home/me"));
        let (branch, parent) = resolve_worktree_defaults(
            Path::new("/home/me/sirio"),
            Some("develop"),
            Some("main"),
            Some(Path::new("/worktrees")),
        );
        assert_eq!(
            (branch, parent),
            ("develop".into(), PathBuf::from("/worktrees"))
        );
    }

    #[test]
    fn ordering_ignores_unknown_and_noop_moves() {
        let mut ids = vec![1, 2, 3];
        assert!(!move_item(&mut ids, 9, Some(1)));
        assert!(!move_item(&mut ids, 2, Some(2)));
        assert!(move_item(&mut ids, 1, Some(3)));
        assert_eq!(ids, vec![2, 1, 3]);
        assert!(move_item(&mut ids, 1, None));
        assert_eq!(ids, vec![2, 3, 1]);
    }

    #[test]
    fn tab_order_wraps_and_numeric_selection_validates() {
        assert_eq!(move_tab(0, 3, -1), Some(2));
        assert_eq!(move_tab(2, 3, 1), Some(0));
        assert_eq!(numeric_tab_selection(1, 3), Some(0));
        assert_eq!(numeric_tab_selection(9, 3), Some(2));
        assert_eq!(numeric_tab_selection(4, 3), None);
    }

    /// F-CORE-DOM-06: pins the two edges a clamping reimplementation gets
    /// wrong. Position 9 always means "the last tab", even with more than
    /// nine tabs open -- not "the ninth tab" -- and a position beyond the
    /// group is a rejected selection (`None`, i.e. no change), not a silent
    /// clamp to the last tab.
    #[test]
    fn position_nine_always_means_the_last_tab_and_overflow_is_rejected_not_clamped() {
        assert_eq!(numeric_tab_selection(9, 12), Some(11));
        assert_eq!(numeric_tab_selection(5, 3), None);
    }

    #[test]
    fn auto_naming_requires_first_run_or_both_throttles() {
        let start = Instant::now();
        let mut throttle = AutoNamingThrottle::default();
        assert!(
            throttle.should_request(start, 199),
            "the first run is exempt from both gates"
        );
        assert!(throttle.should_request(start, 200));
        throttle.record_request(start, 200);
        assert!(!throttle.should_request(start + Duration::from_secs(31), 399));
        assert!(!throttle.should_request(start + Duration::from_secs(29), 401));
        assert!(throttle.should_request(start + Duration::from_secs(31), 400));
    }

    #[test]
    fn once_gate_runs_only_the_first_callback() {
        let mut gate = OnceGate::default();
        let mut calls = 0;
        assert!(gate.fire(|| calls += 1));
        assert!(!gate.fire(|| calls += 1));
        assert_eq!(calls, 1);
    }

    #[cfg(windows)]
    mod windows_display_paths {
        use super::*;

        #[test]
        fn verbatim_disk_prefix_is_stripped_from_the_string() {
            assert_eq!(display_path_against(Path::new(r"\\?\D:\x"), None), r"D:\x");
        }

        #[test]
        fn verbatim_unc_prefix_keeps_the_unc_authority() {
            assert_eq!(
                display_path_against(Path::new(r"\\?\UNC\srv\share\x"), None),
                r"\\srv\share\x"
            );
        }

        #[test]
        fn plain_non_prefixed_path_is_unchanged() {
            assert_eq!(
                display_path_against(Path::new(r"D:\Progetti\sirio"), None),
                r"D:\Progetti\sirio"
            );
        }

        #[test]
        fn verbatim_home_path_collapses_to_tilde_with_a_backslash() {
            let home = Path::new(r"C:\Users\me");
            assert_eq!(
                display_path_against(Path::new(r"\\?\C:\Users\me\Progetti\sirio"), Some(home)),
                r"~\Progetti\sirio"
            );
            // Exactly the home directory itself collapses to bare "~".
            assert_eq!(
                display_path_against(Path::new(r"\\?\C:\Users\me"), Some(home)),
                "~"
            );
        }

        #[test]
        fn verbatim_path_outside_home_stays_absolute() {
            let home = Path::new(r"C:\Users\me");
            assert_eq!(
                display_path_against(Path::new(r"\\?\D:\elsewhere"), Some(home)),
                r"D:\elsewhere"
            );
        }
    }

    /// Non-Windows output must stay byte-for-byte identical to the old
    /// `display_path`: no prefix stripping exists there, and `~` keeps the
    /// forward slash it has always used.
    #[cfg(not(windows))]
    mod unix_display_paths {
        use super::*;

        #[test]
        fn paths_render_unchanged() {
            assert_eq!(
                display_path_against(Path::new("/opt/tooling"), None),
                "/opt/tooling"
            );
            assert_eq!(
                display_path_against(Path::new("/home/other"), Some(Path::new("/home/me"))),
                "/home/other"
            );
        }

        #[test]
        fn home_relative_paths_collapse_with_a_forward_slash() {
            assert_eq!(
                display_path_against(Path::new("/home/me/proj"), Some(Path::new("/home/me"))),
                "~/proj"
            );
            assert_eq!(
                display_path_against(Path::new("/home/me"), Some(Path::new("/home/me"))),
                "~"
            );
        }
    }
}
