//! Small ACP client used by Sirio's chat layer.
//!
//! The wire protocol and subprocess lifecycle are delegated to Zed's
//! `agent-client-protocol` SDK. This crate owns the application-facing event
//! stream and the command boundary around one agent session.

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    AuthMethod, CancelNotification, ClientCapabilities, ContentBlock, ContentChunk, ImageContent,
    InitializeRequest, NewSessionRequest, PermissionOption as ProtocolPermissionOption,
    PlanEntry as ProtocolPlanEntry, PlanEntryStatus as ProtocolPlanEntryStatus, PromptRequest,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse, ResourceLink,
    SelectedPermissionOutcome, SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigOptionValue, SessionConfigSelectOptions, SessionModeState, SessionNotification,
    SessionUpdate, SetSessionConfigOptionRequest, SetSessionModeRequest, TextContent,
    ToolCallContent, ToolCallLocation, ToolCallStatus,
};
use agent_client_protocol::{
    AcpAgent, AcpAgentConfig, Agent, Client, ConnectionTo, ErrorCode, Lines,
};
use anyhow::{Result, anyhow};
use async_process::Child;
use futures::executor::block_on;
use futures::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

mod chat;
mod mcp_config;

pub use chat::{ChatSession, ChatSessionConfig, ChatSnapshot, ChatStatus};
pub use mcp_config::discover_mcp_servers;

// `npx -y` may have to download and unpack the ACP adapter before the first
// protocol byte exists. Keep that cold-start budget bounded, but long enough
// that a healthy first launch is not mistaken for a dead agent.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
/// How long the agent may stay **silent** during a turn before it counts as
/// hung. This is deliberately an idle window, not a cap on the turn: an
/// agentic turn that runs for an hour is normal as long as it keeps
/// reporting, and killing a working agent loses the whole session, not just
/// the turn. The clock restarts on every notification the agent sends.
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
    /// The agent rejected `initialize` or `session/new` with ACP's
    /// `auth_required` error (wire code -32000). The agent has already
    /// exited by the time this reaches the caller — there is no live
    /// connection left to authenticate on — so recovery is: run the
    /// agent's own CLI login out-of-band, then relaunch (F-CHAT-02).
    AuthRequired {
        /// Authentication methods the agent advertised during `initialize`,
        /// when it reported any before the request that failed.
        methods: Vec<AuthMethodInfo>,
    },
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
            Self::AuthRequired { methods } if methods.is_empty() => {
                write!(f, "ACP agent requires authentication before it can be used")
            }
            Self::AuthRequired { methods } => {
                let names = methods
                    .iter()
                    .map(|method| method.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "ACP agent requires authentication ({names})")
            }
        }
    }
}

impl std::error::Error for AcpError {}

/// One authentication method the agent advertised in its `initialize`
/// response (ACP's `AuthMethod`, flattened to what a caller needs to render
/// CLI login guidance — the wire `id` doubles as the `authenticate` RPC's
/// `method_id` for a future live-authenticate flow).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthMethodInfo {
    /// Protocol identifier for this method.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Optional agent-provided description.
    pub description: Option<String>,
}

fn auth_method_info(method: &AuthMethod) -> AuthMethodInfo {
    AuthMethodInfo {
        id: method.id().to_string(),
        name: method.name().to_string(),
        description: method.description().map(str::to_string),
    }
}

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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContextUsage {
    /// Tokens currently in the context window.
    pub used: u64,
    /// Total context window size in tokens.
    pub size: u64,
    /// Cumulative cost when the agent reports it.
    pub cost: Option<ContextCost>,
    /// Cumulative input tokens, when the agent reports end-of-turn usage
    /// (F-CHAT-18; requires `unstable_end_turn_token_usage`, so it stays
    /// `None` against agents that never send a `PromptResponse.usage`).
    pub input_tokens: Option<u64>,
    /// Cumulative output tokens, same source and caveat as `input_tokens`.
    pub output_tokens: Option<u64>,
    /// Cumulative cache-read tokens, same source and caveat as `input_tokens`.
    pub cached_read_tokens: Option<u64>,
}

/// Cumulative cost attached to an ACP usage update.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextCost {
    /// Amount in the reported currency.
    pub amount: f64,
    /// ISO 4217 currency code.
    pub currency: String,
}

/// An image the user attached to the composer, ready for an ACP image block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageAttachment {
    /// Media type, e.g. `image/png`.
    pub mime_type: String,
    /// Base64-encoded media payload.
    pub base64_data: String,
}

/// One slash command advertised by the agent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailableCommandInfo {
    /// Command name (e.g. `create_plan`).
    pub name: String,
    /// Human-readable description of what the command does.
    pub description: String,
}

/// One choice in an effort-level selector.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffortChoice {
    /// Protocol value sent to the agent.
    pub value: String,
    /// Human-readable label.
    pub name: String,
}

/// The effort-level selector advertised by the session, when the agent
/// offers one (OpenCode reports it as a `configOptions` select with id
/// `effort`; standard ACP agents may tag it with another category).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffortOption {
    /// Configuration option identifier used by `session/set_config_option`.
    pub option_id: String,
    /// Human-readable label for the option, when the agent provides one.
    pub name: Option<String>,
    /// Currently selected value, when the agent reports one.
    pub current_value: Option<String>,
    /// The offered levels.
    pub choices: Vec<EffortChoice>,
}

/// A mode the agent can operate in for this session (ACP's `SessionMode`,
/// e.g. "ask"/"plan"/"auto" — the offered granularity is entirely
/// agent-defined; see the ACP session-modes spec) (F-CHAT-15).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentMode {
    /// Protocol identifier sent back via `session/set_mode`.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Optional agent-provided description.
    pub description: Option<String>,
}

/// The session-mode selector advertised by an ACP session (ACP's
/// `SessionModeState`) — a distinct wire concept from [`ModelCatalog`] and
/// [`EffortOption`], which ride the separate `SessionConfigOption`
/// mechanism (F-CHAT-15).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModeCatalog {
    /// The mode currently active.
    pub current_id: String,
    /// Every mode the agent offers.
    pub options: Vec<AgentMode>,
    /// When `Some`, this mode is applied with `session/set_config_option`
    /// against this option id — OpenCode reports its mode as a `configOptions`
    /// select tagged `category: "mode"`, not through ACP's session-modes API.
    /// When `None`, it is applied with `session/set_mode` as before.
    pub config_option_id: Option<String>,
}

/// A structured question folded out of an `AskUserQuestion`-shaped tool
/// input attached to a permission request. The permission's own options
/// remain the wire of record; this only adds the free-text prompt and text
/// input affordances the raw options cannot carry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionQuestion {
    /// The question's header, falling back to the prompt text.
    pub header: String,
    /// The question body; empty when it duplicates the header.
    pub prompt: String,
    /// A free-text input offered alongside the options, when declared.
    pub text_input: Option<PermissionTextInput>,
}

/// Free-text input configuration for a structured question.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionTextInput {
    /// Placeholder shown in the empty field.
    pub placeholder: Option<String>,
    /// Text prefilled into the field.
    pub prefill: Option<String>,
}

/// One row of the agent's execution plan (F-CHAT-24).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanEntryInfo {
    /// Human-readable description of what this task aims to accomplish.
    pub content: String,
    /// Wire status: `pending`, `in_progress`, or `completed`.
    pub status: String,
}

/// A file diff a tool call produced (F-CHAT-31/32).
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCallDiff {
    /// The file path being modified.
    pub path: PathBuf,
    /// The original content; `None` for a newly created file.
    pub old_text: Option<String>,
    /// The content after modification.
    pub new_text: String,
}

/// One piece of content a tool call produced (F-CHAT-23).
#[derive(Clone, Debug, PartialEq)]
pub enum ToolCallContentInfo {
    /// Plain text content — the common case for command output, search
    /// results, or file contents.
    Text(String),
    /// A file modification, shown as a diff.
    Diff(ToolCallDiff),
    /// Content the protocol carries in a shape this client does not render
    /// (an embedded terminal, image, or audio block) — named so a caller
    /// can show that *something* arrived without inventing detail the
    /// protocol did not provide as text or diff.
    Other,
}

/// A file location a tool call read or touched (F-CHAT-32), enabling
/// "follow-along" treatment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCallLocationInfo {
    /// The absolute file path being accessed or modified.
    pub path: PathBuf,
    /// Optional line number within the file.
    pub line: Option<u32>,
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
        /// Tool category (`Read`, `Edit`, `Execute`, ...), for icon and
        /// detail treatment (F-CHAT-23). Formatted from the protocol's
        /// `ToolKind` — `"Other"` when the agent did not report one.
        kind: String,
        /// Content produced by the call so far — text output or diffs.
        content: Vec<ToolCallContentInfo>,
        /// File locations this call touches (F-CHAT-32).
        locations: Vec<ToolCallLocationInfo>,
        /// Raw input sent to the tool, pretty-printed JSON, when present.
        raw_input: Option<String>,
        /// Raw output the tool returned, pretty-printed JSON, when present.
        raw_output: Option<String>,
    },
    /// A non-terminal tool call update.
    ToolCallUpdated {
        /// Tool call identifier.
        id: String,
        /// Updated title, when present.
        title: Option<String>,
        /// Updated status, when present.
        status: Option<String>,
        /// Updated tool category, when present.
        kind: Option<String>,
        /// Replacement content collection, when the update carries one —
        /// the protocol overwrites rather than extends (F-CHAT-23/31).
        content: Option<Vec<ToolCallContentInfo>>,
        /// Replacement locations collection, when the update carries one.
        locations: Option<Vec<ToolCallLocationInfo>>,
        /// Updated raw input, pretty-printed JSON, when present.
        raw_input: Option<String>,
        /// Updated raw output, pretty-printed JSON, when present.
        raw_output: Option<String>,
    },
    /// A tool call reached a terminal status.
    ToolCallCompleted {
        /// Tool call identifier.
        id: String,
        /// Terminal protocol status.
        status: String,
        /// Updated tool category, when the completing update carries one.
        kind: Option<String>,
        /// Replacement content collection, when the completing update
        /// carries one — the final diff/output commonly arrives here.
        content: Option<Vec<ToolCallContentInfo>>,
        /// Replacement locations collection, when the completing update
        /// carries one.
        locations: Option<Vec<ToolCallLocationInfo>>,
        /// Updated raw input, pretty-printed JSON, when present.
        raw_input: Option<String>,
        /// Updated raw output, pretty-printed JSON, when present.
        raw_output: Option<String>,
    },
    /// The model selector changed or became available.
    ModelCatalog(ModelCatalog),
    /// The agent advertised its slash-command list.
    AvailableCommands(Vec<AvailableCommandInfo>),
    /// The agent advertised an effort-level selector.
    Effort(EffortOption),
    /// The agent reported current context-window usage.
    ContextUsage(ContextUsage),
    /// The agent reported an input/output/cache token breakdown for the
    /// turn that just ended (F-CHAT-18; requires the ACP-agent-side
    /// `unstable_end_turn_token_usage` extension, so this fires only for
    /// agents that opt in).
    TokenUsageBreakdown {
        /// Cumulative input tokens across the session.
        input_tokens: u64,
        /// Cumulative output tokens across the session.
        output_tokens: u64,
        /// Cumulative cache-read tokens across the session, when reported.
        cached_read_tokens: Option<u64>,
    },
    /// The agent is waiting for a caller decision.
    PermissionRequest {
        /// Handle passed to [`AcpClient::respond_permission`].
        request_id: u64,
        /// Session that owns the request.
        session_id: String,
        /// Title of the tool call asking for the decision.
        title: String,
        /// Available choices.
        options: Vec<PermissionOption>,
        /// A structured question carried by the tool input, when present.
        question: Option<PermissionQuestion>,
    },
    /// The agent published or advanced its execution plan.
    PlanUpdate {
        /// Current plan rows; the client replaces any earlier plan.
        entries: Vec<PlanEntryInfo>,
    },
    /// A session update outside the chat/tool subset, retained so callers can
    /// observe protocol traffic without treating it as a transport failure.
    OtherSessionUpdate {
        /// The debug name of the protocol update variant.
        kind: String,
    },
    /// The current prompt turn finished.
    TurnEnded {
        /// Protocol stop reason, retained as text to remain forward-compatible.
        stop_reason: String,
    },
    /// The agent transport failed or closed before the client shut it down.
    /// A mid-session request rejected with ACP's `auth_required` error
    /// (F-CHAT-02) also arrives here rather than as its own variant — this
    /// enum is matched exhaustively by every ACP event consumer in the
    /// workspace, so widening it is a breaking change across crates this
    /// slice does not own; [`AcpError::AuthRequired`]'s `Display` renders
    /// the same auth-guidance text `AcpClient::launch` uses for a startup
    /// failure, so the message string alone still carries it.
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
    PromptContent(Vec<ContentBlock>),
    SetModel { config_id: String, value: String },
    SetConfigOption { option_id: String, value: String },
    SetMode(String),
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
    /// Authentication methods the agent advertised, when it supports any.
    /// Populated even when the session it precedes was created without
    /// needing one of them (F-CHAT-02).
    pub auth_methods: Vec<AuthMethodInfo>,
}

/// A live client connection to one ACP agent subprocess.
pub struct AcpClient {
    command_tx: async_channel::Sender<Command>,
    event_tx: Option<EventStreamSender>,
    pending_permissions: PermissionWaiters,
    child: ChildHandle,
    session_id: String,
    initialize_info: InitializeInfo,
    model_catalog: Option<ModelCatalog>,
    // Live, not a fixed snapshot like `model_catalog`: the worker updates it
    // in place as `session/set_mode` is confirmed or the agent pushes a
    // `CurrentModeUpdate`, so a caller can re-read it after any signal that
    // something changed rather than needing a dedicated typed event
    // (F-CHAT-15; see `AcpEvent::TransportError`'s doc for why this crate's
    // event enum cannot grow a new variant this wave).
    mode_catalog: Arc<Mutex<Option<ModeCatalog>>>,
    // MCP-configuration-flavored lines observed on the agent's stderr
    // (F-CHAT-33). See `looks_like_mcp_warning`'s doc for why stderr is the
    // channel: ACP's wire protocol has no dedicated concept for this.
    mcp_warnings: Arc<Mutex<Vec<String>>>,
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
        Self::launch_with_timeouts(command, cwd, startup_timeout, PROMPT_TIMEOUT)
    }

    fn launch_with_timeouts(
        command: AgentCommand,
        cwd: impl AsRef<Path>,
        startup_timeout: Duration,
        prompt_timeout: Duration,
    ) -> Result<(Self, EventStream)> {
        let cwd = cwd.as_ref().to_path_buf();
        let (command_tx, command_rx) = async_channel::unbounded();
        let (event_tx, event_rx) = async_channel::unbounded();
        let worker_event_tx = event_tx.clone();
        let (worker_tx, worker_rx) = mpsc::sync_channel(2);
        let pending_permissions: PermissionWaiters = Arc::new(Mutex::new(HashMap::new()));
        let worker_pending = Arc::clone(&pending_permissions);
        let mode_catalog: Arc<Mutex<Option<ModeCatalog>>> = Arc::new(Mutex::new(None));
        let worker_mode_catalog = Arc::clone(&mode_catalog);
        let mcp_warnings: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let worker_mcp_warnings = Arc::clone(&mcp_warnings);

        let worker = thread::Builder::new()
            .name("sirio-acp".into())
            .spawn(move || {
                run_connection(
                    command,
                    cwd,
                    command_rx,
                    worker_event_tx,
                    worker_tx,
                    worker_pending,
                    worker_mode_catalog,
                    worker_mcp_warnings,
                    prompt_timeout,
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
                event_tx: Some(event_tx),
                pending_permissions,
                child,
                session_id: startup.session_id,
                initialize_info: startup.initialize_info,
                model_catalog: startup.model_catalog,
                mode_catalog,
                mcp_warnings,
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

    /// The session-mode selector advertised by the agent, if any
    /// (F-CHAT-15). Unlike [`Self::model_catalog`], this reflects the
    /// current state live: re-reading it after [`Self::set_mode`] resolves,
    /// or after any event, picks up a `session/set_mode` confirmation or an
    /// agent-pushed `CurrentModeUpdate`. `None` means the agent did not
    /// advertise `modes` in its `session/new` response — most ACP agents
    /// today don't.
    #[must_use]
    pub fn mode_catalog(&self) -> Option<ModeCatalog> {
        self.mode_catalog
            .lock()
            .ok()
            .and_then(|catalog| catalog.clone())
    }

    /// MCP-configuration-flavored lines observed on the agent's stderr so
    /// far, oldest first and capped at a small bound (F-CHAT-33). Empty
    /// when nothing matched — most sessions, most of the time.
    #[must_use]
    pub fn mcp_warnings(&self) -> Vec<String> {
        self.mcp_warnings
            .lock()
            .map(|warnings| warnings.clone())
            .unwrap_or_default()
    }

    /// Ask the agent to switch to a different session mode (F-CHAT-15),
    /// e.g. an "ask"/"plan"/"auto" selector some ACP agents expose. Valid
    /// `mode_id`s come from [`Self::mode_catalog`]'s advertised options.
    pub fn set_mode(&self, mode_id: impl Into<String>) -> Result<()> {
        self.command_tx
            .send_blocking(Command::SetMode(mode_id.into()))
            .map_err(|error| anyhow!("ACP worker is not running: {error}"))
    }

    /// Send a user turn without blocking on its streamed response.
    pub fn prompt(&self, text: impl Into<String>) -> Result<()> {
        self.command_tx
            .send_blocking(Command::Prompt(text.into()))
            .map_err(|error| anyhow!("ACP worker is not running: {error}"))
    }

    /// Send a user turn composed of text, file mentions, and images, as
    /// separate ACP content blocks (see [`prompt_blocks`] for the mapping).
    pub fn prompt_content(
        &self,
        text: impl Into<String>,
        mention_paths: Vec<String>,
        images: Vec<ImageAttachment>,
        cwd: impl AsRef<Path>,
    ) -> Result<()> {
        let blocks = prompt_blocks(&text.into(), &mention_paths, &images, cwd.as_ref());
        if blocks.is_empty() {
            return Ok(());
        }
        self.command_tx
            .send_blocking(Command::PromptContent(blocks))
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

    /// Ask the agent to change a non-model configuration option (effort).
    pub fn set_config_option(
        &self,
        option_id: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<()> {
        self.command_tx
            .send_blocking(Command::SetConfigOption {
                option_id: option_id.into(),
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

    /// Withdraw a permission request without selecting any option (F-CHAT-25).
    pub fn cancel_permission(&self, request_id: u64) -> Result<()> {
        let waiter = self
            .pending_permissions
            .lock()
            .map_err(|_| anyhow!("ACP permission state is poisoned"))?
            .remove(&request_id)
            .ok_or_else(|| anyhow!("unknown ACP permission request {request_id}"))?;
        waiter
            .send_blocking(PermissionChoice::Cancelled)
            .map_err(|error| anyhow!("ACP permission request is no longer active: {error}"))
    }

    /// Stop the agent and wait for the subprocess worker to finish.
    pub fn shutdown(&mut self) -> Result<()> {
        let Some(worker) = self.worker.take() else {
            self.event_tx.take();
            return Ok(());
        };

        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(0);
        if let Err(error) = self
            .command_tx
            .send_blocking(Command::Shutdown(shutdown_tx))
        {
            terminate_and_reap_blocking(&self.child);
            let _ = worker.join();
            self.event_tx.take();
            return Err(anyhow!("ACP worker is not running: {error}"));
        }

        let result = match shutdown_rx.recv_timeout(SHUTDOWN_TIMEOUT) {
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
                    "ACP worker exited during shutdown".into(),
                )))
            }
        };
        self.event_tx.take();
        result
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

/// Make a resolved command launchable by `CreateProcess` on every platform.
/// Windows does not execute `.cmd` or `.bat` files directly, while npm puts
/// those shims in the registry as the executable path.
fn command_for_process(command: AgentCommand) -> AgentCommand {
    #[cfg(windows)]
    {
        let is_batch = command.program.extension().is_some_and(|extension| {
            let extension = extension.to_string_lossy();
            extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
        });
        if is_batch {
            let shell = std::env::var_os("COMSPEC").unwrap_or_else(|| "cmd.exe".into());
            let mut args = vec![
                "/d".to_string(),
                "/c".to_string(),
                command.program.to_string_lossy().into_owned(),
            ];
            args.extend(command.args);
            return AgentCommand {
                program: PathBuf::from(shell),
                args,
            };
        }
    }
    command
}

// Pre-existing wiring, not a J4-ci change: bundling these into a params struct is a real
// refactor of the connection's call sites and out of scope for the CI-gate slice that found
// this lint newly enforced. Silencing it here is the minimal, behavior-preserving fix.
#[allow(clippy::too_many_arguments)]
fn run_connection(
    command: AgentCommand,
    cwd: PathBuf,
    command_rx: async_channel::Receiver<Command>,
    event_tx: EventStreamSender,
    worker_tx: mpsc::SyncSender<WorkerSignal>,
    pending_permissions: PermissionWaiters,
    mode_catalog: Arc<Mutex<Option<ModeCatalog>>>,
    mcp_warnings: Arc<Mutex<Vec<String>>>,
    prompt_timeout: Duration,
) {
    let started = Arc::new(AtomicBool::new(false));
    let clean_shutdown = Arc::new(AtomicBool::new(false));
    let permission_counter = Arc::new(AtomicU64::new(1));
    let prompt_counter = Arc::new(AtomicU64::new(1));
    let active_prompt = Arc::new(AtomicU64::new(0));
    let timeout_reason: Arc<Mutex<Option<AcpError>>> = Arc::new(Mutex::new(None));
    // Captured right after `initialize` succeeds so it survives a later
    // `session/new` failure — that failure aborts the connection closure via
    // `?` before a `Startup` carrying this can ever be built, and the AUTH
    // REQUIRED error handled below needs the advertised methods on hand.
    let auth_methods: Arc<Mutex<Vec<AuthMethodInfo>>> = Arc::new(Mutex::new(Vec::new()));
    let shutdown_ack: Arc<Mutex<Option<mpsc::SyncSender<()>>>> = Arc::new(Mutex::new(None));
    // Every stderr line, not just the MCP-shaped ones: when the agent dies the
    // protocol only reports "incoming transport closed", which names no cause.
    // Its last words are the only evidence of why it went away.
    let stderr_tail: StderrTail = Arc::new(Mutex::new(VecDeque::new()));
    let drain_tail = Arc::clone(&stderr_tail);
    let activity: ActivityClock = Arc::new(Mutex::new(std::time::Instant::now()));
    let attempted_program = command.program.clone();
    let command = command_for_process(command);
    let agent = AcpAgent::new(
        AcpAgentConfig::new(command.program)
            .args(command.args)
            .env("TERM", "xterm-256color"),
    );

    let (stdin, stdout, stderr, child) = match agent.spawn_process() {
        Ok(process) => process,
        Err(error) => {
            let _ = worker_tx.send(WorkerSignal::Startup(Err(AcpError::Transport(
                format!("could not launch `{}`: {error}", attempted_program.display()),
            ))));
            return;
        }
    };
    let child: ChildHandle = Arc::new(Mutex::new(Some(child)));
    let connection_finished = Arc::new(AtomicBool::new(false));
    let watchdog_child = Arc::clone(&child);
    let watchdog_waiters = Arc::clone(&pending_permissions);
    let watchdog_finished = Arc::clone(&connection_finished);
    let _watchdog = thread::Builder::new()
        .name("sirio-acp-permission-watchdog".into())
        .spawn(move || {
            // The permission handler blocks the connection's single dispatch
            // task while it waits for a choice, so a transport that dies with
            // a permission pending would otherwise hang until PERMISSION_TIMEOUT
            // — the read loop can never notice the EOF. Watch the child
            // process instead: on exit, withdraw every pending permission so
            // the handler unblocks and the connection observes the death.
            //
            // Withdrawing *once* is not enough, and that was a real bug. A
            // request the agent wrote just before dying is still sitting in the
            // pipe when the child is reaped: the kernel keeps buffered bytes
            // readable after the writer is gone, so the handler can register
            // its waiter well after the exit has been observed. A watchdog that
            // cancelled and broke drained an empty map and left that waiter
            // stranded for the full five-minute PERMISSION_TIMEOUT, with the
            // card still answerable and the transport error never surfaced.
            // It reproduced about one run in six, and — being a race — hid
            // whenever the window was instrumented. So once the child is gone,
            // keep sweeping until the connection itself is finished.
            let mut child_gone = false;
            loop {
                if watchdog_finished.load(Ordering::Acquire) {
                    break;
                }
                if !child_gone {
                    child_gone =
                        watchdog_child
                            .lock()
                            .ok()
                            .is_none_or(|mut child| match child.as_mut() {
                                Some(child) => {
                                    let wait = child.status();
                                    let probe = async_io::Timer::after(Duration::from_millis(50));
                                    futures::pin_mut!(wait, probe);
                                    matches!(
                                        block_on(futures::future::select(wait, probe)),
                                        futures::future::Either::Left(_)
                                    )
                                }
                                None => true,
                            });
                }
                if child_gone {
                    // Cheap: `cancel_permissions` drains, so every sweep after
                    // the first is a lock and an empty iteration until a late
                    // waiter shows up. Sweep faster than the pre-exit poll —
                    // the connection is now blocked on exactly this.
                    cancel_permissions(&watchdog_waiters);
                    thread::sleep(Duration::from_millis(10));
                } else {
                    thread::sleep(Duration::from_millis(100));
                }
            }
        });
    if worker_tx
        .send(WorkerSignal::Process(Arc::clone(&child)))
        .is_err()
    {
        terminate_and_reap_blocking(&child);
        return;
    }
    let _stderr_drain = thread::Builder::new()
        .name("sirio-acp-stderr".into())
        .spawn(move || {
            block_on(drain_stderr(stderr, mcp_warnings, drain_tail));
        });

    let connection_event_tx = event_tx.clone();
    let started_for_connection = Arc::clone(&started);
    let clean_shutdown_for_connection = Arc::clone(&clean_shutdown);
    let timeout_for_connection = Arc::clone(&timeout_reason);
    let auth_methods_for_connection = Arc::clone(&auth_methods);
    let worker_for_connection = worker_tx.clone();
    let shutdown_ack_for_connection = Arc::clone(&shutdown_ack);
    let child_for_connection = Arc::clone(&child);
    let stderr_tail_for_connection = Arc::clone(&stderr_tail);
    let activity_for_connection = Arc::clone(&activity);
    let connection_result = block_on(async move {
        let notification_activity = Arc::clone(&activity_for_connection);
        let permission_activity = Arc::clone(&activity_for_connection);
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
        let auth_methods_seen = Arc::clone(&auth_methods_for_connection);
        let mode_catalog_for_notifications = Arc::clone(&mode_catalog);
        let mode_catalog_for_session = Arc::clone(&mode_catalog);
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
            .name("sirio")
            .on_receive_notification(
                async move |notification: SessionNotification, _connection| {
                    // Proof the agent is working, whatever the update says:
                    // the prompt watchdog measures silence, not turn length.
                    touch_activity(&notification_activity);
                    // Applied before `notification_to_events` folds the
                    // update into an `AcpEvent`, so `AcpClient::mode_catalog`
                    // is already current by the time a caller reacts to that
                    // event (F-CHAT-15).
                    if let SessionUpdate::ConfigOptionUpdate(ref update) = notification.update
                        && let Some(catalog) =
                            mode_catalog_from_options(Some(&update.config_options))
                        && let Ok(mut guard) = mode_catalog_for_notifications.lock()
                    {
                        *guard = Some(catalog);
                    }
                    if let SessionUpdate::CurrentModeUpdate(ref update) = notification.update {
                        apply_current_mode(
                            &mode_catalog_for_notifications,
                            update.current_mode_id.to_string(),
                        );
                    }
                    for event in notification_to_events(notification) {
                        let _ = notification_events.send(event).await;
                    }
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_request(
                async move |request: RequestPermissionRequest,
                            responder,
                            _connection: ConnectionTo<Agent>| {
                    touch_activity(&permission_activity);
                    let request_id = permission_counter.fetch_add(1, Ordering::Relaxed);
                    let (choice_tx, choice_rx) = async_channel::bounded(1);
                    if let Ok(mut waiters) = permission_waiters.lock() {
                        waiters.insert(request_id, choice_tx);
                    }
                    let options = request.options.iter().map(permission_option).collect();
                    let title = request
                        .tool_call
                        .fields
                        .title
                        .clone()
                        .unwrap_or_else(|| "Permission requested".into());
                    let question = request
                        .tool_call
                        .fields
                        .raw_input
                        .as_ref()
                        .and_then(parse_permission_question);
                    let _ = permission_events
                        .send(AcpEvent::PermissionRequest {
                            request_id,
                            session_id: request.session_id.to_string(),
                            title,
                            options,
                            question,
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
                let session_auth_methods: Vec<AuthMethodInfo> = initialize
                    .auth_methods
                    .iter()
                    .map(auth_method_info)
                    .collect();
                if let Ok(mut seen) = auth_methods_seen.lock() {
                    *seen = session_auth_methods.clone();
                }

                // F-CHAT-33: `.mcp.json` in the project root, if present, is
                // this crate's only source of MCP server config — ACP's
                // `session/new` requires the client to name every server the
                // agent should attempt to connect to; the agent never
                // discovers the file on its own. See `mcp_config`'s doc
                // comment for why an empty list here makes any MCP
                // connection-failure banner permanently unreachable.
                let mcp_servers = crate::mcp_config::discover_mcp_servers(&cwd);
                let session = connection
                    .send_request(NewSessionRequest::new(cwd).mcp_servers(mcp_servers))
                    .block_task()
                    .await?;
                let initialize_info = InitializeInfo {
                    protocol_version: format!("{:?}", initialize.protocol_version),
                    agent_capabilities: format!("{:?}", initialize.agent_capabilities),
                    auth_methods: session_auth_methods.clone(),
                };
                let model_catalog = model_catalog_from_options(session.config_options.as_ref());
                let effort = effort_from_options(session.config_options.as_ref());
                if let Ok(mut catalog) = mode_catalog_for_session.lock() {
                    if let Some(state) = &session.modes {
                        *catalog = Some(mode_catalog_from_state(state));
                    } else if let Some(fallback) =
                        mode_catalog_from_options(session.config_options.as_ref())
                    {
                        *catalog = Some(fallback);
                    }
                }
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
                if let Some(effort) = effort {
                    let _ = connection_events.send(AcpEvent::Effort(effort)).await;
                }
                started.store(true, Ordering::Release);
                while let Ok(command) = command_rx.recv().await {
                    match command {
                        Command::Prompt(_) | Command::PromptContent(_) => {
                            let prompt_blocks = match command {
                                Command::Prompt(text) => {
                                    vec![ContentBlock::Text(TextContent::new(text))]
                                }
                                Command::PromptContent(blocks) => blocks,
                                _ => unreachable!(),
                            };
                            let session_id = session.session_id.clone();
                            let event_tx = connection_events.clone();
                            let prompt_auth_methods = session_auth_methods.clone();
                            let prompt_id = prompt_counter.fetch_add(1, Ordering::Relaxed);
                            active_prompt.store(prompt_id, Ordering::Release);
                            let active_prompt_for_result = Arc::clone(&active_prompt);
                            let prompt_stderr_tail = Arc::clone(&stderr_tail_for_connection);
                            // Dropped when the result arrives (or the connection dies
                            // with the request pending) so the timeout thread below
                            // wakes immediately instead of sleeping the full window.
                            let (prompt_done_tx, prompt_done_rx) = mpsc::channel::<()>();
                            connection
                                .send_request(PromptRequest::new(session_id, prompt_blocks))
                                .on_receiving_result(move |result| async move {
                                    drop(prompt_done_tx);
                                    active_prompt_for_result.store(0, Ordering::Release);
                                    match result {
                                        Ok(response) => {
                                            if let Some(usage) = response.usage {
                                                let _ = event_tx
                                                    .send(AcpEvent::TokenUsageBreakdown {
                                                        input_tokens: usage.input_tokens,
                                                        output_tokens: usage.output_tokens,
                                                        cached_read_tokens: usage
                                                            .cached_read_tokens,
                                                    })
                                                    .await;
                                            }
                                            let _ = event_tx
                                                .send(AcpEvent::TurnEnded {
                                                    stop_reason: format!(
                                                        "{:?}",
                                                        response.stop_reason
                                                    ),
                                                })
                                                .await;
                                        }
                                        // The session stays alive; the agent
                                        // just refused this turn pending
                                        // authentication. Route it through
                                        // the same guidance text a startup
                                        // failure gets (F-CHAT-02) rather
                                        // than the bare transport string.
                                        Err(error) if error.code == ErrorCode::AuthRequired => {
                                            let message = AcpError::AuthRequired {
                                                methods: prompt_auth_methods,
                                            }
                                            .to_string();
                                            let _ = event_tx
                                                .send(AcpEvent::TransportError(message))
                                                .await;
                                        }
                                        Err(error) => {
                                            let report = stderr_tail_report(&prompt_stderr_tail);
                                            let _ = event_tx
                                                .send(AcpEvent::TransportError(format!(
                                                    "prompt failed: {error}{report}"
                                                )))
                                                .await;
                                        }
                                    }
                                    Ok(())
                                })?;
                            let active_prompt = Arc::clone(&active_prompt);
                            let timeout_reason = Arc::clone(&prompt_timeout_reason);
                            let child = Arc::clone(&child_for_prompt);
                            let prompt_activity = Arc::clone(&activity_for_connection);
                            touch_activity(&prompt_activity);
                            let _ = thread::Builder::new()
                                .name("sirio-acp-prompt-timeout".into())
                                .spawn(move || {
                                    // Wait out the idle window, then wait again
                                    // for whatever the agent's last report
                                    // pushed it back to. Only silence for a
                                    // whole window reaches the kill below.
                                    loop {
                                        let remaining = prompt_timeout
                                            .saturating_sub(idle_for(&prompt_activity));
                                        if remaining.is_zero() {
                                            break;
                                        }
                                        if prompt_done_rx.recv_timeout(remaining)
                                            != Err(mpsc::RecvTimeoutError::Timeout)
                                        {
                                            return;
                                        }
                                    }
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
                                            duration: prompt_timeout,
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
                        Command::SetConfigOption { option_id, value } => {
                            let event_tx = connection_events.clone();
                            let mode_catalog_for_config = Arc::clone(&mode_catalog_for_session);
                            connection
                                .send_request(SetSessionConfigOptionRequest::new(
                                    session.session_id.clone(),
                                    option_id,
                                    SessionConfigOptionValue::value_id(value),
                                ))
                                .on_receiving_result(move |result| async move {
                                    match result {
                                        Ok(response) => {
                                            if let Some(catalog) = mode_catalog_from_options(Some(
                                                &response.config_options,
                                            )) && let Ok(mut guard) =
                                                mode_catalog_for_config.lock()
                                            {
                                                *guard = Some(catalog);
                                            }
                                            if let Some(effort) =
                                                effort_from_options(Some(&response.config_options))
                                            {
                                                let _ =
                                                    event_tx.send(AcpEvent::Effort(effort)).await;
                                            }
                                        }
                                        Err(error) => {
                                            let _ = event_tx
                                                .send(AcpEvent::TransportError(format!(
                                                    "config option selection failed: {error}"
                                                )))
                                                .await;
                                        }
                                    }
                                    Ok(())
                                })?;
                        }
                        Command::SetMode(mode_id) => {
                            let event_tx = connection_events.clone();
                            let mode_catalog_for_result = Arc::clone(&mode_catalog_for_session);
                            let confirmed_mode_id = mode_id.clone();
                            connection
                                .send_request(SetSessionModeRequest::new(
                                    session.session_id.clone(),
                                    mode_id,
                                ))
                                .on_receiving_result(move |result| async move {
                                    match result {
                                        // `SetSessionModeResponse` carries no
                                        // state back — ACP leaves
                                        // confirmation to a later
                                        // `CurrentModeUpdate` — so update
                                        // optimistically, the same tradeoff
                                        // a `CurrentModeUpdate` push resolves
                                        // authoritatively when it arrives.
                                        Ok(_response) => {
                                            apply_current_mode(
                                                &mode_catalog_for_result,
                                                confirmed_mode_id,
                                            );
                                        }
                                        Err(error) => {
                                            let _ = event_tx
                                                .send(AcpEvent::TransportError(format!(
                                                    "set mode failed: {error}"
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

    connection_finished.store(true, Ordering::Release);
    terminate_and_reap_blocking(&child);
    if let Ok(mut ack) = shutdown_ack.lock()
        && let Some(ack) = ack.take()
    {
        let _ = ack.send(());
    }
    if !started.load(Ordering::Acquire) {
        let error =
            if let Some(reason) = timeout_reason.lock().ok().and_then(|reason| reason.clone()) {
                reason
            } else if let Err(rpc_error) = connection_result.as_ref()
                && rpc_error.code == ErrorCode::AuthRequired
            {
                // `initialize` or `session/new` rejected the handshake asking
                // for authentication — distinguish it from a generic transport
                // failure so the caller can show CLI login guidance and an
                // actionable retry instead of a raw error string (F-CHAT-02).
                let methods = auth_methods
                    .lock()
                    .map(|seen| seen.clone())
                    .unwrap_or_default();
                AcpError::AuthRequired { methods }
            } else {
                AcpError::Transport(
                    connection_result
                        .as_ref()
                        .err()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "ACP worker exited before initialization".into()),
                )
            };
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
        } else {
            let report = stderr_tail_report(&stderr_tail);
            let detail = match connection_result {
                Err(error) => format!(": {error}"),
                Ok(()) => String::new(),
            };
            let _ = event_tx.send_blocking(AcpEvent::TransportError(format!(
                "ACP transport closed unexpectedly{detail}{report}"
            )));
        }
    }
}

type EventStreamSender = async_channel::Sender<AcpEvent>;

/// Drains the agent subprocess's stderr so its pipe never backs up and
/// blocks the child, scanning each line for an MCP-configuration-flavored
/// failure (F-CHAT-33) — ACP's wire protocol has no dedicated concept for a
/// misconfigured MCP server (confirmed by inspecting the full v1 schema:
/// no error code, no `SessionUpdate` variant, nothing on
/// `NewSessionResponse` beyond `modes`/`config_options`/`meta`), so a CLI
/// adapter's own stderr text is the only channel such a problem has ever
/// been observed to use.
async fn drain_stderr(
    stderr: impl futures::AsyncRead + Unpin,
    mcp_warnings: Arc<Mutex<Vec<String>>>,
    tail: StderrTail,
) {
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    continue;
                }
                if looks_like_mcp_warning(trimmed) {
                    push_mcp_warning(&mcp_warnings, trimmed.to_string());
                }
                push_stderr_tail(&tail, trimmed.to_string());
            }
        }
    }
}

type ActivityClock = Arc<Mutex<std::time::Instant>>;

fn touch_activity(clock: &ActivityClock) {
    if let Ok(mut at) = clock.lock() {
        *at = std::time::Instant::now();
    }
}

fn idle_for(clock: &ActivityClock) -> Duration {
    clock
        .lock()
        .map(|at| at.elapsed())
        .unwrap_or(Duration::ZERO)
}

type StderrTail = Arc<Mutex<VecDeque<String>>>;

/// How much of the agent's stderr is kept for the death report. Enough for a
/// stack trace's first frames, small enough to sit inside an error message.
const MAX_STDERR_TAIL: usize = 20;

fn push_stderr_tail(tail: &StderrTail, line: String) {
    if let Ok(mut tail) = tail.lock() {
        if tail.len() >= MAX_STDERR_TAIL {
            tail.pop_front();
        }
        tail.push_back(line);
    }
}

fn stderr_tail_report(tail: &StderrTail) -> String {
    let lines = tail
        .lock()
        .map(|tail| tail.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    if lines.is_empty() {
        return String::new();
    }
    format!("\nagent stderr (last {} lines):\n{}", lines.len(), lines.join("\n"))
}

/// The bound on [`AcpClient::mcp_warnings`] — a long session's stderr
/// should not grow this without limit.
const MAX_MCP_WARNINGS: usize = 20;

fn push_mcp_warning(cell: &Arc<Mutex<Vec<String>>>, warning: String) {
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
fn looks_like_mcp_warning(line: &str) -> bool {
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

/// Folds an `AskUserQuestion`-shaped tool input (`{questions: [{header,
/// question, options: [{label, description}]}]}`) into the display fields a
/// question card needs. Only the first question is surfaced; multi-question
/// payloads are rare and the extra ones would need a second card. A
/// `_sirioTextInput` metadata object declares the free-text affordance.
fn parse_permission_question(raw_input: &serde_json::Value) -> Option<PermissionQuestion> {
    let input = raw_input.as_object()?;
    let questions = input.get("questions")?.as_array()?;
    let first = questions.first()?.as_object()?;
    let prompt = first.get("question")?.as_str()?.to_string();
    let header = first
        .get("header")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| prompt.clone());
    // Pi's `ui/select` repeats its header in `question`; a card that prints
    // both would show the same sentence twice.
    let body = if header.trim() == prompt.trim() {
        String::new()
    } else {
        prompt
    };
    // The key is part of the contract an agent writes, not one we control, so
    // the pre-rename spelling stays readable: an agent still emitting
    // `_tillerTextInput` would otherwise lose its text field with no error.
    let text_input = input
        .get("_sirioTextInput")
        .or_else(|| input.get("_tillerTextInput"))
        .and_then(|value| {
            let object = value.as_object()?;
            Some(PermissionTextInput {
                placeholder: object
                    .get("placeholder")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned),
                prefill: object
                    .get("prefill")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned),
            })
        });
    Some(PermissionQuestion {
        header,
        prompt: body,
        text_input,
    })
}

fn plan_entry(entry: &ProtocolPlanEntry) -> PlanEntryInfo {
    PlanEntryInfo {
        content: entry.content.clone(),
        status: match entry.status {
            ProtocolPlanEntryStatus::Pending => "pending",
            ProtocolPlanEntryStatus::InProgress => "in_progress",
            ProtocolPlanEntryStatus::Completed => "completed",
            _ => "pending",
        }
        .into(),
    }
}

fn content_text(content: ContentChunk) -> Option<String> {
    match content.content {
        ContentBlock::Text(text) => Some(text.text),
        _ => None,
    }
}

/// Converts the protocol's tool-call content collection to the app-facing
/// shape (F-CHAT-23/31): text stays text, a diff stays a diff, and anything
/// else (embedded terminal, image, audio) becomes `Other` rather than being
/// silently dropped.
fn tool_call_content_info(content: Vec<ToolCallContent>) -> Vec<ToolCallContentInfo> {
    content
        .into_iter()
        .map(|item| match item {
            ToolCallContent::Content(content) => match content.content {
                ContentBlock::Text(text) => ToolCallContentInfo::Text(text.text),
                _ => ToolCallContentInfo::Other,
            },
            ToolCallContent::Diff(diff) => ToolCallContentInfo::Diff(ToolCallDiff {
                path: diff.path,
                old_text: diff.old_text,
                new_text: diff.new_text,
            }),
            ToolCallContent::Terminal(_) => ToolCallContentInfo::Other,
            _ => ToolCallContentInfo::Other,
        })
        .collect()
}

fn tool_call_location_info(locations: Vec<ToolCallLocation>) -> Vec<ToolCallLocationInfo> {
    locations
        .into_iter()
        .map(|location| ToolCallLocationInfo {
            path: location.path,
            line: location.line,
        })
        .collect()
}

/// Pretty-prints raw tool input/output for display; falls back to the
/// value's default `Display` on the (unreachable in practice) case that
/// re-serializing an already-deserialized `Value` fails.
fn tool_call_raw_json(value: Option<serde_json::Value>) -> Option<String> {
    value.map(|value| serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()))
}

fn notification_to_events(notification: SessionNotification) -> Vec<AcpEvent> {
    match notification.update {
        SessionUpdate::AgentMessageChunk(content) => vec![
            content_text(content)
                .map(AcpEvent::AgentMessageChunk)
                .unwrap_or_else(|| AcpEvent::OtherSessionUpdate {
                    kind: "AgentMessageChunk(non-text)".into(),
                }),
        ],
        SessionUpdate::AgentThoughtChunk(content) => vec![
            content_text(content)
                .map(AcpEvent::ThoughtChunk)
                .unwrap_or_else(|| AcpEvent::OtherSessionUpdate {
                    kind: "AgentThoughtChunk(non-text)".into(),
                }),
        ],
        SessionUpdate::ToolCall(tool) => vec![AcpEvent::ToolCallStarted {
            id: tool.tool_call_id.to_string(),
            title: tool.title,
            status: format!("{:?}", tool.status),
            kind: format!("{:?}", tool.kind),
            content: tool_call_content_info(tool.content),
            locations: tool_call_location_info(tool.locations),
            raw_input: tool_call_raw_json(tool.raw_input),
            raw_output: tool_call_raw_json(tool.raw_output),
        }],
        SessionUpdate::ToolCallUpdate(update) => {
            let status = update.fields.status.map(|status| format!("{:?}", status));
            let kind = update.fields.kind.map(|kind| format!("{:?}", kind));
            let content = update.fields.content.map(tool_call_content_info);
            let locations = update.fields.locations.map(tool_call_location_info);
            let raw_input = tool_call_raw_json(update.fields.raw_input);
            let raw_output = tool_call_raw_json(update.fields.raw_output);
            vec![if matches!(
                update.fields.status,
                Some(ToolCallStatus::Completed | ToolCallStatus::Failed)
            ) {
                AcpEvent::ToolCallCompleted {
                    id: update.tool_call_id.to_string(),
                    status: status.unwrap_or_else(|| "Unknown".into()),
                    kind,
                    content,
                    locations,
                    raw_input,
                    raw_output,
                }
            } else {
                AcpEvent::ToolCallUpdated {
                    id: update.tool_call_id.to_string(),
                    title: update.fields.title,
                    status,
                    kind,
                    content,
                    locations,
                    raw_input,
                    raw_output,
                }
            }]
        }
        SessionUpdate::UsageUpdate(usage) => vec![AcpEvent::ContextUsage(ContextUsage {
            used: usage.used,
            size: usage.size,
            cost: usage.cost.map(|cost| ContextCost {
                amount: cost.amount,
                currency: cost.currency,
            }),
            input_tokens: None,
            output_tokens: None,
            cached_read_tokens: None,
        })],
        SessionUpdate::AvailableCommandsUpdate(update) => {
            vec![AcpEvent::AvailableCommands(
                update
                    .available_commands
                    .into_iter()
                    .map(|command| AvailableCommandInfo {
                        name: command.name,
                        description: command.description,
                    })
                    .collect(),
            )]
        }
        SessionUpdate::ConfigOptionUpdate(update) => {
            let options = update.config_options;
            let mut events = Vec::with_capacity(2);
            if let Some(catalog) = model_catalog_from_options(Some(&options)) {
                events.push(AcpEvent::ModelCatalog(catalog));
            }
            if let Some(effort) = effort_from_options(Some(&options)) {
                events.push(AcpEvent::Effort(effort));
            }
            if events.is_empty() {
                events.push(AcpEvent::OtherSessionUpdate {
                    kind: "ConfigOptionUpdate".into(),
                });
            }
            events
        }
        SessionUpdate::Plan(plan) => vec![AcpEvent::PlanUpdate {
            entries: plan.entries.iter().map(plan_entry).collect(),
        }],
        // The cell update that actually matters already happened at the
        // notification-handler call site, before this pure fold ever sees
        // the update (F-CHAT-15) — this arm only keeps the signal a caller
        // gets out of the generic "other" bucket so it knows to re-read
        // `AcpClient::mode_catalog` rather than mistaking this for
        // unmodeled protocol traffic.
        SessionUpdate::CurrentModeUpdate(update) => vec![AcpEvent::OtherSessionUpdate {
            kind: format!("CurrentModeUpdate({})", update.current_mode_id),
        }],
        _ => vec![AcpEvent::OtherSessionUpdate {
            kind: "other".into(),
        }],
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

/// Extracts the effort-level selector from a session configuration option
/// list: the select whose id is `effort` (or whose category tag reads
/// `effort`, as OpenCode reports it).
fn effort_from_options(options: Option<&Vec<SessionConfigOption>>) -> Option<EffortOption> {
    let option = options?.iter().find(|option| {
        option.id.to_string() == "effort"
            || matches!(&option.category, Some(SessionConfigOptionCategory::Other(category)) if category == "effort")
    })?;
    let SessionConfigKind::Select(select) = &option.kind else {
        return None;
    };
    let choices = match &select.options {
        SessionConfigSelectOptions::Ungrouped(values) => values.clone(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter().cloned())
            .collect(),
        _ => Vec::new(),
    };
    Some(EffortOption {
        option_id: option.id.to_string(),
        name: Some(option.name.clone()),
        current_value: Some(select.current_value.to_string()),
        choices: choices
            .into_iter()
            .map(|choice| EffortChoice {
                value: choice.value.to_string(),
                name: choice.name,
            })
            .collect(),
    })
}

/// Extracts the session-mode selector from a session configuration option
/// list: the select whose id is `mode` (or whose category tag reads
/// `mode`, as OpenCode reports it).
fn mode_catalog_from_options(options: Option<&Vec<SessionConfigOption>>) -> Option<ModeCatalog> {
    let option = options?.iter().find(|option| {
        option.id.to_string() == "mode"
            || matches!(&option.category, Some(SessionConfigOptionCategory::Mode))
            || matches!(&option.category, Some(SessionConfigOptionCategory::Other(category)) if category == "mode")
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
    Some(ModeCatalog {
        current_id: select.current_value.to_string(),
        options: values
            .into_iter()
            .map(|option| AgentMode {
                id: option.value.to_string(),
                name: option.name.clone(),
                description: option.description.clone(),
            })
            .collect(),
        config_option_id: Some(option.id.to_string()),
    })
}

/// Converts ACP's `SessionModeState` (from `session/new`'s `modes` field)
/// into the client's [`ModeCatalog`] (F-CHAT-15).
fn mode_catalog_from_state(state: &SessionModeState) -> ModeCatalog {
    ModeCatalog {
        current_id: state.current_mode_id.to_string(),
        options: state
            .available_modes
            .iter()
            .map(|mode| AgentMode {
                id: mode.id.to_string(),
                name: mode.name.clone(),
                description: mode.description.clone(),
            })
            .collect(),
        config_option_id: None,
    }
}

/// Updates the live mode-catalog cell's current selection in place, from
/// either a confirmed `session/set_mode` or an agent-pushed
/// `CurrentModeUpdate` (F-CHAT-15). Synthesizes an options-less catalog if
/// nothing was seeded yet — the agent that pushes a mode change without
/// ever having advertised `session/new`'s `modes` field is not one this
/// crate has observed, but `AcpClient::mode_catalog` should still report
/// the current mode id rather than staying `None`.
fn apply_current_mode(cell: &Arc<Mutex<Option<ModeCatalog>>>, current_id: String) {
    if let Ok(mut catalog) = cell.lock() {
        match catalog.as_mut() {
            Some(existing) => existing.current_id = current_id,
            None => {
                *catalog = Some(ModeCatalog {
                    current_id,
                    options: Vec::new(),
                    config_option_id: None,
                });
            }
        }
    }
}

/// The `file://` URI for an absolute path, as the agent side receives it.
///
/// A POSIX path already starts with `/`, so concatenating it onto `file://`
/// lands on the well-formed three-slash form (`file:///home/me/notes.md`):
/// empty authority, then the absolute path. A Windows path starts with a
/// drive letter and separates with `\`, so the same concatenation yields
/// `file://C:\Users\me\notes.md`, which is malformed on two axes at once —
/// the authority slot swallows `C:`, making it read as a host, and
/// backslashes are not URI separators. The drive form needs the third slash
/// and forward separators: `file:///C:/Users/me/notes.md`.
///
/// Percent-encoding is deliberately NOT applied, on either platform. The
/// POSIX arm has always passed paths through raw, so encoding them now would
/// change what every existing agent receives for every mention — a wire
/// contract change, not a portability fix. A path containing a space or `#`
/// is therefore still not a strictly valid URI. That is a real gap, and it
/// belongs to a deliberate agreement with the agent side rather than to a
/// silent side effect of making Windows work.
fn file_uri(path: &Path) -> String {
    #[cfg(windows)]
    {
        // `display()` keeps the platform separators; only the separators and
        // the leading slash differ from the POSIX form.
        format!("file:///{}", path.display().to_string().replace('\\', "/"))
    }
    #[cfg(not(windows))]
    {
        format!("file://{}", path.display())
    }
}

/// Assembles the `session/prompt` content blocks from a draft, mirroring the
/// reference app's `ChatPromptBuilder`: the trimmed text first, then one
/// resource link per mentioned path (absolute paths as-is, relative paths
/// resolved against the session working directory, both serialized as
/// `file://` URIs — see [`file_uri`]), then one image block per attachment.
fn prompt_blocks(
    text: &str,
    mention_paths: &[String],
    images: &[ImageAttachment],
    cwd: &Path,
) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        blocks.push(ContentBlock::Text(TextContent::new(trimmed)));
    }
    for path in mention_paths {
        let path = Path::new(path);
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        blocks.push(ContentBlock::ResourceLink(ResourceLink::new(
            name,
            file_uri(&resolved),
        )));
    }
    for image in images {
        blocks.push(ContentBlock::Image(ImageContent::new(
            image.base64_data.clone(),
            image.mime_type.clone(),
        )));
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        Diff, ToolCall, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
    };

    #[test]
    fn maps_text_updates_to_typed_events() {
        let message = SessionNotification::new(
            "session",
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new("ciao"),
            ))),
        );
        assert_eq!(
            notification_to_events(message),
            vec![AcpEvent::AgentMessageChunk("ciao".into())]
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
            notification_to_events(update),
            vec![AcpEvent::ToolCallCompleted {
                id: "tool".into(),
                status: "Completed".into(),
                kind: None,
                content: None,
                locations: None,
                raw_input: None,
                raw_output: None,
            }]
        );
    }

    #[test]
    fn maps_started_tool_call_to_started_event() {
        let update = SessionNotification::new(
            "session",
            SessionUpdate::ToolCall(ToolCall::new("tool", "Read file")),
        );
        assert!(matches!(
            notification_to_events(update).as_slice(),
            [AcpEvent::ToolCallStarted { id, title, .. }] if id == "tool" && title == "Read file"
        ));
    }

    /// P91 part 2: the widened seam. A tool call whose content is a diff
    /// and whose input/output are raw JSON must carry `kind`, the diff, the
    /// location, and both raw values all the way to the typed event — not
    /// just `{id, title, status}`.
    #[test]
    fn started_tool_call_carries_kind_content_locations_and_raw_input_and_output() {
        let tool = ToolCall::new("tool-1", "Edit file")
            .kind(ToolKind::Edit)
            .content(vec![ToolCallContent::Diff(Diff::new(
                "/tmp/example.rs",
                "new contents\n",
            ))])
            .locations(vec![ToolCallLocation::new("/tmp/example.rs").line(3)])
            .raw_input(serde_json::json!({"path": "/tmp/example.rs"}))
            .raw_output(serde_json::json!({"bytesWritten": 12}));
        let update = SessionNotification::new("session", SessionUpdate::ToolCall(tool));

        let events = notification_to_events(update);
        assert_eq!(events.len(), 1);
        match &events[0] {
            AcpEvent::ToolCallStarted {
                id,
                title,
                kind,
                content,
                locations,
                raw_input,
                raw_output,
                ..
            } => {
                assert_eq!(id, "tool-1");
                assert_eq!(title, "Edit file");
                assert_eq!(kind, "Edit");
                assert_eq!(
                    content,
                    &vec![ToolCallContentInfo::Diff(ToolCallDiff {
                        path: PathBuf::from("/tmp/example.rs"),
                        old_text: None,
                        new_text: "new contents\n".into(),
                    })]
                );
                assert_eq!(
                    locations,
                    &vec![ToolCallLocationInfo {
                        path: PathBuf::from("/tmp/example.rs"),
                        line: Some(3),
                    }]
                );
                let raw_input = raw_input.as_ref().expect("raw input was set");
                assert!(raw_input.contains("/tmp/example.rs"));
                let raw_output = raw_output.as_ref().expect("raw output was set");
                assert!(raw_output.contains("bytesWritten"));
            }
            other => panic!("expected ToolCallStarted, got {other:?}"),
        }
    }

    /// The status-only path (widget/spinner updates with no new content)
    /// must not manufacture kind/content/locations/input the agent never
    /// sent — `None` all the way through, not `Some(default)`.
    #[test]
    fn status_only_update_leaves_the_widened_fields_unset() {
        let update = SessionNotification::new(
            "session",
            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                "tool-1",
                ToolCallUpdateFields::default().status(ToolCallStatus::InProgress),
            )),
        );
        assert_eq!(
            notification_to_events(update),
            vec![AcpEvent::ToolCallUpdated {
                id: "tool-1".into(),
                title: None,
                status: Some("InProgress".into()),
                kind: None,
                content: None,
                locations: None,
                raw_input: None,
                raw_output: None,
            }]
        );
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
            notification_to_events(update),
            vec![AcpEvent::ContextUsage(ContextUsage {
                used: 53_000,
                size: 200_000,
                cost: Some(ContextCost {
                    amount: 0.045,
                    currency: "USD".into(),
                }),
                input_tokens: None,
                output_tokens: None,
                cached_read_tokens: None,
            })]
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
            notification_to_events(update).as_slice(),
            [AcpEvent::ModelCatalog(ModelCatalog { selected_id, .. })] if selected_id == "haiku"
        ));
    }

    #[test]
    fn maps_available_commands_update_to_typed_event() {
        let update = SessionNotification::new(
            "session",
            SessionUpdate::AvailableCommandsUpdate(
                agent_client_protocol::schema::v1::AvailableCommandsUpdate::new(vec![
                    agent_client_protocol::schema::v1::AvailableCommand::new(
                        "cr",
                        "Code review the diff",
                    ),
                    agent_client_protocol::schema::v1::AvailableCommand::new(
                        "research",
                        "Research a topic",
                    ),
                ]),
            ),
        );
        assert_eq!(
            notification_to_events(update),
            vec![AcpEvent::AvailableCommands(vec![
                AvailableCommandInfo {
                    name: "cr".into(),
                    description: "Code review the diff".into(),
                },
                AvailableCommandInfo {
                    name: "research".into(),
                    description: "Research a topic".into(),
                },
            ])]
        );
    }

    #[test]
    fn maps_effort_selector_to_effort_event_alongside_the_model_catalog() {
        let update = SessionNotification::new(
            "session",
            SessionUpdate::ConfigOptionUpdate(
                agent_client_protocol::schema::v1::ConfigOptionUpdate::new(vec![
                    SessionConfigOption::select(
                        "model",
                        "Model",
                        "sonnet",
                        vec![
                            agent_client_protocol::schema::v1::SessionConfigSelectOption::new(
                                "sonnet", "Sonnet",
                            ),
                        ],
                    )
                    .category(SessionConfigOptionCategory::Model),
                    SessionConfigOption::select(
                        "effort",
                        "Reasoning effort",
                        "medium",
                        vec![
                            agent_client_protocol::schema::v1::SessionConfigSelectOption::new(
                                "low", "Low",
                            ),
                            agent_client_protocol::schema::v1::SessionConfigSelectOption::new(
                                "medium", "Medium",
                            ),
                            agent_client_protocol::schema::v1::SessionConfigSelectOption::new(
                                "high", "High",
                            ),
                        ],
                    )
                    .category(SessionConfigOptionCategory::Other("effort".into())),
                ]),
            ),
        );
        assert_eq!(
            notification_to_events(update),
            vec![
                AcpEvent::ModelCatalog(ModelCatalog {
                    config_id: "model".into(),
                    selected_id: "sonnet".into(),
                    options: vec![ModelOption {
                        id: "sonnet".into(),
                        name: "Sonnet".into(),
                        description: None,
                    }],
                }),
                AcpEvent::Effort(EffortOption {
                    option_id: "effort".into(),
                    name: Some("Reasoning effort".into()),
                    current_value: Some("medium".into()),
                    choices: vec![
                        EffortChoice {
                            value: "low".into(),
                            name: "Low".into(),
                        },
                        EffortChoice {
                            value: "medium".into(),
                            name: "Medium".into(),
                        },
                        EffortChoice {
                            value: "high".into(),
                            name: "High".into(),
                        },
                    ],
                }),
            ]
        );
    }

    #[test]
    fn extracts_effort_choices_from_session_configuration() {
        let options = vec![SessionConfigOption::select(
            "effort",
            "Reasoning effort",
            "high",
            vec![
                agent_client_protocol::schema::v1::SessionConfigSelectOption::new("low", "Low"),
                agent_client_protocol::schema::v1::SessionConfigSelectOption::new("high", "High"),
            ],
        )];
        let effort = effort_from_options(Some(&options)).expect("effort selector");
        assert_eq!(effort.option_id, "effort");
        assert_eq!(effort.current_value.as_deref(), Some("high"));
        assert_eq!(
            effort.choices,
            vec![
                EffortChoice {
                    value: "low".into(),
                    name: "Low".into(),
                },
                EffortChoice {
                    value: "high".into(),
                    name: "High".into(),
                },
            ]
        );
    }

    #[test]
    fn mode_catalog_from_options_extracts_opencode_mode() {
        let options = vec![
            SessionConfigOption::select(
                "mode",
                "Session Mode",
                "build",
                vec![
                    agent_client_protocol::schema::v1::SessionConfigSelectOption::new(
                        "build", "Build",
                    ),
                    agent_client_protocol::schema::v1::SessionConfigSelectOption::new(
                        "plan", "Plan",
                    ),
                ],
            )
            .category(SessionConfigOptionCategory::Other("mode".into())),
        ];
        let catalog = mode_catalog_from_options(Some(&options)).expect("mode selector");
        assert_eq!(catalog.current_id, "build");
        assert_eq!(catalog.config_option_id, Some("mode".into()));
        assert_eq!(
            catalog.options,
            vec![
                AgentMode {
                    id: "build".into(),
                    name: "Build".into(),
                    description: None,
                },
                AgentMode {
                    id: "plan".into(),
                    name: "Plan".into(),
                    description: None,
                },
            ]
        );
    }

    #[test]
    fn mode_catalog_from_options_returns_none_without_mode_entry() {
        let options = vec![
            SessionConfigOption::select(
                "model",
                "Model",
                "sonnet",
                vec![
                    agent_client_protocol::schema::v1::SessionConfigSelectOption::new(
                        "sonnet", "Sonnet",
                    ),
                ],
            )
            .category(SessionConfigOptionCategory::Model),
        ];
        assert!(mode_catalog_from_options(Some(&options)).is_none());
        assert!(mode_catalog_from_options(None).is_none());
        assert!(mode_catalog_from_options(Some(&Vec::new())).is_none());
    }

    #[test]
    fn mode_catalog_from_state_has_no_config_option_id() {
        let state = SessionModeState::new("ask", vec![]);
        let catalog = mode_catalog_from_state(&state);
        assert_eq!(catalog.config_option_id, None);
    }

    #[test]
    fn prompt_blocks_builds_trimmed_text_resource_links_and_images() {
        let cwd = std::env::temp_dir().join("sirio-acp-blocks-test");
        let blocks = prompt_blocks(
            "  hello world  \n",
            &["sub/notes.md".into(), "/abs/file.png".into()],
            &[ImageAttachment {
                mime_type: "image/png".into(),
                base64_data: "AAAA".into(),
            }],
            &cwd,
        );
        assert_eq!(blocks.len(), 4, "text + two links + one image");
        assert_eq!(
            blocks[0],
            ContentBlock::Text(TextContent::new("hello world"))
        );
        // The expected URI is built through the same helper the production
        // path uses, so this assertion pins the RESOLUTION (relative mention
        // joined onto the session cwd) rather than re-deriving the spelling.
        // The shape assertions below are what pin the spelling, and they are
        // written out rather than computed — otherwise a helper that emitted
        // nonsense would agree with itself and the test would pass.
        assert_eq!(
            blocks[1],
            ContentBlock::ResourceLink(ResourceLink::new(
                "notes.md",
                file_uri(&cwd.join("sub").join("notes.md"))
            ))
        );
        // `/abs/file.png` is absolute on unix but NOT on Windows, where an
        // absolute path needs a drive or a UNC prefix — so production
        // resolves it against the session cwd there. Mirror that decision
        // rather than hardcoding one platform's answer.
        let png = Path::new("/abs/file.png");
        let png_resolved = if png.is_absolute() {
            png.to_path_buf()
        } else {
            cwd.join(png)
        };
        assert_eq!(
            blocks[2],
            ContentBlock::ResourceLink(ResourceLink::new("file.png", file_uri(&png_resolved)))
        );

        // Shape, stated independently of the helper: three slashes, then an
        // absolute path with forward separators and no backslash anywhere —
        // `file://C:\Users\…` was the Windows defect, malformed both by the
        // missing slash and by the separators.
        let ContentBlock::ResourceLink(link) = &blocks[1] else {
            panic!("blocks[1] must be a resource link, got {:?}", blocks[1]);
        };
        assert!(
            link.uri.starts_with("file:///"),
            "a file URI needs the empty authority and its third slash: {}",
            link.uri
        );
        assert!(
            !link.uri.contains('\\'),
            "URI separators are forward slashes on every platform: {}",
            link.uri
        );
        assert!(
            link.uri.ends_with("/sub/notes.md"),
            "the resolved mention must survive into the URI: {}",
            link.uri
        );
        assert_eq!(
            blocks[3],
            ContentBlock::Image(ImageContent::new("AAAA", "image/png"))
        );
    }

    #[test]
    fn prompt_blocks_omits_whitespace_only_text() {
        let blocks = prompt_blocks("   ", &[], &[], &std::env::temp_dir());
        assert!(blocks.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn windows_batch_commands_are_run_through_cmd() {
        let command = command_for_process(
            AgentCommand::new(r"C:\Program Files\OpenCode.CMD").args(["acp", "--stdio"]),
        );

        assert_eq!(
            command.program.file_name().and_then(|name| name.to_str()),
            Some("cmd.exe")
        );
        assert_eq!(
            command.args,
            vec![
                "/d",
                "/c",
                r"C:\Program Files\OpenCode.CMD",
                "acp",
                "--stdio"
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn non_batch_commands_are_not_wrapped() {
        let command = AgentCommand::new(r"C:\Program Files\node.exe").arg("agent.js");
        assert_eq!(command_for_process(command.clone()), command);
    }

    #[cfg(unix)]
    #[test]
    fn silent_agent_hits_startup_timeout_and_is_terminated() {
        let started = std::time::Instant::now();
        let timeout = Duration::from_millis(100);
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

    /// A tiny ACP agent fixture: reads JSON-RPC lines on stdin and replies
    /// per the `case` arms in `body`, matching a substring of the request
    /// line (mirrors the fixture style already used to test the chat layer
    /// against a fake agent, e.g. `sirio_ui::chat`'s ACP tests).
    #[cfg(unix)]
    fn fixture_agent(body: &str) -> AgentCommand {
        AgentCommand::new("/bin/sh").args([
            "-c".to_string(),
            format!(
                r#"while IFS= read -r line; do id=$(printf '%s' "$line" | sed -E 's/.*"id":([^,]+),.*/\1/'); case "$line" in {body} esac; done"#
            ),
        ])
    }

    #[cfg(unix)]
    #[test]
    fn session_creation_auth_required_error_becomes_typed_auth_required() {
        // (F-CHAT-02) The agent advertises a login method during
        // `initialize`, then refuses `session/new` with ACP's
        // `auth_required` error (wire code -32000, ErrorCode::AuthRequired).
        let command = fixture_agent(
            r#"*initialize*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"protocolVersion":1,"agentCapabilities":{},"authMethods":[{"id":"login","name":"Login","description":"agent auth login"}]}}' ;; *session/new*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"error":{"code":-32000,"message":"Authentication required"}}' ;;"#,
        );
        let result = AcpClient::launch_with_timeout(command, ".", Duration::from_secs(5));

        let error = match result {
            Ok((mut client, _events)) => {
                let _ = client.shutdown();
                panic!("agent requiring auth unexpectedly created a session")
            }
            Err(error) => error,
        };

        match error.downcast_ref::<AcpError>() {
            Some(AcpError::AuthRequired { methods }) => {
                assert_eq!(
                    methods,
                    &[AuthMethodInfo {
                        id: "login".into(),
                        name: "Login".into(),
                        description: Some("agent auth login".into()),
                    }]
                );
            }
            other => panic!("expected typed AuthRequired error, got {other:?} ({error:#})"),
        }
        // The Display text is what actually reaches the transcript today
        // (chat.rs formats the launch error with `{error:#}`), so it must
        // carry the auth-guidance content, not a bare transport string.
        assert!(
            format!("{error:#}").contains("Login"),
            "auth guidance should name the advertised method, got {error:#}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn successful_session_still_carries_advertised_auth_methods() {
        // An agent may advertise a login method while still letting an
        // already-authenticated session through — `InitializeInfo` must
        // carry that even when nothing blocked on it.
        let command = fixture_agent(
            r#"*initialize*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"protocolVersion":1,"agentCapabilities":{},"authMethods":[{"id":"login","name":"Login"}]}}' ;; *session/new*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"sessionId":"test"}}' ;;"#,
        );
        let (mut client, _events) =
            AcpClient::launch_with_timeout(command, ".", Duration::from_secs(5))
                .expect("agent that advertises but does not require auth should still connect");

        assert_eq!(
            client.initialize_info().auth_methods,
            vec![AuthMethodInfo {
                id: "login".into(),
                name: "Login".into(),
                description: None,
            }]
        );
        let _ = client.shutdown();
    }

    #[cfg(unix)]
    #[test]
    fn prompt_auth_required_error_reaches_the_transcript_as_auth_guidance() {
        // (F-CHAT-02) The connection survives past session creation; the
        // agent only refuses the *prompt*. This is the mid-turn half of the
        // row's evidence ("real send confirmed... but transcript pane
        // rendered nothing") — the fix must not require a fresh launch to
        // surface the banner.
        let command = fixture_agent(
            r#"*initialize*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"protocolVersion":1,"agentCapabilities":{},"authMethods":[{"id":"login","name":"Login"}]}}' ;; *session/new*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"sessionId":"test"}}' ;; *session/prompt*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"error":{"code":-32000,"message":"Authentication required"}}' ;;"#,
        );
        let (mut client, events) =
            AcpClient::launch_with_timeout(command, ".", Duration::from_secs(5))
                .expect("fixture agent should create a session");

        client
            .prompt("hello")
            .expect("prompt should be accepted by the worker");

        let event = block_on(events.recv()).expect("worker should report the auth failure");
        match event {
            AcpEvent::TransportError(message) => {
                assert!(
                    message.contains("Login") && message.contains("authentication"),
                    "expected auth guidance in the transport message, got {message:?}"
                );
            }
            other => panic!("expected a TransportError carrying auth guidance, got {other:?}"),
        }

        let _ = client.shutdown();
    }

    #[cfg(unix)]
    #[test]
    fn session_creation_carries_the_advertised_mode_catalog() {
        // (F-CHAT-15) `session/new`'s `modes` field is ACP's session-modes
        // primitive — a distinct wire concept from the `config_options`
        // model/effort selectors this crate already extracts.
        let command = fixture_agent(
            r#"*initialize*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"protocolVersion":1,"agentCapabilities":{},"authMethods":[]}}' ;; *session/new*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"sessionId":"test","modes":{"currentModeId":"ask","availableModes":[{"id":"ask","name":"Ask"},{"id":"plan","name":"Plan","description":"Plan before editing"}]}}}' ;;"#,
        );
        let (mut client, _events) =
            AcpClient::launch_with_timeout(command, ".", Duration::from_secs(5))
                .expect("fixture agent should create a session");

        assert_eq!(
            client.mode_catalog(),
            Some(ModeCatalog {
                current_id: "ask".into(),
                options: vec![
                    AgentMode {
                        id: "ask".into(),
                        name: "Ask".into(),
                        description: None,
                    },
                    AgentMode {
                        id: "plan".into(),
                        name: "Plan".into(),
                        description: Some("Plan before editing".into()),
                    },
                ],
                config_option_id: None,
            })
        );

        let _ = client.shutdown();
    }

    #[cfg(unix)]
    #[test]
    fn agent_without_session_modes_reports_no_mode_catalog() {
        // Most installed ACP agents today don't advertise `modes` at all —
        // `mode_catalog` must stay `None` rather than synthesizing one, so
        // a caller can tell "no selector" apart from "selector with no
        // options" (F-CHAT-15).
        let command = fixture_agent(
            r#"*initialize*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"protocolVersion":1,"agentCapabilities":{},"authMethods":[]}}' ;; *session/new*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"sessionId":"test"}}' ;;"#,
        );
        let (mut client, _events) =
            AcpClient::launch_with_timeout(command, ".", Duration::from_secs(5))
                .expect("fixture agent should create a session");

        assert_eq!(client.mode_catalog(), None);

        let _ = client.shutdown();
    }

    #[cfg(unix)]
    #[test]
    fn set_mode_updates_the_live_catalog_once_the_agent_confirms() {
        // (F-CHAT-15) `SetSessionModeResponse` carries no state back, so the
        // live catalog only advances once the worker thread has actually
        // processed the confirmed response — this drives that round trip
        // through the real worker/command-channel machinery rather than
        // calling the extractor directly.
        let command = fixture_agent(
            r#"*initialize*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"protocolVersion":1,"agentCapabilities":{},"authMethods":[]}}' ;; *session/new*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"sessionId":"test","modes":{"currentModeId":"ask","availableModes":[{"id":"ask","name":"Ask"},{"id":"plan","name":"Plan"}]}}}' ;; *session/set_mode*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{}}' ;;"#,
        );
        let (mut client, _events) =
            AcpClient::launch_with_timeout(command, ".", Duration::from_secs(5))
                .expect("fixture agent should create a session");

        client
            .set_mode("plan")
            .expect("set_mode should be accepted by the worker");

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let updated = loop {
            if let Some(catalog) = client.mode_catalog()
                && catalog.current_id == "plan"
            {
                break catalog;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "mode catalog never reflected the confirmed set_mode, last seen {:?}",
                client.mode_catalog()
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(
            updated.options.len(),
            2,
            "the confirmed set_mode should update current_id in place, not drop the options"
        );

        let _ = client.shutdown();
    }

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

    #[cfg(unix)]
    #[test]
    fn mcp_configuration_failure_on_stderr_is_captured_as_a_warning() {
        // (F-CHAT-33) ACP's wire protocol has no dedicated concept for
        // this — confirmed by inspecting the full v1 schema before writing
        // `looks_like_mcp_warning` — so a CLI adapter's own stderr text,
        // written outside ACP's JSON-RPC channel entirely, is the only
        // place this crate has any hope of observing the problem.
        let command = AgentCommand::new("/bin/sh").args([
            "-c",
            r#"printf '%s\n' 'Error: failed to connect to MCP server "docs": connection refused' >&2
while IFS= read -r line; do id=$(printf '%s' "$line" | sed -E 's/.*"id":([^,]+),.*/\1/'); case "$line" in *initialize*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"protocolVersion":1,"agentCapabilities":{},"authMethods":[]}}' ;; *session/new*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"sessionId":"test"}}' ;; esac; done"#,
        ]);
        let (mut client, _events) =
            AcpClient::launch_with_timeout(command, ".", Duration::from_secs(5))
                .expect("fixture agent should create a session despite the stderr line");

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let warnings = loop {
            let warnings = client.mcp_warnings();
            if !warnings.is_empty() {
                break warnings;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "mcp_warnings never observed the agent's stderr line"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("MCP server"));
        assert!(warnings[0].contains("connection refused"));

        let _ = client.shutdown();
    }

    /// Launches a real agent over `npx`, so it depends on the network, on npm
    /// resolving a package, and on the machine not being busy. Kept because
    /// it is the only end-to-end check of startup, but run it deliberately:
    ///
    ///     cargo test -p sirio_acp -- --ignored
    #[test]
    #[ignore = "needs a real agent over the network; flakes the gate under load"]
    fn real_agent_completes_startup_before_deadline() {
        let started = std::time::Instant::now();
        let program = std::env::var_os("SIRIO_ACP_PROGRAM")
            .expect("set SIRIO_ACP_PROGRAM to run this ignored test against a real agent");
        let result = AcpClient::launch(AgentCommand::new(program), std::env::temp_dir());
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

    /// (F-CHAT-33, sweep I4-settle) Locks in a diagnosis that took two
    /// independent hand-drives of the real npx agent, outside this crate
    /// entirely, to pin down. A broken `.mcp.json` server is real, wired all
    /// the way to `session/new` (`discover_mcp_servers`, above), and the
    /// agent DOES eventually notice it is unreachable -- but only across two
    /// gates this test cannot get past from inside the repo:
    ///
    /// 1. **Trust.** The first time any project's `.mcp.json` names a
    ///    server, the agent marks it "Pending approval (run `claude` to
    ///    approve)" and never attempts a connection at all -- confirmed live
    ///    by hand-driving the JSON-RPC wire directly: with the project
    ///    unapproved, a `claude mcp list` tool call the agent ran on request
    ///    reported `Pending approval`, not a connection attempt. That
    ///    approval lives in the user's own `~/.claude.json` (a per-project
    ///    `enabledMcpjsonServers` list), which nothing in Sirio's launch
    ///    path ever populates -- a fresh Sirio project hits this wall on
    ///    every single session, not just a broken one.
    /// 2. **Channel.** Manually pre-approving the same scratch project (by
    ///    editing `~/.claude.json` directly, outside this repo -- not
    ///    reproducible in an automated test without mutating shared global
    ///    state) and re-driving the wire DID produce a real connection
    ///    attempt and a real failure: `Failed to connect — ENOENT: ENOENT:
    ///    no such file or directory, posix_spawn '/definitely/missing/mcp-
    ///    nonexistent-binary'`. But that text arrived as ordinary
    ///    conversational/tool-call content inside a `session/update` --
    ///    because the agent had been explicitly asked to run `claude mcp
    ///    list` and check -- never once on the child process's own stderr,
    ///    the only channel `drain_stderr`/`looks_like_mcp_warning` reads.
    ///    Nothing was pushed spontaneously either: `session/new`'s response
    ///    and the commands-list push that follows it carry no MCP status at
    ///    all before anyone asks.
    ///
    /// So this crate's own reach ends exactly where this test can still
    /// prove something real: the unapproved case, which is what every
    /// fresh Sirio project actually gets. If a future agent version starts
    /// writing to the child's stderr instead, this goes red and says so.
    #[test]
    #[ignore = "needs a real agent over the network; flakes the gate under load"]
    fn real_agent_stays_silent_on_stderr_for_an_unapproved_broken_mcp_json() {
        let scratch = std::env::temp_dir().join(format!(
            "sirio-f-chat-33-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&scratch).expect("scratch dir");
        std::fs::write(
            scratch.join(".mcp.json"),
            r#"{"mcpServers":{"broken-server":{"command":"/definitely/missing/mcp-nonexistent-binary","args":[]}}}"#,
        )
        .expect("write .mcp.json");

        let program = std::env::var_os("SIRIO_ACP_PROGRAM")
            .expect("set SIRIO_ACP_PROGRAM to run this ignored test against a real agent");
        let (mut client, _events) = AcpClient::launch(AgentCommand::new(program), &scratch)
            .expect("real ACP agent should initialize even with a broken .mcp.json");

        // Give the agent well past its own startup + any lazy connection
        // attempt it might make on its own initiative.
        std::thread::sleep(Duration::from_secs(10));
        let warnings = client.mcp_warnings();
        assert!(
            warnings.is_empty(),
            "if this ever fires, the agent finally started reporting MCP \
             failures on the process's own stderr for an unapproved project \
             -- re-open F-CHAT-33, this is exactly the signal it was waiting \
             on: {warnings:?}"
        );

        let _ = client.shutdown();
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[cfg(unix)]
    fn drain_until_turn_end(events: &EventStream, budget: Duration) -> Vec<AcpEvent> {
        let deadline = std::time::Instant::now() + budget;
        let mut seen = Vec::new();
        while std::time::Instant::now() < deadline {
            let receive = events.recv();
            let timer = async_io::Timer::after(Duration::from_millis(100));
            futures::pin_mut!(receive, timer);
            match block_on(futures::future::select(receive, timer)) {
                futures::future::Either::Left((Ok(event), _)) => {
                    let done = matches!(
                        event,
                        AcpEvent::TurnEnded { .. }
                            | AcpEvent::Timeout { .. }
                            | AcpEvent::TransportError(_)
                    );
                    seen.push(event);
                    if done {
                        break;
                    }
                }
                futures::future::Either::Left((Err(_), _)) => break,
                futures::future::Either::Right(_) => {}
            }
        }
        seen
    }

    #[cfg(unix)]
    #[test]
    fn a_busy_turn_outlives_the_prompt_window() {
        // The watchdog used to cap the whole turn: an agent still streaming
        // after the window was SIGKILLed anyway, which killed the session and
        // surfaced as a bare "incoming transport closed" on session/prompt.
        // Six chunks 200ms apart run well past this 500ms window.
        let command = fixture_agent(
            r#"*initialize*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"protocolVersion":1,"agentCapabilities":{},"authMethods":[]}}' ;; *session/new*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"sessionId":"test"}}' ;; *session/prompt*) i=0; while [ $i -lt 6 ]; do sleep 0.2; printf '%s\n' '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"test","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"tick"}}}}'; i=$((i+1)); done; printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"stopReason":"end_turn"}}' ;;"#,
        );
        let (mut client, events) = AcpClient::launch_with_timeouts(
            command,
            ".",
            Duration::from_secs(5),
            Duration::from_millis(500),
        )
        .expect("fixture agent should create a session");

        client.prompt("work for a while").expect("prompt accepted");
        let seen = drain_until_turn_end(&events, Duration::from_secs(10));

        assert!(
            seen.iter()
                .any(|event| matches!(event, AcpEvent::TurnEnded { .. })),
            "a turn that kept reporting never finished: {seen:?}"
        );
        assert!(
            !seen.iter().any(|event| matches!(
                event,
                AcpEvent::Timeout {
                    operation: TimeoutOperation::Prompt,
                    ..
                }
            )),
            "a working agent was killed by the idle watchdog: {seen:?}"
        );

        let _ = client.shutdown();
    }

    #[cfg(unix)]
    #[test]
    fn a_silent_turn_still_trips_the_prompt_window() {
        // The other half of the contract: silence for a whole window is a
        // hung agent and must still be reaped.
        let command = fixture_agent(
            r#"*initialize*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"protocolVersion":1,"agentCapabilities":{},"authMethods":[]}}' ;; *session/new*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"sessionId":"test"}}' ;; *session/prompt*) sleep 30 ;;"#,
        );
        let (mut client, events) = AcpClient::launch_with_timeouts(
            command,
            ".",
            Duration::from_secs(5),
            Duration::from_millis(300),
        )
        .expect("fixture agent should create a session");

        client.prompt("say nothing").expect("prompt accepted");
        let started = std::time::Instant::now();
        let seen = drain_until_turn_end(&events, Duration::from_secs(5));
        let elapsed = started.elapsed();

        // Left alone the fixture sits for 30s, so an error this early is the
        // watchdog reaping it. The turn's own failure is what the user sees;
        // the connection-level `Timeout` never races ahead of it.
        assert!(
            seen.iter()
                .any(|event| matches!(event, AcpEvent::TransportError(_) | AcpEvent::Timeout { .. })),
            "a silent agent was never reaped: {seen:?}"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "the reap took {elapsed:?}, far past the 300ms window"
        );

        let _ = client.shutdown();
    }
}
