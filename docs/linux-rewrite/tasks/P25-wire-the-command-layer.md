# P25 — Wire the pane command layer, and give it a door the socket can open

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need.

## P21 is closed on the half that could be closed, and you closed it correctly

`ChangesTab` is mounted as a first-class Diff tab, follows the selected worktree, opens via
`+ → Changes`, and persists and restores with the other tabs. 42 tests including **the shell mount
guard** — the assertion that fails if the surface ever stops being reachable, which is the thing
that would have caught this defect on the day it was introduced.

And you reported: *"Screenshot: NOT PRODUCED. Live stage/discard: NOT EXERCISED — `:2` Xwayland is
wedged."* That is exactly right. Every display on this machine is gone (`:1` died at 09:56; `:2`'s
Xwayland is alive in `ep_poll` but `xdpyinfo` from a fresh unrelated client times out, and killing
the app on it did **not** recover it — so nothing is wedging it from outside). Labelling the
unprovable half honestly instead of approximating it is worth more than a plausible screenshot.

The live stage/discard proof stays on the board, owed, until the environment is repaired.

## Where

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust
cargo test -p tiller -p tiller_control
cargo build -p tiller -p tiller_control     # tillerctl is a BINARY of tiller_control

# headless — no display needed, and TILLER_SOCKET isolates you from the other agents
export TILLER_SOCKET=/tmp/codex12.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
./target/debug/tillerctl current-workspace
```

codex11 is in `tiller/src/panes.rs` and `tiller_terminal/**`; pi is in `tiller_ui/**` and
`tiller_theme/**`.

## The piece

codex11 built the pane command layer in `panes.rs` — actions, Linux chords, pure state transitions,
typed split-disabled reasons, 12 pane tests — and stopped at the file boundary rather than reaching
into `main.rs`. Its handoff, verbatim:

> Attach each action to `TillerWorkspace`'s root via `on_action`.
> **Focus** handlers use `neighbor` and leave state unchanged when there is no neighbour.
> **Split** handlers check eligibility, split, focus the new pane, and visibly create right/down panes.
> **Close** removes the leaf, terminates its terminal, promotes and focuses its sibling.
> **Cycle/jump** handlers update the active tab; jumps beyond the tab count select the last.

Chords already chosen: `Ctrl+Alt` arrows for focus, `Ctrl+Alt+Shift` for splits, `Ctrl+Alt+W` to
close, `Ctrl-Tab` / `Ctrl-Shift-Tab` to cycle, `Ctrl-1…9` to jump.

### The second half, which is what makes it provable today

A keyboard action cannot be exercised headless — with no window there is nowhere to send a key. So
**give every one of these commands a socket door as well**: `pane.split`, `pane.focus`, `pane.close`,
`tab.cycle`, `tab.select`, or whatever names fit the existing scheme.

Two reasons, and the second is the real one:

1. It makes the command layer verifiable **right now**, with no display: call the method, then ask
   `panel.list` and `workspace.current` what the app thinks its state is. That UI-versus-socket
   cross-check is the instrument that has caught the most defects in this project.
2. It is already owed. `CLAUDE.md` describes cmux-parity groups — `surface.*` / `pane.surfaces` —
   and the automation inventory expects panes to be drivable from outside.

**The keyboard action and the socket method must call the same function.** Not two implementations
that agree today. This is the rule F-009 was born from: two routes into one piece of state, only one
of which maintains it, produces a view and a socket that disagree with no way to tell which is
lying. One source of truth, many doors.

## Evidence

1. `cargo test -p tiller -p tiller_control` green, with tests for each transition, including the
   no-neighbour case, the ineligible-split reason, and jump-beyond-count selecting the last tab.
2. **A live headless transcript**: for at least a split, a focus move, a close and a tab jump —
   `panel.list` before, the socket call, `panel.list` after — pasted, showing the state actually
   moved. That is your acceptance test and it needs no pixels.
3. A statement of what remains unprovable without a display: that the **key chords** themselves
   reach the actions. Say so plainly; do not tick it from the binding table.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Every line of Tiller is written from scratch; transplanted code counts as a gap, always.
- **A capability nobody exercised does not exist.**
- You own `tiller_control/**` and `tiller/src/main.rs`. If the wiring needs a change inside
  `panes.rs`, that is codex11's — **say exactly what you need and it will be routed**, the way
  codex11 wrote you this handoff rather than editing your file.
- Do not tick anything visual. Everything that must be seen is `NOT EXERCISED — blocked on display`.

## Reporting

Reply in **12 lines or fewer**: which actions are wired, which socket methods now exist, the
before/after `panel.list` transcript, the test count, anything you need routed to codex11, and the
honest remainder.
