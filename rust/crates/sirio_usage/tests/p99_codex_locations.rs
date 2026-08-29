//! P99 exercise of `F-CORE-USG-05`'s location and lifetime clauses: Codex
//! credentials load from `$CODEX_HOME/auth.json` or `~/.codex/auth.json`
//! (in that precedence, with an empty `$CODEX_HOME` falling back to home),
//! require both tokens, and are refresh-needed after eight days.
//!
//! Environment variables are process-global and the test harness runs one
//! file's tests as threads of one process — so, exactly like
//! `sirio_agents/tests/home_isolation.rs`, this lives in its own file (its
//! own process) and keeps every env mutation inside ONE serial `#[test]`.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use sirio_usage::{
    CodexOAuthCredentials, CredentialLoadError, codex_auth_file_path, codex_has_credentials_at,
    load_codex_credentials,
};

/// 2026-08-01T00:00:00Z — the `last_refresh` written into both fixtures.
const LAST_REFRESH_EPOCH: u64 = 1_785_542_400;
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("sirio-p99-codex-loc-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create temporary directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn auth_file_precedence_load_refresh_and_missing_at_both_locations() {
    let home = TempDir::new("home");
    let codex_home = TempDir::new("codex-home");
    let empty_codex_home = TempDir::new("empty");

    // A valid credential file at the $CODEX_HOME location…
    std::fs::write(
        codex_home.path().join("auth.json"),
        r#"{"auth_mode":"oauth","tokens":{"access_token":"a1","refresh_token":"r1","account_id":"acct-1"},"last_refresh":"2026-08-01T00:00:00Z"}"#,
    )
    .unwrap();
    // …and a valid one, without account id, at the ~/.codex location.
    std::fs::create_dir_all(home.path().join(".codex")).unwrap();
    std::fs::write(
        home.path().join(".codex/auth.json"),
        r#"{"tokens":{"access_token":"a2","refresh_token":"r2"},"last_refresh":"2026-08-01T00:00:00Z"}"#,
    )
    .unwrap();

    let original_home = std::env::var_os("HOME");
    let original_codex_home = std::env::var_os("CODEX_HOME");
    // The "~" in `~/.codex` is resolved by `user_home_dir`, which reads
    // `HOME` on unix and `USERPROFILE` on Windows (a native GUI launch
    // there never sets `HOME`; see that function's doc comment). The
    // fixture must point *both* knobs at the temp dir or the fallback
    // asserts resolve against the real home on Windows. Setting
    // `USERPROFILE` here is harmless on unix — the unix arm never reads
    // it.
    let original_userprofile = std::env::var_os("USERPROFILE");
    // SAFETY: this is the only test in this file, so nothing else in this
    // process reads these variables concurrently.
    unsafe {
        std::env::set_var("HOME", home.path());
        std::env::set_var("USERPROFILE", home.path());
        std::env::set_var("CODEX_HOME", codex_home.path());
    }

    // $CODEX_HOME set: it wins over ~/.codex.
    assert_eq!(codex_auth_file_path(), codex_home.path().join("auth.json"));
    let credentials = load_codex_credentials(&codex_auth_file_path()).expect("loads");
    assert_eq!(credentials.access_token, "a1");
    assert_eq!(credentials.refresh_token, "r1");
    assert_eq!(credentials.account_id.as_deref(), Some("acct-1"));
    let last_refresh = SystemTime::UNIX_EPOCH + Duration::from_secs(LAST_REFRESH_EPOCH);
    assert_eq!(credentials.last_refresh, Some(last_refresh));

    // Refresh-needed after eight days, deterministically probed around the
    // fixture's own last_refresh (valid at 7 days, expired past 8).
    assert!(!credentials.needs_refresh(last_refresh + 7 * DAY));
    assert!(credentials.needs_refresh(last_refresh + 8 * DAY + Duration::from_secs(1)));

    // An empty $CODEX_HOME falls back to ~/.codex/auth.json.
    unsafe { std::env::set_var("CODEX_HOME", "") };
    assert_eq!(codex_auth_file_path(), home.path().join(".codex/auth.json"));
    let fallback = load_codex_credentials(&codex_auth_file_path()).expect("loads from home");
    assert_eq!(fallback.access_token, "a2");
    assert_eq!(fallback.account_id, None);

    // An unset $CODEX_HOME falls back the same way.
    unsafe { std::env::remove_var("CODEX_HOME") };
    assert_eq!(codex_auth_file_path(), home.path().join(".codex/auth.json"));

    // Missing file: presence is false and the load is an IO error.
    unsafe { std::env::set_var("CODEX_HOME", empty_codex_home.path()) };
    let missing = codex_auth_file_path();
    assert_eq!(missing, empty_codex_home.path().join("auth.json"));
    assert!(!codex_has_credentials_at(&missing));
    assert!(matches!(
        load_codex_credentials(&missing),
        Err(CredentialLoadError::Io(_))
    ));

    // Both tokens are required: a file carrying only an access token does
    // not load (and one carrying only a refresh token does not either).
    let partial = empty_codex_home.path().join("partial.json");
    std::fs::write(&partial, r#"{"tokens":{"access_token":"only-a"}}"#).unwrap();
    assert!(matches!(
        load_codex_credentials(&partial),
        Err(CredentialLoadError::MissingTokens)
    ));
    std::fs::write(&partial, r#"{"tokens":{"refresh_token":"only-r"}}"#).unwrap();
    assert!(matches!(
        load_codex_credentials(&partial),
        Err(CredentialLoadError::MissingTokens)
    ));
    assert!(!codex_has_credentials_at(&partial));

    // A credential file without a last_refresh needs a refresh immediately.
    let no_refresh = empty_codex_home.path().join("no-refresh.json");
    std::fs::write(
        &no_refresh,
        r#"{"tokens":{"access_token":"a","refresh_token":"r"}}"#,
    )
    .unwrap();
    let credentials: CodexOAuthCredentials = load_codex_credentials(&no_refresh).expect("loads");
    assert_eq!(credentials.last_refresh, None);
    assert!(credentials.needs_refresh(SystemTime::UNIX_EPOCH + Duration::from_secs(1)));

    // SAFETY: restore the process environment for hygiene.
    unsafe {
        match &original_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        match &original_codex_home {
            Some(value) => std::env::set_var("CODEX_HOME", value),
            None => std::env::remove_var("CODEX_HOME"),
        }
        match &original_userprofile {
            Some(value) => std::env::set_var("USERPROFILE", value),
            None => std::env::remove_var("USERPROFILE"),
        }
    }
}
