//! Small ACP client used by Tiller's chat layer.
//!
//! The wire protocol and subprocess lifecycle are delegated to Zed's
//! `agent-client-protocol` SDK. This crate owns the application-facing event
//! stream and the command boundary around one agent session.

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    CancelNotification, ClientCapabilities, ContentBlock, ContentChunk, InitializeRequest,
    NewSessionRequest, PermissionOption as ProtocolPermissionOption, PromptRequest,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigOptionValue, SessionConfigSelectOptions, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, TextContent, ToolCallStatus,
};
use agent_client_protocol::{AcpAgent, AcpAgentConfig, Agent, Client, ConnectionTo, Lines};
use anyhow::{Result, anyhow};
use async_process::Child;
use futures::executor::block_on;
use futures::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

// `npx -y` may have to download and unpack the ACP adapter before the first
// protocol byte exists. Keep that cold-start budget bounded, but long enough
// that a healthy first launch is not mistaken for a dead agent.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
const PROMPT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const PERMISSION_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const CHILD_REAP_TIMEOUT: Duration = Duration::from_secs(2);

type ChildHandle = Arc<Mutex<Option<Child>>>;

/// The operation that exceeded its bounded wait.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeoutOperation {
    /// ACP initialize plus session creation.
    Startup,
    /// A submitted prompt turn.
    Prompt,
    /// A permission request awaiting the caller's choice.
    Permission,
    /// The client shutdown handshake and worker join.
    Shutdown,
}

/// A typed client failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcpError {
    /// An external-process wait exceeded its configured deadline.
    Timeout {
        /// Which wait exceeded its deadline.
        operation: TimeoutOperation,
        /// The configured deadline.
        duration: Duration,
    },
    /// The ACP transport failed independently of a deadline.
    Transport(String),
    /// The agent rejected a request or reported an agent-level failure.
    Agent(String),
}

impl std::fmt::Display for AcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout {
                operation,
                duration,
            } => write!(f, "ACP {operation:?} timed out after {duration:?}"),
            Self::Transport(message) => write!(f, "ACP transport error: {message}"),
            Self::Agent(message) => write!(f, "ACP agent error: {message}"),
        }
    }
}

impl std::error::Error for AcpError {}

/// The asynchronous event stream returned by [`AcpClient::launch`].
pub type EventStream = async_channel::Receiver<AcpEvent>;

/// A process to launch as an ACP agent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentCommand {
    /// Executable name or absolute path.
    pub program: PathBuf,
    /// Arguments passed without shell parsing.
    pub args: Vec<String>,
}

impl AgentCommand {
    /// Build a command from an executable name.
    #[must_use]
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    /// Add one argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add several arguments.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }
}

/// A permission option exposed to the caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionOption {
    /// The protocol option identifier to send back to the agent.
    pub id: String,
    /// Human-readable label.
    pub name: String,
    /// Protocol-defined permission kind.
    pub kind: String,
}

/// A model choice advertised by the agent for this session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelOption {
    /// Protocol value sent by the agent.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Optional agent-provided description.
    pub description: Option<String>,
}

/// The model selector advertised by an ACP session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelCatalog {
    /// Configuration option identifier used by `session/set_config_option`.
    pub config_id: String,
    /// Available model choices.
    pub options: Vec<ModelOption>,
    /// Current value selected by the agent.
    pub selected_id: String,
}

/// Context usage reported by the agent for the current session.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextUsage {
    /// Tokens currently in the context window.
    pub used: u64,
    /// Total context window size in tokens.
    pub size: u64,
    /// Cumulative cost when the agent reports it.
    pub cost: Option<ContextCost>,
}

/// Cumulative cost attached to an ACP usage update.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextCost {
    /// Amount in the reported currency.
    pub amount: f64,
    /// ISO 4217 currency code.
    pub currency: String,
}

/// Typed events emitted by one ACP session.
#[derive(Clone, Debug, PartialEq)]
pub enum AcpEvent {
    /// A streamed assistant response chunk.
    AgentMessageChunk(String),
    /// A streamed internal reasoning chunk.
    ThoughtChunk(String),
    /// A newly started tool call.
    ToolCallStarted {
        /// Tool call identifier.
        id: String,
        /// Human-readable tool title.
        title: String,
        /// Initial protocol status.
        status: String,
    },
    /// A non-terminal tool call update.
    ToolCallUpdated {
        /// Tool call identifier.
        id: String,
        /// Updated title, when present.
        title: Option<String>,
        /// Updated status, when present.
        status: Option<String>,
    },
    /// A tool call reached a terminal status.
    ToolCallCompleted {
        /// Tool call identifier.
        id: String,
        /// Terminal protocol status.
        status: String,
    },
    /// The model selector changed or became available.
    ModelCatalog(ModelCatalog),
    /// The agent reported current context-window usage.
    ContextUsage(ContextUsage),
    /// A session update outside the chat/tool subset, retained so callers can
    /// observe protocol traffic without treating it as a transport failure.
    OtherSessionUpdate {
        /// The debug name of the protocol update variant.
        kind: String,
    },
    /// The agent is waiting for a caller decision.
    PermissionRequest {
        /// Handle passed to [`AcpClient::respond_permission`].
        request_id: u64,
        /// Session that owns the request.
        session_id: String,
        /// Available choices.
        options: Vec<PermissionOption>,
    },
    /// The current prompt turn finished.
    TurnEnded {
        /// Protocol stop reason, retained as text to remain forward-compatible.
        stop_reason: String,
    },
    /// The agent transport failed or closed before the client shut it down.
    TransportError(String),
    /// A bounded wait on the agent expired; the child was terminated.
    Timeout {
        /// Which wait expired.
        operation: TimeoutOperation,
        /// The configured deadline.
        duration: Duration,
    },
}

#[derive(Debug)]
enum Command {
    Prompt(String),
    SetModel { config_id: String, value: String },
    Cancel,
    Shutdown(mpsc::SyncSender<()>),
}

#[derive(Debug)]
enum PermissionChoice {
    Selected(String),
    Cancelled,
}

type PermissionWaiters = Arc<Mutex<HashMap<u64, async_channel::Sender<PermissionChoice>>>>;

enum WorkerSignal {
    Process(ChildHandle),
    Startup(std::result::Result<Startup, AcpError>),
}

/// Negotiated information from the ACP initialize handshake.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitializeInfo {
    /// Version selected by the agent.
    pub protocol_version: String,
    /// Debug representation of the agent capabilities.
    pub agent_capabilities: String,
}

/// A live client connection to one ACP agent subprocess.
pub struct AcpClient {
    command_tx: async_channel::Sender<Command>,
    pending_permissions: PermissionWaiters,
    child: ChildHandle,
    session_id: String,
    initialize_info: InitializeInfo,
    model_catalog: Option<ModelCatalog>,
    worker: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for AcpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpClient")
            .field("session_id", &self.session_id)
            .field("initialize_info", &self.initialize_info)
            .finish_non_exhaustive()
    }
}

impl AcpClient {
    /// Launch an agent, complete ACP initialization, and create a session.
    ///
    /// The returned receiver is unbounded so an agent can continue receiving
    /// protocol traffic while the caller is rendering or deciding on a
    /// permission request.
    pub fn launch(command: AgentCommand, cwd: impl AsRef<Path>) -> Result<(Self, EventStream)> {
        Self::launch_with_timeout(command, cwd, STARTUP_TIMEOUT)
    }

    fn launch_with_timeout(
        command: AgentCommand,
        cwd: impl AsRef<Path>,
        startup_timeout: Duration,
    ) -> Result<(Self, EventStream)> {
        let cwd = cwd.as_ref().to_path_buf();
        let (command_tx, command_rx) = async_channel::unbounded();
        let (event_tx, event_rx) = async_channel::unbounded();
        let (worker_tx, worker_rx) = mpsc::sync_channel(2);
        let pending_permissions: PermissionWaiters = Arc::new(Mutex::new(HashMap::new()));
        let worker_pending = Arc::clone(&pending_permissions);

        let worker = thread::Builder::new()
            .name("tiller-acp".into())
            .spawn(move || {
                run_connection(
                    command,
                    cwd,
                    command_rx,
                    event_tx,
                    worker_tx,
                    worker_pending,
                );
            })?;

        let deadline = std::time::Instant::now() + startup_timeout;
        let mut child = None;
        let startup = loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let signal = match worker_rx.recv_timeout(remaining) {
                Ok(signal) => signal,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Some(process) = child.take() {
                        terminate_and_reap_blocking(&process);
                    }
                    detach_worker(worker);
                    return Err(anyhow::Error::new(AcpError::Timeout {
                        operation: TimeoutOperation::Startup,
                        duration: startup_timeout,
                    }));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Some(process) = child.take() {
                        terminate_and_reap_blocking(&process);
                    }
                    detach_worker(worker);
                    return Err(anyhow::Error::new(AcpError::Transport(
                        "ACP worker exited before initialization".into(),
                    )));
                }
            };
            match signal {
                WorkerSignal::Process(process) => child = Some(process),
                WorkerSignal::Startup(result) => match result {
                    Ok(startup) => break startup,
                    Err(error) => {
                        if let Some(process) = child.take() {
                            terminate_and_reap_blocking(&process);
                        }
                        detach_worker(worker);
                        return Err(anyhow::Error::new(error));
                    }
                },
            }
        };
        let child = child.ok_or_else(|| anyhow!("ACP worker did not expose its process"))?;

        Ok((
            Self {
                command_tx,
                pending_permissions,
                child,
                session_id: startup.session_id,
                initialize_info: startup.initialize_info,
                model_catalog: startup.model_catalog,
                worker: Some(worker),
            },
            event_rx,
        ))
    }

    /// The ACP session identifier created during launch.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Information selected during initialize negotiation.
    #[must_use]
    pub fn initialize_info(&self) -> &InitializeInfo {
        &self.initialize_info
    }

    /// The model selector advertised during session creation, if any.
    #[must_use]
    pub fn model_catalog(&self) -> Option<&ModelCatalog> {
        self.model_catalog.as_ref()
    }

    /// Send a user turn without blocking on its streamed response.
    pub fn prompt(&self, text: impl Into<String>) -> Result<()> {
        self.command_tx
            .send_blocking(Command::Prompt(text.into()))
            .map_err(|error| anyhow!("ACP worker is not running: {error}"))
    }

    /// Ask the agent to change the current session model.
    pub fn set_model(&self, config_id: impl Into<String>, value: impl Into<String>) -> Result<()> {
        self.command_tx
            .send_blocking(Command::SetModel {
                config_id: config_id.into(),
                value: value.into(),
            })
            .map_err(|error| anyhow!("ACP worker is not running: {error}"))
    }

    /// Cancel the current prompt turn.
    pub fn cancel(&self) -> Result<()> {
        self.command_tx
            .send_blocking(Command::Cancel)
            .map_err(|error| anyhow!("ACP worker is not running: {error}"))
    }

    /// Answer a permission request surfaced in [`AcpEvent::PermissionRequest`].
    pub fn respond_permission(&self, request_id: u64, option_id: impl Into<String>) -> Result<()> {
        let waiter = self
            .pending_permissions
            .lock()
            .map_err(|_| anyhow!("ACP permission state is poisoned"))?
            .remove(&request_id)
            .ok_or_else(|| anyhow!("unknown ACP permission request {request_id}"))?;
        waiter
            .send_blocking(PermissionChoice::Selected(option_id.into()))
            .map_err(|error| anyhow!("ACP permission request is no longer active: {error}"))
    }

    /// Stop the agent and wait for the subprocess worker to finish.
    pub fn shutdown(&mut self) -> Result<()> {
        if let Some(worker) = self.worker.take() {
            let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(0);
            self.command_tx
                .send_blocking(Command::Shutdown(shutdown_tx))
                .map_err(|error| anyhow!("ACP worker is not running: {error}"))?;
            match shutdown_rx.recv_timeout(SHUTDOWN_TIMEOUT) {
                Ok(()) => {
                    let _ = worker.join();
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    terminate_and_reap_blocking(&self.child);
                    let _ = thread::spawn(move || {
                        let _ = worker.join();
                    });
                    return Err(anyhow::Error::new(AcpError::Timeout {
                        operation: TimeoutOperation::Shutdown,
                        duration: SHUTDOWN_TIMEOUT,
                    }));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = worker.join();
                    return Err(anyhow::Error::new(AcpError::Transport(
                        "ACP worker exited during shutdown".into(),
                    )));
                }
            }
        }
        Ok(())
    }
}

impl Drop for AcpClient {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

struct Startup {
    session_id: String,
    initialize_info: InitializeInfo,
    model_catalog: Option<ModelCatalog>,
}

fn run_connection(
    command: AgentCommand,
    cwd: PathBuf,
    command_rx: async_channel::Receiver<Command>,
    event_tx: EventStreamSender,
    worker_tx: mpsc::SyncSender<WorkerSignal>,
    pending_permissions: PermissionWaiters,
) {
    let started = Arc::new(AtomicBool::new(false));
    let clean_shutdown = Arc::new(AtomicBool::new(false));
    let permission_counter = Arc::new(AtomicU64::new(1));
    let prompt_counter = Arc::new(AtomicU64::new(1));
    let active_prompt = Arc::new(AtomicU64::new(0));
    let timeout_reason: Arc<Mutex<Option<AcpError>>> = Arc::new(Mutex::new(None));
    let shutdown_ack: Arc<Mutex<Option<mpsc::SyncSender<()>>>> = Arc::new(Mutex::new(None));
    let agent = AcpAgent::new(
        AcpAgentConfig::new(command.program)
            .args(command.args)
            .env("TERM", "xterm-256color"),
    );

    let (stdin, stdout, stderr, child) = match agent.spawn_process() {
        Ok(process) => process,
        Err(error) => {
            let _ = worker_tx.send(WorkerSignal::Startup(Err(AcpError::Transport(
                error.to_string(),
            ))));
            return;
        }
    };
    let child: ChildHandle = Arc::new(Mutex::new(Some(child)));
    if worker_tx
        .send(WorkerSignal::Process(Arc::clone(&child)))
        .is_err()
    {
        terminate_and_reap_blocking(&child);
        return;
    }
    let _stderr_drain = thread::Builder::new()
        .name("tiller-acp-stderr".into())
        .spawn(move || {
            block_on(drain_stderr(stderr));
        });

    let connection_event_tx = event_tx.clone();
    let started_for_connection = Arc::clone(&started);
    let clean_shutdown_for_connection = Arc::clone(&clean_shutdown);
    let timeout_for_connection = Arc::clone(&timeout_reason);
    let worker_for_connection = worker_tx.clone();
    let shutdown_ack_for_connection = Arc::clone(&shutdown_ack);
    let child_for_connection = Arc::clone(&child);
    let connection_result = block_on(async move {
        let notification_events = connection_event_tx.clone();
        let permission_events = connection_event_tx.clone();
        let connection_events = connection_event_tx.clone();
        let permission_waiters = Arc::clone(&pending_permissions);
        let connection_waiters = Arc::clone(&pending_permissions);
        let permission_counter = Arc::clone(&permission_counter);
        let prompt_counter = Arc::clone(&prompt_counter);
        let active_prompt = Arc::clone(&active_prompt);
        let started = Arc::clone(&started_for_connection);
        let clean_shutdown = Arc::clone(&clean_shutdown_for_connection);
        let startup_sender = worker_for_connection.clone();
        let timeout_reason = Arc::clone(&timeout_for_connection);
        let permission_timeout_reason = Arc::clone(&timeout_reason);
        let prompt_timeout_reason = Arc::clone(&timeout_reason);
        let shutdown_ack = Arc::clone(&shutdown_ack_for_connection);
        let child_for_prompt = Arc::clone(&child_for_connection);
        let outgoing = futures::sink::unfold(stdin, |mut writer, line: String| async move {
            writer.write_all(line.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            Ok::<_, std::io::Error>(writer)
        });
        let incoming = BufReader::new(stdout).lines();
        let transport = Lines::new(outgoing, incoming);
        let protocol = Client
            .builder()
            .name("tiller")
            .on_receive_notification(
                async move |notification: SessionNotification, _connection| {
                    let _ = notification_events
                        .send(notification_to_event(notification))
                        .await;
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_request(
                async move |request: RequestPermissionRequest,
                            responder,
                            _connection: ConnectionTo<Agent>| {
                    let request_id = permission_counter.fetch_add(1, Ordering::Relaxed);
                    let (choice_tx, choice_rx) = async_channel::bounded(1);
                    if let Ok(mut waiters) = permission_waiters.lock() {
                        waiters.insert(request_id, choice_tx);
                    }
                    let options = request.options.iter().map(permission_option).collect();
                    let _ = permission_events
                        .send(AcpEvent::PermissionRequest {
                            request_id,
                            session_id: request.session_id.to_string(),
                            options,
                        })
                        .await;
                    let choice = choice_rx.recv();
                    let timeout = async_io::Timer::after(PERMISSION_TIMEOUT);
                    futures::pin_mut!(choice, timeout);
                    let choice = match futures::future::select(choice, timeout).await {
                        futures::future::Either::Left((choice, _)) => {
                            choice.unwrap_or(PermissionChoice::Cancelled)
                        }
                        futures::future::Either::Right((_, _)) => {
                            if let Ok(mut waiters) = permission_waiters.lock() {
                                waiters.remove(&request_id);
                            }
                            let timeout = AcpError::Timeout {
                                operation: TimeoutOperation::Permission,
                                duration: PERMISSION_TIMEOUT,
                            };
                            record_timeout(&permission_timeout_reason, timeout.clone());
                            return Err(agent_client_protocol::Error::internal_error());
                        }
                    };
                    if let Ok(mut waiters) = permission_waiters.lock() {
                        waiters.remove(&request_id);
                    }
                    let outcome = match choice {
                        PermissionChoice::Selected(option_id) => {
                            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                                option_id,
                            ))
                        }
                        PermissionChoice::Cancelled => RequestPermissionOutcome::Cancelled,
                    };
                    responder.respond(RequestPermissionResponse::new(outcome))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_with(transport, async move |connection| {
                let initialize = connection
                    .send_request(
                        InitializeRequest::new(ProtocolVersion::V1)
                            .client_capabilities(ClientCapabilities::default()),
                    )
                    .block_task()
                    .await?;
                if initialize.protocol_version != ProtocolVersion::V1 {
                    return Err(agent_client_protocol::Error::internal_error());
                }

                let session = connection
                    .send_request(NewSessionRequest::new(cwd))
                    .block_task()
                    .await?;
                let initialize_info = InitializeInfo {
                    protocol_version: format!("{:?}", initialize.protocol_version),
                    agent_capabilities: format!("{:?}", initialize.agent_capabilities),
                };
                let model_catalog = model_catalog_from_options(session.config_options.as_ref());
                let model_event = model_catalog.clone().map(AcpEvent::ModelCatalog);
                startup_sender
                    .send(WorkerSignal::Startup(Ok(Startup {
                        session_id: session.session_id.to_string(),
                        initialize_info,
                        model_catalog,
                    })))
                    .map_err(|_| agent_client_protocol::Error::internal_error())?;
                if let Some(model_event) = model_event {
                    let _ = connection_events.send(model_event).await;
                }
                started.store(true, Ordering::Release);
                while let Ok(command) = command_rx.recv().await {
                    match command {
                        Command::Prompt(text) => {
                            let session_id = session.session_id.clone();
                            let event_tx = connection_events.clone();
                            let prompt_id = prompt_counter.fetch_add(1, Ordering::Relaxed);
                            active_prompt.store(prompt_id, Ordering::Release);
                            let active_prompt_for_result = Arc::clone(&active_prompt);
                            connection
                                .send_request(PromptRequest::new(
                                    session_id,
                                    vec![ContentBlock::Text(TextContent::new(text))],
                                ))
                                .on_receiving_result(move |result| async move {
                                    active_prompt_for_result.store(0, Ordering::Release);
                                    match result {
                                        Ok(response) => {
                                            let _ = event_tx
                                                .send(AcpEvent::TurnEnded {
                                                    stop_reason: format!(
                                                        "{:?}",
                                                        response.stop_reason
                                                    ),
                                                })
                                                .await;
                                        }
                                        Err(error) => {
                                            let _ = event_tx
                                                .send(AcpEvent::TransportError(format!(
                                                    "prompt failed: {error}"
                                                )))
                                                .await;
                                        }
                                    }
                                    Ok(())
                                })?;
                            let active_prompt = Arc::clone(&active_prompt);
                            let timeout_reason = Arc::clone(&prompt_timeout_reason);
                            let child = Arc::clone(&child_for_prompt);
                            let _ = thread::Builder::new()
                                .name("tiller-acp-prompt-timeout".into())
                                .spawn(move || {
                                    thread::sleep(PROMPT_TIMEOUT);
                                    if active_prompt
                                        .compare_exchange(
                                            prompt_id,
                                            0,
                                            Ordering::AcqRel,
                                            Ordering::Acquire,
                                        )
                                        .is_ok()
                                    {
                                        let timeout = AcpError::Timeout {
                                            operation: TimeoutOperation::Prompt,
                                            duration: PROMPT_TIMEOUT,
                                        };
                                        record_timeout(&timeout_reason, timeout);
                                        terminate_and_reap_blocking(&child);
                                    }
                                });
                        }
                        Command::SetModel { config_id, value } => {
                            let event_tx = connection_events.clone();
                            connection
                                .send_request(SetSessionConfigOptionRequest::new(
                                    session.session_id.clone(),
                                    config_id,
                                    SessionConfigOptionValue::value_id(value),
                                ))
                                .on_receiving_result(move |result| async move {
                                    match result {
                                        Ok(response) => {
                                            if let Some(catalog) = model_catalog_from_options(Some(
                                                &response.config_options,
                                            )) {
                                                let _ = event_tx
                                                    .send(AcpEvent::ModelCatalog(catalog))
                                                    .await;
                                            }
                                        }
                                        Err(error) => {
                                            let _ = event_tx
                                                .send(AcpEvent::TransportError(format!(
                                                    "model selection failed: {error}"
                                                )))
                                                .await;
                                        }
                                    }
                                    Ok(())
                                })?;
                        }
                        Command::Cancel => {
                            cancel_permissions(&connection_waiters);
                            connection.send_notification(CancelNotification::new(
                                session.session_id.clone(),
                            ))?;
                        }
                        Command::Shutdown(ack) => {
                            cancel_permissions(&connection_waiters);
                            clean_shutdown.store(true, Ordering::Release);
                            if let Ok(mut shutdown_ack) = shutdown_ack.lock() {
                                *shutdown_ack = Some(ack);
                            }
                            break;
                        }
                    }
                }
                Ok(())
            });
        protocol.await
    });

    terminate_and_reap_blocking(&child);
    if let Ok(mut ack) = shutdown_ack.lock()
        && let Some(ack) = ack.take()
    {
        let _ = ack.send(());
    }
    if !started.load(Ordering::Acquire) {
        let error = timeout_reason
            .lock()
            .ok()
            .and_then(|reason| reason.clone())
            .unwrap_or_else(|| {
                AcpError::Transport(
                    connection_result
                        .as_ref()
                        .err()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "ACP worker exited before initialization".into()),
                )
            });
        let _ = worker_tx.send(WorkerSignal::Startup(Err(error)));
    } else if !clean_shutdown.load(Ordering::Acquire) {
        if let Some(AcpError::Timeout {
            operation,
            duration,
        }) = timeout_reason.lock().ok().and_then(|reason| reason.clone())
        {
            let _ = event_tx.send_blocking(AcpEvent::Timeout {
                operation,
                duration,
            });
        } else if let Err(error) = connection_result {
            let _ = event_tx.send_blocking(AcpEvent::TransportError(format!(
                "ACP transport closed unexpectedly: {error}"
            )));
        } else {
            let _ = event_tx.send_blocking(AcpEvent::TransportError(
                "ACP transport closed unexpectedly".into(),
            ));
        }
    }
}

type EventStreamSender = async_channel::Sender<AcpEvent>;

async fn drain_stderr(mut stderr: impl futures::AsyncRead + Unpin) {
    use futures::AsyncReadExt as _;

    let mut buffer = [0_u8; 8 * 1024];
    while stderr
        .read(&mut buffer)
        .await
        .ok()
        .is_some_and(|size| size > 0)
    {}
}

fn record_timeout(reason: &Arc<Mutex<Option<AcpError>>>, timeout: AcpError) {
    if let Ok(mut reason) = reason.lock() {
        *reason = Some(timeout);
    }
}

async fn terminate_and_reap(child: &ChildHandle) {
    let Some(mut child) = child.lock().ok().and_then(|mut child| child.take()) else {
        return;
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
    let _ = futures::future::select(wait, timeout).await;
}

fn terminate_and_reap_blocking(child: &ChildHandle) {
    block_on(terminate_and_reap(child));
}

fn detach_worker(worker: JoinHandle<()>) {
    let _ = thread::spawn(move || {
        let _ = worker.join();
    });
}

fn cancel_permissions(pending: &PermissionWaiters) {
    if let Ok(mut waiters) = pending.lock() {
        for (_, waiter) in waiters.drain() {
            let _ = waiter.send_blocking(PermissionChoice::Cancelled);
        }
    }
}

fn permission_option(option: &ProtocolPermissionOption) -> PermissionOption {
    PermissionOption {
        id: option.option_id.to_string(),
        name: option.name.clone(),
        kind: format!("{:?}", option.kind),
    }
}

fn content_text(content: ContentChunk) -> Option<String> {
    match content.content {
        ContentBlock::Text(text) => Some(text.text),
        _ => None,
    }
}

fn notification_to_event(notification: SessionNotification) -> AcpEvent {
    match notification.update {
        SessionUpdate::AgentMessageChunk(content) => content_text(content)
            .map(AcpEvent::AgentMessageChunk)
            .unwrap_or_else(|| AcpEvent::OtherSessionUpdate {
                kind: "AgentMessageChunk(non-text)".into(),
            }),
        SessionUpdate::AgentThoughtChunk(content) => content_text(content)
            .map(AcpEvent::ThoughtChunk)
            .unwrap_or_else(|| AcpEvent::OtherSessionUpdate {
                kind: "AgentThoughtChunk(non-text)".into(),
            }),
        SessionUpdate::ToolCall(tool) => AcpEvent::ToolCallStarted {
            id: tool.tool_call_id.to_string(),
            title: tool.title,
            status: format!("{:?}", tool.status),
        },
        SessionUpdate::ToolCallUpdate(update) => {
            let status = update.fields.status.map(|status| format!("{:?}", status));
            if matches!(
                update.fields.status,
                Some(ToolCallStatus::Completed | ToolCallStatus::Failed)
            ) {
                AcpEvent::ToolCallCompleted {
                    id: update.tool_call_id.to_string(),
                    status: status.unwrap_or_else(|| "Unknown".into()),
                }
            } else {
                AcpEvent::ToolCallUpdated {
                    id: update.tool_call_id.to_string(),
                    title: update.fields.title,
                    status,
                }
            }
        }
        SessionUpdate::UsageUpdate(usage) => AcpEvent::ContextUsage(ContextUsage {
            used: usage.used,
            size: usage.size,
            cost: usage.cost.map(|cost| ContextCost {
                amount: cost.amount,
                currency: cost.currency,
            }),
        }),
        SessionUpdate::ConfigOptionUpdate(update) => {
            model_catalog_from_options(Some(&update.config_options))
                .map(AcpEvent::ModelCatalog)
                .unwrap_or_else(|| AcpEvent::OtherSessionUpdate {
                    kind: "ConfigOptionUpdate".into(),
                })
        }
        _ => AcpEvent::OtherSessionUpdate {
            kind: "other".into(),
        },
    }
}

fn model_catalog_from_options(options: Option<&Vec<SessionConfigOption>>) -> Option<ModelCatalog> {
    let option = options?.iter().find(|option| {
        option.id.to_string() == "model"
            || matches!(option.category, Some(SessionConfigOptionCategory::Model))
    })?;
    let SessionConfigKind::Select(select) = &option.kind else {
        return None;
    };
    let values = match &select.options {
        SessionConfigSelectOptions::Ungrouped(values) => values.clone(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter().cloned())
            .collect(),
        _ => Vec::new(),
    };
    Some(ModelCatalog {
        config_id: option.id.to_string(),
        selected_id: select.current_value.to_string(),
        options: values
            .into_iter()
            .map(|option| ModelOption {
                id: option.value.to_string(),
                name: option.name,
                description: option.description,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{ToolCall, ToolCallUpdate, ToolCallUpdateFields};

    #[test]
    fn maps_text_updates_to_typed_events() {
        let message = SessionNotification::new(
            "session",
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new("ciao"),
            ))),
        );
        assert_eq!(
            notification_to_event(message),
            AcpEvent::AgentMessageChunk("ciao".into())
        );
    }

    #[test]
    fn maps_completed_tool_update_to_terminal_event() {
        let update = SessionNotification::new(
            "session",
            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                "tool",
                ToolCallUpdateFields::default().status(ToolCallStatus::Completed),
            )),
        );
        assert_eq!(
            notification_to_event(update),
            AcpEvent::ToolCallCompleted {
                id: "tool".into(),
                status: "Completed".into(),
            }
        );
    }

    #[test]
    fn maps_started_tool_call_to_started_event() {
        let update = SessionNotification::new(
            "session",
            SessionUpdate::ToolCall(ToolCall::new("tool", "Read file")),
        );
        assert!(matches!(
            notification_to_event(update),
            AcpEvent::ToolCallStarted { id, title, .. } if id == "tool" && title == "Read file"
        ));
    }

    #[test]
    fn maps_usage_update_to_typed_context_usage() {
        let update = SessionNotification::new(
            "session",
            SessionUpdate::UsageUpdate(
                agent_client_protocol::schema::v1::UsageUpdate::new(53_000, 200_000)
                    .cost(agent_client_protocol::schema::v1::Cost::new(0.045, "USD")),
            ),
        );
        assert_eq!(
            notification_to_event(update),
            AcpEvent::ContextUsage(ContextUsage {
                used: 53_000,
                size: 200_000,
                cost: Some(ContextCost {
                    amount: 0.045,
                    currency: "USD".into(),
                }),
            })
        );
    }

    #[test]
    fn extracts_model_choices_from_session_configuration() {
        let options = vec![
            SessionConfigOption::select(
                "model",
                "Model",
                "sonnet",
                vec![
                    agent_client_protocol::schema::v1::SessionConfigSelectOption::new(
                        "sonnet", "Sonnet",
                    ),
                    agent_client_protocol::schema::v1::SessionConfigSelectOption::new(
                        "haiku", "Haiku",
                    ),
                ],
            )
            .category(SessionConfigOptionCategory::Model),
        ];
        let catalog = model_catalog_from_options(Some(&options)).expect("model selector");
        assert_eq!(catalog.config_id, "model");
        assert_eq!(catalog.selected_id, "sonnet");
        assert_eq!(
            catalog.options,
            vec![
                ModelOption {
                    id: "sonnet".into(),
                    name: "Sonnet".into(),
                    description: None,
                },
                ModelOption {
                    id: "haiku".into(),
                    name: "Haiku".into(),
                    description: None,
                },
            ]
        );
    }

    #[test]
    fn maps_model_configuration_updates_to_catalog_events() {
        let update = SessionNotification::new(
            "session",
            SessionUpdate::ConfigOptionUpdate(
                agent_client_protocol::schema::v1::ConfigOptionUpdate::new(vec![
                    SessionConfigOption::select(
                        "model",
                        "Model",
                        "haiku",
                        vec![
                            agent_client_protocol::schema::v1::SessionConfigSelectOption::new(
                                "haiku", "Haiku",
                            ),
                        ],
                    )
                    .category(SessionConfigOptionCategory::Model),
                ]),
            ),
        );
        assert!(matches!(
            notification_to_event(update),
            AcpEvent::ModelCatalog(ModelCatalog { selected_id, .. }) if selected_id == "haiku"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn silent_agent_hits_startup_timeout_and_is_terminated() {
        let started = std::time::Instant::now();
        let timeout = Duration::from_secs(1);
        let result = AcpClient::launch_with_timeout(
            AgentCommand::new("/bin/sh").args(["-c", "sleep 60"]),
            ".",
            timeout,
        );
        let error = match result {
            Ok((mut client, _events)) => {
                let _ = client.shutdown();
                panic!("silent agent unexpectedly initialized")
            }
            Err(error) => error,
        };

        assert!(
            matches!(
                error.downcast_ref::<AcpError>(),
                Some(AcpError::Timeout {
                    operation: TimeoutOperation::Startup,
                    ..
                })
            ),
            "expected typed startup timeout, got {error:#}"
        );
        assert!(
            started.elapsed() < timeout + Duration::from_secs(2),
            "startup timeout exceeded its bound: {:?}",
            started.elapsed()
        );
    }

    /// Launches a real agent over `npx`, so it depends on the network, on npm
    /// resolving a package, and on the machine not being busy. Kept because
    /// it is the only end-to-end check of startup, but run it deliberately:
    ///
    ///     cargo test -p tiller_acp -- --ignored
    #[test]
    #[ignore = "needs a real agent over the network; flakes the gate under load"]
    fn real_agent_completes_startup_before_deadline() {
        let started = std::time::Instant::now();
        let result = AcpClient::launch(
            AgentCommand::new("npx").args(["-y", "@agentclientprotocol/claude-agent-acp@latest"]),
            std::env::temp_dir(),
        );
        let elapsed = started.elapsed();
        let (mut client, _events) = result.expect("real ACP agent should initialize");
        assert!(
            elapsed < STARTUP_TIMEOUT / 3,
            "real ACP startup took {:?}, expected well inside {:?}",
            elapsed,
            STARTUP_TIMEOUT
        );
        client.shutdown().expect("real ACP agent should shut down");
    }
}
