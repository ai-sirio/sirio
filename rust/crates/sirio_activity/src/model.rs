//! The layered agent-activity state machine, ported from
//! `TillerCore/AgentActivityModel.swift`.
//!
//! Whether a pane shows as running / needs-input / done / error is resolved
//! from four independent evidence layers, weakest overridden by strongest
//! as it arrives, and the agent's own reports above all of them:
//!
//! - **The agent's reports (OSC 7501).** A pane holding a report record shows
//!   what the turn tracker (`report::TurnTracker`) makes of it, ahead of Layer E and
//!   A–D; see [`AgentActivityModel::program_report`].
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
//! change), **program-owned** (the host named the pane's foreground program —
//! cleared only by [`AgentActivityModel::program_gone`]: title and process
//! evidence never set or clear it).
//!
//! The crate decides, it does not go looking: feed it events (a hook push,
//! a new title, a content match, a process list) and read the resolved
//! status. Process enumeration and PTY reading belong to the caller.

use std::collections::{HashMap, HashSet};
use std::io;
use std::time::{Duration, Instant};

use crate::report::{ReportedState, TITLE_GRACE, TurnTracker};
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
    /// Panes whose identity came from the host naming their foreground program.
    program_owned_panes: HashSet<String>,
    /// Panes whose session has named a program at least once. Set by the first
    /// `Program(Some(_))` the pane receives and kept across `Program(None)`:
    /// the session still names programs, so Layer D no longer polls the pane.
    program_session_panes: HashSet<String>,
    /// What the program on the pane reports over OSC 7501, one turn
    /// tracker per pane holding a live record. Kept per pane, apart from identity:
    /// a report can arrive before the program is named. While a pane holds one
    /// (and is an agent pane) it decides the pane's status ahead of every layer.
    reports: HashMap<String, TurnTracker>,
    /// When the host named each program-owned pane's agent: a title and the screen
    /// set no status for [`TITLE_GRACE`] after it.
    program_identified_at: HashMap<String, Instant>,
    /// Windows only: program-owned panes in a session that never names a program
    /// whose agent process Layer D has seen. Its disappearing is the one signal
    /// there that the agent left.
    process_seen: HashSet<String>,
    /// Panes whose resolved status went to nothing since the last
    /// [`Self::take_cleared_panes`]: a cleared status queues no transition, yet a
    /// pane that was Running must stop reading as running.
    cleared_panes: Vec<String>,
    /// Every change of a pane's resolved status, in order and uncoalesced,
    /// until [`Self::take_resolved_transitions`] drains it.
    resolved_transitions: Vec<Transition>,
}

impl AgentActivityModel {
    /// An empty model.
    pub fn new() -> Self {
        Self::default()
    }

    /// Acts only when the pane's resolved status changes: records the
    /// timestamp and queues the resolved transition for
    /// [`Self::take_resolved_transitions`]. Signals can reassert the same
    /// status frequently, but those are not meaningful transitions for an age
    /// displayed to users or for a stop.
    fn track_status_change(&mut self, pane_id: &str, old: Option<AgentStatus>, now: Instant) {
        let new = self.resolved(pane_id);
        if old == new {
            return;
        }
        if let Some(new) = new {
            self.status_changed_at.insert(pane_id.to_string(), now);
            self.resolved_transitions.push(Transition {
                pane_id: pane_id.to_string(),
                old,
                new,
            });
        } else {
            self.status_changed_at.remove(pane_id);
            self.cleared_panes.push(pane_id.to_string());
        }
    }

    /// Drains the panes whose resolved status went to nothing since the last call.
    /// The app refreshes each one's running flag, which no transition carries.
    pub fn take_cleared_panes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.cleared_panes)
    }

    /// Drains the queue of resolved status changes since the last call. Each
    /// entry's `old` and `new` are the pane's resolved status, Layer E
    /// included, so a claim another layer ignored queues nothing. A status
    /// that is cleared has no `Transition` and queues nothing.
    pub fn take_resolved_transitions(&mut self) -> Vec<Transition> {
        std::mem::take(&mut self.resolved_transitions)
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
        // The agent reports its own turns: while its record is live a hook sets no
        // status (the session it carries is the caller's business) and is a non-change.
        if self.record_shows(pane_id) {
            let standing = self.resolved(pane_id);
            return Transition {
                pane_id: pane_id.to_string(),
                old: standing,
                new: standing.unwrap_or(status),
            };
        }
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

    /// A Layer A push reached this pane: only an agent's hooks or `sirioctl notify` send one.
    pub fn has_hook_evidence(&self, pane_id: &str) -> bool {
        self.last_hook_push_at.contains_key(pane_id)
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
        if self.startup_is_silent(pane_id, now) {
            return None;
        }
        // --- Unregistered pane: try to identify the agent from title text ---
        let Some(agent_id) = self.pane_agents.get(pane_id).cloned() else {
            let identified = identify_agent_from_title(title)?;
            // Read before ownership changes what `resolved` consults: a
            // standing Layer E claim is the old status of this change.
            let old = self.resolved(pane_id);
            self.pane_agents
                .insert(pane_id.to_string(), identified.to_string());
            self.title_owned_panes.insert(pane_id.to_string());
            // The host names this session's programs, and the title of an agent that
            // starts can reach the app before the program that is the agent does. The
            // title names the agent; what it says is a startup, and says nothing of a
            // turn: identity only. The next title applies as it always did.
            if self.program_session_panes.contains(pane_id) {
                self.program_identified_at
                    .insert(pane_id.to_string(), Instant::now());
                self.track_status_change(pane_id, old, now);
                return None;
            }
            let status =
                detect_status_from_title(title, identified).unwrap_or(AgentStatus::Running);
            self.agent_status.insert(pane_id.to_string(), status);
            self.track_status_change(pane_id, old, now);
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
            // Nothing a title says clears the identity a report shows: the report's
            // turn end would lose its stop.
            if self.title_owned_panes.contains(pane_id) && !self.record_shows(pane_id) {
                let old = self.resolved(pane_id);
                self.agent_status.remove(pane_id);
                self.program_identified_at.remove(pane_id);
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
        if !self.pane_agents.contains_key(pane_id) || self.startup_is_silent(pane_id, now) {
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
        // Read before ownership changes what `resolved` consults: a
        // standing Layer E claim is the old status of this change.
        let old = self.resolved(pane_id);
        self.pane_agents
            .insert(pane_id.to_string(), agent_id.to_string());
        self.process_owned_panes.insert(pane_id.to_string());
        self.agent_status
            .insert(pane_id.to_string(), AgentStatus::Running);
        self.track_status_change(pane_id, old, Instant::now());
        Some(Transition {
            pane_id: pane_id.to_string(),
            old: None,
            new: AgentStatus::Running,
        })
    }

    /// The foreground agent process disappeared: the pane is back to a
    /// plain shell. Clears process-owned panes only — spawn- and
    /// title-owned panes have their own exit/clearing paths. The pane's report
    /// record goes with its identity.
    pub fn process_gone(&mut self, pane_id: &str) {
        if !self.process_owned_panes.contains(pane_id) {
            return;
        }
        let old = self.resolved(pane_id);
        self.agent_status.remove(pane_id);
        self.pane_agents.remove(pane_id);
        self.process_owned_panes.remove(pane_id);
        // The record is the lost agent's: the next one named here starts with none. A caller
        // that settles a turn does so first (`program_exited`, then `drop_report`).
        self.reports.remove(pane_id);
        self.track_status_change(pane_id, old, Instant::now());
    }

    /// Applies a Layer-D observation to the model. The I/O half
    /// (`inspect_foreground_agent_in`) may run on any thread; this is the
    /// state half and must run wherever the model lives.
    pub fn apply_process_signal(
        &mut self,
        pane_id: &str,
        observation: io::Result<Option<&'static str>>,
    ) -> io::Result<Option<Transition>> {
        match observation {
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
        let observation = crate::process::inspect_foreground_agent_in(snapshot, shell_pid);
        self.apply_process_signal(pane_id, observation)
    }

    // ------------------------------------------------------------------
    // Foreground program named by the host
    // ------------------------------------------------------------------

    /// The pane's session has named a program (even one that is no agent): from
    /// now on the host, not the Layer-D process walk, tells what runs in it.
    /// Idempotent; survives [`Self::program_gone`]. Only a session that names
    /// programs ever calls this, so a tab without job control keeps Layer D.
    pub fn note_program_session(&mut self, pane_id: &str) {
        if !self.program_session_panes.contains(pane_id) {
            self.program_session_panes.insert(pane_id.to_string());
        }
    }

    /// Whether this pane's session has named a program.
    pub fn is_program_session(&self, pane_id: &str) -> bool {
        self.program_session_panes.contains(pane_id)
    }

    /// The pane's foreground program is `agent_id`: identity only, never a
    /// status. The pane is in `pane_agents` (so it is an agent pane) and owned
    /// by the program, leaving title and process ownership. Idempotent: a
    /// reconnect seeds the program the pane already has. Another agent
    /// replacing the standing identity drops the status the old one had.
    /// A pane whose agent Sirio itself registered (a spawn, a restore) keeps
    /// that ownership: its own exit path clears it.
    pub fn program_identified(&mut self, pane_id: &str, agent_id: &'static str) {
        let owned_by_evidence = self.title_owned_panes.contains(pane_id)
            || self.process_owned_panes.contains(pane_id)
            || self.program_owned_panes.contains(pane_id);
        if self.pane_agents.contains_key(pane_id) && !owned_by_evidence {
            return;
        }
        let same = self.pane_agents.get(pane_id).map(String::as_str) == Some(agent_id);
        if same && self.program_owned_panes.contains(pane_id) {
            return;
        }
        // Read before ownership changes what `resolved` consults.
        let old = self.resolved(pane_id);
        // A different agent than the one standing: the record is the old one's.
        let replaced = !same && self.pane_agents.contains_key(pane_id);
        self.program_identified_at
            .insert(pane_id.to_string(), Instant::now());
        self.pane_agents
            .insert(pane_id.to_string(), agent_id.to_string());
        self.title_owned_panes.remove(pane_id);
        self.process_owned_panes.remove(pane_id);
        self.program_owned_panes.insert(pane_id.to_string());
        if !same {
            self.agent_status.remove(pane_id);
            if replaced {
                self.reports.remove(pane_id);
            }
        }
        self.track_status_change(pane_id, old, Instant::now());
    }

    /// The foreground is no longer the agent, or never named one: clears a
    /// program-owned pane's identity and status. Title-, process- and
    /// spawn-owned panes have their own clearing paths and are left alone.
    pub fn program_gone(&mut self, pane_id: &str) {
        if !self.program_owned_panes.contains(pane_id) {
            return;
        }
        let old = self.resolved(pane_id);
        self.agent_status.remove(pane_id);
        self.pane_agents.remove(pane_id);
        self.program_owned_panes.remove(pane_id);
        self.reports.remove(pane_id);
        self.program_identified_at.remove(pane_id);
        self.process_seen.remove(pane_id);
        self.track_status_change(pane_id, old, Instant::now());
    }

    // ------------------------------------------------------------------
    // The program's own reports (OSC 7501)
    // ------------------------------------------------------------------

    /// The program on the pane reported `state`, which the host saw `at_ms`
    /// milliseconds into the session, or cleared its record (`None`). The turn
    /// tracker (`report::TurnTracker`) turns reports into the status the pane shows.
    ///
    /// A report older than the one held is a replay of an event queued across a
    /// reconnect and is ignored; the same report again changes nothing. A clear
    /// always applies and removes the status of every layer below the record too: a
    /// title that read Running under a turn must not come back when the turn is
    /// withdrawn.
    pub fn program_report(
        &mut self,
        pane_id: &str,
        state: Option<ReportedState>,
        at_ms: u64,
        now: Instant,
    ) {
        let old = self.resolved(pane_id);
        match state {
            Some(state) => self
                .reports
                .entry(pane_id.to_string())
                .or_insert_with(TurnTracker::new)
                .report(state, at_ms),
            None => {
                if self.reports.remove(pane_id).is_some() {
                    self.agent_status.remove(pane_id);
                }
            }
        }
        self.track_status_change(pane_id, old, now);
    }

    /// The program left (the shell is the foreground again, or its session ended):
    /// a pane that was Running by its report settles to Done, identity kept. The
    /// caller drains the resulting transition before [`Self::program_gone`], so the
    /// stop is found while the pane is still an agent's.
    pub fn program_exited(&mut self, pane_id: &str, now: Instant) {
        let old = self.resolved(pane_id);
        if let Some(tracker) = self.reports.get_mut(pane_id) {
            tracker.exited();
        }
        self.track_status_change(pane_id, old, now);
    }

    /// Whether the pane holds a report record. Live while it does, even when what
    /// it says shows no status.
    pub fn has_live_report(&self, pane_id: &str) -> bool {
        self.reports.contains_key(pane_id)
    }

    /// The record decides the pane's status: it exists and the pane is an agent
    /// pane. A record with no identity waits, and swallows nothing meanwhile.
    fn record_shows(&self, pane_id: &str) -> bool {
        self.reports.contains_key(pane_id) && self.pane_agents.contains_key(pane_id)
    }

    /// The pane's session ended (its child exited, or the host was lost): the record
    /// belongs to that session and goes, whoever owns the pane. What it last showed
    /// becomes the layered status, so nothing changes yet; the owner's own handling
    /// (a spawn-owned pane's exit code, `program_gone`) follows and now shows. A new
    /// session starts its clock again at zero, which a kept record would treat as a
    /// replay. Call it after [`Self::program_exited`] and the drain.
    pub fn drop_report(&mut self, pane_id: &str, now: Instant) {
        let shows = self.record_shows(pane_id);
        let old = self.resolved(pane_id);
        let Some(tracker) = self.reports.remove(pane_id) else {
            return;
        };
        if shows {
            match tracker.status() {
                Some(status) => {
                    self.agent_status.insert(pane_id.to_string(), status);
                }
                None => {
                    self.agent_status.remove(pane_id);
                }
            }
        }
        self.track_status_change(pane_id, old, now);
    }

    /// Windows only: Layer D saw the agent's process on this pane. See
    /// [`Self::process_lost`].
    pub fn note_process_seen(&mut self, pane_id: &str) {
        if self.program_owned_panes.contains(pane_id) && !self.program_session_panes.contains(pane_id)
        {
            self.process_seen.insert(pane_id.to_string());
        }
    }

    /// Windows only: Layer D sees no agent process on a pane it had seen one on. Names no
    /// foreground program there, so this is the edge that says the agent left; a pane
    /// Layer D never saw one on (a Node agent it cannot see) is not cleared by it.
    pub fn process_lost(&mut self, pane_id: &str) -> bool {
        self.process_seen.remove(pane_id)
            && self.program_owned_panes.contains(pane_id)
            && !self.program_session_panes.contains(pane_id)
    }

    /// A pane's title and screen set no status while its record shows and for
    /// [`TITLE_GRACE`] after the agent is recognised: what a CLI writes while it
    /// starts says nothing about a turn. Covers a program-owned pane, and a
    /// title-owned one in a session that names programs (the title came first).
    fn startup_is_silent(&self, pane_id: &str, now: Instant) -> bool {
        let covered = self.program_owned_panes.contains(pane_id)
            || (self.title_owned_panes.contains(pane_id)
                && self.program_session_panes.contains(pane_id));
        covered
            && (self.record_shows(pane_id)
                || self
                    .program_identified_at
                    .get(pane_id)
                    .is_some_and(|at| now.saturating_duration_since(*at) < TITLE_GRACE))
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
        let changed = if self.is_evidence_owned(pane_id) {
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

    /// Whether a layer other than the surface owns this pane's identity (its
    /// title, its process, its foreground program), so only that layer clears it.
    fn is_evidence_owned(&self, pane_id: &str) -> bool {
        self.title_owned_panes.contains(pane_id)
            || self.process_owned_panes.contains(pane_id)
            || self.program_owned_panes.contains(pane_id)
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
        // The program's own reports come first, `None` included: a record that shows
        // no status is the agent saying nothing is going on. Until the pane is an
        // agent pane they wait, and show once the program is named.
        if self.record_shows(pane_id)
            && let Some(tracker) = self.reports.get(pane_id)
        {
            return tracker.status();
        }
        if !self.is_evidence_owned(pane_id)
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
        self.program_owned_panes.remove(pane_id);
        self.program_session_panes.remove(pane_id);
        self.reports.remove(pane_id);
        self.program_identified_at.remove(pane_id);
        self.process_seen.remove(pane_id);
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

    /// Whether this pane's agent identity came from the host naming its
    /// foreground program.
    pub fn is_program_owned(&self, pane_id: &str) -> bool {
        self.program_owned_panes.contains(pane_id)
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

    /// How many panes have an agent at work right now — each pane once, by
    /// its resolved status, so two panes running the same CLI are two agents
    /// a restart would stop (`running_agent_ids` de-duplicates by agent and
    /// would say one). The update card's warning reads this.
    pub fn running_pane_count(&self) -> usize {
        self.agent_status
            .keys()
            .chain(self.entity_status.keys())
            .chain(self.reports.keys())
            .collect::<HashSet<&String>>()
            .into_iter()
            .filter(|pane_id| self.resolved(pane_id) == Some(AgentStatus::Running))
            .count()
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

    #[test]
    fn apply_process_signal_identifies_and_clears_process_owned_panes() {
        let mut model = AgentActivityModel::new();
        let identified = model
            .apply_process_signal("pane-process", Ok(Some("codex")))
            .expect("process observation should apply");
        assert_eq!(
            identified.map(|transition| transition.new),
            Some(AgentStatus::Running)
        );
        assert!(model.is_process_owned("pane-process"));

        let cleared = model
            .apply_process_signal("pane-process", Ok(None))
            .expect("process disappearance should apply");
        assert_eq!(cleared, None);
        assert!(!model.is_process_owned("pane-process"));
        assert_eq!(model.status("pane-process"), None);
    }

    #[test]
    fn apply_process_signal_not_found_clears_process_owned_panes() {
        let mut model = AgentActivityModel::new();
        model.process_identified("pane-process", "codex");

        let result = model.apply_process_signal(
            "pane-process",
            Err(io::Error::new(io::ErrorKind::NotFound, "process gone")),
        );
        assert!(matches!(result, Ok(None)));
        assert!(!model.is_process_owned("pane-process"));
        assert_eq!(model.status("pane-process"), None);
    }

    #[test]
    fn apply_process_signal_propagates_other_errors_without_mutating_process_owned_panes() {
        let mut model = AgentActivityModel::new();
        model.process_identified("pane-process", "codex");

        let error = model
            .apply_process_signal(
                "pane-process",
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied")),
            )
            .expect_err("non-NotFound errors should be propagated");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(model.is_process_owned("pane-process"));
        assert_eq!(model.status("pane-process"), Some(AgentStatus::Running));
    }

    #[test]
    fn an_empty_model_counts_no_running_pane() {
        assert_eq!(AgentActivityModel::new().running_pane_count(), 0);
    }

    #[test]
    fn two_panes_running_the_same_agent_count_twice() {
        let mut model = AgentActivityModel::new();
        let now = Instant::now();
        model.notify("pane-1", AgentStatus::Running, now);
        model.notify("pane-2", AgentStatus::Running, now);
        assert_eq!(model.running_pane_count(), 2);
    }

    #[test]
    fn only_running_counts() {
        let mut model = AgentActivityModel::new();
        let now = Instant::now();
        model.notify("pane-1", AgentStatus::NeedsInput, now);
        model.notify("pane-2", AgentStatus::Done, now);
        model.notify("pane-3", AgentStatus::Error, now);
        model.notify("pane-4", AgentStatus::Running, now);
        assert_eq!(model.running_pane_count(), 1);
    }

    #[test]
    fn a_chat_known_only_to_its_surface_counts() {
        let mut model = AgentActivityModel::new();
        model.set_entity_status("pane-chat", Some(AgentStatus::Running));
        assert_eq!(model.running_pane_count(), 1);
    }

    #[test]
    fn a_pane_known_to_two_layers_counts_once() {
        let mut model = AgentActivityModel::new();
        model.notify("pane-1", AgentStatus::Running, Instant::now());
        model.set_entity_status("pane-1", Some(AgentStatus::Running));
        assert_eq!(model.running_pane_count(), 1);
    }

    #[test]
    fn the_resolved_status_decides_not_a_stale_layer() {
        let mut model = AgentActivityModel::new();
        model.notify("pane-1", AgentStatus::Running, Instant::now());
        model.set_entity_status("pane-1", Some(AgentStatus::Done));
        assert_eq!(model.running_pane_count(), 0);
    }

    #[test]
    fn a_closed_pane_does_not_count() {
        let mut model = AgentActivityModel::new();
        model.notify("pane-1", AgentStatus::Running, Instant::now());
        model.pane_closed("pane-1");
        assert_eq!(model.running_pane_count(), 0);
    }

    fn statuses(transitions: &[Transition]) -> Vec<(Option<AgentStatus>, AgentStatus)> {
        transitions.iter().map(|t| (t.old, t.new)).collect()
    }

    #[test]
    fn a_surface_claim_queues_its_resolved_transition() {
        let mut model = AgentActivityModel::new();
        model.set_entity_status("pane-1", Some(AgentStatus::Running));
        model.set_entity_status("pane-1", Some(AgentStatus::Done));
        let taken = model.take_resolved_transitions();
        assert_eq!(
            taken,
            vec![
                Transition {
                    pane_id: "pane-1".into(),
                    old: None,
                    new: AgentStatus::Running,
                },
                Transition {
                    pane_id: "pane-1".into(),
                    old: Some(AgentStatus::Running),
                    new: AgentStatus::Done,
                },
            ]
        );
    }

    #[test]
    fn each_write_queues_its_own_transition() {
        let mut model = AgentActivityModel::new();
        let t = Instant::now();
        model.notify("pane-1", AgentStatus::Running, t);
        model.notify(
            "pane-1",
            AgentStatus::NeedsInput,
            t + Duration::from_millis(1),
        );
        model.notify("pane-1", AgentStatus::Running, t + Duration::from_millis(2));
        assert_eq!(
            statuses(&model.take_resolved_transitions()),
            vec![
                (None, AgentStatus::Running),
                (Some(AgentStatus::Running), AgentStatus::NeedsInput),
                (Some(AgentStatus::NeedsInput), AgentStatus::Running),
            ],
            "nothing is coalesced"
        );
    }

    #[test]
    fn reasserting_evidence_queues_nothing() {
        let mut model = AgentActivityModel::new();
        model.set_entity_status("pane-1", Some(AgentStatus::Done));
        model.set_entity_status("pane-1", Some(AgentStatus::Done));
        assert_eq!(model.take_resolved_transitions().len(), 1);
    }

    #[test]
    fn an_ignored_claim_queues_nothing() {
        let mut model = AgentActivityModel::new();
        // A title identifies the pane and so queues its own transition.
        model.handle_title_change("pane-1", ". working", Instant::now());
        model.take_resolved_transitions();
        // A title-owned pane ignores the surface's claim.
        model.set_entity_status("pane-1", Some(AgentStatus::Done));
        assert_eq!(model.take_resolved_transitions(), vec![]);
    }

    #[test]
    fn a_stale_hook_push_queues_nothing() {
        let mut model = AgentActivityModel::new();
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_secs(1);
        model.notify("pane-1", AgentStatus::Running, t1);
        model.notify("pane-1", AgentStatus::Done, t0);
        assert_eq!(
            statuses(&model.take_resolved_transitions()),
            vec![(None, AgentStatus::Running)]
        );
    }

    #[test]
    fn a_cleared_status_queues_nothing() {
        let mut model = AgentActivityModel::new();
        // A process claim identifies the pane and so queues its own transition.
        model.process_identified("pane-1", "claude");
        model.take_resolved_transitions();
        model.process_gone("pane-1");
        assert_eq!(model.take_resolved_transitions(), vec![]);
    }

    #[test]
    fn taking_drains_the_queue() {
        let mut model = AgentActivityModel::new();
        model.set_entity_status("pane-1", Some(AgentStatus::Done));
        assert_eq!(model.take_resolved_transitions().len(), 1);
        assert_eq!(model.take_resolved_transitions(), vec![]);
    }

    #[test]
    fn identifying_a_pane_by_title_queues_the_surface_claim_as_old() {
        let mut model = AgentActivityModel::new();
        model.set_entity_status("pane-1", Some(AgentStatus::Done));
        model.take_resolved_transitions();
        model.handle_title_change("pane-1", "\u{2733} idle", Instant::now());
        assert_eq!(
            statuses(&model.take_resolved_transitions()),
            vec![(Some(AgentStatus::Done), AgentStatus::NeedsInput)]
        );
    }

    #[test]
    fn identifying_a_pane_by_process_queues_the_surface_claim_as_old() {
        let mut model = AgentActivityModel::new();
        model.set_entity_status("pane-1", Some(AgentStatus::Done));
        model.take_resolved_transitions();
        model.process_identified("pane-1", "claude");
        assert_eq!(
            statuses(&model.take_resolved_transitions()),
            vec![(Some(AgentStatus::Done), AgentStatus::Running)]
        );
    }
}
