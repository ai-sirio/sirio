# P90 Honest Browser Control Responses Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task with verification checkpoints.

**Goal:** Make every Linux `browser.*` socket method either perform its existing behavior or return an explicit method-specific failure, while advertising only the three supported behaviors.

**Architecture:** Keep the ten browser names in the dispatch set so known-but-unimplemented methods remain distinguishable from unknown methods. Add a separate three-entry capability set, validate unsupported request shapes before queueing, and give queued browser actions the same reply channel as every other UI action. The UI-side browser handler returns protocol pairs or an error and never creates a tab except for `browser.open`.

**Tech Stack:** Rust, GPUI, `tiller_control::ControlResponse`, `std::sync::mpsc::Sender`, existing `BrowserSurface`/`BrowserState`, Rust unit tests in `rust/crates/tiller/src/main.rs`.

## Global Constraints

- Do not implement browser automation in P90.
- Advertise only `browser.open`, URL-based `browser.navigate`, and `browser.act` with the driving flag.
- Keep the seven unsupported methods dispatchable and return explicit errors naming method and reason.
- `browser.open` must require `url` or the existing `address` alias; no `example.com` fallback for control requests.
- Non-`open` browser requests must never create a browser tab when no browser surface exists.
- Do not edit `docs/linux-rewrite/INVENTORY-LEDGER.md`, any F-BRW verdict, or the parallel P91 `rust/crates/tiller_ui/src/chat.rs` change.
- Preserve all unrelated dirty-worktree changes; stage only P90 files in each commit.

---

### Task 1: Rewrite the browser control contract tests

**Files:**
- Modify: `rust/crates/tiller/src/main.rs` near `browser_methods_are_accepted_and_advertised`
- Test: the same Rust test module, using the existing `AppControlHandler` fixture

**Interfaces:**
- Consumes: current `BROWSER_METHODS`, `AppControlHandler::handle`, and capability rows.
- Produces: assertions that lock the honest dispatch/capability contract before implementation.

- [ ] **Step 1: Replace the all-success browser loop with the supported capability list.**

Assert that `system.capabilities` contains exactly these browser entries:

```rust
let advertised: Vec<_> = methods
    .iter()
    .filter_map(|row| row.get("method").map(String::as_str))
    .filter(|method| method.starts_with("browser."))
    .collect();
assert_eq!(advertised, ["browser.open", "browser.navigate", "browser.act"]);
```

- [ ] **Step 2: Assert unsupported browser names fail explicitly without queueing an action.**

For each of `browser.get`, `browser.screenshot`, `browser.snapshot`, `browser.wait`,
`browser.eval`, `browser.console`, and `browser.errors`, call `handler.handle` and assert:

```rust
assert!(!response.ok, "{method} must not report fabricated success");
let error = response.error.as_deref().expect("unsupported error");
assert!(error.contains(method), "error must name {method}: {error}");
assert!(error.contains("unsupported"), "error must explain {method}: {error}");
assert!(actions.lock().expect("action queue").is_empty());
```

- [ ] **Step 3: Assert request-shape errors for supported methods.**

Call `browser.open` without `url`/`address`, `browser.navigate` with `action=reload`, and
`browser.act` with `verb=click`. Each must return `ok: false`, include its method name and a
reason, and leave the action queue empty. This prevents invalid requests from reaching the UI
worker and proves that the old `example.com` fallback is gone.

- [ ] **Step 4: Run the focused test to verify the new assertions fail against the old behavior.**

Run:

```bash
cargo test -p tiller --bin tiller browser_methods_are_accepted_and_advertised -- --nocapture
```

Expected: FAIL because all ten methods are currently advertised and unsupported requests return
`ok: true` after being queued.

---

### Task 2: Implement truthful browser dispatch and replies

**Files:**
- Modify: `rust/crates/tiller/src/main.rs:188-199` for browser method/capability sets
- Modify: `rust/crates/tiller/src/main.rs:279-283` for `ControlAction::Browser`
- Modify: `rust/crates/tiller/src/main.rs:981-1043` for capability generation
- Modify: `rust/crates/tiller/src/main.rs:1683-1700` for validated queued dispatch
- Modify: `rust/crates/tiller/src/main.rs:4558-4646` for browser action results and side-effect guard
- Modify: `rust/crates/tiller/src/main.rs:2673-2675` for reply delivery

**Interfaces:**
- Consumes: the failing contract assertions from Task 1 and existing `BrowserSurface::state`,
  `submit_address`, and `set_agent_driving` APIs.
- Produces: `ControlAction::Browser { method, params, reply }`, a three-entry capability list,
  method-specific validation errors, and result pairs for `browser.open`, `browser.navigate`, and
  `browser.act`.

- [ ] **Step 1: Add a separate advertised capability set and browser reply field.**

Keep all ten names in `BROWSER_METHODS` for known-method dispatch and add:

```rust
const BROWSER_CAPABILITIES: [&str; 3] = [
    "browser.open",
    "browser.navigate",
    "browser.act",
];
```

Add `reply: ControlReply` to `ControlAction::Browser`, then make the worker match arm send the
`Result<Vec<(String, String)>, String>` returned by the workspace handler.

- [ ] **Step 2: Make `system.capabilities` use only the advertised set.**

Remove the ten inline browser entries from the general method array and append/extend the array
with `BROWSER_CAPABILITIES`, preserving the existing row encoding and all non-browser methods.

- [ ] **Step 3: Add pure request validation before queueing.**

Use a small helper in `main.rs` that returns `Option<String>` and applies these exact rules:

```rust
fn browser_request_error(method: &str, params: &BTreeMap<String, String>) -> Option<String> {
    if !BROWSER_CAPABILITIES.contains(&method) {
        return Some(format!(
            "{method} is unsupported on Linux: browser automation is not implemented"
        ));
    }
    match method {
        "browser.open" if params
            .get("url")
            .or_else(|| params.get("address"))
            .is_none_or(|url| url.trim().is_empty()) => {
            Some("browser.open requires a non-empty url".to_string())
        }
        "browser.navigate" if params.contains_key("action") => Some(format!(
            "{method} action is unsupported on Linux: only URL navigation is implemented"
        )),
        "browser.navigate" if params
            .get("url")
            .or_else(|| params.get("address"))
            .or_else(|| params.get("href"))
            .is_none_or(|url| url.trim().is_empty()) => {
            Some("browser.navigate requires a non-empty url".to_string())
        }
        "browser.act"
            if !params.contains_key("driving") && !params.contains_key("agentDriving") => {
            Some(
                "browser.act is unsupported on Linux: only the driving flag is implemented"
                    .to_string(),
            )
        }
        _ => None,
    }
}
```

The browser dispatch arm returns `ControlResponse::failure` for this helper result; otherwise it
uses `self.queue_action` and constructs `ControlAction::Browser` with the reply sender. Thus the
seven stubs and unsupported verbs fail synchronously without touching the workspace.

- [ ] **Step 4: Return browser results and reject implicit surfaces.**

Change `add_browser_tab` to return the new pane/surface identifier and the created browser entity,
using the existing internal pane id in a stable `surface` response value. Change
`handle_browser_action` to return `Result<Vec<(String, String)>, String>`:

```rust
// browser.open: create only here, then read the new surface state.
Ok(vec![
    ("surface".into(), surface_id),
    ("url".into(), surface.state().address().into()),
    ("title".into(), surface.state().page_title().into()),
])
```

For non-`open` methods, return `Err(format!("{method} failed: no browser surface"))` when
`browser_surface()` is absent instead of calling `add_browser_tab`. `browser.navigate` submits its
URL and returns current `url`/`title`; `browser.act` applies the existing driving flag and returns
the resulting `driving` value. The validation helper prevents the seven no-op methods and
unsupported navigate/act verbs from reaching this UI-side match.

- [ ] **Step 5: Run the focused tests and formatter.**

Run:

```bash
cargo fmt --check
cargo test -p tiller --bin tiller browser_methods_are_accepted_and_advertised -- --nocapture
```

Expected: formatter clean and the rewritten contract test passes.

- [ ] **Step 6: Commit the implementation and test rewrite.**

```bash
git add rust/crates/tiller/src/main.rs
git commit -m "fix: make browser control responses honest"
```

The commit message/report must explicitly say that the old all-success assertions were inverted
because they pinned a no-op stub, not because coverage was weakened.

---

### Task 3: Verify the transport contract live

**Files:**
- No source changes expected.
- Evidence: terminal output from the running Linux Tiller socket.

**Interfaces:**
- Consumes: the implementation commit from Task 2 and the active Unix socket selected by
  `$TILLER_SOCKET` or `/run/user/1000/TillerRust/control.sock`.
- Produces: recorded JSON responses proving capabilities, explicit unsupported errors, missing-URL
  rejection, no-surface failure, and the successful `browser.open` result when a controlled app
  instance is available.

- [ ] **Step 1: Identify the active app/socket without changing another agent's workspace.**

Resolve the socket from the environment/default path and inspect the process owning it. If the
existing app is not running the current implementation, start the built app in a controlled
process and use its private socket path.

- [ ] **Step 2: Probe capabilities and unsupported methods with a raw Unix client.**

Send newline-delimited JSON for `system.capabilities`, `browser.eval` with `script=1+1`,
`browser.screenshot` with a temporary path, `browser.navigate` with `action=reload`, and
`browser.act` with `verb=click`. Record that capabilities contains only the three supported names,
each unsupported request has `ok:false` and a method-specific error, and the screenshot path is
not created.

- [ ] **Step 3: Probe missing URL and supported open in a controlled run.**

Send `browser.open` with no params and record the explicit failure. If the controlled app is
available, send `browser.open` with a safe local/HTTP URL and record the returned `surface`, `url`,
and `title`; then send a non-open request before opening in a fresh workspace if possible to prove
it returns `no browser surface` without creating a tab.

- [ ] **Step 4: Run proportional repository verification.**

Run the package test for the tiller binary and `cargo test --workspace` if the focused gate is
green and the shared worktree is not contending for the same build artifacts. Run `Scripts/ci.sh`
only if the user explicitly requests the full repository gate; otherwise report the focused checks
and any environment limitation.

