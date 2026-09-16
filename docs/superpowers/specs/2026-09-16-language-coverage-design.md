# Every language Sirio recognises, coloured and served — design

**Date:** 2026-09-16
**Status:** implemented
**Parent specs:** `docs/superpowers/specs/2026-09-14-lsp-client-design.md`
(the language table and its loader), F-EDIT-07 (the source editor and
`Language`)
**Scope of this document:** closing the gap between the languages Sirio
*recognises* and the two things it can do with them.

## The gap

`sirio_ui::editor::Language` has recognised twenty-three languages plus
plain text since F-EDIT-07. Two independent capabilities hang off that
recognition, and both covered a fraction of it:

| | covered | of |
|---|---|---|
| syntax colour | 8 | 23 |
| language server | 4 | 23 |

Opening `Main.java` detected Java, named Java in the status line, and then
rendered the file in one colour with no "Go to Definition" in its context
menu. Nothing anywhere reported a gap — which is the whole problem. A
language with no grammar produces an empty span list, and an empty span
list is what a file with nothing to classify produces too. A language with
no table entry starts no server, negotiates no capability, and the action
is simply absent from the menu, which is indistinguishable from a build
with no LSP support at all.

Both halves fail *silently and identically*, so neither could be found by
using the app — only by reading two lists side by side and noticing they
are different lengths.

## §1 Colour — `sirio_syntax`

`bezel-syntax` 0.1.4 carries seven grammars: Rust, Python,
TypeScript/TSX, JSON, Go, Bash, TOML. It also documents the way to add
one, and the design takes that path rather than forking bezel or moving
the `=0.1.4` pin, which CLAUDE.md treats as a visual change to review:

> A language the table does not carry is a `Lang::new` `static` of your
> own, highlighted through `Lang::highlight` — the same path the built-in
> rows take.

`sirio_syntax` is a new leaf crate holding fifteen such rows. Spans come
back as ordinary `bezel::theme::HighlightKind` values, so they reach
`Theme::syntax_palette` with no second code path and no second palette.

Three constraints shaped it:

1. **A grammar crate must reach tree-sitter through `tree-sitter-language`,
   never by naming a `tree-sitter` version of its own.** Two tree-sitters
   in the graph are two unrelated `Language` types with one name. This
   ruled out `tree-sitter-kotlin` (pins `>= 0.21, < 0.23`) in favour of
   `tree-sitter-kotlin-sg`.

2. **Capture names have to be translated.** Each grammar ships its own
   `highlights.scm` in its own dialect — `@delimiter`, `@field`,
   `@repeat`, `@text.title` — and bezel's `kind_of` answers
   `HighlightKind::Variable`, the plain text colour, to every name it does
   not know. A query full of unknown names still compiles, still parses
   and still produces spans; they are simply all the colour of ordinary
   text. `sirio_syntax::captures` rewrites them on the way in.

3. **`@spell` has to be deleted, not renamed.** Swift, SQL and Zig write
   it as a second capture on the node the same pattern captured as
   `@comment`, and `tree-sitter-highlight` resolves a node captured twice
   by taking the *last* capture. Shipping those queries unaltered paints
   every comment in those three languages as plain text — measured:
   `(0..6, Variable)` on Swift's `// doc`. Renaming it to a `_`-prefixed
   name (the class bezel drops) is not enough either: the later capture
   still displaces `@comment`, it just resolves to no highlight, so the
   comment ends up with no span rather than the wrong one. Only removing
   the capture leaves `@comment` standing.

The same last-capture-wins rule settles C++: its shipped query is a
supplement to C's (the `; inherits: c` convention, which the crates
strip), so the two are concatenated **C first**.

`@none` needs nothing done to it. It marks a node that should take no
colour — Kotlin's `${…}` inside a string, Markdown's fenced code — and the
unknown-name fallback already answers `Variable`, which is what it asks
for.

## §2 Servers — the shipped table

`LanguageTable::defaults` grows from four entries to twenty-two, one per
language, each naming the command its own project documents with the
arguments that put it on stdio.

| Language | Extensions | Command | Root markers |
|---|---|---|---|
| Rust | `rs` | `rust-analyzer` | `Cargo.toml` |
| TypeScript / JavaScript | `ts` `tsx` `js` `jsx` `mjs` `cjs` | `typescript-language-server --stdio` | `package.json`, `tsconfig.json` |
| Python | `py` `pyi` | `pyright-langserver --stdio` | `pyproject.toml`, `setup.py`, `requirements.txt` |
| Go | `go` | `gopls` | `go.mod` |
| C / C++ | `c` `h` `cc` `cpp` `cxx` `hpp` `hh` `hxx` | `clangd` | `compile_commands.json`, `compile_flags.txt`, `.clangd`, `CMakeLists.txt`, `Makefile` |
| Java | `java` | `jdtls` | `pom.xml`, `build.gradle`(`.kts`), `settings.gradle`(`.kts`), `.project` |
| Kotlin | `kt` `kts` | `kotlin-language-server` | `settings.gradle`(`.kts`), `build.gradle.kts`, `pom.xml` |
| Swift | `swift` | `sourcekit-lsp` | `Package.swift` |
| Ruby | `rb` | `ruby-lsp` | `Gemfile`, `.ruby-version` |
| PHP | `php` | `intelephense --stdio` | `composer.json` |
| Lua | `lua` | `lua-language-server` | `.luarc.json`, `.luarc.jsonc`, `stylua.toml` |
| Zig | `zig` | `zls` | `build.zig`, `build.zig.zon` |
| YAML | `yaml` `yml` | `yaml-language-server --stdio` | — |
| JSON | `json` `jsonc` | `vscode-json-language-server --stdio` | — |
| HTML | `html` `htm` | `vscode-html-language-server --stdio` | — |
| CSS | `css` | `vscode-css-language-server --stdio` | — |
| Shell | `sh` `bash` `zsh` | `bash-language-server start` | — |
| TOML | `toml` | `taplo lsp stdio` | — |
| Markdown | `md` `markdown` | `marksman server` | `.marksman.toml` |
| XML | `xml` | `lemminx` | — |
| SQL | `sql` | `sqls` | — |

C and C++ are one entry on purpose: clangd serves both, and separate
entries would run two of it over the same compilation database. `.fish` is
coloured as a shell script and has no server; no shell server speaks it.

Every one of these is still overridden by an entry for the same extension
in `languages.toml`, which is read first.

### §2.1 A missing binary stops being news

The parent spec's §5 said a binary that is not found gets *a visible
notice naming the command, once*. That was right when the table held four
servers: a missing one then meant a typo in the user's `languages.toml`.

With twenty-two, "not installed" becomes the ordinary state of most of the
table on any given machine — nobody has all of them — so the notice would
appear on nearly every file opened and would drown the case it exists for,
a server that *is* installed and broke. `LspError::NotInstalled` is
therefore its own variant, told apart at the `spawn` by
`io::ErrorKind::NotFound`, and the shell says nothing about it. The pair is
still marked dead, so the launch is attempted once per session, and the
variant carries the command for any caller that does want to name it.

This is the same distinction `LspError::NotReady` already draws: not every
non-answer is a fault.

## §3 The tests that keep the lists together

The failure this design corrects cannot be seen by using the app, so it
has to be a test, and the test has to be about *agreement between lists*
rather than about behaviour:

| Test | Crate | What drifting looks like without it |
|---|---|---|
| `every_language_the_editor_recognises_has_a_grammar` | `sirio_ui` | a language is added to `Language` and renders as plain text |
| `every_language_the_editor_recognises_has_a_server` | `sirio` | a language is added and its context menu quietly loses an action |
| `a_file_the_editor_cannot_colour_gets_no_server_either` | `sirio` | an extension gets a server and no colour |
| `every_query_compiles_against_its_grammar` | `sirio_syntax` | a grammar bump breaks a query and the language goes back to one colour |
| `no_two_default_entries_claim_the_same_extension` | `sirio_lsp` | a second entry is shadowed and never runs |

The cross-crate pair lives in `sirio` because `sirio_lsp` sits below
`sirio_ui` and cannot see `Language` at all. The first run of
`a_file_the_editor_cannot_colour_gets_no_server_either` found a drift that
predated this work: `.pyi` had carried a Python server since the table was
written, and `Language::from_path` did not know the extension, so Python
stub files got completions and no colour.
