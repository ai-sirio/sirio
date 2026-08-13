# P40 Lifecycle Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure every control-owned pane process and its descendants terminate when a panel, workspace, or the application closes, while documenting that `worktree.set` metadata is runtime-only.

**Architecture:** Keep each control pane in its own Unix session, record that session's process-group id beside the direct child pid, and signal the group. `SIGTERM` gets 500 ms; an uncooperative group receives `SIGKILL`, followed by a bounded reap wait. Workspace closure uses a path-scoped registry operation before returning from the existing UI transition.

**Tech Stack:** Rust 2024, libc PTY/session calls, `PaneRegistry`, GPUI workspace control queue, Cargo tests, external `ps`/`kill -0` verification.

## Global Constraints

- All descendants started by a control pane must be included in close and shutdown lifecycle behavior.
- A pane that ignores `SIGTERM` receives `SIGKILL` after the 500 ms grace period.
- `workspace.close` must terminate control panes belonging to the selected worktree, including when it is not the current worktree.
- `worktree.set` comment/session metadata must not be described as persistent unless a database column and round trip exist.
- Do not modify the renderer-owned `tiller_terminal/**` or `tiller/src/panes.rs` for this control-tier fix.

---

### Task 1: Make control-pane termination process-group aware

**Files:**
- Modify: `rust/crates/tiller_control/src/panel.rs`
- Modify: `rust/crates/tiller_control/tests/control_integration.rs`

**Interfaces:**
- Consumes: existing `PaneRegistry::close`, `PaneRegistry::shutdown`, and `setsid()` in `child_exec`.
- Produces: group-aware cleanup for all existing callers, with the same public APIs and errors.

- [ ] **Step 1: Add a failing descendant-lifecycle test.**

Add a real-process integration test beside `pane_registry_shutdown_terminates_live_children` that starts `sleep 60` in the background, writes both the shell pid and descendant pid to a file, calls `registry.close`, and asserts both pids disappear. The test must poll for up to five seconds so it observes the external process state rather than the registry map.

- [ ] **Step 2: Run the focused test and verify it fails for the direct-child reason.**

Run:

```bash
source ~/.cargo/env && cd rust && cargo test -p tiller_control --test control_integration pane_registry_close_terminates_process_group -- --exact --nocapture
```

Expected: the direct shell exits but the background `sleep` remains alive, proving the test catches the lifecycle leak.

- [ ] **Step 3: Store and signal the pane's process group.**

Add a `process_group` field to `PaneProcess`, initialize it to the child pid after `child_exec` has called `setsid()`, and change `terminate` to signal the group with `killpg`: `SIGTERM`, wait `TERMINATE_GRACE` (500 ms), then `SIGKILL` and wait again. Add comments stating the escalation contract and the limitation that descendants which create a new session are outside this group.

- [ ] **Step 4: Run the focused test and the control crate tests.**

Run:

```bash
source ~/.cargo/env && cd rust && cargo test -p tiller_control --test control_integration pane_registry_close_terminates_process_group -- --exact --nocapture
source ~/.cargo/env && cd rust && cargo test -p tiller_control
```

Expected: the new test and the complete `tiller_control` package pass.

### Task 2: Close panes with their workspace and correct the metadata contract

**Files:**
- Modify: `rust/crates/tiller_control/src/panel.rs`
- Modify: `rust/crates/tiller/src/main.rs`
- Modify: `rust/crates/tiller_control/src/protocol.rs`
- Modify: `rust/crates/tiller_control/tests/control_integration.rs`

**Interfaces:**
- Consumes: `PaneRegistry::shutdown_for(&Path)`, `Workspace::close_workspace`, and `protocol::request::worktree_set`.
- Produces: path-scoped cleanup invoked by `workspace.close`; no database/schema changes.

- [ ] **Step 1: Add a failing path-scoped cleanup test.**

Create two real control panes in two temporary directories, call `shutdown_for` for only the first directory, and assert the first pane and its descendant are gone while the second pane remains listed/alive. This locks the selector boundary before wiring the app call.

- [ ] **Step 2: Run the focused test and verify it fails because the method is absent.**

Run:

```bash
source ~/.cargo/env && cd rust && cargo test -p tiller_control --test control_integration pane_registry_shutdown_for_only_its_worktree -- --exact --nocapture
```

Expected: compilation fails because `PaneRegistry::shutdown_for` does not exist.

- [ ] **Step 3: Implement `shutdown_for` and call it from workspace close.**

Remove matching control entries, mark them closed, clear the active id only when it belongs to the removed set, terminate each process group, and return any path validation error. In `Workspace::close_workspace`, call it for the resolved path for both current and non-current worktrees; retain the existing external-pane clearing for the current renderer workspace.

- [ ] **Step 4: Make `worktree.set` documentation honest.**

Change the request builder documentation and the in-memory control-state documentation to say that comment/session values are runtime metadata rebuilt empty on restart. Do not add a DB column because the task's accepted fix is to correct the persistence claim.

- [ ] **Step 5: Run focused tests and compile the application.**

Run:

```bash
source ~/.cargo/env && cd rust && cargo test -p tiller_control
source ~/.cargo/env && cd rust && cargo test -p tiller --bin tiller tests::control_state_can_associate_a_session_and_comment_with_a_worktree
source ~/.cargo/env && cd rust && cargo build -p tiller
```

Expected: all commands exit successfully; existing behavior still reports the metadata in the live control response.

### Task 3: Verify from outside the app and report the honest remainder

**Files:**
- No additional source files; use the built binary and existing scripts.

**Interfaces:**
- Consumes: `tillerctl panel create/close`, `tillerctl close-workspace`, `tillerctl quit`, and OS process inspection.
- Produces: PID lists before and after `panel.close`, `workspace.close`, and real app quit.

- [ ] **Step 1: Build and launch headless with an isolated socket.**

Use `source ~/.cargo/env`, `TILLER_SOCKET=/tmp/codex12.sock`, and `env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller` as specified by the task brief.

- [ ] **Step 2: Exercise a compound command and record all group pids.**

Create a pane running `sleep 60 & printf '%s %s' "$$" "$!" > /tmp/p40-pids; wait`, record the pid list with `ps`, close it, and record `ps`/`kill -0` after `panel.close`. Repeat with a pane in a workspace that is closed by `workspace.close`, then create one more and quit the app for real.

- [ ] **Step 3: Run `./Scripts/ci-linux.sh` and inspect the final diff.**

Report the exact PID lists, the 500 ms `SIGTERM` then `SIGKILL` policy, the runtime-only `worktree.set` decision, and any pre-existing gate failure or manual limitation in 12 lines or fewer.
