# P42 ACP Hardest Test Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Re-exercise ACP end to end, lock down streaming and failure-path outcomes, and verify all five agent adapters still obey their local-config and Codex TOML invariants.

**Architecture:** Keep protocol coverage headless. A small ACP fixture process will speak the same line-delimited JSON-RPC protocol as an agent and expose deterministic modes for normal streaming/tool/permission, malformed input, process death, and cancellation. One ignored real-agent integration test will drive the installed Claude ACP adapter, require streamed chunks and a tool permission round-trip, and prove the turn by finding a nonce-named file on disk.

**Tech Stack:** Rust 2024, Cargo integration tests, `tiller_acp`, `agent-client-protocol` v2, `serde_json`, installed `claude`/`npx` for the real acceptance run.

## Global Constraints

- Do not touch `tiller_ui/**`, `tiller_theme/**`, `tiller/src/main.rs`, `tiller_control/**`, or `tiller_git/**`.
- Never copy code from `../_tiller-refs/{waku,zed,orca,t3code}`.
- `prepare` may write only worktree-local configuration and must leave `~/.claude/settings.json` and `~/.codex/config.toml` unchanged.
- Codex `-c notify=[...]` JSON must not contain `\\/`; it is parsed as TOML.
- `opencode` and `omp` are unreachable when their executables are absent; absence is not a failed adapter.

---

### Task 1: Deterministic ACP protocol fixture and red tests

**Files:**
- Create: `rust/crates/tiller_acp/tests/acp_integration.rs`
- Create: `rust/crates/tiller_acp/tests/fixtures/acp_fixture.py`
- Modify: `rust/crates/tiller_acp/Cargo.toml` only if the fixture test needs an explicit dev dependency

**Interfaces:**
- The fixture accepts one mode argument and speaks ACP v1 over stdin/stdout.
- The test helper launches it through `AgentCommand` and returns `AcpClient` plus `EventStream`.
- The normal mode emits two assistant chunks, a tool-call start/update, a permission request, and a terminal response after the permission response.
- The malformed, death, and cancel modes produce deterministic protocol outcomes without waiting for production five-minute limits.

- [x] **Step 1: Add the fixture and tests before changing production code.**

  Define test names that assert exact outcomes: `streams_chunks_tool_and_permission_denial`, `malformed_frame_is_rejected_without_hanging_the_session`, `agent_death_is_transport_error`, and `cancel_produces_a_terminal_outcome`.

- [x] **Step 2: Run the integration test to verify the missing seam fails.**

  Run `source ~/.cargo/env && cd rust && cargo test -p tiller_acp --test acp_integration -- --nocapture`.

  Expected: compilation or assertion failure because the fixture/test harness and required public behavior do not yet exist.

---

### Task 2: Minimal ACP lifecycle behavior

**Files:**
- Modify: `rust/crates/tiller_acp/src/lib.rs`
- Test: `rust/crates/tiller_acp/tests/acp_integration.rs`

**Interfaces:**
- Preserve `AcpClient::launch`, `prompt`, `cancel`, `respond_permission`, `shutdown`, and `AcpEvent` as the application seam.
- Any new public error/event data must remain typed and observable by callers.

- [x] **Step 1: Implement only the behavior named by the failing tests.**

  The existing implementation already satisfied the required lifecycle behavior once exercised: malformed frames are rejected and the session continues, unexpected child exit emits `AcpEvent::TransportError`, cancellation resolves both pending prompts and pending permissions, and a denied permission response reaches the fixture as the rejected option selection. No production change was necessary.

- [x] **Step 2: Run the focused ACP integration test until green.**

  Run `source ~/.cargo/env && cd rust && cargo test -p tiller_acp --test acp_integration -- --nocapture`.

- [x] **Step 3: Run the existing ACP unit tests.**

  Run `source ~/.cargo/env && cd rust && cargo test -p tiller_acp`.

---

### Task 3: Adapter evidence and invariants

**Files:**
- Modify: `rust/crates/tiller_agents/tests/adapters_tests.rs`
- Modify: `rust/crates/tiller_agents/tests/home_isolation.rs` only if a separate process is required for environment isolation

**Interfaces:**
- Assess `AgentCatalog`/`ALL` entries in catalog order.
- Record actual executable availability and classify missing `opencode`/`omp` as unreachable.
- Call every adapter’s `prepare` in a temporary worktree and compare the actual global config mtimes before and after.

- [x] **Step 1: Add the mtime regression test before any adapter change.**

  Capture `metadata.modified()` for `$HOME/.claude/settings.json` and `$HOME/.codex/config.toml`, call all five adapters’ `prepare`, then assert each existing file’s timestamp and existence are unchanged.

- [x] **Step 2: Run adapter tests and verify the invariant is green.**

  Run `source ~/.cargo/env && cd rust && cargo test -p tiller_agents --test adapters_tests -- --nocapture`.

- [x] **Step 3: Verify the Codex command round trip still rejects slash escaping.**

  Assert the generated `-c` value contains no `\\/`, decodes to the six expected notify arguments, and preserves a slashy tillerctl path.

---

### Task 4: Real Claude ACP acceptance run

**Files:**
- Modify: `rust/crates/tiller_acp/examples/acp_smoke.rs` if the existing smoke entry point is extended for the evidence run
- Create: `rust/crates/tiller_acp/tests/real_claude.rs` if a repeatable ignored test is the cleanest seam

**Interfaces:**
- Use `npx -y @agentclientprotocol/claude-agent-acp@latest` unless `TILLER_ACP_PROGRAM` supplies an equivalent command.
- Send a prompt containing a random nonce and request a file whose name contains that nonce.
- Require at least two assistant chunks, a tool-call event, a permission request, the permission response, a terminal turn event, and the nonce file on disk.

- [x] **Step 1: Run the real Claude path with the existing smoke harness or ignored test.**

  Run `source ~/.cargo/env && cd rust && cargo test -p tiller_acp -- --ignored --nocapture` (or the narrowed ignored test).

- [x] **Step 2: Record the nonce and the exact failure-path outcomes.**

  Preserve no credentials or user-global config contents; report only the nonce, event/outcome names, availability counts, and the mtime comparison.

---

### Task 5: Full verification and handoff

**Files:**
- No additional production files unless a focused test exposes a required fix in `tiller_acp/**` or `tiller_agents/**`.

- [x] **Step 1: Run focused package tests.**

  Run `source ~/.cargo/env && cd rust && cargo test -p tiller_acp -p tiller_agents`.

- [x] **Step 2: Run the repository Linux gate.**

  Run `./Scripts/ci-linux.sh` from the repository root.

- [x] **Step 3: Review the scoped diff and check forbidden paths.**

  Run `git diff --check -- rust/crates/tiller_acp rust/crates/tiller_agents` and verify no changed path is outside the claimed packages plus the plan/evidence files.

- [x] **Step 4: Report in 12 lines or fewer.**

  Include whether real ACP works, the nonce, each failure-path outcome, global mtime evidence, the five-entry availability counts, and the honest remainder.

## Verification Record

- Real Claude ACP passed three times; final nonce: `tiller-acp-1786623346473812029`.
- Real run observed at least two streamed assistant chunks, a tool call and completion, an allow permission request/response, `EndTurn`, and the nonce file on disk.
- Deterministic ACP integration: 5/5 passed — denial reaches the rejected option, malformed input is rejected while the session continues, mid-stream death emits `TransportError`, prompt cancellation returns `Cancelled`, and permission-pending cancellation returns `Cancelled`.
- Adapter assessment: 5 entries total; 3 available (`claude`, `codex`, `pi`), 2 unreachable (`opencode`, `omp`), 0 failed.
- Global config mtimes before/after were unchanged: `.claude/settings.json` `1786576594.444752722`; `.codex/config.toml` `1786602045.304240035`.
- `cargo test -p tiller_acp -p tiller_agents` passed; `source ~/.cargo/env && ./Scripts/ci-linux.sh` printed `CI OK`.
- Repository-wide format drift and clippy warnings remain pre-existing outside this piece; no new production ACP/adapter fix was required.
