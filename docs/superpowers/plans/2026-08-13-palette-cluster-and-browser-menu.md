# Palette Cluster and Browser Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make the tiller drawn-test cluster trustworthy and remove the silent New Browser menu action without building the browser surface.

**Architecture:** First isolate the test-harness failure from the palette behavior: the Linux theme portal follower must not start zbus on libtest threads, and palette tests must drive the workspace through a focus path that actually reaches the root shortcut handler. Then keep the browser action reversible by omitting the unsupported menu item and deleting only its dead shell arm.

**Tech Stack:** Rust, GPUI `TestAppContext`/`VisualTestContext`, `tiller_theme`, `tiller_ui::tab_bar`, and the `tiller` shell.

## Global Constraints

- Use the exact gate stage command `cargo test --workspace` for the workspace result.
- Do not build browser chrome or a browser pane while the P72 overlap evidence is unresolved.
- Preserve the typed `NewTabAction::NewBrowser` variant for reversibility unless compilation proves it can be removed safely.
- Classify each broken palette test explicitly as a test-harness/test defect or production-code defect.
- Do not modify unrelated owner files merely to hide gate failures; report the P74 `PlanUpdate`/`ChatEntry::Plan` construction seams separately.

### Task 1: Make the test-harness guard truthful

**Files:**
- Modify: `rust/crates/tiller_theme/src/lib.rs`
- Test: existing theme tests plus a named thread-name regression test

- [ ] Add a failing test proving `tests::...` is recognized as a libtest thread.
- [ ] Run it and observe the expected false result.
- [ ] Accept both `tests::` and nested `::tests::` names in `in_test_harness`.
- [ ] Run the focused test and the four palette/shortcut tests.

### Task 2: Repair only tests that fail before exercising palette behavior

**Files:**
- Modify: `rust/crates/tiller/src/main.rs` test helpers/tests only if the root cause is confirmed.

- [ ] Rebuild the target once the external P74 compile seams are available.
- [ ] Run each palette test through Cargo, not a stale binary.
- [ ] If `open_palette_for_test` is the defect, change the test setup to use a real focused shell surface and assert the shortcut path before dispatch assertions.
- [ ] If production key routing is the defect, add a failing drawn assertion for the root route and fix the shell instead.

### Task 3: Remove the silent unsupported browser menu entry

**Files:**
- Modify: `rust/crates/tiller/src/main.rs`
- Modify: `rust/crates/tiller_ui/src/tab_bar.rs`
- Test: a drawn menu test asserting `new-tab-item-new-browser` is absent.

- [ ] Add the failing drawn test against the production `TabBar` menu.
- [ ] Run it and observe that New Browser is currently present.
- [ ] Stop rendering that menu item and remove the shell no-op arm while retaining the enum variant.
- [ ] Run the drawn test and the existing typed menu dispatch tests, updating only the stale expectation that enumerated the removed unsupported item.

### Task 4: Verify and report

- [ ] Run the focused tiller tests and the exact `cargo test --workspace` stage.
- [ ] Run `cargo fmt --all -- --check` and `git diff --check` on owned files.
- [ ] Report every broken test with the test/code classification, exact unrelated compile blockers, and the reversible browser-menu behavior.
