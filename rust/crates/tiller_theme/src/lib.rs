//! Tiller's shared color, spacing, and typography tokens.
//!
//! The values in this crate are a direct port of `AppTheme.swift` and
//! `AppSurfaceColor.swift`. A resolved [`Theme`] is installed as a GPUI global
//! so views can retrieve the same tokens from their render context.

use gpui::{App, FontWeight, Global, Pixels, Rgba, Size, WindowAppearance, px, size};
use std::ops::Deref;

/// The appearance selected by Tiller's appearance setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    /// Follow the current system/window appearance.
    #[default]
    System,
    /// Always use the light palette.
    Light,
    /// Always use the dark palette.
    Dark,
}

/// The resolved light/dark appearance of an active [`Theme`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    /// The light palette.
    Light,
    /// The dark palette.
    Dark,
}

impl From<WindowAppearance> for Appearance {
    fn from(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::Light,
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::Dark,
        }
    }
}

impl ThemeMode {
    fn resolve(self, system_appearance: WindowAppearance) -> Appearance {
        match self {
            Self::System => system_appearance.into(),
            Self::Light => Appearance::Light,
            Self::Dark => Appearance::Dark,
        }
    }
}

/// All adaptive colors used by Tiller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeColors {
    /// Sidebar and chrome surface (`#1B1C1F` dark, `#E9EAED` light).
    pub background: Rgba,
    /// Window canvas behind the floating cards (`#131417` dark, `#DCDDE2` light).
    pub canvas: Rgba,
    /// Terminal surface, kept separate because the terminal subsystem consumes this name.
    pub terminal_surface: Rgba,
    /// Focused tab accent.
    pub tab_focus_accent: Rgba,
    /// Tab status accent for an agent waiting for input.
    pub tab_needs_input: Rgba,
    /// Tab status accent for a completed agent.
    pub tab_done: Rgba,
    /// Tab status accent for an errored agent.
    pub tab_error: Rgba,
    /// Chat transcript surface.
    pub chat_surface: Rgba,
    /// Tint used by the sidebar, tab bar, and usage bar material.
    pub chrome_tint: Rgba,
    /// Tab-chip underline.
    pub tab_chip_underline: Rgba,
    /// Shared one-pixel border/divider stroke.
    pub hairline: Rgba,
    /// Hover fill for sidebar rows.
    pub row_hover: Rgba,
    /// Hover fill for transcript rows.
    pub chat_row_hover: Rgba,
    /// Selected row fill.
    pub selection_fill: Rgba,
    /// Selected row ring.
    pub selection_ring: Rgba,
    /// Row title text.
    pub title: Rgba,
    /// Selected row title text.
    pub title_selected: Rgba,
    /// Secondary row text.
    pub subtitle: Rgba,
    /// Caption/meta text.
    pub meta: Rgba,
    /// Primary pill fill.
    pub primary_pill_bg: Rgba,
    /// Filter field fill.
    pub filter_field_bg: Rgba,
    /// Tree guide stroke, including its source alpha.
    pub tree_guide: Rgba,
    /// Staged-file status color.
    pub git_staged: Rgba,
    /// Modified-file status color.
    pub git_modified: Rgba,
    /// Untracked-file status color.
    pub git_untracked: Rgba,
    /// Conflict-file status color.
    pub git_conflict: Rgba,
    /// Addition diff accent.
    pub diff_addition: Rgba,
    /// Addition diff background.
    pub diff_addition_background: Rgba,
    /// Deletion diff accent.
    pub diff_deletion: Rgba,
    /// Deletion diff background.
    pub diff_deletion_background: Rgba,
    /// Hunk diff background.
    pub diff_hunk_background: Rgba,
    /// Chat card fill.
    pub card_fill: Rgba,
    /// Recessed code/diff fill.
    pub code_inset_fill: Rgba,
    /// Composer primary text. SwiftUI obtains this from the semantic system text color;
    /// GPUI's resolved theme uses the corresponding tuned title token.
    pub primary_text_color: Rgba,
    /// Clickable file-link color.
    pub file_link: Rgba,
    /// Task-card accent rail.
    pub rail_task: Rgba,
    /// Question-card accent rail.
    pub rail_question: Rgba,
    /// Edit-card accent rail.
    pub rail_edit: Rgba,
    /// Tool-card accent rail.
    pub rail_tool: Rgba,
}

impl ThemeColors {
    fn adaptive(light: Rgba, dark: Rgba, appearance: Appearance) -> Rgba {
        match appearance {
            Appearance::Light => light,
            Appearance::Dark => dark,
        }
    }

    fn for_appearance(appearance: Appearance) -> Self {
        let background = Self::adaptive(
            color(233.0 / 255.0, 234.0 / 255.0, 237.0 / 255.0, 1.0),
            color(27.0 / 255.0, 28.0 / 255.0, 31.0 / 255.0, 1.0),
            appearance,
        );
        let canvas = Self::adaptive(
            color(220.0 / 255.0, 221.0 / 255.0, 226.0 / 255.0, 1.0),
            color(19.0 / 255.0, 20.0 / 255.0, 23.0 / 255.0, 1.0),
            appearance,
        );
        let terminal_surface = Self::adaptive(
            color(233.0 / 255.0, 234.0 / 255.0, 237.0 / 255.0, 1.0),
            color(27.0 / 255.0, 28.0 / 255.0, 31.0 / 255.0, 1.0),
            appearance,
        );
        let tab_focus_accent = Self::adaptive(
            color(0.24, 0.38, 0.78, 1.0),
            color(0.55, 0.64, 1.00, 1.0),
            appearance,
        );
        let tab_needs_input = Self::adaptive(
            color(0.67, 0.42, 0.02, 1.0),
            color(0.95, 0.72, 0.28, 1.0),
            appearance,
        );
        let tab_done = Self::adaptive(
            color(0.10, 0.45, 0.22, 1.0),
            color(0.48, 0.78, 0.57, 1.0),
            appearance,
        );
        let tab_error = Self::adaptive(
            color(0.68, 0.12, 0.17, 1.0),
            color(0.94, 0.43, 0.47, 1.0),
            appearance,
        );
        let chat_surface = terminal_surface;
        let tab_chip_underline = Self::adaptive(
            color(0.62, 0.64, 0.72, 1.0),
            color(0.52, 0.55, 0.64, 1.0),
            appearance,
        );
        let hairline = Self::adaptive(
            color(0.82, 0.83, 0.87, 1.0),
            color(0.25, 0.26, 0.31, 1.0),
            appearance,
        );
        let row_hover = Self::adaptive(
            color(0.90, 0.905, 0.93, 1.0),
            color(32.0 / 255.0, 36.0 / 255.0, 45.0 / 255.0, 1.0),
            appearance,
        );
        let chat_row_hover = Self::adaptive(
            color(237.0 / 255.0, 237.0 / 255.0, 240.0 / 255.0, 1.0),
            color(48.0 / 255.0, 49.0 / 255.0, 53.0 / 255.0, 1.0),
            appearance,
        );
        let selection_fill = Self::adaptive(
            color(0.85, 0.86, 0.91, 1.0),
            color(0.169, 0.184, 0.227, 1.0),
            appearance,
        );
        let selection_ring = Self::adaptive(
            color(0.72, 0.74, 0.82, 1.0),
            color(0.227, 0.251, 0.314, 1.0),
            appearance,
        );
        let title = Self::adaptive(
            color(0.15, 0.16, 0.20, 1.0),
            color(242.0 / 255.0, 243.0 / 255.0, 245.0 / 255.0, 1.0),
            appearance,
        );
        let title_selected = Self::adaptive(
            color(0.05, 0.05, 0.08, 1.0),
            color(242.0 / 255.0, 243.0 / 255.0, 245.0 / 255.0, 1.0),
            appearance,
        );
        let subtitle = Self::adaptive(
            color(0.35, 0.37, 0.45, 1.0),
            color(168.0 / 255.0, 171.0 / 255.0, 178.0 / 255.0, 1.0),
            appearance,
        );
        let meta = Self::adaptive(
            color(0.38, 0.40, 0.48, 1.0),
            color(168.0 / 255.0, 171.0 / 255.0, 178.0 / 255.0, 1.0),
            appearance,
        );
        let primary_pill_bg = Self::adaptive(
            color(0.88, 0.885, 0.92, 1.0),
            color(52.0 / 255.0, 53.0 / 255.0, 57.0 / 255.0, 1.0),
            appearance,
        );
        let filter_field_bg = Self::adaptive(
            color(1.0, 1.0, 1.0, 1.0),
            color(52.0 / 255.0, 53.0 / 255.0, 57.0 / 255.0, 1.0),
            appearance,
        );
        let tree_guide = Self::adaptive(
            color(0.0, 0.0, 0.0, 0.12),
            color(1.0, 1.0, 1.0, 0.14),
            appearance,
        );
        let git_staged = Self::adaptive(
            color(0.08, 0.42, 0.20, 1.0),
            color(0.55, 0.82, 0.63, 1.0),
            appearance,
        );
        let git_modified = Self::adaptive(
            color(0.58, 0.35, 0.05, 1.0),
            color(0.91, 0.69, 0.36, 1.0),
            appearance,
        );
        let git_untracked = Self::adaptive(
            color(0.08, 0.36, 0.60, 1.0),
            color(0.43, 0.68, 0.91, 1.0),
            appearance,
        );
        let git_conflict = Self::adaptive(
            color(0.62, 0.12, 0.16, 1.0),
            color(0.90, 0.58, 0.60, 1.0),
            appearance,
        );
        let diff_addition_background = Self::adaptive(
            color(0.88, 0.96, 0.90, 1.0),
            color(0.08, 0.24, 0.14, 1.0),
            appearance,
        );
        let diff_deletion_background = Self::adaptive(
            color(0.98, 0.89, 0.90, 1.0),
            color(0.27, 0.08, 0.10, 1.0),
            appearance,
        );
        let diff_hunk_background = Self::adaptive(
            color(0.88, 0.92, 0.98, 1.0),
            color(0.10, 0.17, 0.28, 1.0),
            appearance,
        );
        let card_fill = Self::adaptive(
            color(0.91, 0.915, 0.94, 1.0),
            color(44.0 / 255.0, 47.0 / 255.0, 57.0 / 255.0, 1.0),
            appearance,
        );
        let code_inset_fill = Self::adaptive(
            color(1.0, 1.0, 1.0, 1.0),
            color(13.0 / 255.0, 14.0 / 255.0, 16.0 / 255.0, 1.0),
            appearance,
        );
        let rail_task = Self::adaptive(
            color(0.36, 0.30, 0.68, 1.0),
            color(0.49, 0.42, 0.84, 1.0),
            appearance,
        );
        let rail_tool = Self::adaptive(
            color(0.55, 0.57, 0.65, 1.0),
            color(0.40, 0.42, 0.50, 1.0),
            appearance,
        );

        Self {
            background,
            canvas,
            terminal_surface,
            tab_focus_accent,
            tab_needs_input,
            tab_done,
            tab_error,
            chat_surface,
            chrome_tint: background,
            tab_chip_underline,
            hairline,
            row_hover,
            chat_row_hover,
            selection_fill,
            selection_ring,
            title,
            title_selected,
            subtitle,
            meta,
            primary_pill_bg,
            filter_field_bg,
            tree_guide,
            git_staged,
            git_modified,
            git_untracked,
            git_conflict,
            diff_addition: git_staged,
            diff_addition_background,
            diff_deletion: git_conflict,
            diff_deletion_background,
            diff_hunk_background,
            card_fill,
            code_inset_fill,
            primary_text_color: title,
            file_link: git_untracked,
            rail_task,
            rail_question: git_modified,
            rail_edit: git_staged,
            rail_tool,
        }
    }
}

/// Spacing and geometry tokens from `AppTheme`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spacing {
    /// Floating-card corner radius.
    pub card_corner_radius: Pixels,
    /// Gap between cards and the window edge.
    pub card_gap: Pixels,
    /// Floating-card shadow radius.
    pub card_shadow_radius: Pixels,
    /// Floating-card shadow vertical offset.
    pub card_shadow_y_offset: Pixels,
    /// Title-strip height.
    pub title_strip_height: Pixels,
    /// Traffic-light inset in the title strip.
    pub traffic_light_inset: Pixels,
    /// Title-strip icon size.
    pub title_strip_icon_size: Pixels,
    /// Title-bar button frame.
    pub titlebar_control_frame: Size<Pixels>,
    /// Spacing between title-bar controls.
    pub titlebar_control_spacing: Pixels,
    /// Bottom usage bar height.
    pub bottom_bar_height: Pixels,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            card_corner_radius: px(6.0),
            card_gap: px(10.0),
            card_shadow_radius: px(18.0),
            card_shadow_y_offset: px(6.0),
            title_strip_height: px(28.0),
            traffic_light_inset: px(78.0),
            title_strip_icon_size: px(13.0),
            titlebar_control_frame: size(px(24.0), px(24.0)),
            titlebar_control_spacing: px(2.0),
            bottom_bar_height: px(30.0),
        }
    }
}

/// Interface and code type-scale tokens ported from `AppFont.swift`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typography {
    /// Default UI base size, in points.
    pub base_size: Pixels,
    /// Code size, in points.
    pub code_size: Pixels,
    /// Code font weight (`SF Mono Light`).
    pub code_weight: FontWeight,
    /// Large-title size.
    pub large_title: Pixels,
    /// Title size.
    pub title: Pixels,
    /// Title-2 size.
    pub title2: Pixels,
    /// Title-3 size.
    pub title3: Pixels,
    /// Headline/body size.
    pub headline: Pixels,
    /// Callout/subheadline size.
    pub callout: Pixels,
    /// Footnote/caption size.
    pub footnote: Pixels,
    /// Caption-2 size.
    pub caption2: Pixels,
}

impl Typography {
    /// Returns the default 13-point scale from `AppFont`.
    pub fn default_scale() -> Self {
        Self::for_base_size(13.0)
    }

    /// Returns the AppFont scale for a chosen base size.
    pub fn for_base_size(base_size: f32) -> Self {
        let delta = base_size - 13.0;
        let scaled = |points: f32| px((points + delta).max(6.0));

        Self {
            base_size: px(base_size),
            code_size: scaled(12.0),
            code_weight: FontWeight::LIGHT,
            large_title: scaled(23.0),
            title: scaled(20.0),
            title2: scaled(16.0),
            title3: scaled(14.0),
            headline: scaled(13.0),
            callout: scaled(12.0),
            footnote: scaled(11.0),
            caption2: scaled(10.0),
        }
    }
}

impl Default for Typography {
    fn default() -> Self {
        Self::default_scale()
    }
}

/// The resolved Tiller theme stored as a GPUI global.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// The requested appearance mode.
    pub mode: ThemeMode,
    /// The active resolved appearance.
    pub appearance: Appearance,
    /// Adaptive color tokens.
    pub colors: ThemeColors,
    /// Spacing and geometry tokens.
    pub spacing: Spacing,
    /// Typography tokens.
    pub typography: Typography,
    /// Shared opacity for translucent surfaces.
    pub translucent_surface_opacity: f32,
    /// Compatibility accessor for the original scaffold and the app shell.
    pub canvas: Rgba,
}

impl Global for Theme {}

impl Deref for Theme {
    type Target = ThemeColors;

    fn deref(&self) -> &Self::Target {
        &self.colors
    }
}

impl Theme {
    /// Returns the theme installed in the GPUI application global.
    pub fn get(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    /// Installs a theme, resolving `System` against the current window appearance.
    pub fn install(mode: ThemeMode, cx: &mut App) {
        cx.set_global(Self::for_mode(mode, cx.window_appearance()));
    }

    /// Installs a replacement theme mode in the GPUI global.
    pub fn set_mode(mode: ThemeMode, cx: &mut App) {
        Self::install(mode, cx);
    }

    /// Installs the system-following theme.
    pub fn init(cx: &mut App) {
        Self::install(ThemeMode::System, cx);
    }

    /// Returns a theme resolved for a requested mode and system appearance.
    pub fn for_mode(mode: ThemeMode, system_appearance: WindowAppearance) -> Self {
        let appearance = mode.resolve(system_appearance);
        Self::for_appearance(mode, appearance)
    }

    /// Returns a light theme without requiring a GPUI application context.
    pub fn light() -> Self {
        Self::for_appearance(ThemeMode::Light, Appearance::Light)
    }

    /// Returns a dark theme without requiring a GPUI application context.
    pub fn dark() -> Self {
        Self::for_appearance(ThemeMode::Dark, Appearance::Dark)
    }

    /// Returns a system-mode theme resolved against the supplied appearance.
    pub fn system(system_appearance: WindowAppearance) -> Self {
        Self::for_mode(ThemeMode::System, system_appearance)
    }

    /// Returns the surface opacity used when translucency is enabled.
    pub fn surface_opacity(translucency_enabled: bool) -> f32 {
        if translucency_enabled {
            0.96
        } else {
            1.0
        }
    }

    fn for_appearance(mode: ThemeMode, appearance: Appearance) -> Self {
        let colors = ThemeColors::for_appearance(appearance);
        Self {
            mode,
            appearance,
            canvas: colors.canvas,
            colors,
            spacing: Spacing::default(),
            typography: Typography::default(),
            translucent_surface_opacity: 0.96,
        }
    }
}

fn color(r: f32, g: f32, b: f32, a: f32) -> Rgba {
    Rgba { r, g, b, a }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expect_color(actual: Rgba, expected: (f32, f32, f32, f32)) {
        assert_eq!(actual.r, expected.0);
        assert_eq!(actual.g, expected.1);
        assert_eq!(actual.b, expected.2);
        assert_eq!(actual.a, expected.3);
    }

    #[test]
    fn dark_palette_matches_swift_sources() {
        let theme = Theme::dark();
        let f = |r, g, b| (r, g, b, 1.0);

        expect_color(theme.background, f(27.0 / 255.0, 28.0 / 255.0, 31.0 / 255.0));
        expect_color(theme.canvas, f(19.0 / 255.0, 20.0 / 255.0, 23.0 / 255.0));
        expect_color(theme.terminal_surface, f(27.0 / 255.0, 28.0 / 255.0, 31.0 / 255.0));
        expect_color(theme.tab_focus_accent, f(0.55, 0.64, 1.00));
        expect_color(theme.tab_needs_input, f(0.95, 0.72, 0.28));
        expect_color(theme.tab_done, f(0.48, 0.78, 0.57));
        expect_color(theme.tab_error, f(0.94, 0.43, 0.47));
        expect_color(theme.chat_surface, f(27.0 / 255.0, 28.0 / 255.0, 31.0 / 255.0));
        expect_color(theme.tab_chip_underline, f(0.52, 0.55, 0.64));
        expect_color(theme.hairline, f(0.25, 0.26, 0.31));
        expect_color(theme.row_hover, f(32.0 / 255.0, 36.0 / 255.0, 45.0 / 255.0));
        expect_color(theme.chat_row_hover, f(48.0 / 255.0, 49.0 / 255.0, 53.0 / 255.0));
        expect_color(theme.selection_fill, f(0.169, 0.184, 0.227));
        expect_color(theme.selection_ring, f(0.227, 0.251, 0.314));
        expect_color(theme.title, f(242.0 / 255.0, 243.0 / 255.0, 245.0 / 255.0));
        expect_color(theme.title_selected, f(242.0 / 255.0, 243.0 / 255.0, 245.0 / 255.0));
        expect_color(theme.subtitle, f(168.0 / 255.0, 171.0 / 255.0, 178.0 / 255.0));
        expect_color(theme.meta, f(168.0 / 255.0, 171.0 / 255.0, 178.0 / 255.0));
        expect_color(theme.primary_pill_bg, f(52.0 / 255.0, 53.0 / 255.0, 57.0 / 255.0));
        expect_color(theme.filter_field_bg, f(52.0 / 255.0, 53.0 / 255.0, 57.0 / 255.0));
        expect_color(theme.tree_guide, (1.0, 1.0, 1.0, 0.14));
        expect_color(theme.git_staged, f(0.55, 0.82, 0.63));
        expect_color(theme.git_modified, f(0.91, 0.69, 0.36));
        expect_color(theme.git_untracked, f(0.43, 0.68, 0.91));
        expect_color(theme.git_conflict, f(0.90, 0.58, 0.60));
        expect_color(theme.diff_addition, f(0.55, 0.82, 0.63));
        expect_color(theme.diff_addition_background, f(0.08, 0.24, 0.14));
        expect_color(theme.diff_deletion, f(0.90, 0.58, 0.60));
        expect_color(theme.diff_deletion_background, f(0.27, 0.08, 0.10));
        expect_color(theme.diff_hunk_background, f(0.10, 0.17, 0.28));
        expect_color(theme.card_fill, f(44.0 / 255.0, 47.0 / 255.0, 57.0 / 255.0));
        expect_color(theme.code_inset_fill, f(13.0 / 255.0, 14.0 / 255.0, 16.0 / 255.0));
        expect_color(theme.primary_text_color, f(242.0 / 255.0, 243.0 / 255.0, 245.0 / 255.0));
        expect_color(theme.file_link, f(0.43, 0.68, 0.91));
        expect_color(theme.rail_task, f(0.49, 0.42, 0.84));
        expect_color(theme.rail_question, f(0.91, 0.69, 0.36));
        expect_color(theme.rail_edit, f(0.55, 0.82, 0.63));
        expect_color(theme.rail_tool, f(0.40, 0.42, 0.50));
    }

    #[test]
    fn light_palette_matches_swift_sources() {
        let theme = Theme::light();
        let f = |r, g, b| (r, g, b, 1.0);

        expect_color(theme.background, f(233.0 / 255.0, 234.0 / 255.0, 237.0 / 255.0));
        expect_color(theme.canvas, f(220.0 / 255.0, 221.0 / 255.0, 226.0 / 255.0));
        expect_color(theme.terminal_surface, f(233.0 / 255.0, 234.0 / 255.0, 237.0 / 255.0));
        expect_color(theme.tab_focus_accent, f(0.24, 0.38, 0.78));
        expect_color(theme.tab_needs_input, f(0.67, 0.42, 0.02));
        expect_color(theme.tab_done, f(0.10, 0.45, 0.22));
        expect_color(theme.tab_error, f(0.68, 0.12, 0.17));
        expect_color(theme.chat_surface, f(233.0 / 255.0, 234.0 / 255.0, 237.0 / 255.0));
        expect_color(theme.tab_chip_underline, f(0.62, 0.64, 0.72));
        expect_color(theme.hairline, f(0.82, 0.83, 0.87));
        expect_color(theme.row_hover, f(0.90, 0.905, 0.93));
        expect_color(theme.chat_row_hover, f(237.0 / 255.0, 237.0 / 255.0, 240.0 / 255.0));
        expect_color(theme.selection_fill, f(0.85, 0.86, 0.91));
        expect_color(theme.selection_ring, f(0.72, 0.74, 0.82));
        expect_color(theme.title, f(0.15, 0.16, 0.20));
        expect_color(theme.title_selected, f(0.05, 0.05, 0.08));
        expect_color(theme.subtitle, f(0.35, 0.37, 0.45));
        expect_color(theme.meta, f(0.38, 0.40, 0.48));
        expect_color(theme.primary_pill_bg, f(0.88, 0.885, 0.92));
        expect_color(theme.filter_field_bg, f(1.0, 1.0, 1.0));
        expect_color(theme.tree_guide, (0.0, 0.0, 0.0, 0.12));
        expect_color(theme.git_staged, f(0.08, 0.42, 0.20));
        expect_color(theme.git_modified, f(0.58, 0.35, 0.05));
        expect_color(theme.git_untracked, f(0.08, 0.36, 0.60));
        expect_color(theme.git_conflict, f(0.62, 0.12, 0.16));
        expect_color(theme.diff_addition, f(0.08, 0.42, 0.20));
        expect_color(theme.diff_addition_background, f(0.88, 0.96, 0.90));
        expect_color(theme.diff_deletion, f(0.62, 0.12, 0.16));
        expect_color(theme.diff_deletion_background, f(0.98, 0.89, 0.90));
        expect_color(theme.diff_hunk_background, f(0.88, 0.92, 0.98));
        expect_color(theme.card_fill, f(0.91, 0.915, 0.94));
        expect_color(theme.code_inset_fill, f(1.0, 1.0, 1.0));
        expect_color(theme.primary_text_color, f(0.15, 0.16, 0.20));
        expect_color(theme.file_link, f(0.08, 0.36, 0.60));
        expect_color(theme.rail_task, f(0.36, 0.30, 0.68));
        expect_color(theme.rail_question, f(0.58, 0.35, 0.05));
        expect_color(theme.rail_edit, f(0.08, 0.42, 0.20));
        expect_color(theme.rail_tool, f(0.55, 0.57, 0.65));
    }

    #[test]
    fn every_adaptive_token_differs_between_light_and_dark() {
        let light = Theme::light().colors;
        let dark = Theme::dark().colors;
        let tokens = [
            ("background", light.background, dark.background),
            ("canvas", light.canvas, dark.canvas),
            ("terminal_surface", light.terminal_surface, dark.terminal_surface),
            ("tab_focus_accent", light.tab_focus_accent, dark.tab_focus_accent),
            ("tab_needs_input", light.tab_needs_input, dark.tab_needs_input),
            ("tab_done", light.tab_done, dark.tab_done),
            ("tab_error", light.tab_error, dark.tab_error),
            ("chat_surface", light.chat_surface, dark.chat_surface),
            ("chrome_tint", light.chrome_tint, dark.chrome_tint),
            ("tab_chip_underline", light.tab_chip_underline, dark.tab_chip_underline),
            ("hairline", light.hairline, dark.hairline),
            ("row_hover", light.row_hover, dark.row_hover),
            ("chat_row_hover", light.chat_row_hover, dark.chat_row_hover),
            ("selection_fill", light.selection_fill, dark.selection_fill),
            ("selection_ring", light.selection_ring, dark.selection_ring),
            ("title", light.title, dark.title),
            ("title_selected", light.title_selected, dark.title_selected),
            ("subtitle", light.subtitle, dark.subtitle),
            ("meta", light.meta, dark.meta),
            ("primary_pill_bg", light.primary_pill_bg, dark.primary_pill_bg),
            ("filter_field_bg", light.filter_field_bg, dark.filter_field_bg),
            ("tree_guide", light.tree_guide, dark.tree_guide),
            ("git_staged", light.git_staged, dark.git_staged),
            ("git_modified", light.git_modified, dark.git_modified),
            ("git_untracked", light.git_untracked, dark.git_untracked),
            ("git_conflict", light.git_conflict, dark.git_conflict),
            ("diff_addition", light.diff_addition, dark.diff_addition),
            (
                "diff_addition_background",
                light.diff_addition_background,
                dark.diff_addition_background,
            ),
            ("diff_deletion", light.diff_deletion, dark.diff_deletion),
            (
                "diff_deletion_background",
                light.diff_deletion_background,
                dark.diff_deletion_background,
            ),
            ("diff_hunk_background", light.diff_hunk_background, dark.diff_hunk_background),
            ("card_fill", light.card_fill, dark.card_fill),
            ("code_inset_fill", light.code_inset_fill, dark.code_inset_fill),
            ("primary_text_color", light.primary_text_color, dark.primary_text_color),
            ("file_link", light.file_link, dark.file_link),
            ("rail_task", light.rail_task, dark.rail_task),
            ("rail_question", light.rail_question, dark.rail_question),
            ("rail_edit", light.rail_edit, dark.rail_edit),
            ("rail_tool", light.rail_tool, dark.rail_tool),
        ];

        for (name, light, dark) in tokens {
            assert_ne!(light, dark, "adaptive token {name} did not change");
        }
    }

    #[test]
    fn system_mode_follows_window_appearance() {
        assert_eq!(
            Theme::system(WindowAppearance::Light).appearance,
            Appearance::Light
        );
        assert_eq!(
            Theme::system(WindowAppearance::VibrantLight).appearance,
            Appearance::Light
        );
        assert_eq!(
            Theme::system(WindowAppearance::Dark).appearance,
            Appearance::Dark
        );
        assert_eq!(
            Theme::system(WindowAppearance::VibrantDark).appearance,
            Appearance::Dark
        );
    }

    #[test]
    fn spacing_and_typography_match_swift_sources() {
        let spacing = Spacing::default();
        assert_eq!(spacing.card_corner_radius, px(6.0));
        assert_eq!(spacing.card_gap, px(10.0));
        assert_eq!(spacing.card_shadow_radius, px(18.0));
        assert_eq!(spacing.card_shadow_y_offset, px(6.0));
        assert_eq!(spacing.title_strip_height, px(28.0));
        assert_eq!(spacing.traffic_light_inset, px(78.0));
        assert_eq!(spacing.title_strip_icon_size, px(13.0));
        assert_eq!(spacing.titlebar_control_frame, size(px(24.0), px(24.0)));
        assert_eq!(spacing.titlebar_control_spacing, px(2.0));
        assert_eq!(spacing.bottom_bar_height, px(30.0));

        let typography = Typography::default();
        assert_eq!(typography.base_size, px(13.0));
        assert_eq!(typography.code_size, px(12.0));
        assert_eq!(typography.code_weight, FontWeight::LIGHT);
        assert_eq!(typography.large_title, px(23.0));
        assert_eq!(typography.title, px(20.0));
        assert_eq!(typography.title2, px(16.0));
        assert_eq!(typography.title3, px(14.0));
        assert_eq!(typography.headline, px(13.0));
        assert_eq!(typography.callout, px(12.0));
        assert_eq!(typography.footnote, px(11.0));
        assert_eq!(typography.caption2, px(10.0));
        assert_eq!(Theme::dark().translucent_surface_opacity, 0.96);
        assert_eq!(Theme::surface_opacity(true), 0.96);
        assert_eq!(Theme::surface_opacity(false), 1.0);
    }
}
