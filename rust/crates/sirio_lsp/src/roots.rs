//! Where a language server should be rooted.
//!
//! Not the worktree root. Sirio's own `Cargo.toml` lives in `rust/`, so a
//! rust-analyzer rooted at the worktree would find no project at all — the
//! feature would fail on the repository it is written in. Instead the file's
//! directory is walked upward looking for the language's marker files.
//!
//! The walk stops at the worktree root and never climbs past it. A stray
//! `Cargo.toml` in `$HOME` would otherwise capture every Rust file on the
//! machine into a single server rooted at the home directory.

use std::path::{Path, PathBuf};

/// The directory a server for `file` should run in.
///
/// Searches `file`'s own directory first and then each ancestor up to and
/// including `worktree_root`, returning the first that contains any of
/// `markers`. Falls back to `worktree_root`: no marker is a normal state (a
/// scratch file in a repo with no manifest), not an error.
pub fn project_root(file: &Path, worktree_root: &Path, markers: &[String]) -> PathBuf {
    let Some(start) = file.parent() else {
        return worktree_root.to_path_buf();
    };
    if !start.starts_with(worktree_root) {
        return worktree_root.to_path_buf();
    }
    if markers.is_empty() {
        return worktree_root.to_path_buf();
    }

    let mut directory = Some(start);
    while let Some(current) = directory {
        if markers.iter().any(|marker| current.join(marker).exists()) {
            return current.to_path_buf();
        }
        if current == worktree_root {
            break;
        }
        directory = current.parent();
    }
    worktree_root.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway directory tree. Returns the root; `paths` are created as
    /// empty files, parents included.
    fn tree(label: &str, paths: &[&str]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "sirio-lsp-roots-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for path in paths {
            let full = root.join(path);
            std::fs::create_dir_all(full.parent().expect("a file has a parent")).unwrap();
            std::fs::write(&full, b"").unwrap();
        }
        root
    }

    fn markers(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn a_marker_beside_the_file_roots_there() {
        let root = tree("beside", &["app/Cargo.toml", "app/src/main.rs"]);
        assert_eq!(
            project_root(&root.join("app/src/main.rs"), &root, &markers(&["Cargo.toml"])),
            root.join("app")
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_marker_several_levels_up_is_found() {
        // This is Sirio's own shape: the manifest is in rust/, the file is
        // several directories below it, and the worktree root has neither.
        let root = tree(
            "deep",
            &["rust/Cargo.toml", "rust/crates/sirio_lsp/src/roots.rs"],
        );
        assert_eq!(
            project_root(
                &root.join("rust/crates/sirio_lsp/src/roots.rs"),
                &root,
                &markers(&["Cargo.toml"])
            ),
            root.join("rust")
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn no_marker_anywhere_falls_back_to_the_worktree_root() {
        // A scratch file in a repo with no manifest must still work.
        let root = tree("none", &["notes/scratch.rs"]);
        assert_eq!(
            project_root(&root.join("notes/scratch.rs"), &root, &markers(&["Cargo.toml"])),
            root
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_walk_stops_at_the_worktree_root_and_does_not_escape_it() {
        // The marker sits ABOVE the worktree. Finding it would root a server
        // outside the worktree the user is working in — and on a machine
        // where $HOME has a stray Cargo.toml, every Rust file in every repo
        // would share one server.
        let outer = tree("escape", &["Cargo.toml", "repo/src/main.rs"]);
        let worktree = outer.join("repo");
        assert_eq!(
            project_root(&worktree.join("src/main.rs"), &worktree, &markers(&["Cargo.toml"])),
            worktree
        );
        std::fs::remove_dir_all(&outer).ok();
    }

    #[test]
    fn the_nearest_marker_wins_when_there_are_two() {
        // A workspace manifest at the top and a crate manifest below: the
        // crate's own root is the more specific answer.
        let root = tree("nearest", &["Cargo.toml", "crates/a/Cargo.toml", "crates/a/src/lib.rs"]);
        assert_eq!(
            project_root(&root.join("crates/a/src/lib.rs"), &root, &markers(&["Cargo.toml"])),
            root.join("crates/a")
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn any_of_several_markers_matches() {
        let root = tree("several", &["web/tsconfig.json", "web/src/app.ts"]);
        assert_eq!(
            project_root(
                &root.join("web/src/app.ts"),
                &root,
                &markers(&["package.json", "tsconfig.json"])
            ),
            root.join("web")
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_empty_marker_list_roots_at_the_worktree() {
        let root = tree("empty", &["a/b.rs"]);
        assert_eq!(project_root(&root.join("a/b.rs"), &root, &[]), root);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_file_outside_the_worktree_roots_at_the_worktree() {
        // Defensive: the caller resolved the worktree from the path, so this
        // should not happen — but guessing a root outside it would be worse
        // than the obvious fallback.
        let root = tree("outside", &["repo/Cargo.toml"]);
        let worktree = root.join("repo");
        assert_eq!(
            project_root(std::path::Path::new("/elsewhere/x.rs"), &worktree, &markers(&["Cargo.toml"])),
            worktree
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_worktree_root_itself_is_searched() {
        let root = tree("atroot", &["Cargo.toml", "src/main.rs"]);
        assert_eq!(
            project_root(&root.join("src/main.rs"), &root, &markers(&["Cargo.toml"])),
            root
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
