# Removing bezel's UI crates: the before/after captures

Every PR that moves a surface off bezel's UI crates onto Ely (or onto Sirio's own code) is judged
by screen captures taken before and after it, compared pixel by pixel. This page says how the
captures are made, how they are compared, what the mask table means and what each PR recorded.

The tools:

| File | What it is |
| --- | --- |
| `Scripts/bezel-off-captures.sh` | Captures the tree it runs in: the visual sweep, the surfaces probe, the Changes E2E and the Ely chat E2E, into one set of PNG frames. |
| `Scripts/capture-diff.py` | Compares two sets of frames and writes `changed.tsv` and a static `index.html`. |
| `Scripts/capture-masks.tsv` | The regions a clock drives, painted black before two sets are compared. |
| `Scripts/Tests/test-capture-diff.sh` | The contract of `capture-diff.py` on synthetic images (`CAPTURE DIFF CONTRACT OK`). |
| `SIRIO_REDUCED_MOTION=1` | A debug-only switch (compiled out of release builds) that makes gpui snap every animation to its rest state. The capture script exports it. |

## Reproduce

Everything below runs with scratch under `$HOME/.cache/sirio-e2e-tmp` (the capture script
exports it; `/tmp` is a small tmpfs and control sockets must stay under 108 bytes). It needs
Xvfb, ImageMagick 7 (`magick`, `import`, `identify`), `xdotool`, `xwininfo`, Zig 0.15.2 on PATH and
Mesa's lavapipe at `/usr/share/vulkan/icd.d/lvp_icd.json`.

```bash
# The base: a worktree of the commit the PR branches from. The script builds into that
# worktree's own rust/target (it unsets CARGO_TARGET_DIR), never the head's.
git worktree add ../bezel-off-base <base-commit>
(cd ../bezel-off-base && Scripts/bezel-off-captures.sh --out-dir ../bezel-off-captures/before --xvfb)

# The PR: the same script in the PR's tree, so the same build recipe and the same display size.
Scripts/bezel-off-captures.sh --out-dir ../bezel-off-captures/after --xvfb

# The diff: masks are always applied; --allow is the PR's attribution file.
Scripts/capture-diff.py ../bezel-off-captures/before/frames ../bezel-off-captures/after/frames \
  --out ../bezel-off-captures/diff --masks Scripts/capture-masks.tsv [--allow ATTRIBUTION.tsv]
```

`bezel-off-captures.sh` prints `CAPTURES OK: DIR (<n> frames)`. Its `DIR/frames/<source>-<appearance>/`
is what `capture-diff.py` reads; `DIR/runs/<source>/` keeps each source's own transcript, app log
and every frame it took. `--display :N` replaces `--xvfb` when a display already runs; use the
same geometry for the base and the PR, because the window size is part of every frame.

What the script does to make two runs of one build identical:

- `SIRIO_REDUCED_MOTION=1`: a debug build calls `cx.set_reduce_motion(true)` before `Theme::init`,
  so every `with_animation` element (spinners, the thinking orb, fades) draws its rest state.
- One renderer: Mesa's software Vulkan only (`VK_ICD_FILENAMES` and `VK_DRIVER_FILES`).
- One build: it builds `sirio`, `sirioctl` and the chat E2E's seed and probe first, and unsets
  `CARGO_TARGET_DIR`, so `visual-sweep.sh` (which has `rust/target` fixed) and the other sources
  drive the same binary.
- One display geometry with `--xvfb` (1600x1000x24).

What it cannot remove, a mask hides or the script drops by name.

## Reading the diff

`capture-diff.py BEFORE AFTER --out DIR [--masks TSV] [--allow TSV]` pairs frames by their path
under BEFORE and AFTER without the `.png` (`changes-dark/local-split`) and prints one of:

| Line | Exit | Meaning |
| --- | --- | --- |
| `CAPTURE DIFF IDENTICAL` | 0 | No frame differs once the masks are painted over. |
| `CAPTURE DIFF OK (<n> changed, all attributed)` | 0 | Frames changed, and every changed pixel lies in an `--allow` rectangle. |
| `CAPTURE DIFF FAILED (...)` | 1 | A frame was added, removed or changed size, or a pixel changed outside every mask and allow rectangle (an `UNATTRIBUTED <frame>` line says which). |
| `capture-diff: ...` on stderr | 2 | The tool could not decide: a missing tree, a malformed table, no frame at all, a file that is not an image. |

`DIR/changed.tsv` has one row per listed frame, with the columns
`frame status pixels bbox unattributed_pixels rows`:

- `status` is `changed`, `added` (only in AFTER), `removed` (only in BEFORE) or `size` (both, at
  different sizes). An identical frame is not listed.
- `pixels` is the number of pixels that differ at all (fuzz 0), after the masks. `bbox` is
  `x,y,w,h` of the union of those pixels. `unattributed_pixels` is how many of them lie outside
  every allow rectangle. `rows` lists the allow lines whose rectangle holds at least one changed
  pixel, joined with `;`. All four are `-` or empty for `added`, `removed` and `size`.
- A pixel is counted from the mask ImageMagick's `compare` draws, never from its `-metric AE`
  figure: on ImageMagick 7.1 (HDRI) that figure is not a pixel count (a one-pixel change printed
  0.6, and a change of 1/255 in one channel printed 0.0013 with exit 0).

`DIR/index.html` is a static page: the listed frames as before | after | diff, with the allowed
rectangles drawn in outline over each image. The listed frames' images are copied under `DIR`
(`before/`, `after/`, `diff/`) and the page reaches them by relative paths with no leading slash,
so `DIR` can be published as it is.

## The mask table: `Scripts/capture-masks.tsv`

One rectangle per line, tab separated: `frame_glob  x  y  w  h  why`. `frame_glob` is matched
(fnmatch, so `*` also crosses `/`) against the frame name, with or without `.png`; `x,y` is the
top left corner and `w,h` the size, in pixels of the frame. A line that is blank or starts with
`#` is skipped.

A mask is a region that a clock moves and a deterministic build cannot stop: a text caret, a
spinner, the thinking orb, a trace timer, a relative time ("2m ago"), a token count. Both images
are painted black there before they are compared, so a difference inside it does not exist. The
`why` names the clock. A mask is sized to the whole element the clock drives (the caret cell, the
timestamp's text box), not to the pixels two runs happened to differ in: two runs undersample a
blink.

A mask is never a way to make a real difference go away. Where a PR changes pixels that a mask
covers, the frame is blind there, so a PR that touches one of these elements says so in its
section below and checks it by eye.

A frame that cannot be made deterministic with masks is dropped by name in the `DROPPED` list in
`Scripts/bezel-off-captures.sh`, and the script prints `NOTE dropped <frame>: <why>` for each.

## The allow table: a PR's attribution file

The same format, passed as `--allow`. Its `why` is the first cell of the PR's inventory row that
explains the change (for example `ui::tooltip::Tooltip`). A changed pixel inside an allow
rectangle is attributed to that row; a changed pixel outside every allow and mask rectangle makes
the diff fail. `changed.tsv` shows which rows each frame used and `index.html` outlines them, so a
reviewer sees the rectangle and the change together.

## Masks in force

`Scripts/capture-masks.tsv` was derived from three runs of one build (60 frames each: 5 + 5
sweep, 4 + 4 Changes, 42 chat) and checked against a fourth that had no mask added for it (it
found the cancelled line sized to the digits two runs happened to differ in, one more noise pixel
and the session dots; all three are in the table now). One line per rectangle; the table says
what each category is and what it blinds.

| Category | Frames | What moves | What the mask blinds |
| --- | --- | --- | --- |
| Usage meter | `sweep-*`, `changes-*`, `chat-ui-default/scenarios/*` | The status bar's left end reads the user's live account: percent used and the reset countdown (`4h 9m`), fetched over the network, so it differs per run and is sometimes absent. | The meter and its loading mark, x 76..376, y 802..824. |
| Fixture path | the same | The status bar's right end shows the branch and the fixture directory, whose name `mktemp` randomises (not a clock). | The right label, from x 955 (chat), 1040 (Changes) or 1140 (sweep). |
| Message time | the chat scenarios that show one (see the file) | The `HH:MM` wall clock between turns, centred at x 715. | 52 x 17 px at each time's row. |
| Cancelled-line time | `scenarios/cancel/frames/after-cancel` | `HH:MM · cancelled`, centred, whose width follows the digits. | The whole line, x 655..775. |
| Turn duration | `scenarios/staged-queue/frames/queue-drained` | `125 ms`, the elapsed time of a turn. | 50 x 17 px. |
| Crash addresses | `scenarios/death-recovery/frames/agent-died` | The dead fixture agent's crash report prints process addresses that ASLR moves on every start (not a clock). | 400 x 45 px of the error text. |
| Identity time | `chat-ui-default/identity/*` | The same `HH:MM` in the identity probe's own window (820x700). | 52 x 18 px. |
| Sidebar session dots | `chat-ui-default/scenarios/*` | The green connected indicators on the session's sidebar row show in some runs and not in others: a timing race of the fixture session, not a clock (seen in `activity` and `death-recovery/recovered`). | 32 x 26 px at the right end of the session's sidebar row. |
| Glyph noise | `changes-*/error`, `changes-*/local-*`, `scenarios/activity`, `scenarios/effort-full*` | One glyph pixel renders 1/255 apart in one channel between app processes: the same in every frame of one process, different between processes (text rasterization, not a clock). | Exactly the observed pixels, 1x1 (1x2 at 855,95). |

No caret, spinner, orb or trace-timer mask was needed: with `SIRIO_REDUCED_MOTION=1` those draw
their rest state. The glyph-noise set has not settled: the third run found a pixel the first two
had not, so a later run can show another one. A new one is a line to add to the table (one pixel,
this `why`), not a change to look for, and `Scripts/capture-diff.py` stays at fuzz 0 on purpose.

## Per PR

Each section is filled in by the last task of its PR: the frames that changed, the attribution
file, and the reviewer's verdict on the page.

### PR A: names, tooling and behaviour checks (`refactor/bezel-off-a`)

To be filled in by Task 4.

### PR B: the pieces Ely lacks, rebuilt in Sirio (`refactor/bezel-off-b`)

To be filled in by Task 7.

### PR C: text fields and tooltips on Ely (`refactor/bezel-off-c`)

To be filled in by Task 10.

### PR D: menus, popovers, scrollbars and icons on Ely (`refactor/bezel-off-d`)

To be filled in by Task 12.

### PR E: Settings, then the crates out (`refactor/bezel-off-e`)

To be filled in by Task 14.
