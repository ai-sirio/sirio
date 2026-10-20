//! Which project and worktree a working directory belongs to.
//!
//! The chat tab knows only the directory its agent runs in; the session
//! panel names the project and says whether that is the primary worktree. The
//! host app owns the catalog and publishes this list into a GPUI global each
//! time it rebuilds its control state, so `sirio_ui` never sees the catalog.

use std::path::{Path, PathBuf};

use gpui::{App, Global};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeInfo {
    pub project_name: String,
    pub root: PathBuf,
    pub is_primary: bool,
}

struct Worktrees(Vec<WorktreeInfo>);

impl Global for Worktrees {}

/// Replaces the list `lookup` reads.
pub fn set_worktrees(worktrees: Vec<WorktreeInfo>, cx: &mut App) {
    cx.set_global(Worktrees(worktrees));
}

/// The worktree whose root holds `cwd`, the deepest one when roots nest, or
/// `None` when nothing was published or `cwd` is outside every root.
pub fn lookup(cwd: &Path, cx: &App) -> Option<WorktreeInfo> {
    let worktrees = cx.try_global::<Worktrees>()?;
    deepest_containing(cwd, &worktrees.0).cloned()
}

fn resolved(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Component-wise (`/p/a` does not hold `/p/ab`), after canonicalising both
/// sides when the path exists, so a symlinked cwd still finds its root.
pub(crate) fn deepest_containing<'a>(
    cwd: &Path,
    worktrees: &'a [WorktreeInfo],
) -> Option<&'a WorktreeInfo> {
    let cwd = resolved(cwd);
    worktrees
        .iter()
        .map(|worktree| (resolved(&worktree.root), worktree))
        .filter(|(root, _)| cwd.starts_with(root))
        .max_by_key(|(root, _)| root.components().count())
        .map(|(_, worktree)| worktree)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(root: &str) -> WorktreeInfo {
        WorktreeInfo {
            project_name: "p".to_string(),
            root: PathBuf::from(root),
            is_primary: false,
        }
    }

    #[test]
    fn deepest_containing_prefers_the_deepest_root() {
        let worktrees = [info("/p"), info("/p/.wt/a")];
        let found = deepest_containing(Path::new("/p/.wt/a/src"), &worktrees).unwrap();
        assert_eq!(found.root, PathBuf::from("/p/.wt/a"));
    }

    #[test]
    fn deepest_containing_is_component_wise() {
        let worktrees = [info("/p/a")];
        assert!(deepest_containing(Path::new("/p/ab"), &worktrees).is_none());
    }

    #[test]
    fn deepest_containing_outside_every_root_is_none() {
        let worktrees = [info("/p"), info("/q")];
        assert!(deepest_containing(Path::new("/elsewhere/x"), &worktrees).is_none());
    }

    #[test]
    fn deepest_containing_the_root_itself_matches() {
        let worktrees = [info("/p")];
        let found = deepest_containing(Path::new("/p"), &worktrees).unwrap();
        assert_eq!(found.root, PathBuf::from("/p"));
    }
}
