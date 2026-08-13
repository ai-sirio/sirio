//! Reusable UI primitives, mirroring the role of Zed's `ui` crate.
//!
//! OWNERSHIP: this file declares modules only. Do not add rendering code here
//! and do not edit it from a piece worktree — the integrator owns it.

pub mod changes;
pub mod chat;
pub mod browser;
pub mod composer;
pub mod controls;
pub mod editor;
pub mod file_view;
pub mod project_forms;
pub mod project_identity;
pub mod right_panel;
pub mod row_reorder;
pub mod settings;
pub mod sidebar;
pub mod status_bar;
pub mod tab_bar;
pub mod titlebar;

// P32: visual-bar conformance suite — test-only module, no rendering code.
#[cfg(test)]
mod conformance;
