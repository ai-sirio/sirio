//! Publishing a file so a reader never observes a partial one.
//!
//! Both the registry cache and the install manifest need the same
//! guarantee, and both grew their own copy of temp-write-then-rename. The
//! copies drifted: each named its scratch file after its target with a
//! fixed suffix, so two writers aiming at one target shared one scratch
//! file and clobbered each other before either renamed. The installer had
//! already learned this and names its staging for its owner
//! (`installer::staging_dir`); this module is that lesson applied to the
//! other two sites, in one place that cannot drift again.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A scratch path beside `target`, unique per writer.
///
/// Beside, not inside a temp dir: `rename` is only atomic within one
/// filesystem, and a scratch file elsewhere silently turns the publish
/// into a copy that can be observed half-done.
///
/// The pid separates processes; the counter separates writers inside one
/// process, which a pid alone cannot — two rapid Refresh clicks are the
/// case that motivated it.
pub(crate) fn temp_sibling(target: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let ticket = NEXT.fetch_add(1, Ordering::Relaxed);

    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}-{ticket}.tmp", std::process::id()));
    target.with_file_name(name)
}

/// Write `bytes` so that `target` is either its old content or all of the
/// new content, never a mixture.
///
/// A failure part-way leaves the scratch file behind rather than a
/// damaged target; that is the trade this pattern exists to make.
pub(crate) fn write_atomically(target: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = temp_sibling(target);
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sirio-atomic-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn two_writers_never_share_one_temp_file() {
        let target = Path::new("/data/registry.json");

        let first = temp_sibling(target);
        let second = temp_sibling(target);

        assert_ne!(
            first, second,
            "a fixed temp name lets a second writer overwrite the first \
             writer's scratch file before either renames"
        );
    }

    #[test]
    fn the_temp_file_sits_beside_its_target() {
        let target = Path::new("/data/cache/registry.json");

        let temp = temp_sibling(target);

        assert_eq!(
            temp.parent(),
            target.parent(),
            "rename is only atomic within one filesystem, so the scratch \
             file must be a sibling"
        );
        assert!(
            temp.file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(".tmp"),
            "a sweep needs to recognise abandoned scratch files"
        );
    }

    #[test]
    fn a_published_file_leaves_no_scratch_behind() {
        let dir = scratch("published");
        let target = dir.join("registry.json");

        write_atomically(&target, b"payload").unwrap();

        assert_eq!(std::fs::read_to_string(&target).unwrap(), "payload");
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "scratch files left behind: {leftovers:?}"
        );
    }

    #[test]
    fn a_missing_parent_directory_is_created() {
        let dir = scratch("missing-parent");
        let target = dir.join("nested").join("deeper").join("manifest.json");

        write_atomically(&target, b"{}").unwrap();

        assert_eq!(std::fs::read_to_string(&target).unwrap(), "{}");
    }

    #[test]
    fn a_second_write_replaces_the_first_whole() {
        let dir = scratch("replace");
        let target = dir.join("registry.json");

        write_atomically(&target, b"first-and-much-longer").unwrap();
        write_atomically(&target, b"second").unwrap();

        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "second",
            "a shorter payload must not leave a tail of the longer one"
        );
    }
}
