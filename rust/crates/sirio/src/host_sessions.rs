//! Where an orphaned host session gets a tab (SP2 spec §4.2).
//!
//! Pure helpers: picking the worktree whose path contains a session's cwd
//! (the deepest one), else its project, else the active worktree — and the
//! pane-id floor new panes must start above.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use sirio_control::panel::owning_worktree;
use sirio_host_protocol::term::Origin;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdoptionTarget {
    Worktree(usize),
    Project(usize),
    Active,
}

pub fn adoption_target(
    cwd: &Path,
    worktrees: &[(usize, PathBuf)],
    projects: &[(usize, PathBuf)],
) -> AdoptionTarget {
    let cwd = canonical_or_raw(cwd);
    let mut best: Option<(usize, usize)> = None;
    for (id, path) in worktrees {
        let candidate = canonical_or_raw(path);
        if cwd.starts_with(&candidate) {
            let depth = candidate.components().count();
            if best.is_none_or(|(_, best_depth)| depth > best_depth) {
                best = Some((*id, depth));
            }
        }
    }
    if let Some((id, _)) = best {
        return AdoptionTarget::Worktree(id);
    }
    let mut best: Option<(usize, usize)> = None;
    for (id, path) in projects {
        let candidate = canonical_or_raw(path);
        if cwd.starts_with(&candidate) {
            let depth = candidate.components().count();
            if best.is_none_or(|(_, best_depth)| depth > best_depth) {
                best = Some((*id, depth));
            }
        }
    }
    if let Some((id, _)) = best {
        return AdoptionTarget::Project(id);
    }
    AdoptionTarget::Active
}

/// The app sessions a set of closing worktrees owns (SP2 spec §4.2): every
/// `origin: app` session whose cwd the deepest worktree holding it is one of
/// `closing`, plus every app session a persisted or parked layout names
/// (`layout_ids`, as `pane-<leaf>`). `known` lists every worktree the app
/// knows; the closing ones are counted whether or not it lists them. Control
/// sessions are never returned: `PaneRegistry::shutdown_for` ends those.
pub fn app_sessions_to_end(
    listed: &[(String, Origin, PathBuf)],
    closing: &[PathBuf],
    known: &[PathBuf],
    layout_ids: &HashSet<String>,
) -> Vec<String> {
    let mut candidates = known.to_vec();
    candidates.extend(closing.iter().cloned());
    listed
        .iter()
        .filter(|(_, origin, _)| *origin == Origin::App)
        .filter(|(id, _, cwd)| {
            layout_ids.contains(id)
                || owning_worktree(cwd, &candidates).is_some_and(|owner| closing.contains(owner))
        })
        .map(|(id, _, _)| id.clone())
        .collect()
}

fn canonical_or_raw(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn next_pane_floor(listed: &[String]) -> usize {
    let mut max: Option<usize> = None;
    for id in listed {
        let Some(rest) = id.strip_prefix("pane-") else {
            continue;
        };
        if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(n) = rest.parse::<usize>() else {
            continue;
        };
        max = Some(max.map_or(n, |m: usize| m.max(n)));
    }
    max.map_or(0, |m| m.saturating_add(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf { PathBuf::from(s) }

    #[test]
    fn the_deepest_worktree_containing_the_cwd_wins() {
        let worktrees = [(1, p("/r")), (2, p("/r/wt/feature")), (3, p("/other"))];
        assert_eq!(adoption_target(&p("/r/wt/feature/src"), &worktrees, &[]), AdoptionTarget::Worktree(2));
        assert_eq!(adoption_target(&p("/r/docs"), &worktrees, &[]), AdoptionTarget::Worktree(1));
    }

    #[test]
    fn a_prefix_that_is_not_a_path_component_does_not_match() {
        let worktrees = [(1, p("/r/app"))];
        assert_eq!(adoption_target(&p("/r/application"), &worktrees, &[]), AdoptionTarget::Active);
    }

    #[test]
    fn no_worktree_falls_back_to_the_project_then_the_active_worktree() {
        let projects = [(9, p("/r"))];
        assert_eq!(adoption_target(&p("/r/x"), &[], &projects), AdoptionTarget::Project(9));
        assert_eq!(adoption_target(&p("/elsewhere"), &[], &projects), AdoptionTarget::Active);
    }

    #[test]
    fn a_symlinked_cwd_matches_its_canonical_worktree() {
        let dir = std::env::temp_dir().join(format!("sirio-adopt-{}", std::process::id()));
        let real = dir.join("real");
        std::fs::create_dir_all(real.join("sub")).unwrap();
        let link = dir.join("link");
        let _ = std::fs::remove_file(&link);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).unwrap();
        #[cfg(unix)]
        assert_eq!(adoption_target(&link.join("sub"), &[(4, real.clone())], &[]), AdoptionTarget::Worktree(4));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_pane_floor_is_above_every_app_session_and_ignores_the_rest() {
        let listed = ["pane-3", "pane-17", "pane-c0123456789ab", "pane-x"].map(String::from);
        assert_eq!(next_pane_floor(&listed), 18);
        assert_eq!(next_pane_floor(&[]), 0);
    }

    fn listed(id: &str, origin: Origin, cwd: &str) -> (String, Origin, PathBuf) {
        (id.to_string(), origin, p(cwd))
    }

    #[test]
    fn closing_a_worktree_ends_the_app_sessions_whose_deepest_worktree_it_is() {
        let known = [p("/r"), p("/r/wt/feature")];
        let listed = [
            listed("pane-1", Origin::App, "/r/wt/feature/src"),
            listed("pane-2", Origin::App, "/r/docs"),
            listed("pane-3", Origin::App, "/r/wt/feature"),
        ];
        let ended = app_sessions_to_end(&listed, &[p("/r/wt/feature")], &known, &HashSet::new());
        assert_eq!(ended, ["pane-1", "pane-3"]);
    }

    #[test]
    fn a_control_session_is_never_one_to_end_here() {
        let known = [p("/r/wt")];
        let listed = [listed("c-held", Origin::Control, "/r/wt/sub")];
        let layout = HashSet::from(["c-held".to_string()]);
        assert!(app_sessions_to_end(&listed, &[p("/r/wt")], &known, &layout).is_empty());
    }

    #[test]
    fn a_layout_leaf_counts_even_when_its_session_cwd_is_elsewhere() {
        let known = [p("/r/wt")];
        let listed = [listed("pane-7", Origin::App, "/elsewhere")];
        let layout = HashSet::from(["pane-7".to_string()]);
        assert_eq!(app_sessions_to_end(&listed, &[p("/r/wt")], &known, &layout), ["pane-7"]);
    }

    #[test]
    fn a_sibling_whose_name_starts_with_the_closing_worktree_is_not_owned() {
        let known = [p("/r/app"), p("/r/application")];
        let listed = [listed("pane-4", Origin::App, "/r/application/x")];
        assert!(app_sessions_to_end(&listed, &[p("/r/app")], &known, &HashSet::new()).is_empty());
    }

    #[test]
    fn closing_a_project_ends_the_sessions_of_each_of_its_worktrees() {
        let known = [p("/r/a"), p("/r/b"), p("/r/other")];
        let listed = [
            listed("pane-1", Origin::App, "/r/a"),
            listed("pane-2", Origin::App, "/r/b/src"),
            listed("pane-3", Origin::App, "/r/other"),
        ];
        let closing = [p("/r/a"), p("/r/b")];
        assert_eq!(app_sessions_to_end(&listed, &closing, &known, &HashSet::new()), ["pane-1", "pane-2"]);
    }
}

/// Renames a pane id everywhere a tab's history names it: its root, its split
/// and close events, and the scrollback keyed by it (fix batch 5). The tab's
/// tree is renamed separately, leaf by leaf.
pub fn rename_pane_in_session_state(
    state: &mut crate::session::SessionTabState,
    from: usize,
    to: usize,
) {
    use crate::session::PaneEvent;
    if state.root_id == Some(from) {
        state.root_id = Some(to);
    }
    for event in &mut state.pane_events {
        match event {
            PaneEvent::Split {
                focused, new_id, ..
            } => {
                if *focused == from {
                    *focused = to;
                }
                if *new_id == from {
                    *new_id = to;
                }
            }
            PaneEvent::Close { id } => {
                if *id == from {
                    *id = to;
                }
            }
            PaneEvent::SetRatio { .. } => {}
        }
    }
    if let Some(scrollback) = state.scrollback.remove(&from) {
        state.scrollback.insert(to, scrollback);
    }
}

#[cfg(test)]
mod pane_id_tests {
    use super::*;
    use crate::session::{PaneEvent, SessionTabState};

    #[test]
    fn a_moved_pane_is_renamed_in_its_tab_history() {
        let mut state = SessionTabState {
            root_id: Some(2),
            pane_events: vec![
                PaneEvent::Split {
                    focused: 2,
                    new_id: 3,
                    direction: "right".to_string(),
                },
                PaneEvent::Split {
                    focused: 3,
                    new_id: 4,
                    direction: "down".to_string(),
                },
                PaneEvent::Close { id: 4 },
            ],
            ..SessionTabState::default()
        };
        state.scrollback.insert(2, b"root".to_vec());
        state.scrollback.insert(9, b"other".to_vec());
        rename_pane_in_session_state(&mut state, 2, 7);
        assert_eq!(state.root_id, Some(7));
        assert_eq!(
            state.pane_events[0],
            PaneEvent::Split {
                focused: 7,
                new_id: 3,
                direction: "right".to_string(),
            }
        );
        assert_eq!(state.pane_events[1], PaneEvent::Split {
            focused: 3,
            new_id: 4,
            direction: "down".to_string(),
        });
        assert!(state.scrollback.contains_key(&7));
        assert!(!state.scrollback.contains_key(&2));
        assert!(state.scrollback.contains_key(&9));
    }
}
