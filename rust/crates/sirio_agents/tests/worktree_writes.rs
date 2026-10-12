//! `prepare` writes into the worktree, whose content a change request's author
//! controls. A committed symlink where a hook or a skill goes must not send the
//! write outside the worktree: `prepare` refuses, and the link's target is
//! untouched.
//!
//! Unix only: the tests plant symlinks.
#![cfg(unix)]

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sirio_agents::{AgentAdapter, ClaudeCodeAdapter, OhMyPiAdapter, OpenCodeAdapter};

const PANE_ID: &str = "12345678-1234-1234-1234-123456789abc";
const SIRIOCTL: &str = "/usr/local/bin/sirioctl";

/// A throwaway directory under TMPDIR, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("sirio-worktree-writes-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(std::fs::canonicalize(&path).expect("canonicalize temp dir"))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A worktree and a folder outside it holding `secret.txt`. Returns the
/// worktree, the outside folder and the secret's path.
fn layout(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let worktree = root.join("worktree");
    let outside = root.join("outside");
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let secret = outside.join("secret.txt");
    std::fs::write(&secret, "untouched").unwrap();
    (worktree, outside, secret)
}

fn assert_outside_untouched(outside: &Path, secret: &Path) {
    assert_eq!(std::fs::read_to_string(secret).unwrap(), "untouched", "a file outside the worktree was written");
    let names: Vec<String> = std::fs::read_dir(outside)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["secret.txt"], "something was made outside the worktree");
}

#[test]
fn omp_refuses_a_committed_hook_link_and_leaves_its_target_alone() {
    let root = TempDir::new();
    let (worktree, outside, secret) = layout(&root.0);
    std::fs::create_dir_all(worktree.join(".sirio")).unwrap();
    symlink(&secret, worktree.join(".sirio/omp-hook.ts")).unwrap();

    let result = OhMyPiAdapter.prepare(worktree.to_str().unwrap(), PANE_ID, SIRIOCTL);

    assert!(result.is_err(), "omp prepared through a committed symlink");
    assert_outside_untouched(&outside, &secret);
}

#[test]
fn omp_refuses_a_symlinked_sirio_folder_and_leaves_its_target_alone() {
    let root = TempDir::new();
    let (worktree, outside, secret) = layout(&root.0);
    symlink(&outside, worktree.join(".sirio")).unwrap();

    let result = OhMyPiAdapter.prepare(worktree.to_str().unwrap(), PANE_ID, SIRIOCTL);

    assert!(result.is_err(), "omp prepared through a symlinked folder");
    assert_outside_untouched(&outside, &secret);
}

#[test]
fn opencode_refuses_a_symlinked_plugin_folder_and_leaves_its_target_alone() {
    let root = TempDir::new();
    let (worktree, outside, secret) = layout(&root.0);
    symlink(&outside, worktree.join(".opencode")).unwrap();

    let result = OpenCodeAdapter.prepare(worktree.to_str().unwrap(), PANE_ID, SIRIOCTL);

    assert!(result.is_err(), "opencode prepared through a symlinked folder");
    assert_outside_untouched(&outside, &secret);
}

#[test]
fn opencode_refuses_a_committed_plugin_link_and_leaves_its_target_alone() {
    let root = TempDir::new();
    let (worktree, outside, secret) = layout(&root.0);
    std::fs::create_dir_all(worktree.join(".opencode/plugin")).unwrap();
    symlink(&secret, worktree.join(".opencode/plugin/sirio-session.js")).unwrap();

    let result = OpenCodeAdapter.prepare(worktree.to_str().unwrap(), PANE_ID, SIRIOCTL);

    assert!(result.is_err(), "opencode prepared through a committed symlink");
    assert_outside_untouched(&outside, &secret);
}

#[test]
fn claude_refuses_a_symlinked_settings_folder_and_leaves_its_target_alone() {
    let root = TempDir::new();
    let (worktree, outside, secret) = layout(&root.0);
    symlink(&outside, worktree.join(".claude")).unwrap();

    let result = ClaudeCodeAdapter.prepare(worktree.to_str().unwrap(), PANE_ID, SIRIOCTL);

    assert!(result.is_err(), "claude prepared through a symlinked folder");
    assert_outside_untouched(&outside, &secret);
}

/// A FIFO outside the worktree with no writer: reading it blocks for ever,
/// the way a link to `/dev/zero` never ends. A `prepare` that reads through
/// a committed link to it never returns.
fn fifo(outside: &Path) -> PathBuf {
    let fifo = outside.join("fifo");
    let status = std::process::Command::new("mkfifo").arg(&fifo).status().expect("mkfifo runs");
    assert!(status.success(), "mkfifo failed");
    fifo
}

/// Runs `prepare` on a thread and reports whether it returned within a few
/// seconds; a read through the link blocks it, and the test process ends it.
fn prepare_returns(adapter: &'static (dyn AgentAdapter + Sync), worktree: &Path) -> bool {
    let (done, finished) = std::sync::mpsc::channel();
    let worktree = worktree.to_str().unwrap().to_string();
    std::thread::spawn(move || {
        let _ = adapter.prepare(&worktree, PANE_ID, SIRIOCTL);
        let _ = done.send(());
    });
    finished.recv_timeout(std::time::Duration::from_secs(5)).is_ok()
}

#[test]
fn claude_never_reads_its_settings_through_a_committed_link() {
    let root = TempDir::new();
    let (worktree, outside, _secret) = layout(&root.0);
    std::fs::create_dir_all(worktree.join(".claude")).unwrap();
    symlink(fifo(&outside), worktree.join(".claude/settings.local.json")).unwrap();

    assert!(prepare_returns(&ClaudeCodeAdapter, &worktree), "prepare read the settings through a committed link");
}

#[test]
fn claude_never_reads_its_skill_through_a_committed_link() {
    let root = TempDir::new();
    let (worktree, outside, _secret) = layout(&root.0);
    std::fs::create_dir_all(worktree.join(".claude/skills/sirio")).unwrap();
    symlink(fifo(&outside), worktree.join(".claude/skills/sirio/SKILL.md")).unwrap();

    assert!(prepare_returns(&ClaudeCodeAdapter, &worktree), "prepare read the skill through a committed link");
}

#[test]
fn omp_still_prepares_a_plain_worktree() {
    let root = TempDir::new();
    let (worktree, _outside, _secret) = layout(&root.0);

    OhMyPiAdapter.prepare(worktree.to_str().unwrap(), PANE_ID, SIRIOCTL).unwrap();

    assert!(worktree.join(".sirio/omp-hook.ts").is_file());
    assert!(worktree.join(".agents/skills/sirio/SKILL.md").is_file());
}
