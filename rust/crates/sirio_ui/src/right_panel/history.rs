//! The History view: `git log` over local refs, laid out as a commit graph.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::PathBuf;
use std::time::Duration;

use chrono::TimeZone;
use gpui::{
    AnyElement, AppContext as _, Context, Corners, EventEmitter, FocusHandle,
    InteractiveElement as _, IntoElement, ParentElement as _, Path, Render,
    StatefulInteractiveElement as _, Styled as _, Task, Window, canvas, div, fill, point,
    prelude::FluentBuilder as _, px, uniform_list,
};
use sirio_git::{
    CommitRecord, GitBranches, GitLog, GraphRow, LayoutCursor, LogFilter, extend_layout, layout,
};
use sirio_theme::Theme;

use super::history_toolbar;
use crate::loading;

/// Commits requested per chunk.
const CHUNK: usize = 500;
/// How long the field sits still before the query runs. Every keystroke
/// would otherwise start a full walk of every reachable commit: `--grep` is
/// O(walk), not O(page).
pub(crate) const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
/// Height of one commit row.
const ROW_HEIGHT: f32 = 26.0;
/// Height of one commit row while a text filter is active.
///
/// A row that matched on body text carries a second line explaining why, and
/// `uniform_list` gives every row in the list the same height — so the whole
/// list grows for the duration of the search rather than only the annotated
/// rows, which it has no way to make taller on their own. Without this the
/// annotation drew straight over the next row's subject.
const ROW_HEIGHT_ANNOTATED: f32 = 42.0;
// The three widths below were measured off the running app at
// `Typography::footnote` (13px), not estimated: `conformance.rs` records
// that gpui's headless text system reports the same advance for every
// character of every family, so a test cannot be the ruler here and a
// screenshot has to be. Measured 2026-08-23: "e.palmisano" 52px,
// "2026-08-23" 63px. Lowercase runs about 4.7px per character and digits
// about 6.3 — a third apart, which is why there is no single "character
// width" constant to divide by.
//
/// Author column. A budget, not a fit: names have no bound, so the column
/// truncates by design. Holds the 52px name this repository writes with
/// half as much again for a longer one.
const AUTHOR_WIDTH: f32 = 78.0;
/// Date column. A fit, not a budget: `%Y-%m-%d` is always the same ten
/// characters and can never grow past this.
///
/// 71px held the 63px ink the Linux screenshot measured, but the running
/// Windows app (#366) cut the last digit (`2026-09-0`) and the headless
/// stub already needs 78px for ten characters — so 71 left no room for a
/// wider face or side bearings. 80px fits both with room to spare.
const DATE_WIDTH: f32 = 80.0;
/// Horizontal gap between a row's columns.
const ROW_GAP: f32 = 6.0;
/// What a commit row loses to chrome it does not control: the panel's two
/// 1px borders (`shell_chrome::panel`) plus the row's own `px(8.0)` padding
/// on each side. `panel_width` counts both, so the row's own arithmetic has
/// to take them off again.
const ROW_CHROME: f32 = 18.0;
/// Narrowest a subject may get before a trailing column is dropped to feed
/// it: about twenty-seven characters at the measured advance above — far
/// enough past a conventional-commit prefix to reach the message, which is
/// where two `fix(sidebar):` commits start to differ.
const MIN_SUBJECT_WIDTH: f32 = 130.0;

/// Membership toggle that keeps the vector a set: `LogFilter` treats a
/// repeated value as a repeated git argument, and git would then OR a term
/// with itself.
fn toggle_in(values: &mut Vec<String>, value: String) {
    if let Some(index) = values.iter().position(|existing| *existing == value) {
        values.remove(index);
    } else {
        values.push(value);
    }
}

/// Why the list is empty, when it is empty for a reason worth naming.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EmptyReason {
    NotARepository,
    NoCommits,
    /// A filter is active and matched nothing. Distinct from `NoCommits`:
    /// the repository is not empty, the query is.
    NoMatches,
}

/// Emitted upward when a commit row is clicked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GitHistoryEvent {
    OpenCommit(String),
}

pub(crate) struct GitHistory {
    repo_root: PathBuf,
    /// The live query. Changing it resets everything below — see
    /// [`GitHistory::set_filter`].
    pub(crate) filter: LogFilter,
    /// What is typed in the field right now, which is not yet what is
    /// queried — `filter.text` is. The two differ for `SEARCH_DEBOUNCE`.
    pub(crate) search_draft: String,
    pub(crate) search_regex: bool,
    pub(crate) search_case_sensitive: bool,
    pub(crate) search_blink: crate::caret::Blink,
    /// Whether the last drawn frame lit the search field's bar. Kept as its
    /// own field, rather than read straight off `search_blink`, because
    /// `caret::schedule` parks an unfocused surface's blink at *visible* —
    /// so the blink phase alone says nothing about whether a bar belongs on
    /// screen. Focus × phase does.
    pub(crate) search_caret_visible: bool,
    /// The pathspec being typed. Free text rather than a directory picker: a
    /// pathspec is more expressive than a picker, and git validates it.
    pub(crate) path_draft: String,
    /// The Paths popup's own caret state — a separate surface from the
    /// search field, so it owns a separate blink and a separate timer.
    pub(crate) path_blink: crate::caret::Blink,
    pub(crate) path_caret_visible: bool,
    /// The panel's current width, pushed in by the host each render. The
    /// view has no way to measure its own container, and guessing from the
    /// last drawn frame would lag a frame behind every drag.
    pub(crate) panel_width: f32,
    search_task: Option<Task<()>>,
    /// Bumped per scheduled search; a timer that wakes stale does nothing.
    search_generation: u64,
    /// Created on first render, so the field can be focused before it ever
    /// exists without panicking on a missing handle.
    search_focus: Option<FocusHandle>,
    /// Same lifecycle as [`GitHistory::search_focus`], for the Paths popup's
    /// free-text row.
    path_focus: Option<FocusHandle>,
    /// Which dropdown is open, if any. One at a time: two popups at once
    /// would need a z-order and a dismissal rule neither of them earns.
    pub(crate) open_chip: Option<history_toolbar::FilterChip>,
    /// Local branch names, read once per repository and refreshed with the
    /// tree. Empty until the first read returns.
    pub(crate) branch_options: Vec<String>,
    /// Fetched commit bodies for rows that matched on body text alone, keyed
    /// by sha. Filled lazily, one body per visible row.
    bodies: HashMap<String, String>,
    /// Shas whose body fetch is still in flight, so a scroll does not queue
    /// the same fetch twice.
    bodies_fetching: HashSet<String>,
    pub(crate) commits: Vec<CommitRecord>,
    pub(crate) rows: Vec<GraphRow>,
    layout_cursor: Option<LayoutCursor>,
    pub(crate) error: Option<String>,
    /// A follow-up chunk that failed. Kept apart from `error` so the commits
    /// already on screen are not thrown away for it.
    pub(crate) pagination_error: Option<String>,
    pub(crate) empty_reason: Option<EmptyReason>,
    /// Whether the first load has finished, successfully or not.
    pub(crate) settled: bool,
    load_task: Option<Task<()>>,
    /// Bumped on every load request; a stale result is discarded.
    generation: u64,
    exhausted: bool,
}

impl GitHistory {
    pub(crate) fn new(repo_root: PathBuf, cx: &mut Context<Self>) -> Self {
        let mut history = Self {
            repo_root,
            filter: LogFilter::default(),
            search_draft: String::new(),
            search_regex: false,
            search_case_sensitive: false,
            search_blink: crate::caret::Blink::new(),
            search_caret_visible: false,
            path_draft: String::new(),
            path_blink: crate::caret::Blink::new(),
            path_caret_visible: false,
            panel_width: 405.0,
            search_task: None,
            search_generation: 0,
            search_focus: None,
            path_focus: None,
            open_chip: None,
            branch_options: Vec::new(),
            bodies: HashMap::new(),
            bodies_fetching: HashSet::new(),
            commits: Vec::new(),
            rows: Vec::new(),
            layout_cursor: None,
            error: None,
            pagination_error: None,
            empty_reason: None,
            settled: false,
            load_task: None,
            generation: 0,
            exhausted: false,
        };
        history.load_next_chunk(cx);
        history
    }

    /// Requests the next chunk. Single-flight: a request while one is in
    /// flight is dropped, not queued.
    /// Whether a history chunk is currently being fetched. The toolbar uses
    /// this to show a compact indicator while settled rows remain visible.
    pub(super) fn is_loading(&self) -> bool {
        self.load_task.is_some()
    }

    pub(crate) fn load_next_chunk(&mut self, cx: &mut Context<Self>) {
        if self.load_task.is_some() || self.exhausted {
            return;
        }
        self.error = None;
        self.pagination_error = None;
        let repo_root = self.repo_root.clone();
        let filter = self.filter.clone();
        let skip = self.commits.len();
        self.generation += 1;
        let generation = self.generation;
        self.load_task = Some(cx.spawn(async move |this, cx| {
            // `has_commits` shells out to `git rev-parse`. It is resolved
            // here, beside the log read, and never in the `update` closure
            // below: that closure runs on the render thread, and a blocking
            // subprocess there stalls the frame.
            let loaded = cx
                .background_spawn(async move {
                    let commits = GitLog::commits(&repo_root, skip, CHUNK, &filter);
                    let has_commits = matches!(&commits, Ok(loaded) if loaded.is_empty())
                        .then(|| GitLog::has_commits(&repo_root));
                    // Branches shell out too, so they ride the same background
                    // task as the first chunk and never run on the render
                    // thread. `None` means this round was not the first chunk.
                    let branch_options =
                        (skip == 0).then(|| GitBranches::list(&repo_root).unwrap_or_default());
                    (commits, has_commits, branch_options)
                })
                .await;
            let (loaded, has_commits, branch_options) = loaded;
            let _ = this.update(cx, |this, cx| {
                this.load_task = None;
                this.settled = true;
                if generation != this.generation {
                    return;
                }
                if let Some(options) = branch_options {
                    this.branch_options = options;
                }
                match loaded {
                    Ok(commits) => {
                        if commits.len() < CHUNK {
                            this.exhausted = true;
                        }
                        let chunk = match this.layout_cursor.as_ref() {
                            Some(cursor) => extend_layout(cursor, &commits),
                            None => layout(&commits),
                        };
                        let cursor = chunk.cursor().clone();
                        this.commits.extend(commits);
                        this.rows.extend(chunk.rows);
                        this.layout_cursor = Some(cursor);
                        this.empty_reason = match (this.commits.is_empty(), has_commits) {
                            // Order matters: on a repository with commits *and*
                            // a filter, both arms could fire, and the filter is
                            // the one that explains the emptiness.
                            (true, _) if this.filter.is_filtering() => Some(EmptyReason::NoMatches),
                            (true, Some(false)) => Some(EmptyReason::NoCommits),
                            _ => None,
                        };
                        this.error = None;
                    }
                    Err(error) => {
                        if this.commits.is_empty() {
                            if this.repo_root.join(".git").exists() {
                                this.error = Some(error.to_string());
                            } else {
                                this.empty_reason = Some(EmptyReason::NotARepository);
                            }
                        } else {
                            this.pagination_error = Some(error.to_string());
                        }
                    }
                }
                cx.notify();
            });
        }));
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        self.exhausted = false;
        self.load_next_chunk(cx);
    }

    /// Adds or removes one value from a chip's selection and re-runs the
    /// query. Every chip writes a different `LogFilter` field, which is the
    /// only place their behaviour differs.
    pub(crate) fn toggle_chip_option(
        &mut self,
        chip: history_toolbar::FilterChip,
        value: String,
        cx: &mut Context<Self>,
    ) {
        let mut filter = self.filter.clone();
        match chip {
            history_toolbar::FilterChip::Branch => toggle_in(&mut filter.branches, value),
            history_toolbar::FilterChip::User => toggle_in(&mut filter.authors, value),
            // Date and Paths do not multi-select; Tasks 4 and 5 give them
            // their own entry points rather than bending this one.
            history_toolbar::FilterChip::Date | history_toolbar::FilterChip::Paths => return,
        }
        self.set_filter(filter, cx);
    }

    /// Distinct authors among the commits currently loaded, in first-seen
    /// order so the list does not reshuffle as more chunks arrive.
    ///
    /// Deliberately not exhaustive: `git log --format=%an --branches` over
    /// the whole repository would walk every reachable commit, which is the
    /// cost the person filtering is trying to avoid. The dropdown says so.
    pub(crate) fn author_options(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.commits
            .iter()
            .filter(|commit| seen.insert(commit.author.clone()))
            .map(|commit| commit.author.clone())
            .collect()
    }

    /// The search field's focus handle; created lazily on first render.
    pub(crate) fn search_focus_handle(&self) -> &FocusHandle {
        self.search_focus.as_ref().expect("just initialized")
    }

    /// The Paths popup's focus handle; created lazily on first render.
    pub(crate) fn path_focus_handle(&self) -> &FocusHandle {
        self.path_focus.as_ref().expect("just initialized")
    }

    /// Where each chip's option list comes from.
    pub(crate) fn options_for(&self, chip: history_toolbar::FilterChip) -> Vec<String> {
        match chip {
            history_toolbar::FilterChip::Branch => self.branch_options.clone(),
            history_toolbar::FilterChip::User => self.author_options(),
            // The Date popup renders its own fixed presets.
            history_toolbar::FilterChip::Date => Vec::new(),
            // The Paths popup renders the free-text draft row instead.
            history_toolbar::FilterChip::Paths => Vec::new(),
        }
    }

    /// What each chip currently has selected, in the shape its popup ticks.
    pub(crate) fn selection_for(&self, chip: history_toolbar::FilterChip) -> Vec<String> {
        match chip {
            history_toolbar::FilterChip::Branch => self.filter.branches.clone(),
            history_toolbar::FilterChip::User => self.filter.authors.clone(),
            // Single-select: the one bound, or nothing.
            history_toolbar::FilterChip::Date => self.filter.since.iter().cloned().collect(),
            history_toolbar::FilterChip::Paths => vec![self.path_draft.clone()],
        }
    }

    /// Sets or clears the `--since` bound. Date is single-select, unlike
    /// Branch and User: two lower bounds would mean nothing.
    pub(crate) fn set_date_preset(&mut self, since: Option<String>, cx: &mut Context<Self>) {
        let filter = LogFilter {
            since,
            ..self.filter.clone()
        };
        self.set_filter(filter, cx);
    }

    /// Applies the typed pathspec, or clears it when the field is empty.
    pub(crate) fn set_path_filter(&mut self, cx: &mut Context<Self>) {
        let paths = if self.path_draft.trim().is_empty() {
            Vec::new()
        } else {
            vec![PathBuf::from(self.path_draft.trim())]
        };
        let filter = LogFilter {
            paths,
            ..self.filter.clone()
        };
        self.set_filter(filter, cx);
    }

    /// IntelliSort: `--topo-order` keeps a merged branch's commits
    /// contiguous instead of interleaving them by date.
    pub(crate) fn toggle_topo_order(&mut self, cx: &mut Context<Self>) {
        let filter = LogFilter {
            topo_order: !self.filter.topo_order,
            ..self.filter.clone()
        };
        self.set_filter(filter, cx);
    }

    /// Replaces the filter and restarts the query from the top.
    ///
    /// This is a reset, never a narrowing of what is already loaded.
    /// `load_next_chunk` computes `skip` as `self.commits.len()`, which is
    /// only true while `commits` is exactly a prefix of the log; dropping
    /// rows out of it would make every later page skip the wrong commits.
    pub(crate) fn set_filter(&mut self, filter: LogFilter, cx: &mut Context<Self>) {
        if self.filter == filter {
            return;
        }
        self.filter = filter;
        // Strand any chunk still in flight: it was fetched for the old query
        // and must not land in the new set. Dropping the task cancels it;
        // the generation bump covers a result already on its way back.
        self.generation += 1;
        self.load_task = None;
        self.commits.clear();
        self.rows.clear();
        self.layout_cursor = None;
        self.exhausted = false;
        self.settled = false;
        self.empty_reason = None;
        self.error = None;
        self.pagination_error = None;
        self.load_next_chunk(cx);
    }

    /// Runs the current draft as a query immediately — Enter, or a toggle
    /// flipped, where waiting would feel broken.
    pub(crate) fn apply_search_now(&mut self, cx: &mut Context<Self>) {
        self.search_generation += 1;
        let generation = self.search_generation;
        let draft = self.search_draft.clone();
        let regex = self.search_regex;
        let case_sensitive = self.search_case_sensitive;
        let base = self.filter.clone();
        let repo_root = self.repo_root.clone();

        if draft.is_empty() {
            self.set_filter(
                LogFilter {
                    text: None,
                    rev: None,
                    ..base
                },
                cx,
            );
            return;
        }

        // `resolve_commit` shells out, so it runs off the render thread —
        // the same reason `has_commits` is resolved beside the log read
        // rather than inside the `update` closure.
        self.search_task = Some(cx.spawn(async move |this, cx| {
            let resolved = if sirio_git::looks_like_hash(&draft) {
                cx.background_spawn({
                    let repo_root = repo_root.clone();
                    let draft = draft.clone();
                    async move { GitLog::resolve_commit(&repo_root, &draft) }
                })
                .await
            } else {
                None
            };
            let _ = this.update(cx, |this, cx| {
                if this.search_generation != generation {
                    return;
                }
                let filter = match resolved {
                    // It named a commit: show that commit, and do not also
                    // grep for its own hash.
                    Some(sha) => LogFilter {
                        rev: Some(sha),
                        text: None,
                        ..base
                    },
                    // Hex-looking but unresolvable, or not hex at all: text.
                    None => LogFilter {
                        rev: None,
                        text: Some(draft),
                        regex,
                        case_sensitive,
                        ..base
                    },
                };
                this.set_filter(filter, cx);
            });
        }));
    }

    /// Runs the draft once the typing stops.
    ///
    /// A gpui timer, not `std::thread::sleep`: only the former can be moved
    /// by `background_executor.advance_clock`, so only the former leaves this
    /// testable without sleeping for real.
    pub(crate) fn schedule_search(&mut self, cx: &mut Context<Self>) {
        self.search_generation += 1;
        let generation = self.search_generation;
        self.search_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| {
                if this.search_generation != generation {
                    return;
                }
                this.apply_search_now(cx);
            });
        }));
    }

    /// Keys for the search field.
    ///
    /// Typing and Backspace go through the debounce; Enter and Escape do not.
    /// A key the user meant as "now" must not sit for 250ms.
    pub(crate) fn on_search_key(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) {
        self.search_blink.wake();
        match event.keystroke.key.as_str() {
            "enter" => self.apply_search_now(cx),
            "escape" => {
                self.search_draft.clear();
                self.apply_search_now(cx);
            }
            "backspace" => {
                self.search_draft.pop();
                self.schedule_search(cx);
            }
            _ => {
                if let Some(character) = event.keystroke.key_char.as_deref()
                    && character != "\n"
                {
                    self.search_draft.push_str(character);
                    self.schedule_search(cx);
                }
            }
        }
        cx.notify();
    }

    fn flip_search_blink(&mut self, cx: &mut Context<Self>) {
        self.search_blink.flip();
        cx.notify();
    }

    /// Blink timer tick for the Paths popup's free-text row.
    fn flip_path_blink(&mut self, cx: &mut Context<Self>) {
        self.path_blink.flip();
        cx.notify();
    }

    /// Key handling for the pathspec row: the same shape as
    /// [`GitHistory::on_search_key`], but with no debounce to schedule —
    /// git validates the pathspec only when it is applied.
    pub(crate) fn on_path_key(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) {
        self.path_blink.wake();
        match event.keystroke.key.as_str() {
            "enter" => self.set_path_filter(cx),
            "escape" => {
                self.path_draft.clear();
                self.set_path_filter(cx);
            }
            "backspace" => {
                self.path_draft.pop();
            }
            _ => {
                if let Some(character) = event.keystroke.key_char.as_deref()
                    && character != "\n"
                {
                    self.path_draft.push_str(character);
                }
            }
        }
        cx.notify();
    }
}

impl EventEmitter<GitHistoryEvent> for GitHistory {}

impl Render for GitHistory {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let entity = cx.entity();

        self.search_focus
            .get_or_insert_with(|| cx.focus_handle().tab_stop(true));
        self.path_focus
            .get_or_insert_with(|| cx.focus_handle().tab_stop(true));
        let search_focus = self.search_focus.as_ref().expect("just initialized");
        let search_focused = search_focus.is_focused(window);
        crate::caret::schedule(
            &mut self.search_blink,
            search_focused,
            Self::flip_search_blink,
            cx,
        );
        self.search_caret_visible = search_focused && self.search_blink.visible();
        let path_focused = self
            .path_focus
            .as_ref()
            .expect("just initialized")
            .is_focused(window);
        crate::caret::schedule(
            &mut self.path_blink,
            path_focused,
            Self::flip_path_blink,
            cx,
        );
        self.path_caret_visible = path_focused && self.path_blink.visible();

        let toolbar = history_toolbar::render_toolbar(
            history_toolbar::toolbar_layout(self.panel_width),
            self,
            entity.clone(),
            theme,
            window,
            cx,
        )
        .into_any_element();

        let content: AnyElement = if let Some(error) = self.error.clone() {
            let retry_entity = entity.clone();
            div()
                .id("history-error")
                .debug_selector(|| "history-error".to_owned())
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(theme.spacing.titlebar_control_spacing)
                .p(theme.spacing.card_gap)
                .child(
                    div()
                        .text_size(theme.typography.headline)
                        .text_color(theme.danger)
                        .child(format!("History unavailable: {error}")),
                )
                .child(
                    div()
                        .id("history-retry")
                        .debug_selector(|| "history-retry".to_owned())
                        .px(px(10.0))
                        .py(px(5.0))
                        .rounded(theme.radii.control)
                        .text_color(theme.text)
                        .bg(theme.element_hover)
                        .hover(|style| style.bg(theme.element_hover))
                        .on_click(move |_, _, cx| {
                            retry_entity.update(cx, |history, cx| history.retry(cx));
                        })
                        .child("Retry"),
                )
                .into_any_element()
        } else if !self.settled {
            div()
                .id("history-loading")
                .debug_selector(|| "history-loading".to_owned())
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(theme.spacing.card_gap)
                .text_size(theme.typography.headline)
                .text_color(theme.text_muted)
                .child(loading::indeterminate(
                    "history-loading-orb",
                    loading::GENERIC_ORB,
                    &theme,
                    window,
                    cx,
                ))
                .child("Loading history…")
                .child(loading::skeleton_rows(
                    "history-skeleton",
                    loading::SKELETON_ROWS,
                    &theme,
                    window,
                    cx,
                ))
                .into_any_element()
        } else if let Some(reason) = self.empty_reason {
            let label = match reason {
                EmptyReason::NotARepository => "Not a git repository",
                EmptyReason::NoCommits => "No commits yet",
                EmptyReason::NoMatches => "No commits match the filter",
            };
            div()
                .id("history-empty")
                .debug_selector(|| "history-empty".to_owned())
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.text_faint)
                .child(label)
                .into_any_element()
        } else {
            let commits = self.commits.clone();
            let rows = self.rows.clone();
            // Zero width means "draw no graph": the rows of a filtered set are
            // not contiguous, so any lane between them would be a lie. `rows` is
            // still computed and still zips 1:1 with `commits` — emptying it
            // would make the `zip` in the list builder yield nothing at all.
            let graph_width = if self.filter.is_filtering() {
                0.0
            } else {
                graph_width(&rows)
            };
            // Every row in a `uniform_list` is the same height, so the list
            // has to be tall enough for the tallest row it might draw. While a
            // text search is on, any row can gain a body-match line, so they
            // all get the room — rather than the annotated ones spilling over
            // their neighbours, which is what the running app showed.
            let row_height = if self.filter.text.is_some() {
                ROW_HEIGHT_ANNOTATED
            } else {
                ROW_HEIGHT
            };
            // Which trailing columns the panel is currently wide enough for.
            // Resolved once per frame rather than per row: every row in a
            // `uniform_list` has the same width, so a per-row answer would be
            // the same answer computed hundreds of times.
            let columns = row_columns(self.panel_width, graph_width);
            let row_entity = entity.clone();
            let list = uniform_list(
                "right-panel-history",
                commits.len(),
                cx.processor(move |history, range: Range<usize>, _window, _cx| {
                    if range.end >= commits.len()
                        && !history.exhausted
                        && history.pagination_error.is_none()
                    {
                        history.load_next_chunk(_cx);
                    }
                    // A row whose subject already contains the search text
                    // needs no explanation; one that does not must have
                    // matched in the body, so fetch that one body — lazily,
                    // per visible row, once per sha (see `bodies_fetching`).
                    if let Some(text) = history.filter.text.clone() {
                        let case_sensitive = history.search_case_sensitive;
                        let repo_root = history.repo_root.clone();
                        for index in range.clone() {
                            let Some(commit) = commits.get(index) else {
                                continue;
                            };
                            let subject_matches = if case_sensitive {
                                commit.subject.contains(&text)
                            } else {
                                commit.subject.to_lowercase().contains(&text.to_lowercase())
                            };
                            if !subject_matches
                                && !history.bodies.contains_key(&commit.sha)
                                && history.bodies_fetching.insert(commit.sha.clone())
                            {
                                let sha = commit.sha.clone();
                                let repo = repo_root.clone();
                                let fetch_sha = sha.clone();
                                _cx.spawn(async move |this, cx| {
                                    let fetched = cx
                                        .background_spawn(
                                            async move { GitLog::body(&repo, &fetch_sha) },
                                        )
                                        .await;
                                    let _ = this.update(cx, |this, cx| {
                                        this.bodies_fetching.remove(&sha);
                                        if let Ok(body) = fetched {
                                            this.bodies.insert(sha.clone(), body);
                                            cx.notify();
                                        }
                                    });
                                })
                                .detach();
                            }
                        }
                    }
                    range
                        .filter_map(|index| commits.get(index).zip(rows.get(index)))
                        .map(|(commit, row)| {
                            let body_match = history.filter.text.as_ref().and_then(|text| {
                                history.bodies.get(&commit.sha).and_then(|body| {
                                    body.lines()
                                        .find(|line| {
                                            if history.search_case_sensitive {
                                                line.contains(text)
                                            } else {
                                                line.to_lowercase().contains(&text.to_lowercase())
                                            }
                                        })
                                        .map(|line| line.trim().to_owned())
                                })
                            });
                            render_history_row(
                                commit.clone(),
                                row.clone(),
                                graph_width,
                                row_entity.clone(),
                                theme,
                                body_match,
                                row_height,
                                columns,
                            )
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .debug_selector(|| "right-panel-history".to_owned())
            .flex_1()
            .min_h(px(0.0));
            let body = div().flex_1().min_h(px(0.0)).flex().flex_col().child(list);
            if let Some(error) = self.pagination_error.clone() {
                let retry_entity = entity;
                body.child(
                    div()
                        .id("history-pagination-error")
                        .debug_selector(|| "history-pagination-error".to_owned())
                        .w_full()
                        .flex_none()
                        .px(px(8.0))
                        .py(px(5.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(theme.typography.footnote)
                        .text_color(theme.danger)
                        .child(
                            div()
                                .flex_1()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(format!("History unavailable: {error}")),
                        )
                        .child(
                            div()
                                .id("history-pagination-retry")
                                .debug_selector(|| "history-pagination-retry".to_owned())
                                .px(px(6.0))
                                .py(px(3.0))
                                .rounded(theme.radii.control)
                                .text_color(theme.text)
                                .bg(theme.element_hover)
                                .on_click(move |_, _, cx| {
                                    retry_entity.update(cx, |history, cx| history.retry(cx));
                                })
                                .child("Retry"),
                        ),
                )
            } else {
                body
            }
            .into_any_element()
        };

        // The toolbar sits above every state — error, loading, empty and
        // list alike — so the field never disappears under the state it is
        // meant to change.
        div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(toolbar)
            .child(content)
            .into_any_element()
    }
}

fn render_history_row(
    commit: CommitRecord,
    row_data: GraphRow,
    graph_width: f32,
    entity: gpui::Entity<GitHistory>,
    theme: Theme,
    body_match: Option<String>,
    row_height: f32,
    columns: RowColumns,
) -> impl IntoElement {
    let sha = commit.sha.clone();
    // Built before anything is moved out of `commit` below.
    let (tooltip_subject, tooltip_meta) = commit_tooltip_text(&commit);
    let subject_color = if commit.parents.len() > 1 {
        theme.text_faint
    } else {
        theme.text
    };
    let date = chrono::Local
        .timestamp_opt(commit.timestamp, 0)
        .single()
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "—".to_owned());
    div()
        .id(format!("history-row-{sha}"))
        .debug_selector(|| "history-row".to_owned())
        .h(px(row_height))
        .w_full()
        .flex()
        .items_center()
        .gap(px(ROW_GAP))
        .px(px(8.0))
        .hover(|style| style.bg(theme.element_hover))
        .tooltip(move |_, cx| -> gpui::AnyView {
            cx.new(|_| CommitTooltip {
                theme,
                subject: tooltip_subject.clone(),
                meta: tooltip_meta.clone(),
            })
            .into()
        })
        .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
            entity.update(cx, |_, cx| {
                cx.emit(GitHistoryEvent::OpenCommit(sha.clone()))
            });
        })
        .when(graph_width > 0.0, |row| {
            row.child(
                div()
                    .id("history-graph")
                    .debug_selector(|| "history-graph".to_owned())
                    .w(px(graph_width))
                    .h_full()
                    .flex_none()
                    .child(graph_column(&row_data, theme)),
            )
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .text_ellipsis()
                // Footnote, not the panel's inherited base size: the History
                // list is a dense strip like the status and tab bars, and at
                // the panel's narrow end the base size truncated subjects to
                // "docs…" with room for nothing else. It also matches the
                // body-match line below, so a row reads as one unit.
                .text_size(theme.typography.footnote)
                .text_color(subject_color)
                .child(commit.subject)
                // The subject alone does not explain the match, so the
                // body line that does is drawn beneath it.
                .when_some(body_match, |column, line| {
                    column.child(
                        div()
                            .debug_selector(|| "history-body-match".to_owned())
                            .text_size(theme.typography.footnote)
                            .text_color(theme.text_faint)
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(format!("└ {line}")),
                    )
                }),
        )
        .when(columns.author, move |row| {
            row.child(
                div()
                    .debug_selector(|| "history-row-author".to_owned())
                    .w(px(AUTHOR_WIDTH))
                    .flex_none()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_faint)
                    .child(commit.author),
            )
        })
        .when(columns.date, move |row| {
            row.child(
                div()
                    .debug_selector(|| "history-row-date".to_owned())
                    .w(px(DATE_WIDTH))
                    .flex_none()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_faint)
                    .child(date),
            )
        })
}

/// Which of a commit row's trailing columns fit at the panel's current
/// width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RowColumns {
    author: bool,
    date: bool,
}

/// Room left for the subject once `columns` and the graph are drawn.
///
/// `content` is the row's own box — `panel_width` less [`ROW_CHROME`]. The
/// gap count is derived rather than assumed because a hidden column takes
/// its gap with it.
fn subject_width(content: f32, graph_width: f32, columns: RowColumns) -> f32 {
    let mut children = 1usize;
    let mut fixed = 0.0;
    if graph_width > 0.0 {
        children += 1;
        fixed += graph_width;
    }
    if columns.author {
        children += 1;
        fixed += AUTHOR_WIDTH;
    }
    if columns.date {
        children += 1;
        fixed += DATE_WIDTH;
    }
    content - fixed - ROW_GAP * (children - 1) as f32
}

/// Drops trailing columns, author first, until the subject has room to be
/// read.
///
/// Flexbox on its own does the opposite. The subject is the only column that
/// *can* shrink — it is `flex_1` with a zero floor, while author and date
/// are `flex_none` — so when the row runs out of room the subject is what
/// goes, and the two fixed columns overflow the panel's border. That is
/// backwards: a row whose subject is gone identifies nothing, while a row
/// without its author still does. So the width decision is taken here,
/// before layout, and the columns that lose are not drawn at all.
///
/// The graph is deliberately *not* on this ladder, even though at six lanes
/// it costs more than the date column. Author and date are metadata about a
/// commit; the graph is the commit's place in history, which is what the
/// view is for. Narrowing it would also have to narrow it honestly — a lane
/// clipped mid-fan draws a commit with no node — and that is a different
/// change from this one.
///
/// Analogous to `history_toolbar::toolbar_layout` and deliberately shaped
/// like it: a pure function of the panel width, testable without a window.
fn row_columns(panel_width: f32, graph_width: f32) -> RowColumns {
    let content = panel_width - ROW_CHROME;
    [
        RowColumns {
            author: true,
            date: true,
        },
        RowColumns {
            author: false,
            date: true,
        },
    ]
    .into_iter()
    .find(|columns| subject_width(content, graph_width, *columns) >= MIN_SUBJECT_WIDTH)
    // Last rung: nothing left to drop, so the subject takes what there is.
    .unwrap_or(RowColumns {
        author: false,
        date: false,
    })
}

/// The two blocks of a commit row's hover card: the subject in full, and one
/// meta line under it.
///
/// Split out of the view so it can be tested without hovering anything. The
/// date is deliberately formatted with the hour here and without it in the
/// row: the column is a fixed 72px and the row is where space is scarce,
/// while the card is where the detail the row could not fit belongs.
fn commit_tooltip_text(commit: &CommitRecord) -> (String, String) {
    let when = chrono::Local
        .timestamp_opt(commit.timestamp, 0)
        .single()
        .map(|date| date.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "—".to_owned());
    let short: String = commit.sha.chars().take(7).collect();
    let mut meta = format!("{} · {when} · {short}", commit.author);
    // Refs are what git itself puts first when it prints a commit; they are
    // appended rather than led with because the row already has no room for
    // them and the subject is still what identifies the commit.
    if !commit.refs.is_empty() {
        meta.push_str(" · ");
        meta.push_str(&commit.refs.join(", "));
    }
    (commit.subject.clone(), meta)
}

/// The hover card for one commit row, on the pattern of
/// `status_bar::StatusBarTooltip`.
///
/// Earns its place because the row truncates at *every* width, not only the
/// narrow ones: a conventional-commit subject outgrows a 200px column long
/// before the panel is at its floor.
struct CommitTooltip {
    theme: Theme,
    subject: String,
    meta: String,
}

impl Render for CommitTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(420.0))
            .flex()
            .flex_col()
            .gap(px(3.0))
            .px(px(8.0))
            .py(px(5.0))
            .rounded(self.theme.radii.control)
            .bg(self.theme.surface_raised)
            .border_1()
            .border_color(self.theme.border)
            .child(
                div()
                    .text_size(self.theme.typography.footnote)
                    .text_color(self.theme.text)
                    .child(self.subject.clone()),
            )
            .child(
                div()
                    .text_size(self.theme.typography.caption2)
                    .text_color(self.theme.text_faint)
                    .child(self.meta.clone()),
            )
    }
}

/// Horizontal pitch between lanes.
const LANE_WIDTH: f32 = 12.0;
/// Lanes drawn before the column stops growing.
const MAX_LANES: usize = 6;
/// Radius of a commit node.
const NODE_RADIUS: f32 = 3.5;

fn graph_width(rows: &[GraphRow]) -> f32 {
    let peak = rows
        .iter()
        .map(|row| {
            let through = row
                .through
                .iter()
                .rposition(Option::is_some)
                .map_or(0, |index| index + 1);
            let joins = row
                .joins_in
                .iter()
                .map(|(column, _)| column + 1)
                .max()
                .unwrap_or(0);
            let edges = row
                .edges_out
                .iter()
                .map(|(column, _)| column + 1)
                .max()
                .unwrap_or(0);
            through.max(joins).max(edges).max(row.lane + 1)
        })
        .max()
        .unwrap_or(1)
        .clamp(1, MAX_LANES);
    peak as f32 * LANE_WIDTH
}

/// Whether this commit's node, and the curves anchored to it, can be drawn
/// at all.
///
/// The column's width stops at `MAX_LANES * LANE_WIDTH`, so a node past the
/// cap is clipped away entirely and every curve reaching for it runs off the
/// right edge and stops in mid-air. `layout` imposes no cap of its own, so a
/// repository with more than `MAX_LANES` concurrent branches reaches this.
fn node_is_drawable(row: &GraphRow) -> bool {
    row.lane < MAX_LANES
}

fn graph_column(row: &GraphRow, theme: Theme) -> impl IntoElement {
    let row = row.clone();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let origin_x: f32 = bounds.origin.x.into();
            let origin_y: f32 = bounds.origin.y.into();
            let height: f32 = bounds.size.height.into();
            let center_y = origin_y + height / 2.0;
            let x_for_lane = |lane: usize| origin_x + lane as f32 * LANE_WIDTH + LANE_WIDTH / 2.0;

            for (column, color) in row.through.iter().enumerate() {
                if column >= MAX_LANES || color.is_none() {
                    continue;
                }
                let x = x_for_lane(column);
                let half = 0.75;
                let mut path = Path::new(point(px(x - half), px(origin_y)));
                path.line_to(point(px(x + half), px(origin_y)));
                path.line_to(point(px(x + half), px(origin_y + height)));
                path.line_to(point(px(x - half), px(origin_y + height)));
                path.line_to(point(px(x - half), px(origin_y)));
                window.paint_path(path, theme.graph_lane(color.unwrap_or_default()));
            }

            if !node_is_drawable(&row) {
                return;
            }

            for &(source, color) in &row.joins_in {
                if source >= MAX_LANES {
                    continue;
                }
                let start_x = x_for_lane(source);
                let end_x = x_for_lane(row.lane);
                let start = point(px(start_x), px(origin_y));
                let end = point(px(end_x), px(center_y));
                let control = point(px((start_x + end_x) / 2.0), px(center_y));
                let mut path = Path::new(start);
                path.curve_to(end, control);
                window.paint_path(path, theme.graph_lane(color));
            }

            for &(target, color) in &row.edges_out {
                if target >= MAX_LANES {
                    continue;
                }
                let start_x = x_for_lane(row.lane);
                let end_x = x_for_lane(target);
                let start = point(px(start_x), px(center_y));
                let end = point(px(end_x), px(origin_y + height));
                let control = point(px((start_x + end_x) / 2.0), px(center_y));
                let mut path = Path::new(start);
                path.curve_to(end, control);
                window.paint_path(path, theme.graph_lane(color));
            }

            let node_x = x_for_lane(row.lane);
            let node_bounds = gpui::Bounds::new(
                point(px(node_x - NODE_RADIUS), px(center_y - NODE_RADIUS)),
                gpui::size(px(NODE_RADIUS * 2.0), px(NODE_RADIUS * 2.0)),
            );
            window.paint_quad(
                fill(node_bounds, theme.graph_lane(row.color))
                    .corner_radii(Corners::all(px(NODE_RADIUS))),
            );
        },
    )
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, TestAppContext, VisualTestContext};
    use sirio_theme::Theme;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    use history_toolbar::FilterChip;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "sirio-right-panel-history-test-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn seed_two_commits(dir: &Path) {
        git(dir, &["init", "-q", "-b", "main"]);
        git(dir, &["config", "user.email", "t@example.com"]);
        git(dir, &["config", "user.name", "Tester"]);
        std::fs::write(dir.join("a.txt"), "first").expect("write");
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", "first"]);
        std::fs::write(dir.join("b.txt"), "second").expect("write");
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", "second"]);
    }

    fn pump_until(cx: &TestAppContext, mut condition: impl FnMut() -> bool) {
        cx.executor().allow_parking();
        for _ in 0..600 {
            if condition() {
                return;
            }
            cx.executor()
                .advance_clock(std::time::Duration::from_secs(1));
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.run_until_parked();
        }
        panic!("condition never became true within the pump budget");
    }

    #[gpui::test]
    fn a_repository_without_commits_reports_the_empty_state(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        git(&dir.0, &["init", "-q", "-b", "main"]);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));

        pump_until(cx, || history.read_with(cx, |history, _| history.settled));

        history.read_with(cx, |history, _| {
            assert!(history.commits.is_empty());
            assert_eq!(history.empty_reason, Some(EmptyReason::NoCommits));
            assert!(history.error.is_none(), "an unborn HEAD is not an error");
        });
    }

    #[gpui::test]
    fn a_directory_without_git_reports_not_a_repository(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));

        pump_until(cx, || history.read_with(cx, |history, _| history.settled));

        history.read_with(cx, |history, _| {
            assert_eq!(history.empty_reason, Some(EmptyReason::NotARepository));
        });
    }

    #[gpui::test]
    fn commits_are_loaded_with_a_graph_row_each(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));

        pump_until(cx, || {
            history.read_with(cx, |history, _| !history.commits.is_empty())
        });

        history.read_with(cx, |history, _| {
            assert_eq!(
                history.commits.len(),
                history.rows.len(),
                "one graph row per commit"
            );
            assert_eq!(history.commits[0].subject, "second");
        });
    }

    #[gpui::test]
    fn a_failed_follow_up_chunk_keeps_the_commits_already_shown(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| !history.commits.is_empty())
        });

        std::fs::remove_dir_all(dir.0.join(".git")).expect("remove .git");
        history.update(cx, |history, cx| {
            history.exhausted = false;
            history.load_next_chunk(cx);
        });
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.pagination_error.is_some())
        });

        history.read_with(cx, |history, _| {
            assert_eq!(
                history.commits.len(),
                2,
                "existing rows survive a failed chunk"
            );
            assert!(
                history.error.is_none(),
                "the whole view is not an error state"
            );
        });
    }

    #[test]
    fn an_outgoing_lane_also_occupies_graph_width() {
        let branched = GraphRow {
            lane: 0,
            color: 0,
            through: vec![Some(0)],
            joins_in: Vec::new(),
            edges_out: vec![(4, 1)],
        };

        assert_eq!(
            graph_width(std::slice::from_ref(&branched)),
            5.0 * LANE_WIDTH
        );
    }

    #[test]
    fn the_graph_column_is_capped_at_six_lanes() {
        let wide = GraphRow {
            lane: 0,
            color: 0,
            through: (0..12).map(|i| Some(i % 6)).collect(),
            joins_in: Vec::new(),
            edges_out: Vec::new(),
        };

        let width = graph_width(std::slice::from_ref(&wide));

        assert_eq!(
            width,
            6.0 * LANE_WIDTH,
            "beyond six lanes the column stops growing"
        );
    }

    #[test]
    fn a_single_lane_column_is_one_lane_wide() {
        let narrow = GraphRow {
            lane: 0,
            color: 0,
            through: vec![Some(0)],
            joins_in: Vec::new(),
            edges_out: Vec::new(),
        };

        assert_eq!(graph_width(std::slice::from_ref(&narrow)), LANE_WIDTH);
    }

    #[test]
    fn a_node_past_the_lane_cap_is_not_drawn() {
        // `layout` caps nothing, so a repository with more concurrent
        // branches than the column can show produces these. Drawing one puts
        // the node outside the clipped column and leaves any curve reaching
        // for it ending in mid-air.
        let beyond = GraphRow {
            lane: MAX_LANES,
            color: 0,
            through: vec![Some(0); MAX_LANES + 1],
            joins_in: vec![(5, 1)],
            edges_out: Vec::new(),
        };
        let inside = GraphRow {
            lane: MAX_LANES - 1,
            color: 0,
            through: vec![Some(0); MAX_LANES],
            joins_in: Vec::new(),
            edges_out: Vec::new(),
        };

        assert!(!node_is_drawable(&beyond));
        assert!(
            node_is_drawable(&inside),
            "the last lane inside the cap still draws"
        );
    }

    #[test]
    fn holes_left_by_ended_lanes_do_not_widen_the_column() {
        let settled = GraphRow {
            lane: 0,
            color: 0,
            through: vec![Some(0), None, None],
            joins_in: Vec::new(),
            edges_out: Vec::new(),
        };

        assert_eq!(graph_width(std::slice::from_ref(&settled)), LANE_WIDTH);
    }

    /// The panel's own default. Nothing is dropped at the width the app
    /// starts at, or the fix would be a regression for everyone who never
    /// touches the divider.
    #[test]
    fn the_default_panel_width_keeps_every_column() {
        assert_eq!(
            row_columns(405.0, MAX_LANES as f32 * LANE_WIDTH),
            RowColumns {
                author: true,
                date: true
            }
        );
    }

    /// The width the app starts at must clear the first rung by a readable
    /// margin, not by a rounding error. It used to clear it by 5px — the
    /// author column was one pixel of chrome away from vanishing at the
    /// default, which would have read as a bug rather than as the ladder
    /// working.
    ///
    /// Deliberately not bought by lowering `MIN_SUBJECT_WIDTH`: the margin
    /// is the distance to that floor, so moving the floor to widen the
    /// margin measures nothing. It is bought by charging each column what it
    /// actually holds.
    #[test]
    fn the_default_panel_width_clears_the_first_rung_with_room_to_spare() {
        let graph = MAX_LANES as f32 * LANE_WIDTH;
        let subject = subject_width(
            405.0 - ROW_CHROME,
            graph,
            RowColumns {
                author: true,
                date: true,
            },
        );

        // Two lowercase characters of clearance, so a change to any one piece of
        // chrome cannot flip the default's behaviour on its own. Was three
        // characters (+14px) before #366 widened the date column from 71px to
        // 80px so `2026-09-03` fits on Windows; the default still keeps every
        // column with 9px to spare.
        assert!(
            subject >= MIN_SUBJECT_WIDTH + 8.0,
            "the default width leaves the subject {subject}px, only {}px clear of the \
             {MIN_SUBJECT_WIDTH}px floor",
            subject - MIN_SUBJECT_WIDTH
        );
    }

    /// The author goes first. In a single-author repository the column
    /// repeats one name down the whole list, while the date is the only
    /// thing placing a commit in time.
    #[test]
    fn a_narrow_panel_drops_the_author_before_the_date() {
        assert_eq!(
            row_columns(340.0, MAX_LANES as f32 * LANE_WIDTH),
            RowColumns {
                author: false,
                date: true
            }
        );
    }

    /// At the panel's 220px floor the subject gets the row to itself. This
    /// is the width the running app showed the defect at: the subject was
    /// squeezed to nothing and the two fixed columns overflowed the border.
    #[test]
    fn the_panel_floor_keeps_only_the_subject() {
        assert_eq!(
            row_columns(220.0, MAX_LANES as f32 * LANE_WIDTH),
            RowColumns {
                author: false,
                date: false
            }
        );
    }

    /// Filtering hides the graph, and the columns come back into the space
    /// it was using. The room is genuinely there, so refusing to use it
    /// would be its own bug.
    #[test]
    fn hiding_the_graph_gives_the_columns_back() {
        assert_eq!(
            row_columns(340.0, 0.0),
            RowColumns {
                author: true,
                date: true
            }
        );
    }

    /// The invariant the ladder exists to hold, checked across the panel's
    /// whole settings range rather than at the three widths above: while any
    /// column is still droppable, the subject is never below its minimum.
    /// Only the last rung — subject alone, nothing left to give — may be.
    #[test]
    fn a_column_is_never_kept_at_the_subjects_expense() {
        let graph = MAX_LANES as f32 * LANE_WIDTH;
        for width in 220..=640 {
            let width = width as f32;
            let columns = row_columns(width, graph);
            if !columns.author && !columns.date {
                continue;
            }
            let subject = subject_width(width - ROW_CHROME, graph, columns);
            assert!(
                subject >= MIN_SUBJECT_WIDTH,
                "at {width}px the row keeps {columns:?} and leaves the subject {subject}px"
            );
        }
    }

    fn tooltip_fixture(refs: Vec<String>) -> CommitRecord {
        CommitRecord {
            sha: "0123456789abcdef0123456789abcdef01234567".to_owned(),
            parents: Vec::new(),
            refs,
            author: "e.palmisano".to_owned(),
            timestamp: 1_755_000_000,
            subject: "feat(history): drop the columns the panel cannot fit".to_owned(),
        }
    }

    /// The card carries what the row had to give up. Asserted by structure
    /// rather than against a literal string: the timestamp is rendered in
    /// local time, so a literal would pass here and fail in another zone.
    #[test]
    fn the_tooltip_carries_the_full_subject_and_the_dropped_columns() {
        let commit = tooltip_fixture(vec!["HEAD -> main".to_owned()]);
        let (subject, meta) = commit_tooltip_text(&commit);

        assert_eq!(subject, commit.subject, "the subject is never truncated");
        assert!(meta.starts_with("e.palmisano · "));
        assert!(
            meta.contains("0123456"),
            "the short sha is seven characters"
        );
        assert!(
            !meta.contains("0123456789a"),
            "and not the whole object name"
        );
        assert!(meta.ends_with(" · HEAD -> main"));
    }

    /// Most commits carry no refs, and an empty list must not leave a
    /// dangling separator behind it.
    #[test]
    fn a_commit_without_refs_ends_at_its_sha() {
        let (_, meta) = commit_tooltip_text(&tooltip_fixture(Vec::new()));

        assert!(meta.ends_with("0123456"), "meta was {meta:?}");
    }

    /// An insertion bar is a claim about where typing lands, so a field
    /// that cannot receive typing must not draw one. `caret::schedule`
    /// parks an unfocused surface's blink at *visible* (nothing is meant to
    /// read it), and the search row rendered straight off that phase — so
    /// the History toolbar showed a permanently-solid bar in a field that
    /// had never been focused.
    #[gpui::test]
    async fn the_search_caret_stays_dark_until_the_field_is_focused(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let window = cx.add_window(|_window, cx| GitHistory::new(dir.0.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let history =
            cx.update(|window, _| window.root::<GitHistory>().flatten().expect("history root"));

        assert!(
            !history.read_with(&cx.cx, |history, _| history.search_caret_visible),
            "an unfocused search field must not claim to be taking input"
        );

        cx.update(|window, cx| {
            history.update(cx, |history, cx| {
                let handle = history.search_focus_handle().clone();
                handle.focus(window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });

        assert!(
            history.read_with(&cx.cx, |history, _| history.search_caret_visible),
            "and focusing it must light the bar"
        );
    }

    /// The Paths popup is a free-text row with its own focus handle and its
    /// own key handler, so it is a text field by every measure except the
    /// one the user checks: it drew no insertion bar at all.
    #[gpui::test]
    async fn the_paths_field_draws_a_caret_while_it_holds_focus(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let window = cx.add_window(|_window, cx| GitHistory::new(dir.0.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let history =
            cx.update(|window, _| window.root::<GitHistory>().flatten().expect("history root"));

        history.update(&mut cx.cx, |history, cx| {
            history.open_chip = Some(history_toolbar::FilterChip::Paths);
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("history-path-field").is_some(),
            "fixture invariant: the Paths popup must be open for this test to \
             exercise anything"
        );

        cx.update(|window, cx| {
            history.update(cx, |history, cx| {
                let handle = history.path_focus_handle().clone();
                handle.focus(window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });

        assert!(
            cx.debug_bounds("history-path-caret").is_some(),
            "a focused free-text row must show where the next character lands"
        );
        assert!(
            history.read_with(&cx.cx, |history, _| history.path_caret_visible),
            "and the bar must be lit, not merely present in layout"
        );
    }

    /// A filter that matches nothing must not claim the repository is empty.
    #[gpui::test]
    fn a_filter_with_no_matches_is_not_an_empty_repository(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));

        history.update(cx, |history, cx| {
            history.set_filter(
                LogFilter {
                    text: Some("nothing-matches-this".to_owned()),
                    ..LogFilter::default()
                },
                cx,
            );
        });
        pump_until(cx, || {
            history.read_with(cx, |history, _| {
                history.settled && history.commits.is_empty()
            })
        });

        assert_eq!(
            history.read_with(cx, |history, _| history.empty_reason),
            Some(EmptyReason::NoMatches),
            "the repository has two commits; only the filter is empty"
        );
    }

    /// A filter change restarts the query from the top. Narrowing the loaded
    /// vector instead would break `skip = self.commits.len()`, which assumes
    /// `commits` is exactly a prefix of the log.
    #[gpui::test]
    fn changing_the_filter_reloads_from_the_first_commit(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 2)
        });

        history.update(cx, |history, cx| {
            history.set_filter(
                LogFilter {
                    text: Some("second".to_owned()),
                    ..LogFilter::default()
                },
                cx,
            );
        });
        pump_until(cx, || {
            history.read_with(cx, |history, _| {
                history.settled && history.commits.len() == 1
            })
        });

        history.read_with(cx, |history, _| {
            assert_eq!(history.commits[0].subject, "second");
            assert_eq!(
                history.rows.len(),
                history.commits.len(),
                "the graph rows are rebuilt for the new result set, not left stale"
            );
        });
    }

    /// On a filtered set the rows are not contiguous in history, so a lane
    /// drawn between two of them would connect commits that are not parent
    /// and child. Hiding is the only option that draws nothing false.
    #[gpui::test]
    async fn the_graph_column_disappears_while_a_filter_is_active(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let window = cx.add_window(|_window, cx| GitHistory::new(dir.0.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let history =
            cx.update(|window, _| window.root::<GitHistory>().flatten().expect("history root"));
        pump_until(&cx.cx, || {
            history.read_with(&cx.cx, |history, _| history.commits.len() == 2)
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("history-graph").is_some(),
            "baseline: the graph column is drawn without a filter"
        );

        history.update(&mut cx.cx, |history, cx| {
            history.set_filter(
                LogFilter {
                    text: Some("second".to_owned()),
                    ..LogFilter::default()
                },
                cx,
            );
        });
        pump_until(&cx.cx, || {
            history.read_with(&cx.cx, |history, _| history.commits.len() == 1)
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("history-graph").is_none(),
            "a filtered set draws no lanes"
        );
    }

    /// Found by running the app, not by any of the tests above: the
    /// body-match line drew straight over the next row's subject.
    ///
    /// `uniform_list` gives every row one height, and it has no way to make
    /// just the annotated ones taller — so the list has to be tall enough for
    /// the tallest row it might draw, which means all rows grow while a text
    /// search is on. Asserting on the drawn height is the only way to see
    /// this: every test that checks the data was already green.
    #[gpui::test]
    async fn rows_grow_to_fit_a_body_match_line(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let window = cx.add_window(|_window, cx| GitHistory::new(dir.0.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let history =
            cx.update(|window, _| window.root::<GitHistory>().flatten().expect("history root"));
        pump_until(&cx.cx, || {
            history.read_with(&cx.cx, |history, _| history.commits.len() == 2)
        });
        cx.run_until_parked();
        let unfiltered = cx
            .debug_bounds("history-row")
            .expect("a row with no filter")
            .size
            .height;

        history.update(&mut cx.cx, |history, cx| {
            history.set_filter(
                LogFilter {
                    text: Some("second".to_owned()),
                    ..LogFilter::default()
                },
                cx,
            );
        });
        pump_until(&cx.cx, || {
            history.read_with(&cx.cx, |history, _| history.commits.len() == 1)
        });
        cx.run_until_parked();
        let filtered = cx
            .debug_bounds("history-row")
            .expect("a row with a filter")
            .size
            .height;

        assert!(
            filtered > unfiltered,
            "rows must make room for the annotation: {filtered:?} is not taller than {unfiltered:?}"
        );
    }

    /// Three keystrokes must produce one query, not three: `--grep` walks
    /// every reachable commit, so a query per keystroke is a query per
    /// keystroke too many.
    #[gpui::test]
    fn typing_debounces_into_a_single_query(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 2)
        });

        for draft in ["s", "se", "sec"] {
            history.update(cx, |history, cx| {
                history.search_draft = draft.to_owned();
                history.schedule_search(cx);
            });
        }
        cx.run_until_parked();
        assert_eq!(
            history.read_with(cx, |history, _| history.filter.text.clone()),
            None,
            "nothing is queried while the typing is still going"
        );

        cx.executor().advance_clock(SEARCH_DEBOUNCE);
        cx.run_until_parked();

        assert_eq!(
            history.read_with(cx, |history, _| history.filter.text.clone()),
            Some("sec".to_owned()),
            "one query, with the last draft"
        );
    }

    /// An emptied field is not a filter for the empty string.
    #[gpui::test]
    fn clearing_the_field_removes_the_filter(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));

        history.update(cx, |history, cx| {
            history.search_draft = "second".to_owned();
            history.apply_search_now(cx);
        });
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 1)
        });

        history.update(cx, |history, cx| {
            history.search_draft.clear();
            history.apply_search_now(cx);
        });
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 2)
        });

        assert!(history.read_with(cx, |history, _| !history.filter.is_filtering()));
    }

    /// Selecting a branch narrows the query to it, and deselecting it puts
    /// the view back to every local branch — which is `LogFilter`'s empty
    /// `branches`, not a branch list containing everything.
    #[gpui::test]
    fn choosing_a_branch_narrows_the_query_and_deselecting_widens_it(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 2)
        });

        history.update(cx, |history, cx| {
            history.toggle_chip_option(FilterChip::Branch, "main".to_owned(), cx);
        });
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));
        history.read_with(cx, |history, _| {
            assert_eq!(history.filter.branches, vec!["main".to_owned()]);
            assert!(history.filter.is_filtering());
        });

        history.update(cx, |history, cx| {
            history.toggle_chip_option(FilterChip::Branch, "main".to_owned(), cx);
        });
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));
        history.read_with(cx, |history, _| {
            assert!(
                history.filter.branches.is_empty(),
                "empty means every branch; it must not become a list of all of them"
            );
            assert!(!history.filter.is_filtering());
        });
    }

    /// The author list is derived from the commits actually loaded, so it is
    /// a convenience list rather than the repository's full author set. The
    /// exhaustive alternative is a complete walk — exactly the cost someone
    /// filtering is trying to avoid.
    #[gpui::test]
    fn the_author_list_comes_from_the_loaded_commits_without_duplicates(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 2)
        });

        history.read_with(cx, |history, _| {
            assert_eq!(
                history.author_options(),
                vec!["Tester".to_owned()],
                "both commits share one author, so the list has one entry"
            );
        });

        history.update(cx, |history, cx| {
            history.toggle_chip_option(FilterChip::User, "Tester".to_owned(), cx);
        });
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));
        history.read_with(cx, |history, _| {
            assert_eq!(history.filter.authors, vec!["Tester".to_owned()]);
            assert_eq!(history.commits.len(), 2, "both commits are Tester's");
        });
    }

    /// Presets are handed to git verbatim: git parses its own relative
    /// dates, so nothing here computes a timestamp that would then need a
    /// clock to test.
    #[gpui::test]
    fn a_date_preset_is_passed_to_git_verbatim(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));

        history.update(cx, |history, cx| {
            history.set_date_preset(Some("7 days ago".to_owned()), cx);
        });
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));
        history.read_with(cx, |history, _| {
            assert_eq!(history.filter.since.as_deref(), Some("7 days ago"));
            assert!(history.filter.is_filtering());
            assert_eq!(history.commits.len(), 2, "both commits are from today");
        });

        history.update(cx, |history, cx| history.set_date_preset(None, cx));
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));
        history.read_with(cx, |history, _| {
            assert!(history.filter.since.is_none());
            assert!(!history.filter.is_filtering());
        });
    }

    /// A pathspec narrows to the commits that touched it. `b.txt` arrives
    /// with the second commit, so it must not match the first.
    #[gpui::test]
    fn a_pathspec_narrows_to_the_commits_that_touched_it(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 2)
        });

        history.update(cx, |history, cx| {
            history.path_draft = "b.txt".to_owned();
            history.set_path_filter(cx);
        });
        pump_until(cx, || {
            history.read_with(cx, |history, _| {
                history.settled && history.commits.len() == 1
            })
        });
        history.read_with(cx, |history, _| {
            assert_eq!(history.commits[0].subject, "second");
        });
    }

    /// IntelliSort changes the ordering flag and nothing else — in
    /// particular it is not filtering, so the graph stays and an empty
    /// result would still mean "no commits", not "no matches".
    #[gpui::test]
    fn intellisort_reorders_without_filtering(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 2)
        });

        history.update(cx, |history, cx| history.toggle_topo_order(cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| {
                history.settled && history.commits.len() == 2
            })
        });
        history.read_with(cx, |history, _| {
            assert!(history.filter.topo_order);
            assert!(
                !history.filter.is_filtering(),
                "ordering is not filtering: the graph must stay"
            );
        });
    }

    /// At the panel's narrow end the four chips collapse behind one button,
    /// and at its wide end they are all present. The drawn frame is the only
    /// place this is actually true or false.
    #[gpui::test]
    async fn the_chips_collapse_when_the_panel_is_narrow(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let window = cx.add_window(|_window, cx| GitHistory::new(dir.0.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // The test window is wide, so every chip is drawn.
        assert!(cx.debug_bounds("history-chip-branch").is_some());
        assert!(cx.debug_bounds("history-chip-paths").is_some());
        assert!(cx.debug_bounds("history-chips-collapsed").is_none());

        let history =
            cx.update(|window, _| window.root::<GitHistory>().flatten().expect("history root"));
        history.update(&mut cx.cx, |history, cx| {
            history.panel_width = 230.0;
            cx.notify();
        });
        cx.run_until_parked();

        assert!(cx.debug_bounds("history-chip-branch").is_none());
        assert!(
            cx.debug_bounds("history-chips-collapsed").is_some(),
            "the four chips are behind one button, not gone"
        );
    }

    /// What the running app showed: at the panel's narrow end the subject
    /// was squeezed to nothing while the author and date kept their fixed
    /// widths and spilled past the border. The drawn frame is where that is
    /// true or false — `row_columns` alone cannot prove the row obeys it.
    #[gpui::test]
    async fn a_narrow_panel_drops_the_columns_and_keeps_the_subject(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let window = cx.add_window(|_window, cx| GitHistory::new(dir.0.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let history =
            cx.update(|window, _| window.root::<GitHistory>().flatten().expect("history root"));
        pump_until(&cx.cx, || {
            history.read_with(&cx.cx, |history, _| history.commits.len() == 2)
        });
        cx.run_until_parked();

        // The panel's own default width: nothing is dropped there.
        assert!(cx.debug_bounds("history-row-author").is_some());
        assert!(cx.debug_bounds("history-row-date").is_some());

        history.update(&mut cx.cx, |history, cx| {
            history.panel_width = 230.0;
            cx.notify();
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("history-row").is_some(),
            "the subject is what a narrow row keeps"
        );
        assert!(
            cx.debug_bounds("history-row-author").is_none(),
            "the author yields before the subject does"
        );
        assert!(
            cx.debug_bounds("history-row-date").is_none(),
            "and so does the date"
        );
    }

    /// #366: the date column wrapped to two lines when its text was wider
    /// than `DATE_WIDTH`, so the first line sat half a line above the row's
    /// subject (and the first row's date stuck out above the list, under the
    /// header) while the second line was clipped with no ellipsis.
    ///
    /// The author column already carries `overflow_hidden` + `text_ellipsis`
    /// and stays single-line; the date must do the same. The drawn frame is
    /// the only place this is true or false — `DATE_WIDTH` alone cannot prove
    /// the row obeys it.
    #[gpui::test]
    async fn the_date_column_stays_single_line_like_the_author(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let window = cx.add_window(|_window, cx| GitHistory::new(dir.0.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let history =
            cx.update(|window, _| window.root::<GitHistory>().flatten().expect("history root"));
        pump_until(&cx.cx, || {
            history.read_with(&cx.cx, |history, _| history.commits.len() == 2)
        });
        cx.run_until_parked();

        let author = cx
            .debug_bounds("history-row-author")
            .expect("author column is drawn at the default width");
        let date = cx
            .debug_bounds("history-row-date")
            .expect("date column is drawn at the default width");

        let author_h: f32 = author.size.height.into();
        let date_h: f32 = date.size.height.into();
        assert!(
            (date_h - author_h).abs() < 1.0,
            "date ({date_h}px) must stay single-line like the author ({author_h}px), or it sits half a line above its row (#366)"
        );
        assert!(
            date_h <= ROW_HEIGHT + 1.0,
            "a wrapped date ({date_h}px) spills out of its {ROW_HEIGHT}px row (#366)"
        );
    }
}
