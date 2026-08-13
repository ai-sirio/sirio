//! Errors produced by the git layer.

use std::fmt;
use std::path::PathBuf;

/// A failure while shelling out to `git`.
///
/// Mirrors `GitError` from the Swift app's `TillerGit` package: a non-accepted
/// exit code carries the process's stderr so the UI can surface git's own
/// message instead of inventing one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitError {
    /// `git` could not be spawned at all (binary missing, permissions, ...).
    Spawn {
        /// The underlying OS error message.
        message: String,
    },
    /// `git` exited with a status outside the caller's accepted set.
    CommandFailed {
        /// The process exit code (`128` is git's "fatal" convention).
        code: i32,
        /// Everything git wrote to stderr.
        stderr: String,
    },
    /// Staging was refused because one or more requested paths are
    /// unmerged. The paths are included so the caller can resolve them or
    /// choose a different mutation without discovering conflicts one at a
    /// time.
    ConflictedPaths {
        /// Repo-relative conflicted paths that blocked staging.
        paths: Vec<PathBuf>,
    },
    /// The process succeeded but its output could not be parsed. This is a
    /// bug in this crate's parser, not in git — surfaced so callers can
    /// distinguish it from a git failure.
    InvalidOutput {
        /// What failed to parse.
        message: String,
    },
    /// The git process did not exit within its wall-clock budget and was
    /// killed. Distinct from [`GitError::CommandFailed`] (git ran and
    /// failed), so a caller can tell a hung process apart and decide whether
    /// to retry.
    TimedOut {
        /// The command that was killed, args joined for diagnostics.
        command: String,
        /// The budget that was exceeded.
        timeout: std::time::Duration,
    },
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitError::Spawn { message } => {
                write!(f, "failed to spawn git: {message}")
            }
            GitError::CommandFailed { code, stderr } => {
                let stderr = stderr.trim();
                if stderr.is_empty() {
                    write!(f, "git exited with status {code}")
                } else {
                    write!(f, "git exited with status {code}: {stderr}")
                }
            }
            GitError::ConflictedPaths { paths } => {
                let noun = if paths.len() == 1 { "path" } else { "paths" };
                let paths = paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "refusing to stage conflicted {noun}: {paths}")
            }
            GitError::InvalidOutput { message } => {
                write!(f, "failed to parse git output: {message}")
            }
            GitError::TimedOut { command, timeout } => {
                write!(
                    f,
                    "git {command} did not finish within {timeout:?} and was killed"
                )
            }
        }
    }
}

impl std::error::Error for GitError {}
