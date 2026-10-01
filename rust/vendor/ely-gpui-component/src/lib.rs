//! Pinned Ely chat and agent components adapted to Sirio's GPUI packages.
pub mod agent;
mod assets;
pub mod buttons;
pub mod chat;
mod compat;
pub mod data_display;
pub mod feedback;
pub mod forms;
pub mod layout;
pub mod menus;
pub mod motion;
pub mod overlays;
pub mod primitives;
pub mod theme;
pub mod typography;

pub use assets::Assets;

/// Initialize chat components without default fonts or global shortcuts.
pub fn init_chat(cx: &mut gpui::App) {
    if cx.has_global::<theme::Theme>() {
        return;
    }
    theme::Theme::init(cx);
    forms::bind_keys(cx);
}
