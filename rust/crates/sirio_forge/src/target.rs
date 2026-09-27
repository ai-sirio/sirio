//! Where a change request lives: a forge host and a project path, read from
//! a git remote URL.

/// A forge host and the project path on it — `github.com` +
/// `ai-sirio/sirio`, or `git.example.com:8443` + `group/sub/project`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ForgeTarget {
    /// Lower-case host. A port is kept only for an HTTP(S) remote, and only
    /// when it is not the scheme's default: an SSH port says nothing about
    /// where the web API listens.
    pub host: String,
    /// The project path, without a leading slash or a trailing `.git`.
    pub project: String,
}

/// Parses the remote URL forms git accepts for a hosted repository:
/// `scheme://[user[:password]@]host[:port]/path` and the scp form
/// `[user@]host:path`.
///
/// Returns `None` for local paths, `file://` URLs, schemes git cannot fetch
/// a forge over, and any path with fewer than two segments — a forge project
/// is always `owner/name` or deeper.
pub fn parse_remote_url(url: &str) -> Option<ForgeTarget> {
    let url = url.trim();
    let (host, path) = match url.split_once("://") {
        Some((scheme, rest)) => split_url(scheme, rest)?,
        None => split_scp(url)?,
    };
    let host = host.to_ascii_lowercase();
    if host.is_empty() || host.contains(['/', '\\', ' ']) {
        return None;
    }
    let project = path.trim_matches('/');
    let project = project
        .strip_suffix(".git")
        .unwrap_or(project)
        .trim_end_matches('/');
    let segments: Vec<&str> = project.split('/').collect();
    if segments.len() < 2 || segments.iter().any(|segment| segment.is_empty()) {
        return None;
    }
    Some(ForgeTarget {
        host,
        project: project.to_string(),
    })
}

/// The URL form. Credentials are dropped with everything before the last
/// `@` of the authority; the port survives only for HTTP(S), minus the
/// scheme's default.
fn split_url<'a>(scheme: &str, rest: &'a str) -> Option<(String, &'a str)> {
    let scheme = scheme.to_ascii_lowercase();
    let web = match scheme.as_str() {
        "https" | "http" => true,
        "ssh" | "git" | "git+ssh" | "ssh+git" => false,
        _ => return None,
    };
    let slash = rest.find('/')?;
    let (authority, path) = rest.split_at(slash);
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => (host, Some(port)),
        _ => (host_port, None),
    };
    let default_port = if scheme == "https" { "443" } else { "80" };
    let host = match port {
        Some(port) if web && port != default_port => format!("{host}:{port}"),
        _ => host.to_string(),
    };
    Some((host, path))
}

/// The scp form. A one-letter "host" is a Windows drive (`C:\src\repo`).
fn split_scp(url: &str) -> Option<(String, &str)> {
    let (before, path) = url.split_once(':')?;
    let host = before.rsplit_once('@').map_or(before, |(_, host)| host);
    if host.len() <= 1 || host.contains(['/', '\\']) {
        return None;
    }
    Some((host.to_string(), path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(url: &str) -> Option<(String, String)> {
        parse_remote_url(url).map(|target| (target.host, target.project))
    }

    fn some(host: &str, project: &str) -> Option<(String, String)> {
        Some((host.to_string(), project.to_string()))
    }

    #[test]
    fn the_scp_form_names_host_and_project() {
        assert_eq!(
            parsed("git@github.com:ai-sirio/sirio.git"),
            some("github.com", "ai-sirio/sirio")
        );
    }

    #[test]
    fn a_trailing_git_and_slash_are_not_part_of_the_project() {
        assert_eq!(
            parsed("https://github.com/ai-sirio/sirio.git/"),
            some("github.com", "ai-sirio/sirio")
        );
        assert_eq!(
            parsed("https://github.com/ai-sirio/sirio/"),
            some("github.com", "ai-sirio/sirio")
        );
    }

    #[test]
    fn gitlab_subgroups_stay_in_the_project_path() {
        assert_eq!(
            parsed("https://gitlab.example.com/group/sub/project.git"),
            some("gitlab.example.com", "group/sub/project")
        );
        assert_eq!(
            parsed("git@gitlab.example.com:group/sub/project.git"),
            some("gitlab.example.com", "group/sub/project")
        );
    }

    #[test]
    fn an_ssh_port_is_not_the_api_port() {
        assert_eq!(
            parsed("ssh://git@gitlab.example.com:2222/group/project.git"),
            some("gitlab.example.com", "group/project")
        );
    }

    #[test]
    fn an_https_port_is_where_the_api_listens() {
        assert_eq!(
            parsed("https://git.example.com:8443/group/project.git"),
            some("git.example.com:8443", "group/project")
        );
    }

    #[test]
    fn a_default_port_is_dropped() {
        assert_eq!(
            parsed("https://github.com:443/ai-sirio/sirio"),
            some("github.com", "ai-sirio/sirio")
        );
        assert_eq!(
            parsed("http://git.example.com:80/group/project"),
            some("git.example.com", "group/project")
        );
    }

    #[test]
    fn credentials_in_the_url_never_reach_the_target() {
        let target = parse_remote_url("https://oauth2:glpat-secret@gitlab.com/group/project.git")
            .expect("parses");
        assert_eq!(
            (target.host.as_str(), target.project.as_str()),
            ("gitlab.com", "group/project")
        );
        assert!(!format!("{target:?}").contains("secret"));
    }

    #[test]
    fn the_host_is_case_folded_and_the_project_is_not() {
        assert_eq!(
            parsed("https://GitHub.COM/Ai-Sirio/Sirio"),
            some("github.com", "Ai-Sirio/Sirio")
        );
    }

    #[test]
    fn surrounding_whitespace_is_ignored() {
        assert_eq!(
            parsed("  git@github.com:ai-sirio/sirio.git\n"),
            some("github.com", "ai-sirio/sirio")
        );
    }

    #[test]
    fn local_paths_are_not_forge_remotes() {
        for url in [
            "/srv/git/group/repo.git",
            "../group/repo",
            "file:///srv/git/group/repo.git",
            r"C:\src\group\repo",
            "C:/src/group/repo",
            "",
        ] {
            assert_eq!(parsed(url), None, "{url:?}");
        }
    }

    #[test]
    fn a_project_needs_an_owner_and_a_name() {
        for url in [
            "https://github.com/ai-sirio",
            "git@github.com:sirio.git",
            "https://github.com/",
            "https://github.com//sirio",
            "https://github.com/ai-sirio//sirio",
        ] {
            assert_eq!(parsed(url), None, "{url:?}");
        }
    }

    #[test]
    fn a_scheme_git_cannot_fetch_from_a_forge_is_refused() {
        assert_eq!(parsed("ftp://git.example.com/group/project"), None);
    }
}
