//! Executing one GraphQL request and turning the answer into data or a
//! [`ForgeError`] — once for both forges and both transports — plus the
//! small readers the forge modules parse with. Parsing reads JSON by
//! pointer rather than into fixed structs, so a missing or `null` field
//! degrades to its default instead of failing a whole list (spec §10).

use serde_json::{Value, json};

use crate::client::ForgeClient;
use crate::error::ForgeError;
use crate::mapping;
use crate::model::Revisions;
use crate::transport::{ApiResponse, RestRequest};

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
    check_status(host, response)?;
    let value = parse_body(host, response)?;
    let host = host.to_string();
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
        let depth = error
            .get("path")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
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

/// What an HTTP status means for any request: fine, or the error it is.
fn check_status(host: &str, response: &ApiResponse) -> Result<(), ForgeError> {
    let host = host.to_string();
    match response.status {
        200..=299 => Ok(()),
        401 => Err(ForgeError::NotAuthenticated { host }),
        403 | 429 if is_rate_limited(response) => Err(ForgeError::RateLimited {
            reset_at: rate_limit_reset(response),
            host,
        }),
        403 => Err(ForgeError::Forbidden {
            detail: rest_message(&response.body).unwrap_or_else(|| "forbidden".to_string()),
            sso_url: sso_url(response),
            host,
        }),
        404 => Err(ForgeError::NotFound { host }),
        status => Err(ForgeError::UnexpectedResponse {
            host,
            detail: format!("HTTP {status}"),
        }),
    }
}

fn parse_body(host: &str, response: &ApiResponse) -> Result<Value, ForgeError> {
    serde_json::from_slice(&response.body).map_err(|error| ForgeError::UnexpectedResponse {
        host: host.to_string(),
        detail: format!("not JSON: {error}"),
    })
}

/// A write's status. A `400`, `405`, `409` or `422` is the forge saying no
/// and why — a conflict, a validation, a method the account may not use —
/// so it is a [`ForgeError::Rejected`] carrying its words, not an
/// unreadable answer.
fn write_status(host: &str, response: &ApiResponse) -> Result<(), ForgeError> {
    match response.status {
        400 | 405 | 409 | 422 => Err(ForgeError::Rejected {
            host: host.to_string(),
            message: rest_message(&response.body)
                .unwrap_or_else(|| format!("HTTP {}", response.status)),
        }),
        _ => check_status(host, response),
    }
}

/// A REST answer: `Ok` for a 2xx, the error it means otherwise.
pub(crate) fn interpret_rest(
    host: &str,
    response: ApiResponse,
) -> Result<ApiResponse, ForgeError> {
    write_status(host, &response)?;
    Ok(response)
}

/// A GraphQL mutation. A query may degrade past an `errors` entry, because
/// the rest of its data is still worth drawing; a mutation may not — a
/// refusal is a refusal, and GitHub sends one with HTTP 200 and a `data`
/// whose payload is `null`, which the query interpreter would take for a
/// success.
pub(crate) fn execute_mutation(
    client: &ForgeClient,
    operation: &str,
    query: &str,
    variables: Value,
) -> Result<Value, ForgeError> {
    let body = json!({ "operationName": operation, "query": query, "variables": variables });
    let body = serde_json::to_vec(&body).expect("a JSON value serialises");
    let response = client.transport.post_graphql(&body)?;
    interpret_mutation(&client.host, &response)
}

/// One REST call, its answer interpreted as a write's: a 2xx, or the error
/// it means.
pub(crate) fn execute_rest(
    client: &ForgeClient,
    request: &RestRequest,
) -> Result<ApiResponse, ForgeError> {
    let response = client.transport.request(request)?;
    interpret_rest(&client.host, response)
}

pub(crate) fn interpret_mutation(host: &str, response: &ApiResponse) -> Result<Value, ForgeError> {
    write_status(host, response)?;
    let value = parse_body(host, response)?;
    if let Some(error) = mutation_error(host, response, &value) {
        return Err(error);
    }
    let data = match value.get("data") {
        Some(data) if !data.is_null() => data.clone(),
        _ => {
            return Err(ForgeError::UnexpectedResponse {
                host: host.to_string(),
                detail: "no data".to_string(),
            });
        }
    };
    match payload_error(host, &data) {
        Some(error) => Err(error),
        None => Ok(data),
    }
}

/// The first entry of a mutation's top-level `errors`, as the error it means.
fn mutation_error(host: &str, response: &ApiResponse, value: &Value) -> Option<ForgeError> {
    let first = value.get("errors")?.as_array()?.first()?;
    let kind = first.get("type").and_then(Value::as_str).unwrap_or("");
    let text = first
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("the forge reported an error");
    let host = host.to_string();
    Some(match kind {
        "FORBIDDEN" | "INSUFFICIENT_SCOPES" => ForgeError::Forbidden {
            host,
            detail: text.to_string(),
            sso_url: sso_url(response),
        },
        "NOT_FOUND" => ForgeError::NotFound { host },
        "RATE_LIMITED" => ForgeError::RateLimited {
            reset_at: rate_limit_reset(response),
            host,
        },
        _ => match unknown_field(text) {
            Some(field) => ForgeError::Unsupported { host, what: field },
            None => ForgeError::Rejected {
                host,
                message: text.to_string(),
            },
        },
    })
}

/// GitLab refuses a mutation with HTTP 200 and the reasons in the payload's
/// own `errors`, as strings. A payload that is `null` with no reason did
/// nothing, and is not a success.
fn payload_error(host: &str, data: &Value) -> Option<ForgeError> {
    for payload in data.as_object()?.values() {
        if payload.is_null() {
            return Some(ForgeError::UnexpectedResponse {
                host: host.to_string(),
                detail: "the forge answered with no result".to_string(),
            });
        }
        let reasons: Vec<&str> = payload
            .get("errors")
            .and_then(Value::as_array)
            .map(|errors| errors.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if !reasons.is_empty() {
            return Some(ForgeError::Rejected {
                host: host.to_string(),
                message: reasons.join("; "),
            });
        }
    }
    None
}

/// The words in a REST error body, whatever its shape: GitHub's `message`,
/// GitLab's `message` (a string, or an object of field errors) or `error`.
fn rest_message(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    for key in ["message", "error_description", "error"] {
        match value.get(key) {
            Some(Value::String(text)) => return Some(text.clone()),
            Some(other) if !other.is_null() => return Some(other.to_string()),
            _ => {}
        }
    }
    None
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
    value
        .split_once("url=")
        .map(|(_, url)| url.trim().to_string())
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

/// `None` when the field is absent or `null` — "the server did not say".
pub(crate) fn opt_bool(value: &Value, pointer: &str) -> Option<bool> {
    value.pointer(pointer).and_then(Value::as_bool)
}

pub(crate) fn bool_at(value: &Value, pointer: &str) -> bool {
    value
        .pointer(pointer)
        .and_then(Value::as_bool)
        .unwrap_or(false)
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
    info.get("hasNextPage")
        .and_then(Value::as_bool)
        .filter(|more| *more)?;
    info.get("endCursor")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// A connection read with `last:` has older items before this page.
pub(crate) fn has_previous_page(value: &Value, connection: &str) -> bool {
    bool_at(value, &format!("{connection}/pageInfo/hasPreviousPage"))
}

/// A commit id as a forge spells it: 40 (SHA-1) or 64 (SHA-256) hex digits,
/// returned lower-case. Anything else is `None`, because these strings end
/// up on git's command line — one that begins with `-` would be an option.
pub(crate) fn commit_id(value: Option<&str>) -> Option<String> {
    let value = value?;
    let well_formed =
        matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit());
    well_formed.then(|| value.to_ascii_lowercase())
}

/// The revisions a change request's diff is taken between. `None` unless both
/// ends are well formed; a malformed `start` alone is dropped.
pub(crate) fn revisions(
    base: Option<&str>,
    head: Option<&str>,
    start: Option<&str>,
) -> Option<Revisions> {
    Some(Revisions {
        base_sha: commit_id(base)?,
        head_sha: commit_id(head)?,
        start_sha: commit_id(start),
    })
}

#[cfg(test)]
mod revision_tests {
    use super::*;

    const BASE: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678";
    const HEAD: &str = "b2c3d4e5f60718293a4b5c6d7e8f901234567890";

    #[test]
    fn two_well_formed_ends_give_revisions() {
        let found = revisions(Some(BASE), Some(HEAD), Some(BASE)).expect("revisions");
        assert_eq!(found.base_sha, BASE);
        assert_eq!(found.head_sha, HEAD);
        assert_eq!(found.start_sha.as_deref(), Some(BASE));
    }

    #[test]
    fn a_missing_end_gives_none() {
        assert_eq!(revisions(None, Some(HEAD), None), None);
        assert_eq!(revisions(Some(BASE), None, None), None);
        assert_eq!(revisions(None, None, None), None);
    }

    #[test]
    fn anything_that_is_not_a_full_hex_commit_id_gives_none() {
        let forty_dashes = "-".repeat(40);
        let not_hex = format!("g{}", &BASE[1..]);
        let spaced = format!(" {BASE}");
        let newline = format!("{BASE}\n");
        let option = "--upload-pack=touch /tmp/pwned";
        for bad in [
            "",
            "abc",
            &BASE[..39],
            &format!("{BASE}0"),
            forty_dashes.as_str(),
            not_hex.as_str(),
            spaced.as_str(),
            newline.as_str(),
            option,
        ] {
            assert_eq!(commit_id(Some(bad)), None, "{bad:?} must be refused");
            assert_eq!(revisions(Some(bad), Some(HEAD), None), None);
            assert_eq!(revisions(Some(BASE), Some(bad), None), None);
        }
    }

    #[test]
    fn a_malformed_start_alone_is_dropped_not_fatal() {
        let found = revisions(Some(BASE), Some(HEAD), Some("nope")).expect("revisions");
        assert_eq!(found.start_sha, None);
    }

    #[test]
    fn upper_case_is_lowered_and_a_sha256_id_is_accepted() {
        assert_eq!(commit_id(Some(&BASE.to_uppercase())).as_deref(), Some(BASE));
        let sha256 = "0123456789abcdef".repeat(4);
        assert_eq!(commit_id(Some(&sha256)).as_deref(), Some(sha256.as_str()));
    }
}

#[cfg(test)]
mod write_tests {
    use super::*;

    const HOST: &str = "ghe.test";

    fn answer(status: u16, headers: &[(&str, &str)], body: Value) -> ApiResponse {
        ApiResponse {
            status,
            headers: headers
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
            body: serde_json::to_vec(&body).expect("a JSON value serialises"),
        }
    }

    fn mutation(body: Value) -> Result<Value, ForgeError> {
        interpret_mutation(HOST, &answer(200, &[], body))
    }

    /// The trap: GitHub answers a refused mutation with HTTP 200, `data`
    /// present but its payload `null`, and the reason only in `errors`. The
    /// query interpreter returns that `data` as a success.
    #[test]
    fn a_refused_github_mutation_is_rejected_with_the_forges_reason() {
        let refused = json!({
            "data": { "mergePullRequest": null },
            "errors": [{ "type": "UNPROCESSABLE", "path": ["mergePullRequest"], "message": "Pull request is not mergeable" }]
        });
        assert_eq!(
            mutation(refused),
            Err(ForgeError::Rejected {
                host: HOST.into(),
                message: "Pull request is not mergeable".into()
            })
        );
    }

    #[test]
    fn missing_scopes_are_forbidden_not_rejected() {
        let error = mutation(json!({
            "data": null,
            "errors": [{ "type": "INSUFFICIENT_SCOPES", "message": "Your token has not been granted the required scopes" }]
        }))
        .expect_err("a refusal");
        assert!(matches!(&error, ForgeError::Forbidden { detail, sso_url: None, .. } if detail.contains("required scopes")), "{error:?}");
    }

    #[test]
    fn a_fine_grained_token_without_the_permission_is_forbidden() {
        let error = mutation(json!({
            "data": { "addComment": null },
            "errors": [{ "type": "FORBIDDEN", "message": "Resource not accessible by personal access token" }]
        }))
        .expect_err("a refusal");
        assert!(matches!(error, ForgeError::Forbidden { .. }), "{error:?}");
    }

    #[test]
    fn a_saml_refusal_carries_its_authorisation_link() {
        let response = answer(
            200,
            &[("x-github-sso", "required; url=https://github.com/orgs/acme/sso?authorization_request=abc")],
            json!({ "data": null, "errors": [{ "type": "FORBIDDEN", "message": "Resource protected by organization SAML enforcement" }] }),
        );
        let error = interpret_mutation(HOST, &response).expect_err("a refusal");
        assert!(
            matches!(&error, ForgeError::Forbidden { sso_url: Some(url), .. } if url.ends_with("authorization_request=abc")),
            "{error:?}"
        );
    }

    #[test]
    fn a_change_request_that_vanished_is_not_found() {
        let error = mutation(json!({
            "data": { "closePullRequest": null },
            "errors": [{ "type": "NOT_FOUND", "path": ["closePullRequest"], "message": "Could not resolve to a node" }]
        }))
        .expect_err("a refusal");
        assert_eq!(error, ForgeError::NotFound { host: HOST.into() });
    }

    #[test]
    fn a_rate_limited_mutation_says_when_to_come_back() {
        let response = answer(
            200,
            &[("x-ratelimit-reset", "4102444800")],
            json!({ "data": null, "errors": [{ "type": "RATE_LIMITED", "message": "API rate limit exceeded" }] }),
        );
        assert_eq!(
            interpret_mutation(HOST, &response),
            Err(ForgeError::RateLimited { host: HOST.into(), reset_at: Some(4_102_444_800) })
        );
    }

    /// GitLab answers 200 and puts an application error in the payload.
    #[test]
    fn a_gitlab_payload_error_is_a_rejection() {
        assert_eq!(
            mutation(json!({ "data": { "createNote": { "errors": ["Body can't be blank"], "note": null } } })),
            Err(ForgeError::Rejected { host: HOST.into(), message: "Body can't be blank".into() })
        );
        assert_eq!(
            mutation(json!({ "data": { "mergeRequestUpdate": { "errors": ["Title can't be blank", "Target branch is invalid"], "mergeRequest": null } } })),
            Err(ForgeError::Rejected { host: HOST.into(), message: "Title can't be blank; Target branch is invalid".into() })
        );
    }

    /// An older GitLab does not know the mutation: that is "not supported on
    /// this version", not a refusal the user could fix.
    #[test]
    fn a_mutation_the_server_lacks_is_unsupported() {
        let error = mutation(json!({
            "errors": [{ "message": "Field 'mergeRequestRequestChanges' doesn't exist on type 'Mutation'", "extensions": { "code": "undefinedField" } }]
        }))
        .expect_err("a refusal");
        assert_eq!(
            error,
            ForgeError::Unsupported { host: HOST.into(), what: "mergeRequestRequestChanges".into() }
        );
    }

    #[test]
    fn a_payload_that_is_null_without_a_reason_is_unreadable_not_a_success() {
        let error = mutation(json!({ "data": { "addComment": null } })).expect_err("nothing was done");
        assert!(matches!(error, ForgeError::UnexpectedResponse { .. }), "{error:?}");
    }

    #[test]
    fn an_empty_error_list_is_not_an_error() {
        let data = mutation(json!({
            "data": { "addComment": { "commentEdge": { "node": { "id": "IC_1" } } } },
            "errors": []
        }))
        .expect("a success");
        assert_eq!(data["addComment"]["commentEdge"]["node"]["id"], "IC_1");
        let gitlab = mutation(json!({ "data": { "createNote": { "errors": [], "note": { "id": "gid://gitlab/Note/1" } } } }))
            .expect("a success");
        assert_eq!(gitlab["createNote"]["note"]["id"], "gid://gitlab/Note/1");
    }

    #[test]
    fn a_write_status_is_read_like_a_reads_and_a_422_is_a_rejection() {
        assert_eq!(
            interpret_mutation(HOST, &answer(401, &[], json!({}))),
            Err(ForgeError::NotAuthenticated { host: HOST.into() })
        );
        assert_eq!(
            interpret_mutation(HOST, &answer(422, &[], json!({ "message": "Validation Failed" }))),
            Err(ForgeError::Rejected { host: HOST.into(), message: "Validation Failed".into() })
        );
    }

    fn rest(status: u16, headers: &[(&str, &str)], body: Value) -> Result<ApiResponse, ForgeError> {
        interpret_rest(HOST, answer(status, headers, body))
    }

    #[test]
    fn a_rest_success_hands_its_answer_back() {
        let response = rest(201, &[], json!({ "approved": true })).expect("a success");
        assert_eq!(response.status, 201);
        assert!(rest(204, &[], json!(null)).is_ok(), "an empty answer is a success too");
    }

    #[test]
    fn a_rest_refusal_keeps_the_forges_reason_whatever_its_shape() {
        for status in [400_u16, 405, 409, 422] {
            assert_eq!(
                rest(status, &[], json!({ "message": "405 Method Not Allowed" })),
                Err(ForgeError::Rejected { host: HOST.into(), message: "405 Method Not Allowed".into() }),
                "status {status}"
            );
        }
        assert_eq!(
            rest(409, &[], json!({ "message": { "base": ["SHA does not match HEAD of source branch"] } })),
            Err(ForgeError::Rejected { host: HOST.into(), message: r#"{"base":["SHA does not match HEAD of source branch"]}"#.into() }),
            "GitLab answers a validation as an object"
        );
        assert_eq!(
            rest(422, &[], json!({ "error": "invalid" })),
            Err(ForgeError::Rejected { host: HOST.into(), message: "invalid".into() })
        );
        assert_eq!(
            rest(422, &[], json!({})),
            Err(ForgeError::Rejected { host: HOST.into(), message: "HTTP 422".into() }),
            "a refusal with no words still says what happened"
        );
    }

    #[test]
    fn a_rest_403_is_forbidden_unless_it_is_a_rate_limit() {
        let error = rest(403, &[], json!({ "error": "insufficient_scope", "message": "403 Forbidden" })).expect_err("refused");
        assert!(matches!(error, ForgeError::Forbidden { .. }), "{error:?}");
        assert_eq!(
            rest(403, &[("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", "4102444800")], json!({ "message": "API rate limit exceeded" })),
            Err(ForgeError::RateLimited { host: HOST.into(), reset_at: Some(4_102_444_800) })
        );
    }

    #[test]
    fn a_rest_404_401_and_500_keep_their_meaning() {
        assert_eq!(rest(404, &[], json!({})), Err(ForgeError::NotFound { host: HOST.into() }));
        assert_eq!(rest(401, &[], json!({})), Err(ForgeError::NotAuthenticated { host: HOST.into() }));
        assert!(matches!(rest(500, &[], json!({})), Err(ForgeError::UnexpectedResponse { .. })));
    }
}
