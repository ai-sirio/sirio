//! Claude Code's own stdio protocol, as types and pure functions.
//!
//! This crate is what `claude -p --output-format stream-json
//! --input-format stream-json` speaks, with no process attached: parsing,
//! building, and the small tables that turn wire values into things a
//! surface can draw. `sirio_acp::claude` owns the subprocess and the
//! channels; `sirio_usage` uses the same types for a one-shot usage query.
//!
//! The protocol is the Claude Agent SDK's internal contract, not a
//! published API. Everything here is therefore written to degrade rather
//! than reject: see [`message`]'s module docs for the rule.

pub mod catalog;
pub mod control;
pub mod message;

pub use catalog::{
    AccountInfo, Catalog, CommandInfo, Effort, EffortChoice, ModeInfo, ModelInfo, Modes,
};
pub use control::{CanUseTool, ControlEnvelope, ControlRequest, PermissionResult};
pub use message::{
    AssistantMessage, AssistantPayload, CliMessage, ContentBlock, Delta, McpServerStatus,
    ModelUsage, ResultPayload, StreamEventPayload, SystemPayload, ToolResult, ToolUse, UserMessage,
    UserPayload,
};
