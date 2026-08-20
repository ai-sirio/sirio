//! Layer-D process inspection (`AgentActivityModel`'s foreground-process signal).
//!
//! The activity model deliberately remains a pure state machine. This module
//! is the platform boundary that supplies it with the process names below a
//! terminal shell. Linux exposes the equivalent of the macOS libproc walk via
//! `/proc/<pid>/task/<pid>/children` and `/proc/<pid>/comm` — implemented
//! below. macOS uses the system `libproc` API because it has no `/proc` tree;
//! Windows remains unsupported here.

use std::collections::HashSet;
use std::io;

use crate::model::{CATALOG_IDS, identify_agent_from_process_names};

/// Inspects the process tree and resolves the first supported agent in catalog
/// order. The returned identity is static because it comes from the fixed
/// [`CATALOG_IDS`] catalog.
pub fn inspect_foreground_agent(shell_pid: u32) -> io::Result<Option<&'static str>> {
    let names = inspect_process_names(shell_pid)?;
    Ok(identify_agent_from_process_names(&names, &CATALOG_IDS))
}

#[cfg(target_os = "linux")]
use std::collections::VecDeque;
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
const PROC_ROOT: &str = "/proc";
#[cfg(any(target_os = "linux", target_os = "macos"))]
const MAX_DEPTH: usize = 5;
#[cfg(any(target_os = "linux", target_os = "macos"))]
const MAX_PROCESSES: usize = 50;

/// Returns process comm names for the shell's descendants, including direct
/// children and nested descendants, bounded to the same depth and process
/// count as the reference activity detector.
///
/// A disappeared descendant is normal during polling and is skipped. Failure
/// to inspect the shell itself is returned so the caller can distinguish an
/// unavailable process tree from a tree with no matching agent.
#[cfg(target_os = "linux")]
pub fn inspect_process_names(shell_pid: u32) -> io::Result<HashSet<String>> {
    inspect_process_names_from(Path::new(PROC_ROOT), shell_pid)
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn read_comm(proc_root: &Path, pid: u32) -> io::Result<Option<String>> {
    let path: PathBuf = proc_root.join(pid.to_string()).join("comm");
    match std::fs::read_to_string(path) {
        Ok(name) => Ok(Some(name.trim().to_string())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "macos")]
mod macos_process {
    use std::io;
    use std::os::raw::{c_int, c_void};

    const INITIAL_PID_CAPACITY: usize = 64;
    const MAX_PID_CAPACITY: usize = 16_384;
    const PROCESS_NAME_CAPACITY: usize = 256;

    #[link(name = "proc")]
    unsafe extern "C" {
        fn proc_listchildpids(ppid: c_int, buffer: *mut c_void, buffersize: c_int) -> c_int;
        fn proc_name(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
    }

    fn to_pid_t(pid: u32) -> io::Result<c_int> {
        c_int::try_from(pid)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "process id is too large"))
    }

    pub fn child_pids(parent: u32) -> io::Result<Vec<u32>> {
        let parent = to_pid_t(parent)?;
        let mut buffer = vec![0_i32; INITIAL_PID_CAPACITY];

        loop {
            let buffer_size = (buffer.len() * std::mem::size_of::<i32>()) as c_int;
            // libproc returns the number of PIDs copied, not a byte count.
            let reported_count =
                unsafe { proc_listchildpids(parent, buffer.as_mut_ptr().cast(), buffer_size) };
            if reported_count < 0 {
                return Err(io::Error::last_os_error());
            }

            let count = (reported_count as usize).min(buffer.len());
            if (reported_count as usize) < buffer.len() {
                return Ok(buffer[..count]
                    .iter()
                    .copied()
                    .filter(|pid| *pid > 0)
                    .map(|pid| pid as u32)
                    .collect());
            }

            if buffer.len() >= MAX_PID_CAPACITY {
                return Err(io::Error::new(
                    io::ErrorKind::OutOfMemory,
                    "macOS child-process list exceeded safety limit",
                ));
            }
            buffer.resize((buffer.len() * 2).min(MAX_PID_CAPACITY), 0);
        }
    }

    pub fn process_name(pid: u32) -> io::Result<Option<String>> {
        let pid = to_pid_t(pid)?;
        let mut buffer = [0_u8; PROCESS_NAME_CAPACITY];
        let reported_length =
            unsafe { proc_name(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
        if reported_length <= 0 {
            return Ok(None);
        }
        let length = buffer
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or((reported_length as usize).min(buffer.len()));
        Ok(Some(
            String::from_utf8_lossy(&buffer[..length]).into_owned(),
        ))
    }

    pub fn is_process_gone(error: &io::Error) -> bool {
        error.kind() == io::ErrorKind::NotFound || matches!(error.raw_os_error(), Some(3))
    }
}

/// Returns process names for the shell's descendants using macOS's live
/// `libproc` process table. The traversal has the same depth and process-count
/// bounds as the Linux `/proc` implementation above.
#[cfg(target_os = "macos")]
pub fn inspect_process_names(shell_pid: u32) -> io::Result<HashSet<String>> {
    use std::collections::VecDeque;

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
            && let Some(name) = macos_process::process_name(pid)?
        {
            names.insert(name);
        }
        if depth >= MAX_DEPTH {
            continue;
        }

        match macos_process::child_pids(pid) {
            Ok(children) => queue.extend(children.into_iter().map(|child| (child, depth + 1))),
            Err(error) if depth > 0 && macos_process::is_process_gone(&error) => continue,
            Err(error) => return Err(error),
        }
    }

    Ok(names)
}

/// Windows counterpart: Toolhelp32 (`CreateToolhelp32Snapshot` plus
/// `Process32First`/`Process32Next`, walking `th32ParentProcessID`).
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn inspect_process_names(_shell_pid: u32) -> io::Result<HashSet<String>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "process-tree inspection is not implemented on this platform (Windows: Toolhelp32)",
    ))
}

#[cfg(all(test, target_os = "linux"))]
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
