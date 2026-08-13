# P27 — Persistence and lifecycle, proven by actually restarting

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need.

## P25 is closed, and the socket doors did exactly what they were for

All pane and tab keyboard actions are wired to `TillerWorkspace`'s root handlers, and the same
transitions are reachable as `pane.split`, `pane.focus`, `pane.close`, `tab.cycle`, `tab.select`.
Your headless transcript proves the state really moved: split `pane-1 → pane-2`; focus left
`pane-2 → pane-1`; close removed `pane-1`; tab select activated `pane-0`. 81 tests. Nothing needed
routing to codex11.

That is the payoff of the design constraint: because the action and the method call the **same**
function, a command layer that cannot be typed into today is nonetheless proven today. And you
stated the limit plainly — *the key chords themselves remain unexercised without a working display*
— rather than inferring them from the binding table.

## Where

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust
cargo test -p tiller -p tiller_control && cargo build -p tiller -p tiller_control

export TILLER_SOCKET=/tmp/codex12.sock            # isolates you from the other agents
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
./target/debug/tillerctl current-workspace
```

No display on this machine presents. codex11 is building `Scripts/ci-linux.sh` — a Linux
verification gate with a headless smoke test; use it once it exists. pi is in `tiller_ui/**`.

## The piece

`docs/linux-rewrite/01-inventory-app.md`, section **`## Persistence and lifecycle`** — **8 entries**,
F-PER-01 through F-PER-08. Count them yourself before you start; that number is my reading of the
file, and the file is the contract. (An earlier brief of mine said 14 `F-TERM` rows when there were
11, and the agent that checked was right to say so.)

This whole section is exercisable **headless**, because persistence is state and the socket reports
state. The pattern for every entry is the same:

1. Put the app into a known condition through the socket.
2. **Quit it and start it again** — a real process exit and a real relaunch, not a reload.
3. Ask the socket what it now believes, and compare.

The critic has already verified that tabs, the active tab, and worktrees survive two clean restart
cycles, so some of this passes today. **Confirm and extend; do not rebuild what works.**

Entries worth particular care:

- **F-PER-04** — scrollback reattaches when a worktree is remounted. Write a **nonce** into a
  terminal, switch away so the host unmounts it, come back, and read the scrollback: the nonce is
  the proof. Anything less cannot distinguish "restored" from "a fresh empty terminal".
- **F-PER-06** — quitting flushes state *and* terminates child processes. Two claims, two
  measurements: state present after relaunch, and the old child gone (`pgrep` the pid before and
  after). A quit that leaves orphaned PTYs behind is a real defect and this machine has been
  accumulating stray instances all morning.
- **F-PER-05** — restoring launch-snapshot tabs must **not replace current state**: an unrelated
  current tab has to survive the restore. That "without replacing" clause is the entire point of the
  entry, so test the interference case, not just the happy path.
- **F-PER-08** — mentions browser-origin grants. `browser.*` is explicitly unsupported on Linux, so
  that half is **UNREACHABLE, not FAILED**. The general-settings half is testable and pi has just
  been working on settings persistence — check it rather than assuming.

Where an entry needs something that does not exist yet, build it if it is in your files
(`tiller/src/main.rs`, `tiller_control/**`), and say so if it is not.

## Evidence

For each entry: the socket state **before** the quit, the fact of a real process exit and relaunch,
and the socket state **after** — pasted. Plus the nonce for F-PER-04 and the pid check for F-PER-06.

Use the verdicts precisely: **PASSED** (exercised, with evidence) · **FAILED** (exercised, did not
work) · **UNREACHABLE** (a stated external reason) · **NOT EXERCISED — blocked on display** (for
anything that must be seen; never ticked from reading the code).

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Every line of Tiller is written from scratch; transplanted code counts as a gap, always.
- **A capability nobody exercised does not exist.**
- You own `tiller_control/**` and `tiller/src/main.rs`. codex11 owns `tiller/src/panes.rs`,
  `tiller_terminal/**` and `Scripts/**` it created; pi owns `tiller_ui/**` and `tiller_theme/**`.
- Clean up after yourself: kill the instances you start and remove your sockets. Stray processes
  have corrupted other agents' evidence today.

## Reporting

Reply in **12 lines or fewer**: how many entries you exercised and the four counts, the
before/after transcripts for the ones that matter, what you had to build, and the honest remainder.
