//! Sirio's boundary with Ely. Sirio remains the token authority: the host
//! installs Sirio's `Theme`, and this module keeps Ely's theme and
//! bezel-theme's registry following it, and serves Ely's assets beside
//! Sirio's own.
use crate::chat::identity;
use bezel::motion::AppExt;
use ely_gpui_component::theme::{Mode, Theme as ElyTheme, ThemeMetrics};
use gpui::{App, AssetSource, Global};
use sirio_theme::{Appearance, Theme};
use std::borrow::Cow;

pub struct AppAssets;
impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if path.starts_with("ely/") {
            return ely_gpui_component::Assets.load(path);
        }
        if path.starts_with("sirio-chat/") {
            return Ok(identity::asset(path).map(Cow::Borrowed));
        }
        bezel::ui::icons::Assets.load(path)
    }
    fn list(&self, path: &str) -> anyhow::Result<Vec<gpui::SharedString>> {
        let mut paths = bezel::ui::icons::Assets.list(path)?;
        paths.extend(ely_gpui_component::Assets.list(path)?);
        paths.extend(
            identity::ASSETS
                .iter()
                .filter(|(name, _)| name.starts_with(path))
                .map(|(name, _)| (*name).into()),
        );
        Ok(paths)
    }
}

#[derive(Default)]
struct LastTheme(Option<(Theme, bool)>);
impl Global for LastTheme {}

pub fn init(cx: &mut App) {
    ely_gpui_component::init_chat(cx);
    if !cx.has_global::<LastTheme>() {
        cx.set_global(LastTheme::default());
        cx.observe_global::<Theme>(sync_theme_if_changed).detach();
    }
    sync_theme_if_changed(cx);
}

pub(crate) fn sync_theme_if_changed(cx: &mut App) {
    if !cx.has_global::<LastTheme>() {
        init(cx);
        return;
    }
    let Some(theme) = cx.try_global::<Theme>().copied() else {
        return;
    };
    let reduced = cx.reduced_motion();
    if cx.global::<LastTheme>().0 == Some((theme, reduced)) {
        return;
    }
    cx.global_mut::<LastTheme>().0 = Some((theme, reduced));
    // Installers already do this; a global swapped without one (the portal
    // follower's path) reaches bezel here.
    theme.install_into_bezel(cx);
    let mode = match theme.appearance {
        Appearance::Light => Mode::Light,
        Appearance::Dark => Mode::Dark,
    };
    // `set_palette` starts Ely's cross-fade; `set_mode_now` ends it before its
    // first tick and lands on the palette at once, as Sirio's themes switch.
    ElyTheme::set_palette(mode, Some(theme.colors.ely), cx);
    ElyTheme::set_mode_now(mode, cx);
    ElyTheme::update(cx, |ely| {
        ely.font_family = theme.typography.ui_family.into();
        ely.mono_family = theme.typography.code_family.into();
        ely.reduced_motion = reduced;
        let t = theme.typography;
        let r = theme.radii;
        ely.metrics = Some(ThemeMetrics {
            text: [
                t.footnote,
                t.ui_size,
                t.base_size,
                t.callout,
                t.headline,
                t.title2,
                t.title,
                t.large_title,
            ],
            radii: [r.chip, r.control, r.code_block, r.composer],
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, WindowAppearance};
    use sirio_theme::{BaseColor, ThemeMode};

    fn boot(cx: &mut TestAppContext) {
        cx.update(|cx| {
            Theme::init(cx);
            init(cx);
        });
        cx.run_until_parked();
    }

    fn bezel_holds(cx: &App, theme: &Theme) -> bool {
        format!("{:?}", bezel::theme::Theme::of(cx)) == format!("{:?}", theme.to_bezel_theme())
    }

    /// What the Linux portal follower does: build the next theme and set it,
    /// with no installer in between.
    #[gpui::test]
    async fn a_theme_swapped_without_an_installer_reaches_both_consumers(cx: &mut TestAppContext) {
        boot(cx);
        let next = Theme::for_mode(ThemeMode::Light, WindowAppearance::Light, BaseColor::Slate);
        cx.update(|cx| cx.set_global(next));
        cx.run_until_parked();
        cx.update(|cx| {
            assert!(bezel_holds(cx, &next), "bezel kept the previous theme");
            assert_eq!(
                bezel::theme::current_appearance(),
                bezel::theme::Appearance::Light
            );
            let ely = cx.global::<ElyTheme>();
            assert!(!ely.is_dark());
            assert_eq!(ely.colors, next.colors.ely, "Ely is not on Sirio's palette");
        });
    }

    #[gpui::test]
    async fn translucency_reaches_ely_faded_and_bezel_opaque(cx: &mut TestAppContext) {
        boot(cx);
        let opaque = cx.update(|cx| *Theme::get(cx));
        let faded = opaque.with_translucency(true);
        cx.update(|cx| cx.set_global(faded));
        cx.run_until_parked();
        cx.update(|cx| {
            let ely = cx.global::<ElyTheme>();
            assert_eq!(ely.colors, faded.colors.ely);
            assert_eq!(
                ely.colors.overlay.a,
                opaque.colors.ely.overlay.a * faded.translucent_surface_opacity
            );
            assert!(bezel_holds(cx, &opaque), "bezel must never fade");
        });
    }
}
