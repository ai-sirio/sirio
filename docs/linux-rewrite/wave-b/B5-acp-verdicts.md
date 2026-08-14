# B5-acp — critic verdicts

Verified from `linux/gpui-waku` at the integration HEAD (`358e72c`). Slice owns only
`rust/crates/tiller_acp/src/lib.rs`; the sibling files each row's manifest also names
(`tiller_ui/src/chat.rs`, `tiller_ui/src/settings.rs`) were confirmed untouched — `grep -n
"mcp_warnings\|McpWarning\|mode_catalog\|set_mode(\|AuthRequired" rust/crates/tiller_ui/src/chat.rs`
returns zero hits except the pre-existing generic error path.

## Static/test confirmation

- `cargo test -p tiller_acp --lib` → 22 passed, 1 ignored (network-dependent, pre-existing) —
  matches the report exactly, including the three new tests per row.
- `cargo test -p tiller_acp --test acp_integration --test chat_integration` → 11 passed, 6
  passed, 0 failed — matches.
- `cargo check -p tiller_acp --all-targets` → clean. `cargo build -p tiller` → clean.

## Live drive

Used `Scripts/wayland-drive.sh` with `TILLER_ACP_PROGRAM` pointed at a hand-written `/bin/sh`
JSON-RPC fixture (same shape as `lib.rs`'s own `fixture_agent` test helper) that advertises
`authMethods:[{"id":"login","name":"Login",...}]` on `initialize` then rejects `session/new`
with wire code -32000. Setting `TILLER_ACP_PROGRAM` before launch makes the app's own default
Chat tab (`add_chat_tab(None, ...)` → `Chat::launch`) connect to the fixture with no other
wiring needed — this is a real, unmodified app code path, not a test harness.

### F-CHAT-02

Reproduced twice on independent fresh instances (`verify-B5-acp2`, `verify-B5-acp3`): the
default Chat tab's transcript renders a red error card reading **"could not launch ACP agent:
ACP agent requires authentication (Login)"** with a **Retry** button — where the prior ledger
evidence was a wholly blank transcript pane after the same real-send confirmation. Clicked
Retry (`verify-B5-acp3`, coords confirmed against the actual captured frame, not assumed): it
re-attempted the connection through the same live code path and rendered a second, identical
auth-guidance card stacked below the first — proving Retry is wired and the detection is
deterministic, not a one-off.

This is real, discriminating progress — nothing renders like this without this slice's
`AcpError::AuthRequired` + `Display` impl, confirmed structurally absent before (ledger:
"no auth banner, no Retry"). It does not fully satisfy the row's clause, though: this is the
pre-existing generic `ErrorKind::Connection` card (no distinct "Authentication required"
banner styling), and it carries no **CLI guidance** — no "run `claude auth login`" hint, only
the bare advertised method name "(Login)" folded into prose. The dedicated `ErrorKind::AuthRequired`
+ settings.rs login-argv banner is exactly what `wantedForeignFiles` says is still owed, and
that owed half is precisely the half the clause needs to reach `PASSED`.

Screenshots: `verify-B5-acp3` run, frames `02-before-retry.png` (first card) and
`03-after-retry-click.png` (two stacked cards after Retry).

### F-CHAT-15

Live-clicked the mode/model pill ("● idle Opus Plan Mode") on a real session (default
Chat tab, no fixture override — real `npx @agentclientprotocol/claude-agent-acp` reached
idle within the settle window) at its exact on-screen coordinates, confirmed against the
captured frame. Before and after frames are pixel-identical in the pill region — no dropdown,
no popover, nothing opens. This matches the ledger's existing finding exactly ("the chooser
NEVER opens").

Structurally confirmed why: `AcpClient::mode_catalog()` / `AcpClient::set_mode()` (this
slice's new live accessors) have **zero** call sites in `tiller_ui/src/chat.rs` — the pill's
click handler still only flips the same hardcoded 4-way UI-state string the ledger already
documented. Nothing a user can reach changed. Verdict is unchanged from the ledger.

### F-CHAT-33

No render path exists to drive: `mcp_warnings()` and `ErrorKind::McpWarning` have zero
references anywhere in `tiller_ui/src/chat.rs` (confirmed by grep, matching the report's own
"detection/exposure half only" framing and the ledger's prior "code-absent, not a coverage
gap" finding for this exact half). The turn-error half this row's `half-proven` already rests
on is untouched by this slice. Nothing observable changed; verdict is unchanged from the
ledger.

## Verdicts

| id | verdict | why |
|---|---|---|
| F-CHAT-02 | FAILED — defective | Live-reproduced: a real auth-guidance error card + working Retry now render (was wholly absent) — but it's the generic Connection-kind card, not a dedicated banner, and carries no CLI login guidance, so the clause is not fully met. |
| F-CHAT-15 | FAILED — defective | Live-reclicked the pill on a real session; frame unchanged, no dropdown. Backend `mode_catalog`/`set_mode` built but zero call sites in chat.rs. Unchanged from ledger. |
| F-CHAT-33 | half-proven | Backend `mcp_warnings()` built and unit-tested, but zero call sites in chat.rs — still no render path. Turn-error half (unaffected by this slice) keeps this at half-proven, unchanged from ledger. |
