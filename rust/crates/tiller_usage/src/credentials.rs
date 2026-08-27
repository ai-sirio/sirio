//! An on-disk credential store for provider session cookies — the Linux
//! counterpart of the Swift app's `KeychainCredentialStore` (F-SET-12).
//!
//! # Security posture, stated plainly
//!
//! Values are stored as **plaintext JSON** in a file created with mode
//! `0600` (owner read/write only) under the user's XDG data directory.
//! That is the same at-rest protection as the credential files this crate
//! already reads as its ground truth — `~/.codex/auth.json` and
//! `~/.claude/.credentials.json` — namely the OS user boundary, nothing
//! more. Anyone weighing where to keep a cookie should weigh it exactly
//! like those files.
//!
//! The Secret Service API (gnome-keyring) was considered and deliberately
//! not used: unattended `cargo test` runs and parallel app instances
//! writing into the user's *login keyring* are ambient credential
//! mutations no human approved, an unlocked-keyring prompt can block a
//! headless session, and a keyring-backed store cannot be pointed at a
//! hermetic fixture. `TILLER_CREDENTIALS` overrides the file path, which
//! is what tests and the capture lane use for isolation.

use std::path::{Path, PathBuf};

/// Why a credential operation failed. Save/Clear surfaces this text in the
/// settings UI (the Swift original's "Failed to update Keychain…" label).
#[derive(Debug)]
pub enum CredentialStoreError {
    /// The store file (or its directory) could not be read or written.
    Io {
        /// The store path the operation was against.
        path: PathBuf,
        /// The underlying filesystem error.
        source: std::io::Error,
    },
    /// No `TILLER_CREDENTIALS`, no `XDG_DATA_HOME` and no platform fallback
    /// (`LOCALAPPDATA` on Windows, `HOME` on POSIX): there is nowhere to put
    /// the store.
    NoHome,
}

impl std::fmt::Display for CredentialStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "could not update {}: {source}", path.display())
            }
            Self::NoHome => {
                #[cfg(windows)]
                let message = "no LOCALAPPDATA or XDG_DATA_HOME to place the credential store";
                #[cfg(not(windows))]
                let message = "no HOME or XDG_DATA_HOME to place the credential store";
                write!(f, "{message}")
            }
        }
    }
}

impl std::error::Error for CredentialStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::NoHome => None,
        }
    }
}

/// A key→value file of provider credentials. Construction never touches
/// the filesystem; each operation re-reads the file, so concurrent app
/// instances see each other's writes on their next read. Clone is a cheap
/// handle copy (the path), not a data copy — clones observe the same file.
#[derive(Clone)]
pub struct CredentialStore {
    path: PathBuf,
}

impl CredentialStore {
    /// The store at an explicit path — the test/fixture seam.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The store at the environment's path: `$TILLER_CREDENTIALS` if set,
    /// else `$XDG_DATA_HOME/tiller/credentials.json`, else the platform
    /// fallback — `%LOCALAPPDATA%\Tiller\credentials.json` on Windows
    /// (where `HOME` is ignored entirely), `$HOME/.local/share/tiller/credentials.json`
    /// on POSIX.
    pub fn from_env() -> Result<Self, CredentialStoreError> {
        resolve_store_path(
            std::env::var_os("TILLER_CREDENTIALS").as_deref(),
            std::env::var_os("XDG_DATA_HOME").as_deref(),
            std::env::var_os("HOME").as_deref(),
            std::env::var_os("LOCALAPPDATA").as_deref(),
        )
        .map(Self::at)
    }

    /// The value stored under `key`, or `None` when the file is missing,
    /// unreadable or malformed — read parity with the Swift Keychain read,
    /// where any lookup failure collapses to "no cookie". Writes are the
    /// operations that surface errors.
    pub fn get(&self, key: &str) -> Option<String> {
        let raw = std::fs::read_to_string(&self.path).ok()?;
        let map: serde_json::Value = serde_json::from_str(&raw).ok()?;
        map.get(key)?.as_str().map(str::to_string)
    }

    /// Stores `value` under `key`, creating the file (mode `0600`) and its
    /// directory (mode `0700`) as needed. A malformed store file is
    /// replaced — its other entries were already unreadable.
    pub fn set(&self, key: &str, value: &str) -> Result<(), CredentialStoreError> {
        let mut map = self.entries();
        map.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
        self.write(&map)
    }

    /// Removes `key`. Removing an absent key (or from an absent file) is
    /// `Ok` — the observable state, "no credential stored", already holds.
    pub fn delete(&self, key: &str) -> Result<(), CredentialStoreError> {
        if !self.path.exists() {
            return Ok(());
        }
        let mut map = self.entries();
        map.remove(key);
        self.write(&map)
    }

    fn entries(&self) -> serde_json::Map<String, serde_json::Value> {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .and_then(|value| match value {
                serde_json::Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn write(
        &self,
        map: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), CredentialStoreError> {
        let io_error = |source| CredentialStoreError::Io {
            path: self.path.clone(),
            source,
        };

        if let Some(dir) = self.path.parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir).map_err(io_error)?;
            // Windows counterpart of "owner-only": not `mode()` (no POSIX permission
            // bits) but a DACL restricted to the current user, e.g. via
            // `SetNamedSecurityInfoW` or the `windows-acl` crate. Not implemented — on
            // Windows this directory is left at its inherited (usually already
            // per-user, under %LOCALAPPDATA%) ACL rather than pretending to tighten it.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
            }
        }

        // Write-then-rename so a concurrent reader never sees a torn file,
        // with the temp file already at 0600 so the credential is never on
        // disk with wider permissions, however briefly.
        let body = serde_json::Value::Object(map.clone()).to_string();
        let tmp = self.path.with_extension("json.tmp");
        {
            use std::io::Write;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            // Same Windows gap as the directory above: no `mode()` equivalent, real
            // fix is a per-user DACL on the file, not implemented here.
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&tmp).map_err(io_error)?;
            file.write_all(body.as_bytes()).map_err(io_error)?;
        }
        // `create(true)` keeps an existing temp file's old mode; enforce.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
                .map_err(io_error)?;
        }
        std::fs::rename(&tmp, &self.path).map_err(io_error)
    }
}

/// Pure path resolution, separated from the process environment so the
/// precedence is testable without mutating env vars (which are process
/// global while tests run in parallel).
///
/// Precedence: `explicit`, then `xdg_data_home`, then the platform
/// fallback — `local_app_data` under `Tiller/credentials.json` on Windows,
/// `home` under `.local/share/tiller/credentials.json` on POSIX. On Windows
/// `home` is ignored entirely: Git Bash exports an MSYS `HOME` whose drive
/// form would resolve to a bogus drive-root path, the #230 class of bug.
fn resolve_store_path(
    explicit: Option<&std::ffi::OsStr>,
    xdg_data_home: Option<&std::ffi::OsStr>,
    #[cfg(windows)] _home: Option<&std::ffi::OsStr>,
    #[cfg(not(windows))] home: Option<&std::ffi::OsStr>,
    #[cfg(windows)] local_app_data: Option<&std::ffi::OsStr>,
    #[cfg(not(windows))] _local_app_data: Option<&std::ffi::OsStr>,
) -> Result<PathBuf, CredentialStoreError> {
    if let Some(path) = explicit.filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    if let Some(data) = xdg_data_home.filter(|data| !data.is_empty()) {
        return Ok(Path::new(data).join("tiller/credentials.json"));
    }
    #[cfg(windows)]
    {
        if let Some(root) = local_app_data.filter(|root| !root.is_empty()) {
            return Ok(Path::new(root).join("Tiller").join("credentials.json"));
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = home.filter(|home| !home.is_empty()) {
            return Ok(Path::new(home).join(".local/share/tiller/credentials.json"));
        }
    }
    Err(CredentialStoreError::NoHome)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempStoreDir(PathBuf);

    impl TempStoreDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("tiller-credentials-{label}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            Self(path)
        }

        fn store_path(&self) -> PathBuf {
            self.0.join("nested/credentials.json")
        }
    }

    impl Drop for TempStoreDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn round_trips_and_deletes_without_disturbing_other_keys() {
        let dir = TempStoreDir::new("round-trip");
        let store = CredentialStore::at(dir.store_path());

        assert_eq!(store.get("opencode-go-cookie"), None, "empty store");

        store
            .set("opencode-go-cookie", "auth=Fe26.2**abc")
            .expect("set creates directory and file");
        store
            .set("ollama-cloud-cookie", "session=xyz")
            .expect("second key");

        assert_eq!(
            store.get("opencode-go-cookie").as_deref(),
            Some("auth=Fe26.2**abc")
        );
        assert_eq!(
            store.get("ollama-cloud-cookie").as_deref(),
            Some("session=xyz")
        );

        store.delete("opencode-go-cookie").expect("delete");
        assert_eq!(store.get("opencode-go-cookie"), None, "deleted key is gone");
        assert_eq!(
            store.get("ollama-cloud-cookie").as_deref(),
            Some("session=xyz"),
            "deleting one key never disturbs another"
        );

        store
            .delete("opencode-go-cookie")
            .expect("deleting an absent key is Ok — the state already holds");
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_created_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempStoreDir::new("perms");
        let store = CredentialStore::at(dir.store_path());
        store.set("k", "secret").expect("set");

        let mode = std::fs::metadata(dir.store_path())
            .expect("store file exists")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "the credential file is owner read/write only"
        );
    }

    #[test]
    fn a_malformed_file_reads_as_absent_and_set_heals_it() {
        let dir = TempStoreDir::new("malformed");
        let store = CredentialStore::at(dir.store_path());
        std::fs::create_dir_all(dir.store_path().parent().unwrap()).unwrap();
        std::fs::write(dir.store_path(), "not json at all").unwrap();

        assert_eq!(
            store.get("k"),
            None,
            "a malformed store reads as absent, like a failed Keychain lookup"
        );
        store
            .set("k", "v")
            .expect("set replaces the malformed file");
        assert_eq!(store.get("k").as_deref(), Some("v"));
    }

    #[test]
    fn delete_on_a_missing_file_is_ok_and_creates_nothing() {
        let dir = TempStoreDir::new("missing-delete");
        let store = CredentialStore::at(dir.store_path());
        store.delete("k").expect("nothing to remove is success");
        assert!(
            !dir.store_path().exists(),
            "deleting from an absent store must not create the file"
        );
    }

    #[test]
    fn store_path_precedence_is_explicit_then_xdg_then_home() {
        use std::ffi::OsStr;
        let local = OsStr::new(r"C:\Users\u\AppData\Local");

        assert_eq!(
            resolve_store_path(
                Some(OsStr::new("/tmp/x.json")),
                Some(OsStr::new("/xdg")),
                Some(OsStr::new("/home/u")),
                Some(local),
            )
            .unwrap(),
            PathBuf::from("/tmp/x.json"),
            "TILLER_CREDENTIALS wins"
        );
        assert_eq!(
            resolve_store_path(
                None,
                Some(OsStr::new("/xdg")),
                Some(OsStr::new("/home/u")),
                Some(local),
            )
            .unwrap(),
            PathBuf::from("/xdg/tiller/credentials.json")
        );
        #[cfg(not(windows))]
        {
            assert_eq!(
                resolve_store_path(Some(OsStr::new("")), None, Some(OsStr::new("/home/u")), None)
                    .unwrap(),
                PathBuf::from("/home/u/.local/share/tiller/credentials.json"),
                "an empty override is no override"
            );
        }
        #[cfg(windows)]
        {
            assert_eq!(
                resolve_store_path(Some(OsStr::new("")), None, None, Some(local)).unwrap(),
                PathBuf::from(r"C:\Users\u\AppData\Local\Tiller\credentials.json"),
                "an empty override is no override"
            );
        }
        assert!(matches!(
            resolve_store_path(None, None, None, None),
            Err(CredentialStoreError::NoHome)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn the_store_resolves_under_local_app_data_on_windows() {
        use std::ffi::OsStr;

        let root = OsStr::new(r"C:\Users\x\AppData\Local");
        let path = resolve_store_path(None, None, None, Some(root))
            .expect("LOCALAPPDATA is a valid Windows fallback");
        assert!(
            path.starts_with(r"C:\Users\x\AppData\Local"),
            "the store lives under the given LOCALAPPDATA root: {}",
            path.display()
        );
        assert!(
            path.ends_with(r"Tiller\credentials.json"),
            "the store is Tiller\\credentials.json under that root: {}",
            path.display()
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_posix_home_is_ignored_on_windows() {
        use std::ffi::OsStr;

        let path = resolve_store_path(
            None,
            None,
            Some(OsStr::new("/c/Users/x")),
            Some(OsStr::new(r"C:\Users\x\AppData\Local")),
        )
        .expect("LOCALAPPDATA wins over the MSYS HOME that Git Bash exports");
        let text = path.to_string_lossy();
        assert!(
            text.starts_with(r"C:\Users\x\AppData\Local"),
            "the store resolves under LOCALAPPDATA, not the MSYS home: {text}"
        );
        assert!(
            !text.contains(".local"),
            "no POSIX .local layout leaks into the Windows path: {text}"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn the_posix_layout_is_unchanged() {
        use std::ffi::OsStr;

        assert_eq!(
            resolve_store_path(None, None, Some(OsStr::new("/home/u")), None).unwrap(),
            PathBuf::from("/home/u/.local/share/tiller/credentials.json")
        );
    }
}
