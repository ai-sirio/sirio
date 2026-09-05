//! Real agent usage data for the status bar, ported from the Swift app's
//! `UsageStore` / `ProviderUsage` / the per-provider fetchers.
//!
//! The Swift app tracks four providers, all implemented here:
//!
//! - **Claude** reads local state only: it drives a hidden `claude` PTY and
//!   parses the `/usage` panel ([`claude`]).
//! - **Codex** reads its OAuth credentials from `~/.codex/auth.json` (or
//!   `$CODEX_HOME/auth.json` — the file `codex` itself writes) and calls
//!   `chatgpt.com/backend-api/wham/usage`, refreshing the access token
//!   through OpenAI's token endpoint when it is rejected ([`codex`]).
//! - **OpenCode Go** reads its session cookie from this platform's local
//!   store — the app's own [`CredentialStore`] here, the `com.sirio.usage`
//!   Keychain item on macOS — and scrapes its usage page on opencode.ai
//!   ([`opencode_go`]).
//! - **Ollama Cloud** reads its session cookie from the same store and
//!   scrapes ollama.com's settings page, best-effort by design ([`ollama`]).
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
pub use claude::{
    ClaudeUsageFetcher, classify_failure, claude_config_dir, claude_has_credentials_at,
    parse_claude_usage,
};
pub use codex::{
    CodexOAuthCredentials, CodexUsageFetcher, CredentialLoadError, TokenRefreshFailure,
    classify_token_refresh_failure, codex_auth_file_path, codex_has_credentials_at,
    load_codex_credentials,
};
pub use credentials::{CredentialStore, CredentialStoreError};
pub use model::{
    ProviderUsage, ProviderUsageState, UsageFetchOutcome, UsageReason, UsageWindow, reduce,
    reset_countdown,
};
pub use ollama::{OllamaCloudUsageFetcher, extract_ollama_cloud_usage, parse_ollama_cloud_usage};
pub use opencode_go::OpenCodeGoUsageFetcher;

/// The user's home directory, or `None` when the environment offers no
/// source. Windows has no `HOME`: a native GUI launch (Explorer, the start
/// menu) never sets it — git-bash does, which is exactly why this only
/// breaks outside a terminal — so `USERPROFILE` is the primary source,
/// with `HOMEDRIVE`+`HOMEPATH` as the fallback. Unix keeps reading
/// `HOME` unchanged. Callers must degrade on `None` (report "no
/// credentials", fall back to a sane path), never panic.
///
/// Duplicated deliberately across the leaf crates that need it —
/// `sirio_usage`, `sirio_project/src/domain.rs`, and `sirio/src/main.rs`
/// (+ its environment-map variant in `sirio/src/session.rs`) — because
/// none of them may gain a dependency on a shared helper crate; each
/// keeps this ~10-line private copy instead.
pub(crate) fn user_home_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERPROFILE")
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
            .or_else(|| {
                let drive = std::env::var_os("HOMEDRIVE").filter(|value| !value.is_empty())?;
                let path = std::env::var_os("HOMEPATH").filter(|value| !value.is_empty())?;
                Some(std::path::PathBuf::from(drive).join(path))
            })
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
    }
}

/// Working directory for helper processes used by usage probes. It must not
/// inherit the app's launch directory: LaunchServices commonly starts Sirio
/// in `/`, and a CLI launched there may inspect the whole machine. The
/// temporary directory is outside the user's protected home folders.
pub(crate) fn probe_working_directory() -> std::path::PathBuf {
    std::env::temp_dir()
}

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
