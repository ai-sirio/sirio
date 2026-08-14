# Verdicts — E03-chat (F-CHAT)

Adjudicated against `docs/linux-rewrite/sweep/E03-chat-evidence.md` and its captures under
`reference/linux-progress/drive-E03-chat/`. I did not drive this slice, did not build any of the
surfaces it touches, and did not compile — read-only source checks (`grep`/targeted line reads)
plus direct inspection of every cited capture (opened with `Read`, two pixel-cropped with
`convert` for exact button-boundary/label checks), never taken on the driver's prose alone.

All five rows' cited captures were opened. Four of five verdicts land where the driver put them.
One (`F-CHAT-24`) moves from NOT EXERCISED to PASSED, matching the driver's claim, but with a
genuine inaccuracy caught in the write-up (see below) that does not change the outcome because of
how the code is structured.

---

## F-CHAT-05 — composer disable while permission-wait or offline

**Verdict: half-proven** (unchanged — offline half stands from pass 17; permission-wait half
still not reached, now with a fuller reason why).

`02-f05-mid.png` confirms the setup exactly as described: a live Claude Code ACP agent completes a
`pwd`-via-Bash turn with no permission gate, composer re-enabled with plain `Message…` placeholder.
`02-f05-write-mid.png` confirms the escalation: a file-write prompt produces an `Edit … Write
/tmp/f05_perm_probe.txt` card reading **Pending** with a red **Revert** link — this is a
tool-call/edit entry, not `Entry::Permission` (no `permission-option-allow`/`-deny` anywhere in the
frame, no Allow/Deny buttons). While that card shows Pending and the pill reads `working`, the
composer placeholder is `Type to queue for the next turn…`, still accepting keystrokes.

This is real, but it does not newly resolve F-CHAT-05's open half. The unit test that names the
row's exact clause (`a_permission_prompt_answers_both_ways`, chat.rs:6690, `"the composer cannot
send while a permission is pending (F-CHAT-05)"`) gates on a genuine `Entry::Permission` with
`resolved: None` — a state this pass never produced with a real agent. What was captured instead is
an adjacent, already-understood state: mid-turn/tool-pending queueing, which is F-CHAT-06's proven
behavior (`"Type to queue for the next turn…"`), not F-CHAT-05's. The permission-wait half stays
open — this build's installed Claude Code CLI auto-applies file edits with a revert affordance
rather than raising the ACP `permission_request` this code path renders, and no prompt tried this
pass could force it to. Offline-half evidence from pass 17 (composer inert, no placeholder) is
untouched and still stands.

Captures used: `02-f05-connecting-send.png`, `02-f05-mid.png`, `02-f05-write-mid.png` — all
confirmed to show what the evidence doc claims. None of them reaches the discriminating state
(a true pending `permission_request` card).

---

## F-CHAT-13 — drag-and-drop / "+" attach control

**Verdict: NOT EXERCISED** (unchanged — reproduced on a second lane, same result).

`02-f13-plus-click4.png` shows the pointer sitting directly on the composer's `+` control (cursor
tip on the glyph, idle/unfocused composer) — the driver's "cursor confirmed on the control" claim
checks out. All four post-click captures (`plus-click2`/`3`/`4` plus `03-f13-plus-click.png`, which
is the file that actually corresponds to the fourth attempt — the evidence doc's own filename list
has a one-off naming slip, `02-f13-plus-click.png` vs the on-disk `03-f13-plus-click.png`, that
does not affect what the frame shows) are pixel-identical to the pre-click baseline: idle composer,
no popup, no dialog. Confirmed independently: `Scripts/wayland-virtual-pointer.c` (read only, not
compiled) implements `motion_absolute`/`button`/`frame` and nothing else — no
`wl_data_device`/drag-offer protocol call anywhere in the file — so a synthetic drag genuinely
cannot be issued from this lane's tooling. The portal-picker/forced-repaint ambiguity the driver
raises is a real open question this pass correctly declines to resolve past. Net effect: same
instrument wall as the prior X-lane pass, now also ruled out on Wayland — still not a code-level
finding either way.

Captures used: `02-f13-before.png`, `02-f13-plus-click2.png`, `02-f13-plus-click4.png`,
`03-f13-plus-click.png` — none discriminating (all show the same idle, popup-free state).

---

## F-CHAT-20 — transcript follow + manual-scroll ownership

**Verdict: half-proven** (unchanged — follow half stands from pass 17; manual-scroll half still
not reached, confirmed unreachable on this lane's current tooling).

`02-f20-mid-stream.png` shows a live numbered-list stream mid-flight (rows ~49–80 visible, viewport
pinned to the growing tail) — consistent with, but not additive to, the follow half already proven
in pass 17. Independently confirmed the manual-scroll half's blocker: `Scripts/wayland-drive.sh`'s
action DSL comment block documents only `click`/`move`/`type`/`key`; grepping
`Scripts/wayland-virtual-pointer.c` for scroll/axis calls finds none — only
`motion_absolute`/`button`/`frame`. No wheel or axis event can be synthesized from this lane
without extending the virtual-pointer protocol client, which this pass correctly declined to do
(no compiling permitted). The row's missing half is genuinely instrument-blocked here, not
avoided.

Capture used: `02-f20-mid-stream.png` — reproduces the already-proven half, does not touch the
missing one.

---

## F-CHAT-24 — plan approval attaches to the Plan card and advances the turn

**Verdict: PASSED** (moved from NOT EXERCISED — first live drive of this code path).

`02-f24-a-plan.png` shows the live mechanism exactly as the row's test name promises: a Plan card
("2 steps", both completed) with a "Ready to code?" approval **attached to the bottom of that same
card** (not a separate card), four option buttons, and a "Question waiting · Ready to code?" bar
below. `03-f24-b-resolved.png` shows the resolution: the option buttons are gone, replaced by
`Answered: Yes, and auto-accept edits` on the same card, followed by "Plan approved. Executing it
now." and a brand-new `Write /tmp/f24_probe.txt` tool card with its own fresh Allow/Deny/Always
permission — the turn genuinely continued past the approval rather than stalling. Both conjuncts
the row's test name names (attaches-to-card, plan-advances) are directly visible in these two
frames, not inferred.

One inaccuracy in the driver's write-up, caught by pixel-checking rather than taken on trust: the
evidence doc states "clicked 'Yes, and manually approve edits'" and cites `click 924 420`. Cropping
`02-f24-a-plan.png` at that button row shows `x=924` lands inside the **"Yes, and auto-accept
edits"** label's bounds (≈817–942px), not "manually approve edits" (≈967–1142px) — and the resolved
frame's text (`Answered: Yes, and auto-accept edits`) confirms the coordinate, not the prose. The
driver clicked and reported a different button than the one they described clicking. This does not
change the verdict: `respond_permission` (chat.rs:2185) is a single generic handler — it sets
`approval.resolved = option.label` and forwards `option.id` to the ACP client for whichever button
was pressed, with no per-label branch in Tiller's own code — so any one of the four options
exercises the identical attach/resolve/advance path the row cares about. Only one of the four
options was trialed live, short of the source clause's "choose each available decision in separate
trials," but the same-handler structure makes that gap non-discriminating for correctness, the same
reasoning the ledger already accepted for F-CHAT-17's partial (3-of-6) effort-picker coverage.

Captures used: `02-f24-a-plan.png`, `03-f24-b-resolved.png` — both discriminating and both verified
against the frame pixels, not just the evidence doc's prose.

---

## F-CHAT-25 — AskUserQuestion free-text answer

**Verdict: NOT EXERCISED** (unchanged — confirmed environmentally blocked, not code-absent).

`02-f25-a-question.png` shows exactly what the evidence doc claims: three `ToolSearch` calls, then
the agent's own plain-text reply, "There's no `AskUserQuestion` tool available in this environment
— it's a Claude Code CLI feature that isn't wired up here. I can't invoke it, structured or
otherwise," followed by the question asked as ordinary chat text. No `Entry::Permission`/question
card, no `AnswerTextInput`, nothing to click — confirmed by the agent's own words, not inferred.
This tells us about this specific installed CLI build's ACP tool surface, not about
`render_question_answer_row`/`answer_question_text`/`cancel_question` (chat.rs), which remain
proven only by the drawn `cancel_on_a_question_closes_it_without_an_answer` test, never live. The
driver did not attempt Pi (`ui/select`, flagged by a source comment as the likely alternate route)
or Codex/opencode this pass, for time reasons — a reasonable follow-up lead, correctly left as a
lead rather than forced.

Capture used: `02-f25-a-question.png` — non-discriminating for the row's actual UI (nothing the row
describes was ever rendered), but conclusive for why: agent-reported tool absence, not a Tiller
defect.

---

## Notes on the driver's blockers list

All three blockers hold up under independent check: `wayland-virtual-pointer.c` genuinely has no
axis/scroll or drag-offer implementation (grepped directly, not taken on the driver's summary);
`Scripts/wayland-drive.sh`'s DSL genuinely exposes only `click`/`move`/`type`/`key`. These are real,
reusable findings for whoever picks up F-CHAT-13's and F-CHAT-20's missing halves next — they
should not re-litigate the same instrument wall from scratch.
