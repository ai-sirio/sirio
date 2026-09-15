# Navigating code with a language server — design

**Date:** 2026-09-15
**Status:** approved, not yet implemented
**Parent spec:** `docs/superpowers/specs/2026-09-14-lsp-client-design.md`
**Scope of this document:** sub-project 3 — hover, go-to-definition and
diagnostics. The first slice of the feature that is visible on screen.

## What exists, and what is missing

Sub-projects 1 and 2 landed the protocol and the supervision.
`sirio_lsp` frames `Content-Length` messages, runs the
`initialize`/`shutdown` handshake, maps byte offsets to UTF-16 positions,
reads `~/.config/sirio/languages.toml` and roots a server at its project.
`sirio`'s `LspSupervisor` starts one lazily when a file that wants one is
opened, and stops every one of them on `cx.on_app_quit`.

Nothing yet asks a server a question. Reading the seam again before
designing the answer turned up three gaps that have to close first, and
they are the substance of §1.

**No one answers a server's requests.** `Client::respond` exists and has
no caller. Today this is harmless by accident: `initialize` sends
`ClientCapabilities::default()`, which declares nothing, and
rust-analyzer sends `workspace/configuration` and
`window/workDoneProgress/create` only to a client that declared support
for them. The moment we declare the capabilities hover and definition
need, the risk returns — and it is the silent stall the parent spec's §2
warns about.

**No one drains `incoming()`.** The channel is unbounded, so it never
blocks the read loop; it grows instead. It is also where
`publishDiagnostics` arrives, so today there is nowhere for a diagnostic
to be read from.

**No one sends `textDocument/didOpen`.** The server is never told which
files are open. rust-analyzer can still answer from what it loaded off
disk, but it publishes diagnostics only for open documents, and an
edited, unsaved buffer does not exist for it at all.

What is already in place is the hard part. `EditableLine` computes
`line_start + layout.index_for_position(event.position)` on every mouse
event, so **the byte offset under the cursor is already available**
(`file_view.rs:1926`). The gutter is a 52px `div` ready to carry a glyph.
`deferred(anchored().position(…))` already backs the context menu
(`file_view.rs:1114`). And `file_view.rs:1977` already computes
`clicked_in_place && platform_held`, consuming it only for Markdown
links — the modifier-click that go-to-definition needs is a new `else`
on an existing branch, not a new mechanism.

## Decisions

| Decision | Choice | Why |
|---|---|---|
| Hover trigger | Mouse at rest ~300 ms | What "like Zed" means in practice. Costs one timer task, renewed per move. |
| Definition trigger | Modifier+click, plus a context-menu entry | The universal gesture, plus a discoverable path for whoever does not know it. |
| Diagnostics | Gutter glyph, message in the hover card | Reuses everything the hover card builds; no new surface. |
| Hover content | Plain text, code fences stripped | Keeps a Markdown parser out of the paint path. The signature stays readable. |
| Document sync | Full text, debounced 300 ms | The parent spec's "read, not author" decision is what makes full text affordable. |
| `sirio_ui` dependencies | Unchanged — no `sirio_lsp` edge | See "Amendment to the parent spec" below. |

## Amendment to the parent spec

The parent spec's §1 planned for `sirio_ui` to take the *types*
(`Hover`, `Diagnostic`, `Location`, `DocumentSymbol`) from `sirio_lsp`,
on the precedent of `sirio_acp`. **Sub-project 3 does not add that edge.**

The same paragraph forbids `sirio_ui` from learning UTF-16 — and
`lsp_types::Diagnostic` *is* UTF-16 positions. Taking the type into the
UI would carry in exactly what the sentence above it rules out. A type
should cross a crate boundary only when its invariants cross with it,
and "columns are UTF-16 code units" has no one to enforce it inside
`sirio_ui`.

Everything that crosses the seam is already converted: a line number, a
byte range, a string, a severity. The view model therefore lives in
`sirio_ui`, the conversion in `sirio`, and the crate graph is unchanged
by this sub-project. Sub-project 4 should be designed against this
amendment, not against the parent's §1.

## §1 Plumbing

### A router task per server

`ServerHandle` grows a third field: `{ server, read_loop, router }`. The
router runs on the **foreground** executor, because it updates entities,
and consumes `server.incoming()`:

| Shape | Action |
|---|---|
| `ServerRequest { id, method }` | **Always answered.** `null` for `workspace/configuration`, `client/registerCapability`, `window/workDoneProgress/create`; JSON-RPC `-32601 MethodNotFound` for anything else. |
| `Notification "textDocument/publishDiagnostics"` | Converted and delivered to every open `FileView` on that path. |
| Any other notification | Dropped, counted through `sirio_perf::event` under a content-free static name. |

Answering with an error rather than a silent drop is the point: a server
that asked something we do not implement learns so immediately, instead
of waiting on a reply that never comes. This needs a new
`Client::respond_error`.

**The router's logic is pure, its task is a shell.** `answer_for(method)`
and the diagnostic conversion are free functions, tested with no process,
no channel and no window. This keeps "the protocol is respected" and
"the task is wired correctly" as two separately diagnosable failures.

### Declared capabilities

`ClientCapabilities::default()` becomes a real declaration:
`textDocument.hover.contentFormat`, `textDocument.definition` with
`linkSupport: false` (so the server must answer `Location` rather than
`LocationLink`, halving what has to be parsed),
`textDocument.publishDiagnostics`, and
`textDocument.synchronization.didSave`.

Deliberately **not** declared: `workspace.configuration` and
`window.workDoneProgress`. Those are precisely the invitations to
requests we could only answer with `null`. The principle:
**declare only what you can use; answer everything regardless.**

### URIs, both ways

A new `sirio_lsp::uri` module: `uri_for_path` and `path_for_uri`, with
percent-encoding in both directions. This is not ornamental.
`lifecycle.rs:62` currently builds `format!("file://{}", root.display())`,
which yields an invalid URI for any path containing a space or a `#`;
`lifecycle` moves onto the new function. The reverse direction is what
tells a diagnostic which file it belongs to and where a definition lives.

### Document synchronisation

A new `sirio_lsp::document` module: `did_open`, `did_change` (full text),
`did_save`, `did_close`, plus a per-document version counter owned by the
supervisor.

`didOpen` is sent both when a server finishes launching for an already
open file **and** when a file opens onto an already running server — one
function reached from both paths, because the launch is asynchronous and
either order actually happens. `didChange` is debounced 300 ms after an
edit; `didClose` fires when the tab closes.

## §2 Hover

1. `EditableLine`'s `MouseMoveEvent` handler, today active only while
   dragging, gains a "no button held" branch calling
   `FileView::hover_moved(offset, window_point, cx)`.
2. `hover_moved` is deliberately poor: **an unchanged offset returns
   immediately**, with no `notify`. Otherwise it replaces a 300 ms timer
   `Task` — and in gpui replacing a `Task` cancels it, so dwell
   cancellation costs nothing.
3. On expiry the view emits `FileViewEvent::Hover { path, offset, seq }`.
4. `sirio` builds a `LineIndex` over the current buffer, converts the
   offset to a `Position`, issues `textDocument/hover`, and returns the
   answer through a setter.

**Staleness is the delicate part.** The reply arrives asynchronously and
the pointer has moved on. `FileView` keeps a monotonic counter and
**discards any reply whose `seq` is not the current one**. Without it the
card appears where the cursor no longer is.

The card reuses `deferred(anchored().position(p).snap_to_window())`, the
context menu's own mechanism. rust-analyzer replies in Markdown; the card
renders it as plain text in the code font with the ``` fences stripped.

Dismissal: a move onto a different offset restarts the timer; a key
press or the context menu closes it. Moving the pointer *onto* the card
does not dismiss it, and for free — the card paints above, so no new
offset ever reaches `EditableLine`.

## §3 Go to definition

1. **Modifier+click.** `file_view.rs:1977` already holds
   `clicked_in_place && platform_held`. For a non-Markdown file it now
   emits `FileViewEvent::GoToDefinition { path, offset }`.
2. **Context menu.** `FileContextAction::GoToDefinition`, routed `App`,
   carrying a `disabled_reason` when the server offers no
   `definitionProvider` or has not started — the shape #478 established.
   This forces one change: `set_shell_facts` is called once, when the tab
   opens, before any server exists. `start_language_server_for` must push
   the facts again when a launch succeeds, or the entry is permanently
   disabled with a stale reason.
3. `sirio` issues `textDocument/definition`, takes the first result,
   converts URI to path, and then:
   - **same file** — `scroll_to_item(line, Center)`, caret at the offset;
   - **another file** — `add_file_tab`, then reveal;
   - **nothing** — an honest `set_notice`, not silence.

**A trap the plan must name, or the implementer writes a compiling
no-op:** a freshly opened tab loads its content asynchronously. Revealing
a line immediately after `add_file_tab` does nothing, because the lines
do not exist yet. `FileView` needs `pending_reveal: Option<usize>`,
applied when `load_task` completes.

## §4 Diagnostics

The router resolves the URI to a path, finds the open views on it, and
delivers. **Conversion happens before delivery**: LSP ranges are UTF-16
and the `LineIndex` belongs to `sirio`, so what reaches the UI is
`{ line, byte range, severity, message }`.

The gutter glyph needs one precaution that is easy to get wrong. The
gutter is a 52px `div` containing `format!("{:>5} ", index + 1)`; adding
a glyph inside it shifts every line number on any file with an error. It
becomes a two-column flex row instead — 10px for the mark, 42px for the
number, the total unchanged — so **the numbers never move**.
`theme.danger` for errors, `theme.warning` for warnings,
`theme.text_faint` for information and hints. On a line carrying several
diagnostics the glyph shows the most severe and the card lists them all.

The message enters the same card as the hover text, and **the diagnostic
half of the card is built locally, with no round trip**. Errors therefore
appear instantly, and a server with no `hoverProvider` still shows them.

## §5 Structure

```
sirio_lsp/src/           (new)
  uri.rs          uri_for_path / path_for_uri, percent-encoding both ways
  document.rs     did_open / did_change / did_save / did_close + versions
  navigation.rs   hover(..) -> Option<String>, definition(..) -> Vec<Target>
  diagnostics.rs  parse(..) -> Option<(PathBuf, Vec<RawDiagnostic>)>
  connection.rs   + respond_error       lifecycle.rs   declared capabilities

sirio/src/
  lsp.rs          ServerHandle.router, document versions, PURE dispatch fns
  main.rs         event handlers, document lifecycle, reveal, facts refresh

sirio_ui/src/file_view.rs
  dwell state, set_hover / set_diagnostics / reveal_at, the mark column,
  FileViewEvent::{Hover, GoToDefinition, ContentChanged}
```

`navigation.rs` returns plain Rust (`Target { path, line, character }`),
so `sirio` never destructures a three-shaped `GotoDefinitionResponse`:
knowledge of the protocol stops where `sirio_lsp` stops.

## §6 Testing

- **`sirio_lsp`** — URI round-trips over hostile paths (space, `#`,
  accents, `+`); `document` payloads asserted as JSON against the
  existing scripted fake server; `respond_error` framing; `hover` against
  a Markdown reply **containing raw newlines in the payload**, the case
  the whole `Content-Length` framing exists for; `definition` in all
  three response shapes (single, array, `null`).
- **`sirio`** — a table over `answer_for` including an unknown method
  (`-32601`); a UTF-16 range over a line containing an emoji mapped to
  the correct byte range.
- **`sirio_ui`** — on the `mounted_file_view` bench: no event before
  300 ms and exactly one after; a stale `seq` ignored; the mark painted
  **without shifting the line numbers**; the card showing a diagnostic
  with no hover text present; `reveal_at` on a not-yet-loaded view
  applying once the load completes.

Runner: `cargo nextest run -p sirio`, never `cargo test -p sirio` — the
latter reports two false GTK failures that nextest does not, because
each test owns its process.

## Out of scope

References panel and document-symbol outline (sub-project 4). Syntax
colour fidelity and `semanticTokens` (sub-project 5). Restarting a
crashed server, which sub-project 2 deliberately does not do. Markdown
rendering inside the hover card.
