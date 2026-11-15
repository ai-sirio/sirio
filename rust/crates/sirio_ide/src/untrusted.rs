//! Reading a file whose path came from Claude: only regular files, checked on the open handle,
//! and never a read that can block on a FIFO or a device.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// What reading an untrusted path found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOutcome {
    /// There is no such file (the proposal creates one).
    Missing,
    Contents(Vec<u8>),
    Refused(Refusal),
}

/// Why a path was not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// A directory, FIFO, device or socket.
    NotRegular,
    /// Larger than the cap; the size known (at least `cap + 1`).
    TooLarge(u64),
    /// Present but not readable (permissions, an I/O error).
    Unreadable,
}

#[cfg(unix)]
fn open(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    // Non-blocking, so opening a FIFO with no writer returns at once and `fstat` can refuse it;
    // no controlling terminal, so opening a terminal device cannot take one.
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)
}

#[cfg(not(unix))]
fn open(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

/// Read `path` if it is a regular file of at most `cap` bytes. A symlink to a regular file is
/// followed; the check is on the handle, so a swap after it changes nothing.
pub fn read_regular_file(path: &Path, cap: u64) -> ReadOutcome {
    let file = match open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ReadOutcome::Missing,
        Err(_) => return ReadOutcome::Refused(Refusal::Unreadable),
    };
    let Ok(meta) = file.metadata() else {
        return ReadOutcome::Refused(Refusal::Unreadable);
    };
    if !meta.is_file() {
        return ReadOutcome::Refused(Refusal::NotRegular);
    }
    if meta.len() > cap {
        return ReadOutcome::Refused(Refusal::TooLarge(meta.len()));
    }
    let mut bytes = Vec::new();
    if file.take(cap + 1).read_to_end(&mut bytes).is_err() {
        return ReadOutcome::Refused(Refusal::Unreadable);
    }
    if bytes.len() as u64 > cap {
        return ReadOutcome::Refused(Refusal::TooLarge(cap + 1));
    }
    ReadOutcome::Contents(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Dir(PathBuf);

    impl Dir {
        fn new(tag: &str) -> Dir {
            let dir = std::env::temp_dir().join(format!("sirio-ide-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Dir(dir)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_is_missing() {
        let d = Dir::new("missing");
        assert_eq!(read_regular_file(&d.0.join("nope"), 10), ReadOutcome::Missing);
    }

    #[cfg(unix)]
    #[test]
    fn fifo_is_refused_without_blocking() {
        let d = Dir::new("fifo");
        let fifo = d.0.join("pipe");
        let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(read_regular_file(&fifo, 10));
        });
        let outcome = rx.recv_timeout(std::time::Duration::from_secs(1)).expect("returned");
        assert_eq!(outcome, ReadOutcome::Refused(Refusal::NotRegular));
    }

    #[test]
    fn directory_is_refused() {
        let d = Dir::new("dir");
        assert_eq!(
            read_regular_file(&d.0, 10),
            ReadOutcome::Refused(Refusal::NotRegular)
        );
    }

    #[test]
    fn over_cap_is_too_large() {
        let d = Dir::new("over");
        let f = d.0.join("f");
        std::fs::write(&f, [b'x'; 11]).unwrap();
        assert_eq!(read_regular_file(&f, 10), ReadOutcome::Refused(Refusal::TooLarge(11)));
    }

    #[test]
    fn at_cap_is_read() {
        let d = Dir::new("at");
        let f = d.0.join("f");
        std::fs::write(&f, [b'x'; 10]).unwrap();
        assert_eq!(read_regular_file(&f, 10), ReadOutcome::Contents(vec![b'x'; 10]));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_to_a_regular_file_is_followed() {
        let d = Dir::new("link");
        let f = d.0.join("f");
        std::fs::write(&f, b"hi").unwrap();
        let l = d.0.join("l");
        std::os::unix::fs::symlink(&f, &l).unwrap();
        assert_eq!(read_regular_file(&l, 10), ReadOutcome::Contents(b"hi".to_vec()));
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            return; // root reads mode 000 files
        }
        let d = Dir::new("unreadable");
        let f = d.0.join("f");
        std::fs::write(&f, b"secret").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert_eq!(read_regular_file(&f, 10), ReadOutcome::Refused(Refusal::Unreadable));
    }
}
