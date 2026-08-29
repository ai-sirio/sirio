//! Windows applying: self-location under `%LOCALAPPDATA%\Programs\Sirio` and
//! the silent installer re-run (#315).

use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use super::ApplyError;

/// The installer is a GUI-subsystem binary, so no console would flash even
/// without this; it is belt-and-braces for the updater never showing a
/// window of its own under any circumstances (`CREATE_NO_WINDOW`, 0x08000000).
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The directory a Windows install of Sirio lives in: the Inno
/// `DefaultDirName={localappdata}\Programs\Sirio` from `Scripts/build-inno.sh`
/// (#310). Anything else — a dev build, a copied binary — is not an install
/// the updater can update in place.
pub fn expected_install_dir(local_app_data: &Path) -> PathBuf {
    local_app_data.join("Programs").join("Sirio")
}

/// Pure self-location check: `sirio.exe` must be running directly out of the
/// expected install directory.
pub fn self_locate_at(exe_path: &Path, expected_dir: &Path) -> Result<PathBuf, ApplyError> {
    let exe_dir = exe_path.parent().ok_or(ApplyError::InstallNotFound)?;
    if exe_dir != expected_dir {
        return Err(ApplyError::InstallNotFound);
    }
    Ok(expected_dir.to_path_buf())
}

/// Self-location for the real process. Refusing is the healthy outcome for a
/// dev build, so no fallback path guessing happens here — the download-page
/// pointer IS the fallback ([spec §5.1]).
///
/// [spec §5.1]: https://github.com/ai-sirio/sirio/blob/main/docs/superpowers/specs/2026-08-29-auto-update-design.md
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
pub fn spawn_silent(path: &Path, args: &[&str]) -> Result<Child, String> {
    use std::os::windows::process::CommandExt;
    Command::new(path)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_extensionless_pe_runs_with_the_silent_launcher() {
        // `sirio_update` stages the artifact as `sirio-update-<version>
        // -<platform>` — no `.exe`. This ticket should not take on faith that
        // a running `.exe`-shaped PE launches without its extension, so copy
        // a real PE to an extensionless name and run it through the exact
        // launcher the updater uses.
        let staged = std::env::temp_dir().join(format!("sirio-apply-pe-{}", std::process::id()));
        let _ = std::fs::remove_file(&staged);
        std::fs::copy(r"C:\Windows\System32\cmd.exe", &staged).unwrap();

        // Evidence via exit code, not stdout: the launcher spawns with
        // inherited handles (a GUI installer needs none), so whatever the
        // child prints lands in the harness's stream, not a pipe we control.
        // An extensionless `cmd.exe /C exit 7` returning 7 proves both that
        // CreateProcessW launched the PE by content, not name, and that its
        // arguments were honored.
        let mut child = spawn_silent(&staged, &["/C", "exit", "7"]).unwrap();
        let status = child.wait().unwrap();
        let _ = std::fs::remove_file(&staged);

        assert_eq!(
            status.code(),
            Some(7),
            "extensionless PE did not run cmd.exe /C exit 7: {status:?}"
        );
    }
}
