//! The control socket: how external tools drive a running Tiller, and how
//! the agent hooks report status back. Ported from the Swift
//! `TillerControl` package.
//!
//! The server listens on a unix socket — `$TILLER_SOCKET` when set, otherwise
//! `$XDG_RUNTIME_DIR/TillerRust/control.sock`, falling back to the XDG state
//! directory when no runtime directory exists (see [`protocol`]). The Swift
//! original defaulted to `~/Library/Application Support/Tiller/control.sock`;
//! that path is history, not current behaviour. It dispatches line-delimited
//! JSON requests
//! ([`protocol::ControlRequest`]) to a [`server::ControlHandler`] the app
//! implements — this crate carries the transport and the protocol and knows
//! nothing about GPUI.
//!
//! `tillerctl` is the CLI client that speaks the same protocol with the same
//! subcommand names as the Swift CLI: `ping`, `capabilities`, `identify`,
//! `list-workspaces`, `new-workspace`, `select-workspace`,
//! `current-workspace`, `close-workspace`, `notify`, `session-ref`,
//! `list-notifications`, `clear-notifications`.

/// The version reported by the Tiller app and `tillerctl`.
///
/// This is the single runtime version source and reads the `[workspace.package]`
/// `version` field through this crate's `version.workspace = true` declaration.
/// That declaration matters because this crate ships beside `tiller`; moving
/// this constant to a crate with its own version would silently change what it
/// reports.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Formats the human-readable output for `tillerctl version`.
pub fn format_version_lines(cli_version: &str, app_version: Option<&str>) -> Vec<String> {
    let mut lines = vec![format!("tillerctl {cli_version}")];
    if let Some(app_version) = app_version {
        lines.push(format!("running Tiller {app_version}"));
        if app_version != cli_version {
            lines.push(format!(
                "warning: tillerctl {cli_version} does not match the running Tiller {app_version}"
            ));
        }
    }
    lines
}

/// Formats the machine-readable output for `tillerctl version`.
pub fn format_version_json(cli_version: &str, app_version: Option<&str>) -> String {
    serde_json::json!({
        "cliVersion": cli_version,
        "appVersion": app_version,
        "matches": app_version == Some(cli_version),
    })
    .to_string()
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

    #[test]
    fn version_is_a_non_empty_semantic_version() {
        assert!(!VERSION.is_empty());
        let components: Vec<_> = VERSION.split('.').collect();
        assert_eq!(components.len(), 3);
        assert!(components.iter().all(|component| {
            !component.is_empty() && component.parse::<u64>().is_ok()
        }));
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
                "warning: tillerctl {cli_version} does not match the running Tiller {app_version}"
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
