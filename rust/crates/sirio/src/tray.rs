//! F-USE-04 / F-USE-05 / F-WIN-08: a StatusNotifierItem tray icon -- the
//! Linux equivalent of the Swift `MenuBarExtra` + `AgentRosterView` pair
//! (`App/SirioApp.swift:115`, `App/AgentRosterView.swift` in the Swift
//! tree). COSMIC's panel (and GNOME/KDE, via their SNI/AppIndicator
//! bridges) render this with no extra install step. `ksni` wraps the
//! freedesktop D-Bus protocol; its "blocking" feature runs the whole D-Bus
//! service on its own OS thread, so nothing here needs an async runtime
//! wired through GPUI's own executor.
//!
//! ksni's D-Bus thread cannot reach GPUI's `Context<SirioWorkspace>`
//! directly, so this module only ever talks to `main.rs` through two
//! `Arc<Mutex<..>>` values -- the same bridge shape `main.rs` already uses
//! for `pending_actions`/`control_actions`: [`SharedRoster`] is written by
//! the workspace and read by the tray's `menu()`; [`TrayRequestQueue`] is
//! written by menu clicks and drained by the workspace's existing 40ms
//! poll loop.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CREATESTRUCTW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetCursorPos, GetMessageW, GetWindowLongPtrW,
    HWND_MESSAGE, IDI_APPLICATION, LoadIconW, MF_DISABLED, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG,
    PostMessageW, PostQuitMessage, RegisterClassW, SetForegroundWindow, SetWindowLongPtrW,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, WM_APP, WM_CONTEXTMENU,
    WM_LBUTTONUP, WM_NCCREATE, WM_NCDESTROY, WM_RBUTTONUP, WNDCLASSW, WS_POPUP,
};

use sirio_activity::AgentStatus;

/// One roster row: a worktree with at least one pane reporting a live
/// [`AgentStatus`]. Mirrors Swift's `activeAgentWorktrees` filter
/// (`App/AppModel.swift:157`).
#[derive(Clone, PartialEq)]
pub struct TrayRosterEntry {
    pub path: PathBuf,
    pub branch: String,
    pub project_name: String,
    pub status: AgentStatus,
    /// *When* this entry's status last changed -- never how long ago
    /// (#187). The poll loop in `main.rs` keeps the previous tick's
    /// snapshot and only nudges the tray when the two differ, so every
    /// field here is part of that change key. An elapsed `Duration` grows
    /// on its own, so it made every tick compare unequal and the guard
    /// never held. The age is computed where it is rendered, by
    /// [`roster_menu_label`], which is also fresher.
    pub status_changed_at: Option<Instant>,
}

/// What a tray click asks the app to do next. Drained by the same 40ms
/// poll loop `main.rs` already runs for `WorkspaceAction`/`ControlAction`.
pub enum TrayRequest {
    /// A roster row (F-USE-05): reselect the worktree, then jump to its
    /// worst-status tab.
    SelectWorktree(PathBuf),
    /// The tray icon's own left click, and the roster's fallback when no
    /// row exists to click (F-WIN-08): just re-show the window. Linux has
    /// no Dock icon to click for this, which is why this row lives in the
    /// tray slice at all.
    ShowWindow,
    /// "Quit Sirio".
    Quit,
}

pub type SharedRoster = Arc<Mutex<Vec<TrayRosterEntry>>>;
pub type TrayRequestQueue = Arc<Mutex<Vec<TrayRequest>>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrayIconKind {
    Normal,
    Attention,
}

fn tray_icon_kind(roster: &[TrayRosterEntry]) -> TrayIconKind {
    if roster
        .iter()
        .any(|entry| entry.status == AgentStatus::NeedsInput)
    {
        TrayIconKind::Attention
    } else {
        TrayIconKind::Normal
    }
}

const NO_ACTIVE_AGENTS_LABEL: &str = "No active agents";
const OPEN_SIRIO_LABEL: &str = "Open Sirio";
const QUIT_SIRIO_LABEL: &str = "Quit Sirio";
#[cfg(target_os = "linux")]
const NORMAL_ICON_NAME: &str = "utilities-terminal";
#[cfg(target_os = "linux")]
// `dialog-question` is the freedesktop mark for an unanswered question,
// matching `NeedsInput` instead of implying that something is broken. If a
// theme lacks it, the host degrades to no overlay rather than a wrong icon.
const ATTENTION_ICON_NAME: &str = "dialog-question";

fn format_status_age(age: Duration) -> String {
    let seconds = age.as_secs();
    if seconds < 60 {
        format!("{}s", seconds.max(1))
    } else if seconds < 60 * 60 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}h", seconds / (60 * 60))
    }
}

/// The caller supplies `now` for the same reason
/// `AgentActivityModel::status_age_for_panes` does: it keeps this
/// deterministic in tests. Production callers pass `Instant::now()` at the
/// moment the menu is actually drawn (#187).
fn roster_menu_label(entry: &TrayRosterEntry, now: Instant) -> String {
    let status = entry.status.human_label();
    let status = entry
        .status_changed_at
        .and_then(|changed_at| now.checked_duration_since(changed_at))
        .map_or_else(
            || status.to_string(),
            |age| format!("{status} · {}", format_status_age(age)),
        );
    format!("{} — {} ({status})", entry.branch, entry.project_name)
}

/// Opaque handle kept alive for the process's lifetime. `ksni` serves
/// `GetLayout` from an internally cached menu tree that it only rebuilds
/// (and announces via `LayoutUpdated`) inside `Handle::update` -- writing
/// straight into [`SharedRoster`] changes what `Tray::menu()` would
/// *return*, but never nudges ksni into calling it again on its own, so
/// [`TrayHandle::nudge`] is what actually makes a roster change visible to
/// a D-Bus client. Confirmed live: a bare `SharedRoster` write left
/// `GetLayout` answering a stale "No active agents" indefinitely; adding
/// the `update(|_| {})` call below is what made it pick up the change.
#[cfg(target_os = "linux")]
pub struct TrayHandle(ksni::blocking::Handle<AgentRosterTray>);

#[cfg(target_os = "linux")]
impl TrayHandle {
    pub fn nudge(&self) {
        let _ = self.0.update(|_tray| {});
    }
}

/// Opaque handle for the dedicated Windows tray message-pump thread.
#[cfg(target_os = "windows")]
pub struct TrayHandle {
    hwnd: isize,
    thread: Option<std::thread::JoinHandle<()>>,
    roster: SharedRoster,
    icon: isize,
    attention_icon: isize,
    shown_kind: Mutex<TrayIconKind>,
}

#[cfg(target_os = "windows")]
impl TrayHandle {
    /// Windows caches the notification-area icon, so compare the shared roster
    /// with the currently shown kind and ask the shell to repaint only when
    /// the two differ.
    pub fn nudge(&self) {
        let Ok(roster) = self.roster.lock() else {
            return;
        };
        let kind = tray_icon_kind(&roster);
        drop(roster);

        let Ok(mut shown_kind) = self.shown_kind.lock() else {
            return;
        };
        if *shown_kind == kind {
            return;
        }

        let icon = match kind {
            TrayIconKind::Normal => self.icon,
            TrayIconKind::Attention => self.attention_icon,
        } as windows_sys::Win32::UI::WindowsAndMessaging::HICON;
        let mut data = notify_icon_data(self.hwnd as HWND, icon);
        data.uFlags = NIF_ICON;
        if unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) } != 0 {
            *shown_kind = kind;
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for TrayHandle {
    fn drop(&mut self) {
        if self.hwnd == 0 {
            return;
        }

        // The handle is only created after the pump owns this valid window;
        // posting a message does not dereference the opaque HWND here.
        let posted = unsafe {
            PostMessageW(
                self.hwnd as windows_sys::Win32::Foundation::HWND,
                WINDOWS_TRAY_SHUTDOWN_MESSAGE,
                0,
                0,
            )
        } != 0;
        if posted {
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

#[cfg(all(
    not(target_os = "linux"),
    not(target_os = "windows"),
    not(target_os = "macos")
))]
pub struct TrayHandle;

#[cfg(all(
    not(target_os = "linux"),
    not(target_os = "windows"),
    not(target_os = "macos")
))]
impl TrayHandle {
    pub fn nudge(&self) {}
}

/// The status item and the menu delegate that renders it, both owned for
/// the process's lifetime.
///
/// Unlike Linux's ksni handle and Windows' pump thread, nothing here is
/// `Send`: `NSStatusItem` is main-thread-only, and every caller is already
/// on it — `spawn` runs inside `Application::run`'s callback and `nudge` is
/// called from the workspace's own GPUI update. Keeping the AppKit objects
/// in the handle rather than behind a channel is what makes that true by
/// construction instead of by convention.
#[cfg(target_os = "macos")]
pub struct TrayHandle {
    status_item: objc2::rc::Id<objc2_app_kit::NSStatusItem>,
    /// Retained so the menu's delegate outlives the menu, which holds only
    /// a weak reference to it.
    _delegate: objc2::rc::Id<macos::TrayMenuDelegate>,
    roster: SharedRoster,
    /// The kind the button currently shows. `nudge` repaints only when this
    /// changes, which is the contract the Windows port established.
    shown_kind: std::cell::Cell<TrayIconKind>,
}

#[cfg(target_os = "macos")]
impl TrayHandle {
    pub fn nudge(&self) {
        let kind = self
            .roster
            .lock()
            .map(|roster| tray_icon_kind(&roster))
            .unwrap_or(TrayIconKind::Normal);
        if kind == self.shown_kind.get() {
            return;
        }
        let Some(mtm) = objc2_foundation::MainThreadMarker::new() else {
            return;
        };
        self.shown_kind.set(kind);
        macos::apply_icon(&self.status_item, kind, mtm);
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{
        NO_ACTIVE_AGENTS_LABEL, OPEN_SIRIO_LABEL, QUIT_SIRIO_LABEL, SharedRoster, TrayIconKind,
        TrayRequest, TrayRequestQueue, roster_menu_label,
    };
    use objc2::rc::Id;
    use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject, Sel};
    use objc2::{ClassType, DeclaredClass, declare_class, msg_send_id, mutability, sel};
    use objc2_app_kit::{NSImage, NSMenu, NSMenuDelegate, NSMenuItem, NSStatusItem};
    use objc2_foundation::{MainThreadMarker, NSString, ns_string};
    use std::time::Instant;

    /// SF Symbols, chosen to say the same thing the freedesktop names on
    /// Linux do: a terminal at rest, and the mark for an unanswered
    /// question when an agent needs input — not an error mark, which would
    /// claim something is broken. Both render as template images, so the
    /// menu bar tints them for light, dark and "reduce transparency" on its
    /// own; a coloured badge would be flattened away by exactly that.
    const NORMAL_SYMBOL: &str = "terminal";
    const ATTENTION_SYMBOL: &str = "questionmark.circle";
    /// Drawn instead of an image where the symbol cannot be loaded (macOS
    /// older than 11, or a future rename): a titled status item is still a
    /// working roster, where an item with neither image nor title is an
    /// invisible one.
    const NORMAL_FALLBACK_TITLE: &str = "T";
    const ATTENTION_FALLBACK_TITLE: &str = "T?";

    fn symbol_for(kind: TrayIconKind) -> &'static str {
        match kind {
            TrayIconKind::Normal => NORMAL_SYMBOL,
            TrayIconKind::Attention => ATTENTION_SYMBOL,
        }
    }

    fn fallback_title_for(kind: TrayIconKind) -> &'static str {
        match kind {
            TrayIconKind::Normal => NORMAL_FALLBACK_TITLE,
            TrayIconKind::Attention => ATTENTION_FALLBACK_TITLE,
        }
    }

    /// Points the status item's button at `kind`'s symbol, falling back to a
    /// title when the symbol does not resolve.
    pub(super) fn apply_icon(
        status_item: &NSStatusItem,
        kind: TrayIconKind,
        mtm: MainThreadMarker,
    ) {
        let Some(button) = (unsafe { status_item.button(mtm) }) else {
            return;
        };
        let symbol = NSString::from_str(symbol_for(kind));
        let description = NSString::from_str("Sirio");
        let image = unsafe {
            NSImage::imageWithSystemSymbolName_accessibilityDescription(&symbol, Some(&description))
        };
        match image {
            Some(image) => {
                unsafe { image.setTemplate(true) };
                unsafe { button.setImage(Some(&image)) };
                unsafe { button.setTitle(ns_string!("")) };
            }
            None => {
                unsafe { button.setImage(None) };
                unsafe { button.setTitle(&NSString::from_str(fallback_title_for(kind))) };
            }
        }
    }

    pub(super) struct DelegateState {
        pub roster: SharedRoster,
        pub requests: TrayRequestQueue,
        /// Held rather than re-derived: every method below is called by
        /// AppKit on the main thread, and carrying the proof from `spawn`
        /// keeps that a fact of construction instead of a runtime check
        /// that could only ever fail by being wrong about it.
        pub mtm: MainThreadMarker,
    }

    declare_class!(
        /// Rebuilds the roster menu each time it is about to open, and
        /// receives its clicks.
        ///
        /// The menu is built on open rather than on every roster write for
        /// the reason `roster_menu_label` documents: the age it renders is
        /// only correct for the instant it is drawn, and the only instant
        /// that matters is the one the user is looking at it.
        pub(super) struct TrayMenuDelegate;

        unsafe impl ClassType for TrayMenuDelegate {
            type Super = NSObject;
            type Mutability = mutability::MainThreadOnly;
            const NAME: &'static str = "SirioTrayMenuDelegate";
        }

        impl DeclaredClass for TrayMenuDelegate {
            type Ivars = DelegateState;
        }

        unsafe impl NSObjectProtocol for TrayMenuDelegate {}

        unsafe impl NSMenuDelegate for TrayMenuDelegate {
            #[method(menuNeedsUpdate:)]
            unsafe fn menu_needs_update(&self, menu: &NSMenu) {
                self.rebuild(menu);
            }
        }

        unsafe impl TrayMenuDelegate {
            /// A roster row. The clicked worktree is carried by the item's
            /// tag — its index in the roster snapshot the menu was built
            /// from, resolved back to a path here.
            #[method(rosterItemClicked:)]
            unsafe fn roster_item_clicked(&self, sender: &NSMenuItem) {
                let index = unsafe { sender.tag() };
                let path = self
                    .ivars()
                    .roster
                    .lock()
                    .ok()
                    .and_then(|roster| {
                        usize::try_from(index)
                            .ok()
                            .and_then(|index| roster.get(index).map(|entry| entry.path.clone()))
                    });
                match path {
                    Some(path) => self.push(TrayRequest::SelectWorktree(path)),
                    // The roster changed between building the menu and the
                    // click. Showing the window is the honest answer: it is
                    // what the row would have done first anyway.
                    None => self.push(TrayRequest::ShowWindow),
                }
            }

            #[method(openSirioClicked:)]
            unsafe fn open_sirio_clicked(&self, _sender: &NSMenuItem) {
                self.push(TrayRequest::ShowWindow);
            }

            #[method(quitSirioClicked:)]
            unsafe fn quit_sirio_clicked(&self, _sender: &NSMenuItem) {
                self.push(TrayRequest::Quit);
            }
        }
    );

    impl TrayMenuDelegate {
        pub(super) fn new(
            mtm: MainThreadMarker,
            roster: SharedRoster,
            requests: TrayRequestQueue,
        ) -> Id<Self> {
            let this = mtm.alloc::<Self>().set_ivars(DelegateState {
                roster,
                requests,
                mtm,
            });
            unsafe { msg_send_id![super(this), init] }
        }

        fn push(&self, request: TrayRequest) {
            if let Ok(mut requests) = self.ivars().requests.lock() {
                requests.push(request);
            }
        }

        /// The menu's settled shape: roster rows (or one disabled "No active
        /// agents"), `Open Sirio`, separator, `Quit Sirio` — with `Open
        /// Sirio` present so the only enabled action is never the
        /// destructive one.
        fn rebuild(&self, menu: &NSMenu) {
            unsafe { menu.removeAllItems() };
            let now = Instant::now();
            let rows: Vec<String> = self
                .ivars()
                .roster
                .lock()
                .map(|roster| {
                    roster
                        .iter()
                        .map(|entry| roster_menu_label(entry, now))
                        .collect()
                })
                .unwrap_or_default();

            if rows.is_empty() {
                let item = self.item(NO_ACTIVE_AGENTS_LABEL, None, -1);
                unsafe { item.setEnabled(false) };
                menu.addItem(&item);
            } else {
                for (index, label) in rows.into_iter().enumerate() {
                    let item = self.item(
                        &label,
                        Some(sel!(rosterItemClicked:)),
                        isize::try_from(index).unwrap_or(-1),
                    );
                    menu.addItem(&item);
                }
            }

            let open = self.item(OPEN_SIRIO_LABEL, Some(sel!(openSirioClicked:)), -1);
            menu.addItem(&open);
            menu.addItem(&NSMenuItem::separatorItem(self.ivars().mtm));
            let quit = self.item(QUIT_SIRIO_LABEL, Some(sel!(quitSirioClicked:)), -1);
            menu.addItem(&quit);
        }

        fn item(&self, label: &str, action: Option<Sel>, tag: isize) -> Id<NSMenuItem> {
            let title = NSString::from_str(label);
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    self.ivars().mtm.alloc::<NSMenuItem>(),
                    &title,
                    action,
                    ns_string!(""),
                )
            };
            if action.is_some() {
                unsafe { item.setTarget(Some(self)) };
            }
            unsafe { item.setTag(tag) };
            item
        }
    }

    pub(super) fn menu_for(delegate: &TrayMenuDelegate) -> Id<NSMenu> {
        let menu = NSMenu::new(delegate.ivars().mtm);
        let protocol: &ProtocolObject<dyn NSMenuDelegate> = ProtocolObject::from_ref(delegate);
        unsafe { menu.setDelegate(Some(protocol)) };
        menu
    }
}

#[cfg(target_os = "linux")]
struct AgentRosterTray {
    roster: SharedRoster,
    requests: TrayRequestQueue,
}

#[cfg(target_os = "linux")]
impl ksni::Tray for AgentRosterTray {
    fn id(&self) -> String {
        "sirio".into()
    }

    fn title(&self) -> String {
        "Sirio".into()
    }

    fn icon_name(&self) -> String {
        NORMAL_ICON_NAME.into()
    }

    fn overlay_icon_name(&self) -> String {
        let kind = self
            .roster
            .lock()
            .map(|roster| tray_icon_kind(&roster))
            .unwrap_or(TrayIconKind::Normal);
        if kind == TrayIconKind::Attention {
            // ksni 0.3 exposes OverlayIconName, which keeps the base icon
            // recognisable while the question mark signals that an agent is
            // waiting for an answer.
            ATTENTION_ICON_NAME.into()
        } else {
            String::new()
        }
    }

    /// Left-click on the icon itself. There is no Swift equivalent -- the
    /// menu bar's popover has no bare "show window" button either -- but
    /// on Linux the tray is the *only* re-show surface (no Dock icon), so
    /// idle activation has to do something rather than nothing.
    fn activate(&mut self, _x: i32, _y: i32) {
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(TrayRequest::ShowWindow);
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        let entries = self
            .roster
            .lock()
            .map(|roster| roster.clone())
            .unwrap_or_default();
        let mut items: Vec<MenuItem<Self>> = Vec::new();
        if entries.is_empty() {
            items.push(
                StandardItem {
                    label: NO_ACTIVE_AGENTS_LABEL.into(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        } else {
            for entry in entries {
                let path = entry.path.clone();
                let label = roster_menu_label(&entry, Instant::now());
                items.push(
                    StandardItem {
                        label,
                        activate: Box::new(move |this: &mut Self| {
                            if let Ok(mut requests) = this.requests.lock() {
                                requests.push(TrayRequest::SelectWorktree(path.clone()));
                            }
                        }),
                        ..Default::default()
                    }
                    .into(),
                );
            }
        }
        items.push(
            StandardItem {
                label: OPEN_SIRIO_LABEL.into(),
                activate: Box::new(|this: &mut Self| {
                    if let Ok(mut requests) = this.requests.lock() {
                        requests.push(TrayRequest::ShowWindow);
                    }
                }),
                ..Default::default()
            }
            .into(),
        );
        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: QUIT_SIRIO_LABEL.into(),
                activate: Box::new(|this: &mut Self| {
                    if let Ok(mut requests) = this.requests.lock() {
                        requests.push(TrayRequest::Quit);
                    }
                }),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

/// Registers the StatusNotifierItem and returns the shared roster (the
/// caller keeps it fresh), the request queue (the caller drains it), and
/// the handle whose `nudge()` the caller must call after every roster
/// write for that write to actually reach a D-Bus client (see
/// [`TrayHandle`]'s doc comment). Returns `None` if no SNI host answered
/// -- headless CI, a WM with no tray applet -- in which case the app runs
/// exactly as it did before this module existed, just without the roster
/// surface.
#[cfg(target_os = "linux")]
pub fn spawn() -> Option<(SharedRoster, TrayRequestQueue, TrayHandle)> {
    use ksni::blocking::TrayMethods;
    let roster: SharedRoster = Arc::new(Mutex::new(Vec::new()));
    let requests: TrayRequestQueue = Arc::new(Mutex::new(Vec::new()));
    let tray = AgentRosterTray {
        roster: roster.clone(),
        requests: requests.clone(),
    };
    match tray.spawn() {
        Ok(handle) => Some((roster, requests, TrayHandle(handle))),
        Err(error) => {
            eprintln!("[tray] StatusNotifierItem unavailable, roster disabled: {error}");
            None
        }
    }
}

#[cfg(target_os = "windows")]
const WINDOWS_TRAY_CALLBACK_MESSAGE: u32 = WM_APP + 1;
#[cfg(target_os = "windows")]
const WINDOWS_TRAY_SHUTDOWN_MESSAGE: u32 = WM_APP + 2;
#[cfg(target_os = "windows")]
const WINDOWS_TRAY_FIRST_ROSTER_COMMAND: u32 = 1;
#[cfg(target_os = "windows")]
const WINDOWS_TRAY_OPEN_COMMAND: u32 = 0x7ffe;
#[cfg(target_os = "windows")]
const WINDOWS_TRAY_QUIT_COMMAND: u32 = 0x7fff;

#[cfg(target_os = "windows")]
struct WindowsTrayState {
    roster: SharedRoster,
    requests: TrayRequestQueue,
}

#[cfg(target_os = "windows")]
struct WindowsTrayReady {
    hwnd: isize,
    icon: isize,
    attention_icon: isize,
}

#[cfg(target_os = "windows")]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn notify_icon_data(
    hwnd: HWND,
    icon: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW::default();
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = 1;
    data.hIcon = icon;
    if !icon.is_null() {
        data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        data.uCallbackMessage = WINDOWS_TRAY_CALLBACK_MESSAGE;
        let tip = wide("Sirio");
        data.szTip[..tip.len()].copy_from_slice(&tip);
    }
    data
}

#[cfg(target_os = "windows")]
fn delete_windows_tray_icon(hwnd: HWND) {
    let data = notify_icon_data(hwnd, std::ptr::null_mut());
    // `data` contains the same window/id pair used to add this icon and is
    // alive for the duration of this synchronous Win32 call.
    let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
}

#[cfg(target_os = "windows")]
fn windows_tray_state(hwnd: HWND) -> *mut WindowsTrayState {
    // This window's user data is either null or the Box pointer installed by
    // `WM_NCCREATE`, and the call only reads that opaque pointer value.
    unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowsTrayState }
}

#[cfg(target_os = "windows")]
fn show_windows_menu(hwnd: HWND, state: &WindowsTrayState) {
    let entries = state
        .roster
        .lock()
        .map(|roster| roster.clone())
        .unwrap_or_default();

    // The menu handle is consumed by `DestroyMenu` below after all item text
    // pointers have been used by the synchronous Win32 calls.
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }

    if entries.is_empty() {
        let label = wide(NO_ACTIVE_AGENTS_LABEL);
        // `label` remains allocated while AppendMenuW copies its text.
        let _ =
            unsafe { AppendMenuW(menu, MF_STRING | MF_DISABLED | MF_GRAYED, 0, label.as_ptr()) };
    } else {
        for (index, entry) in entries.iter().enumerate() {
            let label = wide(&roster_menu_label(entry, Instant::now()));
            let command = WINDOWS_TRAY_FIRST_ROSTER_COMMAND + index as u32;
            // `label` remains allocated while AppendMenuW copies its text.
            let _ = unsafe { AppendMenuW(menu, MF_STRING, command as usize, label.as_ptr()) };
        }
    }

    let open_label = wide(OPEN_SIRIO_LABEL);
    // `open_label` remains allocated while AppendMenuW copies its text.
    let _ = unsafe {
        AppendMenuW(
            menu,
            MF_STRING,
            WINDOWS_TRAY_OPEN_COMMAND as usize,
            open_label.as_ptr(),
        )
    };

    // A null item string is the documented separator form of AppendMenuW.
    let _ = unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null()) };
    let quit_label = wide(QUIT_SIRIO_LABEL);
    // `quit_label` remains allocated while AppendMenuW copies its text.
    let _ = unsafe {
        AppendMenuW(
            menu,
            MF_STRING,
            WINDOWS_TRAY_QUIT_COMMAND as usize,
            quit_label.as_ptr(),
        )
    };

    let mut point = POINT::default();
    // Win32 writes one POINT into this valid, stack-owned output location.
    let have_cursor = unsafe { GetCursorPos(&mut point) } != 0;
    let command = if have_cursor {
        // This must happen before TrackPopupMenu: without it, the Win32 menu
        // can remain open when the user clicks away from it.
        let _ = unsafe { SetForegroundWindow(hwnd) };
        // The menu owns `menu` only for this synchronous call, and the null
        // RECT is the documented form when no exclusion rectangle is needed.
        unsafe {
            TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                0,
                hwnd,
                std::ptr::null(),
            ) as u32
        }
    } else {
        0
    };

    if command == WINDOWS_TRAY_QUIT_COMMAND {
        if let Ok(mut requests) = state.requests.lock() {
            requests.push(TrayRequest::Quit);
        }
    } else if command == WINDOWS_TRAY_OPEN_COMMAND {
        if let Ok(mut requests) = state.requests.lock() {
            requests.push(TrayRequest::ShowWindow);
        }
    } else if command >= WINDOWS_TRAY_FIRST_ROSTER_COMMAND {
        let index = (command - WINDOWS_TRAY_FIRST_ROSTER_COMMAND) as usize;
        if let Some(entry) = entries.get(index) {
            if let Ok(mut requests) = state.requests.lock() {
                requests.push(TrayRequest::SelectWorktree(entry.path.clone()));
            }
        }
    }

    // `menu` is the handle returned above and no Win32 call still uses it.
    let _ = unsafe { DestroyMenu(menu) };
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn windows_window_proc(
    hwnd: HWND,
    message: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE && lparam != 0 {
        // WM_NCCREATE supplies a valid CREATESTRUCTW pointer whose creation
        // parameter is the Box pointer passed to CreateWindowExW.
        let state = unsafe { (*(lparam as *const CREATESTRUCTW)).lpCreateParams as isize };
        // This window is still being created, so its user-data slot is ours to
        // initialize with the state pointer for later callbacks.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state) };
    }

    if message == WINDOWS_TRAY_SHUTDOWN_MESSAGE {
        delete_windows_tray_icon(hwnd);
        // This callback runs on the pump thread, so PostQuitMessage targets
        // exactly the GetMessageW loop that owns this window.
        unsafe { PostQuitMessage(0) };
        return 0;
    }

    if message == WINDOWS_TRAY_CALLBACK_MESSAGE {
        let state = windows_tray_state(hwnd);
        if !state.is_null() {
            // The user-data pointer remains the live Box allocation until the
            // pump destroys this window after GetMessageW returns.
            let state = unsafe { &*state };
            match lparam as u32 {
                WM_LBUTTONUP => {
                    if let Ok(mut requests) = state.requests.lock() {
                        requests.push(TrayRequest::ShowWindow);
                    }
                }
                WM_RBUTTONUP | WM_CONTEXTMENU => show_windows_menu(hwnd, state),
                _ => {}
            }
        }
        return 0;
    }

    if message == WM_NCDESTROY {
        // WM_NCDESTROY is the final callback for this HWND, so clearing the
        // slot prevents later messages from observing the freed Box pointer.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
    }

    // Unhandled messages belong to this window and are safe to forward to the
    // standard Win32 window procedure.
    unsafe { DefWindowProcW(hwnd, message, _wparam, lparam) }
}

#[cfg(target_os = "windows")]
fn run_windows_tray(
    roster: SharedRoster,
    requests: TrayRequestQueue,
    ready: std::sync::mpsc::SyncSender<Option<WindowsTrayReady>>,
) {
    let class_name = wide("SirioTrayWindowClass");
    // A null module name asks Windows for the current executable's instance.
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    if instance.is_null() {
        eprintln!("[tray] could not get the executable module, roster disabled");
        let _ = ready.send(None);
        return;
    }

    let window_class = WNDCLASSW {
        lpfnWndProc: Some(windows_window_proc),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        ..Default::default()
    };
    // `window_class` and `class_name` stay alive through the synchronous class
    // registration call, as required by RegisterClassW.
    if unsafe { RegisterClassW(&window_class) } == 0 {
        eprintln!("[tray] could not register the tray window class, roster disabled");
        let _ = ready.send(None);
        return;
    }

    let state = Box::into_raw(Box::new(WindowsTrayState { roster, requests }));
    // HWND_MESSAGE makes this a message-only window: it has no visible surface
    // and remains dedicated to Shell_NotifyIcon callbacks.
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            instance,
            state.cast(),
        )
    };
    if hwnd.is_null() {
        // This pointer came from Box::into_raw above and CreateWindowExW did
        // not return an owning window, so reclaim it exactly once here.
        let _ = unsafe { Box::from_raw(state) };
        eprintln!("[tray] could not create the tray window, roster disabled");
        let _ = ready.send(None);
        return;
    }

    // Resource IDs 1 and 2 are the normal and attention app icons used by
    // gpui and the Windows tray respectively.
    let mut icon = unsafe { LoadIconW(instance, std::ptr::with_exposed_provenance::<u16>(1)) };
    if icon.is_null() {
        // A system application icon keeps the normal tray state alive when its
        // embedded resource is unavailable in an unusual build.
        icon = unsafe { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) };
    }
    let mut attention_icon =
        unsafe { LoadIconW(instance, std::ptr::with_exposed_provenance::<u16>(2)) };
    if attention_icon.is_null() {
        // Keep the attention state usable with the same documented fallback
        // when its embedded resource is unavailable in an unusual build.
        attention_icon = unsafe { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) };
    }
    if icon.is_null() || attention_icon.is_null() {
        // hwnd is the message-only window created by this thread and is valid
        // for the synchronous cleanup call below.
        unsafe { DestroyWindow(hwnd) };
        // The window is gone, so this is the sole owner of the state Box.
        let _ = unsafe { Box::from_raw(state) };
        eprintln!("[tray] could not load a tray icon, roster disabled");
        let _ = ready.send(None);
        return;
    }

    let data = notify_icon_data(hwnd, icon);
    // `data` is fully initialized and remains alive for the Shell_NotifyIconW
    // call that copies the notification icon registration.
    if unsafe { Shell_NotifyIconW(NIM_ADD, &data) } == 0 {
        // hwnd is still owned by this thread, so it can be synchronously
        // destroyed before reclaiming the state passed to CreateWindowExW.
        unsafe { DestroyWindow(hwnd) };
        let _ = unsafe { Box::from_raw(state) };
        eprintln!("[tray] could not add the Windows tray icon, roster disabled");
        let _ = ready.send(None);
        return;
    }

    if ready
        .send(Some(WindowsTrayReady {
            hwnd: hwnd as isize,
            icon: icon as isize,
            attention_icon: attention_icon as isize,
        }))
        .is_err()
    {
        delete_windows_tray_icon(hwnd);
        // hwnd is still owned by this thread and no message loop consumer is
        // waiting now that the caller abandoned the startup handshake.
        unsafe { DestroyWindow(hwnd) };
        // The window is gone, so this is the sole owner of the state Box.
        let _ = unsafe { Box::from_raw(state) };
        return;
    }

    let mut message = MSG::default();
    // GetMessageW writes a complete message into this valid, stack-owned slot.
    while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
        // The successful GetMessageW call initialized `message` for this call.
        unsafe { TranslateMessage(&message) };
        // The dispatched message targets the window owned by this thread.
        unsafe { DispatchMessageW(&message) };
    }

    // The shutdown callback has already removed the icon; this destroys the
    // message-only window before its Box-backed callback state is reclaimed.
    unsafe { DestroyWindow(hwnd) };
    // The window is gone, so this is the sole owner of the state Box.
    let _ = unsafe { Box::from_raw(state) };
}

#[cfg(target_os = "windows")]
pub fn spawn() -> Option<(SharedRoster, TrayRequestQueue, TrayHandle)> {
    let roster: SharedRoster = Arc::new(Mutex::new(Vec::new()));
    let requests: TrayRequestQueue = Arc::new(Mutex::new(Vec::new()));
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::spawn({
        let thread_roster = roster.clone();
        let thread_requests = requests.clone();
        move || run_windows_tray(thread_roster, thread_requests, ready_tx)
    });

    match ready_rx.recv() {
        Ok(Some(ready)) => Some((
            roster.clone(),
            requests,
            TrayHandle {
                hwnd: ready.hwnd,
                thread: Some(thread),
                roster,
                icon: ready.icon,
                attention_icon: ready.attention_icon,
                shown_kind: Mutex::new(TrayIconKind::Normal),
            },
        )),
        Ok(None) | Err(_) => {
            let _ = thread.join();
            None
        }
    }
}

/// Registers the macOS status item, for the process's lifetime (#99).
///
/// Unlike Linux (a D-Bus service on its own thread) and Windows (a pump
/// thread owning a message-only window), this spawns nothing: `NSStatusItem`
/// and its menu are main-thread-only, and this is already called on the
/// main thread from inside `Application::run`'s callback, with AppKit's run
/// loop running under GPUI. A worker thread here would be the one shape
/// AppKit refuses.
///
/// Returns `None` only when there is no main thread to speak for — which
/// cannot happen from that call site, and declines rather than asserting if
/// it ever does.
#[cfg(target_os = "macos")]
pub fn spawn() -> Option<(SharedRoster, TrayRequestQueue, TrayHandle)> {
    use objc2_app_kit::{NSStatusBar, NSVariableStatusItemLength};

    let mtm = objc2_foundation::MainThreadMarker::new()?;
    let roster: SharedRoster = Arc::new(Mutex::new(Vec::new()));
    let requests: TrayRequestQueue = Arc::new(Mutex::new(Vec::new()));

    let status_item =
        unsafe { NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength) };
    let delegate = macos::TrayMenuDelegate::new(mtm, roster.clone(), requests.clone());
    let menu = macos::menu_for(&delegate);
    unsafe { status_item.setMenu(Some(&menu)) };
    macos::apply_icon(&status_item, TrayIconKind::Normal, mtm);

    let handle = TrayHandle {
        status_item,
        _delegate: delegate,
        roster: roster.clone(),
        shown_kind: std::cell::Cell::new(TrayIconKind::Normal),
    };
    Some((roster, requests, handle))
}

/// No StatusNotifierItem host exists on the remaining targets (see the
/// module doc comment and docs/linux-rewrite/PORTABILITY.md). This is not a
/// fallback that pretends to work: those targets continue to decline rather
/// than claiming a tray implementation they do not have.
#[cfg(all(
    not(target_os = "linux"),
    not(target_os = "windows"),
    not(target_os = "macos")
))]
pub fn spawn() -> Option<(SharedRoster, TrayRequestQueue, TrayHandle)> {
    eprintln!("[tray] not implemented on this platform, roster disabled");
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_menu_label_matches_the_linux_format() {
        let entry = TrayRosterEntry {
            path: PathBuf::from("/worktrees/feature"),
            branch: "feature".into(),
            project_name: "Sirio".into(),
            status: AgentStatus::NeedsInput,
            status_changed_at: None,
        };

        assert_eq!(
            roster_menu_label(&entry, Instant::now()),
            "feature — Sirio (needs input)"
        );
    }

    #[test]
    fn format_status_age_uses_seconds_minutes_and_hours_at_boundaries() {
        assert_eq!(format_status_age(std::time::Duration::from_secs(59)), "59s");
        assert_eq!(format_status_age(std::time::Duration::from_secs(60)), "1m");
        assert_eq!(
            format_status_age(std::time::Duration::from_secs(59 * 60)),
            "59m"
        );
        assert_eq!(
            format_status_age(std::time::Duration::from_secs(60 * 60)),
            "1h"
        );
        assert_eq!(format_status_age(std::time::Duration::ZERO), "1s");
    }

    #[test]
    fn roster_menu_label_includes_status_age_when_present() {
        // #187: the age is derived at label time from an absolute instant,
        // so the fixture states *when* the status changed and hands the
        // same `now` to the formatter.
        let now = Instant::now();
        let entry = TrayRosterEntry {
            path: PathBuf::from("/worktrees/feature"),
            branch: "feature/login".into(),
            project_name: "Sirio".into(),
            status: AgentStatus::NeedsInput,
            status_changed_at: now.checked_sub(std::time::Duration::from_secs(4 * 60)),
        };

        assert_eq!(
            roster_menu_label(&entry, now),
            "feature/login — Sirio (needs input · 4m)"
        );
    }

    #[test]
    fn tray_action_labels_are_nonempty_and_distinct() {
        assert!(!OPEN_SIRIO_LABEL.is_empty());
        assert!(!QUIT_SIRIO_LABEL.is_empty());
        assert_ne!(OPEN_SIRIO_LABEL, QUIT_SIRIO_LABEL);
    }

    #[test]
    fn tray_icon_kind_reports_attention_regardless_of_roster_position() {
        let normal = TrayRosterEntry {
            path: PathBuf::from("/worktrees/normal"),
            branch: "normal".into(),
            project_name: "Sirio".into(),
            status: AgentStatus::Running,
            status_changed_at: None,
        };
        let waiting = TrayRosterEntry {
            path: PathBuf::from("/worktrees/waiting"),
            branch: "waiting".into(),
            project_name: "Sirio".into(),
            status: AgentStatus::NeedsInput,
            status_changed_at: None,
        };

        assert_eq!(
            tray_icon_kind(&[normal.clone(), waiting.clone()]),
            TrayIconKind::Attention
        );
        assert_eq!(tray_icon_kind(&[waiting, normal]), TrayIconKind::Attention);
    }

    #[test]
    fn tray_icon_kind_is_normal_for_an_empty_roster() {
        assert_eq!(tray_icon_kind(&[]), TrayIconKind::Normal);
    }

    #[test]
    fn tray_icon_kind_is_normal_when_no_entry_needs_input() {
        let roster = [
            TrayRosterEntry {
                path: PathBuf::from("/worktrees/running"),
                branch: "running".into(),
                project_name: "Sirio".into(),
                status: AgentStatus::Running,
                status_changed_at: None,
            },
            TrayRosterEntry {
                path: PathBuf::from("/worktrees/done"),
                branch: "done".into(),
                project_name: "Sirio".into(),
                status: AgentStatus::Done,
                status_changed_at: None,
            },
        ];

        assert_eq!(tray_icon_kind(&roster), TrayIconKind::Normal);
    }

    #[test]
    fn empty_roster_uses_the_shared_label() {
        assert_eq!(NO_ACTIVE_AGENTS_LABEL, "No active agents");
    }
}
