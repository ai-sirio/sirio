//! The table of language servers: what exists, and which file wants which.
//!
//! Pure by construction — it parses text and answers questions about it.
//! Nothing here reads a file, resolves a path against the real filesystem,
//! or starts a process; those belong to [`crate::loader`] and to `sirio`'s
//! supervisor respectively. Keeping this half pure is what lets the whole
//! matching story be table-driven tests with no temp directories.

use std::path::Path;

use serde::Deserialize;

use crate::{Asset, Recipe};

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
    /// How to get `command`, when Sirio can get it. `#[serde(skip)]` is the
    /// decision, not an oversight: which npm package is the right one is
    /// Sirio's knowledge, not the reader's to configure — and an entry
    /// written in `languages.toml` therefore carries `None`, which is the
    /// right answer. If you point Sirio at your own jdtls it must not offer
    /// to download another one underneath it.
    #[serde(skip)]
    pub install: Option<crate::Recipe>,
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
    /// twenty-one of these. A command that is not on `PATH` resolves to
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
            install: Option<crate::Recipe>,
        ) -> LanguageEntry {
            LanguageEntry {
                name: name.to_owned(),
                extensions: extensions.iter().map(|value| (*value).to_owned()).collect(),
                command: command.to_owned(),
                args: args.iter().map(|value| (*value).to_owned()).collect(),
                roots: roots.iter().map(|value| (*value).to_owned()).collect(),
                install,
            }
        }

        Self {
            entries: vec![
                entry(
                    "rust",
                    &["rs"],
                    "rust-analyzer",
                    &[],
                    &["Cargo.toml"],
                    Some(
                        // rust-analyzer 2026-09-14
                        Recipe::Release {
                            id: "rust-analyzer",
                            version: "2026-09-14",
                            bin: "rust-analyzer",
                            assets: &[
                                (
                                    "linux-x86_64",
                                    Asset {
                                        url: "https://github.com/rust-lang/rust-analyzer/releases/download/2026-09-14/rust-analyzer-x86_64-unknown-linux-gnu.gz",
                                        sha256: "7609ba53f85cd80a3bde77a4b2e94e304d0f94650e4f4cffc061b9bca454ba75",
                                        bytes: 14853937,
                                    },
                                ),
                                (
                                    "linux-aarch64",
                                    Asset {
                                        url: "https://github.com/rust-lang/rust-analyzer/releases/download/2026-09-14/rust-analyzer-aarch64-unknown-linux-gnu.gz",
                                        sha256: "3d32c50aebf9288c2fd11b559813441bdff2aa57fbbb7177ad0ffe5ac4e9ad3d",
                                        bytes: 14327723,
                                    },
                                ),
                                (
                                    "darwin-aarch64",
                                    Asset {
                                        url: "https://github.com/rust-lang/rust-analyzer/releases/download/2026-09-14/rust-analyzer-aarch64-apple-darwin.gz",
                                        sha256: "0c579403271f4021eb1efdfaa9bedb43e099595d02a02ee9b1f34c6c51a3ac26",
                                        bytes: 13877061,
                                    },
                                ),
                                (
                                    "darwin-x86_64",
                                    Asset {
                                        url: "https://github.com/rust-lang/rust-analyzer/releases/download/2026-09-14/rust-analyzer-x86_64-apple-darwin.gz",
                                        sha256: "58d827adc7bde3b8986ff52795484f462564a2beed3ba4f4b2cbc3cfd58ae05e",
                                        bytes: 14637240,
                                    },
                                ),
                                (
                                    "windows-x86_64",
                                    Asset {
                                        url: "https://github.com/rust-lang/rust-analyzer/releases/download/2026-09-14/rust-analyzer-x86_64-pc-windows-msvc.zip",
                                        sha256: "631ea40942cbc1e70a3465218f27f49fd73d82d9c0dd21e5279d8417f7f2dd93",
                                        bytes: 17515427,
                                    },
                                ),
                            ],
                        },
                    ),
                ),
                entry(
                    "typescript",
                    &["ts", "tsx", "js", "jsx", "mjs", "cjs"],
                    "typescript-language-server",
                    &["--stdio"],
                    &["package.json", "tsconfig.json"],
                    Some(Recipe::Npm {
                        package: "typescript-language-server",
                        version: "6.0.0",
                        bin: "node_modules/.bin/typescript-language-server",
                    }),
                ),
                entry(
                    "python",
                    &["py", "pyi"],
                    "pyright-langserver",
                    &["--stdio"],
                    &["pyproject.toml", "setup.py", "requirements.txt"],
                    Some(Recipe::Npm {
                        package: "pyright",
                        version: "1.1.414",
                        bin: "node_modules/.bin/pyright-langserver",
                    }),
                ),
                entry(
                    "go",
                    &["go"],
                    "gopls",
                    &[],
                    &["go.mod"],
                    Some(Recipe::Manual {
                        needs: "Go, then `go install golang.org/x/tools/gopls@latest`",
                        url: "https://github.com/golang/tools/tree/master/gopls#installation",
                    }),
                ),
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
                    Some(
                        // clangd 22.1.6
                        Recipe::Release {
                            id: "clangd",
                            version: "22.1.6",
                            bin: "clangd_22.1.6/bin/clangd",
                            assets: &[
                                (
                                    "linux-x86_64",
                                    Asset {
                                        url: "https://github.com/clangd/clangd/releases/download/22.1.6/clangd-linux-22.1.6.zip",
                                        sha256: "a9c77443af2e447ed467e84771848d3a6ac1c56f84bcfcde717e66318de77cfa",
                                        bytes: 114790601,
                                    },
                                ),
                                (
                                    "darwin-aarch64",
                                    Asset {
                                        url: "https://github.com/clangd/clangd/releases/download/22.1.6/clangd-mac-22.1.6.zip",
                                        sha256: "631aef462556cbd74e0ebaae1778a38d1997d0ba3371652ca54f82652a179e7d",
                                        bytes: 98113276,
                                    },
                                ),
                                (
                                    "windows-x86_64",
                                    Asset {
                                        url: "https://github.com/clangd/clangd/releases/download/22.1.6/clangd-windows-22.1.6.zip",
                                        sha256: "ce54f16e0b4fd76d450eeda9664420b195360b73febcfe40e661108fa57f2ce1",
                                        bytes: 28198778,
                                    },
                                ),
                            ],
                        },
                    ),
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
                    Some(Recipe::Manual {
                        needs: "a JVM (Java 21 or newer)",
                        url: "https://github.com/eclipse-jdtls/eclipse.jdt.ls#installation",
                    }),
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
                    Some(Recipe::Manual {
                        needs: "a JVM (Java 17 or newer)",
                        url: "https://github.com/fwcd/kotlin-language-server#installation",
                    }),
                ),
                entry(
                    "swift",
                    &["swift"],
                    "sourcekit-lsp",
                    &[],
                    &["Package.swift"],
                    Some(Recipe::Manual {
                        needs: "a Swift toolchain, which carries sourcekit-lsp",
                        url: "https://github.com/swiftlang/sourcekit-lsp#installation",
                    }),
                ),
                entry(
                    "ruby",
                    &["rb"],
                    "ruby-lsp",
                    &[],
                    &["Gemfile", ".ruby-version"],
                    Some(Recipe::Manual {
                        needs: "Ruby, then `gem install ruby-lsp`",
                        url: "https://shopify.github.io/ruby-lsp/",
                    }),
                ),
                entry(
                    "php",
                    &["php"],
                    "intelephense",
                    &["--stdio"],
                    &["composer.json"],
                    Some(Recipe::Npm {
                        package: "intelephense",
                        version: "1.18.5",
                        bin: "node_modules/.bin/intelephense",
                    }),
                ),
                entry(
                    "lua",
                    &["lua"],
                    "lua-language-server",
                    &[],
                    &[".luarc.json", ".luarc.jsonc", "stylua.toml"],
                    Some(
                        // lua-language-server 3.19.1
                        Recipe::Release {
                            id: "lua-language-server",
                            version: "3.19.1",
                            bin: "bin/lua-language-server",
                            assets: &[
                                (
                                    "linux-x86_64",
                                    Asset {
                                        url: "https://github.com/LuaLS/lua-language-server/releases/download/3.19.1/lua-language-server-3.19.1-linux-x64.tar.gz",
                                        sha256: "e9235d2d72ef55bc41cf8c99cda2ed64777682024b4bb81f5dea425060c5cbb8",
                                        bytes: 3677772,
                                    },
                                ),
                                (
                                    "linux-aarch64",
                                    Asset {
                                        url: "https://github.com/LuaLS/lua-language-server/releases/download/3.19.1/lua-language-server-3.19.1-linux-arm64.tar.gz",
                                        sha256: "abd2572e8fc929dc838a81ffb8473c5bce0bf39bfe8edb4b120b3b623176ce83",
                                        bytes: 2613202,
                                    },
                                ),
                                (
                                    "darwin-aarch64",
                                    Asset {
                                        url: "https://github.com/LuaLS/lua-language-server/releases/download/3.19.1/lua-language-server-3.19.1-darwin-arm64.tar.gz",
                                        sha256: "0bc077f4447f076b4c92c14e9fd303f5b569eda2ec74b4dca2b55f75fae2e90c",
                                        bytes: 3284464,
                                    },
                                ),
                                (
                                    "windows-x86_64",
                                    Asset {
                                        url: "https://github.com/LuaLS/lua-language-server/releases/download/3.19.1/lua-language-server-3.19.1-win32-x64.zip",
                                        sha256: "fdb9a59108cf62517813c97fa5549b0e16d1ef0688306bac728b08434db7e4cd",
                                        bytes: 4453980,
                                    },
                                ),
                            ],
                        },
                    ),
                ),
                entry(
                    "zig",
                    &["zig"],
                    "zls",
                    &[],
                    &["build.zig", "build.zig.zon"],
                    Some(Recipe::Manual {
                        needs: "zls, which publishes only .tar.xz archives Sirio cannot yet unpack",
                        url: "https://github.com/zigtools/zls#installation",
                    }),
                ),
                entry(
                    "yaml",
                    &["yaml", "yml"],
                    "yaml-language-server",
                    &["--stdio"],
                    &[],
                    Some(Recipe::Npm {
                        package: "yaml-language-server",
                        version: "1.24.0",
                        bin: "node_modules/.bin/yaml-language-server",
                    }),
                ),
                // The three servers VS Code's own web tooling is published
                // as, and the only maintained ones for these languages.
                entry(
                    "json",
                    &["json", "jsonc"],
                    "vscode-json-language-server",
                    &["--stdio"],
                    &[],
                    Some(Recipe::Npm {
                        package: "vscode-langservers-extracted",
                        version: "4.10.0",
                        bin: "node_modules/.bin/vscode-json-language-server",
                    }),
                ),
                entry(
                    "html",
                    &["html", "htm"],
                    "vscode-html-language-server",
                    &["--stdio"],
                    &[],
                    Some(Recipe::Npm {
                        package: "vscode-langservers-extracted",
                        version: "4.10.0",
                        bin: "node_modules/.bin/vscode-html-language-server",
                    }),
                ),
                entry(
                    "css",
                    &["css"],
                    "vscode-css-language-server",
                    &["--stdio"],
                    &[],
                    Some(Recipe::Npm {
                        package: "vscode-langservers-extracted",
                        version: "4.10.0",
                        bin: "node_modules/.bin/vscode-css-language-server",
                    }),
                ),
                // `start` is the subcommand; without it the binary prints
                // usage and exits, which reads as a server that died.
                entry(
                    "bash",
                    &["sh", "bash", "zsh"],
                    "bash-language-server",
                    &["start"],
                    &[],
                    Some(Recipe::Npm {
                        package: "bash-language-server",
                        version: "5.7.1",
                        bin: "node_modules/.bin/bash-language-server",
                    }),
                ),
                entry(
                    "toml",
                    &["toml"],
                    "taplo",
                    &["lsp", "stdio"],
                    &[],
                    Some(
                        // taplo 0.10.0
                        Recipe::Release {
                            id: "taplo",
                            version: "0.10.0",
                            bin: "taplo",
                            assets: &[
                                (
                                    "linux-x86_64",
                                    Asset {
                                        url: "https://github.com/tamasfe/taplo/releases/download/0.10.0/taplo-linux-x86_64.gz",
                                        sha256: "8fe196b894ccf9072f98d4e1013a180306e17d244830b03986ee5e8eabeb6156",
                                        bytes: 5116068,
                                    },
                                ),
                                (
                                    "linux-aarch64",
                                    Asset {
                                        url: "https://github.com/tamasfe/taplo/releases/download/0.10.0/taplo-linux-aarch64.gz",
                                        sha256: "033681d01eec8376c3fd38fa3703c79316f5e14bb013d859943b60a07bccdcc3",
                                        bytes: 4631779,
                                    },
                                ),
                                (
                                    "darwin-aarch64",
                                    Asset {
                                        url: "https://github.com/tamasfe/taplo/releases/download/0.10.0/taplo-darwin-aarch64.gz",
                                        sha256: "713734314c3e71894b9e77513c5349835eefbd52908445a0d73b0c7dc469347d",
                                        bytes: 4616415,
                                    },
                                ),
                                (
                                    "windows-x86_64",
                                    Asset {
                                        url: "https://github.com/tamasfe/taplo/releases/download/0.10.0/taplo-windows-x86_64.zip",
                                        sha256: "1615eed140039bd58e7089109883b1c434de5d6de8f64a993e6e8c80ca57bdf9",
                                        bytes: 5182591,
                                    },
                                ),
                            ],
                        },
                    ),
                ),
                entry(
                    "markdown",
                    &["md", "markdown"],
                    "marksman",
                    &["server"],
                    &[".marksman.toml"],
                    Some(
                        // marksman 2026-02-08
                        Recipe::Release {
                            id: "marksman",
                            version: "2026-02-08",
                            bin: "marksman",
                            assets: &[
                                (
                                    "linux-x86_64",
                                    Asset {
                                        url: "https://github.com/artempyanykh/marksman/releases/download/2026-02-08/marksman-linux-x64",
                                        sha256: "be5098e8213219269c47fc0d916a66fa31ce0602ec967475c722260aabf26087",
                                        bytes: 22500875,
                                    },
                                ),
                                (
                                    "linux-aarch64",
                                    Asset {
                                        url: "https://github.com/artempyanykh/marksman/releases/download/2026-02-08/marksman-linux-arm64",
                                        sha256: "db8e124527f7f8048e3e6c91821b9c52ef173d92c01e47d221bf1337afd962fb",
                                        bytes: 21851058,
                                    },
                                ),
                                (
                                    "darwin-aarch64",
                                    Asset {
                                        url: "https://github.com/artempyanykh/marksman/releases/download/2026-02-08/marksman-macos",
                                        sha256: "6a801c17b5ac0dba69787c5282b3b3bd416e66c96253fae098d311c6bbd1833b",
                                        bytes: 43856208,
                                    },
                                ),
                                (
                                    "windows-x86_64",
                                    Asset {
                                        url: "https://github.com/artempyanykh/marksman/releases/download/2026-02-08/marksman.exe",
                                        sha256: "a6d05beb08ebe41b0a9f09c98a438540421436fa5531424c22e0bb1d22529705",
                                        bytes: 20502938,
                                    },
                                ),
                            ],
                        },
                    ),
                ),
                entry(
                    "xml",
                    &["xml"],
                    "lemminx",
                    &[],
                    &[],
                    Some(
                        // lemminx 0.29.3
                        // Each zip holds one platform-named file (lemminx-linux-x86_64, …),
                        // not `lemminx`: no single `bin` fits all three (see the task report).
                        Recipe::Release {
                            id: "lemminx",
                            version: "0.29.3",
                            bin: "lemminx",
                            assets: &[
                                (
                                    "linux-x86_64",
                                    Asset {
                                        url: "https://github.com/redhat-developer/vscode-xml/releases/download/0.29.3/lemminx-linux-x86_64.zip",
                                        sha256: "1acc44e24201c1d2f5ccb4e43e7426ed0df6909207a81ff199810e2808104d89",
                                        bytes: 16903991,
                                    },
                                ),
                                (
                                    "darwin-aarch64",
                                    Asset {
                                        url: "https://github.com/redhat-developer/vscode-xml/releases/download/0.29.3/lemminx-osx-aarch_64.zip",
                                        sha256: "185db5630ce85be43ea0fab034e7841b1327c2793db05ab481e029cf493d86ce",
                                        bytes: 16271050,
                                    },
                                ),
                                (
                                    "windows-x86_64",
                                    Asset {
                                        url: "https://github.com/redhat-developer/vscode-xml/releases/download/0.29.3/lemminx-win32.zip",
                                        sha256: "7eaefaac68253b0ec8e0ad1f1c0f2d0755423d4e99e52497428b52f80df28eb7",
                                        bytes: 16578930,
                                    },
                                ),
                            ],
                        },
                    ),
                ),
                entry(
                    "sql",
                    &["sql"],
                    "sqls",
                    &[],
                    &[],
                    Some(
                        // sqls v0.2.48
                        Recipe::Release {
                            id: "sqls",
                            version: "v0.2.48",
                            bin: "sqls",
                            assets: &[
                                (
                                    "linux-x86_64",
                                    Asset {
                                        url: "https://github.com/sqls-server/sqls/releases/download/v0.2.48/sqls-linux-0.2.48.zip",
                                        sha256: "30047b92c41658c821b7803d2c2a3a1ce4e17ee769ceff6f24bb9e3daaf5d4dc",
                                        bytes: 10736904,
                                    },
                                ),
                                (
                                    "darwin-aarch64",
                                    Asset {
                                        url: "https://github.com/sqls-server/sqls/releases/download/v0.2.48/sqls-darwin-0.2.48.zip",
                                        sha256: "b44165ca597a4b4298d56657bc911aa3ca8a591befefde4e29566923c6229f3d",
                                        bytes: 10731807,
                                    },
                                ),
                                (
                                    "windows-x86_64",
                                    Asset {
                                        url: "https://github.com/sqls-server/sqls/releases/download/v0.2.48/sqls-windows-0.2.48.zip",
                                        sha256: "df6453b2ddcb4e748547d0288b826251a24af099749dc7a9ddea587aac3d4365",
                                        bytes: 10743962,
                                    },
                                ),
                            ],
                        },
                    ),
                ),
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
    fn the_shipped_table_names_twenty_one_servers() {
        // Four comments in this repository claimed nineteen or twenty-two on
        // 2026-09-16, none of them right. Prose cannot be diffed; this can.
        // Twenty-three is the count of *languages*: clangd covers two of them
        // and typescript six.
        assert_eq!(LanguageTable::defaults().entries().len(), 21);
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
        assert_eq!(
            table.entries().len(),
            LanguageTable::defaults().entries().len()
        );
    }

    #[test]
    fn every_default_entry_says_how_to_get_its_server() {
        // The failure this forbids is silence. A row added without an answer
        // reaches the reader as a disabled menu entry and no way forward,
        // which is exactly the 0.18.0 bug wearing a different coat.
        for entry in LanguageTable::defaults().entries() {
            assert!(
                entry.install.is_some(),
                "`{}` names `{}` and says nothing about how to get it",
                entry.name,
                entry.command
            );
        }
    }

    #[test]
    fn a_recipe_that_cannot_install_says_what_is_needed_first() {
        for entry in LanguageTable::defaults().entries() {
            if let Some(crate::Recipe::Manual { needs, url }) = &entry.install {
                assert!(!needs.is_empty(), "`{}` gives no reason", entry.name);
                assert!(
                    url.starts_with("https://"),
                    "`{}` gives no place to go",
                    entry.name
                );
            }
        }
    }

    #[test]
    fn three_languages_share_one_npm_package() {
        // json, html and css all come from vscode-langservers-extracted. The
        // store is keyed on the package, so they must resolve to one id or the
        // same download happens three times.
        let table = LanguageTable::defaults();
        let ids: Vec<Option<&str>> = ["json", "html", "css"]
            .iter()
            .map(|name| {
                table
                    .entries()
                    .iter()
                    .find(|entry| entry.name == *name)
                    .expect("the entry exists")
                    .install
                    .as_ref()
                    .and_then(crate::Recipe::store_id)
            })
            .collect();
        assert_eq!(
            ids,
            vec![
                Some("vscode-langservers-extracted"),
                Some("vscode-langservers-extracted"),
                Some("vscode-langservers-extracted"),
            ]
        );
    }

    #[test]
    fn a_user_entry_carries_no_recipe() {
        // `#[serde(skip)]` holds: a table written by hand cannot name a
        // download, and an override of a language Sirio knows loses the recipe
        // along with the command it replaces.
        let table = LanguageTable::parse(
            "[[language]]\nname = \"java\"\nextensions = [\"java\"]\ncommand = \"/opt/my/jdtls\"\n",
        )
        .expect("a hand-written table parses");
        assert!(table.entries()[0].install.is_none());
    }
}
