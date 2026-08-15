# Wave C slice C-CHAT-3 — verdicts

Critic pass. No code edited (independent-critic house rule). Every row below was re-driven live
on fresh instances of the Wayland lane (`Scripts/wayland-drive.sh` and manually-launched
instances of the same binary, `rust/target/debug/tiller`, built at HEAD, labels `critic-c3` /
`critic-c3f` / `critic-c3g`) — the builder's report and `INTEGRATION.md` were read only for
routes, never accepted as proof. Two of the six rows (`F-PERSIST-DB-06`, `F-SET-20`) were
reported `blocked` by the builder but had since been patched by the integrator's foreign-file
pass (`INTEGRATION.md` §4, commits `ea5071b`/`6af7ff4`/`cddf094`) — both were re-driven fresh
against that landed code, independent of either document's narrative, with opposite outcomes.

## F-CORE-AUTH-01 — PASSED

The sandbox's `claude` CLI is genuinely authenticated (`claude auth status` → `e.palmisano@reply.it`),
so the row's own "already-correct" claim was directly checkable without a real OAuth click-through.
Live-drove `surface.settings.select section=ai-providers`, forced a repaint, captured
`03-ai-providers-forced.png`: the Claude Code card reads **"Signed in e.palmisano@reply.it"**,
sourced from `discover_claude_identity` → `parse_claude_json` on real `claude auth status` stdout
(`settings.rs:539-551`). Discriminating: the Codex card on the same frame reads "Not signed in"
(a live negative control on the same screen, not a hardcoded string) and OpenCode Go/Ollama show
their own independent states — ruling out a static placeholder.

## F-PERSIST-DB-06 — PASSED (builder's "blocked" is stale; integrator's fix works, more strongly than INTEGRATION.md's own evidence)

Code check: `AppDatabase::account_identity`/`save_account_identity` exist in `db.rs:709-736`;
`Settings::with_database_path`/`sync_account_identity_cache` in `settings.rs:1008-1040` call them
on every discovery sweep; `main.rs:8551` wires `.with_database_path(...)` at construction.

Live drive, instance `critic-c3` (`TILLER_DB=/tmp/critic-c3.sqlite`): opened AI Providers (frame
`03-ai-providers-forced.png`, "Signed in e.palmisano@reply.it" as above), then read the DB
directly with Python's `sqlite3` module (no `sqlite3` CLI on this box):
`SELECT * FROM account_identity` → `[('claude', 'e.palmisano@reply.it', 1786795854592)]` — a real
row, not the builder's stale "0 rows".

**Stronger, discriminating test** (this pass, not in either document): killed the instance and
relaunched it against the **same DB** with `PATH` stripped of the directory containing the
`claude` binary (`which claude` → exit 1 under the modified `PATH`), so live discovery via
`claude auth status` was structurally impossible. Selected AI Providers again
(`04-nopath-fallback.png`): the card **still** reads "Signed in e.palmisano@reply.it". Since a
live shell-out could not have produced this, the value can only have come from the cached
`account_identity` row read back by `sync_one_account_identity`'s fallback path
(`settings.rs:1032-1040`) — proving both the write and the read/fallback halves work across a
real process restart, not just within one session.

## F-SET-14 — PASSED

Live drive, instance `critic-c3`, real gestures via the persistent virtual pointer (not `ctl`):
clicked Codex's "Add Account" — a real `x-terminal-emulator -e codex login` spawned
(`cosmic-term`), printing the actual OpenAI OAuth URL into the terminal
(`03-post-click.png`). Confirmed via `ps -eo pid,ppid,pgid,sid`: the login process detaches into
its own session exactly as the report describes (launcher pgid `778803` vs. login pgid `778841`,
same shape that made the old `kill -TERM <launcher>` a no-op). With the login process 18s old and
still alive, clicked "Cancel" through the same synthetic pointer and polled `ps -p 778841` at
0.3s intervals: the process was gone by the 4th poll, **dead within ~1.6s of the click** — well
inside the row's "not still running" bar. Follow-up capture (`06-after-cancel.png`) shows a clean
revert to "Add Account" with no error banner and no leftover process. (One initial click at
identical on-screen coordinates silently no-op'd because it fired before the just-selected
"AI Providers" section had actually repainted — WAYLAND-LANE.md's "repaint is lazy" trap applies
to synthetic clicks immediately following a `ctl` state change, not just to `shot`; inserting a
forced-repaint `shot` between the `ctl select` and the `click` fixed it. Noting this since it cost
real time and isn't spelled out in the lane doc for the click case specifically.)

## F-SET-20 — FAILED — defective (integrator's fix is incomplete; live-reproduced exactly the row's own VERIFY clause)

`INTEGRATION.md` claims this was "threaded end to end" (`cddf094`), and the struct/persistence
layer genuinely is: `SettingsSnapshot.translucency` (`settings.rs:377`), both directions of
`main.rs:8329`/`8360`'s round-trip, and `AppSettings.translucency` + `appearance.translucency`
load/save in `tiller_persistence/src/db.rs:824,962` are all real and covered by tests.

But the row's VERIFY clause is specifically about `surface.settings.select`'s wire payload, and
that is a **third**, separate serialization path — `settings_report_pairs` in
`rust/crates/tiller/src/main.rs:1864-1958` — which hand-builds its `values: BTreeMap` field by
field and was never updated to add `snapshot.translucency`. Live-reproduced on instance
`critic-c3`: queried `surface.settings.select section=appearance` over the control socket before
any interaction — no `translucency` key anywhere in the payload (confirmed by substring search
over the full JSON, not just the `values` sub-map). Clicked the Translucency toggle through the
virtual pointer (`08-after-toggle.png` shows it visibly ON, confirming the click-and-render half
genuinely works). Queried the same `ctl` call again — **still no `translucency` key**, byte for
byte the same field set as before the click. This is the exact defect the row's VERIFY line
names ("it should carry a translucency key/value matching the toggle") reproduced live, not
inferred from source alone. The struct field exists and round-trips through persistence; it is
simply never copied into the one function that actually puts it on the wire.

## F-CHAT-33 — half-proven

Code check confirms the "code-absent, not a coverage gap" defect on record is genuinely fixed:
`Chat::mcp_warnings_shown`, `Chat::surface_mcp_warnings`, and `ErrorKind::McpWarning` all exist in
`tiller_ui/src/chat.rs` and `surface_mcp_warnings()` is called from the `AcpEvent::TurnEnded`
handler (`chat.rs:1576`), matching the report's description. `tiller_acp`'s own
`mcp_warnings_stderr_line_is_observed_end_to_end` / `mcp_configuration_failure_on_stderr_is_captured_as_a_warning`
tests pass, proving the underlying stderr-capture-and-match plumbing.

Attempted genuine live reproduction, twice, on a fresh single-project instance (`critic-c3g`, no
pre-existing tabs to collide with — see note below) with a real, authenticated `claude` CLI:
wrote a project `.mcp.json` declaring a server with a nonexistent binary
(`{"mcpServers":{"brokenmcp":{"command":"/nonexistent/does-not-exist-binary"}}}`) in the chat's
actual cwd, forced a fresh agent process (killed the running `claude-agent-acp` subtree so the
next turn would re-read config), and drove a real turn via `surface.chat.send`. The turn
completed cleanly with **no** error card — `looks_like_mcp_warning` never fired because the real
CLI never wrote a matching line to stderr for this failure mode. Verified independently, outside
Tiller: ran the exact SDK-bundled binary Tiller invokes
(`.../claude-agent-sdk-linux-x64/claude`) with matching flags
(`--permission-mode auto --allow-dangerously-skip-permissions --setting-sources=user,project,local`)
directly, stderr captured separately — genuinely empty of any MCP-related text. The failure *is*
real and detectable (`claude mcp list` in the same directory reports
`brokenmcp: ... ✘ Failed to connect — ENOENT ...`), but that text comes from a separate diagnostic
subcommand never invoked during an ordinary turn, not from the turn's own stderr — so the
condition `looks_like_mcp_warning` is built to catch may not be reachable through this CLI's
normal non-interactive turn flow at all, at least for a broken stdio-type server.

Grading `half-proven`, not `FAILED`: the code is genuinely present, correctly wired, and
crate-level tested (a real improvement over the prior "code-absent" state), but I could not
observe the actual user-facing transcript card fire from a real turn despite two independent,
genuine attempts with a real authenticated CLI — the live half of this row remains unconfirmed
either way. A future pass should try triggering the warning through Codex or a different failure
shape (e.g. an MCP server that starts and then fails mid-session) before concluding either way.

*(Aside, not a finding against this slice: getting a clean single-project instance for this test
took two throwaway attempts — `surface.chat.open`/`surface.chat.send` resolved to the
**already-mounted `tiller-linux` worktree's** pre-existing `default-chat` tab even when a
different, freshly-added project's worktree id was passed as `worktree=`, and even from a
completely fresh DB with only one project ever added. The spawned agent's cwd was consistently
`tiller-linux` regardless. This looks like a real cross-workspace routing/cwd bug in the control
surface, but it touches files this slice doesn't own and isn't one of the six rows assigned —
flagging for whoever owns `main.rs`'s chat-tab lookup, not grading it here.)*

## F-SET-15 — UNREACHABLE (reconfirmed, same shape as F-SET-21)

Reconfirmed by code inspection and live capture (`03-ai-providers-forced.png`,
`05-fullscreen-forced.png`, `06-after-cancel.png`): every provider card renders exactly one
"System default / This device / Active" Accounts row, and "Active" is hardwired `true` for it —
there is no code path that produces a second, independently-selectable row for any provider on
this build, by design (single credential slot per provider). The row's VERIFY clause (select
between two accounts, confirm the Active badge moves) has no state to exercise. Not a defect to
fix and not exercisable as written — same disposition as `F-SET-21`'s existing ruling. Marking
`UNREACHABLE` rather than inheriting the builder's "not-attempted"/"pending a design ruling"
framing, since the ledger separates "could not reach" from "ran out of time" and this is
genuinely the former.

## Files touched this pass

None. `git status --porcelain` shows no changes under this slice's owned files
(`rust/crates/tiller_acp/src/lib.rs`, `rust/crates/tiller_markdown/src/{file_events,lib}.rs`,
`rust/crates/tiller_persistence/src/migrations.rs`, `rust/crates/tiller_ui/src/{chat,settings}.rs`,
`rust/crates/tiller_usage/src/account.rs`) — every row was a verification-only pass. A temporary
`.mcp.json` was written to the worktree root for the `F-CHAT-33` live test and deleted before
this file was committed; all scratch Wayland instances, sockets, and `/tmp` scratch project
directories created by this pass were torn down.
