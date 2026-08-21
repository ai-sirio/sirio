//! `git log` reading: commit records for the History view.

use std::path::Path;

use crate::GitError;
use crate::git;

const LOG_FORMAT: &str = "--format=%x1e%H%x1f%P%x1f%D%x1f%an%x1f%at%x1f%s";

/// Field separator inside one record: ASCII US.
const FIELD: char = '\u{1f}';
/// Record separator between commits: ASCII RS. Deliberately not `NUL`, which
/// collides with `-z` and which git reserves as its own record delimiter. Git
/// only forbids NUL in commit messages; US and RS remain format delimiters.
const RECORD: char = '\u{1e}';

/// The commit fields the History view needs, in `git log` order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommitRecord {
    /// Full 40-character object name.
    pub sha: String,
    /// Full object names of the parents, in git's order. Empty for a root
    /// commit; two or more for a merge.
    pub parents: Vec<String>,
    /// Decorations from `%D`, already split: `HEAD -> main`, `tag: v1`, ...
    pub refs: Vec<String>,
    /// Author name (`%an`), not the committer.
    pub author: String,
    /// Author date as a Unix timestamp (`%at`).
    pub timestamp: i64,
    /// One-line subject (`%s`).
    pub subject: String,
}

/// Namespace for commit-history operations.
pub struct GitLog;

impl GitLog {
    /// Reads local-branch commits in date order, including merge parents.
    pub fn commits(repo: &Path, skip: usize, limit: usize) -> Result<Vec<CommitRecord>, GitError> {
        let skip_arg = format!("--skip={skip}");
        let limit_arg = format!("-n{limit}");
        match git::run_accepting(
            &[
                "log",
                "--branches",
                "--date-order",
                &skip_arg,
                &limit_arg,
                LOG_FORMAT,
            ],
            repo,
            &[0],
        ) {
            Ok(output) => Ok(parse_log(&output.stdout_string())),
            Err(error) if is_unborn_head(&error) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    /// Whether `HEAD` resolves to a commit.
    pub fn has_commits(repo: &Path) -> bool {
        git::run_accepting(&["rev-parse", "--verify", "HEAD"], repo, &[0]).is_ok()
    }
}

fn is_unborn_head(error: &GitError) -> bool {
    matches!(
        error,
        GitError::CommandFailed { stderr, .. }
            if stderr.contains("does not have any commits yet")
                || stderr.contains("unknown revision or path not in the working tree")
    )
}

/// Parses the output of the `--format` this module sends. A record whose field
/// count is not exactly six is dropped rather than partially filled: a
/// truncated capture (see `GitCommandResult::truncated`) must not produce a
/// commit with an empty sha that the graph would then try to link.
pub fn parse_log(output: &str) -> Vec<CommitRecord> {
    output
        .split(RECORD)
        .filter_map(|record| {
            let record = record.trim_matches('\n');
            if record.is_empty() {
                return None;
            }
            let fields: Vec<_> = record.split(FIELD).collect();
            if fields.len() != 6 {
                return None;
            }
            let sha = fields[0];
            if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            let parents = fields[1];
            let refs = fields[2];
            let author = fields[3];
            let timestamp = fields[4].parse().ok()?;
            let subject = fields[5];
            Some(CommitRecord {
                sha: sha.to_owned(),
                parents: parents
                    .split_whitespace()
                    .map(ToOwned::to_owned)
                    .collect(),
                refs: refs
                    .split(',')
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty())
                    .map(ToOwned::to_owned)
                    .collect(),
                author: author.to_owned(),
                timestamp,
                subject: subject.to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One record per commit, fields split on US (0x1f), records on RS (0x1e).
    fn fixture(records: &[&str]) -> String {
        records
            .iter()
            .map(|record| format!("\u{1e}{record}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn parses_a_single_commit() {
        let output = fixture(&["0123456789abcdef0123456789abcdef01234567\u{1f}\u{1f}HEAD -> main\u{1f}Ada\u{1f}1700000000\u{1f}initial commit"]);

        let commits = parse_log(&output);

        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].sha, "0123456789abcdef0123456789abcdef01234567");
        assert!(commits[0].parents.is_empty());
        assert_eq!(commits[0].refs, vec!["HEAD -> main".to_owned()]);
        assert_eq!(commits[0].author, "Ada");
        assert_eq!(commits[0].timestamp, 1_700_000_000);
        assert_eq!(commits[0].subject, "initial commit");
    }

    #[test]
    fn parses_multiple_parents_of_a_merge() {
        let output = fixture(&["1111111111111111111111111111111111111111\u{1f}p1 p2\u{1f}\u{1f}Ada\u{1f}1700000001\u{1f}merge branch 'x'"]);

        let commits = parse_log(&output);

        assert_eq!(commits[0].parents, vec!["p1".to_owned(), "p2".to_owned()]);
    }

    #[test]
    fn an_empty_ref_field_yields_no_refs() {
        let output = fixture(&["2222222222222222222222222222222222222222\u{1f}p1\u{1f}\u{1f}Ada\u{1f}1700000002\u{1f}fix: thing"]);

        let commits = parse_log(&output);

        assert!(commits[0].refs.is_empty());
    }

    #[test]
    fn splits_multiple_refs_on_comma() {
        let output = fixture(&["3333333333333333333333333333333333333333\u{1f}p1\u{1f}HEAD -> main, origin/main, tag: v1\u{1f}Ada\u{1f}1\u{1f}s"]);

        let commits = parse_log(&output);

        assert_eq!(
            commits[0].refs,
            vec![
                "HEAD -> main".to_owned(),
                "origin/main".to_owned(),
                "tag: v1".to_owned(),
            ]
        );
    }

    #[test]
    fn keeps_spaces_and_unicode_in_a_subject() {
        let output = fixture(&["4444444444444444444444444444444444444444\u{1f}p1\u{1f}\u{1f}Ada Lovelace\u{1f}3\u{1f}feat: aggiunge il pannello — con trattino"]);

        let commits = parse_log(&output);

        assert_eq!(commits[0].author, "Ada Lovelace");
        assert_eq!(commits[0].subject, "feat: aggiunge il pannello — con trattino");
    }

    #[test]
    fn empty_output_yields_no_commits() {
        assert!(parse_log("").is_empty());
    }

    #[test]
    fn a_record_with_missing_fields_is_skipped_not_panicked_on() {
        let output = fixture(&["truncated\u{1f}p1"]);

        assert!(parse_log(&output).is_empty());
    }

    #[test]
    fn rejects_a_sha_that_is_not_40_hex_characters() {
        let output = fixture(&["abc123\u{1f}\u{1f}\u{1f}Ada\u{1f}1\u{1f}subject"]);

        assert!(parse_log(&output).is_empty());
    }

    #[test]
    fn rejects_a_record_with_extra_fields() {
        let output = fixture(&[
            "5555555555555555555555555555555555555555\u{1f}\u{1f}\u{1f}Ada\u{1f}1\u{1f}subject\u{1f}extra",
        ]);

        assert!(parse_log(&output).is_empty());
    }

    #[test]
    fn rejects_a_record_with_an_invalid_timestamp() {
        let output = fixture(&[
            "6666666666666666666666666666666666666666\u{1f}\u{1f}\u{1f}Ada\u{1f}not-a-timestamp\u{1f}subject",
        ]);

        assert!(parse_log(&output).is_empty());
    }
}
