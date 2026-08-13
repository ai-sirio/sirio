# P29 — Build the visual sweep now, so the evidence exists the minute a display does

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you
need.

## P26 is closed and the workspace finally has a gate

`Scripts/ci-linux.sh` plus its contract test. Output: `PRE-EXISTING FORMAT DRIFT…`,
`PRE-EXISTING CLIPPY WARNINGS: 61`, `PASS: headless smoke test`, **`CI OK`** — in 12.8 seconds.
Separating pre-existing drift from new drift is what will keep it alive: a gate that is red on
arrival gets ignored within a day. Another agent has already run it and reported `CI OK`, so it
works for someone who did not write it — which is the only real test of a gate.

You also asked whether to merge or open a PR. **Neither: keep the branch as it is.** Nothing in this
project is committed yet and that is deliberate; it is the operator's call, not ours.

## Where

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
./Scripts/ci-linux.sh
```

codex12 is in `tiller/src/main.rs` and `tiller_control/**`, fixing two persistence defects; pi is in
`tiller_ui/**` and `tiller_theme/**`.

## The problem this piece solves

**No display on this machine presents.** `:1` died; `:2`'s Xwayland is alive in `ep_poll` but
`xdpyinfo` from a fresh unrelated client times out, and killing the app on it did not recover it;
freshly created displays produce one-colour frames under every driver combination tried. The fix is
a compositor restart and that is the operator's decision.

So the project's definition of done — *a critic exercises every inventory entry live, and the UI is
put next to waku's frames and judged* — is stalled on something we cannot fix. **What we can do is
make sure that when a display returns, the entire visual evidence set is one command away** instead
of an afternoon of manual driving.

Right now the visual evidence is ad-hoc: one screenshot per piece, taken by whoever remembered,
named inconsistently, at different sizes, of whatever state the app happened to restore into.

## The piece: `Scripts/visual-sweep.sh`

One command that launches the app, walks it through every named surface, captures each, and writes a
labelled set ready to put beside waku's.

### Drive the state through the control socket, not through clicks

This is the important design decision. Absolute-coordinate clicking has been the most fragile thing
in this project all night — it moves the operator's real pointer, it breaks when the window moves,
and it silently drives another agent's window when it goes wrong.

The socket can now set up every state deterministically: `workspace.select`, `panel.create`,
`pane.split`, `pane.focus`, `tab.cycle`, `tab.select`, plus whatever opens Changes and Settings.
Check `system.capabilities` for the current list rather than trusting this brief. Where a surface
cannot be reached over the socket, **say so** — that is a real gap in the automation surface and it
gets routed, not worked around with a click.

### The surfaces to sweep

At minimum, each as a named frame: an empty workspace, a chat, a terminal with real output, a split
layout, the Changes tab against a repository with **one staged, one modified and one untracked file
at once**, the Files tree, and each Settings section. Build the fixture repository yourself with
`git init` so the frames are reproducible rather than dependent on whatever this worktree looks like
that day.

### Rules the harness must obey

- **Never report a blank frame as a success.** Fewer than ~200 distinct colours (`identify -format '%k'`)
  means presentation failed, not that the surface is empty. Exit non-zero with a message that says
  which. This guard is why today's failure was diagnosable at all.
- Match `_NET_WM_PID`, so it can never photograph another agent's instance.
- Prefer `import -window <id>`; a root crop is occlusion-prone and has already produced two phantom
  defects.
- Its own `TILLER_SOCKET` and its own fixture paths; clean up processes and sockets on exit,
  including on failure.
- With no usable display, it must **fail clearly and immediately**, naming the reason — never
  produce a directory of black rectangles that someone later mistakes for evidence.

### Then: the comparison

waku's frames are at
`../_tiller-refs/waku/website/public/app-screenshot-dark.png` and `…-light.png`, and its measured
visual system is in `docs/linux-rewrite/03-visual-bar-and-gpui-patterns.md`. Emit a contact sheet or
a side-by-side that puts our frame next to waku's at the same scale, so a judgement can be made by
looking rather than by remembering. There is prior art in `reference/sbs.py` and
`reference/capture.sh` — read them before writing your own.

**You are not judging the result.** A critic that did not build any of this will do that. Your job
is to make its judgement cheap and repeatable.

## What can be proven today, and what cannot

Provable now, headless: **every state-setup step**. Run the sweep with no display and show the
socket transcript proving each named surface was actually reached — `panel.list` and
`workspace.current` after each step. Also provable: that the capture stage refuses correctly when
there is no display, with the right message and exit code.

Not provable now: the frames themselves. Say so.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  waku is the visual bar and the source of *ideas*; every line of Tiller is written from scratch.
  Reading `reference/sbs.py` is fine — that is ours.
- **A capability nobody exercised does not exist** — including this script.
- You own `tiller_terminal/**`, `tiller/src/panes.rs`, and the `Scripts/` you created.

## Reporting

Reply in **12 lines or fewer**: which surfaces the sweep covers, the socket transcript proving they
were reached, which surfaces the socket cannot reach (routed as gaps), what the capture stage does
with no display, and the honest remainder.
