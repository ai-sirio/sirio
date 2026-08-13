//! [`CosmicTheme`] — the composed COSMIC token set.
//!
//! COSMIC-01 installed this as a *second*, independent GPUI [`Global`]
//! alongside [`crate::Theme`], for a surface to opt into by reading
//! `CosmicTheme::get(cx)` instead of `Theme::get(cx)`. COSMIC-02 retires
//! that seam: [`crate::Theme`] now carries a `cosmic: CosmicTheme` field of
//! its own (populated in `Theme::for_appearance`, via [`Self::resolve`]),
//! so every surface that already reads `Theme::get(cx)` — which is all of
//! them — gets COSMIC tokens for free, with no separate install step. A
//! standalone `CosmicTheme` global with its own `install`/`init`/`get`/
//! `set_mode` would now be dead the moment `Theme` carries it too — the
//! exact "built, tested, called by nothing" shape `DEAD-MODELS.md` warns
//! about — so that API is gone; [`Self::resolve`]/`for_mode`/`light`/`dark`
//! remain as the pure, context-free constructors `Theme`'s own resolution
//! path (and this module's tests) use.
//!
//! `resolve_system`/`resolve` on [`ThemeMode`] are `crate::lib`'s own
//! private methods (visible here because this module is a descendant of
//! the crate root, not re-exported) — reused rather than re-implemented,
//! so "system" cannot mean two different things between the token sets.

use super::container::CosmicContainers;
use super::live;
use super::radii::CosmicRadii;
use super::semantic::CosmicSemanticColors;
use super::spacing::CosmicSpacing;
use crate::{Appearance, ThemeMode};
use gpui::WindowAppearance;

/// The full COSMIC token set for one resolved appearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CosmicTheme {
    /// The mode this theme was resolved from (`System` re-resolves on the
    /// XDG portal, same as `Theme`).
    pub mode: ThemeMode,
    /// Whether this resolved theme is the dark variant.
    pub is_dark: bool,
    /// The container hierarchy: `background` → `primary` → `secondary`.
    pub containers: CosmicContainers,
    /// Named semantic component colors (`accent`, `success`, ...).
    pub semantic: CosmicSemanticColors,
    /// The ten-step spacing scale.
    pub spacing: CosmicSpacing,
    /// The six-step corner-radius scale.
    pub radii: CosmicRadii,
    /// Whether this resolved theme's colors came from the user's real
    /// COSMIC configuration (`true`) or the transcribed stock default
    /// (`false`) — surfaced so a diagnostic view (or a test) can tell the
    /// two apart without re-deriving it.
    pub accent_is_live: bool,
}

impl CosmicTheme {
    /// Returns a `CosmicTheme` resolved for `mode` and the given window
    /// appearance, without requiring a live app context — used by tests
    /// and by [`crate::Theme::for_mode`]/`for_mode_linux` (via
    /// [`Self::resolve`]) alike.
    pub fn for_mode(mode: ThemeMode, system_appearance: WindowAppearance) -> Self {
        #[cfg(target_os = "linux")]
        let appearance = mode.resolve_system(system_appearance);
        #[cfg(not(target_os = "linux"))]
        let appearance = mode.resolve(system_appearance);
        Self::resolve(mode, appearance)
    }

    /// Returns a light `CosmicTheme` without requiring a GPUI app context.
    pub fn light() -> Self {
        Self::resolve(ThemeMode::Light, Appearance::Light)
    }

    /// Returns a dark `CosmicTheme` without requiring a GPUI app context.
    pub fn dark() -> Self {
        Self::resolve(ThemeMode::Dark, Appearance::Dark)
    }

    /// Resolves a `CosmicTheme` for an already-resolved `(mode, appearance)`
    /// pair, trying the live on-disk COSMIC config
    /// ([`live::detect`]) first and falling back to the transcribed stock
    /// palette. `pub(crate)` rather than private: [`crate::Theme::for_appearance`]
    /// calls this directly with its own already-resolved `(mode,
    /// appearance)`, so `Theme`'s resolution and this one can never
    /// disagree about what "System" or "the live reader" mean — the same
    /// reasoning `COSMIC-DESIGN.md` gives for reusing `ThemeMode::resolve`
    /// instead of re-implementing it.
    pub(crate) fn resolve(mode: ThemeMode, appearance: Appearance) -> Self {
        let is_dark = matches!(appearance, Appearance::Dark);

        if let Some(live) = live::detect()
            && live.is_dark == is_dark
        {
            return Self {
                mode,
                is_dark,
                containers: live.containers,
                semantic: live.semantic,
                spacing: CosmicSpacing::default(),
                radii: CosmicRadii::default(),
                accent_is_live: true,
            };
        }

        Self {
            mode,
            is_dark,
            containers: if is_dark {
                CosmicContainers::dark()
            } else {
                CosmicContainers::light()
            },
            semantic: if is_dark {
                CosmicSemanticColors::dark()
            } else {
                CosmicSemanticColors::light()
            },
            spacing: CosmicSpacing::default(),
            radii: CosmicRadii::default(),
            accent_is_live: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_mode_resolves_to_the_dark_container_hierarchy() {
        let theme = CosmicTheme::for_mode(ThemeMode::Dark, WindowAppearance::Dark);
        assert!(theme.is_dark);
        // Background must be the darkest of the three layers even when the
        // live reader is in play on this machine.
        assert!(theme.containers.background.base.r <= theme.containers.secondary.base.r);
    }

    #[test]
    fn light_mode_resolves_to_the_light_container_hierarchy() {
        let theme = CosmicTheme::for_mode(ThemeMode::Light, WindowAppearance::Light);
        assert!(!theme.is_dark);
    }

    #[test]
    fn system_mode_never_panics_and_always_resolves() {
        // Exercises the same "unresolved portal defaults dark" rule as
        // `Theme` — this must not depend on GPUI having heard from the
        // XDG portal.
        let theme = CosmicTheme::for_mode(ThemeMode::System, WindowAppearance::Light);
        let _ = theme.is_dark;
    }

    #[test]
    fn light_and_dark_helpers_do_not_require_an_app_context() {
        assert!(CosmicTheme::dark().is_dark);
        assert!(!CosmicTheme::light().is_dark);
    }
}
