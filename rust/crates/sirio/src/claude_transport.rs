//! Which transport a Claude chat opens on, and why.
//!
//! Native when the `claude` on PATH is new enough for the protocol this
//! build speaks; the ACP wrapper otherwise. The fallback is automatic —
//! there is no setting — so the reason has to be legible: every `Wrapper`
//! carries one, and Settings → Agents renders it as a sentence.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use sirio_claude::{ClaudeVersion, MIN_CLAUDE_VERSION};

/// How long `claude --version` gets. It is a local exec that prints one
/// line; a CLI that cannot manage it in this window is one whose version
/// we do not know, which resolves to the wrapper either way.
const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Why a Claude chat is on the wrapper rather than the binary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WrapperReason {
    /// No `claude` on PATH.
    NotOnPath,
    /// It is older than the protocol this build speaks.
    VersionBelowFloor { found: String },
    /// It ran but printed nothing a version could be read from.
    VersionUnreadable,
    /// `SIRIO_CLAUDE_TRANSPORT=acp`.
    Forced,
}

/// The resolved transport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Resolution {
    Native { program: PathBuf, version: String },
    Wrapper { reason: WrapperReason },
}

impl Default for Resolution {
    fn default() -> Self {
        Self::Wrapper {
            reason: WrapperReason::NotOnPath,
        }
    }
}

impl Resolution {
    /// One sentence for the Agents row. It names what was read, never a
    /// guess: an unreadable version says exactly that.
    #[must_use]
    pub fn note(&self) -> String {
        match self {
            Self::Native { version, .. } => format!("Native · claude {version}"),
            Self::Wrapper { reason } => match reason {
                WrapperReason::NotOnPath => "claude is not on PATH — using the ACP wrapper".into(),
                WrapperReason::VersionBelowFloor { found } => format!(
                    "claude {found} is older than {MIN_CLAUDE_VERSION} — using the ACP wrapper"
                ),
                WrapperReason::VersionUnreadable => {
                    "claude did not report a version — using the ACP wrapper".into()
                }
                WrapperReason::Forced => {
                    "SIRIO_CLAUDE_TRANSPORT=acp — using the ACP wrapper".into()
                }
            },
        }
    }

    /// The native program, when that is what resolved.
    #[must_use]
    pub fn native_program(&self) -> Option<&std::path::Path> {
        match self {
            Self::Native { program, .. } => Some(program),
            Self::Wrapper { .. } => None,
        }
    }
}

/// The facts resolution is computed from, injected so it is testable
/// without a `claude` on the machine running the tests.
pub struct Inputs {
    pub executable: Option<PathBuf>,
    pub version_output: Option<String>,
    pub forced_acp: bool,
}

/// Decides the transport. Pure.
#[must_use]
pub fn resolve(inputs: Inputs) -> Resolution {
    if inputs.forced_acp {
        return Resolution::Wrapper {
            reason: WrapperReason::Forced,
        };
    }
    let Some(program) = inputs.executable else {
        return Resolution::Wrapper {
            reason: WrapperReason::NotOnPath,
        };
    };
    let Some(version) = inputs
        .version_output
        .as_deref()
        .and_then(ClaudeVersion::parse)
    else {
        return Resolution::Wrapper {
            reason: WrapperReason::VersionUnreadable,
        };
    };
    if !version.meets_floor() {
        return Resolution::Wrapper {
            reason: WrapperReason::VersionBelowFloor {
                found: version.to_string(),
            },
        };
    }
    Resolution::Native {
        program,
        version: version.to_string(),
    }
}

/// Runs `claude --version`, bounded. Blocking: callers run it on the
/// background executor, beside the rest of `compute_launch_sources`.
#[must_use]
pub fn probe_version(program: &std::path::Path) -> Option<String> {
    let mut child = std::process::Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + VERSION_PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
    let output = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recent_claude_on_path_resolves_to_the_native_transport() {
        let resolution = resolve(Inputs {
            executable: Some("/usr/local/bin/claude".into()),
            version_output: Some("2.1.273 (Claude Code)".into()),
            forced_acp: false,
        });
        assert_eq!(
            resolution,
            Resolution::Native {
                program: "/usr/local/bin/claude".into(),
                version: "2.1.273".into()
            }
        );
        assert_eq!(resolution.note(), "Native · claude 2.1.273");
    }

    #[test]
    fn an_old_claude_falls_back_and_names_both_versions() {
        let resolution = resolve(Inputs {
            executable: Some("/usr/local/bin/claude".into()),
            version_output: Some("2.1.200 (Claude Code)".into()),
            forced_acp: false,
        });
        assert_eq!(
            resolution,
            Resolution::Wrapper {
                reason: WrapperReason::VersionBelowFloor {
                    found: "2.1.200".into()
                }
            }
        );
        assert_eq!(
            resolution.note(),
            "claude 2.1.200 is older than 2.1.257 — using the ACP wrapper"
        );
    }

    #[test]
    fn a_missing_claude_falls_back_and_says_which_binary_is_missing() {
        let resolution = resolve(Inputs {
            executable: None,
            version_output: None,
            forced_acp: false,
        });
        assert_eq!(
            resolution,
            Resolution::Wrapper {
                reason: WrapperReason::NotOnPath
            }
        );
        assert_eq!(
            resolution.note(),
            "claude is not on PATH — using the ACP wrapper"
        );
    }

    #[test]
    fn a_claude_whose_version_cannot_be_read_is_not_assumed_new_enough() {
        // Refusing is the honest answer: the fallback works, and guessing
        // would put an unknown CLI on an undocumented protocol.
        let resolution = resolve(Inputs {
            executable: Some("/usr/local/bin/claude".into()),
            version_output: Some("something went wrong".into()),
            forced_acp: false,
        });
        assert_eq!(
            resolution,
            Resolution::Wrapper {
                reason: WrapperReason::VersionUnreadable
            }
        );
        assert_eq!(
            resolution.note(),
            "claude did not report a version — using the ACP wrapper"
        );
    }

    #[test]
    fn the_diagnostic_valve_forces_the_wrapper_without_probing() {
        let resolution = resolve(Inputs {
            executable: Some("/usr/local/bin/claude".into()),
            version_output: Some("2.1.273 (Claude Code)".into()),
            forced_acp: true,
        });
        assert_eq!(
            resolution,
            Resolution::Wrapper {
                reason: WrapperReason::Forced
            }
        );
        assert_eq!(
            resolution.note(),
            "SIRIO_CLAUDE_TRANSPORT=acp — using the ACP wrapper"
        );
    }
}
