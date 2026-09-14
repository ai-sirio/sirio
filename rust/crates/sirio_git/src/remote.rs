//! Git remote URL parsing.

use std::path::Path;

use crate::git;

/// Namespace for remote metadata helpers.
pub struct GitRemote;

impl GitRemote {
    /// The raw `origin` remote URL, or `None` when there is no origin.
    ///
    /// Reads the configured value directly rather than going through
    /// `git remote get-url`, which rewrites the URL through any
    /// `url.<base>.insteadOf` mapping in the user's config — a rewrite that
    /// would hand callers a URL the remote was never configured with.
    pub fn origin_url(repo: &Path) -> Option<String> {
        let output =
            git::run_accepting(&["config", "--get", "remote.origin.url"], repo, &[0]).ok()?;
        let url = output.stdout_string().trim().to_owned();
        (!url.is_empty()).then_some(url)
    }

    /// Returns the owner of the `origin` GitHub remote, or `None` when the
    /// remote is absent, non-GitHub, or malformed.
    pub fn github_owner(repo: &Path) -> Option<String> {
        Self::github_owner_from_url(&Self::origin_url(repo)?)
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

/// Free-function spelling for the origin URL.
pub fn origin_url(repo: &Path) -> Option<String> {
    GitRemote::origin_url(repo)
}

/// Free-function spelling for remote owner lookup.
pub fn github_owner(repo: &Path) -> Option<String> {
    GitRemote::github_owner(repo)
}

/// Free-function spelling for clone URL project naming.
pub fn project_name(url: &str) -> String {
    GitRemote::project_name(url)
}

/// The full commit id HEAD resolves to, or `None` when HEAD is unborn (a
/// repository created with `git init` and nothing committed).
pub fn head_sha(repo: &Path) -> Option<String> {
    let output = git::run_accepting(&["rev-parse", "--verify", "--quiet", "HEAD"], repo, &[0, 1])
        .ok()?;
    let sha = output.stdout_string().trim().to_owned();
    (!sha.is_empty()).then_some(sha)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn init_repo(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sirio-remote-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create repo dir");
        Command::new("git").arg("init").current_dir(&dir).output().expect("git init");
        dir
    }

    #[test]
    fn the_origin_url_comes_back_verbatim() {
        let dir = init_repo("origin");
        Command::new("git")
            .args(["remote", "add", "origin", "git@github.com:ai-sirio/sirio.git"])
            .current_dir(&dir)
            .output()
            .expect("add remote");

        assert_eq!(
            GitRemote::origin_url(&dir).as_deref(),
            Some("git@github.com:ai-sirio/sirio.git")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_repository_without_an_origin_reports_none() {
        let dir = init_repo("no-origin");
        assert_eq!(GitRemote::origin_url(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_head_sha_is_a_full_commit_id() {
        let dir = init_repo("head-sha");
        std::fs::write(dir.join("README.md"), "hi\n").expect("write file");
        for args in [
            vec!["add", "README.md"],
            vec![
                "-c",
                "user.email=test@example.com",
                "-c",
                "user.name=Test",
                "commit",
                "-m",
                "initial",
            ],
        ] {
            let out = Command::new("git").args(args).current_dir(&dir).output().expect("git");
            assert!(out.status.success(), "git failed: {}", String::from_utf8_lossy(&out.stderr));
        }

        let sha = head_sha(&dir).expect("HEAD resolves");
        assert_eq!(sha.len(), 40);
        assert!(sha.chars().all(|c| c.is_ascii_hexdigit()), "not hex: {sha}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_repository_with_no_commits_has_no_head() {
        let dir = init_repo("no-head");
        assert_eq!(head_sha(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_owner_still_resolves_through_the_shared_lookup() {
        let dir = init_repo("owner");
        Command::new("git")
            .args(["remote", "add", "origin", "https://github.com/ai-sirio/sirio.git"])
            .current_dir(&dir)
            .output()
            .expect("add remote");

        assert_eq!(GitRemote::github_owner(&dir).as_deref(), Some("ai-sirio"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
