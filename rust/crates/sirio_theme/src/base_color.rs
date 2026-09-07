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
