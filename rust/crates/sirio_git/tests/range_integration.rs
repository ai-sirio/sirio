//! Range reads against a real repository: a branch that left `main` with
//! `main` moving on afterwards, and the awkward names a forge shows.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use sirio_git::{
    DiffOrigin, GitError, RangeFile, range_file_diff, range_files, range_stats, show_blob,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("sirio-range-test-{}-{unique}", std::process::id()));
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

fn ensure_generous_timeout() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // SAFETY: test process, set once; a wider budget can only turn a
        // would-be timeout into a pass, never the reverse.
        unsafe { std::env::set_var("SIRIO_GIT_TIMEOUT_MS", "120000") };
    });
}

/// Runs `git <args>` in `dir`, asserting success, and returns its trimmed stdout.
fn git(dir: &Path, args: &[&str]) -> String {
    ensure_generous_timeout();
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git must be installed to run these tests");
    assert!(
        output.status.success(),
        "`git {args:?}` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn write(repo: &Path, relative: &str, content: &[u8]) {
    let path = repo.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(path, content).expect("write fixture file");
}

fn commit(repo: &Path, message: &str) -> String {
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c", "user.email=test@sirio.dev", "-c", "user.name=Sirio Test",
            "-c", "commit.gpgSign=false", "commit", "-q", "-m", message,
        ],
    );
    git(repo, &["rev-parse", "HEAD"])
}

struct Fixture {
    _dir: TempDir,
    repo: PathBuf,
    base: String,
    head: String,
}

fn sixty_lines(changed: Option<usize>) -> String {
    (1..=60)
        .map(|n| {
            if Some(n) == changed {
                format!("changed {n}\n")
            } else {
                format!("line {n}\n")
            }
        })
        .collect()
}

/// `main` → `feat` (an edit, an awkward add, a binary, a rename, a delete, a
/// leading-dash file), then `main` moves on: `base` is main's *new* tip, not
/// the merge base, so only `base...head` gives the change request's diff.
fn fixture() -> Fixture {
    let dir = TempDir::new();
    let repo = dir.path().to_path_buf();
    git(&repo, &["init", "-q", "-b", "main"]);
    write(&repo, "a.txt", sixty_lines(None).as_bytes());
    write(&repo, "docs/old notes.md", b"alpha\nbeta\ngamma\ndelta\n");
    write(&repo, "gone.txt", b"bye\nbye\n");
    write(&repo, "-flag.txt", b"x\n");
    commit(&repo, "base");
    git(&repo, &["checkout", "-q", "-b", "feat"]);
    write(&repo, "a.txt", sixty_lines(Some(42)).as_bytes());
    write(&repo, "sp ace/ünï.txt", b"hi\n");
    write(&repo, "new\nline.txt", b"nl\n");
    write(&repo, "bin.dat", &[0, 1, 2, 0]);
    git(&repo, &["mv", "docs/old notes.md", "docs/new notes.md"]);
    write(&repo, "docs/new notes.md", b"alpha\nBETA\ngamma\ndelta\n");
    git(&repo, &["rm", "-q", "gone.txt"]);
    write(&repo, "-flag.txt", b"y\n");
    let head = commit(&repo, "change");
    git(&repo, &["checkout", "-q", "main"]);
    write(&repo, "main-only.txt", b"later\n");
    let base = commit(&repo, "main moves on");
    Fixture { _dir: dir, repo, base, head }
}

fn summary(mut files: Vec<RangeFile>) -> Vec<(char, String, Option<String>)> {
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
        .into_iter()
        .map(|file| {
            (
                file.status,
                file.path.to_string_lossy().into_owned(),
                file.old_path.map(|old| old.to_string_lossy().into_owned()),
            )
        })
        .collect()
}

#[test]
fn range_files_lists_what_the_branch_changed_since_the_merge_base() {
    let f = fixture();
    let listed = summary(range_files(&f.repo, &f.base, &f.head).expect("range files"));
    assert_eq!(
        listed,
        vec![
            ('M', "-flag.txt".to_string(), None),
            ('M', "a.txt".to_string(), None),
            ('A', "bin.dat".to_string(), None),
            ('R', "docs/new notes.md".to_string(), Some("docs/old notes.md".to_string())),
            ('D', "gone.txt".to_string(), None),
            ('A', "new\nline.txt".to_string(), None),
            ('A', "sp ace/ünï.txt".to_string(), None),
        ],
        "main-only.txt must not appear: `base...head` diffs against the merge base"
    );
}

#[test]
fn range_stats_counts_each_file_by_its_new_path() {
    let f = fixture();
    let stats = range_stats(&f.repo, &f.base, &f.head).expect("range stats");
    let of = |path: &str| {
        *stats
            .get(Path::new(path))
            .unwrap_or_else(|| panic!("no stat for {path}: {stats:?}"))
    };
    assert_eq!((of("a.txt").additions, of("a.txt").deletions), (1, 1));
    assert_eq!((of("sp ace/ünï.txt").additions, of("sp ace/ünï.txt").deletions), (1, 0));
    assert_eq!((of("docs/new notes.md").additions, of("docs/new notes.md").deletions), (1, 1));
    assert_eq!((of("gone.txt").additions, of("gone.txt").deletions), (0, 2));
    assert_eq!((of("new\nline.txt").additions, of("new\nline.txt").deletions), (1, 0));
    assert!(of("bin.dat").is_binary);
}

#[test]
fn a_files_diff_is_read_on_its_own_and_a_rename_stays_one_change() {
    let f = fixture();
    let diff = range_file_diff(&f.repo, &f.base, &f.head, Path::new("a.txt"), None, 3)
        .expect("a.txt diff");
    assert_eq!(diff.hunks.len(), 1);
    let added: Vec<_> = diff.hunks[0]
        .lines
        .iter()
        .filter(|line| line.origin == DiffOrigin::Addition)
        .collect();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].new_line_number, Some(42));
    assert_eq!(added[0].content, "changed 42");

    let rename = range_file_diff(
        &f.repo,
        &f.base,
        &f.head,
        Path::new("docs/new notes.md"),
        Some(Path::new("docs/old notes.md")),
        3,
    )
    .expect("rename diff");
    assert_eq!(
        (rename.additions, rename.deletions),
        (1, 1),
        "a detected rename, not a deletion plus an addition"
    );

    let dash = range_file_diff(&f.repo, &f.base, &f.head, Path::new("-flag.txt"), None, 3)
        .expect("a leading dash is only ever after `--`");
    assert_eq!((dash.additions, dash.deletions), (1, 1));

    let binary = range_file_diff(&f.repo, &f.base, &f.head, Path::new("bin.dat"), None, 3)
        .expect("binary diff");
    assert!(binary.is_binary);

    let newline = range_file_diff(&f.repo, &f.base, &f.head, Path::new("new\nline.txt"), None, 3)
        .expect("a newline in the name is only ever after `--`, as one argument");
    assert_eq!((newline.additions, newline.deletions), (1, 0));
}

#[test]
fn a_string_that_is_not_a_full_commit_id_never_reaches_git() {
    let f = fixture();
    let marker = std::env::temp_dir().join(format!("sirio-pwned-{}", std::process::id()));
    let option = format!("--upload-pack=touch {}", marker.display());
    let newline = format!("{}\n", f.head);
    let short = f.head[..39].to_string();
    // Git itself exits 128 for some of these, so the exit code alone cannot
    // tell the validator from git: the refusal must be the validator's own.
    fn refused_by_the_validator<T: std::fmt::Debug>(result: Result<T, GitError>, what: &str) {
        let Err(GitError::CommandFailed { code: 128, stderr }) = result else {
            panic!("{what} must be refused with CommandFailed(128)");
        };
        assert!(stderr.starts_with("not a commit id"), "{what}: git ran, or the message changed: {stderr}");
    }
    for bad in ["", "HEAD", option.as_str(), newline.as_str(), short.as_str()] {
        refused_by_the_validator(range_files(&f.repo, bad, &f.head), &format!("range_files base {bad:?}"));
        refused_by_the_validator(range_files(&f.repo, &f.base, bad), &format!("range_files head {bad:?}"));
        refused_by_the_validator(range_stats(&f.repo, bad, &f.head), &format!("range_stats {bad:?}"));
        refused_by_the_validator(
            range_file_diff(&f.repo, &f.base, bad, Path::new("a.txt"), None, 3),
            &format!("range_file_diff {bad:?}"),
        );
        refused_by_the_validator(show_blob(&f.repo, bad, Path::new("a.txt")), &format!("show_blob {bad:?}"));
    }
    assert!(!marker.exists(), "an option-shaped string must never be run");
}

#[test]
fn show_blob_returns_the_exact_bytes_at_a_revision() {
    let f = fixture();
    assert_eq!(show_blob(&f.repo, &f.head, Path::new("bin.dat")).expect("binary"), vec![0, 1, 2, 0]);
    assert_eq!(
        show_blob(&f.repo, &f.head, Path::new("sp ace/ünï.txt")).expect("unicode name"),
        b"hi\n".to_vec()
    );
    assert_eq!(show_blob(&f.repo, &f.head, Path::new("-flag.txt")).expect("dash name"), b"y\n".to_vec());
    assert_eq!(
        show_blob(&f.repo, &f.head, Path::new("new\nline.txt")).expect("newline name"),
        b"nl\n".to_vec()
    );
    assert_eq!(
        show_blob(&f.repo, &f.base, Path::new("gone.txt")).expect("a deleted file, at the base"),
        b"bye\nbye\n".to_vec()
    );
    assert!(show_blob(&f.repo, &f.head, Path::new("gone.txt")).is_err(), "absent at the head");
}
