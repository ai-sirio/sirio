//! Drives `sirio_forge` the way the app does, for
//! `Scripts/Tests/test-forge-e2e.sh`.
//!
//! One command per run. The answer is printed on stdout as `KEY value`
//! lines, one fact per line, so the script can match each with `grep -qxF`.
//! A `ForgeError` prints `ERR <Variant>` (plus `SSO <url>` or `RESET <unix>`
//! when the error carries one) and exits 20; bad usage exits 2. The script
//! decides which outcome is expected.
//!
//! Not a `#[test]`: it needs forges on loopback, the real `gh` and `glab`,
//! and the debug-only `SIRIO_FORGE_TEST_ENDPOINTS` door.
//!
//! ```text
//! forge_probe --forge github|gitlab --host H --project P (--cli | --token T) <command> [args]
//! ```

use std::process::ExitCode;

use sirio_forge::{
    ChangePage, ChangeState, CheckStatus, CiState, CliProgram, CliTransport, EventKind,
    FileChangeKind, Filter, Forge, ForgeClient, ForgeError, ForgeTarget, HostSetting, ListQuery,
    Means, Resolution, ReviewOutcome, ReviewState, SystemProbes, TimelineItem, TokenTransport,
    Transport, resolve,
};

enum Failure {
    Usage(String),
    Forge(ForgeError),
}

impl From<ForgeError> for Failure {
    fn from(error: ForgeError) -> Self {
        Self::Forge(error)
    }
}

fn usage(message: &str) -> Failure {
    Failure::Usage(message.to_string())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&Args::parse(&args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(message)) => {
            eprintln!("usage: {message}");
            ExitCode::from(2)
        }
        Err(Failure::Forge(error)) => {
            println!("ERR {}", error.variant_name());
            match &error {
                ForgeError::Forbidden {
                    sso_url: Some(url), ..
                } => println!("SSO {url}"),
                ForgeError::RateLimited {
                    reset_at: Some(reset),
                    ..
                } => println!("RESET {reset}"),
                _ => {}
            }
            ExitCode::from(20)
        }
    }
}

/// `--name value` flags, `--cli`/`--more` switches, and the bare words in
/// order.
struct Args {
    flags: Vec<(String, String)>,
    switches: Vec<String>,
    words: Vec<String>,
}

impl Args {
    const SWITCHES: [&'static str; 2] = ["cli", "more"];

    fn parse(raw: &[String]) -> Self {
        let mut args = Self {
            flags: Vec::new(),
            switches: Vec::new(),
            words: Vec::new(),
        };
        let mut iter = raw.iter();
        while let Some(item) = iter.next() {
            match item.strip_prefix("--") {
                Some(name) if Self::SWITCHES.contains(&name) => {
                    args.switches.push(name.to_string())
                }
                Some(name) => {
                    let value = iter.next().cloned().unwrap_or_default();
                    args.flags.push((name.to_string(), value));
                }
                None => args.words.push(item.clone()),
            }
        }
        args
    }

    fn flag(&self, name: &str) -> Option<&str> {
        self.flags
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn switch(&self, name: &str) -> bool {
        self.switches.iter().any(|switch| switch == name)
    }
}

fn forge_flag(args: &Args, name: &str) -> Result<Forge, Failure> {
    match args.flag(name) {
        Some("github") => Ok(Forge::GitHub),
        Some("gitlab") => Ok(Forge::GitLab),
        _ => Err(usage(&format!("--{name} github|gitlab"))),
    }
}

fn client(args: &Args) -> Result<ForgeClient, Failure> {
    let forge = forge_flag(args, "forge")?;
    let host = args
        .flag("host")
        .ok_or_else(|| usage("--host H"))?
        .to_string();
    let project = args
        .flag("project")
        .ok_or_else(|| usage("--project P"))?
        .to_string();
    let transport: Box<dyn Transport> = if args.switch("cli") {
        Box::new(CliTransport::new(CliProgram::for_forge(forge), &host))
    } else {
        let token = args
            .flag("token")
            .ok_or_else(|| usage("--cli or --token T"))?;
        Box::new(TokenTransport::new(forge, &host, token.to_string()))
    };
    Ok(ForgeClient::new(
        forge,
        ForgeTarget { host, project },
        transport,
    ))
}

fn run(args: &Args) -> Result<(), Failure> {
    let command = args
        .words
        .first()
        .cloned()
        .ok_or_else(|| usage("a command"))?;
    if command == "resolve" {
        return resolve_command(args);
    }
    let client = client(args)?;
    match command.as_str() {
        "viewer" => println!("VIEWER {}", client.viewer()?),
        "list" => {
            let filter = match args.words.get(1).map(String::as_str) {
                Some("mine") => Filter::Mine,
                Some("to-review") => Filter::ToReview,
                Some("all-open") => Filter::AllOpen,
                Some("closed") => Filter::ClosedAndMerged,
                _ => {
                    return Err(usage(
                        "list mine|to-review|all-open|closed [--search S] [--more]",
                    ));
                }
            };
            let query = ListQuery {
                filter,
                search: args.flag("search").map(str::to_string),
            };
            let first = client.list(&query, None)?;
            print_page(1, &first);
            if args.switch("more")
                && let Some(next) = &first.next
            {
                print_page(2, &client.list(&query, Some(next))?);
            }
        }
        "count" => println!("COUNT {}", client.to_review_count()?),
        "branch" => {
            let name = args
                .words
                .get(1)
                .ok_or_else(|| usage("branch NAME [--owner O]"))?;
            match client.for_branch(name, args.flag("owner"))? {
                Some(found) => println!(
                    "BRANCH {} {}",
                    found.reference.label(),
                    state_word(found.state)
                ),
                None => println!("BRANCH none"),
            }
        }
        "header" => {
            let header = client.header(number(args)?)?;
            println!(
                "HEADER {} state={} additions={} deletions={} files={} commits={} truncated={}",
                header.summary.reference.label(),
                state_word(header.summary.state),
                count_word(header.additions),
                count_word(header.deletions),
                count_word(header.changed_files),
                count_word(header.commit_count),
                yes_no(header.timeline_truncated),
            );
            println!("BODY {}", header.body.lines().next().unwrap_or(""));
            for reviewer in &header.reviewers {
                println!(
                    "REVIEWER {} {}",
                    reviewer.login,
                    outcome_word(reviewer.outcome)
                );
            }
            for item in &header.timeline {
                println!("{}", timeline_line(item));
            }
        }
        "commits" => {
            let listing = client.commits(number(args)?)?;
            for commit in &listing.items {
                println!(
                    "COMMIT {} {} {}",
                    commit.short_sha, commit.author, commit.title
                );
            }
            println!("TRUNCATED {}", yes_no(listing.truncated));
        }
        "checks" => {
            let listing = client.checks(number(args)?)?;
            for check in &listing.items {
                println!(
                    "CHECK {} {} {} {}",
                    check_word(check.status),
                    check.group.as_deref().unwrap_or("-"),
                    check.name,
                    count_word(check.duration_secs.map(|secs| secs as u32)),
                );
            }
            println!("TRUNCATED {}", yes_no(listing.truncated));
        }
        "files" => {
            let listing = client.files(number(args)?)?;
            for file in &listing.items {
                println!(
                    "FILE {} +{} -{} {}",
                    kind_word(file.kind),
                    file.additions,
                    file.deletions,
                    file.path
                );
            }
            println!("TRUNCATED {}", yes_no(listing.truncated));
        }
        "create-url" => {
            let branch = args
                .words
                .get(1)
                .ok_or_else(|| usage("create-url BRANCH"))?;
            println!("CREATE {}", client.creation_url(branch));
        }
        other => return Err(usage(&format!("unknown command {other}"))),
    }
    Ok(())
}

fn forge_word(forge: Forge) -> &'static str {
    match forge {
        Forge::GitHub => "github",
        Forge::GitLab => "gitlab",
    }
}

/// `resolve --host H [--setting github|gitlab[:cli|token]] [--token-forge github|gitlab]`
fn resolve_command(args: &Args) -> Result<(), Failure> {
    let host = args.flag("host").ok_or_else(|| usage("--host H"))?;
    let setting = match args.flag("setting") {
        None => None,
        Some(text) => {
            let (forge, means) = match text.split_once(':') {
                Some((forge, means)) => (forge, Some(means)),
                None => (text, None),
            };
            let forge = match forge {
                "github" => Forge::GitHub,
                "gitlab" => Forge::GitLab,
                _ => return Err(usage("--setting github|gitlab[:cli|token]")),
            };
            let means = match means {
                None => None,
                Some("cli") => Some(Means::Cli),
                Some("token") => Some(Means::Token),
                Some(_) => return Err(usage("--setting github|gitlab[:cli|token]")),
            };
            Some(HostSetting {
                host: host.to_string(),
                forge,
                means,
            })
        }
    };
    let stored = match args.flag("token-forge") {
        None => None,
        Some(_) => Some(forge_flag(args, "token-forge")?),
    };
    match resolve(host, setting.as_ref(), stored, &SystemProbes) {
        Resolution::Ready { forge, means } => {
            let means = match means {
                Means::Cli => "cli",
                Means::Token => "token",
            };
            println!("RESOLVE ready {} {means}", forge_word(forge));
        }
        Resolution::NotConnected { forge } => {
            println!("RESOLVE not-connected {}", forge_word(forge))
        }
        Resolution::UnknownForge => println!("RESOLVE unknown"),
    }
    Ok(())
}

fn number(args: &Args) -> Result<u64, Failure> {
    args.words
        .get(1)
        .and_then(|word| word.parse().ok())
        .ok_or_else(|| usage("a change request number"))
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn count_word(value: Option<u32>) -> String {
    value.map_or_else(|| "-".to_string(), |value| value.to_string())
}

fn state_word(state: ChangeState) -> &'static str {
    match state {
        ChangeState::Draft => "draft",
        ChangeState::Open => "open",
        ChangeState::Merged => "merged",
        ChangeState::Closed => "closed",
    }
}

fn ci_word(ci: CiState) -> String {
    match ci {
        CiState::NoChecks => "none".to_string(),
        CiState::Running(Some(progress)) => format!("running:{}/{}", progress.done, progress.total),
        CiState::Running(None) => "running".to_string(),
        CiState::Passed => "passed".to_string(),
        CiState::Failed => "failed".to_string(),
        CiState::Canceled => "canceled".to_string(),
    }
}

fn review_word(review: ReviewState) -> String {
    match review {
        ReviewState::Approved { count } => format!("approved:{count}"),
        ReviewState::ChangesRequested => "changes".to_string(),
        ReviewState::ReviewRequired => "required".to_string(),
        ReviewState::None => "none".to_string(),
    }
}

fn outcome_word(outcome: ReviewOutcome) -> &'static str {
    match outcome {
        ReviewOutcome::Approved => "approved",
        ReviewOutcome::ChangesRequested => "changes",
        ReviewOutcome::Commented => "commented",
        ReviewOutcome::Dismissed => "dismissed",
        ReviewOutcome::Requested => "requested",
        ReviewOutcome::Other => "other",
    }
}

fn check_word(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Queued => "queued",
        CheckStatus::Running => "running",
        CheckStatus::Passed => "passed",
        CheckStatus::Failed => "failed",
        CheckStatus::Canceled => "canceled",
        CheckStatus::Skipped => "skipped",
        CheckStatus::Neutral => "neutral",
    }
}

fn kind_word(kind: Option<FileChangeKind>) -> &'static str {
    match kind {
        Some(FileChangeKind::Added) => "added",
        Some(FileChangeKind::Modified) => "modified",
        Some(FileChangeKind::Deleted) => "deleted",
        Some(FileChangeKind::Renamed) => "renamed",
        Some(FileChangeKind::Copied) => "copied",
        None => "-",
    }
}

fn print_page(index: u32, page: &ChangePage) {
    println!(
        "PAGE {index} rows={} next={}",
        page.items.len(),
        yes_no(page.next.is_some())
    );
    for item in &page.items {
        println!(
            "ROW {} state={} ci={} review={} me={} comments={} source={} owner={} author={} title={}",
            item.reference.label(),
            state_word(item.state),
            ci_word(item.ci),
            review_word(item.review),
            yes_no(item.review_requested_from_me),
            item.comments,
            item.source_branch,
            item.source_owner.as_deref().unwrap_or("-"),
            item.author,
            item.title,
        );
    }
}

fn timeline_line(item: &TimelineItem) -> String {
    match item {
        TimelineItem::Comment { author, .. } => format!("COMMENT {author}"),
        TimelineItem::Review {
            author,
            outcome,
            line_comments,
            ..
        } => {
            format!(
                "REVIEW {author} {} lines={}",
                outcome_word(*outcome),
                line_comments.len()
            )
        }
        TimelineItem::LineComment(comment) => format!(
            "LINE {} {}:{}",
            comment.author,
            comment.path,
            count_word(comment.line)
        ),
        TimelineItem::Event { kind, .. } => format!(
            "EVENT {}",
            match kind {
                EventKind::CommitsPushed { count } => format!("commits:{count}"),
                EventKind::ReviewRequested { reviewer } => format!("review-requested:{reviewer}"),
                EventKind::Merged => "merged".to_string(),
                EventKind::Closed => "closed".to_string(),
                EventKind::Reopened => "reopened".to_string(),
                EventKind::ReadyForReview => "ready".to_string(),
                EventKind::ConvertedToDraft => "draft".to_string(),
                EventKind::Other(text) => format!("other:{text}"),
            }
        ),
    }
}
