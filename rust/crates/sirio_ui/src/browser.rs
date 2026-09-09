//! P72 browser-composition surface (WKWebView/WebKitGTK via `wry` in a native
//! child window, composited alongside GPUI's own surface).
//!
//! This module is part of the production module graph on both Linux and
//! macOS. GTK/X11 setup remains Linux-only; macOS uses the AppKit handle
//! exposed by GPUI directly.
//! The `browser_spike`/`browser_surface` examples and `crates/sirio`'s
//! panes both use it directly.

use std::path::PathBuf;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    rc::Rc,
    sync::mpsc,
    time::Duration,
};

#[cfg(target_os = "linux")]
use std::{ffi::c_ulong, time::Instant};

use bezel::ui::input::TextField;
use gpui::{
    App, Bounds, Context, Element, ElementId, FocusHandle, Focusable, GlobalElementId,
    InspectorElementId, IntoElement, KeyDownEvent, LayoutId, Pixels, Render, Style, Task, Window,
    div, prelude::*, px, relative,
};
use raw_window_handle::HasWindowHandle;
#[cfg(target_os = "windows")]
use raw_window_handle::{HandleError, RawWindowHandle, Win32WindowHandle, WindowHandle};
#[cfg(target_os = "linux")]
use raw_window_handle::{HandleError, RawWindowHandle, WindowHandle, XlibWindowHandle};
use sirio_theme::Theme;
#[cfg(target_os = "windows")]
use std::num::NonZeroIsize;

use sirio_ui::loading;
use wry::{
    NewWindowFeatures, NewWindowResponse, PageLoadEvent, Rect, WebContext, WebView, WebViewBuilder,
    dpi::{LogicalPosition, LogicalSize},
};

/// GPUI's Linux backend exposes its X11 surface as XCB, while wry's
/// WebKitGTK backend currently accepts only an Xlib window ID. The ID is
/// shared by both X11 APIs, so this adapter tests that narrow seam without
/// pretending the two handle types are interchangeable in general.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
struct XlibParent {
    window: c_ulong,
}

#[cfg(target_os = "linux")]
impl XlibParent {
    fn from_gpui(window: &Window) -> Result<Self, String> {
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

#[cfg(target_os = "linux")]
impl HasWindowHandle for XlibParent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // The XID is copied from a live GPUI window and remains valid while
        // the spike owns that window. XIDs are explicitly allowed to be
        // borrowed as raw handles by raw-window-handle.
        Ok(unsafe {
            WindowHandle::borrow_raw(RawWindowHandle::Xlib(XlibWindowHandle::new(self.window)))
        })
    }
}

fn build_webview<W: HasWindowHandle>(parent: &W) -> Result<WebView, wry::Error> {
    WebViewBuilder::new()
        .with_url("https://example.com")
        .with_bounds(Rect {
            position: LogicalPosition::new(336_i32, 52_i32).into(),
            size: LogicalSize::new(1098_i32, 728_i32).into(),
        })
        .build_as_child(parent)
}

#[cfg(target_os = "linux")]
fn build_spike_webview(window: &Window) -> (Option<WebView>, Option<String>) {
    match gtk::init() {
        Ok(()) => match build_webview(window) {
            Ok(webview) => (Some(webview), None),
            Err(direct_error) => match XlibParent::from_gpui(window) {
                Ok(parent) => match build_webview(&parent) {
                    Ok(webview) => (
                        Some(webview),
                        Some(
                            "WebKitGTK child · XCB→Xlib adapter · GTK loop pumped by GPUI"
                                .to_owned(),
                        ),
                    ),
                    Err(bridge_error) => (
                        None,
                        Some(format!(
                            "Direct XCB build failed: {direct_error}; XCB→Xlib build failed: {bridge_error}"
                        )),
                    ),
                },
                Err(bridge_error) => (
                    None,
                    Some(format!(
                        "Direct XCB build failed: {direct_error}; XCB→Xlib adapter failed: {bridge_error}"
                    )),
                ),
            },
        },
        Err(error) => (None, Some(format!("GTK init failed: {error}"))),
    }
}

#[cfg(not(target_os = "linux"))]
fn build_spike_webview(window: &Window) -> (Option<WebView>, Option<String>) {
    match build_webview_for_native_child(window) {
        Ok(webview) => (Some(webview), None),
        Err(error) => (None, Some(error)),
    }
}

/// Native GPUI chrome with a live WebKitGTK child window on the right.
pub struct BrowserSpike {
    webview: Option<WebView>,
    pump_task: Option<Task<()>>,
    startup_error: Option<String>,
}

impl BrowserSpike {
    pub fn new(window: &mut Window, _cx: &mut Context<Self>) -> Self {
        let (webview, startup_error) = build_spike_webview(window);

        #[cfg(target_os = "linux")]
        let pump_task = webview.as_ref().map(|_| {
            _cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    if this
                        .update(cx, |_view, _cx| {
                            while gtk::events_pending() {
                                gtk::main_iteration_do(false);
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
        });

        #[cfg(not(target_os = "linux"))]
        let pump_task = None;

        Self {
            webview,
            pump_task,
            startup_error,
        }
    }
}

impl Render for BrowserSpike {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let _keep_pump_alive = self.pump_task.as_ref();
        let status = self.startup_error.clone().unwrap_or_else(|| {
            "WebKitGTK child · https://example.com · GTK loop pumped by GPUI".to_owned()
        });

        div()
            .size_full()
            .bg(theme.surface)
            .text_color(theme.text)
            .child(
                div()
                    .h(px(52.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .px(px(20.0))
                    .bg(theme.bg)
                    .border_b_1()
                    .border_color(theme.border)
                    .text_size(theme.typography.headline)
                    .child("P72 · Browser composition spike"),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .w_full()
                    .child(
                        div()
                            .absolute()
                            .left(px(0.0))
                            .top(px(0.0))
                            .bottom(px(0.0))
                            .w(px(336.0))
                            .p(px(20.0))
                            .bg(theme.bg)
                            .border_r_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .text_size(theme.typography.title)
                                    .text_color(theme.text)
                                    .child("Native GPUI sidebar"),
                            )
                            .child(
                                div()
                                    .mt(px(18.0))
                                    .text_size(theme.typography.base_size)
                                    .text_color(theme.text_muted)
                                    .child(
                                        "The page on the right is a real WebKitGTK child window.",
                                    ),
                            )
                            .child(
                                div()
                                    .mt(px(18.0))
                                    .text_size(theme.typography.footnote)
                                    .text_color(theme.text_faint)
                                    .child(status),
                            ),
                    )
                    // Deliberately overlaps the native child window. If the
                    // child is above GPUI, this bright marker disappears;
                    // either result answers the z-order part of P72.
                    .child(
                        div()
                            .absolute()
                            .left(px(260.0))
                            .top(px(92.0))
                            .w(px(520.0))
                            .h(px(92.0))
                            .flex()
                            .items_center()
                            .justify_start()
                            .px(px(16.0))
                            .bg(theme.brand_coral)
                            .text_color(theme.bg)
                            .text_size(theme.typography.title)
                            .child("GPUI → WebKit"),
                    ),
            )
    }
}

impl Drop for BrowserSpike {
    fn drop(&mut self) {
        if let Some(webview) = &self.webview {
            let _ = webview.set_visible(false);
        }
    }
}

/// The destination for a link opened from chat or another native surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserLinkTarget {
    /// Load the link in the current internal browser surface.
    Internal,
    /// Leave Sirio and let the host open the link in the system browser.
    External,
}

/// Events a browser host must handle outside the reusable surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserEvent {
    /// Navigate this browser surface to the normalized URL.
    Navigate(String),
    /// Ask the host to open this URL with the system browser.
    OpenExternal(String),
    /// The native child started loading a URL.
    PageStarted(String),
    /// The native child finished loading a URL.
    PageFinished(String),
    /// The native child reported a document title.
    TitleChanged(String),
}

/// Browser failures that can be shown in Sirio's native chrome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserError {
    /// The address is not an HTTP(S) URL that the embedded browser accepts.
    InvalidAddress(String),
    /// The WebKit child rejected an otherwise valid navigation request.
    Navigation(String),
}

/// #307: what a Windows machine without the WebView2 Runtime sees in place
/// of the browser's content. Names the runtime, owns that Sirio could not
/// install it, and points at the one retry action.
const RUNTIME_MISSING_MESSAGE: &str = "Sirio needs the Microsoft Edge WebView2 Runtime to show pages here, and could not install it. The rest of Sirio works without it — check your network connection, or install the runtime from Microsoft, then retry.";

/// Why the native webview child never came up, when it didn't. #307 split
/// the old plain `Option<String>` in two: a missing WebView2 runtime on
/// Windows gets the full-content explanation with a retry action, while
/// any other engine failure keeps the small red banner above an empty
/// pane, exactly as before.
#[derive(Clone, Debug, PartialEq, Eq)]
enum StartupFailure {
    /// Windows: `wry::webview_version()` found no WebView2 Runtime installed.
    RuntimeMissing,
    /// Any other engine-start failure, carrying the message for the banner.
    Failed(String),
}

impl StartupFailure {
    /// The one-line text for the host-facing startup-error contract (#131):
    /// every flavor reports `Some`, so `browser.open` answers `ok:false`
    /// for a dead surface no matter why it is dead.
    fn message(&self) -> &str {
        match self {
            Self::RuntimeMissing => RUNTIME_MISSING_MESSAGE,
            Self::Failed(error) => error,
        }
    }

    /// The generic red banner's text. A missing runtime renders no banner:
    /// the full-content explanation replaces the content instead.
    fn banner_text(&self) -> Option<&str> {
        match self {
            Self::RuntimeMissing => None,
            Self::Failed(error) => Some(error),
        }
    }
}

impl std::fmt::Display for BrowserError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAddress(message) | Self::Navigation(message) => {
                formatter.write_str(message)
            }
        }
    }
}

/// A pending agent-origin permission shown in the GPUI doorhanger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionPrompt {
    origin: String,
}

impl PermissionPrompt {
    /// The origin asking for browser access.
    pub fn origin(&self) -> &str {
        &self.origin
    }
}

/// The state that stays testable without a live X11 display or WebKit.
///
/// The native child is deliberately not part of this model. GPUI owns the
/// toolbar, error banner, activity indicator and permission doorhanger; this
/// model owns the state those surfaces describe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserState {
    address: String,
    page_title: String,
    loading: bool,
    agent_driving: bool,
    error: Option<String>,
    history: Vec<String>,
    history_index: usize,
    permission_prompt: Option<PermissionPrompt>,
    allowed_origins: BTreeSet<String>,
}

impl BrowserState {
    /// Creates a browser at a valid initial address.
    pub fn new(initial_url: &str) -> Result<Self, BrowserError> {
        let address = normalize_address(initial_url)?;
        Ok(Self {
            address: address.clone(),
            page_title: String::new(),
            loading: true,
            agent_driving: false,
            error: None,
            history: vec![address],
            history_index: 0,
            permission_prompt: None,
            allowed_origins: BTreeSet::new(),
        })
    }

    /// The normalized address shown in the toolbar.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The latest document title, or an empty string before one is reported.
    pub fn page_title(&self) -> &str {
        &self.page_title
    }

    /// Whether WebKit is currently loading a document.
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    /// Whether an ACP/browser action is currently driving this surface.
    pub fn agent_driving(&self) -> bool {
        self.agent_driving
    }

    /// The current visible browser error, if any.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The pending GPUI doorhanger, if any.
    pub fn permission_prompt(&self) -> Option<&PermissionPrompt> {
        self.permission_prompt.as_ref()
    }

    /// Whether a Back action has a history entry to visit.
    pub fn can_go_back(&self) -> bool {
        self.history_index > 0
    }

    /// Whether a Forward action has a history entry to visit.
    pub fn can_go_forward(&self) -> bool {
        self.history_index + 1 < self.history.len()
    }

    /// Normalizes and begins an address-field navigation.
    pub fn submit_address(&mut self, input: &str) -> Result<String, BrowserError> {
        let address = match normalize_address(input) {
            Ok(address) => address,
            Err(error) => {
                self.error = Some(error.to_string());
                self.loading = false;
                return Err(error);
            }
        };
        self.begin_navigation(address.clone());
        Ok(address)
    }

    /// Records that WebKit began loading a URL.
    pub fn did_start_navigation(&mut self, url: &str) {
        if let Ok(url) = normalize_address(url) {
            if !self.loading {
                self.record_navigation(url.clone());
            }
            self.address = url;
            self.loading = true;
            self.error = None;
        }
    }

    /// Records a successful WebKit navigation and updates the title/content
    /// chrome without painting anything over the child window.
    pub fn did_finish_navigation(&mut self, url: &str, title: &str) {
        if let Ok(url) = normalize_address(url) {
            self.replace_current_navigation(url.clone());
            self.address = url;
        }
        self.page_title = title.to_owned();
        self.loading = false;
        self.error = None;
    }

    /// Records a navigation failure in the native browser chrome.
    pub fn did_fail_navigation(&mut self, message: impl Into<String>) {
        self.loading = false;
        self.error = Some(message.into());
    }

    /// Moves history backward and returns the URL WebKit should visit.
    pub fn go_back(&mut self) -> Option<String> {
        if !self.can_go_back() {
            return None;
        }
        self.history_index -= 1;
        let address = self.history[self.history_index].clone();
        self.begin_navigation(address.clone());
        Some(address)
    }

    /// Moves history forward and returns the URL WebKit should visit.
    pub fn go_forward(&mut self) -> Option<String> {
        if !self.can_go_forward() {
            return None;
        }
        self.history_index += 1;
        let address = self.history[self.history_index].clone();
        self.begin_navigation(address.clone());
        Some(address)
    }

    /// Returns the current URL and marks a reload as in progress.
    pub fn reload(&mut self) -> Option<String> {
        if self.address.is_empty() {
            None
        } else {
            self.loading = true;
            self.error = None;
            Some(self.address.clone())
        }
    }

    /// Stops a pending navigation. The WebKit adapter calls `window.stop()`
    /// because wry exposes no cross-backend stop method.
    pub fn stop_loading(&mut self) -> bool {
        let was_loading = self.loading;
        self.loading = false;
        was_loading
    }

    /// Opens a link either in this surface or through the host's system
    /// browser, preserving the documented Cmd+Shift bypass.
    pub fn open_link(
        &mut self,
        url: &str,
        target: BrowserLinkTarget,
    ) -> Result<BrowserEvent, BrowserError> {
        let url = normalize_address(url)?;
        Ok(match target {
            BrowserLinkTarget::Internal => {
                self.begin_navigation(url.clone());
                BrowserEvent::Navigate(url)
            }
            BrowserLinkTarget::External => BrowserEvent::OpenExternal(url),
        })
    }

    /// Updates the F-BRW-05 agent-driving indicator.
    pub fn set_agent_driving(&mut self, driving: bool) {
        self.agent_driving = driving;
    }

    /// Requests a permission doorhanger unless this origin was already
    /// granted. Persistence is injected by the host through the grant
    /// snapshot methods below, so the view does not invent a second store.
    pub fn request_permission(&mut self, origin: &str) {
        if !self.is_origin_allowed(origin) {
            self.permission_prompt = Some(PermissionPrompt {
                origin: origin.to_owned(),
            });
        }
    }

    /// Allows the pending origin and returns it for durable persistence.
    pub fn allow_permission(&mut self) -> Option<String> {
        let prompt = self.permission_prompt.take()?;
        self.allowed_origins.insert(prompt.origin.clone());
        Some(prompt.origin)
    }

    /// Denies the pending origin and returns it for telemetry or a host
    /// decision; denial is intentionally not persisted as an allow grant.
    pub fn deny_permission(&mut self) -> Option<String> {
        self.permission_prompt.take().map(|prompt| prompt.origin)
    }

    /// Seeds the view from the host's durable browser-origin grants.
    pub fn set_allowed_origins(&mut self, origins: impl IntoIterator<Item = String>) {
        self.allowed_origins = origins.into_iter().collect();
    }

    /// Returns the current durable-grant snapshot for persistence.
    pub fn allowed_origins(&self) -> impl Iterator<Item = &str> {
        self.allowed_origins.iter().map(String::as_str)
    }

    /// Revokes one origin grant.
    pub fn revoke_origin(&mut self, origin: &str) -> bool {
        self.allowed_origins.remove(origin)
    }

    /// Revokes all origin grants.
    pub fn revoke_all_origins(&mut self) {
        self.allowed_origins.clear();
    }

    /// Returns whether the origin has already been allowed.
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        self.allowed_origins.contains(origin)
    }

    fn begin_navigation(&mut self, address: String) {
        self.record_navigation(address.clone());
        self.address = address;
        self.loading = true;
        self.error = None;
    }

    fn record_navigation(&mut self, address: String) {
        if self.history.get(self.history_index) == Some(&address) {
            return;
        }
        if self.history_index + 1 < self.history.len()
            && self.history[self.history_index + 1] == address
        {
            self.history_index += 1;
            return;
        }
        self.history.truncate(self.history_index + 1);
        self.history.push(address);
        self.history_index = self.history.len() - 1;
    }

    fn replace_current_navigation(&mut self, address: String) {
        if let Some(current) = self.history.get_mut(self.history_index) {
            *current = address;
        } else {
            self.history.push(address);
            self.history_index = self.history.len() - 1;
        }
    }
}

/// Normalizes an address-field value to an HTTP(S) URL.
pub fn normalize_address(input: &str) -> Result<String, BrowserError> {
    let input = input.trim();
    if input.is_empty() || input.chars().any(char::is_whitespace) {
        return Err(BrowserError::InvalidAddress(
            "Enter a valid HTTP or HTTPS address".to_owned(),
        ));
    }
    let address = if input.starts_with("http://") || input.starts_with("https://") {
        input.to_owned()
    } else {
        format!("https://{input}")
    };
    let Some((scheme, host_and_path)) = address.split_once("://") else {
        return Err(BrowserError::InvalidAddress(
            "Only HTTP and HTTPS addresses are supported".to_owned(),
        ));
    };
    let valid_scheme = scheme == "http" || scheme == "https";
    let host = host_and_path.split('/').next().unwrap_or_default();
    if !valid_scheme || host.is_empty() || host.starts_with('.') || host.ends_with('.') {
        return Err(BrowserError::InvalidAddress(
            "Only HTTP and HTTPS addresses are supported".to_owned(),
        ));
    }
    Ok(address)
}

#[derive(Clone, Debug)]
enum WebEvent {
    NavigationRequested(String),
    NewWindowRequested(String),
    PageLoad(PageLoadEventKind, String),
    TitleChanged(String),
}

#[derive(Clone, Copy, Debug)]
enum PageLoadEventKind {
    Started,
    Finished,
}

type SharedWebView = Rc<RefCell<Option<WebView>>>;
type SharedWebEvents = Rc<RefCell<Vec<WebEvent>>>;
/// F-BRW-01: `set_bounds`'s own `Rect` conversion (`native_webview_rect`) is
/// a verified no-op — GPUI's bounds are already logical and wry's
/// `to_logical` on an already-`Logical` value does not rescale. The
/// remaining shrink (reproduced live, ~0.857x = 1/1.1667, matching a
/// 112/96 DPI ratio) happens *below* that call, inside GTK/GDK's own
/// geometry plumbing for the foreign X11 window `set_bounds` moves — wry
/// exposes no hook to see or disable it. `webview.bounds()` reads the
/// window's real on-screen geometry straight from `XGetWindowAttributes`,
/// so comparing a requested size against that readback measures GTK's
/// silent factor directly, with no assumption about *why* GTK applies it.
/// `None` means "not yet calibrated"; `Some(factor)` is the multiplier
/// applied to every future request so the requested and actual rects
/// converge.
type SharedScaleCorrection = Rc<Cell<Option<f64>>>;

/// F-BRW: whether the native child window is currently mapped, mirrored on our
/// side because wry exposes `set_visible` but no way to read it back.
///
/// The webview is a real X11 child window layered *above* GPUI's GL surface. It
/// is not a GPUI element and takes no part in GPUI's paint, clip or z-order, so
/// leaving [`NativeWebViewElement`] out of a frame's element tree does not hide
/// it — it only means nobody moved it that frame. Before this existed,
/// `set_visible` was called from exactly two places, both `Drop` impls, and
/// never with `true`: an opened Browser tab therefore kept painting its last
/// page over whatever occupied the same rectangle afterwards, for the life of
/// the process. Reproduced four independent times by the F-BRW critic, which
/// also found it hiding the origin URL on the Permissions screen (F-BRW-08).
///
/// It starts `true` because that is what wry leaves behind: `build_as_child`
/// ends in `XMapWindow`, so the window is mapped before we ever touch it.
/// Initialising this to `false` would make the first hide a no-op against a
/// window that is, in fact, on screen — which is the original bug wearing a
/// flag.
type SharedNativeVisibility = Rc<Cell<bool>>;

/// #376: whether a GPUI overlay that paints above the page (the `+` new-tab
/// menu, a tab context menu, the command palette, a modal) is currently open
/// somewhere over this surface's rectangle.
///
/// The webview is a native child HWND on Windows (and an X11 child on Linux),
/// layered above GPUI's own surface: it takes no part in GPUI paint, clip or
/// z-order, so a menu drawn by GPUI lands *behind* the page and its rows stop
/// being clickable. Hiding the child while the overlay is open is the same
/// contract [`BrowserSurface::set_native_visible`] already keeps for
/// off-screen surfaces, only scoped to "on screen but covered".
///
/// Shared by cell with the element for the same reason as the visibility
/// mirror: the host sets it from `read` during `render`, and `prepaint` reads
/// it after. Starts `false`: no overlay is open for a fresh surface.
type SharedOverlayObscured = Rc<Cell<bool>>;

/// The state a freshly built webview is actually in. Named rather than written
/// inline so the reason above has somewhere to be tested; see
/// `the_visibility_mirror_starts_where_wry_leaves_the_window`.
fn initial_native_visibility() -> SharedNativeVisibility {
    Rc::new(Cell::new(true))
}

/// Map or unmap the native child, skipping the call when it already agrees.
///
/// The idempotence is load-bearing, not tidiness: `prepaint` runs every frame
/// and asks for `true` every time, so an unconditional call would drive an
/// X11 map request at frame rate.
fn apply_native_visible(webview: &SharedWebView, flag: &SharedNativeVisibility, want: bool) {
    if flag.get() == want {
        return;
    }
    if let Some(webview) = webview.borrow().as_ref()
        && webview.set_visible(want).is_err()
    {
        return;
    }
    flag.set(want);
}

/// Push GTK's pending work and Xlib's output buffer all the way to the X
/// server.
///
/// This is the other half of the F-BRW tab-close bug, and the half that is
/// invisible from Rust. wry's `set_visible(false)` is
/// `XUnmapWindow(gdk_display, child)` and dropping a `WebView` ends in
/// `XDestroyWindow` on that same connection (wry 0.56.1,
/// `webkitgtk/mod.rs`: `set_visible_x11`, `X11Data::drop`). Xlib *buffers*
/// both: neither reaches the server until something flushes. In every other
/// case the surface's own 16 ms pump — `while gtk::events_pending() {
/// main_iteration_do }` — flushes it on the next tick, which is why hiding a
/// browser on tab-switch or behind Settings works.
///
/// Closing the tab is the one case where that pump is what we are destroying.
/// The unmap and the destroy are issued into a buffer nobody will ever drain
/// again, so the last page stays `IsViewable` and keeps painting over the
/// empty pane or the terminal that takes its place — reproduced twice by the
/// `brw-unmap` critic, once from a clean instance, five seconds and many
/// forced repaints after the tab was gone.
///
/// Two rounds because the first iteration is what lets GTK act on
/// `gtk_widget_destroy`/`gtk_window_close` and emit their own X requests; the
/// flush after it is what puts those on the wire.
///
/// **Which half actually flushes, measured.** The `brw-close` critic tried to
/// break `Scripts/Tests/test-x11-unmap-needs-a-flush.sh` and found that
/// deleting the explicit `display.flush()` alone still passes: `events_pending`
/// reaches `XPending`, which is `XEventsQueued(QueuedAfterFlush)` and flushes on
/// its own. Deleting the whole recipe correctly fails. So the iteration is the
/// load-bearing half here, and `display.flush()` is a guarantee rather than the
/// mechanism — kept because "the loop happened to flush" is a property of GDK's
/// event source, not a contract, and because an early `break` in a future
/// version of that loop would silently take the flush with it. Not cargo cult:
/// the test isolates "no flush at all", and that limit is stated in the report
/// rather than papered over.
#[cfg(target_os = "linux")]
fn flush_native_window_ops() {
    for _ in 0..2 {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if let Some(display) = gtk::gdk::Display::default() {
            display.flush();
        }
    }
}

/// Unmap and destroy the native child, now, and make sure the server hears
/// about it.
///
/// Split out from [`BrowserSurface::close_native`] so the cell arithmetic has
/// somewhere to be tested without a live X11 window; see
/// `closing_takes_the_child_out_of_the_shared_cell`.
/// #255: whether [`BrowserSurface::ensure_pump_task`] should arm the pump on
/// this frame. Split out from the method so the decision is testable without a
/// window -- constructing a surface means constructing a real WebView2.
///
/// Two properties, both load-bearing:
/// - idempotent, so `render` can call it every frame and get one task;
/// - silent for a surface with no webview, which is how a *closed* surface
///   stays closed: `close_native` empties the webview cell, and without this
///   guard the next frame would arm a pump for a webview that no longer exists.
fn should_arm_pump(has_task: bool, has_webview: bool) -> bool {
    !has_task && has_webview
}

fn close_native_window(webview: &SharedWebView, flag: &SharedNativeVisibility) {
    apply_native_visible(webview, flag, false);
    // Taking the value out of the cell is what destroys the X11 window: the
    // `Rc` clones held by the element share the *cell*, not the `WebView`, so
    // this drop is the last one and runs wry's teardown here rather than
    // whenever GPUI happens to release the entity. Every other holder is left
    // looking at `None`, which every call site already handles.
    let taken = webview.borrow_mut().take();
    #[cfg(target_os = "linux")]
    let had_native_window = taken.is_some();
    drop(taken);
    #[cfg(target_os = "linux")]
    if had_native_window {
        flush_native_window_ops();
    }
}

/// F-CTRL-BROWSER-06: shadows `console.log/warn/error/info/debug` with
/// wrappers that append to `window.__sirioConsole` before calling through
/// to the original method, so page output keeps working in devtools while
/// `browser.console` can read the buffer back via `evaluate_script`.
const CONSOLE_CAPTURE_SCRIPT: &str = r#"(function () {
  window.__sirioConsole = window.__sirioConsole || [];
  var levels = ["log", "warn", "error", "info", "debug"];
  levels.forEach(function (level) {
    var original = console[level] ? console[level].bind(console) : function () {};
    console[level] = function () {
      try {
        var parts = Array.prototype.map.call(arguments, function (arg) {
          try {
            return typeof arg === "string" ? arg : JSON.stringify(arg);
          } catch (e) {
            return String(arg);
          }
        });
        window.__sirioConsole.push({ level: level, message: parts.join(" ") });
      } catch (e) {}
      original.apply(console, arguments);
    };
  });
})();"#;

fn wait_for_script_result(
    receiver: mpsc::Receiver<String>,
    timeout: Duration,
) -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        let deadline = Instant::now() + timeout;
        loop {
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
            match receiver.try_recv() {
                Ok(value) => return Ok(value),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("evaluate_script callback disconnected".to_string());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if Instant::now() >= deadline {
                return Err("evaluate_script timed out".to_string());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        receiver.recv_timeout(timeout).map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => "evaluate_script timed out".to_string(),
            mpsc::RecvTimeoutError::Disconnected => {
                "evaluate_script callback disconnected".to_string()
            }
        })
    }
}

/// Where the engine keeps the one shared browser profile (R6.2/R6.3).
///
/// Required on Windows rather than tidy: with nothing set, WebView2 writes
/// `<exe>.WebView2\EBWebView` **beside the binary** — reproduced on this tree,
/// where opening a webview from an example left
/// `browser_eval_probe.exe.WebView2/EBWebView` in `target/debug/examples`. An
/// installed Sirio under `Program Files` cannot create that, so the browser
/// would fail for exactly the users who installed it properly.
///
/// One profile for the whole app, never one per worktree (R6.1).
///
/// The location mirrors `sirio::session::app_support_root`'s rule rather than
/// calling it: `sirio_ui` sits BESIDE `sirio` in the crate layering
/// (CLAUDE.md), not under it. This is the same tension #230 and #246 record —
/// if the rule changes there, change it here too. `None` means "let the engine
/// decide", which is only reached when the profile variables are all absent.
/// #125 (spec R6.1): the profile directory, handed down by the app at startup.
///
/// `sirio_ui` sits beside `sirio` in the crate layering (see CLAUDE.md) and
/// cannot call `session::browser_profile_path`, so the app pushes the resolved
/// path in rather than the scoping rule being written twice. Unset -- an
/// example binary, a test -- falls back to the user-wide location below.
static PROFILE_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Called once by the app before the first browser surface is built.
///
/// Later calls are ignored, which is exactly what is wanted: a profile a
/// running WebView2 has already opened must not move out from under it.
pub fn set_profile_dir(path: PathBuf) {
    let _ = PROFILE_DIR.set(path);
}

fn browser_profile_dir() -> Option<PathBuf> {
    if let Some(path) = PROFILE_DIR.get() {
        return Some(path.clone());
    }
    let root = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            #[cfg(windows)]
            {
                std::env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
            }
            #[cfg(not(windows))]
            {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .map(|home| home.join(".local").join("share"))
            }
        })?;
    Some(root.join("Sirio").join("browser"))
}

fn build_production_webview<W: HasWindowHandle>(
    parent: &W,
    initial_url: &str,
    events: &SharedWebEvents,
    context: &mut WebContext,
) -> Result<WebView, wry::Error> {
    let navigation_events = events.clone();
    let window_events = events.clone();
    let page_events = events.clone();
    let title_events = events.clone();
    // R6.2: the profile lives where Sirio says, not beside the binary.
    WebViewBuilder::new_with_web_context(context)
        .with_url(initial_url)
        // F-CTRL-BROWSER-06: wry exposes no cross-platform console-message
        // hook, so `browser.console` captures by shadowing the console
        // methods before any page script runs (this init script executes
        // on every navigation, ahead of the document's own scripts) and
        // reading the buffer back with `evaluate_script`.
        .with_initialization_script(CONSOLE_CAPTURE_SCRIPT)
        .with_navigation_handler(move |url| {
            navigation_events
                .borrow_mut()
                .push(WebEvent::NavigationRequested(url));
            true
        })
        .with_new_window_req_handler(move |url, _features: NewWindowFeatures| {
            window_events
                .borrow_mut()
                .push(WebEvent::NewWindowRequested(url));
            NewWindowResponse::Deny
        })
        .with_on_page_load_handler(move |event, url| {
            let event = match event {
                PageLoadEvent::Started => PageLoadEventKind::Started,
                PageLoadEvent::Finished => PageLoadEventKind::Finished,
            };
            page_events
                .borrow_mut()
                .push(WebEvent::PageLoad(event, url));
        })
        .with_document_title_changed_handler(move |title| {
            title_events
                .borrow_mut()
                .push(WebEvent::TitleChanged(title));
        })
        .with_bounds(Rect {
            position: LogicalPosition::new(0_i32, 0_i32).into(),
            size: LogicalSize::new(1_i32, 1_i32).into(),
        })
        .build_as_child(parent)
}

#[cfg(target_os = "linux")]
fn build_production_webview_for_platform(
    window: &Window,
    initial_url: &str,
    events: &SharedWebEvents,
    context: &mut WebContext,
) -> Result<WebView, String> {
    match gtk::init() {
        Ok(()) => match build_production_webview(window, initial_url, events, context) {
            Ok(webview) => Ok(webview),
            Err(direct_error) => match XlibParent::from_gpui(window) {
                Ok(parent) => build_production_webview(&parent, initial_url, events, context).map_err(
                    |bridge_error| {
                        format!(
                            "Direct XCB build failed: {direct_error}; XCB→Xlib build failed: {bridge_error}"
                        )
                    },
                ),
                Err(bridge_error) => Err(format!(
                    "Direct XCB build failed: {direct_error}; XCB→Xlib adapter failed: {bridge_error}"
                )),
            },
        },
        Err(error) => Err(format!("GTK init failed: {error}")),
    }
}

#[cfg(not(target_os = "linux"))]
#[cfg_attr(target_os = "windows", allow(dead_code))]
fn build_production_webview_for_platform(
    window: &Window,
    initial_url: &str,
    events: &SharedWebEvents,
    context: &mut WebContext,
) -> Result<WebView, String> {
    build_production_webview_for_native_child(window, initial_url, events, context)
}

/// The engine this platform actually hosts, for error text (#131, #144).
///
/// The non-Linux path is named for macOS throughout, and its failures said
/// "WKWebView child failed" on Windows too, where the engine is WebView2 — so
/// a user was told the wrong component had failed. #131 asked for
/// per-platform engine names.
///
/// The functions carry the platform-neutral `_for_native_child` name for the
/// same reason (#125, spec R2.3): they were called `_for_macos` while running
/// on Windows, and a name that contradicts where the code runs is how a
/// macOS-only assumption gets added to a shared body without anyone noticing.
///
/// The spec also asks for a real `#[cfg(target_os = "windows")]` branch here.
/// That is deliberately NOT taken: both divergences it was meant to host have
/// since moved out -- the profile directory arrives through `set_profile_dir`
/// (spec R6.1) and the geometry lives in `native_webview_rect` (spec R3.3) --
/// so a Windows arm today would delegate straight back to this body with
/// nothing in it. An empty branch is a worse lie than a shared one; add it
/// when there is a divergence to put inside.
#[cfg(not(target_os = "linux"))]
const NATIVE_ENGINE: &str = if cfg!(target_os = "windows") {
    "WebView2"
} else {
    "WKWebView"
};

/// What GPUI failed to hand us, named for the platform's own window type.
#[cfg(not(target_os = "linux"))]
const NATIVE_WINDOW_HANDLE: &str = if cfg!(target_os = "windows") {
    "Win32"
} else {
    "AppKit"
};

#[cfg(not(target_os = "linux"))]
fn build_webview_for_native_child<W: HasWindowHandle>(parent: &W) -> Result<WebView, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build_webview(parent))) {
        Ok(Ok(webview)) => Ok(webview),
        Ok(Err(error)) => Err(format!("{NATIVE_ENGINE} child failed: {error}")),
        Err(_) => Err(format!(
            "{NATIVE_ENGINE} child failed: GPUI did not expose a usable \
             {NATIVE_WINDOW_HANDLE} window handle"
        )),
    }
}

#[cfg(not(target_os = "linux"))]
#[cfg_attr(target_os = "windows", allow(dead_code))]
fn build_production_webview_for_native_child(
    window: &Window,
    initial_url: &str,
    events: &SharedWebEvents,
    context: &mut WebContext,
) -> Result<WebView, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        build_production_webview(window, initial_url, events, context)
    })) {
        Ok(Ok(webview)) => Ok(webview),
        Ok(Err(error)) => Err(format!("{NATIVE_ENGINE} child failed: {error}")),
        Err(_) => Err(format!(
            "{NATIVE_ENGINE} child failed: GPUI did not expose a usable \
             {NATIVE_WINDOW_HANDLE} window handle"
        )),
    }
}

/// #368: the HWND a deferred WebView2 build parents to.
///
/// Captured synchronously from the GPUI window (cheap, no COM wait, no
/// message-loop pump) while the App borrow is still held, then used later
/// from a foreground task with no App borrow held. Passing the raw integer
/// across is what lets the build run lease-free: the `Window` itself cannot
/// cross the spawn boundary, and rebuilding from it would re-acquire the
/// borrow the pump must not see held.
#[cfg(target_os = "windows")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeferredParent {
    hwnd: isize,
}

#[cfg(target_os = "windows")]
impl DeferredParent {
    fn from_hwnd(hwnd: isize) -> Result<Self, String> {
        if hwnd == 0 {
            return Err(format!(
                "{NATIVE_ENGINE} child failed: GPUI did not expose a usable \
                 {NATIVE_WINDOW_HANDLE} window handle"
            ));
        }
        Ok(Self { hwnd })
    }
}

#[cfg(target_os = "windows")]
impl HasWindowHandle for DeferredParent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let hwnd = NonZeroIsize::new(self.hwnd).ok_or(HandleError::Unavailable)?;
        // SAFETY: the integer was read from a live GPUI window on the main
        // thread moments earlier. The deferred build runs on the same main
        // thread while that window still lives (the surface holding this
        // parent is mounted in it); wry only reads the integer to parent
        // the child, and a stale value fails the build rather than
        // reaching undefined behaviour.
        let handle = Win32WindowHandle::new(hwnd);
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(handle)) })
    }
}

/// #368: read the Win32 HWND out of a GPUI window without building anything.
///
/// Cheap and pump-free, so it is safe to call while the App is borrowed.
/// Everything that waits on COM (and therefore pumps `DispatchMessageW`)
/// happens later, in the deferred build.
#[cfg(target_os = "windows")]
fn capture_window_hwnd(window: &Window) -> Result<isize, String> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| format!("GPUI window handle unavailable: {error}"))?;
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get()),
        raw => Err(format!("GPUI returned unsupported handle: {raw:?}")),
    }
}

/// #369 (Ctrl+L residual): steal Win32 thread focus from the WebView2 child.
///
/// wry's `focus_parent()` is `SetFocus(parent)` alone. That is a no-op when
/// the parent already has thread focus (the click path, which is why the
/// first fix worked there), but it cannot steal thread focus when the child
/// still holds it (the `Ctrl+L` path after a page click: GPUI shows a caret
/// via `window.focus`, yet the first keystrokes still go to the page).
/// Attach the two threads, foreground the parent top-level, and focus it.
/// Best-effort with fallback to the plain `focus_parent()` below, so a
/// failure here can only leave the previous behaviour, never break it.
///
/// Pump-safe: `SetFocus`/`SetForegroundWindow` deliver `WM_SETFOCUS`/
/// `WM_ACTIVATE` synchronously, and GPUI's Windows backend holds no borrow
/// across those (focus messages fall through to `DefWindowProc`, activation
/// only resets modifiers and spawns) — unlike WebView2 creation (#255, #368),
/// which must stay off every GPUI lease.
///
/// `pub` so the host's shell root can reclaim focus on any GPUI click
/// (sidebar, tab strip, toolbar): the same stolen-focus state survives a
/// click outside the page, and only a Win32 reclaim fixes the next chord.
#[cfg(target_os = "windows")]
pub fn steal_win32_focus_from_webview_child(window: &Window) {
    let Ok(parent_isize) = capture_window_hwnd(window) else {
        return;
    };
    steal_win32_focus_to_parent(parent_isize);
}

/// Raw-HWND core of [`steal_win32_focus_from_webview_child`], split out so
/// the WebView2 accelerator handler (which already holds the parent HWND,
/// with no `Window` in reach) can steal before reposting a chord.
#[cfg(target_os = "windows")]
fn steal_win32_focus_to_parent(parent_isize: isize) {
    use windows_sys::Win32::{
        Foundation::HWND,
        System::Threading::{AttachThreadInput, GetCurrentThreadId},
        UI::{
            Input::KeyboardAndMouse::{GetFocus, SetFocus},
            WindowsAndMessaging::{
                GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
            },
        },
    };
    let parent = parent_isize as HWND;
    unsafe {
        let focused = GetFocus();
        if focused == parent {
            return;
        }
        let current_tid = GetCurrentThreadId();
        let mut focused_pid = 0u32;
        let focused_tid = if focused.is_null() {
            0
        } else {
            GetWindowThreadProcessId(focused, &mut focused_pid)
        };
        let foreground = GetForegroundWindow();
        let mut foreground_pid = 0u32;
        let foreground_tid = if foreground.is_null() {
            0
        } else {
            GetWindowThreadProcessId(foreground, &mut foreground_pid)
        };
        let mut attached_focused = false;
        let mut attached_foreground = false;
        if focused_tid != 0 && focused_tid != current_tid {
            attached_focused = AttachThreadInput(current_tid, focused_tid, 1) != 0;
        }
        if foreground_tid != 0 && foreground_tid != current_tid && foreground_tid != focused_tid {
            attached_foreground = AttachThreadInput(current_tid, foreground_tid, 1) != 0;
        }
        let _ = SetForegroundWindow(parent);
        let _ = SetFocus(parent);
        if attached_focused {
            AttachThreadInput(current_tid, focused_tid, 0);
        }
        if attached_foreground {
            AttachThreadInput(current_tid, foreground_tid, 0);
        }
    }
}

/// #369 follow-up: which Win32 virtual-key chords belong to the Sirio host
/// and must be forwarded from the WebView2 accelerator handler.
///
/// Pure so the rule stays testable without a window: the handler can only
/// run while a live page holds native focus. Forwards exactly the chords the
/// host handles globally — window commands (`Ctrl+L/T/O/S`, `Ctrl+Shift+`
/// `P/L/D/R/H/B/W`, `Ctrl+,`, `Escape`), pane/tab chords (`Ctrl+Tab`,
/// `Ctrl+Shift+Tab`, `Ctrl+1..9`, `Ctrl(+Shift)+W`, `Ctrl+Alt` arrows/`W`,
/// `Ctrl+Alt+Shift` right/down) and the universal palette (`Ctrl+Shift+P`,
/// `Ctrl+K` outside a terminal, decided by the host at dispatch). Everything
/// else stays in the page: typing, `Ctrl+A/C/V/X/Z`, arrows, `F5`/`Ctrl+R`
/// reload, `Ctrl+P` print, `Ctrl+F` find, `Alt+Left/Right` history — stealing
/// any of those would break the page to deliver keys the host ignores.
///
/// `D`/`R`/`H` (not `S`/`I`/`O`) because #374 retargeted the Windows layout
/// chords: `Ctrl+Shift+S/I/O` are OS hotkeys that never reach the window, so
/// on Windows sidebar is `Ctrl+Shift+D`, right panel `Ctrl+Shift+R`, restore
/// is `Ctrl+Shift+H`. This rule only runs on Windows (the handler is
/// Windows-only), hence only the Windows letters.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn should_forward_accelerator_to_host(ctrl: bool, shift: bool, alt: bool, vk: u32) -> bool {
    // Win32 virtual keys (no dependency needed for the pure rule).
    const VK_TAB: u32 = 0x09;
    const VK_ESCAPE: u32 = 0x1B;
    const VK_B: u32 = 0x42;
    const VK_D: u32 = 0x44;
    const VK_H: u32 = 0x48;
    const VK_K: u32 = 0x4B;
    const VK_L: u32 = 0x4C;
    const VK_O: u32 = 0x4F;
    const VK_P: u32 = 0x50;
    const VK_R: u32 = 0x52;
    const VK_S: u32 = 0x53;
    const VK_T: u32 = 0x54;
    const VK_W: u32 = 0x57;
    const VK_LEFT: u32 = 0x25;
    const VK_UP: u32 = 0x26;
    const VK_RIGHT: u32 = 0x27;
    const VK_DOWN: u32 = 0x28;
    const VK_OEM_COMMA: u32 = 0xBC;
    if alt {
        // Only the pane chords use Alt; `Alt+Left/Right` (history), `Alt+D`
        // (location) and `AltGr` typing must keep reaching the page.
        return ctrl && matches!(vk, VK_LEFT | VK_UP | VK_RIGHT | VK_DOWN | VK_W);
    }
    if !ctrl {
        // Plain `Escape` closes settings/palette; every other bare key is
        // typing or page navigation. (`Ctrl+Esc` opens Start, `Shift+Esc`
        // is untouched — no arm below lists them — so the OS and the page
        // keep them.)
        return vk == VK_ESCAPE && !shift;
    }
    if shift {
        matches!(vk, VK_P | VK_L | VK_D | VK_R | VK_H | VK_B | VK_W | VK_TAB)
    } else {
        matches!(
            vk,
            VK_L | VK_T | VK_O | VK_S | VK_W | VK_K | VK_TAB | VK_OEM_COMMA
        ) || (0x31..=0x39).contains(&vk)
    }
}

/// #368: the engine build for a deferred surface, off every GPUI lease.
///
/// Runs from a foreground task after `new` has returned, so no `App` borrow
/// is held while WebView2's `wait_for_async_operation` pumps the message
/// loop. A queued tick (`ensure_tree_refresh`, `changes::refresh`, any
/// other) then borrows cleanly instead of panicking on a second
/// `borrow_mut`.
#[cfg(target_os = "windows")]
fn build_deferred_webview(
    parent_hwnd: isize,
    initial_url: &str,
    events: &SharedWebEvents,
    context: &mut WebContext,
) -> Result<WebView, String> {
    let parent = DeferredParent::from_hwnd(parent_hwnd)?;
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        build_production_webview(&parent, initial_url, events, context)
    })) {
        Ok(Ok(webview)) => Ok(webview),
        Ok(Err(error)) => Err(format!("{NATIVE_ENGINE} child failed: {error}")),
        Err(_) => Err(format!(
            "{NATIVE_ENGINE} child failed: GPUI did not expose a usable \
             {NATIVE_WINDOW_HANDLE} window handle"
        )),
    }
}

/// #368: the pending failure a deferred surface carries before its webview
/// exists. Pure so the rule stays testable without a window: a missing
/// runtime explains itself immediately, otherwise an invalid initial URL
/// keeps its fallback error for the later build to preserve.
fn pending_startup_failure(
    runtime_missing: bool,
    startup_error: Option<String>,
) -> Option<StartupFailure> {
    if runtime_missing {
        return Some(StartupFailure::RuntimeMissing);
    }
    startup_error.map(StartupFailure::Failed)
}

/// #368: what a deferred build installs. Pure so the merge stays testable:
/// a live webview keeps whatever the surface already carried (e.g. the
/// invalid-URL fallback error), while a failed build replaces it with the
/// engine error.
fn deferred_install_failure(
    existing: Option<StartupFailure>,
    outcome_failure: Option<StartupFailure>,
    has_webview: bool,
) -> Option<StartupFailure> {
    if has_webview {
        existing
    } else {
        outcome_failure.or(existing)
    }
}

/// A production browser surface: all browser chrome is GPUI, while page
/// pixels live in the native child window below it.
pub struct BrowserSurface {
    state: BrowserState,
    address_field: gpui::Entity<TextField>,
    /// F-BRW-03: mirrors the TextField focus handle, refreshed each
    /// render (the only place a `Window` is available). Guards the
    /// PageLoad(Finished) address resync below from clobbering a caret or
    /// selection the user is actively editing.
    address_focused: bool,
    webview: SharedWebView,
    /// R6.2/R6.3: the engine's profile context, held for the surface's life.
    /// `WebViewBuilder` only borrows it while building, so this is ownership
    /// rather than a back-reference -- but the engine keeps using the
    /// directory it names, so dropping it early would pull the profile out
    /// from under a live webview.
    ///
    /// An `Option` so a retry (#307) can hand the context to the engine
    /// build while the surface's GPUI lease is released, then take it back;
    /// see `take_retry_attempt`.
    _web_context: Option<WebContext>,
    /// F-BRW-01: native-side geometry correction, calibrated once against
    /// the child bounds reported by wry; see [`SharedScaleCorrection`].
    webview_scale_correction: SharedScaleCorrection,
    /// F-BRW: whether the native child is mapped; see [`SharedNativeVisibility`].
    webview_visible: SharedNativeVisibility,
    /// #376: whether a covering GPUI overlay is open; see [`SharedOverlayObscured`].
    overlay_obscured: SharedOverlayObscured,
    web_events: SharedWebEvents,
    events: Vec<BrowserEvent>,
    /// Load-bearing by existing, not by being read. A GPUI [`Task`] is
    /// cancel-on-drop, so this field owns the 16 ms event task that calls
    /// `pump_web_events`; on Linux it also drains GTK's event queue. Drop the
    /// field and the browser event state stops being observed.
    ///
    /// The `dead_code` allow is therefore deliberate: it is not a task whose
    /// handle went missing. An independent critic read the bare warning as a
    /// "possible task-cancellation gap", which is the natural misreading, so
    /// the reason is written here rather than left to be re-derived.
    #[allow(dead_code)]
    pump_task: Option<Task<()>>,
    /// Why the native child never came up, if it didn't; see [`StartupFailure`].
    startup_failure: Option<StartupFailure>,
}

/// #307: whether the platform's own webview runtime is absent. Only
/// Windows answers this today: WebView2 is a machine-level Microsoft
/// component an installed Sirio can legitimately lack (the installer's
/// bootstrap attempt is deliberately non-fatal — its most common failure
/// is simply having no network), and wry's version probe is exactly the
/// "is it there" question. Everywhere else the runtime ships with the
/// OS, and this reports `false`, so nothing about the surface changes.
fn webview_runtime_missing() -> bool {
    #[cfg(target_os = "windows")]
    {
        wry::webview_version().is_err()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// #307: what a retry's engine build needs in hand while the surface's
/// lease is released — the profile context taken out of the surface, the
/// shared event sink the new webview must feed, and the address to load.
struct RetryAttempt {
    web_context: WebContext,
    web_events: SharedWebEvents,
    address: String,
}

impl RetryAttempt {
    /// Runs the actual engine creation, off the surface's lease (see
    /// `take_retry_attempt` for why that matters). Probes the runtime
    /// first, the same way `new` does: still absent means the explanation
    /// stands, and the context is handed back untouched.
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    fn build(self, window: &Window) -> RetryOutcome {
        let RetryAttempt {
            mut web_context,
            web_events,
            address,
        } = self;
        if webview_runtime_missing() {
            return RetryOutcome {
                webview: None,
                failure: Some(StartupFailure::RuntimeMissing),
                web_context: Some(web_context),
            };
        }
        match build_production_webview_for_platform(window, &address, &web_events, &mut web_context)
        {
            Ok(webview) => RetryOutcome {
                webview: Some(webview),
                failure: None,
                web_context: Some(web_context),
            },
            Err(error) => RetryOutcome {
                webview: None,
                failure: Some(StartupFailure::Failed(error)),
                web_context: Some(web_context),
            },
        }
    }

    /// #368: the same engine build, but parented to a captured HWND instead
    /// of a live `Window`, so it can run from a foreground task with no App
    /// borrow held. The pump inside WebView2 init then has no outer
    /// `borrow_mut` to re-enter.
    #[cfg(target_os = "windows")]
    fn build_deferred(self, parent_hwnd: isize) -> RetryOutcome {
        let RetryAttempt {
            mut web_context,
            web_events,
            address,
        } = self;
        if webview_runtime_missing() {
            return RetryOutcome {
                webview: None,
                failure: Some(StartupFailure::RuntimeMissing),
                web_context: Some(web_context),
            };
        }
        match build_deferred_webview(parent_hwnd, &address, &web_events, &mut web_context) {
            Ok(webview) => RetryOutcome {
                webview: Some(webview),
                failure: None,
                web_context: Some(web_context),
            },
            Err(error) => RetryOutcome {
                webview: None,
                failure: Some(StartupFailure::Failed(error)),
                web_context: Some(web_context),
            },
        }
    }
}

/// #307: the result of a retry's engine build, plus the profile context
/// handed back for the surface to keep holding.
struct RetryOutcome {
    webview: Option<WebView>,
    failure: Option<StartupFailure>,
    web_context: Option<WebContext>,
}

impl BrowserSurface {
    /// Creates a browser surface attached to the current GPUI native window.
    ///
    /// The caller owns the returned entity and can consume [`BrowserEvent`]
    /// values with [`Self::take_events`]. Persistence of browser grants stays
    /// with the host; this view accepts snapshots via [`BrowserState`].
    pub fn new(initial_url: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (state, startup_error) = match BrowserState::new(initial_url) {
            Ok(state) => (state, None),
            Err(error) => (
                BrowserState::new("https://example.com").expect("fallback URL is valid"),
                Some(error.to_string()),
            ),
        };
        let address_field = cx.new(|cx| {
            let mut field = TextField::new(cx).with_placeholder("");
            field.set_content(state.address(), cx);
            field
        });
        let web_events = Rc::new(RefCell::new(Vec::new()));
        // R6.3: one shared profile in a Sirio-owned directory. Left unset,
        // WebView2 writes `<exe>.WebView2\EBWebView` beside the binary, which
        // an installed Sirio under Program Files cannot create.
        let web_context = WebContext::new(browser_profile_dir());
        #[cfg(not(target_os = "windows"))]
        {
            let mut web_context = web_context;
            // #307: a Windows machine without the WebView2 Runtime gets a full
            // explanation in place of the content, not an engine error it can't
            // act on. The probe runs before the build: it answers the one
            // question the build error cannot -- "is the runtime there at all".
            let (webview, startup_failure) = if webview_runtime_missing() {
                (None, Some(StartupFailure::RuntimeMissing))
            } else {
                match build_production_webview_for_platform(
                    window,
                    state.address(),
                    &web_events,
                    &mut web_context,
                ) {
                    Ok(webview) => (Some(webview), startup_error.map(StartupFailure::Failed)),
                    Err(error) => (None, Some(StartupFailure::Failed(error))),
                }
            };
            let webview = Rc::new(RefCell::new(webview));
            // #255: armed on first render, never here -- see `ensure_pump_task`.
            let pump_task = None;

            Self {
                state,
                address_field,
                address_focused: false,
                webview,
                _web_context: Some(web_context),
                webview_scale_correction: Rc::new(Cell::new(None)),
                webview_visible: initial_native_visibility(),
                overlay_obscured: Rc::new(Cell::new(false)),
                web_events,
                events: Vec::new(),
                pump_task,
                startup_failure,
            }
        }
        #[cfg(target_os = "windows")]
        {
            // #368: never build WebView2 while the App is borrowed. `new`
            // runs inside `cx.new` (itself inside the workspace `update`),
            // and WebView2 init pumps `DispatchMessageW`, which runs any
            // queued GPUI foreground task (`ensure_tree_refresh`,
            // `changes::refresh`, ...) straight into a second `borrow_mut`
            // -> "RefCell already borrowed" -> abort. The same race fires
            // from session restore, which builds every tab in one update.
            //
            // Return a pending surface now (chrome + loading state, no
            // child) and build the child from a foreground task with no
            // borrow held. The HWND is captured here -- cheap, pump-free --
            // because the `Window` cannot cross the spawn boundary.
            let startup_failure = pending_startup_failure(webview_runtime_missing(), startup_error);
            let parent_hwnd = capture_window_hwnd(window).ok();
            let should_defer = !matches!(startup_failure, Some(StartupFailure::RuntimeMissing))
                && parent_hwnd.is_some();
            // A HWND capture failure without a runtime-missing explanation
            // still needs words in place of the content.
            let startup_failure = match (startup_failure, parent_hwnd) {
                (None, None) => Some(StartupFailure::Failed(format!(
                    "{NATIVE_ENGINE} child failed: GPUI did not expose a usable \
                     {NATIVE_WINDOW_HANDLE} window handle"
                ))),
                (failure, _) => failure,
            };
            let surface = Self {
                state,
                address_field,
                address_focused: false,
                webview: Rc::new(RefCell::new(None)),
                _web_context: Some(web_context),
                webview_scale_correction: Rc::new(Cell::new(None)),
                webview_visible: initial_native_visibility(),
                overlay_obscured: Rc::new(Cell::new(false)),
                web_events,
                events: Vec::new(),
                pump_task: None,
                startup_failure,
            };
            if should_defer {
                let parent_hwnd = parent_hwnd.expect("deferred only when HWND captured");
                cx.spawn(async move |this, cx| {
                    let attempt = this
                        .update(cx, |surface, _| surface.take_retry_attempt())
                        .ok()
                        .flatten();
                    let Some(attempt) = attempt else {
                        return;
                    };
                    let outcome = attempt.build_deferred(parent_hwnd);
                    let _ = this.update(cx, |surface, cx| {
                        surface.install_deferred(outcome, cx);
                        #[cfg(target_os = "windows")]
                        surface.attach_host_accelerators(parent_hwnd);
                    });
                })
                .detach();
            }
            return surface;
        }
    }

    /// Arms the 16 ms task that drains this surface's web events.
    ///
    /// Deliberately armed from `render`, not from `new` (#255). A surface
    /// built inside an `App::update` -- which is exactly what session restore
    /// does, building every tab in one update -- would otherwise leave a
    /// foreground task queued behind it. Constructing the *next* webview
    /// pumps the platform message loop from inside WebView2's own
    /// initialisation, that pump runs the queued task, and the task's
    /// `update` re-enters the borrow the outer `cx.new` is still holding:
    /// "RefCell already borrowed", with the window never opening at all.
    /// One Browser tab could not trigger it -- there was no earlier task to
    /// run -- and two always did.
    ///
    /// Arming from render puts the first tick strictly after construction has
    /// returned. It is the same one-timer-per-surface discipline
    /// the streaming border already follows, and it
    /// closes the class rather than the two-tab instance of it.
    ///
    /// Idempotent, and it does not resurrect a closed surface: `close_native`
    /// empties the webview cell, so the guard below stays false afterwards.
    fn ensure_pump_task(&mut self, cx: &mut Context<Self>) {
        if !should_arm_pump(self.pump_task.is_some(), self.webview.borrow().is_some()) {
            return;
        }
        #[cfg(target_os = "linux")]
        {
            self.pump_task = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    if this
                        .update(cx, |surface, cx| {
                            // #195: the GTK iteration must keep running --
                            // it is what drives the webview at all -- but
                            // the repaint is only owed when the page has
                            // actually done something.
                            let drained = surface.pump_web_events(cx);
                            #[cfg(target_os = "linux")]
                            while gtk::events_pending() {
                                gtk::main_iteration_do(false);
                            }
                            if drained {
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
        #[cfg(not(target_os = "linux"))]
        {
            self.pump_task = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    if this
                        .update(cx, |surface, cx| {
                            // #195: a 60 Hz unconditional notify repainted
                            // the whole window for as long as any browser
                            // surface existed.
                            if surface.pump_web_events(cx) {
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
    }

    /// Borrows the testable state for host synchronization.
    pub fn state(&self) -> &BrowserState {
        &self.state
    }

    /// Moves focus from the native page child back to GPUI's address field.
    ///
    /// `Window::focus` only updates GPUI's focus tree. A wry child is a real
    /// native window and can keep the operating system focus after a page
    /// click, so hand focus back to its parent before focusing the TextField.
    pub fn focus_address_bar(&self, window: &mut Window, cx: &mut App) {
        #[cfg(target_os = "windows")]
        steal_win32_focus_from_webview_child(window);
        if let Some(webview) = self.webview.borrow().as_ref() {
            let _ = webview.focus_parent();
        }
        let focus = self.address_field.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    // `impl Focusable for BrowserSurface` below gives the host (F-WIN-06's
    // "Focus Address Bar" command) a way to move keyboard focus into the
    // address field without going through a synthetic click, the same way
    // `Chat::focus_handle` hands out its composer's handle.

    /// #369 follow-up: forward host chords pressed while the page holds
    /// Win32 focus.
    ///
    /// Stealing focus inside `focus_address_bar` cannot suffice on its own:
    /// after a page click the WebView2 child owns Win32 focus and consumes
    /// its accelerators (`Ctrl+L`, `Ctrl+Shift+P`, …), so the chord never
    /// reaches the GPUI window and the steal never runs. This hooks
    /// `ICoreWebView2Controller::add_AcceleratorKeyPressed`: chords in
    /// [`should_forward_accelerator_to_host`] are marked handled (the page
    /// and Edge never see them — no print dialog on `Ctrl+Shift+P`) and
    /// reposted to the parent top-level, stealing Win32 focus first so the
    /// redelivered key — and every key after it — lands in GPUI. All other
    /// keys are left to the page untouched.
    ///
    /// Runs on the UI thread that owns the controller (the same foreground
    /// task that built it, #368); the closure captures only the parent HWND
    /// integer, touches no GPUI lease, and uses `PostMessage` (never `Send`),
    /// so it cannot re-enter a borrow the way synchronous creation pumps do
    /// (#255). The registration is fire-and-forget like wry's own handlers:
    /// COM holds its reference, and destroying the webview (`close_native`)
    /// releases it with the controller.
    #[cfg(target_os = "windows")]
    fn attach_host_accelerators(&self, parent_hwnd: isize) {
        use webview2_com::{
            AcceleratorKeyPressedEventHandler,
            Microsoft::Web::WebView2::Win32::{
                COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN, COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
            },
        };
        use wry::WebViewExtWindows;
        let controller = {
            let borrowed = self.webview.borrow();
            let Some(webview) = borrowed.as_ref() else {
                return;
            };
            webview.controller()
        };
        let handler = AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
            use windows_sys::Win32::UI::{
                Input::KeyboardAndMouse::GetKeyState, WindowsAndMessaging::PostMessageW,
            };
            let Some(args) = args else {
                return Ok(());
            };
            let mut kind = COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN;
            let mut vk = 0u32;
            let mut lparam = 0i32;
            unsafe {
                args.KeyEventKind(&mut kind)?;
                args.VirtualKey(&mut vk)?;
                args.KeyEventLParam(&mut lparam)?;
            }
            // Key releases need no forwarding: the steal below already moved
            // Win32 focus to the parent, so releases arrive there naturally;
            // reposting them would only double-deliver what the host ignores
            // anyway (actions fire on key-down).
            if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
            {
                return Ok(());
            }
            const VK_SHIFT: i32 = 0x10;
            const VK_CONTROL: i32 = 0x11;
            const VK_MENU: i32 = 0x12;
            let held = |vk: i32| unsafe { (GetKeyState(vk) as u16 & 0x8000) != 0 };
            if !should_forward_accelerator_to_host(
                held(VK_CONTROL),
                held(VK_SHIFT),
                held(VK_MENU),
                vk,
            ) {
                return Ok(());
            }
            steal_win32_focus_to_parent(parent_hwnd);
            const WM_KEYDOWN: u32 = 0x0100;
            const WM_SYSKEYDOWN: u32 = 0x0104;
            let message = if kind == COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN {
                WM_SYSKEYDOWN
            } else {
                WM_KEYDOWN
            };
            unsafe {
                args.SetHandled(true)?;
                PostMessageW(
                    parent_hwnd as windows_sys::Win32::Foundation::HWND,
                    message,
                    vk as usize,
                    lparam as isize,
                );
            }
            Ok(())
        }));
        let mut token = 0i64;
        let _ = unsafe { controller.add_AcceleratorKeyPressed(&handler, &mut token) };
    }

    /// Map or unmap the native child window.
    ///
    /// The host has to call this because GPUI cannot: the webview is an X11
    /// child window above GPUI's surface, so dropping the element from the
    /// render tree hides nothing. Every surface that stops being shown — a tab
    /// that is no longer the group's active one, any browser at all while
    /// Settings covers the pane area — has to be told, or its last page keeps
    /// painting over whatever takes that rectangle next.
    ///
    /// Safe to call every frame; it is a no-op when the state already agrees.
    pub fn set_native_visible(&self, visible: bool) {
        apply_native_visible(&self.webview, &self.webview_visible, visible);
    }

    /// Whether the native child is currently mapped. Exists for tests — wry
    /// offers no read-back, so this reports our mirror of it.
    pub fn native_visible(&self) -> bool {
        self.webview_visible.get()
    }

    /// #376: marks this surface as covered by a GPUI overlay (menu, popover,
    /// palette, modal) that the native child would otherwise paint over.
    ///
    /// Takes `&self` like [`Self::set_native_visible`] so the host can sync
    /// it every frame from `read` during `render`. The flag itself hides
    /// nothing: [`NativeWebViewElement::prepaint`] reads it after `render`
    /// and unmaps instead of mapping, which is what keeps the hide from
    /// being undone the same frame (the element stays mounted for layout,
    /// so an unconditional `set_native_visible(false)` here would lose to
    /// the prepaint that runs right after). Clearing the flag lets the next
    /// prepaint map the child again, so no explicit re-show call is needed.
    pub fn set_overlay_obscured(&self, obscured: bool) {
        self.overlay_obscured.set(obscured);
    }

    /// Whether this surface is currently marked as covered by a GPUI overlay.
    pub fn overlay_obscured(&self) -> bool {
        self.overlay_obscured.get()
    }

    /// Tear the native child down for good, on the close path.
    ///
    /// [`Self::set_native_visible`] is not enough here and `Drop` is not a
    /// close path. Both issue their X requests into a buffer that only this
    /// surface's own pump drains, and closing the tab is precisely when that
    /// pump goes away — see [`flush_native_window_ops`]. So the host has to
    /// say "closed" explicitly, exactly as `close_tab` already interrupts a
    /// terminal rather than trusting its `Drop`.
    ///
    /// Idempotent: a second call finds an empty cell and does nothing.
    pub fn close_native(&mut self) {
        close_native_window(&self.webview, &self.webview_visible);
        // The pump exists to service a webview that no longer exists. Dropping
        // the task cancels it (GPUI tasks are cancel-on-drop); leaving it would
        // keep iterating GTK every 16 ms for a dead surface.
        self.pump_task = None;
    }

    /// The error produced while constructing this surface (e.g. an invalid
    /// initial address that forced the `https://example.com` fallback), if
    /// any. Distinct from [`BrowserState::error`], which tracks in-flight
    /// navigation failures after construction.
    ///
    /// Every failure flavor reports `Some` here — including #307's missing
    /// WebView2 runtime — so the host's `browser.open` keeps answering
    /// `ok:false` for a dead surface no matter why it is dead.
    pub fn startup_error(&self) -> Option<&str> {
        self.startup_failure.as_ref().map(StartupFailure::message)
    }

    /// #307: takes out of the surface everything a retry's engine build
    /// needs, so the build can run while NO GPUI lease on this surface is
    /// held. That release is the whole point: creating a WebView2 pumps the
    /// platform message loop (#255), and a task queued for this surface —
    /// the address field — must be able to lease it during the pump
    /// instead of panicking on a double lease. Returns `None` when there is
    /// no profile context to hand the engine, i.e. retrying cannot even be
    /// attempted.
    fn take_retry_attempt(&mut self) -> Option<RetryAttempt> {
        Some(RetryAttempt {
            web_context: self._web_context.take()?,
            web_events: self.web_events.clone(),
            address: self.state.address().to_owned(),
        })
    }

    /// #307: installs a retry's outcome. A live webview — whose mirror
    /// starts mapped exactly as in `new`, because wry maps the child on
    /// creation — or the failure to explain: the same runtime-missing
    /// explanation stands, or a new engine failure takes the banner. The
    /// pump task re-arms by itself from the next `render`, through the same
    /// `should_arm_pump` guard that closes surfaces stay closed behind.
    fn install_retry(&mut self, outcome: RetryOutcome, cx: &mut Context<Self>) {
        self._web_context = outcome.web_context;
        self.startup_failure = outcome.failure;
        if let Some(webview) = outcome.webview {
            self.webview_visible = initial_native_visibility();
            *self.webview.borrow_mut() = Some(webview);
        }
        cx.notify();
    }

    /// #368: installs a deferred initial build. Unlike a retry, a live
    /// webview keeps whatever the pending surface already carried (the
    /// invalid-URL fallback error, if any): clearing it would turn a known
    /// bad address into a silent success. Only a failed build replaces the
    /// explanation.
    #[cfg(target_os = "windows")]
    fn install_deferred(&mut self, outcome: RetryOutcome, cx: &mut Context<Self>) {
        self._web_context = outcome.web_context;
        let has_webview = outcome.webview.is_some();
        if let Some(webview) = outcome.webview {
            self.webview_visible = initial_native_visibility();
            *self.webview.borrow_mut() = Some(webview);
        }
        self.startup_failure =
            deferred_install_failure(self.startup_failure.take(), outcome.failure, has_webview);
        cx.notify();
    }

    /// #307: the full-content explanation a Windows machine without the
    /// WebView2 Runtime sees, with its one action. Rendered in place of the
    /// webview region — not as a banner — so the pane cannot be mistaken
    /// for an empty one, and the failure names the one thing that fixes it.
    fn render_runtime_missing(&self, theme: Theme, entity: gpui::Entity<Self>) -> impl IntoElement {
        div()
            .id("browser-runtime-missing")
            .debug_selector(|| "browser-runtime-missing".to_owned())
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.0))
            .px(px(24.0))
            .child(
                div()
                    .text_size(theme.typography.headline)
                    .text_color(theme.text)
                    .child("Sirio needs the Microsoft Edge WebView2 Runtime"),
            )
            .child(
                div()
                    .max_w(px(520.0))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_faint)
                    .child(RUNTIME_MISSING_MESSAGE),
            )
            .child(
                div()
                    .id("browser-runtime-retry")
                    .debug_selector(|| "browser-runtime-retry".to_owned())
                    .h(px(30.0))
                    .px(px(14.0))
                    .mt(px(6.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radii.control)
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text)
                    .hover(|style| style.bg(theme.element_hover))
                    // The rebuild must NOT run under this surface's GPUI
                    // lease: creating a WebView2 pumps the platform message
                    // loop (#255), and a task queued for this surface — the
                    // address field — would re-enter the lease and
                    // panic. Take what the build needs out, build lease-free,
                    // then reinstall the outcome.
                    //
                    // #368: on Windows the build must additionally run with
                    // no App borrow held at all. A queued tick for any
                    // other entity (`ensure_tree_refresh`,
                    // `changes::refresh`, ...) would otherwise re-enter
                    // the outer borrow through the same pump and abort
                    // exactly like a new tab does. Capture the HWND now
                    // (pump-free) and build from a foreground task.
                    .on_click(move |_, window, cx| {
                        #[cfg(target_os = "windows")]
                        {
                            let Ok(parent_hwnd) = capture_window_hwnd(window) else {
                                entity.update(cx, |surface, cx| {
                                    surface.startup_failure = Some(StartupFailure::Failed(
                                        format!(
                                            "{NATIVE_ENGINE} child failed: GPUI did not expose a usable \
                                             {NATIVE_WINDOW_HANDLE} window handle"
                                        ),
                                    ));
                                    cx.notify();
                                });
                                return;
                            };
                            let Some(attempt) =
                                entity.update(cx, |surface, _| surface.take_retry_attempt())
                            else {
                                return;
                            };
                            let entity_weak = entity.downgrade();
                            cx.spawn(async move |cx| {
                                let outcome = attempt.build_deferred(parent_hwnd);
                                let _ = entity_weak.update(cx, |surface, cx| {
                                    surface.install_retry(outcome, cx);
                                    #[cfg(target_os = "windows")]
                                    surface.attach_host_accelerators(parent_hwnd);
                                });
                            })
                            .detach();
                            return;
                        }
                        #[cfg(not(target_os = "windows"))]
                        {
                            let Some(attempt) =
                                entity.update(cx, |surface, _| surface.take_retry_attempt())
                            else {
                                return;
                            };
                            let outcome = attempt.build(window);
                            entity.update(cx, |surface, cx| surface.install_retry(outcome, cx));
                        }
                    })
                    .child("Retry"),
            )
    }

    /// Records an out-of-band navigation failure (e.g. a control-socket
    /// reachability probe run before the address was handed to WebKit) in
    /// the same visible error banner WebKit's own failures use.
    pub fn record_navigation_error(&mut self, message: impl Into<String>) {
        self.state.did_fail_navigation(message);
    }

    /// Returns and clears browser events produced by WebKit or an explicit
    /// host action.
    pub fn take_events(&mut self) -> Vec<BrowserEvent> {
        std::mem::take(&mut self.events)
    }

    /// Starts an address-field navigation in WebKit.
    pub fn submit_address(&mut self, input: &str) -> Result<(), BrowserError> {
        let address = self.state.submit_address(input)?;
        self.load_url(&address)
    }

    /// Navigates to a link according to the internal/external routing choice.
    pub fn open_link(
        &mut self,
        url: &str,
        target: BrowserLinkTarget,
    ) -> Result<BrowserEvent, BrowserError> {
        let event = self.state.open_link(url, target)?;
        if let BrowserEvent::Navigate(address) = &event {
            self.load_url(address)?;
        }
        self.events.push(event.clone());
        Ok(event)
    }

    /// Requests a GPUI doorhanger for an agent origin.
    pub fn request_permission(&mut self, origin: &str) {
        self.state.request_permission(origin);
    }

    /// Allows and returns the origin for host persistence.
    pub fn allow_permission(&mut self) -> Option<String> {
        self.state.allow_permission()
    }

    /// Denies and returns the origin for host telemetry.
    pub fn deny_permission(&mut self) -> Option<String> {
        self.state.deny_permission()
    }

    /// Seeds this surface from the host's durable browser-origin grants.
    pub fn set_allowed_origins(&mut self, origins: impl IntoIterator<Item = String>) {
        self.state.set_allowed_origins(origins);
    }

    /// Returns the origins currently allowed by this surface.
    pub fn allowed_origins(&self) -> impl Iterator<Item = &str> {
        self.state.allowed_origins()
    }

    /// Updates the F-BRW-05 activity marker.
    pub fn set_agent_driving(&mut self, driving: bool) {
        self.state.set_agent_driving(driving);
    }

    /// F-CTRL-BROWSER-06: runs `script` in the page and returns its result
    /// (JSON-serialized by WebKit) as a string, or an error if the page
    /// throws or the timeout elapses first. Linux drains the WebKitGTK/GLib
    /// queue while waiting; macOS waits for WKWebView's completion callback
    /// and leaves AppKit's run loop to GPUI.
    pub fn evaluate_script(&self, script: &str, timeout: Duration) -> Result<String, String> {
        let webview_ref = self.webview.borrow();
        let webview = webview_ref
            .as_ref()
            .ok_or_else(|| "Browser child is unavailable".to_string())?;
        let (sender, receiver) = mpsc::channel();
        webview
            .evaluate_script_with_callback(script, move |value| {
                let _ = sender.send(value);
            })
            .map_err(|error| format!("evaluate_script failed: {error}"))?;
        wait_for_script_result(receiver, timeout)
    }

    /// Runs `script` and hands its result to `on_result` when the engine
    /// delivers it, **without blocking the calling thread** (#142).
    ///
    /// This exists because [`Self::evaluate_script`] cannot work off Linux.
    /// There it ends in `receiver.recv_timeout`, which blocks the very thread
    /// the engine needs in order to deliver the completion — WKWebView's on
    /// macOS, WebView2's STA on Windows — so the result can never arrive and
    /// every call burns exactly its timeout. The script itself runs fine: a
    /// probe that set `document.title` had the new title reach the chrome
    /// while the call was still timing out. It is a delivery failure, not an
    /// execution one.
    ///
    /// Pumping the platform's event loop instead is specifically ruled out:
    /// on macOS that re-enters GPUI while the workspace entity is mutably
    /// borrowed. The caller keeps its own reply channel and answers late.
    ///
    /// `Err` here means the script could not be *started*; a script that runs
    /// and fails reports through `on_result`.
    pub fn evaluate_script_async(
        &self,
        script: &str,
        on_result: impl FnOnce(String) + Send + 'static,
    ) -> Result<(), String> {
        let webview_ref = self.webview.borrow();
        let webview = webview_ref
            .as_ref()
            .ok_or_else(|| "Browser child is unavailable".to_string())?;
        // wry hands us an `Fn`, but the caller's answer may only be sent once
        // — a reply channel is consumed by sending. Hold it in a `Mutex` and
        // take it on the first call, so a duplicate completion is ignored
        // rather than panicking or answering twice.
        let on_result = std::sync::Mutex::new(Some(on_result));
        webview
            .evaluate_script_with_callback(script, move |value| {
                let taken = on_result
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                if let Some(on_result) = taken {
                    on_result(value);
                }
            })
            .map_err(|error| format!("evaluate_script failed: {error}"))
    }

    fn load_url(&mut self, address: &str) -> Result<(), BrowserError> {
        if let Some(webview) = self.webview.borrow().as_ref() {
            webview
                .load_url(address)
                .map_err(|error| BrowserError::Navigation(error.to_string()))?;
        }
        Ok(())
    }

    fn navigate_history(&mut self, address: Option<String>, cx: &mut Context<Self>) {
        let Some(address) = address else {
            return;
        };
        let result = self.load_url(&address);
        if let Err(error) = result {
            self.state.did_fail_navigation(error.to_string());
        } else {
            self.address_field
                .update(cx, |field, cx| field.set_content(address, cx));
        }
    }

    fn on_back(&mut self, cx: &mut Context<Self>) {
        let address = self.state.go_back();
        self.navigate_history(address, cx);
        // F-BRW-02: go_back()/navigate_history() mutate self.state and
        // the state and TextField correctly, but nothing repaints without an
        // explicit notify — unlike on_address_key's "enter" path, which
        // does call it after submit_address().
        cx.notify();
    }

    fn on_forward(&mut self, cx: &mut Context<Self>) {
        let address = self.state.go_forward();
        self.navigate_history(address, cx);
        cx.notify();
    }

    fn on_reload(&mut self, cx: &mut Context<Self>) {
        let Some(address) = self.state.reload() else {
            return;
        };
        if let Some(webview) = self.webview.borrow().as_ref()
            && let Err(error) = webview.reload()
        {
            self.state.did_fail_navigation(error.to_string());
        } else if self.webview.borrow().is_none() {
            self.state
                .did_fail_navigation("Browser child is unavailable".to_owned());
        }
        self.address_field
            .update(cx, |field, cx| field.set_content(address, cx));
    }

    fn on_stop(&mut self) {
        if self.state.stop_loading()
            && let Some(webview) = self.webview.borrow().as_ref()
        {
            let _ = webview.evaluate_script("window.stop();");
        }
    }

    fn on_address_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if event.keystroke.key == "enter" {
            let draft = self.address_field.read(cx).content().to_string();
            if self.submit_address(&draft).is_ok() {
                let address = self.state.address().to_owned();
                self.address_field
                    .update(cx, |field, cx| field.set_content(address, cx));
            }
            cx.notify();
        }
    }

    /// Drains the queue the webview callbacks push into, and reports
    /// whether there was anything in it (#195).
    ///
    /// The return value is what decides whether the pump notifies. The
    /// pump runs at 60 Hz, and it used to notify on every one of those
    /// ticks whether or not the page had done anything, which forced a
    /// full repaint sixty times a second for as long as any browser
    /// surface existed. Measured on Windows against a static local page
    /// with no scripts, no animation and no timers, that cost 27.6% of a
    /// core against a 5.8% baseline, and backgrounding the tab saved
    /// nothing.
    fn pump_web_events(&mut self, cx: &mut Context<Self>) -> bool {
        let pending = std::mem::take(&mut *self.web_events.borrow_mut());
        let drained = !pending.is_empty();
        for event in pending {
            match event {
                WebEvent::NavigationRequested(url) => {
                    self.state.did_start_navigation(&url);
                    self.events.push(BrowserEvent::Navigate(url));
                }
                WebEvent::NewWindowRequested(url) => {
                    if let Ok(url) = normalize_address(&url) {
                        self.events.push(BrowserEvent::OpenExternal(url));
                    }
                }
                WebEvent::PageLoad(PageLoadEventKind::Started, url) => {
                    self.state.did_start_navigation(&url);
                    self.events.push(BrowserEvent::PageStarted(url));
                }
                WebEvent::PageLoad(PageLoadEventKind::Finished, url) => {
                    let title = self.state.page_title().to_owned();
                    self.state.did_finish_navigation(&url, &title);
                    // F-BRW-03: WebKit fires PageLoad(Finished) once per
                    // sub-resource/iframe, not just once per navigation.
                    // Resyncing the address field on every one of those
                    // events reset the caret to end-of-text mid-edit,
                    // turning a click-to-position or ctrl+a select-all into
                    // a no-op the instant a stray Finished event landed —
                    // the field could then only ever append. Skip the
                    // resync while the user is at the field; `submit_address`
                    // already sets the authoritative text on Enter.
                    if !self.address_focused {
                        let address = self.state.address().to_owned();
                        self.address_field
                            .update(cx, |field, cx| field.set_content(address, cx));
                    }
                    self.events.push(BrowserEvent::PageFinished(url));
                }
                WebEvent::TitleChanged(title) => {
                    self.state.page_title = title.clone();
                    self.events.push(BrowserEvent::TitleChanged(title));
                }
            }
        }
        drained
    }

    fn render_toolbar(
        &self,
        theme: Theme,
        entity: gpui::Entity<Self>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let address_field = self.address_field.clone();
        let focus_address = entity.clone();
        let address_entity = entity.clone();
        let back = entity.clone();
        let forward = entity.clone();
        let reload = entity.clone();
        let stop = entity.clone();
        let page_title = if self.state.page_title().is_empty() {
            "Browser".to_owned()
        } else {
            self.state.page_title().to_owned()
        };
        let reload_content = if self.state.is_loading() {
            loading::compact("browser-reload-spinner", window, cx)
        } else {
            div().child("↻").into_any_element()
        };
        div()
            .id("browser-toolbar")
            .debug_selector(|| "browser-toolbar".to_owned())
            .h(px(48.0))
            .w_full()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(12.0))
            .bg(theme.bg)
            .border_b_1()
            .border_color(theme.border)
            .child(browser_button(
                "browser-back",
                "‹",
                theme,
                self.state.can_go_back(),
                move |cx| back.update(cx, |surface, cx| surface.on_back(cx)),
            ))
            .child(browser_button(
                "browser-forward",
                "›",
                theme,
                self.state.can_go_forward(),
                move |cx| forward.update(cx, |surface, cx| surface.on_forward(cx)),
            ))
            .child(browser_button_element(
                "browser-reload",
                reload_content,
                theme,
                true,
                move |cx| {
                    reload.update(cx, |surface, cx| {
                        if surface.state.is_loading() {
                            surface.on_stop();
                        } else {
                            surface.on_reload(cx);
                        }
                    })
                },
            ))
            .child(
                div()
                    .id("browser-address-field")
                    .debug_selector(|| "browser-address-field".to_owned())
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .on_click(move |_, window, cx| {
                        focus_address.update(cx, |surface, cx| {
                            surface.focus_address_bar(window, cx);
                        });
                    })
                    .on_key_down(move |event, _, cx| {
                        address_entity.update(cx, |surface, cx| {
                            surface.on_address_key(event, cx);
                        });
                    })
                    .child(
                        div()
                            .ml(px(10.0))
                            .mr(px(8.0))
                            .text_color(theme.text_faint)
                            .child("◎"),
                    )
                    .child(div().flex_1().min_w_0().child(address_field)),
            )
            .child(
                div()
                    .id("browser-page-title")
                    .debug_selector(|| "browser-page-title".to_owned())
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .max_w(px(220.0))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_muted)
                    .child(page_title),
            )
            .when(self.state.agent_driving(), |this| {
                this.child(
                    div()
                        .id("browser-agent-driving")
                        .debug_selector(|| "browser-agent-driving".to_owned())
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded(theme.radii.control)
                        .bg(theme.text)
                        .text_color(theme.bg)
                        .text_size(theme.typography.footnote)
                        .child("Agent driving"),
                )
            })
            .child(browser_button(
                "browser-stop",
                "Stop",
                theme,
                self.state.is_loading(),
                move |cx| stop.update(cx, |surface, _| surface.on_stop()),
            ))
    }
}

fn browser_button(
    id: &'static str,
    label: &'static str,
    theme: Theme,
    enabled: bool,
    callback: impl Fn(&mut gpui::App) + 'static,
) -> impl IntoElement {
    browser_button_element(
        id,
        div().child(label).into_any_element(),
        theme,
        enabled,
        callback,
    )
}

fn browser_button_element(
    id: &'static str,
    content: impl IntoElement,
    theme: Theme,
    enabled: bool,
    callback: impl Fn(&mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .h(px(30.0))
        .min_w(px(30.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(theme.radii.control)
        .text_size(theme.typography.footnote)
        .text_color(if enabled {
            theme.text
        } else {
            theme.text_faint
        })
        .when(enabled, |this| {
            this.hover(|style| style.bg(theme.element_hover))
                .on_click(move |_, _, cx| callback(cx))
        })
        .child(content)
}

/// F-WIN-06: hands out the address bar's own focus handle -- the same
/// pattern `Chat`'s `Focusable` impl uses for its composer -- so a host
/// command (Focus Address Bar, `ctrl-l`) can call `browser.focus_handle(cx)`
/// through `Entity<BrowserSurface>`'s blanket `Focusable` impl and hand the
/// result straight to `window.focus`, with no bespoke accessor.
impl Focusable for BrowserSurface {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.address_field.read(cx).focus_handle(cx)
    }
}

impl Render for BrowserSurface {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // #255: the pump belongs here, not in `new`.
        self.ensure_pump_task(cx);
        let theme = *Theme::get(cx);
        // F-BRW-03: this is the only place a `Window` is available to ask
        // the focus system directly; pump_web_events (a background timer
        // task) reads this snapshot instead of calling is_focused itself.
        self.address_focused = self
            .address_field
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let entity = cx.entity();
        let permission = self.state.permission_prompt().cloned();
        let webview = NativeWebViewElement::new(
            self.webview.clone(),
            self.webview_scale_correction.clone(),
            self.webview_visible.clone(),
            self.overlay_obscured.clone(),
        );

        div()
            .id("browser-surface")
            .debug_selector(|| "browser-surface".to_owned())
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.surface)
            .text_color(theme.text)
            .child(self.render_toolbar(theme, entity.clone(), window, cx))
            .when_some(
                self.startup_failure
                    .as_ref()
                    .and_then(StartupFailure::banner_text)
                    .map(str::to_owned),
                |this, error| {
                    this.child(
                        div()
                            .id("browser-error")
                            .debug_selector(|| "browser-error".to_owned())
                            .w_full()
                            .px(px(12.0))
                            .py(px(8.0))
                            .bg(theme.danger)
                            .text_color(theme.bg)
                            .text_size(theme.typography.footnote)
                            .child(error),
                    )
                },
            )
            .when_some(self.state.error().map(str::to_owned), |this, error| {
                this.child(
                    div()
                        .id("browser-navigation-error")
                        .debug_selector(|| "browser-navigation-error".to_owned())
                        .w_full()
                        .px(px(12.0))
                        .py(px(8.0))
                        .bg(theme.danger)
                        .text_color(theme.bg)
                        .text_size(theme.typography.footnote)
                        .child(error),
                )
            })
            .when_some(permission, |this, prompt| {
                let allow = entity.clone();
                let deny = entity.clone();
                this.child(
                    div()
                        .id("browser-permission-doorhanger")
                        .debug_selector(|| "browser-permission-doorhanger".to_owned())
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(12.0))
                        .py(px(8.0))
                        .bg(theme.warning)
                        .text_color(theme.bg)
                        .text_size(theme.typography.footnote)
                        .child(format!(
                            "Allow agent browser access to {}?",
                            prompt.origin()
                        ))
                        .child(browser_button(
                            "browser-permission-allow",
                            "Allow",
                            theme,
                            true,
                            move |cx| {
                                allow.update(cx, |surface, _| {
                                    let _ = surface.allow_permission();
                                })
                            },
                        ))
                        .child(browser_button(
                            "browser-permission-deny",
                            "Deny",
                            theme,
                            true,
                            move |cx| {
                                deny.update(cx, |surface, _| {
                                    let _ = surface.deny_permission();
                                })
                            },
                        )),
                )
            })
            // The native child is the final layout region, not a sibling
            // overlapped by GPUI. This is the option-1 z-order contract.
            // #307: with the WebView2 runtime absent there is no child at
            // all, and the region explains itself instead of sitting empty.
            .child({
                let runtime_missing =
                    matches!(self.startup_failure, Some(StartupFailure::RuntimeMissing));
                div()
                    .id("browser-webview-region")
                    .relative()
                    .flex_1()
                    .when(runtime_missing, |region| {
                        region.child(self.render_runtime_missing(theme, entity.clone()))
                    })
                    .when(!runtime_missing, |region| region.child(webview))
            })
    }
}

impl Drop for BrowserSurface {
    fn drop(&mut self) {
        // The backstop for surfaces nobody closed explicitly — a restore that
        // replaces a pane's content, a worktree going away. It has to do the
        // full teardown and not just the unmap: dropping `self.webview` as an
        // ordinary field happens *after* this body returns, so an
        // `XDestroyWindow` issued then would land in the buffer with nothing
        // left to flush it.
        self.close_native();
    }
}

/// Converts a GPUI layout rect into wry's `Rect`, tagged `Logical` so wry's
/// own `WebView::set_bounds` (which re-derives logical pixels by calling
/// `bounds.to_logical(self.webview.scale_factor())` unconditionally,
/// before ever touching a GTK/X11 geometry call — see wry 0.56's
/// `webkitgtk/mod.rs`) treats the numbers as a literal pass-through
/// (`PixelUnit::Logical::to_logical` is a no-op regardless of the scale
/// factor argument) straight down to the raw X11 `resize`/`move_` calls,
/// which operate in physical screen pixels.
///
/// F-BRW-01 (corrected diagnosis): the earlier fix here treated `bounds`
/// (the `Bounds<Pixels>` GPUI's layout engine hands to `prepaint`) as
/// already being that physical-pixel target and passed it straight
/// through unscaled. Live instrumentation
/// (`[F-BRW-01 DEBUG] element bounds=... window.scale_factor=1.1666666`)
/// proved that wrong: GPUI lays this element out in its own internal
/// units, which on a desktop with a fractional `window.scale_factor()`
/// (1.1667 here — this box's Xft/GDK monitor scale) are already
/// `1/scale_factor` smaller than the physical pixels the raw X11 child
/// window needs. Confirmed two ways: (1) `webview.bounds()` (real
/// `XGetWindowAttributes` readback) always reported the *unscaled*
/// `bounds` value back byte-for-byte, meaning wry places the child
/// exactly where asked and applies no scaling of its own on this build;
/// (2) the D-P1 critic's own requested/observed pair (850x792 requested,
/// 728x679 observed) is exactly `850/1.1667` and `792/1.1667` — i.e. the
/// *live* `prepaint` bounds were already the post-divide value, not the
/// pre-divide design size. Multiplying back by `window.scale_factor()`
/// here recovers the true physical target before it ever reaches wry.
///
/// #143: that reasoning is entirely X11-shaped, and the function used to
/// apply it everywhere under a neutral name. macOS passes the value to
/// `setFrame`, whose frames are logical points, and Windows converts logical
/// to physical itself against a live `GetDpiForWindow` — so both want GPUI's
/// logical bounds untouched, and multiplying by the scale factor misplaced
/// the page by exactly that factor the moment the window met a display whose
/// scale was not 1. The conversion is now split in two, each **named for the
/// coordinate space it speaks**, because a neutral name holding a platform
/// rule explained only in a comment is what let this ship.
///
/// Both are compiled on every platform so both stay under test everywhere;
/// only the dispatcher below is `cfg`-gated.
/// Compiled on every platform so it stays under test everywhere, even where
/// only Linux calls it (#143).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn x11_physical_webview_rect(bounds: Bounds<Pixels>, scale_factor: f64) -> Rect {
    let scale_factor = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    Rect {
        position: LogicalPosition::new(
            f64::from(bounds.origin.x) * scale_factor,
            f64::from(bounds.origin.y) * scale_factor,
        )
        .into(),
        size: LogicalSize::new(
            (f64::from(bounds.size.width) * scale_factor).max(1.0),
            (f64::from(bounds.size.height) * scale_factor).max(1.0),
        )
        .into(),
    }
}

/// GPUI's layout bounds handed through unchanged, for the platforms whose
/// `set_bounds` already speaks logical units: macOS (`setFrame`, logical
/// points) and Windows (wry converts against `GetDpiForWindow` itself).
/// Identity at every scale factor — that is the whole contract (#143).
fn logical_webview_rect(bounds: Bounds<Pixels>) -> Rect {
    Rect {
        position: LogicalPosition::new(f64::from(bounds.origin.x), f64::from(bounds.origin.y))
            .into(),
        size: LogicalSize::new(
            f64::from(bounds.size.width).max(1.0),
            f64::from(bounds.size.height).max(1.0),
        )
        .into(),
    }
}

/// Picks the conversion this platform's `set_bounds` actually wants.
fn native_webview_rect(bounds: Bounds<Pixels>, scale_factor: f64) -> Rect {
    #[cfg(target_os = "linux")]
    {
        x11_physical_webview_rect(bounds, scale_factor)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = scale_factor;
        logical_webview_rect(bounds)
    }
}

/// Reads the raw numeric width/height out of a `wry::Rect`'s `Size`,
/// regardless of whether it is tagged `Physical` or `Logical`. Passing
/// `scale_factor: 1.0` makes both variants' `to_logical` a no-op (see
/// `dpi::PixelUnit::to_logical`), so this returns exactly the numbers
/// stored, unscaled — including the mislabeled-but-real physical pixels
/// `WebView::bounds()` reads back from `XGetWindowAttributes`.
/// Compiled on every platform so it stays under test everywhere, even where
/// only Linux calls it (#143).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn rect_size(rect: &Rect) -> (f64, f64) {
    let size = rect.size.to_logical::<f64>(1.0);
    (size.width, size.height)
}

/// Scales a `Rect`'s position and size uniformly by `factor`, reusing
/// [`rect_size`]'s tag-independent readout so this composes with rects
/// built by [`native_webview_rect`] or read back from `WebView::bounds()`.
/// Compiled on every platform so it stays under test everywhere, even where
/// only Linux calls it (#143).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn scale_rect(rect: &Rect, factor: f64) -> Rect {
    let position = rect.position.to_logical::<f64>(1.0);
    let (width, height) = rect_size(rect);
    Rect {
        position: LogicalPosition::new(position.x * factor, position.y * factor).into(),
        size: LogicalSize::new((width * factor).max(1.0), (height * factor).max(1.0)).into(),
    }
}

struct NativeWebViewElement {
    webview: SharedWebView,
    /// #143: read only on Linux, where the GTK/X11 self-calibration
    /// lives. Kept on every platform so the struct shape does not fork.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    scale_correction: SharedScaleCorrection,
    visible: SharedNativeVisibility,
    /// #376: when set, a GPUI menu/popover/modal is open above the page.
    obscured: SharedOverlayObscured,
}

impl NativeWebViewElement {
    fn new(
        webview: SharedWebView,
        scale_correction: SharedScaleCorrection,
        visible: SharedNativeVisibility,
        obscured: SharedOverlayObscured,
    ) -> Self {
        Self {
            webview,
            scale_correction,
            visible,
            obscured,
        }
    }
}

impl IntoElement for NativeWebViewElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NativeWebViewElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _: &mut App,
    ) -> Self::PrepaintState {
        // Reaching prepaint *is* the signal that this surface is on screen:
        // the element only enters the tree for the group's active tab, and the
        // Settings branch returns before building the pane tree at all. The
        // host is responsible for the matching hide — see
        // [`BrowserSurface::set_native_visible`].
        //
        // #376: an open GPUI overlay inverts that signal. The child HWND sits
        // above GPUI's own surface, so a menu that overlaps the page would
        // paint behind it and stop being clickable. The host marks covering
        // overlays through `set_overlay_obscured`; while marked, unmap here
        // instead of mapping and skip the move, so the hide survives the very
        // frame that requested it. Clearing the mark lets the next prepaint
        // map again with no explicit re-show.
        if self.obscured.get() {
            apply_native_visible(&self.webview, &self.visible, false);
            return;
        }
        apply_native_visible(&self.webview, &self.visible, true);

        if let Some(webview) = self.webview.borrow().as_ref() {
            // F-BRW-01: `bounds` arrives in GPUI's internal layout units,
            // which are `1/window.scale_factor()` smaller than physical
            // screen pixels whenever that factor is fractional (proven
            // live at 1.1666666 on this desktop). `native_webview_rect`
            // multiplies back up to the physical target before handing it
            // to wry — see its doc comment for the live evidence.
            let requested = native_webview_rect(bounds, _window.scale_factor() as f64);

            // Residual-error safety net for the GTK/X11 quirk only: a
            // one-shot self-calibration in case this box's
            // actual/requested pair still disagrees by a consistent ratio
            // (e.g. a compositor-level rounding quirk). It is a no-op
            // (factor converges to 1.0) whenever the direct correction
            // above already lands exactly.
            //
            // #143: Linux-only. Off Linux there is no GTK quirk to absorb,
            // and keeping it is precisely what let a factor-of-two error
            // reach a second display unnoticed: it latched 1.0 against the
            // first screen, then never measured again, so the one thing
            // that could have caught the misplacement stayed silent.
            #[cfg(target_os = "linux")]
            let corrected = match self.scale_correction.get() {
                Some(factor) => scale_rect(&requested, factor),
                None => requested,
            };
            #[cfg(not(target_os = "linux"))]
            let corrected = requested;
            let _ = webview.set_bounds(corrected);

            #[cfg(target_os = "linux")]
            if self.scale_correction.get().is_none()
                && let Ok(actual) = webview.bounds()
            {
                let (requested_w, requested_h) = rect_size(&requested);
                let (actual_w, actual_h) = rect_size(&actual);
                // Guard against the 1x1 startup stub and any transient
                // zero reading -- only calibrate once both dimensions
                // are large enough to measure a ratio meaningfully.
                if requested_w > 8.0 && requested_h > 8.0 && actual_w > 8.0 && actual_h > 8.0 {
                    let factor_w = requested_w / actual_w;
                    let factor_h = requested_h / actual_h;
                    // The observed bug (when present at all) is a
                    // uniform scale, not an independent per-axis one;
                    // average the two measurements to damp noise from
                    // integer pixel rounding on either side.
                    let factor = (factor_w + factor_h) / 2.0;
                    if (factor - 1.0).abs() > 0.01 {
                        self.scale_correction.set(Some(factor));
                        let _ = webview.set_bounds(scale_rect(&requested, factor));
                    } else {
                        self.scale_correction.set(Some(1.0));
                    }
                }
            }
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mirror has to start where wry leaves the window, and wry leaves it
    /// mapped: `build_as_child` ends in `XMapWindow`.
    ///
    /// This is the whole bug in one value. Start the mirror at `false` and the
    /// first `set_native_visible(false)` sees "already hidden" and returns
    /// without calling wry — against a window that is, in fact, on screen. The
    /// browser then keeps painting over Settings and Chat exactly as it did
    /// before any of this existed, and every test below still passes, because
    /// they only ever observe the mirror.
    ///
    /// So this test is not about the constant. It is the place the reason is
    /// written down.
    #[test]
    fn the_visibility_mirror_starts_where_wry_leaves_the_window() {
        assert!(
            initial_native_visibility().get(),
            "wry maps the child window on creation, so our mirror must start mapped"
        );
    }

    /// #369 follow-up: the forwarding rule is the whole fix for chords
    /// pressed while the page holds native focus, so it is pinned here —
    /// forwarding too little strands host chords in Edge (the `Ctrl+L` /
    /// `Ctrl+Shift+P` reports), forwarding too much breaks the page (reload,
    /// find, print, history, `AltGr` typing).
    #[test]
    fn host_chords_are_forwarded_from_the_page() {
        // Window commands, palette, settings, pane/tab chords.
        for (ctrl, shift, alt, vk) in [
            (true, false, false, 0x4C),  // Ctrl+L — Focus Address Bar
            (true, true, false, 0x50),   // Ctrl+Shift+P — palette
            (true, true, false, 0x4C),   // Ctrl+Shift+L — New Browser
            (true, true, false, 0x44),   // Ctrl+Shift+D — sidebar (#374 Windows)
            (true, true, false, 0x52),   // Ctrl+Shift+R — right panel (#374 Windows)
            (true, true, false, 0x48),   // Ctrl+Shift+H — restore launch (#374 Windows)
            (true, true, false, 0x42),   // Ctrl+Shift+B — secondary pane
            (true, true, false, 0x57),   // Ctrl+Shift+W — close tab
            (true, false, false, 0x54),  // Ctrl+T — new terminal
            (true, false, false, 0x4F),  // Ctrl+O — open file
            (true, false, false, 0x53),  // Ctrl+S — save file
            (true, false, false, 0x57),  // Ctrl+W — close tab
            (true, false, false, 0x4B),  // Ctrl+K — palette fallback
            (true, false, false, 0xBC),  // Ctrl+, — settings
            (false, false, false, 0x1B), // Escape — close settings/palette
            (true, false, false, 0x09),  // Ctrl+Tab — cycle tab
            (true, true, false, 0x09),   // Ctrl+Shift+Tab — cycle back
            (true, false, false, 0x31),  // Ctrl+1 — jump to tab
            (true, false, false, 0x39),  // Ctrl+9 — jump to tab
            (true, false, true, 0x25),   // Ctrl+Alt+Left — focus pane
            (true, false, true, 0x28),   // Ctrl+Alt+Down — focus pane
            (true, false, true, 0x57),   // Ctrl+Alt+W — close pane
            (true, true, true, 0x27),    // Ctrl+Alt+Shift+Right — split pane
        ] {
            assert!(
                should_forward_accelerator_to_host(ctrl, shift, alt, vk),
                "host chord must be forwarded: ctrl={ctrl} shift={shift} alt={alt} vk={vk:#X}"
            );
        }
    }

    #[test]
    fn page_keys_stay_in_the_page() {
        for (ctrl, shift, alt, vk) in [
            (false, false, false, 0x41), // typing
            (true, false, false, 0x41),  // Ctrl+A — page select-all
            (true, false, false, 0x43),  // Ctrl+C — copy
            (true, false, false, 0x56),  // Ctrl+V — paste
            (true, false, false, 0x58),  // Ctrl+X — cut
            (true, false, false, 0x5A),  // Ctrl+Z — undo
            (false, false, false, 0x25), // arrows — page scroll/caret
            (false, false, false, 0x74), // F5 — reload
            (true, false, false, 0x52),  // Ctrl+R — reload
            (true, false, false, 0x50),  // Ctrl+P — print
            (true, false, false, 0x46),  // Ctrl+F — find
            (true, false, false, 0x4E),  // Ctrl+N — no host binding
            (true, false, false, 0x44),  // Ctrl+D — no host binding
            (true, true, false, 0x53),   // Ctrl+Shift+S — OS hotkey, never arrives (#374)
            (true, true, false, 0x49),   // Ctrl+Shift+I — OS hotkey, never arrives (#374)
            (true, true, false, 0x4F),   // Ctrl+Shift+O — OS hotkey, never arrives (#374)
            (false, false, true, 0x25),  // Alt+Left — history back
            (true, false, true, 0x51),   // AltGr+Q typing — must not steal
            (true, false, false, 0x1B),  // Ctrl+Esc — Start menu, never swallow
            (false, true, false, 0x1B),  // Shift+Esc — never swallow
            (false, false, false, 0x7B), // F12 — devtools
        ] {
            assert!(
                !should_forward_accelerator_to_host(ctrl, shift, alt, vk),
                "page key must not be forwarded: ctrl={ctrl} shift={shift} alt={alt} vk={vk:#X}"
            );
        }
    }

    #[test]
    fn script_result_wait_returns_the_callback_value() {
        let (sender, receiver) = std::sync::mpsc::channel();
        sender.send("callback-result".to_string()).unwrap();

        assert_eq!(
            wait_for_script_result(receiver, Duration::from_millis(10)),
            Ok("callback-result".to_string())
        );
    }

    #[test]
    fn hiding_and_showing_only_call_through_on_a_real_change() {
        // No webview: this exercises the flag arithmetic, not the X11 window.
        // The mirror is the part that decides whether wry is called at all, and
        // it is the part that can be wrong without anything failing to compile.
        let webview: SharedWebView = Rc::new(RefCell::new(None));
        let flag = initial_native_visibility();

        apply_native_visible(&webview, &flag, true);
        assert!(flag.get(), "showing an already-shown child leaves it shown");

        apply_native_visible(&webview, &flag, false);
        assert!(!flag.get(), "hiding a shown child hides it");

        apply_native_visible(&webview, &flag, false);
        assert!(!flag.get(), "hiding twice is idempotent");

        apply_native_visible(&webview, &flag, true);
        assert!(flag.get(), "a hidden child can be shown again");
    }

    /// #376: the overlay mark a fresh surface carries. No overlay is open for
    /// a surface nobody has told about one, so prepaint must map on its first
    /// frame; starting obscured would leave every new browser blank until
    /// some unrelated menu opened and closed.
    ///
    /// The cell part (not the surface) is what is pinned here: the surface
    /// needs a window to build, the rule does not.
    #[test]
    fn overlay_obscured_starts_clear() {
        let obscured: SharedOverlayObscured = Rc::new(Cell::new(false));
        assert!(
            !obscured.get(),
            "a fresh surface is not covered by any overlay"
        );
    }

    /// #376: the mark the host syncs every frame. Setting it twice is not an
    /// error (render syncs unconditionally), and clearing it is what lets the
    /// next prepaint map the child again with no explicit re-show call.
    #[test]
    fn overlay_obscured_mark_round_trips() {
        let obscured: SharedOverlayObscured = Rc::new(Cell::new(false));
        obscured.set(true);
        assert!(obscured.get(), "marking a covered surface stays marked");
        obscured.set(true);
        assert!(obscured.get(), "marking twice is idempotent");
        obscured.set(false);
        assert!(!obscured.get(), "closing the overlay clears the mark");
    }

    /// #376: the mark round-trips through the surface's own accessors. The
    /// host syncs it from `read` every frame while `prepaint` reads the
    /// clone the element was built with — if those were two cells, the mark
    /// would never reach the paint path and the menu would stay behind the
    /// page exactly as before. No window is needed: the methods only touch
    /// the shared cell.
    #[gpui::test]
    async fn overlay_mark_round_trips_through_the_surface(cx: &mut gpui::TestAppContext) {
        let surface = cx.update(|cx| {
            Theme::init(cx);
            bezel::ui::input::init(cx);
            cx.new(|cx| {
                let state = BrowserState::new("https://example.com").expect("valid URL");
                let address_field = cx.new(|cx| {
                    let mut field = TextField::new(cx);
                    field.set_content(state.address(), cx);
                    field
                });
                BrowserSurface {
                    state,
                    address_field,
                    address_focused: false,
                    webview: Rc::new(RefCell::new(None)),
                    _web_context: None,
                    webview_scale_correction: Rc::new(Cell::new(None)),
                    webview_visible: initial_native_visibility(),
                    overlay_obscured: Rc::new(Cell::new(false)),
                    web_events: Rc::new(RefCell::new(Vec::new())),
                    events: Vec::new(),
                    pump_task: None,
                    startup_failure: None,
                }
            })
        });
        surface.update(cx, |surface, _| {
            assert!(
                !surface.overlay_obscured(),
                "a fresh surface carries no overlay mark"
            );
            surface.set_overlay_obscured(true);
            assert!(
                surface.overlay_obscured(),
                "the host mark must be readable back"
            );
            surface.set_overlay_obscured(true);
            assert!(
                surface.overlay_obscured(),
                "syncing every frame must be idempotent"
            );
            surface.set_overlay_obscured(false);
            assert!(
                !surface.overlay_obscured(),
                "closing the overlay must clear the mark so prepaint maps again"
            );
        });
    }

    /// The two properties `close_tab` leans on. Not the flush — that one needs
    /// a real X server and lives in `Scripts/Tests/`, because the whole point
    /// of the bug is that it is invisible from inside the process.
    #[test]
    fn closing_takes_the_child_out_of_the_shared_cell() {
        let webview: SharedWebView = Rc::new(RefCell::new(None));
        // The clone an element would be holding, made before the close.
        let element_side = Rc::clone(&webview);
        let flag = initial_native_visibility();

        close_native_window(&webview, &flag);
        assert!(!flag.get(), "closing hides the child");
        assert!(
            element_side.borrow().is_none(),
            "closing empties the cell every other holder reads through, so a \
             late `set_bounds` or `set_visible` finds nothing instead of a \
             destroyed window"
        );

        close_native_window(&webview, &flag);
        assert!(
            !flag.get(),
            "closing twice is harmless — Drop runs after an explicit close"
        );
    }

    #[test]
    fn address_submission_normalizes_http_and_reports_invalid_input() {
        let mut browser = BrowserState::new("https://example.com").expect("valid initial URL");

        assert_eq!(
            browser
                .submit_address("example.org/path")
                .expect("valid address"),
            "https://example.org/path"
        );
        assert_eq!(browser.address(), "https://example.org/path");
        assert!(browser.is_loading());

        let error = browser
            .submit_address("not a URL")
            .expect_err("spaces are not a valid browser address");
        assert!(matches!(error, BrowserError::InvalidAddress(_)));
        assert!(browser.error().is_some());
    }

    #[test]
    fn navigation_history_controls_follow_real_page_events() {
        let mut browser = BrowserState::new("https://one.example").expect("valid initial URL");
        browser.did_finish_navigation("https://one.example", "One");
        browser
            .submit_address("https://two.example")
            .expect("valid address");
        browser.did_finish_navigation("https://two.example", "Two");

        assert!(browser.can_go_back());
        assert!(!browser.can_go_forward());
        assert_eq!(browser.go_back(), Some("https://one.example".into()));
        assert!(!browser.can_go_back());
        assert!(browser.can_go_forward());
        assert_eq!(browser.go_forward(), Some("https://two.example".into()));
        assert_eq!(browser.reload(), Some("https://two.example".into()));

        browser.did_start_navigation("https://slow.example");
        assert!(browser.is_loading());
        assert!(browser.stop_loading());
        assert!(!browser.is_loading());
    }

    #[test]
    fn redirect_chain_is_one_history_entry() {
        let mut browser = BrowserState::new("https://example.com").expect("valid initial URL");
        browser.did_finish_navigation("https://example.com/", "Example Domain");
        browser.did_start_navigation("https://iana.org/domains/example");
        browser.did_start_navigation("https://www.iana.org/domains/example");
        browser.did_start_navigation("http://www.iana.org/help/example-domains");
        browser.did_finish_navigation(
            "https://www.iana.org/help/example-domains",
            "Example Domains",
        );

        assert_eq!(
            browser.go_back(),
            Some("https://example.com/".to_owned()),
            "a redirect chain must not create intermediate Back entries"
        );
    }

    #[test]
    fn permission_doorhanger_resolves_and_persists_by_origin() {
        let mut browser = BrowserState::new("https://example.com").expect("valid initial URL");
        browser.request_permission("https://agent.example");
        assert_eq!(
            browser.permission_prompt().map(|prompt| prompt.origin()),
            Some("https://agent.example")
        );

        let origin = browser.allow_permission().expect("pending prompt");
        assert_eq!(origin, "https://agent.example");
        assert!(browser.is_origin_allowed("https://agent.example"));
        assert!(browser.permission_prompt().is_none());

        browser.request_permission("https://denied.example");
        assert_eq!(
            browser.deny_permission(),
            Some("https://denied.example".into())
        );
        assert!(!browser.is_origin_allowed("https://denied.example"));
    }

    #[test]
    fn browser_link_routing_keeps_http_internal_and_honors_external_bypass() {
        let mut browser = BrowserState::new("https://example.com").expect("valid initial URL");

        assert_eq!(
            browser.open_link("http://docs.example", BrowserLinkTarget::Internal),
            Ok(BrowserEvent::Navigate("http://docs.example".into()))
        );
        assert_eq!(
            browser.open_link("https://docs.example", BrowserLinkTarget::External),
            Ok(BrowserEvent::OpenExternal("https://docs.example".into()))
        );
    }

    #[test]
    fn webview_bounds_with_unit_scale_pass_through_unchanged() {
        // At scale_factor 1.0 (the common case) native_webview_rect must
        // still be a plain unit conversion, not a coordinate change.
        let bounds = Bounds::new(
            gpui::point(gpui::px(386.0), gpui::px(133.0)),
            gpui::size(gpui::px(850.0), gpui::px(792.0)),
        );

        let rect = native_webview_rect(bounds, 1.0);

        assert_eq!(
            rect.position,
            wry::dpi::LogicalPosition::new(386.0, 133.0).into()
        );
        assert_eq!(rect.size, wry::dpi::LogicalSize::new(850.0, 792.0).into());
    }

    #[test]
    fn x11_webview_bounds_recover_physical_target_from_live_fractional_scale_factor() {
        // F-BRW-01 (corrected diagnosis): live instrumentation on this box
        // showed `prepaint` receiving `bounds` of ~330.857,114.0 /
        // 728.571x678.857 with `window.scale_factor() == 1.1666666` for a
        // panel whose true physical target was x=386,y=133, 850x792 (the
        // exact pair the D-P1 critic separately measured). GPUI's layout
        // bounds are `1/scale_factor` smaller than physical screen pixels
        // on a fractional-scale desktop; native_webview_rect must multiply
        // back up by that same scale_factor to recover the physical rect
        // wry needs to hand the raw X11 child window.
        let laid_out_bounds = Bounds::new(
            gpui::point(gpui::px(330.857_15), gpui::px(114.000_01)),
            gpui::size(gpui::px(728.571_5), gpui::px(678.857_2)),
        );

        let rect = x11_physical_webview_rect(laid_out_bounds, 1.166_666_6);

        let (width, height) = rect_size(&rect);
        let position = rect.position.to_logical::<f64>(1.0);
        assert!((position.x - 386.0).abs() < 1.0, "x = {}", position.x);
        assert!((position.y - 133.0).abs() < 1.0, "y = {}", position.y);
        assert!((width - 850.0).abs() < 1.0, "width = {width}");
        assert!((height - 792.0).abs() < 1.0, "height = {height}");
    }

    /// #143: the mirror of the test above, and the one that would have
    /// caught the defect. macOS hands the value to `setFrame`, whose frames
    /// are logical points, and Windows converts logical to physical itself
    /// against a live `GetDpiForWindow` — so on both the conversion must be
    /// **identity at every scale factor**. Applying X11's multiplication
    /// there misplaced and mis-sized the page by exactly the display's scale
    /// the moment the window met a screen whose factor was not 1.
    #[test]
    fn logical_webview_bounds_are_identity_at_any_scale_factor() {
        let laid_out_bounds = Bounds::new(
            gpui::point(gpui::px(300.0), gpui::px(117.0)),
            gpui::size(gpui::px(880.0), gpui::px(640.0)),
        );

        let rect = logical_webview_rect(laid_out_bounds);
        let (width, height) = rect_size(&rect);
        let position = rect.position.to_logical::<f64>(1.0);

        assert!(
            (position.x - 300.0).abs() < f64::EPSILON,
            "x = {}",
            position.x
        );
        assert!(
            (position.y - 117.0).abs() < f64::EPSILON,
            "y = {}",
            position.y
        );
        assert!((width - 880.0).abs() < f64::EPSILON, "width = {width}");
        assert!((height - 640.0).abs() < f64::EPSILON, "height = {height}");

        // The 2x display from the report: the same bounds must still come
        // back unchanged, because nothing here consults a scale factor at
        // all. Before the split this drew the page at double its offset,
        // overflowing the pane and covering the right-hand panel.
        let same = logical_webview_rect(laid_out_bounds);
        let (same_w, same_h) = rect_size(&same);
        assert!((same_w - width).abs() < f64::EPSILON);
        assert!((same_h - height).abs() < f64::EPSILON);
    }

    /// The dispatcher must pick the space this platform's `set_bounds`
    /// speaks: physical on Linux, logical everywhere else (#143).
    #[test]
    fn native_webview_rect_speaks_this_platforms_coordinate_space() {
        let laid_out_bounds = Bounds::new(
            gpui::point(gpui::px(300.0), gpui::px(117.0)),
            gpui::size(gpui::px(880.0), gpui::px(640.0)),
        );
        let dispatched = native_webview_rect(laid_out_bounds, 2.0);
        let (width, _) = rect_size(&dispatched);
        let position = dispatched.position.to_logical::<f64>(1.0);

        #[cfg(target_os = "linux")]
        {
            assert!((position.x - 600.0).abs() < 1.0, "x = {}", position.x);
            assert!((width - 1760.0).abs() < 1.0, "width = {width}");
        }
        #[cfg(not(target_os = "linux"))]
        {
            assert!((position.x - 300.0).abs() < 1.0, "x = {}", position.x);
            assert!((width - 880.0).abs() < 1.0, "width = {width}");
        }
    }

    #[test]
    fn webview_bounds_clamp_to_a_minimum_visible_size() {
        let bounds = Bounds::new(
            gpui::point(gpui::px(0.0), gpui::px(0.0)),
            gpui::size(gpui::px(0.0), gpui::px(0.0)),
        );

        let rect = native_webview_rect(bounds, 1.0);

        assert_eq!(rect.size, wry::dpi::LogicalSize::new(1.0, 1.0).into());
    }

    #[test]
    fn webview_bounds_reject_non_finite_scale_factor() {
        let bounds = Bounds::new(
            gpui::point(gpui::px(10.0), gpui::px(10.0)),
            gpui::size(gpui::px(100.0), gpui::px(100.0)),
        );

        let rect = native_webview_rect(bounds, f64::NAN);

        // Falls back to an unscaled (factor 1.0) rect rather than
        // propagating NaN into the geometry wry hands X11.
        assert_eq!(rect.size, wry::dpi::LogicalSize::new(100.0, 100.0).into());
    }

    #[test]
    fn scale_correction_recovers_the_exact_shrink_d_p1_measured_live() {
        // F-BRW-01: the D-P1 critic requested a webview at x=386,y=133,
        // 850x792 and read back real X server geometry (`webview.bounds()`)
        // spanning only x=331..1059 -- 728px wide, at position 331, not
        // 386. That is GTK/GDK silently applying ~1/1.1667 below anything
        // `native_webview_rect` touches. `scale_rect` must recover the
        // exact corrective factor from that observed pair, in both
        // dimensions, without hardcoding the ratio anywhere.
        let requested = native_webview_rect(
            Bounds::new(
                gpui::point(gpui::px(386.0), gpui::px(133.0)),
                gpui::size(gpui::px(850.0), gpui::px(792.0)),
            ),
            1.0,
        );
        let observed_shrink = 1.0 / 1.1667_f64;
        let actual = scale_rect(&requested, observed_shrink);

        let (actual_w, actual_h) = rect_size(&actual);
        // Matches the critic's live pixel scan (728px content span) to
        // within a pixel of rounding.
        assert!((actual_w - 728.0).abs() < 1.0, "actual_w = {actual_w}");

        let (requested_w, requested_h) = rect_size(&requested);
        let factor_w = requested_w / actual_w;
        let factor_h = requested_h / actual_h;
        let recovered_factor = (factor_w + factor_h) / 2.0;

        // The correction loop: re-request at `requested * recovered_factor`
        // and let GTK's own (unknown, unmodeled) shrink apply again, the
        // same way it did on the frame we calibrated from. The result must
        // land back at what was originally requested -- the actual
        // real-world invariant `prepaint`'s calibration branch relies on.
        let corrected_request = scale_rect(&requested, recovered_factor);
        let corrected_real_geometry = scale_rect(&corrected_request, observed_shrink);
        let (final_w, final_h) = rect_size(&corrected_real_geometry);
        assert!((final_w - requested_w).abs() < 1.0, "final_w = {final_w}");
        assert!((final_h - requested_h).abs() < 1.0, "final_h = {final_h}");
    }

    #[gpui::test]
    async fn address_field_keeps_content_in_the_bezel_text_field(cx: &mut gpui::TestAppContext) {
        let field = cx.update(|cx| {
            Theme::init(cx);
            bezel::ui::input::init(cx);
            cx.new(|cx| {
                let mut field = TextField::new(cx);
                field.set_content("https://www.iana.org", cx);
                field
            })
        });

        cx.update(|cx| {
            assert_eq!(field.read(cx).content(), "https://www.iana.org");
        });
    }

    /// #255: the pump is armed once, only for a live webview.
    ///
    /// The "no webview" arm is what keeps a closed surface closed -- `render`
    /// runs again after `close_native`, and arming there would resurrect a
    /// 16 ms timer for a webview that has already been dropped.
    #[test]
    fn the_pump_is_armed_once_and_never_for_a_dead_webview() {
        assert!(
            should_arm_pump(false, true),
            "a live webview with no task yet must arm the pump"
        );
        assert!(
            !should_arm_pump(true, true),
            "render runs every frame; a second task must never be armed"
        );
        assert!(
            !should_arm_pump(false, false),
            "a closed surface has no webview and must stay closed"
        );
        assert!(!should_arm_pump(true, false));
    }

    #[gpui::test]
    async fn address_field_programmatic_updates_leave_the_caret_at_the_end(
        cx: &mut gpui::TestAppContext,
    ) {
        let field = cx.update(|cx| {
            Theme::init(cx);
            bezel::ui::input::init(cx);
            cx.new(TextField::new)
        });
        cx.update(|cx| {
            field.update(cx, |field, cx| {
                field.set_content("https://www.example.com", cx)
            });
            let field = field.read(cx);
            assert_eq!(field.cursor(), field.content().len());
        });
    }

    #[gpui::test]
    async fn address_bar_gets_priority_over_the_page_title_in_a_narrow_pane(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            Theme::init(cx);
            bezel::ui::input::init(cx);
        });
        let window = cx.open_window(gpui::size(px(390.0), px(200.0)), |_, cx| {
            let mut state = BrowserState::new("https://example.com").expect("valid URL");
            state.did_finish_navigation("https://example.com", "Example Domain");
            let address_field = cx.new(|cx| {
                let mut field = TextField::new(cx);
                field.set_content(state.address(), cx);
                field
            });
            BrowserSurface {
                state,
                address_field,
                address_focused: false,
                webview: Rc::new(RefCell::new(None)),
                _web_context: None,
                webview_scale_correction: Rc::new(Cell::new(None)),
                webview_visible: initial_native_visibility(),
                overlay_obscured: Rc::new(Cell::new(false)),
                web_events: Rc::new(RefCell::new(Vec::new())),
                events: Vec::new(),
                pump_task: None,
                startup_failure: None,
            }
        });
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let address = cx
            .debug_bounds("browser-address-field")
            .expect("the address field is drawn");
        let title = cx
            .debug_bounds("browser-page-title")
            .expect("the page title is drawn");
        assert!(
            address.size.width >= title.size.width,
            "the address bar must not be narrower than the page title: \
             address={address:?} title={title:?}"
        );
    }

    /// #307: on a Windows machine without the WebView2 Runtime the browser
    /// surface explains itself instead of failing quietly. The message is
    /// the whole feature: it must name the runtime, own that Sirio could
    /// not install it, and point at the one retry action. (The button
    /// itself is GPUI chrome needing a live window; the words are the part
    /// that can be pinned here.)
    #[test]
    fn runtime_missing_message_names_the_runtime_and_the_failed_install() {
        let message = StartupFailure::RuntimeMissing.message();
        assert!(
            message.contains("Microsoft Edge WebView2 Runtime"),
            "the message must name the runtime: {message}"
        );
        assert!(
            message.contains("could not install"),
            "the message must own the failed install: {message}"
        );
        assert!(
            message.to_lowercase().contains("retry"),
            "the message must point at the retry action: {message}"
        );
    }

    /// #307: a runtime-missing surface shows the explanation *in place of
    /// the content*; the generic red banner above it would be a second,
    /// redundant message about the same failure.
    #[test]
    fn runtime_missing_shows_the_explanation_not_the_error_banner() {
        assert!(StartupFailure::RuntimeMissing.banner_text().is_none());
    }

    #[test]
    fn an_engine_failure_keeps_the_generic_error_banner() {
        let failure = StartupFailure::Failed("WebView2 child failed: boom".to_owned());
        assert_eq!(failure.banner_text(), Some("WebView2 child failed: boom"));
        assert_eq!(failure.message(), "WebView2 child failed: boom");
    }

    /// AC "with the runtime present, nothing changes", for the platforms
    /// that do not probe: only Windows can classify the runtime as missing,
    /// and everywhere else the surface behaves exactly as before.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn non_windows_platforms_never_classify_the_runtime_as_missing() {
        assert!(!webview_runtime_missing());
    }

    /// #368: a pending surface explains a missing runtime immediately, so
    /// the deferred build is never even scheduled; otherwise it preserves
    /// the invalid-URL fallback error for the later build to keep.
    #[test]
    fn pending_failure_prefers_runtime_missing_over_startup_error() {
        assert_eq!(
            pending_startup_failure(true, Some("bad url".to_owned())),
            Some(StartupFailure::RuntimeMissing)
        );
        assert_eq!(
            pending_startup_failure(false, Some("bad url".to_owned())),
            Some(StartupFailure::Failed("bad url".to_owned()))
        );
        assert_eq!(pending_startup_failure(false, None), None);
    }

    /// #368: a live deferred webview keeps what the pending surface already
    /// carried (the invalid-URL fallback); only a failed build replaces the
    /// explanation. Clearing on success would turn a known bad address into
    /// a silent ok.
    #[test]
    fn deferred_install_keeps_pending_error_on_success_replaces_on_failure() {
        let pending = Some(StartupFailure::Failed("bad url".to_owned()));
        assert_eq!(
            deferred_install_failure(pending.clone(), None, true),
            pending,
            "success must not clear the pending fallback error"
        );
        let engine_error = Some(StartupFailure::Failed(
            "WebView2 child failed: boom".to_owned(),
        ));
        assert_eq!(
            deferred_install_failure(pending, engine_error.clone(), false),
            engine_error,
            "failure must replace the pending explanation"
        );
        assert_eq!(deferred_install_failure(None, None, true), None);
    }

    /// #368: the deferred parent is just the captured HWND. Zero is never a
    /// window, so it fails here -- cheaply, without touching COM -- instead
    /// of failing inside the pumped build.
    #[cfg(target_os = "windows")]
    #[test]
    fn deferred_parent_rejects_a_null_hwnd() {
        assert!(DeferredParent::from_hwnd(0).is_err());
        assert_eq!(
            DeferredParent::from_hwnd(12345).expect("non-zero HWND"),
            DeferredParent { hwnd: 12345 }
        );
    }
}
