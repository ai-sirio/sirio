//! Runs one chat shell command (`!cmd`): the user's shell in a session of its own, its output
//! streamed as events, and nothing of it left running when the block ends.
//!
//! Two threads serve a run. The reader copies the merged output of the shell (one pipe holds both
//! its stdout and its stderr, so the lines keep their order) into the events channel. The waiter
//! waits for the shell, gives the reader a moment to drain the pipe, kills whatever the shell left
//! in its group (a background job holds the pipe open, and would hold the block with it), and ends
//! the stream with [`ShellEvent::Exited`].
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
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use async_channel::{Receiver, Sender};

/// The most one [`ShellEvent::Output`] carries.
const CHUNK: usize = 8 * 1024;

/// How many events wait in the channel before the reader holds the command back.
const QUEUE: usize = 32;

/// How long the reader has, after the shell exits, to drain what is buffered in the pipe before
/// the leftover is killed.
const GRACE: Duration = Duration::from_millis(200);

/// How long the reader has, after the leftover is killed, to reach the end of the pipe. A process
/// outside the group (a daemon that called `setsid`) can hold the pipe open for good, and the block
/// ends without waiting for it.
const DRAIN: Duration = Duration::from_secs(2);

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
/// - Windows: `%COMSPEC%`, else `cmd.exe`. The argument is `/C`.
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
        (shell, vec![OsString::from("/C")])
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

/// One running shell command. Dropping it kills the command's whole group; the events of a dropped
/// run still arrive, ending in `Exited { stopped: true, .. }`.
pub struct ShellRun {
    shared: Arc<Shared>,
    events: Receiver<ShellEvent>,
}

impl ShellRun {
    /// Starts `command` in the user's shell, with `cwd` as its working directory.
    ///
    /// The shell has no stdin, `TERM=dumb` and no git prompt. On Unix it leads a session of its own
    /// (so it has no controlling terminal to open `/dev/tty` on, and the group is `pid`'s); on
    /// Windows it runs in a job object that kills what is in it when it closes.
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
            // cmd.exe does not read its command line the way `arg` quotes it.
            shell_command.raw_arg(command);
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
            // leader of a session and of a group of its own, which `killpg` reaches whole.
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

        #[cfg(windows)]
        let job = match Job::adopt(&child) {
            Ok(job) => job,
            Err(error) => {
                let mut child = child;
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let shared = Arc::new(Shared {
            pid: child.id(),
            life: Mutex::new(Life::default()),
            finished: Condvar::new(),
            #[cfg(windows)]
            job,
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

    /// The shell's process id (on Unix, also its process group's and its session's).
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.shared.pid
    }

    /// Asks the command to end: SIGTERM to its whole group, and SIGKILL after 2 s if the shell has
    /// not ended by then. On Windows the job is terminated at once. Never waits.
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

impl Drop for ShellRun {
    /// Kills the group at once. Never waits: the waiter thread cleans up on its own.
    fn drop(&mut self) {
        let mut life = self.shared.life();
        if !life.finished {
            life.stopped = true;
            // On Windows this is `TerminateJobObject`, not left to the job's last handle closing:
            // the waiter thread holds a copy of the handle.
            self.shared.signal(&life, Signal::Kill);
        }
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
    #[cfg(windows)]
    job: Job,
}

#[derive(Default)]
struct Life {
    /// The shell has been reaped and the leftover killed. Nothing is signalled after that: on Unix
    /// the number that named the group is free to name an unrelated process group, and a chat block
    /// can be dropped hours later.
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

    /// Signals the shell's group, if it is still the shell's: the caller holds the lock, so no
    /// reap can slip in between the check and the signal.
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
            // SAFETY: a plain syscall. Its error (ESRCH, a group that is already empty) is not one
            // there is anything to do about.
            unsafe { libc::killpg(self.pid as libc::pid_t, signal) };
        }
        #[cfg(windows)]
        {
            let _ = signal;
            self.job.terminate();
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
                    return;
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

    /// Kills what the shell left in its group, reaps the shell and lets signals stop.
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

/// Where the reader and the waiter put events, so that `Exited` is the last one.
struct Outbox {
    events: Sender<ShellEvent>,
    /// True once `Exited` is on its way; held by the reader across a send.
    sealed: Mutex<bool>,
}

impl Outbox {
    /// Queues a chunk, waiting for room if the consumer is behind. After `finish` it is dropped:
    /// the reader goes on draining the pipe, so a process that outlived the block (and still holds
    /// the pipe) does not die of a broken pipe at its next write.
    fn output(&self, chunk: Vec<u8>) {
        let sealed = self.sealed.lock().unwrap_or_else(PoisonError::into_inner);
        if !*sealed {
            // An error is a consumer that left; the command goes on all the same.
            let _ = self.events.send_blocking(ShellEvent::Output(chunk));
        }
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
            Ok(read) => outbox.output(buffer[..read].to_vec()),
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
    let status = shared.end(&mut child);
    reader_ended(reader, DRAIN);
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

// macOS picks /bin/zsh, which always exists, so this is the Linux and BSD rule.
#[cfg(all(test, unix, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn shell_choice_falls_back_from_bash_to_sh() {
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
}
