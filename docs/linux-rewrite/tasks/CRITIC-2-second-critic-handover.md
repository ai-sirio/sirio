# Second critic — handover for `sonnet`

**Take this up when P80 is delivered, not before.** It changes your role, not your current piece.

## Why the roster is changing shape

The stopping condition for this whole project is one sentence: *the full-app critic ticks every
inventory entry by exercising it live.* Not "the builders finished" — **the critic verified.** That
makes critic throughput the project's rate limit, and the roster has been the wrong shape for it.

Measured tonight: `fable` spent **44 minutes** on a plain-launch batch of roughly six rows, doing
careful work — re-capturing to defeat paint lag, verifying its own helper before trusting a verdict.
Meanwhile three builders shipped about 28 rows' worth of work into the queue, and
`ADJUDICATION-BACKLOG.md` already lists **45 rows that are reachable with the route named** and
simply await exercise. One critic cannot drain that.

So: **you become the second critic.** Builders drop to `codex11` and `codex12`; critics rise to
`fable` and you. That is a better shape when verification, not construction, is the constraint.

## What a critic is here — four rules

1. **You start fresh.** You do not read the builder's reasoning before judging. If you already know
   how something was built, that knowledge is contamination, not context.
2. **You compile, launch and screenshot the app yourself.** Not the builder's captures. Not a test
   suite's word for it.
3. **You exercise the feature the way a person would.** Click the control. Type in the field. For
   agent rows, connect a real ACP agent, send a message, watch it stream.
4. **A feature you have not successfully tried does not exist.** That is the whole standard. If it
   does not compile or does not render, *that is the gap*, by definition.

**Only a critic changes a verdict.** A builder's claim is never a verdict, no matter how confident.

## What you may not judge, and why it matters

You have built a large part of this UI. **You cannot judge your own work** — that is the one rule
that makes a critic worth anything.

Do not judge:

- **`F-SET-*`** — you built `F-SET-09/14/16/22` and ruled `F-SET-21`/`F-SET-15`.
- **`F-PRJ-13/14/15/16` and `F-PER-07`** — your P80 piece.
- **`F-SET-11` / `F-USE-*`** — `status_bar.rs` is yours.
- Anything resting on `titlebar.rs`, `controls.rs`, `composer.rs`, `icons.rs`, `sfsymbol.rs`,
  `chat.rs` or `tiller_theme/**` where **you** wrote the surface being tested.

Those belong to `fable`. **When in doubt, hand it over** — a wrongly-claimed independence is worse
than a slow queue.

What is yours to judge is most of what remains: terminal, sidebar, project, git, browser, editor,
ACP, persistence and activity rows. `codex11` and `codex12` built those; you did not.

## Divide the work so you do not both drive the same row

`fable` is mid **pass 17**. Its plan, in its own order: plain-launch drives (terminal menu sweep,
`F-TERM-09`, `F-CHG-06`, `F-SID-12`, `F-SET-19/20`), then agent-launch drives (eight `F-CHAT` rows
plus the `F-ACT-10` live half), then blocked-half dependencies and the ledger update.

**Start from the opposite end** so you do not collide: take `ADJUDICATION-BACKLOG.md`'s
*reachable — 45 rows, route named* table from the bottom up, skipping anything in your exclusion
list and anything `fable` has claimed. Announce which block you have taken before you start.

## Verdict vocabulary — use exactly these

`PASSED` · `half-proven` · `FAILED — absent` · `FAILED — defective` · `UNREACHABLE` ·
`N/A — platform` · `NOT EXERCISED`

**`builder-claimed, unverified` is not `PASSED`.** Row format, and the backticks matter:

```
| `F-XXX-NN` | VERDICT | evidence | source |
```

## Two false-verdict shapes that have already cost this project passes

**A platform claim you did not test.** `ENVIRONMENT.md` twice stated as fact that XTEST cannot
deliver button 3 under XWayland. It can — `reference/linux-progress/p17-rclick-term.png` shows the
terminal context menu open. The real cause was that the harness had no button-3 path at all. **Twelve
rows were one step from being recorded as false negatives.** Verify the instrument can perform the
action before concluding the subject cannot receive it.

**A surface that is present but occluded.** In that same frame the Files panel paints over the
context menu, truncating long labels at the panel's edge. The menu is fine. Judging those items
absent would send somebody to rebuild what already works. It is a z-order defect and its own row.

**And one more, discovered tonight:** the ledger's evidence strings go stale in a way no script
detects. `F-BRW-01`'s *"no browser surface"* was true at pass 8 and is false now. **Read the tree,
not the ledger's opinion of the tree.**

## Instruments

- **The harness**: `Scripts/linux-drive.sh`. Keyboard chords land **only after a real click** gives
  the app X focus — click first, then send the chord. `fable` has observed **paint lag on the first
  capture after an action**; take a second and compare before believing a frame.
- **The persistence DB**: WAL mode, and there is no `sqlite3` CLI on this box. Copying
  `tiller.sqlite` alone reads as *not persisted* for things that persisted fine. The working recipe
  is in `ENVIRONMENT.md`.
- **`Scripts/transplant-check.py`** — the goal says a critic finding transplanted code is always a
  gap. This checks it: `0` clean, `1` candidates, `2` references missing. It exits 2 rather than 0
  when the references are absent, so a missing-refs run can never read as clean.
- **There is no headless critic.** Xvfb and Xephyr both paint nothing; GPUI's blade renderer needs
  DRI3. Verification is tied to the real display. Closed avenue, do not reopen it.

## Commit discipline in a shared worktree

Four agents write to this tree. **Commit by explicit path — never `git add -A`**, which would sweep
up another pane's half-written file. Captures belong in `reference/linux-progress/`, and a proof
that lives only in `/tmp` is deleted by the next pass and takes its verdict's replayability with it.
