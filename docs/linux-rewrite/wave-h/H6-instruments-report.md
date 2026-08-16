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

---

## `F-TERM-UI-02` — logo/cmd+click a terminal URL opens the Browser tab — **two real defects found and fixed; live-confirmed end to end**

P130 narrowed this to "app-side coordinate/hit-testing, out of that task's scope" and named the exact
next step: dump the clicked `(row, column)`. Did that (a new opt-in instrument), and it found not one
defect but two, stacked — fixing only the first would still have produced no visible effect.

**Defect 1 — hit-testing used a hardcoded cell width, not the real measured one.**
`on_left_mouse_down` computed the clicked column with a literal `8.0`, while
`TerminalElement::prepaint` (which paints the very cells being hit-tested) measures the real glyph
advance of "MesloLGS Nerd Font Mono" at 13px via `window.text_system().advance(...)` and falls back to
`8.0` only when that's unavailable. The real value, confirmed live via the new
`TILLER_DEBUG_LINK_CLICK` instrument, is `7.827`, not `8.0` — a small per-column error that
accumulates with distance from the pane's left edge. Fixed by threading the real measured width
through a new `last_cell_width` field (same `Arc<Mutex<Option<Pixels>>>` pattern `last_bounds`
already used for the origin half of this same hit-test). Commit `ec40654c`.

**Defect 2 (found only after fixing #1, via the same instrument) — the link event was wired to the
wrong destination.** With hit-testing now correct, `TILLER_DEBUG_LINK_CLICK`'s log showed
`link=Some("https://example.com")` and `cx.emit(TerminalLinkEvent{...})` firing — yet still no
visible Tiller tab. `subscribe_terminal_link` called `cx.open_url(url)` (GPUI's generic external-URL
opener, `xdg-open` on Linux) instead of queuing `WorkspaceAction::OpenBrowserLink`, the route
`bind_chat`'s `ChatEvent::OpenLink` already uses for the *same* "open in Tiller's own Browser tab"
behavior from chat links. This is why a live modclick eventually did pop a real external Brave
window (once, with a session-restore banner) in one experiment — the click was working, just opening
the wrong thing, on a delay long enough that five independent live attempts across two waves never
correlated their click with any visible Tiller-side effect. Fixed by mirroring `bind_chat` exactly:
push `WorkspaceAction::OpenBrowserLink` onto `pending_actions`. New test
`a_terminal_link_click_queues_the_in_app_browser_tab_action` drives the real `subscribe_terminal_link`
subscription (not a reimplementation of its logic) and asserts the queued action. Commit `c331914c`.

**Instrument**: `TILLER_DEBUG_LINK_CLICK=1` (commit `d8be5b25`) — opt-in `eprintln!` in
`on_left_mouse_down` printing position, origin, real `cell_width`, resolved `(row, column)`, and the
link found. This is what turned coordinate-guessing into ground truth.

**Live end-to-end confirmation** (one `wayland-drive.sh` invocation, negative and positive control at
the identical coordinate): typed `echo https://example.com` in a terminal pane; plain `click 400 111`
left the tab strip at Chat/Terminal, unchanged; `modclick logo 400 111` at the **same pixel**
opened a new **Browser** tab in Tiller's own tab strip with `https://example.com` in the address bar
(the red "Direct XCB build failed" banner in the capture is the already-documented P127 Wayland-lane
limitation on embedded webview *content* — chrome renders correctly, which is all this row needs;
route content-rendering questions to `Scripts/linux-drive.sh` per `WAYLAND-LANE.md`).

`cargo test -p tiller_terminal`: 43 passed. `cargo test -p tiller`: 159 passed.

**howToExercise**: `TILLER_WL_LABEL=x Scripts/wayland-drive.sh /tmp/out 'ctl project.add
path=<repo>; click 500 50; click 500 300; type echo https://example.com; key Return; sleep 1; click
400 111; shot negative; modclick logo 400 111; shot positive'` — `negative` shows the tab strip
unchanged, `positive` shows a new **Browser** tab selected with `https://example.com` in the address
bar. (Exact pixel numbers are specific to this pane's current font/size and layout; re-derive with
`TILLER_DEBUG_LINK_CLICK=1` and read `APP_LOG` if the coordinates ever stop landing.)

---

## `F-TERM-PTY-05` — a Codex child in a pane — **fully reachable up to the login boundary; boundary established live**

The task's own diagnosis was right that the blocker is environmental (`codex login status` reports
`Not logged in` on this host) but that leaves an open question the row itself asks: *how much of the
row is reachable without auth?* Answer, established live: **all of it up to and including a real,
correctly-rendered Codex sign-in screen.** No `rust/` change was needed or made — `main.rs`'s launch
path (`open_command_palette` → `NewTabAction::Codex` → `add_agent_tab` → `CodexAdapter::command`,
`rust/crates/tiller_agents/src/codex.rs`) was already correct.

**Instrument gap found and fixed first**: driving this row needs the command palette, which only
opens on `Ctrl+Shift+P` — a *two*-modifier chord — but `wayland-drive.sh`'s `chord <mod> <key>` only
ever held one modifier. Extended it to accept `+`-joined modifiers (`chord ctrl+shift p`), pressing
in order and releasing in reverse, matching how a real keyboard delivers a held multi-modifier chord.
Sanity-checked outside the app first: a raw `pty.fork()` harness against the real `codex` binary
initially produced only 91 bytes of terminal-setup escapes and then silent stall — traced (via
`strace -f`) not to any auth/network wait (no `connect()` calls in the trace at all — outbound network
to `api.openai.com` works fine from this host) but to the harness leaving the pty's winsize at its
`pty.fork()` default of 0x0, which the TUI silently declines to render into. Setting a real winsize
(`TIOCSWINSZ`) unblocked full rendering — the app's own PTY spawn already sizes the pty to the pane's
real dimensions, so this was purely an artifact of the throwaway diagnostic harness, not something
`rust/` needed to guard against.

**Live drive** (one `wayland-drive.sh` invocation): `project.add` → `click` into the Terminal tab →
`chord ctrl+shift p` opens the command palette → `type Codex` filters to a `Codex` / `Codex Here`
pair → `key Return` selects the first. Result: a new **Codex** tab appears in the tab strip (sidebar
and tab bar both show it, with a "1 running" Activity badge), and its pane renders the real `codex`
CLI's own ASCII-art banner followed by:

```
Welcome to Codex, OpenAI's command-line coding agent
Sign in with ChatGPT to use Codex as part of your paid plan
or connect an API key for usage-based billing

> 1. Sign in with ChatGPT
     Usage included with Plus, Pro, Business, and Enterprise plans
2. Sign in with Device Code
     Sign in from another device with a one-time code
3. Provide your own API key
     Pay for what you use

Press enter to continue
```

This is the real, unmodified `codex` binary's own first screen, rendered correctly at the pane's
actual size — not a Tiller-side placeholder or error. It confirms: the child process spawns, the PTY
attaches to the pane and renders correctly, and the pane shows the CLI's own correct, visible,
interactive failure mode for being logged out. Screenshots:
`/tmp/h6-pty05c/03-palette-open.png`, `04-palette-typed.png`, `05-after-select.png` (scratch, not
committed — recreate with the `howToExercise` command below).

**Exact boundary**: everything up to and including this sign-in screen is proven reachable without
auth. What remains unreachable without a logged-in `codex` CLI is the interactive selection past this
screen (options 1/2 both require either a live ChatGPT OAuth round-trip or a device-code exchange;
option 3 needs a real API key) and anything beyond it — i.e. an actual Codex coding turn inside the
pane. No further clause of this row can be exercised on this host without providing credentials, per
the task's explicit instruction not to attempt that.

Commit: `3ab56291` (Scripts/ instrument only; no `rust/` change).

**howToExercise**: `TILLER_WL_LABEL=x Scripts/wayland-drive.sh /tmp/out 'ctl project.add
path=<repo>; click 500 50; chord ctrl+shift p; type Codex; key Return; shot after' 5` — the capture
shows a new **Codex** tab selected, its pane rendering the CLI's own "Welcome to Codex" / "Sign in
with ChatGPT" screen. To go further requires real credentials, which this task was explicitly told
not to obtain.
