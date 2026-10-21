//! Git clone with live transfer progress.

use std::path::Path;
use std::time::Duration;

use crate::GitError;
use crate::git::{GIT_BINARY, GitRunner, path_arg};

/// How long a clone may go without writing a line of progress before it is
/// killed.
///
/// An inactivity budget rather than a total deadline: `--progress` makes git
/// write continuously, so a large clone that is advancing is never killed
/// however long it takes, while one stalled on the network, a lost connection
/// or a prompt nobody can answer is, after a minute of silence.
const CLONE_STALL_TIMEOUT: Duration = Duration::from_secs(60);

/// Namespace for clone operations.
pub struct GitClone;

impl GitClone {
    /// Clones `url` into `destination`, forwarding each `Receiving objects`
    /// percentage as a value from 0.0 through 1.0. The streaming runner
    /// owns process completion and reports git failures through `GitError`.
    ///
    /// Returns whether the underlying stderr capture was truncated (see
    /// [`crate::GitCommandResult::truncated`]). This is **not** a failure —
    /// the streaming runner already delivered every progress line to
    /// `on_progress` as it arrived; truncation only means the retained raw
    /// capture (used for diagnostics on failure) was cut short. A caller
    /// that only consumes progress can ignore the flag; one that wants to
    /// tell the user their view of the clone was incomplete should surface
    /// it as a non-fatal notice.
    pub fn clone<F>(url: &str, destination: &Path, on_progress: F) -> Result<bool, GitError>
    where
        F: FnMut(f64),
    {
        Self::clone_with(
            Path::new(GIT_BINARY),
            CLONE_STALL_TIMEOUT,
            url,
            destination,
            on_progress,
        )
    }

    /// [`Self::clone`] with the git binary and the stall budget chosen by the
    /// caller, so a test can run a fake git with a short budget.
    ///
    /// A clone that fails leaves nothing it created behind: a killed git
    /// cannot clean up after itself, and a half-made destination would make
    /// every retry fail with "already exists and is not an empty directory".
    fn clone_with<F>(
        binary: &Path,
        stall: Duration,
        url: &str,
        destination: &Path,
        mut on_progress: F,
    ) -> Result<bool, GitError>
    where
        F: FnMut(f64),
    {
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut arguments = vec!["clone".to_string(), "--progress".to_string()];
        // Git's local-clone optimization can avoid the receiving phase
        // entirely. Disabling it for a local source keeps progress behavior
        // observable while remaining offline and fast for local fixtures.
        if Path::new(url).exists() || parent.join(url).exists() {
            arguments.push("--no-local".to_string());
        }
        // The source goes through `path_arg` like the destination: a
        // caller handing over a canonicalized path passes `\\?\C:\...`,
        // which git parses as a remote (`hostname contains invalid
        // characters`). A verbatim prefix can only appear on a Windows
        // filesystem path — no real URL starts with `\\?\` — so stripping
        // it here cannot corrupt a genuine remote. The same prefix also
        // rides inside a `file://` URL a caller serialized from a
        // canonicalized path (`file://\\?\C:\...`); git resolves that to
        // `//\\?\C:\...` and cannot open it, so it is stripped from the
        // URL's remainder too.
        #[cfg(windows)]
        let mut url = url.to_string();
        #[cfg(not(windows))]
        let url = url.to_string();
        #[cfg(windows)]
        if let Some(rest) = url.strip_prefix("file://")
            && let Some(stripped) = crate::git::strip_verbatim_prefix(rest)
        {
            url = format!("file://{stripped}");
        }
        arguments.push(path_arg(Path::new(&url)));
        arguments.push(path_arg(destination));
        let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
        // Git is spawned inside the parent, so a parent that does not exist
        // yet (the default `~/Sirio/projects` on a fresh install) would fail
        // the spawn itself, before git could report anything.
        std::fs::create_dir_all(parent).map_err(|error| GitError::Io {
            path: parent.to_path_buf(),
            message: error.to_string(),
        })?;
        // Only a destination this call is about to create is this call's to
        // remove: an existing (empty) folder the user pointed at stays.
        let existed = destination.symlink_metadata().is_ok();
        let result = GitRunner::run_streaming_remote_with_binary(
            binary,
            &arguments,
            parent,
            stall,
            move |line| {
                if let Some(progress) = Self::parse_progress(&line) {
                    on_progress(progress);
                }
            },
        );
        match result {
            Ok(result) => Ok(result.truncated),
            Err(error) => {
                if !existed {
                    let _ = std::fs::remove_dir_all(destination);
                }
                Err(error)
            }
        }
    }

    /// Extracts only `Receiving objects: N%` progress. Other clone phases do
    /// not describe object transfer and are intentionally ignored.
    pub fn parse_progress(line: &str) -> Option<f64> {
        let line = line.trim_start();
        let rest = line.strip_prefix("Receiving objects:")?;
        let percent_end = rest.find('%')?;
        let digits = rest[..percent_end].trim();
        let percent = digits.parse::<f64>().ok()?;
        if !(0.0..=100.0).contains(&percent) {
            return None;
        }
        Some(percent / 100.0)
    }
}

/// Convenience free-function spelling of [`GitClone::clone`]. See its doc
/// comment for the meaning of the returned `truncated` flag.
pub fn clone_repository<F>(url: &str, destination: &Path, on_progress: F) -> Result<bool, GitError>
where
    F: FnMut(f64),
{
    GitClone::clone(url, destination, on_progress)
}

#[cfg(test)]
mod tests {
    use super::GitClone;
    use crate::GitError;
    use crate::git::set_output_limit_override_for_test;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "sirio-git-clone-truncation-{tag}-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(path.canonicalize().expect("canonicalize temp dir"))
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("git is installed");
        assert!(status.success(), "git {args:?} failed in {dir:?}");
    }

    /// A source repo with enough content that `git clone --progress` writes
    /// more than a tiny cap's worth of stderr, without needing a slow, huge
    /// fixture (git's `--progress` output is percentage-throttled, not
    /// volume-scaled — see `clone.rs`'s test module doc and
    /// `docs/linux-rewrite/fullapp/` for the measurement this is based on).
    fn source_repo(tag: &str) -> TempDir {
        let repo = TempDir::new(tag);
        git(repo.path(), &["init", "-q", "-b", "main"]);
        git(repo.path(), &["config", "user.email", "test@sirio.dev"]);
        git(repo.path(), &["config", "user.name", "Sirio Test"]);
        for index in 0..40 {
            std::fs::write(
                repo.path().join(format!("file-{index:03}.txt")),
                "content\n".repeat(32),
            )
            .expect("write fixture file");
        }
        git(repo.path(), &["add", "-A"]);
        git(repo.path(), &["commit", "-q", "-m", "seed"]);
        repo
    }

    /// The finding this crate ships to fix: `GitCommandResult::truncated`
    /// used to be produced and then discarded at this exact call site
    /// (`GitRunner::run_streaming(...)?; Ok(())`). This test would fail
    /// against that old signature — there would be no `bool` to assert on —
    /// and fails today if `GitClone::clone` silently drops the flag again.
    #[test]
    fn clone_reports_truncated_when_the_output_cap_is_lowered() {
        let source = source_repo("truncated-source");
        let destination_parent = TempDir::new("truncated-destination");
        let destination = destination_parent.path().join("clone");

        set_output_limit_override_for_test(Some(64));
        let result = GitClone::clone(source.path().to_str().unwrap(), &destination, |_| {});
        set_output_limit_override_for_test(None);

        let truncated = result.expect("a truncated clone is Ok, not an error");
        assert!(
            truncated,
            "a 64-byte cap against this fixture's --progress stderr must truncate"
        );
        assert!(
            destination.join("file-000.txt").exists(),
            "the clone itself must still have completed despite the truncated capture"
        );
    }

    /// The negative control: with the ordinary (large) cap, the same clone
    /// must not report truncation. Without this, a `truncated` flag that is
    /// hardcoded to `true` would pass the positive test above too.
    #[test]
    fn clone_does_not_report_truncated_under_the_default_cap() {
        let source = source_repo("clean-source");
        let destination_parent = TempDir::new("clean-destination");
        let destination = destination_parent.path().join("clone");

        set_output_limit_override_for_test(None);
        let result = GitClone::clone(source.path().to_str().unwrap(), &destination, |_| {});

        let truncated = result.expect("clone under the default cap succeeds");
        assert!(
            !truncated,
            "the default 10MB cap must not truncate this small fixture's output"
        );
    }

    /// "Clone Repository…" offers `~/Sirio/projects` as the destination's
    /// parent, a folder a fresh install does not have yet. Git is spawned
    /// *inside* the parent, so a missing one used to fail the spawn itself
    /// ("No such file or directory") before git ran at all.
    #[test]
    fn clone_creates_a_missing_parent_folder() {
        let source = source_repo("missing-parent-source");
        let root = TempDir::new("missing-parent-destination");
        let destination = root.path().join("missing").join("parent").join("clone");

        let result = GitClone::clone(source.path().to_str().unwrap(), &destination, |_| {});

        result.expect("a clone into a parent that does not exist yet succeeds");
        assert!(destination.join("file-000.txt").exists());
    }

    /// A fake `git` that creates its last argument (the destination) the way
    /// a real clone does, writes one progress line and then goes silent for
    /// thirty seconds, so only the stall deadline can end it.
    #[cfg(unix)]
    fn stalling_git(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let script = dir.join("stalling-git.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nfor last; do :; done\nmkdir -p \"$last/.git\"\n\
             echo \"Cloning into '$last'...\" 1>&2\nsleep 30\n",
        )
        .expect("write fake git");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("make fake git executable");
        script
    }

    /// A clone killed for stalling cannot clean up after itself (the kill is
    /// SIGKILL), and a leftover half-clone makes every retry fail with
    /// "destination path already exists and is not an empty directory".
    #[cfg(unix)]
    #[test]
    fn a_stalled_clone_removes_the_destination_it_created() {
        let root = TempDir::new("stalled-removes");
        let git = stalling_git(root.path());
        let destination = root.path().join("clone");

        let result = GitClone::clone_with(
            &git,
            Duration::from_millis(300),
            "https://example.invalid/repo.git",
            &destination,
            |_| {},
        );

        assert!(
            matches!(result, Err(GitError::Stalled { .. })),
            "expected Stalled, got {result:?}"
        );
        assert!(
            !destination.exists(),
            "the half-made clone was left behind for the retry to trip over"
        );
    }

    /// The cleanup must never delete something that was there before the
    /// clone: the user may have pointed the form at a folder they made.
    #[cfg(unix)]
    #[test]
    fn a_stalled_clone_keeps_a_destination_that_existed_before() {
        let root = TempDir::new("stalled-keeps");
        let git = stalling_git(root.path());
        let destination = root.path().join("clone");
        std::fs::create_dir(&destination).expect("create the empty destination");

        let result = GitClone::clone_with(
            &git,
            Duration::from_millis(300),
            "https://example.invalid/repo.git",
            &destination,
            |_| {},
        );

        assert!(
            matches!(result, Err(GitError::Stalled { .. })),
            "expected Stalled, got {result:?}"
        );
        assert!(
            destination.is_dir(),
            "a destination that existed before the clone was removed"
        );
    }
}
