# P97 Project Settings Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Connect the existing project-settings card and icon picker to the catalog and SQLite so project name, repository type, icon source, and tint survive closing the card and relaunching Tiller.

**Architecture:** Keep `ProjectIcon` and its serialized pair conversion in `tiller_ui`, carry persistence-neutral project identity fields through the session catalog, and let `SidebarEvent` deliver edits to the app shell. The shell updates the catalog and schedules the existing persistence writer; sidebar rows are updated immediately from the same value.

**Tech Stack:** Rust, GPUI, Cargo workspace, SQLite through `tiller_persistence`, existing `AgentAccentColor` palette.

## Global Constraints

- Read `docs/linux-rewrite/ENVIRONMENT.md` before building or driving the app.
- Do not edit `docs/linux-rewrite/INVENTORY-LEDGER.md`.
- Use the existing schema; do not add a migration.
- Preserve unrelated dirty work and stage only explicit paths; never use `git add -A`.
- Treat GitHub/favicon avatar values as descriptors; do not add network I/O.

### Task 1: Persistable icon conversion

**Files:**
- Modify: `rust/crates/tiller_ui/src/project_identity.rs`
- Test: `rust/crates/tiller_ui/src/project_identity.rs`

- [ ] **Step 1:** Add `ProjectIcon::persisted_parts()` and `ProjectIcon::from_persisted_parts()` covering Symbol, Emoji, LocalPng, GitHub, and Favicon values, with safe defaults for unknown stored values.
- [ ] **Step 2:** Run `cargo test -p tiller_ui project_icon_storage_round_trips_symbol_emoji_and_avatar_variants --lib` and confirm it passes.

### Task 2: Carry identity through catalog and sidebar

**Files:**
- Modify: `rust/crates/tiller/src/session.rs`
- Modify: `rust/crates/tiller_ui/src/sidebar.rs`
- Modify: `rust/crates/tiller/src/main.rs`
- Test: `rust/crates/tiller/src/session.rs`
- Test: `rust/crates/tiller_ui/src/sidebar.rs`

- [ ] **Step 1:** Add catalog fields for optional display name, repository type, icon storage, and color storage, preserving them when restoring and rewriting the catalog.
- [ ] **Step 2:** Add sidebar project presentation fields, initialize rows from them, and render the selected symbol/tint in the project row.
- [ ] **Step 3:** Add project-settings edit events, editable display-name/repository controls, and immediate row updates.
- [ ] **Step 4:** Handle the event in `main.rs`, update the catalog, and schedule the existing catalog save path.
- [ ] **Step 5:** Run the focused UI and session tests.

### Task 3: Verify and commit

**Files:**
- No additional source files.

- [ ] **Step 1:** Run focused package tests and `Scripts/ci.sh` if the shared tree is compilable.
- [ ] **Step 2:** Inspect `git diff --check`, `git status`, and the exact staged path list; verify `INVENTORY-LEDGER.md` is neither edited nor staged.
- [ ] **Step 3:** Commit only the P97 paths with a Conventional Commit message that notes `sidebar.rs` may contain concurrent edits.
