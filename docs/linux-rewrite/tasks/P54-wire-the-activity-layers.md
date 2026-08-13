# P54 — Wire the three revived evidence layers into the app

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you need.

## D2 landed well

Three named drawn tests, and the one that shows you thought about the surface rather than the feature:
`ctrl_k_in_a_focused_terminal_does_not_open_the_palette`. A palette that steals `Ctrl+K` from a
focused terminal would have been a bug found by a user, not by a test. `D-CMD-01`/`D-CMD-02` marked
`builder-claimed, unverified` is the right discipline — you left the verdict to the critic.

The `panes.rs:825,915` breakage is `pi` mid-piece. Correctly reported as someone else's; leave it.

## The piece

`codex11` rebuilt three of the four agent-activity evidence layers in P50 and deliberately did **not**
touch `main.rs`, writing `docs/linux-rewrite/tasks/P50-activity-wiring-contract.md` instead. That was
right — `main.rs` is yours, and two agents editing it is how a day gets lost.

But it means the layers currently reach nothing. **Read that contract and wire them.**

This is the shape that keeps recurring and that this project keeps paying for: a crate does the right
thing, its tests pass, and nothing on the surface ever calls it. P50 exists because three layers
passed their own tests with **zero callers**. Wiring is not the boring part of that piece; it is the
part that makes it real.

Layer B (OSC titles), Layer C (content on output-settle) and Layer D (foreground process, 500 ms
cadence) all have sources now. What they lack is a consumer.

## The trap codex11 named, which is yours to honour

> **child exit must not rewrite title-owned or process-owned state.**

Three ownership kinds share one model:

- **spawn-owned** — Tiller launched it; cleared by watching the process exit.
- **title-owned** — cleared *only* when the title stops matching that agent's conventions.
- **process-owned** — cleared *only* by `processGone`, never by an unrelated title change.

Wiring is exactly where one layer's signal starts wiping another's, and where the absence of a signal
gets mistaken for evidence of absence. Every past bug in this area was a confusion between these
three. If you make one behavioural decision in this piece, make it this one, and state it.

Note also the standing hazard: a `SetTitle` context action (the user-typed pane label, which you
built) and the OSC title the shell writes are **different things**. Layer B means the second.

## Evidence

Not unit tests on the layers — codex11 already has those, and they are precisely what passed while
nothing was wired.

The proof is **the app**: a running process where a pane's status changes because a real signal
arrived, observed from outside over the control socket with your own `TILLER_SOCKET`. At minimum,
show a title-driven transition and a process-driven transition, and show that one does not clobber the
other.

Name every proof by test function or transcript path so the critic replays instead of re-deriving.
Mark rows `builder-claimed, unverified`, never `PASSED`.

Why that rule tightened today: `fable` audited the 146 `PASSED` rows and found **10 false**, all by
one mechanism — every noun in a VERIFY clause existed somewhere in the file, so true facts about
different things were assembled into a verdict about something that did not exist. Measured false rate
was **21% among rows judged by reading code and ~0% among rows judged by executing a transcript**. A
verdict cannot outrun its transcript.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. Confirm
  with `git branch --show-current` before you start — `codex11` spent a piece in the wrong worktree
  today and the tell was a gate error it read as the wrong script.
- The gate is **`Scripts/ci-linux.sh`**. `Scripts/ci.sh` is the macOS gate and cannot pass here.
- **Do not edit `tiller_ui/**`** (`pi`, mid-piece) or `tiller_persistence`/`tiller_acp` (`codex11`).
- **Proceed without asking for design approval.**

## What comes after, so you can plan

Two more contracts will need wiring into `main.rs` once `codex11` re-lands them: P52 (chat transcript
persistence) and P53 (the `chat.*` socket doors — `grep '"chat\.'` currently returns nothing, which is
why `D-J1` cannot be exercised past its project leg). They are **not** this piece. `main.rs` stays
single-owner, and you will get them as one batch when both contracts exist.

## Reporting

**12 lines or fewer**: what you wired and where, the one ownership decision you made and why, the two
transitions **by test name or transcript path**, the `ci-linux.sh` result, and the honest remainder.
