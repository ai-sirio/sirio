//! Git remote URL parsing.

use std::path::Path;

use crate::git;

/// Namespace for remote metadata helpers.
pub struct GitRemote;

impl GitRemote {
    /// Returns the owner of the `origin` GitHub remote, or `None` when the
    /// remote is absent, non-GitHub, or malformed.
    pub fn github_owner(repo: &Path) -> Option<String> {
        let output = git::run_accepting(&["remote", "get-url", "origin"], repo, &[0]).ok()?;
        Self::github_owner_from_url(&output.stdout_string())
    }

    /// Parses both scp-like SSH (`git@github.com:owner/project.git`) and URL
    /// forms (`https://github.com/owner/project.git`, including ssh URLs).
    pub fn github_owner_from_url(url: &str) -> Option<String> {
        let url = url.trim();
        let (authority, path) = if let Some(rest) = url.strip_prefix("git@") {
            let (host, path) = rest.split_once(':')?;
            (host, path)
        } else if let Some((_, rest)) = url.split_once("://") {
            let slash = rest.find('/').unwrap_or(rest.len());
            let (authority, path) = rest.split_at(slash);
            (
                authority
                    .rsplit_once('@')
                    .map_or(authority, |(_, host)| host),
                path,
            )
        } else {
            return None;
        };

        let host = authority
            .split(':')
            .next()
            .unwrap_or(authority)
            .trim_end_matches('/');
        if !host.eq_ignore_ascii_case("github.com") {
            return None;
        }
        path.trim_matches('/')
            .split('/')
            .next()
            .filter(|owner| !owner.is_empty())
            .map(ToOwned::to_owned)
    }

    /// Derives the project folder name from a clone source: an HTTPS or SSH
    /// URL, or a local path. Trailing separators and a trailing `.git`
    /// suffix are removed before the final segment is selected.
    ///
    /// The backslash counts as a separator so a native Windows path pasted
    /// into the Clone field — `C:\src\widgets`, which `git clone` accepts —
    /// yields `widgets` rather than the whole tail after the drive colon.
    /// It costs nothing elsewhere: a backslash never separates segments in
    /// an HTTP URL or an scp-style remote.
    pub fn project_name(url: &str) -> String {
        let mut trimmed = url.trim().to_string();
        loop {
            if trimmed.ends_with('/') || trimmed.ends_with('\\') {
                trimmed.pop();
            } else if trimmed.ends_with(".git") {
                trimmed.truncate(trimmed.len() - 4);
            } else {
                break;
            }
        }
        trimmed
            .rsplit(['/', ':', '\\'])
            .next()
            .unwrap_or_default()
            .to_string()
    }
}

/// Free-function spelling for remote owner lookup.
pub fn github_owner(repo: &Path) -> Option<String> {
    GitRemote::github_owner(repo)
}

/// Free-function spelling for clone URL project naming.
pub fn project_name(url: &str) -> String {
    GitRemote::project_name(url)
}
