# E04-chat drive evidence (F-CHAT)

Drive lane: Wayland (`TILLER_WL_LABEL=drive-E04-chat`). Captures in
`reference/linux-progress/drive-E04-chat/`. HEAD under test: `4073297`.

## F-CHAT-26 — pending-question bar + Show jump

Ledger row 175, prior verdict NOT EXERCISED ("the pass-8 absent was wrong… unexercised live").

**Drive.** `project.add` this worktree, opened the chat surface, selected it forward
(`tab.select index=2`), then over the socket sent a real ACP prompt designed to make Claude
Code stop for a plan approval: `Enter_plan_mode_and_use_the_ExitPlanMode_tool_to_present_a
_one_step_plan_to_add_a_comment_to_README`. Polled `surface.chat.read` until it returned a
genuine unresolved permission row (`{"id":"1","kind":"permission","status":"pending"}`) — this
is exactly the `Entry::Permission { resolved: None, expired: false }` arm that
`pending_question()` (chat.rs:1490) matches, so the row's own trigger condition was reached
live, not stubbed. Captured `02-pending-state.png` and, from an independent second turn in the
same session, `02-pending2.png`.

**State half — confirmed live.** Measured the composer/bar band (`900x180+400+750` on the
1715x972 frame) with `convert … -colorspace Gray -format stddev/mean`:

| capture | question state | stddev | mean |
|---|---|---|---|
| `02-chat-open.png` | no turn sent, empty transcript (negative control) | 0.00 | 21.0 |
| `02-pending-state.png` | live unresolved permission entry | 60.90 | 43.04 |
| `02-pending2.png` | live unresolved permission entry, second turn | 49.21 | 36.51 |

The negative control is a flat `stddev 0` — nothing drawn in that band when there is no pending
question, so the instrument is awake and the empty case is genuinely empty. Both pending
captures are strongly non-uniform in exactly that band, on two independently driven turns. This
corroborates the chat.rs:1490 comment: the bar's presence tracks `pending_question()` live, not
just in the unit test.

**Gesture half — not exercised.** I have no vision on this session (measurement-only, per
instructions) and could not locate the small "Show" control's exact pixel bounds — GPUI's
`debug_bounds`/`debug_selector` machinery that would answer this precisely is test-only, not
exposed over the live control socket. I clicked two blind guesses on the second pending turn
(`click 1150 850`, `click 1190 895`, captures `03-after-click-1150-850.png` and
`04-after-click-1190-895.png`) but with no ground truth for the button's bounds and no way to
see whether the transcript scrolled, this is not evidence either way — recording the attempt
for the record, not as a claim. Prior X11 evidence (`INVENTORY-LEDGER.md` F-CHAT-23, sweep
A1-P109) found Show/body-click/Stop all no-op on this exact bar in a real click test; I could not
independently confirm or refute that on this lane.

**Claim: partially-exercised.** State half (bar renders exactly while a question is pending)
is freshly, discriminatingly proven live on Wayland. Gesture half (Show scrolls to the card) is
unproven here — the missing half is the click, and it needs either vision or `DISPLAY=:1` with a
known-good pixel target to close.

## F-CHAT-27 — expired unanswered-question state ("No answer — the turn ended")

Ledger row 176, prior verdict NOT EXERCISED ("the pass-8 absent was wrong… unexercised live").

**Drive.** Same session as above, immediately after confirming a live `pending` permission
entry (`02-pending-state.png`), sent `surface.chat.stop surfaceId=default-chat` over the
socket — the same stop path the composer's Stop button uses (`control_stop`, mirrors the
rendered control per the codebase's own doc comment on `control_send`/`control_permission`).

**Result — confirmed live.** `surface.chat.read` immediately after the stop returned:

```
{"id":"1","kind":"permission","status":"expired"}
{"kind":"turn","text":"22:09 · cancelled"}
```

`status` flipped from `"pending"` to `"expired"` in `control_entry_row` (chat.rs:5448-5449),
which is driven directly by the same `expired` field the render arm reads at chat.rs:3839-3846
(`else if expired { … "No answer — the turn ended" }`) — this is the live analogue of the
`a_question_whose_turn_ends_unanswered_expires_instead_of_waiting` unit test (chat.rs:4625),
now reproduced against a real ACP turn instead of a fixture. A `TurnFooter` entry
(`"22:09 · cancelled"`) was appended in the same transition, matching `expire_unanswered()`
being called from the `TurnEnded` arm (chat.rs:1479) before the footer is pushed. Screenshot
`03-after-stop.png` captured the post-expiry frame (13366 colours, non-blank).

**Claim: exercised-working.** Both branches of the row's own description (a question exists
unanswered; the turn ends; the card marks itself expired instead of staying stuck) were
produced by a live drive, not inferred from source reading.

## F-CHAT-33 — MCP-configuration-warning half

Ledger row 182, prior verdict half-proven: the turn-error half is proven live (pass 17,
killing the ACP subtree mid-stream); the MCP-configuration-warning half was never triggered.
`ADJUDICATION-BACKLOG.md` already flagged this half as unnameable: "no turn-error/MCP-warning
rendering found in chat.rs beyond `attach_error`. Cannot name a control."

**What I drove.** Read `chat.rs` end to end for anything MCP-shaped rather than trying to
provoke an MCP server misconfiguration blind (the recipe doc itself says report unreachable
"if none is installed" rather than improvise one — and there is no MCP server configured in
this worktree to misconfigure). `grep -in mcp rust/crates/tiller_ui/src/chat.rs` returns exactly
one hit, a doc comment about MCP tool *output* size (`F-CHAT-23: … large MCP responses`), not
about a configuration-warning UI. The error rendering path has a single `ErrorKind` variant:

```rust
enum ErrorKind {
    Connection,
}
```

Every place that constructs `Entry::Error` in the file (six call sites: chat.rs:502, 1496,
1512, 1836, 2130, 2548/2563, 7127/7156) passes `ErrorKind::Connection`. There is no second
variant, no `Mcp`/`Configuration`/`ToolConfig` kind, and no separate render arm keyed on one —
confirmed by reading the full `Entry::Error` render arm alongside the type definition, not by a
single grep. Repo-wide, `grep -rln mcp rust/crates --include=*.rs -i` (excluding tests) matches
only this one file, this one doc-comment line.

**Claim: could-not-reach — the code path does not exist to drive.** This is not "I couldn't
find a way to trigger it live"; it's that the transcript's error rendering has exactly one kind
and it is not MCP-configuration-shaped. The turn-error half stays proven per pass 17; the
MCP-configuration-warning half stays owed because there is nothing in the current binary for a
drive to reach — a defect/absence finding, not a coverage gap. (I did not edit any source to
verify this, per the no-compile constraint; this is a read-only confirmation of the
already-recorded backlog note, cross-checked against `4073297`.)
