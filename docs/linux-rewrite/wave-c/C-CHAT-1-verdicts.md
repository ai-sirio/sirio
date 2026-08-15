# Wave C slice C-CHAT-1 — verdicts

Critic pass. No code edited (independent-critic house rule; `git status --porcelain` clean under
this slice's owned files at the end of this pass). The builder's report and `INTEGRATION.md` were
read only for routes and for the exact diffs they claim landed — neither was accepted as proof.
Every row below was re-driven live against the real binary (`rust/target/debug/tiller`, built at
HEAD) on fresh Wayland-lane instances (`Scripts/wayland-drive.sh`, labels `critic-cchat1` /
`critic-auth2` / `critic-f14fresh`), using a real, authenticated `claude-agent-acp` (this box's own
`~/.claude/.credentials.json`) and a small fixture ACP-agent shell script for the one row that
needs an auth failure. All scratch Wayland instances, sockets, `/tmp` scratch git repos, and the
one fixture script were torn down/left in `/tmp` (never under this worktree) before finishing.

**One safety note, not a row finding:** an early combined-invocation drive for the fresh-tab half
of the `F-CHAT-14` check raced `ctl workspace.select` against an immediate UI click and the agent
ended up editing this worktree's own `README.md` instead of the intended `/tmp` scratch repo
(appended a marker line, mtime confirms). Caught immediately via `git status --porcelain` and
reverted with `git checkout -- README.md` before writing anything here; `git log` shows no stray
commit. This is the same `surface.chat.open`/`.send` cross-workspace cwd-routing bug
`C-CHAT-3-verdicts.md`'s `F-CHAT-33` section already flagged for whoever owns `main.rs`'s chat-tab
lookup — not one of this slice's six rows, not re-litigated here, but worth the warning for any
sibling critic who packs `workspace.select` and a same-invocation chat turn together without a
settle gap. The primary (non-raced) `F-CHAT-14` evidence below used a properly-settled
`workspace.select` in its own prior invocation and edited exactly the intended scratch file.

## F-CHAT-15 — PASSED

Code check: `mode_catalog`/`mode_picker_open`/`toggle_mode_picker`/`select_mode` are real
(`chat.rs:881-2032`), the status pill's `on_click` is gated `when(mode_selectable, ...)` where
`mode_selectable = self.has_completed_turn && self.mode_catalog.is_some()` (`chat.rs:4894`), and
`mode-option-<id>` rows exist (`chat.rs:5257`). Fixture test
`mode_picker_selects_an_agent_advertised_mode_and_updates_the_pill` passes (58/58 in `chat::`).

Live drive, instance `critic-cchat1`: added a scratch project, opened a default (Claude Code ACP)
chat tab, sent a real turn over `surface.chat.send` ("Say hi in one word", real reply "Hi!").
Contra the report's claim that "Claude/Codex/Pi ACP bridges observed so far don't [advertise
modes]" — this box's real `claude-agent-acp` does. Post-turn the status pill read **"Auto ⌄"**
(discriminating: pre-turn/pre-connect it read plain "idle"/"connecting" with no chevron, matching
`mode_selectable`'s gate). Clicked the pill through the virtual pointer: a real dropdown opened
listing six live agent-advertised modes — **Auto, Manual, Accept Edits, Plan Mode, Don't Ask,
Bypass Permissions** — with "Auto" highlighted as current (frame `03-after-pill-click.png`). This
is the actual `ModeCatalog` from the live session, not a fixture. Did not additionally re-confirm
the "selecting a row updates the label and closes the dropdown" half live (two follow-up attempts
mistimed the click against turn-completion state); that half rests on the passing fixture test
plus the `select_mode` code read, which is a materially weaker leg than the dropdown-opens half.

## F-CHAT-02 — PASSED

Code check: `ErrorKind::AuthRequired`, `classify_connection_error`, and the
`chat-auth-required-banner` debug selector are real and wired through both the launch-failure and
`AcpEvent::TransportError` paths (`chat.rs:517-549`, `4386-4409`). End-to-end fixture test
`auth_required_launch_gets_a_dedicated_banner_with_login_guidance` passes, driving a real
subprocess that rejects `session/new` with wire code -32000.

Live drive, instance `critic-auth2`: reused that exact fixture shell script (same JSON-RPC stub
lines as the passing test) as `TILLER_ACP_PROGRAM` against a real scratch project, and launched a
chat tab through the real "+ → New Chat" UI menu (not a control-socket shortcut, not the test
harness). The rendered card is amber (not the generic red connection card), reads **"could not
launch ACP agent: ACP agent requires authentication (Login)"**, and below it: **"Sign in from a
terminal using this agent's own CLI (for example, its `login` subcommand), then Retry — Tiller
cannot complete a…"** — exactly the row's claimed guidance sentence, genuinely rendered pixels
(frame `04-chat-view.png`), not inferred from the test. Status pill correctly reads "offline".

## F-CHAT-14 — FAILED — defective (integrator's applied fix does not resolve the live behaviour; a second, deeper gap was found in the same area)

`INTEGRATION.md` §4 claims commit `620b9a3` (`TillerWorkspace::new` now loops `bind_chat` over
restored `TabContent::Chat` tabs, mirroring the existing `Changes`-tab loop) closes this exactly as
the builder's report described. The diff is real and correctly placed (`main.rs:2562-2568`,
unconditional, iterating the same `tabs: Vec<OpenTab>` parameter `restore_tabs` produces) — but a
live drive against the actual fixed binary shows the user-facing bug is still there.

**Live reproduction, instance `critic-cchat1`, scratch project `/tmp/f14repo`:** created a chat tab
via the real "New Chat" UI menu, then killed and relaunched the app against the *same* DB three
times in a row (a genuine process restart each time, not `session.restore` — the `TillerWorkspace::new`
path the fix targets) to force the tab through `restore_tabs`. Confirmed restored/reconnected each
time (tab survives, session resumes, mode pill later reads "Auto"). In one continuous session after
the final restart: opened the overflow menu, clicked **Follow Edited Files** (menu re-opened to
confirm — label read **"Stop Following"**, i.e. genuinely on), then sent a real turn over
`surface.chat.send` asking the live agent to append a marker line to `README.md`. The transcript
shows `Completed · Edit README.md`; `cat /tmp/f14repo/README.md` independently confirms the real
edit landed (`hello` / `F14_PROOF_MARKER`); the Files panel shows a `Diff` badge on `README.md`.
**No file tab opened** — the tab strip still shows only `Chat` and `Terminal` (frame
`02-after-turn2.png`), exactly the bug the row and the builder's own report describe, reproduced
against the code the integrator says fixes it.

Re-read the diff and its surrounding call graph looking for why: `restore_tabs` (the app-startup
function feeding `TillerWorkspace::new`, `main.rs:7821`) and `restore_tabs_in_workspace` (a
**separate**, near-duplicate function used by the `session.restore` control method's handler,
`restore_launch_snapshot`, `main.rs:3742-3778`) both independently build `TabContent::Chat` via
`Chat::launch_with_command`. The integrator's loop only covers the first (cold-start) function's
output. `restore_launch_snapshot` calls only `Self::bind_terminal_tabs(&tabs, cx)` after
`restore_tabs_in_workspace` — **no matching chat loop was added there** — so a chat tab recovered
via the `session.restore` control method (the row's own report explicitly names this as an
equally-valid drive: *"quit and relaunch Tiller, or use `session.restore`"*) reproduces the
identical bug through a second, entirely unpatched code path. This second gap is confirmed by
direct code reading, not live-driven separately (the live reproduction above already proves the
row fails end to end without needing it) — flagging it so whoever picks this row up next fixes
both call sites instead of re-discovering the same bug in the other one.

## F-CHAT-18 — FAILED — absent (re-confirmed against the vendored protocol source directly)

Read `agent-client-protocol-schema-1.5.0/src/{v1,v2}/client.rs` directly (not just `tiller_acp`):
`UsageUpdate` (the wire type behind `SessionUpdate::UsageUpdate`) carries exactly `used: u64`,
`size: u64`, `cost: Option<Cost>` — no input/output/cache-token fields, in either schema version.
The `input_tokens`/`output_tokens`/`cached_read_tokens` fields the row wants live only on an
unrelated `Usage` struct (`v1/agent.rs:3366` / `v2/agent.rs`), gated
`#[cfg(feature = "unstable_end_turn_token_usage")]` and describing a different message
("end of turn", not "context window"). `tiller_acp::ContextUsage` (`lib.rs:210`) faithfully mirrors
everything `UsageUpdate` actually carries. The report's claim is exact, word for word against the
source. Confirmed by direct source instrument, not by re-reading the report's own words.

## F-CHAT-05 — half-proven (unchanged)

Code re-confirmed: `can_send()` (`chat.rs:1628`) independently checks `pending_question().is_none()`
rather than folding it into `streaming`, matching the row's own comment about not conflating a
permission-wait with a streaming state. This box has exactly two live ACP programs
(`claude-agent-acp`, authenticated; `codex-acp`, not authenticated per the report) and Claude's own
global `defaultMode: auto` is not prompt-dependent, so the specific live scenario the row's route
asks for (an agent whose mode genuinely differs from Claude's default, driven through a real
gating turn) remains undriveable on this box's available agents. Not re-attempted live this pass;
grading held at the builder's own `half-proven`, not re-litigated with weaker evidence.

## F-CHAT-13 — UNREACHABLE (re-confirmed)

`ENVIRONMENT.md`'s XDND section (`xdotool` has no drag-source, direct read) and the Wayland
`wayland-virtual-pointer.c`/`wayland-drive.sh` DSL (only `click`/`move`, no drag-offer protocol,
confirmed by grep) both still lack any XDND primitive on this box. Same disposition as the
builder's own record — a harness limitation, not a code gap, and not exercisable from here.

## F-CHAT-20 — UNREACHABLE (re-confirmed)

Grepped `Scripts/wayland-drive.sh` and `Scripts/wayland-virtual-pointer.c` for a scroll/axis
primitive — zero hits, confirming the report's claim directly rather than trusting it. No X11-lane
attempt made (out of scope for this slice per the row's own note). Same disposition as the
builder's record.

## Files touched this pass

None under this slice's owned files. `git status --porcelain` at `rust/crates/tiller_acp/src/lib.rs`,
`rust/crates/tiller_markdown/src/{file_events,lib}.rs`, `rust/crates/tiller_persistence/src/migrations.rs`,
`rust/crates/tiller_ui/src/{chat,settings}.rs`, `rust/crates/tiller_usage/src/account.rs` is clean —
every row was verification-only. The one incidental edit to this worktree's own top-level
`README.md` (see safety note above) was reverted with `git checkout -- README.md` before this file
was written; `git status --porcelain` on that path is clean.
