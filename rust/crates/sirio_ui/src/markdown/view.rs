//! A [`MarkdownBlocks`] body drawn by Ely's renderer and selected by Sirio's own model.
//!
//! Ely draws; this element owns everything else (design §5.6): the selection (the
//! app-wide `text_selection` for a run, the chat's one offset space for a transcript), the
//! gather that tells drawing where the selection is and tells hit-testing where the text
//! is, the one path that opens a link, and the right press.
//!
//! One path opens links. Ely's `on_link` is set to a closure that opens only through the
//! [`ClickGate`], and the gate is open only for a left-button click with no drag and no
//! selection in the body, so a drag across a link never follows it and a right or middle
//! press, which Ely's `InteractiveText` would take for a click, never opens anything.
//! Sirio's own release handler opens nothing: no link opens twice.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::panic::Location;
use std::rc::Rc;

use ely_gpui_component::documents::{BlockCull, MarkdownBlock, MarkdownRenderer, TextGather};
use ely_gpui_component::navigation::Sections;
use ely_gpui_component::theme::ActiveTheme as _;
use gpui::{
    AnyElement, App, Bounds, DispatchPhase, Element, ElementId, Entity, EntityId, GlobalElementId,
    Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Size, TextAlign, TextRun, Window,
    canvas, div, prelude::*, px,
};

use super::code::{CodeStyle, code_hook, table_frame};
use super::cull::{Culler, Force, Memory, Plan, redrew};
use super::gather::{BodyLayouts, SirioGather};
use super::menu::{self, MarkdownMenuItem};
use super::{MarkdownBlocks, PartKind, PlainText};
use crate::chat::TranscriptInteraction;
use crate::text_selection::{
    Active, RunKey, begin, dragging, extend, finish, focus_sink, owns, release, selected_range,
    word_range,
};

/// The surface's opener for a link the reader clicked.
pub(crate) type LinkHandler = Rc<dyn Fn(&str, &mut Window, &mut App)>;
/// Takes a right press on the body.
pub(crate) type MenuHandler = Rc<dyn Fn(MenuPress, &mut Window, &mut App)>;

/// The mark a live reply is drawn with after its last text, where more will come.
const CARET: &str = "▍";

/// How far a press may travel and still be a click.
const CLICK_SLOP: f32 = 4.0;

/// What a view does beyond drawing the blocks.
#[derive(Clone, Default)]
pub(crate) struct ViewFlags {
    /// Loads `http(s)` pictures (the surface's policy, §5.5).
    pub web_pictures: bool,
    /// The headings' anchors, for a table of contents that scrolls to them.
    pub sections: Option<Entity<Sections>>,
    /// Stands in for top-level blocks that sit far outside the window (a chat reply, which
    /// grows without end): see `cull`.
    pub cull: bool,
}

/// Who owns the body's selection.
#[derive(Clone)]
pub(crate) enum SelectionHost {
    /// The app-wide selection of `text_selection`: Preview and change requests.
    Run { id: ElementId },
    /// The chat's one offset space: a drag may start in the user's prose and end here.
    /// `source_start` is where this body's plain text starts in the transcript's.
    Transcript {
        source_start: usize,
        interaction: TranscriptInteraction,
    },
    /// Drawn without selection: the subagent card's body.
    None,
}

/// What a right press found.
pub(crate) struct MenuPress {
    /// Where the press fell, in window pixels.
    pub position: Point<Pixels>,
    /// The selected text, if any.
    pub selected: Option<String>,
    /// The address of the link pressed.
    pub link: Option<String>,
    /// What the menu's rows do to this body.
    pub actions: BodyActions,
}

impl MenuPress {
    /// The rows the press offers.
    pub(crate) fn items(&self) -> Vec<MarkdownMenuItem> {
        menu::items(self.selected.is_some(), self.link.as_deref())
    }
}

#[derive(Clone)]
enum Mode {
    Run(RunKey),
    Transcript {
        source_start: usize,
        interaction: TranscriptInteraction,
    },
    None,
}

/// One body's selection, read and set from outside its handlers.
#[derive(Clone)]
pub(crate) struct BodyActions {
    plain: Rc<PlainText>,
    mode: Mode,
    view: EntityId,
}

impl BodyActions {
    /// The selected text, slices of the plain text: silent parts and separators included.
    pub(crate) fn selected(&self, cx: &App) -> Option<String> {
        let range = match &self.mode {
            Mode::Run(run) => selected_range(run, &self.plain.text, cx)?,
            Mode::Transcript {
                source_start,
                interaction,
            } => local(interaction.selected_span(cx)?, *source_start, self.plain.text.len())?,
            Mode::None => return None,
        };
        self.plain.text.get(range).map(str::to_owned)
    }

    /// Selects the whole body. A transcript's own menu selects its whole transcript.
    pub(crate) fn select_all(&self, window: &mut Window, cx: &mut App) {
        let Mode::Run(run) = &self.mode else {
            return;
        };
        if self.plain.text.is_empty() {
            return;
        }
        begin(
            Active {
                run: run.clone(),
                text: self.plain.text.clone().into(),
                anchor: 0,
                head: self.plain.text.len(),
                dragging: false,
                view: self.view,
            },
            cx,
        );
        focus_sink(window, cx);
    }
}

/// The part of the transcript's `span` that lies in a body of `len` bytes starting at
/// `start`, as the body's own bytes.
fn local(span: Range<usize>, start: usize, len: usize) -> Option<Range<usize>> {
    let from = span.start.max(start);
    let to = span.end.min(start + len);
    (from < to).then(|| from - start..to - start)
}

/// Opens a link only for a click: set by a release, spent by the link that hears it.
#[derive(Clone, Default)]
struct ClickGate {
    open: Rc<Cell<bool>>,
    /// Where the left press began on this body.
    pressed: Rc<Cell<Option<Point<Pixels>>>>,
}

impl ClickGate {
    /// Whether a link may open now; the answer is spent.
    fn take(&self) -> bool {
        self.open.replace(false)
    }
}

/// `blocks` as an element that draws and selects them.
///
/// A body with no `Run` host is identified by the call site, under whatever identified
/// elements enclose it.
#[track_caller]
pub(crate) fn markdown_view(
    blocks: &MarkdownBlocks,
    host: SelectionHost,
    links: LinkHandler,
    flags: ViewFlags,
) -> MarkdownView {
    let id = match &host {
        SelectionHost::Run { id } => id.clone(),
        _ => ElementId::CodeLocation(*Location::caller()),
    };
    MarkdownView {
        id,
        plain: blocks.text.clone(),
        nodes: blocks.nodes.clone(),
        caret: blocks.caret,
        host,
        links,
        flags,
        menu: None,
        layouts: BodyLayouts::default(),
        gate: ClickGate::default(),
        preselect: false,
        run: None,
        rendered: None,
        memory: None,
        culler: None,
    }
}

pub(crate) struct MarkdownView {
    id: ElementId,
    plain: Rc<PlainText>,
    nodes: Rc<[MarkdownBlock]>,
    /// Paints a caret after the last text of the body (a live reply): in no text of its own, so
    /// the plain text, a copy and a selection never hold it.
    caret: bool,
    host: SelectionHost,
    links: LinkHandler,
    flags: ViewFlags,
    menu: Option<MenuHandler>,
    /// What the last frame drew; the handlers read it.
    layouts: BodyLayouts,
    gate: ClickGate,
    /// Selects the whole body when it first lays out (the selection probe's frame).
    preselect: bool,
    run: Option<RunKey>,
    rendered: Option<AnyElement>,
    /// What a culling body remembers between frames, and this frame's pass over it.
    memory: Option<Rc<RefCell<Memory>>>,
    culler: Option<Rc<Culler>>,
}

impl MarkdownView {
    /// Takes a right press on the body.
    pub(crate) fn on_menu(mut self, handler: MenuHandler) -> Self {
        self.menu = Some(handler);
        self
    }

    /// What the body's drawing leaves behind: its texts, their layouts and links.
    pub(crate) fn layouts(&self) -> BodyLayouts {
        self.layouts.clone()
    }

    /// The selection of this frame, as a range of the plain text.
    fn selection(&self, cx: &App) -> Option<Range<usize>> {
        match &self.host {
            SelectionHost::Run { .. } => selected_range(self.run.as_ref()?, &self.plain.text, cx),
            SelectionHost::Transcript {
                source_start,
                interaction,
            } => local(interaction.selected_span(cx)?, *source_start, self.plain.text.len()),
            SelectionHost::None => None,
        }
    }

    fn mode(&self) -> Mode {
        match (&self.host, &self.run) {
            (SelectionHost::Run { .. }, Some(run)) => Mode::Run(run.clone()),
            (
                SelectionHost::Transcript {
                    source_start,
                    interaction,
                },
                _,
            ) => Mode::Transcript {
                source_start: *source_start,
                interaction: interaction.clone(),
            },
            _ => Mode::None,
        }
    }

    /// The body drawn once: a fresh gather, and a culler over the body's memory when it has
    /// one. Blocks in `force` are drawn whatever the memory says.
    fn draw(&mut self, force: Force, cx: &App) -> MarkdownRenderer {
        let held = self.memory.as_ref().map(|memory| memory.borrow().held());
        let gather = Rc::new(SirioGather::new(
            self.plain.clone(),
            self.selection(cx),
            self.layouts.clone(),
            held,
        ));
        let (gate, links) = (self.gate.clone(), self.links.clone());
        let mut renderer = MarkdownRenderer::blocks("body", self.nodes.clone())
            .gather(gather.clone() as Rc<dyn TextGather>)
            .diagrams(false)
            .web_pictures(self.flags.web_pictures)
            .code(code_hook(CodeStyle::read(cx), gather.clone()))
            .table_frame(table_frame(gather.clone()))
            .on_link(move |url, window, cx| {
                if gate.take() {
                    cx.stop_propagation();
                    window.prevent_default();
                    links(url, window, cx);
                }
            });
        if let Some(sections) = &self.flags.sections {
            renderer = renderer.sections(sections);
        }
        if let Some(memory) = &self.memory {
            let culler = Rc::new(Culler::new(memory.clone(), gather, force));
            renderer = renderer.cull(culler.clone() as Rc<dyn BlockCull>);
            self.culler = Some(culler);
        }
        renderer
    }

    /// Fixes a pass that stood in for what the window shows: the body is drawn again, in the
    /// same frame, with the blocks it needs, laid out at the width it was given and put where
    /// the first drawing would have been (the way gpui's list does with its items). Whether it
    /// did, in which case the new drawing is prepainted already.
    fn settle(
        &mut self,
        culler: &Culler,
        bounds: Bounds<Pixels>,
        mask: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let Plan::Redraw { force, again } = culler.plan(bounds, mask, window) else {
            return false;
        };
        redrew(matches!(force, Force::All));
        let body = self.draw(force, cx);
        let mut rendered = div().w(bounds.size.width).child(body).into_any_element();
        let space = Size {
            width: gpui::AvailableSpace::Definite(bounds.size.width),
            height: gpui::AvailableSpace::MaxContent,
        };
        rendered.layout_as_root(space, window, cx);
        rendered.prepaint_at(bounds.origin, window, cx);
        self.rendered = Some(rendered);
        if again {
            window.request_animation_frame();
        }
        true
    }

    /// The caret, after the end of the last text of the body: where the next word will go. A body
    /// that does not end in a text (a rule, a picture, a formula, a table of contents: the nearest
    /// text is above them, and a caret there would be in the wrong row) has none, nor has one
    /// whose last text is not on this frame (a culled block), nor one with no text at all.
    fn paint_caret(&self, window: &mut Window, cx: &mut App) {
        let Some(last) = (self.plain.parts.len()).checked_sub(1) else {
            return;
        };
        if !self.plain.ends_in_text || self.plain.parts[last].kind != PartKind::Drawn {
            return;
        }
        let Some(spot) = self.layouts.caret_spot(last) else {
            return;
        };
        let run = TextRun {
            len: CARET.len(),
            font: window.text_style().font(),
            color: cx.theme().colors.fg_muted,
            ..TextRun::default()
        };
        let line = window
            .text_system()
            .shape_line(CARET.into(), spot.font_size, &[run], None);
        if line
            .paint(spot.origin, spot.line_height, TextAlign::Left, None, window, cx)
            .is_ok()
        {
            self.layouts.set_caret(spot.origin);
        }
    }

    fn listen(&self, hitbox: Hitbox, window: &mut Window) {
        let view = window.current_view();
        let actions = BodyActions {
            plain: self.plain.clone(),
            mode: self.mode(),
            view,
        };
        let (layouts, gate) = (self.layouts.clone(), self.gate.clone());

        // A press. Capture: a press anywhere else ends this body's selection even when
        // something on top stops the event, and the click gate learns where a click began.
        // Bubble: a press on the body begins a selection; a right press asks for the menu.
        {
            let (actions, layouts, gate, hitbox) =
                (actions.clone(), layouts.clone(), gate.clone(), hitbox.clone());
            let menu = self.menu.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                let hovered = hitbox.is_hovered(window);
                match (event.button, phase) {
                    (MouseButton::Left, DispatchPhase::Capture) => {
                        gate.open.set(false);
                        gate.pressed.set(hovered.then_some(event.position));
                        if let (Mode::Run(run), false) = (&actions.mode, hovered) {
                            release(run, cx);
                        }
                    }
                    (MouseButton::Left, DispatchPhase::Bubble) if hovered => {
                        press(&actions, &layouts, event, window, cx);
                    }
                    (MouseButton::Right, DispatchPhase::Bubble) if hovered => {
                        if let Some(menu) = &menu {
                            let press = MenuPress {
                                position: event.position,
                                selected: actions.selected(cx),
                                link: layouts.link_at(event.position),
                                actions: actions.clone(),
                            };
                            menu(press, window, cx);
                        }
                    }
                    _ => {}
                }
            });
        }

        // The drag follows the pointer wherever it goes; the caret falls back to the
        // nearest text, so a drag past the last line ends on it. A transcript's bodies share
        // one selection and each speaks only for the pointer over it.
        {
            let (actions, layouts, hitbox) = (actions.clone(), layouts.clone(), hitbox.clone());
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture || !event.dragging() {
                    return;
                }
                match &actions.mode {
                    Mode::Run(run) if dragging(run, cx) => {
                        if let Some(offset) = layouts.offset_at(event.position) {
                            extend(run, offset, view, window, cx);
                        }
                    }
                    Mode::Transcript {
                        source_start,
                        interaction,
                    } if interaction.is_dragging(cx) && hitbox.is_hovered(window) => {
                        if let Some(offset) = layouts.offset_at(event.position) {
                            interaction.extend_selection(source_start + offset, window, cx);
                        }
                    }
                    _ => {}
                }
            });
        }

        // The release. Capture, so it runs before the link under the pointer hears its own
        // click: it decides whether this release is one, and only a left click that moved
        // little, over this body, with nothing selected between press and release, is.
        {
            let (actions, hitbox) = (actions, hitbox);
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                if event.button != MouseButton::Left {
                    gate.open.set(false);
                    return;
                }
                let began = gate.pressed.take();
                let nothing_selected = match &actions.mode {
                    // Not the owner when the press began on no text (a picture): nothing selected.
                    Mode::Run(run) => finish(run, view, cx) != Some(true),
                    Mode::Transcript { interaction, .. } => !interaction.end_drag(cx),
                    Mode::None => true,
                };
                let near = began.is_some_and(|from| {
                    (from.x - event.position.x).abs() <= px(CLICK_SLOP)
                        && (from.y - event.position.y).abs() <= px(CLICK_SLOP)
                });
                // A drop (from the OS or from inside the app) ends over the link too, and is
                // not a click.
                let dropped = cx.has_active_drag();
                gate.open
                    .set(!dropped && nothing_selected && near && hitbox.is_hovered(window));
            });
        }
    }
}

/// A left press on the body: a caret for a click or a drag, a word for a double click, the
/// text for a triple click.
fn press(
    actions: &BodyActions,
    layouts: &BodyLayouts,
    event: &MouseDownEvent,
    window: &mut Window,
    cx: &mut App,
) {
    // A press on a reply is the press of a reply wherever it lands, a picture included: the
    // transcript must not take the focus for it (the composer keeps it, and a drag that
    // selects something takes it later).
    if matches!(actions.mode, Mode::Transcript { .. }) {
        window.prevent_default();
    }
    // A word is the glyph the pointer is on, not the nearer edge, which in the right half
    // of a word's last letter is the separator after it.
    let offset = match event.click_count {
        2 => layouts.glyph_at(event.position),
        _ => layouts.offset_at(event.position),
    };
    let Some(offset) = offset else {
        return;
    };
    let plain = &actions.plain;
    let (anchor, head) = match event.click_count {
        0 | 1 => (offset, offset),
        2 => {
            let word = word_range(&plain.text, offset);
            (word.start, word.end)
        }
        _ => {
            let span = (plain.parts.iter().rev())
                .find(|part| part.range.start <= offset)
                .map_or(offset..offset, |part| part.range.clone());
            (span.start, span.end)
        }
    };
    match &actions.mode {
        Mode::Run(run) => {
            begin(
                Active {
                    run: run.clone(),
                    text: plain.text.clone().into(),
                    anchor,
                    head,
                    dragging: true,
                    view: actions.view,
                },
                cx,
            );
            if event.click_count >= 2 {
                focus_sink(window, cx);
                window.prevent_default();
            }
        }
        Mode::Transcript {
            source_start,
            interaction,
        } => {
            interaction.begin_selection((source_start + anchor, source_start + head), window, cx);
        }
        Mode::None => {}
    }
}

impl IntoElement for MarkdownView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for MarkdownView {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.run = matches!(self.host, SelectionHost::Run { .. })
            .then(|| RunKey::new(id, &self.plain.text));
        if let (true, Some(run)) = (self.preselect, &self.run)
            && !owns(run, cx)
            && !self.plain.text.is_empty()
        {
            begin(
                Active {
                    run: run.clone(),
                    text: self.plain.text.clone().into(),
                    anchor: 0,
                    head: self.plain.text.len(),
                    dragging: false,
                    view: window.current_view(),
                },
                cx,
            );
        }
        // The gate outlives the frame: a press and its release are two frames apart.
        self.gate = window.with_optional_element_state::<ClickGate, _>(id, |state, _| {
            let gate = state.flatten().unwrap_or_default();
            (gate.clone(), Some(gate))
        });
        self.gate.open.set(false);

        self.memory = (self.flags.cull && super::cull::enabled()).then(|| {
            window.with_optional_element_state::<Rc<RefCell<Memory>>, _>(id, |state, _| {
                let memory = state.flatten().unwrap_or_default();
                (memory.clone(), Some(memory))
            })
        });
        if let Some(memory) = &self.memory {
            memory.borrow_mut().sync(&self.nodes, window, cx);
        }
        // Drawing runs inside this call: the gather has every text when it returns.
        let mut rendered = self.draw(Force::Nothing, cx).into_any_element();
        let layout = rendered.request_layout(window, cx);
        self.rendered = Some(rendered);
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let mask = window.content_mask().bounds;
        let redrawn = match self.culler.clone() {
            Some(culler) => self.settle(&culler, bounds, mask, window, cx),
            None => false,
        };
        if !redrawn {
            self.rendered
                .as_mut()
                .expect("markdown requested layout before prepaint")
                .prepaint(window, cx);
        }
        if let Some(culler) = self.culler.take() {
            culler.remember(bounds, mask);
        }
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.listen(hitbox.clone(), window);
        self.rendered
            .as_mut()
            .expect("markdown prepainted before paint")
            .paint(window, cx);
        if self.caret {
            self.paint_caret(window, cx);
        }
    }
}

/// The probes' body: `source` as the Preview shows it, under `id`, with what the scroll
/// probe reads reported on stdout as `BLOCK doc <id> <n> <x> <y> <w> <h>` (window pixels)
/// whenever the sideways frames move. `select_all` paints the whole body selected.
pub(crate) fn probe_view(id: &'static str, source: &str, select_all: bool) -> AnyElement {
    use std::io::Write as _;

    use super::{Policy, Surface, build};

    let blocks = build(
        source,
        &mut Policy {
            surface: Surface::Preview,
            base: None,
            diagrams: None,
            file_links: None,
        },
    );
    let mut view = markdown_view(
        &blocks,
        SelectionHost::Run { id: id.into() },
        Rc::new(|_, _, _| {}),
        ViewFlags::default(),
    );
    view.preselect = select_all;
    let layouts = view.layouts();
    let reported = Rc::new(Cell::new(Vec::<(usize, [i64; 4])>::new()));
    let report = canvas(
        |_, _, _| (),
        move |_, _, _, _| {
            let round = |value: Pixels| f32::from(value).round() as i64;
            let now: Vec<_> = (0..layouts.frame_count())
                .filter_map(|ordinal| {
                    let bounds = layouts.frame_bounds(ordinal)?;
                    Some((
                        ordinal,
                        [
                            round(bounds.origin.x),
                            round(bounds.origin.y),
                            round(bounds.size.width),
                            round(bounds.size.height),
                        ],
                    ))
                })
                .collect();
            let before = reported.take();
            if before != now {
                let mut out = std::io::stdout().lock();
                for (ordinal, [x, y, w, h]) in &now {
                    let _ = writeln!(out, "BLOCK doc {id} {ordinal} {x} {y} {w} {h}");
                }
                let _ = out.flush();
            }
            reported.set(now);
        },
    )
    .absolute()
    .size(px(0.0));
    div().child(view).child(report).into_any_element()
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
