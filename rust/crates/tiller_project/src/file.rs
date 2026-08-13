use std::path::{Component, Path, PathBuf};

/// A node in a worktree's file tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileTreeEntry {
    /// The display name of this node.
    pub name: String,
    /// The path relative to the worktree root.
    pub relative_path: PathBuf,
    /// Whether this node is a directory.
    pub is_directory: bool,
    /// Children in directory-first, localized-name order.
    pub children: Vec<Self>,
}

/// Maximum image size accepted by a file drop.
pub const MAX_DROPPED_IMAGE_BYTES: u64 = 10 * 1024 * 1024;

/// How a dropped path is represented to the terminal/editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DroppedPath {
    Relative(PathBuf),
    Absolute(PathBuf),
}

/// A validated image file drop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DroppedFile {
    pub path: PathBuf,
    pub reference: DroppedPath,
    pub size_bytes: u64,
}

#[derive(Debug)]
pub enum FileDropError {
    Io(std::io::Error),
    Unsupported(PathBuf),
    TooLarge { path: PathBuf, size_bytes: u64 },
    InvalidWorktree(PathBuf),
}

impl std::fmt::Display for FileDropError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "reading dropped file: {error}"),
            Self::Unsupported(path) => write!(f, "unsupported dropped file: {}", path.display()),
            Self::TooLarge { path, size_bytes } => write!(
                f,
                "dropped file '{}' is {size_bytes} bytes; the limit is {MAX_DROPPED_IMAGE_BYTES}",
                path.display()
            ),
            Self::InvalidWorktree(path) => write!(f, "invalid worktree: {}", path.display()),
        }
    }
}

impl std::error::Error for FileDropError {}

impl From<std::io::Error> for FileDropError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Validates and classifies one dropped image path.
pub fn classify_file_drop(worktree: &Path, path: &Path) -> Result<DroppedFile, FileDropError> {
    let worktree = std::fs::canonicalize(worktree)
        .map_err(|_| FileDropError::InvalidWorktree(worktree.to_path_buf()))?;
    let path = std::fs::canonicalize(path)?;
    let metadata = std::fs::metadata(&path)?;
    if !metadata.is_file() {
        return Err(FileDropError::Unsupported(path));
    }
    let supported = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp"
            )
        });
    if !supported {
        return Err(FileDropError::Unsupported(path));
    }
    if metadata.len() > MAX_DROPPED_IMAGE_BYTES {
        return Err(FileDropError::TooLarge {
            path,
            size_bytes: metadata.len(),
        });
    }
    let reference = path
        .strip_prefix(&worktree)
        .map(|relative| DroppedPath::Relative(relative.to_path_buf()))
        .unwrap_or_else(|_| DroppedPath::Absolute(path.clone()));
    Ok(DroppedFile {
        path,
        reference,
        size_bytes: metadata.len(),
    })
}

/// Shell-quotes one path using POSIX single quotes.
pub fn shell_quote_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Turns dropped paths into the one terminal insertion string.
pub fn terminal_file_drop(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| shell_quote_path(path))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Why a file-tree request was rejected.
#[derive(Debug)]
pub enum FileTreeError {
    /// The filesystem could not be read.
    Io(std::io::Error),
    /// A path was not a safe relative path.
    InvalidPath(PathBuf),
    /// A symlink was encountered; following it could escape the worktree.
    Symlink(PathBuf),
}

impl std::fmt::Display for FileTreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "reading file tree: {error}"),
            Self::InvalidPath(path) => write!(f, "unsafe relative path: {}", path.display()),
            Self::Symlink(path) => {
                write!(f, "symlink is not allowed in file tree: {}", path.display())
            }
        }
    }
}

impl std::error::Error for FileTreeError {}

impl From<std::io::Error> for FileTreeError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Validates the path form accepted by file-tree and drop operations.
pub fn validate_relative_path(path: &Path) -> Result<(), FileTreeError> {
    let raw = path.to_string_lossy();
    if path.as_os_str().is_empty()
        || raw
            .split('/')
            .any(|component| component == "." || component == "..")
        || path.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_)
                    | Component::RootDir
                    | Component::ParentDir
                    | Component::CurDir
            )
        })
    {
        return Err(FileTreeError::InvalidPath(path.to_path_buf()));
    }
    Ok(())
}

/// Loads a worktree tree without following symlinks or exposing `.git`.
pub fn load_file_tree(root: &Path) -> Result<Vec<FileTreeEntry>, FileTreeError> {
    let root_metadata = std::fs::symlink_metadata(root)?;
    if root_metadata.file_type().is_symlink() {
        return Err(FileTreeError::Symlink(root.to_path_buf()));
    }
    if !root_metadata.is_dir() {
        return Err(FileTreeError::Io(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            format!("{} is not a directory", root.display()),
        )));
    }
    read_directory(root, Path::new(""))
}

fn read_directory(root: &Path, relative_root: &Path) -> Result<Vec<FileTreeEntry>, FileTreeError> {
    let mut entries = Vec::new();
    for item in std::fs::read_dir(root)? {
        let item = item?;
        let name = item.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        let relative_path = if relative_root.as_os_str().is_empty() {
            PathBuf::from(&name)
        } else {
            relative_root.join(&name)
        };
        validate_relative_path(&relative_path)?;
        let file_type = item.file_type()?;
        if file_type.is_symlink() {
            return Err(FileTreeError::Symlink(relative_path));
        }
        let is_directory = file_type.is_dir();
        let children = if is_directory {
            read_directory(&item.path(), &relative_path)?
        } else {
            Vec::new()
        };
        entries.push(FileTreeEntry {
            name,
            relative_path,
            is_directory,
            children,
        });
    }
    entries.sort_by(|left, right| {
        (!left.is_directory, left.name.to_lowercase(), &left.name).cmp(&(
            !right.is_directory,
            right.name.to_lowercase(),
            &right.name,
        ))
    });
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn temp_root() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "tiller-file-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn loads_safe_tree_with_directories_before_localized_files() {
        let root = temp_root();
        fs::create_dir(root.join("z-dir")).unwrap();
        fs::create_dir(root.join("A-dir")).unwrap();
        fs::write(root.join("é.txt"), b"e").unwrap();
        fs::write(root.join("a.txt"), b"a").unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".git/config"), b"private").unwrap();

        let tree = load_file_tree(&root).unwrap();
        assert_eq!(
            tree.iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["A-dir", "z-dir", "a.txt", "é.txt"]
        );
        assert!(
            tree.iter()
                .all(|entry| entry.relative_path != Path::new(".git"))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_symlinks_instead_of_traversing_them() {
        let root = temp_root();
        let outside = temp_root();
        fs::write(outside.join("secret.txt"), b"secret").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("linked")).unwrap();

        assert!(matches!(
            load_file_tree(&root),
            Err(FileTreeError::Symlink(path)) if path.ends_with("linked")
        ));

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn rejects_paths_with_absolute_or_parent_components() {
        assert!(validate_relative_path(Path::new("/absolute")).is_err());
        assert!(validate_relative_path(Path::new("a/../secret")).is_err());
        assert!(validate_relative_path(Path::new("a/./file")).is_err());
    }

    #[test]
    fn classifies_supported_drops_relative_to_the_worktree() {
        let root = temp_root();
        fs::write(root.join("screen.PNG"), b"png").unwrap();
        let dropped = classify_file_drop(&root, &root.join("screen.PNG")).unwrap();
        assert_eq!(
            dropped.reference,
            DroppedPath::Relative(Path::new("screen.PNG").into())
        );
        assert_eq!(dropped.size_bytes, 3);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_unsupported_and_oversized_drops_and_keeps_outside_paths_absolute() {
        let root = temp_root();
        let outside = temp_root();
        fs::write(root.join("note.txt"), b"text").unwrap();
        assert!(matches!(
            classify_file_drop(&root, &root.join("note.txt")),
            Err(FileDropError::Unsupported(_))
        ));
        let huge = root.join("huge.webp");
        let file = fs::File::create(&huge).unwrap();
        file.set_len(MAX_DROPPED_IMAGE_BYTES + 1).unwrap();
        assert!(matches!(
            classify_file_drop(&root, &huge),
            Err(FileDropError::TooLarge { .. })
        ));
        let external = outside.join("photo.jpg");
        fs::write(&external, b"jpg").unwrap();
        let dropped = classify_file_drop(&root, &external).unwrap();
        assert_eq!(
            dropped.reference,
            DroppedPath::Absolute(external.canonicalize().unwrap())
        );
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn terminal_drop_is_one_quoted_space_separated_string() {
        assert_eq!(
            terminal_file_drop(&[
                PathBuf::from("a file.png"),
                PathBuf::from("it's.webp"),
                PathBuf::from("日本語.gif")
            ]),
            "'a file.png' 'it'\\''s.webp' '日本語.gif'"
        );
    }
}
