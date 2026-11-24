//! `ShellRun` against real processes: output, exit code, the session, and what is left behind.
//!
//! The runner picks the user's `$SHELL`, so these need a POSIX one (bash, zsh, dash). Under fish,
//! run them as `SHELL=/bin/sh cargo test -p sirio_chat --test shell_run`.

use std::path::Path;

use sirio_chat::shell::ShellRun;

#[cfg(unix)]
mod unix {
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

    /// Kills the pid when a test ends, so an assertion that fails does not leave it running.
    struct KillOnDrop(i32);

    impl Drop for KillOnDrop {
        fn drop(&mut self) {
            // SAFETY: a plain syscall; a pid that is gone just answers ESRCH.
            unsafe { libc::kill(self.0, libc::SIGKILL) };
        }
    }

    /// Whether `pid` is gone, giving init up to 2 s to reap a killed orphan: a zombie still
    /// answers `kill(pid, 0)`.
    fn gone(pid: i32) -> bool {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            // SAFETY: signal 0 only checks that the pid exists.
            let alive = unsafe { libc::kill(pid, 0) } == 0
                || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH);
            if !alive {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
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
        let _cleanup = KillOnDrop(run.pid() as i32);
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
        let child = read_pid(&events);
        let _cleanup = (KillOnDrop(child), KillOnDrop(run.pid() as i32));
        run.stop();
        let done = finish(&events, Duration::from_secs(3));
        // A signal's death is the shell's own `$?`: 128 plus the signal.
        assert_eq!((done.code, done.stopped), (Some(128 + libc::SIGTERM), true));
        assert!(gone(run.pid() as i32), "the shell is gone");
        assert!(gone(child), "the background sleep is gone");
    }

    #[test]
    fn drop_kills_the_whole_group() {
        let run = ShellRun::start("sleep 300 & echo $!; sleep 300", &cwd()).expect("starts");
        let events = run.events();
        let child = read_pid(&events);
        let shell = run.pid() as i32;
        let _cleanup = (KillOnDrop(child), KillOnDrop(shell));
        drop(run);
        let done = finish(&events, Duration::from_secs(3));
        assert_eq!((done.code, done.stopped), (Some(128 + libc::SIGKILL), true));
        assert!(gone(shell), "the shell is gone");
        assert!(gone(child), "the background sleep is gone");
    }

    #[test]
    fn background_job_does_not_hold_the_block() {
        let run = ShellRun::start("sleep 30 & echo $!", &cwd()).expect("starts");
        let events = run.events();
        let _cleanup = KillOnDrop(run.pid() as i32);
        let done = finish(&events, Duration::from_secs(1));
        assert_eq!((done.code, done.stopped), (Some(0), false));
        let child: i32 = String::from_utf8_lossy(&done.output).trim().parse().expect("a pid");
        let _cleanup = KillOnDrop(child);
        assert!(gone(child), "the background sleep is gone");
    }
}

#[test]
fn missing_cwd_is_an_error() {
    assert!(ShellRun::start("true", Path::new("/nonexistent/dir")).is_err());
}
