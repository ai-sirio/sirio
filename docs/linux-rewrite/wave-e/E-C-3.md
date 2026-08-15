# Wave E slice E-C-3 — 6 rows

**Scheduling round 1 of 5.** You run alone this round.

Slices in **earlier** rounds have already landed their commits; slices in **later** rounds have not started. So every file below is yours exclusively right now — but **re-read each one from disk before editing**, because an earlier round has changed some of them since this brief was written. Do not trust quoted line numbers.

## Files you own

- `rust/crates/tiller/src/main.rs`
- `rust/crates/tiller/src/session.rs`
- `rust/crates/tiller_control/src/client.rs`
- `rust/crates/tiller_persistence/src/db.rs`
- `rust/crates/tiller_project/src/settings.rs`
- `rust/crates/tiller_ui/src/browser.rs`
- `rust/crates/tiller_ui/src/editor.rs`
- `rust/crates/tiller_ui/src/right_panel.rs`
- `rust/crates/tiller_ui/src/sidebar.rs`
- `rust/crates/tiller_ui/src/status_bar.rs`

## Rows

### `F-EDIT-08` — ledger line 226, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_control/src/client.rs, rust/crates/tiller_ui/src/editor.rs, rust/crates/tiller_ui/src/right_panel.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Independently inspected the builder's own capture PNGs (not their prose): real distinct CLAUDE.md tab, no duplicate on re-click, matches sound click_count>=2 dedup logic in source. My own 3 live reproduction attempts (3 and 5 rapid clicks, measured 215-280ms intervals) failed to open a tab, and later attempts returned blank frames under confirmed severe machine overload (load avg 91-101/12 cores) -- cannot separate gesture-timing confound from a real defect. sweep D-MAIN-4, 2026-08-15

### `F-PRJ-03` — ledger line 96, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Code+own-run test confirm the 3-button prompt is a real in-window GPUI overlay (Wayland platform prompt() returns None, falls to build_custom_prompt), correctly gated on .git existence; live drive proved the git-checkout happy path (zero-prompt add) end to end. Could not steer this lane's real xdg-desktop-portal picker to a non-git folder (it silently resolves to the app's own cwd, a git repo) so the non-git 3-button branch was not seen live. sweep D-MAIN-5, 2026-08-15

### `F-SET-04` — ledger line 292, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_persistence/src/db.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Independently traced both restore call sites myself (main.rs:9440-9442 and :4275-4277): the gate is real and unconditional, feeding an empty BTreeMap into restore_tabs/restore_tabs_in_workspace when the setting is off. Did not attempt a live native-agent-resume restart (would need a fabricated on-disk Claude/Codex session); code-level proof only. sweep D-MAIN-5, 2026-08-15

### `F-SET-10` — ledger line 298, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/status_bar.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Read status_bar.rs myself: on_refresh_clicked unconditionally sets all four provider states to Loading and cx.notify()s synchronously before spawning real background fetches. Live click reached the real handler without incident, but the transient Loading pixels were never caught -- every forced-repaint capture round trip outlasted the fetch under this session's severe ~20-instance shared-machine contention. sweep D-MAIN-5, 2026-08-15

### `F-SET-24` — ledger line 312, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_project/src/settings.rs, rust/crates/tiller_ui/src/browser.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Re-drove empty state live (fresh capture, genuine). Sharper defect: grep shows request_permission has zero production callers anywhere in crates/tiller/src — only revoke is wired from main.rs. The grant arm is unreachable code on any lane, not merely blocked by the X11/file-dialog gap previously recorded. sweep D-U, 2026-08-15

### `F-SID-11` — ledger line 80, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Folder-worktree row confirmed live: adding a plain non-git folder now renders a real worktree row with path + Primary badge (previously absent). Comment confirmed NOT working, live and after a genuine process restart against the same DB: ctl worktree.set comment=... returns ok but the comment never appears on the row under any condition tested; main.rs's handler never pushes it into the materialized Sidebar entity. sweep D-MAIN-6, 2026-08-15

