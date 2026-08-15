//! The Codex provider: OAuth credentials from `~/.codex/auth.json` (or
//! `$CODEX_HOME/auth.json`) and the ChatGPT rate-limit API, ported from
//! `TillerCore/CodexUsageFetcher.swift`.
//!
//! Codex keeps no local usage state; its numbers live behind
//! `chatgpt.com/backend-api/wham/usage`, authenticated with the same
//! OAuth tokens `codex` itself stores. The flow mirrors the Swift app:
//! load the credentials, call the API with a bounded timeout, and if the
//! access token is rejected (401), refresh it through OpenAI's token
//! endpoint — writing the refreshed tokens back into the user's auth file,
//! merged so `codex`'s own fields survive — and retry once. No credentials
//! or no account → [`UsageReason::LoggedOut`]; anything else that refuses
//! → [`UsageReason::Error`]; a fetch that outlives its budget → `TimedOut`,
//! which the reducer keeps as visibly stale.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::Value;

use crate::http::{HttpError, get, post};
use crate::model::{ProviderUsage, UsageFetchOutcome, UsageReason, UsageWindow};

/// The ChatGPT rate-limit endpoint.
const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
/// OpenAI's OAuth token endpoint, used to refresh an expired access token.
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
/// The OAuth client id `codex` itself uses.
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
/// How long a fetch may take before giving up (the Swift app's bound).
pub const TIMEOUT: Duration = Duration::from_secs(15);

/// The subset of `~/.codex/auth.json` this fetcher needs.
#[derive(Clone)]
pub struct CodexOAuthCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub account_id: Option<String>,
    pub last_refresh: Option<SystemTime>,
}

#[derive(Debug)]
pub enum CredentialLoadError {
    Io(std::io::Error),
    Json(serde_json::Error),
    MissingTokens,
}

impl CodexOAuthCredentials {
    pub const REFRESH_AFTER: Duration = Duration::from_secs(8 * 24 * 60 * 60);

    pub fn needs_refresh(&self, now: SystemTime) -> bool {
        self.last_refresh
            .and_then(|last| now.duration_since(last).ok())
            .is_none_or(|age| age >= Self::REFRESH_AFTER)
    }
}

type CodexCredentials = CodexOAuthCredentials;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenRefreshFailure {
    Reused,
    Revoked,
    Expired,
    Other,
}

/// Maps a classified token-refresh failure (F-CORE-USG-06) onto the
/// `UsageReason` the status bar renders distinct copy for. `Other` still
/// collapses to the generic `LoggedOut`, matching the pre-existing
/// behaviour for failures that aren't a clearly reused/revoked/expired
/// refresh token (e.g. a network error, or a malformed response).
pub fn token_refresh_failure_reason(failure: TokenRefreshFailure) -> UsageReason {
    match failure {
        TokenRefreshFailure::Reused => UsageReason::TokenReused,
        TokenRefreshFailure::Revoked => UsageReason::TokenRevoked,
        TokenRefreshFailure::Expired => UsageReason::TokenExpired,
        TokenRefreshFailure::Other => UsageReason::LoggedOut,
    }
}

pub fn classify_token_refresh_failure(status: i32, body: &str) -> TokenRefreshFailure {
    let lower = body.to_ascii_lowercase();
    if lower.contains("reuse") {
        TokenRefreshFailure::Reused
    } else if lower.contains("revok") {
        TokenRefreshFailure::Revoked
    } else if lower.contains("expir") || (status == 401 && lower.contains("invalid_grant")) {
        TokenRefreshFailure::Expired
    } else {
        TokenRefreshFailure::Other
    }
}

/// `~/.codex/auth.json`, or `$CODEX_HOME/auth.json` when set — the same
/// precedence `codex` itself uses.
pub fn codex_auth_file_path() -> PathBuf {
    if let Some(home) = std::env::var_os("CODEX_HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home).join("auth.json");
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".codex/auth.json"))
        .expect("HOME must be set")
}

/// Whether a Codex auth file carries usable OAuth tokens — the same parse
/// [`load_credentials_from`] performs. **Presence, not validity**: the
/// tokens may have expired (the usage fetch answers that); this answers
/// only "has the user signed in".
pub fn codex_has_credentials_at(path: &std::path::Path) -> bool {
    load_credentials_from(path).is_ok()
}

fn auth_file_path() -> PathBuf {
    codex_auth_file_path()
}

fn load_credentials() -> Result<CodexCredentials, CredentialLoadError> {
    load_credentials_from(&auth_file_path())
}

pub fn load_codex_credentials(path: &Path) -> Result<CodexOAuthCredentials, CredentialLoadError> {
    let data = std::fs::read(path).map_err(CredentialLoadError::Io)?;
    let json: Value = serde_json::from_slice(&data).map_err(CredentialLoadError::Json)?;
    let tokens = json
        .get("tokens")
        .and_then(Value::as_object)
        .ok_or(CredentialLoadError::MissingTokens)?;
    let access_token = tokens
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or(CredentialLoadError::MissingTokens)?;
    let refresh_token = tokens
        .get("refresh_token")
        .and_then(Value::as_str)
        .ok_or(CredentialLoadError::MissingTokens)?;
    Ok(CodexCredentials {
        access_token: access_token.to_string(),
        refresh_token: refresh_token.to_string(),
        account_id: tokens
            .get("account_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        last_refresh: json
            .get("last_refresh")
            .and_then(Value::as_str)
            .and_then(parse_system_time),
    })
}

fn load_credentials_from(path: &Path) -> Result<CodexCredentials, CredentialLoadError> {
    load_codex_credentials(path)
}

fn parse_system_time(value: &str) -> Option<SystemTime> {
    let parsed = chrono::DateTime::parse_from_rfc3339(value).ok()?;
    let seconds = parsed.timestamp();
    (seconds >= 0)
        .then_some(seconds as u64)
        .and_then(|seconds| SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds)))
}

/// Merges refreshed tokens into the auth file rather than overwriting it —
/// `codex` itself may store other fields Tiller doesn't know about.
fn save_credentials(credentials: &CodexCredentials) -> Result<(), ()> {
    save_credentials_to(&auth_file_path(), credentials)
}

fn save_credentials_to(path: &std::path::Path, credentials: &CodexCredentials) -> Result<(), ()> {
    let mut json: Value = std::fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_else(|| Value::Object(Default::default()));
    let mut tokens = json
        .get("tokens")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    tokens.insert(
        "access_token".into(),
        Value::String(credentials.access_token.clone()),
    );
    tokens.insert(
        "refresh_token".into(),
        Value::String(credentials.refresh_token.clone()),
    );
    if let Some(account_id) = &credentials.account_id {
        tokens.insert("account_id".into(), Value::String(account_id.clone()));
    }
    json.as_object_mut()
        .ok_or(())?
        .insert("tokens".into(), Value::Object(tokens));
    json.as_object_mut()
        .ok_or(())?
        .insert("last_refresh".into(), Value::String(now_iso8601()));
    let data = serde_json::to_vec_pretty(&json).map_err(|_| ())?;
    std::fs::write(path, data).map_err(|_| ())
}

fn now_iso8601() -> String {
    // ISO8601 with millisecond precision and a Z suffix, matching what the
    // Swift app writes for `last_refresh`.
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
/// Calls the wham API with `access_token`; 401/403 means the token is no
/// longer accepted.
fn fetch_usage(
    access_token: &str,
    account_id: Option<&str>,
    timeout: Duration,
) -> Result<ProviderUsage, CodexApiFailure> {
    let mut headers = vec![
        ("Authorization", format!("Bearer {access_token}")),
        ("Accept", "application/json".to_string()),
        ("User-Agent", "Tiller".to_string()),
    ];
    if let Some(account_id) = account_id {
        headers.push(("ChatGPT-Account-Id", account_id.to_string()));
    }
    let header_refs: Vec<(&str, &str)> = headers
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let response = match get(USAGE_URL, &header_refs, timeout.as_secs()) {
        Ok(response) => response,
        Err(HttpError::TimedOut) => return Err(CodexApiFailure::TimedOut),
        Err(HttpError::Network(error)) => {
            eprintln!("[codex-usage] request failed: {error}");
            return Err(CodexApiFailure::Error);
        }
    };
    match response.code {
        401 | 403 => Err(CodexApiFailure::Unauthorized),
        200..=299 => match parse_usage(&response.body) {
            Some(usage) => Ok(usage),
            None => Err(CodexApiFailure::Error),
        },
        _ => Err(CodexApiFailure::Error),
    }
}

enum CodexApiFailure {
    Unauthorized,
    TimedOut,
    Error,
}

/// Refreshes the credentials via OpenAI's token endpoint; the response's
/// refresh token (when present) replaces the stored one, mirroring the
/// Swift's `CodexTokenRefresher`.
fn refresh_token(credentials: &CodexCredentials) -> Result<CodexCredentials, TokenRefreshFailure> {
    refresh_token_at(TOKEN_URL, credentials)
}

/// [`refresh_token`], against an explicit endpoint rather than the hardcoded
/// [`TOKEN_URL`] constant — split out so `F-CORE-USG-05`'s tests can drive a
/// real refresh round trip (the actual HTTP response parsing and
/// merge-save-on-success path, codex.rs:154-172) against a local fixture
/// server instead of `auth.openai.com`.
fn refresh_token_at(
    url: &str,
    credentials: &CodexCredentials,
) -> Result<CodexCredentials, TokenRefreshFailure> {
    let body = format!(
        r#"{{"client_id":"{CLIENT_ID}","grant_type":"refresh_token","refresh_token":"{}","scope":"openid profile email"}}"#,
        credentials.refresh_token
    );
    let response = post(
        url,
        &[("Content-Type", "application/json")],
        &body,
        TIMEOUT.as_secs(),
    )
    .map_err(|_| TokenRefreshFailure::Other)?;
    if response.code != 200 {
        return Err(classify_token_refresh_failure(
            response.code,
            &response.body,
        ));
    }
    let json: Value =
        serde_json::from_str(&response.body).map_err(|_| TokenRefreshFailure::Other)?;
    let access_token = json
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or(TokenRefreshFailure::Other)?;
    let refresh_token = json
        .get("refresh_token")
        .and_then(Value::as_str)
        .unwrap_or(&credentials.refresh_token);
    Ok(CodexCredentials {
        access_token: access_token.to_string(),
        refresh_token: refresh_token.to_string(),
        account_id: credentials.account_id.clone(),
        last_refresh: Some(SystemTime::now()),
    })
}

/// Parses the wham API response: `rate_limit.primary_window` is the 5-hour
/// session window, `secondary_window` the weekly one. `None` when no window
/// could be read.
fn parse_usage(body: &str) -> Option<ProviderUsage> {
    let json: Value = serde_json::from_str(body).ok()?;
    let rate_limit = json.get("rate_limit")?;
    let session = window(rate_limit.get("primary_window")?, "5h");
    let weekly = rate_limit
        .get("secondary_window")
        .and_then(Value::as_object)
        .and_then(|_| window(rate_limit.get("secondary_window")?, "wk"));
    let usage = ProviderUsage {
        session,
        weekly,
        monthly: None,
        fable_weekly: None,
    };
    usage.has_any().then_some(usage)
}

fn window(raw: &Value, label: &str) -> Option<UsageWindow> {
    let used_percent = raw.get("used_percent")?.as_u64()? as u8;
    let resets_at = raw
        .get("reset_at")
        .and_then(Value::as_u64)
        .and_then(|epoch| {
            std::time::SystemTime::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(epoch))
        });
    Some(UsageWindow {
        label: label.to_string(),
        used_percent: used_percent.min(100),
        resets_at,
    })
}

/// The Codex usage fetcher. **Blocking** — bounded by [`TIMEOUT`]; call it
/// off the render thread.
pub struct CodexUsageFetcher;

impl CodexUsageFetcher {
    pub fn fetch() -> UsageFetchOutcome {
        let credentials = match load_credentials() {
            Ok(credentials) => credentials,
            Err(_) => return UsageFetchOutcome::Unavailable(UsageReason::LoggedOut),
        };

        // F-CORE-USG-05: `needs_refresh`'s 8-day gate (`REFRESH_AFTER`) had
        // zero callers before this — the token only ever refreshed
        // reactively, after the API had already rejected it with a 401.
        // Refresh ahead of that once the on-disk token is old enough. A
        // failed *proactive* refresh is not fatal here: this call is
        // optimistic (the token may still be perfectly good, or the
        // network hiccuped), so fall back to the on-disk token and let the
        // reactive 401 path below make the real LoggedOut determination.
        let credentials = if credentials.needs_refresh(SystemTime::now()) {
            match refresh_token(&credentials) {
                Ok(refreshed) => {
                    let _ = save_credentials(&refreshed);
                    refreshed
                }
                Err(failure) => {
                    eprintln!("[codex-usage] proactive token refresh failed: {failure:?}");
                    credentials
                }
            }
        } else {
            credentials
        };

        match fetch_usage(
            &credentials.access_token,
            credentials.account_id.as_deref(),
            TIMEOUT,
        ) {
            Ok(usage) => return UsageFetchOutcome::Success(usage),
            Err(CodexApiFailure::TimedOut) => return UsageFetchOutcome::TimedOut,
            Err(CodexApiFailure::Error) => {
                return UsageFetchOutcome::Unavailable(UsageReason::Error);
            }
            Err(CodexApiFailure::Unauthorized) => {}
        }

        // The access token was rejected: refresh once and retry.
        let refreshed = match refresh_token(&credentials) {
            Ok(refreshed) => refreshed,
            Err(failure) => {
                // F-CORE-USG-06: route `classify_token_refresh_failure`'s
                // classification (Reused/Revoked/Expired/Other) into a
                // `UsageReason` variant the status bar can render distinct
                // copy for, instead of collapsing every case to the same
                // generic `LoggedOut`.
                eprintln!("[codex-usage] token refresh failed: {failure:?}");
                return UsageFetchOutcome::Unavailable(token_refresh_failure_reason(failure));
            }
        };
        let _ = save_credentials(&refreshed);
        match fetch_usage(
            &refreshed.access_token,
            refreshed.account_id.as_deref(),
            TIMEOUT,
        ) {
            Ok(usage) => UsageFetchOutcome::Success(usage),
            Err(CodexApiFailure::TimedOut) => UsageFetchOutcome::TimedOut,
            Err(CodexApiFailure::Unauthorized) | Err(CodexApiFailure::Error) => {
                UsageFetchOutcome::Unavailable(UsageReason::LoggedOut)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// A one-shot local HTTP fixture (F-CORE-USG-05): binds an ephemeral
    /// loopback port, accepts exactly one connection, replies with
    /// `status_line`/`body`, and hands back the URL to POST to. Lets a test
    /// drive [`refresh_token_at`] through a real socket and a real curl
    /// child process — a genuine refresh round trip — without reaching
    /// `auth.openai.com`.
    fn one_shot_http_fixture(status_line: &'static str, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
        let addr = listener.local_addr().expect("listener has a local addr");
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                // The request is small (a single JSON POST body); one read
                // is enough to drain it before this fixture replies.
                let mut buf = [0u8; 8192];
                let _ = stream.read(&mut buf);
                let response = format!(
                    "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        format!("http://{addr}/oauth/token")
    }

    /// F-CORE-USG-05: drives a *real* successful refresh — a genuine HTTP
    /// response parsed by [`refresh_token_at`], not a synthetic
    /// `CodexCredentials` value — into [`save_credentials_to`]'s
    /// merge-not-overwrite logic (codex.rs:154-172), the path
    /// `CodexUsageFetcher::fetch` calls on a real refresh success. Confirms
    /// the auth file's fields Tiller doesn't know about survive the merge.
    #[test]
    fn a_real_refresh_success_merges_into_the_auth_file_without_losing_unrelated_fields() {
        let url = one_shot_http_fixture(
            "200 OK",
            r#"{"access_token":"fresh-access","refresh_token":"fresh-refresh"}"#,
        );
        let credentials = CodexCredentials {
            access_token: "stale".into(),
            refresh_token: "old-refresh".into(),
            account_id: Some("acct-1".into()),
            last_refresh: None,
        };
        let refreshed =
            refresh_token_at(&url, &credentials).expect("a 200 response is a successful refresh");
        assert_eq!(refreshed.access_token, "fresh-access");
        assert_eq!(refreshed.refresh_token, "fresh-refresh");
        assert_eq!(refreshed.account_id.as_deref(), Some("acct-1"));

        let dir = std::env::temp_dir().join(format!(
            "tiller-codex-real-refresh-merge-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("auth.json");
        std::fs::write(
            &path,
            r#"{"auth_mode":"oauth","tokens":{"access_token":"stale","refresh_token":"old-refresh","account_id":"acct-1","id_token":"opaque-jwt"},"custom_field":"kept","last_refresh":"stale-stamp"}"#,
        )
        .unwrap();
        save_credentials_to(&path, &refreshed).expect("saves the real refresh response");
        let json: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(json["tokens"]["access_token"], "fresh-access");
        assert_eq!(json["tokens"]["refresh_token"], "fresh-refresh");
        // Fields this fetcher never wrote — inside `tokens` and outside it —
        // survive the merge from a real refresh response, not just a
        // hand-built `CodexCredentials`.
        assert_eq!(json["tokens"]["id_token"], "opaque-jwt");
        assert_eq!(json["auth_mode"], "oauth");
        assert_eq!(json["custom_field"], "kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F-CORE-USG-06: the classification `refresh_token`'s caller currently
    /// discards (codex.rs:336-339) is real and reachable through a genuine
    /// non-200 HTTP response, not just a direct
    /// `classify_token_refresh_failure` call.
    #[test]
    fn a_real_401_response_is_classified_through_refresh_token_at() {
        let url = one_shot_http_fixture("401 Unauthorized", r#"{"error":"refresh token revoked"}"#);
        let credentials = CodexCredentials {
            access_token: "stale".into(),
            refresh_token: "old-refresh".into(),
            account_id: None,
            last_refresh: None,
        };
        match refresh_token_at(&url, &credentials) {
            Err(error) => assert_eq!(error, TokenRefreshFailure::Revoked),
            Ok(_) => panic!("a 401 response must not report a successful refresh"),
        }
    }

    /// The real wham API response captured from this machine on the review
    /// day (fields elided after the ones the parser reads).
    const REAL_RESPONSE: &str = r#"{
      "user_id": "user-nUTsVXW83ujiNqqzTzH5SzOR",
      "account_id": "c1020389-13f2-479f-be87-a4f89aae9f37",
      "email": "enzopalmisano.pvt@gmail.com",
      "plan_type": "plus",
      "rate_limit": {
        "allowed": true,
        "limit_reached": false,
        "primary_window": {
          "used_percent": 51,
          "limit_window_seconds": 604800,
          "reset_after_seconds": 508778,
          "reset_at": 1787038258
        },
        "secondary_window": null
      }
    }"#;

    #[test]
    fn parses_the_real_wham_response() {
        let usage = parse_usage(REAL_RESPONSE).expect("real response parses");
        assert_eq!(
            usage.session,
            Some(UsageWindow {
                label: "5h".into(),
                used_percent: 51,
                resets_at: Some(
                    std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1787038258)
                ),
            })
        );
        assert_eq!(
            usage.weekly, None,
            "secondary_window is null on this account"
        );
    }

    #[test]
    fn parses_a_secondary_window_when_present() {
        let body = r#"{"rate_limit":{"primary_window":{"used_percent":20},"secondary_window":{"used_percent":40,"reset_at":1800000000}}}"#;
        let usage = parse_usage(body).expect("parses");
        assert_eq!(usage.session.as_ref().map(|w| w.used_percent), Some(20));
        assert_eq!(
            usage
                .weekly
                .as_ref()
                .map(|w| (w.used_percent, w.label.as_str())),
            Some((40, "wk"))
        );
    }

    #[test]
    fn garbage_is_not_usage() {
        assert_eq!(parse_usage("not json at all"), None);
        assert_eq!(parse_usage(r#"{"rate_limit":{}}"#), None);
    }

    #[test]
    fn loads_credentials_from_a_temp_auth_file() {
        let dir = std::env::temp_dir().join(format!("tiller-codex-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("auth.json");
        std::fs::write(
            &path,
            r#"{"auth_mode":"oauth","tokens":{"access_token":"acc","refresh_token":"ref","account_id":"a1"},"last_refresh":"2026-08-08T11:06:24Z"}"#,
        )
        .unwrap();
        let credentials = load_credentials_from(&path).expect("loads");
        assert_eq!(credentials.access_token, "acc");
        assert_eq!(credentials.refresh_token, "ref");
        assert_eq!(credentials.account_id.as_deref(), Some("a1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_credentials_are_not_signed_in() {
        let dir = std::env::temp_dir().join(format!("tiller-codex-missing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load_credentials_from(&dir.join("auth.json")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_refreshed_tokens_merges_and_preserves_other_fields() {
        let dir = std::env::temp_dir().join(format!("tiller-codex-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("auth.json");
        std::fs::write(
            &path,
            r#"{"auth_mode":"oauth","tokens":{"access_token":"old","refresh_token":"ref","account_id":"a1"},"last_refresh":"x"}"#,
        )
        .unwrap();
        let refreshed = CodexCredentials {
            access_token: "new".into(),
            refresh_token: "new-ref".into(),
            account_id: Some("a1".into()),
            last_refresh: None,
        };
        save_credentials_to(&path, &refreshed).expect("saves");
        let credentials = load_credentials_from(&path).expect("reloads");
        assert_eq!(credentials.access_token, "new");
        assert_eq!(credentials.refresh_token, "new-ref");
        assert_eq!(credentials.account_id.as_deref(), Some("a1"));
        // The unrelated top-level field survived the merge.
        let json: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(json["auth_mode"], "oauth");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_writes_a_parseable_last_refresh_and_keeps_unknown_token_fields() {
        let dir = std::env::temp_dir().join(format!(
            "tiller-codex-last-refresh-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("auth.json");
        std::fs::write(
            &path,
            r#"{"tokens":{"access_token":"old","refresh_token":"old-r","id_token":"opaque-jwt"},"custom":"kept"}"#,
        )
        .unwrap();
        save_credentials_to(
            &path,
            &CodexCredentials {
                access_token: "new".into(),
                refresh_token: "new-r".into(),
                account_id: None,
                last_refresh: None,
            },
        )
        .expect("saves");
        let reloaded = load_credentials_from(&path).expect("reloads");
        assert_eq!(reloaded.access_token, "new");
        assert_eq!(reloaded.refresh_token, "new-r");
        // The merge-save stamps a fresh last-refresh time the loader can
        // parse back (the fixture had none, so any parsed value is the
        // stamp), and it is recent.
        let last_refresh = reloaded.last_refresh.expect("last_refresh is written");
        let age = SystemTime::now()
            .duration_since(last_refresh)
            .expect("the stamp is in the past");
        assert!(age < Duration::from_secs(60), "stale stamp: {age:?}");
        // Unknown fields inside and outside `tokens` survive the merge.
        let json: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(json["tokens"]["id_token"], "opaque-jwt");
        assert_eq!(json["custom"], "kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_old_refresh_times_need_refresh_after_eight_days() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let fresh = CodexOAuthCredentials {
            access_token: "a".into(),
            refresh_token: "r".into(),
            account_id: None,
            last_refresh: Some(now - Duration::from_secs(60)),
        };
        assert!(!fresh.needs_refresh(now));
        assert!(
            CodexOAuthCredentials {
                last_refresh: Some(now - CodexOAuthCredentials::REFRESH_AFTER),
                ..fresh.clone()
            }
            .needs_refresh(now)
        );
        assert!(
            CodexOAuthCredentials {
                last_refresh: None,
                ..fresh
            }
            .needs_refresh(now)
        );
    }

    #[test]
    fn refresh_401_is_classified_by_its_provider_reason() {
        assert_eq!(
            classify_token_refresh_failure(401, "refresh token reused"),
            TokenRefreshFailure::Reused
        );
        assert_eq!(
            classify_token_refresh_failure(401, "token revoked"),
            TokenRefreshFailure::Revoked
        );
        assert_eq!(
            classify_token_refresh_failure(401, "invalid_grant: expired"),
            TokenRefreshFailure::Expired
        );
        assert_eq!(
            classify_token_refresh_failure(500, "server unavailable"),
            TokenRefreshFailure::Other
        );
    }

    /// F-CORE-USG-06: the classified failure must route to a distinct
    /// `UsageReason` the status bar renders distinct copy for — not
    /// collapse to the generic `LoggedOut` for every case.
    #[test]
    fn token_refresh_failure_routes_to_distinct_usage_reasons() {
        assert_eq!(
            token_refresh_failure_reason(TokenRefreshFailure::Reused),
            UsageReason::TokenReused
        );
        assert_eq!(
            token_refresh_failure_reason(TokenRefreshFailure::Revoked),
            UsageReason::TokenRevoked
        );
        assert_eq!(
            token_refresh_failure_reason(TokenRefreshFailure::Expired),
            UsageReason::TokenExpired
        );
        assert_eq!(
            token_refresh_failure_reason(TokenRefreshFailure::Other),
            UsageReason::LoggedOut
        );
    }
}
