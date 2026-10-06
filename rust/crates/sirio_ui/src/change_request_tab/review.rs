//! The drafted review (spec §7.3, slice B3c): the strip that shows while the
//! viewer has one on the forge, the dialog that submits it with a verdict,
//! and the dialog that asks before it is discarded.

use ely_gpui_component::buttons::{Button, ButtonVariant};
use ely_gpui_component::forms::{Input, TextInput};
use ely_gpui_component::overlays::Dialog;
use ely_gpui_component::primitives::Severity;
use sirio_forge::{Action, ReviewVerdict};

use super::compose::WriteTarget;
use crate::ely_ui::{ButtonState, message, new_input, text_button};
use super::*;

#[derive(Default)]
pub(crate) struct ReviewUi {
    /// The submit dialog's body, while it is open.
    pub(crate) submit: Option<Entity<TextInput>>,
    pub(crate) discard: bool,
}

/// A button of the Discard dialog, findable by its id: the destructive
/// confirm, with the danger look of `changes.rs`'s
/// `dialog_button(…, ButtonVariant::Danger, …)`.
fn danger_button(
    id: &'static str,
    label: &'static str,
    enabled: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .flex_none()
        .child(
            Button::new(id, label)
                .variant(ButtonVariant::Danger)
                .disabled(!enabled)
                .on_click(move |_, window, cx| on_click(window, cx)),
        )
        .into_any_element()
}

impl ChangeRequestTab {
    fn draft_comments(&self) -> Option<u32> {
        self.header.value().and_then(|header| header.draft.as_ref()).map(|draft| draft.comments)
    }

    /// The reason the last review write failed, while it is the review's.
    pub(crate) fn review_error(&self) -> Option<String> {
        let target = WriteTarget::Review;
        self.write_refusal
            .as_ref()
            .filter(|(owner, _)| *owner == target)
            .map(|(_, message)| message.clone())
            .or_else(|| match &self.actions.state {
                actions::ActionState::Failed { message, .. } if self.write_target.as_ref() == Some(&target) => Some(message.clone()),
                _ => None,
            })
    }

    pub(crate) fn open_review_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
        self.draft_comments().ok_or("There is no review in progress.")?;
        if self.review.submit.is_none() {
            self.review.submit = Some(new_input(window, cx, "", Some((3, 10)), "Leave a summary…"));
        }
        self.review.discard = false;
        cx.notify();
        Ok(())
    }

    pub(crate) fn review_submit_set_text(&mut self, text: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let input = self.review.submit.clone().ok_or("The submit dialog is not open.")?;
        input.update(cx, |input, cx| input.set_text(text, cx));
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn review_submit_text(&self, cx: &App) -> Option<String> {
        self.review.submit.as_ref().map(|input| input.read(cx).text().to_string())
    }

    pub(crate) fn submit_review(&mut self, verdict: ReviewVerdict, cx: &mut Context<Self>) -> Result<(), String> {
        let input = self.review.submit.clone().ok_or("The submit dialog is not open.")?;
        let body = input.read(cx).text().to_string();
        self.start_write(WriteTarget::Review, Action::ReviewSubmit { verdict, body }, cx)
    }

    pub(crate) fn close_review_submit(&mut self, cx: &mut Context<Self>) {
        self.review.submit = None;
        self.forget_review_error();
        cx.notify();
    }

    pub(crate) fn open_review_discard(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        self.draft_comments().ok_or("There is no review in progress.")?;
        self.review.submit = None;
        self.review.discard = true;
        cx.notify();
        Ok(())
    }

    pub(crate) fn close_review_discard(&mut self, cx: &mut Context<Self>) {
        self.review.discard = false;
        cx.notify();
    }

    /// The dialog closes on confirm (B3c revision (i)): a failure shows
    /// under the strip, since a discard has no text to keep.
    pub(crate) fn confirm_review_discard(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        self.review.discard = false;
        self.start_write(WriteTarget::Review, Action::ReviewDiscard, cx)
    }

    fn forget_review_error(&mut self) {
        if self.write_refusal.as_ref().is_some_and(|(owner, _)| *owner == WriteTarget::Review) {
            self.write_refusal = None;
        }
        if self.write_target == Some(WriteTarget::Review) && !self.action_busy() {
            self.actions.state = actions::ActionState::Idle;
        }
    }

    pub(crate) fn render_review_strip(&self, theme: &Theme, entity: &Entity<Self>) -> Option<AnyElement> {
        let comments = self.draft_comments()?;
        let busy = self.action_busy();
        let (submit, discard) = (entity.clone(), entity.clone());
        let words = format!("Review in progress · {comments} {}", if comments == 1 { "comment" } else { "comments" });
        let error = (self.review.submit.is_none() && !self.review.discard).then(|| self.review_error()).flatten();
        Some(
            div()
                .id("change-request-review-strip")
                .debug_selector(|| "change-request-review-strip".to_owned())
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .px(px(16.0))
                .py(px(8.0))
                .border_b_1()
                .border_color(theme.ely.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().flex_1().min_w_0().child(message(Severity::Info, words, theme)))
                        .child(text_button(
                            "change-request-review-discard",
                            "Discard",
                            None,
                            ButtonState::enabled(!busy),
                            move |_, cx| {
                                discard.update(cx, |tab, cx| {
                                    let _ = tab.open_review_discard(cx);
                                })
                            },
                        ))
                        .child(text_button(
                            "change-request-review-submit",
                            "Submit review",
                            None,
                            ButtonState::enabled(!busy).primary(),
                            move |window, cx| {
                                submit.update(cx, |tab, cx| {
                                    let _ = tab.open_review_submit(window, cx);
                                })
                            },
                        )),
                )
                .children(error.map(|error| message(Severity::Danger, error, theme)))
                .into_any_element(),
        )
    }

    pub(crate) fn render_review_dialog(&self, theme: &Theme, entity: &Entity<Self>) -> Option<AnyElement> {
        let comments = self.draft_comments()?;
        let caps = &self.header.value()?.capabilities;
        let busy = self.action_busy();
        let pending = format!("{comments} pending {}", if comments == 1 { "comment" } else { "comments" });
        if let Some(input) = &self.review.submit {
            let close = entity.clone();
            let mut dialog = Dialog::new("change-request-review-submit-dialog", "Submit review", move |_, cx| {
                close.update(cx, |tab, cx| tab.close_review_submit(cx))
            })
            .detail(pending)
            .child(Input::new(input))
            .children(self.review_error().map(|error| message(Severity::Danger, error, theme)))
            .action(|close| {
                text_button("change-request-review-submit-cancel", "Cancel", None, ButtonState::IDLE, move |window, cx| {
                    close(window, cx)
                })
            });
            let verdicts: [(&'static str, &'static str, ReviewVerdict, bool); 3] = [
                ("change-request-review-submit-comment", "Comment", ReviewVerdict::Comment, caps.can_comment),
                ("change-request-review-submit-approve", "Approve", ReviewVerdict::Approve, caps.can_approve),
                ("change-request-review-submit-request-changes", "Request changes", ReviewVerdict::RequestChanges, caps.can_request_changes),
            ];
            for (id, label, verdict, _) in verdicts.into_iter().filter(|(.., allowed)| *allowed) {
                let send = entity.clone();
                let primary = verdict == ReviewVerdict::Comment;
                dialog = dialog.action(move |_| {
                    let state = ButtonState::enabled(!busy).loading(busy);
                    text_button(id, label, None, if primary { state.primary() } else { state }, move |_, cx| {
                        send.update(cx, |tab, cx| {
                            let _ = tab.submit_review(verdict, cx);
                        })
                    })
                });
            }
            return Some(div().debug_selector(|| "change-request-review-submit-dialog".to_owned()).child(dialog).into_any_element());
        }
        if self.review.discard {
            let (close, confirm) = (entity.clone(), entity.clone());
            let dialog = Dialog::new("change-request-review-discard-dialog", "Discard your review?", move |_, cx| {
                close.update(cx, |tab, cx| tab.close_review_discard(cx))
            })
            .child(selectable_text(format!("This deletes {pending}. They cannot be recovered.")))
            .action(|close| {
                text_button("change-request-review-discard-cancel", "Cancel", None, ButtonState::IDLE, move |window, cx| {
                    close(window, cx)
                })
            })
            .action(move |close| {
                danger_button("change-request-review-discard-confirm", "Discard", !busy, move |window, cx| {
                    // Confirm first: closing runs `close_review_discard`.
                    confirm.update(cx, |tab, cx| {
                        let _ = tab.confirm_review_discard(cx);
                    });
                    close(window, cx);
                })
            });
            return Some(div().debug_selector(|| "change-request-review-discard-dialog".to_owned()).child(dialog).into_any_element());
        }
        None
    }
}
