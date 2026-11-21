//! The right-click menu of a forge body (a description, a comment, a review thread).
//!
//! Sirio's own popup with exactly the Markdown rows: Copy, Select all, and over a link Open
//! link and Copy link address. The tab owns it, so a body in a diff card asks the tab for it.

use gpui::{
    AnyElement, App, Entity, IntoElement, Pixels, Point, Window, anchored, deferred, div,
    prelude::*, px,
};
use sirio_theme::Theme;

use super::ChangeRequestTab;
use crate::markdown::{BodyActions, LinkHandler, MarkdownMenuItem, MenuHandler, MenuPress};

/// What a right press on a body offered, kept while the menu is open.
#[derive(Clone)]
pub(super) struct BodyMenu {
    at: Point<Pixels>,
    items: Vec<MarkdownMenuItem>,
    /// The text selected when the press fell: a press on a row ends the selection before
    /// the row acts.
    selected: Option<String>,
    actions: BodyActions,
    links: LinkHandler,
}

impl BodyMenu {
    fn new(press: MenuPress, links: LinkHandler) -> Self {
        Self {
            at: press.position,
            items: press.items(),
            selected: press.selected,
            actions: press.actions,
            links,
        }
    }

    fn perform(&self, item: &MarkdownMenuItem, window: &mut Window, cx: &mut App) {
        match item {
            MarkdownMenuItem::Copy { enabled } => {
                if let (true, Some(text)) = (*enabled, &self.selected) {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.clone()));
                }
            }
            other => crate::markdown::perform(other, &self.actions, &self.links, window, cx),
        }
    }

    /// The popup, at the press.
    pub(super) fn render(&self, theme: &Theme, tab: &Entity<ChangeRequestTab>) -> AnyElement {
        let dismiss = tab.clone();
        let mut menu = div()
            .id("change-request-menu")
            .debug_selector(|| "change-request-menu".to_owned())
            // The pointer over the menu is the menu's, not the text's under it.
            .occlude()
            .w(px(200.0))
            .p(px(6.0))
            .rounded(theme.radii.user_pill)
            .border_1()
            .border_color(theme.ely.border)
            .bg(theme.menu_surface())
            .shadow_lg()
            .on_mouse_down_out(move |_, _, cx| {
                dismiss.update(cx, |tab, cx| tab.close_body_menu(cx));
            });
        for (index, item) in self.items.iter().cloned().enumerate() {
            let selector = format!("change-request-menu-item-{index}");
            let debug = selector.clone();
            let enabled = item.enabled();
            let (tab, menu_state) = (tab.clone(), self.clone());
            let label = item.label();
            menu = menu.child(
                div()
                    .id(selector)
                    .debug_selector(move || debug.clone())
                    .w_full()
                    .min_h(px(29.0))
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(theme.radii.control)
                    .flex()
                    .items_center()
                    .text_size(theme.typography.footnote)
                    .when(!enabled, |this| {
                        this.text_color(theme.ely.fg_subtle).cursor_not_allowed()
                    })
                    .when(enabled, |this| {
                        this.text_color(theme.ely.fg)
                            .hover(|style| style.bg(theme.ely.hover))
                            .on_click(move |_, window, cx| {
                                tab.update(cx, |tab, cx| tab.close_body_menu(cx));
                                menu_state.perform(&item, window, cx);
                            })
                    })
                    .child(label),
            );
        }
        deferred(anchored().position(self.at).snap_to_window().child(menu))
            .priority(1)
            .into_any_element()
    }
}

/// The handler a body gets for its right press: the tab shows the menu.
pub(super) fn on_press(
    tab: gpui::WeakEntity<ChangeRequestTab>,
    links: LinkHandler,
) -> MenuHandler {
    std::rc::Rc::new(move |press, _window, cx| {
        let menu = BodyMenu::new(press, links.clone());
        let _ = tab.update(cx, |tab, cx| tab.show_body_menu(menu, cx));
    })
}
