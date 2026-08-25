//! The document editor, headless.
//!
//! P34 replaces the read-only file viewer with an editor. All of the
//! editor's *substance* — load, edit, save, conflict detection, dirty
//! state, dedupe, language detection, the Markdown formatting operations —
//! lives here as pure, synchronous code over a [`String`] buffer and a
//! byte-offset [`Selection`]. Nothing in this module touches GPUI, so every
//! entry in `01-inventory-app.md`'s "Documents and editors" section that is
//! not about pixels is testable without a display. The pixel-bound half
//! (caret, scrolling, the toolbar) stays in `file_view.rs` and is
//! `NOT EXERCISED — blocked on display`.
//!
//! # The state machine
//!
//! ```text
//!                    ┌──────────────────────────────────────────┐
//!                    │        Editor (buffer + metadata)        │
//!                    └──────────────────────────────────────────┘
//!   open(path) ──► status: Loaded | Missing | Unreadable | Binary | TooLarge
//!   buffer ──► snapshot (the last on-disk content we synced from)
//!   edit ops ──► dirty = buffer != snapshot        (F-EDIT-04, F-TAB-16)
//!   check_external() ──► conflict = ChangedOnDisk | DeletedOnDisk
//!        (compares disk against snapshot — F-EDIT-05, F-EDIT-06)
//!   reload()  ──► buffer = disk, dirty = false     (F-EDIT-05 "Reload")
//!   keep()    ──► snapshot = disk, buffer kept     (F-EDIT-05 "Keep")
//!   save()    ──► disk = buffer, dirty = false     (F-EDIT-04/06)
//! ```
//!
//! Three facts make the conflict machinery honest rather than decorative:
//!
//! - `check_external` *reads the file again* — a conflict is only ever
//!   reported from a live comparison against the disk, never from a
//!   timestamp guess.
//! - `reload` and `keep` both leave the editor consistent afterwards:
//!   Reload adopts the disk content wholesale; Keep adopts it as the new
//!   `snapshot` (so a *second* external change re-flags) while keeping the
//!   user's buffer, which stays dirty until a save clobbers the disk.
//! - `save` writes the buffer unconditionally and can therefore *recreate*
//!   a file deleted while open (F-EDIT-06); when the write fails it leaves
//!   a visible `save_error` instead of swallowing the failure.
//!
//! # What maps to which inventory entry
//!
//! - F-EDIT-04 (edit and save survives reopen) — [`Editor::replace`] +
//!   [`Editor::save`] + a fresh [`Editor::open`] of the same path.
//! - F-EDIT-05 (changed on disk; Reload and Keep) — [`Editor::check_external`],
//!   [`Editor::reload`], [`Editor::keep`].
//! - F-EDIT-06 (deleted externally; save recreates or errors) —
//!   `Conflict::DeletedOnDisk` + [`Editor::save`].
//! - F-EDIT-07 (language detection, plain-text fallback, no wrapping,
//!   four-space indent) — [`Language::from_path`], [`INDENT_UNIT`],
//!   [`CODE_EDITOR_WRAPS`], [`Editor::insert_indent`].
//! - F-EDIT-08 (no duplicate documents) — [`DocumentRegistry`].
//! - F-EDIT-02 (Markdown formatting) — [`Editor::format_bold`],
//!   [`Editor::format_italic`], [`Editor::toggle_heading`],
//!   [`Editor::toggle_list`], [`Editor::make_link`].
//! - F-EDIT-03 (large-file manual preview) — [`MARKDOWN_PREVIEW_THRESHOLD`],
//!   [`Editor::preview_locked`], [`Editor::request_preview`].
//! - F-EDIT-13 (distinct missing/unreadable messages) — [`LoadStatus::Missing`]
//!   vs [`LoadStatus::Unreadable`] and [`Editor::load_message`].
//! - F-EDIT-10/11 (platform adaptations) — [`fs_actions::reveal_command`]
//!   (Finder reveal on macOS or directory open on Linux) and
//!   [`fs_actions::copy_path_text`] (the clipboard derivation; the X11
//!   clipboard write itself needs a display and is not exercised).

use std::path::{Path, PathBuf};
use tiller_markdown::{DocumentChange, DocumentError, MarkdownDocument};

/// Largest file the editor will load at all. Refusing a bigger file keeps a
/// click in the Files tree from allocating an unbounded string on the UI
/// process; the notice is rendered in place of the content. Mirrors the
/// read-only viewer's ceiling (`file_view.rs` before P34) — same policy,
/// now owned by the editor model so the view and any future callers share
/// one constant.
pub const MAX_FILE_BYTES: u64 = 1_048_576;

/// Markdown files larger than this do not auto-render their preview; they
/// open as editable source with a "Large file — manual preview" state
/// (F-EDIT-03) until [`Editor::request_preview`] is called. The threshold
/// exists because rendering a huge document through the markdown parser on
/// every keystroke is what the manual-preview affordance exists to avoid —
/// the file itself still loads fine (it is under [`MAX_FILE_BYTES`]).
pub const MARKDOWN_PREVIEW_THRESHOLD: u64 = 256 * 1024;

/// Code files do not wrap (F-EDIT-07): one line of source is one visual
/// line, and the horizontal scroller owns the overflow. This is a behavior
/// the editor model states so the pixel layer cannot drift from it.
pub const CODE_EDITOR_WRAPS: bool = false;

/// Four-space indentation (F-EDIT-07): pressing Tab in a code file inserts
/// exactly this unit. The Markdown editor's Tab does nothing special.
pub const INDENT_UNIT: &str = "    ";

/// The Markdown source syntax used by the formatting operations.
mod md {
    pub const BOLD: &str = "**";
    pub const ITALIC: &str = "*";
    pub const HEADING: &str = "# ";
    pub const LIST: &str = "- ";
}

/// A language detected from a file path (F-EDIT-07).
///
/// Detection is purely extension-based — the editor does not sniff content.
/// Anything without a known extension resolves to [`Language::PlainText`],
/// which is the fallback the inventory entry requires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    PlainText,
    Markdown,
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Shell,
    Json,
    Yaml,
    Toml,
    C,
    Cpp,
    Go,
    Swift,
    Kotlin,
    Java,
    Ruby,
    Php,
    Html,
    Css,
    Sql,
    Xml,
    Lua,
    Zig,
}

impl Language {
    /// Resolves a path's language from its extension, case-insensitively,
    /// falling back to plain text for unknown or absent extensions.
    pub fn from_path(path: &Path) -> Language {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match extension.as_str() {
            "md" | "markdown" => Language::Markdown,
            "rs" => Language::Rust,
            "py" | "pyw" => Language::Python,
            "js" | "mjs" | "cjs" | "jsx" => Language::JavaScript,
            "ts" | "mts" | "cts" | "tsx" => Language::TypeScript,
            "sh" | "bash" | "zsh" | "fish" => Language::Shell,
            "json" => Language::Json,
            "yaml" | "yml" => Language::Yaml,
            "toml" => Language::Toml,
            "c" | "h" => Language::C,
            "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => Language::Cpp,
            "go" => Language::Go,
            "swift" => Language::Swift,
            "kt" | "kts" => Language::Kotlin,
            "java" => Language::Java,
            "rb" => Language::Ruby,
            "php" => Language::Php,
            "html" | "htm" => Language::Html,
            "css" => Language::Css,
            "sql" => Language::Sql,
            "xml" => Language::Xml,
            "lua" => Language::Lua,
            "zig" => Language::Zig,
            _ => Language::PlainText,
        }
    }

    /// The human-readable name shown in the editor's status line.
    pub fn name(&self) -> &'static str {
        match self {
            Language::PlainText => "plain text",
            Language::Markdown => "Markdown",
            Language::Rust => "Rust",
            Language::Python => "Python",
            Language::JavaScript => "JavaScript",
            Language::TypeScript => "TypeScript",
            Language::Shell => "Shell",
            Language::Json => "JSON",
            Language::Yaml => "YAML",
            Language::Toml => "TOML",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::Go => "Go",
            Language::Swift => "Swift",
            Language::Kotlin => "Kotlin",
            Language::Java => "Java",
            Language::Ruby => "Ruby",
            Language::Php => "PHP",
            Language::Html => "HTML",
            Language::Css => "CSS",
            Language::Sql => "SQL",
            Language::Xml => "XML",
            Language::Lua => "Lua",
            Language::Zig => "Zig",
        }
    }

    pub fn is_plain_text(&self) -> bool {
        matches!(self, Language::PlainText)
    }
}

/// A half-open byte range into the buffer. Offsets are validated against
/// UTF-8 char boundaries on construction, so an editor op can never split a
/// multibyte character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub start: usize,
    pub end: usize,
}

impl Selection {
    /// A collapsed selection (a caret) at `offset`.
    pub fn point(offset: usize) -> Selection {
        Selection {
            start: offset,
            end: offset,
        }
    }

    /// Builds a selection from byte offsets, rejecting non-boundary or
    /// inverted ranges. An editor op must never receive an invalid one.
    pub fn new(buffer: &str, start: usize, end: usize) -> Option<Selection> {
        if start > end || !buffer.is_char_boundary(start) || !buffer.is_char_boundary(end) {
            return None;
        }
        Some(Selection { start, end })
    }

    pub fn is_collapsed(&self) -> bool {
        self.start == self.end
    }

    /// The selected text inside `buffer`. The selection is valid by
    /// construction, so this cannot panic.
    pub fn text<'a>(&self, buffer: &'a str) -> &'a str {
        &buffer[self.start..self.end]
    }
}

/// Why the editor is not showing editable content. Distinct from
/// [`Conflict`], which is about the file *changing* while open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadStatus {
    /// The file was read and its content sits in the buffer.
    Loaded,
    /// The path does not exist. This is a *specific* state with its own
    /// message (F-EDIT-13), not a generic read failure.
    Missing,
    /// The path exists but could not be read (permissions, a directory, …).
    /// The message carries the OS error so the surface can show it.
    Unreadable(String),
    /// NUL bytes or invalid UTF-8: not editable as text.
    Binary,
    /// Over [`MAX_FILE_BYTES`]; refused at load.
    TooLarge { size: u64 },
}

/// The editor's answer to "did the file change under us?" — set by
/// [`Editor::check_external`], which re-reads the disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conflict {
    /// Disk matches the snapshot we synced from.
    None,
    /// The file's content on disk differs from the snapshot (F-EDIT-05).
    /// The surface offers Reload and Keep; both are wired.
    ChangedOnDisk,
    /// The file was deleted while open (F-EDIT-06). The buffer is retained
    /// and a save recreates the file.
    DeletedOnDisk,
}

/// How disk reads fail; lets the load path tell "gone" from "unreadable"
/// — the two distinct messages F-EDIT-13 requires.
#[derive(Debug)]
enum ReadError {
    NotFound,
    Other(std::io::Error),
}

impl ReadError {
    fn message(&self) -> String {
        match self {
            ReadError::NotFound => "file not found".to_string(),
            ReadError::Other(error) => error.to_string(),
        }
    }
}

fn read_disk(path: &Path) -> Result<Vec<u8>, ReadError> {
    std::fs::read(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ReadError::NotFound
        } else {
            ReadError::Other(error)
        }
    })
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0) || String::from_utf8(bytes.to_vec()).is_err()
}

/// The headless document: buffer, the on-disk state we know about, and the
/// derived dirty/conflict flags. See the module docs for the state machine.
///
/// The type owns no resources — every method is synchronous — so it is
/// `Send` and can be constructed on a background executor by the view.
#[derive(Debug)]
pub struct Editor {
    path: PathBuf,
    /// The markdown crate owns the live disk snapshot for files that loaded
    /// successfully. The scalar fields below remain the fallback for a
    /// missing/unreadable synthetic editor created with `from_buffer`.
    document: Option<MarkdownDocument>,
    buffer: String,
    /// The last on-disk content we synced from: the file's content at open,
    /// after a reload, or after a save. `None` when the file has never been
    /// successfully read (missing or unreadable at open).
    snapshot: Option<String>,
    status: LoadStatus,
    conflict: Conflict,
    save_error: Option<String>,
    language: Language,
    /// F-EDIT-03: a Markdown file over [`MARKDOWN_PREVIEW_THRESHOLD`] opens
    /// with its preview locked (source shown, no auto-render) until
    /// [`Editor::request_preview`] is called.
    preview_locked: bool,
    dirty: bool,
}

impl Editor {
    /// Reads `path` into a new editor. Never fails: a missing path becomes
    /// [`LoadStatus::Missing`] (an openable tab with a specific message),
    /// an unreadable one [`LoadStatus::Unreadable`], and so on — the caller
    /// renders whatever status results.
    pub fn open(path: &Path) -> Editor {
        let language = Language::from_path(path);
        let mut editor = Editor {
            path: path.to_path_buf(),
            document: None,
            buffer: String::new(),
            snapshot: None,
            status: LoadStatus::Loaded,
            conflict: Conflict::None,
            save_error: None,
            language,
            preview_locked: false,
            dirty: false,
        };
        match read_disk(&editor.path) {
            Ok(bytes) => {
                if bytes.len() as u64 > MAX_FILE_BYTES {
                    editor.status = LoadStatus::TooLarge {
                        size: bytes.len() as u64,
                    };
                } else if is_binary(&bytes) {
                    editor.status = LoadStatus::Binary;
                } else {
                    let text = String::from_utf8(bytes).expect("is_binary checked UTF-8");
                    editor.buffer = text.clone();
                    editor.snapshot = Some(text);
                    editor.document = Some(
                        MarkdownDocument::load(&editor.path)
                            .expect("validated text file must load as a markdown document"),
                    );
                    editor.status = LoadStatus::Loaded;
                    editor.recompute_preview_lock();
                }
            }
            Err(ReadError::NotFound) => editor.status = LoadStatus::Missing,
            Err(error) => editor.status = LoadStatus::Unreadable(error.message()),
        }
        editor
    }

    /// Builds an editor over content that is already in memory, with the
    /// disk content set to the same text (clean, no conflict). Used by the
    /// formatting-operation tests and by flows that create a fresh file
    /// rather than reading one.
    pub fn from_buffer(path: PathBuf, buffer: impl Into<String>) -> Editor {
        let buffer = buffer.into();
        let language = Language::from_path(&path);
        let mut editor = Editor {
            path,
            document: None,
            snapshot: Some(buffer.clone()),
            status: LoadStatus::Loaded,
            conflict: Conflict::None,
            save_error: None,
            language,
            preview_locked: false,
            dirty: false,
            buffer,
        };
        editor.recompute_preview_lock();
        editor
    }

    fn recompute_preview_lock(&mut self) {
        self.preview_locked = self.language == Language::Markdown
            && self.buffer.len() as u64 > MARKDOWN_PREVIEW_THRESHOLD;
    }

    fn recompute_dirty(&mut self) {
        if let Some(document) = &self.document {
            self.dirty = document.is_dirty();
            return;
        }
        self.dirty = match &self.snapshot {
            Some(disk) => self.buffer != *disk,
            // No disk baseline: the buffer is dirty once it is non-empty.
            None => !self.buffer.is_empty(),
        };
    }

    /// Mirrors the editor buffer into the crate-owned document model. Keeping
    /// this at one seam means every UI edit calls `MarkdownDocument::set_text`
    /// and the model remains the sole owner of the live dirty calculation.
    fn sync_document_from_buffer(&mut self) {
        if let Some(document) = &mut self.document {
            document.set_text(self.buffer.clone());
            self.dirty = document.is_dirty();
        } else {
            self.recompute_dirty();
        }
    }

    fn sync_buffer_from_document(&mut self) {
        if let Some(document) = &self.document {
            self.buffer = document.text().to_owned();
            self.snapshot = Some(document.text().to_owned());
            self.dirty = document.is_dirty();
        }
    }

    // ── Read-only accessors ────────────────────────────────────────────

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn buffer(&self) -> &str {
        self.document
            .as_ref()
            .map_or(&self.buffer, MarkdownDocument::text)
    }

    pub fn language(&self) -> Language {
        self.language
    }

    pub fn status(&self) -> &LoadStatus {
        &self.status
    }

    pub fn conflict(&self) -> Conflict {
        if let Some(document) = &self.document {
            if document.is_deleted() {
                Conflict::DeletedOnDisk
            } else if document.has_conflict() {
                Conflict::ChangedOnDisk
            } else {
                Conflict::None
            }
        } else {
            self.conflict
        }
    }

    pub fn save_error(&self) -> Option<&str> {
        self.save_error.as_deref()
    }

    /// The dirty flag F-TAB-16 needs: a document this tab shows is dirty
    /// when its buffer differs from the last on-disk state we synced from.
    pub fn is_dirty(&self) -> bool {
        self.document
            .as_ref()
            .map_or(self.dirty, MarkdownDocument::is_dirty)
    }

    /// F-EDIT-03: whether the Markdown preview is held back until
    /// [`Editor::request_preview`]. Always false for non-Markdown files.
    pub fn preview_locked(&self) -> bool {
        self.preview_locked
    }

    /// The specific message for the current load status (F-EDIT-13):
    /// missing and unreadable are distinct strings, never a shared
    /// "something went wrong".
    pub fn load_message(&self) -> Option<String> {
        match &self.status {
            LoadStatus::Loaded => None,
            LoadStatus::Missing => {
                Some(format!("This file does not exist: {}", self.path.display()))
            }
            LoadStatus::Unreadable(error) => Some(format!(
                "This file cannot be read ({}): {}",
                error,
                self.path.display()
            )),
            LoadStatus::Binary => {
                Some("This file appears to be binary and cannot be shown as text.".to_string())
            }
            LoadStatus::TooLarge { size } => Some(format!(
                "This file is too large to open ({size} bytes; limit is {MAX_FILE_BYTES} bytes)."
            )),
        }
    }

    // ── Editing ────────────────────────────────────────────────────────

    /// Replaces `range` with `text` (an empty range inserts), then
    /// recomputes dirty state. Rejects selections that would split a
    /// multibyte character. F-EDIT-04's edit half.
    pub fn replace(&mut self, range: Selection, text: &str) -> Result<(), String> {
        if !self.buffer.is_char_boundary(range.start) || !self.buffer.is_char_boundary(range.end) {
            return Err("selection splits a multibyte character".to_string());
        }
        self.buffer.replace_range(range.start..range.end, text);
        self.sync_document_from_buffer();
        Ok(())
    }

    /// Inserts `text` at a caret offset. F-EDIT-04's edit half, for the
    /// common "type at the caret" case.
    pub fn insert(&mut self, at: usize, text: &str) -> Result<(), String> {
        self.replace(Selection::point(at), text)
    }

    /// Inserts the four-space indent unit (F-EDIT-07) at a caret offset.
    pub fn insert_indent(&mut self, at: usize) -> Result<(), String> {
        self.insert(at, INDENT_UNIT)
    }

    // ── External change (F-EDIT-05 / F-EDIT-06) ────────────────────────

    /// Re-reads the file and compares it against the snapshot. This is the
    /// only way a conflict is ever reported — a banner that appears without
    /// a real disk comparison is a lie, and the tests below write the file
    /// from outside the editor to prove the states are reachable.
    pub fn check_external(&mut self) -> Conflict {
        if self.document.is_some() {
            let (reloaded_text, dirty, conflict, error) = {
                let document = self.document.as_mut().expect("document checked above");
                let change = document.refresh_from_disk();
                let reloaded_text = matches!(change, Ok(DocumentChange::Reloaded))
                    .then(|| document.text().to_owned());
                let error = change.as_ref().err().map(ToString::to_string);
                let conflict = if document.is_deleted() {
                    Conflict::DeletedOnDisk
                } else if document.has_conflict() {
                    Conflict::ChangedOnDisk
                } else {
                    Conflict::None
                };
                (reloaded_text, document.is_dirty(), conflict, error)
            };
            if let Some(text) = reloaded_text {
                self.buffer = text.clone();
                self.snapshot = Some(text);
            }
            self.dirty = dirty;
            if let Some(error) = error {
                self.save_error = Some(error);
            }
            self.conflict = conflict;
            return conflict;
        }
        self.conflict = match (&self.snapshot, read_disk(&self.path)) {
            (Some(disk), Ok(current)) if current == disk.as_bytes() => Conflict::None,
            (Some(_), Ok(_)) => Conflict::ChangedOnDisk,
            // The file we had is gone: this is the recreate-on-save case,
            // not a generic conflict.
            (Some(_), Err(ReadError::NotFound)) => Conflict::DeletedOnDisk,
            // Exists but unreadable now: we cannot compare, so the honest
            // answer is that it is no longer what we knew.
            (Some(_), Err(_)) => Conflict::ChangedOnDisk,
            // No baseline (Missing/Unreadable/Binary/TooLarge at open, or a
            // save that never succeeded): there is nothing to conflict with.
            (None, _) => Conflict::None,
        };
        self.conflict
    }

    /// F-EDIT-05 "Reload": discard the buffer and adopt the on-disk
    /// content, clearing dirty and conflict. Fails only when the file
    /// cannot be read back (deleted, too large, binary) — the caller shows
    /// the error instead of a banner that resolved into nothing.
    pub fn reload(&mut self) -> Result<(), String> {
        let bytes = read_disk(&self.path).map_err(|error| error.message())?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err("the file grew beyond the load limit".to_string());
        }
        if is_binary(&bytes) {
            return Err("the file on disk is no longer text".to_string());
        }
        let text = String::from_utf8(bytes).expect("is_binary checked UTF-8");
        self.document =
            Some(MarkdownDocument::load(&self.path).map_err(|error| error.to_string())?);
        self.buffer = text.clone();
        self.snapshot = Some(text);
        self.status = LoadStatus::Loaded;
        self.conflict = Conflict::None;
        self.dirty = false;
        self.recompute_preview_lock();
        Ok(())
    }

    /// F-EDIT-05 "Keep": dismiss the banner and treat the *current* disk
    /// content as the new baseline, while keeping the user's buffer. Dirty
    /// stays true (the buffer still differs from disk), so a later save
    /// overwrites the external change knowingly. A second external change
    /// re-flags, because the snapshot moved forward.
    pub fn keep(&mut self) {
        if let Some(document) = &mut self.document {
            document.keep_external();
            self.dirty = document.is_dirty();
            self.conflict = Conflict::None;
            return;
        }
        if let Ok(bytes) = read_disk(&self.path)
            && let Ok(text) = String::from_utf8(bytes)
        {
            self.snapshot = Some(text);
        }
        self.conflict = Conflict::None;
        self.recompute_dirty();
    }

    /// F-EDIT-04/06: write the buffer to the path — recreating the file if
    /// it was deleted externally (F-EDIT-06's first half). On failure the
    /// error is returned *and* kept in [`Editor::save_error`], so the
    /// surface can show a visible error rather than silently losing the
    /// save (F-EDIT-06's second half).
    pub fn save(&mut self) -> Result<(), String> {
        // A file refused at load never entered the buffer; writing the empty
        // buffer over it would silently truncate it. Refuse instead.
        if matches!(
            self.status,
            LoadStatus::Binary | LoadStatus::TooLarge { .. }
        ) {
            let message = "this file was not loaded as text and cannot be overwritten".to_string();
            self.save_error = Some(message.clone());
            return Err(message);
        }
        if let Some(document) = &mut self.document {
            match document.save() {
                Ok(()) => {
                    self.sync_buffer_from_document();
                    self.status = LoadStatus::Loaded;
                    self.conflict = Conflict::None;
                    self.save_error = None;
                    return Ok(());
                }
                Err(error) => {
                    let message = match error {
                        DocumentError::Conflict => {
                            "the file changed on disk; choose Reload or Keep before saving"
                                .to_string()
                        }
                        other => other.to_string(),
                    };
                    self.save_error = Some(message.clone());
                    return Err(message);
                }
            }
        }
        match std::fs::write(&self.path, self.buffer.as_bytes()) {
            Ok(()) => {
                self.snapshot = Some(self.buffer.clone());
                self.status = LoadStatus::Loaded;
                self.conflict = Conflict::None;
                self.save_error = None;
                self.dirty = false;
                Ok(())
            }
            Err(error) => {
                let message = error.to_string();
                self.save_error = Some(message.clone());
                Err(message)
            }
        }
    }

    /// F-EDIT-03: unlock the Markdown preview for a large file.
    pub fn request_preview(&mut self) {
        self.preview_locked = false;
    }

    // ── Markdown formatting (F-EDIT-02) ────────────────────────────────
    //
    // Pure text transformations over buffer + selection: given a selection
    // they rewrite the buffer and return where the selection should now be,
    // so the toolbar can place the caret/selection without knowing anything
    // about Markdown. All are toggles where a natural one exists: wrapping
    // an already-wrapped selection unwraps it, and prefixing an already
    // prefixed set of lines strips the prefix.

    /// Wraps the selection in `**` (bold); unwraps if already wrapped;
    /// inserts `****` with the caret centered for a collapsed selection.
    pub fn format_bold(&mut self, selection: Selection) -> Selection {
        self.toggle_wrap(selection, md::BOLD)
    }

    /// Wraps the selection in `*` (italic); unwraps if already wrapped;
    /// inserts `**` with the caret centered for a collapsed selection.
    pub fn format_italic(&mut self, selection: Selection) -> Selection {
        self.toggle_wrap(selection, md::ITALIC)
    }

    fn toggle_wrap(&mut self, selection: Selection, marker: &str) -> Selection {
        let result = if selection.is_collapsed() {
            let position = selection.start;
            let doubled = format!("{marker}{marker}");
            self.buffer.insert_str(position, &doubled);
            // Caret sits between the two markers, ready to type.
            Selection::point(position + marker.len())
        } else if self.wrapped_in(selection, marker) {
            let (start, end) = (selection.start - marker.len(), selection.end + marker.len());
            let inner = self.buffer[selection.start..selection.end].to_string();
            self.buffer.replace_range(start..end, &inner);
            Selection::new(&self.buffer, start, start + inner.len())
                .expect("unwrap keeps boundary alignment")
        } else {
            let (start, end) = (selection.start, selection.end);
            self.buffer.insert_str(end, marker);
            self.buffer.insert_str(start, marker);
            Selection::new(&self.buffer, start + marker.len(), end + marker.len())
                .expect("wrap keeps boundary alignment")
        };
        self.sync_document_from_buffer();
        result
    }

    fn wrapped_in(&self, selection: Selection, marker: &str) -> bool {
        selection.start >= marker.len()
            && selection.end + marker.len() <= self.buffer.len()
            && self
                .buffer
                .get(selection.start - marker.len()..selection.start)
                == Some(marker)
            && self.buffer.get(selection.end..selection.end + marker.len()) == Some(marker)
    }

    fn toggle_line_prefix(&mut self, selection: Selection, prefix: &str) -> Selection {
        let range = whole_lines_range(&self.buffer, selection);
        let lines: Vec<&str> = self.buffer[range.clone()].split('\n').collect();
        let all_prefixed = !lines.is_empty() && lines.iter().all(|line| line.starts_with(prefix));
        let transformed: Vec<String> = if all_prefixed {
            lines
                .iter()
                .map(|line| line.strip_prefix(prefix).unwrap_or(line).to_string())
                .collect()
        } else {
            lines.iter().map(|line| format!("{prefix}{line}")).collect()
        };
        let new_text = transformed.join("\n");
        self.buffer.replace_range(range.clone(), &new_text);
        let result = Selection::new(&self.buffer, range.start, range.start + new_text.len())
            .expect("prefix toggling keeps boundary alignment");
        self.sync_document_from_buffer();
        result
    }

    /// Toggles `# ` on each line covered by the selection: adds the prefix
    /// unless *every* covered line already has it, in which case the prefix
    /// is stripped. The selection is expanded to whole lines and returned
    /// as such, so the toolbar can keep the block selected.
    pub fn toggle_heading(&mut self, selection: Selection) -> Selection {
        self.toggle_line_prefix(selection, md::HEADING)
    }

    /// Toggles `- ` on each line covered by the selection (list item), with
    /// the same all-or-nothing semantics as [`Editor::toggle_heading`].
    pub fn toggle_list(&mut self, selection: Selection) -> Selection {
        self.toggle_line_prefix(selection, md::LIST)
    }

    /// Wraps the selection as a Markdown link `[label](url)`. With a
    /// collapsed selection, inserts `[](url)` and places the caret inside
    /// the brackets; with a selection, keeps the label selected. The URL is
    /// the toolbar's input (the toolbar owns any prompting); this function
    /// only ever rearranges text.
    pub fn make_link(&mut self, selection: Selection, url: &str) -> Selection {
        let result = if selection.is_collapsed() {
            let position = selection.start;
            let inserted = format!("[]({url})");
            self.buffer.insert_str(position, &inserted);
            Selection::point(position + 1)
        } else {
            let (start, end) = (selection.start, selection.end);
            let label = self.buffer[start..end].to_string();
            let inserted = format!("[{label}]({url})");
            self.buffer.replace_range(start..end, &inserted);
            Selection::new(&self.buffer, start + 1, start + 1 + label.len())
                .expect("link wrapping keeps boundary alignment")
        };
        self.sync_document_from_buffer();
        result
    }
}

/// An inline Markdown link `[label](target)` found in a line of source
/// text: the label's byte range within *that line* (what a click should
/// land on, F-CORE-FILE-04) and the raw target text between the parens
/// (what [`Selection`]-free code hands to a link resolver).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownLinkSpan {
    pub label_range: std::ops::Range<usize>,
    pub target: String,
}

/// Finds inline `[label](target)` links in one line of Markdown source, for
/// the code surface's click-to-open affordance (F-CORE-FILE-04). Image
/// syntax (`![alt](src)`) is skipped — an image reference is not something
/// the file view can open as a document. Reference-style links
/// (`[label][ref]`) are out of scope: the toolbar only ever writes the
/// inline form (see [`Editor::make_link`]), so that is what this looks for.
pub fn markdown_links_in_line(line: &str) -> Vec<MarkdownLinkSpan> {
    let bytes = line.as_bytes();
    let mut spans = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'[' {
            index += 1;
            continue;
        }
        let label_start = index + 1;
        let Some(close_bracket) = line[label_start..].find(']') else {
            index += 1;
            continue;
        };
        let label_end = label_start + close_bracket;
        let after_bracket = label_end + 1;
        if bytes.get(after_bracket) != Some(&b'(') {
            index += 1;
            continue;
        }
        let target_start = after_bracket + 1;
        let Some(close_paren) = line[target_start..].find(')') else {
            index += 1;
            continue;
        };
        let target_end = target_start + close_paren;
        let is_image = index > 0 && bytes[index - 1] == b'!';
        if !is_image && label_start < label_end {
            spans.push(MarkdownLinkSpan {
                label_range: label_start..label_end,
                target: line[target_start..target_end].to_string(),
            });
        }
        index = target_end + 1;
    }
    spans
}

/// The word touching `offset` — the contiguous run of alphanumeric/`_`
/// bytes adjacent to it — for double-click word selection (F-EDIT-02).
/// `None` when `offset` sits between two non-word characters (a click on
/// whitespace or punctuation), which leaves the caret collapsed rather
/// than manufacturing an empty selection.
pub fn word_range_at(buffer: &str, offset: usize) -> Option<std::ops::Range<usize>> {
    let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
    let before = buffer[..offset].chars().next_back();
    let after = buffer[offset..].chars().next();
    if !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char) {
        return None;
    }
    let start = buffer[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_word_char(*c))
        .last()
        .map(|(index, _)| index)
        .unwrap_or(offset);
    let end = buffer[offset..]
        .char_indices()
        .take_while(|(_, c)| is_word_char(*c))
        .last()
        .map(|(index, c)| offset + index + c.len_utf8())
        .unwrap_or(offset);
    Some(start..end)
}

/// Byte range of the whole lines covered by `selection`, from the start of
/// the line containing `start` to the end (exclusive of the newline) of the
/// line containing `end`. A collapsed selection covers its own line. The
/// trailing newline of the last covered line is excluded even when the
/// selection extends past it, so toggling a prefix over a whole buffer never
/// touches an empty trailing line.
fn whole_lines_range(buffer: &str, selection: Selection) -> std::ops::Range<usize> {
    let line_start = buffer[..selection.start]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    // If the selection ends exactly at a newline, that newline terminates
    // the *previous* line — back up one char so it is not part of the range.
    let content_end = if selection.end > 0 && buffer.as_bytes()[selection.end - 1] == b'\n' {
        selection.end - 1
    } else {
        selection.end
    };
    let line_end = match buffer[content_end..].find('\n') {
        Some(index) => content_end + index,
        None => buffer.len(),
    };
    line_start..line_end
}

/// Open documents by path, for F-EDIT-08: opening the same path twice must
/// focus the existing document, never produce a second copy.
///
/// Keys are canonicalized when the path exists on disk (so `./file` and
/// `path/to/file` resolve to one document) and fall back to the literal
/// path when it does not (a missing file cannot be canonicalized). The
/// registry owns identity and focus order only — the shell owns the tabs
/// and asks the registry before creating a view.
#[derive(Debug, Default)]
pub struct DocumentRegistry {
    next_id: usize,
    documents: Vec<RegisteredDocument>,
    focused: Option<DocumentId>,
}

#[derive(Debug)]
struct RegisteredDocument {
    id: DocumentId,
    key: PathBuf,
}

/// Opaque document identity. Two `DocumentId`s are equal iff they name the
/// same registry entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DocumentId(pub usize);

/// What [`DocumentRegistry::open`] reports: a brand-new document, or an
/// existing one that should be focused instead of duplicated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenOutcome {
    New(DocumentId),
    Existing(DocumentId),
}

impl OpenOutcome {
    pub fn id(&self) -> DocumentId {
        match self {
            OpenOutcome::New(id) | OpenOutcome::Existing(id) => *id,
        }
    }
}

impl DocumentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `path` if it is not open, otherwise reports the existing
    /// document and makes it the focused one. Two opens of the same path
    /// yield exactly one document — the F-EDIT-08 proof is a registry with
    /// `len() == 1` after both calls.
    pub fn open(&mut self, path: &Path) -> OpenOutcome {
        let key = registry_key(path);
        if let Some(existing) = self.documents.iter().find(|doc| doc.key == key) {
            self.focused = Some(existing.id);
            return OpenOutcome::Existing(existing.id);
        }
        let id = DocumentId(self.next_id);
        self.next_id += 1;
        self.documents.push(RegisteredDocument { id, key });
        self.focused = Some(id);
        OpenOutcome::New(id)
    }

    /// Whether `path` is registered as open.
    pub fn contains(&self, path: &Path) -> bool {
        let key = registry_key(path);
        self.documents.iter().any(|doc| doc.key == key)
    }

    /// The id registered for `path`, if any.
    pub fn id_for(&self, path: &Path) -> Option<DocumentId> {
        let key = registry_key(path);
        self.documents
            .iter()
            .find(|doc| doc.key == key)
            .map(|doc| doc.id)
    }

    /// Removes a closed document. Returns whether it was present.
    pub fn close(&mut self, id: DocumentId) -> bool {
        let before = self.documents.len();
        self.documents.retain(|doc| doc.id != id);
        if self.focused == Some(id) {
            self.focused = self.documents.last().map(|doc| doc.id);
        }
        self.documents.len() != before
    }

    /// The most recently opened/focused document, if any.
    pub fn focused(&self) -> Option<DocumentId> {
        self.focused
    }

    pub fn len(&self) -> usize {
        self.documents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }
}

/// The registry key for a path: canonical when the path resolves (so two
/// spellings of one file dedupe), literal otherwise.
fn registry_key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Platform adaptations for file and URL actions (F-EDIT-10, F-EDIT-11).
/// Command construction is testable headlessly; the actions themselves need
/// a desktop environment and stay `NOT EXERCISED — blocked on display`.
pub mod fs_actions {
    use std::ffi::OsStr;
    use std::path::Path;
    use std::process::Command;

    /// Builds the platform command used to open a path or an external URL.
    pub fn open_command(target: &OsStr) -> Command {
        #[cfg(target_os = "macos")]
        {
            let mut command = Command::new("open");
            command.args(["--"]).arg(target);
            command
        }

        #[cfg(target_os = "linux")]
        {
            let mut command = Command::new("xdg-open");
            command.arg(target);
            command
        }

        #[cfg(target_os = "windows")]
        {
            // `rundll32 url.dll,FileProtocolHandler` is the classic
            // ShellExecute equivalent with no extra dependency: it asks
            // Explorer's handler for the file's/URL's default association,
            // the same resolution a double-click performs, and it works for
            // both local paths and http(s) URLs. The comma-bearing verb must
            // stay one argument, so it is passed as a single `arg` (never
            // shell-quoted — `Command` does not go through a shell, which is
            // exactly why `cmd /c start` is avoided: its first-quoted-arg
            // quirks are a quoting minefield for paths with spaces). The
            // spawn returns immediately, like xdg-open/open.
            let mut command = Command::new("rundll32");
            command.arg("url.dll,FileProtocolHandler").arg(target);
            command
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let mut command = Command::new("__tiller_unsupported_platform_open__");
            command.arg(target);
            command
        }
    }

    /// F-EDIT-10: builds the platform command for "Show in Finder".
    /// macOS uses `open -R <path>`; Linux opens the file's containing
    /// directory with Linux's platform opener; Windows asks Explorer to
    /// open the containing folder with the file highlighted
    /// (`/select,<path>`).
    ///
    /// Returns `None` when no parent directory exists (a bare relative
    /// filename with no current directory resolvable) on macOS/Linux/
    /// Windows, or on any other unsupported platform.
    ///
    /// On platforms with no reveal surface the body is just `None`. The
    /// `path` parameter stays named `path` everywhere (a `_path` rename
    /// would break the macOS/Linux/Windows arms' compile, the same
    /// regression the notification poster's `_payload` rename caused,
    /// which is why that fix added a Windows arm instead), so the
    /// unused-parameter warning is scoped out with an attribute.
    #[cfg_attr(
        not(any(target_os = "macos", target_os = "linux", target_os = "windows")),
        allow(unused_variables)
    )]
    pub fn reveal_command(path: &Path) -> Option<Command> {
        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().ok()?.join(path)
        };

        #[cfg(target_os = "macos")]
        {
            let mut command = Command::new("open");
            command.args(["-R", "--"]).arg(absolute);
            Some(command)
        }

        #[cfg(target_os = "linux")]
        {
            let directory = absolute.parent()?;
            let mut command = Command::new("xdg-open");
            command.arg(directory);
            Some(command)
        }

        #[cfg(target_os = "windows")]
        {
            // `explorer /select,<path>` opens the file's containing folder
            // with the file highlighted — the Explorer equivalent of
            // `open -R` / opening the parent directory. The selector must
            // be one argument (`/select,` glued to the absolute path, no
            // shell involved), and Explorer returns immediately. A
            // relative input is absolutized above so the selector names a
            // real location.
            let mut command = Command::new("explorer");
            command.arg(format!("/select,{}", absolute.display()));
            Some(command)
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            None
        }
    }

    /// F-EDIT-11, derivation half: the text a "Copy path" action would put
    /// on the clipboard. The path is absolutized so the pasted text is
    /// unambiguous regardless of where it came from; the clipboard *write*
    /// itself needs an X11/Wayland display and is not exercised.
    pub fn copy_path_text(path: &Path) -> String {
        if path.is_absolute() {
            tiller_project::display_absolute_path(path)
        } else {
            std::env::current_dir()
                .map(|dir| tiller_project::display_absolute_path(&dir.join(path)))
                .unwrap_or_else(|_| tiller_project::display_absolute_path(path))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A real file on disk that cleans itself up. The conflict and save
    /// tests depend on the file actually existing and being writable — an
    /// in-memory mock would prove nothing.
    struct TempFile(PathBuf);

    impl TempFile {
        fn new(name: &str, contents: &str) -> Self {
            Self::with_suffix(name, "", contents)
        }

        /// Like [`TempFile::new`] but the on-disk name ends with
        /// `extension` (dot included), so language detection sees it.
        fn with_extension(extension: &str, contents: &str) -> Self {
            Self::with_suffix("", &format!(".{extension}"), contents)
        }

        fn with_suffix(name: &str, extension: &str, contents: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tiller-editor-{name}-{}-{id}{extension}",
                std::process::id()
            ));
            std::fs::write(&path, contents).expect("write temporary file");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn sel(buffer: &str, start: usize, end: usize) -> Selection {
        Selection::new(buffer, start, end).expect("test selection is on char boundaries")
    }

    // ── F-EDIT-04: edit and save; the change survives reopen ───────────

    #[test]
    fn edit_save_and_reopen_keeps_the_change() {
        let file = TempFile::new("edit-save", "one\ntwo\nthree\n");
        let mut editor = Editor::open(file.path());
        assert_eq!(editor.buffer(), "one\ntwo\nthree\n");
        assert!(!editor.is_dirty());

        // Edit: replace "two" with "TWO" and append a line.
        let buffer = editor.buffer();
        let two = sel(buffer, 4, 7);
        editor.replace(two, "TWO").expect("replace");
        editor
            .insert(editor.buffer().len(), "four\n")
            .expect("append");
        assert!(editor.is_dirty(), "editing marks the document dirty");

        editor.save().expect("save writes the buffer");
        assert!(!editor.is_dirty(), "saving clears the dirty flag");

        // "Closing and reopening": a fresh editor reads the saved disk.
        let reopened = Editor::open(file.path());
        assert_eq!(reopened.buffer(), "one\nTWO\nthree\nfour\n");
        assert!(!reopened.is_dirty());
    }

    // ── F-EDIT-05: changed on disk; Reload and Keep both work ──────────

    #[test]
    fn external_change_is_detected_by_rereading_the_disk() {
        let file = TempFile::new("conflict", "original\n");
        let mut editor = Editor::open(file.path());
        assert_eq!(editor.check_external(), Conflict::None);

        // The mutation comes from *outside* the editor — a real write.
        std::fs::write(file.path(), "changed externally\n").expect("external write");
        assert_eq!(editor.check_external(), Conflict::None);
        assert_eq!(
            editor.buffer(),
            "changed externally\n",
            "a clean editor adopts the external content"
        );
    }

    #[test]
    fn reload_adopts_the_external_content_and_clears_the_conflict() {
        let file = TempFile::new("reload", "original\n");
        let mut editor = Editor::open(file.path());
        editor
            .insert(editor.buffer().len(), "user's unsaved edit\n")
            .expect("edit");
        std::fs::write(file.path(), "external\n").expect("external write");
        assert_eq!(editor.check_external(), Conflict::ChangedOnDisk);

        editor.reload().expect("reload reads the disk");
        assert_eq!(editor.buffer(), "external\n");
        assert!(!editor.is_dirty());
        assert_eq!(editor.conflict(), Conflict::None);
    }

    #[test]
    fn keep_preserves_the_buffer_and_still_saves_over_the_external_change() {
        let file = TempFile::new("keep", "original\n");
        let mut editor = Editor::open(file.path());
        editor
            .insert(editor.buffer().len(), "user's edit\n")
            .expect("edit");
        std::fs::write(file.path(), "external\n").expect("external write");
        assert_eq!(editor.check_external(), Conflict::ChangedOnDisk);

        editor.keep();
        assert_eq!(
            editor.conflict(),
            Conflict::None,
            "Keep dismisses the banner"
        );
        assert_eq!(
            editor.buffer(),
            "original\nuser's edit\n",
            "Keep keeps the user's buffer"
        );
        assert!(
            editor.is_dirty(),
            "buffer still differs from disk: dirty until saved"
        );

        // A save now knowingly clobbers the external change.
        editor.save().expect("save");
        let on_disk = std::fs::read_to_string(file.path()).expect("read back");
        assert_eq!(on_disk, "original\nuser's edit\n");
        assert!(!editor.is_dirty());
    }

    #[test]
    fn a_second_external_change_after_keep_is_flagged_again() {
        let file = TempFile::new("keep-twice", "original\n");
        let mut editor = Editor::open(file.path());
        editor
            .insert(editor.buffer().len(), "local edit\n")
            .expect("local edit");
        std::fs::write(file.path(), "first external\n").expect("external write");
        assert_eq!(editor.check_external(), Conflict::ChangedOnDisk);
        editor.keep();
        assert_eq!(editor.conflict(), Conflict::None);

        std::fs::write(file.path(), "second external\n").expect("external write");
        assert_eq!(
            editor.check_external(),
            Conflict::ChangedOnDisk,
            "Keep moved the baseline forward, so a new change re-flags"
        );
    }

    // ── F-EDIT-06: deleted externally; save recreates or errors ────────

    #[test]
    fn external_deletion_detects_and_save_recreates_the_file() {
        let file = TempFile::new("recreate", "content\n");
        let mut editor = Editor::open(file.path());
        editor
            .insert(editor.buffer().len(), "more\n")
            .expect("edit");

        std::fs::remove_file(file.path()).expect("external delete");
        assert_eq!(editor.check_external(), Conflict::DeletedOnDisk);
        assert_eq!(
            editor.buffer(),
            "content\nmore\n",
            "the buffer survives the external deletion"
        );

        editor.save().expect("save recreates the file");
        assert!(file.path().exists(), "save recreated the deleted file");
        let on_disk = std::fs::read_to_string(file.path()).expect("read back");
        assert_eq!(on_disk, "content\nmore\n");
        assert_eq!(editor.conflict(), Conflict::None);
        assert!(!editor.is_dirty());
    }

    #[test]
    fn save_that_cannot_recreate_shows_a_visible_error() {
        // Build a path whose parent directory is deleted, so the recreate
        // cannot possibly succeed — a real failure, not a mocked one.
        let doomed =
            std::env::temp_dir().join(format!("tiller-editor-doomed-{}", std::process::id()));
        std::fs::create_dir_all(&doomed).expect("create doomed dir");
        let doomed_file = doomed.join("note.txt");
        std::fs::write(&doomed_file, "content\n").expect("write");
        std::fs::remove_dir_all(&doomed).expect("remove doomed dir");
        assert!(!doomed_file.exists());

        let mut editor = Editor::open(&doomed_file);
        assert!(matches!(editor.status(), LoadStatus::Missing));
        editor
            .insert(editor.buffer().len(), "more\n")
            .expect("edit");
        let error = editor
            .save()
            .expect_err("save cannot recreate without a parent");
        assert!(!error.is_empty());
        assert_eq!(
            editor.save_error(),
            Some(error.as_str()),
            "the error is kept visible, not swallowed"
        );
        assert!(
            editor.is_dirty(),
            "a failed save leaves the buffer dirty and unsaved"
        );
    }

    // ── F-EDIT-07: language, fallback, no wrapping, four-space indent ──

    #[test]
    fn known_extensions_detect_their_language() {
        for (name, expected) in [
            ("main.rs", Language::Rust),
            ("notes.md", Language::Markdown),
            ("README.MARKDOWN", Language::Markdown),
            ("app.py", Language::Python),
            ("index.js", Language::JavaScript),
            ("component.tsx", Language::TypeScript),
            ("build.sh", Language::Shell),
            ("package.json", Language::Json),
            ("config.yaml", Language::Yaml),
            ("Cargo.toml", Language::Toml),
            ("main.c", Language::C),
            ("main.cpp", Language::Cpp),
            ("main.go", Language::Go),
            ("main.swift", Language::Swift),
            ("main.kt", Language::Kotlin),
            ("Main.java", Language::Java),
            ("script.rb", Language::Ruby),
            ("index.php", Language::Php),
            ("index.html", Language::Html),
            ("style.css", Language::Css),
            ("query.sql", Language::Sql),
            ("data.xml", Language::Xml),
            ("main.lua", Language::Lua),
            ("main.zig", Language::Zig),
        ] {
            assert_eq!(
                Language::from_path(Path::new(name)),
                expected,
                "extension map: {name}"
            );
        }
    }

    #[test]
    fn unknown_or_absent_extensions_fall_back_to_plain_text() {
        assert_eq!(
            Language::from_path(Path::new("mystery.xyzzy")),
            Language::PlainText
        );
        assert_eq!(
            Language::from_path(Path::new("README")),
            Language::PlainText
        );
        assert_eq!(
            Language::from_path(Path::new("no-extension-file")),
            Language::PlainText
        );
        assert_eq!(Language::from_path(Path::new("")), Language::PlainText);
    }

    #[test]
    fn code_editor_does_not_wrap_and_indents_with_four_spaces() {
        // Pinned at compile time so the constant cannot drift silently.
        const _: () = assert!(!CODE_EDITOR_WRAPS);
        assert_eq!(
            INDENT_UNIT.chars().count(),
            4,
            "four-space indentation (F-EDIT-07)"
        );
        assert!(
            INDENT_UNIT.chars().all(|c| c == ' '),
            "the indent unit is spaces"
        );

        let mut editor = Editor::from_buffer(PathBuf::from("code.rs"), "a\nb\n");
        editor.insert_indent(2).expect("indent"); // caret at the start of "b"
        assert_eq!(
            editor.buffer(),
            "a\n    b\n",
            "Tab inserts exactly the four-space unit"
        );
    }

    // ── F-EDIT-08: one document per path ───────────────────────────────

    #[test]
    fn two_opens_of_the_same_path_yield_one_document() {
        let file = TempFile::new("dedupe", "content\n");
        let mut registry = DocumentRegistry::new();

        let first = registry.open(file.path());
        let second = registry.open(file.path());

        match (first, second) {
            (OpenOutcome::New(first_id), OpenOutcome::Existing(second_id)) => {
                assert_eq!(first_id, second_id, "the second open returns the first id")
            }
            (first, second) => panic!("expected New then Existing, got {first:?} then {second:?}"),
        }
        assert_eq!(registry.len(), 1, "one resulting document");
        assert_eq!(registry.focused(), Some(first.id()));
        assert!(registry.contains(file.path()));
        assert_eq!(registry.id_for(file.path()), Some(first.id()));
    }

    #[cfg(unix)]
    #[test]
    fn registry_keys_are_canonical_so_aliased_paths_dedupe() {
        use std::os::unix::fs::symlink;

        let file = TempFile::new("dedupe-canon", "content\n");
        let real = std::fs::canonicalize(file.path()).expect("canonicalize");
        let alias = real
            .parent()
            .expect("parent")
            .join(format!("tiller-editor-alias-{}", std::process::id()));
        symlink(&real, &alias).expect("create symlink alias");

        let mut registry = DocumentRegistry::new();
        let first = registry.open(&real);
        let second = registry.open(&alias);
        assert!(
            matches!(
                (first, second),
                (OpenOutcome::New(a), OpenOutcome::Existing(b)) if a == b
            ),
            "a real path and its symlink alias resolve to one document"
        );
        assert_eq!(registry.len(), 1);

        let _ = std::fs::remove_file(&alias);
    }

    #[test]
    fn closing_a_document_removes_only_that_entry() {
        let first = TempFile::new("close-a", "a\n");
        let second = TempFile::new("close-b", "b\n");
        let mut registry = DocumentRegistry::new();
        let a = registry.open(first.path()).id();
        let b = registry.open(second.path()).id();
        assert_eq!(registry.len(), 2);

        assert!(registry.close(a));
        assert_eq!(registry.len(), 1);
        assert!(!registry.contains(first.path()));
        assert!(registry.contains(second.path()));
        assert!(!registry.close(a), "closing twice reports absent");
        assert_eq!(registry.focused(), Some(b));
    }

    // ── F-EDIT-02: Markdown formatting as pure text transformations ────

    #[test]
    fn bold_wraps_unwraps_and_handles_a_caret() {
        let mut editor = Editor::from_buffer(PathBuf::from("note.md"), "make this word bold\n");
        let word = sel(editor.buffer(), 10, 14); // "word"
        let after = editor.format_bold(word);
        assert_eq!(editor.buffer(), "make this **word** bold\n");
        assert_eq!(
            after,
            sel(editor.buffer(), 12, 16),
            "the word stays selected"
        );

        // Toggle: formatting an already-bold selection unwraps it.
        let inner = sel(editor.buffer(), 12, 16); // the word inside the markers
        let after = editor.format_bold(inner);
        assert_eq!(editor.buffer(), "make this word bold\n");
        assert_eq!(after, sel(editor.buffer(), 10, 14));
    }

    #[test]
    fn italic_wraps_and_a_caret_gets_an_empty_pair() {
        let mut editor = Editor::from_buffer(PathBuf::from("note.md"), "emphasise me\n");
        let word = sel(editor.buffer(), 0, 4); // "emph"
        let after = editor.format_italic(word);
        assert_eq!(editor.buffer(), "*emph*asise me\n");
        assert_eq!(after, sel(editor.buffer(), 1, 5));

        let mut caret_editor = Editor::from_buffer(PathBuf::from("note.md"), "abc\n");
        let caret = Selection::point(1);
        let after = caret_editor.format_italic(caret);
        assert_eq!(caret_editor.buffer(), "a**bc\n");
        assert_eq!(after, Selection::point(2), "caret sits between the markers");
    }

    #[test]
    fn heading_toggles_on_single_and_multiple_lines() {
        let mut editor = Editor::from_buffer(PathBuf::from("note.md"), "title\nbody\n");
        let whole = sel(editor.buffer(), 0, 10);
        let after = editor.toggle_heading(whole);
        assert_eq!(editor.buffer(), "# title\n# body\n");
        assert_eq!(
            after,
            sel(editor.buffer(), 0, 14),
            "the block stays selected"
        );

        let again = editor.toggle_heading(after);
        assert_eq!(editor.buffer(), "title\nbody\n");
        assert_eq!(
            again,
            sel(editor.buffer(), 0, 10),
            "the block stays selected"
        );

        // A caret on one line only prefixes that line.
        let mut single = Editor::from_buffer(PathBuf::from("note.md"), "a\nb\n");
        single.toggle_heading(Selection::point(3)); // the "b" line
        assert_eq!(single.buffer(), "a\n# b\n");
    }

    #[test]
    fn list_toggles_per_line() {
        let mut editor = Editor::from_buffer(PathBuf::from("note.md"), "one\ntwo\nthree\n");
        let whole = sel(editor.buffer(), 0, 14);
        editor.toggle_list(whole);
        assert_eq!(editor.buffer(), "- one\n- two\n- three\n");

        let whole_again = sel(editor.buffer(), 0, editor.buffer().len() - 1);
        editor.toggle_list(whole_again);
        assert_eq!(editor.buffer(), "one\ntwo\nthree\n");
    }

    #[test]
    fn make_link_wraps_a_selection_and_places_the_caret_for_none() {
        let mut editor = Editor::from_buffer(PathBuf::from("note.md"), "visit the docs page\n");
        let docs = sel(editor.buffer(), 10, 14); // "docs"
        let after = editor.make_link(docs, "https://example.com");
        assert_eq!(
            editor.buffer(),
            "visit the [docs](https://example.com) page\n"
        );
        assert_eq!(
            after,
            sel(editor.buffer(), 11, 15),
            "the label stays selected"
        );

        let mut caret_editor = Editor::from_buffer(PathBuf::from("note.md"), "see \n");
        let caret = Selection::point(4);
        let after = caret_editor.make_link(caret, "https://example.com");
        assert_eq!(caret_editor.buffer(), "see [](https://example.com)\n");
        assert_eq!(after, Selection::point(5), "caret is inside the brackets");
    }

    // ── F-CORE-FILE-04: rendered link spans are real click targets ─────

    #[test]
    fn markdown_links_in_line_finds_the_label_range_and_target() {
        let line = "visit the [docs](notes/setup.md:12) page";
        let spans = markdown_links_in_line(line);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].label_range, 11..15, "the bracketed label");
        assert_eq!(&line[spans[0].label_range.clone()], "docs");
        assert_eq!(spans[0].target, "notes/setup.md:12");
    }

    #[test]
    fn markdown_links_in_line_skips_images_and_finds_several() {
        let line = "![alt](img.png) then [a](a.md) and [b](b.md)";
        let spans = markdown_links_in_line(line);
        assert_eq!(
            spans
                .iter()
                .map(|span| span.target.as_str())
                .collect::<Vec<_>>(),
            vec!["a.md", "b.md"],
            "the image link is not a document to open"
        );
    }

    #[test]
    fn markdown_links_in_line_ignores_unterminated_syntax() {
        assert!(markdown_links_in_line("no links here").is_empty());
        assert!(markdown_links_in_line("[unterminated(link.md)").is_empty());
        assert!(markdown_links_in_line("[label](unterminated").is_empty());
    }

    // ── F-EDIT-02: double-click word selection ──────────────────────────

    #[test]
    fn word_range_at_finds_the_touching_word() {
        let buffer = "the quick brown fox";
        assert_eq!(word_range_at(buffer, 6), Some(4..9), "middle of 'quick'");
        assert_eq!(
            word_range_at(buffer, 4),
            Some(4..9),
            "leading edge of 'quick'"
        );
        assert_eq!(
            word_range_at(buffer, 9),
            Some(4..9),
            "trailing edge of 'quick'"
        );
    }

    #[test]
    fn word_range_at_returns_none_between_non_word_characters() {
        let buffer = "a  b";
        assert_eq!(word_range_at(buffer, 2), None, "middle of the gap");
    }

    #[test]
    fn word_range_at_keeps_multibyte_boundaries() {
        let buffer = "café au lait";
        // Click inside "é" itself would be an invalid char boundary; click
        // right after it lands on the whole word.
        assert_eq!(word_range_at(buffer, 5), Some(0..5), "end of 'café'");
    }

    #[test]
    fn formatting_never_splits_multibyte_characters() {
        // "café au lait" — é is two bytes. Offsets here are on char
        // boundaries; the wrap must stay aligned and the results valid.
        let mut editor = Editor::from_buffer(PathBuf::from("note.md"), "café au lait\n");
        let word = sel(editor.buffer(), 0, 5); // "café"
        let after = editor.format_bold(word);
        assert_eq!(editor.buffer(), "**café** au lait\n");
        assert_eq!(after, sel(editor.buffer(), 2, 7), "selection follows the é");

        // And an invalid selection is rejected, not panic'd on.
        let mut editor = Editor::from_buffer(PathBuf::from("note.md"), "é\n");
        let mid_char = Selection { start: 1, end: 1 }; // inside the é
        assert!(
            editor.insert(mid_char.start, "x").is_err(),
            "splitting a char is refused"
        );
    }

    #[test]
    fn formatting_marks_the_document_dirty() {
        let mut editor = Editor::from_buffer(PathBuf::from("note.md"), "plain\n");
        assert!(!editor.is_dirty());
        editor.format_bold(sel(editor.buffer(), 0, 5));
        assert!(editor.is_dirty(), "a formatting op is an edit");
    }

    // ── F-EDIT-03: large-file manual preview ───────────────────────────

    #[test]
    fn markdown_over_the_preview_threshold_locks_preview_until_requested() {
        let big = "x".repeat((MARKDOWN_PREVIEW_THRESHOLD as usize) + 1);
        let file = TempFile::with_extension("md", &big);
        let editor = Editor::open(file.path());
        assert_eq!(
            editor.status(),
            &LoadStatus::Loaded,
            "a big markdown file still opens (it is under MAX_FILE_BYTES)"
        );
        assert_eq!(editor.language(), Language::Markdown);
        assert!(
            editor.preview_locked(),
            "preview is held for manual preview"
        );

        let mut editor = editor;
        editor.request_preview();
        assert!(!editor.preview_locked());
    }

    #[test]
    fn small_markdown_and_large_plain_text_never_lock_preview() {
        let small = TempFile::with_extension("md", "# hi\n");
        let small_editor = Editor::open(small.path());
        assert!(
            !small_editor.preview_locked(),
            "small markdown renders automatically"
        );

        let big_text = "y".repeat((MARKDOWN_PREVIEW_THRESHOLD as usize) + 1);
        let file = TempFile::new("big-txt", &big_text);
        let editor = Editor::open(file.path());
        assert_eq!(editor.language(), Language::PlainText);
        assert!(
            !editor.preview_locked(),
            "preview locking is a markdown concept only"
        );
    }

    // ── F-EDIT-13: distinct missing and unreadable messages ────────────

    #[test]
    fn a_missing_path_and_an_unreadable_path_have_distinct_messages() {
        let missing_path = std::env::temp_dir().join(format!(
            "tiller-editor-missing-{}-never-written",
            std::process::id()
        ));
        let missing = Editor::open(&missing_path);
        assert_eq!(missing.status(), &LoadStatus::Missing);
        let missing_message = missing
            .load_message()
            .expect("a missing file has a message");
        assert!(
            missing_message.contains("does not exist"),
            "missing message names the absence: {missing_message}"
        );

        // A directory is a path that exists but cannot be read as a file.
        let dir_path = std::env::temp_dir().join(format!(
            "tiller-editor-unreadable-dir-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir_path).expect("create dir");
        let unreadable = Editor::open(&dir_path);
        assert!(
            matches!(unreadable.status(), LoadStatus::Unreadable(_)),
            "a directory is unreadable as a file"
        );
        let unreadable_message = unreadable
            .load_message()
            .expect("an unreadable file has a message");
        assert!(
            unreadable_message.contains("cannot be read"),
            "unreadable message names the read failure: {unreadable_message}"
        );

        assert_ne!(
            missing_message, unreadable_message,
            "the two failure messages must be distinct"
        );
        let _ = std::fs::remove_dir_all(&dir_path);
        let _ = std::fs::remove_file(&missing_path);
    }

    #[test]
    fn binary_and_too_large_files_keep_their_states() {
        let binary = TempFile::new("binary", "header\0payload");
        let editor = Editor::open(binary.path());
        assert_eq!(editor.status(), &LoadStatus::Binary);
        assert!(editor.load_message().is_some());

        let huge = vec![b'x'; MAX_FILE_BYTES as usize + 1];
        let file = TempFile::new("huge", &String::from_utf8(huge).expect("ascii"));
        let editor = Editor::open(file.path());
        assert!(
            matches!(editor.status(), LoadStatus::TooLarge { size } if *size == MAX_FILE_BYTES + 1)
        );
    }

    #[test]
    fn save_refuses_to_overwrite_a_file_that_never_loaded_as_text() {
        // Saving the empty buffer over a binary file would silently
        // truncate it; the model must refuse, visibly.
        let binary = TempFile::new("binary-save", "\0\0\0");
        let mut editor = Editor::open(binary.path());
        assert_eq!(editor.status(), &LoadStatus::Binary);
        let error = editor
            .save()
            .expect_err("binary files cannot be overwritten");
        assert!(!error.is_empty());
        assert_eq!(editor.save_error(), Some(error.as_str()));
        assert_eq!(
            std::fs::read(binary.path()).expect("read back"),
            b"\0\0\0",
            "the binary file is untouched"
        );
    }

    // ── F-EDIT-10 / F-EDIT-11: platform adaptations ────────────────────

    #[test]
    fn reveal_command_uses_the_platform_command() {
        let file = TempFile::new("reveal", "content\n");
        let command = fs_actions::reveal_command(file.path())
            .expect("an absolute path always has a parent directory");

        #[cfg(target_os = "macos")]
        {
            assert_eq!(command.get_program(), "open");
            let args: Vec<_> = command.get_args().collect();
            assert_eq!(
                args,
                vec![OsStr::new("-R"), OsStr::new("--"), file.path().as_os_str()]
            );
        }

        #[cfg(target_os = "linux")]
        {
            assert_eq!(command.get_program(), "xdg-open");
            let expected_dir = file.path().parent().expect("parent");
            let args: Vec<_> = command.get_args().collect();
            assert_eq!(args, vec![expected_dir.as_os_str()]);
        }

        #[cfg(target_os = "windows")]
        {
            // The selector is one glued argument (`/select,` + path), never
            // split — that is what Explorer expects.
            assert_eq!(command.get_program(), "explorer");
            let args: Vec<_> = command.get_args().collect();
            let expected = format!("/select,{}", file.path().display());
            assert_eq!(args, vec![OsStr::new(&expected)]);
        }
    }

    #[test]
    fn open_command_uses_the_platform_opener_for_paths_and_urls() {
        for target in [
            OsStr::new("/tmp/note.txt"),
            OsStr::new("https://example.com"),
        ] {
            let command = fs_actions::open_command(target);

            #[cfg(target_os = "macos")]
            {
                assert_eq!(command.get_program(), "open");
                assert_eq!(
                    command.get_args().collect::<Vec<_>>(),
                    vec![OsStr::new("--"), target]
                );
            }

            #[cfg(target_os = "linux")]
            {
                assert_eq!(command.get_program(), "xdg-open");
                assert_eq!(command.get_args().collect::<Vec<_>>(), vec![target]);
            }

            #[cfg(target_os = "windows")]
            {
                // The comma-bearing verb must stay one argument (the
                // rundll32 form is `url.dll,FileProtocolHandler <target>`,
                // never split on the comma).
                assert_eq!(command.get_program(), "rundll32");
                assert_eq!(
                    command.get_args().collect::<Vec<_>>(),
                    vec![OsStr::new("url.dll,FileProtocolHandler"), target]
                );
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn reveal_command_absolutizes_relative_paths() {
        let relative = Path::new("relative/note.txt");
        let expected = std::env::current_dir()
            .expect("current directory")
            .join(relative);
        let command = fs_actions::reveal_command(relative).expect("macOS reveal command");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(
            args,
            vec![OsStr::new("-R"), OsStr::new("--"), expected.as_os_str()]
        );
    }

    #[test]
    fn copy_path_derives_the_absolute_path_text() {
        let file = TempFile::new("copy-path", "content\n");
        let copied = fs_actions::copy_path_text(file.path());
        assert_eq!(copied, file.path().to_string_lossy());

        // A relative path is absolutized against the current directory.
        let relative = Path::new("relative/note.txt");
        let absolutized = fs_actions::copy_path_text(relative);
        // `starts_with('/')` was the unix spelling of "absolute"; on
        // Windows an absolute path starts with a drive letter or `\`, so
        // the portable spelling is the platform's own `is_absolute`.
        assert!(
            Path::new(&absolutized).is_absolute(),
            "the pasted path is absolute: {absolutized}"
        );
        // Component-wise (not byte-wise, which would split on the
        // platform's separator): the joined suffix is the relative input.
        assert!(Path::new(&absolutized).ends_with(relative));
    }
}
