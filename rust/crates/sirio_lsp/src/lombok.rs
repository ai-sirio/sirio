//! Which projects use Lombok, and which Lombok jar is the newest on this machine.
//!
//! jdtls only understands Lombok-generated members when Lombok's Java agent is
//! on its JVM. The agent is the jar Maven or Gradle already downloaded, so
//! nothing here fetches anything: it reads two caches, newest version wins.
//!
//! The split follows `config.rs` and `loader.rs`: version ordering, reading
//! `localRepository` out of `settings.xml` and the text test are pure; the
//! functions that list directories and read build files are the only ones that
//! touch the filesystem. The caller passes the child environment in, so this
//! crate does not depend on `sirio_env`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A Lombok version, ordered by value.
///
/// `parts` has its trailing zeros removed, so `1.18` and `1.18.0` are the same
/// version and the derived lexicographic order equals a zero-padded numeric
/// comparison. `release` comes second and is `true` without a suffix, which
/// makes a suffixed version sort below the same version without one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LombokVersion {
    parts: Vec<u64>,
    release: bool,
}

impl LombokVersion {
    /// Reads a version folder's name: dot-separated numbers, optionally followed
    /// by `-suffix`. Anything else answers `None`, because a folder that is not
    /// a version must be skipped, never fail a launch.
    pub fn parse(folder: &str) -> Option<Self> {
        let (numbers, suffix) = match folder.split_once('-') {
            Some((numbers, suffix)) => (numbers, Some(suffix)),
            None => (folder, None),
        };
        if suffix.is_some_and(str::is_empty) {
            return None;
        }
        let mut parts = Vec::new();
        for part in numbers.split('.') {
            if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            parts.push(part.parse::<u64>().ok()?);
        }
        while parts.last() == Some(&0) {
            parts.pop();
        }
        Some(Self {
            parts,
            release: suffix.is_none(),
        })
    }
}

/// A complete Lombok jar found in one of the caches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LombokJar {
    pub path: PathBuf,
    pub version: LombokVersion,
}

/// Whether a build file's text names Lombok: the `org.projectlombok`
/// dependency, the `io.freefair.lombok` plugin and a catalog alias such as
/// `libs.lombok` all contain it. Case-sensitive on purpose.
pub fn mentions_lombok(text: &str) -> bool {
    text.contains("lombok")
}

/// The `<localRepository>` of a Maven `settings.xml`, when it is set.
///
/// Comments are stripped first, so a commented-out element is not the live
/// one. The text is trimmed, `${user.home}` becomes `home`, and a result that is
/// empty or not absolute answers `None`, which sends the caller to the default
/// repository.
pub fn local_repository(settings_xml: &str, home: &Path) -> Option<PathBuf> {
    let mut text = String::with_capacity(settings_xml.len());
    let mut rest = settings_xml;
    while let Some(start) = rest.find("<!--") {
        text.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + "-->".len()..],
            // An unterminated comment swallows the rest of the file.
            None => rest = "",
        }
    }
    text.push_str(rest);

    const OPEN: &str = "<localRepository>";
    const CLOSE: &str = "</localRepository>";
    let start = text.find(OPEN)? + OPEN.len();
    let end = start + text[start..].find(CLOSE)?;
    let expanded = text[start..end]
        .trim()
        .replace("${user.home}", &home.to_string_lossy());
    let path = PathBuf::from(expanded);
    (!path.as_os_str().is_empty() && path.is_absolute()).then_some(path)
}

/// The user's home as the child sees it: `USERPROFILE` on Windows, `HOME`
/// elsewhere. `windows` is a parameter so the rule is testable on every platform.
fn home_dir(environment: &BTreeMap<String, String>, windows: bool) -> Option<PathBuf> {
    let key = if windows { "USERPROFILE" } else { "HOME" };
    environment
        .get(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// The newest complete Lombok jar across Maven's local repository and Gradle's
/// cache, or `None` when neither holds one.
///
/// `environment` is the one `sirio_env::for_child()` returns. Only an exact
/// `lombok-<v>.jar` with a length above zero counts, so a folder holding only
/// `.lastUpdated`, `.part` or `-sources` files, or an empty jar, is as good as
/// absent and an older complete jar wins.
pub fn newest_jar(environment: &BTreeMap<String, String>) -> Option<LombokJar> {
    let home = home_dir(environment, cfg!(windows))?;

    // Read as bytes: a settings.xml in ISO-8859-1 (an accent in a comment) is not
    // UTF-8, and the setting is ASCII wherever the file is not.
    let maven = std::fs::read(home.join(".m2").join("settings.xml"))
        .ok()
        .and_then(|bytes| local_repository(&String::from_utf8_lossy(&bytes), &home))
        .unwrap_or_else(|| home.join(".m2").join("repository"));
    let gradle = environment
        .get("GRADLE_USER_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".gradle"));

    let mut candidates = Vec::new();
    let maven_lombok = maven.join("org").join("projectlombok").join("lombok");
    for (directory, name) in version_folders(&maven_lombok) {
        candidates.extend(complete_jar(&directory, &name));
    }
    let gradle_lombok = gradle
        .join("caches")
        .join("modules-2")
        .join("files-2.1")
        .join("org.projectlombok")
        .join("lombok");
    for (directory, name) in version_folders(&gradle_lombok) {
        // Gradle puts the jar under a hash folder it chooses.
        let Ok(hashes) = std::fs::read_dir(&directory) else {
            continue;
        };
        for hash in hashes.flatten() {
            candidates.extend(complete_jar(&hash.path(), &name));
        }
    }

    candidates
        .into_iter()
        .reduce(|best, next| if next.version > best.version { next } else { best })
}

/// The subfolders of `parent` whose names parse as a version, with the name.
fn version_folders(parent: &Path) -> Vec<(PathBuf, String)> {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            LombokVersion::parse(&name)?;
            Some((entry.path(), name))
        })
        .collect()
}

/// `<directory>/lombok-<name>.jar`, when it is a file with some content.
fn complete_jar(directory: &Path, name: &str) -> Option<LombokJar> {
    let path = directory.join(format!("lombok-{name}.jar"));
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() == 0 {
        return None;
    }
    Some(LombokJar {
        path,
        version: LombokVersion::parse(name)?,
    })
}

/// The build files whose text decides whether a project uses Lombok.
const BUILD_FILES: [&str; 6] = [
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "gradle/libs.versions.toml",
];

/// Whether a build file in `root`, or in any directory above it up to and
/// including `worktree_root`, mentions Lombok. A parent POM that declares Lombok
/// for every module counts; a file above the worktree never does. A `root`
/// outside the worktree is checked alone. An unreadable file counts as silent.
pub fn project_uses_lombok(root: &Path, worktree_root: &Path) -> bool {
    let mut directory = Some(root);
    while let Some(current) = directory {
        if BUILD_FILES.iter().any(|name| {
            std::fs::read(current.join(name))
                .is_ok_and(|bytes| mentions_lombok(&String::from_utf8_lossy(&bytes)))
        }) {
            return true;
        }
        if current == worktree_root || !root.starts_with(worktree_root) {
            break;
        }
        directory = current.parent();
    }
    false
}

/// The one argv element that puts Lombok's agent on jdtls's JVM. The launcher
/// takes the path as is, so a space in it needs no quoting.
pub fn agent_arg(jar: &Path) -> String {
    format!("--jvm-arg=-javaagent:{}", jar.to_string_lossy())
}

#[cfg(test)]
mod tests {
    // Every way this unit can fail, written before the code:
    //
    // - a version compares by its text, so 1.18.9 beats 1.18.30;
    // - a suffixed version (-SNAPSHOT) beats, or ties with, the release it precedes;
    // - a folder that is not a version (`edge-SNAPSHOT`, `latest`, `1..2`) parses, or fails the scan;
    // - settings.xml without a localRepository invents one;
    // - a commented-out localRepository is taken as the live one;
    // - `${user.home}` is not expanded, or the text keeps its newlines and padding;
    // - a relative or blank localRepository replaces the default repository;
    // - "mentions lombok" is case-insensitive, or misses the plugin and catalog spellings;
    // - the newest jar is taken from one cache only, not across Maven and Gradle;
    // - a newer folder holding only `.lastUpdated`, a `.part`, a `-sources` jar or a
    //   zero-byte jar beats an older complete jar;
    // - a non-version folder beside real ones aborts the scan;
    // - a set localRepository is searched together with `~/.m2/repository`;
    // - a settings.xml that is not UTF-8 (an ISO-8859-1 accent in a comment) loses its
    //   localRepository;
    // - no cache at all panics instead of answering None;
    // - a parent build file up to the worktree root is missed, or one above it counts;
    // - a version catalog is not read;
    // - a jar path with a space is split or quoted in the agent argument;
    // - Windows reads HOME, or others read USERPROFILE.

    use super::*;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    fn v(text: &str) -> LombokVersion {
        LombokVersion::parse(text).unwrap()
    }

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sirio-lsp-lombok-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// `<repo>/org/projectlombok/lombok/<version>/lombok-<version>.jar`, non-empty.
    fn maven_jar(repo: &Path, version: &str) -> PathBuf {
        let jar = repo
            .join("org/projectlombok/lombok")
            .join(version)
            .join(format!("lombok-{version}.jar"));
        write(&jar, b"jar");
        jar
    }

    fn gradle_jar(base: &Path, version: &str) -> PathBuf {
        let jar = base
            .join("caches/modules-2/files-2.1/org.projectlombok/lombok")
            .join(version)
            .join("0123abcd")
            .join(format!("lombok-{version}.jar"));
        write(&jar, b"jar");
        jar
    }

    fn env_with(home: &Path) -> BTreeMap<String, String> {
        let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        BTreeMap::from([(key.to_owned(), home.to_string_lossy().into_owned())])
    }

    #[test]
    fn versions_compare_by_number_not_text() {
        assert!(v("1.18.9") < v("1.18.30"));
        assert!(v("1.18.30-SNAPSHOT") < v("1.18.30"));
        assert!(v("1.18.30") < v("1.18.31-SNAPSHOT"));
    }

    #[test]
    fn a_folder_that_is_not_a_version_does_not_parse() {
        for name in ["edge-SNAPSHOT", "", "1..2", ".1", "1.", "latest"] {
            assert_eq!(LombokVersion::parse(name), None, "{name}");
        }
    }

    #[test]
    fn settings_without_local_repository_answer_none() {
        assert_eq!(
            local_repository("<settings></settings>", Path::new("/h")),
            None
        );
    }

    #[test]
    fn a_commented_out_local_repository_is_ignored() {
        let xml = "<settings><!-- <localRepository>/x</localRepository> --></settings>";
        assert_eq!(local_repository(xml, Path::new("/h")), None);
    }

    #[test]
    fn user_home_is_expanded_and_whitespace_trimmed() {
        let xml = "<localRepository>\n  ${user.home}/m2 custom\n</localRepository>";
        assert_eq!(
            local_repository(xml, Path::new("/h")),
            Some(PathBuf::from("/h/m2 custom"))
        );
    }

    #[test]
    fn a_relative_or_empty_local_repository_is_ignored() {
        for xml in [
            "<localRepository>repo</localRepository>",
            "<localRepository> </localRepository>",
        ] {
            assert_eq!(local_repository(xml, Path::new("/h")), None, "{xml}");
        }
    }

    #[test]
    fn mentions_lombok_is_a_case_sensitive_substring() {
        assert!(mentions_lombok("<groupId>org.projectlombok</groupId>"));
        assert!(mentions_lombok("id 'io.freefair.lombok' version '8'"));
        assert!(mentions_lombok("compileOnly(libs.lombok)"));
        assert!(!mentions_lombok("Lombok"));
    }

    #[test]
    fn the_newest_jar_across_maven_and_gradle_wins() {
        let home = scratch("across");
        maven_jar(&home.join(".m2/repository"), "1.18.30");
        let gradle = gradle_jar(&home.join(".gradle"), "1.18.36");
        let found = newest_jar(&env_with(&home)).unwrap();
        assert_eq!(found.path, gradle);
        assert_eq!(found.version, v("1.18.36"));

        // And GRADLE_USER_HOME replaces ~/.gradle.
        let other = scratch("across-gradle-home");
        let moved = gradle_jar(&other, "1.18.38");
        let mut env = env_with(&home);
        env.insert("GRADLE_USER_HOME".into(), other.to_string_lossy().into_owned());
        assert_eq!(newest_jar(&env).unwrap().path, moved);
    }

    #[test]
    fn a_newer_folder_without_a_complete_jar_loses_to_an_older_complete_one() {
        let home = scratch("incomplete");
        let repo = home.join(".m2/repository");
        let good = maven_jar(&repo, "1.18.30");
        let lombok = repo.join("org/projectlombok/lombok");
        write(&lombok.join("1.18.42/lombok-1.18.42.jar.lastUpdated"), b"x");
        write(&lombok.join("1.18.42/lombok-1.18.42.jar.part"), b"x");
        write(&lombok.join("1.18.42/lombok-1.18.42-sources.jar"), b"x");
        write(&lombok.join("1.18.44/lombok-1.18.44.jar"), b"");
        let found = newest_jar(&env_with(&home)).unwrap();
        assert_eq!(found.path, good);
        assert_eq!(found.version, v("1.18.30"));
    }

    #[test]
    fn a_non_version_folder_beside_real_ones_is_skipped() {
        let home = scratch("non-version");
        let repo = home.join(".m2/repository");
        let good = maven_jar(&repo, "1.18.30");
        maven_jar(&repo, "edge-SNAPSHOT");
        assert_eq!(newest_jar(&env_with(&home)).unwrap().path, good);
    }

    #[test]
    fn a_set_local_repository_replaces_the_default_one() {
        let home = scratch("custom-repo");
        write(
            &home.join(".m2/settings.xml"),
            format!(
                "<settings><localRepository>{}</localRepository></settings>",
                home.join("custom").display()
            )
            .as_bytes(),
        );
        let custom = maven_jar(&home.join("custom"), "1.18.20");
        maven_jar(&home.join(".m2/repository"), "1.18.40");
        let found = newest_jar(&env_with(&home)).unwrap();
        assert_eq!(found.path, custom);
        assert_eq!(found.version, v("1.18.20"));
    }

    #[test]
    fn a_settings_file_that_is_not_utf8_still_gives_its_local_repository() {
        // An ISO-8859-1 `è` (0xE8) in a comment before the setting, as a
        // corporate settings.xml written on a Latin-1 machine has it.
        let home = scratch("latin1");
        let mut xml = b"<settings><!-- repository di prova: \xe8 locale -->".to_vec();
        xml.extend_from_slice(
            format!("<localRepository>{}</localRepository></settings>", home.join("custom").display())
                .as_bytes(),
        );
        write(&home.join(".m2/settings.xml"), &xml);
        let custom = maven_jar(&home.join("custom"), "1.18.20");
        maven_jar(&home.join(".m2/repository"), "1.18.40");
        assert_eq!(newest_jar(&env_with(&home)).unwrap().path, custom);
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn no_cache_at_all_answers_none() {
        let home = scratch("empty");
        assert_eq!(newest_jar(&env_with(&home)), None);
        assert_eq!(newest_jar(&BTreeMap::new()), None);
    }

    #[test]
    fn a_parent_build_file_up_to_the_worktree_counts() {
        let worktree = scratch("parent");
        write(&worktree.join("pom.xml"), b"<artifactId>lombok</artifactId>");
        write(&worktree.join("app/pom.xml"), b"<project/>");
        assert!(project_uses_lombok(&worktree.join("app"), &worktree));
    }

    #[test]
    fn a_build_file_above_the_worktree_does_not_count() {
        let outer = scratch("above");
        write(&outer.join("pom.xml"), b"<artifactId>lombok</artifactId>");
        let worktree = outer.join("wt");
        write(&worktree.join("app/pom.xml"), b"<project/>");
        assert!(!project_uses_lombok(&worktree.join("app"), &worktree));
    }

    #[test]
    fn a_version_catalog_counts() {
        let root = scratch("catalog");
        write(
            &root.join("gradle/libs.versions.toml"),
            b"[libraries]\nlombok = { module = \"org.projectlombok:lombok\" }\n",
        );
        assert!(project_uses_lombok(&root, &root));
    }

    #[test]
    fn the_agent_argument_is_one_element_with_the_path_intact() {
        assert_eq!(
            agent_arg(Path::new("/h/m2 custom/lombok-1.18.30.jar")),
            "--jvm-arg=-javaagent:/h/m2 custom/lombok-1.18.30.jar"
        );
    }

    #[test]
    fn windows_reads_userprofile_and_others_read_home() {
        let env = BTreeMap::from([
            ("HOME".to_owned(), "/h".to_owned()),
            ("USERPROFILE".to_owned(), r"C:\Users\A B".to_owned()),
        ]);
        assert_eq!(home_dir(&env, true), Some(PathBuf::from(r"C:\Users\A B")));
        assert_eq!(home_dir(&env, false), Some(PathBuf::from("/h")));
    }
}
