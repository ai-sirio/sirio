use std::path::{Path, PathBuf};

/// The externally observable state of an open document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentChange {
    Unchanged,
    Reloaded,
    Conflict,
    Deleted,
}

/// A UTF-8 markdown document with conflict-aware external reload behavior.
#[derive(Debug)]
pub struct MarkdownDocument {
    path: PathBuf,
    text: String,
    saved_text: String,
    dirty: bool,
    conflict: bool,
    deleted: bool,
}

#[derive(Debug)]
pub enum DocumentError {
    Io(std::io::Error),
    InvalidUtf8(std::string::FromUtf8Error),
    Conflict,
    Deleted,
}

impl std::fmt::Display for DocumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "document I/O: {error}"),
            Self::InvalidUtf8(error) => write!(f, "document is not UTF-8: {error}"),
            Self::Conflict => f.write_str("document changed externally while it had local edits"),
            Self::Deleted => f.write_str("document was deleted externally"),
        }
    }
}

impl std::error::Error for DocumentError {}

impl From<std::io::Error> for DocumentError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl MarkdownDocument {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, DocumentError> {
        let path = path.into();
        let text = std::fs::read(&path)
            .map_err(DocumentError::Io)
            .and_then(|bytes| String::from_utf8(bytes).map_err(DocumentError::InvalidUtf8))?;
        Ok(Self {
            path,
            saved_text: text.clone(),
            text,
            dirty: false,
            conflict: false,
            deleted: false,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn has_conflict(&self) -> bool {
        self.conflict
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.dirty = self.text != self.saved_text;
    }

    /// Detects an external change and reloads it only when local text is clean.
    pub fn refresh_from_disk(&mut self) -> Result<DocumentChange, DocumentError> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.deleted = true;
                return Ok(DocumentChange::Deleted);
            }
            Err(error) => return Err(DocumentError::Io(error)),
        };
        let external = String::from_utf8(bytes).map_err(DocumentError::InvalidUtf8)?;
        if external == self.saved_text {
            return Ok(DocumentChange::Unchanged);
        }
        if self.dirty {
            self.conflict = true;
            return Ok(DocumentChange::Conflict);
        }
        self.text = external.clone();
        self.saved_text = external;
        self.deleted = false;
        Ok(DocumentChange::Reloaded)
    }

    /// Atomically writes the current text and clears dirty/conflict state.
    pub fn save(&mut self) -> Result<(), DocumentError> {
        if self.conflict {
            return Err(DocumentError::Conflict);
        }
        if self.deleted {
            return Err(DocumentError::Deleted);
        }
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document");
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let temp = parent.join(format!(".{name}.tiller-{}-{nonce}-tmp", std::process::id()));
        std::fs::write(&temp, self.text.as_bytes())?;
        let file = std::fs::File::open(&temp)?;
        file.sync_all()?;
        std::fs::rename(&temp, &self.path)?;
        self.saved_text = self.text.clone();
        self.dirty = false;
        self.conflict = false;
        self.deleted = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> (std::path::PathBuf, String) {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "tiller-markdown-doc-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.md");
        fs::write(&path, "before\n").unwrap();
        (path, dir.to_string_lossy().into_owned())
    }

    #[test]
    fn saves_atomically_and_reloads_clean_external_changes() {
        let (path, dir) = fixture();
        let mut document = MarkdownDocument::load(&path).unwrap();
        document.set_text("local\n");
        assert!(document.is_dirty());
        document.save().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "local\n");
        fs::write(&path, "external\n").unwrap();
        assert_eq!(
            document.refresh_from_disk().unwrap(),
            DocumentChange::Reloaded
        );
        assert_eq!(document.text(), "external\n");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dirty_external_changes_become_conflict_and_deletion_is_explicit() {
        let (path, dir) = fixture();
        let mut document = MarkdownDocument::load(&path).unwrap();
        document.set_text("local\n");
        fs::write(&path, "external\n").unwrap();
        assert_eq!(
            document.refresh_from_disk().unwrap(),
            DocumentChange::Conflict
        );
        assert!(document.has_conflict());
        assert!(matches!(document.save(), Err(DocumentError::Conflict)));
        fs::remove_file(&path).unwrap();
        assert_eq!(
            document.refresh_from_disk().unwrap(),
            DocumentChange::Deleted
        );
        assert!(document.is_deleted());
        let _ = fs::remove_dir_all(dir);
    }
}
