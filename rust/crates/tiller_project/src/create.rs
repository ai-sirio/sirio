//! Filesystem operation used by the create-project surface.

use std::fmt;
use std::path::{Component, Path, PathBuf};

/// A failure while creating a project directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectCreationError {
    /// The form submitted only whitespace.
    EmptyName,
    /// A project name must be one directory component, not a path.
    InvalidName,
    /// The parent or destination could not be created.
    Io { path: PathBuf, message: String },
}

impl fmt::Display for ProjectCreationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => formatter.write_str("project name cannot be empty"),
            Self::InvalidName => formatter.write_str("project name must be one folder name"),
            Self::Io { path, message } => {
                write!(formatter, "could not create {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for ProjectCreationError {}

/// Creates one new project directory beneath `parent` and returns its path.
///
/// The operation intentionally creates only the requested directory. Git
/// initialization, discovery, and sidebar registration belong to the host
/// that consumes the returned path.
pub fn create_project(parent: &Path, name: &str) -> Result<PathBuf, ProjectCreationError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ProjectCreationError::EmptyName);
    }

    let mut components = Path::new(name).components();
    let is_single_folder_name = matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    );
    if !is_single_folder_name {
        return Err(ProjectCreationError::InvalidName);
    }

    let destination = parent.join(name);
    std::fs::create_dir(&destination).map_err(|error| ProjectCreationError::Io {
        path: destination.clone(),
        message: error.to_string(),
    })?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{ProjectCreationError, create_project};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            // Wall-clock nanos alone are not unique: under load the clock can
            // return the same value for back-to-back calls in this process,
            // colliding into AlreadyExists. A process-local counter cannot.
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tiller-project-create-test-{}-{}",
                std::process::id(),
                id
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir(&path).expect("create test parent");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn creates_named_project_directory() {
        let parent = TempDir::new();

        let created = create_project(&parent.0, "new-project").expect("create project");

        assert_eq!(created, parent.0.join("new-project"));
        assert!(created.is_dir());
    }

    #[test]
    fn empty_or_nested_names_are_rejected_before_touching_disk() {
        let parent = TempDir::new();

        assert_eq!(
            create_project(&parent.0, "   "),
            Err(ProjectCreationError::EmptyName)
        );
        assert_eq!(
            create_project(&parent.0, "nested/project"),
            Err(ProjectCreationError::InvalidName)
        );
        assert!(!parent.0.join("nested").exists());
    }

    #[test]
    fn existing_project_is_a_visible_creation_failure() {
        let parent = TempDir::new();
        std::fs::create_dir(parent.0.join("already-there")).expect("create existing project");

        let error = create_project(&parent.0, "already-there").expect_err("duplicate fails");

        assert!(matches!(error, ProjectCreationError::Io { .. }));
    }
}
