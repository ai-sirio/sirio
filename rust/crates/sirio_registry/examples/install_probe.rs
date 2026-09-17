//! Installs one pinned artifact into a store, so a shell can drive the real
//! installer.
//!
//! `sirio_lsp`'s `lsp_probe` is the other half of the language-server
//! end-to-end test: this one fetches a server and records it, that one
//! launches what the store now records and asks it a question. Neither half
//! is reachable from a shell otherwise. The installer is this crate's, the
//! recipe table is `sirio_lsp`'s, and those two crates deliberately never
//! see each other — the conversion between them is
//! `sirio::lsp_install::agent_for`, which sits behind the app's entire
//! dependency tree (GPUI, bezel, libghostty). A test that has to build the
//! app to install a 22 MB Markdown server is a test nobody runs, so this
//! probe builds the one agent shape `agent_for` builds for a
//! `Recipe::Release` — id, version, and one `Binary` distribution for one
//! platform — out of fields the caller reads verbatim out of `config.rs`.
//!
//! What that leaves unchecked is the conversion itself, which
//! `lsp_install`'s own unit tests cover for all three recipe arms. What it
//! does check is everything downstream: the download, the pinned sha256 in
//! `BinaryArtifact::sha256`, the unpack, the executable bit, and the store
//! manifest the app reads at startup.
//!
//! ```text
//! cargo run -p sirio_registry --example install_probe -- \
//!     <store-root> <platform> <id> <version> <url> <sha256> <bin>
//! ```
//!
//! `EXECUTABLE` is the path out of the *manifest*, re-read after the
//! install rather than whatever the installer returned, so a manifest that
//! does not round-trip shows up here instead of as a server the app cannot
//! find later. Exit codes: 0 installed, 1 the installer refused, 2 usage.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sirio_registry::{BinaryArtifact, Distribution, InstallStore, Installer, RegistryAgent};

fn fail(message: &str) -> ! {
    println!("INSTALL ERR {message}");
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root, platform, id, version, url, sha256, bin] = args.as_slice() else {
        eprintln!(
            "usage: install_probe <store-root> <platform> <id> <version> <url> <sha256> <bin>"
        );
        std::process::exit(2);
    };

    let mut artifacts = BTreeMap::new();
    artifacts.insert(
        platform.clone(),
        BinaryArtifact {
            archive: url.clone(),
            cmd: bin.clone(),
            args: Vec::new(),
            sha256: Some(sha256.clone()),
        },
    );
    let agent = RegistryAgent {
        id: id.clone(),
        name: id.clone(),
        version: version.clone(),
        description: None,
        repository: None,
        website: None,
        license: None,
        icon: None,
        distributions: vec![Distribution::Binary(artifacts)],
    };

    let root = PathBuf::from(root);
    let store = InstallStore::new(root);
    println!("STORE {}", store.root().display());

    if let Err(error) = Installer::new(InstallStore::new(store.root())).install(&agent, platform) {
        fail(&error.to_string());
    }

    // The app never sees the installer's return value; it reads this.
    let Some(recorded) = store.manifest(id) else {
        fail("the install finished but the store records no manifest");
    };
    if !recorded.executable.exists() {
        fail(&format!(
            "the manifest names {} but nothing is there",
            recorded.executable.display()
        ));
    }
    println!("VERSION {}", recorded.version);
    println!("INTEGRITY {:?}", recorded.integrity);
    println!("EXECUTABLE {}", recorded.executable.display());
}
