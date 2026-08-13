# Linux Verification Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and exercise `Scripts/ci-linux.sh` as the Linux workspace's honest, isolated verification gate.

**Architecture:** A Bash entrypoint runs named Cargo, Python, and headless-control stages from the repository root. It captures diagnostics per stage, distinguishes arrival drift from command failures, and uses a private socket/database/process group for the smoke test.

**Tech Stack:** Bash, Cargo/Rust, Python 3 `unittest`, `tiller`, and `tillerctl`.

## Global Constraints

- Do not edit `Scripts/ci.sh`; it remains the macOS gate.
- Run from `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` and use the `rust/` Cargo workspace.
- Build package `tiller_control`, not a nonexistent `tillerctl` package.
- Use a generated nonce for the panel write/read assertion.
- Use private `TILLER_SOCKET`, `TILLER_DB`, logs, and process cleanup on every exit path.
- State in the script header that visual behavior is not covered.
- Touch only `Scripts/**` plus the clean, unowned warning file selected in the design.

### Task 1: Add the failing shell contract test

**Files:**
- Create: `Scripts/Tests/test-ci-linux.sh`

- [ ] **Step 1: Write the failing test**

The test will locate `Scripts/ci-linux.sh`, require its executable contract,
and assert the stage order and safety markers with `grep`/`awk`. It must fail
before the gate exists and identify the missing script as the reason.

- [ ] **Step 2: Run the test to verify it fails**

Run `bash Scripts/Tests/test-ci-linux.sh`.

Expected: nonzero exit because `Scripts/ci-linux.sh` is absent.

### Task 2: Implement the isolated Linux gate

**Files:**
- Create: `Scripts/ci-linux.sh`

- [ ] **Step 1: Implement the header, root resolution, temp paths, and cleanup trap**

Use `set -Eeuo pipefail`, derive the repository root from `BASH_SOURCE`, create
`/tmp`-scoped run paths, override `TILLER_SOCKET` and `TILLER_DB`, and trap
cleanup that terminates the private app process group, removes the socket, and
removes the run directory.

- [ ] **Step 2: Implement named Cargo and Python stages**

Run format check, clippy, the two package builds, workspace tests, and the two
Python suites in the required order. Capture output per stage, print
pre-existing format/clippy drift separately, and make command errors report the
stage before exiting nonzero.

- [ ] **Step 3: Implement the headless smoke stage**

Launch `env -u DISPLAY -u WAYLAND_DISPLAY rust/target/debug/tiller`, wait for
the private socket, validate `current-workspace`, create/write/read a panel,
decode `panel.read`, and require a generated nonce in the decoded output.

- [ ] **Step 4: Run the contract test**

Run `bash Scripts/Tests/test-ci-linux.sh`.

Expected: PASS, with every required stage and safety marker present.

### Task 3: Clear the safe clippy warnings

**Files:**
- Modify: `rust/crates/tiller_usage/src/claude.rs`

- [ ] **Step 1: Make only the warning-preserving cleanup**

Remove the unnecessary `mut` from the spawned PTY binding and apply the two
same-file clippy suggestions that do not change behavior: iterate over the
candidate slice directly instead of indexing through `j`, and pass
`libc::TIOCSCTTY` without a same-type conversion.

- [ ] **Step 2: Verify the focused crate**

Run `source ~/.cargo/env && cargo test --manifest-path rust/Cargo.toml -p tiller_usage`.

Expected: exit 0 with the usage tests passing.

### Task 4: Run the complete gate and record evidence

**Files:**
- Verify: `Scripts/ci-linux.sh`

- [ ] **Step 1: Run the full gate with timing**

Run `source ~/.cargo/env && time Scripts/ci-linux.sh` from the worktree root.

Expected: every executable stage runs, headless round trip returns the generated
nonce, cleanup leaves no private app or socket, and the script prints `CI OK`.

- [ ] **Step 2: Inspect the final diff**

Run `git status --short -- Scripts/ci-linux.sh Scripts/Tests/test-ci-linux.sh rust/crates/tiller_usage/src/claude.rs` and `git diff --check`.

Expected: only the owned gate/test and the clean warning file are changed, with
no whitespace errors and no edit to `Scripts/ci.sh`.
