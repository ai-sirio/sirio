//! Sirio's skill for the agents the user runs by hand, anywhere: the copy
//! each CLI reads from its own user-level directory. `prepare` writes the
//! worktree's copy for a Sirio pane; this module writes the user's. The
//! bytes are the skill compiled into this binary, never one fetched over the
//! network, and each location is the `skills` CLI's per-agent table (1.7.2).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{SKILL_MANAGED_PREFIX, write_atomic};

/// The skill this binary ships: what every user-level location receives.
const BUNDLED_SKILL: &str = include_str!("../../../../skills/sirio/SKILL.md");

/// Where each CLI reads its user-level Sirio skill, with the label the
/// Settings card names it by. `CLAUDE_CONFIG_DIR` relocates Claude Code's
/// directory and `XDG_CONFIG_HOME` OpenCode's, as each CLI reads them; an
/// empty value is no value, and an XDG value must be absolute to count.
/// Codex, Pi and Oh-My-Pi read one shared directory, so it has one entry.
pub fn user_skill_paths(
    home: &Path,
    environment: &BTreeMap<String, String>,
) -> Vec<(&'static str, PathBuf)> {
    let claude_dir = environment
        .get("CLAUDE_CONFIG_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));
    let xdg_config = environment
        .get("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty() && Path::new(value).is_absolute())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    vec![
        ("Claude Code", skill_file(&claude_dir)),
        ("Codex, Pi, Oh-My-Pi", skill_file(&home.join(".agents"))),
        ("OpenCode", skill_file(&xdg_config.join("opencode"))),
    ]
}

/// `<agent config dir>/skills/sirio/SKILL.md`, where each CLI reads its user skills.
fn skill_file(config_dir: &Path) -> PathBuf {
    config_dir.join("skills").join("sirio").join("SKILL.md")
}

/// One location's outcome: the file written or refreshed, or why it was not.
#[derive(Debug, PartialEq, Eq)]
pub struct UserSkillReport {
    pub label: &'static str,
    pub outcome: Result<PathBuf, String>,
}

impl UserSkillReport {
    /// `<agent>: <file>` or `<agent>: failed — <error>`, the shape of
    /// `GlobalHookReport::summary_line`.
    pub fn summary_line(&self) -> String {
        match &self.outcome {
            Ok(path) => format!("{}: {}", self.label, path.display()),
            Err(error) => format!("{}: failed — {error}", self.label),
        }
    }
}

/// Settings → Install Skill: writes the bundled copy at every user-level
/// location, creating its folders. A file at a location that does not carry
/// Sirio's marker is someone else's, so it is reported and left as it is.
pub fn install_user_skills(
    home: &Path,
    environment: &BTreeMap<String, String>,
) -> Vec<UserSkillReport> {
    user_skill_paths(home, environment)
        .into_iter()
        .map(|(label, path)| UserSkillReport {
            label,
            outcome: install_one(&path),
        })
        .collect()
}

/// The startup refresh: brings each copy Sirio already wrote to the bundled
/// text. It never creates a copy, so a location the user never installed to
/// gets no report at all; an unmarked file is reported, not overwritten.
pub fn refresh_user_skills(
    home: &Path,
    environment: &BTreeMap<String, String>,
) -> Vec<UserSkillReport> {
    user_skill_paths(home, environment)
        .into_iter()
        .filter_map(|(label, path)| {
            refresh_one(&path).map(|outcome| UserSkillReport { label, outcome })
        })
        .collect()
}

/// Writes the bundled copy over whatever is at `path`, unless that is a file
/// Sirio did not write.
fn install_one(path: &Path) -> Result<PathBuf, String> {
    match existing_copy(path)? {
        Some(text) if !is_managed(&text) => Err(not_ours(path)),
        _ => write_bundled(path),
    }
}

/// Brings a copy Sirio wrote to the bundled text. `None` when there is no copy
/// at `path`: the refresh only ever updates what the user installed.
fn refresh_one(path: &Path) -> Option<Result<PathBuf, String>> {
    match existing_copy(path) {
        Err(error) => Some(Err(error)),
        Ok(None) => None,
        Ok(Some(text)) if !is_managed(&text) => Some(Err(not_ours(path))),
        // Already current: leave the file, and its modification time, alone.
        Ok(Some(text)) if text == BUNDLED_SKILL => Some(Ok(path.to_path_buf())),
        Ok(Some(_)) => Some(write_bundled(path)),
    }
}

/// The text at `path`, or `None` when nothing is there.
fn existing_copy(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("could not read {}: {error}", path.display())),
    }
}

/// A file carrying the shared managed-file prefix is one Sirio wrote, even
/// under an older build's marker wording (see `install_skill`).
fn is_managed(text: &str) -> bool {
    text.contains(SKILL_MANAGED_PREFIX)
}

fn not_ours(path: &Path) -> String {
    format!(
        "{} was not written by Sirio, so it is left as it is",
        path.display()
    )
}

/// Writes the bundled copy atomically, creating the folders it needs: the CLI's
/// own skill folder may not exist yet on a machine that never ran the CLI.
fn write_bundled(path: &Path) -> Result<PathBuf, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    write_atomic(path, BUNDLED_SKILL.as_bytes())
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A throwaway directory under TMPDIR, removed on drop. Canonicalized, so
    /// its paths are absolute on every platform.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("sirio-user-skill-{}-{unique}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(std::fs::canonicalize(&path).expect("canonicalize temp dir"))
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn environment(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    fn bundled() -> &'static str {
        include_str!("../../../../skills/sirio/SKILL.md")
    }

    fn report<'a>(reports: &'a [UserSkillReport], label: &str) -> &'a UserSkillReport {
        reports
            .iter()
            .find(|report| report.label == label)
            .unwrap_or_else(|| panic!("no report for {label} in {reports:?}"))
    }

    #[test]
    fn user_skills_land_where_each_cli_reads_them() {
        let root = TempDir::new();
        let home = root.path().join("home");
        let relocated = environment(&[
            ("CLAUDE_CONFIG_DIR", root.path().join("cfg/claude").to_str().unwrap()),
            ("XDG_CONFIG_HOME", root.path().join("cfg/xdg").to_str().unwrap()),
        ]);
        let defaults = vec![
            ("Claude Code", home.join(".claude/skills/sirio/SKILL.md")),
            ("Codex, Pi, Oh-My-Pi", home.join(".agents/skills/sirio/SKILL.md")),
            ("OpenCode", home.join(".config/opencode/skills/sirio/SKILL.md")),
        ];
        assert_eq!(user_skill_paths(&home, &environment(&[])), defaults);

        let expected = vec![
            ("Claude Code", root.path().join("cfg/claude/skills/sirio/SKILL.md")),
            ("Codex, Pi, Oh-My-Pi", home.join(".agents/skills/sirio/SKILL.md")),
            ("OpenCode", root.path().join("cfg/xdg/opencode/skills/sirio/SKILL.md")),
        ];
        assert_eq!(user_skill_paths(&home, &relocated), expected);

        let unusable = environment(&[
            ("CLAUDE_CONFIG_DIR", ""),
            ("XDG_CONFIG_HOME", "relative/xdg"),
        ]);
        assert_eq!(user_skill_paths(&home, &unusable), defaults);

        let reports = install_user_skills(&home, &relocated);
        for (label, path) in &expected {
            let written = report(&reports, label);
            assert_eq!(written.outcome, Ok(path.clone()), "{label} reports where it wrote");
            assert_eq!(
                std::fs::read_to_string(path).expect("the copy is written"),
                bundled(),
                "{label} gets the bundled skill"
            );
        }
    }

    #[test]
    fn user_skill_without_the_marker_is_left_alone() {
        let home = TempDir::new();
        let path = home.path().join(".agents/skills/sirio/SKILL.md");
        std::fs::create_dir_all(path.parent().unwrap()).expect("dir");
        std::fs::write(&path, "# My own sirio notes\n").expect("seed the user's file");
        let env = environment(&[]);

        let installed = install_user_skills(home.path(), &env);
        let refreshed = refresh_user_skills(home.path(), &env);

        for reports in [&installed, &refreshed] {
            assert!(
                report(reports, "Codex, Pi, Oh-My-Pi").outcome.is_err(),
                "a file without Sirio's marker is refused: {reports:?}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(&path).expect("still there"),
            "# My own sirio notes\n",
            "the user's file survives both the button and the startup refresh byte for byte"
        );
    }

    #[test]
    fn refresh_never_creates_a_user_skill() {
        let home = TempDir::new();
        let env = environment(&[]);

        let reports = refresh_user_skills(home.path(), &env);

        assert!(reports.is_empty(), "nothing to refresh, so nothing to report: {reports:?}");
        for (_, path) in user_skill_paths(home.path(), &env) {
            assert!(!path.exists(), "refresh created {}", path.display());
        }
    }

    #[test]
    fn refresh_updates_an_outdated_managed_copy() {
        let home = TempDir::new();
        let path = home.path().join(".claude/skills/sirio/SKILL.md");
        std::fs::create_dir_all(path.parent().unwrap()).expect("dir");
        // The wording an older build wrote: the shared prefix, not today's sentence.
        std::fs::write(
            &path,
            "<!-- Machine-managed by Sirio (build 1). Do not edit. -->\nold content\n",
        )
        .expect("seed an older install");

        let reports = refresh_user_skills(home.path(), &environment(&[]));

        assert_eq!(
            std::fs::read_to_string(&path).expect("refreshed"),
            bundled(),
            "the older copy is brought to the bundled text"
        );
        assert_eq!(reports.len(), 1, "only the copy that exists is reported: {reports:?}");
        assert_eq!(report(&reports, "Claude Code").outcome, Ok(path));
    }
}
