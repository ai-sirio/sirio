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

- **Right-click: the earlier entry here was wrong about the cause, and the correction matters.**
  This file previously stated as fact that XTEST cannot deliver button 3 under XWayland. That is
  **unproven and probably false.** What was actually true: `Scripts/linux-drive.sh` had **no
  button-3 path at all** — its `click()` helper hardcoded `xdotool click 1`, and there was no
  `rclick`. Button 1 travels that same mousemove-then-XTEST route and lands on every frame the
  critic has ever captured, and XWayland does not discriminate by button for a focused X client.
  An `rclick` helper now exists, mirroring `click` verbatim. **Verify it before trusting a verdict
  from it** — the app does bind right-click (22 `MouseButton::Right` handlers across `main.rs`,
  `right_panel.rs`, `sidebar.rs` and `tiller_terminal/lib.rs`), so if it still does nothing, that is
  a finding rather than a restriction.

  The original judgement stands even though its reason did not: a right-click that "does nothing" was
  the harness, **not** a defect, and twelve rows were one step from being recorded as false
  negatives. The lesson is that "the platform forbids it" is the most expensive kind of wrong answer,
  because it closes the avenue — check for a missing helper before concluding a restriction.
- **Keyboard chords land only after a real click** has given the app X focus. Click first, then send
  the chord.
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

## Shared build target

All panes share one cargo target directory, so a `cargo test --workspace` taken while others build
holds the lock and stalls them. Prefer `-p <crate>` while the roster is busy, and expect an
occasional wait rather than a hang.
