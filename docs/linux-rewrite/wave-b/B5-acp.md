# Wave B slice B5-acp — 3 rows to build

**ACP agent protocol**

## Files you own this wave

- `rust/crates/tiller_acp/src/lib.rs`

No other agent will touch these. **You must not edit any file outside this list** —
every other source file belongs to a sibling and an edit there is silently lost or
silently overwrites theirs. If a fix genuinely needs a file you do not own, stop and
report it rather than making the change.

## Rows

### `F-CHAT-02` — ledger line 151, currently **FAILED — absent**

- **Triage:** build, size L
- **Files triage expects:** rust/crates/tiller_acp/src/lib.rs, rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_ui/src/settings.rs
- **Approach:** Add ACP auth-required detection (InitializeInfo has no auth_methods field, AcpError has no Auth variant, no authenticate RPC exists anywhere) and a new ErrorKind::AuthRequired rendered with CLI guidance (reuse settings.rs's existing per-agent login argv) and the existing Retry machinery.
- **Shared cause:** Its stale 'blank transcript' evidence (P109 batch, pre-h_full-fix) is a red herring for this row's real clause, which is the auth banner itself — genuinely unbuilt at the protocol level.
- **Evidence on record:** Live-driven: real send confirmed (context%/checkmark advance) but transcript pane rendered nothing at all — no auth banner, no Retry, through 14s wait + forced repaint. shots/57,60,61.

### `F-CHAT-15` — ledger line 164, currently **FAILED — defective**

- **Triage:** build, size L
- **Files triage expects:** rust/crates/tiller_acp/src/lib.rs, rust/crates/tiller_ui/src/chat.rs
- **Approach:** No AgentMode/SessionMode/available_modes concept exists anywhere; AcpEvent has no mode variant; the status pill's Ask/idle/working/offline labels are a hardcoded 4-way UI-state match, not agent-reported data. Needs research into whether any installed agent exposes a settable session mode over ACP before scoping a mode-report/set-mode surface plus a picker popover (model the popover on the existing model/effort one).
- **Evidence on record:** the pill DISPLAYS truthfully through every state — `Opus Plan Mode` pre-session, `● Ask ⌄` post-completion, mode/model/effort all correct (pass 17 frames p17-ah3/aj7) — but the chooser NEVER opens: clicks on the pill at idle and at offline, waits of 2s and 4s (menus elsewhere in this app paint ≤2.5s), zero dropdown (p17-am2/an4-modemenu-long/aj7). The clause's action — "choose each available mode"

### `F-CHAT-33` — ledger line 182, currently **half-proven**

- **Triage:** build, size M
- **Files triage expects:** rust/crates/tiller_acp/src/lib.rs, rust/crates/tiller_ui/src/chat.rs
- **Approach:** Turn-error half already works. MCP-configuration-warning half: zero non-test/non-comment 'mcp' mentions anywhere in tiller_acp or chat.rs -- no MCP concept modeled at all. Needs research into whether the ACP wire protocol/agents even surface an MCP config problem, then a new ErrorKind variant reusing the existing Entry::Error render/OK-dismiss path.
- **Evidence on record:** Reconfirmed independently at HEAD 4073297 (not just on driver's word): chat.rs enum ErrorKind has exactly one variant, Connection; every Entry::Error construction site uses it; the sole 'mcp' hit in the file is an unrelated doc comment; repo-wide grep across rust/crates (excluding tests) finds no other MCP mention. MCP-configuration-warning half has no render path to reach in the current binary --

