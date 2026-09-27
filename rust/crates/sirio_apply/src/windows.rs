//! Windows applying: self-location under `%LOCALAPPDATA%\Programs\Sirio` and
//! the silent installer re-run (#315).
//!
//! The module compiles on every target so its tests run everywhere; the real
//! applying sequence and raw process spawn are `#[cfg(target_os = "windows")]`
//! — `launch()` remains available to the cross-platform unit tests.

use std::path::Path;
#[cfg(target_os = "windows")]
use std::path::PathBuf;

use super::{ApplyError, Launcher};
#[cfg(target_os = "windows")]
use super::{expected_install_dir, self_locate_at};
use sirio_update::VerifiedUpdate;

/// The installer is a GUI-subsystem binary, so no console would flash even
/// without this; it is belt-and-braces for the updater never showing a
/// window of its own under any circumstances (`CREATE_NO_WINDOW`, 0x08000000).
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Self-location for the real process. Refusing is the healthy outcome for a
/// dev build, so no fallback path guessing happens here — the download-page
/// pointer IS the fallback ([spec §5.1]).
///
/// [spec §5.1]: https://github.com/ai-sirio/sirio/blob/main/docs/superpowers/specs/2026-08-29-auto-update-design.md
#[cfg(target_os = "windows")]
pub fn self_locate() -> Result<PathBuf, ApplyError> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            // Normal interactive sessions always set LOCALAPPDATA; fall back
            // to the canonical derivation for hostile/odd environments.
            std::env::var_os("USERPROFILE")
                .map(|home| PathBuf::from(home).join("AppData").join("Local"))
        })
        .ok_or(ApplyError::InstallNotFound)?;
    let expected = expected_install_dir(&base);
    let exe = std::env::current_exe().map_err(|_| ApplyError::InstallNotFound)?;
    self_locate_at(&exe, &expected)
}

/// The whole Windows applying sequence: self-location first (refusing is the
/// healthy outcome when this is not an install, and nothing is launched
/// then), then the silent re-run.
#[cfg(target_os = "windows")]
pub fn apply(update: &VerifiedUpdate, run: &Launcher<'_>) -> Result<(), ApplyError> {
    self_locate()?;
    launch(update, run)
}

/// The launch half of the sequence: the staged installer with Inno's own
/// silent flags. The flags live here, in the module that owns the installer,
/// not in the shared dispatch.
#[cfg_attr(not(windows), allow(dead_code))] // exercised by cross-platform unit tests and the Windows-only applying path
fn launch(update: &VerifiedUpdate, run: &Launcher<'_>) -> Result<(), ApplyError> {
    run(&update.path, &["/VERYSILENT", "/NORESTART"]).map_err(ApplyError::Launch)
}

/// Start the update payload detached, silently, with no window.
///
/// Fire-and-forget on purpose: the Inno installer closes this process and
/// restarts Sirio as part of the install, so nothing here may wait on it, and
/// a helper that "applies the swap after exit" is exactly the second thing
/// that can fail after the app is already gone ([spec §5.2]). The staged path
/// (`VerifiedUpdate.path` from #311) carries no `.exe` extension; Windows
/// loads a PE by content, not name, which the test
/// `an_extensionless_pe_runs` pins down on a real machine.
///
/// [spec §5.2]: https://github.com/ai-sirio/sirio/blob/main/docs/superpowers/specs/2026-08-29-auto-update-design.md
#[cfg(target_os = "windows")]
pub fn spawn_silent(path: &Path, args: &[&str]) -> Result<std::process::Child, String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    Command::new(path)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| error.to_string())
}

/// Non-Windows builds compile the applying flow so its tests run everywhere,
/// but the dispatch never routes here, so the real spawn is unreachable.
#[cfg(not(target_os = "windows"))]
#[cfg_attr(not(windows), allow(dead_code))] // retained as the documented off-platform launcher shape
pub fn spawn_silent(_path: &Path, _args: &[&str]) -> Result<std::process::Child, String> {
    Err("the silent installer launcher only exists on Windows".into())
}
