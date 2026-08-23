//! The History view: `git log` over local refs, laid out as a commit graph.

use std::ops::Range;
use std::path::PathBuf;

use chrono::TimeZone;
use gpui::{
    AppContext as _, Context, Corners, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Path, Render, StatefulInteractiveElement as _, Styled as _, Task, Window,
    canvas, div, fill, point, px, uniform_list,
};
use tiller_git::{CommitRecord, GitLog, GraphRow, LogFilter, layout};
use tiller_theme::Theme;

/// Commits requested per chunk.
const CHUNK: usize = 500;
/// Height of one commit row.
const ROW_HEIGHT: f32 = 26.0;

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
    pub(crate) commits: Vec<CommitRecord>,
    pub(crate) rows: Vec<GraphRow>,
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
            commits: Vec::new(),
            rows: Vec::new(),
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
                    (commits, has_commits)
                })
                .await;
            let (loaded, has_commits) = loaded;
            let _ = this.update(cx, |this, cx| {
                this.load_task = None;
                this.settled = true;
                if generation != this.generation {
                    return;
                }
                match loaded {
                    Ok(commits) => {
                        if commits.len() < CHUNK {
                            this.exhausted = true;
                        }
                        this.commits.extend(commits);
                        this.rows = layout(&this.commits);
                        this.empty_reason = match (this.commits.is_empty(), has_commits) {
                            // Order matters: on a repository with commits *and*
                            // a filter, both arms could fire, and the filter is
                            // the one that explains the emptiness.
                            (true, _) if this.filter.is_filtering() => {
                                Some(EmptyReason::NoMatches)
                            }
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
        self.exhausted = false;
        self.settled = false;
        self.empty_reason = None;
        self.error = None;
        self.pagination_error = None;
        self.load_next_chunk(cx);
    }
}

impl EventEmitter<GitHistoryEvent> for GitHistory {}

impl Render for GitHistory {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let entity = cx.entity();

        if let Some(error) = self.error.clone() {
            let retry_entity = entity.clone();
            return div()
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
                        .text_color(theme.git_conflict)
                        .child(format!("History unavailable: {error}")),
                )
                .child(
                    div()
                        .id("history-retry")
                        .debug_selector(|| "history-retry".to_owned())
                        .px(px(10.0))
                        .py(px(5.0))
                        .rounded(theme.radii.control)
                        .text_color(theme.title)
                        .bg(theme.row_hover)
                        .hover(|style| style.bg(theme.row_hover))
                        .on_click(move |_, _, cx| {
                            retry_entity.update(cx, |history, cx| history.retry(cx));
                        })
                        .child("Retry"),
                )
                .into_any_element();
        }

        if !self.settled {
            return div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.meta)
                .child("Loading history…")
                .into_any_element();
        }

        if let Some(reason) = self.empty_reason {
            let label = match reason {
                EmptyReason::NotARepository => "Not a git repository",
                EmptyReason::NoCommits => "No commits yet",
                EmptyReason::NoMatches => "No commits match the filter",
            };
            return div()
                .id("history-empty")
                .debug_selector(|| "history-empty".to_owned())
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.meta)
                .child(label)
                .into_any_element();
        }

        let commits = self.commits.clone();
        let rows = self.rows.clone();
        let graph_width = graph_width(&rows);
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
                range
                    .filter_map(|index| commits.get(index).zip(rows.get(index)))
                    .map(|(commit, row)| {
                        render_history_row(
                            commit.clone(),
                            row.clone(),
                            graph_width,
                            row_entity.clone(),
                            theme,
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
                    .text_color(theme.git_conflict)
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
                            .text_color(theme.title)
                            .bg(theme.row_hover)
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
    }
}

fn render_history_row(
    commit: CommitRecord,
    row: GraphRow,
    graph_width: f32,
    entity: gpui::Entity<GitHistory>,
    theme: Theme,
) -> impl IntoElement {
    let sha = commit.sha.clone();
    let subject_color = if commit.parents.len() > 1 {
        theme.meta
    } else {
        theme.title
    };
    let date = chrono::Local
        .timestamp_opt(commit.timestamp, 0)
        .single()
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "—".to_owned());
    div()
        .id(format!("history-row-{sha}"))
        .debug_selector(|| "history-row".to_owned())
        .h(px(ROW_HEIGHT))
        .w_full()
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(8.0))
        .hover(|style| style.bg(theme.row_hover))
        .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
            entity.update(cx, |_, cx| {
                cx.emit(GitHistoryEvent::OpenCommit(sha.clone()))
            });
        })
        .child(
            div()
                .w(px(graph_width))
                .h_full()
                .flex_none()
                .child(graph_column(&row, theme)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .text_ellipsis()
                .text_color(subject_color)
                .child(commit.subject),
        )
        .child(
            div()
                .w(px(90.0))
                .flex_none()
                .overflow_hidden()
                .text_ellipsis()
                .text_color(theme.meta)
                .child(commit.author),
        )
        .child(
            div()
                .w(px(72.0))
                .flex_none()
                .text_color(theme.meta)
                .child(date),
        )
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
    use gpui::{AppContext, TestAppContext};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tiller_theme::Theme;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tiller-right-panel-history-test-{}-{unique}",
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
        assert!(node_is_drawable(&inside), "the last lane inside the cap still draws");
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
            history.read_with(cx, |history, _| history.settled && history.commits.is_empty())
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
            history.read_with(cx, |history, _| history.settled && history.commits.len() == 1)
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
}
