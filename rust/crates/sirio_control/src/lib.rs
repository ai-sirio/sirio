//! The control socket: how external tools drive a running Sirio, and how
//! the agent hooks report status back. Ported from the Swift
//! `SirioControl` package.
//!
//! The server listens on a unix socket — `$SIRIO_SOCKET` when set, otherwise
//! `$XDG_RUNTIME_DIR/Sirio/control.sock`, falling back to the XDG state
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

/// Which release channel a binary belongs to, compiled in at build time.
///
/// `Stable` is what a user gets, built from a tag; `Nightly` is built from
/// `main` on a schedule; `Dev` is not a channel at all — it is what every
/// build gets when the release job did not pass `SIRIO_RELEASE_CHANNEL`,
/// and it refuses to update rather than pretending to belong somewhere
/// (spec §3.3). Runtime discovery was rejected on purpose: a channel read
/// from a settings file is a channel the user can edit.
///
/// This is the contract the updater (#311) and the nightly release
/// pipeline (#317) build on: they read [`ReleaseChannel::RELEASE_CHANNEL`]
/// — or pass `SIRIO_RELEASE_CHANNEL=stable|nightly` — and gate the whole
/// update path on [`ReleaseChannel::updates_enabled`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseChannel {
    Stable,
    Nightly,
    Dev,
}

impl ReleaseChannel {
    /// Resolves the channel from the value the release job passes through
    /// `SIRIO_RELEASE_CHANNEL`. `None` (env unset) and any unknown value
    /// both resolve to [`ReleaseChannel::Dev`] — the safe default is the
    /// one that never updates.
    pub const fn from_env_value(value: Option<&str>) -> Self {
        // Byte matching because `str` patterns are not allowed in const fns
        // on stable; `as_bytes` is const-stable.
        let bytes: &[u8] = match value {
            Some(value) => value.as_bytes(),
            None => &[],
        };
        match bytes {
            b"stable" => Self::Stable,
            b"nightly" => Self::Nightly,
            _ => Self::Dev,
        }
    }

    /// The compile-time channel of the binary this constant is compiled
    /// into. Defined here and only here so every reader — the app,
    /// `sirioctl`, the updater — sees one value: an `option_env!` re-read
    /// in a second crate could go stale across cached builds.
    pub const RELEASE_CHANNEL: Self = Self::from_env_value(option_env!("SIRIO_RELEASE_CHANNEL"));

    /// Lowercase wire form, used by `system.capabilities` and Settings.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Nightly => "nightly",
            Self::Dev => "dev",
        }
    }

    /// Whether the update path may run at all. Only a real channel
    /// updates; a dev build refuses (spec §3.3: "a dev build is not a
    /// channel").
    pub const fn updates_enabled(self) -> bool {
        !matches!(self, Self::Dev)
    }
}

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

/// Formats the machine-readable output for `sirioctl version`. `channel` is
/// the compiled release channel: the release job asks the shipped binary for
/// it (Scripts/assert-built-channel.sh) rather than trusting the test build.
pub fn format_version_json(cli_version: &str, app_version: Option<&str>) -> String {
    serde_json::json!({
        "cliVersion": cli_version,
        "appVersion": app_version,
        "matches": app_version == Some(cli_version),
        "channel": ReleaseChannel::RELEASE_CHANNEL.as_str(),
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
    PaneError, PaneExitStatus, PaneInfo, PaneRegistry, PaneStateSnapshot, ScrollbackSource,
    base64_encode, terminal_key_bytes,
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
        let path = Path::new("/run/user/1000/Sirio/control.sock");
        assert_eq!(super::display_endpoint(path), path.display().to_string());
    }

    #[cfg(windows)]
    #[test]
    fn display_endpoint_on_windows_is_the_pipe_name() {
        let endpoint = super::display_endpoint(Path::new(
            r"C:\Users\alice\AppData\Local\Sirio\control.sock",
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

    use super::ReleaseChannel;

    #[test]
    fn env_value_selects_the_compiled_channel() {
        assert_eq!(
            ReleaseChannel::from_env_value(Some("stable")),
            ReleaseChannel::Stable
        );
        assert_eq!(
            ReleaseChannel::from_env_value(Some("nightly")),
            ReleaseChannel::Nightly
        );
    }

    #[test]
    fn unset_or_unknown_env_value_falls_back_to_dev() {
        assert_eq!(ReleaseChannel::from_env_value(None), ReleaseChannel::Dev);
        assert_eq!(
            ReleaseChannel::from_env_value(Some("")),
            ReleaseChannel::Dev
        );
        assert_eq!(
            ReleaseChannel::from_env_value(Some("preview")),
            ReleaseChannel::Dev
        );
        assert_eq!(
            ReleaseChannel::from_env_value(Some("STABLE")),
            ReleaseChannel::Dev
        );
    }

    #[test]
    fn a_dev_build_refuses_updates_and_a_real_channel_does_not() {
        assert!(!ReleaseChannel::Dev.updates_enabled());
        assert!(ReleaseChannel::Stable.updates_enabled());
        assert!(ReleaseChannel::Nightly.updates_enabled());
    }

    #[test]
    fn channel_wire_form_round_trips_through_the_env_value() {
        for channel in [
            ReleaseChannel::Stable,
            ReleaseChannel::Nightly,
            ReleaseChannel::Dev,
        ] {
            assert_eq!(
                ReleaseChannel::from_env_value(Some(channel.as_str())),
                channel
            );
        }
    }

    #[test]
    fn the_compiled_channel_is_the_env_value_the_build_saw() {
        assert_eq!(
            ReleaseChannel::RELEASE_CHANNEL,
            ReleaseChannel::from_env_value(option_env!("SIRIO_RELEASE_CHANNEL"))
        );
    }

    /// Spec §3.5 (the Zed post-mortem): a release whose channel silently
    /// fell back to the dev default would update never and look exactly
    /// like the bug that started this. The release job compiles with
    /// `SIRIO_RELEASE_CHANNEL=stable` (tag builds) or `=nightly` (the
    /// scheduled build, #317) and runs this suite, so the gate asserts both
    /// halves: the compiled constant really is that channel and updating is
    /// enabled. Outside those jobs the test is a no-op — local builds are
    /// dev builds by design.
    ///
    /// Cargo records `option_env!` reads in dep-info, so a changed value
    /// recompiles this crate rather than reusing a stale dev-compiled rlib.
    /// This test still runs on the gate's build, not on the artifact: the
    /// release job also asks the shipped `sirioctl version --json` for its
    /// channel (Scripts/assert-built-channel.sh), so the two halves of §3.5
    /// are asserted on the binary that ships as well as here.
    #[test]
    fn release_gate_a_release_build_carries_its_channel_with_updates_enabled() {
        let expected = match std::env::var("SIRIO_RELEASE_CHANNEL").as_deref() {
            Ok("stable") => ReleaseChannel::Stable,
            Ok("nightly") => ReleaseChannel::Nightly,
            _ => return,
        };
        assert_eq!(
            ReleaseChannel::RELEASE_CHANNEL,
            expected,
            "SIRIO_RELEASE_CHANNEL={} did not reach the compiled channel constant",
            expected.as_str()
        );
        assert!(expected.updates_enabled());
        assert!(ReleaseChannel::RELEASE_CHANNEL.updates_enabled());
    }

    #[test]
    fn version_json_contains_both_versions_and_the_match_boolean() {
        let output = format_version_json("cli-version", Some("app-version"));
        let value: serde_json::Value = serde_json::from_str(&output).expect("version JSON");

        assert_eq!(value["cliVersion"], "cli-version");
        assert_eq!(value["appVersion"], "app-version");
        assert_eq!(value["matches"], false);
    }

    /// The release job asserts the channel on the *shipped* binary, not
    /// only on the test build (spec §3.5): `sirioctl version --json` is the
    /// one place a compiled artifact reports what it was compiled as.
    #[test]
    fn version_json_reports_the_compiled_channel() {
        let output = format_version_json("cli-version", None);
        let value: serde_json::Value = serde_json::from_str(&output).expect("version JSON");

        assert_eq!(value["channel"], ReleaseChannel::RELEASE_CHANNEL.as_str());
        assert_eq!(value["appVersion"], serde_json::Value::Null);
    }
}
