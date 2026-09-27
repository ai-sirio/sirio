//! Every way a forge request fails, each carrying what the UI needs to show
//! its remedy (spec §10).

/// Why a forge request failed. No variant has a field that could hold a
/// token, and the transports never put one into a message.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ForgeError {
    /// The means is the CLI, and the CLI is not on PATH.
    #[error("{program} is not on PATH")]
    NotInstalled { program: &'static str },
    /// 401, a CLI signed in to no account on the host, or an anonymous answer.
    #[error("not signed in to {host}")]
    NotAuthenticated { host: String },
    /// A 403 that is not a rate limit: missing scopes, or GitHub SAML SSO.
    #[error("{host} refused the request: {detail}")]
    Forbidden {
        host: String,
        detail: String,
        sso_url: Option<String>,
    },
    /// The project or change request does not exist, or is not visible.
    #[error("not found on {host}")]
    NotFound { host: String },
    /// The forge's rate limit; `reset_at` is Unix seconds when known.
    #[error("rate limited by {host}")]
    RateLimited { host: String, reset_at: Option<i64> },
    /// The TLS handshake failed — typically a corporate CA missing from the
    /// system trust store.
    #[error("TLS failed with {host}: {detail}")]
    Tls { host: String, detail: String },
    /// DNS, connection refused, timeout.
    #[error("cannot reach {host}: {detail}")]
    Network { host: String, detail: String },
    /// The server does not know a field the query names. The GitLab client
    /// answers it with its baseline query; it never reaches a caller.
    #[error("{host} does not know the field {field}")]
    UnknownField { host: String, field: String },
    /// An answer Sirio cannot read.
    #[error("unexpected answer from {host}: {detail}")]
    UnexpectedResponse { host: String, detail: String },
}

impl ForgeError {
    /// The variant's name, as the e2e probe prints it.
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::NotInstalled { .. } => "NotInstalled",
            Self::NotAuthenticated { .. } => "NotAuthenticated",
            Self::Forbidden { .. } => "Forbidden",
            Self::NotFound { .. } => "NotFound",
            Self::RateLimited { .. } => "RateLimited",
            Self::Tls { .. } => "Tls",
            Self::Network { .. } => "Network",
            Self::UnknownField { .. } => "UnknownField",
            Self::UnexpectedResponse { .. } => "UnexpectedResponse",
        }
    }
}
