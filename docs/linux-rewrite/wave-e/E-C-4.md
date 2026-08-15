# Wave E slice E-C-4 — 5 rows

**Scheduling round 4 of 5.** Running at the same time as: E-P2 — none of them owns any file you own, verified before launch.

Slices in **earlier** rounds have already landed their commits; slices in **later** rounds have not started. So every file below is yours exclusively right now — but **re-read each one from disk before editing**, because an earlier round has changed some of them since this brief was written. Do not trust quoted line numbers.

## Files you own

- `rust/crates/tiller/src/main.rs`
- `rust/crates/tiller/src/panes.rs`
- `rust/crates/tiller_activity/src/activity.rs`
- `rust/crates/tiller_terminal/src/context_menu.rs`
- `rust/crates/tiller_terminal/src/lib.rs`
- `rust/crates/tiller_ui/src/right_panel.rs`
- `rust/crates/tiller_ui/src/sidebar.rs`

## Rows

### `F-SID-15` — ledger line 84, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Wave-D critic evidence (2026-08-15, current tree):** New finding, reproduced twice in separate app instances: Remove Worktree item exists and its confirm-gate code reads correctly, but a pixel-precise click on its label never reaches dispatch_context_action (menu stays open) — it instead opens the unrelated New Worktree... row positioned underneath, because render_context_menu (sidebar.rs) lacks the deferred() wrapper the tab menu documents needing. No confirm dialog ever appears. sweep D-MAIN-6, 2026-08-15

### `F-TAB-01` — ledger line 117, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/right_panel.rs
- **Wave-D critic evidence (2026-08-15, current tree):** No commit touched right_panel.rs this wave. Own live redrive (resolution-matched coordinates) reproduced a worse manifestation than previously recorded: expanding docs flashed Loading files... and settled back COLLAPSED, so the child-click gesture the row describes couldn't even be attempted. Standing verdict stands. sweep D-MAIN-6, 2026-08-15

### `F-TAB-11` — ledger line 127, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/panes.rs, rust/crates/tiller_terminal/src/context_menu.rs, rust/crates/tiller_terminal/src/lib.rs
- **Wave-D critic evidence (2026-08-15, current tree):** No commit touched context_menu.rs, panes.rs, or the relevant lib.rs wiring this wave. Confirmed via source: TerminalContextItem still has exactly 3 fields, ITEMS is a flat compile-time const, split_disabled_reason remains dead code with zero callers. Standing verdict stands. sweep D-MAIN-6, 2026-08-15

### `F-TERM-08` — ledger line 326, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_activity/src/activity.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Gating code (pane_close_needs_confirmation, request_close_terminal_at, both entry points) is correct on inspection. Live: notify badge and 12-item context menu each independently confirmed working, but 4 attempts to click Close Terminal on a running pane never yielded a clean banner capture, and a no-notify control produced a visually identical frame; a bare rightclick+shot also once failed to open the menu at all under ~75 load average. Cannot distinguish a real missing-banner defect from dropped synthetic input under this pass's contention. sweep D-MAIN-7, 2026-08-15

### `F-TERM-UI-01` — ledger line 535, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_terminal/src/lib.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Independently reconfirmed the 12-item menu still renders correctly. Split Left/Right and Copy Pane ID/Paste proven by other rows; the same six items (Copy, Copy Context, Set Title, Copy Terminal ID, Split Above/Down, Clear Terminal) remain unexercised by both builder and critic. No regression found. sweep D-MAIN-7, 2026-08-15

