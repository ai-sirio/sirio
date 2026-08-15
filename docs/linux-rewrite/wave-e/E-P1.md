# Wave E slice E-P1 — 6 rows

**Scheduling round 5 of 5.** You run alone this round.

Slices in **earlier** rounds have already landed their commits; slices in **later** rounds have not started. So every file below is yours exclusively right now — but **re-read each one from disk before editing**, because an earlier round has changed some of them since this brief was written. Do not trust quoted line numbers.

## Files you own

- `rust/crates/tiller_acp/src/lib.rs`
- `rust/crates/tiller_activity/src/status.rs`
- `rust/crates/tiller_terminal/src/lib.rs`
- `rust/crates/tiller_ui/src/changes.rs`
- `rust/crates/tiller_ui/src/chat.rs`
- `rust/crates/tiller_ui/src/right_panel.rs`

## Rows

### `F-CHAT-05` — ledger line 154, currently **half-proven**

- **Files:** rust/crates/tiller_ui/src/chat.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Guard code (pending_question gating insert_text) re-confirmed unchanged in chat.rs. Two new live attempts against the real claude CLI (Auto and Manual permission modes), asking it to Edit a file outside the worktree, never produced a real Entry::Permission/accept-reject card -- tool calls went Pending to Completed with no human gate. Target condition still unreachable in this sandbox. sweep D-P1, 2026-08-15

### `F-CHAT-20` — ledger line 169, currently **UNREACHABLE**

- **Files:** rust/crates/tiller_ui/src/chat.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Re-confirmed by fresh grep -- zero scroll/axis primitive, same as builder's record. sweep C-CHAT-1, 2026-08-15

### `F-CHAT-33` — ledger line 182, currently **half-proven**

- **Files:** rust/crates/tiller_acp/src/lib.rs, rust/crates/tiller_terminal/src/lib.rs, rust/crates/tiller_ui/src/chat.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Placed a genuinely broken .mcp.json (nonexistent binary command) and ran a real turn against the real claude CLI in a fresh conversation; no mcp-warning card appeared in the transcript. Third independent live attempt (2 prior + this one) with the same negative result -- code remains real/unit-tested but the live trigger stays unreached in this sandbox. .mcp.json removed after test. sweep D-P1, 2026-08-15

### `F-CHG-18` — ledger line 209, currently **NOT EXERCISED**

- **Files:** rust/crates/tiller_terminal/src/lib.rs, rust/crates/tiller_ui/src/changes.rs, rust/crates/tiller_ui/src/right_panel.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Confirmed drag source (changes.rs on_drag) and a REAL drop target in tiller_terminal/src/lib.rs:1551 (on_drop::<(PathBuf,String)>) wired to receive_diff_drop — code is real, not test-only, contradicting a stale comment claiming no target exists. Committed wayland-drive.sh still has no drag verb; did not use the uncommitted local script diff that adds one. sweep D-U, 2026-08-15

### `F-CHG-22` — ledger line 213, currently **half-proven**

- **Files:** rust/crates/tiller_activity/src/status.rs, rust/crates/tiller_ui/src/right_panel.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Drove real notify calls on a live pane id and captured 4 forced-repaint frames: running (orange dot + '1 running'), needs-input ('?'), error ('!' + red dot), done ('✓') — all visually distinct, stronger than prior ambiguous frames. Clicking the collapsed Activity header at 3 plausible y-offsets never expanded it; that half stays unproven. sweep D-U, 2026-08-15

### `F-TERM-UI-02` — ledger line 536, currently **FAILED — defective**

- **Files:** rust/crates/tiller_terminal/src/lib.rs, rust/crates/tiller_ui/src/changes.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Lane gained `modclick`, making this reachable -- overturning the builder's repeated 'no modifier parameter' excuse. Source: on_left_mouse_down (lib.rs:1071) uses event.position directly without subtracting the terminal element's bounds.origin, unlike the paint path nearby. Live: printed a URL in a real terminal, modclick ctrl at its exact on-screen position; panel.list before/after is byte-identical (no new Browser surface), no visible reaction. Independently reproduces the app's real position-math bug in a live multi-pane window. sweep D-P2, 2026-08-15

