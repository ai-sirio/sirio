//! The one `git fetch` Sirio runs (spec §6): a change request's revisions,
//! into refs Sirio owns, without a prompt, a hang, or a trace in
//! `FETCH_HEAD`, the tags or the user's branches.

use std::fmt;
use std::path::Path;
use std::time::Duration;

use crate::error::GitError;
use crate::git;

const NAMESPACE: &str = "refs/sirio/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// git did not answer within the timeout and was killed.
    TimedOut,
    /// git failed; `detail` is the tail of its stderr with credentials removed.
    Failed { detail: String },
}

impl fmt::Display for FetchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TimedOut => formatter.write_str("git fetch did not answer in time"),
            Self::Failed { detail } => formatter.write_str(detail),
        }
    }
}

impl std::error::Error for FetchError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchRefspec {
    source: String,
    destination: String,
}

fn ref_is_safe(name: &str) -> bool {
    !name.is_empty()
        && !name.contains("..")
        && !name.chars().any(|c| c.is_whitespace() || c.is_control() || ":?[\\^~*".contains(c))
}

impl FetchRefspec {
    pub fn new(source: &str, destination: &str) -> Result<Self, FetchError> {
        if !(source.starts_with("refs/") && destination.starts_with(NAMESPACE))
            || !ref_is_safe(source)
            || !ref_is_safe(destination)
        {
            return Err(FetchError::Failed {
                detail: format!("refusing the refspec {source} -> {destination}"),
            });
        }
        Ok(Self {
            source: source.to_string(),
            destination: destination.to_string(),
        })
    }

    fn spelled(&self) -> String {
        format!("+{}:{}", self.source, self.destination)
    }
}

/// Fetches `refspecs` from `remote`, at most `timeout`.
pub fn fetch_refs(
    repo: &Path,
    remote: &str,
    refspecs: &[FetchRefspec],
    timeout: Duration,
) -> Result<(), FetchError> {
    if remote.is_empty() || remote.starts_with('-') {
        return Err(FetchError::Failed {
            detail: format!("refusing the remote name {remote:?}"),
        });
    }
    let spelled: Vec<String> = refspecs.iter().map(FetchRefspec::spelled).collect();
    match run(repo, remote, &spelled, timeout, true) {
        // `--no-write-fetch-head` needs git 2.29; an older git says so, and
        // then `FETCH_HEAD` is written — harmless, but never a failure.
        Err(Failure::FlagUnknown) => run(repo, remote, &spelled, timeout, false).map_err(Failure::into_error),
        other => other.map_err(Failure::into_error),
    }
}

/// Why one `git fetch` attempt failed.
#[derive(Debug, PartialEq, Eq)]
enum Failure {
    /// git is older than 2.29 and refused `--no-write-fetch-head`.
    FlagUnknown,
    Fetch(FetchError),
}

impl Failure {
    fn into_error(self) -> FetchError {
        match self {
            Self::FlagUnknown => FetchError::Failed {
                detail: "git does not know --no-write-fetch-head".to_string(),
            },
            Self::Fetch(error) => error,
        }
    }
}

/// Reads a failed fetch's whole stderr: git's own refusal of
/// `--no-write-fetch-head` comes first, above its full usage text, so it is
/// looked for before the stderr is cut down to the tail the user sees.
fn classify(stderr: &str) -> Failure {
    let refused = stderr
        .lines()
        .any(|line| line.trim() == "error: unknown option `no-write-fetch-head'");
    if refused {
        return Failure::FlagUnknown;
    }
    Failure::Fetch(FetchError::Failed {
        detail: tail(&redact_credentials(stderr)),
    })
}

fn run(
    repo: &Path,
    remote: &str,
    spelled: &[String],
    timeout: Duration,
    no_write_fetch_head: bool,
) -> Result<(), Failure> {
    // An empty `--refmap` stops the remote's configured fetch refspec from
    // also updating `refs/remotes/<remote>/…` for a branch named here, and
    // `--no-recurse-submodules` keeps a moved submodule pointer from fetching
    // inside the submodule. No auto-gc or auto-maintenance after the fetch:
    // it could detach and outlive the timeout. Config keys, not options, so
    // an older git ignores them instead of refusing the fetch.
    let mut args = vec![
        "-c",
        "gc.auto=0",
        "-c",
        "maintenance.auto=false",
        "fetch",
        "--no-tags",
        "--quiet",
        "--refmap=",
        "--no-recurse-submodules",
    ];
    if no_write_fetch_head {
        args.push("--no-write-fetch-head");
    }
    args.push("--");
    args.push(remote);
    args.extend(spelled.iter().map(String::as_str));
    match git::run_remote(&args, repo, timeout) {
        Ok(output) if output.is_success() => Ok(()),
        Ok(output) if no_write_fetch_head => Err(classify(&output.stderr)),
        Ok(output) => Err(Failure::Fetch(FetchError::Failed {
            detail: tail(&redact_credentials(&output.stderr)),
        })),
        Err(GitError::TimedOut { .. }) => Err(Failure::Fetch(FetchError::TimedOut)),
        Err(error) => Err(Failure::Fetch(FetchError::Failed {
            detail: redact_credentials(&error.to_string()),
        })),
    }
}

/// The last few lines of git's stderr: where its verdict is.
fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().filter(|line| !line.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(6)..].join("\n")
}

/// Removes `user:password@` and `token@` from every URL in `text`, up to the
/// next whitespace or quote — a token may contain a `/`, so the URL's own
/// structure cannot be trusted to end the credential. The scp form
/// `git@host:path` has no `://` and is left alone.
pub fn redact_credentials(text: &str) -> String {
    let mut cleaned = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let (head, tail) = rest.split_at(at + 3);
        cleaned.push_str(head);
        let end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '\'' | '"'))
            .unwrap_or(tail.len());
        let (span, remainder) = tail.split_at(end);
        match span.rfind('@') {
            Some(index) => cleaned.push_str(&span[index + 1..]),
            None => cleaned.push_str(span),
        }
        rest = remainder;
    }
    cleaned.push_str(rest);
    cleaned
}

/// Every remote of `repo` with its configured URL (`config --get`, so no
/// `insteadOf` rewrite: a forge is recognised by the URL the user wrote).
pub fn list_remotes(repo: &Path) -> Vec<(String, String)> {
    let Ok(output) = git::run_accepting(&["remote"], repo, &[0]) else {
        return Vec::new();
    };
    output
        .stdout_string()
        .lines()
        .filter_map(|name| {
            let name = name.trim();
            crate::remote::remote_url(repo, name).map(|url| (name.to_string(), url))
        })
        .collect()
}

/// Every ref under `prefix`, by full name.
pub fn refs_under(repo: &Path, prefix: &str) -> Result<Vec<String>, GitError> {
    let output = git::run_accepting(&["for-each-ref", "--format=%(refname)", prefix], repo, &[0])?;
    Ok(output.stdout_string().lines().map(str::to_string).collect())
}

/// Deletes `name`, which must be inside Sirio's own `refs/sirio/`.
pub fn delete_ref(repo: &Path, name: &str) -> Result<(), GitError> {
    if !name.starts_with(NAMESPACE) || !ref_is_safe(name) {
        return Err(GitError::CommandFailed {
            code: 128,
            stderr: format!("refusing to delete {name:?}: not a Sirio ref"),
        });
    }
    git::run_accepting(&["update-ref", "-d", name], repo, &[0]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_and_password_are_removed_from_an_http_url() {
        assert_eq!(
            redact_credentials("fatal: unable to access 'https://bob:hunter2@ghe.test/acme/w.git/': 403"),
            "fatal: unable to access 'https://ghe.test/acme/w.git/': 403"
        );
    }

    #[test]
    fn a_bare_token_is_removed_too() {
        assert_eq!(redact_credentials("https://ghp_abc123@github.com/o/r.git"), "https://github.com/o/r.git");
    }

    #[test]
    fn every_url_in_the_text_is_cleaned() {
        assert_eq!(
            redact_credentials("a https://u:p@one.test/x b http://t@two.test/y c"),
            "a https://one.test/x b http://two.test/y c"
        );
    }

    #[test]
    fn a_token_that_contains_a_slash_leaves_no_fragment() {
        let cleaned = redact_credentials("https://user:ab/cd@host.test/x.git");
        assert!(!cleaned.contains("ab") && !cleaned.contains("cd@"), "{cleaned}");
    }

    #[test]
    fn urls_without_credentials_and_the_scp_form_are_untouched() {
        for text in [
            "https://github.com/o/r.git",
            "git@github.com:o/r.git",
            "fatal: could not read Username for 'http://127.0.0.1:1234': terminal prompts disabled",
            "no url here",
        ] {
            assert_eq!(redact_credentials(text), text);
        }
    }

    #[test]
    fn a_refspec_outside_the_namespace_or_with_odd_characters_is_refused() {
        for (source, destination) in [
            ("refs/heads/x", "refs/heads/y"),
            ("refs/pull/7/head", "refs/sirio/a b"),
            ("--upload-pack=x", "refs/sirio/x"),
            ("refs/heads/a:b", "refs/sirio/x"),
            ("refs/heads/a", "refs/sirio/x:y"),
            ("refs/heads/*", "refs/sirio/x"),
            ("refs/heads/a..b", "refs/sirio/x"),
            ("refs/heads/a", ""),
        ] {
            assert!(
                FetchRefspec::new(source, destination).is_err(),
                "{source} -> {destination} must be refused"
            );
        }
        assert!(FetchRefspec::new("refs/pull/7/head", "refs/sirio/change-requests/o/7/head").is_ok());
    }

    /// What git older than 2.29 prints for `--no-write-fetch-head`: the
    /// refusal, then its whole usage text (taken from a real `git fetch`).
    const OLD_GIT_REFUSAL: &str = r#"error: unknown option `no-write-fetch-head'
usage: git fetch [<options>] [<repository> [<refspec>...]]
   or: git fetch [<options>] <group>
   or: git fetch --multiple [<options>] [(<repository>|<group>)...]
   or: git fetch --all [<options>]

    -v, --[no-]verbose    be more verbose
    -q, --[no-]quiet      be more quiet
    --[no-]all            fetch from all remotes
    --[no-]set-upstream   set upstream for git pull/fetch
    -a, --[no-]append     append to .git/FETCH_HEAD instead of overwriting
    --[no-]atomic         use atomic transaction to update references
    --[no-]upload-pack <path>
                          path to upload pack on remote end
    -f, --[no-]force      force overwrite of local reference
    -m, --[no-]multiple   fetch from multiple remotes
    -t, --[no-]tags       fetch all tags and associated objects
    -n                    do not fetch all tags (--no-tags)
    -j, --[no-]jobs <n>   number of submodules fetched in parallel
    --[no-]prefetch       modify the refspec to place all refs within refs/prefetch/
    -p, --[no-]prune      prune remote-tracking branches no longer on remote
    -P, --[no-]prune-tags prune local tags no longer on remote and clobber changed tags
    --[no-]recurse-submodules[=<on-demand>]
                          control recursive fetching of submodules
    --[no-]dry-run        dry run
    --[no-]porcelain      machine-readable output
    -k, --[no-]keep       keep downloaded pack
    -u, --[no-]update-head-ok
                          allow updating of HEAD ref
    --[no-]progress       force progress reporting
    --[no-]depth <depth>  deepen history of shallow clone
    --[no-]shallow-since <time>
                          deepen history of shallow repository based on time
    --[no-]shallow-exclude <ref>
                          deepen history of shallow clone, excluding ref
    --[no-]deepen <n>     deepen history of shallow clone
    --unshallow           convert to a complete repository
    --refetch             re-fetch without negotiating common commits
    --[no-]update-shallow accept refs that update .git/shallow
    --refmap <refmap>     specify fetch refmap
    -o, --[no-]server-option <server-specific>
                          option to transmit
    -4, --ipv4            use IPv4 addresses only
    -6, --ipv6            use IPv6 addresses only
    --[no-]negotiation-restrict <revision>
                          report that we have only objects reachable from this object
    --[no-]negotiation-tip <revision>
                          alias of --negotiation-restrict
    --[no-]negotiation-include <revision>
                          ensure this ref is always sent as a negotiation have
    --[no-]negotiate-only do not fetch a packfile; instead, print ancestors of negotiation tips
    --[no-]filter <args>  object filtering
    --[no-]auto-maintenance
                          run 'maintenance --auto' after fetching
    --[no-]auto-gc        run 'maintenance --auto' after fetching
    --[no-]show-forced-updates
                          check for forced-updates on all updated branches
    --[no-]write-commit-graph
                          write the commit-graph after fetching
    --[no-]stdin          accept refspecs from stdin

"#;

    #[test]
    fn an_old_git_refusing_no_write_fetch_head_is_recognised_above_its_usage_text() {
        assert!(OLD_GIT_REFUSAL.lines().count() > 50, "the fixture is the full usage dump");
        assert_eq!(classify(OLD_GIT_REFUSAL), Failure::FlagUnknown);
    }

    #[test]
    fn any_other_failure_is_reported_with_the_tail_of_stderr() {
        let other_option = OLD_GIT_REFUSAL.replacen("no-write-fetch-head", "refetch", 1);
        assert!(matches!(classify(&other_option), Failure::Fetch(FetchError::Failed { .. })));
        assert_eq!(
            classify("fatal: couldn't find remote ref refs/pull/9/head\n"),
            Failure::Fetch(FetchError::Failed {
                detail: "fatal: couldn't find remote ref refs/pull/9/head".to_string()
            })
        );
    }
}
