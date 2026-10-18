//! Pinned Ely chat and agent components adapted to Sirio's GPUI packages.
pub mod agent;
mod assets;
pub mod buttons;
pub mod chat;
mod compat;
pub mod data_display;
pub mod editor;
mod expansion;
pub mod feedback;
pub mod forms;
pub mod git;
pub mod layout;
pub mod menus;
pub mod motion;
pub mod navigation;
pub mod overlays;
pub mod primitives;
pub mod shell;
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

/// Initialize the code editor: the chat's setup, then the editor's keys.
/// Idempotent, and separate from `init_chat`'s guard, so a host that ran
/// `init_chat` first still gets the editor's keys.
pub fn init_editor(cx: &mut gpui::App) {
    if cx.has_global::<EditorKeysBound>() {
        return;
    }
    init_chat(cx);
    editor::bind_keys(cx);
    cx.set_global(EditorKeysBound);
}

/// Marks that `init_editor` has bound the editor's keys.
struct EditorKeysBound;

impl gpui::Global for EditorKeysBound {}
