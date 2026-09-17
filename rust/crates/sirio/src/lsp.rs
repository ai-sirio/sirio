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

use sirio_lsp::{Capabilities, ConfigLoader, LanguageEntry, LanguageTable, Reload, Server};

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
            Ok(serde_json::Value::Array(vec![
                serde_json::Value::Null;
                items
            ]))
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

/// Which `(project root, language)` a file resolves to, given a table.
///
/// Separated from the registry on purpose: this is the decision that used
/// to be approximated by a containment match, and as a free function over
/// a plain table it is testable with no server running anywhere.
pub fn key_for_file(
    table: &LanguageTable,
    file: &Path,
    worktree_root: &Path,
) -> Option<(PathBuf, String)> {
    let entry = table.for_path(file)?;
    Some((
        sirio_lsp::project_root(file, worktree_root, &entry.roots),
        entry.name.clone(),
    ))
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

/// Converts the protocol's symbol kinds into the ones the overlay draws.
///
/// The mirror of [`view_diagnostics`], and for the same reason: a
/// `sirio_lsp::SymbolKind` is the protocol's table, a
/// `sirio_ui::outline::OutlineKind` is a case a view can paint, and the app
/// is the only place that knows both.
pub fn view_symbols(raw: &[sirio_lsp::Symbol]) -> Vec<sirio_ui::outline::OutlineSymbol> {
    use sirio_ui::outline::{OutlineKind, OutlineSymbol};

    raw.iter()
        .map(|symbol| OutlineSymbol {
            name: symbol.name.clone(),
            detail: symbol.detail.clone(),
            kind: match symbol.kind {
                sirio_lsp::SymbolKind::Function => OutlineKind::Function,
                sirio_lsp::SymbolKind::Method => OutlineKind::Method,
                sirio_lsp::SymbolKind::Struct => OutlineKind::Struct,
                sirio_lsp::SymbolKind::Enum => OutlineKind::Enum,
                sirio_lsp::SymbolKind::Interface => OutlineKind::Interface,
                sirio_lsp::SymbolKind::Field => OutlineKind::Field,
                sirio_lsp::SymbolKind::Constant => OutlineKind::Constant,
                sirio_lsp::SymbolKind::Variable => OutlineKind::Variable,
                sirio_lsp::SymbolKind::Module => OutlineKind::Module,
                sirio_lsp::SymbolKind::Other => OutlineKind::Other,
            },
            line: symbol.line as usize,
            depth: symbol.depth,
        })
        .collect()
}

pub struct LspSupervisor {
    loader: ConfigLoader,
    servers: HashMap<(PathBuf, String), ServerHandle>,
    /// The version each open document is at, per the protocol's requirement
    /// that it only ever rise.
    versions: sirio_lsp::DocumentVersions,
    /// Keys with no server, and why. Kept so a crashed server is not
    /// restarted in a loop: one that crashes on a file crashes again on
    /// restart, and on rust-analyzer that means re-indexing the repository
    /// forever.
    dead: HashMap<(PathBuf, String), Dead>,
    /// The text each open document was last *sent* with.
    ///
    /// The server's copy of a file is only ever as good as this, so this is
    /// what "has the document changed" is answered against — not the view's
    /// notify, which also fires for a caret blink. Holding the text rather
    /// than a revision counter is deliberate: it needs no cooperation from
    /// `Editor`'s eleven buffer-write sites, and a `String` comparison
    /// checks length before it reads a byte.
    synced: HashMap<PathBuf, String>,
}

/// Why a key has no server and will not be tried again — and, for the arms
/// that offer one, what the reader can do about it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Dead {
    /// The command is not on `PATH` and Sirio can fetch it.
    Installable { recipe: sirio_lsp::Recipe },
    /// The command is not on `PATH` and something has to come first.
    Manual {
        needs: &'static str,
        url: &'static str,
    },
    /// The command names a program that is not there and Sirio has no
    /// recipe for it: an entry of the reader's own, which carries no recipe
    /// on purpose — Sirio must not offer a second copy of a server they
    /// already chose. The name they wrote is all there is to give back.
    NotInstalled { command: String },
    /// It launched and then failed. Already shown on a card at the moment
    /// it happened; nothing further is owed, and reinstalling is not the
    /// cure.
    Failed,
}

/// What a document needs told to a server, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentSync {
    /// The server has never seen this document: send `textDocument/didOpen`.
    Open { version: i32 },
    /// The server's copy is stale: send `textDocument/didChange`.
    Change { version: i32 },
}

impl LspSupervisor {
    pub fn new(loader: ConfigLoader) -> Self {
        Self {
            loader,
            servers: HashMap::new(),
            versions: sirio_lsp::DocumentVersions::default(),
            dead: HashMap::new(),
            synced: HashMap::new(),
        }
    }

    /// Records that `path` is about to be sent as `text`, and says which
    /// notification that is. `None` when the server already has this exact
    /// text — the common answer, since every caret move asks.
    ///
    /// Recording happens here rather than at the send site because the send
    /// is asynchronous: two notifies arriving before the first write lands
    /// would otherwise both decide the server needs telling.
    pub fn sync_text(&mut self, path: &Path, text: &str) -> Option<DocumentSync> {
        match self.synced.get(path) {
            Some(sent) if sent == text => None,
            Some(_) => {
                self.synced.insert(path.to_path_buf(), text.to_owned());
                Some(DocumentSync::Change {
                    version: self.versions.changed(path),
                })
            }
            None => {
                self.synced.insert(path.to_path_buf(), text.to_owned());
                Some(DocumentSync::Open {
                    version: self.versions.opened(path),
                })
            }
        }
    }

    /// The entry a file wants, from the table **as already loaded**.
    ///
    /// [`Self::entry_for`] re-reads the table from disk first, which is the
    /// right freshness step when a file is being opened and the wrong one
    /// on a path that runs on every view notify. Opening still refreshes;
    /// keeping a server in step does not need to.
    pub fn known_entry_for(&self, file: &Path) -> Option<LanguageEntry> {
        self.loader.table().for_path(file).cloned()
    }

    /// The entry whose `name` is this language, from the table as already
    /// loaded. `known_entry_for` answers by path; the install action knows
    /// only the language, because that is what the card's button carries.
    pub fn known_entry_for_language(&self, language: &str) -> Option<LanguageEntry> {
        self.loader
            .table()
            .entries()
            .iter()
            .find(|entry| entry.name == language)
            .cloned()
    }

    /// The binary Sirio installed for this entry, if it installed one and
    /// the file is still there. Consulted only after a spawn by name has
    /// answered `NotInstalled`, which is what makes PATH win.
    ///
    /// A manifest outliving its executable is not an offer — that is a
    /// half-deleted install, and handing back a path that is not there
    /// would fail with a message about a path nobody recognises.
    pub fn installed_binary_for(
        &self,
        entry: &LanguageEntry,
        store: &sirio_registry::InstallStore,
    ) -> Option<PathBuf> {
        let id = entry.install.as_ref()?.store_id()?;
        let installed = store.manifest(id)?;
        installed.executable.exists().then_some(installed.executable)
    }

    /// Whether the server already holds exactly this text.
    ///
    /// Read-only and cheap on purpose: the observer that watches a file view
    /// fires on every notify — a caret blink included — and this is what
    /// keeps that from reaching a config-file stat twice a second.
    pub fn is_synced(&self, path: &Path, text: &str) -> bool {
        self.synced.get(path).is_some_and(|sent| sent == text)
    }

    /// Forgets what a closed document was synced with, so reopening it is a
    /// fresh `didOpen` rather than a `didChange` against a document the
    /// server no longer holds.
    pub fn forget_synced(&mut self, path: &Path) {
        self.synced.remove(path);
        self.versions.closed(path);
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
        self.dead.contains_key(key)
    }

    pub fn mark_dead(&mut self, key: (PathBuf, String), reason: Dead) {
        self.servers.remove(&key);
        self.dead.entry(key).or_insert(reason);
    }

    /// Lets a key be launched again. **One caller only:** an install that
    /// succeeded. Not a timer, not another tab opening, not a refresh —
    /// and never for `Failed`, which is what keeps a crashing server from
    /// being restarted in a loop.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn revive(&mut self, key: &(PathBuf, String)) {
        if matches!(self.dead.get(key), Some(Dead::Failed)) {
            return;
        }
        self.dead.remove(key);
    }

    /// Why this file's language has no server, when there is a reason worth
    /// giving. Replaces `missing_command_for`, which could only say a name.
    ///
    /// `None` covers three different situations that need no reason: no
    /// entry claims the extension, a server is running, or one is on its
    /// way. The reason `Failed` gives is the one the reader already had.
    pub fn dead_reason_for(&self, file: &Path, worktree_root: &Path) -> Option<&Dead> {
        let key = key_for_file(self.table(), file, worktree_root)?;
        self.dead.get(&key)
    }

    pub fn insert(&mut self, key: (PathBuf, String), handle: ServerHandle) {
        self.servers.insert(key, handle);
    }

    /// The running server a path would use, if there is one.
    pub fn server_for(&self, key: &(PathBuf, String)) -> Option<&Server> {
        self.servers.get(key).map(|handle| &handle.server)
    }

    /// The table as already loaded, with no refresh. The refresh in
    /// [`Self::entry_for`] is a freshness step that belongs to opening a
    /// file; it is also the only reason that method needs `&mut self`, and
    /// the only reason the capability questions below used to be answered
    /// by a containment match rather than an exact lookup.
    pub fn table(&self) -> &LanguageTable {
        self.loader.table()
    }

    /// The negotiated capabilities of the server this file would actually
    /// use. `None` when no entry claims the extension, or when no server is
    /// running for that exact (project root, language) pair — so a Python
    /// file inside a Rust project answers `None`, which is the whole point.
    pub fn capability_for(&self, file: &Path, worktree_root: &Path) -> Option<Capabilities> {
        let key = key_for_file(self.table(), file, worktree_root)?;
        self.servers
            .get(&key)
            .map(|handle| handle.server.capabilities())
    }

    pub fn definition_available(&self, file: &Path, worktree_root: &Path) -> bool {
        self.capability_for(file, worktree_root)
            .is_some_and(|offered| offered.definition)
    }

    pub fn references_available(&self, file: &Path, worktree_root: &Path) -> bool {
        self.capability_for(file, worktree_root)
            .is_some_and(|offered| offered.references)
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

    impl LspSupervisor {
        fn table_for_test(&self) -> &sirio_lsp::LanguageTable {
            self.table()
        }
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
        let answer =
            answer_for("client/registerCapability", &serde_json::json!({})).expect("answered");
        assert_eq!(answer, serde_json::Value::Null);
    }

    #[test]
    fn an_unknown_request_is_refused_with_method_not_found() {
        // Refusing is an answer. Silence is what stalls a server, and a
        // stalled server produces no error anywhere.
        let (code, message) =
            answer_for("window/showMessageRequest", &serde_json::json!({})).expect_err("refused");
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
            start: sirio_lsp::lsp_types::Position {
                line: 1,
                character: 4,
            },
            end: sirio_lsp::lsp_types::Position {
                line: 1,
                character: 6,
            },
            severity: sirio_lsp::Severity::Warning,
            message: "unused variable".to_owned(),
        }];
        let converted = view_diagnostics(text, &raw);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].line, 1);
        assert_eq!(converted[0].message, "unused variable");
        let crab = &text[converted[0].range.clone()];
        assert_eq!(
            crab, "\u{1f980}",
            "the range must cover the emoji, not half of it"
        );
    }

    #[test]
    fn a_diagnostic_past_the_end_of_the_buffer_is_clamped_not_dropped() {
        // The buffer moves while the server is still thinking, so a stale
        // range outliving its text is normal. Panicking on it is not.
        let text = "one line\n";
        let raw = vec![sirio_lsp::RawDiagnostic {
            start: sirio_lsp::lsp_types::Position {
                line: 40,
                character: 0,
            },
            end: sirio_lsp::lsp_types::Position {
                line: 40,
                character: 5,
            },
            severity: sirio_lsp::Severity::Error,
            message: "stale".to_owned(),
        }];
        let converted = view_diagnostics(text, &raw);
        assert_eq!(converted.len(), 1);
        assert!(converted[0].range.start <= text.len());
        assert!(converted[0].range.end <= text.len());
        assert!(converted[0].range.start <= converted[0].range.end);
    }

    #[test]
    fn a_python_file_under_a_rust_root_resolves_to_a_different_key() {
        // The regression this closes: the old containment match asked only
        // whether a running server's root was a prefix of the path, so
        // rust-analyzer's root made every file under it look served —
        // including files it cannot say anything about.
        let mut supervisor = LspSupervisor::new(loader_with_defaults());
        let table = supervisor.table_for_test();
        let root = std::path::Path::new("/repo");

        let rust = key_for_file(table, std::path::Path::new("/repo/src/main.rs"), root)
            .expect("a .rs file resolves");
        let python = key_for_file(table, std::path::Path::new("/repo/tool.py"), root)
            .expect("a .py file resolves");

        assert_ne!(
            rust, python,
            "a Rust server's key must not be the key a Python file resolves to"
        );
        assert_eq!(rust.1, "rust");
        assert_eq!(python.1, "python");
    }

    #[test]
    fn a_file_no_entry_claims_resolves_to_no_key_at_all() {
        // No key means no server, which means the menu entry is disabled
        // with a reason rather than enabled and inert.
        let mut supervisor = LspSupervisor::new(loader_with_defaults());
        let table = supervisor.table_for_test();
        assert!(
            key_for_file(
                table,
                std::path::Path::new("/repo/NOTES.txt"),
                std::path::Path::new("/repo")
            )
            .is_none()
        );
    }

    #[test]
    fn symbols_arrive_in_the_view_s_own_vocabulary() {
        // The protocol's kind is a number in a table; the view's is a case
        // it can draw. Converting here is what keeps sirio_ui free of
        // lsp-types — the same reason view_diagnostics exists above.
        let raw = vec![
            sirio_lsp::Symbol {
                name: "LspSupervisor".to_owned(),
                detail: Some("struct".to_owned()),
                kind: sirio_lsp::SymbolKind::Struct,
                line: 10,
                depth: 0,
            },
            sirio_lsp::Symbol {
                name: "servers".to_owned(),
                detail: None,
                kind: sirio_lsp::SymbolKind::Field,
                line: 12,
                depth: 1,
            },
        ];
        let converted = view_symbols(&raw);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].name, "LspSupervisor");
        assert_eq!(converted[0].kind, sirio_ui::outline::OutlineKind::Struct);
        assert_eq!(converted[0].detail.as_deref(), Some("struct"));
        assert_eq!(converted[0].line, 10);
        assert_eq!(converted[1].depth, 1, "the indent survives the crossing");
        assert_eq!(converted[1].kind, sirio_ui::outline::OutlineKind::Field);
    }

    /// One filename per language the editor recognises, and the language it
    /// must resolve to. Two lists that have to agree — the editor's and the
    /// server table's — and this is the only place both are visible, since
    /// `sirio_lsp` sits below `sirio_ui` and cannot see `Language` at all.
    const SAMPLE_FILES: &[(sirio_ui::editor::Language, &str)] = &[
        (sirio_ui::editor::Language::Markdown, "README.md"),
        (sirio_ui::editor::Language::Rust, "main.rs"),
        (sirio_ui::editor::Language::Python, "app.py"),
        (sirio_ui::editor::Language::JavaScript, "index.js"),
        (sirio_ui::editor::Language::TypeScript, "index.ts"),
        (sirio_ui::editor::Language::Shell, "build.sh"),
        (sirio_ui::editor::Language::Json, "package.json"),
        (sirio_ui::editor::Language::Yaml, "ci.yml"),
        (sirio_ui::editor::Language::Toml, "Cargo.toml"),
        (sirio_ui::editor::Language::C, "main.c"),
        (sirio_ui::editor::Language::Cpp, "main.cpp"),
        (sirio_ui::editor::Language::Go, "main.go"),
        (sirio_ui::editor::Language::Swift, "App.swift"),
        (sirio_ui::editor::Language::Kotlin, "Main.kt"),
        (sirio_ui::editor::Language::Java, "Main.java"),
        (sirio_ui::editor::Language::Ruby, "app.rb"),
        (sirio_ui::editor::Language::Php, "index.php"),
        (sirio_ui::editor::Language::Html, "index.html"),
        (sirio_ui::editor::Language::Css, "site.css"),
        (sirio_ui::editor::Language::Sql, "schema.sql"),
        (sirio_ui::editor::Language::Xml, "pom.xml"),
        (sirio_ui::editor::Language::Lua, "init.lua"),
        (sirio_ui::editor::Language::Zig, "main.zig"),
    ];

    /// What "Java isn't there" actually was. The editor recognised
    /// `Main.java`, named it in the status line, and the shipped table had
    /// no entry for `.java` — so no server started, no capability was
    /// negotiated, and "Go to Definition" was never offered. Nothing
    /// reported a gap at any layer; the action simply was not in the menu.
    #[test]
    fn every_language_the_editor_recognises_has_a_server() {
        let table = sirio_lsp::LanguageTable::defaults();
        for (language, file) in SAMPLE_FILES {
            let path = std::path::Path::new(file);
            assert_eq!(
                sirio_ui::editor::Language::from_path(path),
                *language,
                "{file} is the sample for {}, so it has to resolve to it",
                language.name()
            );
            assert!(
                table.for_path(path).is_some(),
                "{} is offered in the editor with no server entry for {file}",
                language.name()
            );
        }
        for language in sirio_ui::editor::Language::ALL {
            if language.is_plain_text() {
                continue;
            }
            assert!(
                SAMPLE_FILES.iter().any(|(sample, _)| *sample == language),
                "{} was added to the editor and never checked here",
                language.name()
            );
        }
    }

    #[test]
    fn a_file_the_editor_cannot_colour_gets_no_server_either() {
        // The other direction. A `.txt` has no language and must not drag a
        // server in; every extension the table claims must be one the
        // editor recognises, or the two lists have drifted.
        let table = sirio_lsp::LanguageTable::defaults();
        assert!(table.for_path(std::path::Path::new("notes.txt")).is_none());
        for entry in table.entries() {
            for extension in &entry.extensions {
                let file = format!("sample.{extension}");
                let language = sirio_ui::editor::Language::from_path(std::path::Path::new(&file));
                assert!(
                    !language.is_plain_text(),
                    "`{}` claims .{extension}, which the editor renders as plain text",
                    entry.name
                );
            }
        }
    }

    #[test]
    fn capabilities_are_absent_while_no_server_runs() {
        // Nothing is running in a fresh supervisor, so every question about
        // a capability answers "no" — never "maybe".
        let supervisor = LspSupervisor::new(loader_with_defaults());
        let file = std::path::Path::new("/repo/src/main.rs");
        let root = std::path::Path::new("/repo");
        assert!(supervisor.capability_for(file, root).is_none());
        assert!(!supervisor.definition_available(file, root));
        assert!(!supervisor.references_available(file, root));
    }

    #[test]
    fn a_command_that_is_missing_and_installable_offers_its_recipe() {
        let mut supervisor = LspSupervisor::new(loader_with_defaults());
        let file = std::path::Path::new("/repo/src/main.rs");
        let root = std::path::Path::new("/repo");
        let entry = supervisor.entry_for(file).expect("rust is in the defaults");
        let key = supervisor.key_for(file, root, &entry);
        supervisor.mark_dead(
            key,
            Dead::Installable {
                recipe: entry.install.clone().expect("rust has a recipe"),
            },
        );
        assert!(matches!(
            supervisor.dead_reason_for(file, root),
            Some(Dead::Installable { .. })
        ));
    }

    #[test]
    fn a_language_whose_server_needs_a_toolchain_says_so_instead() {
        let mut supervisor = LspSupervisor::new(loader_with_defaults());
        let file = std::path::Path::new("/repo/src/Main.java");
        let root = std::path::Path::new("/repo");
        let entry = supervisor.entry_for(file).expect("java is in the defaults");
        let key = supervisor.key_for(file, root, &entry);
        let Some(sirio_lsp::Recipe::Manual { needs, url }) = entry.install.clone() else {
            panic!("java is a Manual recipe");
        };
        supervisor.mark_dead(key, Dead::Manual { needs, url });
        match supervisor.dead_reason_for(file, root) {
            Some(Dead::Manual { needs, .. }) => assert!(needs.contains("JVM")),
            other => panic!("expected the toolchain explanation, got {other:?}"),
        }
    }

    #[test]
    fn the_path_wins_over_what_sirio_installed() {
        // Zed's rule, and its reason: worktree 1 may pin its own gopls while
        // worktree 3 falls back to ours. Spawning by name *is* the PATH
        // lookup, so the store is consulted only after NotInstalled — which is
        // what makes this ordering free.
        let store = sirio_registry::InstallStore::new(std::env::temp_dir().join(
            format!("sirio-lsp-store-{}", std::process::id()),
        ));
        let supervisor = LspSupervisor::new(loader_with_defaults());
        // A command every machine has, standing in for one the reader installed.
        let entry = sirio_lsp::LanguageEntry {
            name: "probe".into(),
            extensions: vec!["probe".into()],
            command: "sh".into(),
            args: Vec::new(),
            roots: Vec::new(),
            install: Some(sirio_lsp::Recipe::Npm {
                package: "never-used",
                version: "1.0.0",
                bin: "never-used",
            }),
        };
        assert!(
            supervisor.installed_binary_for(&entry, &store).is_none(),
            "nothing is installed, so step 2 has nothing to offer"
        );
    }

    #[test]
    fn an_installed_server_is_offered_by_absolute_path() {
        // The other half of step 2, and the shape of it: what the store
        // gives back is one file's path, never a directory to put on the
        // child's PATH — which is what keeps two installed servers from
        // seeing each other, and from displacing the reader's own choice.
        let root = std::env::temp_dir().join(format!(
            "sirio-lsp-store-installed-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let store = sirio_registry::InstallStore::new(&root);
        let entry = sirio_lsp::LanguageEntry {
            name: "probe".into(),
            extensions: vec!["probe".into()],
            // Not on PATH: everything below is what step 2 answers *after*
            // that name was tried and missed.
            command: "sirio-no-such-language-server".into(),
            args: Vec::new(),
            roots: Vec::new(),
            install: Some(sirio_lsp::Recipe::Npm {
                package: "shared-package",
                version: "1.0.0",
                bin: "never-used",
            }),
        };
        let executable = root
            .join("shared-package")
            .join("1.0.0")
            .join("never-used");
        std::fs::create_dir_all(executable.parent().expect("a store version directory"))
            .expect("create the store layout");
        std::fs::write(&executable, "#!/bin/sh\n").expect("write the installed binary");
        store
            .write(&sirio_registry::InstalledAgent {
                id: "shared-package".into(),
                version: "1.0.0".into(),
                executable: executable.clone(),
                args: Vec::new(),
                integrity: sirio_registry::Integrity::None,
            })
            .expect("record the install");

        let supervisor = LspSupervisor::new(loader_with_defaults());
        assert_eq!(
            supervisor.installed_binary_for(&entry, &store),
            Some(executable.clone()),
            "a manifest whose executable is there is what step 2 offers"
        );

        std::fs::remove_file(&executable).expect("remove the installed binary");
        assert_eq!(
            supervisor.installed_binary_for(&entry, &store),
            None,
            "a manifest outliving its binary is not an offer"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_successful_install_revives_a_dead_key() {
        let mut supervisor = LspSupervisor::new(loader_with_defaults());
        let file = std::path::Path::new("/repo/src/main.rs");
        let root = std::path::Path::new("/repo");
        let entry = supervisor.entry_for(file).expect("rust is in the defaults");
        let key = supervisor.key_for(file, root, &entry);
        supervisor.mark_dead(
            key.clone(),
            Dead::Installable {
                recipe: entry.install.clone().unwrap(),
            },
        );
        assert!(supervisor.is_dead(&key));

        supervisor.revive(&key);

        assert!(
            !supervisor.is_dead(&key),
            "an install that worked lets the launch be tried again"
        );
    }

    #[test]
    fn installing_does_not_revive_a_server_that_crashed() {
        // Reinstalling is not the cure for a crash. A server that dies on a
        // file dies again on restart, and on rust-analyzer that means
        // re-indexing the repository forever.
        let mut supervisor = LspSupervisor::new(loader_with_defaults());
        let file = std::path::Path::new("/repo/src/main.rs");
        let root = std::path::Path::new("/repo");
        let entry = supervisor.entry_for(file).expect("rust is in the defaults");
        let key = supervisor.key_for(file, root, &entry);
        supervisor.mark_dead(key.clone(), Dead::Failed);

        supervisor.revive(&key);

        assert!(supervisor.is_dead(&key), "Failed is final");
    }

    #[test]
    fn a_file_no_entry_claims_has_no_dead_reason() {
        // Something *is* dead here, so `None` below is the answer to a
        // question rather than the emptiness of the map. A `.txt` has no
        // entry and so no key, and the reason a Java file gives must not
        // leak onto a file no server was ever named for.
        let mut supervisor = LspSupervisor::new(loader_with_defaults());
        let file = std::path::Path::new("/repo/src/Main.java");
        let root = std::path::Path::new("/repo");
        let entry = supervisor.entry_for(file).expect("java is in the defaults");
        let key = supervisor.key_for(file, root, &entry);
        supervisor.mark_dead(key, Dead::Failed);
        assert!(
            supervisor
                .dead_reason_for(
                    std::path::Path::new("/repo/NOTES.txt"),
                    std::path::Path::new("/repo")
                )
                .is_none()
        );
    }
}
