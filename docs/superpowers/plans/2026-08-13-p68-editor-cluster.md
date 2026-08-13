# P68 Editor Cluster Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Exercise the editor rows already present, then complete the remaining P68 editor and file-row behaviours without touching `main.rs` or concurrent owners' files.

**Architecture:** Keep file editing and formatting transformations in `tiller_ui::editor::Editor`, expose the Markdown formatting controls from `FileView`, and keep file-row actions local to `RightPanel` so this cluster does not widen the `main.rs` event seam. Context-menu actions use the existing Linux filesystem helpers and GPUI clipboard boundary; product drag is accepted only if a real file-row source can be exercised end-to-end.

**Tech Stack:** Rust 2024, GPUI, `swift-testing`-style named Rust tests already used by this tree (`#[test]` and `#[gpui::test]`), `tiller_theme::Theme`, `tiller_markdown`.

## Global Constraints

- Work only in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
- Do not edit `rust/crates/tiller/src/main.rs`; codex12 owns its P64 seam.
- Do not edit `tiller_theme`, `settings.rs`, `sidebar.rs`, `status_bar.rs`, `chat.rs`, `tab_bar.rs`, `composer.rs`, `tiller_agents`, or `tiller_control`.
- Use `Theme` tokens for new colours, spacing, and radii; record missing tokens instead of adding literals.
- Do not change ledger verdicts; report stale-FAILED and builder evidence for `pireview`.
- Named drawn tests must use the existing `VisualTestContext`/`TestAppContext` pump pattern and must assert the action, not only the drawn noun.

### Task 1: Confirm existing rows and build state

**Files:**
- Read: `docs/linux-rewrite/tasks/P68-the-editor-cluster-and-three-rows-that-lie.md`
- Read: `docs/linux-rewrite/EVIDENCE-STANDARD.md`
- Read: `rust/crates/tiller_ui/src/editor.rs`
- Read: `rust/crates/tiller_ui/src/file_view.rs`
- Read: `rust/crates/tiller_ui/src/right_panel.rs`

- [ ] Run the exact clippy command extracted from `Scripts/ci-linux.sh`, using the available local cargo path if the shell PATH lacks it.
- [ ] Run `editor::tests` and record the 27 editor behaviours by test name.
- [ ] Attempt the named FileView tests and record unrelated compile blockers without editing their owners' files.
- [ ] Exercise the three stale rows through the existing tests/transcripts: `edit_save_and_reopen_keeps_the_change`, `external_deletion_detects_and_save_recreates_the_file`, and `two_opens_of_the_same_path_yield_one_document`.

### Task 2: Markdown formatting toolbar

**Files:**
- Modify: `rust/crates/tiller_ui/src/file_view.rs`
- Test: `rust/crates/tiller_ui/src/file_view.rs`

**Interface:** `FileView` owns a byte-range `Selection`; clicking a drawn source line selects that line, and `MarkdownFormatOp` remains the public transformation boundary. The toolbar emits Bold, Italic, Heading, List, and Link actions through `FileView::format_markdown`.

- [ ] Add a named drawn test that enters Code mode, clicks a source line, confirms `file-format-toolbar` and all five controls are drawn, then clicks each control and observes the source buffer change.
- [ ] Run that test to expose the missing toolbar/selection surface.
- [ ] Add selection state and line-click selection to `FileView`; preserve the returned selection from each operation.
- [ ] Render the toolbar only for Markdown Code mode, using `Theme` colours/spacing/radii and stable debug selectors.
- [ ] Make the Link control use the existing `MarkdownFormatOp::Link` seam with a deterministic default URL until a text prompt exists; document that limitation in the test/report.
- [ ] Run the named test and the editor model tests.

### Task 3: File-row context menu and copy path

**Files:**
- Modify: `rust/crates/tiller_ui/src/right_panel.rs`
- Read: `rust/crates/tiller_ui/src/editor.rs`
- Test: `rust/crates/tiller_ui/src/right_panel.rs`

**Interface:** RightPanel owns an optional `FileContextMenu { path }`. A right-click opens it; Open reuses `RightPanelEvent::OpenFile`, Reveal runs the Linux `xdg-open` command for the containing directory, and Copy Path writes `fs_actions::copy_path_text` to the GPUI clipboard. No `main.rs` seam is widened.

- [ ] Add a drawn test that right-clicks a real file row and asserts Open, Reveal, and Copy Path are present.
- [ ] Run it to expose the missing context-menu surface.
- [ ] Add context-menu state, right-click handling, and themed menu rows with stable selectors.
- [ ] Wire Open to the existing file-open event, Reveal to the existing `fs_actions::reveal_command`, and Copy Path to GPUI's clipboard boundary.
- [ ] Extend the drawn test to click Open and verify the existing event, and add focused helper assertions for Reveal/Copy Path.
- [ ] Run the named tests and package tests where the concurrent `settings.rs` compile error permits.

### Task 4: Drag evaluation and final verification

**Files:**
- Read/modify only if needed: `rust/crates/tiller_ui/src/right_panel.rs`
- Read: `rust/crates/tiller_terminal/**`, `rust/crates/tiller_git/**`

- [ ] Check whether a real file row has both a GPUI payload source and a product drop target.
- [ ] If both exist, add a real event-path test; otherwise refuse F-EDIT-12 explicitly as harness-only and leave it unbuilt.
- [ ] Run the exact gate invocation and distinguish PATH/toolchain or unrelated owner failures from P68 failures.
- [ ] Update the execution plan status and prepare the ≤12-line report without changing verdict columns.
