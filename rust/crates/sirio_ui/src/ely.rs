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

pub(crate) fn init(cx: &mut App) {
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
    ElyTheme::set_mode_now(
        match theme.appearance {
            Appearance::Light => Mode::Light,
            Appearance::Dark => Mode::Dark,
        },
        cx,
    );
    ElyTheme::update(cx, |ely| {
        let c = theme.colors;
        let p = &mut ely.colors;
        p.bg = c.surface.into();
        p.surface = c.surface_raised.into();
        p.sunken = c.input_bg.into();
        p.overlay = c.surface_raised.into();
        p.hover = c.element_hover.into();
        p.active = c.element_active.into();
        p.border = c.border.into();
        p.border_strong = c.border_opaque.into();
        p.fg = c.text.into();
        p.fg_muted = c.text_muted.into();
        p.fg_subtle = c.text_faint.into();
        p.fg_disabled = c.text_faint.into();
        p.accent = c.text.into();
        p.accent_hover = c.text_muted.into();
        p.on_accent = c.surface.into();
        p.focus = c.text.into();
        p.link = c.file_link.into();
        p.selection = c.selection.into();
        p.success = c.success.into();
        p.warning = c.warning.into();
        p.danger = c.danger.into();
        p.success_subtle = p.success.opacity(0.12);
        p.warning_subtle = p.warning.opacity(0.12);
        p.danger_subtle = p.danger.opacity(0.12);
        p.tooltip_bg = c.surface_raised.into();
        p.tooltip_fg = c.text.into();
        p.backdrop = c.overlay.into();
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
