//! Layered agent-activity detection for Tiller panes.
//!
//! Decides whether a pane's agent is running, idle, or waiting for input,
//! from four independent evidence layers — weakest overridden by strongest
//! as it arrives. Ported from `TillerCore/AgentActivityModel.swift` and its
//! named pieces (`AgentTitleIdentity`, `AgentTitleStatus`,
//! `AgentSignalMerger`, `ScreenManifest`).
//!
//! Pure Rust, no GPUI, no I/O: the crate decides, it does not go looking.
//! Feed it events (a hook push, a new title, a content match, a process
//! list) and read the resolved status; process enumeration and PTY reading
//! belong to the caller.
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

mod content;
mod model;
mod status;
mod title;

pub use content::detect_content_status;
pub use model::{AgentActivityModel, CATALOG_IDS, identify_agent_from_process_names};
pub use status::{AgentStatus, Transition};
pub use title::{
    TITLE_DEBOUNCE, contains_braille_spinner, detect_status_from_title, identify_agent_from_title,
    should_apply_title_signal,
};
