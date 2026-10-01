//! Text that can be selected with the mouse and copied.
//!
//! gpui paints a `&str`, `String` or `SharedString` child and does nothing
//! else with it: no hitbox, no mouse handlers, no id. So nothing drawn that
//! way can be selected, and until this module every surface that wanted a
//! selection built its own — the chat transcript, the file editor, the
//! terminal. Everything else in the app (Settings, the change request tab,
//! banners, dialogs, the diff) was painted and inert. Neither bezel nor gpui
//! ships the missing piece, so [`SelectableText`] is it: a drop-in for a text
//! child that lays out and paints exactly like one, and additionally takes a
//! drag, a double-click (a token) and a triple-click (the whole line).
//!
//! One selection exists at a time, held in a [`Global`]; it belongs to the
//! run that made it and ends when another run starts one, when the pointer
//! presses anywhere else, or when focus leaves the workspace's sink.
//!
//! **Where it must not be used.** A press inside a clickable row fires the
//! row's click on release even after a drag, so text inside a button, a
//! sidebar row, a tab or a palette entry stays plain. The same goes for the
//! window's drag region. Selection here is per line of text; it does not
//! extend across two separate runs.
//!
//! **Copy.** Ctrl/Cmd+C is bound to [`CopySelectedText`] outside a terminal.
//! gpui dispatches a bound action *before* any raw `on_key_down`, and stops
//! there unless the handler propagates — so [`on_copy`] declines (propagates)
//! whenever it has nothing to copy or the workspace's [`set_sink`] handle is
//! not what holds focus. A terminal that is focused therefore still reads
//! Ctrl+C as SIGINT, whatever was selected before it took focus.
//!
//! The host wires three things: [`init`] once, [`set_sink`] with the handle
//! of its root, and `.on_action(text_selection::on_copy)` on that root.

use crate::chat::paint_wrapped_span;
use gpui::{
    App, Bounds, ClipboardItem, CursorStyle, DispatchPhase, Element, ElementId, EntityId,
    FocusHandle, Global, GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId,
    IntoElement, KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, SharedString, StyledText, TextLayout, Window, accesskit, actions,
};
use sirio_theme::Theme;
use std::{
    hash::{Hash, Hasher},
    ops::Range,
    panic::Location,
};

actions!(text_selection, [CopySelectedText]);

/// Binds Ctrl/Cmd+C to [`CopySelectedText`] everywhere but a terminal.
pub fn init(cx: &mut App) {
    cx.default_global::<TextSelection>();
    cx.bind_keys([
        KeyBinding::new("ctrl-c", CopySelectedText, Some("!Terminal")),
        KeyBinding::new("cmd-c", CopySelectedText, Some("!Terminal")),
    ]);
}

/// Registers the handle that takes keyboard focus when a selection is made,
/// and the only focus [`on_copy`] will answer to. It is the workspace root's
/// own handle: nothing types into it, so it can hold focus while text is
/// selected without swallowing anything.
pub fn set_sink(sink: FocusHandle, cx: &mut App) {
    cx.default_global::<TextSelection>().sink = Some(sink);
}

/// The workspace root's handler for [`CopySelectedText`].
pub fn on_copy(_: &CopySelectedText, window: &mut Window, cx: &mut App) {
    match selected_text(window, cx) {
        Some(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
        // Not ours to handle. An action stops propagating by default, which
        // would deny the raw key to whatever is focused — a terminal's
        // Ctrl+C is SIGINT.
        None => cx.propagate(),
    }
}

fn selected_text(window: &Window, cx: &App) -> Option<String> {
    let state = cx.try_global::<TextSelection>()?;
    if !state.sink.as_ref()?.is_focused(window) {
        return None;
    }
    let active = state.active.as_ref()?;
    active.text.get(active.range()).map(str::to_owned)
}

#[derive(Default)]
struct TextSelection {
    sink: Option<FocusHandle>,
    active: Option<Active>,
}

impl Global for TextSelection {}

/// Identifies one run of text. The text is part of the key so two runs a
/// list builds from one call site — same parent, same id — cannot claim each
/// other's selection.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct RunKey {
    id: GlobalElementId,
    text: u64,
}

impl RunKey {
    pub(crate) fn new(id: Option<&GlobalElementId>, text: &str) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        Self {
            id: id.cloned().unwrap_or_default(),
            text: hasher.finish(),
        }
    }
}

pub(crate) struct Active {
    pub(crate) run: RunKey,
    /// The run's text when the selection began: what Copy reads, so a label
    /// that later changes can neither be sliced by a stale range nor leak
    /// characters that were never on screen.
    pub(crate) text: SharedString,
    pub(crate) anchor: usize,
    pub(crate) head: usize,
    pub(crate) dragging: bool,
    /// The view that paints the run, told to repaint whenever this changes.
    pub(crate) view: EntityId,
}

impl Active {
    /// The selected byte range, ordered and snapped to character boundaries.
    fn range(&self) -> Range<usize> {
        let (low, high) = if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        };
        floor_char_boundary(&self.text, low)..floor_char_boundary(&self.text, high)
    }
}

fn floor_char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

pub(crate) fn owns(run: &RunKey, cx: &App) -> bool {
    cx.try_global::<TextSelection>()
        .and_then(|state| state.active.as_ref())
        .is_some_and(|active| &active.run == run)
}

pub(crate) fn begin(active: Active, cx: &mut App) {
    let view = active.view;
    let previous = cx
        .default_global::<TextSelection>()
        .active
        .replace(active)
        .map(|previous| previous.view);
    if let Some(previous) = previous
        && previous != view
    {
        cx.notify(previous);
    }
    cx.notify(view);
}

pub(crate) fn release(run: &RunKey, cx: &mut App) {
    if !owns(run, cx) {
        return;
    }
    if let Some(active) = cx.default_global::<TextSelection>().active.take() {
        cx.notify(active.view);
    }
}

/// The span `run` has selected, while it still shows `text`. `None` when the
/// run owns nothing, or an empty span.
pub(crate) fn selected_range(run: &RunKey, text: &str, cx: &App) -> Option<Range<usize>> {
    let active = cx
        .try_global::<TextSelection>()?
        .active
        .as_ref()
        .filter(|active| &active.run == run && active.text == text)?;
    let range = active.range();
    (!range.is_empty()).then_some(range)
}

/// Whether `run` owns a selection whose drag is still in progress.
pub(crate) fn dragging(run: &RunKey, cx: &App) -> bool {
    cx.try_global::<TextSelection>()
        .and_then(|state| state.active.as_ref())
        .is_some_and(|active| &active.run == run && active.dragging)
}

/// Moves the head of `run`'s selection. Focus moves into the sink only once
/// something is actually selected, so a plain click on text leaves a focused
/// terminal or composer exactly as it was.
pub(crate) fn extend(run: &RunKey, head: usize, view: EntityId, window: &mut Window, cx: &mut App) {
    let selecting = cx
        .default_global::<TextSelection>()
        .active
        .as_mut()
        .filter(|active| &active.run == run)
        .map(|active| {
            active.head = head;
            !active.range().is_empty()
        })
        .unwrap_or(false);
    if selecting {
        focus_sink(window, cx);
    }
    cx.notify(view);
}

/// Ends `run`'s drag. `None` when it was not the owner; otherwise whether a
/// selection remains — a press and release with nothing selected between
/// them was a click, and leaves nothing behind.
pub(crate) fn finish(run: &RunKey, view: EntityId, cx: &mut App) -> Option<bool> {
    if !owns(run, cx) {
        return None;
    }
    let state = cx.default_global::<TextSelection>();
    let remains = match state.active.as_mut() {
        Some(active) => {
            active.dragging = false;
            !active.range().is_empty()
        }
        None => false,
    };
    if !remains {
        state.active = None;
    }
    cx.notify(view);
    Some(remains)
}

pub(crate) fn focus_sink(window: &mut Window, cx: &mut App) {
    let Some(sink) = cx
        .try_global::<TextSelection>()
        .and_then(|state| state.sink.clone())
    else {
        return;
    };
    if !sink.is_focused(window) {
        window.focus(&sink, cx);
    }
}

/// The byte offset of the glyph under the pointer, and whether the pointer
/// is on the text at all (off it, the offset clamps to an end).
///
/// What a double-click means by "the word here": the pointer's own glyph, not
/// the nearest caret, or the right half of a word's last letter would take
/// the separator after it. A line clipped with an ellipsis answers with the
/// end of `text` for anywhere on or past the "…" — see [`caret_index`].
fn hit_index(layout: &TextLayout, position: Point<Pixels>, text: &str) -> (usize, bool) {
    let hit = layout.index_for_position(position);
    let raw = match hit {
        Ok(index) | Err(index) => index,
    };
    let shown = layout.text();
    if shown != text
        && let Some(visible) = shown.strip_suffix('…')
        && raw >= visible.len()
    {
        return (text.len(), false);
    }
    (floor_char_boundary(text, raw), hit.is_ok())
}

/// The byte offset of the caret position nearest the pointer.
///
/// gpui's `index_for_position` is a hit test: it names the glyph *under* the
/// pointer. A caret sits between glyphs, so the pointer's half of that glyph
/// decides which edge it belongs to — otherwise the last character dragged
/// over would never be included.
///
/// A line clipped with an ellipsis lays out — and answers in offsets of —
/// only its visible prefix and the "…". The prefix shares its offsets with
/// `text`, so those need no translation; a caret on or past the ellipsis
/// means "and the rest" and reads as the end of `text`, which is what
/// copying a truncated title or path is expected to give.
fn caret_index(layout: &TextLayout, position: Point<Pixels>, text: &str) -> usize {
    let (index, inside) = hit_index(layout, position, text);
    if !inside {
        return index;
    }
    let Some(next) = text[index..]
        .chars()
        .next()
        .map(|character| index + character.len_utf8())
    else {
        return index;
    };
    match (layout.position_for_index(index), layout.position_for_index(next)) {
        (Some(left), Some(right))
            if left.y == right.y && position.x - left.x > right.x - position.x =>
        {
            next
        }
        _ => index,
    }
}

#[derive(PartialEq)]
enum CharClass {
    /// Letters, digits and what one segment of a path, a branch, a URL or a
    /// version is made of: a double-click takes all of it. `/` and `\` are
    /// not part of it — they separate segments, so `feat/foo-bar` is two.
    Token,
    Space,
    /// Anything else selects the run of that one character.
    Other(char),
}

fn is_token_char(character: char) -> bool {
    character.is_alphanumeric()
        || matches!(
            character,
            '_' | '-' | '.' | ':' | '@' | '~' | '#' | '%' | '+' | '='
        )
}

fn classify(character: char) -> CharClass {
    if is_token_char(character) {
        CharClass::Token
    } else if character.is_whitespace() {
        CharClass::Space
    } else {
        CharClass::Other(character)
    }
}

/// The span a double-click at byte `index` selects.
pub(crate) fn word_range(text: &str, index: usize) -> Range<usize> {
    if text.is_empty() {
        return 0..0;
    }
    let index = floor_char_boundary(text, index);
    // Past the last character the click meant the last one.
    let (at, under) = match text[index..].chars().next() {
        Some(character) => (index, character),
        None => {
            let character = text[..index]
                .chars()
                .next_back()
                .expect("text is not empty");
            (index - character.len_utf8(), character)
        }
    };
    let class = classify(under);
    let mut start = at;
    let mut end = at + under.len_utf8();
    while let Some(previous) = text[..start].chars().next_back()
        && classify(previous) == class
    {
        start -= previous.len_utf8();
    }
    while let Some(next) = text[end..].chars().next()
        && classify(next) == class
    {
        end += next.len_utf8();
    }
    if class == CharClass::Token {
        // "see control.sock." ends a sentence; the dot is not the file's.
        while end - start > 1
            && text[start..end]
                .chars()
                .next_back()
                .is_some_and(|last| matches!(last, '.' | ':'))
        {
            end -= 1;
        }
    }
    start..end
}

/// Text that can be selected and copied; see the module documentation.
///
/// Lays out exactly like the plain string it stands in for, so swapping it
/// in cannot move anything, and inherits the surrounding text style the
/// same way.
pub struct SelectableText {
    id: ElementId,
    text: SharedString,
    styled: StyledText,
}

/// A selectable line of text. Its identity defaults to the call site, which
/// is unique unless the call sits in a loop: there, give each item its own
/// with [`SelectableText::id`].
#[track_caller]
pub fn selectable_text(text: impl Into<SharedString>) -> SelectableText {
    let text = text.into();
    SelectableText {
        id: ElementId::CodeLocation(*Location::caller()),
        styled: StyledText::new(text.clone()),
        text,
    }
}

impl SelectableText {
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }
}

impl IntoElement for SelectableText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SelectableText {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    // The label role gpui's own `Text` element gives the `text!` macro, so
    // a label swapped to this element stays visible to assistive technology.
    fn a11y_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Label)
    }

    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        node.set_value(self.text.to_string());
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.styled.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.styled
            .prepaint(id, inspector_id, bounds, state, window, cx);
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let run = RunKey::new(id, &self.text);
        let layout = self.styled.layout().clone();

        if let Some(range) = selected_range(&run, &self.text, cx) {
            let fill = Theme::get(cx).ely.active;
            paint_wrapped_span(&layout, bounds, range, fill, window, |_| {});
        }
        window.set_cursor_style(CursorStyle::IBeam, hitbox);
        self.listen(run, layout, hitbox.clone(), window);

        self.styled
            .paint(id, inspector_id, bounds, state, &mut (), window, cx);
    }
}

impl SelectableText {
    fn listen(&self, run: RunKey, layout: TextLayout, hitbox: Hitbox, window: &mut Window) {
        let view = window.current_view();

        // A press. Capture: a press anywhere else ends this run's selection
        // even when something on top of the pointer stops the event. Bubble:
        // a press on this run begins one.
        let (down_run, down_layout, down_text) = (run.clone(), layout.clone(), self.text.clone());
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if event.button != MouseButton::Left {
                return;
            }
            let hovered = hitbox.is_hovered(window);
            match phase {
                DispatchPhase::Capture if !hovered => release(&down_run, cx),
                DispatchPhase::Bubble if hovered => {
                    let (anchor, head) = match event.click_count {
                        0 | 1 => {
                            let index = caret_index(&down_layout, event.position, &down_text);
                            (index, index)
                        }
                        2 => {
                            let (index, _) = hit_index(&down_layout, event.position, &down_text);
                            let word = word_range(&down_text, index);
                            (word.start, word.end)
                        }
                        _ => (0, down_text.len()),
                    };
                    begin(
                        Active {
                            run: down_run.clone(),
                            text: down_text.clone(),
                            anchor,
                            head,
                            dragging: true,
                            view,
                        },
                        cx,
                    );
                    if event.click_count >= 2 {
                        // A word or a line is a selection already. Taking
                        // the press keeps a focusable ancestor from
                        // claiming focus back after the sink took it.
                        focus_sink(window, cx);
                        window.prevent_default();
                    }
                }
                _ => {}
            }
        });

        // The drag. It follows the pointer wherever it goes, not only over
        // the run: a short line is easy to overshoot, and the caret clamps
        // to its end.
        let (move_run, move_layout, move_text) = (run.clone(), layout, self.text.clone());
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Capture || !event.dragging() {
                return;
            }
            if !dragging(&move_run, cx) {
                return;
            }
            let head = caret_index(&move_layout, event.position, &move_text);
            extend(&move_run, head, view, window, cx);
        });

        // The release ends the drag; a press-and-release with no drag between
        // them selected nothing and leaves no selection behind.
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
            if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                return;
            }
            finish(&run, view, cx);
        });
    }
}

// The ways this can fail, written before the code that has to survive them.
//
// Selection itself
//   1. A drag selects the wrong span: bytes taken for characters, so accented
//      text slices inside a code point, or a right-to-left drag copies the
//      span backwards.
//   2. A drag that leaves a short line (the pointer runs past its end) stops
//      following the pointer instead of clamping to the end.
//   3. Double-click selects a fragment of a path or branch name instead of the
//      token; triple-click selects less than the whole line.
//   4. A selection starts through something drawn on top of the text (a modal
//      backdrop), reaching text the user cannot see.
//
// Ownership
//   5. Two lines end up highlighted at once, or Copy returns the older one.
//   6. A selection survives a click elsewhere and Copy still returns it.
//   7. The text under a selection changes: a stale range slices the new text
//      (panic) or copies characters that were never on screen.
//
// Keyboard — the one that can hurt
//   8. Ctrl+C with nothing selected is swallowed: an action handler stops
//      propagation by default, so a terminal that is focused never sees the
//      raw key and SIGINT stops working.
//   9. A stale selection outlives the focus that made it: the user selects a
//      label, goes back to the terminal, presses Ctrl+C and gets a copy
//      instead of an interrupt.
//
// Layout
//  10. Swapping a plain string for the selectable element changes its size,
//      wrapping or truncation, so every surface it is applied to shifts.
#[cfg(test)]
pub(crate) mod testing {
    //! A stand-in for the workspace root and the gestures that drive it, for
    //! any surface's test that wants to prove its text can be selected and
    //! copied: host the view under [`Root`], then [`copy_line`].

    use super::*;
    use gpui::{
        AnyView, AppContext as _, ClipboardItem, Context, Entity, FocusHandle, InteractiveElement,
        IntoElement, KeyDownEvent, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent,
        ParentElement, Pixels, Point, Render, Styled, TestAppContext, VisualTestContext, div,
        point, px,
    };
    use sirio_theme::Theme;
    use std::{cell::RefCell, rc::Rc};

    pub(crate) const SENTINEL: &str = "clipboard untouched";

    /// A focusable surface that reads raw keys, and what it was handed.
    #[derive(Clone)]
    pub(crate) struct Probe {
        pub(crate) focus: FocusHandle,
        pub(crate) keys: Rc<RefCell<Vec<String>>>,
    }

    /// Stands in for the workspace root: the "Workspace" key context, the
    /// focus sink the host registers, the app-level copy handler — plus a
    /// "Terminal" and an "Editor" probe that record the raw keys they are
    /// handed, the way the real terminal reads Ctrl+C as SIGINT and the file
    /// editor handles its own Copy.
    pub(crate) struct Root {
        sink: FocusHandle,
        pub(crate) terminal: Probe,
        pub(crate) editor: Probe,
        body: AnyView,
    }

    impl Root {
        pub(crate) fn new(body: AnyView, cx: &mut Context<Self>) -> Self {
            let probe = |cx: &mut Context<Self>| Probe {
                focus: cx.focus_handle(),
                keys: Rc::default(),
            };
            Self {
                sink: cx.focus_handle(),
                terminal: probe(cx),
                editor: probe(cx),
                body,
            }
        }
    }

    impl Render for Root {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            set_sink(self.sink.clone(), cx);
            div()
                .size_full()
                .key_context("Workspace")
                .track_focus(&self.sink)
                .on_action(on_copy)
                .child(self.body.clone())
                .child(self.probe("terminal-probe", "Terminal", &self.terminal, 0.0, cx))
                .child(self.probe("editor-probe", "Editor", &self.editor, 20.0, cx))
        }
    }

    impl Root {
        /// A 10px focusable square that records the raw keys it is handed —
        /// the way the real terminal reads Ctrl+C as SIGINT and the file
        /// editor handles its own Copy on a raw key.
        fn probe(
            &self,
            id: &'static str,
            context: &'static str,
            probe: &Probe,
            offset: f32,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let keys = probe.keys.clone();
            div()
                .id(id)
                .absolute()
                .bottom_0()
                .right(px(offset))
                .size(px(10.0))
                .key_context(context)
                .track_focus(&probe.focus)
                .on_key_down(cx.listener(move |_, event: &KeyDownEvent, _, _| {
                    keys.borrow_mut().push(event.keystroke.unparse());
                }))
        }
    }

    pub(crate) fn boot(cx: &mut TestAppContext) {
        cx.update(|cx| {
            Theme::init(cx);
            init(cx);
        });
    }

    pub(crate) fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
        cx.read_from_clipboard().and_then(|item| item.text())
    }

    pub(crate) fn reset_clipboard(cx: &mut VisualTestContext) {
        cx.write_to_clipboard(ClipboardItem::new_string(SENTINEL.to_string()));
    }

    pub(crate) fn drag(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        let middle = point((from.x + to.x) / 2.0, (from.y + to.y) / 2.0);
        for position in [middle, to] {
            cx.simulate_mouse_move(position, MouseButton::Left, Modifiers::none());
        }
        cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    }

    pub(crate) fn press(cx: &mut VisualTestContext, at: Point<Pixels>, click_count: usize) {
        cx.simulate_event(MouseDownEvent {
            position: at,
            modifiers: Modifiers::none(),
            button: MouseButton::Left,
            click_count,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: at,
            modifiers: Modifiers::none(),
            button: MouseButton::Left,
            click_count,
        });
    }

    /// Hosts the view `build` makes under a [`Root`], with the theme and the
    /// key binding installed, and hands back the view and the two probes.
    pub(crate) fn host<V: Render + 'static>(
        cx: &mut TestAppContext,
        build: impl FnOnce(&mut gpui::Window, &mut Context<V>) -> V,
    ) -> (Entity<V>, (Probe, Probe), &mut VisualTestContext) {
        boot(cx);
        let mut view = None;
        let mut probes = None;
        let (_, vcx) = cx.add_window_view(|window, cx| {
            let body = cx.new(|cx| build(window, cx));
            view = Some(body.clone());
            let root = Root::new(body.into(), cx);
            probes = Some((root.terminal.clone(), root.editor.clone()));
            root
        });
        (view.unwrap(), probes.unwrap(), vcx)
    }

    /// Like [`copy_line`] for a block of text with padding: presses `inset`
    /// inside its top-left corner, releases just inside its bottom-right, and
    /// returns what Ctrl+C then copies.
    ///
    /// The release stays *inside* the block because the chat transcript's own
    /// selection only follows a drag while the pointer is over its text.
    pub(crate) fn copy_span(
        cx: &mut VisualTestContext,
        selector: &'static str,
        inset: Pixels,
    ) -> Option<String> {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("no element with debug selector {selector:?} is drawn"));
        reset_clipboard(cx);
        drag(
            cx,
            point(bounds.left() + inset, bounds.top() + inset),
            point(bounds.right() - px(1.0), bounds.bottom() - px(1.0)),
        );
        cx.simulate_keystrokes("ctrl-c");
        clipboard(cx)
    }

    /// Drags across the whole element `selector` — from just inside its left
    /// edge to well past its right end, on its middle line — then presses
    /// Ctrl+C, and returns what the clipboard holds. The clipboard is set to
    /// [`SENTINEL`] first, so a selection that never happened reads as
    /// `Some(SENTINEL)` rather than as whatever an earlier test left there.
    pub(crate) fn copy_line(cx: &mut VisualTestContext, selector: &'static str) -> Option<String> {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("no element with debug selector {selector:?} is drawn"));
        let y = bounds.center().y;
        reset_clipboard(cx);
        drag(
            cx,
            point(bounds.left() + px(1.0), y),
            point(bounds.right() + px(200.0), y),
        );
        cx.simulate_keystrokes("ctrl-c");
        clipboard(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::settings::{Settings, SettingsSnapshot};
    use gpui::{
        AppContext as _, Context, InteractiveElement, IntoElement, Modifiers, ParentElement, Pixels,
        Point, Render, Styled, TestAppContext, VisualTestContext, div, point,
        prelude::FluentBuilder as _, px, size,
    };

    /// One selectable line in a box of known width, optionally covered by a
    /// modal-style backdrop, next to a plain-string twin for layout parity.
    struct Lab {
        text: SharedString,
        width: Pixels,
        covered: bool,
        /// Clip the line to its box with an ellipsis, as a title does.
        truncated: bool,
    }

    impl Render for Lab {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut Context<Self>,
        ) -> impl IntoElement {
            div()
                .size_full()
                .child(
                    div()
                        .debug_selector(|| "lab-line".into())
                        .w(self.width)
                        .when(self.truncated, |this| {
                            this.overflow_hidden().text_ellipsis().whitespace_nowrap()
                        })
                        .child(selectable_text(self.text.clone())),
                )
                .child(
                    div()
                        .debug_selector(|| "lab-plain".into())
                        .mt(px(40.0))
                        .w(self.width)
                        .child(self.text.clone()),
                )
                // A flex item takes its content's width, so this one reads
                // the text's own width — and with it one character's.
                .child(
                    div().mt(px(40.0)).flex().flex_row().child(
                        div()
                            .debug_selector(|| "lab-measure".into())
                            .child(self.text.clone()),
                    ),
                )
                .children(self.covered.then(|| {
                    div()
                        .id("lab-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .occlude()
                }))
        }
    }

    fn drag_chars(cx: &mut VisualTestContext, text: &str, from: f32, to: f32) {
        let (from, to) = (line_point(cx, from, text), line_point(cx, to, text));
        drag(cx, from, to);
    }

    fn press_chars(cx: &mut VisualTestContext, text: &str, at: f32, click_count: usize) {
        let at = line_point(cx, at, text);
        press(cx, at, click_count);
    }

    fn lab<'a>(
        cx: &'a mut TestAppContext,
        text: &str,
        width: f32,
    ) -> (gpui::Entity<Lab>, (Probe, Probe), &'a mut VisualTestContext) {
        boot(cx);
        let text: SharedString = text.to_string().into();
        let mut entity = None;
        let mut probes = None;
        let (_, vcx) = cx.add_window_view(|_, cx| {
            let lab = cx.new(|_| Lab {
                text,
                width: px(width),
                covered: false,
                truncated: false,
            });
            entity = Some(lab.clone());
            let root = Root::new(lab.into(), cx);
            probes = Some((root.terminal.clone(), root.editor.clone()));
            root
        });
        vcx.simulate_resize(size(px(800.0), px(400.0)));
        vcx.run_until_parked();
        (entity.unwrap(), probes.unwrap(), vcx)
    }

    /// Width of one character in the test platform's monospaced font.
    fn char_width(cx: &mut VisualTestContext, text: &str) -> f32 {
        let bounds = cx.debug_bounds("lab-measure").expect("measuring line renders");
        f32::from(bounds.size.width) / text.chars().count() as f32
    }

    /// The pointer position of the boundary `chars` characters into the line.
    fn line_point(cx: &mut VisualTestContext, chars: f32, text: &str) -> Point<Pixels> {
        let bounds = cx.debug_bounds("lab-line").expect("selectable line renders");
        point(
            bounds.left() + px(char_width(cx, text) * chars),
            bounds.center().y,
        )
    }

    // ----- word_range: the pure unit, proven on its own ------------------

    /// A path or a branch is a run of segments, and a double-click takes the
    /// one under the pointer: `feat` or `foo-bar`, not `feat/foo-bar` — while
    /// a hyphen, a dot or an underscore stays inside its segment.
    #[test]
    fn a_double_click_takes_one_segment_of_a_path_or_branch() {
        let text = "open feat/foo-bar now";
        let feat = text.find("feat").unwrap();
        let foo_bar = text.find("foo-bar").unwrap();
        for index in feat..feat + "feat".len() {
            assert_eq!(&text[word_range(text, index)], "feat", "index {index}");
        }
        for index in foo_bar..foo_bar + "foo-bar".len() {
            assert_eq!(&text[word_range(text, index)], "foo-bar", "index {index}");
        }
        let slash = text.find('/').unwrap();
        assert_eq!(&text[word_range(text, slash)], "/", "the separator itself");

        let path = "/run/user/1000/Sirio/control.sock";
        let file = path.find("control").unwrap();
        assert_eq!(&path[word_range(path, file + 3)], "control.sock");
        let windows = r"C:\Users\me\a_b.txt";
        let name = windows.find("a_b").unwrap();
        assert_eq!(&windows[word_range(windows, name + 1)], "a_b.txt");
    }

    #[test]
    fn a_word_never_swallows_the_punctuation_that_ends_a_sentence() {
        let text = "see control.sock.";
        assert_eq!(&text[word_range(text, 6)], "control.sock");
        let text = "path: /run/x";
        assert_eq!(&text[word_range(text, 2)], "path");
    }

    #[test]
    fn a_word_is_measured_in_bytes_but_moves_by_characters() {
        let text = "città è là";
        // Byte 5 is inside the two-byte `à`, not on a character boundary.
        let range = word_range(text, 5);
        assert_eq!(&text[range], "città");
        let accented = text.find('è').unwrap();
        assert_eq!(&text[word_range(text, accented)], "è");
    }

    #[test]
    fn a_word_at_the_ends_of_the_text_is_still_a_word() {
        let text = "alpha beta";
        assert_eq!(&text[word_range(text, text.len())], "beta");
        assert_eq!(&text[word_range(text, 0)], "alpha");
        assert_eq!(word_range("", 0), 0..0);
    }

    #[test]
    fn a_click_on_whitespace_takes_the_whitespace_not_a_neighbour() {
        let text = "a   b";
        assert_eq!(&text[word_range(text, 2)], "   ");
    }

    // ----- Selection through the real event path --------------------------

    #[gpui::test]
    async fn dragging_across_a_line_copies_exactly_the_span_between_the_two_points(
        cx: &mut TestAppContext,
    ) {
        let text = "alpha beta gamma";
        let (_, _, cx) = lab(cx, text, 400.0);
        reset_clipboard(cx);

        drag_chars(cx, text, 6.0, 10.0);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(clipboard(cx).as_deref(), Some("beta"));

        // The other direction reads the same span, not a reversed one.
        reset_clipboard(cx);
        drag_chars(cx, text, 10.0, 6.0);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(clipboard(cx).as_deref(), Some("beta"));
    }

    #[gpui::test]
    async fn a_caret_sits_on_the_nearer_edge_of_the_glyph_under_the_pointer(
        cx: &mut TestAppContext,
    ) {
        let text = "alpha beta gamma";
        let (_, _, cx) = lab(cx, text, 400.0);
        reset_clipboard(cx);

        // 5.9 is over the space but almost at the `b`'s left edge; 9.7 is
        // over the final `a` but almost at its right edge. A hit test would
        // read " bet"; a caret reads the word the user meant.
        drag_chars(cx, text, 5.9, 9.7);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(clipboard(cx).as_deref(), Some("beta"));
    }

    #[gpui::test]
    async fn a_drag_that_runs_past_the_end_of_a_short_line_clamps_to_it(
        cx: &mut TestAppContext,
    ) {
        let text = "alpha beta";
        let (_, _, cx) = lab(cx, text, 400.0);
        reset_clipboard(cx);

        let start = line_point(cx, 6.0, text);
        drag(cx, start, point(px(700.0), start.y));
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(clipboard(cx).as_deref(), Some("beta"));
    }

    /// A title clipped with an ellipsis shows a prefix and "…". Dragging over
    /// the visible letters copies those letters; dragging onto the ellipsis
    /// means "and the rest", the way copying a truncated table cell does.
    #[gpui::test]
    async fn a_clipped_line_copies_its_full_text_once_the_drag_reaches_the_ellipsis(
        cx: &mut TestAppContext,
    ) {
        let text = "alpha beta gamma delta epsilon zeta";
        let (lab_view, _, cx) = lab(cx, text, 110.0);
        lab_view.update(cx, |lab, cx| {
            lab.truncated = true;
            cx.notify();
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("lab-line").unwrap().size.width
                < cx.debug_bounds("lab-measure").unwrap().size.width,
            "the box must be narrower than the text for this test to mean anything"
        );

        reset_clipboard(cx);
        drag_chars(cx, text, 0.0, 5.0);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(clipboard(cx).as_deref(), Some("alpha"), "inside the visible part");

        reset_clipboard(cx);
        let start = line_point(cx, 0.0, text);
        let bounds = cx.debug_bounds("lab-line").unwrap();
        drag(cx, start, point(bounds.right() + px(50.0), start.y));
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(clipboard(cx).as_deref(), Some(text), "onto the ellipsis and beyond");
    }

    #[gpui::test]
    async fn accented_text_is_sliced_on_character_boundaries(cx: &mut TestAppContext) {
        let text = "città è là";
        let (_, _, cx) = lab(cx, text, 400.0);
        reset_clipboard(cx);

        drag_chars(cx, text, 0.0, 5.0);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(clipboard(cx).as_deref(), Some("città"));

        reset_clipboard(cx);
        drag_chars(cx, text, 6.0, 10.0);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(clipboard(cx).as_deref(), Some("è là"));
    }

    #[gpui::test]
    async fn double_click_selects_a_segment_and_triple_click_the_line(cx: &mut TestAppContext) {
        let text = "open feat/foo-bar now";
        let (_, _, cx) = lab(cx, text, 400.0);
        reset_clipboard(cx);

        press_chars(cx, text, 8.5, 2);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(clipboard(cx).as_deref(), Some("feat"));

        reset_clipboard(cx);
        press_chars(cx, text, 12.5, 2);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(clipboard(cx).as_deref(), Some("foo-bar"));

        reset_clipboard(cx);
        press_chars(cx, text, 8.5, 3);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(clipboard(cx).as_deref(), Some(text));
    }

    #[gpui::test]
    async fn text_under_a_modal_backdrop_cannot_be_selected(cx: &mut TestAppContext) {
        let text = "alpha beta gamma";
        let (lab_view, _, cx) = lab(cx, text, 400.0);

        // Positive control first: uncovered, the same drag copies.
        reset_clipboard(cx);
        drag_chars(cx, text, 0.0, 5.0);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(clipboard(cx).as_deref(), Some("alpha"));

        lab_view.update(cx, |lab, cx| {
            lab.covered = true;
            cx.notify();
        });
        cx.run_until_parked();
        reset_clipboard(cx);
        drag_chars(cx, text, 6.0, 10.0);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(
            clipboard(cx).as_deref(),
            Some(SENTINEL),
            "the backdrop is on top; nothing under it may be selected"
        );
    }

    // ----- Ownership -------------------------------------------------------

    #[gpui::test]
    async fn a_click_elsewhere_ends_the_selection(cx: &mut TestAppContext) {
        let text = "alpha beta gamma";
        let (_, _, cx) = lab(cx, text, 400.0);

        reset_clipboard(cx);
        drag_chars(cx, text, 0.0, 5.0);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(clipboard(cx).as_deref(), Some("alpha"), "control: it copies");

        cx.simulate_click(point(px(600.0), px(300.0)), Modifiers::none());
        reset_clipboard(cx);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(
            clipboard(cx).as_deref(),
            Some(SENTINEL),
            "the click ended the selection, so there is nothing left to copy"
        );
    }

    #[gpui::test]
    async fn text_that_changes_under_a_selection_is_neither_a_panic_nor_a_ghost(
        cx: &mut TestAppContext,
    ) {
        let text = "alpha beta gamma";
        let (lab_view, _, cx) = lab(cx, text, 400.0);

        drag_chars(cx, text, 6.0, 16.0);
        lab_view.update(cx, |lab, cx| {
            lab.text = "hi".into();
            cx.notify();
        });
        cx.run_until_parked();

        reset_clipboard(cx);
        cx.simulate_keystrokes("ctrl-c");
        let copied = clipboard(cx);
        assert!(
            matches!(copied.as_deref(), Some(SENTINEL) | Some("beta gamma")),
            "copied {copied:?}: only what was selected, or nothing"
        );
    }

    // ----- Keyboard: nobody else's Ctrl+C may be taken ---------------------
    //
    // Two different things can be focused when Ctrl+C is pressed, and they are
    // protected by two different layers: the binding is scoped `!Terminal`, so
    // a terminal never even sees the action; and `on_copy` itself declines
    // unless the selection's own sink holds focus, which is what protects a
    // surface that is not a terminal but reads the raw key — the file editor's
    // Copy. Each is tested on its own probe, so removing either guard fails.

    #[gpui::test]
    async fn ctrl_c_with_nothing_selected_reaches_a_focused_editor(cx: &mut TestAppContext) {
        let text = "alpha beta gamma";
        let (_, (_, editor), cx) = lab(cx, text, 400.0);
        cx.update(|window, cx| window.focus(&editor.focus, cx));
        cx.run_until_parked();

        reset_clipboard(cx);
        cx.simulate_keystrokes("ctrl-c");

        assert_eq!(
            editor.keys.borrow().as_slice(),
            ["ctrl-c"],
            "an action stops propagation by default; declining is what lets the editor copy"
        );
        assert_eq!(clipboard(cx).as_deref(), Some(SENTINEL));
    }

    #[gpui::test]
    async fn a_stale_selection_never_takes_ctrl_c_from_an_editor_focused_afterwards(
        cx: &mut TestAppContext,
    ) {
        let text = "alpha beta gamma";
        let (_, (_, editor), cx) = lab(cx, text, 400.0);

        drag_chars(cx, text, 0.0, 5.0);
        reset_clipboard(cx);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(clipboard(cx).as_deref(), Some("alpha"), "control: it copies");

        // The user moves to the editor without a press that would clear the
        // selection, and copies there.
        cx.update(|window, cx| window.focus(&editor.focus, cx));
        cx.run_until_parked();
        reset_clipboard(cx);
        cx.simulate_keystrokes("ctrl-c");

        assert_eq!(editor.keys.borrow().as_slice(), ["ctrl-c"]);
        assert_eq!(
            clipboard(cx).as_deref(),
            Some(SENTINEL),
            "the label's old selection must not answer for the editor's Copy"
        );
    }

    #[gpui::test]
    async fn ctrl_c_still_reaches_a_focused_terminal_whatever_was_selected(
        cx: &mut TestAppContext,
    ) {
        let text = "alpha beta gamma";
        let (_, (terminal, _), cx) = lab(cx, text, 400.0);

        // Nothing selected.
        cx.update(|window, cx| window.focus(&terminal.focus, cx));
        cx.run_until_parked();
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(terminal.keys.borrow().as_slice(), ["ctrl-c"]);

        // Something selected earlier, terminal focused since.
        terminal.keys.borrow_mut().clear();
        drag_chars(cx, text, 0.0, 5.0);
        cx.update(|window, cx| window.focus(&terminal.focus, cx));
        cx.run_until_parked();
        reset_clipboard(cx);
        cx.simulate_keystrokes("ctrl-c");

        assert_eq!(
            terminal.keys.borrow().as_slice(),
            ["ctrl-c"],
            "the terminal reads this key as SIGINT"
        );
        assert_eq!(clipboard(cx).as_deref(), Some(SENTINEL));
    }

    // ----- Layout: swapping the element must not move anything -------------

    #[gpui::test]
    async fn the_selectable_element_lays_out_exactly_like_the_plain_string(
        cx: &mut TestAppContext,
    ) {
        // Wide enough to sit on one line, then narrow enough to wrap.
        for (text, width) in [
            ("one short line", 400.0),
            ("this sentence is long enough to wrap onto several lines", 130.0),
        ] {
            let (_, _, cx) = lab(cx, text, width);
            let selectable = cx.debug_bounds("lab-line").expect("selectable renders");
            let plain = cx.debug_bounds("lab-plain").expect("plain renders");
            assert_eq!(selectable.size, plain.size, "{text:?} at {width}px");
        }
    }

    // ----- The real surface -------------------------------------------------

    /// The path Settings → General prints for the control socket is the text
    /// a user most plausibly wants to paste into a terminal. It was painted
    /// but not selectable; this drives the real Settings view to prove it now
    /// is, end to end: real layout, real pointer events, real clipboard.
    #[gpui::test]
    async fn the_control_socket_path_in_settings_can_be_selected_and_copied(
        cx: &mut TestAppContext,
    ) {
        const PATH: &str = "/run/user/1000/Sirio/control.sock";
        boot(cx);
        let snapshot = SettingsSnapshot {
            socket_path: PATH.into(),
            ..Default::default()
        };
        let (_, cx) = cx.add_window_view(|_, cx| {
            let settings = cx.new(|cx| Settings::with_snapshot(cx, snapshot));
            Root::new(settings.into(), cx)
        });
        cx.simulate_resize(size(px(1100.0), px(3200.0)));
        cx.run_until_parked();

        let general = cx
            .debug_bounds("settings-category-General")
            .expect("General category is offered");
        cx.simulate_click(general.center(), Modifiers::none());
        cx.run_until_parked();

        let line = cx
            .debug_bounds("settings-control-socket-path")
            .expect("the resolved path line renders");
        let y = line.center().y;
        let kind = if cfg!(windows) { "Named pipe" } else { "Socket path" };

        // Whole line: from just inside its left edge to well past its end.
        reset_clipboard(cx);
        drag(cx, point(line.left() + px(1.0), y), point(line.right() + px(200.0), y));
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(
            clipboard(cx).as_deref(),
            Some(format!("{kind}: {PATH}").as_str())
        );

        // Part of it: a proper, non-empty prefix — the selection follows the
        // pointer rather than always taking everything.
        reset_clipboard(cx);
        drag(
            cx,
            point(line.left() + px(1.0), y),
            point(line.left() + line.size.width / 8.0, y),
        );
        cx.simulate_keystrokes("ctrl-c");
        let full = format!("{kind}: {PATH}");
        let copied = clipboard(cx).expect("clipboard is readable");
        assert!(
            !copied.is_empty()
                && copied != SENTINEL
                && copied.len() < full.len()
                && full.starts_with(&copied),
            "copied {copied:?} out of {full:?}"
        );

        // Double-click at the end of the path takes its last segment — not the
        // label before it, and not the whole path.
        reset_clipboard(cx);
        press(cx, point(line.right() - px(4.0), y), 2);
        cx.simulate_keystrokes("ctrl-c");
        let copied = clipboard(cx).expect("clipboard is readable");
        assert_eq!(copied, "control.sock", "double-click takes one path segment");
    }
}
