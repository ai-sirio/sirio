//! `fetch_refs` against real repositories, including the two ways a fetch
//! can hang: an ssh that waits, and an HTTP server that wants a password.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use sirio_git::{
    FetchError, FetchRefspec, delete_ref, fetch_refs, list_remotes, object_exists, refs_under,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("sirio-fetch-test-{}-{unique}", std::process::id()));
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

fn git(dir: &Path, args: &[&str]) -> String {
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

fn write_commit(repo: &Path, file: &str, content: &str, message: &str) {
    std::fs::write(repo.join(file), content).expect("write fixture file");
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c", "user.email=test@sirio.dev", "-c", "user.name=Sirio Test",
            "-c", "commit.gpgSign=false", "commit", "-q", "-m", message,
        ],
    );
}

/// The forge's git side: `main` (tagged `v1`) and a change request's head
/// kept under `refs/pull/7/head` — and nothing else.
struct Remote {
    _dirs: (TempDir, TempDir),
    bare: PathBuf,
    pull_head: String,
    main_tip: String,
}

fn remote_with_a_change_request() -> Remote {
    let source = TempDir::new();
    let s = source.path();
    git(s, &["init", "-q", "-b", "main"]);
    write_commit(s, "a.txt", "one\n", "first");
    write_commit(s, "b.txt", "two\n", "second");
    git(s, &["tag", "v1"]);
    let main_tip = git(s, &["rev-parse", "HEAD"]);
    git(s, &["checkout", "-q", "-b", "pr"]);
    write_commit(s, "c.txt", "three\n", "the change request");
    let pull_head = git(s, &["rev-parse", "HEAD"]);
    let holder = TempDir::new();
    let bare = holder.path().join("forge.git");
    git(
        holder.path(),
        &["clone", "-q", "--bare", s.to_str().unwrap(), bare.to_str().unwrap()],
    );
    git(&bare, &["update-ref", "refs/pull/7/head", &pull_head]);
    git(&bare, &["update-ref", "-d", "refs/heads/pr"]);
    Remote { _dirs: (source, holder), bare, pull_head, main_tip }
}

/// A checkout with `origin` at `url` and no credential helper to answer for it.
fn work_repo(url: &str) -> TempDir {
    let work = TempDir::new();
    git(work.path(), &["init", "-q", "-b", "main"]);
    git(work.path(), &["config", "credential.helper", ""]);
    git(work.path(), &["remote", "add", "origin", url]);
    work
}

fn specs() -> Vec<FetchRefspec> {
    vec![
        FetchRefspec::new("refs/pull/7/head", "refs/sirio/change-requests/origin/7/head").unwrap(),
        FetchRefspec::new("refs/heads/main", "refs/sirio/change-requests/origin/7/base").unwrap(),
    ]
}

#[test]
fn a_fetch_lands_the_refs_and_touches_nothing_else() {
    let remote = remote_with_a_change_request();
    let work = work_repo(remote.bare.to_str().unwrap());
    fetch_refs(work.path(), "origin", &specs(), Duration::from_secs(60)).expect("fetch");

    assert_eq!(
        refs_under(work.path(), "refs/sirio/change-requests/").expect("refs"),
        vec![
            "refs/sirio/change-requests/origin/7/base".to_string(),
            "refs/sirio/change-requests/origin/7/head".to_string(),
        ]
    );
    assert!(object_exists(work.path(), &remote.pull_head));
    assert!(object_exists(work.path(), &remote.main_tip));
    assert!(!work.path().join(".git/FETCH_HEAD").exists(), "FETCH_HEAD stays untouched");
    assert_eq!(git(work.path(), &["tag", "-l"]), "", "no tag came along");
    assert_eq!(git(work.path(), &["branch", "-a"]), "", "no branch came along");
}

#[test]
fn a_ref_the_remote_does_not_have_is_a_failure_that_says_so() {
    let remote = remote_with_a_change_request();
    let work = work_repo(remote.bare.to_str().unwrap());
    let missing = vec![
        FetchRefspec::new("refs/pull/999/head", "refs/sirio/change-requests/origin/999/head")
            .unwrap(),
    ];
    let Err(FetchError::Failed { detail }) =
        fetch_refs(work.path(), "origin", &missing, Duration::from_secs(60))
    else {
        panic!("a missing remote ref must fail the fetch");
    };
    assert!(detail.contains("999"), "the reason names the ref: {detail}");
}

#[test]
fn delete_ref_refuses_everything_outside_the_sirio_namespace() {
    let remote = remote_with_a_change_request();
    let work = work_repo(remote.bare.to_str().unwrap());
    fetch_refs(work.path(), "origin", &specs(), Duration::from_secs(60)).expect("fetch");

    assert!(delete_ref(work.path(), "refs/heads/main").is_err());
    assert!(delete_ref(work.path(), "HEAD").is_err());
    delete_ref(work.path(), "refs/sirio/change-requests/origin/7/head").expect("delete");
    assert_eq!(
        refs_under(work.path(), "refs/sirio/change-requests/").expect("refs"),
        vec!["refs/sirio/change-requests/origin/7/base".to_string()]
    );
}

#[test]
fn list_remotes_reports_names_and_configured_urls() {
    let work = work_repo("https://ghe.test/acme/widgets.git");
    git(work.path(), &["remote", "add", "upstream", "git@ghe.test:acme/widgets.git"]);
    let mut remotes = list_remotes(work.path());
    remotes.sort();
    assert_eq!(
        remotes,
        vec![
            ("origin".to_string(), "https://ghe.test/acme/widgets.git".to_string()),
            ("upstream".to_string(), "git@ghe.test:acme/widgets.git".to_string()),
        ]
    );
    let bare_repo = TempDir::new();
    git(bare_repo.path(), &["init", "-q"]);
    assert_eq!(list_remotes(bare_repo.path()), vec![]);
}

#[cfg(unix)]
#[test]
fn an_ssh_that_waits_forever_is_killed_at_the_timeout() {
    use std::os::unix::fs::PermissionsExt;
    let work = work_repo("ssh://hang.invalid/x.git");
    let script = work.path().join("hang.sh");
    std::fs::write(&script, "#!/bin/sh\nsleep 30\n").expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    git(work.path(), &["config", "core.sshCommand", script.to_str().unwrap()]);

    let started = Instant::now();
    let result = fetch_refs(work.path(), "origin", &specs(), Duration::from_secs(1));
    assert_eq!(result, Err(FetchError::TimedOut));
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the whole process tree was killed, not waited for: {:?}",
        started.elapsed()
    );
}

#[cfg(unix)]
#[test]
fn a_remote_that_asks_for_a_password_fails_fast_instead_of_prompting() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buffer = [0u8; 4096];
            let _ = stream.read(&mut buffer);
            let _ = stream.write_all(
                b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"x\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    let work = work_repo(&format!("http://127.0.0.1:{port}/x.git"));

    let started = Instant::now();
    let Err(FetchError::Failed { detail }) =
        fetch_refs(work.path(), "origin", &specs(), Duration::from_secs(30))
    else {
        panic!("a 401 must fail the fetch");
    };
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "it failed at once instead of waiting for a password: {:?}",
        started.elapsed()
    );
    assert!(detail.contains("terminal prompts disabled"), "{detail}");
}
