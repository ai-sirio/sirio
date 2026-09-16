//! The table of language servers: what exists, and which file wants which.
//!
//! Pure by construction — it parses text and answers questions about it.
//! Nothing here reads a file, resolves a path against the real filesystem,
//! or starts a process; those belong to [`crate::loader`] and to `sirio`'s
//! supervisor respectively. Keeping this half pure is what lets the whole
//! matching story be table-driven tests with no temp directories.

use std::path::Path;

use serde::Deserialize;

/// One language's server, as the user writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LanguageEntry {
    pub name: String,
    pub extensions: Vec<String>,
    pub command: String,
    /// Absent means none. rust-analyzer takes no arguments; most others do.
    #[serde(default)]
    pub args: Vec<String>,
    /// Marker filenames that identify a project root, searched upward from
    /// the file. Absent means none, which roots the server at the worktree.
    #[serde(default)]
    pub roots: Vec<String>,
}

/// The whole table, in the order entries were written. Order is meaningful:
/// the first entry claiming an extension wins.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LanguageTable {
    #[serde(default, rename = "language")]
    entries: Vec<LanguageEntry>,
}

/// A table that would not parse, with enough detail to point the user at it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// 1-based, when the parser knew one.
    pub line: Option<usize>,
    pub message: String,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl LanguageTable {
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        toml::from_str(text).map_err(|error| ConfigError {
            // `toml`'s span is a byte range; the line is what a human can act
            // on, so convert here rather than making every caller do it.
            line: error
                .span()
                .map(|span| text[..span.start].lines().count().max(1)),
            message: error.message().to_owned(),
        })
    }

    /// The compiled-in table, used when no file exists and when one exists
    /// but will not parse.
    ///
    /// One entry per language `sirio_ui::editor::Language` recognises, so
    /// the set of files Sirio colours and the set it can answer "go to
    /// definition" for are the same set. They were not: the table held four
    /// languages while the editor knew twenty-four, and the twenty
    /// in between had no way to say so — the context menu simply did not
    /// offer the action, which reads as "this build has no LSP".
    ///
    /// Naming a server is not promising one is installed. Nobody has all
    /// nineteen of these. A command that is not on `PATH` resolves to
    /// [`crate::LspError::NotInstalled`], which the shell deliberately says
    /// nothing about; the entry costs nothing until the day the user
    /// installs `jdtls`, and then it costs nothing to discover either.
    ///
    /// Each command is the one its own project documents, with the
    /// arguments that put it on stdio. Anything here can be overridden by
    /// adding an entry for the same extension to `languages.toml`, which is
    /// consulted first.
    pub fn defaults() -> Self {
        fn entry(
            name: &str,
            extensions: &[&str],
            command: &str,
            args: &[&str],
            roots: &[&str],
        ) -> LanguageEntry {
            LanguageEntry {
                name: name.to_owned(),
                extensions: extensions.iter().map(|value| (*value).to_owned()).collect(),
                command: command.to_owned(),
                args: args.iter().map(|value| (*value).to_owned()).collect(),
                roots: roots.iter().map(|value| (*value).to_owned()).collect(),
            }
        }

        Self {
            entries: vec![
                entry("rust", &["rs"], "rust-analyzer", &[], &["Cargo.toml"]),
                entry(
                    "typescript",
                    &["ts", "tsx", "js", "jsx", "mjs", "cjs"],
                    "typescript-language-server",
                    &["--stdio"],
                    &["package.json", "tsconfig.json"],
                ),
                entry(
                    "python",
                    &["py", "pyi"],
                    "pyright-langserver",
                    &["--stdio"],
                    &["pyproject.toml", "setup.py", "requirements.txt"],
                ),
                entry("go", &["go"], "gopls", &[], &["go.mod"]),
                // clangd is one server for both languages, and keying it as
                // one entry is what keeps a project from running two of
                // them over the same compilation database.
                entry(
                    "c",
                    &["c", "h", "cc", "cpp", "cxx", "hpp", "hh", "hxx"],
                    "clangd",
                    &[],
                    &[
                        "compile_commands.json",
                        "compile_flags.txt",
                        ".clangd",
                        "CMakeLists.txt",
                        "Makefile",
                    ],
                ),
                entry(
                    "java",
                    &["java"],
                    "jdtls",
                    &[],
                    &[
                        "pom.xml",
                        "build.gradle",
                        "build.gradle.kts",
                        "settings.gradle",
                        "settings.gradle.kts",
                        ".project",
                    ],
                ),
                entry(
                    "kotlin",
                    &["kt", "kts"],
                    "kotlin-language-server",
                    &[],
                    &[
                        "settings.gradle.kts",
                        "settings.gradle",
                        "build.gradle.kts",
                        "pom.xml",
                    ],
                ),
                entry("swift", &["swift"], "sourcekit-lsp", &[], &["Package.swift"]),
                entry(
                    "ruby",
                    &["rb"],
                    "ruby-lsp",
                    &[],
                    &["Gemfile", ".ruby-version"],
                ),
                entry(
                    "php",
                    &["php"],
                    "intelephense",
                    &["--stdio"],
                    &["composer.json"],
                ),
                entry(
                    "lua",
                    &["lua"],
                    "lua-language-server",
                    &[],
                    &[".luarc.json", ".luarc.jsonc", "stylua.toml"],
                ),
                entry(
                    "zig",
                    &["zig"],
                    "zls",
                    &[],
                    &["build.zig", "build.zig.zon"],
                ),
                entry(
                    "yaml",
                    &["yaml", "yml"],
                    "yaml-language-server",
                    &["--stdio"],
                    &[],
                ),
                // The three servers VS Code's own web tooling is published
                // as, and the only maintained ones for these languages.
                entry(
                    "json",
                    &["json", "jsonc"],
                    "vscode-json-language-server",
                    &["--stdio"],
                    &[],
                ),
                entry(
                    "html",
                    &["html", "htm"],
                    "vscode-html-language-server",
                    &["--stdio"],
                    &[],
                ),
                entry(
                    "css",
                    &["css"],
                    "vscode-css-language-server",
                    &["--stdio"],
                    &[],
                ),
                // `start` is the subcommand; without it the binary prints
                // usage and exits, which reads as a server that died.
                entry(
                    "bash",
                    &["sh", "bash", "zsh"],
                    "bash-language-server",
                    &["start"],
                    &[],
                ),
                entry("toml", &["toml"], "taplo", &["lsp", "stdio"], &[]),
                entry(
                    "markdown",
                    &["md", "markdown"],
                    "marksman",
                    &["server"],
                    &[".marksman.toml"],
                ),
                entry("xml", &["xml"], "lemminx", &[], &[]),
                entry("sql", &["sql"], "sqls", &[], &[]),
            ],
        }
    }

    /// The user's entries first, then the defaults. First match wins, so a
    /// user entry for an extension a default also claims shadows it — which
    /// is what makes the defaults overridable by addition rather than by
    /// finding and editing them.
    pub fn with_defaults_appended(mut self) -> Self {
        self.entries.extend(Self::defaults().entries);
        self
    }

    pub fn entries(&self) -> &[LanguageEntry] {
        &self.entries
    }

    pub fn for_extension(&self, extension: &str) -> Option<&LanguageEntry> {
        self.entries.iter().find(|entry| {
            entry
                .extensions
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        })
    }

    pub fn for_path(&self, path: &Path) -> Option<&LanguageEntry> {
        let extension = path.extension()?.to_str()?;
        self.for_extension(extension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_ENTRIES: &str = r#"
[[language]]
name = "rust"
extensions = ["rs"]
command = "rust-analyzer"
roots = ["Cargo.toml"]

[[language]]
name = "go"
extensions = ["go"]
command = "gopls"
args = ["-remote=auto"]
roots = ["go.mod"]
"#;

    #[test]
    fn a_table_parses_into_its_entries() {
        let table = LanguageTable::parse(TWO_ENTRIES).expect("valid table");
        assert_eq!(table.entries().len(), 2);
        let go = table.for_extension("go").expect("go entry");
        assert_eq!(go.command, "gopls");
        assert_eq!(go.args, vec!["-remote=auto".to_owned()]);
        assert_eq!(go.roots, vec!["go.mod".to_owned()]);
    }

    #[test]
    fn args_and_roots_default_to_empty_rather_than_failing() {
        // rust-analyzer needs no arguments, and a language with no marker is
        // legitimate — it simply roots at the worktree.
        let table = LanguageTable::parse(TWO_ENTRIES).expect("valid table");
        let rust = table.for_extension("rs").expect("rust entry");
        assert!(rust.args.is_empty());
    }

    #[test]
    fn an_extension_is_matched_case_insensitively() {
        // `.RS` on a case-insensitive filesystem is the same language.
        let table = LanguageTable::parse(TWO_ENTRIES).expect("valid table");
        assert!(table.for_extension("RS").is_some());
    }

    #[test]
    fn an_unknown_extension_is_none_and_not_an_error() {
        // A .txt file has no language server. That is normal, not a failure.
        let table = LanguageTable::parse(TWO_ENTRIES).expect("valid table");
        assert!(table.for_extension("txt").is_none());
    }

    #[test]
    fn for_path_reads_the_extension_off_a_path() {
        let table = LanguageTable::parse(TWO_ENTRIES).expect("valid table");
        assert_eq!(
            table.for_path(std::path::Path::new("/tmp/x/main.rs")).map(|e| e.name.as_str()),
            Some("rust")
        );
        assert!(table.for_path(std::path::Path::new("/tmp/x/README")).is_none());
    }

    /// The table and `sirio_ui::editor::Language` have to agree on which
    /// files Sirio understands. This crate cannot see that enum — it sits
    /// below it — so the pairing is checked from `sirio`, in
    /// `lsp::tests::every_language_the_editor_recognises_has_a_server`.
    /// What is checked here is the table's own content.
    #[test]
    fn the_shipped_defaults_name_a_server_for_every_language_sirio_opens() {
        let defaults = LanguageTable::defaults();
        for (extension, command) in [
            ("rs", "rust-analyzer"),
            ("ts", "typescript-language-server"),
            ("py", "pyright-langserver"),
            ("go", "gopls"),
            ("c", "clangd"),
            ("cpp", "clangd"),
            ("java", "jdtls"),
            ("kt", "kotlin-language-server"),
            ("swift", "sourcekit-lsp"),
            ("rb", "ruby-lsp"),
            ("php", "intelephense"),
            ("lua", "lua-language-server"),
            ("zig", "zls"),
            ("yml", "yaml-language-server"),
            ("json", "vscode-json-language-server"),
            ("html", "vscode-html-language-server"),
            ("css", "vscode-css-language-server"),
            ("sh", "bash-language-server"),
            ("toml", "taplo"),
            ("md", "marksman"),
            ("xml", "lemminx"),
            ("sql", "sqls"),
        ] {
            let entry = defaults
                .for_extension(extension)
                .unwrap_or_else(|| panic!("no default entry for .{extension}"));
            assert_eq!(entry.command, command, "for .{extension}");
        }
    }

    #[test]
    fn no_two_default_entries_claim_the_same_extension() {
        // `for_extension` takes the first match, so a duplicate would
        // silently shadow — and C and C++ sharing one clangd entry is
        // exactly the shape that makes this easy to get wrong.
        let defaults = LanguageTable::defaults();
        let mut seen: Vec<(&str, &str)> = Vec::new();
        for entry in defaults.entries() {
            for extension in &entry.extensions {
                if let Some((_, owner)) = seen
                    .iter()
                    .find(|(candidate, _)| candidate.eq_ignore_ascii_case(extension))
                {
                    panic!(
                        ".{extension} is claimed by both `{owner}` and `{}`",
                        entry.name
                    );
                }
                seen.push((extension, &entry.name));
            }
        }
    }

    #[test]
    fn every_default_entry_has_a_distinct_name() {
        // The name is half the key a running server is stored under, so two
        // entries sharing one would share a server.
        let defaults = LanguageTable::defaults();
        let mut names: Vec<&str> = defaults
            .entries()
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate language name in the defaults");
    }

    #[test]
    fn a_user_entry_wins_over_a_shipped_default_for_the_same_extension() {
        // Precedence is what makes the defaults overridable by ADDING an
        // entry, without having to know the default exists or where it sits.
        let user = LanguageTable::parse(
            r#"
[[language]]
name = "rust"
extensions = ["rs"]
command = "my-rust-analyzer"
roots = ["Cargo.toml"]
"#,
        )
        .expect("valid table");
        let merged = user.with_defaults_appended();
        assert_eq!(merged.for_extension("rs").expect("rust entry").command, "my-rust-analyzer");
        // and the untouched defaults still resolve
        assert_eq!(merged.for_extension("go").expect("go entry").command, "gopls");
    }

    #[test]
    fn malformed_toml_reports_the_line_so_the_user_can_find_it() {
        let error = LanguageTable::parse("[[language]\nname = \"rust\"").unwrap_err();
        assert!(
            error.line.is_some(),
            "a parse error must carry a line number, or the notice cannot point anywhere: {error:?}"
        );
    }

    #[test]
    fn an_entry_missing_a_required_field_is_a_parse_error() {
        // `command` has no sensible default: a language entry that names no
        // program is not a usable entry.
        let error = LanguageTable::parse(
            r#"
[[language]]
name = "rust"
extensions = ["rs"]
"#,
        )
        .unwrap_err();
        assert!(error.message.contains("command"), "the error must name the missing field: {error:?}");
    }

    #[test]
    fn an_empty_table_is_valid_and_matches_nothing() {
        let table = LanguageTable::parse("").expect("an empty file is a valid empty table");
        assert!(table.entries().is_empty());
        assert!(table.for_extension("rs").is_none());
    }

    #[test]
    fn defaults_appended_to_an_empty_table_are_just_the_defaults() {
        let table = LanguageTable::parse("").unwrap().with_defaults_appended();
        assert_eq!(table.entries().len(), LanguageTable::defaults().entries().len());
    }
}
