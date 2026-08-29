//! COSMIC's corner-radius scale, transcribed from
//! `cosmic-theme::CornerRadii`'s `Default` impl (verified against the crate
//! source and against `Theme::light_default()` / `Theme::dark_default()` —
//! the scale does not vary between modes).

/// COSMIC's six-step corner-radius scale.
///
/// Each step is `[f32; 4]` (matching upstream) so a future caller can round
/// corners independently per side; GPUI's `div()` only exposes a single
/// uniform radius via `.rounded()`, so today's call sites read `radius[0]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CosmicRadii {
    pub radius_0: [f32; 4],
    pub radius_xs: [f32; 4],
    pub radius_s: [f32; 4],
    pub radius_m: [f32; 4],
    pub radius_l: [f32; 4],
    pub radius_xl: [f32; 4],
}

impl Default for CosmicRadii {
    fn default() -> Self {
        Self {
            radius_0: [0.0; 4],
            radius_xs: [4.0; 4],
            radius_s: [8.0; 4],
            radius_m: [16.0; 4],
            radius_l: [32.0; 4],
            radius_xl: [160.0; 4],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_cosmic_themes_verified_defaults() {
        let r = CosmicRadii::default();
        assert_eq!(r.radius_0[0], 0.0);
        assert_eq!(r.radius_xs[0], 4.0);
        assert_eq!(r.radius_s[0], 8.0);
        assert_eq!(r.radius_m[0], 16.0);
        assert_eq!(r.radius_l[0], 32.0);
        assert_eq!(r.radius_xl[0], 160.0);
    }
}
