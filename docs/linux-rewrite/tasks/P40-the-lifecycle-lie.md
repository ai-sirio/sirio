# P40 — Closing a workspace leaves processes running

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need.

## P37 landed, and you honoured the boundary you had asked to cross

Typed conflict errors, the mutation methods, shared `tiller_git` functions so the UI and automation
call the same code, and **`tiller_ui/src/changes.rs` left untouched** — the split held, and pi
closed the rendering half from its own side in the same window.

You also confirmed the reach of the new test harness, which mattered to everyone: `VisualTestContext`
supports **`simulate_keystrokes`**, drags via real mouse down/move/up, and double-clicks via
`click_count`. That last set is what turns the `F-TAB` chord entries from unprovable into testable,
and they had been stuck since P20.

And you drew a distinction worth keeping: native Finder, file chooser, clipboard and browser effects
are **external-backend concerns, not display blockers**. Two different reasons an entry cannot be
proven here, and lumping them together would have hidden which ones a compositor restart would
actually fix.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
./Scripts/ci-linux.sh
cd rust && export TILLER_SOCKET=/tmp/codex12.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
```

pi is in `tiller_ui/**`; codex11 is in `tiller_persistence/**`, `tiller_terminal/**` and
`tiller/src/panes.rs`.

## The finding — the critic's biggest non-display gap

> **The control tier's lifecycle lie.** `workspace.close` leaves control panes' PTYs and orphaned
> process groups running — its own `VERIFY` clause demands termination. And `terminate()` kills only
> the **direct child**, so compound-command panes leak on `panel.close` and on app quit too. Both
> measured, both still present in the live tree.

This is worse than an inventory miss, for three reasons.

**It contradicts a claim already made.** F-PER-06 was reported PASSED with real evidence — a child
PID alive before quit and gone after. That measurement was true and too narrow: it watched *the
direct child*. A pane running `bash -lc 'foo | bar'` leaves the pipeline behind.

**It is silent and cumulative.** Nothing reports a leak. It shows up hours later as a machine full
of stray processes — which this one has been accumulating all day, and which was repeatedly
mistaken for other problems.

**It is a resource-lifetime bug in a program whose whole purpose is running other people's
processes.** Tiller exists to host agents. Leaking them is close to the centre of what it must not
do.

### The mechanism, so you can aim at it

Killing a child does not kill what the child started. A shell spawned as a pane becomes a parent
itself, and signalling only the immediate pid leaves its children reparented to init and running.

The usual answer is to give each pane its own **process group** (or session) at spawn and signal
the *group*, so everything the pane started dies with it. Then escalate rather than assume: ask
politely, wait a bounded time, then insist. Getting the order or the waiting wrong produces either
zombies or data loss on a well-behaved child that was mid-write.

Decide and state: what happens to a pane that ignores the polite signal, and how long it gets.

### Also from the same finding

`worktree.set`'s comments **claim persistence and it dies on restart — there is no DB column.** A
comment asserting a property the code does not have is worse than no comment: the next reader
believes it. Either persist it or correct the comment; say which and why.

## Evidence

The check must be **from outside the app**, because the app's own view is what was wrong before:

1. Create a pane running a **compound command** — a pipeline or a shell that spawns a child, so
   there is a process group and not just one pid.
2. Record every pid in that group.
3. `panel.close`, then `workspace.close`, then quit the app for real.
4. `ps` for those pids **after each step**. All gone, or the piece is not done.

Do the same for app quit with a pane still running, and paste the pid lists before and after. Then
`./Scripts/ci-linux.sh`.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- **A capability nobody exercised does not exist** — and F-PER-06 is the reminder that *exercised
  narrowly* is its own failure mode. When you re-verify it, verify the group.
- You own `tiller_control/**`, `tiller/src/main.rs`, project discovery and `tiller_git/**`. Pane
  spawning may reach into `tiller_terminal/**` or `tiller/src/panes.rs`, which are **codex11's** —
  if the fix belongs there, write the spec and it will be routed.

## Reporting

Reply in **12 lines or fewer**: what leaked and why, what you changed, the pid lists before and
after each of the three steps, what a pane that ignores the polite signal gets, what you did about
`worktree.set`, and the honest remainder.
