//! Minimal, synchronous shell-out to `git`.
//!
//! The Swift app runs git through `GitRunner` (async Process + pipes). This
//! crate is deliberately synchronous and std-only (one exception: `libc` for
//! process-group signals, see below): discovery runs a handful of
//! short-lived commands, and callers that need async (the UI) can wrap the
//! calls in a task. What matters is parity of *behavior* — same commands,
//! same acceptance rules, same stderr passthrough on failure.
//!
//! # There is deliberately no output cap here — read this before adding a caller
//!
//! `sirio_git::GitRunner` caps captured output (`DEFAULT_OUTPUT_LIMIT_BYTES`,
//! overridable, raising `OutputTruncated`). This runner does not: `read_to_end`
//! reads each pipe to EOF into an unbounded `Vec<u8>`, and this crate's
//! `GitError` has no truncation variant at all. That is currently safe, and it
//! is safe by *scope* rather than by luck — checked, not assumed (2026-08-20):
//!
//!   - `run`/`run_with_timeout`/`run_success` are all `pub(crate)`, so nothing
//!     outside this crate can reach them.
//!   - There are exactly **two** call sites, both in `discovery.rs`:
//!     `git worktree list --porcelain` (:80) and `git branch --show-current`
//!     (:91). Both outputs scale with worktree *count* and branch *name*, never
//!     with repository content — so neither can grow the way `diff`/`log`/`show`
//!     output does, which is what the cap one crate over exists to defend
//!     against. There is no hang risk either: `read_to_end` fully drains the
//!     pipe before returning.
//!
//! The hazard is therefore a *future* one, and it is aimed at whoever adds the
//! third call site. `run_success` is already imported here and convenient, so a
//! feature wanting `git log`/`git show` output during discovery has an easy,
//! obvious, uncapped path sitting ready. **If you are adding a command whose
//! output scales with repository content, do not just call it — go read
//! `sirio_git`'s `read_capped` first and decide deliberately.** The duplication
//! between these two runners is intentional (this crate stays dependency-free,
//! mirroring the Swift app's separate `SirioGit` package), but nothing
//! structural keeps them from drifting apart the moment one is changed and the
//! other is not.
//!
//! Every invocation is bounded by a wall-clock timeout ([`run_with_timeout`],
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
//!   `killpg(SIGKILL)`, so descendants die with it and the pipes close
//!   (Zed spawns with `setsid` and kills with `killpg` for the same reason);
//! - if the pipes are somehow still open after the kill (a process in an
//!   uninterruptible sleep cannot die), the reader threads are abandoned
//!   after a short grace period instead of joined, so the caller is never
//!   blocked by pipe EOF no matter what the processes do.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::GitError;

/// The git binary to invoke. Resolved via `PATH`; tests put a fake `git`
/// script first on `PATH` to exercise the timeout path.
const GIT_BINARY: &str = "git";

/// How long a single git invocation may run before it is killed.
///
/// Discovery commands (`worktree list --porcelain`, `branch --show-current`,
/// `rev-parse`) are local metadata reads that finish in milliseconds; ten
/// seconds leaves generous headroom for cold caches and network-mounted home
/// directories while still bounding a hung process.
pub const DEFAULT_GIT_TIMEOUT: Duration = Duration::from_secs(10);

/// How often the runner polls the child while waiting.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// How long the runner waits for the pipe readers to finish after killing
/// the process group, and for the killed child to be reaped, before giving
/// up. Bounded so the deadline stays real even for unkillable processes.
const KILL_GRACE_PERIOD: Duration = Duration::from_millis(100);

/// Captured result of one `git` invocation.
#[derive(Debug)]
pub(crate) struct GitOutput {
    pub stdout: String,
    pub stderr: String,
    /// Process exit code (`None` if terminated by a signal).
    pub status: Option<i32>,
}

/// Runs `git <args>` with `cwd` as the working directory, capturing stdout
/// and stderr, with the default timeout. Never checks the exit code —
/// callers decide what to accept — but surfaces spawn failures as
/// [`GitError::Spawn`] and deadline misses as [`GitError::TimedOut`].
pub(crate) fn run(args: &[&str], cwd: &Path) -> Result<GitOutput, GitError> {
    run_with_timeout(args, cwd, DEFAULT_GIT_TIMEOUT)
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
    // finish (a grandchild holding the pipes is an anomaly the timeout path
    // is built for, not this one). Collect their output.
    let stdout = stdout_rx.recv().unwrap_or_default();
    let stderr = stderr_rx.recv().unwrap_or_default();
    let _ = stdout_reader.join();
    let _ = stderr_reader.join();

    Ok(GitOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
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

/// Kills the child and every descendant, matching Zed's `util::process::Child`:
/// the child runs in its own process group (see `run_with_timeout`), so
/// `killpg` reaches the whole tree. Non-Unix falls back to killing the child
/// alone.
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

/// Runs `git <args>` in `cwd` and requires exit status 0, returning stdout.
pub(crate) fn run_success(args: &[&str], cwd: &Path) -> Result<String, GitError> {
    let output = run(args, cwd)?;
    if output.status == Some(0) {
        Ok(output.stdout)
    } else {
        Err(GitError::CommandFailed {
            code: output.status.unwrap_or(-1),
            stderr: output.stderr,
        })
    }
}

/// Unix-only as a whole: every test in here (and the scratch-directory
/// helper they share) depends on the `#!/bin/sh` fake git below, so on
/// Windows the module would compile to nothing but dead imports and a dead
/// helper — which `-D warnings` rejects. A portable test added later moves
/// this gate back to a plain `#[cfg(test)]` and pushes `#[cfg(unix)]` down
/// onto the shell-script fixtures instead.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Proves the timeout fires for real: a fake `git` script that sleeps for
    /// ten seconds is put first on `PATH` and the runner is pointed at it
    /// with a 200 ms budget. The test must come back with
    /// [`GitError::TimedOut`] within a bounded time — not hang, and not
    /// merely believe the error type exists.
    ///
    /// The fake is `#!/bin/sh` + `sleep 10`: killing the shell leaves the
    /// `sleep` grandchild holding the output pipes, which is exactly the
    /// failure shape the process-group kill and reader abandonment defend
    /// against. If the deadline were only reported and not enforced, this
    /// test would take ~10 s and fail its elapsed assertion.
    ///
    /// `PATH` is process-global, so the test re-executes itself as a child
    /// process with the fake first on `PATH`; the parent asserts on the
    /// child's outcome. Sibling tests (and the integration binary) never see
    /// the poisoned `PATH`. Unix-only: the fake is a shell script.
    #[cfg(unix)]
    #[test]
    fn git_timeout_fires() {
        use std::os::unix::fs::PermissionsExt;

        if std::env::var_os("SIRIO_GIT_TIMEOUT_TEST").is_none() {
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
                .env("SIRIO_GIT_TIMEOUT_TEST", "1")
                .env(
                    "PATH",
                    format!("{}:{}", fake_dir.display(), path.to_string_lossy()),
                )
                .output()
                .expect("rerun the test with the fake git on PATH");

            let _ = std::fs::remove_dir_all(&scratch);
            // Surface the child's measured elapsed in the parent's output.
            print!("{}", String::from_utf8_lossy(&output.stdout));
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
            &["rev-parse", "HEAD"],
            Path::new("/"),
            Duration::from_millis(200),
        );
        let elapsed = started.elapsed();
        println!("measured elapsed: {elapsed:?}");

        assert!(
            matches!(result, Err(GitError::TimedOut { .. })),
            "expected TimedOut, got {result:?}"
        );
        assert!(
            elapsed < Duration::from_secs(1),
            "timeout took {elapsed:?} — the deadline is not enforced"
        );
    }

    fn scratch_dir() -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("sirio-git-timeout-{}-{unique}", std::process::id()))
    }
}
