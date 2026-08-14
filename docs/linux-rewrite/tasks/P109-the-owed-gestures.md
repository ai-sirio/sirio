# P109 — the thirty-four owed gestures

**Owner: `sonnet`, as critic.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`,
branch `linux/gpui-waku`. **Claim the display drive lock** — you released it after `P104`.

## What this is

`P106` drove 56 rows on the Wayland lane, which has no synthetic input. 22 were settled there; **34
came back owed a gesture** and are now the largest ready batch on the board. You are driving them
on `DISPLAY=:1`.

**The gestures are already written.** `P106-report.md` names, per row, the exact gesture owed — and
`fable`'s slice close-out near the end of that file is a numbered 13-item batch with the gesture
spelled out for each. Do not re-derive them; do not re-read the census reasoning about why a row is
built. The report says what to do; the screen says what happened.

```
F-PRJ-03 F-PRJ-04 F-PRJ-07 F-PRJ-10 F-PRJ-11 F-PRJ-12 F-PRJ-17 F-PRJ-18
F-CHAT-02 F-CHAT-16 F-CHAT-18 F-CHAT-21 F-CHAT-22 F-CHAT-23 F-CHAT-31 F-CHAT-34
F-SET-16 F-SET-21 F-SID-06 F-SID-11 F-SID-15 F-SID-16 F-SID-17
F-TAB-01 F-TAB-11 F-TAB-18 F-TAB-23 F-TAB-24 F-TAB-28
F-WIN-01 F-WIN-10 F-CHG-01 F-CHG-20 F-PER-07 F-TERM-SPLIT-01
```

You did not build or census any of these, which is the property that makes your report worth
anything.

## The chat rows need the composer, not the socket

`F-CHAT-*` here must be driven **by typing into the visible composer and clicking the controls.**
Do not reach for `surface.chat.*`: it drives a chat session that is never rendered — separate ACP
agent, separate database — and it returns `surfaceId: "default-chat"`, which really is the visible
tab's persisted id, so it looks right while proving nothing. `P107` is fixing that; until it lands,
the socket route cannot settle a chat row. `WAYLAND-LANE.md` §"`surface.chat.*` does not drive the
chat you can see" has the evidence.

This is also why `F-CHAT-21` and `-22` are on your list at all: `P106` found the rendered transcript
empty after a completed turn, which was a correct observation about the wrong surface.

## The lane

`Scripts/linux-drive.sh` is launch → act → capture → kill. Most of this batch needs the app alive
across many gestures, so hold the lock yourself — `ENVIRONMENT.md` §"Holding the lock yourself" has
the `mkdir` snippet and its release `trap`. Export `TILLER_DRIVE_LABEL=sonnet`.

The three traps that have each cost a false result here, unchanged from `P104`:

1. **`mkdir` lock, not flock.** Use the default `/tmp/tiller-drive-1.lockd`; a regular file at that
   path fails `EEXIST` forever (exit 6).
2. **Right-click is `rclick`.** `click()` has no button-3 path. Six rows in this batch are
   right-click menus.
3. **The Files panel can paint over a menu.** If a menu looks empty, check whether something is
   drawn on top before reporting absence.

Two rows in the batch are drags (`F-TAB-18`, `F-SID-16/17`) and one is a drag-then-Escape
(`F-TAB-24`). If `linux-drive.sh` has no drag helper, say so and report those rows unreached rather
than approximating a drag with a click — an approximated gesture is the "declared path passes for a
control nobody can reach" failure this project has already paid for.

## The discipline

- **A recipe says what to DO, never what to CONCLUDE.** If you catch yourself confirming an
  expectation rather than probing, you have stopped being an independent control.
- **Report verbatim where wording matters** — dialog text, menu items top to bottom, greyed entries
  and their reason text, banner copy. A paraphrase of a label is not evidence of the label.
- **A function you did not successfully exercise does not exist.** No inference from adjacent
  behaviour; if the menu item is there, click it.
- **Conjunctions split.** Drive and record each conjunct separately.
- If a row's gesture does not exist on screen, **that is a finding, not a report bug** — say what
  you found instead.

## What to produce

`docs/linux-rewrite/P109-report.md`, **committed every 10 rows.** Per row: the gesture driven, what
the screen showed (verbatim where it matters), and the capture filename. **No verdict column** —
`pireview` sets verdicts in `P108` and will read this.

**Release the lock when you finish and say so in your pane.**

## The rules

- **Do not edit `INVENTORY-LEDGER.md`.** Do not edit any `rust/` source: a bug you find is a
  finding, not a task.
- Commit path-scoped, never `git add -A`. `grep '??'` before calling it done.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
