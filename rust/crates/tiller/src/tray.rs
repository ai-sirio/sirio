//! F-USE-04 / F-USE-05 / F-WIN-08: a StatusNotifierItem tray icon -- the
//! Linux equivalent of the Swift `MenuBarExtra` + `AgentRosterView` pair
//! (`App/TillerApp.swift:115`, `App/AgentRosterView.swift` in the Swift
//! tree). COSMIC's panel (and GNOME/KDE, via their SNI/AppIndicator
//! bridges) render this with no extra install step. `ksni` wraps the
//! freedesktop D-Bus protocol; its "blocking" feature runs the whole D-Bus
//! service on its own OS thread, so nothing here needs an async runtime
//! wired through GPUI's own executor.
//!
//! ksni's D-Bus thread cannot reach GPUI's `Context<TillerWorkspace>`
//! directly, so this module only ever talks to `main.rs` through two
//! `Arc<Mutex<..>>` values -- the same bridge shape `main.rs` already uses
//! for `pending_actions`/`control_actions`: [`SharedRoster`] is written by
//! the workspace and read by the tray's `menu()`; [`TrayRequestQueue`] is
//! written by menu clicks and drained by the workspace's existing 40ms
//! poll loop.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NOTIFYICONDATAW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, GetWindowLongPtrW, LoadIconW, PostMessageW,
    PostQuitMessage, RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, TrackPopupMenu,
    TranslateMessage, CREATESTRUCTW, GWLP_USERDATA, HWND_MESSAGE, IDI_APPLICATION, MF_DISABLED,
    MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP,
    WM_CONTEXTMENU, WM_LBUTTONUP, WM_NCCREATE, WM_NCDESTROY, WM_RBUTTONUP, WNDCLASSW, WS_POPUP,
};

use tiller_activity::AgentStatus;

/// One roster row: a worktree with at least one pane reporting a live
/// [`AgentStatus`]. Mirrors Swift's `activeAgentWorktrees` filter
/// (`App/AppModel.swift:157`).
#[derive(Clone, PartialEq)]
pub struct TrayRosterEntry {
    pub path: PathBuf,
    pub branch: String,
    pub project_name: String,
    pub status: AgentStatus,
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
    /// "Quit Tiller".
    Quit,
}

pub type SharedRoster = Arc<Mutex<Vec<TrayRosterEntry>>>;
pub type TrayRequestQueue = Arc<Mutex<Vec<TrayRequest>>>;

const NO_ACTIVE_AGENTS_LABEL: &str = "No active agents";

fn roster_menu_label(entry: &TrayRosterEntry) -> String {
    format!(
        "{} — {} ({})",
        entry.branch,
        entry.project_name,
        entry.status.human_label()
    )
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
}

#[cfg(target_os = "windows")]
impl TrayHandle {
    /// Windows constructs this menu fresh at every click, unlike ksni, which
    /// serves `GetLayout` from an internally cached tree and needs poking, so
    /// a [`SharedRoster`] write is visible with no further action.
    pub fn nudge(&self) {}
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

#[cfg(all(not(target_os = "linux"), not(target_os = "windows")))]
pub struct TrayHandle;

#[cfg(all(not(target_os = "linux"), not(target_os = "windows")))]
impl TrayHandle {
    pub fn nudge(&self) {}
}

#[cfg(target_os = "linux")]
struct AgentRosterTray {
    roster: SharedRoster,
    requests: TrayRequestQueue,
}

#[cfg(target_os = "linux")]
impl ksni::Tray for AgentRosterTray {
    fn id(&self) -> String {
        "tiller".into()
    }

    fn title(&self) -> String {
        "Tiller".into()
    }

    fn icon_name(&self) -> String {
        "utilities-terminal".into()
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
                let label = roster_menu_label(&entry);
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
        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Quit Tiller".into(),
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
const WINDOWS_TRAY_QUIT_COMMAND: u32 = 0x7fff;

#[cfg(target_os = "windows")]
struct WindowsTrayState {
    roster: SharedRoster,
    requests: TrayRequestQueue,
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
        let tip = wide("Tiller");
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
        let _ = unsafe {
            AppendMenuW(menu, MF_STRING | MF_DISABLED | MF_GRAYED, 0, label.as_ptr())
        };
    } else {
        for (index, entry) in entries.iter().enumerate() {
            let label = wide(&roster_menu_label(entry));
            let command = WINDOWS_TRAY_FIRST_ROSTER_COMMAND + index as u32;
            // `label` remains allocated while AppendMenuW copies its text.
            let _ = unsafe { AppendMenuW(menu, MF_STRING, command as usize, label.as_ptr()) };
        }
    }

    // A null item string is the documented separator form of AppendMenuW.
    let _ = unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null()) };
    let quit_label = wide("Quit Tiller");
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
    ready: std::sync::mpsc::SyncSender<Option<isize>>,
) {
    let class_name = wide("TillerTrayWindowClass");
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

    // Resource ID 1 is the same embedded app-icon.ico resource used by gpui.
    let mut icon = unsafe { LoadIconW(instance, 1usize as *const u16) };
    if icon.is_null() {
        // A system application icon keeps the tray alive when the embedded
        // resource is unavailable in an unusual build.
        icon = unsafe { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) };
    }
    if icon.is_null() {
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

    if ready.send(Some(hwnd as isize)).is_err() {
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
        Ok(Some(hwnd)) => Some((
            roster,
            requests,
            TrayHandle {
                hwnd,
                thread: Some(thread),
            },
        )),
        Ok(None) | Err(_) => {
            let _ = thread.join();
            None
        }
    }
}

/// No StatusNotifierItem host exists on non-Windows, non-Linux targets (see
/// the module doc comment and docs/linux-rewrite/PORTABILITY.md). This is not
/// a fallback that pretends to work: those targets continue to decline rather
/// than claiming a tray implementation they do not have.
#[cfg(all(not(target_os = "linux"), not(target_os = "windows")))]
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
            project_name: "Tiller".into(),
            status: AgentStatus::NeedsInput,
        };

        assert_eq!(roster_menu_label(&entry), "feature — Tiller (needs input)");
    }

    #[test]
    fn empty_roster_uses_the_shared_label() {
        assert_eq!(NO_ACTIVE_AGENTS_LABEL, "No active agents");
    }
}
