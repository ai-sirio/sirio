//! Child-process supervision shared by Sirio's chat transports.
//!
//! The ACP client and the native Claude transport both launch an agent as a
//! child process, watch its stderr, enforce an idle window on a turn and build
//! a death report when it goes away. Those helpers live here so a transport
//! does not have to reach into another one's crate for them.

use async_process::Child;
use futures::executor::block_on;
use sirio_chat_vocab::{ChatError, TimeoutOperation};
use std::collections::VecDeque;
use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// How long the agent may stay **silent** during a turn before it counts as
/// hung. This is deliberately an idle window, not a cap on the turn: an
/// agentic turn that runs for an hour is normal as long as it keeps
/// reporting, and killing a working agent loses the whole session, not just
/// the turn. The clock restarts on every notification the agent sends.
pub const PROMPT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
pub const PERMISSION_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const CHILD_REAP_TIMEOUT: Duration = Duration::from_secs(2);

pub type ChildHandle = Arc<Mutex<Option<Child>>>;

pub type ActivityClock = Arc<Mutex<std::time::Instant>>;

pub fn touch_activity(clock: &ActivityClock) {
    if let Ok(mut at) = clock.lock() {
        *at = std::time::Instant::now();
    }
}

pub fn idle_for(clock: &ActivityClock) -> Duration {
    clock
        .lock()
        .map(|at| at.elapsed())
        .unwrap_or(Duration::ZERO)
}

pub type StderrTail = Arc<Mutex<VecDeque<String>>>;

/// How much of the agent's stderr is kept for the death report. Enough for a
/// stack trace's first frames, small enough to sit inside an error message.
const MAX_STDERR_TAIL: usize = 20;

pub fn push_stderr_tail(tail: &StderrTail, line: String) {
    if let Ok(mut tail) = tail.lock() {
        if tail.len() >= MAX_STDERR_TAIL {
            tail.pop_front();
        }
        tail.push_back(line);
    }
}

/// How long a death report waits for the stderr drain thread to reach EOF
/// before giving up on the agent's last words. The two pipes close together
/// when the process dies, but they are read by different threads: without
/// this the report is routinely built from a tail the drain has not finished
/// filling, and the missing line is always the final one — the only line that
/// says anything about why the agent went away.
const STDERR_DRAIN_GRACE: Duration = Duration::from_millis(200);

pub async fn wait_for_stderr_drain(drained: &Arc<AtomicBool>) {
    let deadline = std::time::Instant::now() + STDERR_DRAIN_GRACE;
    while !drained.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
        async_io::Timer::after(Duration::from_millis(5)).await;
    }
}

pub fn stderr_tail_report(tail: &StderrTail) -> String {
    let lines = tail
        .lock()
        .map(|tail| tail.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    if lines.is_empty() {
        return String::new();
    }
    format!(
        "\nagent stderr (last {} lines):\n{}",
        lines.len(),
        lines.join("\n")
    )
}

/// The bound on `AcpClient::mcp_warnings` — a long session's stderr
/// should not grow this without limit.
pub const MAX_MCP_WARNINGS: usize = 20;

pub fn push_mcp_warning(cell: &Arc<Mutex<Vec<String>>>, warning: String) {
    if let Ok(mut warnings) = cell.lock() {
        if warnings.len() >= MAX_MCP_WARNINGS {
            warnings.remove(0);
        }
        warnings.push(warning);
    }
}

/// Conservative on purpose: "mcp" alone also matches benign startup logging
/// some adapters emit on a successful connection (e.g. "Connected to MCP
/// server 'foo'"), so a failure-shaped word must co-occur before this
/// counts as a warning rather than routine noise (F-CHAT-33).
pub fn looks_like_mcp_warning(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if !lower.contains("mcp") {
        return false;
    }
    const FAILURE_WORDS: [&str; 10] = [
        "error",
        "fail",
        "warn",
        "unable",
        "could not",
        "refused",
        "timed out",
        "timeout",
        "invalid",
        "not found",
    ];
    FAILURE_WORDS.iter().any(|word| lower.contains(word))
}

pub fn record_timeout(reason: &Arc<Mutex<Option<ChatError>>>, timeout: ChatError) {
    if let Ok(mut reason) = reason.lock() {
        *reason = Some(timeout);
    }
}

async fn terminate_and_reap(child: &ChildHandle) -> Option<ExitStatus> {
    let Some(mut child) = child.lock().ok().and_then(|mut child| child.take()) else {
        return None;
    };
    #[cfg(unix)]
    if let Ok(pid) = i32::try_from(child.id()) {
        // `AcpAgent::spawn_process` makes the child a process-group leader;
        // terminate wrappers such as npx together with their real agent.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let wait = child.status();
    let timeout = async_io::Timer::after(CHILD_REAP_TIMEOUT);
    futures::pin_mut!(wait, timeout);
    match futures::future::select(wait, timeout).await {
        futures::future::Either::Left((status, _)) => status.ok(),
        futures::future::Either::Right((_, _)) => None,
    }
}

pub fn terminate_and_reap_blocking(child: &ChildHandle) -> Option<ExitStatus> {
    block_on(terminate_and_reap(child))
}

/// What the OS said about the agent's exit, recorded by whoever reaped it.
/// [`terminate_and_reap`] takes the child out of its handle, so a report
/// built afterwards has nothing left to ask.
pub type ExitStatusSlot = Arc<Mutex<Option<ExitStatus>>>;

pub fn record_exit_status(slot: &ExitStatusSlot, status: Option<ExitStatus>) {
    if let Some(status) = status
        && let Ok(mut slot) = slot.lock()
    {
        slot.get_or_insert(status);
    }
}

/// How long a death report waits for the kernel to make the agent's exit
/// status readable. Stdout reaching EOF means the process is on its way out,
/// but the two events are not the same instant.
const CHILD_EXIT_PROBE: Duration = Duration::from_millis(200);

/// How the agent process ended, phrased for the error the user reads. An
/// empty string when nothing has observed the exit yet — a still-running
/// agent that merely closed its stdout is a real case, and guessing a cause
/// there would be worse than saying nothing.
pub async fn child_exit_report(child: &ChildHandle, recorded: &ExitStatusSlot) -> String {
    let deadline = std::time::Instant::now() + CHILD_EXIT_PROBE;
    loop {
        if let Some(status) = recorded.lock().ok().and_then(|slot| *slot) {
            return format!(" ({})", describe_exit(status));
        }
        if let Some(status) = try_child_status(child) {
            record_exit_status(recorded, Some(status));
            return format!(" ({})", describe_exit(status));
        }
        if std::time::Instant::now() >= deadline {
            return String::new();
        }
        async_io::Timer::after(Duration::from_millis(5)).await;
    }
}

/// Non-blocking so the child lock is never held across an await — a
/// `MutexGuard` alive over one would make the connection's futures `!Send`.
fn try_child_status(child: &ChildHandle) -> Option<ExitStatus> {
    let mut guard = child.lock().ok()?;
    let child = guard.as_mut()?;
    child.try_status().ok().flatten()
}

pub fn describe_exit(status: ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("agent killed by signal {signal}");
        }
    }
    match status.code() {
        Some(code) => format!("agent exited with code {code}"),
        None => "agent exited without reporting a status".to_string(),
    }
}

/// The deadline Sirio itself enforced, when one fired. Without this the turn
/// error blames the transport for a kill Sirio decided on.
pub fn timeout_cause(reason: &Arc<Mutex<Option<ChatError>>>) -> String {
    match reason.lock().ok().and_then(|reason| reason.clone()) {
        Some(ChatError::Timeout {
            operation: TimeoutOperation::Prompt,
            duration,
        }) => format!(
            "\nSirio reaped the agent after {}s without a single update from it (prompt watchdog)",
            duration.as_secs()
        ),
        Some(ChatError::Timeout {
            operation,
            duration,
        }) => format!(
            "\nSirio's {operation:?} wait expired after {}s",
            duration.as_secs()
        ),
        _ => String::new(),
    }
}

pub fn detach_worker(worker: JoinHandle<()>) {
    let _ = thread::spawn(move || {
        let _ = worker.join();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_mcp_warning_requires_mcp_and_a_failure_word() {
        assert!(looks_like_mcp_warning(
            r#"Error: failed to connect to MCP server "docs": connection refused"#
        ));
        assert!(looks_like_mcp_warning("MCP server 'search' timed out"));
        assert!(!looks_like_mcp_warning(
            "Connected to MCP server 'docs' successfully"
        ));
        assert!(!looks_like_mcp_warning("something else entirely failed"));
        assert!(!looks_like_mcp_warning(""));
    }
}
