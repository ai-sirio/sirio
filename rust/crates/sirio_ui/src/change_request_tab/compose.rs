//! The one new line comment being written in a change request's diff
//! (spec §7.2, B3b): its view under the line, and the tab's side of it.

use super::suggestion;
use super::*;

use ely_gpui_component::forms::{Input, InputEvent, TextInput};
use gpui::WeakEntity;
use sirio_forge::{LineAnchor, ReviewTarget, Revisions, Side};

use crate::diff_annotations::CommentAnchor;
use crate::ely_ui::new_input;

pub(crate) const COMPOSER_KEY: u64 = 0x636f_6d70_6f73_6572; // "composer"

/// Which open field a write in flight came from, so its failure shows there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WriteTarget {
    Composer,
    Reply(String),
    Resolve(String),
    EditComment(String),
    Review,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WriteStatus {
    pub busy: bool,
    pub error: Option<String>,
    pub in_review: bool,
}

pub(crate) struct LineComposer {
    pub(crate) anchor: CommentAnchor,
    pub(crate) revisions: Revisions,
    pub(crate) view: Entity<ComposerView>,
    suggested: Option<String>,
}

pub(crate) struct ComposerView {
    label: String,
    new_side: bool,
    field: FieldState,
    write: WriteStatus,
    pub(crate) revision: u64,
    owner: WeakEntity<ChangeRequestTab>,
}

/// An Ely text field that can be created by render, when a window exists,
/// while retaining text set before that first render.
pub(super) struct FieldState {
    input: Option<Entity<TextInput>>,
    pending_text: String,
}

impl FieldState {
    pub(super) fn new(pending_text: impl Into<String>) -> Self {
        Self {
            input: None,
            pending_text: pending_text.into(),
        }
    }

    /// Returns the input and whether this call created it.
    pub(super) fn ensure<T: 'static>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<T>,
        rows: Option<(usize, usize)>,
        placeholder: &'static str,
    ) -> (Entity<TextInput>, bool) {
        if let Some(input) = &self.input {
            return (input.clone(), false);
        }
        let input = new_input(window, cx, &self.pending_text, rows, placeholder);
        self.pending_text.clear();
        self.input = Some(input.clone());
        (input, true)
    }

    pub(super) fn set_text(&mut self, text: &str, cx: &mut App) {
        if let Some(input) = &self.input {
            input.update(cx, |input, cx| input.set_text(text, cx));
        } else {
            self.pending_text = text.to_string();
        }
    }

    pub(super) fn text(&self, cx: &App) -> String {
        self.input
            .as_ref()
            .map(|input| input.read(cx).text().to_string())
            .unwrap_or_else(|| self.pending_text.clone())
    }
}

impl ComposerView {
    fn label(anchor: &CommentAnchor) -> String {
        let position = if anchor.start.is_some() {
            format!("lines {}–{}", anchor.first(), anchor.last())
        } else {
            format!("line {}", anchor.last())
        };
        let old = if anchor.side == AnnotationSide::Old { " (old)" } else { "" };
        format!("Comment on {} {position}{old}", anchor.path.display())
    }

    pub(crate) fn new(anchor: &CommentAnchor, owner: WeakEntity<ChangeRequestTab>) -> Self {
        Self {
            label: Self::label(anchor),
            new_side: anchor.side == AnnotationSide::New,
            field: FieldState::new(String::new()),
            write: WriteStatus::default(),
            revision: 0,
            owner,
        }
    }

    pub(crate) fn ensure_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        let (input, created) = self.field.ensure(
            window,
            cx,
            Some((3, 12)),
            "Leave a comment…",
        );
        if !created {
            return input;
        }
        let owner = self.owner.clone();
        cx.subscribe(&input, move |_view, _input, event: &InputEvent, cx| match event {
            InputEvent::Submit => {
                let owner = owner.clone();
                cx.defer(move |cx| {
                    let _ = owner.update(cx, |tab, cx| {
                        let _ = tab.send_line_comment(cx);
                    });
                });
            }
            InputEvent::Changed => {
                _view.revision += 1;
                threads::push_from(_view.owner.clone(), cx);
                cx.notify();
            }
            InputEvent::Focus | InputEvent::Blur => {}
        })
        .detach();
        input
    }

    pub(crate) fn set_anchor(&mut self, anchor: &CommentAnchor, cx: &mut Context<Self>) {
        self.label = Self::label(anchor);
        self.new_side = anchor.side == AnnotationSide::New;
        self.revision += 1;
        cx.notify();
    }

    pub(crate) fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.field.set_text(text, cx);
        cx.notify();
    }

    pub(crate) fn text(&self, cx: &App) -> String {
        self.field.text(cx)
    }

    pub(crate) fn set_write(&mut self, status: WriteStatus, cx: &mut Context<Self>) {
        if self.write == status {
            return;
        }
        let error_changed = self.write.error != status.error;
        self.write = status;
        if error_changed {
            self.revision += 1;
            threads::push_from(self.owner.clone(), cx);
        }
        cx.notify();
    }
}

impl Render for ComposerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let input = self.ensure_input(window, cx);
        let owner = self.owner.clone();
        let send_owner = owner.clone();
        let text = self.text(cx);
        let write = self.write.clone();
        let mut card = threads::card(("change-request-composer-line", 0usize), &theme)
            .child(selectable_text(self.label.clone()))
            .child(Input::new(&input));
        if let Some(error) = write.error {
            card = card.child(
                div()
                    .id("change-request-line-composer-error")
                    .debug_selector(|| "change-request-line-composer-error".into())
                    .text_color(theme.ely.danger)
                    .child(selectable_text(error)),
            );
        }
        let (suggest_owner, review_owner) = (owner.clone(), owner.clone());
        let can_send = !write.busy && !text.trim().is_empty();
        card.child(
            div()
                .flex()
                .justify_end()
                .gap(px(6.0))
                .child(actions::action_button(
                    "change-request-line-composer-cancel",
                    "Cancel",
                    &theme,
                    true,
                    move |_, cx| {
                        let _ = owner.update(cx, |tab, cx| tab.cancel_composer(cx));
                    },
                ))
                .when(self.new_side, |row| {
                    row.child(actions::action_button(
                        "change-request-line-composer-suggest",
                        "Suggest",
                        &theme,
                        !write.busy,
                        move |_, cx| {
                            let _ = suggest_owner.update(cx, |tab, cx| {
                                let _ = tab.insert_suggestion(cx);
                            });
                        },
                    ))
                })
                .child(actions::action_button(
                    "change-request-line-composer-review",
                    if write.in_review { "Add to review" } else { "Start a review" },
                    &theme,
                    can_send,
                    move |_, cx| {
                        let _ = review_owner.update(cx, |tab, cx| {
                            let _ = tab.send_line_to_review(cx);
                        });
                    },
                ))
                .child(actions::action_button(
                    "change-request-line-composer-send",
                    "Comment",
                    &theme,
                    can_send,
                    move |_, cx| {
                        let _ = send_owner.update(cx, |tab, cx| {
                            let _ = tab.send_line_comment(cx);
                        });
                    },
                )),
        )
    }
}

impl ChangeRequestTab {
    /// Opens the composer at `anchor`, or moves the open one there: its text
    /// goes with it, so a second pick never throws words away.
    pub(crate) fn open_composer(
        &mut self,
        anchor: CommentAnchor,
        cx: &mut Context<Self>,
    ) {
        let revisions = match &self.range {
            RangeState::Ready { revisions, .. } => revisions.clone(),
            _ => return,
        };
        let mut reanchor_suggestion = false;
        let mut suggested_text = None;
        let mut suggested = None;
        let mut suggestion_error = None;
        if let Some(open) = self.line_composer.as_ref() {
            if let Some(previous) = open.suggested.as_ref() {
                reanchor_suggestion = true;
                let text = open.view.read(cx).text(cx);
                if text.contains(previous) {
                    match self.suggestion_block_for(&anchor, cx) {
                        Ok(replacement) => {
                            suggested_text = Some(text.replacen(previous, &replacement, 1));
                            suggested = Some(replacement);
                        }
                        Err(error) => {
                            suggested_text = Some(text.replacen(previous, "", 1).trim().to_string());
                            suggestion_error = Some(error);
                        }
                    }
                } else {
                    suggested = Some(previous.clone());
                    suggestion_error = Some(format!("The suggestion was written for {}.", open.anchor.spec()));
                }
            }
        }
        match &mut self.line_composer {
            Some(open) => {
                open.anchor = anchor.clone();
                open.revisions = revisions.clone();
                if reanchor_suggestion {
                    open.suggested = suggested;
                }
                open.view.update(cx, |view, cx| {
                    view.set_anchor(&anchor, cx);
                    if let Some(text) = suggested_text.as_deref() {
                        view.set_text(text, cx);
                    }
                });
            }
            None => {
                let owner = cx.entity().downgrade();
                let view = cx.new(|_| ComposerView::new(&anchor, owner));
                self.line_composer = Some(LineComposer { anchor: anchor.clone(), revisions, view, suggested: None });
                // A fresh composer starts from a default status: pick up the
                // tab's write state (e.g. a review already in progress) now.
                // A re-anchored composer already has a status.
                self.sync_writes(cx);
            }
        }
        if let Some(error) = suggestion_error {
            self.write_refusal = Some((WriteTarget::Composer, error));
            self.sync_writes(cx);
        }
        self.push_annotations(cx);
        if let RangeState::Ready { changes, .. } = &self.range {
            let line = Some(anchor.last());
            changes.update(cx, |changes, cx| {
                changes.focus_anchor(&anchor.path, anchor.side, line, Some(COMPOSER_KEY), cx)
            });
        }
        cx.notify();
    }

    /// `path:old|new:line` or `path:old|new:first-last`, as the socket says it.
    /// Served in every build: opening a composer writes nothing to a forge.
    pub fn compose_at(&mut self, spec: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let malformed = || "compose needs PATH:old|new:LINE or PATH:old|new:FIRST-LAST".to_string();
        let mut parts = spec.rsplitn(3, ':');
        let (lines, side, path) = (parts.next(), parts.next(), parts.next());
        let (Some(lines), Some(side), Some(path)) = (lines, side, path) else { return Err(malformed()) };
        let side = match side {
            "old" => AnnotationSide::Old,
            "new" => AnnotationSide::New,
            _ => return Err(malformed()),
        };
        let (first, last) = match lines.split_once('-') {
            Some((first, last)) => (
                first.parse::<u32>().map_err(|_| malformed())?,
                last.parse::<u32>().map_err(|_| malformed())?,
            ),
            None => {
                let line = lines.parse::<u32>().map_err(|_| malformed())?;
                (line, line)
            }
        };
        let RangeState::Ready { changes, .. } = &self.range else {
            return Err("The diff is not loaded.".to_string());
        };
        let anchor = changes.read(cx).comment_anchor(
            Path::new(path),
            side,
            last.max(first),
            (first != last).then_some(first.min(last)),
        )?;
        if !self.commentable(cx) {
            return Err("You cannot comment on this change request.".to_string());
        }
        self.open_composer(anchor, cx);
        Ok(())
    }

    /// Closes the one line composer, if one is open. Served in every build.
    pub fn cancel_composer(&mut self, cx: &mut Context<Self>) {
        if self.line_composer.take().is_some() {
            if self
                .write_refusal
                .as_ref()
                .is_some_and(|(target, _)| target == &WriteTarget::Composer)
            {
                self.write_refusal = None;
            }
            if self.write_target == Some(WriteTarget::Composer) && !self.action_busy() {
                self.actions.state = actions::ActionState::Idle;
            }
            self.push_annotations(cx);
            cx.notify();
        }
    }

    pub(crate) fn send_line_comment(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let open = self
            .line_composer
            .as_ref()
            .ok_or("No comment is being written.")?;
        let RangeState::Ready { .. } = &self.range else {
            return Err("The diff is not loaded.".to_string());
        };
        let anchor = to_line_anchor(&open.anchor);
        let body = open.view.read(cx).text(cx);
        let action = Action::LineComment { anchor, revisions: open.revisions.clone(), body };
        self.start_write(WriteTarget::Composer, action, cx)
    }

    pub(crate) fn send_line_to_review(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let open = self
            .line_composer
            .as_ref()
            .ok_or("No comment is being written.")?;
        let RangeState::Ready { .. } = &self.range else {
            return Err("The diff is not loaded.".to_string());
        };
        let anchor = to_line_anchor(&open.anchor);
        let body = open.view.read(cx).text(cx);
        let action = Action::ReviewAdd {
            target: ReviewTarget::Line { anchor, revisions: open.revisions.clone() },
            body,
        };
        self.start_write(WriteTarget::Composer, action, cx)
    }

    /// *Suggest*: the anchored lines, as the diff reads them now, in a
    /// ```suggestion block after what is written (spec §7.3). A refusal
    /// shows under the composer like a send's does, and keeps the text; a
    /// later successful Suggest clears it the way a send or a cancel does.
    pub fn insert_suggestion(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        match self.suggested_text(cx) {
            Ok((text, block)) => {
                if self
                    .write_refusal
                    .as_ref()
                    .is_some_and(|(target, _)| target == &WriteTarget::Composer)
                {
                    self.write_refusal = None;
                }
                if let Some(open) = self.line_composer.as_mut() {
                    open.suggested = Some(block);
                }
                self.composer_set_text(&text, cx);
                self.sync_writes(cx);
                Ok(())
            }
            Err(message) => {
                self.write_refusal = Some((WriteTarget::Composer, message.clone()));
                self.sync_writes(cx);
                Err(message)
            }
        }
    }

    /// What *Suggest* would insert: what is written plus the anchored
    /// lines' current text in a ```suggestion block.
    fn suggested_text(&self, cx: &App) -> Result<(String, String), String> {
        let open = self
            .line_composer
            .as_ref()
            .ok_or("No comment is being written.".to_string())?;
        let block = self.suggestion_block_for(&open.anchor, cx)?;
        let written = open.view.read(cx).text(cx);
        let text = if written.trim().is_empty() { block.clone() } else { format!("{}\n\n{block}", written.trim_end()) };
        Ok((text, block))
    }

    fn suggestion_block_for(&self, anchor: &CommentAnchor, cx: &App) -> Result<String, String> {
        if anchor.side != AnnotationSide::New {
            return Err("A suggestion replaces lines on the new side.".to_string());
        }
        let RangeState::Ready { changes, .. } = &self.range else {
            return Err("The diff is not loaded.".to_string());
        };
        let lines = changes.read(cx).anchored_text(anchor).ok_or("Those lines are not in the loaded diff.".to_string())?;
        Ok(suggestion::suggestion_block(&lines))
    }

    #[cfg(test)]
    pub(crate) fn composer_text(&self, cx: &App) -> Option<String> {
        self.line_composer.as_ref().map(|open| open.view.read(cx).text(cx))
    }

    #[cfg(test)]
    pub(crate) fn composer_in_review(&self, cx: &App) -> Option<bool> {
        self.line_composer.as_ref().map(|open| open.view.read(cx).write.in_review)
    }

    /// Every thread write goes through here: `perform`, and the field it
    /// came from learns it is in flight.
    pub(crate) fn start_write(
        &mut self,
        target: WriteTarget,
        action: Action,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let sent = self.perform(action, cx);
        match &sent {
            Ok(()) => {
                self.write_target = Some(target);
                self.write_refusal = None;
            }
            Err(message) => self.write_refusal = Some((target, message.clone())),
        }
        self.sync_writes(cx);
        sent
    }

    /// Tells the composer and every thread card whether a write is in
    /// flight and which one failed (spec §7.2: the reason shows under the
    /// field that sent it).
    pub(crate) fn sync_writes(&mut self, cx: &mut Context<Self>) {
        let busy = self.action_busy();
        let in_review = self.header.value().is_some_and(|header| header.draft.is_some());
        let failed = match &self.actions.state {
            actions::ActionState::Failed { message, .. } => Some(message.clone()),
            _ => None,
        };
        let write_target = self.write_target.clone();
        let write_refusal = self.write_refusal.clone();
        let status_for = |target: &WriteTarget| WriteStatus {
            busy,
            error: write_refusal
                .as_ref()
                .filter(|(owner, _)| owner == target)
                .map(|(_, message)| message.clone())
                .or_else(|| failed.clone().filter(|_| write_target.as_ref() == Some(target))),
            in_review,
        };
        if let Some(open) = &self.line_composer {
            let status = status_for(&WriteTarget::Composer);
            open.view.update(cx, |view, cx| view.set_write(status, cx));
        }
        for view in self.thread_views.values() {
            let thread_view = view.read(cx);
            let thread_id = thread_view.thread.id.clone();
            let comments = thread_view.thread.comments.clone();
            let belongs_to_thread = |target: &WriteTarget| match target {
                WriteTarget::Reply(id) | WriteTarget::Resolve(id) => id == &thread_id,
                WriteTarget::EditComment(id) => comments.iter().any(|comment| {
                    comment.id == *id || comment.edit.as_ref().is_some_and(|edit| edit.id == *id)
                }),
                WriteTarget::Composer => false,
                WriteTarget::Review => false,
            };
            let error = write_refusal
                .as_ref()
                .filter(|(target, _)| belongs_to_thread(target))
                .map(|(_, message)| message.clone())
                .or_else(|| {
                    failed.clone().filter(|_| {
                        write_target.as_ref().is_some_and(belongs_to_thread)
                    })
                });
            view.update(cx, |view, cx| {
                view.set_write(WriteStatus { busy, error, in_review }, cx);
            });
        }
    }

    /// Whether the gutter may offer a comment: the forge says the viewer may
    /// comment, and the diff's head is known.
    pub(crate) fn commentable(&self, _cx: &App) -> bool {
        self.header
            .value()
            .is_some_and(|header| header.capabilities.can_comment)
            && matches!(self.range, RangeState::Ready { .. })
    }

    pub(crate) fn composer_set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(open) = &self.line_composer {
            open.view.update(cx, |view, cx| view.set_text(text, cx));
        }
    }
}

fn to_line_anchor(anchor: &CommentAnchor) -> LineAnchor {
    LineAnchor {
        path: anchor.path.to_string_lossy().replace('\\', "/"),
        side: match anchor.side {
            AnnotationSide::Old => Side::Old,
            AnnotationSide::New => Side::New,
        },
        line: anchor.line,
        start: anchor.start,
    }
}
