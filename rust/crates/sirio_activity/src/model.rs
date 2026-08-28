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
use std::time::{Duration, Instant};

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
    status_changed_at: HashMap<String, Instant>,
    /// Layer E — what the live surface entity knows about itself and no
    /// other layer can see: an ACP chat that is mid-stream, a terminal that
    /// failed to spawn or has already reaped its child. Revocable, so it is
    /// kept apart from `agent_status` rather than written into it: when the
    /// surface stops making a claim the pane falls straight back on layers
    /// A–D instead of being stuck at the last thing the surface said.
    entity_status: HashMap<String, AgentStatus>,
    pane_agents: HashMap<String, String>,
    title_owned_panes: HashSet<String>,
    process_owned_panes: HashSet<String>,
}

impl AgentActivityModel {
    /// An empty model.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the timestamp only when the pane's resolved status changes.
    /// Signals can reassert the same status frequently, but those are not
    /// meaningful transitions for an age displayed to users.
    fn track_status_change(&mut self, pane_id: &str, old: Option<AgentStatus>, now: Instant) {
        let new = self.resolved(pane_id);
        if old == new {
            return;
        }
        if new.is_some() {
            self.status_changed_at.insert(pane_id.to_string(), now);
        } else {
            self.status_changed_at.remove(pane_id);
        }
    }

    fn set_agent_status(&mut self, pane_id: &str, status: AgentStatus, now: Instant) {
        let old = self.resolved(pane_id);
        self.agent_status.insert(pane_id.to_string(), status);
        self.track_status_change(pane_id, old, now);
    }

    // ------------------------------------------------------------------
    // Layer A: explicit hook push
    // ------------------------------------------------------------------

    /// Applies a `sirioctl notify` (Layer A) hook signal and records the
    /// wall-clock timestamp so Layer-B signals are debounced. Returns the
    /// transition so the caller can decide on notifications.
    ///
    /// The caller supplies the timestamp, so pushes can arrive out of order:
    /// `sirio_control` serves each socket connection on its own OS thread,
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
        self.set_agent_status(pane_id, status, now);
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
        self.set_agent_status(pane_id, AgentStatus::Running, now);
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
        now: Instant,
    ) -> Option<Transition> {
        let old = self.agent_status.get(pane_id).copied()?;
        let new = AgentStatus::from_exit_code(exit_code);
        self.set_agent_status(pane_id, new, now);
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
            self.set_agent_status(pane_id, status, now);
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
                let old = self.resolved(pane_id);
                self.agent_status.remove(pane_id);
                self.pane_agents.remove(pane_id);
                self.title_owned_panes.remove(pane_id);
                self.track_status_change(pane_id, old, now);
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
        self.set_agent_status(pane_id, status, now);
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
        self.set_agent_status(pane_id, status, now);
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
        self.process_owned_panes.insert(pane_id.to_string());
        self.set_agent_status(pane_id, AgentStatus::Running, Instant::now());
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
        let old = self.resolved(pane_id);
        self.agent_status.remove(pane_id);
        self.pane_agents.remove(pane_id);
        self.process_owned_panes.remove(pane_id);
        self.track_status_change(pane_id, old, Instant::now());
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
        let snapshot = crate::process::take_snapshot()?;
        self.refresh_process_signal_in(&snapshot, pane_id, shell_pid)
    }

    /// The same refresh, against a snapshot the caller already took (#248).
    ///
    /// Layer D runs once per terminal pane. Taking the snapshot outside lets
    /// one tick serve every pane instead of enumerating the machine's whole
    /// process table once per pane — the cost that made an idle minimised
    /// Sirio grow with the number of open terminals.
    pub fn refresh_process_signal_in(
        &mut self,
        snapshot: &crate::process::ProcessSnapshot,
        pane_id: &str,
        shell_pid: u32,
    ) -> io::Result<Option<Transition>> {
        match crate::process::inspect_foreground_agent_in(snapshot, shell_pid) {
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
    // Layer E: live-surface evidence
    // ------------------------------------------------------------------

    /// Records what the pane's own live surface reports about itself —
    /// evidence no other layer can produce: an ACP chat that is mid-stream
    /// or has finished a turn, a terminal that failed to spawn or whose
    /// child has already been reaped.
    ///
    /// This exists so a worktree row has exactly one place to ask "what is
    /// this pane's status". Before it, the app read the model for every
    /// worktree but the selected one, and the live entities directly for
    /// that one — so the same row answered differently depending on whether
    /// it was selected. Feeding the surfaces' own facts in here keeps
    /// F-CORE-ACT-17's "priority result across its pane statuses" a
    /// statement about one map.
    ///
    /// Unlike [`Self::apply_content_signal`] and [`Self::apply_exit_result`],
    /// this is *not* gated on the pane already being a known agent pane. A
    /// surface reporting on itself is first-hand evidence about that pane
    /// whether or not any layer has identified an agent in it — a terminal
    /// that exited non-zero is in an error state, and the Rust app already
    /// draws exactly that on the tab's own status cell. Gating it here would
    /// put the tab cell and the worktree row back into disagreement, which
    /// is the whole defect this layer exists to end. It stays out of the
    /// *identity* queries regardless: [`Self::agent_id_for_panes`] and
    /// [`Self::running_agent_ids`] still only ever name panes with a
    /// registered agent.
    ///
    /// It *is* subject to pane ownership, which is not a formality: a
    /// title-owned or process-owned pane is cleared only by its own layer
    /// (the title ceasing to match, the process disappearing), and the PTY
    /// child-exit path already refuses to rewrite those for exactly that
    /// reason — see `apply_terminal_activity_event`'s `ChildExited` arm.
    /// Letting a surface's exit code through here would have reintroduced
    /// the bug that guard exists to prevent, and did: a claude pane
    /// identified from its OSC title had its Running overwritten with Done
    /// the moment the shell behind it exited. A chat pane is never
    /// title- or process-owned, so its evidence is never affected.
    ///
    /// `None` withdraws the claim; the pane falls back on layers A–D.
    ///
    /// Returns whether the stored evidence changed, so a caller that syncs
    /// on every redraw can skip the redraw it would otherwise cause.
    pub fn set_entity_status(&mut self, pane_id: &str, status: Option<AgentStatus>) -> bool {
        let old = self.resolved(pane_id);
        let changed = if self.title_owned_panes.contains(pane_id)
            || self.process_owned_panes.contains(pane_id)
        {
            self.entity_status.remove(pane_id).is_some()
        } else {
            match status {
                Some(status) => {
                    self.entity_status.insert(pane_id.to_string(), status) != Some(status)
                }
                None => self.entity_status.remove(pane_id).is_some(),
            }
        };
        self.track_status_change(pane_id, old, Instant::now());
        changed
    }

    /// The live surface's own claim about this pane, if it is making one.
    pub fn entity_status(&self, pane_id: &str) -> Option<AgentStatus> {
        self.entity_status.get(pane_id).copied()
    }

    /// The pane's resolved status: Layer E when the surface is making a
    /// claim, the layered A–D status otherwise. Every "what is this pane
    /// doing" question in the app answers with this.
    ///
    /// Ownership is enforced here, at read time, and not only on the write:
    /// the two are ordered by whichever event happened to arrive first, and
    /// a surface that exits *before* the 500 ms Layer-D sweep identifies its
    /// agent would otherwise have staked an error the sweep could no longer
    /// displace. Owned panes are cleared by their own layer, full stop.
    fn resolved(&self, pane_id: &str) -> Option<AgentStatus> {
        if !self.title_owned_panes.contains(pane_id)
            && !self.process_owned_panes.contains(pane_id)
            && let Some(status) = self.entity_status.get(pane_id)
        {
            return Some(*status);
        }
        self.agent_status.get(pane_id).copied()
    }

    // ------------------------------------------------------------------
    // Pane closed
    // ------------------------------------------------------------------

    /// Removes all state for a closed pane. Idempotent.
    pub fn pane_closed(&mut self, pane_id: &str) {
        self.agent_status.remove(pane_id);
        self.entity_status.remove(pane_id);
        self.last_hook_update_at.remove(pane_id);
        self.last_hook_push_at.remove(pane_id);
        self.status_changed_at.remove(pane_id);
        self.pane_agents.remove(pane_id);
        self.title_owned_panes.remove(pane_id);
        self.process_owned_panes.remove(pane_id);
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    /// The pane's current status, if it has one — Layer E first, then the
    /// layered A–D status. This is the query, so it must not be used to
    /// decide what a layer's own write should overwrite; the layer methods
    /// read `agent_status` directly for that.
    pub fn status(&self, pane_id: &str) -> Option<AgentStatus> {
        self.resolved(pane_id)
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
        pane_ids
            .iter()
            .filter_map(|id| self.resolved(id))
            .min_by_key(|status| status.priority())
    }

    /// Age of the same pane whose status [`Self::status_for_panes`] selects.
    /// The caller supplies `now` so this remains deterministic in tests.
    pub fn status_age_for_panes(&self, pane_ids: &[&str], now: Instant) -> Option<Duration> {
        now.checked_duration_since(self.status_changed_at_for_panes(pane_ids)?)
    }

    /// *When* the status [`Self::status_for_panes`] selects last changed,
    /// rather than how long ago (#187).
    ///
    /// Anything that keeps a roster entry around across ticks wants this
    /// one, not [`Self::status_age_for_panes`]: an elapsed duration is
    /// recomputed against a fresh `now` on every read, so a value derived
    /// from it differs every time even when nothing moved -- which silently
    /// defeats any equality check used to detect change. An `Instant` is
    /// stable until the status actually moves.
    pub fn status_changed_at_for_panes(&self, pane_ids: &[&str]) -> Option<Instant> {
        let status = self.status_for_panes(pane_ids)?;
        let pane_id = pane_ids
            .iter()
            .find(|id| self.resolved(id) == Some(status))?;
        self.status_changed_at.get(*pane_id).copied()
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
            if let Some(pane) = pane_ids.iter().find(|id| self.resolved(id) == Some(wanted)) {
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
            .filter(|id| self.resolved(id) == Some(AgentStatus::Running))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// F-CORE-ACT-17 (identity half): "the agent identity query returns the
    /// first matching agent in worktree tab/pane order" -- the sub-clause a
    /// prior live drive (two agents at two *different* priorities) could not
    /// exercise, because a real priority conflict never produces a tie.
    ///
    /// `agent_spawned` sets `.running` unconditionally (see its own doc
    /// comment above), so two spawned agents with no later `notify` call are
    /// a genuine tie at the only priority level present -- exactly the case
    /// `main.rs:5765`'s live `sync_worktree_activity` feeds through this same
    /// `agent_id_for_panes` call on every render. `list_for` (`sirio_control
    /// /src/panel.rs:372`) sorts a worktree's panes by pane-id string before
    /// handing them to this function, so pane-id order stands in for
    /// tab/pane order here, exactly as it does for the live app's real
    /// sequentially-assigned pane ids (`pane-0`, `pane-1`, ...).
    #[test]
    fn agent_id_for_panes_breaks_a_tie_by_pane_order_not_by_agent_identity() {
        let now = Instant::now();

        // pane-90 (claude) sorts before pane-91 (codex): claude must win.
        let mut model = AgentActivityModel::new();
        model.agent_spawned("pane-90", "claude", now);
        model.agent_spawned("pane-91", "codex", now);
        assert_eq!(
            model.agent_id_for_panes(&["pane-90", "pane-91"]),
            Some("claude"),
            "both panes are tied at Running; the earlier pane-id must win"
        );

        // Same two agents, orders swapped via pane id (pane-80 < pane-91):
        // codex now sorts first, so codex must win. If the tie-break were
        // secretly keyed on agent identity (e.g. catalog order, or
        // alphabetical agent id) rather than genuine pane order, this
        // assertion would still see "claude" and fail.
        let mut swapped = AgentActivityModel::new();
        swapped.agent_spawned("pane-80", "codex", now);
        swapped.agent_spawned("pane-91", "claude", now);
        assert_eq!(
            swapped.agent_id_for_panes(&["pane-80", "pane-91"]),
            Some("codex"),
            "swapping which pane-id sorts first must flip the winner"
        );

        // Sanity: a caller that (mis)orders the slice itself controls the
        // outcome too -- `agent_id_for_panes` trusts the order it is given,
        // it does not re-sort. This is the same slice-order contract
        // `main.rs:5755`'s `refs` relies on after `list_for`'s own sort.
        assert_eq!(
            model.agent_id_for_panes(&["pane-91", "pane-90"]),
            Some("codex"),
            "the function must follow the slice order it is handed, not re-derive one"
        );
    }

    #[test]
    fn status_age_for_panes_uses_the_winning_pane_and_supplied_now() {
        let start = Instant::now();
        let mut model = AgentActivityModel::new();
        model.agent_spawned("pane-running", "claude", start);
        model.agent_spawned(
            "pane-waiting",
            "codex",
            start + std::time::Duration::from_secs(10),
        );
        model.notify(
            "pane-waiting",
            AgentStatus::NeedsInput,
            start + std::time::Duration::from_secs(20),
        );

        assert_eq!(
            model.status_age_for_panes(
                &["pane-running", "pane-waiting"],
                start + std::time::Duration::from_secs(35),
            ),
            Some(std::time::Duration::from_secs(15))
        );
    }

    #[test]
    fn status_age_does_not_reset_for_a_reassertion_but_does_for_a_change() {
        let start = Instant::now();
        let mut model = AgentActivityModel::new();
        model.agent_spawned("pane-1", "claude", start);
        model.notify(
            "pane-1",
            AgentStatus::Running,
            start + std::time::Duration::from_secs(5),
        );
        assert_eq!(
            model.status_age_for_panes(&["pane-1"], start + std::time::Duration::from_secs(20),),
            Some(std::time::Duration::from_secs(20))
        );

        model.notify(
            "pane-1",
            AgentStatus::NeedsInput,
            start + std::time::Duration::from_secs(25),
        );
        assert_eq!(
            model.status_age_for_panes(&["pane-1"], start + std::time::Duration::from_secs(40),),
            Some(std::time::Duration::from_secs(15))
        );
    }
}
