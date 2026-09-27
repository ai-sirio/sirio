use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use sirio_git::{GitClone, GitDiffSideBySide, GitError, GitRemote};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("sirio-git-p41-{tag}-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create temp directory");
        Self(path.canonicalize().expect("canonicalize temp directory"))
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

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git is installed");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repo(tag: &str) -> TempDir {
    let repo = TempDir::new(tag);
    git(repo.path(), &["init", "-q", "-b", "main"]);
    git(repo.path(), &["config", "user.email", "test@sirio.dev"]);
    git(repo.path(), &["config", "user.name", "Sirio Test"]);
    std::fs::write(repo.path().join("file.txt"), "one\ntwo\nthree\n").expect("write fixture");
    // `* -text` keeps fixture content byte-exact through clone checkouts:
    // on Windows the global core.autocrlf default would otherwise rewrite
    // the checked-out file to CRLF and break byte-level content asserts.
    // On Unix this is a no-op — the worktree bytes were already literal.
    std::fs::write(repo.path().join(".gitattributes"), "* -text\n").expect("write gitattributes");
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "root"]);
    repo
}

#[test]
fn clone_from_local_repository_reports_receiving_progress() {
    let source = repo("clone-source");
    for index in 0..80 {
        std::fs::write(
            source.path().join(format!("file-{index:03}.txt")),
            "content\n".repeat(64),
        )
        .expect("write clone fixture");
    }
    git(source.path(), &["add", "-A"]);
    git(source.path(), &["commit", "-q", "-m", "bulk"]);

    let destination_parent = TempDir::new("clone-destination");
    let destination = destination_parent.path().join("clone");
    let progress = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&progress);
    GitClone::clone(
        source.path().to_str().unwrap(),
        &destination,
        move |value| {
            observed.lock().unwrap().push(value);
        },
    )
    .expect("local clone succeeds");

    assert!(destination.join("file-000.txt").exists());
    let progress = progress.lock().unwrap();
    assert!(
        !progress.is_empty(),
        "clone emitted no Receiving objects progress"
    );
    assert!(progress.iter().all(|value| (0.0..=1.0).contains(value)));
    assert_eq!(
        GitClone::parse_progress("Receiving objects:  45% (9/20), done."),
        Some(0.45)
    );

    let failed = GitClone::clone(
        "/definitely/not/a/repository",
        &destination_parent.path().join("failed-clone"),
        |_| {},
    )
    .expect_err("invalid clone source must fail through the runner");
    assert!(matches!(failed, GitError::CommandFailed { .. }));
}

#[test]
fn clone_from_file_url_creates_the_requested_destination() {
    let source = repo("clone-file-url-source");
    let destination_parent = TempDir::new("clone-file-url-destination");
    let destination = destination_parent.path().join("file-url-clone");
    let url = format!("file://{}", source.path().display());

    GitClone::clone(&url, &destination, |_| {}).expect("file URL clone succeeds");

    assert_eq!(
        std::fs::read_to_string(destination.join("file.txt")).expect("read cloned file"),
        "one\ntwo\nthree\n"
    );
}

#[test]
fn remote_parsing_supports_github_ssh_https_and_project_suffixes() {
    assert_eq!(
        GitRemote::github_owner_from_url("git@github.com:acme/widgets.git"),
        Some("acme".into())
    );
    assert_eq!(
        GitRemote::github_owner_from_url("https://github.com/acme/widgets.git"),
        Some("acme".into())
    );
    assert_eq!(
        GitRemote::github_owner_from_url("https://gitlab.com/acme/widgets.git"),
        None
    );
    assert_eq!(
        GitRemote::project_name("https://github.com/acme/widgets.git/"),
        "widgets"
    );
    assert_eq!(
        GitRemote::project_name("git@github.com:acme/widgets"),
        "widgets"
    );
    // A local clone source pasted out of Explorer is a native Windows path,
    // and `git clone` takes it. Cutting only at a slash or a colon left the
    // whole tail after the drive letter as the "name", which the Clone form
    // then rejected as not a single folder name.
    assert_eq!(GitRemote::project_name(r"C:\src\widgets"), "widgets");
    assert_eq!(GitRemote::project_name(r"C:\src\widgets.git"), "widgets");
    assert_eq!(GitRemote::project_name(r"C:\src\widgets\"), "widgets");

    let repository = repo("remote");
    git(
        repository.path(),
        &["remote", "add", "origin", "git@github.com:acme/widgets.git"],
    );
    assert_eq!(
        GitRemote::github_owner(repository.path()),
        Some("acme".into())
    );
    git(
        repository.path(),
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/acme/widgets.git/",
        ],
    );
    assert_eq!(
        GitRemote::github_owner(repository.path()),
        Some("acme".into())
    );
}

#[test]
fn side_by_side_handles_real_rename_binary_and_large_context() {
    let repository = repo("side-by-side-real");
    let source = repository.path().join("source.txt");
    let binary = repository.path().join("binary.bin");
    let original = (0..220)
        .map(|line| format!("middle-{line}\n"))
        .collect::<String>();
    std::fs::write(&source, &original).expect("write large source");
    std::fs::write(&binary, b"binary-one\0bytes").expect("write binary");
    git(repository.path(), &["add", "-A"]);
    git(
        repository.path(),
        &["commit", "-q", "-m", "rename and binary"],
    );
    git(repository.path(), &["mv", "source.txt", "renamed.txt"]);

    let changed = original.replacen("middle-150\n", "replacement\ninserted\n", 1);
    std::fs::write(repository.path().join("renamed.txt"), changed).expect("write renamed change");
    std::fs::write(&binary, b"binary-two\0bytes").expect("write binary change");

    let snapshot = sirio_git::status(repository.path()).expect("real status");
    let rename = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == Path::new("renamed.txt"))
        .expect("rename entry");
    assert_eq!(
        rename.original_path.as_deref(),
        Some(Path::new("source.txt"))
    );
    let diff = sirio_git::diff_entry(
        repository.path(),
        rename,
        sirio_git::WHOLE_FILE_CONTEXT_LINES,
    )
    .expect("real rename diff");
    let rows = GitDiffSideBySide::rows(&diff);
    assert!(rows.iter().any(|row| row.is_hunk));
    assert!(rows.iter().any(|row| {
        row.left
            .as_ref()
            .is_some_and(|line| line.content == "middle-149")
            && row
                .right
                .as_ref()
                .is_some_and(|line| line.content == "middle-149")
    }));
    assert!(rows.iter().all(|row| {
        row.left
            .as_ref()
            .is_none_or(|line| !line.content.starts_with("rename "))
    }));

    let binary_entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == Path::new("binary.bin"))
        .expect("binary entry");
    assert!(
        sirio_git::diff_entry(repository.path(), binary_entry, 3)
            .expect("real binary diff")
            .is_binary
    );
}

#[test]
fn mutation_validation_rejects_stale_and_duplicate_entries() {
    let repository = repo("actions");
    std::fs::write(repository.path().join("stale.txt"), "stale\n").expect("write stale fixture");
    let stale = sirio_git::status(repository.path()).unwrap().entries;
    let stale_entry = stale
        .iter()
        .find(|entry| entry.path == Path::new("stale.txt"))
        .unwrap()
        .clone();
    git(repository.path(), &["add", "--", "stale.txt"]);
    assert!(matches!(
        sirio_git::GitActions::discard_untracked(repository.path(), &[stale_entry]),
        Err(GitError::Action(_))
    ));

    std::fs::write(repository.path().join("duplicate.txt"), "duplicate\n")
        .expect("write duplicate fixture");
    let entry = sirio_git::status(repository.path())
        .unwrap()
        .entries
        .into_iter()
        .find(|entry| entry.path == Path::new("duplicate.txt"))
        .unwrap();
    assert!(matches!(
        sirio_git::GitActions::stage(repository.path(), &[entry.clone(), entry]),
        Err(GitError::Action(_))
    ));
}
