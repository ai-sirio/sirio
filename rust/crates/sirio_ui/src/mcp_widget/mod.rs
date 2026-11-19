//! MCP Apps widgets: an isolated webview per widget.
//!
//! Widgets exist only on Linux under X11, so everything that touches wry or GTK is
//! compiled there only. The bridge, the event types and the availability flag exist on every
//! target, so the chat compiles everywhere.

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod bridge;
#[cfg(target_os = "linux")]
mod webview;

#[cfg(target_os = "linux")]
pub use webview::{
    Inbox, RELAY_SCHEME, VIEW_SCHEME, WidgetInbox, WidgetPage, accepts_drops, build_widget_child,
    deliver_script, flush_x, new_secret, pump_gtk, relay_url, view_url,
};

#[cfg_attr(not(target_os = "linux"), path = "view_stub.rs")]
mod view;

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

pub use bridge::AskOutcome;
pub use view::McpWidget;

#[derive(Debug, Clone, PartialEq)]
pub enum WidgetPhase {
    Loading,
    Ready,
    Failed(String),
    Closed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WidgetEvent {
    /// The widget asks to call a tool; settle it with `McpWidget::answer_ask`.
    Ask { ask: u64, name: String, arguments: Value },
    /// The height the view asked for, in CSS pixels; the chat clamps it.
    Resized(u32),
    /// The bridge closed the widget (three denials, or a flood), with the reason. An explicit
    /// `McpWidget::close` does not emit this.
    Closed(String),
}

/// What an outside observer (the chat, the E2E) may read off a widget.
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetSnapshot {
    pub phase: WidgetPhase,
    /// The last height the view asked for; 160 while loading.
    pub height: u32,
    /// Whether the webview is shown.
    pub mapped: bool,
    pub asks: u32,
    pub answered: u32,
    /// `"<method>:<tool or empty>:<reason>"`, newest last, capped.
    pub refused: Vec<String>,
    /// The webview's X window id, for `xwininfo -id`; `None` without one.
    pub xid: Option<u64>,
    /// Whether the bridge has sent `tool-input` / `tool-result` to the view.
    pub input_delivered: bool,
    pub result_delivered: bool,
}

static WIDGET_WEBVIEW_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Set once at startup by `main.rs`, like `browser::set_profile_dir`. Ignored off Linux.
pub fn set_widget_webview_available(on: bool) {
    WIDGET_WEBVIEW_AVAILABLE.store(on && cfg!(target_os = "linux"), Ordering::SeqCst);
}

/// True only on Linux after `set_widget_webview_available(true)`.
pub fn widget_webview_available() -> bool {
    WIDGET_WEBVIEW_AVAILABLE.load(Ordering::SeqCst)
}
