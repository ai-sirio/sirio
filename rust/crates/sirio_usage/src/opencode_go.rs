//! The OpenCode Go provider: a locally stored session cookie and two HTTPS
//! pages on opencode.ai, ported from `App/OpenCodeGoUsageFetcher.swift` and
//! `TillerCore/OpenCodeGoUsageParser.swift`.
//!
//! On macOS the cookie lives in the `com.sirio.usage` Keychain service the
//! Swift app writes, read read-only through the `security` CLI. On this
//! platform it lives in the app's own [`crate::CredentialStore`]
//! (F-SET-12), written by the settings surface. Flow: normalize the cookie,
//! discover the workspace id from `/_server` (or take the settings
//! override), then scrape the usage page (`/workspace/<id>/go`, React
//! Flight wire format) for the session (5h), weekly (wk) and monthly (mo)
//! windows. A missing cookie → logged out; a page that refuses or cannot
//! be parsed → error; a fetch that outlives its budget → `TimedOut`
//! (visibly stale, never a fabricated number).

#[cfg(target_os = "macos")]
use std::process::Command;
use std::time::Duration;

use crate::http::{HttpError, get};
use crate::model::{ProviderUsage, UsageFetchOutcome, UsageReason, UsageWindow};

/// The `X-Server-Id` / `id` of the OpenCode Go workspace server, from the
/// Swift app.
const SERVER_ID: &str = "def39973159c7f0483d8793a822b8dbb10d067e12c65455fcb4608459ba0234f";
/// The keychain service the Swift app's `KeychainCredentialStore` uses —
/// one service for every provider's cookie, so [`crate::ollama`] shares it.
#[cfg(target_os = "macos")]
pub(crate) const KEYCHAIN_SERVICE: &str = "com.sirio.usage";
/// How long a fetch may take before giving up (the Swift app's bound).
pub const TIMEOUT: Duration = Duration::from_secs(12);

/// A bare token (no `=`) is wrapped as `auth=<token>`; anything already
/// containing `=` passes through — the Swift's `normalizeCookie`.
pub fn normalize_cookie(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.contains('=') {
        trimmed.to_string()
    } else {
        format!("auth={trimmed}")
    }
}

/// Extracts the `wrk_...` workspace id from the `/_server` response body.
pub fn extract_workspace_id(body: &str) -> Option<String> {
    find_quoted_id(body).map(str::to_string)
}

fn find_quoted_id(body: &str) -> Option<&str> {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i + 4 < bytes.len() {
        // Look for `id:"wrk_..."`
        if bytes[i..].starts_with(b"id:") {
            let mut j = i + 3;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if bytes.get(j) == Some(&b'"') {
                let start = j + 1;
                let end = body[start..]
                    .find('"')
                    .map(|offset| start + offset)
                    .unwrap_or(body.len());
                let id = &body[start..end];
                if id.starts_with("wrk_") && id.len() > 4 {
                    return Some(id);
                }
            }
        }
        i += 1;
    }
    None
}

/// Extracts session/weekly/monthly usage windows from the usage page body
/// (React Flight wire format). Mirrors the Swift's regexes: find the
/// `{...}` block after each key (optionally `$R[n]=`-prefixed), then read
/// `usagePercent` and `resetInSec` from inside that block.
pub fn extract_usage(body: &str) -> Option<ProviderUsage> {
    let session = window(body, "rollingUsage", "5h");
    let weekly = window(body, "weeklyUsage", "wk");
    let monthly = window(body, "monthlyUsage", "mo");
    let usage = ProviderUsage {
        session,
        weekly,
        monthly,
        fable_weekly: None,
    };
    usage.has_any().then_some(usage)
}

fn window(body: &str, key: &str, label: &str) -> Option<UsageWindow> {
    let block = block_after(body, key)?;
    let used_percent = integer_after(&block, "usagePercent")? as u8;
    let reset_in_sec = integer_after(&block, "resetInSec")?;
    let resets_at = std::time::SystemTime::now().checked_add(Duration::from_secs(reset_in_sec));
    Some(UsageWindow {
        label: label.to_string(),
        used_percent: used_percent.min(100),
        resets_at,
    })
}

/// The `{...}` block following `key`, accepting a `:` separator and an
/// optional `$R[n]=` prefix: `key:$R[12]={...}` or `key:{...}`.
fn block_after(body: &str, key: &str) -> Option<String> {
    let key_bytes = key.as_bytes();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i + key_bytes.len() <= bytes.len() {
        if &bytes[i..i + key_bytes.len()] == key_bytes {
            let mut j = i + key_bytes.len();
            // The `:` separating the key from its value.
            if bytes.get(j) == Some(&b':') {
                j += 1;
            }
            // Optional `$R[n]=` marker.
            if bytes.get(j) == Some(&b'$') {
                while j < bytes.len() && bytes[j] != b'=' {
                    j += 1;
                }
                j += 1;
            }
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if bytes.get(j) == Some(&b'{') {
                // Find the matching close brace (the Swift regex is
                // `[^}]*`, so the first close brace ends the block).
                let open = j;
                let close = body[open..].find('}').map(|offset| open + offset)?;
                return Some(body[open + 1..close].to_string());
            }
        }
        i += 1;
    }
    None
}

fn integer_after(text: &str, key: &str) -> Option<u64> {
    let key_bytes = key.as_bytes();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + key_bytes.len() <= bytes.len() {
        if &bytes[i..i + key_bytes.len()] == key_bytes {
            let mut j = i + key_bytes.len();
            // The wire format is `usagePercent:12` / `resetInSec:100`.
            if bytes.get(j) == Some(&b':') {
                j += 1;
            }
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            let start = j;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > start {
                return text[start..j].parse().ok();
            }
        }
        i += 1;
    }
    None
}

/// Reads a keychain generic password via the `security` CLI, read-only.
#[cfg(target_os = "macos")]
pub(crate) fn keychain_cookie(service: &str, account: &str) -> Option<String> {
    let output = Command::new("security")
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .current_dir(crate::probe_working_directory())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let cookie = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!cookie.is_empty()).then_some(cookie)
}

/// The OpenCode Go session cookie from this platform's local store —
/// **presence, not validity**. macOS reads the Keychain item the Swift app
/// writes; other platforms read the app's own credential file
/// ([`crate::CredentialStore`]), the store the settings surface saves into
/// (F-SET-12).
pub(crate) fn opencode_go_local_cookie() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        keychain_cookie(KEYCHAIN_SERVICE, OpenCodeGoUsageFetcher::COOKIE_KEY)
    }
    #[cfg(not(target_os = "macos"))]
    {
        crate::CredentialStore::from_env()
            .ok()?
            .get(OpenCodeGoUsageFetcher::COOKIE_KEY)
            .map(|cookie| cookie.trim().to_string())
            .filter(|cookie| !cookie.is_empty())
    }
}

/// A settings override becomes the workspace id only when it has content
/// after trimming — an empty or whitespace field means "discover", not
/// "fetch workspace ''" (the Swift fetcher's guard).
pub fn workspace_id_from_override(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

/// The OpenCode Go usage fetcher. **Blocking** — bounded by [`TIMEOUT`];
/// call it off the render thread.
pub struct OpenCodeGoUsageFetcher;

impl OpenCodeGoUsageFetcher {
    /// The credential key for the session cookie — the Swift
    /// `OpenCodeGoUsageFetcher.cookieKey`, shared by the Keychain item and
    /// the on-disk store so the two platforms name the credential
    /// identically.
    pub const COOKIE_KEY: &'static str = "opencode-go-cookie";

    /// Fetches usage. A non-empty `workspace_id_override` (the settings
    /// field) skips `/_server` discovery entirely — the Swift fetcher's
    /// `workspaceIdOverride` behavior.
    pub fn fetch(workspace_id_override: Option<&str>) -> UsageFetchOutcome {
        let Some(raw_cookie) = opencode_go_local_cookie() else {
            return UsageFetchOutcome::Unavailable(UsageReason::LoggedOut);
        };
        let cookie = normalize_cookie(&raw_cookie);

        let workspace_id = match workspace_id_from_override(workspace_id_override) {
            Some(id) => id,
            None => {
                let server_url = format!("https://opencode.ai/_server?id={SERVER_ID}");
                let server_body = match get(
                    &server_url,
                    &[("Cookie", &cookie), ("X-Server-Id", SERVER_ID)],
                    TIMEOUT.as_secs(),
                ) {
                    Ok(response) if (200..300).contains(&response.code) => response.body,
                    Ok(_) => return UsageFetchOutcome::Unavailable(UsageReason::Error),
                    Err(HttpError::TimedOut) => return UsageFetchOutcome::TimedOut,
                    Err(HttpError::Network(error)) => {
                        eprintln!("[opencode-go-usage] server request failed: {error}");
                        return UsageFetchOutcome::Unavailable(UsageReason::Error);
                    }
                };
                match extract_workspace_id(&server_body) {
                    Some(id) => id,
                    None => return UsageFetchOutcome::Unavailable(UsageReason::Error),
                }
            }
        };

        let usage_url = format!("https://opencode.ai/workspace/{workspace_id}/go");
        let page = match get(&usage_url, &[("Cookie", &cookie)], TIMEOUT.as_secs()) {
            Ok(response) if (200..300).contains(&response.code) => response.body,
            Ok(_) => return UsageFetchOutcome::Unavailable(UsageReason::Error),
            Err(HttpError::TimedOut) => return UsageFetchOutcome::TimedOut,
            Err(HttpError::Network(error)) => {
                eprintln!("[opencode-go-usage] usage request failed: {error}");
                return UsageFetchOutcome::Unavailable(UsageReason::Error);
            }
        };

        match extract_usage(&page) {
            Some(usage) => UsageFetchOutcome::Success(usage),
            None => UsageFetchOutcome::Unavailable(UsageReason::Error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_bare_and_prefixed_cookies() {
        assert_eq!(normalize_cookie("tok123"), "auth=tok123");
        assert_eq!(normalize_cookie("auth=tok123"), "auth=tok123");
        assert_eq!(normalize_cookie("  auth=tok123  "), "auth=tok123");
        assert_eq!(normalize_cookie(""), "");
    }

    #[test]
    fn extracts_a_workspace_id_from_the_server_body() {
        // The `/_server` wire format uses unquoted keys: `{id:"wrk_..."}`.
        assert_eq!(
            extract_workspace_id(r#"{result:{items:[{id:"wrk_abc123",name:"x"}]}}"#),
            Some("wrk_abc123".to_string())
        );
        // A bare `id:null` must not match.
        assert_eq!(extract_workspace_id(r#"{id:null}"#), None);
    }

    #[test]
    fn parses_a_react_flight_usage_page() {
        // The wire format puts resetInSec before usagePercent and may
        // prefix blocks with $R[n]= — the parser must be order-agnostic.
        let page = r#"$R[0]={id:"wrk_1"}
$R[1]={rollingUsage:$R[2]={resetInSec:100,usagePercent:12}}
$R[3]={weeklyUsage:{usagePercent:34,resetInSec:200}}
$R[4]={monthlyUsage:$R[5]={usagePercent:71,resetInSec:300}}"#;
        let usage = extract_usage(page).expect("parses");
        assert_eq!(
            usage
                .session
                .as_ref()
                .map(|w| (w.used_percent, w.label.as_str())),
            Some((12, "5h"))
        );
        assert_eq!(
            usage
                .weekly
                .as_ref()
                .map(|w| (w.used_percent, w.label.as_str())),
            Some((34, "wk"))
        );
        assert_eq!(
            usage
                .monthly
                .as_ref()
                .map(|w| (w.used_percent, w.label.as_str())),
            Some((71, "mo"))
        );
    }

    /// F-SET-12: the workspace-ID override is honored only with content —
    /// trimmed, and an empty field falls back to `/_server` discovery.
    #[test]
    fn workspace_override_requires_content_after_trimming() {
        assert_eq!(
            workspace_id_from_override(Some("  wrk_abc  ")),
            Some("wrk_abc".to_string())
        );
        assert_eq!(workspace_id_from_override(Some("")), None);
        assert_eq!(workspace_id_from_override(Some("   ")), None);
        assert_eq!(workspace_id_from_override(None), None);
    }

    #[test]
    fn garbage_is_not_usage() {
        assert_eq!(extract_usage("<!doctype html><html>"), None);
        assert_eq!(extract_usage(r#"{"rollingUsage":null}"#), None);
        assert_eq!(window("rollingUsage:null", "rollingUsage", "5h"), None);
    }
}
