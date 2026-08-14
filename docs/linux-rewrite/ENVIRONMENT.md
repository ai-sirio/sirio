# The machine, as measured

Facts about *this* box that builders keep rediscovering, each one costing a pane twenty minutes of
shell archaeology. Every line here was measured, not assumed. **Briefs should link here rather than
restate it**, and anything you discover the hard way belongs in this file the same hour.

Last measured: 2026-08-13, late evening.

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

## There is no headless critic

Running the app under **Xvfb and Xephyr both produce a window that paints nothing** — a single
unique colour in the capture. GPUI's blade renderer reports
`vulkan: No DRI3 support detected - required for presentation`. Forcing lavapipe
(`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`) clears the fatal error but not the
blankness.

**A closed avenue, documented so it is not reopened.** Verification is tied to a real display.

## Agent CLIs actually installed

| CLI | state |
|---|---|
| `claude` | installed |
| `codex` | installed |
| `opencode` | 1.18.18, installed and runs |
| `oh-my-pi` | 0.2.0 — **ships the binary `oh-my-pi`, with no `omp` alias** |
| `pi` | installed |

`tiller_agents/src/omp.rs:35` hardcodes `"omp"`. **No symlink was created to paper over that**: if
our adapter looks for a binary the distribution does not provide, the omp rows are defective for the
wrong name, not unreachable for a missing install. That distinction is the whole verdict.

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
