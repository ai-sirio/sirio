//! Local branch listing.

use std::path::Path;

use crate::GitError;
use crate::git;

/// Namespace for branch operations.
pub struct GitBranches;

impl GitBranches {
    /// Lists local branch names using `git branch --list`, preserving spaces
    /// inside a branch name instead of splitting the output into words.
    pub fn list(repo: &Path) -> Result<Vec<String>, GitError> {
        let output = git::run_accepting(
            &["branch", "--list", "--format=%(refname:short)"],
            repo,
            &[0],
        )?;
        Ok(Self::parse(&output.stdout_string()))
    }

    /// Parses one branch name per line. This is deliberately line-based:
    /// branch names are opaque strings and may contain internal whitespace.
    pub fn parse(output: &str) -> Vec<String> {
        output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }
}

/// Convenience function for callers that prefer free functions.
pub fn list_branches(repo: &Path) -> Result<Vec<String>, GitError> {
    GitBranches::list(repo)
}
