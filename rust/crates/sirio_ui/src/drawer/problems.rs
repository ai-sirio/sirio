//! What every language server last said about every file.
//!
//! Ways this can fail, written before the code:
//! - a publish adds to a server's earlier entries for the path instead of
//!   replacing them, so a fixed error stays listed;
//! - an empty publish (how a server says "fixed") leaves the path behind, so
//!   an empty file group is drawn;
//! - two servers on one path overwrite each other, or one's empty publish
//!   wipes the other's findings;
//! - a server that exits takes another server's entries with it, or leaves
//!   its own to be listed forever;
//! - Information and Hint are counted as nothing, or as warnings, instead of
//!   as notes;
//! - `files` lists the current tab's file in the middle, lists a path whose
//!   entries are all gone, or invents the current path when it has none.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sirio_lsp::Severity;

use super::ServerKey;

/// One finding as Problems lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredProblem {
    pub line: u32,
    /// UTF-16 code units, 0-based, as the protocol counts.
    pub column: u32,
    pub severity: Severity,
    pub message: String,
    pub source: Option<String>,
}

/// The totals the drawer's title shows. Information and Hint are notes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProblemCounts {
    pub errors: usize,
    pub warnings: usize,
    pub notes: usize,
}

/// Findings keyed by path, then by the server that reported them.
#[derive(Debug, Default)]
pub struct ProblemStore {
    by_path: BTreeMap<PathBuf, BTreeMap<ServerKey, Vec<StoredProblem>>>,
}

impl ProblemStore {
    /// Replaces `server`'s findings for `path`; an empty list clears them.
    pub fn publish(&mut self, server: &ServerKey, path: &Path, problems: Vec<StoredProblem>) {
        if problems.is_empty() {
            if let Some(servers) = self.by_path.get_mut(path) {
                servers.remove(server);
                if servers.is_empty() {
                    self.by_path.remove(path);
                }
            }
            return;
        }
        self.by_path
            .entry(path.to_path_buf())
            .or_default()
            .insert(server.clone(), problems);
    }

    /// Drops everything `server` reported, and nothing another server did.
    pub fn server_gone(&mut self, server: &ServerKey) {
        self.by_path.retain(|_, servers| {
            servers.remove(server);
            !servers.is_empty()
        });
    }

    pub fn errors(&self) -> usize {
        self.counts().errors
    }

    pub fn counts(&self) -> ProblemCounts {
        let mut counts = ProblemCounts::default();
        for problem in self.by_path.values().flat_map(|s| s.values().flatten()) {
            match problem.severity {
                Severity::Error => counts.errors += 1,
                Severity::Warning => counts.warnings += 1,
                Severity::Information | Severity::Hint => counts.notes += 1,
            }
        }
        counts
    }

    /// Every server's findings merged per path, `first` leading when it has
    /// any; the rest keep path order, which the panel then sorts its own way.
    pub fn files(&self, first: Option<&Path>) -> Vec<(PathBuf, Vec<StoredProblem>)> {
        let merged = |path: &PathBuf, servers: &BTreeMap<ServerKey, Vec<StoredProblem>>| {
            (path.clone(), servers.values().flatten().cloned().collect())
        };
        let mut listed = Vec::with_capacity(self.by_path.len());
        if let Some((path, servers)) = first.and_then(|f| self.by_path.get_key_value(f)) {
            listed.push(merged(path, servers));
        }
        for (path, servers) in &self.by_path {
            if Some(path.as_path()) != first {
                listed.push(merged(path, servers));
            }
        }
        listed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sirio_lsp::Severity;
    use std::path::{Path, PathBuf};

    fn key(language: &str) -> ServerKey {
        (PathBuf::from("/project"), language.to_owned())
    }

    fn problem(line: u32, severity: Severity, message: &str) -> StoredProblem {
        StoredProblem {
            line,
            column: 0,
            severity,
            message: message.to_owned(),
            source: None,
        }
    }

    fn messages(store: &ProblemStore, path: &str) -> Vec<String> {
        store
            .files(None)
            .into_iter()
            .filter(|(p, _)| p == Path::new(path))
            .flat_map(|(_, found)| found.into_iter().map(|p| p.message))
            .collect()
    }

    #[test]
    fn a_publish_replaces_that_servers_entries_for_that_path() {
        let mut store = ProblemStore::default();
        let rust = key("rust");
        let path = Path::new("/project/a.rs");
        store.publish(&rust, path, vec![problem(1, Severity::Error, "old")]);
        store.publish(&rust, path, vec![problem(2, Severity::Error, "new")]);
        assert_eq!(messages(&store, "/project/a.rs"), vec!["new"]);
        assert_eq!(store.errors(), 1);
    }

    #[test]
    fn an_empty_publish_clears_the_path() {
        let mut store = ProblemStore::default();
        let rust = key("rust");
        let path = Path::new("/project/a.rs");
        store.publish(&rust, path, vec![problem(1, Severity::Error, "boom")]);
        store.publish(&rust, path, Vec::new());
        assert!(store.files(None).is_empty());
        assert_eq!(store.errors(), 0);
    }

    #[test]
    fn two_servers_on_one_path_are_kept_apart_and_merged_in_files() {
        let mut store = ProblemStore::default();
        let path = Path::new("/project/a.ts");
        store.publish(&key("ts"), path, vec![problem(1, Severity::Error, "tsc")]);
        store.publish(
            &key("eslint"),
            path,
            vec![problem(2, Severity::Warning, "lint")],
        );
        let mut found = messages(&store, "/project/a.ts");
        found.sort();
        assert_eq!(found, vec!["lint", "tsc"]);
        // One server clearing the path leaves the other's findings.
        store.publish(&key("eslint"), path, Vec::new());
        assert_eq!(messages(&store, "/project/a.ts"), vec!["tsc"]);
    }

    #[test]
    fn server_gone_drops_only_that_servers_entries() {
        let mut store = ProblemStore::default();
        let path = Path::new("/project/a.ts");
        store.publish(&key("ts"), path, vec![problem(1, Severity::Error, "tsc")]);
        store.publish(
            &key("eslint"),
            path,
            vec![problem(2, Severity::Error, "lint")],
        );
        store.publish(
            &key("ts"),
            Path::new("/project/b.ts"),
            vec![problem(3, Severity::Error, "tsc b")],
        );
        store.server_gone(&key("ts"));
        assert_eq!(messages(&store, "/project/a.ts"), vec!["lint"]);
        assert!(messages(&store, "/project/b.ts").is_empty());
        assert_eq!(store.errors(), 1);
    }

    #[test]
    fn counts_group_information_and_hint_as_notes() {
        let mut store = ProblemStore::default();
        store.publish(
            &key("rust"),
            Path::new("/project/a.rs"),
            vec![
                problem(1, Severity::Error, "e"),
                problem(2, Severity::Warning, "w"),
                problem(3, Severity::Information, "i"),
                problem(4, Severity::Hint, "h"),
            ],
        );
        let counts = store.counts();
        assert_eq!((counts.errors, counts.warnings, counts.notes), (1, 1, 2));
        assert_eq!(store.errors(), 1);
    }

    #[test]
    fn files_puts_the_given_path_first_and_omits_empty_paths() {
        let mut store = ProblemStore::default();
        let rust = key("rust");
        for name in ["a.rs", "b.rs", "c.rs"] {
            store.publish(
                &rust,
                &Path::new("/project").join(name),
                vec![problem(1, Severity::Error, name)],
            );
        }
        store.publish(&rust, Path::new("/project/d.rs"), Vec::new());
        let first = Path::new("/project/c.rs");
        let listed: Vec<PathBuf> = store
            .files(Some(first))
            .into_iter()
            .map(|(path, _)| path)
            .collect();
        assert_eq!(listed.len(), 3, "the empty path is not listed");
        assert_eq!(listed[0], first);
        // A first path with nothing to show is not invented.
        let none: Vec<PathBuf> = store
            .files(Some(Path::new("/project/zzz.rs")))
            .into_iter()
            .map(|(path, _)| path)
            .collect();
        assert_eq!(none.len(), 3);
        assert!(!none.contains(&PathBuf::from("/project/zzz.rs")));
    }
}
