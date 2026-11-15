//! The lockfile Claude Code finds this IDE by: `<dir>/<port>.lock`. The directory is always
//! the caller's; this module never computes a path in the user's home.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

/// What varies in the lockfile; `transport` and `runningInWindows` are constants of the format.
pub struct LockBody {
    pub pid: u32,
    pub workspace_folders: Vec<PathBuf>,
    pub ide_name: String,
    pub auth_token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Wire<'a> {
    pid: u32,
    workspace_folders: &'a [PathBuf],
    ide_name: &'a str,
    transport: &'static str,
    auth_token: &'a str,
    running_in_windows: bool,
}

/// A lockfile this instance wrote, for the port it bound. Only that file is ever touched.
pub struct Lockfile {
    path: PathBuf,
}

impl Lockfile {
    /// Create `dir` (0700 on unix) if missing, then write `<port>.lock`, replacing a stale one.
    pub fn write(dir: &Path, port: u16, body: &LockBody) -> io::Result<Lockfile> {
        create_dir_owner_only(dir)?;
        let lock = Lockfile { path: dir.join(format!("{port}.lock")) };
        lock.rewrite(body)?;
        Ok(lock)
    }

    /// Replace the content through a temp file renamed over the lockfile.
    pub fn rewrite(&self, body: &LockBody) -> io::Result<()> {
        let wire = Wire {
            pid: body.pid,
            workspace_folders: &body.workspace_folders,
            ide_name: &body.ide_name,
            transport: "ws",
            auth_token: &body.auth_token,
            running_in_windows: false,
        };
        let json = serde_json::to_vec(&wire).map_err(io::Error::other)?;
        let tmp = self.path.with_extension("lock.tmp");
        let result = write_owner_only(&tmp, &json).and_then(|()| std::fs::rename(&tmp, &self.path));
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn remove(self) -> io::Result<()> {
        match std::fs::remove_file(&self.path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

fn create_dir_owner_only(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

fn write_owner_only(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sirio_ide_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn body(folders: &[&str]) -> LockBody {
        LockBody {
            pid: 4242,
            workspace_folders: folders.iter().map(PathBuf::from).collect(),
            ide_name: "Sirio".into(),
            auth_token: "0123456789abcdef0123456789abcdef".into(),
        }
    }

    fn read(l: &Lockfile) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(l.path()).unwrap()).unwrap()
    }

    #[test]
    fn lockfile_body_has_exactly_the_six_keys() {
        let dir = tmp("keys");
        let l = Lockfile::write(&dir, 12345, &body(&["/a"])).unwrap();
        let v = read(&l);
        let keys: BTreeSet<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        let want: BTreeSet<&str> =
            ["pid", "workspaceFolders", "ideName", "transport", "authToken", "runningInWindows"]
                .into_iter()
                .collect();
        assert_eq!(keys, want);
        assert_eq!(v["transport"], "ws");
        assert_eq!(v["runningInWindows"], false);
        assert_eq!(l.path().file_name().unwrap(), "12345.lock");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn lockfile_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmp("mode");
        let l = Lockfile::write(&dir, 1, &body(&[])).unwrap();
        assert_eq!(std::fs::metadata(l.path()).unwrap().permissions().mode() & 0o777, 0o600);
        l.rewrite(&body(&["/b"])).unwrap();
        assert_eq!(std::fs::metadata(l.path()).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn missing_dir_is_created_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmp("dirmode").join("nested");
        Lockfile::write(&dir, 1, &body(&[])).unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn lockfile_overwrites_a_stale_file_of_the_same_name() {
        let dir = tmp("stale");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("777.lock"), b"stale garbage").unwrap();
        let l = Lockfile::write(&dir, 777, &body(&["/fresh"])).unwrap();
        assert_eq!(read(&l)["workspaceFolders"][0], "/fresh");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rewrite_replaces_folders_atomically() {
        let dir = tmp("rewrite");
        let l = Lockfile::write(&dir, 55, &body(&["/a"])).unwrap();
        l.rewrite(&body(&["/b", "/c"])).unwrap();
        let v = read(&l);
        assert_eq!(v["workspaceFolders"], serde_json::json!(["/b", "/c"]));
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, vec!["55.lock".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_deletes_only_its_own_file() {
        let dir = tmp("remove");
        let l = Lockfile::write(&dir, 99, &body(&[])).unwrap();
        std::fs::write(dir.join("1234.lock"), b"{}").unwrap();
        let p = l.path().to_path_buf();
        l.remove().unwrap();
        assert!(!p.exists());
        assert!(dir.join("1234.lock").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
