//! The Environment card: where this chat runs. The project, the directory and
//! the branch, resolved once when the column opens and cached; a render only
//! reads them.

use super::{Row, Section};
use crate::worktree_lookup::WorktreeInfo;
use ely_gpui_component::primitives::IconName;
use std::path::{Path, PathBuf};

/// The directory line's budget, in characters, before its middle is elided.
const DIRECTORY_BUDGET: usize = 34;

/// What the checkout is on. `Unread` until the first read lands and
/// `Unavailable` when it is not a git checkout: both draw no branch line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum Branch {
    #[default]
    Unread,
    Named(String),
    /// A detached HEAD: the short hash, never named a branch.
    Detached(String),
    Unavailable,
}

#[derive(Default)]
pub(super) struct Environment {
    /// The worktree the chat's directory belongs to, `None` when the host
    /// published none or the directory is outside every root.
    pub(super) worktree: Option<WorktreeInfo>,
    pub(super) branch: Branch,
    /// The user's home directory, read once when the column opens, so the
    /// every-frame `section` does no environment work.
    pub(super) home: Option<PathBuf>,
    /// Counts the branch reads started. A card read that began before the
    /// latest one is older than the line it would overwrite.
    pub(super) read_seq: u64,
}

/// Reads the checkout's branch. It spawns `git`, so it runs off the GPUI
/// thread.
pub(super) fn read_branch(path: &Path) -> Branch {
    match sirio_project::current_branch(path) {
        Ok(Some(branch)) => Branch::Named(branch),
        Ok(None) => match sirio_project::read_head_label(path) {
            Some(hash) => Branch::Detached(hash),
            None => Branch::Unavailable,
        },
        Err(_) => Branch::Unavailable,
    }
}

pub(super) fn section(environment: &Environment, cwd: &Path) -> Section {
    let home = environment.home.as_deref();
    let directory = cwd;
    let project = environment
        .worktree
        .as_ref()
        .map(|worktree| worktree.project_name.clone())
        .or_else(|| {
            directory
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    let mut rows = vec![
        Row {
            icon: Some(IconName::Box),
            ..Row::new("environment.project", project, "project")
        },
        Row {
            icon: Some(IconName::Folder),
            ..Row::new("environment.directory", display_path(directory, home), "")
        },
    ];
    let tag = environment
        .worktree
        .as_ref()
        .is_some_and(|worktree| !worktree.is_primary)
        .then_some("worktree")
        .unwrap_or("");
    let branch = match &environment.branch {
        Branch::Named(name) => Some(name.clone()),
        Branch::Detached(hash) => Some(format!("HEAD detached at {hash}")),
        Branch::Unread | Branch::Unavailable => None,
    };
    if let Some(branch) = branch {
        rows.push(Row {
            icon: Some(IconName::GitBranch),
            ..Row::new("environment.branch", branch, tag)
        });
    }
    Section {
        key: "environment",
        title: "Environment",
        summary: String::new(),
        note: String::new(),
        bar: Vec::new(),
        rows,
        custom_body: false,
    }
}

/// `path` relative to `home` when it lies under it, with its middle elided
/// when it is longer than the budget: the first component, then the
/// trailing ones that fit.
fn display_path(path: &Path, home: Option<&Path>) -> String {
    let relative = home
        .filter(|home| !home.as_os_str().is_empty())
        .and_then(|home| path.strip_prefix(home).ok());
    let text = match relative {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.to_string_lossy()),
        None => path.to_string_lossy().into_owned(),
    }
    .replace('\\', "/");
    if text.chars().count() <= DIRECTORY_BUDGET {
        return text;
    }
    let parts: Vec<&str> = text.split('/').collect();
    let head = parts[0];
    let mut kept = 1;
    // `head/…/` plus the tail, as many trailing components as still fit,
    // and always the last one.
    while kept < parts.len() - 1 {
        let tail: usize = parts[parts.len() - kept - 1..]
            .iter()
            .map(|part| part.chars().count() + 1)
            .sum();
        if head.chars().count() + 2 + tail > DIRECTORY_BUDGET {
            break;
        }
        kept += 1;
    }
    if parts.len() <= kept + 1 {
        return text;
    }
    format!("{head}/…/{}", parts[parts.len() - kept..].join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worktree(project: &str, root: &str, is_primary: bool) -> Option<WorktreeInfo> {
        Some(WorktreeInfo {
            project_name: project.into(),
            root: PathBuf::from(root),
            is_primary,
        })
    }

    fn labels(section: &Section) -> Vec<(&str, &str, &str)> {
        section
            .rows
            .iter()
            .map(|row| (row.key.as_str(), row.label.as_str(), row.detail.as_str()))
            .collect()
    }

    #[test]
    fn a_detached_head_says_so_and_is_never_named_a_branch() {
        let environment = Environment {
            worktree: worktree("p", "/p", true),
            branch: Branch::Detached("abc1234".into()),
            home: None,
            read_seq: 0,
        };
        let section = section(&environment, Path::new("/p"));
        let rows = labels(&section);
        assert_eq!(rows[2], ("environment.branch", "HEAD detached at abc1234", ""));
    }

    #[test]
    fn a_directory_outside_every_worktree_shows_the_directory_alone() {
        let environment = Environment {
            worktree: None,
            branch: Branch::Named("main".into()),
            home: None,
            read_seq: 0,
        };
        let section = section(&environment, Path::new("/srv/scratch"));
        let rows = labels(&section);
        assert_eq!(rows[0], ("environment.project", "scratch", "project"));
        assert_eq!(rows[1], ("environment.directory", "/srv/scratch", ""));
        assert_eq!(rows[2], ("environment.branch", "main", ""), "no worktree tag");
    }

    #[test]
    fn a_directory_that_is_not_a_checkout_has_no_branch_line() {
        for branch in [Branch::Unread, Branch::Unavailable] {
            let environment = Environment {
                worktree: None,
                branch,
                home: None,
                read_seq: 0,
            };
            let section = section(&environment, Path::new("/srv/scratch"));
            assert_eq!(section.rows.len(), 2, "project and directory only");
        }
    }

    #[test]
    fn the_directory_is_the_chats_own_not_the_worktree_root() {
        let environment = Environment {
            worktree: worktree("p", "/p/.wt/a", false),
            branch: Branch::Named("feat/x".into()),
            home: None,
            read_seq: 0,
        };
        let section = section(&environment, Path::new("/p/.wt/a/src"));
        let rows = labels(&section);
        assert_eq!(rows[1].1, "/p/.wt/a/src");
        assert_eq!(rows[2], ("environment.branch", "feat/x", "worktree"));
    }

    #[test]
    fn a_path_under_home_is_home_relative_and_a_long_one_loses_its_middle() {
        let home = PathBuf::from("/home/ada");
        assert_eq!(
            display_path(Path::new("/home/ada/work/app"), Some(&home)),
            "~/work/app"
        );
        assert_eq!(display_path(Path::new("/home/ada"), Some(&home)), "~");
        assert_eq!(
            display_path(
                Path::new("/home/ada/Projects/sirio/.herdr/worktrees/clear-river-0342"),
                Some(&home)
            ),
            "~/…/worktrees/clear-river-0342"
        );
        assert_eq!(
            display_path(Path::new("/srv/a/very/long/path/to/some/deep/worktree"), None),
            "/…/long/path/to/some/deep/worktree"
        );
        assert_eq!(display_path(Path::new("/srv/short"), None), "/srv/short");
    }
}
