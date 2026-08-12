//! Bounded HTTPS via `curl` (macOS ships it), for the network-backed usage
//! providers. The deadline is enforced by curl's own `--max-time`, so the
//! child process can never hang the caller; the fetch is a blocking call and
//! must run off the render thread (the status bar runs it on GPUI's
//! background executor).

use std::process::Command;

/// A completed HTTP response.
pub struct HttpResponse {
    /// The response body.
    pub body: String,
    /// The HTTP status code.
    pub code: i32,
}

/// Why the HTTP request could not complete.
#[derive(Debug)]
pub enum HttpError {
    /// `curl`'s "operation timeout" (exit 28): the request was aborted by
    /// the deadline.
    TimedOut,
    /// Spawn failure or a non-zero curl exit — the connection itself failed.
    Network(String),
}

/// GET `url` with `headers`, bounded by `timeout_secs`. Non-2xx responses
/// are returned with their status code (the caller decides what 401/403
/// means); transport failures are [`HttpError`].
pub fn get(
    url: &str,
    headers: &[(&str, &str)],
    timeout_secs: u64,
) -> Result<HttpResponse, HttpError> {
    request("GET", url, headers, None, timeout_secs)
}

/// POST `body` to `url` with `headers`, bounded by `timeout_secs`.
pub fn post(
    url: &str,
    headers: &[(&str, &str)],
    body: &str,
    timeout_secs: u64,
) -> Result<HttpResponse, HttpError> {
    request("POST", url, headers, Some(body), timeout_secs)
}

fn request(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
    timeout_secs: u64,
) -> Result<HttpResponse, HttpError> {
    let mut command = Command::new("curl");
    command
        .arg("--silent")
        .arg("--show-error")
        .arg("--max-time")
        .arg(timeout_secs.to_string())
        .arg("--location")
        // A trailing marker with the status code, so 401/403 can be told
        // apart from other failures without `--fail` discarding the code.
        .arg("--write-out")
        .arg("\n%{http_code}")
        .arg("--request")
        .arg(method);
    for (name, value) in headers {
        command.arg("--header").arg(format!("{name}: {value}"));
    }
    if let Some(body) = body {
        command.arg("--data-binary").arg(body);
    }
    command.arg(url);

    let output = command
        .output()
        .map_err(|error| HttpError::Network(error.to_string()))?;
    if let Some(code) = output.status.code() {
        if code == 28 {
            return Err(HttpError::TimedOut);
        }
        if code != 0 {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(HttpError::Network(stderr));
        }
    }

    // Split off the trailing `\n<code>` marker written by `--write-out`.
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    let code = text
        .rfind('\n')
        .and_then(|split| text[split + 1..].trim().parse::<i32>().ok())
        .unwrap_or(0);
    if let Some(split) = text.rfind('\n') {
        text.truncate(split);
    }
    Ok(HttpResponse { body: text, code })
}
