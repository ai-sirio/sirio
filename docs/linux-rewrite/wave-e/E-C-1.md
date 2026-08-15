# Wave E slice E-C-1 — 6 rows

**Scheduling round 3 of 5.** Running at the same time as: E-P4 — none of them owns any file you own, verified before launch.

Slices in **earlier** rounds have already landed their commits; slices in **later** rounds have not started. So every file below is yours exclusively right now — but **re-read each one from disk before editing**, because an earlier round has changed some of them since this brief was written. Do not trust quoted line numbers.

## Files you own

- `rust/crates/tiller/src/main.rs`
- `rust/crates/tiller_activity/src/bootstrap.rs`
- `rust/crates/tiller_activity/src/mount.rs`
- `rust/crates/tiller_activity/tests/activity_domain_integration.rs`
- `rust/crates/tiller_persistence/src/db.rs`
- `rust/crates/tiller_project/src/domain.rs`
- `rust/crates/tiller_project/src/file_link.rs`
- `rust/crates/tiller_project/src/settings.rs`
- `rust/crates/tiller_terminal/src/domain.rs`
- `rust/crates/tiller_terminal/src/link_router.rs`
- `rust/crates/tiller_ui/src/changes.rs`
- `rust/crates/tiller_ui/src/chat.rs`
- `rust/crates/tiller_ui/src/editor.rs`
- `rust/crates/tiller_ui/src/file_view.rs`
- `rust/crates/tiller_ui/src/right_panel.rs`

## Rows

### `F-CHAT-34` — ledger line 183, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_persistence/src/db.rs, rust/crates/tiller_ui/src/chat.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Root cause: tiller_persistence::db::save_tabs deletes+reinserts every tab row on each session autosave (schedule_save, 28 call sites); chat_turn.tab_id has ON DELETE CASCADE to tab(id), so nearly any ordinary action wipes persisted turns. Live, one unbroken process: chat_turn=1 immediately after a completed real-agent turn; chat_turn=0 after one further ordinary ctl tab.select; Chat History then showed 'No past chats' though the live in-memory transcript still displayed the turn. Builder's own live evidence only covers the empty-state half, never a listed session. sweep D-MAIN-1, 2026-08-15

### `F-CHG-13` — ledger line 204, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/changes.rs, rust/crates/tiller_ui/src/right_panel.rs
- **Wave-D critic evidence (2026-08-15, current tree):** add_changes_tab calls ChangesTab::focus_path synchronously right after ChangesTab::new, but entries starts empty and only populates via an async git_task — focus_path's match against entries always no-ops in real use; the unit test masks this by pumping entries to populate first. Live: two real edits, clicked the Files-panel Diff affordance for note.md — Changes tab opened with both files listed but neither's diff expanded (chg-shots 04-c-diff-clicked.png). sweep D-MAIN-2, 2026-08-15

### `F-CORE-ACT-25` — ledger line 361, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_activity/src/bootstrap.rs, rust/crates/tiller_activity/tests/activity_domain_integration.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Confirmed architectural gap: select_worktree (main.rs) tears down the previous worktree's panes synchronously on every switch (panes.set_external(&old_path, Vec::new())) — exactly one worktree is ever mounted, so there is no concurrently-mounted-worktrees set to order. BootstrapRestoreOrder still has zero callers. No new lane gesture creates this missing concept. sweep D-MAIN-2, 2026-08-15

### `F-CORE-ACT-26` — ledger line 362, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_activity/src/mount.rs, rust/crates/tiller_project/src/settings.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Same confirmed gap as ACT-25 (same source read of select_worktree): WorktreeMountPolicy has zero callers, mounted_worktrees/limit_mounted_worktrees remain persisted-only settings with no eviction consumer anywhere in main.rs. sweep D-MAIN-2, 2026-08-15

### `F-CORE-DOM-07` — ledger line 375, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_project/src/domain.rs, rust/crates/tiller_terminal/src/domain.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Promoted from prior NOT EXERCISED: re-grepped at HEAD, AutoNamingThrottle still has zero callers outside domain.rs/tests, and main.rs has no transcript-driven naming call site anywhere to gate. Same architectural-gap class as ACT-25/26; no lane gesture could reach a feature that isn't built. sweep D-MAIN-2, 2026-08-15

### `F-CORE-FILE-04` — ledger line 389, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_project/src/file_link.rs, rust/crates/tiller_terminal/src/link_router.rs, rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_ui/src/editor.rs, rust/crates/tiller_ui/src/file_view.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Preview mode (the file view's default) renders links via chat.rs render_inline, whose on_click calls cx.open_url() directly, never FileViewEvent::OpenFile; the builder's fix only wires the non-default Code-mode + platform-modifier-click path. Live wayland-drive: opened note.md in default Preview, clicked the rendered link — no new tab, no visible change (f04-shots 02-7-opened.png / 03-8-linkclicked.png). sweep D-MAIN-2, 2026-08-15

