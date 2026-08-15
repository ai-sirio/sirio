# Wave C slice C-P4 — 7 rows

Runs in parallel with other slices. Every file your rows touch is listed below and no other slice owns any of them.

## Files you own

- `rust/crates/tiller_project/src/file.rs`
- `rust/crates/tiller_terminal/src/context_menu.rs`
- `rust/crates/tiller_ui/src/browser.rs`
- `rust/crates/tiller_ui/src/file_view.rs`
- `rust/crates/tiller_ui/src/right_panel.rs`

## Rows

### `F-CHG-03` — ledger line 194, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/right_panel.rs
- **Evidence on record:** half-proven (driver claimed exercised-working; downgraded): real distinct error state confirmed (chmod 000, AE~78k vs OK, does not self-heal without a click). But driver's 2 cited 'recovered' frames show the Changes-tab content recovered while the Files panel (this row's actual subject) is still 'Files unavailable: Permission denied' -- checked 5 more uncited same-session frames, 7/8 post-click captures show Files panel still stuck; only 1 uncited frame (07-scan-c5.png) shows true full recovery. Loading-flash sub-c

### `F-CHG-20` — ledger line 211, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_ui/src/right_panel.rs
- **Evidence on record:** Running state confirmed working (2 live labelled rows). Nothing-running state: 'No activity' label renders as illegible sub-pixel specks, confirmed real via edge-crop/contrast/repaint checks, not a rendering artifact of the check. shots/224b,224d.

### `F-BRW-01` — ledger line 250, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_ui/src/browser.rs
- **Evidence on record:** Two independent live captures (4s/10s settle): pixel-scanned rect (331,114) 729x679 — bit-identical to pre-fix FAILED evidence. Sidebar 'Primary' badge still clipped to 'Pri'; ~176px still unpainted before Files panel. Code fix confirmed present at HEAD but produces zero on-screen change.

### `F-BRW-03` — ledger line 252, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_ui/src/browser.rs
- **Evidence on record:** Caret repositioning via click and End/Home now works. But typing is broken 3 separate ways across 3 relaunches: xdotool 'type' landed 0 characters; 3 separate 'key' calls landed out of order/one position off (https://example.com/ became htweqtps://example.com/); 4 separate 'key' calls dropped 3 of 4 chars. Reproducible, not a flake. ctrl+a-then-replace not cleanly separable from this corruption.

### `F-TERM-04` — ledger line 322, currently **half-proven**

- **Files triage named:** rust/crates/tiller_terminal/src/context_menu.rs
- **Evidence on record:** Re-confirmed (no new drive), verified by critic: open_context_menu right-click-only (lib.rs:1446/1505), wayland-drive.sh has no right-click primitive, full ControlAction enum has no clipboard bypass. Does not touch existing half-proven basis (p17-rclick-term.png shows menu live via X11 lane, opened and confirmed by critic). Owed half unchanged.

### `F-CORE-FILE-03` — ledger line 387, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_project/src/file.rs
- **Evidence on record:** Independently confirmed wayland-drive.sh's action vocabulary (ctl/click/move/type/key/title/shot) has no drag primitive — no press/motion/release sequence exists. Matches ENVIRONMENT.md:76-78. Tooling gap, not a code question.

### `F-CORE-FILE-06` — ledger line 391, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/file_view.rs
- **Evidence on record:** Silent-reload/clean-buffer independently reconfirmed via 02-17-editor-open.png -> 03-18-after-external-edit.png (real FileView tab, content silently updated, no banner, distinct from Changes panel) -- correcting the pass's own mis-cited 02-20/04-22 pair (idle Terminal, not the editor). Dirty-buffer/conflict-banner gesture demonstrably missed: 03-24-dirty-b.png shows the typed marker unsent in the Terminal prompt, not the editor; the pass's cited 02-26/04-28 pair shows the git Changes panel, not FileView's conflict 

