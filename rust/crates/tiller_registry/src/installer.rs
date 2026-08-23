//! Installing one agent: download, verify what can be verified, unpack
//! safely, and commit by rewriting the manifest.
//!
//! The commit point matters. `rename` onto a non-empty directory fails on
//! Linux, so a directory swap is necessarily remove-then-move and a crash
//! inside that window leaves *nothing*. Installing into
//! `<root>/<id>/<version>/` and letting the manifest name the version turns
//! the commit into a single-file rename, which is genuinely atomic: a crash
//! anywhere before it leaves the previously recorded version fully intact.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::model::{BinaryArtifact, Distribution, RegistryAgent};
use crate::resolve::{InstalledAgent, Integrity};
use crate::store::InstallStore;

/// Ceilings, so a hostile or broken artifact cannot fill the disk or hang
/// the row forever. The largest artifact in the registry today is tens of
/// megabytes.
const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 1024 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("{agent}: no artifact published for this platform")]
    NoArtifactForPlatform { agent: String },
    #[error("{agent}: archive format is not supported ({url})")]
    UnsupportedArchive { agent: String, url: String },
    #[error("{agent}: distribution kind is not supported")]
    UnsupportedDistribution { agent: String },
    #[error("{agent}: checksum did not match; nothing was installed")]
    ChecksumMismatch { agent: String },
    #[error("{agent}: archive entry {entry} escapes the destination")]
    UnsafeArchiveEntry { agent: String, entry: String },
    #[error("{agent}: {cmd} is missing from the unpacked artifact")]
    MissingCommand { agent: String, cmd: String },
    #[error("{agent}: another install is already running")]
    AlreadyRunning { agent: String },
    #[error("{agent}: {message}")]
    Failed { agent: String, message: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnpackKind {
    Zip,
    TarGz,
    /// Not an archive: `sigit` publishes bare executables.
    BareExecutable,
    /// `.tar.bz2` (goose). Named rather than silently skipped.
    Unsupported,
}

pub struct Installer {
    store: InstallStore,
}

pub fn unpack_kind(url: &str) -> UnpackKind {
    let name = url.rsplit('/').next().unwrap_or(url);
    if name.ends_with(".zip") {
        UnpackKind::Zip
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        UnpackKind::TarGz
    } else if name.ends_with(".tar.bz2") || name.ends_with(".tar.xz") {
        UnpackKind::Unsupported
    } else {
        UnpackKind::BareExecutable
    }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `None` means the registry published no hash — recorded as
/// [`Integrity::None`], never silently treated as verified.
pub(crate) fn verify_sha256(bytes: &[u8], expected: Option<&str>) -> Result<Integrity, ()> {
    match expected {
        None => Ok(Integrity::None),
        Some(expected) if sha256_hex(bytes).eq_ignore_ascii_case(expected) => Ok(Integrity::Sha256),
        Some(_) => Err(()),
    }
}

/// Rejects both traversal and absolute paths. Callers must additionally
/// reject non-regular, non-directory entries — a symlink pointing outside
/// the destination passes a path check and still escapes.
pub(crate) fn safe_entry_path(destination: &Path, entry: &Path) -> Result<PathBuf, ()> {
    if entry.is_absolute() {
        return Err(());
    }
    let mut resolved = destination.to_path_buf();
    for component in entry.components() {
        match component {
            std::path::Component::Normal(part) => resolved.push(part),
            std::path::Component::CurDir => {}
            _ => return Err(()),
        }
    }
    resolved.starts_with(destination).then_some(resolved).ok_or(())
}

pub(crate) fn staging_dir(root: &Path, id: &str, version: &str) -> PathBuf {
    root.join(".staging").join(format!("{id}-{version}-{}", std::process::id()))
}

impl Installer {
    pub fn new(store: InstallStore) -> Self {
        Self { store }
    }

    /// Removes staging left by a process that is gone. Called once at
    /// startup: a killed install must not leak a directory — or a lock —
    /// forever.
    ///
    /// Lock files are swept unconditionally, because a lock that survived
    /// into a new run is stale by construction: the process that took it
    /// does not exist any more. (Two Tiller instances installing the same
    /// agent at the same moment is out of scope — the app is
    /// single-instance by the control-socket design — and the worst case if
    /// it ever happened is a redundant download into separate staging
    /// directories, not corruption, because the commit point is a
    /// single-file rename.)
    pub fn sweep_staging(&self) -> anyhow::Result<()> {
        let staging_root = self.store.root().join(".staging");
        let Ok(entries) = std::fs::read_dir(&staging_root) else {
            return Ok(());
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if name.ends_with(".lock") {
                let _ = std::fs::remove_file(&path);
                continue;
            }
            let owner: Option<u32> = name.rsplit('-').next().and_then(|pid| pid.parse().ok());
            if owner == Some(std::process::id()) {
                continue;
            }
            let _ = std::fs::remove_dir_all(&path);
        }
        Ok(())
    }

    pub fn install(
        &self,
        agent: &RegistryAgent,
        platform_key: &str,
    ) -> Result<InstalledAgent, InstallError> {
        // Same precedence `resolve` uses, and for the same reason: prefer a
        // binary artifact built for this machine, fall back to npx only when
        // the agent publishes nothing for this platform. An agent may declare
        // both — `kilo` and `sigit` do.
        if let Some(artifact) = agent.distributions.iter().find_map(|distribution| match distribution
        {
            Distribution::Binary(artifacts) => artifacts.get(platform_key),
            _ => None,
        }) {
            return self.install_binary(agent, artifact);
        }
        // Task 6 replaces this arm with the real npx install.
        Err(InstallError::UnsupportedDistribution { agent: agent.id.clone() })
    }

    fn install_binary(
        &self,
        agent: &RegistryAgent,
        artifact: &BinaryArtifact,
    ) -> Result<InstalledAgent, InstallError> {
        let kind = unpack_kind(&artifact.archive);
        if kind == UnpackKind::Unsupported {
            return Err(InstallError::UnsupportedArchive {
                agent: agent.id.clone(),
                url: artifact.archive.clone(),
            });
        }

        let staging = staging_dir(self.store.root(), &agent.id, &agent.version);
        let _guard = InstallGuard::acquire(self.store.root(), &agent.id)?;
        let fail = |message: String| InstallError::Failed { agent: agent.id.clone(), message };

        std::fs::create_dir_all(&staging).map_err(|error| fail(error.to_string()))?;

        let bytes = download(&artifact.archive).map_err(|error| fail(error.to_string()))?;
        let integrity = verify_sha256(&bytes, artifact.sha256.as_deref())
            .map_err(|()| InstallError::ChecksumMismatch { agent: agent.id.clone() })?;

        match kind {
            UnpackKind::Zip => unpack_zip(&agent.id, &bytes, &staging)?,
            UnpackKind::TarGz => unpack_tar_gz(&agent.id, &bytes, &staging)?,
            UnpackKind::BareExecutable => {
                let name = artifact.cmd.trim_start_matches("./");
                std::fs::write(staging.join(name), &bytes)
                    .map_err(|error| fail(error.to_string()))?;
            }
            UnpackKind::Unsupported => unreachable!("rejected above"),
        }

        let relative = artifact.cmd.trim_start_matches("./");
        let command = staging.join(relative);
        if !command.exists() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(InstallError::MissingCommand {
                agent: agent.id.clone(),
                cmd: artifact.cmd.clone(),
            });
        }
        set_executable(&command).map_err(|error| fail(error.to_string()))?;

        let final_dir = self.store.root().join(&agent.id).join(&agent.version);
        if let Some(parent) = final_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|error| fail(error.to_string()))?;
        }
        let _ = std::fs::remove_dir_all(&final_dir);
        std::fs::rename(&staging, &final_dir).map_err(|error| fail(error.to_string()))?;

        let installed = InstalledAgent {
            id: agent.id.clone(),
            version: agent.version.clone(),
            executable: final_dir.join(relative),
            args: artifact.args.clone(),
            integrity,
        };
        // The commit point: everything above is recoverable, this is not.
        self.store.write(&installed).map_err(|error| fail(error.to_string()))?;
        Ok(installed)
    }
}

/// A per-agent lock so a double-click on Install is a no-op rather than two
/// concurrent unpacks into the same path.
struct InstallGuard {
    path: PathBuf,
}

impl InstallGuard {
    fn acquire(root: &Path, id: &str) -> Result<Self, InstallError> {
        let path = root.join(".staging").join(format!("{id}.lock"));
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| InstallError::AlreadyRunning { agent: id.to_string() })?;
        Ok(Self { path })
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn download(url: &str) -> anyhow::Result<Vec<u8>> {
    // Same request-builder spelling verified against ureq 3.4 in
    // `client.rs`; only the body handling differs (binary reader with a
    // size cap instead of read_to_string).
    let response = ureq::get(url)
        .config()
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .build()
        .call()?;
    let mut body = response.into_body().into_reader().take(MAX_DOWNLOAD_BYTES);
    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn unpack_zip(agent: &str, bytes: &[u8], destination: &Path) -> Result<(), InstallError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|error| InstallError::Failed { agent: agent.into(), message: error.to_string() })?;
    let mut written: u64 = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| InstallError::Failed {
            agent: agent.into(),
            message: error.to_string(),
        })?;
        let Some(name) = entry.enclosed_name() else {
            return Err(InstallError::UnsafeArchiveEntry {
                agent: agent.into(),
                entry: entry.name().to_string(),
            });
        };
        let target = safe_entry_path(destination, &name).map_err(|()| {
            InstallError::UnsafeArchiveEntry { agent: agent.into(), entry: name.display().to_string() }
        })?;
        if entry.is_dir() {
            let _ = std::fs::create_dir_all(&target);
            continue;
        }
        written += entry.size();
        if written > MAX_UNPACKED_BYTES {
            return Err(InstallError::Failed {
                agent: agent.into(),
                message: "unpacked size exceeds the ceiling".into(),
            });
        }
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut file = std::fs::File::create(&target).map_err(|error| InstallError::Failed {
            agent: agent.into(),
            message: error.to_string(),
        })?;
        std::io::copy(&mut entry, &mut file).map_err(|error| InstallError::Failed {
            agent: agent.into(),
            message: error.to_string(),
        })?;
    }
    Ok(())
}

fn unpack_tar_gz(agent: &str, bytes: &[u8], destination: &Path) -> Result<(), InstallError> {
    let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    let entries = archive.entries().map_err(|error| InstallError::Failed {
        agent: agent.into(),
        message: error.to_string(),
    })?;
    let mut written: u64 = 0;
    for entry in entries {
        let mut entry = entry.map_err(|error| InstallError::Failed {
            agent: agent.into(),
            message: error.to_string(),
        })?;
        // A symlink or hardlink can point outside the destination and pass
        // a pure path check, so entry *type* is filtered before the path is
        // even considered.
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir()) {
            return Err(InstallError::UnsafeArchiveEntry {
                agent: agent.into(),
                entry: format!("{kind:?}"),
            });
        }
        let path = entry.path().map_err(|error| InstallError::Failed {
            agent: agent.into(),
            message: error.to_string(),
        })?;
        let target = safe_entry_path(destination, &path).map_err(|()| {
            InstallError::UnsafeArchiveEntry { agent: agent.into(), entry: path.display().to_string() }
        })?;
        if kind.is_dir() {
            let _ = std::fs::create_dir_all(&target);
            continue;
        }
        written += entry.header().size().unwrap_or(0);
        if written > MAX_UNPACKED_BYTES {
            return Err(InstallError::Failed {
                agent: agent.into(),
                message: "unpacked size exceeds the ceiling".into(),
            });
        }
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        entry.unpack(&target).map_err(|error| InstallError::Failed {
            agent: agent.into(),
            message: error.to_string(),
        })?;
    }
    Ok(())
}

/// Exactly one path, so `tar` (which preserves modes) and `zip` (which does
/// not) end up behaving the same and nothing extra is granted.
fn set_executable(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        std::fs::set_permissions(path, permissions)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unpack_kind_comes_from_the_url() {
        // Measured across the registry's 95 artifacts: tar.gz 55, zip 32,
        // tar.bz2 4, bare executable 4. The bare case has no unpacking step
        // at all, so it cannot be an afterthought.
        assert_eq!(unpack_kind("https://x/opencode-linux-x64.zip"), UnpackKind::Zip);
        assert_eq!(unpack_kind("https://x/agent-linux.tar.gz"), UnpackKind::TarGz);
        assert_eq!(unpack_kind("https://x/agent-linux.tgz"), UnpackKind::TarGz);
        assert_eq!(
            unpack_kind("https://x/goose-x86_64-unknown-linux-gnu.tar.bz2"),
            UnpackKind::Unsupported
        );
        assert_eq!(unpack_kind("https://x/sigit-linux-amd64"), UnpackKind::BareExecutable);
        assert_eq!(unpack_kind("https://x/sigit-win-amd64.exe"), UnpackKind::BareExecutable);
    }

    #[test]
    fn a_mismatched_hash_aborts_the_install() {
        let bytes = b"payload";
        assert!(verify_sha256(bytes, Some(&"ff".repeat(32))).is_err());
    }

    #[test]
    fn a_matching_hash_passes_and_is_recorded() {
        let bytes = b"payload";
        let digest = sha256_hex(bytes);
        assert_eq!(verify_sha256(bytes, Some(&digest)).unwrap(), Integrity::Sha256);
    }

    #[test]
    fn an_absent_hash_installs_but_is_recorded_as_unverified() {
        // 9 of the 18 binary agents publish at least one artifact with no
        // hash. Refusing them buys no safety — the fallback is downloading
        // the same file by hand — so the fact is recorded instead.
        assert_eq!(verify_sha256(b"payload", None).unwrap(), Integrity::None);
    }

    #[test]
    fn an_entry_escaping_the_destination_is_refused() {
        let dest = std::path::Path::new("/data/staging");
        assert!(safe_entry_path(dest, std::path::Path::new("../../etc/passwd")).is_err());
        assert!(safe_entry_path(dest, std::path::Path::new("/etc/passwd")).is_err());
        assert!(safe_entry_path(dest, std::path::Path::new("bin/agent")).is_ok());
    }

    #[test]
    fn a_staging_directory_is_named_for_its_owner() {
        let staging = staging_dir(std::path::Path::new("/data"), "codex-acp", "1.6.2");
        let name = staging.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("codex-acp-1.6.2-"));
        assert!(
            name.ends_with(&std::process::id().to_string()),
            "the pid lets a startup sweep tell live staging from abandoned"
        );
        assert_eq!(staging.parent().unwrap(), std::path::Path::new("/data/.staging"));
    }

    #[test]
    fn sweeping_removes_staging_left_by_a_dead_process() {
        let root = std::env::temp_dir()
            .join(format!("tiller-sweep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dead = root.join(".staging/codex-acp-1.6.2-1");
        let live = staging_dir(&root, "codex-acp", "1.6.2");
        std::fs::create_dir_all(&dead).unwrap();
        std::fs::create_dir_all(&live).unwrap();

        Installer::new(InstallStore::new(root.clone())).sweep_staging().unwrap();

        assert!(!dead.exists(), "abandoned staging is collected");
        assert!(live.exists(), "this process's own staging is left alone");
    }

    #[test]
    fn sweeping_also_clears_a_lock_left_by_a_killed_install() {
        // Without this, one killed install makes that agent permanently
        // uninstallable: `InstallGuard::acquire` uses create_new, so the
        // orphaned lock rejects every later attempt with AlreadyRunning.
        let root = std::env::temp_dir()
            .join(format!("tiller-sweep-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let lock = root.join(".staging/codex-acp.lock");
        std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
        std::fs::write(&lock, "").unwrap();

        Installer::new(InstallStore::new(root.clone())).sweep_staging().unwrap();

        assert!(!lock.exists(), "a lock that outlived its process is stale");
    }

    #[test]
    fn an_unsupported_archive_names_the_format() {
        let error = InstallError::UnsupportedArchive {
            agent: "goose".into(),
            url: "https://x/goose.tar.bz2".into(),
        };
        let rendered = error.to_string();
        assert!(rendered.contains("goose"));
        assert!(rendered.contains("tar.bz2"), "the gap is visible, not mysterious");
    }
}
