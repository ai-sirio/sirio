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
    AnyElement, AnyView, App, Context, Entity, EventEmitter, Focusable, FontWeight, IntoElement, Render,
    SharedString, Subscription, Task, WeakEntity, Window, div, prelude::*, px,
};
use sirio_git::{FileDiff, diff_proposal};
use sirio_theme::Theme;

use crate::changes::{CHANGES_CONTEXT_LINES, ChangesTab, ChangesTabEvent, PROPOSED_SIDE_CLOSED};
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

/// `text` with the line endings of `reference` put back: normalised to LF first (the Ely
/// buffer is LF only, a proposal may carry CRLF), then CRLF again when `reference` has more
/// CRLF line ends than bare LF ones (a tie is LF).
pub fn restore_line_endings(reference: &[u8], text: &str) -> String {
    let (mut crlf, mut lf) = (0usize, 0usize);
    for (index, byte) in reference.iter().enumerate() {
        if *byte == b'\n' {
            if index > 0 && reference[index - 1] == b'\r' {
                crlf += 1;
            } else {
                lf += 1;
            }
        }
    }
    let lf_text = text.replace("\r\n", "\n");
    if crlf > lf { lf_text.replace('\n', "\r\n") } else { lf_text }
}

/// What Accept sends and the diff shows: the edited text (or the proposal) in the line
/// endings of the original, or, with no original, of the proposal as Claude sent it.
fn text_to_send(original: Option<&[u8]>, proposal: &str, edited: Option<&str>) -> String {
    // An original with no line ending at all (empty, or one line) carries no convention.
    let reference = original.filter(|bytes| bytes.contains(&b'\n')).unwrap_or(proposal.as_bytes());
    restore_line_endings(reference, edited.unwrap_or(proposal))
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
    enabled: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let mut button = Button::new(id, label)
        .disabled(!enabled)
        .on_click(move |_, window, cx| on_click(window, cx));
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
    /// The file on disk is no longer what the diff compares against (it changed after the
    /// proposal opened). Stays until the proposal ends; the diff is not recomputed (spec 10.13).
    disk_changed: bool,
    /// The warning the bar drew on its last frame, for the debug state (set while drawing).
    drawn_warning: std::cell::Cell<Option<&'static str>>,
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
    /// A diff is being computed: the body does not show what Accept would send yet.
    diff_pending: bool,
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
            disk_changed: false,
            drawn_warning: std::cell::Cell::new(None),
            scratch: PathBuf::new(),
            mode: Mode::Diff,
            code: None,
            changes: None,
            changes_subscription: None,
            diff_task: None,
            diff_generation: 0,
            diff_stats: None,
            diff_pending: false,
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
        self.disk_changed = false;
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

    /// The editor's text, once Edit has made one.
    fn edited_text(&self, cx: &App) -> Option<String> {
        self.code.as_ref().map(|code| code.read(cx).text().to_string())
    }

    /// Exactly what Accept sends, and so what the diff compares the disk against.
    fn to_send(&self, cx: &App) -> String {
        text_to_send(self.original.as_deref().map(|bytes| bytes.as_slice()), &self.proposal, self.edited_text(cx).as_deref())
    }

    /// Whether the proposed side is no longer what Claude sent: new lines the user wrote
    /// are lines Claude never receives, so that side takes no new comment.
    fn proposal_was_edited(&self, cx: &App) -> bool {
        self.edited_text(cx).is_some_and(|text| text != self.proposal.replace("\r\n", "\n"))
    }

    /// The part of "too large" that needs no allocation: the disk file and the raw proposal.
    fn too_large_inputs(&self) -> bool {
        self.original.as_ref().is_some_and(|bytes| bytes.len() as u64 > MAX_FILE_BYTES)
            || self.proposal.len() as u64 > MAX_FILE_BYTES
    }

    /// Whether Edit is available: a refused or over-cap proposal can only be answered.
    pub fn can_edit(&self) -> bool {
        self.refusal.is_none() && !self.too_large_inputs()
    }

    /// Whether the body is the editor (Edit mode) rather than the diff.
    pub fn is_editing(&self) -> bool {
        self.mode == Mode::Edit
    }

    /// Whether the body says why the proposal cannot be drawn (refused, or over the cap).
    pub fn is_refused(&self) -> bool {
        self.diff_failure.is_some() && self.mode == Mode::Diff
    }

    /// Whether the bar warns that the file has unsaved edits in an editor tab.
    pub fn is_dirty_in_editor(&self) -> bool {
        self.dirty_in_editor
    }

    /// The focus handles keys reach this tab through: the diff list, the Edit body's code
    /// editor and the open composer's input. Keyboard focus is "in" the tab when one of them
    /// contains it (the bar's buttons and text take none).
    pub fn focus_handles(&self, cx: &App) -> Vec<gpui::FocusHandle> {
        let mut handles = self.changes.as_ref().map(|changes| changes.read(cx).focus_handles()).unwrap_or_default();
        if let Some(code) = &self.code {
            handles.push(code.read(cx).focus_handle(cx));
        }
        if let Some(open) = &self.composer
            && let Some(input) = &open.view.read(cx).input
        {
            handles.push(input.focus_handle(cx));
        }
        handles
    }

    /// Whether the bar warns that the file changed on disk after the proposal opened.
    pub fn is_disk_changed(&self) -> bool {
        self.disk_changed
    }

    /// The warning the bar drew on its last frame (its words), if any.
    pub fn drawn_warning(&self) -> Option<&'static str> {
        self.drawn_warning.get()
    }

    /// Whether a look at the disk could still change the answer: there is a disk side to
    /// compare (the proposal is not refused) and the warning is not up already.
    pub fn wants_disk_check(&self) -> bool {
        self.refusal.is_none() && !self.disk_changed
    }

    /// `now` is the file on disk as it reads now (`None`: it does not exist). When it is not
    /// what the proposal opened against, the bar warns and keeps warning until the proposal
    /// ends. The diff stays as it was: whether Claude writes the text verbatim or applies its
    /// edit again is unknown, so no recomputed diff would be truthful in both cases.
    pub fn note_disk(&mut self, now: Option<&[u8]>, cx: &mut Context<Self>) {
        if self.disk_changed || self.refusal.is_some() {
            return;
        }
        if self.original.as_deref().map(|bytes| bytes.as_slice()) != now {
            self.disk_changed = true;
            cx.notify();
        }
    }

    /// The text of a proposal too large to draw (also the one an over-cap original gets).
    pub fn too_large_message() -> String {
        "This change is too large to draw (the limit is 1 MiB for the file and for the proposal). \
         You can still Reject or Accept it."
            .to_owned()
    }

    /// The Changes surface that draws the diff, once there is one (the app's debug verbs reach
    /// its view mode through it).
    pub fn changes(&self) -> Option<Entity<ChangesTab>> {
        self.changes.clone()
    }

    /// Whether Accept would send bytes the body does not show yet: a diff is being computed.
    /// Edit mode (the user sees their own text) and a body with no diff to wait for are live.
    pub fn accept_live(&self) -> bool {
        self.mode == Mode::Edit || self.diff_failure.is_some() || !self.diff_pending
    }

    /// Redraws the bar's warning: the app drives it as the file's editor tab turns dirty or clean.
    pub fn set_dirty_in_editor(&mut self, dirty: bool, cx: &mut Context<Self>) {
        if self.dirty_in_editor != dirty {
            self.dirty_in_editor = dirty;
            cx.notify();
        }
    }

    /// Computes the diff of the disk against what Accept would send, off the main thread.
    fn recompute(&mut self, cx: &mut Context<Self>) {
        self.diff_generation += 1;
        let generation = self.diff_generation;
        self.diff_task = None;
        self.diff_stats = None;
        self.diff_pending = false;
        self.diff_failure = None;
        if let Some(reason) = &self.refusal {
            self.diff_failure = Some(reason.clone());
            cx.notify();
            return;
        }
        // The raw sizes first: nothing is copied for a proposal that is refused anyway.
        let text = if self.too_large_inputs() { None } else { Some(self.to_send(cx)) };
        let Some(text) = text.filter(|text| text.len() as u64 <= MAX_FILE_BYTES) else {
            self.diff_failure = Some(Self::too_large_message());
            cx.notify();
            return;
        };
        self.diff_pending = true;
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
                tab.diff_pending = false;
                match result {
                    Ok(diff) => tab.apply_diff(Arc::new(diff), cx),
                    Err(error) => {
                        tab.diff_failure = Some(format!("Sirio could not compute the diff: {error}. You can still Reject or Accept it."));
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn apply_diff(&mut self, diff: Arc<FileDiff>, cx: &mut Context<Self>) {
        self.diff_stats = Some((diff.additions, diff.deletions));
        let is_new = self.original.is_none();
        let new_side_open = !self.proposal_was_edited(cx);
        if !new_side_open && self.composer.as_ref().is_some_and(|open| open.anchor.side == AnnotationSide::New) {
            self.composer = None;
        }
        match &self.changes {
            Some(changes) => changes.update(cx, |changes, cx| changes.set_proposal(diff, is_new, cx)),
            None => {
                let root = self.display_root.clone();
                let changes = cx.new(|cx| {
                    let mut changes = ChangesTab::for_proposal(root, diff, is_new, cx);
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
        if let Some(changes) = &self.changes {
            changes.update(cx, |changes, cx| changes.set_new_side_commentable(new_side_open, cx));
        }
        self.push_annotations(cx);
    }

    /// The buttons' own entry point, also used by the debug verb.
    pub fn press(&mut self, action: ProposalAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            ProposalAction::Accept => {
                if self.accept_live() {
                    cx.emit(ProposalEvent::Accept(self.to_send(cx)));
                }
            }
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
    /// `false` when Edit is unavailable (the tab is refused or over the cap): nothing changed.
    pub fn set_edit_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.can_edit() {
            return false;
        }
        let code = self.ensure_code(window, cx);
        code.update(cx, |code, cx| code.set_text(text.replace("\r\n", "\n"), cx));
        self.mode = Mode::Edit;
        cx.notify();
        true
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
                .line_highlighter(standalone_highlighter(language))
                .without_breakpoints();
            code.set_view_options(prefs, cx);
            code
        });
        self.code = Some(code.clone());
        code
    }

    pub fn add_comment(&mut self, comment: ProposalComment, cx: &mut Context<Self>) {
        if comment.side == CommentSide::New && self.proposal_was_edited(cx) {
            self.notice = Some(PROPOSED_SIDE_CLOSED.to_owned());
            self.push_annotations(cx);
            cx.notify();
            return;
        }
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

    /// The side and last line of the open composer, whether the diff's list laid that line out
    /// in view (`None` while it has not laid it out), and how many characters the draft holds
    /// (never the text), if a composer is open.
    pub fn composer_position(&self, cx: &App) -> Option<(CommentSide, u32, Option<bool>, usize)> {
        self.composer.as_ref().map(|open| {
            let side = match open.anchor.side {
                AnnotationSide::Old => CommentSide::Old,
                AnnotationSide::New => CommentSide::New,
            };
            let in_view = self
                .changes
                .as_ref()
                .and_then(|changes| changes.read(cx).line_in_view(&open.anchor.path, open.anchor.side, open.anchor.last()));
            (side, open.anchor.last(), in_view, open.view.read(cx).text(cx).chars().count())
        })
    }

    /// How many comment cards the diff draws for this proposal: read from the diff's own
    /// report of where each annotation was placed, not from the comments held.
    pub fn card_count(&self, cx: &App) -> usize {
        self.changes.as_ref().map_or(0, |changes| {
            changes
                .read(cx)
                .report()
                .annotations
                .iter()
                .filter(|annotation| annotation.key != COMPOSER_KEY && annotation.placed != "hidden")
                .count()
        })
    }

    /// The debug verb's entry: what a click on the gutter and typing do. The anchor is the one
    /// the gutter would pick (lines `line` to `end_line`), the composer opens through
    /// `open_composer` (so the diff scrolls to it), its text is set, and `commit` presses Add.
    /// An error leaves the tab as it was.
    #[allow(clippy::too_many_arguments)]
    pub fn comment_through_composer(
        &mut self,
        side: CommentSide,
        line: u32,
        end_line: Option<u32>,
        text: &str,
        commit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.mode == Mode::Edit {
            return Err("no diff is shown while the proposal is being edited".to_owned());
        }
        let changes = self.changes.clone().ok_or_else(|| "the proposal's diff is not loaded".to_owned())?;
        if side == CommentSide::New && self.proposal_was_edited(cx) {
            return Err(PROPOSED_SIDE_CLOSED.to_owned());
        }
        if commit && text.trim().is_empty() {
            return Err("a comment needs text".to_owned());
        }
        let (first, last) = (line.min(end_line.unwrap_or(line)), line.max(end_line.unwrap_or(line)));
        let anchor = changes.read(cx).comment_anchor(
            &self.display_path(),
            annotation_side(side),
            last,
            (first != last).then_some(first),
        )?;
        self.notice = None;
        self.open_composer(anchor, cx);
        let view = self.composer.as_ref().map(|open| open.view.clone()).ok_or_else(|| "no composer opened".to_owned())?;
        let input = view.update(cx, |view, cx| view.ensure_input(window, cx));
        input.update(cx, |input, cx| input.set_text(text.to_owned(), cx));
        if commit {
            self.commit_composer(cx);
        }
        Ok(())
    }

    fn open_composer(&mut self, anchor: CommentAnchor, cx: &mut Context<Self>) {
        if anchor.side == AnnotationSide::New && self.proposal_was_edited(cx) {
            self.notice = Some(PROPOSED_SIDE_CLOSED.to_owned());
            cx.notify();
            return;
        }
        let side = match anchor.side {
            AnnotationSide::Old => CommentSide::Old,
            AnnotationSide::New => CommentSide::New,
        };
        let label = side_phrase(side, (anchor.first(), anchor.last()));
        match &self.composer {
            Some(open) => open.view.update(cx, |view, cx| {
                view.set_label(label, cx);
                view.request_focus(cx);
            }),
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

    /// Added and deleted lines of the diff on show, once computed.
    pub fn diff_counts(&self) -> Option<(usize, usize)> {
        self.diff_stats
    }

    /// The owner terminal's title, once known (a parked proposal's worktree came back).
    pub fn set_owner_title(&mut self, title: Option<String>, cx: &mut Context<Self>) {
        if self.owner_title != title {
            self.owner_title = title;
            cx.notify();
        }
    }

    /// Whether the bar names the owner terminal that proposes the change.
    pub fn has_owner_title(&self) -> bool {
        self.owner_title.is_some()
    }

    /// The bar's line: who proposes, and the diff's counts once known.
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
        // The unsaved-edits warning wins where both apply: it is the one that decides whether
        // Accept loses the user's work.
        let warning = if self.dirty_in_editor {
            Some(DIRTY_WARNING)
        } else if self.disk_changed {
            Some(DISK_CHANGED_WARNING)
        } else {
            None
        };
        self.drawn_warning.set(warning);
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
                            bar_button("proposal-show-diff", "Show diff", false, true, press(ProposalAction::ShowDiff))
                        } else {
                            bar_button("proposal-edit", "Edit", false, true, press(ProposalAction::Edit))
                        })
                    })
                    .child(bar_button("proposal-reject", self.reject_label(), false, true, press(ProposalAction::Reject)))
                    .child(bar_button("proposal-accept", "Accept", true, self.accept_live(), press(ProposalAction::Accept))),
            )
            .when_some(warning, |bar, warning| {
                bar.child(
                    div()
                        .id("proposal-dirty")
                        .debug_selector(|| "proposal-dirty".into())
                        .text_size(theme.typography.scaled(12.0))
                        .text_color(theme.ely.warning)
                        .child(selectable_text(warning).id("proposal-dirty-text")),
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
/// The file changed after the proposal opened (saved from its editor tab, or written by
/// something else); the diff still shows the file as it was.
pub const DISK_CHANGED_WARNING: &str =
    "The file changed on disk since Claude proposed this change: the diff shows it as it was";

impl Render for ProposalTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::ely::sync_theme_if_changed(cx);
        let theme = *Theme::get(cx);
        let bar = self.render_bar(&theme, cx);
        let body = if self.mode == Mode::Edit && self.code.is_some() {
            let code = self.code.clone().expect("checked above");
            div().id("proposal-editor").debug_selector(|| "proposal-editor".into()).flex_1().min_h(px(0.0)).child(code)
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
        } else if self.diff_pending {
            div().id("proposal-loading").debug_selector(|| "proposal-loading".into()).flex_1().min_h(px(0.0)).p(px(16.0)).text_color(theme.ely.fg_muted).child("Loading…")
        } else if let Some(changes) = &self.changes {
            div().id("proposal-diff").flex_1().min_h(px(0.0)).child(changes.clone())
        } else {
            div().id("proposal-loading").debug_selector(|| "proposal-loading".into()).flex_1().min_h(px(0.0)).p(px(16.0)).text_color(theme.ely.fg_muted).child("Loading…")
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
    /// The composer was just opened (or pointed at another line): its input takes the keyboard
    /// on the next draw, which has the window a focus needs.
    focus_pending: bool,
}

impl ComposerView {
    fn new(label: String, owner: WeakEntity<ProposalTab>) -> Self {
        Self { label, input: None, revision: 0, owner, focus_pending: true }
    }

    fn request_focus(&mut self, cx: &mut Context<Self>) {
        self.focus_pending = true;
        cx.notify();
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
        if std::mem::take(&mut self.focus_pending) {
            window.focus(&input.focus_handle(cx), cx);
        }
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

    #[test]
    fn crlf_proposal_into_lf_file_becomes_lf() {
        assert_eq!(text_to_send(Some(b"a\nb\n"), "a\r\nc\r\n", None), "a\nc\n");
    }

    #[test]
    fn missing_original_follows_the_proposal() {
        assert_eq!(text_to_send(None, "a\r\nb\r\n", None), "a\r\nb\r\n");
        assert_eq!(text_to_send(None, "a\nb\n", None), "a\nb\n");
    }

    #[test]
    fn original_without_line_endings_follows_the_proposal() {
        assert_eq!(text_to_send(Some(b""), "a\r\nb\r\n", None), "a\r\nb\r\n");
        assert_eq!(text_to_send(Some(b"one line"), "a\r\nb\r\n", None), "a\r\nb\r\n");
    }

    #[test]
    fn edit_without_keystroke_sends_the_same_bytes() {
        let originals: [Option<&[u8]>; 3] = [None, Some(b"x\ny\n"), Some(b"x\r\ny\r\n")];
        for proposal in ["a\r\nb\r\n", "a\nb\n"] {
            let lf = proposal.replace("\r\n", "\n");
            for original in originals {
                assert_eq!(
                    text_to_send(original, proposal, None),
                    text_to_send(original, proposal, Some(&lf)),
                    "proposal {proposal:?}, original {original:?}"
                );
            }
        }
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
