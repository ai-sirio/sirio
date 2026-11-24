//! Runs one chat shell command (`!cmd`): the user's shell in a session of its own, its output
//! streamed as events, and nothing of it left running when the block ends.
//!
//! Two threads serve a run. The reader copies the merged output of the shell (one pipe holds both
//! its stdout and its stderr, so the lines keep their order) into the events channel. The waiter
//! waits for the shell, gives the reader a moment to drain the pipe, kills whatever the shell left
//! in its session (a background job holds the pipe open, and would hold the block with it), and
//! ends the stream with [`ShellEvent::Exited`].
//!
//! On Unix the shell is a session leader, and "the command" is every process of that session: a
//! program the shell runs can leave the shell's process group for one of its own (coreutils
//! `timeout` does) and stay in the session. Only a program that starts a session of its own, as a
//! daemon does, is out of reach; a command finishes when its shell exits, so the block ends without
//! waiting for one, even when it keeps the pipe open and keeps writing to it.
//!
//! The channel is bounded, so a consumer that is slower than a command (`yes`) holds the command
//! back through the full pipe instead of growing memory without limit. The same bound means a
//! consumer that never drains it holds the run's threads: it is expected to drain the channel.

#[cfg(unix)]
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use async_channel::{Receiver, Sender};

/// The most one [`ShellEvent::Output`] carries.
const CHUNK: usize = 8 * 1024;

/// How many events wait in the channel before the reader holds the command back.
const QUEUE: usize = 32;

/// How long the reader has, after the shell exits, to drain what is buffered in the pipe before
/// the leftover is killed.
const GRACE: Duration = Duration::from_millis(200);

/// How long the reader has, after the leftover is killed, to reach the end of the pipe. A process
/// outside the session (a daemon that called `setsid`) can hold the pipe open for good, silent or
/// not, and a command finishes when its shell exits: the block ends without waiting for it. Time
/// the reader spends handing a chunk to a consumer that is behind is not counted, so the tail of
/// the output (at most one pipe buffer and the chunk in hand) reaches a slow consumer.
const DRAIN: Duration = Duration::from_secs(2);

/// The most the reader may read, after the leftover is killed, before the block ends without it:
/// what bounds a stranger that writes fast while the consumer is slow, when the clock above is
/// paused. Far above what a pipe buffer holds.
const DRAIN_BYTES: u64 = 1024 * 1024;

/// How often the waiter looks at what the reader is doing while it waits for it.
const DRAIN_STEP: Duration = Duration::from_millis(50);

/// How often the waiter looks again at a shell that `waitid` reports as stopped, not exited.
#[cfg(unix)]
const STOPPED_POLL: Duration = Duration::from_millis(50);

/// How long a shell has to die of the SIGTERM [`ShellRun::stop`] sends before it gets a SIGKILL.
#[cfg(unix)]
const STOP_GRACE: Duration = Duration::from_secs(2);

/// What a run reports. `Exited` is the last event: it follows every `Output`, and the channel is
/// closed behind it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellEvent {
    /// The next bytes of the merged output, cut anywhere (a UTF-8 character or an escape sequence
    /// can be split across two chunks).
    Output(Vec<u8>),
    /// The shell ended. `code` is its exit status; on Unix a signal's death is `128 + signal`, the
    /// shell's own `$?`, so a run that started always has one. `-1` is the one made-up value: the
    /// system could not say how the shell ended. `stopped` is whether [`ShellRun::stop`] (or
    /// dropping the run) came before the shell ended.
    Exited { code: Option<i32>, stopped: bool },
}

/// The shell and the arguments that make it run one command: the command text goes after them.
///
/// - Unix: `$SHELL` when it is set and not empty, else `/bin/zsh` on macOS, else the first of
///   `/bin/bash` and `/bin/sh` that exists. The argument is `-c`.
/// - Windows: `%COMSPEC%`, else `cmd.exe`. The arguments are `/D` (no AutoRun commands before
///   ours), `/S` and `/C`; [`ShellRun::start`] puts the command in quotes after them, which is the
///   one pair of quotes `/S` takes off, so a command that starts with a quote of its own is read
///   as written.
#[must_use]
pub fn shell_invocation() -> (OsString, Vec<OsString>) {
    #[cfg(unix)]
    {
        let shell = choose_shell(std::env::var_os("SHELL").as_deref(), |path| path.exists());
        (shell, vec![OsString::from("-c")])
    }
    #[cfg(windows)]
    {
        let shell = std::env::var_os("COMSPEC")
            .filter(|shell| !shell.is_empty())
            .unwrap_or_else(|| OsString::from("cmd.exe"));
        let args = ["/D", "/S", "/C"].map(OsString::from);
        (shell, args.to_vec())
    }
}

/// The shell to run when the environment says `env_shell`; `exists` is the file system's word on
/// the fallbacks.
#[cfg(unix)]
fn choose_shell(env_shell: Option<&OsStr>, exists: impl Fn(&Path) -> bool) -> OsString {
    if let Some(shell) = env_shell.filter(|shell| !shell.is_empty()) {
        return shell.to_owned();
    }
    let fallbacks: &[&str] = if cfg!(target_os = "macos") {
        &["/bin/zsh", "/bin/bash", "/bin/sh"]
    } else {
        &["/bin/bash", "/bin/sh"]
    };
    // A system with neither still gets a name that fails to start with a message, not a panic.
    let chosen = fallbacks
        .iter()
        .find(|path| exists(Path::new(path)))
        .or(fallbacks.last());
    OsString::from(chosen.copied().unwrap_or("/bin/sh"))
}

/// One running shell command. Dropping it kills the command's whole session; the events of a
/// dropped run still arrive, ending in `Exited { stopped: true, .. }`.
pub struct ShellRun {
    shared: Arc<Shared>,
    events: Receiver<ShellEvent>,
}

impl ShellRun {
    /// Starts `command` in the user's shell, with `cwd` as its working directory.
    ///
    /// The shell has no stdin, `TERM=dumb` and no git prompt. On Unix it leads a session of its own
    /// (so it has no controlling terminal to open `/dev/tty` on, and the group is `pid`'s); on
    /// Windows it runs in a job object that kills what is in it when it closes (or, if the job
    /// could not be made, on its own: a stop then reaches the shell only).
    ///
    /// # Errors
    /// When the pipe, the process or its threads cannot be created: the shell is missing, `cwd` is
    /// not a directory, and so on. Nothing is left running.
    pub fn start(command: &str, cwd: &Path) -> io::Result<Self> {
        let (shell, args) = shell_invocation();
        let (reader, writer) = io::pipe()?;
        let (events_tx, events) = async_channel::bounded(QUEUE);
        let outbox = Arc::new(Outbox {
            events: events_tx,
            sealed: Mutex::new(false),
            read_bytes: AtomicU64::new(0),
            sending: AtomicBool::new(false),
        });
        let (reader_done, reader_ended) = mpsc::channel();
        let (handoff, handoff_rx) = mpsc::channel::<(Child, Arc<Shared>)>();

        // The threads go first: once the shell exists, nothing that can fail is left to do, so a
        // failure never has a process to clean up.
        let waiter_outbox = Arc::clone(&outbox);
        thread::Builder::new().name("shell-wait".into()).spawn(move || {
            if let Ok((child, shared)) = handoff_rx.recv() {
                wait_for_shell(&shared, child, &reader_ended, &waiter_outbox);
            }
        })?;
        thread::Builder::new()
            .name("shell-read".into())
            .spawn(move || read_output(reader, &outbox, reader_done))?;

        let mut shell_command = sirio_env::command(&shell);
        shell_command.args(&args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // cmd.exe does not read its command line the way `arg` quotes it. With `/S` it takes
            // off the first quote and the last one, and runs what is between them as written.
            shell_command.raw_arg(format!("\"{command}\""));
            shell_command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
        }
        #[cfg(not(windows))]
        shell_command.arg(command);
        shell_command
            .current_dir(cwd)
            .env("TERM", "dumb")
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(writer.try_clone()?)
            .stderr(writer);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // SAFETY: `setsid` is async-signal-safe and touches no memory. It makes the shell the
            // leader of a session and of a group of its own: the session is what a stop reaches.
            unsafe {
                shell_command.pre_exec(|| {
                    if libc::setsid() == -1 {
                        Err(io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                });
            }
        }
        let child = shell_command.spawn()?;
        // The command holds the parent's copies of the pipe's writer: until it is gone the reader
        // cannot see the end of the output.
        drop(shell_command);

        // The command is running by now, and a run that could not be put in a job (a very fast
        // command can be gone before it is assigned) is still a run: it has no job to kill, and a
        // stop reaches the shell alone.
        #[cfg(windows)]
        let job = Job::adopt(&child).ok();
        let shared = Arc::new(Shared {
            pid: child.id(),
            life: Mutex::new(Life::default()),
            finished: Condvar::new(),
            #[cfg(windows)]
            job,
            #[cfg(windows)]
            process: Process::of(&child),
        });
        // The waiter thread is blocked on this message, so the send fails only if it has died.
        if let Err(lost) = handoff.send((child, Arc::clone(&shared))) {
            let (mut child, _) = lost.0;
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other("the thread that waits for the shell is gone"));
        }
        Ok(Self { shared, events })
    }

    /// The events of this run: every `Output`, then `Exited`. A clone of one channel, so clones
    /// share the events between them.
    #[must_use]
    pub fn events(&self) -> Receiver<ShellEvent> {
        self.events.clone()
    }

    /// The shell's process id (on Unix, also its process group's and its session's id).
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.shared.pid
    }

    /// Asks the command to end: SIGTERM to its whole session, and SIGKILL after 2 s if the shell
    /// has not ended by then. On Windows the job is terminated at once (the shell alone, for a run
    /// that has no job). Never waits.
    pub fn stop(&self) {
        let mut life = self.shared.life();
        if life.finished {
            return;
        }
        life.stopped = true;
        self.shared.signal(&life, Signal::Term);
        #[cfg(unix)]
        if !std::mem::replace(&mut life.escalating, true) {
            drop(life);
            Shared::escalate_later(&self.shared);
        }
    }
}

impl ShellRun {
    /// Kills the command's whole session at once, with no SIGTERM first and no wait. It is what the
    /// chat's Escape does to a command that was asked to stop and is still there (the SIGKILL of
    /// [`Self::stop`] comes 2 s after it, and a second `stop` only repeats the SIGTERM), what it
    /// does to a run that has no block left to say so, and what [`Drop`] does. The run still ends
    /// with `Exited { stopped: true, .. }`. Does nothing once the command has ended.
    pub fn kill(&self) {
        let mut life = self.shared.life();
        if !life.finished {
            life.stopped = true;
            // On Windows this is `TerminateJobObject` (`TerminateProcess` on the shell for a run
            // without a job), not left to the job's last handle closing: the waiter thread holds
            // a copy of the handle.
            self.shared.signal(&life, Signal::Kill);
        }
    }
}

impl Drop for ShellRun {
    /// Kills the whole session at once. Never waits (a scan of the processes takes a few
    /// milliseconds): the waiter thread cleans up on its own.
    fn drop(&mut self) {
        self.kill();
    }
}

#[derive(Clone, Copy)]
enum Signal {
    Term,
    Kill,
}

/// What `ShellRun` and the threads of its run share.
struct Shared {
    pid: u32,
    life: Mutex<Life>,
    /// Notified when `life.finished` turns true.
    finished: Condvar,
    /// What a stop and a leftover kill reach on Windows. `None` when the shell could not be put in
    /// a job.
    #[cfg(windows)]
    job: Option<Job>,
    /// The shell's own handle, for a run without a job. It is the waiter's `Child` handle, valid
    /// for as long as `life.finished` is false.
    #[cfg(windows)]
    process: Process,
}

#[derive(Default)]
struct Life {
    /// The shell has been reaped and the leftover killed. Nothing is signalled after that: on Unix
    /// the number that named the session is free to name an unrelated process group, and a chat
    /// block can be dropped hours later.
    finished: bool,
    /// [`ShellRun::stop`] or the drop came while the shell was still there to be stopped.
    stopped: bool,
    /// A thread is already waiting out the grace of a SIGTERM.
    #[cfg(unix)]
    escalating: bool,
}

impl Shared {
    fn life(&self) -> MutexGuard<'_, Life> {
        // A panic elsewhere must not stop a drop from killing the group.
        self.life.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Signals the command, if it is still the shell's: the caller holds the lock, so no reap can
    /// slip in between the check and the signal.
    ///
    /// On Unix the command is the shell's group (it, and whatever kept that group) and every
    /// process of its session, found by a scan each time: a program that left the group for one of
    /// its own (`timeout` does) is in the session still. The shell's pid is held by the unreaped
    /// shell for as long as `finished` is false, and so is the session's id.
    fn signal(&self, life: &Life, signal: Signal) {
        if life.finished {
            return;
        }
        #[cfg(unix)]
        {
            let signal = match signal {
                Signal::Term => libc::SIGTERM,
                Signal::Kill => libc::SIGKILL,
            };
            let leader = self.pid as libc::pid_t;
            if leader <= 1 {
                // Not a shell of ours, and `killpg(0, ..)` would be the caller's own group.
                return;
            }
            // SAFETY: plain syscalls. Their errors (ESRCH, a group or a process that is already
            // gone) are not ones there is anything to do about.
            unsafe { libc::killpg(leader, signal) };
            for member in session_members(leader) {
                // SAFETY: as above; `session_members` gives only pids above 1 other than ours.
                unsafe { libc::kill(member, signal) };
            }
        }
        #[cfg(windows)]
        {
            let _ = signal;
            match &self.job {
                Some(job) => job.terminate(),
                // No job: only the shell is reachable. Once it has exited this fails, and that is
                // the whole of the leftover kill for such a run.
                None => self.process.terminate(),
            }
        }
    }

    /// Blocks until the shell has ended, without reaping it where the system lets that wait: a
    /// zombie shell keeps its pid, and so its group's number, reserved while the leftover is
    /// killed.
    fn wait_exited(&self, child: &mut Child) {
        #[cfg(unix)]
        {
            let _ = child;
            // SAFETY: an all-zero `siginfo_t` is a valid one for `waitid` to fill in.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            loop {
                // SAFETY: a plain syscall on a child of this process.
                let waited = unsafe {
                    libc::waitid(
                        libc::P_PID,
                        self.pid as libc::id_t,
                        &raw mut info,
                        libc::WEXITED | libc::WNOWAIT,
                    )
                };
                if waited == 0 {
                    let code = info.si_code;
                    if matches!(code, libc::CLD_EXITED | libc::CLD_KILLED | libc::CLD_DUMPED) {
                        return;
                    }
                    // A stop or a continue is not an exit: macOS reports them even without
                    // WSTOPPED (golang/go#19314). WNOWAIT leaves the event there to be reported
                    // again, so wait a moment rather than spin.
                    thread::sleep(STOPPED_POLL);
                    continue;
                }
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                // The shell is not ours to wait for any more (SIGCHLD ignored, so it was reaped on
                // its own): its number may be anyone's.
                let mut life = self.life();
                life.finished = true;
                self.finished.notify_all();
                return;
            }
        }
        #[cfg(windows)]
        {
            let _ = child.wait();
        }
    }

    /// Kills what the shell left in its session, reaps the shell and lets signals stop.
    fn end(&self, child: &mut Child) -> io::Result<ExitStatus> {
        let mut life = self.life();
        if life.finished {
            // `wait_exited` gave up, so there is nothing to signal and nothing worth the lock.
            drop(life);
            return child.wait();
        }
        self.signal(&life, Signal::Kill);
        // The shell has exited, so this returns at once.
        let status = child.wait();
        life.finished = true;
        self.finished.notify_all();
        status
    }

    /// Sends SIGKILL if the shell is still there after [`STOP_GRACE`], from a thread of its own, so
    /// `stop` never waits.
    #[cfg(unix)]
    fn escalate_later(this: &Arc<Self>) {
        let shared = Arc::clone(this);
        let spawned = thread::Builder::new().name("shell-stop".into()).spawn(move || {
            let life = shared.life();
            let (life, _) = shared
                .finished
                .wait_timeout_while(life, STOP_GRACE, |life| !life.finished)
                .unwrap_or_else(PoisonError::into_inner);
            shared.signal(&life, Signal::Kill);
        });
        if spawned.is_err() {
            // No thread to wait with: a stop that cannot escalate is a kill.
            let life = this.life();
            this.signal(&life, Signal::Kill);
        }
    }
}

/// Every process of the session `sid` that the shell's own `killpg` does not reach: the session's
/// members outside the group `sid`, with the shell and `sid`'s group left out, and never this
/// process or pid 1.
///
/// Found by a scan of every process, now: a session's members are not kept anywhere, and the ones
/// that matter (a program that moved to a group of its own) are not the shell's children for long.
/// A process that exits during the scan is simply missed or already gone. The unreaped shell holds
/// `sid` (see `Life::finished`), so no other session can take the session's number; the members'
/// own pids are not held by anything. A member that exits, is reaped and has its pid reused by an
/// unrelated process between the scan and the kill would be signalled in its place. That takes the
/// whole pid space turning over within the milliseconds between the two, and it is accepted.
#[cfg(target_os = "linux")]
fn session_members(sid: libc::pid_t) -> Vec<libc::pid_t> {
    let Ok(processes) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    // SAFETY: `getpid` cannot fail.
    let me = unsafe { libc::getpid() };
    let mut members = Vec::new();
    for process in processes.flatten() {
        let name = process.file_name();
        let Some(pid) = name.to_str().and_then(|name| name.parse::<libc::pid_t>().ok()) else {
            continue;
        };
        if pid <= 1 || pid == me || pid == sid {
            continue;
        }
        // A process that exits during the scan has no stat file any more.
        let Ok(stat) = std::fs::read(format!("/proc/{pid}/stat")) else {
            continue;
        };
        if let Some((group, session)) = parse_stat(&String::from_utf8_lossy(&stat))
            && session == sid
            && group != sid
        {
            members.push(pid);
        }
    }
    members
}

/// The process group and the session of a process, from its `/proc/<pid>/stat`:
/// `pid (comm) state ppid pgrp session ...`. The name is whatever the process called itself,
/// spaces and parentheses included, so the fields are counted from the last `)`.
#[cfg(target_os = "linux")]
fn parse_stat(stat: &str) -> Option<(libc::pid_t, libc::pid_t)> {
    let after_name = &stat[stat.rfind(')')? + 1..];
    let mut fields = after_name.split_ascii_whitespace().skip(2);
    let group = fields.next()?.parse().ok()?;
    let session = fields.next()?.parse().ok()?;
    Some((group, session))
}

/// macOS has no `/proc`: libproc lists the pids and `getsid` names the session of each one.
#[cfg(target_os = "macos")]
fn session_members(sid: libc::pid_t) -> Vec<libc::pid_t> {
    // SAFETY: with no buffer, `proc_listallpids` only says how many pids there are.
    let listed = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    let Ok(count) = usize::try_from(listed) else {
        return Vec::new();
    };
    // Room for the processes born between the two calls.
    let mut pids: Vec<libc::pid_t> = vec![0; count + count / 8 + 16];
    let bytes = (pids.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int;
    // SAFETY: the buffer is `bytes` long.
    let filled = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    // libproc reports the number of pids it copied, not a byte count.
    pids.truncate(usize::try_from(filled).unwrap_or(0));
    // SAFETY: `getpid` cannot fail.
    let me = unsafe { libc::getpid() };
    pids.into_iter()
        .filter(|&pid| pid > 1 && pid != me && pid != sid)
        // SAFETY: plain syscalls; a process that is gone or not ours to ask gives -1, which is
        // not `sid`.
        .filter(|&pid| unsafe { libc::getsid(pid) } == sid && unsafe { libc::getpgid(pid) } != sid)
        .collect()
}

/// Other Unixes have no scan here: a stop reaches the shell's group only.
#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn session_members(_sid: libc::pid_t) -> Vec<libc::pid_t> {
    Vec::new()
}

/// Where the reader and the waiter put events, so that `Exited` is the last one.
struct Outbox {
    events: Sender<ShellEvent>,
    /// True once `Exited` is on its way; held by the reader across a send.
    sealed: Mutex<bool>,
    /// How many bytes the reader has taken out of the pipe.
    read_bytes: AtomicU64,
    /// The reader is handing a chunk to the consumer, and may be held up by it.
    sending: AtomicBool,
}

impl Outbox {
    /// Queues a chunk, waiting for room if the consumer is behind. After `finish` it is dropped:
    /// the reader goes on draining the pipe, so a process that outlived the block (and still holds
    /// the pipe) does not die of a broken pipe at its next write.
    fn output(&self, chunk: Vec<u8>) {
        self.sending.store(true, Ordering::SeqCst);
        let sealed = self.sealed.lock().unwrap_or_else(PoisonError::into_inner);
        if !*sealed {
            // An error is a consumer that left; the command goes on all the same.
            let _ = self.events.send_blocking(ShellEvent::Output(chunk));
        }
        drop(sealed);
        self.sending.store(false, Ordering::SeqCst);
    }

    /// Queues the last event and closes the channel behind it. Waits for a chunk that is being
    /// sent, so none can come after it; what is already queued stays receivable.
    fn finish(&self, exited: ShellEvent) {
        *self.sealed.lock().unwrap_or_else(PoisonError::into_inner) = true;
        let _ = self.events.send_blocking(exited);
        self.events.close();
    }
}

/// The reader thread: the pipe's bytes into the outbox, until the pipe ends. `_done` is dropped
/// when this returns, which is how the waiter learns it ended.
fn read_output(mut pipe: io::PipeReader, outbox: &Outbox, _done: mpsc::Sender<()>) {
    let mut buffer = vec![0u8; CHUNK];
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => {
                outbox.read_bytes.fetch_add(read as u64, Ordering::SeqCst);
                outbox.output(buffer[..read].to_vec());
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

/// Whether the reader thread ended within `within`.
fn reader_ended(reader: &mpsc::Receiver<()>, within: Duration) -> bool {
    // The reader never sends: its end is its sender being dropped.
    matches!(reader.recv_timeout(within), Err(RecvTimeoutError::Disconnected))
}

/// Waits for the reader thread to end, up to the first of: its end (the pipe's), [`DRAIN`] of
/// wall clock that the reader did not spend handing a chunk to the consumer, and [`DRAIN_BYTES`]
/// read since `read_before`. A command finishes when its shell exits, so a process that outlived
/// the kill and holds the pipe (and may keep writing to it) does not hold the block.
fn wait_for_reader(reader: &mpsc::Receiver<()>, outbox: &Outbox, read_before: u64) {
    let mut left = DRAIN;
    let mut last = Instant::now();
    loop {
        // The reader never sends: its end is its sender being dropped.
        if !matches!(reader.recv_timeout(DRAIN_STEP), Err(RecvTimeoutError::Timeout)) {
            return;
        }
        let now = Instant::now();
        let step = now.duration_since(last);
        last = now;
        if !outbox.sending.load(Ordering::SeqCst) {
            left = left.saturating_sub(step);
        }
        let read = outbox.read_bytes.load(Ordering::SeqCst).saturating_sub(read_before);
        if left.is_zero() || read >= DRAIN_BYTES {
            return;
        }
    }
}

/// The waiter thread: from the shell's exit to the last event.
fn wait_for_shell(
    shared: &Shared,
    mut child: Child,
    reader: &mpsc::Receiver<()>,
    outbox: &Outbox,
) {
    shared.wait_exited(&mut child);
    // A stop that comes after the shell ended on its own does not relabel it.
    let stopped = shared.life().stopped;
    reader_ended(reader, GRACE);
    let read_before = outbox.read_bytes.load(Ordering::SeqCst);
    let status = shared.end(&mut child);
    wait_for_reader(reader, outbox, read_before);
    outbox.finish(ShellEvent::Exited {
        code: Some(exit_code(status)),
        stopped,
    });
}

/// The exit code of a shell, `128 + signal` when a signal ended it, and `-1` when the system did
/// not say.
fn exit_code(status: io::Result<ExitStatus>) -> i32 {
    let Ok(status) = status else { return -1 };
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    status.code().unwrap_or(-1)
}

/// The shell's process handle, borrowed from the `Child` the waiter holds.
#[cfg(windows)]
struct Process(windows_sys::Win32::Foundation::HANDLE);

// SAFETY: a process handle is a kernel handle, usable from any thread; `Shared::life` says for how
// long the `Child` it belongs to is there.
#[cfg(windows)]
unsafe impl Send for Process {}
#[cfg(windows)]
unsafe impl Sync for Process {}

#[cfg(windows)]
impl Process {
    fn of(child: &Child) -> Self {
        use std::os::windows::io::AsRawHandle;
        Self(child.as_raw_handle() as _)
    }

    fn terminate(&self) {
        // SAFETY: the caller holds `Shared::life` with `finished` false, so the `Child` that owns
        // the handle is alive. Terminating a process that has exited fails, harmlessly.
        unsafe { windows_sys::Win32::System::Threading::TerminateProcess(self.0, 1) };
    }
}

/// A job object that kills everything in it when its last handle closes, and on `terminate`.
#[cfg(windows)]
struct Job(windows_sys::Win32::Foundation::HANDLE);

// SAFETY: a job handle is a kernel handle, usable from any thread.
#[cfg(windows)]
unsafe impl Send for Job {}
#[cfg(windows)]
unsafe impl Sync for Job {}

#[cfg(windows)]
impl Job {
    fn adopt(child: &Child) -> io::Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };
        // SAFETY: plain Win32 calls on a handle made here and on the child's own.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(io::Error::last_os_error());
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let set = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if set == 0 || AssignProcessToJobObject(job, child.as_raw_handle() as _) == 0 {
                let error = io::Error::last_os_error();
                CloseHandle(job);
                return Err(error);
            }
            Ok(Self(job))
        }
    }

    fn terminate(&self) {
        // SAFETY: the handle is this job's own. A job with nothing left in it is fine to terminate.
        unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1) };
    }
}

#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the handle is this job's own and closed once.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(all(test, unix))]
mod tests {
    // macOS picks /bin/zsh, which always exists, so this is the Linux and BSD rule.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn shell_choice_falls_back_from_bash_to_sh() {
        use super::choose_shell;
        use std::ffi::OsStr;
        use std::path::Path;

        let has_bash = |path: &Path| path == Path::new("/bin/bash");
        let has_neither = |_: &Path| false;
        assert_eq!(choose_shell(None, has_bash), OsStr::new("/bin/bash"));
        assert_eq!(choose_shell(None, has_neither), OsStr::new("/bin/sh"));
        assert_eq!(choose_shell(Some(OsStr::new("")), has_bash), OsStr::new("/bin/bash"));
        assert_eq!(choose_shell(Some(OsStr::new("")), has_neither), OsStr::new("/bin/sh"));
        assert_eq!(
            choose_shell(Some(OsStr::new("/usr/bin/fish")), has_bash),
            OsStr::new("/usr/bin/fish")
        );
    }

    /// The name of a process is whatever it called itself: spaces, parentheses and all.
    #[cfg(target_os = "linux")]
    #[test]
    fn proc_stat_is_read_from_the_last_parenthesis() {
        use super::parse_stat;

        assert_eq!(parse_stat("4242 (bash) S 4000 4242 4242 34816 4242"), Some((4242, 4242)));
        assert_eq!(parse_stat("77 (a) b (c d) R 1 55 66 0 -1 0"), Some((55, 66)));
        assert_eq!(parse_stat("77 (x) Z 1 2"), None);
        assert_eq!(parse_stat("77 no parentheses"), None);
    }
}
