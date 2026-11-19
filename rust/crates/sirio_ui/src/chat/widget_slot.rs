//! MCP Apps widgets in their transcript rows.
//!
//! A tool call whose tool names a UI gets a slot: plain data (phase, error, what the call
//! said before a widget existed, header text, the last sweep's record) with the widget
//! entity optional, so a test can make a slot without a webview. The lookup that decides
//! who gets a slot is `widget_lookup`; this file is the GPUI half: the CLI calls off the UI
//! thread, the widget's events, the row, and the sweep that places each webview over its row.
//!
//! The slot's prepaint only records where it was drawn, by tool-use id, as a list item index
//! and an offset inside that item: `List::prepaint` holds its state mutably borrowed while the
//! items prepaint, so nothing here reads `ListState` there. A canvas after the list runs the
//! sweep once the list has set its viewport and reads the list then.

use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    rc::Rc,
};

use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, Pixels, Point,
    Size, Subscription, Window, canvas, div, prelude::*, px,
};
use sirio_chat::{
    McpOrigin, McpServer, McpToolItem, McpToolUpdate, UiToolRef,
    mcp_apps::{bridge::HostContext, csp::UiCsp, document::widget_document},
    ui_tool_for,
};
use sirio_theme::{Appearance, Theme};

use super::{
    Chat, Entry, TRANSCRIPT_WIDTH, composer_row::TRANSCRIPT_PAD_X, question_dock::question_view,
    segment_turns, session_panel::Placement, tool_call_run_bounds_inclusive,
    visible_text::visible_text,
    widget_asks::AskState,
    widget_lookup::{Delivery, Held, Lookups, Step, Update},
};
use crate::mcp_widget::{
    McpWidget, WidgetEvent, WidgetPhase, WidgetSnapshot, widget_webview_available,
};

/// Widgets alive at once in one chat; the oldest beyond that is closed.
const MAX_LIVE: usize = 6;
const LOADING_HEIGHT: u32 = 160;
const MIN_HEIGHT: u32 = 80;
const MAX_HEIGHT: u32 = 800;

const NO_SESSION: &str = "The widget could not be loaded: the session is gone";
/// Why a slot closed when its CLI went away, and when it was replaced by a new process.
pub(super) const SESSION_ENDED: &str = "the session ended";
pub(super) const SESSION_RESTARTED: &str = "the session restarted";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SlotPhase {
    Resolving,
    Loading,
    Ready,
    Failed,
    Closed,
}

impl SlotPhase {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Resolving => "resolving",
            Self::Loading => "loading",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Closed => "closed",
        }
    }

    fn is_live(self) -> bool {
        matches!(self, Self::Loading | Self::Ready)
    }
}

/// What the last sweep saw of one slot.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Seen {
    pub(super) swept: bool,
    /// Whether the slot was laid out in the last sweep.
    pub(super) drawn: bool,
    /// Where the slot was, from the last sweep that drew it.
    pub(super) slot: Option<Bounds<Pixels>>,
    pub(super) viewport: Bounds<Pixels>,
    pub(super) overlay_open: bool,
    pub(super) dock_open: bool,
    /// The host had the chat off screen (`unmap_widgets`) or covered by an overlay.
    pub(super) hidden: bool,
}

/// What a slot's own state and its widget's last snapshot say, together.
pub(super) struct SlotView {
    pub(super) phase: SlotPhase,
    pub(super) error: String,
    pub(super) height: u32,
    pub(super) mapped: bool,
    pub(super) asks: u32,
    pub(super) answered: u32,
    pub(super) refused: Vec<String>,
    pub(super) xid: Option<u64>,
    pub(super) input_delivered: bool,
    pub(super) result_delivered: bool,
}

pub(super) struct Slot {
    pub(super) id: String,
    pub(super) server: String,
    /// The server's name as the CLI lists it, for the name a widget's call goes by.
    pub(super) raw_server: String,
    pub(super) origin: McpOrigin,
    pub(super) tool: String,
    /// The chat's turn key when the call was announced.
    pub(super) turn: u64,
    pub(super) phase: SlotPhase,
    /// One line, already through `visible_text`.
    pub(super) error: String,
    /// What the call said before the widget existed.
    pub(super) held: Option<Held>,
    /// The origins the document may reach, comma-separated; empty is no network.
    pub(super) domains: String,
    /// The CSP entries the host refused, `|`-separated.
    pub(super) dropped: String,
    pub(super) widget: Option<Entity<McpWidget>>,
    /// The page's answer, held until the chat has a window to build the widget in.
    pub(super) waiting_read: Option<(String, Result<serde_json::Value, String>)>,
    subscriptions: Vec<Subscription>,
    snapshot: RefCell<Option<WidgetSnapshot>>,
    seen: Cell<Seen>,
}

impl Slot {
    pub(super) fn resolving(id: String, turn: u64, hit: &UiToolRef, held: Held) -> Self {
        Self {
            id,
            server: visible_text(&hit.server, false),
            raw_server: hit.server.clone(),
            origin: hit.origin.clone(),
            tool: visible_text(&hit.tool, false),
            turn,
            phase: SlotPhase::Resolving,
            error: String::new(),
            held: Some(held),
            domains: String::new(),
            dropped: String::new(),
            widget: None,
            waiting_read: None,
            subscriptions: Vec::new(),
            snapshot: RefCell::new(None),
            seen: Cell::new(Seen::default()),
        }
    }

    pub(super) fn view(&self) -> SlotView {
        let snapshot = self.snapshot.borrow();
        let own = |phase: SlotPhase| SlotView {
            phase,
            error: self.error.clone(),
            height: 0,
            mapped: false,
            asks: 0,
            answered: 0,
            refused: Vec::new(),
            xid: None,
            input_delivered: false,
            result_delivered: false,
        };
        let (Some(_), Some(snap)) = (&self.widget, snapshot.as_ref()) else {
            return own(self.phase);
        };
        if !self.phase.is_live() {
            return own(self.phase);
        }
        let (phase, error) = match &snap.phase {
            WidgetPhase::Loading => (SlotPhase::Loading, self.error.clone()),
            WidgetPhase::Ready => (SlotPhase::Ready, self.error.clone()),
            WidgetPhase::Failed(error) => (SlotPhase::Failed, visible_text(error, true)),
            WidgetPhase::Closed => (SlotPhase::Closed, self.error.clone()),
        };
        SlotView {
            phase,
            error,
            height: snap.height,
            mapped: snap.mapped,
            asks: snap.asks,
            answered: snap.answered,
            refused: snap.refused.clone(),
            xid: snap.xid,
            input_delivered: snap.input_delivered,
            result_delivered: snap.result_delivered,
        }
    }
}

/// Where a slot's prepaint was: the list item it is in, and its place inside that item.
#[derive(Clone, Copy)]
struct Record {
    item_ix: usize,
    offset: Point<Pixels>,
    size: Size<Pixels>,
}

/// The chat's widgets.
#[derive(Default)]
pub(super) struct Widgets {
    lookups: Lookups<Vec<McpServer>, UiToolRef>,
    /// In call order.
    slots: Vec<Slot>,
    records: Rc<RefCell<HashMap<String, Record>>>,
    /// The top-left of the list item being prepainted, set by the probe at the item's start.
    origin: Rc<Cell<Point<Pixels>>>,
    /// Where the jump-to-latest disc was drawn this frame.
    disc: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The host covers the chat (a browser overlay is open).
    obscured: bool,
    /// The viewport of the last sweep: the clamp for a widget's height reads it.
    viewport: Cell<Bounds<Pixels>>,
    /// The window the chat was last drawn in, which a new webview needs.
    window: Option<AnyWindowHandle>,
    /// The asks the widgets have open, and the card each one is shown in.
    pub(super) asks: AskState,
}

impl Widgets {
    pub(super) fn has_slot(&self, id: &str) -> bool {
        self.slots.iter().any(|slot| slot.id == id)
    }

    pub(super) fn slot(&self, id: &str) -> Option<&Slot> {
        self.slots.iter().find(|slot| slot.id == id)
    }

    /// A new session: nothing of the old session's lookups stays.
    pub(super) fn reset_lookups(&mut self) {
        self.lookups.reset();
    }

    /// Every widget entity the slots hold.
    pub(super) fn entities(&self) -> Vec<Entity<McpWidget>> {
        self.slots.iter().filter_map(|slot| slot.widget.clone()).collect()
    }

    /// The servers the lookups last kept, for a widget's own calls.
    pub(super) fn known_servers(&self) -> Option<Vec<McpServer>> {
        self.lookups.list().cloned()
    }

    /// A reconnect, toggle or reload: the stored list is old, the next call asks again.
    pub(super) fn invalidate_lookups(&mut self) {
        self.lookups.invalidate();
    }

    pub(super) fn note_window(&mut self, window: AnyWindowHandle) {
        self.window = Some(window);
    }

    /// Whether a page was read for a slot that is waiting for a window to be built in.
    pub(super) fn has_waiting_read(&self) -> bool {
        self.slots.iter().any(|slot| slot.waiting_read.is_some())
    }

    /// The slot of a test, with no webview.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(super) fn insert_test_slot(&mut self, slot: Slot) {
        self.slots.push(slot);
    }

    /// A server list for a test, with no client to read it from.
    #[cfg(test)]
    pub(super) fn seed_test_servers(&mut self, servers: Vec<McpServer>) {
        self.lookups.seed(servers);
    }

    /// A page read that waits for a window, for a test.
    #[cfg(test)]
    pub(super) fn hold_test_read(&mut self, id: &str) {
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.id == id) {
            slot.waiting_read = Some((String::new(), Err(String::new())));
        }
    }

    fn max_height(&self) -> u32 {
        let viewport = f32::from(self.viewport.get().size.height);
        if viewport <= 0.0 {
            return MAX_HEIGHT;
        }
        (((viewport * 0.8) as u32).min(MAX_HEIGHT)).max(MIN_HEIGHT)
    }

    /// The height a widget's box has: what the view asked for, clamped.
    fn box_height(&self, asked: u32) -> u32 {
        asked.clamp(MIN_HEIGHT, self.max_height())
    }
}

/// The accepted origins, each once, comma-separated.
fn origins(csp: &UiCsp) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for origin in csp.connect.iter().chain(&csp.resource) {
        if !seen.contains(&origin.as_str()) {
            seen.push(origin);
        }
    }
    seen.iter().map(|origin| visible_text(origin, false)).collect::<Vec<_>>().join(",")
}

impl Chat {
    /// The turn the chat is in, for "at most one refresh per turn": the user messages so far.
    fn widget_turn(&self) -> u64 {
        self.entries.iter().filter(|entry| matches!(entry, Entry::User { .. })).count() as u64
    }

    /// One step of a main-thread MCP call.
    pub(super) fn apply_mcp_tool(&mut self, update: McpToolUpdate, cx: &mut Context<Self>) {
        let McpToolUpdate { id, item } = update;
        match item {
            McpToolItem::Called { name, input } => {
                if !widget_webview_available() {
                    return;
                }
                let turn = self.widget_turn();
                let client = &self.client;
                let steps = self.widgets.lookups.called(
                    &id,
                    &name,
                    input,
                    turn,
                    || client.as_ref().and_then(|client| client.mcp_servers()),
                    |list, name| ui_tool_for(list, name),
                );
                self.run_lookup_steps(steps, cx);
            }
            McpToolItem::Returned { result } => self.tell_call(&id, Update::Returned(result), cx),
            McpToolItem::Cancelled => self.tell_call(&id, Update::Cancelled, cx),
        }
    }

    fn run_lookup_steps(&mut self, steps: Vec<Step<UiToolRef>>, cx: &mut Context<Self>) {
        for step in steps {
            match step {
                Step::Refresh { ticket } => self.start_status_refresh(ticket, cx),
                Step::Hit { id, turn, hit, held } => self.open_slot(id, turn, hit, held, cx),
                // A call with no UI leaves nothing behind.
                Step::Miss { .. } => {}
            }
        }
    }

    /// Asks the CLI for the servers' tools, off the UI thread.
    fn start_status_refresh(&mut self, ticket: u64, cx: &mut Context<Self>) {
        let Some(control) = self.client.as_ref().and_then(|client| client.mcp_control()) else {
            self.status_refreshed(ticket, None, cx);
            return;
        };
        cx.spawn(async move |this, cx| {
            let answer = cx.background_spawn(async move { control.refresh_status() }).await;
            let _ = this.update(cx, |chat, cx| chat.status_refreshed(ticket, answer.ok(), cx));
        })
        .detach();
    }

    fn status_refreshed(
        &mut self,
        ticket: u64,
        answer: Option<Vec<McpServer>>,
        cx: &mut Context<Self>,
    ) {
        let steps =
            self.widgets.lookups.refreshed(ticket, answer, |list, name| ui_tool_for(list, name));
        self.run_lookup_steps(steps, cx);
    }

    /// A result or a cancellation of a call that already has a slot, or is still looked up.
    fn tell_call(&mut self, id: &str, update: Update, cx: &mut Context<Self>) {
        if self.widgets.lookups.holds(id) {
            self.widgets.lookups.absorb(id, update);
            return;
        }
        let Some(slot) = self.widgets.slots.iter_mut().find(|slot| slot.id == id) else {
            return;
        };
        match slot.widget.clone() {
            Some(widget) if slot.view().phase.is_live() => {
                widget.update(cx, |widget, cx| match update {
                    Update::Returned(result) => widget.tool_result(result, cx),
                    Update::Cancelled => widget.tool_cancelled(cx),
                });
                self.refresh_snapshot(id, cx);
            }
            Some(_) => {}
            None => {
                if let Some(held) = slot.held.as_mut() {
                    held.take(update);
                }
            }
        }
    }

    /// A call with a UI: its slot appears, and the page it names is read.
    fn open_slot(
        &mut self,
        id: String,
        turn: u64,
        hit: UiToolRef,
        held: Held,
        cx: &mut Context<Self>,
    ) {
        if self.widgets.has_slot(&id) {
            return;
        }
        self.widgets.slots.push(Slot::resolving(id.clone(), turn, &hit, held));
        self.slot_changed(&id, cx);
        let Some(control) = self.client.as_ref().and_then(|client| client.mcp_control()) else {
            self.fail_slot(&id, NO_SESSION.to_owned(), cx);
            return;
        };
        let UiToolRef { server, resource_uri, .. } = hit;
        cx.spawn(async move |this, cx| {
            let uri = resource_uri.clone();
            let answer = cx
                .background_spawn(async move { control.read_resource(&server, &uri) })
                .await
                .map_err(|error| format!("{error:#}"));
            let window = this.read_with(cx, |chat, _| chat.widgets.window).ok().flatten();
            let Some(window) = window else {
                // A chat that was never drawn (a prompt sent to a background tab) has no
                // window yet: the first frame builds the widget from this answer.
                let _ = this
                    .update(cx, |chat, cx| chat.hold_read(&id, resource_uri.clone(), answer, cx));
                return;
            };
            let _ = cx.update_window(window, |_, window, cx| {
                let _ = this.update(cx, |chat, cx| {
                    chat.resource_read(&id, &resource_uri, answer, window, cx);
                });
            });
        })
        .detach();
    }

    /// The answer to `read_resource`: a widget, or a one-line error.
    fn resource_read(
        &mut self,
        id: &str,
        uri: &str,
        answer: Result<serde_json::Value, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let waiting = self
            .widgets
            .slots
            .iter()
            .any(|slot| slot.id == id && slot.phase == SlotPhase::Resolving);
        if !waiting {
            return;
        }
        let document = answer.and_then(|contents| {
            widget_document(&contents, uri).map_err(|error| error.to_string())
        });
        let document = match document {
            Ok(document) => document,
            Err(error) => {
                self.fail_slot(id, visible_text(&error, true), cx);
                return;
            }
        };
        let domains = origins(&document.csp);
        let dropped = document
            .dropped
            .iter()
            .map(|entry| visible_text(entry, false))
            .collect::<Vec<_>>()
            .join("|");
        let context = HostContext {
            dark: Theme::get(cx).appearance == Appearance::Dark,
            width: self.widget_width(),
            max_height: self.widgets.max_height(),
            host_version: env!("CARGO_PKG_VERSION").to_owned(),
        };
        let widget = cx.new(|cx| McpWidget::new(document, context, window, cx));
        let subscriptions = vec![
            cx.subscribe(&widget, {
                let id = id.to_owned();
                move |chat, _, event: &WidgetEvent, cx| chat.widget_event(&id, event, cx)
            }),
            cx.observe(&widget, {
                let id = id.to_owned();
                move |chat, _, cx| {
                    if chat.refresh_snapshot(&id, cx) {
                        chat.remeasure_row(&id);
                    }
                    cx.notify();
                }
            }),
        ];
        let Some(slot) = self.widgets.slots.iter_mut().find(|slot| slot.id == id) else {
            return;
        };
        slot.domains = domains;
        slot.dropped = dropped;
        slot.phase = SlotPhase::Loading;
        slot.widget = Some(widget.clone());
        slot.subscriptions = subscriptions;
        let held = slot.held.take();
        // Input first, then the result, then a cancellation: the bridge holds them until the
        // view has initialized.
        for delivery in held.into_iter().flat_map(Held::deliveries) {
            widget.update(cx, |widget, cx| match delivery {
                Delivery::Input(input) => widget.tool_input(input, cx),
                Delivery::Result(result) => widget.tool_result(result, cx),
                Delivery::Cancelled => widget.tool_cancelled(cx),
            });
        }
        self.refresh_snapshot(id, cx);
        self.cap_live_widgets(cx);
        self.slot_changed(id, cx);
    }

    /// The read answered and the chat has no window yet: hold it for the first frame.
    fn hold_read(
        &mut self,
        id: &str,
        uri: String,
        answer: Result<serde_json::Value, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(slot) = self
            .widgets
            .slots
            .iter_mut()
            .find(|slot| slot.id == id && slot.phase == SlotPhase::Resolving)
        else {
            return;
        };
        slot.waiting_read = Some((uri, answer));
        // A frame builds it; one may be due already, so ask for another.
        cx.notify();
    }

    /// Builds the widgets whose page was read while the chat had no window.
    pub(super) fn build_waiting_widgets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ready: Vec<_> = self
            .widgets
            .slots
            .iter_mut()
            .filter_map(|slot| {
                let (uri, answer) = slot.waiting_read.take()?;
                Some((slot.id.clone(), uri, answer))
            })
            .collect();
        for (id, uri, answer) in ready {
            self.resource_read(&id, &uri, answer, window, cx);
        }
    }

    /// The width the view is told it has: the reading column less its padding.
    fn widget_width(&self) -> u32 {
        let column = self
            .chat_width()
            .map_or(TRANSCRIPT_WIDTH, |width| f32::from(width).min(TRANSCRIPT_WIDTH));
        (column - 2.0 * TRANSCRIPT_PAD_X).max(200.0) as u32
    }

    fn fail_slot(&mut self, id: &str, error: String, cx: &mut Context<Self>) {
        if let Some(slot) = self.widgets.slots.iter_mut().find(|slot| slot.id == id) {
            slot.phase = SlotPhase::Failed;
            slot.error = error;
        }
        self.slot_changed(id, cx);
    }

    pub(super) fn widget_event(&mut self, id: &str, event: &WidgetEvent, cx: &mut Context<Self>) {
        match event {
            WidgetEvent::Ask { ask, name, arguments } => {
                self.widget_asked(id, *ask, name.clone(), arguments.clone(), cx);
            }
            WidgetEvent::Resized(_) => {
                self.refresh_snapshot(id, cx);
                self.remeasure_row(id);
                cx.notify();
            }
            WidgetEvent::Closed(why) => {
                let Some(index) = self.widgets.slots.iter().position(|slot| slot.id == id) else {
                    return;
                };
                self.close_slot(index, visible_text(why, true), cx);
                self.slot_changed(id, cx);
            }
        }
    }

    /// Closes the widget of the slot at `index` and marks the slot closed. The widget's own
    /// `close` emits nothing, so the phase is set here.
    fn close_slot(&mut self, index: usize, error: String, cx: &mut Context<Self>) {
        // The ask is answered while the view may still be alive to get its denial, and no
        // card is left that an Allow could run a call from.
        let id = self.widgets.slots[index].id.clone();
        self.deny_widget_asks_of(&id, cx);
        let slot = &mut self.widgets.slots[index];
        slot.phase = SlotPhase::Closed;
        slot.error = error;
        if let Some(widget) = slot.widget.clone() {
            widget.update(cx, |widget, cx| widget.close(cx));
        }
        self.refresh_snapshot(&id, cx);
    }

    /// At most `MAX_LIVE` widgets are live: past that, the oldest live one is closed.
    pub(super) fn cap_live_widgets(&mut self, cx: &mut Context<Self>) {
        loop {
            let mut live = self
                .widgets
                .slots
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.view().phase.is_live())
                .map(|(index, _)| index);
            let Some(oldest) = live.next() else { return };
            if live.count() < MAX_LIVE {
                return;
            }
            let id = self.widgets.slots[oldest].id.clone();
            self.close_slot(oldest, String::new(), cx);
            self.slot_changed(&id, cx);
        }
    }

    /// Reads the widget's snapshot into the slot; true when the row's layout may have changed.
    pub(super) fn refresh_snapshot(&self, id: &str, cx: &App) -> bool {
        let Some(slot) = self.widgets.slot(id) else { return false };
        let Some(widget) = &slot.widget else { return false };
        let fresh = widget.read(cx).snapshot();
        let before = slot.view();
        *slot.snapshot.borrow_mut() = Some(fresh);
        let after = slot.view();
        before.phase != after.phase || before.height != after.height
    }

    /// The row of the call changed shape: measure it again, and the run it is drawn in.
    fn slot_changed(&mut self, id: &str, cx: &mut Context<Self>) {
        self.remeasure_row(id);
        cx.notify();
    }

    /// A run of tool calls is one box drawn at its tail item, so the whole run is measured.
    fn remeasure_row(&self, id: &str) {
        let position = self
            .entries
            .iter()
            .rposition(|entry| matches!(entry, Entry::ToolCall { id: row, .. } if row == id));
        let Some((start, end)) =
            position.and_then(|index| tool_call_run_bounds_inclusive(&self.entries, index))
        else {
            return;
        };
        self.list_state.remeasure_items(start..end + 1);
    }

    /// The transcript was replaced: the rows the slots belonged to are gone.
    pub(super) fn close_widgets(&mut self, cx: &mut Context<Self>) {
        self.deny_all_widget_asks(cx);
        for slot in &self.widgets.slots {
            if let Some(widget) = slot.widget.clone() {
                widget.update(cx, |widget, cx| widget.close(cx));
            }
        }
        self.widgets.slots.clear();
        // Calls still being looked up have no row either; their answers land on nothing.
        self.widgets.lookups.discard();
        self.widgets.records.borrow_mut().clear();
        self.widgets.disc.set(None);
        cx.notify();
    }

    /// The CLI went away or was replaced: every widget is closed, every open ask denied, and
    /// the rows say why. A page read for the old session is dropped, so nothing is built from
    /// it into the new one.
    pub(super) fn close_session_widgets(&mut self, why: &str, cx: &mut Context<Self>) {
        self.deny_all_widget_asks(cx);
        for index in 0..self.widgets.slots.len() {
            let slot = &mut self.widgets.slots[index];
            slot.waiting_read = None;
            if matches!(slot.phase, SlotPhase::Resolving | SlotPhase::Loading | SlotPhase::Ready) {
                let id = slot.id.clone();
                self.close_slot(index, why.to_owned(), cx);
                self.slot_changed(&id, cx);
            }
        }
    }

    // --- Placement ---

    /// Hides every widget at once: the host took this chat off screen.
    pub fn unmap_widgets(&self, cx: &mut Context<Self>) {
        if self.widgets.slots.is_empty() {
            return;
        }
        self.hide_widgets(cx);
    }

    /// The host covers this chat (or no longer does). Hides at once; notifies only when the
    /// flag changes, so the sweep of the next frame places the widgets again.
    pub fn set_widgets_obscured(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.widgets.obscured == on {
            return;
        }
        self.widgets.obscured = on;
        if on {
            self.hide_widgets(cx);
        }
        cx.notify();
    }

    /// Hides every widget at once, and records that the host did: the read reports what the
    /// host saw until the next paint's sweep sets it again from the host's overlays.
    fn hide_widgets(&self, cx: &mut Context<Self>) {
        for slot in &self.widgets.slots {
            let mut seen = slot.seen.get();
            seen.hidden = true;
            slot.seen.set(seen);
            let Some(widget) = &slot.widget else { continue };
            widget.update(cx, |widget, cx| widget.place(None, cx));
            self.refresh_snapshot(&slot.id, cx);
        }
    }

    /// Whether anything is open that the chat draws over the transcript, or a drag runs.
    pub(super) fn chat_overlay_open(&self, cx: &App) -> bool {
        self.model_picker_open
            || self.mode_picker_open
            || self.context_popover_open
            || self.background_popover_open
            || self.effort_picker_open
            || self.thinking_picker_open
            || self.overflow_open
            || self.history_open
            || self.history_delete_confirm.is_some()
            || self.bypass_ask.is_some()
            || self.pending_rewind.is_some()
            // A popup that is closing is still on screen until its exit has played.
            || self.composer_context_menu.get().is_some()
            || self.transcript_context_menu.get().is_some()
            || self.slash_popup_visible()
            || self.mention_popup_visible()
            || matches!(self.session_panel_placement(), Some(Placement::Overlay(_)))
            // An external file drag shows the "Drop files to attach" overlay; any drag
            // unmaps, because a pointer over a webview would never reach the drag.
            || cx.has_active_drag()
    }

    /// The canvas after the list: runs the sweep once the list has set its viewport.
    pub(super) fn widget_sweep(&self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        canvas(
            move |_, _, cx| {
                let plan = entity.read(cx).plan_widgets(cx);
                for (widget, at) in plan {
                    widget.update(cx, |widget, cx| widget.place(at, cx));
                }
                let chat = entity.read(cx);
                for slot in &chat.widgets.slots {
                    chat.refresh_snapshot(&slot.id, cx);
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_0()
        .into_any_element()
    }

    /// Where each widget goes this frame: `Some` only for a slot laid out wholly inside the
    /// viewport with nothing over it. Every other widget is hidden, including one with no
    /// record this frame. Also stores what was seen, and clears the records.
    fn plan_widgets(&self, cx: &App) -> Vec<(Entity<McpWidget>, Option<Bounds<Pixels>>)> {
        let widgets = &self.widgets;
        let records = std::mem::take(&mut *widgets.records.borrow_mut());
        let disc = widgets.disc.take();
        // A sweep is a paint, so the chat is on screen: only the host's overlays hide it.
        let hidden = widgets.obscured;
        if widgets.slots.is_empty() {
            return Vec::new();
        }
        let viewport = self.list_state.viewport_bounds();
        widgets.viewport.set(viewport);
        let overlay_open = self.chat_overlay_open(cx);
        let dock_open = question_view(&self.entries).is_some();
        let mut plan = Vec::new();
        for slot in &widgets.slots {
            let placed = records.get(&slot.id).and_then(|record| {
                let item = self.list_state.bounds_for_item(record.item_ix)?;
                Some(Bounds::new(item.origin + record.offset, record.size))
            });
            let mut seen = slot.seen.get();
            seen.swept = true;
            seen.drawn = placed.is_some();
            seen.viewport = viewport;
            seen.overlay_open = overlay_open;
            seen.dock_open = dock_open;
            seen.hidden = hidden;
            if placed.is_some() {
                seen.slot = placed;
            }
            slot.seen.set(seen);
            let Some(widget) = &slot.widget else { continue };
            let at = placed.filter(|bounds| {
                !overlay_open
                    && !dock_open
                    && !hidden
                    && inside(&viewport, bounds)
                    && disc.is_none_or(|disc| !bounds.intersects(&disc))
            });
            plan.push((widget.clone(), at));
        }
        plan
    }

    // --- The row ---

    /// The zero-size probe at the start of a list item holding a slot: it tells the slots
    /// after it where their item begins.
    pub(super) fn widget_item_probe(&self, start: usize, end: usize) -> Option<AnyElement> {
        let drawn = (start..=end).any(|index| {
            matches!(self.entries.get(index), Some(Entry::ToolCall { id, .. })
                if self.widgets.has_slot(id))
        });
        if !drawn {
            return None;
        }
        let origin = self.widgets.origin.clone();
        Some(
            canvas(move |bounds, _, _| origin.set(bounds.origin), |_, _, _, _| {})
                .absolute()
                .top_0()
                .left_0()
                .size_0()
                .into_any_element(),
        )
    }

    /// A canvas that stores where the jump-to-latest disc was drawn this frame.
    pub(super) fn disc_probe(&self) -> AnyElement {
        let disc = self.widgets.disc.clone();
        canvas(move |bounds, _, _| disc.set(Some(bounds)), |_, _, _, _| {})
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .into_any_element()
    }

    /// Whether this call is drawn with a slot of its own, apart from any `Verb · N` fold.
    pub(super) fn call_has_slot(&self, id: &str) -> bool {
        self.widgets.has_slot(id)
    }

    /// The slot under a call's row: its header, then the widget's box or one line of text.
    pub(super) fn widget_slot_element(
        &self,
        id: &str,
        entry_index: usize,
        theme: &Theme,
        bezel_theme: &bezel_theme::Theme,
    ) -> Option<AnyElement> {
        let slot = self.widgets.slot(id)?;
        let view = slot.view();
        let item_ix = tool_call_run_bounds_inclusive(&self.entries, entry_index)
            .map_or(entry_index, |(_, end)| end);
        let title = format!("{} · {}", slot.server, slot.tool);
        let reach = if slot.domains.is_empty() {
            "No network".to_owned()
        } else {
            format!("Can reach: {}", slot.domains)
        };
        let line = |text: String| {
            div()
                .w_full()
                .px(px(12.0))
                .py(px(8.0))
                .text_size(theme.typography.footnote)
                .text_color(theme.ely.fg_muted)
                .child(gpui::SharedString::from(text))
        };
        let body = match view.phase {
            SlotPhase::Failed => line(format!("{title}: {}", view.error)).into_any_element(),
            SlotPhase::Closed if view.error.is_empty() => {
                line(format!("{title}: widget closed")).into_any_element()
            }
            SlotPhase::Closed => {
                line(format!("{title}: widget closed ({})", view.error)).into_any_element()
            }
            SlotPhase::Resolving => line(format!("{title}: loading the widget")).into_any_element(),
            SlotPhase::Loading | SlotPhase::Ready => {
                let height = self.widgets.box_height(if view.phase == SlotPhase::Loading {
                    LOADING_HEIGHT
                } else {
                    view.height
                });
                let records = self.widgets.records.clone();
                let origin = self.widgets.origin.clone();
                let key = id.to_owned();
                let probe = canvas(
                    move |bounds, _, _| {
                        records.borrow_mut().insert(
                            key,
                            Record {
                                item_ix,
                                offset: bounds.origin - origin.get(),
                                size: bounds.size,
                            },
                        );
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full();
                let header = div()
                    .w_full()
                    .px(px(12.0))
                    .py(px(6.0))
                    .flex()
                    .flex_row()
                    .gap(px(12.0))
                    .text_size(theme.typography.footnote)
                    .child(
                        div()
                            .flex_none()
                            .text_color(theme.ely.fg_muted)
                            .child(gpui::SharedString::from(title)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(theme.ely.fg_subtle)
                            .child(gpui::SharedString::from(reach)),
                    );
                let hidden = if view.mapped {
                    None
                } else {
                    Some(gpui::SharedString::from("Widget hidden while it is not fully in view"))
                };
                let selector = format!("mcp-widget-box-{id}");
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(
                        div()
                            .relative()
                            .debug_selector(move || selector.clone())
                            .w_full()
                            .h(px(height as f32))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(theme.typography.footnote)
                            .text_color(theme.ely.fg_subtle)
                            .children(hidden)
                            .child(probe),
                    )
                    .into_any_element()
            }
        };
        Some(
            div()
                .w_full()
                .border_t_1()
                .border_color(bezel_theme.border)
                .child(body)
                .into_any_element(),
        )
    }

    // --- The read ---

    /// The `mcpWidgets` rows of `surface.chat.read`, in call order.
    pub(super) fn widget_rows(&self) -> Vec<BTreeMap<String, String>> {
        let turns = segment_turns(&self.entries);
        let flag = |on: bool| if on { "1" } else { "0" }.to_owned();
        let top = |bounds: Option<Bounds<Pixels>>| {
            bounds.map(|bounds| f32::from(bounds.top()).to_string()).unwrap_or_default()
        };
        let bottom = |bounds: Option<Bounds<Pixels>>| {
            bounds.map(|bounds| f32::from(bounds.bottom()).to_string()).unwrap_or_default()
        };
        self.widgets
            .slots
            .iter()
            .map(|slot| {
                let view = slot.view();
                let seen = slot.seen.get();
                let row = self.entries.iter().rposition(
                    |entry| matches!(entry, Entry::ToolCall { id, .. } if *id == slot.id),
                );
                let footer = row
                    .and_then(|row| turns.iter().find(|turn| turn.start <= row && row <= turn.end))
                    .and_then(|turn| turn.footer);
                let viewport = seen.swept.then_some(seen.viewport);
                let refused = view
                    .refused
                    .iter()
                    .map(|entry| visible_text(entry, false))
                    .collect::<Vec<_>>()
                    .join("|");
                BTreeMap::from([
                    ("toolUseId".to_owned(), slot.id.clone()),
                    ("server".to_owned(), slot.server.clone()),
                    ("tool".to_owned(), slot.tool.clone()),
                    ("turn".to_owned(), slot.turn.to_string()),
                    ("turnFooter".to_owned(), footer.map(|f| f.to_string()).unwrap_or_default()),
                    ("phase".to_owned(), view.phase.as_str().to_owned()),
                    ("error".to_owned(), view.error.clone()),
                    ("mapped".to_owned(), flag(view.mapped)),
                    ("height".to_owned(), view.height.to_string()),
                    ("slotDrawn".to_owned(), flag(seen.drawn)),
                    ("slotTop".to_owned(), top(seen.slot)),
                    ("slotBottom".to_owned(), bottom(seen.slot)),
                    ("viewportTop".to_owned(), top(viewport)),
                    ("viewportBottom".to_owned(), bottom(viewport)),
                    ("overlayOpen".to_owned(), flag(seen.overlay_open)),
                    ("dockOpen".to_owned(), flag(seen.dock_open)),
                    ("hidden".to_owned(), flag(seen.hidden)),
                    ("xid".to_owned(), view.xid.map(|xid| xid.to_string()).unwrap_or_default()),
                    ("domains".to_owned(), slot.domains.clone()),
                    ("dropped".to_owned(), slot.dropped.clone()),
                    ("inputDelivered".to_owned(), flag(view.input_delivered)),
                    ("resultDelivered".to_owned(), flag(view.result_delivered)),
                    ("asks".to_owned(), view.asks.to_string()),
                    ("answered".to_owned(), view.answered.to_string()),
                    ("refused".to_owned(), refused),
                ])
            })
            .collect()
    }
}

/// Whether `inner` lies wholly inside `outer`.
fn inside(outer: &Bounds<Pixels>, inner: &Bounds<Pixels>) -> bool {
    inner.left() >= outer.left()
        && inner.right() <= outer.right()
        && inner.top() >= outer.top()
        && inner.bottom() <= outer.bottom()
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;
    use serde_json::json;
    use sirio_chat::{McpOrigin, UiToolRef};

    use super::*;
    use crate::chat::tests::refresh_frame;
    use crate::chat::testing::{offline_chat, prepare};

    fn tool() -> UiToolRef {
        UiToolRef {
            server: "s".into(),
            origin: McpOrigin::User,
            tool: "show".into(),
            resource_uri: "ui://s/show".into(),
        }
    }

    /// How it can fail, written first: a chat that was never drawn has no window, so a page
    /// read for it either fails its slot for good, or is built inside the render that finds
    /// the window, or waits for a frame that nothing asks for.
    #[gpui::test]
    async fn a_page_read_before_the_first_frame_waits_for_the_chat_to_be_drawn(
        cx: &mut TestAppContext,
    ) {
        prepare(cx);
        let (chat, cx) = cx.add_window_view(|_, cx| offline_chat(cx));
        chat.update(cx, |chat, cx| {
            let held = Held::new(json!({}));
            chat.widgets.insert_test_slot(Slot::resolving("a".into(), 1, &tool(), held));
            // The server named something that is not a widget page: building it fails at
            // once, with no webview, which is how this test sees the build happen.
            let contents = json!([{ "uri": "ui://s/show", "mimeType": "text/plain", "text": "x" }]);
            // The harness drew the chat once already: forget the window it noted.
            chat.widgets.window = None;
            chat.hold_read("a", "ui://s/show".into(), Ok(contents), cx);
            assert!(chat.widgets.window.is_none());
            let slot = chat.widgets.slot("a").expect("the slot exists");
            assert_eq!(slot.phase, SlotPhase::Resolving);
            assert!(slot.waiting_read.is_some());
        });
        refresh_frame(cx);
        chat.read_with(cx, |chat, _| {
            let slot = chat.widgets.slot("a").expect("the slot exists");
            assert_eq!(slot.phase, SlotPhase::Failed);
            assert!(slot.waiting_read.is_none());
        });
    }

    /// How it can fail, written first: the host takes a chat off screen and `unmap_widgets`
    /// hides the widgets, but the read, which reports what the last sweep and the host saw,
    /// still says `hidden "0"` for a slot with no webview, or until the next paint.
    #[gpui::test]
    async fn a_chat_taken_off_screen_reports_its_widgets_hidden_until_painted_again(
        cx: &mut TestAppContext,
    ) {
        prepare(cx);
        let (chat, cx) = cx.add_window_view(|_, cx| offline_chat(cx));
        let hidden = |chat: &Chat| {
            let row = chat.widget_rows().remove(0);
            (row["hidden"].clone(), row["mapped"].clone())
        };
        chat.update(cx, |chat, cx| {
            let held = Held::new(json!({}));
            chat.widgets.insert_test_slot(Slot::resolving("a".into(), 1, &tool(), held));
            chat.plan_widgets(cx);
            assert_eq!(hidden(chat), ("0".to_owned(), "0".to_owned()));
            chat.unmap_widgets(cx);
            assert_eq!(hidden(chat), ("1".to_owned(), "0".to_owned()));
            chat.plan_widgets(cx);
            assert_eq!(hidden(chat).0, "0");
        });
    }
}
