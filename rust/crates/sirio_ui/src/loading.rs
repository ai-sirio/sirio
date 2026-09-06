//! The one place Bezel loading primitives are called from.
//!
//! OWNERSHIP: this module renders; it owns no business state. Every surface
//! keeps its own task/status state machine and asks here only for a picture of
//! it. Bezel imports live here and nowhere else, so a Bezel API change is a
//! one-file change.
//!
//! A note on the `id` every entry point takes: Bezel 0.1.3 currently discards
//! it (`loaders.rs` binds `let _key`, `popover.rs` takes `_id`), so today it
//! keys nothing and a collision would be symptomless. Pass a unique one anyway
//! — it is the identity these primitives are documented to take, and the day
//! Bezel starts honouring it, duplicates would become state bleeding between
//! elements with no compile error to warn you.

use std::time::Duration;

use bezel::motion::Painter;
use bezel::ui::loaders;
use bezel::ui::popover;
use bezel::ui::widgets::Controls;
use gpui::{
    AnyElement, App, Div, InteractiveElement, IntoElement, ParentElement, Styled, Window, div, px,
};
use sirio_theme::Theme;

/// The glyph slot in an Activity-derived reasoning header.
pub const THINKING_GLYPH: f32 = 14.0;
/// The generic orb for a full-surface first load or empty state.
pub const GENERIC_ORB: f32 = 44.0;
/// The cell size of the compact refresh spinner.
pub const COMPACT_MINI_CELL: f32 = 2.5;
/// Determinate progress: track thickness and the width the gallery demos.
pub const PROGRESS_TRACK: f32 = 4.0;
pub const PROGRESS_MAX_WIDTH: f32 = 280.0;
/// A subordinate skeleton: three rows, never the sole activity signal. Its
/// geometry belongs to Bezel's `popover::redacted_rows`: 28px rows, 6px gaps,
/// and 4px vertical padding.
pub const SKELETON_ROWS: usize = 3;

/// The label for a settled reasoning header.
///
/// `None` means Sirio has no truthful duration for the run, and the label says
/// so by omission rather than inventing an elapsed time.
pub fn thought_label(elapsed: Option<Duration>) -> String {
    match elapsed {
        Some(elapsed) => format!("Thought for {:.0}s", elapsed.as_secs_f32()),
        None => "Thought".to_string(),
    }
}

/// Clamp a caller's fraction into the unit range. A determinate bar shows a
/// truthful fraction of a known total; a caller that overshoots is pinned, not
/// panicked. `NaN` means the fraction is unknown, so it renders as empty.
pub fn clamp_fraction(fraction: f32) -> f32 {
    if fraction.is_nan() {
        0.0
    } else {
        fraction.clamp(0.0, 1.0)
    }
}

/// Build the Bezel palette expected by its published UI primitives while
/// retaining Sirio's appearance and accent. Bezel's loading APIs use their own
/// theme type, whereas the adapter's public contract intentionally exposes
/// Sirio's theme type to its callers.
///
/// This module used to push Sirio's appearance into Bezel's process-wide
/// paint mirror on every call, because `ink`, `wash` and `hairline` are free
/// functions with no `cx` and would otherwise paint for the previous
/// appearance. `Theme::sync_appearance` now does it wherever the theme global
/// is written, which is the one place that can be sure it happened.
fn bezel_theme(theme: &Theme) -> bezel::theme::Theme {
    let mut bezel_theme = match theme.appearance {
        sirio_theme::Appearance::Light => bezel::theme::Theme::light(),
        sirio_theme::Appearance::Dark => bezel::theme::Theme::dark(),
    };
    bezel_theme.accent = theme.brand_coral.into();
    bezel_theme
}

/// The view whose render is leasing the shared Bezel animation clock.
fn painter(window: &Window) -> Painter {
    Painter::from(window.current_view())
}

/// An Activity-derived reasoning indicator using Bezel's Cluster orb.
pub fn thinking_indicator(
    id: &'static str,
    theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let bezel_theme = bezel_theme(theme);
    loaders::orb(
        loaders::Orb::Cluster,
        id,
        THINKING_GLYPH,
        &bezel_theme,
        painter(window),
        cx,
    )
    .into_any_element()
}

/// A generic Bezel Cluster orb at the caller's requested size.
pub fn indeterminate(
    id: &'static str,
    size: f32,
    theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let bezel_theme = bezel_theme(theme);
    loaders::orb(
        loaders::Orb::Cluster,
        id,
        size,
        &bezel_theme,
        painter(window),
        cx,
    )
    .into_any_element()
}

/// The compact Bezel mini gradient spinner for refresh/status slots.
///
/// The spinner sits in a wrapper that carries `id` as its debug selector:
/// Bezel hangs no selector on the key it takes (see the module note), so
/// without the wrapper no test could tell a slot holding a spinner from
/// an empty one. Every caller puts it in a centered flex slot, so a flex
/// wrapper sized by its content is layout-neutral.
pub fn compact(id: &'static str, window: &mut Window, cx: &mut App) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .flex()
        .child(loaders::mini_gradient_spinner(
            id,
            COMPACT_MINI_CELL,
            painter(window),
            cx,
        ))
        .into_any_element()
}

/// A determinate progress bar with the gallery's fixed track and width.
pub fn progress(fraction: f32, theme: &Theme) -> Div {
    let bezel_theme = bezel_theme(theme);
    div()
        .w_full()
        .max_w(px(PROGRESS_MAX_WIDTH))
        .h(px(PROGRESS_TRACK))
        .child(bezel_theme.progress_bar(clamp_fraction(fraction)))
}

/// A subordinate set of redacted rows. Bezel's `pulse_delta` owns the shared
/// clock and its `reduced_motion` behavior; no Sirio timer is created here.
pub fn skeleton_rows(
    id: &'static str,
    count: usize,
    theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let bezel_theme = bezel_theme(theme);
    popover::redacted_rows(id, &bezel_theme, count, painter(window), cx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_run_without_a_truthful_duration_is_labelled_thought() {
        assert_eq!(thought_label(None), "Thought");
    }

    #[test]
    fn a_run_with_a_duration_reports_whole_seconds() {
        assert_eq!(
            thought_label(Some(Duration::from_millis(4400))),
            "Thought for 4s"
        );
        assert_eq!(
            thought_label(Some(Duration::from_millis(600))),
            "Thought for 1s"
        );
    }

    #[test]
    fn a_fraction_outside_the_unit_range_is_clamped_not_rejected() {
        assert_eq!(clamp_fraction(-0.5), 0.0);
        assert_eq!(clamp_fraction(1.7), 1.0);
        assert_eq!(clamp_fraction(0.35), 0.35);
        assert_eq!(clamp_fraction(f32::NAN), 0.0);
    }

    /// The palette handed to a Bezel primitive is built for Sirio's
    /// appearance, not for whatever Bezel's process-wide mirror last held.
    /// That mirror is `Theme::sync_appearance`'s job now, and it is tested in
    /// `sirio_theme`; what is still this module's job is picking the right
    /// side of Bezel's palette, and keeping Sirio's coral on the accent that
    /// its loaders paint with.
    #[test]
    fn the_bezel_palette_is_built_for_sirios_appearance() {
        let light = bezel_theme(&Theme::light());
        assert_eq!(light.bg, bezel::theme::Theme::light().bg);
        assert_eq!(light.accent, Theme::light().brand_coral.into());

        let dark = bezel_theme(&Theme::dark());
        assert_eq!(dark.bg, bezel::theme::Theme::dark().bg);
        assert_eq!(dark.accent, Theme::dark().brand_coral.into());
    }

    #[test]
    fn the_geometry_matches_the_gallery_evidence() {
        assert_eq!(THINKING_GLYPH, 14.0);
        assert_eq!(GENERIC_ORB, 44.0);
        assert_eq!(COMPACT_MINI_CELL, 2.5);
        assert_eq!(PROGRESS_TRACK, 4.0);
        assert_eq!(PROGRESS_MAX_WIDTH, 280.0);
        assert_eq!(SKELETON_ROWS, 3);
    }
}
