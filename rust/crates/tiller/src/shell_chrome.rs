use gpui::{div, prelude::*, App, Div, FocusHandle, Stateful, Window, WindowBackgroundAppearance};
use tiller_theme::Theme;

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

pub(crate) fn current_platform_material(translucency_enabled: bool) -> ShellMaterial {
    resolve_material(
        translucency_enabled,
        cfg!(all(
            not(test),
            any(target_os = "macos", target_os = "windows")
        )),
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

pub(crate) fn panel(
    id: &'static str,
    focus_handle: &FocusHandle,
    focus_visible: bool,
    theme: &Theme,
) -> Stateful<Div> {
    let focus_ring_id = format!("{id}-focus-ring");

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
        .when(focus_visible, |this| {
            this.child(
                div()
                    .id(focus_ring_id.clone())
                    .debug_selector(move || focus_ring_id.clone())
                    .absolute()
                    .inset_0()
                    .border_1()
                    .border_color(theme.panel_focus_ring)
                    .rounded(theme.radii.shell_panel),
            )
        })
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
