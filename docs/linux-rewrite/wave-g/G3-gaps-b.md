# Wave G slice G3-gaps-b — 3 rows

**Scheduling round 3 of 6.** You run alone this round.

Earlier rounds have landed; later rounds have not started. Every file below is yours exclusively right now — but **re-read each from disk before editing**; an earlier round may have changed it since this brief was written, so do not trust quoted line numbers.

## Files you own

- `rust/crates/tiller/src/main.rs`
- `rust/crates/tiller/src/panes.rs`
- `rust/crates/tiller/src/session.rs`
- `rust/crates/tiller_project/src/layout.rs`
- `rust/crates/tiller_terminal/src/context_menu.rs`
- `rust/crates/tiller_terminal/src/lib.rs`
- `rust/crates/tiller_ui/src/chat.rs`
- `rust/crates/tiller_ui/src/editor.rs`
- `rust/crates/tiller_ui/src/file_view.rs`
- `rust/crates/tiller_ui/src/right_panel.rs`
- `rust/crates/tiller_ui/src/tab_bar.rs`

## Rows

### `F-CORE-WSP-04` — ledger line 380, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller_project/src/layout.rs, rust/crates/tiller_terminal/src/lib.rs
- **Latest critic evidence (2026-08-16, current tree):** Fresh grep at HEAD: LayoutCommand/classify_layout_command have callers only inside layout.rs plus a bare lib.rs re-export, zero app callers. Independently corroborated by docs/linux-rewrite/tasks/P78-the-seven-functions-the-app-never-calls.md ('zero app callers, dead enum'). No production code changed; builder's blocked claim holds.

### `F-CORE-WSP-08` — ledger line 384, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller_project/src/layout.rs, rust/crates/tiller_terminal/src/lib.rs, rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_ui/src/editor.rs, rust/crates/tiller_ui/src/file_view.rs
- **Latest critic evidence (2026-08-16, current tree):** Same instrument as WSP-04: WorkspaceTabViewState/WorkspaceTab referenced only in layout.rs/lib.rs re-export. P78 doc confirms independently: session store never persists view_state, so the row's own pass bar (restart survival) can't be met. No production code changed.

### `F-TAB-11` — ledger line 127, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/panes.rs, rust/crates/tiller_terminal/src/context_menu.rs, rust/crates/tiller_terminal/src/lib.rs, rust/crates/tiller_ui/src/right_panel.rs, rust/crates/tiller_ui/src/tab_bar.rs
- **Latest critic evidence (2026-08-16, current tree):** Source-confirmed at HEAD: TerminalContextItem has only label/action/route (no enabled/disabled_reason), ITEMS is a flat const[12], items() takes no params; split_disabled_reason carries #[allow(dead_code)] and its only reference is inside its own #[cfg(test)] module. Feature genuinely absent, matching builder's honest 'blocked' claim.

