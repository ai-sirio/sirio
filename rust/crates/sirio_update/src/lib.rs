//! Discovery, download and payload verification for Sirio updates.
//!
//! This crate deliberately stops at [`VerifiedUpdate`]. Platform crates own
//! applying that file, so a downloaded artifact can never be mistaken for an
//! installed update here.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

pub use sirio_control::ReleaseChannel;
use sirio_release::{ChannelManifest, ManifestArtifact};

const MANIFEST_HOST: &str = "https://dl.sirioai.app";

pub const STABLE_POLL_INTERVAL: Duration = Duration::from_secs(86_400);
pub const NIGHTLY_POLL_INTERVAL: Duration = Duration::from_secs(3_600);
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Build the production manifest URL for a compiled release channel.
pub fn manifest_url_for(channel: ReleaseChannel) -> String {
    format!("{MANIFEST_HOST}/{}.json", channel.as_str())
}

/// The manifest URL used by this binary. A URL override is compiled only
/// into debug builds, so a release Stable or Nightly binary has no test
/// endpoint to use. The channel gate remains in [`Updater::check_at`].
pub fn manifest_url() -> String {
    #[cfg(debug_assertions)]
    if let Some(url) = option_env!("SIRIO_UPDATE_MANIFEST_URL").filter(|url| !url.is_empty()) {
        return url.to_string();
    }

    manifest_url_for(ReleaseChannel::RELEASE_CHANNEL)
}

/// The minimum interval between manifest checks for a real release channel.
pub const fn poll_interval(channel: ReleaseChannel) -> Option<Duration> {
    match channel {
        ReleaseChannel::Stable => Some(STABLE_POLL_INTERVAL),
        ReleaseChannel::Nightly => Some(NIGHTLY_POLL_INTERVAL),
        ReleaseChannel::Dev => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvailableUpdate {
    pub version: String,
    pub notes: String,
    pub artifact: ManifestArtifact,
}

/// A downloaded artifact whose hash and Ed25519 signature both passed.
///
/// This is the hand-off contract for the platform-specific apply tickets:
/// `path` is staged and ready to apply, but this crate never executes,
/// installs or replaces it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedUpdate {
    pub version: String,
    pub notes: String,
    pub path: PathBuf,
    pub platform: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadResult {
    /// Stable auto-download was skipped because the platform reports a
    /// metered connection.
    SkippedMetered,
    /// This version already had a download attempt during this interval.
    SkippedAlreadyAttempted,
    Ready(VerifiedUpdate),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckResult {
    Disabled,
    NotDue,
    UpToDate,
    Available(AvailableUpdate),
    Ready(VerifiedUpdate),
}

type Fetch = Box<dyn Fn(&str) -> Result<Vec<u8>, String> + Send + Sync>;

pub struct Updater {
    channel: ReleaseChannel,
    manifest_url: String,
    staging_dir: PathBuf,
    accepted_keys: sirio_release::AcceptedKeys,
    fetch: Fetch,
    last_check: Option<SystemTime>,
    last_download_attempt: Option<(String, SystemTime)>,
}

impl Updater {
    /// Construct the production updater. The compiled release channel gates
    /// all network work; a dev build returns [`CheckResult::Disabled`].
    pub fn new(
        staging_dir: impl Into<PathBuf>,
        accepted_keys: sirio_release::AcceptedKeys,
    ) -> Self {
        Self {
            channel: ReleaseChannel::RELEASE_CHANNEL,
            manifest_url: manifest_url(),
            staging_dir: staging_dir.into(),
            accepted_keys,
            fetch: Box::new(fetch_http),
            last_check: None,
            last_download_attempt: None,
        }
    }

    /// Check immediately when due. The first call is always due, which gives
    /// both Stable and Nightly their launch check.
    pub fn check(&mut self, metered: bool) -> Result<CheckResult, UpdateError> {
        self.check_at(SystemTime::now(), metered)
    }

    /// Testable form of [`Self::check`] with an explicit clock.
    pub fn check_at(&mut self, now: SystemTime, metered: bool) -> Result<CheckResult, UpdateError> {
        if !self.channel.updates_enabled() {
            return Ok(CheckResult::Disabled);
        }

        let interval = poll_interval(self.channel).ok_or(UpdateError::Disabled)?;
        if let Some(last_check) = self.last_check
            && now
                .duration_since(last_check)
                .map(|age| age < interval)
                .unwrap_or(false)
        {
            return Ok(CheckResult::NotDue);
        }
        // Record the attempt before touching the network. A failed check is
        // still a check and must not turn into a tight retry loop.
        self.last_check = Some(now);

        let bytes = (self.fetch)(&self.manifest_url).map_err(UpdateError::Network)?;
        let json =
            String::from_utf8(bytes).map_err(|error| UpdateError::Manifest(error.to_string()))?;
        let manifest = ChannelManifest::parse(&json)
            .map_err(|error| UpdateError::Manifest(error.to_string()))?;
        if manifest.channel != self.channel.as_str() {
            return Err(UpdateError::ChannelMismatch {
                expected: self.channel.as_str().to_string(),
                actual: manifest.channel,
            });
        }

        let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))
            .map_err(|error| UpdateError::Version(error.to_string()))?;
        let version = semver::Version::parse(&manifest.version)
            .map_err(|error| UpdateError::Version(error.to_string()))?;
        if version <= current {
            return Ok(CheckResult::UpToDate);
        }

        let platform = sirio_registry::current_platform_key();
        let artifact =
            manifest
                .artifacts
                .get(platform)
                .cloned()
                .ok_or_else(|| UpdateError::NoArtifact {
                    platform: platform.to_string(),
                })?;
        let available = AvailableUpdate {
            version: manifest.version,
            notes: manifest.notes,
            artifact,
        };

        if self.channel == ReleaseChannel::Stable {
            match self.download_at(&available, now, metered)? {
                DownloadResult::Ready(update) => return Ok(CheckResult::Ready(update)),
                DownloadResult::SkippedMetered | DownloadResult::SkippedAlreadyAttempted => {}
            }
        }
        Ok(CheckResult::Available(available))
    }

    /// Download and verify an available update. Nightly callers use this
    /// explicit operation; Stable calls it automatically from [`Self::check_at`].
    pub fn download(
        &mut self,
        update: &AvailableUpdate,
        metered: bool,
    ) -> Result<DownloadResult, UpdateError> {
        self.download_at(update, SystemTime::now(), metered)
    }

    /// Testable form of [`Self::download`] with an explicit clock.
    pub fn download_at(
        &mut self,
        update: &AvailableUpdate,
        now: SystemTime,
        metered: bool,
    ) -> Result<DownloadResult, UpdateError> {
        if !self.channel.updates_enabled() {
            return Ok(DownloadResult::SkippedAlreadyAttempted);
        }
        if metered {
            return Ok(DownloadResult::SkippedMetered);
        }

        let interval = poll_interval(self.channel).ok_or(UpdateError::Disabled)?;
        if let Some((version, attempted_at)) = &self.last_download_attempt
            && version == &update.version
            && now
                .duration_since(*attempted_at)
                .map(|age| age < interval)
                .unwrap_or(false)
        {
            return Ok(DownloadResult::SkippedAlreadyAttempted);
        }
        self.last_download_attempt = Some((update.version.clone(), now));

        let version = semver::Version::parse(&update.version)
            .map_err(|error| UpdateError::Version(error.to_string()))?;
        let platform = sirio_registry::current_platform_key().to_string();
        let target = self
            .staging_dir
            .join(format!("sirio-update-{version}-{platform}"));
        let bytes = (self.fetch)(&update.artifact.url).map_err(UpdateError::Network)?;
        let temporary = write_temporary(&target, &bytes).map_err(UpdateError::Staging)?;

        if let Err(error) = self.accepted_keys.verify(&update.artifact, &bytes) {
            let _ = std::fs::remove_file(&temporary);
            return Err(UpdateError::Verification(error.to_string()));
        }
        std::fs::rename(&temporary, &target).map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            UpdateError::Staging(error.to_string())
        })?;

        Ok(DownloadResult::Ready(VerifiedUpdate {
            version: version.to_string(),
            notes: update.notes.clone(),
            path: target,
            platform,
        }))
    }

    pub fn last_checked_at(&self) -> Option<SystemTime> {
        self.last_check
    }
}

fn fetch_http(url: &str) -> Result<Vec<u8>, String> {
    let response = ureq::get(url)
        .config()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .call()
        .map_err(|error| error.to_string())?;
    let mut reader = response.into_body().into_reader();
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

fn write_temporary(target: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let ticket = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let temporary = target.with_file_name(format!(
        ".{}-{}-{ticket}.part",
        target.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        Ok::<_, String>(())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(temporary)
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("updates are disabled for this build")]
    Disabled,
    #[error("update request failed: {0}")]
    Network(String),
    #[error("manifest is invalid: {0}")]
    Manifest(String),
    #[error("manifest version is invalid: {0}")]
    Version(String),
    #[error("manifest channel is {actual:?}, expected {expected:?}")]
    ChannelMismatch { expected: String, actual: String },
    #[error("manifest has no artifact for platform {platform:?}")]
    NoArtifact { platform: String },
    #[error("could not stage update: {0}")]
    Staging(String),
    #[error("downloaded update failed verification: {0}")]
    Verification(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use ed25519_dalek::{Signer as _, SigningKey};
    use sha2::{Digest, Sha256};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sirio-update-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn empty_keys() -> sirio_release::AcceptedKeys {
        sirio_release::AcceptedKeys::from_base64(std::iter::empty::<&str>()).unwrap()
    }

    /// The version this crate is compiled at — the one `check_at` compares
    /// every manifest against.
    fn current_version() -> semver::Version {
        semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is semver")
    }

    /// A version strictly newer than the compiled one, derived rather than
    /// spelled out. Every commit in this repo bumps the workspace version
    /// (see CLAUDE.md), so a literal — the `0.7.0` these tests used to carry
    /// — stops being newer the day the product reaches it, and then five
    /// tests fail for a reason that has nothing to do with the updater.
    /// That is exactly what happened when the workspace passed 0.7.0.
    fn newer_version() -> String {
        let current = current_version();
        format!("{}.{}.0", current.major, current.minor + 1)
    }

    fn manifest(channel: &str, version: &str) -> Vec<u8> {
        let artifact = sirio_release::ManifestArtifact {
            url: "https://example.invalid/artifact".into(),
            sha256: "00".repeat(32),
            signature: format!("{}==", "A".repeat(86)),
        };
        serde_json::to_vec(&sirio_release::ChannelManifest {
            schema: sirio_release::MANIFEST_SCHEMA,
            channel: channel.into(),
            version: version.into(),
            notes: "notes".into(),
            artifacts: [(sirio_registry::current_platform_key().into(), artifact)]
                .into_iter()
                .collect(),
        })
        .unwrap()
    }

    fn signed_manifest(
        channel: &str,
        signer: &SigningKey,
        version: &str,
        url: &str,
        bytes: &[u8],
    ) -> Vec<u8> {
        let artifact = sirio_release::ManifestArtifact {
            url: url.into(),
            sha256: hex::encode(Sha256::digest(bytes)),
            signature: STANDARD.encode(signer.sign(bytes).to_bytes()),
        };
        serde_json::to_vec(&sirio_release::ChannelManifest {
            schema: sirio_release::MANIFEST_SCHEMA,
            channel: channel.into(),
            version: version.into(),
            notes: "notes".into(),
            artifacts: [(sirio_registry::current_platform_key().into(), artifact)]
                .into_iter()
                .collect(),
        })
        .unwrap()
    }

    #[test]
    fn manifest_url_uses_the_compiled_channel() {
        assert_eq!(
            manifest_url_for(ReleaseChannel::Stable),
            "https://dl.sirioai.app/stable.json"
        );
        assert_eq!(
            manifest_url_for(ReleaseChannel::Nightly),
            "https://dl.sirioai.app/nightly.json"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_override_is_selected_from_the_compiled_environment() {
        let expected = option_env!("SIRIO_UPDATE_MANIFEST_URL")
            .filter(|url| !url.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| manifest_url_for(ReleaseChannel::RELEASE_CHANNEL));
        assert_eq!(manifest_url(), expected);
    }

    #[test]
    fn a_newer_manifest_is_reported_as_available() {
        let dir = temp_dir("available");
        let offered = newer_version();
        let served = offered.clone();
        let mut updater =
            Updater::for_test(ReleaseChannel::Nightly, dir, empty_keys(), move |url| {
                assert_eq!(url, "https://dl.sirioai.app/nightly.json");
                Ok(manifest("nightly", &served))
            });

        let result = updater.check_at(SystemTime::UNIX_EPOCH, false).unwrap();
        assert!(matches!(
            result,
            CheckResult::Available(AvailableUpdate { ref version, .. }) if *version == offered
        ));
    }

    #[test]
    fn an_equal_or_older_manifest_is_not_available() {
        // The compiled version itself, which is the `<=` boundary this test
        // is named for, and the floor of the version line. Both derived or
        // permanent: a literal near the current version silently stops
        // testing the boundary the moment the product moves past it.
        for version in [current_version().to_string(), "0.0.1".to_string()] {
            let dir = temp_dir("old");
            let mut updater =
                Updater::for_test(ReleaseChannel::Nightly, dir, empty_keys(), move |_| {
                    Ok(manifest("nightly", &version))
                });
            assert_eq!(
                updater.check_at(SystemTime::UNIX_EPOCH, false).unwrap(),
                CheckResult::UpToDate
            );
        }
    }

    #[test]
    fn a_signed_stable_artifact_is_ready_only_after_verification() {
        let signer = SigningKey::from_bytes(&[7; 32]);
        let bytes = b"signed release".to_vec();
        let artifact_url = "https://example.invalid/artifact";
        let offered = newer_version();
        let manifest = signed_manifest("stable", &signer, &offered, artifact_url, &bytes);
        let keys = sirio_release::AcceptedKeys::from_base64([
            STANDARD.encode(signer.verifying_key().as_bytes())
        ])
        .unwrap();
        let dir = temp_dir("signed");
        let mut updater =
            Updater::for_test(ReleaseChannel::Stable, dir.clone(), keys, move |url| {
                Ok(if url.ends_with("stable.json") {
                    manifest.clone()
                } else {
                    bytes.clone()
                })
            });

        let result = updater.check_at(SystemTime::UNIX_EPOCH, false).unwrap();
        let CheckResult::Ready(ready) = result else {
            panic!("expected a verified update, got {result:?}");
        };
        assert_eq!(ready.version, offered);
        assert_eq!(std::fs::read(&ready.path).unwrap(), b"signed release");
        assert_eq!(ready.path.parent(), Some(dir.as_path()));
        assert!(std::fs::read_dir(dir).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".part")
        }));
    }

    #[test]
    fn failed_verification_removes_the_staged_download_and_returns_an_error() {
        let signer = SigningKey::from_bytes(&[8; 32]);
        let expected = b"signed release".to_vec();
        let manifest = signed_manifest(
            "stable",
            &signer,
            &newer_version(),
            "https://example.invalid/artifact",
            &expected,
        );
        let keys = sirio_release::AcceptedKeys::from_base64([
            STANDARD.encode(signer.verifying_key().as_bytes())
        ])
        .unwrap();
        let dir = temp_dir("rejected");
        let mut updater =
            Updater::for_test(ReleaseChannel::Stable, dir.clone(), keys, move |url| {
                Ok(if url.ends_with("stable.json") {
                    manifest.clone()
                } else {
                    b"tampered release".to_vec()
                })
            });

        let error = updater.check_at(SystemTime::UNIX_EPOCH, false).unwrap_err();
        assert!(
            matches!(error, UpdateError::Verification(_)),
            "got {error:?}"
        );
        assert!(std::fs::read_dir(dir).unwrap().next().is_none());
    }

    #[test]
    fn stable_does_not_download_on_a_metered_connection() {
        let artifact_fetches = Arc::new(AtomicUsize::new(0));
        let observed = artifact_fetches.clone();
        let dir = temp_dir("metered");
        let offered = newer_version();
        let mut updater =
            Updater::for_test(ReleaseChannel::Stable, dir, empty_keys(), move |url| {
                if url.ends_with("stable.json") {
                    Ok(manifest("stable", &offered))
                } else {
                    observed.fetch_add(1, Ordering::Relaxed);
                    Ok(Vec::new())
                }
            });

        assert!(matches!(
            updater.check_at(SystemTime::UNIX_EPOCH, true).unwrap(),
            CheckResult::Available(_)
        ));
        assert_eq!(artifact_fetches.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_version_is_downloaded_at_most_once_per_interval() {
        let signer = SigningKey::from_bytes(&[9; 32]);
        let bytes = b"signed release".to_vec();
        let manifest = signed_manifest(
            "nightly",
            &signer,
            &newer_version(),
            "https://example.invalid/artifact",
            &bytes,
        );
        let keys = sirio_release::AcceptedKeys::from_base64([
            STANDARD.encode(signer.verifying_key().as_bytes())
        ])
        .unwrap();
        let fetches = Arc::new(AtomicUsize::new(0));
        let observed = fetches.clone();
        let dir = temp_dir("once");
        let mut updater = Updater::for_test(ReleaseChannel::Nightly, dir, keys, move |url| {
            if url.ends_with("nightly.json") {
                Ok(manifest.clone())
            } else {
                observed.fetch_add(1, Ordering::Relaxed);
                Ok(bytes.clone())
            }
        });
        let CheckResult::Available(update) =
            updater.check_at(SystemTime::UNIX_EPOCH, false).unwrap()
        else {
            panic!("expected an available update");
        };

        assert!(matches!(
            updater
                .download_at(&update, SystemTime::UNIX_EPOCH, false)
                .unwrap(),
            DownloadResult::Ready(_)
        ));
        assert_eq!(
            updater
                .download_at(&update, SystemTime::UNIX_EPOCH, false)
                .unwrap(),
            DownloadResult::SkippedAlreadyAttempted
        );
        assert_eq!(fetches.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn stable_and_nightly_checks_are_due_at_launch_then_use_their_floors() {
        let mut stable = Updater::for_test(
            ReleaseChannel::Stable,
            temp_dir("stable-floor"),
            empty_keys(),
            |_| Ok(manifest("stable", "0.6.0")),
        );
        assert_eq!(
            stable.check_at(SystemTime::UNIX_EPOCH, false).unwrap(),
            CheckResult::UpToDate
        );
        assert_eq!(
            stable
                .check_at(
                    SystemTime::UNIX_EPOCH + STABLE_POLL_INTERVAL - Duration::from_secs(1),
                    false
                )
                .unwrap(),
            CheckResult::NotDue
        );

        let mut nightly = Updater::for_test(
            ReleaseChannel::Nightly,
            temp_dir("nightly-floor"),
            empty_keys(),
            |_| Ok(manifest("nightly", "0.6.0")),
        );
        assert_eq!(
            nightly.check_at(SystemTime::UNIX_EPOCH, false).unwrap(),
            CheckResult::UpToDate
        );
        assert_eq!(
            nightly
                .check_at(
                    SystemTime::UNIX_EPOCH + NIGHTLY_POLL_INTERVAL - Duration::from_secs(1),
                    false
                )
                .unwrap(),
            CheckResult::NotDue
        );
    }

    #[test]
    fn a_dev_build_never_touches_the_network() {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let mut updater = Updater::for_test(
            ReleaseChannel::Dev,
            temp_dir("dev"),
            empty_keys(),
            move |_| {
                observed.fetch_add(1, Ordering::Relaxed);
                Ok(Vec::new())
            },
        );

        assert!(matches!(
            updater.check_at(SystemTime::UNIX_EPOCH, false),
            Ok(CheckResult::Disabled)
        ));
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    impl Updater {
        fn for_test(
            channel: ReleaseChannel,
            staging_dir: impl Into<PathBuf>,
            accepted_keys: sirio_release::AcceptedKeys,
            fetch: impl Fn(&str) -> Result<Vec<u8>, String> + Send + Sync + 'static,
        ) -> Self {
            Self {
                channel,
                manifest_url: manifest_url_for(channel),
                staging_dir: staging_dir.into(),
                accepted_keys,
                fetch: Box::new(fetch),
                last_check: None,
                last_download_attempt: None,
            }
        }
    }
}
