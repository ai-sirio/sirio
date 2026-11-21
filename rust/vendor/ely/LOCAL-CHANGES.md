# Upstream Ely, vendored verbatim

- Source: https://github.com/ELY-Open-Source/Ely-GPUI-Components
- Rev: `8d7723ec` (never moved)
- Crates: `ely-core`, `ely-chat`, `ely-agent`, `ely-editor`, `ely-git`, `ely-documents`,
  `ely-collab`, `ely-charts`, under `crates/`.
- File set taken from each crate: `Cargo.toml`, `src/`, and `icons/`, `assets/` and `LICENSE` where
  present. Never `AGENTS.md` or `examples/`. `LICENSE` at this directory's root is upstream's.
- Sirio reaches these crates only through `rust/vendor/ely-gpui-component`, a facade that keeps
  the `ely_gpui_component::*` paths (see "The facade and the switch" below).
- Check: `ELY_UPSTREAM=<a clone of upstream with the rev> rust/vendor/ely/check-vendor.sh [out-dir]`
  extracts each crate at the rev, diffs it against this directory (`<out-dir>/<crate>.diff`; a
  fresh temporary directory when none is given) and compares the differing paths (changed text and
  binary files, files on one side only) with the list under "Differing paths" below: a path that
  differs and is not listed fails, and so does a listed path that no longer differs. It checks
  paths, not hunks; that a hunk is the one its section describes is a review by hand. It does not
  look at `ely-palette` or the facade. It prints `VENDOR CHECK OK`. Run it after any edit here, and
  before and after a sync.

## Differing paths

The set `check-vendor.sh` accepts: every path under `crates/` that differs from upstream at the rev, one
bullet each, and nothing else. A path counts as listed only here, not when it is named elsewhere in these
notes; a path listed here that no longer differs fails the check as stale. The sections below say what each
change is and carries its proof. The check compares paths, not hunks: whether a hunk is the one its section
describes is a review by hand.

- `crates/ely-agent/Cargo.toml`
- `crates/ely-agent/src/control.rs`
- `crates/ely-agent/src/permission.rs`
- `crates/ely-agent/src/tools.rs`
- `crates/ely-charts/Cargo.toml`
- `crates/ely-chat/Cargo.toml`
- `crates/ely-chat/src/attach.rs`
- `crates/ely-chat/src/composer.rs`
- `crates/ely-chat/src/expansion.rs`
- `crates/ely-chat/src/lib.rs`
- `crates/ely-chat/src/message.rs`
- `crates/ely-chat/src/status.rs`
- `crates/ely-collab/Cargo.toml`
- `crates/ely-core/Cargo.toml`
- `crates/ely-core/icons/clipboard-list.svg`
- `crates/ely-core/icons/file-pen.svg`
- `crates/ely-core/icons/shield-off.svg`
- `crates/ely-core/src/feedback/messages.rs`
- `crates/ely-core/src/forms/input.rs`
- `crates/ely-core/src/forms/mod.rs`
- `crates/ely-core/src/forms/text/actions.rs`
- `crates/ely-core/src/forms/text/edit.rs`
- `crates/ely-core/src/forms/text/element.rs`
- `crates/ely-core/src/forms/text/mod.rs`
- `crates/ely-core/src/menus/draw.rs`
- `crates/ely-core/src/menus/model.rs`
- `crates/ely-core/src/menus/mod.rs`
- `crates/ely-core/src/menus/panel.rs`
- `crates/ely-core/src/navigation/tabs.rs`
- `crates/ely-core/src/navigation/toc.rs`
- `crates/ely-core/src/overlays/dialog_keys.rs`
- `crates/ely-core/src/overlays/dialog.rs`
- `crates/ely-core/src/overlays/dialogs.rs`
- `crates/ely-core/src/overlays/mod.rs`
- `crates/ely-core/src/overlays/popover_close.rs`
- `crates/ely-core/src/overlays/popover.rs`
- `crates/ely-core/src/primitives/icon/names.rs`
- `crates/ely-core/src/theme/mod.rs`
- `crates/ely-core/src/theme/palette.rs`
- `crates/ely-core/src/theme/syntax.rs`
- `crates/ely-core/src/theme/tokens.rs`
- `crates/ely-core/src/typography/effects.rs`
- `crates/ely-core/src/typography/fit.rs`
- `crates/ely-documents/Cargo.toml`
- `crates/ely-documents/src/lib.rs`
- `crates/ely-documents/src/markdown.rs`
- `crates/ely-documents/src/markdown/build.rs`
- `crates/ely-documents/src/markdown/build/tests.rs`
- `crates/ely-documents/src/markdown/tex.rs`
- `crates/ely-documents/src/math.rs`
- `crates/ely-documents/src/modes.rs`
- `crates/ely-documents/src/picture.rs`
- `crates/ely-documents/src/render/blocks.rs`
- `crates/ely-documents/src/render/math_text.rs`
- `crates/ely-documents/src/render.rs`
- `crates/ely-documents/src/render/select.rs`
- `crates/ely-documents/src/render/table.rs`
- `crates/ely-documents/src/render/tests.rs`
- `crates/ely-documents/src/tests.rs`
- `crates/ely-documents/src/tree/emoji.rs`
- `crates/ely-documents/src/tree/html_block.rs`
- `crates/ely-documents/src/tree/mod.rs`
- `crates/ely-documents/src/tree/sources.rs`
- `crates/ely-documents/src/tree/tests.rs`
- `crates/ely-documents/src/tree/tests/inline.rs`
- `crates/ely-editor/Cargo.toml`
- `crates/ely-editor/src/anchors.rs`
- `crates/ely-editor/src/buffer.rs`
- `crates/ely-editor/src/edit.rs`
- `crates/ely-editor/src/filters.rs`
- `crates/ely-editor/src/find.rs`
- `crates/ely-editor/src/gutter.rs`
- `crates/ely-editor/src/hosting_tests.rs`
- `crates/ely-editor/src/ime.rs`
- `crates/ely-editor/src/keys.rs`
- `crates/ely-editor/src/layout.rs`
- `crates/ely-editor/src/lib.rs`
- `crates/ely-editor/src/line.rs`
- `crates/ely-editor/src/moves.rs`
- `crates/ely-editor/src/problems.rs`
- `crates/ely-editor/src/row.rs`
- `crates/ely-editor/src/search.rs`
- `crates/ely-editor/src/sideways.rs`
- `crates/ely-editor/src/state.rs`
- `crates/ely-editor/src/syntax.rs`
- `crates/ely-editor/src/view.rs`
- `crates/ely-git/Cargo.toml`
- `crates/ely-git/src/rows.rs`

## Changed paths

The sections below say what each path listed under "Differing paths" carries, and its proof. The
list is what the check accepts as different from upstream; anything else differing is a failure.

### Manifests (all eight)

`crates/<crate>/Cargo.toml` for each crate:

- `x.workspace = true` becomes upstream's workspace value, literally (package fields, third-party
  dependencies). `[lints]` tables are dropped. `publish = false` is added.
- `gpui` and `gpui_platform` stay `workspace = true`, now Sirio's workspace entries. Each crate gains
  a `[target.'cfg(target_os = "linux")'.dependencies]` section enabling gpui's `wayland` and `x11`,
  the same section the old `ely-gpui-component` carries.
- `ely-core`: `default = []` (was `["fonts"]`), and the `five_icons` `[[example]]` is dropped with
  `examples/`.
- `ely-core` also keeps upstream's `gpui = { workspace = true, features = ["stacker"] }`
  (`crates/ely-core/Cargo.toml:28`), verbatim. This is not a Sirio change, and it reaches the whole
  app: Cargo unifies gpui's features across the graph, so gpui's `stacker` feature is on for every
  crate that links gpui, and taffy layout and the element tree now run through `stacksafe`. It adds
  `stacksafe`, `stacker` and `psm` to `Cargo.lock` (none is on `main`), and `psm` has a native
  assembler build step on every platform. Nothing in the tree runs that step on Windows before the
  release: `windows-check.yml` builds no gpui crate, so the first compile of the app on Windows is
  the nightly after the merge (`cargo check --target x86_64-pc-windows-msvc` stopped at `psm`'s
  assembler during the sync's own check). Dropping the feature would mean editing `ely-core`'s
  manifest or overriding it from the workspace; neither is done.

- `crates/ely-core/Cargo.toml`
- `crates/ely-chat/Cargo.toml`
- `crates/ely-agent/Cargo.toml`
- `crates/ely-editor/Cargo.toml`
- `crates/ely-git/Cargo.toml`
- `crates/ely-documents/Cargo.toml`
- `crates/ely-collab/Cargo.toml`
- `crates/ely-charts/Cargo.toml`

### Upstream's patched pulldown-cmark (L7)

Two `ely-documents` tests of upstream's `e71b644b` (`markdown::tex::tests::a_note_in_a_markdown_link_frees_the_math_after_it`
and `tree::tests::inline::a_link_around_a_footnote_reference_stays_a_link`) need upstream's patched
pulldown-cmark. They were `#[ignore]`d until this change; the `#[ignore]` lines are gone, so
`markdown/tex/tests.rs` no longer differs from upstream. `tree/tests/inline.rs` still does, for the reasons in its other sections.

What: `rust/vendor/pulldown-cmark` is upstream's `vendor/pulldown-cmark` at `8d7723ec` (0.13.6, 47 files),
taken verbatim, wired through `[patch.crates-io]` in `rust/Cargo.toml` (and listed in the workspace
`exclude`). `Cargo.lock`'s `pulldown-cmark` entry becomes 0.13.6 with no registry `source`. Nothing in
it differs from upstream, and `check-vendor.sh` also compares this directory with upstream's: any
differing path (changed, or on one side only) fails, since this change is taken whole and has no
differing-paths entry.

Proof: `cd rust && cargo nextest run -p ely-documents --features test-support -E 'test(a_note_in_a_markdown_link_frees_the_math_after_it) | test(a_link_around_a_footnote_reference_stays_a_link)'`
failed both tests with the `#[ignore]`s removed and registry 0.13.5, and passes both (2 run, 2 passed)
with the vendored 0.13.6; `cargo nextest run -p sirio_markdown` runs 65 tests, all passing, before and after.

### The palette split (`ely-palette`), rows 58 to 60

`Palette`, `Syntax`, `SyntaxTheme`, `syntax_themes`, `Mix`, `HUE_NAMES`, `color_blind_chart` and
`Mode` (with `From<WindowAppearance>`) now live in the sibling crate `rust/vendor/ely-palette`,
re-extracted from this rev's `ely-core` theme files. `ely-core` depends on it and re-exports the
types at the original `theme::` paths, so `ely_core::theme::{Palette, Syntax, Mode}` are the same
types Sirio's theme crate holds.

- `crates/ely-core/Cargo.toml`: gains `ely-palette.workspace = true`.
- `crates/ely-core/src/theme/mod.rs`: `Mode` and its `From<WindowAppearance>` impl are removed; the
  `palette` and `syntax` modules are gone; `pub use ely_palette::{HUE_NAMES, Mix, Mode, Palette,
  Syntax, SyntaxTheme, syntax_themes}`; the `color_blind_chart` call reads `ely_palette::`.
- `crates/ely-core/src/theme/palette.rs`, `crates/ely-core/src/theme/syntax.rs`: deleted; their
  content moved into `ely-palette`.

In `rust/vendor/ely-palette`: `Palette` and `Syntax` derive `Copy` (all fields are `Hsla`, so
Sirio's `Theme` stays `Copy`), and `color_blind_chart` is `pub` instead of `pub(crate)`. The colour
values are upstream's at this rev (the dark palette and the chart hues moved on since the old
extraction). `ely-palette` is upstream's two theme files plus `Copy` and `pub`, and
`SyntaxTheme::of` returns `self.light` and `self.dark` where upstream clones them (a `.clone()` on a
`Copy` type once `Syntax` is `Copy`); upstream's other `.clone()` calls stay (row 61 is cosmetic and
not carried). `check-vendor.sh` does not look at `ely-palette`, so these are held by review alone.

Outcomes:

- Row 58: re-applied (split re-made on this rev's files). Proof: the closure check and
  `cargo nextest run -p sirio_theme`, which compile `Mode`/`Palette` as one type across both.
- Row 59: re-applied. Proof: `sirio_theme`'s `ThemeColors` derives `Copy` over a `Palette` field,
  so it fails to compile without it (`cargo nextest run -p sirio_theme`).
- Row 60: re-applied. Proof: `ely-core` calls `ely_palette::color_blind_chart` across the crate
  boundary; it does not compile with `pub(crate)`.

Moved tests: the five upstream tests in `palette.rs` (`a_color_goes_by_its_name`,
`mix_hits_both_ends_and_the_middle`, `a_name_no_color_has_fails`, `high_contrast_strengthens_borders`,
`palette_mix_moves_every_field`) now live in `rust/vendor/ely-palette/src/palette.rs`, outside the
eight-crate closure, which is why the closure count fell from 1427 to 1422. They are a plain
`#[cfg(test)]` module, and `ely-palette` is a workspace member, so `Scripts/ci.sh`'s workspace
nextest run covers them. Run by hand: `cd rust && cargo nextest run -p ely-palette` (5 run, 5 passed).

Dark values: the dark palette (`bg` `#252525` instead of `#131110`, and the neighbouring greys) and
the chart hues differ from the old extraction. No Sirio draw path reads them: Sirio's only
`ely_palette` users are `sirio_theme/src/lib.rs` (a `Palette` field) and `presets.rs` (its own
literals); the old `ely-gpui-component` calls `Palette::dark` only as `Theme::target()`'s fallback
(`theme/mod.rs:196`), overridden by `sirio_ui/src/ely.rs:75` `set_palette`; colour-blind mode is
enabled nowhere. Task 10's section 6.4 captures confirm it.

Changed paths of the palette split:

- `crates/ely-core/src/theme/mod.rs`
- `crates/ely-core/src/theme/palette.rs`
- `crates/ely-core/src/theme/syntax.rs`

## ely-core hooks (Task 3)

Sirio's local changes to the old vendored crate, carried onto upstream's `ely-core`. Every row says
re-applied, absorbed, dropped or re-decided, and its proof. A proof named "runs at Task 8" is a
Sirio-side test or E2E: no Sirio crate uses these crates before Task 8, so it cannot run earlier.
`ely-core` gains a `[dev-dependencies]` entry, `gpui` with its `test-support` feature, so the plain
`#[cfg(test)]` modules below (which the repository's gate runs) build without ely-core's own
`test-support` feature.

- `crates/ely-core/Cargo.toml`: the `[dev-dependencies]` entry above.

### Dialogs and popovers (rows 13 to 19, 87, 88)

- `crates/ely-core/src/overlays/dialog.rs`: both cards (centred and fullscreen) set
  `text_color(colors.fg)`, because a dialog is raised above its owner and otherwise inherited the
  owner's text colour (gpui's default black in Sirio, unreadable on a dark card).
  `Dialog::on_enter(field, build)` is `pub` and names its own field (upstream's is `pub(crate)` and
  takes the `focus_first` field, panicking without one).
- `crates/ely-core/src/overlays/dialogs.rs`: `PromptDialog` passes its field to `focus_first` and to
  `on_enter`.
- `crates/ely-core/src/overlays/popover.rs`: `Popover::on_close(handler)` runs each time the panel
  closes, however it closed (trigger, Escape, a press outside, focus leaving), after focus is handed
  back and only if it was open; the hook is stored in the popover's keyed state on every render.
  `Popover::open(bool)` lets the owner hold the panel open or closed; it follows on the next draw and
  every close, the owner's included, runs `on_close`.
- `crates/ely-core/src/overlays/mod.rs`: declares the two test modules below.
- `crates/ely-core/src/overlays/dialog_keys.rs` (new, plain `#[cfg(test)]`):
  `a_dialog_takes_the_first_key_typed_right_after_it_opens` and the three moved dialog tests.
- `crates/ely-core/src/overlays/popover_close.rs` (new, plain `#[cfg(test)]`): the two popover tests
  moved from the old crate's `overlays/tests.rs`.

Outcomes:

- Row 13: re-applied. Proof: unproven (no test or E2E step checks the card's text colour; `test-project-identity-e2e.sh` captures show it only by eye at Task 10).
- Rows 14, 15: absorbed. `Dialog::held` and `Dialog::focus_first` are `pub` (`#[doc(hidden)]`) upstream;
  grep proof (`overlays/dialog.rs`).
- Row 16: re-applied (above). Proof: `overlays::dialog_keys::a_dialog_takes_the_first_key_typed_right_after_it_opens`
  (runs `on_enter(field, ..)` through `PromptDialog`); `project_identity_dialog.rs` calls it at Task 8.
- Row 17: absorbed by a different mechanism, and a feelable change. The old rule (a release of Enter
  confirms only after a press the dialog saw) existed because the old crate confirmed on the *release*
  of Enter. Upstream confirms on the key-down (its `capture_action(Enter)`), and has no key-up
  handler, so the release of an Enter pressed before the dialog opened can never confirm it. The
  consequence is that Enter now confirms on press, not on release. Nothing in Sirio depends on the
  release: `test-project-identity-e2e.sh` scenario 4 (keydown opens, keyup leaves it open, a full
  Return confirms) holds either way. Proof: `overlays::dialog_keys::a_dialog_takes_the_first_key_typed_right_after_it_opens`
  (its Enter half has a positive control: an Enter that was pressed does confirm), and the old crate's
  three dialog tests, moved unchanged in intent into the same module and passing on upstream's code
  (ran, each alone, 1 test): `overlays::dialog_keys::a_release_with_no_press_does_not_confirm`,
  `overlays::dialog_keys::a_press_and_its_release_confirm_once`,
  `overlays::dialog_keys::a_press_while_the_field_is_out_of_focus_arms_nothing`. The first was written
  first and was GREEN on the verbatim code, not red: upstream's key-down confirm already meets it.
  For the section 6.4 list for Enzo.
- Row 18: re-applied. Proof: as row 16 (`a_dialog_takes_the_first_key_typed_right_after_it_opens` goes through `PromptDialog`).
- Row 19: re-decided: upstream's `HoverListenerMode::InputModalityIndependent` on scroll, hover card
  and tooltip is taken (`layout/scroll.rs`, `overlays/hover.rs`, `primitives/tooltip.rs` are verbatim).
  The local "fork's `on_hover`" existed only because bezel-gpui lacked the mode. Reason: no documented
  Sirio requirement beyond that. Proof: named hand check in Task 10's captures (hover cards, tooltips).
- Rows 87, 88: re-applied. Proofs (ran): `overlays::popover_close::every_way_a_popover_closes_tells_its_owner_once`,
  `overlays::popover_close::a_popover_follows_its_owner_open_and_closed`.

### The text field (rows 62 to 82, 179 to 185, 196, 228)

- `crates/ely-core/src/forms/text/mod.rs`:
  - `TextInput::detached(cx)` is `new` without a `Window`; `new` is `detached` plus `attach`, the one
    place the focus and blur subscriptions are made (it sets `attached`). A detached field attaches at
    its first `render` (`first_frame`); if the handle is already focused by then and the window is
    active, it takes the focused state (`gained_focus`, shared with the listener), emits one
    `InputEvent::Focus` and starts its blink. In an inactive window the listener fires when the window
    activates, so taking focus at the first frame too would report it twice.
  - `TextInput::host_context(context)`: the root's key context is the one node `ElyInput <context>`
    (`ElyInput ElyMultiLine <context>` with rows), built when `multi_line` or `host_context` is called
    (`refresh_context`), not every frame. `context` is one identifier (`debug_assert!`): gpui's context
    parser recurses forever on a `.` and reads `=` as key/value.
  - `TextInput::reset_text(text, cx)` is `set_text` plus a fresh undo history.
  - `TextInput::set_rows(min, max, cx)` replaces a multi-line field's rows, notifying only on change.
  - `wheeled`: the caret's laid-out position when the wheel last scrolled; cleared by `replace`,
    `restore`, `move_to`, `select_to` and `set_text`.
  - `Newline`, `DeleteToLineStart`, `DeleteToLineEnd`, `DeleteWordRight` and `MULTI_LINE_CONTEXT` are
    `pub use`d from `forms::text`.
- `crates/ely-core/src/forms/text/actions.rs`:
  - the three delete actions and their handlers (ported from bezel-ui 0.1.4; each is one undo step;
    the line is the logical one between newlines), `MULTI_LINE_CONTEXT` (`ElyMultiLine`, `pub`);
  - bindings: `word_key!("delete")` for `DeleteWordRight`; on macOS `cmd-backspace`
    (`DeleteToLineStart`), `ctrl-k` (`DeleteToLineEnd`), the emacs set (`ctrl-a` Home, `ctrl-e` End,
    `ctrl-b` Left, `ctrl-f` Right, `ctrl-h` Backspace, `ctrl-d` Delete) and `ctrl-n`/`ctrl-p` Down/Up in
    `ElyMultiLine` alone;
  - `scroll_wheel` (`ScrollEvent`, registered with `on_scroll` for fields with rows only): scrolls
    within the laid-out overflow, records `wheeled`, and stops the event only when it scrolled, so at
    the limits the wheel reaches a page behind the field;
  - `render` calls `first_frame` and reads its key context from `self.context`.
- `crates/ely-core/src/forms/text/element.rs`: `content_height`; `keep_in_view` clamps without
  following the caret while `wheeled` still equals it; prepaint drops the hold when the caret's
  laid-out position changed by another path.
- `crates/ely-core/src/forms/text/edit.rs`: `is_word` (a segment counts only with a letter or digit in
  it) in `words`, so word motion and deletion skip punctuation as bezel's field did; upstream's test
  `words_skip_spaces_and_punctuation_stops` is rewritten (going back from `new` in
  `hello  brave, new` lands on `brave`), and `a_word_is_a_run_with_a_letter_or_digit_in_it` is added
  (moved from the old crate). Double-click selection (`word_at`) is untouched.
- `crates/ely-core/src/forms/mod.rs`: `Copy`, `Cut`, `Paste`, `SelectAll` (public upstream already),
  plus `Newline`, the three delete actions and `MULTI_LINE_CONTEXT`.
- `crates/ely-core/src/forms/input.rs`: the one-line frame is `min_h(control height)` where upstream
  has `h(..)`, so at an interface size over 14 the line, caret and selection do not cross the border.

Outcomes:

- Rows 62, 63, 64: re-applied. Proofs: runs at Task 8,
  `a_detached_field_focused_before_its_first_frame_reports_focus_and_types`,
  `a_detached_field_first_drawn_in_an_inactive_window_reports_focus_once_it_activates`.
- Row 65: re-applied. Proof: runs at Task 8, `a_host_binding_registered_after_ely_wins_and_can_hand_the_key_on`.
- Row 66: re-applied. Proof: runs at Task 8, `reset_text_starts_a_fresh_history`.
- Row 67: re-applied (written directly with `ScrollEvent` and `on_scroll`; upstream has no scroll
  handler on the field). Proofs: runs at Task 8, `a_long_multi_line_draft_scrolls_under_the_wheel`,
  `a_wheel_scrolls_a_draft_alone_and_reaches_the_page_at_its_limit`.
- Rows 68, 228: re-applied (the `wheeled` hold and the hand-resolved `filter` line in prepaint).
  Proof: runs at Task 8, `a_wheeled_draft_follows_the_caret_again_on_the_next_edit`.
- Row 69: partial upstream (`Copy`/`Cut`/`Paste`/`SelectAll` already `pub`), `Newline` re-applied as a
  re-export of upstream's own field action (`ely_core::forms::Newline`, bound to `shift-enter`; not
  `ely_editor::Newline`). Proof: runs at Task 8, `enter_sends_and_shift_return_inserts_a_newline`.
- Row 70: re-applied. Proof: runs at Task 8, `sirio_ui/src/chat/composer_height.rs`
  `every_open_chat_takes_a_new_maximum`.
- Rows 71, 72, 73: re-applied. Proofs: runs at Task 8,
  `the_delete_to_line_and_word_actions_delete_what_bezels_did`, `a_delete_to_line_end_is_one_undo_step`.
- Row 74: re-applied with upstream's `word_key!`. Proof: runs at Task 8,
  `the_composer_answers_the_editing_chords_of_its_platform`.
- Rows 75, 76, 77 (macOS chords): re-applied. Proofs: runs at Task 8 and, for the macOS chords,
  only in the release gate (`the_composer_answers_the_editing_chords_of_its_platform`,
  `ctrl_n_and_ctrl_p_do_not_move_a_single_line_field_the_way_down_and_up_do`,
  `the_composer_moves_the_caret_a_row_with_its_platforms_keys`).
- Row 78: re-applied. Proofs: runs at Task 8, `only_a_multi_line_field_names_the_multi_line_context`,
  `a_host_binding_still_wins_in_a_multi_line_field`.
- Rows 79, 80: re-applied. Proof (ran, in `ely-core`): `forms::text::edit::tests::a_word_is_a_run_with_a_letter_or_digit_in_it`.
  Written before the port: RED on the verbatim unit (`the word before 12 in "Hello world."  left: 11
  right: 6`), GREEN after `is_word`.
- Row 81: a host rule (Sirio's readline fallback on macOS); nothing vendored.
- Row 82: re-applied. Proofs: runs at Task 8,
  `settings::agents_page::tests::the_agents_search_keeps_its_row_inside_its_frame_at_every_interface_size`
  and `the_project_name_field_tracks_font_preferences_and_keeps_native_editing`
  (`sirio_ui/src/project_forms.rs`).
- Rows 179 to 185: absorbed. Upstream binds with the compile-time `secondary!` and `word_key!`
  (`typography/keys.rs`), its `keystroke()` uses `parse_with_os`, its selection code calls
  `secondary(cx)`, and `compat::load_key` has no counterpart to carry. Row 185 (`on_scroll` instead of
  `on_scroll_wheel`) is in the row 67 handler. Proof: the closure build.
- Row 196: re-decided: upstream's `select` (a selection outside the text or inside a character is
  logged and left) replaces the local `assert!`. Reason: nothing in Sirio relies on the panic, and the
  field must not take the app down over a bad range.

### Menus (rows 47 to 57, 172 to 175)

- `crates/ely-core/src/menus/panel.rs` (new): `MenuPanel`, `layer`, `Hang` (`Above`, `AboveEnd`,
  `At(point)`) and `panel_surface`, as in the old crate: a host that owns open state, anchor, keys and
  closing draws a menu's rows and surface with Ely's own look; a press runs the row's `on_click` and
  leaves the menu open. `layer` floats a panel over its parent's top edge or at a window point, kept in
  the window, occluding, fading in. A button row (`Menu::buttons`, new upstream) draws with
  `draw::button`, each button running its `on_click`.
- `crates/ely-core/src/menus/model.rs`: `MenuItem::{note, tooltip, selectors, trailing, muted}` and
  the readers `row_selector`, `is_disabled`, `tooltip_text`; `Menu::{step, run, items}` public
  (`run` presses an item entry, not a button row; `items` walks item entries only).
- `crates/ely-core/src/menus/draw.rs`: `row_frame(id, current, disabled, cx)` (public) is the frame of
  `draw::row`, which builds on it; `row` is `pub(super)` for `MenuPanel`; it draws the note, then
  `trailing` (in a `flex_none` wrapper) before the keys, a muted label in `fg_muted`, the debug
  selectors and the tooltip.
- `crates/ely-core/src/menus/mod.rs`: `mod panel`, `pub use draw::row_frame`,
  `pub use panel::{Hang, MenuPanel, layer, panel_surface}`.

Outcomes:

- Rows 47, 48, 49: re-applied. Proof: runs at Task 8, the `sirio_ui/src/chat/band_tests.rs` menu tests (e.g. `a_blocked_fast_row_is_disabled`), which draw the composer menus through `MenuPanel`/`layer`.
- Rows 50, 51, 52, 55, 56, 57: re-applied. Proof: runs at Task 8, `band_tests.rs` (`a_blocked_fast_row_is_disabled`, `the_background_tasks_row_is_informational`).
- Row 53: re-applied. Proof: runs at Task 8, `band_tests.rs` `a_blocked_fast_row_is_disabled` (asserts `open.step(None, 1)`) and `the_background_tasks_row_is_informational`.
- Row 54: re-applied. Proof: unproven (Sirio's own `controls.rs` has a separate `row_frame`; no Sirio test or E2E exercises Ely's `menus::row_frame`).
- Rows 172 to 175: re-applied (the four readers). Proof: runs at Task 8, `band_tests.rs` (`a_blocked_fast_row_is_disabled`; `sirio_ui/src/chat/menus.rs` reads `Menu::items`).
- The old crate's `menus` code had `Menu::item_at(ix)`; upstream's now takes a column for button rows,
  so `run` matches `Entry::Item` itself. No local API was renamed.

### Tabs (row 84)

- `crates/ely-core/src/navigation/tabs.rs`: `compact` also requires a tab with a non-empty label
  (`tabs.iter().any(|tab| !tab.label.is_empty())`). Row 84 re-applied. Proof: unproven (no test covers
  a strip of icon-only tabs; `test-ely-forge-probe.sh` is a visual check).

### Typography and theme (rows 24, 168 to 170)

- `crates/ely-core/src/typography/effects.rs`: `ShimmerText::phase(f32)` draws the band at the phase
  given (0 to 1 across one sweep, wrapping) and animates nothing; the band's colouring is one private
  function, `shimmered`, which both paths call. Rows 168, 169: re-applied. Proof: runs at Task 8,
  `sirio_ui/src/chat/session_panel/trace.rs` `the_card_asks_for_no_frame_but_its_own`.
- `crates/ely-core/src/typography/fit.rs`: `cut_line`, behind `Ellipsis` and `MiddleEllipsis`, joins
  its lines first (`one_line`). Row 170: re-applied. Proof (ran): `typography::fit::tests::a_cut_line_joins_the_lines_it_is_given`
  (moved from the old crate). The selectable-text `end_overlay` path is upstream's and is not joined.
- `crates/ely-core/src/theme/mod.rs`, `crates/ely-core/src/theme/tokens.rs`: `ThemeMetrics` (eight text
  sizes and four radii in pixels), `Theme::metrics: Option<ThemeMetrics>` (`None` in `Theme::init`),
  and `text_size`/`radius` return `px_to_rems(metrics..)` when it is set (one px-to-rem conversion, so
  the font scale is not applied twice). Row 24: re-applied. Proof: runs at Task 8,
  `sirio_ui/src/ely.rs` `a_theme_swapped_without_an_installer_reaches_both_consumers`.

### Feedback (row 25)

- `crates/ely-core/src/feedback/messages.rs`: `Alert::content(element)` takes a host-owned
  (selectable) body; the title is drawn only when non-empty; `body`, actions and dismissal are as
  before. Row 25: re-applied. Proof: unproven (`sirio_ui/src/chat/transcript.rs` draws chat errors through it, but no test asserts the alert).

### Icons and assets (rows 6, 7, 8)

- `crates/ely-core/src/primitives/icon/names.rs`, `crates/ely-core/icons/clipboard-list.svg`,
  `crates/ely-core/icons/file-pen.svg`, `crates/ely-core/icons/shield-off.svg`: `IconName::ClipboardList`,
  `FilePen` and `ShieldOff`, with their SVGs (lucide-static 1.48.0, byte-identical to the old crate's
  and the same release as the rest of upstream's). Row 8: re-applied. Proof (ran): the closure's
  `primitives::icon::tests::every_icon_carries_its_own_lucide_svg`, which checks each new file carries
  its own `lucide-<name>` class; runs at Task 8 for the permission-mode icons (composer captures).
- Row 6, re-decided: the `ely/` namespace is not needed in `ely-core`. Upstream embeds every icon's
  bytes (`svg().data(name.svg())`), so drawing an Ely icon looks nothing up in an asset source, and
  `assets.rs` stays verbatim. Where Sirio's code still needs a path (`IconName::path()`, the chat probe),
  that is the facade's job at Task 8 (spec 5.4: the facade's `AssetSource` maps `ely/`).
- Row 7, re-decided: Inter and JetBrains Mono are not bundled in a release build: `ely-core`'s `fonts`
  feature is off by default here (see Manifests), and Sirio's facade does not call upstream's `init`.
  A test build is another matter: `ely-git`'s Linux dev-dependency
  `ely-core = { features = ["fonts"] }` turns `fonts` on in `--workspace --all-targets` and nextest
  builds, so those fonts are compiled into the test binaries. IBM Plex Sans (for svg text) stays, as
  upstream always embeds it; whether the release binary keeps it is not settled (see
  `THIRD_PARTY_NOTICES.md`, which carries its licence either way).

## ely-chat and ely-agent hooks (Task 4)

Sirio's local changes to the old crate's `chat` and `agent` parts, carried onto upstream's `ely-chat`
and `ely-agent`. Proofs named "runs at Task 8" are Sirio-side tests: no Sirio crate uses these crates
before Task 8. Where a row has no named proof of its own, the upstream tests that exercise the
unchanged default path are named: they ran green on this code (each alone, 1 test:
`ely-chat` `tests::the_reasoning_opens_from_the_keyboard` and
`tests::compose::enter_sends_and_shift_enter_breaks_the_line`; `ely-agent`
`tests::the_prompt_walks_deny_always_then_once` and
`tests::control::enter_sends_a_written_answer_and_skips_a_blank_one`). Sirio's own tests live in
`sirio_ui` (`chat::ely_tests`, `chat::band_tests`, `chat::tests`).

### Expansion (row 27)

- `crates/ely-chat/src/expansion.rs` (new): `Toggle`, `Press`, `resolve`, as in the old crate, but
  `pub` (`#[doc(hidden)] pub mod expansion` in `lib.rs`, with docs because of `deny(missing_docs)`):
  `ely-agent` calls it across the crate boundary. It lives in `ely-chat`, not `ely-core`, because
  `ely-agent` already depends on `ely-chat` and `ely-core` itself has no use for it; so no other
  closure crate gains a dependency.
- Row 27: re-applied. Proof: runs at Task 8, `chat::ely_tests::ely_tool_and_thought_expansion_survives_virtualization`
  (host-controlled open state of a tool card and a thinking block).

### Composer (rows 33, 37 to 45; `crates/ely-chat/src/composer.rs`)

- `SendButton::variant` (default `Primary`, stop takes the same variant); the control is wrapped in a
  `flex_none` div with debug selector `send`, and an inner selector `stop-glyph` while busy or
  `send-ready` while ready (rows 33, 43). Upstream's stop was `Secondary`; the default send path now
  draws the same, since the stop square takes `self.variant` (Primary by default): a visible change
  for any host that does not call `variant` (Sirio always does, through `PromptInput::custom`).
- `InputHint` is no longer a unit struct: `InputHint::new().enter(..)`, `Default` (row 45). Upstream's
  doctest `InputHint;` becomes `InputHint::new().enter("to queue ·")`.
- `PromptInput::{custom, band, send_probe, trailing}` and the fields behind them; the shell has the
  debug selector `ely-prompt-input`, the theme's `surface` fill and a text colour so unstyled content
  is not gpui's default black; custom mode takes no `Enter` action and offers no completion rows;
  `ready` and `focused` are `Option`s so `custom` supplies them (rows 37 to 42, 44). `PromptInput::new`
  keeps upstream's behaviour (readiness from the field's text, Enter sends, `/` and `@` offer rows).
- `PromptInput::custom` draws send and stop in `ButtonVariant::Subtle`; the card border is `border`
  in both focus states; the tool row wraps unless the band is set (row 39). The band: full width, 1px
  top rule, no radius, `surface` fill, 12px sides, 10px above and 8px below, content in a centred
  column at most `column`, the tool row never wraps (row 40).
- Row 38 (`ready`): in the old crate the host-supplied eligibility was the `ready` argument of
  `custom`; there is no separate `ready` method on either side. Re-applied as that argument.
- Outcomes: rows 33, 37 to 45 re-applied. Proofs: runs at Task 8,
  `chat::band_tests::composer_band_spans_the_chat_column_on_one_row` (rows 40, 41, 42, 43, 39),
  `chat::band_tests::narrow_row_fits_from_its_measured_minimum` (rows 40, 42),
  `chat::ely_tests::ely_composer_enter_respects_completion_and_ime` (rows 37, 43),
  `chat::ely_tests::ely_composer_sends_an_attachment_without_text` (rows 38, 43, `send-ready`).
  Rows 33 (`variant`), 44 and 45 (`enter`): unproven (no Sirio test asserts the button variant, the
  card's text colour, or the hint's Enter wording). The default constructor stays proven by
  `tests::compose::enter_sends_and_shift_enter_breaks_the_line` (ran, 1 test).

### Messages (rows 22, 23, 171; `crates/ely-chat/src/message.rs`)

- `MessageAvatar::content` returns the host's element before the role avatar (row 22).
- `ChatContainer::composer_full_bleed` gives the composer the whole width, no padding, no cap
  (row 23).
- The `Role::System` body is `min_w_0` (row 171).
- Outcomes: all re-applied. Proofs: row 22 unproven (the agent mark is drawn by
  `sirio_ui/src/chat/transcript.rs`, no test asserts it); row 23 runs at Task 8,
  `chat::band_tests::composer_band_spans_the_chat_column_on_one_row` (the band spans the column only
  with a full-bleed container); row 171 unproven (no test draws a long unbroken system line).

### Status and thinking (rows 28 (status part), 30, 31, 32; `crates/ely-chat/src/status.rs`)

- `ThinkingIndicator::glyph` replaces the sparkles (row 31); upstream's `label` is `Option` with the
  i18n default and stays.
- `ThinkingBlock::{body, expanded, header_selector, label, thinking_glyph}` (rows 28, 30, 32): `expanded`
  goes through `expansion::resolve`; `label` is the honest label shown muted when thinking is over and
  no duration was measured; `body` replaces the streamed reasoning. Upstream's header accessibility
  (`role(Button)`, `aria_label`, `aria_expanded`, tab stop, focus ring) is kept. The old
  `log::info!("thinking block: open ..")` line is gone with the keyed-state toggle.
- Outcomes: rows 28 (this part), 30, 31, 32 re-applied. Proofs: row 28 runs at Task 8,
  `chat::ely_tests::ely_tool_and_thought_expansion_survives_virtualization` (host toggles through
  `header_selector`); rows 30, 31, 32 unproven (no Sirio test asserts the label or the glyph; the
  default path is covered by `tests::the_reasoning_opens_from_the_keyboard`, ran, 1 test).

### Attachments (row 46; `crates/ely-chat/src/attach.rs`)

- `AttachmentChip::remove_selector`: a debug selector on a `flex_none` wrapper around the remove
  button (the wrapper is always present, as before).
- Row 46: re-applied. Proof: runs at Task 8, `chat::tests::attach_control_accepts_one_image_and_rejects_the_rest`
  (clicks `attachment-remove-0`).

### Tools, requests and questions (rows 28, 29, 34, 35, 36; `crates/ely-agent/src/`)

- `tools.rs`: `ToolCallCard::{body, expanded, header_selector}` and `ToolCallGroup::{expanded,
  header_selector, status}`; the card's header is interactive when it has a body, arguments, a result
  or host control; a group with a `status` draws a `step_mark` in place of the wrench. The shared
  `header` helper now takes `Option<(bool, Press)>` (the open state it shows and the press), not
  upstream's `Option<Entity<bool>>`, and keeps upstream's `role(Button)`, `aria_label` and
  `aria_expanded`; the keyed-state toggle (and its log line) now lives in `expansion::resolve`.
- `permission.rs`: `PermissionPrompt::{custom, header_selector, body, action}`; in custom mode only
  the supplied actions render, and the built-in buttons exist only with an `on_answer`; the built-in
  "Always allow" is wrapped in a div with selector `ely-request-always` (row 36).
- `control.rs`: `HumanInputRequest::{custom, header_selector, body, action}`; no Ely field and no
  default Reply in custom mode, choices only with an `on_answer`.
- Outcomes: rows 28, 29, 34, 35, 36 re-applied. Proofs: runs at Task 8,
  row 28: `chat::ely_tests::ely_tool_and_thought_expansion_survives_virtualization` (the card and the
  thinking block: `expanded`, `header_selector`, `body`), and for the group
  (`expanded` and `header_selector`) `chat::tests::a_subagent_task_card_expands_nested_tool_calls`
  and `chat::tests::a_run_is_one_box_and_same_verb_calls_fold`. Row 29 (the group's `status`):
  unproven as a whole (no test sets it and asserts the mark). Rows 34, 35: row 34's custom mode is
  exercised by `chat::ely_tests::ely_request_sends_only_the_advertised_option_id` and
  `chat::ely_tests::ely_expired_request_cannot_answer_the_next_request`; row 35 (`header_selector` of
  a request) is unproven. Row 36: unproven (the `ely-request-always` wrapper is present by reading
  `permission.rs`; the one Sirio assertion, in `ely_request_sends_only_the_advertised_option_id`, is
  that it is absent in custom mode, so it cannot fail if the wrapper is missing). The built-in
  `PermissionPrompt::new` and `HumanInputRequest::new` paths are proven by the ran tests above
  (`tests::the_prompt_walks_deny_always_then_once`,
  `tests::control::enter_sends_a_written_answer_and_skips_a_blank_one`).

### Row 20 (stream)

Re-decided: the old crate left `StreamingMarkdown` out. Upstream exports it from
`ely-chat/src/stream.rs` and `lib.rs`; both are verbatim and it comes along, unused by Sirio.
No proof needed (nothing changed).

- `crates/ely-chat/src/lib.rs`: `#[doc(hidden)] pub mod expansion;`
- `crates/ely-chat/src/expansion.rs`, `crates/ely-chat/src/composer.rs`,
  `crates/ely-chat/src/message.rs`, `crates/ely-chat/src/status.rs`, `crates/ely-chat/src/attach.rs`
- `crates/ely-agent/src/tools.rs`, `crates/ely-agent/src/permission.rs`, `crates/ely-agent/src/control.rs`

No local API was renamed: no closure crate calls any of the changed signatures (`ely-chat` has
`src/tests/compose.rs` calling `PromptInput::new`, which keeps its signature).

## ely-editor hosting hooks (Task 5)

Sirio's local changes to the old crate's `editor` (rows 89 to 141), carried onto upstream's `ely-editor`.
Proofs named "runs at Task 8" are Sirio-side tests or E2E: no Sirio crate uses these crates before
Task 8. The old crate's performance and sideways-scroll changes (rows 142 to 167) are Task 6's and are
not here.

- `crates/ely-editor/Cargo.toml`: gains `[dev-dependencies]` with `gpui` and its `test-support` feature
  (the same entry `ely-core` has), so the plain `#[cfg(test)]` module below builds without the crate's
  own `test-support` feature.
- `crates/ely-editor/src/hosting_tests.rs` (new, plain `#[cfg(test)]`): the four tests moved from the old
  crate's `editor/tests/mod.rs` (`a_crlf_paste_lands_as_lf_with_the_caret_after_it`,
  `a_composed_text_with_a_crlf_lands_as_lf`, and the two `a_silent_reset_*`), unchanged in intent.
- `crates/ely-editor/src/lib.rs`: declares that module; exports `SingleCursor`, `ViewOptions`, `Finder`
  and `GlobFilter`.
- `crates/ely-editor/src/state.rs`: `ViewOptions` with `set_view_options` and `view_options`; the
  `EditorEvent` variants `Pointer`, `SecondaryClick`, `PlatformClick` and `GitMarkPressed` (so no `Eq`);
  `Options::{read_only_banner, hard_tabs, line_highlighter, breakpoints}`; `Press` and
  `CodeEditor::pressed`; the builders and setters `without_read_only_banner`, `draws_read_only_banner`,
  `hard_tabs`, `set_hard_tabs`, `uses_hard_tabs`, `without_breakpoints`, `line_highlighter`,
  `set_line_highlighter`; `diagnostics()`; `reset_keeping_view`; and `piece_colors`.
- `crates/ely-editor/src/view.rs`: `offset_over_code`; the body takes the id `code-editor-body`, the
  `Pointer` move and hover handlers and the `SecondaryClick` press; the left release calls `release`;
  `on_mouse_up_out` clears `pressed`; the banner is drawn when `draws_read_only_banner()`; the sticky
  header and the minimap colour through `piece_colors`.
- `crates/ely-editor/src/line.rs`: `press` records `pressed`; `release` (the platform click); the code
  column sets `CursorStyle::IBeam`; rulers and indent guides are a one-pixel fill; with a line
  highlighter set the bracket pass leaves the host's colour; `styles` colours through `piece_colors`.
- `crates/ely-editor/src/row.rs`: the ghost row's tail colours through `piece_colors`.
- `crates/ely-editor/src/gutter.rs`: `without_breakpoints` hides the dot and the press; the git bar's hit
  area is 4px and emits `GitMarkPressed`.
- `crates/ely-editor/src/edit.rs`: `lf_text` (applied in `paste`); `chosen`, `select` and
  `select_in_place`; `indent_step` and the hard-tab paths of Tab, Shift-Tab and Enter.
- `crates/ely-editor/src/ime.rs`: both commit paths apply `lf_text`.
- `crates/ely-editor/src/keys.rs`: non-macOS add-cursor is `shift-alt-up` and `shift-alt-down`; `Copy`
  propagates when every selection is empty; `SingleCursor` gains the doc line `deny(missing_docs)` needs
  now that it is exported.
- `crates/ely-editor/src/moves.rs`: `reveal_primary`.
- `crates/ely-editor/src/find.rs`: `Finder`; `find_all` delegates to it.
- `crates/ely-editor/src/filters.rs`: `GlobFilter`; `passes` delegates to it.
- `crates/ely-editor/src/search.rs`: `SearchPanel::{unsaved, selected, full_counts, without_summary}`,
  `SearchResultItem::selected`, and the private `Drawing` struct `results` takes.
- `crates/ely-editor/src/problems.rs`: `ProblemsPanel::counts`.

The line-indexed highlighter keeps upstream's `highlighter` untouched, because `ely-documents` calls it
(`document/mod.rs:173`, `modes.rs:142`). Sirio's is `CodeEditor::line_highlighter(..)` and
`set_line_highlighter(..)`, `Fn(usize, &str, &App) -> Vec<(Range<usize>, HighlightStyle)>`, held in its
own option (`IndexedHighlighter`); it wins when both are set. Without it `piece_colors` falls back to
upstream's `line_colors`, so upstream's behaviour is unchanged. Renamed API: Task 8 re-points
every Sirio caller of the indexed signature to `line_highlighter`/`set_line_highlighter`:
`sirio_ui/src/file_view/code_surface.rs` (the builder call and `set_highlighter` at line 432),
`sirio_ui/src/proposal_tab.rs:569` (`.highlighter(standalone_highlighter(language))`) and
`sirio_ui/examples/ely_editor_probe.rs`.

The row selector stays upstream's `editor-line-{n}` (row 127): `ely-documents`' own tests look it up
(`document::tests::secondary::a_right_press_in_source_keeps_the_selection`). Sirio's tests that look up
`code-editor-line-{n}` (seven uses, all in `sirio_ui/src/file_view.rs`: lines 3638, 3648, 3695, 4308, 4374, 5666 and 5815) follow at
Task 8.

Outcomes:

- Row 89: absorbed. Upstream carries the three panels and their inline tests, ahead of the old copy.
  Proofs (ran, each alone, 1 test): `problems::tests::files_with_worse_problems_lead_and_filters_hide`,
  `filters::tests::globs_take_includes_then_drop_excludes`.
- Rows 90, 91, 92: re-applied. Behaviour (ran): `find_all` and `passes` now run through `Finder` and
  `GlobFilter`, and `find::tests::finds_by_case_word_and_pattern`,
  `filters::tests::globs_take_includes_then_drop_excludes` and
  `tests::stale::a_glob_too_big_for_a_regex_matches_nothing` show that behaviour is unchanged; they would
  also pass with the old bodies back, so they do not prove the types are present. Presence: runs at Task 8,
  the compile of `sirio_ui/src/drawer/search_engine.rs:15` (it imports `Finder` and `GlobFilter`), plus the
  E2E scenarios `search_options` and `search_scope`.
- Rows 93, 94, 95, 96: re-applied. Proof: unproven. The E2E scenarios `search_dirty`, `search_open`,
  `search_cap` and `search_scope` assert Sirio's own state (`search_unsaved`, `search_selected`, the
  summary line), which stays true if Ely's panel drew none of the label, the wash, the `3 of 412` badge
  or the missing summary; only their captures show the drawing. Named hand check at Task 10: the captures
  `search-results` and `search-cap` (the `unsaved` label, the chosen row's wash, the badge, one totals line).
- Row 97: re-applied (internal). Proof: unproven beyond the closure build; `ReferencesPanel` passes the
  empty values.
- Row 98: re-applied. Proof: unproven. `problems_volume` asserts the store's counts, not the toggles'
  labels. Named hand check at Task 10: its capture `problems-volume` shows 1250, 2500 and 1250 on the
  toggles, not the 1 500 drawn.
- Rows 99, 100, 103: absorbed. Upstream's wider module set, its newer base files and its test module
  are taken whole; the old crate's ports from upstream `main` are in them. All 15 old window tests of
  `tests/mod.rs` have a same-named counterpart upstream, which ran green here; the old inline tests with
  no counterpart (the lookup by range, the edit log, the bracket index, the one-pass folds, the nesting
  index, `rows_are_lines`, the minimap plan) belong to Task 6's performance changes. Two further upstream
  tests stand for old ones (ran, each alone, 1 test, `--features test-support` for the second):
  `anchors::tests::anchors_go_after_text_written_at_them` for the old `offsets_ride_their_edits`, and
  `tests::escape_in_the_find_query_closes_the_box`.
- Rows 101, 102: absorbed (`ely-core` has `Breadcrumb` and `StatusBarItem`).
- Row 104: dropped. Upstream's `InlineChat` is vendored and `chat_row` draws it. Sirio never calls
  `set_chat` (grep of `rust/crates`), so nothing on screen changes.
- Row 105: absorbed (`ely_charts::tint`, through `use ely_charts as charts`).
- Row 106: dropped; upstream's `pub use layout::stack` and `code_colors` stay.
- Row 107: re-applied at Task 8, in the facade (spec section 5.4), not in `ely-editor`, which keeps
  upstream's `bind_keys`. Proof: runs at Task 8.
- Row 108: re-applied. Proof: runs at Task 8, `file_view.rs`
  `escape_closes_find_then_a_hunk_and_leaves_the_cursors_until_none_is_open` (it `capture_action`s
  `SingleCursor`, so it does not compile without the export).
- Row 109: re-applied. Proof: runs at Task 8, E2E `test-editor-e2e.sh` scenario `pane_keys`
  (shift-alt-down adds a cursor, ctrl-alt-down moves the pane focus). Linux only; macOS keeps `cmd-alt`.
- Row 110: re-applied. Proof: runs at Task 8, E2E scenario `copy_propagates` (an empty-selection copy
  copies nothing). Upstream's copy of the current line with no selection is gone, as before.
- Row 111: absorbed (same binding upstream). Upstream's handler also closes the chat first when one is
  open, which Sirio never opens. Proof: runs at Task 8, as row 108.
- Rows 112, 113, 114, 115: re-applied under the new name (above). Proof: runs at Task 8,
  `Scripts/Tests/test-ely-editor-probe.sh` (step 1: the highlighter's colour on line 3, the lexer's keyword
  colour kept out, the bracket run on line 4 drawn unlike the lexer's, with a control run) and E2E scenario
  `colour`. Unproven: the clipping and logging of out-of-range spans (113) and the ghost tail (114); the
  probe's own header says it does not cover them.
- Row 116: re-applied. Proof: unproven (no test reads `view_options` back; the probe says the options'
  effect on the frame is not covered).
- Row 117: re-applied. Proof: runs at Task 8, the probe's step 4 (no toggle from a click in the slot) and its
  step 8 (the `breakpoints-on` control run toggles).
- Row 118: absorbed. Upstream's `prose.rs` `reset` has the same body (new buffer, caret 0, history, folds,
  marked and composing, marks cleared, `Changed`, blink). Proof: a grep shows the same definition (old `state.rs:451-462`, upstream
  `prose.rs:38-47`: identical statements in the same order).
- Rows 119, 120, 121: re-applied. Proof: runs at Task 8, the probe's steps 2 (Pointer offset inside line 3),
  3 (the git bar is not the code) and 5 (`SecondaryClick` from a right click); for 119 also
  `file_view.rs` `the_pointer_sliding_from_the_code_onto_the_find_bar_drops_the_card` (a pointer resting on
  the code raises the diagnostic card from the `Pointer` offset).
- Row 122: re-applied. Proof: runs at Task 8, the same test: the card is dropped when the pointer slides
  onto the find bar, which only happens through the body's hover-leave `Pointer { offset: None }`.
- Row 123: re-applied. Proof: runs at Task 8, the probe's step 6 (a plain click emits no `PlatformClick`, a
  Super-held click does).
- Row 124: re-applied. Proof: unproven (nothing presses outside and releases).
- Row 125: re-applied. Proof: runs at Task 8, the probe's step 3 (`GitMarkPressed` from a real click on line
  5's git bar) and E2E scenario `git_marks` (a mark opens its hunk).
- Row 126: re-applied. Proof: the closure build (`Point<Pixels>` is not `Eq`, so the derive cannot stay).
- Row 127: dropped with a reason: Sirio follows upstream's selector, and its callers follow at Task 8 (above). Proof (ran): `ely-documents` `document::tests::secondary::a_right_press_in_source_keeps_the_selection`
  finds `editor-line-0`, so renaming the selector would break it.
- Row 128: re-applied. Proof: unproven. `file_view.rs` `revealing_a_line_scrolls_it_into_view` moves
  the caret, so `select` scrolls on its own and the test passes with `reveal_primary` a no-op; no test
  places the caret where it already is.
- Rows 129, 130, 131: re-applied. Proofs (ran, each alone, 1 test): `hosting_tests::a_crlf_paste_lands_as_lf_with_the_caret_after_it`,
  `hosting_tests::a_composed_text_with_a_crlf_lands_as_lf`,
  `hosting_tests::a_silent_reset_keeps_its_cursors_clamped_and_its_history_empty`,
  `hosting_tests::a_silent_reset_moves_a_cursor_inside_a_character_back_onto_it`. The first two went red
  with `lf_text` made an identity, and green again after it was put back.
- Row 132: re-applied. Proof: unproven (no test or probe step draws a ruler).
- Row 133: re-applied. Proof: runs at Task 8, the probe's step 6b (the cursor X shows over text, gutter
  digits and git bar).
- Row 134: re-applied (`select` keeps its behaviour through `chosen`). Proof (ran): the upstream window
  tests that call `select`, e.g. `tests::a_selection_inside_a_fold_opens_it`.
- Row 135: re-applied. Proof: unproven (no test calls `select_in_place`; `file_view/code_surface.rs`
  right-press placement has no test or E2E step that checks the view stays).
- Row 136: re-applied. Proof: runs at Task 8, E2E scenario `diagnostics` (the hover card follows the
  findings after a typed character and an undo). The upstream test
  `tests::stale::a_diagnostic_rides_the_typing_before_it` reads the field, not the getter, so it is not
  cited.
- Rows 137, 138, 139: re-applied. Proof: runs at Task 8, E2E scenario `snapshot` (`ely_banner=false` through
  `draws_read_only_banner`, no Ely banner on a snapshot). The default path (the banner drawn and its
  Unlock) is proven by the upstream tests `tests::a_read_only_editor_ignores_typing_and_undo` and
  `tests::enter_presses_the_banner_button_inside_the_editor`, which ran.
- Rows 140, 141: re-applied. Proof: runs at Task 8, E2E scenario `status_actions` (with tabs Tab inserts a
  tab and Shift-Tab takes it off; Enter after an opener and a block indent in Tabs mode).

## ely-editor performance and sideways scroll (Task 6)

Sirio's performance work and its sideways scroll (rows 142 to 167 of the checklist), carried from the old
crate's `editor` onto upstream's `ely-editor`. Proofs named "runs at Task 8" are Sirio-side: no Sirio
crate uses these crates before Task 8.

- `crates/ely-editor/Cargo.toml`: depends on `sirio_perf` (path `../../../../crates/sirio_perf`, which has no
  dependencies of its own, so no cycle).
- `crates/ely-editor/src/buffer.rs`: the log of the last 32 edits and `Buffer::kept_since(stamp) -> Option<Kept>`; the two
  edit-log tests moved with it.
- `crates/ely-editor/src/syntax.rs`: `BracketIndex`, `Structure` (folds, brackets and the nesting index kept per text
  revision), one-pass `folds`, `opens`, `Structure::enclosing`; `brackets` and the climbing `enclosing` are
  test-only references. Upstream's `colors`, `colors_with`, `styles` and `occurrences` stay. The old inline
  tests moved with it (the bracket index, the one-pass folds, the one-line answers, the nesting index).
- `crates/ely-editor/src/layout.rs`: `rows_are_lines`, `Rows`, `MinimapPlan` and `minimap_plan`; the tests of the shortcut and
  of the minimap sampling moved with them. `Rows::Lines` and `Rows::Listed` carry upstream's typewriter room
  (`Row::Room`): `Lines { lines, pad }` holds `pad` room rows at each end as a count, `Listed` holds them in
  its list, so `primary_row`, `row_of`, `line_from` and `get` give what upstream's `Vec<Row>` gave. `Rows::of`
  takes the pad. `around` is gone (see row 152).
- `crates/ely-editor/src/anchors.rs`: `Reach`, `Marks::{reindex, diagnostics_touching, backgrounds_touching}`; `shift` and
  `clear` re-index; upstream's anchors stay. The two lookup tests moved with it.
- `crates/ely-editor/src/state.rs`: `Marks` holds the two reaches; `set_diagnostics` and `set_backgrounds` re-index;
  `CodeEditor::{folds, brackets, enclosing}` over a `RefCell<Structure>`; `scroll_x`, `scroll_x_max`,
  `followed`, `widest`, `bar_press`; `Metrics` gains `code_left`, `code_width` and `body_left`.
- `crates/ely-editor/src/view.rs`: the `sirio_perf` spans, labels copied from the old crate (`CodeEditor.render`,
  `.active_block`, `.brackets`, `.rows`, `.sideways`, `.sticky`, `.minimap`, `.visible_rows`,
  `.minimap_rows`, `.minimap_paint`); `Frame` holds `rows: Rows`, `brackets: Rc<..>`, `needle` and `active`
  and drops `folds` and `occurrences`; the sticky header asks `enclosing` and shifts with the code;
  `minimap_rows`, the minimap read in prepaint and painted in one `paint_layer`; `needle` skips a
  multi-line selection; `scroll_sideways`, `horizontal_bar` (selector `code-editor-hbar`, thumb follows the
  pointer from the press through `bar_press`), the list in `on_axis` and the body's horizontal wheel;
  `offset_over_code` tests the gutter against `code_left` (this replaces Task 5's pre-#659
  `metrics.left - metrics.advance`). Deviation from the old crate: when no line is wider than the column (`scroll_x_max` is zero) a purely
  horizontal wheel (Shift and the wheel, no vertical delta) scrolls the rows, as gpui does for a list; an event
  with a vertical delta is left to the list, so a trackpad swipe's horizontal drift is not added. Upstream's
  `ely-documents` test `document::tests::scroll::source_margins_scroll` requires the fallback. The wheel is
  read in `metrics.wheel` units, the list's own text style, where the old crate used `window.line_height()`
  at event time: the two differ (78 px against 57 px for three lines in that test), and the sideways branch
  now steps by the same line the rows do. `on_axis` is
  `ely_core::layout::on_axis` (the editor's own `layout`
  module hides the crate-level name).
- `crates/ely-editor/src/line.rs`: brackets by search, the needle looked for on each drawn line, washes and diagnostics by
  range, `x` and `grid` subtract `scroll_x`, the strong guide from `Frame.active`, the code canvas records
  `code_left` and `code_width` and repaints when the width changes.
- `crates/ely-editor/src/row.rs`: the removed-hunk and ghost rows clip and shift by `scroll_x`; the row reads
  `Rows::get` (no index); lens and chat rows do not move.
- `crates/ely-editor/src/gutter.rs`: the worst problem by range, the fold chevron by `syntax::opens`.
- `crates/ely-editor/src/moves.rs`: motions skip the fold scan while nothing is folded; `folds()` from the structure;
  `primary_row` answers the caret line directly while rows are lines (plus the typewriter pad).
- `crates/ely-editor/src/ime.rs`: `measured` is `pub(super)`; the rows are asked by `Rows::row_of`.
- `crates/ely-editor/src/lib.rs`: `mod sideways;`.
- `crates/ely-editor/src/keys.rs`: the Fold key reads the cached `CodeEditor::folds()`.
- `crates/ely-editor/src/sideways.rs` (new): `follow`.

Outcomes:

- Row 142: re-applied. Proof: the closure build (`cargo build -p ely-editor`; the dependency has no
  dependencies, so no cycle).
- Row 143: re-applied. Proof: unproven (spans change nothing a test reads); the labels were compared with
  the old crate's by `diff` of their sorted list: identical.
- Row 144: re-applied. Proofs (ran, each alone, 1 test): `buffer::tests::the_edits_after_a_stamp_leave_a_prefix_and_a_suffix`,
  `buffer::tests::edits_older_than_the_log_are_not_remembered`.
- Row 145: re-applied. Proofs: `syntax::tests::the_bracket_index_follows_every_kind_of_edit`,
  `syntax::tests::a_one_character_edit_reads_only_its_own_line`.
- Row 146: re-applied. Proof: `syntax::tests::one_pass_folds_match_the_pass_they_replace`.
- Row 147: re-applied. Proof: `syntax::tests::one_line_answers_agree_with_the_folds_of_the_whole_text`.
- Row 148: re-applied. Proof: unproven for the folds caching (`an_edit_inside_a_line_does_not_read_the_file_again` exercises row 156's nesting index and cannot fail if `CodeEditor::folds` recomputed every call); the bracket half is covered by row 145's tests.
- Row 149: re-applied. Proof: unproven beyond the closure build; the old crate had no test of the needle on a
  line, and the window tests that draw an editor pass with either form. No Sirio E2E asserts the wash
  of the selected word's other appearances.
- Row 150: re-applied. Proof: `layout::tests::rows_are_lines_only_when_nothing_adds_or_hides_a_row`
  (the shortcut itself). The typewriter pad added to `Rows` is new and unproven. `ely-documents` `look.rs`
  `typewriter_keeps_the_source_caret_mid_view` drives `Rows::Lines { pad > 0 }` but is not a proof: it
  asserts a relative +10 `top_line`, which a wrong pad shifts equally. No test was written after the code.
- Row 151: re-applied. Proof: unproven (no test reads `primary_row` or counts fold scans); the window tests
  that move the caret (`tests::cursors_added_above_keep_climbing`) pass through it.
- Row 152: re-applied as a removal, not dropped. Upstream calls `around` once, from `sticky`, and that
  call is the one row 156 replaces with `enclosing`; with it gone `around` has no caller, as in the old
  crate, so it and its one assertion in `layout::tests` are removed.
- Row 153: re-applied. Proof: `layout::tests::the_minimap_samples_one_row_a_pixel_and_one_more_line_moves_no_sample_far`.
- Row 154: re-applied. Proof: unproven (the picture is the same; the old crate's measurement is
  `docs/perf/2026-10-ely-code-editor.md`).
- Row 155: re-applied. Proofs: `anchors::tests::a_lookup_by_range_finds_what_a_scan_of_every_range_finds`,
  `anchors::tests::marks_look_their_lists_up_in_the_owners_order_and_follow_edits`.
- Row 156: re-applied. Proofs: `syntax::tests::the_nesting_index_agrees_with_climbing_on_every_line`,
  `syntax::tests::the_nesting_index_follows_every_kind_of_edit`.
- Rows 157, 158, 159, 160, 161, 162, 163, 164, 165, 166: re-applied. Proof: runs at Task 8,
  `Scripts/Tests/test-ely-editor-probe.sh` step 7 (the wheel right and Shift with the wheel move line 12
  while the gutter and the rows stay, a drag of the bar's thumb moves it, End and Home follow the caret).
  Unproven: the `code-editor-hbar` selector (no script reads it), the sticky header's shift and the
  deleted-hunk and ghost rows' clip. The wheel fallback added to row 161 (a pure horizontal wheel scrolls the
  rows while nothing is wide) has its proof (ran): `ely-documents` `document::tests::scroll::source_margins_scroll`.
- Row 167: re-applied (`measured` is `pub(super)`). Proof: the closure build (`view.rs` calls it).

## ely-documents host seams (Markdown on ely-documents, rows L1 to L6 and L8 to L10)

Spec `docs/superpowers/specs/2026-11-20-ely-markdown-documents-design.md` §5.2. Sirio parses once,
rewrites the tree and owns selection, links and menus, so the vendored renderer gains opt-in seams.
Each is off unless a host asks; an upstream caller draws exactly as before. Paths: `ely-documents`
`render.rs`, `render/select.rs`, `render/blocks.rs`, `render/table.rs`, `picture.rs`, `tree/mod.rs`,
`lib.rs` and `ely-core` `navigation/toc.rs`. `Node::Image` gained `width`, so its in-crate literals
in `tree/tests.rs` and `tests.rs` read `width: None`. Tests are in `render/tests.rs` (`mod host`),
`tree/tests.rs` and `toc.rs`.

- L1 `TextGather` (trait, re-exported) and `MarkdownRenderer::gather(Rc<dyn TextGather>)`. Ely's private
  `Gather` implements it by delegating to its inherent methods. With a host gather the body is not wrapped
  by `select::selectable` (no `track_focus`, no mouse or key handler, `selectable` ignored), a picture's
  right press is off, and a linked picture takes no `tab_index`, so a press on it moves no focus. Why: Sirio
  owns selection (§5.3). Proofs: `a_host_gather_sees_every_drawn_text_in_order`,
  `a_host_gather_body_takes_no_focus` (it also checks that an Ely selectable body and an unhosted linked
  picture do take focus).
- L2 `parse_markdown(source)` (today's crate-private `parse`) and `parse_markdown_with(source, html_block)`:
  `Reader` carries the hook and `block` asks it for each HTML node, at any depth, replacing it when it
  returns some. Why: Sirio rewrites HTML blocks into blocks (details, images) before drawing. Proof:
  `parse_markdown_with_replaces_html_blocks_at_any_depth`.
- L3 `Node::Image { width: Option<f32> }`; the reader sets `None`; `Picture` draws at `width` logical pixels
  when some, `max_w_full` kept. Why: sized HTML images. Proof: `a_picture_with_a_width_draws_that_wide`.
- L4 `MarkdownRenderer::web_pictures(bool)` (default off): on, `http(s)://` and `//` addresses load through
  gpui's `ImageAssetLoader` with `Resource::Uri`; off, the "Not loaded from the web" note stays. The note is
  wrapped in a block `div` with a debug selector `markdown-picture-web-note` for the test (layout unchanged).
  Why: the web-image policy (§5.5) is the host's. Proof: `web_pictures_off_keeps_the_note_on_drops_it`.
- L5 `MarkdownRenderer::diagrams(bool)` (default on): off, `mermaid` fences go to the `code` hook, else the
  default code path. `ely-chat`'s `StreamingMarkdown` keeps drawing diagrams. Proof:
  `diagrams_off_hands_mermaid_to_the_code_hook` (it also checks that on, the hook is not called).
- L6 `Sections::reveal(&SharedString) -> bool`: the arithmetic of `TableOfContents`' click (the click itself
  is untouched); false when no anchor of that name has drawn. Proofs:
  `reveal_scrolls_a_known_section_to_the_top`, `reveal_of_an_unknown_section_is_false`.
- L8 `MarkdownRenderer::table_frame(Fn(ElementId, AnyElement) -> AnyElement)`: when set, each table is a
  `display: grid` with `grid_cols_max_content(columns)` (columns size to their widest cell and align across
  rows; short rows are padded with empty cells), `flex_none`, drawn at natural width and passed to the frame
  closure, which must place it in a flex row or scroll area (a block parent would stretch it). Cells wear the
  row look themselves (head fill, top border). The grid has debug selector `markdown-table-<n>`. Unset,
  upstream's flex rows are unchanged. Proofs: `a_framed_table_keeps_its_natural_width_and_its_columns`,
  `framed_table_off_keeps_upstreams_wrapping`.
- L9 `MarkdownRenderer::cull(Rc<dyn BlockCull>)`, with `BlockCull` and `BlockUse` (re-exported): with a cull set,
  the body's top-level loop asks `skip(ix, node)` before each block. When it answers `Some((element, use))`
  the loop pushes that element and advances its running counts (element keys, headings, tables) by `use`, so
  every later element id, heading ordinal and table number is what a full draw gives; otherwise the block is
  drawn, the counts it took are reported to `drawn(ix, use)`, and the block sits in a `flex_col` wrapper so the
  wrapper's bounds hold the block's margins (a `Rule` is `my_2`). The body div gets `on_children_prepainted`
  that hands every child's bounds to `placed`. Why: Sirio's chat stands in for the blocks of a long streamed
  reply that sit far outside the window. With no cull the code path is `draw.nodes`, unchanged. Proofs:
  `a_skipped_block_leaves_every_later_id_heading_table_and_push_as_a_full_draw_has_them` (ids of the code and
  table hooks, `markdown-heading-N` and `markdown-cell-T-R-C` selectors and the host gather's pushes, with
  each block skipped in turn),
  `without_a_cull_the_tree_is_the_one_a_never_skipping_cull_lays_out`.
- L10 two readers of what the parser reads, for a host that measures a source before it parses it:
  `markdown_parsed_text(source) -> Cow<str>` (`markdown.rs` `parsed_text`, `markdown/build.rs` `normalised`) is the
  text of the first pass, one leading U+FEFF dropped and each lone CR as LF (it calls the same `lone_crs` `read`
  does); `markdown_accepts_swapped_texts(source, accept) -> bool` (`markdown.rs`, `markdown/tex.rs`
  `accepts_swapped`) hands `accept` the text of each round of the pass that reads `\(` and `\[` pairs as math,
  before it is parsed, and stops at the first refusal (`tex::parse` is now `read` with an always-yes callback;
  nothing else in it moves). Both re-exported from `lib.rs`. Why: a deep tree overflows the stack in the passes
  that recurse over it, so Sirio refuses a source past a cap first, and a guard that reads the source as
  written misses a quote marker behind a byte order mark, a line a lone CR ends, and a closer that the second
  pass hides inside math (`*a \(x*\) ` repeated, then `a* ` repeated, is flat in the first pass and deep in the
  second). Calling Ely's own functions leaves one implementation, so the two cannot drift. No upstream caller
  changes. Proof: Sirio's `markdown::tests::a_leading_byte_order_mark_does_not_hide_nesting`,
  `a_lone_cr_does_not_hide_nesting` and `the_pass_that_reads_parenthesised_math_does_not_hide_nesting`.
- Facade: `ely-gpui-component` re-exports `ely_documents as documents` and forwards `test-support`
  (the facade is outside `check-vendor.sh`).

## Featureless test build

`Scripts/ci.sh` runs `cargo nextest run --workspace` with no features. Upstream always tests with
`test-support`: its own `mod tests` and `init_for_tests` are gated `all(test, feature = "test-support")`.
Six plain `#[cfg(test)]` modules in `ely-git` and `ely-documents` call `init_for_tests`,
`gpui::TestAppContext` or `#[gpui::test]`, which exist only with that feature, so the featureless
build did not compile. Each of the six takes upstream's gate, `#[cfg(all(test, feature = "test-support"))]`,
in place of `#[cfg(test)]`; nothing else in them changes. With test-support on, `test` is on too, so
the modules compiled in a test-support build are the same as before.

Changed paths (each is one attribute line):

- `crates/ely-git/src/rows.rs`: inline `mod tests` (was line 134).
- `crates/ely-documents/src/render/math_text.rs`: inline `mod tests` (was line 306).
- `crates/ely-documents/src/math.rs`: `mod tests;` (was line 285), whose file is `math/tests.rs`.
- `crates/ely-documents/src/modes.rs`: `mod tests;` (was line 491), whose file is `modes/tests.rs`.
- `crates/ely-documents/src/render.rs`: `mod tests;` (was line 540), whose file is `render/tests.rs`.
- `crates/ely-documents/src/render/select.rs`: `mod tests;` (was line 326), whose file is
  `render/select/tests.rs`.

`ely-documents`' `templates.rs` was a seventh and no longer is: upstream already gates each item of its
`mod tests` that uses gpui (`Sized`, `drawn` and the two `#[gpui::test]` functions), so the module
compiles featureless as upstream wrote it, and the file is upstream's again. Its plain test
`templates_keep_by_category_and_query` runs in `ci.sh`.

Why: `ci.sh` runs `nextest --workspace` with no features; upstream always tests with test-support.

Cost: the 16 plain `#[test]` functions in the six modules (2 in `rows.rs`, 1 in `math_text.rs`, 10 in
`math/tests.rs`, 2 in `modes/tests.rs`, 1 in `render/select/tests.rs`) run only under test-support.
No CI job runs the ely crates with test-support (`Scripts/ci-linux.sh` enables it for the gpui copy
only), so `ci.sh` does not run them. The gpui tests in the same modules could not compile featureless
before and still cannot. Moving the gate onto the gpui tests alone, so the 16 run again, is the
follow-up if they must run in CI; it is not done here, because the task fixes the gate on the module.

Outcome: local change, re-decided against the featureless build. Proof: `cd rust && cargo nextest run
-p ely-git -p ely-documents --no-run` and `cargo nextest run -p ely-core -p ely-chat -p ely-agent
-p ely-editor -p ely-collab -p ely-charts --no-run` both exit 0 with no error and no warning; the
second needed no further gate. The featureless run (`cargo nextest run -p ely-git -p ely-documents`, 2026-11-21) is 183 tests, all
passing, none skipped. With `--features test-support`,
`-p ely-palette -p ely-documents -p ely-core` ran 1207 tests on 2026-11-21, none skipped: 1206 passed on the
first run, and the wall-clock budget test `html_block::tests::references_and_long_runs_read_as_a_browser_reads_them`
went red (3.1 s) while another build loaded the machine and passed on its own.

## Timing tests off the featureless build

`Scripts/ci.sh` runs `cargo nextest run --workspace` with no features, so upstream's plain `#[cfg(test)]`
unit tests run in Sirio's gate, and `pr.yml` runs that script. A test whose assertion depends on elapsed
time does not belong there: it fails under load (`html_block`'s 2 s wall-clock budget took 3.3 s in a
full run), and the project keeps timing tests out of its gate. That covers both kinds in `ely-documents`:
the wall-clock budgets (`web_time::Instant`), and the ratio tests that compare two runs' thread CPU time
through `clock::spent` ("four times the input in at most eight times the time"; a loaded host still
moves a CPU-time ratio, through cache and frequency effects). Each such assertion takes
`#[cfg(feature = "test-support")]`, so it compiles only with the crate's `test-support` feature, and
the closure check, which runs with that feature, still runs them. This is not an `#[ignore]`: the
spec's two ignores are gone (see L7).

A test that carries behaviour assertions besides its timing keeps them in the featureless run: the gate
is on the timing statements only, and nothing else in the test changes.

Gated whole (the test is only a timing; one attribute line above its `#[test]`):

- `crates/ely-documents/src/tree/html_block.rs`: `nested_tables_read_in_linear_time` and
  `hostile_html_reads_in_linear_time` (both `clock::spent` ratios).
- `crates/ely-documents/src/tree/sources.rs`: `a_long_paragraph_reads_back_in_one_pass` (a 500 ms
  wall-clock budget; its one other assertion, the offset of the fifth character, is a result of the
  same input).
- `crates/ely-documents/src/tree/emoji.rs`: `long_runs_scan_in_linear_time` (a 2 s budget).
- `crates/ely-documents/src/markdown/build/tests.rs`: `closing_parentheses_trail_off_in_linear_time`,
  `many_literal_links_read_in_linear_time` and `rejected_domains_read_in_linear_time`.

Gated on the timing part only:

- `crates/ely-documents/src/tree/html_block.rs`: `references_and_long_runs_read_as_a_browser_reads_them`
  keeps its entity-decoding assertion (`&notit; &amp &ampx &lt3 &nope;` read as a browser reads it)
  featureless; the 256 KiB `Instant` budget is inside a `#[cfg(feature = "test-support")]` block. The
  test carries no attribute on its function, as upstream has none.
- `crates/ely-documents/src/tree/tests/inline.rs`: `a_close_rebuilds_at_most_three_alike` and
  `a_crossed_close_keeps_marks_nested_and_linear` keep their style and mark assertions; each
  `clock::spent` ratio loop is inside a `#[cfg(feature = "test-support")]` block (in the second the
  `source` closure moved into the block with it).
- `crates/ely-documents/src/markdown/build/tests.rs`: `dollar_fences_inside_open_code_stay_code` and
  `cells_past_the_head_keep_their_text_as_written` keep their tree assertions; the `linear(..)` call at
  the end of each has the attribute as a statement.

Support for the above, in the same files: the `linear` helper and the `spent` import of
`markdown/build/tests.rs` are gated with their callers, and `crates/ely-documents/src/lib.rs` declares
`mod clock;` under `#[cfg(all(test, feature = "test-support"))]` (was `#[cfg(test)]`), because no
featureless test calls `clock::spent` any more and the function would be dead code.

Already behind `test-support`: `document/tests/` (its module is gated, so `format.rs`'s
`many_paragraphs_format_in_linear_time` was never in the featureless build). The other `Instant`
users in the vendored crates (`feedback/clock.rs`'s two tests) build their own instants and assert
exact values; they are deterministic and stay.

Changed paths: the five files above plus `lib.rs`: `tree/html_block.rs`, `tree/sources.rs`,
`tree/emoji.rs`, `tree/tests/inline.rs`, `markdown/build/tests.rs` and `lib.rs`, all under
`crates/ely-documents/src/`.

Why: `ci.sh` runs upstream's plain unit tests featureless, and these assert timings that fail under load.

Outcome: local change. Proof: `cd rust && cargo nextest list -p ely-documents` (no features) lists none
of the seven gated-whole names and still lists the `references_and_long_runs...`,
`a_close_rebuilds_at_most_three_alike`, `a_crossed_close_keeps_marks_nested_and_linear`,
`dollar_fences_inside_open_code_stay_code` and `cells_past_the_head_keep_their_text_as_written` tests;
`cargo nextest run -p ely-documents --features test-support` runs them all, timing parts included.

Also in `markdown/build/`: the repository's `.gitignore` has a bare `build/` rule, which hid this
directory (`node.rs`, `tests.rs`, `text.rs`) from git, so it was on disk and not in any commit. The root
`.gitignore` now un-ignores `rust/vendor/ely/crates/ely-documents/src/markdown/build/`, and the three
files are committed.

## The facade and the switch (Task 8)

`rust/vendor/ely-gpui-component` is now two files, `Cargo.toml` and `src/lib.rs`, a facade over the
vendored crates; its old sources, assets, licences and notes are deleted (they stay in git history at
`aacddd7ec`). It is not a vendored copy of anything, so `check-vendor.sh` does not look at it.

- Chapters `buttons`, `data_display`, `feedback`, `forms`, `layout`, `lists`, `menus`, `motion`,
  `navigation`, `overlays`, `primitives`, `shell`, `theme`, `typography` are `pub use ely_core::<chapter>`
  (`shell` is there because the old crate exposed it; no Sirio code calls it);
  `chat` = `ely_chat`, `agent` = `ely_agent`, `editor` = `ely_editor`; `git` = `ely_git` plus
  `ely_core::lists::GitStatus`.
- `init_chat` (the theme and `forms::bind_keys` only, never upstream's `setup`: its Tab, Shift-Tab and
  F6 bindings stay out, guarded by `ely_ui::tests::ely_init_binds_no_focus_keys`, which passed on the
  old crate and passes after the switch; it binds a `FocusNext` first as a control, to show its filter
  finds a focus binding, and clears the bindings before it runs Sirio's setup) and `init_editor` (checklist row 107: `init_chat`, then
  `editor::bind_keys`, behind a private `EditorKeysBound` global).
- `Assets` serves `ely/icons/<name>.svg` from `IconName::ALL`'s embedded bytes (Task 3's decision:
  `ely-core`'s `assets.rs` stays verbatim). `IconName::path()` is a trait in the facade, `IconPath`
  (an inherent method cannot be added to upstream's type), returning that path; the chat probe imports it.
- `test-support` forwards to the five crates the facade depends on (`ely-core`, `ely-chat`, `ely-agent`,
  `ely-editor`, `ely-git`), for runs by hand: no Sirio manifest enables it. It does not reach
  `ely-documents`, `ely-collab` or `ely-charts`, which the facade does not depend on; the closure
  check runs each crate with `-p <crate> --features test-support`.
- Sirio call sites re-pointed: `CodeEditor::highlighter(..)` and `set_highlighter(..)` of the old
  line-indexed signature became `line_highlighter(..)` and `set_line_highlighter(..)`
  (`file_view/code_surface.rs`, `proposal_tab.rs`, `examples/ely_editor_probe.rs`); the debug selector
  prefix `code-editor-line-` became `editor-line-` (`file_view.rs`, seven uses; no script uses it).

One more hook, found by the switch: `crates/ely-core/src/menus/draw.rs`. Upstream's `draw::menu_level`
sets `debug_selector("menu-row-<label>")` on a row after the row's own builder ran, so it replaced the
name a host gave through `MenuItem::selectors`, and Sirio's `changes-menu-copy-path` and the other
`changes-menu-*` and `change-request-menu-*` names were never drawn. The row now uses its own selector
when it has one, and `menu-row-<label>` otherwise (upstream's tests keep finding their rows). The
hunk clones the row's selector and label (both `SharedString`, a reference count) into the
`debug_selector` closure, which builds `format!("menu-row-{label}")` only when the item has no
selector of its own: the first version built that string for every row on every frame. Proof
(ran): `changes::tests::a_right_click_on_a_file_row_copies_its_path`,
`changes::tests::a_change_requests_row_offers_its_file_on_the_forge` and
`right_panel::change_requests::tests::a_right_click_on_a_row_offers_its_link_to_copy`, red before,
green after, and green again on the lazy form (3 run, 3 passed); upstream's menu tests, 612 `ely-core`
tests with `--features test-support`, pass.

## Row outcomes the sections above do not give

The checklist (`docs/superpowers/plans/2026-11-19-ely-upstream-sync-local-changes-checklist.md`, 234
rows) is gitignored, so each row's outcome is spelled out here. Rows named in a section above keep
that section's outcome; the rest follow. Upstream `8d7723ec` is an ancestor-complete superset of
every upstream-main commit Sirio had ported, so a row whose change was "carry an upstream commit"
is present by construction in the verbatim files.

### Manifest, init and assets (rows 1 to 12, 21, 26, 83)

- Row 1 (the `sirio-chat` default feature and the narrowed dependency set): the feature lives on in
  the facade's `Cargo.toml`; the eight crates are workspace members with `ely-core`'s `default = []`.
  Proof: `cargo build -p sirio`.
- Row 2 (local `mod.rs` files declaring only the closure): dropped. Every upstream module is
  vendored whole and the declared sets are upstream's.
- Row 3 (`init_chat` early return): kept, in the facade's `init_chat` (`has_global::<theme::Theme>()`,
  then `Theme::init` and `forms::bind_keys`). Proof (indirect): `ely_ui::tests::ely_init_binds_no_focus_keys`
  runs the facade's init but asserts row 4, not the early return; no test calls `init_chat` twice.
- Row 4 (no global Tab, Shift-Tab or F6): kept by never calling upstream's `setup()`, which binds them
  under `ElyFocus`. Proof (ran): `ely_ui::tests::ely_init_binds_no_focus_keys` (passed on the old crate
  and passes after the switch).
- Row 5 (no eager asset validation): kept; the facade calls neither upstream's `init` nor
  `assets::missing`, which reading the facade's `lib.rs` shows. Proof (indirect): `ely_chat_probe` in
  Task 8's fix round loads icons through the facade's `Assets`; it does not show the absence of a
  validation pass.
- Rows 6, 7, 8: see "Icons and assets" under the `ely-core` hooks.
- Rows 9, 10, 11, 12 (the image-source helper, the code colours, the palette search helpers, `tint`):
  absorbed. Upstream makes them public where the old crate carried copies
  (`ely_documents::blocks::media::source`, `ely_editor::code_colors`,
  `ely_core::navigation::palette::{Fit, fuzzy, marked}`, `ely_charts::tint`). No Sirio crate calls
  any of them (grep of `rust/crates`). Proof: the closure build.
- Row 21 (what the old crate left out): re-decided. Upstream's tests, i18n and the rest are vendored
  whole and unused.
- Row 26 (a host rule: `ChatContainer` with Sirio's own `ListState`, row ids scoped to the chat entity,
  the transcript generation and the row index): nothing vendored. Proof: none of its own; the
  `ely-chat-ui` and `chat-session-panel` E2Es, which drive the host's rows, ran green at Task 10.
- Row 83 (the engine, bezel-gpui `=0.3.8`): re-decided to `gpui-pre` 0.3.12 by the sync itself (the
  workspace's `gpui` entry); nothing here depends on bezel's engine.

### Git and palette leftovers (rows 61, 85, 86)

- Row 61 (`.clone()` on `Copy` types replaced): not carried, cosmetic; upstream's `.clone()` calls stay,
  except the two in `SyntaxTheme::of`, which are dropped (see the palette split).
- Rows 85, 86 (`GitStatus`): absorbed. `ely_core::lists::GitStatus` is upstream's own; `ely-git` imports
  it, and the facade's `git` chapter re-exports it (`sirio_ui/src/changes.rs` uses it). Proof: the
  variants are identical to the old copy's (Task 7, by `diff`), and `cargo build -p sirio`.

### Compared and not ported (rows 176 to 178, 186 to 206)

All 24 verified against the vendored sources (this directory), not against the checklist's wording.
"Taken" means the file is upstream's, unchanged by Sirio.

- Row 176 (`HoverCard`'s `HoverListenerMode`): taken (`overlays/hover.rs:89,119`); the hover mode is
  row 19, a hand check below.
- Row 177 (`ScrollSource`, the press-blocking track): taken (`layout/scroll/source.rs`,
  `layout/scroll.rs`).
- Row 178 (`Disclosure`, `Collapsible`, `DotBadge`): taken; the differences were documentation only.
- Row 186 (focus handed back past overlays): taken (`primitives/focus.rs`); behaviour not re-run.
- Row 187 (Tab, Shift-Tab and F6 under `ElyFocus`): the code is vendored (`lib.rs:86-90`) and
  deliberately never bound: row 4.
- Row 188 (submenu hover intent, flip left, occlusion): taken (`menus/aim.rs`, `mod aim`).
- Row 189 (`Menu::buttons` rows): taken (`menus/model.rs:195`); Sirio builds none.
- Row 190 (the manual `ContextMenu` mode, its `Styled` impl): taken (`menus/hosts.rs:293,305`).
- Row 191 (the floating option list occludes the wheel): taken (`forms/options.rs:204`).
- Row 192 (comments only): taken.
- Row 193 (code colours per language): taken (`forms/syntax.rs`).
- Row 194 (i18n, documents, git, typewriter, `BlockEditor`, `Retired`, `Held`): all vendored, none used.
- Row 195 (`Role::TextInput`, `aria_label`, `aria_value`): taken (`forms/text/mod.rs:503`).
- Row 196: re-decided toward upstream (see the text field): `select` logs and leaves a bad range
  (`forms/text/mod.rs:489`).
- Row 197 (`forms/files.rs` `file_name`): taken; Sirio has no `FileInput` or `DropZone` caller.
- Row 198 (narrow `ScrollArea`): taken, with row 19's hover mode.
- Row 199 (property rows, segments, shrinking status items): taken; `StatusBarItem` carries F1 below.
- Row 200 (`Breadcrumb`, button and icon-button hunks, the dialog's IME Enter): taken. The dialog's
  Enter is row 17 (press, not release).
- Rows 201, 203, 204, 205 (navigation, forms and buttons tests; cron, units, segmented): taken
  (`navigation/tests/`, `forms/tests/`, `forms/date/cron.rs`, `forms/unit.rs`, `buttons/segmented.rs`,
  `buttons/tests.rs`).
- Row 202 (`collab/remote.rs`, `terminal/paint.rs`): `ely-collab` is vendored whole (`remote.rs`
  present); there is no terminal crate and Sirio has its own terminal.
- Row 206 (`BlockEditor`'s `self.edits`): taken (`forms/text/ime.rs:110`).

### Ports from upstream main (rows 207 to 234)

Every one of these commits is an ancestor of `8d7723ec`, so each is present in the vendored files;
none is a Sirio change. Rows 207 to 211 (the find-box Escape, owner offsets riding edits, `select` and
`edit` in `edit.rs`, `offset_in`, the glob's `inspect_err`), 213 to 226 (the new files, `Metrics::size`,
whitespace, `line_row`, `press` and `drag` taking a window, `mod measure`/`wide`, the text stamp,
`Frame::stamp`, `column`, the `fresh` gate, whole-pixel carets, `shape_text` errors), 227 (an
unfocused one-line field shows its start), 229 (`ime.rs`), 230 and 231 (the tab strip's sideways
scroll and `narrow`), 232 (`Theme::hairline`) and 233 (`slide`'s missing marker) are upstream's.
Row 212 is upstream's `tests/` module taken whole, so the one `stale.rs` test the old crate dropped is
back. Row 228 is in the text field section. Row 234 (a caveat: arrows act only while the
strip holds focus, and there is no Tab traversal) still holds, because the facade never binds Tab:
row 4. Proof for the lot: the closure's own tests (1447 with `--features test-support` at Task 6) and
`check-vendor.sh`, which would flag any local edit to these files.

## Evidence from the switch (Tasks 8 and 10)

"Runs at Task 8" in the sections above means the 36 `sirio_ui` tests below, all green on the
facade, plus the suites of the three crates `-p sirio_ui -p sirio -p sirio_theme` (2435 run, 2435 passed after
the round-1 fixes: 1838 in `sirio_ui`, 556 in `sirio`, 41 in `sirio_theme`). Names, as
`cargo nextest` prints them:

- `chat::band_tests`: `a_blocked_fast_row_is_disabled`, `composer_band_spans_the_chat_column_on_one_row`,
  `narrow_row_fits_from_its_measured_minimum`, `the_background_tasks_row_is_informational`.
- `chat::ely_tests`: `ely_composer_enter_respects_completion_and_ime`,
  `ely_composer_sends_an_attachment_without_text`, `ely_expired_request_cannot_answer_the_next_request`,
  `ely_request_sends_only_the_advertised_option_id`,
  `ely_tool_and_thought_expansion_survives_virtualization`.
- `chat::tests`: `a_run_is_one_box_and_same_verb_calls_fold`, `a_subagent_task_card_expands_nested_tool_calls`,
  `attach_control_accepts_one_image_and_rejects_the_rest`, `enter_sends_and_shift_return_inserts_a_newline`,
  `the_composer_answers_the_editing_chords_of_its_platform`,
  `the_composer_moves_the_caret_a_row_with_its_platforms_keys`.
- `chat::composer_height::tests::every_open_chat_takes_a_new_maximum`,
  `chat::session_panel::trace::tests::the_card_asks_for_no_frame_but_its_own`.
- `ely_ui::tests`: `a_delete_to_line_end_is_one_undo_step`,
  `a_detached_field_first_drawn_in_an_inactive_window_reports_focus_once_it_activates`,
  `a_detached_field_focused_before_its_first_frame_reports_focus_and_types`,
  `a_host_binding_registered_after_ely_wins_and_can_hand_the_key_on`,
  `a_host_binding_still_wins_in_a_multi_line_field`, `a_long_multi_line_draft_scrolls_under_the_wheel`,
  `a_wheel_scrolls_a_draft_alone_and_reaches_the_page_at_its_limit`,
  `a_wheeled_draft_follows_the_caret_again_on_the_next_edit`,
  `ctrl_n_and_ctrl_p_do_not_move_a_single_line_field_the_way_down_and_up_do`,
  `only_a_multi_line_field_names_the_multi_line_context`, `reset_text_starts_a_fresh_history`,
  `the_delete_to_line_and_word_actions_delete_what_bezels_did`, `ely_init_binds_no_focus_keys`.
- `ely::tests::a_theme_swapped_without_an_installer_reaches_both_consumers`.
- `file_view::tests`: `escape_closes_find_then_a_hunk_and_leaves_the_cursors_until_none_is_open`,
  `revealing_a_line_scrolls_it_into_view`,
  `the_pointer_sliding_from_the_code_onto_the_find_bar_drops_the_card`.
- `settings::agents_page::tests::the_agents_search_keeps_its_row_inside_its_frame_at_every_interface_size`,
  `project_forms::tests::the_project_name_field_tracks_font_preferences_and_keeps_native_editing`.

Task 10 ran the framed E2Es and capture sweeps against the pre-sync tree and this one and read every
pair that differs (the method and its limit are in `differences.md`, which the PR description carries). Several rows above say
"unproven" or "runs at Task 8" where no test asserts the output; Task 10 closed them by capture
(the output is on screen in a green run, which is weaker than an assertion):

- Proven by capture: rows 13 (dialog text colour), 25 (`Alert::content`), 54 (`row_frame`, the effort
  list), 22, 29, 30, 31, 32, 33, 35, 36, 44, 45, 171 (the chat captures: avatars, tool groups, thinking
  label and glyph, send and stop, permission selectors, composer colours and hint, a long system
  line), 47 to 57 and 172 to 175 (the composer menus), 70 (`composer_resize`), 93 to 96, 98, 116, 128,
  132, 135 and 148, 149, 151, 154 (editor E2E scenarios and frames), the horizontal bar selector and
  the sticky and hunk clipping (probe step 7 and frames).
- Row 107 (`init_editor`) is proven: the 47 editor-e2e scenarios, `ely_editor_probe`, and
  `ely_init_binds_no_focus_keys`.
- Row 6, facade side: the draw path is proven (every icon in every capture, and `ely_chat_probe`
  through `Assets::icon`). `Assets::list` and the load-miss branch never run (no in-tree caller of
  either), so a missing icon would show as an empty box.

Hand checks owed (nothing automated holds them; do not read these rows as proven):

- Row 19, hover mode: focus the terminal, move the mouse over a scroll bar, a tooltip trigger and a
  hover card without clicking. Each must react, the tooltip after its delay.
- Row 84, compact tabs: on the change-requests tab, drag the right panel narrow until the tabs lose
  their labels. A tab with no label must stay icon-only.
- Row 81, on a Mac: Ctrl-K in a chat composer and in the file editor deletes to the line end; in a
  terminal it still reaches readline.
- Rows 75 to 77, on a Mac: the composer answers `cmd-backspace`, the emacs chords and `ctrl-n` and
  `ctrl-p` (the platform's tests run only in the release gate).
- Row 124: in the editor press the mouse in text, drag out of the window, release outside and come
  back. No selection follows the pointer.
- Row 143: with the perf trace on, `CodeEditor.render` spans appear and carry no text.
- Typewriter room (new code in `Rows`): Settings, Editor, turn typewriter scrolling on and type below
  the fold. The caret line stays about a third from the bottom.
- Dialog Enter (row 17): open the discard dialog and hold Enter. It confirms once, not twice.
- The horizontal wheel (row 161): in a short file (no line wider than the column) a purely
  horizontal wheel scrolls the rows; an event with a vertical delta is left to the list.
- On Mac and Windows, the owed checks of the PR: composer chords, a long draft under the wheel, the
  effort list, a dialog's Enter, and the editor's sideways scroll, hard tabs and find.

## Dropped tests of the old crate

Two Sirio-local tests of the old `ely-gpui-component` have no counterpart in the new tree.

- `primitives::icon::tests::every_icon_ships_in_the_bundle` (every `IconName` loads through the crate's
  `Assets` under `ely/icons/<name>.svg`). Dropped because the facade's `Assets` is now derived from
  `IconName::ALL` itself (`load` looks the name up in that list and returns the icon's embedded bytes),
  so every icon is served by construction and the old check, written against a `rust-embed` folder that
  could miss a file, would assert what the code does. What covers the icon files themselves is
  upstream's `primitives::icon::tests::every_icon_carries_its_own_lucide_svg`. Nothing tests the facade's
  `Assets::load` or `list` directly: `ely_chat_probe` draws icons through `Assets::icon`, and the load-miss
  branch and `list` never run in-tree (no caller).
- `theme::palette_copy_tests::palette_and_syntax_are_copy` (`Palette` and `Syntax` are `Copy`). Dropped
  because the compiler holds it: `sirio_theme`'s `ThemeColors` derives `Copy` over a `Palette` field and
  `SyntaxTheme::of` returns `Syntax` by value from `&self`, so neither compiles if the type stops being
  `Copy`.

## Visible changes accepted by the sync

Everything on screen or under the fingers that the sync changed, each traced to upstream or to a row
this file re-decided. Enzo accepted F1, F2 and D1 on 2026-11-20 (through the coordinator); F1 is
accepted as upstream ships it, and he can still ask for an override in the facade or in `ely-core`.
The frames and the method are in the sync's own evidence (`artifacts/ely-sync/`, not committed).

- F1, status bar items 2 px wider and 2 px taller: upstream `2d827752`
  (`crates/ely-core/src/shell/bars.rs`, `StatusBarItem::render`) gives a clickable item a
  transparent 1 px border and a focus ring. Every editor status row's text sits 1, 3 and 5 px further
  left, and the popover or list above an item opens 1 px higher. Proven by a revert (17 of 23 frames
  identical to before). The same commit makes the item a gpui Tab stop (`tab_index(0)`). Nothing in
  Sirio moves focus onto it by key, so the ring is never drawn, only the border: gpui-pre has no
  built-in Tab handling (it only runs the `focus_next` and `focus_prev` the host dispatches), no Sirio
  binding calls them (`ely_init_binds_no_focus_keys`), and the item's mouse-down calls
  `prevent_default`, so a click does not take focus. The only code in Sirio that dispatches `FocusNext`
  is `file_view/status_actions.rs` (two sites, one for the list popovers and one for the bell), and it
  dispatches from the popover's own panel, a trapping `FocusScope`: its step advances until the focus is
  inside the panel and puts it back where it started when the panel has no stop, so a status bar item
  cannot keep it. The override, if wanted, is two lines in `ely-core` or the facade.
- F2, tooltips in the theme font: `crates/ely-core/src/primitives/tooltip.rs:30` sets the theme's font
  family. A tooltip's label is a little narrower and a long one can wrap at another word.
- D1, a dialog focuses its first control: its focus ring or caret shows a frame after opening, and
  whatever sat behind loses its focus look (spec 5.4's re-decided row 4, upstream's change log).

The rows that change behaviour and are not a frame difference (the list the spec's §6.4 asks to go to Enzo):

- Dialog Enter confirms on press, not on release (row 17). Hand check: open the discard dialog and hold
  Enter; it confirms once.
- Hover reacts without a click: scroll bars, tooltips and hover cards take upstream's
  `HoverListenerMode::InputModalityIndependent` (row 19). Hand check: row 19 below.
- `ely-palette`'s dark defaults (`bg` `#252525` instead of `#131110`, and the greys near it) and the chart
  hues changed (the palette split). No Sirio draw path reads them, and no dark capture differs for them.
- The stop button draws the same variant as send (row 33): `Primary` by default, where upstream's stop was
  `Secondary`. Sirio always sets the variant through `PromptInput::custom` (`Subtle`), so nothing changes
  in Sirio; a host that does not call `variant` would see it.
- The editor's right press runs upstream's `secondary_press` (focus, caret, keeps a selection it lands in)
  before Sirio's `SecondaryClick` handling. The end state matches the old crate, so it is not visible.
- A pure horizontal wheel scrolls the rows when no line is wider than the column
  (`ely-editor/src/view.rs`, row 161): in a short file Shift and the wheel move the list vertically,
  where the old crate did nothing. An event with a vertical delta is left to the list. Hand check: open
  a short file and wheel it sideways.
- The sideways wheel step on a file that does have wide lines changed: it now reads the wheel in
  `metrics.wheel` units (the list's own text style) where the old crate used `window.line_height()` at
  event time (`view.rs`, the `on_scroll` handler). Three wheel lines moved about 78 px against 57 px
  (26 px a line against 19), so sideways scrolling is faster than before and steps by the same line the
  rows do.

Hand checks owed in the PR, not run here: on a Mac and on Windows, the composer chords, a long draft
under the wheel, the effort list, a dialog's Enter, and the editor's sideways scroll, hard tabs and find;
and on a Mac, whether the blurred window looks as before (`rust/vendor/README.md`, the blend factor).
