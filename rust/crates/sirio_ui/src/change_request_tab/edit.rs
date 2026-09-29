//! Edit in place (spec §7.1): the title, the target branch and the
//! description as fields at the top of the Conversation, and one of the
//! viewer's own comments turned into a field where it stands.

use bezel::ui::input::{Shape, TextField, normalize};
use sirio_forge::{Action, CommentRef};

use super::actions::{ActionState, CommentEdit, EditFields, action_button, indexed_button};
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

    /// The pencil on one of the viewer's own comments.
    pub(crate) fn start_comment_edit(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.action_busy() {
            return;
        }
        let Some(header) = self.header.value() else {
            return;
        };
        let (comment, body) = match header.timeline.get(index) {
            Some(
                TimelineItem::Comment {
                    edit: Some(edit),
                    body,
                    ..
                }
                | TimelineItem::Review {
                    edit: Some(edit),
                    body,
                    ..
                },
            ) => (edit.clone(), body.clone()),
            _ => return,
        };
        self.actions.comment_edit = Some(CommentEdit {
            comment,
            field: field(cx, &body, BODY_SHAPE, "Comment"),
        });
        cx.notify();
    }

    pub(crate) fn cancel_comment_edit(&mut self, cx: &mut Context<Self>) {
        self.actions.comment_edit = None;
        if self.actions.state.kind() == "edit-comment" {
            self.actions.state = ActionState::Idle;
        }
        cx.notify();
    }

    pub(crate) fn save_comment_edit(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let (comment, body) = {
            let edit = self
                .actions
                .comment_edit
                .as_ref()
                .ok_or("no comment is being edited")?;
            (
                edit.comment.clone(),
                edit.field.read(cx).content().to_string(),
            )
        };
        // Like the header edit's untouched path: words that match what the
        // forge holds — compared as the field holds them — are not a write.
        let header = self.header.value().ok_or("the change request is not loaded")?;
        let unchanged = header.timeline.iter().any(|item| match item {
            TimelineItem::Comment {
                edit: Some(edit),
                body: current,
                ..
            }
            | TimelineItem::Review {
                edit: Some(edit),
                body: current,
                ..
            } => edit.id == comment.id && body == normalize(current, BODY_SHAPE),
            _ => false,
        });
        if unchanged {
            self.actions.comment_edit = None;
            cx.notify();
            return Ok(());
        }
        self.perform(Action::EditComment { comment, body }, cx)
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

    /// The open editor of this timeline entry, when the viewer is editing it.
    pub(crate) fn editor_for(
        &self,
        own: Option<&CommentRef>,
        theme: &Theme,
        entity: &Entity<Self>,
    ) -> Option<AnyElement> {
        let open = self.actions.comment_edit.as_ref()?;
        (own?.id == open.comment.id).then(|| self.render_comment_editor(open, theme, entity))
    }

    /// The *Edit* button on an entry the viewer may edit and is not editing.
    pub(crate) fn edit_pencil(
        &self,
        index: usize,
        own: Option<&CommentRef>,
        theme: &Theme,
        entity: &Entity<Self>,
    ) -> Option<AnyElement> {
        let own = own?;
        if self
            .actions
            .comment_edit
            .as_ref()
            .is_some_and(|open| open.comment.id == own.id)
        {
            return None;
        }
        let entity = entity.clone();
        Some(
            indexed_button(
                "change-request-comment-edit",
                index,
                "Edit",
                theme,
                !self.action_busy(),
                move |cx| entity.update(cx, |tab, cx| tab.start_comment_edit(index, cx)),
            )
            .into_any_element(),
        )
    }

    /// One of the viewer's own comments, open: its field with *Save* and
    /// *Cancel*, in the place of its rendered text.
    pub(crate) fn render_comment_editor(
        &self,
        edit: &CommentEdit,
        theme: &Theme,
        entity: &Entity<Self>,
    ) -> AnyElement {
        let enabled = !self.action_busy();
        let (save, cancel) = (entity.clone(), entity.clone());
        div()
            .id("change-request-comment-editor")
            .debug_selector(|| "change-request-comment-editor".into())
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(edit.field.clone())
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(6.0))
                    .child(action_button(
                        "change-request-comment-edit-cancel",
                        "Cancel",
                        theme,
                        enabled,
                        move |cx| cancel.update(cx, |tab, cx| tab.cancel_comment_edit(cx)),
                    ))
                    .child(action_button(
                        "change-request-comment-edit-save",
                        "Save",
                        theme,
                        enabled,
                        move |cx| {
                            save.update(cx, |tab, cx| {
                                let _ = tab.save_comment_edit(cx);
                            })
                        },
                    )),
            )
            .into_any_element()
    }
}
