//! Fetching the registry, with a disk cache that survives being offline.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::model::AcpRegistry;

type Fetch = Box<dyn Fn() -> anyhow::Result<String> + Send + Sync>;
type Clock = Box<dyn Fn() -> SystemTime + Send + Sync>;

pub struct RegistryClient {
    cache_path: PathBuf,
    fetch: Fetch,
    now: Clock,
}

impl RegistryClient {
    pub const REGISTRY_URL: &'static str =
        "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";
    pub const DEFAULT_MAX_AGE: Duration = Duration::from_secs(86_400);

    /// `fetch` and `now` are injected so every rule below is a unit test
    /// with no network and no wall clock.
    pub fn new(
        cache_path: impl Into<PathBuf>,
        fetch: impl Fn() -> anyhow::Result<String> + Send + Sync + 'static,
        now: impl Fn() -> SystemTime + Send + Sync + 'static,
    ) -> Self {
        Self {
            cache_path: cache_path.into(),
            fetch: Box::new(fetch),
            now: Box::new(now),
        }
    }

    /// The production constructor.
    pub fn with_http(cache_path: impl Into<PathBuf>) -> Self {
        Self::new(
            cache_path,
            || {
                let body = ureq::get(Self::REGISTRY_URL)
                    .config()
                    .timeout_global(Some(Duration::from_secs(30)))
                    .build()
                    .call()?
                    .body_mut()
                    .read_to_string()?;
                Ok(body)
            },
            SystemTime::now,
        )
    }

    pub fn registry(&self, max_age: Duration, force: bool) -> anyhow::Result<AcpRegistry> {
        if !force && let Some(cached) = self.fresh_cache(max_age) {
            return Ok(cached);
        }

        match (self.fetch)() {
            Ok(body) => match AcpRegistry::from_json(&body) {
                Ok(registry) => {
                    // Validated first: a bad payload never clobbers a good
                    // cache. A cache that will not take the write is a
                    // stale cache next time, not a failed fetch: the
                    // payload in hand is already validated, and discarding
                    // it would make an unwritable cache directory
                    // indistinguishable from being offline.
                    if let Err(error) = self.write_cache(&body) {
                        eprintln!("sirio: registry fetched but not cached: {error}");
                    }
                    Ok(registry)
                }
                Err(_) => self
                    .cached()
                    .ok_or_else(|| anyhow::anyhow!("registry payload did not decode")),
            },
            Err(error) => self.cached().ok_or(error),
        }
    }

    fn fresh_cache(&self, max_age: Duration) -> Option<AcpRegistry> {
        let modified = std::fs::metadata(&self.cache_path).ok()?.modified().ok()?;
        let age = (self.now)().duration_since(modified).ok()?;
        (age < max_age).then(|| self.cached())?
    }

    /// A cache that fails to decode is absent, not fatal.
    fn cached(&self) -> Option<AcpRegistry> {
        let body = std::fs::read_to_string(&self.cache_path).ok()?;
        AcpRegistry::from_json(&body).ok()
    }

    /// This file is read every time the Agents screen opens, so a process
    /// killed mid-write must not leave a truncated document behind, and a
    /// second writer must not be able to take this one's scratch file away
    /// before it lands. Both guarantees live in `crate::atomic`.
    fn write_cache(&self, body: &str) -> anyhow::Result<()> {
        crate::atomic::write_atomically(&self.cache_path, body.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    const GOOD: &str = r#"{"version":"1.0.0","agents":[
        {"id":"a","name":"A","version":"1.0.0",
         "distribution":{"npx":{"package":"a@1.0.0"}}}]}"#;
    const OTHER: &str = r#"{"version":"2.0.0","agents":[]}"#;

    fn temp_cache(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("sirio-registry-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("registry.json")
    }

    #[test]
    fn a_fresh_cache_is_served_without_touching_the_network() {
        let cache = temp_cache("fresh");
        std::fs::write(&cache, GOOD).unwrap();
        let client = RegistryClient::new(
            cache,
            // The panic is the assertion: reaching the network at all is
            // the failure.
            || panic!("the network must not be touched for a fresh cache"),
            SystemTime::now,
        );
        let registry = client.registry(Duration::from_secs(86_400), false).unwrap();
        assert_eq!(registry.version, "1.0.0");
    }

    #[test]
    fn a_failed_fetch_falls_back_to_the_cached_copy() {
        let cache = temp_cache("offline");
        std::fs::write(&cache, GOOD).unwrap();
        let client = RegistryClient::new(
            cache,
            || Err(anyhow::anyhow!("no network")),
            SystemTime::now,
        );
        let registry = client.registry(Duration::ZERO, true).unwrap();
        assert_eq!(
            registry.version, "1.0.0",
            "an installed agent still launches offline; that is the point of installing"
        );
    }

    #[test]
    fn an_invalid_payload_never_clobbers_a_good_cache() {
        let cache = temp_cache("clobber");
        std::fs::write(&cache, GOOD).unwrap();
        let client = RegistryClient::new(
            cache.clone(),
            || Ok("{ this is not json".to_string()),
            SystemTime::now,
        );
        let registry = client.registry(Duration::ZERO, true).unwrap();
        assert_eq!(registry.version, "1.0.0");
        assert_eq!(
            std::fs::read_to_string(&cache).unwrap(),
            GOOD,
            "the good cache is still on disk byte for byte"
        );
    }

    #[test]
    fn a_successful_fetch_replaces_the_cache() {
        let cache = temp_cache("replace");
        std::fs::write(&cache, GOOD).unwrap();
        let client = RegistryClient::new(cache.clone(), || Ok(OTHER.to_string()), SystemTime::now);
        let registry = client.registry(Duration::ZERO, true).unwrap();
        assert_eq!(registry.version, "2.0.0");
        assert_eq!(std::fs::read_to_string(&cache).unwrap(), OTHER);
        assert!(
            scratch_files(&cache).is_empty(),
            "the scratch file used for the atomic write is gone"
        );
    }

    #[test]
    fn a_corrupt_cache_is_treated_as_absent_rather_than_fatal() {
        // A build killed mid-write, or a hand-edited file, must not make
        // the Agents screen permanently unopenable.
        let cache = temp_cache("corrupt");
        std::fs::write(&cache, "{ truncated").unwrap();
        let client = RegistryClient::new(cache, || Ok(GOOD.to_string()), SystemTime::now);
        let registry = client.registry(Duration::from_secs(86_400), false).unwrap();
        assert_eq!(registry.version, "1.0.0");
    }

    #[test]
    fn no_cache_and_no_network_is_an_error_not_a_fabricated_registry() {
        let cache = temp_cache("nothing");
        let _ = std::fs::remove_file(&cache);
        let client = RegistryClient::new(
            cache,
            || Err(anyhow::anyhow!("no network")),
            SystemTime::now,
        );
        assert!(client.registry(Duration::ZERO, true).is_err());
    }

    /// Any leftover scratch file, whatever this writer happened to name
    /// its own — asserting on one fixed name would pass vacuously the
    /// moment the naming changes.
    fn scratch_files(cache: &std::path::Path) -> Vec<String> {
        std::fs::read_dir(cache.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn a_validated_registry_survives_a_cache_that_cannot_be_written() {
        // An unwritable cache directory is a persistence problem. Letting
        // it fail the call would report "no registry" to a caller holding
        // a registry that fetched and decoded perfectly.
        let dir =
            std::env::temp_dir().join(format!("sirio-registry-blocked-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let blocker = dir.join("not-a-directory");
        std::fs::write(
            &blocker,
            "this is a file, so nothing can be created inside it",
        )
        .unwrap();

        let client = RegistryClient::new(
            blocker.join("registry.json"),
            || Ok(GOOD.to_string()),
            SystemTime::now,
        );

        let registry = client
            .registry(Duration::ZERO, true)
            .expect("a fetched, validated registry is returned even when it cannot be cached");
        assert_eq!(registry.version, "1.0.0");
    }
}
