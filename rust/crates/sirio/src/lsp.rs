//! The running language servers, and the decision of which one a file wants.
//!
//! Lives in `sirio` rather than in `sirio_lsp` because it owns processes and
//! gpui tasks, and the repo's rule is that processes are wired here — the
//! same reason `sirio_ui` touches neither `sirio_terminal` nor
//! `sirio_control`.
//!
//! Servers are keyed by `(project root, language)`, not by worktree: two
//! Rust workspaces inside one worktree need two servers, and this repository
//! is itself a case where the project root (`rust/`) is not the worktree root.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sirio_lsp::{ConfigLoader, LanguageEntry, Reload, Server};

/// One running server: the connection, plus the gpui task driving its read
/// loop. Dropping the task stops the loop, so the handle owning both is what
/// keeps a server alive.
pub struct ServerHandle {
    pub server: Server,
    pub read_loop: gpui::Task<()>,
}

pub struct LspSupervisor {
    loader: ConfigLoader,
    servers: HashMap<(PathBuf, String), ServerHandle>,
    /// Keys whose server died. Kept so a crashed server is not restarted in
    /// a loop: one that crashes on a file crashes again on restart, and on
    /// rust-analyzer that means re-indexing the repository forever.
    dead: Vec<(PathBuf, String)>,
}

impl LspSupervisor {
    pub fn new(loader: ConfigLoader) -> Self {
        Self {
            loader,
            servers: HashMap::new(),
            dead: Vec::new(),
        }
    }

    /// The entry a file wants, re-reading the table first if it has moved.
    /// This is the whole of the lazy-reload behaviour: the file is consulted
    /// exactly when it is about to matter.
    ///
    /// Returns the reload outcome alongside so the caller can surface a
    /// broken table once, at the moment it was noticed.
    pub fn entry_for_with_reload(&mut self, file: &Path) -> (Option<LanguageEntry>, Reload) {
        let reload = self.loader.refresh();
        let entry = self.loader.table().for_path(file).cloned();
        (entry, reload)
    }

    pub fn entry_for(&mut self, file: &Path) -> Option<LanguageEntry> {
        self.entry_for_with_reload(file).0
    }

    pub fn key_for(
        &self,
        file: &Path,
        worktree_root: &Path,
        entry: &LanguageEntry,
    ) -> (PathBuf, String) {
        (
            sirio_lsp::project_root(file, worktree_root, &entry.roots),
            entry.name.clone(),
        )
    }

    pub fn is_running(&self, key: &(PathBuf, String)) -> bool {
        self.servers.contains_key(key)
    }

    pub fn is_dead(&self, key: &(PathBuf, String)) -> bool {
        self.dead.contains(key)
    }

    pub fn mark_dead(&mut self, key: (PathBuf, String)) {
        self.servers.remove(&key);
        if !self.dead.contains(&key) {
            self.dead.push(key);
        }
    }

    pub fn insert(&mut self, key: (PathBuf, String), handle: ServerHandle) {
        self.servers.insert(key, handle);
    }

    /// Empties the registry, handing every server back for shutdown. Used by
    /// `cx.on_app_quit`: the caller stops each one, and the kill after the
    /// grace period is the contract, not a fallback.
    pub fn take_all(&mut self) -> Vec<ServerHandle> {
        self.servers.drain().map(|(_, handle)| handle).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader_with_defaults() -> sirio_lsp::ConfigLoader {
        // A path that will never exist, so the loader sits on the defaults.
        sirio_lsp::ConfigLoader::new(
            std::env::temp_dir().join("sirio-lsp-supervisor-absent/languages.toml"),
        )
    }

    #[test]
    fn a_rust_file_resolves_to_the_rust_entry() {
        let mut supervisor = LspSupervisor::new(loader_with_defaults());
        let entry = supervisor
            .entry_for(std::path::Path::new("/repo/src/main.rs"))
            .expect("a .rs file has an entry among the defaults");
        assert_eq!(entry.name, "rust");
        assert_eq!(entry.command, "rust-analyzer");
    }

    #[test]
    fn a_file_with_no_language_resolves_to_nothing_quietly() {
        let mut supervisor = LspSupervisor::new(loader_with_defaults());
        assert!(
            supervisor
                .entry_for(std::path::Path::new("/repo/NOTES.txt"))
                .is_none()
        );
    }

    #[test]
    fn the_key_pairs_the_project_root_with_the_language() {
        let mut supervisor = LspSupervisor::new(loader_with_defaults());
        let entry = supervisor
            .entry_for(std::path::Path::new("/repo/src/main.rs"))
            .expect("rust entry");
        let key = supervisor.key_for(
            std::path::Path::new("/repo/src/main.rs"),
            std::path::Path::new("/repo"),
            &entry,
        );
        // No Cargo.toml exists at that path, so the root falls back to the
        // worktree — the pairing is what this test pins down.
        assert_eq!(key, (std::path::PathBuf::from("/repo"), "rust".to_owned()));
    }

    #[test]
    fn two_languages_under_one_root_are_two_different_keys() {
        // The registry must not collapse a Rust server and a Go server that
        // happen to share a directory.
        let mut supervisor = LspSupervisor::new(loader_with_defaults());
        let rust = supervisor
            .entry_for(std::path::Path::new("/repo/a.rs"))
            .expect("rust");
        let go = supervisor
            .entry_for(std::path::Path::new("/repo/a.go"))
            .expect("go");
        let root = std::path::Path::new("/repo");
        assert_ne!(
            supervisor.key_for(std::path::Path::new("/repo/a.rs"), root, &rust),
            supervisor.key_for(std::path::Path::new("/repo/a.go"), root, &go),
        );
    }
}
