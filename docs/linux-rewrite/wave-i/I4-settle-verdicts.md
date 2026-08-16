# I4-settle verdicts

Critic pass over the three rows in `I4-settle-report.md`. Each reached with a fresh, independent
live drive this pass (not a re-run of the builder's own script verbatim) — routes and raw findings
below; the compact evidence lands in `INVENTORY-LEDGER.md`.

## `F-WIN-09` — titlebar double-click preference — **PASSED**

Own live drive, X11 lane, `Scripts/linux-drive.sh`, `DISPLAY=:1`. First attempt (raw
`xdotool mousemove --window $WID ...` + `click --repeat 2`) landed on nothing — `WM_STATE` stayed
`Normal` — which is itself the documented reason the harness's `click()` helper translates to
absolute screen coordinates instead of `--window` targeting (X11-guest-of-Wayland drops
window-targeted synthetic events). Redone through the harness's real coordinate math
(`WIN_X`/`WIN_Y` + window-relative `700,16`):

```
gsettings set org.gnome.desktop.wm.preferences action-double-click-titlebar minimize
Scripts/linux-drive.sh out.png '... click at WIN_X+700, WIN_Y+16 twice, 100ms apart ...'
xprop -id $WID WM_STATE _NET_WM_STATE
```

Before: `WM_STATE: Normal`. After: `WM_STATE: Iconic`, `_NET_WM_STATE` gained
`_NET_WM_STATE_HIDDEN` — the real ICCCM/EWMH minimize signal, not a pixel diff. gsetting restored to
`toggle-maximize` afterward. This independently reproduces the builder's finding with a corrected
instrument of my own construction, and the failed first attempt is itself useful evidence for why
the harness's `click()` helper (not raw `--window` xdotool) is required on this lane.

## `F-CHAT-33` — MCP warning banner — **UNREACHABLE (through the default agent)**

Ran the builder's new regression test live: `cargo test -p tiller_acp --lib -- --ignored
real_agent_stays_silent_on_stderr_for_an_unapproved_broken_mcp_json` → passed in 11.63s against the
real network agent, matching the claimed 11.7s.

Then went further, independently, with my own hand-built Python JSON-RPC driver (not the builder's
script) speaking directly to `npx -y @agentclientprotocol/claude-agent-acp@latest` over stdio against
a fresh, genuinely-unapproved scratch repo with a broken `.mcp.json`
(`{"mcpServers":{"broken-server":{"command":"/definitely/missing/mcp-nonexistent-binary"...`):
`initialize` → `session/new` (with the matching `mcpServers` entry) → a `session/prompt` asking it to
list its MCP servers and report errors. Result: the agent itself chose to run `claude mcp list` as a
Bash tool call, and its output — arriving as ordinary `session/update` `tool_call_update` content, not
on the process's own stderr — read `broken-server: /definitely/missing/mcp-nonexistent-binary -
⏸ Pending approval (run \`claude\` to approve)`. Across the whole exchange the process's own stderr
stream (pumped continuously via a background thread, not just sampled at the end) produced **zero
lines**. This independently confirms both halves of the builder's report — the unapproved-trust-gate
silence, and the "wrong channel" finding (MCP status arrives as chat/tool-call text, never stderr) —
through a driver I wrote myself, not a copy of theirs.

The task brief's own framing applies directly: the CLI emits nothing on stderr for this failure shape,
in the only state a real Tiller project is ever actually in (unapproved). `looks_like_mcp_warning`'s
vocabulary was never the blocker — the channel it watches carries no signal for the one agent
integration (`ClaudeCodeAdapter`) verified this pass. Not tested this pass: Codex's ACP adapter
(`tiller_agents/src/codex.rs`, `@agentclientprotocol/codex-acp@latest`) — a different vendor binary
with an unknown MCP-config format and unknown stderr behavior, the one remaining candidate that could
in principle make this row reachable. `opencode.acp_program()` is `None` (no ACP path at all, per
`tiller_agents/src/lib.rs`'s own test), so it cannot ever trigger this row.

## `F-CHAT-05` — offline composer — **FAILED — defective**

Own live drive, `wayland-drive.sh`, `TILLER_ACP_PROGRAM=/definitely/missing/mcp-nonexistent-binary`,
fresh scratch repo, one invocation: `project.add` → click the worktree row → `tab.select index=1` →
click the composer → type a marker string only this drive could have produced
(`critic-owned draft cRiTiC-9f3q must not vanish`) → `key Return` → screenshot →
`surface.chat.read`. Confirmed by eye and by socket read-back: composer placeholder reads "Agent
offline — reconnecting when you send…"; the field visibly accepts real typed keystrokes (not greyed,
not read-only, orange focus ring present); pressing Return does not send or clear — a **second**
identical red `could not launch ACP agent` banner appears and `surface.chat.read` returns
`"composerText":"critic-owned draft cRiTiC-9f3q must not vanish"` byte for byte, `"status":"idle"`.
This matches the builder's claim exactly, via a drive of my own construction.

**This changes the verdict, not just re-confirms it.** The row's own cited `SRC`
(`App/Chat/ChatComposerView.swift:72`, in the Swift reference at
`/home/enzopalmisano/Scrivania/Progetti/tiller`) was checked against the actual reference behavior,
per house rules ("check scope claims against the reference — it goes both ways"), not just read as a
historical VERIFY string. `ChatComposerView.swift:25-28`: `canInteract` is true only when
`controller.state` is `.ready`, `.prompting`, or `.detached` (no process started yet) **and** there is
no pending permission; `.disabled(!canInteract)` gates the whole editor (line 92).
`ChatController.swift`'s `start()` catch block (line 360) sets `state = .disconnected(message:
"\(error)")` on exactly this failure shape — a failed agent launch/connect — and `.disconnected` is
**not** in `canInteract`'s allow-list. So the reference macOS app **does disable the composer** on a
failed connection, exactly as the contract's literal VERIFY line says, and has no retry-and-resend
feature for this state at all (the composer is inert, not queuing). The Linux rewrite's "keep it
enabled, retry on Send" behavior is therefore a genuine divergence from the cited reference, not a
stale VERIFY line written before this UX existed — the builder's premise for proposing a reword
(`tasks/P132-f-chat-05-offline-composer-reword.md`) does not hold up against the reference it names.
The dangerous half (draft loss) is still correctly disproven — that finding stands — but the row's
literal, reference-matching requirement (disable on connection failure) is not met, and the
permission-wait half remains, separately, never live-reached by any pass. One required half fails on
the merits; the row fails.
