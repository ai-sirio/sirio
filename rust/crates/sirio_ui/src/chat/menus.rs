//! The chat's menus: the secondary-click menus on the composer field and on
//! the transcript, and the composer's "more" menu. Rows are Ely's; whether a
//! menu is open, where it hangs and what Escape does stay with `Chat`.

use super::controls::{OverflowRow, PickerRow, popup_width};
use super::*;
use ely_gpui_component::menus::{Hang, Menu, MenuItem, MenuPanel, layer};

impl Chat {
    pub(super) fn render_composer_context_menu(
        &self,
        _bezel_theme: &bezel::theme::Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *Theme::get(cx);
        let position = *self
            .composer_context_menu
            .get()
            .expect("mounted composer context menu");
        // A menu that is closing has no exit to play: it is gone at once, and
        // takes no more clicks.
        if self.composer_context_menu.closing_since().is_some() {
            return div().into_any_element();
        }
        let entity = cx.entity();
        let menu = ComposerContextItem::ALL
            .into_iter()
            .fold(Menu::new(), |menu, item| {
                let entity = entity.clone();
                menu.item(
                    MenuItem::new(item.label())
                        .selectors(format!("composer-context-{}", item.selector()), None)
                        .on_click(move |window, cx| {
                            entity.update(cx, |chat, cx| {
                                chat.composer_context_action(item, window, cx)
                            });
                        }),
                )
            });
        layer(
            "composer-context-menu-layer",
            Hang::At(position),
            div()
                .id("composer-context-menu")
                .debug_selector(|| "composer-context-menu".into())
                .w(popup_width(&theme, 160.0))
                .on_mouse_down_out(cx.listener(|chat, _, _, cx| {
                    chat.close_composer_context_menu(cx);
                }))
                .child(MenuPanel::new("composer-context-rows", menu)),
            cx,
        )
    }

    pub(super) fn render_transcript_context_menu(
        &self,
        _bezel_theme: &bezel::theme::Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *Theme::get(cx);
        let position = *self
            .transcript_context_menu
            .get()
            .expect("mounted transcript context menu");
        if self.transcript_context_menu.closing_since().is_some() {
            return div().into_any_element();
        }
        let entity = cx.entity();
        let menu = TranscriptContextItem::ALL
            .into_iter()
            .fold(Menu::new(), |menu, item| {
                let entity = entity.clone();
                menu.item(
                    MenuItem::new(item.label())
                        .selectors(format!("transcript-context-{}", item.selector()), None)
                        .on_click(move |window, cx| {
                            entity.update(cx, |chat, cx| {
                                chat.transcript_context_action(item, window, cx)
                            });
                        }),
                )
            });
        layer(
            "transcript-context-menu-layer",
            Hang::At(position),
            div()
                .id("transcript-context-menu")
                .debug_selector(|| "transcript-context-menu".into())
                .w(popup_width(&theme, 160.0))
                .on_mouse_down_out(cx.listener(|chat, _, _, cx| {
                    chat.close_transcript_context_menu(cx);
                }))
                .child(MenuPanel::new("transcript-context-rows", menu)),
            cx,
        )
    }

    /// The composer's "more" menu (F-CHAT-14): Follow Edited Files, New
    /// Conversation and Chat History. Follow is a toggle and leaves the menu
    /// open; the other two close it.
    pub(super) fn render_chat_menu(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let rows = self.picker_rows(cx);
        let menu = rows.iter().fold(Menu::new(), |menu, row| {
            let PickerRow::Overflow(which) = row else {
                return menu;
            };
            let (label, selector) = match which {
                OverflowRow::Follow if self.following_edited_files => {
                    ("Stop Following", "overflow-follow")
                }
                OverflowRow::Follow => ("Follow Edited Files", "overflow-follow"),
                OverflowRow::NewConversation => ("New Conversation", "overflow-new-conversation"),
                OverflowRow::History => ("Chat History", "overflow-chat-history"),
            };
            menu.item(
                MenuItem::new(label)
                    .selectors(selector, None)
                    .on_click(self.picker_click(row.clone(), cx)),
            )
        });
        layer(
            "composer-overflow-menu-menu",
            Hang::AboveEnd,
            div()
                .id("composer-overflow-menu")
                .debug_selector(|| "composer-overflow-menu".into())
                .key_context("ChatOverflowMenu")
                .track_focus(&self.overflow_focus)
                .on_action(cx.listener(Self::cancel))
                .on_action(cx.listener(Self::picker_previous))
                .on_action(cx.listener(Self::picker_next))
                .on_action(cx.listener(Self::picker_choose))
                .w(popup_width(theme, 200.0))
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.overflow_open = false;
                    cx.notify();
                }))
                .child(
                    MenuPanel::new("overflow-rows", menu)
                        .current(self.marked_picker_row(rows.len())),
                ),
            cx,
        )
    }
}
