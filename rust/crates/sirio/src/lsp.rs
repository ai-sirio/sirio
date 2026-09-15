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
    /// Consumes `server.incoming()`: answers every server request and
    /// delivers every diagnostic. Without it the channel — unbounded, so it
    /// never blocks — simply grows, and nothing ever reads a diagnostic.
    pub router: gpui::Task<()>,
}

/// What we answer a server request with: `Ok` is a result, `Err` is a
/// JSON-RPC error — and **both are answers**. The only wrong move is
/// silence, which leaves the server blocked with nothing in any log.
pub fn answer_for(
    method: &str,
    params: &serde_json::Value,
) -> Result<serde_json::Value, (i64, String)> {
    match method {
        "workspace/configuration" => {
            // One entry per requested item, each `null` for "nothing
            // configured". A bare `null` here is a shape error that some
            // servers treat as a failed request.
            let items = params
                .get("items")
                .and_then(serde_json::Value::as_array)
                .map_or(0, Vec::len);
            Ok(serde_json::Value::Array(vec![serde_json::Value::Null; items]))
        }
        // We declare no dynamic registration, so these should not arrive —
        // but a server that sends one anyway must not be left waiting.
        "client/registerCapability"
        | "client/unregisterCapability"
        | "window/workDoneProgress/create" => Ok(serde_json::Value::Null),
        // JSON-RPC's own "I do not implement this". Saying so lets the
        // server carry on instead of blocking on us.
        other => Err((-32601, format!("{other} is not implemented by Sirio"))),
    }
}

/// Converts the protocol's positions into the line-and-byte terms the view
/// speaks. **This is where UTF-16 stops.**
///
/// A range that no longer fits the buffer is clamped rather than dropped:
/// the text moves while the server is still thinking, so a stale range is
/// ordinary, and losing the finding would be worse than showing it a
/// character off.
pub fn view_diagnostics(
    text: &str,
    raw: &[sirio_lsp::RawDiagnostic],
) -> Vec<sirio_ui::file_view::FileDiagnostic> {
    use sirio_ui::file_view::{DiagnosticSeverity, FileDiagnostic};

    let index = sirio_lsp::LineIndex::new(text);
    raw.iter()
        .map(|finding| {
            let start = index.offset(finding.start).unwrap_or(text.len());
            let end = index.offset(finding.end).unwrap_or(text.len()).max(start);
            FileDiagnostic {
                line: finding.start.line as usize,
                range: start..end,
                severity: match finding.severity {
                    sirio_lsp::Severity::Error => DiagnosticSeverity::Error,
                    sirio_lsp::Severity::Warning => DiagnosticSeverity::Warning,
                    sirio_lsp::Severity::Information => DiagnosticSeverity::Information,
                    sirio_lsp::Severity::Hint => DiagnosticSeverity::Hint,
                },
                message: finding.message.clone(),
            }
        })
        .collect()
}

pub struct LspSupervisor {
    loader: ConfigLoader,
    servers: HashMap<(PathBuf, String), ServerHandle>,
    /// The version each open document is at, per the protocol's requirement
    /// that it only ever rise.
    versions: sirio_lsp::DocumentVersions,
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
            versions: sirio_lsp::DocumentVersions::default(),
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

    pub fn versions_mut(&mut self) -> &mut sirio_lsp::DocumentVersions {
        &mut self.versions
    }

    /// The running server a path would use, if there is one.
    pub fn server_for(&self, key: &(PathBuf, String)) -> Option<&Server> {
        self.servers.get(key).map(|handle| &handle.server)
    }

    /// Whether a running server that offers definitions covers `path`.
    /// Read-only on purpose: the menu facts are computed from `&self`, and
    /// the key-building entry lookup needs `&mut` for its table reload —
    /// so this matches by containment instead of recomputing the key.
    pub fn definition_available(&self, path: &Path) -> bool {
        self.servers.iter().any(|((root, _), handle)| {
            path.starts_with(root) && handle.server.capabilities().definition
        })
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

    #[test]
    fn a_configuration_request_is_answered_with_one_null_per_item() {
        // The protocol requires one entry per requested item. `null` in
        // each says "no configuration", which is exactly true of Sirio.
        let params = serde_json::json!({ "items": [ {"section": "rust-analyzer"},
                                                    {"section": "rust-analyzer.cargo"} ] });
        let answer = answer_for("workspace/configuration", &params).expect("answered");
        assert_eq!(answer, serde_json::json!([null, null]));
    }

    #[test]
    fn a_registration_request_is_accepted() {
        let answer = answer_for("client/registerCapability", &serde_json::json!({}))
            .expect("answered");
        assert_eq!(answer, serde_json::Value::Null);
    }

    #[test]
    fn an_unknown_request_is_refused_with_method_not_found() {
        // Refusing is an answer. Silence is what stalls a server, and a
        // stalled server produces no error anywhere.
        let (code, message) = answer_for("window/showMessageRequest", &serde_json::json!({}))
            .expect_err("refused");
        assert_eq!(code, -32601);
        assert!(message.contains("window/showMessageRequest"));
    }

    #[test]
    fn diagnostics_arrive_in_line_and_byte_terms() {
        // The emoji is the point: LSP counts `character` in UTF-16 code
        // units, so column 2 here is byte 4. Getting this wrong underlines
        // the wrong text and is invisible on ASCII fixtures.
        let text = "let a = 1;\nlet \u{1f980} = 2;\n";
        let raw = vec![sirio_lsp::RawDiagnostic {
            start: sirio_lsp::lsp_types::Position { line: 1, character: 4 },
            end: sirio_lsp::lsp_types::Position { line: 1, character: 6 },
            severity: sirio_lsp::Severity::Warning,
            message: "unused variable".to_owned(),
        }];
        let converted = view_diagnostics(text, &raw);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].line, 1);
        assert_eq!(converted[0].message, "unused variable");
        let crab = &text[converted[0].range.clone()];
        assert_eq!(crab, "\u{1f980}", "the range must cover the emoji, not half of it");
    }

    #[test]
    fn a_diagnostic_past_the_end_of_the_buffer_is_clamped_not_dropped() {
        // The buffer moves while the server is still thinking, so a stale
        // range outliving its text is normal. Panicking on it is not.
        let text = "one line\n";
        let raw = vec![sirio_lsp::RawDiagnostic {
            start: sirio_lsp::lsp_types::Position { line: 40, character: 0 },
            end: sirio_lsp::lsp_types::Position { line: 40, character: 5 },
            severity: sirio_lsp::Severity::Error,
            message: "stale".to_owned(),
        }];
        let converted = view_diagnostics(text, &raw);
        assert_eq!(converted.len(), 1);
        assert!(converted[0].range.start <= text.len());
        assert!(converted[0].range.end <= text.len());
        assert!(converted[0].range.start <= converted[0].range.end);
    }
}
