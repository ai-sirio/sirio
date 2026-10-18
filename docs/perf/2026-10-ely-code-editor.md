# Ely code editor performance against main: 2026-10-18

Status: **measured; a constant 1.4 to 2.5 ms per typed or scrolled frame (median) remains against main and is accepted** (user, 2026-10-18; see "Known gap"). The cost that grew with the file is gone, on a degenerate file and on real code. Everything below is for the release build with the `perf-native` journal on one machine, in software rendering; compare trees, not absolutes.

This is Task 12b of the Ely code editor plan (`docs/superpowers/specs/2026-10-17-ely-code-editor-foundations`, spec 5.8). Round 1 found that the branch's frame cost grew with the file's length (typing at 1 MiB: 32.8 ms against main's 4.1); fix round 1 removed that; fix round 2 (this document's last measurements) removed what that round's benchmark file could not show (a climb over every line above the caret on any file indented under one header, a minimap that painted every token as its own placed quad) and measured on real code. The tables are the measurements of the final tree; the "Before and after this round" tables are the same harness on the tree this round started from.

The minimap captures are in [`2026-10-ely-code-editor-minimap/`](2026-10-ely-code-editor-minimap/). The summaries of every run (`2026-10-ely-code-editor-measurements.json`) and the scripts that took them (`2026-10-ely-code-editor-harness/`, named below as `harness/…`) sit beside this file on the machine that took them and are not in git (`.gitignore` keeps only reports and their captures). The raw per-run directories (`trace.tsv`, `native.jsonl`, `result.json`, window captures: about 3.5 GB for the 400 runs of the four directories) stay on the machine that took them, in `~/.cache/sirio-perf/runs-r2` (window B), `runs-r2e` (window E) and `runs-fix1` / `runs` (rounds 1 and fix 1), and are not in git.

## Machine and build

- Intel Core i7-8565U (4 cores, 8 threads) at 1.8 GHz, 20 GB, Linux 7.2.3-arch1-3 (Arch). Other sessions share the box: the load average at the start of each run is in `series-b.log` / `series-e.log` (1.4 to 5.7, mostly 2 to 3). Main's own median drifts by up to 0.8 ms between the two windows below (for example `type` on `real.rs`: 4.27 ms in window B, 5.06 ms in window E), so a difference under about 1 ms between two *windows* is noise; within a window the trees are interleaved per round.
- A private Xvfb `:95` (1920x1080x24) per measurement session, killed by its pid afterwards, with Mesa lavapipe (`VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json`), window 1470x833, `GPUI_X11_SCALE_FACTOR=1`. The present is a software present: `draw` (the frame's `draw_start..draw_end`) is the CPU's work building the frame; `draw->present` adds lavapipe's.
- Build profile: the workspace's `release` profile (no LTO), `cargo build --release -j 4 -p sirio --features sirio/perf-native --bin sirio -p sirio_control --bin sirioctl -p sirio_host --bin sirio-host`, rustc 1.98.1, Zig 0.15.2, `GHOSTTY_SOURCE_DIR` a copy of the pinned ghostty source (commit a887df42, the same for every tree), `TMPDIR=$HOME/.cache/sirio-6d-tmp`.
- Trees and binaries (`sha256` of the `sirio` binary):

| name | commit | what it is | sha256 |
|---|---|---|---|
| main | `f8c143f96ba72516b03d2aa9b26c3dbc0da6d772` | `main`, bezel editor, no minimap | `af9beab46c7fcff8f518d0f85b52219d44cb47f925d10d261a2b2d42d836c3a4` |
| base | `f623c687` | the tree this round started from (round 1 plus fix 1 plus the controller's script fix) | `ca35d0a1fd90bf8d89b0316f0eb59703892043ba467e2ae74f1842d85acb8b0d` |
| mid | `75283a14` | base plus the enclosing index, the minimap's pixel-row sampling and the range lookups (before the paint-layer change) | `3f78195bdfdaaae761c99ab75b64ee2c4ddc9fc88960cfb0e0c628d555ce54d9` |
| final | `534bd6b5` | mid plus `perf(ely): paint the minimap's tokens in one layer` | `22b1fa5a1682ae53c0c5bcadb96a78a2c3527eaa1242faec5bd477a6abd93aaa` |
| 57b9274d | `57b9274d` | round 1's tree, before any fix (its binary no longer exists; its numbers are round 1's, below, and are **missing** for real code) | - |

The branch's merge-base with main is `91b3cad0`; main carries one unrelated commit (`#637`, a sidebar label).

## Harness

Real input into the real binary. Each run starts a fresh isolated app (private `HOME`, XDG dirs, `SIRIO_DB`, `SIRIO_SOCKET`, `SIRIO_HOST_HOME`, stub agent CLIs first on `PATH`, `SIRIO_PERF_TRACE` and `SIRIO_PERF_NATIVE_TRACE` at fresh paths) on a fresh git repository holding the one fixture file, drives it with `xdotool`, and stops it and its host afterwards. `world-g.sh` builds that world, `runr2.sh TREE WORKLOAD TAG` is one run, `analyze.py` reads a run's `trace.tsv` (spans) and `native.jsonl` (GPUI's foreground journal: presented frames and input events) inside the run's epoch window, `summ-r2.py` and `build-doc-json.py` pool runs. A frame belongs to a window by its `draw_start`; a key's latency is its key-down start to the end of the next presented frame that starts after the event ends.

The input sequences (the same for every tree):

```sh
# open and focus (all workloads)
xdotool mousemove 1437 219 click --repeat 2 --delay 80 1; sleep 3   # double-click the file in the right panel's Files view
xdotool mousemove 1127 424 click 1; sleep 1                         # a click on the editor pane (focus; the minimap on the branch)
# type:      caret at line 1
xdotool key ctrl+Home; sleep 2
xdotool type --delay 50 "the quick brown fox jumps over the lazy dog while seven wizards box quickly and the five boxing wizards jump quickly over a very lazy dog again and again until two hundred chars were typed!!"
# typeend:   caret at the end of an indented line near the end of the file (the case that climbed on the base)
xdotool key ctrl+End Up Up End; sleep 2     # then the same xdotool type
# scroll:    one Page Down every 100 ms, 400 of them
xdotool key --delay 100 --repeat 400 Next
# flood:     the key-repeat rate a held Down gives, 785 x 50 presses
for i in $(seq 1 785); do xdotool key --delay 4 --repeat 50 Down; sleep 0.03; done
```

(`xdotool type` loses about 10 of the 200 characters on every tree alike; the journal sees 190 key downs.)

Every run reported `trace.dropped` 0 and journal `lost` 0, with a health row past the window's end (`flushed_past_end` in the JSON): nothing was dropped.

## Fixtures

Three files cut at a line boundary under `MAX_FILE_BYTES` (1,048,576), made by `2026-10-ely-code-editor-harness/gen-fixtures.py REPO_ROOT OUT_DIR` from the tree at `f623c687`, plus round 1's degenerate file:

| file | bytes | lines | what it is | sha256 |
|---|---|---|---|---|
| `real.rs` | 1,048,539 | 23,977 | every tracked `.rs` file of the repository, sorted by path, concatenated | `3c0c332911043c1094a3ba495b9a3e4a2303dd201ad46911b7c3b4862a6c6012` |
| `meta.json` | 1,048,570 | 36,371 | `cargo metadata --format-version 1 --offline` pretty-printed with `indent=2`; every line is indented under line 1's `{` | `ab85c001eff2d96443da4cec13d03400b554c21c97d5530e913654a67674b438` |
| `Big.java` | 1,048,546 | 43,243 | a synthetic single class of 4-space-indented methods (loops, ifs, brackets inside strings); every line is indented under `public class Big {` | `e01311a63c8ffa449cc9e58b02b8ef63140fcaf40ac19a3ef9f939cf1faafcd7` |
| `big-1mib.txt` ("degenerate", "txt") | 1,048,563 | 39,247 | round 1's fixture: `line N of the big file`, no brackets, no indentation | (generated by `world-g.sh`) |
| 8 KB and 100 KB | 8,000 / 100,000 | | the same text cut shorter | |

`meta.json` and `Big.java` are what a file indented under one header looks like (a class, a JSON document, a Rust `impl` or `mod tests`); `real.rs` is mostly top-level items with short bodies. The first benchmark file (the degenerate one) has no brackets and no indentation, so the bracket index, `enclosing`, `opens` and the indent guides never ran on it.

## Results: the final tree against main

Window E: main and final interleaved per round, 3 runs each (pooled frames), `type` 200 characters, `typeend` the same at the end of the file, `scroll` 400 Page Downs. Draw time is median / p95 in ms.

| workload | file | main | final (534bd6b5) | final - main (median) | key to present, main | key to present, final |
|---|---|---|---|---|---|---|
| type | real.rs | 5.06 / 6.12 | 6.53 / 9.09 | +1.47 | 44.5 / 58.1 | 54.6 / 73.3 |
| type | meta.json | 4.28 / 6.00 | 6.52 / 8.59 | +2.24 | 42.2 / 53.4 | 49.3 / 70.4 |
| type | Big.java | 4.75 / 7.85 | 6.82 / 9.16 | +2.07 | 42.5 / 54.9 | 52.0 / 72.1 |
| type | degenerate txt | 3.42 / 4.93 | 5.80 / 7.45 | +2.38 | 42.3 / 53.7 | 47.3 / 65.2 |
| typeend | real.rs | 6.30 / 9.46 | 7.70 / 9.54 | +1.41 | 45.3 / 59.0 | 59.5 / 74.7 |
| typeend | meta.json | 3.93 / 6.14 | 6.45 / 8.15 | +2.52 | 41.5 / 52.0 | 49.9 / 69.9 |
| typeend | Big.java | 5.12 / 7.99 | 6.90 / 9.30 | +1.78 | 43.7 / 57.4 | 52.3 / 71.2 |
| scroll | real.rs | 6.16 / 7.69 | 8.49 / 11.74 | +2.33 | 41.3 / 49.8 | 50.1 / 68.6 |
| scroll | meta.json | 4.65 / 5.66 | 6.77 / 8.82 | +2.13 | 39.0 / 48.3 | 46.6 / 57.6 |
| scroll | Big.java | 4.51 / 5.57 | 6.85 / 8.16 | +2.34 | 39.0 / 48.5 | 46.5 / 57.6 |
| scroll | degenerate txt | 4.03 / 4.93 | 6.11 / 7.49 | +2.08 | 38.6 / 48.1 | 44.9 / 55.4 |

Main's `typeend` on `real.rs` has its first run at 8.28 ms in this window (8.25 in window B) and 5.73 / 5.67 (6.78 / 6.90) in the other two, a first-run effect of unknown cause that lifts main's pooled median there; against runs 2 and 3 alone the gap in that cell is +2.0 ms, not +1.4.

Each tree's draw time is the same across file sizes, which is the point of this round. Typing at 8 KB / 100 KB / 1 MiB (degenerate, one run for the two smaller sizes, three for 1 MiB), draw median / p95: main 2.72 / 3.60, 2.89 / 3.64, 3.42 / 4.93; final 4.92 / 7.09, 5.98 / 8.02, 5.80 / 7.45. Round 1's growth (branch 6.07, 8.81, 32.8 ms) is gone; what is left is the constant.

The editor's own render spans on the final tree, median per run (main has none of these spans): `FileView.render` 0.043 to 0.055 ms and `CodeEditor.render` 0.04 to 0.06 ms scrolling, 0.15 to 0.6 ms typing (the bracket index's splice and the nesting index on a shape change), on every file; round 1's `CodeEditor.render` on the 1 MiB file was 25.5 ms. The rest of a frame is in GPUI's layout and paint of the rows and the minimap, which no `sirio_perf` span sees (`CodeEditor.visible_rows` totals about 1.6 ms a frame on `real.rs`, `CodeEditor.minimap_rows` about 0.8 ms).

## Before and after this round (window B)

Main, base, mid and (below) final, draw median / p95 ms. Window B has main, base and mid interleaved per round; final is in window E, so compare final to base through each window's main (the last column).

| workload | file | main (B) | base f623c687 | mid 75283a14 | main (E) | final | base - main(B) | final - main(E) |
|---|---|---|---|---|---|---|---|---|
| type | real.rs | 4.27 / 5.92 | 9.11 / 12.50 | 11.22 / 14.30 | 5.06 / 6.12 | 6.53 / 9.09 | +4.84 | +1.47 |
| type | meta.json | 4.45 / 6.09 | 7.03 / 8.66 | 6.78 / 9.06 | 4.28 / 6.00 | 6.52 / 8.59 | +2.58 | +2.24 |
| type | Big.java | 5.33 / 8.08 | 7.99 / 10.92 | 8.95 / 12.97 | 4.75 / 7.85 | 6.82 / 9.16 | +2.66 | +2.07 |
| typeend | real.rs | 7.20 / 9.38 | 10.83 / 14.23 | 10.41 / 12.99 | 6.30 / 9.46 | 7.70 / 9.54 | +3.63 | +1.41 |
| typeend | meta.json | 4.81 / 6.15 | 10.87 / 12.85 | 7.37 / 8.93 | 3.93 / 6.14 | 6.45 / 8.15 | +6.06 | +2.52 |
| typeend | Big.java | 5.73 / 7.96 | 12.02 / 14.62 | 7.95 / 11.22 | 5.12 / 7.99 | 6.90 / 9.30 | +6.29 | +1.78 |
| scroll | real.rs | 6.09 / 7.88 | 10.58 / 13.18 | 11.91 / 16.06 | 6.16 / 7.69 | 8.49 / 11.74 | +4.49 | +2.33 |
| scroll | meta.json | 4.64 / 5.75 | 7.79 / 9.57 | 7.59 / 9.25 | 4.65 / 5.66 | 6.77 / 8.82 | +3.15 | +2.13 |
| scroll | Big.java | 4.38 / 5.59 | 8.47 / 9.85 | 8.49 / 9.83 | 4.51 / 5.57 | 6.85 / 8.16 | +4.09 | +2.34 |

What each step of the round did, by the spans (median per run, ms):

- **`enclosing`'s climb** (`CodeEditor.render`, which timed the strong guide's climb as `CodeEditor.folds`, and `CodeEditor.sticky`): typing at the end of `meta.json` 4.35 / 4.28 / 3.51 before, 0.17 / 0.16 / 0.16 after (`Big.java` 4.38 / 4.56 / 4.29 before, 0.43 after); on `real.rs`, whose items are not nested under one header, 2.3 before, 0.4 after. A frame at the end of an indented 1 MiB file went from 10.9 / 12.0 to 7.4 / 8.0 ms in draw time (window B, mid against base; main 4.8 / 5.7). Scrolling `meta.json` also stops paying for the sticky header's climb (`CodeEditor.render` 0.47 to 0.06 ms).
- **The minimap, first version of the pixel-row sampling (mid)** made real code *slower* than base: `real.rs` scroll 10.58 to 11.91, type 9.11 to 11.22. The minimap paints one quad per coloured token (about 3,000 a frame on `real.rs`); the old stride sampling painted the same number of quads, so the count was not the difference. See "The minimap" below: its pixels are snapped, and the old sampling lost about a quarter of its rows to that, by accident; the new one drew a row at every pixel, and each `paint_quad` was costing a microsecond in GPUI's draw-order lookup.
- **Painting the tokens in one `paint_layer`** (final): the minimap's paint 3.4 ms to 0.31 ms a frame, `real.rs` scroll 11.9 to 8.5 ms in draw time, with the picture unchanged (pixel for pixel, dark and light: `compare -metric AE` 0 between mid and final).

## The Down flood

785 x `Down` x50 at about 175 keys a second for 205 s on the 1 MiB degenerate file; one run each:

| tree | key downs that reached the app | frames presented | draw median / p95 | key to present median / p95 |
|---|---|---|---|---|
| main | 39,250 | 1,550 | 3.26 / 3.72 | 96 / 228 |
| mid (75283a14) | 39,248 | 519 | 4.29 / 4.91 | 225 / 886 |
| final (534bd6b5) | 39,250 | 535 | 4.25 / 4.92 | 221 / 1,049 |

Round 1's numbers: the tree before any fix reached 4,450 key downs in the window and presented **0 frames** (it starved); after fix 1 the branch reached 39,250 and presented 556 frames. The branch keeps presenting and keeps up with the input, but it presents a third of the frames main does and each Down costs it more (a key takes about 2.3 times as long to reach a frame). This is part of the known gap; it was not investigated further (no profiler).

## Find and Select All

Find: Ctrl+F on `big-1mib.txt`, query `e` (the bar reads "1 of 117741"; every match is handed to the editor as a wash), then ` of the bi` one key per 500 ms. Key to present for the 11 keys, ms:

| tree | run | keys |
|---|---|---|
| base | 1 | 80 72 34 39 41 33 37 34 43 40 33 |
| base | 2 | 42 40 41 42 38 33 44 37 36 42 38 |
| base | 3 | 73 71 32 39 37 46 42 37 41 39 37 |
| mid | 1 | 74 38 32 39 37 38 38 39 31 38 41 |
| mid | 2 | 77 73 37 31 44 37 44 38 40 33 39 |
| mid | 3 | 72 37 37 35 40 39 36 39 39 33 37 |
| final | 1 | 79 70 39 39 37 32 39 44 34 41 36 |
| final | 2 | 69 36 40 38 34 41 40 40 46 43 37 |
| final | 3 | 70 89 40 38 40 37 31 37 42 40 42 |

Frame draw median during the find windows: base 4.0 to 4.4 ms, mid 4.1 to 4.3, final 4.1 to 4.4. **The range lookup (`perf(ely): look up a row's washes and diagnostics by range`) measured no change on this workload**: a find keystroke is a frame period (about 35 ms on this machine) plus the recomputation of the matches, and the first one or two keys (117,741, then tens of thousands of washes) cost about 70 to 80 ms on all three trees. It is kept because it is correct, tested against a scan of every range and cheap, but its effect is not visible here. On `real.rs` (find `e`): base 82 83 79 75 40 83 79 80 41 45 43; mid 91 42 82 78 43 47 89 77 42 45 43; final 83 40 86 79 41 48 82 79 74 37 41.

Select All (`big-1mib.txt`; Ctrl+A, then 40 x Shift+Down with the selection live, against 40 x Down), draw median / p95 ms and frames: idle with the selection, 0 frames in 3 s on every tree (no blink frames); plain Down x40: base 4.20 / 5.40 (48 frames), mid 4.11 / 5.09 (45), final 4.15 / 4.39 (45); Shift+Down with the whole file selected: base 4.19 / 4.72, mid 4.28 / 5.20, final 4.23 / 4.87 (40 frames each). A selection of the whole file adds nothing (round 1's finding, kept).

## Git marks

The four git commands behind `refresh_git_diff` (status, diff of the file, symbolic-ref, for-each-ref), 30 repetitions timed from outside (`harness/gittime.py`): the E2E repository with two modified files median 7.5 ms (p95 7.7, max 10.6); the repository with the 1 MiB file and one changed line median 3.8 ms (p95 4.0, max 6.1). They run off the GPUI thread; this round changed nothing about them (round 1: 8.6 and 4.5 ms).

## The minimap

**What it draws.** The minimap exists on the branch only: main's editor has no minimap (see `2026-10-ely-code-editor-minimap/minimap-main-dark.png`: the pane ends in a plain scrollbar), so every frame of the branch pays for a feature main does not draw. A file taller than the minimap's height in rows (more than about 650 lines here) is sampled; shorter files are drawn row for row, as before.

**Sampling.** The old code stepped through the rows `ceil(1 / tall)` apart (rows 0, 37, 74, ... on `real.rs`). The new code draws one row per pixel, the row at `floor(y / tall)` (`layout::MinimapPlan::samples`). A unit test (written first, red against the old sampling; `editor::layout::tests::the_minimap_samples_one_row_a_pixel_and_one_more_line_moves_no_sample_far`) proves that adding one line to a file of 20,000 to 30,000 rows moves no pixel's sampled row by more than one, where the stride sampling moved every sample whenever the stride changed.

**Captures** (`2026-10-ely-code-editor-minimap/`, `real.rs`, 1470x833 window, the minimap's top 180 rows of pixels at 4x, left: before, middle: after an Enter on line 6 (`Down` x5, `Return`), right: the pixels that differ, in red): `minimap-base-{dark,light}.png` (the tree this round started from), `minimap-final-{dark,light}.png`, and `minimap-main-dark.png` (the same file on main: no minimap).

What the captures show, read from them and from `compare`:

- Before and after the Enter, the final tree's minimap keeps its bands (the same blocks of long and short lines at the same heights) but nearly every sampled row changes: each pixel now shows the line before the one it showed, since the whole file moved down by one line and a pixel samples a fixed row. Whole-crop differing pixels (100x660, the minimap only): final 2,750 (dark) / 2,603 (light) of 66,000 (4.2% / 3.9%), base 3,361 / 3,151 (5.1% / 4.8%). So the pixel-row sampling is *a little* steadier than the stride sampling, not steady: a one-line shift re-picks a different (neighbouring) line at every pixel for any sampling at a fixed lattice. What the test proves is the weaker thing it says: no pixel's row moves by more than one line. A sampling that did not move under a shift would have to choose lines by their content (a hash of the text), which gives an uneven density; that was not tried.
- The final minimap is denser than the base's: in the dark capture 14,916 pixels of a crop differ from the background in the final tree against 10,673 in the base. The base's sampling drew a row every 1.0098 pixels and painted a 0.71 pixel high quad; GPUI snaps a quad's corners to device pixels (`bezel-gpui` `snap_bounds`), so a quad whose top falls at .5 to .8 of a pixel snaps to a box of no height and is not painted. About a quarter of the base's rows were never painted, by accident, and the new sampling, which places a row at every pixel, paints them all.
- The paint layer changes no pixel (mid against final: 0 differing pixels in dark and in light).

**What a minimap frame costs, on `real.rs`** (a `CodeEditor.minimap_paint` span in diagnostic builds of the mid tree; scroll, one or two runs each; draw median over the run):

| variant | `minimap_paint` median ms | draw median ms |
|---|---|---|
| stride sampling (the base's), y = row x tall | 2.24, 2.26, 2.27 | 10.26, 10.35, 10.01 |
| pixel-row sampling, y at the pixel (mid) | 3.40, 3.37 | 11.86, 11.90 |
| pixel-row, y = row x tall | 3.46 | 12.23 |
| pixel-row, y = pixel + 0.5 / + 0.3 | 3.39 / 3.39 | 12.02 / 11.97 |
| pixel-row, y = 1.02 x pixel (rows more than a pixel apart) | 2.33 | 10.64 |
| stride rows at y = 1 px apart | 3.22 | 11.71 |
| pixel-row in one `paint_layer` (final) | 0.31, 0.31 | 8.61, 8.42 |

The quad counts are the same (2,945 tokens drawn with the new sampling, 2,909 with the stride one), so the 1.1 ms is not the number of quads: it is the draw-order lookup `Scene::insert_primitive` does for every quad that is not inside a layer, and it is cheaper only for quads that snap to nothing. Inside one `Window::paint_layer` the minimap's tokens share a draw order and skip the lookup.

Where the rest of the `real.rs` gap is, by Editor settings (one run each, scroll 400 Page Downs and type 200, draw median / p95 ms, final tree): minimap on 8.12 / 14.72 (scroll) and 6.67 / 9.38 (type); minimap off 7.21 / 9.45 and 5.40 / 7.83; minimap, sticky scroll, indent guides and rainbow brackets all off 6.88 / 8.92 and 5.15 / 8.29; main 6.16 / 7.69 and 5.06 / 6.12 (window E). The minimap accounts for about 0.9 ms of the 2.3 ms scroll gap and 1.3 ms of the 1.5 ms typing gap on real code; with every optional feature off the branch is within 0.7 ms of main on scroll and 0.1 ms on typing. On the mid tree the same bisect (scroll, `real.rs`) read: default 11.33, minimap off 7.14, sticky off 11.91, guides off 11.85, rainbow off 12.04, all off 6.47 (base: minimap off 7.02, sticky off 10.89, rainbow off 10.56).

## Known gap

A constant 1.4 to 2.5 ms per typed or scrolled frame against main (median; up to 4 ms at p95 when scrolling `real.rs`), independent of the file's size, and about three times the per-key cost during a Down flood. **Accepted by the user on 2026-10-18.**

- What is known: the minimap, a feature main does not have, accounts for 0.9 to 1.3 ms of it on real code (above); the rest is in GPUI's layout and paint of the editor's rows, which no `sirio_perf` span covers (the editor's own spans total under 2 ms a frame, `FileView.render` 0.05 ms, `CodeEditor.render` under 0.6 ms). Ely builds each row from several elements (gutter cell, guides, a text canvas, carets); bezel's editor was not read to compare.
- What is not known: which of those it is. No profiler was available: `perf` and `samply` are not installed and `ptrace` is refused (`yama ptrace_scope=1`), so no sampling profile of a frame was taken. The explanation above is a hypothesis from the bisect by settings, not a measurement.
- Not tried: caching the minimap's quads between frames when the text does not change (it is the one part of a frame that does not change while scrolling), choosing minimap lines by content to make its picture steady under edits, and any work on Ely's per-row element count.
- Other costs that remain by design, noted rather than changed: `Rows::Listed`'s slow path while a fold is set or a diff hunk is open (about 0.2 ms at 1 MiB); more than 32 edits between two reads of the bracket index re-lex the whole file once per keystroke; `Buffer::replace` recomputes its line starts and `snapshot()` clones the text on every edit (both pre-existing); the nesting index and the bracket index are rebuilt in one pass over the lines / brackets when an edit moves a line's shape, which is the file's line count (about 0.4 ms at 24,000 lines) on an Enter or a change of indentation, and nothing on a keystroke inside a line.

## Rounds 1 and fix 1 (degenerate file only; `57b9274d` on real code is missing)

From the Task 12b report, release + `perf-native`, 200 characters typed and Page Down scrolling (1200 presses on main, 1800 on the branch, to the end of the file), 3 runs per tree, draw median / p95 ms, 1 MiB degenerate file:

| workload | main | `57b9274d` (before any fix) | after fix 1 (`3b445a56`) |
|---|---|---|---|
| type 200 | 4.13 / 5.78 | 32.79 / 38.15 | 5.01 / 7.41 |
| scroll | 4.00 / 4.87 | 36.85 / 41.71 | 5.99 / 7.52 |

At 8,000 / 100,000 / 1,048,563 bytes, typing: main 2.27 / 2.52 / 4.1, `57b9274d` 6.07 / 8.81 / 32.8. Round 1's `CodeEditor.render` span on the 1 MiB file: 25.49 ms (minimap 15.80, brackets 6.39, occurrences 1.82, folds 1.09, rows 0.20), 0.04 ms after fix 1. Those rounds' run directories are `~/.cache/sirio-perf/runs` and `runs-fix1`.
