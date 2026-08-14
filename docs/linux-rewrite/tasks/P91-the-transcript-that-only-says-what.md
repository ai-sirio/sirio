# P91 — the transcript that says *what* but never *what happened*

**Owner: `sonnet`, as builder.** Start it when your P82 verification is finished. Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

`chat.rs` and `tiller_acp` have no other claimant — every commit touching them is a recovery or
checkpoint commit — so this piece is yours end to end. You did not build either, which is also why a
critic can judge it afterwards.

Six ledger rows sit on one root cause: **the transcript renders that a tool ran, never what it did.**
`F-CHAT-21`, `-22`, `-23`, `-28`, `-31`, `-32`.

## The seam, and why it is the whole point

I traced this before writing the brief, because building the visible half against data that never
arrives is the single most common way work here dies.

`tiller_acp/src/lib.rs:1198` maps the protocol's tool-call update like this:

```rust
SessionUpdate::ToolCall(tool) => vec![AcpEvent::ToolCallStarted {
    id, title, status                       // <- three fields, and the rest of `tool` is dropped
}],
```

`AcpEvent::ToolCallStarted` / `ToolCallUpdated` / `ToolCallCompleted` (`lib.rs:258-287`) carry only
`{id, title, status}`. The update path in `chat.rs:706-726` therefore merges only a title and a
status, and `Entry::ToolCall { title, status, .. }` renders a rail card holding exactly those two
strings.

**So three of the six rows are blocked in the ACP layer, not the UI.** The protocol's `ToolCall`
value carries considerably more than three fields — it is a generated type, and I deliberately did
not enumerate its fields from a stale registry copy rather than hand you names I had not verified.
**Read the type yourself** (`agent_client_protocol` 2.0.0, as used at `lib.rs:1198`) and report what
it actually offers — content, kind, locations, raw input/output, or whatever it turns out to be. That
list determines what rows 23, 31 and 32 can honestly show.

**Do not invent the data.** If the protocol does not carry a diff, then `F-CHAT-31` is not buildable
today and the honest deliverable is a written finding saying so. A plausible-looking diff card fed by
nothing is worse than an absent one, because it survives being looked at.

## Part 1 — `F-CHAT-21`, and it is nearly free

Thinking already works end to end: `AcpEvent::ThoughtChunk` arrives, `chat.rs:695-700` accumulates
consecutive chunks into one `Entry::Thought(String)`, and `chat.rs:2438` renders it as italic body
text. **The data is there and the entry is live.** What is missing is only the control: the thought
renders permanently and fully expanded, with no way to collapse it.

Add expand/collapse with a collapsed summary. This is the cheapest row in the queue — do it first, so
the piece has something landed before the harder parts.

## Part 2 — `F-CHAT-23`, `-31`, `-32`, in that order

They share a spine: widen the ACP event, widen `Entry::ToolCall`, then render.

- `F-CHAT-23` — a tool call should show its detail, not just title and status.
- `F-CHAT-32` — an edit summary for a tool call that changed files.
- `F-CHAT-31` — a diff preview in the transcript.

Widen the seam once and all three land on it. Do the ACP half first and prove it with the event, not
with the UI: a test that asserts the mapping carries the new fields is the right first commit.

## Part 3 — `F-CHAT-22` and `-28`, investigate before building

`F-CHAT-22` is grouped-step expansion; `F-CHAT-28` is subagent task cards. Before writing UI for
either, establish whether the protocol delivers anything to group or any notion of a subagent. If it
does not, say so plainly and leave the rows alone — a finding is a real deliverable here and these
two are the likeliest of the six to be genuinely unbuildable.

## The rules that apply to every line

- **Inspiration, never code.** waku, Zed and orca are references to look at, not to copy from. A
  critic finding transplanted code counts as a gap, always.
- **Code plus a green test is `NOT EXERCISED`, never `PASSED`.** You are building, not judging.
- **Do not edit `INVENTORY-LEDGER.md`.** Report what you built and what you exercised; a critic moves
  the verdicts.

## Done means

1. `F-CHAT-21` landed and driven live — a real agent, a real thinking block, collapsed and expanded.
2. The ACP event widened, with the field list you found reported explicitly.
3. Whatever of `F-CHAT-23`/`-31`/`-32` the protocol actually supports, built and exercised against a
   **live ACP agent** — connect one from the workspace, send a message that makes it use a tool, and
   show the transcript rendering the result. The standing goal is explicit that a feature the critic
   has not successfully tried does not exist; the same standard applies to your own claim.
4. Parts 3's two rows either built or reported unbuildable with the evidence.
5. `cargo fmt`, suite green on what you touched, and `git status --short | grep '??'` before you
   finish — `tiller_terminal` currently has two untracked module files that `lib.rs` already
   declares, which is exactly the trap this check exists for.

## Builder report — sonnet, 2026-08-14

**1. `F-CHAT-21`.** Landed at `396dd27` — `Entry::Thought` carries its own `expanded: bool`;
collapsed shows a capped one-line summary, a click toggles it live or after the turn ends.
Exercised by GPUI tests in `chat.rs` that construct a thought entry and simulate the click. I did
not re-drive this against the real windowed app this session — the shared display lock
(`/tmp/tiller-drive-1.lockd`) was held by another agent's live session throughout (see item 3) — so
this claim rests on the GPUI test harness's simulated click/render cycle, not a screenshot. Flagging
that distinction explicitly rather than letting "landed and driven live" imply more than was checked
this round.

**2. Field list found.** Read `agent_client_protocol-schema-1.5.0`'s `ToolCall` /
`ToolCallUpdate` / `ToolCallUpdateFields` directly (not a stale registry copy). Beyond
`{id, title, status}`, the protocol carries:

- `kind` — an enum (`Edit`, `Read`, `Delete`, `Move`, `Search`, `Execute`, `Think`, `Fetch`,
  `SwitchMode`, `Other`); no `Task`/`Agent`/`Subagent` variant exists anywhere in this enum, which
  bears on item 4.
- `content` — a list of `Text` or `Diff { path, old_text, new_text }` entries.
- `locations` — `{ path, line }` pairs, the files/lines the tool touched.
- `raw_input` — the tool's raw JSON arguments.
- `raw_output` — the tool's raw JSON result. **Caught late**: this one was missed in the first
  widening pass despite the brief pairing it with `raw_input` explicitly. Added this session at
  `f3069d6`, symmetric with `raw_input` end to end (`AcpEvent` → `Entry::ToolCall` → render).
- `meta` (`_meta`) — present on both types, typed as `serde_json::Map<String, Value>`. The schema's
  own doc comment says implementations "MUST NOT make assumptions about values at these keys." Not
  widened into `AcpEvent` — investigated instead (item 4) because the brief's Part 3 question is
  exactly whether this bag carries anything usable, and it does not for the case that matters here.

All five typed fields are widened through `AcpEvent::ToolCallStarted/Updated/Completed`
(`lib.rs`), `Entry::ToolCall` (`chat.rs`), and rendered by `render_tool_call_card`. Test:
`started_tool_call_carries_kind_content_locations_and_raw_input_and_output` (`tiller_acp/src/lib.rs`).

**3. Live-agent verification (`F-CHAT-23`/`-31`/`-32`).** Connected a real agent from the
workspace — `npx @agentclientprotocol/claude-agent-acp@latest`, bridging to the installed `claude`
CLI — through the production `AcpClient::launch` / `notification_to_events` path (a temporary
`eprintln!` probe on `tool.meta`/`update.meta` was added for item 4's investigation and fully
reverted; `git diff` against `f3069d6` is empty). Prompted it to write a file then read it back,
which produced two consecutive real tool calls (Write, then Read), 9 tool-call-lifecycle events
total, ending in a normal `TurnEnded { stop_reason: "EndTurn" }`. Observed directly in the Debug
output:

- `kind`: `"Edit"` for the write, `"Read"` for the read.
- `content`: a real `Diff { old_text: None, new_text: "hello from ACP\n" }` mid-write, then a real
  `Text("```\n1\thello from ACP\n```")` on the completed read.
- `locations`: the real temp-file path, with `line: Some(1)` once the read resolved it.
- `raw_input`: progressively-populated real JSON (`{}` → `{"file_path": …}` → `{"file_path": …,
  "content": …}`).
- `raw_output`: `None` on `Started`/most `Updated` events (the tool hasn't returned yet), populated
  on `Completed` with the tool's real return value (e.g.
  `"1\thello from ACP\n2\t"` for the read).

This is the exact seam this task widened, driven by a real subprocess rather than a fixture, and
it produced correctly-shaped, real data at every widened field. Full transcript:
`/tmp/claude-1000/-home-enzopalmisano--claude/993e51a1-270a-49d7-b90e-35d74aeac1e0/scratchpad/p91-live-check-2.log`.

**What this does not cover:** actually watching `chat.rs` paint these events on the real, windowed
app. The rendering logic itself is exercised by GPUI tests using data shaped identically to what
this live run produced (`Diff`/`Text` content, real locations, both raw fields) — but that is a
render test against realistic fixtures, not a screenshot of the live pipeline end to end. I did not
force this: `/tmp/tiller-drive-1.lockd` was held (`pid=1101962 label=fable-p92-held`, alive,
43+ minutes) by another agent's own live-driving session for the full duration of this work, and
the project's own environment notes are explicit that contending for that lock corrupts the other
holder's input and is the expensive kind of false negative. Recording this as a real, named gap
rather than asserting a screenshot I did not take.

**4. Part 3 — `F-CHAT-22` and `F-CHAT-28`.**

- `F-CHAT-22` (grouped-step expansion): **built**, at `2638338`. The protocol carries no group id,
  but grouping consecutive `Entry::ToolCall` entries by transcript adjacency is real structure, not
  invented data — every grouped entry is still a real, individually protocol-sourced tool call, and
  the live run above produced exactly this shape organically (two consecutive tool calls from one
  prompt, no synthetic setup needed to get a run of length 2). Collapses to one "N steps" header by
  default; a `group_expanded: bool` on the run's tail entry toggles every member to its own full
  card. Pure-function boundary detection (`tool_call_run_bounds`) plus one GPUI end-to-end test
  (`consecutive_tool_calls_group_under_one_toggle_and_expand_to_full_cards`) cover it.
- `F-CHAT-28` (subagent task cards): **unbuildable with today's evidence — leaving the row alone.**
  Two independent signals, both gathered live rather than assumed from the schema:
  1. `ToolKind`, the protocol's own typed vocabulary for what a tool call *is*, has no
     `Task`/`Agent`/`Subagent` variant — see item 2. There is no protocol-level concept of "this
     tool call is a subagent" to hang UI on.
  2. `_meta`, the one open extension point, was probed live across both runs (three tool calls, four
     distinct `_meta` snapshots). Every one carried only implementation tags —
     `{"claudeCode": {"toolName": "Write"}}`, once with `"nonExecutionKind": "permission-rule"`, and
     once with a full `toolResponse` echo — never anything resembling a parent/subagent identifier.
     This matches the SwiftUI original's use of a Claude-specific `parent_tool_use_id` that the
     generic ACP schema has no equivalent for, and confirms this specific bridge doesn't fill the
     gap through `_meta` either, at least not for ordinary (non-Task-tool) calls, which is all this
     session's prompts exercised — I did not specifically try to force the agent to spawn a Claude
     Code subagent (its `Task` tool), so this finding covers "ordinary tool calls carry no subagent
     data," not "no prompt could ever surface one." Recorded as the honest scope of what was
     actually tried, per the brief's own standard.

**5. Verification.** `cargo fmt -p tiller_ui -p tiller_acp -- --check` clean. `cargo test -p
tiller_acp`: 11 unit + 6 integration passed, 1 ignored (needs live credentials, pre-existing).
`cargo test -p tiller_ui --lib`: 246 passed, 0 failed. `git status --short | grep '??'` shows no
untracked files under `chat.rs` or `tiller_acp` scope; the previously-documented `tiller_terminal`
untracked-module trap is not present in the current tree. The temporary investigation example
(`tiller_acp/examples/p91_live_check.rs`) was deleted; the temporary `_meta` `eprintln!` probes in
`lib.rs` were fully reverted (`git diff` against `f3069d6` is empty).

## Live UI pass — sonnet, 2026-08-14 (continued)

The lock freed up. This closes the gap item 3 above named explicitly: actually watching `chat.rs`
paint these events on the real, windowed app, not just GPUI tests against realistic fixtures.

**Setup.** Held `/tmp/tiller-drive-1.lockd` (`label=sonnet-p91-live-ui`) for the full session,
launched the real `target/debug/tiller` binary directly (not `linux-drive.sh`'s one-shot model,
since a live chat turn takes longer than a single launch-act-capture-kill cycle), and drove it with
a standalone `xdotool`/`import` harness scripted to match the script's own click/type/key/screenshot
helpers.

**Blocker found and worked around, not fixed: the restored session's "Browser" tab's native
webview child window never unmaps.** The session restored with several stale `Browser` panes open.
Switching tabs, closing every `Browser` entry in both the sidebar and the top tab strip — confirmed
by `Files` and `Terminal` panes visibly responding underneath — still left the exact same "Example
Domain" content painted over the main viewport. `xwininfo -root -tree` on the app's window showed
why: an XEmbed-style child window (`0x800024`, 850×792) sits under the app's own top-level window,
holding the *entire* content pane's screen region, and it was still mapped after its owning tab was
closed through the UI. This is one level worse than what `ENVIRONMENT.md`'s existing P72 note says
("a native child window sits above the GL surface and cannot be reordered") — it isn't just
un-reorderable, closing its tab doesn't unmap it either. Worked around for this session only by
`xdotool windowunmap 0x800024` directly, which is not a real fix and doesn't belong in this task's
scope — flagging it here since whoever owns P72 will want the extra detail (the exact window ID
pattern and that tab-close doesn't clean it up), not editing that row myself per the standing rule
that a critic moves verdicts, not the builder.

**F-CHAT-22 (grouped tool calls) — confirmed on screen.** Prompted the live agent (same
`@agentclientprotocol/claude-agent-acp` bridge as item 3, this time through the actual composer, not
a Rust example) to read two files. The transcript rendered a collapsed "2 steps" header immediately
expandable to two individually-labeled `Completed · Read <path>` rows — real grouped-run rendering
from two real, consecutive tool calls, on screen.

**F-CHAT-23 / F-CHAT-31 / F-CHAT-32 — confirmed on screen.** Prompted the agent to write a scratch
file, edit one line in it, then read it back — a real Write → permission gate → Edit → permission
gate → Read chain, each step approved live via the "Allow Once" button in the permission-request
card. The expanded `Edit` tool-call card rendered a genuine colored diff:

```
/tmp/p91-edit-test.txt
    hello p91
-   world
+   world edited
```

red `-`/green `+` lines, driven by a real `Content::Diff` from the live agent, not a fixture. This is
on-screen evidence for the detail card (23) and the diff preview (31) at once; I did not find a
separate distinct "summary" affordance beyond this expanded card (no separate "+1/-1" badge in the
collapsed header), so I'm treating the diff card itself as satisfying 32 too rather than asserting a
UI element I didn't actually see.

**F-CHAT-21 (thinking collapse/expand) — attempted live, not landed.** This is the one row this pass
could not close. Sent four separate prompts designed to force a visible thinking block — two with
"think hard" / "ultrathink" triggers under the session's default `Opus Plan Mode`, two more after
switching the composer's model selector to plain `Default (recommended)` at `XHIGH` reasoning effort
— and in all four turns the agent answered directly with no distinct collapsible thought entry
rendered anywhere in the transcript. The connected agent simply never emitted a `ThoughtChunk` this
session, under any model/effort/trigger combination tried. That means the collapse/expand control
itself was never exercised against a real thought this pass; the GPUI unit-test evidence cited in
item 1 above (constructed thought entry, simulated click) remains the only exercised evidence for
this specific row. Recording this as a real, attempted-and-missed gap rather than either asserting a
screenshot that doesn't exist or repeating the earlier "did not force this" framing — this time the
lock was held, the agent was live, and the feature still didn't surface.

**Teardown.** App process killed, `/tmp/tiller-drive-1.lockd` removed, after this addendum was
written.
