//! Sending one GraphQL request to a forge and returning its HTTP answer.
//!
//! Two implementations of [`Transport`], chosen by the user's means (spec
//! §5): [`TokenTransport`] speaks HTTP itself, and [`CliTransport`] (Task 4)
//! hands the request to `gh api` / `glab api`, which own authentication.
//! Neither reads the answer — `graphql::execute` does that once for both.

use std::io::{Read as _, Write as _};
use std::process::{Command, Stdio};
use std::time::Duration;
use std::time::Instant;

use crate::error::ForgeError;
use crate::model::Forge;

/// An HTTP answer. Header names are lower-case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl ApiResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// The verbs a REST call uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestMethod {
    Get,
    Post,
    Put,
    Delete,
}

impl RestMethod {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        }
    }
}

/// One REST call, for what GraphQL does not carry: approving a GitLab merge
/// request, re-running a GitHub job, reading a job's log. `path` is relative
/// to the forge's API root — `repos/acme/widgets/actions/jobs/7/rerun`,
/// `projects/team%2Fapp/merge_requests/7/approve` — and is built by Sirio,
/// never taken from what a forge said. `body`, when there is one, is JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestRequest {
    pub method: RestMethod,
    pub path: String,
    pub body: Option<Vec<u8>>,
    /// A CI log rather than a JSON answer. The token means then does not
    /// follow a redirect — the caller fetches the `Location` itself with no
    /// credential (spec §4) — and reads up to `LOG_DOWNLOAD_LIMIT`; `gh` is
    /// told to print terminal escapes.
    pub log: bool,
}

impl RestRequest {
    pub(crate) fn get(path: String) -> Self {
        Self { method: RestMethod::Get, path, body: None, log: false }
    }
}

/// Sends requests to one forge host: a GraphQL body — `{"operationName",
/// "query", "variables"}` as JSON — or one REST call.
pub trait Transport: Send + Sync {
    fn post_graphql(&self, body: &[u8]) -> Result<ApiResponse, ForgeError>;
    fn request(&self, request: &RestRequest) -> Result<ApiResponse, ForgeError>;
}

const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

/// HTTP with a personal access token, sent as `Authorization: Bearer`,
/// which both forges accept for one.
pub struct TokenTransport {
    endpoint: String,
    forge: Forge,
    host: String,
    token: String,
    agent: ureq::Agent,
    log_agent: ureq::Agent,
}

impl TokenTransport {
    pub fn new(forge: Forge, host: &str, token: String) -> Self {
        Self {
            endpoint: graphql_endpoint(forge, host),
            forge,
            host: host.to_string(),
            token,
            agent: http_agent(HTTP_TIMEOUT),
            log_agent: log_agent(),
        }
    }
}

impl std::fmt::Debug for TokenTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TokenTransport")
            .field("endpoint", &self.endpoint)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl TokenTransport {
    /// Reads whatever the forge answered, whatever its status: a status is
    /// an answer here, and `interpret` decides what it means.
    fn read(&self, response: ureq::http::Response<ureq::Body>, limit: u64) -> Result<ApiResponse, ForgeError> {
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_ascii_lowercase(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        let mut body = response.into_body();
        let body = body
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|error| match error {
                ureq::Error::BodyExceedsLimit(_) => ForgeError::UnexpectedResponse {
                    host: self.host.clone(),
                    detail: "the answer is larger than Sirio reads; open it in the browser".to_string(),
                },
                other => classify(&self.host, other),
            })?;
        Ok(ApiResponse {
            status,
            headers,
            body,
        })
    }
}

impl Transport for TokenTransport {
    fn post_graphql(&self, body: &[u8]) -> Result<ApiResponse, ForgeError> {
        let _perf = sirio_perf::span("forge.http_request", 0);
        let response = self
            .agent
            .post(&self.endpoint)
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .header("User-Agent", "Sirio")
            .send(body)
            .map_err(|error| classify(&self.host, error))?;
        self.read(response, 10 * 1024 * 1024)
    }

    fn request(&self, request: &RestRequest) -> Result<ApiResponse, ForgeError> {
        let _perf = sirio_perf::span("forge.http_rest", 0);
        let builder = ureq::http::Request::builder()
            .method(request.method.as_str())
            .uri(rest_endpoint(self.forge, &self.host, &request.path))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/json")
            .header("User-Agent", "Sirio");
        let built = match &request.body {
            Some(body) => builder
                .header("Content-Type", "application/json")
                .body(body.clone()),
            None => builder.body(Vec::new()),
        };
        let http_request = built.map_err(|error| ForgeError::Network {
            host: self.host.clone(),
            detail: format!("an unusable request: {error}"),
        })?;
        let agent = if request.log { &self.log_agent } else { &self.agent };
        let response = agent
            .run(http_request)
            .map_err(|error| classify(&self.host, error))?;
        self.read(response, if request.log { crate::LOG_DOWNLOAD_LIMIT } else { 10 * 1024 * 1024 })
    }
}

/// A client that returns every HTTP status as an answer rather than an
/// error, and verifies certificates with the platform's own verifier.
pub(crate) fn http_agent(timeout: Duration) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let config = ureq::Agent::config_builder()
        .tls_config(tls)
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .build();
    ureq::Agent::new_with_config(config)
}

/// How long a log download may take: a large log is tens of megabytes.
pub(crate) const LOG_TIMEOUT: Duration = Duration::from_secs(120);

/// Like `http_agent`, but a redirect is handed back instead of followed:
/// GitHub's log answers with a signed URL on another host, which the caller
/// fetches with no credential (spec §4).
fn log_agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let config = ureq::Agent::config_builder()
        .tls_config(tls)
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .timeout_global(Some(LOG_TIMEOUT))
        .build();
    ureq::Agent::new_with_config(config)
}

pub(crate) fn classify(host: &str, error: ureq::Error) -> ForgeError {
    let host = host.to_string();
    match error {
        ureq::Error::Tls(detail) => ForgeError::Tls {
            host,
            detail: detail.to_string(),
        },
        ureq::Error::Rustls(detail) => ForgeError::Tls {
            host,
            detail: detail.to_string(),
        },
        ureq::Error::TlsRequired => ForgeError::Tls {
            host,
            detail: "the server requires TLS".to_string(),
        },
        ureq::Error::Timeout(_) => ForgeError::Network {
            host,
            detail: "timed out".to_string(),
        },
        ureq::Error::HostNotFound => ForgeError::Network {
            host,
            detail: "host not found".to_string(),
        },
        other => ForgeError::Network {
            host,
            detail: other.to_string(),
        },
    }
}

/// GitHub.com's API lives on `api.github.com`; GitHub Enterprise Server and
/// every GitLab serve GraphQL at `/api/graphql` on their own host.
pub(crate) fn graphql_endpoint(forge: Forge, host: &str) -> String {
    if forge == Forge::GitHub && host == "github.com" {
        let base = test_base(host).unwrap_or_else(|| "https://api.github.com".to_string());
        return format!("{base}/graphql");
    }
    format!("{}/api/graphql", web_base(host))
}

/// Where a REST `path` lives: `https://api.github.com/` for github.com,
/// `https://H/api/v3/` on GitHub Enterprise Server, `https://H/api/v4/` on
/// GitLab.
pub(crate) fn rest_endpoint(forge: Forge, host: &str, path: &str) -> String {
    if forge == Forge::GitHub && host == "github.com" {
        let base = test_base(host).unwrap_or_else(|| "https://api.github.com".to_string());
        return format!("{base}/{path}");
    }
    let root = match forge {
        Forge::GitHub => "api/v3",
        Forge::GitLab => "api/v4",
    };
    format!("{}/{root}/{path}", web_base(host))
}

/// `https://<host>`, or the test endpoint standing in for it.
pub(crate) fn web_base(host: &str) -> String {
    test_base(host).unwrap_or_else(|| format!("https://{host}"))
}

/// Debug builds only: `SIRIO_FORGE_TEST_ENDPOINTS="host=http://127.0.0.1:P,…"`
/// replaces a host's scheme and authority, so the e2e script can point the
/// real transports at a loopback forge. A release build has no such door,
/// as it has none for `SIRIO_UPDATE_MANIFEST_URL` (spec §11.1).
fn test_base(host: &str) -> Option<String> {
    #[cfg(debug_assertions)]
    {
        let map = std::env::var("SIRIO_FORGE_TEST_ENDPOINTS").ok()?;
        map.split(',')
            .filter_map(|entry| entry.split_once('='))
            .find(|(name, _)| name.trim().eq_ignore_ascii_case(host))
            .map(|(_, base)| base.trim().trim_end_matches('/').to_string())
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = host;
        None
    }
}

const CLI_TIMEOUT: Duration = Duration::from_secs(30);

/// Which forge CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliProgram {
    Gh,
    Glab,
}

impl CliProgram {
    pub fn for_forge(forge: Forge) -> Self {
        match forge {
            Forge::GitHub => Self::Gh,
            Forge::GitLab => Self::Glab,
        }
    }

    pub fn command(self) -> &'static str {
        match self {
            Self::Gh => "gh",
            Self::Glab => "glab",
        }
    }
}

/// The forge's own CLI, which owns authentication — its keyring, OAuth
/// refresh, SSO. The request goes to
/// `<cli> api --hostname H --include --method POST graphql --input -`, the
/// body on stdin.
#[derive(Debug)]
pub struct CliTransport {
    program: CliProgram,
    host: String,
}

impl CliTransport {
    pub fn new(program: CliProgram, host: &str) -> Self {
        Self {
            program,
            host: host.to_string(),
        }
    }
}

impl CliTransport {
    fn run_error(&self, error: RunError) -> ForgeError {
        match error {
            RunError::NotInstalled => ForgeError::NotInstalled {
                program: self.program.command(),
            },
            RunError::TimedOut => ForgeError::Network {
                host: self.host.clone(),
                detail: format!(
                    "{} did not answer within {} s",
                    self.program.command(),
                    CLI_TIMEOUT.as_secs()
                ),
            },
            RunError::OutputTooLarge => ForgeError::UnexpectedResponse {
                host: self.host.clone(),
                detail: "the log is larger than 64 MiB; open it in the browser".to_string(),
            },
            RunError::Io(detail) => ForgeError::Network {
                host: self.host.clone(),
                detail,
            },
        }
    }
}

impl Transport for CliTransport {
    fn post_graphql(&self, body: &[u8]) -> Result<ApiResponse, ForgeError> {
        let _perf = sirio_perf::span("forge.cli_request", 0);
        let args = graphql_args(self.program, &self.host);
        let output = run(self.program, &args, Some(body), CLI_TIMEOUT)
            .map_err(|error| self.run_error(error))?;
        interpret_cli(self.program, &self.host, &output)
    }

    fn request(&self, request: &RestRequest) -> Result<ApiResponse, ForgeError> {
        let _perf = sirio_perf::span("forge.cli_rest", 0);
        let timeout = if request.log { LOG_TIMEOUT } else { CLI_TIMEOUT };
        let mut args = rest_args(self.program, &self.host, request);
        // Leave room for --include's HTTP headers; the parsed body still
        // has the exact download cap. Stop a chatty CLI before it fills memory.
        let limit = request.log.then_some(crate::LOG_DOWNLOAD_LIMIT + 64 * 1024);
        let mut output = run_bounded(self.program, &args, request.body.as_deref(), timeout, limit)
            .map_err(|error| self.run_error(error))?;
        // A gh older than the flag refuses it; a log read is safe to repeat.
        if request.log && String::from_utf8_lossy(&output.stderr).contains("unknown flag: --allow-escape-sequences") {
            args.retain(|arg| *arg != "--allow-escape-sequences");
            output = run_bounded(self.program, &args, request.body.as_deref(), timeout, limit)
                .map_err(|error| self.run_error(error))?;
        }
        let response = interpret_cli(self.program, &self.host, &output)?;
        if request.log && response.body.len() as u64 > crate::LOG_DOWNLOAD_LIMIT {
            return Err(self.run_error(RunError::OutputTooLarge));
        }
        Ok(response)
    }
}

fn rest_args<'a>(program: CliProgram, host: &'a str, request: &'a RestRequest) -> Vec<&'a str> {
    let mut args = vec![
        "api",
        "--hostname",
        host,
        "--include",
        "--method",
        request.method.as_str(),
        request.path.as_str(),
    ];
    if request.log && program == CliProgram::Gh {
        args.push("--allow-escape-sequences");
    }
    if request.body.is_some() {
        args.extend(["--input", "-"]);
        if program == CliProgram::Glab {
            args.extend(["-H", "Content-Type: application/json"]);
        }
    }
    args
}

fn graphql_args<'a>(program: CliProgram, host: &'a str) -> Vec<&'a str> {
    let mut args = vec![
        "api",
        "--hostname",
        host,
        "--include",
        "--method",
        "POST",
        "graphql",
        "--input",
        "-",
    ];
    if program == CliProgram::Glab {
        args.extend(["-H", "Content-Type: application/json"]);
    }
    args
}

pub(crate) struct CliOutput {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub(crate) enum RunError {
    NotInstalled,
    TimedOut,
    Io(String),
    OutputTooLarge,
}

/// Runs a CLI with a deadline, feeding it `stdin`. Output drains on two
/// threads so a chatty child cannot fill a pipe and stall; past the
/// deadline the child is killed.
pub(crate) fn run(
    program: CliProgram,
    args: &[&str],
    stdin: Option<&[u8]>,
    timeout: Duration,
) -> Result<CliOutput, RunError> {
    run_bounded(program, args, stdin, timeout, None)
}

fn run_bounded(
    program: CliProgram,
    args: &[&str],
    stdin: Option<&[u8]>,
    timeout: Duration,
    limit: Option<u64>,
) -> Result<CliOutput, RunError> {
    let exceeded = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut child = Command::new(program.command())
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                RunError::NotInstalled
            } else {
                RunError::Io(error.to_string())
            }
        })?;
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let out_exceeded = exceeded.clone();
    let out_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(limit) = limit {
            let _ = stdout.take(limit + 1).read_to_end(&mut bytes);
            if bytes.len() as u64 > limit {
                out_exceeded.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        } else {
            let _ = stdout.read_to_end(&mut bytes);
        }
        bytes
    });
    let err_exceeded = exceeded.clone();
    let err_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(limit) = limit {
            let _ = stderr.take(limit + 1).read_to_end(&mut bytes);
            if bytes.len() as u64 > limit {
                err_exceeded.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        } else {
            let _ = stderr.read_to_end(&mut bytes);
        }
        bytes
    });
    if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let _ = pipe.write_all(input);
        // `pipe` drops here: the CLI sees the end of its input.
    }
    let deadline = Instant::now() + timeout;
    let status = loop {
        if exceeded.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Err(error) => return Err(RunError::Io(error.to_string())),
        }
    };
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    if exceeded.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(RunError::OutputTooLarge);
    }
    match status {
        Some(status) => Ok(CliOutput {
            code: status.code(),
            stdout,
            stderr,
        }),
        None => Err(RunError::TimedOut),
    }
}

/// Both CLIs print an HTTP error's whole answer under `--include` and exit
/// 1, so output that starts with a status line is an answer whatever the
/// exit code. Otherwise `gh` exits 4 when it holds no credential for the
/// host, and anything else failed to reach it.
fn interpret_cli(
    program: CliProgram,
    host: &str,
    output: &CliOutput,
) -> Result<ApiResponse, ForgeError> {
    if output.stdout.starts_with(b"HTTP/") {
        return parse_included(&output.stdout).ok_or_else(|| ForgeError::UnexpectedResponse {
            host: host.to_string(),
            detail: format!("unreadable `{} --include` output", program.command()),
        });
    }
    if program == CliProgram::Gh && output.code == Some(4) {
        return Err(ForgeError::NotAuthenticated {
            host: host.to_string(),
        });
    }
    Err(ForgeError::Network {
        host: host.to_string(),
        detail: first_line(&output.stderr),
    })
}

/// Splits `--include` output into status, headers and body. Both CLIs end
/// the status line with `\n` and header lines with `\r\n` (gh 2.100, glab
/// 1.119), so lines split on `\n` and lose a trailing `\r`.
fn parse_included(raw: &[u8]) -> Option<ApiResponse> {
    let mut rest = raw;
    let mut lines = Vec::new();
    loop {
        let newline = rest.iter().position(|&byte| byte == b'\n')?;
        let line = &rest[..newline];
        rest = &rest[newline + 1..];
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            break;
        }
        lines.push(String::from_utf8_lossy(line).into_owned());
    }
    let mut lines = lines.into_iter();
    let status = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let headers = lines
        .filter_map(|line| {
            line.split_once(':')
                .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        })
        .collect();
    Some(ApiResponse {
        status,
        headers,
        body: rest.to_vec(),
    })
}

/// glab boxes its errors in blank lines and an `ERROR` banner.
fn first_line(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && *line != "ERROR")
        .unwrap_or("no output")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// glab 1.119 sends `--input` bodies without a `Content-Type`, and
    /// GitLab then reads no query at all and answers `Unexpected end of
    /// document`. Naming the type is what makes the body count.
    #[test]
    fn glab_graphql_names_its_json_content_type() {
        let args = graphql_args(CliProgram::Glab, "code.example.it");
        let header = args.iter().position(|arg| *arg == "-H");
        assert_eq!(
            header.and_then(|at| args.get(at + 1)),
            Some(&"Content-Type: application/json")
        );
    }
}
