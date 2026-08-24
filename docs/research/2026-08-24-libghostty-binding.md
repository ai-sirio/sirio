# libghostty-vt: published crate, or our own FFI over a vendored Ghostty tree?

This document answers one question — **does the published `libghostty-vt` 0.2.1 crate expose everything a direct cell renderer needs, or does Tiller need its own FFI over a vendored Ghostty tree?** — for the migration charted in [#27](https://github.com/tillerai/tiller/issues/27). It is a record of *facts with sources*, not a recommendation: the crate-vs-vendor call is a separate decision for a human to make against these facts. Facts were gathered on **2026-08-24**. `libghostty-vt`'s own C header says the API "is not yet stable and is definitely going to change", so re-verify before committing to any pin.

## Source discipline used here

Only primary sources: the published `.crate` tarballs for `libghostty-vt` 0.2.1 and `libghostty-vt-sys` 0.2.1 downloaded from `static.crates.io`, the `Uzaaft/libghostty-rs` git history at the exact release commit, the `ghostty-org/ghostty` tree at the commit `build.rs` pins, herdr's own vendored patch files and CI workflows, `Xuanwo/gpui-ghostty`'s own sources, the crates.io and GitHub Actions APIs. No blog posts, no third-party summaries.

**One methodological note that turns out to matter a great deal.** The claims recorded while charting #27 — "requires Zig 0.16.x", "README documents Linux and macOS only" — were read from the *repository's current `master`*. The published 0.2.1 artifact is a different tree. Everything below distinguishes explicitly between:

- **0.2.1 as published** — the `.crate` tarballs, and the repo at commit `46a9d2ac941ed600cf43c5e6299c8dfd1d3a1ef0`, which `.cargo_vcs_info.json` inside the tarball names as the source revision;
- **`master` today** — which has moved past 0.2.1 and has different requirements.

Where the two disagree, both are given. Where something could not be established, it is listed in Part 6 rather than filled in with plausible reasoning.

### Version anchors used throughout

| Thing | Value | Source |
| --- | --- | --- |
| `libghostty-vt` 0.2.1 published | 2026-07-18T18:04:40Z | `https://crates.io/api/v1/crates/libghostty-vt` |
| 0.2.1 source revision | `46a9d2ac941ed600cf43c5e6299c8dfd1d3a1ef0` ("release: 0.2.1", 2026-07-15) | `.cargo_vcs_info.json` in `libghostty-vt-0.2.1.crate` |
| Ghostty commit 0.2.1 builds | `a887df42c56f6de86c0fe6da9c4eeca37931e083` (2026-07-11) | `libghostty-vt-sys-0.2.1/build.rs:7` |
| Ghostty version at that commit | `1.3.2-dev` | `ghostty/build.zig.zon` @ `a887df42` |
| Zig required by that Ghostty | **0.15.2** | `ghostty/build.zig.zon` @ `a887df42`, `.minimum_zig_version` |
| Earlier releases | 0.1.0 & 0.1.1 (2026-03-28), 0.2.0 (2026-06-16) | crates.io API |

---

# Part 1 — What `libghostty-vt` 0.2.1 actually exposes

## 1.1 Shape of the crate

The published tarball contains 21 Rust source files, 9,200 lines, and **no `build.rs`** — the native build lives entirely in the companion `libghostty-vt-sys` crate, pinned as an exact `version = "0.2.1"` dependency (`libghostty-vt-0.2.1/Cargo.toml`). The safe crate re-exports the raw bindings wholesale:

```rust
pub use libghostty_vt_sys as ffi;
```
— `libghostty-vt-0.2.1/src/lib.rs:90`

That single line matters for the whole crate-vs-vendor question and is returned to in §1.8: anything the C API has and the safe wrapper lacks is still callable from Tiller through `libghostty_vt::ffi::*` with `unsafe`, without forking anything.

Public modules (`src/lib.rs:93-111`): `terminal`, `alloc`, `build_info`, `error`, `fmt`, `focus`, `key`, `kitty`, `log`, `mouse`, `osc`, `paste`, `render`, `screen`, `selection`, `sgr`, `style`, `unicode`.

The crate's own doc comment states the thread-safety position that #27 already recorded, and states *why* it is not going to change soon:

> "In particular, all `libghostty-vt` types are `!Send`, meaning they cannot be *transferred* across threads, since the C API is allowed to use thread-local state; they are also `!Sync` […] We currently do not expect to lift these limitations unless the C API starts to make stronger guarantees regarding thread safety."
> — `src/lib.rs:59-66`

It also states a suggested architecture that is relevant to Tiller's redesign: "we encourage you to create the terminal on a separate thread (or task in async programming), and use [channels](std::sync::mpsc::channel) to communicate" (`src/lib.rs:68-71`). This is a binding-author opinion, not a guarantee — it is quoted because it is first-party and because #27 lists the concurrency redesign as an open constraint.

## 1.2 Two distinct read paths, and which one is for rendering

This is the single most important structural fact about the API and it is easy to miss.

**Path A — the render-state path.** `RenderState::update(&terminal) -> Snapshot`, then `RowIterator`, then `CellIterator`. This is what a direct cell renderer is meant to use.

**Path B — the grid-ref path.** `Terminal::grid_ref(point) -> GridRef`, with `GridRef::cell/row/style/graphemes/hyperlink_uri`. The crate is explicit that this is *not* the render path:

> "This API is not meant to be used as the core of render loop. It isn't built to sustain the framerates needed for rendering large screens. Use the render state API for that."
> — `src/screen.rs:41-43`

Path B references are also extremely short-lived: "A grid reference is only valid until the next update to the terminal instance. There is no guarantee that a grid reference will remain valid after ANY operation, even if a seemingly unrelated part of the grid is changed" (`src/screen.rs:35-39`). A `TrackedGridRef` (`src/screen.rs:163`) exists for long-lived anchors — selections, search state, marks — and follows its cell across scroll, scrollback pruning and resize/reflow, but its own docs say "Each tracked reference adds bookkeeping to terminal mutations. Use them sparingly" (`src/screen.rs:159-161`).

The reason to hold on to this distinction: **exactly one thing a renderer might want is available on Path B and not on Path A** (§1.4).

## 1.3 Cell contents and attributes — present, and complete

Per-cell, on the render path (`src/render.rs`, `impl CellIteration`):

| Need | API | Line |
| --- | --- | --- |
| Grapheme codepoints (allocating) | `graphemes() -> Result<Vec<char>>` | `render.rs:804` |
| Grapheme count | `graphemes_len() -> Result<usize>` | `render.rs:814` |
| Graphemes into caller buffer | `graphemes_buf(&mut [char])` | `render.rs:822` |
| Graphemes as UTF-8 into a `String` | `graphemes_utf8(&mut String)` | `render.rs:840` |
| Full style | `style() -> Result<Style>` | `render.rs:753` |
| Resolved foreground RGB | `fg_color() -> Result<Option<RgbColor>>` | `render.rs:774` |
| Resolved background RGB | `bg_color() -> Result<Option<RgbColor>>` | `render.rs:792` |
| Cheap "is anything styled?" gate | `has_styling() -> Result<bool>` | `render.rs:919` |
| Selection membership | `is_selected() -> Result<bool>` | `render.rs:910` |
| Raw cell escape hatch | `raw_cell() -> Result<Cell>` | `render.rs:748` |
| Random access within a row | `select(x: u16)` | `render.rs:728` |

`fg_color`/`bg_color` do the palette resolution for you. `bg_color`'s doc says it "Flattens the three possible sources: [`Cell::bg_color_rgb`], [`Cell::bg_color_palette`] (looked up in the palette), or the style's [`bg_color`]" (`render.rs:783-786`), and `fg_color` notes "Bold color handling is not applied; the caller should handle bold styling separately" (`render.rs:769-770`).

`Style` (`src/style.rs:31-43`) carries **eleven** attributes plus three colors:

```rust
pub struct Style {
    pub fg_color: StyleColor,
    pub bg_color: StyleColor,
    pub underline_color: StyleColor,
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub blink: bool,
    pub inverse: bool,
    pub invisible: bool,
    pub strikethrough: bool,
    pub overline: bool,
    pub underline: Underline,
}
```

`Underline` (`style.rs:122-129`) is `None | Single | Double | Curly | Dotted | Dashed`. For calibration against #27's inventory of the current seam: alacritty exposes 14 cell flags of which Tiller's paint loop consumes three and hardcodes `underline: None, strikethrough: None`. This struct hands over all of it as plain fields, already resolved.

Via `raw_cell() -> screen::Cell`, per-cell (`src/screen.rs:336-398`): `codepoint`, `content_tag`, `wide`, `has_text`, `has_styling`, `style_id`, `has_hyperlink`, `is_protected`, `semantic_content` (OSC 133 output/input/prompt), `bg_color_palette`, `bg_color_rgb`.

`CellWide` (`screen.rs:435-444`) is `Narrow | Wide | SpacerTail | SpacerHead`, with `SpacerTail` documented "Do not render" and `SpacerHead` as "Spacer at end of soft-wrapped line for a wide character" — i.e. the wide-char bookkeeping Tiller currently derives from alacritty's `WIDE_CHAR_SPACER` flag, but with the soft-wrap case distinguished.

Per-row, on the render path: `dirty()`, `raw_row() -> screen::Row`, `set_dirty(bool)`, `selection() -> Result<Option<RowSelection>>` (`render.rs:626-641`). Via `raw_row()` (`screen.rs:284-327`): `is_wrapped`, `is_wrap_continuation`, `has_grapheme_cluster`, `is_styled`, `has_hyperlink`, `semantic_prompt`, `has_kitty_virtual_placeholder`, `is_dirty`.

Per-frame (`render.rs:444-527`): `dirty()`, `cols()`, `rows()`, `cursor_color()`, `cursor_visible()`, `cursor_blinking()`, `cursor_password_input()`, `cursor_visual_style()`, `cursor_viewport()`, `colors()`, `set_dirty()`.

**Assessment: for cell contents and attributes, nothing a direct cell renderer needs is missing.**

## 1.4 Grapheme clusters — present; hyperlinks — present, but not on the render path

**Grapheme clusters: fully present.** Four accessors on the render path (§1.3), a per-row `has_grapheme_cluster()` fast-path, a `CellContentTag::CodepointGrapheme` discriminant (`screen.rs:423`), and a whole `unicode` module documented as modelling "mode 2027, grapheme clustering, enabled […] so callers can predict column layout that exactly matches what" the terminal did (`src/unicode.rs:27, 42`).

**Hyperlinks: a real, precise gap.** The situation is:

- **Presence** is on the render path: `Cell::has_hyperlink()` (`screen.rs:373`) via `raw_cell()`, and `Row::has_hyperlink()` (`screen.rs:311`, "may have false positives") via `raw_row()`.
- **The URI itself is not.** The only accessor is `GridRef::hyperlink_uri(&self, buf: &mut [u8]) -> Result<usize>` (`src/screen.rs:116`) — Path B, the path the crate's own docs say is "not meant to be used as the core of render loop".

This is not a Rust-binding omission that a hand-rolled FFI would fix. The C API's own render-state cell-data enum has no hyperlink kind. `RenderStateRowCellsData` (`libghostty-vt-sys-0.2.1/src/bindings.rs:3011-3036`) has exactly nine kinds:

`RAW`, `STYLE`, `GRAPHEMES_LEN`, `GRAPHEMES_BUF`, `BG_COLOR`, `FG_COLOR`, `SELECTED`, `HAS_STYLING`, `GRAPHEMES_UTF8`.

and `RenderStateRowData` (`bindings.rs:2814-2829`) has four: `DIRTY`, `RAW`, `CELLS`, `SELECTION`. Neither carries a URI.

**Consequence for Tiller.** Reading OSC 8 URIs (which #27 notes `link_router.rs` currently fakes with an `https://` regex) means, at this Ghostty pin: detect `has_hyperlink` on the render path, then step out to `Terminal::grid_ref(point)` → `GridRef::hyperlink_uri()` for the cells that have one. That is workable — hyperlinked cells are rare, and the lookup is on hover/click rather than per frame — but it is a second traversal against an API explicitly disclaimed for render-loop use. **Vendoring does not change this; only patching Ghostty would**, which is precisely herdr's model (Part 2).

## 1.5 `KeyEncoder` / `MouseEncoder` — both present

`key::Encoder` (`src/key.rs:27`): `encode`, `encode_to_vec`, and eight option setters — `set_cursor_key_application` (DEC 1), `set_keypad_key_application` (DEC 66), `set_ignore_keypad_with_numlock` (DEC 1035), `set_alt_esc_prefix` (DEC 1036), **`set_modify_other_keys_state_2`** (`key.rs:181`), `set_kitty_flags`, `set_macos_option_as_alt`, `set_backarrow_key_mode`.

The one that matters most for wiring into a real app is `set_options_from_terminal` (`key.rs:136`), whose doc reads:

> "Reads the terminal's current modes and flags and applies them to the encoder's options. This sets cursor key application mode, keypad mode, alt escape prefix, modifyOtherKeys state, and Kitty keyboard protocol flags from the terminal state.
> Note that the `macos_option_as_alt` option cannot be determined from terminal state and is reset to [`OptionAsAlt::False`] by this call."
> — `src/key.rs:126-134`

`key::Event` (`key.rs:228`) carries action, key, mods, consumed mods, composing state, UTF-8 text, and unshifted codepoint. `KittyKeyFlags` is a bitflags type (`key.rs:643`), and `Terminal::kitty_keyboard_flags()` (`terminal.rs:588`) reads the live flags — which is the kitty keyboard protocol #27 records as never being consumed today.

`mouse::Encoder` (`src/mouse.rs:32`): `encode`, `encode_to_vec`, `set_options_from_terminal` (`mouse.rs:128`), `set_tracking_mode`, `set_format`, `set_size`, `set_any_button_pressed`, `set_track_last_cell`, `reset`. `Format` and `TrackingMode` enums cover the mouse modes; `Terminal::is_mouse_tracking()` (`terminal.rs:609`) reports whether the app wants mouse events at all.

A `focus` module (`src/focus.rs`) encodes focus-in/out events, and `paste` (`src/paste.rs`) validates paste safety — both things Tiller currently hand-rolls or omits.

**Assessment: nothing missing.**

## 1.6 Formatters — all three present

`fmt::Formatter` (`src/fmt.rs:18`) with `Format::{Plain, Vt, Html}` (`fmt.rs:257-266`): "Plain text (no escape sequences)", "VT sequences preserving colors, styles, URLs, etc.", "HTML with inline styles".

`FormatterOptions` (`fmt.rs:24`) offers `with_format`, `with_unwrap` ("unwrap soft-wrapped lines"), `with_trim`, `with_selection` (restrict output to a range; default is the whole screen), plus emit toggles for palette (OSC 4), modes, scrolling region, tabstops, pwd (OSC 7), keyboard modes, cursor (CUP), style (SGR), **hyperlink (OSC 8)**, protection (DECSCA), kitty keyboard, charsets.

Output comes back three ways: `format_alloc` (allocated `Bytes`), `format_buf` (caller buffer, with `Err(OutOfSpace { required })` retry protocol), and `format_len` (size query) — `fmt.rs:178-243`.

Note the asymmetry with §1.4: the **VT formatter can emit OSC 8 hyperlinks** (`with_hyperlink`, `fmt.rs:115`) even though the render path cannot read a URI. So `capture_scrollback` and the Layer-C content signal (#27's open item) are well served, whereas per-cell link routing is not.

`selection` additionally has `format_selection_alloc` / `format_selection_buf` (`src/selection.rs:367, 404`) for formatting just a selection.

## 1.7 Selection and Kitty graphics — both present

**Selection** (`src/selection.rs`, 644 lines, plus a 660-line `selection/gesture.rs`). `Selection::new(start, end, rectangle)` (`selection.rs:56`) — rectangular selection is a constructor flag. Terminal-side: `set_selection`, `select_all`, `select_line`, `select_output`, `select_word`, `select_word_between` (`selection.rs:210-334`). Query: `contains`, `equals`, `order`, `to_ordered`, `adjust`. `SelectLineOptions` takes custom whitespace and an optional `with_semantic_prompt_boundary`; `SelectWordOptions` takes custom boundary codepoints.

`select_word_between` exists specifically for double-click-drag, and its C doc explains a UI problem most terminals get wrong: "If a user double-clicks one word and drags across spaces or punctuation toward another word, selecting only the word directly under the current pointer can flicker or collapse when the pointer is between words" (`bindings.rs`, doc for `ghostty_terminal_select_word_between`).

On the render path, selection is available per cell (`is_selected`, `render.rs:910`) and per row as a range (`RowIteration::selection`, `render.rs:641`), and both the Rust doc and the C doc explicitly steer renderers to the row-range form: "Renderers that can draw cells in spans may be more efficient calling [`RowIteration::selection`] once per row and applying that range directly, avoiding one C API call per cell for selection state" (`render.rs:905-907`).

**Kitty graphics** (`src/kitty/graphics.rs`, 969 lines, behind the `kitty-graphics` feature which is **on by default** — `Cargo.toml.orig`, `default = ["kitty-graphics"]`). Terminal-side policy: storage limit, and separate allow-flags for file / temp-file / shared-memory image sources (`graphics.rs:256-310`). Image query: `id`, `number`, `width`, `height`, `format`, `compression`, `data() -> &[u8]`, `generation()` (`graphics.rs:376-417`). Placements: `PlacementIterator` (`graphics.rs:229`) with `pixel_size`, `grid_size`, `viewport_pos`, `source_rect`, `rect`, `placement_render_info`, `set_layer`, and per-placement `image_id`, `placement_id`, `is_virtual`, `x_offset`, `y_offset`, `source_x/y/width/height`, `columns`, `rows`, `z` (`graphics.rs:516-718`). A pluggable PNG decoder is installed via `set_png_decoder` (`graphics.rs:828`) with a `RustPngDecoder` supplied behind the optional `png` feature.

This is more than a "the protocol exists" claim — it is the full set of geometry a GPUI paint loop would need to place an image quad. #27 lists Kitty graphics in the paint loop as unspecifiable until the render boundary settles; the data side is not the blocker.

## 1.8 What is in the C API but not safely wrapped

Mechanically: `libghostty-vt-sys-0.2.1/src/bindings.rs` declares **173** `ghostty_*` entry points; the safe crate calls **155** of them. The 18 unwrapped ones are:

```
ghostty_cell_get_multi
ghostty_color_rgb_get
ghostty_key_event_get_utf8
ghostty_kitty_graphics_image_get_multi
ghostty_kitty_graphics_placement_get_multi
ghostty_mode_report_encode
ghostty_render_state_get_multi
ghostty_render_state_row_cells_get_multi
ghostty_render_state_row_get_multi
ghostty_row_get_multi
ghostty_selection_gesture_get_multi
ghostty_sgr_attribute_tag
ghostty_sgr_attribute_value
ghostty_sgr_unknown_full
ghostty_sgr_unknown_partial
ghostty_size_report_encode
ghostty_terminal_get_multi
ghostty_type_json
```

Nine of the eighteen are `*_get_multi`, and **this is the one omission with direct performance consequences for a cell renderer.** The C doc:

> "Get multiple data fields from the current cell in a single call. Each element in the keys array specifies a data kind, and the corresponding element in the values array receives the result."
> — `bindings.rs:3057`, doc for `ghostty_render_state_row_cells_get_multi`

The safe `CellIteration` does not use it. Every accessor in §1.3 is a separate `ghostty_render_state_row_cells_get` call (`render.rs:734-745`), so a renderer wanting style + fg + bg + graphemes + selection pays five C calls per cell per frame instead of one.

**But this does not argue for a fork.** Because of `pub use libghostty_vt_sys as ffi;` (`src/lib.rs:90`), `ghostty_render_state_row_cells_get_multi` is callable from Tiller today as `libghostty_vt::ffi::ghostty_render_state_row_cells_get_multi(...)` inside an `unsafe` block, against the same iterator handle the safe API hands out. The cost is a small `unsafe` shim in `tiller_terminal`, not a vendored Ghostty tree. (Whether the batched call is actually faster in practice was **not measured** — see Part 6.)

The other nine are peripheral to rendering: `mode_report_encode` / `size_report_encode` (reply encoders), the four `sgr_*` introspection helpers, `type_json` (type introspection), `color_rgb_get`, and `key_event_get_utf8`.

## 1.9 What is absent from the C API entirely

- **No PTY.** Already established in #27; confirmed — nothing in the 173 entry points spawns or reads a process.
- **No text search.** Grepping the full bindings for "search" returns only word-*selection* helpers and prose. A scrollback find feature would have to be built on the formatters or on grid traversal.
- **No hyperlink URI on the render path** (§1.4).
- **No `Send`/`Sync`** (§1.1).

## 1.10 First-party statement of instability

Worth quoting because it applies equally to *both* routes under consideration:

> "WARNING: This is an incomplete, work-in-progress API. It is not yet stable and is definitely going to change."
> — `ghostty/include/ghostty/vt.h`, lines 10-11, at commit `a887df42`

and the binding's own:

> "This library is currently in development and the API is not yet stable. Breaking changes are expected in future versions. Use with caution in production code."
> — `libghostty-vt-0.2.1/src/lib.rs:13-15`

The `libghostty-vt-sys` README repeats it in build terms: "libghostty-vt is pre-1.0, so these bindings do not guarantee compatibility with arbitrary installed C API revisions."

---

# Part 2 — herdr's two patches, and whether 0.2.1 can reach them unpatched

## 2.0 The two trees are almost the same vintage

This turns out to be the enabling fact for the whole of Part 2. herdr does not vendor an old or a forked Ghostty; it vendors a tree four days newer than the one the crate pins.

| | `libghostty-vt` 0.2.1 | herdr `master` |
| --- | --- | --- |
| Ghostty source commit | `a887df42c56f6de86c0fe6da9c4eeca37931e083` | `c5a21edfcbc2d5b46540ad91b7980aca31f5f1f3` |
| Commit date | 2026-07-11 | 2026-07-15 |
| `build.zig.zon` `.version` | `1.3.2-dev` | `1.3.2-dev` |
| `.minimum_zig_version` | **`0.15.2`** | **`0.15.2`** |
| Where recorded | `libghostty-vt-sys-0.2.1/build.rs:7` | [`vendor/libghostty-vt.vendor.json`](https://github.com/herdrdev/herdr/blob/master/vendor/libghostty-vt.vendor.json) (`source_commit`), [`vendor/libghostty-vt/VERSION`](https://github.com/herdrdev/herdr/blob/master/vendor/libghostty-vt/VERSION) = `1.3.2-HEAD-+c5a21edfc` |

A corroborating detail: the last member of the C `GhosttyTerminalData` enum in 0.2.1's bindings is `VIEWPORT_ACTIVE = 32` (`libghostty-vt-sys-0.2.1/src/bindings.rs`), and herdr's patch 0002 appends `modify_other_keys = 33` immediately after `viewport_active = 32`. The two C API revisions line up member for member.

**So the patches are not compensating for the crate being behind. They are compensating for gaps that exist in Ghostty's C API at both pins.** The right question is therefore not "can the crate catch up?" but "does Tiller need what these patches provide?"

## 2.1 herdr's own record of why the patches exist

[`vendor/libghostty-vt.patches.md`](https://github.com/herdrdev/herdr/blob/master/vendor/libghostty-vt.patches.md) is an index with a fixed schema per patch: status, patch file, herdr issue, upstream discussion, upstream PR, vendored base, local files, reason, removal condition, verification commands. Two entries, both `status: active`.

**Both entries record `upstream discussion: not opened` and `upstream pr: not opened`.** Neither patch has been proposed to Ghostty. That is herdr's own statement, and it means these are not "fixes in flight" that a crate consumer can wait out.

The maintenance policy lives in herdr's [`AGENTS.md`](https://github.com/herdrdev/herdr/blob/master/AGENTS.md):

> "When updating libghostty-vt, check every active patch in `vendor/libghostty-vt.patches.md`. If the new upstream commit contains the fix, remove the local patch and index entry, then rerun the listed verification. If not, reapply the patch on top of the new vendored source."

and it is *enforced*, not merely written down: [`scripts/test_vendor_libghostty_vt.py`](https://github.com/herdrdev/herdr/blob/master/scripts/test_vendor_libghostty_vt.py) has `test_local_vendor_patches_are_listed_in_patch_index` and `test_local_vendor_patches_are_applied_to_vendored_tree`, the latter running `git apply --check --reverse` for every `*.patch`. **The patches are pre-applied into the checked-in vendored tree, not applied at build time.** This is the real recurring cost of the vendor route, and herdr has automated it rather than absorbed it.

## 2.2 Patch 0001 — DEC private mode 2027 default

[`vendor/patches/libghostty-vt/0001-default-grapheme-cluster-mode.patch`](https://github.com/herdrdev/herdr/blob/master/vendor/patches/libghostty-vt/0001-default-grapheme-cluster-mode.patch), 1021 bytes, one hunk, one added line, in `vendor/libghostty-vt/src/terminal/c/terminal.zig` at `fn new_`:

```diff
@@ -344,4 +344,5 @@ fn new_(
         .rows = opts.rows,
         .max_scrollback = opts.max_scrollback,
+        .default_modes = .{ .grapheme_cluster = true },
     });
```

herdr's stated reason:

> "Herdr renders terminal cells directly and requires DEC private mode 2027 to store flags, ZWJ emoji, and other multi-codepoint grapheme clusters in one cell. This patch makes clustering active for new terminals **and keeps it as the reset default so RIS (`ESC c`) does not disable it**." (emphasis added)

**Classification: a changed default value, not a new C API entry point.** `default_modes` is an existing upstream field of the Zig `Terminal.Options` struct ([`vendor/libghostty-vt/src/terminal/Terminal.zig:246`](https://github.com/herdrdev/herdr/blob/master/vendor/libghostty-vt/src/terminal/Terminal.zig)); the patch only populates it from the C wrapper. Mode 2027 is an ordinary DEC private mode in the upstream table — `.{ .name = "grapheme_cluster", .value = 2027 },` at `src/terminal/modes.zig:297`, which I independently confirmed at the crate's own pin `a887df42`.

### Can 0.2.1 reach this unpatched? **Partly — and the residue is precisely reset survival.**

**What 0.2.1 can do.** Mode 2027 is a first-class constant and there is a setter:

```rust
pub const GRAPHEME_CLUSTER: Self = Self::new(2027, ModeKind::Dec);   // terminal.rs:953
pub fn set_mode(&mut self, mode: Mode, value: bool) -> Result<&mut Self>  // terminal.rs:457
pub fn mode(&self, mode: Mode) -> Result<bool>                        // terminal.rs:447
```

So `terminal.set_mode(Mode::GRAPHEME_CLUSTER, true)?` at construction is available from safe Rust, no patch, no escape sequence. (Writing `\x1b[?2027h` into `vt_write` also works — it is a normal mode.)

**What 0.2.1 cannot do.** Make it *stick across a reset*. herdr's own index states the gap exactly, and my independent enumeration of the C API corroborates it:

> "libghostty-vt currently exposes current mode mutation but no C API for configuring terminal default modes"
> — `vendor/libghostty-vt.patches.md`, patch 0001, `upstream discussion` field

The C `ghostty_terminal_mode_set` is documented as "Set the value of a terminal mode. Sets the mode identified by the given mode to the specified value" (`bindings.rs:2535`) — the *current* value. The `GhosttyTerminalOption` enum (`bindings.rs:2337-2393`) has 27 members, and the only `DEFAULT_*` ones are `DEFAULT_CURSOR_STYLE = 22` and `DEFAULT_CURSOR_BLINK = 23`. **There is no default-modes option.** Ghostty's `ModeState.reset()` does `self.values = self.default;`, so a guest program issuing RIS (`ESC c`) reverts 2027 to the built-in default of off. herdr's regression test asserts exactly this behaviour:

```rust
#[test]
fn grapheme_cluster_mode_is_default_and_survives_full_reset() {
    let mut terminal = Terminal::new(80, 3, 100).unwrap();
    assert!(terminal.mode_get(MODE_GRAPHEME_CLUSTER).unwrap());
    terminal.write(b"\x1bc");
    assert!(terminal.mode_get(MODE_GRAPHEME_CLUSTER).unwrap());
}
```
— `herdr/src/ghostty/mod.rs:3879-3887`

herdr does not use any escape-sequence workaround: `grep -rn '\[?2027' src/ tests/ scripts/` over herdr returns **zero hits**, and `MODE_GRAPHEME_CLUSTER` (`src/ghostty/mod.rs:180`) appears only in assertions, never in a `mode_set` call. It relies purely on the patch.

**What a crate consumer would have to do instead.** Re-assert the mode after a reset. There is no reset callback in 0.2.1 — the callback set is `on_pty_write`, `on_bell`, `on_enquiry`, `on_xtversion`, `on_title_changed`, `on_pwd_changed`, `on_size`, `on_color_scheme`, `on_device_attributes`, `on_clipboard_write` (`terminal.rs:1536-1678`), and none fires on RIS. So the workaround is to poll `terminal.mode(Mode::GRAPHEME_CLUSTER)` after each `vt_write` batch and re-set it when it has been cleared — one boolean C call per batch, not per cell. **This is a design suggestion, not an established fact: I did not build or test it.** What *is* established is that the ingredients exist (`mode` getter, `set_mode` setter, both public and safe) and that the transient window between a guest's RIS and the next batch check would render flags and ZWJ emoji as separate cells.

## 2.3 Patch 0002 — exposing `modifyOtherKeys` mode 2

[`vendor/patches/libghostty-vt/0002-expose-modify-other-keys-mode.patch`](https://github.com/herdrdev/herdr/blob/master/vendor/patches/libghostty-vt/0002-expose-modify-other-keys-mode.patch), 2558 bytes, five hunks across two files — `include/ghostty/vt/terminal.h` and `src/terminal/c/terminal.zig`. It adds a header enum member:

```diff
   GHOSTTY_TERMINAL_DATA_VIEWPORT_ACTIVE = 32,
+  /**
+   * Whether xterm modifyOtherKeys mode 2 is enabled.
+   *
+   * Output type: bool *
+   */
+  GHOSTTY_TERMINAL_DATA_MODIFY_OTHER_KEYS = 33,
```

the matching Zig tag `modify_other_keys = 33`, its `bool` output-type arm, the read itself — `.modify_other_keys => out.* = t.flags.modify_other_keys_2,` — and a Zig unit test.

**Classification: a new C API entry point** (strictly, a new callable query on an existing entry function). It is not a changed default. `modifyOtherKeys` is not a DEC private mode at all — it is an XTMODKEYS resource held in the terminal flags struct (`modify_other_keys_2: bool = false,` at `Terminal.zig:98`), so `ghostty_terminal_mode_get` cannot reach it by any mode number. Writing `\x1b[>4;2m` into the parser *sets* the flag (the patch's own test proves it), but there is no unpatched way to *read it back as a scalar*.

herdr's stated reason:

> "Herdr must know whether xterm modifyOtherKeys mode 2 is active **to request printable key releases from the outer terminal**. The formatter API can recover this fact only by formatting the active screen and scrollback. A typed terminal-data query exposes the authoritative scalar without formatting or allocation." (emphasis added)

The index also records that this "fixes the performance regression exposed by [PR #2303](https://github.com/herdrdev/herdr/pull/2303)" — i.e. the unpatched route (format the whole screen and scrollback to VT, then parse the result) worked but was too slow to run on the input hot path.

### Can 0.2.1 reach this unpatched? **Not as a scalar read — but Tiller almost certainly does not need the scalar read.**

**The read is genuinely unavailable.** 0.2.1's `TerminalData` enum stops at `VIEWPORT_ACTIVE = 32` (`bindings.rs`), and `Terminal`'s accessors include `kitty_keyboard_flags()` (`terminal.rs:588`) but nothing for modifyOtherKeys. The formatter can emit it — `FormatterOptions::with_keyboard`, "Specify keyboard modes such as ModifyOtherKeys" (`fmt.rs:96-97`) — which is exactly the slow route herdr rejected.

**But the use case does not transfer.** herdr is a TUI that runs *inside somebody else's terminal emulator*; it must decide what to ask that outer terminal to send it. Read herdr's own code and the two directions separate cleanly:

- *Inbound, reading a pane's state* — the patched query: `Terminal::modify_other_keys_enabled()` at `src/ghostty/mod.rs:1037-1039`, consumed by `keyboard_report_all_requested` (`src/pane/terminal.rs:1685`) and `InputState { modify_other_keys: ... }` (`src/pane/terminal.rs:1800`).
- *Outbound, configuring the outer host terminal* — pure escape sequences, no patch needed: `ModifyOtherKeysMode::set_sequence` emits `\x1b[>4;1m` / `\x1b[>4;2m` (`src/input/model.rs:232-238`), gated by `host_modify_other_keys_mode()` which special-cases tmux, WezTerm and Alacritty (`src/input/model.rs:240-272`).

**Tiller has no outer terminal.** It is a native GPUI application that owns the pixels and receives key events from the windowing system. The thing it needs is to *encode* a key event correctly for the program running in the pane — and 0.2.1 covers that without any scalar read, because the C API applies the terminal's modifyOtherKeys state to the encoder directly:

> "Reads the terminal's current modes and flags and applies them to the encoder's options. This sets cursor key application mode, keypad mode, alt escape prefix, **modifyOtherKeys state**, and Kitty keyboard protocol flags from the terminal state."
> — `Encoder::set_options_from_terminal`, `src/key.rs:126-134` (emphasis added)

plus a direct setter, `Encoder::set_modify_other_keys_state_2(bool)` (`src/key.rs:181`), if Tiller ever wants to override it.

There is a third route in herdr worth noting because it shows the scalar read is not load-bearing even for them on all platforms: a Windows-only `KittyKeyboardTracker` ([`src/pane/kitty_keyboard.rs`](https://github.com/herdrdev/herdr/blob/master/src/pane/kitty_keyboard.rs)) that sniffs `CSI > 4 ; N m` out of the byte stream itself (`observe_modify_other_keys`, line 78) and clears on `ESC c`, feeding the ConPTY fallback encoder at `src/pane/terminal.rs:1859`. Byte-stream sniffing is available to any crate consumer.

## 2.4 Summary of Part 2

| | Patch 0001 (mode 2027) | Patch 0002 (modifyOtherKeys) |
| --- | --- | --- |
| Kind | changed **default value** | new **C API entry point** |
| Reachable from 0.2.1? | the mode: **yes** (`set_mode`). Reset survival: **no** | scalar read: **no**. Formatter round-trip: yes (slow). Byte sniffing: yes |
| Does Tiller need it? | **Yes** — Tiller renders cells directly, same as herdr | **Probably not** — it is a TUI-inside-a-terminal concern; Tiller's need is key *encoding*, which `set_options_from_terminal` covers |
| Cost of not patching | re-assert after reset; a transient wrong-render window (untested) | none identified for Tiller's use case |

Neither patch requires the *crate* to be replaced. Patch 0001 addresses a gap in **Ghostty's C API** that a hand-rolled FFI over a vendored tree would hit identically unless Tiller also patched Ghostty — which is a standing maintenance obligation, as §2.1 shows.

---

# Part 3 — Why `Xuanwo/gpui-ghostty` rolled its own

Repo: <https://github.com/Xuanwo/gpui-ghostty>, default branch `main`, Apache-2.0, 81 commits. All substantive code lands between 2026-01-01 and 2026-01-08; everything after that is unmerged dependabot PRs.

## 3.1 Yes, it built its own sys crate — and the crates.io crate appears nowhere

`crates/ghostty_vt_sys/Cargo.toml` has **no dependencies at all** (an empty `[build-dependencies]` section — no `cc`, no `bindgen`), and `crates/ghostty_vt/Cargo.toml:9` has exactly one, `ghostty_vt_sys = { path = "../ghostty_vt_sys" }`. `grep -i libghostty Cargo.lock` returns nothing. `git log --all -S"libghostty"` touches four commits, all prose. **The published crate is a dependency of nothing, at no point in this repo's history.**

Ghostty is a git submodule (`.gitmodules`) pinned to gitlink `6d2dd585a5d87fa745d48188dd096ca6e63014d0`, which is the object of the annotated tag `v1.2.3` (tagged 2025-10-23) — confirmed against the GitHub API, and recorded a second time as a Rust constant:

```rust
pub const PINNED_GHOSTTY_TAG: &str = "v1.2.3";
pub const PINNED_ZIG_VERSION: &str = "0.14.1";
```
— `crates/ghostty_vt_sys/src/lib.rs:7-8`

## 3.2 What the repo states, versus what is inference

**Stated, quoted.** There *is* a recorded reason, but it is about **Ghostty's own upstream build target**, not about the crates.io crate. It exists only in the first two commits and was later deleted:

> ```rust
> // The pinned Ghostty tag (v1.2.3) does not ship a standalone `libghostty-vt` build target.
> // Keep this as a hard error so the failure mode is obvious and actionable.
> panic!(
>     "Ghostty v1.2.3 does not provide a `libghostty-vt` build target yet; \
> update the Ghostty submodule to a revision that exports `zig build lib-vt`, \
> then implement link/bindings in crates/ghostty_vt_sys"
> );
> ```
> — `crates/ghostty_vt_sys/build.rs:30-36` at commit [`76f1851`](https://github.com/Xuanwo/gpui-ghostty/blob/76f1851934afc0275e51db63be172892d83f6861/crates/ghostty_vt_sys/build.rs#L30-L36)

and, in the README at the same commit: "At the pinned Ghostty version (`v1.2.3`), the `libghostty-vt` build target is not yet available, so `--features zig-build` is expected to fail with a clear message." That prose was removed by `7d534e1`; **today's `main` carries no rationale text at all.**

One narrower stated reason, for why the Zig shim reimplements a Ghostty symbol instead of linking Ghostty's:

> ```zig
> // Ghostty's terminal stream uses this symbol as an optimization hook.
> // Provide a portable scalar implementation so we don't need C++ SIMD deps.
> export fn ghostty_simd_decode_utf8_until_control_seq(
> ```
> — `crates/ghostty_vt_sys/zig/lib.zig:800-802`

**On the specific question — why not the crates.io crate — no recorded reason exists.** Searched and found empty: `git grep -i libghostty` on `main` (3 hits, all quoted above or a filename); `git log --all --grep=libghostty` (0 commits); all 81 commit messages are single-line subjects with no bodies, including the two decisive ones (`76f1851 "Bootstrap GPUI terminal workspace"`, `ce40ee6 "Add Zig-built Ghostty VT core wrapper"`); 1 issue, closed, about an IME bug; 5 PRs, none mentioning bindings; `search/issues?q=repo:Xuanwo/gpui-ghostty+libghostty` returns `total_count: 0`. **The repo never acknowledges the crate's existence.**

**Inference from dates — flagged as inference, not as a stated reason.** The chronology makes the question close to moot:

| Event | Date (UTC) |
| --- | --- |
| Ghostty v1.2.3 tagged (the pin) | 2025-10-23 |
| gpui-ghostty repo created | 2025-12-31 21:22 |
| `ghostty_vt_sys` first commit (`76f1851`) | 2025-12-31 21:43 |
| Zig-built VT core lands (`ce40ee6`) | 2025-12-31 22:12 |
| Last substantive commit (`e302598`) | 2026-01-08 10:15 |
| **crates.io `libghostty-vt` 0.1.0 published** | **2026-03-28 19:13** |
| `libghostty-vt` 0.2.1 published | 2026-07-18 18:04 |

**The published crate did not exist — by roughly three months — when `ghostty_vt_sys` was written, and still did not exist when the repo stopped receiving code changes.** There is no evidence of a deliberate rejection of the crate, and none of any later evaluation of it. Whether the author reconsidered after 2026-03-28 is **not determined**: no code commit since 2026-01-08.

(The repo is agent-driven under a fixed brief — `AGENTS.md:6` "Create `ROADMAP.md` based on the provided input document", `AGENTS.md:11` "Do not ask the user any questions". The input document is not in the repo, so any rationale it carried is not determined.)

## 3.3 The more useful finding: gpui-ghostty's binding is far *thinner* than the crate

Read as an argument for rolling our own, gpui-ghostty argues the opposite way. Its C surface is 18 hand-written `ghostty_vt_*` functions (`crates/ghostty_vt_sys/include/ghostty_vt.h`, 86 lines) over a hand-written 870-line Zig shim (`zig/lib.zig`) that **re-implements Ghostty's `Stream` handler by hand** — 21 callbacks (`print`, `backspace`, `setAttribute`, `eraseDisplay`, `startHyperlink`, `setMode`, …, `lib.zig:53-204`). The project's VT feature ceiling is that hand-written list, not Ghostty's `Terminal`.

Measured against the same checklist as Part 1:

| Capability | `libghostty-vt` 0.2.1 | `gpui-ghostty`'s `ghostty_vt` |
| --- | --- | --- |
| Per-cell character access | yes (4 grapheme accessors) | **no** — text only as whole-viewport or whole-row UTF-8 strings |
| Grapheme clusters | yes, plus a `unicode` module | **no API**; `grep -i grapheme` over the repo returns zero hits. Width is re-derived in Rust with the `unicode-width` crate |
| Attributes | 11 bools + 3 colors + 6-way underline enum | a `u8` bitset of 7 flags; underline collapsed to one bit (`lib.zig:384`); inverse/invisible pre-applied by swapping colors in Zig |
| Hyperlinks / OSC 8 | presence on render path, URI via `GridRef` | **yes** — `ghostty_vt_terminal_hyperlink_at(col,row)` (`lib.zig:670-692`) |
| Key encoding | full `Event` + 8 option setters | Ghostty's real `KeyEncoder`, but only for a hardcoded name list (arrows, home/end, pgup/pgdn, ins, del, bksp, enter, tab, esc, f1–f12); anything else returns null (`lib.zig:694-788`) |
| Mouse encoding | `mouse::Encoder` | **not from Ghostty** — hand-rolled SGR in Rust (`view/mod.rs:73, 106`) |
| Selection | full model + 6 selection constructors + gestures | **no Ghostty selection API**; selection lives in the Rust view layer over dumped row text |
| Formatters | Plain / VT / HTML | **plain text only** |
| Kitty graphics | full image + placement geometry | **absent** — zero hits for `kitty|sixel|APC` |

The one thing gpui-ghostty has that 0.2.1's render path lacks is `hyperlink_at(col,row)` — the §1.4 gap — and it obtained it **by writing Zig, not by choosing a different Rust binding.** That is the honest shape of the trade: the gap is real, and closing it costs Zig.

## 3.4 Platform: Windows is not addressed at all

CI (`.github/workflows/ci.yml`) is three jobs: `fmt` on `ubuntu-latest`, `vt` on `ubuntu-latest`, `terminal` on `macos-latest`. **No Windows job.** Zig 0.14.1 via `goto-bus-stop/setup-zig@v2` on both build jobs.

`crates/ghostty_vt_sys/build.rs` contains **no target handling whatsoever** — no `TARGET` read, no `-Dtarget=`, no cross-compile branch. `zig/build.zig:5` uses `b.standardTargetOptions(.{})`, which with no `-Dtarget` resolves to the host, always. `build.rs:68` emits `cargo:rustc-link-lib=c` unconditionally, and `find_zig` falls back to `.context/zig/zig`, produced only by `scripts/bootstrap-zig.sh`, which exits non-zero on any non-Darwin host (`bootstrap-zig.sh:13-16`).

**Bottom line: gpui-ghostty is not evidence that a hand-rolled FFI beats the crate.** It predates the crate, states no opinion about it, its binding is a small fraction of the crate's surface, and it does not build on Windows.

---

# Part 4 — Zig versions: is there a conflict?

**No. At 0.2.1 the two routes require the identical Zig version, 0.15.2.** The "0.16.x" figure in #27 is real but belongs to the crate's `master`, not to 0.2.1.

## 4.1 What the crate requires

The crate's build script performs **no Zig version check at all**. `libghostty-vt-sys-0.2.1/build.rs` never runs `zig version`; it runs `Command::new("zig").arg("build")…` (lines 130-141) and lets the Zig compiler enforce the requirement itself. Ghostty's `build.zig` does that at comptime:

```zig
/// Minimum required zig version.
const minimum_zig_version = @import("build.zig.zon").minimum_zig_version;

comptime {
    buildpkg.requireZig(minimum_zig_version);
}
```
— `ghostty/build.zig:12-17` @ `a887df42`

and `build.zig.zon` at that commit declares `.minimum_zig_version = "0.15.2"`. **So the effective requirement of the published 0.2.1 crate is whatever Ghostty `a887df42` demands: Zig 0.15.2.**

The README shipped *inside* the 0.2.1 tarball says nothing about Zig at all — it is four lines. The repo README at the 0.2.1 release commit `46a9d2a` says:

> "Requires [Zig](https://ziglang.org/) 0.15.x on PATH."

## 4.2 Where "0.16.x" comes from

The repo README on `master` today says 0.16.x. That line changed in commit `6111c4d` "libghostty-vt-sys: update to ab0b9da", dated **2026-07-22** — *after* the 0.2.1 release commit `46a9d2a` (2026-07-15), and `git merge-base --is-ancestor 6111c4d 46a9d2a` returns false, so it is **not** in 0.2.1. That commit moved the Ghostty pin to `ab0b9da9e88fcb4b0533a1854e84628f663930af`, whose `build.zig.zon` declares `.minimum_zig_version = "0.16.0"`.

**The Zig requirement is a property of the Ghostty pin, not of the binding.** Both routes inherit it the same way.

| Route | Ghostty pin | Zig required |
| --- | --- | --- |
| `libghostty-vt` **0.2.1** (published) | `a887df42` | **0.15.2** |
| `libghostty-vt` `master` (unreleased) | `ab0b9da` | 0.16.0 |
| **herdr** `master` (vendored) | `c5a21edfc` | **0.15.2** |
| `gpui-ghostty` | v1.2.3 | 0.14.1 |

herdr pins 0.15.2 in CI via `mlugg/setup-zig@…v2.2.1` (`ci.yml:92-97, 168-172`, and the same in `release.yml`, `preview.yml`, `build-artifacts-manual.yml`), and `rust-toolchain.toml` pins Rust `1.96.1`.

## 4.3 The macOS "patched Zig" — an SDK problem, not a Ghostty problem

herdr's CI installs Zig from Homebrew on macOS only:

```yaml
      - name: Install patched Zig on macOS
        if: runner.os == 'macOS'
        run: |
          HOMEBREW_NO_AUTO_UPDATE=1 brew install zig@0.15
          echo "$(brew --prefix zig@0.15)/bin" >> "$GITHUB_PATH"
          echo "ZIG_GLOBAL_CACHE_DIR=$GITHUB_WORKSPACE/.zig-cache" >> "$GITHUB_ENV"
          echo "ZIG_LOCAL_CACHE_DIR=$GITHUB_WORKSPACE/.zig-cache" >> "$GITHUB_ENV"
          "$(brew --prefix zig@0.15)/bin/zig" version
```
— [`.github/workflows/ci.yml:108-115`](https://github.com/herdrdev/herdr/blob/master/.github/workflows/ci.yml)

The `Install Zig` step using `mlugg/setup-zig` is gated `if: runner.os != 'macOS'`, so on macOS this Homebrew build is the only Zig.

**No workflow file carries a comment explaining why**, and the introducing commit's message is the single line `ci: use homebrew zig on macos`. The reason is recorded in the tracker instead — [herdr issue #2300](https://github.com/herdrdev/herdr/issues/2300):

> "`macos-latest` now runs macOS 26, whose SDK is newer than the libSystem bundled with the pinned Zig 0.15.2. Linking the vendored libghostty-vt build runner dies with a wall of undefined libc symbols:
> ```
> error: undefined symbol: _dispatch_semaphore_signal
> error: undefined symbol: __availability_version_check
> error: undefined symbol: _abort
> ```
> […] the `brew install zig@0.15` that puts a Zig built against the current SDK on `PATH`."

Corroborated by [herdr issue #2411](https://github.com/herdrdev/herdr/issues/2411) (`error: undefined symbol: _getcwd`).

**This is decision-relevant and it is route-independent.** It is a stock-Zig-0.15.2-versus-macOS-26-SDK incompatibility hit while linking *any* Zig-built Ghostty on a current macOS runner. Tiller would hit it on the crate route too, because the crate route also shells out to `zig build` against a 0.15.2-requiring Ghostty. The mitigation — install `zig@0.15` from Homebrew rather than a stock tarball — costs three lines of CI and transfers unchanged.

**What "patched" means in the Homebrew formula was not determined** beyond "built against the current macOS SDK"; herdr records no detail, and I did not read the formula.

One inconsistency worth carrying forward as evidence of the ongoing cost: herdr's own `build-artifacts-manual.yml` still uses `mlugg/setup-zig` unconditionally on its macOS job (lines 156-162) — the #2300 fix was not applied there.

## 4.4 Zig on every developer machine — true for both routes

Neither route bundles a compiler.

- **Crate:** `libghostty-vt-sys-0.2.1/build.rs:130` is `Command::new("zig")` — bare name, resolved on `PATH`. There is no `$ZIG` override and no bootstrap. It additionally shells out to **`git clone --filter=blob:none --no-checkout`** at build time (`build.rs:341-348`) unless `GHOSTTY_SOURCE_DIR` is set, so `git` and network access are build dependencies too. `GHOSTTY_ZIG_SYSTEM_DIR` exists for sandboxed package managers.
- **herdr:** `build.rs:63` is `env::var("ZIG").unwrap_or_else(|_| "zig".into())` — `$ZIG` override, else `PATH`. No version check either; the 0.15.2 requirement is enforced socially (`AGENTS.md:163` tells maintainers to point `$env:ZIG` at `C:\Users\herdr\zig-0.15.2\zig.exe`) and by the CI pin. Failure to find Zig is a bare `NotFound` panic ([issue #2281](https://github.com/herdrdev/herdr/issues/2281)). No network fetch at build time, because the source is checked in.

**Net difference:** the crate route adds a `git clone` and network access to every clean build that does not set `GHOSTTY_SOURCE_DIR`; the vendor route trades that for ~1.3 MB+ of checked-in Zig source and the patch-reapplication duty of §2.1.

---

# Part 5 — Windows: does the published crate actually build?

**Yes. The crate's own repository runs a Windows CI job that was green at the exact 0.2.1 release commit, on both `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`.** The premise in #27 — "its README documents Linux and macOS only" — is a README omission, not a capability limit, and the README is not where the answer lives.

## 5.1 The evidence, in order of strength

**(a) A dedicated Windows workflow exists at the 0.2.1 commit.** `.github/workflows/windows-ci.yml` @ `46a9d2ac941ed600cf43c5e6299c8dfd1d3a1ef0`:

```yaml
name: Windows CI
...
        matrix:
          include:
            - name: Windows x86_64 MSVC
              runner: windows-latest
              target: x86_64-pc-windows-msvc
            - name: Windows aarch64 MSVC
              runner: windows-11-arm
              target: aarch64-pc-windows-msvc
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - name: Install Zig
        uses: mlugg/setup-zig@v2
        with:
          version: 0.15.2
      ...
      - name: Build libghostty-vt
        run: cargo build -p libghostty-vt --target ${{ matrix.target }}
```

Note the Zig version: **0.15.2** — the same one herdr pins, corroborating Part 4 from a second direction.

**(b) It passed at that commit.** `GET /repos/Uzaaft/libghostty-rs/actions/runs?head_sha=46a9d2ac941ed600cf43c5e6299c8dfd1d3a1ef0`:

```
Windows CI | completed | success | https://github.com/Uzaaft/libghostty-rs/actions/runs/29429433788
CI         | completed | success | https://github.com/Uzaaft/libghostty-rs/actions/runs/29429434292
```

**(c) `build.rs` handles Windows deliberately, in three separate places.** All from `libghostty-vt-sys-0.2.1/build.rs`:

Target mapping (lines 380-397) covers both MSVC triples plus two GNU ones:

```rust
"x86_64-pc-windows-gnu" => "x86_64-windows-gnu",
"aarch64-pc-windows-gnullvm" => "aarch64-windows-gnu",
"x86_64-pc-windows-msvc" => "x86_64-windows-msvc",
"aarch64-pc-windows-msvc" => "aarch64-windows-msvc",
```

Windows library naming (lines 36-50) — `ghostty-vt-static.lib` for static, and for dynamic `ghostty-vt.lib` / `ghostty-vt.dll` / `libghostty-vt.dll.lib` / `libghostty-vt.dll.a`. And Windows artifact placement (lines 372-378), with its own explanatory comment:

```rust
/// On Windows, Zig may place the DLL in `bin/` and the import lib in `lib/`,
/// so both are included.
fn library_search_dirs(target: &str, install_prefix: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![install_prefix.join("lib")];
    if target.contains("windows") {
        dirs.push(install_prefix.join("bin"));
    }
    dirs
}
```

This is not incidental portability; someone debugged a Windows build to write that comment.

## 5.2 What the README does and does not say

The README at the 0.2.1 commit does not claim Linux/macOS-only. It mentions the two platforms once, in a narrow context — "When building with `link-dynamic`, set `LD_LIBRARY_PATH` on Linux or `DYLD_LIBRARY_PATH` on macOS" — which is a correct statement about dynamic linking that simply omits the Windows equivalent. **The README's silence on Windows is an omission; `build.rs` and CI both contradict the inference drawn from it.**

## 5.3 Comparison with herdr, and one thing herdr proves that the crate does not

herdr's `build.rs:6-18` maps the same two MSVC triples, its `ci.yml` matrix includes `windows-latest` (plus a `windows-conpty-package` job on `windows-2022`), and its release matrix ships `x86_64-pc-windows-msvc`.

Two asymmetries in herdr's favour, both worth recording honestly:

1. **herdr ships Windows binaries; the crate only compiles.** The crate's Windows job runs `cargo build`, not `cargo test`. It proves the native build and link succeed on both MSVC targets; it does **not** prove the resulting library behaves correctly on Windows.
2. **`aarch64-pc-windows-msvc` is better covered by the crate than by herdr.** The crate builds it on a `windows-11-arm` runner; herdr's `build.rs` maps the triple but no herdr workflow builds it. herdr's `windows-arm64.yml` runs on `windows-11-arm` but installs no Zig and builds nothing — it only tests that the x86_64 binary installs and runs as a fallback.

## 5.4 One caveat on the docs.rs evidence

docs.rs shows "All builds succeeded" for 0.2.1 (built 2026-07-18, rustc 1.99.0-nightly, 13 s). **This is not evidence that the native build works anywhere**, because `build.rs:68-70` returns early under docs.rs:

```rust
// docs.rs has no Zig toolchain. The checked-in bindings in src/bindings.rs
// are enough for generating documentation, so skip the entire native
// build when running under docs.rs.
if env::var("DOCS_RS").is_ok() {
    return;
}
```

Cited only so nobody later mistakes the green docs.rs badge for a build guarantee.

---

# Part 6 — Side-by-side

| Question | `libghostty-vt` 0.2.1 crate | Own FFI over a vendored Ghostty tree |
| --- | --- | --- |
| Cell contents, attributes, wide/spacer | complete (11 attrs + 3 colors + underline enum) | identical (same C API) — but you write the wrapper |
| Grapheme clusters | complete, 4 accessors + `unicode` module | identical |
| Hyperlink presence | yes, on render path | identical |
| **Hyperlink URI on render path** | **no — C API gap at this pin**; reachable via `GridRef` (disclaimed for render loops) | **same gap** unless you patch Ghostty or write a Zig shim (what gpui-ghostty did) |
| `KeyEncoder` / `MouseEncoder` | complete, incl. `set_options_from_terminal` | identical |
| Formatters (plain/VT/HTML) | complete, with selection scoping | identical |
| Selection | complete + gesture module | identical |
| Kitty graphics | complete (images + placement geometry) | identical |
| Batched `*_get_multi` getters | declared but not safely wrapped — **callable via `libghostty_vt::ffi` with `unsafe`** | you would wrap them yourself |
| DEC 2027 on | `set_mode(GRAPHEME_CLUSTER, true)` | same, or patch the default |
| **DEC 2027 surviving RIS** | **not available** (no default-modes C API) | available **only by patching Ghostty** |
| modifyOtherKeys scalar read | not available (formatter or byte-sniffing only) | available only by patching; **Tiller's use case does not need it** |
| Zig required | **0.15.2** (Ghostty `a887df42`) | 0.15.2 (herdr, Ghostty `c5a21edfc`) |
| macOS 26 SDK / stock Zig 0.15.2 link failure | applies | applies identically |
| Windows `x86_64-pc-windows-msvc` | **builds, green CI at the release commit** | builds and ships (herdr) |
| Windows `aarch64-pc-windows-msvc` | **builds, green CI on `windows-11-arm`** | mapped in build.rs, no CI job (herdr) |
| Windows runtime behaviour proven | no (build-only CI) | yes for x86_64 (herdr tests + ships) |
| Build-time network | `git clone` of Ghostty unless `GHOSTTY_SOURCE_DIR` | none (source checked in) |
| Repo weight | none | full Ghostty tree |
| Recurring maintenance | bump a version number | re-apply patches on every pin bump; herdr automates this with two Python tests |
| Pin control | whatever the crate author chose; `GHOSTTY_SOURCE_DIR` overrides the source but not `bindings.rs` | full |

## 6.1 The answer, stated plainly

**For everything a direct cell renderer reads and writes, `libghostty-vt` 0.2.1 is sufficient.** Cells, attributes, graphemes, wide-char spacers, resolved colors, dirty tracking, cursor state, selection, both encoders, all three formatters, and the full Kitty graphics geometry are present and safely wrapped. It builds on Windows on both MSVC targets with green CI at the release commit, and it needs the same Zig 0.15.2 herdr pins.

**Three gaps are real, and none of them is closed by writing our own FFI over the same tree:**

1. **OSC 8 URIs are not on the render path** — a gap in Ghostty's C API, not in the Rust binding (§1.4). Vendoring inherits it; only patching Ghostty or writing a Zig shim closes it.
2. **DEC 2027 cannot be made reset-durable** — herdr patches Ghostty for this (§2.2). A crate consumer can set the mode but must re-assert it after a guest RIS.
3. **The batched `*_get_multi` getters are unwrapped** — but they are reachable today through `libghostty_vt::ffi` with `unsafe` (§1.8), so this costs a shim, not a fork.

**Two premises recorded in #27 do not survive contact with the published artifact:** 0.2.1 requires Zig **0.15.2**, not 0.16.x (that is `master`, post-release), and it **does** build on Windows on both MSVC triples.

---

# Part 7 — What I could not establish

Stated plainly, because an honest gap is more useful than a confident guess.

1. **Whether the batched `*_get_multi` C calls are actually faster in practice** than the per-property calls the safe crate makes. The C doc describes the batching; no benchmark exists in either repo, and I ran none. The *shape* of the cost (one C call per property per cell per frame) is established; the magnitude is not.
2. **Whether `libghostty-vt` 0.2.1 works correctly on Windows at runtime.** Its Windows CI runs `cargo build`, not `cargo test`. Compilation and linking on both MSVC targets are proven; behaviour is not.
3. **What "patched" means in Homebrew's `zig@0.15` formula**, beyond "built against the current macOS SDK". herdr records only the symptom (undefined libc symbols). I did not read the formula.
4. **Whether either herdr patch has ever been discussed with Ghostty upstream.** herdr's own index says `upstream discussion: not opened` and `upstream pr: not opened` for both. I did not search the Ghostty repo to confirm no equivalent proposal exists there under a different author.
5. **Why `Xuanwo/gpui-ghostty` did not use the published crate.** No recorded reason exists anywhere in that repo — no prose, no comment, no commit body, no issue, no PR (§3.2). The date evidence (the crate postdates the code by ~3 months) is inference, not a stated reason, and whether the author later reconsidered is not determined: no code commit since 2026-01-08.
6. **The `ROADMAP.md` "provided input document"** that gpui-ghostty's `AGENTS.md:6` says the roadmap was generated from. Not in the repo; any rationale it carried is unrecoverable.
7. **Whether `GHOSTTY_SOURCE_DIR` is a viable middle route** — pointing the published crate at a locally patched Ghostty checkout, keeping the crate's safe API while patching the Zig. `build.rs:86-89, 110-121` makes the override authoritative over both pkg-config and the fetch, and asserts only that `build.zig` exists. But `src/bindings.rs` is *checked in*, not generated, so a patch adding a C enum member (herdr's 0002) would need `bindings.rs` regenerated too — the crate ships a `gen-bindings` binary behind the `bindgen-tool` feature for exactly that. **I did not test this path.** It looks like the cheapest way to get patch 0001's reset-durability without abandoning the crate, and it deserves a prototype before the crate-vs-vendor ticket is closed.
8. **How much of Tiller's `tiller_terminal` actually changes under either route.** Out of scope here; #27 records the seam analysis and the architecture ticket owns it.

---

## Source index

**Published artifacts (crates.io)**
- `libghostty-vt` 0.2.1 — <https://static.crates.io/crates/libghostty-vt/libghostty-vt-0.2.1.crate>
- `libghostty-vt-sys` 0.2.1 — <https://static.crates.io/crates/libghostty-vt-sys/libghostty-vt-sys-0.2.1.crate>
- Version/publish metadata — <https://crates.io/api/v1/crates/libghostty-vt>
- docs.rs build status — <https://docs.rs/crate/libghostty-vt/0.2.1/builds>

**Uzaaft/libghostty-rs** (all read at the 0.2.1 release commit `46a9d2ac941ed600cf43c5e6299c8dfd1d3a1ef0` unless noted)
- Repo — <https://github.com/Uzaaft/libghostty-rs>
- README @ `46a9d2a` — <https://github.com/Uzaaft/libghostty-rs/blob/46a9d2ac941ed600cf43c5e6299c8dfd1d3a1ef0/README.md>
- `.github/workflows/windows-ci.yml` @ `46a9d2a`
- `.github/workflows/ci.yml` @ `46a9d2a`
- Green CI runs at `46a9d2a` — <https://github.com/Uzaaft/libghostty-rs/actions/runs/29429433788> (Windows), <https://github.com/Uzaaft/libghostty-rs/actions/runs/29429434292>
- Commit `6111c4d` "libghostty-vt-sys: update to ab0b9da" (2026-07-22) — the post-0.2.1 Zig 0.16.x bump

**ghostty-org/ghostty**
- `build.zig`, `build.zig.zon`, `src/terminal/modes.zig`, `include/ghostty/vt.h` @ `a887df42c56f6de86c0fe6da9c4eeca37931e083`
- `build.zig.zon` @ `ab0b9da9e88fcb4b0533a1854e84628f663930af` (Zig 0.16.0)
- `build.zig.zon` @ `c5a21edfcbc2d5b46540ad91b7980aca31f5f1f3` (herdr's vendored base; Zig 0.15.2)

**herdrdev/herdr** (branch `master`)
- Patch index — <https://github.com/herdrdev/herdr/blob/master/vendor/libghostty-vt.patches.md>
- <https://github.com/herdrdev/herdr/blob/master/vendor/patches/libghostty-vt/0001-default-grapheme-cluster-mode.patch>
- <https://github.com/herdrdev/herdr/blob/master/vendor/patches/libghostty-vt/0002-expose-modify-other-keys-mode.patch>
- <https://github.com/herdrdev/herdr/blob/master/vendor/libghostty-vt.vendor.json>, `vendor/libghostty-vt/VERSION`
- <https://github.com/herdrdev/herdr/blob/master/build.rs>, `AGENTS.md`, `flake.nix`, `nix/package.nix`
- <https://github.com/herdrdev/herdr/blob/master/scripts/test_vendor_libghostty_vt.py>, `scripts/vendor_libghostty_vt.py`
- <https://github.com/herdrdev/herdr/blob/master/.github/workflows/ci.yml>, `release.yml`, `preview.yml`, `build-artifacts-manual.yml`, `windows-arm64.yml`
- `src/ghostty/mod.rs`, `src/pane/terminal.rs`, `src/input/model.rs`, `src/pane/kitty_keyboard.rs`
- Vendored Ghostty sources: `vendor/libghostty-vt/src/terminal/Terminal.zig`, `src/terminal/modes.zig`, `src/terminal/c/terminal.zig`, `include/ghostty/vt/terminal.h`
- Issues — [#243](https://github.com/herdrdev/herdr/issues/243), [#285](https://github.com/herdrdev/herdr/issues/285), [#2281](https://github.com/herdrdev/herdr/issues/2281), [#2300](https://github.com/herdrdev/herdr/issues/2300), [#2411](https://github.com/herdrdev/herdr/issues/2411), PR [#2303](https://github.com/herdrdev/herdr/pull/2303)

**Xuanwo/gpui-ghostty** (branch `main`)
- Repo — <https://github.com/Xuanwo/gpui-ghostty>
- `Cargo.toml`, `Cargo.lock`, `README.md`, `ROADMAP.md`, `AGENTS.md`, `.gitmodules`
- `crates/ghostty_vt_sys/build.rs`, `src/lib.rs`, `include/ghostty_vt.h`, `zig/build.zig`, `zig/build.zig.zon`, `zig/lib.zig`
- `crates/ghostty_vt/src/lib.rs`, `crates/gpui_ghostty_terminal/src/view/mod.rs`, `src/font.rs`
- `.github/workflows/ci.yml`, `scripts/bootstrap-zig.sh`
- Commit [`76f1851`](https://github.com/Xuanwo/gpui-ghostty/commit/76f1851934afc0275e51db63be172892d83f6861) (the deleted rationale), `ce40ee6`, `7d534e1`

**Tiller**
- Map issue — <https://github.com/tillerai/tiller/issues/27>
- This ticket — <https://github.com/tillerai/tiller/issues/28>
