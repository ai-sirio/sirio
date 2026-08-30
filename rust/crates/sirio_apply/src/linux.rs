//! Linux applying: self-location via `$APPIMAGE` and the atomic rename (#314).
//!
//! A verified download is an AppImage, and on Linux an AppImage is swapped by
//! **replacing one file** ([spec §1.3]): the staged artifact is copied to a
//! sibling name in the target's own directory, made executable, then
//! `rename(2)`'d over `$APPIMAGE`. The rename is atomic exactly because the
//! incoming copy lands beside the target — a cross-device rename would be a
//! copy, and a copy is the thing this design refuses to let touch the live
//! file ([spec §5.2]).
//!
//! The rename is the only destructive step and it is atomic, so rollback
//! costs nothing: any failure before it leaves the original untouched, and
//! the leftover `.incoming` sibling is cleared by the next successful apply
//! (the file-rename analogue of macOS's move-aside cleanups). The app is
//! **not** restarted: the swap applies and the running instance carries on
//! until the user relaunches.
//!
//! Self-location reads `$APPIMAGE`, never `current_exe()`: inside an AppImage
//! the executable resolves into the ephemeral `/tmp/.mount_SirioXXXXXX`
//! squashfs, which is not the file to replace and disappears when the process
//! exits. `$APPIMAGE` is the only handle on the real file, and **its absence
//! is the "this is not an install" signal** on this platform — refused with
//! the download-page pointer, never guessed ([spec §5.1]). That is also the
//! permanent state of a `cargo run` dev build, so the refusal path is
//! exercised constantly.
//!
//! Everything here is testable without Linux: the whole sequence runs on the
//! real filesystem, so tests drive it against a temp directory on any
//! platform. Only the executable bit is Unix-specific — `make_executable` is
//! a no-op elsewhere, mirroring how `windows::spawn_silent` stubs its
//! platform-only primitive.
//!
//! [spec §1.3]: https://github.com/ai-sirio/sirio/blob/main/docs/superpowers/specs/2026-08-29-auto-update-design.md
//! [spec §5.1]: https://github.com/ai-sirio/sirio/blob/main/docs/superpowers/specs/2026-08-29-auto-update-design.md
//! [spec §5.2]: https://github.com/ai-sirio/sirio/blob/main/docs/superpowers/specs/2026-08-29-auto-update-design.md

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use super::ApplyError;
use sirio_update::VerifiedUpdate;

/// The whole Linux applying sequence: self-location first (refusing is the
/// healthy outcome when this is not an install, and nothing is touched
/// then), then the file swap.
pub fn apply(update: &VerifiedUpdate) -> Result<(), ApplyError> {
    let target = self_locate()?;
    // Fail fast when `$APPIMAGE` does not actually name a regular file (a
    // stray value is not an install either).
    if !target.is_file() {
        return Err(ApplyError::InstallNotFound);
    }
    apply_at(update, &target)
}

/// Self-location for the real process: `$APPIMAGE` is set by the AppImage
/// runtime to the path of the running image.
fn self_locate() -> Result<PathBuf, ApplyError> {
    self_locate_at(std::env::var_os("APPIMAGE").as_deref())
}

/// Pure self-location: `$APPIMAGE` names the running AppImage, or the update
/// is refused (a dev build, a bare binary — not an install). Refusing is the
/// healthy outcome, so no fallback path guessing happens here; the
/// download-page pointer IS the fallback.
fn self_locate_at(appimage: Option<&OsStr>) -> Result<PathBuf, ApplyError> {
    appimage.map(PathBuf::from).ok_or(ApplyError::InstallNotFound)
}

/// The swap itself. `target` is the live AppImage (`$APPIMAGE`); the verified
/// artifact is copied to a sibling name in the same directory first, then
/// only atomic renames touch the live path: an interruption half-way through
/// the copy corrupts `incoming`, never the file the user launches.
fn apply_at(update: &VerifiedUpdate, target: &Path) -> Result<(), ApplyError> {
    let incoming = target.with_file_name(format!(
        "{}.incoming",
        target.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()
    ));
    // Leftovers from an interrupted earlier apply are redundant — the file at
    // `target` is the live one. Clear them so the rename below is
    // collision-free; a failure surfaces before the live path is touched.
    if incoming.exists() {
        fs::remove_file(&incoming).map_err(|error| {
            ApplyError::Swap(format!(
                "could not clear the stale incoming copy at {incoming:?}: {error}"
            ))
        })?;
    }

    let result = (|| -> Result<(), String> {
        fs::copy(&update.path, &incoming).map_err(|error| error.to_string())?;
        make_executable(&incoming)?;
        // Never rename a non-executable file over the live one: the swap is
        // refused until the incoming copy is actually executable.
        if !is_executable(&incoming) {
            return Err(format!(
                "the staged copy at {} is not executable",
                incoming.display()
            ));
        }
        fs::rename(&incoming, target).map_err(|error| error.to_string())
    })();
    if let Err(error) = result {
        // The rename is the only destructive step and it is atomic, so the
        // original is untouched whatever failed before it; the partial
        // incoming copy is redundant, drop it.
        let _ = fs::remove_file(&incoming);
        return Err(ApplyError::Swap(format!(
            "could not replace the running AppImage: {error}"
        )));
    }
    Ok(())
}

/// Make a file executable. On Unix this is `chmod 755`; other platforms have
/// no executable bit, and the dispatch never routes here anyway, so it is a
/// no-op that keeps the sequence and its tests running everywhere (the same
/// shape as `windows::spawn_silent`'s off-platform stub).
#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Whether a file carries an executable bit. Meaningful only on Unix; on
/// other platforms the check is vacuously true.
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|meta| meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("sirio-apply-linux-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn update(path: &Path) -> VerifiedUpdate {
        VerifiedUpdate {
            version: "0.7.0".into(),
            notes: "notes".into(),
            path: path.to_path_buf(),
            platform: "linux-x86_64".into(),
        }
    }

    fn appimage_fixture(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = temp_dir(name);
        let appimage = root.join("Sirio-0.6.0-x86_64.AppImage");
        fs::write(&appimage, "old").unwrap();
        let staged = root.join("sirio-update-0.7.0-linux-x86_64");
        fs::write(&staged, "new").unwrap();
        (root, appimage, staged)
    }

    #[test]
    fn self_location_accepts_the_appimage_environment_variable() {
        let appimage = Path::new("/home/someone/Downloads/Sirio-0.6.0-x86_64.AppImage");
        assert_eq!(
            self_locate_at(Some(appimage.as_os_str())).unwrap(),
            appimage
        );
    }

    #[test]
    fn missing_appimage_is_the_not_an_install_signal_and_points_at_the_download_page() {
        let error = self_locate_at(None).unwrap_err();
        assert_eq!(error, ApplyError::InstallNotFound);
        assert!(error.to_string().contains(super::super::DOWNLOAD_PAGE_URL));
    }

    #[test]
    fn a_verified_download_replaces_the_appimage_and_leaves_no_incoming_copy() {
        let (_root, appimage, staged) = appimage_fixture("swap");

        apply_at(&update(&staged), &appimage).unwrap();

        assert_eq!(fs::read_to_string(&appimage).unwrap(), "new");
        assert!(!appimage.with_file_name("Sirio-0.6.0-x86_64.AppImage.incoming").exists());
    }

    #[test]
    fn a_failure_before_the_rename_leaves_the_original_appimage_untouched() {
        let (_root, appimage, _staged) = appimage_fixture("failure");
        let missing = _root.join("sirio-update-0.8.0-linux-x86_64");

        let error = apply_at(&update(&missing), &appimage).unwrap_err();

        assert!(matches!(error, ApplyError::Swap(_)), "{error:?}");
        assert_eq!(
            fs::read_to_string(&appimage).unwrap(),
            "old",
            "a failed copy must never touch the live AppImage"
        );
        assert!(
            !appimage.with_file_name("Sirio-0.6.0-x86_64.AppImage.incoming").exists(),
            "the partial incoming copy must be cleaned up"
        );
    }

    #[test]
    fn leftovers_from_an_interrupted_apply_do_not_block_the_next_one() {
        let (_root, appimage, staged) = appimage_fixture("stale");
        // A previous run died between the copy and the rename.
        fs::write(
            appimage.with_file_name("Sirio-0.6.0-x86_64.AppImage.incoming"),
            "half-written",
        )
        .unwrap();

        apply_at(&update(&staged), &appimage).unwrap();

        assert_eq!(fs::read_to_string(&appimage).unwrap(), "new");
        assert!(
            !appimage.with_file_name("Sirio-0.6.0-x86_64.AppImage.incoming").exists()
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_replacement_is_executable() {
        use std::os::unix::fs::PermissionsExt;
        let (_root, appimage, staged) = appimage_fixture("executable");

        apply_at(&update(&staged), &appimage).unwrap();

        let mode = fs::metadata(&appimage).unwrap().permissions().mode();
        assert_ne!(mode & 0o111, 0, "the swapped-in AppImage must be executable");
    }
}