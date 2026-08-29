use gpui::{App, Div, FocusHandle, Stateful, Window, WindowBackgroundAppearance, div, prelude::*};
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
            Self::Opaque => theme.frame_fallback,
            Self::Blurred => theme.frame_surface,
        }
    }
}

/// #58: the center terminal panel never takes the shell focus treatment, even
/// when the keyboard is genuinely inside it — the pane's own contents are the
/// focus indicator there, and a ring around the whole terminal is noise.
///
/// A named constant rather than a `false` literal with a comment beside it, so
/// the decision is something a test can assert instead of something a reader
/// has to notice.
pub(crate) const CENTER_PANEL_FOCUS_VISIBLE: bool = false;

pub(crate) fn panel_border(theme: &Theme, focus_visible: bool) -> gpui::Rgba {
    if focus_visible {
        theme.panel_focus_ring
    } else {
        theme.panel_border
    }
}

pub(crate) fn focus_is_keyboard_visible(
    focus_handle: &FocusHandle,
    window: &Window,
    cx: &App,
) -> bool {
    window.last_input_was_keyboard() && focus_handle.contains_focused(window, cx)
}

/// A shell panel, with keyboard focus shown as a single brightened border.
///
/// This used to paint a second, absolutely-positioned ring inside the border
/// as well. Two rings in the accent coral was the loudest thing on screen; one
/// border in a neutral says the same thing — this pane has the keyboard —
/// without competing with the pane's own contents for attention.
pub(crate) fn panel(
    id: &'static str,
    focus_handle: &FocusHandle,
    focus_visible: bool,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .relative()
        .size_full()
        .bg(theme.panel_surface)
        .border_1()
        .border_color(panel_border(theme, focus_visible))
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
        assert_eq!(
            ShellMaterial::Opaque.frame_fill(&theme),
            theme.frame_fallback
        );
        assert_eq!(
            ShellMaterial::Blurred.frame_fill(&theme),
            theme.frame_surface
        );
    }

    #[test]
    fn panel_border_uses_focus_ring_only_when_focus_is_visible() {
        let theme = Theme::dark();

        assert_eq!(panel_border(&theme, false), theme.panel_border);
        assert_eq!(panel_border(&theme, true), theme.panel_focus_ring);
    }
}
