# Verdicts — W03-chat (F-CHAT)

Adjudicated against `docs/linux-rewrite/wave-a/W03-chat-evidence.md` and its captures under
`reference/linux-progress/wavea-W03-chat/`. I did not drive this slice and did not build any of
the surfaces it touches (nor did I drive any other W03 lane — house rule extends "critic never
the builder" to "critic never the driver"). Read-only: source reads (`grep`/targeted line
reads of `rust/crates/tiller_ui/src/chat.rs`, `rust/crates/tiller_acp/src/lib.rs`,
`Scripts/wayland-drive.sh`, `Scripts/wayland-virtual-pointer.c`), no compile, no edits under
`rust/`. Every cited capture was opened with `Read` and inspected directly, including pixel
positions of UI elements against the driver's claimed click coordinates — never taken on prose
alone.

5 of 6 verdicts land where the driver/triage recommended. One (`F-CHAT-18`) does **not** —
see below; this is a disagreement with both the driver's evidence framing and triage's
"reclassify" recommendation, not a rubber stamp.

---

## F-CHAT-13 — drop files into chat to attach them

**Verdict: FAILED — absent** (was NOT EXERCISED; triage's reclassify accepted).

Independently reproduced the driver's two greps (`ExternalPaths` repo-wide under `rust/`: zero
hits; `on_drop`/`ondrop` in `chat.rs`: zero hits) and went one step further: checked whether
`on_drop`/`ExternalPaths` are even real, working GPUI primitives in this codebase, rather than
taking their absence in `chat.rs` as ambiguous. They are — `tiller_terminal/src/lib.rs:1448/1453`,
`tiller_ui/src/right_panel.rs:1932`, `tiller_ui/src/sidebar.rs:2182/2500`, and
`tiller/src/main.rs:5373` all wire real `.on_drop::<T>()` handlers for internal drag types
(`PathBuf`, `RowDrag`, `DraggedPaneDivider`). `ExternalPaths` (GPUI's OS-level file-drop type) has
zero uses anywhere in `rust/crates`, confirmed separately from `on_drop`. So the pattern this
feature would need exists and works elsewhere in this same app — it was simply never built for
the chat pane. Combined with the driver's own reproduction and the prior two independent
live-drive findings cited in the record (cursor-on-'+'-no-popup on X11; wayland pointer injector
has no drag-offer/`wl_data_device` call, confirmed again by my own read of
`Scripts/wayland-virtual-pointer.c`), this is now four independent checks across two lanes and one
direct source read, all agreeing: source-absent, not merely unreached.

Discriminating: yes — a hit on either grep, or evidence that `on_drop`/`ExternalPaths` aren't
real primitives in this codebase, would have falsified the claim.

---

## F-CHAT-18 — context ring popover: fraction/remaining + input/output/cache/cost breakdown

**Verdict: FAILED — absent** (unchanged from ledger — but the evidence underneath it changes
completely, and I am overruling triage's "reclassify" recommendation).

The clause is a conjunction: open the ring, confirm fraction/remaining is shown, **and** confirm
the full input/output/cache/cost breakdown is shown. The breakdown half is decisively,
structurally absent — read `tiller_acp/src/lib.rs:164-171` myself:
`pub struct ContextUsage { used: u64, size: u64, cost: Option<ContextCost> }`. There is no
input/output/cache field anywhere in the ACP layer for this to ever draw, confirmed by a second
grep for those terms near any usage/context type in the same file (zero hits). No click sequence,
however precisely aimed, can ever satisfy this clause without a data-model change upstream of the
UI. This is the same class of finding as `F-CHAT-14`'s dead `following_edited_files` bool — a
source-provable absence that doesn't need a live drive to settle, and it is independently
corroborated by three other documents already in this repo reaching the same conclusion from the
same struct (`PASSED-AUDIT.md` #4, `STALE-FAILED-CENSUS.md`, `RECENSUS-2026-08-14.md`).

Where I part ways with triage: triage's reclassify case rests on the current ledger evidence
("context-indicator click opens nothing") being stale, captured 18 minutes before the
`23af26c` `.h_full()` fix in the same P109 batch as `F-CHAT-02`/`F-CHAT-16`. That's correctly
flagged — I opened `p109/shots/45-context-indicator-click.png` and `55-context-indicator-fresh.png`
myself and confirmed the composer sits pinned to the *top* of the window in both, immediately
below the tab bar, the textbook symptom the fix's own commit message names (the transcript's
virtualized list collapsing to zero height and shoving everything above it up). So yes, that
evidence is suspect and shouldn't be relied on as-is. But triage's fix for this — "re-drive first"
— is what this pass attempted, and it didn't land: I checked the exact render-order source
(`chat.rs:4755-4768`, a `16x16px` `div` immediately before the `{context_percent}%` label inside a
`gap(6px)` toolbar row) against the driver's click coordinates. In the two window sizes actually
captured (`1728px` wide in `02-f18-scan-base.png`/`04-f18-scan-b.png`, `1400px` wide in
`03-f18-scan-a.png`/`05-f18-scan-c.png`), the real ring sits at roughly `x≈985-1120, y≈792-864`
depending on window width; every driven click (`1690,930` / `1660,930` / `1620,940`) lands
consistently in the **Files sidebar**, hundreds of pixels away, in both window sizes. So this
pass's re-drive is not evidence either way for the "does it open" half — it's a clean miss, as the
driver's own write-up concedes. Net: the "opens nothing" half of the old evidence is undermined but
not replaced by anything new and live; the breakdown half is, independently, conclusively absent
and always was. Since the clause explicitly names the breakdown as part of what must be confirmed,
and that part can never be satisfied by any click, the row stays FAILED — absent on that basis
alone, not on the discredited click evidence. A future pass could still usefully re-drive the
"opens with fraction/tokens/cost" half for its own sake (it may well work — the click handler is
real and unconditional, and a green drawn test covers the state transition), but that would only
ever upgrade this row to half-proven at best, never past it, because the breakdown can't exist.

Discriminating: yes for the verdict-determining part (the struct read is a direct absence check
that a breakdown field would have falsified); the click-scan itself is explicitly not
discriminating (confirmed missed-target, not a proof of anything).

---

## F-CHAT-20 — transcript follow + manual-scroll ownership

**Verdict: half-proven** (unchanged).

Independently re-derived, not just re-read: `grep -n "scroll\|axis\|wheel"` over both
`Scripts/wayland-drive.sh` and `Scripts/wayland-virtual-pointer.c` returns nothing — confirmed
this lane's action DSL and its virtual-pointer client have no way to synthesize a wheel/axis
event, matching the driver's claim exactly. Also confirmed the follow-half mechanism directly:
`list_state.set_follow_mode(FollowMode::Tail)` at `chat.rs:939` (constructor) and `push_entry`
(`chat.rs:993-1000`) checking `is_following_tail()` before re-pinning after a splice — real code,
matches the row's proven half. The X11 lane that could supply a scroll primitive is explicitly
out of scope for this slice per the task brief, so the manual-scroll-ownership half stays owed,
not merely unattempted-this-pass.

Discriminating: yes (a scroll/axis hit in either file would have falsified "unreachable"), even
though it reproduces rather than changes the standing verdict.

---

## F-CHAT-25 — AskUserQuestion free-text/option answer, via an alternate CLI

**Verdict: NOT EXERCISED** (unchanged — triage's "exercise" instruction was not actually carried
out this pass).

The driver explicitly chose not to spend this row's budget on a live ACP turn ("a substantial,
possibly-hanging network-dependent drive"), doing a source-only re-check instead
(`Entry::Permission.text_input` at `chat.rs:208`, threading at `chat.rs:1432-1437`,
`render_question_answer_row` at `chat.rs:2748` — all confirmed present by my own read) and
confirming `pi`/`codex` are installed. That's real information but it re-confirms what the record
already had (client wiring exists, tested elsewhere via `F-CHAT-24`) — triage's specific ask, "try
Pi's `ui/select`", was not attempted. The row's core blocker (this build's Claude Code CLI never
raises a real `AskUserQuestion` over ACP) is unchanged and unretested. Per the routing rule ("if
the driver only got partway through the slice, judge the rows it reached and return NOT EXERCISED
for the rest"), this row was reached but not actually exercised, so it stays where it was.

Discriminating: no — the row's own evidence self-reports this explicitly.

---

## F-CHAT-26 — pending-question bar + Show jump

**Verdict: half-proven** (unchanged).

Opened `02-f2627-chat-open.png` (confirms Chat tab genuinely foregrounded — orange underline on
the "Chat" tab, idle empty composer, `idle`/`Opus Plan Mode` — before any send, refuting the
wrong-tab-foregrounded bug this row previously blamed) and `02-f2627-permission.png` (confirms a
real turn ran live: `working` pill, `38%` context, streaming tool output from the `run` skill
across several completed `grep`/`find`/`ls` steps, Stop-square control visible, `Type to queue for
the next turn…` placeholder — and no `Entry::Permission`/question card anywhere in the frame). Both
captures corroborate the driver's account exactly.

This genuinely retires the specific "wrong tab was foregrounded" theory as *the* explanation for
past failures to reproduce this row's visual half — a real, useful, repeatable procedural fix. But
it does not newly prove or disprove the row's actual clause: no permission entry was ever raised
this pass (this Claude Code CLI's default ACP permission mode auto-approved every tool call under
a plain shell-command prompt), so `Show`/scroll-to-reveal was never exercised and the
pending-question-bar visual state was not recaptured. The backend half (`Entry::Permission` proven
live via a plan-mode turn in a prior pass, `chat.rs:1490`) is untouched by this pass, not
re-broken — just not re-attempted via this pass's different, non-plan-mode route. Owed: the same
plan-mode turn that produced the proof previously, now with the tab correctly foregrounded, to
actually reach and photograph the bar.

Discriminating: no, for the row's core visual claim (never reached this pass). The
tab-foregrounding confirmation is itself a real, discriminating finding for a narrower question
(whether tab-state was the blocker) — it was, at least for reaching this route; it wasn't
sufficient to reach the target state, because the CLI's permission mode is the actual current
blocker, not the tab.

---

## F-CHAT-27 — expired unanswered-question state ("No answer — the turn ended")

**Verdict: half-proven** (unchanged).

Same drive, same session as `F-CHAT-26` (shared cause, confirmed by the same two captures). No
permission entry ever appeared under the plain shell-command prompt this pass drove, so
`surface.chat.stop` was never exercised against a pending permission and the expired-state visual
(`control_entry_row`'s "No answer — the turn ended" text, render arm confirmed present at
`chat.rs:3839-3846`/mirror `5448-5449` by my own read) was not recaptured. The prior pass's
data-model proof (`surface.chat.stop` flipping a real pending entry to expired,
`expire_unanswered()` call site `~1479-1512`) is unchanged and untouched by this pass — it wasn't
retested, and nothing here contradicts it.

Discriminating: no — no new evidence either way; recorded as attempted-and-not-reached, matching
the driver's own framing.

---

## Summary

| id | ledger verdict going in | this pass | change |
|---|---|---|---|
| F-CHAT-13 | NOT EXERCISED | FAILED — absent | yes (triage's reclassify accepted) |
| F-CHAT-18 | FAILED — absent | FAILED — absent | evidence basis replaced; triage's reclassify **rejected** |
| F-CHAT-20 | half-proven | half-proven | no |
| F-CHAT-25 | NOT EXERCISED | NOT EXERCISED | no (triage's "exercise" not actually carried out) |
| F-CHAT-26 | half-proven | half-proven | no |
| F-CHAT-27 | half-proven | half-proven | no |
