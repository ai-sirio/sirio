use std::time::Duration;

#[cfg_attr(not(unix), allow(dead_code))]
const TERMINATE_GRACE: Duration = Duration::from_millis(500);

#[cfg(unix)]
pub fn terminate_process_group(process_group: u32) {
    let process_group = process_group as libc::pid_t;
    if process_group <= 0 {
        return;
    }

    unsafe {
        libc::killpg(process_group, libc::SIGTERM);
    }

    std::thread::spawn(move || {
        std::thread::sleep(TERMINATE_GRACE);
        let group_is_alive = unsafe { libc::killpg(process_group, 0) == 0 };
        if group_is_alive {
            unsafe {
                libc::killpg(process_group, libc::SIGKILL);
            }
        }
    });
}

/// Every process id that is currently a descendant of `root` (not
/// including `root` itself), discovered by walking the kernel's own live
/// parent/child view (F-PER-06).
///
/// `/proc/<pid>/task/<tid>/children` is read fresh per pid rather than
/// cached: a job-control-spawned child (a compound command, a backgrounded
/// job) can detach into its own session/process group at any point after
/// spawn, and a stubborn descendant can itself keep forking, so only a walk
/// done at kill time — not the single pgid captured once at spawn — can
/// find all of it. A process can have multiple threads (`task/<tid>`), and
/// each thread's `children` file only lists the children *that thread*
/// directly spawned, so every tid must be read to see the whole process's
/// children.
#[cfg(target_os = "linux")]
pub fn descendant_pids(root: libc::pid_t) -> Vec<libc::pid_t> {
    let mut discovered = Vec::new();
    let mut seen = std::collections::HashSet::new();
    seen.insert(root);
    let mut frontier = vec![root];
    while let Some(pid) = frontier.pop() {
        let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
            continue;
        };
        for task in tasks.flatten() {
            let Ok(contents) = std::fs::read_to_string(task.path().join("children")) else {
                continue;
            };
            for token in contents.split_whitespace() {
                let Ok(child) = token.parse::<libc::pid_t>() else {
                    continue;
                };
                if seen.insert(child) {
                    discovered.push(child);
                    frontier.push(child);
                }
            }
        }
    }
    discovered
}

/// macOS implementation of [`descendant_pids`], using libproc's live child
/// table because macOS has no `/proc` filesystem.
#[cfg(all(unix, not(target_os = "linux")))]
pub fn descendant_pids(root: libc::pid_t) -> Vec<libc::pid_t> {
    #[cfg(target_os = "macos")]
    {
        let mut discovered = Vec::new();
        let mut seen = std::collections::HashSet::from([root]);
        let mut frontier = vec![root];
        while let Some(pid) = frontier.pop() {
            let Ok(children) = macos_child_pids(pid) else {
                continue;
            };
            for child in children {
                if seen.insert(child) {
                    discovered.push(child);
                    frontier.push(child);
                }
            }
        }
        discovered
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = root;
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
pub fn macos_child_pids(parent: libc::pid_t) -> std::io::Result<Vec<libc::pid_t>> {
    use std::os::raw::{c_int, c_void};

    const INITIAL_PID_CAPACITY: usize = 64;
    const MAX_PID_CAPACITY: usize = 16_384;

    #[link(name = "proc")]
    unsafe extern "C" {
        fn proc_listchildpids(ppid: c_int, buffer: *mut c_void, buffersize: c_int) -> c_int;
    }

    let mut buffer = vec![0_i32; INITIAL_PID_CAPACITY];
    loop {
        let buffer_size = (buffer.len() * std::mem::size_of::<libc::pid_t>()) as c_int;
        // libproc returns the number of PIDs copied, not a byte count.
        let reported_count =
            unsafe { proc_listchildpids(parent, buffer.as_mut_ptr().cast(), buffer_size) };
        if reported_count < 0 {
            return Err(std::io::Error::last_os_error());
        }

        let count = (reported_count as usize).min(buffer.len());
        if (reported_count as usize) < buffer.len() {
            return Ok(buffer[..count]
                .iter()
                .copied()
                .filter(|pid| *pid > 0)
                .map(|pid| pid as libc::pid_t)
                .collect());
        }

        if buffer.len() >= MAX_PID_CAPACITY {
            return Err(std::io::Error::new(
                std::io::ErrorKind::OutOfMemory,
                "macOS child-process list exceeded safety limit",
            ));
        }
        buffer.resize((buffer.len() * 2).min(MAX_PID_CAPACITY), 0);
    }
}

/// The distinct process groups spanning the shell (`shell_pid`) and every
/// descendant it has right now, deduplicated. A single `killpg` on the pgid
/// captured once at spawn only ever reaches the shell's *original* group —
/// a job-control-spawned child that detached into a new group (a compound
/// command run under job control, a backgrounded job in an interactive
/// shell) is invisible to it and survives shutdown as an orphan (F-PER-06).
/// Reading `getpgid` per pid at kill time, rather than assuming the shell's
/// own pid is still its pgid, is what makes this correct even if the shell
/// itself has re-grouped.
#[cfg(unix)]
pub fn descendant_process_groups(shell_pid: libc::pid_t) -> Vec<libc::pid_t> {
    let mut pids = vec![shell_pid];
    pids.extend(descendant_pids(shell_pid));
    let mut groups: Vec<libc::pid_t> = Vec::new();
    for pid in pids {
        let pgid = unsafe { libc::getpgid(pid) };
        if pgid > 0 && !groups.contains(&pgid) {
            groups.push(pgid);
        }
    }
    groups
}

/// Terminates the shell at `shell_pid` and every process group any of its
/// current descendants live in — not just the group captured at spawn
/// (F-PER-06). Each distinct group gets its own SIGTERM-then-grace-then-
/// SIGKILL handling via [`terminate_process_group`].
#[cfg(unix)]
pub fn terminate_descendant_process_groups(shell_pid: u32) {
    for group in descendant_process_groups(shell_pid as libc::pid_t) {
        terminate_process_group(group as u32);
    }
}

/// Windows stand-in for [`terminate_descendant_process_groups`]. Unix's
/// `killpg`/`getpgid` walk over `/proc/<pid>/task/<tid>/children` has no
/// Toolhelp32 port here (`CreateToolhelp32Snapshot` + `Process32First`/
/// `Process32Next`, matching `th32ParentProcessID`, would enumerate the
/// descendants; there is no Windows process-group signal to broadcast to
/// once found). The Windows fix is a per-session Job Object with
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; see [`KillOnClose`].
#[cfg(not(unix))]
pub fn terminate_descendant_process_groups(_shell_pid: u32) {}

/// Windows: every session's child goes into a job that kills its whole tree
/// when the job's last handle closes (SP2 spec §6). Nested jobs (Windows 8+)
/// make this work inside a host that is itself in a job it could not leave.
#[cfg(windows)]
pub struct KillOnClose(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl KillOnClose {
    pub fn adopt(process: std::os::windows::io::RawHandle) -> std::io::Result<Self> {
        use windows_sys::Win32::System::JobObjects::*;
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let set = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if set == 0 || AssignProcessToJobObject(job, process as _) == 0 {
                let error = std::io::Error::last_os_error();
                windows_sys::Win32::Foundation::CloseHandle(job);
                return Err(error);
            }
            Ok(Self(job))
        }
    }
}

#[cfg(windows)]
impl Drop for KillOnClose {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

// SAFETY: a job handle is a kernel handle usable from any thread.
#[cfg(windows)]
unsafe impl Send for KillOnClose {}

/// Unix: a host is a session leader without a controlling terminal (SP1
/// §5.4). If opening a PTY made the PTY its controlling terminal — an
/// `openpty` that does not pass `O_NOCTTY` — the host gives it up at once,
/// before the child's `setsid`/`TIOCSCTTY` needs it. A process with no
/// controlling terminal cannot open `/dev/tty`, so the check is the open.
#[cfg(unix)]
pub fn drop_acquired_controlling_terminal() {
    // SAFETY: plain open/ioctl/close on a path and the fd it returns.
    unsafe {
        let fd = libc::open(c"/dev/tty".as_ptr(), libc::O_RDWR | libc::O_NOCTTY);
        if fd >= 0 {
            if libc::getsid(0) == libc::getpid() {
                libc::ioctl(fd, libc::TIOCNOTTY as _);
            }
            libc::close(fd);
        }
    }
}
