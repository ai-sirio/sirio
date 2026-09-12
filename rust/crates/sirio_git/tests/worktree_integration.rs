//! Integration tests for worktree creation and removal against REAL
//! temporary git repositories. Everything is asserted against what
//! `git worktree list --porcelain` actually reports afterwards — not
//! against this crate's own idea of what happened.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use sirio_git::{WorktreeError, create_worktree, derive_worktree_path, remove_worktree};

/// A throwaway directory, removed on drop. Canonicalized so paths match what
/// git reports (macOS `/var` is a symlink to `/private/var`).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "sirio-worktree-test-{tag}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(std::fs::canonicalize(&path).expect("canonicalize temp dir"))
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
        // per-call `SIRIO_GIT_TIMEOUT_MS` lookup, and a wider budget can
        // only turn a would-be timeout into a pass — never the reverse.
        unsafe { std::env::set_var("SIRIO_GIT_TIMEOUT_MS", TEST_GIT_TIMEOUT_MS) };
    });
}

/// Runs `git <args>` in `dir`, returning stdout.
fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git must be installed to run these tests");
    assert!(
        output.status.success(),
        "`git {args:?}` failed in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The spelling git uses for `path`: forward slashes, no verbatim prefix
/// (`C:/Users/...`), while the fixtures hold canonicalized `\\?\C:\...`
/// paths.
///
/// This is both what `git worktree list --porcelain` prints — so
/// [`porcelain_branch`] normalizes before comparing — and the only spelling
/// git accepts *as an argument*: handed a verbatim path it refuses with
/// `could not create leading directories of '//?/C:/...': Invalid
/// argument`. The production code strips the prefix itself
/// (`sirio_git::git::path_arg`), so every fixture that shells out to the
/// bare [`git`] helper with a fixture path must do the same here.
fn porcelain_spelling(path: &Path) -> String {
    #[cfg(windows)]
    let mut spelling = path.display().to_string();
    #[cfg(not(windows))]
    let spelling = path.display().to_string();
    #[cfg(windows)]
    if let Some(rest) = spelling.strip_prefix(r"\\?\") {
        spelling = rest.to_string();
    }
    spelling.replace('\\', "/")
}

fn porcelain_branch(repo: &Path, path: &Path) -> Option<String> {
    let output = git_stdout(repo, &["worktree", "list", "--porcelain"]);
    let mut current_path: Option<&str> = None;
    for line in output.lines() {
        if let Some(worktree_path) = line.strip_prefix("worktree ") {
            current_path = Some(worktree_path);
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/")
            && current_path == Some(porcelain_spelling(path).as_str())
        {
            return Some(branch.to_string());
        }
    }
    None
}

fn porcelain_worktree_count(repo: &Path) -> usize {
    git_stdout(repo, &["worktree", "list", "--porcelain"])
        .lines()
        .filter(|line| line.starts_with("worktree "))
        .count()
}

/// Creates a git repo with one commit on `main`.
///
/// The identity is written into the repo's own config, not passed as `-c`
/// on the first commit alone: `created_worktree_survives_a_porcelain_round_trip_with_base`
/// makes a *second* commit through the bare `git` helper, and on a machine
/// with no global git identity (this one) git refuses it with "Author
/// identity unknown" and the test dies in its fixture rather than in the
/// code under test. Same defect, same fix, as `git_integration.rs`.
fn make_repo(tag: &str) -> TempDir {
    let repo = TempDir::new(tag);
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.email", "test@sirio.dev"]);
    git(repo.path(), &["config", "user.name", "Sirio Test"]);
    std::fs::write(repo.path().join("file.txt"), "one\ntwo\nthree\n").expect("write file");
    git(repo.path(), &["add", "-A"]);
    git(
        repo.path(),
        &[
            "-c",
            "user.email=test@sirio.dev",
            "-c",
            "user.name=Sirio Test",
            "commit",
            "-m",
            "root",
        ],
    );
    repo
}

#[test]
fn creates_a_worktree_on_a_new_branch() {
    let repo = make_repo("create");
    let parent = repo.path().parent().expect("parent");
    let project_name = repo.path().file_name().unwrap().to_string_lossy();
    let branch = "feature-x";
    let path = derive_worktree_path(parent, &project_name, branch);
    let _ = std::fs::remove_dir_all(&path);

    create_worktree(repo.path(), branch, &path, None).expect("create worktree");

    assert_eq!(
        porcelain_branch(repo.path(), &path).as_deref(),
        Some(branch),
        "git worktree list --porcelain reports the new worktree on the new branch"
    );
    assert!(
        path.join("file.txt").exists(),
        "the checkout has the repo's files"
    );
}

#[test]
fn attaches_to_an_existing_branch_without_a_worktree() {
    let repo = make_repo("attach");
    // Create a branch that is not checked out anywhere.
    git(repo.path(), &["branch", "existing-branch"]);
    let path = repo.path().with_extension("wt-existing");

    create_worktree(repo.path(), "existing-branch", &path, None)
        .expect("attach to existing branch");

    assert_eq!(
        porcelain_branch(repo.path(), &path).as_deref(),
        Some("existing-branch")
    );
}

#[test]
fn branch_that_already_has_a_worktree_is_refused_clearly() {
    let repo = make_repo("dupe");
    let first = repo.path().with_extension("wt-first");
    create_worktree(repo.path(), "feature-x", &first, None).expect("first worktree");

    let second = repo.path().with_extension("wt-second");
    let error = create_worktree(repo.path(), "feature-x", &second, None)
        .expect_err("a second worktree for the same branch must be refused");

    assert!(
        matches!(
            error,
            WorktreeError::BranchAlreadyCheckedOut { ref branch, .. } if branch == "feature-x"
        ),
        "the refusal names the branch: {error}"
    );

    // Nothing broken was created: porcelain still reports exactly one
    // worktree for that branch, and the second path does not exist.
    assert_eq!(
        porcelain_worktree_count(repo.path()),
        2,
        "main + the one worktree"
    );
    assert_eq!(
        porcelain_branch(repo.path(), &first).as_deref(),
        Some("feature-x")
    );
    assert!(!second.exists(), "no broken second checkout was created");
}

#[test]
fn branch_with_a_slash_becomes_a_nested_directory() {
    let repo = make_repo("slash");
    let parent = repo.path().parent().expect("parent");
    let project_name = repo.path().file_name().unwrap().to_string_lossy();
    let branch = "feature/login";
    let path = derive_worktree_path(parent, &project_name, branch);

    create_worktree(repo.path(), branch, &path, None).expect("create with a slash");

    assert!(
        path.join("file.txt").exists(),
        "the nested checkout exists at {}",
        path.display()
    );
    assert_eq!(
        porcelain_branch(repo.path(), &path).as_deref(),
        Some("feature/login"),
        "porcelain reports the slash branch on the nested path"
    );
}

#[test]
fn branch_with_a_space_is_refused_by_git_and_creates_nothing() {
    let repo = make_repo("space");
    let parent = repo.path().parent().expect("parent");
    let project_name = repo.path().file_name().unwrap().to_string_lossy();
    let branch = "café branch"; // spaces are invalid in git branch names
    let path = derive_worktree_path(parent, &project_name, branch);

    let error = create_worktree(repo.path(), branch, &path, None)
        .expect_err("git rejects a branch name with a space");

    assert!(
        matches!(error, WorktreeError::Git(_)),
        "git's own refusal is surfaced, name unmangled: {error}"
    );
    assert!(!path.exists(), "no broken checkout was created");
    assert_eq!(porcelain_worktree_count(repo.path()), 1);
}

#[test]
fn branch_with_non_ascii_characters_is_created_raw() {
    let repo = make_repo("unicode");
    let parent = repo.path().parent().expect("parent");
    let project_name = repo.path().file_name().unwrap().to_string_lossy();
    let branch = "café-branch"; // non-ASCII is legal; spaces are not
    let path = derive_worktree_path(parent, &project_name, branch);

    create_worktree(repo.path(), branch, &path, None).expect("create with non-ASCII");

    assert!(path.exists());
    assert_eq!(
        porcelain_branch(repo.path(), &path).as_deref(),
        Some("café-branch"),
        "porcelain reports the raw branch name"
    );
}

#[test]
fn unborn_head_creates_a_valid_worktree_with_an_unborn_branch() {
    // git supports creating a worktree from a repository with no commits:
    // the new branch is simply unborn too (all-zero HEAD in porcelain).
    let repo = TempDir::new("unborn");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.email", "test@sirio.dev"]);
    git(repo.path(), &["config", "user.name", "Sirio Test"]);

    let path = repo.path().with_extension("wt-unborn");
    create_worktree(repo.path(), "feature-x", &path, None).expect("unborn HEAD creation");

    let output = git_stdout(repo.path(), &["worktree", "list", "--porcelain"]);
    assert!(
        output.contains(&format!(
            "worktree {}\nHEAD 0000000",
            porcelain_spelling(&path)
        )),
        "the unborn worktree is reported: {output}"
    );
    assert_eq!(
        porcelain_branch(repo.path(), &path).as_deref(),
        Some("feature-x")
    );
}

#[test]
fn remove_worktree_removes_it_from_porcelain() {
    let repo = make_repo("remove");
    let path = repo.path().with_extension("wt-remove");
    create_worktree(repo.path(), "feature-x", &path, None).expect("create");
    assert_eq!(porcelain_worktree_count(repo.path()), 2);

    remove_worktree(repo.path(), &path, "feature-x").expect("remove");

    assert_eq!(
        porcelain_worktree_count(repo.path()),
        1,
        "git worktree list --porcelain no longer reports the removed worktree"
    );
    assert!(!path.exists(), "the checkout directory is gone");
}

#[test]
fn remove_worktree_survives_a_renamed_main_repository() {
    // The main repository is renamed on disk after the worktree was added
    // (what happened when `tiller-rust-gpui` became `sirio`). The worktree's
    // `.git` file still names the old location, and `git worktree remove`
    // refuses with "is not a .git file, error code 7" — even under
    // `--force`, since the link is validated before anything else.
    let original = make_repo("remove-renamed");
    let path = original.path().with_extension("wt-remove-renamed");
    create_worktree(original.path(), "feature-x", &path, None).expect("create");

    let renamed = TempDir(original.path().with_extension("renamed"));
    std::fs::rename(original.path(), renamed.path()).expect("rename the main repository");

    remove_worktree(renamed.path(), &path, "feature-x").expect("remove after the rename");

    assert_eq!(
        porcelain_worktree_count(renamed.path()),
        1,
        "git worktree list --porcelain no longer reports the removed worktree"
    );
    assert!(!path.exists(), "the checkout directory is gone");
}

#[test]
fn remove_worktree_deletes_its_branch() {
    let repo = make_repo("remove-branch");
    let path = repo.path().with_extension("wt-remove-branch");
    create_worktree(repo.path(), "feature-x", &path, None).expect("create");

    remove_worktree(repo.path(), &path, "feature-x").expect("remove");

    let branches = git_stdout(
        repo.path(),
        &["branch", "--list", "--format=%(refname:short)"],
    );
    assert!(
        !branches.lines().any(|branch| branch == "feature-x"),
        "removing the worktree also deletes its branch: {branches}"
    );
}

#[test]
fn remove_worktree_keeps_branch_when_deletion_is_refused() {
    let repo = make_repo("remove-branch-refused");
    let path = repo.path().with_extension("wt-remove-branch-refused");
    create_worktree(repo.path(), "feature-x", &path, None).expect("create");

    // A second worktree normally cannot check out the same branch. Point its
    // symbolic HEAD at the branch directly so git branch -D has a real
    // linked-worktree checkout to refuse after the target is removed.
    let other = repo.path().with_extension("wt-other");
    let other_arg = porcelain_spelling(&other);
    git(repo.path(), &["worktree", "add", "--detach", &other_arg]);
    git(&other, &["symbolic-ref", "HEAD", "refs/heads/feature-x"]);

    remove_worktree(repo.path(), &path, "feature-x").expect("worktree removal succeeds");

    assert!(!path.exists(), "the target checkout directory is gone");
    assert_eq!(
        porcelain_branch(repo.path(), &other).as_deref(),
        Some("feature-x"),
        "the second worktree still checks out the branch"
    );
    let branches = git_stdout(
        repo.path(),
        &["branch", "--list", "--format=%(refname:short)"],
    );
    assert!(
        branches.lines().any(|branch| branch == "feature-x"),
        "the branch survives git's refusal: {branches}"
    );
}

#[test]
fn remove_forces_through_uncommitted_changes() {
    let repo = make_repo("dirty");
    let path = repo.path().with_extension("wt-dirty");
    create_worktree(repo.path(), "feature-x", &path, None).expect("create");
    std::fs::write(path.join("uncommitted.txt"), "work in progress\n").expect("write");

    // The confirmation dialog in the UI is the safety net now: after it,
    // removal must go through even with uncommitted work in the checkout.
    remove_worktree(repo.path(), &path, "feature-x").expect("removal is forced through");

    assert!(!path.exists(), "the dirty checkout directory is gone");
    assert_eq!(
        porcelain_worktree_count(repo.path()),
        1,
        "only the primary worktree remains"
    );
}

#[test]
fn remove_deletes_the_directory_when_git_refuses() {
    let repo = make_repo("locked");
    let path = repo.path().with_extension("wt-locked");
    create_worktree(repo.path(), "feature-x", &path, None).expect("create");
    // A locked worktree is one thing a single `--force` cannot remove, so
    // git's refusal is deterministic here; the fallback must still take the
    // checkout off disk.
    let path_arg = porcelain_spelling(&path);
    git(repo.path(), &["worktree", "lock", &path_arg]);

    remove_worktree(repo.path(), &path, "feature-x").expect("the fallback removes the checkout");

    assert!(!path.exists(), "the locked checkout directory is gone");
}

#[test]
fn remove_never_deletes_a_directory_that_is_not_a_checkout() {
    let repo = make_repo("not-a-checkout");
    // A plain directory with a file in it, never registered as a worktree:
    // git refuses, and the disk fallback must refuse too instead of
    // `rm -rf`-ing whatever a stale sidebar row happened to point at.
    let path = repo.path().with_extension("plain-dir");
    std::fs::create_dir_all(&path).expect("create dir");
    std::fs::write(path.join("keep.txt"), "not yours\n").expect("write");

    let error = remove_worktree(repo.path(), &path, "feature-x")
        .expect_err("a directory git does not know is left alone");

    assert!(matches!(error, WorktreeError::Git(_)), "{error}");
    assert!(
        path.join("keep.txt").exists(),
        "the unrelated directory and its contents survive"
    );
    std::fs::remove_dir_all(&path).expect("cleanup");
}

#[test]
fn remove_never_deletes_another_repositorys_checkout() {
    // The disk fallback runs *because* git refused, and the commonest
    // reason for a refusal is that the path is not this repository's
    // worktree at all. "Is it a linked checkout" is therefore not enough
    // on its own: a checkout of a different repository answers yes.
    let ours = make_repo("foreign-ours");
    let theirs = make_repo("foreign-theirs");
    let foreign = theirs.path().with_extension("wt-foreign");
    create_worktree(theirs.path(), "feature-x", &foreign, None).expect("create");
    std::fs::write(
        foreign.join("their-work.txt"),
        "someone else's work
",
    )
    .expect("write");
    assert!(
        foreign.join(".git").is_file(),
        "the fixture must pass the weaker 'is a linked checkout' test"
    );

    let error = remove_worktree(ours.path(), &foreign, "feature-x")
        .expect_err("another repository's checkout is not ours to delete");

    assert!(matches!(error, WorktreeError::Git(_)), "{error}");
    assert!(
        foreign.join("their-work.txt").exists(),
        "the other repository's checkout and its uncommitted work survive"
    );
    let _ = std::fs::remove_dir_all(&foreign);
}

#[test]
fn remove_never_deletes_a_checkout_that_holds_the_repository() {
    // A checkout that contains the repository would take the repository
    // with it. Nothing creates this shape on purpose; the guard is there
    // because the fallback is the one path that deletes recursively.
    let repo = make_repo("contains-repo");
    let parent = repo
        .path()
        .parent()
        .expect("repo has a parent")
        .to_path_buf();
    std::fs::write(
        parent.join(".git"),
        format!(
            "gitdir: {}
",
            repo.path().join(".git").display()
        ),
    )
    .expect("write a checkout marker above the repository");

    let error = remove_worktree(repo.path(), &parent, "feature-x")
        .expect_err("a directory holding the repository is never deleted");

    assert!(matches!(error, WorktreeError::Git(_)), "{error}");
    assert!(repo.path().exists(), "the repository survives");
    let _ = std::fs::remove_file(parent.join(".git"));
}

#[test]
fn non_git_directory_is_refused() {
    let dir = TempDir::new("notgit");
    let path = dir.path().with_extension("wt");
    let error = create_worktree(dir.path(), "feature-x", &path, None)
        .expect_err("a non-git directory has no worktrees");
    assert!(
        matches!(error, WorktreeError::Git(_)),
        "git's refusal is surfaced: {error}"
    );
    assert!(!path.exists());
}

#[test]
fn created_worktree_survives_a_porcelain_round_trip_with_base() {
    let repo = make_repo("base");
    // Advance main so a base matters.
    std::fs::write(repo.path().join("file.txt"), "one\ntwo\nthree\nfour\n").expect("write");
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-m", "second"]);

    let path = repo.path().with_extension("wt-based");
    create_worktree(repo.path(), "based", &path, Some("main")).expect("create with base");

    let based_head = git_stdout(repo.path(), &["log", "-1", "--format=%H", "based"]);
    let main_head = git_stdout(repo.path(), &["log", "-1", "--format=%H", "main"]);
    assert_eq!(
        based_head.trim(),
        main_head.trim(),
        "the branch was created from the given base"
    );
}
