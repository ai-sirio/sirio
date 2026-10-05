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
/// launchd starts a job with its own, not the caller's.
pub fn spawn_detached(spec: &DetachedSpawn) -> io::Result<DetachMethod> {
    imp::spawn(spec)
}

/// Reaps the direct child on a thread so it never lingers as a zombie while
/// the caller lives; the child itself is not affected.
#[cfg(target_os = "linux")]
fn reap_in_background(mut child: std::process::Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::os::unix::process::CommandExt;

    pub fn spawn(spec: &DetachedSpawn) -> io::Result<DetachMethod> {
        if systemd_user_available() {
            let unit = format!("{}-{}", spec.label, std::process::id());
            let mut command = Command::new("systemd-run");
            command
                .args(["--user", "--scope", "--quiet", "--collect"])
                .arg(format!("--unit={unit}"))
                .arg("--")
                .arg(&spec.program)
                .args(&spec.args);
            if spawn_setsid(command, spec).is_ok() {
                return Ok(DetachMethod::SystemdScope);
            }
        }
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        spawn_setsid(command, spec)?;
        Ok(DetachMethod::Setsid)
    }

    fn spawn_setsid(mut command: Command, spec: &DetachedSpawn) -> io::Result<()> {
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
        let child = command.spawn()?;
        reap_in_background(child);
        Ok(())
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

#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    pub fn spawn(spec: &DetachedSpawn) -> io::Result<DetachMethod> {
        // SAFETY: getuid has no preconditions and cannot fail.
        let uid = unsafe { libc::getuid() };
        let label = format!(
            "app.sirioai.sirio.host.{}",
            spec.label.trim_start_matches("sirio-host-")
        );
        std::fs::create_dir_all(&spec.launchd_dir)?;
        let plist = spec.launchd_dir.join(format!("{label}.plist"));
        std::fs::write(&plist, plist_xml(&label, spec))?;
        let domain = format!("gui/{uid}");
        // Only reached on an `Absent` verdict, so nothing is running under
        // this label: a leftover loaded job from a previous host is removed.
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("{domain}/{label}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let status = Command::new("launchctl")
            .args(["bootstrap", &domain])
            .arg(&plist)
            .status()?;
        if !status.success() {
            return Err(io::Error::other(format!(
                "launchctl bootstrap exited {status}"
            )));
        }
        Ok(DetachMethod::Launchd)
    }

    fn escape(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    fn plist_xml(label: &str, spec: &DetachedSpawn) -> String {
        let mut arguments = format!(
            "<string>{}</string>",
            escape(&spec.program.to_string_lossy())
        );
        for arg in &spec.args {
            arguments.push_str(&format!(
                "<string>{}</string>",
                escape(&arg.to_string_lossy())
            ));
        }
        let mut environment = String::new();
        for (key, value) in std::env::vars_os() {
            environment.push_str(&format!(
                "<key>{}</key><string>{}</string>",
                escape(&key.to_string_lossy()),
                escape(&value.to_string_lossy())
            ));
        }
        format!(
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
"#,
            cwd = escape(&spec.cwd.to_string_lossy()),
        )
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
