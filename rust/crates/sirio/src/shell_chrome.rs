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
/// Windows used to be on this list and is not any more (#144/#145, ADR 0002).
/// Sirio disables GPUI's DirectComposition on Windows so the Browser
/// surface's child HWND composes at all, and the fallback HWND path is
/// `DXGI_ALPHA_MODE_IGNORE` — the window is opaque no matter what is asked
/// for. Claiming blur here would leave the setting looking honoured while
/// nothing changed on screen.
pub(crate) fn current_platform_material(translucency_enabled: bool) -> ShellMaterial {
    resolve_material(
        translucency_enabled,
        cfg!(all(not(test), target_os = "macos")),
    )
}

impl ShellMaterial {
    pub(crate) fn window_background(self) -> WindowBackgroundAppearance {
        match self {
            Self::Opaque => WindowBackgroundAppearance::Opaque,
            Self::Blurred => WindowBackgroundAppearance::Blurred,
        }
    }

    pub(crate) fn frame_fill(self, theme: &Theme) -> gpui::Rgba {
        match self {
            Self::Opaque => theme.bg,
            Self::Blurred => theme.frame_surface,
        }
    }
}

/// A shell panel. Keyboard focus used to brighten this border to `text_muted`
/// (and before that, paint a second coral ring inside it — #58 already kept
/// the center panel out of that treatment). Both are gone: the pane's own
/// contents are the focus indicator, and every panel rests on `border_opaque`
/// no matter where the keyboard is.
pub(crate) fn panel(
    id: &'static str,
    focus_handle: &FocusHandle,
    theme: &Theme,
) -> Stateful<Div> {
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
            theme.frame_surface
        );
    }
}
