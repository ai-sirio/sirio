//! Sirio's one name for Ely: a facade over the upstream `ely-*` crates vendored under
//! `vendor/ely/crates`. Sirio's code reaches Ely only through here.
//!
//! Upstream's own `init` is not used: it binds Tab, Shift-Tab and F6 globally and needs an asset
//! source serving its fonts. Sirio's two entry points are [`init_chat`] and [`init_editor`].
use std::borrow::Cow;

use gpui::{App, AssetSource, Global, Result, SharedString};

pub use ely_core::{
    buttons, data_display, feedback, forms, layout, lists, menus, motion, navigation, overlays,
    primitives, theme, typography,
};

pub use ely_agent as agent;
pub use ely_chat as chat;
pub use ely_editor as editor;

/// `ely-git`, with the file status its badges take.
pub mod git {
    pub use ely_core::lists::GitStatus;
    pub use ely_git::*;
}

/// Initialize chat components without default fonts or global shortcuts.
pub fn init_chat(cx: &mut App) {
    if cx.has_global::<theme::Theme>() {
        return;
    }
    theme::Theme::init(cx);
    forms::bind_keys(cx);
}

/// Initialize the code editor: the chat's setup, then the editor's keys.
/// Idempotent, and separate from `init_chat`'s guard, so a host that ran
/// `init_chat` first still gets the editor's keys.
pub fn init_editor(cx: &mut App) {
    if cx.has_global::<EditorKeysBound>() {
        return;
    }
    init_chat(cx);
    editor::bind_keys(cx);
    cx.set_global(EditorKeysBound);
}

/// Marks that `init_editor` has bound the editor's keys.
struct EditorKeysBound;

impl Global for EditorKeysBound {}

/// The path an Ely icon has in [`Assets`]: `ely/icons/<lucide name>.svg`.
pub trait IconPath {
    fn path(self) -> String;
}

impl IconPath for primitives::IconName {
    fn path(self) -> String {
        format!("ely/icons/{}.svg", self.name())
    }
}

/// Ely's icons in the `ely/` namespace, as the old crate served them. Upstream draws an icon
/// from its embedded bytes, so only a caller that wants a path (the chat probe) looks one up.
pub struct Assets;

impl Assets {
    fn icon(path: &str) -> Option<primitives::IconName> {
        let name = path.strip_prefix("ely/icons/")?.strip_suffix(".svg")?;
        primitives::IconName::ALL
            .iter()
            .copied()
            .find(|icon| icon.name() == name)
    }
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(Self::icon(path).map(|icon| Cow::Borrowed(icon.svg())))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(primitives::IconName::ALL
            .iter()
            .map(|icon| icon.path())
            .filter(|name| name.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}
