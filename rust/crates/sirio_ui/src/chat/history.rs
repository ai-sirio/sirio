//! The Chat History popover (F-CHAT-34/35): the worktree's past chats, each
//! with Open and Delete (which asks first), or the "No past chats" empty state.
//! A row's identity is its session's tab id, never its position.

use super::controls::popup_width;
use super::*;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    menus::{Hang, layer, panel_surface},
    theme::{ActiveTheme as _, ControlSize, Radius, TextSize},
    typography::EllipsisTooltip,
};

impl Chat {
    pub(super) fn render_history(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let ely = cx.theme();
        let (fg, fg_muted, hover) = (ely.colors.fg, ely.colors.fg_muted, ely.colors.hover);
        let (radius, text_size) = (ely.radius(Radius::Md), ely.text_size(TextSize::Sm));
        let rows: Vec<AnyElement> = if self.history_sessions.is_empty() {
            vec![
                div()
                    .id("chat-history-empty")
                    .debug_selector(|| "chat-history-empty".into())
                    .px_2()
                    .py_2p5()
                    .text_size(text_size)
                    .text_color(fg_muted)
                    .child("No past chats")
                    .into_any_element(),
            ]
        } else {
            self.history_sessions
                .iter()
                .enumerate()
                .map(|(index, session)| {
                    let tab_id = session.tab_id.clone();
                    let confirming = self.history_delete_confirm.as_deref() == Some(&tab_id);
                    let title = if session.title.is_empty() {
                        "Untitled chat".to_string()
                    } else {
                        session.title.clone()
                    };
                    let open_tab_id = tab_id.clone();
                    let controls = if confirming {
                        let confirm_id = tab_id.clone();
                        div()
                            .flex()
                            .flex_none()
                            .gap_0p5()
                            .child(
                                div()
                                    .debug_selector({
                                        let selector =
                                            format!("chat-history-confirm-delete-{tab_id}");
                                        move || selector.clone()
                                    })
                                    .child(
                                        Button::new(("chat-history-confirm", index), "Confirm")
                                            .variant(ButtonVariant::Danger)
                                            .size(ControlSize::Sm)
                                            .on_click(cx.listener(move |chat, _, _, cx| {
                                                chat.confirm_delete_chat_session(
                                                    confirm_id.clone(),
                                                    cx,
                                                );
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .debug_selector({
                                        let selector =
                                            format!("chat-history-cancel-delete-{tab_id}");
                                        move || selector.clone()
                                    })
                                    .child(
                                        Button::new(("chat-history-cancel", index), "Cancel")
                                            .variant(ButtonVariant::Ghost)
                                            .size(ControlSize::Sm)
                                            .on_click(cx.listener(move |chat, _, _, cx| {
                                                chat.cancel_delete_chat_session(cx);
                                            })),
                                    ),
                            )
                            .into_any_element()
                    } else {
                        let delete_tab_id = tab_id.clone();
                        div()
                            .flex_none()
                            .debug_selector({
                                let selector = format!("chat-history-delete-{tab_id}");
                                move || selector.clone()
                            })
                            .child(
                                Button::new(("chat-history-delete", index), "Delete")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ControlSize::Sm)
                                    .on_click(cx.listener(move |chat, _, _, cx| {
                                        chat.request_delete_chat_session(delete_tab_id.clone(), cx);
                                    })),
                            )
                            .into_any_element()
                    };
                    div()
                        .id(SharedString::from(format!("chat-history-row-{tab_id}")))
                        .debug_selector({
                            let selector = format!("chat-history-row-{tab_id}");
                            move || selector.clone()
                        })
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_1p5()
                        .pl_2()
                        .pr_0p5()
                        .py_0p5()
                        .rounded(radius)
                        .hover(move |style| style.bg(hover))
                        .child(
                            div()
                                .id(SharedString::from(format!("chat-history-open-{tab_id}")))
                                .debug_selector({
                                    let selector = format!("chat-history-open-{tab_id}");
                                    move || selector.clone()
                                })
                                .flex_1()
                                .min_w_0()
                                .cursor_pointer()
                                .text_size(text_size)
                                .text_color(fg)
                                .child(EllipsisTooltip::new(("chat-history-title", index), title))
                                .on_click(cx.listener(move |chat, _, _, cx| {
                                    chat.open_chat_history_session(open_tab_id.clone(), cx);
                                })),
                        )
                        .child(controls)
                        .into_any_element()
                })
                .collect()
        };
        layer(
            "chat-history-menu-menu",
            Hang::AboveEnd,
            div()
                .id("chat-history-menu")
                .debug_selector(|| "chat-history-menu".into())
                .key_context("ChatHistoryMenu")
                .track_focus(&self.history_focus)
                .on_action(cx.listener(Self::cancel))
                .w(popup_width(theme, 260.0))
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.history_open = false;
                    cx.notify();
                }))
                .child(
                    panel_surface("chat-history-surface", cx)
                        .p_1()
                        .max_h(px(320.0))
                        .overflow_y_scroll()
                        .children(rows),
                ),
            cx,
        )
    }
}
