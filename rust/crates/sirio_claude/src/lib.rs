//! Claude Code's own stdio protocol, as types and pure functions.
//!
//! This crate is what `claude -p --output-format stream-json
//! --input-format stream-json` speaks, with no process attached: parsing,
//! building, and the small tables that turn wire values into things a
//! surface can draw. `sirio_chat::claude` owns the subprocess and the
//! channels; `sirio_usage` uses the same types for a one-shot usage query.
//!
//! The protocol is the Claude Agent SDK's internal contract, not a
//! published API. Everything here is therefore written to degrade rather
//! than reject: see [`message`]'s module docs for the rule.

pub mod catalog;
pub mod context;
pub mod control;
pub mod hooks;
pub mod launch;
pub mod mcp;
pub mod message;
pub mod permissions;
pub mod reload;
pub mod tools;
pub mod usage;
pub mod version;

pub use catalog::{
    AccountInfo, Catalog, CommandInfo, EFFORT_DEFAULT, EFFORT_ULTRACODE, Effort, EffortChoice,
    FastMode, ModeInfo, ModelInfo, Modes,
};
pub use context::{
    CategoryKind, ContextBreakdown, ContextCategory, ContextItem, ContextOrigins, ItemOrigin,
    MemoryFile, OriginNode, Standing,
};
pub use control::{
    CanUseTool, ControlEnvelope, ControlRequest, ElicitationRequest, PermissionResult,
    THINKING_DISPLAYS, control_error, control_request_subtype, elicitation_response, is_refusal,
};
pub use hooks::{Hook, HookEvent, HooksListing, HooksMode, HooksPolicy, parse_hooks_listing};
pub use launch::{ChatSession, LaunchLine};
pub use mcp::{McpAuthStart, McpOrigin, McpServer, McpState, parse_mcp_status};
pub use message::{
    AgentToolResult, AssistantMessage, AssistantPayload, BackgroundTask, CliMessage,
    CompactMetadata, ContentBlock, Delta, LocalCommandRun, McpServerStatus, MessageLifecycle, MessageUsage,
    InvalidConfigFile, ModelUsage, RateLimitPush, ResultPayload, StartupFailure, StreamEventPayload,
    SystemPayload, TaskOutput, TaskUsage, ToolResult, ToolUse, UserMessage, UserPayload,
};
pub use permissions::{PermissionRule, PermissionRules, SettingsError, parse_permission_rules};
pub use reload::{CacheImpact, PluginsReloaded, parse_plugins_reload};
pub use tools::{ToolContent, ToolInfo, ToolKind, ToolLocation, describe, diff_from_result};
pub use usage::{ContextUsageReport, PlanUsage, RateLimitWindow, RewindOutcome, whole_percent};
pub use version::{ClaudeVersion, MIN_CLAUDE_VERSION};
