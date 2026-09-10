//! P99 exercises for four inventory rows, against real repositories and
//! real processes:
//!
//! - `F-GIT-RUN-02` — streaming stderr lines arrive incrementally, split on
//!   CR/LF, before the command completes.
//! - `F-GIT-BRANCH-01` — exact branch names through the Git layer, plus the
//!   recorded proof that git itself cannot produce a branch name containing
//!   a space (so the space conjunct lives at the parse seam).
//! - `F-GIT-STATUS-02` — directory aggregation over a conflict, a rename
//!   and changes at several depths (API subject; see the ledger ruling).
//! - `F-GIT-DIFF-03` — side-by-side rows for context, pure deletion, pure
//!   addition and replacement runs (API subject; see the ledger ruling).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use sirio_git::{
    DEFAULT_CONTEXT_LINES, DiffOrigin, DirectoryGitStatus, GitBranches, diff_entry,
    directory_statuses, list_branches, run_streaming, status,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("sirio-p99-git-{}-{unique}", std::process::id()));
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

/// The crate's runner bounds every git call with a wall-clock deadline;
/// under machine load a tiny fixture invocation can legitimately exceed the
/// 10 s default, so these tests opt into a generous budget exactly like
/// `git_integration.rs` does.
fn ensure_generous_timeout() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // SAFETY: test process, set once, before any concurrent reader.
        unsafe { std::env::set_var("SIRIO_GIT_TIMEOUT_MS", "120000") };
    });
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

/// Runs `git <args>` in `dir`, returning whether it succeeded.
fn git_allowed_to_fail(dir: &Path, args: &[&str]) -> bool {
    ensure_generous_timeout();
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git must be installed to run these tests")
        .status
        .success()
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn make_repo() -> TempDir {
    let dir = TempDir::new();
    git(dir.path(), &["init", "-b", "main", "."]);
    git(dir.path(), &["config", "user.email", "test@sirio"]);
    git(dir.path(), &["config", "user.name", "Sirio Test"]);
    write(dir.path(), "README.md", "seed\n");
    git(dir.path(), &["add", "README.md"]);
    git(dir.path(), &["commit", "-m", "seed"]);
    dir
}

// ---------------------------------------------------------------- RUN-02

/// Unix-only: the emitter is handed to `sh -c` and the whole fixture
/// premise (no ETXTBSY, `/bin/sh`) is a POSIX one — Windows has no
/// `/bin/sh` and the LF/CR splitting this exercises is covered there by
/// the real-clone streaming tests.
#[cfg(unix)]
#[test]
fn streaming_lines_arrive_incrementally_before_completion() {
    // Imported here, not at the top of the file: this is the only user of
    // either name, and top-level imports would be dead on Windows, where
    // `-D warnings` turns that into a build failure.
    use sirio_git::GitRunner;
    use std::time::Instant;

    ensure_generous_timeout();
    let dir = TempDir::new();
    // Two LF lines separated by a sleep, then a CR-separated pair: the
    // reader must deliver on both delimiters, and must deliver the first
    // line while the process is still sleeping.
    //
    // The emitter is handed to `sh -c` rather than written to a file and
    // exec'd, and that is deliberate — do not "simplify" it back. Writing an
    // executable and then exec'ing it inside a multi-threaded test binary
    // races: `fs::write` holds a write fd, a sibling test's `fork` (this
    // target spawns ~25 git processes through `make_repo`) copies the whole
    // fd table into its child, and until that child reaches `execve` the
    // kernel still counts a writer on our inode — so our own exec fails with
    // ETXTBSY, "Text file busy". `O_CLOEXEC`, which Rust sets by default,
    // does not close that window: the fd is dropped at the child's `execve`,
    // not at its `fork`. It cost one red workspace run at load 24. Passing
    // the body as an argument removes the precondition instead of retrying
    // around it: no file is written, so no writer fd can be inherited.
    let sentinel = dir.path().join("alpha-seen");
    let emitter = format!(
        "echo alpha >&2; for i in $(seq 1 200); do [ -f '{}' ] && break; sleep 0.05; done; [ -f '{}' ] || exit 42; echo beta >&2; printf 'gamma\\rdelta\\n' >&2",
        sentinel.display(),
        sentinel.display()
    );

    let mut arrivals: Vec<(String, Instant)> = Vec::new();
    let result = GitRunner::run_streaming_with_binary(
        Path::new("/bin/sh"),
        &["-c", &emitter],
        dir.path(),
        |line| {
            let first = arrivals.is_empty();
            arrivals.push((line, Instant::now()));
            if first {
                std::fs::write(&sentinel, "seen").expect("release emitter sentinel");
            }
        },
    )
    .expect("the emitter runs");
    let finished = Instant::now();

    assert_eq!(
        arrivals
            .iter()
            .map(|(line, _)| line.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "beta", "gamma", "delta"],
        "lines split on LF and CR alike, in emission order"
    );
    assert!(
        finished >= arrivals[0].1,
        "first line precedes command completion"
    );
    assert_eq!(result.exit_code, 0);
    assert!(result.stderr.contains("alpha") && result.stderr.contains("delta"));
}

#[test]
fn a_real_git_clone_streams_progress_and_completes() {
    let source = make_repo();
    write(source.path(), "src/lib.rs", "pub fn seed() {}\n");
    git(source.path(), &["add", "src/lib.rs"]);
    git(source.path(), &["commit", "-m", "more objects"]);

    let scratch = TempDir::new();
    // A file:// URL needs forward slashes and no verbatim prefix — the
    // fixture path is canonicalized, which on Windows yields `\\?\C:\...`,
    // a spelling git cannot open (it resolves the URL to `//\\?\C:\...`).
    #[cfg(windows)]
    let display = source
        .path()
        .display()
        .to_string()
        .strip_prefix(r"\\?\")
        .unwrap_or("")
        .to_string();
    #[cfg(not(windows))]
    let display = source.path().display().to_string();
    #[cfg(windows)]
    let url = format!("file://{}", display.replace('\\', "/"));
    #[cfg(not(windows))]
    let url = format!("file://{display}");
    let mut lines = Vec::new();
    let result = run_streaming(
        &["clone", "--progress", &url, "dest"],
        scratch.path(),
        |line| lines.push(line),
    )
    .expect("clone succeeds");

    assert_eq!(result.exit_code, 0);
    assert!(scratch.path().join("dest/.git").is_dir());
    assert!(
        !lines.is_empty(),
        "a clone with --progress must stream stderr lines"
    );
    // LC_ALL=C in the runner: progress arrives in English regardless of
    // the desktop locale (this box runs an Italian locale).
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Cloning") || line.contains("objects")),
        "expected English clone progress, got {lines:?}"
    );
}

// -------------------------------------------------------------- BRANCH-01

#[test]
fn branch_listing_returns_exact_names_and_git_refuses_spaced_names() {
    let repo = make_repo();
    git(repo.path(), &["branch", "feature/one"]);
    git(repo.path(), &["branch", "release-1.2"]);
    git(repo.path(), &["branch", "änderung.v2"]);

    let mut names = list_branches(repo.path()).expect("list branches");
    names.sort();
    assert_eq!(
        names,
        ["feature/one", "main", "release-1.2", "änderung.v2"],
        "exact names, including slash and non-ASCII"
    );

    // The VERIFY asks for branches with spaces. git itself cannot produce
    // one — recorded here in all three refusal modes so the impossibility
    // is replayable, and so this row reopens if git ever changes its mind.
    assert!(
        !git_allowed_to_fail(repo.path(), &["branch", "has space"]),
        "git branch must refuse a name containing a space"
    );
    assert!(
        !git_allowed_to_fail(repo.path(), &["update-ref", "refs/heads/has space", "HEAD"]),
        "git update-ref must refuse a name containing a space"
    );
    // Even a hand-forged loose ref is ignored by `git branch --list` (with
    // a warning on stderr), so a spaced name can never reach the parser
    // from a real repository.
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    std::fs::write(repo.path().join(".git/refs/heads/has space"), head.stdout).unwrap();
    let mut names = list_branches(repo.path()).expect("listing still succeeds");
    names.sort();
    assert_eq!(
        names,
        ["feature/one", "main", "release-1.2", "änderung.v2"],
        "the forged spaced ref must not appear"
    );

    // The layer itself preserves internal spaces verbatim: if a future git
    // ever hands one over, it survives the parse.
    assert_eq!(
        GitBranches::parse("has space\nmain\n \n"),
        vec!["has space".to_string(), "main".to_string()]
    );
}

// -------------------------------------------------------------- STATUS-02

#[test]
fn directory_statuses_mark_every_ancestor_with_precedence_and_both_rename_sides() {
    let repo = make_repo();

    // Base tree, committed.
    write(repo.path(), "src/app/deep/mod.rs", "mod deep;\n");
    write(repo.path(), "src/app/other.rs", "pub fn other() {}\n");
    write(repo.path(), "docs/guide/readme.md", "guide\n");
    write(repo.path(), "conflict/dir/file.txt", "base\n");
    write(repo.path(), "conflict/also_mod.txt", "base\n");
    write(repo.path(), "move/from/orig.txt", "moving\n");
    write(repo.path(), "root_note.txt", "root\n");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base tree"]);

    // A real conflict: both branches edit conflict/dir/file.txt.
    git(repo.path(), &["checkout", "-b", "left"]);
    write(repo.path(), "conflict/dir/file.txt", "left\n");
    git(repo.path(), &["commit", "-am", "left"]);
    git(repo.path(), &["checkout", "main"]);
    write(repo.path(), "conflict/dir/file.txt", "right\n");
    git(repo.path(), &["commit", "-am", "right"]);
    assert!(
        !git_allowed_to_fail(repo.path(), &["merge", "left"]),
        "the merge must stop on the conflict"
    );

    // Changes at several depths while the merge is unresolved.
    write(repo.path(), "src/app/deep/mod.rs", "mod deep; // edited\n");
    write(repo.path(), "src/zz_new.txt", "untracked\n"); // untracked next to a modification
    write(repo.path(), "docs/guide/notes.txt", "untracked only\n");
    write(
        repo.path(),
        "conflict/also_mod.txt",
        "modified next to a conflict\n",
    );
    write(repo.path(), "root_note.txt", "root edited\n");
    // A staged rename: original and destination ancestors both count.
    std::fs::create_dir_all(repo.path().join("moved_dest")).unwrap();
    git(
        repo.path(),
        &["mv", "move/from/orig.txt", "moved_dest/renamed.txt"],
    );

    let snapshot = status(repo.path()).expect("status parses");
    let map = directory_statuses(&snapshot.entries);

    let expected: HashMap<PathBuf, DirectoryGitStatus> = [
        // Modified at depth 3; the untracked sibling cannot demote them.
        ("src", DirectoryGitStatus::Changed),
        ("src/app", DirectoryGitStatus::Changed),
        ("src/app/deep", DirectoryGitStatus::Changed),
        // Untracked alone marks its ancestors untracked.
        ("docs", DirectoryGitStatus::Untracked),
        ("docs/guide", DirectoryGitStatus::Untracked),
        // Conflict beats the modification sharing the same parent.
        ("conflict", DirectoryGitStatus::Conflicted),
        ("conflict/dir", DirectoryGitStatus::Conflicted),
        // The rename contributes both its original and current ancestors.
        ("move", DirectoryGitStatus::Changed),
        ("move/from", DirectoryGitStatus::Changed),
        ("moved_dest", DirectoryGitStatus::Changed),
    ]
    .into_iter()
    .map(|(path, status)| (PathBuf::from(path), status))
    .collect();

    assert_eq!(
        map, expected,
        "every non-root ancestor, with conflicted > changed > untracked"
    );
    // The root-level edit contributes no ancestor entries at all.
    assert!(!map.contains_key(Path::new("")) && !map.contains_key(Path::new(".")));
}

/// The Files tree tints a *file* row from the same three-valued vocabulary
/// its ancestors are tinted from, so `DirectoryGitStatus::for_entry` — the
/// classification the aggregator itself applies — must agree with the
/// aggregate every leaf produces. Proved over one real repo carrying a real
/// merge conflict, an untracked file, a modification and a `git mv`.
#[test]
fn for_entry_classifies_leaves_exactly_as_it_marks_their_ancestors() {
    let repo = make_repo();

    write(repo.path(), "conflict/dir/file.txt", "base\n");
    write(repo.path(), "mod/dir/file.txt", "base\n");
    write(repo.path(), "move/from/orig.txt", "moving\n");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base tree"]);

    git(repo.path(), &["checkout", "-b", "left"]);
    write(repo.path(), "conflict/dir/file.txt", "left\n");
    git(repo.path(), &["commit", "-am", "left"]);
    git(repo.path(), &["checkout", "main"]);
    write(repo.path(), "conflict/dir/file.txt", "right\n");
    git(repo.path(), &["commit", "-am", "right"]);
    assert!(
        !git_allowed_to_fail(repo.path(), &["merge", "left"]),
        "the merge must stop on the conflict"
    );

    write(repo.path(), "mod/dir/file.txt", "edited\n");
    write(repo.path(), "fresh/dir/new.txt", "untracked\n");
    std::fs::create_dir_all(repo.path().join("moved_dest")).unwrap();
    git(
        repo.path(),
        &["mv", "move/from/orig.txt", "moved_dest/renamed.txt"],
    );

    let snapshot = status(repo.path()).expect("status parses");
    let by_path: HashMap<PathBuf, DirectoryGitStatus> = snapshot
        .entries
        .iter()
        .map(|entry| (entry.path.clone(), DirectoryGitStatus::for_entry(entry)))
        .collect();

    assert_eq!(
        by_path.get(Path::new("conflict/dir/file.txt")),
        Some(&DirectoryGitStatus::Conflicted),
        "an unmerged leaf is conflicted, not merely changed"
    );
    assert_eq!(
        by_path.get(Path::new("mod/dir/file.txt")),
        Some(&DirectoryGitStatus::Changed)
    );
    assert_eq!(
        by_path.get(Path::new("fresh/dir/new.txt")),
        Some(&DirectoryGitStatus::Untracked)
    );
    assert_eq!(
        by_path.get(Path::new("moved_dest/renamed.txt")),
        Some(&DirectoryGitStatus::Changed),
        "a rename destination is a change, never an untracked file"
    );

    // Every leaf's own classification is exactly what its immediate parent
    // inherits when that parent has no stronger sibling beneath it.
    let directories = directory_statuses(&snapshot.entries);
    for (path, leaf_status) in &by_path {
        let parent = path.parent().expect("fixture leaves are all nested");
        assert_eq!(
            directories.get(parent),
            Some(leaf_status),
            "{} and its parent disagree",
            path.display()
        );
    }

    assert_eq!(DirectoryGitStatus::Conflicted.slug(), "conflicted");
    assert_eq!(DirectoryGitStatus::Changed.slug(), "changed");
    assert_eq!(DirectoryGitStatus::Untracked.slug(), "untracked");
}

// ---------------------------------------------------------------- DIFF-03

#[test]
fn side_by_side_rows_pair_context_zip_replacements_and_pad_pure_runs() {
    let repo = make_repo();
    write(
        repo.path(),
        "sbs.txt",
        "ctx-a\nold-1\nold-2\nctx-b\ndel-only\nctx-c\nctx-d\n",
    );
    git(repo.path(), &["add", "sbs.txt"]);
    git(repo.path(), &["commit", "-m", "side-by-side base"]);
    write(
        repo.path(),
        "sbs.txt",
        "ctx-a\nnew-1\nnew-2\nnew-3\nctx-b\nctx-c\nadd-only\nctx-d\n",
    );

    let snapshot = status(repo.path()).expect("status parses");
    let entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == Path::new("sbs.txt"))
        .expect("sbs.txt is modified");
    let diff = diff_entry(repo.path(), entry, DEFAULT_CONTEXT_LINES).expect("diff loads");
    let rows = sirio_git::side_by_side_rows(&diff);

    // The shape of every row: (left content, right content, is_hunk).
    let shape: Vec<(Option<&str>, Option<&str>, bool)> = rows
        .iter()
        .map(|row| {
            (
                row.left.as_ref().map(|line| line.content.as_str()),
                row.right.as_ref().map(|line| line.content.as_str()),
                row.is_hunk,
            )
        })
        .collect();
    assert_eq!(
        shape,
        [
            (Some("@@ -1,7 +1,8 @@"), None, true), // full-width hunk header
            (Some("ctx-a"), Some("ctx-a"), false), // context pairs itself
            (Some("old-1"), Some("new-1"), false), // replacement zipped…
            (Some("old-2"), Some("new-2"), false),
            (None, Some("new-3"), false), // …and padded where runs differ
            (Some("ctx-b"), Some("ctx-b"), false),
            (Some("del-only"), None, false), // pure deletion: left only
            (Some("ctx-c"), Some("ctx-c"), false),
            (None, Some("add-only"), false), // pure addition: right only
            (Some("ctx-d"), Some("ctx-d"), false),
        ]
    );

    // Ids are sequential, and the header is the only full-width row.
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row.id, index);
    }
    assert_eq!(rows.iter().filter(|row| row.is_hunk).count(), 1);
    let header = rows[0].left.as_ref().expect("header sits on the left");
    assert!(header.is_hunk && rows[0].right.is_none());

    // Line numbers land on the correct sides: context carries both, a
    // deletion has no new number, an addition no old number.
    let ctx_b = rows[5].left.as_ref().unwrap();
    assert_eq!(
        (ctx_b.old_line_number, ctx_b.new_line_number),
        (Some(4), Some(5))
    );
    let deletion = rows[6].left.as_ref().unwrap();
    assert_eq!(deletion.origin, DiffOrigin::Deletion);
    assert_eq!(
        (deletion.old_line_number, deletion.new_line_number),
        (Some(5), None)
    );
    let addition = rows[8].right.as_ref().unwrap();
    assert_eq!(addition.origin, DiffOrigin::Addition);
    assert_eq!(
        (addition.old_line_number, addition.new_line_number),
        (None, Some(7))
    );

    // Metadata lines are omitted from every row.
    for row in &rows {
        for side in [&row.left, &row.right].into_iter().flatten() {
            let content = &side.content;
            assert!(
                !content.starts_with("diff --git")
                    && !content.starts_with("index ")
                    && !content.starts_with("--- ")
                    && !content.starts_with("+++ "),
                "metadata leaked into a row: {content:?}"
            );
        }
    }
}
