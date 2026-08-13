//! COSMIC's spacing scale, transcribed from `cosmic-theme::Spacing`'s
//! `Default` impl (verified against the crate source and against
//! `Theme::light_default()` / `Theme::dark_default()` — the scale does not
//! vary between modes).

/// COSMIC's ten-step spacing scale, in logical pixels.
///
/// Field names mirror `cosmic-theme::Spacing`'s `space_*` fields (minus the
/// `space_` prefix, which was redundant on a type already named `Spacing`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CosmicSpacing {
    pub none: u16,
    pub xxxs: u16,
    pub xxs: u16,
    pub xs: u16,
    pub s: u16,
    pub m: u16,
    pub l: u16,
    pub xl: u16,
    pub xxl: u16,
    pub xxxl: u16,
}

impl Default for CosmicSpacing {
    fn default() -> Self {
        Self {
            none: 0,
            xxxs: 4,
            xxs: 8,
            xs: 12,
            s: 16,
            m: 24,
            l: 32,
            xl: 48,
            xxl: 64,
            xxxl: 128,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_cosmic_themes_verified_defaults() {
        let s = CosmicSpacing::default();
        assert_eq!(
            (
                s.none, s.xxxs, s.xxs, s.xs, s.s, s.m, s.l, s.xl, s.xxl, s.xxxl
            ),
            (0, 4, 8, 12, 16, 24, 32, 48, 64, 128)
        );
    }
}
