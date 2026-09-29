//! The composer that closes the Conversation (spec §7.1): a growing field
//! and the three ways to send what is in it — a comment, an approval, a
//! request for changes.

use bezel::ui::input::{Shape, TextField};
use gpui::{Global, KeyBinding, actions};
use sirio_forge::{Action, ReviewVerdict};

use super::actions::action_button;
use super::*;

actions!(change_request_composer, [SendComment]);

/// The key context the composer's field claims; `bind_keys` scopes to it.
pub(crate) const KEY_CONTEXT: &str = "ChangeRequestComposer";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposerSend {
    Comment,
    Approve,
    RequestChanges,
}

struct KeysBound;

impl Global for KeysBound {}

/// Cmd/Ctrl+Enter sends the comment. Bound once, whichever tab asks first.
pub(crate) fn bind_keys(cx: &mut App) {
    if cx.try_global::<KeysBound>().is_some() {
        return;
    }
    cx.set_global(KeysBound);
    #[cfg(target_os = "macos")]
    let chord = "cmd-enter";
    #[cfg(not(target_os = "macos"))]
    let chord = "ctrl-enter";
    cx.bind_keys([KeyBinding::new(chord, SendComment, Some(KEY_CONTEXT))]);
}

/// The composer's field, ready to be kept in the tab's state.
pub(crate) fn new_field(cx: &mut Context<ChangeRequestTab>) -> Entity<TextField> {
    bind_keys(cx);
    cx.new(|cx| {
        TextField::new(cx)
            .with_shape(Shape::Grow { min: 3, max: 12 })
            .with_placeholder("Leave a comment…")
            .with_key_context(KEY_CONTEXT)
    })
}

impl ChangeRequestTab {
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
        let body = self.actions.composer.read(cx).content().to_string();
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

    pub(crate) fn render_composer(&self, theme: &Theme, entity: &Entity<Self>) -> Option<AnyElement> {
        let caps = self.header.value()?.capabilities;
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
        let on_key = entity.clone();
        Some(
            div()
                .id("change-request-composer")
                .debug_selector(|| "change-request-composer".into())
                .flex()
                .flex_col()
                .gap(px(8.0))
                .on_action(move |_: &SendComment, _window, cx| {
                    on_key.update(cx, |tab, cx| {
                        let _ = tab.send_composer(ComposerSend::Comment, cx);
                    })
                })
                .child(self.actions.composer.clone())
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(6.0))
                        .when(caps.can_request_changes, |row| {
                            row.child(action_button(
                                "change-request-send-request-changes",
                                "Request changes",
                                theme,
                                !busy && !blank,
                                send(ComposerSend::RequestChanges),
                            ))
                        })
                        .when(caps.can_approve, |row| {
                            row.child(action_button(
                                "change-request-send-approve",
                                "Approve",
                                theme,
                                !busy,
                                send(ComposerSend::Approve),
                            ))
                        })
                        .child(action_button(
                            "change-request-send-comment",
                            "Comment",
                            theme,
                            !busy && !blank,
                            send(ComposerSend::Comment),
                        )),
                )
                .into_any_element(),
        )
    }
}
