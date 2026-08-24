//! `git log` reading: commit records for the History view.

use std::path::{Path, PathBuf};

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

/// Everything the History view can ask `git log` to narrow by.
///
/// Data plus one pure translation into arguments, so every combination is
/// testable without running git and without a window. `Default` is exactly
/// the query the view ran before filtering existed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogFilter {
    /// Free text for `--grep`. Matches the whole message, body included:
    /// git has no subject-only flag.
    pub text: Option<String>,
    /// `-E` when set, `-F` when not. Off means *literal*, not "basic regex".
    pub regex: bool,
    pub case_sensitive: bool,
    /// Empty means every local branch, as before.
    pub branches: Vec<String>,
    /// Repeated `--author`; git ORs them.
    pub authors: Vec<String>,
    /// Passed to git verbatim, so git's own relative forms ("7 days ago")
    /// work without this crate computing timestamps.
    pub since: Option<String>,
    pub until: Option<String>,
    pub paths: Vec<PathBuf>,
    /// IntelliSort: `--topo-order` keeps a merged branch's commits contiguous
    /// instead of interleaving them by date.
    pub topo_order: bool,
}

impl LogFilter {
    /// Whether any *predicate* is set. Ordering is excluded on purpose: a
    /// re-ordered history is still the whole history, so it must not hide the
    /// graph or turn an empty result into "no matches".
    pub fn is_filtering(&self) -> bool {
        self.text.is_some()
            || !self.branches.is_empty()
            || !self.authors.is_empty()
            || self.since.is_some()
            || self.until.is_some()
            || !self.paths.is_empty()
    }

    /// Revision selection, ordering and predicates — everything that must
    /// precede `--skip`/`-n`/`--format`. Pathspecs are in
    /// [`LogFilter::pathspec_args`] instead, because they must come after
    /// those, behind `--`.
    pub fn args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if self.branches.is_empty() {
            args.push("--branches".to_owned());
        } else {
            for branch in &self.branches {
                args.push(format!("--branches={branch}"));
            }
        }

        args.push(
            if self.topo_order {
                "--topo-order"
            } else {
                "--date-order"
            }
            .to_owned(),
        );

        if let Some(text) = &self.text {
            args.push(if self.regex { "-E" } else { "-F" }.to_owned());
            args.push(format!("--grep={text}"));
        }
        for author in &self.authors {
            args.push(format!("--author={author}"));
        }
        // One `-i` covers --grep and --author both; emitting it with neither
        // present would be noise in every unfiltered query.
        if !self.case_sensitive && (self.text.is_some() || !self.authors.is_empty()) {
            args.push("-i".to_owned());
        }
        if let Some(since) = &self.since {
            args.push(format!("--since={since}"));
        }
        if let Some(until) = &self.until {
            args.push(format!("--until={until}"));
        }

        args
    }

    /// `["--", <path>…]`, or empty. The caller appends this **last**, after
    /// `--skip`, `-n` and `--format`: everything after `--` is a path to git.
    pub fn pathspec_args(&self) -> Vec<String> {
        if self.paths.is_empty() {
            return Vec::new();
        }
        let mut args = vec!["--".to_owned()];
        args.extend(
            self.paths
                .iter()
                .map(|path| path.to_string_lossy().into_owned()),
        );
        args
    }
}

/// Namespace for commit-history operations.
pub struct GitLog;

impl GitLog {
    /// Reads commits in the filter's order, including merge parents.
    pub fn commits(
        repo: &Path,
        skip: usize,
        limit: usize,
        filter: &LogFilter,
    ) -> Result<Vec<CommitRecord>, GitError> {
        let mut args = vec!["log".to_owned()];
        args.extend(filter.args());
        args.push(format!("--skip={skip}"));
        args.push(format!("-n{limit}"));
        args.push(LOG_FORMAT.to_owned());
        // Last, and only last: git reads everything after `--` as a path.
        args.extend(filter.pathspec_args());

        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        match git::run_accepting(&borrowed, repo, &[0]) {
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
                parents: parents.split_whitespace().map(ToOwned::to_owned).collect(),
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
    use std::path::PathBuf;

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
        let output = fixture(&[
            "0123456789abcdef0123456789abcdef01234567\u{1f}\u{1f}HEAD -> main\u{1f}Ada\u{1f}1700000000\u{1f}initial commit",
        ]);

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
        let output = fixture(&[
            "1111111111111111111111111111111111111111\u{1f}p1 p2\u{1f}\u{1f}Ada\u{1f}1700000001\u{1f}merge branch 'x'",
        ]);

        let commits = parse_log(&output);

        assert_eq!(commits[0].parents, vec!["p1".to_owned(), "p2".to_owned()]);
    }

    #[test]
    fn an_empty_ref_field_yields_no_refs() {
        let output = fixture(&[
            "2222222222222222222222222222222222222222\u{1f}p1\u{1f}\u{1f}Ada\u{1f}1700000002\u{1f}fix: thing",
        ]);

        let commits = parse_log(&output);

        assert!(commits[0].refs.is_empty());
    }

    #[test]
    fn splits_multiple_refs_on_comma() {
        let output = fixture(&[
            "3333333333333333333333333333333333333333\u{1f}p1\u{1f}HEAD -> main, origin/main, tag: v1\u{1f}Ada\u{1f}1\u{1f}s",
        ]);

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
        let output = fixture(&[
            "4444444444444444444444444444444444444444\u{1f}p1\u{1f}\u{1f}Ada Lovelace\u{1f}3\u{1f}feat: aggiunge il pannello — con trattino",
        ]);

        let commits = parse_log(&output);

        assert_eq!(commits[0].author, "Ada Lovelace");
        assert_eq!(
            commits[0].subject,
            "feat: aggiunge il pannello — con trattino"
        );
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

    #[test]
    fn a_default_filter_is_the_query_the_view_already_ran() {
        let args = LogFilter::default().args();

        assert_eq!(args, vec!["--branches", "--date-order"]);
        assert!(LogFilter::default().pathspec_args().is_empty());
    }

    /// Regex off must mean *literal*, not "basic regex" — otherwise typing
    /// `fix(ui)` silently searches for a group.
    #[test]
    fn text_without_regex_matches_fixed_strings() {
        let filter = LogFilter {
            text: Some("fix(ui)".to_owned()),
            ..LogFilter::default()
        };

        let args = filter.args();

        assert!(args.contains(&"-F".to_owned()));
        assert!(!args.contains(&"-E".to_owned()));
        assert!(args.contains(&"--grep=fix(ui)".to_owned()));
    }

    #[test]
    fn text_with_regex_uses_extended_syntax() {
        let filter = LogFilter {
            text: Some("^feat".to_owned()),
            regex: true,
            ..LogFilter::default()
        };

        let args = filter.args();

        assert!(args.contains(&"-E".to_owned()));
        assert!(!args.contains(&"-F".to_owned()));
    }

    #[test]
    fn case_insensitivity_is_emitted_once_and_only_when_it_can_apply() {
        let searching = LogFilter {
            text: Some("socket".to_owned()),
            ..LogFilter::default()
        };
        assert_eq!(
            searching.args().iter().filter(|arg| *arg == "-i").count(),
            1,
            "-i covers --grep and --author together, so it is emitted once"
        );

        let sensitive = LogFilter {
            case_sensitive: true,
            ..searching.clone()
        };
        assert!(!sensitive.args().contains(&"-i".to_owned()));

        assert!(
            !LogFilter::default().args().contains(&"-i".to_owned()),
            "nothing to match, so no -i"
        );
    }

    #[test]
    fn authors_repeat_and_branches_replace_the_default_selection() {
        let filter = LogFilter {
            authors: vec!["Ada".to_owned(), "Bob".to_owned()],
            branches: vec!["main".to_owned(), "release/*".to_owned()],
            ..LogFilter::default()
        };

        let args = filter.args();

        assert!(args.contains(&"--author=Ada".to_owned()));
        assert!(args.contains(&"--author=Bob".to_owned()));
        assert!(args.contains(&"--branches=main".to_owned()));
        assert!(args.contains(&"--branches=release/*".to_owned()));
        assert!(
            !args.contains(&"--branches".to_owned()),
            "an explicit selection replaces the bare --branches, it does not add to it"
        );
    }

    /// Pathspecs are deliberately *not* in `args()`. They must follow `--`,
    /// and `--` must follow `--skip`/`-n`/`--format`, which the caller adds
    /// in between. Returning them together would put the pathspec before
    /// arguments git then reads as paths.
    #[test]
    fn pathspecs_are_kept_apart_from_the_predicate_arguments() {
        let filter = LogFilter {
            paths: vec![PathBuf::from("rust/crates/tiller_git")],
            ..LogFilter::default()
        };

        assert!(!filter.args().contains(&"--".to_owned()));
        assert_eq!(
            filter.pathspec_args(),
            vec!["--".to_owned(), "rust/crates/tiller_git".to_owned()]
        );
    }

    #[test]
    fn intellisort_swaps_the_ordering_flag() {
        let filter = LogFilter {
            topo_order: true,
            ..LogFilter::default()
        };

        let args = filter.args();

        assert!(args.contains(&"--topo-order".to_owned()));
        assert!(!args.contains(&"--date-order".to_owned()));
    }

    /// Ordering is not filtering: a re-ordered history is still the whole
    /// history, so the graph stays and an empty result still means "no
    /// commits", not "no matches".
    #[test]
    fn ordering_alone_does_not_count_as_filtering() {
        assert!(!LogFilter::default().is_filtering());
        assert!(
            !LogFilter {
                topo_order: true,
                ..LogFilter::default()
            }
            .is_filtering()
        );
        assert!(
            LogFilter {
                text: Some("x".to_owned()),
                ..LogFilter::default()
            }
            .is_filtering()
        );
    }

    /// The seam that matters: predicates reach git, and they compose with
    /// `--skip`/`-n` — git applies them *before* limiting, which is the whole
    /// reason filtering can live git-side without breaking pagination.
    #[test]
    fn a_text_filter_reaches_git_and_still_paginates() {
        let dir = std::env::temp_dir().join(format!("tiller-log-filter-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp repo");

        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .expect("spawn git");
            assert!(output.status.success(), "git {args:?} failed");
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["config", "user.name", "Tester"]);
        for subject in ["alpha one", "beta", "alpha two"] {
            std::fs::write(dir.join(subject), subject).expect("write");
            git(&["add", "."]);
            git(&["commit", "-q", "-m", subject]);
        }

        let filter = LogFilter {
            text: Some("alpha".to_owned()),
            ..LogFilter::default()
        };

        let all = GitLog::commits(&dir, 0, 10, &filter).expect("filtered log");
        assert_eq!(all.len(), 2, "only the two alpha commits match");

        let second_page = GitLog::commits(&dir, 1, 10, &filter).expect("filtered log, skipped");
        assert_eq!(
            second_page.len(),
            1,
            "--skip counts within the filtered results, not within the whole log"
        );
        assert_eq!(second_page[0].subject, "alpha one");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
