# P34 — shell routing for the editor (for codex12 / the integrator)

The editor's logic lives in `tiller_ui` (`crates/tiller_ui/src/editor.rs`
headless model + tests; `file_view.rs` view delegating to it). Everything
below is a **shell-side** change in `tiller/src/main.rs` (and, for the
right-panel context menu, `tiller_ui/src/right_panel.rs` is mine — but the
menu surfaces a shell action, so the wiring is listed here). None of it is
pixel-testable on this machine, so these are the *wiring* specs, not
exercised claims.

## 1. F-EDIT-08 — dedupe: one document per path

`add_file_tab` (main.rs:2546) creates a new `FileView` unconditionally.
Route it through the registry:

```rust
// Workspace gains a field: documents: tiller_ui::editor::DocumentRegistry
fn add_file_tab(&mut self, path: PathBuf, cx: &mut Context<Self>) {
    let outcome = self.documents.open(&path);
    if let Some(index) = self.tabs.iter().position(|tab| tab_is_path(tab, &path)) {
        // Focus the existing tab instead of opening a second copy.
        self.active_tab = index;
        self.schedule_save();
        self.sync_activity(cx);
        cx.notify();
        return;
    }
    // ... existing tab creation; the FileView itself needs no change.
    // Associate tab.id ↔ outcome.id() so close_tab can registry.close().
}
```

`tab_is_path` reads the `TabContent::File { view }` entity's `view.read(cx).path()`.
Call `self.documents.close(id)` inside `close_tab` for `TabContent::File` tabs.
Proof (already green in `tiller_ui`): `DocumentRegistry::open` twice on one
path returns `New` then `Existing(same id)`, `len() == 1`.

## 2. F-EDIT-04 — ⌘S saves the active file

Bind a `cmd-s` action (GPUI `KeyBinding::new("cmd-s", Save, ...)` in the
workspace key context) that resolves the active tab and, when it is a
`TabContent::File`, calls:

```rust
view.update(cx, |view, cx| {
    if let Err(error) = view.save(cx) {
        // surface the error: the editor kept it visible
        // (view.editor().save_error()); a toast is fine.
    }
});
```

The view's `save()` is synchronous (files are ≤ 1 MiB by `MAX_FILE_BYTES`).

## 3. F-TAB-16 — dirty close confirmation

`FileView::is_dirty()` is the flag. In `close_tab` (main.rs:2441) and
`close_tab_by_id` (main.rs:3244), when the tab being closed is a
`TabContent::File` and `view.read(cx).is_dirty()` is true, show a
confirm/discard prompt and keep the tab on cancel. The prompt itself is
pixel-bound (`NOT EXERCISED`); the flag is tested through the view
(`dirty_state_is_reachable_through_the_view`). Optionally paint a dirty dot
in `render_open_tab` from the same flag.

## 4. F-EDIT-05/06 — external-change checks on activation

The conflict banner appears when `view.check_external()` has run and the
editor's `conflict()` is non-`None`. Fire it:
- when a file tab becomes active (`select_tab`, `select_activity`), and
- on window focus (the workspace's focus handler), for the active file tab.

`check_external` re-reads the file and compares against the last synced
snapshot; `view.reload(cx)` / `view.keep(cx)` / `view.save(cx)` are the
banner's resolutions (the banner buttons already call them). Tests:
`conflict_detection_and_resolutions_work_through_the_view` + the
model-level external-mutation tests.

## 5. F-EDIT-10/11 — right-panel file context menu

The Files tree (`right_panel.rs`) currently opens a file on click
(`RightPanelEvent::OpenFile`). The context-menu entries that don't exist
yet can be added by the shell as:
- **Open** → existing `OpenFile` path.
- **Show in folder** (F-EDIT-10, Linux adaptation of "Show in Finder"):
  `tiller_ui::editor::fs_actions::reveal_command(path)` returns the
  `xdg-open <containing directory>` command; `spawn()` it. Derivation is
  tested; the spawn needs a display → `NOT EXERCISED`.
- **Copy path** (F-EDIT-11): `fs_actions::copy_path_text(path)` yields the
  absolute path string; put it on the clipboard via your existing clipboard
  mechanism. Derivation is tested; the X11 clipboard write needs a display
  → `NOT EXERCISED`.

## 6. F-EDIT-02 — toolbar → format ops

When the Markdown toolbar exists (pixel-bound), route clicks through
`FileView::format_markdown(op, selection, cx)` with
`tiller_ui::file_view::MarkdownFormatOp::{Bold, Italic, Heading, List,
Link { url } }`. All transformations are tested at the model level.

## Ownership note

`tiller_ui/src/editor.rs`, `tiller_ui/src/file_view.rs`, and the
`pub mod editor;` line in `tiller_ui/src/lib.rs` are P34's. Do not touch
`tiller_ui/src/conformance.rs` (integrator-owned).
