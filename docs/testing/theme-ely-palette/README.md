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

Set `OUT` to a writable output directory outside the repository; the commands
below use it for scratch output and captures.

The reviewer measured all 70 before/after frame pairs on this run: every
difference was inside the 14-pixel status strip, with bounding box
`~1250x14+207+806`; the strip's colour histograms were identical, though pixel
counts changed. Repeat these commands for each same-named pair to reproduce
the measurement:

```bash
BEFORE="$OUT/before/notte-light/chat.png"
AFTER="$OUT/after/notte-light/chat.png"
magick "$BEFORE" "$AFTER" -compose difference -composite -threshold 0 -format '%@' info:
magick "$BEFORE" -crop 1250x14+207+806 +repage -format '%c' histogram:info:- | sort > "$OUT/before-strip.txt"
magick "$AFTER" -crop 1250x14+207+806 +repage -format '%c' histogram:info:- | sort > "$OUT/after-strip.txt"
diff -u "$OUT/before-strip.txt" "$OUT/after-strip.txt"
```

## Visual sweep

The seeded driver captures five states for each of seven base colours in dark
and light appearance: empty workspace, chat, terminal, split layout and file
tree. Its JSON string settings are decoded to the raw scalar values the app
stores in its SQLite settings table.

Build the baseline binary before the current binary so both captures use the
intended revisions:

```bash
OUT=/path/to/theme-sweeps
mkdir -p "$OUT"
export CARGO_TARGET_DIR="$OUT/cargo-target"
git worktree add ../theme-ely-baseline f1b578b3
(cd ../theme-ely-baseline/rust && cargo build -p sirio)
cp "$CARGO_TARGET_DIR/debug/sirio" "$OUT/sirio-baseline"
(cd rust && cargo build -p sirio -p sirio_control --bins)
mkdir -p rust/target/debug
cp "$CARGO_TARGET_DIR/debug/sirio" rust/target/debug/sirio
cp "$CARGO_TARGET_DIR/debug/sirioctl" rust/target/debug/sirioctl
```

Start any Xvfb on an unused display `:N` (for example, `:99`); these commands
do not use the operator's display or Wayland:

```bash
DISPLAY_NAME=:99
Xvfb "$DISPLAY_NAME" -screen 0 1920x1080x24 > "$OUT/xvfb.log" 2>&1 &
XVFB_PID=$!
unset DISPLAY WAYLAND_DISPLAY
Scripts/Tests/test-theme-sweep.sh --out-dir "$OUT/before" --bin "$OUT/sirio-baseline" --display "$DISPLAY_NAME" --settle 2
Scripts/Tests/test-theme-sweep.sh --out-dir "$OUT/after" --bin "$PWD/rust/target/debug/sirio" --display "$DISPLAY_NAME" --settle 2
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
mkdir -p "$OUT/tmp"
TMPDIR="$OUT/tmp" CARGO_TARGET_DIR="$OUT/cargo-target" cargo nextest run -p sirio_theme -p ely-palette -p ely-gpui-component -p sirio_terminal -p sirio_ui -p sirio
```

Nextest reported 1455 passed and 1 skipped. Passed counts by crate:

| Crate | Passed |
|---|---:|
| `sirio_theme` | 43 |
| `ely-palette` | 5 |
| `ely-gpui-component` | 73 |
| `sirio_terminal` | 119 |
| `sirio_ui` | 779 |
| `sirio` | 436 |
