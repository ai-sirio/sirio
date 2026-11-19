//! MCP Apps widgets: an isolated webview per widget.
//!
//! Widgets exist only on Linux under X11, so everything that touches wry or GTK is
//! compiled there only.

#[cfg(target_os = "linux")]
mod webview;

#[cfg(target_os = "linux")]
pub use webview::{
    Inbox, RELAY_SCHEME, VIEW_SCHEME, WidgetInbox, WidgetPage, accepts_drops, build_widget_child,
    deliver_script, flush_x, new_secret, pump_gtk, relay_url, view_url,
};
