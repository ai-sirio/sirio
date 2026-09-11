//! The payload `Scripts/Tests/test-update-e2e.sh` signs and serves in place of
//! the real Inno installer.
//!
//! It exists so the end-to-end probe can prove the *whole* chain ran rather
//! than stopping one step short: the marker file this writes can only appear
//! if the manifest parsed, the artifact downloaded, its SHA-256 and Ed25519
//! signature both verified, the staged file landed under the staging
//! directory, self-location accepted the install, and `spawn_silent` launched
//! the extensionless staged PE. Its contents are the arguments it was given,
//! which is how the test asserts `/VERYSILENT /NORESTART` reached the child.
//!
//! Deliberately not an installer of any kind: it writes one file and exits.

fn main() {
    let marker = std::env::var("SIRIO_FAKE_INSTALLER_MARKER").expect(
        "SIRIO_FAKE_INSTALLER_MARKER must name the file this stand-in installer writes; \
         it is set by Scripts/Tests/test-update-e2e.sh and inherited through the spawn",
    );
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::fs::write(&marker, args.join(" "))
        .unwrap_or_else(|error| panic!("could not write the marker at {marker}: {error}"));
}
