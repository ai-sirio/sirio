//! Sending one GraphQL request to a forge and returning its HTTP answer.
//!
//! Two implementations of [`Transport`], chosen by the user's means (spec
//! §5): [`TokenTransport`] speaks HTTP itself, and [`CliTransport`] (Task 4)
//! hands the request to `gh api` / `glab api`, which own authentication.
//! Neither reads the answer — `graphql::execute` does that once for both.

use std::time::Duration;

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

/// Sends a GraphQL request body — `{"operationName", "query", "variables"}`
/// as JSON — to one forge host.
pub trait Transport: Send + Sync {
    fn post_graphql(&self, body: &[u8]) -> Result<ApiResponse, ForgeError>;
}

const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

/// HTTP with a personal access token, sent as `Authorization: Bearer`,
/// which both forges accept for one.
pub struct TokenTransport {
    endpoint: String,
    host: String,
    token: String,
    agent: ureq::Agent,
}

impl TokenTransport {
    pub fn new(forge: Forge, host: &str, token: String) -> Self {
        Self {
            endpoint: graphql_endpoint(forge, host),
            host: host.to_string(),
            token,
            agent: http_agent(HTTP_TIMEOUT),
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
        let body = response
            .into_body()
            .read_to_vec()
            .map_err(|error| classify(&self.host, error))?;
        Ok(ApiResponse { status, headers, body })
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

fn classify(host: &str, error: ureq::Error) -> ForgeError {
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
