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

/// Opaque handle kept alive for the process's lifetime. `ksni` serves
/// `GetLayout` from an internally cached menu tree that it only rebuilds
/// (and announces via `LayoutUpdated`) inside `Handle::update` -- writing
/// straight into [`SharedRoster`] changes what `Tray::menu()` would
/// *return*, but never nudges ksni into calling it again on its own, so
/// [`TrayHandle::nudge`] is what actually makes a roster change visible to
/// a D-Bus client. Confirmed live: a bare `SharedRoster` write left
/// `GetLayout` answering a stale "No active agents" indefinitely; adding
/// the `update(|_| {})` call below is what made it pick up the change.
pub struct TrayHandle(ksni::blocking::Handle<AgentRosterTray>);

impl TrayHandle {
    pub fn nudge(&self) {
        let _ = self.0.update(|_tray| {});
    }
}

struct AgentRosterTray {
    roster: SharedRoster,
    requests: TrayRequestQueue,
}

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
        let entries = self.roster.lock().map(|roster| roster.clone()).unwrap_or_default();
        let mut items: Vec<MenuItem<Self>> = Vec::new();
        if entries.is_empty() {
            items.push(
                StandardItem {
                    label: "No active agents".into(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        } else {
            for entry in entries {
                let path = entry.path.clone();
                let label = format!(
                    "{} — {} ({})",
                    entry.branch,
                    entry.project_name,
                    entry.status.human_label()
                );
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
