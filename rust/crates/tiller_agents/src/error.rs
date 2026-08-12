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
}

impl fmt::Display for PrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrepareError::Io(error) => write!(f, "failed to write worktree config: {error}"),
            PrepareError::Json(error) => write!(f, "failed to serialize worktree config: {error}"),
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
