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

use std::path::PathBuf;
use std::time::Duration;

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
struct CodexCredentials {
    access_token: String,
    refresh_token: String,
    account_id: Option<String>,
}

/// `~/.codex/auth.json`, or `$CODEX_HOME/auth.json` when set — the same
/// precedence `codex` itself uses.
fn auth_file_path() -> PathBuf {
    if let Some(home) = std::env::var_os("CODEX_HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home).join("auth.json");
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".codex/auth.json"))
        .expect("HOME must be set")
}

fn load_credentials() -> Result<CodexCredentials, ()> {
    load_credentials_from(&auth_file_path())
}

fn load_credentials_from(path: &std::path::Path) -> Result<CodexCredentials, ()> {
    let data = std::fs::read(path).map_err(|_| ())?;
    let json: Value = serde_json::from_slice(&data).map_err(|_| ())?;
    let tokens = json.get("tokens").and_then(Value::as_object).ok_or(())?;
    let access_token = tokens
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or(())?;
    let refresh_token = tokens
        .get("refresh_token")
        .and_then(Value::as_str)
        .ok_or(())?;
    Ok(CodexCredentials {
        access_token: access_token.to_string(),
        refresh_token: refresh_token.to_string(),
        account_id: tokens
            .get("account_id")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// Merges refreshed tokens into the auth file rather than overwriting it —
/// `codex` itself may store other fields Tiller doesn't know about.
fn save_credentials(credentials: &CodexCredentials) -> Result<(), ()> {
    save_credentials_to(&auth_file_path(), credentials)
}

fn save_credentials_to(
    path: &std::path::Path,
    credentials: &CodexCredentials,
) -> Result<(), ()> {
    let mut json: Value = std::fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_else(|| Value::Object(Default::default()));
    let mut tokens = json
        .get("tokens")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    tokens.insert("access_token".into(), Value::String(credentials.access_token.clone()));
    tokens.insert("refresh_token".into(), Value::String(credentials.refresh_token.clone()));
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
    chrono::Utc::now()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
/// Calls the wham API with `access_token`; 401/403 means the token is no
/// longer accepted.
fn fetch_usage(access_token: &str, account_id: Option<&str>, timeout: Duration) -> Result<ProviderUsage, CodexApiFailure> {
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
fn refresh_token(credentials: &CodexCredentials) -> Result<CodexCredentials, ()> {
    let body = format!(
        r#"{{"client_id":"{CLIENT_ID}","grant_type":"refresh_token","refresh_token":"{}","scope":"openid profile email"}}"#,
        credentials.refresh_token
    );
    let response = post(
        TOKEN_URL,
        &[("Content-Type", "application/json")],
        &body,
        TIMEOUT.as_secs(),
    )
    .map_err(|_| ())?;
    if response.code != 200 {
        return Err(());
    }
    let json: Value = serde_json::from_str(&response.body).map_err(|_| ())?;
    let access_token = json.get("access_token").and_then(Value::as_str).ok_or(())?;
    let refresh_token = json
        .get("refresh_token")
        .and_then(Value::as_str)
        .unwrap_or(&credentials.refresh_token);
    Ok(CodexCredentials {
        access_token: access_token.to_string(),
        refresh_token: refresh_token.to_string(),
        account_id: credentials.account_id.clone(),
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
        .and_then(|epoch| std::time::SystemTime::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(epoch)));
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
            Err(()) => return UsageFetchOutcome::Unavailable(UsageReason::LoggedOut),
        };

        match fetch_usage(&credentials.access_token, credentials.account_id.as_deref(), TIMEOUT) {
            Ok(usage) => return UsageFetchOutcome::Success(usage),
            Err(CodexApiFailure::TimedOut) => return UsageFetchOutcome::TimedOut,
            Err(CodexApiFailure::Error) => return UsageFetchOutcome::Unavailable(UsageReason::Error),
            Err(CodexApiFailure::Unauthorized) => {}
        }

        // The access token was rejected: refresh once and retry.
        let refreshed = match refresh_token(&credentials) {
            Ok(refreshed) => refreshed,
            Err(()) => return UsageFetchOutcome::Unavailable(UsageReason::LoggedOut),
        };
        let _ = save_credentials(&refreshed);
        match fetch_usage(&refreshed.access_token, refreshed.account_id.as_deref(), TIMEOUT) {
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
                    std::time::SystemTime::UNIX_EPOCH
                        + std::time::Duration::from_secs(1787038258)
                ),
            })
        );
        assert_eq!(usage.weekly, None, "secondary_window is null on this account");
    }

    #[test]
    fn parses_a_secondary_window_when_present() {
        let body = r#"{"rate_limit":{"primary_window":{"used_percent":20},"secondary_window":{"used_percent":40,"reset_at":1800000000}}}"#;
        let usage = parse_usage(body).expect("parses");
        assert_eq!(usage.session.as_ref().map(|w| w.used_percent), Some(20));
        assert_eq!(usage.weekly.as_ref().map(|w| (w.used_percent, w.label.as_str())), Some((40, "wk")));
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
}
