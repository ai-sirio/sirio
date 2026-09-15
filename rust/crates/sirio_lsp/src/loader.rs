//! Where the table lives, when to read it again, and what to do when it will
//! not parse.
//!
//! Reload is lazy: the file is re-read when its mtime has moved and someone
//! asks. No watcher, no inotify, no extra thread — the only moment the table
//! matters is when a server is about to start.
//!
//! The environment arrives as a map rather than being read from `std::env`,
//! which is what makes the path lookup testable without touching the real
//! environment. `sirio_agents::opencode` resolves OpenCode's config the same
//! way, for the same reason.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::config::LanguageTable;

/// `$SIRIO_CONFIG_DIR`, else `$XDG_CONFIG_HOME/sirio`, else
/// `~/.config/sirio` — then `languages.toml` inside it.
pub fn config_path(environment: &BTreeMap<String, String>, home: &Path) -> PathBuf {
    let directory = environment
        .get("SIRIO_CONFIG_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            environment
                .get("XDG_CONFIG_HOME")
                .filter(|value| !value.is_empty())
                .map(|xdg| Path::new(xdg).join("sirio"))
        })
        .unwrap_or_else(|| home.join(".config").join("sirio"));
    directory.join("languages.toml")
}

#[derive(Debug)]
pub enum Reload {
    /// The file has not moved since the last read.
    Unchanged,
    /// A fresh table is in place — from the file, or from the defaults when
    /// there is no file.
    Loaded,
    /// The file exists and will not parse. The previous table is still in
    /// place; see the type docs for why that is deliberate.
    Failed { message: String },
}

pub struct ConfigLoader {
    path: PathBuf,
    /// `None` until the first successful read, and after a read that found
    /// no file — so creating the file later is seen as a change.
    seen: Option<SystemTime>,
    table: LanguageTable,
}

impl ConfigLoader {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            seen: None,
            table: LanguageTable::defaults(),
        }
    }

    pub fn table(&self) -> &LanguageTable {
        &self.table
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Re-reads when the mtime has moved. Cheap enough to call before every
    /// server start: one `stat` in the common case.
    pub fn refresh(&mut self) -> Reload {
        let mtime = std::fs::metadata(&self.path)
            .and_then(|metadata| metadata.modified())
            .ok();

        if mtime.is_none() {
            // No file: the defaults are the table.
            self.seen = None;
            self.table = LanguageTable::defaults();
            return Reload::Loaded;
        }
        if mtime == self.seen {
            return Reload::Unchanged;
        }

        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) => {
                return Reload::Failed {
                    message: format!("could not read {}: {error}", self.path.display()),
                };
            }
        };

        match LanguageTable::parse(&text) {
            Ok(table) => {
                self.seen = mtime;
                self.table = table.with_defaults_appended();
                Reload::Loaded
            }
            Err(error) => {
                // Record the mtime anyway: without it every refresh would
                // re-read and re-report the same broken file, turning one
                // mistake into a stream of identical notices.
                self.seen = mtime;
                Reload::Failed {
                    message: format!("{}: {error}", self.path.display()),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn sirio_config_dir_wins_when_set() {
        let path = config_path(
            &env(&[("SIRIO_CONFIG_DIR", "/custom"), ("XDG_CONFIG_HOME", "/xdg")]),
            Path::new("/home/u"),
        );
        assert_eq!(path, Path::new("/custom/languages.toml"));
    }

    #[test]
    fn xdg_config_home_is_used_when_sirio_config_dir_is_absent() {
        let path = config_path(&env(&[("XDG_CONFIG_HOME", "/xdg")]), Path::new("/home/u"));
        assert_eq!(path, Path::new("/xdg/sirio/languages.toml"));
    }

    #[test]
    fn home_dot_config_is_the_last_resort() {
        let path = config_path(&env(&[]), Path::new("/home/u"));
        assert_eq!(path, Path::new("/home/u/.config/sirio/languages.toml"));
    }

    #[test]
    fn an_empty_environment_value_is_treated_as_unset() {
        // An exported-but-empty XDG_CONFIG_HOME is common and must not
        // produce `/sirio/languages.toml` at the filesystem root.
        let path = config_path(&env(&[("XDG_CONFIG_HOME", "")]), Path::new("/home/u"));
        assert_eq!(path, Path::new("/home/u/.config/sirio/languages.toml"));
    }

    fn scratch(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sirio-lsp-loader-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("languages.toml")
    }

    #[test]
    fn an_absent_file_yields_exactly_the_defaults() {
        let mut loader = ConfigLoader::new(scratch("absent"));
        assert!(matches!(loader.refresh(), Reload::Loaded));
        assert_eq!(
            loader.table().entries().len(),
            LanguageTable::defaults().entries().len()
        );
    }

    #[test]
    fn a_valid_file_is_merged_ahead_of_the_defaults() {
        let path = scratch("valid");
        std::fs::write(
            &path,
            "[[language]]\nname = \"rust\"\nextensions = [\"rs\"]\ncommand = \"mine\"\n",
        )
        .unwrap();
        let mut loader = ConfigLoader::new(path.clone());
        assert!(matches!(loader.refresh(), Reload::Loaded));
        assert_eq!(loader.table().for_extension("rs").unwrap().command, "mine");
        assert_eq!(loader.table().for_extension("go").unwrap().command, "gopls");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn a_broken_file_reports_the_problem_and_leaves_the_defaults_standing() {
        // The least obvious decision in this design: a syntax error removes
        // nothing. The notice says what is wrong; the defaults keep working.
        let path = scratch("broken");
        std::fs::write(&path, "[[language]\nname = \"rust\"").unwrap();
        let mut loader = ConfigLoader::new(path.clone());
        match loader.refresh() {
            Reload::Failed { message } => {
                assert!(message.contains("line"), "the message must point at a line: {message}");
            }
            other => panic!("a broken file must report a failure, got {other:?}"),
        }
        assert_eq!(loader.table().for_extension("rs").unwrap().command, "rust-analyzer");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn a_second_refresh_without_a_change_reports_unchanged() {
        let path = scratch("unchanged");
        std::fs::write(&path, "").unwrap();
        let mut loader = ConfigLoader::new(path.clone());
        assert!(matches!(loader.refresh(), Reload::Loaded));
        assert!(matches!(loader.refresh(), Reload::Unchanged));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn an_edited_file_is_picked_up_on_the_next_refresh() {
        let path = scratch("edited");
        std::fs::write(&path, "").unwrap();
        let mut loader = ConfigLoader::new(path.clone());
        loader.refresh();
        // Sleep past the filesystem's mtime granularity; a same-second
        // rewrite can otherwise carry the same timestamp.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(
            &path,
            "[[language]]\nname = \"rust\"\nextensions = [\"rs\"]\ncommand = \"edited\"\n",
        )
        .unwrap();
        assert!(matches!(loader.refresh(), Reload::Loaded));
        assert_eq!(loader.table().for_extension("rs").unwrap().command, "edited");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
