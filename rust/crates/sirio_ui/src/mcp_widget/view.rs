//! The native widget view: one webview per widget, driven by the bridge.
//!
//! All widgets share one 16 ms task that runs GTK for at most 4 ms, then moves each widget's
//! inbox through its bridge. A widget notifies only when something happened.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, EventEmitter, Pixels, WeakEntity,
    Window,
};
use gtk::prelude::*;
use serde_json::Value;
use sirio_chat::mcp_apps::{bridge::HostContext, csp::view_csp, document::WidgetDocument};
use wry::{
    Rect, WebView, WebViewExtUnix,
    dpi::{LogicalPosition, LogicalSize},
};

use super::{
    AskOutcome, WidgetEvent, WidgetPhase, WidgetSnapshot,
    bridge::{BridgeEffect, WidgetBridge},
    webview::{
        Inbox, WidgetInbox, WidgetPage, build_widget_child, deliver_script, flush_x, new_secret,
        pump_gtk,
    },
};

const TICK: Duration = Duration::from_millis(16);
const GTK_BUDGET: Duration = Duration::from_millis(4);
/// A closing widget's webview is dropped this long after `close`, answered or not.
const TEARDOWN_GRACE: Duration = Duration::from_secs(1);
const LOADING_HEIGHT: u32 = 160;
const MAX_REFUSALS: usize = 100;

thread_local! {
    static REGISTRY: RefCell<Vec<WeakEntity<McpWidget>>> = const { RefCell::new(Vec::new()) };
    static PUMP_ARMED: Cell<bool> = const { Cell::new(false) };
}

pub struct McpWidget {
    bridge: WidgetBridge,
    webview: Option<WebView>,
    inbox: WidgetInbox,
    phase: WidgetPhase,
    height: u32,
    /// Where the webview is shown, in layout pixels; `None`: hidden.
    target: Option<Bounds<Pixels>>,
    window: AnyWindowHandle,
    scale: f64,
    asks: u32,
    answered: u32,
    refused: Vec<String>,
    /// Messages the view posted outside its relay, over the widget's life.
    forged: u32,
    closing_since: Option<Instant>,
}

impl EventEmitter<WidgetEvent> for McpWidget {}

impl McpWidget {
    pub fn new(
        doc: WidgetDocument,
        ctx: HostContext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let inbox: WidgetInbox = Rc::new(RefCell::new(Inbox::default()));
        let page = Arc::new(WidgetPage {
            key: new_secret()[..16].to_owned(),
            secret: new_secret(),
            html: doc.html,
            view_csp: view_csp(&doc.csp),
        });
        let scale = f64::from(window.scale_factor());
        let (webview, phase) = match build_widget_child(window, page, inbox.clone(), true) {
            Ok(webview) => (Some(webview), WidgetPhase::Loading),
            Err(error) => (None, WidgetPhase::Failed(error.to_string())),
        };
        if webview.is_some() {
            let app: &App = cx;
            register(cx.weak_entity(), app);
        }
        Self {
            bridge: WidgetBridge::new(ctx),
            webview,
            inbox,
            phase,
            height: LOADING_HEIGHT,
            target: None,
            window: window.window_handle(),
            scale: if scale.is_finite() && scale > 0.0 { scale } else { 1.0 },
            asks: 0,
            answered: 0,
            refused: Vec::new(),
            forged: 0,
            closing_since: None,
        }
    }

    pub fn tool_input(&mut self, arguments: Value, cx: &mut Context<Self>) {
        let effects = self.bridge.tool_input(arguments);
        self.apply(effects, cx);
    }

    pub fn tool_result(&mut self, result: Value, cx: &mut Context<Self>) {
        let effects = self.bridge.tool_result(result);
        self.apply(effects, cx);
    }

    pub fn tool_cancelled(&mut self, cx: &mut Context<Self>) {
        let effects = self.bridge.tool_cancelled("cancelled");
        self.apply(effects, cx);
    }

    pub fn answer_ask(&mut self, ask: u64, outcome: AskOutcome, cx: &mut Context<Self>) {
        let effects = self.bridge.answer_ask(ask, outcome, Instant::now());
        if !effects.is_empty() {
            self.answered += 1;
        }
        self.apply(effects, cx);
    }

    /// `Some(bounds)`: show there; `None`: hide now. An unchanged target does nothing, and
    /// placement never notifies.
    pub fn place(&mut self, at: Option<Bounds<Pixels>>, _cx: &mut Context<Self>) {
        if self.target == at {
            return;
        }
        let Some(webview) = &self.webview else { return };
        if self.closing_since.is_some() {
            return;
        }
        self.target = at;
        match at {
            Some(bounds) => {
                let _ = webview.set_bounds(self.rect(bounds));
                let _ = webview.set_visible(true);
            }
            None => {
                let _ = webview.set_visible(false);
                flush_x();
            }
        }
    }

    /// Asks the view to tear down, hides it, and drops the webview at its answer or after 1 s.
    /// Idempotent. It emits no event: the caller knows it closed.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.begin_close();
        cx.notify();
    }

    pub fn record_refusal(&mut self, entry: String) {
        if self.refused.len() >= MAX_REFUSALS {
            self.refused.remove(0);
        }
        self.refused.push(entry);
    }

    pub fn snapshot(&self) -> WidgetSnapshot {
        WidgetSnapshot {
            phase: self.phase.clone(),
            height: self.height,
            mapped: self.target.is_some() && self.webview.is_some(),
            asks: self.asks,
            answered: self.answered,
            refused: self.refused.clone(),
            xid: self.webview.as_ref().and_then(x11_id),
            input_delivered: self.bridge.input_delivered(),
            result_delivered: self.bridge.result_delivered(),
            forged: self.forged,
        }
    }

    /// Physical pixels, as the browser surface places its webview: wry passes a logical rect
    /// through to the raw X11 window.
    fn rect(&self, bounds: Bounds<Pixels>) -> Rect {
        let scale = self.scale;
        Rect {
            position: LogicalPosition::new(
                f64::from(bounds.origin.x) * scale,
                f64::from(bounds.origin.y) * scale,
            )
            .into(),
            size: LogicalSize::new(
                f64::from(bounds.size.width) * scale,
                f64::from(bounds.size.height) * scale,
            )
            .into(),
        }
    }

    /// Re-reads the window's scale; a changed one re-places a shown webview, because the
    /// logical bounds the chat holds did not change and `place` will not run again.
    fn refresh_scale(&mut self, cx: &mut Context<Self>) {
        let read = |_: gpui::AnyView, window: &mut Window, _: &mut App| {
            f64::from(window.scale_factor())
        };
        let Ok(scale) = cx.update_window(self.window, read) else {
            return;
        };
        if !scale.is_finite() || scale <= 0.0 || scale == self.scale {
            return;
        }
        self.scale = scale;
        if let (Some(webview), Some(target)) = (&self.webview, self.target) {
            let _ = webview.set_bounds(self.rect(target));
        }
    }

    fn begin_close(&mut self) {
        if self.closing_since.is_some() || self.webview.is_none() {
            return;
        }
        self.closing_since = Some(Instant::now());
        if self.target.take().is_some() {
            if let Some(webview) = &self.webview {
                let _ = webview.set_visible(false);
            }
            flush_x();
        }
        if !matches!(self.phase, WidgetPhase::Failed(_)) {
            self.phase = WidgetPhase::Closed;
        }
        for effect in self.bridge.teardown() {
            match effect {
                BridgeEffect::Send(text) => self.deliver(&text),
                BridgeEffect::TornDown => self.drop_webview(),
                _ => {}
            }
        }
    }

    fn drop_webview(&mut self) {
        self.target = None;
        self.closing_since = None;
        if let Some(webview) = self.webview.take() {
            drop(webview);
            flush_x();
        }
        if !matches!(self.phase, WidgetPhase::Failed(_)) {
            self.phase = WidgetPhase::Closed;
        }
    }

    fn deliver(&self, text: &str) {
        if let Some(webview) = &self.webview {
            let _ = webview.evaluate_script(&deliver_script(text));
        }
    }

    /// Runs the effects; true when the snapshot changed.
    fn apply(&mut self, effects: Vec<BridgeEffect>, cx: &mut Context<Self>) -> bool {
        let mut changed = false;
        for effect in effects {
            match effect {
                BridgeEffect::Send(text) => self.deliver(&text),
                BridgeEffect::Ask { ask, name, arguments } => {
                    self.asks += 1;
                    changed = true;
                    cx.emit(WidgetEvent::Ask { ask, name, arguments });
                }
                BridgeEffect::Resize(height) => {
                    changed |= self.height != height;
                    self.height = height;
                    cx.emit(WidgetEvent::Resized(height));
                }
                BridgeEffect::Refused(entry) => {
                    self.record_refusal(entry);
                    changed = true;
                }
                BridgeEffect::TornDown => {
                    self.drop_webview();
                    changed = true;
                }
                BridgeEffect::Close(why) => {
                    self.begin_close();
                    changed = true;
                    cx.emit(WidgetEvent::Closed(why));
                }
            }
        }
        if changed {
            cx.notify();
        }
        changed
    }

    /// One pump tick for this widget: its inbox through the bridge, and the teardown grace.
    fn tick(&mut self, cx: &mut Context<Self>) {
        if self.webview.is_none() {
            return;
        }
        self.refresh_scale(cx);
        let now = Instant::now();
        let (messages, forged, dropped): (Vec<String>, u32, u32) = {
            let mut inbox = self.inbox.borrow_mut();
            let messages = inbox.messages.drain(..).collect();
            (messages, std::mem::take(&mut inbox.forged), std::mem::take(&mut inbox.dropped))
        };
        let mut changed = false;
        if forged > 0 {
            self.forged = self.forged.saturating_add(forged);
            let effects = self.bridge.forged();
            self.apply(effects, cx);
            changed = true;
        }
        let effects = self.bridge.stray(forged.saturating_add(dropped), now);
        changed |= self.apply(effects, cx);
        for text in messages {
            let effects = self.bridge.on_message(&text, now);
            changed |= self.apply(effects, cx);
        }
        if self.phase == WidgetPhase::Loading && self.bridge.ready() {
            self.phase = WidgetPhase::Ready;
            changed = true;
        }
        if self.closing_since.is_some_and(|since| now.duration_since(since) >= TEARDOWN_GRACE) {
            self.drop_webview();
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }
}

impl Drop for McpWidget {
    fn drop(&mut self) {
        self.drop_webview();
    }
}

/// The X window wry maps and unmaps for this webview: the GTK toplevel's own window, which
/// wry sets to the child window it created.
fn x11_id(webview: &WebView) -> Option<u64> {
    let toplevel = webview.webview().toplevel()?;
    let window = toplevel.window()?;
    let window = window.downcast::<gdkx11::X11Window>().ok()?;
    Some(window.xid() as u64)
}

/// Adds a widget to the shared pump and arms the pump task if it is not running.
fn register(widget: WeakEntity<McpWidget>, app: &App) {
    REGISTRY.with_borrow_mut(|registry| registry.push(widget));
    if PUMP_ARMED.replace(true) {
        return;
    }
    app.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(TICK).await;
            if !cx.update(pump_tick) {
                break;
            }
        }
    })
    .detach();
}

/// Runs GTK for at most 4 ms, then each live widget's tick. False when no widget with a
/// webview is left, which also disarms the pump.
fn pump_tick(app: &mut App) -> bool {
    pump_gtk(GTK_BUDGET);
    let live: Vec<_> = REGISTRY
        .with_borrow(|registry| registry.iter().filter_map(WeakEntity::upgrade).collect());
    for widget in &live {
        widget.update(app, |widget, cx| widget.tick(cx));
    }
    let any = {
        let kept: Vec<_> = live
            .into_iter()
            .filter(|widget| widget.read(app).webview.is_some())
            .map(|widget| widget.downgrade())
            .collect();
        let any = !kept.is_empty();
        REGISTRY.with_borrow_mut(|registry| *registry = kept);
        any
    };
    if !any {
        PUMP_ARMED.set(false);
    }
    any
}
