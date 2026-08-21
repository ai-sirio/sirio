//! `git log` reading: commit records for the History view.

/// Field separator inside one record: ASCII US.
const FIELD: char = '\u{1f}';
/// Record separator between commits: ASCII RS. Deliberately not `NUL`, which
/// collides with `-z` and which git would also use as its own record
/// delimiter; a subject can contain neither of these two control characters.
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

/// Parses the output of the `--format` this module sends. A record whose
/// field count is short is dropped rather than partially filled: a truncated
/// capture (see `GitCommandResult::truncated`) must not produce a commit with
/// an empty sha that the graph would then try to link.
pub fn parse_log(output: &str) -> Vec<CommitRecord> {
    output
        .split(RECORD)
        .filter_map(|record| {
            let record = record.trim_start_matches('\n');
            if record.is_empty() {
                return None;
            }
            let mut fields = record.split(FIELD);
            let sha = fields.next()?.to_owned();
            let parents = fields.next()?;
            let refs = fields.next()?;
            let author = fields.next()?.to_owned();
            let timestamp = fields.next()?;
            let subject = fields.next()?.to_owned();
            if sha.is_empty() {
                return None;
            }
            Some(CommitRecord {
                sha,
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
                author,
                timestamp: timestamp.trim().parse().unwrap_or_default(),
                subject,
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
        let output = fixture(&["abc123\u{1f}\u{1f}HEAD -> main\u{1f}Ada\u{1f}1700000000\u{1f}initial commit"]);

        let commits = parse_log(&output);

        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].sha, "abc123");
        assert!(commits[0].parents.is_empty());
        assert_eq!(commits[0].refs, vec!["HEAD -> main".to_owned()]);
        assert_eq!(commits[0].author, "Ada");
        assert_eq!(commits[0].timestamp, 1_700_000_000);
        assert_eq!(commits[0].subject, "initial commit");
    }

    #[test]
    fn parses_multiple_parents_of_a_merge() {
        let output = fixture(&["m1\u{1f}p1 p2\u{1f}\u{1f}Ada\u{1f}1700000001\u{1f}merge branch 'x'"]);

        let commits = parse_log(&output);

        assert_eq!(commits[0].parents, vec!["p1".to_owned(), "p2".to_owned()]);
    }

    #[test]
    fn an_empty_ref_field_yields_no_refs() {
        let output = fixture(&["c1\u{1f}p1\u{1f}\u{1f}Ada\u{1f}1700000002\u{1f}fix: thing"]);

        let commits = parse_log(&output);

        assert!(commits[0].refs.is_empty());
    }

    #[test]
    fn splits_multiple_refs_on_comma() {
        let output = fixture(&["c1\u{1f}p1\u{1f}HEAD -> main, origin/main, tag: v1\u{1f}Ada\u{1f}1\u{1f}s"]);

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
        let output = fixture(&["c1\u{1f}p1\u{1f}\u{1f}Ada Lovelace\u{1f}3\u{1f}feat: aggiunge il pannello — con trattino"]);

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
}
