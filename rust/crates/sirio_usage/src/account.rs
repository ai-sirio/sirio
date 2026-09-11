//! Local account state: whether a provider has credentials on this
//! machine, read synchronously from disk.
//!
//! This answers the settings surface's "is this provider signed in"
//! question with what the program can actually check — the same files the
//! CLIs themselves write (`~/.codex/auth.json`, the Claude config
//! directory, the macOS Keychain cookie). It never validates a token
//! (that requires the bounded network/PTY fetch the usage bar runs) and
//! never blocks the render thread on anything but a local file read.
//!
//! The honest rule: **presence, not validity**. A credential file existing
//! means the user signed in at some point; nothing here proves the token
//! still works. The settings surface labels this "Signed in" — weaker than
//! "Active" (which would claim the API accepted the credential), stronger
//! than "unknown" (which would throw away real state).

use std::path::PathBuf;

use serde_json::Value;

use crate::UsageProvider;
use crate::claude::claude_has_credentials_at;
use crate::codex::codex_has_credentials_at;
use crate::user_home_dir;

/// Whether a provider has local credentials on this machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalAccountState {
    /// The provider stores usable credentials on this machine.
    SignedIn,
    /// The provider stores no credentials here.
    SignedOut,
    /// This platform has no local store for this provider's credentials.
    /// The state is *unknown* — saying "signed out" would be a guess about
    /// a store that does not exist. No provider constructs this today
    /// (every cookie provider now reads [`crate::CredentialStore`] here
    /// and the macOS Keychain there — F-SET-12/F-SET-13); it stays as the
    /// honest answer for any future provider whose store is not built.
    NoLocalStore,
}

/// The identity fields shown for a locally authenticated agent account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAccountIdentity {
    pub logged_in: bool,
    pub email: String,
    pub organization: Option<String>,
}

impl AgentAccountIdentity {
    /// Parses Claude's account JSON without treating malformed input as a
    /// logged-out claim.
    pub fn parse_claude_json(raw: &str) -> Option<Self> {
        let json: Value = serde_json::from_str(raw).ok()?;
        let account = json.get("account").unwrap_or(&json);
        let email = first_string(account, &["email", "email_address"])?;
        let organization = first_string(
            account,
            &["organization", "organizationName", "organization_name"],
        );
        Some(Self {
            logged_in: true,
            email,
            organization,
        })
    }
}

/// Returns the first nonempty credential-status line emitted by Codex.
pub fn parse_codex_identity(raw: &str) -> Option<String> {
    raw.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(str::to_string)
}

impl LocalAccountState {
    /// The label shown in the settings surface. "Unknown" is the honest
    /// answer when there is no local state to read.
    pub fn label(self) -> &'static str {
        match self {
            LocalAccountState::SignedIn => "Signed in",
            LocalAccountState::SignedOut => "Not signed in",
            LocalAccountState::NoLocalStore => "Unknown",
        }
    }
}

impl UsageProvider {
    /// The provider's local credential state, read synchronously from disk.
    /// Never a network call, never a PTY: the settings surface runs this
    /// wherever it constructs its model.
    pub fn local_account_state(self) -> LocalAccountState {
        match self {
            UsageProvider::Claude => {
                if claude_has_credentials_at(&claude_credentials_file())
                    || claude_has_keychain_credentials()
                {
                    LocalAccountState::SignedIn
                } else {
                    LocalAccountState::SignedOut
                }
            }
            UsageProvider::Codex => {
                if codex_has_credentials_at(&codex_auth_file()) {
                    LocalAccountState::SignedIn
                } else {
                    LocalAccountState::SignedOut
                }
            }
            UsageProvider::OpenCodeGo => opencode_go_account_state(),
            // F-SET-13: same store, same rule as OpenCode Go — the
            // cookie's presence is the account state.
            UsageProvider::OllamaCloud => {
                state_from_cookie_presence(crate::ollama::ollama_cloud_local_cookie().is_some())
            }
        }
    }
}

/// Whether macOS's Keychain carries the primary Claude session — see
/// [`crate::claude::claude_has_keychain_credentials`]. `.credentials.json`
/// alone is not the full story on macOS: the current `claude` CLI keeps its
/// OAuth session in the Keychain, not the file.
#[cfg(target_os = "macos")]
fn claude_has_keychain_credentials() -> bool {
    crate::claude::claude_has_keychain_credentials()
}

#[cfg(not(target_os = "macos"))]
fn claude_has_keychain_credentials() -> bool {
    false
}

/// The Codex auth file: `$CODEX_HOME/auth.json`, else
/// `~/.codex/auth.json` — the same precedence `codex` itself uses.
fn codex_auth_file() -> PathBuf {
    if let Some(home) = std::env::var_os("CODEX_HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home).join("auth.json");
    }
    user_home_dir()
        .map(|home| home.join(".codex/auth.json"))
        // No home → an empty path — the subsequent file read fails, and
        // every caller treats that as "no credentials". Never a panic.
        .unwrap_or_default()
}

/// The Claude credentials file: `$CLAUDE_CONFIG_DIR/.credentials.json`,
/// else `~/.claude/.credentials.json` — the config directory `claude`
/// itself uses.
fn claude_credentials_file() -> PathBuf {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR")
        && !dir.is_empty()
    {
        return PathBuf::from(dir).join(".credentials.json");
    }
    user_home_dir()
        .map(|home| home.join(".claude/.credentials.json"))
        // Degrade to "no credentials" (empty path → failed read) instead
        // of panicking, as before this helper existed.
        .unwrap_or_default()
}

fn opencode_go_account_state() -> LocalAccountState {
    // macOS reads the Keychain item the Swift app writes; this platform
    // reads the app's own credential store, which the settings surface
    // saves into (F-SET-12). Either way the cookie's presence is the
    // account state — presence, not validity.
    state_from_cookie_presence(crate::opencode_go::opencode_go_local_cookie().is_some())
}

/// The presence→state mapping shared by the platform cookie sources, split
/// out so tests can drive it from a fixture [`crate::CredentialStore`]
/// without touching the process environment.
fn state_from_cookie_presence(present: bool) -> LocalAccountState {
    if present {
        LocalAccountState::SignedIn
    } else {
        LocalAccountState::SignedOut
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_file(name: &str, contents: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("sirio-account-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("credentials");
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn labels_are_honest_words() {
        assert_eq!(LocalAccountState::SignedIn.label(), "Signed in");
        assert_eq!(LocalAccountState::SignedOut.label(), "Not signed in");
        assert_eq!(LocalAccountState::NoLocalStore.label(), "Unknown");
    }

    #[test]
    fn codex_credentials_presence_reads_the_auth_file() {
        let path = temp_file(
            "codex-valid",
            r#"{"auth_mode":"oauth","tokens":{"access_token":"a","refresh_token":"r"}}"#,
        );
        assert!(codex_has_credentials_at(&path));
        let _ = std::fs::remove_file(&path);

        let path = temp_file("codex-missing-tokens", r#"{"auth_mode":"oauth"}"#);
        assert!(
            !codex_has_credentials_at(&path),
            "an auth file without tokens is not a signed-in state"
        );
        let _ = std::fs::remove_file(&path);

        let path = temp_file("codex-garbage", "not json");
        assert!(!codex_has_credentials_at(&path));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn claude_credentials_presence_reads_oauth_or_api_key() {
        // OAuth login: `claudeAiOauth` with an access token.
        let path = temp_file(
            "claude-oauth",
            r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-x","refreshToken":"r"}}"#,
        );
        assert!(claude_has_credentials_at(&path));
        let _ = std::fs::remove_file(&path);

        // API-key login: `hashedToken`.
        let path = temp_file("claude-key", r#"{"hashedToken":"abc"}"#);
        assert!(claude_has_credentials_at(&path));
        let _ = std::fs::remove_file(&path);

        // An empty credentials file is not a signed-in state.
        let path = temp_file("claude-empty", r#"{}"#);
        assert!(!claude_has_credentials_at(&path));
        let _ = std::fs::remove_file(&path);

        let path = temp_file("claude-garbage", "not json");
        assert!(!claude_has_credentials_at(&path));
        let _ = std::fs::remove_file(&path);
    }

    /// F-SET-13: Ollama Cloud's account state is the cookie's presence in
    /// the app's own credential store — no longer `NoLocalStore`, because
    /// the store now exists. Driven through a fixture store like the
    /// OpenCode Go test below: the environment-reading path is one
    /// `is_some()` away from this mapping.
    #[test]
    fn ollama_cloud_state_is_cookie_presence_in_the_credential_store() {
        let path = std::env::temp_dir().join(format!(
            "sirio-account-ollama-cookie-{}/credentials.json",
            std::process::id()
        ));
        let store = crate::CredentialStore::at(&path);

        let key = crate::OllamaCloudUsageFetcher::COOKIE_KEY;
        assert_eq!(
            state_from_cookie_presence(store.get(key).is_some()),
            LocalAccountState::SignedOut,
            "no cookie stored → signed out, an answer — not Unknown"
        );

        store.set(key, "session=abc").expect("save cookie");
        assert_eq!(
            state_from_cookie_presence(store.get(key).is_some()),
            LocalAccountState::SignedIn
        );

        store.delete(key).expect("clear cookie");
        assert_eq!(
            state_from_cookie_presence(store.get(key).is_some()),
            LocalAccountState::SignedOut,
            "clearing the cookie signs the provider out"
        );

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// F-SET-12: the OpenCode Go account state on this platform is the
    /// cookie's presence in the app's own credential store — no longer
    /// `NoLocalStore`, because the store now exists. Driven through a
    /// fixture store: the environment-reading path is one `is_some()` away
    /// from this mapping.
    #[test]
    fn opencode_go_state_is_cookie_presence_in_the_credential_store() {
        let path = std::env::temp_dir().join(format!(
            "sirio-account-cookie-{}/credentials.json",
            std::process::id()
        ));
        let store = crate::CredentialStore::at(&path);

        let key = crate::OpenCodeGoUsageFetcher::COOKIE_KEY;
        assert_eq!(
            state_from_cookie_presence(store.get(key).is_some()),
            LocalAccountState::SignedOut,
            "no cookie stored → signed out, an answer — not Unknown"
        );

        store.set(key, "auth=Fe26.2**abc").expect("save cookie");
        assert_eq!(
            state_from_cookie_presence(store.get(key).is_some()),
            LocalAccountState::SignedIn
        );

        store.delete(key).expect("clear cookie");
        assert_eq!(
            state_from_cookie_presence(store.get(key).is_some()),
            LocalAccountState::SignedOut,
            "clearing the cookie signs the provider out"
        );

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn parses_account_identity_without_inventing_logged_in_state() {
        assert_eq!(
            AgentAccountIdentity::parse_claude_json(
                r#"{"account":{"email":"user@example.com","organizationName":"Acme"}}"#
            ),
            Some(AgentAccountIdentity {
                logged_in: true,
                email: "user@example.com".into(),
                organization: Some("Acme".into())
            })
        );
        assert_eq!(AgentAccountIdentity::parse_claude_json("{}"), None);
        assert_eq!(
            parse_codex_identity("\n Logged in using ChatGPT - user@example.com\n"),
            Some("Logged in using ChatGPT - user@example.com".into())
        );
        assert_eq!(parse_codex_identity("  \n"), None);
    }
}
