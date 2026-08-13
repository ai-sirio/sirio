# Critic pass 9 — produce the definitive per-entry status of all 388

**You are pireview, pane `w1:p6`. You are the critic.** Your context was just reset, deliberately.
You built none of this.

## Pass 8 found the root cause, not another symptom

> **The shell has no menu bar and no command layer at all.** That explains **19 of the 29 absent
> non-browser features** — `F-TAB 02/08/09/12/13/14/17/21/26/27`, `F-WIN 02/03/04/05`,
> `F-SID 07/08/09/12/14`. The only shell interaction surface is the tab strip (activate, ✕) plus
> the `+` menu.

Nineteen entries had been counted as nineteen missing features. They are **one missing thing,
counted nineteen times** — every operation whose only route to a user is a menu is absent because
there are no menus. That has been routed to the shell owner, which is now building the command layer
first and hanging the operations off it.

Three more from the same pass, each of a kind only exercising finds:

- **`F-CHAT-24/25`'s old PASSED was only the permission-card mechanism** — the named Plan and
  Question cards do not exist, nor a long list of others. An old verdict that was true of the
  machinery and false of the feature.
- **`F-SET-03`'s Check for Updates is a dead no-op button** — worse than pass 7 had it. A control
  that does nothing is the purest form of this codebase's failure mode.
- **`F-SID`: 7 PASSED, 2 PARTIAL, 9 FAILED-absent**, with `03` and `05` **upgraded** from an old
  FAILED because the surface genuinely changed. Corrections in both directions, which is what makes
  the rest of the report worth reading.

And the test-health hold is cleared: the flaky drawn mutation tests (`F-CHG-10/11/14`, `F-EDIT-09`)
now pass deterministically three times over, and the chord-dispatch and payload-drag fixtures you
asked for exist.

## The piece: the one artefact this project still lacks

The goal says done means **every inventory entry exercised live and ticked by a critic that did not
build it.** After nine passes and thirty-odd builder pieces, **no single artefact says where each of
the 388 entries stands.** `INVENTORY-STATUS.md` is the closest thing and it is a summary by area,
assembled by an orchestrator from reports, partly out of date, and it explicitly says its counts are
not evidence.

Produce the real one: **a per-entry table covering all 388**, in
`docs/linux-rewrite/INVENTORY-LEDGER.md`. One row per entry:

| id | verdict | evidence | judged |
|---|---|---|---|

- **id** — `F-TAB-07`, `F-CORE-ACT-19`, and so on, from both inventory files.
- **verdict** — `PASSED` · `FAILED — absent` · `FAILED — defective` · `UNREACHABLE` (with the
  reason) · `N/A — platform` · `NOT EXERCISED — blocked on display` (appearance only) ·
  `NOT EXERCISED` · `half-proven` (with which half).
- **evidence** — one short phrase pointing at what proves it: a test name, a socket transcript, a
  pass number. Not prose.
- **judged** — which pass, or `builder-claimed, unverified` where only a builder has touched it.

**Keep `FAILED — absent` and `FAILED — defective` distinct.** Pass 8 is the reason: they need
different work, different estimates, and conflating them is how nineteen entries looked like
nineteen problems.

**Do not re-exercise everything** — that would take days and is not what this is. Consolidate what
the nine passes already established, mark honestly what has never been judged by you, and where
`builder-claimed` is all you have, say so rather than inheriting it as PASSED. **Your verdicts are
the only ones that count toward done, and an entry nobody independent has touched is not done, no
matter how good the builder's evidence was.**

Where the ledger disagrees with `INVENTORY-STATUS.md`, the ledger wins and the summary should be
corrected to match.

End it with the totals — how many in each verdict — because that number is the answer to "how far
along is this", and right now nobody can state it.

## Where and how

```bash
cp -a /home/enzopalmisano/Scrivania/Progetti/tiller-linux /tmp/critic-pass9
source ~/.cargo/env && cd /tmp/critic-pass9/rust && cargo build -p tiller -p tiller_control
export TILLER_SOCKET=/tmp/critic9.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
```

Your own report, `CRITIC-baseline.md`, is the primary source — it holds every pass. **Never call an
X tool**; they block forever here. Record the snapshot time.

If budget runs out before all 388 rows, **stop and say where you got to**. A ledger covering 250
entries honestly is worth more than 388 rows where the last hundred were filled in from memory.

## Method

- **Append, never rewrite** in `CRITIC-baseline.md`; the ledger is a new file and may be written
  whole.
- **You do not fix anything.**

## Reporting

Reply in **12 lines or fewer**: the totals per verdict, how many entries have never been judged by
you, the three largest blocks of absent work, and **the single biggest gap that is not the display**.
