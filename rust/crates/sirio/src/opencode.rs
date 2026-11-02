//! OpenCode's status for the panes running it.
//!
//! OpenCode v2 sends nothing in the terminal, so its status is read from the service its
//! TUI talks to (`sirio_opencode_watch`, design section 3.6). This module owns the seam:
//! it knows which panes run `opencode` and where, starts the watch while any does and
//! drops it after the last one, links each pane to a session with `sirio_opencode_watch::link`
//! and puts the session's state into the pane's report record.
//!
//! The watch's link and unlink is the only writer of such a pane's record: the host's
//! `ProgramStatus` for a pane whose agent is `opencode` is ignored
//! (`panes::apply_terminal_activity_event`), because OpenCode sends no 7501 and every host
//! status there is a `None`, the re-seed of a reconnect included, which would wipe the record
//! the watch put there. Nothing is logged here: a title, a directory and a session are
//! content.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use futures::channel::mpsc::UnboundedSender;
use sirio_activity::{AgentActivityModel, ReportedState};
use sirio_opencode_watch::{OcPane, OcSession, OcState, Watch, WatchEvent, discover, link};
use sirio_terminal::TerminalActivityEvent;

/// The catalog id the host's program resolution gives OpenCode.
const AGENT: &str = "opencode";

/// A pane whose foreground program is `opencode`.
struct Running {
    /// Canonical; empty when the host could not read the program's cwd (nothing links).
    cwd: PathBuf,
    /// Wall-clock milliseconds at which the app learned of this program.
    started_ms: u64,
}

pub(crate) struct OpenCodeStatus {
    /// Where the watch's thread hands over what it has to say; the workspace drains the
    /// other end (`SirioWorkspace::start_opencode_delivery`).
    sink: UnboundedSender<WatchEvent>,
    home: PathBuf,
    /// The last OSC title of every terminal pane: a title can come before the program
    /// event that names the pane's program.
    titles: HashMap<String, String>,
    panes: BTreeMap<String, Running>,
    /// What the service said last. Empty while it is down.
    sessions: Vec<OcSession>,
    /// For each linked pane: the session, and the state and time its record holds.
    reported: HashMap<String, (String, OcState, u64)>,
    watch: Option<Watch>,
    watched: Vec<PathBuf>,
}

impl OpenCodeStatus {
    pub(crate) fn new(sink: UnboundedSender<WatchEvent>, home: PathBuf) -> Self {
        Self {
            sink,
            home,
            titles: HashMap::new(),
            panes: BTreeMap::new(),
            sessions: Vec::new(),
            reported: HashMap::new(),
            watch: None,
            watched: Vec::new(),
        }
    }

    /// One terminal event of `pane_id`, after the activity model has applied it.
    /// `was_opencode` is whether the pane's agent was `opencode` before it. Returns
    /// whether a pane's record changed.
    pub(crate) fn note_event(
        &mut self,
        activity: &mut AgentActivityModel,
        pane_id: &str,
        event: &TerminalActivityEvent,
        was_opencode: bool,
        now: Instant,
    ) -> bool {
        match event {
            // A title the terminal reported, or the one the host held when the session was
            // attached again (a relaunch, a reconnect): events arrive in order, so the
            // last one is the pane's title.
            TerminalActivityEvent::OscTitle(title) | TerminalActivityEvent::SeededTitle(title) => {
                self.titles.insert(pane_id.to_string(), title.clone());
            }
            TerminalActivityEvent::Program(Some(program)) => {
                if activity.agent_id(pane_id) == Some(AGENT) {
                    let cwd = program
                        .cwd
                        .as_deref()
                        .map(|cwd| std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.into()))
                        .unwrap_or_default();
                    match self.panes.get_mut(pane_id) {
                        // The same program named again (a seed, a reconnect).
                        Some(running) if was_opencode => running.cwd = cwd,
                        _ => {
                            self.panes.insert(
                                pane_id.to_string(),
                                Running {
                                    cwd,
                                    started_ms: wall_ms(),
                                },
                            );
                        }
                    }
                }
            }
            TerminalActivityEvent::Program(None)
            | TerminalActivityEvent::ChildExited { .. }
            | TerminalActivityEvent::SessionLost => {
                // The caller has settled the pane's turn and the model clears the record
                // itself (`program_exited`, `program_gone`): only the bookkeeping goes.
                self.panes.remove(pane_id);
                self.reported.remove(pane_id);
            }
            _ => return false,
        }
        self.sync(activity, now)
    }

    /// What the watch said. Ignored once no pane runs `opencode`.
    pub(crate) fn on_watch(
        &mut self,
        activity: &mut AgentActivityModel,
        event: WatchEvent,
        now: Instant,
    ) -> bool {
        if self.watch.is_none() {
            return false;
        }
        match event {
            WatchEvent::Sessions(sessions) => self.sessions = sessions,
            // Every session is unknown now, so every link goes: the mark stays, no status.
            WatchEvent::Down(_) => self.sessions.clear(),
        }
        self.relink(activity, now)
    }

    /// Brings the watch and the links in line with the panes: prunes the panes that no
    /// longer run `opencode` (a closed pane loses its identity), starts the watch with the
    /// first and drops it after the last, and hands it the folders the panes are in.
    pub(crate) fn sync(&mut self, activity: &mut AgentActivityModel, now: Instant) -> bool {
        if self.panes.is_empty() && self.watch.is_none() {
            return false;
        }
        self.panes
            .retain(|pane_id, _| activity.agent_id(pane_id) == Some(AGENT));
        let mut directories: Vec<PathBuf> = self
            .panes
            .values()
            .map(|running| running.cwd.clone())
            .filter(|cwd| !cwd.as_os_str().is_empty())
            .collect();
        directories.sort();
        directories.dedup();
        if self.panes.is_empty() {
            // Dropped, not stopped: the UI thread does not wait for the thread.
            self.watch = None;
            self.watched.clear();
            self.sessions.clear();
        } else if self.watch.is_none() {
            let sink = self.sink.clone();
            let home = self.home.clone();
            self.watch = Some(Watch::start(
                move || discover(&home, &|name| std::env::var(name).ok()),
                move |event| {
                    let _ = sink.unbounded_send(event);
                },
                directories.clone(),
            ));
            self.watched = directories;
        } else if directories != self.watched {
            if let Some(watch) = &self.watch {
                watch.set_directories(directories.clone());
            }
            self.watched = directories;
        }
        self.relink(activity, now)
    }

    /// Puts each linked pane's session state into its record, and clears the record of a
    /// pane that lost its link. Returns whether any record was written or cleared.
    fn relink(&mut self, activity: &mut AgentActivityModel, now: Instant) -> bool {
        let panes: Vec<OcPane> = self
            .panes
            .iter()
            .filter(|(_, running)| !running.cwd.as_os_str().is_empty())
            .map(|(pane_id, running)| OcPane {
                pane_id: pane_id.clone(),
                cwd: running.cwd.clone(),
                title: self.titles.get(pane_id).cloned().unwrap_or_default(),
                started_ms: running.started_ms,
            })
            .collect();
        let links = link(&panes, &self.sessions);
        let mut changed = false;
        let lost: Vec<String> = self
            .reported
            .keys()
            .filter(|pane_id| !links.iter().any(|(linked, _)| linked == *pane_id))
            .cloned()
            .collect();
        for pane_id in lost {
            self.reported.remove(&pane_id);
            activity.program_report(&pane_id, None, 0, now);
            changed = true;
        }
        for (pane_id, session_id) in links {
            let Some(session) = self.sessions.iter().find(|session| session.id == session_id)
            else {
                continue;
            };
            let held = (session_id, session.state, session.at_ms);
            match self.reported.get(&pane_id) {
                Some(already) if *already == held => continue,
                // Another session: its times are not comparable with the old one's, and
                // the tracker drops a report older than the one it holds.
                Some((old_session, ..)) if *old_session != held.0 => {
                    activity.program_report(&pane_id, None, 0, now)
                }
                _ => {}
            }
            activity.program_report(&pane_id, Some(reported(session.state)), session.at_ms, now);
            self.reported.insert(pane_id, held);
            changed = true;
        }
        changed
    }
}

fn reported(state: OcState) -> ReportedState {
    match state {
        OcState::Busy => ReportedState::Working,
        OcState::Waiting => ReportedState::Blocked,
        OcState::Idle => ReportedState::Idle,
        OcState::Error => ReportedState::Error,
    }
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}
