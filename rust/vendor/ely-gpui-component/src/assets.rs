use gpui::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;
use std::borrow::Cow;

/// Offline assets in the `ely/` namespace.
#[derive(RustEmbed)]
#[folder = "assets"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(path
            .strip_prefix("ely/")
            .and_then(Self::get)
            .map(|file| file.data))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .map(|name| format!("ely/{name}"))
            .filter(|name| name.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}
