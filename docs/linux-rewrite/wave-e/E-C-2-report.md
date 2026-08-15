# Wave E slice E-C-2 — report

## `F-CTRL-WORK-01` — fixed: schedule_catalog was wiping the persisted comment on every boot

Root cause confirmed exactly as recorded: `main()` calls `session_store.schedule_catalog(&project_catalog)`
at startup (catalog id normalization) *before* it reloads persisted comments into `ControlState`.
`write_catalog` rebuilt every worktree row from the in-memory `ProjectCatalog` — which has no
`comment` field at all — via `WorktreeRecord::new()` (comment: `None`) and upserted it with
`comment = excluded.comment`, so the column was reset to NULL on every single `schedule_catalog`
call, not just at startup: any later catalog write (adding a project, cloning a worktree, etc.)
would have wiped a comment set via `worktree.set` moments earlier too.

Fix: `write_catalog` (`rust/crates/tiller/src/session.rs`) now looks up each project's existing
worktree rows before rebuilding them, and carries the existing row's `comment`/`created_at`
forward into the freshly-built record before the upsert, instead of defaulting them.

Added a regression test, `schedule_catalog_preserves_a_previously_persisted_worktree_comment`,
that reproduces the exact sequence (`schedule_catalog` → persist a comment on the resulting row →
`schedule_catalog` again) and asserts the comment survives. Fails on the pre-fix code, passes now.
All 30 existing `session::tests` pass unchanged.

Files: `rust/crates/tiller/src/session.rs`.

**How to exercise:** `ctl project.add path=<dir>` then `ctl worktree.set worktree=<path>
comment="agent pane"`, restart the process against the same DB (or trigger any later
`schedule_catalog` call, e.g. adding a second project), then read the `worktree` table's
`comment` column directly — it now stays `"agent pane"` instead of reading back NULL.

## `F-CTRL-BROWSER-04` — implemented `browser.snapshot`; `browser.screenshot` documented as a real gap

`browser.snapshot` now reaches `handle_browser_action` instead of being pre-rejected by
`BROWSER_CAPABILITIES`. It runs a small JS snippet (`BROWSER_SNAPSHOT_SCRIPT`) through the same
`evaluate_script` plumbing `browser.eval`/`browser.console` already use, and returns a JSON
structural snapshot (tag/role/accessible-name/bounding-box for interactive and labelled
elements) — an accessibility-tree-shaped answer to "what's on the page and where", not a pixel
dump.

`browser.screenshot` deliberately stays out of `BROWSER_CAPABILITIES`: wry 0.56's public
`WebView` API (`webkitgtk` backend, see `build_webview`/`build_production_webview` in
`rust/crates/tiller_ui/src/browser.rs`) exposes no pixel-capture method at all, and this
workspace has no `webkit2gtk` FFI dependency to reach WebKitGTK's own
`webkit_web_view_get_snapshot` directly. A real fix needs: (1) adding a `webkit2gtk`/`gtk-sys`
dependency, (2) getting the underlying `WebKitWebView*` handle out through wry (not exposed
publicly — would need an unsafe transmute against wry's internal GTK widget layout, or a wry
fork/patch), (3) bridging `webkit_web_view_get_snapshot`'s `GAsyncResult` callback back into
GPUI's executor. That is a new-dependency, FFI-boundary undertaking, not a wiring gap — leaving
it pre-rejected rather than shipping a screenshot method that silently returns garbage or panics.

Updated the existing `browser_methods_are_explicit_and_capabilities_are_truthful` unit test:
`browser.snapshot` moved from the "must stay pre-rejected as unsupported" group into the
"must be accepted, not pre-rejected" group (alongside `browser.get`/`browser.wait`), and the
advertised-capabilities assertion now includes it. `browser.screenshot` stays in the rejected
group. All `tests::browser_*` tests pass.

Files: `rust/crates/tiller/src/main.rs`.

**How to exercise:** `ctl browser.open url=https://example.com` on an existing workspace, then
`ctl browser.snapshot` — previously always `{"error":"browser.snapshot is unsupported on
Linux: browser automation is not implemented"}`; now returns either a real JSON snapshot payload
or the WebView's own "Browser child is unavailable" error (same class of state-dependent error
`browser.eval`/`browser.console` already produce under this lane's Wayland WebView-construction
limitation — see `F-CTRL-BROWSER-06` below), never the blanket unsupported rejection.
`ctl browser.screenshot` is unchanged and still correctly reports unsupported.

## `F-CTRL-BROWSER-05` / `F-CTRL-BROWSER-06` — re-verified, no code change: structural Wayland lane limitation, not a defect

Re-read `build_webview`/`build_production_webview` in `rust/crates/tiller_ui/src/browser.rs`.
Both already try the direct XCB path first and fall back to an `XlibParent` XCB→Xlib adapter —
there is no missing fallback path to add. Under this lane's pure-Wayland compositor, both attempts
fail (visible XCB error banner), so no native `WebView` child window is ever constructed and
nothing downstream of it (`browser.wait`'s loading flag, `browser.eval`/`browser.console`'s
content-level success) can be driven to completion here. This matches the prior two critic
passes exactly: the wave-D critic held the one X11-capable lane slot for the whole pass and still
only reached the same "Browser child is unavailable" state-dependent error, not a content-level
result — an environment constraint (X11 vs. Wayland display availability), not something fixable
by editing `main.rs`/`browser.rs`. No production code change made for these two rows; they stay
as recorded (`half-proven`). Confirming further requires the X11 (`DISPLAY=:1`) lane and enough
otherwise-idle machine capacity for `browser.wait`'s polling loop to actually terminate before its
timeout, not more source changes.

Files touched: none (verification only).

**How to exercise:** requires the X11 lane (`DISPLAY=:1`), not `wayland-drive.sh`. `ctl
browser.open url=<local test page>` then `ctl browser.wait` / `ctl browser.eval
script="document.title"` — under X11 with the machine otherwise idle, `browser.wait` should
observe `loading` flip to `false` and `browser.eval` should return real page content, closing
the half these two rows are missing. A null/hung result under contention is inconclusive, not a
disproof — rerun when the X11 lane is free.

## `F-CORE-WSP-04` / `F-CORE-WSP-08` — blocked: real architectural gaps, not wiring oversights

Re-grepped fresh at HEAD, same result both prior critic passes recorded:
`grep -rn "LayoutCommand\|classify_layout_command\|WorkspaceTabViewState\|WorkspaceTab\b"
crates/` finds real callers only inside `rust/crates/tiller_project/src/layout.rs` itself, plus
a bare re-export in `tiller_project/src/lib.rs`. Zero callers in `tiller`/`tiller_ui`/anywhere
else.

The actual app in `rust/crates/tiller/src/main.rs` implements tab/pane lifecycle (insert, split,
move, close, activate, rename, divider-fraction drag) as direct, ad hoc GPUI state mutations —
`close_tab`, `close_tab_by_id`, and similar methods scattered through a ~13k-line file — with no
seam that currently produces or consumes a `LayoutCommand`. `layout.rs`'s `LayoutCommand` enum
and `classify_layout_command` (which derives `{structural: bool, focus: FocusIntent}` from a
command) look like the intended single dispatch point for a future refactor where every tab/pane
mutation goes through one typed command and the returned `LayoutTransition` decides whether a
structural re-layout or just a refocus is needed — but that refactor never happened. Wiring this
up for real means:

1. Replacing every direct tab/pane mutation call site in `main.rs` (there are well over a dozen —
   insert, split, move, close threaded through both id- and index-based call sites, activate,
   rename, `SetDividerFraction` for the drag handler) with construction of the matching
   `LayoutCommand` and a single `apply_layout_command` entry point.
2. Using `classify_layout_command`'s returned `LayoutTransition` to decide, at that one entry
   point, whether to invalidate/relayout the pane tree (`structural: true`) or only move focus
   (`FocusIntent::Tab`/`Divider`) — replacing whatever ad hoc "do I need to relayout" logic each
   call site currently has inline.
3. Doing this without breaking any of `main.rs`'s existing pane/tab tests, which assert on the
   current direct-mutation behavior.

That is a cross-cutting refactor of the app's central mutation surface, not a call site to add —
exactly the "architectural gap, not a bug" case the brief calls out by name
(`WorkspaceTabViewState`, `LayoutCommand` are both listed examples). Attempting a narrow token
call site (e.g. calling `classify_layout_command` from one new, unused helper) would make `grep`
show a caller while changing zero behavior — worse than reporting this honestly. No production
code changed for these two rows.

Files touched: none (confirmed the recorded diagnosis still holds; no safe partial fix exists
within this row's file list without the larger refactor above).

**How to exercise:** N/A until the refactor lands — there is no control-socket method or UI
action that currently reaches `LayoutCommand`/`classify_layout_command`. Post-refactor, any tab
drag/split/close/rename/divider-drag action exercised through the normal UI or `ctl
surface.*`/`pane.*` methods should route through `apply_layout_command` and can be verified by
adding a debug assertion or log there.
