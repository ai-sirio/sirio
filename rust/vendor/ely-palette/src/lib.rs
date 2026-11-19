//! Ely's colour vocabulary — `Palette`, `Syntax`, `Mode` — split out of
//! upstream `ely-core`'s `theme` module (`theme/palette.rs`, `theme/syntax.rs`
//! and `Mode` at 8d7723ec), so Sirio's theme crate can hold Ely's palette
//! without compiling Ely's components. `ely-core` re-exports these types at
//! `theme::{Palette, Syntax, Mode, ...}`. See `../ely/LOCAL-CHANGES.md`.

use gpui::WindowAppearance;

mod palette;
mod syntax;

pub use palette::*;
pub use syntax::{SyntaxTheme, syntax_themes};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Light,
    Dark,
}

impl From<WindowAppearance> for Mode {
    fn from(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Mode::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => Mode::Light,
        }
    }
}
