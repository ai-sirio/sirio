//! Ely's colour vocabulary — `Palette`, `Syntax`, `Mode` — split out of
//! `ely-gpui-component`'s `theme` module (upstream `src/theme/palette.rs` and
//! `src/theme/syntax.rs` at e17e31a6890c09ebcfa8b61133d7bc7c625edf69), so
//! Sirio's theme crate can hold Ely's palette without compiling Ely's
//! components. See `../ely-gpui-component/LOCAL-CHANGES.md`.

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
