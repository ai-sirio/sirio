# The machine, as measured

Facts about *this* box that builders keep rediscovering, each one costing a pane twenty minutes of
shell archaeology. Every line here was measured, not assumed. **Briefs should link here rather than
restate it**, and anything you discover the hard way belongs in this file the same hour.

Last measured: 2026-08-13, late evening. **Then the machine changed — read the next section before
anything below it.**

---

# 2026-08-18 — the box changed AGAIN: back on x86, 12 cores, and BOTH lanes are alive

The Pi 5 section below is history. Measured 2026-08-18 on the machine this work now runs on.
**Translate every absolute path from the Pi section; do not follow it.**

| | measured 2026-08-18 |
|---|---|
| host | x86_64 desktop, **12 cores**, 31 GB RAM, 352 GB free on `/` |
| worktree | `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku` |
| GPU | **AMD** (`0d:00.0` VGA, device 7590), `/dev/dri/card1` + `renderD128`, Vulkan 1.4.318, Mesa 25.2.8 — real hardware |
| desktop session | **COSMIC** (Pop!_OS) on Wayland, socket `wayland-1` in `/run/user/1000` |
| `cargo build --workspace` | **exit 0 in 14.5 s** (warm target), 2 warnings (`browser.rs:827` `pump_task`; `main.rs:9864` `sidebar_projects`) |
| workflow fan-out cap | `min(16, cores-2)` = **10 agents at a time** (the Pi's ceiling of 2 is gone) |
| agent CLIs | `claude`, **`codex` 0.147.0**, **`opencode` 1.18.18**, **`pi`** all present; `oh-my-pi` still upstream-broken |

## Both lanes work here. Neither needs `pi-session.sh`.

**`pi-session.sh` is Pi-only — do not run it.** The parent compositor on this box is the user's own
COSMIC session. Point the nested lane at it:

```bash
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1
export TILLER_WL_LABEL=<your-unique-label>
cp rust/target/debug/tiller /tmp/$TILLER_WL_LABEL-tiller
export TILLER_WL_BIN=/tmp/$TILLER_WL_LABEL-tiller
Scripts/wayland-drive.sh /tmp/$TILLER_WL_LABEL-shots '<actions>' 15
```

Verified 2026-08-18 10:41: booted, captured `1715x972 · 6792 colours`, full UI — sidebar, tab bar,
Chat/Terminal tabs, Files panel listing the real repo, status bar with live Claude usage.
`settle` 15 is enough here; the Pi needed 20–30.

**`DISPLAY=:1` is ALIVE again** — COSMIC runs a rootless XWayland on the real AMD GPU
(`Xwayland :1 -rootless`, `/tmp/.X11-unix/X1`, `xdpyinfo` answers). The Pi's "X11 is not a lane"
finding does **not** hold here. This is what the `F-BRW` bucket needs.

> **`:1` is the user's own desktop X server, not a dedicated one.** Synthetic XTEST input there
> moves a real person's pointer. Before driving it, prefer an isolated nested X display; if you do
> use `:1`, take `Scripts/linux-drive.sh`'s lock and say in your evidence that you used the shared
> session.

`oh-my-pi` remains unrunnable: `bin/oh-my-pi.js` carries TypeScript annotations behind a
`#!/usr/bin/env node` shebang, so it dies at `bin/oh-my-pi.js:176` under **both** node and bun
(re-measured 2026-08-18). Upstream defect, not an environment gap.

---

# 2026-08-17 — the box is a Raspberry Pi 5 now, and everything below was measured on a different one

Every absolute path in the rest of this file, in `STATE.md`, `QUEUE.md` and the P-reports —
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, `/home/enzopalmisano/Hello-World` — belongs
to an x86 desktop that is no longer the machine. **Translate; do not follow.**

| | measured 2026-08-17 |
|---|---|
| host | Raspberry Pi 5, **aarch64**, 4 cores, 7.7 GB RAM, NVMe with 434 GB free |
| worktree | `/home/epalmi/tiller`, branch `linux/gpui-waku` |
| GPU | Broadcom **V3D** via `/dev/dri/renderD128` — real hardware, not lavapipe |
| `cargo check --workspace` | **exit 0**, 7 m 07 s, 2 warnings (`browser.rs:827` `pump_task` never read; `main.rs:9482` `sidebar_projects` never used) |
| `rust/target` | 5.1 GB · `target/debug/tiller` 284 MB |
| agent CLIs | **`claude` only** (`~/.local/bin/claude`), plus `node`/`npx`/`bun`. `codex`, `opencode`, `pi`, `oh-my-pi` are **absent** — every row that needs one is environment-blocked here for a different reason than it was on the old box |
| `cargo` on PATH | still no. `export PATH="$HOME/.cargo/bin:$PATH"` |

## The display: a headless sway that renders on the real GPU

There is no monitor. A **parent** sway runs on the wlroots *headless* backend and still composites
through V3D, so GPUI's Vulkan renderer paints real pixels and `grim` captures them:

```
WAYLAND_DISPLAY=wayland-1   XDG_RUNTIME_DIR=/run/user/1000    # the parent compositor
wayvnc -o HEADLESS-1 127.0.0.1 5900                           # a human can watch over VNC
```

X11 is **not** a lane here: XWayland on V3D fails at swapchain creation. The `DISPLAY=:1` lane and
its single-holder drive lock — the throughput ceiling this project spent a day working around — do
not exist on this box. `F-BRW` rows, which WAYLAND-LANE.md sends to `DISPLAY=:1`, have no lane at
all here.

## `Scripts/wayland-drive.sh` works unmodified, and it is the lane

Verified 2026-08-17 10:26: it boots its own **nested** sway inside the parent above, launches the
app with a private SQLite DB and control socket, and captured `1715x972 · 5434 colours`.

```bash
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1
export TILLER_WL_LABEL=mylabel
Scripts/wayland-drive.sh /tmp/shots 'ctl project.add path=/home/epalmi/tiller
                                     shot sidebar' 8
```

The full vocabulary is live on this box — `click`, `rightclick`, `down`/`up`/`drag`, `scroll`,
`chord`, `modclick`, `xdnd`, `type`, `key`, `title`, `ctl`, `shot`. No lock, any number of
instances in parallel.

**`TILLER_WL_BIN` pins the binary** (added 2026-08-17). A critic judging a wave must not have the
binary swapped under it by a builder rebuilding the shared `rust/target`:

```bash
cp rust/target/debug/tiller /tmp/mylabel-tiller
export TILLER_WL_BIN=/tmp/mylabel-tiller
```

### The trap that cost a session an hour

A session on this Pi built a compositor, a VNC server and a from-scratch RFB input driver in Python
before anyone checked whether the repo already had a drive lane. It did, it was far richer — the
hand-rolled driver had no modifiers, no drag, no scroll, no screenshot — and it worked here on the
first try. **Check `Scripts/` before building an instrument.** The same lesson as the right-click
finding below: the expensive wrong answer is the one that closes an avenue you never tested.

## The box gets starved by leaked agent infrastructure, not by the build

Measured 2026-08-18 00:00, with two builders mid-piece: **load average 67 on four cores, 202 MB
free.** The build was not the cause. `cargo`/`rustc` accounted for three processes; the load came
from **52 `bun` processes holding 3 GB and 380 % CPU** — every core, spent on nothing.

Two distinct leaks, and it is worth telling them apart:

- **Orphaned MCP servers.** Each Claude session and subagent spawns the Telegram plugin's
  `bun server.ts`, and they do not exit when the session does. 28 had outlived their session, 19 of
  those ignored `SIGTERM`. Killing them returned ~2 GB and dropped the load by 20.
- **`ccstatusline` churn.** A fresh `bun` starts per status-line refresh, per live session. With
  dozens of sessions alive that is a continuous storm of process startups, which is what a load
  average of 67 against ~4 runnable processes actually measures.

Find the orphans by walking each `bun`'s parent chain and keeping only those with a live `claude`
ancestor — never by age or by `pkill -x bun`, which would kill the live sessions' own servers:

```bash
ps -eo pid=,ppid=,rss=,comm=      # then walk ppid up; no live claude ancestor => orphan
```

**Why this belongs in a file about correctness, not housekeeping.** Under that starvation the
lane lies. A critic in this session recorded that roughly half its drives returned a blank first
frame and that individual synthetic clicks were dropped — **two apparent `F-CORE-ACT-23` failures
turned out to be dropped clicks**, and were only caught because that critic re-ran every case
until it was unambiguous. A builder in the same window hit `collect2: ld terminated with signal 9`
linking the test binary. Starvation reads exactly like "the app ignored my input" and exactly like
"the linker is broken", and both are the shapes this project has already mistaken for findings.

**Never record a negative from a single dropped input.** Check `free -h` and `uptime` before
believing a blank frame.

## Rust toolchain

`cargo` **is not on the PATH** in the codex panes, though it is installed:

```bash
/home/enzopalmisano/.cargo/bin/cargo          # a rustup symlink; works
export PATH="$HOME/.cargo/bin:$PATH"          # or fix it once per shell
```

`cargo: command not found` therefore means a PATH gap, **not** a broken toolchain. Do not install
anything.

## System libraries

GTK3, WebKit2GTK 4.1, libsoup3 and libxdo headers are installed **globally**
(`libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libxdo-dev`). The `wry` dependency behind the
browser surface drags in GTK3, which is why `cargo build -p tiller` once failed on `gdk-3.0`,
`atk`, `cairo` and `pango`.

**No `PKG_CONFIG_PATH`, no sysroot, no vendored copies.** If a build fails on a system header, say
so rather than working around it — the fix is a package, and it is the orchestrator's to install.

## Killing a stray app instance

```bash
pkill -x tiller      # correct
pkill -f "target/debug/tiller"   # WRONG — kills your own shell
```

The `-f` form matches the `bash -c` wrapper whose command line *contains* that string, so the shell
running the command matches its own pattern and dies (exit 144).

## The display, and what cannot be automated on it

The desktop session is **Wayland**. `DISPLAY=:1` is XWayland, and the app runs as an X client there.

- **Right-click works. Settled by evidence, after this file twice said otherwise.**
  Use `rclick` in `Scripts/linux-drive.sh`. Proof:
  `reference/linux-progress/p17-rclick-term.png` shows the terminal context menu open with Copy,
  Paste, Set Title, the four Splits, Clear and Close.

  This file previously stated as fact that XTEST cannot deliver button 3 under XWayland. **That was
  false.** The real cause was that `linux-drive.sh` had no button-3 path at all — its `click()`
  helper hardcoded `xdotool click 1`, and no right-click was ever sent. Button 1 always travelled
  that same mousemove-then-XTEST route, and XWayland does not discriminate by button for a focused
  X client.

  Twelve rows were one step from being recorded as false negatives on the strength of a platform
  claim that was never tested. **"The platform forbids it" is the most expensive kind of wrong
  answer, because it closes the avenue** — check that the instrument can perform the action before
  concluding the subject cannot receive it.

  One caveat for whoever drives a menu next: in that same frame the **Files panel paints over the
  context menu**, truncating every long label at the panel's edge while short ones stay whole. The
  menu itself is fine — that is a z-order defect and a separate row, not an absent surface. It is
  also *unlike* the P72 webview occlusion: there a native child window sits above the GL surface and
  cannot be reordered, whereas both of these are GPUI elements and the paint order is ours to fix.
- **Keyboard chords land only after a real click** has given the app X focus. Click first, then send
  the chord.
- **The first capture after an action often shows the frame from *before* it.** Observed by the
  critic on 2026-08-14 driving a dialog: `r0` and `r1` were identical and only `r2` had changed. The
  action had landed; the screenshot had not caught up.

  **This manufactures false negatives, and they are the expensive kind** — a control that works reads
  as a control that does nothing, so the row is marked `FAILED — absent` and somebody is sent to
  rebuild what already works. Take a second capture and compare before believing any frame that
  shows *no change*. A frame that shows the change you expected needs no second look; a frame that
  shows nothing does.
- **The portal file picker is Wayland-side and invisible to X captures.** It will not appear in a
  screenshot even when it is open. XDND drags are equally out of reach: `xdotool` has no source
  window to negotiate the protocol, so file-drop rows are unexercisable by this harness (a human
  hand can still do them — record NOT EXERCISED with the instrument reason, never FAILED).
- **Input events QUEUE while the app's main loop is busy, and deliver late IN ORDER.** Measured
  pass 17 (p17-ar1..ar3): after a relaunch that restored five PTY panes, a context menu stayed
  open ≥24s after the right-click, two follow-up clicks and a typed sentence appeared to do
  nothing — two frames 4s apart identical — then the ENTIRE queued sequence executed correctly
  at once (menu item fired, agent booted, buffered keystrokes reached its PTY). Allow 15–20s of
  settle after launching an app that restores many panes, or every coordinate you clicked was
  right and every frame you shot says nothing happened.
- **`xdotool type` is ASCII-only in practice**: an em-dash (—) in the typed string comes out as
  the literal text `nosymbol` (p17-ap3). Keep typed prompts to plain ASCII.
- **Coordinates belong to a layout, not to the app.** Two pass-17 misclicks came from reusing
  coordinates across differently-populated layouts: the error banner's Retry button sits at the
  TOP of the transcript when the stream died early but is pushed down by whatever streamed first,
  and sidebar rows shift as persisted tabs accumulate across fixture-DB runs. Anchor clicks in a
  frame from the SAME run/layout, never a remembered one.
- **Only one agent may drive the display at a time.** Measured 2026-08-14, 01:22, the hard way.

  `linux-drive.sh` solves **which window to photograph** — it matches `_NET_WM_PID` against the
  process it launched, precisely so nobody drives a stranger's instance. It does **not** solve
  **which window receives the click.** `xdotool mousemove` moves the one global X pointer, and the
  click goes to whatever window is under it. So two simultaneous drivers corrupt each other's input
  while each still photographs its own window correctly.

  The symptom is a frame that says *the control did nothing* about code that is fine — **a false
  negative, the expensive kind**, and indistinguishable from a real one by inspection.

  Proof: the identical sequence (right-click a file row, click `Open`) opened the editor in one
  capture and left the menu sitting open in the very next, with nothing else changed.

  **Claim the display before a drive batch and say so in your pane.** If your captures overlap
  somebody else's window in time, re-take any frame that shows *no change* before writing it to the
  ledger — same rule as paint lag, different cause.

## The drive lock, and its one trap

`Scripts/linux-drive.sh` serializes display access with an atomic `mkdir` lock
(default `/tmp/tiller-drive-1.lockd` for DISPLAY=:1), self-healing when the holder pid is dead
or the hold exceeds 30 minutes. Export `TILLER_DRIVE_LABEL=<you>` so the holder file names you.

**The trap:** pointing `TILLER_DRIVE_LOCK` at a path where a REGULAR FILE already exists (e.g.
left over from the earlier flock design) makes `mkdir` fail EEXIST forever — exit 6 with
`Holder: unknown`, and the self-heal cannot help because there is no pid to read. `rm -f` the
stale file or use the default path. The flock design it replaced had the opposite failure:
fd 9 inherited by a long-lived reparented process kept the lock held for a finished run and
deadlocked every driver on the machine for an hour.

### Holding the lock yourself, for a drive `linux-drive.sh` cannot do

`linux-drive.sh` is launch → act → capture → kill. Some work needs the app **alive across many
steps**: running an agent until a hook fires, waiting on a stream, proving a settings round-trip
survives a relaunch. That work moves the same single global pointer, so it needs the same lock — and
having no documented way to hold one is exactly why `codex12` came to drive with raw
`xdotool windowactivate` at 03:13 on 2026-08-14 while two other agents were capturing.

Take it the way the script does, so the same self-healing applies:

```bash
LOCKDIR=/tmp/tiller-drive-1.lockd            # the DISPLAY=:1 default
until mkdir "$LOCKDIR" 2>/dev/null; do
  hpid=$(sed -n 's/^pid=\([0-9]*\).*/\1/p' "$LOCKDIR/holder" 2>/dev/null)
  # self-heal: the recorded holder died without cleaning up
  if [ -n "$hpid" ] && ! kill -0 "$hpid" 2>/dev/null; then rm -rf "$LOCKDIR"; continue; fi
  sleep 5
done
printf 'pid=%s label=%s since=%s\n' "$$" "${TILLER_DRIVE_LABEL:-unlabelled}" "$(date -Is)" \
  >"$LOCKDIR/holder"
trap 'rm -rf "$LOCKDIR"' EXIT INT TERM       # the trap is the point — a lock you forget to
                                             # release is worse than never taking one
```

**`import -window` is not protection.** It photographs your own window correctly even while
`windowactivate` and `xdotool key` are stealing focus from everybody else. So a raw driver's own
frames look plausible while it corrupts other agents' input — the false negative above, arriving
from the direction you are least likely to check.

## Driving with a fixture database

`TILLER_DB=/path/to/fixture.sqlite` points the app at a scratch database — the pass-17 pattern
for exercising persistence rows without touching `~/.local/state/TillerRust`. Two facts to hold:
state ACCUMULATES across drives (each run's tabs/worktree selection persist into the next run's
restore, shifting sidebar geometry under previously-valid coordinates), and reading the fixture
mid-run needs the same WAL-aware read-only open as the real DB (section below).

## Headless rendering: closed on X11, open on Wayland

Running the app under **Xvfb and Xephyr both produce a window that paints nothing** — a single
unique colour in the capture. GPUI's blade renderer reports
`vulkan: No DRI3 support detected - required for presentation`. Forcing lavapipe
(`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`) clears the fatal error but not the
blankness. **On X11 this is a closed avenue** — do not reopen it.

**On Wayland it works.** Verified 2026-08-14: under a nested headless `sway`, the app renders the
complete UI and `grim` captures real pixels. X11 fails because GPUI must present into an X drawable
through DRI3; Wayland succeeds because the client hands a `wl_buffer` to the compositor, which
composites it in software. Same lavapipe, same absent GPU, opposite outcome.

This retires the old claim that "verification is tied to a real display". It is not — **input** is.
The Wayland lane renders and captures but cannot deliver a click or a keystroke, so gestures still
need `DISPLAY=:1` and the drive lock, while everything visual that can be driven over the control
socket now runs in parallel without it.

**Setup, limits and the four traps: `WAYLAND-LANE.md`.** Read it before using the lane — two of the
traps (lazy repaint, and input tools that report success while doing nothing) manufacture false
verdicts in both directions.

## Agent CLIs actually installed

| CLI | state |
|---|---|
| `claude` | installed |
| `codex` | installed |
| `opencode` | 1.18.18, installed and runs |
| `oh-my-pi` | 0.2.0 — **ships the binary `oh-my-pi`, with no `omp` alias** |
| `pi` | installed |

**Corrected 2026-08-14.** This section previously stated that `tiller_agents/src/omp.rs:35`
hardcodes `"omp"` and that the adapter therefore seeks a binary the distribution does not ship. That
reading was of the wrong function. The adapter keeps two distinct names, and only one of them is a
binary:

```rust
fn id(&self)              -> &'static str { "omp" }        // :35 — Tiller's internal identifier
fn executable_name(&self) -> &'static str { "oh-my-pi" }   // :43 — the actual binary
// and both command builders spawn the real name:
//   :69  oh-my-pi --hook <file>
//   :81  oh-my-pi --hook <file> --resume=<ref>
```

`id()` is what pass 16 read as a binary name. The launch path uses `oh-my-pi`, which is what the
distribution installs, so the adapter is name-correct today.

**No symlink was ever created**, and none is needed — that decision still stands and is still the
right one: papering over a name mismatch with a symlink would have hidden whether the adapter or the
environment was wrong. Keep it that way if this ever regresses.

The omp rows are therefore *unexercised*, not defective: nothing blocks a launch and nobody has
launched one. Verify through the hook — `tillerctl notify` arriving from the generated hook file is
the clause — because a session that launches but emits nothing is a different failure from one that
cannot launch at all.

Note also that the npm package `omp@1.0.0` is a squatted placeholder — its description is literally
`"new"`. It is not the project and must not be installed.

## Reading the persistence database

The app's SQLite lives under one directory per checkout:

```
~/.local/state/TillerRust/checkouts/tiller-linux-ea1b05ec/tiller.sqlite
```

`session.rs:166` derives it from `XDG_STATE_HOME` (or `$HOME/.local/state`) on Linux.

**There is no `sqlite3` CLI on this box**, and none is needed — Python ships the same library:

```bash
python3 -c "
import sqlite3, os
db = os.path.expanduser('~/.local/state/TillerRust/checkouts/tiller-linux-ea1b05ec/tiller.sqlite')
con = sqlite3.connect(f'file:{db}?mode=ro', uri=True)
print(con.execute('PRAGMA user_version').fetchone())
"
```

**The database is in WAL mode, and this is a false-verdict trap.** Copying `tiller.sqlite` on its own
gives a stale snapshot — recent writes are still in `tiller.sqlite-wal`, which was 61 KB when this was
measured. A persistence row checked that way reads as *not persisted* when it persisted fine. Either
open the live file read-only as above (SQLite then reads the WAL), copy **all three** of
`.sqlite`, `-wal` and `-shm` together, or quit the app first so it checkpoints.

Measured 2026-08-14: `user_version` 11, twelve tables. `browser_origin_grant` exists and is empty —
the schema half of `F-BRW-07` is real; it has zero rows because nothing can grant an origin until the
browser surface is mounted.

## Shared build target

All panes share one cargo target directory, so a `cargo test --workspace` taken while others build
holds the lock and stalls them. Prefer `-p <crate>` while the roster is busy, and expect an
occasional wait rather than a hang.

## Working with the orchestrator: never idle on an approval gate

The `superpowers` workflow asks for design approval before implementation, and **the orchestrator
is the approver on this project** — the user is asleep and has instructed that recommended actions
be taken without waiting. Post your design, then **keep working on everything that does not depend
on the answer.**

Two agents lost roughly an hour each this way on 2026-08-14: one stopped on a single
`system.capabilities` policy question while the whole of its dispatch split, reply channel, error
type and test rewrite were independent of it; another finished a spec and idled. In both cases the
pending decision was worth **one constant**.

The habit that fixes it: **structure the work so the undecided part is one line.** Put the disputed
list in a named constant, the disputed threshold in a `const`, the disputed copy in one string —
build everything around it, and change that one line when the answer arrives. Then ask, and carry
on.

Block only when proceeding under either answer would waste the work. That is rare; it is not the
common case, and it has never yet been the case on this project.

## A shared file is not a reason to leave work uncommitted

Four agents edit one worktree, so `main.rs`, `settings.rs` and `chat.rs` are usually dirty with
somebody else's in-progress work. Twice on 2026-08-14 an agent finished a piece and **deliberately
left it unstaged** for that reason — once on `main.rs`, once on `settings.rs`. Both were being
careful, and both were choosing the larger risk: **this repo has already lost its object database
once**, and uncommitted work is exactly what that destroys.

Git's finest granularity here is the file, so staging a shared file necessarily stages whatever the
other agent has in it right now. That is acceptable. What is not acceptable is committing a **red**
intermediate state, because that cost lands on everyone at once.

The protocol:

1. `cargo check -p <crate>` — or the narrowest gate that covers the file.
2. Green → commit path-scoped, and **say in the commit message that the file may carry concurrent
   edits from another agent.** An honest message costs nothing; a silent one makes the next
   bisect lie.
3. Red → attribute the failure before you act on it. If the cause is in files you never touched, it
   is someone's intermediate state: **say whose and which files in your report**, keep your own work
   staged-but-uncommitted only until they land, and do not try to fix it. On 2026-08-14 `codex11`
   did this correctly — 11 errors in `tiller_ui`, attributed to `P91`'s unfinished `raw_output`
   fields in `chat.rs`/`tiller_acp`, files it had not opened.

Never `git add -A`. Never commit another agent's work under a message describing only your own.

## `oh-my-pi` cannot start on this machine, and it is not our bug

Every `F-AGENT-OMP-*` row is blocked upstream of Tiller. Verified 2026-08-14 while judging `P96`:

```
$ oh-my-pi …
SyntaxError: Unexpected token ':'
```

`oh-my-pi@0.2.0` ships **un-transpiled TypeScript in a file Node is told to execute**:
`bin/oh-my-pi.js:176` is `function checkFile(path: string, label: string) {`, and the file opens with
`#!/usr/bin/env node`. Node cannot parse it, so the process dies before any session starts.

**Tiller is invoking the correct name.** The package manifest declares exactly one binary —
`"bin": { "oh-my-pi": "./bin/oh-my-pi.js" }` — which is precisely what the adapter's
`executable_name()` returns. There is nothing to fix on our side, and the earlier
distribution-name/executable-name conflation is genuinely fixed.

So `F-AGENT-OMP-01` and `-02` are `NOT EXERCISED` **with an instrument reason**, not `FAILED`. Do not
re-drive them, and do not "fix" the adapter to work around a broken third-party package — that would
make our code wrong in order to make a bad build run.

**For the user, when you are awake:** this needs `oh-my-pi` reinstalled or pinned to a version whose
published `bin` is actually JavaScript. It was deliberately left alone rather than downgrading a
global npm package unattended.

## The critic pane came back on a different model — the rule is *not the builder*, not *pireview*

**Resolved 2026-08-14 (afternoon):** `pireview` (`w1:p6`) now runs `glm-5.3` on Zai and accepts
dispatches normally. The block below is kept because the *failure shape* recurs, not because the
pane is still down.

Earlier that day, on deepseek-v4-pro/Opencode Go, it refused every dispatch:

```
Error: 429: {"type":"GoUsageLimitError","message":"Monthly usage limit reached. Resets in 10 days…"}
```

It accepted the text and failed at the model call, so **a dispatch to it looks delivered and simply
never runs.** Read the pane back after dispatching, or you will believe a critic is working when
none is.

**What this changes.** "The critic runs on `pireview`" was never the real constraint; it was where
the role happened to live. The real invariant is:

> **The critic must not be the agent that built the piece.** A builder judging its own work is not
> a second opinion, it is the same opinion with more confidence.

So the role **rotates** among whoever is live: `codex11`, `codex12`, `sonnet`, `fable`. When you
hand out a critic pass, name the builder in the brief and pick anyone else. A critic still starts
fresh, still exercises live, and still may not accept a green test as `PASSED`.

`P101` is the first brief affected — it critiques `codex11`'s `P95`, so it may go to anyone
**except `codex11`**.

## Two herdr gestures that report success and do nothing

Both were found on 2026-08-14 by reading the pane back. Neither errors; the only tell is that
`herdr agent list` still says `agent_status: idle` after you were sure you had sent something.

- **Submitting a dispatch to a codex pane.** After `herdr pane send-text`, `herdr pane send-keys
  <pane> enter` inserts a **newline into the composer** instead of submitting, and the pane stays
  `idle` however many times you press it. Use **`herdr pane run <pane> "<text>"`** — it sends text
  and Enter in one call, and it is the gesture that actually submits. Make it the default dispatch
  verb for every pane, not just codex.
- **Cycling a Claude Code pane out of `⏸ manual mode`.** `herdr pane send-keys <pane> shift+tab`
  is accepted — no `invalid_key` error — and is inert. `S-tab`, `btab` and `shift-tab` are
  rejected outright. What works is sending the raw backtab escape as text:

  ```bash
  herdr pane send-text <pane> "$(printf '\033[Z')"     # → ⏵⏵ accept edits on
  ```

  Generalise it: for any TUI chord herdr does not model, send the raw terminal escape as text.

**Counting keypresses is not evidence.** Confirm every dispatch with `herdr agent list` and require
`working` before you believe the pane received it.

### `/clear` queues behind an in-flight turn

A Claude Code pane that is mid-turn takes `/clear` as a **queued message** — the footer says
"Press up to edit queued messages" and the context gauge does not move. Send `esc` to interrupt
first, then clear, then confirm the gauge reads `0/1.0M`. A pane you believe is fresh and is
actually carrying 227k of a previous task is a critic that is no longer independent.

## Approval gates: the shape they take, and what may be cleared without the user

Both agent CLIs gate constantly, and each gate stops the pane dead. The prompts differ:

- **codex** — `› 1. Yes, proceed (y) / 2. Yes, and don't ask again for these files (a) / 3. No`.
  Option 2 is usually the right one: it clears the whole piece rather than the next line of it.
- **Claude Code** — `❯ 1. Yes / 2. Yes, allow reading from <dir> from this project / 3. No`.
  Option 2 is durable across the session; option 1 gates again immediately. A pane whose cwd is
  outside the worktree gates on **every** `cd … && git …`, so take the broadening option the first
  time it is offered.

`scratchpad/gatekeeper.py` polls `herdr agent list`, and for a blocked pane parses the options and
clears it **unless the screen matches a denylist** (`rm -rf`, `sudo`, `git push`,
`git reset --hard`, `git clean`, `git checkout --`, `git rebase`, `--force`, `curl`, `wget`, `ssh`,
`pkill`, `systemctl`, package installs, `.ssh/`, `credentials`, `gh pr/issue`). A refusal is logged
as `REFUSED` and left blocked for a human. It prefers the broadening "don't ask again" option when
one is offered.

**It is deny-primary on purpose.** The first version was the inverse — approve only what matched an
allowlist — and it stalled panes on `grep`, on `git ls-files`, on overwriting a markdown report, and
on `ls && git branch`, whose only sin was a gate shape the parser did not recognise. Recognising a
gate's shape is not a safety property, and a gatekeeper that blocks benign work is not "safe", it
just moves the whole roster's throughput onto one human. The agents are confined to a
version-controlled worktree, so reversible work needs no approval; the denylist is for what is not
reversible.

Two shapes it deliberately still refuses, because they want a human glance: any `rm -rf`, even the
documented drive-lock self-heal and scratch-fixture setup, and anything touching credentials.
Expect to clear those by hand.

The standing instruction is that recommended actions are taken while the user is asleep. That
covers reversible work inside a version-controlled worktree. It does not cover anything that
leaves the machine or destroys history, and those stay blocked no matter how long a pane waits.
