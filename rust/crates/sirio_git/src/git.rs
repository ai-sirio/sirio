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
//! `SIRIO_GIT_TIMEOUT_MS` environment variable is unset or unparseable.
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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::GitError;

/// A caller-held handle that cancels a running [`GitRunner::run_streaming`]
/// (or [`GitRunner::run_streaming_cancellable`]) invocation from another
/// thread. Cloning shares the same underlying flag, so the handle that
/// started the command and the handle used to cancel it may live on
/// different threads — e.g. a background worker thread running the git
/// process and the GPUI surface that owns a "Cancel" affordance.
///
/// Setting the flag does not itself kill anything. The runner's poll loop
/// already wakes every [`POLL_INTERVAL`] to check its deadline; cancellation
/// is checked on that same cadence, and once observed the runner kills the
/// whole process group exactly the way a timeout does (see [`kill_tree`]) and
/// returns [`GitError::Cancelled`] instead of the command's output. Calling
/// [`Self::cancel`] after the process has already exited has no effect — the
/// runner reports whatever the process actually did.
#[derive(Clone, Debug, Default)]
pub struct GitCancellationToken(Arc<AtomicBool>);

impl GitCancellationToken {
    /// A fresh, uncancelled token.
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Requests cancellation of whatever invocation this token was passed
    /// to. Idempotent and safe to call from any thread, any number of times,
    /// whether or not a command is currently running.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether [`Self::cancel`] has been called.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

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
const TIMEOUT_ENV_VAR: &str = "SIRIO_GIT_TIMEOUT_MS";

/// How often the runner polls the child while waiting.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// How long the runner waits for the pipe readers to finish after killing
/// the process group, and for the killed child to be reaped, before giving
/// up. Bounded so the deadline stays real even for unkillable processes.
const KILL_GRACE_PERIOD: Duration = Duration::from_millis(100);

/// How long the runner waits for the reader threads to deliver their final
/// buffers after the child has exited *on its own* (no kill, no timeout).
///
/// On Unix the child's descendants die with its process group, so both
/// pipes reach EOF moments after the child exits and a short bound suffices.
/// Windows has no process groups in this picture: git clone's helper
/// subprocesses (`git-upload-pack`, `git-index-pack`) inherit the stderr
/// pipe and can outlive the clone itself by hundreds of milliseconds, so
/// EOF — and with it the retained capture — arrives late. The 100 ms kill
/// grace would abandon the readers first and the capture would come back
/// empty even though every progress line was delivered (observed with the
/// clone-truncation fixture: 15 lines delivered, zero bytes retained). Two
/// seconds covers helper exit while still bounding a genuinely stuck pipe.
#[cfg(windows)]
const STREAM_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(not(windows))]
const STREAM_DRAIN_TIMEOUT: Duration = KILL_GRACE_PERIOD;

/// How many bytes of one stream the runner will retain before it stops
/// accumulating and marks the capture truncated.
///
/// A `git` command can produce unbounded output — `git log -p` over a large
/// history, or `git show` of a generated blob — and the runner holds the whole
/// capture in memory. The cap makes that bounded. It is deliberately large
/// enough that no realistic diff or status listing reaches it, so hitting it
/// means something genuinely pathological.
const DEFAULT_OUTPUT_LIMIT_BYTES: usize = 10 * 1024 * 1024;

/// Overrides [`DEFAULT_OUTPUT_LIMIT_BYTES`], mirroring the timeout override
/// above. Tests use it so they can prove the limit without generating ten
/// megabytes of real output.
const OUTPUT_LIMIT_ENV_VAR: &str = "SIRIO_GIT_OUTPUT_LIMIT_BYTES";

// A per-thread cap that outranks the environment, for the one test that needs
// to prove the limit is really wired into the runner.
//
// It exists because the obvious spelling — `set_var` around the call — is a
// **process-global** mutation, and `cargo test` runs this crate's tests as
// threads in one process. That test held a 512-byte cap for the duration of a
// single `git config --list`, and any other test unlucky enough to shell out
// to git inside that window inherited it. `git init` prints roughly 670 bytes
// of `init.defaultBranch` advice on a machine where that setting is unset, so
// `initializes_a_folder_as_a_git_repository` failed with `OutputTruncated`
// — intermittently, more often under load, and in a crate whose code had not
// changed. It read exactly like a timing flake and was not one.
//
// A thread-local cannot leak that way: each test owns its own thread, and the
// limit is resolved on the calling thread before the reader threads are
// spawned, so the value they capture is the right one.
#[cfg(test)]
thread_local! {
    static OUTPUT_LIMIT_OVERRIDE: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
}

/// Sets (or clears, with `None`) [`OUTPUT_LIMIT_OVERRIDE`] on the calling
/// thread. `pub(crate)` so other modules' tests — e.g. clone.rs's proof that
/// [`crate::GitClone::clone`] surfaces truncation — can force the condition
/// without the process-global `set_var` footgun documented above.
#[cfg(test)]
pub(crate) fn set_output_limit_override_for_test(limit: Option<usize>) {
    OUTPUT_LIMIT_OVERRIDE.with(|cell| cell.set(limit));
}

/// Resolves the byte cap for a single stream. A malformed or zero value falls
/// back to the default rather than disabling the cap, so a typo in the
/// environment cannot quietly restore unbounded buffering.
fn configured_output_limit() -> usize {
    #[cfg(test)]
    if let Some(limit) = OUTPUT_LIMIT_OVERRIDE.with(std::cell::Cell::get) {
        return limit;
    }
    output_limit_from_env(std::env::var(OUTPUT_LIMIT_ENV_VAR).ok().as_deref())
}

fn output_limit_from_env(raw: Option<&str>) -> usize {
    raw.and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_OUTPUT_LIMIT_BYTES)
}

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

/// The completed result returned by the streaming runner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitCommandResult {
    /// Raw stdout captured while the command ran.
    pub stdout: Vec<u8>,
    /// Complete stderr, including the line endings that were used by git.
    pub stderr: String,
    /// The process exit code.
    pub exit_code: i32,
    /// Whether either stream hit the byte cap. When true, `stdout`/`stderr`
    /// hold only the first [`DEFAULT_OUTPUT_LIMIT_BYTES`] bytes of that stream
    /// and a caller that parses them is parsing an incomplete document.
    ///
    /// This is reported rather than raised as an error: the capture is still
    /// useful (progress lines already delivered, a partial diff still renders),
    /// and turning it into an `Err` would change what every existing caller
    /// receives for a command that otherwise succeeded.
    pub truncated: bool,
}

impl GitCommandResult {
    /// Decodes stdout lossily, matching the buffered runner's convenience
    /// accessor while retaining raw bytes in the result.
    pub fn stdout_string(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
}

/// Public entry point for git operations that need live stderr progress.
pub struct GitRunner;

impl GitRunner {
    /// Runs `git <args>` in `cwd`, invoking `on_line` for each non-empty line
    /// of stderr as soon as its CR/LF delimiter arrives. The completed result
    /// is returned only after the child exits and both output streams close.
    pub fn run_streaming<F>(
        args: &[&str],
        cwd: &Path,
        on_line: F,
    ) -> Result<GitCommandResult, GitError>
    where
        F: FnMut(String),
    {
        Self::run_streaming_with_binary(Path::new(GIT_BINARY), args, cwd, on_line)
    }

    /// Testable/configurable form of [`Self::run_streaming`]. The normal
    /// product path resolves `git` through `PATH`; this form also lets callers
    /// choose an explicit executable when a packaged git is required.
    pub fn run_streaming_with_binary<F>(
        binary: &Path,
        args: &[&str],
        cwd: &Path,
        on_line: F,
    ) -> Result<GitCommandResult, GitError>
    where
        F: FnMut(String),
    {
        Self::run_streaming_cancellable(binary, args, cwd, None, on_line)
    }

    /// Cancellable form of [`Self::run_streaming_with_binary`]. When
    /// `cancellation` is `Some` and its token is cancelled while the process
    /// is still running, the process group is killed the same way a timeout
    /// kills it and [`GitError::Cancelled`] is returned instead of the
    /// command's output. `cancellation: None` behaves exactly like
    /// [`Self::run_streaming_with_binary`] — this is its full implementation.
    pub fn run_streaming_cancellable<F>(
        binary: &Path,
        args: &[&str],
        cwd: &Path,
        cancellation: Option<&GitCancellationToken>,
        mut on_line: F,
    ) -> Result<GitCommandResult, GitError>
    where
        F: FnMut(String),
    {
        let mut command = Command::new(binary);
        command
            .args(args)
            .current_dir(cwd)
            // Every parser in this crate (clone progress, status output,
            // branch listing) matches git's English output. Without a fixed
            // locale the progress lines arrive translated — "Ricezione degli
            // oggetti" instead of "Receiving objects" — and the parsed state
            // silently stays empty. git's own scripting docs prescribe
            // LC_ALL=C for exactly this reason.
            .env("LC_ALL", "C")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|error| GitError::Spawn {
            message: error.to_string(),
        })?;

        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let (events_tx, events_rx) = mpsc::channel();
        let output_limit = configured_output_limit();
        let stdout_reader = std::thread::spawn({
            let events_tx = events_tx.clone();
            move || {
                let (captured, truncated) = read_capped(stdout, output_limit);
                let _ = events_tx.send(StreamEvent::StdoutDone(captured, truncated));
            }
        });
        let stderr_reader = std::thread::spawn(move || {
            read_stderr_lines(stderr, output_limit, events_tx);
        });

        let timeout = configured_timeout();
        let deadline = Instant::now() + timeout;
        let mut status = None;
        let mut stdout = None;
        let mut stderr = None;
        while status.is_none() {
            while let Ok(event) = events_rx.try_recv() {
                handle_stream_event(event, &mut on_line, &mut stdout, &mut stderr);
            }

            match child.try_wait() {
                Ok(Some(exit)) => status = Some(exit),
                Ok(None) => {
                    if cancellation.is_some_and(GitCancellationToken::is_cancelled) {
                        kill_tree(&mut child);
                        reap_within_grace(&mut child);
                        drop(events_rx);
                        return Err(GitError::Cancelled {
                            command: args.join(" "),
                        });
                    }
                    if Instant::now() >= deadline {
                        kill_tree(&mut child);
                        reap_within_grace(&mut child);
                        drop(events_rx);
                        return Err(GitError::TimedOut {
                            command: args.join(" "),
                            timeout,
                        });
                    }
                    if let Ok(event) = events_rx.recv_timeout(POLL_INTERVAL) {
                        handle_stream_event(event, &mut on_line, &mut stdout, &mut stderr);
                    }
                }
                Err(error) => {
                    kill_tree(&mut child);
                    reap_within_grace(&mut child);
                    drop(events_rx);
                    return Err(GitError::Spawn {
                        message: error.to_string(),
                    });
                }
            }
        }

        // The process is gone, but the reader threads may still be delivering
        // their final buffers. Drain until both completion events arrive —
        // with [`STREAM_DRAIN_TIMEOUT`]'s bound, which is longer on Windows
        // because helper subprocesses can hold the pipes open past the
        // child's exit. This loop only runs on the natural-exit path: the
        // kill/timeout/cancel paths return above, so the longer bound never
        // delays a call that killed something.
        while stdout.is_none() || stderr.is_none() {
            match events_rx.recv_timeout(STREAM_DRAIN_TIMEOUT) {
                Ok(event) => handle_stream_event(event, &mut on_line, &mut stdout, &mut stderr),
                Err(_) => break,
            }
        }
        if stdout.is_some() && stderr.is_some() {
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
        }

        let exit_code = status.and_then(|status| status.code()).unwrap_or(-1);
        let (stdout_bytes, stdout_truncated) = stdout.unwrap_or_default();
        let (stderr_bytes, stderr_truncated) = stderr.unwrap_or_default();
        let result = GitCommandResult {
            stdout: stdout_bytes,
            stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
            exit_code,
            truncated: stdout_truncated || stderr_truncated,
        };
        if exit_code == 0 {
            Ok(result)
        } else {
            Err(GitError::CommandFailed {
                code: exit_code,
                stderr: result.stderr,
            })
        }
    }
}

/// Free-function spelling for the default streaming runner.
pub fn run_streaming<F>(args: &[&str], cwd: &Path, on_line: F) -> Result<GitCommandResult, GitError>
where
    F: FnMut(String),
{
    GitRunner::run_streaming(args, cwd, on_line)
}

enum StreamEvent {
    StderrLine(String),
    /// Final stderr capture, and whether it hit the byte cap.
    StderrDone(Vec<u8>, bool),
    /// Final stdout capture, and whether it hit the byte cap.
    StdoutDone(Vec<u8>, bool),
}

fn handle_stream_event<F>(
    event: StreamEvent,
    on_line: &mut F,
    stdout: &mut Option<(Vec<u8>, bool)>,
    stderr: &mut Option<(Vec<u8>, bool)>,
) where
    F: FnMut(String),
{
    match event {
        StreamEvent::StderrLine(line) => on_line(line),
        StreamEvent::StderrDone(output, truncated) => *stderr = Some((output, truncated)),
        StreamEvent::StdoutDone(output, truncated) => *stdout = Some((output, truncated)),
    }
}

fn read_stderr_lines(
    mut pipe: impl std::io::Read,
    limit: usize,
    events_tx: mpsc::Sender<StreamEvent>,
) {
    let mut captured = Vec::new();
    let mut truncated = false;
    let mut pending = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = match std::io::Read::read(&mut pipe, &mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(count) => count,
        };
        // Only the retained capture is capped. Progress lines keep being
        // delivered to `on_line` past the limit, because they are consumed as
        // they arrive and never accumulate.
        let room = limit.saturating_sub(captured.len());
        if count > room {
            captured.extend_from_slice(&buffer[..room]);
            truncated = true;
        } else {
            captured.extend_from_slice(&buffer[..count]);
        }
        for &byte in &buffer[..count] {
            if byte == b'\r' || byte == b'\n' {
                if !pending.is_empty() {
                    let line = String::from_utf8_lossy(&pending).into_owned();
                    if events_tx.send(StreamEvent::StderrLine(line)).is_err() {
                        return;
                    }
                    pending.clear();
                }
            } else {
                pending.push(byte);
            }
        }
    }
    if !pending.is_empty() {
        let _ = events_tx.send(StreamEvent::StderrLine(
            String::from_utf8_lossy(&pending).into_owned(),
        ));
    }
    let _ = events_tx.send(StreamEvent::StderrDone(captured, truncated));
}

impl GitOutput {
    pub fn stdout_string(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn is_success(&self) -> bool {
        self.status == Some(0)
    }
}

/// Converts a filesystem path into the string git accepts on its command
/// line.
///
/// On Windows, `std::fs::canonicalize` returns "verbatim" paths carrying
/// the `\\?\` prefix (`\\?\C:\...`, `\\?\UNC\server\share\...`). The
/// prefix is meaningful to the Windows kernel and to Rust's own fs calls —
/// it is what lifts the 260-character path limit — but git does not
/// understand it and rejects the argument outright (`fatal: could not
/// create work tree dir '\\?\C:\...': Invalid argument`). Every path that
/// leaves this crate as a git argument is converted here, so the prefix is
/// stripped from the argv string only. The `Path` itself is never altered,
/// so Rust-side fs operations keep the verbatim form's long-path
/// capability; stripping cannot regress a working invocation either,
/// because git rejects the prefixed form in every case.
///
/// On other platforms the path passes through unchanged, byte for byte.
pub(crate) fn path_arg(path: &Path) -> String {
    let string = path.to_string_lossy();
    #[cfg(windows)]
    {
        if let Some(stripped) = strip_verbatim_prefix(&string) {
            return stripped;
        }
    }
    string.into_owned()
}

/// Strips the Windows `\\?\` verbatim prefix from a path string:
/// `\\?\C:\...` becomes `C:\...` and `\\?\UNC\server\share\...` becomes
/// `\\server\share\...` (the UNC authority is preserved in the double-
/// backslash form git expects). Returns `None` when the path is not
/// verbatim-prefixed.
#[cfg(windows)]
pub(crate) fn strip_verbatim_prefix(path: &str) -> Option<String> {
    let rest = path.strip_prefix(r"\\?\")?;
    if let Some(unc) = rest.strip_prefix("UNC\\") {
        Some(format!(r"\\{unc}"))
    } else {
        Some(rest.to_string())
    }
}

/// The deadline for default-path invocations: the `SIRIO_GIT_TIMEOUT_MS`
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

/// Runs `git <args>` with `cwd` as the working directory, capturing stdout
/// and stderr, within an explicit wall-clock timeout. Never checks the exit
/// code — callers decide what to accept — but surfaces spawn failures as
/// [`GitError::Spawn`] and deadline misses as [`GitError::TimedOut`].
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
    let output_limit = configured_output_limit();
    let stdout_reader = std::thread::spawn(move || {
        let _ = stdout_tx.send(read_capped(stdout, output_limit));
    });
    let stderr_reader = std::thread::spawn(move || {
        let _ = stderr_tx.send(read_capped(stderr, output_limit));
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
    let (stdout, stdout_truncated) = stdout_rx.recv().unwrap_or_default();
    let (stderr, stderr_truncated) = stderr_rx.recv().unwrap_or_default();
    let _ = stdout_reader.join();
    let _ = stderr_reader.join();

    if stdout_truncated || stderr_truncated {
        return Err(GitError::OutputTruncated {
            command: args.join(" "),
            limit: output_limit,
        });
    }

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
    stdout_rx: &mpsc::Receiver<(Vec<u8>, bool)>,
    stderr_rx: &mpsc::Receiver<(Vec<u8>, bool)>,
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

/// Reads a pipe to EOF, best-effort, retaining at most `limit` bytes.
///
/// Returns the captured prefix and whether anything was dropped. Reading
/// *continues* past the limit and the surplus is discarded, rather than the
/// reader returning early: a pipe nobody drains fills its kernel buffer, and
/// git then blocks forever on its next write. Stopping early would trade
/// unbounded memory for a hang, which is the worse of the two.
fn read_capped(mut pipe: impl std::io::Read, limit: usize) -> (Vec<u8>, bool) {
    let mut captured = Vec::new();
    let mut truncated = false;
    let mut chunk = [0_u8; 8192];
    loop {
        let count = match std::io::Read::read(&mut pipe, &mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(count) => count,
        };
        let room = limit.saturating_sub(captured.len());
        if room == 0 {
            truncated = true;
            continue;
        }
        if count > room {
            captured.extend_from_slice(&chunk[..room]);
            truncated = true;
        } else {
            captured.extend_from_slice(&chunk[..count]);
        }
    }
    (captured, truncated)
}

/// Runs `git <args>` in `cwd` and requires one of `accepted` exit codes,
/// returning the full captured output.
pub(crate) fn run_accepting(
    args: &[&str],
    cwd: &Path,
    accepted: &[i32],
) -> Result<GitOutput, GitError> {
    run_accepting_with_timeout(args, cwd, accepted, configured_timeout())
}

/// Like [`run_accepting`], with an explicit wall-clock timeout — for the
/// few invocations that talk to a remote and cannot be held to the local
/// default.
pub(crate) fn run_accepting_with_timeout(
    args: &[&str],
    cwd: &Path,
    accepted: &[i32],
    timeout: Duration,
) -> Result<GitOutput, GitError> {
    let output = run_with_timeout(args, cwd, timeout)?;
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

    /// Proves cancellation is real, not merely reported: a fake git script
    /// that writes its own pid to a file and then streams progress for ~10s
    /// is run through [`GitRunner::run_streaming_cancellable`] on a worker
    /// thread. The test waits for genuine proof the process is running — its
    /// pid file exists *and* at least one progress line has arrived over the
    /// `on_line` callback — before cancelling, so a cancel that raced a
    /// process which never truly started could not pass by accident. It then
    /// asserts both halves of the clause: the runner reports
    /// [`GitError::Cancelled`], and the pid captured from the child is
    /// actually gone from the process table afterward (`kill(pid, 0)`
    /// fails), not just that the runner stopped waiting on it.
    ///
    /// Before this test existed, no code path in this crate (or anywhere in
    /// the workspace — the whole `Cancel*`/`cancel` vocabulary was absent)
    /// could stop a running git operation at all: cancelling meant nothing
    /// to call, so this exact test could not even be written against the
    /// pre-fix runner.
    #[cfg(unix)]
    #[test]
    fn git_cancellation_kills_the_child_and_reports_cancelled() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::Mutex;

        let scratch = scratch_dir();
        std::fs::create_dir_all(&scratch).expect("create scratch dir");
        let pid_file = scratch.join("child.pid");
        let script = scratch.join("slow-git.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho $$ > '{}'\ni=0\nwhile [ $i -lt 100 ]; do\n  \
                 echo \"Receiving objects: $i% (1234/5678)\" 1>&2\n  i=$((i + 2))\n  \
                 sleep 0.1\ndone\n",
                pid_file.display()
            ),
        )
        .expect("write fake git script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("make fake git script executable");

        let token = GitCancellationToken::new();
        let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        let handle = {
            let token = token.clone();
            let lines = Arc::clone(&lines);
            let script = script.clone();
            let cwd = scratch.clone();
            std::thread::spawn(move || {
                GitRunner::run_streaming_cancellable(&script, &[], &cwd, Some(&token), {
                    let lines = Arc::clone(&lines);
                    move |line| lines.lock().unwrap().push(line)
                })
            })
        };

        // Proof #1: the process actually started — its own pid landed on
        // disk, written from inside the running script.
        let start_deadline = Instant::now() + Duration::from_secs(5);
        let mut pid: Option<i32> = None;
        while Instant::now() < start_deadline {
            if let Ok(text) = std::fs::read_to_string(&pid_file)
                && let Ok(parsed) = text.trim().parse::<i32>()
            {
                pid = Some(parsed);
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let pid = pid.expect("fake git child never wrote its pid — it did not start");

        // Proof #2: it is genuinely mid-flight, not merely spawned — at
        // least one progress line reached the streaming callback.
        let progress_deadline = Instant::now() + Duration::from_secs(5);
        while lines.lock().unwrap().is_empty() && Instant::now() < progress_deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !lines.lock().unwrap().is_empty(),
            "no progress line arrived before cancellation — the process was never observed running"
        );

        token.cancel();

        let result = handle.join().expect("runner thread panicked");
        let _ = std::fs::remove_dir_all(&scratch);

        assert!(
            matches!(result, Err(GitError::Cancelled { .. })),
            "expected Cancelled, got {result:?}"
        );

        // The reported outcome alone is not proof: confirm the pid is
        // actually gone from the process table, not merely that the runner
        // gave up waiting on it.
        let mut still_alive = true;
        let reap_deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < reap_deadline {
            let alive = unsafe { libc::kill(pid, 0) } == 0;
            if !alive {
                still_alive = false;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !still_alive,
            "pid {pid} answers kill(pid, 0) after cancellation — the child was not killed"
        );
    }

    /// Exercises the "launch failure" leg of the row's VERIFY clause (a
    /// missing git command), which no existing test in this crate covered:
    /// every prior test either ran the real system `git` or a fake script
    /// that itself exists on disk. Pointing the runner at a path with
    /// nothing there must report [`GitError::Spawn`], not panic or hang.
    #[test]
    fn missing_git_binary_reports_spawn_failure() {
        let scratch = scratch_dir();
        let missing = scratch.join("this-binary-does-not-exist-git");
        let result =
            GitRunner::run_streaming_with_binary(&missing, &["--version"], Path::new("/"), |_| {});
        assert!(
            matches!(result, Err(GitError::Spawn { .. })),
            "expected Spawn for a missing binary, got {result:?}"
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

    #[test]
    fn output_limit_from_env_parses_override_and_falls_back() {
        assert_eq!(
            output_limit_from_env(Some("4096")),
            4096,
            "a numeric value is a byte cap"
        );
        assert_eq!(
            output_limit_from_env(None),
            DEFAULT_OUTPUT_LIMIT_BYTES,
            "unset falls back to the production default"
        );
        assert_eq!(
            output_limit_from_env(Some("not-a-number")),
            DEFAULT_OUTPUT_LIMIT_BYTES,
            "an unparseable value falls back rather than disabling the cap"
        );
        assert_eq!(
            output_limit_from_env(Some("0")),
            DEFAULT_OUTPUT_LIMIT_BYTES,
            "zero must not mean 'unbounded' -- a typo cannot restore unbounded buffering"
        );
    }

    /// F-GIT-RUN-01: the runner enforces an output limit, and drains the pipe
    /// past it instead of stopping.
    ///
    /// The drain half is the part worth testing: a reader that returned at the
    /// cap would leave the pipe full, and git would block forever on its next
    /// write — trading unbounded memory for a hang. Reading a source longer
    /// than the cap and reaching EOF at all is what proves the surplus was
    /// consumed rather than abandoned.
    #[test]
    fn read_capped_retains_a_prefix_and_drains_the_rest() {
        let source = vec![b'x'; 10_000];

        let (captured, truncated) = read_capped(source.as_slice(), 4096);
        assert_eq!(captured.len(), 4096, "retains exactly the cap");
        assert!(truncated, "and reports that it dropped the surplus");

        let (captured, truncated) = read_capped(source.as_slice(), 10_000);
        assert_eq!(captured.len(), 10_000, "an exact fit is not truncation");
        assert!(!truncated);

        let (captured, truncated) = read_capped(source.as_slice(), 20_000);
        assert_eq!(captured.len(), 10_000, "a roomy cap retains everything");
        assert!(!truncated);
    }

    /// The limit is not merely defined, it is wired into the streaming runner:
    /// a real `git` command whose output exceeds the cap comes back with the
    /// prefix and `truncated` set, rather than the whole document.
    ///
    /// The cap is applied through [`OUTPUT_LIMIT_OVERRIDE`], a thread-local,
    /// **not** through `set_var`. This test used to set the process-global
    /// environment override for the duration of the call below, which any
    /// concurrently-running test that shelled out to git inherited — see that
    /// constant's own comment for the failure it caused.
    #[test]
    fn run_streaming_reports_a_truncated_capture() {
        let directory = scratch_dir();
        std::fs::create_dir_all(&directory).expect("create scratch dir");

        let init = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&directory)
            .status();
        if !matches!(init, Ok(status) if status.success()) {
            let _ = std::fs::remove_dir_all(&directory);
            return; // no usable git on this box; the pure tests still cover the logic
        }

        OUTPUT_LIMIT_OVERRIDE.with(|limit| limit.set(Some(512)));
        let result = GitRunner::run_streaming(&["config", "--list"], &directory, |_| {});
        OUTPUT_LIMIT_OVERRIDE.with(|limit| limit.set(None));
        let _ = std::fs::remove_dir_all(&directory);

        let result = result.expect("git config --list exits 0");
        assert!(
            result.stdout.len() <= 512,
            "the capture must not exceed the configured cap, got {} bytes",
            result.stdout.len()
        );
        if result.stdout.len() == 512 {
            assert!(
                result.truncated,
                "a capture that filled the cap must report itself truncated"
            );
        }
    }

    /// The regression guard for the leak itself, stated as the property that
    /// was violated rather than as the symptom it produced.
    ///
    /// A sibling test capping the output at 512 bytes must not change what any
    /// other test's thread sees, because `cargo test` runs them concurrently in
    /// one process. When the cap was installed with `set_var`, it did, and
    /// `initializes_a_folder_as_a_git_repository` failed with `OutputTruncated`
    /// whenever its `git init` — around 670 bytes of `init.defaultBranch`
    /// advice on a box where that setting is unset — overlapped the window.
    #[test]
    fn the_output_limit_override_is_confined_to_its_own_thread() {
        OUTPUT_LIMIT_OVERRIDE.with(|limit| limit.set(Some(512)));

        let seen_by_another_thread = std::thread::spawn(configured_output_limit)
            .join()
            .expect("the probe thread must not panic");

        let seen_here = configured_output_limit();
        OUTPUT_LIMIT_OVERRIDE.with(|limit| limit.set(None));

        assert_eq!(
            seen_here, 512,
            "the override must apply on the thread that set it, or this test \
             proves nothing about confinement"
        );
        assert_eq!(
            seen_by_another_thread, DEFAULT_OUTPUT_LIMIT_BYTES,
            "another thread must still see the default; a process-global \
             override is what made a 670-byte `git init` fail in an unrelated \
             test"
        );
    }

    /// Verbatim-prefix paths are the exact spelling `std::fs::canonicalize`
    /// produces on Windows, and the prefix is what git rejected on real
    /// clones — see [`path_arg`]'s own doc comment for the full story. The
    /// non-Windows half of this test pins the byte-identity contract.
    #[test]
    fn path_arg_passes_ordinary_paths_through_unchanged() {
        assert_eq!(path_arg(Path::new("/tmp/note.md")), "/tmp/note.md");
        assert_eq!(path_arg(Path::new("relative/path")), "relative/path");
    }

    #[cfg(windows)]
    #[test]
    fn path_arg_strips_the_windows_verbatim_prefix() {
        assert_eq!(
            path_arg(Path::new(r"\\?\C:\Users\me\project")),
            r"C:\Users\me\project"
        );
        assert_eq!(
            path_arg(Path::new(r"\\?\UNC\server\share\repo")),
            r"\\server\share\repo"
        );
        assert_eq!(
            path_arg(Path::new("plain\\relative\\path")),
            "plain\\relative\\path",
            "a backslash path without the prefix must pass through"
        );
    }

    fn scratch_dir() -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("sirio-git-timeout-{}-{unique}", std::process::id()))
    }
}
