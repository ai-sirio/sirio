//! What a turn did, as events a chat can build a trace from.
//!
//! Transport-neutral on purpose: only the native Claude client emits these
//! (`claude::trace::TraceFold`), and no ACP client ever does, so a chat that
//! has received none shows no trace. Every instant is the one the worker
//! stamped when it read the line from the agent, not the one the chat
//! received the event at: the GPUI thread can lag behind the pipe.

use std::time::Instant;

/// One step in the life of a turn.
///
/// `parent: None` means the turn's root, the main Claude Code thread.
#[derive(Clone, Debug, PartialEq)]
pub enum TraceEvent {
    /// The user's line was written to the agent.
    TurnStarted { at: Instant },
    /// A span opened. A subagent's id is its `Agent` (formerly `Task`)
    /// `tool_use` id; a tool's is its own; a model call's is the Messages API
    /// `message.id`, except a subagent's final report, `"<agent id>/final"`.
    SpanStarted {
        id: String,
        parent: Option<String>,
        kind: SpanKind,
        name: String,
        detail: String,
        model: Option<String>,
        at: Instant,
    },
    /// The agent named a span it had already opened, as `task_started` does.
    SpanNamed {
        id: String,
        name: String,
        detail: String,
    },
    /// A running subagent's live totals, from `task_progress`.
    SpanProgress {
        id: String,
        tokens: Option<u64>,
        tool_uses: Option<u32>,
        activity: Option<String>,
    },
    /// A model call's tokens.
    SpanUsage { id: String, tokens: TokenCount },
    /// A permission was asked: `span` is the one it waits on, the root when
    /// `None`.
    WaitStarted {
        span: Option<String>,
        request_id: u64,
        at: Instant,
    },
    /// The wait ended: it was answered, it timed out, or its tool returned.
    WaitEnded { request_id: u64, at: Instant },
    /// A span closed. `measured_ms` is the agent's own clock for it where it
    /// has one, and wins over the two instants.
    SpanEnded {
        id: String,
        outcome: SpanOutcome,
        at: Instant,
        measured_ms: Option<u64>,
    },
    /// The turn's `result` arrived. The fold has ended what it knows ends
    /// with the turn: each wait still open, with a `WaitEnded` just ahead of
    /// this event, and a subagent's open tools, which end with their
    /// subagent. Any span still open at this point is one the turn
    /// abandoned, and the store ends it as interrupted: the fold does not
    /// say which they are.
    TurnEnded {
        at: Instant,
        outcome: TurnOutcome,
        /// The agent's own clock for the turn.
        measured_ms: Option<u64>,
        /// The turn's own spend, `None` when it cannot be told apart from
        /// the process's running total.
        cost_usd: Option<f64>,
        /// The turn's own tokens, on the same terms as `cost_usd`.
        tokens: Option<TokenCount>,
    },
}

/// What a span is.
#[derive(Clone, Debug, PartialEq)]
pub enum SpanKind {
    /// An `Agent` tool call and the agent it runs.
    Subagent,
    /// One model call.
    Llm,
    /// Any other tool call.
    Tool,
}

/// How a span ended.
#[derive(Clone, Debug, PartialEq)]
pub enum SpanOutcome {
    Done,
    /// The tool reported an error; `note` is the first line of it.
    Failed {
        note: String,
    },
    /// Still open when its parent or its turn ended.
    Interrupted,
}

/// How a turn ended, as far as its `result` and Sirio's own interrupt say.
#[derive(Clone, Debug, PartialEq)]
pub enum TurnOutcome {
    Done,
    Failed {
        reason: String,
    },
    /// Sirio sent an interrupt this turn, whatever the `result` says.
    Interrupted,
}

/// A token count in the two halves the trace shows: everything the model
/// read (fresh, cached and newly cached input) and what it wrote.
#[derive(Clone, Debug, PartialEq)]
pub struct TokenCount {
    pub input: u64,
    pub output: u64,
}
