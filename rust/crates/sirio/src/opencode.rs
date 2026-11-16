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
//!
//! A tab Sirio launched as OpenCode joins the watch from its first title, in its tab's
//! worktree, because the host names no program for it (`note_launched`). A fresh OpenCode 2
//! tab brings the session Sirio created for its launch, which the link prefers to a guess by
//! title; a restored tab has none.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::channel::mpsc::UnboundedSender;
use sirio_activity::{AgentActivityModel, ReportedState};
use sirio_opencode_watch::{
    OcPane, OcSession, OcState, TITLED, Watch, WatchEvent, discover, link,
};
use sirio_terminal::TerminalActivityEvent;

/// How long `opencode --version` may take (it answers in well under a second).
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// Asks the installed `opencode` for its major version and remembers the answer
/// (`sirio_agents::note_installed_major`; `None` is "asked, no answer"). Runs the binary:
/// never on the UI thread. Nothing of its output is logged.
///
/// It runs in `directory` (when that exists): version managers (mise, asdf, a project's
/// `node_modules/.bin` shim) pick the binary by folder, and the pane runs `opencode` in its
/// worktree. The memo stays process-wide, because one installation per machine is what it
/// models.
pub(crate) fn probe_installed_major(directory: &Path) -> Option<u64> {
    let major = run_version_probe(directory);
    sirio_agents::note_installed_major(major);
    major
}

fn run_version_probe(directory: &Path) -> Option<u64> {
    // The unit tests never exec whatever `opencode` the machine has: an unknown version is
    // what they have always had (the v1 plugin is written).
    if cfg!(test) {
        return None;
    }
    let program = sirio_agents::find_executable_on_path("opencode")?;
    let mut command = sirio_env::command(program);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if directory.is_dir() {
        command.current_dir(directory);
    }
    let mut child = command.spawn().ok()?;
    let stdout = child.stdout.take()?;
    // Read on a thread of its own, so a binary that never closes its pipe cannot outlast the
    // timeout below.
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("opencode-version".into())
        .spawn(move || {
            use std::io::Read;
            let mut text = String::new();
            let _ = stdout.take(4096).read_to_string(&mut text);
            let _ = sender.send(text);
        })
        .ok()?;
    let deadline = Instant::now() + VERSION_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                let left = deadline.saturating_duration_since(Instant::now());
                let text = receiver.recv_timeout(left).ok()?;
                return sirio_agents::parse_version_major(&text);
            }
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// The catalog id the host's program resolution gives OpenCode.
const AGENT: &str = "opencode";

/// A pane whose foreground program is `opencode`, or a tab Sirio launched as OpenCode.
struct Running {
    /// Canonical (a launched tab's: its worktree); empty when the host could not read the
    /// program's cwd (nothing links).
    cwd: PathBuf,
    /// Wall-clock milliseconds at which the app learned of this program.
    started_ms: u64,
    /// Whether a `Program` event named the pane's program (a typed `opencode`); a launched
    /// tab's never is.
    named: bool,
    /// The session Sirio created for this launch.
    created: Option<String>,
}

pub(crate) struct OpenCodeStatus {
    /// Where the watch's thread hands over what it has to say, tagged with the watch's
    /// generation; the workspace drains the other end
    /// (`SirioWorkspace::start_opencode_delivery`).
    sink: UnboundedSender<(u64, WatchEvent)>,
    /// The generation of the watch now running: a late word from one that was dropped
    /// (a call in flight when its last pane left) is not this one's.
    generation: u64,
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
    pub(crate) fn new(sink: UnboundedSender<(u64, WatchEvent)>, home: PathBuf) -> Self {
        Self {
            sink,
            generation: 0,
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
                        Some(running) if was_opencode => {
                            running.cwd = cwd;
                            running.named = true;
                        }
                        _ => {
                            self.panes.insert(
                                pane_id.to_string(),
                                Running {
                                    cwd,
                                    started_ms: wall_ms(),
                                    named: true,
                                    created: None,
                                },
                            );
                        }
                    }
                }
            }
            // A reconnect's re-seed, and the app's listener's seed of a hidden tab, send a
            // `Program(None)` for a session that never named a program: a launched tab's
            // `opencode` is still there (`main.rs` keeps the same rule, `program_none_ends_one`).
            TerminalActivityEvent::Program(None)
                if self.panes.get(pane_id).is_some_and(|running| !running.named) =>
            {
                return false;
            }
            TerminalActivityEvent::Program(None)
            | TerminalActivityEvent::ChildExited { .. }
            | TerminalActivityEvent::SessionLost => {
                // The caller has settled the pane's turn and the model clears the record
                // itself (`program_exited`, `program_gone`): only the bookkeeping goes, the
                // title too (the next `opencode` in this pane must not link by it).
                self.forget(pane_id);
            }
            _ => return false,
        }
        self.sync(activity, now)
    }

    /// A pane is gone (closed, or its program left): nothing of it stays, so a pane id
    /// that is used again starts clean.
    pub(crate) fn forget(&mut self, pane_id: &str) {
        self.titles.remove(pane_id);
        self.panes.remove(pane_id);
        self.reported.remove(pane_id);
    }

    /// Whether the watch follows `pane_id`.
    pub(crate) fn tracks(&self, pane_id: &str) -> bool {
        self.panes.contains_key(pane_id)
    }

    /// A pane whose agent is `opencode` but whose program the host does not name: a tab Sirio
    /// launched runs `opencode` as its session's own process (`$SHELL -lc opencode`), and the
    /// host names only a program a shell hands the terminal to. Its folder is the tab's
    /// worktree and its start is now, its first title; the caller's next `sync` links it. A
    /// `Program` event that names the pane later (a typed `opencode` whose title came first)
    /// corrects the folder. `created` is the session Sirio created for a fresh tab's launch,
    /// `None` for a restored pane.
    pub(crate) fn note_launched(
        &mut self,
        pane_id: &str,
        worktree: &Path,
        created: Option<String>,
    ) {
        let cwd = std::fs::canonicalize(worktree).unwrap_or_else(|_| worktree.to_path_buf());
        self.panes.entry(pane_id.to_string()).or_insert(Running {
            cwd,
            started_ms: wall_ms(),
            named: false,
            created,
        });
    }

    /// What the watch of `generation` said. Ignored once no pane runs `opencode`, and when
    /// it is a watch that was dropped.
    pub(crate) fn on_watch(
        &mut self,
        activity: &mut AgentActivityModel,
        generation: u64,
        event: WatchEvent,
        now: Instant,
    ) -> bool {
        if self.watch.is_none() || generation != self.generation {
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
        let gone: Vec<String> = self
            .panes
            .keys()
            .filter(|pane_id| activity.agent_id(pane_id) != Some(AGENT))
            .cloned()
            .collect();
        for pane_id in gone {
            self.forget(&pane_id);
        }
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
            self.generation += 1;
            let generation = self.generation;
            self.watch = Some(Watch::start(
                move || discover(&home, &|name| std::env::var(name).ok()),
                move |event| {
                    let _ = sink.unbounded_send((generation, event));
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
                held: self.reported.get(pane_id).map(|(held, ..)| held.clone()),
                created: running.created.clone(),
            })
            .collect();
        // One keep rule, in `link`: a held session keeps its pane while it has no title yet
        // (the TUI's title can land before the service's rename) or has the pane's title.
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
            // A finish a new link holds is hidden (reported Idle) only where it cannot be the
            // pane's own: a first-turn link (the title is still `OpenCode`: the session is
            // another pane's, or the pane's own would have been seen busy at its link) or a
            // finish older than the pane's `opencode` (a resumed session). A title link to a
            // finish that came after the pane started is the pane's own turn whose rename
            // landed after it ended, and reports as it is. Both clocks are wall milliseconds
            // of this machine.
            let newly = self
                .reported
                .get(&pane_id)
                .is_none_or(|(old_session, ..)| *old_session != held.0);
            let pane_state = self.panes.get(&pane_id);
            let first_turn_link = !self
                .titles
                .get(&pane_id)
                .is_some_and(|title| title.starts_with(TITLED));
            let older_than_pane =
                pane_state.is_some_and(|running| session.at_ms < running.started_ms);
            let shown = if newly
                && matches!(session.state, OcState::Done | OcState::Error)
                && (first_turn_link || older_than_pane)
            {
                OcState::Idle
            } else {
                session.state
            };
            match self.reported.get(&pane_id) {
                Some(already) if *already == held => continue,
                // Another session: its times are not comparable with the old one's, and
                // the tracker drops a report older than the one it holds.
                Some((old_session, ..)) if *old_session != held.0 => {
                    activity.program_report(&pane_id, None, 0, now)
                }
                _ => {}
            }
            activity.program_report(&pane_id, Some(reported(shown)), session.at_ms, now);
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
        // The service's own end of a turn is a finish however short the turn was.
        OcState::Done => ReportedState::Done,
        OcState::Error => ReportedState::Error,
    }
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}
