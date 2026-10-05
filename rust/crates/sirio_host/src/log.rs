//! The host's own log (spec §5.8): 4 MiB, two old generations. Lines carry
//! names and ids, never paths a user typed, prompts or scrollback.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

const ROTATE_AT: u64 = 4 << 20;

pub struct Log {
    path: PathBuf,
    lock: Mutex<()>,
}

impl Log {
    pub fn new(path: PathBuf) -> Self {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    pub fn line(&self, event: &str, detail: &str) {
        let _guard = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        if fs::metadata(&self.path)
            .map(|m| m.len() >= ROTATE_AT)
            .unwrap_or(false)
        {
            let one = self.path.with_extension("log.1");
            let _ = fs::rename(&one, self.path.with_extension("log.2"));
            let _ = fs::rename(&self.path, &one);
        }
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(f, "{secs}\t{}\t{event}\t{detail}", std::process::id());
        }
    }
}
