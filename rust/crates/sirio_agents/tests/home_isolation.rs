//! The invariant that matters most: `prepare` writes worktree-local hook
//! configuration and never touches the user's global config.
//!
//! This lives in its own integration-test file on purpose. It sets the
//! process-global `HOME`, and Rust runs the tests of one binary as concurrent
//! threads in a single process — so sitting next to the other adapter tests it
//! changed `HOME` underneath them and made them fail intermittently. A separate
//! test file gets its own process, which is the isolation this needs.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sirio_agents::ALL;

const PANE_ID: &str = "12345678-1234-1234-1234-123456789abc";
const SIRIOCTL: &str = "/usr/local/bin/sirioctl";

/// A throwaway directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("sirio-agents-home-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(std::fs::canonicalize(&path).expect("canonicalize temp dir"))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// Everything under the directory, recursively, as relative paths.
    fn tree(&self) -> Vec<PathBuf> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("read dir") {
                let entry = entry.expect("dir entry");
                let path = entry.path();
                out.push(path.strip_prefix(base).expect("under base").to_path_buf());
                if path.is_dir() {
                    walk(&path, base, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.0, &self.0, &mut out);
        out.sort();
        out
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn prepare_never_touches_a_fake_home_directory() {
    let home = TempDir::new();
    let worktree = TempDir::new();

    // Point HOME at the fake home. If any adapter ever wrote to ~/.claude,
    // ~/.codex, ~/.config or anywhere else under it, this catches it.
    unsafe { std::env::set_var("HOME", home.path()) };

    for adapter in ALL {
        adapter
            .prepare(worktree.path().to_str().unwrap(), PANE_ID, SIRIOCTL)
            .expect("prepare succeeds");
    }

    assert!(
        home.tree().is_empty(),
        "nothing may be created under the home directory, got {:?}",
        home.tree()
    );
}
