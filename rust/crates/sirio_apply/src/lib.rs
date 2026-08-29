//! Applying a verified Sirio update, per platform (#313, #314, #315).
//!
//! [`sirio_update`] deliberately stops at a [`VerifiedUpdate`] — a staged,
//! hash- and signature-verified artifact that has never been executed. This
//! crate owns the other half: turning that file into an installed Sirio on
//! the platform it runs on. It consumes the download; it never re-checks it.
//!
//! # Windows (#315)
//!
//! A verified download is applied by **re-running the installer silently**:
//! `SirioSetup-<version>.exe /VERYSILENT /NORESTART`. Windows is the only
//! platform where the update restarts the app — Inno closes the running
//! instance and relaunches it as part of the install, which is exactly why
//! the confirming control's label differs there ("Update and restart Sirio").
//!
//! Self-location comes first and refusing is a valid outcome ([spec §5.1]).
//! The install directory is `%LOCALAPPDATA%\Programs\Sirio` (the installer's
//! `DefaultDirName`). When `sirio.exe` is not running from there — a
//! `cargo run` dev build, a copied binary — the update is refused with a
//! pointer to the download page instead of guessing. Because the installed
//! `AppId` matches, the re-run is an upgrade in place, not a second
//! installation, and Inno already rolls back a failed install on its own
//! ([spec §6.1]).
//!
//! # macOS (#313)
//!
//! A verified download is a `.dmg` around a `Sirio.app`; it is applied by
//! **swapping the installed bundle** (see [`macos`]). The running instance
//! carries on until the user relaunches — macOS never restarts the app, so a
//! successful return means "the new version is on disk".
//!
//! # Linux (#314)
//!
//! A verified download is an AppImage; it is applied by **renaming over the
//! running image** (see [`linux`]). Self-location reads `$APPIMAGE` — inside
//! an AppImage `current_exe()` points into the ephemeral squashfs mount, not
//! the file to replace — and its absence is the "this is not an install"
//! signal. The running instance carries on until the user relaunches, like
//! macOS.
//!
//! Both platform modules are compiled on every target so their logic and
//! tests run everywhere; the only `#[cfg(target_os)]` in this crate is the
//! dispatch inside [`apply`] (plus the raw process-spawning primitives no
//! other platform can execute).
//!
//! [spec §5.1]: https://github.com/ai-sirio/sirio/blob/main/docs/superpowers/specs/2026-08-29-auto-update-design.md
//! [spec §6.1]: https://github.com/ai-sirio/sirio/blob/main/docs/superpowers/specs/2026-08-29-auto-update-design.md

use std::path::{Path, PathBuf};

use sirio_update::VerifiedUpdate;

/// Where a human downloads Sirio when an in-place update must be refused.
///
/// The artifact host is `https://dl.sirioai.app` (read from the manifest on
/// every check), but the human-facing starting point of the "how do I get
/// Sirio back" path is the release page itself (spec §2.1/§2.4: GitHub
/// Releases is the artifact host, and §5.1 says to point at the download page
/// rather than guess an install path).
pub const DOWNLOAD_PAGE_URL: &str = "https://github.com/ai-sirio/sirio/releases";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ApplyError {
    /// Self-location failed: this copy of Sirio is not running from an
    /// install location the updater recognizes, so updating in place is
    /// refused rather than guessed at.
    #[error(
        "Sirio is not running from an install directory the updater recognizes; \
         download Sirio from {DOWNLOAD_PAGE_URL} and install it there"
    )]
    InstallNotFound,
    /// The installer (or swap mechanism) could not be started.
    #[error("the update could not be applied because it failed to start: {0}")]
    Launch(String),
    /// The swap failed; the previous version is back in place (macOS) or
    /// untouched (Linux, where the rename is atomic).
    #[error("the update could not be swapped into place: {0}")]
    Swap(String),
    /// The macOS swap failed *and* the previous version could not be put
    /// back: the user's working copy is the moved-aside one.
    #[error(
        "the swap failed ({attempted}); rolling back also failed, so the previous \
         version is still at {aside:?} — restore it by hand"
    )]
    Rollback { attempted: String, aside: PathBuf },
    /// This platform's applying module has not landed yet (see crate docs).
    #[error("applying an update is not implemented on this platform yet")]
    UnsupportedPlatform,
}

/// Apply a verified update on the current platform.
///
/// On Windows this is fire-and-forget by design: the installer closes the
/// running app and restarts it, so a successful return means "the installer
/// was handed the update", not "the update is installed". On macOS the swap
/// is synchronous and a successful return means "the new version is on
/// disk"; the running instance keeps going until the user relaunches.
pub fn apply(update: &VerifiedUpdate) -> Result<(), ApplyError> {
    // The dispatch is the only platform decision in the crate: each module
    // owns its whole applying sequence (self-location + how the artifact is
    // applied), so nothing platform-specific leaks into shared code.
    #[cfg(target_os = "windows")]
    return windows::apply(update, &real_launch);
    #[cfg(target_os = "macos")]
    return macos::apply(update);
    #[cfg(target_os = "linux")]
    return linux::apply(update);
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = update;
        Err(ApplyError::UnsupportedPlatform)
    }
}

/// The injectable launcher seam: takes the staged artifact and its
/// arguments, returns the launch outcome. [`apply`] uses [`real_launch`].
/// The lifetime parameter keeps the trait object bound to the caller's
/// region (a bare `dyn` in an alias would default to `'static`).
type Launcher<'a> = dyn Fn(&Path, &[&str]) -> Result<(), String> + 'a;

/// The real launcher: spawn the staged installer, detached, and let it run.
#[cfg(target_os = "windows")]
fn real_launch(path: &Path, args: &[&str]) -> Result<(), String> {
    windows::spawn_silent(path, args).map(|_child| ())
}

pub mod linux;
pub mod macos;
mod windows;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn expected_install_dir_is_localappdata_programs_sirio() {
        let base = Path::new(r"C:\Users\someone\AppData\Local");
        assert_eq!(
            windows::expected_install_dir(base),
            PathBuf::from(r"C:\Users\someone\AppData\Local\Programs\Sirio")
        );
    }

    #[test]
    fn self_locate_accepts_an_exe_in_the_expected_dir() {
        let expected = PathBuf::from(r"C:\Users\someone\AppData\Local\Programs\Sirio");
        assert_eq!(
            windows::self_locate_at(expected.join("sirio.exe").as_path(), &expected),
            Ok(expected)
        );
    }

    #[test]
    fn self_locate_refuses_an_exe_outside_the_install_dir() {
        let expected = PathBuf::from(r"C:\Users\someone\AppData\Local\Programs\Sirio");
        let dev_build_exe = PathBuf::from(r"D:\projects\sirio\rust\target\debug\sirio.exe");
        assert_eq!(
            windows::self_locate_at(&dev_build_exe, &expected),
            Err(ApplyError::InstallNotFound)
        );
    }
}
