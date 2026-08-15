# Wave D slice D-P3 — 4 rows

Runs in parallel with every other slice. Every file your rows touch is listed below and no other slice owns any of them.

## Files you own

- `rust/crates/tiller_activity/src/model.rs`
- `rust/crates/tiller_agents/src/codex.rs`
- `rust/crates/tiller_project/src/file.rs`
- `rust/crates/tiller_usage/src/codex.rs`

## Rows

### `F-CORE-ACT-11` — ledger line 347, currently **half-proven**

- **Files:** rust/crates/tiller_activity/src/model.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Real 3-way integration test passes. Live-refuted the builder's spawn-owned blocker: + menu reachable, real Claude Code tab spawns Running and clears to Error on SIGTERM (36/37.png). Process-owned leg fully live-proven separately (ACT-06). Title-owned leg live attempt was inconclusive (input race on a fresh pane). Stays half-proven - simultaneous 3-pane cross-check not completed live. sweep C-P2, 2026-08-15

### `F-CORE-FILE-03` — ledger line 387, currently **FAILED — absent**

- **Files:** rust/crates/tiller_project/src/file.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Code: grepped tiller_ui/tiller_terminal/tiller for .on_drag( outside tests - file rows (right_panel.rs render_file_row) wire only on_mouse_down; only PathBuf .on_drag is a test-only DragFixture, commented as harness-only, product wiring not yet built. Live: used Scripts/linux-drive.sh's new drag helper (e8f72a6, landed post-report) to drag README.md onto a Terminal pane - terminal received no text, only a row-select fired (14a/14-drag-result.png). Tooling gap now closed; feature genuinely absent, not just unexercised. sweep C-P4, 2026-08-15

### `F-CORE-USG-05` — ledger line 403, currently **half-proven**

- **Files:** rust/crates/tiller_usage/src/codex.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Live-confirmed the proactive refresh gate fires for real (log: proactive token refresh failed: Expired) in the same run as USG-06. Merge-on-success proven only via real local-HTTP-fixture unit test, not the live binary (TOKEN_URL not overridable). Stays half-proven. sweep C-P2, 2026-08-15

### `F-CORE-USG-07` — ledger line 405, currently **half-proven**

- **Files:** rust/crates/tiller_agents/src/codex.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** half-proven (unchanged, independently re-verified): Err(_) -> LoggedOut branch exists in tiller_usage/src/codex.rs (line 331, minor drift from cited 319, same branch). Positive-control label pair 'Codex 100% 5h' vs 'Codex logged out' (empty CODEX_HOME) stands proven. Refresh/merge-save-on-success half (shared w/ F-CORE-USG-05) still owed, no new instrument found. sweep C-U, 2026-08-15

