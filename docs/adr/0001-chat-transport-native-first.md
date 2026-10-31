---
status: accepted
date: 2026-10-31
---

# A chat speaks its agent's native protocol, and ACP is the fallback

A chat tab talks to every agent (Claude Code, Codex, OpenCode, Pi, Oh-My-Pi)
through that agent's own machine protocol, served by the binary on the user's
PATH, the same one a terminal pane runs. Each native chat starts its own agent
process and ends it when the chat closes. The Agent Client Protocol remains a
first-class transport. It carries any agent that has no native transport,
including agents Sirio adds later, and it is the fallback for an agent that
has one. We chose this because the session panel needs what ACP
does not carry: what fills the context window, the connectors, and a trace of
each turn down to its subagents' tool calls. A live probe on 2026-10-21 measured
that ceiling. Codex's ACP bridge never forwards a subagent's own tool calls. omp
sends no subagent metadata at all. OpenCode's subagent tree exists only as a
vendor `_meta` extension. Claude moved first, in September 2026, and showed that
one seam (`AcpEvent`) keeps `sirio_ui::chat` unaware of which transport a tab uses.

The transport is resolved once, when the chat starts. The native transport is
used when the binary is on PATH at or above the minimum version its conformance
test certifies. Otherwise the chat uses ACP, and Settings → Agents shows why.
`SIRIO_<AGENT>_TRANSPORT=acp` forces the fallback for diagnosis. A native
session that fails shows its error in the chat and never switches transport
behind the user's back, because a silent switch would hide exactly the protocol
change the conformance test exists to catch.

## Considered options

- **ACP for every agent, reading each vendor's `_meta` for the panel.** One
  protocol to maintain, but the panel would be capped by the bridges. Those
  extensions are not a contract either.
- **Native only, ACP removed.** Less code, but a chat would stop working for
  anyone whose binary is too old to speak the native protocol.

## Consequences

- ACP keeps a home of its own: `sirio_acp` is the ACP client and nothing else.
  The native transports live beside it, under `sirio_chat`, together with the
  choice between transports. Claude's native transport moves there too. That
  way the ACP client can grow, with new agents and new protocol versions,
  without touching the native transports.
- Each agent keeps two chat transports. Both must emit the same events, so a
  transport is done when its chat does at least everything its ACP chat did,
  plus the panel's data and the resume described below.
- Most of these protocols are not published, stable contracts, and the CLIs
  update themselves. So each native transport degrades rather than rejects:
  it ignores unknown events and fields, and a refused request degrades that
  one feature, not the whole session. Each transport also has an opt-in
  conformance test that spends a real turn and stays off the release gate.
- A native chat uses the user's own version, configuration and sign-in for the
  agent, not those bundled in an ACP wrapper package.
- A native chat resumes the agent's own session when Sirio reopens it, as
  Claude's does. The ACP transport never did this: it always started a fresh
  session beside the restored transcript. When the agent refuses to resume,
  the chat starts a fresh session rather than leaving a dead tab.
- When an agent's protocol does not report something, the session panel leaves
  out the card or row it cannot fill. Context is the one exception. It always
  has a total, so it says when it has no breakdown behind that total.
