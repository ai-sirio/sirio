//! Claude Code driven over its own stdio protocol, in place of an ACP
//! wrapper.
//!
//! The shape mirrors the ACP client next door — one worker thread, a
//! command channel in, an unbounded event channel out, four bounded waits —
//! and, crucially, emits the same [`AcpEvent`](crate::AcpEvent) values. The
//! chat surface is written against that vocabulary, so which transport a
//! tab uses is invisible above this line.

mod events;
mod worker;

use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Result, anyhow};

use crate::{
    AcpError, ChildHandle, EventStream, ModeCatalog, ModelCatalog, SHUTDOWN_TIMEOUT,
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
    event_tx: Option<async_channel::Sender<crate::AcpEvent>>,
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
                    return Err(anyhow::Error::new(AcpError::Timeout {
                        operation: TimeoutOperation::Startup,
                        duration: startup_timeout,
                    }));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Some(child) = child.take() {
                        terminate_and_reap_blocking(&child);
                    }
                    crate::detach_worker(worker);
                    return Err(anyhow::Error::new(AcpError::Transport(
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

    /// Sends a plain-text turn. [`Self::prompt_content`] (Task 8) is the
    /// full form; this is the one the integration tests drive.
    pub fn prompt(&self, text: impl Into<String>) -> Result<()> {
        self.command_tx
            .send_blocking(worker::Command::Prompt(text.into()))
            .map_err(|error| anyhow!("the Claude worker is not running: {error}"))
    }

    /// The Claude session id, once a turn has named it. `None` before the
    /// first turn: unlike ACP, this protocol has no session to create, so
    /// the id does not exist until the CLI opens one.
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

    /// The mode selector, re-read live: `set_permission_mode` and the CLI's
    /// own `system/status` both land here.
    #[must_use]
    pub fn mode_catalog(&self) -> Option<ModeCatalog> {
        self.shared.mode_catalog()
    }

    /// MCP servers that failed to connect, as sentences (F-CHAT-33).
    #[must_use]
    pub fn mcp_warnings(&self) -> Vec<String> {
        self.shared.mcp_warnings()
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
                Err(anyhow::Error::new(AcpError::Timeout {
                    operation: TimeoutOperation::Shutdown,
                    duration: SHUTDOWN_TIMEOUT,
                }))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = worker.join();
                Err(anyhow::Error::new(AcpError::Transport(
                    "the Claude worker exited during shutdown".into(),
                )))
            }
        };
        self.event_tx.take();
        result
    }
}

impl Drop for ClaudeClient {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
