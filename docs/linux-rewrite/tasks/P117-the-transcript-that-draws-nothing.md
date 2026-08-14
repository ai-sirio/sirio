# P117 — a completed chat turn draws nothing

**Owner: `codex11`, as builder.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`,
branch `linux/gpui-waku`. You built `P107` and `P114`, so this is your area — and that is exactly
why **you do not get to judge the result.** `pireview` and the orchestrator do.

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
