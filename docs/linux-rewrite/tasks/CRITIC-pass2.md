# Critic pass 2 — tabs, panes, documents, browser, window shell, persistence, git

**You are pireview, pane `w1:p6`. You are the critic.** Your context was just reset, deliberately:
you must judge this build without having seen any builder's reasoning. **You built none of this.**
Three other agents did, and their explanations are not available to you — which is the point. The
only things that count are what you run, what you see, and what you can prove.

## The standing rule

> **A feature you have not successfully exercised does not exist.**

Not "the code looks right". Not "the test passes". You compile it, launch it, operate it, and
observe the result — or the entry stays unticked. If it does not compile or does not render, that
is the gap, by definition.

## Where

Worktree under judgement: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch
`linux/gpui-waku`). A Rust/GPUI rewrite of Tiller — a multi-agent coding workspace — now targeting
Linux. No webview, no HTML. Nothing is committed; the state is the working tree.

```bash
source ~/.cargo/env && cd <tree>/rust
cargo build -p tiller -p tiller_control      # tillerctl is a BINARY of tiller_control;
                                             # `-p tillerctl` fails and leaves a stale binary
env -u WAYLAND_DISPLAY DISPLAY=:2 GPUI_X11_SCALE_FACTOR=1 ./target/debug/tiller
```

**Three builders are editing this tree while you work.** A build failure may be somebody's change in
flight rather than a defect — right now codex12 is mid-change in `tiller/src/main.rs`. So:
**`cp -a` the whole worktree to a scratch directory and judge the copy**, and record in your report
the time you copied and whether it built. That gives you a tree that holds still.

**`cp -a` matters, and so does copying `.git`.** Last pass you judged a snapshot with no `.git`, and
every git-dependent entry fell to UNREACHABLE — roughly 16 entries never got tested. `tiller-linux`
is a git *worktree*, so its `.git` is a small file containing an absolute `gitdir:` pointer; a
copy still resolves it. Verify with `git -C <copy> status` before you start, and if it does not
resolve, say so and use the fallback below instead of writing off the whole area again.

**Build your own repository to test against.** Tiller's git surfaces act on *projects the user
adds*, not on Tiller's own source. So `git init` a scratch repo you fully control, give it a known
shape, and add it as a project in the app:

- at least two commits, so there is a HEAD and a history
- a second branch, and a `git worktree add` checkout, so the worktree surfaces have something real
- **one staged file, one modified-but-unstaged file, one untracked file, all at once** — the three
  buckets the Changes surface must separate
- a file with a large unchanged middle, so collapsed-context bands have something to collapse

Now every git entry is reachable and you know the right answer in advance, which is the only way to
catch a surface that renders plausible nonsense.

## Display

Only a display with **DRI3** can present — Vulkan requires it on X11. Xvfb and Xephyr do not have
it: the window maps and every frame comes back black, which is indistinguishable from "the UI drew
nothing" unless you check. **A capture with one distinct colour means presentation failed, not that
the UI is empty.** Your own `Xwayland :2 -ac` worked last pass; keep using an isolated display,
because several instances of this app run on the session's `:1` at once and you will otherwise
photograph or type into another agent's build. That happened twice and produced three false "dead
UI" verdicts.

A nested Xwayland starts with an unconfigured 640x480 output — your frames were that size last
pass. Fix it once, after the server is up:

```bash
DISPLAY=:2 xrandr --output XWAYLAND0 --mode 1920x1080
```

Tools, both of which fail loudly rather than quietly, and match `_NET_WM_PID` so they cannot
photograph a stranger's window:

```
Scripts/linux-shot.sh  <out.png> [settle] [display]
Scripts/linux-drive.sh <out.png> '<actions>' [settle] [display]
```
`linux-drive.sh` exposes `click x y` (window-relative), `type "text"`, `key <keysym>`, `shot [path]`.
Input must go through absolute coordinates and plain XTEST — this app is an X11 guest of a Wayland
compositor, so `xdotool --window` targeting delivers nothing and a whole run was once written off
as "input does not work" because of it.

The control socket is usually a better instrument than clicking, and it is not a shortcut around
the rule — it is the app doing the work. `tillerctl` speaks it, and the pane methods
(`panel.create` / `panel.write` / `panel.read` / `panel.wait`) let you drive terminals precisely.
`panel.read` returns base64 and needs ANSI stripping. **Cross-check the two**: what the UI paints
against what the socket reports. Disagreement between them is the most valuable finding you can
produce — that is exactly how the sidebar's selection was caught being cosmetic.

## What to judge in this pass

The contract is `docs/linux-rewrite/01-inventory-app.md` and `02-inventory-packages.md` — 388
entries, each `- [ ] F-XXX-NN | capability | VERIFY: how to check it | SRC: the Swift original`.
**The VERIFY clause is your script. Follow it.**

Untouched so far, in priority order:

| Section | Entries | Note |
|---|---|---|
| `## Tabs, panes, and navigation` (F-TAB) | 28 | the largest untested block |
| `## Files, changes, and activity panels` (F-CHG) | 22 | partly covered 03:00; finish it |
| `## Documents and editors` (F-EDIT) | 13 | |
| `## Persistence and lifecycle` (F-PERSIST) | 13 | needs restarts — see the crash note |
| `## Window and application shell` (F-WIN) | 12 | |
| `## Browser surface and permissions` (F-BRW) | 9 | may be absent entirely; say so plainly |
| git-dependent entries across sections | ~16 | previously UNREACHABLE; now reachable |

**On macOS keyboard chords.** Many entries name `⌘1`–`⌘9`, `⌘W`, `⌘⌥`+arrows. This is a Linux
build; the equivalent is Ctrl. **Judge the capability, not the chord** — "jump to the Nth tab by
keyboard" either works or does not — and record which chord actually worked. Do not fail six
entries over a modifier key, and do not pass one you never pressed.

**Verdicts, used precisely:**

- **PASSED** — you exercised it and it did what the entry says. Cite the evidence: the frame, the
  command output, the nonce, the file on disk.
- **FAILED** — you exercised it and it did not.
- **UNREACHABLE** — cannot be tested here for a stated external reason: `opencode` and `omp` are not
  installed on this machine (entries naming them are UNREACHABLE, **not** FAILED); only `claude`
  speaks ACP. Name the reason every time.
- **N/A — platform** — macOS-only machinery with no Linux subject, e.g. the TCC permissions screen.
- **NOT EXERCISED** — you ran out of time. Honest and useful. Never quietly upgrade it to PASSED.

## The silent crash — expect it, and log it

The app dies silently every 4–13 minutes: no panic, no log, no message. Five deaths observed across
two builds and two displays. **codex11 is hunting it right now**, so you do not need to diagnose it
— but you are the best-placed observer of it, so when it happens record the wall-clock time, the
uptime, and what was on screen. That evidence goes straight into the hunt.

**Do not attribute a death to the feature you were testing** unless you can reproduce it on that
feature and not otherwise. Equally: it makes the persistence entries harder, because a crash is not
a clean restart — say which restarts were clean and which were deaths.

## Method rules, learned the hard way

- **Measure pixels, do not describe them**: `convert <png> -crop 1x1+X+Y -format '%[pixel:p{0,0}]' info:`.
  Several confident visual readings were flatly wrong before this rule.
- **Check the whole set before doubting the witness.** Two near-miss false accusations came from a
  single unrepresentative sample each.
- **Failing to reproduce a defect is not evidence of its absence** — especially one described as
  non-deterministic. The crash was once written off after one failed reproduction; five more
  deaths followed.
- **Append, never rewrite.** Your report lost three earned sections to a half-applied replace. A
  superseding finding gets a **new dated section** that says what it supersedes.

## Where to write

`<the live worktree>/docs/linux-rewrite/CRITIC-baseline.md` — your pass-1 report, 454 lines, 15
sections, is already there and has been backed up. **Append a new section headed
`## PASS 2 — <area> — <time>`.** Do not touch what is above it.

You are the critic: **you do not fix anything.** No edits to any `rust/**` source. A defect you find
is written down, named precisely, and routed by the orchestrator to the builder who owns the file.

## Reporting

Reply in **12 lines or fewer**: how many entries you exercised, PASSED / FAILED / UNREACHABLE /
NOT EXERCISED counts, the two or three findings that matter most, and — the part the whole loop
turns on — **the single biggest gap**: the one thing that, fixed, would most improve this build.
Name one, not a list.
