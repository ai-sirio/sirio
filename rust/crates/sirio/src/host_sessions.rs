//! Where an orphaned host session gets a tab (SP2 spec §4.2).
//!
//! Pure helpers: picking the worktree whose path contains a session's cwd
//! (the deepest one), else its project, else the active worktree — and the
//! pane-id floor new panes must start above.

use std::path::{Path, PathBuf};

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
}
