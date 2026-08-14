# P101 — critic pass on P95's seven rows

**Owner: `pireview`, as critic. You did not build this** — `codex11` did, in commit `cc73d71`
("fix: connect existing linux rewrite seams", 578 insertions across `file_view.rs`,
`right_panel.rs`, `file_events.rs`, `main.rs`). Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

Start fresh. Do not read the builder's plan file until you have formed your own view of what the
code does — `docs/superpowers/plans/2026-08-14-p95-connect-the-halves.md` is their account, not
evidence.

## The seven rows

Their clauses and `VERIFY:` lines are in `docs/linux-rewrite/INVENTORY.md`; their current verdicts
and evidence are in `docs/linux-rewrite/INVENTORY-LEDGER.md`. **Read each row's own `VERIFY:` line
and treat it as your script** — it says what to feed in and what to inspect.

`F-CORE-FILE-03` · `F-CORE-FILE-06` · `F-EDIT-05` · `F-EDIT-12` · `F-SET-09` ·
`F-TERM-PTY-06` · `F-TERM-UI-02`

The brief they worked from is `docs/linux-rewrite/tasks/P95-connect-the-halves-that-already-exist.md`.
Every one of these rows was **"the expensive half is already written, the connection is missing."**
So the question you are answering is narrow and specific: **is the connection real, and does the
feature now happen when a user does the thing?**

## What the builder reported, so you can attack it

They reported the work done, and one gap in their own evidence: *the UI was launched and the
right-click Files menu was observed, but the re-capture after the final correction was blocked by
the `DISPLAY=:1` mutex held by another session.* **So the last state of the code was never seen
running.** That is exactly the gap you exist to close — do not inherit their screenshot, take your
own after the final commit.

## The rule that decides every verdict here

**Code plus a green test is `NOT EXERCISED`, never `PASSED`.** These seven rows were *already*
green-tested before `P95` — that is why they were filed as unconnected halves rather than as
missing features. A passing `cargo test` therefore proves nothing new about any of them. What
moves a row is **you performing the user's gesture and seeing the result.**

Watch for the specific failure this piece invites: a connection that compiles, is subscribed, and
fires — into a handler that updates state nothing renders. `F-TERM-UI-02` is the one to distrust
most; its whole history is an event with no subscriber, and "I added a subscriber" is not the same
claim as "the link opened."

## Two conjunctive rows

Check the clause text yourself, but at minimum: where a clause says *"and"* or *"then"*, **verify
and record per conjunct.** A single "works" hides whichever half you touched last. If one conjunct
holds and the other is untouched, that is `half-proven`, **naming which is which** — not `PASSED`.

## The display

`sonnet` holds `DISPLAY=:1` for `P91`/`P94`. Do not fight it and do not idle waiting for it.

1. Start with everything that needs no display: read the diff, run the package tests, and exercise
   what the **headless lane** can reach — `docs/linux-rewrite/HEADLESS-LANE.md` has the setup, the
   two traps (108-byte socket path; the lavapipe ICD path), and the read-twice rule. Use your own
   `TILLER_DB` and `TILLER_SOCKET` under `/tmp/`, and clean up by matching on the env var, never on
   the process name.
2. Take `:1` when it frees and do the visual and gesture work then.

**A feature you have not successfully tried does not exist.** If the display never frees, the honest
verdict for a surface-subject row is `NOT EXERCISED` **with the instrument reason written down** —
never `PASSED`, and never `FAILED` either.

## Verdict vocabulary

`PASSED` (works as the clause describes, exercised live) · `half-proven` (naming which conjunct) ·
`FAILED — defective` (acts on the wrong thing, or the effect never happens) · `FAILED — absent`
(genuinely not built) · `NOT EXERCISED` (the harness could not perform it — give the instrument
reason) · `UNREACHABLE` (the host never mounts it; wiring owed).

## The rules

- **You may edit `INVENTORY-LEDGER.md`** — you are the critic; that is your job. **Do not hand-edit
  the totals**: recount with the one-liner in its Totals block. The total is frozen at **389**; if
  your recount disagrees, you created or destroyed a row.
- Record, per row, the **command or gesture** that produced the verdict. A verdict with no
  reproduction is not a verdict.
- Commit path-scoped, never `git add -A`. Several agents are working in this tree — follow
  `ENVIRONMENT.md` §"A shared file is not a reason to leave work uncommitted".
- A red `cargo test --workspace` is usually another agent's intermediate state; **attribute a red
  gate before reporting it.** Prefer `cargo test -p <crate>`.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
- Report the verdict deltas when you finish.
