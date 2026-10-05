//! Review threads in *Files* (spec §4, slice B3a): which threads the diff
//! draws and in what order, and the two cards it draws them with — one
//! under its line, and one folded section per file for those the forge
//! calls outdated.

use std::hash::{DefaultHasher, Hash, Hasher};

use ely_gpui_component::data_display::{Avatar, Tag};
use ely_gpui_component::theme::AvatarSize;
use gpui::WeakEntity;
use sirio_forge::{ReviewThread, Side, ThreadComment};

use crate::diff_annotations::{Annotation, AnnotationKind, AnnotationSide};
use super::*;

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
        .filter(|thread| !thread.file_level && thread.line.is_some() && first_published(thread).is_some())
        .collect();
    let at = |thread: &ReviewThread| first_published(thread).and_then(|comment| comment.at);
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

/// Each published comment's body as Markdown, aligned with them.
fn comment_docs(thread: &ReviewThread, theme: &Theme) -> Vec<markdown::Doc> {
    thread
        .comments
        .iter()
        .filter(|comment| !comment.pending)
        .map(|comment| markdown_doc(&comment.body, theme))
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

/// The published comments of a thread, one block each.
fn comment_blocks(thread: &ReviewThread, docs: &[markdown::Doc], theme: &Theme) -> Vec<AnyElement> {
    let now = style::now();
    thread
        .comments
        .iter()
        .filter(|comment| !comment.pending)
        .zip(docs)
        .enumerate()
        .map(|(index, (comment, doc))| {
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
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
                        ),
                )
                .child(div().id(("change-request-thread-body", index)).child(
                    Chat::render_markdown_document_with_link_override(doc.clone(), theme, open_links()),
                ))
                .into_any_element()
        })
        .collect()
}

fn card(id: impl Into<gpui::ElementId>, theme: &Theme) -> gpui::Stateful<gpui::Div> {
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

/// One thread under its line: open while unresolved, folded to one line
/// once resolved, and either way a click on its header turns it over.
pub(crate) struct ThreadView {
    key: u64,
    pub(crate) thread: ReviewThread,
    docs: Vec<markdown::Doc>,
    expanded: bool,
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

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.expanded = !self.expanded;
        self.revision += 1;
        cx.notify();
        push_from(self.owner.clone(), cx);
    }
}

/// The owner reads every card's revision, this one included, so it is told
/// after this card's update has ended.
fn push_from<T>(owner: WeakEntity<ChangeRequestTab>, cx: &mut Context<T>) {
    cx.defer(move |cx| {
        let _ = owner.update(cx, |tab, cx| tab.push_annotations(cx));
    });
}

impl Render for ThreadView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _perf = sirio_perf::span("change_request.thread_render", 0);
        let theme = *Theme::get(cx);
        let thread = &self.thread;
        let published = thread.comments.iter().filter(|comment| !comment.pending).count();
        let folded = thread.resolved && !self.expanded;
        let summary = if folded {
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
        let header = div()
            .id(("change-request-thread-header", self.key))
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor_pointer()
            .child(EIcon::new(IconName::MessageSquare).size(EIconSize::Sm))
            .child(div().text_color(theme.ely.fg_muted).child(summary))
            .when(thread.side == Side::Old && !folded, |this| {
                this.child(Tag::new(("change-request-thread-old", self.key), "old"))
            })
            .when(thread.resolved && !folded, |this| {
                this.child(Tag::new(("change-request-thread-resolved", self.key), "Resolved").tone(Tone::Success))
            })
            .child(div().flex_1())
            .child(
                div()
                    .text_color(theme.ely.fg_subtle)
                    .child(plural(published, "comment", "comments")),
            )
            .on_click(cx.listener(|view, _, _, cx| view.toggle(cx)));
        card(("change-request-thread", self.key), &theme)
            .child(header)
            .when(!folded, |this| this.children(comment_blocks(thread, &self.docs, &theme)))
    }
}

/// A file's outdated threads, folded under one line at the top of the file:
/// the code each was written on no longer reads the same.
pub(crate) struct OutdatedView {
    path: String,
    threads: Vec<ReviewThread>,
    docs: Vec<Vec<markdown::Doc>>,
    open: bool,
    pub(crate) revision: u64,
    owner: WeakEntity<ChangeRequestTab>,
}

impl OutdatedView {
    fn new(path: String, threads: Vec<ReviewThread>, owner: WeakEntity<ChangeRequestTab>, theme: &Theme) -> Self {
        Self {
            docs: threads.iter().map(|thread| comment_docs(thread, theme)).collect(),
            path,
            threads,
            open: false,
            revision: 0,
            owner,
        }
    }

    fn set_threads(&mut self, threads: Vec<ReviewThread>, theme: &Theme, cx: &mut Context<Self>) {
        if threads == self.threads {
            return;
        }
        self.docs = threads.iter().map(|thread| comment_docs(thread, theme)).collect();
        self.threads = threads;
        self.revision += 1;
        cx.notify();
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.open = !self.open;
        self.revision += 1;
        cx.notify();
        push_from(self.owner.clone(), cx);
    }
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
            .when(self.open, |this| {
                this.children(self.threads.iter().zip(&self.docs).enumerate().map(|(index, (thread, docs))| {
                    div()
                        .id(("change-request-outdated-thread", index))
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .pt(px(6.0))
                        .border_t_1()
                        .border_color(theme.ely.border)
                        .child(div().text_color(theme.ely.fg_muted).child(where_label(thread)))
                        .children(thread.diff_hunk.as_deref().map(|hunk| quoted_code(hunk, &theme)))
                        .children(comment_blocks(thread, docs, &theme))
                }))
            })
    }
}

impl ChangeRequestTab {
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
        self.rebuild_thread_views(cx);
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
        let mut outdated: Vec<(String, Vec<ReviewThread>)> = Vec::new();
        for thread in drawn {
            if thread.outdated {
                match outdated.iter_mut().find(|(path, _)| *path == thread.path) {
                    Some((_, threads)) => threads.push(thread),
                    None => outdated.push((thread.path.clone(), vec![thread])),
                }
                continue;
            }
            let key = thread_key(&thread.id);
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
            thread_views.insert(key, view);
        }
        let mut outdated_views = HashMap::new();
        for (path, threads) in outdated {
            let view = match self.outdated_views.remove(&path) {
                Some(view) => {
                    view.update(cx, |view, cx| view.set_threads(threads, &theme, cx));
                    view
                }
                None => {
                    let (owner, path) = (owner.clone(), path.clone());
                    cx.new(|_| OutdatedView::new(path, threads, owner, &theme))
                }
            };
            outdated_views.insert(path, view);
        }
        self.thread_views = thread_views;
        self.outdated_views = outdated_views;
    }

    fn annotations_for(&self, cx: &App) -> Vec<Annotation> {
        let Some(listing) = self.threads.value() else {
            return Vec::new();
        };
        let mut annotations: Vec<Annotation> = drawn_in_diff(&listing.items)
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
            revision: self.outdated_views.get(&path).map_or(0, |view| view.read(cx).revision),
            path: PathBuf::from(path),
            side: AnnotationSide::New,
            line: None,
            start_line: None,
            kind: AnnotationKind::Outdated { count },
        }));
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
        assert_eq!(ids(&drawn_in_diff(&all)), vec!["normal"]);
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
}
