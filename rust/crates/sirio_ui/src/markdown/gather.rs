//! The body's side of Ely's text gather (design §5.6).
//!
//! Drawing hands every text it lays out to a [`SirioGather`], in push order. The gather
//! answers with the selection's highlight for that text, and keeps the layouts so a pointer
//! position becomes a place in the body's plain text ([`BodyLayouts`]).
//!
//! A text's place is found by order: the n-th push is the n-th drawn part of the
//! [`PlainText`], which `text.rs` walks in the renderer's own order.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use ely_gpui_component::documents::TextGather;
use ely_gpui_component::theme::ActiveTheme as _;
use gpui::{App, Bounds, ElementId, HighlightStyle, Hsla, Pixels, Point, TextLayout, point, px};

use super::cull::HeldStore;
use super::sideways::Held;
use super::{PartKind, PlainText};
use crate::text_selection::{caret_index, hit_index};

/// One text the renderer drew, as the last frame laid it out.
#[derive(Clone)]
pub(crate) struct Drawn {
    /// Its drawn part in the plain text; `None` when the push did not match the walk, so
    /// the text can be neither hit nor highlighted.
    pub part: Option<usize>,
    /// A list item's marker, written before this text.
    pub lead: String,
    pub text: String,
    /// Where the text itself (past any marker) starts in the plain text.
    pub start: usize,
    pub layout: TextLayout,
    pub links: Vec<(Range<usize>, String)>,
    /// The bytes of the text the selection highlighted (what the tests compare).
    #[cfg(test)]
    pub selected: Vec<Range<usize>>,
}

/// What the tests read of one drawn text.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DrawnPart {
    pub part: Option<usize>,
    pub start: usize,
    pub bounds: Bounds<Pixels>,
    pub selected: Vec<Range<usize>>,
}

/// A link the last frame drew.
pub(crate) struct DrawnLink {
    pub label: String,
    pub url: String,
    /// Where its label starts.
    pub start: Point<Pixels>,
    pub line_height: Pixels,
    /// How wide its first line painted.
    pub width: Pixels,
}

#[derive(Default)]
struct Frame {
    texts: Vec<Drawn>,
    /// Where each sideways frame sat, by its place among the body's.
    frames: Vec<Option<Bounds<Pixels>>>,
}

/// What a body's drawing left behind, shared with the handlers that read it: the texts,
/// their layouts and their links. The same value serves every frame.
#[derive(Clone, Default)]
pub(crate) struct BodyLayouts(Rc<RefCell<Frame>>);

/// The text nearest a position, and the position clamped onto it.
struct Nearest {
    text: usize,
    at: Point<Pixels>,
}

impl BodyLayouts {
    fn clear(&self) {
        let mut frame = self.0.borrow_mut();
        frame.texts.clear();
        frame.frames.clear();
    }

    fn add(&self, drawn: Drawn) {
        self.0.borrow_mut().texts.push(drawn);
    }

    /// The text a position is on or nearest, as the selection reads it: the first text
    /// below the position when it is above them all, the one beside it on its line,
    /// otherwise the end of the last.
    fn nearest(&self, position: Point<Pixels>) -> Option<Nearest> {
        let frame = self.0.borrow();
        let mut beside: Option<(Pixels, usize)> = None;
        let mut last = None;
        for (k, drawn) in frame.texts.iter().enumerate() {
            if drawn.part.is_none() {
                continue;
            }
            last = Some(k);
            let bounds = drawn.layout.bounds();
            if position.y < bounds.top() {
                if beside.is_none() {
                    return Some(Nearest {
                        text: k,
                        at: bounds.origin,
                    });
                }
                break;
            }
            if position.y <= bounds.bottom() {
                let gap = (bounds.left() - position.x)
                    .max(position.x - bounds.right())
                    .max(Pixels::ZERO);
                if beside.is_none_or(|(best, _)| gap < best) {
                    beside = Some((gap, k));
                }
            }
        }
        let k = match beside {
            Some((_, k)) => k,
            None => {
                let k = last?;
                let bounds = frame.texts[k].layout.bounds();
                return Some(Nearest {
                    text: k,
                    at: point(bounds.right(), bounds.bottom() - px(1.0)),
                });
            }
        };
        let bounds = frame.texts[k].layout.bounds();
        Some(Nearest {
            text: k,
            at: point(position.x.clamp(bounds.left(), bounds.right()), position.y),
        })
    }

    /// The caret nearest `position`, as a place in the plain text: between two glyphs, the
    /// half of a glyph the pointer is in deciding which edge.
    pub(crate) fn offset_at(&self, position: Point<Pixels>) -> Option<usize> {
        let near = self.nearest(position)?;
        let frame = self.0.borrow();
        let drawn = &frame.texts[near.text];
        Some(drawn.start + caret_index(&drawn.layout, near.at, &drawn.text))
    }

    /// The glyph under `position`, as a place in the plain text: what a double-click means
    /// by "the word here". Off the text it clamps to the nearest.
    pub(crate) fn glyph_at(&self, position: Point<Pixels>) -> Option<usize> {
        let near = self.nearest(position)?;
        let frame = self.0.borrow();
        let drawn = &frame.texts[near.text];
        Some(drawn.start + hit_index(&drawn.layout, near.at, &drawn.text).0)
    }

    /// The address of the link `position` is on.
    pub(crate) fn link_at(&self, position: Point<Pixels>) -> Option<String> {
        let frame = self.0.borrow();
        frame.texts.iter().find_map(|drawn| {
            if drawn.links.is_empty() || !drawn.layout.bounds().contains(&position) {
                return None;
            }
            let index = drawn.layout.index_for_position(position).ok()?;
            let (_, url) = drawn.links.iter().find(|(range, _)| range.contains(&index))?;
            Some(url.clone())
        })
    }

    /// Every link drawn, with the boxes it occupies (one per line it spans), in window
    /// pixels.
    pub(crate) fn link_rects(&self) -> Vec<(Bounds<Pixels>, String)> {
        let frame = self.0.borrow();
        let mut rects = Vec::new();
        for drawn in &frame.texts {
            let bounds = drawn.layout.bounds();
            let line = drawn.layout.line_height();
            for (range, url) in &drawn.links {
                let (Some(from), Some(to)) = (
                    drawn.layout.position_for_index(range.start),
                    drawn.layout.position_for_index(range.end),
                ) else {
                    continue;
                };
                if from.y == to.y {
                    rects.push((Bounds::from_corners(from, point(to.x, to.y + line)), url.clone()));
                    continue;
                }
                rects.push((
                    Bounds::from_corners(from, point(bounds.right(), from.y + line)),
                    url.clone(),
                ));
                if to.y > from.y + line {
                    rects.push((
                        Bounds::from_corners(
                            point(bounds.left(), from.y + line),
                            point(bounds.right(), to.y),
                        ),
                        url.clone(),
                    ));
                }
                rects.push((
                    Bounds::from_corners(point(bounds.left(), to.y), point(to.x, to.y + line)),
                    url.clone(),
                ));
            }
        }
        rects
    }

    /// Every link drawn, one entry for each, as the control socket reports it: its label and
    /// address, and where its first line sits, in window pixels.
    pub(crate) fn links(&self) -> Vec<DrawnLink> {
        let frame = self.0.borrow();
        let mut found = Vec::new();
        for drawn in &frame.texts {
            let bounds = drawn.layout.bounds();
            let line_height = drawn.layout.line_height();
            for (range, url) in &drawn.links {
                let (Some(from), Some(to)) = (
                    drawn.layout.position_for_index(range.start),
                    drawn.layout.position_for_index(range.end),
                ) else {
                    continue;
                };
                // A label that wraps ends its first line at the edge of the text.
                let same_row = (to.y - from.y).abs() < line_height / 2.0;
                let end = if same_row { to.x } else { bounds.right() };
                found.push(DrawnLink {
                    label: drawn.text.get(range.clone()).unwrap_or_default().to_owned(),
                    url: url.clone(),
                    start: from,
                    line_height,
                    width: end - from.x,
                });
            }
        }
        found
    }

    /// Every text of the last frame with the box it was laid out in (window pixels), in
    /// push order: what the debug readout of a Preview gives a driver to aim at.
    pub(crate) fn text_rows(&self) -> Vec<(Bounds<Pixels>, String)> {
        let frame = self.0.borrow();
        (frame.texts.iter())
            .map(|drawn| (drawn.layout.bounds(), drawn.text.clone()))
            .collect()
    }

    /// Every text of the last frame: its part, where it starts, where it was laid out and
    /// which of its bytes the selection highlighted.
    #[cfg(test)]
    pub(crate) fn drawn_parts(&self) -> Vec<DrawnPart> {
        let frame = self.0.borrow();
        (frame.texts.iter())
            .map(|drawn| DrawnPart {
                part: drawn.part,
                start: drawn.start,
                bounds: drawn.layout.bounds(),
                selected: drawn.selected.clone(),
            })
            .collect()
    }

    /// Every text pushed to the last frame's gather: the marker written before it and the
    /// text, in push order.
    #[cfg(test)]
    pub(crate) fn pushes(&self) -> Vec<(String, String)> {
        let frame = self.0.borrow();
        (frame.texts.iter())
            .map(|drawn| (drawn.lead.clone(), drawn.text.clone()))
            .collect()
    }

    /// Where the `index`-th text of the last frame was laid out.
    #[cfg(test)]
    pub(crate) fn text_bounds(&self, index: usize) -> Option<Bounds<Pixels>> {
        self.0.borrow().texts.get(index).map(|drawn| drawn.layout.bounds())
    }

    /// Where the `ordinal`-th sideways frame sat in the last frame.
    pub(crate) fn frame_bounds(&self, ordinal: usize) -> Option<Bounds<Pixels>> {
        self.0.borrow().frames.get(ordinal).copied().flatten()
    }

    /// How many sideways frames the last frame drew.
    pub(crate) fn frame_count(&self) -> usize {
        self.0.borrow().frames.len()
    }

    pub(super) fn record_frame(&self, ordinal: usize, bounds: Bounds<Pixels>) {
        let mut frame = self.0.borrow_mut();
        if frame.frames.len() <= ordinal {
            frame.frames.resize(ordinal + 1, None);
        }
        frame.frames[ordinal] = Some(bounds);
    }
}

/// One frame's drawing of one body: reads the selection to paint, records what was drawn.
pub(crate) struct SirioGather {
    plain: Rc<PlainText>,
    /// The parts the renderer pushes, in order.
    drawn: Vec<usize>,
    /// The selected bytes of the plain text.
    selection: Option<Range<usize>>,
    layouts: BodyLayouts,
    /// Pushes so far.
    pushed: Cell<usize>,
    lead: RefCell<String>,
    /// The ids of the sideways frames handed out so far, stood-in ones included.
    frames: RefCell<Vec<ElementId>>,
    /// Where a body that stands in for blocks keeps its sideways frames' state.
    held: Option<Rc<HeldStore>>,
    /// Whether a push that did not fit the walk has been reported.
    reported: Cell<bool>,
    /// The highlight the last text was given, for the push that follows it.
    #[cfg(test)]
    last_selected: RefCell<Vec<Range<usize>>>,
}

impl SirioGather {
    /// A gather for one frame, which clears what `layouts` held.
    pub(crate) fn new(
        plain: Rc<PlainText>,
        selection: Option<Range<usize>>,
        layouts: BodyLayouts,
        held: Option<Rc<HeldStore>>,
    ) -> Self {
        layouts.clear();
        let drawn = (plain.parts.iter().enumerate())
            .filter(|(_, part)| part.kind == PartKind::Drawn)
            .map(|(ix, _)| ix)
            .collect();
        Self {
            plain,
            drawn,
            selection,
            layouts,
            pushed: Cell::new(0),
            lead: RefCell::default(),
            frames: RefCell::default(),
            held,
            reported: Cell::new(false),
            #[cfg(test)]
            last_selected: RefCell::default(),
        }
    }

    pub(crate) fn layouts(&self) -> &BodyLayouts {
        &self.layouts
    }

    /// The next sideways frame's place among the body's, and its state when the body owns it;
    /// `id` is the id Ely gave the frame.
    pub(crate) fn next_frame(&self, id: &ElementId) -> (usize, Option<Held>) {
        let mut frames = self.frames.borrow_mut();
        frames.push(id.clone());
        let held = self.held.as_ref().map(|store| store.take(id));
        (frames.len() - 1, held)
    }

    /// Pushes and sideways frames so far.
    pub(super) fn counts(&self) -> (usize, usize) {
        (self.pushed.get(), self.frames.borrow().len())
    }

    /// The ids of the frames handed out from the `from`-th on.
    pub(super) fn frames_from(&self, from: usize) -> Vec<ElementId> {
        self.frames.borrow()[from..].to_vec()
    }

    /// Steps over a block that a stand-in draws: its pushes and its frames, whose state is
    /// kept for when the block is drawn again.
    pub(super) fn skip(&self, pushes: usize, frames: &[ElementId]) {
        self.pushed.set(self.pushed.get() + pushes);
        self.frames.borrow_mut().extend_from_slice(frames);
        if let Some(store) = &self.held {
            for id in frames {
                store.keep(id);
            }
        }
    }

    /// The part the next push is, if `text` is it.
    fn expected(&self, text: &str) -> Option<usize> {
        let part = *self.drawn.get(self.pushed.get())?;
        let place = &self.plain.parts[part];
        match self.plain.text.get(place.range.start + place.lead..place.range.end) {
            Some(own) if own == text => Some(part),
            _ => {
                if !self.reported.replace(true) {
                    eprintln!("markdown: a drawn text is not the plain text's next part");
                }
                None
            }
        }
    }

    /// The selection's highlight on `text`, the next drawn, in `color`.
    pub(crate) fn highlight(&self, text: &str, color: Hsla) -> Vec<(Range<usize>, HighlightStyle)> {
        let found = self.highlighted(text, color);
        #[cfg(test)]
        self.last_selected
            .replace(found.iter().map(|(range, _)| range.clone()).collect());
        found
    }

    fn highlighted(&self, text: &str, color: Hsla) -> Vec<(Range<usize>, HighlightStyle)> {
        let Some(selection) = &self.selection else {
            return Vec::new();
        };
        let Some(part) = self.expected(text) else {
            return Vec::new();
        };
        let place = &self.plain.parts[part];
        let start = place.range.start + place.lead;
        let from = selection.start.max(start);
        let to = selection.end.min(place.range.end);
        if from >= to {
            return Vec::new();
        }
        let range = from - start..to - start;
        if !text.is_char_boundary(range.start) || !text.is_char_boundary(range.end) {
            return Vec::new();
        }
        vec![(
            range,
            HighlightStyle {
                background_color: Some(color),
                ..Default::default()
            },
        )]
    }

    /// Notes a drawn text, in order.
    pub(crate) fn note(&self, text: &str, layout: TextLayout, links: &[(Range<usize>, String)]) {
        let part = self.expected(text);
        self.pushed.set(self.pushed.get() + 1);
        let start = part.map_or(0, |part| {
            let place = &self.plain.parts[part];
            place.range.start + place.lead
        });
        self.layouts.add(Drawn {
            part,
            lead: self.lead.take(),
            text: text.to_owned(),
            start,
            layout,
            links: links.to_vec(),
            #[cfg(test)]
            selected: self.last_selected.take(),
        });
    }
}

impl TextGather for SirioGather {
    fn over(&self, text: &str, cx: &App) -> Vec<(Range<usize>, HighlightStyle)> {
        self.highlight(text, cx.theme().colors.selection)
    }

    fn push(&self, text: &str, layout: TextLayout, links: &[(Range<usize>, String)]) {
        self.note(text, layout, links);
    }

    fn lead(&self, lead: String) {
        *self.lead.borrow_mut() = lead;
    }
}
