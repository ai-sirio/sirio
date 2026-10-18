//! The one stable copy of `sirioctl` that hooks, the PATH link and every pane
//! name. It lives in the app's data directory and is kept current by content:
//! a copy whose bytes differ from the binary bundled beside the app is written
//! again, through a temporary file and a `rename`, so a hook running the old
//! copy keeps running it and a second Sirio racing the first leaves one whole
//! file.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use sirio_ui::settings::CliStatus;

use crate::cli_path::{LinkOutcome, link_on_path, recorded_login_path};

/// The control CLI's file name: `sirioctl.exe` on Windows, bare on unix.
///
/// The extension is part of the name here because this module does not
/// *search* for the binary the way a shell does: it stats a path it built
/// itself (a sibling of the app executable, or the stable copy), and a stat
/// needs the exact spelling the toolchain emitted. Cmd.exe would resolve a bare
/// `sirioctl` by appending `.EXE` from PATHEXT, and so does the PATH search in
/// `sirio_agents::find_executable_in_path`, but neither runs here.
///
/// One definition feeds both the sibling candidate and the stable copy, so the
/// two cannot disagree on which name to look for.
pub(crate) fn sirioctl_binary_name() -> &'static str {
    #[cfg(windows)]
    {
        "sirioctl.exe"
    }
    #[cfg(not(windows))]
    {
        "sirioctl"
    }
}

/// `<data>/Sirio/bin`, or `bin-debug` for a debug build's own copy. On unix
/// `<data>` is `XDG_DATA_HOME` when it is absolute, else `HOME/.local/share`. On
/// Windows it is `LOCALAPPDATA` alone, never `XDG_DATA_HOME`: the Inno uninstaller
/// removes `{localappdata}\Sirio\bin` from the user's Path, so this must be that
/// directory (ruling R15).
///
/// `None` when no base is absolute. There is deliberately no temp-directory
/// fallback: a copy in a temp directory is cleared by the cleaner, and a PATH
/// entry there is writable by anyone on the machine.
pub(crate) fn sirioctl_install_dir(
    environment: &BTreeMap<String, String>,
    debug: bool,
) -> Option<PathBuf> {
    let absolute = |name: &str| {
        environment
            .get(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    #[cfg(windows)]
    let data = absolute("LOCALAPPDATA")?;
    #[cfg(not(windows))]
    let data = absolute("XDG_DATA_HOME")
        .or_else(|| absolute("HOME").map(|home| home.join(".local/share")))?;
    Some(
        data.join("Sirio")
            .join(if debug { "bin-debug" } else { "bin" }),
    )
}

/// The copy user-global hooks name. They name the release copy from any build,
/// so a debug build's *Install Hooks* never points into a checkout.
pub(crate) fn release_sirioctl_path(environment: &BTreeMap<String, String>) -> Option<PathBuf> {
    sirioctl_install_dir(environment, false).map(|dir| dir.join(sirioctl_binary_name()))
}

/// Whether `dir` may lead a child's PATH. On unix it must be a directory this
/// user owns that no other user can write to: a file dropped there would
/// otherwise shadow a command in every pane. Windows checks only that it exists.
pub(crate) fn safe_path_prefix(dir: &Path) -> bool {
    // `symlink_metadata`, so a link is never a directory here: what a link
    // names can change under the check, and a PATH entry must not.
    let Ok(metadata) = std::fs::symlink_metadata(dir) else {
        return false;
    };
    if !metadata.is_dir() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: `getuid` takes no arguments and cannot fail.
        let uid = unsafe { libc::getuid() };
        metadata.uid() == uid && metadata.mode() & 0o022 == 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Creates the stable directory. A directory this call creates is `0755`; an
/// existing one keeps its mode.
pub(crate) fn create_install_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o755)
            .create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// `SIRIO_CLI_AS_RELEASE=1` makes a debug build keep the release copy, so the
/// copy and its PATH entry can be exercised as an installed user has them.
pub(crate) fn cli_as_release() -> bool {
    cfg!(debug_assertions) && std::env::var("SIRIO_CLI_AS_RELEASE").as_deref() == Ok("1")
}

/// Whether this process keeps its copy in `bin-debug`.
pub(crate) fn stable_is_debug() -> bool {
    cfg!(debug_assertions) && !cli_as_release()
}

/// What `ensure_stable_copy` did with the copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyOutcome {
    /// There was no copy, and the bundled binary was written.
    Installed,
    /// The copy was a different file, or not a plain executable file. It was
    /// replaced.
    Replaced,
    /// The copy already holds the bundled binary's bytes. Nothing was written.
    Current,
    /// No bundled binary was found beside the app or on PATH. A copy that is
    /// there is kept.
    SourceMissing,
    /// A copy exists and runs, but could not be replaced; the reason was logged.
    Stale,
}

/// Serializes this process's installs. The startup copy, a pane's first resolve
/// and Install Hooks can all reach `ensure_stable_copy` at once, and on Windows
/// two calls moving the copy aside would race each other.
static INSTALL: Mutex<()> = Mutex::new(());

/// Makes the stable copy current for this process and returns its path.
///
/// The source is the binary beside `current_exe`, else a PATH hit that is not
/// the destination itself. The bytes are compared; a copy that is not a plain
/// file, or lacks the executable bit, or differs, is replaced. A copy that
/// cannot be replaced but still runs is kept as `Stale`, and the reason is
/// logged.
pub(crate) fn ensure_stable_copy(
    current_exe: &Path,
    environment: &BTreeMap<String, String>,
    debug: bool,
) -> Result<(PathBuf, CopyOutcome), String> {
    let _install = INSTALL.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = sirioctl_install_dir(environment, debug).ok_or_else(|| {
        // Only the base `sirioctl_install_dir` reads on this platform counts.
        let reason = if cfg!(windows) {
            "sirioctl has no stable directory: LOCALAPPDATA is not an absolute path"
        } else {
            "sirioctl has no stable directory: neither XDG_DATA_HOME nor HOME is an absolute path"
        };
        reason.to_string()
    })?;
    let destination = dir.join(sirioctl_binary_name());
    let Some(source) = find_source(current_exe, environment, &destination) else {
        // Nothing to copy from, but a copy that is there still works.
        if is_executable_file(&destination) {
            return Ok((destination, CopyOutcome::SourceMissing));
        }
        return Err(format!(
            "sirioctl is unavailable: checked {} and PATH; build or install the control CLI before launching an agent",
            current_exe
                .parent()
                .map(|path| path.join(sirioctl_binary_name()).display().to_string())
                .unwrap_or_else(|| "the app executable directory".to_string())
        ));
    };
    create_install_dir(&dir)
        .map_err(|error| format!("could not create {}: {error}", dir.display()))?;
    if is_current(&source, &destination) {
        return Ok((destination, CopyOutcome::Current));
    }
    // `symlink_metadata` sees a dangling link too, which is a copy to replace.
    let existed = std::fs::symlink_metadata(&destination).is_ok();
    if let Err(reason) = replace_copy(&source, &destination) {
        if is_executable_file(&destination) {
            eprintln!("[sirioctl] {reason}; the copy that is there is kept");
            return Ok((destination, CopyOutcome::Stale));
        }
        return Err(reason);
    }
    let outcome = if existed {
        CopyOutcome::Replaced
    } else {
        CopyOutcome::Installed
    };
    Ok((destination, outcome))
}

/// The binary to copy: the one beside the app, else the first PATH hit. The
/// stable copy itself is never a source, so a PATH entry that names it cannot
/// make the copy copy itself.
fn find_source(
    current_exe: &Path,
    environment: &BTreeMap<String, String>,
    destination: &Path,
) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(parent) = current_exe.parent() {
        candidates.push(parent.join(sirioctl_binary_name()));
    }
    if let Some(path) = environment.get("PATH")
        && let Some(found) = sirio_agents::find_executable_in_path("sirioctl", OsStr::new(path))
    {
        candidates.push(found);
    }
    candidates
        .into_iter()
        .find(|candidate| is_executable_file(candidate) && !is_stable_copy(candidate, destination))
}

/// Whether `candidate` is the stable copy under another name: a regular file
/// that a path resolves to the destination's file.
fn is_stable_copy(candidate: &Path, destination: &Path) -> bool {
    let Ok(copy) = std::fs::symlink_metadata(destination) else {
        return false;
    };
    copy.file_type().is_file()
        && std::fs::canonicalize(candidate).ok() == std::fs::canonicalize(destination).ok()
}

/// Whether `destination` is already a plain executable file holding the bytes
/// of `source`. A link is never current, even one that resolves to them: the
/// stable copy is a file.
fn is_current(source: &Path, destination: &Path) -> bool {
    let Ok(copy) = std::fs::symlink_metadata(destination) else {
        return false;
    };
    if !copy.file_type().is_file() || !is_executable_file(destination) {
        return false;
    }
    let Ok(source_metadata) = std::fs::metadata(source) else {
        return false;
    };
    if source_metadata.len() != copy.len() {
        return false;
    }
    match (std::fs::read(source), std::fs::read(destination)) {
        (Ok(source_bytes), Ok(copy_bytes)) => source_bytes == copy_bytes,
        _ => false,
    }
}

/// Distinguishes the temporary and aside names of calls in one process.
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Writes `source` over `destination` through a temporary file in the same
/// directory, so no reader sees a partial copy: on unix a `rename` replaces the
/// file atomically and a reader of the old copy keeps reading it. Windows will
/// not overwrite a running `.exe`, so the old file is renamed aside first and
/// removed by a later call. If the install then fails, the old file is renamed
/// back, so the copy that ran stays where it was.
fn replace_copy(source: &Path, destination: &Path) -> Result<(), String> {
    let dir = destination.parent().ok_or_else(|| {
        format!(
            "sirioctl install path has no parent: {}",
            destination.display()
        )
    })?;
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = dir.join(format!(".sirioctl-{}-{sequence}.tmp", std::process::id()));
    #[cfg(windows)]
    remove_aside_copies(dir);
    let failed = |error: std::io::Error| {
        let _ = std::fs::remove_file(&temporary);
        format!(
            "could not install sirioctl at {}: {error}",
            destination.display()
        )
    };
    if let Err(error) = write_temporary(source, &temporary) {
        return Err(failed(error));
    }
    #[cfg(windows)]
    let aside = match move_aside(destination, dir, sequence) {
        Ok(aside) => aside,
        Err(error) => return Err(failed(error)),
    };
    if let Err(error) = std::fs::rename(&temporary, destination) {
        #[cfg(windows)]
        if let Some(aside) = &aside {
            let _ = std::fs::rename(aside, destination);
        }
        return Err(failed(error));
    }
    Ok(())
}

/// Copies `source` to `temporary`, executable.
fn write_temporary(source: &Path, temporary: &Path) -> std::io::Result<()> {
    std::fs::copy(source, temporary)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(temporary, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

/// Renames the copy out of the way, so Windows accepts the new one. `None` when
/// there was no copy to move: it was moved already.
#[cfg(windows)]
fn move_aside(destination: &Path, dir: &Path, sequence: u64) -> std::io::Result<Option<PathBuf>> {
    let aside = dir.join(format!(
        "sirioctl.old-{}-{sequence}.exe",
        std::process::id()
    ));
    match std::fs::rename(destination, &aside) {
        Ok(()) => Ok(Some(aside)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Removes the old copies a Windows replacement left aside. One still in use
/// cannot be removed, and is tried again on a later call.
#[cfg(windows)]
fn remove_aside_copies(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("sirioctl.old-") && name.ends_with(".exe") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

type CopyResult = Result<(PathBuf, CopyOutcome), String>;

/// The copy and the PATH link as the last job left them: the startup job's, then
/// each Install in PATH's, which replaces both. One lock, so a reader never pairs
/// one job's copy with another's link. `None` until the startup job has finished.
static CLI_OUTCOMES: Mutex<Option<(CopyResult, LinkOutcome)>> = Mutex::new(None);

fn store_outcomes(copy: CopyResult, link: LinkOutcome) {
    *CLI_OUTCOMES.lock().unwrap_or_else(PoisonError::into_inner) = Some((copy, link));
}

fn stored_outcomes() -> Option<(CopyResult, LinkOutcome)> {
    CLI_OUTCOMES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Makes the stable copy, then the PATH link to it, on a background thread at
/// startup, so no launch waits for either. The outcomes are kept for
/// `resolve_for_pane` and the Settings card. The login PATH the link checks is
/// the one `main` recorded before this call. The receiver resolves once the
/// outcomes are stored, so the card can ask for them then.
pub(crate) fn ensure_at_startup(
    current_exe: PathBuf,
    environment: BTreeMap<String, String>,
) -> futures::channel::oneshot::Receiver<()> {
    let (sender, started) = futures::channel::oneshot::channel();
    let spawned = std::thread::Builder::new()
        .name("sirio-cli-copy".into())
        .spawn(move || {
            let (outcome, link) = copy_and_link(&current_exe, &environment, false);
            store_outcomes(outcome, link);
            refresh_user_skills_at_startup(&environment);
            // The receiver is gone only if the app is, and then nothing is left to tell.
            let _ = sender.send(());
        });
    if let Err(error) = spawned {
        eprintln!("[sirio] sirioctl copy not started: {error}");
    }
    started
}

/// Refreshes the user-level skill copies Sirio already wrote, so an update that
/// changed the bundled skill reaches the user's agents too (spec §5.7). Only the
/// copies that exist are touched: a user who never clicked Install Skill gets
/// nothing created. A failure is logged here and never stops the launch.
fn refresh_user_skills_at_startup(environment: &BTreeMap<String, String>) {
    let Some((home, skill_environment)) = skill_home_and_environment(environment) else {
        eprintln!("[skill] user-level skills not refreshed: no home directory");
        return;
    };
    for report in sirio_agents::refresh_user_skills(&home, &skill_environment) {
        if report.outcome.is_err() {
            eprintln!("[skill] {}", report.summary_line());
        }
    }
}

/// The home directory and environment the user-level skills are written from,
/// read the way the startup link reads HOME: the user environment once the launch
/// recorded it, else the process's. Windows has no HOME, so USERPROFILE is read
/// there. `None` when no absolute home directory is known.
pub(crate) fn skill_home_and_environment(
    process: &BTreeMap<String, String>,
) -> Option<(PathBuf, BTreeMap<String, String>)> {
    let user = sirio_env::installed();
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let home = user
        .and_then(|user| user.get(key))
        .or_else(|| process.get(key))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())?;
    let environment = user.cloned().unwrap_or_else(|| process.clone());
    Some((home, environment))
}

/// Makes the stable copy, then the PATH link to it, and returns both. The copy's
/// failure and the link's unusual outcomes are logged here, once, for both jobs.
/// `admin` lets macOS ask for the administrator's password, which only Install in
/// PATH allows.
fn copy_and_link(
    current_exe: &Path,
    environment: &BTreeMap<String, String>,
    admin: bool,
) -> (CopyResult, LinkOutcome) {
    let copy = ensure_stable_copy(current_exe, environment, stable_is_debug());
    if let Err(error) = &copy {
        eprintln!("[sirio] sirioctl: {error}");
    }
    let link = link_copy(&copy, environment, admin);
    // `NeedsAdmin` is the usual state of a macOS user with no writable
    // directory on the login PATH: not a failure, so not logged each launch.
    if matches!(
        link,
        LinkOutcome::Failed(_) | LinkOutcome::Foreign { .. } | LinkOutcome::NotOnLoginPath(_)
    ) {
        eprintln!("[sirio] sirioctl on PATH: {link:?}");
    }
    (copy, link)
}

/// The link to the copy just made. A debug build keeps its own copy and leaves
/// the user's PATH alone.
fn link_copy(
    copy: &CopyResult,
    environment: &BTreeMap<String, String>,
    admin: bool,
) -> LinkOutcome {
    if stable_is_debug() {
        return LinkOutcome::Skipped("debug build");
    }
    if release_sirioctl_path(environment).is_none() {
        return LinkOutcome::Skipped("no data directory");
    }
    // The copy's failure is already logged by the startup thread; the link
    // does not repeat it.
    let Ok((stable, _)) = copy else {
        return LinkOutcome::Skipped("the copy failed");
    };
    // Only the unix arms read a home; the Windows arm names none, so a Windows
    // launch (which never sets HOME) reaches the link.
    #[cfg(unix)]
    let home = {
        let home = sirio_env::installed()
            .and_then(|user| user.get("HOME"))
            .or_else(|| environment.get("HOME"))
            .map(PathBuf::from)
            .filter(|home| home.is_absolute());
        match home {
            Some(home) => home,
            None => return LinkOutcome::Skipped("no home directory"),
        }
    };
    #[cfg(windows)]
    let home = PathBuf::new();
    link_on_path(stable, &home, recorded_login_path(), admin)
}

/// Settings → Install in PATH: makes the stable copy if it is missing, then the
/// PATH link with the administrator's prompt where macOS wants it, and keeps both
/// outcomes for `card_status`. Blocks, so the caller runs it on its own thread.
pub(crate) fn install_from_settings(current_exe: &Path, environment: &BTreeMap<String, String>) {
    let (copy, link) = copy_and_link(current_exe, environment, true);
    store_outcomes(copy, link);
}

/// The Settings card's words for sirioctl's copy and PATH link, from the stored
/// outcomes. `None` until the startup job has finished, so the card shows no CLI
/// rows before then.
pub(crate) fn card_status() -> Option<CliStatus> {
    let (copy, link) = stored_outcomes()?;
    Some(status_for(&copy, &link))
}

fn status_for(copy: &CopyResult, link: &LinkOutcome) -> CliStatus {
    let (stable_path, version_line) = match copy {
        Ok((path, outcome)) => (path.display().to_string(), version_words(*outcome)),
        Err(error) => (format!("Not installed: {error}"), String::new()),
    };
    CliStatus {
        stable_path,
        version_line,
        path_line: path_line(link),
    }
}

/// The copy's build in words. A copy kept beside no bundled binary was never
/// compared, so it does not claim a build.
fn version_words(outcome: CopyOutcome) -> String {
    String::from(match outcome {
        CopyOutcome::Installed | CopyOutcome::Replaced | CopyOutcome::Current => {
            "Same build as Sirio"
        }
        CopyOutcome::Stale => "Older than this Sirio — it could not be replaced",
        CopyOutcome::SourceMissing => "Not checked: Sirio has no sirioctl beside it",
    })
}

/// The card's line for the PATH link. Not every case is reachable on every
/// platform, but each one has its words.
fn path_line(link: &LinkOutcome) -> String {
    match link {
        LinkOutcome::Linked(place) | LinkOutcome::AlreadyLinked(place) => {
            format!("On PATH at {}", place.display())
        }
        LinkOutcome::NotOnLoginPath(place) => {
            let dir = place.parent().unwrap_or(place);
            format!(
                "Linked at {}, but your login shell does not search {}",
                place.display(),
                dir.display()
            )
        }
        LinkOutcome::Foreign {
            place,
            points_to: Some(target),
        } => format!(
            "{} is not Sirio's ({}) — left as it is",
            place.display(),
            target.display()
        ),
        LinkOutcome::Foreign {
            place,
            points_to: None,
        } => format!("{} is not Sirio's — left as it is", place.display()),
        LinkOutcome::NeedsAdmin => "Not on PATH — Install in PATH asks for your password".into(),
        LinkOutcome::Skipped(reason) => format!("Not installed: {reason}"),
        LinkOutcome::Failed(error) => format!("Could not link: {error}"),
    }
}

/// The stable copy a pane starts with. It reads no contents: once the startup
/// copy is `Ok` its path is named, and until then, or after it failed, a copy
/// that is there and executable is named as it is. Only with no executable copy
/// at all is one made here, which is the first launch.
pub(crate) fn resolve_for_pane(
    current_exe: &Path,
    environment: &BTreeMap<String, String>,
    debug: bool,
) -> Result<PathBuf, String> {
    if let Some((Ok((path, _)), _)) = stored_outcomes() {
        return Ok(path);
    }
    if let Some(stable) =
        sirioctl_install_dir(environment, debug).map(|dir| dir.join(sirioctl_binary_name()))
        && is_executable_file(&stable)
    {
        return Ok(stable);
    }
    ensure_stable_copy(current_exe, environment, debug).map(|(path, _)| path)
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE: AtomicU64 = AtomicU64::new(0);

    /// The variable that holds the data root here: `LOCALAPPDATA` on Windows,
    /// `XDG_DATA_HOME` elsewhere.
    #[cfg(windows)]
    const DATA_BASE: &str = "LOCALAPPDATA";
    #[cfg(not(windows))]
    const DATA_BASE: &str = "XDG_DATA_HOME";

    fn fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "sirio-cli-install-{name}-{}-{}",
            std::process::id(),
            FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create the fixture root");
        root
    }

    /// A data home under `root` and a PATH that holds no `sirioctl`.
    fn environment_under(root: &Path) -> BTreeMap<String, String> {
        BTreeMap::from([
            (
                DATA_BASE.to_string(),
                root.join("data").to_string_lossy().into_owned(),
            ),
            (
                "PATH".to_string(),
                root.join("no-path").to_string_lossy().into_owned(),
            ),
        ])
    }

    /// The release copy a test under `root` expects.
    fn stable_path(root: &Path) -> PathBuf {
        root.join("data")
            .join("Sirio")
            .join("bin")
            .join(sirioctl_binary_name())
    }

    /// An app executable with a `sirioctl` beside it holding `bytes`: the
    /// source the resolver finds first. Returns the app's path.
    fn app_beside(root: &Path, app: &str, bytes: &[u8]) -> PathBuf {
        let dir = root.join(app);
        std::fs::create_dir_all(&dir).expect("create the app fixture");
        let current_exe = dir.join("sirio");
        std::fs::write(&current_exe, b"sirio").expect("write the app fixture");
        std::fs::write(dir.join(sirioctl_binary_name()), bytes).expect("write the source");
        make_executable(&current_exe);
        make_executable(&dir.join(sirioctl_binary_name()));
        current_exe
    }

    fn write_stable(root: &Path, bytes: &[u8]) -> PathBuf {
        let stable = stable_path(root);
        std::fs::create_dir_all(stable.parent().expect("stable parent"))
            .expect("create the stable directory");
        std::fs::write(&stable, bytes).expect("write the existing copy");
        stable
    }

    fn set_modified(path: &Path, seconds: u64) {
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open the fixture for a timestamp")
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds)),
            )
            .expect("set the fixture timestamp");
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .expect("make the fixture executable");
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Path) {}

    #[test]
    fn no_absolute_base_gives_no_install_dir() {
        let relative = BTreeMap::from([
            ("XDG_DATA_HOME".to_string(), "relative/data".to_string()),
            ("HOME".to_string(), "relative/home".to_string()),
        ]);
        assert_eq!(sirioctl_install_dir(&relative, false), None);
        assert_eq!(sirioctl_install_dir(&BTreeMap::new(), true), None);
        assert_eq!(release_sirioctl_path(&BTreeMap::new()), None);
    }

    /// The Inno uninstaller removes `{localappdata}\Sirio\bin` from the user's
    /// Path, so the app's copy must sit there whatever `XDG_DATA_HOME` says.
    #[cfg(windows)]
    #[test]
    fn windows_install_dir_ignores_xdg_data_home() {
        let with_xdg = BTreeMap::from([
            ("XDG_DATA_HOME".to_string(), r"C:\xdg".to_string()),
            (
                "LOCALAPPDATA".to_string(),
                r"C:\Users\u\AppData\Local".to_string(),
            ),
        ]);
        assert_eq!(
            sirioctl_install_dir(&with_xdg, false),
            Some(PathBuf::from(r"C:\Users\u\AppData\Local\Sirio\bin"))
        );
        let xdg_only = BTreeMap::from([("XDG_DATA_HOME".to_string(), r"C:\xdg".to_string())]);
        assert_eq!(sirioctl_install_dir(&xdg_only, false), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_world_writable_dir_is_not_a_path_prefix() {
        use std::os::unix::fs::PermissionsExt;
        let root = fixture("world-writable");
        let dir = root.join("bin");
        std::fs::create_dir_all(&dir).expect("create the directory");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777))
            .expect("make it world-writable");
        assert!(!safe_path_prefix(&dir));
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o775))
            .expect("make it group-writable");
        assert!(!safe_path_prefix(&dir));
        assert!(!safe_path_prefix(&root.join("missing")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn a_dir_of_our_own_is_a_path_prefix() {
        use std::os::unix::fs::PermissionsExt;
        let root = fixture("own-dir");
        let dir = root.join("bin");
        std::fs::create_dir_all(&dir).expect("create the directory");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))
            .expect("make it ours");
        assert!(safe_path_prefix(&dir));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_dir_is_not_a_path_prefix() {
        use std::os::unix::fs::PermissionsExt;
        let root = fixture("symlinked-dir");
        let real = root.join("real");
        std::fs::create_dir_all(&real).expect("create the directory");
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755))
            .expect("make it ours");
        let link = root.join("link");
        std::os::unix::fs::symlink(&real, &link).expect("link the directory");
        assert!(safe_path_prefix(&real), "the directory itself is a prefix");
        assert!(
            !safe_path_prefix(&link),
            "a link to a directory is not a prefix: the link can be retargeted"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn different_bytes_are_replaced_even_when_the_copy_is_newer() {
        let root = fixture("newer-copy");
        let environment = environment_under(&root);
        // Equal lengths: a comparison by size alone would call them current.
        let current_exe = app_beside(&root, "app", b"new build!");
        let source = current_exe
            .parent()
            .expect("app dir")
            .join(sirioctl_binary_name());
        set_modified(&source, 1_000_000);
        let stable = write_stable(&root, b"old build!");
        make_executable(&stable);
        set_modified(&stable, 2_000_000);

        let (path, outcome) =
            ensure_stable_copy(&current_exe, &environment, false).expect("the copy is refreshed");
        assert_eq!(outcome, CopyOutcome::Replaced);
        assert_eq!(path, stable);
        assert_eq!(
            std::fs::read(&stable).expect("read the copy"),
            b"new build!"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn same_bytes_leave_the_copy_untouched() {
        let root = fixture("same-bytes");
        let environment = environment_under(&root);
        let current_exe = app_beside(&root, "app", b"same build");
        let source = current_exe
            .parent()
            .expect("app dir")
            .join(sirioctl_binary_name());
        ensure_stable_copy(&current_exe, &environment, false).expect("the first install");
        let stable = stable_path(&root);
        // The source is newer, so a timestamp check would rewrite the copy.
        set_modified(&stable, 1_000_000);
        set_modified(&source, 2_000_000);
        let before = std::fs::metadata(&stable).expect("copy metadata");

        let (_, outcome) =
            ensure_stable_copy(&current_exe, &environment, false).expect("the copy is current");
        assert_eq!(outcome, CopyOutcome::Current);
        let after = std::fs::metadata(&stable).expect("copy metadata");
        assert_eq!(
            after.modified().expect("mtime"),
            before.modified().expect("mtime")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(after.ino(), before.ino(), "the copy was not rewritten");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn same_bytes_without_the_executable_bit_are_replaced() {
        use std::os::unix::fs::PermissionsExt;
        let root = fixture("not-executable");
        let environment = environment_under(&root);
        let current_exe = app_beside(&root, "app", b"same build");
        let stable = write_stable(&root, b"same build");
        std::fs::set_permissions(&stable, std::fs::Permissions::from_mode(0o644))
            .expect("drop the executable bit");

        let (_, outcome) =
            ensure_stable_copy(&current_exe, &environment, false).expect("the copy is repaired");
        assert_eq!(outcome, CopyOutcome::Replaced);
        assert!(is_executable_file(&stable));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn a_reader_of_the_old_copy_keeps_reading_it() {
        use std::io::Read;
        let root = fixture("old-reader");
        let environment = environment_under(&root);
        let old = app_beside(&root, "old", b"old build");
        ensure_stable_copy(&old, &environment, false).expect("the first install");
        let mut reader = std::fs::File::open(stable_path(&root)).expect("open the old copy");

        let new = app_beside(&root, "new", b"new build");
        let (_, outcome) = ensure_stable_copy(&new, &environment, false).expect("the replacement");
        assert_eq!(outcome, CopyOutcome::Replaced);
        let mut old_bytes = Vec::new();
        reader
            .read_to_end(&mut old_bytes)
            .expect("read the old copy to its end");
        assert_eq!(old_bytes, b"old build");
        assert_eq!(
            std::fs::read(stable_path(&root)).expect("read the copy"),
            b"new build"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn two_concurrent_installs_leave_one_whole_file() {
        let root = fixture("concurrent");
        let environment = environment_under(&root);
        // Large enough that a write in place takes long enough to be seen half
        // done by a reader.
        const SIZE: usize = 8 << 20;
        let first_bytes = vec![b'a'; SIZE];
        let second_bytes = vec![b'b'; SIZE];
        let first = app_beside(&root, "first", &first_bytes);
        let second = app_beside(&root, "second", &second_bytes);
        let stable = stable_path(&root);
        let barrier = std::sync::Barrier::new(3);
        let finished = AtomicU64::new(0);
        std::thread::scope(|scope| {
            for exe in [&first, &second] {
                let (environment, barrier, finished) = (&environment, &barrier, &finished);
                scope.spawn(move || {
                    barrier.wait();
                    for _ in 0..12 {
                        ensure_stable_copy(exe, environment, false)
                            .expect("every concurrent install succeeds");
                    }
                    finished.fetch_add(1, Ordering::SeqCst);
                });
            }
            // Every read of the copy while the installs run is one source,
            // whole. Before the first install there is no copy to read.
            let (stable, barrier, finished) = (&stable, &barrier, &finished);
            let (first_bytes, second_bytes) = (&first_bytes, &second_bytes);
            scope.spawn(move || {
                barrier.wait();
                while finished.load(Ordering::SeqCst) < 2 {
                    match std::fs::read(stable) {
                        Ok(bytes) => assert!(
                            bytes == *first_bytes || bytes == *second_bytes,
                            "a reader saw a partial copy of {} bytes",
                            bytes.len()
                        ),
                        Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::NotFound),
                    }
                }
            });
        });
        let installed = std::fs::read(&stable).expect("read the copy");
        assert!(
            installed == first_bytes || installed == second_bytes,
            "the copy must be one source, whole"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_source_keeps_a_present_copy() {
        let root = fixture("missing-source");
        let environment = environment_under(&root);
        let stable = write_stable(&root, b"kept build");
        make_executable(&stable);

        let (path, outcome) = ensure_stable_copy(&root.join("nowhere/sirio"), &environment, false)
            .expect("a present copy is kept");
        assert_eq!(path, stable);
        assert_eq!(outcome, CopyOutcome::SourceMissing);
        assert_eq!(
            std::fs::read(&stable).expect("read the copy"),
            b"kept build"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_source_and_copy_is_an_error() {
        let root = fixture("missing-both");
        let environment = environment_under(&root);
        let error = ensure_stable_copy(&root.join("nowhere/sirio"), &environment, false)
            .expect_err("nothing to install from");
        assert!(error.contains("sirioctl is unavailable"), "got {error}");
        assert!(!stable_path(&root).exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn debug_and_release_copies_live_apart() {
        let root = fixture("debug-release");
        let environment = environment_under(&root);
        let release_exe = app_beside(&root, "release", b"release build");
        let debug_exe = app_beside(&root, "debug", b"debug build");

        let (release, _) =
            ensure_stable_copy(&release_exe, &environment, false).expect("the release copy");
        let (debug, _) =
            ensure_stable_copy(&debug_exe, &environment, true).expect("the debug copy");
        assert_eq!(release, stable_path(&root));
        assert_eq!(
            debug,
            root.join("data")
                .join("Sirio")
                .join("bin-debug")
                .join(sirioctl_binary_name())
        );
        assert_eq!(std::fs::read(&release).expect("read"), b"release build");
        assert_eq!(std::fs::read(&debug).expect("read"), b"debug build");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_foreign_file_at_the_stable_path_is_replaced() {
        // Spec §5.1: the stable path lies inside Sirio's own data directory,
        // so whatever sits there is Sirio's to replace.
        let root = fixture("foreign-file");
        let environment = environment_under(&root);
        let current_exe = app_beside(&root, "app", b"bundled build");
        write_stable(&root, b"not sirioctl at all");

        let (path, outcome) =
            ensure_stable_copy(&current_exe, &environment, false).expect("the copy is replaced");
        assert_eq!(outcome, CopyOutcome::Replaced);
        assert_eq!(std::fs::read(&path).expect("read"), b"bundled build");
        assert!(is_executable_file(&path));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_at_the_stable_path_becomes_a_copy() {
        // A link to a build with identical bytes is still not the copy: the
        // stable copy is a file, never a link into a checkout or a mount.
        let root = fixture("symlink-copy");
        let environment = environment_under(&root);
        let linked = root.join("target").join(sirioctl_binary_name());
        // A link's own length is its target's path length. The bytes are sized
        // to that length, so a check that reads the link itself cannot tell it
        // from a copy of the same bytes.
        let payload = vec![b'x'; linked.as_os_str().len()];
        let current_exe = app_beside(&root, "app", &payload);
        std::fs::create_dir_all(linked.parent().expect("target dir")).expect("create target");
        std::fs::write(&linked, &payload).expect("write the linked build");
        make_executable(&linked);
        std::fs::create_dir_all(stable_path(&root).parent().expect("stable dir"))
            .expect("create the stable directory");
        std::os::unix::fs::symlink(&linked, stable_path(&root)).expect("link the stable path");

        let (path, outcome) =
            ensure_stable_copy(&current_exe, &environment, false).expect("the link is replaced");
        assert_eq!(outcome, CopyOutcome::Replaced);
        assert!(
            std::fs::symlink_metadata(&path)
                .expect("the stable path")
                .file_type()
                .is_file(),
            "the stable copy must be a file, not a link"
        );
        assert_eq!(std::fs::read(&path).expect("read"), payload);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn user_global_hooks_name_the_release_path_from_a_debug_build() {
        // The release path does not depend on the build that writes the hooks:
        // a debug build names `bin`, never its own `bin-debug`.
        let data = std::env::temp_dir().join("sirio-cli-install-hooks-data");
        let environment =
            BTreeMap::from([(DATA_BASE.to_string(), data.to_string_lossy().into_owned())]);
        let release = release_sirioctl_path(&environment);
        assert_eq!(
            release,
            Some(data.join("Sirio").join("bin").join(sirioctl_binary_name()))
        );
        assert_eq!(
            release,
            sirioctl_install_dir(&environment, false).map(|dir| dir.join(sirioctl_binary_name()))
        );
        assert_ne!(
            release,
            sirioctl_install_dir(&environment, true).map(|dir| dir.join(sirioctl_binary_name()))
        );
    }

    /// The installed control CLI must carry the bytes of the artifact the
    /// resolver picked. The stable copy is a file on every platform.
    fn assert_copied_from(installed: &Path, source: &Path) {
        assert_eq!(
            std::fs::read(installed).expect("read the installed sirioctl"),
            std::fs::read(source).expect("read the source sirioctl"),
            "the installed sirioctl must carry the source artifact's bytes"
        );
        #[cfg(unix)]
        assert!(
            std::fs::symlink_metadata(installed)
                .expect("the installed sirioctl")
                .file_type()
                .is_file(),
            "the stable copy is a copy, never a symlink"
        );
    }

    #[test]
    fn sirioctl_resolver_installs_the_sibling_binary_in_xdg_data_bin() {
        // The fixture names the binary the way the platform ships it, and the
        // resolver has to look for that name.
        let root = fixture("sibling");
        let executable_dir = root.join("target/debug");
        std::fs::create_dir_all(&executable_dir).expect("create the executable fixture");
        let current_exe = executable_dir.join("sirio");
        let sirioctl = executable_dir.join(sirioctl_binary_name());
        std::fs::write(&current_exe, b"sirio").expect("write the app fixture");
        std::fs::write(&sirioctl, b"sirioctl").expect("write the sibling");
        make_executable(&current_exe);
        make_executable(&sirioctl);
        let environment = BTreeMap::from([
            (
                DATA_BASE.to_string(),
                root.join("data").to_string_lossy().into_owned(),
            ),
            (
                "PATH".to_string(),
                root.join("empty-path").to_string_lossy().into_owned(),
            ),
        ]);

        let (resolved, outcome) = ensure_stable_copy(&current_exe, &environment, false)
            .expect("the sibling sirioctl is installed");
        assert_eq!(resolved, stable_path(&root));
        assert!(resolved.is_absolute());
        assert_eq!(outcome, CopyOutcome::Installed);
        assert_copied_from(&resolved, &sirioctl);
        let _ = std::fs::remove_dir_all(root);
    }

    /// Windows regression: the build emits `sirioctl.exe`, and the resolver
    /// must look for exactly that name. An extensionless `sirioctl` next to
    /// the app is not a real Windows artifact and may not satisfy it.
    #[cfg(windows)]
    #[test]
    fn sirioctl_resolver_requires_the_exe_sibling_on_windows() {
        let root = fixture("exe-sibling");
        let executable_dir = root.join("target/debug");
        std::fs::create_dir_all(&executable_dir).expect("create the executable fixture");
        let current_exe = executable_dir.join("sirio.exe");
        std::fs::write(&current_exe, b"sirio").expect("write the app fixture");
        std::fs::write(executable_dir.join("sirioctl"), b"wrong artifact")
            .expect("write the extensionless decoy");
        std::fs::create_dir_all(root.join("empty-path")).expect("create the empty PATH dir");
        let environment = BTreeMap::from([
            (
                DATA_BASE.to_string(),
                root.join("data").to_string_lossy().into_owned(),
            ),
            (
                "PATH".to_string(),
                root.join("empty-path").to_string_lossy().into_owned(),
            ),
        ]);

        let error = ensure_stable_copy(&current_exe, &environment, false)
            .expect_err("an extensionless sirioctl must not satisfy the resolver");
        assert!(error.contains("sirioctl is unavailable"), "got {error}");

        let sirioctl = executable_dir.join("sirioctl.exe");
        std::fs::write(&sirioctl, b"sirioctl").expect("write the real artifact");
        let (resolved, _) = ensure_stable_copy(&current_exe, &environment, false)
            .expect("the .exe sibling must resolve");
        assert_eq!(
            resolved,
            root.join("data")
                .join("Sirio")
                .join("bin")
                .join("sirioctl.exe")
        );
        assert_copied_from(&resolved, &sirioctl);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn sirioctl_resolver_falls_back_to_path_and_reports_missing_binary() {
        let root = fixture("path-fallback");
        let executable_dir = root.join("app");
        let path_dir = root.join("path");
        std::fs::create_dir_all(&executable_dir).expect("create the app fixture");
        std::fs::create_dir_all(&path_dir).expect("create the PATH fixture");
        let current_exe = executable_dir.join("sirio");
        let path_sirioctl = path_dir.join(sirioctl_binary_name());
        std::fs::write(&current_exe, b"sirio").expect("write the app fixture");
        std::fs::write(&path_sirioctl, b"sirioctl").expect("write the PATH fixture");
        make_executable(&current_exe);
        make_executable(&path_sirioctl);

        let mut environment = BTreeMap::from([
            (
                DATA_BASE.to_string(),
                root.join("data").to_string_lossy().into_owned(),
            ),
            ("PATH".to_string(), path_dir.to_string_lossy().into_owned()),
        ]);
        let (resolved, _) = ensure_stable_copy(&current_exe, &environment, false)
            .expect("the PATH sirioctl is installed");
        assert_eq!(resolved, stable_path(&root));
        assert_copied_from(&resolved, &path_sirioctl);

        environment.insert(
            DATA_BASE.into(),
            root.join("missing-data").display().to_string(),
        );
        environment.insert(
            "PATH".into(),
            root.join("missing-path").display().to_string(),
        );
        let error = ensure_stable_copy(&root.join("missing/sirio"), &environment, false)
            .expect_err("a missing sirioctl must be surfaced");
        assert!(error.contains("sirioctl is unavailable"), "got {error}");
        let _ = std::fs::remove_dir_all(root);
    }

    /// A dangling link at the stable path is the ordinary end state of a
    /// rebuild: the link outlives the `target/` it pointed into. It is replaced
    /// by a copy rather than refused.
    #[cfg(unix)]
    #[test]
    fn sirioctl_resolver_replaces_a_stale_install_symlink() {
        let root = fixture("stale-link");
        let path_dir = root.join("path");
        std::fs::create_dir_all(&path_dir).expect("create the PATH fixture");
        let path_sirioctl = path_dir.join("sirioctl");
        std::fs::write(&path_sirioctl, b"sirioctl").expect("write the PATH fixture");
        make_executable(&path_sirioctl);
        let destination = stable_path(&root);
        std::fs::create_dir_all(destination.parent().expect("install parent"))
            .expect("create the install dir");
        std::os::unix::fs::symlink(root.join("deleted-target/debug/sirioctl"), &destination)
            .expect("create the dangling link");
        assert!(std::fs::metadata(&destination).is_err());
        let environment = BTreeMap::from([
            (
                DATA_BASE.to_string(),
                root.join("data").to_string_lossy().into_owned(),
            ),
            ("PATH".to_string(), path_dir.to_string_lossy().into_owned()),
        ]);

        let (resolved, outcome) = ensure_stable_copy(&root.join("app/sirio"), &environment, false)
            .expect("a dangling link is replaced, not refused");
        assert_eq!(resolved, destination);
        assert_eq!(outcome, CopyOutcome::Replaced);
        assert_copied_from(&resolved, &path_sirioctl);
        let _ = std::fs::remove_dir_all(root);
    }

    /// A pane starts with the copy that is there. Its contents are not read or
    /// compared: the source has different bytes, and the copy keeps its own.
    #[test]
    fn resolving_for_a_pane_reads_no_contents_once_a_copy_exists() {
        let root = fixture("resolve-for-pane");
        let environment = environment_under(&root);
        let current_exe = app_beside(&root, "app", b"copy build");
        ensure_stable_copy(&current_exe, &environment, false).expect("the first install");
        // Same length, different bytes: a compare-and-replace would replace it.
        std::fs::write(
            current_exe
                .parent()
                .expect("app dir")
                .join(sirioctl_binary_name()),
            b"other buil",
        )
        .expect("write the newer source");

        let resolved =
            resolve_for_pane(&current_exe, &environment, false).expect("a pane resolves");
        assert_eq!(resolved, stable_path(&root));
        assert_eq!(
            std::fs::read(&resolved).expect("read the copy"),
            b"copy build",
            "resolving must not replace the copy"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// A copy that runs but cannot be replaced is kept, and the outcome says
    /// so: the install failed, but a pane can still start with the copy.
    #[cfg(unix)]
    #[test]
    fn a_copy_that_cannot_be_replaced_is_kept_as_stale() {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: `getuid` takes no arguments and cannot fail.
        if unsafe { libc::getuid() } == 0 {
            // Root ignores the directory mode, so the refusal cannot happen.
            return;
        }
        let root = fixture("stale-copy");
        let environment = environment_under(&root);
        let first = app_beside(&root, "first", b"old build");
        ensure_stable_copy(&first, &environment, false).expect("the first install");
        let stable = stable_path(&root);
        let directory = stable.parent().expect("stable dir").to_path_buf();
        let newer = app_beside(&root, "newer", b"new build");

        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o555))
            .expect("make the directory read-only");
        let result = ensure_stable_copy(&newer, &environment, false);
        // Restored before any assertion, so the fixture can be removed.
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755))
            .expect("restore the directory mode");

        let (path, outcome) = result.expect("a copy that runs is kept, not an error");
        assert_eq!(outcome, CopyOutcome::Stale);
        assert_eq!(path, stable);
        assert_eq!(
            std::fs::read(&stable).expect("read the copy"),
            b"old build",
            "the copy that runs keeps its bytes"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// The first launch has no copy at all: the pane needs one now.
    #[test]
    fn resolving_for_a_pane_makes_the_first_copy_when_none_exists() {
        let root = fixture("resolve-first-copy");
        let environment = environment_under(&root);
        let current_exe = app_beside(&root, "app", b"first build");

        let resolved =
            resolve_for_pane(&current_exe, &environment, false).expect("a pane resolves");
        assert_eq!(resolved, stable_path(&root));
        assert_eq!(
            std::fs::read(&resolved).expect("read the copy"),
            b"first build"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// The card's words follow each outcome exactly. A copy that is kept but
    /// was never compared says so, and a copy that failed has no version line
    /// at all. The foreign file's line puts a space before the dash.
    #[test]
    fn cli_status_words_follow_each_outcome() {
        let copy = |outcome| Ok((PathBuf::from("/data/Sirio/bin/sirioctl"), outcome));
        let linked = LinkOutcome::AlreadyLinked(PathBuf::from("/home/me/.local/bin/sirioctl"));
        for outcome in [
            CopyOutcome::Current,
            CopyOutcome::Installed,
            CopyOutcome::Replaced,
        ] {
            assert_eq!(
                status_for(&copy(outcome), &linked).version_line,
                "Same build as Sirio",
                "{outcome:?}"
            );
        }
        assert_eq!(
            status_for(&copy(CopyOutcome::Stale), &linked).version_line,
            "Older than this Sirio — it could not be replaced"
        );
        assert_eq!(
            status_for(&copy(CopyOutcome::SourceMissing), &linked).version_line,
            "Not checked: Sirio has no sirioctl beside it"
        );
        let failed: CopyResult = Err("no room in the data directory".into());
        assert_eq!(status_for(&failed, &linked).version_line, "");

        let foreign = |points_to| LinkOutcome::Foreign {
            place: PathBuf::from("/usr/local/bin/sirioctl"),
            points_to,
        };
        assert_eq!(
            path_line(&foreign(Some(PathBuf::from("/opt/other/sirioctl")))),
            "/usr/local/bin/sirioctl is not Sirio's (/opt/other/sirioctl) — left as it is"
        );
        assert_eq!(
            path_line(&foreign(None)),
            "/usr/local/bin/sirioctl is not Sirio's — left as it is"
        );
    }
}
