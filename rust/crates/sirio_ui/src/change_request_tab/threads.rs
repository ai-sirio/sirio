//! Review threads in *Files* (spec §4, slice B3a): which threads the diff
//! draws and in what order, and the two cards it draws them with — one
//! under its line, and one folded section per file for those the forge
//! calls outdated.

use std::hash::{DefaultHasher, Hash, Hasher};

use ely_gpui_component::buttons::{Button, ButtonVariant};
use ely_gpui_component::data_display::{Avatar, Tag};
use ely_gpui_component::forms::{Input, InputEvent, TextInput};
use ely_gpui_component::theme::AvatarSize;
use gpui::WeakEntity;
use sirio_forge::{CommentKind, CommentRef, ReviewTarget, ReviewThread, Side, ThreadComment};

use crate::diff_annotations::{Annotation, AnnotationKind, AnnotationSide};
use super::*;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum ConversationEntry {
    Item(usize),
    Thread(usize),
}

/// Keeps ordinary activity in its original order and inserts published threads
/// before the next later timestamp. Without thread data the old timeline stays.
pub(crate) fn merge_threads(timeline: &[TimelineItem], threads: &[ReviewThread]) -> Vec<ConversationEntry> {
    if threads.is_empty() {
        return (0..timeline.len()).map(ConversationEntry::Item).collect();
    }
    let mut published: Vec<(usize, Option<i64>)> = threads.iter().enumerate()
        .filter_map(|(index, thread)| first_published(thread).map(|comment| (index, comment.at)))
        .collect();
    published.sort_by(|(a, at_a), (b, at_b)| {
        at_a.is_none().cmp(&at_b.is_none())
            .then_with(|| at_a.cmp(at_b))
            .then_with(|| threads[*a].id.cmp(&threads[*b].id))
    });
    let mut pending = published.into_iter().peekable();
    let mut entries = Vec::new();
    for (index, item) in timeline.iter().enumerate() {
        let at = match item {
            TimelineItem::LineComment(comment) if held_by_a_thread(comment, threads) => continue,
            TimelineItem::LineComment(comment) => comment.at,
            TimelineItem::Comment { at, .. } | TimelineItem::Review { at, .. }
                | TimelineItem::Event { at, .. } => *at,
        };
        if let Some(at) = at {
            while pending.peek().is_some_and(|(_, thread_at)| thread_at.is_some_and(|time| time < at)) {
                entries.push(ConversationEntry::Thread(pending.next().unwrap().0));
            }
        }
        entries.push(ConversationEntry::Item(index));
    }
    entries.extend(pending.map(|(index, _)| ConversationEntry::Thread(index)));
    entries
}

/// Whether a loaded thread carries this line comment: same file, author and
/// words. One that none carries (a list cut short) stays in the timeline.
fn held_by_a_thread(comment: &sirio_forge::LineComment, threads: &[ReviewThread]) -> bool {
    threads.iter().any(|thread| {
        thread.path == comment.path
            && thread.comments.iter().any(|held| held.author == comment.author && held.body == comment.body)
    })
}

/// How much of an outdated thread's quoted code is shown.
const HUNK_LINES: usize = 8;

fn first_published(thread: &ReviewThread) -> Option<&ThreadComment> {
    thread.comments.iter().find(|comment| !comment.pending)
}

/// The threads drawn in the diff: about a line, with at least one published
/// comment, ordered by that comment's time and then by id, so two threads
/// on one line keep their order across reloads.
pub(crate) fn drawn_in_diff(threads: &[ReviewThread]) -> Vec<&ReviewThread> {
    let mut drawn: Vec<&ReviewThread> = threads
        .iter()
        .filter(|thread| !thread.file_level && thread.line.is_some() && !thread.comments.is_empty())
        .collect();
    let at = |thread: &ReviewThread| first_published(thread).or(thread.comments.first()).and_then(|comment| comment.at);
    drawn.sort_by(|a, b| at(a).cmp(&at(b)).then_with(|| a.id.cmp(&b.id)));
    drawn
}

/// The outdated threads among those drawn, counted per file, files in the
/// order their first one appears.
pub(crate) fn section_counts(threads: &[ReviewThread]) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for thread in drawn_in_diff(threads).into_iter().filter(|thread| thread.outdated) {
        match counts.iter_mut().find(|(path, _)| *path == thread.path) {
            Some((_, count)) => *count += 1,
            None => counts.push((thread.path.clone(), 1)),
        }
    }
    counts
}

/// The annotation key of a thread: its forge id, hashed.
pub(crate) fn thread_key(id: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish()
}

/// The annotation key of a file's outdated section.
pub(crate) fn outdated_key(path: &str) -> u64 {
    thread_key(&format!("outdated:{path}"))
}

fn annotation_side(side: Side) -> AnnotationSide {
    match side {
        Side::Old => AnnotationSide::Old,
        Side::New => AnnotationSide::New,
    }
}

/// One drawn piece of a comment's body: Markdown, or a suggestion as a
/// small before/after change.
#[derive(Clone)]
pub(crate) enum Part {
    Doc(markdown::Doc),
    Suggestion { before: Option<Vec<String>>, after: Vec<String> },
}

/// How many lines a suggestion replaces: the thread's range, plus GitLab's
/// lines above; `None` when they cannot be read from the quoted hunk (old
/// side, or lines below the anchor).
fn replaced_count(thread: &ReviewThread, above: u32, below: u32) -> Option<usize> {
    if thread.side != Side::New || below > 0 {
        return None;
    }
    let line = thread.line?;
    let span = line.saturating_sub(thread.start_line.unwrap_or(line)) + 1;
    usize::try_from(span.checked_add(above)?).ok()
}

/// Each comment's body as parts: Markdown, and suggestions as changes.
fn comment_docs(thread: &ReviewThread, theme: &Theme) -> Vec<Vec<Part>> {
    thread
        .comments
        .iter()
        .map(|comment| {
            suggestion::split(&comment.body)
                .into_iter()
                .map(|part| match part {
                    suggestion::BodyPart::Text(text) => Part::Doc(markdown_doc(&text, theme)),
                    suggestion::BodyPart::Suggestion { lines, above, below } => Part::Suggestion {
                        before: thread
                            .diff_hunk
                            .as_deref()
                            .zip(replaced_count(thread, above, below))
                            .and_then(|(hunk, count)| suggestion::replaced_lines(hunk, count)),
                        after: lines,
                    },
                })
                .collect()
        })
        .collect()
}

fn where_label(thread: &ReviewThread) -> String {
    match (thread.start_line, thread.line) {
        (Some(start), Some(line)) if start != line => format!("{} lines {start}–{line}", thread.path),
        (_, Some(line)) => format!("{} line {line}", thread.path),
        (_, None) => thread.path.clone(),
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

pub(super) fn card(id: impl Into<gpui::ElementId>, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .mx(px(12.0))
        .my(px(4.0))
        .p(px(10.0))
        .rounded(theme.radii.control)
        .border_1()
        .border_color(theme.ely.border)
        .bg(theme.ely.bg)
        .flex()
        .flex_col()
        .gap(px(8.0))
}

pub(super) fn nested_card(id: impl Into<gpui::ElementId>, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    card(id, theme).mx(px(0.0)).my(px(0.0))
}

/// One thread under its line: open while unresolved, folded to one line
/// once resolved, and either way a click on its header turns it over.
pub(crate) struct ThreadView {
    key: u64,
    pub(crate) thread: ReviewThread,
    docs: Vec<Vec<Part>>,
    expanded: bool,
    pub(super) reply: Option<compose::FieldState>,
    pub(super) editing: Option<(CommentRef, compose::FieldState)>,
    pub(super) write: compose::WriteStatus,
    /// Bumped whenever the card's height changes, so the diff measures its
    /// row again.
    pub(crate) revision: u64,
    owner: WeakEntity<ChangeRequestTab>,
}

impl ThreadView {
    fn new(thread: ReviewThread, owner: WeakEntity<ChangeRequestTab>, theme: &Theme) -> Self {
        Self {
            key: thread_key(&thread.id),
            docs: comment_docs(&thread, theme),
            expanded: !thread.resolved,
            reply: None,
            editing: None,
            write: compose::WriteStatus::default(),
            thread,
            revision: 0,
            owner,
        }
    }

    /// A reload's copy of the same thread; a change of resolution folds or
    /// opens it the way a fresh card would be.
    fn set_thread(&mut self, thread: ReviewThread, theme: &Theme, cx: &mut Context<Self>) {
        if thread == self.thread {
            return;
        }
        if thread.resolved != self.thread.resolved {
            self.expanded = !thread.resolved;
        }
        self.docs = comment_docs(&thread, theme);
        self.thread = thread;
        self.revision += 1;
        cx.notify();
    }

    /// Only the header is drawn.
    pub(crate) fn folded(&self) -> bool {
        !self.expanded
    }

    /// A thread only the viewer can see takes no reply and no resolve.
    fn published(&self) -> usize {
        self.thread.comments.iter().filter(|comment| !comment.pending).count()
    }

    pub(crate) fn open_reply(&mut self, cx: &mut Context<Self>) {
        if self.reply.is_none() {
            self.reply = Some(compose::FieldState::new(String::new()));
            self.revision += 1;
            cx.notify();
            push_from(self.owner.clone(), cx);
        }
    }

    pub(crate) fn close_reply(&mut self, cx: &mut Context<Self>) {
        if self.reply.take().is_some() {
            self.revision += 1;
            cx.notify();
            push_from(self.owner.clone(), cx);
        }
    }

    pub(crate) fn start_edit(&mut self, comment: CommentRef, body: &str, cx: &mut Context<Self>) {
        if self.editing.as_ref().is_some_and(|(open, _)| open.id == comment.id) {
            return;
        }
        self.editing = Some((comment, compose::FieldState::new(body.to_string())));
        self.revision += 1;
        cx.notify();
        push_from(self.owner.clone(), cx);
    }

    pub(crate) fn close_edit(&mut self, cx: &mut Context<Self>) {
        if self.editing.take().is_some() {
            self.revision += 1;
            cx.notify();
            push_from(self.owner.clone(), cx);
        }
    }

    pub(crate) fn set_write(&mut self, status: compose::WriteStatus, cx: &mut Context<Self>) {
        if self.write == status {
            return;
        }
        let error_changed = self.write.error != status.error;
        self.write = status;
        if error_changed {
            self.revision += 1;
            push_from(self.owner.clone(), cx);
        }
        cx.notify();
    }

    pub(crate) fn reply_text(&self, cx: &App) -> Option<String> {
        self.reply.as_ref().map(|field| field.text(cx))
    }

    pub(crate) fn set_reply_text(&mut self, text: &str, cx: &mut Context<Self>) -> Result<(), String> {
        self.reply.as_mut().ok_or("no reply is being written")?.set_text(text, cx);
        cx.notify();
        Ok(())
    }

    pub(crate) fn set_edit_text(&mut self, text: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let Some((_, field)) = &mut self.editing else {
            return Err("no thread comment is being edited".to_string());
        };
        field.set_text(text, cx);
        cx.notify();
        Ok(())
    }

    pub(crate) fn edit_value(&self, cx: &App) -> Option<(CommentRef, String)> {
        self.editing
            .as_ref()
            .map(|(comment, field)| (comment.clone(), field.text(cx)))
    }

    pub(crate) fn editing_comment_id(&self) -> Option<String> {
        self.editing.as_ref().map(|(comment, _)| comment.id.clone())
    }

    fn ensure_reply_input(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<TextInput> {
        let (input, created) = self.reply.as_mut().expect("reply field is open").ensure(
            window,
            cx,
            Some((3, 12)),
            "Reply…",
        );
        if created {
            let owner = self.owner.clone();
            let thread = self.thread.id.clone();
            cx.subscribe(&input, move |view, _input, event: &InputEvent, cx| match event {
                InputEvent::Submit => {
                    let (owner, thread) = (owner.clone(), thread.clone());
                    cx.defer(move |cx| {
                        let _ = owner.update(cx, |tab, cx| {
                            let _ = tab.send_reply(&thread, cx);
                        });
                    });
                }
                InputEvent::Changed => {
                    view.revision += 1;
                    push_from(view.owner.clone(), cx);
                    cx.notify();
                }
                InputEvent::Focus | InputEvent::Blur => {}
            }).detach();
        }
        input
    }

    fn ensure_edit_input(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<TextInput> {
        let (input, created) = self.editing.as_mut().expect("edit field is open").1.ensure(
            window,
            cx,
            Some((3, 12)),
            "Edit comment…",
        );
        if created {
            let owner = self.owner.clone();
            cx.subscribe(&input, move |view, _input, event: &InputEvent, cx| match event {
                InputEvent::Submit => {
                    let owner = owner.clone();
                    cx.defer(move |cx| {
                        let _ = owner.update(cx, |tab, cx| {
                            let _ = tab.save_thread_comment_edit(cx);
                        });
                    });
                }
                InputEvent::Changed => {
                    view.revision += 1;
                    push_from(view.owner.clone(), cx);
                    cx.notify();
                }
                InputEvent::Focus | InputEvent::Blur => {}
            }).detach();
        }
        input
    }

    fn render_comments(&mut self, window: &mut Window, cx: &mut Context<Self>, theme: &Theme) -> Vec<AnyElement> {
        let now = style::now();
        let comments: Vec<(usize, (ThreadComment, Vec<Part>))> = self.thread.comments.iter()
            .cloned()
            .zip(self.docs.iter().cloned())
            .enumerate()
            .collect();
        // One running number for every suggestion in the card, so each of
        // its elements keeps a unique id.
        let mut suggestion_no = 0usize;
        comments.into_iter().map(|(index, (comment, parts))| {
            let editing = self.editing.as_ref().is_some_and(|(editing, _)| editing.id == comment.id);
            let mut metadata = div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(Avatar::new(("change-request-thread-avatar", index), comment.author.clone()).size(AvatarSize::Xs))
                .child(
                    div()
                        .id(("change-request-thread-author", index))
                        .font_weight(FontWeight::MEDIUM)
                        .child(selectable_text(comment.author.clone())),
                )
                .child(
                    div()
                        .id(("change-request-thread-time", index))
                        .text_color(theme.ely.fg_subtle)
                        .child(selectable_text(style::age(now, comment.at))),
                );
            if comment.pending {
                metadata = metadata.child(
                    div()
                        .flex_none()
                        .child(Tag::new(("change-request-thread-pending", index), "Pending").tone(Tone::Warning)),
                );
            }
            if comment.edit.is_some() && !editing {
                let owner = self.owner.clone();
                let comment_id = comment.id.clone();
                metadata = metadata.child(
                    div()
                        .id(("change-request-thread-comment-edit", index))
                        .debug_selector(|| "change-request-thread-comment-edit".into())
                        .flex_none()
                        .child(
                            Button::new(("change-request-thread-comment-edit-button", index), "Edit")
                                .variant(ButtonVariant::Ghost)
                                .disabled(self.write.busy)
                                .on_click(move |_, _, cx| {
                                    let comment_id = comment_id.clone();
                                    let _ = owner.update(cx, |tab, cx| {
                                        let _ = tab.start_thread_comment_edit(&comment_id, cx);
                                    });
                                }),
                        ),
                );
            }
            if comment.edit.as_ref().is_some_and(|edit| edit.kind == CommentKind::Draft) && !editing {
                let owner = self.owner.clone();
                let comment_id = comment.id.clone();
                metadata = metadata.child(
                    div()
                        .id(("change-request-thread-comment-delete", index))
                        .debug_selector(|| "change-request-thread-comment-delete".into())
                        .flex_none()
                        .child(
                            Button::new(("change-request-thread-comment-delete-button", index), "Delete")
                                .variant(ButtonVariant::Ghost)
                                .disabled(self.write.busy)
                                .on_click(move |_, _, cx| {
                                    let comment_id = comment_id.clone();
                                    let _ = owner.update(cx, |tab, cx| {
                                        let _ = tab.delete_draft_comment(&comment_id, cx);
                                    });
                                }),
                        ),
                );
            }
            let body = if editing {
                let input = self.ensure_edit_input(window, cx);
                let owner = self.owner.clone();
                div()
                    .id(("change-request-thread-comment-editor", index))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(Input::new(&input))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(6.0))
                            .child(actions::action_button(
                                "change-request-thread-comment-edit-cancel",
                                "Cancel",
                                theme,
                                true,
                                {
                                    let owner = owner.clone();
                                    move |_, cx| {
                                        let _ = owner.update(cx, |tab, cx| tab.cancel_thread_comment_edit(cx));
                                    }
                                },
                            ))
                            .child(actions::action_button(
                                "change-request-thread-comment-edit-save",
                                "Save",
                                theme,
                                !self.write.busy,
                                move |_, cx| {
                                    let _ = owner.update(cx, |tab, cx| {
                                        let _ = tab.save_thread_comment_edit(cx);
                                    });
                                },
                            )),
                    )
                    .into_any_element()
            } else {
                let owner = self.owner.clone();
                let drawn: Vec<AnyElement> = parts.into_iter().map(|part| match part {
                    Part::Doc(doc) => Chat::render_markdown_document_with_link_override(doc, theme, open_links()),
                    Part::Suggestion { before, after } => {
                        let no = suggestion_no;
                        suggestion_no += 1;
                        suggestion_diff(("change-request-suggestion", no), before.as_deref(), &after, theme, owner.clone())
                    }
                }).collect();
                div()
                    .id(("change-request-thread-body", index))
                    .children(drawn)
                    .into_any_element()
            };
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(metadata)
                .child(body)
                .into_any_element()
        }).collect()
    }

    pub fn open(&mut self, cx: &mut Context<Self>) {
        if !self.expanded {
            self.expanded = true;
            self.revision += 1;
            cx.notify();
            push_from(self.owner.clone(), cx);
        }
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.expanded = !self.expanded;
        self.revision += 1;
        cx.notify();
        push_from(self.owner.clone(), cx);
    }
}

/// The owner reads every card's revision, this one included, so it is told
/// after this card's update has ended.
pub(super) fn push_from<T>(owner: WeakEntity<ChangeRequestTab>, cx: &mut Context<T>) {
    cx.defer(move |cx| {
        let _ = owner.update(cx, |tab, cx| tab.push_annotations(cx));
    });
}

impl Render for ThreadView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _perf = sirio_perf::span("change_request.thread_render", 0);
        let theme = *Theme::get(cx);
        let thread = &self.thread;
        let published = thread.comments.iter().filter(|comment| !comment.pending).count();
        let pending = thread.comments.len() - published;
        let folded = self.folded();
        let summary = if folded && thread.resolved {
            let who = thread
                .resolved_by
                .clone()
                .or_else(|| first_published(thread).map(|comment| comment.author.clone()))
                .unwrap_or_default();
            format!(
                "Resolved · {who} · {}",
                plural(published.saturating_sub(1), "reply", "replies")
            )
        } else {
            where_label(thread)
        };
        let (thread_id, can_resolve, can_reply, resolved, write, outdated, diff_hunk) = (
            thread.id.clone(),
            thread.can_resolve && published > 0,
            thread.can_reply && published > 0,
            thread.resolved,
            self.write.clone(),
            thread.outdated,
            thread.diff_hunk.clone(),
        );
        let header = div()
            .id(("change-request-thread-header", self.key))
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor_pointer()
            .child(EIcon::new(IconName::MessageSquare).size(EIconSize::Sm))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .truncate()
                    .text_color(theme.ely.fg_muted)
                    .child(summary),
            )
            .when(thread.side == Side::Old && !(folded && thread.resolved), |this| {
                this.child(
                    div()
                        .flex_none()
                        .child(Tag::new(("change-request-thread-old", self.key), "old")),
                )
            })
            .when(thread.resolved && !folded, |this| {
                this.child(
                    div().flex_none().child(
                        Tag::new(("change-request-thread-resolved", self.key), "Resolved")
                            .tone(Tone::Success),
                    ),
                )
            })
            .child(
                div()
                    .flex_none()
                    .text_color(theme.ely.fg_subtle)
                    .child(plural(published, "comment", "comments")),
            )
            .when(pending > 0, |this| {
                this.child(
                    div().flex_none().child(
                        Tag::new(("change-request-thread-pending-count", self.key), format!("{pending} pending"))
                            .tone(Tone::Warning),
                    ),
                )
            })
            .when(can_resolve, |this| {
                let owner = self.owner.clone();
                let id = thread_id.clone();
                let label = if resolved { "Unresolve" } else { "Resolve" };
                this.child(
                    div()
                        .id(("change-request-thread-resolve", self.key))
                        .debug_selector(|| "change-request-thread-resolve".into())
                        .flex_none()
                        .child(
                            Button::new(("change-request-thread-resolve-button", self.key), label)
                                .variant(ButtonVariant::Ghost)
                                .disabled(write.busy)
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    let id = id.clone();
                                    let _ = owner.update(cx, |tab, cx| {
                                        let _ = tab.resolve_thread(&id, !resolved, cx);
                                    });
                                }),
                        ),
                )
            })
            .on_click(cx.listener(|view, _, _, cx| view.toggle(cx)));
        let mut card = if outdated {
            nested_card(("change-request-thread", self.key), &theme).child(header)
        } else {
            card(("change-request-thread", self.key), &theme).child(header)
        };
        if !folded {
            if let Some(hunk) = diff_hunk.as_deref()
                && outdated
            {
                card = card.child(quoted_code(hunk, &theme));
            }
            card = card.children(self.render_comments(window, cx, &theme));
            if can_reply {
                if self.reply.is_some() {
                    let input = self.ensure_reply_input(window, cx);
                    let text = self.reply_text(cx).unwrap_or_default();
                    let owner = self.owner.clone();
                    let send_owner = owner.clone();
                    let review_owner = owner.clone();
                    let cancel_id = thread_id.clone();
                    let send_id = thread_id.clone();
                    let review_id = thread_id.clone();
                    card = card.child(Input::new(&input)).child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(6.0))
                            .child(actions::action_button(
                                "change-request-thread-reply-cancel",
                                "Cancel",
                                &theme,
                                true,
                                move |_, cx| {
                                    let id = cancel_id.clone();
                                    let _ = owner.update(cx, |tab, cx| tab.cancel_thread_reply(&id, cx));
                                },
                            ))
                            .child(actions::action_button(
                                "change-request-thread-reply-review",
                                if write.in_review { "Add to review" } else { "Start a review" },
                                &theme,
                                !write.busy && !text.trim().is_empty(),
                                move |_, cx| {
                                    let id = review_id.clone();
                                    let _ = review_owner.update(cx, |tab, cx| {
                                        let _ = tab.send_reply_to_review(&id, cx);
                                    });
                                },
                            ))
                            .child(actions::action_button(
                                "change-request-thread-reply-send",
                                "Comment",
                                &theme,
                                !write.busy && !text.trim().is_empty(),
                                move |_, cx| {
                                    let id = send_id.clone();
                                    let _ = send_owner.update(cx, |tab, cx| {
                                        let _ = tab.send_reply(&id, cx);
                                    });
                                },
                            )),
                    );
                } else {
                    let owner = self.owner.clone();
                    let id = thread_id.clone();
                    card = card.child(crate::ely_ui::text_button(
                        "change-request-thread-reply-open",
                        "Reply…",
                        None,
                        crate::ely_ui::ButtonState::IDLE,
                        move |_, cx| {
                            let id = id.clone();
                            let _ = owner.update(cx, |tab, cx| {
                                let _ = tab.open_reply(&id, cx);
                            });
                        },
                    ));
                }
            }
        }
        if let Some(error) = write.error {
            card = card.child(
                div()
                    .id("change-request-thread-write-error")
                    .debug_selector(|| "change-request-thread-write-error".into())
                    .text_color(theme.ely.danger)
                    .child(selectable_text(error)),
            );
        }
        card
    }
}

/// A file's outdated threads, folded under one line at the top of the file:
/// the code each was written on no longer reads the same.
pub(crate) struct OutdatedView {
    path: String,
    threads: Vec<Entity<ThreadView>>,
    open: bool,
    pub(crate) revision: u64,
    owner: WeakEntity<ChangeRequestTab>,
}

impl OutdatedView {
    fn new(path: String, threads: Vec<Entity<ThreadView>>, owner: WeakEntity<ChangeRequestTab>) -> Self {
        Self { path, threads, open: false, revision: 0, owner }
    }

    fn set_threads(&mut self, threads: Vec<Entity<ThreadView>>, cx: &mut Context<Self>) {
        if threads == self.threads {
            return;
        }
        self.threads = threads;
        self.revision += 1;
        cx.notify();
    }

    /// Its own revision and every card's: the list measures the section
    /// again when any card in it changes height.
    pub(crate) fn measured(&self, cx: &App) -> u64 {
        self.revision + self.threads.iter().map(|view| view.read(cx).revision).sum::<u64>()
    }

    pub fn open(&mut self, cx: &mut Context<Self>) {
        if !self.open {
            self.open = true;
            self.revision += 1;
            cx.notify();
            push_from(self.owner.clone(), cx);
        }
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.open = !self.open;
        self.revision += 1;
        cx.notify();
        push_from(self.owner.clone(), cx);
    }
}

/// A proposed change: the lines it replaces in red, the proposal in green.
/// Applying it stays on the forge (spec §12).
fn suggestion_diff(id: (&'static str, usize), before: Option<&[String]>, after: &[String], theme: &Theme, owner: WeakEntity<ChangeRequestTab>) -> AnyElement {
    let before = before.unwrap_or_default();
    let before_len = before.len();
    let line = |n: usize, prefix: char, text: &str, color| {
        div().text_color(color).child(selectable_text(format!("{prefix} {text}")).id(("change-request-suggestion-line", n)))
    };
    div()
        .id(id)
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(div().text_color(theme.ely.fg_muted).child("Suggested change"))
                .child(actions::indexed_button(
                    "change-request-suggestion-open",
                    id.1,
                    "Open on the forge",
                    theme,
                    true,
                    move |_, cx| {
                        let _ = owner.update(cx, |tab, cx| tab.open_on_forge(cx));
                    },
                )),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .p(px(6.0))
                .rounded(theme.radii.control)
                .bg(theme.ely.sunken)
                .font_family(theme.typography.code_family)
                .children(before.iter().enumerate().map(|(n, text)| line(n, '-', text, theme.ely.danger)))
                .children(after.iter().enumerate().map(|(n, text)| line(before_len + n, '+', text, theme.ely.success))),
        )
        .into_any_element()
}

/// The last lines of the code a thread quoted, coloured by their prefix.
fn quoted_code(hunk: &str, theme: &Theme) -> AnyElement {
    let lines: Vec<&str> = hunk.lines().collect();
    let shown = &lines[lines.len().saturating_sub(HUNK_LINES)..];
    div()
        .flex()
        .flex_col()
        .p(px(6.0))
        .rounded(theme.radii.control)
        .bg(theme.ely.sunken)
        .font_family(theme.typography.code_family)
        .children(shown.iter().map(|line| {
            let color = match line.chars().next() {
                Some('+') => theme.ely.success,
                Some('-') => theme.ely.danger,
                _ => theme.ely.fg_muted,
            };
            div().text_color(color).child(line.to_string())
        }))
        .into_any_element()
}

impl Render for OutdatedView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _perf = sirio_perf::span("change_request.thread_render", 0);
        let theme = *Theme::get(cx);
        let key = outdated_key(&self.path);
        let header = div()
            .id(("change-request-outdated-header", key))
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor_pointer()
            .child(
                EIcon::new(if self.open { IconName::ChevronDown } else { IconName::ChevronRight })
                    .size(EIconSize::Sm),
            )
            .child(
                div()
                    .text_color(theme.ely.fg_muted)
                    .child(plural(self.threads.len(), "outdated thread", "outdated threads")),
            )
            .on_click(cx.listener(|view, _, _, cx| view.toggle(cx)));
        card(("change-request-outdated", key), &theme)
            .child(header)
            .when(self.open, |this| this.children(self.threads.iter().cloned()))
    }
}

impl ChangeRequestTab {
    /// Opens the change request on its forge: what the header's open button
    /// does, and what a suggestion's button does too (spec §12: no apply).
    pub(crate) fn open_on_forge(&self, cx: &mut App) {
        if let Some(url) = self.header.value().map(|header| header.summary.web_url.clone()) {
            cx.open_url(&url);
        }
    }

    fn thread_view(&self, id: &str) -> Result<Entity<ThreadView>, String> {
        self.thread_views.get(&thread_key(id)).cloned().ok_or_else(|| format!("no thread {id}"))
    }

    pub(crate) fn open_reply(&mut self, thread: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let view = self.thread_view(thread)?;
        if view.read(cx).published() == 0 {
            return Err("That thread is part of your review in progress.".to_string());
        }
        view.update(cx, |view, cx| view.open_reply(cx));
        Ok(())
    }

    pub(crate) fn cancel_thread_reply(&mut self, thread: &str, cx: &mut Context<Self>) {
        if let Ok(view) = self.thread_view(thread) {
            view.update(cx, |view, cx| view.close_reply(cx));
        }
        let target = compose::WriteTarget::Reply(thread.to_string());
        if self.write_refusal.as_ref().is_some_and(|(owner, _)| owner == &target) {
            self.write_refusal = None;
        }
        if self.write_target.as_ref() == Some(&target) && !self.action_busy() {
            self.actions.state = actions::ActionState::Idle;
        }
        self.sync_writes(cx);
        cx.notify();
    }

    pub(crate) fn reply_set_text(&mut self, thread: &str, text: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let view = self.thread_view(thread)?;
        view.update(cx, |view, cx| view.set_reply_text(text, cx))
    }

    pub(crate) fn send_reply(&mut self, thread: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let view = self.thread_view(thread)?;
        let body = view.read(cx).reply_text(cx).ok_or("no reply is being written")?;
        self.start_write(
            compose::WriteTarget::Reply(thread.to_string()),
            Action::Reply { thread: thread.to_string(), body },
            cx,
        )
    }

    pub(crate) fn send_reply_to_review(&mut self, thread: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let view = self.thread_view(thread)?;
        let body = view.read(cx).reply_text(cx).ok_or("no reply is being written")?;
        self.start_write(
            compose::WriteTarget::Reply(thread.to_string()),
            Action::ReviewAdd { target: ReviewTarget::Reply { thread: thread.to_string() }, body },
            cx,
        )
    }

    pub(crate) fn delete_draft_comment(&mut self, comment: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let draft = self
            .threads
            .value()
            .and_then(|listing| listing.items.iter().flat_map(|thread| thread.comments.iter()).find(|item| item.id == comment))
            .and_then(|item| item.edit.clone())
            .filter(|edit| edit.kind == CommentKind::Draft)
            .ok_or("Only a comment of your review in progress can be deleted.")?;
        self.start_write(
            compose::WriteTarget::EditComment(comment.to_string()),
            Action::DraftDelete { comment: draft },
            cx,
        )
    }

    pub(crate) fn resolve_thread(&mut self, thread: &str, resolved: bool, cx: &mut Context<Self>) -> Result<(), String> {
        self.thread_view(thread)?;
        self.start_write(
            compose::WriteTarget::Resolve(thread.to_string()),
            Action::Resolve { thread: thread.to_string(), resolved },
            cx,
        )
    }

    pub(crate) fn start_thread_comment_edit(&mut self, comment_id: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let (thread_id, comment, body) = self.threads.value()
            .and_then(|listing| listing.items.iter().find_map(|thread| {
                thread.comments.iter().find(|comment| comment.id == comment_id && comment.edit.is_some())
                    .map(|comment| (thread.id.clone(), comment.edit.clone().expect("checked"), comment.body.clone()))
            }))
            .ok_or("that comment cannot be edited")?;
        let view = self.thread_view(&thread_id).map_err(|_| "that comment cannot be edited")?;
        let other_editors: Vec<_> = self.thread_views.values()
            .filter(|other| other.read(cx).editing_comment_id().is_some_and(|id| id != comment_id))
            .cloned()
            .collect();
        for other in other_editors {
            other.update(cx, |view, cx| view.close_edit(cx));
        }
        view.update(cx, |view, cx| view.start_edit(comment, &body, cx));
        Ok(())
    }

    pub(crate) fn cancel_thread_comment_edit(&mut self, cx: &mut Context<Self>) {
        let editor = self.thread_views.values()
            .find(|view| view.read(cx).editing_comment_id().is_some())
            .cloned();
        let comment_id = editor.as_ref()
            .and_then(|view| view.read(cx).editing_comment_id());
        if let Some(view) = editor {
            view.update(cx, |view, cx| view.close_edit(cx));
        }
        if let Some(comment_id) = comment_id {
            let target = compose::WriteTarget::EditComment(comment_id);
            if self.write_refusal.as_ref().is_some_and(|(owner, _)| owner == &target) {
                self.write_refusal = None;
            }
            if self.write_target.as_ref() == Some(&target) && !self.action_busy() {
                self.actions.state = actions::ActionState::Idle;
            }
        }
        self.sync_writes(cx);
        cx.notify();
    }

    pub(crate) fn thread_comment_set_text(&mut self, text: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let view = self.thread_views.values()
            .find(|view| view.read(cx).editing_comment_id().is_some())
            .cloned()
            .ok_or("no thread comment is being edited")?;
        view.update(cx, |view, cx| view.set_edit_text(text, cx))
    }

    pub(crate) fn save_thread_comment_edit(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let view = self.thread_views.values()
            .find(|view| view.read(cx).editing_comment_id().is_some())
            .cloned()
            .ok_or("no thread comment is being edited")?;
        let (comment, body) = view.read(cx).edit_value(cx).ok_or("no thread comment is being edited")?;
        let original = self.threads.value()
            .and_then(|listing| listing.items.iter().flat_map(|thread| thread.comments.iter())
                .find(|item| item.id == comment.id || item.edit.as_ref().is_some_and(|edit| edit.id == comment.id)))
            .map(|item| item.body.clone())
            .ok_or("that comment cannot be edited")?;
        let body = crate::ely_ui::normalize(&body, true);
        if body == crate::ely_ui::normalize(&original, true) {
            self.cancel_thread_comment_edit(cx);
            return Ok(());
        }
        self.start_write(
            compose::WriteTarget::EditComment(comment.id.clone()),
            Action::EditComment { comment, body },
            cx,
        )
    }

    #[cfg(test)]
    pub(crate) fn reply_text(&self, thread: &str, cx: &App) -> Option<String> {
        self.thread_views.get(&thread_key(thread)).and_then(|view| view.read(cx).reply_text(cx))
    }

    #[cfg(test)]
    pub(crate) fn thread_write_error(&self, thread: &str, cx: &App) -> Option<String> {
        self.thread_views.get(&thread_key(thread)).and_then(|view| view.read(cx).write.error.clone())
    }

    #[cfg(test)]
    pub(crate) fn thread_in_review(&self, thread: &str, cx: &App) -> Option<bool> {
        self.thread_views.get(&thread_key(thread)).map(|view| view.read(cx).write.in_review)
    }

    #[cfg(test)]
    pub(crate) fn suggestion_parts(&self, thread: &str, cx: &App) -> Vec<(Option<Vec<String>>, Vec<String>)> {
        self.thread_views.get(&thread_key(thread)).map(|view| {
            view.read(cx).docs.iter().flatten().filter_map(|part| match part {
                Part::Suggestion { before, after } => Some((before.clone(), after.clone())),
                Part::Doc(_) => None,
            }).collect()
        }).unwrap_or_default()
    }

    /// Selects Files and opens the thread's card or outdated section. Queued
    /// reveals retain the side and annotation key until the range is ready.
    pub fn reveal_thread(&mut self, id: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let thread = self.threads.value().and_then(|listing| listing.items.iter().find(|thread| thread.id == id))
            .cloned().ok_or_else(|| format!("no thread {id}"))?;
        let key = thread_key(id);
        let outdated_drawn = thread.outdated && !drawn_in_diff(std::slice::from_ref(&thread)).is_empty();
        let anchor = if outdated_drawn {
            if let Some(view) = self.thread_views.get(&key) {
                view.update(cx, |view, cx| view.open(cx));
            }
            if let Some(section) = self.outdated_views.get(&thread.path) {
                section.update(cx, |section, cx| section.open(cx));
                Some(outdated_key(&thread.path))
            } else {
                None
            }
        } else if let Some(view) = self.thread_views.get(&key) {
            view.update(cx, |view, cx| view.open(cx));
            Some(key)
        } else if drawn_in_diff(std::slice::from_ref(&thread)).is_empty() {
            None
        } else if let Some(section) = self.outdated_views.get(&thread.path) {
            section.update(cx, |section, cx| section.open(cx));
            Some(outdated_key(&thread.path))
        } else {
            None
        };
        self.select_inner(InnerTab::Files, cx);
        let path = PathBuf::from(&thread.path);
        let side = annotation_side(thread.side);
        let line = if thread.file_level { None } else { thread.line };
        match &self.range {
            RangeState::Ready { changes, .. } => {
                changes.update(cx, |changes, cx| changes.focus_anchor(&path, side, line, anchor, cx));
            }
            _ => self.pending_reveal = Some((path, side, line, anchor)),
        }
        Ok(())
    }

    /// The socket folds the same cards as their clickable headers.
    pub fn toggle_thread(&mut self, id: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let thread = self.threads.value().and_then(|listing| listing.items.iter().find(|thread| thread.id == id))
            .ok_or_else(|| format!("no thread {id}"))?;
        if thread.outdated && !drawn_in_diff(std::slice::from_ref(thread)).is_empty() {
            if let Some(section) = self.outdated_views.get(&thread.path) {
                section.update(cx, |section, cx| section.toggle(cx));
            }
        } else if let Some(view) = self.thread_views.get(&thread_key(id)) {
            view.update(cx, |view, cx| view.toggle(cx));
        } else if !drawn_in_diff(std::slice::from_ref(thread)).is_empty() {
            if let Some(section) = self.outdated_views.get(&thread.path) {
                section.update(cx, |section, cx| section.toggle(cx));
            }
        }
        Ok(())
    }

    pub(super) fn render_thread_entry(&self, index: usize, thread: &ReviewThread, theme: &Theme, entity: &Entity<Self>) -> RailItem {
        let first = first_published(thread).expect("merged entries have a published comment");
        let published = thread.comments.iter().filter(|comment| !comment.pending).count();
        let where_ = thread.line.filter(|_| !thread.file_level).map_or("file".to_string(), |line| line.to_string());
        let (entity, id) = (entity.clone(), thread.id.clone());
        RailItem::new(
            div().id(("change-request-thread-entry-head", index)).flex().gap(px(6.0))
                .child(div().font_weight(FontWeight::MEDIUM).child(selectable_text(first.author.clone())))
                .child(div().text_color(theme.ely.fg_muted).child(selectable_text("commented"))),
        )
        .icon(IconName::MessageSquareDiff)
        .time(div().id(("change-request-thread-entry-time", index)).text_color(theme.ely.fg_subtle)
            .child(selectable_text(style::age(style::now(), first.at))))
        .child(
            div().id(("change-request-thread-entry-footnote", index)).flex().flex_wrap().gap(px(6.0))
                .text_size(theme.typography.footnote).text_color(theme.ely.fg_muted)
                .child(div().id(("change-request-thread-entry-link", index)).cursor_pointer()
                    .text_color(theme.sirio.quantity).child(format!("on {}:{where_}", thread.path))
                    .on_click(move |_, _, cx| { entity.update(cx, |tab, cx| { let _ = tab.reveal_thread(&id, cx); }); }))
                .child(selectable_text(format!("· {}", plural(published.saturating_sub(1), "reply", "replies"))))
                .when(thread.resolved, |this| this.child("· Resolved"))
                .when(thread.outdated, |this| this.child("· Outdated")),
        )
        .children(self.thread_bodies.get(&thread.id).map(|doc| {
            div().id(("change-request-thread-entry-body", index)).child(
                Chat::render_markdown_document_with_link_override(doc.clone(), theme, open_links()),
            )
        }))
    }

    pub(super) fn thread_report(&self, cx: &App) -> Vec<(String, String)> {
        let published: Vec<&ReviewThread> = self.threads.value().map(|listing| listing.items.iter()
            .filter(|thread| first_published(thread).is_some()).collect()).unwrap_or_default();
        let count = |predicate: fn(&ReviewThread) -> bool| published.iter().filter(|thread| predicate(thread)).count().to_string();
        let rows = match &self.range {
            RangeState::Ready { changes, .. } => changes.read(cx).report().annotations.into_iter().filter_map(|row| {
                if row.key == compose::COMPOSER_KEY {
                    Some(format!("composer:{}", row.placed))
                } else if let Some(view) = self.outdated_views.get(&row.path.to_string_lossy().into_owned())
                    .filter(|_| row.key == outdated_key(&row.path.to_string_lossy()))
                {
                    let section = view.read(cx);
                    Some(format!("outdated:{}:{}:{}", section.path, section.threads.len(), if section.open { "open" } else { "folded" }))
                } else {
                    let id = self.thread_ids.get(&row.key)?;
                    let view = self.thread_views.get(&row.key)?.read(cx);
                    Some(format!("{id}:{}:{}", row.placed, if view.folded() { "folded" } else { "open" }))
                }
            }).collect::<Vec<_>>().join("|"),
            _ => String::new(),
        };
        let conversation = self.header.value().zip(self.threads.value()).map_or(0, |(header, threads)| {
            merge_threads(&header.timeline, &threads.items).iter().filter(|entry| matches!(entry, ConversationEntry::Thread(_))).count()
        });
        vec![
            ("threads_open".to_string(), count(|thread| !thread.resolved)),
            ("threads_resolved".to_string(), count(|thread| thread.resolved)),
            ("threads_outdated".to_string(), count(|thread| thread.outdated)),
            ("threads_file".to_string(), count(|thread| thread.file_level)),
            ("threads_pending".to_string(), self.threads.value().map(|listing| listing.items.iter().flat_map(|thread| thread.comments.iter()).filter(|comment| comment.pending).count().to_string()).unwrap_or_else(|| "0".to_string())),
            ("thread_rows".to_string(), rows),
            ("conversation_threads".to_string(), conversation.to_string()),
            ("threads_notice".to_string(), self.threads_notice().map(|(_, text, _)| text).unwrap_or_default()),
            ("suggestions".to_string(), self.threads.value().map(|listing| drawn_in_diff(&listing.items).into_iter()
                .filter(|thread| thread.comments.iter().any(|comment| suggestion::split(&comment.body).iter().any(|part| matches!(part, suggestion::BodyPart::Suggestion { .. }))))
                .map(|thread| thread.id.clone()).collect::<Vec<_>>().join(",")).unwrap_or_default()),
        ]
    }

    /// What Files says above the diff about the thread read: a failure or a
    /// stale answer, which Retry reads again, or a list the forge cut short.
    pub(super) fn threads_notice(&self) -> Option<(Severity, String, bool)> {
        match &self.threads {
            Slot::Failed(error) => Some((Severity::Danger, format!("Review threads could not be read: {error}"), true)),
            Slot::Loaded { stale: Some(error), .. } => {
                Some((Severity::Warning, format!("Review threads may be out of date: {error}"), true))
            }
            Slot::Loaded { value, stale: None } if value.truncated => Some((
                Severity::Info,
                "Only the first review threads are shown; the rest are on the forge.".to_string(),
                false,
            )),
            _ => None,
        }
    }

    /// The notice as drawn above the diff, with Retry when reading again helps.
    pub(super) fn render_threads_notice(&self, theme: &Theme, entity: &Entity<Self>) -> Option<AnyElement> {
        let (severity, text, retry) = self.threads_notice()?;
        let entity = entity.clone();
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .pb(px(6.0))
                .child(ely_ui::message(severity, text, theme))
                .when(retry, |this| {
                    this.child(ely_ui::text_button(
                        "change-request-threads-retry",
                        "Retry",
                        Some(IconName::RefreshCw),
                        ButtonState::IDLE,
                        move |_, cx| entity.update(cx, |tab, cx| tab.retry_threads(cx)),
                    ))
                })
                .into_any_element(),
        )
    }

    /// The Retry under a failed or stale thread read.
    pub(crate) fn retry_threads(&mut self, cx: &mut Context<Self>) {
        self.load_threads(cx);
    }

    /// Reads every thread on the background executor; a later call's answer
    /// replaces an earlier one's.
    pub(crate) fn load_threads(&mut self, cx: &mut Context<Self>) {
        if self.rate_paused() {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.threads_generation += 1;
        let generation = self.threads_generation;
        let number = self.reference.number;
        self.threads.begin();
        self.threads_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { client.review_threads(number) })
                .await;
            let _ = this.update(cx, |tab, cx| {
                if tab.threads_generation == generation {
                    if let Err(error) = &result {
                        tab.note_rate_limited(error);
                    }
                    tab.apply_threads(result, cx);
                }
            });
        }));
        cx.notify();
    }

    fn apply_threads(&mut self, result: Result<Listing<ReviewThread>, ForgeError>, cx: &mut Context<Self>) {
        self.threads.finish(result);
        let theme = *Theme::get(cx);
        self.thread_bodies = self.threads.value().map(|listing| listing.items.iter()
            .filter_map(|thread| first_published(thread).map(|comment| (thread.id.clone(), markdown_doc(&comment.body, &theme))))
            .collect()).unwrap_or_default();
        self.thread_ids = self.threads.value().map(|listing| listing.items.iter()
            .map(|thread| (thread_key(&thread.id), thread.id.clone())).collect()).unwrap_or_default();
        self.rebuild_thread_views(cx);
        self.sync_writes(cx);
        self.push_annotations(cx);
        cx.notify();
    }

    /// One card per drawn thread and one section per file with outdated
    /// ones; a card whose key survives the reload is kept, with its fold.
    fn rebuild_thread_views(&mut self, cx: &mut Context<Self>) {
        let Some(listing) = self.threads.value() else {
            return;
        };
        let drawn: Vec<ReviewThread> = drawn_in_diff(&listing.items).into_iter().cloned().collect();
        let owner = cx.entity().downgrade();
        let theme = *Theme::get(cx);
        let mut thread_views = HashMap::new();
        let mut outdated: Vec<(String, Vec<Entity<ThreadView>>)> = Vec::new();
        for thread in drawn {
            let key = thread_key(&thread.id);
            let (path, is_outdated) = (thread.path.clone(), thread.outdated);
            let view = match self.thread_views.remove(&key) {
                Some(view) => {
                    view.update(cx, |view, cx| view.set_thread(thread, &theme, cx));
                    view
                }
                None => {
                    let owner = owner.clone();
                    cx.new(|_| ThreadView::new(thread, owner, &theme))
                }
            };
            if is_outdated {
                match outdated.iter_mut().find(|(at, _)| *at == path) {
                    Some((_, views)) => views.push(view.clone()),
                    None => outdated.push((path, vec![view.clone()])),
                }
            }
            thread_views.insert(key, view);
        }
        let mut outdated_views = HashMap::new();
        for (path, views) in outdated {
            let view = match self.outdated_views.remove(&path) {
                Some(view) => {
                    view.update(cx, |view, cx| view.set_threads(views, cx));
                    view
                }
                None => {
                    let (owner, path) = (owner.clone(), path.clone());
                    cx.new(|_| OutdatedView::new(path, views, owner))
                }
            };
            outdated_views.insert(path, view);
        }
        self.thread_views = thread_views;
        self.outdated_views = outdated_views;
    }

    fn annotations_for(&self, cx: &App) -> Vec<Annotation> {
        let mut annotations = Vec::new();
        if let Some(listing) = self.threads.value() {
            annotations = drawn_in_diff(&listing.items)
                .into_iter()
                .filter(|thread| !thread.outdated)
                .map(|thread| {
                    let key = thread_key(&thread.id);
                    Annotation {
                        key,
                        path: PathBuf::from(&thread.path),
                        side: annotation_side(thread.side),
                        line: thread.line,
                        start_line: thread.start_line,
                        kind: AnnotationKind::Thread { open: !thread.resolved },
                        revision: self.thread_views.get(&key).map_or(0, |view| view.read(cx).revision),
                    }
                })
                .collect();
            annotations.extend(section_counts(&listing.items).into_iter().map(|(path, count)| Annotation {
                key: outdated_key(&path),
                revision: self.outdated_views.get(&path).map_or(0, |view| view.read(cx).measured(cx)),
                path: PathBuf::from(path),
                side: AnnotationSide::New,
                line: None,
                start_line: None,
                kind: AnnotationKind::Outdated { count },
            }));
        }
        if let Some(open) = &self.line_composer {
            annotations.push(Annotation {
                key: compose::COMPOSER_KEY,
                path: open.anchor.path.clone(),
                side: open.anchor.side,
                line: Some(open.anchor.last()),
                start_line: open.anchor.start.map(|_| open.anchor.first()),
                kind: AnnotationKind::Composer,
                revision: open.view.read(cx).revision,
            });
        }
        annotations
    }

    /// Hands the diff its cards: after a read, when the diff is (re)built,
    /// and when a card folds or opens.
    pub(crate) fn push_annotations(&mut self, cx: &mut Context<Self>) {
        let RangeState::Ready { changes, .. } = &self.range else {
            return;
        };
        let changes = changes.clone();
        let annotations = self.annotations_for(cx);
        let mut views: HashMap<u64, gpui::AnyView> =
            self.thread_views.iter().map(|(key, view)| (*key, view.clone().into())).collect();
        views.extend(self.outdated_views.iter().map(|(path, view)| (outdated_key(path), view.clone().into())));
        if let Some(open) = &self.line_composer {
            views.insert(compose::COMPOSER_KEY, open.view.clone().into());
        }
        changes.update(cx, |changes, cx| changes.set_annotations(annotations, views, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sirio_forge::{ReviewThread, Side, ThreadComment};

    fn comment(id: &str, at: Option<i64>, pending: bool) -> ThreadComment {
        ThreadComment { id: id.to_string(), author: "bob".to_string(), body: "b".to_string(), at, edit: None, pending }
    }

    fn thread(id: &str, path: &str, line: Option<u32>, at: Option<i64>) -> ReviewThread {
        ReviewThread {
            id: id.to_string(),
            path: path.to_string(),
            side: Side::New,
            line,
            start_line: None,
            outdated: false,
            resolved: false,
            resolved_by: None,
            diff_hunk: None,
            can_reply: true,
            can_resolve: true,
            file_level: false,
            comments: vec![comment(&format!("{id}-c"), at, false)],
        }
    }

    fn ids(threads: &[&ReviewThread]) -> Vec<String> {
        threads.iter().map(|thread| thread.id.clone()).collect()
    }

    #[test]
    fn only_line_threads_with_a_published_comment_are_drawn() {
        let normal = thread("normal", "a.rs", Some(4), Some(10));
        let file = ReviewThread { file_level: true, line: None, ..thread("file", "a.rs", None, Some(11)) };
        let draft = ReviewThread { comments: vec![comment("d", Some(12), true)], ..thread("draft", "a.rs", Some(5), None) };
        let lineless = thread("lineless", "a.rs", None, Some(13));
        let all = [normal, file, draft, lineless];
        // Revision (g): a draft-only thread is drawn (it offers no reply and
        // no resolve); the Conversation and the open counts stay published-only.
        assert_eq!(ids(&drawn_in_diff(&all)), vec!["normal", "draft"]);
    }

    #[test]
    fn threads_are_ordered_by_their_first_published_comment() {
        let late = thread("c", "a.rs", Some(1), Some(30));
        let early = thread("b", "a.rs", Some(1), Some(10));
        let tie = thread("a", "a.rs", Some(1), Some(10));
        // A pending comment before the published one does not count.
        let mut drafted = thread("d", "a.rs", Some(1), Some(20));
        drafted.comments.insert(0, comment("p", Some(1), true));
        let all = [late, early, tie, drafted];
        assert_eq!(ids(&drawn_in_diff(&all)), vec!["a", "b", "d", "c"]);
    }

    #[test]
    fn outdated_threads_become_one_section_per_file() {
        let a1 = ReviewThread { outdated: true, ..thread("a1", "a.rs", Some(1), Some(1)) };
        let b1 = ReviewThread { outdated: true, ..thread("b1", "b.rs", Some(1), Some(2)) };
        let a2 = ReviewThread { outdated: true, ..thread("a2", "a.rs", Some(9), Some(3)) };
        let current = thread("now", "a.rs", Some(5), Some(4));
        let all = [a1, b1, a2, current];
        assert_eq!(section_counts(&all), vec![("a.rs".to_string(), 2), ("b.rs".to_string(), 1)]);
    }

    #[test]
    fn keys_are_stable_and_distinct() {
        assert_eq!(thread_key("PRRT_1"), thread_key("PRRT_1"));
        assert_ne!(thread_key("PRRT_1"), thread_key("PRRT_2"));
        assert_ne!(outdated_key("a.rs"), thread_key("a.rs"));
    }

    #[gpui::test]
    fn an_absurd_suggestion_offset_draws_no_before_lines(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let theme = cx.update(|cx| *Theme::get(cx));
        let mut absurd = thread("absurd", "a.rs", Some(10), Some(1));
        absurd.diff_hunk = Some("@@ -10,1 +10,1 @@\n-old 10\n+new 10".to_string());
        absurd.comments = vec![ThreadComment {
            id: "absurd-c".to_string(),
            author: "bob".to_string(),
            body: "```suggestion:-4294967295+0\nnew 10\n```".to_string(),
            at: Some(1),
            edit: None,
            pending: false,
        }];
        let docs = comment_docs(&absurd, &theme);
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].len(), 1);
        let (before, after) = match &docs[0][0] {
            Part::Suggestion { before, after } => (before.clone(), after.clone()),
            Part::Doc(_) => panic!("expected a suggestion"),
        };
        assert_eq!(before, None);
        assert_eq!(after, vec!["new 10".to_string()]);
    }

    #[gpui::test]
    fn a_thread_reveal_before_the_diff_keeps_its_side_and_card(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let tab = cx.new(|cx| ChangeRequestTab::new(
            crate::forge_source::testing::reference(101), String::new(), std::env::temp_dir(), cx,
        ));
        tab.update(cx, |tab, cx| {
            let old = ReviewThread { side: Side::Old, resolved: true, ..thread("old", "a.rs", Some(4), Some(1)) };
            tab.apply_threads(Ok(Listing { items: vec![old], truncated: false }), cx);
            tab.reveal_thread("old", cx).expect("a loaded thread");
            assert_eq!(tab.inner, InnerTab::Files);
            assert_eq!(tab.pending_reveal, Some((PathBuf::from("a.rs"), AnnotationSide::Old, Some(4), Some(thread_key("old")))));
            assert!(tab.thread_views[&thread_key("old")].read(cx).expanded);
            assert_eq!(tab.reveal_thread("missing", cx), Err("no thread missing".to_string()));
        });
        cx.run_until_parked();
    }

    #[gpui::test]
    fn opening_and_toggling_threads_updates_their_fold_and_report(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let tab = cx.new(|cx| ChangeRequestTab::new(
            crate::forge_source::testing::reference(101), String::new(), std::env::temp_dir(), cx,
        ));
        tab.update(cx, |tab, cx| {
            let resolved = ReviewThread { resolved: true, ..thread("r", "a.rs", Some(4), Some(1)) };
            let outdated = ReviewThread { outdated: true, ..thread("o", "a.rs", Some(5), Some(2)) };
            let file = ReviewThread { file_level: true, ..thread("f", "a.rs", None, Some(3)) };
            let draft = ReviewThread { comments: vec![comment("p", Some(4), true)], ..thread("d", "a.rs", Some(6), None) };
            tab.apply_threads(Ok(Listing { items: vec![resolved, outdated, file, draft], truncated: false }), cx);
            tab.toggle_thread("r", cx).unwrap();
            assert!(tab.thread_views[&thread_key("r")].read(cx).expanded);
            tab.toggle_thread("o", cx).unwrap();
            assert!(tab.outdated_views["a.rs"].read(cx).open);
            tab.reveal_thread("o", cx).unwrap();
            assert!(tab.outdated_views["a.rs"].read(cx).open, "reveal must open, rather than toggle, the section");
            assert_eq!(tab.pending_reveal.as_ref().unwrap().3, Some(outdated_key("a.rs")));
            assert_eq!(tab.toggle_thread("missing", cx), Err("no thread missing".to_string()));
            let report: HashMap<_, _> = tab.report(cx).into_iter().collect();
            assert_eq!(report["threads_open"], "2");
            assert_eq!(report["threads_resolved"], "1");
            assert_eq!(report["threads_outdated"], "1");
            assert_eq!(report["threads_file"], "1");
            tab.toggle_thread("o", cx).unwrap();
            tab.reveal_thread("f", cx).unwrap();
            assert_eq!(tab.pending_reveal, Some((PathBuf::from("a.rs"), AnnotationSide::New, None, None)));
            assert!(!tab.outdated_views["a.rs"].read(cx).open, "a file-level reveal must not open an unrelated outdated section");
            let changes = cx.new(|cx| ChangesTab::for_range(std::env::temp_dir(), "base".into(), "head".into(), cx));
            tab.range = RangeState::Ready {
                revisions: Revisions { base_sha: "base".into(), head_sha: "head".into(), start_sha: None },
                changes,
            };
            tab.push_annotations(cx);
            tab.toggle_thread("r", cx).unwrap();
        });
        // The owner must read the revisions after the toggling entity's
        // update finishes, even when a diff is already receiving the cards.
        cx.run_until_parked();
        tab.read_with(cx, |tab, cx| {
            let report: HashMap<_, _> = tab.report(cx).into_iter().collect();
            // Revision (g): the draft-only thread is drawn too, open by default.
            assert_eq!(report["thread_rows"], "r:hidden:folded|d:hidden:open|outdated:a.rs:1:folded");
            assert_eq!(tab.annotations_for(cx)[0].revision, 2);
        });
    }

    mod merge_threads {
        use super::*;
        use sirio_forge::{LineComment, TimelineItem, ReviewOutcome, EventKind};
        fn timeline() -> Vec<TimelineItem> {
            vec![
                TimelineItem::Comment { author: "a".into(), body: "x".into(), at: Some(10), edit: None },
                // Held by the loaded threads below: `thread` writes bob's "b" on a.rs.
                TimelineItem::LineComment(LineComment { author: "bob".into(), path: "a.rs".into(), line: Some(4), body: "b".into(), at: Some(15) }),
                TimelineItem::Review { author: "c".into(), outcome: ReviewOutcome::Commented, body: String::new(), at: Some(30), line_comments: Vec::new(), edit: None },
                TimelineItem::Event { actor: None, kind: EventKind::Merged, at: None },
            ]
        }

        #[test]
        fn line_comments_leave_the_timeline_and_threads_join_it_by_time() {
            let t1 = thread("t1", "a.rs", Some(1), Some(20));
            let t2 = thread("t2", "a.rs", Some(2), Some(5));
            let t3 = ReviewThread { comments: vec![comment("p", Some(1), true)], ..thread("t3", "a.rs", Some(3), None) };
            let t4 = thread("t4", "a.rs", Some(4), None);
            let threads = [t1, t2, t3, t4];
            assert_eq!(
                merge_threads(&timeline(), &threads),
                vec![
                    ConversationEntry::Thread(1),
                    ConversationEntry::Item(0),
                    ConversationEntry::Thread(0),
                    ConversationEntry::Item(2),
                    ConversationEntry::Item(3),
                    ConversationEntry::Thread(3),
                ]
            );
        }

        /// A list cut short (GitLab pages every discussion, system notes
        /// included) may lack the thread a line comment belongs to: the
        /// comment then stays where it was instead of vanishing from both
        /// the Conversation and the diff.
        #[test]
        fn a_line_comment_no_loaded_thread_holds_stays_in_place() {
            let mut items = timeline();
            items.insert(2, TimelineItem::LineComment(LineComment {
                author: "dave".into(), path: "b.rs".into(), line: Some(9), body: "lost?".into(), at: Some(25),
            }));
            let threads = [thread("t1", "a.rs", Some(1), Some(20))];
            assert_eq!(
                merge_threads(&items, &threads),
                vec![
                    ConversationEntry::Item(0),
                    ConversationEntry::Thread(0),
                    ConversationEntry::Item(2),
                    ConversationEntry::Item(3),
                    ConversationEntry::Item(4),
                ]
            );
        }

        #[test]
        fn with_no_threads_loaded_the_timeline_is_unchanged() {
            assert_eq!(
                merge_threads(&timeline(), &[]),
                (0..4).map(ConversationEntry::Item).collect::<Vec<_>>()
            );
        }
    }
}
