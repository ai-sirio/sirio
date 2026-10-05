//! One change request, read-only, in the Secondary half of the centre split
//! (spec §7.2, mockup A): a fixed header, then Conversation, Commits, Checks
//! and Files. Its identity is a `ChangeRef`, which is what the host keys the
//! tab by and what the session store keeps.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FontWeight, Hsla, IntoElement, Render, Task,
    Window, div, prelude::*, px,
};
use sirio_forge::{
    Action, ChangeHeader, ChangeRef, Check, CheckJob, CheckStatus, CiState, CommitSummary, EventKind, FileChange,
    FileChangeKind, Forge, ForgeClient, ForgeError, Listing, Revisions, ReviewOutcome,
    ReviewThread, RerunTarget, TimelineItem,
};
use sirio_theme::Theme;

mod actions;
mod compose;
mod composer;
mod edit;
mod merge;
mod people;
mod suggestion;
mod threads;

use crate::change_request_style as style;
use ely_gpui_component::{
    data_display::{Timeline, TimelineItem as RailItem, Tone},
    forms::Choice,
    navigation::Tabs,
    primitives::{Icon as EIcon, IconName, Severity},
    theme::IconSize as EIconSize,
};
use crate::ely_ui::{self, ButtonState};
use crate::changes::{ChangesTab, ChangesTabEvent, ForgeFiles};
use crate::diff_annotations::AnnotationSide;
use threads::{ConversationEntry, merge_threads};
use crate::chat::{Chat, LinkClickOverride};
use crate::forge_source::{self, Connection, RevisionError};
use crate::text_selection::selectable_text;

/// The inner tab a change request shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InnerTab {
    #[default]
    Conversation,
    Commits,
    Checks,
    Files,
}

impl InnerTab {
    pub const ALL: [Self; 4] = [Self::Conversation, Self::Commits, Self::Checks, Self::Files];

    /// The session store's spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::Commits => "commits",
            Self::Checks => "checks",
            Self::Files => "files",
        }
    }

    /// Anything unknown — a session written before, or by a later build —
    /// opens on the conversation.
    pub fn parse(value: &str) -> Self {
        match value {
            "commits" => Self::Commits,
            "checks" => Self::Checks,
            "files" => Self::Files,
            _ => Self::Conversation,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Conversation => "Conversation",
            Self::Commits => "Commits",
            Self::Checks => "Checks",
            Self::Files => "Files",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeRequestTabEvent {
    /// The title the forge now reports; the host's tab strip follows it.
    TitleChanged(String),
    /// A commit was clicked: the host opens it locally when the object is in
    /// the repository, otherwise on the forge.
    OpenCommit { sha: String, web_url: String },
    /// *Open in editor* on a file of the diff: the host opens the local file
    /// when the worktree is at `revisions.head_sha`, a read-only snapshot
    /// otherwise (`deleted`: at the base, where the file still exists).
    OpenFile {
        path: PathBuf,
        line: Option<u32>,
        revisions: Revisions,
        deleted: bool,
    },
    /// Open a CI job's log in its own tab (spec §15.1).
    OpenLog {
        job: CheckJob,
        name: String,
        web_url: Option<String>,
    },
    /// The user closed a tab that can no longer reach its change request.
    Close,
    /// A write reached the forge and the tab re-read it: the host refreshes
    /// the right panel's list now instead of at its next tick.
    Changed,
}

/// One piece of what the tab shows.
pub(crate) enum Slot<T> {
    Idle,
    Loading,
    /// Loaded. `stale` is a later refresh's failure, shown as a line above
    /// the data instead of in place of it (spec §10).
    Loaded {
        value: T,
        stale: Option<ForgeError>,
    },
    Failed(ForgeError),
}

impl<T> Slot<T> {
    pub(crate) fn value(&self) -> Option<&T> {
        match self {
            Self::Loaded { value, .. } => Some(value),
            _ => None,
        }
    }

    pub(crate) fn begin(&mut self) {
        if !matches!(self, Self::Loaded { .. }) {
            *self = Self::Loading;
        }
    }

    pub(crate) fn finish(&mut self, result: Result<T, ForgeError>) {
        *self = match (result, std::mem::replace(self, Self::Idle)) {
            (Ok(value), _) => Self::Loaded { value, stale: None },
            (Err(error), Self::Loaded { value, .. }) => Self::Loaded {
                value,
                stale: Some(error),
            },
            (Err(error), _) => Self::Failed(error),
        };
    }
}

/// A selected tab refreshes when chosen, unless it did just now.
const FRESH_FOR: Duration = Duration::from_secs(10);
/// While its CI runs, a tab refreshes on its own at this pace (spec §9).
const CI_REFRESH: Duration = Duration::from_secs(60);
/// How soon to look again while the forge works out whether it can merge.
const MERGE_CHECK_REFRESH: Duration = Duration::from_secs(5);

/// The *Files* inner tab's diff, beside the forge's own list (spec §7.1),
/// which stays the fallback for every state but `Ready`.
pub(crate) enum RangeState {
    /// Files was not shown yet, or the header is still loading.
    Idle,
    /// The revisions are being made local.
    Fetching,
    Ready {
        revisions: Revisions,
        changes: Entity<ChangesTab>,
    },
    /// `revisions` is the pair that failed: the periodic header refresh does
    /// not retry it — only Retry does.
    Failed {
        error: RevisionError,
        revisions: Option<Revisions>,
    },
    /// The forge did not report base and head.
    NoRevisions,
}

fn short(sha: &str) -> &str {
    forge_source::short_sha(sha)
}

/// One sentence to add under a failed fetch when git could not sign in:
/// the fetch runs with git's own credentials, and a token given to Sirio
/// does not reach it — the first thing a token-means user on a private
/// https remote hits. `None` for every other failure.
pub(crate) fn error_hint(error: &RevisionError) -> Option<&'static str> {
    let RevisionError::FetchFailed { detail } = error else {
        return None;
    };
    let could_not_sign_in = ["terminal prompts disabled", "could not read Username", "Permission denied (publickey)"]
        .iter()
        .any(|mark| detail.contains(mark));
    could_not_sign_in.then_some(
        "Sirio fetches with git's own credentials (a credential helper or an ssh key), not with the token — for GitHub, `gh auth setup-git` sets one up.",
    )
}

pub struct ChangeRequestTab {
    reference: ChangeRef,
    title: String,
    inner: InnerTab,
    started: bool,
    client: Option<Arc<ForgeClient>>,
    /// Why there is no client — shown with Retry and Close; the tab is never
    /// dropped on its own (spec §8).
    pub(crate) unreachable: Option<String>,
    pub(crate) header: Slot<ChangeHeader>,
    pub(crate) description: Option<markdown::Doc>,
    /// The Markdown of each timeline entry that has a body, aligned with it.
    bodies: Vec<Option<markdown::Doc>>,
    /// First published thread bodies for the Conversation, cached by forge id.
    thread_bodies: HashMap<String, markdown::Doc>,
    thread_ids: HashMap<u64, String>,
    pub(crate) commits: Slot<Listing<CommitSummary>>,
    pub(crate) checks: Slot<Listing<Check>>,
    pub(crate) files: Slot<Listing<FileChange>>,
    /// Review threads (spec §4): read with *Files* and on every refresh but
    /// the CI timer's.
    pub(crate) threads: Slot<Listing<ReviewThread>>,
    threads_generation: u64,
    threads_task: Option<Task<()>>,
    /// The cards the diff draws, by annotation key; outdated sections by path.
    thread_views: HashMap<u64, Entity<threads::ThreadView>>,
    outdated_views: HashMap<String, Entity<threads::OutdatedView>>,
    line_composer: Option<compose::LineComposer>,
    pub(crate) write_target: Option<compose::WriteTarget>,
    write_refusal: Option<(compose::WriteTarget, String)>,
    check_folds: HashMap<String, bool>,
    generation: u64,
    /// A rate limit's reset: no request before it (spec §9).
    paused_until: Option<i64>,
    last_refresh: Option<Instant>,
    connect_task: Option<Task<()>>,
    header_task: Option<Task<()>>,
    commits_task: Option<Task<()>>,
    checks_task: Option<Task<()>>,
    files_task: Option<Task<()>>,
    /// The next look, and how long it waits: a minute while CI runs, a few
    /// seconds while the forge is still checking mergeability.
    ci_timer: Option<(Duration, Task<()>)>,
    /// The worktree the tab was opened in: where its revisions are made
    /// local and its diff is read.
    worktree: PathBuf,
    pub(crate) range: RangeState,
    range_task: Option<Task<()>>,
    range_subscription: Option<gpui::Subscription>,
    /// "Updated to head …" once a new head rebuilt the diff.
    updated_notice: Option<String>,
    /// A `reveal` asked before the diff existed.
    pending_reveal: Option<(PathBuf, AnnotationSide, Option<u32>, Option<u64>)>,
    /// A commit whose revisions could not be made local, with its forge URL.
    commit_error: Option<(RevisionError, String)>,
    commit_task: Option<Task<()>>,
    /// The write in flight, and the fields of an edit in progress.
    actions: actions::ActionsState,
}

impl ChangeRequestTab {
    pub fn new(
        reference: ChangeRef,
        title: String,
        worktree: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::restored(reference, title, InnerTab::Conversation, worktree, cx)
    }

    /// A tab brought back from the session: it shows its saved title at once
    /// and loads when it is shown (spec §8).
    pub fn restored(
        reference: ChangeRef,
        title: String,
        inner: InnerTab,
        worktree: PathBuf,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            reference,
            title,
            inner,
            started: false,
            client: None,
            unreachable: None,
            header: Slot::Idle,
            description: None,
            bodies: Vec::new(),
            thread_bodies: HashMap::new(),
            thread_ids: HashMap::new(),
            commits: Slot::Idle,
            checks: Slot::Idle,
            files: Slot::Idle,
            threads: Slot::Idle,
            threads_generation: 0,
            threads_task: None,
            thread_views: HashMap::new(),
            outdated_views: HashMap::new(),
            line_composer: None,
            write_target: None,
            write_refusal: None,
            check_folds: HashMap::new(),
            generation: 0,
            paused_until: None,
            last_refresh: None,
            connect_task: None,
            header_task: None,
            commits_task: None,
            checks_task: None,
            files_task: None,
            ci_timer: None,
            worktree,
            range: RangeState::Idle,
            range_task: None,
            range_subscription: None,
            updated_notice: None,
            pending_reveal: None,
            commit_error: None,
            commit_task: None,
            actions: actions::ActionsState::new(),
        }
    }

    /// Opens the log of the loaded check whose job is `job_id` — what a row's
    /// click does, and `surface.ci_log.open`.
    pub fn open_log_by_job(&mut self, job_id: u64, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(listing) = self.checks.value() else {
            return Err("the checks are not loaded".to_string());
        };
        let Some(check) = listing.items.iter().find(|check| check.job.as_ref().is_some_and(|job| job.job_id == job_id)) else {
            return Err(format!("no CI job {job_id} among the checks"));
        };
        cx.emit(ChangeRequestTabEvent::OpenLog {
            job: check.job.clone().expect("found by its job"),
            name: check.name.clone(),
            web_url: check.url.clone(),
        });
        Ok(())
    }

    pub fn reference(&self) -> &ChangeRef {
        &self.reference
    }

    pub fn inner_tab(&self) -> InnerTab {
        self.inner
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// The strip title: `#578 Fix the login redirect`, or the label alone.
    pub fn tab_title(reference: &ChangeRef, title: &str) -> String {
        if title.is_empty() {
            reference.label()
        } else {
            format!("{} {title}", reference.label())
        }
    }

    /// The bare title inside a saved strip title.
    pub fn title_from_tab(reference: &ChangeRef, tab_title: &str) -> String {
        tab_title
            .strip_prefix(&reference.label())
            .map(str::trim)
            .unwrap_or(tab_title)
            .to_string()
    }

    /// The host calls this whenever the tab is chosen.
    pub fn on_selected(&mut self, cx: &mut Context<Self>) {
        if !self.started {
            self.connect(cx);
        } else if self.last_refresh.is_none_or(|at| at.elapsed() > FRESH_FOR) {
            self.refresh(cx);
        }
    }

    fn connect(&mut self, cx: &mut Context<Self>) {
        self.started = true;
        let Some(source) = forge_source::source(cx) else {
            self.unreachable = Some("Change requests are unavailable in this build.".to_string());
            cx.notify();
            return;
        };
        self.unreachable = None;
        let reference = self.reference.clone();
        self.connect_task = Some(cx.spawn(async move |this, cx| {
            let answer = cx
                .background_spawn(async move { source.client_for(&reference) })
                .await;
            let _ = this.update(cx, |tab, cx| match answer {
                Ok(client) => {
                    tab.client = Some(client);
                    tab.refresh(cx);
                }
                Err(connection) => {
                    tab.unreachable = Some(unreachable_text(&connection));
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    /// Forget what the host resolved for this forge, and try again.
    pub(crate) fn retry(&mut self, cx: &mut Context<Self>) {
        if let Some(source) = forge_source::source(cx) {
            source.forget(&self.reference.host);
        }
        self.client = None;
        self.connect(cx);
    }

    /// While a rate limit's reset is in the future, no path asks the forge.
    fn rate_paused(&mut self) -> bool {
        if let Some(until) = self.paused_until {
            if style::now() < until {
                return true;
            }
            self.paused_until = None;
        }
        false
    }

    fn note_rate_limited(&mut self, error: &ForgeError) {
        if let ForgeError::RateLimited {
            reset_at: Some(reset),
            ..
        } = error
        {
            self.paused_until = Some(*reset);
        }
    }

    /// Everything the tab shows, threads included: a manual refresh, a retry,
    /// the re-read after a write.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.refresh_header(cx);
        self.load_threads(cx);
    }

    /// The header and the inner tab shown, without the threads: what the CI
    /// timer asks for.
    fn refresh_header(&mut self, cx: &mut Context<Self>) {
        if self.rate_paused() {
            // Stay armed: the pause ends by itself, and a running CI is still
            // worth looking at once it does.
            self.schedule_refresh(self.header.value().and_then(refresh_wait), cx);
            return;
        }
        let Some(client) = self.client.clone() else {
            self.connect(cx);
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        self.last_refresh = Some(Instant::now());
        self.header.begin();
        let number = self.reference.number;
        let header_client = client.clone();
        self.header_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { header_client.header(number) })
                .await;
            let _ = this.update(cx, |tab, cx| {
                if tab.generation == generation {
                    tab.apply_header(result, cx);
                }
            });
        }));
        if self.inner != InnerTab::Conversation {
            self.load_inner(self.inner, cx);
        }
        cx.notify();
    }

    fn apply_header(&mut self, result: Result<ChangeHeader, ForgeError>, cx: &mut Context<Self>) {
        if let Ok(header) = &result {
            if header.summary.title != self.title {
                self.title = header.summary.title.clone();
                cx.emit(ChangeRequestTabEvent::TitleChanged(self.title.clone()));
            }
            let theme = *Theme::get(cx);
            self.description = Some(markdown_doc(&header.body, &theme));
            self.bodies = header
                .timeline
                .iter()
                .map(|item| {
                    body_of(item)
                        .filter(|body| !body.trim().is_empty())
                        .map(|body| markdown_doc(body, &theme))
                })
                .collect();
            self.schedule_refresh(refresh_wait(header), cx);
        }
        if let Err(error) = &result {
            self.note_rate_limited(error);
            // The timer cleared itself before asking; a failed answer must not
            // end the polling while the header we still show says CI runs.
            self.schedule_refresh(self.header.value().and_then(refresh_wait), cx);
        }
        let succeeded = result.is_ok();
        self.header.finish(result);
        self.ensure_range(cx);
        if succeeded {
            if let RangeState::Ready { changes, .. } = &self.range {
                let (changes, commentable) = (changes.clone(), self.commentable(cx));
                changes.update(cx, |changes, cx| changes.set_commentable(commentable, cx));
            }
        }
        cx.notify();
    }

    /// Look again after `wait`, or stop when there is nothing to wait for
    /// (spec §9). A shorter wait replaces a longer one already armed.
    fn schedule_refresh(&mut self, wait: Option<Duration>, cx: &mut Context<Self>) {
        let Some(wait) = wait else {
            self.ci_timer = None;
            return;
        };
        if self.ci_timer.as_ref().is_some_and(|(armed, _)| *armed <= wait) {
            return;
        }
        self.ci_timer = Some((
            wait,
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |tab, cx| {
                    tab.ci_timer = None;
                    tab.refresh_header(cx);
                });
            }),
        ));
    }

    pub fn select_inner(&mut self, inner: InnerTab, cx: &mut Context<Self>) {
        self.inner = inner;
        let idle = match inner {
            InnerTab::Conversation => false,
            InnerTab::Commits => matches!(self.commits, Slot::Idle),
            InnerTab::Checks => matches!(self.checks, Slot::Idle),
            InnerTab::Files => matches!(self.files, Slot::Idle),
        };
        if idle {
            self.load_inner(inner, cx);
        }
        if inner == InnerTab::Files {
            if matches!(self.threads, Slot::Idle) {
                self.load_threads(cx);
            }
            self.ensure_range(cx);
        }
        cx.notify();
    }

    fn load_inner(&mut self, inner: InnerTab, cx: &mut Context<Self>) {
        if self.rate_paused() {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let number = self.reference.number;
        let generation = self.generation;
        match inner {
            InnerTab::Conversation => {}
            InnerTab::Commits => {
                self.commits.begin();
                self.commits_task = Some(cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_spawn(async move { client.commits(number) })
                        .await;
                    let _ = this.update(cx, |tab, cx| {
                        if tab.generation == generation {
                            if let Err(error) = &result {
                                tab.note_rate_limited(error);
                            }
                            tab.commits.finish(result);
                            cx.notify();
                        }
                    });
                }));
            }
            InnerTab::Checks => {
                self.checks.begin();
                self.checks_task = Some(cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_spawn(async move { client.checks(number) })
                        .await;
                    let _ = this.update(cx, |tab, cx| {
                        if tab.generation == generation {
                            if let Err(error) = &result {
                                tab.note_rate_limited(error);
                            }
                            tab.checks.finish(result);
                            cx.notify();
                        }
                    });
                }));
            }
            InnerTab::Files => {
                self.files.begin();
                self.files_task = Some(cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_spawn(async move { client.files(number) })
                        .await;
                    let _ = this.update(cx, |tab, cx| {
                        if tab.generation == generation {
                            if let Err(error) = &result {
                                tab.note_rate_limited(error);
                            }
                            tab.files.finish(result);
                            cx.notify();
                        }
                    });
                }));
            }
        }
    }

    /// Called whenever Files may need its diff: when it is selected, and when a
    /// header lands. Does nothing until the header is here (it carries the
    /// revisions), and nothing while the same revisions are fetching, ready
    /// or already failed.
    fn ensure_range(&mut self, cx: &mut Context<Self>) {
        if self.inner != InnerTab::Files {
            return;
        }
        let Some(header) = self.header.value() else {
            return;
        };
        let Some(revisions) = header.revisions.clone() else {
            // A diff already shown stays: a header that transiently reports
            // no revisions is not a reason to take it away. A fetch still
            // running has nothing to show for it, so it is cancelled.
            if matches!(self.range, RangeState::Fetching) {
                self.range_task = None;
            }
            if !matches!(self.range, RangeState::Ready { .. }) {
                self.range = RangeState::NoRevisions;
            }
            cx.notify();
            return;
        };
        let target_branch = header.summary.target_branch.clone();
        match &self.range {
            RangeState::Fetching => return,
            RangeState::Ready {
                revisions: current,
                ..
            } if *current == revisions => return,
            RangeState::Failed {
                revisions: Some(failed),
                ..
            } if *failed == revisions => return,
            _ => {}
        }
        let (carried, was_ready) = match &self.range {
            RangeState::Ready { changes, .. } => (changes.read(cx).expanded_paths(), true),
            _ => (Vec::new(), false),
        };
        self.start_range_fetch(revisions, target_branch, carried, was_ready, cx);
    }

    /// Makes `revisions` local on the background executor and hands the
    /// answer to `range_ready`; `carried` are the files to reopen in the
    /// rebuilt diff.
    fn start_range_fetch(
        &mut self,
        revisions: Revisions,
        target_branch: String,
        carried: Vec<PathBuf>,
        was_ready: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = forge_source::source(cx) else {
            self.range = RangeState::Failed {
                error: RevisionError::Git {
                    detail: "Change requests are unavailable in this build.".to_string(),
                },
                revisions: Some(revisions),
            };
            cx.notify();
            return;
        };
        self.range = RangeState::Fetching;
        let reference = self.reference.clone();
        let worktree = self.worktree.clone();
        let wanted = revisions.clone();
        self.range_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    source.ensure_revisions(&worktree, &reference, &wanted, Some(&target_branch))
                })
                .await;
            let _ = this.update(cx, |tab, cx| {
                tab.range_ready(revisions, carried, was_ready, result, cx)
            });
        }));
        cx.notify();
    }

    fn range_ready(
        &mut self,
        revisions: Revisions,
        carried: Vec<PathBuf>,
        was_ready: bool,
        result: Result<(), RevisionError>,
        cx: &mut Context<Self>,
    ) {
        // The header may have moved while this fetch ran (a new push landed
        // in a refresh): what completed is then not what Files should show.
        // The completion is dropped and the header's own revisions fetched,
        // with the opened files carried across as they would be otherwise.
        let wanted = self.header.value().and_then(|header| header.revisions.clone());
        if wanted.as_ref() != Some(&revisions) {
            self.range = RangeState::Idle;
            match (wanted, self.header.value().map(|header| header.summary.target_branch.clone())) {
                (Some(current), Some(target_branch)) if self.inner == InnerTab::Files => {
                    self.start_range_fetch(current, target_branch, carried, was_ready, cx);
                }
                _ => self.ensure_range(cx),
            }
            cx.notify();
            return;
        }
        match result {
            Ok(()) => {
                let (worktree, base, head) = (
                    self.worktree.clone(),
                    revisions.base_sha.clone(),
                    revisions.head_sha.clone(),
                );
                let changes = cx.new(|cx| ChangesTab::for_range(worktree, base, head, cx));
                if let Some(header) = self.header.value() {
                    let files = ForgeFiles {
                        forge: header.summary.reference.forge,
                        web_url: header.summary.web_url.clone(),
                    };
                    changes.update(cx, |changes, _| changes.set_forge_files(files));
                }
                changes.update(cx, |changes, cx| {
                    for path in &carried {
                        changes.focus_path(path, cx);
                    }
                });
                self.range_subscription = Some(cx.subscribe(
                    &changes,
                    |tab, _changes, event: &ChangesTabEvent, cx| match event {
                        ChangesTabEvent::OpenFile(absolute) => {
                            let relative = absolute
                                .strip_prefix(&tab.worktree)
                                .unwrap_or(absolute)
                                .to_path_buf();
                            let _ = tab.open_file(relative, None, cx);
                        }
                        ChangesTabEvent::CommentOn(anchor) => tab.open_composer(anchor.clone(), cx),
                        ChangesTabEvent::CommentRefused(reason) => {
                            let _ = tab.record_refusal("line-comment", Err(reason.clone()), cx);
                        }
                    },
                ));
                self.updated_notice = was_ready
                    .then(|| format!("Updated to head {}", short(&revisions.head_sha)));
                self.range = RangeState::Ready {
                    revisions,
                    changes: changes.clone(),
                };
                self.push_annotations(cx);
                let commentable = self.commentable(cx);
                changes.update(cx, |changes, cx| changes.set_commentable(commentable, cx));
                // A new head moves the lines threads sit on; the CI timer that
                // noticed it reads no threads, so they are read for it here.
                if was_ready {
                    self.load_threads(cx);
                }
                if let Some((path, side, line, key)) = self.pending_reveal.take() {
                    changes.update(cx, |changes, cx| changes.focus_anchor(&path, side, line, key, cx));
                }
            }
            Err(error) => {
                self.range = RangeState::Failed {
                    error,
                    revisions: Some(revisions),
                };
            }
        }
        cx.notify();
    }

    /// The failed diff's Retry: asks for the same revisions again, loading
    /// the header first when there is none.
    pub(crate) fn retry_range(&mut self, cx: &mut Context<Self>) {
        self.range = RangeState::Idle;
        if self.header.value().is_none() {
            self.refresh(cx);
        } else {
            self.ensure_range(cx);
        }
    }

    /// Shows `path` in Files and, given a line, opens the diff there. Before
    /// the diff exists the request waits for it.
    pub fn reveal(&mut self, path: PathBuf, line: Option<u32>, cx: &mut Context<Self>) {
        self.select_inner(InnerTab::Files, cx);
        match &self.range {
            RangeState::Ready { changes, .. } => {
                let changes = changes.clone();
                changes.update(cx, |changes, cx| match line {
                    Some(line) => changes.focus_line(&path, line as usize, cx),
                    None => changes.focus_path(&path, cx),
                });
            }
            _ => self.pending_reveal = Some((path, AnnotationSide::New, line, None)),
        }
    }

    /// *Open in editor* for a file of the diff. `Err` when there is no diff.
    pub fn open_file(
        &mut self,
        path: PathBuf,
        line: Option<u32>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let RangeState::Ready { revisions, changes } = &self.range else {
            return Err("the diff is not ready".to_string());
        };
        let deleted = changes.read(cx).is_deleted(&path);
        cx.emit(ChangeRequestTabEvent::OpenFile {
            path,
            line,
            revisions: revisions.clone(),
            deleted,
        });
        Ok(())
    }

    /// A commit of the change request: its revisions are made local first, and
    /// then the host opens it (spec §5.4). A failure is shown above the list
    /// with the forge as the remedy; the tab never opens the forge by itself.
    pub fn open_commit(&mut self, sha: String, web_url: String, cx: &mut Context<Self>) {
        self.commit_error = None;
        let revisions = self.header.value().and_then(|header| {
            header
                .revisions
                .clone()
                .map(|revisions| (revisions, header.summary.target_branch.clone()))
        });
        let (Some((revisions, target_branch)), Some(source)) =
            (revisions, forge_source::source(cx))
        else {
            // Nothing to fetch by: the host still opens it if it is local.
            cx.emit(ChangeRequestTabEvent::OpenCommit { sha, web_url });
            return;
        };
        let (reference, worktree) = (self.reference.clone(), self.worktree.clone());
        self.commit_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    source.ensure_revisions(&worktree, &reference, &revisions, Some(&target_branch))
                })
                .await;
            let _ = this.update(cx, |tab, cx| match result {
                Ok(()) => cx.emit(ChangeRequestTabEvent::OpenCommit { sha, web_url }),
                Err(error) => {
                    tab.commit_error = Some((error, web_url));
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    /// `open_commit` by sha alone, for the control socket: the URL comes from
    /// the loaded commit list.
    pub fn open_commit_by_sha(&mut self, sha: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let url = self
            .commits
            .value()
            .and_then(|listing| listing.items.iter().find(|commit| commit.sha == sha))
            .map(|commit| commit.web_url.clone())
            .ok_or_else(|| "that commit is not in the loaded list".to_string())?;
        self.open_commit(sha.to_string(), url, cx);
        Ok(())
    }

    /// The strip's glyph colour, once the state is known.
    pub fn state_color(&self, theme: &Theme) -> Option<Hsla> {
        self.header
            .value()
            .map(|header| style::state_color(header.summary.state, theme))
    }

    /// What the control socket reports about this tab (Task 9).
    pub fn report(&self, cx: &App) -> Vec<(String, String)> {
        let (state, rows) = if self.unreachable.is_some() {
            ("unreachable", 0)
        } else {
            let rows = |slot_rows: Option<usize>| slot_rows.unwrap_or(0);
            match self.inner {
                InnerTab::Conversation => (
                    slot_state(&self.header),
                    rows(self.header.value().map(|header| self.threads.value().map_or(
                        header.timeline.len(), |threads| merge_threads(&header.timeline, &threads.items).len(),
                    ))),
                ),
                InnerTab::Commits => (
                    slot_state(&self.commits),
                    rows(self.commits.value().map(|l| l.items.len())),
                ),
                InnerTab::Checks => (
                    slot_state(&self.checks),
                    rows(self.checks.value().map(|l| l.items.len())),
                ),
                InnerTab::Files => match &self.range {
                    RangeState::Ready { changes, .. } => (
                        "loaded",
                        changes
                            .read(cx)
                            .report()
                            .sections
                            .iter()
                            .map(|section| section.files.len())
                            .sum(),
                    ),
                    RangeState::Fetching => ("loading", 0),
                    RangeState::Failed { .. } => ("error", 0),
                    _ => (
                        slot_state(&self.files),
                        rows(self.files.value().map(|l| l.items.len())),
                    ),
                },
            }
        };
        let (files_mode, head) = match &self.range {
            RangeState::Idle => ("idle", String::new()),
            RangeState::Fetching => ("fetching", String::new()),
            RangeState::Ready { revisions, .. } => ("diff", short(&revisions.head_sha).to_string()),
            RangeState::Failed { .. } => ("error", String::new()),
            RangeState::NoRevisions => ("no-revisions", String::new()),
        };
        let (diff_files, diff_summary, files_focus) = match &self.range {
            RangeState::Ready { changes, .. } => {
                let changes = changes.read(cx);
                let mut summary: Vec<String> = changes
                    .report()
                    .sections
                    .iter()
                    .flat_map(|section| section.files.iter())
                    .map(|file| {
                        format!(
                            "{} +{} -{}",
                            file.path.display(),
                            file.additions,
                            file.deletions
                        )
                    })
                    .collect();
                summary.sort();
                (
                    summary.len().to_string(),
                    summary.join("|"),
                    changes
                        .focused_path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default(),
                )
            }
            _ => ("0".to_string(), String::new(), String::new()),
        };
        let files_error = match &self.range {
            RangeState::Failed { error, .. } => error.to_string(),
            _ => String::new(),
        };
        let mut group_words = Vec::new();
        let mut row_words = Vec::new();
        if let Some(listing) = self.checks.value() {
            let can_rerun = self.header.value().is_some_and(|header| header.capabilities.can_rerun_checks);
            let (pipeline, groups) = check_groups(&listing.items, self.reference.forge, can_rerun);
            for group in pipeline.iter().chain(groups.iter()) {
                let open = self.check_folds.get(&group.key).copied().unwrap_or(group.open_by_default);
                let mut word = format!("{}:{}", group.title, if open { "open" } else { "folded" });
                if let Some(run) = group.rerun_failed {
                    word.push_str(&format!(":rerun-failed={run}"));
                }
                group_words.push(word);
            }
            for index in groups.iter().flat_map(|group| group.members.iter()) {
                let check = &listing.items[*index];
                let status = match check.status {
                    CheckStatus::Failed => "failed",
                    CheckStatus::Running => "running",
                    CheckStatus::Queued => "queued",
                    CheckStatus::Passed => "passed",
                    CheckStatus::Canceled => "canceled",
                    CheckStatus::Skipped => "skipped",
                    CheckStatus::Neutral => "neutral",
                };
                let mut word = format!("{}:{status}", check.name);
                if let Some(job) = &check.job {
                    word.push_str(&format!(":job={}", job.job_id));
                }
                if can_rerun && rerunnable(check) {
                    word.push_str(":rerun");
                }
                row_words.push(word);
            }
        }
        let thread_replying = self.threads.value().and_then(|listing| {
            listing.items.iter().find_map(|thread| {
                let view = self.thread_views.get(&threads::thread_key(&thread.id))?;
                view.read(cx).reply.is_some().then(|| thread.id.clone())
            })
        }).unwrap_or_default();
        let thread_comment_editing = self.threads.value().and_then(|listing| {
            listing.items.iter().find_map(|thread| {
                self.thread_views.get(&threads::thread_key(&thread.id))?
                    .read(cx)
                    .editing_comment_id()
            })
        }).unwrap_or_default();
        let mut report = vec![
            ("label".to_string(), self.reference.label()),
            ("title".to_string(), self.title.clone()),
            ("inner".to_string(), self.inner.as_str().to_string()),
            ("state".to_string(), state.to_string()),
            ("rows".to_string(), rows.to_string()),
            ("check_groups".to_string(), if group_words.is_empty() { "-".to_string() } else { group_words.join("|") }),
            ("check_rows".to_string(), if row_words.is_empty() { "-".to_string() } else { row_words.join("|") }),
            ("files_mode".to_string(), files_mode.to_string()),
            ("head".to_string(), head),
            ("diff_files".to_string(), diff_files),
            ("diff_summary".to_string(), diff_summary),
            ("files_focus".to_string(), files_focus),
            ("files_error".to_string(), files_error),
            (
                "commit_error".to_string(),
                self.commit_error
                    .as_ref()
                    .map(|(error, _)| error.to_string())
                    .unwrap_or_default(),
            ),
            (
                "notice".to_string(),
                self.updated_notice.clone().unwrap_or_default(),
            ),
            (
                "list_rows".to_string(),
                self.files
                    .value()
                    .map_or(0, |list| list.items.len())
                    .to_string(),
            ),
            (
                "cr_state".to_string(),
                self.header
                    .value()
                    .map(|header| style::state_label(header.summary.state).to_lowercase())
                    .unwrap_or_default(),
            ),
            ("caps".to_string(), self.caps_words()),
            (
                "line_composer".to_string(),
                self.line_composer
                    .as_ref()
                    .map(|open| open.anchor.spec())
                    .unwrap_or_default(),
            ),
            ("thread_replying".to_string(), thread_replying),
            ("thread_comment_editing".to_string(), thread_comment_editing),
            (
                "line_composer_len".to_string(),
                self.line_composer
                    .as_ref()
                    .map_or(0, |open| open.view.read(cx).text(cx).chars().count())
                    .to_string(),
            ),
            (
                "line_composer_error".to_string(),
                self.write_refusal
                    .as_ref()
                    .filter(|(target, _)| target == &compose::WriteTarget::Composer)
                    .map(|(_, message)| message.clone())
                    .or_else(|| match (&self.actions.state, &self.write_target) {
                        (
                            actions::ActionState::Failed { message, .. },
                            Some(compose::WriteTarget::Composer),
                        ) => Some(message.clone()),
                        _ => None,
                    })
                    .unwrap_or_default(),
            ),
            (
                "commentable".to_string(),
                if self.commentable(cx) { "yes" } else { "no" }.to_string(),
            ),
            (
                "composer_len".to_string(),
                self.actions
                    .composer
                    .as_ref()
                    .map_or(0, |input| input.read(cx).text().len())
                    .to_string(),
            ),
            (
                "editing".to_string(),
                if self.actions.edit.is_some() { "yes" } else { "no" }.to_string(),
            ),
            (
                "comment_editing".to_string(),
                self.actions
                    .comment_edit
                    .as_ref()
                    .map(|edit| edit.comment.id.clone())
                    .unwrap_or_default(),
            ),
            ("action".to_string(), self.actions.state.word().to_string()),
            ("action_kind".to_string(), self.actions.state.kind().to_string()),
            ("action_message".to_string(), self.action_message()),
            ("merge_strip".to_string(), self.strip().word().to_string()),
            ("merge_message".to_string(), self.strip().text()),
            ("merge_verdict".to_string(), self.merge_verdict_word()),
            (
                "merge_method".to_string(),
                self.merge_method().map_or("", |method| method.word()).to_string(),
            ),
            (
                "picker".to_string(),
                self.actions
                    .picker
                    .as_ref()
                    .map_or("closed", |picker| picker.kind.word())
                    .to_string(),
            ),
            ("picker_candidates".to_string(), self.picker_candidates().to_string()),
            (
                "picker_chosen".to_string(),
                self.actions
                    .picker
                    .as_ref()
                    .map(|picker| picker.chosen.iter().cloned().collect::<Vec<_>>().join(","))
                    .unwrap_or_default(),
            ),
            (
                "merge_dialog".to_string(),
                if self.actions.merge.dialog.is_some() { "open" } else { "closed" }.to_string(),
            ),
            (
                "merge_sending".to_string(),
                if self.merge_sending(&["merge", "auto-merge", "cancel-auto-merge"]) { "yes" } else { "no" }.to_string(),
            ),
            (
                "merge_dialog_message".to_string(),
                self.merge_dialog_status().map(|(_, text)| text).unwrap_or_default(),
            ),
        ];
        report.extend(self.thread_report(cx));
        report
    }
}

/// How long the tab waits before looking again at what it shows: shortly
/// while the forge is still checking mergeability, a minute while CI runs.
fn refresh_wait(header: &ChangeHeader) -> Option<Duration> {
    if merge::strip_kind(header.summary.state, &header.capabilities.merge) == merge::StripKind::Checking {
        Some(MERGE_CHECK_REFRESH)
    } else if matches!(header.summary.ci, CiState::Running(_)) {
        Some(CI_REFRESH)
    } else {
        None
    }
}

fn slot_state<T>(slot: &Slot<T>) -> &'static str {
    match slot {
        Slot::Idle | Slot::Loading => "loading",
        Slot::Loaded { .. } => "loaded",
        Slot::Failed(_) => "error",
    }
}

pub(crate) fn unreachable_text(connection: &Connection) -> String {
    match connection {
        Connection::NotConnected { forge, host } => format!(
            "Not signed in to {host}. Sign in with `{} auth login --hostname {host}`, or add a token in Settings → Git Hosting.",
            style::cli_name(*forge)
        ),
        Connection::UnknownForge { host } => {
            format!(
                "Sirio does not know which forge {host} runs; say which in the right panel's change request view."
            )
        }
        Connection::NoForgeRemote | Connection::Ready(_) => {
            "This change request's project cannot be reached.".to_string()
        }
    }
}

fn body_of(item: &TimelineItem) -> Option<&str> {
    match item {
        TimelineItem::Comment { body, .. } | TimelineItem::Review { body, .. } => Some(body),
        TimelineItem::LineComment(comment) => Some(&comment.body),
        TimelineItem::Event { .. } => None,
    }
}

/// A forge body through the Preview's Markdown path, `expand_html` subset
/// included, with its images turned into text (see [`images_as_text`]).
fn markdown_doc(body: &str, theme: &Theme) -> markdown::Doc {
    let preview = crate::markdown_preview::build(
        sirio_markdown::parse(body),
        Path::new(""),
        &crate::markdown_preview::Diagrams::default(),
        &crate::markdown_preview::palette(theme),
    );
    let mut doc = preview.doc;
    images_as_text(&mut doc);
    doc
}

/// Sirio installs no HTTP client for GPUI, so a remote picture draws as an
/// empty box (checked 2026-09-27), and a forge body's relative picture has
/// no file behind it here. Each picture becomes its alt text (or "image"),
/// a link when it is remote.
pub(crate) fn images_as_text(doc: &mut markdown::Doc) {
    for block in &mut doc.blocks {
        let markdown::BlockKind::Image { url, alt, .. } = &block.kind else {
            continue;
        };
        let label = if alt.text.trim().is_empty() {
            "image".to_string()
        } else {
            alt.text.clone()
        };
        let mut text = markdown::Text::plain(label.clone());
        if url.contains("://") {
            text.marks.push(markdown::MarkSpan {
                range: 0..label.len(),
                mark: markdown::Mark::Link(url.clone()),
            });
        }
        block.kind = markdown::BlockKind::Paragraph(text);
    }
}

fn open_links() -> LinkClickOverride {
    Rc::new(|url, _window, cx| cx.open_url(url))
}

fn error_panel(
    message: String,
    theme: &Theme,
    retry: Rc<dyn Fn(&mut App)>,
    close: Option<Rc<dyn Fn(&mut App)>>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(theme.spacing.card_gap)
        .p(theme.spacing.card_gap)
        .child(ely_ui::message(Severity::Danger, message, theme))
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(ely_ui::text_button(
                    "change-request-retry",
                    "Retry",
                    Some(IconName::RefreshCw),
                    ButtonState::IDLE,
                    move |_, cx| retry(cx),
                ))
                .when_some(close, |this, close| {
                    this.child(ely_ui::text_button(
                        "change-request-close",
                        "Close",
                        Some(IconName::X),
                        ButtonState::IDLE,
                        move |_, cx| close(cx),
                    ))
                }),
        )
        .into_any_element()
}

fn stale_line(message: String, theme: &Theme, retry: Rc<dyn Fn(&mut App)>) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(ely_ui::message(
                    Severity::Warning,
                    format!("Refresh failed · {message}"),
                    theme,
                )),
        )
        .child(ely_ui::text_button(
            "change-request-stale-retry",
            "Retry",
            Some(IconName::RefreshCw),
            ButtonState::IDLE,
            move |_, cx| retry(cx),
        ))
        .into_any_element()
}

fn slot_view<T>(
    slot: &Slot<T>,
    what: &'static str,
    theme: &Theme,
    retry: Rc<dyn Fn(&mut App)>,
    close: Option<Rc<dyn Fn(&mut App)>>,
    loaded: impl FnOnce(&T) -> AnyElement,
) -> AnyElement {
    match slot {
        Slot::Idle | Slot::Loading => div()
            .text_color(theme.ely.fg_subtle)
            .child(format!("Loading {what}…"))
            .into_any_element(),
        Slot::Failed(error) => error_panel(error.to_string(), theme, retry, close),
        Slot::Loaded { value, stale } => div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .when_some(stale.as_ref(), |this, error| {
                this.child(stale_line(error.to_string(), theme, retry.clone()))
            })
            .child(loaded(value))
            .into_any_element(),
    }
}

/// A `path:line` that takes the reader to that line of the diff.
fn line_link(
    id: (&'static str, usize),
    text: String,
    path: String,
    line: Option<u32>,
    theme: &Theme,
    entity: Entity<ChangeRequestTab>,
) -> AnyElement {
    div()
        .id(id)
        .cursor_pointer()
        .text_color(theme.sirio.quantity)
        .child(text)
        .on_click(move |_, _, cx| {
            let path = PathBuf::from(&path);
            entity.update(cx, |tab, cx| tab.reveal(path, line, cx));
        })
        .into_any_element()
}

impl EventEmitter<ChangeRequestTabEvent> for ChangeRequestTab {}

impl ChangeRequestTab {
    fn retry_handle(entity: &Entity<Self>) -> Rc<dyn Fn(&mut App)> {
        let entity = entity.clone();
        Rc::new(move |cx| entity.update(cx, |tab, cx| tab.refresh(cx)))
    }

    fn close_handle(entity: &Entity<Self>) -> Rc<dyn Fn(&mut App)> {
        let entity = entity.clone();
        Rc::new(move |cx| entity.update(cx, |_, cx| cx.emit(ChangeRequestTabEvent::Close)))
    }

    fn render_header(&self, theme: &Theme, entity: &Entity<Self>) -> impl IntoElement {
        let summary = self.header.value().map(|header| &header.summary);
        let web_url = summary.map(|summary| summary.web_url.clone());
        let refresh = entity.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(16.0))
            .pt(px(12.0))
            .pb(px(10.0))
            .border_b_1()
            .border_color(theme.ely.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_baseline()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .debug_selector(|| "change-request-title".into())
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_size(theme.typography.title3)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(selectable_text(self.title.clone())),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_color(theme.ely.fg_subtle)
                                    .child(selectable_text(self.reference.label())),
                            ),
                    )
                    .when_some(web_url, |this, url| {
                        this.child(ely_ui::icon_button(
                            "change-request-open-browser",
                            IconName::ExternalLink,
                            "Open on the forge",
                            true,
                            move |_, cx| cx.open_url(&url),
                        ))
                    })
                    .child(ely_ui::icon_button(
                        "change-request-refresh",
                        IconName::RefreshCw,
                        "Refresh",
                        true,
                        move |_, cx| refresh.update(cx, |tab, cx| tab.refresh(cx)),
                    )),
            )
            .when_some(self.render_action_bar(theme, entity), |this, bar| this.child(bar))
            .when_some(summary, |this, summary| {
                let meta = format!(
                    "{} · {} → {} · updated {}",
                    summary.author,
                    summary.source_branch,
                    summary.target_branch,
                    style::age(style::now(), summary.updated_at)
                );
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(theme.typography.footnote)
                        .text_color(theme.ely.fg_muted)
                        .child(ely_ui::state_badge(summary.state))
                        .child(
                            div()
                                .debug_selector(|| "change-request-meta".into())
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(selectable_text(meta)),
                        ),
                )
            })
            .when_some(self.render_people_row(theme, entity), |this, row| this.child(row))
            .when_some(self.render_action_status(theme), |this, status| this.child(status))
    }

    fn inner_count(&self, inner: InnerTab) -> Option<String> {
        let header = self.header.value()?;
        match inner {
            InnerTab::Conversation => Some(header.summary.comments.to_string()),
            InnerTab::Commits => header.commit_count.map(|count| count.to_string()),
            InnerTab::Checks => match header.summary.ci {
                CiState::Running(Some(progress)) => {
                    Some(format!("{}/{}", progress.done, progress.total))
                }
                _ => self
                    .checks
                    .value()
                    .map(|listing| listing.items.len().to_string()),
            },
            InnerTab::Files => header.changed_files.map(|count| count.to_string()),
        }
    }

    fn render_inner_strip(&self, theme: &Theme, entity: &Entity<Self>) -> impl IntoElement {
        let ci = self.header.value().map(|header| header.summary.ci);
        // `Tabs` panics when `selected` names no tab: both come from InnerTab::ALL.
        let choices: Vec<Choice> = InnerTab::ALL
            .into_iter()
            .map(|inner| {
                // The pane is narrow: only Checks carries an icon, because it is
                // the one that carries state (the CI result).
                let choice = Choice::new(inner.as_str(), inner.title());
                let choice = match (inner, ci.and_then(|ci| style::ci_icon(ci, theme))) {
                    (InnerTab::Checks, Some((icon, _))) => choice.icon(icon),
                    _ => choice,
                };
                match self.inner_count(inner) {
                    Some(count) => choice.note(count),
                    None => choice,
                }
            })
            .collect();
        let entity = entity.clone();
        div()
            .id("change-request-inner-tabs")
            .debug_selector(|| "change-request-inner-tabs".to_owned())
            .px(px(10.0))
            // A narrow pane scrolls the strip instead of clipping its last tab.
            .overflow_x_scroll()
            .child(
                Tabs::new("change-request-tabs", choices, self.inner.as_str()).on_change(
                    move |value, _, cx| {
                        // An unknown value opens the conversation, never panics.
                        let inner = InnerTab::parse(value);
                        entity.update(cx, |tab, cx| tab.select_inner(inner, cx));
                    },
                ),
            )
    }

    fn render_conversation(&self, theme: &Theme, entity: &Entity<Self>) -> AnyElement {
        slot_view(
            &self.header,
            "the conversation",
            theme,
            Self::retry_handle(entity),
            Some(Self::close_handle(entity)),
            |header| {
                let mut column = div().flex().flex_col().gap(px(12.0));
                if let Some(card) = self.render_edit_card(theme, entity) {
                    column = column.child(card);
                }
                if header.timeline_truncated {
                    let url = header.summary.web_url.clone();
                    column = column.child(ely_ui::text_button(
                        "change-request-earlier",
                        "Earlier activity is on the forge",
                        Some(IconName::ArrowUpRight),
                        ButtonState::IDLE,
                        move |_, cx| cx.open_url(&url),
                    ));
                }
                if let Some(doc) = self
                    .description
                    .clone()
                    .filter(|doc| !doc.blocks.is_empty())
                {
                    column = column.child(
                        div()
                            .debug_selector(|| "change-request-description".into())
                            .p(px(12.0))
                            .rounded(theme.radii.control)
                            .border_1()
                            .border_color(theme.ely.border)
                            .bg(theme.ely.bg)
                            .child(Chat::render_markdown_document_with_link_override(
                                doc,
                                theme,
                                open_links(),
                            )),
                    );
                }
                let mut rail = Timeline::new();
                if let Some(threads) = self.threads.value() {
                    for entry in merge_threads(&header.timeline, &threads.items) {
                        rail = rail.item(match entry {
                            ConversationEntry::Item(index) => self.render_timeline_item(index, &header.timeline[index], theme, entity),
                            ConversationEntry::Thread(index) => self.render_thread_entry(index, &threads.items[index], theme, entity),
                        });
                    }
                } else {
                    for (index, item) in header.timeline.iter().enumerate() {
                        rail = rail.item(self.render_timeline_item(index, item, theme, entity));
                    }
                }
                column = column.child(rail);
                if let Some(composer) = self.render_composer(theme, entity) {
                    column = column.child(composer);
                }
                column.into_any_element()
            },
        )
    }

    fn render_timeline_item(
        &self,
        index: usize,
        item: &TimelineItem,
        theme: &Theme,
        entity: &Entity<Self>,
    ) -> RailItem {
        let now = style::now();
        let head = |who: String, what: String| {
            div()
                .id(("change-request-timeline-item", index))
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(selectable_text(who)),
                )
                .child(
                    div()
                        .text_color(theme.ely.fg_muted)
                        .child(selectable_text(what)),
                )
        };
        // Each piece of an entry is its own element outside `head`'s id scope, so
        // each carries the entry's index: two with the same id panic gpui's a11y
        // tree in a debug build.
        let when = |at: Option<i64>| {
            div()
                .id(("change-request-timeline-time", index))
                .text_color(theme.ely.fg_subtle)
                .child(selectable_text(style::age(now, at)))
        };
        let body = self.bodies.get(index).cloned().flatten().map(|doc| {
            div().id(("change-request-timeline-body", index)).child(Chat::render_markdown_document_with_link_override(
                doc,
                theme,
                open_links(),
            ))
        });
        let own = match item {
            TimelineItem::Comment { edit, .. } | TimelineItem::Review { edit, .. } => edit.as_ref(),
            _ => None,
        };
        // Where the viewer is editing this entry, its field takes the place of
        // its text.
        let editor = self.editor_for(own, theme, entity);
        let pencil = self.edit_pencil(index, own, theme, entity);
        let body: Option<AnyElement> = editor.or_else(|| body.map(IntoElement::into_any_element));
        match item {
            TimelineItem::Comment { author, at, .. } => RailItem::new(
                head(author.clone(), "commented".to_string()).children(pencil),
            )
            .time(when(*at))
            .icon(IconName::MessageSquare)
            .children(body),
            TimelineItem::Review {
                author,
                outcome,
                at,
                line_comments,
                ..
            } => {
                let (verb, icon, tone) = match outcome {
                    ReviewOutcome::Approved => ("approved", IconName::CircleCheck, Tone::Success),
                    ReviewOutcome::ChangesRequested => {
                        ("requested changes", IconName::CircleAlert, Tone::Warning)
                    }
                    ReviewOutcome::Commented | ReviewOutcome::Other => {
                        ("reviewed", IconName::MessageSquare, Tone::Neutral)
                    }
                    ReviewOutcome::Dismissed => {
                        ("had a review dismissed", IconName::Ban, Tone::Neutral)
                    }
                    ReviewOutcome::Requested => {
                        ("was asked to review", IconName::Eye, Tone::Neutral)
                    }
                };
                RailItem::new(head(author.clone(), verb.to_string()).children(pencil))
                    .time(when(*at))
                    .icon(icon)
                    .tone(tone)
                    .children(body)
                    .children(line_comments.iter().filter(|_| self.threads.value().is_none()).enumerate().map(|(position, comment)| {
                        div()
                            .id(("change-request-line-comment-row", index * 1000 + position))
                            .flex()
                            .gap(px(6.0))
                            .text_size(theme.typography.footnote)
                            .text_color(theme.ely.fg_muted)
                            .child(line_link(
                                ("change-request-line-comment", index * 1000 + position),
                                format!(
                                    "on {}:{}",
                                    comment.path,
                                    comment
                                        .line
                                        .map_or("?".to_string(), |line| line.to_string())
                                ),
                                comment.path.clone(),
                                comment.line,
                                theme,
                                entity.clone(),
                            ))
                            .child(selectable_text(format!("— {}", comment.body)))
                    }))
            }
            TimelineItem::LineComment(comment) => {
                RailItem::new(head(comment.author.clone(), "commented".to_string()))
                    .time(when(comment.at))
                    .icon(IconName::MessageSquareDiff)
                    .child(
                        div().id(("change-request-line-comment-row", index * 1000)).text_size(theme.typography.footnote).child(line_link(
                            ("change-request-line-comment", index * 1000),
                            format!(
                                "on {}:{}",
                                comment.path,
                                comment
                                    .line
                                    .map_or("?".to_string(), |line| line.to_string())
                            ),
                            comment.path.clone(),
                            comment.line,
                            theme,
                            entity.clone(),
                        )),
                    )
                    .children(body)
            }
            TimelineItem::Event { actor, kind, at } => {
                let (what, tone) = match kind {
                    EventKind::CommitsPushed { count } => (
                        format!("added {count} commit{}", if *count == 1 { "" } else { "s" }),
                        Tone::Neutral,
                    ),
                    EventKind::ReviewRequested { reviewer } => {
                        (format!("asked {reviewer} to review"), Tone::Neutral)
                    }
                    EventKind::Merged => ("merged".to_string(), Tone::Accent),
                    EventKind::Closed => ("closed".to_string(), Tone::Danger),
                    EventKind::Reopened => ("reopened".to_string(), Tone::Success),
                    EventKind::ReadyForReview => {
                        ("marked it ready for review".to_string(), Tone::Success)
                    }
                    EventKind::ConvertedToDraft => {
                        ("marked it as a draft".to_string(), Tone::Neutral)
                    }
                    EventKind::Other(text) => (text.clone(), Tone::Neutral),
                };
                RailItem::new(head(actor.clone().unwrap_or_default(), what))
                    .time(when(*at))
                    .icon(IconName::Dot)
                    .tone(tone)
            }
        }
    }

    fn render_commits(
        &self,
        listing: &Listing<CommitSummary>,
        theme: &Theme,
        entity: &Entity<Self>,
    ) -> AnyElement {
        let now = style::now();
        let hover = theme.ely.hover;
        div()
            .flex()
            .flex_col()
            .when_some(self.commit_error.clone(), |this, (error, web_url)| {
                let hint = error_hint(&error);
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(6.0))
                        .pb(px(6.0))
                        .text_size(theme.typography.footnote)
                        .text_color(theme.ely.danger)
                        .child(div().flex_1().child(error.to_string()))
                        .child(ely_ui::text_button(
                            "change-request-commit-forge",
                            "Open on the forge",
                            Some(IconName::ArrowUpRight),
                            ButtonState::IDLE,
                            move |_, cx| cx.open_url(&web_url),
                        )),
                )
                .when_some(hint, |this, hint| {
                    this.child(
                        div()
                            .px(px(6.0))
                            .pb(px(6.0))
                            .text_size(theme.typography.footnote)
                            .text_color(theme.ely.fg_muted)
                            .child(hint),
                    )
                })
            })
            .children(listing.items.iter().enumerate().map(|(index, commit)| {
                let entity = entity.clone();
                let sha = commit.sha.clone();
                let url = commit.web_url.clone();
                div()
                    .id(("change-request-commit", index))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(6.0))
                    .py(px(5.0))
                    .rounded(theme.radii.control)
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |tab, cx| tab.open_commit(sha.clone(), url.clone(), cx))
                    })
                    .child(
                        EIcon::new(IconName::GitCommitHorizontal)
                            .size(EIconSize::Sm)
                            .color(theme.ely.fg_subtle),
                    )
                    .child(
                        div()
                            .flex_none()
                            .font_family(theme.typography.code_family)
                            .text_color(theme.ely.fg_muted)
                            .child(commit.short_sha.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(commit.title.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(theme.typography.footnote)
                            .text_color(theme.ely.fg_subtle)
                            .child(format!(
                                "{} · {}",
                                commit.author,
                                style::age(now, commit.at)
                            )),
                    )
            }))
            .when(listing.truncated, |this| {
                this.child(
                    div()
                        .text_color(theme.ely.fg_subtle)
                        .child("More commits are on the forge."),
                )
            })
            .into_any_element()
    }

    fn render_checks(
        &self,
        listing: &Listing<Check>,
        theme: &Theme,
        entity: &Entity<Self>,
    ) -> AnyElement {
        let can_rerun = self.header.value()
            .is_some_and(|header| header.capabilities.can_rerun_checks);
        let busy = self.action_busy();
        let (pipeline, groups) = check_groups(&listing.items, self.reference.forge, can_rerun);
        let mut content = div().flex().flex_col();
        if let Some(pipeline) = pipeline {
            let mut row = div()
                .id("change-request-pipeline")
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(6.0))
                .py(px(5.0))
                .child(
                    EIcon::new(IconName::Workflow)
                        .size(EIconSize::Sm)
                        .color(theme.ely.fg_subtle),
                )
                .child(pipeline.title)
                .child(div().flex_1());
            if let Some(run) = pipeline.rerun_failed {
                let entity = entity.clone();
                row = row.child(ely_ui::icon_button(
                    "change-request-rerun-pipeline",
                    IconName::RotateCcw,
                    "Re-run failed",
                    !busy,
                    move |_, cx| entity.update(cx, |tab, cx| {
                        let _ = tab.perform(Action::Rerun(RerunTarget::FailedInRun(run)), cx);
                    }),
                ));
            }
            content = content.child(row);
        }
        for (group_index, group) in groups.into_iter().enumerate() {
            let open = self.check_folds.get(&group.key).copied()
                .unwrap_or(group.open_by_default);
            let failed = group.members.iter()
                .filter(|index| listing.items[**index].status == CheckStatus::Failed)
                .count();
            let count = if failed > 0 {
                format!("{failed} failed · {}", group.members.len())
            } else {
                group.members.len().to_string()
            };
            let selector_key = group.key.clone();
            let toggle_key = group.key.clone();
            let toggle = entity.clone();
            let mut header = div()
                .id(("change-request-check-group", group_index))
                .debug_selector(move || format!("change-request-check-group-{selector_key}"))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .id(("change-request-check-disclosure", group_index))
                        .flex()
                        .flex_1()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(6.0))
                        .py(px(5.0))
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            toggle.update(cx, |tab, cx| {
                                let open = tab.check_folds.get(&toggle_key).copied().unwrap_or(open);
                                tab.check_folds.insert(toggle_key.clone(), !open);
                                cx.notify();
                            })
                        })
                        .child(
                            EIcon::new(if open { IconName::ChevronDown } else { IconName::ChevronRight })
                                .size(EIconSize::Sm),
                        )
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(group.title))
                        .child(div().text_color(theme.ely.fg_subtle).child(count)),
                );
            if let Some(run) = group.rerun_failed {
                let entity = entity.clone();
                header = header.child(
                    div().id(("change-request-rerun-group", group_index)).child(ely_ui::icon_button(
                        "change-request-rerun-group",
                        IconName::RotateCcw,
                        "Re-run failed",
                        !busy,
                        move |_, cx| entity.update(cx, |tab, cx| {
                            let _ = tab.perform(Action::Rerun(RerunTarget::FailedInRun(run)), cx);
                        }),
                    )),
                );
            }
            content = content.child(header);
            if !open {
                continue;
            }
            for index in group.members {
                let check = &listing.items[index];
                let (icon, tint) = style::check_icon(check.status, theme);
                let job_id = check.job.as_ref().map(|job| job.job_id);
                let url = check.url.clone();
                let log = entity.clone();
                let hover = theme.ely.hover;
                let mut row = div()
                    .id(("change-request-check", index))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id(("change-request-check-open", index))
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(6.0))
                            .py(px(5.0))
                            .rounded(theme.radii.control)
                            .when(job_id.is_some() || url.is_some(), |this| {
                                this.cursor_pointer().hover(move |style| style.bg(hover))
                            })
                            .on_click(move |_, _, cx| {
                                if let Some(job_id) = job_id {
                                    log.update(cx, |tab, cx| {
                                        let _ = tab.open_log_by_job(job_id, cx);
                                    });
                                } else if let Some(url) = &url {
                                    cx.open_url(url);
                                }
                            })
                            .child(EIcon::new(icon).size(EIconSize::Sm).color(tint))
                            .child(
                                div().flex_1().min_w_0().overflow_hidden()
                                    .text_ellipsis().whitespace_nowrap().child(check.name.clone()),
                            )
                            .when_some(check.duration_secs, |this, secs| {
                                this.child(div().flex_none().text_color(theme.ely.fg_subtle)
                                    .child(style::duration_text(secs)))
                            }),
                    );
                if let Some(job_id) = job_id {
                    if can_rerun && rerunnable(check) {
                        let entity = entity.clone();
                        row = row.child(
                            div().id(("change-request-rerun-job", index)).child(ely_ui::icon_button(
                                "change-request-rerun-job",
                                IconName::RotateCcw,
                                "Re-run",
                                !busy,
                                move |_, cx| entity.update(cx, |tab, cx| {
                                    let _ = tab.perform(Action::Rerun(RerunTarget::Job(job_id)), cx);
                                }),
                            )),
                        );
                    }
                    if let Some(url) = check.url.clone() {
                        row = row.child(
                            div().id(("change-request-check-browser", index)).child(ely_ui::icon_button(
                                "change-request-check-browser",
                                IconName::ExternalLink,
                                "Open in browser",
                                !busy,
                                move |_, cx| cx.open_url(&url),
                            )),
                        );
                    }
                }
                content = content.child(row);
            }
        }
        content
            .when(listing.truncated, |this| {
                this.child(div().text_color(theme.ely.fg_subtle).child("More checks are on the forge."))
            })
            .into_any_element()
    }

    fn render_files(&self, listing: &Listing<FileChange>, theme: &Theme) -> AnyElement {
        let files_url = self
            .header
            .value()
            .map(|header| match self.reference.forge {
                Forge::GitHub => format!("{}/files", header.summary.web_url),
                Forge::GitLab => format!("{}/diffs", header.summary.web_url),
            });
        let hover = theme.ely.hover;
        div()
            .flex()
            .flex_col()
            .children(listing.items.iter().enumerate().map(|(index, file)| {
                let (letter, tint): (&str, Hsla) = match file.kind {
                    Some(FileChangeKind::Added) => ("A", theme.ely.success),
                    Some(FileChangeKind::Deleted) => ("D", theme.ely.danger),
                    Some(FileChangeKind::Renamed) => ("R", theme.sirio.quantity),
                    Some(FileChangeKind::Copied) => ("C", theme.sirio.quantity),
                    Some(FileChangeKind::Modified) => ("M", theme.ely.warning),
                    None => ("·", theme.ely.fg_subtle),
                };
                let url = files_url.clone();
                div()
                    .id(("change-request-file", index))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(6.0))
                    .py(px(5.0))
                    .rounded(theme.radii.control)
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(move |_, _, cx| {
                        if let Some(url) = &url {
                            cx.open_url(url)
                        }
                    })
                    .child(
                        div()
                            .w(px(14.0))
                            .flex_none()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(tint)
                            .child(letter),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(file.path.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(theme.sirio.diff_add)
                            .child(format!("+{}", file.additions)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(theme.sirio.diff_del)
                            .child(format!("−{}", file.deletions)),
                    )
            }))
            .when(listing.truncated, |this| {
                this.child(
                    div()
                        .text_color(theme.ely.fg_subtle)
                        .child("More files are on the forge."),
                )
            })
            .into_any_element()
    }

    fn render_files_tab(&self, theme: &Theme, entity: &Entity<Self>) -> AnyElement {
        let list = |this: &Self| {
            slot_view(
                &this.files,
                "files",
                theme,
                Self::retry_handle(entity),
                None,
                |listing| this.render_files(listing, theme),
            )
        };
        let note = |text: String| {
            div()
                .text_size(theme.typography.footnote)
                .text_color(theme.ely.fg_muted)
                .child(text)
        };
        match &self.range {
            RangeState::Ready { revisions, changes } => div()
                .flex()
                .flex_col()
                .size_full()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(12.0))
                        .py(px(6.0))
                        .text_size(theme.typography.footnote)
                        .text_color(theme.ely.fg_muted)
                        .child(format!(
                            "base {} … head {}",
                            short(&revisions.base_sha),
                            short(&revisions.head_sha)
                        ))
                        .when_some(self.updated_notice.clone(), |this, notice| {
                            this.child(div().text_color(theme.ely.fg_subtle).child(notice))
                        }),
                )
                .children(self.render_threads_notice(theme, entity))
                .child(div().flex_1().min_h(px(0.0)).child(changes.clone()))
                .into_any_element(),
            RangeState::Fetching => div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(note(format!(
                    "Fetching {}'s commits for the diff…",
                    self.reference.label()
                )))
                .child(list(self))
                .into_any_element(),
            RangeState::Failed { error, .. } => {
                let retry = entity.clone();
                let forge_url = self
                    .header
                    .value()
                    .map(|header| match self.reference.forge {
                        Forge::GitHub => format!("{}/files", header.summary.web_url),
                        Forge::GitLab => format!("{}/diffs", header.summary.web_url),
                    });
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_color(theme.ely.danger)
                            .child(div().flex_1().child(error.to_string()))
                            .child(ely_ui::text_button(
                                "change-request-range-retry",
                                "Retry",
                                Some(IconName::RefreshCw),
                                ButtonState::IDLE,
                                move |_, cx| retry.update(cx, |tab, cx| tab.retry_range(cx)),
                            ))
                            .when_some(forge_url, |this, url| {
                                this.child(ely_ui::text_button(
                                    "change-request-range-forge",
                                    "Open on the forge",
                                    Some(IconName::ArrowUpRight),
                                    ButtonState::IDLE,
                                    move |_, cx| cx.open_url(&url),
                                ))
                            }),
                    )
                    .when_some(error_hint(error), |this, hint| this.child(note(hint.to_string())))
                    .child(list(self))
                    .into_any_element()
            }
            RangeState::NoRevisions => div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(note(
                    "The forge does not report this change request's revisions.".to_string(),
                ))
                .child(list(self))
                .into_any_element(),
            RangeState::Idle => list(self),
        }
    }

    fn render_unreachable(
        &self,
        message: String,
        theme: &Theme,
        entity: &Entity<Self>,
    ) -> AnyElement {
        let retry = entity.clone();
        let close = entity.clone();
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(theme.spacing.card_gap)
            .p(px(24.0))
            .child(
                div()
                    .max_w(px(520.0))
                    .child(ely_ui::message(Severity::Warning, message, theme)),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(ely_ui::text_button(
                        "change-request-reconnect",
                        "Retry",
                        Some(IconName::RefreshCw),
                        ButtonState::IDLE,
                        move |_, cx| retry.update(cx, |tab, cx| tab.retry(cx)),
                    ))
                    .child(ely_ui::text_button(
                        "change-request-close",
                        "Close",
                        Some(IconName::X),
                        ButtonState::IDLE,
                        move |_, cx| {
                            close.update(cx, |_, cx| cx.emit(ChangeRequestTabEvent::Close))
                        },
                    )),
            )
            .into_any_element()
    }
}

impl Render for ChangeRequestTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _perf = sirio_perf::span("ChangeRequestTab.render", cx.entity_id().as_u64());
        // Ely follows Sirio's theme only through an observer that runs after the
        // app's start-up closure; without this a Light user sees Ely's dark
        // palette (docs/testing/ely-forge-probe.md).
        crate::ely::sync_theme_if_changed(cx);
        // A restored tab loads the first time it is drawn (spec §8).
        if !self.started {
            self.started = true;
            let this = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = this.update(cx, |tab, cx| tab.connect(cx));
            });
        }
        self.ensure_composer(window, cx);
        let theme = *Theme::get(cx);
        let entity = cx.entity();
        let body = match self.unreachable.clone() {
            Some(message) => self.render_unreachable(message, &theme, &entity),
            None => {
                let retry = Self::retry_handle(&entity);
                let content = match self.inner {
                    InnerTab::Conversation => self.render_conversation(&theme, &entity),
                    InnerTab::Commits => {
                        slot_view(&self.commits, "commits", &theme, retry, None, |listing| {
                            self.render_commits(listing, &theme, &entity)
                        })
                    }
                    InnerTab::Checks => {
                        slot_view(&self.checks, "checks", &theme, retry, None, |listing| {
                            self.render_checks(listing, &theme, &entity)
                        })
                    }
                    InnerTab::Files => self.render_files_tab(&theme, &entity),
                };
                let embedded =
                    self.inner == InnerTab::Files && matches!(self.range, RangeState::Ready { .. });
                let body = div()
                    .id("change-request-body")
                    .debug_selector(|| "change-request-body".to_owned())
                    .flex_1()
                    .min_h(px(0.0));
                // The diff scrolls itself; a scroll container around it would fight it.
                let body = if embedded {
                    body.overflow_hidden()
                } else {
                    body.overflow_y_scroll().p(px(16.0))
                };
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_col()
                    .child(self.render_inner_strip(&theme, &entity))
                    .child(body.child(content))
                    .into_any_element()
            }
        };
        div()
            .id("change-request-tab")
            .debug_selector(|| "change-request-tab".to_owned())
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.sirio.canvas)
            .text_color(theme.ely.fg)
            .child(self.render_header(&theme, &entity))
            .children(self.render_merge_strip(&theme, &entity))
            .child(body)
            .children(self.render_merge_dialog(&theme, &entity))
    }
}

/// One header of *Checks*: a workflow run (GitHub) or a stage (GitLab), and
/// the checks under it (spec §15.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CheckGroup {
    pub key: String,
    pub title: String,
    /// Indexes into the listing, failed first, then running, queued, the rest.
    pub members: Vec<usize>,
    pub open_by_default: bool,
    /// The run whose failed jobs *Re-run failed* would send, when it may.
    pub rerun_failed: Option<u64>,
}

fn rank(status: CheckStatus) -> u8 {
    match status {
        CheckStatus::Failed => 0,
        CheckStatus::Running => 1,
        CheckStatus::Queued => 2,
        CheckStatus::Canceled => 3,
        _ => 4,
    }
}

fn rerunnable(check: &Check) -> bool {
    matches!(check.status, CheckStatus::Failed | CheckStatus::Canceled)
        && check.job.as_ref().is_some_and(|job| job.retryable)
}

/// Groups the checks by run; on GitLab, also the pipeline row that carries
/// *Re-run failed*, because GitLab retries a pipeline, not a stage.
pub(crate) fn check_groups(items: &[Check], forge: Forge, can_rerun: bool) -> (Option<CheckGroup>, Vec<CheckGroup>) {
    let mut groups: Vec<CheckGroup> = Vec::new();
    let mut other: Vec<usize> = Vec::new();
    for (index, check) in items.iter().enumerate() {
        let Some(group) = &check.group else {
            other.push(index);
            continue;
        };
        let run = check.job.as_ref().and_then(|job| job.run_id);
        let key = match (forge, run) {
            (Forge::GitHub, Some(run)) => format!("run:{run}:{group}"),
            _ => format!("group:{group}"),
        };
        match groups.iter_mut().find(|existing| existing.key == key) {
            Some(existing) => existing.members.push(index),
            None => groups.push(CheckGroup { key, title: group.clone(), members: vec![index], open_by_default: false, rerun_failed: None }),
        }
    }
    if !other.is_empty() {
        groups.push(CheckGroup { key: "other".into(), title: "Other checks".into(), members: other, open_by_default: false, rerun_failed: None });
    }
    for group in &mut groups {
        group.members.sort_by_key(|index| (rank(items[*index].status), *index));
        group.open_by_default = group.members.iter().any(|index| rank(items[*index].status) <= 1);
        if forge == Forge::GitHub && can_rerun && group.key != "other" {
            group.rerun_failed = group
                .members
                .iter()
                .find(|index| rerunnable(&items[**index]))
                .and_then(|index| items[*index].job.as_ref().and_then(|job| job.run_id));
        }
    }
    // Worst group first; "Other checks" always last.
    let worst = |group: &CheckGroup| group.members.iter().map(|index| rank(items[*index].status)).min().unwrap_or(4);
    let other_last = |group: &CheckGroup| group.key == "other";
    groups.sort_by_key(|group| (other_last(group), worst(group)));
    let pipeline = (forge == Forge::GitLab)
        .then(|| items.iter().find_map(|check| check.job.as_ref().and_then(|job| job.run_id)))
        .flatten()
        .map(|run| CheckGroup {
            key: "pipeline".into(),
            title: format!("Pipeline #{run}"),
            members: Vec::new(),
            open_by_default: true,
            rerun_failed: (can_rerun && items.iter().any(rerunnable)).then_some(run),
        });
    (pipeline, groups)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use gpui::TestAppContext;
    use sirio_forge::Forge;
    use sirio_theme::Theme;

    use super::*;
    use super::actions::ActionState;
    use crate::changes::ChangesTabEvent;
    use crate::forge_source::testing::{self, CannedForge, FakeSource};
    use crate::forge_source::{self, Connection, RevisionError};

    fn pump_until(cx: &TestAppContext, mut condition: impl FnMut() -> bool) {
        cx.executor().allow_parking();
        // These tests wait on real `git` processes, as `changes.rs`'s do.
        for _ in 0..3000 {
            if condition() {
                return;
            }
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(100));
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.run_until_parked();
        }
        panic!("condition never became true within the pump budget");
    }

    fn forge_with_header() -> Arc<CannedForge> {
        let forge = Arc::new(CannedForge::default());
        forge.answer("Viewer", testing::viewer());
        forge.answer(
            "ChangeRequestHeader",
            testing::header(101, "Fix the login redirect", "## What\n\nFixes it."),
        );
        forge.answer("ChangeRequestCommits", testing::commits());
        forge
    }

    #[test]
    fn images_become_text_and_remote_ones_links() {
        let mut doc = markdown::parse(
            "![after](https://ghe.test/a.png)\n\n![](https://ghe.test/b.png)\n\n![local](docs/c.png)\n",
        );
        images_as_text(&mut doc);
        let paragraphs: Vec<(String, Option<String>)> = doc
            .blocks
            .iter()
            .map(|block| match &block.kind {
                markdown::BlockKind::Paragraph(text) => (
                    text.text.clone(),
                    text.marks.iter().find_map(|span| match &span.mark {
                        markdown::Mark::Link(url) => Some(url.clone()),
                        _ => None,
                    }),
                ),
                other => panic!("an image survived as {other:?}"),
            })
            .collect();
        assert_eq!(
            paragraphs,
            vec![
                (
                    "after".to_string(),
                    Some("https://ghe.test/a.png".to_string())
                ),
                (
                    "image".to_string(),
                    Some("https://ghe.test/b.png".to_string())
                ),
                ("local".to_string(), None),
            ]
        );
    }

    /// The composer is built by a render, not the constructor: the constructor
    /// has no window, and Ely's input needs one.
    #[gpui::test]
    fn the_composer_is_built_by_a_render_and_not_by_the_constructor(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let source = FakeSource::ready(testing::github_client(forge_with_header()), None);
        cx.update(|cx| forge_source::set_source(source, cx));
        let windowless = cx.new(|cx| {
            ChangeRequestTab::new(testing::reference(101), String::new(), std::env::temp_dir(), cx)
        });
        assert!(windowless.read_with(cx, |tab, _| tab.actions.composer.is_none()));
        let (tab, cx) = cx.add_window_view(|_, cx| {
            ChangeRequestTab::new(testing::reference(101), String::new(), std::env::temp_dir(), cx)
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(tab.read_with(cx, |tab, _| tab.actions.composer.is_some()));
    }

    /// A composer holding only blanks reads as blank, so *Comment* stays off;
    /// the E2E reads the same flag through the report.
    #[gpui::test]
    fn a_blank_composer_is_blank_and_a_typed_one_is_not(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let source = FakeSource::ready(testing::github_client(forge_with_header()), None);
        cx.update(|cx| forge_source::set_source(source, cx));
        let (tab, cx) = cx.add_window_view(|_, cx| {
            ChangeRequestTab::new(testing::reference(101), String::new(), std::env::temp_dir(), cx)
        });
        let input = tab.update_in(cx, |tab, window, cx| {
            tab.ensure_composer(window, cx);
            tab.actions.composer.clone().expect("built")
        });
        assert!(tab.read_with(cx, |tab, _| tab.actions.composer_blank));
        input.update(cx, |input, cx| input.set_text("  \n ", cx));
        assert!(tab.read_with(cx, |tab, _| tab.actions.composer_blank), "whitespace is blank");
        input.update(cx, |input, cx| input.set_text("looks good", cx));
        assert!(!tab.read_with(cx, |tab, _| tab.actions.composer_blank));
    }

    /// A refresh refused because the forge is rate-limited must leave the CI
    /// timer armed while CI is still running, or the tab never looks again.
    #[gpui::test]
    fn a_paused_refresh_keeps_the_ci_timer_armed_while_ci_runs(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let source = FakeSource::ready(testing::github_client(forge_with_header()), None);
        cx.update(|cx| forge_source::set_source(source, cx));
        let tab = cx.new(|cx| {
            ChangeRequestTab::new(testing::reference(101), String::new(), std::env::temp_dir(), cx)
        });
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        pump_until(cx, || tab.read_with(cx, |tab, _| tab.header.value().is_some()));
        tab.update(cx, |tab, cx| {
            if let Slot::Loaded { value, .. } = &mut tab.header {
                value.summary.ci = CiState::Running(None);
            }
            tab.ci_timer = None;
            tab.paused_until = Some(style::now() + 3600);
            tab.refresh(cx);
            assert!(
                tab.ci_timer.is_some(),
                "a refresh refused by the rate limit dropped the CI timer"
            );
        });
    }

    #[gpui::test]
    fn a_timer_refresh_that_hits_the_rate_limit_keeps_the_ci_timer_armed(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let source = FakeSource::ready(testing::github_client(forge_with_header()), None);
        cx.update(|cx| forge_source::set_source(source, cx));
        let tab = cx.new(|cx| {
            ChangeRequestTab::new(testing::reference(101), String::new(), std::env::temp_dir(), cx)
        });
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        pump_until(cx, || tab.read_with(cx, |tab, _| tab.header.value().is_some()));
        tab.update(cx, |tab, cx| {
            if let Slot::Loaded { value, .. } = &mut tab.header {
                value.summary.ci = CiState::Running(None);
            }
            // The state the timer's task leaves: it cleared itself, then asked.
            tab.ci_timer = None;
            tab.apply_header(
                Err(ForgeError::RateLimited {
                    host: "github.com".into(),
                    reset_at: Some(style::now() + 3600),
                }),
                cx,
            );
            assert!(
                tab.ci_timer.is_some(),
                "a rate-limited timer refresh stopped the CI polling for good"
            );
        });
    }

    #[gpui::test]
    fn a_selected_tab_loads_its_header_and_follows_the_forges_title(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let forge = forge_with_header();
        let source = FakeSource::ready(testing::github_client(forge.clone()), None);
        cx.update(|cx| forge_source::set_source(source, cx));
        let tab =
            cx.new(|cx| ChangeRequestTab::new(testing::reference(101), "old title".into(), std::env::temp_dir(), cx));
        let titles = Rc::new(RefCell::new(Vec::new()));
        let seen = titles.clone();
        cx.update(|cx| {
            cx.subscribe(&tab, move |_, event: &ChangeRequestTabEvent, _| {
                if let ChangeRequestTabEvent::TitleChanged(title) = event {
                    seen.borrow_mut().push(title.clone());
                }
            })
            .detach()
        });
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| tab.header.value().is_some())
        });
        tab.read_with(cx, |tab, _| {
            assert_eq!(tab.title(), "Fix the login redirect");
            assert!(
                tab.description.is_some(),
                "the body is rendered once, when it arrives"
            );
        });
        assert_eq!(*titles.borrow(), vec!["Fix the login redirect".to_string()]);
    }

    /// The title of a change request is the first thing a reviewer wants to
    /// paste into a message; it and the meta line under it were painted but
    /// not selectable. Drives the real tab, loaded from the fake forge.
    #[gpui::test]
    fn the_title_and_meta_line_of_a_change_request_can_be_copied(cx: &mut TestAppContext) {
        let forge = forge_with_header();
        let source = FakeSource::ready(testing::github_client(forge), None);
        cx.update(|cx| forge_source::set_source(source, cx));
        let executor = cx.executor();
        let (tab, _, cx) = crate::text_selection::testing::host(cx, |_, cx| {
            ChangeRequestTab::new(testing::reference(101), String::new(), std::env::temp_dir(), cx)
        });
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        executor.allow_parking();
        for _ in 0..600 {
            if tab.read_with(cx, |tab, _| tab.header.value().is_some()) {
                break;
            }
            executor.advance_clock(std::time::Duration::from_millis(100));
            std::thread::sleep(std::time::Duration::from_millis(5));
            cx.run_until_parked();
        }
        cx.run_until_parked();

        assert_eq!(
            crate::text_selection::testing::copy_line(cx, "change-request-title").as_deref(),
            Some("Fix the login redirect")
        );
        let meta = crate::text_selection::testing::copy_line(cx, "change-request-meta")
            .expect("the clipboard is readable");
        assert!(
            meta.contains(" → ") && meta.contains("updated"),
            "the meta line copied whole: {meta:?}"
        );
    }

    /// A pull request's description is prose a reviewer quotes. It is
    /// rendered Markdown, which had no selection at all outside the chat.
    #[gpui::test]
    fn the_description_of_a_change_request_can_be_selected_and_copied(cx: &mut TestAppContext) {
        let forge = forge_with_header();
        let source = FakeSource::ready(testing::github_client(forge), None);
        cx.update(|cx| forge_source::set_source(source, cx));
        let executor = cx.executor();
        let (tab, _, cx) = crate::text_selection::testing::host(cx, |_, cx| {
            ChangeRequestTab::new(testing::reference(101), String::new(), std::env::temp_dir(), cx)
        });
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        executor.allow_parking();
        for _ in 0..600 {
            if tab.read_with(cx, |tab, _| tab.header.value().is_some()) {
                break;
            }
            executor.advance_clock(std::time::Duration::from_millis(100));
            std::thread::sleep(std::time::Duration::from_millis(5));
            cx.run_until_parked();
        }
        cx.run_until_parked();

        // The heading and the paragraph, joined the way the page reads.
        assert_eq!(
            crate::text_selection::testing::copy_span(cx, "change-request-description", px(13.0))
                .as_deref(),
            Some("What\n\nFixes it.")
        );
    }

    #[gpui::test]
    fn an_inner_tab_loads_the_first_time_it_is_shown(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let forge = forge_with_header();
        let source = FakeSource::ready(testing::github_client(forge.clone()), None);
        cx.update(|cx| forge_source::set_source(source, cx));
        let tab = cx.new(|cx| ChangeRequestTab::new(testing::reference(101), String::new(), std::env::temp_dir(), cx));
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| tab.header.value().is_some())
        });
        assert_eq!(
            forge.count("ChangeRequestCommits"),
            0,
            "nothing is asked for a tab never shown"
        );
        tab.update(cx, |tab, cx| tab.select_inner(InnerTab::Commits, cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| tab.commits.value().is_some())
        });
        tab.update(cx, |tab, cx| tab.select_inner(InnerTab::Conversation, cx));
        tab.update(cx, |tab, cx| tab.select_inner(InnerTab::Commits, cx));
        cx.run_until_parked();
        assert_eq!(
            forge.count("ChangeRequestCommits"),
            1,
            "a loaded tab is not asked again on return"
        );
    }

    #[gpui::test]
    fn a_failed_inner_tab_keeps_the_conversation(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let forge = forge_with_header();
        forge.fail("ChangeRequestChecks", 500);
        let source = FakeSource::ready(testing::github_client(forge), None);
        cx.update(|cx| forge_source::set_source(source, cx));
        let tab = cx.new(|cx| ChangeRequestTab::new(testing::reference(101), String::new(), std::env::temp_dir(), cx));
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| tab.header.value().is_some())
        });
        tab.update(cx, |tab, cx| tab.select_inner(InnerTab::Checks, cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| matches!(tab.checks, Slot::Failed(_)))
        });
        tab.read_with(cx, |tab, _| {
            assert!(
                tab.header.value().is_some(),
                "an inner tab's failure is its own"
            );
        });
    }

    #[gpui::test]
    fn a_tab_that_cannot_connect_keeps_its_title_and_offers_retry(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let source = FakeSource::with(Connection::NotConnected {
            forge: Forge::GitHub,
            host: "ghe.test".into(),
        });
        cx.update(|cx| forge_source::set_source(source.clone(), cx));
        let tab = cx.new(|cx| {
            ChangeRequestTab::restored(
                testing::reference(101),
                "Fix the login redirect".into(),
                InnerTab::Checks,
                std::env::temp_dir(),
                cx,
            )
        });
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        pump_until(cx, || tab.read_with(cx, |tab, _| tab.unreachable.is_some()));
        tab.read_with(cx, |tab, _| {
            assert_eq!(
                tab.title(),
                "Fix the login redirect",
                "the saved title stays"
            );
            assert!(
                tab.unreachable
                    .as_deref()
                    .is_some_and(|why| why.contains("gh auth login --hostname ghe.test"))
            );
        });
        tab.update(cx, |tab, cx| tab.retry(cx));
        assert_eq!(
            *source.forgotten.lock().unwrap(),
            vec!["ghe.test".to_string()],
            "Retry asks the host again"
        );
    }

    struct RepoDir(PathBuf);

    impl Drop for RepoDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git must be installed to run these tests");
        assert!(
            output.status.success(),
            "`git {args:?}` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn commit_all(dir: &Path, message: &str) {
        git(dir, &["add", "-A"]);
        git(dir, &["-c", "commit.gpgSign=false", "commit", "-q", "-m", message]);
    }

    /// `main` → `feat`: `a.txt` edited at line 42 of 60, `c.txt` added,
    /// `gone.txt` deleted. Returns the repository and its `(base, head)`.
    fn range_repo() -> (RepoDir, String, String) {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("sirio-crtab-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create repo dir");
        let dir = std::fs::canonicalize(&dir).expect("canonicalize");
        git(&dir, &["init", "-q", "-b", "main"]);
        git(&dir, &["config", "user.email", "tests@example.invalid"]);
        git(&dir, &["config", "user.name", "Sirio tests"]);
        let sixty = |changed: Option<usize>| -> String {
            (1..=60)
                .map(|n| if Some(n) == changed { format!("edited {n}\n") } else { format!("line {n}\n") })
                .collect()
        };
        std::fs::write(dir.join("a.txt"), sixty(None)).expect("a.txt");
        std::fs::write(dir.join("gone.txt"), "bye\n").expect("gone.txt");
        commit_all(&dir, "base");
        let base = git(&dir, &["rev-parse", "HEAD"]);
        git(&dir, &["checkout", "-q", "-b", "feat"]);
        std::fs::write(dir.join("a.txt"), sixty(Some(42))).expect("edit a.txt");
        std::fs::write(dir.join("c.txt"), "new\n").expect("c.txt");
        git(&dir, &["rm", "-q", "gone.txt"]);
        commit_all(&dir, "change");
        let head = git(&dir, &["rev-parse", "HEAD"]);
        (RepoDir(dir), base, head)
    }

    /// One more commit on `feat`; the new head.
    fn push_another(dir: &Path) -> String {
        std::fs::write(dir.join("d.txt"), "later\n").expect("d.txt");
        commit_all(dir, "later");
        git(dir, &["rev-parse", "HEAD"])
    }

    fn report_value(tab: &ChangeRequestTab, cx: &App, key: &str) -> String {
        tab.report(cx)
            .into_iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
            .unwrap_or_default()
    }

    fn forge_for(base: &str, head: &str) -> Arc<CannedForge> {
        let forge = Arc::new(CannedForge::default());
        forge.answer("Viewer", testing::viewer());
        forge.answer(
            "ChangeRequestHeader",
            testing::header_with_revisions(101, "Fix the login redirect", "## What", base, head),
        );
        forge.answer(
            "ChangeRequestFiles",
            testing::files(&[("a.txt", 1, 1, "MODIFIED"), ("c.txt", 1, 0, "ADDED"), ("gone.txt", 0, 1, "DELETED")]),
        );
        forge.answer("ChangeRequestCommits", testing::commits());
        forge
    }

    fn open_tab(
        cx: &mut TestAppContext,
        source: Arc<FakeSource>,
        repo: &RepoDir,
        inner: InnerTab,
    ) -> Entity<ChangeRequestTab> {
        cx.update(|cx| {
            Theme::init(cx);
            forge_source::set_source(source.clone(), cx);
        });
        let tab = cx.new(|cx| {
            ChangeRequestTab::new(testing::reference(101), "Fix".to_string(), repo.0.clone(), cx)
        });
        tab.update(cx, |tab, cx| {
            tab.on_selected(cx);
            tab.select_inner(inner, cx);
        });
        tab
    }

    fn ready_changes(tab: &Entity<ChangeRequestTab>, cx: &TestAppContext) -> Entity<crate::changes::ChangesTab> {
        tab.read_with(cx, |tab, _| match &tab.range {
            RangeState::Ready { changes, .. } => changes.clone(),
            _ => panic!("the diff is not ready"),
        })
    }

    #[gpui::test]
    async fn files_makes_the_revisions_local_then_shows_the_diff(cx: &mut TestAppContext) {
        let (repo, base, head) = range_repo();
        let source = FakeSource::ready(testing::github_client(forge_for(&base, &head)), None);
        let tab = open_tab(cx, source.clone(), &repo, InnerTab::Files);
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "files_mode") == "diff"));
        tab.read_with(cx, |tab, cx| {
            assert_eq!(report_value(tab, cx, "state"), "loaded");
            assert_eq!(report_value(tab, cx, "head"), head[..7]);
        });
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "diff_files") == "3"));
        tab.read_with(cx, |tab, cx| {
            assert_eq!(
                report_value(tab, cx, "diff_summary"),
                "a.txt +1 -1|c.txt +1 -0|gone.txt +0 -1"
            );
        });
        let ensured = source.ensured.lock().unwrap().clone();
        assert_eq!(ensured.len(), 1, "one fetch for the tab");
        let (reference, revisions, target) = &ensured[0];
        assert_eq!(*reference, testing::reference(101));
        assert_eq!(
            (revisions.base_sha.as_str(), revisions.head_sha.as_str(), target.as_deref()),
            (base.as_str(), head.as_str(), Some("main"))
        );
    }

    #[gpui::test]
    async fn a_failed_fetch_keeps_the_forges_list_and_says_why(cx: &mut TestAppContext) {
        let (repo, base, head) = range_repo();
        let source = FakeSource::ready(testing::github_client(forge_for(&base, &head)), None);
        *source.revisions_answer.lock().unwrap() =
            Err(RevisionError::FetchFailed { detail: "boom".to_string() });
        let tab = open_tab(cx, source, &repo, InnerTab::Files);
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "files_mode") == "error" && report_value(tab, cx, "list_rows") == "3"
            })
        });
        tab.read_with(cx, |tab, cx| {
            assert!(report_value(tab, cx, "files_error").contains("boom"));
            assert_eq!(report_value(tab, cx, "state"), "error");
        });
    }

    #[gpui::test]
    async fn a_forge_that_reports_no_revisions_leaves_its_own_list(cx: &mut TestAppContext) {
        let (repo, base, head) = range_repo();
        let forge = forge_for(&base, &head);
        forge.answer("ChangeRequestHeader", testing::header(101, "Fix the login redirect", "## What"));
        let source = FakeSource::ready(testing::github_client(forge), None);
        let tab = open_tab(cx, source.clone(), &repo, InnerTab::Files);
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "files_mode") == "no-revisions" && report_value(tab, cx, "rows") == "3"
            })
        });
        assert!(source.ensured.lock().unwrap().is_empty(), "nothing to fetch by");
    }

    #[gpui::test]
    async fn a_new_head_rebuilds_the_range_and_keeps_the_opened_files(cx: &mut TestAppContext) {
        let (repo, base, head) = range_repo();
        let forge = forge_for(&base, &head);
        let source = FakeSource::ready(testing::github_client(forge.clone()), None);
        let tab = open_tab(cx, source, &repo, InnerTab::Files);
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "files_mode") == "diff"));
        let opened = vec![PathBuf::from("a.txt"), PathBuf::from("c.txt")];
        let changes = ready_changes(&tab, cx);
        changes.update(cx, |changes, cx| {
            changes.focus_path(Path::new("a.txt"), cx);
            changes.focus_path(Path::new("c.txt"), cx);
        });
        pump_until(cx, || changes.read_with(cx, |changes, _| changes.expanded_paths() == opened));

        let newer = push_another(&repo.0);
        forge.answer(
            "ChangeRequestHeader",
            testing::header_with_revisions(101, "Fix the login redirect", "## What", &base, &newer),
        );
        tab.update(cx, |tab, cx| tab.refresh(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "head") == newer[..7] && report_value(tab, cx, "files_mode") == "diff"
            })
        });
        tab.read_with(cx, |tab, cx| {
            assert!(report_value(tab, cx, "notice").contains("Updated to head"));
        });
        // Both opened files come back, not only the last one carried.
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| match &tab.range {
                RangeState::Ready { changes, .. } => changes.read(cx).expanded_paths() == opened,
                _ => false,
            })
        });
    }

    fn threads_json(nodes: Vec<serde_json::Value>) -> String {
        serde_json::json!({"data": {"repository": {"pullRequest": {"reviewThreads": {
            "pageInfo": {"hasNextPage": false, "endCursor": null}, "nodes": nodes
        }}}}})
        .to_string()
    }

    /// One unresolved thread on new-side `line` of a.txt, with one comment.
    fn thread_node(id: &str, line: u32) -> serde_json::Value {
        serde_json::json!({
            "id": id, "isResolved": false, "isOutdated": false, "path": "a.txt",
            "line": line, "startLine": null, "originalLine": line, "originalStartLine": null,
            "diffSide": "RIGHT", "startDiffSide": null, "subjectType": "LINE",
            "viewerCanReply": true, "viewerCanResolve": true, "viewerCanUnresolve": false,
            "resolvedBy": null,
            "comments": {"nodes": [{
                "id": format!("{id}-c"), "author": {"login": "bob"}, "body": "Handle it.",
                "createdAt": "2026-09-20T10:00:00Z", "diffHunk": "",
                "viewerCanUpdate": false,
                "pullRequestReview": {"state": "COMMENTED"}
            }]}
        })
    }

    fn facts_json(reply: bool, resolve: bool, unresolve: bool) -> String {
        serde_json::json!({"data": {"node": {"viewerCanReply": reply, "viewerCanResolve": resolve,
            "viewerCanUnresolve": unresolve, "pullRequest": {"number": 101}}}}).to_string()
    }

    async fn a_thread(cx: &mut TestAppContext, node: serde_json::Value) -> (Entity<ChangeRequestTab>, Arc<CannedForge>, RepoDir) {
        let (repo, base, head) = range_repo();
        let forge = forge_for(&base, &head);
        forge.answer("ChangeRequestThreads", threads_json(vec![node]));
        forge.answer("ChangeRequestActionContext", action_context_json(&head));
        forge.answer("ReviewThreadContext", facts_json(true, true, true));
        let source = FakeSource::ready(testing::github_client(forge.clone()), None);
        let tab = open_tab(cx, source, &repo, InnerTab::Files);
        pump_until(cx, || tab.read_with(cx, |tab, _| tab.threads.value().is_some()));
        (tab, forge, repo)
    }

    /// A thread node whose last comment is part of the viewer's pending review.
    fn with_pending_comment(mut node: serde_json::Value, id: &str, body: &str) -> serde_json::Value {
        node["comments"]["nodes"].as_array_mut().expect("comments").push(serde_json::json!({
            "id": id,
            "author": { "login": "fake-user" },
            "body": body,
            "createdAt": "2026-10-05T10:00:00Z",
            "viewerCanUpdate": true,
            "diffHunk": "@@ -1 +1 @@\n line",
            "pullRequestReview": { "id": "PRR_1", "state": "PENDING" },
        }));
        node
    }

    /// A pull request answer (header or action context) with the viewer's
    /// pending review in it.
    fn with_pending_review(answer: String, id: &str, comments: u32) -> String {
        let mut value: serde_json::Value = serde_json::from_str(&answer).expect("JSON");
        value["data"]["repository"]["pullRequest"]["pendingReview"] =
            serde_json::json!({ "nodes": [ { "id": id, "comments": { "totalCount": comments } } ] });
        value.to_string()
    }

    /// The id `thread_node` gives its comment.
    fn first_comment_id() -> String {
        "PRRT_1-c".to_string()
    }

    /// The full head SHA the tab's diff was built from.
    fn report_head(tab: &Entity<ChangeRequestTab>, cx: &TestAppContext) -> String {
        tab.read_with(cx, |tab, _| match &tab.range {
            RangeState::Ready { revisions, .. } => revisions.head_sha.clone(),
            _ => panic!("the diff is not ready"),
        })
    }

    #[gpui::test]
    async fn a_pending_comment_is_drawn_and_can_be_deleted(cx: &mut TestAppContext) {
        let node = with_pending_comment(thread_node("PRRT_1", 42), "PRRC_9", "Not sent yet.");
        let (tab, forge, _repo) = a_thread(cx, node).await;
        tab.read_with(cx, |tab, cx| {
            assert_eq!(report_value(tab, cx, "threads_pending"), "1");
            assert_eq!(report_value(tab, cx, "threads_open"), "1", "the open count stays published-only");
        });
        forge.answer("DeletePullRequestReviewComment", ok_mutation("deletePullRequestReviewComment"));
        tab.update(cx, |tab, cx| tab.delete_draft_comment("PRRC_9", cx)).expect("a pending comment");
        pump_until(cx, || forge.count("DeletePullRequestReviewComment") == 1);
        assert_eq!(forge.sent("DeletePullRequestReviewComment").expect("sent")["input"], serde_json::json!({ "id": "PRRC_9" }));
        let published = tab.update(cx, |tab, cx| tab.delete_draft_comment(&first_comment_id(), cx));
        assert!(published.is_err(), "a published comment is not deleted");
    }

    #[gpui::test]
    async fn a_thread_with_only_pending_comments_is_drawn_and_offers_no_reply(cx: &mut TestAppContext) {
        let mut node = with_pending_comment(thread_node("PRRT_2", 42), "PRRC_9", "Mine.");
        node["comments"]["nodes"].as_array_mut().unwrap().remove(0);
        let (tab, _forge, _repo) = a_thread(cx, node).await;
        // The card needs the diff as well as the threads; the file starts
        // folded, so open it before asking where the card sits.
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "files_mode") == "diff"));
        let changes = ready_changes(&tab, cx);
        changes.update(cx, |changes, cx| changes.focus_path(Path::new("a.txt"), cx));
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "thread_rows").contains("PRRT_2:line:")));
        tab.read_with(cx, |tab, cx| {
            assert!(report_value(tab, cx, "thread_rows").contains("PRRT_2:line:"), "drawn in the diff");
            assert_eq!(report_value(tab, cx, "conversation_threads"), "0", "not in the Conversation");
        });
        let refused = tab.update(cx, |tab, cx| tab.open_reply("PRRT_2", cx));
        assert_eq!(refused, Err("That thread is part of your review in progress.".to_string()));
    }

    #[gpui::test]
    async fn a_reply_added_to_the_review_in_progress_joins_it(cx: &mut TestAppContext) {
        let (tab, forge, _repo) = a_thread(cx, thread_node("PRRT_1", 42)).await;
        let head = report_head(&tab, cx);
        forge.answer("ChangeRequestActionContext", with_pending_review(action_context_json(&head), "PRR_1", 1));
        forge.answer("AddPullRequestReviewThreadReply", ok_mutation("addPullRequestReviewThreadReply"));
        tab.update(cx, |tab, cx| tab.open_reply("PRRT_1", cx)).expect("a loaded thread");
        tab.update(cx, |tab, cx| tab.reply_set_text("PRRT_1", "In the review.", cx)).expect("an open reply");
        tab.update(cx, |tab, cx| tab.send_reply_to_review("PRRT_1", cx)).expect("sent");
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "thread_replying").is_empty()));
        let input = &forge.sent("AddPullRequestReviewThreadReply").expect("sent")["input"];
        assert_eq!(input["pullRequestReviewId"], "PRR_1");
        assert_eq!(input["body"], "In the review.");
        assert_eq!(forge.count("AddPullRequestReview"), 0, "the review in progress is reused");
    }

    fn ok_mutation(field: &str) -> String {
        serde_json::json!({"data": {field: {"clientMutationId": null}}}).to_string()
    }

    #[gpui::test]
    async fn a_reply_goes_to_its_thread_and_its_field_closes_on_success(cx: &mut TestAppContext) {
        let (tab, forge, _repo) = a_thread(cx, thread_node("PRRT_1", 42)).await;
        forge.answer("AddPullRequestReviewThreadReply", ok_mutation("addPullRequestReviewThreadReply"));
        tab.update(cx, |tab, cx| tab.open_reply("PRRT_1", cx)).expect("a loaded thread");
        tab.update(cx, |tab, cx| tab.reply_set_text("PRRT_1", "Done in the next push.", cx)).expect("an open reply");
        assert_eq!(tab.read_with(cx, |tab, cx| report_value(tab, cx, "thread_replying")), "PRRT_1");
        let reads = forge.count("ChangeRequestThreads");
        tab.update(cx, |tab, cx| tab.send_reply("PRRT_1", cx)).expect("sent");
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "action") == "idle" && report_value(tab, cx, "thread_replying").is_empty()));
        assert_eq!(forge.count("AddPullRequestReviewThreadReply"), 1);
        pump_until(cx, || forge.count("ChangeRequestThreads") > reads);
    }

    #[gpui::test]
    async fn a_second_write_while_a_reply_is_in_flight_is_refused(cx: &mut TestAppContext) {
        let (tab, forge, _repo) = a_thread(cx, thread_node("PRRT_1", 42)).await;
        tab.update(cx, |tab, cx| tab.open_reply("PRRT_1", cx)).expect("a loaded thread");
        tab.update(cx, |tab, cx| tab.reply_set_text("PRRT_1", "Wait for it", cx)).expect("an open reply");
        tab.update(cx, |tab, _| tab.actions.state = ActionState::Working("reply"));
        let refused = tab.update(cx, |tab, cx| tab.resolve_thread("PRRT_1", true, cx));
        assert_eq!(refused, Err("an action is already in flight".to_string()));
        cx.run_until_parked();
        assert_eq!(forge.count("ResolveReviewThread"), 0);
        tab.read_with(cx, |tab, cx| {
            assert_eq!(report_value(tab, cx, "thread_replying"), "PRRT_1");
            assert_eq!(tab.reply_text("PRRT_1", cx).as_deref(), Some("Wait for it"));
        });
    }

    /// Like `a_thread`, but the tab is built in a window, so the test can call
    /// the socket entry `control_act`, which needs one. The window context is
    /// cloned out of the borrow, so the caller keeps using `cx` afterwards.
    async fn windowed_thread(
        cx: &mut TestAppContext,
    ) -> (
        Entity<ChangeRequestTab>,
        Arc<CannedForge>,
        RepoDir,
        gpui::VisualTestContext,
    ) {
        let (repo, base, head) = range_repo();
        let forge = forge_for(&base, &head);
        forge.answer("ChangeRequestThreads", threads_json(vec![thread_node("PRRT_1", 42)]));
        forge.answer("ChangeRequestActionContext", action_context_json(&head));
        forge.answer("ReviewThreadContext", facts_json(true, true, true));
        let source = FakeSource::ready(testing::github_client(forge.clone()), None);
        cx.update(|cx| {
            Theme::init(cx);
            forge_source::set_source(source, cx);
        });
        let (tab, vcx) = {
            let (tab, vcx) = cx.add_window_view(|_, cx| {
                ChangeRequestTab::new(testing::reference(101), "Fix".to_string(), repo.0.clone(), cx)
            });
            (tab, vcx.clone())
        };
        tab.update(cx, |tab, cx| {
            tab.on_selected(cx);
            tab.select_inner(InnerTab::Files, cx);
        });
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "threads_open") == "1"));
        (tab, forge, repo, vcx)
    }

    /// A socket thread write refused while a reply is in flight leaves that
    /// write — its Working state, task and target — alone, and the reason
    /// shows on the thread's card instead of in `action_message`.
    #[gpui::test]
    async fn a_socket_resolve_while_a_reply_is_in_flight_keeps_that_reply(cx: &mut TestAppContext) {
        let (tab, forge, _repo, mut vcx) = windowed_thread(cx).await;
        tab.update(cx, |tab, cx| tab.open_reply("PRRT_1", cx)).expect("a loaded thread");
        tab.update(cx, |tab, cx| tab.reply_set_text("PRRT_1", "Wait for it", cx)).expect("an open reply");
        tab.update(cx, |tab, _| {
            tab.actions.state = ActionState::Working("reply");
            tab.write_target = Some(compose::WriteTarget::Reply("PRRT_1".to_string()));
        });
        let params = BTreeMap::from([("thread".to_string(), "PRRT_1".to_string())]);
        let refused =
            tab.update_in(&mut vcx, |tab, window, cx| tab.control_act("resolve", &params, window, cx));
        assert_eq!(refused, Err("an action is already in flight".to_string()));
        tab.read_with(cx, |tab, cx| {
            assert!(matches!(tab.actions.state, ActionState::Working("reply")));
            assert_eq!(tab.write_target, Some(compose::WriteTarget::Reply("PRRT_1".to_string())));
            assert_eq!(report_value(tab, cx, "action_message"), "");
            assert_eq!(
                tab.thread_write_error("PRRT_1", cx).as_deref(),
                Some("an action is already in flight")
            );
            assert_eq!(tab.reply_text("PRRT_1", cx).as_deref(), Some("Wait for it"));
        });
        assert_eq!(forge.count("ResolveReviewThread"), 0);
    }

    /// A socket thread write refused with nothing in flight reports its reason
    /// in `action_message`, which is where the E2E reads it.
    #[gpui::test]
    async fn a_socket_thread_refusal_with_nothing_in_flight_is_reported(cx: &mut TestAppContext) {
        let (tab, forge, _repo, mut vcx) = windowed_thread(cx).await;
        let params = BTreeMap::from([("text".to_string(), "Late note".to_string())]);
        let refused =
            tab.update_in(&mut vcx, |tab, window, cx| tab.control_act("line-comment", &params, window, cx));
        assert_eq!(
            refused,
            Err("No comment is being written; open one with thread --compose.".to_string())
        );
        tab.read_with(cx, |tab, cx| {
            assert_eq!(report_value(tab, cx, "action"), "failed");
            assert_eq!(report_value(tab, cx, "action_kind"), "line-comment");
            assert_eq!(
                report_value(tab, cx, "action_message"),
                "No comment is being written; open one with thread --compose."
            );
        });
        let empty: BTreeMap<String, String> = BTreeMap::new();
        let refused =
            tab.update_in(&mut vcx, |tab, window, cx| tab.control_act("reply", &empty, window, cx));
        assert_eq!(refused, Err("reply needs thread".to_string()));
        tab.read_with(cx, |tab, cx| {
            assert_eq!(report_value(tab, cx, "action_kind"), "reply");
            assert_eq!(report_value(tab, cx, "action_message"), "reply needs thread");
        });
        assert_eq!(forge.rest_count(COMMENTS), 0);
    }

    /// An idle socket refusal after a completed write belongs to no earlier
    /// field: `line_composer_error` stays empty while `action_message`
    /// carries the refusal.
    #[gpui::test]
    async fn an_idle_socket_refusal_after_a_completed_write_belongs_to_no_field(
        cx: &mut TestAppContext,
    ) {
        let (tab, forge, _repo, mut vcx) = windowed_thread(cx).await;
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "files_mode") == "diff"
                    && report_value(tab, cx, "commentable") == "yes"
            })
        });
        let changes = ready_changes(&tab, cx);
        changes.update(cx, |changes, cx| changes.focus_path(Path::new("a.txt"), cx));
        pump_until(cx, || changes.read_with(cx, |changes, _| {
            changes.comment_anchor(Path::new("a.txt"), AnnotationSide::New, 43, None).is_ok()
        }));
        tab.update(cx, |tab, cx| tab.compose_at("a.txt:new:43", cx)).expect("a line near the change");
        forge.answer_rest(COMMENTS, 201, r#"{"id":1}"#);
        tab.update(cx, |tab, cx| tab.composer_set_text("Looks right", cx));
        tab.update(cx, |tab, cx| tab.send_line_comment(cx)).expect("sent");
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "action") == "idle"
                    && report_value(tab, cx, "line_composer").is_empty()
            })
        });
        tab.read_with(cx, |tab, _| {
            assert_eq!(tab.write_target, Some(compose::WriteTarget::Composer));
        });
        let params = BTreeMap::from([("thread".to_string(), "missing".to_string())]);
        let refused =
            tab.update_in(&mut vcx, |tab, window, cx| tab.control_act("resolve", &params, window, cx));
        assert_eq!(refused, Err("no thread missing".to_string()));
        tab.read_with(cx, |tab, cx| {
            assert_eq!(report_value(tab, cx, "action"), "failed");
            assert_eq!(report_value(tab, cx, "action_kind"), "resolve");
            assert_eq!(report_value(tab, cx, "action_message"), "no thread missing");
            assert_eq!(report_value(tab, cx, "line_composer_error"), "");
        });
        assert_eq!(forge.count("ResolveReviewThread"), 0);
    }

    #[gpui::test]
    async fn a_resolved_thread_folds_once_the_forge_says_so(cx: &mut TestAppContext) {
        let (tab, forge, _repo) = a_thread(cx, thread_node("PRRT_1", 42)).await;
        forge.answer("ResolveReviewThread", ok_mutation("resolveReviewThread"));
        tab.update(cx, |tab, cx| tab.reveal_thread("PRRT_1", cx)).expect("a loaded thread");
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "thread_rows") == "PRRT_1:line:open"));
        let mut resolved = thread_node("PRRT_1", 42);
        resolved["isResolved"] = true.into();
        forge.answer("ChangeRequestThreads", threads_json(vec![resolved]));
        tab.update(cx, |tab, cx| tab.resolve_thread("PRRT_1", true, cx)).expect("sent");
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "thread_rows") == "PRRT_1:line:folded"));
        assert_eq!(forge.count("ResolveReviewThread"), 1);
    }

    #[gpui::test]
    async fn a_refused_reply_keeps_its_text_and_says_why_on_its_card(cx: &mut TestAppContext) {
        let (tab, forge, _repo) = a_thread(cx, thread_node("PRRT_1", 42)).await;
        forge.answer("ReviewThreadContext", facts_json(false, false, false));
        tab.update(cx, |tab, cx| tab.open_reply("PRRT_1", cx)).expect("a loaded thread");
        tab.update(cx, |tab, cx| tab.reply_set_text("PRRT_1", "Let me in", cx)).expect("an open reply");
        tab.update(cx, |tab, cx| tab.send_reply("PRRT_1", cx)).expect("sent");
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "action") == "failed"));
        tab.read_with(cx, |tab, cx| {
            assert_eq!(tab.reply_text("PRRT_1", cx).as_deref(), Some("Let me in"));
            assert_eq!(tab.thread_write_error("PRRT_1", cx).as_deref(), Some("You cannot reply to this thread."));
        });
        assert_eq!(forge.count("AddPullRequestReviewThreadReply"), 0);
    }

    #[gpui::test]
    async fn a_gutter_refusal_does_not_keep_the_failed_reply_s_owner(cx: &mut TestAppContext) {
        let (tab, forge, _repo) = a_thread(cx, thread_node("PRRT_1", 42)).await;
        forge.answer("ReviewThreadContext", facts_json(false, false, false));
        tab.update(cx, |tab, cx| tab.open_reply("PRRT_1", cx)).expect("a loaded thread");
        tab.update(cx, |tab, cx| tab.reply_set_text("PRRT_1", "Keep this reply", cx)).expect("an open reply");
        tab.update(cx, |tab, cx| tab.send_reply("PRRT_1", cx)).expect("the refusal arrives asynchronously");
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "action") == "failed"));
        assert_eq!(
            tab.read_with(cx, |tab, _| tab.write_target.clone()),
            Some(compose::WriteTarget::Reply("PRRT_1".to_string()))
        );

        let reason = "A range stays on one side of the diff.";
        let changes = ready_changes(&tab, cx);
        changes.update(cx, |_, cx| cx.emit(ChangesTabEvent::CommentRefused(reason.to_string())));
        tab.update(cx, |tab, cx| tab.sync_writes(cx));

        tab.read_with(cx, |tab, cx| {
            assert_eq!(report_value(tab, cx, "action_kind"), "line-comment");
            assert_eq!(report_value(tab, cx, "action_message"), reason);
            assert_eq!(tab.thread_write_error("PRRT_1", cx), None);
        });
    }

    #[gpui::test]
    async fn one_s_own_thread_comment_is_edited_as_a_review_comment(cx: &mut TestAppContext) {
        let mut node = thread_node("PRRT_1", 42);
        node["comments"]["nodes"][0]["viewerCanUpdate"] = true.into();
        let (tab, forge, _repo) = a_thread(cx, node).await;
        forge.answer("UpdatePullRequestReviewComment", ok_mutation("updatePullRequestReviewComment"));
        tab.update(cx, |tab, cx| tab.start_thread_comment_edit("PRRT_1-c", cx)).expect("an editable comment");
        tab.update(cx, |tab, cx| tab.thread_comment_set_text("Handle it, please.", cx)).expect("an open editor");
        tab.update(cx, |tab, cx| tab.save_thread_comment_edit(cx)).expect("sent");
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "action") == "idle" && report_value(tab, cx, "thread_comment_editing").is_empty()));
        assert_eq!(forge.count("UpdatePullRequestReviewComment"), 1);
        assert!(tab.update(cx, |tab, cx| tab.start_thread_comment_edit("nobody", cx)).is_err());
    }

    /// The CI timer's refresh reads no threads, but a new head it brings
    /// rebuilds the diff: the threads are read again for that head, or the
    /// cards keep the old head's lines.
    #[gpui::test]
    async fn a_new_head_from_the_ci_timer_reads_the_threads_again(cx: &mut TestAppContext) {
        let (repo, base, head) = range_repo();
        let forge = forge_for(&base, &head);
        forge.answer("ChangeRequestThreads", threads_json(vec![thread_node("PRRT_1", 42)]));
        let source = FakeSource::ready(testing::github_client(forge.clone()), None);
        let tab = open_tab(cx, source, &repo, InnerTab::Files);
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "files_mode") == "diff" && report_value(tab, cx, "threads_open") == "1"
            })
        });
        let reads = forge.count("ChangeRequestThreads");
        let newer = push_another(&repo.0);
        forge.answer(
            "ChangeRequestHeader",
            testing::header_with_revisions(101, "Fix the login redirect", "## What", &base, &newer),
        );
        tab.update(cx, |tab, cx| tab.refresh_header(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "head") == newer[..7] && report_value(tab, cx, "files_mode") == "diff"
            })
        });
        pump_until(cx, || forge.count("ChangeRequestThreads") > reads);
    }

    /// A thread read that fails says so in Files, and its Retry reads again;
    /// the diff is not left without cards and without a reason.
    #[gpui::test]
    async fn a_failed_thread_read_says_so_and_retry_reads_again(cx: &mut TestAppContext) {
        let (repo, base, head) = range_repo();
        let forge = forge_for(&base, &head);
        forge.fail("ChangeRequestThreads", 500);
        let source = FakeSource::ready(testing::github_client(forge.clone()), None);
        let tab = open_tab(cx, source, &repo, InnerTab::Files);
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "files_mode") == "diff" && !report_value(tab, cx, "threads_notice").is_empty()
            })
        });
        tab.read_with(cx, |tab, cx| {
            assert!(
                report_value(tab, cx, "threads_notice").starts_with("Review threads could not be read"),
                "{}",
                report_value(tab, cx, "threads_notice")
            );
        });
        forge.answer("ChangeRequestThreads", threads_json(vec![thread_node("PRRT_1", 42)]));
        tab.update(cx, |tab, cx| tab.retry_threads(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "threads_notice").is_empty() && report_value(tab, cx, "threads_open") == "1"
            })
        });
    }

    /// An unresolved card folds from its header like a resolved one: a header
    /// that takes the click and changes nothing would be a dead control.
    #[gpui::test]
    async fn an_open_thread_folds_from_its_header(cx: &mut TestAppContext) {
        let (repo, base, head) = range_repo();
        let forge = forge_for(&base, &head);
        forge.answer("ChangeRequestThreads", threads_json(vec![thread_node("PRRT_1", 42)]));
        let source = FakeSource::ready(testing::github_client(forge), None);
        let tab = open_tab(cx, source, &repo, InnerTab::Files);
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "files_mode") == "diff" && report_value(tab, cx, "threads_open") == "1"
            })
        });
        tab.update(cx, |tab, cx| tab.reveal_thread("PRRT_1", cx)).expect("a loaded thread");
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| report_value(tab, cx, "thread_rows") == "PRRT_1:line:open")
        });
        tab.update(cx, |tab, cx| tab.toggle_thread("PRRT_1", cx)).expect("a loaded thread");
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| report_value(tab, cx, "thread_rows") == "PRRT_1:line:folded")
        });
    }

    /// The action context `act` reads before writing: the pull request's id,
    /// its head and the permission to comment.
    fn action_context_json(head: &str) -> String {
        serde_json::json!({"data": {"repository": {
            "viewerPermission": "WRITE", "mergeCommitAllowed": true, "squashMergeAllowed": true,
            "rebaseMergeAllowed": false, "autoMergeAllowed": false, "deleteBranchOnMerge": false,
            "pullRequest": {
                "id": "PR_1", "state": "OPEN", "isDraft": false, "locked": false,
                "headRefOid": head, "headRefName": "feat", "isCrossRepository": false,
                "viewerCanUpdate": true, "viewerCanClose": true, "viewerCanReopen": false,
                "viewerDidAuthor": false, "viewerCanEnableAutoMerge": false,
                "mergeStateStatus": "CLEAN", "reviewDecision": null, "autoMergeRequest": null,
                "commits": {"nodes": []}, "reviewRequests": {"nodes": []}
            }}}}).to_string()
    }

    const COMMENTS: &str = "POST repos/acme/widgets/pulls/101/comments";

    async fn composing(
        cx: &mut TestAppContext,
    ) -> (Entity<ChangeRequestTab>, Arc<CannedForge>, RepoDir, String) {
        let (repo, base, head) = range_repo();
        let forge = forge_for(&base, &head);
        forge.answer("ChangeRequestThreads", threads_json(vec![]));
        forge.answer("ChangeRequestActionContext", action_context_json(&head));
        let source = FakeSource::ready(testing::github_client(forge.clone()), None);
        let tab = open_tab(cx, source, &repo, InnerTab::Files);
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "files_mode") == "diff"
                    && report_value(tab, cx, "commentable") == "yes"
            })
        });
        let changes = ready_changes(&tab, cx);
        changes.update(cx, |changes, cx| changes.focus_path(Path::new("a.txt"), cx));
        pump_until(cx, || changes.read_with(cx, |changes, _| {
            changes.comment_anchor(Path::new("a.txt"), AnnotationSide::New, 43, None).is_ok()
        }));
        (tab, forge, repo, head)
    }

    #[gpui::test]
    async fn a_line_comment_is_sent_at_its_line_and_the_threads_are_read_again(cx: &mut TestAppContext) {
        let (tab, forge, _repo, head) = composing(cx).await;
        tab.update(cx, |tab, cx| tab.compose_at("a.txt:new:43", cx)).expect("a line near the change");
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "thread_rows").contains("composer:line")
            })
        });
        let reads = forge.count("ChangeRequestThreads");
        forge.answer_rest(COMMENTS, 201, r#"{"id":1}"#);
        tab.update(cx, |tab, cx| tab.composer_set_text("Is this \"ok\"? naïve ☕\n- yes", cx));
        tab.update(cx, |tab, cx| tab.send_line_comment(cx)).expect("sent");
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "action") == "idle"
                    && report_value(tab, cx, "line_composer").is_empty()
            })
        });
        let sent = forge.rest_body(COMMENTS).expect("the comment reached the forge");
        assert_eq!(forge.rest_count(COMMENTS), 1);
        assert_eq!(
            sent,
            serde_json::json!({
                "body": "Is this \"ok\"? naïve ☕\n- yes",
                "commit_id": head,
                "path": "a.txt",
                "line": 43,
                "side": "RIGHT"
            })
        );
        pump_until(cx, || forge.count("ChangeRequestThreads") > reads);
    }

    #[gpui::test]
    async fn a_line_composer_refuses_a_new_head_and_keeps_its_text(cx: &mut TestAppContext) {
        let (tab, forge, repo, _head) = composing(cx).await;
        tab.update(cx, |tab, cx| tab.compose_at("a.txt:new:43", cx)).expect("a line near the change");
        tab.update(cx, |tab, cx| tab.composer_set_text("Keep this comment", cx));

        let newer = push_another(&repo.0);
        forge.answer(
            "ChangeRequestHeader",
            testing::header_with_revisions(
                101,
                "Fix the login redirect",
                "## What",
                &git(&repo.0, &["rev-parse", "main"]),
                &newer,
            ),
        );
        forge.answer("ChangeRequestActionContext", action_context_json(&newer));
        tab.update(cx, |tab, cx| tab.refresh(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                matches!(&tab.range, RangeState::Ready { revisions, .. } if revisions.head_sha == newer)
            })
        });

        tab.update(cx, |tab, cx| tab.send_line_comment(cx)).expect("the refusal arrives asynchronously");
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "action") == "failed"));
        tab.read_with(cx, |tab, cx| {
            assert_eq!(report_value(tab, cx, "line_composer"), "a.txt:new:43");
            assert_eq!(report_value(tab, cx, "line_composer_len"), "17");
            assert_eq!(
                report_value(tab, cx, "line_composer_error"),
                "The branch changed since you opened this. Reload to review the new commits."
            );
        });
        assert_eq!(forge.rest_count(COMMENTS), 0);
    }

    #[gpui::test]
    async fn a_refused_position_keeps_the_text_and_says_why(cx: &mut TestAppContext) {
        let (tab, forge, _repo, _head) = composing(cx).await;
        let header_reads = forge.count("ChangeRequestHeader");
        let thread_reads = forge.count("ChangeRequestThreads");
        forge.answer("ChangeRequestThreads", threads_json(vec![thread_node("PRRT_1", 42)]));
        forge.answer_rest(
            COMMENTS,
            422,
            r#"{"message":"Unprocessable Entity","errors":["pull_request_review_thread.line must be part of the diff"]}"#,
        );
        tab.update(cx, |tab, cx| tab.compose_at("a.txt:new:41-43", cx)).expect("a range near the change");
        tab.update(cx, |tab, cx| tab.composer_set_text("Range", cx));
        tab.update(cx, |tab, cx| tab.send_line_comment(cx)).expect("sent");
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "action") == "failed"));
        tab.read_with(cx, |tab, cx| {
            assert_eq!(report_value(tab, cx, "line_composer"), "a.txt:new:41-43");
            assert_eq!(report_value(tab, cx, "line_composer_len"), "5");
            assert_eq!(
                report_value(tab, cx, "line_composer_error"),
                "pull_request_review_thread.line must be part of the diff"
            );
        });
        pump_until(cx, || {
            forge.count("ChangeRequestHeader") > header_reads
                && forge.count("ChangeRequestThreads") > thread_reads
                && tab.read_with(cx, |tab, cx| {
                    report_value(tab, cx, "thread_rows").contains("PRRT_1:line:")
                })
        });
        tab.read_with(cx, |tab, cx| {
            assert_eq!(report_value(tab, cx, "line_composer"), "a.txt:new:41-43");
            assert_eq!(report_value(tab, cx, "line_composer_len"), "5");
            assert_eq!(
                report_value(tab, cx, "line_composer_error"),
                "pull_request_review_thread.line must be part of the diff"
            );
        });
    }

    #[gpui::test]
    async fn a_rate_paused_send_shows_its_refusal_under_the_composer(cx: &mut TestAppContext) {
        let (tab, forge, _repo, _head) = composing(cx).await;
        tab.update(cx, |tab, cx| tab.compose_at("a.txt:new:43", cx)).expect("a line near the change");
        tab.update(cx, |tab, cx| tab.composer_set_text("Keep this draft", cx));
        tab.update(cx, |tab, cx| {
            tab.write_target = Some(compose::WriteTarget::Composer);
            tab.actions.state = actions::ActionState::Failed {
                kind: "line-comment",
                message: "an earlier failure".to_string(),
            };
            tab.paused_until = Some(style::now() + 3600);
            assert_eq!(
                tab.send_line_comment(cx),
                Err("the forge asked Sirio to wait; try again after its reset".to_string())
            );
        });
        tab.read_with(cx, |tab, cx| {
            assert_eq!(report_value(tab, cx, "line_composer_len"), "15");
            assert_eq!(
                report_value(tab, cx, "line_composer_error"),
                "the forge asked Sirio to wait; try again after its reset"
            );
            assert_eq!(report_value(tab, cx, "action"), "failed");
        });
        assert_eq!(forge.rest_count(COMMENTS), 0);
    }

    #[gpui::test]
    async fn a_busy_refusal_is_shown_without_replacing_the_in_flight_write(cx: &mut TestAppContext) {
        let (tab, forge, _repo, _head) = composing(cx).await;
        forge.answer_rest(COMMENTS, 201, r#"{"id":1}"#);
        tab.update(cx, |tab, cx| tab.compose_at("a.txt:new:43", cx)).expect("a line near the change");
        tab.update(cx, |tab, cx| tab.composer_set_text("Keep this draft", cx));
        tab.update(cx, |tab, cx| {
            tab.send_line_comment(cx).expect("the first write is accepted");
            assert!(matches!(tab.actions.state, actions::ActionState::Working("line-comment")));
            let target = tab.write_target.clone();

            assert_eq!(
                tab.send_line_comment(cx),
                Err("an action is already in flight".to_string())
            );
            assert!(matches!(tab.actions.state, actions::ActionState::Working("line-comment")));
            assert_eq!(tab.write_target, target);
        });
        tab.read_with(cx, |tab, cx| {
            assert_eq!(report_value(tab, cx, "line_composer_len"), "15");
            assert_eq!(report_value(tab, cx, "line_composer_error"), "an action is already in flight");
        });
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "action") == "idle"
                    && report_value(tab, cx, "line_composer").is_empty()
            })
        });
        assert_eq!(forge.rest_count(COMMENTS), 1, "the refused second send was not posted");
    }

    #[gpui::test]
    async fn a_close_failure_is_not_shown_under_a_stale_line_composer(cx: &mut TestAppContext) {
        let (tab, forge, _repo, _head) = composing(cx).await;
        forge.answer_rest(COMMENTS, 422, r#"{"message":"line rejected"}"#);
        tab.update(cx, |tab, cx| tab.compose_at("a.txt:new:43", cx)).expect("a line near the change");
        tab.update(cx, |tab, cx| tab.composer_set_text("Keep this draft", cx));
        tab.update(cx, |tab, cx| tab.send_line_comment(cx)).expect("the comment is sent");
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| report_value(tab, cx, "action") == "failed")
        });

        forge.fail("ClosePullRequest", 422);
        tab.update(cx, |tab, cx| tab.perform(Action::Close, cx)).expect("Close is accepted");
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                matches!(tab.actions.state, actions::ActionState::Failed { kind: "close", .. })
            })
        });
        tab.read_with(cx, |tab, cx| {
            assert_eq!(report_value(tab, cx, "line_composer"), "a.txt:new:43");
            assert_eq!(report_value(tab, cx, "line_composer_len"), "15");
            assert_eq!(report_value(tab, cx, "line_composer_error"), "");
            assert_eq!(report_value(tab, cx, "action_message"), "failed");
        });
    }

    #[gpui::test]
    async fn a_rate_limited_failed_write_keeps_the_reread_guard(cx: &mut TestAppContext) {
        let (tab, forge, _repo, _head) = composing(cx).await;
        let header_reads = forge.count("ChangeRequestHeader");
        let thread_reads = forge.count("ChangeRequestThreads");
        forge.rate_limited("ClosePullRequest", style::now() + 3600);
        tab.update(cx, |tab, cx| tab.perform(Action::Close, cx)).expect("Close is accepted");
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                matches!(tab.actions.state, actions::ActionState::Failed { kind: "close", .. })
            })
        });
        assert_eq!(forge.count("ChangeRequestHeader"), header_reads);
        assert_eq!(forge.count("ChangeRequestThreads"), thread_reads);
    }

    #[gpui::test]
    async fn the_open_composer_keeps_its_text_when_the_threads_are_read_again(cx: &mut TestAppContext) {
        let (tab, forge, _repo, _head) = composing(cx).await;
        tab.update(cx, |tab, cx| tab.compose_at("a.txt:old:42", cx)).expect("the removed line");
        tab.update(cx, |tab, cx| tab.composer_set_text("Kept", cx));
        let reads = forge.count("ChangeRequestThreads");
        tab.update(cx, |tab, cx| tab.load_threads(cx));
        pump_until(cx, || {
            forge.count("ChangeRequestThreads") > reads
                && tab.read_with(cx, |tab, cx| report_value(tab, cx, "threads_notice").is_empty())
        });
        tab.read_with(cx, |tab, cx| {
            assert_eq!(report_value(tab, cx, "line_composer"), "a.txt:old:42");
            assert_eq!(report_value(tab, cx, "line_composer_len"), "4");
            assert!(report_value(tab, cx, "thread_rows").contains("composer:line"));
        });
    }

    #[gpui::test]
    async fn a_gutter_pick_opens_the_composer_and_a_refusal_says_why(cx: &mut TestAppContext) {
        let (tab, _forge, _repo, _head) = composing(cx).await;
        let changes = ready_changes(&tab, cx);
        let anchor = changes
            .read_with(cx, |changes, _| {
                changes.comment_anchor(Path::new("a.txt"), AnnotationSide::New, 44, None)
            })
            .expect("a line near the change");
        changes.update(cx, |_, cx| cx.emit(ChangesTabEvent::CommentOn(anchor)));
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "line_composer") == "a.txt:new:44"));
        changes.update(cx, |_, cx| {
            cx.emit(ChangesTabEvent::CommentRefused(
                "A range stays on one side of the diff.".into(),
            ))
        });
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "action_message") == "A range stays on one side of the diff."
            })
        });
        tab.update(cx, |tab, cx| tab.cancel_composer(cx));
        assert!(tab.read_with(cx, |tab, cx| report_value(tab, cx, "line_composer").is_empty()));
    }

    #[gpui::test]
    async fn a_line_too_far_from_the_change_is_refused_at_compose(cx: &mut TestAppContext) {
        let (tab, _forge, _repo, _head) = composing(cx).await;
        assert_eq!(
            tab.update(cx, |tab, cx| tab.compose_at("a.txt:new:30", cx)),
            Err("That line cannot take a comment.".to_string())
        );
        assert_eq!(
            tab.update(cx, |tab, cx| tab.compose_at("a.txt:sideways:3", cx)),
            Err("compose needs PATH:old|new:LINE or PATH:old|new:FIRST-LAST".to_string())
        );
    }

    #[gpui::test]
    async fn a_reveal_during_a_new_heads_fetch_keeps_the_opened_files(cx: &mut TestAppContext) {
        let (repo, base, head) = range_repo();
        let forge = forge_for(&base, &head);
        let source = FakeSource::ready(testing::github_client(forge.clone()), None);
        let tab = open_tab(cx, source, &repo, InnerTab::Files);
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "files_mode") == "diff"));
        let changes = ready_changes(&tab, cx);
        changes.update(cx, |changes, cx| {
            changes.focus_path(Path::new("a.txt"), cx);
            changes.focus_path(Path::new("c.txt"), cx);
        });
        pump_until(cx, || {
            changes.read_with(cx, |changes, _| {
                changes.expanded_paths() == vec![PathBuf::from("a.txt"), PathBuf::from("c.txt")]
            })
        });

        // A line-comment click lands while the new head's revisions are
        // fetching: it is made from the notify that starts the fetch, before
        // the fetch can finish.
        let revealed = Rc::new(RefCell::new(false));
        let flag = revealed.clone();
        cx.update(|cx| {
            cx.observe(&tab, move |tab, cx| {
                let fetching = matches!(tab.read(cx).range, RangeState::Fetching);
                if fetching && !*flag.borrow() {
                    *flag.borrow_mut() = true;
                    tab.update(cx, |tab, cx| tab.reveal(PathBuf::from("d.txt"), Some(1), cx));
                }
            })
            .detach();
        });
        let newer = push_another(&repo.0);
        forge.answer(
            "ChangeRequestHeader",
            testing::header_with_revisions(101, "Fix the login redirect", "## What", &base, &newer),
        );
        tab.update(cx, |tab, cx| tab.refresh(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "head") == newer[..7] && report_value(tab, cx, "files_mode") == "diff"
            })
        });
        assert!(*revealed.borrow(), "the reveal was made while the fetch ran");
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| match &tab.range {
                RangeState::Ready { changes, .. } => {
                    changes.read(cx).expanded_paths()
                        == vec![PathBuf::from("a.txt"), PathBuf::from("c.txt"), PathBuf::from("d.txt")]
                }
                _ => false,
            })
        });
    }

    #[test]
    fn a_fetch_that_could_not_sign_in_gets_the_credentials_hint_and_nothing_else_does() {
        for detail in [
            "fatal: could not read Username for 'https://ghe.test': terminal prompts disabled",
            "remote: Repository not found.\nfatal: Authentication failed for 'https://x/'\nterminal prompts disabled",
            "git@ghe.test: Permission denied (publickey).\nfatal: Could not read from remote repository.",
        ] {
            let hint = error_hint(&RevisionError::FetchFailed { detail: detail.to_string() })
                .unwrap_or_else(|| panic!("{detail:?} is a sign-in failure"));
            assert!(hint.contains("git's own credentials") && hint.contains("gh auth setup-git"), "{hint}");
        }
        for error in [
            RevisionError::FetchFailed { detail: "fatal: couldn't find remote ref refs/pull/9/head".to_string() },
            RevisionError::FetchTimedOut,
            RevisionError::NoMatchingRemote { expected: "ghe.test/acme/widgets".to_string() },
            RevisionError::RevisionGone { sha: "0123456".to_string() },
            RevisionError::Git { detail: "terminal prompts disabled".to_string() },
        ] {
            assert_eq!(error_hint(&error), None, "{error:?} is not a sign-in failure");
        }
    }

    /// The header moves (a new push) while Files is still fetching the old
    /// revisions: what completes is not what Files should show. The header
    /// is landed through `apply_header` from the notify that starts the
    /// fetch, because the test scheduler orders the two background tasks by
    /// its seed and cannot be told which finishes first.
    #[gpui::test]
    async fn a_header_that_moves_while_files_is_fetching_ends_on_the_new_head(cx: &mut TestAppContext) {
        let (repo, base, head) = range_repo();
        let forge = forge_for(&base, &head);
        let client = testing::github_client(forge.clone());
        let source = FakeSource::ready(client.clone(), None);
        let tab = open_tab(cx, source.clone(), &repo, InnerTab::Conversation);
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "state") == "loaded"));
        let newer = push_another(&repo.0);

        let moved = Rc::new(RefCell::new(false));
        let flag = moved.clone();
        let (base_now, newer_now, forge_now) = (base.clone(), newer.clone(), forge.clone());
        cx.update(|cx| {
            cx.observe(&tab, move |tab, cx| {
                let fetching = matches!(tab.read(cx).range, RangeState::Fetching);
                if fetching && !*flag.borrow() {
                    *flag.borrow_mut() = true;
                    forge_now.answer(
                        "ChangeRequestHeader",
                        testing::header_with_revisions(101, "Fix the login redirect", "## What", &base_now, &newer_now),
                    );
                    let header = client.header(101);
                    tab.update(cx, |tab, cx| tab.apply_header(header, cx));
                }
            })
            .detach();
        });
        tab.update(cx, |tab, cx| tab.select_inner(InnerTab::Files, cx));
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "files_mode") != "fetching"));
        assert!(*moved.borrow(), "the header landed while the fetch ran");
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "head") == newer[..7] && report_value(tab, cx, "files_mode") == "diff"
            })
        });
        let ensured: Vec<String> = source.ensured.lock().unwrap().iter().map(|(_, r, _)| r.head_sha.clone()).collect();
        assert_eq!(ensured, vec![head.clone(), newer.clone()], "the old head's fetch, then the new one's");
    }

    #[gpui::test]
    async fn a_line_comment_link_reveals_its_file_in_files(cx: &mut TestAppContext) {
        let (repo, base, head) = range_repo();
        let source = FakeSource::ready(testing::github_client(forge_for(&base, &head)), None);
        let tab = open_tab(cx, source, &repo, InnerTab::Conversation);
        // Before the header, hence the range, has loaded: the request waits.
        tab.update(cx, |tab, cx| tab.reveal(PathBuf::from("a.txt"), Some(42), cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| {
                report_value(tab, cx, "inner") == "files" && report_value(tab, cx, "files_focus") == "a.txt"
            })
        });
    }

    #[gpui::test]
    async fn opening_a_file_from_the_diff_asks_the_host_with_the_revisions(cx: &mut TestAppContext) {
        let (repo, base, head) = range_repo();
        let source = FakeSource::ready(testing::github_client(forge_for(&base, &head)), None);
        let tab = open_tab(cx, source, &repo, InnerTab::Files);
        let events: Rc<RefCell<Vec<ChangeRequestTabEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|cx| {
            cx.subscribe(&tab, move |_, event: &ChangeRequestTabEvent, _| {
                // The header's title arrives as an event too; only the opens matter here.
                if matches!(event, ChangeRequestTabEvent::OpenFile { .. }) {
                    sink.borrow_mut().push(event.clone())
                }
            })
            .detach();
        });
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "diff_files") == "3"));
        let changes = ready_changes(&tab, cx);
        // The same event the `↗` button of an expanded row emits.
        changes.update(cx, |_, cx| cx.emit(ChangesTabEvent::OpenFile(repo.0.join("a.txt"))));
        changes.update(cx, |_, cx| cx.emit(ChangesTabEvent::OpenFile(repo.0.join("gone.txt"))));
        cx.run_until_parked();
        let seen = events.borrow().clone();
        assert!(
            matches!(&seen[0], ChangeRequestTabEvent::OpenFile { path, line: None, deleted: false, revisions }
                if path == Path::new("a.txt") && revisions.head_sha == head),
            "{seen:?}"
        );
        assert!(
            matches!(&seen[1], ChangeRequestTabEvent::OpenFile { path, deleted: true, .. }
                if path == Path::new("gone.txt")),
            "{seen:?}"
        );
    }

    #[gpui::test]
    async fn a_commit_click_fetches_first_and_a_failed_fetch_offers_the_forge(cx: &mut TestAppContext) {
        let (repo, base, head) = range_repo();
        let source = FakeSource::ready(testing::github_client(forge_for(&base, &head)), None);
        let tab = open_tab(cx, source.clone(), &repo, InnerTab::Conversation);
        pump_until(cx, || tab.read_with(cx, |tab, cx| report_value(tab, cx, "state") == "loaded"));
        let events: Rc<RefCell<Vec<ChangeRequestTabEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|cx| {
            cx.subscribe(&tab, move |_, event: &ChangeRequestTabEvent, _| {
                sink.borrow_mut().push(event.clone())
            })
            .detach();
        });
        let sha = "1111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
        let url = "https://ghe.test/acme/widgets/commit/1111111".to_string();

        tab.update(cx, |tab, cx| tab.open_commit(sha.clone(), url.clone(), cx));
        pump_until(cx, || !events.borrow().is_empty());
        assert_eq!(
            events.borrow()[0],
            ChangeRequestTabEvent::OpenCommit { sha: sha.clone(), web_url: url.clone() }
        );
        assert_eq!(source.ensured.lock().unwrap().len(), 1, "the commit's revisions were made local first");

        events.borrow_mut().clear();
        *source.revisions_answer.lock().unwrap() =
            Err(RevisionError::FetchFailed { detail: "boom".to_string() });
        tab.update(cx, |tab, cx| tab.open_commit(sha, url, cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, cx| report_value(tab, cx, "commit_error").contains("boom"))
        });
        assert!(events.borrow().is_empty(), "a commit that could not be fetched is not opened");
    }
    fn job(name: &str, status: CheckStatus, group: Option<&str>, job: Option<(u64, u64, bool)>) -> Check {
        Check {
            name: name.to_string(),
            status,
            group: group.map(str::to_string),
            duration_secs: None,
            url: Some(format!("https://forge.test/{name}")),
            job: job.map(|(job_id, run_id, retryable)| CheckJob { job_id, run_id: Some(run_id), retryable }),
        }
    }

    #[test]
    fn checks_group_by_run_failed_first_and_the_rest_together() {
        let items = vec![
            job("build", CheckStatus::Passed, Some("CI"), Some((1, 1, true))),
            job("test", CheckStatus::Failed, Some("CI"), Some((2, 1, true))),
            job("lint", CheckStatus::Running, Some("Lint"), Some((3, 4, false))),
            job("deploy/preview", CheckStatus::Running, None, None),
            job("docs", CheckStatus::Passed, Some("Docs"), Some((5, 6, true))),
        ];
        let (pipeline, groups) = check_groups(&items, Forge::GitHub, true);
        assert!(pipeline.is_none());
        let titles: Vec<&str> = groups.iter().map(|group| group.title.as_str()).collect();
        // Worst first; the checks with no group last.
        assert_eq!(titles, ["CI", "Lint", "Docs", "Other checks"]);
        assert_eq!(groups[0].members, [1, 0]);
        assert!(groups[0].open_by_default && groups[1].open_by_default && !groups[2].open_by_default);
        assert_eq!(groups[0].rerun_failed, Some(1));
        assert_eq!(groups[1].rerun_failed, None);
        assert_eq!(groups[3].members, [3]);
    }

    #[test]
    fn the_same_workflow_in_two_runs_is_two_groups() {
        let items = vec![
            job("a", CheckStatus::Failed, Some("CI"), Some((1, 1, true))),
            job("b", CheckStatus::Failed, Some("CI"), Some((2, 9, true))),
        ];
        let (_, groups) = check_groups(&items, Forge::GitHub, true);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn rerunning_failed_needs_the_permission_and_a_retryable_failure() {
        let items = vec![job("test", CheckStatus::Failed, Some("CI"), Some((2, 1, false)))];
        assert_eq!(check_groups(&items, Forge::GitHub, true).1[0].rerun_failed, None);
        let items = vec![job("test", CheckStatus::Canceled, Some("CI"), Some((2, 1, true)))];
        assert_eq!(check_groups(&items, Forge::GitHub, true).1[0].rerun_failed, Some(1));
        assert_eq!(check_groups(&items, Forge::GitHub, false).1[0].rerun_failed, None);
    }

    #[test]
    fn gitlab_reruns_failed_jobs_per_pipeline_not_per_stage() {
        let items = vec![
            job("build", CheckStatus::Passed, Some("build"), Some((1, 45, true))),
            job("rspec", CheckStatus::Failed, Some("test"), Some((2, 45, true))),
            job("lint", CheckStatus::Running, Some("test"), Some((3, 45, false))),
        ];
        let (pipeline, groups) = check_groups(&items, Forge::GitLab, true);
        let pipeline = pipeline.expect("a pipeline row");
        assert_eq!((pipeline.title.as_str(), pipeline.rerun_failed), ("Pipeline #45", Some(45)));
        assert!(groups.iter().all(|group| group.rerun_failed.is_none()));
        assert_eq!(groups.iter().map(|group| group.title.as_str()).collect::<Vec<_>>(), ["test", "build"]);
        // Nothing failed: still the row, with no button.
        let calm = vec![job("build", CheckStatus::Passed, Some("build"), Some((1, 45, true)))];
        assert_eq!(check_groups(&calm, Forge::GitLab, true).0.map(|row| row.rerun_failed), Some(None));
        // No job carries a pipeline (a baseline answer): no row.
        let bare = vec![job("x", CheckStatus::Failed, Some("test"), None)];
        assert!(check_groups(&bare, Forge::GitLab, true).0.is_none());
    }

    #[test]
    fn queued_only_check_groups_start_folded() {
        let items = vec![job("test", CheckStatus::Queued, Some("CI"), Some((2, 1, false)))];
        assert!(!check_groups(&items, Forge::GitHub, true).1[0].open_by_default);
    }

    #[test]
    fn rerun_refusals_name_the_workflow_write_permission() {
        let error = ForgeError::Forbidden {
            host: "forge.test".to_string(),
            detail: "permission denied".to_string(),
            sso_url: None,
        };
        for forge in [Forge::GitHub, Forge::GitLab] {
            for kind in ["rerun-job", "rerun-failed"] {
                assert_eq!(
                    actions::action_error_text(&error, forge, kind),
                    "This token cannot re-run workflows: it needs Actions: write (fine-grained) or repo (classic) on GitHub, api on GitLab."
                );
            }
            assert!(actions::action_error_text(&error, forge, "close").contains("Settings → Git Hosting"));
        }
    }

    #[test]
    fn check_group_keys_do_not_collide_with_names_or_other_checks() {
        let items = vec![
            job("a", CheckStatus::Failed, Some("CI"), Some((1, 1, true))),
            job("b", CheckStatus::Failed, Some("CI#1"), None),
        ];
        let (_, groups) = check_groups(&items, Forge::GitHub, true);
        assert_eq!(groups.len(), 2, "a name must not alias another workflow's run key");
        assert_ne!(groups[0].key, groups[1].key);

        let items = vec![
            job("a", CheckStatus::Failed, Some("other"), Some((1, 45, true))),
            job("b", CheckStatus::Running, Some("test"), Some((2, 45, false))),
            job("c", CheckStatus::Passed, None, None),
        ];
        let (_, groups) = check_groups(&items, Forge::GitLab, true);
        assert_eq!(groups.iter().map(|group| group.title.as_str()).collect::<Vec<_>>(), ["other", "test", "Other checks"]);
        assert_ne!(groups[0].key, groups[2].key, "disclosures must have separate overrides");
    }

}
