use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use sirio_git::{
    FastForward, FetchError, RemoteOutcome, create_worktree_at, create_worktree_tracking,
    ensure_push_refspec, ensure_remote, fast_forward, fetch_branch, is_ancestor, local_branch,
    set_upstream, upstream_of, worktree_for_branch,
};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git").args(args).current_dir(cwd).output().expect("git runs");
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// `repo/` cloned from `origin.git`, which has `main` and `feat` (one commit
/// ahead of `main`). Returns (scratch, repo, bare, feat sha).
fn fixture(tag: &str) -> (Scratch, PathBuf, PathBuf, String) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("sirio-checkout-{tag}-{}-{nanos}", std::process::id()));
    let seed = root.join("seed");
    std::fs::create_dir_all(&seed).unwrap();
    git(&seed, &["init", "-q", "-b", "main"]);
    git(&seed, &["config", "user.email", "test@sirio.dev"]);
    git(&seed, &["config", "user.name", "Sirio Test"]);
    std::fs::write(seed.join("a.txt"), "one\n").unwrap();
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-q", "-m", "one"]);
    git(&seed, &["checkout", "-q", "-b", "feat"]);
    std::fs::write(seed.join("a.txt"), "two\n").unwrap();
    git(&seed, &["commit", "-q", "-am", "two"]);
    let feat = git(&seed, &["rev-parse", "HEAD"]);
    git(&seed, &["checkout", "-q", "main"]);
    let bare = root.join("origin.git");
    git(&root, &["clone", "-q", "--bare", seed.to_str().unwrap(), bare.to_str().unwrap()]);
    let repo = root.join("repo");
    git(&root, &["clone", "-q", bare.to_str().unwrap(), repo.to_str().unwrap()]);
    git(&repo, &["config", "user.email", "test@sirio.dev"]);
    git(&repo, &["config", "user.name", "Sirio Test"]);
    (Scratch(root), repo, bare, feat)
}

const TIMEOUT: Duration = Duration::from_secs(30);

#[test]
fn fetching_a_branch_creates_its_remote_tracking_ref() {
    let (_scratch, repo, _bare, feat) = fixture("fetch");
    fetch_branch(&repo, "origin", "feat", TIMEOUT).expect("fetch");
    assert_eq!(git(&repo, &["rev-parse", "refs/remotes/origin/feat"]), feat);
}

#[test]
fn fetching_a_branch_the_remote_lacks_fails_with_the_reason() {
    let (_scratch, repo, _bare, _feat) = fixture("fetch-missing");
    match fetch_branch(&repo, "origin", "nope", TIMEOUT) {
        Err(FetchError::Failed { detail }) => assert!(detail.contains("nope"), "{detail}"),
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn a_branch_name_git_refuses_is_refused_before_git_runs() {
    let (_scratch, repo, _bare, _feat) = fixture("fetch-invalid");
    assert!(matches!(fetch_branch(&repo, "origin", "a..b", TIMEOUT), Err(FetchError::Failed { .. })));
    assert!(matches!(fetch_branch(&repo, "origin", "-x", TIMEOUT), Err(FetchError::Failed { .. })));
}

#[test]
fn a_fetch_that_hangs_stops_at_the_timeout() {
    let (_scratch, repo, _bare, _feat) = fixture("fetch-hang");
    git(&repo, &["config", "protocol.ext.allow", "always"]);
    git(&repo, &["remote", "add", "slow", "ext::sh -c sleep% 30"]);
    let started = std::time::Instant::now();
    let result = fetch_branch(&repo, "slow", "feat", Duration::from_millis(500));
    assert!(matches!(result, Err(FetchError::TimedOut)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_remote_is_added_once_and_another_url_is_a_conflict() {
    let (_scratch, repo, bare, _feat) = fixture("remote");
    let url = bare.to_str().unwrap();
    assert_eq!(ensure_remote(&repo, "sirio-alice", url).unwrap(), RemoteOutcome::Added);
    assert_eq!(ensure_remote(&repo, "sirio-alice", url).unwrap(), RemoteOutcome::AlreadyThere);
    assert_eq!(
        ensure_remote(&repo, "sirio-alice", "https://elsewhere.example/x.git").unwrap(),
        RemoteOutcome::Conflict { existing: url.to_string() }
    );
    assert_eq!(git(&repo, &["remote", "get-url", "sirio-alice"]), url);
}

#[test]
fn a_local_branch_reports_its_commit_and_upstream() {
    let (_scratch, repo, _bare, _feat) = fixture("local");
    assert_eq!(local_branch(&repo, "feat").unwrap(), None);
    let main = local_branch(&repo, "main").unwrap().expect("main exists");
    assert_eq!(main.sha, git(&repo, &["rev-parse", "main"]));
    let upstream = main.upstream.expect("main tracks origin");
    assert_eq!((upstream.remote.as_str(), upstream.branch.as_str()), ("origin", "main"));
}

#[test]
fn ancestry_is_answered_both_ways() {
    let (_scratch, repo, _bare, feat) = fixture("ancestor");
    fetch_branch(&repo, "origin", "feat", TIMEOUT).unwrap();
    let main = git(&repo, &["rev-parse", "main"]);
    assert!(is_ancestor(&repo, &main, &feat).unwrap());
    assert!(!is_ancestor(&repo, &feat, &main).unwrap());
}

#[test]
fn a_tracking_worktree_pushes_back_to_its_upstream() {
    let (scratch, repo, bare, _feat) = fixture("track");
    fetch_branch(&repo, "origin", "feat", TIMEOUT).unwrap();
    let path = scratch.0.join("repo-feat");
    create_worktree_tracking(&repo, "feat", &path, "origin/feat").expect("worktree");
    assert_eq!(worktree_for_branch(&repo, "feat").unwrap().map(|p| p.canonicalize().unwrap()), Some(path.canonicalize().unwrap()));
    let upstream = upstream_of(&repo, "feat").unwrap().expect("tracks");
    assert_eq!((upstream.remote.as_str(), upstream.branch.as_str()), ("origin", "feat"));
    std::fs::write(path.join("a.txt"), "three\n").unwrap();
    git(&path, &["commit", "-q", "-am", "three"]);
    git(&path, &["push", "-q"]);
    assert_eq!(git(&bare, &["rev-parse", "refs/heads/feat"]), git(&path, &["rev-parse", "HEAD"]));
}

#[test]
fn a_fork_branch_with_another_local_name_pushes_with_a_plain_git_push() {
    let (scratch, repo, _bare, feat) = fixture("fork-push");
    // The fork: a second bare with the same `feat`.
    let fork = scratch.0.join("fork.git");
    git(&scratch.0, &["clone", "-q", "--bare", scratch.0.join("origin.git").to_str().unwrap(), fork.to_str().unwrap()]);
    assert_eq!(ensure_remote(&repo, "sirio-alice", fork.to_str().unwrap()).unwrap(), RemoteOutcome::Added);
    fetch_branch(&repo, "sirio-alice", "feat", TIMEOUT).unwrap();
    let path = scratch.0.join("repo-alice-feat");
    create_worktree_tracking(&repo, "alice/feat", &path, "sirio-alice/feat").unwrap();
    ensure_push_refspec(&repo, "sirio-alice", "alice/feat", "feat").unwrap();
    ensure_push_refspec(&repo, "sirio-alice", "alice/feat", "feat").unwrap();
    assert_eq!(git(&repo, &["config", "--get-all", "remote.sirio-alice.push"]).lines().count(), 1);
    std::fs::write(path.join("a.txt"), "from the maintainer\n").unwrap();
    git(&path, &["commit", "-q", "-am", "maintainer"]);
    git(&path, &["push", "-q"]);
    let pushed = git(&path, &["rev-parse", "HEAD"]);
    assert_ne!(pushed, feat);
    assert_eq!(git(&fork, &["rev-parse", "refs/heads/feat"]), pushed);
    let no_alice = Command::new("git").args(["rev-parse", "--verify", "--quiet", "refs/heads/alice/feat"]).current_dir(&fork).output().unwrap();
    assert!(!no_alice.status.success(), "the fork must not gain an alice/feat branch");
}

#[test]
fn a_worktree_at_a_commit_has_no_upstream() {
    let (scratch, repo, _bare, feat) = fixture("at");
    fetch_branch(&repo, "origin", "feat", TIMEOUT).unwrap();
    let path = scratch.0.join("repo-ro");
    create_worktree_at(&repo, "alice/feat", &path, &feat).expect("worktree");
    assert_eq!(git(&path, &["rev-parse", "HEAD"]), feat);
    assert_eq!(upstream_of(&repo, "alice/feat").unwrap(), None);
}

#[test]
fn an_upstream_can_be_set_on_an_existing_branch() {
    let (_scratch, repo, _bare, feat) = fixture("set-upstream");
    fetch_branch(&repo, "origin", "feat", TIMEOUT).unwrap();
    git(&repo, &["branch", "--no-track", "feat", &feat]);
    set_upstream(&repo, "feat", "origin/feat").unwrap();
    let upstream = upstream_of(&repo, "feat").unwrap().expect("tracks");
    assert_eq!(upstream.branch, "feat");
}

#[test]
fn fast_forward_moves_only_a_clean_worktree_that_is_behind() {
    let (scratch, repo, bare, _feat) = fixture("ff");
    fetch_branch(&repo, "origin", "feat", TIMEOUT).unwrap();
    let path = scratch.0.join("repo-feat");
    create_worktree_tracking(&repo, "feat", &path, "origin/feat").unwrap();
    assert_eq!(fast_forward(&path, "origin/feat").unwrap(), FastForward::UpToDate);

    // Someone pushes to the bare; the worktree is behind.
    let other = scratch.0.join("other");
    git(&scratch.0, &["clone", "-q", "-b", "feat", bare.to_str().unwrap(), other.to_str().unwrap()]);
    git(&other, &["config", "user.email", "o@sirio.dev"]);
    git(&other, &["config", "user.name", "Other"]);
    std::fs::write(other.join("b.txt"), "b\n").unwrap();
    git(&other, &["add", "."]);
    git(&other, &["commit", "-q", "-m", "b"]);
    git(&other, &["push", "-q"]);
    let pushed = git(&other, &["rev-parse", "HEAD"]);
    fetch_branch(&repo, "origin", "feat", TIMEOUT).unwrap();
    assert_eq!(fast_forward(&path, "origin/feat").unwrap(), FastForward::Advanced { to: pushed.clone() });
    assert_eq!(git(&path, &["rev-parse", "HEAD"]), pushed);

    // A tracked change makes it dirty: nothing moves.
    std::fs::write(other.join("b.txt"), "bb\n").unwrap();
    git(&other, &["commit", "-q", "-am", "bb"]);
    git(&other, &["push", "-q"]);
    fetch_branch(&repo, "origin", "feat", TIMEOUT).unwrap();
    std::fs::write(path.join("a.txt"), "local edit\n").unwrap();
    assert_eq!(fast_forward(&path, "origin/feat").unwrap(), FastForward::Dirty);
    assert_eq!(git(&path, &["rev-parse", "HEAD"]), pushed);

    // A local commit makes it diverge: nothing moves.
    git(&path, &["commit", "-q", "-am", "local"]);
    let local = git(&path, &["rev-parse", "HEAD"]);
    assert_eq!(fast_forward(&path, "origin/feat").unwrap(), FastForward::Diverged);
    assert_eq!(git(&path, &["rev-parse", "HEAD"]), local);
}

#[test]
fn an_untracked_file_does_not_make_a_worktree_dirty() {
    let (scratch, repo, _bare, _feat) = fixture("untracked");
    fetch_branch(&repo, "origin", "feat", TIMEOUT).unwrap();
    let path = scratch.0.join("repo-feat");
    create_worktree_tracking(&repo, "feat", &path, "origin/feat").unwrap();
    std::fs::write(path.join("notes.md"), "mine\n").unwrap();
    assert_eq!(fast_forward(&path, "origin/feat").unwrap(), FastForward::UpToDate);
}
