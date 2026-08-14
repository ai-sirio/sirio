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
//! - **OpenCode Go** reads its session cookie from this platform's local
//!   store — the app's own [`CredentialStore`] here, the `com.tiller.usage`
//!   Keychain item on macOS — and scrapes its usage page on opencode.ai
//!   ([`opencode_go`]).
//!
//! Ollama Cloud is not implemented: it needs a session cookie whose store
//! integration has not been built yet, and no local state exists to read
//! instead.
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

mod account;
mod claude;
mod codex;
mod credentials;
mod http;
mod model;
mod ollama;
mod opencode_go;

pub use account::{AgentAccountIdentity, LocalAccountState, parse_codex_identity};
pub use credentials::{CredentialStore, CredentialStoreError};
pub use claude::{
    ClaudeUsageFetcher, classify_failure, claude_config_dir, claude_has_credentials_at,
    parse_claude_usage,
};
pub use codex::{
    CodexOAuthCredentials, CodexUsageFetcher, CredentialLoadError, TokenRefreshFailure,
    classify_token_refresh_failure, codex_auth_file_path, codex_has_credentials_at,
    load_codex_credentials,
};
pub use model::{
    ProviderUsage, ProviderUsageState, UsageFetchOutcome, UsageReason, UsageWindow, reduce,
};
pub use ollama::parse_ollama_cloud_usage;
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
    pub const ALL: [Self; 4] = [
        Self::Claude,
        Self::Codex,
        Self::OpenCodeGo,
        Self::OllamaCloud,
    ];

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

    /// Stable preference key controlling whether this provider is shown and
    /// refreshed. Missing preferences default to enabled for compatibility.
    pub fn preference_key(self) -> &'static str {
        match self {
            Self::Claude => "usage.claude.enabled",
            Self::Codex => "usage.codex.enabled",
            Self::OpenCodeGo => "usage.opencodeGo.enabled",
            Self::OllamaCloud => "usage.ollamaCloud.enabled",
        }
    }

    pub fn is_enabled(self, preferences: &std::collections::HashMap<String, bool>) -> bool {
        preferences
            .get(self.preference_key())
            .copied()
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_catalog_and_preferences_are_stable() {
        assert_eq!(UsageProvider::ALL.len(), 4);
        let mut preferences = std::collections::HashMap::new();
        assert!(UsageProvider::Claude.is_enabled(&preferences));
        preferences.insert(UsageProvider::Claude.preference_key().to_string(), false);
        assert!(!UsageProvider::Claude.is_enabled(&preferences));
        assert_eq!(UsageProvider::OpenCodeGo.id(), "opencode");
    }
}
