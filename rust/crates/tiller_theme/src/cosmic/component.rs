//! The semantic-component color shape — COSMIC's `Component`, reduced to
//! the fields this crate's call sites actually consume.
//!
//! Upstream's `cosmic_theme::Component` carries twelve fields (adding
//! `selected`, `selected_text`, `focus`, `disabled`, `on_disabled`,
//! `disabled_border`). This transcription keeps the six the brief calls
//! out — "base colour, on (text) colour, divider, and hover/pressed
//! overlays", plus `border` for focus/outline use — because nothing in
//! this crate reads the other six yet. Adding them later is additive: it
//! does not change the meaning of the six kept here.

use gpui::Rgba;

/// One semantic color's full interaction range, e.g. `accent` or
/// `destructive_button`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CosmicComponent {
    /// Resting fill (or, for outline-only components, resting text/icon).
    pub base: Rgba,
    /// Fill on hover.
    pub hover: Rgba,
    /// Fill while pressed.
    pub pressed: Rgba,
    /// Text/icon color drawn on top of `base`.
    pub on: Rgba,
    /// Divider color scoped to this component (e.g. a button's own rule).
    pub divider: Rgba,
    /// Border/outline color, also used for focus rings.
    pub border: Rgba,
}
