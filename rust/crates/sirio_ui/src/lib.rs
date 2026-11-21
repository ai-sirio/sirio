//! Reusable UI primitives, mirroring the role of Zed's `ui` crate.
//!
//! OWNERSHIP: this file declares modules only. Do not add rendering code here
//! and do not edit it from a piece worktree — the integrator owns it.

pub mod ansi_log;
pub mod app_platform;
pub mod browser;
// `src/browser.rs` is compiled twice: once as this crate's `browser` module,
// and once as a bare `mod browser;` inside the `browser_*` examples, whose
// crate root is the example itself. A `crate::` path therefore resolves to
// two different roots and breaks the example build — so browser.rs addresses
// shared modules by this crate's own name, which this alias makes valid from
// inside the crate as well.
extern crate self as sirio_ui;

pub mod caret;
pub mod change_request_tab;
pub mod ci_log_tab;
pub mod combobox;
mod change_request_style;
pub(crate) mod ely_ui;
pub mod changes;
pub mod chat;
pub mod composer_prefs;
pub mod controls;
pub mod diff_annotations;
pub mod drawer;
pub mod editor;
pub mod editor_prefs;
pub mod ely;
pub mod file_context_menu;
pub mod file_view;
pub mod branch_card;
pub mod forge_pause;
pub mod forge_source;
pub mod handoff;
mod horizontal_scroll;
// F-CHG-06: the single git-status -> colour resolver. Deliberately its own
// module rather than a helper inside right_panel or changes, because those two
// each having their own copy is the defect it exists to make impossible.
pub mod git_status_style;
pub mod loading;
pub mod mcp_widget;
pub(crate) mod markdown;
pub mod markdown_preview;

/// A Markdown body as the Preview draws it, for the probes (`markdown_selection_probe`,
/// `markdown_scroll_probe`), which cannot reach the private `markdown` module. Reports
/// the sideways frames on stdout as `BLOCK doc <id> <n> <x> <y> <w> <h>`.
#[doc(hidden)]
pub fn markdown_probe_view(id: &'static str, source: &str) -> gpui::AnyElement {
    markdown::probe_view(id, source, false)
}

/// [`markdown_probe_view`] with the whole body selected.
#[doc(hidden)]
pub fn markdown_probe_view_selected(id: &'static str, source: &str) -> gpui::AnyElement {
    markdown::probe_view(id, source, true)
}
pub mod modal;
pub mod motion;
// The Linux XCB-to-Xlib parent fallback the browser and the widget webview share.
// `pub` only because the `browser_*` examples compile `browser.rs` as their own crate.
#[cfg(target_os = "linux")]
#[doc(hidden)]
pub mod native_child;
pub mod orbit;
pub mod outline;
pub mod pane_launcher;
pub mod popup;
pub mod project_forms;
pub mod project_identity;
pub mod project_identity_dialog;
pub mod proposal_tab;
pub mod prompts;
// Views the example probes draw; reaches `pub(crate)` scrollbars. Not application code.
#[doc(hidden)]
pub mod probes;
pub mod right_panel;
pub mod row_reorder;
pub mod scroll;
pub mod settings;
pub mod sidebar;
pub mod status;
pub mod status_bar;
pub mod tab_bar;
pub mod text_selection;
pub mod titlebar;
mod trace_frame;
mod trace_style;
pub mod trace_tab;
pub mod update_card;
pub mod worktree_lookup;
pub mod worktree_picker;

// P32: visual-bar conformance suite — test-only module, no rendering code.
#[cfg(test)]
mod conformance;
