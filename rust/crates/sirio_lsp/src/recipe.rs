//! How to get the server a language names, when Sirio can get it.
//!
//! Plain data, and deliberately so: this crate is a leaf and must not learn
//! `sirio_registry`'s vocabulary. `sirio` is the one place that sees both,
//! and it is where a `Recipe` becomes something the installer understands.

/// One downloadable file, pinned. See the design's §7 for why the version,
/// the hash and the size are written here rather than queried at install
/// time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub url: &'static str,
    /// Lower-case hex, 64 characters. GitHub publishes this on every asset
    /// as `digest: "sha256:…"`, so unlike the ACP registry — where 47 of 95
    /// artifacts publish nothing — every install here is verified.
    pub sha256: &'static str,
    /// Shown before the button is pressed. clangd is 114 MB, and an
    /// `[Install]` that does not say so is a dishonest button.
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recipe {
    /// One npm package at one version. `bin` names which executable inside
    /// it, because json, html and css share `vscode-langservers-extracted`
    /// and must share one install.
    Npm {
        /// Also the store id: three languages, one directory.
        package: &'static str,
        version: &'static str,
        bin: &'static str,
    },
    /// A pinned release asset per platform, keyed by
    /// `sirio_registry::current_platform_key()`'s spelling. A platform the
    /// project publishes nothing for is simply absent.
    Release {
        id: &'static str,
        version: &'static str,
        /// Path of the executable inside the unpacked archive.
        bin: &'static str,
        assets: &'static [(&'static str, Asset)],
    },
    /// Why it cannot be installed, in terms a reader can act on. The arm
    /// Zed has no equivalent for: where Zed says nothing, this says what
    /// has to come first.
    Manual {
        needs: &'static str,
        url: &'static str,
    },
}

impl Recipe {
    /// Whether this recipe can install anything at all, as opposed to
    /// explaining why it cannot.
    pub fn is_installable(&self) -> bool {
        !matches!(self, Self::Manual { .. })
    }

    /// The store directory this recipe installs into. `None` for `Manual`.
    /// Shared on purpose where the package is shared.
    pub fn store_id(&self) -> Option<&'static str> {
        match self {
            Self::Npm { package, .. } => Some(package),
            Self::Release { id, .. } => Some(id),
            Self::Manual { .. } => None,
        }
    }
}
