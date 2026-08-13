//! Transcribed defaults for COSMIC's semantic component colors — the named
//! roles the brief calls out: `button`, `accent`, `accent_button`,
//! `success`/`success_button`, `destructive`/`destructive_button`,
//! `warning`/`warning_button`, `icon_button`, `link_button`, `list_button`,
//! `text_button`, plus the dialog `shade`.
//!
//! Sourced the same way as [`super::palette`]: `Theme::dark_default()` /
//! `Theme::light_default()` executed against the real crate and read back
//! off the wire, cross-checked against this machine's own unmodified dark
//! COSMIC config.
//!
//! `accent_text`, `control_tint`, `text_tint` and `window_hint` are all
//! `None` (unset) in both stock palettes upstream — COSMIC derives a
//! readable-on-accent text color and a control/text tint automatically
//! when these are absent, so they are omitted here rather than transcribed
//! as a color that does not exist by default.

use super::component::CosmicComponent;
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

/// The full named set of semantic component colors for one appearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CosmicSemanticColors {
    pub button: CosmicComponent,
    pub accent: CosmicComponent,
    pub accent_button: CosmicComponent,
    pub success: CosmicComponent,
    pub success_button: CosmicComponent,
    pub destructive: CosmicComponent,
    pub destructive_button: CosmicComponent,
    pub warning: CosmicComponent,
    pub warning_button: CosmicComponent,
    pub icon_button: CosmicComponent,
    pub link_button: CosmicComponent,
    pub list_button: CosmicComponent,
    pub text_button: CosmicComponent,
    /// Dialog/modal backdrop shade.
    pub shade: Rgba,
}

impl CosmicSemanticColors {
    /// The stock `cosmic-dark` semantic colors.
    pub fn dark() -> Self {
        Self {
            button: component(
                "#9E9E9E40",
                "#63636366",
                "#2B2B2B9F",
                "#FFFFFFFF",
                "#FFFFFF33",
                "#BEBEBEFF",
            ),
            accent: component(
                "#63D0DFFF",
                "#63BAC6FF",
                "#3C737AFF",
                "#000000FF",
                "#000000FF",
                "#63D0DFFF",
            ),
            accent_button: component(
                "#63D0DFFF",
                "#63BAC6FF",
                "#3C737AFF",
                "#030303FF",
                "#030303FF",
                "#63D0DFFF",
            ),
            success: component(
                "#5EDB8CFF",
                "#5FC384FF",
                "#3A7851FF",
                "#000000FF",
                "#000000FF",
                "#5EDB8CFF",
            ),
            success_button: component(
                "#5EDB8CFF",
                "#5FC384FF",
                "#3A7851FF",
                "#000000FF",
                "#030303FF",
                "#5EDB8CFF",
            ),
            destructive: component(
                "#FFA09AFF",
                "#E0948FFF",
                "#8A5B58FF",
                "#000000FF",
                "#000000FF",
                "#FFA09AFF",
            ),
            destructive_button: component(
                "#FFA09AFF",
                "#E0948FFF",
                "#8A5B58FF",
                "#000000FF",
                "#030303FF",
                "#FFA09AFF",
            ),
            warning: component(
                "#FFA37DFF",
                "#E09678FF",
                "#8A5C49FF",
                "#000000FF",
                "#000000FF",
                "#FFA37DFF",
            ),
            warning_button: component(
                "#FFA37DFF",
                "#E09678FF",
                "#8A5C49FF",
                "#000000FF",
                "#FFFFFFFF",
                "#FFA37DFF",
            ),
            icon_button: component(
                "#00000000",
                "#63636333",
                "#16161680",
                "#BEBEBEFF",
                "#BEBEBE33",
                "#BEBEBEFF",
            ),
            link_button: component(
                "#00000000",
                "#00000000",
                "#00000000",
                "#63D0DFFF",
                "#63D0DF33",
                "#BEBEBEFF",
            ),
            list_button: component(
                "#00000000",
                "#00000000",
                "#16161680",
                "#FFFFFFFF",
                "#FFFFFF33",
                "#BEBEBEFF",
            ),
            text_button: component(
                "#00000000",
                "#63636333",
                "#16161680",
                "#63D0DFFF",
                "#63D0DF33",
                "#BEBEBEFF",
            ),
            shade: h("#00000052"),
        }
    }

    /// The stock `cosmic-light` semantic colors.
    pub fn light() -> Self {
        Self {
            button: component(
                "#2E2E2E40",
                "#2B2B2B66",
                "#6868689F",
                "#272727FF",
                "#27272733",
                "#161616FF",
            ),
            accent: component(
                "#00525AFF",
                "#14555CFF",
                "#5F888CFF",
                "#FFFFFFFF",
                "#FFFFFFFF",
                "#00525AFF",
            ),
            accent_button: component(
                "#00525AFF",
                "#14555CFF",
                "#5F888CFF",
                "#E1E1E1FF",
                "#DEDEDEFF",
                "#00525AFF",
            ),
            success: component(
                "#00572CFF",
                "#145937FF",
                "#5F8A75FF",
                "#FFFFFFFF",
                "#FFFFFFFF",
                "#00572CFF",
            ),
            success_button: component(
                "#00572CFF",
                "#145937FF",
                "#5F8A75FF",
                "#FFFFFFFF",
                "#DEDEDEFF",
                "#00572CFF",
            ),
            destructive: component(
                "#890418FF",
                "#811727FF",
                "#A3616BFF",
                "#FFFFFFFF",
                "#FFFFFFFF",
                "#890418FF",
            ),
            destructive_button: component(
                "#890418FF",
                "#811727FF",
                "#A3616BFF",
                "#FFFFFFFF",
                "#DEDEDEFF",
                "#890418FF",
            ),
            warning: component(
                "#792C00FF",
                "#753714FF",
                "#9B755FFF",
                "#FFFFFFFF",
                "#FFFFFFFF",
                "#792C00FF",
            ),
            warning_button: component(
                "#792C00FF",
                "#753714FF",
                "#9B755FFF",
                "#FFFFFFFF",
                "#000000FF",
                "#792C00FF",
            ),
            icon_button: component(
                "#00000000",
                "#63636333",
                "#BEBEBE80",
                "#161616FF",
                "#16161633",
                "#161616FF",
            ),
            link_button: component(
                "#00000000",
                "#00000000",
                "#00000000",
                "#00525AFF",
                "#00525A33",
                "#161616FF",
            ),
            list_button: component(
                "#00000000",
                "#00000000",
                "#BEBEBE80",
                "#272727FF",
                "#27272733",
                "#161616FF",
            ),
            text_button: component(
                "#00000000",
                "#63636333",
                "#BEBEBE80",
                "#00525AFF",
                "#00525A33",
                "#161616FF",
            ),
            shade: h("#00000014"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_and_light_accents_are_distinct() {
        assert_ne!(
            CosmicSemanticColors::dark().accent.base,
            CosmicSemanticColors::light().accent.base
        );
    }

    #[test]
    fn ghost_buttons_still_show_a_pressed_state() {
        // Ghost-styled roles (icon/list/text buttons) are transparent at
        // rest; assert they still darken on press so the role is not
        // simply dead. `link_button` is the one COSMIC role that stays
        // fully transparent through every interaction state (verified
        // against the real crate) — its affordance is entirely in `on`,
        // so it is intentionally excluded here.
        for (name, c) in [
            ("icon_button", CosmicSemanticColors::dark().icon_button),
            ("list_button", CosmicSemanticColors::dark().list_button),
            ("text_button", CosmicSemanticColors::dark().text_button),
        ] {
            assert_ne!(
                c.base, c.pressed,
                "{name} pressed state must differ from base"
            );
        }
    }
}
