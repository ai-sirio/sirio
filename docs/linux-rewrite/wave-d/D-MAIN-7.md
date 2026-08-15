# Wave D slice D-MAIN-7 — 7 rows

**Serialised slice — link 7 of 8 in the `D-MAIN` chain.** Earlier links have landed; later links have not started. Nobody else holds `main.rs`. **Re-read the file from disk before editing** — an earlier link changed it since this brief was written.

## Files you own

- `rust/crates/tiller/src/main.rs`

## Rows

### `F-TAB-23` — ledger line 139, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/panes.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** X11 lane 2026-08-15: typed MARKER_ORIGINAL_PANE into sole terminal, right-click -> Split Left. New (breadcrumb-less) pane appeared on the right, marker-bearing original stayed left (x4, x5). Confirms Left/Right indistinguishable via the menu route, not just socket. delegated_terminal_context_action/split_focused_inner/render order all read correct on inspection — mismatch unexplained, flagged open. sweep C-MAIN-4, 2026-08-15

### `F-TERM-06` — ledger line 324, currently **half-proven**

- **Files:** rust/crates/tiller_terminal/src/context_menu.rs, rust/crates/tiller_terminal/src/lib.rs, rust/crates/tiller/src/main.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** half-proven (unchanged, independently re-verified): CopyPaneId/CopyTerminalId reachable only via right-click menu (context_menu.rs:13-14,59,64 -> lib.rs:1138-1143 clipboard write); main.rs:2211-2212 confirms no ControlAction path exists. Items visible in p17-rclick-term.png. Click+clipboard-read remains owed, no new drive this pass. sweep C-U, 2026-08-15

### `F-TERM-08` — ledger line 326, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_activity/src/activity.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Re-confirmed via grep 2026-08-15: requires_close_confirmation zero callers, close_focused_pane ungated. State half (ClosePane works) already stood; confirmation half re-confirmed absent. sweep C-MAIN-4, 2026-08-15

### `F-TERM-09` — ledger line 327, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_activity/src/model.rs, rust/crates/tiller_activity/src/title.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Not re-driven this pass; source confirmed unchanged since prior evidence (last touch 2026-08-13, before pass-17 evidence). Carrying prior verdict forward explicitly un-reverified live here. sweep C-MAIN-4, 2026-08-15

### `F-TERM-SPLIT-01` — ledger line 534, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/panes.rs, rust/crates/tiller_terminal/src/lifecycle.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** See F-TAB-23 — identical live evidence, same underlying split_terminal_at_with_placement path. Placement defect confirmed via menu route this pass, not just socket. sweep C-MAIN-4, 2026-08-15

### `F-TERM-UI-01` — ledger line 535, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** X11 lane 2026-08-15: rclick correctly hit-tests the focused terminal and renders all 12 spec'd actions (previously blocked, Wayland has no right-click device at all — confirmed via wayland-virtual-pointer.c hard-coding BTN_LEFT only). Split Left invoked, real effect observed. Not all 12 items individually inspected; verdict stays half-proven but the owed lane-blocker is resolved. sweep C-MAIN-4, 2026-08-15

### `F-USE-06` — ledger line 269, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Not re-driven this pass; register_restored_agent call sites (restore_tabs, restore_tabs_in_workspace) unchanged since prior finding. Carrying prior verdict forward, explicitly un-reverified live here. sweep C-MAIN-4, 2026-08-15

