//! `git log` reading against real temporary repositories.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use tiller_git::GitLog;

/// A throwaway directory, removed on drop. Canonicalized so paths match what
/// git reports (macOS `/var` is a symlink to `/private/var`).
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("tiller-git-test-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create temp dir");
        let path = std::fs::canonicalize(&path).expect("canonicalize temp dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs `git <args>` in `dir`, asserting success.
fn git(dir: &Path, args: &[&str]) {
    ensure_generous_timeout();
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("git must be installed to run these tests");
    assert!(
        status.success(),
        "`git {args:?}` failed in {}",
        dir.display()
    );
}

fn head_sha(dir: &Path, revision: &str) -> String {
    let output = Command::new("git")
        .args(["rev-parse", revision])
        .current_dir(dir)
        .output()
        .expect("git must be installed to run these tests");
    assert!(output.status.success(), "rev-parse {revision} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// The crate's runner bounds every git invocation with a wall-clock
/// deadline (10 s by default) so a hung git can never freeze the UI. Under
/// machine load — parallel compiles, high load average — a single
/// invocation on a tiny fixture repo can legitimately exceed that budget,
/// so the tests opt into a generous one. The production default is
/// untouched; the override only applies while this variable is set.
const TEST_GIT_TIMEOUT_MS: &str = "120000";

/// Sets the process-wide timeout override exactly once, before any test
/// shells out through the crate's runner. Every test in this file reaches
/// [`git`] before touching the crate API, so this one call site covers all
/// of them.
fn ensure_generous_timeout() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // SAFETY: test process, set once. The only reader is the crate's
        // per-call `TILLER_GIT_TIMEOUT_MS` lookup, and a wider budget can
        // only turn a would-be timeout into a pass — never the reverse.
        unsafe { std::env::set_var("TILLER_GIT_TIMEOUT_MS", TEST_GIT_TIMEOUT_MS) };
    });
}

/// Builds: c1 <- c2 on main, plus a branch `side` off c1 merged into main.
fn repo_with_a_merge() -> TempDir {
    let dir = TempDir::new();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "t@example.com"]);
    git(dir.path(), &["config", "user.name", "Tester"]);

    std::fs::write(dir.path().join("a.txt"), "1").expect("write a.txt");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "c1"]);

    git(dir.path(), &["checkout", "-q", "-b", "side"]);
    std::fs::write(dir.path().join("b.txt"), "2").expect("write b.txt");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "c2 on side"]);

    git(dir.path(), &["checkout", "-q", "main"]);
    std::fs::write(dir.path().join("c.txt"), "3").expect("write c.txt");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "c3 on main"]);

    git(
        dir.path(),
        &["merge", "-q", "--no-ff", "side", "-m", "merge side"],
    );
    dir
}

fn repo_with_a_rename() -> TempDir {
    let dir = TempDir::new();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "t@example.com"]);
    git(dir.path(), &["config", "user.name", "Tester"]);
    std::fs::write(dir.path().join("old.txt"), "content").expect("write");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "initial"]);
    std::fs::rename(dir.path().join("old.txt"), dir.path().join("new.txt")).expect("rename");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "rename"]);
    dir
}

fn repo_with_a_utf8_path() -> TempDir {
    let dir = TempDir::new();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "t@example.com"]);
    git(dir.path(), &["config", "user.name", "Tester"]);
    std::fs::write(dir.path().join("café.txt"), "content").expect("write");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "utf8 path"]);
    dir
}

fn repo_with_a_glob_path() -> TempDir {
    let dir = TempDir::new();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "t@example.com"]);
    git(dir.path(), &["config", "user.name", "Tester"]);
    std::fs::write(dir.path().join("star*.txt"), "literal").expect("write");
    std::fs::write(dir.path().join("star-match.txt"), "wildcard").expect("write");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "glob path"]);
    dir
}

#[test]
fn reads_commits_newest_first_with_parents() {
    let dir = repo_with_a_merge();

    let commits = GitLog::commits(dir.path(), 0, 100).expect("log");

    assert_eq!(commits[0].subject, "merge side");
    assert_eq!(commits[0].parents.len(), 2, "a --no-ff merge has two parents");
    assert!(commits.iter().any(|c| c.subject == "c2 on side"),
        "--branches must include commits reachable only from other local branches");
}

#[test]
fn skip_and_limit_paginate() {
    let dir = repo_with_a_merge();

    let first = GitLog::commits(dir.path(), 0, 2).expect("log");
    let second = GitLog::commits(dir.path(), 2, 2).expect("log");

    assert_eq!(first.len(), 2);
    assert!(!second.is_empty());
    assert_ne!(first[0].sha, second[0].sha);
}

#[test]
fn a_repository_without_commits_reports_no_commits_rather_than_an_error() {
    let dir = TempDir::new();
    git(dir.path(), &["init", "-q", "-b", "main"]);

    assert!(!GitLog::has_commits(dir.path()));
    assert!(GitLog::commits(dir.path(), 0, 10).expect("log").is_empty());
}

#[test]
fn a_directory_that_is_not_a_repository_is_an_error() {
    let dir = TempDir::new();

    assert!(GitLog::commits(dir.path(), 0, 10).is_err());
}

#[test]
fn commit_files_returns_the_current_path_for_a_rename() {
    let dir = repo_with_a_rename();
    let sha = head_sha(dir.path(), "HEAD");

    let files = tiller_git::commit_files(dir.path(), &sha).expect("files");

    assert_eq!(files, vec![('R', PathBuf::from("new.txt"))]);
}

#[test]
fn commit_files_and_diff_support_merge_commits() {
    let dir = repo_with_a_merge();
    let sha = head_sha(dir.path(), "HEAD");

    let files = tiller_git::commit_files(dir.path(), &sha).expect("files");
    let diff = tiller_git::commit_diff_entry(dir.path(), &sha, Path::new("b.txt"))
        .expect("diff");

    assert!(files.iter().any(|(status, path)| {
        *status == 'A' && path == Path::new("b.txt")
    }));
    assert!(!diff.hunks.is_empty());
}

#[test]
fn commit_files_and_diff_preserve_utf8_paths() {
    let dir = repo_with_a_utf8_path();
    let sha = head_sha(dir.path(), "HEAD");
    let path = Path::new("café.txt");

    let files = tiller_git::commit_files(dir.path(), &sha).expect("files");
    let diff = tiller_git::commit_diff_entry(dir.path(), &sha, path).expect("diff");

    assert_eq!(files, vec![('A', path.to_path_buf())]);
    assert!(!diff.hunks.is_empty());
}

#[test]
fn commit_diff_entry_treats_glob_characters_as_literal() {
    let dir = repo_with_a_glob_path();
    let sha = head_sha(dir.path(), "HEAD");
    let path = Path::new("star*.txt");

    let diff = tiller_git::commit_diff_entry(dir.path(), &sha, path).expect("diff");

    assert_eq!(diff.additions, 1);
    assert_eq!(diff.hunks[0].lines[0].content, "literal");
}

#[test]
fn commit_diff_entry_ignores_forced_git_colors() {
    let dir = repo_with_a_utf8_path();
    git(dir.path(), &["config", "color.ui", "always"]);
    let sha = head_sha(dir.path(), "HEAD");

    let diff = tiller_git::commit_diff_entry(dir.path(), &sha, Path::new("café.txt"))
        .expect("diff");

    assert!(!diff.hunks.is_empty());
}

#[test]
fn lists_the_files_a_commit_touched() {
    let dir = repo_with_a_merge();
    let sha = head_sha(dir.path(), "main~1");

    let files = tiller_git::commit_files(dir.path(), &sha).expect("files");

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].0, 'A');
    assert_eq!(files[0].1, PathBuf::from("c.txt"));
}

#[test]
fn reads_the_patch_of_one_file_in_a_commit() {
    let dir = repo_with_a_merge();
    let sha = head_sha(dir.path(), "main~1");

    let diff = tiller_git::commit_diff_entry(dir.path(), &sha, Path::new("c.txt")).expect("diff");

    assert!(!diff.hunks.is_empty(), "an added file has one hunk");
}

#[test]
fn the_root_commit_diffs_against_nothing_without_erroring() {
    let dir = repo_with_a_merge();
    let sha = head_sha(dir.path(), "main^{/c1}");

    let files = tiller_git::commit_files(dir.path(), &sha).expect("files");
    let diff = tiller_git::commit_diff_entry(dir.path(), &sha, Path::new("a.txt")).expect("diff");

    assert!(files.iter().any(|(_, path)| path == Path::new("a.txt")));
    assert!(!diff.hunks.is_empty());
}
