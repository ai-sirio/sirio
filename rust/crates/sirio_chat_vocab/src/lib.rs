//! The chat event vocabulary shared by every chat transport.
//!
//! The ACP client and the native Claude client both speak to the chat layer
//! in these types and nothing else: [`ChatEvent`] is the stream, [`ChatError`]
//! the typed failure, and the rest is what the events carry. This crate
//! depends on no local crate, so a transport and a surface can share the
//! vocabulary without depending on each other.

use std::path::PathBuf;
use std::time::Duration;

mod permission;
mod trace_event;

pub use permission::parse_permission_question;
pub use trace_event::{SpanKind, SpanOutcome, TokenCount, TraceEvent, TurnOutcome};

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
pub enum ChatError {
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

impl std::fmt::Display for ChatError {
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

impl std::error::Error for ChatError {}

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

/// The asynchronous event stream returned by `AcpClient::launch`.
pub type EventStream = async_channel::Receiver<ChatEvent>;

/// A permission option exposed to the caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionOption {
    /// The protocol option identifier to send back to the agent.
    pub id: String,
    /// Human-readable label.
    pub name: String,
    /// Protocol-defined permission kind.
    pub kind: String,
    /// What choosing it means, when the agent says. Only a native
    /// `AskUserQuestion` option carries one; ACP options have no such field.
    pub description: Option<String>,
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

/// How much of the model's reasoning the session sends.
///
/// Unlike every other picker's state this is not read from the agent:
/// neither the handshake nor `get_settings` reports the session's current
/// thinking display, so all a surface can honestly know is what it has
/// itself chosen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThinkingDisplay {
    /// What this session chose, or `None` while nothing has been chosen.
    pub chosen: Option<String>,
    /// The agent refused the request — an older CLI — so the control is
    /// not worth offering again.
    pub unsupported: bool,
}

/// One slash command advertised by the agent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailableCommandInfo {
    /// Command name (e.g. `create_plan`).
    pub name: String,
    /// Human-readable description of what the command does.
    pub description: String,
    /// The arguments it takes, as the agent spells them for its own help.
    /// `None` when it takes none — which is how both transports say it:
    /// Claude Code sends an empty `argumentHint`, ACP sends no `input`.
    pub argument_hint: Option<String>,
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
pub enum ChatEvent {
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
        /// Handle passed to `AcpClient::respond_permission`.
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
    /// slice does not own; [`ChatError::AuthRequired`]'s `Display` renders
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
    /// One step in the life of the turn, for the chat's trace. Only the
    /// native Claude client emits these; an ACP agent never does, so a chat
    /// on one has no trace and the transcript is its only record.
    Trace(TraceEvent),
}

/// Something that happened to the session and belongs in the transcript at
/// the point it happened, rather than in a corner of the chrome.
///
/// These ride a drainable queue rather than an [`ChatEvent`] variant on
/// purpose: that enum is matched exhaustively by every consumer in the
/// workspace, so widening it is a breaking change across crates. The
/// signal is an [`ChatEvent::OtherSessionUpdate`] named `SessionNotice`,
/// and the surface drains the queue when it sees one — which keeps the
/// notices in stream order with the rows around them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionNotice {
    /// The context window was compacted and part of the conversation is
    /// no longer in it.
    Compacted {
        /// `manual` when the user asked for it, `auto` when the window
        /// forced it. The second is the one worth reading.
        trigger: Option<String>,
        /// Context size in tokens before and after.
        pre_tokens: Option<u64>,
        post_tokens: Option<u64>,
    },
    /// A task that had been sent to the background reached its end.
    BackgroundTaskEnded {
        /// The CLI's own sentence, already written for a reader.
        summary: String,
    },
    /// The plan stopped allowing turns.
    RateLimited {
        /// The CLI's own word for the refusal.
        status: String,
        /// Which window ran out, when it named one.
        window: Option<String>,
        /// When that window resets, in unix seconds.
        resets_at: Option<i64>,
    },
}
