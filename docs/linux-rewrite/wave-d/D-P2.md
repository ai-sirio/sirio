# Wave D slice D-P2 — 7 rows

Runs in parallel with every other slice. Every file your rows touch is listed below and no other slice owns any of them.

## Files you own

- `rust/crates/tiller_agents/src/claude.rs`
- `rust/crates/tiller_terminal/src/lib.rs`
- `rust/crates/tiller_ui/src/changes.rs`
- `rust/crates/tiller_ui/src/settings.rs`
- `rust/crates/tiller_usage/src/claude.rs`
- `rust/crates/tiller_usage/src/model.rs`

## Rows

### `F-EDIT-12` — ledger line 230, currently **NOT EXERCISED**

- **Files:** rust/crates/tiller_terminal/src/lib.rs, rust/crates/tiller_ui/src/changes.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Critic re-ran a_drawn_change_row_drags_its_diff_payload_to_a_drop_target (1 passed); real ChangesTab over a real git repo, real MouseDown/Move/Up onto a DiffDropTargetFixture (stand-in only because tiller_ui cannot depend on tiller_terminal). No drag primitive of any kind exists in wayland-drive.sh, so un-driveable live on this lane. sweep C-P1, 2026-08-15

### `F-CORE-TERM-02` — ledger line 395, currently **half-proven**

- **Files:** rust/crates/tiller_terminal/src/lib.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Critic re-ran shift_f10_opens_the_context_menu_from_the_keyboard (1 passed) and read it: real TerminalView with live PTY, real simulate_click focus, keystrokes driven through actual on_key_down dispatch, asserts terminal-context-item-0 drawn. Live chord instrument-blocked: wayland-drive.sh key action wraps wtype -k for one named key with no modifier composition. sweep C-P1, 2026-08-15

### `F-TERM-PTY-06` — ledger line 531, currently **half-proven**

- **Files:** rust/crates/tiller_terminal/src/lib.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Critic re-ran a_drawn_terminal_accepts_a_real_external_paths_drop_with_several_files (1 passed) and confirmed no code changed. on_drop::<ExternalPaths> wired on the running-terminal branch; ENVIRONMENT.md records XDND as out of reach of the harness on any lane, broader than this Wayland lane. sweep C-P1, 2026-08-15

### `F-TERM-UI-02` — ledger line 536, currently **NOT EXERCISED**

- **Files:** rust/crates/tiller_terminal/src/lib.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Critic re-ran platform_modifier_click_opens_a_terminal_link (1 passed) and read it: real PTY prints a URL, two real simulate_mouse_down at the link's actual position, Modifiers::none asserts no event and platform:true asserts the exact URL. wayland-drive.sh click has no modifier parameter. sweep C-P1, 2026-08-15

### `F-USE-03` — ledger line 266, currently **half-proven**

- **Files:** rust/crates/tiller_usage/src/claude.rs, rust/crates/tiller_usage/src/model.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Live-captured the genuine Loading state ('Claude ...' / 'Codex ...') persisting for seconds on a fresh launch, then its real transition to Loaded/Unavailable text. Frames fresh3-04-crop.png, fresh3-05-crop.png. Stale-after-timeout half still not attempted. Stays half-proven, stronger footing. sweep C-P2, 2026-08-15

### `F-SET-11` — ledger line 299, currently **half-proven**

- **Files:** rust/crates/tiller_agents/src/claude.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** half-proven (unchanged): Claude's 3 remaining states blocked by claude.rs:397 dotfile re-sourcing, confirmed live-code, no synthetic instrument reaches it. Codex half was mislabeled as blocked in an earlier record; CodexUsageFetcher is HTTP+auth.json-based and already proven reachable via CODEX_HOME isolation for F-CORE-USG-05/07 this sweep -- correction preserved. sweep C-U, 2026-08-15

### `F-SET-20` — ledger line 308, currently **FAILED — defective**

- **Files:** rust/crates/tiller_ui/src/settings.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Live: toggled Translucency ON (visually confirmed via screenshot). ctl surface.settings.select payload has zero translucency key before AND after -- settings_report_pairs (main.rs) never copies snapshot.translucency into its hand-built values map. FAILED — defective, root cause pinpointed to exact function/lines. sweep C-CHAT-3, 2026-08-15

