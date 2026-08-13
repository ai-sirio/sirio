//! The layered agent-activity state machine, ported from
//! `TillerCore/AgentActivityModel.swift`.
//!
//! Whether a pane shows as running / needs-input / done / error is resolved
//! from four independent evidence layers, weakest overridden by strongest
//! as it arrives:
//!
//! - **Layer A — hook pushes.** Agents with native hooks call in with an
//!   explicit status. Authoritative; a recent push suppresses Layer B for a
//!   debounce window ([`crate::title::TITLE_DEBOUNCE`]).
//! - **Layer B — terminal title.** Identity and status read from the OSC
//!   title via each CLI's own convention.
//! - **Layer C — content signal.** Matching live pane scrollback; NOT
//!   debounced against Layer A, because a genuine content match is closer
//!   to ground truth than a stale title.
//! - **Layer D — foreground process.** The pane shell's direct children
//!   matched against the agent catalog. The only signal that catches agents
//!   with no usable title convention; Node/Bun-hosted CLIs are invisible
//!   here and rely on Layer B.
//!
//! The model also owns pane ownership, which decides who may clear a
//! pane's status: **spawn-owned** (we launched it — cleared when the
//! process exits), **title-owned** (cleared only when the title stops
//! matching that agent's conventions), **process-owned** (cleared only by
//! [`AgentActivityModel::process_gone`], never by an unrelated title
//! change).
//!
//! The crate decides, it does not go looking: feed it events (a hook push,
//! a new title, a content match, a process list) and read the resolved
//! status. Process enumeration and PTY reading belong to the caller.

use std::collections::{HashMap, HashSet};
use std::io;
use std::time::Instant;

use crate::notification::NotificationPayload;
use crate::status::{AgentStatus, Transition};
use crate::title::{
    TITLE_DEBOUNCE, detect_status_from_title, identify_agent_from_title, should_apply_title_signal,
};

/// The fixed catalog of supported agent CLIs, in display order — mirrors
/// `AgentCatalog.all` in the Swift app.
pub const CATALOG_IDS: [&str; 5] = ["claude", "codex", "opencode", "pi", "omp"];

/// Pure state machine for agent lifecycle status in one pane population.
///
/// Values are `String` pane ids, opaque to the model (the Swift app uses
/// UUID strings; callers own the format).
#[derive(Debug, Default)]
pub struct AgentActivityModel {
    agent_status: HashMap<String, AgentStatus>,
    last_hook_update_at: HashMap<String, Instant>,
    /// Timestamp of the pane's most recent *accepted* Layer-A push
    /// (`notify`) or spawn. The guard against out-of-order hook delivery:
    /// a push older than this is stale evidence and must not regress the
    /// pane. Distinct from `last_hook_update_at`, which content signals
    /// also advance for the title debounce — a real hook must never be
    /// rejected because of Layer C, and content must never be gated by a
    /// hook timestamp.
    last_hook_push_at: HashMap<String, Instant>,
    pane_agents: HashMap<String, String>,
    title_owned_panes: HashSet<String>,
    process_owned_panes: HashSet<String>,
}

impl AgentActivityModel {
    /// An empty model.
    pub fn new() -> Self {
        Self::default()
    }

    // ------------------------------------------------------------------
    // Layer A: explicit hook push
    // ------------------------------------------------------------------

    /// Applies a `tillerctl notify` (Layer A) hook signal and records the
    /// wall-clock timestamp so Layer-B signals are debounced. Returns the
    /// transition so the caller can decide on notifications.
    ///
    /// The caller supplies the timestamp, so pushes can arrive out of order:
    /// `tiller_control` serves each socket connection on its own OS thread,
    /// and a real agent's lifecycle events fired close together can reach
    /// the model in either order. A push older than the pane's most recent
    /// accepted Layer-A push is stale evidence and is rejected — the status
    /// keeps its standing value, and the returned transition reports that
    /// non-change (`old == new`) so the caller's notification policy stays
    /// silent. The guard compares Layer A against Layer A only: content
    /// signals (Layer C) never feed it, so a real hook still overwrites
    /// content unconditionally, and Layer C is never gated by a hook
    /// timestamp.
    pub fn notify(&mut self, pane_id: &str, status: AgentStatus, now: Instant) -> Transition {
        let old = self.agent_status.get(pane_id).copied();
        if self
            .last_hook_push_at
            .get(pane_id)
            .is_some_and(|last| now < *last)
        {
            return Transition {
                pane_id: pane_id.to_string(),
                old,
                // The standing status, so the caller sees a non-change.
                // (For a pane whose status was cleared since its last
                // accepted push, `old` is None and the pushed status is
                // reported as `new` while still not being applied — a
                // cleared pane is never resurrected by stale evidence.)
                new: old.unwrap_or(status),
            };
        }
        self.agent_status.insert(pane_id.to_string(), status);
        self.last_hook_update_at.insert(pane_id.to_string(), now);
        self.last_hook_push_at.insert(pane_id.to_string(), now);
        Transition {
            pane_id: pane_id.to_string(),
            old,
            new: status,
        }
    }

    // ------------------------------------------------------------------
    // Spawn and restore
    // ------------------------------------------------------------------

    /// Registers a newly spawned agent pane and sets its initial status to
    /// `.running`. Returns nothing — running at spawn time is expected and
    /// never triggers a notification.
    pub fn agent_spawned(&mut self, pane_id: &str, agent_id: &str, now: Instant) {
        self.pane_agents
            .insert(pane_id.to_string(), agent_id.to_string());
        self.agent_status
            .insert(pane_id.to_string(), AgentStatus::Running);
        self.last_hook_update_at.insert(pane_id.to_string(), now);
        // The spawn is Layer-A-relevant (the Swift model says so): it is
        // the baseline any later hook push must be newer than.
        self.last_hook_push_at.insert(pane_id.to_string(), now);
    }

    /// The agent id registered for a pane, or `None` for plain shells.
    pub fn agent_id(&self, pane_id: &str) -> Option<&str> {
        self.pane_agents.get(pane_id).map(String::as_str)
    }

    /// Registers a restored pane's agent identity without claiming a
    /// status. Unlike [`Self::agent_spawned`], does NOT set `.running` — a
    /// restored chat tab may be idle, and the real status arrives later via
    /// [`Self::notify`].
    pub fn register_agent_id(&mut self, pane_id: &str, agent_id: &str) {
        self.pane_agents
            .insert(pane_id.to_string(), agent_id.to_string());
    }

    // ------------------------------------------------------------------
    // Process exit
    // ------------------------------------------------------------------

    /// Maps a process exit code to `.done` (code 0) or `.error` (non-zero).
    /// Returns `None` when the pane is no longer tracked (already cleaned
    /// up by a prior close).
    pub fn apply_exit_result(
        &mut self,
        pane_id: &str,
        exit_code: i32,
        _now: Instant,
    ) -> Option<Transition> {
        let old = self.agent_status.get(pane_id).copied()?;
        let new = AgentStatus::from_exit_code(exit_code);
        self.agent_status.insert(pane_id.to_string(), new);
        Some(Transition {
            pane_id: pane_id.to_string(),
            old: Some(old),
            new,
        })
    }

    // ------------------------------------------------------------------
    // Layer B: title change
    // ------------------------------------------------------------------

    /// Handles a terminal title change (Layer B). For an already-known
    /// pane, derives status from the title text and applies it unless a
    /// more recent Layer-A push is still authoritative. For an unregistered
    /// pane, tries to identify the agent from the title text.
    ///
    /// Returns a transition when the status changed meaningfully, or `None`
    /// when the title carries no recognisable status opinion or a recent
    /// hook signal overrides it.
    pub fn handle_title_change(
        &mut self,
        pane_id: &str,
        title: &str,
        now: Instant,
    ) -> Option<Transition> {
        // --- Unregistered pane: try to identify the agent from title text ---
        let Some(agent_id) = self.pane_agents.get(pane_id).cloned() else {
            let identified = identify_agent_from_title(title)?;
            self.pane_agents
                .insert(pane_id.to_string(), identified.to_string());
            self.title_owned_panes.insert(pane_id.to_string());
            let status =
                detect_status_from_title(title, identified).unwrap_or(AgentStatus::Running);
            self.agent_status.insert(pane_id.to_string(), status);
            return Some(Transition {
                pane_id: pane_id.to_string(),
                old: None,
                new: status,
            });
        };

        // --- Known pane: detect status from title, or clear if unmatched ---
        let Some(status) = detect_status_from_title(title, &agent_id) else {
            // Title no longer matches this agent's conventions at all.
            // Only clear title-owned panes; spawn-owned panes rely on
            // process exit and must survive a transient nil classification.
            if self.title_owned_panes.contains(pane_id) {
                self.agent_status.remove(pane_id);
                self.pane_agents.remove(pane_id);
                self.title_owned_panes.remove(pane_id);
            }
            return None;
        };

        // --- Debounce: skip if a recent Layer-A push is authoritative ---
        if !should_apply_title_signal(
            self.last_hook_update_at.get(pane_id).copied(),
            now,
            TITLE_DEBOUNCE,
        ) {
            return None;
        }

        let old = self.agent_status.get(pane_id).copied();
        self.agent_status.insert(pane_id.to_string(), status);
        Some(Transition {
            pane_id: pane_id.to_string(),
            old,
            new: status,
        })
    }

    // ------------------------------------------------------------------
    // Layer C: content signal
    // ------------------------------------------------------------------

    /// Applies a content-derived (Layer C) status signal — matched against
    /// live pane buffer text by [`crate::content::detect_content_status`].
    /// Unlike Layer B, this is NOT gated by the title debounce: a genuine
    /// content match is closer to ground truth than a title convention and
    /// may correct a stale Layer A/B status. A subsequent real hook
    /// ([`Self::notify`]) still overwrites it unconditionally, so Layer C
    /// can never permanently lock a stale status.
    pub fn apply_content_signal(
        &mut self,
        pane_id: &str,
        status: AgentStatus,
        now: Instant,
    ) -> Option<Transition> {
        if !self.pane_agents.contains_key(pane_id) {
            return None;
        }
        let old = self.agent_status.get(pane_id).copied();
        if old == Some(status) {
            return None;
        }
        self.agent_status.insert(pane_id.to_string(), status);
        self.last_hook_update_at.insert(pane_id.to_string(), now);
        Some(Transition {
            pane_id: pane_id.to_string(),
            old,
            new: status,
        })
    }

    // ------------------------------------------------------------------
    // Layer D: foreground-process identification
    // ------------------------------------------------------------------

    /// Registers a pane whose shell has a live foreground agent process
    /// (Layer D) — the only signal that catches agents which never emit an
    /// OSC title (Codex). Ignores panes that are already registered by any
    /// other layer: process evidence only says "alive", so it must never
    /// downgrade a richer status.
    pub fn process_identified(&mut self, pane_id: &str, agent_id: &str) -> Option<Transition> {
        if self.pane_agents.contains_key(pane_id) {
            return None;
        }
        self.pane_agents
            .insert(pane_id.to_string(), agent_id.to_string());
        self.agent_status
            .insert(pane_id.to_string(), AgentStatus::Running);
        self.process_owned_panes.insert(pane_id.to_string());
        Some(Transition {
            pane_id: pane_id.to_string(),
            old: None,
            new: AgentStatus::Running,
        })
    }

    /// The foreground agent process disappeared: the pane is back to a
    /// plain shell. Clears process-owned panes only — spawn- and
    /// title-owned panes have their own exit/clearing paths.
    pub fn process_gone(&mut self, pane_id: &str) {
        if !self.process_owned_panes.contains(pane_id) {
            return;
        }
        self.agent_status.remove(pane_id);
        self.pane_agents.remove(pane_id);
        self.process_owned_panes.remove(pane_id);
    }

    /// Refreshes Layer-D evidence from a terminal shell PID. A matching
    /// descendant claims an otherwise-unregistered pane as running; an
    /// unmatched or disappeared process tree clears only process-owned state.
    /// Other ownership kinds remain untouched.
    pub fn refresh_process_signal(
        &mut self,
        pane_id: &str,
        shell_pid: u32,
    ) -> io::Result<Option<Transition>> {
        match crate::process::inspect_foreground_agent(shell_pid) {
            Ok(Some(agent_id)) => Ok(self.process_identified(pane_id, agent_id)),
            Ok(None) => {
                self.process_gone(pane_id);
                Ok(None)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.process_gone(pane_id);
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    // ------------------------------------------------------------------
    // Pane closed
    // ------------------------------------------------------------------

    /// Removes all state for a closed pane. Idempotent.
    pub fn pane_closed(&mut self, pane_id: &str) {
        self.agent_status.remove(pane_id);
        self.last_hook_update_at.remove(pane_id);
        self.last_hook_push_at.remove(pane_id);
        self.pane_agents.remove(pane_id);
        self.title_owned_panes.remove(pane_id);
        self.process_owned_panes.remove(pane_id);
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    /// The pane's current status, if it has one.
    pub fn status(&self, pane_id: &str) -> Option<AgentStatus> {
        self.agent_status.get(pane_id).copied()
    }

    /// Whether this pane's agent identity came from title identification.
    pub fn is_title_owned(&self, pane_id: &str) -> bool {
        self.title_owned_panes.contains(pane_id)
    }

    /// Whether this pane's agent identity came from foreground-process
    /// identification.
    pub fn is_process_owned(&self, pane_id: &str) -> bool {
        self.process_owned_panes.contains(pane_id)
    }

    /// Highest-priority agent status among the given pane ids
    /// (error > needs-input > running > done). Returns `None` if no agent
    /// panes are in the collection.
    pub fn status_for_panes(&self, pane_ids: &[&str]) -> Option<AgentStatus> {
        AgentStatus::highest_priority(pane_ids.iter().filter_map(|id| self.agent_status.get(*id)))
    }

    /// Agent id of the most relevant pane among the given pane ids (same
    /// priority order as [`Self::status_for_panes`]). Returns `None` if no
    /// agent panes are in the collection.
    pub fn agent_id_for_panes(&self, pane_ids: &[&str]) -> Option<&str> {
        for wanted in [
            AgentStatus::Error,
            AgentStatus::NeedsInput,
            AgentStatus::Running,
            AgentStatus::Done,
        ] {
            if let Some(pane) = pane_ids
                .iter()
                .find(|id| self.agent_status.get(**id) == Some(&wanted))
            {
                return self.pane_agents.get(*pane).map(String::as_str);
            }
        }
        pane_ids
            .iter()
            .find_map(|id| self.pane_agents.get(*id).map(String::as_str))
    }

    /// Distinct agent ids currently `.running` among the given pane ids,
    /// filtered and ordered by `catalog_ids` for stable left-to-right icon
    /// order in the worktree row's trailing running-agents badge.
    pub fn running_agent_ids<'a>(
        &self,
        pane_ids: &[&str],
        catalog_ids: &[&'a str],
    ) -> Vec<&'a str> {
        let running: HashSet<&str> = pane_ids
            .iter()
            .filter(|id| self.agent_status.get(**id) == Some(&AgentStatus::Running))
            .filter_map(|id| self.pane_agents.get(*id).map(String::as_str))
            .collect();
        catalog_ids
            .iter()
            .copied()
            .filter(|id| running.contains(id))
            .collect()
    }

    /// Builds the platform-neutral notification payload for a tracked pane.
    /// The caller supplies display context because project/worktree ownership
    /// belongs above this crate.
    #[allow(clippy::too_many_arguments)]
    pub fn build_payload(
        &self,
        pane_id: &str,
        status: AgentStatus,
        agent_display_name: &str,
        worktree_id: &str,
        worktree_branch: &str,
        project_name: Option<&str>,
        worktree_comment: Option<&str>,
    ) -> Option<NotificationPayload> {
        if !self.pane_agents.contains_key(pane_id) {
            return None;
        }
        let mut body = worktree_branch.to_string();
        if let Some(project_name) = project_name {
            body.push_str(" · ");
            body.push_str(project_name);
        }
        if let Some(comment) = worktree_comment
            .map(str::trim)
            .filter(|comment| !comment.is_empty())
        {
            body.push_str("  ·  ");
            body.push_str(comment);
        }
        Some(NotificationPayload {
            pane_id: pane_id.to_string(),
            worktree_id: worktree_id.to_string(),
            title: format!("{agent_display_name} — {}", status.human_label()),
            body,
        })
    }
}

/// Layer D pure matcher: the first catalog agent (in catalog order) whose
/// comm name appears among the pane shell's direct child process names.
/// The caller enumerates the processes; this only decides.
pub fn identify_agent_from_process_names<'a>(
    names: &HashSet<String>,
    catalog_ids: &[&'a str],
) -> Option<&'a str> {
    catalog_ids
        .iter()
        .copied()
        .find(|id| names.iter().any(|name| name == id))
}
