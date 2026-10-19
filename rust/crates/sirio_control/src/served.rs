//! The methods a Sirio app serves over the control socket. `system.capabilities`
//! answers with `served_methods`, and so does the check that every command the
//! skill names is served by a release build (`tests/skill_is_served.rs`), which
//! reads the list without starting an app.

pub const BROWSER_METHODS: [&str; 11] = [
    "browser.open",
    "browser.navigate",
    "browser.get",
    "browser.screenshot",
    "browser.snapshot",
    "browser.act",
    "browser.wait",
    "browser.eval",
    "browser.console",
    "browser.errors",
    "browser.permission",
];
// F-CTRL-BROWSER-03/04/05/06/F-PER-08: browser.get, browser.wait,
// browser.eval, browser.console, browser.snapshot and browser.permission
// are real, implemented methods (see handle_browser_action) — they must not
// be turned away here as "not implemented" before ever reaching that
// dispatch.
//
// browser.screenshot stays out of this list on purpose: wry 0.56's public
// WebView API (crates/sirio_ui/src/browser.rs's `build_webview`/
// `build_production_webview`) has no pixel-capture entry point on the
// webkitgtk backend, and there is no other dependency in this workspace
// that reaches WebKitGTK's own `webkit_web_view_get_snapshot`. A true
// screenshot needs a new `webkit2gtk` FFI dependency plus a GAsyncResult
// callback bridged back into GPUI — out of this row's file list. See
// F-CTRL-BROWSER-04's row in the wave report for the sketch.
pub const BROWSER_CAPABILITIES: [&str; 9] = [
    "browser.open",
    "browser.navigate",
    "browser.act",
    "browser.get",
    "browser.wait",
    "browser.eval",
    "browser.console",
    "browser.snapshot",
    "browser.permission",
];

/// The browser actions the skill's `not_supported` table names. Sirio V1 does not
/// serve them, so `sirioctl` refuses each one itself, without sending a request.
pub const BROWSER_NOT_SUPPORTED: &[&str] = &[
    "offline",
    "trace",
    "screencast",
    "record",
    "network-route",
    "network-mock",
    "network-intercept",
    "raw-input",
    "download",
    "upload",
    "pdf",
    "viewport",
    "devtools",
    "extensions",
    "bookmarks",
    "history",
    "errors",
    "screenshot",
];

/// Served by debug builds only. A release build leaves these out of
/// `system.capabilities`, and answers them with "unknown method".
const DEBUG_METHODS: [&str; 33] = [
    "surface.change_request.act",
    "surface.open_list.open",
    "surface.changes.confirm",
    "surface.editor.open",
    "surface.editor.open_snapshot",
    "surface.editor.state",
    "surface.editor.type",
    "surface.editor.keys",
    "surface.editor.select",
    "surface.editor.hover",
    "surface.editor.click",
    "surface.editor.context",
    "surface.editor.git_mark",
    "surface.editor.crumb",
    "surface.editor.find",
    "surface.editor.find_step",
    "surface.editor.replace",
    "surface.editor.find_close",
    "surface.editor.drawer",
    "surface.editor.problem",
    "surface.editor.save",
    "surface.editor.conflict",
    "surface.editor.mode",
    "surface.editor.status",
    "surface.editor.pref",
    "surface.host.end_all",
    "surface.settings.act",
    "surface.terminal.open",
    "surface.terminal.restart",
    "surface.terminal.state",
    "surface.update.read",
    "surface.update.act",
    "surface.update.inject",
];

/// The methods the app serves: the release list, plus `DEBUG_METHODS` when
/// `debug` is set. `system.capabilities` passes `cfg!(debug_assertions)`.
pub fn served_methods(debug: bool) -> Vec<&'static str> {
    let mut methods = vec![
        "system.ping",
        "system.capabilities",
        "system.identify",
        "system.quit",
        "project.list",
        "project.add",
        "workspace.list",
        "workspace.create",
        "workspace.select",
        "tray.jump",
        "workspace.current",
        "workspace.close",
        "worktree.set",
        "notify",
        "panel.create",
        "panel.split",
        "panel.list",
        "panel.write",
        "panel.key",
        "panel.read",
        "panel.state",
        "panel.scrollback",
        "panel.wait",
        "panel.focus",
        "panel.show",
        "panel.close",
        "pane.split",
        "pane.focus",
        "pane.close",
        "tab.cycle",
        "tab.select",
        "notification.create",
        "notification.list",
        "notification.clear",
        "session.ref",
        "session.restore",
        "session.transcript",
        "surface.changes.open",
        "surface.changes.read",
        "surface.changes.stage",
        "surface.changes.unstage",
        "surface.changes.discard",
        "surface.changes.stage_all",
        "surface.changes.discard_all",
        "surface.changes.view",
        "surface.changes.dialog",
        "git.branches",
        "surface.settings.open",
        "surface.settings.select",
        "surface.settings.read",
        "surface.change_requests.show",
        "surface.change_requests.read",
        "surface.change_requests.filter",
        "surface.change_requests.search",
        "surface.change_requests.token",
        "surface.change_request.open",
        "surface.change_request.tab",
        "surface.change_request.read",
        "surface.change_request.checkout",
        "surface.change_request.handoff",
        "surface.ci_log.open",
        "surface.ci_log.read",
        "surface.ci_log.view",
        "surface.change_request.reveal",
        "surface.change_request.thread",
        "surface.change_request.open_file",
        "surface.change_request.open_commit",
        "surface.tabs.read",
        "surface.tabs.select",
        "surface.tabs.close",
        "surface.layout.read",
        "surface.file.read",
        "settings.account.add",
        "settings.account.select",
        "surface.chat.open",
        "surface.chat.send",
        "surface.chat.compose",
        "surface.chat.permission",
        "surface.chat.stop",
        "surface.chat.read",
    ];
    methods.extend(BROWSER_CAPABILITIES);
    if debug {
        methods.extend(DEBUG_METHODS);
    }
    methods
}

#[cfg(test)]
mod tests {
    use super::served_methods;

    /// `surface.settings.act` runs a Settings install, which writes the user's
    /// home directory. Only a debug build may serve it, so a release build must
    /// neither list it nor answer it.
    #[test]
    fn settings_act_is_served_by_debug_builds_only() {
        assert!(served_methods(true).contains(&"surface.settings.act"));
        assert!(!served_methods(false).contains(&"surface.settings.act"));
    }
}
