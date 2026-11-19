# Upstream Ely, vendored verbatim

- Source: https://github.com/ELY-Open-Source/Ely-GPUI-Components
- Rev: `8d7723ec` (never moved)
- Crates: `ely-core`, `ely-chat`, `ely-agent`, `ely-editor`, `ely-git`, `ely-documents`,
  `ely-collab`, `ely-charts`, under `crates/`.
- File set taken from each crate: `Cargo.toml`, `src/`, and `icons/`, `assets/` and `LICENSE` where
  present. Never `AGENTS.md` or `examples/`. `LICENSE` at this directory's root is upstream's.
- Nothing in Sirio depends on these crates yet.
- Check: `artifacts/ely-sync/drivers/check-vendor.sh` extracts each crate at the rev, diffs it
  against this directory and fails on any differing path not named below. It prints
  `VENDOR CHECK OK`.

## Changed paths

Every path below is a path the check accepts as different from upstream. Anything else differing
is a failure.

### Manifests (all eight)

`crates/<crate>/Cargo.toml` for each crate:

- `x.workspace = true` becomes upstream's workspace value, literally (package fields, third-party
  dependencies). `[lints]` tables are dropped. `publish = false` is added.
- `gpui` and `gpui_platform` stay `workspace = true`, now Sirio's workspace entries. Each crate gains
  a `[target.'cfg(target_os = "linux")'.dependencies]` section enabling gpui's `wayland` and `x11`,
  the same section the old `ely-gpui-component` carries.
- `ely-core`: `default = []` (was `["fonts"]`), and the `five_icons` `[[example]]` is dropped with
  `examples/`.

- `crates/ely-core/Cargo.toml`
- `crates/ely-chat/Cargo.toml`
- `crates/ely-agent/Cargo.toml`
- `crates/ely-editor/Cargo.toml`
- `crates/ely-git/Cargo.toml`
- `crates/ely-documents/Cargo.toml`
- `crates/ely-collab/Cargo.toml`
- `crates/ely-charts/Cargo.toml`

### Ignored tests

Two `ely-documents` tests of upstream's `e71b644b` need upstream's patched pulldown-cmark. Each
gains `#[ignore = "needs upstream's patched pulldown-cmark; step 2"]` and nothing else.

- `crates/ely-documents/src/markdown/tex/tests.rs`
- `crates/ely-documents/src/tree/tests/inline.rs`

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
extraction); upstream's `.clone()` calls stay (row 61 is cosmetic and not carried here).

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
- Row 7, re-decided: Inter and JetBrains Mono are not bundled: `ely-core`'s `fonts` feature is off by
  default here (see Manifests), and Sirio's facade does not call upstream's `init`. IBM Plex Sans (for
  svg text) stays, as upstream always embeds it.

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

## Featureless test build

`Scripts/ci.sh` runs `cargo nextest run --workspace` with no features. Upstream always tests with
`test-support`: its own `mod tests` and `init_for_tests` are gated `all(test, feature = "test-support")`.
Seven plain `#[cfg(test)]` modules in `ely-git` and `ely-documents` call `init_for_tests`,
`gpui::TestAppContext` or `#[gpui::test]`, which exist only with that feature, so the featureless
build did not compile. Each of the seven takes upstream's gate, `#[cfg(all(test, feature = "test-support"))]`,
in place of `#[cfg(test)]`; nothing else in them changes. With test-support on, `test` is on too, so
the modules compiled in a test-support build are the same as before.

Changed paths (each is one attribute line):

- `crates/ely-git/src/rows.rs`: inline `mod tests` (was line 134).
- `crates/ely-documents/src/templates.rs`: inline `mod tests` (was line 351).
- `crates/ely-documents/src/render/math_text.rs`: inline `mod tests` (was line 306).
- `crates/ely-documents/src/math.rs`: `mod tests;` (was line 285), whose file is `math/tests.rs`.
- `crates/ely-documents/src/modes.rs`: `mod tests;` (was line 491), whose file is `modes/tests.rs`.
- `crates/ely-documents/src/render.rs`: `mod tests;` (was line 540), whose file is `render/tests.rs`.
- `crates/ely-documents/src/render/select.rs`: `mod tests;` (was line 326), whose file is
  `render/select/tests.rs`.

Why: `ci.sh` runs `nextest --workspace` with no features; upstream always tests with test-support.

Cost: the 17 plain `#[test]` functions in these modules (2 in `rows.rs`, 1 in `templates.rs`, 1 in
`math_text.rs`, 10 in `math/tests.rs`, 2 in `modes/tests.rs`, 1 in `render/select/tests.rs`) now run
only under test-support. No CI job runs the ely crates with test-support (`Scripts/ci-linux.sh`
enables it for the gpui copy only), so `ci.sh` no longer runs them. The gpui tests in the same
modules could not compile featureless before and still cannot. Moving the gate onto the gpui tests
alone, so the 17 run again, is the follow-up if they must run in CI; it is not done here, because the
task fixes the gate on the module.

Outcome: local change, re-decided against the featureless build. Proof: `cd rust && cargo nextest run
-p ely-git -p ely-documents --no-run` and `cargo nextest run -p ely-core -p ely-chat -p ely-agent
-p ely-editor -p ely-collab -p ely-charts --no-run` both exit 0 with no error and no warning; the
second needed no further gate. The featureless run of `ely-git` and `ely-documents` (`--no-fail-fast`)
runs 186 tests: 185 pass, one fails, and 2 are ignored (the step-2 pair). The failure is
`tree::html_block::tests::references_and_long_runs_read_as_a_browser_reads_them`, a 2 s wall-clock
budget for 256 KiB of `&`: it took 3.3 s in the full run and 2.6 s alone under load, and 1.97 s alone
at `--test-threads 2`, so on this machine it sits at its budget and `ci.sh` may fail on it. That test
is upstream's own and is not touched here. With `--features test-support` the same two crates run 617
tests, all passing (2 ignored), that test included (1.59 s).

## Wall-clock tests off the featureless build

`Scripts/ci.sh` runs `cargo nextest run --workspace` with no features, so upstream's plain `#[cfg(test)]`
unit tests run in Sirio's gate. Three of them in `ely-documents` assert a wall-clock budget, timed with
`web_time::Instant::now()` and `elapsed()`, and fail under load: `html_block` took 3.3 s against its 2 s
budget in the full run. The project keeps wall-clock tests out of its gate. Each of the three test
functions takes `#[cfg(feature = "test-support")]` above its `#[test]`, so it compiles only with the
crate's `test-support` feature. The closure check runs with `--features test-support`, so it still runs
them. This is not an `#[ignore]`: the spec's two ignores (see "Ignored tests" above) are untouched.

Changed paths (one attribute line each, on the test function only):

- `crates/ely-documents/src/tree/html_block.rs`: `tree::html_block::tests::references_and_long_runs_read_as_a_browser_reads_them`.
- `crates/ely-documents/src/tree/sources.rs`: `tree::sources::tests::a_long_paragraph_reads_back_in_one_pass`.
- `crates/ely-documents/src/tree/emoji.rs`: `tree::emoji::tests::long_runs_scan_in_linear_time`.

Why: `ci.sh` runs upstream's plain unit tests featureless, and these three assert wall-clock budgets that
fail under load. The crate's other timing tests go through `clock::spent`, which reads the thread's CPU
time, not the wall clock, so they stay as they are.

Outcome: local change. Proof: `cd rust && cargo nextest list -p ely-documents` (no features) lists none
of the three names, and `cd rust && cargo nextest run -p ely-documents --features test-support -E
'test(=<full::path::name>)' --test-threads 2` runs each one, exactly once.
