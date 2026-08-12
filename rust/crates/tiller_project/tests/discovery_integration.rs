//! Integration tests for real git discovery: these create throwaway
//! repositories on disk with `git init` and exercise the full shell-out +
//! parse pipeline, exactly as the Swift app's `TillerGitTests` do.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use tiller_project::{
    Workspace, current_branch, discover_project, discover_worktrees, is_git_repository,
    parse_worktree_list,
};

/// A throwaway directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tiller-project-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        // Canonicalize so paths match what git reports: on macOS `/var` is a
        // symlink to `/private/var`, and `git worktree list` prints the real
        // path.
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

/// Runs `git <args>` in `dir`, asserting success (like the Swift tests'
/// `runGitForTest`).
fn git(dir: &Path, args: &[&str]) {
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

/// Runs `git <args>` in `dir` and returns stdout.
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

/// Creates a throwaway git repo with one commit on `main`.
fn make_git_repo() -> TempDir {
    let dir = TempDir::new();
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "test@tiller.dev"]);
    git(dir.path(), &["config", "user.name", "Tiller Test"]);
    std::fs::write(dir.path().join("file.txt"), "one\n").expect("write fixture file");
    git(dir.path(), &["add", "--", "file.txt"]);
    git(dir.path(), &["commit", "-m", "root"]);
    dir
}

#[test]
fn non_git_directory_is_reported_as_non_git() {
    let dir = TempDir::new();

    assert!(
        !is_git_repository(dir.path()),
        "plain dir has no .git entry"
    );

    let discovered = discover_project(dir.path()).expect("non-git discovery succeeds");
    assert!(!discovered.is_git);
    assert!(
        discovered.worktrees.is_empty(),
        "no worktrees for a non-git dir"
    );

    let mut workspace = Workspace::new();
    let project_id = workspace
        .load_project(dir.path())
        .expect("loading a non-git dir succeeds");
    let project = workspace.project(project_id).expect("project is present");
    assert!(!project.is_git);
    assert_eq!(project.root_path, dir.path());
    assert_eq!(
        project.name,
        dir.path().file_name().unwrap().to_string_lossy()
    );
    assert_eq!(workspace.worktrees_of(project_id).count(), 0);
}

#[test]
fn discovers_primary_checkout_of_real_repo() {
    let repo = make_git_repo();

    assert!(is_git_repository(repo.path()));

    let discovered = discover_project(repo.path()).expect("discovery succeeds");
    assert!(discovered.is_git);
    assert_eq!(discovered.worktrees.len(), 1);

    let main = &discovered.worktrees[0];
    assert!(main.is_primary, "the only worktree is the primary checkout");
    assert_eq!(main.path, repo.path());
    assert_eq!(main.branch.as_deref(), Some("main"));
    assert!(!main.locked);
    assert!(!main.prunable);

    assert_eq!(
        current_branch(repo.path()).expect("current branch resolves"),
        Some("main".to_string()),
        "current_branch reports the branch of the primary checkout"
    );
}

#[test]
fn discovers_linked_worktree_with_branch() {
    let repo = make_git_repo();
    let linked = repo.path().with_extension("wt-feature");
    git(
        repo.path(),
        &["worktree", "add", "-b", "feature", linked.to_str().unwrap()],
    );

    let worktrees = discover_worktrees(repo.path()).expect("discovery succeeds");
    assert_eq!(worktrees.len(), 2, "primary + linked");

    assert!(worktrees[0].is_primary);
    assert_eq!(worktrees[0].branch.as_deref(), Some("main"));

    assert!(!worktrees[1].is_primary);
    assert_eq!(worktrees[1].path, linked);
    assert_eq!(worktrees[1].branch.as_deref(), Some("feature"));

    assert_eq!(
        current_branch(&linked).expect("current branch resolves"),
        Some("feature".to_string()),
        "current_branch works from a linked checkout"
    );
}

#[test]
fn detached_head_is_reported_without_a_branch() {
    let repo = make_git_repo();
    git(repo.path(), &["checkout", "--detach", "HEAD"]);

    let worktrees = discover_worktrees(repo.path()).expect("discovery succeeds");
    let main = &worktrees[0];
    assert!(main.is_primary);
    assert_eq!(main.branch, None, "detached HEAD has no branch");
    assert!(main.head.is_some(), "the commit id is still reported");

    assert_eq!(
        current_branch(repo.path()).expect("current branch resolves"),
        None,
        "current_branch is None on a detached HEAD"
    );

    // Feed the real porcelain output through the parser end to end.
    let real_output = git_stdout(repo.path(), &["worktree", "list", "--porcelain"]);
    let parsed = parse_worktree_list(&real_output);
    assert_eq!(parsed.len(), 1);
    assert!(parsed[0].is_primary);
    assert_eq!(
        parsed[0].branch, None,
        "real porcelain marks the detached entry"
    );
}

#[test]
fn parses_real_porcelain_output_with_prunable_entry() {
    let repo = make_git_repo();
    let doomed = repo.path().with_extension("wt-doomed");
    git(
        repo.path(),
        &["worktree", "add", "-b", "doomed", doomed.to_str().unwrap()],
    );

    // Remove the checkout directory but leave the admin metadata: git now
    // reports the entry as prunable.
    std::fs::remove_dir_all(&doomed).expect("remove worktree checkout");

    let real_output = git_stdout(repo.path(), &["worktree", "list", "--porcelain"]);
    let parsed = parse_worktree_list(&real_output);

    assert_eq!(parsed.len(), 2);
    let prunable = parsed
        .iter()
        .find(|entry| entry.path == doomed)
        .expect("doomed worktree still listed");
    assert!(
        prunable.prunable,
        "real porcelain marks the entry as prunable"
    );
    assert!(!prunable.is_primary);
    assert!(!parsed[0].prunable, "primary checkout is not prunable");
}

#[test]
fn load_project_wires_a_real_repo_into_the_workspace() {
    let repo = make_git_repo();
    let linked = repo.path().with_extension("wt-feature");
    git(
        repo.path(),
        &["worktree", "add", "-b", "feature", linked.to_str().unwrap()],
    );

    let mut workspace = Workspace::new();
    let project_id = workspace
        .load_project(repo.path())
        .expect("load a real repo");

    let project = workspace.project(project_id).expect("project present");
    assert!(project.is_git);
    assert_eq!(project.root_path, repo.path());

    let worktrees: Vec<_> = workspace.worktrees_of(project_id).collect();
    assert_eq!(worktrees.len(), 2);
    assert_eq!(worktrees[0].branch, "main");
    assert!(worktrees[0].is_primary);
    assert_eq!(worktrees[1].branch, "feature");
    assert!(!worktrees[1].is_primary);
    assert_ne!(
        worktrees[0].id, worktrees[1].id,
        "worktrees get distinct ids"
    );

    // The model is usable end to end: open a tab and filter by it.
    let tab_id = workspace
        .add_tab(worktrees[0].id, "Chat", tiller_project::TabKind::AgentChat)
        .expect("tab added to a real worktree");
    let filtered = workspace.filter("chat");
    assert_eq!(filtered.projects.len(), 1);
    assert_eq!(filtered.projects[0].worktrees.len(), 1);
    assert_eq!(filtered.projects[0].worktrees[0].tabs[0].tab.id, tab_id);
}
