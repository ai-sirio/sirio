//! Layer-D process inspection (`AgentActivityModel`'s foreground-process signal).
//!
//! The activity model deliberately remains a pure state machine. This module
//! is the platform boundary that supplies it with the process names below a
//! terminal shell. Linux exposes the equivalent of the macOS libproc walk via
//! `/proc/<pid>/task/<pid>/children` and `/proc/<pid>/comm` — implemented
//! below. macOS uses the system `libproc` API because it has no `/proc` tree;
//! Windows uses a Toolhelp32 snapshot (`CreateToolhelp32Snapshot` plus
//! `Process32First`/`Process32Next`, walking `th32ParentProcessID`).

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
#[cfg(windows)]
use std::collections::{HashMap, VecDeque};
#[cfg(windows)]
const MAX_DEPTH: usize = 5;
#[cfg(windows)]
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

// ---------------------------------------------------------------------------
// Windows: Toolhelp32 snapshot
// ---------------------------------------------------------------------------

/// Hand-declared kernel32 surface rather than a binding crate: this crate is
/// deliberately dependency-free ("pure std" in its Cargo.toml) so it stays
/// testable without a window, and the same precedent already exists above in
/// the hand-declared `libproc` block. Everything needed here lives in
/// kernel32, which the MSVC linker pulls in anyway.
#[cfg(windows)]
mod windows_process {
    use std::io;

    const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    const ERROR_NO_MORE_FILES: i32 = 18;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    type Handle = *mut core::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, th32_process_id: u32) -> Handle;
        fn Process32FirstW(snapshot: Handle, entry: *mut ProcessEntry32W) -> i32;
        fn Process32NextW(snapshot: Handle, entry: *mut ProcessEntry32W) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
        fn OpenProcess(desired_access: u32, inherit_handle: i32, pid: u32) -> Handle;
        fn GetProcessTimes(
            process: Handle,
            creation_time: *mut FileTime,
            exit_time: *mut FileTime,
            kernel_time: *mut FileTime,
            user_time: *mut FileTime,
        ) -> i32;
    }

    /// Mirrors WinAPI's `PROCESSENTRY32W`. `th32DefaultHeapID` is a
    /// `ULONG_PTR`, so its width follows the pointer — hence `usize`.
    #[repr(C)]
    struct ProcessEntry32W {
        size: u32,
        usage_count: u32,
        process_id: u32,
        default_heap_id: usize,
        module_id: u32,
        thread_count: u32,
        parent_process_id: u32,
        base_priority: i32,
        flags: u32,
        exe_file: [u16; 260],
    }

    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    /// One snapshot row, with the image name already normalized to the bare
    /// shape the catalog matches against.
    pub struct Entry {
        pub pid: u32,
        pub parent_pid: u32,
        pub name: String,
    }

    /// Owns the snapshot handle so early returns cannot leak it.
    struct Snapshot(Handle);

    impl Snapshot {
        fn new() -> io::Result<Self> {
            // pid 0 means "all processes" for TH32CS_SNAPPROCESS.
            let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
            if handle.is_null() || handle == -1_isize as Handle {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(handle))
        }
    }

    impl Drop for Snapshot {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    /// Windows image names carry a `.exe` suffix that Linux comm names never
    /// have, while [`crate::model::identify_agent_from_process_names`] matches
    /// bare lower-case catalog ids like `codex`. Strip the suffix and fold to
    /// lower-case: NTFS is case-insensitive, so the same binary surfaces as
    /// `PING.EXE`, `ping.exe`, or anything in between depending on how its
    /// spawner wrote the command line (observed verbatim in tests), and the
    /// Linux `/proc` comm names this set must stay interchangeable with are
    /// conventionally lower-case.
    fn normalized_image_name(raw: &[u16]) -> String {
        let length = raw.iter().position(|unit| *unit == 0).unwrap_or(raw.len());
        let name = String::from_utf16_lossy(&raw[..length]);
        match name.len().checked_sub(4).and_then(|cut| name.get(cut..)) {
            Some(suffix) if suffix.eq_ignore_ascii_case(".exe") => {
                name[..name.len() - 4].to_ascii_lowercase()
            }
            _ => name.to_ascii_lowercase(),
        }
    }

    /// One pass over a whole-system snapshot. Taken once per call rather than
    /// queried incrementally because Toolhelp32 offers no "children of X"
    /// query — parentage comes from filtering rows by `th32ParentProcessID`.
    pub fn entries() -> io::Result<Vec<Entry>> {
        let snapshot = Snapshot::new()?;
        // SAFETY: `entry` is a valid PROCESSENTRY32W whose `size` field is set
        // to its own size, as WinAPI requires before the first call.
        let mut entry: ProcessEntry32W = unsafe { std::mem::zeroed() };
        entry.size = std::mem::size_of::<ProcessEntry32W>() as u32;

        let mut out = Vec::new();
        unsafe {
            if Process32FirstW(snapshot.0, &mut entry) == 0 {
                return Err(io::Error::last_os_error());
            }
            loop {
                out.push(Entry {
                    pid: entry.process_id,
                    parent_pid: entry.parent_process_id,
                    name: normalized_image_name(&entry.exe_file),
                });
                if Process32NextW(snapshot.0, &mut entry) == 0 {
                    // ERROR_NO_MORE_FILES is the documented end-of-snapshot
                    // marker, not a failure.
                    let error = io::Error::last_os_error();
                    if error.raw_os_error() != Some(ERROR_NO_MORE_FILES) {
                        return Err(error);
                    }
                    break;
                }
            }
        }
        Ok(out)
    }

    /// Counts creation-time resolutions so a test can pin the lazy-walk cost
    /// property (see `windows_tests` below). Test builds only — the shipped
    /// binary pays no counter.
    #[cfg(test)]
    pub(crate) static CREATION_TIME_QUERIES: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(0);

    /// Creation time as an opaque, comparable counter (FILETIME ticks).
    ///
    /// `None` covers both "cannot open" (a protected process) and "already
    /// exited between snapshot and now"; callers fail open on it, because a
    /// vanished candidate is indistinguishable from a legitimate child that
    /// lost a race with the poll.
    pub fn creation_time(pid: u32) -> Option<u64> {
        #[cfg(test)]
        CREATION_TIME_QUERIES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // SAFETY: all four output pointers are valid `FileTime`s for the
        // duration of the call; the handle is closed on every path.
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None;
            }
            let (mut creation, mut exit, mut kernel, mut user): (
                FileTime,
                FileTime,
                FileTime,
                FileTime,
            ) = std::mem::zeroed();
            let ok =
                GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user);
            CloseHandle(process);
            if ok == 0 {
                return None;
            }
            Some(((creation.high as u64) << 32) | creation.low as u64)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::normalized_image_name;

        #[test]
        fn normalizes_suffix_and_case_to_catalog_shape() {
            assert_eq!(normalized_image_name(&utf16("codex.exe")), "codex");
            assert_eq!(normalized_image_name(&utf16("CODEX.EXE")), "codex");
            assert_eq!(normalized_image_name(&utf16("PING")), "ping");
            assert_eq!(normalized_image_name(&utf16("pwsh.dll")), "pwsh.dll");
            assert_eq!(normalized_image_name(&utf16("exe")), "exe");
            assert_eq!(normalized_image_name(&[]), "");
        }

        fn utf16(value: &str) -> Vec<u16> {
            value.encode_utf16().collect()
        }
    }
}

/// Returns process names for the shell's descendants using one Toolhelp32
/// snapshot. The traversal mirrors the Linux `/proc` arm exactly: breadth-first
/// from the shell (depth 0, whose own name is excluded), bounded by the same
/// [`MAX_DEPTH`] and [`MAX_PROCESSES`] limits.
///
/// Known limitation, inherited from the Linux arm and NOT a bug: Node/Bun-
/// hosted CLIs (pi, omp) run as their host binary — `node.exe` on Windows —
/// so they are invisible to Layer D here just as they are behind `node` on
/// Linux, and they keep relying on Layer B titles instead.
#[cfg(windows)]
pub fn inspect_process_names(shell_pid: u32) -> io::Result<HashSet<String>> {
    // `pty_shell_pid` (tiller_terminal) reports 0 when `GetProcessId` could
    // not resolve the ConPTY child; 0 is never a real Windows pid. Walking it
    // anyway would be actively harmful: Toolhelp32 happily reports system
    // processes whose `th32ParentProcessID` is 0, so a pid-0 walk would pin
    // random system processes onto this pane. Refuse instead — and refuse
    // with `Err`, not `Ok(empty)`, because `AgentActivityModel::
    // refresh_process_signal` treats `Ok(None)`/empty as "process_gone" and
    // an unknown pid must never clear a legitimately identified pane.
    if shell_pid == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "shell pid is unknown (0); refusing to enumerate the process tree",
        ));
    }

    let entries = windows_process::entries()?;
    // children_of and name_of are built eagerly in this one pass over the
    // already-materialized snapshot rows, and that is deliberate: the rows are
    // plain memory by the time `entries()` returns (the snapshot syscall is a
    // single call), so indexing them costs no handle operations at all. Only
    // creation times need OpenProcess/GetProcessTimes per pid — the expensive
    // part — so those alone are resolved lazily below, for pids the walk
    // actually touches, keeping the per-poll cost O(walked) like the Linux
    // /proc arm rather than O(all processes).
    let mut children_of: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut name_of: HashMap<u32, String> = HashMap::new();
    for entry in entries {
        children_of.entry(entry.parent_pid).or_default().push(entry.pid);
        name_of.insert(entry.pid, entry.name);
    }

    // The shell vanished between the PTY watcher resolving its pid and this
    // snapshot. Like the Linux arm's depth-0 NotFound, this is surfaced as an
    // error (not an empty set) so the model can tell "pane is gone" apart
    // from "nothing running under the pane".
    if !name_of.contains_key(&shell_pid) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "shell process is absent from the process snapshot",
        ));
    }

    let mut names = HashSet::new();
    let mut queue = VecDeque::from([(shell_pid, 0_usize)]);
    let mut visited = HashSet::new();
    // Per-call memo of creation times. The same pid can be reached more than
    // once through the queue (as a candidate under several parents before the
    // visited set dedupes it), and each memo miss costs one OpenProcess +
    // GetProcessTimes round trip on the caller's thread, so every pid pays it
    // at most once per walk.
    let mut creation_times: HashMap<u32, Option<u64>> = HashMap::new();
    let resolve_creation_time =
        |cache: &mut HashMap<u32, Option<u64>>, pid: u32| -> Option<u64> {
            *cache.entry(pid).or_insert_with(|| windows_process::creation_time(pid))
        };

    while let Some((pid, depth)) = queue.pop_front() {
        if !visited.insert(pid) {
            continue;
        }
        if visited.len() > MAX_PROCESSES {
            break;
        }

        if depth > 0
            && let Some(name) = name_of.get(&pid)
        {
            names.insert(name.clone());
        }
        if depth >= MAX_DEPTH {
            continue;
        }

        // Resolved here, not when queued: only nodes the walk actually
        // expands pay for a lookup, and each exactly once thanks to the memo.
        let pid_created = resolve_creation_time(&mut creation_times, pid);

        if let Some(children) = children_of.get(&pid) {
            for &child in children {
                // Windows recycles pids, and an orphaned process keeps the
                // parent pid it recorded at spawn time forever. If that
                // original parent exited and its pid was later reused by THIS
                // pane's shell, the orphan's stale `th32ParentProcessID` now
                // names our shell and the orphan would be misattributed to
                // this pane. A genuine child always postdates its parent, so
                // a candidate older than the pid it claims as parent is a
                // recycled-pid ghost and is dropped. When a creation time is
                // unknowable (protected or already-exited process) the
                // candidate stays: a vanished process is routine mid-poll,
                // and a protected process will not carry an agent image name.
                let child_created = resolve_creation_time(&mut creation_times, child);
                if let (Some(parent_created), Some(child_created)) = (pid_created, child_created)
                    && child_created < parent_created
                {
                    continue;
                }
                queue.push_back((child, depth + 1));
            }
        }
    }

    Ok(names)
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::*;
    use std::sync::atomic::Ordering;

    /// Pins Layer D's cost property, not just its correctness: creation-time
    /// lookups (one OpenProcess + GetProcessTimes round trip each) must stay
    /// proportional to the WALKED SUBTREE, never one per row of the
    /// whole-system snapshot. This runs every poll on the GPUI main thread,
    /// once per pane, so an eager lookup over ~300 snapshot rows per pane per
    /// poll would be thousands of handle operations per second on the render
    /// thread.
    #[test]
    fn resolves_creation_times_only_for_the_walked_subtree() {
        let comspec =
            std::env::var("ComSpec").expect("ComSpec is set on every Windows install");
        let root = std::env::temp_dir().join(format!(
            "tiller-activity-cost-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create cost fixture directory");
        let agent = root.join("codex.exe");
        std::fs::copy(&comspec, &agent).expect("copy shell as codex.exe fixture");
        let mut child = std::process::Command::new(&agent)
            .args(["/c", "ping -n 6 127.0.0.1 >nul"])
            .spawn()
            .expect("spawn cost fixture child");
        std::thread::sleep(std::time::Duration::from_millis(200));

        let before = windows_process::CREATION_TIME_QUERIES.load(Ordering::Relaxed);
        let names = inspect_process_names(std::process::id()).expect("walk from test process");
        let after = windows_process::CREATION_TIME_QUERIES.load(Ordering::Relaxed);
        let queries = (after - before) as usize;
        // Taken after the walk; snapshot size wobbles between calls, but by
        // far less than the margin asserted against.
        let snapshot_size = windows_process::entries().expect("count snapshot rows").len();

        assert!(names.contains("codex"), "fixture must be found, got {names:?}");
        assert!(
            snapshot_size > 50,
            "expected a realistically populated system, got {snapshot_size} rows"
        );
        assert!(
            queries < snapshot_size,
            "walk must not resolve a creation time per snapshot row \
             ({queries} lookups for {snapshot_size} rows)"
        );
        // Tighter ceiling for this fixture's tiny subtree (test process + the
        // alias + ping, plus whatever few siblings cargo leaves running); if
        // the walk ever regresses to eager lookups this fails long before the
        // snapshot-size assertion does.
        assert!(
            queries <= 25,
            "a two-level subtree must not cost {queries} creation-time lookups"
        );

        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(root);
    }
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
