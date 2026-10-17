//! `sirioctl` on the user's PATH, at startup and from the Settings button (spec
//! §5.2). The link names the stable copy, so it keeps working when the app is
//! quit and after every update.
//!
//! Where "on PATH" lands differs by platform, and `link_on_path` is the one
//! place that knows: `~/.local/bin` (Linux; on macOS when `/usr/local/bin` is
//! not writable without a password), `/usr/local/bin` through the administrator's
//! prompt on request (macOS), or the stable copy's directory on the user's `Path`
//! (Windows). Something Sirio did not make at a link's place is never replaced.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use crate::cli_install::sirioctl_binary_name;

/// What `link_on_path` did. The Settings card says each case in words. Not every
/// platform's arm builds every case, but the card reads all of them.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinkOutcome {
    /// Made now. On Windows, the stable copy's directory joined the user's `Path`.
    Linked(PathBuf),
    /// Already there, pointing at the stable copy (Windows: already on `Path`).
    AlreadyLinked(PathBuf),
    /// Something Sirio did not make holds the place, and it was left alone.
    /// `points_to` is its target when it is a link.
    Foreign {
        place: PathBuf,
        points_to: Option<PathBuf>,
    },
    /// The link is there, but its directory is not on the login PATH, so a new
    /// shell will not find it. Reported, never shown as success.
    NotOnLoginPath(PathBuf),
    /// macOS: no directory is writable without a password, and `~/.local/bin` is
    /// not on the login PATH. The Settings button asks for the administrator.
    NeedsAdmin,
    /// Not attempted, and why.
    Skipped(&'static str),
    /// Attempted and failed, and why.
    Failed(String),
}

/// The login shell's PATH, as `main` read it once. The startup link and the
/// Settings button both need it, and reading it again would run a login shell.
static LOGIN_PATH: OnceLock<Option<OsString>> = OnceLock::new();

/// Records the login shell's PATH for this process. `main` calls it once, after
/// it has read the login PATH; a second call is ignored.
pub(crate) fn set_login_path(login: Option<OsString>) {
    let _ = LOGIN_PATH.set(login);
}

/// The login shell's PATH as `set_login_path` recorded it; `None` when none was
/// recorded or the shell printed none.
pub(crate) fn recorded_login_path() -> Option<&'static OsStr> {
    LOGIN_PATH.get().and_then(|login| login.as_deref())
}

/// Puts `sirioctl` on the user's PATH: a link to `stable` where this platform
/// searches (see the module doc), or on Windows the stable copy's directory on
/// the user's `Path`.
///
/// `login_path` is the login shell's PATH. `None` (the shell could not be read)
/// counts as not on it, so the outcome says so rather than claiming success.
/// `admin` asks macOS for the administrator's password when no directory is
/// writable; no other platform reads it.
#[cfg(target_os = "macos")]
pub(crate) fn link_on_path(
    stable: &Path,
    home: &Path,
    login_path: Option<&OsStr>,
    admin: bool,
) -> LinkOutcome {
    link_on_macos(stable, home, login_path, admin, Path::new(SYSTEM_BIN_DIR))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn link_on_path(
    stable: &Path,
    home: &Path,
    login_path: Option<&OsStr>,
    _admin: bool,
) -> LinkOutcome {
    link_in_user_bin(stable, home, login_path)
}

#[cfg(windows)]
pub(crate) fn link_on_path(
    stable: &Path,
    _home: &Path,
    _login_path: Option<&OsStr>,
    _admin: bool,
) -> LinkOutcome {
    link_on_windows_path(stable)
}

/// The directory macOS puts `sirioctl` in when it may: Homebrew on Intel makes
/// it writable, and VS Code's `code` command goes there too.
#[cfg(any(target_os = "macos", test))]
const SYSTEM_BIN_DIR: &str = "/usr/local/bin";

/// `~/.local/bin/sirioctl`, the user's own directory. A link there always works
/// for the user; a new shell finds it only when the login PATH searches the
/// directory, which is reported when it does not.
#[cfg(unix)]
fn link_in_user_bin(stable: &Path, home: &Path, login_path: Option<&OsStr>) -> LinkOutcome {
    if !home.is_absolute() {
        return LinkOutcome::Skipped("no home directory");
    }
    let dir = home.join(".local/bin");
    let place = dir.join(sirioctl_binary_name());
    let made = match inspect_place(&place, stable, sirio_data_root(stable)) {
        Err(outcome) => return outcome,
        Ok(Place::Current) => false,
        Ok(Place::Free | Place::Replaceable) => {
            if let Err(outcome) = write_link(&place, stable) {
                return outcome;
            }
            true
        }
    };
    settle(&dir, place, made, login_path)
}

/// The outcome of a link that is in place: `Linked` or `AlreadyLinked`, unless
/// the login PATH does not search its directory, which is `NotOnLoginPath`.
/// Every platform's success goes through here, so no unsearched link is ever
/// reported as success.
#[cfg(unix)]
fn settle(dir: &Path, place: PathBuf, made: bool, login_path: Option<&OsStr>) -> LinkOutcome {
    match (made, on_login_path(dir, login_path)) {
        (true, true) => LinkOutcome::Linked(place),
        (false, true) => LinkOutcome::AlreadyLinked(place),
        (_, false) => LinkOutcome::NotOnLoginPath(place),
    }
}

/// `<data>/Sirio`, which holds the stable copy at `bin/sirioctl`. A link into it
/// is Sirio's own, whichever copy it names now: an update moves the copy, and
/// the link has to follow.
#[cfg(unix)]
fn sirio_data_root(stable: &Path) -> Option<&Path> {
    stable.parent()?.parent()
}

/// What holds a link's place before anything is written there.
#[cfg(unix)]
enum Place {
    /// Nothing is there.
    Free,
    /// A link to the stable copy.
    Current,
    /// A link into Sirio's own data that names another copy, or nothing.
    Replaceable,
}

/// Looks at `place` without changing it. A regular file, a directory, or a link
/// outside Sirio's data is someone else's, and comes back as `Foreign`.
#[cfg(unix)]
fn inspect_place(
    place: &Path,
    stable: &Path,
    sirio_root: Option<&Path>,
) -> Result<Place, LinkOutcome> {
    let metadata = match std::fs::symlink_metadata(place) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Place::Free),
        Err(error) => {
            return Err(LinkOutcome::Failed(format!(
                "could not look at {}: {error}",
                place.display()
            )));
        }
    };
    if !metadata.file_type().is_symlink() {
        return Err(LinkOutcome::Foreign {
            place: place.to_path_buf(),
            points_to: None,
        });
    }
    // A link whose target cannot be read is treated as foreign: it is not
    // provably Sirio's.
    let target = std::fs::read_link(place).ok();
    if target.as_deref() == Some(stable) {
        return Ok(Place::Current);
    }
    if target
        .as_deref()
        .zip(sirio_root)
        .is_some_and(|(target, root)| target.starts_with(root))
    {
        return Ok(Place::Replaceable);
    }
    Err(LinkOutcome::Foreign {
        place: place.to_path_buf(),
        points_to: target,
    })
}

/// Makes `place` a link to `stable`. The link is made beside `place` and renamed
/// over it, so a link already there is replaced whole, never removed first.
#[cfg(unix)]
fn write_link(place: &Path, stable: &Path) -> Result<(), LinkOutcome> {
    let failed = |error: std::io::Error| {
        LinkOutcome::Failed(format!("could not link {}: {error}", place.display()))
    };
    let Some(dir) = place.parent() else {
        return Err(LinkOutcome::Failed(format!(
            "{} has no directory",
            place.display()
        )));
    };
    std::fs::create_dir_all(dir).map_err(failed)?;
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = dir.join(format!(
        ".sirioctl-link-{}-{sequence}.tmp",
        std::process::id()
    ));
    std::os::unix::fs::symlink(stable, &temporary).map_err(failed)?;
    std::fs::rename(&temporary, place).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        failed(error)
    })
}

/// Distinguishes the temporary links of calls in one process: the startup link
/// and a Settings click can run at once.
#[cfg(unix)]
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Whether the login PATH searches `dir`. No login PATH (the shell could not be
/// read) searches nothing.
#[cfg(unix)]
fn on_login_path(dir: &Path, login_path: Option<&OsStr>) -> bool {
    login_path.is_some_and(|login| std::env::split_paths(login).any(|entry| entry.as_path() == dir))
}

/// macOS: `/usr/local/bin` when it is writable without a password, else
/// `~/.local/bin` when the login PATH searches it, else `NeedsAdmin`. With
/// `admin`, the directory is written through the administrator's prompt instead.
/// `system_bin` is a parameter so the policy can be tested against a stand-in.
#[cfg(target_os = "macos")]
fn link_on_macos(
    stable: &Path,
    home: &Path,
    login_path: Option<&OsStr>,
    admin: bool,
    system_bin: &Path,
) -> LinkOutcome {
    let place = system_bin.join(sirioctl_binary_name());
    // The system place decides first: a file that is not Sirio's stays the
    // answer, whatever the user directory holds.
    match inspect_place(&place, stable, sirio_data_root(stable)) {
        Err(outcome) => return outcome,
        Ok(Place::Current) => return settle(system_bin, place, false, login_path),
        Ok(_) if is_writable_dir(system_bin) => {
            return match write_link(&place, stable) {
                Ok(()) => settle(system_bin, place, true, login_path),
                Err(outcome) => outcome,
            };
        }
        Ok(_) => {}
    }
    if admin {
        return link_with_administrator(stable, login_path);
    }
    if on_login_path(&home.join(".local/bin"), login_path) {
        return link_in_user_bin(stable, home, login_path);
    }
    LinkOutcome::NeedsAdmin
}

/// Whether `dir` exists and this user may write into it without a prompt.
#[cfg(target_os = "macos")]
fn is_writable_dir(dir: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;

    let Ok(path) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `path` is NUL-terminated and lives across the call, and `access`
    // only reads it.
    dir.is_dir() && unsafe { libc::access(path.as_ptr(), libc::W_OK) } == 0
}

/// Makes `/usr/local/bin/sirioctl` through macOS's administrator prompt. The
/// call returns once the prompt is answered, so the caller runs it off the UI
/// thread.
#[cfg(target_os = "macos")]
fn link_with_administrator(stable: &Path, login_path: Option<&OsStr>) -> LinkOutcome {
    if stable.to_str().is_none() {
        return LinkOutcome::Failed(format!(
            "{} is not UTF-8, so it cannot be linked as administrator",
            stable.display()
        ));
    }
    let system_bin = Path::new(SYSTEM_BIN_DIR);
    let place = system_bin.join(sirioctl_binary_name());
    let script = admin_link_script(stable);
    match sirio_env::command("osascript")
        .args(["-e", script.as_str()])
        .output()
    {
        Ok(output) if output.status.success() => settle(system_bin, place, true, login_path),
        Ok(output) => LinkOutcome::Failed(format!(
            "the administrator link was not made: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Err(error) => LinkOutcome::Failed(format!("could not run osascript: {error}")),
    }
}

/// The AppleScript that makes `/usr/local/bin/sirioctl` a link to `stable` with
/// the administrator's rights. The path is quoted for the shell first (single
/// quotes, a `'` closed, escaped and reopened), then the whole shell command is
/// escaped for the AppleScript string that carries it (backslash and double
/// quote).
#[cfg(any(target_os = "macos", test))]
fn admin_link_script(stable: &Path) -> String {
    let shell_path = stable.to_string_lossy().replace('\'', r"'\''");
    let command =
        format!("mkdir -p {SYSTEM_BIN_DIR} && ln -sfn '{shell_path}' {SYSTEM_BIN_DIR}/sirioctl");
    let applescript = command.replace('\\', r"\\").replace('"', r#"\""#);
    format!(r#"do shell script "{applescript}" with administrator privileges"#)
}

/// Windows: the stable copy's directory joins the user's `Path` (`HKCU`, so no
/// administrator), and the change is broadcast so new shells see it. Inno's
/// uninstaller takes the entry out again.
#[cfg(windows)]
fn link_on_windows_path(stable: &Path) -> LinkOutcome {
    let Some(dir) = stable.parent() else {
        return LinkOutcome::Failed(format!(
            "{} has no directory to add to PATH",
            stable.display()
        ));
    };
    match add_to_user_path(&dir.to_string_lossy()) {
        Ok(true) => LinkOutcome::Linked(dir.to_path_buf()),
        Ok(false) => LinkOutcome::AlreadyLinked(dir.to_path_buf()),
        Err(reason) => LinkOutcome::Failed(reason),
    }
}

/// Appends `entry` to the user's `Path` unless it is there already, and returns
/// whether it appended. The value is written back as `REG_EXPAND_SZ`, its
/// `%VARIABLES%` unexpanded, so the entries the user has keep their meaning.
#[cfg(windows)]
fn add_to_user_path(entry: &str) -> Result<bool, String> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, RegCloseKey, RegOpenKeyExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
    };

    let subkey = wide("Environment");
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: `subkey` is NUL-terminated and lives across the call; `key`
    // receives the handle it opens.
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            &mut key,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("could not open HKCU\\Environment: error {status}"));
    }
    let appended = update_user_path(key, entry);
    // SAFETY: `key` was opened above, and this is its one close.
    unsafe { RegCloseKey(key) };
    let appended = appended?;
    if appended {
        let setting = wide("Environment");
        let mut reply = 0usize;
        // SAFETY: `setting` is NUL-terminated and outlives the call, which
        // waits at most five seconds per window. A failed broadcast leaves the
        // value written; shells started later see it either way.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                setting.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                5000,
                &mut reply,
            )
        };
    }
    Ok(appended)
}

/// Reads `Path` from the open key and writes `entry` onto its end when it is not
/// there. Returns whether it wrote.
#[cfg(windows)]
fn update_user_path(
    key: windows_sys::Win32::System::Registry::HKEY,
    entry: &str,
) -> Result<bool, String> {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{REG_EXPAND_SZ, RegQueryValueExW, RegSetValueExW};

    let name = wide("Path");
    let mut size = 0u32;
    // The first call sizes the value. A `Path` that is not there is an empty one.
    // SAFETY: a null buffer with a size pointer asks only for the size.
    let status = unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    let current = match status {
        ERROR_FILE_NOT_FOUND => String::new(),
        ERROR_SUCCESS if size == 0 => String::new(),
        ERROR_SUCCESS => {
            let mut data = vec![0u16; (size as usize).div_ceil(2)];
            // SAFETY: `data` holds `size` bytes, and `size` is updated to the
            // bytes written.
            let status = unsafe {
                RegQueryValueExW(
                    key,
                    name.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    data.as_mut_ptr().cast(),
                    &mut size,
                )
            };
            if status != ERROR_SUCCESS {
                return Err(format!("could not read the user's Path: error {status}"));
            }
            String::from_utf16_lossy(&data[..(size as usize / 2).min(data.len())])
                .trim_end_matches('\0')
                .to_string()
        }
        other => return Err(format!("could not read the user's Path: error {other}")),
    };
    if current.split(';').any(|item| same_directory(item, entry)) {
        return Ok(false);
    }
    let value = if current.is_empty() {
        entry.to_string()
    } else {
        format!("{current};{entry}")
    };
    let data = wide(&value);
    // SAFETY: `data` is NUL-terminated and `size` in bytes covers all of it.
    let status = unsafe {
        RegSetValueExW(
            key,
            name.as_ptr(),
            0,
            REG_EXPAND_SZ,
            data.as_ptr().cast(),
            (data.len() * 2) as u32,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("could not write the user's Path: error {status}"));
    }
    Ok(true)
}

/// Windows paths compare without case and without a trailing separator.
#[cfg(windows)]
fn same_directory(left: &str, right: &str) -> bool {
    left.trim_end_matches('\\')
        .eq_ignore_ascii_case(right.trim_end_matches('\\'))
}

/// `text` as UTF-16 with a terminating NUL, the form the Windows API takes.
#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    /// A scratch root under the temp dir, named for its test and this process.
    #[cfg(unix)]
    fn fixture(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("sirio-cli-path-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create the fixture root");
        root
    }

    /// The stable copy at `<root>/data/Sirio/bin/sirioctl`, and a home directory.
    #[cfg(unix)]
    fn stable_and_home(root: &Path) -> (PathBuf, PathBuf) {
        let stable = root.join("data/Sirio/bin/sirioctl");
        std::fs::create_dir_all(stable.parent().expect("a parent")).expect("create the stable dir");
        std::fs::write(&stable, b"sirioctl").expect("write the stable copy");
        let home = root.join("home");
        std::fs::create_dir_all(&home).expect("create the home");
        (stable, home)
    }

    /// A login PATH that searches `~/.local/bin`, as a Linux login shell does.
    #[cfg(unix)]
    fn login_path_with_local_bin(home: &Path) -> OsString {
        std::env::join_paths([
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
            home.join(".local/bin"),
        ])
        .expect("join the login PATH")
    }

    #[cfg(unix)]
    #[test]
    fn links_into_local_bin_and_reports_it() {
        let root = fixture("links");
        let (stable, home) = stable_and_home(&root);
        let login = login_path_with_local_bin(&home);
        let place = home.join(".local/bin/sirioctl");

        let outcome = link_on_path(&stable, &home, Some(login.as_os_str()), false);

        assert_eq!(outcome, LinkOutcome::Linked(place.clone()));
        assert_eq!(
            std::fs::read_link(&place).expect("the link is there"),
            stable
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn an_existing_link_to_the_stable_copy_is_already_linked() {
        let root = fixture("already");
        let (stable, home) = stable_and_home(&root);
        let login = login_path_with_local_bin(&home);
        let place = home.join(".local/bin/sirioctl");
        std::fs::create_dir_all(place.parent().expect("a parent")).expect("create the bin dir");
        symlink(&stable, &place).expect("make the link");

        let outcome = link_on_path(&stable, &home, Some(login.as_os_str()), false);

        assert_eq!(outcome, LinkOutcome::AlreadyLinked(place.clone()));
        assert_eq!(
            std::fs::read_link(&place).expect("the link is there"),
            stable
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_foreign_file_is_left_alone_and_reported() {
        let root = fixture("foreign");
        let (stable, home) = stable_and_home(&root);
        let login = login_path_with_local_bin(&home);
        let place = home.join(".local/bin/sirioctl");
        std::fs::create_dir_all(place.parent().expect("a parent")).expect("create the bin dir");

        std::fs::write(&place, b"someone else's").expect("write a foreign file");
        assert_eq!(
            link_on_path(&stable, &home, Some(login.as_os_str()), false),
            LinkOutcome::Foreign {
                place: place.clone(),
                points_to: None,
            }
        );
        assert_eq!(
            std::fs::read(&place).expect("the foreign file is still there"),
            b"someone else's"
        );

        std::fs::remove_file(&place).expect("remove the file");
        let elsewhere = root.join("elsewhere/sirioctl");
        std::fs::create_dir_all(elsewhere.parent().expect("a parent")).expect("create elsewhere");
        std::fs::write(&elsewhere, b"another copy").expect("write the other copy");
        symlink(&elsewhere, &place).expect("make a foreign link");
        assert_eq!(
            link_on_path(&stable, &home, Some(login.as_os_str()), false),
            LinkOutcome::Foreign {
                place: place.clone(),
                points_to: Some(elsewhere.clone()),
            }
        );
        assert_eq!(
            std::fs::read_link(&place).expect("the foreign link is still there"),
            elsewhere
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn link_in_a_directory_off_the_login_path_is_reported() {
        let root = fixture("off-path");
        let (stable, home) = stable_and_home(&root);
        let login = OsString::from("/usr/bin:/bin");
        let place = home.join(".local/bin/sirioctl");

        let outcome = link_on_path(&stable, &home, Some(login.as_os_str()), false);

        assert_eq!(outcome, LinkOutcome::NotOnLoginPath(place.clone()));
        assert_eq!(
            std::fs::read_link(&place).expect("the link is made anyway"),
            stable
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_sirio_link_is_replaced() {
        let root = fixture("dangling");
        let (stable, home) = stable_and_home(&root);
        let login = login_path_with_local_bin(&home);
        let place = home.join(".local/bin/sirioctl");
        std::fs::create_dir_all(place.parent().expect("a parent")).expect("create the bin dir");
        // A copy from before an update, removed since: inside Sirio's own data.
        let removed = root.join("data/Sirio/bin-0.32/sirioctl");
        symlink(&removed, &place).expect("make the dangling link");
        assert!(!removed.exists());

        let outcome = link_on_path(&stable, &home, Some(login.as_os_str()), false);

        assert_eq!(outcome, LinkOutcome::Linked(place.clone()));
        assert_eq!(
            std::fs::read_link(&place).expect("the link now names the stable copy"),
            stable
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_never_asks_without_admin() {
        use std::os::unix::fs::PermissionsExt;

        let root = fixture("macos-admin");
        let (stable, home) = stable_and_home(&root);
        let system_bin = root.join("usr-local-bin");
        std::fs::create_dir_all(&system_bin).expect("create the stand-in");
        std::fs::set_permissions(&system_bin, std::fs::Permissions::from_mode(0o555))
            .expect("make the stand-in unwritable");
        // The login PATH does not search `~/.local/bin`, so there is no
        // fallback: the only answer is to ask, and asking is the button's job.
        let login = OsString::from("/usr/bin:/bin");

        let outcome = link_on_macos(&stable, &home, Some(login.as_os_str()), false, &system_bin);

        assert_eq!(outcome, LinkOutcome::NeedsAdmin);
        assert!(!system_bin.join("sirioctl").exists());
        std::fs::set_permissions(&system_bin, std::fs::Permissions::from_mode(0o755))
            .expect("restore the stand-in");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn admin_script_quotes_a_path_with_quotes_and_spaces() {
        let stable = Path::new(r#"/home/u/My "Data"/it's\bin/sirioctl"#);

        assert_eq!(
            admin_link_script(stable),
            r#"do shell script "mkdir -p /usr/local/bin && ln -sfn '/home/u/My \"Data\"/it'\\''s\\bin/sirioctl' /usr/local/bin/sirioctl" with administrator privileges"#
        );
    }
}
