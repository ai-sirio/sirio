//! Executing one GraphQL request and turning the answer into data or a
//! [`ForgeError`] — once for both forges and both transports — plus the
//! small readers the forge modules parse with. Parsing reads JSON by
//! pointer rather than into fixed structs, so a missing or `null` field
//! degrades to its default instead of failing a whole list (spec §10).
// Some readers are first used by Tasks 6-7; Task 7, Step 7 deletes this.
#![allow(dead_code)]

use serde_json::{Value, json};

use crate::client::ForgeClient;
use crate::error::ForgeError;
use crate::mapping;
use crate::transport::ApiResponse;

pub(crate) fn execute(
    client: &ForgeClient,
    operation: &str,
    query: &str,
    variables: Value,
) -> Result<Value, ForgeError> {
    let body = json!({ "operationName": operation, "query": query, "variables": variables });
    let body = serde_json::to_vec(&body).expect("a JSON value serialises");
    let response = client.transport.post_graphql(&body)?;
    interpret(&client.host, &response)
}

/// For a query with no baseline: a server that does not know a field is
/// simply an answer Sirio cannot read.
pub(crate) fn no_unknown_field(error: ForgeError) -> ForgeError {
    match error {
        ForgeError::UnknownField { host, field } => ForgeError::UnexpectedResponse {
            host,
            detail: format!("the server does not know `{field}`"),
        },
        other => other,
    }
}

fn interpret(host: &str, response: &ApiResponse) -> Result<Value, ForgeError> {
    let host = host.to_string();
    match response.status {
        200..=299 => {}
        401 => return Err(ForgeError::NotAuthenticated { host }),
        403 | 429 if is_rate_limited(response) => {
            return Err(ForgeError::RateLimited {
                reset_at: rate_limit_reset(response),
                host,
            });
        }
        403 => {
            return Err(ForgeError::Forbidden {
                detail: message(&response.body).unwrap_or_else(|| "forbidden".to_string()),
                sso_url: sso_url(response),
                host,
            });
        }
        404 => return Err(ForgeError::NotFound { host }),
        status => {
            return Err(ForgeError::UnexpectedResponse {
                host,
                detail: format!("HTTP {status}"),
            });
        }
    }
    let value: Value =
        serde_json::from_slice(&response.body).map_err(|error| ForgeError::UnexpectedResponse {
            host: host.clone(),
            detail: format!("not JSON: {error}"),
        })?;
    let errors = value
        .get("errors")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for error in &errors {
        let kind = error.get("type").and_then(Value::as_str).unwrap_or("");
        let text = error.get("message").and_then(Value::as_str).unwrap_or("");
        // Only a missing repository or change request is "not found"; a
        // deeper NOT_FOUND (a deleted team among the reviewers) degrades.
        let depth = error.get("path").and_then(Value::as_array).map_or(0, Vec::len);
        if kind == "NOT_FOUND" && depth <= 2 {
            return Err(ForgeError::NotFound { host });
        }
        if kind == "RATE_LIMITED" {
            return Err(ForgeError::RateLimited {
                reset_at: rate_limit_reset(response),
                host,
            });
        }
        if let Some(field) = unknown_field(text) {
            return Err(ForgeError::UnknownField { host, field });
        }
    }
    match value.get("data") {
        Some(data) if !data.is_null() => Ok(data.clone()),
        _ => Err(ForgeError::UnexpectedResponse {
            detail: errors
                .first()
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("no data")
                .to_string(),
            host,
        }),
    }
}

fn is_rate_limited(response: &ApiResponse) -> bool {
    response.status == 429
        || response.header("x-ratelimit-remaining") == Some("0")
        || response.header("ratelimit-remaining") == Some("0")
        || response.header("retry-after").is_some()
}

/// GitHub's `x-ratelimit-reset` and GitLab's `ratelimit-reset` are Unix
/// seconds; `retry-after` is a delay.
fn rate_limit_reset(response: &ApiResponse) -> Option<i64> {
    response
        .header("x-ratelimit-reset")
        .or_else(|| response.header("ratelimit-reset"))
        .and_then(|value| value.trim().parse::<i64>().ok())
        .or_else(|| {
            let delay = response.header("retry-after")?.trim().parse::<i64>().ok()?;
            Some(chrono::Utc::now().timestamp() + delay)
        })
}

/// `X-GitHub-SSO: required; url=https://github.com/orgs/…/sso?…`.
fn sso_url(response: &ApiResponse) -> Option<String> {
    let value = response.header("x-github-sso")?;
    value.split_once("url=").map(|(_, url)| url.trim().to_string())
}

fn message(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    value.get("message")?.as_str().map(str::to_string)
}

/// GitLab: `Field 'x' doesn't exist on type 'Y'` or `Field 'x' doesn't
/// accept argument 'y'`.
fn unknown_field(message: &str) -> Option<String> {
    let rest = message.strip_prefix("Field '")?;
    let (field, tail) = rest.split_once('\'')?;
    (tail.starts_with(" doesn't exist") || tail.starts_with(" doesn't accept argument"))
        .then(|| field.to_string())
}

pub(crate) fn opt_str<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

pub(crate) fn str_at(value: &Value, pointer: &str) -> String {
    opt_str(value, pointer).unwrap_or_default().to_string()
}

pub(crate) fn opt_u32(value: &Value, pointer: &str) -> Option<u32> {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .and_then(|number| u32::try_from(number).ok())
}

pub(crate) fn u32_at(value: &Value, pointer: &str) -> u32 {
    opt_u32(value, pointer).unwrap_or(0)
}

pub(crate) fn bool_at(value: &Value, pointer: &str) -> bool {
    value.pointer(pointer).and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn time_at(value: &Value, pointer: &str) -> Option<i64> {
    opt_str(value, pointer).and_then(mapping::unix_seconds)
}

/// The non-null entries of the array at `pointer`.
pub(crate) fn array_at<'a>(value: &'a Value, pointer: &str) -> Vec<&'a Value> {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter(|item| !item.is_null()).collect())
        .unwrap_or_default()
}

/// The cursor that continues the connection at `connection`, when it has a
/// next page.
pub(crate) fn next_cursor(value: &Value, connection: &str) -> Option<String> {
    let info = value.pointer(&format!("{connection}/pageInfo"))?;
    info.get("hasNextPage").and_then(Value::as_bool).filter(|more| *more)?;
    info.get("endCursor").and_then(Value::as_str).map(str::to_string)
}

/// A connection read with `last:` has older items before this page.
pub(crate) fn has_previous_page(value: &Value, connection: &str) -> bool {
    bool_at(value, &format!("{connection}/pageInfo/hasPreviousPage"))
}
