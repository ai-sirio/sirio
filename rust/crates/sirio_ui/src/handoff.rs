//! Taking a change request into a worktree (change requests C1, spec C §5):
//! which branch and push target it gets, and whether an existing worktree or
//! branch is reused, a new one created, or the checkout refused. Pure: the
//! host gathers the facts from git and the forge.

use std::path::PathBuf;

use sirio_forge::HeadRepository;

/// The project's remote that points at the change request's repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedRemote {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadOnlyReason {
    ForkRefusesPush,
    BranchGone,
    HeadRepositoryGone,
}

impl ReadOnlyReason {
    pub fn message(self) -> &'static str {
        match self {
            Self::ForkRefusesPush => "the fork does not accept pushes",
            Self::BranchGone => "the source branch is gone from the forge",
            Self::HeadRepositoryGone => "the source repository is gone",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PushTarget {
    Listed { remote: String, branch: String },
    Fork { remote: String, url: String, branch: String },
    ReadOnly(ReadOnlyReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// The local branch.
    pub branch: String,
    pub push: PushTarget,
}

impl Target {
    /// The remote and the branch on it, when the worktree tracks one.
    pub fn remote_and_branch(&self) -> Option<(&str, &str)> {
        match &self.push {
            PushTarget::Listed { remote, branch } | PushTarget::Fork { remote, branch, .. } => {
                Some((remote, branch))
            }
            PushTarget::ReadOnly(_) => None,
        }
    }

    /// `remote/branch`, the upstream git names.
    pub fn upstream(&self) -> Option<String> {
        self.remote_and_branch().map(|(remote, branch)| format!("{remote}/{branch}"))
    }
}

/// The target of a change request checkout. `base` is the remote the project
/// lists for the forge; `remotes` is every remote of the project. `gone_branch`
/// names the local branch when the head repository no longer exists (the forge
/// then drops its owner too, so `<owner>/<branch>` cannot be built).
pub fn target(
    source_branch: &str,
    number: u64,
    base: &ListedRemote,
    remotes: &[ListedRemote],
    head: Option<&HeadRepository>,
    gone_branch: &str,
) -> Target {
    let Some(head) = head else {
        return Target {
            branch: gone_branch.to_string(),
            push: PushTarget::ReadOnly(ReadOnlyReason::HeadRepositoryGone),
        };
    };
    if !head.cross_repository {
        let push = if head.branch_exists {
            PushTarget::Listed { remote: base.name.clone(), branch: source_branch.to_string() }
        } else {
            PushTarget::ReadOnly(ReadOnlyReason::BranchGone)
        };
        return Target { branch: source_branch.to_string(), push };
    }
    // The viewer's own fork, already a remote (origin, usually): the branch is
    // theirs, so it keeps its name and tracks that remote, as in the same
    // repository.
    if let Some(own) = remotes.iter().find(|remote| names_head_repository(&remote.url, head)) {
        let push = if head.branch_exists {
            PushTarget::Listed { remote: own.name.clone(), branch: source_branch.to_string() }
        } else {
            PushTarget::ReadOnly(ReadOnlyReason::BranchGone)
        };
        return Target { branch: source_branch.to_string(), push };
    }
    let push = if !head.branch_exists {
        PushTarget::ReadOnly(ReadOnlyReason::BranchGone)
    } else if !head.can_push {
        PushTarget::ReadOnly(ReadOnlyReason::ForkRefusesPush)
    } else {
        PushTarget::Fork {
            remote: fork_remote_name(&head.owner, number),
            url: clone_url_like(&base.url, head),
            branch: source_branch.to_string(),
        }
    };
    Target { branch: format!("{}/{}", head.owner, source_branch), push }
}

/// Whether a remote's URL names the change request's head repository, in
/// either of the forge's spellings.
fn names_head_repository(url: &str, head: &HeadRepository) -> bool {
    let url = normalised_url(url);
    !url.is_empty() && (url == normalised_url(&head.http_url) || url == normalised_url(&head.ssh_url))
}

/// `host/owner/repo` for comparing two spellings of one repository: the
/// scheme and any user are dropped, the host is lower-cased, and a trailing
/// `/` and `.git` go. The path keeps its case.
pub fn normalised_url(url: &str) -> String {
    let url = url.trim();
    let (authority, path) = match url.split_once("://") {
        Some((_, rest)) => rest.split_once('/').unwrap_or((rest, "")),
        // The scp-like form `user@host:path`.
        None => url.split_once(':').unwrap_or((url, "")),
    };
    let host = authority.rsplit('@').next().unwrap_or("").to_ascii_lowercase();
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path).trim_end_matches('/');
    if host.is_empty() || path.is_empty() {
        return String::new();
    }
    format!("{host}/{path}")
}

fn uses_ssh(url: &str) -> bool {
    url.starts_with("ssh://") || (url.contains('@') && url.contains(':') && !url.contains("://"))
}

fn clone_url_like(listed: &str, head: &HeadRepository) -> String {
    let (preferred, other) = if uses_ssh(listed) {
        (&head.ssh_url, &head.http_url)
    } else {
        (&head.http_url, &head.ssh_url)
    };
    if preferred.is_empty() { other.clone() } else { preferred.clone() }
}

/// `sirio-<owner>-<number>`, lower-case, with every run of characters a remote
/// name should not carry folded to one `-`. One remote per change request: a
/// remote's push mapping names one branch, so two change requests from one
/// owner cannot share it.
pub fn fork_remote_name(owner: &str, number: u64) -> String {
    let mut slug = String::new();
    for character in owner.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() { format!("sirio-fork-{number}") } else { format!("sirio-{slug}-{number}") }
}

/// The branch as one directory name: `alice/feat` → `alice-feat`.
pub fn worktree_dir_branch(branch: &str) -> String {
    branch.replace('/', "-")
}

/// A local branch of the target's name that no worktree has checked out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchFacts {
    pub sha: String,
    /// `remote/branch`.
    pub upstream: Option<String>,
    /// It is the change request's head or an ancestor of it.
    pub reaches_head: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
    /// A worktree linked to this change request that still exists.
    pub linked: Option<PathBuf>,
    /// A worktree with the target branch checked out, and that branch's
    /// upstream (`remote/branch`).
    pub checked_out: Option<(PathBuf, Option<String>)>,
    pub branch: Option<BranchFacts>,
    /// The URL of an existing remote named as the fork target's remote.
    pub fork_remote_url: Option<String>,
    pub new_path: PathBuf,
    pub new_path_taken: bool,
    pub head_sha: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Start {
    Track { upstream: String },
    At { commit: String },
    Existing { set_upstream: Option<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    Reuse { worktree: PathBuf, record_link: bool },
    Create { path: PathBuf, branch: String, start: Start, add_remote: Option<(String, String)> },
    Refuse(Refusal),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    CheckedOutTrackingElsewhere { worktree: PathBuf, upstream: Option<String> },
    BranchTracksElsewhere { branch: String, upstream: String },
    BranchPointsElsewhere { branch: String },
    RemoteUrlDiffers { remote: String, url: String },
    PathTaken { path: PathBuf },
}

impl Refusal {
    pub fn message(&self) -> String {
        match self {
            Self::CheckedOutTrackingElsewhere { worktree, upstream } => format!(
                "{} already has this branch, tracking {}",
                worktree.display(),
                upstream.as_deref().unwrap_or("nothing")
            ),
            Self::BranchTracksElsewhere { branch, upstream } => {
                format!("the local branch {branch} tracks {upstream}; rename or delete it first")
            }
            Self::BranchPointsElsewhere { branch } => format!(
                "the local branch {branch} has commits the change request does not; check it out yourself or push them"
            ),
            Self::RemoteUrlDiffers { remote, url } => {
                format!("the remote {remote} already points at {url}")
            }
            Self::PathTaken { path } => format!("{} already exists", path.display()),
        }
    }
}

pub fn decide(target: &Target, facts: &Facts) -> Plan {
    if let Some(worktree) = &facts.linked {
        return Plan::Reuse { worktree: worktree.clone(), record_link: false };
    }
    let upstream = target.upstream();
    if let Some((worktree, tracking)) = &facts.checked_out {
        return if *tracking == upstream {
            Plan::Reuse { worktree: worktree.clone(), record_link: true }
        } else {
            Plan::Refuse(Refusal::CheckedOutTrackingElsewhere {
                worktree: worktree.clone(),
                upstream: tracking.clone(),
            })
        };
    }
    let mut add_remote = None;
    if let PushTarget::Fork { remote, url, .. } = &target.push {
        match &facts.fork_remote_url {
            Some(existing) if existing != url => {
                return Plan::Refuse(Refusal::RemoteUrlDiffers { remote: remote.clone(), url: existing.clone() });
            }
            Some(_) => {}
            None => add_remote = Some((remote.clone(), url.clone())),
        }
    }
    if facts.new_path_taken {
        return Plan::Refuse(Refusal::PathTaken { path: facts.new_path.clone() });
    }
    let start = match &facts.branch {
        Some(branch) => {
            if let Some(tracking) = &branch.upstream
                && Some(tracking) != upstream.as_ref()
            {
                return Plan::Refuse(Refusal::BranchTracksElsewhere {
                    branch: target.branch.clone(),
                    upstream: tracking.clone(),
                });
            }
            if !branch.reaches_head {
                return Plan::Refuse(Refusal::BranchPointsElsewhere { branch: target.branch.clone() });
            }
            Start::Existing { set_upstream: if branch.upstream.is_none() { upstream } else { None } }
        }
        None => match upstream {
            Some(upstream) => Start::Track { upstream },
            None => Start::At { commit: facts.head_sha.clone() },
        },
    };
    Plan::Create { path: facts.new_path.clone(), branch: target.branch.clone(), start, add_remote }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sirio_forge::HeadRepository;
    use std::path::PathBuf;

    fn listed(url: &str) -> ListedRemote {
        ListedRemote { name: "origin".into(), url: url.into() }
    }

    fn head(owner: &str, cross: bool, branch_exists: bool, can_push: bool) -> HeadRepository {
        HeadRepository {
            owner: owner.into(),
            project: format!("{owner}/widgets"),
            http_url: format!("https://forge.example/{owner}/widgets.git"),
            ssh_url: format!("git@forge.example:{owner}/widgets.git"),
            cross_repository: cross,
            branch_exists,
            can_push,
        }
    }

    fn target_of(source: &str, head: Option<&HeadRepository>, remotes: &[ListedRemote]) -> Target {
        target(source, 101, &remotes[0], remotes, head, "pr-101")
    }

    #[test]
    fn same_repository_tracks_the_listed_remote() {
        let remotes = [listed("https://forge.example/acme/widgets.git")];
        let t = target_of("feat", Some(&head("acme", false, true, true)), &remotes);
        assert_eq!(t.branch, "feat");
        assert_eq!(t.push, PushTarget::Listed { remote: "origin".into(), branch: "feat".into() });
        assert_eq!(t.upstream().as_deref(), Some("origin/feat"));
    }

    #[test]
    fn a_fork_that_accepts_pushes_gets_its_own_remote_in_the_listed_scheme() {
        let remotes = [listed("https://forge.example/acme/widgets.git")];
        let https = target_of("feat", Some(&head("alice", true, true, true)), &remotes);
        assert_eq!(https.branch, "alice/feat");
        assert_eq!(
            https.push,
            PushTarget::Fork { remote: "sirio-alice-101".into(), url: "https://forge.example/alice/widgets.git".into(), branch: "feat".into() }
        );
        assert_eq!(https.upstream().as_deref(), Some("sirio-alice-101/feat"));
        let ssh = target_of("feat", Some(&head("alice", true, true, true)), &[listed("git@forge.example:acme/widgets.git")]);
        assert!(matches!(ssh.push, PushTarget::Fork { ref url, .. } if url == "git@forge.example:alice/widgets.git"));
        let ssh_scheme = target_of("feat", Some(&head("alice", true, true, true)), &[listed("ssh://git@forge.example/acme/widgets.git")]);
        assert!(matches!(ssh_scheme.push, PushTarget::Fork { ref url, .. } if url.starts_with("git@")));
    }

    #[test]
    fn a_fork_that_refuses_pushes_is_read_only() {
        let remotes = [listed("https://forge.example/acme/widgets.git")];
        let t = target_of("feat", Some(&head("alice", true, true, false)), &remotes);
        assert_eq!(t.branch, "alice/feat");
        assert_eq!(t.push, PushTarget::ReadOnly(ReadOnlyReason::ForkRefusesPush));
        assert_eq!(t.upstream(), None);
    }

    #[test]
    fn a_gone_branch_is_read_only_in_the_same_repository_and_in_a_fork() {
        let remotes = [listed("https://f/acme/w.git")];
        let same = target_of("feat", Some(&head("acme", false, false, true)), &remotes);
        assert_eq!((same.branch.as_str(), &same.push), ("feat", &PushTarget::ReadOnly(ReadOnlyReason::BranchGone)));
        let fork = target_of("feat", Some(&head("alice", true, false, true)), &remotes);
        assert_eq!((fork.branch.as_str(), &fork.push), ("alice/feat", &PushTarget::ReadOnly(ReadOnlyReason::BranchGone)));
    }

    #[test]
    fn a_deleted_head_repository_takes_the_numbered_name_and_is_read_only() {
        let remotes = [listed("https://f/acme/w.git")];
        let t = target("feat", 101, &remotes[0], &remotes, None, "pr-101");
        assert_eq!(t.branch, "pr-101");
        assert_eq!(t.push, PushTarget::ReadOnly(ReadOnlyReason::HeadRepositoryGone));
    }

    #[test]
    fn a_fork_with_a_missing_url_in_the_listed_scheme_uses_the_other() {
        let mut fork = head("alice", true, true, true);
        fork.ssh_url.clear();
        let remotes = [listed("git@forge.example:acme/widgets.git")];
        let t = target_of("feat", Some(&fork), &remotes);
        assert!(matches!(t.push, PushTarget::Fork { ref url, .. } if url.starts_with("https://")));
    }

    #[test]
    fn the_viewers_own_fork_remote_is_the_push_target_whatever_it_is_called() {
        // origin is the viewer's fork, upstream the base repository: the
        // checkout pushes to origin and the branch keeps its own name.
        let remotes = [
            listed("https://forge.example/acme/widgets.git"),
            ListedRemote { name: "origin".into(), url: "git@FORGE.example:alice/widgets.git/".into() },
        ];
        let own = target_of("feat", Some(&head("alice", true, true, true)), &remotes);
        assert_eq!(own.branch, "feat");
        assert_eq!(own.push, PushTarget::Listed { remote: "origin".into(), branch: "feat".into() });
        assert_eq!(own.upstream().as_deref(), Some("origin/feat"));
        let gone = target_of("feat", Some(&head("alice", true, false, true)), &remotes);
        assert_eq!((gone.branch.as_str(), &gone.push), ("feat", &PushTarget::ReadOnly(ReadOnlyReason::BranchGone)));
    }

    #[test]
    fn a_remote_of_another_owner_is_not_the_viewers_own_fork() {
        let remotes = [
            listed("https://forge.example/acme/widgets.git"),
            ListedRemote { name: "mirror".into(), url: "https://forge.example/bob/widgets.git".into() },
        ];
        let t = target_of("feat", Some(&head("alice", true, true, true)), &remotes);
        assert_eq!(t.branch, "alice/feat");
        assert!(matches!(t.push, PushTarget::Fork { ref remote, .. } if remote == "sirio-alice-101"));
    }

    #[test]
    fn repository_urls_name_one_repository_in_every_spelling() {
        let canonical = normalised_url("https://forge.example/alice/widgets.git");
        for spelling in [
            "http://forge.example/alice/widgets.git",
            "https://FORGE.example/alice/widgets",
            "https://forge.example/alice/widgets.git/",
            "ssh://git@forge.example/alice/widgets.git",
            "git@forge.example:alice/widgets.git",
            "git@FORGE.example:alice/widgets/",
        ] {
            assert_eq!(normalised_url(spelling), canonical, "{spelling}");
        }
        assert_ne!(normalised_url("https://forge.example/bob/widgets.git"), canonical);
        assert_ne!(normalised_url("https://other.example/alice/widgets.git"), canonical);
    }

    #[test]
    fn fork_remote_names_are_valid_git_remote_names() {
        assert_eq!(fork_remote_name("alice", 101), "sirio-alice-101");
        assert_eq!(fork_remote_name("Alice.Smith", 7), "sirio-alice-smith-7");
        assert_eq!(fork_remote_name("forks/alice", 201), "sirio-forks-alice-201");
        assert_eq!(fork_remote_name("a..b", 3), "sirio-a-b-3");
        assert_eq!(fork_remote_name("-x-", 4), "sirio-x-4");
        assert_eq!(fork_remote_name("...", 5), "sirio-fork-5");
    }

    #[test]
    fn two_change_requests_from_one_owner_get_two_remotes() {
        assert_ne!(fork_remote_name("alice", 101), fork_remote_name("alice", 102));
    }

    #[test]
    fn a_worktree_directory_is_one_flat_name() {
        assert_eq!(worktree_dir_branch("feat"), "feat");
        assert_eq!(worktree_dir_branch("alice/feature/login"), "alice-feature-login");
    }

    const HEAD_SHA: &str = "bbbb";

    fn facts() -> Facts {
        Facts {
            linked: None,
            checked_out: None,
            branch: None,
            fork_remote_url: None,
            new_path: PathBuf::from("/work/widgets-feat"),
            new_path_taken: false,
            head_sha: HEAD_SHA.into(),
        }
    }

    fn listed_target() -> Target {
        Target { branch: "feat".into(), push: PushTarget::Listed { remote: "origin".into(), branch: "feat".into() } }
    }

    fn fork_target() -> Target {
        Target {
            branch: "alice/feat".into(),
            push: PushTarget::Fork { remote: "sirio-alice".into(), url: "https://f/alice/w.git".into(), branch: "feat".into() },
        }
    }

    fn read_only_target() -> Target {
        Target { branch: "alice/feat".into(), push: PushTarget::ReadOnly(ReadOnlyReason::ForkRefusesPush) }
    }

    #[test]
    fn a_linked_worktree_is_reused_without_a_new_link() {
        let f = Facts { linked: Some("/work/widgets-old".into()), checked_out: Some(("/x".into(), None)), ..facts() };
        assert_eq!(decide(&listed_target(), &f), Plan::Reuse { worktree: "/work/widgets-old".into(), record_link: false });
    }

    #[test]
    fn a_worktree_on_the_branch_with_the_right_upstream_is_reused_and_linked() {
        let f = Facts { checked_out: Some(("/work/w".into(), Some("origin/feat".into()))), ..facts() };
        assert_eq!(decide(&listed_target(), &f), Plan::Reuse { worktree: "/work/w".into(), record_link: true });
        let ro = Facts { checked_out: Some(("/work/r".into(), None)), ..facts() };
        assert_eq!(decide(&read_only_target(), &ro), Plan::Reuse { worktree: "/work/r".into(), record_link: true });
    }

    #[test]
    fn a_worktree_on_the_branch_tracking_elsewhere_is_refused() {
        let f = Facts { checked_out: Some(("/work/w".into(), Some("mine/feat".into()))), ..facts() };
        assert_eq!(
            decide(&listed_target(), &f),
            Plan::Refuse(Refusal::CheckedOutTrackingElsewhere { worktree: "/work/w".into(), upstream: Some("mine/feat".into()) })
        );
    }

    #[test]
    fn an_existing_fork_remote_with_another_url_is_refused() {
        let f = Facts { fork_remote_url: Some("https://other/x.git".into()), ..facts() };
        assert_eq!(
            decide(&fork_target(), &f),
            Plan::Refuse(Refusal::RemoteUrlDiffers { remote: "sirio-alice".into(), url: "https://other/x.git".into() })
        );
    }

    #[test]
    fn an_existing_fork_remote_with_the_same_url_is_not_added_again() {
        let f = Facts { fork_remote_url: Some("https://f/alice/w.git".into()), ..facts() };
        match decide(&fork_target(), &f) {
            Plan::Create { add_remote, start, branch, .. } => {
                assert_eq!(add_remote, None);
                assert_eq!(start, Start::Track { upstream: "sirio-alice/feat".into() });
                assert_eq!(branch, "alice/feat");
            }
            other => panic!("expected Create, got {other:?}"),
        }
        match decide(&fork_target(), &facts()) {
            Plan::Create { add_remote, .. } => {
                assert_eq!(add_remote, Some(("sirio-alice".into(), "https://f/alice/w.git".into())));
            }
            other => panic!("expected Create, got {other:?}"),
        }
    }

    #[test]
    fn a_taken_path_is_refused() {
        let f = Facts { new_path_taken: true, ..facts() };
        assert_eq!(decide(&listed_target(), &f), Plan::Refuse(Refusal::PathTaken { path: "/work/widgets-feat".into() }));
    }

    #[test]
    fn a_local_branch_that_reaches_the_head_is_used_and_given_its_upstream() {
        let f = Facts { branch: Some(BranchFacts { sha: "aaaa".into(), upstream: None, reaches_head: true }), ..facts() };
        assert_eq!(
            decide(&listed_target(), &f),
            Plan::Create {
                path: "/work/widgets-feat".into(),
                branch: "feat".into(),
                start: Start::Existing { set_upstream: Some("origin/feat".into()) },
                add_remote: None,
            }
        );
        let tracked = Facts {
            branch: Some(BranchFacts { sha: "aaaa".into(), upstream: Some("origin/feat".into()), reaches_head: true }),
            ..facts()
        };
        assert!(matches!(decide(&listed_target(), &tracked), Plan::Create { start: Start::Existing { set_upstream: None }, .. }));
    }

    #[test]
    fn a_local_branch_that_points_elsewhere_is_refused() {
        let f = Facts { branch: Some(BranchFacts { sha: "cccc".into(), upstream: None, reaches_head: false }), ..facts() };
        assert_eq!(decide(&listed_target(), &f), Plan::Refuse(Refusal::BranchPointsElsewhere { branch: "feat".into() }));
    }

    #[test]
    fn a_local_branch_tracking_elsewhere_is_refused() {
        let f = Facts {
            branch: Some(BranchFacts { sha: "aaaa".into(), upstream: Some("mine/feat".into()), reaches_head: true }),
            ..facts()
        };
        assert_eq!(
            decide(&listed_target(), &f),
            Plan::Refuse(Refusal::BranchTracksElsewhere { branch: "feat".into(), upstream: "mine/feat".into() })
        );
    }

    #[test]
    fn a_fresh_checkout_tracks_its_upstream_or_starts_read_only_at_the_head() {
        assert!(matches!(
            decide(&listed_target(), &facts()),
            Plan::Create { start: Start::Track { ref upstream }, .. } if upstream == "origin/feat"
        ));
        assert!(matches!(
            decide(&read_only_target(), &facts()),
            Plan::Create { start: Start::At { ref commit }, add_remote: None, .. } if commit == HEAD_SHA
        ));
    }

    #[test]
    fn a_branch_with_its_own_commits_is_told_to_keep_them_and_never_to_delete_them() {
        let message = Refusal::BranchPointsElsewhere { branch: "feat".into() }.message();
        assert!(message.contains("commits the change request does not"), "{message}");
        assert!(!message.contains("delete"), "{message}");
    }

    #[test]
    fn every_refusal_and_read_only_reason_says_something() {
        for refusal in [
            Refusal::CheckedOutTrackingElsewhere { worktree: "/w".into(), upstream: None },
            Refusal::BranchTracksElsewhere { branch: "b".into(), upstream: "u".into() },
            Refusal::BranchPointsElsewhere { branch: "b".into() },
            Refusal::RemoteUrlDiffers { remote: "r".into(), url: "u".into() },
            Refusal::PathTaken { path: "/p".into() },
        ] {
            assert!(!refusal.message().is_empty());
        }
        for reason in [ReadOnlyReason::ForkRefusesPush, ReadOnlyReason::BranchGone, ReadOnlyReason::HeadRepositoryGone] {
            assert!(!reason.message().is_empty());
        }
    }
}
