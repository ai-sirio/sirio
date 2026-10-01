//! Agent identity comes from the host, independently of the model catalogue.
use gpui::{AnyElement, ElementId, IntoElement, div, prelude::*, px, rgb, svg};
use sirio_theme::{Theme, ThemeMode};

pub(super) const ASSETS: &[(&str, &[u8])] = &[
    (
        "sirio-chat/claude-code.svg",
        include_bytes!("../../../../assets/icons/chat/claude-code.svg"),
    ),
    (
        "sirio-chat/codex.svg",
        include_bytes!("../../../../assets/icons/lobehub/codex.svg"),
    ),
    (
        "sirio-chat/opencode.svg",
        include_bytes!("../../../../assets/icons/lobehub/opencode.svg"),
    ),
    (
        "sirio-chat/pi.svg",
        include_bytes!("../../../../assets/icons/lobehub/pi.svg"),
    ),
    (
        "sirio-chat/omp.svg",
        include_bytes!("../../../../assets/icons/agent-omp.svg"),
    ),
];
pub(super) fn asset(path: &str) -> Option<&'static [u8]> {
    ASSETS
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, bytes)| *bytes)
}
fn mark_path(id: Option<&str>, name: &str) -> Option<&'static str> {
    let known_name = match name.to_ascii_lowercase().as_str() {
        "claude code" | "claude" => Some("claude"),
        "codex" => Some("codex"),
        "opencode" | "open code" => Some("opencode"),
        "pi" => Some("pi"),
        "oh my pi" | "oh-my-pi" | "omp" => Some("omp"),
        _ => None,
    };
    match id
        .map(|id| id.strip_suffix("-acp").unwrap_or(id))
        .or(known_name)
    {
        Some("claude" | "claude-code") => Some("sirio-chat/claude-code.svg"),
        Some("codex") => Some("sirio-chat/codex.svg"),
        Some("opencode") => Some("sirio-chat/opencode.svg"),
        Some("pi") => Some("sirio-chat/pi.svg"),
        Some("omp") => Some("sirio-chat/omp.svg"),
        _ => None,
    }
}
pub(crate) fn render_mark(
    id: ElementId,
    agent_id: Option<&str>,
    name: &str,
    theme: &Theme,
) -> AnyElement {
    let dark = Theme::for_mode(
        ThemeMode::Dark,
        gpui::WindowAppearance::Dark,
        theme.base_color,
    );
    let plate = div()
        .id(id)
        .flex_none()
        .size(px(26.0))
        .rounded(theme.radii.control)
        .flex()
        .items_center()
        .justify_center()
        .bg(dark.colors.surface_raised)
        .text_color(rgb(0xffffff));
    match mark_path(agent_id, name) {
        Some(path) => plate
            .child(svg().path(path).size(px(18.0)).text_color(rgb(0xffffff)))
            .into_any_element(),
        None => plate
            .child(if name == "Unknown agent" {
                "?".into()
            } else {
                name.chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string()
            })
            .into_any_element(),
    }
}
