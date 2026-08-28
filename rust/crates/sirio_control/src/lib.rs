//! The control socket: how external tools drive a running Sirio, and how
//! the agent hooks report status back. Ported from the Swift
//! `SirioControl` package.
//!
//! The server listens on a unix socket — `$TILLER_SOCKET` when set, otherwise
//! `$XDG_RUNTIME_DIR/TillerRust/control.sock`, falling back to the XDG state
//! directory when no runtime directory exists (see [`protocol`]). The Swift
//! original defaulted to `~/Library/Application Support/Sirio/control.sock`;
//! that path is history, not current behaviour. It dispatches line-delimited
//! JSON requests
//! ([`protocol::ControlRequest`]) to a [`server::ControlHandler`] the app
//! implements — this crate carries the transport and the protocol and knows
//! nothing about GPUI.
//!
//! `sirioctl` is the CLI client that speaks the same protocol with the same
//! subcommand names as the Swift CLI: `ping`, `capabilities`, `identify`,
//! `list-workspaces`, `new-workspace`, `select-workspace`,
//! `current-workspace`, `close-workspace`, `notify`, `session-ref`,
//! `list-notifications`, `clear-notifications`.

/// The version reported by the Sirio app and `sirioctl`.
///
/// This is the single runtime version source and reads the `[workspace.package]`
/// `version` field through this crate's `version.workspace = true` declaration.
/// That declaration matters because this crate ships beside `sirio`; moving
/// this constant to a crate with its own version would silently change what it
/// reports.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Formats the human-readable output for `sirioctl version`.
pub fn format_version_lines(cli_version: &str, app_version: Option<&str>) -> Vec<String> {
    let mut lines = vec![format!("sirioctl {cli_version}")];
    if let Some(app_version) = app_version {
        lines.push(format!("running Sirio {app_version}"));
        if app_version != cli_version {
            lines.push(format!(
                "warning: sirioctl {cli_version} does not match the running Sirio {app_version}"
            ));
        }
    }
    lines
}

/// Formats the machine-readable output for `sirioctl version`.
pub fn format_version_json(cli_version: &str, app_version: Option<&str>) -> String {
    serde_json::json!({
        "cliVersion": cli_version,
        "appVersion": app_version,
        "matches": app_version == Some(cli_version),
    })
    .to_string()
}

/// Returns the address users can act on for the control transport.
pub fn display_endpoint(path: &std::path::Path) -> String {
    #[cfg(windows)]
    {
        display_endpoint_from(path, windows_pipe::pipe_name_for_path(path))
    }

    #[cfg(not(windows))]
    {
        path.display().to_string()
    }
}

#[cfg(windows)]
fn display_endpoint_from(path: &std::path::Path, endpoint: Result<String, String>) -> String {
    endpoint.unwrap_or_else(|_| path.display().to_string())
}

pub mod client;
pub mod extract;
pub mod panel;
pub mod protocol;
pub mod server;

// The Windows named-pipe transport. Compiled only on Windows; on unix the
// crate never sees it, keeping the unix build byte-identical.
#[cfg(windows)]
pub mod windows_pipe;

pub use client::{ClientError, round_trip};
pub use extract::{session_ref_from_json, session_ref_from_payload_arguments};
pub use panel::{
    PaneError, PaneExitStatus, PaneInfo, PaneRegistry, PaneStateSnapshot, base64_encode,
    terminal_key_bytes,
};
pub use protocol::{
    ControlRequest, ControlResponse, decode_request, decode_response, default_socket_path,
    encode_line,
};
pub use server::{ControlHandler, ControlServer, ServerError};

#[cfg(test)]
mod tests {
    use super::{VERSION, format_version_json, format_version_lines};
    use std::path::Path;

    #[cfg(unix)]
    #[test]
    fn display_endpoint_on_unix_is_the_path() {
        let path = Path::new("/run/user/1000/TillerRust/control.sock");
        assert_eq!(super::display_endpoint(path), path.display().to_string());
    }

    #[cfg(windows)]
    #[test]
    fn display_endpoint_on_windows_is_the_pipe_name() {
        let endpoint = super::display_endpoint(Path::new(
            r"C:\Users\alice\AppData\Local\TillerRust\control.sock",
        ));
        assert!(endpoint.starts_with(r"\\.\pipe\"), "{endpoint}");
    }

    #[cfg(windows)]
    #[test]
    fn display_endpoint_preserves_an_explicit_pipe_name() {
        let path = Path::new(r"\\.\pipe\custom");
        assert_eq!(super::display_endpoint(path), path.display().to_string());
    }

    #[cfg(windows)]
    #[test]
    fn display_endpoint_falls_back_to_the_path_when_pipe_derivation_fails() {
        let path = Path::new(r"C:\Users\alice\control.sock");
        assert_eq!(
            super::display_endpoint_from(path, Err("test failure".to_string())),
            path.display().to_string()
        );
    }

    #[test]
    fn version_is_a_non_empty_semantic_version() {
        assert!(!VERSION.is_empty());
        let components: Vec<_> = VERSION.split('.').collect();
        assert_eq!(components.len(), 3);
        assert!(
            components
                .iter()
                .all(|component| { !component.is_empty() && component.parse::<u64>().is_ok() })
        );
    }

    #[test]
    fn version_output_without_a_running_app_contains_the_cli_version() {
        let lines = format_version_lines(VERSION, None);
        assert!(lines.iter().any(|line| line.contains(VERSION)));
    }

    #[test]
    fn version_output_calls_out_a_mismatched_running_app() {
        let cli_version = "cli-version";
        let app_version = "app-version";
        let lines = format_version_lines(cli_version, Some(app_version));

        assert!(lines.iter().any(|line| {
            line == &format!(
                "warning: sirioctl {cli_version} does not match the running Sirio {app_version}"
            )
        }));
    }

    #[test]
    fn version_output_omits_the_warning_for_a_matching_pair() {
        let lines = format_version_lines("cli-version", Some("cli-version"));
        assert!(lines.iter().all(|line| !line.starts_with("warning:")));
    }

    #[test]
    fn version_json_contains_both_versions_and_the_match_boolean() {
        let output = format_version_json("cli-version", Some("app-version"));
        let value: serde_json::Value = serde_json::from_str(&output).expect("version JSON");

        assert_eq!(value["cliVersion"], "cli-version");
        assert_eq!(value["appVersion"], "app-version");
        assert_eq!(value["matches"], false);
    }
}
