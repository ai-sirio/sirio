# Removing bezel's UI crates: the before/after captures

Every PR that moves a surface off bezel's UI crates onto Ely (or onto Sirio's own code) is judged
by screen captures taken before and after it, compared pixel by pixel. This page says how the
captures are made, how they are compared, what the mask table means and what each PR recorded.

The tools:

| File | What it is |
| --- | --- |
| `Scripts/bezel-off-captures.sh` | Captures the tree it runs in: the visual sweep, the surfaces probe, the Changes E2E and the Ely chat E2E, into one set of PNG frames. |
| `Scripts/capture-diff.py` | Compares two sets of frames (a pixel counts as changed when a channel differs by more than `--tolerance`, default 1 level) and writes `changed.tsv` and a static `index.html`. |
| `Scripts/capture-masks.tsv` | The regions a clock drives, painted black before two sets are compared. |
| `Scripts/Tests/test-surfaces-probe.sh` | Captures the frames the app's own E2E tests do not reach, from `sirio_ui`'s `surfaces_probe` example: the eight Settings pages, Git Hosting again with a forge source (`settings-git-hosting-accounts`), a tab context menu and a tooltip (`SURFACES PROBE OK (<n> frames, <g> gaps)`). `Scripts/bezel-off-captures.sh` runs it once per appearance. |
| `Scripts/Tests/test-capture-diff.sh` | The contract of `capture-diff.py` on synthetic images (`CAPTURE DIFF CONTRACT OK`). |
| `SIRIO_REDUCED_MOTION=1` | A debug-only switch (a no-op in release builds) that makes gpui snap every animation to its rest state. The capture script exports it. |

## Reproduce

Everything below runs with scratch under `$HOME/.cache/sirio-e2e-tmp` (the capture script
exports it; `/tmp` is a small tmpfs and control sockets must stay under 108 bytes). It needs
Xvfb, ImageMagick 7 (`magick`, `import`, `identify`), `xdotool`, `xwininfo`, Zig 0.15.2 on PATH and
Mesa's lavapipe at `/usr/share/vulkan/icd.d/lvp_icd.json` (`SIRIO_CAPTURE_VK_ICD=FILE` names another
ICD file, for a distro that installs it elsewhere, such as `lvp_icd.x86_64.json`).

```bash
# The base: a worktree of the commit the PR branches from. The script builds into that
# worktree's own rust/target (it unsets CARGO_TARGET_DIR), never the head's.
git worktree add ../bezel-off-base <base-commit>
(cd ../bezel-off-base && Scripts/bezel-off-captures.sh --out-dir ../bezel-off-captures/before --xvfb)

# The PR: the same script in the PR's tree, so the same build recipe and the same display size.
Scripts/bezel-off-captures.sh --out-dir ../bezel-off-captures/after --xvfb

# The diff: masks are always applied (the tolerance is 1 level unless --tolerance says otherwise);
# --allow is the PR's attribution file.
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

`capture-diff.py BEFORE AFTER --out DIR [--masks TSV] [--allow TSV] [--tolerance N]` pairs frames by their path
under BEFORE and AFTER without the `.png` (`changes-dark/local-split`) and prints one of:

| Line | Exit | Meaning |
| --- | --- | --- |
| `CAPTURE DIFF IDENTICAL` | 0 | No frame has a pixel more than the tolerance apart once the masks are painted over. |
| `CAPTURE DIFF OK (<n> changed, all attributed)` | 0 | Frames changed, and every changed pixel lies in an `--allow` rectangle. |
| `CAPTURE DIFF FAILED (...)` | 1 | A frame was added, removed or changed size, or a pixel changed (by more than the tolerance) outside every mask and allow rectangle (an `UNATTRIBUTED <frame>` line says which). |
| `capture-diff: ...` on stderr | 2 | The tool could not decide: a missing tree, a malformed table, a tolerance outside 0..255, no frame at all, a file that is not an image. |

`DIR/changed.tsv` has one row per listed frame, with the columns
`frame status pixels bbox unattributed_pixels rows`:

- `status` is `changed`, `added` (only in AFTER), `removed` (only in BEFORE) or `size` (both, at
  different sizes). An identical frame is not listed.
- `pixels` is the number of pixels more than the tolerance apart, after the masks. `bbox` is
  `x,y,w,h` of the union of those pixels. `unattributed_pixels` is how many of them lie outside
  every allow rectangle. `rows` lists the allow lines whose rectangle holds at least one changed
  pixel, joined with `;`. All four are `-` or empty for `added`, `removed` and `size`.
- Every count comes from a difference image, never from ImageMagick's `compare`: the script
  writes the largest per-channel difference of each pixel (R, G, B and alpha, in 8-bit levels, one
  byte per pixel) and a pixel is changed iff that byte is greater than the tolerance. `compare
  -fuzz` is a colour distance, which is not that rule, and on ImageMagick 7.1 (HDRI) its `-metric
  AE` figure is not a pixel count (a one-pixel change printed 0.6, a change of 1/255 in one channel
  0.0013 with exit 0). The pipeline is described at the top of `Scripts/capture-diff.py`.

`DIR/index.html` is a static page: the listed frames as before | after | diff, with the allowed
rectangles drawn in outline over each image. The listed frames' images are copied under `DIR`
(`before/`, `after/`, `diff/`) and the page reaches them by relative paths with no leading slash,
so `DIR` can be published as it is.

## The tolerance: `--tolerance N`

`--tolerance N` (levels on the 8-bit scale, an integer from 0 to 255, default 1) is the noise
floor. After the masks are painted, a pixel counts as changed iff the largest of its channels'
differences (R, G, B and alpha) between the two images is greater than N; the verdict, `pixels`,
`bbox`, `unattributed_pixels` and the `rows` are all counted that way. `--tolerance 0` compares
strictly.

Why the default is 1: Mesa's software Vulkan (lavapipe) rasterizes a few glyph pixels exactly one
level apart in one channel from one app process to the next. The pixel is the same in every frame
of one process and differs between processes, and the set of such pixels did not settle over five
runs (two of them found a pixel the earlier ones had not), so masking them one by one cannot end.

What it cannot hide: a difference of two levels or more in any channel (R, G, B or alpha) of any
pixel outside a mask, in any frame; every frame is read as sRGB first, so a frame that
ImageMagick stored as a one-channel grayscale PNG (it does for a neutral picture) is compared in
green and blue too. In practice a change of colour, shape or position that a person can see moves
some pixel that far. It does hide a real change
of exactly one level, so a PR that means to change a colour by one level must say so and be
checked with `--tolerance 0`.

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

## The surfaces probe

`Scripts/Tests/test-surfaces-probe.sh` (`--xvfb | --display :N`, `--out-dir DIR`, `--appearance
dark|light`, `--no-build`, `--no-software-vulkan`) captures one 1470x833 window per frame and
appearance, each a process of `rust/crates/sirio_ui/examples/surfaces_probe.rs`, to
`DIR/frames/<frame>-<appearance>.png`: 11 frames, so 22 first pages, and 12 more pages for the
three Settings pages that are longer than the window (34 frames; the set is 94). In the set they
are `surfaces-<appearance>/<frame>`. The frames:

| Frame | What it shows |
| --- | --- |
| `settings-ai-providers` (3 pages) | The four provider cards: signed in with an identity, not signed in, signed in, no local store; the OpenCode Go and Ollama Cloud cookie fields and accounts rows on the later pages. |
| `settings-agents` | Claude Code installed (green dot, Installed badge, meta line); Codex installed with an Update and no published checksum; OpenCode and Pi with Install; Pi's failed install (the error strip); Oh-My-Pi with no ACP server; the registry warning strip; the search field. |
| `settings-language-servers` (3 pages) | Absent with Install, installed by Sirio, declined (Offer again), a manual guide, a failed install, then the rest of the 21 languages. |
| `settings-git-hosting` | The empty state (no forge source). |
| `settings-git-hosting-accounts` | A source of three fixed, made-up hosts installed through `forge_source::set_source`: github.com on `gh` (Detect chosen), a self-hosted GitLab on a read-only pinned token (the GitHub and GitLab buttons, Detect, CLI and Token, the token field, Save token, Forget token), and a host that asks for its forge. |
| `settings-general` (3 pages) | About, an available update with its button, the toggles, the summarizer picker; Chat history and Performance with the steppers; Markdown preview (the PlantUML line and server field); the sirioctl card, Host, Agent Skill and Agent Hooks. |
| `settings-permissions` | Browser origin grants (the macOS permission rows draw on macOS only). |
| `settings-appearance`, `settings-editor` | The whole pages (they fit the window) as `SettingsSnapshot::default()` draws them, with the theme set to the frame's appearance. |
| `tab-menu` | `tab_bar::render_tab_context_menu`: enabled and disabled rows and separators. |
| `tooltip` | A 120x28 box at (200, 200) with `controls::tooltip("Probe tooltip")`, the pointer on it. |

Every Settings frame is seeded through the public builders only, so nothing of the machine
reaches a frame and no API exists for the probe; every probe runs with `PATH` unset (that is how
the Agents page reaches its warning strip). A state those builders cannot reach prints
`GAP <frame> <state>` and is counted, never faked: the Agents search with no match, the
summarizer popover open, and the macOS permission rows (on a non-macOS host).

Pages. After a Settings frame's first page the script scrolls the detail column with the wheel:
the pointer goes onto the blank margin left of the content, 9 notches turn (one notch is 78 px,
so a step is 702 px of the 792 px viewport and consecutive pages share 90 px), and the pointer
leaves the window. It keeps `<frame>-p2-<appearance>`, `-p3-`, ... until a step moves nothing (the
capture is pixel-identical to the last page and is dropped), so a page that fits takes no extra
frame. At most 8 pages a frame: a step after the eighth that still moves fails the script. The
script prints `PAGES: <frame>-<appearance> <n>` per frame (`SURFACES_MAX_PAGES=N` changes the cap of 8; a cap below a page count the frames really have fails with the cap's message). The pointer never rests on a row: a row
a pointer sat on stays drawn hovered until the pointer is seen to leave it, and a window that
maps with the pointer outside it is drawn with the row at its centre hovered until the pointer has
been in the window and left it, so the script does that before the first page too.

## Masks in force

`Scripts/capture-masks.tsv` was derived from runs of one build (94 frames each: 5 + 5 sweep,
4 + 4 Changes, 42 chat, 34 surfaces probe); two runs made after it was fitted found the cancelled line sized to the
digits two runs happened to differ in, the session dots, and the status bar path longer than
the mask (a `TMPDIR` change), and a later run found the scrollbar thumb; all four are in the table now. One
line per rectangle; the table says what each category is and what it blinds.

| Category | Frames | What moves | What the mask blinds |
| --- | --- | --- | --- |
| Usage meter | `sweep-*`, `changes-*`, `chat-ui-default/scenarios/*` | The status bar's left end reads the user's live account: percent used and the reset countdown (`4h 9m`), fetched over the network, so it differs per run and is sometimes absent. | The meter and its loading mark, x 76..376, y 802..824. |
| Fixture path | the same | The status bar's right end shows the branch and the fixture directory, whose name `mktemp` randomises (not a clock). The label is right-aligned, so its left edge follows the length of `TMPDIR` (found on the sixth run, when `visual-sweep.sh` began to honour `TMPDIR` and its label grew 114 px to the left). | The right label, from x 780 (room for the longest `TMPDIR` the script accepts, 60 characters). |
| Message time | the chat scenarios that show one (see the file) | The `HH:MM` wall clock between turns, centred at x 715. | 52 x 17 px at each time's row. |
| Cancelled-line time | `scenarios/cancel/frames/after-cancel` | `HH:MM · cancelled`, centred, whose width follows the digits. | The whole line, x 655..775. |
| Turn duration | `scenarios/staged-queue/frames/queue-drained` | `125 ms`, the elapsed time of a turn. | 50 x 17 px. |
| Crash addresses | `scenarios/death-recovery/frames/agent-died` | The dead fixture agent's crash report prints process addresses that ASLR moves on every start (not a clock). | 400 x 45 px of the error text. |
| Identity time | `chat-ui-default/identity/*` | The same `HH:MM` in the identity probe's own window (820x700). | 52 x 18 px. |
| Sidebar session dots | `chat-ui-default/scenarios/*` | The green connected indicators on the session's sidebar row show in some runs and not in others: a timing race of the fixture session, not a clock (seen in `activity` and `death-recovery/recovered`). | 32 x 26 px at the right end of the session's sidebar row. |
| Scrollbar thumb | `scenarios/long-history/frames/scrolled-up*` | After a turn completes the transcript scrollbar's thumb follows the transcript's estimated total height, so it sits about 7 px higher in some runs while the visible content is identical (a run-to-run accident, not a clock; found on held-out run 7). | The track column only: 10 x 540 px at x 1133, y 77. |
| Agents toolbar and subtitle | `surfaces-*/settings-agents-[dl]*` (the first page only) | The toolbar stamps `Refreshed HH:MM:SS` with the wall clock when the page is built, and Geist draws digits at different widths (`12:01:57` is 5 px narrower than `12:02:50`). The toolbar is right-aligned, so the stamp's width pushes the search field left (x 683..686 over 41 runs: 15 at random times and 26 more with `TZ` set so that every hour 00..23 was drawn) and narrows the header's left column, whose subtitle then wraps differently (3 of 15 runs). Nothing below y 170 or right of the stamp moved. | Two rectangles: the stamp and the search field, x 668..1022, y 54..94; and the subtitle block, x 475..687, y 88..170. The `Agents 5` title row, the `Update All` and `Refresh` buttons and everything below stay visible. |

No caret, spinner, orb or trace-timer mask was needed: with `SIRIO_REDUCED_MOTION=1` those draw
their rest state. A mask is for a clock or a run-to-run accident, sized to the whole element; a
difference of one level is not one (the tolerance takes it).

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
