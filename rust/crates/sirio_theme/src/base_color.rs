//! The neutral family the palette's greys are tinted with.
//!
//! bezel ships five, quoted from Tailwind's neutral families at their 500
//! step (`BASE_COLORS` in bezel's `brand.rs`) — the same list shadcn offers
//! as its base colour. They are hues, not palettes: `Brand::apply` rotates
//! the hue of every grey and leaves lightness alone, because bezel's two
//! palettes were tuned against measured contrast ratios and a brand rotates
//! that work rather than replacing it.

use bezel::theme::Tint;

/// One of bezel's five base colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BaseColor {
    /// bezel's shipped grey, carrying no hue at all.
    #[default]
    Neutral,
    Stone,
    Zinc,
    Gray,
    Slate,
}

impl BaseColor {
    /// Every variant, in the order the picker offers them — bezel's own
    /// order in `BASE_COLORS`, neutral first.
    pub const ALL: [Self; 5] = [
        Self::Neutral,
        Self::Stone,
        Self::Zinc,
        Self::Gray,
        Self::Slate,
    ];

    /// The oklch hue and chroma this family tints the greys with.
    pub fn tint(self) -> Tint {
        match self {
            Self::Neutral => Tint::NONE,
            Self::Stone => Tint::new(58.071, 0.013),
            Self::Zinc => Tint::new(285.938, 0.016),
            Self::Gray => Tint::new(264.364, 0.027),
            Self::Slate => Tint::new(257.417, 0.046),
        }
    }

    /// The user-visible name. Tailwind's, because a user who has met these
    /// names anywhere else has met exactly these colours.
    pub fn title(self) -> &'static str {
        match self {
            Self::Neutral => "Neutral",
            Self::Stone => "Stone",
            Self::Zinc => "Zinc",
            Self::Gray => "Gray",
            Self::Slate => "Slate",
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
            ]
        );
        assert_eq!(
            BaseColor::ALL.map(BaseColor::title),
            ["Neutral", "Stone", "Zinc", "Gray", "Slate"]
        );
    }
}
