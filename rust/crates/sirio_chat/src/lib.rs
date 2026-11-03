//! Chat transports for Sirio.
//!
//! A chat tab talks to its agent over one of two transports: an ACP agent
//! (the client lives in `sirio_acp`) or Claude Code over its own stdio
//! protocol (`claude`). [`ChatClient`] is the one surface both reach, and
//! [`ChatSession`] is the non-drawing session state a surface or the control
//! socket drives on top of it.

use anyhow::{Result, anyhow};
use sirio_acp::AcpClient;
use std::path::Path;

mod chat;
mod claude;

pub use chat::{ChatSession, ChatSessionConfig, ChatSnapshot, ChatStatus};
pub use claude::{ClaudeClient, ClaudeLaunch, McpControl};
pub use sirio_acp::AgentCommand;
pub use sirio_chat_vocab::*;

/// What a chat tab launches.
///
/// Two transports reach the same surface: an ACP agent, and Claude Code
/// over its own protocol. Which one a tab uses is resolved at open time
/// (see `sirio`'s `agent_launch_for`) and never persisted, so a machine
/// that gains or loses a `claude` picks the right one on the next open.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchSpec {
    /// An ACP agent: a program that speaks the Agent Client Protocol.
    Acp(AgentCommand),
    /// Claude Code, driven over its own stdio protocol.
    Claude(ClaudeLaunch),
}

impl LaunchSpec {
    /// A short, stable name for logs and the Settings row. Not a label to
    /// show a user on its own.
    #[must_use]
    pub fn transport_name(&self) -> &'static str {
        match self {
            Self::Acp(_) => "acp",
            Self::Claude(_) => "claude",
        }
    }

    /// Whether this is the native Claude transport.
    #[must_use]
    pub fn is_native_claude(&self) -> bool {
        matches!(self, Self::Claude(_))
    }
}

/// A live chat connection, whichever transport backs it.
///
/// An enum rather than a trait object: there are exactly two transports,
/// both known at compile time, and an exhaustive match is what makes a new
/// one impossible to forget.
#[derive(Debug)]
pub enum ChatClient {
    Acp(AcpClient),
    Claude(ClaudeClient),
}

impl ChatClient {
    /// Launches whichever transport the spec names.
    pub fn launch(launch: LaunchSpec, cwd: impl AsRef<Path>) -> Result<(Self, EventStream)> {
        match launch {
            LaunchSpec::Acp(command) => {
                let (client, events) = AcpClient::launch(command, cwd)?;
                Ok((Self::Acp(client), events))
            }
            LaunchSpec::Claude(launch) => {
                let (client, events) = ClaudeClient::launch(launch, cwd)?;
                Ok((Self::Claude(client), events))
            }
        }
    }

    /// The agent's session id. `None` on the native transport until the
    /// first turn opens one.
    #[must_use]
    pub fn session_id(&self) -> Option<String> {
        match self {
            Self::Acp(client) => Some(client.session_id().to_string()),
            Self::Claude(client) => client.session_id(),
        }
    }

    /// The model selector advertised during session creation, if any.
    #[must_use]
    pub fn model_catalog(&self) -> Option<ModelCatalog> {
        match self {
            Self::Acp(client) => client.model_catalog().cloned(),
            Self::Claude(client) => client.model_catalog(),
        }
    }

    /// The session-mode selector, re-read live like the ACP one.
    #[must_use]
    pub fn mode_catalog(&self) -> Option<ModeCatalog> {
        match self {
            Self::Acp(client) => client.mode_catalog(),
            Self::Claude(client) => client.mode_catalog(),
        }
    }

    /// What the session says about fast mode. Only the native transport
    /// has it: ACP has no equivalent, and says so with `None`.
    #[must_use]
    pub fn fast_mode(&self) -> Option<FastMode> {
        match self {
            Self::Acp(_) => None,
            Self::Claude(client) => client.fast_mode(),
        }
    }

    /// Notices posted since the last read, draining them. Read on an
    /// [`ChatEvent::OtherSessionUpdate`] named `SessionNotice`. The ACP
    /// transport reports none: the protocol carries no equivalent.
    pub fn take_notices(&self) -> Vec<SessionNotice> {
        match self {
            Self::Acp(_) => Vec::new(),
            Self::Claude(client) => client.take_notices(),
        }
    }

    /// How many background tasks are running, where the transport says.
    #[must_use]
    pub fn background_task_count(&self) -> usize {
        match self {
            Self::Acp(_) => 0,
            Self::Claude(client) => client.background_task_count(),
        }
    }

    /// How much of the model's reasoning this session sends, where the
    /// transport has such a thing.
    #[must_use]
    pub fn thinking_display(&self) -> Option<ThinkingDisplay> {
        match self {
            Self::Acp(_) => None,
            Self::Claude(client) => Some(client.thinking_display()),
        }
    }

    /// Choose how much of the reasoning to receive; `None` leaves the
    /// agent on its own answer.
    pub fn set_thinking_display(&self, display: Option<String>) -> Result<()> {
        match self {
            Self::Acp(_) => Ok(()),
            Self::Claude(client) => client.set_thinking_display(display),
        }
    }

    /// Turn fast mode on or off, where the transport has it.
    pub fn set_fast_mode(&self, enabled: bool) -> Result<()> {
        match self {
            Self::Acp(_) => Ok(()),
            Self::Claude(client) => client.set_fast_mode(enabled),
        }
    }

    /// MCP-configuration-flavored lines observed on the agent's stderr.
    #[must_use]
    pub fn mcp_warnings(&self) -> Vec<String> {
        match self {
            Self::Acp(client) => client.mcp_warnings(),
            Self::Claude(client) => client.mcp_warnings(),
        }
    }

    /// Send a user turn without blocking on its streamed response.
    pub fn prompt(&self, text: impl Into<String>) -> Result<()> {
        match self {
            Self::Acp(client) => client.prompt(text),
            Self::Claude(client) => client.prompt(text),
        }
    }

    /// Send a user turn composed of text, file mentions, and images.
    pub fn prompt_content(
        &self,
        text: impl Into<String>,
        mention_paths: Vec<String>,
        images: Vec<ImageAttachment>,
        cwd: impl AsRef<Path>,
    ) -> Result<()> {
        match self {
            Self::Acp(client) => client.prompt_content(text, mention_paths, images, cwd),
            Self::Claude(client) => client.prompt_content(text, mention_paths, images, cwd),
        }
    }

    /// Ask the agent to change the current session model.
    pub fn set_model(&self, config_id: impl Into<String>, value: impl Into<String>) -> Result<()> {
        match self {
            Self::Acp(client) => client.set_model(config_id, value),
            Self::Claude(client) => client.set_model(config_id, value),
        }
    }

    /// Ask the agent to change a non-model configuration option (effort).
    pub fn set_config_option(
        &self,
        option_id: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<()> {
        match self {
            Self::Acp(client) => client.set_config_option(option_id, value),
            Self::Claude(client) => client.set_config_option(option_id, value),
        }
    }

    /// Ask the agent to switch to a different session mode.
    pub fn set_mode(&self, mode_id: impl Into<String>) -> Result<()> {
        match self {
            Self::Acp(client) => client.set_mode(mode_id),
            Self::Claude(client) => client.set_mode(mode_id),
        }
    }

    /// Cancel the current prompt turn.
    pub fn cancel(&self) -> Result<()> {
        match self {
            Self::Acp(client) => client.cancel(),
            Self::Claude(client) => client.cancel(),
        }
    }

    /// Answer a permission request surfaced in [`ChatEvent::PermissionRequest`].
    pub fn respond_permission(&self, request_id: u64, option_id: impl Into<String>) -> Result<()> {
        match self {
            Self::Acp(client) => client.respond_permission(request_id, option_id),
            Self::Claude(client) => client.respond_permission(request_id, option_id),
        }
    }

    /// Withdraw a permission request without selecting any option.
    pub fn cancel_permission(&self, request_id: u64) -> Result<()> {
        match self {
            Self::Acp(client) => client.cancel_permission(request_id),
            Self::Claude(client) => client.cancel_permission(request_id),
        }
    }

    /// Stop the agent and wait for the subprocess worker to finish.
    pub fn shutdown(&mut self) -> Result<()> {
        match self {
            Self::Acp(client) => client.shutdown(),
            Self::Claude(client) => client.shutdown(),
        }
    }

    /// Whether this transport can restore files to an earlier turn.
    #[must_use]
    pub fn supports_rewind(&self) -> bool {
        matches!(self, Self::Claude(_))
    }

    /// [`Self::supports_rewind`] answered from the spec alone, for a
    /// surface deciding whether to draw an affordance before it connects.
    #[must_use]
    pub fn supports_rewind_for(launch: &LaunchSpec) -> bool {
        launch.is_native_claude()
    }

    /// Asks the agent to restore tracked files to their state at a turn.
    /// Only the native transport speaks this request; anything else is
    /// refused rather than silently dropped.
    pub fn rewind_files(
        &self,
        user_message_id: &str,
        dry_run: bool,
    ) -> Result<sirio_claude::RewindOutcome> {
        match self {
            Self::Claude(client) => client.rewind_files(user_message_id, dry_run),
            Self::Acp(_) => Err(anyhow!("this agent cannot restore files")),
        }
    }

    /// The id of the last turn this client sent, which a rewind names.
    /// `None` until the transport has one, and always `None` on ACP, which
    /// cannot rewind.
    #[must_use]
    pub fn last_user_message_id(&self) -> Option<String> {
        match self {
            Self::Claude(client) => client.last_user_message_id(),
            Self::Acp(_) => None,
        }
    }

    /// Whether the launch's resume id was refused and the session is fresh
    /// instead. The ACP transport has no such notion and says `false`.
    #[must_use]
    pub fn resumed_session_refused(&self) -> bool {
        match self {
            Self::Claude(client) => client.resumed_session_refused(),
            Self::Acp(_) => false,
        }
    }

    /// Whether this transport reports its connectors and the context
    /// breakdown behind them.
    #[must_use]
    pub fn supports_connectors(&self) -> bool {
        matches!(self, Self::Claude(_))
    }

    /// The CLI's breakdown of what fills the window. ACP agents have none.
    #[must_use]
    pub fn context_breakdown(&self) -> Option<sirio_claude::ContextBreakdown> {
        match self {
            Self::Claude(client) => client.context_breakdown(),
            Self::Acp(_) => None,
        }
    }

    /// Asks for the breakdown without a turn; the answer arrives as an
    /// `OtherSessionUpdate` named `ContextBreakdown`. ACP agents have nothing
    /// to ask, which is not a failure.
    pub fn refresh_context(&self) -> Result<()> {
        match self {
            Self::Claude(client) => client.refresh_context(),
            Self::Acp(_) => Ok(()),
        }
    }

    /// The connectors the session last reported.
    #[must_use]
    pub fn mcp_servers(&self) -> Option<Vec<sirio_claude::McpServer>> {
        match self {
            Self::Claude(client) => client.mcp_servers(),
            Self::Acp(_) => None,
        }
    }

    /// The connector calls as a handle that can leave the GPUI thread, which
    /// the client cannot: it is not `Clone` and the calls block. ACP agents
    /// have none.
    #[must_use]
    pub fn mcp_control(&self) -> Option<McpControl> {
        match self {
            Self::Claude(client) => Some(client.mcp_control()),
            Self::Acp(_) => None,
        }
    }

    /// Asks the agent for each connector's state, blocking for up to 10 s.
    pub fn refresh_mcp_status(&self) -> Result<Vec<sirio_claude::McpServer>> {
        match self {
            Self::Claude(client) => client.refresh_mcp_status(),
            Self::Acp(_) => Err(anyhow!("this agent does not report its connectors")),
        }
    }

    /// Asks the agent to connect a server again, blocking for up to 30 s.
    pub fn mcp_reconnect(&self, server: &str) -> Result<()> {
        match self {
            Self::Claude(client) => client.mcp_reconnect(server),
            Self::Acp(_) => Err(anyhow!("this agent does not report its connectors")),
        }
    }

    /// Starts a connector's sign-in, blocking for up to 30 s.
    pub fn mcp_authenticate(&self, server: &str) -> Result<sirio_claude::McpAuthStart> {
        match self {
            Self::Claude(client) => client.mcp_authenticate(server),
            Self::Acp(_) => Err(anyhow!("this agent does not report its connectors")),
        }
    }
}

/// What a session says about fast mode, and the context breakdown folded by
/// origin. Re-exported because `sirio_ui` reads them and does not depend on
/// `sirio_claude`.
pub use sirio_claude::{
    ContextBreakdown, ContextOrigins, FastMode, McpAuthStart, McpOrigin, McpServer, McpState,
    OriginNode, Standing, THINKING_DISPLAYS,
};
