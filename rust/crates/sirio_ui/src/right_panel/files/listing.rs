//! The listings the Files tree holds, and how they are kept current.
//!
//! The tree shows folded rows (`java / it / acme / app / v2`), and a fold
//! can only be decided from listings that are already in memory: the UI
//! thread never reads the disk. This module is the loader that reads them in
//! the background task, ahead of the user (spec §3.5), and the merge that
//! keeps a refresh from undoing what the user did while it ran (spec §3.6).

use super::{FileNode, GitMarkers, read_tree};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A listing with more child directories than this gets no lookahead, and
/// its rows show unfolded.
pub(super) const LOOKAHEAD_DIR_CAP: usize = 256;

/// The folders macOS guards with a privacy prompt, relative to the home
/// directory. Lookahead never reads one; opening it does, as today.
const PRIVACY_GUARDED: [&str; 7] = [
    "Desktop",
    "Documents",
    "Downloads",
    "Pictures",
    "Movies",
    "Music",
    "Library",
];

/// The single entry of `listing` that a fold continues into: the entries
/// not starting with `.` (every entry when `show_hidden`) are exactly one,
/// and it is a directory.
pub(super) fn single_visible_dir(listing: &[FileNode], show_hidden: bool) -> Option<&FileNode> {
    let mut visible = listing
        .iter()
        .filter(|node| show_hidden || !node.name.starts_with('.'));
    let only = visible.next()?;
    if visible.next().is_some() {
        return None;
    }
    only.is_dir.then_some(only)
}

/// Whether `listing` folds in either toggle state.
pub(super) fn could_fold(listing: &[FileNode]) -> bool {
    single_visible_dir(listing, false).is_some() || single_visible_dir(listing, true).is_some()
}

/// Whether `path` is one of the folders macOS guards under `home`, by path
/// equality. Pure, so it compiles and is tested everywhere; only
/// [`read_with_chains`] decides when to ask, and only on macOS.
pub(super) fn is_privacy_guarded(path: &Path, home: &Path) -> bool {
    path.parent() == Some(home)
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| PRIVACY_GUARDED.contains(&name))
}

/// Reads `directory`, then looks ahead into its listing.
///
/// Looking ahead means reading each child directory's listing (unless the
/// listing holds more than [`LOOKAHEAD_DIR_CAP`] directories, or the child
/// is privacy-guarded) and marking the child `loaded`. A child whose listing
/// [`could_fold`] is a chain link: the lookahead follows it into its own
/// listing, recursively. A child's read failure lands in its `read_error`;
/// only the top-level read is an `Err`.
pub(super) fn read_with_chains(
    root: &Path,
    directory: &Path,
    markers: &GitMarkers,
) -> Result<Vec<FileNode>, String> {
    let home = privacy_home();
    let mut listing = read_tree(root, directory, markers)?;
    look_ahead(root, &mut listing, markers, home.as_deref());
    Ok(listing)
}

/// Reads the whole tree the panel will draw: the root with its lookahead,
/// then every directory in `expanded` (its own listing if the lookahead
/// did not read it, its children's lookahead if that did not run), and so
/// on down the open path. Closed directories are not descended; each held
/// listing is read once per reload.
pub(super) fn reload_tree(
    root: &Path,
    expanded: &HashSet<PathBuf>,
    markers: &GitMarkers,
) -> Result<Vec<FileNode>, String> {
    let home = privacy_home();
    let mut tree = read_tree(root, root, markers)?;
    look_ahead(root, &mut tree, markers, home.as_deref());
    reopen(root, &mut tree, expanded, markers, home.as_deref());
    Ok(tree)
}

/// Puts what the user did while a refresh ran back onto the fresh tree.
///
/// `expanded` comes from `current`, matched by path and kind. Only nodes
/// open in `current` are descended: nothing is carried below a closed
/// directory. The fresh listings win; where fresh did not load a child that
/// `current` had loaded below an open directory, that subtree is kept until
/// the next refresh re-reads it.
pub(super) fn carry_open_state(current: &[FileNode], fresh: Vec<FileNode>) -> Vec<FileNode> {
    carry_nodes(current, fresh, false)
}

/// The home directory the privacy guard compares against. `cfg!`, not
/// `#[cfg]`: the macOS branch type-checks on every platform. With no
/// `HOME`, nothing is guarded.
fn privacy_home() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(PathBuf::from)
    } else {
        None
    }
}

/// Reads the listing of each child directory of `listing` and follows the
/// chains. See [`read_with_chains`].
fn look_ahead(root: &Path, listing: &mut [FileNode], markers: &GitMarkers, home: Option<&Path>) {
    if listing.iter().filter(|node| node.is_dir).count() > LOOKAHEAD_DIR_CAP {
        return;
    }
    for node in listing.iter_mut().filter(|node| node.is_dir) {
        if home.is_some_and(|home| is_privacy_guarded(&node.path, home)) {
            continue;
        }
        match read_tree(root, &node.path, markers) {
            Ok(children) => {
                node.children = children;
                node.loaded = true;
                node.read_error = None;
                if could_fold(&node.children) {
                    look_ahead(root, &mut node.children, markers, home);
                    node.children_resolved = true;
                } else {
                    node.children_resolved = node.children.iter().all(|child| !child.is_dir);
                }
            }
            Err(error) => {
                node.read_error = Some(error);
                node.children.clear();
                node.loaded = false;
                node.children_resolved = false;
            }
        }
    }
}

/// Opens every directory of `nodes` that is in `expanded`, reading what its
/// children need, and descends into the ones it opened.
fn reopen(
    root: &Path,
    nodes: &mut [FileNode],
    expanded: &HashSet<PathBuf>,
    markers: &GitMarkers,
    home: Option<&Path>,
) {
    for node in nodes {
        if !node.is_dir || !expanded.contains(&node.path) {
            continue;
        }
        node.expanded = true;
        if !node.loaded {
            match read_tree(root, &node.path, markers) {
                Ok(children) => {
                    node.children = children;
                    node.loaded = true;
                    node.read_error = None;
                    look_ahead(root, &mut node.children, markers, home);
                    node.children_resolved = true;
                }
                Err(error) => {
                    // Unreadable since it was opened (a deleted folder is
                    // already gone from its parent's listing).
                    node.read_error = Some(error);
                    node.children.clear();
                    node.children_resolved = false;
                    continue;
                }
            }
        } else if !node.children_resolved {
            look_ahead(root, &mut node.children, markers, home);
            node.children_resolved = true;
        }
        reopen(root, &mut node.children, expanded, markers, home);
    }
}

/// `carry_open_state`, with a switch for the level below an open directory:
/// there (`adopt_loaded`), a fresh child directory the loader did not read
/// takes the subtree `current` already held for it.
fn carry_nodes(current: &[FileNode], fresh: Vec<FileNode>, adopt_loaded: bool) -> Vec<FileNode> {
    let by_path: HashMap<(&Path, bool), &FileNode> = current
        .iter()
        .map(|node| ((node.path.as_path(), node.is_dir), node))
        .collect();
    fresh
        .into_iter()
        .map(|mut node| {
            let Some(held) = by_path.get(&(node.path.as_path(), node.is_dir)) else {
                return node;
            };
            node.expanded = held.expanded;
            if node.is_dir {
                if held.expanded {
                    carry_into_open(held, &mut node);
                } else if adopt_loaded {
                    adopt_listing(held, &mut node);
                }
            }
            node
        })
        .collect()
}

/// Merges what `held` (open in the current tree) knows into the fresh `node`.
fn carry_into_open(held: &FileNode, node: &mut FileNode) {
    if node.loaded {
        let fresh = std::mem::take(&mut node.children);
        node.children = carry_nodes(&held.children, fresh, true);
        node.children_resolved = node.children_resolved || held.children_resolved;
    } else {
        adopt_listing(held, node);
    }
}

/// Takes `held`'s listing for a fresh node the loader did not read. A read
/// that failed keeps its error rather than a stale listing.
fn adopt_listing(held: &FileNode, node: &mut FileNode) {
    if node.loaded || !held.loaded || node.read_error.is_some() {
        return;
    }
    node.children = held.children.clone();
    node.loaded = true;
    node.children_resolved = held.children_resolved;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "sirio-files-listing-test-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }

        fn dir(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            std::fs::create_dir_all(&path).expect("create directory");
            path
        }

        fn file(&self, relative: &str) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("create parent");
            std::fs::write(path, "x").expect("write file");
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn read(root: &Path) -> Vec<FileNode> {
        read_with_chains(root, root, &GitMarkers::default()).expect("read the fixture")
    }

    fn reload(root: &Path, expanded: &[&str]) -> Vec<FileNode> {
        let set = expanded.iter().map(|path| root.join(path)).collect();
        reload_tree(root, &set, &GitMarkers::default()).expect("reload the fixture")
    }

    fn child<'a>(nodes: &'a [FileNode], name: &str) -> &'a FileNode {
        nodes
            .iter()
            .find(|node| node.name == name)
            .unwrap_or_else(|| panic!("no entry named {name} in {:?}", names(nodes)))
    }

    fn names(nodes: &[FileNode]) -> Vec<&str> {
        nodes.iter().map(|node| node.name.as_str()).collect()
    }

    fn node(name: &str, is_dir: bool) -> FileNode {
        FileNode {
            path: PathBuf::from(name),
            name: name.rsplit('/').next().expect("a name").to_string(),
            is_dir,
            git_status: None,
            is_ignored: false,
            expanded: false,
            read_error: None,
            children: Vec::new(),
            loaded: false,
            children_resolved: false,
        }
    }

    fn folder(
        path: &str,
        expanded: bool,
        loaded: bool,
        resolved: bool,
        children: Vec<FileNode>,
    ) -> FileNode {
        let mut folder = node(path, true);
        folder.expanded = expanded;
        folder.loaded = loaded;
        folder.children_resolved = resolved;
        folder.children = children;
        folder
    }

    #[test]
    fn lookahead_reads_each_child_directory_listing() {
        let dir = TempDir::new();
        dir.file("a/x.txt");
        dir.file("b/y.txt");
        dir.file("b/z.txt");
        let tree = read(&dir.0);
        for name in ["a", "b"] {
            let node = child(&tree, name);
            assert!(node.loaded && node.children_resolved, "{name}");
        }
        assert_eq!(names(&child(&tree, "b").children), ["y.txt", "z.txt"]);
    }

    #[test]
    fn a_chain_is_followed_to_its_leaf() {
        let dir = TempDir::new();
        dir.file("main/java/it/acme/app/v2/App.java");
        dir.file("main/java/it/acme/app/v2/domain/model/Order.java");
        let tree = read(&dir.0);
        let mut node = child(&tree, "main");
        for name in ["java", "it", "acme", "app"] {
            assert!(node.loaded && node.children_resolved, "{}", node.name);
            node = child(&node.children, name);
        }
        let v2 = child(&node.children, "v2");
        assert!(v2.loaded);
        assert_eq!(names(&v2.children), ["domain", "App.java"]);
        assert!(!child(&v2.children, "domain").loaded);
        assert!(!v2.children_resolved);
        assert!(node.loaded && node.children_resolved, "app");
    }

    #[test]
    fn a_dotfile_does_not_stop_the_chain() {
        let dir = TempDir::new();
        dir.file("a/.DS_Store");
        dir.file("a/b/c/f.txt");
        let tree = read(&dir.0);
        let b = child(&child(&tree, "a").children, "b");
        assert!(b.loaded);
        assert!(child(&b.children, "c").loaded);
    }

    #[test]
    fn a_link_loads_its_dot_directories_too() {
        let dir = TempDir::new();
        dir.file("a/.cache/x");
        dir.file("a/b/y.txt");
        let tree = read(&dir.0);
        let a = child(&tree, "a");
        assert!(a.children_resolved);
        assert!(child(&a.children, ".cache").loaded);
        assert!(child(&a.children, "b").loaded);
    }

    #[test]
    fn a_lone_dot_directory_continues_the_chain() {
        let dir = TempDir::new();
        dir.file(".github/workflows/ci.yml");
        let tree = read(&dir.0);
        let github = child(&tree, ".github");
        assert!(github.loaded);
        assert!(child(&github.children, "workflows").loaded);
    }

    #[test]
    fn a_file_or_two_folders_end_the_chain() {
        let dir = TempDir::new();
        dir.file("a/f.txt");
        dir.dir("a/b/c");
        dir.dir("d/e/g");
        dir.dir("d/h");
        let tree = read(&dir.0);
        // The chain ends at the directory that is not a link: its own
        // listing is read (that is how the walk learns it ended) and
        // nothing below it is.
        for name in ["a", "d"] {
            let ended = child(&tree, name);
            assert!(ended.loaded, "{name}");
            assert!(!ended.children_resolved, "{name}");
            assert!(ended.children.iter().all(|node| !node.loaded), "{name}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_directory_is_never_walked() {
        let dir = TempDir::new();
        let outside = TempDir::new();
        outside.file("secret/x.txt");
        std::os::unix::fs::symlink(&outside.0, dir.0.join("link")).expect("create the symlink");
        let tree = read(&dir.0);
        let link = child(&tree, "link");
        assert!(!link.is_dir);
        assert!(!link.loaded);
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_link_records_its_error_and_ends_the_chain() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new();
        let locked = dir.dir("a/b");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000))
            .expect("lock the directory");
        // Running as root reads it anyway; the case cannot be staged then.
        let staged = std::fs::read_dir(&locked).is_err();
        let result = read_with_chains(&dir.0, &dir.0.join("a"), &GitMarkers::default());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755))
            .expect("restore the directory");
        if !staged {
            return;
        }
        let listing = result.expect("a child's failure is not the read's failure");
        let b = child(&listing, "b");
        assert!(b.read_error.is_some());
        assert!(!b.loaded);
    }

    #[test]
    fn a_listing_over_the_cap_gets_no_lookahead() {
        let dir = TempDir::new();
        for index in 0..=LOOKAHEAD_DIR_CAP {
            dir.dir(&format!("d{index}"));
        }
        let tree = read(&dir.0);
        assert_eq!(tree.len(), LOOKAHEAD_DIR_CAP + 1);
        assert!(tree.iter().all(|node| !node.loaded));
    }

    #[test]
    fn reload_rereads_open_listings_below_the_lookahead() {
        let dir = TempDir::new();
        dir.file("p/q/x.txt");
        dir.dir("p/r");
        let before = reload(&dir.0, &["p", "p/q"]);
        let q = child(&child(&before, "p").children, "q");
        assert_eq!(names(&q.children), ["x.txt"]);
        dir.file("p/q/new.txt");
        let after = reload(&dir.0, &["p", "p/q"]);
        let p = child(&after, "p");
        assert!(p.expanded);
        let q = child(&p.children, "q");
        assert!(q.expanded);
        assert_eq!(names(&q.children), ["new.txt", "x.txt"]);
    }

    #[test]
    fn reload_rereads_an_open_folder_inside_a_listing_over_the_cap() {
        let dir = TempDir::new();
        dir.file("big/d0/x.txt");
        for index in 1..=LOOKAHEAD_DIR_CAP {
            dir.dir(&format!("big/d{index}"));
        }
        dir.file("big/d0/new.txt");
        let tree = reload(&dir.0, &["big", "big/d0"]);
        let d0 = child(&child(&tree, "big").children, "d0");
        assert!(d0.loaded);
        assert_eq!(names(&d0.children), ["new.txt", "x.txt"]);
    }

    #[test]
    fn reload_drops_an_open_folder_deleted_on_disk() {
        let dir = TempDir::new();
        dir.file("p/q/x.txt");
        dir.dir("p/r");
        std::fs::remove_dir_all(dir.0.join("p/q")).expect("remove the folder");
        let tree = reload(&dir.0, &["p", "p/q"]);
        assert_eq!(names(&child(&tree, "p").children), ["r"]);
    }

    #[test]
    fn reload_reads_nothing_below_a_closed_directory() {
        let dir = TempDir::new();
        dir.dir("x/y");
        dir.dir("x/z/w");
        let tree = reload(&dir.0, &[]);
        let x = child(&tree, "x");
        assert!(x.loaded);
        assert!(x.children.iter().all(|node| !node.loaded));
    }

    #[test]
    fn carry_open_state_keeps_a_folder_opened_during_a_refresh() {
        let current = vec![folder(
            "a",
            true,
            true,
            true,
            vec![folder("a/b", false, true, true, vec![node("a/b/f", false)])],
        )];
        let fresh = vec![folder(
            "a",
            false,
            true,
            false,
            vec![folder("a/b", false, false, false, vec![])],
        )];
        let merged = carry_open_state(&current, fresh);
        assert!(merged[0].expanded);
        assert!(merged[0].children[0].loaded);
        assert_eq!(names(&merged[0].children[0].children), ["f"]);
    }

    #[test]
    fn carry_open_state_lets_fresh_entries_win() {
        let current = vec![folder(
            "it",
            true,
            true,
            true,
            vec![folder("it/acme", false, true, true, vec![])],
        )];
        let fresh = vec![folder(
            "it",
            false,
            true,
            false,
            vec![
                folder("it/acme", false, false, false, vec![]),
                node("it/New.java", false),
            ],
        )];
        let merged = carry_open_state(&current, fresh);
        assert_eq!(names(&merged[0].children), ["acme", "New.java"]);
        assert!(merged[0].children[0].loaded);
    }

    #[test]
    fn carry_open_state_closes_what_the_user_closed() {
        let current = vec![folder("a", false, true, true, vec![])];
        let fresh = vec![folder("a", true, true, true, vec![])];
        assert!(!carry_open_state(&current, fresh)[0].expanded);
    }

    #[test]
    fn carry_open_state_carries_nothing_below_a_closed_directory() {
        let current = vec![folder(
            "p",
            false,
            true,
            true,
            vec![folder("p/q", true, true, true, vec![node("p/q/f", false)])],
        )];
        let fresh = vec![folder(
            "p",
            false,
            true,
            false,
            vec![folder("p/q", false, false, false, vec![])],
        )];
        let merged = carry_open_state(&current, fresh);
        assert!(!merged[0].children[0].loaded);
        assert!(!merged[0].children[0].expanded);
    }

    #[test]
    fn privacy_guarded_folders_are_the_macos_list() {
        let home = Path::new("/home/user");
        for name in [
            "Desktop",
            "Library",
            "Documents",
            "Downloads",
            "Pictures",
            "Movies",
            "Music",
        ] {
            assert!(is_privacy_guarded(&home.join(name), home), "{name}");
        }
        assert!(!is_privacy_guarded(&home.join("Desktop/proj"), home));
        assert!(!is_privacy_guarded(&home.join("src"), home));
        assert!(!is_privacy_guarded(Path::new("/elsewhere/Desktop"), home));
    }
}
