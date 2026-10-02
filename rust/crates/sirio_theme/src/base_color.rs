//! The neutral family the palette's greys are tinted with.
//!
//! bezel ships five, quoted from Tailwind's neutral families at their 500
//! step (`BASE_COLORS` in bezel's `brand.rs`) — the same list shadcn offers
//! as its base colour. They are hues, not palettes: `Brand::apply` rotates
//! the hue of every grey and leaves lightness alone, because bezel's two
//! palettes were tuned against measured contrast ratios and a brand rotates
//! that work rather than replacing it.
//!
//! Sirio adds two presets, and both are surface ladders bezel's tint cannot
//! reach. `Notte` is a hue *and* a ladder: four given dark surfaces that are
//! lighter than bezel's dark page. `Onice` is a ladder alone, with no hue: the
//! same kind of solid grey ladder `Neutral` paints, started at pure black.
//! The ladders are painted in `bezel_theme_for` (`lib.rs`); this file only
//! carries the values and the tint measured from them. See
//! `docs/THEME-PROVENANCE.md`, "Preset ladders".

use bezel_theme::Tint;

use crate::Appearance;

/// One of bezel's five base colours, or one of Sirio's own presets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BaseColor {
    /// bezel's shipped grey, carrying no hue at all. Both appearances paint
    /// the approved surface ladder on top (see [`BaseColor::grey_ladder`]).
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
    /// Sirio's darkest preset: no hue, and in dark a grey ladder that starts
    /// at pure black ([`ONICE_DARK`]). Black has no light form, so light is
    /// Neutral's own ladder.
    Onice,
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

/// The six solid rungs of a grey ladder, as `0xRRGGBB`. Solid fills rather
/// than veils: the hover is a value of its own, not a wash over the surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GreyLadder {
    /// Central panes, the terminal, and the window frame's fallback.
    pub page: u32,
    /// Sidebars, panels and cards.
    pub surface: u32,
    /// Chat, dialogs and overlays.
    pub raised: u32,
    /// A raised element under the pointer.
    pub raised_hover: u32,
    /// The composer and the inset wells.
    pub input: u32,
    /// A row under the pointer.
    pub hover: u32,
}

/// Approved shell values (2026-10-02, darkened from 2026-09-12's `#141414`
/// ladder, which read as too grey): sidebars `#131313`, central panes
/// `#0E0E0E`, chat `#1A1A1A`, composer `#242424`, hover `#292929`.
const NEUTRAL_DARK: GreyLadder = GreyLadder {
    page: 0x0E0E0E,
    surface: 0x131313,
    raised: 0x1A1A1A,
    raised_hover: 0x292929,
    input: 0x242424,
    hover: 0x292929,
};

/// The light steps mirror the dark ones.
const NEUTRAL_LIGHT: GreyLadder = GreyLadder {
    page: 0xF8F8F8,
    surface: 0xE8E8E8,
    raised: 0xECECEC,
    raised_hover: 0xD9D9D9,
    input: 0xFFFFFF,
    hover: 0xD9D9D9,
};

/// Neutral's shape from pure black: steps of ten code values (`00`, `0A`,
/// `14`, `1E`) and a hover of `26`, whose jump over the raised rung is the
/// one Neutral's 2026-09-12 hover `#363636` made over its own (0.077 in oklab lightness).
/// Depth has to come from these steps alone: a shadow cannot be darker than
/// the page, so on black it does not show.
const ONICE_DARK: GreyLadder = GreyLadder {
    page: 0x000000,
    surface: 0x0A0A0A,
    raised: 0x141414,
    raised_hover: 0x262626,
    input: 0x1E1E1E,
    hover: 0x262626,
};

impl BaseColor {
    /// Every variant, in the order the picker offers them — bezel's own
    /// order in `BASE_COLORS`, neutral first, Sirio's presets last.
    pub const ALL: [Self; 7] = [
        Self::Neutral,
        Self::Stone,
        Self::Zinc,
        Self::Gray,
        Self::Slate,
        Self::Notte,
        Self::Onice,
    ];

    /// The grey ladder this base feeds only to `Theme::to_bezel_theme`, or
    /// `None` when its surfaces are bezel's own (the tinted five) or Notte's.
    /// Sirio's matching values are hand-maintained in `presets.rs`; edit both
    /// ladder sources until sub-project 7.
    pub(crate) fn grey_ladder(self, appearance: Appearance) -> Option<GreyLadder> {
        match (self, appearance) {
            (Self::Neutral, Appearance::Dark) => Some(NEUTRAL_DARK),
            (Self::Neutral | Self::Onice, Appearance::Light) => Some(NEUTRAL_LIGHT),
            (Self::Onice, Appearance::Dark) => Some(ONICE_DARK),
            _ => None,
        }
    }

    /// The oklch hue and chroma this family tints the greys with.
    pub fn tint(self) -> Tint {
        match self {
            Self::Neutral | Self::Onice => Tint::NONE,
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
    /// Sirio's own for the presets.
    pub fn title(self) -> &'static str {
        match self {
            Self::Neutral => "Neutral",
            Self::Stone => "Stone",
            Self::Zinc => "Zinc",
            Self::Gray => "Gray",
            Self::Slate => "Slate",
            Self::Notte => "Notte",
            Self::Onice => "Onice",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Notte's tint is not quoted from Tailwind: it is the mean oklch hue and
    /// chroma of the four surfaces the preset was given, so the text and
    /// plates bezel tints with it share the family of the surfaces they sit
    /// on. Recomputed here from the ladder so the two cannot drift apart.
    #[test]
    fn notte_tint_is_the_mean_of_its_own_ladder() {
        let ladder = NOTTE_LADDER;
        let rungs = [
            ladder.page,
            ladder.surface,
            ladder.raised,
            ladder.raised_hover,
        ];
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
