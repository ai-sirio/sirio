//! Errors produced while preparing a worktree for an agent.

use std::fmt;

/// A failure while writing worktree-local hook configuration.
#[derive(Debug)]
pub enum PrepareError {
    /// A filesystem operation failed (creating the config directory, writing
    /// the file, atomically renaming it).
    Io(std::io::Error),
    /// The existing config file could not be re-serialized after merging.
    Json(serde_json::Error),
    /// The skill markdown Sirio was asked to install carries none of its
    /// own managed-file marker — installing it would leave a file nothing
    /// can later recognize as Sirio's own. Mirrors
    /// `SirioSkillProvisioner.Error.missingMarker` in the Swift app
    /// (F-AGENT-SAFE-01).
    MissingSkillMarker,
    /// `install_skill` was asked to provision an agent id this port does
    /// not know a skill destination for. Mirrors
    /// `SirioSkillProvisioner.Error.unsupportedAgent`.
    UnsupportedSkillAgent(String),
    /// A skill file already exists at the destination and carries no
    /// Sirio managed-file marker — refusing to overwrite it protects a
    /// user's own hand-authored file of the same name. Mirrors
    /// `SirioSkillProvisioner.Error.unmanagedFile` (F-AGENT-SAFE-01).
    UnmanagedSkillFile(std::path::PathBuf),
    /// The user's own config file (Settings → Install Hooks) no longer
    /// parses, so setting one key would mean rewriting it from whatever
    /// could be salvaged — it is left exactly as found instead.
    UnparseableUserConfig {
        path: std::path::PathBuf,
        reason: String,
    },
}

impl fmt::Display for PrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrepareError::Io(error) => write!(f, "failed to write agent config: {error}"),
            PrepareError::Json(error) => write!(f, "failed to serialize agent config: {error}"),
            PrepareError::UnparseableUserConfig { path, reason } => {
                write!(
                    f,
                    "refusing to rewrite unparseable config at {}: {reason}",
                    path.display()
                )
            }
            PrepareError::MissingSkillMarker => {
                write!(f, "Sirio skill content is missing its managed-file marker")
            }
            PrepareError::UnsupportedSkillAgent(id) => write!(f, "unsupported Sirio agent: {id}"),
            PrepareError::UnmanagedSkillFile(path) => {
                write!(
                    f,
                    "refusing to overwrite unmanaged skill at {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for PrepareError {}

impl From<std::io::Error> for PrepareError {
    fn from(error: std::io::Error) -> Self {
        PrepareError::Io(error)
    }
}

impl From<serde_json::Error> for PrepareError {
    fn from(error: serde_json::Error) -> Self {
        PrepareError::Json(error)
    }
}
