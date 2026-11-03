//! The saved prompts folder, with no gpui types.
//!
//! One `.md` file per prompt holds exactly the text Copy puts on the
//! clipboard. `index.json` beside them holds the titles and the order, newest
//! first: `{"version":1,"prompts":[{"file":"<name>.md","title":"…"}]}`. File
//! names are generated here and never derived from a title.
//!
//! Every change re-reads the index from disk, applies only itself and writes
//! it back atomically, so two Sirio processes lose at most a change made at
//! the very same moment. Every read reconciles the index with the folder
//! (spec §4): an `.md` file with no entry follows the indexed ones by name,
//! with its stem as title; an entry whose file is gone is dropped; a missing
//! or unparseable index means every file is unindexed.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const INDEX: &str = "index.json";
/// How much of a file the list reads to build its two-line preview.
const PREVIEW_BYTES: u64 = 4096;

/// One prompt as the list shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptEntry {
    pub path: PathBuf,
    /// The index title, or the file stem for an unindexed file. An empty
    /// string means untitled.
    pub title: String,
}

#[derive(Serialize, Deserialize)]
struct Index {
    version: u32,
    prompts: Vec<IndexEntry>,
}

#[derive(Serialize, Deserialize)]
struct IndexEntry {
    file: String,
    title: String,
}

/// The prompts folder. Holds only its path: every call reads the disk.
pub struct PromptStore {
    dir: PathBuf,
}

impl PromptStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// An `.md` file directly inside the folder, compared lexically first
    /// and then canonicalized, so the folder reached another way still counts.
    pub fn owns(&self, path: &Path) -> bool {
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            return false;
        }
        let Some(parent) = path.parent() else {
            return false;
        };
        if parent == self.dir {
            return true;
        }
        match (fs::canonicalize(parent), fs::canonicalize(&self.dir)) {
            (Ok(parent), Ok(dir)) => parent == dir,
            _ => false,
        }
    }

    /// The prompts in display order: indexed ones first, in index order,
    /// then the unindexed ones by file name. Creates nothing.
    pub fn list(&self) -> Vec<PromptEntry> {
        let mut files = self.md_files();
        let mut listed: Vec<PromptEntry> = self
            .read_index()
            .into_iter()
            .filter(|entry| files.remove(&entry.file))
            .map(|entry| PromptEntry {
                path: self.dir.join(&entry.file),
                title: entry.title,
            })
            .collect();
        listed.extend(files.into_iter().map(|file| {
            let path = self.dir.join(&file);
            let title = stem(&path);
            PromptEntry { path, title }
        }));
        listed
    }

    /// A new empty, untitled prompt, first in the order.
    pub fn create(&self) -> io::Result<PromptEntry> {
        fs::create_dir_all(&self.dir)?;
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_millis());
        let mut attempt = 1u32;
        let (file, path) = loop {
            let file = if attempt == 1 {
                format!("prompt-{millis}.md")
            } else {
                format!("prompt-{millis}-{attempt}.md")
            };
            let path = self.dir.join(&file);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => break (file, path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => attempt += 1,
                Err(error) => return Err(error),
            }
        };
        let indexed = self.update_index(|prompts| {
            prompts.insert(
                0,
                IndexEntry {
                    file: file.clone(),
                    title: String::new(),
                },
            );
            true
        });
        if let Err(error) = indexed {
            // An unindexed empty file would list under its generated name.
            let _ = fs::remove_file(&path);
            return Err(error);
        }
        Ok(PromptEntry {
            path,
            title: String::new(),
        })
    }

    /// Sets the title of a prompt that is on disk. An unindexed file gains
    /// an entry after the indexed ones.
    pub fn retitle(&self, path: &Path, title: &str) -> io::Result<()> {
        let file = self.file_name(path)?;
        if !path.is_file() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "the prompt is not on disk"));
        }
        self.update_index(|prompts| {
            match prompts.iter_mut().find(|entry| entry.file == file) {
                Some(entry) => entry.title = title.to_owned(),
                None => prompts.push(IndexEntry {
                    file: file.clone(),
                    title: title.to_owned(),
                }),
            }
            true
        })
    }

    /// Removes the file and its entry. A file that is already gone is fine.
    pub fn delete(&self, path: &Path) -> io::Result<()> {
        let file = self.file_name(path)?;
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.update_index(|prompts| {
            let before = prompts.len();
            prompts.retain(|entry| entry.file != file);
            prompts.len() != before
        })
    }

    /// The index title, or the file stem for an unindexed file.
    pub fn title(&self, path: &Path) -> String {
        let file = path.file_name().and_then(|name| name.to_str());
        self.read_index()
            .into_iter()
            .find(|entry| Some(entry.file.as_str()) == file)
            .map_or_else(|| stem(path), |entry| entry.title)
    }

    /// The card's preview, from at most the first 4096 bytes of the file.
    pub fn read_preview(&self, path: &Path) -> String {
        let mut head = Vec::new();
        match fs::File::open(path) {
            Ok(file) => {
                if file.take(PREVIEW_BYTES).read_to_end(&mut head).is_err() {
                    return String::new();
                }
            }
            Err(_) => return String::new(),
        }
        preview(&String::from_utf8_lossy(&head))
    }

    fn md_files(&self) -> BTreeSet<String> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return BTreeSet::new();
        };
        entries
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_file())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| Path::new(name).extension().and_then(|e| e.to_str()) == Some("md"))
            .collect()
    }

    /// The index's entries; empty when it is missing or cannot be parsed.
    fn read_index(&self) -> Vec<IndexEntry> {
        fs::read(self.dir.join(INDEX))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Index>(&bytes).ok())
            .map_or_else(Vec::new, |index| index.prompts)
    }

    /// Re-reads the index from disk, applies `change` and writes it back
    /// atomically, unless `change` reports that it changed nothing.
    fn update_index(&self, change: impl FnOnce(&mut Vec<IndexEntry>) -> bool) -> io::Result<()> {
        let mut prompts = self.read_index();
        if !change(&mut prompts) {
            return Ok(());
        }
        let mut bytes = serde_json::to_vec_pretty(&Index { version: 1, prompts })
            .map_err(io::Error::other)?;
        bytes.push(b'\n');
        static WRITES: AtomicU64 = AtomicU64::new(0);
        let temp = self.dir.join(format!(
            ".index-{}-{}.tmp",
            std::process::id(),
            WRITES.fetch_add(1, Ordering::Relaxed)
        ));
        let written = (|| {
            let mut file = OpenOptions::new().write(true).create_new(true).open(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temp, self.dir.join(INDEX))
        })();
        if written.is_err() {
            let _ = fs::remove_file(&temp);
        }
        written
    }

    /// The file name of a prompt this store owns.
    fn file_name(&self, path: &Path) -> io::Result<String> {
        path.file_name()
            .and_then(|name| name.to_str())
            .filter(|_| self.owns(path))
            .map(str::to_owned)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a prompt in this folder"))
    }
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The text's first two lines after any leading blank ones, without a BOM
/// or carriage returns.
pub fn preview(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    text.lines()
        .skip_while(|line| line.trim().is_empty())
        .take(2)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    //! The ways the store can fail, written before the store:
    //!
    //! - a read creates the folder or the index (`list` on a missing folder);
    //! - a new prompt is not empty, not first, or not untitled;
    //! - two prompts made in the same millisecond share a file;
    //! - a file another program dropped in is lost, misplaced or misnamed;
    //! - an entry whose file is gone still shows;
    //! - a missing or corrupt index hides the files, or a corrupt one is never
    //!   repaired;
    //! - a non-`.md` file or a subfolder shows up as a prompt;
    //! - a change writes back a stale copy and drops another process's entry;
    //! - delete fails on a file that is already gone, or leaves its entry;
    //! - an atomic write leaves its temp file behind;
    //! - a path outside the folder, or not `.md`, counts as a prompt;
    //! - a title with quotes, emoji or `/` is mangled or leaks into a file name;
    //! - the preview shows leading blank lines, a BOM or carriage returns;
    //! - building a preview reads a whole multi-megabyte file.

    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A folder path under the system temp dir that does not exist yet, and
    /// is removed with everything in it on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            Self(std::env::temp_dir().join(format!(
                "sirio-prompts-test-{}-{unique}",
                std::process::id()
            )))
        }

        fn store(&self) -> PromptStore {
            PromptStore::new(&self.0)
        }

        fn write(&self, name: &str, contents: &str) -> PathBuf {
            std::fs::create_dir_all(&self.0).expect("create folder");
            let path = self.0.join(name);
            std::fs::write(&path, contents).expect("write file");
            path
        }

        fn index(&self) -> serde_json::Value {
            let bytes = std::fs::read(self.0.join("index.json")).expect("read index");
            serde_json::from_slice(&bytes).expect("parse index")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn file_name(path: &Path) -> &str {
        path.file_name().and_then(|n| n.to_str()).expect("utf-8 file name")
    }

    fn titles(entries: &[PromptEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.title.as_str()).collect()
    }

    fn index_with(dir: &TempDir, entries: &[(&str, &str)]) {
        let prompts: Vec<_> =
            entries.iter().map(|(file, title)| serde_json::json!({"file": file, "title": title})).collect();
        dir.write("index.json", &serde_json::json!({"version": 1, "prompts": prompts}).to_string());
    }

    #[test]
    fn list_on_a_missing_folder_is_empty_and_creates_nothing() {
        let dir = TempDir::new();
        assert!(dir.store().list().is_empty());
        assert!(!dir.0.exists());
    }

    #[test]
    fn create_writes_an_empty_md_first_in_order_with_an_empty_title() {
        let dir = TempDir::new();
        let store = dir.store();
        let entry = store.create().expect("create");
        assert_eq!(store.list(), vec![entry.clone()]);
        assert_eq!(entry.title, "");
        assert_eq!(std::fs::metadata(&entry.path).expect("metadata").len(), 0);
        assert_eq!(entry.path.extension().and_then(|e| e.to_str()), Some("md"));
        assert_eq!(entry.path.parent(), Some(dir.0.as_path()));
    }

    #[test]
    fn two_creates_in_the_same_millisecond_get_distinct_files() {
        let dir = TempDir::new();
        let store = dir.store();
        let first = store.create().expect("first");
        let second = store.create().expect("second");
        assert_ne!(first.path, second.path);
        let paths: Vec<_> = store.list().into_iter().map(|e| e.path).collect();
        assert_eq!(paths, vec![second.path, first.path]);
    }

    #[test]
    fn an_unindexed_md_follows_the_indexed_ones_by_name_with_its_stem_as_title() {
        let dir = TempDir::new();
        dir.write("a.md", "a");
        dir.write("z.md", "z");
        dir.write("b.md", "b");
        index_with(&dir, &[("a.md", "Alpha")]);
        let listed = dir.store().list();
        let names: Vec<_> = listed.iter().map(|e| file_name(&e.path)).collect();
        assert_eq!(names, vec!["a.md", "b.md", "z.md"]);
        assert_eq!(titles(&listed), vec!["Alpha", "b", "z"]);
    }

    #[test]
    fn an_entry_without_a_file_is_dropped() {
        let dir = TempDir::new();
        dir.write("kept.md", "k");
        index_with(&dir, &[("gone.md", "Gone"), ("kept.md", "Kept")]);
        let listed = dir.store().list();
        assert_eq!(titles(&listed), vec!["Kept"]);
        assert_eq!(listed[0].path, dir.0.join("kept.md"));
    }

    #[test]
    fn a_missing_index_lists_every_md_by_stem() {
        let dir = TempDir::new();
        dir.write("second.md", "2");
        dir.write("first.md", "1");
        assert_eq!(titles(&dir.store().list()), vec!["first", "second"]);
    }

    #[test]
    fn an_unparseable_index_is_treated_as_missing_and_rewritten_on_the_next_change() {
        let dir = TempDir::new();
        let a = dir.write("a.md", "a");
        dir.write("b.md", "b");
        dir.write("index.json", "{not json");
        let store = dir.store();
        assert_eq!(titles(&store.list()), vec!["a", "b"]);
        store.retitle(&a, "Alpha").expect("retitle");
        let index = dir.index();
        assert_eq!(index["prompts"][0]["file"], "a.md");
        assert_eq!(index["prompts"][0]["title"], "Alpha");
        assert_eq!(titles(&store.list()), vec!["Alpha", "b"]);
    }

    #[test]
    fn non_md_files_and_subfolders_are_ignored() {
        let dir = TempDir::new();
        dir.write("notes.txt", "t");
        dir.write("prompt.md.bak", "b");
        dir.write("real.md", "r");
        std::fs::create_dir_all(dir.0.join("folder.md")).expect("subfolder");
        std::fs::write(dir.0.join("folder.md").join("inner.md"), "i").expect("inner");
        let listed = dir.store().list();
        let names: Vec<_> = listed.iter().map(|e| file_name(&e.path)).collect();
        assert_eq!(names, vec!["real.md"]);
    }

    #[test]
    fn retitle_keeps_an_entry_added_by_another_store_meanwhile() {
        let dir = TempDir::new();
        let a = dir.store();
        let b = dir.store();
        let mine = a.create().expect("a creates");
        let theirs = b.create().expect("b creates");
        a.retitle(&mine.path, "Mine").expect("a retitles");
        let listed = a.list();
        assert_eq!(listed.len(), 2);
        assert!(listed.contains(&PromptEntry { path: theirs.path, title: String::new() }));
        assert!(listed.contains(&PromptEntry { path: mine.path, title: "Mine".into() }));
    }

    #[test]
    fn delete_removes_file_and_entry_and_tolerates_a_missing_file() {
        let dir = TempDir::new();
        let store = dir.store();
        let doomed = store.create().expect("create doomed");
        let kept = store.create().expect("create kept");
        store.delete(&doomed.path).expect("delete");
        assert!(!doomed.path.exists());
        assert_eq!(store.list(), vec![kept.clone()]);
        let files: Vec<_> = dir.index()["prompts"]
            .as_array()
            .expect("prompts array")
            .iter()
            .map(|e| e["file"].as_str().expect("file").to_owned())
            .collect();
        assert_eq!(files, vec![file_name(&kept.path).to_owned()]);

        std::fs::remove_file(&kept.path).expect("remove by hand");
        store.delete(&kept.path).expect("delete a file that is already gone");
        assert!(store.list().is_empty());
        assert!(dir.index()["prompts"].as_array().expect("prompts array").is_empty());
    }

    #[test]
    fn writes_leave_no_temp_file_behind() {
        let dir = TempDir::new();
        let store = dir.store();
        let one = store.create().expect("one");
        let two = store.create().expect("two");
        store.retitle(&one.path, "One").expect("retitle one");
        store.retitle(&two.path, "Two").expect("retitle two");
        store.delete(&one.path).expect("delete one");
        let mut names: Vec<_> = std::fs::read_dir(&dir.0)
            .expect("read folder")
            .map(|e| e.expect("entry").file_name().into_string().expect("utf-8"))
            .collect();
        names.sort();
        assert_eq!(names, vec!["index.json".to_owned(), file_name(&two.path).to_owned()]);
    }

    #[test]
    fn owns_accepts_only_md_directly_inside_the_folder() {
        let dir = TempDir::new();
        let store = dir.store();
        assert!(store.owns(&dir.0.join("prompt-1.md")));
        assert!(!store.owns(&dir.0.join("index.json")));
        assert!(!store.owns(&dir.0.join("notes.txt")));
        assert!(!store.owns(&dir.0.join("sub").join("deep.md")));
        assert!(!store.owns(&std::env::temp_dir().join("elsewhere.md")));

        // The same folder reached through `..` is still the folder.
        let child = dir.0.join("child");
        std::fs::create_dir_all(&child).expect("child");
        let file = dir.write("real.md", "r");
        assert!(store.owns(&child.join("..").join("real.md")));
        assert!(store.owns(&file));
    }

    #[test]
    fn unicode_title_round_trips() {
        let dir = TempDir::new();
        let store = dir.store();
        let title = "Revisione “Codex” — 1/10 🚀";
        let entry = store.create().expect("create");
        store.retitle(&entry.path, title).expect("retitle");
        assert_eq!(store.title(&entry.path), title);
        assert_eq!(titles(&dir.store().list()), vec![title]);
        let name = file_name(&entry.path);
        assert!(name.starts_with("prompt-") && name.ends_with(".md"), "{name}");
        assert!(name.is_ascii(), "{name}");
    }

    #[test]
    fn preview_skips_blank_lead_lines_and_keeps_two() {
        assert_eq!(preview("\n\n  \nA\nB\nC"), "A\nB");
    }

    #[test]
    fn preview_drops_bom_and_carriage_returns() {
        assert_eq!(preview("\u{feff}A\r\nB\r\nC"), "A\nB");
        assert_eq!(preview("A\rB\rC"), "A\nB");
    }

    #[test]
    fn read_preview_reads_only_the_head_of_a_large_file() {
        let dir = TempDir::new();
        let mut text = String::from("A\nB\n");
        text.push_str(&"x".repeat(5 * 1024 * 1024));
        let path = dir.write("big.md", &text);
        assert_eq!(dir.store().read_preview(&path), "A\nB");
    }
}
