//! Stands in for the top-level blocks of a long reply that sit far outside the window.
//!
//! A streamed reply of a few tens of KiB is a hundred blocks, and every chunk would lay out,
//! prepaint and paint all of them although the window shows a dozen. A body that is asked to
//! cull remembers, for each top-level block, where it sat and how tall it was; the next
//! frame draws a block that sat far outside the window (more than a window's height above or
//! below the visible rows, 512 px at least) as an empty block of the same height. Everything
//! a reader can reach stays the same: the stand-in takes the block's place in the column, and
//! Ely's element ids, the gather's offsets and the sideways frames' ordinals advance by what
//! the block took when it was last drawn.
//!
//! The decision is a frame stale, since a body cannot know where the window is until it is
//! placed. A stand-in whose real extent meets the visible rows after all is a miss, and the
//! body fixes it in the same frame: in prepaint, with the definite width the list gave it,
//! it draws those blocks, lays the tree out again and prepaints it where the first would
//! have been, as gpui's list does with its items. A width change forgets every height, so
//! that frame draws everything and asks for one more, which measures at the new width.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::panic::Location;
use std::rc::Rc;

use ely_gpui_component::documents::{BlockCull, BlockUse, Inline, MarkdownBlock, Style as Mark};
use ely_gpui_component::theme::{ActiveTheme as _, TextSize};
use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, SharedString, Style, Window, px, relative,
};

use super::gather::SirioGather;
use super::sideways::Held;

/// The least margin kept drawn above and below the rows the window shows.
const MARGIN: f32 = 512.0;

/// The state of a body's sideways frames, whether it draws them or stands in for them.
#[derive(Default)]
pub(super) struct HeldStore {
    frames: RefCell<HashMap<ElementId, Held>>,
    /// The frames drawn or stood in for since the last prune.
    touched: RefCell<HashSet<ElementId>>,
}

impl HeldStore {
    /// The state of frame `id`, made when there is none.
    pub(super) fn take(&self, id: &ElementId) -> Held {
        self.touched.borrow_mut().insert(id.clone());
        (self.frames.borrow_mut())
            .entry(id.clone())
            .or_insert_with(Held::new)
            .clone()
    }

    /// Keeps frame `id`'s state through a frame that does not draw it.
    pub(super) fn keep(&self, id: &ElementId) {
        self.touched.borrow_mut().insert(id.clone());
    }

    /// Forgets the frames nobody drew or stood in for since the last prune.
    fn prune(&self) {
        let touched = self.touched.take();
        self.frames.borrow_mut().retain(|id, _| touched.contains(id));
    }
}

/// What heights hang on besides a block and the width: the rem, the text sizes, the fonts and
/// the window's scale. Any change forgets every block.
#[derive(Clone, PartialEq)]
struct BodyKey {
    rem: Pixels,
    sizes: [Pixels; 3],
    fonts: [SharedString; 2],
    scale: f32,
}

impl BodyKey {
    fn read(window: &Window, cx: &App) -> Self {
        let theme = cx.theme();
        let rem = window.rem_size();
        let size = |size: TextSize| theme.text_size(size).to_pixels(rem);
        Self {
            rem,
            sizes: [size(TextSize::Base), size(TextSize::Sm), size(TextSize::Xs)],
            fonts: [theme.font_family.clone(), theme.mono_family.clone()],
            scale: window.scale_factor(),
        }
    }
}

/// A top-level block as its last drawing left it.
#[derive(Clone)]
struct Seen {
    /// Its wrapper's top against the body's, and its height.
    top: Pixels,
    height: Pixels,
    used: BlockUse,
    pushes: usize,
    frames: Vec<ElementId>,
    /// Draws at one height now and another later (see [`settles_late`]): never stood in for.
    late: bool,
}

/// Whether `block` holds a picture or math: they draw at one height before they have loaded
/// and another after, and a stand-in does not load anything, so the height remembered while
/// one was not loaded would stay wrong. Such a block is always drawn.
fn settles_late(block: &MarkdownBlock) -> bool {
    use MarkdownBlock as B;
    let inline = |inline: &Inline| inline.styles.iter().any(|(_, style)| *style == Mark::Math);
    let any = |blocks: &[MarkdownBlock]| blocks.iter().any(settles_late);
    match block {
        B::Image { .. } | B::Math(_) => true,
        B::Paragraph(text) | B::Heading(_, text) => inline(text),
        B::Quote(blocks) | B::Footnote { body: blocks, .. } | B::Alert { body: blocks, .. } => {
            any(blocks)
        }
        B::List { items, .. } => items.iter().any(|(_, blocks)| any(blocks)),
        B::Table { head, rows, .. } => {
            let cells = |cells: &Vec<Vec<MarkdownBlock>>| cells.iter().any(|cell| any(cell));
            cells(head) || rows.iter().any(cells)
        }
        _ => false,
    }
}

/// The rows of the body the window showed, against the body's top, and how far around them
/// blocks stay drawn.
#[derive(Clone, Copy)]
struct Rows {
    from: Pixels,
    to: Pixels,
    margin: Pixels,
}

/// What a body remembers of its blocks between frames; it lives in the body's element state.
#[derive(Default)]
pub(super) struct Memory {
    nodes: Option<Rc<[MarkdownBlock]>>,
    blocks: Vec<Option<Seen>>,
    key: Option<BodyKey>,
    width: Option<Pixels>,
    rows: Option<Rows>,
    held: Rc<HeldStore>,
}

impl Memory {
    /// The store the body's sideways frames keep their state in.
    pub(super) fn held(&self) -> Rc<HeldStore> {
        self.held.clone()
    }

    /// Forgets what changed: everything when the key does, else the blocks that differ from
    /// last frame's and those past the end.
    pub(super) fn sync(&mut self, nodes: &Rc<[MarkdownBlock]>, window: &Window, cx: &App) {
        let key = BodyKey::read(window, cx);
        if self.key.as_ref() != Some(&key) {
            self.blocks.clear();
            self.key = Some(key);
        }
        let same = (self.nodes.as_ref()).is_some_and(|old| Rc::ptr_eq(old, nodes));
        if !same {
            let old = self.nodes.clone();
            for (ix, seen) in self.blocks.iter_mut().enumerate() {
                let before = old.as_ref().and_then(|old| old.get(ix));
                if before.is_none() || before != nodes.get(ix) {
                    *seen = None;
                }
            }
            self.nodes = Some(nodes.clone());
        }
        self.blocks.truncate(nodes.len());
    }
}

/// Which blocks to draw whatever the memory says.
#[derive(Clone, Default)]
pub(super) enum Force {
    #[default]
    Nothing,
    These(HashSet<usize>),
    All,
}

/// What one block's drawing took: Ely's counts, the gather's pushes and frames.
type Took = (BlockUse, usize, Vec<ElementId>);

/// One frame's pass over one body.
pub(super) struct Culler {
    memory: Rc<RefCell<Memory>>,
    gather: Rc<SirioGather>,
    force: Force,
    /// The gather's pushes and frames before the block being drawn.
    mark: Cell<(usize, usize)>,
    took: RefCell<Vec<Option<Took>>>,
    /// The stand-ins of this pass and where each lays out.
    stand_ins: Rc<RefCell<Vec<(usize, LayoutId)>>>,
    placed: RefCell<Option<Vec<Bounds<Pixels>>>>,
}

/// What to do about a pass once the body's width and place are known.
pub(super) enum Plan {
    Keep,
    /// Draw again with `force`; `again` asks for one more frame.
    Redraw { force: Force, again: bool },
}

impl Culler {
    pub(super) fn new(
        memory: Rc<RefCell<Memory>>,
        gather: Rc<SirioGather>,
        force: Force,
    ) -> Self {
        Self {
            memory,
            gather,
            force,
            mark: Cell::new((0, 0)),
            took: RefCell::default(),
            stand_ins: Rc::default(),
            placed: RefCell::default(),
        }
    }

    fn note(&self, ix: usize, took: Took) {
        let mut all = self.took.borrow_mut();
        if all.len() <= ix {
            all.resize(ix + 1, None);
        }
        all[ix] = Some(took);
    }

    /// The rows of the body the window shows: the content mask's against the body's top.
    fn rows(bounds: Bounds<Pixels>, mask: Bounds<Pixels>) -> Rows {
        let from = mask.top().max(bounds.top()) - bounds.top();
        let to = (mask.bottom().min(bounds.bottom()) - bounds.top()).max(from);
        Rows {
            from,
            to,
            margin: mask.size.height.max(px(MARGIN)),
        }
    }

    /// Whether this pass stood in for a block that the window shows after all, or for any
    /// block at a width that is not the one the heights were taken at.
    pub(super) fn plan(
        &self,
        bounds: Bounds<Pixels>,
        mask: Bounds<Pixels>,
        window: &mut Window,
    ) -> Plan {
        let stood = self.stand_ins.borrow().clone();
        let mut memory = self.memory.borrow_mut();
        if memory.width.is_some_and(|width| width != bounds.size.width) {
            memory.blocks.clear();
            return match stood.is_empty() {
                true => Plan::Keep,
                false => Plan::Redraw {
                    force: Force::All,
                    again: true,
                },
            };
        }
        let rows = Self::rows(bounds, mask);
        let missed: HashSet<usize> = (stood.iter())
            .filter(|(_, layout)| {
                let at = window.layout_bounds(*layout);
                at.bottom() - bounds.top() > rows.from && at.top() - bounds.top() < rows.to
            })
            .map(|(ix, _)| *ix)
            .collect();
        match missed.is_empty() {
            true => Plan::Keep,
            false => Plan::Redraw {
                force: Force::These(missed),
                again: false,
            },
        }
    }

    /// Keeps what this pass learnt of the body's blocks (where each sat, how tall, what it
    /// took) and of the rows the window showed.
    pub(super) fn remember(&self, bounds: Bounds<Pixels>, mask: Bounds<Pixels>) {
        let mut memory = self.memory.borrow_mut();
        if let Some(placed) = self.placed.take() {
            let took = self.took.take();
            memory.blocks.resize(placed.len(), None);
            for (ix, block) in placed.iter().enumerate() {
                let late = (memory.nodes.as_ref())
                    .and_then(|nodes| nodes.get(ix))
                    .is_some_and(settles_late);
                memory.blocks[ix] =
                    (took.get(ix).cloned().flatten()).map(|(used, pushes, frames)| Seen {
                        top: block.top() - bounds.top(),
                        height: block.size.height,
                        used,
                        pushes,
                        frames,
                        late,
                    });
            }
        }
        memory.width = Some(bounds.size.width);
        memory.rows = Some(Self::rows(bounds, mask));
        memory.held.prune();
        probe::passed(self.stand_ins.borrow().len());
    }
}

impl BlockCull for Culler {
    fn skip(&self, ix: usize, _block: &MarkdownBlock) -> Option<(AnyElement, BlockUse)> {
        self.mark.set(self.gather.counts());
        if !enabled() {
            return None;
        }
        let forced = match &self.force {
            Force::Nothing => false,
            Force::These(set) => set.contains(&ix),
            Force::All => true,
        };
        if forced {
            return None;
        }
        let seen = {
            let memory = self.memory.borrow();
            let rows = memory.rows?;
            let seen = memory.blocks.get(ix)?.as_ref()?;
            if seen.late {
                return None;
            }
            let (top, bottom) = (seen.top, seen.top + seen.height);
            if bottom >= rows.from - rows.margin && top <= rows.to + rows.margin {
                return None;
            }
            seen.clone()
        };
        self.gather.skip(seen.pushes, &seen.frames);
        self.note(ix, (seen.used, seen.pushes, seen.frames));
        let stand_in = StandIn {
            ix,
            height: seen.height,
            into: self.stand_ins.clone(),
        };
        Some((stand_in.into_any_element(), seen.used))
    }

    fn drawn(&self, ix: usize, used: BlockUse) {
        let (pushes, frames) = self.mark.get();
        let took = (
            used,
            self.gather.counts().0 - pushes,
            self.gather.frames_from(frames),
        );
        self.note(ix, took);
    }

    fn placed(&self, bounds: Vec<Bounds<Pixels>>) {
        *self.placed.borrow_mut() = Some(bounds);
    }
}

/// An empty block as tall as the one it stands for. It notes where it lays out, which tells
/// the body, before it is prepainted, whether the window shows it after all.
struct StandIn {
    ix: usize,
    height: Pixels,
    into: Rc<RefCell<Vec<(usize, LayoutId)>>>,
}

impl IntoElement for StandIn {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for StandIn {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = self.height.into();
        let layout = window.request_layout(style, [], cx);
        self.into.borrow_mut().push((self.ix, layout));
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }
}

/// Whether bodies cull at all; the tests switch it off to compare against a full draw.
#[cfg(not(test))]
pub(super) fn enabled() -> bool {
    true
}

#[cfg(test)]
pub(super) fn enabled() -> bool {
    probe::ON.with(Cell::get)
}

/// What a test reads of the culling and how it switches it off.
#[cfg(test)]
pub(crate) mod probe {
    use std::cell::{Cell, RefCell};

    /// What the passes did since the last [`reset`].
    #[derive(Clone, Default, Debug, PartialEq)]
    pub(crate) struct Stats {
        /// Stand-ins laid out, summed over passes.
        pub stood: usize,
        /// Stand-ins in the latest pass.
        pub last: usize,
        /// Stand-ins in each pass, in order.
        pub per_pass: Vec<usize>,
        /// Passes that drew again in the same frame.
        pub redrawn: usize,
        /// Of those, the ones that drew everything (a width change).
        pub widened: usize,
    }

    thread_local! {
        pub(super) static ON: Cell<bool> = const { Cell::new(true) };
        static STATS: RefCell<Stats> = RefCell::new(Stats {
            stood: 0, last: 0, per_pass: Vec::new(), redrawn: 0, widened: 0,
        });
    }

    pub(crate) fn culling(on: bool) {
        ON.with(|cell| cell.set(on));
    }

    pub(crate) fn reset() {
        STATS.with(|cell| *cell.borrow_mut() = Stats::default());
    }

    pub(crate) fn stats() -> Stats {
        STATS.with(|cell| cell.borrow().clone())
    }

    pub(super) fn passed(stood: usize) {
        STATS.with(|cell| {
            let mut now = cell.borrow_mut();
            now.stood += stood;
            now.last = stood;
            now.per_pass.push(stood);
        });
    }

    pub(super) fn redrew(widened: bool) {
        STATS.with(|cell| {
            let mut now = cell.borrow_mut();
            now.redrawn += 1;
            now.widened += usize::from(widened);
        });
    }
}

#[cfg(not(test))]
mod probe {
    pub(super) fn passed(_stood: usize) {}
    pub(super) fn redrew(_widened: bool) {}
}

/// Notes that a pass drew again in the same frame (for the tests' probe).
pub(super) fn redrew(widened: bool) {
    probe::redrew(widened);
}
