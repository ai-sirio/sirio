//! Starting a process that nothing the app belongs to can take down
//! (spec §5.4). Each arm names what it protects against; the probe in
//! `examples/detach_probe.rs` is what proves it.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub struct DetachedSpawn {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    /// Unit / job label, unique per protocol major (`sirio-host-v1`).
    pub label: String,
    /// Where the macOS plist is written. Ignored elsewhere.
    pub launchd_dir: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetachMethod {
    SystemdScope,
    Setsid,
    Launchd,
    WindowsBreakaway,
    WindowsNoBreakaway,
}

/// Starts `spec.program` so that it outlives the caller, its process group,
/// its systemd scope, its macOS coalition and its Windows job.
///
/// The child inherits the caller's full environment on every platform. On
/// macOS that means the environment is written into the plist, because
/// launchd starts a job with its own, not the caller's; the plist is private
/// to the user and is deleted as soon as launchd has loaded it. A variable
/// whose name or value a plist cannot carry (not UTF-8, or a character XML
/// 1.0 forbids) is left out of the host's environment on macOS only.
///
/// `Ok` means the platform's mechanism accepted the process: on Linux
/// `SystemdScope` additionally means `systemd-run` did not fail within
/// 300 ms (it then falls back to `Setsid`), and a program that exits non-zero
/// that fast reads the same as a scope that could not be created. It does
/// **not** mean the program is serving. The caller must still treat "the
/// endpoint never appeared" as the real start failure.
///
/// On macOS a job already running under the label is another client's host
/// that won a start race (two clients may both have seen no host): it is
/// neither booted out nor started again, and the call returns `Ok(Launchd)`
/// as if it had started it. The caller's wait for the endpoint adopts that
/// host, which is why that wait is the only success test.
pub fn spawn_detached(spec: &DetachedSpawn) -> io::Result<DetachMethod> {
    imp::spawn(spec)
}

/// The exit status of a reaped direct child, delivered once.
#[cfg(target_os = "linux")]
type Exits = std::sync::mpsc::Receiver<io::Result<std::process::ExitStatus>>;

/// Reaps the direct child on a thread so it never lingers as a zombie while
/// the caller lives; the child itself is not affected. The returned channel
/// carries its exit status, for a caller that wants to know about an early
/// exit; one that does not simply drops it.
#[cfg(target_os = "linux")]
fn reap_in_background(mut child: std::process::Child) -> Exits {
    let (sender, exits) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(child.wait());
    });
    exits
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::os::unix::process::CommandExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    /// How long `systemd-run` gets to fail before it is taken to have exec'd
    /// into the program. With `--scope` it registers itself in the new scope
    /// and then replaces itself with the program, so a healthy one never
    /// exits on its own account while the program runs.
    const SYSTEMD_RUN_GRACE: Duration = Duration::from_millis(300);

    pub fn spawn(spec: &DetachedSpawn) -> io::Result<DetachMethod> {
        if systemd_user_available() {
            let mut command = Command::new("systemd-run");
            command
                .args(["--user", "--scope", "--quiet", "--collect"])
                .arg(format!("--unit={}", unit_name(&spec.label)))
                .arg("--")
                .arg(&spec.program)
                .args(&spec.args);
            // Spawning `systemd-run` proves only that it was exec'd. A scope
            // that cannot be created (no bus, a clash on the unit name) makes
            // it exit non-zero straight away, and then nothing is running.
            if let Ok(exits) = spawn_setsid(command, spec)
                && !exited_unsuccessfully(&exits)
            {
                return Ok(DetachMethod::SystemdScope);
            }
        }
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        spawn_setsid(command, spec)?;
        Ok(DetachMethod::Setsid)
    }

    /// Unique per call, not just per process: a process that starts a second
    /// host while the first scope lives must not collide with it.
    fn unit_name(label: &str) -> String {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        format!("{label}-{}-{count}-{nanos:x}", std::process::id())
    }

    fn exited_unsuccessfully(exits: &Exits) -> bool {
        matches!(exits.recv_timeout(SYSTEMD_RUN_GRACE), Ok(Ok(status)) if !status.success())
    }

    fn spawn_setsid(mut command: Command, spec: &DetachedSpawn) -> io::Result<Exits> {
        command
            .current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: setsid is async-signal-safe and touches no Rust state.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Ok(reap_in_background(command.spawn()?))
    }

    fn systemd_user_available() -> bool {
        let Ok(output) = Command::new("systemctl")
            .args(["--user", "is-system-running"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        else {
            return false;
        };
        // The manager's state is on stdout. `degraded` exits non-zero but the
        // manager answers; a failed connection ("Failed to connect to bus",
        // "System has not been booted with systemd") exits non-zero too but
        // prints nothing on stdout, and `offline` / `unknown` mean none --
        // judging by the exit code alone would pick `systemd-run` where it
        // cannot work, and a spawn that never started would read as success.
        matches!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "running" | "degraded" | "starting" | "initializing"
        ) && std::env::var_os("XDG_RUNTIME_DIR").is_some()
    }
}

/// What goes into a launchd plist, kept apart from the macOS arm so that its
/// failure modes run on every platform's test suite.
#[cfg(any(test, target_os = "macos"))]
mod plist {
    use super::DetachedSpawn;
    use std::ffi::{OsStr, OsString};
    use std::io;

    /// Whether XML 1.0 allows the character at all. A plist is XML, and a
    /// parser refuses the whole document at the first one it does not.
    fn is_xml_char(c: char) -> bool {
        matches!(c, '\t' | '\n' | '\r') || (c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}')
    }

    /// `text` as the content of a plist `<string>` / `<key>`, or `None` when
    /// it is not valid UTF-8 or holds a character XML cannot carry. Nothing is
    /// replaced or dropped silently: a caller either gets the exact text or
    /// is told it cannot be had.
    fn xml_text(text: &OsStr) -> Option<String> {
        let text = text.to_str()?;
        if !text.chars().all(is_xml_char) {
            return None;
        }
        let mut escaped = String::with_capacity(text.len());
        for c in text.chars() {
            match c {
                '&' => escaped.push_str("&amp;"),
                '<' => escaped.push_str("&lt;"),
                '>' => escaped.push_str("&gt;"),
                '"' => escaped.push_str("&quot;"),
                '\'' => escaped.push_str("&apos;"),
                other => escaped.push(other),
            }
        }
        Some(escaped)
    }

    fn required(text: &OsStr, what: &str) -> io::Result<String> {
        xml_text(text).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("the {what} cannot be written into a launchd plist"),
            )
        })
    }

    /// The `<key>`/`<string>` pairs of an environment. A variable whose name
    /// or value the plist cannot carry is left out; its content is never
    /// reported anywhere.
    pub(super) fn environment_entries(
        vars: impl IntoIterator<Item = (OsString, OsString)>,
    ) -> String {
        vars.into_iter()
            .filter_map(|(name, value)| {
                Some(format!(
                    "<key>{}</key><string>{}</string>",
                    xml_text(&name)?,
                    xml_text(&value)?
                ))
            })
            .collect()
    }

    pub(super) fn plist_xml(
        label: &str,
        spec: &DetachedSpawn,
        environment: impl IntoIterator<Item = (OsString, OsString)>,
    ) -> io::Result<String> {
        let mut arguments = format!(
            "<string>{}</string>",
            required(spec.program.as_os_str(), "program path")?
        );
        for arg in &spec.args {
            arguments.push_str(&format!("<string>{}</string>", required(arg, "argument")?));
        }
        let cwd = required(spec.cwd.as_os_str(), "working directory")?;
        let label = required(OsStr::new(label), "label")?;
        let environment = environment_entries(environment);
        Ok(format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{label}</string>
<key>ProgramArguments</key><array>{arguments}</array>
<key>WorkingDirectory</key><string>{cwd}</string>
<key>EnvironmentVariables</key><dict>{environment}</dict>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><false/>
<key>AbandonProcessGroup</key><true/>
<key>ProcessType</key><string>Interactive</string>
</dict></plist>
"#
        ))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::ffi::OsString;

        fn env(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
            pairs
                .iter()
                .map(|(k, v)| (OsString::from(k), OsString::from(v)))
                .collect()
        }

        fn spec(program: &str) -> crate::detach::DetachedSpawn {
            crate::detach::DetachedSpawn {
                program: program.into(),
                args: vec!["--serve".into()],
                cwd: "/work".into(),
                label: "sirio-host-v1".into(),
                launchd_dir: "/launchd".into(),
            }
        }

        #[test]
        fn an_ordinary_environment_reaches_the_plist_unchanged() {
            let xml = environment_entries(env(&[
                ("PATH", "/usr/bin:/bin"),
                ("GREETING", "caff\u{e8}\tdue\nrighe"),
            ]));
            assert!(xml.contains("<key>PATH</key><string>/usr/bin:/bin</string>"));
            assert!(xml.contains("<key>GREETING</key><string>caff\u{e8}\tdue\nrighe</string>"));
        }

        #[test]
        fn markup_characters_in_a_value_are_escaped() {
            let xml = environment_entries(env(&[("V", "a&b<c>\"d\"'e'")]));
            assert!(xml.contains("<string>a&amp;b&lt;c&gt;&quot;d&quot;&apos;e&apos;</string>"));
            assert!(!xml.contains("<c>"));
        }

        #[test]
        fn a_value_with_an_escape_character_drops_that_variable_only() {
            let xml = environment_entries(env(&[("SECRET", "tok\u{1b}en-9f3a"), ("KEEP", "yes")]));
            assert!(xml.contains("<key>KEEP</key><string>yes</string>"));
            assert!(!xml.contains("SECRET"));
            assert!(
                !xml.contains("9f3a"),
                "a skipped value must not appear anywhere"
            );
            assert!(!xml.contains('\u{1b}'));
        }

        #[test]
        fn a_name_with_a_control_character_drops_that_variable() {
            let xml = environment_entries(env(&[("BAD\u{1}NAME", "x"), ("KEEP", "yes")]));
            assert!(!xml.contains("BAD"));
            assert!(xml.contains("<key>KEEP</key>"));
        }

        #[test]
        fn xml_noncharacters_drop_the_variable() {
            let xml = environment_entries(env(&[
                ("A", "x\u{fffe}y"),
                ("B", "x\u{ffff}y"),
                ("KEEP", "yes"),
            ]));
            assert!(!xml.contains("<key>A</key>") && !xml.contains("<key>B</key>"));
            assert!(xml.contains("<key>KEEP</key>"));
        }

        #[cfg(unix)]
        #[test]
        fn names_and_values_that_are_not_utf8_drop_the_variable() {
            use std::os::unix::ffi::OsStringExt;
            let xml = environment_entries(vec![
                (OsString::from_vec(b"BAD\xff".to_vec()), OsString::from("x")),
                (OsString::from("VAL"), OsString::from_vec(b"v\xfe".to_vec())),
                (OsString::from("KEEP"), OsString::from("yes")),
            ]);
            assert!(!xml.contains("BAD") && !xml.contains("VAL"));
            assert!(xml.contains("<key>KEEP</key>"));
        }

        #[test]
        fn a_program_that_cannot_be_written_is_refused_rather_than_mangled() {
            let error = plist_xml("app.example.host.v1", &spec("/bin/ho\u{1b}st"), env(&[]))
                .expect_err("a broken plist must not be produced");
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        }

        #[test]
        fn the_plist_names_the_job_the_program_and_the_environment() {
            let xml = plist_xml(
                "app.example.host.v1",
                &spec("/bin/host"),
                env(&[("K", "v")]),
            )
            .expect("plist");
            assert!(xml.contains("<key>Label</key><string>app.example.host.v1</string>"));
            assert!(xml.contains("<string>/bin/host</string><string>--serve</string>"));
            assert!(xml.contains("<key>WorkingDirectory</key><string>/work</string>"));
            assert!(xml.contains(
                "<key>EnvironmentVariables</key><dict><key>K</key><string>v</string></dict>"
            ));
        }
    }
}

/// What to do about a launchd job already under this label, decided from
/// `launchctl print`, kept apart from the macOS arm so that it runs on every
/// platform's test suite.
#[cfg(any(test, target_os = "macos"))]
mod launchd_plan {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Plan {
        /// Nothing is loaded under the label.
        Bootstrap,
        /// A job is loaded but is not running: a leftover to remove first.
        BootoutThenBootstrap,
        /// The job is running: another client's host. Neither remove nor
        /// start anything; the caller waits for its endpoint.
        AlreadyRunning,
    }

    /// `print_succeeded` is whether `launchctl print` exited 0 (the job is
    /// loaded), `stdout` what it printed. launchd prints a `pid = <n>` line
    /// only while the job runs, so only a positive pid counts as running; a
    /// misread toward "running" costs a start timeout, a misread toward "not
    /// running" would boot out a live host.
    pub fn decide(print_succeeded: bool, stdout: &str) -> Plan {
        if !print_succeeded {
            return Plan::Bootstrap;
        }
        if stdout.lines().any(prints_a_running_pid) {
            Plan::AlreadyRunning
        } else {
            Plan::BootoutThenBootstrap
        }
    }

    /// A line that is exactly the `pid` key (not `ppid`, not `parent pid`)
    /// followed by a positive number, with whatever text may trail it ignored.
    fn prints_a_running_pid(line: &str) -> bool {
        let Some(value) = line.trim_start().strip_prefix("pid = ") else {
            return false;
        };
        let digits: String = value.chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u64>().is_ok_and(|pid| pid > 0)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const RUNNING: &str = "gui/501/app.sirioai.sirio.host.v1 = {\n\tactive count = 1\n\tpath = /Users/u/launchd/app.sirioai.sirio.host.v1.plist\n\ttype = LaunchAgent\n\tstate = running\n\n\tprogram = /Users/u/bin/sirio-host\n\tpid = 4242\n\timmediate reason = inefficient\n}\n";
        const LOADED_IDLE: &str = "gui/501/app.sirioai.sirio.host.v1 = {\n\tactive count = 0\n\ttype = LaunchAgent\n\tstate = not running\n\n\tprogram = /Users/u/bin/sirio-host\n\tlast exit code = 0\n}\n";

        #[test]
        fn a_job_that_is_not_loaded_is_bootstrapped_without_a_bootout() {
            assert_eq!(decide(false, ""), Plan::Bootstrap);
            // Whatever a failed print wrote is not a description of a job.
            assert_eq!(decide(false, RUNNING), Plan::Bootstrap);
        }

        #[test]
        fn a_running_job_is_another_clients_host_and_is_left_alone() {
            assert_eq!(decide(true, RUNNING), Plan::AlreadyRunning);
            assert_eq!(decide(true, "\tpid = 7 (spawned)\n"), Plan::AlreadyRunning);
        }

        #[test]
        fn a_loaded_job_with_no_pid_is_a_leftover_to_remove() {
            assert_eq!(decide(true, LOADED_IDLE), Plan::BootoutThenBootstrap);
            assert_eq!(decide(true, ""), Plan::BootoutThenBootstrap);
        }

        #[test]
        fn only_a_positive_pid_counts_as_running() {
            assert_eq!(decide(true, "\tpid = 0\n"), Plan::BootoutThenBootstrap);
            assert_eq!(decide(true, "\tpid = \n"), Plan::BootoutThenBootstrap);
            assert_eq!(decide(true, "\tpid = abc\n"), Plan::BootoutThenBootstrap);
        }

        #[test]
        fn another_key_that_ends_in_pid_is_not_the_pid_line() {
            assert_eq!(
                decide(true, "\tparent pid = 99\n\tppid = 98\n"),
                Plan::BootoutThenBootstrap
            );
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::launchd_plan::{self, Plan};
    use super::plist::plist_xml;
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    use std::path::Path;

    pub fn spawn(spec: &DetachedSpawn) -> io::Result<DetachMethod> {
        // SAFETY: getuid has no preconditions and cannot fail.
        let uid = unsafe { libc::getuid() };
        let label = format!(
            "app.sirioai.sirio.host.{}",
            spec.label.trim_start_matches("sirio-host-")
        );
        let xml = plist_xml(&label, spec, std::env::vars_os())?;
        let domain = format!("gui/{uid}");
        let target = format!("{domain}/{label}");
        // Two clients that both saw `Absent` both arrive here, so a job under
        // this label may be the host the other one has just started: it is
        // never booted out while it runs. (`print` to `bootout` is not
        // atomic: a client that bootstraps in that gap still has its host
        // booted out by this one, which then starts its own; the endpoint
        // wait of whichever client survives adopts the host that is left.)
        let plan = current_plan(&target);
        if plan == Plan::AlreadyRunning {
            return Ok(DetachMethod::Launchd);
        }
        private_dir(&spec.launchd_dir)?;
        let plist = spec.launchd_dir.join(format!("{label}.plist"));
        write_private(&plist, &xml)?;
        if plan == Plan::BootoutThenBootstrap {
            // Loaded but not running: a leftover of a host that is gone.
            let _ = Command::new("launchctl")
                .args(["bootout", &target])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let bootstrap = Command::new("launchctl")
            .args(["bootstrap", &domain])
            .arg(&plist)
            .status();
        // launchd has read the plist by the time `bootstrap` returns, and it
        // carries the caller's whole environment (tokens included): it must
        // not outlive the call, success or not.
        let _ = std::fs::remove_file(&plist);
        let status = bootstrap?;
        if !status.success() {
            // A bootstrap that fails because another client's just won is a
            // lost race, not an error: the caller waits for that host.
            if current_plan(&target) == Plan::AlreadyRunning {
                return Ok(DetachMethod::Launchd);
            }
            return Err(io::Error::other(format!(
                "launchctl bootstrap exited {status}"
            )));
        }
        Ok(DetachMethod::Launchd)
    }

    /// What `launchctl print <target>` says about the job under the label. A
    /// `launchctl` that cannot be run reads as not loaded; the bootstrap that
    /// follows then fails with its own error.
    fn current_plan(target: &str) -> Plan {
        let (loaded, report) = Command::new("launchctl")
            .args(["print", target])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map(|out| {
                (
                    out.status.success(),
                    String::from_utf8_lossy(&out.stdout).into_owned(),
                )
            })
            .unwrap_or((false, String::new()));
        launchd_plan::decide(loaded, &report)
    }

    fn private_dir(dir: &Path) -> io::Result<()> {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        // A directory that already existed keeps the mode it had.
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
    }

    fn write_private(path: &Path, text: &str) -> io::Result<()> {
        let written = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(path)?;
            // `mode` applies only to a file this call creates.
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            file.write_all(text.as_bytes())
        })();
        if written.is_err() {
            let _ = std::fs::remove_file(path);
        }
        written
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
    use windows_sys::Win32::System::Threading::{
        CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS,
    };

    pub fn spawn(spec: &DetachedSpawn) -> io::Result<DetachMethod> {
        let base = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
        match start(spec, base | CREATE_BREAKAWAY_FROM_JOB) {
            Ok(()) => Ok(DetachMethod::WindowsBreakaway),
            // A job without JOB_OBJECT_LIMIT_BREAKAWAY_OK refuses breakaway
            // with ERROR_ACCESS_DENIED; the host is then inside the job and
            // the probe records that this row does not hold there.
            Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {
                start(spec, base)?;
                Ok(DetachMethod::WindowsNoBreakaway)
            }
            Err(error) => Err(error),
        }
    }

    fn start(spec: &DetachedSpawn, flags: u32) -> io::Result<()> {
        Command::new(&spec.program)
            .args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(flags)
            .spawn()
            .map(drop)
    }
}
