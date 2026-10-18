use std::{cell::RefCell, collections::BTreeSet, fmt, ops::Range, rc::Rc, time::Duration};

use gpui::{
    App, AppContext as _, Context, EventEmitter, FocusHandle, Focusable, HighlightStyle, Pixels,
    Point, SharedString, Subscription, Task, UniformListScrollHandle, Window,
};

use super::{
    anchors,
    buffer::Buffer,
    cursor::{Selection, merged},
    decor::{CodeLens, Diagnostic, DiffHunk, GhostText, GitMark, InlayHint},
    syntax,
    view::Frame,
};
use crate::{
    forms::{History, InputEvent, TextInput},
    theme::ActiveTheme,
};

/// How the gutter numbers lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineNumbers {
    #[default]
    Absolute,
    /// Distances from the cursor's line, which shows its own number.
    Relative,
    Hidden,
}

/// How the cursor draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorShape {
    #[default]
    Line,
    Block,
    Underline,
}

/// The view settings a host sets together. `view_options` reads them back as set.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewOptions {
    pub minimap: bool,
    pub sticky_scroll: bool,
    pub indent_guides: bool,
    pub rainbow_brackets: bool,
    pub whitespace: bool,
    /// Thin lines at these columns.
    pub rulers: Vec<usize>,
    /// Spaces a Tab inserts and an indent guide spans. Zero is raised to one.
    pub tab_size: usize,
    pub line_numbers: LineNumbers,
    pub cursor_shape: CursorShape,
}

/// What the editor tells its owner.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorEvent {
    Changed,
    /// A press on a code lens: its line and the item's place.
    Lens(usize, usize),
    /// The inline chat's prompt, sent.
    Asked(SharedString),
    ChatClosed,
    /// The read-only banner's action.
    Unlock,
    /// The pointer moved with no button down: the offset of the code under it; `None` over the gutter, past the text, or on a row that is not text.
    Pointer {
        offset: Option<usize>,
        position: Point<Pixels>,
    },
    /// A right-button press over the code: the offset under it; `None` over the gutter, past the text, or on a row that is not text.
    SecondaryClick {
        offset: Option<usize>,
        position: Point<Pixels>,
    },
    /// A left press and release at one offset with the platform modifier held on either: the gesture that goes to a definition.
    PlatformClick { offset: usize },
    /// A press on a line's git mark, the bar beside its fold chevron.
    GitMarkPressed { line: usize },
}

/// Colors for one line of text, given the line's index and its whole text. Its spans are byte offsets into that text.
type Paint = dyn Fn(usize, &str, &App) -> Vec<(Range<usize>, HighlightStyle)>;

/// The owner's colors for each line, in place of code's lexer.
#[derive(Clone)]
pub(crate) struct LineHighlighter(Rc<Paint>);

impl fmt::Debug for LineHighlighter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LineHighlighter")
    }
}

/// The editor's look and manners.
#[derive(Clone, Debug)]
pub(crate) struct Options {
    pub language: SharedString,
    pub numbers: LineNumbers,
    pub cursor: CursorShape,
    pub whitespace: bool,
    pub guides: bool,
    pub rulers: Vec<usize>,
    pub rainbow: bool,
    pub minimap: bool,
    pub sticky: bool,
    pub read_only: bool,
    /// Whether a read-only editor draws its own banner and its "Make editable".
    pub read_only_banner: bool,
    pub tab: usize,
    /// Whether Tab, Enter after an opener and indent write a tab character in place of spaces (Sirio: LOCAL-CHANGES.md).
    pub hard_tabs: bool,
    pub highlighter: Option<LineHighlighter>,
    /// Whether the gutter's breakpoint slot takes presses and draws breakpoints.
    pub breakpoints: bool,
}

/// What the owner lays over the code.
#[derive(Clone, Debug, Default)]
pub(crate) struct Marks {
    pub diagnostics: Vec<Diagnostic>,
    pub backgrounds: Vec<(Range<usize>, gpui::Hsla)>,
    /// Where `diagnostics` and `backgrounds` lie, to find a line's by search; `reindex` brings them up to date after either list changes.
    pub(crate) diagnostic_reach: super::anchors::Reach,
    pub(crate) background_reach: super::anchors::Reach,
    pub hints: Vec<InlayHint>,
    pub lenses: Vec<CodeLens>,
    pub ghost: Option<GhostText>,
    pub hunks: Vec<DiffHunk>,
    pub git: Vec<(usize, GitMark)>,
    pub breakpoints: BTreeSet<usize>,
    pub chat: Option<usize>,
}

/// A left press in progress: where it began, whether the platform modifier was held on it, and whether it was a single press with neither Shift nor Alt, the only kind a platform click can be.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Press {
    pub offset: usize,
    pub platform: bool,
    pub single: bool,
}

/// The text and cursors one undo step returns to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Snapshot {
    pub text: String,
    pub selections: Vec<Selection>,
}

/// Where the code sat when last painted, in window pixels: its left edge, the top of the first row, a character's width and a row's height.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Metrics {
    pub left: Pixels,
    pub top: Pixels,
    pub advance: Pixels,
    pub line: Pixels,
    /// The code font's size.
    pub size: Pixels,
}

const BLINK: Duration = Duration::from_millis(530);

/// Code to read and edit: syntax colors, many cursors, folds, a gutter, a minimap, and what tools lay over it.
pub struct CodeEditor {
    pub(crate) buffer: Buffer,
    pub(crate) selections: Vec<Selection>,
    pub(crate) history: History<Snapshot>,
    pub(crate) focus: FocusHandle,
    pub(crate) scroll: UniformListScrollHandle,
    pub(crate) folded: BTreeSet<usize>,
    pub(crate) options: Options,
    pub(crate) marks: Marks,
    pub(crate) marked: Option<Range<usize>>,
    /// The text before an input method's composition began, while it runs.
    pub(crate) composing: Option<Snapshot>,
    pub(crate) metrics: Metrics,
    pub(crate) dragging: bool,
    /// The left press in progress, if any.
    pub(crate) pressed: Option<Press>,
    pub(crate) caret_on: bool,
    /// Blink runs only while focused, or unseen cursors redraw forever.
    focused: bool,
    pub(crate) chat_field: gpui::Entity<TextInput>,
    pub(crate) frame: Frame,
    /// Folds and brackets of the text as it stands, kept between frames.
    structure: RefCell<syntax::Structure>,
    blink_epoch: u64,
    _blink: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for CodeEditor {}

impl Focusable for CodeEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// `offset` clamped to `text` and moved back onto a character boundary.
fn floor_boundary(text: &str, offset: usize) -> usize {
    let mut at = offset.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Whether a mark the owner sent fits the text; one that does not is logged and dropped.
fn kept(fits: bool, what: &str) -> bool {
    if !fits {
        log::error!("code editor: {what} lies off the text; dropped");
    }
    fits
}

impl CodeEditor {
    pub fn new(text: impl Into<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle().tab_stop(true);
        let chat_field =
            cx.new(|cx| TextInput::new(window, cx).placeholder("Ask to change this code"));
        let subscriptions = vec![
            cx.on_focus(&focus, window, |editor, _, cx| {
                editor.focused = true;
                editor.restart_blink(cx);
            }),
            cx.on_blur(&focus, window, |editor, _, cx| {
                editor.focused = false;
                editor.dragging = false;
                editor._blink = None;
                cx.notify();
            }),
            cx.subscribe(&chat_field, |_, field, event, cx| {
                if *event != InputEvent::Submit {
                    return;
                }
                let prompt = field.read(cx).text().trim().to_string();
                if prompt.is_empty() {
                    return;
                }
                log::info!("code editor: asked {prompt:?}");
                field.update(cx, |field, cx| field.set_text("", cx));
                cx.emit(EditorEvent::Asked(prompt.into()));
            }),
        ];
        Self {
            buffer: Buffer::new(text),
            selections: vec![Selection::caret(0)],
            history: History::default(),
            focus,
            scroll: UniformListScrollHandle::new(),
            folded: BTreeSet::new(),
            options: Options {
                language: "Plain text".into(),
                numbers: LineNumbers::Absolute,
                cursor: CursorShape::Line,
                whitespace: false,
                guides: true,
                rulers: Vec::new(),
                rainbow: false,
                minimap: false,
                sticky: false,
                read_only: false,
                read_only_banner: true,
                tab: 4,
                hard_tabs: false,
                highlighter: None,
                breakpoints: true,
            },
            marks: Marks::default(),
            marked: None,
            composing: None,
            metrics: Metrics::default(),
            dragging: false,
            pressed: None,
            caret_on: true,
            focused: false,
            chat_field,
            frame: Frame::default(),
            structure: RefCell::default(),
            blink_epoch: 0,
            _blink: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn language(mut self, name: impl Into<SharedString>) -> Self {
        self.options.language = name.into();
        self
    }

    pub fn line_numbers(mut self, numbers: LineNumbers) -> Self {
        self.options.numbers = numbers;
        self
    }

    pub fn cursor_shape(mut self, shape: CursorShape) -> Self {
        self.options.cursor = shape;
        self
    }

    /// Dots for spaces and arrows for tabs.
    pub fn show_whitespace(mut self) -> Self {
        self.options.whitespace = true;
        self
    }

    pub fn hide_indent_guides(mut self) -> Self {
        self.options.guides = false;
        self
    }

    /// Thin lines at these columns.
    pub fn rulers(mut self, columns: impl IntoIterator<Item = usize>) -> Self {
        self.options.rulers = columns.into_iter().collect();
        self
    }

    /// Brackets colored by how deep they sit.
    pub fn rainbow_brackets(mut self) -> Self {
        self.options.rainbow = true;
        self
    }

    pub fn minimap(mut self) -> Self {
        self.options.minimap = true;
        self
    }

    /// The lines that open the blocks around the top line stay pinned above it.
    pub fn sticky_scroll(mut self) -> Self {
        self.options.sticky = true;
        self
    }

    pub fn read_only(mut self) -> Self {
        self.options.read_only = true;
        self
    }

    /// A read-only editor draws no banner of its own and offers no way to unlock it; its owner says why it is read-only.
    pub fn without_read_only_banner(mut self) -> Self {
        self.options.read_only_banner = false;
        self
    }

    /// Spaces a Tab inserts and an indent guide spans.
    pub fn tab_size(mut self, size: usize) -> Self {
        assert!(size > 0, "a tab spans at least one column");
        self.options.tab = size;
        self
    }

    /// Indents with a tab character in place of spaces: Tab, Shift-Tab, a selection's indent and the line after an opener.
    pub fn hard_tabs(mut self) -> Self {
        self.options.hard_tabs = true;
        self
    }

    /// Colors each line with the owner's highlighter in place of code's lexer. It is called with a line's index and the line's whole text, whatever part of the line is shown; its spans are byte offsets into that text.
    pub fn highlighter(
        mut self,
        highlight: impl Fn(usize, &str, &App) -> Vec<(Range<usize>, HighlightStyle)> + 'static,
    ) -> Self {
        self.options.highlighter = Some(LineHighlighter(Rc::new(highlight)));
        self
    }

    /// The gutter's breakpoint slot draws nothing and takes no press; diagnostics still show there.
    pub fn without_breakpoints(mut self) -> Self {
        self.options.breakpoints = false;
        self
    }

    pub fn text(&self) -> &str {
        self.buffer.text()
    }

    /// The findings the owner set, as they stand for the current text: an
    /// edit moves their ranges, and an undo, a reset or a `set_text` clears them.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.marks.diagnostics
    }

    pub fn selections(&self) -> &[Selection] {
        &self.selections
    }

    /// The last cursor placed, which scrolling follows.
    pub fn primary(&self) -> Selection {
        *self.selections.last().expect("an editor keeps a cursor")
    }

    /// Line and column of the primary cursor, both from one.
    pub fn position(&self) -> (usize, usize) {
        let (line, column) = self.buffer.point(self.primary().head);
        (line + 1, column + 1)
    }

    pub fn language_name(&self) -> &SharedString {
        &self.options.language
    }

    /// The line the inline chat sits under, while it is open.
    pub fn chat_line(&self) -> Option<usize> {
        self.marks.chat
    }

    pub fn is_read_only(&self) -> bool {
        self.options.read_only
    }

    /// Whether the editor draws its own read-only banner: read-only, and not
    /// built `without_read_only_banner`. The render asks the same question.
    pub fn draws_read_only_banner(&self) -> bool {
        self.options.read_only && self.options.read_only_banner
    }

    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        let before = self.snapshot();
        self.buffer = Buffer::new(text);
        self.selections = vec![Selection::caret(0)];
        self.folded.clear();
        (self.marked, self.composing) = (None, None);
        anchors::clear(&mut self.marks);
        self.commit(before, false, cx);
    }

    /// New text with no history: undo starts here, with the caret at the start and no folds.
    pub fn reset(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.buffer = Buffer::new(text);
        self.selections = vec![Selection::caret(0)];
        self.history = History::default();
        self.folded.clear();
        (self.marked, self.composing) = (None, None);
        anchors::clear(&mut self.marks);
        cx.emit(EditorEvent::Changed);
        self.restart_blink(cx);
    }

    /// New text like `reset`, but the view stays where it is and each cursor keeps its place: both ends are clamped to the new text and moved back onto a character, and cursors that now share an offset are joined. History and folds are cleared, as `reset` clears them. For a change the owner makes on its own (a reload, a disk adoption), so the reader is not moved.
    pub fn reset_keeping_view(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.buffer = Buffer::new(text);
        let text = self.buffer.text();
        let kept: Vec<Selection> = self
            .selections
            .iter()
            .map(|selection| Selection {
                anchor: floor_boundary(text, selection.anchor),
                head: floor_boundary(text, selection.head),
                goal: None,
            })
            .collect();
        self.selections = merged(kept);
        self.history = History::default();
        self.folded.clear();
        (self.marked, self.composing) = (None, None);
        anchors::clear(&mut self.marks);
        cx.emit(EditorEvent::Changed);
        self.restart_blink(cx);
    }

    /// Replaces the owner's colors for each line, in place of code's lexer; see `highlighter` for what it is called with.
    pub fn set_highlighter(
        &mut self,
        highlight: impl Fn(usize, &str, &App) -> Vec<(Range<usize>, HighlightStyle)> + 'static,
        cx: &mut Context<Self>,
    ) {
        self.options.highlighter = Some(LineHighlighter(Rc::new(highlight)));
        cx.notify();
    }

    /// Whether the editor indents with a tab character in place of spaces.
    pub fn uses_hard_tabs(&self) -> bool {
        self.options.hard_tabs
    }

    /// Chooses between a tab character and spaces for what the editor indents with; the text already there is left as it is.
    pub fn set_hard_tabs(&mut self, hard_tabs: bool, cx: &mut Context<Self>) {
        if self.options.hard_tabs != hard_tabs {
            self.options.hard_tabs = hard_tabs;
            cx.notify();
        }
    }

    /// Sets the view settings together. A tab size of zero is raised to one and logged.
    pub fn set_view_options(&mut self, options: ViewOptions, cx: &mut Context<Self>) {
        if options.tab_size == 0 {
            log::error!("code editor: a tab of zero columns; one used");
        }
        self.options.minimap = options.minimap;
        self.options.sticky = options.sticky_scroll;
        self.options.guides = options.indent_guides;
        self.options.rainbow = options.rainbow_brackets;
        self.options.whitespace = options.whitespace;
        self.options.rulers = options.rulers;
        self.options.tab = options.tab_size.max(1);
        self.options.numbers = options.line_numbers;
        self.options.cursor = options.cursor_shape;
        cx.notify();
    }

    /// The view settings in force.
    pub fn view_options(&self) -> ViewOptions {
        ViewOptions {
            minimap: self.options.minimap,
            sticky_scroll: self.options.sticky,
            indent_guides: self.options.guides,
            rainbow_brackets: self.options.rainbow,
            whitespace: self.options.whitespace,
            rulers: self.options.rulers.clone(),
            tab_size: self.options.tab,
            line_numbers: self.options.numbers,
            cursor_shape: self.options.cursor,
        }
    }

    pub fn set_cursor_shape(&mut self, shape: CursorShape, cx: &mut Context<Self>) {
        self.options.cursor = shape;
        cx.notify();
    }

    pub fn set_line_numbers(&mut self, numbers: LineNumbers, cx: &mut Context<Self>) {
        self.options.numbers = numbers;
        cx.notify();
    }

    pub fn set_read_only(&mut self, read_only: bool, cx: &mut Context<Self>) {
        log::info!("code editor: read only {read_only}");
        self.options.read_only = read_only;
        cx.notify();
    }

    pub fn set_diagnostics(&mut self, mut diagnostics: Vec<Diagnostic>, cx: &mut Context<Self>) {
        let text = self.buffer.text();
        diagnostics
            .retain(|diagnostic| kept(anchors::fits(text, &diagnostic.range), "a diagnostic"));
        if self.marks.diagnostics == diagnostics {
            return;
        }
        self.marks.diagnostics = diagnostics;
        self.marks.reindex();
        cx.notify();
    }

    pub fn set_backgrounds(
        &mut self,
        mut ranges: Vec<(Range<usize>, gpui::Hsla)>,
        cx: &mut Context<Self>,
    ) {
        let text = self.buffer.text();
        ranges.retain(|(range, _)| kept(anchors::fits(text, range), "a background"));
        if self.marks.backgrounds == ranges {
            return;
        }
        self.marks.backgrounds = ranges;
        self.marks.reindex();
        cx.notify();
    }

    pub fn set_inlay_hints(&mut self, mut hints: Vec<InlayHint>, cx: &mut Context<Self>) {
        let text = self.buffer.text();
        hints.retain(|hint| kept(text.is_char_boundary(hint.offset), "an inlay hint"));
        self.marks.hints = hints;
        self.marks.hints.sort_by_key(|hint| hint.offset);
        cx.notify();
    }

    pub fn set_code_lenses(&mut self, lenses: Vec<CodeLens>, cx: &mut Context<Self>) {
        self.marks.lenses = lenses;
        cx.notify();
    }

    /// Suggested text at an offset, in grey until Tab takes it.
    pub fn set_ghost_text(&mut self, ghost: Option<GhostText>, cx: &mut Context<Self>) {
        let text = self.buffer.text();
        self.marks.ghost =
            ghost.filter(|ghost| kept(text.is_char_boundary(ghost.offset), "ghost text"));
        cx.notify();
    }

    pub fn set_diff(&mut self, hunks: Vec<DiffHunk>, cx: &mut Context<Self>) {
        self.marks.hunks = hunks;
        cx.notify();
    }

    pub fn set_git_marks(&mut self, marks: Vec<(usize, GitMark)>, cx: &mut Context<Self>) {
        self.marks.git = marks;
        cx.notify();
    }

    pub fn toggle_breakpoint(&mut self, line: usize, cx: &mut Context<Self>) {
        if !self.marks.breakpoints.remove(&line) {
            self.marks.breakpoints.insert(line);
        }
        log::info!("code editor: breakpoints {:?}", self.marks.breakpoints);
        cx.notify();
    }

    pub fn breakpoints(&self) -> &BTreeSet<usize> {
        &self.marks.breakpoints
    }

    /// Opens the inline chat below a line, or closes it.
    pub fn set_chat(&mut self, line: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        self.marks.chat = line;
        if line.is_some() {
            let focus = self.chat_field.read(cx).focus().clone();
            window.focus(&focus, cx);
        } else {
            window.focus(&self.focus, cx);
            cx.emit(EditorEvent::ChatClosed);
        }
        cx.notify();
    }

    pub(crate) fn set_selections(&mut self, selections: Vec<Selection>, cx: &mut Context<Self>) {
        self.selections = merged(selections);
        self.unfold_cursors();
        self.restart_blink(cx);
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.buffer.text().to_string(),
            selections: self.selections.clone(),
        }
    }

    pub(crate) fn restore(&mut self, snapshot: Snapshot, cx: &mut Context<Self>) {
        self.buffer = Buffer::new(snapshot.text);
        self.selections = snapshot.selections;
        self.marked = None;
        self.composing = None;
        anchors::clear(&mut self.marks);
        cx.emit(EditorEvent::Changed);
        self.restart_blink(cx);
    }

    /// Makes `before` one undo step when the text moved on from it.
    pub(crate) fn commit(&mut self, before: Snapshot, typing: bool, cx: &mut Context<Self>) {
        if before.text != self.buffer.text() {
            if self.composing.is_none() {
                self.history.record(before, typing);
            }
            self.marks.ghost = None;
            self.folded.retain(|line| *line < self.buffer.lines());
            cx.emit(EditorEvent::Changed);
        }
        self.restart_blink(cx);
    }

    /// Shows the cursors now and restarts their blink; still under reduced motion.
    pub(crate) fn restart_blink(&mut self, cx: &mut Context<Self>) {
        self.caret_on = true;
        self.blink_epoch += 1;
        let epoch = self.blink_epoch;
        let still = cx.theme().reduced_motion;
        self._blink = (!still && self.focused).then(|| {
            cx.spawn(async move |editor, cx| {
                loop {
                    cx.background_executor().timer(BLINK).await;
                    let alive = editor.update(cx, |editor, cx| {
                        if editor.blink_epoch == epoch {
                            editor.caret_on = !editor.caret_on;
                            cx.notify();
                        }
                    });
                    if alive.is_err() {
                        return;
                    }
                }
            })
        });
        cx.notify();
    }

    /// The blocks the text opens, read once per revision of the text.
    pub(crate) fn folds(&self) -> Rc<Vec<(usize, usize)>> {
        self.structure.borrow_mut().folds(&self.buffer)
    }

    /// The brackets of the text, read once per revision of the text and, after an edit, only on the lines it touched.
    pub(crate) fn brackets(&self) -> Rc<Vec<syntax::Bracket>> {
        self.structure.borrow_mut().brackets(&self.buffer)
    }

    /// The headers of the blocks that hold `line`, outermost first, from an index built once per revision of the text.
    pub(crate) fn enclosing(&self, line: usize) -> Vec<usize> {
        self.structure.borrow_mut().enclosing(&self.buffer, line)
    }

    pub(crate) fn editable(&self) -> bool {
        !self.options.read_only
    }

    /// Colors for `piece`, the part of `line` that starts `from` bytes into it. With the owner's highlighter, it is called with the line's index and the line's whole text, and the spans are cut to the piece; without one, code's lexer reads the piece as it is, as upstream does. The owner's spans past the line's text are clipped, and one that cuts a character is dropped; either is logged.
    pub(crate) fn piece_colors(
        &self,
        line: usize,
        from: usize,
        piece: &str,
        cx: &App,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let Some(LineHighlighter(highlight)) = &self.options.highlighter else {
            return syntax::colors(piece, cx);
        };
        let whole = self.buffer.line(line);
        let mut off = 0;
        let spans: Vec<_> = highlight(line, whole, cx)
            .into_iter()
            .filter_map(|(span, style)| {
                if span.start > span.end || span.end > whole.len() {
                    off += 1;
                }
                let start = span.start.min(whole.len());
                let end = span.end.min(whole.len());
                if start >= end {
                    return None;
                }
                if !whole.is_char_boundary(start) || !whole.is_char_boundary(end) {
                    off += 1;
                    return None;
                }
                // The part of the span the piece shows, in the piece's own offsets.
                let start = start.max(from);
                let end = end.min(from + piece.len());
                (start < end).then(|| (start - from..end - from, style))
            })
            .collect();
        if off > 0 {
            log::error!(
                "code editor: {off} highlights of line {line} do not fit its text; clipped or dropped"
            );
        }
        spans
    }
}
