# P31 — Socket doors for Changes and Settings, the last two surfaces automation cannot reach

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need.

## P28 closed both defects you found, and the pane one is proven properly

**F-PER-03 fixed.** Before: `pane-0, pane-1` went in and `pane-0, pane-1` came back. After:
`pane-0, pane-1, pane-2` all survive a relaunch, and the store contains the split event. You made
the pane tree replay through a recorded `PaneEvent` history rather than trying to serialise the tree
shape — which is the choice that keeps working when the tree gains a node type.

**F-PER-01 half-closed**, and correctly so: schema-backed scrollback storage bounded at 256 KiB,
with the renderer-owned capture seam **routed to codex11 rather than reached into**. Nonce
`P28_SCROLLBACK_NONCE_…` observed before a real process exit. `CI OK`.

That routing has been passed on; codex11 is editing `tiller_terminal` right now, so it lands in the
right hands at the right moment.

**One thing to know:** pi touched `rust/crates/tiller/src/main.rs` — about five lines, for settings
routing — and flagged it rather than leaving you to find it. `SettingsSnapshot` lost `Copy` and is
now `Clone`, because it carries a `socket_path`.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
./Scripts/ci-linux.sh                       # must print CI OK
cd rust && export TILLER_SOCKET=/tmp/codex12.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
./target/debug/tillerctl current-workspace
```

No display on this machine presents. codex11 owns `tiller/src/panes.rs`, `tiller_terminal/**` and
the `Scripts/` it wrote; pi owns `tiller_ui/**` and `tiller_theme/**`.

## The gap

codex11 built `Scripts/visual-sweep.sh` — one command that walks the app through every surface and
captures each, so that the moment a display exists the whole visual evidence set is one run away.
It drives state **through the control socket** rather than through coordinate clicks, deliberately:
clicking has been the most fragile thing in this project, it moves the operator's real pointer, and
when it goes wrong it silently drives another agent's window.

It reached chat, terminal, split layouts and the files tree. It recorded that **Changes and Settings
cannot be opened over the socket**, and wrote that down as a gap instead of clicking around it.

That gap costs more than the sweep. It also means the critic cannot exercise either surface
headless — and headless is currently the only way anything can be exercised at all. Two of the
app's largest surfaces are therefore unverifiable, including the one carrying ~20 inventory entries.

## What to build

Socket methods that **open and inspect** those two surfaces. At minimum:

- **Changes** — open it for a given worktree, and report what it is showing: the three section
  counts (staged / changed / untracked), the files in each, and per-file `+N -M`. A critic must be
  able to compare that against `git status --porcelain` on the same repository and see them agree.
  That cross-check is the strongest instrument this project has, and it is what caught F-009.
- **Settings** — open it, select a section, and report what the section currently shows. Enough that
  the provider availability badges pi just wired to `tiller_agents::discover_availability()` can be
  read back and compared with what is actually on `PATH`.

Follow the naming scheme already in `system.capabilities`, and **advertise the new methods there** —
a method that works but is not listed is invisible to anyone writing automation, and one that is
listed but does not work is worse. Check for both.

**The rule that applies here more than anywhere:** the socket method and the UI affordance must call
**the same function**. Not a parallel path that reports what it believes the UI would show. If the
socket computes its own answer, it can agree with `git status` while the user's screen shows
something else entirely — which is precisely the class of defect this project has hit four times
(F-009's re-highlighting handler, `ChangesTab` mounted only by a demo binary, a font token five call
sites ignored, and availability badges that were literals). **A socket that reports what the surface
actually holds is a test; one that recomputes the answer is a second opinion.**

## Evidence

A live headless transcript, pasted: open Changes against a fixture repository you build with
`git init` holding **one staged, one modified and one untracked file at once**, then show the socket
report and `git status --porcelain` side by side, agreeing. Then open Settings, read the provider
badges back, and show them matching `which claude`, `which codex`, `which opencode` — `opencode` and
`omp` are **not installed** on this machine, so two of the five must come back unavailable. If they
do not, that is a finding.

Then run `./Scripts/ci-linux.sh`.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Every line of Tiller is written from scratch; transplanted code counts as a gap, always.
- **A capability nobody exercised does not exist.**
- You own `tiller_control/**` and `tiller/src/main.rs`. If reading a surface's state needs a getter
  inside `tiller_ui/**`, that is pi's — **specify it and it will be routed**, the way codex11 routed
  the capture seam to you rather than editing your file.
- Clean up the instances you start and the sockets you create.

## Reporting

Reply in **12 lines or fewer**: which methods now exist, the Changes-versus-`git status` transcript,
the Settings badge readback against `which`, what you needed routed to pi, the `ci-linux.sh` result,
and the honest remainder.
