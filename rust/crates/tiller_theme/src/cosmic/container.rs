//! The container hierarchy — COSMIC's distinctive structural idea: three
//! nested layers, `background` → `primary` → `secondary`, each carrying its
//! own base colour, `on` (text) colour, divider, and a nested [`CosmicComponent`]
//! for the hover/pressed overlays of widgets that live in that layer.
//!
//! Nested surfaces step *through* the hierarchy instead of each layer
//! picking its own grey: Tiller's shell is window → sidebar/terminal host →
//! cards/popovers, which maps onto background → primary → secondary
//! directly.

use super::component::CosmicComponent;
use gpui::Rgba;

/// One layer of the container hierarchy (`background`, `primary`, or
/// `secondary`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CosmicContainer {
    /// The layer's own fill.
    pub base: Rgba,
    /// Text/icon color drawn directly on `base`.
    pub on: Rgba,
    /// Divider color scoped to this layer (e.g. a sidebar's bottom rule).
    pub divider: Rgba,
    /// Colors for widgets (buttons, rows, chips, ...) that live in this
    /// layer: their own base/hover/pressed/on/divider/border.
    pub component: CosmicComponent,
}

/// The full three-layer hierarchy for one appearance (light or dark).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CosmicContainers {
    /// The window canvas — the outermost layer.
    pub background: CosmicContainer,
    /// The layer nested in `background` — sidebar, terminal host.
    pub primary: CosmicContainer,
    /// The layer nested in `primary` — cards, popovers, menus.
    pub secondary: CosmicContainer,
}
