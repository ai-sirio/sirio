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
//! forge_probe ... act N <action> [--body TEXT | --body-file PATH] [--title T] [--target BRANCH] [--id ID --kind comment|review]
//! ```
//!
//! `act`'s actions: `comment`, `approve`, `request-changes`, `review-comment`,
//! `close`, `reopen`, `ready`, `draft`, `edit`, `edit-comment`. It prints
//! `ACT ok`, then `WARNING <text>` when a second step failed.

use std::process::ExitCode;

use sirio_forge::{
    Action, Capabilities, ChangePage, ChangeState, CheckStatus, CiState, CliProgram, CliTransport,
    CommentKind, CommentRef, EventKind, FileChangeKind, Filter, Forge, ForgeClient, ForgeError,
    ForgeTarget, HostSetting, ListQuery, Means, MergeCapability, MergeMethod, MergeVerdict,
    Resolution, ReviewOutcome, ReviewState,
    ReviewVerdict, SystemProbes, TimelineItem, TokenTransport, Transport, resolve,
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
            match &error {
                ForgeError::Forbidden { detail, .. } => println!("DETAIL {detail}"),
                ForgeError::Rejected { message, .. } => println!("MESSAGE {message}"),
                ForgeError::Unsupported { what, .. } => println!("WHAT {what}"),
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
            match &header.revisions {
                Some(revisions) => println!(
                    "REVISIONS base={} head={}",
                    &revisions.base_sha[..7],
                    &revisions.head_sha[..7]
                ),
                None => println!("REVISIONS none"),
            }
            println!("BODY {}", header.body.lines().next().unwrap_or(""));
            println!("CAPS {}", caps_words(&header.capabilities));
            println!("{}", merge_line(&header.capabilities.merge));
            for label in &header.labels {
                println!("LABEL {} {}", label.id, label.name);
            }
            for item in &header.timeline {
                if let Some(edit) = edit_of(item) {
                    println!("EDITABLE {} {}", kind_name(edit.kind), edit.id);
                }
            }
            for reviewer in &header.reviewers {
                println!(
                    "REVIEWER {} {}",
                    reviewer.login,
                    outcome_word(reviewer.outcome)
                );
                if let Some(id) = &reviewer.id {
                    println!("REVIEWER_ID {} {id}", reviewer.login);
                }
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
        "act" => act_command(&client, args)?,
        "candidates" => {
            let kind = args.words.get(1).ok_or_else(|| usage("candidates reviewers|labels N"))?;
            let text = args.flag("text").unwrap_or_default();
            let found = match kind.as_str() {
                "reviewers" => {
                    let number = args
                        .words
                        .get(2)
                        .and_then(|word| word.parse().ok())
                        .ok_or_else(|| usage("candidates reviewers N"))?;
                    client.reviewer_candidates(number, text)?
                }
                "labels" => client.label_candidates(text)?,
                other => return Err(usage(&format!("unknown candidates {other}"))),
            };
            for candidate in found {
                println!(
                    "CANDIDATE {} {} {}",
                    candidate.id,
                    candidate.label,
                    candidate.note.as_deref().unwrap_or("-")
                );
            }
        }
        "scopes" => match client.token_scopes() {
            Some(scopes) => {
                let listed = if scopes.0.is_empty() { "-".to_string() } else { scopes.0.join(",") };
                println!("SCOPES {listed}");
                println!("CAN_WRITE {}", yes_no(scopes.allows_writing(client.forge())));
            }
            None => println!("SCOPES not-reported"),
        },
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

fn caps_words(caps: &Capabilities) -> String {
    let words: Vec<&str> = [
        (caps.can_comment, "comment"),
        (caps.can_approve, "approve"),
        (caps.can_request_changes, "request-changes"),
        (caps.can_edit, "edit"),
        (caps.can_change_state, "state"),
        (caps.can_toggle_draft, "draft"),
    ]
    .into_iter()
    .filter_map(|(on, word)| on.then_some(word))
    .collect();
    if words.is_empty() {
        "-".to_string()
    } else {
        words.join(",")
    }
}

/// The merge strip's facts in one line: the verdict, then what is offered.
fn merge_line(merge: &MergeCapability) -> String {
    let verdict = match &merge.verdict {
        MergeVerdict::Unreported => "unreported".to_string(),
        MergeVerdict::Ready => "ready".to_string(),
        MergeVerdict::WaitingOnChecks => "waiting".to_string(),
        MergeVerdict::Blocked(reason) => format!("blocked:{}", reason.text().replace(' ', "-")),
    };
    let methods: Vec<&str> = merge.methods.list().into_iter().map(MergeMethod::word).collect();
    format!(
        "MERGE {verdict} methods={} default={} auto={} enabled={} delete-default={}",
        if methods.is_empty() { "-".to_string() } else { methods.join(",") },
        merge.default_method.map_or("-", MergeMethod::word),
        yes_no(merge.can_auto_merge),
        merge.auto_merge_enabled.map_or("-", MergeMethod::word),
        yes_no(merge.delete_branch_default),
    )
}

/// Every value of a repeatable flag (`--add a --add b`).
fn all_flags(args: &Args, name: &str) -> Vec<String> {
    args.flags
        .iter()
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
        .collect()
}

fn edit_of(item: &TimelineItem) -> Option<&CommentRef> {
    match item {
        TimelineItem::Comment { edit, .. } | TimelineItem::Review { edit, .. } => edit.as_ref(),
        _ => None,
    }
}

fn kind_name(kind: CommentKind) -> &'static str {
    match kind {
        CommentKind::Comment => "comment",
        CommentKind::Review => "review",
    }
}

/// The body of an action: `--body-file` (read as it is, newlines and all) or
/// `--body`, or nothing.
fn body_of(args: &Args) -> Result<String, Failure> {
    if let Some(path) = args.flag("body-file") {
        return std::fs::read_to_string(path).map_err(|error| usage(&format!("--body-file: {error}")));
    }
    Ok(args.flag("body").unwrap_or_default().to_string())
}

/// `act N <action> ...` — one write, then what came of it.
fn act_command(client: &ForgeClient, args: &Args) -> Result<(), Failure> {
    let number = number(args)?;
    let name = args
        .words
        .get(2)
        .ok_or_else(|| usage("act N <action>"))?
        .as_str();
    let action = match name {
        "comment" => Action::Comment { body: body_of(args)? },
        "approve" => Action::Review { verdict: ReviewVerdict::Approve, body: body_of(args)? },
        "request-changes" => Action::Review { verdict: ReviewVerdict::RequestChanges, body: body_of(args)? },
        "review-comment" => Action::Review { verdict: ReviewVerdict::Comment, body: body_of(args)? },
        "close" => Action::Close,
        "reopen" => Action::Reopen,
        "ready" => Action::MarkReady,
        "draft" => Action::ConvertToDraft,
        "edit" => Action::Edit {
            title: args.flag("title").map(str::to_string),
            body: match (args.flag("body"), args.flag("body-file")) {
                (None, None) => None,
                _ => Some(body_of(args)?),
            },
            target_branch: args.flag("target").map(str::to_string),
        },
        "edit-comment" => Action::EditComment {
            comment: CommentRef {
                id: args.flag("id").ok_or_else(|| usage("edit-comment needs --id"))?.to_string(),
                kind: match args.flag("kind") {
                    Some("review") => CommentKind::Review,
                    _ => CommentKind::Comment,
                },
            },
            body: body_of(args)?,
        },
        "merge" => {
            let method = match args.flag("method") {
                Some("squash") => MergeMethod::Squash,
                Some("rebase") => MergeMethod::Rebase,
                Some("merge") | None => MergeMethod::Merge,
                Some(other) => return Err(usage(&format!("unknown method {other}"))),
            };
            Action::Merge {
                method,
                commit_title: args.flag("title").map(str::to_string),
                commit_message: args.flag("message").map(str::to_string),
                delete_branch: args.flag("delete-branch") == Some("yes"),
                when_checks_pass: args.flag("when-checks-pass") == Some("yes"),
                expected_head: args.flag("head").ok_or_else(|| usage("merge needs --head"))?.to_string(),
            }
        }
        "cancel-auto-merge" => Action::CancelAutoMerge,
        "set-reviewers" => Action::SetReviewers {
            add: all_flags(args, "add"),
            remove: all_flags(args, "remove"),
        },
        "set-labels" => Action::SetLabels {
            add: all_flags(args, "add"),
            remove: all_flags(args, "remove"),
        },
        other => return Err(usage(&format!("unknown action {other}"))),
    };
    let outcome = client.act(number, &action)?;
    println!("ACT ok");
    if let Some(warning) = outcome.warning {
        println!("WARNING {warning}");
    }
    Ok(())
}
