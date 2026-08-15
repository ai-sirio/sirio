# Wave E slice E-C-2 — 6 rows

**Scheduling round 2 of 5.** Running at the same time as: E-P3 — none of them owns any file you own, verified before launch.

Slices in **earlier** rounds have already landed their commits; slices in **later** rounds have not started. So every file below is yours exclusively right now — but **re-read each one from disk before editing**, because an earlier round has changed some of them since this brief was written. Do not trust quoted line numbers.

## Files you own

- `rust/crates/tiller/src/main.rs`
- `rust/crates/tiller/src/session.rs`
- `rust/crates/tiller_persistence/src/db.rs`
- `rust/crates/tiller_persistence/src/model.rs`
- `rust/crates/tiller_project/src/layout.rs`
- `rust/crates/tiller_project/src/worktree.rs`
- `rust/crates/tiller_terminal/src/lib.rs`
- `rust/crates/tiller_ui/src/browser.rs`
- `rust/crates/tiller_ui/src/chat.rs`
- `rust/crates/tiller_ui/src/editor.rs`
- `rust/crates/tiller_ui/src/file_view.rs`
- `rust/crates/tiller_usage/src/model.rs`

## Rows

### `F-CORE-WSP-04` — ledger line 380, currently **NOT EXERCISED**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller_project/src/layout.rs, rust/crates/tiller_terminal/src/lib.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Re-grepped fresh from disk: LayoutCommand/classify_layout_command still have zero callers outside layout.rs/lib.rs re-export. Nothing to drive; confirms builder's blocked claim. sweep D-MAIN-3, 2026-08-15

### `F-CORE-WSP-08` — ledger line 384, currently **NOT EXERCISED**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller_project/src/layout.rs, rust/crates/tiller_terminal/src/lib.rs, rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_ui/src/editor.rs, rust/crates/tiller_ui/src/file_view.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Re-grepped fresh from disk: WorkspaceTabViewState/WorkspaceTab still have zero callers outside layout.rs/lib.rs re-export. Nothing to drive; confirms builder's blocked claim. sweep D-MAIN-3, 2026-08-15

### `F-CTRL-BROWSER-04` — ledger line 447, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/browser.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Live: browser.screenshot and browser.snapshot both still return 'unsupported ... not implemented'. Source-confirmed BROWSER_CAPABILITIES in main.rs still omits both. Matches builder's blocked claim. wayland-drive.sh, label d3wlcheck3. sweep D-MAIN-3, 2026-08-15

### `F-CTRL-BROWSER-05` — ledger line 448, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/browser.rs
- **Wave-D critic evidence (2026-08-15, current tree):** browser.wait no longer errors unsupported and returns a real reply (confirmed live), a genuine change from absent. But loading never flipped false in any attempt; source trace shows build_webview/build_production_webview fail under Wayland entirely (visible XCB error banner), so no native WebView ever exists to fire a load-finished signal - a structural lane limitation, not provable here. The X11 lane needed to close this half was held by another agent for the whole pass. sweep D-MAIN-3, 2026-08-15

### `F-CTRL-BROWSER-06` — ledger line 449, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/browser.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Live: browser.eval/console now reach the real handler with a state-dependent error ("Browser child is unavailable", distinct from "no browser surface"), not the old blanket pre-rejection -- door is genuinely implemented. Took the DISPLAY=:1 lock to check content-level success; same error appeared and browser.wait hung under peak machine load (101 avg/12 cores), inconclusive rather than a disproof. sweep D-MAIN-4, 2026-08-15

### `F-CTRL-WORK-01` — ledger line 435, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller_persistence/src/db.rs, rust/crates/tiller_persistence/src/model.rs, rust/crates/tiller_project/src/worktree.rs, rust/crates/tiller_usage/src/model.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Live restart: comment set and confirmed in DB, process fully killed, fresh process against same DB read back empty (confirmed via direct sqlite read: column is NULL). Root cause in source: main()'s schedule_catalog() call at startup wipes worktree.comment via a fresh WorktreeRecord::new() before the restore step ever runs, so the fix's own write is destroyed by pre-existing code on every boot. sweep D-MAIN-4, 2026-08-15

