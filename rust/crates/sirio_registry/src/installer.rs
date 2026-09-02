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
/// npm puts its resolution error at the top of stderr and its
/// `npm ERR!` summary at the bottom; keeping both ends and eliding the
/// middle preserves what an operator needs without letting a chatty or
/// pathological run accumulate unbounded memory.
const STDERR_HEAD_CAP: usize = 2 * 1024;
const STDERR_TAIL_CAP: usize = 2 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
/// Same ceiling family as the download above: `Command::output()` would
/// otherwise wait forever, leaving the row in flight and its per-agent
/// lock taken.
const NPM_INSTALL_TIMEOUT: Duration = Duration::from_secs(600);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("{agent}: no artifact published for this platform")]
    NoArtifactForPlatform { agent: String },
    #[error("{agent}: archive format is not supported ({url})")]
    UnsupportedArchive { agent: String, url: String },
    #[error("{agent}: distribution kind '{kind}' is not supported")]
    UnsupportedDistribution { agent: String, kind: String },
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
    /// The npm invocation (program plus leading arguments), injectable so
    /// tests can point the npx path at a fake; `npm_install_argv`'s
    /// arguments are appended after it.
    npm_argv: Vec<String>,
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
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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
    resolved
        .starts_with(destination)
        .then_some(resolved)
        .ok_or(())
}

pub(crate) fn staging_dir(root: &Path, id: &str, version: &str) -> PathBuf {
    root.join(".staging")
        .join(format!("{id}-{version}-{}", std::process::id()))
}

impl Installer {
    pub fn new(store: InstallStore) -> Self {
        Self {
            store,
            npm_argv: vec![npm_binary().to_string()],
        }
    }

    /// Points the installer at a stand-in npm. Test-only seam: it exists so
    /// the npx path can be exercised without a live npm on PATH.
    #[cfg(test)]
    fn with_npm(mut self, argv: Vec<String>) -> Self {
        self.npm_argv = argv;
        self
    }

    /// Removes staging left by a process that is gone. Called once at
    /// startup: a killed install must not leak a directory — or a lock —
    /// forever.
    ///
    /// Lock files are swept unconditionally, because a lock that survived
    /// into a new run is stale by construction: the process that took it
    /// does not exist any more. (Two Sirio instances installing the same
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
        if let Some(artifact) =
            agent
                .distributions
                .iter()
                .find_map(|distribution| match distribution {
                    Distribution::Binary(artifacts) => artifacts.get(platform_key),
                    _ => None,
                })
        {
            return self.install_binary(agent, artifact);
        }
        // Same fallback `resolve` uses: npx works wherever Node does.
        if let Some(Distribution::Npx { package, args }) = agent
            .distributions
            .iter()
            .find(|distribution| matches!(distribution, Distribution::Npx { .. }))
        {
            return self.install_npx(agent, package, args);
        }
        Err(InstallError::UnsupportedDistribution {
            agent: agent.id.clone(),
            kind: unsupported_kind_name(agent),
        })
    }

    fn install_npx(
        &self,
        agent: &RegistryAgent,
        package: &str,
        args: &[String],
    ) -> Result<InstalledAgent, InstallError> {
        let staging = staging_dir(self.store.root(), &agent.id, &agent.version);
        let _guard = InstallGuard::acquire(self.store.root(), &agent.id)?;
        let fail = |message: String| InstallError::Failed {
            agent: agent.id.clone(),
            message,
        };

        std::fs::create_dir_all(&staging).map_err(|error| fail(error.to_string()))?;

        // `npm_argv` always names a program; keep the empty-vec mistake
        // from becoming an index panic.
        debug_assert!(
            !self.npm_argv.is_empty(),
            "the npm argv must name a program"
        );
        let mut child = std::process::Command::new(&self.npm_argv[0])
            .args(&self.npm_argv[1..])
            .args(npm_install_argv(&staging.to_string_lossy(), package, false))
            .current_dir(&staging)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|error| {
                fail(format!(
                    "npm could not be run ({error}); the npx distribution needs Node on PATH"
                ))
            })?;
        // Drained on a thread so a chatty npm cannot fill the pipe and
        // stall while the deadline below is being polled.
        let stderr_drain = {
            let pipe = child.stderr.take();
            std::thread::spawn(move || pipe.map(drain_capped).unwrap_or_default())
        };
        if matches!(
            wait_up_to(&mut child, NPM_INSTALL_TIMEOUT),
            WaitOutcome::TimedOut
        ) {
            return Err(fail(format!(
                "npm install timed out after {} s; nothing was installed",
                NPM_INSTALL_TIMEOUT.as_secs()
            )));
        }
        let status = child.wait().map_err(|error| fail(error.to_string()))?;
        let stderr = stderr_drain.join().unwrap_or_default();
        if !status.success() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(fail(stderr));
        }

        let bin_dir = staging.join("node_modules/.bin");
        // A package whose install produced no `.bin` directory exposes no
        // executable — that is the "no bin" case below, not an fs failure.
        // Any other read error is real (permissions, ...) and keeps failing
        // loudly rather than blaming the package.
        let entries: Vec<String> = match std::fs::read_dir(&bin_dir) {
            Ok(entries) => entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(fail(error.to_string())),
        };
        let bin = resolve_bin_name(&entries, package)
            .ok_or_else(|| fail("the installed package exposes no executable".to_string()))?;

        let final_dir = self.store.root().join(&agent.id).join(&agent.version);
        if let Some(parent) = final_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|error| fail(error.to_string()))?;
        }
        let _ = std::fs::remove_dir_all(&final_dir);
        std::fs::rename(&staging, &final_dir).map_err(|error| fail(error.to_string()))?;

        let installed = InstalledAgent {
            id: agent.id.clone(),
            version: agent.version.clone(),
            executable: final_dir.join("node_modules/.bin").join(&bin),
            args: args.to_vec(),
            // npm publishes no hash this code can check against the
            // registry document. Recorded honestly.
            integrity: Integrity::None,
        };
        self.store
            .write(&installed)
            .map_err(|error| fail(error.to_string()))?;
        Ok(installed)
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
        let fail = |message: String| InstallError::Failed {
            agent: agent.id.clone(),
            message,
        };

        std::fs::create_dir_all(&staging).map_err(|error| fail(error.to_string()))?;

        let bytes = download(&artifact.archive).map_err(|error| fail(error.to_string()))?;
        let integrity = verify_sha256(&bytes, artifact.sha256.as_deref()).map_err(|()| {
            InstallError::ChecksumMismatch {
                agent: agent.id.clone(),
            }
        })?;

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
        self.store
            .write(&installed)
            .map_err(|error| fail(error.to_string()))?;
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
            .map_err(|_| InstallError::AlreadyRunning {
                agent: id.to_string(),
            })?;
        Ok(Self { path })
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Picks the launchable entry in `node_modules/.bin`.
///
/// A single entry wins. Otherwise the bin matching the package's own name
/// (scope and version stripped) wins — dependency bins land in the same
/// directory (`codex` beside `codex-acp` from `@openai/codex`) and must
/// never be preferred. Falls back to the longest entry contained in the
/// package name.
pub(crate) fn resolve_bin_name(entries: &[String], package: &str) -> Option<String> {
    let mut candidates: Vec<&String> = entries
        .iter()
        .filter(|entry| !entry.starts_with('.'))
        .collect();
    candidates.sort();
    if candidates.len() == 1 {
        return Some(candidates[0].clone());
    }
    let without_scope = package.rsplit('/').next().unwrap_or(package);
    let base = without_scope
        .split('@')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(without_scope);
    if let Some(exact) = candidates.iter().find(|entry| entry.as_str() == base) {
        return Some((*exact).clone());
    }
    candidates
        .iter()
        .filter(|entry| package.contains(entry.as_str()))
        .max_by_key(|entry| entry.len())
        .map(|entry| (*entry).clone())
}

/// The argv for one npm install. `--ignore-scripts` is the default: a
/// package's `preinstall`/`postinstall` runs maintainer code with the
/// user's privileges *before* they have chosen to launch that agent. The
/// `allow_scripts` path exists for the retry the UI offers after naming
/// which package asked for it.
pub(crate) fn npm_install_argv(prefix: &str, package: &str, allow_scripts: bool) -> Vec<String> {
    let mut argv = vec![
        "install".to_string(),
        "--prefix".to_string(),
        prefix.to_string(),
        "--no-audit".to_string(),
        "--no-fund".to_string(),
    ];
    if !allow_scripts {
        argv.push("--ignore-scripts".to_string());
    }
    argv.push(package.to_string());
    argv
}

fn npm_binary() -> &'static str {
    if cfg!(windows) { "npm.cmd" } else { "npm" }
}

/// Drains `reader` to the end — npm must never be left writing into a full
/// pipe — while retaining at most [`STDERR_HEAD_CAP`] leading and
/// [`STDERR_TAIL_CAP`] trailing bytes; everything between is counted and
/// elided behind a marker naming how much was dropped.
///
/// When anything was dropped, both cut points snap to line boundaries so
/// the marker never sits mid-line — npm diagnostics only read line-wise.
fn drain_capped<R: std::io::Read>(mut reader: R) -> String {
    let mut head: Vec<u8> = Vec::with_capacity(STDERR_HEAD_CAP);
    let mut tail: std::collections::VecDeque<u8> = std::collections::VecDeque::new();
    let mut total: usize = 0;
    let mut chunk = [0u8; 4096];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                for &byte in &chunk[..n] {
                    if total < STDERR_HEAD_CAP {
                        head.push(byte);
                    } else {
                        if tail.len() == STDERR_TAIL_CAP {
                            tail.pop_front();
                        }
                        tail.push_back(byte);
                    }
                    total += 1;
                }
            }
            // A broken or vanished pipe still yields whatever was read.
            Err(_) => break,
        }
    }
    if head.len() + tail.len() == total {
        // Nothing evicted, but the bytes may still be split across both
        // buffers (any total past HEAD_CAP) — rejoin them before returning.
        let mut bytes = std::mem::take(&mut head);
        bytes.extend(tail);
        return String::from_utf8_lossy(&bytes).into_owned();
    }
    if let Some(cut) = head.iter().rposition(|&byte| byte == b'\n') {
        head.truncate(cut + 1);
    }
    // Shave the tail's leading partial line only when a later complete
    // line survives to snap to. When the only newline is the final byte,
    // the "fragment" is the summary line itself and shaving would erase it.
    if let Some(cut) = tail
        .iter()
        .position(|&byte| byte == b'\n')
        .filter(|&cut| cut + 1 < tail.len())
    {
        tail.drain(..=cut);
    }
    let elided = total - head.len() - tail.len();
    // The caps cut on raw byte boundaries, which can split a multi-byte
    // UTF-8 character in half; lossy replacement keeps that from panicking.
    format!(
        "{}\n\n  [ {} bytes elided ]\n\n{}",
        String::from_utf8_lossy(&head),
        elided,
        String::from_utf8_lossy(tail.make_contiguous())
    )
}

enum WaitOutcome {
    Exited,
    TimedOut,
}

/// Waits for `child` up to `timeout`, polling. Kills it at the deadline:
/// an install must never hang forever — least of all while holding its
/// per-agent lock.
fn wait_up_to(child: &mut std::process::Child, timeout: Duration) -> WaitOutcome {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return WaitOutcome::Exited,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return WaitOutcome::TimedOut;
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            // The child is already gone one way or another; the caller's
            // own `wait()` surfaces whatever is knowable.
            Err(_) => return WaitOutcome::Exited,
        }
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
    unpack_zip_capped(agent, bytes, destination, MAX_UNPACKED_BYTES)
}

/// Copies through a reader limited to the remaining budget (`budget + 1`, so
/// crossing it is detectable) and counts what is *actually written* — the
/// sizes an archive declares can lie, and that lie defeats a declared-size
/// ceiling exactly where the ceiling matters.
fn unpack_zip_capped(
    agent: &str,
    bytes: &[u8],
    destination: &Path,
    max_unpacked_bytes: u64,
) -> Result<(), InstallError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|error| {
        InstallError::Failed {
            agent: agent.into(),
            message: error.to_string(),
        }
    })?;
    let mut written: u64 = 0;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| InstallError::Failed {
                agent: agent.into(),
                message: error.to_string(),
            })?;
        let Some(name) = entry.enclosed_name() else {
            return Err(InstallError::UnsafeArchiveEntry {
                agent: agent.into(),
                entry: entry.name().to_string(),
            });
        };
        let target =
            safe_entry_path(destination, &name).map_err(|()| InstallError::UnsafeArchiveEntry {
                agent: agent.into(),
                entry: name.display().to_string(),
            })?;
        if entry.is_dir() {
            let _ = std::fs::create_dir_all(&target);
            continue;
        }
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let budget = max_unpacked_bytes - written;
        let mut file = std::fs::File::create(&target).map_err(|error| InstallError::Failed {
            agent: agent.into(),
            message: error.to_string(),
        })?;
        let mut limited = entry.by_ref().take(budget + 1);
        let copied =
            std::io::copy(&mut limited, &mut file).map_err(|error| InstallError::Failed {
                agent: agent.into(),
                message: error.to_string(),
            })?;
        written += copied;
        if copied > budget {
            return Err(InstallError::Failed {
                agent: agent.into(),
                message: "unpacked size exceeds the ceiling".into(),
            });
        }
    }
    Ok(())
}

fn unpack_tar_gz(agent: &str, bytes: &[u8], destination: &Path) -> Result<(), InstallError> {
    unpack_tar_gz_capped(agent, bytes, destination, MAX_UNPACKED_BYTES)
}

/// Same actual-bytes accounting as [`unpack_zip_capped`] — for tar the
/// header size usually tells the truth, but one code path for both keeps
/// them behaving identically.
fn unpack_tar_gz_capped(
    agent: &str,
    bytes: &[u8],
    destination: &Path,
    max_unpacked_bytes: u64,
) -> Result<(), InstallError> {
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
        let target =
            safe_entry_path(destination, &path).map_err(|()| InstallError::UnsafeArchiveEntry {
                agent: agent.into(),
                entry: path.display().to_string(),
            })?;
        if kind.is_dir() {
            let _ = std::fs::create_dir_all(&target);
            continue;
        }
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let budget = max_unpacked_bytes - written;
        let mut file = std::fs::File::create(&target).map_err(|error| InstallError::Failed {
            agent: agent.into(),
            message: error.to_string(),
        })?;
        let mut limited = entry.by_ref().take(budget + 1);
        let copied =
            std::io::copy(&mut limited, &mut file).map_err(|error| InstallError::Failed {
                agent: agent.into(),
                message: error.to_string(),
            })?;
        written += copied;
        if copied > budget {
            return Err(InstallError::Failed {
                agent: agent.into(),
                message: "unpacked size exceeds the ceiling".into(),
            });
        }
    }
    Ok(())
}

/// Names the kinds being refused, so the error reads "uvx" rather than
/// something vague. A `Binary` kind is never named here: its absence *for
/// this platform* is a different failure upstream.
fn unsupported_kind_name(agent: &RegistryAgent) -> String {
    let mut names: Vec<&str> = Vec::new();
    for distribution in &agent.distributions {
        let name = match distribution {
            Distribution::Binary(_) => continue,
            Distribution::Npx { .. } => "npx",
            Distribution::Uvx { .. } => "uvx",
            Distribution::Unknown => "unknown",
        };
        if !names.contains(&name) {
            names.push(name);
        }
    }
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
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
        assert_eq!(
            unpack_kind("https://x/opencode-linux-x64.zip"),
            UnpackKind::Zip
        );
        assert_eq!(
            unpack_kind("https://x/agent-linux.tar.gz"),
            UnpackKind::TarGz
        );
        assert_eq!(unpack_kind("https://x/agent-linux.tgz"), UnpackKind::TarGz);
        assert_eq!(
            unpack_kind("https://x/goose-x86_64-unknown-linux-gnu.tar.bz2"),
            UnpackKind::Unsupported
        );
        assert_eq!(
            unpack_kind("https://x/sigit-linux-amd64"),
            UnpackKind::BareExecutable
        );
        assert_eq!(
            unpack_kind("https://x/sigit-win-amd64.exe"),
            UnpackKind::BareExecutable
        );
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
        assert_eq!(
            verify_sha256(bytes, Some(&digest)).unwrap(),
            Integrity::Sha256
        );
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
        assert_eq!(
            staging.parent().unwrap(),
            std::path::Path::new("/data/.staging")
        );
    }

    #[test]
    fn sweeping_removes_staging_left_by_a_dead_process() {
        let root = std::env::temp_dir().join(format!("sirio-sweep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dead = root.join(".staging/codex-acp-1.6.2-1");
        let live = staging_dir(&root, "codex-acp", "1.6.2");
        std::fs::create_dir_all(&dead).unwrap();
        std::fs::create_dir_all(&live).unwrap();

        Installer::new(InstallStore::new(root.clone()))
            .sweep_staging()
            .unwrap();

        assert!(!dead.exists(), "abandoned staging is collected");
        assert!(live.exists(), "this process's own staging is left alone");
    }

    #[test]
    fn sweeping_also_clears_a_lock_left_by_a_killed_install() {
        // Without this, one killed install makes that agent permanently
        // uninstallable: `InstallGuard::acquire` uses create_new, so the
        // orphaned lock rejects every later attempt with AlreadyRunning.
        let root = std::env::temp_dir().join(format!("sirio-sweep-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let lock = root.join(".staging/codex-acp.lock");
        std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
        std::fs::write(&lock, "").unwrap();

        Installer::new(InstallStore::new(root.clone()))
            .sweep_staging()
            .unwrap();

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
        assert!(
            rendered.contains("tar.bz2"),
            "the gap is visible, not mysterious"
        );
    }

    // ---- End-to-end safety checks: hostile archives built in memory, no
    // network, exercising the real unpackers rather than their helpers.

    /// Builds an in-memory tar.gz containing exactly `entries` regular
    /// files (path, content).
    fn tar_gz_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        for (path, content) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, path, *content).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn staging_for(name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("sirio-install-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn unpack_tar_gz_refuses_a_traversal_entry_and_writes_nothing_outside() {
        let staging = staging_for("tar-traversal");
        // `tar::Builder`'s own path setter refuses `..`, so a writer cannot
        // produce this archive — but a hostile one can exist, so the name
        // field is patched raw and appended without going through it.
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(3);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        header.as_gnu_mut().unwrap().name[..7].copy_from_slice(b"../evil");
        header.set_cksum();
        builder.append(&header, b"bad".as_slice()).unwrap();
        let bytes = builder.into_inner().unwrap().finish().unwrap();

        let result = unpack_tar_gz("evil", &bytes, &staging);

        assert!(matches!(
            result,
            Err(InstallError::UnsafeArchiveEntry { .. })
        ));
        let outside = staging.parent().unwrap().join("evil");
        assert!(!outside.exists(), "no file may land outside staging");
    }

    #[test]
    fn unpack_tar_gz_refuses_a_symlink_entry() {
        // A symlink pointing anywhere passes a pure path check; the entry
        // *type* filter must catch it first.
        let staging = staging_for("tar-symlink");
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_cksum();
        builder
            .append_link(&mut header, "link", "/etc/passwd")
            .unwrap();
        let bytes = builder.into_inner().unwrap().finish().unwrap();

        let result = unpack_tar_gz("evil", &bytes, &staging);

        assert!(matches!(
            result,
            Err(InstallError::UnsafeArchiveEntry { .. })
        ));
    }

    #[test]
    fn unpack_zip_refuses_a_traversal_entry_and_writes_nothing_outside() {
        let staging = staging_for("zip-traversal");
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .start_file("../evil", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut writer, b"bad").unwrap();
        let bytes = writer.finish().unwrap().into_inner();

        let result = unpack_zip("evil", &bytes, &staging);

        assert!(matches!(
            result,
            Err(InstallError::UnsafeArchiveEntry { .. })
        ));
        let outside = staging.parent().unwrap().join("evil");
        assert!(!outside.exists(), "no file may land outside staging");
    }

    #[test]
    fn a_well_formed_tar_gz_unpacks_where_it_should() {
        let staging = staging_for("tar-happy");
        let bytes = tar_gz_of(&[("bin/agent", b"payload" as &[u8])]);

        unpack_tar_gz("good", &bytes, &staging).unwrap();

        assert_eq!(
            std::fs::read(staging.join("bin/agent")).unwrap(),
            b"payload"
        );
    }

    #[test]
    fn a_well_formed_zip_unpacks_where_it_should() {
        let staging = staging_for("zip-happy");
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .start_file("bin/agent", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut writer, b"payload").unwrap();
        let bytes = writer.finish().unwrap().into_inner();

        unpack_zip("good", &bytes, &staging).unwrap();

        assert_eq!(
            std::fs::read(staging.join("bin/agent")).unwrap(),
            b"payload"
        );
    }

    #[test]
    fn an_unsupported_distribution_names_the_kind() {
        // The global rule: `.tar.bz2` AND `uvx` are refused with an error
        // that names the format.
        let store = InstallStore::new(staging_for("uvx-kind"));
        let agent = RegistryAgent {
            id: "goose".into(),
            name: "goose".into(),
            version: "1.0.0".into(),
            description: None,
            repository: None,
            website: None,
            license: None,
            icon: None,
            distributions: vec![Distribution::Uvx {
                package: "goose-acp".into(),
                args: Vec::new(),
            }],
        };

        let error = Installer::new(store)
            .install(&agent, "linux-x86_64")
            .unwrap_err();

        assert!(error.to_string().contains("uvx"), "got: {error}");
    }

    #[test]
    fn a_small_declared_payload_over_the_ceiling_is_refused_zip() {
        // The ceiling counts bytes actually written, not declared sizes:
        // a ZIP central directory can lie. Lowering the ceiling stands in
        // for generating hundreds of megabytes.
        let staging = staging_for("zip-bomb");
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .start_file("data.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut writer, &[0u8; 256]).unwrap();
        let bytes = writer.finish().unwrap().into_inner();

        let result = unpack_zip_capped("bomb", &bytes, &staging, 8);

        assert!(
            matches!(&result, Err(InstallError::Failed { message, .. })
                if message.contains("ceiling")),
            "got: {result:?}"
        );
    }

    #[test]
    fn a_small_declared_payload_over_the_ceiling_is_refused_tar_gz() {
        let staging = staging_for("tar-bomb");
        let bytes = tar_gz_of(&[("data.bin", &[0u8; 256])]);

        let result = unpack_tar_gz_capped("bomb", &bytes, &staging, 8);

        assert!(
            matches!(&result, Err(InstallError::Failed { message, .. })
                if message.contains("ceiling")),
            "got: {result:?}"
        );
    }

    #[test]
    fn a_single_bin_entry_wins() {
        let entries = vec!["claude-agent-acp".to_string()];
        assert_eq!(
            resolve_bin_name(&entries, "@agentclientprotocol/claude-agent-acp@0.70.0"),
            Some("claude-agent-acp".to_string())
        );
    }

    #[test]
    fn a_dependency_bin_is_never_preferred_over_the_package_bin() {
        // Installing codex-acp also drops a `codex` bin from its
        // @openai/codex dependency. Preferring it would silently launch the
        // wrong program — the Swift original's comment says "must never be
        // preferred".
        let entries = vec!["codex".to_string(), "codex-acp".to_string()];
        assert_eq!(
            resolve_bin_name(&entries, "@agentclientprotocol/codex-acp@1.6.2"),
            Some("codex-acp".to_string())
        );
    }

    #[test]
    fn the_longest_entry_contained_in_the_package_name_is_the_fallback() {
        let entries = vec!["ag".to_string(), "agent-cli".to_string()];
        assert_eq!(
            resolve_bin_name(&entries, "agent-cli-tools@1.0.0"),
            Some("agent-cli".to_string())
        );
    }

    #[test]
    fn no_bin_entries_yields_none_rather_than_a_guess() {
        assert_eq!(resolve_bin_name(&[], "whatever@1.0.0"), None);
    }

    #[test]
    fn dotfiles_are_not_candidates() {
        let entries = vec![".package-lock.json".to_string(), "real-bin".to_string()];
        assert_eq!(
            resolve_bin_name(&entries, "real-bin@1.0.0"),
            Some("real-bin".to_string())
        );
    }

    #[test]
    fn npm_runs_with_scripts_disabled_by_default() {
        let command = npm_install_argv("/tmp/staging", "codex-acp@1.6.2", false);
        assert!(
            command.contains(&"--ignore-scripts".to_string()),
            "installing must not run maintainer preinstall/postinstall by default"
        );
        let with_scripts = npm_install_argv("/tmp/staging", "codex-acp@1.6.2", true);
        assert!(!with_scripts.contains(&"--ignore-scripts".to_string()));
    }

    #[test]
    fn entries_unrelated_to_the_package_yield_none() {
        // Nothing matches the base name and nothing is contained in it:
        // guessing the alphabetically-first bin would be the codex-beside-
        // codex-acp trap all over again, minus the safety net. The honest
        // answer is None, and the caller turns that into an error.
        let entries = vec!["alpha".to_string(), "zeta".to_string()];
        assert_eq!(resolve_bin_name(&entries, "totally-unrelated@2.0.0"), None);
    }

    #[test]
    fn the_npm_install_deadline_is_coherent_with_the_other_ceilings() {
        // Same ceiling family as Task 5's download: nothing stays in
        // flight forever.
        assert_eq!(DOWNLOAD_TIMEOUT, Duration::from_secs(600));
        assert_eq!(NPM_INSTALL_TIMEOUT, Duration::from_secs(600));
    }

    #[test]
    fn wait_up_to_notices_a_child_that_finishes_in_time() {
        let mut child = quick_child(true).spawn().unwrap();
        assert!(matches!(
            wait_up_to(&mut child, Duration::from_secs(30)),
            WaitOutcome::Exited
        ));
    }

    #[test]
    fn wait_up_to_kills_a_child_that_blows_the_deadline() {
        let started = std::time::Instant::now();
        let mut child = quick_child(false).spawn().unwrap();

        assert!(matches!(
            wait_up_to(&mut child, Duration::from_millis(50)),
            WaitOutcome::TimedOut
        ));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the child was killed at the deadline, not waited out"
        );
        assert!(
            child.try_wait().unwrap().is_some(),
            "the killed child was reaped"
        );
    }

    // ---- Task 1: the capped stderr drain.

    #[test]
    fn output_under_the_cap_passes_through_untouched() {
        let text = "npm warn deprecated something\nnpm ERR! summary\n";
        assert_eq!(drain_capped(text.as_bytes()), text);
    }

    #[test]
    fn an_input_between_the_caps_keeps_both_its_first_and_last_line() {
        // 3,000 bytes sits in the gap that once silently truncated: past
        // STDERR_HEAD_CAP, yet too small to evict anything from the tail.
        let mut input = String::new();
        input.push_str("FIRST-LINE-MARKER\n");
        input.push_str(&"filler ".repeat(600 - "FIRST-LINE-MARKER\n".len() / 7));
        while input.len() < 3_000 {
            input.push_str("filler ");
        }
        input.push_str("LAST-LINE-MARKER\n");

        let output = drain_capped(input.as_bytes());

        assert!(output.contains("FIRST-LINE-MARKER"), "head lost: {output}");
        assert!(output.contains("LAST-LINE-MARKER"), "tail lost: {output}");
    }

    #[test]
    fn an_input_exactly_at_the_head_cap_passes_through_whole() {
        let input: Vec<u8> = (0..STDERR_HEAD_CAP)
            .map(|i| b'a' + (i % 26) as u8)
            .collect();
        assert_eq!(drain_capped(&input[..]), String::from_utf8(input).unwrap());
    }

    #[test]
    fn an_input_exactly_at_both_caps_passes_through_whole() {
        let input: Vec<u8> = (0..STDERR_HEAD_CAP + STDERR_TAIL_CAP)
            .map(|i| b'a' + (i % 26) as u8)
            .collect();
        assert_eq!(drain_capped(&input[..]), String::from_utf8(input).unwrap());
    }

    #[test]
    fn output_over_the_cap_keeps_a_recognisable_head_and_a_recognisable_tail() {
        let mut input = String::new();
        input.push_str("HEAD-MARKER: could not resolve dependency\n");
        input.push_str(&"progress noise ".repeat(400));
        input.push('\n');
        input.push_str("TAIL-MARKER: npm ERR! summary\n");

        let output = drain_capped(input.as_bytes());

        assert!(
            output.starts_with("HEAD-MARKER: could not resolve dependency\n"),
            "the head is gone: {output}"
        );
        assert!(
            output.ends_with("TAIL-MARKER: npm ERR! summary\n"),
            "the tail is gone: {output}"
        );
        assert!(!output.contains("progress noise"), "the middle must go");
        assert!(output.contains("bytes elided"), "got: {output}");
    }

    #[test]
    fn the_elision_marker_reports_a_plausible_byte_count() {
        let total = 10_000usize;
        let output = drain_capped(std::io::Cursor::new(vec![b'x'; total]));
        let start = output.find("[ ").unwrap();
        let digits = &output[start + 2..];
        let end = digits.find(" ").unwrap();
        let reported: usize = digits[..end].parse().unwrap();

        assert!(
            reported >= total - STDERR_HEAD_CAP - STDERR_TAIL_CAP,
            "reported {reported}, ceiling allows as few as {}",
            total - STDERR_HEAD_CAP - STDERR_TAIL_CAP
        );
        assert!(reported < total, "reported {reported} of {total}");
        assert_eq!(
            reported,
            total - output.len() + format!("\n\n  [ {reported} bytes elided ]\n\n").len(),
            "head + marker + tail must account for every byte"
        );
    }

    #[test]
    fn invalid_utf8_does_not_panic_and_still_drains() {
        let mut input = b"before \xff\xfe after\nlater line\n".to_vec();
        input.extend([0xff, 0xfe]);

        let output = drain_capped(&input[..]);

        assert!(output.contains("later line"), "got: {output:?}");
    }

    // ---- Task 2: the npx install path with an injectable npm.

    fn npx_agent(id: &str) -> RegistryAgent {
        RegistryAgent {
            id: id.into(),
            name: id.into(),
            version: "1.0.0".into(),
            description: None,
            repository: None,
            website: None,
            license: None,
            icon: None,
            distributions: vec![Distribution::Npx {
                package: format!("{id}-pkg"),
                args: Vec::new(),
            }],
        }
    }

    /// A stand-in npm spelled for this host, mirroring `quick_child`.
    /// Trailing tokens (`npm_install_argv`'s arguments) are harmless:
    /// `cmd`'s and `sh`'s exit builtin ignores them.
    fn fake_npm_argv(script: &str) -> Vec<String> {
        #[cfg(windows)]
        {
            vec!["cmd".into(), "/C".into(), script.to_string()]
        }
        #[cfg(not(windows))]
        {
            vec!["sh".into(), "-c".into(), script.to_string()]
        }
    }

    #[test]
    fn a_failing_fake_npm_fails_the_install_and_carries_its_stderr() {
        let store = InstallStore::new(staging_for("npx-fail"));
        let agent = npx_agent("npx-fail-agent");
        // Verified on this host: the message lands on stderr, the trailing
        // appended argv does not disturb the exit code.
        #[cfg(windows)]
        let script = "echo simulated-npm-boom 1>&2 & exit 1";
        #[cfg(not(windows))]
        let script = "echo simulated-npm-boom 1>&2; exit 1";

        let error = Installer::new(store)
            .with_npm(fake_npm_argv(script))
            .install(&agent, "linux-x86_64")
            .unwrap_err();

        assert!(
            error.to_string().contains("simulated-npm-boom"),
            "stderr was lost: {error}"
        );
    }

    #[test]
    fn a_fake_npm_that_leaves_no_bin_says_the_package_exposes_no_executable() {
        let store = InstallStore::new(staging_for("npx-nobin"));
        let agent = npx_agent("npx-nobin-agent");

        let error = Installer::new(store)
            .with_npm(fake_npm_argv("exit 0"))
            .install(&agent, "linux-x86_64")
            .unwrap_err();

        assert!(error.to_string().contains("no executable"), "got: {error}");
    }

    /// A command that exits immediately (`succeed`) or hangs for a long
    /// time, spelled for this host.
    fn quick_child(succeed: bool) -> std::process::Command {
        #[cfg(windows)]
        {
            let mut command = std::process::Command::new("cmd");
            command.args([
                "/C",
                if succeed {
                    "exit 0"
                } else {
                    "ping -n 30 127.0.0.1 > nul"
                },
            ]);
            command
        }
        #[cfg(not(windows))]
        {
            let mut command = std::process::Command::new("sh");
            command.args(["-c", if succeed { "exit 0" } else { "sleep 30" }]);
            command
        }
    }
}
