# P28 — Two persistence defects you found: the scrollback and the lost pane

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need.

## P27 did the job, including the part that is uncomfortable

5 of 8 entries exercised: **PASSED 2 · FAILED 2 · UNREACHABLE 1 · display-blocked 3**.

- **F-PER-04 PASSED** — the nonce survived a worktree switch-away and switch-back. That is the only
  evidence that distinguishes a restored terminal from a fresh empty one.
- **F-PER-06 PASSED** — child PID 2708265 alive before the quit, gone after it, socket removed.
  Two claims, two measurements.
- You built additive launch restore, a graceful `tillerctl` quit and PTY shutdown cleanup, with
  regression tests.

And you reported **two FAILED** against your own area rather than softening them. That is the
behaviour this project runs on: a finding you produce about your own work is worth more than one
extracted from you later.

You also confirmed `Scripts/ci-linux.sh` prints `CI OK` — the workspace now has a gate that runs on
this platform.

## Where

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
./Scripts/ci-linux.sh                      # the gate; must print CI OK
cd rust
export TILLER_SOCKET=/tmp/codex12.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
./target/debug/tillerctl current-workspace
```

No display on this machine presents; everything here is provable headless. codex11 owns
`tiller/src/panes.rs`, `tiller_terminal/**` and the `Scripts/` it created; pi owns `tiller_ui/**`
and `tiller_theme/**`.

## The two defects

### 1. F-PER-01 — terminal scrollback does not survive a relaunch

Your measurement: *"tabs/worktree restored, but terminal scrollback was fresh after relaunch."*

The entry is explicit — it lists scrollback alongside projects, worktrees, tabs and chats, and its
`VERIFY` clause says to **produce terminal output**, quit, relaunch, and confirm the saved state
returns. A restored tab showing an empty terminal is the same illusion F-PER-04 was designed to
catch, one level up: the container came back and its contents did not.

Note the shape of this, because it is this codebase's recurring failure in a new place: **the tab
persisted, so every structural check passed, while the thing the user actually cares about was
gone.** Persisting a handle is not persisting a surface.

What to build: capture PTY output into the session store and replay it on restore. Decide and state
a bound — how much scrollback, and what happens past it — rather than letting it grow without
limit. `F-PER-04` already proves the in-session reattach path works, so the question is only what
crosses a process boundary.

### 2. F-PER-03 — a split layout loses a pane

Your measurement: *"split panes 3 → 2 after relaunch."*

Three panes went in, two came out. The entry wants mounted worktrees **and their layouts** to
return. A layout that silently drops a pane is worse than one that does not persist at all, because
the user cannot tell which pane they lost or that they lost one.

Find out first whether the tree is being serialised incompletely or restored incompletely — they
have different fixes and the evidence distinguishes them: dump what was written and compare it with
what came back. Do not guess.

**The pane tree lives in `tiller/src/panes.rs`, which is codex11's file.** If the fix belongs
there — a `PaneNode` that cannot round-trip, say — **do not edit it**. Write down exactly what is
needed and it will be routed, the way codex11 handed you the `on_action` spec rather than editing
`main.rs`. If the defect is in how `main.rs` saves or restores what `panes.rs` already provides
correctly, that is yours.

## Evidence

For each: the state **before** the quit, a real process exit and relaunch, and the state **after** —
pasted. For the scrollback, a **nonce** written into the terminal before the quit and read back
after it; nothing weaker can tell restoration from a coincidence. For the layout, the pane count and
ids before and after, plus whatever you dumped from the store to locate the loss.

Then re-run the two entries and report their verdicts, and run `./Scripts/ci-linux.sh`.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Every line of Tiller is written from scratch; transplanted code counts as a gap, always.
- **A capability nobody exercised does not exist.**
- You own `tiller_control/**` and `tiller/src/main.rs`.
- Clean up the instances you start and the sockets you create.

## Reporting

Reply in **12 lines or fewer**: what caused each defect, what you changed, the before/after
transcripts with the nonce and the pane ids, anything routed to codex11, the `ci-linux.sh` result,
and the honest remainder.
