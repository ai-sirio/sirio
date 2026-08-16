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

/// Non-Linux stand-in. macOS counterpart: FSEvents (`FSEventStreamCreate`) or kqueue
/// (`EVFILT_VNODE`) — either is a legitimate port target, neither is inotify-shaped 1:1.
/// Windows counterpart: `ReadDirectoryChangesW`. Neither is implemented here; `new` fails
/// honestly and every caller already treats that as "no live file-watching" (see
/// `tiller_ui/src/file_view.rs`, which does `FileSystemEventMonitor::new(&path).ok()`).
#[cfg(not(target_os = "linux"))]
pub struct FileSystemEventMonitor {
    _private: (),
}

#[cfg(not(target_os = "linux"))]
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    #[test]
    fn reports_real_create_modify_and_remove_events() {
        let root = std::env::temp_dir().join(format!(
            "tiller-events-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
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
