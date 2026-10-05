//! Process identity that survives pid reuse: a pid plus the time the kernel
//! says it started. Only equality of two readings is meaningful — the unit
//! differs per platform.

#[cfg(target_os = "linux")]
pub fn start_time(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Field 22 (starttime). `comm` (field 2) may contain spaces and ')', so
    // count from the last ')'.
    let after_comm = &stat[stat.rfind(')')? + 1..];
    after_comm.split_whitespace().nth(19)?.parse().ok()
}

#[cfg(target_os = "macos")]
pub fn start_time(pid: u32) -> Option<u64> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    let read = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTBSDINFO,
            0,
            &mut info as *mut _ as *mut _,
            size,
        )
    };
    (read == size).then(|| info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec)
}

/// Windows: the creation time as a `FILETIME` (100 ns ticks since 1601).
///
/// Liveness comes from `GetExitCodeProcess`, not from `GetProcessTimes`'s exit
/// time, which is undefined while the process runs. A process that has exited
/// but whose handle is still open elsewhere reads as dead here, since its exit
/// code is not `STILL_ACTIVE`. The one blind spot: a process that exited with
/// code 259 (`STILL_ACTIVE` itself) reads as alive. That only ever yields an
/// unverifiable verdict for a pid whose start time still matches, never a
/// signal sent to the wrong process.
#[cfg(windows)]
pub fn start_time(pid: u32) -> Option<u64> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut exit_code: u32 = 0;
        if GetExitCodeProcess(handle, &mut exit_code) == 0 || exit_code != STILL_ACTIVE as u32 {
            CloseHandle(handle);
            return None;
        }
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        let ok = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user);
        CloseHandle(handle);
        (ok != 0).then(|| ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
compile_error!("sirio_ipc::process::start_time has no implementation for this platform");

/// Whether a process with this pid is running right now.
pub fn exists(pid: u32) -> bool {
    start_time(pid).is_some()
}
