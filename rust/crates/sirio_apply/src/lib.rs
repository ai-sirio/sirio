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
//! # macOS / Linux (#313 / #314)
//!
//! Not implemented here: each platform ticket owns its own applying module.
//! Until they land, calling [`apply`] on a non-Windows target is an error
//! rather than a silent no-op.
//!
//! [spec §5.1]: https://github.com/ai-sirio/sirio/blob/main/docs/superpowers/specs/2026-08-29-auto-update-design.md
//! [spec §6.1]: https://github.com/ai-sirio/sirio/blob/main/docs/superpowers/specs/2026-08-29-auto-update-design.md

use std::path::Path;

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
    /// This platform's applying module has not landed yet (see crate docs).
    #[error("applying an update is not implemented on this platform yet")]
    UnsupportedPlatform,
}

/// The directory a Windows install of Sirio lives in: the Inno
/// `DefaultDirName={localappdata}\Programs\Sirio` from `Scripts/build-inno.sh`
/// (#310). Anything else — a dev build, a copied binary — is not an install
/// the updater can update in place.
///
/// Pure path arithmetic, so it lives here rather than in the `windows`
/// module: its tests are about Windows *paths*, not the Windows API, and a
/// cfg-gated definition made `cargo test` fail to compile everywhere else.
pub fn expected_install_dir(local_app_data: &Path) -> std::path::PathBuf {
    local_app_data.join("Programs").join("Sirio")
}

/// Pure self-location check: `sirio.exe` must be running directly out of the
/// expected install directory. Compiled on every platform for the same reason
/// as [`expected_install_dir`].
pub fn self_locate_at(
    exe_path: &Path,
    expected_dir: &Path,
) -> Result<std::path::PathBuf, ApplyError> {
    let exe_dir = exe_path.parent().ok_or(ApplyError::InstallNotFound)?;
    if exe_dir != expected_dir {
        return Err(ApplyError::InstallNotFound);
    }
    Ok(expected_dir.to_path_buf())
}

/// Apply a verified update on the current platform.
///
/// On Windows this is fire-and-forget by design: the installer closes the
/// running app and restarts it, so a successful return means "the installer
/// was handed the update", not "the update is installed".
pub fn apply(update: &VerifiedUpdate) -> Result<(), ApplyError> {
    // Self-location first; the refusal *is* the outcome when this is not an
    // install (a dev build or copied binary) — nothing is launched then.
    windows::self_locate()?;
    launch(update, &real_launch)
}

/// The injectable launcher seam: takes the staged artifact and its
/// arguments, returns the launch outcome. [`apply`] uses [`real_launch`].
/// The lifetime parameter keeps the trait object bound to the caller's
/// region (a bare `dyn` in an alias would default to `'static`).
type Launcher<'a> = dyn Fn(&Path, &[&str]) -> Result<(), String> + 'a;

/// The launch half of [`apply`], once self-location has already refused.
///
/// Testable with a fake launcher so the assembly of the installer command
/// (path + `/VERYSILENT /NORESTART`) is asserted without spawning anything.
fn launch(update: &VerifiedUpdate, run: &Launcher<'_>) -> Result<(), ApplyError> {
    run(&update.path, &["/VERYSILENT", "/NORESTART"]).map_err(ApplyError::Launch)
}

/// The real launcher: spawn the staged installer, detached, and let it run.
fn real_launch(path: &Path, args: &[&str]) -> Result<(), String> {
    windows::spawn_silent(path, args).map(|_child| ())
}

#[cfg(target_os = "windows")]
mod windows;
#[cfg(not(target_os = "windows"))]
mod windows {
    use super::*;

    pub fn self_locate() -> Result<std::path::PathBuf, ApplyError> {
        Err(ApplyError::UnsupportedPlatform)
    }

    pub fn spawn_silent(_path: &Path, _args: &[&str]) -> Result<std::process::Child, String> {
        Err("no launcher on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn update(path: &str) -> VerifiedUpdate {
        VerifiedUpdate {
            version: "0.7.0".into(),
            notes: "notes".into(),
            path: path.into(),
            platform: "windows-x86_64".into(),
        }
    }

    #[test]
    fn expected_install_dir_is_localappdata_programs_sirio() {
        let base = Path::new(r"C:\Users\someone\AppData\Local");
        let dir = expected_install_dir(base);
        // Compared by components rather than by a literal `\`-joined string:
        // `Path::join` writes the *host's* separator, so the literal form only
        // holds when the test itself runs on Windows. What the function owes
        // is the same two components under the given base on every platform.
        assert!(dir.starts_with(base));
        assert_eq!(dir.strip_prefix(base).unwrap(), Path::new("Programs/Sirio"));
    }

    #[test]
    fn self_locate_accepts_an_exe_in_the_expected_dir() {
        let expected = PathBuf::from(r"C:\Users\someone\AppData\Local\Programs\Sirio");
        assert_eq!(
            self_locate_at(expected.join("sirio.exe").as_path(), &expected),
            Ok(expected)
        );
    }

    #[test]
    fn self_locate_refuses_an_exe_outside_the_install_dir() {
        let expected = PathBuf::from(r"C:\Users\someone\AppData\Local\Programs\Sirio");
        let dev_build_exe = PathBuf::from(r"D:\projects\sirio\rust\target\debug\sirio.exe");
        assert_eq!(
            self_locate_at(&dev_build_exe, &expected),
            Err(ApplyError::InstallNotFound)
        );
    }

    #[test]
    fn apply_runs_the_installer_with_verysilent_and_norestart() {
        let captured_path = std::cell::RefCell::new(None);
        let captured_args = std::cell::RefCell::new(Vec::new());
        // Reborrow so the `move` closure copies references instead of
        // moving the RefCells themselves; the asserts below still own them.
        let path_cell = &captured_path;
        let args_cell = &captured_args;
        let staged = r"C:\staging\sirio-update-0.7.0-windows-x86_64";
        let result = launch(&update(staged), &move |path, args| {
            *path_cell.borrow_mut() = Some(path.to_path_buf());
            *args_cell.borrow_mut() = args.iter().map(|s| s.to_string()).collect();
            Ok(())
        });
        assert_eq!(result, Ok(()));
        assert_eq!(captured_path.borrow().as_deref(), Some(Path::new(staged)));
        assert_eq!(
            *captured_args.borrow(),
            vec!["/VERYSILENT".to_string(), "/NORESTART".to_string()]
        );
    }

    #[test]
    fn apply_propagates_a_launch_failure() {
        let result = launch(
            &update(r"C:\staging\sirio-update-0.7.0-windows-x86_64"),
            &|_, _| Err("access denied".into()),
        );
        assert_eq!(result, Err(ApplyError::Launch("access denied".into())));
    }
}
