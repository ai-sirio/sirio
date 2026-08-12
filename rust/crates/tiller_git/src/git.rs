//! Minimal, synchronous shell-out to `git`.
//!
//! Every invocation is bounded by a wall-clock timeout ([`run_accepting`],
//! default [`DEFAULT_GIT_TIMEOUT`]), so a hung git process or a stalled
//! filesystem can never block the caller indefinitely — the caller is a GPUI
//! update path. The deadline is *real*, not just reported: the child runs in
//! its own process group, and on expiry the whole group is killed and the
//! runner stops waiting on the process's output, not just on the process.
//!
//! # Why the process group, and why abandonment
//!
//! A killed direct child can leave grandchildren behind holding the output
//! pipes (e.g. `git` is a shell script whose `sleep` child inherits them).
//! Reading the pipes to EOF would then block until the grandchild exits —
//! the deadline would be reported but not enforced. Two layers defend
//! against this, exactly as Zed's `util::process::Child` does:
//!
//! - the child is spawned with `process_group(0)` and killed with
//!   `killpg(SIGKILL)`, so descendants die with it and the pipes close;
//! - if the pipes are somehow still open after the kill (a process in an
//!   uninterruptible sleep cannot die), the reader threads are abandoned
//!   after a short grace period instead of joined, so the caller is never
//!   blocked by pipe EOF no matter what the processes do.
//!
//! # Configuring the deadline
//!
//! The default budget ([`DEFAULT_GIT_TIMEOUT`]) applies whenever the
//! `TILLER_GIT_TIMEOUT_MS` environment variable is unset or unparseable.
//! The variable overrides the budget for the *default* path only (every
//! public call in this crate); an explicit budget passed to
//! [`run_with_timeout`] is always honored as given, so the deadline-enforcement
//! tests stay honest. Production does not set the variable and is therefore
//! byte-identical to a build without this feature. It exists so tests
//! running on a heavily loaded machine (parallel compiles, high load
//! average) can widen the budget without changing what the product does,
//! and so operators with pathological filesystems can do the same without a
//! rebuild. The value is consulted on every invocation (not cached), so a
//! test can set it at any point in its process.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::GitError;

/// The git binary to invoke, resolved via `PATH`.
const GIT_BINARY: &str = "git";

/// How long a single git invocation may run before it is killed.
///
/// Worktree and status commands are local metadata reads that finish in
/// milliseconds; ten seconds leaves generous headroom for cold caches and
/// network-mounted home directories while still bounding a hung process.
pub const DEFAULT_GIT_TIMEOUT: Duration = Duration::from_secs(10);

/// Environment variable that overrides [`DEFAULT_GIT_TIMEOUT`], in
/// milliseconds. See the module docs for the semantics: only the default
/// path is affected, and only while the variable is set.
const TIMEOUT_ENV_VAR: &str = "TILLER_GIT_TIMEOUT_MS";

/// How often the runner polls the child while waiting.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// How long the runner waits for the pipe readers to finish after killing
/// the process group, and for the killed child to be reaped, before giving
/// up. Bounded so the deadline stays real even for unkillable processes.
const KILL_GRACE_PERIOD: Duration = Duration::from_millis(100);

/// Captured result of one `git` invocation.
#[derive(Debug)]
pub(crate) struct GitOutput {
    /// Raw stdout. Kept as bytes because `git status --porcelain=v2 -z` is a
    /// NUL-separated byte stream and paths may be non-UTF-8.
    pub stdout: Vec<u8>,
    pub stderr: String,
    /// Process exit code (`None` if terminated by a signal).
    pub status: Option<i32>,
}

impl GitOutput {
    pub fn stdout_string(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn is_success(&self) -> bool {
        self.status == Some(0)
    }
}

/// Runs `git <args>` with `cwd` as the working directory, capturing stdout
/// and stderr, with the default timeout. Never checks the exit code —
/// callers decide what to accept — but surfaces spawn failures as
/// [`GitError::Spawn`] and deadline misses as [`GitError::TimedOut`].
pub(crate) fn run(args: &[&str], cwd: &Path) -> Result<GitOutput, GitError> {
    run_with_timeout(args, cwd, configured_timeout())
}

/// The deadline for default-path invocations: the `TILLER_GIT_TIMEOUT_MS`
/// override when set and parseable, else [`DEFAULT_GIT_TIMEOUT`]. Consulted
/// per call so an override set mid-process (a test process, or an operator
/// changing an environment) takes effect without coordination.
fn configured_timeout() -> Duration {
    timeout_from_env(std::env::var(TIMEOUT_ENV_VAR).ok().as_deref())
}

/// Pure version of [`configured_timeout`], separated for testing: `None` or
/// a non-numeric value falls back to the default; a numeric value is taken
/// as milliseconds (including zero — the operator asked for it).
fn timeout_from_env(raw: Option<&str>) -> Duration {
    match raw.and_then(|value| value.parse::<u64>().ok()) {
        Some(millis) => Duration::from_millis(millis),
        None => DEFAULT_GIT_TIMEOUT,
    }
}

/// Like [`run`], with an explicit wall-clock timeout.
pub(crate) fn run_with_timeout(
    args: &[&str],
    cwd: &Path,
    timeout: Duration,
) -> Result<GitOutput, GitError> {
    let mut command = Command::new(GIT_BINARY);
    command
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Put the child in its own process group so the whole tree — including
    // any grandchild holding the output pipes — can be killed at once.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|error| GitError::Spawn {
        message: error.to_string(),
    })?;

    // Drain both pipes on reader threads so a chatty child cannot deadlock
    // against full pipe buffers. Each reader sends its buffer when the pipe
    // reaches EOF; the channels are what let the caller stop waiting on the
    // readers after the deadline instead of joining them to EOF.
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();
    let stdout_reader = std::thread::spawn(move || {
        let _ = stdout_tx.send(read_to_end(stdout));
    });
    let stderr_reader = std::thread::spawn(move || {
        let _ = stderr_tx.send(read_to_end(stderr));
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    kill_tree_and_stop_waiting(&mut child, &stdout_rx, &stderr_rx);
                    return Err(GitError::TimedOut {
                        command: args.join(" "),
                        timeout,
                    });
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(error) => {
                kill_tree_and_stop_waiting(&mut child, &stdout_rx, &stderr_rx);
                return Err(GitError::Spawn {
                    message: error.to_string(),
                });
            }
        }
    };

    // The child exited, so its pipe write ends are closed and the readers
    // finish. Collect their output.
    let stdout = stdout_rx.recv().unwrap_or_default();
    let stderr = stderr_rx.recv().unwrap_or_default();
    let _ = stdout_reader.join();
    let _ = stderr_reader.join();

    Ok(GitOutput {
        stdout,
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        status: status.code(),
    })
}

/// Kills the child's whole process group and stops waiting on its output:
/// the child is reaped within a grace period and the readers are abandoned
/// if they are still blocked, so this function always returns promptly.
fn kill_tree_and_stop_waiting(
    child: &mut Child,
    stdout_rx: &mpsc::Receiver<Vec<u8>>,
    stderr_rx: &mpsc::Receiver<Vec<u8>>,
) {
    kill_tree(child);
    reap_within_grace(child);
    // Collect whatever the readers managed to finish within the grace
    // period. If a pipe is still open (an unkillable process holds it), the
    // receiver times out and the reader thread is dropped — detached, it
    // finishes whenever the pipe finally closes. The caller must not block.
    let _ = stdout_rx.recv_timeout(KILL_GRACE_PERIOD);
    let _ = stderr_rx.recv_timeout(KILL_GRACE_PERIOD);
}

/// Kills the child and every descendant: the child runs in its own process
/// group (see [`run_with_timeout`]), so `killpg` reaches the whole tree.
/// Non-Unix falls back to killing the child alone.
#[cfg(unix)]
fn kill_tree(child: &mut Child) {
    let pid = child.id() as libc::pid_t;
    // Returns -1 if the group is already gone; nothing to do then.
    unsafe { libc::killpg(pid, libc::SIGKILL) };
}

#[cfg(not(unix))]
fn kill_tree(child: &mut Child) {
    let _ = child.kill();
}

/// Reaps the child if it dies within the grace period. A process in an
/// uninterruptible sleep cannot be killed; leaving a zombie is preferable to
/// blocking the caller on `wait`.
fn reap_within_grace(child: &mut Child) {
    let deadline = Instant::now() + KILL_GRACE_PERIOD;
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Reads a pipe to EOF, best-effort.
fn read_to_end(mut pipe: impl std::io::Read) -> Vec<u8> {
    let mut buffer = Vec::new();
    std::io::Read::read_to_end(&mut pipe, &mut buffer).ok();
    buffer
}

/// Runs `git <args>` in `cwd` and requires one of `accepted` exit codes,
/// returning the full captured output.
pub(crate) fn run_accepting(
    args: &[&str],
    cwd: &Path,
    accepted: &[i32],
) -> Result<GitOutput, GitError> {
    let output = run(args, cwd)?;
    if output
        .status
        .is_some_and(|status| accepted.contains(&status))
    {
        Ok(output)
    } else {
        Err(GitError::CommandFailed {
            code: output.status.unwrap_or(-1),
            stderr: output.stderr,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Proves the timeout fires for real: a fake `git` script that sleeps for
    /// ten seconds is put first on `PATH` and the runner is pointed at it
    /// with a 200 ms budget. The test must come back with
    /// [`GitError::TimedOut`] within a bounded time — not hang, and not
    /// merely believe the error type exists.
    ///
    /// `PATH` is process-global, so the test re-executes itself as a child
    /// process with the fake first on `PATH`; the parent asserts on the
    /// child's outcome. Unix-only: the fake is a shell script.
    #[cfg(unix)]
    #[test]
    fn git_timeout_fires() {
        use std::os::unix::fs::PermissionsExt;

        if std::env::var_os("TILLER_GIT_TIMEOUT_TEST").is_none() {
            let scratch = scratch_dir();
            let fake_dir = scratch.join("fake-bin");
            std::fs::create_dir_all(&fake_dir).expect("create fake bin dir");
            let fake_git = fake_dir.join("git");
            std::fs::write(&fake_git, "#!/bin/sh\nsleep 10\n").expect("write fake git");
            std::fs::set_permissions(&fake_git, std::fs::Permissions::from_mode(0o755))
                .expect("make fake git executable");

            let exe = std::env::current_exe().expect("test binary path");
            let path = std::env::var_os("PATH").unwrap_or_default();
            let output = Command::new(exe)
                .args([
                    "--exact",
                    "git::tests::git_timeout_fires",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("TILLER_GIT_TIMEOUT_TEST", "1")
                .env(
                    "PATH",
                    format!("{}:{}", fake_dir.display(), path.to_string_lossy()),
                )
                .output()
                .expect("rerun the test with the fake git on PATH");

            let _ = std::fs::remove_dir_all(&scratch);
            assert!(
                output.status.success(),
                "child test failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        // Child: `git` on PATH is now the sleeping fake.
        let started = Instant::now();
        let result = run_with_timeout(
            &["worktree", "list", "--porcelain"],
            Path::new("/"),
            Duration::from_millis(200),
        );
        let elapsed = started.elapsed();

        assert!(
            matches!(result, Err(GitError::TimedOut { .. })),
            "expected TimedOut, got {result:?}"
        );
        // The budget is 200 ms and the fake git would otherwise sleep for
        // ten seconds, so any return well under the default proves the
        // deadline fired. Five seconds (not one) keeps the assertion honest
        // under machine load, where spawning the fake and reaping the group
        // can take a moment; the point is "enforced promptly", not
        // "enforced on an idle machine".
        assert!(
            elapsed < Duration::from_secs(5),
            "timeout took {elapsed:?} — the deadline is not enforced"
        );
    }

    #[test]
    fn timeout_from_env_parses_override_and_falls_back() {
        assert_eq!(
            timeout_from_env(Some("120000")),
            Duration::from_secs(120),
            "a numeric value is milliseconds"
        );
        assert_eq!(
            timeout_from_env(None),
            DEFAULT_GIT_TIMEOUT,
            "unset falls back to the production default"
        );
        assert_eq!(
            timeout_from_env(Some("not-a-number")),
            DEFAULT_GIT_TIMEOUT,
            "an unparseable value falls back to the production default"
        );
        assert_eq!(
            timeout_from_env(Some("0")),
            Duration::ZERO,
            "an explicit zero is honored"
        );
    }

    fn scratch_dir() -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "tiller-git-timeout-{}-{unique}",
            std::process::id()
        ))
    }
}
