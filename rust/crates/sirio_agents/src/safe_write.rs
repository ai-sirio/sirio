//! Writes under a worktree that the worktree's own content cannot redirect.
//! A change request's author controls the files a hand-off's worktree holds,
//! so a committed symlink standing where `prepare` writes would send the write
//! outside the worktree.

use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Writes `contents` to `relative` under `worktree`, creating the missing
/// folders on the way. Refuses a symlink on the path, and a component that
/// exists but is not a folder. The write goes to a temp file beside the target
/// first, then is renamed over it, so a link at the final name is replaced
/// rather than followed (and the walk has refused one already).
pub(crate) fn write_in_worktree(worktree: &Path, relative: &Path, contents: &[u8]) -> io::Result<()> {
    let mut names: Vec<&OsStr> = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(name) => names.push(name),
            _ => return Err(io::Error::other(format!("not a path inside the worktree: {}", relative.display()))),
        }
    }
    let Some((file, folders)) = names.split_last() else {
        return Err(io::Error::other("no file name to write"));
    };

    let mut dir = worktree.to_path_buf();
    let mut walked = PathBuf::new();
    for folder in folders {
        dir.push(folder);
        walked.push(folder);
        match std::fs::symlink_metadata(&dir) {
            Ok(meta) if meta.file_type().is_symlink() => return Err(refused(&walked)),
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => return Err(io::Error::other(format!("{} is not a folder", walked.display()))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => std::fs::create_dir(&dir)?,
            Err(error) => return Err(error),
        }
    }

    let target = dir.join(file);
    let walked_file = walked.join(file);
    match std::fs::symlink_metadata(&target) {
        Ok(meta) if meta.file_type().is_symlink() => return Err(refused(&walked_file)),
        Ok(meta) if meta.is_dir() => {
            return Err(io::Error::other(format!("{} is a folder", walked_file.display())));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".{}.sirio-tmp-{}-{unique}", file.to_string_lossy(), std::process::id()));
    // `create_new` never opens an existing entry, so a link planted at the
    // temp name is an error, not a write through it.
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut out| out.write_all(contents));
    if let Err(error) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(error);
    }
    std::fs::rename(&tmp, &target).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// The most `read_in_worktree` reads: a settings or skill file is a few KiB.
const READ_LIMIT: u64 = 1024 * 1024;

/// Reads `relative` under `worktree` the way `write_in_worktree` writes it:
/// a symlink anywhere on the path is refused, so a committed link to a FIFO
/// or `/dev/zero` cannot block or exhaust the read, and only a regular file
/// up to 1 MiB is read. `Ok(None)` when nothing stands at the path.
pub(crate) fn read_in_worktree(worktree: &Path, relative: &Path) -> io::Result<Option<String>> {
    let mut path = worktree.to_path_buf();
    let mut walked = PathBuf::new();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(io::Error::other(format!("not a path inside the worktree: {}", relative.display())));
        };
        path.push(name);
        walked.push(name);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(io::Error::other(format!("refusing to read through a symlink: {}", walked.display())));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        }
    }
    if !std::fs::symlink_metadata(&path)?.is_file() {
        return Err(io::Error::other(format!("{} is not a file", walked.display())));
    }
    let mut text = String::new();
    std::fs::File::open(&path)?.take(READ_LIMIT + 1).read_to_string(&mut text)?;
    if text.len() as u64 > READ_LIMIT {
        return Err(io::Error::other(format!("{} is larger than 1 MiB", walked.display())));
    }
    Ok(Some(text))
}

fn refused(path: &Path) -> io::Error {
    io::Error::other(format!("refusing to write through a symlink: {}", path.display()))
}

#[cfg(all(test, unix))]
mod tests {
    use super::write_in_worktree;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A throwaway directory under TMPDIR, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("sirio-safe-write-{}-{unique}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(std::fs::canonicalize(&path).expect("canonicalize temp dir"))
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A worktree and, beside it, a folder outside it holding a file a symlink
    /// in the worktree points at. Returns (worktree, outside, the target file).
    fn worktree_with_outside(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
        let worktree = root.join("worktree");
        let outside = root.join("outside");
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let target = outside.join("target.txt");
        std::fs::write(&target, "untouched").unwrap();
        (worktree, outside, target)
    }

    #[test]
    fn a_symlinked_parent_directory_is_refused_and_its_target_is_untouched() {
        let root = TempDir::new();
        let (worktree, outside, target) = worktree_with_outside(&root.0);
        symlink(&outside, worktree.join(".opencode")).unwrap();

        let result = write_in_worktree(&worktree, Path::new(".opencode/plugin/sirio-session.js"), b"planted");

        assert!(result.is_err(), "a write went through a symlinked folder");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "untouched");
        assert!(!outside.join("plugin").exists(), "a folder was made outside the worktree");
    }

    #[test]
    fn a_symlink_at_the_final_name_is_refused_and_its_target_is_untouched() {
        let root = TempDir::new();
        let (worktree, _outside, target) = worktree_with_outside(&root.0);
        std::fs::create_dir_all(worktree.join(".sirio")).unwrap();
        symlink(&target, worktree.join(".sirio/omp-hook.ts")).unwrap();

        let result = write_in_worktree(&worktree, Path::new(".sirio/omp-hook.ts"), b"planted");

        assert!(result.is_err(), "a write went through a symlink at the file's name");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "untouched");
    }

    #[test]
    fn a_missing_directory_is_created_and_the_file_written() {
        let root = TempDir::new();
        let worktree = root.0.join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();

        write_in_worktree(&worktree, Path::new(".claude/skills/sirio/SKILL.md"), b"skill").unwrap();

        assert_eq!(std::fs::read_to_string(worktree.join(".claude/skills/sirio/SKILL.md")).unwrap(), "skill");
    }

    #[test]
    fn an_existing_file_is_replaced() {
        let root = TempDir::new();
        let worktree = root.0.join("worktree");
        std::fs::create_dir_all(worktree.join(".sirio")).unwrap();
        std::fs::write(worktree.join(".sirio/omp-hook.ts"), "old").unwrap();

        write_in_worktree(&worktree, Path::new(".sirio/omp-hook.ts"), b"new").unwrap();

        assert_eq!(std::fs::read_to_string(worktree.join(".sirio/omp-hook.ts")).unwrap(), "new");
    }

    #[test]
    fn a_component_that_is_a_file_is_refused() {
        let root = TempDir::new();
        let worktree = root.0.join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(worktree.join(".sirio"), "a file, not a folder").unwrap();

        let result = write_in_worktree(&worktree, Path::new(".sirio/omp-hook.ts"), b"new");

        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(worktree.join(".sirio")).unwrap(), "a file, not a folder");
    }
}
