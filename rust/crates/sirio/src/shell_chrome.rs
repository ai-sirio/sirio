use gpui::{Div, FocusHandle, Stateful, WindowBackgroundAppearance, div, prelude::*};
use sirio_theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShellMaterial {
    Opaque,
    Blurred,
}

pub(crate) fn resolve_material(
    translucency_enabled: bool,
    native_blur_supported: bool,
) -> ShellMaterial {
    match (translucency_enabled, native_blur_supported) {
        (true, true) => ShellMaterial::Blurred,
        _ => ShellMaterial::Opaque,
    }
}

/// Whether this platform can actually paint a blurred window background.
///
/// macOS: native. Windows: through the acrylic accent policy GPUI sets for
/// `WindowBackgroundAppearance::Blurred` -- but only because Sirio vendors
/// `gpui_windows` with a bitblt swap chain on the no-DirectComposition path it
/// forces for the Browser surface (ADR 0002, ADR 0003). Upstream's flip-model
/// HWND swap chain there is composed opaque, which is why Windows was off this
/// list between #144 and ADR 0003: claiming blur would have left the setting
/// looking honoured while nothing changed on screen. Linux never claimed it.
pub(crate) fn current_platform_material(translucency_enabled: bool) -> ShellMaterial {
    resolve_material(
        translucency_enabled,
        cfg!(all(
            not(test),
            any(target_os = "macos", target_os = "windows")
        )),
    )
}

/// How far the shell's own veils are turned up over a blurred backdrop.
///
/// The theme's defaults (`frame_surface` at 0.85 dark / 0.80 light, panels
/// faded to [`Theme::surface_opacity`]) let a hint of the blurred desktop
/// through. Windows' acrylic accent -- the only blurred backdrop GPUI offers
/// there -- is nearly untinted, and at the earlier 0.70 frame default the
/// desktop bled through loudly enough to distract rather than hint (seen on
/// a real desktop, 2026-09-05), so Windows turned the frame veil up to 0.80;
/// the theme default has since passed that, so Windows now takes the theme's
/// frame as-is. Its panels stay opaque on purpose: GPUI's Windows renderer
/// accumulates alpha additively
/// (`SrcBlendAlpha`/`DestBlendAlpha` are both `ONE`), so any panel painted
/// over the frame veil saturates to alpha 1 before the DWM sees it -- a 0.45
/// fade measured as a darker panel (11 against 13) with nothing showing
/// through. Asking for a fade there would only shift the tint, which is the
/// theme's own "opaque beats an unblurred, partially transparent frame"
/// rule. Every other platform keeps the theme's defaults; none of them has
/// been looked at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MaterialStrength {
    /// Alpha the frame fill takes over the blur, `None` for the theme's own
    /// `frame_surface` alpha.
    pub(crate) frame_alpha: Option<f32>,
    /// Opacity the theme's structural surfaces fade to.
    pub(crate) surface_opacity: f32,
}

impl MaterialStrength {
    /// The theme's own veils, untouched.
    pub(crate) fn theme_default() -> Self {
        Self {
            frame_alpha: None,
            surface_opacity: Theme::surface_opacity(true),
        }
    }

    /// The subtler Windows veils: the theme's own frame -- the title strip
    /// and the gaps between panels -- and opaque panels.
    pub(crate) const WINDOWS: Self = Self {
        frame_alpha: None,
        surface_opacity: 1.0,
    };

    pub(crate) fn for_platform() -> Self {
        if cfg!(target_os = "windows") {
            Self::WINDOWS
        } else {
            Self::theme_default()
        }
    }

    pub(crate) fn frame_fill(self, frame_surface: gpui::Rgba) -> gpui::Rgba {
        match self.frame_alpha {
            Some(a) => gpui::Rgba { a, ..frame_surface },
            None => frame_surface,
        }
    }
}

impl ShellMaterial {
    pub(crate) fn window_background(self) -> WindowBackgroundAppearance {
        match self {
            Self::Opaque => WindowBackgroundAppearance::Opaque,
            Self::Blurred => WindowBackgroundAppearance::Blurred,
        }
    }

    pub(crate) fn frame_fill(self, theme: &Theme) -> gpui::Rgba {
        self.frame_fill_at(MaterialStrength::for_platform(), theme)
    }

    fn frame_fill_at(self, strength: MaterialStrength, theme: &Theme) -> gpui::Rgba {
        match self {
            Self::Opaque => theme.bg,
            Self::Blurred => strength.frame_fill(theme.frame_surface),
        }
    }

    /// Whether the theme's structural surfaces fade over this material.
    ///
    /// The fade follows the *resolved* material, not the preference: on a
    /// platform without native blur the window stays opaque, and a faded
    /// panel over an opaque frame would only shift its tint (the spec's
    /// "visual consistency is preferable to an unblurred, partially
    /// transparent frame" rule).
    pub(crate) fn fades_surfaces(self) -> bool {
        matches!(self, Self::Blurred)
    }

    /// Reinstalls the theme global with its surfaces faded (or not) for this
    /// material. Startup and the live toggle both go through here so the two
    /// cannot disagree about what a persisted preference looks like.
    pub(crate) fn apply_to_theme(self, cx: &mut gpui::App) {
        self.apply_to_theme_at(MaterialStrength::for_platform(), cx);
    }

    fn apply_to_theme_at(self, strength: MaterialStrength, cx: &mut gpui::App) {
        cx.set_global(
            Theme::get(cx).with_translucency_at(self.fades_surfaces(), strength.surface_opacity),
        );
    }
}

/// A shell panel. Keyboard focus used to brighten this border to `text_muted`
/// (and before that, paint a second coral ring inside it — #58 already kept
/// the center panel out of that treatment). Both are gone: the pane's own
/// contents are the focus indicator, and every panel rests on `border_opaque`
/// no matter where the keyboard is.
pub(crate) fn panel(id: &'static str, focus_handle: &FocusHandle, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .relative()
        .size_full()
        .bg(theme.surface)
        .border_1()
        .border_color(theme.border_opaque)
        .rounded(theme.radii.shell_panel)
        .overflow_hidden()
        .track_focus(focus_handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_requires_both_the_preference_and_native_support() {
        assert_eq!(resolve_material(false, false), ShellMaterial::Opaque);
        assert_eq!(resolve_material(false, true), ShellMaterial::Opaque);
        assert_eq!(resolve_material(true, false), ShellMaterial::Opaque);
        assert_eq!(resolve_material(true, true), ShellMaterial::Blurred);
    }

    #[test]
    fn current_platform_material_is_opaque_in_headless_tests() {
        assert_eq!(current_platform_material(true), ShellMaterial::Opaque);
    }

    #[test]
    fn material_maps_to_window_background_and_frame_fill() {
        let theme = Theme::dark();

        assert_eq!(
            ShellMaterial::Opaque.window_background(),
            WindowBackgroundAppearance::Opaque
        );
        assert_eq!(
            ShellMaterial::Blurred.window_background(),
            WindowBackgroundAppearance::Blurred
        );
        assert_eq!(ShellMaterial::Opaque.frame_fill(&theme), theme.bg);
        assert_eq!(
            ShellMaterial::Blurred.frame_fill(&theme),
            MaterialStrength::for_platform().frame_fill(theme.frame_surface),
            "the blurred frame is frame_surface at the platform's strength"
        );
        assert!(!ShellMaterial::Opaque.fades_surfaces());
        assert!(ShellMaterial::Blurred.fades_surfaces());
    }

    #[test]
    fn strength_turns_the_frame_veil_up_only_when_asked() {
        let theme = Theme::dark();
        let subtle = MaterialStrength {
            frame_alpha: Some(0.8),
            surface_opacity: 0.9,
        };
        assert_eq!(
            ShellMaterial::Blurred.frame_fill_at(MaterialStrength::theme_default(), &theme),
            theme.frame_surface
        );
        let fill = ShellMaterial::Blurred.frame_fill_at(subtle, &theme);
        assert_eq!(fill.a, 0.8);
        assert_eq!(
            (fill.r, fill.g, fill.b),
            (
                theme.frame_surface.r,
                theme.frame_surface.g,
                theme.frame_surface.b
            )
        );
        assert_eq!(
            ShellMaterial::Opaque.frame_fill_at(subtle, &theme),
            theme.bg,
            "an opaque frame ignores the strength"
        );
    }

    #[test]
    fn windows_is_the_only_platform_with_subtler_veils() {
        let strength = MaterialStrength::for_platform();
        if cfg!(target_os = "windows") {
            assert_eq!(strength, MaterialStrength::WINDOWS);
        } else {
            assert_eq!(strength, MaterialStrength::theme_default());
        }
        assert!(MaterialStrength::WINDOWS.surface_opacity > Theme::surface_opacity(true));
        assert_eq!(
            MaterialStrength::WINDOWS.frame_alpha,
            None,
            "the theme's frame is already the stronger veil"
        );
    }

    #[gpui::test]
    fn applying_a_material_fades_the_theme_only_when_blurred(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            Theme::init(cx);
            ShellMaterial::Blurred.apply_to_theme(cx);
            let theme = *Theme::get(cx);
            assert!(
                theme.translucency_enabled,
                "a blurred material fades the structural surfaces"
            );
            assert_eq!(
                theme.translucent_surface_opacity,
                MaterialStrength::for_platform().surface_opacity,
                "the fade is the platform's, not the theme's default"
            );
            ShellMaterial::Opaque.apply_to_theme(cx);
            assert!(
                !Theme::get(cx).translucency_enabled,
                "an opaque material restores the opaque surfaces"
            );
        });
    }
}
