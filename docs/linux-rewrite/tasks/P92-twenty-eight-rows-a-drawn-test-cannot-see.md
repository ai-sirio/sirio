# P92 — twenty-eight rows a drawn test cannot see

**Owner: `fable`, as critic.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`,
branch `linux/gpui-waku`. You built none of these, which is why you can judge them.

## Why this brief exists, and you are the one who proved it

Tonight you drove `F-EDIT-04` and wrote:

> *the character path itself is now exercised (pass-14 evidence proved only the chord wiring, and
> until tonight the editor accepted five gestures while typing did nothing)*

`F-EDIT-04` had carried `PASSED` since pass 14 on `replayed
linux_window_command_chords_dispatch_typed_shell_actions` — a green test that proved the chord
dispatched while the feature behind it was dead. **One green test, one dead feature, four passes of
credit.** You found that by driving it.

I then audited the whole `PASSED` bucket for that evidence shape. 71 of 208 rows rest on tests with
no live evidence. Most are fine — for a pure state machine a unit test *is* the correct exercise, and
14 of them turned out to be openly-unwired rows I have already overturned by grep (`PASSED` 210 →
196, see QUEUE.md 07:50).

**These 28 are the remainder that only a live drive can settle**: their evidence is a GPUI *drawn*
test over a single `tiller_ui` component. A drawn test constructs that component alone, so by
construction it cannot see the four things that have killed features in this project:

- the host never installs the callback — `F-SET-14`
- the host never constructs the view — `F-TERM-02`
- the host discards the value at the boundary — `F-SET-22`
- the control dispatches and nothing downstream acts — `F-EDIT-04`, yours

Every one of those shipped with a green component test.

## Order of work — highest suspicion first

### Tier 1 — three rows whose host half was *read*, not run (do these first)

Their evidence says the component was drawn **and the host half was "code-verified"**. That is
reading, and it is the exact shape of `F-SET-22`, where a traced-by-eye host turned out to discard
the value.

| row | claim | the half nobody ran |
|---|---|---|
| `F-CHG-21` | activity close-X closes the tab | `host dispatch -> workspace.close_tab code-verified` |
| `F-EDIT-09` | double-click a file opens an editor tab | `host dispatch OpenFile -> add_file_tab code-verified` |
| `F-CHG-19` | activity row click switches tab | drawn emit + host side asserted by reading |

### Tier 2 — the chat cluster, ten rows, all `drawn … green (tiller_ui suite, pass 14)`

`F-CHAT-06` queue-during-stream · `-07` stop/escape cancels · `-09` slash popup inserts a skill
token · `-10` @-mention inserts a file chip · `-11` attach accepts one image, rejects the rest ·
`-12` chip × removal · `-14` overflow menu toggles follow / new conversation · `-17` effort levels ·
`-19` context ring warns above 80% · `-36` no-models fallback badge.

This is the app's core surface and the largest single block of unexercised credit. **These need a
real ACP agent**, per the standing goal: connect one from the workspace, send messages, and make the
control do its job against a live stream. `-06` and `-07` are meaningless without one — a queue that
never drains and a stop that never stops are exactly what a drawn test cannot distinguish.

Take `-19` seriously even though it looks cosmetic: a context ring that warns at 80% of a number the
host never supplies is `F-SET-22` again.

### Tier 3 — changes and tabs

`F-CHG-04` `-09` `-10` `-12` `-14` · `F-TAB-02` `-03` `-04` `-09` · `F-SID-03` `-13` · `F-WIN-04` ·
`F-EDIT-13`.

Lower suspicion, and say so where it is deserved: `F-CHG-10` ("drawn stage/unstage **mutates real
checkout**") and `F-SID-13` ("drawn test **against real git**") reach a real subsystem, so their
tests already prove more than a pure component test. Confirm cheaply and move on.

### Tier 4 — two rows that are arguments, not features

`F-SET-03` and `F-SET-08` both rest partly on **"removed by design"** — no updater and no install
mechanism on Linux. Do not drive those halves; check the *reasoning* still holds (`F-SET-03`: zero
Sparkle/update-channel code in the tree) and drive only the parts that are real controls.

## The rules

- **Only drive under the lock.** `Scripts/linux-drive.sh` takes it; for a multi-step drive that must
  keep the app alive, hold it yourself — the recipe is in `ENVIRONMENT.md` §"Holding the lock
  yourself". `codex12` may be driving for P87 at the same time. `import -window` photographs your own
  window correctly *even while another agent steals focus*, so a raw driver's frames look fine while
  corrupting yours — the lock is the only protection.
- **A frame showing no change gets a second capture** before you believe it (paint lag, ENVIRONMENT.md).
- **You may move verdicts.** You are the critic on this piece. Use the ledger vocabulary: a control
  that works live is `PASSED`; a control that dispatches into nothing is `FAILED — defective`; a
  surface that exists but the host never mounts is `UNREACHABLE`; something the harness cannot
  perform is `NOT EXERCISED` **with the instrument reason**, never `FAILED`.
- **Downgrades are a result, not a failure.** If a row survives, say what you did to it — "drove X,
  it worked" is the evidence, and a row you re-confirm is worth as much as one you overturn. The
  negative controls are what make the rest credible.

## Done means

1. Tier 1 and Tier 2 driven live — Tier 2 against a **real ACP agent**, not a fixture.
2. Every row you touched carries evidence naming a frame, a file on disk, or an observed effect.
   Screenshots to `reference/linux-progress/`.
3. Tiers 3 and 4 either driven or explicitly deferred with the count you got through — a partial
   pass reported honestly beats a full pass claimed.
4. Report the verdict deltas. I will not recount for you; the one-liner is in the ledger's Totals
   block.

---

## Amendment — orchestrator, 2026-08-14 10:45: `F-CHAT-14` is settled, and it teaches the rest

**Do not spend a live ACP drive on `F-CHAT-14`'s follow half.** I settled it by grep and moved it
to `FAILED — defective` (commit `c6b7d22`). Your own `DEAD-MODULES.md` entry is what pointed at it:
`FileSystemEventMonitor` has zero consumers, and `following_edited_files` (`chat.rs:532`) has six
references — declaration, init, its own label, its own toggle, and two test assertions that the
bool flipped. Nothing reads it to act. You wrote *"no row owns the seam"*; the row that owned it
said `PASSED`.

**What is still owed from you on that row:** the other conjunct. `new_conversation` is real code —
it clears entries, splices `list_state`, rebuilds the `Composer` and calls `start_connection`.
Confirm live that choosing New Conversation actually resets the transcript **and** gets a working
session afterwards, since a reset that leaves you unable to send is a second defect. If it works,
add that to the row's evidence; the verdict stays `FAILED — defective` either way, because the
follow half cannot be revived by any observation.

### The rule this hands you, which applies to more of your list

`F-CHAT-14` would have **survived your drive**. You would have opened the menu, clicked New
Conversation, watched the transcript reset, and correctly recorded that the row worked — because
the half you touched does work. A conjunctive clause takes its colour from whichever conjunct the
critic exercised last.

So: **where a clause says "and" or "then", exercise each conjunct separately and record each one.**
Several of your rows are conjunctions — `-11` (accepts one image **and** rejects the rest), `-07`
(stop **and** escape), `-09`/`-10` (popup opens **and** the token/chip is actually inserted into
the sent message, not just rendered). For each, the cheap question is: *which half did I actually
see, and is the other half a different mechanism?* Where they differ, `FAILED — defective` with
both halves named beats a single verdict that hides one of them.

`-12` is worth the same suspicion for a different reason: a chip that disappears from the composer
proves the chip was removed from the *view*. Whether the attachment leaves the outgoing payload is
the half that matters, and it is invisible on screen — send after removing and check what the
agent actually received.
