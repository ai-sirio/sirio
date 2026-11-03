//! Names the program that owns a session's foreground process group.
//!
//! The owner polls the terminal's foreground group; this module turns that into a program
//! (name, argv, cwd) and decides when a report is worth sending. Nothing here logs: argv and
//! cwd are content and never leave the process except as the event itself.

use std::time::{Duration, Instant};

/// At most this many argv entries are kept.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) const MAX_ARGS: usize = 32;
/// A name or argv entry is cut to this many bytes, on a char boundary; a longer cwd is
/// dropped, since a cut path names another directory.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) const MAX_ARG_BYTES: usize = 1024;
/// The whole argv is kept to this many bytes: every `term.list` carries one per session, and a
/// reply over the protocol's payload limit fails every list.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) const MAX_ARGV_BYTES: usize = 4096;
/// A group that has not yet been read twice with the same result is read again this often.
pub(crate) const PROGRAM_REREAD: Duration = Duration::from_millis(250);
/// A group whose last two reads agree is still read this often while it stays the foreground:
/// a wrapper that is still working when it takes the terminal (a shim, `sh -c 'sleep 1;
/// exec prog'`) reads as the wrapper at first.
pub(crate) const PROGRAM_SETTLED_REREAD: Duration = Duration::from_secs(2);
/// After the shell takes the terminal back, status reports are dropped for this long.
pub(crate) const LATE_REPORT_WINDOW: Duration = Duration::from_secs(1);
/// Probes answered closer together than this are answered once.
pub(crate) const PROBE_ANSWER_GAP: Duration = Duration::from_secs(1);

/// The leader of a foreground process group, as read from the operating system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForegroundProgram {
    pub pid: u32,
    /// Empty when the leader could not be read (it exited, or the platform has no reader).
    pub name: String,
    pub argv: Vec<String>,
    pub cwd: Option<String>,
}

/// What one poll of the foreground changes.
pub(crate) enum Update {
    Nothing,
    /// A program is now (or newly read as) the foreground.
    Named(ForegroundProgram),
    /// A new group took the terminal while the named program's leader is gone: the named
    /// program ends (as for `ShellAgain`) and this one is named.
    Replaced(ForegroundProgram),
    /// The shell has been the foreground for two polls in a row after a named program.
    ShellAgain,
}

/// Follows the foreground process group across polls.
///
/// A group is read on its first poll, then every [`PROGRAM_REREAD`] until two reads in a row
/// agree (a fork that has not exec'd yet reads as its parent), then every
/// [`PROGRAM_SETTLED_REREAD`] for as long as it stays the foreground; a read that differs
/// from the last one goes back to the fast rate.
///
/// Reports are accepted from the start: a tab without job control never sees its program
/// leave the shell's group. They are dropped only for [`LATE_REPORT_WINDOW`] after a named
/// program gives the terminal back to the shell (a late report then belongs to a program
/// that is gone), or until something other than the shell is in the foreground again,
/// whichever comes first. A program that replaces the shell without changing the group
/// (`exec claude`) must be heard again once the window is over.
pub(crate) struct ForegroundTracker {
    current: Option<ForegroundProgram>,
    /// The last group seen that was not the shell's.
    group: Option<u32>,
    shell_polls: u8,
    /// The last read of `group`; a second identical read settles it.
    pending: Option<ForegroundProgram>,
    settled: bool,
    last_read: Option<Instant>,
    /// Reports are dropped until this instant, when set.
    late_until: Option<Instant>,
    /// The live program a child group took the terminal from, with that program's group:
    /// the terminal going back to it is the same program again, not a replacement. One
    /// slot, kept for the first such hand-over (a child's own child does not displace it).
    parked: Option<(u32, ForegroundProgram)>,
}

impl ForegroundTracker {
    pub(crate) fn new() -> Self {
        Self {
            current: None,
            group: None,
            shell_polls: 0,
            pending: None,
            settled: false,
            last_read: None,
            late_until: None,
            parked: None,
        }
    }

    /// Whether a status report arriving at `now` is the current program's.
    pub(crate) fn accepts_reports(&self, now: Instant) -> bool {
        self.late_until.is_none_or(|until| now >= until)
    }

    /// `foreground` is the terminal's foreground process group, `None` when it cannot be read
    /// (always, on Windows); `shell` is the session's own process group.
    pub(crate) fn poll(&mut self, foreground: Option<u32>, shell: u32, now: Instant) -> Update {
        let Some(group) = foreground else {
            return Update::Nothing;
        };
        if group == shell {
            self.shell_polls = self.shell_polls.saturating_add(1);
            if self.shell_polls >= 2 && self.group.take().is_some() {
                self.pending = None;
                self.last_read = None;
                self.parked = None;
                if self.current.take().is_some() {
                    self.late_until = Some(now + LATE_REPORT_WINDOW);
                    return Update::ShellAgain;
                }
            }
            return Update::Nothing;
        }

        self.shell_polls = 0;
        self.late_until = None;
        let mut replaced = false;
        if self.group != Some(group) {
            if self.parked.as_ref().is_some_and(|(_, program)| leader_gone(program.pid)) {
                self.parked = None;
            }
            // A child group that took the terminal from a live program has given it back:
            // the program is the same one, and keeps its record.
            if self.parked.as_ref().is_some_and(|(parked, _)| *parked == group) {
                let (_, program) = self.parked.take().expect("checked above");
                self.group = Some(group);
                self.current = Some(program.clone());
                self.pending = Some(program.clone());
                self.settled = true;
                self.last_read = Some(now);
                return Update::Named(program);
            }
            // The named leader is gone and another group has the terminal: the named
            // program is over. A leader still alive may just have handed the terminal to
            // a child group, which keeps the record, and is remembered for its return.
            replaced = self.current.as_ref().is_some_and(|named| leader_gone(named.pid));
            if !replaced
                && self.parked.is_none()
                && let (Some(from), Some(named)) = (self.group, self.current.as_ref())
            {
                self.parked = Some((from, named.clone()));
            }
            self.group = Some(group);
            self.pending = None;
            self.settled = false;
            self.last_read = None;
        }
        let interval = if self.settled {
            PROGRAM_SETTLED_REREAD
        } else {
            PROGRAM_REREAD
        };
        if self.last_read.is_some_and(|at| now.duration_since(at) < interval) {
            return Update::Nothing;
        }
        self.last_read = Some(now);

        let program = match read_program(group) {
            Some(program) => program,
            None => {
                // A leader we already named has gone: keep what we have.
                if self.current.as_ref().is_some_and(|named| named.pid == group) {
                    self.settled = true;
                    return Update::Nothing;
                }
                ForegroundProgram {
                    pid: group,
                    name: String::new(),
                    argv: Vec::new(),
                    cwd: None,
                }
            }
        };
        // Two identical reads in a row settle the group; a different one unsettles it.
        self.settled = self.pending.as_ref() == Some(&program);
        self.pending = Some(program.clone());
        if !replaced && self.current.as_ref() == Some(&program) {
            return Update::Nothing;
        }
        self.current = Some(program.clone());
        if replaced {
            Update::Replaced(program)
        } else {
            Update::Named(program)
        }
    }
}

/// Whether the process `pid` no longer exists. A process we may not signal still exists.
#[cfg(unix)]
fn leader_gone(pid: u32) -> bool {
    let Ok(raw) = libc::pid_t::try_from(pid) else {
        return false;
    };
    unsafe {
        libc::kill(raw, 0) == -1
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
    }
}

#[cfg(not(unix))]
fn leader_gone(_pid: u32) -> bool {
    false
}

/// Keep at most [`MAX_ARG_BYTES`] bytes of `text`, cut on a char boundary.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn cap_text(mut text: String) -> String {
    if text.len() > MAX_ARG_BYTES {
        let mut end = MAX_ARG_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

/// Keep the first entries of `argv` that fit [`MAX_ARGV_BYTES`] in all: the entry that crosses
/// the budget is cut on a char boundary, and nothing follows it.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn cap_argv(argv: Vec<String>) -> Vec<String> {
    let mut left = MAX_ARGV_BYTES;
    let mut kept = Vec::with_capacity(argv.len());
    for mut arg in argv {
        if left == 0 {
            break;
        }
        if arg.len() > left {
            let mut end = left;
            while !arg.is_char_boundary(end) {
                end -= 1;
            }
            arg.truncate(end);
            kept.push(arg);
            break;
        }
        left -= arg.len();
        kept.push(arg);
    }
    kept
}

/// A working directory is kept whole or not at all: a cut path names another directory.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn keep_cwd(path: String) -> Option<String> {
    (path.len() <= MAX_ARG_BYTES).then_some(path)
}

/// Read the process `pid`: its name, argv and working directory. `None` when it is gone.
/// Never panics on whatever the operating system returns.
#[cfg(target_os = "linux")]
pub(crate) fn read_program(pid: u32) -> Option<ForegroundProgram> {
    use std::io::Read;

    let comm = std::fs::read(format!("/proc/{pid}/comm")).ok()?;
    let comm = String::from_utf8_lossy(&comm);
    let name = cap_text(comm.trim_end_matches('\n').to_string());

    // argv is NUL-separated; a process may set an enormous one, so read a bounded prefix.
    let mut raw = Vec::new();
    if let Ok(file) = std::fs::File::open(format!("/proc/{pid}/cmdline")) {
        let _ = file.take((MAX_ARGS * MAX_ARG_BYTES * 2) as u64).read_to_end(&mut raw);
    }
    let mut argv: Vec<String> = raw
        .split(|byte| *byte == 0)
        .take(MAX_ARGS + 1)
        .map(|arg| cap_text(String::from_utf8_lossy(arg).into_owned()))
        .collect();
    // The kernel ends cmdline with a NUL, so the split ends with one empty piece.
    if argv.last().is_some_and(String::is_empty) {
        argv.pop();
    }
    argv.truncate(MAX_ARGS);
    let argv = cap_argv(argv);

    let cwd = std::fs::read_link(format!("/proc/{pid}/cwd"))
        .ok()
        .and_then(|path| keep_cwd(path.to_string_lossy().into_owned()));
    Some(ForegroundProgram {
        pid,
        name,
        argv,
        cwd,
    })
}

/// macOS: `proc_name` for the name, `sysctl(KERN_PROCARGS2)` for argv, `proc_pidinfo` for
/// the working directory. Every byte the kernel returns is parsed with checked slicing.
#[cfg(target_os = "macos")]
pub(crate) fn read_program(pid: u32) -> Option<ForegroundProgram> {
    let raw_pid = libc::c_int::try_from(pid).ok()?;

    let mut name_buf = [0u8; 256];
    let written = unsafe {
        libc::proc_name(
            raw_pid,
            name_buf.as_mut_ptr().cast(),
            name_buf.len() as u32,
        )
    };
    let written = usize::try_from(written).ok().filter(|len| *len > 0)?;
    let name = cap_text(
        String::from_utf8_lossy(name_buf.get(..written.min(name_buf.len()))?).into_owned(),
    );

    let argv = read_argv(raw_pid).unwrap_or_default();

    let cwd = read_cwd(raw_pid);
    Some(ForegroundProgram {
        pid,
        name,
        argv,
        cwd,
    })
}

/// The arguments of `pid`, from the `KERN_PROCARGS2` buffer.
#[cfg(target_os = "macos")]
fn read_argv(pid: libc::c_int) -> Option<Vec<String>> {
    // The buffer is KERN_ARGMAX bytes (what the kernel can ever return), bounded here.
    let mut argmax: libc::c_int = 0;
    let mut size = std::mem::size_of::<libc::c_int>();
    let mut mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            (&mut argmax as *mut libc::c_int).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    let capacity = usize::try_from(argmax).ok()?.clamp(4096, 4 << 20);

    let mut buf = vec![0u8; capacity];
    let mut len = buf.len();
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    parse_procargs2(buf.get(..len)?)
}

/// The layout assumed, from the kernel's `KERN_PROCARGS2` reply: `argc` as a native `i32`,
/// the executable path and a NUL, any number of padding NULs, then `argc` NUL-terminated
/// arguments, then the environment (which is never read).
#[cfg(target_os = "macos")]
fn parse_procargs2(buf: &[u8]) -> Option<Vec<String>> {
    let argc = i32::from_ne_bytes(buf.get(..4)?.try_into().ok()?);
    let argc = usize::try_from(argc).ok()?;
    let rest = buf.get(4..)?;
    let after_path = rest.get(rest.iter().position(|byte| *byte == 0)?..)?;
    let args = after_path.get(after_path.iter().position(|byte| *byte != 0)?..)?;
    Some(cap_argv(
        args.split(|byte| *byte == 0)
            .take(argc.min(MAX_ARGS))
            .map(|arg| cap_text(String::from_utf8_lossy(arg).into_owned()))
            .collect(),
    ))
}

#[cfg(target_os = "macos")]
fn read_cwd(pid: libc::c_int) -> Option<String> {
    let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_vnodepathinfo>();
    let got = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            (&mut info as *mut libc::proc_vnodepathinfo).cast(),
            size as libc::c_int,
        )
    };
    if usize::try_from(got).ok()? != size {
        return None;
    }
    // libc declares the path as [[c_char; 32]; 32], which is MAXPATHLEN bytes.
    let bytes: Vec<u8> = info
        .pvi_cdir
        .vip_path
        .iter()
        .flatten()
        .map(|byte| *byte as u8)
        .take_while(|byte| *byte != 0)
        .collect();
    if bytes.is_empty() {
        return None;
    }
    keep_cwd(String::from_utf8_lossy(&bytes).into_owned())
}

/// Other Unixes have no reader yet; the poll then names every program with an empty name.
#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
pub(crate) fn read_program(_pid: u32) -> Option<ForegroundProgram> {
    None
}

/// Windows has no foreground process group to read.
#[cfg(windows)]
pub(crate) fn read_program(_pid: u32) -> Option<ForegroundProgram> {
    None
}
