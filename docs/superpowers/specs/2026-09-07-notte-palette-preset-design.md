# "Notte" palette preset — design

Date: 2026-09-07
Status: approach approved in brainstorming (option B of three), spec pending user review

## Problem

The request is a new theme built on four colours:

| Given | oklch L | oklch C | oklch H |
|---|---|---|---|
| `#0E1016` | 0.174 | 0.013 | 270.6 |
| `#202127` | 0.249 | 0.011 | 278.0 |
| `#2B2F3A` | 0.306 | 0.021 | 269.4 |
| `#313337` | 0.321 | 0.008 | 264.5 |

Two facts, measured rather than assumed, set the shape of the work.

**The hue is nothing new.** At 264–278° and chroma 0.008–0.021 the four sit
between bezel's Zinc (285.9, 0.016) and Gray (264.4, 0.027) base colours
(`BASE_COLORS`, `bezel-theme-0.1.4/src/brand.rs`).

**The lightness is.** bezel's dark ladder is `bg #060606` (L 0.122),
`surface #0D0D0D` (0.159), `surface_overlay #161616` (0.200),
`surface_raised` (0.235). The given ladder starts where bezel's ends: its page
is lighter than bezel's raised card. `Brand::apply` rotates hue only —
"lightness is never a knob" (`2026-08-31-bezel-base-colors-design.md`) — so
neither the five shipped base colours nor a sixth tint can produce these four
values. The request is not a tint; it is a **different surface ladder**, and
that needs a preset that substitutes the ladder while keeping everything else
bezel's.

Three options were put to the user:

- **A — a sixth tint.** ~30 lines, stays inside bezel's contract, does not
  deliver the four colours. Rejected for that reason.
- **B — a preset ladder.** One override point, the four values verbatim,
  everything else unchanged. **Chosen.**
- **C — a user theme file.** All ~40 bezel tokens editable from disk.
  Rejected: nothing asked for it, and it breaks the discipline that the
  bezel pin protects appearance.

## Decisions

### N1 — A sixth `BaseColor` variant, not a new axis

`BaseColor::Notte` joins `ALL` last, after Slate, in both enums (`sirio_theme`
and `sirio_persistence`), with raw string `"notte"`. The two conversions in
`sirio/src/main.rs` gain an arm each; `SEGMENTED_BASE_COLOR` in
`sirio_ui/src/settings.rs` gains `"Notte"`.

The Appearance page already asks "which greys?"; a preset is another answer to
that question, not a second question. A separate "preset" setting would have
to explain why picking it greys out the base colour, and the persistence
contract would need a second key. Neither is worth it for one preset.

The name is a placeholder the user may rename during spec review. It is
user-visible in the picker and stored as the raw string, so it is decided
once, here.

### N2 — The override lives in `bezel_theme_for`, the one builder

`bezel_theme_for(base, appearance)` (`sirio_theme/src/lib.rs:254`) builds
`Theme::branded(&Brand { tint: base.tint(), .. }, appearance)`. For
`Notte` in `Dark`, and only then, it overwrites the surface ladder of the
branded theme before returning it:

| bezel token | value | who reads it |
|---|---|---|
| `bg` | `#0E1016` | `ThemeColors::bg`; `frame_surface` (softened to 0.85) |
| `surface` | `#202127` | `surface`, `dialog_surface`, `terminal_surface` (dark), the target `text` is softened toward |
| `surface_card` | `#202127` | bezel widgets — a card sits on the page, as in bezel's own dark |
| `surface_raised` | `#2B2F3A` | `ThemeColors::raised` |
| `surface_dialog` | `#2B2F3A` | bezel dialogs |
| `surface_overlay` | `#2B2F3A` | bezel overlays and popovers |
| `surface_raised_hover` | `#313337` | bezel raised-element hover |

Why this one point: `Theme::to_bezel_theme` calls the same builder, and
`install_into_bezel` hands its result to `bezel::theme::Theme::install_custom`,
which installs the given theme verbatim (`install.rs:50`). So bezel-drawn
widgets and Sirio's own `ThemeColors` see one ladder by construction, and
there is no second table to keep in step.

Everything that is not in the table stays what `branded` produced:

- The veils — `element_hover`, `element_active`, `border`, `border_strong`,
  `input_bg`, `code_wash`, `ring`, `selection` — are white-alpha washes and
  compose over the new surfaces exactly as they compose over bezel's.
- The text ladder, `solid`/`on_solid`, `accent` and the syntax palette are
  bezel's, carrying Notte's tint like any other grey.
- The semantic hues (danger, warning, success, diff) are untouched.

**Notte's tint** is measured from the four values, not quoted from Tailwind:
the mean of their oklch hue and chroma, `Tint::new(270.6, 0.013)`, written as
a constant with the derivation in a comment. This is what tints the text and
plates so they share the family of the surfaces they sit on. Gray's numbers
were the alternative; they were rejected because the four colours are the
user's own quote and the tint should come from the same source as the ladder.

### N3 — Notte is dark-only; Light renders as its tint

The user supplied four dark surfaces and no light ones. In `Light`,
`bezel_theme_for(Notte, Light)` applies the tint and no override, so the app
is bezel's light palette in Notte's hue family. `System` therefore swaps
between the Notte ladder (dark) and that tinted light palette. Inventing a
light ladder nobody asked for was rejected.

### N4 — What does not move

- `brand_coral` (decision B4 of the base-colours spec).
- Borders: veils, verbatim (`a_tinted_base_moves_the_greys_and_leaves_sirios_own_colours_alone`).
- Semantic hues: `the_semantic_hues_hold_under_every_base_colour` iterates
  `ALL` and covers Notte with no change.
- `terminal_surface` in dark equals `surface`, so the terminal well is
  `#202127`; `dark_terminal_surface_matches_the_pane_surface` iterates `ALL`.
- `TEXT_SOFTENING` stays 10% toward the surface. Body text on `#202127` and
  `#2B2F3A` clears WCAG AAA (roughly 10.5:1 and 9:1); the exact figures are
  recorded in the provenance doc when measured by the test below.
- Radii, spacing, typography, translucency, the frame material.

### P1 — Persistence: one new raw string, no migration

`"notte"` round-trips through `raw`/`parse`. A downgrade reads it back as
`None` and falls to `Neutral` (`db.rs:930`), which is the same behaviour every
unknown string has today.

### U1 — The picker grows to six segments

`base_color_segment` derives from `ALL`, so it needs no change. Whether six
segments fit the Theme card at the narrowest Settings width is verified on
screen; if they wrap, the fix belongs to the segmented control, not to this
work.

## Provenance

`docs/THEME-PROVENANCE.md` gains a subsection under category 1, "Preset
ladders — Sirio's own": the four values, their oklch, the mapping table above,
and the statement that this is the one place lightness moves and why. The
"three things do not rotate" paragraph is unchanged; the tests that pin the
Neutral palette against bare `Theme::dark()`/`light()` are unchanged.

## Testing — tests first

`sirio_theme` (`lib.rs` tests, next to the base-colour ones):

- `notte_dark_ladder_is_the_four_given_values` — the seven bezel tokens in the
  N2 table equal their hexes, read through `Theme::for_appearance(Dark, Dark,
  Notte).to_bezel_theme()` so the test covers the path bezel widgets use.
- `notte_light_is_only_a_tint` — the light Notte theme equals
  `Theme::branded(&Brand { tint: Notte.tint(), .. }, Light)` token for token.
- `notte_keeps_bezels_veils_text_and_hues` — hover, active, border, text
  ladder, solid, accent and the semantic hues equal the branded values.
- `notte_body_text_clears_aaa_on_every_surface` — `text` against `bg`,
  `surface`, `raised` and `surface_raised_hover` is at least 7:1.
- `notte_depth_ladder_reads_as_depth` — relative luminance strictly
  increases `bg` < `surface` < `surface_raised` < `surface_raised_hover`.
- The loops over `ALL` (`dark_terminal_surface_matches_the_pane_surface`,
  `the_semantic_hues_hold_under_every_base_colour`) pick Notte up unchanged.

`sirio_theme/src/base_color.rs`:

- `all_lists_every_variant_in_display_order` gains Notte at the end.
- `the_four_tinted_families_carry_bezels_own_numbers` is unchanged — Notte
  is not one of bezel's.
- `notte_tint_is_the_mean_of_its_own_ladder` — recomputes the mean oklch
  hue/chroma of the four hexes and compares to the constant within 0.05.

`sirio_persistence`: the `raw`/`parse` round-trip test gains the variant.
`sirio` (`main.rs`): the conversion-table test gains a row.
`sirio_ui` (`settings.rs`): the `SEGMENTED_BASE_COLOR` test gains `"Notte"`.

Manual, after `Scripts/build-dev.sh`: Settings → Appearance in dark with
Notte selected; sidebar, tab strip, a terminal pane, a dialog, a popover;
then switch to Light and back. Six segments fit or are reported.

## Files touched

- `rust/crates/sirio_theme/src/base_color.rs` — variant, tint, title, tests
- `rust/crates/sirio_theme/src/lib.rs` — override in `bezel_theme_for`, tests
- `rust/crates/sirio_persistence/src/model.rs` — variant, raw/parse, test
- `rust/crates/sirio/src/main.rs` — two conversion arms, test row
- `rust/crates/sirio_ui/src/settings.rs` — one label, test
- `docs/THEME-PROVENANCE.md` — the preset subsection

## Risks

- **Hover barely reads.** `#313337` is only 0.015 L above `#2B2F3A` and less
  chromatic. That is the user's ladder and is shipped as given; the depth test
  asserts only the ordering, not a minimum step.
- **Syntax colours were tuned on `#0D0D0D`.** bezel's dark syntax palette is
  kept as is; a lighter page lowers its contrast a little. Checked on screen,
  not by a test, because no threshold was asked for.
- **Six segments.** See U1.
- **A bezel bump that adds surface tokens** would leave them at bezel's
  lightness inside Notte. The provenance rule already treats a bump as a
  visual review; the N2 table is the checklist for that review.
