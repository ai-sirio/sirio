//! The rows the Files tree draws: single-folder chains joined into one row
//! (`java / it / acme / app / v2`), as Zed does (spec §3.1-§3.3).
//!
//! A row is built from the listings the tree already holds; nothing here
//! reads the disk.

use super::FileNode;
use super::listing::single_visible_dir;
use sirio_git::DirectoryGitStatus;
use std::path::{Path, PathBuf};

/// One drawn row: a plain entry, or a chain of directories that each hold
/// only the next one.
#[derive(Clone, Debug)]
pub(in super::super) struct FileRow {
    /// The chain's paths, outermost first and the leaf last. One entry for a
    /// plain row.
    pub(super) chain: Vec<PathBuf>,
    pub(super) label: String,
    pub(super) depth: usize,
    pub(super) is_dir: bool,
    pub(super) expanded: bool,
    pub(super) git_status: Option<DirectoryGitStatus>,
    pub(super) is_ignored: bool,
    pub(super) read_error: Option<String>,
}

impl FileRow {
    /// The leaf's path: what selection, toggling and the menu act on.
    pub(super) fn path(&self) -> &Path {
        self.chain.last().expect("a row has at least one link")
    }

    /// Whether `path` is one of the chain's links.
    pub(super) fn contains(&self, path: &Path) -> bool {
        self.chain.iter().any(|link| link == path)
    }

    pub(super) fn is_folded(&self) -> bool {
        self.chain.len() > 1
    }
}

/// The rows the tree draws, top to bottom. With `show_hidden` off, entries
/// starting with `.` (and everything under them) are skipped.
pub(super) fn build_rows(tree: &[FileNode], show_hidden: bool) -> Vec<FileRow> {
    let mut rows = Vec::new();
    push_rows(tree, show_hidden, 0, &mut rows);
    rows
}

fn push_rows(nodes: &[FileNode], show_hidden: bool, depth: usize, rows: &mut Vec<FileRow>) {
    for first in nodes
        .iter()
        .filter(|node| show_hidden || !node.name.starts_with('.'))
    {
        let mut chain = vec![first.path.clone()];
        let mut label = first.name.clone();
        let mut leaf = first;
        // Drawn open only when every link is (spec §3.4): the links can
        // disagree after a refresh or a change of the hidden-files setting.
        let mut open = first.expanded;
        while leaf.is_dir
            && leaf.loaded
            && leaf.read_error.is_none()
            && let Some(next) = single_visible_dir(&leaf.children, show_hidden)
        {
            chain.push(next.path.clone());
            label.push_str(" / ");
            label.push_str(&next.name);
            open &= next.expanded;
            leaf = next;
        }
        rows.push(FileRow {
            chain,
            label,
            depth,
            is_dir: leaf.is_dir,
            expanded: open,
            git_status: first.git_status,
            is_ignored: leaf.is_ignored,
            read_error: leaf.read_error.clone(),
        });
        // An open directory whose children are not resolved is waiting for
        // its walk: drawing the held listing first and folding it when the
        // walk lands would make the rows jump.
        if leaf.is_dir && open && leaf.children_resolved {
            push_rows(&leaf.children, show_hidden, depth + 1, rows);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "/repo";

    fn node(name: &str, is_dir: bool, children: Vec<FileNode>) -> FileNode {
        FileNode {
            path: PathBuf::from(name),
            name: name.to_string(),
            is_dir,
            git_status: None,
            is_ignored: false,
            expanded: false,
            read_error: None,
            children,
            loaded: is_dir,
            children_resolved: is_dir,
        }
    }

    fn dir(name: &str, children: Vec<FileNode>) -> FileNode {
        node(name, true, children)
    }

    fn file(name: &str) -> FileNode {
        node(name, false, Vec::new())
    }

    fn open(mut node: FileNode) -> FileNode {
        node.expanded = true;
        node
    }

    fn ignored(mut node: FileNode) -> FileNode {
        node.is_ignored = true;
        node
    }

    fn unloaded(mut node: FileNode) -> FileNode {
        node.loaded = false;
        node
    }

    fn unresolved(mut node: FileNode) -> FileNode {
        node.children_resolved = false;
        node
    }

    fn status(mut node: FileNode, status: DirectoryGitStatus) -> FileNode {
        node.git_status = Some(status);
        node
    }

    fn denied(mut node: FileNode) -> FileNode {
        node.read_error = Some("denied".to_string());
        node.loaded = false;
        node
    }

    /// Loaded, yet carrying an error: only the read_error guard can stop a
    /// chain here.
    fn unreadable(mut node: FileNode) -> FileNode {
        node.read_error = Some("denied".to_string());
        node
    }

    /// Gives every node the path its place in the tree implies, under ROOT.
    fn placed(mut nodes: Vec<FileNode>) -> Vec<FileNode> {
        fn place(nodes: &mut [FileNode], parent: &Path) {
            for node in nodes {
                node.path = parent.join(&node.name);
                place(&mut node.children, &node.path.clone());
            }
        }
        place(&mut nodes, Path::new(ROOT));
        nodes
    }

    fn labels(rows: &[FileRow]) -> Vec<&str> {
        rows.iter().map(|row| row.label.as_str()).collect()
    }

    fn depths(rows: &[FileRow]) -> Vec<usize> {
        rows.iter().map(|row| row.depth).collect()
    }

    #[test]
    fn a_folder_holding_a_file_ends_the_chain_there() {
        let tree = placed(vec![dir(
            "java",
            vec![dir("it", vec![file("A.java"), dir("acme", vec![])])],
        )]);
        assert_eq!(labels(&build_rows(&tree, false)), ["java / it"]);
    }

    #[test]
    fn two_folders_end_the_chain() {
        let tree = placed(vec![dir(
            "a",
            vec![dir("b", vec![dir("c", vec![]), dir("d", vec![])])],
        )]);
        assert_eq!(labels(&build_rows(&tree, false)), ["a / b"]);
    }

    fn dotfile_tree() -> Vec<FileNode> {
        placed(vec![dir(
            "a",
            vec![file(".DS_Store"), dir("b", vec![file("x.txt")])],
        )])
    }

    #[test]
    fn a_dotfile_does_not_break_the_chain_while_hidden_files_are_off() {
        assert_eq!(labels(&build_rows(&dotfile_tree(), false)), ["a / b"]);
    }

    #[test]
    fn a_dotfile_breaks_the_chain_while_hidden_files_are_on() {
        assert_eq!(labels(&build_rows(&dotfile_tree(), true)), ["a"]);
    }

    #[test]
    fn a_lone_dot_directory_folds_only_while_hidden_files_are_on() {
        let tree = placed(vec![dir("a", vec![dir(".cache", vec![file("x")])])]);
        assert_eq!(labels(&build_rows(&tree, false)), ["a"]);
        assert_eq!(labels(&build_rows(&tree, true)), ["a / .cache"]);
    }

    #[test]
    fn a_link_shown_as_leaf_after_turning_hidden_files_on_draws_its_children() {
        let tree = placed(vec![open(dir(
            "a",
            vec![
                dir(".cache", vec![file("x")]),
                open(dir("b", vec![file("y.txt")])),
            ],
        ))]);
        let rows = build_rows(&tree, true);
        assert_eq!(labels(&rows), ["a", ".cache", "b", "y.txt"]);
        assert_eq!(depths(&rows), [0, 1, 1, 2]);
    }

    #[test]
    fn a_git_ignored_sibling_breaks_the_chain() {
        let tree = placed(vec![dir(
            "a",
            vec![dir("b", vec![]), ignored(dir("target", vec![]))],
        )]);
        assert_eq!(labels(&build_rows(&tree, false)), ["a"]);
    }

    #[test]
    fn a_folder_holding_one_file_does_not_fold() {
        let tree = placed(vec![dir("a", vec![file("x.txt")])]);
        assert_eq!(labels(&build_rows(&tree, false)), ["a"]);
    }

    #[test]
    fn an_unknown_listing_does_not_fold() {
        // Not loaded, yet holding a lone folder: only the loaded guard stops
        // the chain at `a`.
        let tree = placed(vec![unloaded(dir("a", vec![dir("b", vec![])]))]);
        assert_eq!(labels(&build_rows(&tree, false)), ["a"]);
        let tree = placed(vec![dir("a", vec![unloaded(dir("b", vec![]))])]);
        assert_eq!(labels(&build_rows(&tree, false)), ["a / b"]);
    }

    #[test]
    fn an_unreadable_link_is_the_leaf_and_carries_the_error() {
        let tree = placed(vec![dir("a", vec![denied(dir("b", vec![]))])]);
        let rows = build_rows(&tree, false);
        assert_eq!(labels(&rows), ["a / b"]);
        assert!(rows[0].read_error.is_some());
        // A listing that came back with an error does not fold onward, even
        // if it still holds a lone folder.
        let tree = placed(vec![dir(
            "a",
            vec![unreadable(dir("b", vec![dir("c", vec![])]))],
        )]);
        let rows = build_rows(&tree, false);
        assert_eq!(labels(&rows), ["a / b"]);
        assert!(rows[0].read_error.is_some());
    }

    #[test]
    fn top_level_folders_fold_like_any_other() {
        let tree = placed(vec![dir("docs", vec![dir("adr", vec![file("0001.md")])])]);
        let rows = build_rows(&tree, false);
        assert_eq!(labels(&rows), ["docs / adr"]);
        assert_eq!(depths(&rows), [0]);
    }

    #[test]
    fn the_leaf_children_sit_one_level_below_the_row() {
        let tree = placed(vec![dir(
            "a",
            vec![open(dir("b", vec![open(dir("c", vec![file("f.txt")]))]))],
        )]);
        let tree = placed(tree.into_iter().map(open).collect());
        let rows = build_rows(&tree, false);
        assert_eq!(labels(&rows), ["a / b / c", "f.txt"]);
        assert_eq!(depths(&rows), [0, 1]);
    }

    #[test]
    fn an_open_leaf_with_unresolved_children_draws_none() {
        let tree = placed(vec![unresolved(open(dir(
            "a",
            vec![dir("b", vec![file("x.txt")]), dir("c", vec![file("y.txt")])],
        )))]);
        assert_eq!(labels(&build_rows(&tree, false)), ["a"]);
    }

    #[test]
    fn the_row_takes_path_state_and_ignore_from_the_leaf() {
        let tree = placed(vec![dir(
            "a",
            vec![open(dir("b", vec![ignored(open(dir("c", vec![])))]))],
        )]);
        let tree = placed(tree.into_iter().map(open).collect());
        let rows = build_rows(&tree, false);
        assert!(rows[0].path().ends_with("a/b/c"));
        assert!(rows[0].expanded);
        assert!(rows[0].is_ignored);
    }

    #[test]
    fn a_row_is_open_only_when_every_link_is_expanded() {
        // The head closed while the leaf is open (a refresh reopened it).
        let tree = placed(vec![dir(
            "w",
            vec![open(dir("c", vec![file("f.txt")]))],
        )]);
        let rows = build_rows(&tree, false);
        assert_eq!(labels(&rows), ["w / c"]);
        assert!(!rows[0].expanded);
        // The mirror: the head open, the leaf closed.
        let tree = placed(vec![open(dir("w", vec![dir("c", vec![file("f.txt")])]))]);
        let rows = build_rows(&tree, false);
        assert_eq!(labels(&rows), ["w / c"]);
        assert!(!rows[0].expanded);
    }

    #[test]
    fn the_row_takes_its_status_from_the_outermost_link() {
        let tree = placed(vec![status(
            dir("a", vec![dir("b", vec![dir("c", vec![file("f")])])]),
            DirectoryGitStatus::Changed,
        )]);
        let rows = build_rows(&tree, false);
        assert_eq!(labels(&rows), ["a / b / c"]);
        assert_eq!(rows[0].git_status, Some(DirectoryGitStatus::Changed));
    }

    #[test]
    fn labels_join_names_outermost_first_with_spaced_slashes() {
        let tree = placed(vec![dir(
            "a",
            vec![dir("b", vec![dir("c", vec![file("f")])])],
        )]);
        let rows = build_rows(&tree, false);
        assert_eq!(rows[0].label, "a / b / c");
        assert_eq!(
            rows[0].chain,
            [
                PathBuf::from("/repo/a"),
                PathBuf::from("/repo/a/b"),
                PathBuf::from("/repo/a/b/c")
            ]
        );
        assert!(rows[0].contains(Path::new("/repo/a/b")));
    }
}
