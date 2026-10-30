//! The chat's menus: the secondary-click menus on the composer field and on
//! the transcript, and the composer's "more" menu. Rows are Ely's; whether a
//! menu is open, where it hangs and what Escape does stay with `Chat`.

use super::controls::{OverflowRow, PickerRow, popup_width};
use super::*;
use ely_gpui_component::menus::{Hang, Menu, MenuItem, MenuPanel, layer};

impl Chat {
    pub(super) fn render_composer_context_menu(
        &self,
        _bezel_theme: &bezel_theme::Theme,
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
        _bezel_theme: &bezel_theme::Theme,
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

    /// The rows of the "more" menu as Ely's menu model, so that what is
    /// disabled is the model's to say: it steps over those rows and will not
    /// run them.
    pub(super) fn overflow_menu(&self, cx: &Context<Self>) -> Menu {
        self.overflow_rows()
            .into_iter()
            .fold(Menu::new(), |menu, which| {
                let row = PickerRow::Overflow(which);
                let item = match which {
                    OverflowRow::Fast => {
                        let Some(fast) = self.fast_state() else {
                            return menu;
                        };
                        let item = MenuItem::check("Fast mode", fast.enabled)
                            .selectors("overflow-fast", None);
                        match &fast.blocked_by {
                            Some(reason) => item.disabled(true).tooltip(reason.clone()),
                            None => item,
                        }
                    }
                    OverflowRow::Thinking => {
                        let chosen = self.thinking_state().and_then(|state| state.chosen);
                        let label = match chosen {
                            Some(chosen) => {
                                format!("Thinking: {}", thinking_display_name(&chosen))
                            }
                            None => "Thinking".to_string(),
                        };
                        MenuItem::new(label).selectors("overflow-thinking", None)
                    }
                    OverflowRow::SessionPanel => {
                        let label = if self.session_panel_open() {
                            "Hide session panel"
                        } else {
                            "Show session panel"
                        };
                        MenuItem::new(label).selectors("overflow-session-panel", None)
                    }
                    OverflowRow::Follow if self.following_edited_files => {
                        MenuItem::new("Stop Following").selectors("overflow-follow", None)
                    }
                    OverflowRow::Follow => {
                        MenuItem::new("Follow Edited Files").selectors("overflow-follow", None)
                    }
                    OverflowRow::NewConversation => MenuItem::new("New Conversation")
                        .selectors("overflow-new-conversation", None),
                    OverflowRow::History => {
                        MenuItem::new("Chat History").selectors("overflow-chat-history", None)
                    }
                };
                menu.item(item.on_click(self.picker_click(row, cx)))
            })
    }

    /// The composer's "more" menu (F-CHAT-14): Follow Edited Files, New
    /// Conversation and Chat History, and, while the row is Compact, the
    /// Fast, Thinking and session panel rows above them. Follow and Fast are
    /// toggles and leave the menu open; the others close it.
    pub(super) fn render_chat_menu(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let rows = self.picker_rows(cx);
        let menu = self.overflow_menu(cx);
        layer(
            "composer-overflow-menu-menu",
            Hang::Above,
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
