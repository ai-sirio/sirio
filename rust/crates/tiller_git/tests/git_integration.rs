//! Integration tests against real temporary git repositories. These build
//! repos with `git init` and real commits, then assert on what the crate
//! reports — exactly the pipeline the Changes panel runs.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use tiller_git::{
    DiffOrigin, diff_entry, discard, discard_all, has_head, parse_status, stage, stage_all, stats,
    status, unstage,
};

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
/// [`git`] (directly or via `make_repo`) before touching the crate API, so
/// this one call site covers all of them.
fn ensure_generous_timeout() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // SAFETY: test process, set once. The only reader is the crate's
        // per-call `TILLER_GIT_TIMEOUT_MS` lookup, and a wider budget can
        // only turn a would-be timeout into a pass — never the reverse.
        unsafe { std::env::set_var("TILLER_GIT_TIMEOUT_MS", TEST_GIT_TIMEOUT_MS) };
    });
}

/// Writes a file inside the repo.
fn write(repo: &Path, relative: &str, content: &[u8]) -> PathBuf {
    let path = repo.join(relative);
    std::fs::write(&path, content).expect("write fixture file");
    path
}

/// Creates a git repo with one commit on `main` containing `file.txt`.
/// The identity is passed as `-c` config on the commit itself, so no
/// `git config` calls (and their fsyncs) are needed: two fewer process
/// spawns per repo, which matters when the machine is under load.
fn make_repo() -> TempDir {
    let repo = TempDir::new();
    git(repo.path(), &["init", "-b", "main"]);
    write(repo.path(), "file.txt", b"one\ntwo\nthree\n");
    git(repo.path(), &["add", "-A"]);
    git(
        repo.path(),
        &[
            "-c",
            "user.email=test@tiller.dev",
            "-c",
            "user.name=Tiller Test",
            "commit",
            "-m",
            "root",
        ],
    );
    repo
}

/// Creates a repository with two files that both conflict when `side` is
/// merged into `main`.
fn make_conflicted_repo() -> TempDir {
    let repo = TempDir::new();
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.email", "test@tiller.dev"]);
    git(repo.path(), &["config", "user.name", "Tiller Test"]);
    write(repo.path(), "f.txt", b"base f\n");
    write(repo.path(), "g.txt", b"base g\n");
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-m", "base"]);

    git(repo.path(), &["checkout", "-b", "side"]);
    write(repo.path(), "f.txt", b"side f\n");
    write(repo.path(), "g.txt", b"side g\n");
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-m", "side"]);

    git(repo.path(), &["checkout", "main"]);
    write(repo.path(), "f.txt", b"main f\n");
    write(repo.path(), "g.txt", b"main g\n");
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-m", "main"]);

    ensure_generous_timeout();
    let output = Command::new("git")
        .args(["merge", "side"])
        .current_dir(repo.path())
        .output()
        .expect("git merge runs");
    assert_eq!(output.status.code(), Some(1), "merge conflicts");
    repo
}

fn status_porcelain(repo: &Path) -> String {
    ensure_generous_timeout();
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo)
        .output()
        .expect("git status runs");
    assert!(output.status.success(), "git status succeeds");
    String::from_utf8(output.stdout).expect("git status is utf-8")
}

/// The single status entry for `path`, panicking if absent or not unique.
fn entry_for<'a>(
    snapshot: &'a tiller_git::StatusSnapshot,
    path: &str,
) -> &'a tiller_git::StatusEntry {
    let matches: Vec<_> = snapshot
        .entries
        .iter()
        .filter(|entry| entry.path == Path::new(path))
        .collect();
    assert_eq!(matches.len(), 1, "expected exactly one entry for {path}");
    matches[0]
}

#[test]
fn status_and_diff_handle_spaces_and_non_ascii_paths() {
    let repo = make_repo();

    // Commit files with a space and non-ASCII in the name, then modify them.
    write(repo.path(), "file one.txt", b"original\n");
    write(repo.path(), "café-ünïcode.txt", "original\n".as_bytes());
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-m", "odd names"]);
    write(repo.path(), "file one.txt", b"changed\n");
    write(repo.path(), "café-ünïcode.txt", "näive\n".as_bytes());

    let snapshot = status(repo.path()).expect("status runs");
    assert_eq!(snapshot.entries.len(), 2, "both changed files are reported");

    let spaced = entry_for(&snapshot, "file one.txt");
    assert_eq!(
        spaced.worktree_status,
        Some(tiller_git::StatusKind::Modified)
    );

    let non_ascii = entry_for(&snapshot, "café-ünïcode.txt");
    assert_eq!(
        non_ascii.worktree_status,
        Some(tiller_git::StatusKind::Modified)
    );

    let diff =
        diff_entry(repo.path(), spaced, tiller_git::DEFAULT_CONTEXT_LINES).expect("diff runs");
    assert_eq!(diff.hunks.len(), 1);
    assert_eq!(diff.additions, 1);
    assert_eq!(diff.deletions, 1);

    let counts = stats(repo.path(), &snapshot.entries).expect("stats run");
    assert_eq!(counts[&PathBuf::from("file one.txt")].additions, 1);
    assert_eq!(counts[&PathBuf::from("café-ünïcode.txt")].deletions, 1);
}

#[test]
fn rename_reports_old_and_new_path() {
    let repo = make_repo();
    write(repo.path(), "file.txt", b"one\ntwo\nthree\n");
    git(repo.path(), &["mv", "file.txt", "renamed file.txt"]);

    let snapshot = status(repo.path()).expect("status runs");
    let rename = entry_for(&snapshot, "renamed file.txt");
    assert_eq!(rename.index_status, Some(tiller_git::StatusKind::Renamed));
    assert_eq!(
        rename.original_path.as_deref(),
        Some(Path::new("file.txt")),
        "the rename source is preserved"
    );

    // Diff with both paths: git emits `rename from/to`, not delete+add.
    let diff =
        diff_entry(repo.path(), rename, tiller_git::DEFAULT_CONTEXT_LINES).expect("diff runs");
    assert!(diff.hunks.is_empty(), "a pure rename has no hunks");
    assert_eq!(diff.additions, 0);

    // Numstat keys the stat by the NEW path.
    let counts = stats(repo.path(), &snapshot.entries).expect("stats run");
    assert!(counts.contains_key(&PathBuf::from("renamed file.txt")));
}

#[test]
fn staged_and_worktree_modified_reports_two_states() {
    let repo = make_repo();

    write(repo.path(), "file.txt", b"staged version\n");
    git(repo.path(), &["add", "file.txt"]);
    write(repo.path(), "file.txt", b"staged version\nworktree extra\n");

    let snapshot = status(repo.path()).expect("status runs");
    let entry = entry_for(&snapshot, "file.txt");

    assert_eq!(entry.index_status, Some(tiller_git::StatusKind::Modified));
    assert_eq!(
        entry.worktree_status,
        Some(tiller_git::StatusKind::Modified)
    );
    assert!(entry.is_staged(), "staged part is present");
    assert!(entry.has_worktree_changes(), "worktree part is present");

    // The diff against HEAD shows both changes in one view: the old three
    // lines are replaced by the two new lines.
    let diff =
        diff_entry(repo.path(), entry, tiller_git::DEFAULT_CONTEXT_LINES).expect("diff runs");
    assert_eq!(diff.additions, 2, "one staged line plus one worktree line");
    assert_eq!(diff.deletions, 3, "the original three lines are gone");
}

#[test]
fn untracked_file_has_no_head_diff() {
    let repo = make_repo();
    write(repo.path(), "brand new.txt", b"hello\nworld\n");

    let snapshot = status(repo.path()).expect("status runs");
    let untracked = entry_for(&snapshot, "brand new.txt");
    assert!(untracked.is_untracked());
    assert_eq!(snapshot.untracked().len(), 1);

    // Diffed against /dev/null: every line is an addition.
    let diff =
        diff_entry(repo.path(), untracked, tiller_git::DEFAULT_CONTEXT_LINES).expect("diff runs");
    assert_eq!(diff.additions, 2);
    assert_eq!(diff.deletions, 0);
    assert!(
        diff.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .all(|line| line.origin == DiffOrigin::Addition)
    );

    // Line counts come from disk for untracked files.
    let counts = stats(repo.path(), &snapshot.entries).expect("stats run");
    assert_eq!(counts[&PathBuf::from("brand new.txt")].additions, 2);
}

#[test]
fn binary_file_reports_no_line_counts() {
    let repo = make_repo();
    write(repo.path(), "blob.bin", b"version-one\0with-nul-bytes");
    git(repo.path(), &["add", "blob.bin"]);
    git(repo.path(), &["commit", "-m", "add binary"]);
    write(repo.path(), "blob.bin", b"version-two\0with-nul-bytes");

    let snapshot = status(repo.path()).expect("status runs");
    let binary = entry_for(&snapshot, "blob.bin");

    let diff =
        diff_entry(repo.path(), binary, tiller_git::DEFAULT_CONTEXT_LINES).expect("diff runs");
    assert!(diff.is_binary, "binary diff is marked");
    assert!(diff.hunks.is_empty());
    assert_eq!(diff.additions, 0);

    let counts = stats(repo.path(), &snapshot.entries).expect("stats run");
    let stat = &counts[&PathBuf::from("blob.bin")];
    assert!(stat.is_binary);
    assert_eq!(stat.additions, 0);
    assert_eq!(stat.deletions, 0);
}

#[test]
fn unborn_head_reports_staged_and_untracked_and_diffs_against_dev_null() {
    let repo = TempDir::new();
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.email", "test@tiller.dev"]);
    git(repo.path(), &["config", "user.name", "Tiller Test"]);

    assert!(!has_head(repo.path()), "nothing committed yet");

    write(repo.path(), "new.txt", b"fresh\n");
    git(repo.path(), &["add", "new.txt"]);
    write(repo.path(), "untracked.txt", b"loose\n");

    let snapshot = status(repo.path()).expect("status runs");
    let staged = entry_for(&snapshot, "new.txt");
    assert_eq!(staged.index_status, Some(tiller_git::StatusKind::Added));
    assert!(staged.is_staged());

    let untracked = entry_for(&snapshot, "untracked.txt");
    assert!(untracked.is_untracked());

    // No HEAD: both diff against /dev/null via --no-index.
    let diff =
        diff_entry(repo.path(), staged, tiller_git::DEFAULT_CONTEXT_LINES).expect("diff runs");
    assert_eq!(diff.additions, 1);

    // Stats skip the failing `diff HEAD` and count untracked from disk.
    let counts = stats(repo.path(), &snapshot.entries).expect("stats run");
    assert_eq!(counts[&PathBuf::from("untracked.txt")].additions, 1);

    // Unstage on an unborn HEAD removes the file from the index.
    unstage(repo.path(), &PathBuf::from("new.txt")).expect("unstage works without HEAD");
    let after = status(repo.path()).expect("status runs");
    let now_untracked = entry_for(&after, "new.txt");
    assert!(now_untracked.is_untracked());
    assert!(!now_untracked.is_staged());
}

#[test]
fn crlf_files_diff_cleanly() {
    let repo = make_repo();
    git(repo.path(), &["config", "core.autocrlf", "false"]);
    write(repo.path(), "crlf.txt", b"a\r\nb\r\nc\r\n");
    git(repo.path(), &["add", "crlf.txt"]);
    git(repo.path(), &["commit", "-m", "crlf file"]);
    write(repo.path(), "crlf.txt", b"a\r\nB\r\nc\r\nd\r\n");

    let snapshot = status(repo.path()).expect("status runs");
    let entry = entry_for(&snapshot, "crlf.txt");

    let diff =
        diff_entry(repo.path(), entry, tiller_git::DEFAULT_CONTEXT_LINES).expect("diff runs");
    let hunk = &diff.hunks[0];
    assert_eq!(hunk.lines.len(), 5, "a, -b, +B, c, +d");
    for line in &hunk.lines {
        assert!(!line.content.contains('\r'), "CR is stripped from content");
    }
    assert_eq!(hunk.lines[1].content, "b");
    assert_eq!(hunk.lines[2].content, "B");
    assert_eq!(diff.additions, 2);
    assert_eq!(diff.deletions, 1);
}

#[test]
fn conflicted_file_is_reported_as_unmerged() {
    let repo = make_conflicted_repo();

    let snapshot = status(repo.path()).expect("status runs");
    let entry = entry_for(&snapshot, "f.txt");
    assert!(entry.is_conflicted());
    assert_eq!(entry.index_status, Some(tiller_git::StatusKind::Unmerged));
}

#[test]
fn stage_refuses_a_conflicted_path_without_changing_git() {
    let repo = make_conflicted_repo();
    let before = status_porcelain(repo.path());

    let error = stage(repo.path(), Path::new("f.txt")).expect_err("conflict must be refused");

    assert_eq!(
        error,
        tiller_git::GitError::ConflictedPaths {
            paths: vec![PathBuf::from("f.txt")],
        }
    );
    assert_eq!(status_porcelain(repo.path()), before);
}

#[test]
fn stage_all_refuses_every_conflicted_path_before_mutation() {
    let repo = make_conflicted_repo();
    let before = status_porcelain(repo.path());

    let error = stage_all(repo.path()).expect_err("batch conflict must be refused");

    assert_eq!(
        error,
        tiller_git::GitError::ConflictedPaths {
            paths: vec![PathBuf::from("f.txt"), PathBuf::from("g.txt")],
        }
    );
    assert_eq!(status_porcelain(repo.path()), before);
}

#[test]
fn actions_stage_unstage_and_discard_single_paths() {
    let repo = make_repo();

    // Stage a new file and an edit.
    write(repo.path(), "added.txt", b"new\n");
    write(repo.path(), "file.txt", b"one\nTWO\nthree\n");
    stage(repo.path(), &PathBuf::from("added.txt")).expect("stage new file");
    stage(repo.path(), &PathBuf::from("file.txt")).expect("stage edit");

    let snapshot = status(repo.path()).expect("status runs");
    assert_eq!(snapshot.staged().len(), 2);
    assert!(snapshot.changes().is_empty(), "everything is staged");

    // Unstage one path.
    unstage(repo.path(), &PathBuf::from("file.txt")).expect("unstage");
    let snapshot = status(repo.path()).expect("status runs");
    assert_eq!(snapshot.staged().len(), 1);
    assert_eq!(snapshot.changes().len(), 1);

    // Discard the unstaged worktree change.
    discard(repo.path(), &PathBuf::from("file.txt")).expect("discard");
    let snapshot = status(repo.path()).expect("status runs");
    assert_eq!(snapshot.staged().len(), 1, "only added.txt remains staged");
    assert!(
        snapshot.changes().is_empty(),
        "worktree change was discarded"
    );
}

#[test]
fn actions_stage_all_and_discard_all() {
    let repo = make_repo();

    write(repo.path(), "file.txt", b"one\ntwo\nthree\nfour\n");
    write(repo.path(), "new file.txt", b"new\n");
    stage_all(repo.path()).expect("stage all");

    let snapshot = status(repo.path()).expect("status runs");
    assert!(snapshot.changes().is_empty(), "all changes staged");
    assert_eq!(snapshot.staged().len(), 2);

    // Unstage everything, then discard all worktree changes.
    for entry in snapshot.staged() {
        unstage(repo.path(), &entry.path).expect("unstage");
    }
    let snapshot = status(repo.path()).expect("status runs");
    assert_eq!(snapshot.changes().len(), 1, "file.txt is modified again");
    assert_eq!(snapshot.untracked().len(), 1, "new file.txt is untracked");

    discard_all(repo.path()).expect("discard all");
    let snapshot = status(repo.path()).expect("status runs");
    // Only the untracked file remains — discard_all never touches untracked.
    assert_eq!(snapshot.changes().len(), 0);
    assert_eq!(snapshot.untracked().len(), 1);
}

#[test]
fn parse_status_round_trips_real_porcelain_output() {
    let repo = make_repo();
    write(repo.path(), "file one.txt", b"edited\n");
    write(repo.path(), "untracked.txt", b"new\n");

    // Capture real git output and feed it straight to the parser, proving the
    // parser and the shell-out agree byte for byte.
    let output = Command::new("git")
        .args(["status", "--porcelain=v2", "-z", "--untracked-files=all"])
        .current_dir(repo.path())
        .output()
        .expect("git status runs");
    assert!(output.status.success());
    let parsed = parse_status(&output.stdout).expect("parser accepts real output");

    let snapshot = status(repo.path()).expect("shell-out path agrees");
    assert_eq!(parsed, snapshot);
}
