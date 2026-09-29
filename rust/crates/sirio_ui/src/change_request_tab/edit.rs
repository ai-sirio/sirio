//! Edit in place (spec §7.1): the title, the target branch and the
//! description as fields at the top of the Conversation.

use bezel::ui::input::{Shape, TextField, normalize};
use sirio_forge::Action;

use super::actions::{ActionState, EditFields, action_button};
use super::*;

/// A description or a comment: wraps, grows, then scrolls.
const BODY_SHAPE: Shape = Shape::Grow { min: 4, max: 16 };

fn field(
    cx: &mut Context<ChangeRequestTab>,
    text: &str,
    shape: Shape,
    placeholder: &'static str,
) -> Entity<TextField> {
    let text = text.to_string();
    cx.new(|cx| {
        let mut field = TextField::new(cx)
            .with_shape(shape)
            .with_placeholder(placeholder);
        field.set_content(text, cx);
        field
    })
}

fn caption(text: &'static str, theme: &Theme) -> gpui::Div {
    div()
        .text_size(theme.typography.footnote)
        .text_color(theme.text_muted)
        .child(text)
}

impl ChangeRequestTab {
    /// *Edit*: the three fields, filled with what the forge holds now.
    pub(crate) fn start_edit(&mut self, cx: &mut Context<Self>) {
        if self.action_busy() || self.actions.edit.is_some() {
            return;
        }
        let Some(header) = self.header.value() else {
            return;
        };
        let (title, target, body) = (
            header.summary.title.clone(),
            header.summary.target_branch.clone(),
            header.body.clone(),
        );
        self.actions.edit = Some(EditFields {
            title: field(cx, &title, Shape::Line, "Title"),
            target: field(cx, &target, Shape::Line, "Target branch"),
            body: field(cx, &body, BODY_SHAPE, "Description"),
        });
        cx.notify();
    }

    pub(crate) fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        self.actions.edit = None;
        if self.actions.state.kind() == "edit" {
            self.actions.state = ActionState::Idle;
        }
        cx.notify();
    }

    /// *Save*: only what changed is sent. A description is compared as the
    /// field holds it — line endings folded — so saving an untouched CRLF
    /// description from GitHub is not an edit.
    pub(crate) fn save_edit(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let (title, target, body) = {
            let fields = self.actions.edit.as_ref().ok_or("nothing is being edited")?;
            (
                fields.title.read(cx).content().to_string(),
                fields.target.read(cx).content().to_string(),
                fields.body.read(cx).content().to_string(),
            )
        };
        let header = self.header.value().ok_or("the change request is not loaded")?;
        let title = (title != header.summary.title).then_some(title);
        let target_branch = (target != header.summary.target_branch).then_some(target);
        let body = (body != normalize(&header.body, BODY_SHAPE)).then_some(body);
        if title.is_none() && target_branch.is_none() && body.is_none() {
            self.actions.edit = None;
            cx.notify();
            return Ok(());
        }
        self.perform(
            Action::Edit {
                title,
                body,
                target_branch,
            },
            cx,
        )
    }

    /// The card at the top of the Conversation while the change request is
    /// being edited.
    pub(crate) fn render_edit_card(&self, theme: &Theme, entity: &Entity<Self>) -> Option<AnyElement> {
        let fields = self.actions.edit.as_ref()?;
        let enabled = !self.action_busy();
        let (save, cancel) = (entity.clone(), entity.clone());
        Some(
            div()
                .id("change-request-edit-card")
                .debug_selector(|| "change-request-edit-card".into())
                .flex()
                .flex_col()
                .gap(px(6.0))
                .p(px(12.0))
                .rounded(theme.radii.control)
                .border_1()
                .border_color(theme.border)
                .bg(theme.surface)
                .child(caption("Title", theme))
                .child(fields.title.clone())
                .child(caption("Target branch", theme))
                .child(fields.target.clone())
                .child(caption("Description", theme))
                .child(fields.body.clone())
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(6.0))
                        .child(action_button(
                            "change-request-edit-cancel",
                            "Cancel",
                            theme,
                            enabled,
                            move |cx| cancel.update(cx, |tab, cx| tab.cancel_edit(cx)),
                        ))
                        .child(action_button(
                            "change-request-edit-save",
                            "Save",
                            theme,
                            enabled,
                            move |cx| {
                                save.update(cx, |tab, cx| {
                                    let _ = tab.save_edit(cx);
                                })
                            },
                        )),
                )
                .into_any_element(),
        )
    }
}
