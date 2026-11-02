//! Finding OpenCode's background service: its registration file.
//!
//! The service (`opencode serve --service`) writes `{id, version, url, pid, password}`
//! to `$XDG_STATE_HOME/opencode/service.json` (`<home>/.local/state/opencode/service.json`
//! when that variable is unset; the same on every platform, measured on Linux and read
//! from OpenCode's code for macOS), mode 600. That file is where `opencode` itself finds
//! the service, so it is where Sirio looks: no port to guess and no password in an
//! environment variable (the service ignores them).
//!
//! `discover` only reads that file. It makes no network call, so it can run on any
//! thread. Whether the service behind it is alive, and the same one, is asked of
//! `/api/info` by the watch (see `stream::Client::confirm`).

use std::fmt;
use std::path::Path;

/// Where to send requests, and how to authenticate them.
#[derive(Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// `http://127.0.0.1:<port>`, no trailing slash.
    pub base_url: String,
    /// The Basic credentials: user `opencode` and the registration's password.
    pub auth: Option<(String, String)>,
    /// The service pid the registration names; `/api/info` must answer the same one.
    pub pid: u32,
}

// The password is a secret: it never reaches a log through `{:?}`.
impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("base_url", &self.base_url)
            .field("auth", &self.auth.as_ref().map(|_| "<redacted>"))
            .field("pid", &self.pid)
            .finish()
    }
}

/// Why there is no endpoint. Each is a kind name for the log, never content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointError {
    /// No registration file: OpenCode 1.x, or no service has run.
    NoService,
    /// The file is there and cannot be read or is not what the service writes.
    Unreadable,
    /// The registration points somewhere that is not this machine's service, or the
    /// service that answers is not the one registered.
    Mismatch,
}

impl EndpointError {
    pub fn kind(self) -> &'static str {
        match self {
            EndpointError::NoService => "no_service",
            EndpointError::Unreadable => "unreadable",
            EndpointError::Mismatch => "mismatch",
        }
    }
}

#[derive(serde::Deserialize)]
struct Registration {
    url: String,
    pid: u32,
    #[serde(default)]
    password: String,
}

/// Reads the registration of the running service. `env` is the process environment
/// (`XDG_STATE_HOME`); `home` the user's home directory.
pub fn discover(
    home: &Path,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Endpoint, EndpointError> {
    let state = env("XDG_STATE_HOME")
        .filter(|dir| Path::new(dir).is_absolute())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".local").join("state"));
    let file = state.join("opencode").join("service.json");
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(EndpointError::NoService);
        }
        Err(_) => return Err(EndpointError::Unreadable),
    };
    let registration: Registration =
        serde_json::from_str(&text).map_err(|_| EndpointError::Unreadable)?;
    let base_url = registration.url.trim_end_matches('/').to_string();
    // The credentials go only to a service on this machine.
    if !is_loopback_http(&base_url) {
        return Err(EndpointError::Mismatch);
    }
    let auth = (!registration.password.is_empty())
        .then(|| ("opencode".to_string(), registration.password));
    Ok(Endpoint {
        base_url,
        auth,
        pid: registration.pid,
    })
}

/// `http://127.0.0.1:<port>`, `http://localhost:<port>` or `http://[::1]:<port>`.
fn is_loopback_http(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or_default();
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or_default()
    } else {
        authority
            .rsplit_once(':')
            .map_or(authority, |(host, _)| host)
    };
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}
