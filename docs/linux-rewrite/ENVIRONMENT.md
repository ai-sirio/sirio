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
  screenshot even when it is open.

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
