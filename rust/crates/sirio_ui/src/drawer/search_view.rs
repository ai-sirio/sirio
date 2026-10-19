//! Search as the drawer draws it: the query row, the filters, the totals line
//! and Ely's `SearchPanel` over the results `search_worktree` found.
//!
//! One `WorktreeSearch` belongs to a worktree's drawer, so every file tab of the
//! worktree shows the same query, results, folds and chosen result.
//!
//! Ways this can fail, written before the code:
//! - an older run's answer lands after a newer query's, or in a drawer whose
//!   worktree it was not run for: every run carries the generation it started
//!   under, and an answer is applied only while that is the current one (the
//!   drawer is the worktree's, so the second half is by construction);
//! - a superseded run keeps reading files nobody will look at: starting a run
//!   sets the previous run's cancel flag, and replacing the task drops its debounce;
//! - a query typed in bursts runs once per key instead of once after the pause;
//! - an invalid pattern is drawn as "0 results" or crashes the panel instead of
//!   saying why in the totals line (`summary_line`);
//! - the totals line leaves out what the run did not read (skips) or that it
//!   stopped at the cap, so a partial list reads as the whole answer
//!   (`the_summary_names_skips_and_the_cap`);
//! - a cancelled run is drawn as a finished one, or a run still going is drawn
//!   as finished (`a_running_search_is_not_a_finished_one`);
//! - a capped run of tens of thousands of lines is laid out row by row, so one
//!   frame takes seconds: the panel is given at most `DRAW_ROWS` rows, and says
//!   how many lines it left out (`a_long_list_is_cut_at_the_row_budget`);
//! - the debug verb's `--open=N` counts rows another way than the panel draws
//!   them (folded files, the row budget), so it opens a different hit than the
//!   N-th drawn: both read `drawn_layout`;
//! - an open dirty tab's text is read while its owner is being updated (a
//!   double borrow): the buffers are asked for only inside the run's own task.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ely_gpui_component::buttons::{Button, ButtonVariant, IconButton, ToggleButton, ToggleItem};
use ely_gpui_component::editor::{FileHits, FindOptions, SearchPanel};
use ely_gpui_component::forms::{Input, InputEvent, TextInput};
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::theme::ControlSize;
use gpui::{
    App, Context, Entity, Focusable as _, IntoElement, ParentElement, SharedString, Styled,
    Subscription, Task, Window, div, prelude::*, px,
};
use sirio_theme::Theme;

use super::problems_view::display_path;
use super::search_engine::{FileResult, MATCH_CAP, SearchOutcome, SearchQuery, Skip};
use super::{WorktreeDrawer, WorktreeDrawerEvent, search_engine};

/// How long a query rests before it runs.
const DEBOUNCE: Duration = Duration::from_millis(150);
/// How often a running search shows what it has found so far.
const PROGRESS: Duration = Duration::from_millis(120);
/// The most rows (a file's header, a matched line) the panel is given at once.
const DRAW_ROWS: usize = 1500;

/// The dirty open tabs' LF text by worktree-relative path, as the app has it.
pub type BufferSource = Rc<dyn Fn(&App) -> HashMap<PathBuf, String>>;

/// The four fields, made when the first frame that needs them has a window.
struct Fields {
    query: Entity<TextInput>,
    replace: Entity<TextInput>,
    include: Entity<TextInput>,
    exclude: Entity<TextInput>,
    _subscriptions: Vec<Subscription>,
}

/// What a worktree's Search holds.
#[derive(Default)]
pub(super) struct WorktreeSearch {
    fields: Option<Fields>,
    options: FindOptions,
    files: Vec<FileResult>,
    skipped: BTreeMap<Skip, usize>,
    capped: bool,
    cancelled: bool,
    error: Option<String>,
    /// Files whose tab held unsaved text when the run began: read from the buffer.
    unsaved: BTreeSet<PathBuf>,
    folded: Vec<PathBuf>,
    selected: Option<(PathBuf, usize)>,
    /// The run asked for last. An answer of any other is stale.
    generation: u64,
    cancel: Option<Arc<AtomicBool>>,
    /// The pause and then the run, as one task: dropping it ends the pause.
    task: Option<Task<()>>,
    buffer_source: Option<BufferSource>,
}

/// The files and lines the panel is given: which results are drawn, in order.
struct Drawn<'a> {
    /// Each file, and how many of its lines are drawn (0: folded).
    files: Vec<(&'a FileResult, usize)>,
    /// Lines of unfolded files the row budget left out.
    omitted: usize,
}

/// What the debug state reads of a worktree's Search.
#[derive(Clone, Debug, Default)]
pub struct SearchFields {
    pub query: String,
    /// Every match of the results, not the lines.
    pub results: usize,
    pub files: usize,
    /// The files drawn, relative to the worktree, `/` between the parts.
    pub paths: Vec<String>,
    pub unsaved: Vec<String>,
    pub summary: String,
    pub capped: bool,
    /// `path:line`, the line counted from 1 as the panel draws it.
    pub selected: String,
    /// A query is waiting for its pause, or a run is going.
    pub running: bool,
}

/// What a debug call asks of Search; each field left `None` is left as it is.
#[derive(Clone, Debug, Default)]
pub struct SearchRequest {
    pub query: Option<String>,
    pub replace: Option<String>,
    pub include: Option<String>,
    pub exclude: Option<String>,
    pub case: Option<bool>,
    pub word: Option<bool>,
    pub regex: Option<bool>,
    /// Chooses the N-th drawn line, 0-based.
    pub open: Option<usize>,
    /// Runs the current query again, with no pause.
    pub refresh: bool,
}

/// `12 345` as `12 345`: groups of three, a space between.
fn thousands(count: usize) -> String {
    let digits = count.to_string();
    let mut out = String::new();
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at) % 3 == 0 {
            out.push(' ');
        }
        out.push(digit);
    }
    out
}

/// The last line of a pattern error: the regex crate's message draws the
/// pattern above it, and only that last line fits a totals line.
fn last_line(message: &str) -> &str {
    message.lines().last().unwrap_or(message).trim()
}

/// What a run came to, for `summary_line`.
struct Totals<'a> {
    matches: usize,
    files: usize,
    skipped: &'a BTreeMap<Skip, usize>,
    capped: bool,
    cancelled: bool,
    error: Option<&'a str>,
    running: bool,
}

/// The totals line: `N results in M files`, then `· K skipped (reasons)`, then
/// `· stopped at 20 000 matches`; a pattern that does not compile says why
/// instead; a run still going says so first.
fn summary_line(totals: &Totals) -> String {
    if let Some(error) = totals.error {
        return last_line(error).to_owned();
    }
    let mut line = String::new();
    if totals.running {
        line.push_str("Searching… ");
    }
    line.push_str(&format!("{} results in {} files", totals.matches, totals.files));
    let left_out: usize = totals.skipped.values().sum();
    if left_out > 0 {
        let reasons: Vec<&str> = totals
            .skipped
            .keys()
            .map(|skip| match skip {
                Skip::TooLarge => "too large",
                Skip::Binary => "binary",
                Skip::NotUtf8 => "not UTF-8",
                Skip::Unreadable => "unreadable",
            })
            .collect();
        line.push_str(&format!(" · {left_out} skipped ({})", reasons.join("/")));
    }
    if totals.capped {
        line.push_str(&format!(" · stopped at {} matches", thousands(MATCH_CAP)));
    }
    if totals.cancelled {
        line.push_str(" · cancelled");
    }
    line
}

/// The files the panel draws from `files`: a folded file's header alone, an
/// open one's lines, until `budget` rows are used. A file's header is always
/// drawn while there is room for it.
fn drawn_layout<'a>(files: &'a [FileResult], folded: &[PathBuf], budget: usize) -> Drawn<'a> {
    let mut drawn = Vec::new();
    let mut used = 0usize;
    let mut omitted = 0usize;
    for file in files {
        if used >= budget {
            if !folded.contains(&file.path) {
                omitted += file.hits.len();
            }
            continue;
        }
        used += 1;
        if folded.contains(&file.path) {
            drawn.push((file, 0));
            continue;
        }
        let room = budget - used;
        let lines = file.hits.len().min(room);
        used += lines;
        omitted += file.hits.len() - lines;
        drawn.push((file, lines));
    }
    Drawn { files: drawn, omitted }
}

impl WorktreeSearch {
    fn matches(&self) -> usize {
        self.files
            .iter()
            .flat_map(|file| &file.hits)
            .map(|found| found.ranges_in_file.len())
            .sum()
    }

    fn query(&self, cx: &App) -> Option<SearchQuery> {
        let fields = self.fields.as_ref()?;
        Some(SearchQuery {
            text: fields.query.read(cx).text().to_string(),
            options: self.options,
            include: fields.include.read(cx).text().to_string(),
            exclude: fields.exclude.read(cx).text().to_string(),
        })
    }

    fn running(&self) -> bool {
        self.task.is_some()
    }

    fn summary(&self, empty_query: bool) -> String {
        if empty_query && !self.running() {
            return String::new();
        }
        summary_line(&Totals {
            matches: self.matches(),
            files: self.files.len(),
            skipped: &self.skipped,
            capped: self.capped,
            cancelled: self.cancelled,
            error: self.error.as_deref(),
            running: self.running(),
        })
    }

    /// Drops the results and the chosen one: the query is empty.
    fn clear(&mut self) {
        self.begin(BTreeSet::new());
        self.selected = None;
        self.folded.clear();
    }

    /// A run starts: the last run's results go, the chosen result and the folds
    /// stay (the run may find them again), and `unsaved` are the files this run
    /// reads from their tab.
    fn begin(&mut self, unsaved: BTreeSet<PathBuf>) {
        self.files.clear();
        self.skipped.clear();
        self.capped = false;
        self.cancelled = false;
        self.error = None;
        self.unsaved = unsaved;
    }

    /// A run that has the current generation reports files as it finds them.
    fn add_files(&mut self, found: Vec<FileResult>) {
        self.files.extend(found);
    }

    /// The final answer of a run of the current generation.
    fn finish(&mut self, outcome: SearchOutcome) {
        self.files = outcome.files;
        self.skipped = outcome.skipped;
        self.capped = outcome.capped;
        self.cancelled = outcome.cancelled;
        self.error = outcome.error;
        self.cancel = None;
        // A chosen result the new run does not have stays chosen only if it is there.
        if let Some((path, line)) = &self.selected {
            let there = self.files.iter().any(|file| {
                file.path == *path && file.hits.iter().any(|found| found.hit.line == *line)
            });
            if !there {
                self.selected = None;
            }
        }
        self.folded.retain(|path| self.files.iter().any(|file| file.path == *path));
    }

    /// The first match of the drawn line `index` (0-based, folded files and the
    /// row budget as the panel has them): its file and the byte range to select.
    fn drawn_hit(&self, index: usize) -> Option<(PathBuf, usize, Range<usize>)> {
        let layout = drawn_layout(&self.files, &self.folded, DRAW_ROWS);
        let mut left = index;
        for (file, lines) in layout.files {
            if left < lines {
                let found = &file.hits[left];
                return Some((file.path.clone(), found.hit.line, found.ranges_in_file[0].clone()));
            }
            left -= lines;
        }
        None
    }

    fn fields(&self, cx: &App) -> SearchFields {
        let layout = drawn_layout(&self.files, &self.folded, DRAW_ROWS);
        let query = self
            .fields
            .as_ref()
            .map(|fields| fields.query.read(cx).text().to_string())
            .unwrap_or_default();
        SearchFields {
            summary: self.summary(query.is_empty()),
            query,
            results: self.matches(),
            files: self.files.len(),
            paths: layout.files.iter().map(|(file, _)| display_path(&file.path)).collect(),
            unsaved: self
                .files
                .iter()
                .filter(|file| self.unsaved.contains(&file.path))
                .map(|file| display_path(&file.path))
                .collect(),
            capped: self.capped,
            selected: self
                .selected
                .as_ref()
                .map(|(path, line)| format!("{}:{}", display_path(path), line + 1))
                .unwrap_or_default(),
            running: self.running(),
        }
    }
}

fn new_field(
    window: &mut Window,
    cx: &mut Context<WorktreeDrawer>,
    placeholder: &str,
) -> Entity<TextInput> {
    let placeholder = SharedString::from(placeholder.to_owned());
    cx.new(|cx| TextInput::new(window, cx).placeholder(placeholder))
}

impl WorktreeDrawer {
    /// The app's way of reading the dirty tabs' text: asked once per run, when
    /// the run starts, never while another entity is being updated.
    pub fn set_buffer_source(&mut self, source: BufferSource) {
        self.search.buffer_source = Some(source);
    }

    /// Makes the four fields when there is a window to make them in, and listens
    /// to each: an edit runs the search after a pause, Enter runs it at once.
    pub(super) fn ensure_search_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.fields.is_some() {
            return;
        }
        let query = new_field(window, cx, "Search");
        let replace = new_field(window, cx, "Replace");
        let include = new_field(window, cx, "Include: *.rs");
        let exclude = new_field(window, cx, "Exclude: tests/**");
        let mut subscriptions = Vec::new();
        for field in [&query, &include, &exclude] {
            subscriptions.push(cx.subscribe_in(
                field,
                window,
                |drawer, _, event: &InputEvent, _, cx| match event {
                    InputEvent::Changed => drawer.search_run(DEBOUNCE, cx),
                    InputEvent::Submit => drawer.search_run(Duration::ZERO, cx),
                    _ => {}
                },
            ));
        }
        self.search.fields = Some(Fields {
            query,
            replace,
            include,
            exclude,
            _subscriptions: subscriptions,
        });
    }

    /// Asks for a run after `delay`: the run before it, pause or search, is
    /// superseded. An empty query clears the results instead.
    pub(super) fn search_run(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let search = &mut self.search;
        search.generation += 1;
        let generation = search.generation;
        if let Some(cancel) = search.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        search.task = None;
        let Some(query) = search.query(cx) else {
            return;
        };
        if query.text.is_empty() {
            search.clear();
            cx.notify();
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        search.cancel = Some(cancel.clone());
        let worktree = self.worktree().to_path_buf();
        self.search.task = Some(cx.spawn(async move |drawer, cx| {
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
            }
            // The tabs are read now, on a turn of their own.
            let Ok(buffers) = drawer.update(cx, |drawer, cx| {
                if drawer.search.generation != generation {
                    return None;
                }
                let source = drawer.search.buffer_source.clone();
                let buffers = source.map(|source| source(cx)).unwrap_or_default();
                drawer.search.begin(buffers.keys().cloned().collect());
                cx.notify();
                Some(buffers)
            }) else {
                return;
            };
            let Some(buffers) = buffers else {
                return;
            };
            let found: Arc<Mutex<Vec<FileResult>>> = Arc::default();
            let mut work = cx.background_spawn({
                let found = found.clone();
                async move {
                    search_engine::search_worktree(&worktree, &query, &buffers, &cancel, |file| {
                        if let Ok(mut found) = found.lock() {
                            found.push(file);
                        }
                    })
                }
            });
            let outcome = loop {
                let tick = cx.background_executor().timer(PROGRESS);
                match futures::future::select(&mut work, tick).await {
                    futures::future::Either::Left((outcome, _)) => break outcome,
                    futures::future::Either::Right(_) => {
                        let new = found.lock().map(|mut found| std::mem::take(&mut *found));
                        let applied = drawer.update(cx, |drawer, cx| {
                            if drawer.search.generation != generation {
                                return false;
                            }
                            drawer.search.add_files(new.unwrap_or_default());
                            cx.notify();
                            true
                        });
                        if !applied.unwrap_or(false) {
                            return;
                        }
                    }
                }
            };
            let _ = drawer.update(cx, |drawer, cx| {
                if drawer.search.generation != generation {
                    return;
                }
                drawer.search.finish(outcome);
                drawer.search.task = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Search was asked for from the file tab's key: the drawer opens on Search,
    /// `seed` (the selection, when it is one line) becomes the query, and the
    /// query field takes the keyboard with its text selected.
    pub fn search_in_worktree(
        &mut self,
        seed: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show(super::DrawerTab::Search, cx);
        self.ensure_search_fields(window, cx);
        let Some(fields) = &self.search.fields else {
            return;
        };
        let query = fields.query.clone();
        if let Some(seed) = seed {
            query.update(cx, |field, cx| field.set_text(seed, cx));
        }
        let length = query.read(cx).text().len();
        query.update(cx, |field, cx| field.select(0..length, cx));
        query.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    /// The debug verb: sets what it names, runs when anything that decides the
    /// results changed or a refresh was asked, and chooses a drawn line.
    /// Opens the drawer on Search, as the key does, without taking the keyboard.
    /// The error is the choice that has no line.
    pub fn search_control(
        &mut self,
        request: SearchRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.show(super::DrawerTab::Search, cx);
        self.ensure_search_fields(window, cx);
        let Some(fields) = &self.search.fields else {
            return Err("Search has no fields yet".to_owned());
        };
        let (query, replace, include, exclude) = (
            fields.query.clone(),
            fields.replace.clone(),
            fields.include.clone(),
            fields.exclude.clone(),
        );
        let mut options = self.search.options;
        let mut changed = false;
        for (key, field) in [(request.case, &mut options.case), (request.word, &mut options.word)] {
            if let Some(on) = key {
                changed |= *field != on;
                *field = on;
            }
        }
        if let Some(on) = request.regex {
            changed |= options.regex != on;
            options.regex = on;
        }
        self.search.options = options;
        for (text, field) in [
            (request.query, &query),
            (request.replace, &replace),
            (request.include, &include),
            (request.exclude, &exclude),
        ] {
            if let Some(text) = text {
                field.update(cx, |field, cx| field.set_text(text, cx));
            }
        }
        if changed || request.refresh {
            self.search_run(if request.refresh { Duration::ZERO } else { DEBOUNCE }, cx);
        }
        if let Some(index) = request.open {
            if !self.open_drawn_hit(index, cx) {
                return Err(format!("Search draws no line {index}"));
            }
        }
        cx.notify();
        Ok(())
    }

    /// What the debug state reads of this worktree's Search.
    pub fn search_fields(&self, cx: &App) -> SearchFields {
        self.search.fields(cx)
    }

    /// Chooses the `index`-th line Search draws, as a press on it does. False
    /// when there is no such line.
    pub fn open_drawn_hit(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        let Some((path, line, range)) = self.search.drawn_hit(index) else {
            return false;
        };
        self.choose_hit(path, line, range, cx);
        true
    }

    fn choose_hit(
        &mut self,
        path: PathBuf,
        line: usize,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) {
        self.search.selected = Some((path.clone(), line));
        cx.emit(WorktreeDrawerEvent::OpenHit {
            path: self.worktree().join(&path),
            range,
        });
        cx.notify();
    }

    /// A press on a drawn line (`path` as drawn, `line` 0-based).
    fn press_hit(&mut self, path: &str, line: usize, cx: &mut Context<Self>) {
        let found = self.search.files.iter().find_map(|file| {
            if display_path(&file.path) != path {
                return None;
            }
            let found = file.hits.iter().find(|found| found.hit.line == line)?;
            Some((file.path.clone(), found.ranges_in_file[0].clone()))
        });
        if let Some((path, range)) = found {
            self.choose_hit(path, line, range, cx);
        }
    }

    /// A press on a file's chevron folds or unfolds its lines.
    fn toggle_fold(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(file) = self.search.files.iter().find(|file| display_path(&file.path) == path)
        else {
            return;
        };
        let path = file.path.clone();
        match self.search.folded.iter().position(|folded| *folded == path) {
            Some(at) => {
                self.search.folded.remove(at);
            }
            None => self.search.folded.push(path),
        }
        cx.notify();
    }

    /// Flips one of the three toggles and runs the search again, after the pause.
    fn set_search_options(&mut self, options: FindOptions, cx: &mut Context<Self>) {
        if self.search.options != options {
            self.search.options = options;
            self.search_run(DEBOUNCE, cx);
        }
        cx.notify();
    }
}

/// What the replace button says. It is disabled in this build; the label is
/// the one it will carry.
fn replace_label(matches: usize, files: usize, capped: bool) -> String {
    if capped {
        "Too many results — narrow the search".to_owned()
    } else if matches == 0 {
        "Replace all".to_owned()
    } else {
        format!("Replace {matches} in {files} files")
    }
}

/// The Search tab's contents, for a drawer whose fields exist.
pub(super) fn render(
    drawer: &WorktreeDrawer,
    entity: Entity<WorktreeDrawer>,
    theme: &Theme,
    cx: &App,
) -> gpui::AnyElement {
    let search = &drawer.search;
    let Some(fields) = &search.fields else {
        return div().into_any_element();
    };
    let options = search.options;
    let query_empty = fields.query.read(cx).text().is_empty();
    let summary = search.summary(query_empty);
    let toggle = |key: &'static str,
                  icon: IconName,
                  words: &'static str,
                  on: bool,
                  set: fn(&mut FindOptions, bool)| {
        let entity = entity.clone();
        ToggleButton::new(
            (gpui::ElementId::from("search-toggle"), key),
            ToggleItem::new(key).icon(icon).tooltip(words),
            on,
        )
        .size(ControlSize::Sm)
        .on_toggle(move |on, _, cx| {
            let mut next = options;
            set(&mut next, on);
            entity.update(cx, |drawer, cx| drawer.set_search_options(next, cx));
        })
    };
    let field = |input: &Entity<TextInput>| {
        div().flex_1().min_w(px(0.0)).child(Input::new(input).size(ControlSize::Sm))
    };
    let query_row = div()
        .flex_1()
        .min_w(px(280.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(field(&fields.query))
        .child(toggle("case", IconName::CaseSensitive, "Match case", options.case, |next, on| {
            next.case = on
        }))
        .child(toggle("word", IconName::WholeWord, "Whole word", options.word, |next, on| {
            next.word = on
        }))
        .child(toggle("regex", IconName::Regex, "Regular expression", options.regex, |next, on| {
            next.regex = on
        }));
    // Disabled until Replace all exists: it only draws the label it will carry.
    let replace_row = div()
        .flex_1()
        .min_w(px(340.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(field(&fields.replace))
        .child(
            div().flex_none().child(
                Button::new(
                    "search-replace-all",
                    replace_label(search.matches(), search.files.len(), search.capped),
                )
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .disabled(true),
            ),
        );
    let refresh = {
        let entity = entity.clone();
        div().flex_none().child(
            IconButton::new("search-refresh", IconName::RefreshCw)
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .tooltip("Search again")
                .on_click(move |_, _, cx| {
                    entity.update(cx, |drawer, cx| drawer.search_run(Duration::ZERO, cx))
                }),
        )
    };
    let danger = search.error.is_some();
    // The two filters and the refresh with the totals share a row when the pane
    // is wide enough and take a row each when it is not (the top block wraps).
    let filter = |input: &Entity<TextInput>| {
        div().flex_1().min_w(px(120.0)).child(Input::new(input).size(ControlSize::Sm))
    };
    let totals = div()
        .flex_1()
        .min_w(px(120.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(refresh)
        .child(
            div()
                .id("search-summary")
                .debug_selector(|| "search-summary".to_owned())
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(theme.typography.footnote)
                .text_color(if danger { theme.ely.danger } else { theme.ely.fg_subtle })
                .child(summary),
        );
    let top = div()
        .flex_none()
        .w_full()
        .px(px(8.0))
        .py(px(4.0))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(4.0))
        .border_b_1()
        .border_color(theme.ely.border)
        .child(query_row)
        .child(replace_row)
        .child(filter(&fields.include))
        .child(filter(&fields.exclude))
        .child(totals);

    let layout = drawn_layout(&search.files, &search.folded, DRAW_ROWS);
    let hits: Vec<FileHits> = layout
        .files
        .iter()
        .map(|(file, lines)| FileHits {
            path: display_path(&file.path).into(),
            hits: file.hits[..*lines].iter().map(|found| found.hit.clone()).collect(),
        })
        .collect();
    let folded: Vec<SharedString> = search
        .folded
        .iter()
        .map(|path| SharedString::from(display_path(path)))
        .collect();
    let unsaved: Vec<SharedString> = search
        .unsaved
        .iter()
        .map(|path| SharedString::from(display_path(path)))
        .collect();
    let mut panel = SearchPanel::new("drawer-search", hits)
        .without_summary()
        .folded(folded)
        .unsaved(unsaved)
        .on_open({
            let entity = entity.clone();
            move |path, line, _, cx| {
                let path = path.to_string();
                entity.update(cx, |drawer, cx| drawer.press_hit(&path, line, cx));
            }
        })
        .on_fold({
            let entity = entity.clone();
            move |path, _, cx| {
                let path = path.to_string();
                entity.update(cx, |drawer, cx| drawer.toggle_fold(&path, cx));
            }
        });
    if let Some((path, line)) = &search.selected {
        panel = panel.selected(display_path(path), *line);
    }
    let note = if layout.omitted > 0 {
        Some(format!(
            "{} more lines are not drawn. Narrow the search to see them.",
            layout.omitted
        ))
    } else if query_empty {
        Some("Type to search every file git lists in this worktree.".to_owned())
    } else {
        None
    };
    let results = div()
        .id("search-results")
        .debug_selector(|| "search-results".to_owned())
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .px(px(8.0))
        .py(px(4.0))
        .child(panel)
        .children(note.map(|note| {
            div()
                .px(px(4.0))
                .py(px(6.0))
                .text_size(theme.typography.footnote)
                .text_color(theme.ely.fg_subtle)
                .child(note)
        }));
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(top)
        .child(results)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::super::search_engine::FoundHit;
    use super::*;
    use ely_gpui_component::editor::Hit;

    fn file(path: &str, lines: usize) -> FileResult {
        FileResult {
            path: PathBuf::from(path),
            hits: (0..lines)
                .map(|line| FoundHit {
                    hit: Hit { line, text: "x".into(), ranges: vec![0..1] },
                    ranges_in_file: vec![line * 2..line * 2 + 1],
                })
                .collect(),
        }
    }

    fn totals<'a>(skipped: &'a BTreeMap<Skip, usize>) -> Totals<'a> {
        Totals {
            matches: 11,
            files: 5,
            skipped,
            capped: false,
            cancelled: false,
            error: None,
            running: false,
        }
    }

    #[test]
    fn the_summary_names_skips_and_the_cap() {
        let none = BTreeMap::new();
        assert_eq!(summary_line(&totals(&none)), "11 results in 5 files");
        let skipped = BTreeMap::from([(Skip::TooLarge, 1), (Skip::NotUtf8, 2)]);
        let capped = Totals { capped: true, ..totals(&skipped) };
        assert_eq!(
            summary_line(&capped),
            "11 results in 5 files · 3 skipped (too large/not UTF-8) · stopped at 20 000 matches"
        );
    }

    #[test]
    fn an_error_is_the_summary_in_one_line() {
        let none = BTreeMap::new();
        let broken = Totals {
            error: Some("regex parse error:\n    (\n    ^\nerror: unclosed group"),
            ..totals(&none)
        };
        assert_eq!(summary_line(&broken), "error: unclosed group");
    }

    #[test]
    fn a_running_search_is_not_a_finished_one() {
        let none = BTreeMap::new();
        let running = Totals { running: true, ..totals(&none) };
        assert!(summary_line(&running).starts_with("Searching… "));
        let cancelled = Totals { cancelled: true, ..totals(&none) };
        assert!(summary_line(&cancelled).ends_with("cancelled"));
    }

    #[test]
    fn thousands_are_grouped_by_three() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(20_000), "20 000");
        assert_eq!(thousands(1_234_567), "1 234 567");
    }

    #[test]
    fn a_long_list_is_cut_at_the_row_budget() {
        let files = vec![file("a.rs", 3), file("b.rs", 10), file("c.rs", 2)];
        // Budget 6: a's header and 3 lines, b's header and 1 line; c is out.
        let layout = drawn_layout(&files, &[], 6);
        let drawn: Vec<(&str, usize)> = layout
            .files
            .iter()
            .map(|(file, lines)| (file.path.to_str().unwrap(), *lines))
            .collect();
        assert_eq!(drawn, vec![("a.rs", 3), ("b.rs", 1)]);
        assert_eq!(layout.omitted, 9 + 2);
    }

    #[test]
    fn a_folded_file_costs_its_header_and_hides_its_lines() {
        let files = vec![file("a.rs", 3), file("b.rs", 2)];
        let layout = drawn_layout(&files, &[PathBuf::from("a.rs")], 100);
        let drawn: Vec<usize> = layout.files.iter().map(|(_, lines)| *lines).collect();
        assert_eq!(drawn, vec![0, 2]);
        assert_eq!(layout.omitted, 0);
    }

    #[test]
    fn the_nth_drawn_line_skips_folded_files() {
        let mut search = WorktreeSearch::default();
        search.files = vec![file("a.rs", 2), file("b.rs", 2)];
        search.folded = vec![PathBuf::from("a.rs")];
        let (path, line, range) = search.drawn_hit(1).unwrap();
        assert_eq!((path, line, range), (PathBuf::from("b.rs"), 1, 2..3));
        assert!(search.drawn_hit(2).is_none());
    }
}
