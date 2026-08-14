# Wave B integration report

Integrator pass over the seven builder slices (B1-tabbar-zorder, B2-chat, B3-sidebar,
B4-settings, B5-acp, B6-brw-bar, B7-editor-persist) landed in the shared worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux` on `linux/gpui-waku`.

## 1. `cargo build -p tiller`

**First attempt: failed.**

```
error[E0063]: missing fields `default_worktree_base` and `worktree_location_override` in initializer of `CatalogProjectSettings`
   --> crates/tiller/src/main.rs:2979:24
    |
2979 |         let settings = CatalogProjectSettings {
     |                        ^^^^^^^^^^^^^^^^^^^^^^ missing `default_worktree_base` and `worktree_location_override`
```

B7-editor-persist (`crates/tiller/src/session.rs`, F-PRJ-17/F-PRJ-18) widened
`CatalogProjectSettings` with two new fields but does not own `crates/tiller/src/main.rs`
(owned by B1-tabbar-zorder this wave) and flagged the exact break and fix in its wave
report's "wanted foreign files" list: `update_project_settings`'s struct literal
(~line 2977/2979) needed the two new fields threaded through, sourced from the project's
*existing* settings rather than hard-coded `None` (which would silently wipe any
user-set worktree base/override on the next unrelated icon/color/name edit).

**Fix applied** (mechanical, matches B7's prescribed fix exactly — not a redesign):
`update_project_settings` in `crates/tiller/src/main.rs` now reads
`self.project_catalog.project_settings(&update.id)` before building the new
`CatalogProjectSettings` literal, and carries `default_worktree_base` /
`worktree_location_override` forward from that read instead of omitting them.
Commit `f74334b`.

**Second attempt, after the fix:**

```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.88s
```

`cargo build -p tiller` **succeeds.** (Only pre-existing warning: `pump_task` field never
read in `tiller_ui/src/browser.rs`, part of B6's slice, not a build failure.)

`cargo build --workspace` also verified clean afterward (all crates and binaries, not just
`tiller`).

## 2. `cargo test --workspace --no-run`

Linked every test binary in the workspace with no compile errors, after the fix above:

```
Compiling ... (all crates)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1m 15s
  Executable unittests src/main.rs (target/debug/deps/tiller-2528a5008a75e1e6)
  Executable unittests src/lib.rs (target/debug/deps/tiller_acp-aa6502ddb5e97210)
  Executable tests/acp_integration.rs (...)
  Executable tests/chat_integration.rs (...)
  Executable tests/real_claude.rs (...)
  Executable unittests src/lib.rs (target/debug/deps/tiller_activity-...)
  Executable tests/activity_domain_integration.rs (...)
  Executable tests/activity_integration.rs (...)
  Executable unittests src/lib.rs (target/debug/deps/tiller_agents-...)
  Executable tests/adapters_tests.rs (...)
  Executable tests/home_isolation.rs (...)
  Executable tests/p99_session_rows.rs (...)
  Executable tests/session_sources.rs (...)
  Executable unittests src/lib.rs (target/debug/deps/tiller_control-...)
  Executable unittests src/bin/tillerctl.rs (...)
  Executable tests/control_integration.rs (...)
  Executable unittests src/lib.rs (target/debug/deps/tiller_git-...)
  Executable tests/git_integration.rs (...)
  Executable tests/p41_git_behaviors.rs (...)
  Executable tests/p99_git_rows.rs (...)
  Executable tests/worktree_integration.rs (...)
  Executable unittests src/lib.rs (target/debug/deps/tiller_markdown-...)
  Executable tests/tree_tests.rs (...)
  Executable unittests src/lib.rs (target/debug/deps/tiller_persistence-...)
  Executable tests/persistence_integration.rs (...)
  Executable unittests src/lib.rs (target/debug/deps/tiller_project-...)
  Executable tests/discovery_integration.rs (...)
  Executable tests/p99_naming_throttle.rs (...)
  Executable unittests src/lib.rs (target/debug/deps/tiller_terminal-...)
  Executable unittests src/lib.rs (target/debug/deps/tiller_theme-...)
  Executable unittests src/lib.rs (target/debug/deps/tiller_ui-...)
  Executable unittests src/bin/changes_preview.rs (...)
  Executable unittests src/lib.rs (target/debug/deps/tiller_usage-...)
  Executable tests/p99_account_identity.rs (...)
  Executable tests/p99_codex_locations.rs (...)
  Executable tests/usage_tests.rs (...)
```

No verifier will hit a compile error driving any of these binaries.

## 3. Per-crate test runs (verbatim summary lines)

Ran isolated (`cargo test -p <crate> ...`), not the noisy full-workspace concurrent run —
see the flakiness note at the bottom for why.

**`tiller` (bin, owns B1's slice + all wave surfaces exercised end-to-end):**
```
test result: ok. 142 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 46.02s
```

**`tiller_ui --lib` (B2-chat, B3-sidebar, B4-settings' UI half, B6-brw-bar, B7's UI half — re-run 4 times for stability, always clean):**
```
test result: ok. 284 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.02s
```

**`tiller_persistence --lib`:**
```
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**`tiller_persistence --test persistence_integration`** (after the fix described below):
```
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.58s
```

**`tiller_acp --lib`:**
```
test result: ok. 22 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.01s
```

**`tiller_acp` (all targets — lib, acp_integration, chat_integration, real_claude, doctests):**
```
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s   (acp_integration)
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s    (chat_integration)
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s    (real_claude, network-dependent, pre-existing skip)
```

**`tiller_usage` (all targets):**
```
test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s   (lib, incl. codex::)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s    (p99_account_identity)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s    (p99_codex_locations)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.27s    (usage_tests)
```

**`tiller_terminal` (all targets, B7's non-UI half):**
```
test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.03s
```

Every other workspace crate (`tiller_activity`, `tiller_agents`, `tiller_control`,
`tiller_git`, `tiller_markdown`, `tiller_project`, `tiller_theme`) was untouched by this
wave's manifest and ran clean in both full-workspace passes (see below).

## 4. Two mechanical integration breaks found and fixed

Both are exactly the kind the manifest and QUEUE.md anticipate — a sibling's owned-file
edit reaching into a struct/constant a foreign-file section had already documented — not a
redesign of anyone's change:

1. **`crates/tiller/src/main.rs`** (owned by B1-tabbar-zorder) — `E0063` missing-fields
   compile error from B7-editor-persist's `CatalogProjectSettings` widening. Fixed per
   B7's own prescribed fix (commit `f74334b`).
2. **`crates/tiller_persistence/tests/persistence_integration.rs`** — B7-editor-persist's
   new migration v13 (`account_identity` table, F-PERSIST-DB-06) bumped
   `CURRENT_SCHEMA_VERSION` from 12 to 13, and B7's own wave report flagged that
   `v9_tab_rows_upgrade_to_current_with_no_recorded_agent_identity` still hardcoded the
   literal `12` in two assertions (~lines 274-275) and would fail. Confirmed the failure
   (`left: 13, right: 12`), then replaced both hardcoded-literal assertions with a single
   comparison of `db.schema_version()` against `CURRENT_SCHEMA_VERSION` (B7's own
   suggested fix), so the test doesn't need editing every time a migration is added.
   Commit `bafdc71`.

No other build or test regression was found. No slice was reverted or redesigned.

## 5. Flakiness note (not a regression, not fixed)

Two **different**, pre-existing, timing-sensitive tests each failed exactly once, only
under `cargo test --workspace` (every test binary in the workspace running concurrently,
including PTY/GPUI executors and `sleep`-based fixtures contending for CPU):

- `tiller_ui`'s `chat::tests::stopping_via_click_with_a_queued_item_still_sends_it`
  failed on the first full-workspace run, then passed on isolated reruns (3x) and on a
  second full-workspace run.
- `tiller_git`'s `streaming_lines_arrive_incrementally_before_completion` (asserts a
  `sleep(1.2s)` subprocess's first line arrives >= 600ms before it exits) failed on the
  second full-workspace run, then passed in isolation.

Neither test's file is touched by any wave-B builder (`tiller_git/tests/p99_git_rows.rs`
predates this wave entirely; the chat test lives in B2's owned `chat.rs` but reproduces
clean in isolation, repeatedly). Every per-crate isolated run in section 3 above is 100%
green with no flakes across repeated runs. Left untouched — not a mechanical integration
break, and rewriting a pre-existing timing assumption under load is out of this pass's
scope.

## What a verifier needs to know

- `cargo build -p tiller` succeeds; the binary is at `rust/target/debug/tiller`.
- Drive tests **per-crate** (`cargo test -p <crate> ...`), not the full concurrent
  `cargo test --workspace`, if you want a clean run without chasing the CPU-load
  flakes described in section 5 — both observed flakes vanish in isolation.
- All 7 slices' own reported test commands were re-verified and match their reports,
  except `tiller_persistence`'s `persistence_integration` (fixed above — was not part of
  any builder's own report, discovered by this pass).
