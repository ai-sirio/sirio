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
}

/// `(pane_id, session_id)` for each pane that can be linked with certainty.
///
/// - By title: the pane's title is `OC | <title>`, and exactly one session in the pane's
///   directory has that title; or, among several with it, the one the pane already holds
///   (an older session with a generic title must not break a link that is sure).
/// - By the first turn, while the title is still `OpenCode`: the pane is the only
///   `opencode` pane in its directory, and exactly one session there was created at or
///   after the pane's `opencode` started.
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
            pane.held
                .as_deref()
                .and_then(|held| titled().find(|session| session.id == held))
                .or_else(|| only(titled()))
        } else if pane.title == UNTITLED {
            let alone = panes.iter().filter(|other| other.cwd == pane.cwd).count() == 1;
            if alone {
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
