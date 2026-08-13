# P28 Persistence Defects Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make application pane layouts round-trip across relaunch, add bounded scrollback persistence plumbing where this worktree owns the session store, and route the renderer-owned capture/replay seam to codex11.

**Architecture:** Keep `PaneNode` unchanged and represent application pane mutations in `tiller/src/main.rs` as a replayable, serializable event history. Store that history alongside each tab in the persistence database, then rebuild the live tree through the existing public `PaneNode` operations. Keep scrollback bounded at 256 KiB per pane; the live PTY/grid capture and replay API belongs in `tiller_terminal/**`, which is owned by codex11 and must be handed off rather than edited here.

**Tech Stack:** Rust 2024, GPUI, rusqlite, serde/serde_json, existing `@test`-style Rust unit/integration tests, headless `tillerctl` smoke tests.

## Global Constraints

- Work only in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`.
- Never copy code from `../_tiller-refs/{waku,zed,orca,t3code}`.
- Do not edit `rust/crates/tiller/src/panes.rs`, `rust/crates/tiller_terminal/**`, or `Scripts/**`; route the renderer capture/replay seam to codex11.
- The persisted scrollback bound is 256 KiB per pane; bytes beyond the bound are discarded from the oldest end.
- Evidence must include before-quit state, a real process exit/relaunch, after-relaunch state, the scrollback nonce, pane ids, and the final `./Scripts/ci-linux.sh` result.

---

### Task 1: Persist and restore application pane mutation history

**Files:**
- Modify: `rust/crates/tiller_persistence/src/model.rs`
- Modify: `rust/crates/tiller_persistence/src/migrations.rs`
- Modify: `rust/crates/tiller_persistence/src/db.rs`
- Modify: `rust/crates/tiller_persistence/src/lib.rs`
- Modify: `rust/crates/tiller/src/session.rs`
- Modify: `rust/crates/tiller/src/main.rs`
- Test: `rust/crates/tiller_persistence/tests/persistence_integration.rs`
- Test: `rust/crates/tiller/src/session.rs`
- Test: `rust/crates/tiller/src/main.rs`

**Interfaces:**
- `tiller_persistence` produces a `TabStateRecord { tab_id, state }` table/API keyed by the existing tab rows.
- `session` produces `SessionTabState` JSON containing a pane event history and restored state aligned with `RestoredSession.tabs`.
- `main` produces and replays `PaneEvent::{Split,SetRatio,Close}` without changing `panes.rs`.

- [ ] **Step 1: Write failing persistence tests** for the v4 tab-state table, its foreign-key cleanup when tabs are replaced, and round-tripping an opaque state string.
- [ ] **Step 2: Run the focused persistence tests** and confirm they fail because the table/API does not exist.
- [ ] **Step 3: Add the v4 migration and minimal `AppDatabase` tab-state read/write API.** Keep tab replacement transactional and ensure old databases migrate with existing rows intact.
- [ ] **Step 4: Run the focused persistence tests** and confirm they pass.
- [ ] **Step 5: Write a failing session test** that schedules a layout with a three-pane event history, dumps the stored state, restores it, and asserts the history and pane ids survive.
- [ ] **Step 6: Add `SessionTabState`, bounded state serialization, and restore alignment** in `session.rs`; preserve empty/default state for old databases.
- [ ] **Step 7: Run the focused session test** and confirm it passes.
- [ ] **Step 8: Add `PaneEvent` state to `OpenTab`, append events after successful split/ratio/close actions, and call `schedule_save()` for those mutations.
- [ ] **Step 9: Restore each tab by replaying its saved events through `PaneNode::split_focused`, `set_ratio`, and `remove`; compute the next pane id above all restored ids.
- [ ] **Step 10: Add a main regression test** for replaying three pane ids and a headless control test transcript proving 3 → 3 across a real quit/relaunch.
- [ ] **Step 11: Run the focused main/session tests and the headless transcript.**

### Task 2: Route renderer-owned scrollback capture/replay

**Files:**
- No edits: `rust/crates/tiller_terminal/**` (codex11-owned)
- Modify only if codex11 hands back the seam: `rust/crates/tiller/src/main.rs`, `rust/crates/tiller/src/session.rs`, and persistence state plumbing from Task 1

**Interfaces:**
- Required handoff API: `TerminalView::capture_scrollback() -> Vec<u8>` returning at most 256 KiB, and a constructor/restore operation that paints persisted bytes into the emulator without writing them to the child process.
- `main` must call capture before terminal shutdown and schedule the returned bytes; restore must apply bytes before the shell is shown.

- [ ] **Step 1: Hand codex11 the exact API requirement and ownership boundary:** capture the alacritty grid/scrollback, cap oldest bytes at 256 KiB, and replay into the emulator rather than feeding bytes to the PTY child.
- [ ] **Step 2: Add a failing integration/regression test at the first seam codex11 returns** using a nonce and asserting capture/replay.
- [ ] **Step 3: Wire capture before `shutdown_terminals`, persist it with the tab state, and replay it in both launch restore paths.
- [ ] **Step 4: Run the nonce test through a real quit/relaunch and record the before/after transcript.

### Task 3: Full verification and evidence report

**Files:**
- Verify: `Scripts/ci-linux.sh`
- Verify: database dump and headless control transcript in the shell output

- [ ] **Step 1: Run the complete workspace tests and inspect exit codes.
- [ ] **Step 2: Run the two P28 entries with a nonce, pane ids before/after, real process exit, relaunch, and store dump.
- [ ] **Step 3: Run `./Scripts/ci-linux.sh` and require `CI OK`.
- [ ] **Step 4: Report within 12 lines, including the routed codex11 seam and any honest remainder.
