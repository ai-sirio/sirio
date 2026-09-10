//! The live end-to-end probe for the update chain, driven by
//! `Scripts/Tests/test-update-e2e.sh`.
//!
//! Everything under `sirio_update` and `sirio_apply` is unit-tested with an
//! injected fetch and an injected launcher, which is the right shape for
//! those crates but leaves the one question a shipped updater actually turns
//! on unanswered: does the *compiled* wiring work? The manifest URL, the
//! release channel and the accepted keys are all `option_env!` — baked in at
//! build time so a Stable binary has no test endpoint to be pointed at — so
//! the only way to exercise them is to compile a binary the same way the
//! release job compiles the app, and let it talk to a real server.
//!
//! That is this: the real [`sirio_update::Updater`] over real HTTP against a
//! really-signed manifest, then the real [`sirio_apply::apply`] with the real
//! process spawn. It reports what happened on stdout in a line-per-stage
//! shape and exits non-zero at the first stage that did not complete; the
//! script owns which outcome is the expected one, because "the update was
//! refused" is the correct answer in some of the cases it drives.
//!
//! Not a `#[test]`: it needs a server, a compile-time environment and (on
//! Windows) an install directory to run from, none of which belong inside a
//! `cargo test` run.

use std::path::PathBuf;

fn main() {
    let staging = match std::env::args().nth(1) {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("usage: update_probe <staging-dir>");
            std::process::exit(2);
        }
    };

    // Printed first and asserted by the script: if cargo reused a cached
    // build that did not see the environment, these two lines say so before
    // any network work makes the failure look like something else.
    println!(
        "CHANNEL {}",
        sirio_update::ReleaseChannel::RELEASE_CHANNEL.as_str()
    );
    println!("MANIFEST_URL {}", sirio_update::manifest_url());

    // The same reading `sirio`'s own `accepted_release_keys` does, kept
    // deliberately in step with it: a probe that accepted keys the app would
    // not would verify something the app rejects.
    let raw = option_env!("SIRIO_RELEASE_ACCEPTED_KEYS").unwrap_or("");
    let keys = match sirio_release::AcceptedKeys::from_base64(raw.split_whitespace()) {
        Ok(keys) => keys,
        Err(error) => {
            println!("KEYS err {error}");
            std::process::exit(3);
        }
    };

    let mut updater = sirio_update::Updater::new(&staging, keys);

    let available = match updater.check(false) {
        Ok(sirio_update::CheckResult::Available(update)) => {
            println!("CHECK available {}", update.version);
            update
        }
        Ok(other) => {
            println!("CHECK {other:?}");
            std::process::exit(10);
        }
        Err(error) => {
            println!("CHECK err {error}");
            std::process::exit(11);
        }
    };
    // Newlines flattened so each stage stays one greppable line.
    println!("NOTES {}", available.notes.replace('\n', " "));
    println!("ARTIFACT_URL {}", available.artifact.url);

    let verified = match updater.download(&available, false) {
        Ok(sirio_update::DownloadResult::Ready(verified)) => {
            println!("DOWNLOAD ready {}", verified.path.display());
            verified
        }
        Ok(other) => {
            println!("DOWNLOAD {other:?}");
            std::process::exit(12);
        }
        Err(error) => {
            println!("DOWNLOAD err {error}");
            std::process::exit(13);
        }
    };

    match sirio_apply::apply(&verified) {
        Ok(()) => println!("APPLY ok"),
        Err(error) => {
            println!("APPLY err {error}");
            std::process::exit(14);
        }
    }
}
