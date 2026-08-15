# Wave D slice D-MAIN-6 — 7 rows

**Serialised slice — link 6 of 8 in the `D-MAIN` chain.** Earlier links have landed; later links have not started. Nobody else holds `main.rs`. **Re-read the file from disk before editing** — an earlier link changed it since this brief was written.

## Files you own

- `rust/crates/tiller/src/main.rs`

## Rows

### `F-SID-11` — ledger line 80, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** half-proven (unchanged). Same sidebar.rs commit-touch check as F-SID-06 -- none relevant to branch/path/Primary/status/comment display. Standing P106 half-proven (branch/path/Primary/status shown live; no folder-worktree row exists, comment not rendered) stands. sweep C-MAIN-3, 2026-08-15

### `F-SID-12` — ledger line 81, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** half-proven (unchanged). No wave-C commit touched wayland-virtual-pointer.c. Re-confirmed via grep: sidebar.rs:762 'Set Primary' palette entry still requires the same out-of-scope ctrl-shift-p chord as the primary route. No chord-free path exists on this lane. sweep C-MAIN-3, 2026-08-15

### `F-SID-15` — ledger line 84, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** FAILED -- defective (confirmed, unchanged). grep: literal 'Remove Worktree' does not occur anywhere in sidebar.rs -- no menu entry. Hover-x on_click (sidebar.rs:2454-2472) calls remove_worktree_row(row_id,cx) directly with stop_propagation and no confirm state; remove_worktree_row (sidebar.rs:1525) spawns the real remove_worktree (deletes on-disk dir) immediately on the background executor. No wave-C commit touched this code. Destructive, no safety confirmation, no named route -- matches standing verdict exactly. sweep C-MAIN-3, 2026-08-15

### `F-TAB-01` — ledger line 117, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/right_panel.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Wayland lane 2026-08-15: docs expanded (c4c/02) shows linux-rewrite/superpowers/visual-reviews children; click on child 'superpowers' (c4d/02) collapses docs instead, click lands on the row that shifted into that pixel after collapse (Packages). Confirms parent collapses on child click. sweep C-MAIN-4, 2026-08-15

### `F-TAB-08` — ledger line 124, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/settings.rs, rust/crates/tiller_ui/src/tab_bar.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Re-confirmed via source read only 2026-08-15 (unchanged code): render_chat_empty (tab_bar.rs) still lacks .on_click, matches prior click-to-settings-absent finding exactly. sweep C-MAIN-4, 2026-08-15

### `F-TAB-11` — ledger line 127, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/panes.rs, rust/crates/tiller_terminal/src/context_menu.rs, rust/crates/tiller_terminal/src/lib.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** X11 lane 2026-08-15 (x2-rclick.png): right-click renders full 12-item menu (contra prior 'only Copy'). grep 'disabled|reason' in tiller_terminal/src/{lib,context_menu}.rs = zero hits; items() is a flat unconditional const array. Disabled-with-reason capability confirmed absent; menu-emptiness evidence was stale. sweep C-MAIN-4, 2026-08-15

### `F-TAB-13` — ledger line 129, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Re-confirmed via source read 2026-08-15: add_group still #[cfg(test)]-only, 12 active_group() call sites unchanged in main.rs. Group-attach still dead in production. sweep C-MAIN-4, 2026-08-15

