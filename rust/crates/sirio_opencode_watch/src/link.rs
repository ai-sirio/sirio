//! Which server session a terminal pane is running (design section 3.6).
//!
//! A pane is linked only when it is sure; ambiguity links nothing, and a pane with no
//! link shows OpenCode's mark and no status. Directories are compared as given: the
//! caller canonicalises both sides.

use std::path::PathBuf;

use crate::OcSession;

/// The title OpenCode's TUI gives its window once a session has one: `OC | <title>`.
pub const TITLED: &str = "OC | ";
/// The title it has until the first turn of a session ends.
const UNTITLED: &str = "OpenCode";

/// One terminal pane whose foreground program is `opencode`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OcPane {
    pub pane_id: String,
    /// The program's cwd, canonical.
    pub cwd: PathBuf,
    /// The pane's last OSC title.
    pub title: String,
    /// Wall-clock milliseconds at which the app learned the program was `opencode`.
    pub started_ms: u64,
    /// The session the pane is linked to now, if any.
    pub held: Option<String>,
    /// The session Sirio created for this pane's launch (`opencode --session <id>`), if any.
    pub created: Option<String>,
}

/// `(pane_id, session_id)` for each pane that can be linked with certainty.
///
/// - By title: the pane's title is `OC | <title>`, and exactly one session in the pane's
///   directory has that title; or, among several with it, the one the pane already holds
///   (an older session with a generic title must not break a link that is sure). A held
///   session that has no title yet keeps the link while its rename is on the way, whatever
///   other session has the pane's title; the next relink after the rename decides by title.
///   The exception is the session Sirio created for the pane (`created`) when it has no title
///   and no event since its creation: an empty session the user never prompted, left behind
///   by an in-TUI switch, would otherwise stay linked forever, so it is not kept.
/// - By the session Sirio created, while the title is still `OpenCode`: the pane's `created`
///   session is in its directory, and no other session there (leaving out those another pane
///   names as its `created`) was created at or after it. Such a session was opened inside the
///   TUI (`/new`) and the pane may be showing it, so nothing is linked. Two tabs in one folder
///   each know their own session, so being alone does not matter here.
/// - By the first turn, while the title is still `OpenCode`, when `created` is unknown or
///   names no session in the directory: the pane is the only `opencode` pane in its
///   directory, and exactly one session there was created at or after the pane's `opencode`
///   started.
pub fn link(panes: &[OcPane], sessions: &[OcSession]) -> Vec<(String, String)> {
    let mut links = Vec::new();
    for pane in panes {
        let here = || {
            sessions
                .iter()
                .filter(move |session| session.directory == pane.cwd)
        };
        let chosen = if let Some(title) = pane.title.strip_prefix(TITLED) {
            let titled = || here().filter(|session| !session.title.is_empty() && session.title == title);
            // The session the pane holds, if it is in the folder: it keeps the link while it
            // has no title yet (the TUI's title can land before the service's rename, and
            // only an older session then has the title) or has the pane's title.
            let held = pane
                .held
                .as_deref()
                .and_then(|held| here().find(|session| session.id == held));
            match held {
                // Sirio's own session with no title and no event since its creation was never
                // prompted: it is not worth keeping against the pane's title.
                Some(held)
                    if held.title.is_empty()
                        && pane.created.as_deref() == Some(held.id.as_str())
                        && held.at_ms <= held.created_ms =>
                {
                    only(titled())
                }
                Some(held) if held.title.is_empty() || held.title == title => Some(held),
                _ => only(titled()),
            }
        } else if pane.title == UNTITLED {
            let own = pane
                .created
                .as_deref()
                .and_then(|created| here().find(|session| session.id == created));
            let alone = panes.iter().filter(|other| other.cwd == pane.cwd).count() == 1;
            if let Some(own) = own {
                let others = panes
                    .iter()
                    .filter(|other| other.pane_id != pane.pane_id)
                    .filter_map(|other| other.created.as_deref());
                let others: Vec<&str> = others.collect();
                let opened_in_the_tui = here().any(|session| {
                    session.id != own.id
                        && !others.contains(&session.id.as_str())
                        && session.created_ms >= own.created_ms
                });
                (!opened_in_the_tui).then_some(own)
            } else if alone {
                only(here().filter(|session| session.created_ms >= pane.started_ms))
            } else {
                None
            }
        } else {
            None
        };
        if let Some(session) = chosen {
            links.push((pane.pane_id.clone(), session.id.clone()));
        }
    }
    links
}

/// The one element of `candidates`, or nothing when there are none or several.
fn only<'a>(mut candidates: impl Iterator<Item = &'a OcSession>) -> Option<&'a OcSession> {
    let first = candidates.next()?;
    candidates.next().is_none().then_some(first)
}
