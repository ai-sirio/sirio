# P117 — a completed chat turn draws nothing

**Owner: `pi`, as builder** (reassigned 18:15 — see the addendum; `codex11` is out of credit and
never started). Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch
`linux/gpui-waku`. You did **not** build the chat, which is why you get to fix it and why
**you do not get to judge the result** — `pireview` and the orchestrator do.

## The defect

A chat turn completes end to end. Every piece of chrome around it updates correctly. **The
transcript itself draws nothing.**

Reproduced twice, with different messages, on a binary built 17:34 that includes every chat commit
through `c23da36`:

```bash
TILLER_WL_LABEL=orchvis3 Scripts/wayland-drive.sh /tmp/orchvis3-shots '
  ctl project.add path=/home/enzopalmisano/Scrivania/Progetti/tiller-linux
  ctl surface.chat.open >/dev/null
  ctl tab.select index=1
  ctl surface.chat.send surfaceId=default-chat text=Reply_with_the_single_word_ORCHVIS
  sleep 70
  ctl surface.chat.read surfaceId=default-chat
  shot chat-70s
'
```

`surface.chat.read` at 70 s returned, verbatim:

```json
{"composerText":"","queuedText":"","status":"completed","surfaceId":"default-chat",
 "transcript":"[{\"kind\":\"user\",\"text\":\"Reply_with_the_single_word_ORCHVIS\"},
                {\"kind\":\"assistant\",\"text\":\"ORCHVIS\"},
                {\"kind\":\"turn\",\"text\":\"17:39\"}]"}
```

The agent ran and answered. In the frame taken straight after that read
(`/tmp/orchvis3-shots/02-chat-70s.png`) the Chat tab carries a completed-turn ✓, the composer has
reset to `Message...`, the mode pill reads `Ask`, the model pill `Opus Plan Mode XHIGH`, the context
ring `5%`, and the send arrow is back. Mid-turn the same chrome read `● working`, `40%`, a stop
square, an orange tab dot and `Activity 1 running`.

And the transcript area is blank. No user message, no `ORCHVIS`, no `17:39`. The composer sits at
the top of the pane with ~700 px of nothing beneath it — the layout the pane uses when it believes
it has no entries.

**This is why no test caught it.** Everything a drawn test asserts on is correct.

## What that costs

Every row whose evidence is "the transcript shows X" is unprovable until this is fixed. That
includes the rows you built in `P114` — a hover Copy button and a code-block Copy button cannot be
reached if no message is drawn to hover.

## The one hypothesis, offered and not asserted

The rendered chat's **runtime status** clearly does follow the socket, so `P107` connected
something real. Perhaps status is taken from the ACP session while entries are appended to a
collection the view never reads. **Test it, do not take it.** If you find a different cause, that
finding is worth more than a fast fix on mine.

## The second item — the same smell, one surface over

`surface.changes.open` loads real data: the frame shows `Local changes (50)`, a `Changed (3)` group
with per-file `−11 +146` counts, and an `Untracked (47)` group. But the list is **clipped to roughly
150 px**, cutting the `Untracked` header mid-row and leaving ~600 px empty below
(`/tmp/orchvis2-shots/03-changes-surface.png`). The `surface.changes.open` reply two seconds earlier
read `loading:"true", ready:"false"` with all counts `0`.

Two scrollable content regions — one drawn at zero height, one at a height that looks computed
before its async load arrived. **Establish whether it is one root cause or two before you fix
either.** If it is one, say so plainly; that is the most valuable sentence you can write today.

## How this gets proven

**A green test does not close this.** That is the rule the project exists to enforce, and this
defect is the proof of why: 134 tests pass over a transcript that draws nothing.

- Fix it, then **re-run the exact drive above** and say what `chat.read` returned.
- You cannot see the frame. **The orchestrator will read the capture** — that is the acceptance
  gate, so leave the shots in place and name their paths.
- Add whatever test you think belongs, but report it as a test, never as the proof.
- If you add a control affordance that lets a text-only agent assert on *rendered* entries rather
  than model entries, say so explicitly — it would be worth more than the fix.

## The rules

- **Do not edit `INVENTORY-LEDGER.md`** and do not set verdicts. `pireview` does that.
- Commit path-scoped, never `git add -A`. `grep '??'` before calling it done. Expect `index.lock`
  contention with six panes committing: **retry, never delete the lock.**
- Report to `docs/linux-rewrite/P117-report.md`.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".

---

## Addendum — reassigned, with a code trace and a salvaged patch (orchestrator, 18:20)

**`codex11` is out of credit until 2026-08-20 08:19 and never started this.** Its plan survives at
`docs/superpowers/plans/2026-08-14-p117-transcript.md` with every box unchecked. It left two
**uncommitted** changes in `main.rs`, which I have reversed out and preserved verbatim at
`docs/linux-rewrite/p117-codex11-unverified.patch`. I reversed them because `codex12` is editing the
same file and would otherwise have committed them, unverified, under a `P118` message.

**Take nothing in that patch as established.** Neither half was ever run — the test did not compile
(`E0063`, `OpenTab` constructed with missing fields).

### What is in the patch, and what to make of it

1. **A test worth reviving.** It asserts
   `cx.debug_bounds("chat-transcript").size.height > px(200.0)` after mounting a Chat into a
   workspace tab. That is the right shape of assertion, and it matters beyond this row: **a
   text-only agent can assert on rendered geometry through `debug_bounds`.** Fix its struct literal
   and it becomes the regression test this defect needs.
2. **A source change I would not keep without evidence.** At `main.rs:6435` it deleted the wrapper
   ```rust
   .child(div().flex_1().w_full().overflow_hidden().child(centre_surface))
   ```
   leaving a bare `.child(centre_surface)`. That wrapper is plausibly the very thing giving the
   centre surface its height — removing it looks like the wrong direction. **Verify before adopting
   or discarding.**

### The trace, offered as hypotheses

- `chat.rs:5518` — the `Chat` render root is already `.size_full().flex().flex_col()`.
- `chat.rs:5542-5555` — the transcript is `div().id("chat-transcript").w(px(TRANSCRIPT_WIDTH))
  .pt(px(22.0)).flex_1().flex()` wrapping `list(self.list_state.clone(), …)`.
- `chat.rs:939` — `ListState::new(0, ListAlignment::Top, px(2048.0))`; `push_entry` splices one row
  per entry and keeps the index tree in sync.

So the sizing inside `chat.rs` looks correct on its face, which points **above** it. **A GPUI
virtualized `list()` inside a container with no definite height measures zero and draws no rows** —
that is exactly the observed symptom, and it would also explain the composer sitting at the top of
an empty pane rather than being pushed to the bottom.

`overflow_hidden` on that same wrapper is a candidate for the **Changes** clipping. **One wrapper
could account for both surfaces** — which is the question this task was asked to answer. It is a
hypothesis. Test it; do not take it.

### The gate is unchanged

Re-run the drive at the top of this file and report what `chat.read` returned. Then either measure
the transcript region yourself — `WAYLAND-LANE.md` §"A text-only agent can assert on a frame it
cannot see", with a positive control — or leave the captures and name their paths for a visual pass.
**A green test still does not close this.**

---

## Addendum 2 — for `pi`, 18:15

Three things changed since the addendum above was written.

**The build blocker you hit on `P119` is gone.** `cargo check -p tiller_ui -p tiller` finished clean
at 18:12 against the current working tree. The `AppSettings.opencode_workspace_id_override` work
that was mid-flight has settled. If it breaks again, say so rather than working around it.

**Your own `P119` finding applies directly to this task's gate.** You established that an immediate
socket read races the app and that the state is there after ~3 s. The `chat.read` in the drive at
the top of this file is exactly such a read. **It is not the explanation for this defect** — that
read happened 70 s after send and returned a full transcript, so the model had the entries and the
screen still drew nothing — but when you re-run the drive, settle before reading so you never have
to argue about which of the two effects you are looking at.

**There is now a worked example of proving a rendered result you cannot see.**
`CRITIC-visual-baseline.md` §18:09 drives a click-through and reads the frame back, including the
rule that caught a weak proof: **when a row's fixed value and its default are the same, a capture of
that value proves nothing** — drive it to a state the system would never reach on its own. For this
task that means asserting on a transcript containing a string only your drive could have put there,
not merely on "the region is no longer blank".

Two instruments are available to you and either is acceptable:

- `cx.debug_bounds("chat-transcript")` — the salvaged test in
  `docs/linux-rewrite/p117-codex11-unverified.patch` asserts its height is `> px(200.0)`. Fix its
  `OpenTab` struct literal (it failed `E0063`). **Report this as a test, never as the proof.**
- The frame itself — `WAYLAND-LANE.md` §"A text-only agent can assert on a frame it cannot see",
  with the mandatory positive control. Leave captures in `reference/linux-progress/p117/`, never
  `/tmp`.

Everything else in this file stands, including the second item (the **Changes** clipping) and the
question of whether one wrapper accounts for both surfaces. **Answer that question explicitly**,
even if the answer is "two causes".
