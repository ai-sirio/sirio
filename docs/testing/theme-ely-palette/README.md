# Theme on Ely's palette — evidence

Proof that sub-project 1 of the bezel → Ely migration
(`docs/superpowers/specs/2026-10-01-sirio-theme-on-ely-palette-design.md`)
changes no colour any consumer reads.

## Baseline

`before/` was written on `f1b578b3` plus the dump example alone:

    cd rust && cargo run -p sirio_ui --example theme_dump -- ../docs/testing/theme-ely-palette/before

| File | Holds |
|---|---|
| `sirio.tsv` | Sirio's 38 tokens, 28 combinations (7 bases × dark/light × opaque/translucent), as the `Hsla` gpui paints |
| `ely.tsv` | the `Palette` in Ely's global, same combinations |
| `bezel.txt` | bezel-theme's installed `Theme`, Debug-printed |
| `theme.txt` | Spacing, radii, typography and chrome tokens |

## After

The post-migration dump was generated from this worktree:

```bash
cd rust && cargo run -p sirio_ui --example theme_dump -- ../docs/testing/theme-ely-palette/after
```

It contains the same four files and combinations as `before/`.

## Comparison

```bash
python3 Scripts/theme/compare-theme-dumps.py docs/testing/theme-ely-palette/before docs/testing/theme-ely-palette/after | tee docs/testing/theme-ely-palette/comparison.txt
```

Result: `THEME DUMPS MATCH`. The comparison checks the Sirio and Ely colour
tokens, bezel's installed theme, and non-colour theme values.

The before/after screenshot pixel metrics are in the task's external
`task-10-pixel-diff.tsv` (70 shared frames; it is not committed). The largest
ImageMagick AE values were `notte-light/chat.png` and `empty-workspace.png`
(1863.54), followed by its `files-tree.png`, `split-layout.png` and
`terminal.png` (1863.09); `onice-light` frames followed at 1857.35. A
difference image for `notte-light/chat.png` showed changes in the bottom
status strip: usage counters and elapsed-time text advance between runs, and
the fixture path contains a new temporary directory. The application
surfaces above that strip did not change in that pair.

## Visual sweep

Captures are kept outside the repository at
`/home/epalmisano/Projects/sirio/.worktrees/theme-sweeps`. The seeded driver
captures five states for each of seven base colours in dark and light
appearance: empty workspace, chat, terminal, split layout and file tree. The
driver's JSON string settings are decoded to the raw scalar values the app
stores in its SQLite settings table.

For this machine's shared Cargo target, the baseline binary was saved before
building the current binary:

```bash
git worktree add ../theme-ely-baseline f1b578b3
(cd ../theme-ely-baseline/rust && cargo build -p sirio)
cp "$CARGO_TARGET_DIR/debug/sirio" "$HOME/.cache/sirio-sdd/sirio-baseline"
(cd rust && cargo build -p sirio -p sirio_control --bins)
mkdir -p rust/target/debug
cp "$CARGO_TARGET_DIR/debug/sirio" rust/target/debug/sirio
cp "$CARGO_TARGET_DIR/debug/sirioctl" rust/target/debug/sirioctl
```

The display is an isolated Xvfb on `:193`; these commands do not use the
operator's display or Wayland:

```bash
~/.cache/sirio-sdd/xtools/start-xvfb :193 > ~/.cache/sirio-sdd/theme-xvfb.log 2>&1 &
XVFB_PID=$!
unset DISPLAY WAYLAND_DISPLAY
export PATH="$HOME/.cache/sirio-sdd/xtools:$PATH"
OUT=/home/epalmisano/Projects/sirio/.worktrees/theme-sweeps
Scripts/Tests/test-theme-sweep.sh --out-dir "$OUT/before" --bin "$HOME/.cache/sirio-sdd/sirio-baseline" --display :193 --settle 2
Scripts/Tests/test-theme-sweep.sh --out-dir "$OUT/after" --bin "$PWD/rust/target/debug/sirio" --display :193 --settle 2
kill "$XVFB_PID"
git worktree remove ../theme-ely-baseline
```

Both runs printed `THEME SWEEP OK` and each wrote 70 nonblank frames. The
side-by-side pairs manually opened were `onice-dark`, `notte-dark`,
`slate-light` and `neutral-light`; their application surfaces looked the
same, with the observed changes limited to the dynamic footer counters and
temporary fixture path. The dump comparison is the pixel-exact token proof;
the screenshots are visual evidence.

Not verified: native rendering on macOS or Windows, or physical Wayland
rendering. The captures use Linux under Xvfb.

## Test counts

Command:

```bash
TMPDIR="$HOME/.cache/sirio-sdd/tmp" cargo nextest run -p sirio_theme -p ely-palette -p ely-gpui-component -p sirio_terminal -p sirio_ui -p sirio
```

Nextest reported 1453 passed and 1 skipped. Passed counts by crate:

| Crate | Passed |
|---|---:|
| `sirio_theme` | 41 |
| `ely-palette` | 5 |
| `ely-gpui-component` | 73 |
| `sirio_terminal` | 119 |
| `sirio_ui` | 779 |
| `sirio` | 436 |
