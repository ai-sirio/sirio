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
  `chat::ely_tests::ely_tool_and_thought_expansion_survives_virtualization` (rows 28, 29 header
  selector and host-owned state), `chat::ely_tests::ely_request_sends_only_the_advertised_option_id`
  (rows 34, 35, 36: asserts `ely-request-always` is absent in custom mode);
  `chat::ely_tests::ely_expired_request_cannot_answer_the_next_request` (row 34). The `status` of
  row 29 has no test of its own (unproven); the built-in `PermissionPrompt::new` and
  `HumanInputRequest::new` paths are proven by the ran tests above
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
