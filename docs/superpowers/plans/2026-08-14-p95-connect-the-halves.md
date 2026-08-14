# P95 connect the halves that already exist Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Connect the existing Linux UI, terminal, file-drop, and file-monitor seams to their host behavior, and make the file context menu use the pointer position.

**Architecture:** Keep routing at the owning entity boundary. Settings queues a host action into the workspace, each terminal subscribes to its own `TerminalLinkEvent`, and each `FileView` owns the inotify monitor for its path so external changes reach the existing editor conflict/reload model without a global file watcher. The existing Changes-list drag payload remains `(PathBuf, String)` and the terminal's existing drop receiver remains the live path.

**Tech Stack:** Rust, GPUI, inotify, tiller_ui, tiller_terminal, tiller_markdown, tiller_project.

## Global Constraints

- Preserve unrelated dirty work in the shared `linux/gpui-waku` worktree.
- Do not edit `docs/linux-rewrite/INVENTORY-LEDGER.md`.
- Commit only explicit paths; never use `git add -A`.
- Prefer focused crate tests while other agents share the Cargo target.
- Live display work must hold `/tmp/tiller-drive-1.lockd`; XDND file drops are not exercisable by the documented X harness.

---

### Task 1: Wire the General Settings skill installer

**Files:**
- Modify: `rust/crates/tiller/src/main.rs`
- Test: `rust/crates/tiller/src/main.rs` unit tests

**Interfaces:**
- Consumes: `Settings::on_install_skill(SkillInstallCommand)` and `tiller_project::agent_skill_install_command()`.
- Produces: a queued `WorkspaceAction::InstallSkill` handled by the workspace as a terminal command.

- [ ] **Step 1: Write the failing test** for converting the provisioned command to the terminal shell contract.
- [ ] **Step 2: Run the focused test and confirm it fails because the host helper/action is absent.**
- [ ] **Step 3: Add the queue action, Settings callback, and terminal-tab handler using the existing login-shell convention.
- [ ] **Step 4: Run the focused test and the relevant package test.

### Task 2: Route terminal hyperlinks through the owning pane

**Files:**
- Modify: `rust/crates/tiller/src/main.rs`
- Test: `rust/crates/tiller/src/main.rs` unit tests

**Interfaces:**
- Consumes: `TerminalLinkEvent { target, url }` emitted by each `TerminalView`.
- Produces: a per-terminal subscription that calls GPUI’s URL opener only when the event target matches that pane.

- [ ] **Step 1: Write the failing target-routing test.
- [ ] **Step 2: Run it and confirm the missing helper fails.
- [ ] **Step 3: Subscribe each bound terminal and open the URL through its owning workspace route.
- [ ] **Step 4: Run the focused test and terminal/host package tests.

### Task 3: Keep the existing file-drop path connected

**Files:**
- Inspect only: `rust/crates/tiller_project/src/file.rs`, `rust/crates/tiller_terminal/src/lib.rs`, `rust/crates/tiller_ui/src/changes.rs`, `rust/crates/tiller_ui/src/right_panel.rs`

**Interfaces:**
- Consumes: `(PathBuf, String)` from Changes and `PathBuf` from file drops.
- Produces: no code change if the current production path remains connected; report the live source surface and the unexercisable XDND boundary.

- [ ] **Step 1: Trace both source surfaces and confirm the production receiver calls `terminal_file_drop`.
- [ ] **Step 2: Run focused project and terminal tests.
- [ ] **Step 3: Exercise the reachable path if the display harness can perform it; otherwise record the documented XDND limitation.

### Task 4: Connect inotify to editor reload/conflict state

**Files:**
- Modify: `rust/crates/tiller_markdown/src/file_events.rs`
- Modify: `rust/crates/tiller_ui/src/file_view.rs`
- Test: `rust/crates/tiller_ui/src/file_view.rs`, `rust/crates/tiller_markdown/src/file_events.rs`

**Interfaces:**
- Consumes: `FileSystemEventMonitor` events for the file view’s path.
- Produces: watcher-driven calls to the existing `Editor::check_external`, including clean auto-reload and dirty/deleted/renamed conflict banners.

- [ ] **Step 1: Add a failing FileView test for a watcher event updating clean content and surfacing deletion/rename.
- [ ] **Step 2: Run the focused test and confirm the event handler is absent.
- [ ] **Step 3: Give `FileView` an inotify monitor/poll task and route matching events through `check_external`; allow a missing file to watch its parent directory.
- [ ] **Step 4: Run focused markdown and UI tests.

### Task 5: Place the Files context menu at the pointer

**Files:**
- Modify: `rust/crates/tiller_ui/src/right_panel.rs`
- Test: `rust/crates/tiller_ui/src/right_panel.rs`

**Interfaces:**
- Consumes: GPUI right-click `event.position`.
- Produces: `FileContextMenu { path, position }` rendered with `.left(position.x)` and `.top(position.y)`.

- [ ] **Step 1: Add a failing drawn test asserting the menu origin equals the right-click position.
- [ ] **Step 2: Run it and confirm the current hardcoded origin fails.
- [ ] **Step 3: Store and render the pointer position.
- [ ] **Step 4: Run the focused right-panel test.

### Final verification

- [ ] Run `cargo fmt --check` or format only touched Rust files.
- [ ] Run focused tests for `tiller_project`, `tiller_markdown`, `tiller_terminal`, `tiller_ui`, and `tiller`.
- [ ] Run `git diff --check` and inspect the path-scoped diff.
- [ ] Run `git status --short | grep '??'` before finishing.
- [ ] Hold the documented display lock, launch the app, observe the skill-install and terminal-link routes plus the pointer-positioned menu; document XDND as unexercised if the harness cannot negotiate it.
