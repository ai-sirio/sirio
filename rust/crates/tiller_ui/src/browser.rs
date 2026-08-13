//! P72 throwaway browser-composition spike.
//!
//! This is intentionally not part of the production module graph yet. The
//! `browser_spike` example includes it directly so the experiment can answer
//! whether a WebKitGTK child window can coexist with GPUI's X11 surface.

use std::{cell::RefCell, collections::BTreeSet, ffi::c_ulong, rc::Rc, time::Duration};

use gpui::{
    App, Bounds, Context, Element, ElementId, FocusHandle, GlobalElementId, InspectorElementId,
    IntoElement, KeyDownEvent, LayoutId, Pixels, Render, Style, Task, Window, div, prelude::*, px,
    relative,
};
use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, WindowHandle, XlibWindowHandle,
};
use tiller_theme::Theme;
use wry::{
    NewWindowFeatures, NewWindowResponse, PageLoadEvent, Rect, WebView, WebViewBuilder,
    dpi::LogicalPosition, dpi::LogicalSize,
};

/// GPUI's Linux backend exposes its X11 surface as XCB, while wry's
/// WebKitGTK backend currently accepts only an Xlib window ID. The ID is
/// shared by both X11 APIs, so this adapter tests that narrow seam without
/// pretending the two handle types are interchangeable in general.
#[derive(Clone, Copy)]
struct XlibParent {
    window: c_ulong,
}

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

/// Native GPUI chrome with a live WebKitGTK child window on the right.
pub struct BrowserSpike {
    webview: Option<WebView>,
    pump_task: Option<Task<()>>,
    startup_error: Option<String>,
}

impl BrowserSpike {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (webview, startup_error) = match gtk::init() {
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
        };

        let pump_task = webview.as_ref().map(|_| {
            cx.spawn(async move |this, cx| {
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
            .bg(theme.chat_surface)
            .text_color(theme.title)
            .child(
                div()
                    .h(px(52.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .px(px(20.0))
                    .bg(theme.canvas)
                    .border_b_1()
                    .border_color(theme.hairline)
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
                            .bg(theme.canvas)
                            .border_r_1()
                            .border_color(theme.hairline)
                            .child(
                                div()
                                    .text_size(theme.typography.title)
                                    .text_color(theme.title)
                                    .child("Native GPUI sidebar"),
                            )
                            .child(
                                div()
                                    .mt(px(18.0))
                                    .text_size(theme.typography.base_size)
                                    .text_color(theme.subtitle)
                                    .child(
                                        "The page on the right is a real WebKitGTK child window.",
                                    ),
                            )
                            .child(
                                div()
                                    .mt(px(18.0))
                                    .text_size(theme.typography.footnote)
                                    .text_color(theme.meta)
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
                            .bg(theme.accent)
                            .text_color(theme.canvas)
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
    /// Leave Tiller and let the host open the link in the system browser.
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

/// Browser failures that can be shown in Tiller's native chrome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserError {
    /// The address is not an HTTP(S) URL that the embedded browser accepts.
    InvalidAddress(String),
    /// The WebKit child rejected an otherwise valid navigation request.
    Navigation(String),
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
            self.begin_navigation(url);
        }
    }

    /// Records a successful WebKit navigation and updates the title/content
    /// chrome without painting anything over the child window.
    pub fn did_finish_navigation(&mut self, url: &str, title: &str) {
        if let Ok(url) = normalize_address(url) {
            self.record_navigation(url.clone());
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

fn build_production_webview<W: HasWindowHandle>(
    parent: &W,
    initial_url: &str,
    events: &SharedWebEvents,
) -> Result<WebView, wry::Error> {
    let navigation_events = events.clone();
    let window_events = events.clone();
    let page_events = events.clone();
    let title_events = events.clone();
    WebViewBuilder::new()
        .with_url(initial_url)
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

/// A production browser surface: all browser chrome is GPUI, while page
/// pixels live in the proven native X11 child window below it.
pub struct BrowserSurface {
    state: BrowserState,
    address_draft: String,
    address_focus: FocusHandle,
    webview: SharedWebView,
    web_events: SharedWebEvents,
    events: Vec<BrowserEvent>,
    pump_task: Option<Task<()>>,
    startup_error: Option<String>,
}

impl BrowserSurface {
    /// Creates a browser surface attached to the current GPUI X11 window.
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
        let address_draft = state.address().to_owned();
        let web_events = Rc::new(RefCell::new(Vec::new()));
        let (webview, startup_error) = match gtk::init() {
            Ok(()) => match build_production_webview(window, state.address(), &web_events) {
                Ok(webview) => (Some(webview), startup_error),
                Err(direct_error) => match XlibParent::from_gpui(window) {
                    Ok(parent) => {
                        match build_production_webview(&parent, state.address(), &web_events) {
                            Ok(webview) => (Some(webview), startup_error),
                            Err(bridge_error) => (
                                None,
                                Some(format!(
                                    "Direct XCB build failed: {direct_error}; XCB→Xlib build failed: {bridge_error}"
                                )),
                            ),
                        }
                    }
                    Err(bridge_error) => (
                        None,
                        Some(format!(
                            "Direct XCB build failed: {direct_error}; XCB→Xlib adapter failed: {bridge_error}"
                        )),
                    ),
                },
            },
            Err(error) => (None, Some(format!("GTK init failed: {error}"))),
        };
        let webview = Rc::new(RefCell::new(webview));
        let pump_task = webview.borrow().as_ref().map(|_| {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    if this
                        .update(cx, |surface, cx| {
                            surface.pump_web_events();
                            while gtk::events_pending() {
                                gtk::main_iteration_do(false);
                            }
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
        });

        Self {
            state,
            address_draft,
            address_focus: cx.focus_handle(),
            webview,
            web_events,
            events: Vec::new(),
            pump_task,
            startup_error,
        }
    }

    /// Borrows the testable state for host synchronization.
    pub fn state(&self) -> &BrowserState {
        &self.state
    }

    /// Returns and clears browser events produced by WebKit or an explicit
    /// host action.
    pub fn take_events(&mut self) -> Vec<BrowserEvent> {
        std::mem::take(&mut self.events)
    }

    /// Starts an address-field navigation in WebKit.
    pub fn submit_address(&mut self, input: &str) -> Result<(), BrowserError> {
        let address = self.state.submit_address(input)?;
        self.address_draft = address.clone();
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
            self.address_draft = address.clone();
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

    /// Updates the F-BRW-05 activity marker.
    pub fn set_agent_driving(&mut self, driving: bool) {
        self.state.set_agent_driving(driving);
    }

    fn load_url(&mut self, address: &str) -> Result<(), BrowserError> {
        if let Some(webview) = self.webview.borrow().as_ref() {
            webview
                .load_url(address)
                .map_err(|error| BrowserError::Navigation(error.to_string()))?;
        }
        Ok(())
    }

    fn navigate_history(&mut self, address: Option<String>) {
        let Some(address) = address else {
            return;
        };
        let result = self.load_url(&address);
        if let Err(error) = result {
            self.state.did_fail_navigation(error.to_string());
        } else {
            self.address_draft = address;
        }
    }

    fn on_back(&mut self) {
        let address = self.state.go_back();
        self.navigate_history(address);
    }

    fn on_forward(&mut self) {
        let address = self.state.go_forward();
        self.navigate_history(address);
    }

    fn on_reload(&mut self) {
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
        self.address_draft = address;
    }

    fn on_stop(&mut self) {
        if self.state.stop_loading()
            && let Some(webview) = self.webview.borrow().as_ref()
        {
            let _ = webview.evaluate_script("window.stop();");
        }
    }

    fn on_address_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "enter" => {
                let draft = self.address_draft.clone();
                let _ = self.submit_address(&draft);
                cx.notify();
            }
            "backspace" => {
                self.address_draft.pop();
                cx.notify();
            }
            _ => {
                if let Some(character) = event.keystroke.key_char.as_deref()
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && character != "\n"
                {
                    self.address_draft.push_str(character);
                    cx.notify();
                }
            }
        }
    }

    fn pump_web_events(&mut self) {
        let pending = std::mem::take(&mut *self.web_events.borrow_mut());
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
                    self.address_draft = self.state.address().to_owned();
                    self.events.push(BrowserEvent::PageFinished(url));
                }
                WebEvent::TitleChanged(title) => {
                    self.state.page_title = title.clone();
                    self.events.push(BrowserEvent::TitleChanged(title));
                }
            }
        }
    }

    fn render_toolbar(&self, theme: Theme, entity: gpui::Entity<Self>) -> impl IntoElement {
        let address_focus = self.address_focus.clone();
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
        div()
            .id("browser-toolbar")
            .debug_selector(|| "browser-toolbar".to_owned())
            .h(px(48.0))
            .w_full()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(12.0))
            .bg(theme.canvas)
            .border_b_1()
            .border_color(theme.hairline)
            .child(browser_button(
                "browser-back",
                "‹",
                theme,
                self.state.can_go_back(),
                move |cx| back.update(cx, |surface, _| surface.on_back()),
            ))
            .child(browser_button(
                "browser-forward",
                "›",
                theme,
                self.state.can_go_forward(),
                move |cx| forward.update(cx, |surface, _| surface.on_forward()),
            ))
            .child(browser_button(
                "browser-reload",
                if self.state.is_loading() { "×" } else { "↻" },
                theme,
                true,
                move |cx| {
                    reload.update(cx, |surface, _| {
                        if surface.state.is_loading() {
                            surface.on_stop();
                        } else {
                            surface.on_reload();
                        }
                    })
                },
            ))
            .child(
                div()
                    .id("browser-address-field")
                    .debug_selector(|| "browser-address-field".to_owned())
                    .track_focus(&address_focus)
                    .focusable()
                    .flex_1()
                    .h(px(30.0))
                    .flex()
                    .items_center()
                    .px(px(10.0))
                    .rounded(theme.radii.control)
                    .bg(theme.chat_surface)
                    .border_1()
                    .border_color(theme.hairline)
                    .text_size(theme.typography.footnote)
                    .text_color(theme.title)
                    .on_click(move |_, window, cx| window.focus(&address_focus, cx))
                    .on_key_down(move |event, window, cx| {
                        address_entity.update(cx, |surface, cx| {
                            surface.on_address_key(event, window, cx);
                        });
                    })
                    .child(div().mr(px(8.0)).text_color(theme.meta).child("◎"))
                    .child(self.address_draft.clone()),
            )
            .child(
                div()
                    .id("browser-page-title")
                    .debug_selector(|| "browser-page-title".to_owned())
                    .max_w(px(220.0))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.subtitle)
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
                        .bg(theme.tab_focus_accent)
                        .text_color(theme.canvas)
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
        .text_color(if enabled { theme.title } else { theme.meta })
        .when(enabled, |this| {
            this.hover(|style| style.bg(theme.row_hover))
                .on_click(move |_, _, cx| callback(cx))
        })
        .child(label)
}

impl Render for BrowserSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let entity = cx.entity();
        let permission = self.state.permission_prompt().cloned();
        let webview = NativeWebViewElement::new(self.webview.clone());

        div()
            .id("browser-surface")
            .debug_selector(|| "browser-surface".to_owned())
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.chat_surface)
            .text_color(theme.title)
            .child(self.render_toolbar(theme, entity.clone()))
            .when_some(self.startup_error.clone(), |this, error| {
                this.child(
                    div()
                        .id("browser-error")
                        .debug_selector(|| "browser-error".to_owned())
                        .w_full()
                        .px(px(12.0))
                        .py(px(8.0))
                        .bg(theme.tab_error)
                        .text_color(theme.canvas)
                        .text_size(theme.typography.footnote)
                        .child(error),
                )
            })
            .when_some(self.state.error().map(str::to_owned), |this, error| {
                this.child(
                    div()
                        .id("browser-navigation-error")
                        .debug_selector(|| "browser-navigation-error".to_owned())
                        .w_full()
                        .px(px(12.0))
                        .py(px(8.0))
                        .bg(theme.tab_error)
                        .text_color(theme.canvas)
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
                        .bg(theme.tab_needs_input)
                        .text_color(theme.canvas)
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
            .child(
                div()
                    .id("browser-webview-region")
                    .relative()
                    .flex_1()
                    .child(webview),
            )
    }
}

impl Drop for BrowserSurface {
    fn drop(&mut self) {
        if let Some(webview) = self.webview.borrow().as_ref() {
            let _ = webview.set_visible(false);
        }
    }
}

struct NativeWebViewElement {
    webview: SharedWebView,
}

impl NativeWebViewElement {
    fn new(webview: SharedWebView) -> Self {
        Self { webview }
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
        _: &mut Window,
        _: &mut App,
    ) -> Self::PrepaintState {
        if let Some(webview) = self.webview.borrow().as_ref() {
            let _ = webview.set_bounds(Rect {
                position: LogicalPosition::new(
                    f32::from(bounds.origin.x),
                    f32::from(bounds.origin.y),
                )
                .into(),
                size: LogicalSize::new(
                    f32::from(bounds.size.width).max(1.0),
                    f32::from(bounds.size.height).max(1.0),
                )
                .into(),
            });
        }
        ()
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
}
