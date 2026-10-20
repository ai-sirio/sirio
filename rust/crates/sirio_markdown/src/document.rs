use crate::LineEnding;
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
///
/// The text is LF-only; the line ending the disk used is kept beside it and
/// applied on save. The document is dirty when the text differs from the
/// LF-normalized disk text, or when the ending differs from the disk's.
#[derive(Debug)]
pub struct MarkdownDocument {
    path: PathBuf,
    text: String,
    saved_text: String,
    line_ending: LineEnding,
    saved_line_ending: LineEnding,
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
        let raw = std::fs::read(&path)
            .map_err(DocumentError::Io)
            .and_then(|bytes| String::from_utf8(bytes).map_err(DocumentError::InvalidUtf8))?;
        let line_ending = LineEnding::detect(&raw);
        let text = LineEnding::normalize(&raw);
        Ok(Self {
            path,
            saved_text: text.clone(),
            text,
            line_ending,
            saved_line_ending: line_ending,
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

    /// The ending the next save writes.
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
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

    /// Replaces the text. `text` must be LF-only; the caller normalizes it.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.recompute_dirty();
    }

    /// Chooses the ending the next save writes. Changing it away from the
    /// disk's ending is an unsaved edit.
    pub fn set_line_ending(&mut self, line_ending: LineEnding) {
        self.line_ending = line_ending;
        self.recompute_dirty();
    }

    fn recompute_dirty(&mut self) {
        self.dirty = self.text != self.saved_text || self.line_ending != self.saved_line_ending;
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
        let raw = String::from_utf8(bytes).map_err(DocumentError::InvalidUtf8)?;
        if LineEnding::matches(&raw, &self.saved_text, self.saved_line_ending) {
            self.conflict = false;
            self.deleted = false;
            return Ok(DocumentChange::Unchanged);
        }
        if self.dirty {
            self.conflict = true;
            self.deleted = false;
            return Ok(DocumentChange::Conflict);
        }
        let external = LineEnding::normalize(&raw);
        let external_line_ending = LineEnding::detect(&raw);
        self.text = external.clone();
        self.saved_text = external;
        self.line_ending = external_line_ending;
        self.saved_line_ending = external_line_ending;
        self.deleted = false;
        Ok(DocumentChange::Reloaded)
    }

    /// Atomically writes the current text, with the line ending applied, and
    /// clears dirty/conflict state.
    pub fn save(&mut self) -> Result<(), DocumentError> {
        if self.conflict {
            return Err(DocumentError::Conflict);
        }
        write_atomically(&self.path, self.line_ending.apply(&self.text).as_bytes())?;
        self.saved_text = self.text.clone();
        self.saved_line_ending = self.line_ending;
        self.dirty = false;
        self.conflict = false;
        self.deleted = false;
        Ok(())
    }

    /// Dismisses an external-change banner while preserving the local text.
    /// The current disk content becomes the new comparison baseline; if the
    /// file is still deleted, the old baseline remains so a later save can
    /// recreate it.
    pub fn keep_external(&mut self) {
        if let Ok(bytes) = std::fs::read(&self.path)
            && let Ok(raw) = String::from_utf8(bytes)
        {
            self.saved_text = LineEnding::normalize(&raw);
            self.saved_line_ending = LineEnding::detect(&raw);
            self.deleted = false;
        }
        self.conflict = false;
        self.recompute_dirty();
    }
}

/// Writes `bytes` to `path` by a temporary sibling renamed over it, keeping the
/// mode of the file it replaces. A file that did not exist is created with the
/// default mode.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let temp = parent.join(format!(".{name}.sirio-{}-{nonce}-tmp", std::process::id()));
    let finish = || -> std::io::Result<()> {
        std::fs::write(&temp, bytes)?;
        // The sync handle needs write access: Windows rejects flushing a
        // read-only handle with os error 5 (access denied — FlushFileBuffers
        // demands GENERIC_WRITE), while Unix fsync accepts any open mode,
        // which is why the plain `File::open` spelling worked there and
        // only failed on Windows. The handle is scoped to end before the
        // rename so no handle is live across it on any platform. The
        // `fs::write` handle above is already closed — it was dropped when
        // that call returned. Any failure from the write on removes the
        // temporary file below: a partial one would sit in the worktree as an
        // untracked file.
        {
            let file = std::fs::OpenOptions::new().write(true).open(&temp)?;
            file.sync_all()?;
        }
        // The temporary file has the default mode; the file it replaces may
        // not (an executable script). A path that does not exist yet has no
        // mode to carry over. `metadata` follows a symlink, so a link's
        // target's mode is the one kept — the rename itself still replaces
        // the link with a regular file.
        #[cfg(unix)]
        if let Ok(metadata) = std::fs::metadata(path) {
            std::fs::set_permissions(&temp, metadata.permissions())?;
        }
        std::fs::rename(&temp, path)
    };
    let result = finish();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> (std::path::PathBuf, String) {
        let dir = crate::scratch_dir("sirio-markdown-doc");
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

    #[test]
    fn deleted_document_can_be_saved_again_after_external_removal() {
        let (path, dir) = fixture();
        let mut document = MarkdownDocument::load(&path).unwrap();
        document.set_text("recreated\n");
        fs::remove_file(&path).unwrap();

        assert_eq!(
            document.refresh_from_disk().unwrap(),
            DocumentChange::Deleted
        );
        document.save().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "recreated\n");
        assert!(!document.is_deleted());
        assert!(!document.is_dirty());
        let _ = fs::remove_dir_all(dir);
    }

    /// Failure list, written before the code:
    ///
    /// - the temp-and-rename gives a replaced executable the default mode, so a
    ///   script loses its exec bit (`keeps_the_mode_of_the_file_it_replaces`);
    /// - a path that does not exist yet has no mode to copy and the write
    ///   errors instead of creating it (`creates_a_file_that_did_not_exist`);
    /// - a write that fails leaves its temporary sibling behind, an untracked
    ///   file in the worktree (`a_failed_write_leaves_no_temporary_file`; only
    ///   the rename can be made to fail deterministically, the write's own
    ///   failure, a full disk, is covered by the same cleanup scope).
    #[cfg(unix)]
    #[test]
    fn keeps_the_mode_of_the_file_it_replaces() {
        use std::os::unix::fs::PermissionsExt;
        let dir = crate::scratch_dir("sirio-markdown-atomic-mode");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("run.sh");
        fs::write(&path, "old\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();

        write_atomically(&path, b"new\n").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new\n");
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn creates_a_file_that_did_not_exist() {
        let dir = crate::scratch_dir("sirio-markdown-atomic-new");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fresh.txt");

        write_atomically(&path, b"hello\n").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "hello\n");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_write_leaves_no_temporary_file() {
        let dir = crate::scratch_dir("sirio-markdown-atomic-fail");
        fs::create_dir_all(&dir).unwrap();
        // A file cannot be renamed over a directory, on any platform.
        let target = dir.join("target");
        fs::create_dir(&target).unwrap();

        assert!(write_atomically(&target, b"x").is_err());

        let left: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(left, vec![std::ffi::OsString::from("target")]);
        let _ = fs::remove_dir_all(dir);
    }
}
