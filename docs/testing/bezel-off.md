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
| Agents toolbar and subtitle | `surfaces-*/settings-agents-[dl]*` (the first page only) | The toolbar stamps `Refreshed HH:MM:SS` with the wall clock when the page is built, and Geist draws digits at different widths (`12:01:57` is 5 px narrower than `12:02:50`). The toolbar is right-aligned, so the stamp's width pushes the search field left (x 683..686 over 41 runs: 15 at random times and 26 more with `TZ` set so that every hour 00..23 was drawn) and narrows the header's left column, whose subtitle then wraps differently (3 of 15 runs). Nothing below y 170 or right of the stamp moved. | Two rectangles: the stamp and the search field, x 668..1032, y 54..94 (x 1022 until PR E: Ely's narrower Update All moved the stamp's last digit to x 1028, outside the old rectangle, and two capture runs then differed there by 48 and 50 pixels); and the subtitle block, x 475..687, y 88..170. The `Agents 5` title row, the `Update All` and `Refresh` buttons and everything below stay visible. |

No caret, spinner, orb or trace-timer mask was needed: with `SIRIO_REDUCED_MOTION=1` those draw
their rest state. A mask is for a clock or a run-to-run accident, sized to the whole element; a
difference of one level is not one (the tolerance takes it).

## Behaviour checks

Frames say what a surface looks like; these say what it does. Each passes on the tree before the
PRs that move a surface off bezel, and a PR that touches the behaviour must still pass it. The
checks written for this page (the thumb drag, `composer_keys` and its `composer_field_has_no_fill`,
`reduced_motion`, `unfocused_spinner`, the worktree select's test and the five gpui tests of Enter, the
model search and Ctrl+C) were each shown to fail by breaking the behaviour they
name, so a green run of them means something; the others are existing checks, named here so the
PRs know which ones gate them, and were not mutated for it.

| Behaviour | Check |
| --- | --- |
| Dragging a scrollbar thumb: the thumb follows the pointer, and a long drag stops at the track's end (file view, transcript list, horizontal bar) | `Scripts/Tests/test-scroll-drag-probe.sh --xvfb` |
| The transcript follows its tail, a scrolled-back view does not jump when a turn arrives | `Scripts/Tests/test-ely-chat-ui-e2e.py`, scenario `long_history` |
| The composer's keys as X delivers them: Return sends, Shift+Return is a newline, Up and Down move the caret with no popup open, the slash popup takes Down and Tab and closes on Escape | the same script, scenario `composer_keys` (fixture `composer`: `plain` advertises no slash commands) |
| The composer's field paints no fill and no frame of its own: with an empty draft every pixel of a band across the field is the card's colour, exactly | the same script, scenario `composer_keys`, check `composer_field_has_no_fill` (default 1470x833 window) |
| The worktree select answers Down, Enter and Escape while its search field has the focus, and a query that matches nothing leaves nothing to choose | `the_worktree_combobox_answers_its_keys_while_its_search_field_has_focus` |
| Reduced motion holds the thinking orb and the transcript still | the same script, scenario `reduced_motion` |
| A running turn's sidebar status bloom keeps turning while another window holds the X focus (Sirio's frame clock does not wait for an active window), and what that costs in CPU | the same script, scenario `unfocused_spinner`, with `--cpu-ceiling` |
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
thumb centre line that differ from that line's most common colour, so it does not depend on what
draws the bar (the line is 5 px in from the pane's edge, and 9 px in for pane 1, whose bar has been
Ely's since PR D: a 12 px strip with its 6 px thumb in the strip's left half). A 900x600 window of
three 300x600 panes (`sirio_ui::probes::scroll_drag_view`); the pointer presses a thumb's centre,
moves 3 px (past gpui's 2 px threshold, where the drag starts) then 1 px (the move whose event is the
first drag event, where every bar anchors its grab), then 120 px in six steps. The thumb must have
moved as far as the pointer did after its second move, within 4 px, and a second drag of +2000 px (the
pointer stops at the screen's edge, past the track's end) must leave it flush with the track's end.

### The unfocused spinner and its CPU base

`unfocused_spinner` opens a turn, gives a second window (`surfaces_probe --frame tooltip`) the X
focus, raises Sirio's window back on top without handing the focus back (it asserts
`xdotool getwindowfocus` is the second window before and after the raise), and takes three frames
250 ms apart. At least one pair must differ in the sidebar row's status bloom.

It judges the bloom and not the transcript on purpose. The transcript's thinking row carries Ely's
`ShimmerText`, a gpui animation that asks for its own frames and does not go through the frame
clock, and every repaint of the chat view samples the orb's live phase, so the transcript moves
whether or not the clock is gated on an active window. With bezel's pause turned on the transcript
still differed in three of three runs: that check could not fail. The row's bloom is drawn through
the clock alone (a `Painter` lease taken by the row's own cached view, and gpui replays a cached
view until it is notified), so it freezes exactly when the clock stops leasing. The tab strip's
working mark is not such a region: it kept moving with bezel's pause on.

Which clock that is changed in PR B. Until Task 6 the bloom drew on bezel-motion's clock, whose
`lease` is refused while no window of the app is active and the pause is on, and `main.rs`'s
`cx.set_pause_when_inactive(false)` is what turned the pause off. Shown red then by changing that
`false` to `true` (`sirio` rebuilt): the bloom's region was identical in all three frames in 5 of 5
runs, and 40 to 141 differing pixels per pair with it off in every run. Since Task 6 the bloom is
drawn on `sirio_ui::motion`'s clock, a port that has no pause switch: it keeps turning while no
window of the app is active, because an agent runs for minutes while the user works elsewhere and
the bloom is what says it is still going. (`main.rs` set bezel's flag too, for the clock that
bezel's menu rows and scrollbars leased, until PR E's last task removed the call: no widget leases that clock any more.) The red proof is now the
gated lease: `motion::lease` made to return early when `cx.active_window().is_none()`, which is
what bezel's pause did. Run in Task 6 with `--only unfocused_spinner`, the unfocused crops were
`[0, 0, 0]` and the focused control `[90, 120, 102]`, and the run failed with the "froze while
Sirio was unfocused" message below; restored, it was green again. The same gate applied in Task 5,
before the loaders moved, left the check green because the bloom still ran on bezel's clock.

The rectangle (`SIDEBAR_BLOOM`, in pixels of the default window) has a control on the failure path
only: when the three crops are identical the scenario gives Sirio the focus back and looks at the
same crop again. If it moves, the failure says the bloom froze while unfocused, so the clock pauses
without an active window; if it still does not, the failure says `SIDEBAR_BLOOM <rect> holds nothing
that moves even with Sirio focused; the row moved, update the rectangle`. A PR that restyles the sidebar row or its status
bloom re-runs the gated-lease mutation too, to be sure the new rectangle still discriminates.

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
draws none, and the only other change in that crop is the row's star moving right into the dot's
place, inside the mask too: the fixture-session race the mask names. The renamed
paths resolve to the same crate (the lock has one `bezel-theme`, 0.1.4), so no draw call changed.

### PR B: the pieces Ely lacks, rebuilt in Sirio (`refactor/bezel-off-b`)

PR B changes no pixel: it ports the parts of `bezel-ui` and `bezel-motion` that Ely has no
counterpart for into `sirio_ui`, and points Sirio's callers at them. Each ported file carries the
header `// Ported from bezel-ui 0.1.4 (MIT); see THIRD_PARTY_NOTICES.md.` (or `bezel-motion`) and is
listed in the "bezel-ui and bezel-motion (ported code)" section of `THIRD_PARTY_NOTICES.md`.

**Base and head.** The "before" set is `artifacts/bezel-off/pr-a/after`, captured at `7ea323c3`. PR
A's head `dfe14a3b` changed only documentation and a manifest comment after it (`git diff --stat
7ea323c3 dfe14a3b`: `docs/testing/bezel-off.md`, `rust/crates/sirio_terminal/Cargo.toml` +1,
`rust/vendor/README.md`), so `7ea323c3`'s pixels are the pixels of PR B's base. The "after" set was
captured at `04fa408c`, the head of PR B's code (the commits after it change only documentation: this page and
`docs/agents/e2e-tests.md`).

**What PR B moved into `sirio_ui`.**

| Task | Where | What |
| --- | --- | --- |
| 5 | `motion/` | The frame clock (`Painter`, `lease`), with no pause switch; the hover fades; the cubic-bezier curves; the loaders' phase maths. |
| 6 | `loading.rs` | The orbs, the mini spinner and the skeleton rows. The sidebar row's status bloom now draws on Sirio's clock. |
| 6 | `popup.rs` | `Popup`, `Filter` and `reap_popup`, the open / closing / closed lifecycle of a menu and the searchable list a picker shows. |
| 6 | `chat/thought.rs`, `sidebar/tree_keys.rs` | `Takeover` (whether a thought's body follows the run or obeys the reader); the sidebar tree's arrow-key bindings and cursor step. |
| 7 | `scroll.rs` | `MIN_THUMB`, `thumb` and `offset_for_thumb`, `ScrollbarDrag`, and `FollowState` with `follow` (the pin the thought bodies use). The terminal's, the transcript's and the horizontal bar read them from here. |

**What still reaches bezel, and until which task.** The kept sites are the ones where a value goes
straight into a bezel item, so a ported type would need a glue type:

- `bezel::ui::scroll::{scrollbar, ScrollbarState}` and the `bezel::motion::Painter::of(cx)` handed
  to `ScrollbarState::new` (the file view's vertical bar, the thought bodies' bars, the trace tab
  and the CI log tab): Task 12. Their drag payload is bezel's own `ScrollbarDrag`, a different type
  from `sirio_ui::scroll::ScrollbarDrag`, so a bezel bar and a Sirio bar no longer answer each
  other's drags (`on_drag_move` filters by payload type). The probe's pane 1 is a bezel bar next to
  Sirio's list and horizontal bars.
- `bezel::ui::popover` (`menu_row`, `popover_card`, ...) and the `Fade`, `Painter` and `menu_in`
  that feed it, in the sidebar, the tab bar, the files panel and the composer's popups: Task 11.
  bezel's combobox keeps its own `Popup` and `Filter`: Task 9.
- `ui::tooltip::Tooltip` in `sirio_terminal`, which is why that crate keeps its `bezel` dependency:
  Task 10.
- `main.rs`'s `cx.set_pause_when_inactive(false)` (with a chat test and two examples): Task 14. It
  sets the flag of the clock bezel's remaining items lease; the clock the bloom draws on has no flag.
- The rest of `bezel::ui` (`widgets`, `surface`, `icons`, `input`): PRs C to E.

**Reproduce.**

```bash
# The before set is PR A's after set, already on disk (see above). The after set:
TMPDIR=$HOME/.cache/sirio-e2e-tmp Scripts/bezel-off-captures.sh --xvfb --out-dir artifacts/bezel-off/pr-b/after

# The diff, then the same pair with the mask table as attributions.
Scripts/capture-diff.py artifacts/bezel-off/pr-a/after/frames artifacts/bezel-off/pr-b/after/frames \
  --out artifacts/bezel-off/pr-b/diff --masks Scripts/capture-masks.tsv
Scripts/capture-diff.py artifacts/bezel-off/pr-a/after/frames artifacts/bezel-off/pr-b/after/frames \
  --out artifacts/bezel-off/pr-b/diff-unmasked --allow Scripts/capture-masks.tsv

# The behaviour checks (Xvfb :N is any free display, 1920x1080x24; lavapipe as for the captures).
Scripts/Tests/test-scroll-drag-probe.sh --xvfb
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json \
  TMPDIR=$HOME/.cache/sirio-e2e-tmp Scripts/Tests/test-ely-chat-ui-e2e.py --display :N \
  --only composer_keys,reduced_motion,unfocused_spinner,long_history --cpu-ceiling 413.1
Scripts/Tests/test-text-selection-e2e.sh
(cd rust && cargo nextest run -p sirio_ui && cargo nextest run -p sirio_terminal)
```

**The result.** `bezel-off-captures.sh` printed `CAPTURES OK: artifacts/bezel-off/pr-b/after (94
frames)`; the masked diff printed `compared 94 frames present on both sides (94 before, 94 after),
tolerance 1` and `CAPTURE DIFF IDENTICAL`. `changed.tsv` has the header only. The artifacts are under
`artifacts/bezel-off/pr-b/` (gitignored): `after/`, `diff/` and `diff-unmasked/`.

| Check | Line |
| --- | --- |
| `Scripts/Tests/test-scroll-drag-probe.sh --xvfb` | `SCROLL DRAG PROBE OK (3 bars)` (each bar's thumb moved 120 px for 120 px of pointer, within the 4 px tolerance, and after +2000 px sat flush with its track's end) |
| `Scripts/Tests/test-ely-chat-ui-e2e.py ... --cpu-ceiling 413.1` | `ELY CHAT UI E2E OK`; `summary.json`: `long_history`, `composer_keys`, `reduced_motion`, `unfocused_spinner` all `passed`. `reduced_motion`: transcript pixels differing `[0, 0, 0]`. `unfocused_spinner`: bloom pixels differing `[143, 68, 141]`, and `unfocused-spinner CPU 324.9% of one core over 10 s (load average 4.87 8.77 9.27)`, under the ceiling of 413.1 and next to the base of 329.7%. |
| `Scripts/Tests/test-text-selection-e2e.sh` | `TEXT SELECTION E2E OK (33 tests; ...)` |
| `cargo nextest run -p sirio_terminal the_scrollbar_follows_the_viewport_and_dragging_it_scrolls_the_terminal` | 1 passed |
| The twelve gpui tests the table names (Enter and IME, paste, the composer's keys, the model search, Enter in a form's field, Ctrl+C in the composer, the live thought) | all 12 in the passed list of the full `sirio_ui` run |

The suites on the tree before Task 7 and after it: `sirio_ui` 1398 run, 1398 passed, 0 skipped;
`sirio_terminal` 143 run, 143 passed, 1 skipped; the sorted `cargo nextest list` of each is identical
before and after, so no test was added, removed or renamed (the terminal's 19 scrollbar unit tests
and its 3 drag tests are among them). `cargo build --workspace --all-targets` builds.

**The unfocused spinner.** The check that proves the bloom keeps turning while another window holds
the focus was shown red against the ported clock in Task 6: `motion::lease` made to take no lease
while no window of the app is active left the unfocused crops identical (`[0, 0, 0]`), the focused
control moved (`[90, 120, 102]`), and the run failed with the "froze while Sirio was unfocused"
message; restored, it was green again. Why that is the proof, and why the bloom and not the
transcript, is under [The unfocused spinner and its CPU base](#the-unfocused-spinner-and-its-cpu-base).

**What the masks hide here.** `IDENTICAL` says nothing about pixels inside a mask, and PR B moved
code that draws under two of them: the transcript's scrollbar (`chat/list_scroll.rs`, under the
scrollbar-thumb mask on `long-history/scrolled-up*`) and the sidebar's rows (the session-dots mask).
So the pair was compared a second time with no `--masks` and the mask table as `--allow`
(`artifacts/bezel-off/pr-b/diff-unmasked/`): `CAPTURE DIFF OK (62 changed, all attributed)`, the same
62 frames and the same causes as PR A's pass: the status bar's usage meter (29 frames) and path
(50), message times (34), the Agents toolbar and subtitle (2), the turn duration, the cancelled line
and the crash addresses (1 each) and the session dots (1). The scrollbar-thumb mask holds no changed
pixel in any frame: the four `long-history` frames differ only under the status bar and message-time
masks, so the transcript bar is identical. The frame where the session-dots mask holds a change is
`chat-ui-default/scenarios/activity/frames/tools-and-reasoning`; by eye, the base draws two green
connected dots (the `main` row and the corner of the tab icon) and the head none, and the only other
change in that crop is the row's star moving right into the dot's place, inside the mask too: the
fixture-session race the mask names. The captures run with
`SIRIO_REDUCED_MOTION=1`, which holds every animation at rest, so they cannot see the running bloom
or orb turn; the `unfocused_spinner` row above is what does.

### PR C: text fields and tooltips on Ely (`refactor/bezel-off-c`)

PR C draws every text field and every tooltip with Ely, and changes pixels on purpose: 22 of the 94
frames, all attributed in `docs/testing/bezel-off/pr-c-allow.tsv`. Nothing else moved outside the
masks; what moved inside them is under "What the masks hide here" below.

**Base and head.** The "before" set is `artifacts/bezel-off/pr-b/after`, PR B's after set, captured
at `04fa408c`. PR C's git base is `10243f3a`, and `git diff 04fa408c 10243f3a -- rust/` is two
comments (the doc of `init_motion` in `sirio/src/main.rs` and one in `sirio_terminal/src/scrollbar.rs`),
so the "before" frames are the frames of PR C's base and the base was not captured again. The "after"
set was captured on the working tree of PR C's last commit, `refactor: draw tooltips with Ely`: its
`rust/` diff against `c90f6b71` (the commit before it) is what the tree held while the capture ran.
PR C's code is eight commits on `10243f3a`: Ely's text field learns to be hosted (`57d96855`, `379af0eb`,
`7620843c`), every field on Ely (`4f07a14a`), the editing chords, the framed field's height and Ely's
word unit (`5a2e910b`, `dd88109d`, `c90f6b71`) and the tooltips (`07580658`). Two follow-ups from the
review of the last one come after the capture: `f51d01ab` makes the sidebar and files-panel demos init
Ely (they panicked on a tooltip hover) and folds `segmented_icons_with_tooltip` into `segmented_icons`,
which draws the same tooltips; `139e43c6` is docs.

**What PR C moved.**

| Task | Where | What |
| --- | --- | --- |
| 8 | Ely `forms/text`, `menus` (vendored) | `TextInput::detached` (no `Window`), `host_context` (a host's keys beat Ely's), `reset_text`, the wheel scrolls a multi-line field without the caret pulling it back, `menus::row_frame`. Logged in `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md`. |
| 9 | the composer, the model search, the clone and create forms, the Agents search, the git-hosting token fields, the browser address bar | `TextInput` drawn by Ely's `forms::Input` (or raw, in the composer, which has no frame of its own). Enter in a form or the address bar is `InputEvent::Submit`. |
| 9 | `combobox.rs` | The worktree combobox, ported from bezel-ui 0.1.4 (MIT) onto Ely's input, panel and rows; `worktree_picker.rs` changes only its import. |
| 9 | Ely `forms/text` (vendored) | The editing chords bezel's field bound (delete to the line's start and end, delete the word on the right, the macOS emacs set, Ctrl+N and Ctrl+P in a multi-line field), bezel's word unit for every word motion and delete, and a one-line `Input` that is at least its control height and grows with its text. Logged in `LOCAL-CHANGES.md`. |
| 10 | `controls::tooltip`, `controls::tooltip_with_meta` | Ely's `Tooltip::text` and `with_meta`. The 14 sites that drew bezel's `Tooltip` (the pane launcher's unavailable items, the chat's mode, model, Fast and background-task chips, the file view's context-menu notes and Markdown mode icons, the Agents path, the open list, the composer's slash rows, the sidebar's and the files panel's icon buttons, the terminal's context menu, and "Hide Pane" in `main.rs`) call it; `sidebar_tooltip` and `TooltipBuilder` are gone. |
| 10 | `sirio_terminal` | Its `bezel` dependency was for this tooltip alone and is dropped (`Cargo.lock` loses that one edge). `examples/terminal_demo.rs` now calls `sirio_ui::ely::init`, because Ely's tooltip reads Ely's theme global. |

**Reproduce.**

```bash
# The before set is PR B's after set, already on disk (see above). The after set:
TMPDIR=$HOME/.cache/sirio-e2e-tmp Scripts/bezel-off-captures.sh --xvfb --out-dir artifacts/bezel-off/pr-c/after

# The diff, with this PR's attribution file.
Scripts/capture-diff.py artifacts/bezel-off/pr-b/after/frames artifacts/bezel-off/pr-c/after/frames \
  --out artifacts/bezel-off/pr-c/diff --masks Scripts/capture-masks.tsv \
  --allow docs/testing/bezel-off/pr-c-allow.tsv

# The same pair with no masks, and the masks plus this PR's rows as the allow table.
cat Scripts/capture-masks.tsv docs/testing/bezel-off/pr-c-allow.tsv > artifacts/bezel-off/pr-c/allow-unmasked.tsv
Scripts/capture-diff.py artifacts/bezel-off/pr-b/after/frames artifacts/bezel-off/pr-c/after/frames \
  --out artifacts/bezel-off/pr-c/diff-unmasked --allow artifacts/bezel-off/pr-c/allow-unmasked.tsv

# The theme sweep (Xvfb :N is any free display, 1920x1080x24; lavapipe as for the captures). The
# out-dir is absolute: a relative one fails with "app exited before creating its private control socket".
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json \
  TMPDIR=$HOME/.cache/sirio-e2e-tmp Scripts/Tests/test-theme-sweep.sh --display :N \
  --out-dir "$PWD/artifacts/bezel-off/pr-c/theme-sweep"

(cd rust && cargo build --workspace --all-targets && cargo nextest run -p sirio_ui && cargo nextest run -p sirio \
  && cargo nextest run -p sirio_terminal)
```

**The result.** `bezel-off-captures.sh` printed `CAPTURES OK: artifacts/bezel-off/pr-c/after (94
frames)` (the chat E2E, the sweep, the Changes E2E and both surfaces probes passed, among them the
probe's tooltip check: the hover must add at least 150 pixels near the box). The masked diff printed
`compared 94 frames present on both sides (94 before, 94 after), tolerance 1` and
`CAPTURE DIFF OK (22 changed, all attributed)`. The theme sweep printed `THEME SWEEP OK: <checkout>/artifacts/bezel-off/pr-c/theme-sweep` (the 9 base colours, dark and light: 18 runs of `visual-sweep.sh`). The artifacts are under
`artifacts/bezel-off/pr-c/` (gitignored): `after/`, `diff/`, `diff-unmasked/`, `crops/`,
`hide-pane-tooltip/` and `theme-sweep/`.

| Check | Result |
| --- | --- |
| `cargo build --workspace --all-targets` | exit 0; the same 135 warning lines, at the same files, as on `c90f6b71` |
| `cargo nextest run -p sirio_ui` | 1414 run, 1414 passed, 0 skipped; the sorted `cargo nextest list` is identical to `c90f6b71`'s |
| `cargo nextest run -p sirio` | 531 run, 531 passed; the list is identical |
| `cargo nextest run -p sirio_terminal` | 143 run, 143 passed, 1 skipped; the list is identical |
| `cargo nextest run -p sirio_ui tooltip` | 1 test (`trace_tab::tests::the_tooltip_names_kind_name_start_duration_model_tokens_and_status`), as on `c90f6b71` |
| `cargo nextest run -p sirio_terminal tooltip` | 0 tests, as on `c90f6b71`: no test of that crate has the word in its name, so the filter proves nothing for the terminal's disabled-row tooltip (`a_too_narrow_pane_disables_split_left_on_a_single_line` drives that row, by click) |

No test was added, removed or renamed by the tooltip commit. No test hovers a control to read its
tooltip: what shows a tooltip drawn is the probe's `tooltip` frame, and `tooltip_with_meta` is a
pass-through that only the hand capture under `hide-pane-tooltip/` has drawn.

**Every changed frame** (`artifacts/bezel-off/pr-c/diff/changed.tsv`).

| Frames | Pixels | bbox (x,y,w,h) | Allow row | What |
| --- | --- | --- | --- | --- |
| `chat-ui-default/identity/{claude,codex,omp,opencode,pi}-{dark,light}` (10) | 36 each | 43,562,2,18 | `ui::input::TextField` | The empty composer's caret: bezel's field drew a 2 px bar (`#939393`), Ely's draws a 1 px bar (`#878787`) at the same x, y and height. |
| `chat-ui-default/scenarios/effort/frames/effort-low`, `effort-light/frames/effort-low`, `wide-content/frames/wide-content-diff`, `wide-content-expanded`; `sweep-{dark,light}/chat`, `sweep-{dark,light}/empty-workspace` (8) | 36 each | 349,650,2,18 | `ui::input::TextField` | The same caret. Outside it nothing differs by more than the tolerance, so the composer did not move. |
| `surfaces-dark/settings-git-hosting-accounts-dark` | 34919 | 475,224,720,307 | `ui::input::TextField` | Both token fields (github.com on `gh`, the self-hosted GitLab) are Ely's framed input: 28 px high where bezel's was 34 px, with a 14.5 px placeholder where it was 13 px. The rows under each move up, so the changed pixels run from the first field to the card's bottom edge. |
| `surfaces-light/settings-git-hosting-accounts-light` | 34878 | 475,224,720,307 | `ui::input::TextField` | The same. |
| `surfaces-dark/tooltip-dark` | 6254 | 255,210,107,79 | `ui::tooltip::Tooltip` | The probe's tooltip: a solid fill with a smaller radius and a floating shadow where bezel's was a translucent popover card, 13 px text where it was 12 px, and 18 px below the pointer (Ely's `cursor_offset`) where bezel's sat at the pointer and covered the target's lower edge. The box is the union of both footprints with their shadows. |
| `surfaces-light/tooltip-light` | 8480 | 243,205,132,86 | `ui::tooltip::Tooltip` | The same. In light the fill is `tooltip_bg`, 236 against the page's 232, so the shadow is what draws the tooltip's edge. |

The 20 text-field frames are the 20 frames `artifacts/bezel-off/pr-c/task-9-r1-diff/changed.tsv` listed
for Task 9's last fix round (the same frames, pixel counts and boxes); the tooltip frames are the 2 new
ones. `crops/` has the before and after of the git-hosting rows and of the tooltip in both appearances.

**Changes no capture shows.** None is in the 94 frames, and each is on the list of what Enzo should
see. Most are Ely's own look or a key that works as bezel's field did. Three are not parity with `main`
and are behaviour changes: the word unit in the fields that were already Ely's, Ctrl+K on macOS in those
same fields, and Shift+Enter and Alt+Enter in the one-line forms and the address bar. One is a gap in
what is verified: the macOS-only chord tests.

- The clone and create forms, the browser address bar, the model search and the combobox query take Ely's
  framed field: 28 px high where bezel's was about 34 px, its text at Ely's Base size (14.5 px at the
  default) where it was 13 px.
- Framed fields now grow with the interface font from size 15: the frame is at least its control height
  (28 px), and as tall as its line plus its border above that, so the caret and the selection stay
  inside it. Nothing in the captures runs at those sizes; one test (`the_agents_search_keeps_its_row_inside_its_frame_at_every_interface_size`) does.
- The worktree combobox is rebuilt on Ely's panel and rows: "No matches" inherits the card's text size
  (it was 13 px), the chevron is Ely's `ChevronDown` (bezel's was a 14 px alt-arrow), and a row has no hover fade.
- The editing chords are restored in every field, the composer included. On macOS: Cmd+Backspace
  (delete to the line's start), Option+Delete (the word on the right), Ctrl+K (to the line's end), the emacs set
  (Ctrl+A, E, B, F, H, D) and Ctrl+N and Ctrl+P as Down and Up in a multi-line field. On Linux and Windows:
  Ctrl+Delete (the word on the right).
- Word motion and deletion use bezel's word unit (a run with a letter or a digit in it) in every Ely text
  field, including the ones that were already Ely's before this PR (the change-request panel and tab, the
  editor page, the drawer search, the file view's find fields). Going back a word from the end of
  `Hello world.` lands before `world`, not between `world` and the dot.
- On macOS, Ctrl+K inside the fields that were already Ely's no longer reaches the palette's fallback: it
  deletes to the line's end, as in a native field. Ctrl+Shift+P still opens the palette everywhere.
- Shift+Enter and Alt+Enter no longer submit the one-line forms and the address bar (the old handler
  answered the Enter key with any modifier): Shift+Enter is Ely's newline, which a one-line field ignores.
  Enter, and Ctrl+Enter (Cmd+Enter on macOS), still submit.
- A double-click in a field that was bezel's now selects what is under the pointer (a word, a run of
  punctuation or a space): Ely's field selects on a double-click, bezel's did not.
- Each delete in an Ely field is its own undo step, where bezel's joined a delete onto a run of
  backspaces before it, and a delete that removes nothing records no step.
- The model picker's search head is 6 px shorter, since its field is 28 px where it was about 34 px.
- The macOS-only chord tests (the macOS table of `the_composer_answers_the_editing_chords_of_its_platform`,
  the Ctrl+N and Ctrl+P row test and the one-line guard) first run in the release gate: they never run on
  Linux, and `macos-check.yml` builds and links the app but runs no test suite. The macOS bindings were
  exercised on Linux only by forcing them on for one run.
- The tooltips differ from bezel's in ways the probe's one tooltip does not draw:
  - "Hide Pane" shows its chord, `Ctrl+Shift+B` (the label its binding uses), through `tooltip_with_meta`.
    bezel's `with_keystroke` drew it in 11 px faint text after the title, in the popover card; Ely's draws
    it at the title's size at 60% opacity. One hand capture of it, from a temporary edit of the probe that
    is not in the commit, is under `artifacts/bezel-off/pr-c/hide-pane-tooltip/`.
  - The sidebar's and the files panel's icon-button tooltips were a popover card with 12 px type scaled by
    the sidebar's compact typography and 8 px by 5 px of padding. They are Ely's tooltip now: text at the
    interface size (13 px at the default, so it follows the user's font size), 8 px by 4 px of padding.
  - A tooltip has a maximum width of 256 px and wraps beyond it (the Agents path and a slash command's
    description are the long ones); bezel's had none.
  - A tooltip fades in and rises 4 px. `SIRIO_REDUCED_MOTION` (debug builds) holds that at rest in the
    captures; the animation follows the reduced-motion flag of the rest of Ely's motion.
  - A tooltip sits 18 px below the pointer in every surface, as in the probe.

**What the masks hide here.** `CAPTURE DIFF OK` says nothing about pixels inside a mask, and PR C
migrated the Agents search field, which lies inside a standing mask (the Agents toolbar's, x 668..1022,
y 54..94). So the same pair was compared with no `--masks` and the mask table plus this PR's rows as
`--allow` (`artifacts/bezel-off/pr-c/diff-unmasked/`): `CAPTURE DIFF OK (66 changed, all attributed)`.
The 22 frames above, and 44 more that differ only inside mask rectangles whose `why` is a clock or a
run-to-run accident: the status bar's usage meter and path, message times, the Agents toolbar's clock
(the two frames below), the turn duration, the
cancelled line, the crash addresses, the session dots (`restore/before-restart`), and one frame where
the scrollbar-thumb mask holds 100 changed pixels (`long-history/frames/scrolled-up-after-new-turn`, the
track's y 495..526: the accident the mask names, in a bar PR C does not touch).

The Agents toolbar is the new case. `surfaces-dark/settings-agents-dark` changes 7825 pixels in
684,56,333,34 and `surfaces-light/settings-agents-light` 8328 in 683,56,333,34: the search field and
the `Refreshed HH:MM:SS` stamp, which is the clock. The field alone is x 683..904 (7436 pixels in dark,
7466 in light) and the stamp's digits are the rest (389 and 862). In this pair the stamps have the same
width, so the field did not drift sideways. By eye (`crops/agents-search-{dark,light}-{before,after,stack}.png`
and `crops/agents-{dark,light}-stack.png`): the field is 28 px high where it was 34 px, its radius is
smaller, its placeholder is 14.5 px where it was 13 px, and in light the fill is the card's grey with a
border where bezel's was white. That is Ely's framed input; it is the one text-field change a capture reaches
only without the mask.

Enzo's verdict: approved on 2026-10-27, on the page built from these captures.

### PR D: menus, popovers, scrollbars and icons on Ely (`refactor/bezel-off-d`)

PR D draws every menu and popover, every scrollbar over a gpui `ScrollHandle` and two icons with Ely,
and changes pixels in 2 of the 94 frames, both the probe's new-tab menu (dark and light), attributed in
`docs/testing/bezel-off/pr-d-allow.tsv`. No frame draws a scrollbar that moved, nor either icon, so
Task 12's half of the PR is judged by the probes and by "Changes no capture shows" below, not by frames.

**Base and head.** The "before" set is `artifacts/bezel-off/pr-c/after`, PR C's after set, captured at
`07580658`. PR D's git base is `64ed9ae2`, and `git diff 07580658 64ed9ae2 -- rust/` is two demos
gaining `sirio_ui::ely::init` (`right_panel_demo.rs`, `sidebar_demo.rs`) and a function fold in
`controls.rs` (`segmented_icons_with_tooltip` into `segmented_icons`) with no drawing change, so the
"before" frames are the frames of PR D's base and the base was not captured again. The "after" set was
captured on the tree of `89bbd118`, PR D's last code commit (the working tree was clean). PR D's code is
six commits on `64ed9ae2` and a notices line: the menus and popovers (`06e28245`), the demos that open a
menu init Ely (`cae7e14a`), labels kept inside their card at large interface sizes (`0644c3ca`), two tests
(`632645e6`, `a10c6cda`) and the scrollbars and icons (`89bbd118`), with `THIRD_PARTY_NOTICES.md` listing
`transcript.rs` among the files ported from bezel-ui (`9198f15c`, a `docs:` commit). The commits
after the capture change no pixel: this page and the allow file, the header of `examples/scroll_drag_probe.rs`
(the bars' thickness) and of `test-scroll-drag-probe.sh` (where each thumb sits), the module doc of
`scroll.rs`, one more test of its frame-request helper, and
`composer_view.rs` and `sidebar/sessions.rs` importing Ely's icon types instead of spelling their paths.

**What PR D moved.**

| Task | Where | What |
| --- | --- | --- |
| 11 | `popup.rs` | The drawing half next to Task 6's lifecycle: `menu_card` (Ely's `panel_surface`), `menu_row` (Ely's `row_frame`), `menu_label` (the row-label treatment that ellipsizes), `divider`, `leaving` (the exit, with an occluding overlay), and `hang_below` / `hang_above_at` over one private `hang` (anchored, snapped 8 px inside the window, Ely's fade-in, raised as a layer). |
| 11 | the sidebar's context, New Chat, add-project and worktree-close menus; the tab context menu and the new-tab menu; the files context menu; the slash and mention popups; the combobox menu; the turn preview | Each is `menu_card` + `menu_row` + `divider` through `hang_below`, `hang_above_at` or Ely's `layer`. `bezel::ui::popover`, `bezel::ui::surface` and `anchored_menu_below` are no longer reached. |
| 11 | `sirio` | Its `bezel_theme` dependency goes (the guard in `render_tab_context_menu` was its only reader): `Cargo.lock` loses `sirio -> bezel-theme`. |
| 12 | `file_view::scrollbar` and its callers: the Markdown preview, the CI log, the trace tab; the thought well; pane 1 of the scroll probe | Ely's `layout::Scrollbar` over the tab's `ScrollHandle`. `ScrollbarState` and the bezel `Painter` handed to it are gone from `SurfaceScroll`, `CiLogTab`, `TraceTab`, `ThoughtScroll` (`new()` takes none now) and the probe. `scrollbar(id, handle)` keeps the first-frame canvas and its debug selector. |
| 12 | `scroll::request_frame_when_geometry_moves` | One helper: a canvas that asks for one more frame when the box's length or overflow along an axis, as this frame laid it out, is not what the bar beside it was built from. `file_view::scrollbar` lays it over a bar that is up, and Ely's horizontal bar under a tool call's diff (`chat/mod.rs`) uses it instead of its own canvas. |
| 12 | `composer_view.rs`, `sidebar/sessions.rs` | The mention row's `DOCUMENT` is Ely's `FileText` at `IconSize::Xs` (12 px), the closed chat's delete button's `TRASH_BIN_MINIMALISTIC` is `Trash2` at `Sm` (14 px), each in today's colour. |
| 12 | `Scripts/Tests/test-scroll-drag-probe.sh` | Pane 1's thumb is 4 px nearer the pane's centre (below), so its scan line is 9 px in from the edge instead of 5. Nothing else of the check moved (tolerance, anchoring move, end-flush). |
| 12 | six sidebar test hosts | `cx.update(crate::ely::init)`: they draw a closed session, whose delete button is an Ely `Icon`, which reads Ely's theme. |

What still reaches bezel after PR D, all of it for PR E (`grep -rn 'bezel::' rust/crates`):

- `bezel::ui::widgets` (the button styles and `Buttons`, `Status` and `status_dot`, `Controls`, `Content` and
  `Scaffolding`, `card_row_hover`) in `loading.rs`, `settings.rs`, `settings/git_hosting_page.rs`,
  `settings/agents_page.rs`, `settings/language_servers_page.rs` and `chat/turn_rail.rs`, and in the tests of
  `chat/tool_calls.rs` and `chat/ely_tests.rs`;
- `bezel::ui::icons`, read directly at `settings/git_hosting_page.rs:214` (`MAGNIFER`) and
  `settings/agents_page.rs:9`, and through the `AppAssets` fallback in `ely.rs`;
- `bezel::motion::AppExt`, in `init_motion` (`sirio/src/main.rs`), in one chat test and in two examples
  (`scroll_drag_probe.rs`, `surfaces_probe.rs`).

No vendored Ely file changed in Task 12, so `LOCAL-CHANGES.md` has no entry for it.

**Reproduce.**

```bash
# The before set is PR C's after set, already on disk (see above). The after set. On a busy machine start
# your own Xvfb (1600x1000x24, a display number nobody else uses) and pass --display instead of --xvfb.
TMPDIR=$HOME/.cache/sirio-e2e-tmp Scripts/bezel-off-captures.sh --xvfb --out-dir "$PWD/artifacts/bezel-off/pr-d/after"

# The diff, with this PR's attribution file.
Scripts/capture-diff.py artifacts/bezel-off/pr-c/after/frames artifacts/bezel-off/pr-d/after/frames \
  --out "$PWD/artifacts/bezel-off/pr-d/diff" --masks Scripts/capture-masks.tsv \
  --allow docs/testing/bezel-off/pr-d-allow.tsv

# The same pair with no masks, and the masks plus this PR's rows as the allow table.
cat Scripts/capture-masks.tsv docs/testing/bezel-off/pr-d-allow.tsv > artifacts/bezel-off/pr-d/allow-unmasked.tsv
Scripts/capture-diff.py artifacts/bezel-off/pr-c/after/frames artifacts/bezel-off/pr-d/after/frames \
  --out "$PWD/artifacts/bezel-off/pr-d/diff-unmasked" --allow artifacts/bezel-off/pr-d/allow-unmasked.tsv

# The theme sweep (Xvfb :N is any free display, 1920x1080x24; lavapipe as for the captures; the out-dir is absolute).
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json \
  TMPDIR=$HOME/.cache/sirio-e2e-tmp Scripts/Tests/test-theme-sweep.sh --display :N \
  --out-dir "$PWD/artifacts/bezel-off/pr-d/theme-sweep"

# The probes and E2Es this PR's code is held to (the lavapipe variables and TMPDIR as above).
Scripts/Tests/test-scroll-drag-probe.sh --xvfb
Scripts/Tests/test-markdown-scroll-probe.sh --xvfb
Scripts/Tests/test-editor-e2e.sh --xvfb
Scripts/Tests/test-ely-chat-ui-e2e.py --display :N --only composer_keys,reduced_motion,unfocused_spinner,long_history --cpu-ceiling 413.1
Scripts/Tests/test-text-selection-e2e.sh

(cd rust && cargo build --workspace --all-targets && cargo nextest run -p sirio_ui && cargo nextest run -p sirio \
  && cargo nextest run -p sirio_terminal)
```

**The result.** `bezel-off-captures.sh` printed `CAPTURES OK: artifacts/bezel-off/pr-d/after (94
frames)` (the chat E2E, the sweep, the Changes E2E and both surfaces probes passed). The masked diff
printed `compared 94 frames present on both sides (94 before, 94 after), tolerance 1` and
`CAPTURE DIFF OK (2 changed, all attributed)`. The theme sweep printed `THEME SWEEP OK:
<checkout>/artifacts/bezel-off/pr-d/theme-sweep`. The artifacts are under `artifacts/bezel-off/pr-d/`
(gitignored): `after/`, `diff/`, `diff-unmasked/`, `crops/`, `icons/`, `menu-size-18/`, `probes/`,
`theme-sweep/` and `capture-attempts/`.

It took four capture runs, and three of them are not the set. They ran while other worktrees on this
machine kept its load average between 9 and 20, and the script's own Xvfb and the app's control socket
did not always survive that (`capture-attempts/`):

- Run 1 completed (94 frames) and had one frame the PR did not change: `chat-ui-default/identity/omp-light`,
  6929 pixels. The identity probe types `hello`, presses Return and captures at once; this capture caught
  the composer holding `hello` with the status still `connecting`, before the send landed. The same frame
  in the final set is the finished conversation, identical to PR C's. That is a race in the identity lane
  under load, not a drawing change, and it is not in the allow file.
- Run 2 stopped at `sweep-dark` (`sirioctl: control action dispatch bound (5.0s) fired before the worker
  replied` on `project.add`; the load average was about 20).
- Run 3 stopped at `surfaces-light` (`Failed to initialize X11 client` from the probe; the cause is not
  established).
- Run 4 ran on an Xvfb started beforehand (`Xvfb :137 -screen 0 1600x1000x24 -nolisten tcp`, passed with
  `--display :137`) and is the set above.

| Check | Result |
| --- | --- |
| `cargo build --workspace --all-targets` | exit 0 on PR D's final tree; 135 warning lines, the count PR C recorded, none at a line Task 12 edited. `Cargo.lock` unchanged by Task 12 (Task 11's one edge is in the same PR) |
| `cargo nextest run -p sirio_ui` | 1421 run, 1421 passed, 0 skipped; against the 1418 of `a10c6cda` the sorted `cargo nextest list` differs by three additions (below) and no removal |
| `cargo nextest run -p sirio` | 531 run, 531 passed; the list is identical |
| `cargo nextest run -p sirio_terminal` | 143 run, 143 passed, 1 skipped; the list is identical |
| `cargo nextest run -p sirio_ui an_overflowing_tab_draws_a_scrollbar_and_asks_for_no_frame_while_idle a_long_markdown_preview_shows_a_scrollbar a_live_thought_opens_while_streaming` | 3 run, 3 passed |
| `Scripts/Tests/test-scroll-drag-probe.sh --xvfb` | `SCROLL DRAG PROBE OK (3 bars)`. Pane 1, now Ely's: the pointer moved 120 px after its second move and the thumb 120 px (expected 120, tolerance 4), and after +2000 px the thumb ends at 600 on a 600 px track. With `SCROLL_DRAG_PROBE_SKEW=40` it fails (`the thumb moved 120 px for 160 px of pointer travel`, exit 1) |
| `Scripts/Tests/test-markdown-scroll-probe.sh --xvfb` | `MARKDOWN SCROLL PROBE OK` (dark and light) |
| `Scripts/Tests/test-editor-e2e.sh --xvfb` | `EDITOR E2E OK (47 scenarios, appearance dark, state-only 0)` |
| `Scripts/Tests/test-ely-chat-ui-e2e.py --display :138 --only composer_keys,reduced_motion,unfocused_spinner,long_history --cpu-ceiling 413.1` | `ELY CHAT UI E2E OK`; `unfocused-spinner CPU 313.3% of one core over 10 s (load average 9.75 9.42 9.80)`, against the base of 329.7% and the ceiling of 413.1% |
| `Scripts/Tests/test-text-selection-e2e.sh` | `TEXT SELECTION E2E OK (33 tests)` |

The first full `-p sirio_ui` run had six failures, all `no state of type ely_gpui_component::theme::Theme
exists`: the six sidebar tests that draw a closed session now draw the trash icon through Ely, so their
hosts call `ely::init` (table above). The three added tests are in `scroll.rs`:
`a_box_that_first_overflows_asks_for_the_frame_that_draws_its_bar` (vertical and horizontal) and
`content_that_grows_or_shrinks_under_a_bar_asks_for_one_more_frame`, both written before the helper they
test, and `a_box_resized_along_its_bars_axis_asks_for_one_more_frame_and_across_it_for_none`, added after
review. The first two were red against a stub that returned an empty element and are red again with the
helper's `request_animation_frame` call taken out. The third resizes a horizontal box with its content
changed by the same amount (the overflow stays 700 px) and then changes only its height; it is red with
the length half of the helper's comparison deleted and red with the helper's `geometry` made to ignore
its axis, each tried alone.

**Every changed frame** (`artifacts/bezel-off/pr-d/diff/changed.tsv`).

| Frame | Pixels | bbox (x,y,w,h) | Allow rows | What |
| --- | --- | --- | --- | --- |
| `surfaces-dark/tab-menu-dark` | 28282 | 178,146,284,282 | `ui::popover`, `ui::surface` | The probe's new-tab menu. Bezel's glass card (12 px radius, a hairline) is Ely's solid panel (smaller radius, a floating shadow, so a wider soft edge). The rows are 28 px where bezel's were 33 px (seven rows, so the card is 35 px shorter), the dividers are inset by the card's padding where bezel's ran border to border, and a disabled row is Ely's disabled colour. The card's width and top-left corner are the same. |
| `surfaces-light/tab-menu-light` | 29429 | 180,148,280,310 | `ui::popover`, `ui::surface` | The same, in light. |

Task 11 captured the same two frames with the same pixel counts and boxes, before PR D was rebased onto
PR C and again after its fix round, and nothing else. `crops/tab-menu-{dark,light}-{before,after,stack}.png` has the before and
after of each. Every other frame (92) is within the tolerance of PR C's.

**Changes no capture shows.** Each is on the list of what Enzo should see.

Menus and popovers (Task 11):

- Every menu other than the tab menu is in no capture: the sidebar's context, close and add-project
  menus, the files context menu, the mention and slash popups (`composer_keys`' four frames are dropped
  from the set), the combobox menu and the turn preview. By hand Task 11 saw the slash popup (a frame of
  the chat E2E), the add-project menu (in `sidebar_demo`) and the worktree-close menu at interface 18.
- Tab-menu text follows the interface size: the rows take Ely's `Sm`, which `ely::init` maps to
  Sirio's `ui_size`, where bezel's rows were a fixed 13 px. At the default nothing changes (13 and 13, and the
  frames agree); at 18 the tab menus' text is 18 px, as the sidebar and files menus already were.
- A long label is cut with an ellipsis inside its card (`menu_label`, with `overflow_hidden` on the card
  as a backstop). At interface 18 "Remove from disk and remote branch" ran 80 px past the 280 px card
  before; `menu-size-18/worktree-close-menu-size-18.png` shows it as "Remove from disk and remot…" inside
  the card. Only that menu was looked at at 18; the tab and files menus use the same `menu_label`.
- Ely's rows are a fixed 28 px against bezel's 29 to 33 px, so every menu shrinks a little; at interface
  sizes above 18 a 28 px row is the tightest.
- The entrance is Ely's opacity fade (its `FAST` duration), without bezel's 2 px drop; a hover snaps where
  bezel's rows faded it in; a disabled row is `fg_disabled` where it was faint text.
- A menu reopened within its exit's ~120 ms snaps to full opacity (Ely's own menus do the same). While a menu
  closes, an overlay keeps its dying rows from taking a click (a test pins it).
- `sirio` no longer depends on `bezel_theme` (`Cargo.lock` loses that edge).

Scrollbars and icons (Task 12):

- The CI log's bar goes to Ely although the spec's inventory listed it among the bars Sirio computes
  itself. Its list hands out a gpui `ScrollHandle`, which Ely's bar reads, and the spec's "left to the
  builder" allows it. The terminal's, the transcript's and the horizontal bars keep `crate::scroll`'s
  maths. The bar of the Markdown preview, the CI log, the trace tab and the thought well, none of them in a
  capture, now looks like this (pane 1 of the scroll probe, `crops/scroll-probe-pane1-{before,after,stack}.png`,
  bezel's from Task 3's run of the same probe):
  - the strip is 12 px where it was 10, and the 6 px thumb sits in its left half, so it is 4 px further from
    the pane's edge (x 288..293 where it was 292..297 in a 300 px pane);
  - the idle thumb is `fg` at 22% (`#3D3D3D` on the probe's `#131313`) where bezel's ink was 20%
    (`#424242`); under the pointer it is 42% where it was 32%, and there is no separate colour while it is
    dragged (bezel's was 38%);
  - the shortest thumb is 24 px where it was 25 px, and the thickness comes from Ely's theme in rems (6 px
    at the default);
  - a drag repaints the whole window (`window.refresh()`) where bezel's asked the owning view to repaint.
- The Markdown preview and the CI log now pair an Ely vertical bar (`fg` at 22%, a 6 px thumb in the left half
  of a 12 px strip) with Sirio's own horizontal bar (`ink(0.2)`, a thumb that fills its 10 px strip along the
  bottom edge), so the two bars of one surface differ in colour, in thickness and in how far they sit from
  the edge.
- The drag still anchors at the first move past gpui's 2 px threshold, as bezel's did and as the spec says:
  the probe reads 120 px of thumb for 120 px of pointer on pane 1.
- The bar's frame request now also fires when the box is resized or its content grows or shrinks while the
  bar is up (bezel's compared the track's length only), and the tool diff's horizontal bar asks for the frame
  that redraws it too (it only asked for its first one). An idle tab asks for none: the trace test still
  reads 0. No committed test draws an overflowing tool diff, so for that bar a throwaway test (not
  committed) drew one under a 600 px line in a 700 px window: the bar is up (624 x 12 px), the first
  round of frames asks for 2 and each of the next five for 0; narrowed to 500 px the bar is redrawn at 424
  px, one round asks for 1 and the next three for 0.
- The helper's cost while a thought streams: a thought pinned to its newest line already asks for each
  frame through `scroll::follow`, so the helper adds nothing there. Once the reader has scrolled up in a
  streaming thought the pin is released, and from then on the helper adds one more Chat render per change
  of the thought's height (the bar needs it to draw the right thumb). No test counts a streaming thought's
  frames.
- The mention row's icon is Lucide's `file-text` where it was Solar's `document`, and the closed chat's delete
  button is `trash-2` where it was `trash-bin-minimalistic`. Neither is in a capture (the mention popup is
  not captured, and the delete button is invisible until the pointer is over a closed row). Ely sizes an icon
  in rems times its `font_scale`, which Sirio leaves at 1.0, so both stay 12 and 14 px at every interface
  size: a throwaway test (not committed) read 12 px and 14 px wide at the default and at 18.
  `crops/icons-document-trash-before-after-x4.png` is a throwaway example (not committed) drawing the old and
  the new glyph side by side at the real sizes, scaled up four times.

**What the masks hide here.** `CAPTURE DIFF OK` says nothing about pixels inside a mask. PR D touches
no element under a standing mask except the one named below, so the same pair was compared with no
`--masks` and the mask table plus this PR's rows as `--allow` (`artifacts/bezel-off/pr-d/diff-unmasked/`):
`CAPTURE DIFF OK (64 changed, all attributed)`. The 2 frames above, and 62 more that differ only inside
mask rectangles whose `why` is a clock or a run-to-run accident: the status bar's path (50 frames, the
`mktemp` directory name) and usage meter (29), message times (34, 10 of them in the identity probe), the
Agents toolbar's `Refreshed HH:MM:SS` stamp (`surfaces-{dark,light}/settings-agents`, 376 and 1936
pixels: the digits and, in light, the search field that the stamp's width moved 3 px sideways), the turn duration, the cancelled
line, the crash addresses, and the session dots (`activity/tools-and-reasoning`). No Ely bar, menu or
icon lies under any of them.

The scrollbar-thumb mask (`scenarios/long-history/frames/scrolled-up*`, x 1133, y 77, 10 x 540) does
not hide a change of this PR: it covers the transcript's own bar, `list_scrollbar`, which PR D leaves
(only its doc comment moved), and the Ely bars are in none of those frames. One of the two frames,
`scrolled-up-after-new-turn`, has 100 changed pixels in it (x 1135..1140, y 495..526, the track's lower
part), the accident the mask names, the same figure and place as in PR C's page.

Enzo's verdict: approved on 2026-10-27, on the page built from these captures.

### PR E: Settings, then the crates out (`refactor/bezel-off-e`)

PR E draws every button, strip, badge and the empty state's icon of the Settings pages with Ely
(Task 13), then deletes bezel's UI crates (Task 14). It changes pixels in 12 of the 94 frames, all of
them Settings pages (Agents, Git hosting accounts, Git hosting and Language servers in three pages,
each in dark and light), attributed in `docs/testing/bezel-off/pr-e-allow.tsv`. Removing the crates
changes none: the set captured after it differs from the set Task 13 ended on by nothing under the masks.

**Base and head.** The "before" set is `artifacts/bezel-off/pr-d/after`, PR D's after set, captured on
the tree of `89bbd118`. PR E's git base is `b4f6de7a`, and `git diff 89bbd118 b4f6de7a -- rust/ Scripts/`
is five files and no drawing: the comments of `scroll_drag_probe.rs` and of `test-scroll-drag-probe.sh`,
an import-only change in `composer_view.rs` and `sidebar/sessions.rs`, and in `scroll.rs` one module-doc
sentence and one test. So the "before" frames are the frames of PR E's base and the base was not
captured again. The "after" set was captured on the tree of `474f35e8`, PR E's last code commit (the
working tree was clean). PR E's code is three commits: the Settings widgets (`10894f80`), their sizes and
variants by role (`39d871e7`: a bordered bezel Ghost is Ely's Outline, every Settings button and the
rewind button are `Sm`) and the removal (`474f35e8`). The commits after the capture change no pixel: this
page, the allow file, one mask and one wait line in the editor E2E (both below).

**What PR E moved.**

| Task | Where | What |
| --- | --- | --- |
| 13 | `settings/scaffold.rs` (new, ported from bezel-ui) | The settings page skeleton (`page_header`, `page_subtitle`, `group_box`, `card_row`, `row_title`, `meta_line`), `card_row_hover`, `status_dot`, and `empty_state`, whose icon is Ely's `Search` at 24 px. Same constants as bezel's. |
| 13 | `settings/agents_page.rs`, `settings/language_servers_page.rs`, `settings/git_hosting_page.rs` | Update, Update All, Install guide and Offer again (bordered bezel Ghost) are Ely `Outline` at `Sm`; Refresh, Forget token and the unselected pickers are `Ghost`; Install and Save token and the selected picker are `Primary`. "Installed" is Ely's Success `Badge`. The install-failure and registry-error strips are Ely `Alert`s (Danger and Warning) with selectable text. |
| 13 | `loading::progress` | Ely's `ProgressBar` (2 px) centred in the old 4 px slot, one id per bar because the bar keys its glide on it. Callers: the clone form and the status bar's usage meter (one id per provider). |
| 13 | `chat/turn_rail.rs` | The turn preview's rewind action is an Ely `Ghost` button at `Sm`. |
| 13 | `ely.rs` | `AppAssets` serves only `ely/` and `sirio-chat/`; bezel's icon fallback is gone (no path was served only by it). |
| 13 | `chat/tool_calls.rs` | `the_selectable_output_well_lays_out_like_bezels` is deleted with the bezel widget it compared against; the text-selection E2E reads 32 tests where it read 33 (its floor is 30). |
| 14 | `rust/Cargo.toml`, `crates/sirio`, `crates/sirio_ui`, `rust/vendor/bezel_ui/` | The `bezel` dependency, the `[patch.crates-io] bezel-ui` override and the vendored crate are deleted. `Cargo.lock` loses `bezel`, `bezel-agent`, `bezel-motion` and `bezel-ui` and the two `bezel` edges: 48 deletions, no addition. |
| 14 | `sirio/src/main.rs`, a chat test, `scroll_drag_probe.rs`, `surfaces_probe.rs` | `init_motion` and the three `set_pause_when_inactive(false)` calls go. They set the pause flag of bezel's clock, which nothing leases any more; `sirio_ui::motion`'s clock has no such flag and keeps turning in an unfocused window, which `unfocused_spinner` checks (below). |
| 14 | `sirio/src/main.rs` | The comment above `with_assets` names both prefixes `AppAssets` serves through `svg().path(..)`, Ely's icons (`ely/…`) and the chat's agent marks (`sirio-chat/…`, `chat/identity.rs`), which is why the asset source stays. |
| 14 | docs | `CLAUDE.md`, `README.md`, `docs/agents/architecture/crates.md`, `rust/vendor/README.md` (the `bezel_ui` section goes), `LOCAL-CHANGES.md` and `THIRD_PARTY_NOTICES.md` (the MIT text stands on its own: it no longer points at the deleted `LICENSE`). |

What still reaches bezel after PR E is the engine and three crates: `bezel-gpui`, `bezel-theme`,
`bezel-markdown` (vendored, two changes) and `bezel-syntax`. `grep -rn 'bezel::' rust/crates` prints nothing.

**Reproduce.**

```bash
# The before set is PR D's after set, already on disk (see above). The after set. On a busy machine start
# your own Xvfb (1600x1000x24, a display number nobody else uses) and pass --display instead of --xvfb.
TMPDIR=$HOME/.cache/sirio-e2e-tmp Scripts/bezel-off-captures.sh --xvfb --out-dir "$PWD/artifacts/bezel-off/pr-e/after"

# The diff against PR D's set, with this PR's attribution file.
Scripts/capture-diff.py artifacts/bezel-off/pr-d/after/frames artifacts/bezel-off/pr-e/after/frames \
  --out "$PWD/artifacts/bezel-off/pr-e/diff" --masks Scripts/capture-masks.tsv \
  --allow docs/testing/bezel-off/pr-e-allow.tsv

# The diff against the set Task 13 ended on (artifacts/bezel-off/pr-e/task-13-r1, captured at 39d871e7):
# what removing the crates changed.
Scripts/capture-diff.py artifacts/bezel-off/pr-e/task-13-r1/frames artifacts/bezel-off/pr-e/after/frames \
  --out "$PWD/artifacts/bezel-off/pr-e/diff-vs-task-13-r1" --masks Scripts/capture-masks.tsv

# The same pairs with no masks, and the masks plus this PR's rows as the allow table.
cat Scripts/capture-masks.tsv docs/testing/bezel-off/pr-e-allow.tsv > artifacts/bezel-off/pr-e/allow-unmasked.tsv
Scripts/capture-diff.py artifacts/bezel-off/pr-d/after/frames artifacts/bezel-off/pr-e/after/frames \
  --out "$PWD/artifacts/bezel-off/pr-e/diff-unmasked" --allow artifacts/bezel-off/pr-e/allow-unmasked.tsv
Scripts/capture-diff.py artifacts/bezel-off/pr-e/task-13-r1/frames artifacts/bezel-off/pr-e/after/frames \
  --out "$PWD/artifacts/bezel-off/pr-e/diff-vs-task-13-r1-unmasked" --allow artifacts/bezel-off/pr-e/allow-unmasked.tsv

# The probes and E2Es this PR's code is held to (Xvfb :N is any free display, 1600x1000x24, with lavapipe:
# VK_ICD_FILENAMES and VK_DRIVER_FILES set to /usr/share/vulkan/icd.d/lvp_icd.json; TMPDIR as above).
Scripts/Tests/test-ely-chat-ui-e2e.py --display :N --cpu-ceiling 413.1 --out-dir "$PWD/artifacts/bezel-off/pr-e/step3/chat-e2e"
Scripts/Tests/test-surfaces-probe.sh --xvfb
Scripts/Tests/test-scroll-drag-probe.sh --xvfb
Scripts/Tests/test-text-selection-e2e.sh
Scripts/Tests/test-editor-e2e.sh --xvfb
Scripts/Tests/test-markdown-selection-probe.sh --xvfb
Scripts/Tests/test-markdown-scroll-probe.sh --xvfb
Scripts/Tests/test-chat-session-panel-e2e.sh --state-only
Scripts/Tests/test-open-list-e2e.sh --display :N
Scripts/Tests/test-theme-sweep.sh --display :N --out-dir "$PWD/artifacts/bezel-off/pr-e/step3/theme-sweep"

(cd rust && cargo build --offline --workspace --all-targets && cargo nextest run -p sirio_ui && cargo nextest run -p sirio \
  && cargo nextest run -p sirio_terminal && cargo nextest run -p sirio_syntax \
  && cargo nextest run -p ely-gpui-component --features test-support)
```

**The result.** `bezel-off-captures.sh` printed `CAPTURES OK: artifacts/bezel-off/pr-e/after (94 frames)`
on its first run (the chat E2E, the sweep, the Changes E2E and both surfaces probes passed inside it). The
masked diff against PR D's set printed `compared 94 frames present on both sides (94 before, 94 after),
tolerance 1` and `CAPTURE DIFF OK (12 changed, all attributed)`. The same set against Task 13's final
head printed `CAPTURE DIFF IDENTICAL`: removing the crates, and `init_motion` with them, moved no pixel
under a mask. The artifacts are under `artifacts/bezel-off/pr-e/` (gitignored): `after/`, `diff/`,
`diff-unmasked/`, `diff-vs-task-13-r1/`, `diff-vs-task-13-r1-unmasked/`, `crops/`, `hand/`, `step3/` (every check's log and out-dir) and
Task 13's own sets and diffs (`task-13*`).

The first masked diff against Task 13's head was not identical: `settings-agents-{dark,light}` differed by 48
and 50 pixels at x 1022..1028, y 65..75, the last digit of the toolbar's `Refreshed HH:MM:SS` stamp (`14:14:55`
in one run and `15:18:11` in the other). That is the stamp the Agents toolbar mask exists for, but the mask
ended at x 1022 and Ely's narrower `Update All` moved the stamp's right edge to x 1028. The mask is widened to
x 1032 (below), which is the only change to `Scripts/capture-masks.tsv`. Nothing else differed.

| Check | Result |
| --- | --- |
| `cargo build --offline --workspace --all-targets` | exit 0 on the removal commit's tree; 135 warning lines, the count PR D recorded, none at a line Task 14 edited |
| `cargo tree --offline -i bezel-ui`, `-i bezel-motion`, `-i bezel-agent`, `-i bezel-editor`, `-i bezel` | each: `package ID specification ... did not match any packages`. `bezel-markdown` and `bezel-syntax` still resolve and take none of them. `cargo tree -e features -p sirio` before and after differs by the lines of the four removed crates and nothing else: no kept crate lost a feature |
| `grep -E '^name = "bezel(-ui\|-motion\|-agent\|-editor)?"$' rust/Cargo.lock`; `grep -rn 'bezel::' rust/crates`; `test ! -e rust/vendor/bezel_ui`; `grep -nE '^(bezel\|bezel-editor) *=' rust/Cargo.toml` | each prints nothing (the `test` exits 0) |
| `git diff rust/Cargo.lock` | 48 deletions, 0 insertions: four `[[package]]` blocks and the two `"bezel"` dependency lines; no version or source changed |
| `cargo nextest run -p sirio_ui` | 1420 run, 1420 passed, 0 skipped; against the base's 1420 the sorted `cargo nextest list` is identical |
| `cargo nextest run -p sirio` | 531 run, 531 passed; the list is identical |
| `cargo nextest run -p sirio_terminal` | 143 run, 143 passed, 1 skipped; the list is identical |
| `cargo nextest run -p sirio_syntax` | 13 run, 13 passed; the list is identical |
| `cargo nextest run -p ely-gpui-component --features test-support` | 144 run, 144 passed; the list is identical |
| `Scripts/Tests/test-ely-chat-ui-e2e.py --display :151 --cpu-ceiling 413.1` (no `--only`) | `ELY CHAT UI E2E OK`, with the effort scenarios, the identity lane and `unfocused_spinner` in it; `unfocused-spinner CPU 318.4% of one core over 10 s (load average 7.88 12.29 12.71)`, against PR D's 313.3%, the base of 329.7% and the ceiling of 413.1%, and the bloom's pixels differ between frames 1-2, 2-3 and 1-3 (`[132, 108, 135]`): it turns in an unfocused window with `init_motion` gone |
| `Scripts/Tests/test-surfaces-probe.sh --xvfb` | `SURFACES PROBE OK (34 frames, 3 gaps)` |
| `Scripts/Tests/test-scroll-drag-probe.sh --xvfb` | `SCROLL DRAG PROBE OK (3 bars)`: each bar's thumb moved 120 px for 120 px of pointer (tolerance 4) and ends flush after +2000 px |
| `Scripts/Tests/test-text-selection-e2e.sh` | `TEXT SELECTION E2E OK (32 tests; ...)` (its floor is 30; 33 before Task 13 deleted one test) |
| `Scripts/Tests/test-markdown-selection-probe.sh --xvfb`, `test-markdown-scroll-probe.sh --xvfb` | `MARKDOWN SELECTION PROBE OK`, `MARKDOWN SCROLL PROBE OK` |
| `Scripts/Tests/test-chat-session-panel-e2e.sh --state-only` | `CHAT SESSION PANEL E2E OK (14 scenarios, 7 skipped, appearance default, state-only 1, window default)` |
| `Scripts/Tests/test-open-list-e2e.sh --display :151` | `OPEN LIST E2E OK` |
| `Scripts/Tests/test-theme-sweep.sh --display :151 --out-dir <checkout>/artifacts/bezel-off/pr-e/step3/theme-sweep` | `THEME SWEEP OK: <checkout>/artifacts/bezel-off/pr-e/step3/theme-sweep` |
| `Scripts/Tests/test-editor-e2e.sh --xvfb` | `EDITOR E2E OK (47 scenarios, appearance dark, state-only 0)`, with the script's one-line wait that PR E adds (below). Without it the run was red: `FAIL: git_marks: git_marks expected ...,30:added,41:added, got ...,30:added,40:modified,41:modified`, twice on this tree and once each on `39d871e7` and on `b4f6de7a` (PR D's head; PR D's page records `EDITOR E2E OK (47 scenarios ...)` on PR D's tree), so no code commit of this PR causes it. It passed alone (`--only git_marks`) and after most scenarios, and failed after `context_rows`. Cause, from a debug print of the state before the assertion: the script measured the end of the adopted text from a state whose `git_marks` already follow the disk but whose `text` is still the buffer it replaces (`end=328`, the adopted text is 331 bytes), so `appended\n` landed 3 bytes early, in `line ` / `40` (the disk ended `line 39\nline appended\n40\n`). The marks come from `git diff` of the disk and lead the buffer's adoption by a beat; the user sees the adopted text either way. The added line, `expect_state text "$(escaped "$RUN_DIR/marks-adopt.txt")"` after the marks check, waits for the adopted text before measuring its end |

Every probe and E2E ran one at a time on a private Xvfb (`:151`, `1600x1000x24`, lavapipe), never beside
nextest or the capture run. The editor E2E's runs and the debug print are under `artifacts/bezel-off/pr-e/step3/`.

**Every changed frame** (`artifacts/bezel-off/pr-e/diff/changed.tsv`).

| Frame | Pixels dark / light | bbox (x,y,w,h) | Allow rows | What |
| --- | --- | --- | --- | --- |
| `settings-agents` | 111390 / 111277 | 475,56,720,639 | `widgets::Buttons::button`, `widgets::Status`, `widgets::Content` | Update All, Refresh, Update and Install are Ely buttons (Outline, Ghost, Primary at `Sm`; Ghost has no border); "Installed" is Ely's Success badge, larger and bolder; the registry strip and the install-failure strip are Ely alerts, taller, without a border and with Ely's severity icon. The registry strip is 6 px taller (47 px against bezel's 41), so the card and every row below it sit 6 px lower and the header reflows; row heights are within 1 px of PR D's. |
| `settings-git-hosting-accounts` | 50496 / 50472 | 475,224,720,305 | `widgets::Buttons::button` | The forge and means pickers, Save token and Forget token are Ely buttons at `Sm`, so the rows are 5 and 14 px shorter. The GitLab row is still one line (GitHub, GitLab, Detect, CLI, Token, the field and Save token); the scope line and Forget token share the line below it, as in PR D. |
| `settings-git-hosting` | 253 / 259 | 824,194,22,22 | `ui::icons` | The empty state's icon: Lucide's search at 24 px where it was Solar's magnifier. Nothing else on the page moved. |
| `settings-language-servers` | 58922 / 58880 | 476,151,718,682 | `widgets::Buttons::button`, `widgets::Status` | Install (Primary), Install guide and Offer again (Outline) at `Sm`; the clangd install-failure strip is an Ely alert, 1 px taller. |
| `settings-language-servers-p2` | 52278 / 52190 | 476,41,718,779 | `widgets::Buttons::button` | The same buttons, further down the list. |
| `settings-language-servers-p3` | 25054 / 25054 | 1015,41,159,759 | `widgets::Buttons::button` | The same buttons, the tail of the list. |

`crops/<frame>-{dark,light}-{before,after,stack}.png` has the changed box of each frame from PR D's set and from
this one, and the two side by side (`settings-git-hosting`'s 22 px box is enlarged eight times). Every
other frame (82) is within the tolerance of PR D's. They are the same 12 frames, with the same boxes, as at
Task 13's final head (`task-13-r1`); the pixel counts differ from it only by the stamp. (Task 13's first round, before
the buttons were sized by role, had larger boxes for the accounts and the third Language servers page.) The allow file's
boxes are read from this run's `changed.tsv`.

**Changes no capture shows.** Each is on the list of what Enzo should see.

- **The status bar's usage meter.** It is in every main-window frame and in none of them: a standing mask (x 76..376,
  y 802..824) blinds it, because it reads the user's live account. It is now Ely's 2 px `fg` on `border`, centred in the
  4 px slot of an 18 px meter, gliding between values; bezel's was 4 px of `text` on `ink(0.12)`.
- **The clone form's progress bar** (`project_forms.rs`) is the same function in a 280 px slot, and **the turn
  preview's rewind button** (`chat/turn_rail.rs`) is an Ely Ghost at `Sm` in a 280 px card. No frame, probe or test
  draws either.
- **Renders of the three above**, after only, are described below.
- **Every Settings button is a Tab stop** (Ely gives each `tab_index(0)`); no check asserts the order.
- **The registry-error text is selectable now** (it was a plain string); the strips' wrappers set the text colour
  because `Alert::content` is host-owned and the first capture drew the message black on the tint.
- **Outline's border** on the four bordered buttons is Ely's `border_strong` at one pixel, not bezel's `border`.
- **The unfocused window.** `init_motion` is gone. What it set (bezel's pause flag) only ever governed the clock
  that bezel's scrollbars and Settings widgets leased, and none of them is left; the sidebar bloom and the orb draw on
  `sirio_ui::motion`, which never pauses. `unfocused_spinner` is the automated witness (318.4%, bloom moving, above) and
  a capture cannot be one, because `SIRIO_REDUCED_MOTION=1` snaps every animation to rest. The thinking orb in an
  unfocused window is on the macOS list for that reason.

**What the masks hide here.** `CAPTURE DIFF OK` says nothing about pixels inside a mask. PR E touches no element
under a standing mask except the two named below, so the same pair was compared with no `--masks` and the mask
table plus this PR's rows as `--allow` (`artifacts/bezel-off/pr-e/diff-unmasked/`): `CAPTURE DIFF OK (72 changed,
all attributed)`. The 12 frames above, and 60 more that differ only inside mask rectangles whose `why` is a clock
or a run-to-run accident: the status bar's path and usage meter, the identity probe's message time and the Agents
stamp. Against Task 13's head (`diff-vs-task-13-r1-unmasked/`) 62 frames differ, all inside those masks.

The two it does touch: the status bar's usage-meter mask covers the meter whose bar PR E redraws, so no frame can show it
(the renders below do), and the Agents toolbar's stamp mask lies next to the buttons PR E restyled. The crops of the stamp's
neighbourhood in both runs show nothing but the digits differing under it; no Ely button or alert lies inside the mask.

**The mask that moved.** `Scripts/capture-masks.tsv`'s Agents toolbar row was `668 54 354 40` (x 668..1022) and is
`668 54 364 40` (x 668..1032). In PR D's set the stamp ended near x 1016 and bezel's `Update All` began at 1028; Ely's
`Update All` is narrower and begins at 1038, so the right-aligned toolbar moved the stamp out to x 1028 and its last digit,
which is the clock, fell outside the old rectangle. The new right edge is 6 px short of the button's border. The "Masks in
force" table says so.

**Hand renders of the three surfaces no frame shows.** After only, and not committed.
A throwaway example (`hand_probe`, deleted afterwards) drew them in a 900 x 520 window on a private Xvfb with
lavapipe, in dark and light, with `SIRIO_REDUCED_MOTION=1`. `loading::progress` is the app's own function. The status-bar
segment around it, the clone form's progress block and the preview card around the rewind button are replicas of their call
sites, built from the same public pieces (`menu_card`, Ely's `Button`, the same slot widths), not the surfaces themselves,
because the real ones need a live account, a running clone and a native Claude chat. The files are
`artifacts/bezel-off/pr-e/hand/{usage-meter,clone-progress,rewind-button}-{dark,light}.png` (enlarged three times) and
`window-{dark,light}.png`. What they show:

- The meter's bar is 2 px high in its 18 px slot, drawn in `fg` on a `border` track. The track is faint, so a meter at 12% reads
  as a 2 px dot with a hairline after it, and a stale meter (the same bar at 0.55 opacity) is dimmer again. Whether that reads as
  a meter at a glance is Enzo's call on the Mac.
- The clone bar is the same bar at 280 px (the slot's maximum), centred in its column: an empty track at 0%, a bright bar at 40%
  and 100%.
- The rewind button at `Sm` has no border or fill: it reads as a medium-weight label ("Restore files to this message") at the
  foot of the card, centred in it. Its hover and its click are not shown.
- The glide between two values is not shown: reduced motion snaps it. It is on the Mac list.

**macOS hand-check list.** One check on a build of this branch, with translucency on and off, by Enzo.

*Fields and forms*

- The chat composer (typing, selection, paste, Enter and Shift+Enter, the history keys, the slash and `@` popups) and the
  model search field.
- The clone-repository and create-project forms, the browser's address bar and the worktree picker's search field: each is
  an Ely framed field now.
- A framed text field at interface size 18 (Settings, then the interface size): the frame, the caret and the text still
  line up.

*Menus, popups and tooltips*

- A tab context menu, the files context menu and the sidebar context menu, at the default interface size and at 18 (the
  rows take Ely's `Sm`; a long label ends in an ellipsis inside its card).
- The sidebar's worktree close menu, the `@` mention popup, the slash popup, the worktree combobox's menu and the turn preview
  (hover a tick of the turn rail).
- A tooltip.
- The "Hide Pane" close button's hover (no frame proves it).

*Bars and logs*

- The CI log and the file view bars: Ely's vertical bar and Sirio's own horizontal bar side by side, at the edges of a log
  and a Markdown preview.
- The thinking orb in an unfocused window: it keeps turning while another window holds the focus.

*Settings*

- Settings, then Agents, Git hosting and Language servers: the Outline, Ghost and Primary buttons, the "Installed" badge, the
  two alerts, the empty state's search icon, the rows' height and where the Git hosting line wraps.
- Keyboard: press Tab through the buttons of those three pages. Every Settings button is a Tab stop now; the order should read
  top to bottom and each button should show Ely's focus ring.

*What no frame shows*

- The status-bar usage meter: a 2 px bar in an 18 px slot, centred in its 4 px height, gliding when the value changes, and
  dimmed when the numbers are stale.
- The clone progress bar while a real clone runs.
- The rewind button in the turn preview: its look at `Sm`, a hover, and a click that asks for the restore preview.

Enzo's macOS verdict: pending.
