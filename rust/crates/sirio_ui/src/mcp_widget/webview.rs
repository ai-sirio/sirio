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
    // Only a missing or wrong secret is forged. A message with the right secret whose `m` is
    // not a JSON object is the relay's own traffic gone wrong, not an attack: it is dropped, and
    // still counts toward the rate.
    let wrapped = serde_json::from_str::<serde_json::Value>(body).ok().and_then(|value| {
        let object = value.as_object()?;
        if object.len() != 2 || object.get("s")?.as_str()? != secret {
            return None;
        }
        object.get("m").cloned()
    });
    let mut inbox = inbox.borrow_mut();
    match wrapped {
        None => inbox.forged = inbox.forged.saturating_add(1),
        Some(message) if !message.is_object() => {
            inbox.dropped = inbox.dropped.saturating_add(1);
        }
        Some(_) if inbox.messages.len() >= INBOX_CAP => {
            inbox.dropped = inbox.dropped.saturating_add(1);
        }
        Some(message) => inbox.messages.push_back(message.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // How it can fail, written first: an array `m` with the right secret is counted forged (and
    // closes the widget); a string `m` with the right secret is counted forged; a wrong secret
    // is not counted forged; a dropped `m` does not count toward the rate (it is neither queued
    // nor counted dropped); an object `m` is not queued.
    #[test]
    fn only_a_missing_or_wrong_secret_is_forged_and_a_dropped_message_counts_toward_the_rate() {
        let inbox: WidgetInbox = Rc::new(RefCell::new(Inbox::default()));
        let post = |text: &str| accept_ipc(text, "s3cret", &inbox);
        let state = || {
            let inbox = inbox.borrow();
            (inbox.messages.len(), inbox.forged, inbox.dropped)
        };
        post(r#"{"s":"s3cret","m":{"method":"ping"}}"#);
        assert_eq!(state(), (1, 0, 0), "an object is queued");
        post(r#"{"s":"s3cret","m":[1,2]}"#);
        assert_eq!(state(), (1, 0, 1), "an array is dropped, not forged");
        post(r#"{"s":"s3cret","m":"text"}"#);
        assert_eq!(state(), (1, 0, 2), "a string is dropped, not forged");
        post(r#"{"s":"s3cret","m":null}"#);
        assert_eq!(state(), (1, 0, 3), "null is dropped, not forged");
        post(r#"{"s":"wrong","m":{"method":"ping"}}"#);
        assert_eq!(state(), (1, 1, 3), "a wrong secret is forged");
        post(r#"{"m":{"method":"ping"}}"#);
        post("not json");
        assert_eq!(state(), (1, 3, 3), "a missing secret and a malformed body are forged");
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
        // These two settings are documented no-ops on WebKitGTK 2.54.1 (an `<a ping>` is blocked
        // by the CSP, and DNS prefetching has no working switch there). They are kept because
        // they work on older engines.
        settings.set_enable_hyperlink_auditing(false);
        settings.set_enable_dns_prefetching(false);
        // The engine's own hints: a CSP does not cover them, so each is switched off by feature.
        for identifier in set_link_features(&settings, false) {
            eprintln!("widget webview: the {identifier} feature could not be switched off");
        }
    }
    view.drag_dest_unset();
    view.connect_context_menu(|_, _, _, _| true);
    // The page takes no keyboard input in this version. A click gives the page's native window
    // GTK focus, and under X11 a key typed with the pointer over it goes to that window, so
    // handing focus back cannot keep keys from it. GTK delivers every key event to the
    // top-level first, and a connected handler that stops runs before GtkWindow's own, so the
    // WebView never receives one.
    if let Some(toplevel) = view.toplevel().and_then(|top| top.downcast::<gtk::Window>().ok()) {
        toplevel.connect_key_press_event(|_, _| gtk::glib::Propagation::Stop);
        toplevel.connect_key_release_event(|_, _| gtk::glib::Propagation::Stop);
    }
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

/// The engine features a widget's webview switches off: `<link rel=preconnect>` opens a TCP
/// connection, `<link rel=prefetch>` fetches, and speculation rules prefetch, none of which a CSP
/// covers. These are the identifiers WebKitGTK 2.54.1 lists (`SpeculationRulesPrefetchEnabled`
/// is a preference key there, not a feature identifier).
pub const LINK_FEATURES: [&str; 3] = ["LinkPreconnect", "LinkPrefetch", "SpeculationRulesPrefetch"];

/// Sets each of [`LINK_FEATURES`] on `settings`: off for a widget; the proof's control turns
/// them back on. The feature switch exists from WebKitGTK 2.42 and the `webkit2gtk` bindings in
/// use do not expose it, so the functions are looked up at run time with `dlsym`: on an older
/// WebKitGTK they are absent and nothing is set.
/// Returns the identifiers it could not set: all of them without the API, else those the engine
/// does not list.
pub fn set_link_features(settings: &webkit2gtk::Settings, enabled: bool) -> Vec<&'static str> {
    use std::ffi::{CStr, c_char, c_int, c_void};

    use webkit2gtk::glib::object::ObjectType;

    unsafe extern "C" {
        // Always present in libc/libdl, unlike the WebKitGTK symbols below.
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }
    // RTLD_DEFAULT is a null handle on Linux.
    let find = |name: &CStr| unsafe { dlsym(std::ptr::null_mut(), name.as_ptr()) };
    let all_features = find(c"webkit_settings_get_all_features");
    let set_enabled = find(c"webkit_settings_set_feature_enabled");
    let list_length = find(c"webkit_feature_list_get_length");
    let list_get = find(c"webkit_feature_list_get");
    let list_unref = find(c"webkit_feature_list_unref");
    let identifier_of = find(c"webkit_feature_get_identifier");
    if [all_features, set_enabled, list_length, list_get, list_unref, identifier_of]
        .iter()
        .any(|symbol| symbol.is_null())
    {
        return LINK_FEATURES.to_vec();
    }
    // SAFETY: each symbol is the WebKitGTK 2.42+ function of that name, with the C signature
    // transmuted to here (webkit_settings_get_all_features, webkit_settings_set_feature_enabled,
    // webkit_feature_list_{get_length,get,unref}, webkit_feature_get_identifier). The list is
    // owned by us (transfer full) and unreferenced once; a feature taken from it is borrowed
    // and used only before that; `settings` is a live WebKitSettings for the whole call.
    unsafe {
        let all_features: unsafe extern "C" fn() -> *mut c_void = std::mem::transmute(all_features);
        let set_enabled: unsafe extern "C" fn(*mut c_void, *mut c_void, c_int) =
            std::mem::transmute(set_enabled);
        let list_length: unsafe extern "C" fn(*mut c_void) -> usize =
            std::mem::transmute(list_length);
        let list_get: unsafe extern "C" fn(*mut c_void, usize) -> *mut c_void =
            std::mem::transmute(list_get);
        let list_unref: unsafe extern "C" fn(*mut c_void) = std::mem::transmute(list_unref);
        let identifier_of: unsafe extern "C" fn(*mut c_void) -> *const c_char =
            std::mem::transmute(identifier_of);
        let list = all_features();
        if list.is_null() {
            return LINK_FEATURES.to_vec();
        }
        let mut missing = LINK_FEATURES.to_vec();
        for index in 0..list_length(list) {
            let feature = list_get(list, index);
            if feature.is_null() {
                continue;
            }
            let identifier = identifier_of(feature);
            if identifier.is_null() {
                continue;
            }
            let identifier = CStr::from_ptr(identifier).to_string_lossy().into_owned();
            if let Some(at) = missing.iter().position(|wanted| *wanted == identifier) {
                set_enabled(settings.as_ptr().cast(), feature, c_int::from(enabled));
                missing.remove(at);
            }
        }
        list_unref(list);
        missing
    }
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
