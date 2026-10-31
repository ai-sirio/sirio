//! Claude Code driven over its own stdio protocol, in place of an ACP
//! wrapper.
//!
//! The shape mirrors the ACP client next door — one worker thread, a
//! command channel in, an unbounded event channel out, four bounded waits —
//! and, crucially, emits the same [`ChatEvent`](crate::ChatEvent) values. The
//! chat surface is written against that vocabulary, so which transport a
//! tab uses is invisible above this line.

mod events;
mod permission;
mod prompt;
mod trace;
mod worker;

use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::Ordering, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Result, anyhow};
use sirio_claude::{ContextBreakdown, McpAuthStart, McpServer, RewindOutcome};

use crate::{
    ChatError, ChildHandle, EventStream, ModeCatalog, ModelCatalog, SHUTDOWN_TIMEOUT,
    TimeoutOperation, terminate_and_reap_blocking,
};

/// ACP's startup budget is 120 s because `npx` may have to download a
/// package before the first protocol byte exists. Nothing is downloaded
/// here: the binary is already on PATH, and a handshake measured at 0.73 s
/// has no business taking half a minute.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

/// A `claude` to launch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaudeLaunch {
    /// The resolved executable.
    pub program: PathBuf,
    /// A Claude session id to continue, when this tab is being restored.
    pub resume: Option<String>,
    /// Arguments placed *before* the protocol flags. Empty in production;
    /// a test fixture uses it to put a script in front of `python3`'s own
    /// arguments. Not `pub`, so no production path can set it by accident.
    pub(crate) prefix_args: Vec<String>,
}

impl ClaudeLaunch {
    /// A production launch of the `claude` at `program`.
    #[must_use]
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            resume: None,
            prefix_args: Vec::new(),
        }
    }

    /// Continue a previous Claude session instead of starting a new one.
    #[must_use]
    pub fn resuming(mut self, session_id: impl Into<String>) -> Self {
        self.resume = Some(session_id.into());
        self
    }

    /// A launch whose program takes arguments of its own first — the door
    /// for a fixture that is a script rather than a binary.
    #[must_use]
    pub fn fixture(program: impl Into<PathBuf>, prefix_args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            resume: None,
            prefix_args,
        }
    }
}

/// A live connection to one `claude` subprocess.
pub struct ClaudeClient {
    command_tx: async_channel::Sender<worker::Command>,
    event_tx: Option<async_channel::Sender<crate::ChatEvent>>,
    child: ChildHandle,
    shared: Arc<worker::Shared>,
    worker: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for ClaudeClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaudeClient")
            .field("session_id", &self.session_id())
            .finish_non_exhaustive()
    }
}

impl ClaudeClient {
    /// Launches `claude`, completes the handshake, and returns the client
    /// with its event stream.
    pub fn launch(launch: ClaudeLaunch, cwd: impl AsRef<Path>) -> Result<(Self, EventStream)> {
        Self::launch_with_timeouts(
            launch,
            cwd,
            STARTUP_TIMEOUT,
            crate::PROMPT_TIMEOUT,
            crate::PERMISSION_TIMEOUT,
        )
    }

    /// [`Self::launch`] with every deadline named, so a test can drive the
    /// timeout paths without waiting minutes for them.
    pub fn launch_with_timeouts(
        launch: ClaudeLaunch,
        cwd: impl AsRef<Path>,
        startup_timeout: Duration,
        prompt_timeout: Duration,
        permission_timeout: Duration,
    ) -> Result<(Self, EventStream)> {
        let cwd = cwd.as_ref().to_path_buf();
        let (command_tx, command_rx) = async_channel::unbounded();
        let (event_tx, event_rx) = async_channel::unbounded();
        let shared = Arc::new(worker::Shared::default());
        let (startup_tx, startup_rx) = mpsc::sync_channel(2);

        let worker_shared = Arc::clone(&shared);
        let worker_events = event_tx.clone();
        let worker = thread::Builder::new()
            .name("sirio-claude".into())
            .spawn(move || {
                worker::run(worker::Config {
                    launch,
                    cwd,
                    command_rx,
                    event_tx: worker_events,
                    startup_tx,
                    shared: worker_shared,
                    prompt_timeout,
                    permission_timeout,
                });
            })?;

        let deadline = std::time::Instant::now() + startup_timeout;
        let mut child: Option<ChildHandle> = None;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match startup_rx.recv_timeout(remaining) {
                Ok(worker::Startup::Process(handle)) => child = Some(handle),
                Ok(worker::Startup::Ready) => break,
                Ok(worker::Startup::Failed(error)) => {
                    if let Some(child) = child.take() {
                        terminate_and_reap_blocking(&child);
                    }
                    crate::detach_worker(worker);
                    return Err(anyhow::Error::new(error));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Some(child) = child.take() {
                        terminate_and_reap_blocking(&child);
                    }
                    crate::detach_worker(worker);
                    return Err(anyhow::Error::new(ChatError::Timeout {
                        operation: TimeoutOperation::Startup,
                        duration: startup_timeout,
                    }));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Some(child) = child.take() {
                        terminate_and_reap_blocking(&child);
                    }
                    crate::detach_worker(worker);
                    return Err(anyhow::Error::new(ChatError::Transport(
                        "the Claude worker exited before its handshake".into(),
                    )));
                }
            }
        }
        let child = child.ok_or_else(|| anyhow!("the Claude worker did not expose its process"))?;

        Ok((
            Self {
                command_tx,
                event_tx: Some(event_tx),
                child,
                shared,
                worker: Some(worker),
            },
            event_rx,
        ))
    }

    /// Sends a plain-text turn. [`Self::prompt_content`] is the full
    /// form; this is the one the integration tests drive.
    pub fn prompt(&self, text: impl Into<String>) -> Result<()> {
        let uuid = new_uuid();
        // No mentions, so no working directory is needed to resolve them.
        let line = prompt::user_message(&text.into(), &[], &[], Path::new(""), &uuid);
        self.command_tx
            .send_blocking(worker::Command::Prompt { line, uuid })
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))
    }

    /// The Claude session id, named at launch rather than by the first
    /// turn: Sirio mints it before the child exists and passes it as
    /// `--session-id`, which is what makes a tab resumable even if the CLI
    /// dies before writing an `init` line. The CLI stays the authority —
    /// an `init` naming another session overwrites it. `None` only before
    /// the first launch attempt.
    #[must_use]
    pub fn session_id(&self) -> Option<String> {
        self.shared.session_id()
    }

    /// The Claude Code version this session reported.
    #[must_use]
    pub fn claude_version(&self) -> Option<String> {
        self.shared.claude_version()
    }

    /// The model selector from the handshake.
    #[must_use]
    pub fn model_catalog(&self) -> Option<ModelCatalog> {
        self.shared.model_catalog()
    }

    /// What the session says about fast mode, or `None` when it never
    /// mentioned it — an older CLI, or a build that has no such feature.
    #[must_use]
    pub fn fast_mode(&self) -> Option<sirio_claude::FastMode> {
        self.shared.fast_mode()
    }

    /// Turn fast mode on or off. The cell only moves once the CLI agrees.
    pub fn set_fast_mode(&self, enabled: bool) -> Result<()> {
        self.command_tx
            .send_blocking(worker::Command::SetFastMode(enabled))
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))
    }

    /// What this session chose for the reasoning it receives.
    #[must_use]
    pub fn thinking_display(&self) -> crate::ThinkingDisplay {
        self.shared.thinking_display()
    }

    /// Choose it. The cell only moves once the CLI agrees.
    pub fn set_thinking_display(&self, display: Option<String>) -> Result<()> {
        self.command_tx
            .send_blocking(worker::Command::SetThinkingDisplay(display))
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))
    }

    /// The mode selector, re-read live: `set_permission_mode` and the CLI's
    /// own `system/status` both land here.
    #[must_use]
    pub fn mode_catalog(&self) -> Option<ModeCatalog> {
        self.shared.mode_catalog()
    }

    /// Whether the launch's `--resume` id was refused and the session is
    /// fresh instead. The surface clears its stored id when this is true,
    /// so the next restart does not retry a session that is gone.
    #[must_use]
    pub fn resumed_session_refused(&self) -> bool {
        self.shared.resumed_session_refused()
    }

    /// MCP servers that failed to connect, as sentences (F-CHAT-33).
    #[must_use]
    pub fn mcp_warnings(&self) -> Vec<String> {
        self.shared.mcp_warnings()
    }

    /// The notices posted since the last read, draining them. Read when an
    /// `OtherSessionUpdate` named `SessionNotice` arrives: the queue is
    /// what keeps them in step with the rows around them.
    pub fn take_notices(&self) -> Vec<crate::SessionNotice> {
        self.shared.take_notices()
    }

    /// How many background tasks the CLI last reported running.
    #[must_use]
    pub fn background_task_count(&self) -> usize {
        self.shared.background_task_count()
    }

    /// Stops the agent and waits for the worker to finish.
    pub fn shutdown(&mut self) -> Result<()> {
        let Some(worker) = self.worker.take() else {
            self.event_tx.take();
            return Ok(());
        };
        let (ack_tx, ack_rx) = mpsc::sync_channel(0);
        if let Err(error) = self
            .command_tx
            .send_blocking(worker::Command::Shutdown(ack_tx))
        {
            terminate_and_reap_blocking(&self.child);
            let _ = worker.join();
            self.event_tx.take();
            return Err(anyhow!("the Claude worker is not running: {error}"));
        }
        let result = match ack_rx.recv_timeout(SHUTDOWN_TIMEOUT) {
            Ok(()) => {
                let _ = worker.join();
                Ok(())
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                terminate_and_reap_blocking(&self.child);
                let _ = worker.join();
                Err(anyhow::Error::new(ChatError::Timeout {
                    operation: TimeoutOperation::Shutdown,
                    duration: SHUTDOWN_TIMEOUT,
                }))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = worker.join();
                Err(anyhow::Error::new(ChatError::Transport(
                    "the Claude worker exited during shutdown".into(),
                )))
            }
        };
        self.event_tx.take();
        result
    }

    /// Sends a turn made of text, file mentions and images.
    pub fn prompt_content(
        &self,
        text: impl Into<String>,
        mention_paths: Vec<String>,
        images: Vec<crate::ImageAttachment>,
        cwd: impl AsRef<Path>,
    ) -> Result<()> {
        let uuid = new_uuid();
        let line = prompt::user_message(&text.into(), &mention_paths, &images, cwd.as_ref(), &uuid);
        if line["message"]["content"]
            .as_array()
            .is_none_or(|blocks| blocks.is_empty())
        {
            // Nothing to say and nothing attached: sending an empty turn
            // would burn a turn and return nothing.
            return Ok(());
        }
        self.command_tx
            .send_blocking(worker::Command::Prompt { line, uuid })
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))
    }

    /// Switches model. `config_id` is accepted for signature parity with
    /// the ACP client and is always `"model"` here.
    pub fn set_model(&self, _config_id: impl Into<String>, value: impl Into<String>) -> Result<()> {
        self.command_tx
            .send_blocking(worker::Command::SetModel(value.into()))
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))
    }

    /// Sets a non-model option. Only `"effort"` exists on this transport;
    /// anything else is refused rather than silently dropped.
    pub fn set_config_option(
        &self,
        option_id: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<()> {
        let option_id = option_id.into();
        if option_id != "effort" {
            return Err(anyhow!("Claude Code has no `{option_id}` option"));
        }
        let value = value.into();
        // "default" is the surface's word for "no override"; the wire
        // spells that as a null effort level.
        let level = (value != "default").then_some(value);
        self.command_tx
            .send_blocking(worker::Command::SetEffort(level))
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))
    }

    /// Switches permission mode.
    pub fn set_mode(&self, mode_id: impl Into<String>) -> Result<()> {
        self.command_tx
            .send_blocking(worker::Command::SetMode(mode_id.into()))
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))
    }

    /// Ends the turn in flight.
    pub fn cancel(&self) -> Result<()> {
        self.command_tx
            .send_blocking(worker::Command::Cancel)
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))
    }

    /// Answers a permission card. For a structured question, `option_id`
    /// is the chosen label or the typed text.
    pub fn respond_permission(&self, request_id: u64, option_id: impl Into<String>) -> Result<()> {
        self.command_tx
            .send_blocking(worker::Command::RespondPermission {
                request_id,
                choice: worker::PermissionChoice::Selected(option_id.into()),
            })
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))
    }

    /// Withdraws a card without choosing, which the agent reads as a
    /// refusal (F-CHAT-25).
    pub fn cancel_permission(&self, request_id: u64) -> Result<()> {
        self.command_tx
            .send_blocking(worker::Command::RespondPermission {
                request_id,
                choice: worker::PermissionChoice::Cancelled,
            })
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))
    }

    /// The id of the last turn this client sent, which a rewind names.
    #[must_use]
    pub fn last_user_message_id(&self) -> Option<String> {
        self.shared.last_user_message_id()
    }

    /// Asks the agent to restore tracked files to their state at a turn.
    /// With `dry_run` the CLI reports what it would change and touches
    /// nothing. Blocking, with a bounded wait: a rewind is a question with
    /// one answer, not a turn to stream.
    pub fn rewind_files(&self, user_message_id: &str, dry_run: bool) -> Result<RewindOutcome> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send_blocking(worker::Command::RewindFiles {
                user_message_id: user_message_id.to_string(),
                dry_run,
                reply: reply_tx,
            })
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))?;
        match reply_rx.recv_timeout(Duration::from_secs(30)) {
            Ok(Ok(outcome)) => Ok(outcome),
            Ok(Err(error)) => Err(anyhow!("rewind_files failed: {error}")),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                Err(anyhow!("rewind_files timed out after 30s"))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(anyhow!(
                "the Claude worker is not running: rewind went unanswered"
            )),
        }
    }

    /// The CLI's breakdown of what fills the window, as of the last turn's
    /// end or the last [`Self::refresh_context`] it answered.
    #[must_use]
    pub fn context_breakdown(&self) -> Option<ContextBreakdown> {
        self.shared.context_breakdown()
    }

    /// Asks the CLI for its breakdown without a turn. Not blocking: the
    /// answer is stored and announced with an `OtherSessionUpdate` named
    /// `ContextBreakdown`, and a CLI that refuses leaves everything as it was.
    pub fn refresh_context(&self) -> Result<()> {
        self.command_tx
            .send_blocking(worker::Command::RefreshContext)
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))
    }

    /// The connectors: the list `system/init` named until a status answer
    /// has been read, that answer after.
    #[must_use]
    pub fn mcp_servers(&self) -> Option<Vec<McpServer>> {
        self.shared.mcp_servers()
    }

    /// Asks the CLI for each connector's state and origin, and keeps the
    /// answer. Blocking, bounded at 10 s.
    pub fn refresh_mcp_status(&self) -> Result<Vec<McpServer>> {
        self.mcp_control().refresh_status()
    }

    /// Asks the CLI to connect a server again. Blocking, bounded at 30 s; a
    /// refusal carries the CLI's own sentence.
    pub fn mcp_reconnect(&self, server: &str) -> Result<()> {
        self.mcp_control().reconnect(server)
    }

    /// Starts a server's sign-in and returns the page to open. Blocking,
    /// bounded at 30 s. An empty address is no address.
    pub fn mcp_authenticate(&self, server: &str) -> Result<McpAuthStart> {
        self.mcp_control().authenticate(server)
    }

    /// A handle that makes the three connector calls from another thread.
    /// The client itself is not `Clone`, and the calls block.
    #[must_use]
    pub fn mcp_control(&self) -> McpControl {
        McpControl {
            command_tx: self.command_tx.clone(),
        }
    }
}

/// The connector calls of one session, detached from the client so a caller
/// can make them off its own thread. Every call blocks until the CLI answers
/// or its bound ends, so none belongs on a UI thread. A handle outliving its
/// session answers that the worker is not running.
#[derive(Clone)]
pub struct McpControl {
    command_tx: async_channel::Sender<worker::Command>,
}

impl std::fmt::Debug for McpControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpControl").finish_non_exhaustive()
    }
}

impl McpControl {
    /// Asks the CLI for each connector's state and origin, and keeps the
    /// answer. Blocking, bounded at 10 s.
    pub fn refresh_status(&self) -> Result<Vec<McpServer>> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send_blocking(worker::Command::McpStatus { reply: reply_tx })
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))?;
        wait_for_reply(&reply_rx, "mcp_status", Duration::from_secs(10))
    }

    /// Asks the CLI to connect a server again. Blocking, bounded at 30 s; a
    /// refusal carries the CLI's own sentence.
    pub fn reconnect(&self, server: &str) -> Result<()> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send_blocking(worker::Command::McpReconnect {
                server: server.to_string(),
                reply: reply_tx,
            })
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))?;
        wait_for_reply(&reply_rx, "mcp_reconnect", Duration::from_secs(30))
    }

    /// Starts a server's sign-in and returns the page to open. Blocking,
    /// bounded at 30 s. An empty address is no address.
    pub fn authenticate(&self, server: &str) -> Result<McpAuthStart> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send_blocking(worker::Command::McpAuthenticate {
                server: server.to_string(),
                reply: reply_tx,
            })
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))?;
        let mut start = wait_for_reply(&reply_rx, "mcp_authenticate", Duration::from_secs(30))?;
        if start.auth_url.as_deref() == Some("") {
            start.auth_url = None;
        }
        Ok(start)
    }
}

/// One control request's answer, bounded: the worker's `Err` is the CLI's own
/// sentence, and silence is a timeout rather than a hang.
fn wait_for_reply<T>(
    reply_rx: &mpsc::Receiver<Result<T, String>>,
    verb: &str,
    timeout: Duration,
) -> Result<T> {
    match reply_rx.recv_timeout(timeout) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(anyhow!("{verb} failed: {error}")),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(anyhow!(
            "{verb} timed out after {}s",
            timeout.as_secs()
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(anyhow!(
            "the Claude worker is not running: {verb} went unanswered"
        )),
    }
}

impl Drop for ClaudeClient {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

/// A v4-shaped identifier for one turn. It never leaves this session — the
/// CLI only ever compares it to ids Sirio itself sent — so a monotonic
/// source is enough, and it keeps the dependency graph unchanged.
fn new_uuid() -> String {
    use std::sync::atomic::AtomicU64;
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos() as u64)
        .unwrap_or_default();
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
        (nanos >> 32) as u32,
        (nanos >> 16) as u16,
        (nanos & 0xfff) as u16,
        (count & 0xfff) as u16,
        u64::from(std::process::id()) << 16 | (count >> 12) & 0xffff,
    )
}
