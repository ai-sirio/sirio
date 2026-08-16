# H6-instruments — critic verdicts

Independent critic pass over the six H6-instruments rows. Every row below was re-driven live by
this pass (fresh `wayland-drive.sh` invocations, own screenshots, own MD5s) — none of the
builder's claims were taken on faith. HEAD at drive time: `da5a1bab` (wave-h integration commit),
binary `rust/target/debug/tiller` current as of that commit. `cargo test -p tiller_usage` (10
passed) and `cargo test -p tiller_acp` (6+6 passed, 1 ignored) re-run clean, matching the
integration report.

## `F-USE-03` — PASSED

Ran `TILLER_USAGE_CLAUDE_TIMEOUT_MS=1200` live and captured the status bar: "Claude timed out" in
dimmed grey next to "Codex logged out". Then ran a **negative control** with the env var unset
(same repo, same route) and got a genuinely different, non-dimmed result: "Claude 4% 5h · 3% wk ·
0% Fable" — a real successful usage fetch. Same code path, same coordinates, only the env var
differs — this is discriminating evidence, not a screenshot that merely contains the words. Matches
the builder's claim exactly.

## `F-CHAT-33` — half-proven

Independently confirmed the negative finding. Built a fresh scratch git repo with `.mcp.json`
pointing at a nonexistent binary, opened its Chat tab live (`project.add`, select worktree, select
Chat), sent a real turn, waited 12s+3s. Result: no MCP warning banner in either capture, matching
the builder's own hand-driven-CLI finding. The `discover_mcp_servers` fix itself is real and
unit-tested (verified present in `tiller_acp/src/lib.rs`, `cargo test -p tiller_acp` green), so the
code-level half is proven; the live user-visible banner remains unreachable through the default ACP
agent, exactly as reported — this is not a builder claim I'm relaying, it's a null result I
reproduced myself. One reproducibility note for the record: an early attempt at this same drive
(before I added a `sleep 1` between worktree-select and chat-tab-click) died mid-drive with the
control socket refusing connections; a slower, identically-shaped re-drive completed cleanly twice
in a row, so this reads as a harness-timing flake, not a defect in the row's own code path — flagged
here rather than silently discarded.

## `F-TERM-SCR-02` — PASSED

Recreated the WINCH-trap fixture from scratch (not reused from the builder's own file), drove a
real `Split Right`, typed the trap script into the left pane, and did a real button-held drag
(`down`/4×`move`/`up`) across the divider. Own pane scrollback shows 6 real `SIGWINCH` deliveries
for 6 pointer ops (`cols=41→61→57→84→57→84`), with two of the six arriving ~3.2s after the drag's
`up` — live, asynchronous debounce/settle behavior, not a single synchronous resize. This is a
different divider position and a fresh trap script from the builder's own run, so it's an
independent reproduction, not a re-read of their screenshots.

## `F-TERM-UI-02` — PASSED

Reproduced the exact negative/positive pair at the identical pixel: typed a URL into a Terminal
pane, plain `click 400 111` left the tab strip unchanged (negative), `modclick logo 400 111` at the
same coordinate opened a new Browser tab with the URL in the address bar (positive, red XCB banner
present as expected per the documented P127 embedded-webview limitation — chrome renders, which is
all this row needs). Same-invocation, same-pixel discriminating pair, independently driven.

## `F-TERM-PTY-05` — PASSED (to the documented auth boundary)

Drove `chord ctrl+shift p` → typed `Codex` → `Return` live. A new Codex tab appeared in the sidebar
and tab strip; its pane rendered the real, unmodified `codex` CLI's own ASCII banner and "Sign in
with ChatGPT / Device Code / API key" screen, correctly sized and legible. This is the row's own
literal ask — a Codex child spawns and attaches correctly in a pane — and it is satisfied.
Confirmed independently, own screenshots (`/tmp/h6-critic-pty05/04-after.png`). The
credential-gated clauses past this screen remain out of scope per the task's explicit
do-not-authenticate instruction, same boundary the builder documented.

## `F-CHAT-20` — PASSED

Ran the deterministic ticker fixture (`TILLER_ACP_PROGRAM=Scripts/acp-ticker-fixture.py`,
220 lines @ 0.25s) live, streamed past viewport height, then drove the same four-capture sequence
the builder describes but with my own scroll amounts and timing. Own MD5s: `away` and `stillaway`
are **byte-identical** (`986aa571...`) despite ~4s of continued streaming underneath — genuine
scroll-decouple, not a stale-frame artifact (content shown: LINE 0001–0031 both times). `back`
(LINE 0089–0121) and `repinned` (LINE 0112–0144, a strictly later window) have **different** MD5s —
genuine re-pin tracking the live tail forward. Both halves independently reproduced byte-for-byte.

## Summary

6/6 rows independently re-driven live this pass. 5 PASSED outright; `F-CHAT-33` is `half-proven` —
a real, unit-tested fix landed, but the live banner it should produce remains unreachable through
the default ACP agent, confirmed by my own null result on the exact route described. No row's
verdict was taken from the builder's report without independent re-driving.
