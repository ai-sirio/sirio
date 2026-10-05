//! Proves spec §5.4 row by row: `parent` detaches a `child` that writes a
//! heartbeat file every 200 ms; the script kills the parent's world and
//! checks the heartbeat keeps moving.
//!
//!   detach_probe parent <dir>   detach a child, print `child-method=<m>`, then sleep forever
//!   detach_probe child <dir>    write <dir>/heartbeat (a counter) every 200 ms for 120 s
//!   detach_probe job <dir>      Windows only: run `parent` inside a kill-on-close job, then close the job
//!   detach_probe alive <pid>    exit 0 while the process runs, 1 otherwise

use std::path::PathBuf;
use std::time::Duration;

use sirio_host_client::detach::{DetachedSpawn, spawn_detached};

fn main() {
    let mut args = std::env::args_os().skip(1);
    let role = args
        .next()
        .and_then(|r| r.into_string().ok())
        .unwrap_or_default();
    if role == "alive" {
        let pid: u32 = args
            .next()
            .and_then(|p| p.into_string().ok()?.parse().ok())
            .expect("alive <pid>");
        std::process::exit(if sirio_ipc::process::exists(pid) { 0 } else { 1 });
    }
    let dir = PathBuf::from(args.next().expect("dir"));
    std::fs::create_dir_all(&dir).expect("dir");
    match role.as_str() {
        "parent" => parent(dir),
        "child" => child(dir),
        #[cfg(windows)]
        "job" => job(dir),
        other => panic!("unknown role {other}"),
    }
}

fn parent(dir: PathBuf) {
    if std::env::var_os("SIRIO_PROBE_DELAY").is_some() {
        std::thread::sleep(Duration::from_secs(1));
    }
    let exe = std::env::current_exe().expect("exe");
    let method = spawn_detached(&DetachedSpawn {
        program: exe,
        args: vec!["child".into(), dir.clone().into_os_string()],
        cwd: dir.clone(),
        label: "sirio-host-probe".into(),
        launchd_dir: dir.join("launchd"),
    })
    .expect("spawn_detached");
    println!("child-method={method:?}");
    std::fs::write(dir.join("parent.pid"), std::process::id().to_string()).expect("pid");
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

fn child(dir: PathBuf) {
    std::fs::write(dir.join("child.pid"), std::process::id().to_string()).expect("pid");
    for beat in 0..600u32 {
        std::fs::write(dir.join("heartbeat"), beat.to_string()).expect("heartbeat");
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// A Win32 call that returned failure: the probe has no way to carry on, and
/// a row built on a job that was never set up would prove nothing.
#[cfg(windows)]
fn win32_failed(call: &str) -> ! {
    // `last_os_error` is `GetLastError()` on Windows.
    panic!("{call} failed: {}", std::io::Error::last_os_error());
}

#[cfg(windows)]
fn job(dir: PathBuf) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::*;
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            win32_failed("CreateJobObjectW");
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of_val(&info) as u32,
        ) == 0
        {
            win32_failed("SetInformationJobObject");
        }
        let parent = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("parent")
            .arg(&dir)
            .env("SIRIO_PROBE_DELAY", "1")
            .spawn()
            .expect("parent");
        if AssignProcessToJobObject(job, parent.as_raw_handle() as _) == 0 {
            win32_failed("AssignProcessToJobObject");
        }
        std::thread::sleep(std::time::Duration::from_secs(3));
        // kill-on-close: the parent dies here
        if CloseHandle(job) == 0 {
            win32_failed("CloseHandle");
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
