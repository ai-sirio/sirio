//! Layered agent-activity detection for Tiller panes.
//!
//! Decides whether a pane's agent is running, idle, or waiting for input,
//! from four independent evidence layers — weakest overridden by strongest
//! as it arrives. Ported from `TillerCore/AgentActivityModel.swift` and its
//! named pieces (`AgentTitleIdentity`, `AgentTitleStatus`,
//! `AgentSignalMerger`, `ScreenManifest`).
//!
//! Pure state-machine logic has no GPUI dependency. The platform boundary in
//! [`process`] supplies Linux `/proc` evidence, while PTY reading and event
//! scheduling remain caller-owned.
//!
//! # Layers
//!
//! - **A — hook pushes** ([`AgentActivityModel::notify`]): authoritative
//!   when present; a recent push suppresses Layer B for a debounce window.
//! - **B — terminal title** ([`AgentActivityModel::handle_title_change`]):
//!   each CLI's own title convention, captured empirically. A bare braille
//!   spinner is ambiguous between Claude and Codex and never assigns
//!   identity.
//! - **C — content signal** ([`AgentActivityModel::apply_content_signal`]):
//!   pane scrollback matching; NOT debounced against Layer A.
//! - **D — foreground process** ([`AgentActivityModel::process_identified`]):
//!   direct-child comm names matched against the catalog; the only signal
//!   that catches agents with no usable title convention.
//!
//! # Ownership
//!
//! Who may clear a pane's status: **spawn-owned** (cleared by process
//! exit), **title-owned** (cleared only when the title stops matching),
//! **process-owned** (cleared only by [`AgentActivityModel::process_gone`],
//! never by an unrelated title change).

mod activity;
mod ansi;
mod bootstrap;
mod content;
mod model;
mod mount;
mod notification;
/// #248: public so the app can take one process snapshot per Layer D tick
/// and walk every pane against it, instead of each pane enumerating the
/// machine's processes for itself.
pub mod process;
mod rows;
mod session;
mod sort;
mod status;
mod title;

pub use activity::ActivityStatus;
pub use ansi::strip_ansi;
pub use bootstrap::{BootstrapRestoreOrder, BootstrapRestoreResult};
pub use content::detect_content_status;
pub use model::{AgentActivityModel, CATALOG_IDS, identify_agent_from_process_names};
pub use mount::WorktreeMountPolicy;
pub use notification::{NotificationPayload, NotificationPolicy};
pub use process::{inspect_foreground_agent, inspect_process_names};
pub use rows::{
    ActivityRow, ActivityRowKind, ActivityTab, ActivityTabKind, ActivityWorktreeInput,
    build_activity_rows,
};
pub use session::{
    AgentSessionRef, AgentSessionRestorePlan, AgentSessionRestoreResult, TerminalContentId,
};
pub use sort::AttentionSort;
pub use status::{AgentStatus, Transition};
pub use title::{
    TITLE_DEBOUNCE, contains_braille_spinner, detect_status_from_title, identify_agent_from_title,
    should_apply_title_signal,
};
