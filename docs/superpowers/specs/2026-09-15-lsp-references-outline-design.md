# References and document symbols — design

**Date:** 2026-09-15
**Status:** approved, not yet implemented
**Parent spec:** `docs/superpowers/specs/2026-09-14-lsp-client-design.md`
**Predecessor:** `docs/superpowers/specs/2026-09-15-lsp-navigation-design.md`
**Scope:** sub-project 4 — the two surfaces that need UI Sirio does not have.

## Goal

Answer the last two questions a reader of code asks: *where else is this
used*, and *what is in this file*. Sub-project 3 answered "what is this"
and "where is it defined"; these two are what remain of the parent
spec's read-and-navigate surface.

## What sub-projects 1–3 already left in place

This sub-project is small because the seams were cut for it.

| Already there | Where |
|---|---|
| `Capabilities { hover, definition, references, document_symbols }`, parsed from `InitializeResult` | `sirio_lsp/src/lifecycle.rs:30` |
| `Target { path, line, character }` — already the shape of a reference | `sirio_lsp/src/navigation.rs:21` |
| `targets(&Value)` — flattens `Location`, `Location[]`, `null` and `LocationLink` | `sirio_lsp/src/navigation.rs:97` |
| `open_at_line(path, line)`, including the not-yet-loaded tab | `sirio/src/main.rs:10723` |
| "menu entry → switch the right panel → show results" | `show_file_history`, `sirio/src/main.rs:6058` |
| "host computes, pushes down with a setter" | `RightPanel::set_activity`, `right_panel/mod.rs:461` |
| A lazily-built child entity behind a panel surface | `ensure_history` / `render_history`, `right_panel/mod.rs:737` |
| The scripted in-memory server for protocol tests | `connection::tests::with_scripted_server` |

`references` is therefore a new request against an existing parser, and
the references panel is an existing pattern with a new payload. The
genuinely new work is the symbol parser, the outline overlay, and the
line previews.

## Decisions

| Decision | Choice | Why |
|---|---|---|
| Where references land | A fifth right-panel surface, `PanelView::References` | The results must survive being clicked: a list is walked one entry at a time. A peek anchored at the cursor dies on the first click, and a `TabContent` variant would touch session, persistence, tab reordering and closing. |
| Where the outline lives | A filterable overlay that jumps and disappears | Zed's primary outline affordance. Costs no width from a centre column whose floor is 320px (`MIN_CENTER_PANE_WIDTH`), and needs no synchronisation with the buffer. |
| How both are invoked | File context menu for references; command palette for the outline | References have a target under the pointer; the outline does not. No new keyboard shortcuts — `ctrl-shift-o` is already the palette's History row, and on Windows that chord never reaches the window at all (`main.rs:299`). |
| Symbol tree | Flattened in `sirio_lsp`, `depth` carried as a field | A filtered *tree* has no good answer (keep the parents of matches? the children?). A flat list's indent degrades by itself when filtered, and the recursion stays in one tested place. |
| `includeDeclaration` | `true` | "12 references" that omits the declaration misses the one place the reader most often wants to get back to. |
| `ContentModified` | Named, not retried | Deviation from the parent's §5; see §5 below. |
| `sirio_ui` dependencies | Unchanged — still no `sirio_lsp` edge | The amendment in the predecessor spec holds: two new files, no new dependency. |

## §1 What enters `sirio_lsp`

### `references` joins `navigation.rs`

Same shape as `definition`: a positional request whose answer is
`Location | Location[] | null`. It reuses `targets()`, already tested
against all four response shapes.

```rust
/// Everywhere the symbol at a position is used, the declaration
/// included. Empty is a real answer.
pub async fn references(
    client: &Client,
    path: &Path,
    position: Position,
) -> Result<Vec<Target>, LspError>
```

The params carry `"context": { "includeDeclaration": true }`. A client
that sends the wrong context does not fail — it silently returns a
shorter list, which is why the test asserts the outgoing params rather
than only the parsed answer.

### `symbols.rs`, a new module

`textDocument/documentSymbol` answers in **two incompatible shapes**, and
a client that parses one of them works against half the servers and shows
nothing against the rest, with no error anywhere:

| Shape | Recognised by | Sent by |
|---|---|---|
| `DocumentSymbol[]` — nested, with `children` and `selectionRange` | it has `selectionRange` | rust-analyzer, gopls, tsserver |
| `SymbolInformation[]` — flat, deprecated, with `location` and `containerName` | it has `location` | older servers |

Both flatten into one list:

```rust
pub struct Symbol {
    pub name: String,
    pub detail: Option<String>,
    pub kind: SymbolKind,
    /// Zero-based, the protocol's own line numbering.
    pub line: u32,
    /// Nesting depth in the tree this came from; 0 for the flat shape.
    pub depth: usize,
}

/// The kinds the outline distinguishes, not the protocol's 26.
pub enum SymbolKind {
    Function, Method, Struct, Enum, Interface,
    Field, Constant, Variable, Module, Other,
}

pub async fn document_symbols(client: &Client, path: &Path)
    -> Result<Vec<Symbol>, LspError>;

/// Pure: both shapes, empty array, and `null`.
pub fn parse_symbols(answer: &Value) -> Vec<Symbol>;
```

The nested shape is walked depth-first, so a symbol's children follow it
immediately and `depth` is the indent. The flat shape's `containerName`
is dropped rather than turned into a fake depth: it is a name, not a
position in a tree, and inventing a hierarchy from it would order the
list wrongly.

`SymbolKind` is ours, not `lsp_types::SymbolKind` — which is a newtype
over `u32` whose meaning is a protocol table. The predecessor's amendment
applies unchanged: the conversion to the view's own enum happens in
`sirio`, exactly as `Severity` → `DiagnosticSeverity` already does in
`view_diagnostics`.

## §2 The references surface

`PanelView::References` as a fifth surface — an entry in `ORDER`, an
icon, an element id, a render arm — plus `right_panel/references.rs`
modelled on `history.rs`: a child entity, `ensure_references(cx)`, and a
subscription carrying its events out.

The round trip, each step already having a precedent:

1. Context menu → `FileViewEvent::FindReferences { path, offset }`, the
   twin of `GoToDefinition`.
2. **The panel opens at once**, before the answer:
   `right_panel_visible = true`, `PanelView::set(References, cx)`, and
   the surface showing *Searching…* — the sequence of
   `show_file_history`. Waiting for the answer to open it would make the
   menu entry appear to do nothing for several seconds against a server
   that is still indexing, which is the same reason the outline overlay
   opens before its own answer (§4).
3. `sirio` does what `go_to_definition` does (`main.rs:10679`): a
   `LineIndex` over the buffer, `offset` → `Position`, the request on
   the background executor.
4. The answer reaches the surface through `set_references(..)`, a setter
   shaped like `set_activity`. **An error arrives the same way** and
   becomes the surface's empty state — including `NotReady` (§5), which
   is the likeliest one here.
5. A click on a row → `RightPanelActionEvent::OpenAtLine { path, line }`
   → `open_at_line`, which already handles the tab that is still loading.

### Rows, grouped by file

```rust
pub struct ReferenceRow {
    pub path: PathBuf,
    /// Worktree-relative, for display.
    pub display: String,
    /// Zero-based; the row shows `line + 1`.
    pub line: usize,
    /// The source line, trimmed. Empty when it could not be read.
    pub preview: String,
}
```

One header per file, its results beneath. The grouping is a pure function
over rows already ordered by (path, line) — testable with no window.

### The previews, and their bound

`path:1042` alone is a coordinate; with the line's text beside it the
list reads. But `sirio` holds the buffer only for *open* files, and
references point mostly at closed ones.

So: one background task reads each **distinct** file once, takes the
lines it needs, and pushes the complete rows down in a single
`set_references`. Bounded at 100 distinct files; rows beyond that show
`path:line` with no preview, and the header always states the true total.
Without the bound, "references to `new`" in this repository reads a
thousand files on the UI path.

### The header wants the symbol's name

`"12 references to `spawn`"` — and the LSP answer does not contain the
name. It comes from the buffer around the `offset`: a scan left and right
over `is_alphanumeric() || '_'`, a pure function in `sirio`, which owns
the buffer. When it yields nothing the header says `"12 references"`
rather than inventing one.

An empty result is the panel's own empty state — "No references found" —
because by then the panel is what was asked to show them. Its four states
are distinct and each is said plainly: *Searching…*, the results, "No
references found", and the error text.

Switching worktree clears the list, alongside the panel's existing
`clear_worktree`: a reference row names a path in a tree that is no
longer selected.

## §3 The menu entry, and the defect sub-project 3 left open

`FileContextAction::FindReferences` sits beside `GoToDefinition`, routed
`App`, carrying a `disabled_reason` when the server offers no
`referencesProvider` or has not started — the shape #478 established.

This forces the fix of a known rough edge, in the right direction.
`definition_available` matches **by containment**
(`path.starts_with(root)`), so a Python file inside a Rust project shows
the entry enabled. Cloning that for references would be writing it twice.

The missing piece was already in hand at the call site:

```rust
// main.rs:6006, in file_context_facts, already present
let worktree = self.worktree_root_for(path);
```

The supervisor gains an exact lookup that reads the table **without
reloading it** — the reload is a freshness step that belongs to opening a
file, not to computing menu facts, and it was the only reason `entry_for`
needed `&mut`:

```rust
/// The negotiated capabilities of the server this file would actually
/// use. `None` when no entry matches, no server is running, or the
/// running server is a different language's.
pub fn capability_for(&self, file: &Path, worktree_root: &Path)
    -> Option<&Capabilities>;
```

`definition_available` and `references_available` become two lines over
it, and the defect is closed rather than doubled.

## §4 The outline overlay

Lives in **`sirio_ui/src/outline.rs`**, not in `main.rs`. The command
palette lives in `main.rs` because it is entangled with `PaletteCommand`
and its typed dispatch; the outline has nothing to do with that, and
`main.rs` is already 35 191 lines.

The duplication with the palette is real, deliberate and bounded: the
filter rule is **deliberately identical** —
`trim().to_ascii_lowercase().contains()`, the same line as
`command_palette::filter_entries:496` — so the app's two search boxes
behave the same way. It is a pure function, tested without a window.

1. The palette command "Go to Symbol in File" opens the overlay
   **immediately**, bound to the active file's path, showing *Loading
   symbols…*; the request goes out in the background.
2. The answer calls `set_symbols(path, symbols)`, **discarded if the
   overlay is closed or bound to a different path**. No `seq` counter:
   unlike hover, only one request is ever outstanding, because there is
   only one overlay — the path is the whole reconciliation key.
3. Keys: typing filters, ↑/↓ move, Enter jumps and closes, Esc closes —
   `handle_palette_key`'s behaviour.
4. The jump emits `OutlineEvent::Jump { path, line }`, handled with
   `open_at_line` rather than a direct `reveal_at`: one path, already
   tested.

`sirio` holds it as `outline: Entity<Outline>` and renders it only while
`outline.read(cx).is_open()`. The overlay owns what belongs to an
overlay — its open flag, query, selection, focus, the symbols and the
path they belong to; `sirio` owns the fetch and the jump. That split is
why the keyboard behaviour is testable in `sirio_ui` with no workspace
around it.

Opening *after* the round trip was the alternative and would have been
the mistake: against a cold rust-analyzer the command would appear to do
nothing for several seconds.

Three empty states, each said plainly and distinguishable from the
others: *Loading symbols…*, *No symbols in this file*, *No match*.

In the palette the entry carries its own `disabled_reason` when there is
no active file or no server for it, in the shape `PaletteDisabledReason`
already has. No new keyboard shortcut is introduced.

## §5 `ContentModified`, a declared deviation

It is handled nowhere today: `grep -rn "ContentModified\|32801"` finds
nothing in either crate. It did not show in sub-project 3 because a cold
hover fails silently and no one notices; a references list against a
freshly started server is the first place it bites.

`LspError` gains a variant, classified at the single site where a
`ResponseError` becomes an `LspError` (`connection.rs:169`):

```rust
/// The server accepted the request but its index is not ready.
/// JSON-RPC -32801. Not a fault: rust-analyzer is still reading the
/// repository.
NotReady,
```

**Deviation from the parent spec's §5, which said "retry silently":** it
is named, not retried. A retry needs a timer and a budget, and its
failure mode — a retry loop against a server that indexes for minutes —
is exactly what the "no automatic restart" reasoning in that same
paragraph rejects elsewhere. One honest message costs the user a click
and introduces no machinery.

It appears in the panel's empty state, in the overlay's, and — for free —
in the notices of hover and go-to-definition, which already display
`error.to_string()` (`main.rs:10715`). Two of sub-project 3's surfaces
improve without a line changing in them.

## §6 Structure

```
sirio_lsp/src/
  navigation.rs   + references(..)              reuses targets()
  symbols.rs      NEW  Symbol, SymbolKind, document_symbols(..), parse_symbols(..)
  connection.rs   + the -32801 classification at line 169
  lib.rs          + re-exports

sirio/src/
  lsp.rs          + capability_for(..); definition_available / references_available over it
  main.rs         find_references, go_to_symbol, symbol_at(offset), preview reading,
                  the palette command, the facts fix

sirio_ui/src/
  right_panel/references.rs   NEW  ReferencesList + the grouping
  right_panel/mod.rs          + PanelView::References, set_references, the event bridge
  outline.rs                  NEW  the overlay: query, selection, focus, keys
  file_context_menu.rs        + FindReferences
  file_view.rs                + FileViewEvent::FindReferences
```

`sirio_ui` gains two files and **no dependency**.

## §7 Testing

- **`sirio_lsp`** — `parse_symbols` over **both** shapes (nested with
  children two deep, `depth` asserted; and the flat
  `SymbolInformation`), over an empty array and over `null`.
  `references` against `with_scripted_server`, asserting that
  `includeDeclaration: true` **actually goes out in the params** — the
  wrong context returns a shorter list, not an error.
- **`sirio`** — `symbol_at(text, offset)` at the identifier's edges
  (buffer start, buffer end, underscore, a non-ASCII identifier); the
  grouping function; and the regression for the closed defect:
  `capability_for` answering `None` for a Python file under a Rust root.
- **`sirio_ui`** — on the `mounted_file_view` bench: the menu entry
  disabled *with its reason*; the filter narrowing; the arrows wrapping;
  Enter emitting **exactly one** `Jump`; an answer for a different path
  discarded; one header per file in the list; the overlay's three empty
  states and the panel's *Searching…* distinguished from its "No
  references found" — a surface that says nothing while it waits and a
  surface that found nothing must not look alike.
- Runner: `cargo nextest run -p sirio`, never `cargo test -p sirio` —
  the latter reports two false GTK failures that nextest does not,
  because each test owns its process.

## Global constraints

Unchanged from the parent spec, and restated because every task inherits
them:

- Async on gpui's executor. **No tokio, ambient or otherwise.**
- `sirio_ui` never spawns a process; `sirio` owns every client.
- `sirio_ui` never sees an LSP `Position`; it speaks lines and byte offsets.
- Every `sirio_perf` trace name stays a content-free `&'static str`.
- Conventional Commits; the workspace version moves once per cycle.
- `Scripts/ci.sh` must never need a real language server.

## Out of scope

- `workspace/symbol`, the cross-file symbol search: a different question
  with a different UI.
- `textDocument/rename` and call hierarchy.
- Syntax colour fidelity and `semanticTokens` — sub-project 5.
- Editing the results in place, multibuffer-style.
- Keeping the outline in sync with the buffer. It is fetched when opened
  and closes on Enter or Esc, so its content never outlives the moment it
  was asked for.
