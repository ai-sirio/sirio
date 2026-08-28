//! Pop!_OS COSMIC design tokens: container hierarchy, spacing scale,
//! corner radii, and semantic component colors — see
//! `docs/linux-rewrite/COSMIC-DESIGN.md` for the mapping contract and the
//! depend-vs-transcribe decision behind this module's existence.
//!
//! [`crate::Theme`] carries a `cosmic: `[`CosmicTheme`] field of its own
//! (COSMIC-02), resolved alongside the rest of the theme in
//! `Theme::for_appearance`. [`crate::ThemeColors`], [`crate::Spacing`], and
//! [`crate::Radii`] are untouched by this module and remain the waku-valued
//! tokens `controls.rs` and friends were already built on.

mod component;
mod container;
mod hex;
mod live;
mod palette;
mod radii;
mod semantic;
mod spacing;
mod theme;

pub use component::CosmicComponent;
pub use container::{CosmicContainer, CosmicContainers};
pub use live::LiveCosmicTheme;
pub use radii::CosmicRadii;
pub use semantic::CosmicSemanticColors;
pub use spacing::CosmicSpacing;
pub use theme::CosmicTheme;
