# H6-instruments report

Six rows, all recorded as blocked on proof rather than code. Each subsection is committed
independently as soon as its row is done, per house rules.

## `F-USE-03` — env override built, driven live — **fixed/instrumented, live-confirmed**

`ClaudeUsageFetcher::fetch()` had `TIMEOUT` hardcoded at 25s with no seam for a live drive, and two
critics had correctly declined to risk a real 25s hang against this harness's own 180s silence kill.
Added `TILLER_USAGE_CLAUDE_TIMEOUT_MS`, an opt-in env override read fresh on every `fetch()` call
(`rust/crates/tiller_usage/src/claude.rs`); unset in every normal run, so production behaviour is
byte-for-byte unchanged. New unit test
`timeout_override_reads_the_env_var_and_falls_back_to_the_default` locks in the fallback and the
unparseable-value case (must not panic).

Drove it live: `TILLER_USAGE_CLAUDE_TIMEOUT_MS=1200` (below `SETTLE`'s unconditional 2s pre-`/usage`
sleep, so the deadline is guaranteed to have already passed by the time the phase-2 poll loop's first
`Instant::now() < deadline` check runs) forces a **real** `ClaudeUsageFetcher::fetch()` call — a real
PTY spawn of the real `claude` binary on this host — to return `UsageFetchOutcome::TimedOut`
deterministically, through the actual production `fetch_with` code path, not a synthetic outcome
injected around it. `wayland-drive.sh` capture of the mounted `StatusBar` shows `Claude timed out`
rendered in the same dimmed grey as `Codex logged out` beside it — confirming the real outcome reaches
`apply_outcomes`/`reduce`/`segment_dimmed` on a live entity. (The existing
`gpui::TestAppContext` test already proved the Loaded→Stale reducer wiring with a real `Context<StatusBar>`
entity and synthetic outcomes; this closes the remaining gap — a *real* fetch, not a synthetic one,
reaching that same entity live.)

Commit: `ab7b125f`.

**howToExercise**: launch the app with `TILLER_USAGE_CLAUDE_TIMEOUT_MS=1200` in its environment
(e.g. via `wayland-drive.sh`, which inherits the calling shell's exported env), let it render once,
and `shot` the bottom-left status bar — the Claude segment reads "Claude timed out" in dimmed grey,
distinct in wording from the other Unavailable reasons beside it.

---

## `F-CHAT-33` — MCP warning banner — **real defect found and fixed; row still not fully closeable through the default agent**

Root-caused by direct attack on the CLI, as instructed. `tiller_acp::AcpClient::launch` built every
`session/new` request with `NewSessionRequest::new(cwd)`, whose constructor sets
`mcp_servers: vec![]` — and nothing in the crate ever called the `.mcp_servers(...)` builder to
replace it. ACP's `mcp_servers` is a **required, non-defaulted** field on the wire (confirmed against
`agent-client-protocol-schema`'s `NewSessionRequest` — no `#[serde(default)]`, and a hand-driven
`session/new` with the key omitted entirely comes back `"mcpServers": {"_errors": ["Required value is
missing"]}`); the agent never auto-discovers `.mcp.json` from `cwd` on its own. So every prior
`F-CHAT-33` drive was asking the agent to connect to **nothing**, whether or not `.mcp.json` was
broken — a real, previously-undiagnosed defect, not a stderr-vocabulary gap.

Fixed: added `tiller_acp::mcp_config::discover_mcp_servers` (parses `.mcp.json`'s
`mcpServers` object — stdio `command`/`args`/`env`, or `http`/`sse` `url` — into ACP `McpServer`
entries; missing/malformed file yields an empty list, never a panic) and wired it into the
`session/new` call. 7 new unit tests, `cargo test -p tiller_acp` green (28+11+6 passing, 1
network-requiring test still `ignored` as before).

**This does not by itself make the row driveable.** Confirmed by hand-driving the actual default
agent (`npx -y @agentclientprotocol/claude-agent-acp@latest`, v0.69.0 — the same binary/command
`Chat::launch` spawns) directly over its ACP stdio protocol, three separate live process runs
(~90s combined), with a correctly wire-shaped `mcpServers` entry pointing at a nonexistent binary —
tried both untagged and with an explicit `"type":"stdio"` tag, and followed up with a
`session/prompt` explicitly asking the model to report on its configured MCP servers. Result every
time: **zero stderr**, and the model itself reports `No` server by that name is configured — its
actual visible tool list reflects only the *invoking user's own* global Claude Code MCP config
(`context7`, `figma`, etc. from this environment), never anything supplied via the ACP wire field.
`looks_like_mcp_warning`'s vocabulary was never the blocker: the CLI is architecturally inert to this
field in the currently-configured default agent, so it never has anything to write to stderr about.
The row needs either a different/more ACP-conformant agent, or the native `claude` CLI's own
`.mcp.json` auto-discovery (a different code path, not exercised through this ACP bridge at all) as
its trigger — recorded here so the next pass does not re-spend a fifth attempt on the same route.

Commit: `037006f9`.

**howToExercise**: `cd /tmp/<scratch> && git init -q && printf '{"mcpServers":{"broken-server":{"command":"/definitely/missing/mcp-nonexistent-binary","args":[]}}}' > .mcp.json && git add -A && git commit -qm x`; open that project's Chat tab in the live app (`ctl project.add path=<scratch>` then `tab.select`) and confirm no banner appears (expected, given the finding above) — or, to re-verify the finding directly without the app, feed the JSON-RPC handshake in this report's commit message by hand to `npx -y @agentclientprotocol/claude-agent-acp@latest` and watch its stderr.

---

## `F-TERM-SCR-02` — resize debounce and reflow — **instrument built, driven live; already correct, no code change**

A builder and two critics had declined to build this instrument; built it this pass. No `rust/` change
was needed — `TERMINAL_RESIZE_DEBOUNCE`/`OUTPUT_SETTLE_DEBOUNCE`/`resize_generation` behave exactly as
their names claim under a real divider drag.

**Instrument**: `/tmp/winch-trap.sh` (not committed — a disposable fixture, same convention as P130's
scratch files), run inside a real terminal pane:

```bash
#!/bin/bash
trap 'printf "WINCH %s cols=%s\n" "$(date +%s.%3N)" "$(tput cols)"' WINCH
echo TRAP_READY
while true; do sleep 0.02; done
```

**Drive** (one `wayland-drive.sh` invocation, per house rules): `project.add`, open the Terminal tab,
`rightclick` → `Split Right` to create a real divider, click into the left pane, `type`/`key Return`
the trap script, then a real button-held drag: `down 820 500`, four `move` waypoints stepping right
to `1050 500`, `up 1050 500`, two spaced `shot`s afterward.

**Result — real signal delivery, real coalescing, real convergence, all live in the pane's own
scrollback** (not inferred from code):

```
WINCH 1786886333.774 cols=61
WINCH 1786886334.011 cols=91
WINCH 1786886335.169 cols=61
WINCH 1786886335.572 cols=91
```

Six discrete pointer operations (`down` + 4 `move` + `up`) produced 4 `SIGWINCH` deliveries, not six —
real coalescing, not a 1:1 flood. The last two lines were **not yet present** in the screenshot taken
immediately after `up` and appeared only in a second screenshot taken ~1.1s later from the same
invocation — the resize pipeline keeps doing debounced/settling work asynchronously after the last
input event, exactly the shape `TERMINAL_RESIZE_DEBOUNCE` (120ms) → `OUTPUT_SETTLE_DEBOUNCE` (200ms)
predicts, not an immediate single synchronous resize. After the second capture, the log went quiet —
no runaway resize storm, no oscillation — and the visible divider position matches the pane's final
`cols=91`. Full screenshots: `/tmp/h6-scr02c/` (scratch, not committed).

No commit for code (nothing changed); this section is the record.

**howToExercise**: with a terminal pane running `/tmp/winch-trap.sh` (recreate it — the exact
`trap 'printf ...' WINCH` one-liner above, or type it in by hand) as one side of a `Split Right`,
`down`/`move`×N/`up` the divider and read the pane's own scrollback — no control-socket door exists
or is needed for this row; the pane's stdout **is** the instrument's readout.
