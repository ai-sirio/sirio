//! Release-artifact signing and channel-manifest verification (ticket #306).
//!
//! Trust model: release artifacts are Ed25519-signed with a key whose private
//! half never leaves the signing environment. The app carries a *set* of
//! accepted public keys and accepts a signature from any member, so a key can
//! be rotated by shipping the new key one release before it starts signing.
//! The manifest hash catches corruption; the signature catches substitution —
//! a hash alone proves nothing, because whoever can serve a bad artifact can
//! serve a matching hash. `docs/release-signing.md` documents the format and
//! the key procedures; #311 verifies against that contract.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signature, VerifyingKey, SIGNATURE_LENGTH};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Only schema this build understands. `parse` refuses anything else, so the
/// format can evolve (#311 and later) without old binaries guessing.
pub const MANIFEST_SCHEMA: u32 = 1;

/// One published artifact inside a [`ChannelManifest`], keyed by platform
/// (`darwin-aarch64`, `linux-x86_64`, `windows-x86_64`, … — same spelling as
/// `sirio_registry::current_platform_key`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManifestArtifact {
    pub url: String,
    /// Lowercase hex SHA-256 of the exact artifact bytes.
    pub sha256: String,
    /// Standard base64 of the 64-byte Ed25519 signature over the exact
    /// artifact bytes. Must be present: an artifact without a signature is
    /// rejected, never "accepted unverified".
    pub signature: String,
}

/// The per-channel manifest (`https://dl.sirioai.app/stable.json`), §2.2 of
/// the auto-update design. This structure *is* the wire format: the signing
/// tool serializes it, the app deserializes it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChannelManifest {
    pub schema: u32,
    pub channel: String,
    pub version: String,
    /// Release notes, shipped inside the manifest so they cost zero
    /// additional requests and need no public repo to link to.
    pub notes: String,
    /// One entry per platform this release ships.
    pub artifacts: std::collections::BTreeMap<String, ManifestArtifact>,
}

impl ChannelManifest {
    /// Parse a manifest from its JSON wire form, rejecting anything this
    /// build cannot fully check: wrong schema, malformed hashes, malformed
    /// or missing signatures.
    pub fn parse(json: &str) -> Result<Self, Error> {
        let manifest: Self = serde_json::from_str(json)?;
        if manifest.schema != MANIFEST_SCHEMA {
            return Err(Error::UnsupportedSchema(manifest.schema));
        }
        for (platform, artifact) in &manifest.artifacts {
            let hash = hex::decode(&artifact.sha256)
                .map_err(|_| Error::BadHash(platform.clone()))?;
            if hash.len() != 32 {
                return Err(Error::BadHash(platform.clone()));
            }
            let sig = BASE64
                .decode(&artifact.signature)
                .map_err(|_| Error::BadSignatureEncoding(platform.clone()))?;
            if sig.len() != SIGNATURE_LENGTH {
                return Err(Error::BadSignatureEncoding(platform.clone()));
            }
        }
        Ok(manifest)
    }
}

/// The public keys compiled into a build. A signature from *any* member is
/// accepted; that is what makes rotation possible (add the new key one
/// release before the new key starts signing — see `docs/release-signing.md`).
#[derive(Clone, Debug)]
pub struct AcceptedKeys {
    keys: Vec<VerifyingKey>,
}

impl AcceptedKeys {
    /// Build the accepted set from standard-base64 public keys, as emitted by
    /// `sirio-release keygen` into `sirio-release-signing.pub`.
    pub fn from_base64<I, S>(keys: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut parsed = Vec::new();
        for key in keys {
            let raw = BASE64
                .decode(key.as_ref())
                .map_err(|_| Error::BadPublicKey)?;
            let bytes: [u8; 32] =
                raw.try_into().map_err(|_| Error::BadPublicKey)?;
            let vk = VerifyingKey::from_bytes(&bytes).map_err(|_| Error::BadPublicKey)?;
            parsed.push(vk);
        }
        Ok(Self { keys: parsed })
    }

    /// Check one artifact against its manifest entry: recompute the hash from
    /// the actual bytes, then require a signature that verifies under some
    /// accepted key. A tampered artifact, a wrong signature, a key outside
    /// the set and a missing signature all fail here.
    pub fn verify(&self, artifact: &ManifestArtifact, bytes: &[u8]) -> Result<(), Error> {
        let actual = Sha256::digest(bytes);
        let expected = hex::decode(&artifact.sha256)
            .map_err(|_| Error::BadHash(String::new()))?;
        if expected != actual.as_slice() {
            return Err(Error::HashMismatch {
                expected: artifact.sha256.clone(),
                actual: hex::encode(actual),
            });
        }
        let signature: [u8; SIGNATURE_LENGTH] = BASE64
            .decode(&artifact.signature)
            .map_err(|_| Error::BadSignatureEncoding(String::new()))?
            .try_into()
            .map_err(|_| Error::BadSignatureEncoding(String::new()))?;
        let signature = Signature::from_bytes(&signature);
        if self
            .keys
            .iter()
            .any(|key| key.verify_strict(bytes, &signature).is_ok())
        {
            Ok(())
        } else {
            Err(Error::NoAcceptedKey)
        }
    }
}

/// Render one artifact's manifest URL from the release job's template.
///
/// Placeholders: `{channel}`, `{version}`, `{platform}` and `{file}` — the
/// last is the artifact's own file name, needed because GitHub Releases
/// keep the versioned per-platform names the packaging scripts produce
/// (`Sirio-<v>.dmg`, `Sirio-<v>-x86_64.AppImage`, `SirioSetup-<v>.exe`),
/// which no `{platform}` substitution can reconstruct. The URL is re-read on
/// every check and is not trusted by itself (spec §2.2), so the template is a
/// publishing convenience, not a security boundary.
pub fn artifact_url(
    template: &str,
    channel: &str,
    version: &str,
    platform: &str,
    file: &str,
) -> String {
    template
        .replace("{channel}", channel)
        .replace("{version}", version)
        .replace("{platform}", platform)
        .replace("{file}", file)
}

/// Errors from parsing a manifest or verifying an artifact against one.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("manifest is not valid JSON or misses a required field: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported manifest schema {0}, this build understands {MANIFEST_SCHEMA} only")]
    UnsupportedSchema(u32),
    #[error("artifact {0:?} has a malformed sha256 (expected 64 hex chars)")]
    BadHash(String),
    #[error("artifact {0:?} has a malformed or missing signature")]
    BadSignatureEncoding(String),
    #[error("artifact hash mismatch: manifest says {expected}, downloaded bytes hash to {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("signature does not verify against any accepted public key")]
    NoAcceptedKey,
    #[error("public key is not standard base64 of 32 bytes")]
    BadPublicKey,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer as _, SigningKey};
    use rand_core::OsRng;

    const FIXTURE: &[u8] = b"sirio release artifact bytes";

    /// The release job hosts artifacts in GitHub Releases, whose filenames
    /// differ in shape per platform (`Sirio-<v>.dmg`, `SirioSetup-<v>.exe`),
    /// so the template must be able to name the file itself, not just the
    /// platform key.
    #[test]
    fn artifact_url_substitutes_every_placeholder_including_the_file_name() {
        let url = artifact_url(
            "https://github.com/ai-sirio/sirio/releases/download/v{version}/{file}?c={channel}&p={platform}",
            "stable",
            "0.6.0",
            "darwin-aarch64",
            "Sirio-0.6.0.dmg",
        );
        assert_eq!(
            url,
            "https://github.com/ai-sirio/sirio/releases/download/v0.6.0/Sirio-0.6.0.dmg?c=stable&p=darwin-aarch64"
        );
    }

    #[test]
    fn artifact_url_leaves_a_template_without_placeholders_alone() {
        assert_eq!(
            artifact_url(
                "https://dl.sirioai.app/x",
                "nightly",
                "1",
                "linux-x86_64",
                "f"
            ),
            "https://dl.sirioai.app/x"
        );
    }

    fn key() -> SigningKey {
        SigningKey::generate(&mut OsRng)
    }

    fn b64(bytes: &[u8]) -> String {
        BASE64.encode(bytes)
    }

    fn entry(signer: &SigningKey, bytes: &[u8]) -> ManifestArtifact {
        ManifestArtifact {
            url: "https://dl.sirioai.app/stable/0.6.1/Sirio-0.6.1-darwin-aarch64.dmg".into(),
            sha256: hex::encode(Sha256::digest(bytes)),
            signature: b64(&signer.sign(bytes).to_bytes()),
        }
    }

    fn keys(signer: &SigningKey) -> AcceptedKeys {
        AcceptedKeys::from_base64([b64(signer.verifying_key().as_bytes())]).unwrap()
    }

    /// AC: round-trip — sign a fixture, verify it.
    #[test]
    fn round_trip_sign_then_verify() {
        let signer = key();
        let artifact = entry(&signer, FIXTURE);
        keys(&signer).verify(&artifact, FIXTURE).unwrap();
    }

    /// AC: a tampered artifact is rejected.
    #[test]
    fn tampered_artifact_rejected() {
        let signer = key();
        let artifact = entry(&signer, FIXTURE);
        let mut tampered = FIXTURE.to_vec();
        tampered[3] ^= 0x01;
        let err = keys(&signer).verify(&artifact, &tampered).unwrap_err();
        assert!(matches!(err, Error::HashMismatch { .. }), "got {err:?}");
    }

    /// AC: a valid signature from a key outside the accepted set is rejected.
    #[test]
    fn signature_from_key_outside_set_rejected() {
        let signer = key();
        let outsider = key();
        let artifact = entry(&outsider, FIXTURE);
        let err = keys(&signer).verify(&artifact, FIXTURE).unwrap_err();
        assert!(matches!(err, Error::NoAcceptedKey), "got {err:?}");
    }

    /// AC: a correct hash with a wrong signature is rejected — the hash does
    /// not vouch for the signature.
    #[test]
    fn correct_hash_with_wrong_signature_rejected() {
        let signer = key();
        let mut artifact = entry(&signer, FIXTURE);
        let other_signature = signer.sign(b"different bytes").to_bytes();
        artifact.signature = b64(&other_signature);
        let err = keys(&signer).verify(&artifact, FIXTURE).unwrap_err();
        assert!(matches!(err, Error::NoAcceptedKey), "got {err:?}");
    }

    /// AC: a manifest whose artifact carries no signature at all is rejected
    /// at parse time, before anything is accepted.
    #[test]
    fn missing_signature_field_is_rejected() {
        let json = r#"{
            "schema": 1,
            "channel": "stable",
            "version": "0.6.1",
            "notes": "",
            "artifacts": {
                "darwin-aarch64": {
                    "url": "https://dl.sirioai.app/a.dmg",
                    "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
                }
            }
        }"#;
        assert!(matches!(
            ChannelManifest::parse(json),
            Err(Error::Json(_))
        ));
    }

    /// An empty signature is not a missing field but is still rejected.
    #[test]
    fn empty_signature_rejected() {
        let signer = key();
        let mut artifact = entry(&signer, FIXTURE);
        artifact.signature = String::new();
        let err = keys(&signer).verify(&artifact, FIXTURE).unwrap_err();
        assert!(
            matches!(err, Error::BadSignatureEncoding(_)),
            "got {err:?}"
        );
    }

    /// Rotation: a binary carries a *set* of accepted keys and accepts a
    /// signature from any member — the old key and the one-release-ahead new
    /// key both verify.
    #[test]
    fn any_key_in_accepted_set_verifies() {
        let old = key();
        let upcoming = key();
        let set = AcceptedKeys::from_base64([
            b64(old.verifying_key().as_bytes()),
            b64(upcoming.verifying_key().as_bytes()),
        ])
        .unwrap();
        set.verify(&entry(&old, FIXTURE), FIXTURE).unwrap();
        set.verify(&entry(&upcoming, FIXTURE), FIXTURE).unwrap();
    }

    #[test]
    fn unknown_schema_rejected() {
        let signer = key();
        let manifest = ChannelManifest {
            schema: 2,
            channel: "stable".into(),
            version: "0.6.1".into(),
            notes: String::new(),
            artifacts: [("darwin-aarch64".to_string(), entry(&signer, FIXTURE))]
                .into_iter()
                .collect(),
        };
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(matches!(
            ChannelManifest::parse(&json),
            Err(Error::UnsupportedSchema(2))
        ));
    }

    /// Unknown fields are refused so a typo'd field can never silently carry
    /// security-relevant data this build ignores.
    #[test]
    fn unknown_fields_rejected() {
        let signer = key();
        let manifest = ChannelManifest {
            schema: MANIFEST_SCHEMA,
            channel: "stable".into(),
            version: "0.6.1".into(),
            notes: String::new(),
            artifacts: [("darwin-aarch64".to_string(), entry(&signer, FIXTURE))]
                .into_iter()
                .collect(),
        };
        let mut json = serde_json::to_value(&manifest).unwrap();
        json["sneaky"] = serde_json::json!("ignored at your peril");
        let json = serde_json::to_string(&json).unwrap();
        assert!(matches!(
            ChannelManifest::parse(&json),
            Err(Error::Json(_))
        ));
    }

    /// The signing tool and the app share this exact wire form: serialize →
    /// parse must be lossless, and a parse of the serialized form must accept
    /// the artifact it was built from.
    #[test]
    fn manifest_wire_format_round_trips() {
        let signer = key();
        let manifest = ChannelManifest {
            schema: MANIFEST_SCHEMA,
            channel: "nightly".into(),
            version: "0.6.1".into(),
            notes: "- fixed a thing".into(),
            artifacts: [("linux-x86_64".to_string(), entry(&signer, FIXTURE))]
                .into_iter()
                .collect(),
        };
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        let parsed = ChannelManifest::parse(&json).unwrap();
        assert_eq!(parsed, manifest);
        keys(&signer)
            .verify(&parsed.artifacts["linux-x86_64"], FIXTURE)
            .unwrap();
    }
}
