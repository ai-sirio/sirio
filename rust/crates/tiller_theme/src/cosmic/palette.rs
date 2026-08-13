//! Transcribed defaults for the container hierarchy — COSMIC's stock
//! `cosmic-dark` and `cosmic-light` palettes.
//!
//! These are not hand-picked: they are `cosmic_theme::Theme::dark_default()`
//! / `Theme::light_default()`, executed against the real crate (git
//! revision `1f8d8786`, see `docs/linux-rewrite/COSMIC-DESIGN.md` for the
//! depend-vs-transcribe reasoning) and read back off the wire. Dark was
//! cross-checked against this machine's own unmodified
//! `~/.config/cosmic/com.system76.CosmicTheme.Dark/v2/*` (a genuine
//! Pop!_OS 24.04 install with no builder overrides on that side) and
//! matched byte-for-byte.

use super::component::CosmicComponent;
use super::container::{CosmicContainer, CosmicContainers};
use super::hex::parse as hex;
use gpui::Rgba;

fn h(s: &str) -> Rgba {
    hex(s).unwrap_or_else(|| panic!("transcribed COSMIC hex literal failed to parse: {s}"))
}

fn component(
    base: &str,
    hover: &str,
    pressed: &str,
    on: &str,
    divider: &str,
    border: &str,
) -> CosmicComponent {
    CosmicComponent {
        base: h(base),
        hover: h(hover),
        pressed: h(pressed),
        on: h(on),
        divider: h(divider),
        border: h(border),
    }
}

fn container(base: &str, on: &str, divider: &str, component: CosmicComponent) -> CosmicContainer {
    CosmicContainer {
        base: h(base),
        on: h(on),
        divider: h(divider),
        component,
    }
}

impl CosmicContainers {
    /// The stock `cosmic-dark` container hierarchy.
    pub fn dark() -> Self {
        Self {
            background: container(
                "#1B1B1BFF",
                "#F5F5F5FF",
                "#474747FF",
                component(
                    "#2E2E2EFF",
                    "#434343FF",
                    "#585858FF",
                    "#FFFFFFFF",
                    "#FFFFFF33",
                    "#BEBEBEFF",
                ),
            ),
            primary: container(
                "#272727FF",
                "#FFFFFFFF",
                "#525252FF",
                component(
                    "#363636FF",
                    "#4A4A4AFF",
                    "#5E5E5EFF",
                    "#FFFFFFFF",
                    "#FFFFFF33",
                    "#BEBEBEFF",
                ),
            ),
            secondary: container(
                "#343434FF",
                "#FFFFFFFF",
                "#5C5C5CFF",
                component(
                    "#3B3B3BFF",
                    "#4F4F4FFF",
                    "#626262FF",
                    "#FFFFFFFF",
                    "#FFFFFF33",
                    "#BEBEBEFF",
                ),
            ),
        }
    }

    /// The stock `cosmic-light` container hierarchy.
    pub fn light() -> Self {
        Self {
            background: container(
                "#D7D7D7FF",
                "#121212FF",
                "#B0B0B0FF",
                component(
                    "#F5F5F5FF",
                    "#F6F6F6FF",
                    "#F7F7F7FF",
                    "#272727FF",
                    "#27272733",
                    "#161616FF",
                ),
            ),
            primary: container(
                "#EBEBEBFF",
                "#000000FF",
                "#BCBCBCFF",
                component(
                    "#DADADAFF",
                    "#DEDEDEFF",
                    "#E2E2E2FF",
                    "#0B0B0BFF",
                    "#0B0B0B33",
                    "#161616FF",
                ),
            ),
            secondary: container(
                "#FCFCFCFF",
                "#2C2C2CFF",
                "#D2D2D2FF",
                component(
                    "#F5F5F5FF",
                    "#F6F6F6FF",
                    "#F7F7F7FF",
                    "#272727FF",
                    "#27272733",
                    "#161616FF",
                ),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_background_is_darker_than_dark_secondary() {
        let c = CosmicContainers::dark();
        assert!(c.background.base.r < c.secondary.base.r);
        assert!(c.background.base.r < c.primary.base.r);
        assert!(c.primary.base.r < c.secondary.base.r);
    }

    #[test]
    fn light_background_is_darker_than_light_secondary() {
        let c = CosmicContainers::light();
        assert!(c.background.base.r < c.secondary.base.r);
    }

    #[test]
    fn dark_and_light_are_distinct() {
        let dark = CosmicContainers::dark();
        let light = CosmicContainers::light();
        assert_ne!(dark.background.base, light.background.base);
        assert_ne!(dark.background.on, light.background.on);
    }
}
