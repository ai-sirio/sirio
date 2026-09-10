use std::path::{Path, PathBuf};

/// A filesystem change observed by the Linux file monitor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileEventKind {
    Created,
    Modified,
    Removed,
    Renamed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSystemEvent {
    pub path: PathBuf,
    pub kind: FileEventKind,
}

/// A nonblocking inotify-backed monitor for a directory or a single file.
///
/// inotify is Linux-only (not even generically Unix — macOS has no inotify and uses
/// FSEvents/kqueue instead), so the real implementation is `cfg(target_os = "linux")` below.
#[cfg(target_os = "linux")]
pub struct FileSystemEventMonitor {
    fd: std::os::fd::OwnedFd,
    directory: PathBuf,
    file_filter: Option<PathBuf>,
}

#[cfg(target_os = "linux")]
impl FileSystemEventMonitor {
    pub fn new(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let (directory, file_filter) = match metadata {
            Some(metadata) if metadata.is_dir() => (path.to_path_buf(), None),
            Some(_) | None => (
                path.parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf(),
                Some(path.to_path_buf()),
            ),
        };
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let watch = std::ffi::CString::new(directory.to_string_lossy().as_bytes())
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in path"))?;
        let mask = libc::IN_CREATE
            | libc::IN_MODIFY
            | libc::IN_CLOSE_WRITE
            | libc::IN_DELETE
            | libc::IN_MOVED_FROM
            | libc::IN_MOVED_TO;
        let watch_descriptor = unsafe { libc::inotify_add_watch(fd, watch.as_ptr(), mask) };
        if watch_descriptor < 0 {
            let error = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(error);
        }
        let _ = watch_descriptor;
        Ok(Self {
            // SAFETY: fd is freshly returned by inotify_init1 and ownership
            // transfers to this value exactly once.
            fd: unsafe { std::os::fd::FromRawFd::from_raw_fd(fd) },
            directory,
            file_filter,
        })
    }

    /// Reads all currently queued events without blocking.
    pub fn poll(&self) -> std::io::Result<Vec<FileSystemEvent>> {
        use std::os::fd::AsRawFd;
        let mut buffer = [0u8; 16 * 1024];
        let count = unsafe {
            libc::read(
                self.fd.as_raw_fd(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            )
        };
        if count < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::WouldBlock {
                return Ok(Vec::new());
            }
            return Err(error);
        }
        let mut offset = 0usize;
        let mut events = Vec::new();
        while offset + std::mem::size_of::<libc::inotify_event>() <= count as usize {
            // SAFETY: the kernel writes an inotify_event header followed by
            // `len` name bytes; bounds are checked before each read.
            let event = unsafe { &*buffer.as_ptr().add(offset).cast::<libc::inotify_event>() };
            let header = std::mem::size_of::<libc::inotify_event>();
            let end = offset
                .saturating_add(header)
                .saturating_add(event.len as usize);
            if end > count as usize {
                break;
            }
            let name_bytes = &buffer[offset + header..end];
            let name_end = name_bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(name_bytes.len());
            let name = String::from_utf8_lossy(&name_bytes[..name_end]);
            let path = self.directory.join(name.as_ref());
            if self
                .file_filter
                .as_ref()
                .is_none_or(|filter| filter == &path)
            {
                let mask = event.mask;
                let kind = if mask & (libc::IN_MOVED_FROM | libc::IN_MOVED_TO) != 0 {
                    FileEventKind::Renamed
                } else if mask & (libc::IN_CREATE) != 0 {
                    FileEventKind::Created
                } else if mask & (libc::IN_DELETE) != 0 {
                    FileEventKind::Removed
                } else {
                    FileEventKind::Modified
                };
                events.push(FileSystemEvent { path, kind });
            }
            offset = end;
        }
        Ok(events)
    }
}

/// Windows monitor: `ReadDirectoryChangesW` behind the same seam.
///
/// Shape parity with the Linux arm, deliberately:
/// - **One non-recursive directory watch** (`bWatchSubtree = FALSE`), i.e.
///   inotify's single-directory watch. The watched entry is resolved
///   identically — a directory is watched as itself with no name filter; a
///   file (or a missing path) watches its parent and filters on the full
///   path.
/// - **No coalescing.** inotify delivers `IN_MOVED_FROM` and `IN_MOVED_TO`
///   as two independent records, and this code maps each kernel record to
///   exactly one `FileSystemEvent` — a rename surfaces as two `Renamed`
///   events (old name, new name), the filter usually admitting only the
///   new-name one when the watched file is the rename target. Windows
///   delivers the same shape (`FILE_ACTION_RENAMED_OLD_NAME` +
///   `FILE_ACTION_RENAMED_NEW_NAME`), and editors produce several records
///   per logical save (write temp / rename over / truncate), so the same
///   edit arrives more than once. That duplication is deliberate and
///   matches Linux: the UI consumer (`sirio_ui::file_view`) re-reads the
///   file idempotently, and a missed edit is the failure this feature
///   exists to prevent, so an extra re-read beats a dedupe that could drop
///   a real change.
/// - **Filtering** compares the joined full path against the filter with
///   `PathBuf` equality — exactly what the Linux arm does. On Windows that
///   equality is case-insensitive by platform convention, which is the
///   same comparison the UI makes, so filter and consumer agree. A name
///   delivered in 8.3 short form (only when the directory has short-name
///   generation and the mutation actually used a short name) fails the
///   comparison and is dropped, the same way an inotify name that does not
///   byte-match the filter is dropped.
///
/// Overlapped-IO mechanics:
/// - The read runs against a **manual-reset event** in the `OVERLAPPED`
///   and is re-armed after every drain, so `poll()` never blocks: a
///   zero-timeout wait decides "nothing new" and the parse/re-arm happens
///   only on a completed read.
/// - The buffer and `OVERLAPPED` are boxed so their **addresses are
///   stable** while a read is armed — the kernel writes through those
///   addresses even if the monitor struct itself moves; the boxes move the
///   pointers, not the buffers.
/// - The `FILE_NOTIFY_INFORMATION` chain links records by
///   `NextEntryOffset`; each `FileName` is UTF-16, **not** null-terminated,
///   with `FileNameLength` in **bytes**. Reading the length wrong makes
///   adjacent records parse as filename, so the length is validated
///   (even, in-bounds) before conversion.
/// - `ERROR_NOTIFY_ENUM_DIR` means the buffer overflowed and records were
///   **lost**. It is not "nothing changed": one synthetic `Modified`
///   event for the watched file is reported (a file-filtered watch names
///   its target; a directory watch has no nameable target — the only
///   in-app consumer watches files, so for directories the honest
///   response is a stderr note), so the UI re-reads. That is the degraded
///   mode of choice over silence.
/// - The directory handle is opened with `FILE_FLAG_BACKUP_SEMANTICS`
///   (required to open a directory), `FILE_FLAG_OVERLAPPED`, and
///   `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE` — without
///   write/delete sharing, editors could not save or replace the watched
///   file while it is open. `Drop` cancels the outstanding read
///   (`CancelIoEx`) **before** closing the directory handle, then closes
///   the event handle, so teardown cannot leave the kernel writing into a
///   freed buffer; the monitor outlives its polls by construction (the
///   UI's 100 ms timer task dies with the owning view), so `poll` and
///   `Drop` never race.
#[cfg(target_os = "windows")]
pub struct FileSystemEventMonitor {
    directory: PathBuf,
    file_filter: Option<PathBuf>,
    /// Raw handles, closed by an explicit `Drop` sequence (cancel → close
    /// directory → close event); `OwnedHandle` would close in field order
    /// with no cancellation step in between.
    directory_handle: windows_sys::Win32::Foundation::HANDLE,
    event_handle: windows_sys::Win32::Foundation::HANDLE,
    state: std::cell::RefCell<WindowsWatchState>,
}

#[cfg(target_os = "windows")]
struct WindowsWatchState {
    overlapped: Box<windows_sys::Win32::System::IO::OVERLAPPED>,
    buffer: Box<[u8]>,
    /// Whether a read is currently armed (the kernel may write into
    /// `buffer`/`overlapped` only while this is true).
    armed: bool,
    /// One synthetic "possibly changed" event is owed (an overflow
    /// happened while draining or arming); served by the next `poll`.
    overflow_pending: bool,
}

/// Directory-change filter mask: file names (create/delete/rename) and
/// last-write times (content edits) — the same coverage as the Linux
/// arm's `IN_CREATE | IN_MODIFY | IN_CLOSE_WRITE | IN_DELETE |
/// IN_MOVED_FROM | IN_MOVED_TO`.
#[cfg(target_os = "windows")]
const WATCH_MASK: u32 = windows_sys::Win32::Storage::FileSystem::FILE_NOTIFY_CHANGE_FILE_NAME
    | windows_sys::Win32::Storage::FileSystem::FILE_NOTIFY_CHANGE_LAST_WRITE;

/// `ERROR_NOTIFY_ENUM_DIR`: the read failed because the buffer could not
/// hold the change burst, so records were lost.
#[cfg(target_os = "windows")]
const ERROR_NOTIFY_ENUM_DIR: i32 = windows_sys::Win32::Foundation::ERROR_NOTIFY_ENUM_DIR as i32;

#[cfg(target_os = "windows")]
impl FileSystemEventMonitor {
    pub fn new(path: impl AsRef<Path>) -> std::io::Result<Self> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        };
        use windows_sys::Win32::System::IO::OVERLAPPED;
        use windows_sys::Win32::System::Threading::CreateEventW;

        let path = path.as_ref();
        // Identical resolution to the Linux arm: a directory is watched
        // as itself with no name filter; a file or missing path watches
        // its parent and filters on the full path.
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let (directory, file_filter) = match metadata {
            Some(metadata) if metadata.is_dir() => (path.to_path_buf(), None),
            Some(_) | None => (
                path.parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf(),
                Some(path.to_path_buf()),
            ),
        };
        let directory_wide: Vec<u16> = directory
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: `directory_wide` is a valid null-terminated UTF-16 path.
        // `FILE_FLAG_BACKUP_SEMANTICS` is required to open a directory;
        // sharing read+write+delete keeps editors able to save the watched
        // file and rename over it while the watch is open.
        let directory_handle = unsafe {
            CreateFileW(
                directory_wide.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
                std::ptr::null_mut(),
            )
        };
        if directory_handle == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: a manual-reset event (bManualReset = TRUE) with no name;
        // it stays signaled after a read completes until ResetEvent, which
        // is exactly the level-triggered arbitration poll() wants.
        let event_handle = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if event_handle.is_null() {
            let error = std::io::Error::last_os_error();
            // SAFETY: the directory handle is still open and owned here.
            unsafe { CloseHandle(directory_handle) };
            return Err(error);
        }
        let mut overlapped = Box::new(unsafe { std::mem::zeroed::<OVERLAPPED>() });
        overlapped.hEvent = event_handle;
        let monitor = Self {
            directory,
            file_filter,
            directory_handle,
            event_handle,
            state: std::cell::RefCell::new(WindowsWatchState {
                overlapped,
                // 64 KiB: several times the largest plausible burst of
                // FILE_NOTIFY_INFORMATION records (a name is capped at
                // 255 UTF-16 units); overflow stays a rare pathology.
                buffer: vec![0u8; 64 * 1024].into_boxed_slice(),
                armed: false,
                overflow_pending: false,
            }),
        };
        monitor.arm(&mut monitor.state.borrow_mut())?;
        Ok(monitor)
    }

    /// Arms the first read. `ReadDirectoryChangesW` failures are
    /// `io::Error`s; `ERROR_NOTIFY_ENUM_DIR` is not one of them — an
    /// overflowing initial burst reports "possibly changed" via
    /// [`Self::poll`] rather than making construction fail (a failed
    /// `new` would make the caller give up watching entirely, which is
    /// worse than a degraded watch).
    fn arm(&self, state: &mut WindowsWatchState) -> std::io::Result<()> {
        use windows_sys::Win32::Storage::FileSystem::ReadDirectoryChangesW;

        if state.armed {
            return Ok(());
        }
        let mut transferred: u32 = 0;
        // SAFETY: `state.buffer` and `state.overlapped` are boxed (stable
        // addresses) and no read is armed, so nothing else touches them;
        // the kernel owns both until the event signals, and `poll` is the
        // only reader, after completion. `directory_handle` is a valid
        // open handle for the lifetime of the monitor.
        let ok = unsafe {
            ReadDirectoryChangesW(
                self.directory_handle,
                state.buffer.as_mut_ptr().cast(),
                state.buffer.len() as u32,
                0, // bWatchSubtree = FALSE, matching inotify's single-directory watch
                WATCH_MASK,
                &mut transferred,
                &mut *state.overlapped,
                None, // completion routine, not used (event-based)
            )
        };
        if ok == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_NOTIFY_ENUM_DIR) {
                state.overflow_pending = true;
                return Ok(());
            }
            return Err(error);
        }
        state.armed = true;
        Ok(())
    }

    /// Reads all currently queued events without blocking — the same
    /// contract as the Linux arm's `read`-with-`EAGAIN` drain. A
    /// zero-timeout wait on the completion event decides "nothing
    /// happened" without parking the caller's thread.
    pub fn poll(&self) -> std::io::Result<Vec<FileSystemEvent>> {
        use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
        use windows_sys::Win32::System::IO::GetOverlappedResult;
        use windows_sys::Win32::System::Threading::{ResetEvent, WaitForSingleObject};

        let mut state = self.state.borrow_mut();
        if state.overflow_pending {
            // Serve the owed "possibly changed" signal first; the re-arm
            // below happens on the next poll (or right after drain). An
            // overflow means records were lost, and the honest response
            // is a re-read, not silence.
            state.overflow_pending = false;
            if let Some(filter) = &self.file_filter {
                return Ok(vec![FileSystemEvent {
                    path: filter.clone(),
                    kind: FileEventKind::Modified,
                }]);
            }
            eprintln!(
                "[files] directory-watch overflow on {}: changes may have been lost",
                self.directory.display()
            );
            return Ok(Vec::new());
        }
        if !state.armed {
            self.arm(&mut state)?;
        }
        // SAFETY: `event_handle` is the manual-reset event paired with the
        // armed read. Timeout zero keeps this strictly non-blocking.
        let wait = unsafe { WaitForSingleObject(self.event_handle, 0) };
        if wait == WAIT_TIMEOUT {
            return Ok(Vec::new());
        }
        if wait != WAIT_OBJECT_0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut transferred: u32 = 0;
        // SAFETY: the event signal means the overlapped read completed;
        // GetOverlappedResult with bWait = FALSE returns immediately.
        let ok = unsafe {
            GetOverlappedResult(
                self.directory_handle,
                &*state.overlapped,
                &mut transferred,
                0, // bWait = FALSE: the event already signalled completion
            )
        };
        if ok == 0 {
            let error = std::io::Error::last_os_error();
            // SAFETY: the read completed (with an error), so the event is
            // the one to reset before the next arm.
            unsafe { ResetEvent(self.event_handle) };
            state.armed = false;
            if error.raw_os_error() == Some(ERROR_NOTIFY_ENUM_DIR) {
                // The buffer overflowed mid-flight: records were lost, so
                // report the watched file as possibly changed and re-arm.
                state.overflow_pending = true;
                self.arm(&mut state)?;
                if let Some(filter) = &self.file_filter {
                    return Ok(vec![FileSystemEvent {
                        path: filter.clone(),
                        kind: FileEventKind::Modified,
                    }]);
                }
                eprintln!(
                    "[files] directory-watch overflow on {}: changes may have been lost",
                    self.directory.display()
                );
                return Ok(Vec::new());
            }
            return Err(error);
        }
        // SAFETY: the read completed, so nothing is writing into the
        // buffer anymore; reset the level-triggered event before re-arming.
        unsafe { ResetEvent(self.event_handle) };
        let events = parse_notifications(
            &state.buffer[..transferred as usize],
            &self.directory,
            &self.file_filter,
        );
        state.armed = false;
        self.arm(&mut state)?;
        Ok(events)
    }
}

/// Parses one completed `ReadDirectoryChangesW` buffer: a chain of
/// `FILE_NOTIFY_INFORMATION` records linked by `NextEntryOffset`, each
/// carrying an action and a UTF-16 file name that is **not**
/// null-terminated and whose length is in **bytes**. Malformed tails
/// (odd or out-of-bounds lengths) stop the parse instead of reading
/// adjacent records as filename. Mapping matches the Linux arm exactly:
/// added/removed/renamed actions map to their kinds, everything else
/// (modified, and any action this code does not know) lands as
/// `Modified`, mirroring inotify's `else` branch.
#[cfg(target_os = "windows")]
fn parse_notifications(
    buffer: &[u8],
    directory: &Path,
    file_filter: &Option<PathBuf>,
) -> Vec<FileSystemEvent> {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ACTION_ADDED, FILE_ACTION_REMOVED, FILE_ACTION_RENAMED_NEW_NAME,
        FILE_ACTION_RENAMED_OLD_NAME,
    };

    const HEADER_BYTES: usize = 12; // NextEntryOffset(4) | Action(4) | FileNameLength(4)
    let mut events = Vec::new();
    let mut offset = 0usize;
    while buffer.len().saturating_sub(offset) >= HEADER_BYTES {
        let next = u32::from_ne_bytes(
            buffer[offset..offset + 4]
                .try_into()
                .expect("slice is four bytes"),
        ) as usize;
        let action = u32::from_ne_bytes(
            buffer[offset + 4..offset + 8]
                .try_into()
                .expect("slice is four bytes"),
        );
        let name_bytes = u32::from_ne_bytes(
            buffer[offset + 8..offset + 12]
                .try_into()
                .expect("slice is four bytes"),
        ) as usize;
        let name_start = offset + HEADER_BYTES;
        let name_end = name_start.saturating_add(name_bytes);
        // A valid record always has an even name length in bytes (UTF-16)
        // and a name fully inside the buffer; anything else is a
        // malformed tail and the chain ends here.
        if !name_bytes.is_multiple_of(2) || name_end > buffer.len() {
            break;
        }
        // The even-length check above makes this the complete name.
        let name_units: Vec<u16> = buffer[name_start..name_end]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_ne_bytes(*pair))
            .collect();
        // Lossy like the Linux arm's from_utf8_lossy: an unpaired
        // surrogate in a name becomes U+FFFD rather than dropping the
        // whole event.
        let name = String::from_utf16_lossy(&name_units);
        let path = directory.join(&name);
        if file_filter.as_ref().is_none_or(|filter| filter == &path) {
            let kind = if matches!(
                action,
                FILE_ACTION_RENAMED_OLD_NAME | FILE_ACTION_RENAMED_NEW_NAME
            ) {
                FileEventKind::Renamed
            } else if action == FILE_ACTION_ADDED {
                FileEventKind::Created
            } else if action == FILE_ACTION_REMOVED {
                FileEventKind::Removed
            } else {
                FileEventKind::Modified
            };
            events.push(FileSystemEvent { path, kind });
        }
        if next == 0 {
            break;
        }
        offset = offset.saturating_add(next);
    }
    events
}

#[cfg(target_os = "windows")]
impl Drop for FileSystemEventMonitor {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::IO::CancelIoEx;
        // SAFETY: `directory_handle` is a valid open handle. The pending
        // read (if any) is cancelled first so the kernel stops touching
        // the boxed buffer/OVERLAPPED before they drop; the manual-reset
        // event is signalled by the cancellation, harmlessly. Closing the
        // directory handle after cancellation also satisfies the
        // documented "close the handle to stop the watch" contract.
        unsafe { CancelIoEx(self.directory_handle, std::ptr::null_mut()) };
        unsafe { CloseHandle(self.directory_handle) };
        // SAFETY: `event_handle` is a valid open event handle.
        unsafe { CloseHandle(self.event_handle) };
    }
}

/// Non-Linux, non-Windows stand-in. macOS counterpart: FSEvents
/// (`FSEventStreamCreate`) or kqueue (`EVFILT_VNODE`) — either is a
/// legitimate port target, neither is inotify-shaped 1:1. `new` fails
/// honestly and every caller already treats that as "no live
/// file-watching" (see `sirio_ui/src/file_view.rs`, which does
/// `FileSystemEventMonitor::new(&path).ok()`).
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub struct FileSystemEventMonitor {
    _private: (),
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
impl FileSystemEventMonitor {
    pub fn new(_path: impl AsRef<Path>) -> std::io::Result<Self> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "file-system change notification is not implemented on this platform yet \
             (macOS: FSEvents/kqueue; Windows: ReadDirectoryChangesW)",
        ))
    }

    /// Reads all currently queued events without blocking.
    pub fn poll(&self) -> std::io::Result<Vec<FileSystemEvent>> {
        Ok(Vec::new())
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    fn unique_root() -> PathBuf {
        crate::scratch_dir("sirio-events")
    }

    /// Polls the monitor until an event matching `predicate` arrives or a
    /// generous deadline passes (a read is armed asynchronously, so the
    /// event may take a few scheduling ticks to land), returning every
    /// accumulated event. The 5 s deadline is CI safety, not a timing
    /// dependency: `ReadDirectoryChangesW` delivers asynchronously but
    /// promptly.
    fn poll_until(
        monitor: &FileSystemEventMonitor,
        mut predicate: impl FnMut(&FileSystemEvent) -> bool,
    ) -> Vec<FileSystemEvent> {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut events = Vec::new();
        while std::time::Instant::now() < deadline {
            events.extend(monitor.poll().unwrap());
            if events.iter().any(&mut predicate) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        events
    }

    #[test]
    fn reports_real_create_modify_and_remove_events() {
        let root = unique_root();
        fs::create_dir_all(&root).unwrap();
        let monitor = FileSystemEventMonitor::new(&root).unwrap();
        let file = root.join("note.md");
        fs::write(&file, "one").unwrap();
        fs::write(&file, "two").unwrap();
        fs::remove_file(&file).unwrap();

        let events = poll_until(&monitor, |event| {
            event.path == file && event.kind == FileEventKind::Removed
        });
        assert!(
            events
                .iter()
                .any(|event| event.path == file && event.kind == FileEventKind::Created),
            "the create arrives as Created at the file's path, like inotify's IN_CREATE"
        );
        assert!(
            events
                .iter()
                .any(|event| event.path == file && event.kind == FileEventKind::Modified),
            "an in-place rewrite arrives as Modified at the file's path \
             (FILE_NOTIFY_CHANGE_LAST_WRITE)"
        );
        assert!(
            events
                .iter()
                .any(|event| event.path == file && event.kind == FileEventKind::Removed),
            "the delete arrives as Removed at the file's path"
        );
        drop(monitor);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn replaced_while_watching_surfaces_a_rename_at_the_file_path() {
        // Most editors save by writing a temp sibling and renaming it over
        // the target — exactly the case a naive LAST_WRITE-only filter
        // misses, and the reason the watch mask includes
        // FILE_NOTIFY_CHANGE_FILE_NAME. The filter lets through only the
        // new-name record (the tmp name and the old-name records fail the
        // path comparison, same as inotify's non-matching names).
        let root = unique_root();
        fs::create_dir_all(&root).unwrap();
        let file = root.join("note.md");
        fs::write(&file, "one").unwrap();
        let monitor = FileSystemEventMonitor::new(&file).unwrap();
        let tmp = root.join("note.md.tmp");
        fs::write(&tmp, "replacement").unwrap();
        fs::rename(&tmp, &file).unwrap();

        let events = poll_until(&monitor, |event| {
            event.path == file && event.kind == FileEventKind::Renamed
        });
        assert!(
            events
                .iter()
                .any(|event| event.path == file && event.kind == FileEventKind::Renamed),
            "the rename-over arrives as Renamed at the watched file's path"
        );
        drop(monitor);
        let _ = fs::remove_dir_all(root);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    #[test]
    fn reports_real_create_modify_and_remove_events() {
        let root = crate::scratch_dir("sirio-events");
        fs::create_dir_all(&root).unwrap();
        let monitor = FileSystemEventMonitor::new(&root).unwrap();
        let file = root.join("note.md");
        fs::write(&file, "one").unwrap();
        fs::write(&file, "two").unwrap();
        fs::remove_file(&file).unwrap();

        let mut events = Vec::new();
        for _ in 0..20 {
            events.extend(monitor.poll().unwrap());
            if events
                .iter()
                .any(|event| event.kind == FileEventKind::Removed)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            events
                .iter()
                .any(|event| event.path == file && event.kind == FileEventKind::Created)
        );
        assert!(
            events
                .iter()
                .any(|event| event.path == file && event.kind == FileEventKind::Removed)
        );
        let _ = fs::remove_dir_all(root);
    }
}
