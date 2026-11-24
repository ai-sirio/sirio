//! `ShellRun` against real processes: output, exit code, the session, and what is left behind.
//!
//! The runner picks the user's `$SHELL`, so these need a POSIX one (bash, zsh, dash). Under fish,
//! run them as `SHELL=/bin/sh cargo test -p sirio_chat --test shell_run`.

use std::path::Path;

use sirio_chat::shell::ShellRun;

#[cfg(unix)]
mod unix {
    use std::cell::Cell;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use async_channel::Receiver;
    use futures::future::{Either, select};
    use sirio_chat::shell::{ShellEvent, ShellRun};

    /// How long any one command may take to finish before a test calls it hung.
    const GUARD: Duration = Duration::from_secs(10);

    /// The next event within `within`, or why there is none.
    fn next(events: &Receiver<ShellEvent>, within: Duration) -> Result<ShellEvent, &'static str> {
        futures::executor::block_on(async {
            let received = std::pin::pin!(events.recv());
            match select(received, async_io::Timer::after(within)).await {
                Either::Left((event, _)) => event.map_err(|_| "the channel is closed"),
                Either::Right(_) => Err("no event in time"),
            }
        })
    }

    struct Finished {
        output: Vec<u8>,
        code: Option<i32>,
        stopped: bool,
    }

    /// Collects the output up to `Exited` within `within`, and checks that nothing follows it.
    fn finish(events: &Receiver<ShellEvent>, within: Duration) -> Finished {
        let deadline = Instant::now() + within;
        let mut output = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match next(events, left) {
                Ok(ShellEvent::Output(chunk)) => output.extend(chunk),
                Ok(ShellEvent::Exited { code, stopped }) => {
                    assert_eq!(
                        next(events, Duration::from_millis(500)),
                        Err("the channel is closed"),
                        "Exited must be the last event"
                    );
                    return Finished { output, code, stopped };
                }
                Err(why) => panic!(
                    "no Exited event ({why}); output so far: {:?}",
                    String::from_utf8_lossy(&output)
                ),
            }
        }
    }

    fn cwd() -> std::path::PathBuf {
        // `pwd` prints the physical path, which is not `temp_dir()`'s where /tmp is a link.
        std::env::temp_dir().canonicalize().expect("a temporary directory")
    }

    /// The pid a command prints on its first line.
    fn read_pid(events: &Receiver<ShellEvent>) -> i32 {
        let mut seen = Vec::new();
        while !seen.contains(&b'\n') {
            match next(events, GUARD) {
                Ok(ShellEvent::Output(chunk)) => seen.extend(chunk),
                other => panic!("expected a pid, got {other:?}"),
            }
        }
        String::from_utf8_lossy(&seen).trim().parse().expect("a pid")
    }

    /// A process a test must not leave behind. It is killed at the end of a test that panics,
    /// unless the test has seen it gone: a pid that is gone may be anyone's by now, and a test
    /// that passed has nothing to clean up.
    struct Stray {
        pid: i32,
        seen_gone: Cell<bool>,
    }

    impl Stray {
        fn new(pid: i32) -> Self {
            Self { pid, seen_gone: Cell::new(false) }
        }

        /// Whether the process is gone, giving init up to 2 s to reap a killed orphan: a zombie
        /// still answers `kill(pid, 0)`.
        fn is_gone(&self) -> bool {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                // SAFETY: signal 0 only checks that the pid exists.
                let alive = unsafe { libc::kill(self.pid, 0) } == 0
                    || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH);
                if !alive {
                    self.seen_gone.set(true);
                    return true;
                }
                if Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    impl Stray {
        /// Kills the process: for one that is out of everyone's reach but the test's.
        fn kill(&self) {
            // SAFETY: a plain syscall on a pid this test started and has just seen alive.
            unsafe { libc::kill(self.pid, libc::SIGKILL) };
        }
    }

    impl Drop for Stray {
        fn drop(&mut self) {
            if std::thread::panicking() && !self.seen_gone.get() {
                // SAFETY: a plain syscall on a pid this test started and has not seen gone.
                unsafe { libc::kill(self.pid, libc::SIGKILL) };
            }
        }
    }

    /// A file a command creates when it is ready, removed when the test ends.
    struct Marker(PathBuf);

    impl Marker {
        fn new(name: &str) -> Self {
            let file = format!("sirio-shell-run-{}-{name}", std::process::id());
            Self(std::env::temp_dir().join(file))
        }
    }

    impl Drop for Marker {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /// A command that moves out of the shell's process group into one of its own, stays in the
    /// shell's session (it does not call `setsid`: what stays in the session is what a stop must
    /// reach), and then sleeps; `timeout` does exactly this under a shell. It prints its pid, then
    /// creates `mark`. `None` when there is neither perl nor python3 to do it with.
    fn own_group_sleeper(mark: &Path) -> Option<String> {
        let has = |program: &str| {
            Command::new(program)
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        };
        let mark = mark.display();
        if has("perl") {
            Some(format!(
                concat!(
                    r#"perl -e 'setpgrp(0, 0); $| = 1; print "$$\n"; "#,
                    r#"open(F, ">", $ARGV[0]); close(F); exec "sleep", "300"' '{mark}'"#
                ),
                mark = mark
            ))
        } else if has("python3") {
            Some(format!(
                concat!(
                    r#"python3 -c 'import os, sys; os.setpgid(0, 0); "#,
                    r#"print(os.getpid(), flush=True); open(sys.argv[1], "w").close(); "#,
                    r#"os.execvp("sleep", ["sleep", "300"])' '{mark}'"#
                ),
                mark = mark
            ))
        } else {
            None
        }
    }

    /// What puts a command in a session of its own, as a daemon does: `setsid`, or perl where there
    /// is none (macOS). `None` when neither is here.
    fn detach_prefix() -> Option<&'static str> {
        let has = |program: &str, flag: &str| {
            Command::new(program)
                .arg(flag)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        };
        if has("setsid", "--version") {
            Some("setsid")
        } else if has("perl", "--version") {
            Some("perl -MPOSIX -e 'POSIX::setsid(); exec @ARGV or die' --")
        } else {
            None
        }
    }

    #[test]
    fn runs_in_cwd_with_merged_ordered_output_and_exit_code() {
        let cwd = cwd();
        let run = ShellRun::start("pwd; echo out; echo err 1>&2; exit 3", &cwd).expect("starts");
        let done = finish(&run.events(), GUARD);
        assert_eq!(
            String::from_utf8_lossy(&done.output),
            format!("{}\nout\nerr\n", cwd.display())
        );
        assert_eq!((done.code, done.stopped), (Some(3), false));
    }

    #[test]
    fn stdin_is_null() {
        let run = ShellRun::start("cat", &cwd()).expect("starts");
        let done = finish(&run.events(), Duration::from_secs(2));
        assert_eq!((done.code, done.stopped), (Some(0), false));
    }

    // `ps -o sid=` is what Linux's procps knows; macOS's `ps` has no keyword for a session id.
    #[cfg(target_os = "linux")]
    #[test]
    fn leads_its_own_session() {
        let run = ShellRun::start(r#"echo "$$ $(ps -o sid= -p $$)""#, &cwd()).expect("starts");
        let done = finish(&run.events(), GUARD);
        let text = String::from_utf8_lossy(&done.output);
        let numbers: Vec<&str> = text.split_whitespace().collect();
        assert_eq!(numbers.len(), 2, "pid and session id, got {text:?}");
        assert_eq!(numbers[0], numbers[1], "the shell is its session's leader");
    }

    #[test]
    fn stop_kills_the_whole_group() {
        let run = ShellRun::start("sleep 300 & echo $!; sleep 300", &cwd()).expect("starts");
        let events = run.events();
        let child = Stray::new(read_pid(&events));
        run.stop();
        let done = finish(&events, Duration::from_secs(3));
        // A signal's death is the shell's own `$?`: 128 plus the signal.
        assert_eq!((done.code, done.stopped), (Some(128 + libc::SIGTERM), true));
        assert!(Stray::new(run.pid() as i32).is_gone(), "the shell is gone");
        assert!(child.is_gone(), "the background sleep is gone");
    }

    #[test]
    fn drop_kills_the_whole_group() {
        let run = ShellRun::start("sleep 300 & echo $!; sleep 300", &cwd()).expect("starts");
        let events = run.events();
        let child = Stray::new(read_pid(&events));
        let shell = Stray::new(run.pid() as i32);
        drop(run);
        let done = finish(&events, Duration::from_secs(3));
        assert_eq!((done.code, done.stopped), (Some(128 + libc::SIGKILL), true));
        assert!(shell.is_gone(), "the shell is gone");
        assert!(child.is_gone(), "the background sleep is gone");
    }

    #[test]
    fn background_job_does_not_hold_the_block() {
        let run = ShellRun::start("sleep 30 & echo $!", &cwd()).expect("starts");
        let events = run.events();
        let child = Stray::new(read_pid(&events));
        let done = finish(&events, Duration::from_secs(1));
        assert_eq!((done.code, done.stopped), (Some(0), false));
        assert!(child.is_gone(), "the background sleep is gone");
    }

    #[test]
    fn stop_reaches_a_child_in_a_group_of_its_own() {
        let mark = Marker::new("stop");
        let Some(sleeper) = own_group_sleeper(&mark.0) else {
            eprintln!("SKIP: neither perl nor python3 is here to leave the shell's group");
            return;
        };
        // The `; echo done` keeps the shell from running the sleeper in its own place.
        let run = ShellRun::start(&format!("{sleeper}; echo done"), &cwd()).expect("starts");
        let events = run.events();
        let child = Stray::new(read_pid(&events));
        run.stop();
        let done = finish(&events, Duration::from_secs(3));
        assert!(done.stopped);
        assert!(child.is_gone(), "the child in a group of its own is gone");
    }

    #[test]
    fn a_stranger_that_keeps_writing_does_not_hold_the_block() {
        let mark = Marker::new("stranger");
        let Some(detach) = detach_prefix() else {
            eprintln!("SKIP: neither setsid nor perl is here to start a session");
            return;
        };
        // A writer in a session of its own is out of reach of the sweep and holds the pipe for
        // good, saying something every 0.1 s. It tells its pid through a file, because the pipe
        // is shared; the shell waits for that, then ends.
        let command = format!(
            "{detach} sh -c 'echo $$ > \"$0\"; while :; do echo x; sleep 0.1; done' '{mark}' & \
             until [ -s '{mark}' ]; do sleep 0.02; done; echo hi",
            mark = mark.0.display()
        );
        let run = ShellRun::start(&command, &cwd()).expect("starts");
        let events = run.events();
        let deadline = Instant::now() + GUARD;
        let stranger = loop {
            match std::fs::read_to_string(&mark.0).ok().and_then(|text| text.trim().parse().ok()) {
                Some(pid) => break Stray::new(pid),
                None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                None => panic!("the stranger never told its pid"),
            }
        };
        let done = finish(&events, Duration::from_secs(4));
        assert_eq!((done.code, done.stopped), (Some(0), false));
        let text = String::from_utf8_lossy(&done.output);
        assert!(text.contains("hi\n"), "the shell's own output is there: {text:?}");
        // Not ours to sweep: it is still writing, and it is the test's to kill.
        stranger.kill();
        assert!(stranger.is_gone(), "the stranger is gone");
    }

    #[test]
    fn leftover_kill_reaches_a_child_in_a_group_of_its_own() {
        let mark = Marker::new("leftover");
        let Some(sleeper) = own_group_sleeper(&mark.0) else {
            eprintln!("SKIP: neither perl nor python3 is here to leave the shell's group");
            return;
        };
        // The shell waits for the child to be in its group, then ends with the child still running.
        let command = format!(
            "{sleeper} & until [ -e '{}' ]; do sleep 0.02; done; echo started",
            mark.0.display()
        );
        let run = ShellRun::start(&command, &cwd()).expect("starts");
        let events = run.events();
        let child = Stray::new(read_pid(&events));
        let done = finish(&events, Duration::from_millis(1500));
        assert_eq!((done.code, done.stopped), (Some(0), false));
        assert!(child.is_gone(), "the child in a group of its own is gone");
    }
}

#[test]
fn missing_cwd_is_an_error() {
    assert!(ShellRun::start("true", Path::new("/nonexistent/dir")).is_err());
}
