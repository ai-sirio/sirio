# Notte palette preset — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a sixth base colour, "Notte", whose dark appearance paints the
window on the four given surfaces `#0E1016 · #202127 · #2B2F3A · #313337`
while everything else stays bezel's.

**Architecture:** `BaseColor` gains a `Notte` variant in both the theme and the
persistence enums, mirrored by the two conversions in `main.rs` and one label in
the Settings picker. The only place the four colours are painted is
`bezel_theme_for` in `sirio_theme`, which builds the branded bezel theme as today
and, for Notte in dark only, overwrites its seven surface tokens. Because
`to_bezel_theme` and `ThemeColors::for_appearance` both go through that builder,
bezel's own widgets and Sirio's tokens see one ladder by construction.

**Tech Stack:** Rust, gpui (`bezel-gpui =0.3.8`), `bezel =0.1.4`
(`bezel::theme::Theme`, `Brand`, `Tint`), standard `#[test]`.

**Spec:** `docs/superpowers/specs/2026-09-07-notte-palette-preset-design.md`

## Global Constraints

- bezel stays pinned `=0.1.4`; no dependency changes.
- The preset's name is `Notte` everywhere it is visible; its persisted raw
  string is `"notte"`; no new settings key and no migration.
- `Notte` is the last entry of `BaseColor::ALL` (after `Slate`) in the theme
  enum, and the last variant of the persistence enum.
- Notte's tint is `Tint::new(270.6, 0.013)` — the mean oklch hue and chroma
  of the four given colours.
- Light appearance never applies the ladder: Notte in light is
  `Theme::branded` with Notte's tint and nothing else.
- Every command runs from the repository root; `cargo` commands from `rust/`.
  `cargo test -p sirio` builds `sirio_terminal`, which needs Zig exactly
  0.15.2 on PATH. `sirio_theme`, `sirio_persistence` and `sirio_ui` do not.
- Never run `Scripts/ci.sh` or `Scripts/ci-linux.sh`; the user launches those.
- Commit messages: Conventional Commits, lower-case imperative subject, and
  end every message with the two trailer lines shown in each commit step.
- The `sirio` crate has a known set of pre-existing failing tests on main;
  run the named tests only, and compare failure *lists* against main, never
  counts.

---

### Task 1: The persisted raw string

**Files:**
- Modify: `rust/crates/sirio_persistence/src/model.rs:406-436` (enum, `raw`, `parse`)
- Test: `rust/crates/sirio_persistence/src/model.rs:673-696`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `sirio_persistence::BaseColor::Notte`, with
  `BaseColor::Notte.raw() == "notte"` and `BaseColor::parse("notte") == Some(BaseColor::Notte)`.
  Task 2 matches on it in `main.rs`.

- [ ] **Step 1: Extend the round-trip test**

In `rust/crates/sirio_persistence/src/model.rs`, replace the test
`base_colour_round_trips_through_its_raw_value` with:

```rust
    #[test]
    fn base_colour_round_trips_through_its_raw_value() {
        for base in [
            BaseColor::Neutral,
            BaseColor::Stone,
            BaseColor::Zinc,
            BaseColor::Gray,
            BaseColor::Slate,
            BaseColor::Notte,
        ] {
            assert_eq!(BaseColor::parse(base.raw()), Some(base), "{base:?}");
        }
        assert_eq!(
            BaseColor::raw(BaseColor::Neutral),
            "neutral",
            "the raw values are lower-case, like every other enum setting"
        );
        // The preset is stored under the same lower-case rule; an older
        // build reads "notte" as unknown and falls back to Neutral in the
        // loader, which is the downgrade path every unknown string has.
        assert_eq!(BaseColor::Notte.raw(), "notte");
    }
```

- [ ] **Step 2: Run it and watch it fail to compile**

Run: `cd rust && cargo test -p sirio_persistence base_colour_round_trips_through_its_raw_value`
Expected: compile error `no variant or associated item named 'Notte' found for enum 'BaseColor'`.

- [ ] **Step 3: Add the variant, its raw string and its parse arm**

In the same file, change the enum:

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BaseColor {
    #[default]
    Neutral,
    Stone,
    Zinc,
    Gray,
    Slate,
    /// Sirio's own preset: bezel's greys in a cool hue, on a lighter dark
    /// surface ladder. Not one of bezel's five, so it sits last.
    Notte,
}
```

Add the arm to `raw`:

```rust
            BaseColor::Slate => "slate",
            BaseColor::Notte => "notte",
```

Add the arm to `parse`, before the `_ => None` fallback:

```rust
            "slate" => Some(BaseColor::Slate),
            "notte" => Some(BaseColor::Notte),
            _ => None,
```

- [ ] **Step 4: Run the crate's tests**

Run: `cd rust && cargo test -p sirio_persistence`
Expected: all pass, including `base_colour_round_trips_through_its_raw_value`
and `an_unknown_base_colour_is_not_parsed`.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_persistence/src/model.rs
git commit -m "feat(persistence): accept the notte base colour

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_016Gz9nyKfNg9BB8cJ6Dtbcu"
```

---

### Task 2: The variant, its tint, and the plumbing that names it

After this task every crate compiles with the sixth variant, the picker shows
it, and choosing it behaves exactly like choosing Gray — the ladder comes in
Task 3.

**Files:**
- Modify: `rust/crates/sirio_theme/src/base_color.rs` (whole file)
- Modify: `rust/crates/sirio/src/main.rs:3446-3465` (two conversions)
- Modify: `rust/crates/sirio/src/main.rs:25403-25409` (conversion test)
- Modify: `rust/crates/sirio_ui/src/settings.rs:50-53` (labels)
- Modify: `rust/crates/sirio_ui/src/settings.rs:3965-3969` (label test)

**Interfaces:**
- Consumes: `sirio_persistence::BaseColor::Notte` from Task 1.
- Produces:
  - `sirio_theme::BaseColor::Notte`, `BaseColor::ALL: [Self; 6]` ending in
    `Notte`, `BaseColor::Notte.tint() == Tint::new(270.6, 0.013)`,
    `BaseColor::Notte.title() == "Notte"`.
  - `pub(crate) struct NotteLadder { page, surface, raised, raised_hover: u32 }`
    and `pub(crate) const NOTTE_LADDER: NotteLadder` in `base_color.rs`,
    holding `0x0E1016, 0x202127, 0x2B2F3A, 0x313337`. Task 3 paints from it.

- [ ] **Step 1: Write the failing tests in `base_color.rs`**

Replace the whole `mod tests` at the bottom of
`rust/crates/sirio_theme/src/base_color.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_is_bezels_shipped_grey() {
        // `Tint::NONE` is what `Brand::default` carries, and bezel documents
        // that it reproduces the built-in palette exactly. Anything else here
        // would silently restyle every existing install on upgrade.
        assert_eq!(BaseColor::Neutral.tint(), bezel::theme::Tint::NONE);
        assert_eq!(BaseColor::default(), BaseColor::Neutral);
    }

    #[test]
    fn the_four_tinted_families_carry_bezels_own_numbers() {
        // Transcribed from `BASE_COLORS` in bezel's `brand.rs`. Written out
        // rather than read from that array so a bezel bump that moves a hue
        // fails here instead of restyling the app quietly — the same contract
        // `dark_palette_comes_from_bezel` holds for the palette itself.
        let expected = [
            (BaseColor::Stone, 58.071_f32, 0.013_f32),
            (BaseColor::Zinc, 285.938, 0.016),
            (BaseColor::Gray, 264.364, 0.027),
            (BaseColor::Slate, 257.417, 0.046),
        ];
        for (base, hue, chroma) in expected {
            let tint = base.tint();
            assert_eq!(tint.hue, hue, "{base:?} hue");
            assert_eq!(tint.chroma, chroma, "{base:?} chroma");
        }
    }

    #[test]
    fn all_lists_every_variant_in_display_order() {
        assert_eq!(
            BaseColor::ALL,
            [
                BaseColor::Neutral,
                BaseColor::Stone,
                BaseColor::Zinc,
                BaseColor::Gray,
                BaseColor::Slate,
                BaseColor::Notte,
            ]
        );
        assert_eq!(
            BaseColor::ALL.map(BaseColor::title),
            ["Neutral", "Stone", "Zinc", "Gray", "Slate", "Notte"]
        );
    }

    /// Notte's tint is not quoted from Tailwind: it is the mean oklch hue and
    /// chroma of the four surfaces the preset was given, so the text and
    /// plates bezel tints with it share the family of the surfaces they sit
    /// on. Recomputed here from the ladder so the two cannot drift apart.
    #[test]
    fn notte_tint_is_the_mean_of_its_own_ladder() {
        let ladder = NOTTE_LADDER;
        let rungs = [ladder.page, ladder.surface, ladder.raised, ladder.raised_hover];
        let (hue_sum, chroma_sum) = rungs.iter().fold((0.0_f32, 0.0_f32), |(h, c), &hex| {
            let (_, chroma, hue) = oklch_of(hex);
            (h + hue, c + chroma)
        });
        let mean_hue = hue_sum / rungs.len() as f32;
        let mean_chroma = chroma_sum / rungs.len() as f32;

        let tint = BaseColor::Notte.tint();
        assert!(
            (tint.hue - mean_hue).abs() < 1.0,
            "hue {} is not the ladder's mean {mean_hue:.1}",
            tint.hue
        );
        assert!(
            (tint.chroma - mean_chroma).abs() < 0.001,
            "chroma {} is not the ladder's mean {mean_chroma:.4}",
            tint.chroma
        );
    }

    #[test]
    fn the_notte_ladder_is_darkest_first() {
        let ladder = NOTTE_LADDER;
        assert_eq!(ladder.page, 0x0E1016);
        assert_eq!(ladder.surface, 0x202127);
        assert_eq!(ladder.raised, 0x2B2F3A);
        assert_eq!(ladder.raised_hover, 0x313337);
    }

    /// sRGB `0xRRGGBB` to oklch `(lightness, chroma, hue in degrees)`.
    /// Björn Ottosson's published matrices; bezel exposes only the reverse
    /// direction (`oklch_to_srgb`).
    fn oklch_of(hex: u32) -> (f32, f32, f32) {
        let channel = |shift: u32| {
            let c = ((hex >> shift) & 0xFF) as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let (r, g, b) = (channel(16), channel(8), channel(0));
        let l = (0.412_221_47 * r + 0.536_332_54 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_84 * g + 0.629_978_7 * b).cbrt();
        let lightness = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
        let a = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
        let b_axis = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;
        let chroma = a.hypot(b_axis);
        let hue = b_axis.atan2(a).to_degrees().rem_euclid(360.0);
        (lightness, chroma, hue)
    }
}
```

- [ ] **Step 2: Run them and watch them fail to compile**

Run: `cd rust && cargo test -p sirio_theme base_color`
Expected: compile errors — `no variant named 'Notte'`, `cannot find value 'NOTTE_LADDER'`,
and `ALL` has 5 elements where 6 are expected.

- [ ] **Step 3: Add the variant, the ladder constant, the tint and the title**

Replace everything above `#[cfg(test)]` in
`rust/crates/sirio_theme/src/base_color.rs` with:

```rust
//! The neutral family the palette's greys are tinted with.
//!
//! bezel ships five, quoted from Tailwind's neutral families at their 500
//! step (`BASE_COLORS` in bezel's `brand.rs`) — the same list shadcn offers
//! as its base colour. They are hues, not palettes: `Brand::apply` rotates
//! the hue of every grey and leaves lightness alone, because bezel's two
//! palettes were tuned against measured contrast ratios and a brand rotates
//! that work rather than replacing it.
//!
//! Sirio adds a sixth, `Notte`, which is a hue *and* a surface ladder: four
//! given dark surfaces that no tint can reach, because they are lighter than
//! bezel's dark page. The ladder is painted in `bezel_theme_for`
//! (`lib.rs`); this file only carries the values and the tint measured from
//! them. See `docs/THEME-PROVENANCE.md`, "Preset ladders".

use bezel::theme::Tint;

/// One of bezel's five base colours, or Sirio's own preset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BaseColor {
    /// bezel's shipped grey, carrying no hue at all.
    #[default]
    Neutral,
    Stone,
    Zinc,
    Gray,
    Slate,
    /// Sirio's preset: the cool hue of [`NOTTE_LADDER`] on every grey, and in
    /// dark the ladder itself as the surfaces. Light has no ladder and is the
    /// tint alone.
    Notte,
}

/// The four surfaces the Notte preset was given, darkest first, as
/// `0xRRGGBB`. The one place in the theme where lightness is chosen rather
/// than taken from bezel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NotteLadder {
    /// The page behind everything: the window frame's fallback.
    pub page: u32,
    /// Sidebar, panels, the terminal well, a card on the page.
    pub surface: u32,
    /// Raised elements, dialogs and overlays.
    pub raised: u32,
    /// A raised element under the pointer.
    pub raised_hover: u32,
}

pub(crate) const NOTTE_LADDER: NotteLadder = NotteLadder {
    page: 0x0E1016,
    surface: 0x202127,
    raised: 0x2B2F3A,
    raised_hover: 0x313337,
};

impl BaseColor {
    /// Every variant, in the order the picker offers them — bezel's own
    /// order in `BASE_COLORS`, neutral first, Sirio's preset last.
    pub const ALL: [Self; 6] = [
        Self::Neutral,
        Self::Stone,
        Self::Zinc,
        Self::Gray,
        Self::Slate,
        Self::Notte,
    ];

    /// The oklch hue and chroma this family tints the greys with.
    pub fn tint(self) -> Tint {
        match self {
            Self::Neutral => Tint::NONE,
            Self::Stone => Tint::new(58.071, 0.013),
            Self::Zinc => Tint::new(285.938, 0.016),
            Self::Gray => Tint::new(264.364, 0.027),
            Self::Slate => Tint::new(257.417, 0.046),
            // The mean oklch hue and chroma of the four `NOTTE_LADDER`
            // surfaces (270.6, 278.0, 269.4, 264.5 degrees; 0.013, 0.011,
            // 0.021, 0.008), so the greys bezel tints share the family of the
            // surfaces they sit on. `notte_tint_is_the_mean_of_its_own_ladder`
            // recomputes it.
            Self::Notte => Tint::new(270.6, 0.013),
        }
    }

    /// The user-visible name. Tailwind's for bezel's five, because a user
    /// who has met these names anywhere else has met exactly these colours;
    /// Sirio's own for the preset.
    pub fn title(self) -> &'static str {
        match self {
            Self::Neutral => "Neutral",
            Self::Stone => "Stone",
            Self::Zinc => "Zinc",
            Self::Gray => "Gray",
            Self::Slate => "Slate",
            Self::Notte => "Notte",
        }
    }
}
```

- [ ] **Step 4: Run the theme crate's tests**

Run: `cd rust && cargo test -p sirio_theme`
Expected: all pass. The loops over `BaseColor::ALL`
(`dark_terminal_surface_matches_the_pane_surface`,
`the_semantic_hues_hold_under_every_base_colour`) now include Notte and pass,
because Notte is still only a tint at this point.

- [ ] **Step 5: Extend the conversion test in `main.rs`**

In `rust/crates/sirio/src/main.rs`, in the test
`the_base_colour_converts_both_ways_across_the_crate_boundary`, add the pair:

```rust
            (BaseColor::Slate, sirio_theme::BaseColor::Slate),
            (BaseColor::Notte, sirio_theme::BaseColor::Notte),
```

- [ ] **Step 6: Add the two conversion arms**

Still in `main.rs`, `theme_base_color` gains:

```rust
        BaseColor::Slate => sirio_theme::BaseColor::Slate,
        BaseColor::Notte => sirio_theme::BaseColor::Notte,
```

and `persisted_base_color` gains:

```rust
        sirio_theme::BaseColor::Slate => BaseColor::Slate,
        sirio_theme::BaseColor::Notte => BaseColor::Notte,
```

(Without these arms the crate does not compile — `match` on the enum is
exhaustive — so the test cannot be run red on its own. Confirm the failing
state by reading the compiler error from step 7 before adding the arms if
you want to see it.)

- [ ] **Step 7: Run the conversion test**

Run: `cd rust && cargo test -p sirio the_base_colour_converts_both_ways_across_the_crate_boundary`
Expected: PASS. (Needs Zig 0.15.2 on PATH; see Global Constraints.)

- [ ] **Step 8: Extend the label test in `settings.rs`**

In `rust/crates/sirio_ui/src/settings.rs`, replace the test
`the_base_colour_segments_are_bezels_five_in_order` with:

```rust
    #[test]
    fn the_base_colour_segments_are_bezels_five_then_sirios_preset() {
        assert_eq!(
            SEGMENTED_BASE_COLOR,
            &["Neutral", "Stone", "Zinc", "Gray", "Slate", "Notte"]
        );
        assert_eq!(
            SEGMENTED_BASE_COLOR.len(),
            sirio_theme::BaseColor::ALL.len(),
            "a base colour without a segment cannot be picked"
        );
    }
```

- [ ] **Step 9: Run it and watch it fail**

Run: `cd rust && cargo test -p sirio_ui the_base_colour_segments_are_bezels_five_then_sirios_preset`
Expected: FAIL — left has 5 entries, right has 6.

- [ ] **Step 10: Add the label**

In `settings.rs` replace the constant and its comment with:

```rust
/// bezel's five base colours, in its own order, then Sirio's Notte preset.
/// Text rather than swatches: the five differ by hue at chroma 0.013–0.046,
/// which a 16px pill cannot show, and the names are Tailwind's — a vocabulary
/// a user may already have.
const SEGMENTED_BASE_COLOR: &[&str] = &["Neutral", "Stone", "Zinc", "Gray", "Slate", "Notte"];
```

- [ ] **Step 11: Run the settings tests**

Run: `cd rust && cargo test -p sirio_ui -- the_base_colour_segments_are_bezels_five_then_sirios_preset every_base_colour_maps_to_its_own_segment`
Expected: both PASS.

- [ ] **Step 12: Commit**

```bash
git add rust/crates/sirio_theme/src/base_color.rs rust/crates/sirio/src/main.rs rust/crates/sirio_ui/src/settings.rs
git commit -m "feat(theme): add the notte preset as a sixth base colour

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_016Gz9nyKfNg9BB8cJ6Dtbcu"
```

---

### Task 3: The dark surface ladder

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs:254-265` (`bezel_theme_for`)
- Test: `rust/crates/sirio_theme/src/lib.rs`, inside `mod tests`, directly
  after `translucency_preserves_the_chosen_base_colour`

**Interfaces:**
- Consumes: `BaseColor::Notte`, `BaseColor::Notte.tint()`,
  `base_color::NOTTE_LADDER` from Task 2; the private `rgb_hex(u32) -> Rgba`
  already in `lib.rs`.
- Produces: `bezel_theme_for(BaseColor::Notte, Appearance::Dark)` returns the
  branded theme with `bg`, `surface`, `surface_card`, `surface_raised`,
  `surface_dialog`, `surface_overlay`, `surface_raised_hover` replaced;
  `fn opaque_hsla(hex: u32) -> gpui::Hsla` (private, `lib.rs`). Task 4
  documents it.

- [ ] **Step 1: Write the five failing tests**

In `rust/crates/sirio_theme/src/lib.rs`, inside the first `mod tests`, add
after `translucency_preserves_the_chosen_base_colour`:

```rust
    /// The bezel theme Notte builds, read through the path bezel's own
    /// widgets use, so the test covers both consumers of the builder.
    fn notte_bezel(appearance: Appearance) -> bezel::theme::Theme {
        let mode = match appearance {
            Appearance::Dark => ThemeMode::Dark,
            Appearance::Light => ThemeMode::Light,
        };
        Theme::for_appearance(mode, appearance, BaseColor::Notte).to_bezel_theme()
    }

    /// bezel's palette rotated onto Notte's tint and nothing else — what
    /// Notte would be if it were only a sixth base colour.
    fn notte_tint_only(appearance: bezel::theme::Appearance) -> bezel::theme::Theme {
        bezel::theme::Theme::branded(
            &bezel::theme::Brand {
                tint: BaseColor::Notte.tint(),
                ..Default::default()
            },
            appearance,
        )
    }

    #[test]
    fn notte_dark_ladder_is_the_four_given_values() {
        // Written out rather than read from `NOTTE_LADDER`, so a change to
        // the constant fails here instead of restyling the preset quietly.
        let bezel = notte_bezel(Appearance::Dark);
        for (name, token, hex) in [
            ("bg", bezel.bg, 0x0E1016),
            ("surface", bezel.surface, 0x202127),
            ("surface_card", bezel.surface_card, 0x202127),
            ("surface_raised", bezel.surface_raised, 0x2B2F3A),
            ("surface_dialog", bezel.surface_dialog, 0x2B2F3A),
            ("surface_overlay", bezel.surface_overlay, 0x2B2F3A),
            ("surface_raised_hover", bezel.surface_raised_hover, 0x313337),
        ] {
            assert_eq!(token, opaque_hsla(hex), "{name}");
        }

        // Sirio's own tokens follow the same builder.
        let sirio = ThemeColors::for_appearance(Appearance::Dark, BaseColor::Notte);
        assert_eq!(sirio.bg, Rgba::from(opaque_hsla(0x0E1016)));
        assert_eq!(sirio.surface, Rgba::from(opaque_hsla(0x202127)));
        assert_eq!(sirio.surface_raised, Rgba::from(opaque_hsla(0x2B2F3A)));
        assert_eq!(sirio.terminal_surface, sirio.surface, "the dark terminal well is the pane");
    }

    #[test]
    fn notte_light_is_only_a_tint() {
        // Four dark surfaces were given and no light ones; inventing a light
        // ladder was rejected (spec N3).
        let notte = notte_bezel(Appearance::Light);
        let tinted = notte_tint_only(bezel::theme::Appearance::Light);
        for (name, ours, theirs) in [
            ("bg", notte.bg, tinted.bg),
            ("surface", notte.surface, tinted.surface),
            ("surface_card", notte.surface_card, tinted.surface_card),
            ("surface_raised", notte.surface_raised, tinted.surface_raised),
            ("surface_dialog", notte.surface_dialog, tinted.surface_dialog),
            ("surface_overlay", notte.surface_overlay, tinted.surface_overlay),
            (
                "surface_raised_hover",
                notte.surface_raised_hover,
                tinted.surface_raised_hover,
            ),
            ("text", notte.text, tinted.text),
            ("border", notte.border, tinted.border),
        ] {
            assert_eq!(ours, theirs, "light {name}");
        }
    }

    #[test]
    fn notte_keeps_bezels_veils_text_and_hues() {
        // Only the seven surface tokens move. Veils compose over whatever is
        // beneath them; the text ladder and the semantic hues are bezel's,
        // carrying Notte's tint like any other base colour.
        let notte = notte_bezel(Appearance::Dark);
        let tinted = notte_tint_only(bezel::theme::Appearance::Dark);
        for (name, ours, theirs) in [
            ("element_hover", notte.element_hover, tinted.element_hover),
            ("element_active", notte.element_active, tinted.element_active),
            ("border", notte.border, tinted.border),
            ("border_strong", notte.border_strong, tinted.border_strong),
            ("input_bg", notte.input_bg, tinted.input_bg),
            ("code_wash", notte.code_wash, tinted.code_wash),
            ("ring", notte.ring, tinted.ring),
            ("selection", notte.selection, tinted.selection),
            ("text", notte.text, tinted.text),
            ("text_muted", notte.text_muted, tinted.text_muted),
            ("text_faint", notte.text_faint, tinted.text_faint),
            ("text_dim", notte.text_dim, tinted.text_dim),
            ("solid", notte.solid, tinted.solid),
            ("on_solid", notte.on_solid, tinted.on_solid),
            ("accent", notte.accent, tinted.accent),
            ("danger", notte.danger, tinted.danger),
            ("warning", notte.warning, tinted.warning),
            ("success", notte.success, tinted.success),
            ("diff_add", notte.diff_add, tinted.diff_add),
            ("diff_del", notte.diff_del, tinted.diff_del),
        ] {
            assert_eq!(ours, theirs, "dark {name}");
        }
    }

    #[test]
    fn notte_body_text_clears_aaa_on_every_surface() {
        let sirio = ThemeColors::for_appearance(Appearance::Dark, BaseColor::Notte);
        let bezel = notte_bezel(Appearance::Dark);
        for (name, surface) in [
            ("bg", sirio.bg),
            ("surface", sirio.surface),
            ("surface_raised", sirio.surface_raised),
            ("surface_raised_hover", Rgba::from(bezel.surface_raised_hover)),
        ] {
            let ratio = contrast_ratio(sirio.text, surface);
            assert!(ratio >= 7.0, "text on {name} is {ratio:.1}:1, below AAA");
        }
    }

    #[test]
    fn notte_depth_ladder_reads_as_depth() {
        // Ordering only: the given hover step is small, and that is the
        // user's ladder as given (spec, Risks).
        let bezel = notte_bezel(Appearance::Dark);
        let rungs = [
            ("bg", bezel.bg),
            ("surface", bezel.surface),
            ("surface_raised", bezel.surface_raised),
            ("surface_raised_hover", bezel.surface_raised_hover),
        ];
        for pair in rungs.windows(2) {
            let (lower, upper) = (pair[0], pair[1]);
            assert!(
                relative_luminance(Rgba::from(lower.1)) < relative_luminance(Rgba::from(upper.1)),
                "{} should sit below {}",
                lower.0,
                upper.0
            );
        }
    }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cd rust && cargo test -p sirio_theme notte_`
Expected: compile error `cannot find function 'opaque_hsla'`. After adding
only the `opaque_hsla` helper from step 3 (not the ladder), re-run: expected
`notte_dark_ladder_is_the_four_given_values`, `notte_body_text_clears_aaa_on_every_surface`
and `notte_depth_ladder_reads_as_depth` FAIL (bezel's `#060606`/`#0D0D0D`
ladder is what comes back), while `notte_light_is_only_a_tint` and
`notte_keeps_bezels_veils_text_and_hues` PASS (they hold before and after).

- [ ] **Step 3: Paint the ladder in the one builder**

In `rust/crates/sirio_theme/src/lib.rs`, replace `bezel_theme_for` (lines
254–265) with:

```rust
fn bezel_theme_for(base_color: BaseColor, appearance: Appearance) -> bezel::theme::Theme {
    let mut theme = bezel::theme::Theme::branded(
        &bezel::theme::Brand {
            tint: base_color.tint(),
            ..Default::default()
        },
        match appearance {
            Appearance::Dark => bezel::theme::Appearance::Dark,
            Appearance::Light => bezel::theme::Appearance::Light,
        },
    );
    if base_color == BaseColor::Notte && appearance == Appearance::Dark {
        paint_notte_ladder(&mut theme);
    }
    theme
}

/// The one place lightness moves.
///
/// bezel's `Brand::apply` rotates hue and never lightness, and the Notte
/// preset was given four surfaces that are *lighter* than bezel's dark page
/// (`#202127` is above bezel's raised card), so no tint reaches them. They
/// are painted here, after `branded`, onto the seven surface tokens and
/// nothing else: the veils (`element_hover`, `border`, `input_bg`, …) are
/// white-alpha washes that compose over whatever is beneath them, and the
/// text ladder and semantic hues stay bezel's. Done in this builder, not in
/// `ThemeColors::for_appearance`, because `to_bezel_theme` calls the same
/// function and hands the result to `install_custom` — so bezel's own widgets
/// and Sirio's tokens see one ladder by construction.
///
/// Mutation of the local, in bezel's own `Brand::apply` style; struct-update
/// syntax would need every one of bezel's 72 fields restated.
fn paint_notte_ladder(theme: &mut bezel::theme::Theme) {
    let ladder = base_color::NOTTE_LADDER;
    theme.bg = opaque_hsla(ladder.page);
    theme.surface = opaque_hsla(ladder.surface);
    // A card sits on the page, as it does in bezel's own dark
    // (`surface_card` #0E0E0E beside `surface` #0D0D0D).
    theme.surface_card = opaque_hsla(ladder.surface);
    theme.surface_raised = opaque_hsla(ladder.raised);
    theme.surface_dialog = opaque_hsla(ladder.raised);
    theme.surface_overlay = opaque_hsla(ladder.raised);
    theme.surface_raised_hover = opaque_hsla(ladder.raised_hover);
}

/// Opaque `0xRRGGBB` as the `Hsla` bezel's tokens are stored in.
fn opaque_hsla(hex: u32) -> gpui::Hsla {
    gpui::Hsla::from(rgb_hex(hex))
}
```

`base_color` is the private module declared at `lib.rs:47` (`mod base_color;`),
so `base_color::NOTTE_LADDER` resolves from anywhere in `lib.rs`, tests
included.

- [ ] **Step 4: Run the theme crate's tests**

Run: `cd rust && cargo test -p sirio_theme`
Expected: all pass — the five `notte_` tests, and everything that iterates
`BaseColor::ALL`. `dark_palette_comes_from_bezel` and
`a_tinted_base_moves_the_greys_and_leaves_sirios_own_colours_alone` are
untouched because they read Neutral and Slate.

- [ ] **Step 5: Check the crates above still build**

Run: `cd rust && cargo build -p sirio_ui && cargo build -p sirio`
Expected: both build. (`sirio` needs Zig 0.15.2 on PATH.)

- [ ] **Step 6: Commit**

```bash
git add rust/crates/sirio_theme/src/lib.rs
git commit -m "feat(theme): give the notte preset its own dark surface ladder

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_016Gz9nyKfNg9BB8cJ6Dtbcu"
```

---

### Task 4: Provenance and the on-screen check

**Files:**
- Modify: `docs/THEME-PROVENANCE.md:52-55` (insert after the paragraph
  ending "`a_tinted_base_moves_the_greys_and_leaves_sirios_own_colours_alone` pins all three.")

**Interfaces:**
- Consumes: the names from Task 3 (`paint_notte_ladder`, `opaque_hsla`,
  `NOTTE_LADDER`, the five `notte_` tests).
- Produces: the record future bezel bumps are reviewed against.

- [ ] **Step 1: Write the provenance subsection**

In `docs/THEME-PROVENANCE.md`, between the paragraph that ends with
"`a_tinted_base_moves_the_greys_and_leaves_sirios_own_colours_alone` pins all
three." and the paragraph that begins "**One exception: `text`.**", insert:

```markdown
**Preset ladders — Sirio's own.** `BaseColor::Notte` is the sixth entry of the
picker and the one place in the theme where lightness is chosen rather than
taken from bezel. It was given four surfaces:

| Given | oklch L | oklch C | oklch H | Painted onto |
|---|---|---|---|---|
| `#0E1016` | 0.174 | 0.013 | 270.6 | `bg` |
| `#202127` | 0.249 | 0.011 | 278.0 | `surface`, `surface_card` |
| `#2B2F3A` | 0.306 | 0.021 | 269.4 | `surface_raised`, `surface_dialog`, `surface_overlay` |
| `#313337` | 0.321 | 0.008 | 264.5 | `surface_raised_hover` |

No tint reaches them: bezel's dark page is `#060606` (L 0.122) and its raised
card L 0.235, so the given ladder starts above where bezel's ends, and
`Brand::apply` never moves lightness. `paint_notte_ladder` in `lib.rs`
therefore overwrites those seven tokens after `Theme::branded`, in dark only
— four dark surfaces were given and no light ones, so light is the tint
alone. Everything else stays bezel's: the veils compose over the new
surfaces, the text ladder and semantic hues carry the tint like any other
grey. That tint, `Tint::new(270.6, 0.013)`, is the mean oklch hue and chroma
of the four values (`NOTTE_LADDER`, `base_color.rs`), not one of Tailwind's.

Body text softened by `TEXT_SOFTENING` lands at `#CFD1DA`: 10.5:1 on
`#202127`, 8.8:1 on `#2B2F3A`, 8.3:1 on `#313337`. The tests
`notte_dark_ladder_is_the_four_given_values`,
`notte_keeps_bezels_veils_text_and_hues`, `notte_light_is_only_a_tint`,
`notte_body_text_clears_aaa_on_every_surface` and
`notte_depth_ladder_reads_as_depth` pin all of the above. A bezel bump that
adds a surface token leaves it at bezel's lightness inside Notte; the table is
the checklist for that review.
```

- [ ] **Step 2: Build and launch the app**

Run: `Scripts/build-dev.sh`
Expected: builds and relaunches Sirio.

- [ ] **Step 3: Check on screen, dark**

In Sirio: Settings → Appearance → Tema: Scuro; Colore base: Notte.
Look at, and note in the commit body which of these were seen:

1. The six segments fit the Theme card on one row (spec U1). If they wrap,
   write that down; the fix belongs to the segmented control, not here.
2. Sidebar and tab strip are `#202127`; the window frame behind them is the
   `#0E1016` material.
3. A terminal pane's empty area is the same `#202127` as the pane.
4. Open any dialog (for example the worktree closure menu on a worktree's
   ×) and any popover: both sit on `#2B2F3A` with a visible hairline.
5. Code and diff text in a chat reads clearly on the lighter page.

- [ ] **Step 4: Check on screen, light and back**

Tema: Chiaro — the app is bezel's light palette in a cool grey; nothing is
painted from the ladder. Tema: Scuro — the ladder returns. Tema: Sistema —
follows the system switch the same way.

- [ ] **Step 5: Commit**

```bash
git add docs/THEME-PROVENANCE.md
git commit -m "docs(theme): record the notte preset ladder in the provenance

On screen (dark, Notte): replace this line with one line per check from step 3

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_016Gz9nyKfNg9BB8cJ6Dtbcu"
```

---

## Done when

- `cd rust && cargo test -p sirio_persistence && cargo test -p sirio_theme && cargo test -p sirio_ui` all green.
- `cargo test -p sirio the_base_colour_converts_both_ways_across_the_crate_boundary` green.
- The four on-screen checks recorded in the last commit.
- Then ask the user whether to run `Scripts/ci.sh`; do not run it unasked.
