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

/// Each worktree beside its root as resolved when it was published.
struct Worktrees(Vec<(PathBuf, WorktreeInfo)>);

impl Global for Worktrees {}

/// Replaces the list `lookup` reads. The roots are resolved here, once, rather
/// than on every lookup: the list changes far less often than it is read, and
/// `canonicalize` on a stale mount can stall.
pub fn set_worktrees(worktrees: Vec<WorktreeInfo>, cx: &mut App) {
    let worktrees = worktrees
        .into_iter()
        .map(|worktree| (resolved(&worktree.root), worktree))
        .collect();
    cx.set_global(Worktrees(worktrees));
}

/// The worktree whose root holds `cwd`, the deepest one when roots nest, or
/// `None` when nothing was published or `cwd` is outside every root. It
/// resolves `cwd` on disk, so ask when the cwd changes, never from a render.
pub fn lookup(cwd: &Path, cx: &App) -> Option<WorktreeInfo> {
    let worktrees = cx.try_global::<Worktrees>()?;
    deepest_containing(&resolved(cwd), &worktrees.0).cloned()
}

fn resolved(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Component-wise (`/p/a` does not hold `/p/ab`), on paths already resolved,
/// so a symlinked cwd still finds its root.
fn deepest_containing<'a>(
    cwd: &Path,
    worktrees: &'a [(PathBuf, WorktreeInfo)],
) -> Option<&'a WorktreeInfo> {
    worktrees
        .iter()
        .filter(|(root, _)| cwd.starts_with(root))
        .max_by_key(|(root, _)| root.components().count())
        .map(|(_, worktree)| worktree)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    fn info(root: &str) -> WorktreeInfo {
        WorktreeInfo {
            project_name: "p".to_string(),
            root: PathBuf::from(root),
            is_primary: false,
        }
    }

    fn found(cx: &mut TestAppContext, roots: &[&str], cwd: &str) -> Option<PathBuf> {
        cx.update(|cx| {
            set_worktrees(roots.iter().map(|root| info(root)).collect(), cx);
            lookup(Path::new(cwd), cx).map(|worktree| worktree.root)
        })
    }

    #[gpui::test]
    fn deepest_containing_prefers_the_deepest_root(cx: &mut TestAppContext) {
        let root = found(cx, &["/p", "/p/.wt/a"], "/p/.wt/a/src");
        assert_eq!(root, Some(PathBuf::from("/p/.wt/a")));
    }

    #[gpui::test]
    fn deepest_containing_is_component_wise(cx: &mut TestAppContext) {
        assert_eq!(found(cx, &["/p/a"], "/p/ab"), None);
    }

    #[gpui::test]
    fn deepest_containing_outside_every_root_is_none(cx: &mut TestAppContext) {
        assert_eq!(found(cx, &["/p", "/q"], "/elsewhere/x"), None);
    }

    #[gpui::test]
    fn deepest_containing_the_root_itself_matches(cx: &mut TestAppContext) {
        assert_eq!(found(cx, &["/p"], "/p"), Some(PathBuf::from("/p")));
    }

    #[cfg(unix)]
    #[gpui::test]
    fn a_symlinked_cwd_finds_the_root_it_points_into(cx: &mut TestAppContext) {
        let dir = std::env::temp_dir().join(format!("sirio-worktree-lookup-{}", std::process::id()));
        let root = dir.join("repo");
        std::fs::create_dir_all(root.join("src")).unwrap();
        let link = dir.join("link");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&root, &link).unwrap();
        let found = found(
            cx,
            &[root.to_str().unwrap()],
            link.join("src").to_str().unwrap(),
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(found, Some(root));
    }

    #[gpui::test]
    fn lookup_before_anything_is_published_is_none(cx: &mut TestAppContext) {
        cx.update(|cx| assert_eq!(lookup(Path::new("/p"), cx), None));
    }
}
