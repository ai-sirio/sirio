//! macOS applying: swap the installed `Sirio.app` bundle (#313).
//!
//! A verified download is a `.dmg` around a `Sirio.app`. The swap is
//! **move-aside-then-rename**, never copy-over, so a failure mid-way leaves
//! the old bundle intact rather than a half-written one: the new bundle is
//! copied off the read-only image to a sibling name, then only atomic
//! renames touch the live path. The moved-aside copy *is* the rollback — it
//! is kept until the new bundle is verified in place, then deleted, so a
//! half-applied update cannot leave the user without a working Sirio
//! ([spec §6.1]).
//!
//! The mount is detached on every exit path: a leaked mount is a device the
//! user has to eject by hand ([spec §5.2]).
//!
//! The app is **not** restarted: macOS applies the swap and lets the running
//! instance carry on until the user relaunches.
//!
//! Self-location walks up from the executable to the `.app` bundle
//! (`…/Sirio.app/Contents/MacOS/sirio` → `…/Sirio.app`). No bundle above it
//! means this is a `cargo run` build, not an install — refused with the
//! download-page pointer, never guessed. That is also the permanent state of
//! a dev build, so the refusal path is exercised constantly.
//!
//! Everything here is testable without macOS: the swap is an orchestrator
//! over two injected effects ([`Mounter`]/[`Unmounter`], the `hdiutil`
//! equivalents) and the real filesystem, so tests drive the whole sequence
//! on any platform. Only [`apply`] reaches for the real `hdiutil`.
//!
//! [spec §5.2]: https://github.com/ai-sirio/sirio/blob/main/docs/superpowers/specs/2026-08-29-auto-update-design.md
//! [spec §6.1]: https://github.com/ai-sirio/sirio/blob/main/docs/superpowers/specs/2026-08-29-auto-update-design.md

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use std::fs;

use super::ApplyError;
use sirio_update::VerifiedUpdate;

/// Mount a disk image and return the mount point holding the staged bundle.
type Mounter<'a> = dyn Fn(&Path) -> Result<PathBuf, String> + 'a;

/// Detach a mount point.
type Unmounter<'a> = dyn Fn(&Path) -> Result<(), String> + 'a;

/// The whole macOS applying sequence: self-location first (refusing is the
/// healthy outcome when this is not an install, and nothing is mounted
/// then), then the bundle swap.
pub fn apply(update: &VerifiedUpdate) -> Result<(), ApplyError> {
    let installed = self_locate()?;
    // Fail fast when the `.app` ancestor is not actually a directory (a file
    // that happens to end in `.app` is not an install either).
    if !installed.is_dir() {
        return Err(ApplyError::InstallNotFound);
    }
    swap_at(update, &installed, &real_mount, &real_unmount)
}

/// Self-location for the real process.
fn self_locate() -> Result<PathBuf, ApplyError> {
    let exe = std::env::current_exe().map_err(|_| ApplyError::InstallNotFound)?;
    bundle_of(&exe)
}

/// Pure self-location: walk up from the executable to the first `.app`
/// ancestor. Anything else is not an install the updater recognizes.
fn bundle_of(exe: &Path) -> Result<PathBuf, ApplyError> {
    let mut current = exe;
    while let Some(parent) = current.parent() {
        current = parent;
        if current
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
        {
            return Ok(current.to_path_buf());
        }
    }
    Err(ApplyError::InstallNotFound)
}

/// Attach the verified disk image, swap the bundles, and detach — the
/// unmount runs on every exit path: the swap result is computed first, the
/// detach second, and the caller sees the swap failure (a leak must never
/// mask the real error) or, when the swap succeeded, a detach failure.
fn swap_at(
    update: &VerifiedUpdate,
    installed: &Path,
    mount: &Mounter<'_>,
    unmount: &Unmounter<'_>,
) -> Result<(), ApplyError> {
    let point = mount(&update.path).map_err(|error| {
        ApplyError::Swap(format!("could not mount the update disk image: {error}"))
    })?;
    let swapped = swap(installed, &point);
    let detached = unmount(&point).map_err(|error| {
        ApplyError::Swap(format!("could not detach the update disk image: {error}"))
    });
    swapped.and(detached)
}

/// The swap itself. `point` is a mounted disk image; `installed` is the live
/// bundle this process is running from.
fn swap(installed: &Path, point: &Path) -> Result<(), ApplyError> {
    let Some(staged) = find_bundle_in(point) else {
        return Err(ApplyError::Swap(format!(
            "the update disk image contains no Sirio.app (mounted at {})",
            point.display()
        )));
    };
    let aside = sibling(installed, "previous");
    let incoming = sibling(installed, "incoming");
    // Leftovers from an interrupted earlier swap are redundant — the bundle
    // at `installed` is the live one. Clear them so every rename below is
    // collision-free; a failure surfaces before the live path is touched.
    for stale in [&aside, &incoming] {
        if stale.exists() {
            fs::remove_dir_all(stale).map_err(|error| {
                ApplyError::Swap(format!(
                    "could not clear the stale swap copy at {stale:?}: {error}"
                ))
            })?;
        }
    }

    // Copy off the read-only image onto the install volume first, then only
    // atomic renames touch the live path: a copy interrupted half-way
    // corrupts `incoming`, never `installed`.
    let result = (|| -> Result<(), String> {
        copy_tree(&staged, &incoming)?;
        fs::rename(installed, &aside).map_err(|error| error.to_string())?;
        fs::rename(&incoming, installed).map_err(|error| error.to_string())?;
        verify_in_place(installed)
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&incoming);
        // Rollback: the moved-aside copy IS the previous version. When the
        // new bundle already made it into place (a failed verification), it
        // must go before the previous version can return.
        let restored = if installed.exists() {
            fs::remove_dir_all(installed)
                .and_then(|()| fs::rename(&aside, installed))
                .map_err(|error| error.to_string())
        } else {
            fs::rename(&aside, installed).map_err(|error| error.to_string())
        };
        if restored.is_err() {
            return Err(ApplyError::Rollback {
                attempted: error,
                aside,
            });
        }
        return Err(ApplyError::Swap(error));
    }

    // Verified in place: the backup is redundant now. Best effort — a
    // leftover only costs disk, and failing here would reject a completed
    // update.
    let _ = fs::remove_dir_all(&aside);
    Ok(())
}

/// Find the `Sirio.app` staged inside a mounted disk image: the mount root
/// first, then one level of subdirectories (some images wrap the bundle in a
/// folder next to an `Applications` symlink).
fn find_bundle_in(point: &Path) -> Option<PathBuf> {
    let mut frontier = vec![point.to_path_buf()];
    for _ in 0..2 {
        let mut next = Vec::new();
        for dir in &frontier {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.filter_map(|entry| entry.ok()) {
                let path = entry.path();
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if is_dir
                    && path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
                {
                    return Some(path);
                }
                if is_dir {
                    next.push(path);
                }
            }
        }
        frontier = next;
    }
    None
}

/// The new bundle is real only if it looks like an installed copy: an
/// `Info.plist` and a non-empty `Contents/MacOS`.
fn verify_in_place(bundle: &Path) -> Result<(), String> {
    let contents = bundle.join("Contents");
    let looks_installed = contents.join("Info.plist").is_file()
        && fs::read_dir(contents.join("MacOS"))
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false);
    if looks_installed {
        Ok(())
    } else {
        Err(format!(
            "the new bundle at {} is not a valid Sirio.app",
            bundle.display()
        ))
    }
}

/// The moved-aside or staged copy of the bundle, always a sibling on the
/// same volume so renames stay atomic.
fn sibling(installed: &Path, role: &str) -> PathBuf {
    let stem = installed
        .file_stem()
        .map(|stem| stem.to_string_lossy())
        .unwrap_or_default();
    installed.with_file_name(format!("{stem}.{role}.app"))
}

/// Recursive directory copy; `std::fs` has no `copy_dir_all`.
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let target = to.join(entry.file_name());
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

/// The real mounter: `hdiutil attach -nobrowse -readonly -mountpoint <fresh
/// temp dir> <dmg>`, so the mount point is deterministic instead of parsed
/// out of `hdiutil` output.
fn real_mount(dmg: &Path) -> Result<PathBuf, String> {
    static NEXT_MOUNT: AtomicU64 = AtomicU64::new(0);
    let point = std::env::temp_dir().join(format!(
        "sirio-apply-mount-{}-{}",
        std::process::id(),
        NEXT_MOUNT.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::create_dir_all(&point).map_err(|error| error.to_string())?;
    let output = Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
        .arg(&point)
        .arg(dmg)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(point)
    } else {
        let _ = std::fs::remove_dir(&point);
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// The real unmounter: `hdiutil detach`, then `-force` as the second attempt
/// before giving up and reporting the leak; the scratch mount point is
/// removed either way.
fn real_unmount(point: &Path) -> Result<(), String> {
    let detach = |force: bool| {
        let mut command = Command::new("hdiutil");
        command.arg("detach");
        if force {
            command.arg("-force");
        }
        command.arg(point).output()
    };
    let output = detach(false)
        .and_then(|first| {
            if first.status.success() {
                Ok(first)
            } else {
                detach(true)
            }
        })
        .map_err(|error| error.to_string())?;
    let _ = std::fs::remove_dir(point);
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("sirio-apply-macos-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A bundle on disk whose version marker sits in both `Info.plist` and
    /// the binary, so a test can tell "old" from "new" after a swap.
    fn make_bundle(parent: &Path, name: &str, version: &str) -> PathBuf {
        let bundle = parent.join(name);
        fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        fs::write(bundle.join("Contents/Info.plist"), version).unwrap();
        fs::write(bundle.join("Contents/MacOS/sirio"), version).unwrap();
        bundle
    }

    fn update(path: &Path) -> VerifiedUpdate {
        VerifiedUpdate {
            version: "0.7.0".into(),
            notes: "notes".into(),
            path: path.to_path_buf(),
            platform: "darwin-aarch64".into(),
        }
    }

    /// Fakes `hdiutil`: `mount` materializes the "disk image" directory at a
    /// fresh mount point, `unmount` records the detach and removes the point.
    struct FakeHdiutil {
        root: PathBuf,
        mounted: Cell<u32>,
        detached: RefCell<Vec<PathBuf>>,
        fail_attach: bool,
        fail_detach: bool,
    }

    impl FakeHdiutil {
        fn new(root: &Path) -> Self {
            Self {
                root: root.to_path_buf(),
                mounted: Cell::new(0),
                detached: RefCell::new(Vec::new()),
                fail_attach: false,
                fail_detach: false,
            }
        }

        fn mount(&self, dmg: &Path) -> Result<PathBuf, String> {
            if self.fail_attach {
                return Err("injected attach failure".into());
            }
            let n = self.mounted.get() + 1;
            self.mounted.set(n);
            let point = self.root.join(format!("mnt-{n}"));
            fs::create_dir_all(&point).unwrap();
            for entry in fs::read_dir(dmg).unwrap() {
                let entry = entry.unwrap().path();
                copy_tree(&entry, &point.join(entry.file_name().unwrap())).unwrap();
            }
            Ok(point)
        }

        fn unmount(&self, point: &Path) -> Result<(), String> {
            self.detached.borrow_mut().push(point.to_path_buf());
            if self.fail_detach {
                return Err("injected detach failure".into());
            }
            fs::remove_dir_all(point).unwrap();
            Ok(())
        }

        /// Run [`swap_at`] against this fake, with `installed` as the live bundle.
        fn swap_at(&self, update: &VerifiedUpdate, installed: &Path) -> Result<(), ApplyError> {
            super::swap_at(update, installed, &|dmg| self.mount(dmg), &|point| {
                self.unmount(point)
            })
        }
    }

    fn swap_fixture(name: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf, FakeHdiutil) {
        let root = temp_dir(name);
        let apps = root.join("apps");
        let installed = make_bundle(&apps, "Sirio.app", "0.6.0");
        let dmg = root.join("dmg");
        fs::create_dir_all(&dmg).unwrap();
        make_bundle(&dmg, "Sirio.app", "0.7.0");
        let host = FakeHdiutil::new(&root);
        (root, apps, installed, dmg, host)
    }

    #[test]
    fn self_location_finds_the_bundle_above_the_binary() {
        let exe = Path::new("/Applications/Sirio.app/Contents/MacOS/sirio");
        assert_eq!(
            bundle_of(exe).unwrap(),
            Path::new("/Applications/Sirio.app")
        );
    }

    #[test]
    fn self_location_refuses_a_cargo_run_binary_and_points_at_the_download_page() {
        let error = bundle_of(Path::new("target/debug/sirio")).unwrap_err();
        assert_eq!(error, ApplyError::InstallNotFound);
        assert!(error.to_string().contains(super::super::DOWNLOAD_PAGE_URL));
    }

    #[test]
    fn a_verified_dmg_replaces_the_installed_bundle_which_then_reports_the_new_version() {
        let (_root, apps, installed, dmg, host) = swap_fixture("swap");

        host.swap_at(&update(&dmg), &installed).unwrap();

        assert_eq!(
            fs::read_to_string(installed.join("Contents/Info.plist")).unwrap(),
            "0.7.0"
        );
        assert_eq!(
            fs::read_to_string(installed.join("Contents/MacOS/sirio")).unwrap(),
            "0.7.0"
        );
        // Verified in place, so the moved-aside copy is gone; so is the mount.
        assert!(!apps.join("Sirio.previous.app").exists());
        assert_eq!(host.detached.borrow().len(), 1);
    }

    #[test]
    fn a_failure_after_the_move_aside_restores_the_previous_bundle() {
        let (_root, apps, installed, dmg, host) = swap_fixture("rollback");
        // A bundle whose Contents has no MacOS payload: every rename succeeds,
        // in-place verification fails — which is a failure *after* the move-aside.
        fs::remove_dir_all(dmg.join("Sirio.app/Contents/MacOS")).unwrap();

        let error = host.swap_at(&update(&dmg), &installed).unwrap_err();

        assert!(matches!(error, ApplyError::Swap(_)), "{error:?}");
        assert_eq!(
            fs::read_to_string(installed.join("Contents/MacOS/sirio")).unwrap(),
            "0.6.0",
            "the previous version must be back in place"
        );
        assert!(!apps.join("Sirio.previous.app").exists(), "rolled back");
        assert!(!apps.join("Sirio.incoming.app").exists(), "copy cleaned up");
        assert_eq!(host.detached.borrow().len(), 1, "mount must be detached");
    }

    #[test]
    fn the_mount_is_detached_when_the_image_has_no_bundle() {
        let (_root, _apps, installed, dmg, host) = swap_fixture("empty-dmg");
        fs::remove_dir_all(&dmg).unwrap();
        fs::create_dir_all(dmg.join("docs")).unwrap();

        let error = host.swap_at(&update(&dmg), &installed).unwrap_err();

        assert!(matches!(error, ApplyError::Swap(_)), "{error:?}");
        assert!(error.to_string().contains("contains no Sirio.app"));
        assert_eq!(
            fs::read_to_string(installed.join("Contents/MacOS/sirio")).unwrap(),
            "0.6.0",
            "the live bundle must be untouched"
        );
        assert_eq!(host.detached.borrow().len(), 1, "mount must be detached");
    }

    #[test]
    fn a_failed_attach_touches_nothing_and_detaches_nothing() {
        let (_root, _apps, installed, dmg, mut host) = swap_fixture("attach-fail");
        host.fail_attach = true;

        let error = host.swap_at(&update(&dmg), &installed).unwrap_err();

        assert!(error.to_string().contains("could not mount"), "{error:?}");
        assert_eq!(
            fs::read_to_string(installed.join("Contents/MacOS/sirio")).unwrap(),
            "0.6.0"
        );
        assert!(host.detached.borrow().is_empty(), "no mount to detach");
    }

    #[test]
    fn a_failed_detach_is_surfaced_after_a_completed_swap() {
        let (_root, apps, installed, dmg, mut host) = swap_fixture("detach-fail");
        host.fail_detach = true;

        let error = host.swap_at(&update(&dmg), &installed).unwrap_err();

        assert!(error.to_string().contains("could not detach"), "{error:?}");
        // The swap itself is done; the error only reports the leaked mount.
        assert_eq!(
            fs::read_to_string(installed.join("Contents/Info.plist")).unwrap(),
            "0.7.0"
        );
        assert!(!apps.join("Sirio.previous.app").exists());
        assert_eq!(host.detached.borrow().len(), 1);
    }

    #[test]
    fn leftovers_from_an_interrupted_swap_do_not_block_the_next_one() {
        let (_root, apps, installed, dmg, host) = swap_fixture("stale");
        // A previous run died between move-aside and delete.
        fs::create_dir_all(apps.join("Sirio.previous.app")).unwrap();
        fs::create_dir_all(apps.join("Sirio.incoming.app")).unwrap();

        host.swap_at(&update(&dmg), &installed).unwrap();

        assert_eq!(
            fs::read_to_string(installed.join("Contents/Info.plist")).unwrap(),
            "0.7.0"
        );
        assert!(!apps.join("Sirio.previous.app").exists());
        assert!(!apps.join("Sirio.incoming.app").exists());
    }
}
