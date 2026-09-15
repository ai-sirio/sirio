# Language server configuration and supervision — design

**Date:** 2026-09-15
**Status:** approved, not yet implemented
**Parent:** `docs/superpowers/specs/2026-09-14-lsp-client-design.md` — this is
sub-project 2 of the five that document names. Sub-project 1 (`sirio_lsp`)
is implemented; this one makes it run.

## Goal

Turn `sirio_lsp` from a crate that *can* talk to a language server into an
app that *does*: a user-editable table of servers, a rule for deciding which
one a file needs and where to root it, and a supervisor that starts them
lazily and tears them down when Sirio quits.

## What sub-project 1 left here

`sirio_lsp::Server::launch(command, args, root)` takes a command and a
directory and knows nothing about where either came from. That was
deliberate. This sub-project answers both questions and nothing else: still
no hover, no gutter, no panels — those are sub-projects 3 and 4.

## Decisions

| Decision | Choice | Why |
|---|---|---|
| Config home | `~/.config/sirio/languages.toml` | Helix's model. Sirio's first user config file; the path follows the lookup `sirio_agents::opencode` already performs for OpenCode. |
| Project root | Marker files declared per language, walked up from the file | The alternative — always the worktree root — fails on **this repository**: Sirio's `Cargo.toml` lives in `rust/`, so rust-analyzer rooted at the worktree would find no project at all. |
| Registry key | `(project root, language)` | Not `(worktree, language)`, which the parent spec assumed. With markers, two Rust workspaces inside one worktree correctly get two servers. |
| Shipped defaults | rust, typescript, python, go | The four the agents work in most. Enough that the feature works before anyone edits anything. |
| Reload | Lazy, on mtime, checked when a server would start | No watcher, no inotify, no extra thread. See §5 for what this does **not** promise. |
| Precedence | First matching entry wins | Makes a shipped default overridable by adding an entry, without having to know the default exists or where it sits. |

## §1 Placement

- **`sirio_lsp::config`** — a new module in the existing crate. Pure: parse,
  resolve, decide. It spawns nothing. Adds `toml` with serde derive; the
  repo's existing `toml_edit` is for rewriting Codex's config
  format-preservingly, which is a different job.
- **`sirio`** — the supervisor. Owns the running servers, places each read
  loop on gpui's background executor, tears them down on `cx.on_app_quit`.
  This is the repo's standing rule: processes are wired in `main.rs`.
- **`sirio_ui` — no changes at all.** `FileView::set_notice` and
  `clear_notice` already exist (`file_view.rs:271`), so the one user-visible
  message this sub-project produces reaches the screen through the seam the
  file context menu already established.

## §2 The table

```toml
# ~/.config/sirio/languages.toml

[[language]]
name = "rust"
extensions = ["rs"]
command = "rust-analyzer"
args = []
roots = ["Cargo.toml"]

[[language]]
name = "typescript"
extensions = ["ts", "tsx", "js", "jsx", "mjs", "cjs"]
command = "typescript-language-server"
args = ["--stdio"]
roots = ["package.json", "tsconfig.json"]

[[language]]
name = "python"
extensions = ["py", "pyi"]
command = "pyright-langserver"
args = ["--stdio"]
roots = ["pyproject.toml", "setup.py", "requirements.txt"]

[[language]]
name = "go"
extensions = ["go"]
command = "gopls"
args = []
roots = ["go.mod"]
```

Those four are the compiled-in defaults, used verbatim when the file is
absent and merged ahead of nothing when it is present — a user entry for an
extension a default also claims wins by appearing first (§3).

The directory is resolved the way `sirio_agents::opencode` resolves
OpenCode's: `$SIRIO_CONFIG_DIR`, else `$XDG_CONFIG_HOME/sirio`, else
`~/.config/sirio`. The environment arrives as a parameter, not from
`std::env`, so the whole lookup is testable without touching the real one.

## §3 From a file to a server

Three steps, each testable on its own:

1. **Extension to language.** First matching entry wins. User entries are
   read before the defaults, so adding one overrides without deleting.
2. **Root walk.** From the file's directory, upward, looking for any name in
   that language's `roots`, **stopping at the worktree root**. Found: that
   directory is the project root. Not found: the worktree root, which keeps
   a scratch file in a repo with no manifest working rather than erroring.
3. **Key.** `(project root, language name)`. The worktree root comes from
   `SirioWorkspace::worktree_root_for` (`main.rs:6073`), which already
   resolves a path to its longest matching workspace root — correct for
   nested worktrees without further work.

An extension with no entry resolves to nothing, and nothing is not an
error: a `.txt` file has no language server and must not produce a message.

## §4 Supervision

A `HashMap<(PathBuf, String), ServerHandle>` in `sirio`. A handle owns the
`sirio_lsp::Server`, the gpui task running its read loop, and the latest
diagnostics that server has pushed.

- **Lazy start:** the first file of that language under that root.
- **Lifetime:** until Sirio quits, as the parent spec decided. Memory grows
  monotonically across roots; that was accepted knowingly.
- **Teardown:** `cx.on_app_quit` runs `Server::stop` for each — `shutdown`,
  `exit`, wait, then kill. The kill is the contract, not a fallback:
  `PaneRegistry::shutdown` treats panes the same way, and without it a
  server that ignores `exit` keeps indexing after Sirio is gone.

## §5 Reload, and what it does not promise

The config is re-read when its mtime has changed **and** a server is about
to start. No file watcher.

This means: edit the table, open a file, the new entry applies. It also
means **a running server keeps the configuration it started with**.
Restarting it silently to pick up a change would contradict the parent
spec's "no automatic restart", which exists because a restart loop on
rust-analyzer re-indexes a repository forever. A server that should pick up
new settings is one the user closes.

## §6 Failure

| Failure | Behaviour |
|---|---|
| `languages.toml` will not parse | A notice naming the file and the line, **and the compiled-in defaults are used**. |
| No entry for this extension | Silence. Not every file has a language server. |
| `command` not found on PATH | `FileView::set_notice` naming the command, so the user can fix the entry they wrote. |
| Server crashes | That key is marked dead. No automatic restart. |

Falling back to defaults on a broken file is the least obvious choice here.
The alternative — no servers until it parses — is more literal and more
hostile: it removes a working feature to punish a syntax error in a file the
user was in the middle of improving. The notice says what is wrong; the
defaults keep the rest standing.

## §7 Testing

- **Config parsing and resolution are pure**, so they are table-driven unit
  tests in `sirio_lsp`: extension matching, precedence, malformed input,
  absent file.
- **The root walk** runs against temporary directories: a marker at the
  file's own level, one several levels up, none at all, and one *above* the
  worktree root that must **not** be found.
- **Supervision** is tested in `sirio`, where gpui tests already live.
- No test may require a real language server, and none may need the network.

## Global constraints

- No tokio. The read loops go on gpui's background executor.
- `sirio_lsp` gains no gpui dependency; `config` is as pure as `position`.
- `sirio_ui` is not modified by this sub-project.
- Every `sirio_perf` trace name stays a content-free `&'static str` — a
  language name is fine, a file path is not.
- Dependencies pinned exactly.
- Conventional Commits; no co-author or session trailers.
