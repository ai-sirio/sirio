//! Real agent usage data for the status bar, ported from the Swift app's
//! `UsageStore` / `ProviderUsage` / the per-provider fetchers.
//!
//! The Swift app tracks four providers: Claude, Codex, OpenCode Go and
//! Ollama Cloud. Three are implemented here:
//!
//! - **Claude** reads local state only: it drives a hidden `claude` PTY and
//!   parses the `/usage` panel ([`claude`]).
//! - **Codex** reads its OAuth credentials from `~/.codex/auth.json` (or
//!   `$CODEX_HOME/auth.json` — the file `codex` itself writes) and calls
//!   `chatgpt.com/backend-api/wham/usage`, refreshing the access token
//!   through OpenAI's token endpoint when it is rejected ([`codex`]).
//! - **OpenCode Go** reads its session cookie from the macOS Keychain (the
//!   `com.tiller.usage` item the Swift app writes) and scrapes its usage
//!   page on opencode.ai ([`opencode_go`]).
//!
//! Ollama Cloud is not implemented: it needs a session cookie this app has
//! no store for, and no local state exists to read instead.
//!
//! # Contract
//!
//! - A provider that is missing, unreadable, malformed, or whose API
//!   refuses yields [`UsageFetchOutcome::Unavailable`] — never a panic,
//!   never a blank bar, and **never a plausible-looking number nobody
//!   computed**.
//! - Every fetch is bounded (the Claude PTY session is killed after
//!   [`ClaudeUsageFetcher::TIMEOUT`]; the network calls use curl's
//!   `--max-time`). All fetches are blocking calls and must run off the
//!   render thread (the status bar runs them on GPUI's background
//!   executor).
//! - Stale data is visible: a timed-out refresh keeps the last good value
//!   as [`ProviderUsageState::Stale`] (the bar renders it dimmed) instead
//!   of showing an old number as current.

mod claude;
mod codex;
mod http;
mod model;
mod opencode_go;

pub use claude::{ClaudeUsageFetcher, classify_failure, parse_claude_usage};
pub use codex::CodexUsageFetcher;
pub use model::{
    ProviderUsage, ProviderUsageState, UsageFetchOutcome, UsageReason, UsageWindow, reduce,
};
pub use opencode_go::OpenCodeGoUsageFetcher;

/// The provider identity, matching the Swift `UsageProvider` ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UsageProvider {
    Claude,
    Codex,
    OpenCodeGo,
    OllamaCloud,
}

impl UsageProvider {
    /// The stable id used by the rest of the app (agent catalog id).
    pub fn id(self) -> &'static str {
        match self {
            UsageProvider::Claude => "claude",
            UsageProvider::Codex => "codex",
            UsageProvider::OpenCodeGo => "opencode",
            UsageProvider::OllamaCloud => "ollama",
        }
    }

    /// Whether this provider can be read from local state alone. The three
    /// network providers are listed so callers can decide what to show.
    pub fn reads_local_state(self) -> bool {
        matches!(self, UsageProvider::Claude)
    }
}
