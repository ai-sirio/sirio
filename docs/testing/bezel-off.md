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
| `Scripts/Tests/test-scroll-drag-probe.sh` | Drags the thumb of each of the three scrollbars the app draws, in the `scroll_drag_probe` example, with a real pointer, and checks it follows (`SCROLL DRAG PROBE OK (3 bars)`). |
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

## Behaviour checks

Frames say what a surface looks like; these say what it does. Each passes on the tree before the
PRs that move a surface off bezel, and a PR that touches the behaviour must still pass it. The
checks written for this page (the thumb drag, `composer_keys`, `reduced_motion`, `unfocused_spinner`
and the five gpui tests of Enter, the model search and Ctrl+C) were each shown to fail by breaking the behaviour they
name, so a green run of them means something; the others are existing checks, named here so the
PRs know which ones gate them, and were not mutated for it.

| Behaviour | Check |
| --- | --- |
| Dragging a scrollbar thumb: the thumb follows the pointer, and a long drag stops at the track's end (file view, transcript list, horizontal bar) | `Scripts/Tests/test-scroll-drag-probe.sh --xvfb` |
| The transcript follows its tail, a scrolled-back view does not jump when a turn arrives | `Scripts/Tests/test-ely-chat-ui-e2e.py`, scenario `long_history` |
| The composer's keys as X delivers them: Return sends, Shift+Return is a newline, Up and Down move the caret with no popup open, the slash popup takes Down and Tab and closes on Escape | the same script, scenario `composer_keys` (fixture `composer`: `plain` advertises no slash commands) |
| Reduced motion holds the thinking orb and the transcript still | the same script, scenario `reduced_motion` |
| A running turn's sidebar status bloom keeps turning while another window holds the X focus (bezel's pause-when-inactive stays off), and what that costs in CPU | the same script, scenario `unfocused_spinner`, with `--cpu-ceiling` |
| Input methods: Enter while a completion or a composition is open | `cargo nextest run -p sirio_ui ely_composer_enter_respects_completion_and_ime` |
| Paste into the composer (an image attaches and is sent; the right-click menu's Cut, Copy and Paste) | `pasting_an_image_into_the_composer_attaches_it_and_sends_it_to_the_agent`, `composer_right_click_menu_offers_cut_copy_paste_on_the_field` |
| The composer's keys in gpui: Enter, Shift+Enter, the model search, the `@` popup | `enter_sends_and_shift_return_inserts_a_newline`, `model_search_is_a_text_field_that_never_touches_the_draft`, `at_mention_popup_lists_files_and_inserts_a_mention_token` |
| Enter in a text field does its form's job: the clone URL, the new project's name, the address bar | `pressing_enter_in_the_clone_url_field_starts_the_clone`, `pressing_enter_in_the_create_name_field_creates_the_folder`, `pressing_enter_in_the_address_bar_navigates` |
| The model search keeps answering Down and Enter while its field has the focus | `the_model_search_answers_down_and_enter_while_its_field_has_focus` |
| Ctrl+C (Cmd+C on macOS) in the composer copies the field's selection, not the transcript's | `ctrl_c_in_the_composer_copies_its_selection_and_not_the_transcript` |
| Ctrl+C elsewhere: copies a selection, is never swallowed, and a terminal's stays SIGINT | `Scripts/Tests/test-text-selection-e2e.sh` |
| The terminal's scrollbar follows the viewport, and dragging it scrolls the terminal | `cargo nextest run -p sirio_terminal the_scrollbar_follows_the_viewport_and_dragging_it_scrolls_the_terminal` (needs Zig 0.15.2) |
| A live thought opens while streaming, folds when it settles, and obeys a press | `a_live_thought_opens_while_streaming_folds_on_settle_and_obeys_a_press` |

The gpui tests are the ones that run the real keymap in a drawn frame, which is why IME and paste
stay gpui tests: Xvfb here has no input method and no clipboard tool. The five new tests also
install Ely's bindings (`chat::init` and `ely::init`), so a replacement field that binds the
same keys is caught by them when it lands.

### The thumb drag

`test-scroll-drag-probe.sh` finds each thumb in the capture as the run of pixels in the bar's
centre line that differ from that line's most common colour, so it does not depend on what draws
the bar. A 900x600 window of three 300x600 panes (`sirio_ui::probes::scroll_drag_view`); the
pointer presses a thumb's centre, moves 3 px (past gpui's 2 px threshold, where the drag
starts) then 1 px (the move whose event is the first drag event, where every bar anchors its
grab), then 120 px in six steps. The thumb must have moved as far as the pointer did after its
second move, within 4 px, and a second drag of +2000 px (the pointer stops at the screen's edge,
past the track's end) must leave it flush with the track's end.

### The unfocused spinner and its CPU base

`unfocused_spinner` opens a turn, gives a second window (`surfaces_probe --frame tooltip`) the X
focus, raises Sirio's window back on top without handing the focus back (it asserts
`xdotool getwindowfocus` is the second window before and after the raise), and takes three frames
250 ms apart. At least one pair must differ in the sidebar row's status bloom.

It judges the bloom and not the transcript on purpose. The transcript's thinking row carries Ely's
`ShimmerText`, a gpui animation that asks for its own frames and never reads the pause, and every
repaint of the chat view samples the orb's live phase, so the transcript moves whether or not
bezel's `set_pause_when_inactive(false)` is in force. With the pause turned on the transcript still
differed in three of three runs: that check could not fail. The row's bloom is drawn through bezel's
clock alone (a `Painter` lease taken by the row's own cached view, and gpui replays a cached view
until it is notified), and leases are refused while no window of the app is active and the pause is
on, so it freezes exactly then. The tab strip's working mark is not such a region: it keeps moving
with the pause on. Shown red by changing `main.rs`'s `cx.set_pause_when_inactive(false)` to `true`
(`sirio` rebuilt): the bloom's region was identical in all three frames in 5 of 5 runs, and 40 to
141 differing pixels per pair with it off in every run.

The rectangle (`SIDEBAR_BLOOM`, in pixels of the default window) has a control on the failure path
only: when the three crops are identical the scenario gives Sirio the focus back and looks at the
same crop again. If it moves, the failure says the bloom froze while unfocused, so the pause is on;
if it still does not, the failure says `SIDEBAR_BLOOM <rect> holds nothing that moves even with Sirio
focused; the row moved, update the rectangle`. A PR that restyles the sidebar row or its status
bloom re-runs the `set_pause_when_inactive(true)` mutation too, to be sure the new rectangle still
discriminates.

The scenario prints `unfocused-spinner CPU <x>% of one core over 10 s (load average ...)`: the
utime plus stime of Sirio's process while the second window holds the focus. Mesa's software
Vulkan renders in that process's own threads, so it reads above 100%. A PR that moves the spinner
or the redraw path passes `--cpu-ceiling` and fails above it.

**Base: 329.7% of one core**, load average 4.09 4.47 4.92 when the sample ended (this machine has
8 cores, and the run itself is most of that load), taken with
`Scripts/Tests/test-ely-chat-ui-e2e.py --display :N --only composer_keys,reduced_motion,unfocused_spinner,long_history`
on the tree of the commit that adds the scenario (Sirio 0.37.0, parent `fb6f0e79`). Five other runs with the pause
off read 320.2% to 325.6% at load averages of 4.9 to 6.4, and three with it on 321.3% to 330.0% at
7.1 to 8.5: the figure does not depend on the pause. **The ceiling a later PR passes is
`base × 1.25 + 1` = 413.1%** (`--cpu-ceiling 413.1`; a ceiling of 100 fails the run, a ceiling of
413.1 passes it). The figure includes the thinking row's shimmer, which animates whatever the pause says,
so it is a rendering-cost base and not a measure of the pause.

## Per PR

Each section is filled in by the last task of its PR: the frames that changed, the attribution
file, and the reviewer's verdict on the page.

### PR A: names, tooling and behaviour checks (`refactor/bezel-off-a`)

PR A changes no pixel: it adds this page's tooling and checks, reaches the three kept bezel crates
(`bezel-theme`, `bezel-markdown`, `bezel-syntax`) by their own names (`bezel_theme::`,
`bezel_markdown::`, `bezel_syntax::` in place of `bezel::theme::`, `markdown::`, `syntax::`), and
drops `bezel-editor`, whose `init` only bound keys in two contexts (`BezelEditor`,
`BezelImagePrompt`) that nothing in Sirio enters. `bezel::ui` and `bezel::motion` stay until PRs B to E.

**Base and head.** The "before" set was captured at `08e20455`, the head of this PR's third task,
not at PR A's git base `3c220dd3` (the `feat/effort-list` head). `3c220dd3` has no capture tooling and
no `SIRIO_REDUCED_MOTION` switch, so it cannot produce a set that is comparable with the "after" one.
The head is `08e20455` plus the rename commit. What lies between the git base and `08e20455`, from
`git diff --stat 3c220dd3..08e20455 -- rust/` (9 files, +1226 -1), so that a reader can judge that the
git base would have drawn the same frames:

| Change | Where |
| --- | --- |
| The debug-only switch `SIRIO_REDUCED_MOTION`: `cx.set_reduce_motion(true)` before `ely::init`, behind `cfg!(debug_assertions)` and the env var | `sirio/src/main.rs` (+6) |
| `controls::tooltip`, which no app surface calls yet | `sirio_ui/src/controls.rs` (+10) |
| `#[doc(hidden)] pub mod probes` and its file, the views the example probes draw | `sirio_ui/src/lib.rs` (+3), `sirio_ui/src/probes.rs` (+200) |
| `mod list_scroll` becomes `pub(crate) mod list_scroll` (a visibility change only) | `sirio_ui/src/chat/mod.rs` (1 line) |
| Two new examples that only the capture tooling runs | `examples/scroll_drag_probe.rs` (+67), `examples/surfaces_probe.rs` (+651) |
| Tests: the model search and Ctrl+C in the composer, Enter in the address bar, and the project forms' Enter and clone tests | `chat/mod.rs` (+105), `browser.rs` (+67), `project_forms.rs` (+116) |

None of them changes what the app draws without `SIRIO_REDUCED_MOTION` (the tooltip is called by the
probe only), so the app at `3c220dd3` and at `08e20455` draws the same frames.

**What PR A changes in product code.** Path roots and manifests only: 64 `bezel::theme::` to
`bezel_theme::` (30 files), 216 `markdown::` to `bezel_markdown::` (9 files), 8 `syntax::` to
`bezel_syntax::` (4 files), 2 `theme::` to `bezel_theme::` in `sirio_syntax`, one intra-doc link
`` [`bezel::theme`] `` by hand, and the 12 `::editor::init` calls (the one in `file_view::init` and
eleven in its tests). `rust/Cargo.lock` loses `bezel-editor` and gains three `bezel-theme` edges (`sirio`,
`sirio_terminal`, `sirio_ui`); no version changes.

**Reproduce.**

```bash
# The base: a detached worktree of 08e20455 with its own rust/target (a reflink copy of the
# head's target warms it; the base never builds into the head's target itself). This run's
# base lived at ~/.cache/sirio-wt/bezel-off-a-base.
git worktree add --detach ../bezel-off-a-base 08e20455
cp -a --reflink=auto rust/target ../bezel-off-a-base/rust/target   # optional, only to warm it
(cd ../bezel-off-a-base && TMPDIR=$HOME/.cache/sirio-e2e-tmp \
  Scripts/bezel-off-captures.sh --xvfb --out-dir "$OLDPWD/artifacts/bezel-off/pr-a/before")

# The head, in this tree (the same script, the same geometry).
TMPDIR=$HOME/.cache/sirio-e2e-tmp Scripts/bezel-off-captures.sh --xvfb \
  --out-dir artifacts/bezel-off/pr-a/after

# The diff.
Scripts/capture-diff.py artifacts/bezel-off/pr-a/before/frames artifacts/bezel-off/pr-a/after/frames \
  --out artifacts/bezel-off/pr-a/diff --masks Scripts/capture-masks.tsv

# The same pair with the masks as attributions, not paint: what changed under them.
Scripts/capture-diff.py artifacts/bezel-off/pr-a/before/frames artifacts/bezel-off/pr-a/after/frames \
  --out artifacts/bezel-off/pr-a/diff-unmasked --allow Scripts/capture-masks.tsv
```

**The result.** Both runs printed `CAPTURES OK: ... (94 frames)` (the chat E2E, the sweep, the
Changes E2E and the surfaces probe each passed on both sides, so `composer_keys`, `reduced_motion`,
`unfocused_spinner` and `long_history` held at the base and at the head), and the diff printed
`compared 94 frames present on both sides (94 before, 94 after), tolerance 1` and
`CAPTURE DIFF IDENTICAL`: no frame differs by more than one level outside the masks. No attribution
file was needed. `changed.tsv` has the header only and `index.html` lists nothing. The artifacts are
under `artifacts/bezel-off/pr-a/` (gitignored): `before/`, `after/`, `diff/` and `diff-unmasked/`. The
test suites are the same on both sides: in `sirio_ui`, `sirio_syntax`, `sirio_terminal` and
`sirio_theme`, 1598 tests run, 1597 passed and 1 skipped (`sirio_terminal`'s
`mutation_stamp_stays_flat_while_idle_and_bumps_on_output` failing in the full run on both sides and
passing alone, a known load flake) and 531 in `sirio` (all passed).

**What the masks hide here.** `IDENTICAL` says nothing about pixels inside a mask, and PR A renamed
code that draws under two of them: `chat/list_scroll.rs` (the transcript scrollbar, under the
scrollbar-thumb mask on `long-history/scrolled-up*`) and `sidebar/mod.rs` (a sidebar row, under the
session-dots mask). So the same pair was compared a second time with no `--masks` and the mask table
passed as `--allow`, which attributes each changed pixel to the mask rectangle that holds it
(`artifacts/bezel-off/pr-a/diff-unmasked/`): `CAPTURE DIFF OK (62 changed, all attributed)`. The 62
frames differ only inside mask rectangles whose `why` is a clock or a run-to-run accident: the status
bar's usage meter (29 frames) and path (50), message times (34), the Agents toolbar and subtitle (2),
the turn duration, the cancelled line and the crash addresses (1 each), and the session dots (1). The
scrollbar-thumb mask holds no changed pixel in any frame, so the transcript bar is identical. The one
frame where the session-dots mask holds a change is `restore/before-restart`; by eye, the base frame
draws two green connected dots (the `main` row and the corner of the Ely chat tab) and the head frame
draws none, and nothing else in that crop moves: the fixture-session race the mask names. The renamed
paths resolve to the same crate (the lock has one `bezel-theme`, 0.1.4), so no draw call changed.

### PR B: the pieces Ely lacks, rebuilt in Sirio (`refactor/bezel-off-b`)

To be filled in by Task 7.

### PR C: text fields and tooltips on Ely (`refactor/bezel-off-c`)

To be filled in by Task 10.

### PR D: menus, popovers, scrollbars and icons on Ely (`refactor/bezel-off-d`)

To be filled in by Task 12.

### PR E: Settings, then the crates out (`refactor/bezel-off-e`)

To be filled in by Task 14.
