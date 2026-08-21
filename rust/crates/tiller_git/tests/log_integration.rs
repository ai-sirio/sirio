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
