# P100 notification seams Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task with verification checkpoints.

**Goal:** Make explicit control-socket notifications reach the existing desktop notifier and preserve restored agent identity in the live activity model.

**Architecture:** Keep `post_desktop_notification` as the only desktop backend. Route `record_notification` through it after the in-memory row is recorded, using an injectable poster only at the handler seam for a focused regression test. Pass one `AgentActivityModel` through both restore builders and register each restored tab's root pane identity without assigning a status.

**Tech Stack:** Rust 2024, GPUI test support, `tiller_activity`, `tiller_control`, `notify-send`, D-Bus session bus.

## Global Constraints

- `notification.create` is an explicit request and bypasses `NotificationPolicy` transition suppression.
- A failed `notify-send` spawn is logged and does not fail the socket response or panic the app.
- Both `restore_tabs` and `restore_tabs_in_workspace` must wire restored agent identity.
- Do not edit `INVENTORY-LEDGER.md`.
- Stage and commit only owned paths; never use `git add -A`.

---

### Task 1: Record a failing control-notification delivery test

**Files:**
- Modify: `rust/crates/tiller/src/main.rs` in `AppControlHandler` and its `#[cfg(test)]` module

**Interfaces:**
- Consumes: `ControlRequest`, `AppControlHandler::handle`, `NotificationPayload`.
- Produces: A test-only configurable notification poster that can observe the exact payload sent by `notification.create`.

- [ ] **Step 1: Add a poster field and test helper shape in the test first**

Add a regression test that constructs the existing handler, installs a poster collecting `NotificationPayload`, sends `notification.create` with title `Build finished` and body `The Linux build is ready.`, and asserts the collected payload has those exact values.

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
/home/enzopalmisano/.cargo/bin/cargo test -p tiller --bin tiller control_notification_create_posts_exact_title_and_body -- --exact --nocapture
```

Expected: the test cannot observe a posted payload because `record_notification` only pushes the in-memory row.

- [ ] **Step 3: Commit the failing-test change only if the repository permits an intermediate commit**

Use a path-scoped commit after confirming the failure; do not stage unrelated worktree files.

### Task 2: Route `notification.create` through the existing notifier

**Files:**
- Modify: `rust/crates/tiller/src/main.rs` in `AppControlHandler::record_notification` and constructor defaults

**Interfaces:**
- Consumes: the test poster seam from Task 1 and the existing `post_desktop_notification` function.
- Produces: `record_notification` stores the row and invokes the configured poster with a `NotificationPayload` containing the request title/body.

- [ ] **Step 1: Add the production default poster**

Initialize the handler's poster to `post_desktop_notification`; keep the real `notify-send` function unchanged.

- [ ] **Step 2: Invoke it after storing the row**

Construct `NotificationPayload { pane_id: String::new(), worktree_id: String::new(), title: title.to_string(), body: body.to_string() }`, release the notification-store lock, invoke the poster, and return the existing success response. Do not route this explicit request through `NotificationPolicy`.

- [ ] **Step 3: Run the focused test and verify GREEN**

Run:

```bash
/home/enzopalmisano/.cargo/bin/cargo test -p tiller --bin tiller control_notification_create_posts_exact_title_and_body -- --exact --nocapture
```

Expected: PASS, with the test observing the exact title and body.

### Task 3: Add a failing restore-registration regression test

**Files:**
- Modify: `rust/crates/tiller/src/main.rs` in restore test setup and restoration helpers

**Interfaces:**
- Consumes: `RestoredSession`, `AgentActivityModel`, `restore_tabs`, and `restore_tabs_in_workspace`.
- Produces: A shared restore-registration helper used by both restore builders.

- [ ] **Step 1: Update the restore test to supply an activity model and assert identity**

Use a restored chat tab with `agent_id: Some("codex")`, a root pane id of `7`, and assert after `restore_tabs` that `activity.agent_id("pane-7") == Some("codex")` and `activity.status("pane-7") == None`.

- [ ] **Step 2: Run the focused restore test and verify RED**

Run:

```bash
/home/enzopalmisano/.cargo/bin/cargo test -p tiller --bin tiller restore_tabs_registers_restored_agent_identity -- --exact --nocapture
```

Expected: FAIL because the current restore builder only copies the tab's icon and `OpenTab.agent_id`.

### Task 4: Wire both restore paths without claiming running status

**Files:**
- Modify: `rust/crates/tiller/src/main.rs` in `restore_tabs`, `restore_tabs_in_workspace`, `TillerWorkspace::new`, the launch closure, and restore test callers

**Interfaces:**
- Consumes: `&mut AgentActivityModel` in both restoration builders.
- Produces: launch-time activity state populated before workspace creation; additive restore registration in the existing workspace model.

- [ ] **Step 1: Add one helper for restored root-pane identity**

Create a helper that accepts `&mut AgentActivityModel`, a root pane id, and `Option<&str>` agent id; when the id exists, call `register_agent_id(&format!("pane-{root_id}"), agent_id)` and never call `agent_spawned`.

- [ ] **Step 2: Call the helper from both restore builders**

Pass the activity model into `restore_tabs` and `restore_tabs_in_workspace`, and call the helper after each builder resolves the tab's root pane id and agent id.

- [ ] **Step 3: Transfer the launch model into `TillerWorkspace`**

Create `AgentActivityModel::new()` before the initial `restore_tabs` call, pass it into the builder, then pass that populated model into `TillerWorkspace::new`. Use `&mut self.activity` for additive `restore_tabs_in_workspace`.

- [ ] **Step 4: Update existing restore tests and constructor fixtures**

Supply an `AgentActivityModel::new()` to every direct restore-builder call and constructor fixture, preserving existing test behavior for tabs without an agent id.

- [ ] **Step 5: Run focused restore tests and verify GREEN**

Run:

```bash
/home/enzopalmisano/.cargo/bin/cargo test -p tiller --bin tiller restore_tabs_registers_restored_agent_identity -- --exact --nocapture
/home/enzopalmisano/.cargo/bin/cargo test -p tiller_activity
```

Expected: PASS, including no `.running` status immediately after restore.

### Task 5: Verify runtime delivery, restart behavior, and repository gates

**Files:**
- No additional production files; use `/tmp` capture files and a private control socket/database.

**Interfaces:**
- Consumes: the built `tiller` and `tillerctl` binaries, `dbus-monitor`, and the control socket.
- Produces: reproducible runtime evidence for delivery and suppression.

- [ ] **Step 1: Run focused package tests**

```bash
/home/enzopalmisano/.cargo/bin/cargo test -p tiller --bin tiller
/home/enzopalmisano/.cargo/bin/cargo test -p tiller_activity
```

- [ ] **Step 2: Capture the D-Bus `Notify` call**

Run a private headless app with `/tmp/p100.sqlite` and `/tmp/p100.sock`, capture `dbus-monitor --session "interface='org.freedesktop.Notifications',member='Notify'"`, issue `notification.create` with a unique title/body through `tillerctl` or the socket, and retain output showing the title/body on the wire. Confirm `notification.list` still contains the row.

- [ ] **Step 3: Exercise a real restart with a restored agent in a background tab**

Launch the app with a private database, create an agent tab and a second tab, move the agent tab to the background, quit the process, relaunch with the same database, and send a status transition to the restored pane. Confirm the notification arrives. Then select the agent tab and repeat a transition that should be suppressed by `app_active && pane_visible`; record both outcomes.

- [ ] **Step 4: Run the repository verification gate**

```bash
Scripts/ci.sh
```

Expected: exit 0 and `CI OK`; attribute any unrelated concurrent-file failure before changing files outside this task.

- [ ] **Step 5: Review and commit path-scoped changes**

Inspect `git diff --check`, `git diff --stat`, and `git status --short`; ensure `INVENTORY-LEDGER.md` is untouched. Stage only the design/plan and owned source paths, then commit with a Conventional Commit message that notes `main.rs` may contain concurrent edits.
