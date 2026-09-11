//! `sirio-release` — signing-side CLI for release artifacts (ticket #306).
//!
//!     sirio-release keygen --out <dir>
//!     sirio-release sign --key <file> --channel <c> --version <v> --notes <s>
//!                        --url-template <t> --artifact <platform>=<path> ...
//!                        [--out <file>]
//!     sirio-release verify --manifest <file> --platform <p> --artifact <path>
//!                          --pub-key <base64> ...
//!
//! `keygen` runs once, in the signing environment only; see
//! `docs/release-signing.md` for the procedure and the rotation rules.

use std::collections::BTreeMap;
use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signer as _, SigningKey};
use rand_core::OsRng;
use sha2::{Digest, Sha256};
use sirio_release::{
    AcceptedKeys, ChannelManifest, MANIFEST_SCHEMA, ManifestArtifact, artifact_url,
};

const USAGE: &str = "\
usage:
  sirio-release keygen --out <dir>
  sirio-release sign --key <file> --channel <stable|nightly> --version <v>
                     --notes <text> --url-template <template>
                     --artifact <platform>=<file> ... [--out <file>]
  sirio-release verify --manifest <file> --platform <p> --artifact <file>
                       --pub-key <base64> ...

url-template placeholders: {channel} {version} {platform} {file}
  ({file} is the artifact's own file name, for hosts that keep it)";

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        eprintln!();
        eprintln!("{USAGE}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or("missing command")?;
    let flags = Flags::parse(args)?;

    match command.as_str() {
        "keygen" => keygen(&flags),
        "sign" => sign(&flags),
        "verify" => verify(&flags),
        other => Err(format!("unknown command {other:?}; expected keygen, sign or verify")),
    }
}

/// Flat flag bag: `--flag value`, with `--artifact` and `--pub-key`
/// repeatable. No clap — three commands, hand-rolled is smaller than the
/// dependency.
struct Flags {
    values: BTreeMap<String, String>,
    repeated: Vec<(String, String)>,
}

impl Flags {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut values = BTreeMap::new();
        let mut repeated = Vec::new();
        let mut args = args.peekable();
        while let Some(flag) = args.next() {
            let name = flag
                .strip_prefix("--")
                .filter(|_| flag.len() > 2)
                .ok_or_else(|| format!("expected a --flag, got {flag:?}"))?;
            let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
            if matches!(name, "artifact" | "pub-key") {
                repeated.push((name.to_string(), value));
            } else if values.insert(name.to_string(), value).is_some() {
                return Err(format!("{flag} given twice"));
            }
        }
        Ok(Self { values, repeated })
    }

    fn required(&self, name: &str) -> Result<String, String> {
        self.values
            .get(name)
            .cloned()
            .ok_or_else(|| format!("--{name} is required"))
    }
}

fn keygen(flags: &Flags) -> Result<(), String> {
    let out = PathBuf::from(flags.required("out")?);
    std::fs::create_dir_all(&out).map_err(|e| format!("create {}: {e}", out.display()))?;

    let signing = SigningKey::generate(&mut OsRng);
    let seed = signing.to_bytes();
    let public = signing.verifying_key().to_bytes();

    let key_path = out.join("sirio-release-signing.key");
    let pub_path = out.join("sirio-release-signing.pub");
    write_private(&key_path, &BASE64.encode(seed))?;
    std::fs::write(&pub_path, format!("{}\n", BASE64.encode(public)))
        .map_err(|e| format!("write {}: {e}", pub_path.display()))?;

    println!("private key: {}  (keep out of the repo; see docs/release-signing.md)", key_path.display());
    println!("public key:  {}  (compile into AcceptedKeys)", pub_path.display());
    println!("public (base64): {}", BASE64.encode(public));
    Ok(())
}

fn sign(flags: &Flags) -> Result<(), String> {
    let key_file = flags.required("key")?;
    let channel = flags.required("channel")?;
    let version = flags.required("version")?;
    let notes = flags.required("notes")?;
    let url_template = flags.required("url-template")?;
    let artifacts = collect_artifacts(flags, "artifact")?;

    let seed_text = std::fs::read_to_string(key_file)
        .map_err(|e| format!("read signing key: {e}"))?;
    let signing = signing_key_from_base64(seed_text.trim())?;

    let mut entries = BTreeMap::new();
    for (platform, path) in artifacts {
        let bytes = std::fs::read(&path).map_err(|e| format!("read {path:?}: {e}"))?;
        let sha256 = hex::encode(Sha256::digest(&bytes));
        let signature = BASE64.encode(signing.sign(&bytes).to_bytes());
        let file = PathBuf::from(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .ok_or_else(|| format!("--artifact {platform}={path}: path has no file name"))?;
        let url = artifact_url(&url_template, &channel, &version, &platform, &file);
        entries.insert(platform, ManifestArtifact { url, sha256, signature });
    }

    let manifest = ChannelManifest {
        schema: MANIFEST_SCHEMA,
        channel,
        version,
        notes,
        artifacts: entries,
    };
    emit(&serde_json::to_string_pretty(&manifest).unwrap(), flags)?;
    Ok(())
}

fn verify(flags: &Flags) -> Result<(), String> {
    let manifest_file = flags.required("manifest")?;
    let platform = flags.required("platform")?;
    let artifact_path = single_value(flags, "artifact")?;
    let pub_keys = repeated_values(flags, "pub-key");

    let json = std::fs::read_to_string(manifest_file)
        .map_err(|e| format!("read manifest: {e}"))?;
    let manifest = ChannelManifest::parse(&json).map_err(|e| e.to_string())?;
    let entry = manifest
        .artifacts
        .get(&platform)
        .ok_or_else(|| format!("manifest has no artifact for platform {platform:?}"))?;
    let bytes = std::fs::read(&artifact_path).map_err(|e| format!("read {artifact_path:?}: {e}"))?;
    let keys = AcceptedKeys::from_base64(pub_keys).map_err(|e| e.to_string())?;

    keys.verify(entry, &bytes).map_err(|e| e.to_string())?;
    println!(
        "OK {} {} ({})",
        manifest.version, platform, entry.url
    );
    Ok(())
}

/// Repeatable `--<name> <key>=<value>` pairs (sign's `--artifact`).
fn collect_artifacts(flags: &Flags, name: &str) -> Result<Vec<(String, String)>, String> {
    let pairs: Result<Vec<_>, _> = flags
        .repeated
        .iter()
        .filter(|(flag, _)| flag == name)
        .map(|(_, value)| {
            value
                .split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .ok_or_else(|| format!("--{name} expects <key>=<value>, got {value:?}"))
        })
        .collect();
    let pairs = pairs?;
    if pairs.is_empty() {
        return Err(format!("at least one --{name} is required"));
    }
    Ok(pairs)
}

/// Repeatable single-value flags (`--pub-key`); base64 padding contains `=`,
/// so these must never go through the key=value split.
fn repeated_values(flags: &Flags, name: &str) -> Vec<String> {
    flags
        .repeated
        .iter()
        .filter(|(flag, _)| flag == name)
        .map(|(_, value)| value.clone())
        .collect()
}

fn single_value(flags: &Flags, name: &str) -> Result<String, String> {
    let mut values = flags
        .repeated
        .iter()
        .filter(|(flag, _)| flag == name)
        .map(|(_, value)| value.clone());
    let value = values.next().ok_or_else(|| format!("--{name} is required"))?;
    if values.next().is_some() {
        return Err(format!("--{name} given more than once"));
    }
    Ok(value)
}

fn signing_key_from_base64(text: &str) -> Result<SigningKey, String> {
    let seed: [u8; 32] = BASE64
        .decode(text)
        .map_err(|e| format!("signing key is not base64: {e}"))?
        .try_into()
        .map_err(|_| "signing key must be 32 bytes of base64".to_string())?;
    Ok(SigningKey::from_bytes(&seed))
}

fn emit(json: &str, flags: &Flags) -> Result<(), String> {
    match flags.values.get("out") {
        Some(path) => std::fs::write(path, format!("{json}\n"))
            .map_err(|e| format!("write {path:?}: {e}")),
        None => {
            println!("{json}");
            Ok(())
        }
    }
}

/// The seed file is the release signature itself: owner-only on unix.
#[cfg(unix)]
fn write_private(path: &std::path::Path, contents: &str) -> Result<(), String> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut file| file.write_all(contents.as_bytes()))
        .map_err(|e| format!("write {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn write_private(path: &std::path::Path, contents: &str) -> Result<(), String> {
    std::fs::write(path, contents).map_err(|e| format!("write {}: {e}", path.display()))
}
