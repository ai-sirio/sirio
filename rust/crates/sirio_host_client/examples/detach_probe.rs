//! Proves spec §5.4 row by row: `parent` detaches a `child` that writes a
//! heartbeat file every 200 ms; the script kills the parent's world and
//! checks the heartbeat keeps moving.
//!
//!   detach_probe parent <dir>   detach a child, print `child-method=<m>`, then sleep forever
//!   detach_probe child <dir>    write <dir>/heartbeat (a counter) every 200 ms for 120 s
//!   detach_probe job <dir>      Windows only: run `parent` inside a kill-on-close job, then close the job

use std::path::PathBuf;
use std::time::Duration;

use sirio_host_client::detach::{DetachedSpawn, spawn_detached};

fn main() {
    let mut args = std::env::args_os().skip(1);
    let role = args
        .next()
        .and_then(|r| r.into_string().ok())
        .unwrap_or_default();
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

#[cfg(windows)]
fn job(dir: PathBuf) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::*;
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of_val(&info) as u32,
        );
        let parent = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("parent")
            .arg(&dir)
            .env("SIRIO_PROBE_DELAY", "1")
            .spawn()
            .expect("parent");
        AssignProcessToJobObject(job, parent.as_raw_handle() as _);
        std::thread::sleep(std::time::Duration::from_secs(3));
        CloseHandle(job); // kill-on-close: the parent dies here
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
