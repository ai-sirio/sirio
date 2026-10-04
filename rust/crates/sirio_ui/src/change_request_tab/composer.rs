//! The composer that closes the Conversation (spec §7.1): a growing field
//! and the three ways to send what is in it — a comment, an approval, a
//! request for changes.

use ely_gpui_component::{
    buttons::ButtonVariant,
    forms::InputEvent,
    menus::{Menu, MenuItem, SplitButton},
};
use sirio_forge::{Action, ReviewVerdict};

use super::ely_ui::{ButtonState, new_input, text_button};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposerSend {
    Comment,
    Approve,
    RequestChanges,
}

impl ChangeRequestTab {
    /// Builds the composer the first time there is a window to build it with.
    pub(crate) fn ensure_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.actions.composer.is_some() {
            return;
        }
        let input = new_input(window, cx, "", Some((3, 12)), "Leave a comment…");
        cx.subscribe(&input, |tab: &mut Self, input, event: &InputEvent, cx| match event {
            InputEvent::Changed => {
                tab.actions.composer_blank = input.read(cx).text().trim().is_empty();
                cx.notify();
            }
            // Cmd/Ctrl+Enter: the same send as the *Comment* button.
            InputEvent::Submit => {
                let _ = tab.send_composer(ComposerSend::Comment, cx);
            }
            InputEvent::Focus | InputEvent::Blur => {}
        })
        .detach();
        self.actions.composer = Some(input);
    }

    /// One of the composer's three buttons. `Err` when nothing was sent.
    pub(crate) fn send_composer(
        &mut self,
        how: ComposerSend,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        // Before anything is touched: a second click while the first send is in
        // flight must not disturb what the first remembers having sent.
        if self.action_busy() {
            return Err("an action is already in flight".to_string());
        }
        let Some(composer) = self.actions.composer.clone() else {
            return Err("the composer is not built yet".to_string());
        };
        let body = composer.read(cx).text().to_string();
        self.actions.sent = Some(body.clone());
        let action = match how {
            ComposerSend::Comment => Action::Comment { body },
            ComposerSend::Approve => Action::Review {
                verdict: ReviewVerdict::Approve,
                body,
            },
            ComposerSend::RequestChanges => Action::Review {
                verdict: ReviewVerdict::RequestChanges,
                body,
            },
        };
        let sent = self.perform(action, cx);
        if sent.is_err() {
            self.actions.sent = None;
        }
        sent
    }

    pub(crate) fn render_composer(&self, _theme: &Theme, entity: &Entity<Self>) -> Option<AnyElement> {
        let caps = &self.header.value()?.capabilities;
        if !caps.can_comment {
            return None;
        }
        let busy = self.action_busy();
        let blank = self.actions.composer_blank;
        let send = |how: ComposerSend| {
            let entity = entity.clone();
            move |cx: &mut App| {
                entity.update(cx, |tab, cx| {
                    let _ = tab.send_composer(how, cx);
                })
            }
        };
        let comment = send(ComposerSend::Comment);
        let can_comment_now = !busy && !blank;
        let control: AnyElement = if caps.can_approve || caps.can_request_changes {
            let mut menu = Menu::new();
            if caps.can_approve {
                let run = send(ComposerSend::Approve);
                menu = menu.item(
                    MenuItem::new("Approve")
                        .disabled(busy)
                        .on_click(move |_, cx| run(cx)),
                );
            }
            if caps.can_request_changes {
                let run = send(ComposerSend::RequestChanges);
                menu = menu.item(
                    MenuItem::new("Request changes")
                        .disabled(busy || blank)
                        .on_click(move |_, cx| run(cx)),
                );
            }
            div()
                .id("change-request-send")
                .debug_selector(|| "change-request-send".into())
                .child(
                    SplitButton::new("change-request-send-split", "Comment", menu)
                        .variant(ButtonVariant::Primary)
                        .on_click(move |_, _, cx| {
                            if can_comment_now {
                                comment(cx)
                            }
                        }),
                )
                .into_any_element()
        } else {
            text_button(
                "change-request-send-comment",
                "Comment",
                None,
                ButtonState::enabled(can_comment_now).primary().loading(busy),
                move |_, cx| comment(cx),
            )
        };
        Some(
            div()
                .id("change-request-composer")
                .debug_selector(|| "change-request-composer".into())
                .flex()
                .flex_col()
                .gap(px(8.0))
                .children(self.actions.composer.clone().map(|input| {
                    ely_gpui_component::forms::Input::new(&input)
                }))
                .child(div().flex().justify_end().child(control))
                .into_any_element(),
        )
    }
}
