//! Linux Layer-D process inspection.
//!
//! The activity model deliberately remains a pure state machine. This module
//! is the platform boundary that supplies it with the process names below a
//! terminal shell. Linux exposes the equivalent of the macOS libproc walk in
//! `/proc/<pid>/task/<pid>/children` and `/proc/<pid>/comm`.

use std::collections::{HashSet, VecDeque};
use std::io;
use std::path::{Path, PathBuf};

use crate::model::{CATALOG_IDS, identify_agent_from_process_names};

const PROC_ROOT: &str = "/proc";
const MAX_DEPTH: usize = 5;
const MAX_PROCESSES: usize = 50;

/// Returns process comm names for the shell's descendants, including direct
/// children and nested descendants, bounded to the same depth and process
/// count as the reference activity detector.
///
/// A disappeared descendant is normal during polling and is skipped. Failure
/// to inspect the shell itself is returned so the caller can distinguish an
/// unavailable process tree from a tree with no matching agent.
pub fn inspect_process_names(shell_pid: u32) -> io::Result<HashSet<String>> {
    inspect_process_names_from(Path::new(PROC_ROOT), shell_pid)
}

/// Inspects the process tree and resolves the first supported agent in catalog
/// order. The returned identity is static because it comes from the fixed
/// [`CATALOG_IDS`] catalog.
pub fn inspect_foreground_agent(shell_pid: u32) -> io::Result<Option<&'static str>> {
    let names = inspect_process_names(shell_pid)?;
    Ok(identify_agent_from_process_names(&names, &CATALOG_IDS))
}

fn inspect_process_names_from(proc_root: &Path, shell_pid: u32) -> io::Result<HashSet<String>> {
    let mut names = HashSet::new();
    let mut queue = VecDeque::from([(shell_pid, 0_usize)]);
    let mut visited = HashSet::new();

    while let Some((pid, depth)) = queue.pop_front() {
        if !visited.insert(pid) {
            continue;
        }
        if visited.len() > MAX_PROCESSES {
            break;
        }

        if depth > 0
            && let Some(name) = read_comm(proc_root, pid)?
        {
            names.insert(name);
        }
        if depth >= MAX_DEPTH {
            continue;
        }

        match read_children(proc_root, pid) {
            Ok(children) => queue.extend(children.into_iter().map(|child| (child, depth + 1))),
            Err(error) if depth > 0 && error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }
    }

    Ok(names)
}

fn read_children(proc_root: &Path, pid: u32) -> io::Result<Vec<u32>> {
    let path = proc_root
        .join(pid.to_string())
        .join("task")
        .join(pid.to_string())
        .join("children");
    let contents = std::fs::read_to_string(path)?;
    Ok(contents
        .split_whitespace()
        .filter_map(|value| value.parse::<u32>().ok())
        .collect())
}

fn read_comm(proc_root: &Path, pid: u32) -> io::Result<Option<String>> {
    let path: PathBuf = proc_root.join(pid.to_string()).join("comm");
    match std::fs::read_to_string(path) {
        Ok(name) => Ok(Some(name.trim().to_string())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_nested_children_and_skips_disappeared_descendants() {
        let root = std::env::temp_dir().join(format!(
            "tiller-activity-proc-fixture-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let shell = root.join("100");
        let child = root.join("101");
        let grandchild = root.join("102");
        std::fs::create_dir_all(shell.join("task/100")).expect("shell fixture");
        std::fs::create_dir_all(child.join("task/101")).expect("child fixture");
        std::fs::create_dir_all(grandchild.join("task/102")).expect("grandchild fixture");
        std::fs::write(shell.join("task/100/children"), "101 999\n").expect("children");
        std::fs::write(child.join("task/101/children"), "102\n").expect("grandchildren");
        std::fs::write(grandchild.join("task/102/children"), "").expect("leaf");
        std::fs::write(child.join("comm"), "node\n").expect("child name");
        std::fs::write(grandchild.join("comm"), "codex\n").expect("agent name");

        let names = inspect_process_names_from(&root, 100).expect("walk fixture");
        assert_eq!(
            names,
            HashSet::from(["node".to_string(), "codex".to_string()])
        );
        assert_eq!(
            identify_agent_from_process_names(&names, &CATALOG_IDS),
            Some("codex")
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
