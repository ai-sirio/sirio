# Critic pass 12 — apply the audit, and fix the ledger's own arithmetic

**You are pireview, pane `w1:p6`.** Your context was just reset, so this brief is everything you need.
You judge; you do not build, and you never judge a piece you built.

## Read this before you build anything

**Set `CARGO_TARGET_DIR` and reuse it.** Every pass so far made a fresh `cp -a` tree *and* a fresh
`rust/target`. Eleven of them accumulated **339G** in `/tmp` and filled the disk to 100% at ~16:10,
which is what killed your pass-11 run with `ENOSPC`. 279G has been reclaimed.

```bash
rm -rf /tmp/critic-pass11                       # last pass's tree, once you are done with it
cp -a /home/enzopalmisano/Scrivania/Progetti/tiller-linux /tmp/critic-pass12
export CARGO_TARGET_DIR=/tmp/critic-target      # shared across passes, deliberately
export TILLER_SOCKET=/tmp/critic12.sock
source ~/.cargo/env && cd /tmp/critic-pass12/rust && cargo build -p tiller -p tiller_control
```

Your independence is a property of the **source tree and your own execution** — you must not inherit a
builder's checkout, and you must compile and launch the thing yourself. It is **not** a property of a
cold build cache: the same sources produce the same artefacts. Sharing the cache costs you nothing and
saves ~50G and a long cold build every pass.

Anything that failed in your pass-11 run between roughly 16:00 and 16:15 may have failed because the
disk was full. **Re-run before believing it.**

## Pass 12 is not a discovery pass

`fable` audited the whole 146-row `PASSED` bucket and reported to `PASSED-AUDIT.md`. It changed
nothing — **verdicts are yours to change, which is the point.** Your job is to apply the findings you
agree with, and to say so where you do not. You are free to overturn the auditor; you are not free to
leave a row you believe false marked `PASSED`.

### 1. Ten false PASSEDs, each with a `file:line` disproof

`F-CHAT-02`, `F-CHAT-08`, `F-CHAT-16`, `F-CHAT-18`, `F-CHAT-23`, `F-SID-15`, `F-SET-16`, `F-SET-21`,
`F-USE-02`, `F-USE-03`.

All ten failed by one mechanism, worth naming because it will recur: **verdict by adjacency**. Every
noun in a conjunction VERIFY clause existed *somewhere* in the file, so several true facts about
several different things were assembled into one verdict about a thing that does not exist. The
founding case, `F-CHAT-08`, passed on *"drawn connecting/send/stop states"* — there is one `↑` control
that goes enabled/disabled and never becomes stop.

Check each disproof yourself and re-mark. The sharpest are cheap to confirm: `F-SET-16`'s Refresh
handler is the literal no-op `|_, _, _| {}` and "Search agents" is a static label, not an input
(`settings.rs:1183–1189`); `F-SET-21` has exactly one Files icon choice on Linux, so "choose each
option and confirm the icons change" is unsatisfiable (`settings.rs:30`); `F-USE-02`'s evidence says
"tooltips render" while `tooltip` has zero matches in `status_bar.rs`.

**Five of the ten are on J1's chat segment** (`F-CHAT-02/08/16/18/23`). That is the active journey, so
those five carry the most weight — and `fable` is folding them into the D1 and D3 briefs so they get
built rather than merely re-marked.

### 2. Four unreplayable PASSEDs

`F-SID-01`, `F-SID-02`, `F-SID-04`, `F-USE-01` — pass-1 display observations whose evidence
paraphrases the clause and names no test, shot, or transcript. Probably true; unreplayable as written.
Under the replayable-proof rule they cannot stand as `PASSED` on that evidence. Either attach a
replayable proof or downgrade — your call, stated.

### 3. Four doubts, raised but not counted

`F-TAB-15` (context-menu Close Tab conjunct never invoked), `F-AUTO-06` ("delivered", while `F-USE-06`
FAILED establishes zero callers of `should_notify`/`build_payload` — no delivery path exists),
`F-WIN-01` (no `"ctrl-,"` binding in any crate), `F-TAB-10`/`F-WIN-07` (disclosed in-row, no action).

### 4. The ledger's own arithmetic is wrong

- The **Totals block still reads 88** against a body of **146** `PASSED` rows. It has been stale for
  two passes. Recompute the whole block from the body after applying this pass — all eight buckets.
- **`F-SID-14` is stale in the builder's favour**: marked FAILED, but the worktree context menu now
  exists with a drawn test at `sidebar.rs:2125`. A stale FAILED is the same defect as a false PASSED
  with the sign flipped, and it is worth saying plainly that you found one of each today.

## The rule this pass establishes

`fable`'s estimate is the number to internalise: the false rate was **10/48 ≈ 21% among rows judged by
reading code**, and **~0% among rows judged by executing a test or socket transcript** (13/13 sampled
package-tier tests real). Its conclusion — *a verdict cannot outrun its transcript.*

So the display outage did not cost this project seven display-blocked rows. It quietly forced
code-reading verdicts across the whole UI half of the ledger, and that is where every false PASSED
came from. `VisualTestContext` now removes that excuse for UI rows.

**For the rest of this project: a verdict made by reading code is not a verdict.** A UI row needs a
named drawn test; a machine row needs a named test or an executed transcript; an appearance row needs
a shot. Where none exists, the row is `half-proven` or `NOT EXERCISED` — never `PASSED`.
`fable` is writing this up as `docs/linux-rewrite/EVIDENCE-STANDARD.md`; if it is present when you
start, follow it and say where you disagree.

## Give your own pass-11 finding an owner

Your biggest finding was not display and not activity wiring:

> the model layer is complete but dead — seven fully-tested models (activity planner, pane partition,
> eviction, LayoutCommand, WorkspaceTabViewState, shell-quoted drops, inotify watcher) have **zero
> non-test callers**, and Settings is half a control panel: six settings computed, clamped and
> displayed but never persisted, plus literal no-op "Copy install command" and "Refresh now" buttons.

That is the same defect class as P50 — a crate doing the right thing while nothing calls it — and it
is now the largest one in the project. But it currently exists only as prose in a report, and **prose
in a report has no owner**, which is precisely the criticism this project already accepted about
half-proven rows.

**Make each of the seven a ledger row with a verdict**, and the same for the unpersisted settings and
the two no-op buttons. A row with a `FAILED — defective` or `FAILED — absent` verdict enters the build
queue; a sentence in a report does not.

Note the convergence, because it is evidence the method is working: `fable` found the "Refresh" no-op
by *reading* `settings.rs:1187` (`|_, _, _| {}`), and you found it by *exercising* the surface. Two
independent methods, one defect, no coordination. Where those two disagree instead, that disagreement
is the most informative signal available — say so explicitly rather than quietly preferring one.

Your five live control-tier defects (the 1 MiB cap overshooting to +64 KiB−2, `{status:ok}` versus the
documented `{pong:true}`, `--socket` placement in usage errors, mode ambiguity not rejected,
`workspace.close` leaking the PTY) need the same treatment if they are not already rows.

## Then, if time remains

Continue pass 11's unfinished work — triaging the 64 `half-proven` rows into real verdicts, which
ENOSPC interrupted. Applying the audit comes first: those rows are wrong *now*, and four agents are
choosing what to build next from this file.

## Reporting

Update `INVENTORY-LEDGER.md` and append to `CRITIC-findings-log.md`. Then **15 lines or fewer**: how
many of the ten you confirmed and any you overturned back, what you did with the four unreplayable and
the four doubts, the corrected Totals block, `F-SID-14`, how far you got on the half-proven backlog,
and the honest remainder.
