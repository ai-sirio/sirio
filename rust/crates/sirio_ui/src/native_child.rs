//! A native X11 child window of a GPUI window, for the webviews that live in one.
//!
//! GPUI's Linux backend exposes its X11 surface as XCB, while wry's WebKitGTK
//! backend accepts only an Xlib window ID. The ID is shared by both X11 APIs,
//! so [`XlibParent`] is the narrow adapter, and [`build_in_window`] tries the
//! GPUI handle directly first and the adapter second. The browser surface and
//! the MCP widget webview both build through it.

use std::ffi::c_ulong;

use gpui::Window;
use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, WindowHandle, XlibWindowHandle,
};

/// The XID of a live GPUI window, presented as an Xlib handle.
#[derive(Clone, Copy)]
pub struct XlibParent {
    window: c_ulong,
}

impl XlibParent {
    pub fn from_gpui(window: &Window) -> Result<Self, String> {
        let handle = HasWindowHandle::window_handle(window)
            .map_err(|error| format!("GPUI window handle unavailable: {error}"))?;
        match handle.as_raw() {
            RawWindowHandle::Xcb(handle) => Ok(Self {
                window: c_ulong::from(handle.window.get()),
            }),
            raw => Err(format!("GPUI returned unsupported handle: {raw:?}")),
        }
    }
}

impl HasWindowHandle for XlibParent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // The XID is copied from a live GPUI window and remains valid while
        // the caller owns that window. XIDs are explicitly allowed to be
        // borrowed as raw handles by raw-window-handle.
        Ok(unsafe {
            WindowHandle::borrow_raw(RawWindowHandle::Xlib(XlibWindowHandle::new(self.window)))
        })
    }
}

/// Calls `build` with the GPUI window itself, and on failure with the Xlib adapter.
/// Callers must have run `gtk::init()` first. The flag is true when the adapter built it.
pub fn build_in_window<T>(
    window: &Window,
    build: impl Fn(&dyn HasWindowHandle) -> Result<T, wry::Error>,
) -> Result<(T, bool), String> {
    match build(window) {
        Ok(value) => Ok((value, false)),
        Err(direct_error) => match XlibParent::from_gpui(window) {
            Ok(parent) => build(&parent).map(|value| (value, true)).map_err(|bridge_error| {
                format!(
                    "Direct XCB build failed: {direct_error}; XCB→Xlib build failed: {bridge_error}"
                )
            }),
            Err(bridge_error) => Err(format!(
                "Direct XCB build failed: {direct_error}; XCB→Xlib adapter failed: {bridge_error}"
            )),
        },
    }
}
