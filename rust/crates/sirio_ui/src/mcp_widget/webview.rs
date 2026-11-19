//! The widget webview: one WebKitGTK child per widget, isolated by construction.
//!
//! The widget's HTML never loads as the top page. A relay page (served once, under its own
//! scheme) holds the view in a sandboxed iframe on a second scheme, so the two never share an
//! origin. The view's CSP is a response header the widget cannot edit. Messages travel
//! view -> relay (`postMessage`) -> Rust (IPC, wrapped with a per-widget secret). The webview
//! is ephemeral (incognito), hidden and unfocused at build, takes no drops, shows no context
//! menu, and denies every permission, new window and download.

use std::{
    borrow::Cow,
    cell::RefCell,
    collections::VecDeque,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use gtk::prelude::*;
use webkit2gtk::{AuthenticationRequestExt, FileChooserRequestExt, SettingsExt, WebViewExt};
use wry::{
    NewWindowResponse, PermissionResponse, Rect, WebView, WebViewBuilder, WebViewExtUnix,
    dpi::{LogicalPosition, LogicalSize},
    http::{Request, Response, StatusCode, header},
};

pub const RELAY_SCHEME: &str = "sirio-widget-relay";
pub const VIEW_SCHEME: &str = "sirio-widget-view";

const RELAY_HTML: &str = include_str!("relay.html");
/// An IPC body over this is dropped unread.
const MAX_IPC_BYTES: usize = 1 << 20;
/// The inbox holds this many messages; overflow is dropped.
const INBOX_CAP: usize = 64;

/// What one widget webview serves. `key`: 16 lowercase hex chars; `secret`: 32 lowercase hex
/// chars from getrandom. `view_csp` is always sent; the proof's control passes a permissive one.
pub struct WidgetPage {
    pub key: String,
    pub secret: String,
    pub html: String,
    pub view_csp: String,
}

/// 32 lowercase hex chars from the system's random source.
pub fn new_secret() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the system random source is available");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Default)]
pub struct Inbox {
    pub messages: VecDeque<String>,
    pub forged: u32,
    /// Messages dropped unread or over the cap: they never reach the bridge, but they were
    /// posted, so they count toward its rate.
    pub dropped: u32,
}

/// Messages the relay forwarded with the right secret (unwrapped `m`, as JSON text).
pub type WidgetInbox = Rc<RefCell<Inbox>>;

pub fn relay_url(key: &str) -> String {
    format!("{RELAY_SCHEME}://{key}/")
}

pub fn view_url(key: &str) -> String {
    format!("{VIEW_SCHEME}://{key}/")
}

fn view_origin(key: &str) -> String {
    format!("{VIEW_SCHEME}://{key}")
}

fn relay_csp(key: &str) -> String {
    format!(
        "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; \
         frame-src {}; base-uri 'none'; form-action 'none'",
        view_origin(key)
    )
}

fn relay_document(page: &WidgetPage) -> String {
    RELAY_HTML
        .replace("{{SECRET}}", &page.secret)
        .replace("{{VIEW_URL}}", &view_url(&page.key))
        .replace("{{VIEW_ORIGIN}}", &view_origin(&page.key))
}

fn not_found() -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Cow::Borrowed(&b""[..]))
        .expect("a static response is valid")
}

fn html_response(body: String, csp: &str) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .header("Content-Security-Policy", csp)
        .body(Cow::Owned(body.into_bytes()))
        .unwrap_or_else(|_| not_found())
}

/// Only path `/` on host `key`, no query.
fn addresses(request: &Request<Vec<u8>>, key: &str) -> bool {
    let uri = request.uri();
    uri.host() == Some(key) && uri.path() == "/" && uri.query().is_none()
}

/// Reads one IPC body: an accepted message is queued, a malformed or wrongly-keyed one bumps
/// the forged count, an oversized one or one over the inbox cap is dropped.
fn accept_ipc(body: &str, secret: &str, inbox: &WidgetInbox) {
    if body.len() > MAX_IPC_BYTES {
        let mut inbox = inbox.borrow_mut();
        inbox.dropped = inbox.dropped.saturating_add(1);
        return;
    }
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            let object = value.as_object()?;
            if object.len() != 2 || object.get("s")?.as_str()? != secret {
                return None;
            }
            let message = object.get("m")?;
            message.is_object().then(|| message.to_string())
        });
    let mut inbox = inbox.borrow_mut();
    match message {
        None => inbox.forged = inbox.forged.saturating_add(1),
        Some(_) if inbox.messages.len() >= INBOX_CAP => {
            inbox.dropped = inbox.dropped.saturating_add(1);
        }
        Some(text) => inbox.messages.push_back(text),
    }
}

/// Builds the child webview of `window` with every isolation setting, hidden, at `bounds`.
/// `incognito: false` exists only for the proof's control run. Production passes `true`.
pub fn build_widget_child(
    window: &mut gpui::Window,
    page: Arc<WidgetPage>,
    inbox: WidgetInbox,
    incognito: bool,
) -> wry::Result<WebView> {
    // wry's ephemeral WebContext asserts GTK is up, and a first widget may come before any
    // Browser tab has started it.
    gtk::init().map_err(|error| wry::Error::Io(std::io::Error::other(error.to_string())))?;
    let window: &gpui::Window = window;
    let relay_page = page.clone();
    let view_page = page.clone();
    let key = page.key.clone();
    let nav_relay = relay_url(&key);
    let nav_view = view_url(&key);
    let secret = page.secret.clone();
    let url = relay_url(&key);
    let (webview, _) = crate::native_child::build_in_window(window, |parent| {
        let relay_page = relay_page.clone();
        let view_page = view_page.clone();
        let inbox = inbox.clone();
        let secret = secret.clone();
        let nav_relay = nav_relay.clone();
        let nav_view = nav_view.clone();
        // `build_in_window` may call this twice; each attempt gets its own one-shot state.
        let served = AtomicBool::new(false);
        WebViewBuilder::new()
            .with_incognito(incognito)
            .with_devtools(false)
            .with_visible(false)
            .with_focused(false)
            .with_autoplay(false)
            .with_bounds(Rect {
                position: LogicalPosition::new(0_i32, 0_i32).into(),
                size: LogicalSize::new(1_u32, 1_u32).into(),
            })
            .with_url(url.clone())
            .with_permission_handler(|_| PermissionResponse::Deny)
            .with_custom_protocol(RELAY_SCHEME.to_owned(), move |_, request| {
                if !addresses(&request, &relay_page.key) || served.swap(true, Ordering::SeqCst) {
                    return not_found();
                }
                html_response(relay_document(&relay_page), &relay_csp(&relay_page.key))
            })
            .with_custom_protocol(VIEW_SCHEME.to_owned(), move |_, request| {
                if !addresses(&request, &view_page.key) {
                    return not_found();
                }
                html_response(view_page.html.clone(), &view_page.view_csp)
            })
            .with_navigation_handler(move |target| target == nav_relay || target == nav_view)
            .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
            .with_download_started_handler(|_, _| false)
            .with_ipc_handler(move |request| accept_ipc(request.body(), &secret, &inbox))
            .build_as_child(&parent)
    })
    .map_err(|message| wry::Error::Io(std::io::Error::other(message)))?;

    let view = webview.webview();
    if let Some(settings) = WebViewExt::settings(&view) {
        settings.set_enable_webrtc(false);
        settings.set_enable_media_stream(false);
        // `<a ping>` posts to a URL the page names, and DNS prefetching resolves names it picks.
        settings.set_enable_hyperlink_auditing(false);
        settings.set_enable_dns_prefetching(false);
    }
    view.drag_dest_unset();
    view.connect_context_menu(|_, _, _, _| true);
    // No native file chooser from `<input type=file>`, and no HTTP authentication prompt that
    // looks like Sirio's own: both requests are cancelled.
    view.connect_run_file_chooser(|_, request| {
        request.cancel();
        true
    });
    view.connect_authenticate(|_, request| {
        request.cancel();
        true
    });
    Ok(webview)
}

/// Runs pending GTK events for at most `budget`.
pub fn pump_gtk(budget: Duration) {
    if !gtk::is_initialized_main_thread() {
        return;
    }
    let deadline = Instant::now() + budget;
    while gtk::events_pending() && Instant::now() < deadline {
        gtk::main_iteration_do(false);
    }
}

/// Whether the webview still has a GTK drop target list (it must not).
pub fn accepts_drops(webview: &WebView) -> bool {
    webview.webview().drag_dest_get_target_list().is_some()
}

/// Pushes pending X requests out, as `browser.rs` `flush_native_window_ops` does: an unmap or a
/// destroy sits in Xlib's buffer until something flushes it, and the window stays painted.
pub fn flush_x() {
    // One 4 ms budget for both passes.
    let deadline = Instant::now() + Duration::from_millis(4);
    for _ in 0..2 {
        pump_gtk(deadline.saturating_duration_since(Instant::now()));
        if let Some(display) = gtk::gdk::Display::default() {
            display.flush();
        }
    }
}

/// Rust -> view: `window.__sirioDeliver(<text as a JS string literal>)`.
pub fn deliver_script(text: &str) -> String {
    let literal = serde_json::to_string(text).expect("a string always serializes");
    format!("window.__sirioDeliver({literal})")
}
