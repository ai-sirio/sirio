# Wave C slice C-P3 — 7 rows

Runs in parallel with other slices. Every file your rows touch is listed below and no other slice owns any of them.

## Files you own

- `rust/crates/tiller/src/session.rs`
- `rust/crates/tiller_agents/src/claude.rs`
- `rust/crates/tiller_agents/src/error.rs`
- `rust/crates/tiller_agents/src/lib.rs`
- `rust/crates/tiller_agents/src/omp.rs`
- `rust/crates/tiller_agents/src/opencode.rs`
- `rust/crates/tiller_agents/tests/adapters_tests.rs`
- `rust/crates/tiller_ui/src/controls.rs`
- `rust/crates/tiller_ui/src/sidebar.rs`

## Rows

### `F-AGENT-API-01` — ledger line 458, currently **half-proven**

- **Files triage named:** rust/crates/tiller_agents/tests/adapters_tests.rs, rust/crates/tiller_agents/src/lib.rs
- **Evidence on record:** Live palette launches confirmed for Codex (host pstree: real -c notify=[...tillerctl...,notify,--session,pane-2,--status,needs-input] argv), Pi (live child process), OpenCode (panel.list agent id 'opencode', tab wired -- but panel.read moments later returned 'unknown pane', a real UnknownPane code path, unresolved); Claude confirmed via always-running ACP. Still owed: Oh-My-Pi (upstream-blocked, see OMP-01) and resume for any of the 4, not driven this pass.

### `F-AGENT-OMP-01` — ledger line 469, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_agents/src/omp.rs
- **Evidence on record:** Independently reconfirmed live: oh-my-pi --version still throws SyntaxError at bin/oh-my-pi.js:176 (un-transpiled TS) before argv is read -- reproduced by a second tester, same line, same failure. Instrument-blocked upstream of Tiller, not FAILED.

### `F-AGENT-OMP-02` — ledger line 470, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_agents/src/omp.rs
- **Evidence on record:** Same shared upstream oh-my-pi TS-parse failure reconfirmed independently (see OMP-01); no session reachable to fire any hook event. Instrument-blocked, not FAILED.

### `F-AGENT-SAFE-01` — ledger line 472, currently **half-proven**

- **Files triage named:** rust/crates/tiller_agents/src/claude.rs, rust/crates/tiller_agents/src/error.rs, rust/crates/tiller_agents/src/omp.rs, rust/crates/tiller_agents/src/opencode.rs, rust/crates/tiller_agents/tests/adapters_tests.rs
- **Evidence on record:** Re-grepped tiller_agents/src and tiller_project/src independently for management.marker, managed_by_tiller, overwrite.*refus, skill.*provision -- zero hits, confirmed. No code path exists to drive; worktree-local half (pass 11) unchanged.

### `F-PRJ-17` — ledger line 110, currently **FAILED — absent**

- **Files triage named:** rust/crates/tiller/src/session.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Evidence on record:** session.rs round-trip proven (test + main.rs's fixed update_project_settings verified by reading the code). Live: New Worktree popover still shows exactly one control (branch-name field only); confirm_worktree_prompt still hard-codes None for the base arg -- reproduces ledger evidence byte-for-byte.

### `F-PRJ-18` — ledger line 111, currently **FAILED — absent**

- **Files triage named:** rust/crates/tiller/src/session.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Evidence on record:** Same popover, same finding -- no location chooser/override control anywhere; sidebar.rs has zero worktree_location_override references, confirm_worktree_prompt still hard-codes None.

### `F-PRJ-13` — ledger line 106, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_ui/src/controls.rs
- **Evidence on record:** **pass 13 superseded** — the picker is mounted in the project settings sheet and both controls act *inside it*: clicking a green swatch re-tinted the selected glyph green, and `Reset` returned it to the orange folder (`orch18-picked.png`, `orch18-reset.png`). **But the choice never leaves the panel** — after `Close` the sidebar project row still shows the orange folder, not the green git-branch (`orch18-sidebar.png`). `on_change(ProjectIcon)` is unwired; see the new seam in `SEAMS.md`. Also: the Colour row is **cli

