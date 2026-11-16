//! Claude's proposed edit, shown on the Changes diff (spec §4.7, §4.8): the bar with Edit /
//! Show diff, Reject and Accept, comments beside the diff's lines, and the Ely code editor
//! for Edit. The diff is computed off the main thread against exactly the text Accept sends.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use ely_gpui_component::buttons::{Button, ButtonVariant};
use ely_gpui_component::editor::CodeEditor;
use ely_gpui_component::forms::{Input, InputEvent, TextInput};
use gpui::{
    AnyElement, AnyView, App, Context, Entity, EventEmitter, FontWeight, IntoElement, Render,
    SharedString, Subscription, Task, WeakEntity, Window, div, prelude::*, px,
};
use sirio_git::{FileDiff, diff_proposal};
use sirio_theme::Theme;

use crate::changes::{CHANGES_CONTEXT_LINES, ChangesTab, ChangesTabEvent};
use crate::diff_annotations::{Annotation, AnnotationKind, AnnotationSide, CommentAnchor};
use crate::editor::{Language, MAX_FILE_BYTES};
use crate::editor_prefs::EditorPrefs;
use crate::ely_ui::{ButtonState, new_input, text_button};
use crate::file_view::standalone_highlighter;
use crate::text_selection::selectable_text;


/// Which side of the diff a comment is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentSide {
    /// The file on disk.
    Old,
    /// The proposal (or the text edited from it).
    New,
}

/// One comment written beside a line of the proposal's diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalComment {
    pub side: CommentSide,
    /// First and last line, 1-based and inclusive.
    pub lines: (u32, u32),
    pub text: String,
}

/// `edited` with the line endings of `original` put back. The Ely buffer is LF only, so a
/// CRLF file (more CRLF than bare LF; a tie is LF) would otherwise come back as a whole-file
/// change. A missing or LF original leaves the text as it is.
pub fn restore_line_endings(original: &[u8], edited: &str) -> String {
    let (mut crlf, mut lf) = (0usize, 0usize);
    for (index, byte) in original.iter().enumerate() {
        if *byte == b'\n' {
            if index > 0 && original[index - 1] == b'\r' {
                crlf += 1;
            } else {
                lf += 1;
            }
        }
    }
    if crlf > lf {
        edited.replace("\r\n", "\n").replace('\n', "\r\n")
    } else {
        edited.to_owned()
    }
}

/// One `@<path>#L<n> <comment>` line per comment (`#L<a>-<b>` for a range, `(proposed)`
/// after the position for the proposal's side), or `None` for no comments.
pub fn format_comments(display_path: &str, comments: &[ProposalComment]) -> Option<String> {
    if comments.is_empty() {
        return None;
    }
    let lines: Vec<String> = comments
        .iter()
        .map(|comment| {
            let (first, last) = (comment.lines.0.min(comment.lines.1), comment.lines.0.max(comment.lines.1));
            let range = if first == last { format!("L{first}") } else { format!("L{first}-{last}") };
            let side = match comment.side {
                CommentSide::Old => "",
                CommentSide::New => "(proposed) ",
            };
            let text = comment.text.replace("\r\n", " ").replace(['\n', '\r'], " ");
            format!("@{display_path}#{range} {side}{text}")
        })
        .collect();
    Some(lines.join("\n"))
}

/// Everything `ProposalTab::new` and `replace` need to know about one proposal.
pub struct ProposalInit {
    /// The `tab_name` Claude gave the request; the app answers by it.
    pub tab_name: String,
    /// The file's absolute path, already canonicalised by the connection's thread.
    pub path: PathBuf,
    /// What paths are shown relative to: the owner's worktree.
    pub display_root: PathBuf,
    /// The owner terminal's title, when the connection has an owner.
    pub owner_title: Option<String>,
    /// The file as it is on disk; `None` when it does not exist yet.
    pub original: Option<Vec<u8>>,
    /// The text Claude proposes.
    pub proposal: String,
    /// Why Sirio cannot draw the proposal (binary, unreadable, ...); the tab still answers.
    pub refusal: Option<String>,
    /// The file is open with unsaved edits in an editor tab.
    pub dirty_in_editor: bool,
    /// A private directory for `diff_proposal`'s temporary files.
    pub scratch: PathBuf,
}

/// The four buttons of the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalAction {
    Accept,
    Reject,
    Edit,
    ShowDiff,
}

/// What the tab asks of the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProposalEvent {
    /// Write this text: the proposal (or the edited text), the file's line endings restored.
    Accept(String),
    /// Refuse the change; the comments, formatted, when there are any.
    Reject { comments: Option<String> },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Diff,
    Edit,
}

const COMPOSER_KEY: u64 = 0x7072_6f70_6f73_616c; // "proposal"

fn annotation_side(side: CommentSide) -> AnnotationSide {
    match side {
        CommentSide::Old => AnnotationSide::Old,
        CommentSide::New => AnnotationSide::New,
    }
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

fn bar_button(
    id: &'static str,
    label: impl Into<SharedString>,
    primary: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let mut button = Button::new(id, label).on_click(move |_, window, cx| on_click(window, cx));
    button = if primary { button.primary() } else { button.variant(ButtonVariant::Ghost) };
    div().id(id).debug_selector(move || id.to_owned()).flex_none().child(button).into_any_element()
}

fn side_phrase(side: CommentSide, lines: (u32, u32)) -> String {
    let (first, last) = (lines.0.min(lines.1), lines.0.max(lines.1));
    let position = if first == last { format!("line {first}") } else { format!("lines {first}–{last}") };
    match side {
        CommentSide::Old => format!("Comment on {position}"),
        CommentSide::New => format!("Comment on {position} (proposed)"),
    }
}

/// Claude's proposed edit, hosted on the Changes diff with the bar's controls.
pub struct ProposalTab {
    tab_name: String,
    path: PathBuf,
    display_root: PathBuf,
    owner_title: Option<String>,
    original: Option<Arc<Vec<u8>>>,
    proposal: String,
    refusal: Option<String>,
    dirty_in_editor: bool,
    scratch: PathBuf,
    mode: Mode,
    /// Created on the first Edit (it needs a window); the source of truth for the text after.
    code: Option<Entity<CodeEditor>>,
    changes: Option<Entity<ChangesTab>>,
    changes_subscription: Option<Subscription>,
    diff_task: Option<Task<()>>,
    /// The latest computation wins: a result of an older one is dropped.
    diff_generation: u64,
    diff_stats: Option<(usize, usize)>,
    /// Why the body shows a message instead of a diff.
    diff_failure: Option<String>,
    /// A pick the diff refused, in the user's words.
    notice: Option<String>,
    comments: Vec<ProposalComment>,
    cards: Vec<Entity<CommentCard>>,
    composer: Option<OpenComposer>,
}

struct OpenComposer {
    anchor: CommentAnchor,
    view: Entity<ComposerView>,
}

impl EventEmitter<ProposalEvent> for ProposalTab {}

impl ProposalTab {
    pub fn new(init: ProposalInit, cx: &mut Context<Self>) -> Self {
        let mut tab = Self {
            tab_name: String::new(),
            path: PathBuf::new(),
            display_root: PathBuf::new(),
            owner_title: None,
            original: None,
            proposal: String::new(),
            refusal: None,
            dirty_in_editor: false,
            scratch: PathBuf::new(),
            mode: Mode::Diff,
            code: None,
            changes: None,
            changes_subscription: None,
            diff_task: None,
            diff_generation: 0,
            diff_stats: None,
            diff_failure: None,
            notice: None,
            comments: Vec::new(),
            cards: Vec::new(),
            composer: None,
        };
        tab.load(init, cx);
        tab
    }

    /// A new `openDiff` with the same `tab_name`: the tab shows the new proposal in place.
    /// Comments and edits belonged to the old one and are dropped.
    pub fn replace(&mut self, init: ProposalInit, cx: &mut Context<Self>) {
        self.mode = Mode::Diff;
        self.code = None;
        self.comments.clear();
        self.cards.clear();
        self.composer = None;
        self.notice = None;
        self.load(init, cx);
        self.push_annotations(cx);
    }

    fn load(&mut self, init: ProposalInit, cx: &mut Context<Self>) {
        self.tab_name = init.tab_name;
        self.path = init.path;
        self.display_root = init.display_root;
        self.owner_title = init.owner_title;
        self.original = init.original.map(Arc::new);
        self.proposal = init.proposal;
        self.refusal = init.refusal;
        self.dirty_in_editor = init.dirty_in_editor;
        self.scratch = init.scratch;
        self.recompute(cx);
    }

    pub fn tab_name(&self) -> &str {
        &self.tab_name
    }

    pub fn comment_count(&self) -> usize {
        self.comments.len()
    }

    /// The path comments name: relative to the worktree when the file is in it.
    fn display_path(&self) -> PathBuf {
        self.path.strip_prefix(&self.display_root).unwrap_or(&self.path).to_path_buf()
    }

    pub fn comments_text(&self) -> Option<String> {
        format_comments(&self.display_path().to_string_lossy(), &self.comments)
    }

    /// The Reject button's label, and the debug state's.
    pub fn reject_label(&self) -> String {
        match self.comments.len() {
            0 => "Reject".to_owned(),
            1 => "Reject with 1 comment".to_owned(),
            count => format!("Reject with {count} comments"),
        }
    }

    fn current_text(&self, cx: &App) -> String {
        match &self.code {
            Some(code) => code.read(cx).text().to_string(),
            None => self.proposal.clone(),
        }
    }

    /// Exactly what Accept sends, and so what the diff compares the disk against.
    fn text_to_send(&self, cx: &App) -> String {
        let original = self.original.as_deref().map_or(&[][..], |bytes| bytes.as_slice());
        restore_line_endings(original, &self.current_text(cx))
    }

    fn too_large(&self, text: &str) -> bool {
        self.original.as_ref().is_some_and(|bytes| bytes.len() as u64 > MAX_FILE_BYTES)
            || text.len() as u64 > MAX_FILE_BYTES
    }

    fn can_edit(&self) -> bool {
        self.refusal.is_none() && !self.too_large(&self.proposal)
    }

    /// Why no diff is drawn for `text`, when it is not.
    fn blocker(&self, text: &str) -> Option<String> {
        if let Some(reason) = &self.refusal {
            return Some(reason.clone());
        }
        self.too_large(text).then(|| {
            "This change is too large to draw (the limit is 1 MiB for the file and for the proposal). \
             You can still Reject or Accept it."
                .to_owned()
        })
    }

    /// Computes the diff of the disk against what Accept would send, off the main thread.
    fn recompute(&mut self, cx: &mut Context<Self>) {
        self.diff_generation += 1;
        let generation = self.diff_generation;
        self.diff_task = None;
        let text = self.text_to_send(cx);
        if let Some(reason) = self.blocker(&text) {
            self.diff_failure = Some(reason);
            self.diff_stats = None;
            cx.notify();
            return;
        }
        self.diff_failure = None;
        let original = self.original.clone();
        let display_path = self.display_path();
        let scratch = self.scratch.clone();
        self.diff_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    diff_proposal(
                        original.as_deref().map(|bytes| bytes.as_slice()),
                        text.as_bytes(),
                        &display_path,
                        &scratch,
                        CHANGES_CONTEXT_LINES,
                    )
                })
                .await;
            let _ = this.update(cx, |tab, cx| {
                if tab.diff_generation != generation {
                    return;
                }
                tab.diff_task = None;
                match result {
                    Ok(diff) => tab.apply_diff(Arc::new(diff), cx),
                    Err(error) => {
                        tab.diff_stats = None;
                        tab.diff_failure = Some(format!("Sirio could not compute the diff: {error}"));
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn apply_diff(&mut self, diff: Arc<FileDiff>, cx: &mut Context<Self>) {
        self.diff_stats = Some((diff.additions, diff.deletions));
        match &self.changes {
            Some(changes) => changes.update(cx, |changes, cx| changes.set_proposal(diff, cx)),
            None => {
                let root = self.display_root.clone();
                let changes = cx.new(|cx| {
                    let mut changes = ChangesTab::for_proposal(root, diff, cx);
                    changes.set_commentable(true, cx);
                    changes
                });
                self.changes_subscription =
                    Some(cx.subscribe(&changes, |tab, _, event: &ChangesTabEvent, cx| match event {
                        ChangesTabEvent::CommentOn(anchor) => {
                            tab.notice = None;
                            tab.open_composer(anchor.clone(), cx);
                        }
                        ChangesTabEvent::CommentRefused(reason) => {
                            tab.notice = Some(reason.clone());
                            cx.notify();
                        }
                        ChangesTabEvent::OpenFile(_) => {}
                    }));
                self.changes = Some(changes);
            }
        }
        self.push_annotations(cx);
    }

    /// The buttons' own entry point, also used by the debug verb.
    pub fn press(&mut self, action: ProposalAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            ProposalAction::Accept => cx.emit(ProposalEvent::Accept(self.text_to_send(cx))),
            ProposalAction::Reject => cx.emit(ProposalEvent::Reject { comments: self.comments_text() }),
            ProposalAction::Edit => {
                if self.can_edit() && self.mode == Mode::Diff {
                    self.ensure_code(window, cx);
                    self.mode = Mode::Edit;
                    cx.notify();
                }
            }
            ProposalAction::ShowDiff => {
                if self.mode == Mode::Edit {
                    self.mode = Mode::Diff;
                    self.recompute(cx);
                }
            }
        }
    }

    /// Puts `text` in the editor and shows it (the debug verb's and the tests' entry).
    pub fn set_edit_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_edit() {
            return;
        }
        let code = self.ensure_code(window, cx);
        code.update(cx, |code, cx| code.set_text(text.replace("\r\n", "\n"), cx));
        self.mode = Mode::Edit;
        cx.notify();
    }

    fn ensure_code(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<CodeEditor> {
        if let Some(code) = &self.code {
            return code.clone();
        }
        let language = Language::from_path(&self.path);
        let text = self.proposal.replace("\r\n", "\n");
        let prefs = EditorPrefs::current(cx).view_options();
        let code = cx.new(|cx| {
            let mut code = CodeEditor::new(text, window, cx)
                .language(language.name())
                .highlighter(standalone_highlighter(language))
                .without_breakpoints();
            code.set_view_options(prefs, cx);
            code
        });
        self.code = Some(code.clone());
        code
    }

    pub fn add_comment(&mut self, comment: ProposalComment, cx: &mut Context<Self>) {
        self.comments.push(comment);
        self.rebuild_cards(cx);
        self.push_annotations(cx);
        cx.notify();
    }

    fn remove_comment(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.comments.len() {
            self.comments.remove(index);
            self.rebuild_cards(cx);
            self.push_annotations(cx);
            cx.notify();
        }
    }

    fn rebuild_cards(&mut self, cx: &mut Context<Self>) {
        let owner = cx.entity().downgrade();
        self.cards = self
            .comments
            .iter()
            .enumerate()
            .map(|(index, comment)| {
                let card = CommentCard {
                    index,
                    label: side_phrase(comment.side, comment.lines),
                    text: comment.text.clone(),
                    owner: owner.clone(),
                };
                cx.new(|_| card)
            })
            .collect();
    }

    fn open_composer(&mut self, anchor: CommentAnchor, cx: &mut Context<Self>) {
        let side = match anchor.side {
            AnnotationSide::Old => CommentSide::Old,
            AnnotationSide::New => CommentSide::New,
        };
        let label = side_phrase(side, (anchor.first(), anchor.last()));
        match &self.composer {
            Some(open) => open.view.update(cx, |view, cx| view.set_label(label, cx)),
            None => {
                let owner = cx.entity().downgrade();
                let view = cx.new(|_| ComposerView::new(label, owner));
                self.composer = Some(OpenComposer { anchor: anchor.clone(), view });
            }
        }
        if let Some(open) = &mut self.composer {
            open.anchor = anchor.clone();
        }
        self.push_annotations(cx);
        if let Some(changes) = &self.changes {
            let line = Some(anchor.last());
            changes.update(cx, |changes, cx| {
                changes.focus_anchor(&anchor.path, anchor.side, line, Some(COMPOSER_KEY), cx)
            });
        }
        cx.notify();
    }

    fn cancel_composer(&mut self, cx: &mut Context<Self>) {
        self.composer = None;
        self.push_annotations(cx);
        cx.notify();
    }

    fn commit_composer(&mut self, cx: &mut Context<Self>) {
        let Some(open) = &self.composer else { return };
        let text = open.view.read(cx).text(cx).trim().to_owned();
        if text.is_empty() {
            return;
        }
        let side = match open.anchor.side {
            AnnotationSide::Old => CommentSide::Old,
            AnnotationSide::New => CommentSide::New,
        };
        let lines = (open.anchor.first(), open.anchor.last());
        self.composer = None;
        self.add_comment(ProposalComment { side, lines, text }, cx);
    }

    /// Hands the diff its comment cards and the open composer.
    fn push_annotations(&mut self, cx: &mut Context<Self>) {
        let Some(changes) = self.changes.clone() else { return };
        let path = self.display_path();
        let mut annotations = Vec::new();
        let mut views: HashMap<u64, AnyView> = HashMap::new();
        for (index, comment) in self.comments.iter().enumerate() {
            let key = index as u64 + 1;
            let (first, last) = (comment.lines.0.min(comment.lines.1), comment.lines.0.max(comment.lines.1));
            annotations.push(Annotation {
                key,
                path: path.clone(),
                side: annotation_side(comment.side),
                line: Some(last),
                start_line: (first != last).then_some(first),
                kind: AnnotationKind::Thread { open: false },
                revision: 0,
            });
            if let Some(card) = self.cards.get(index) {
                views.insert(key, card.clone().into());
            }
        }
        if let Some(open) = &self.composer {
            annotations.push(Annotation {
                key: COMPOSER_KEY,
                path,
                side: open.anchor.side,
                line: Some(open.anchor.last()),
                start_line: open.anchor.start.map(|_| open.anchor.first()),
                kind: AnnotationKind::Composer,
                revision: open.view.read(cx).revision,
            });
            views.insert(COMPOSER_KEY, open.view.clone().into());
        }
        changes.update(cx, |changes, cx| changes.set_annotations(annotations, views, cx));
    }

    fn bar_text(&self) -> String {
        let who = match &self.owner_title {
            Some(title) => format!("Claude · {title} proposes a change"),
            None => "Claude proposes a change".to_owned(),
        };
        match self.diff_stats {
            Some((added, deleted)) => format!("{who} · +{added} −{deleted}"),
            None => who,
        }
    }

    fn render_bar(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let press = |action: ProposalAction| {
            let entity = entity.clone();
            move |window: &mut Window, cx: &mut App| {
                entity.update(cx, |tab, cx| tab.press(action, window, cx));
            }
        };
        let in_edit = self.mode == Mode::Edit;
        div()
            .id("proposal-bar")
            .debug_selector(|| "proposal-bar".into())
            .w_full()
            .px(px(10.0))
            .py(px(6.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .border_b_1()
            .border_color(theme.ely.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(theme.typography.scaled(12.5))
                            .text_color(theme.ely.fg)
                            .child(selectable_text(self.bar_text()).id("proposal-bar-text")),
                    )
                    .when(self.can_edit(), |row| {
                        row.child(if in_edit {
                            bar_button("proposal-show-diff", "Show diff", false, press(ProposalAction::ShowDiff))
                        } else {
                            bar_button("proposal-edit", "Edit", false, press(ProposalAction::Edit))
                        })
                    })
                    .child(bar_button("proposal-reject", self.reject_label(), false, press(ProposalAction::Reject)))
                    .child(bar_button("proposal-accept", "Accept", true, press(ProposalAction::Accept))),
            )
            .when(self.dirty_in_editor, |bar| {
                bar.child(
                    div()
                        .id("proposal-dirty")
                        .debug_selector(|| "proposal-dirty".into())
                        .text_size(theme.typography.scaled(12.0))
                        .text_color(theme.ely.warning)
                        .child(selectable_text(DIRTY_WARNING).id("proposal-dirty-text")),
                )
            })
            .when_some(self.notice.clone(), |bar, notice| {
                bar.child(
                    div()
                        .text_size(theme.typography.scaled(12.0))
                        .text_color(theme.ely.fg_muted)
                        .child(selectable_text(notice).id("proposal-notice")),
                )
            })
    }
}

const DIRTY_WARNING: &str = "Unsaved edits in this file: accepting lets Claude overwrite it on disk";

impl Render for ProposalTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::ely::sync_theme_if_changed(cx);
        let theme = *Theme::get(cx);
        let bar = self.render_bar(&theme, cx);
        let body = if self.mode == Mode::Edit && self.code.is_some() {
            let code = self.code.clone().expect("checked above");
            div().id("proposal-edit").debug_selector(|| "proposal-edit".into()).flex_1().min_h(px(0.0)).child(code)
        } else if let Some(reason) = &self.diff_failure {
            div()
                .id("proposal-message")
                .debug_selector(|| "proposal-message".into())
                .flex_1()
                .min_h(px(0.0))
                .p(px(16.0))
                .text_size(theme.typography.scaled(13.0))
                .text_color(theme.ely.fg_muted)
                .child(selectable_text(reason.clone()).id("proposal-message-text"))
        } else if let Some(changes) = &self.changes {
            div().id("proposal-diff").flex_1().min_h(px(0.0)).child(changes.clone())
        } else {
            div().id("proposal-loading").flex_1().min_h(px(0.0)).p(px(16.0)).text_color(theme.ely.fg_muted).child("Loading…")
        };
        div().size_full().flex().flex_col().bg(theme.ely.bg).child(bar).child(body)
    }
}

/// One written comment, under its line.
struct CommentCard {
    index: usize,
    label: String,
    text: String,
    owner: WeakEntity<ProposalTab>,
}

impl Render for CommentCard {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let (owner, index) = (self.owner.clone(), self.index);
        card(("proposal-comment", self.index), &theme)
            .child(
                div()
                    .text_size(theme.typography.scaled(12.0))
                    .text_color(theme.ely.fg_muted)
                    .child(selectable_text(self.label.clone()).id(("proposal-comment-label", self.index))),
            )
            .child(selectable_text(self.text.clone()).id(("proposal-comment-text", self.index)))
            .child(
                div().flex().justify_end().child(text_button(
                    "proposal-comment-remove",
                    "Remove",
                    None,
                    ButtonState::IDLE,
                    move |_, cx| {
                        let _ = owner.update(cx, |tab, cx| tab.remove_comment(index, cx));
                    },
                )),
            )
    }
}

/// The comment being written, under the line it is about.
struct ComposerView {
    label: String,
    input: Option<Entity<TextInput>>,
    /// Bumped when the card's height may have changed, so the list measures it again.
    revision: u64,
    owner: WeakEntity<ProposalTab>,
}

impl ComposerView {
    fn new(label: String, owner: WeakEntity<ProposalTab>) -> Self {
        Self { label, input: None, revision: 0, owner }
    }

    fn set_label(&mut self, label: String, cx: &mut Context<Self>) {
        self.label = label;
        self.revision += 1;
        cx.notify();
    }

    fn text(&self, cx: &App) -> String {
        self.input.as_ref().map(|input| input.read(cx).text().to_string()).unwrap_or_default()
    }

    fn ensure_input(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<TextInput> {
        if let Some(input) = &self.input {
            return input.clone();
        }
        let input = new_input(window, cx, "", Some((2, 8)), "Leave a comment…");
        let owner = self.owner.clone();
        cx.subscribe(&input, move |view, _input, event: &InputEvent, cx| match event {
            InputEvent::Submit => {
                let owner = owner.clone();
                cx.defer(move |cx| {
                    let _ = owner.update(cx, |tab, cx| tab.commit_composer(cx));
                });
            }
            InputEvent::Changed => {
                view.revision += 1;
                let owner = view.owner.clone();
                cx.defer(move |cx| {
                    let _ = owner.update(cx, |tab, cx| tab.push_annotations(cx));
                });
                cx.notify();
            }
            InputEvent::Focus | InputEvent::Blur => {}
        })
        .detach();
        self.input = Some(input.clone());
        input
    }
}

impl Render for ComposerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let input = self.ensure_input(window, cx);
        let can_add = !self.text(cx).trim().is_empty();
        let (cancel_owner, add_owner) = (self.owner.clone(), self.owner.clone());
        card("proposal-composer", &theme)
            .debug_selector(|| "proposal-composer".into())
            .child(selectable_text(self.label.clone()).id("proposal-composer-label"))
            .child(Input::new(&input))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(6.0))
                    .child(text_button(
                        "proposal-composer-cancel",
                        "Cancel",
                        None,
                        ButtonState::IDLE,
                        move |_, cx| {
                            let _ = cancel_owner.update(cx, |tab, cx| tab.cancel_composer(cx));
                        },
                    ))
                    .child(text_button(
                        "proposal-composer-add",
                        "Add",
                        None,
                        ButtonState::enabled(can_add),
                        move |_, cx| {
                            let _ = add_owner.update(cx, |tab, cx| tab.commit_composer(cx));
                        },
                    )),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lf_file_is_unchanged() {
        assert_eq!(restore_line_endings(b"a\nb\n", "a\nc\n"), "a\nc\n");
    }

    #[test]
    fn crlf_file_gets_crlf_back() {
        assert_eq!(restore_line_endings(b"a\r\nb\r\n", "a\nc\n"), "a\r\nc\r\n");
    }

    #[test]
    fn cr_in_edited_text_is_not_doubled() {
        assert_eq!(restore_line_endings(b"a\r\nb\r\n", "a\r\nb\n"), "a\r\nb\r\n");
    }

    #[test]
    fn mixed_file_follows_its_majority() {
        assert_eq!(restore_line_endings(b"a\r\nb\r\nc\n", "x\ny\n"), "x\r\ny\r\n");
        assert_eq!(restore_line_endings(b"a\nb\nc\r\n", "x\ny\n"), "x\ny\n");
        assert_eq!(restore_line_endings(b"a\r\nb\n", "x\ny\n"), "x\ny\n", "a tie is LF");
    }

    #[test]
    fn no_trailing_newline_is_kept_absent() {
        assert_eq!(restore_line_endings(b"a\r\nb", "a\nb"), "a\r\nb");
    }

    #[test]
    fn missing_original_keeps_lf() {
        assert_eq!(restore_line_endings(b"", "a\nb\n"), "a\nb\n");
    }

    fn comment(side: CommentSide, lines: (u32, u32), text: &str) -> ProposalComment {
        ProposalComment { side, lines, text: text.to_owned() }
    }

    #[test]
    fn one_line_per_comment() {
        let comments = [
            comment(CommentSide::Old, (12, 12), "why"),
            comment(CommentSide::Old, (30, 30), "and this"),
        ];
        assert_eq!(
            format_comments("src/a.rs", &comments).as_deref(),
            Some("@src/a.rs#L12 why\n@src/a.rs#L30 and this")
        );
    }

    #[test]
    fn range_is_l_start_dash_end() {
        let comments = [comment(CommentSide::Old, (12, 14), "text")];
        assert_eq!(format_comments("src/a.rs", &comments).as_deref(), Some("@src/a.rs#L12-14 text"));
    }

    #[test]
    fn new_side_is_marked_proposed() {
        let comments = [comment(CommentSide::New, (12, 12), "text")];
        assert_eq!(
            format_comments("src/a.rs", &comments).as_deref(),
            Some("@src/a.rs#L12 (proposed) text")
        );
    }

    #[test]
    fn newlines_inside_a_comment_become_spaces() {
        let comments = [comment(CommentSide::Old, (1, 1), "one\ntwo\r\nthree")];
        assert_eq!(format_comments("a.rs", &comments).as_deref(), Some("@a.rs#L1 one two three"));
    }

    #[test]
    fn empty_list_is_none() {
        assert_eq!(format_comments("src/a.rs", &[]), None);
    }
}
