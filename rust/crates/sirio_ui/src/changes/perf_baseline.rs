//! S1 baseline probes and synthetic workloads, compiled only for unit tests.
//! A probe owns its counts. The thread-local slot merely routes synchronous
//! builder calls to the active fixture; nested scopes restore the previous
//! probe, including during unwinding. Never hold a scope across `.await`.
//! No runtime feature, telemetry, cache, or scheduling change is involved.

use super::*;
use gpui::{TestAppContext, VisualTestContext, size};
use std::cell::RefCell;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Counts {
    render_body: usize,
    section_builds: usize,
    list_builds: usize,
    flattened_rows: usize,
    hashed_rows: usize,
    splices: usize,
    payload_calls: usize,
    payload_bytes: usize,
    draw_payload_copies: usize,
    draw_payload_bytes: usize,
}

#[derive(Clone, Default)]
struct Probe(Rc<RefCell<Counts>>);

thread_local! {
    static ACTIVE: RefCell<Option<Probe>> = const { RefCell::new(None) };
}

struct Scope(Option<Probe>);

impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.with(|active| *active.borrow_mut() = self.0.take());
    }
}

impl Probe {
    fn enter(&self) -> Scope {
        Scope(ACTIVE.with(|active| active.replace(Some(self.clone()))))
    }

    fn counts(&self) -> Counts {
        *self.0.borrow()
    }

    fn reset(&self) {
        *self.0.borrow_mut() = Counts::default();
    }
}

fn record(update: impl FnOnce(&mut Counts)) {
    ACTIVE.with(|active| {
        if let Some(probe) = active.borrow().as_ref() {
            update(&mut probe.0.borrow_mut());
        }
    });
}

pub(super) fn render_body() {
    record(|counts| counts.render_body += 1);
}

pub(super) fn section_build() {
    record(|counts| counts.section_builds += 1);
}

pub(super) fn list_build(rows: usize) {
    record(|counts| {
        counts.list_builds += 1;
        counts.flattened_rows += rows;
        counts.hashed_rows += rows;
    });
}

pub(super) fn splice() {
    record(|counts| counts.splices += 1);
}

pub(super) fn payload_call() {
    record(|counts| counts.payload_calls += 1);
}

pub(super) fn payload_bytes(bytes: usize) {
    record(|counts| counts.payload_bytes += bytes);
}

// Counts only the existing row.clone() on the list's drawing path, not
// every possible Clone in the application and not Rc/reference copies.
pub(super) fn draw_row_clone(row: &ChangeRow) {
    if let ChangeRow::File {
        drag_payload: Some((_, text)),
        ..
    } = row
    {
        record(|counts| {
            counts.draw_payload_copies += 1;
            counts.draw_payload_bytes += text.len();
        });
    }
}

fn synthetic_files_tab(files: usize, lines: usize, expanded: usize) -> ChangesTab {
    assert!(files > 0 && expanded <= files);
    let mut tab = super::tests::synthetic_big_diff_tab(lines);
    let original_entry = tab.entries[0].clone();
    let original_diff = tab.diffs[&original_entry.path].clone();
    let original_stat = tab.stats[&original_entry.path];
    tab.entries.clear();
    tab.diffs.clear();
    tab.stats.clear();
    tab.expanded_changes.clear();
    for i in 0..files {
        let path = PathBuf::from(format!("src/file_{i:03}.rs"));
        let mut entry = original_entry.clone();
        entry.path = path.clone();
        let mut diff = original_diff.clone();
        diff.path = path.clone();
        tab.entries.push(entry);
        tab.diffs.insert(path.clone(), diff);
        tab.stats.insert(path.clone(), original_stat);
        if i < expanded {
            tab.expanded_changes.insert((ChangeSection::Staged, path));
        }
    }
    tab
}

fn percentile_ns(samples: &[u128], percentile: usize) -> u128 {
    assert!(!samples.is_empty());
    assert!((1..=100).contains(&percentile));
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    sorted[(sorted.len() * percentile).div_ceil(100) - 1]
}

#[test]
fn percentile_uses_nearest_rank_without_dropping_slow_samples() {
    assert_eq!(percentile_ns(&[40, 10, 30, 20], 50), 20);
    assert_eq!(percentile_ns(&[40, 10, 30, 20], 95), 40);
    assert_eq!(percentile_ns(&[40, 10, 30, 20], 100), 40);
    assert_eq!(percentile_ns(&[7], 1), 7);
}

#[test]
fn two_builder_calls_are_counted_and_fixture_reset_is_local() {
    let first = Probe::default();
    let second = Probe::default();
    let tab = synthetic_files_tab(1, 50, 1);
    {
        let _scope = first.enter();
        tab.section_rows(DiffViewMode::Unified);
        tab.section_rows(DiffViewMode::Unified);
    }
    {
        let _scope = second.enter();
        tab.section_rows(DiffViewMode::Unified);
    }
    assert_eq!(first.counts().section_builds, 2);
    assert_eq!(second.counts().section_builds, 1);
    first.reset();
    assert_eq!(first.counts(), Counts::default());
    assert_eq!(second.counts().section_builds, 1);
    tab.section_rows(DiffViewMode::Unified); // No scope: neither fixture changes.
    assert_eq!(second.counts().section_builds, 1);
}

#[test]
fn nested_probe_restores_outer_fixture_after_unwind() {
    let outer = Probe::default();
    let inner = Probe::default();
    let tab = synthetic_files_tab(1, 50, 1);
    let _scope = outer.enter();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _scope = inner.enter();
        tab.section_rows(DiffViewMode::Unified);
        panic!("exercise scope restoration");
    }));
    assert!(result.is_err());
    tab.section_rows(DiffViewMode::Unified);
    assert_eq!(inner.counts().section_builds, 1);
    assert_eq!(outer.counts().section_builds, 1);
}

#[test]
fn payload_builder_counts_direct_calls_and_exact_bytes_including_binary() {
    let tab = synthetic_files_tab(1, 50, 1);
    let mut diff = tab.diffs.values().next().unwrap().clone();
    let expected = diff_payload(&diff).unwrap().1;
    let probe = Probe::default();
    let _scope = probe.enter();
    assert_eq!(diff_payload(&diff).unwrap().1, expected);
    assert_eq!(diff_payload(&diff).unwrap().1, expected);
    diff.is_binary = true;
    assert!(diff_payload(&diff).is_none());
    assert_eq!(probe.counts().payload_calls, 3);
    assert_eq!(probe.counts().payload_bytes, expected.len() * 2);
}

#[test]
fn multifile_fixture_preserves_counts_and_duplicate_path_sections() {
    let mut tab = synthetic_files_tab(300, 50, 0);
    tab.entries[0].worktree_status = Some(StatusKind::Modified);
    let probe = Probe::default();
    let _scope = probe.enter();
    let rows = tab.sync_list_rows(tab.section_rows(DiffViewMode::Unified));
    // 300 staged files, one changed file with the same path, two headers.
    assert_eq!(rows.len(), 303);
    assert_eq!(probe.counts().payload_calls, 301);
    assert_eq!(probe.counts().flattened_rows, 303);
    assert_eq!(probe.counts().hashed_rows, 303);
    assert_eq!(probe.counts().splices, 1);
    tab.sync_list_rows(tab.section_rows(DiffViewMode::Unified));
    assert_eq!(probe.counts().list_builds, 2);
    assert_eq!(probe.counts().splices, 1); // Same geometry already avoids splice.
}

#[gpui::test]
async fn drawn_frames_count_rebuilds_and_payload_copies_separately(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let window = cx.add_window(|_, _| synthetic_files_tab(1, 300, 1));
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1200.0), px(800.0)));
    let tab = cx.update(|window, _| window.root::<ChangesTab>().flatten().unwrap());
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let probe = Probe::default();
    {
        let _scope = probe.enter();
        for _ in 0..2 {
            cx.update(|window, cx| {
                tab.update(cx, |_, cx| cx.notify());
                window.draw(cx).clear(cx);
            });
        }
    }
    let counts = probe.counts();
    assert_eq!(counts.render_body, 2);
    assert_eq!(counts.section_builds, 2);
    assert_eq!(counts.list_builds, 2);
    assert_eq!(counts.payload_calls, 2);
    assert_eq!(counts.splices, 0);
    assert!(counts.draw_payload_copies >= 2);
    assert!(counts.draw_payload_bytes > 0);
    cx.update(|window, _| window.remove_window());
}

#[test]
fn multifile_expansion_matrix_keeps_all_payloads_but_changes_row_geometry() {
    for expanded in [0, 1, 300] {
        let tab = synthetic_files_tab(300, 50, expanded);
        let probe = Probe::default();
        let _scope = probe.enter();
        let sections = tab.section_rows(DiffViewMode::Unified);
        assert_eq!(sections[0].rows.len(), 300 + expanded * 51);
        assert_eq!(probe.counts().payload_calls, 300);
    }
}

impl Counts {
    fn metrics(self) -> [(&'static str, usize); 10] {
        [
            ("render_body", self.render_body),
            ("section_builds", self.section_builds),
            ("list_builds", self.list_builds),
            ("flattened_rows", self.flattened_rows),
            ("hashed_rows", self.hashed_rows),
            ("splices", self.splices),
            ("payload_calls", self.payload_calls),
            ("payload_bytes", self.payload_bytes),
            ("draw_payload_copies", self.draw_payload_copies),
            ("draw_payload_bytes", self.draw_payload_bytes),
        ]
    }
}

struct Case {
    name: &'static str,
    files: usize,
    lines: usize,
    expanded: usize,
    hide_header: bool,
}

const CASES: [Case; 7] = [
    Case {
        name: "single_300",
        files: 1,
        lines: 300,
        expanded: 1,
        hide_header: false,
    },
    Case {
        name: "single_5000",
        files: 1,
        lines: 5_000,
        expanded: 1,
        hide_header: false,
    },
    Case {
        name: "single_15000",
        files: 1,
        lines: 15_000,
        expanded: 1,
        hide_header: false,
    },
    Case {
        name: "single_15000_header_hidden",
        files: 1,
        lines: 15_000,
        expanded: 1,
        hide_header: true,
    },
    Case {
        name: "files_300_collapsed",
        files: 300,
        lines: 50,
        expanded: 0,
        hide_header: false,
    },
    Case {
        name: "files_300_one_expanded",
        files: 300,
        lines: 50,
        expanded: 1,
        hide_header: false,
    },
    Case {
        name: "files_300_all_expanded",
        files: 300,
        lines: 50,
        expanded: 300,
        hide_header: false,
    },
];

const WARMUP: usize = 10;
const SAMPLES: usize = 1_000;
const REPEATS: usize = 5;

struct Measurement {
    samples: Vec<u128>,
    counts: Counts,
}

type TimedOperation = fn(&mut VisualTestContext, &gpui::Entity<ChangesTab>) -> u128;

fn prepare_rows(cx: &mut VisualTestContext, tab: &gpui::Entity<ChangesTab>) -> u128 {
    tab.update(&mut cx.cx, |tab, _| {
        let start = std::time::Instant::now();
        // Includes materialization, flatten/hash, and temporary-row destruction.
        drop(tab.sync_list_rows(tab.section_rows(DiffViewMode::Unified)));
        start.elapsed().as_nanos()
    })
}

fn draw_frame(cx: &mut VisualTestContext, tab: &gpui::Entity<ChangesTab>) -> u128 {
    cx.update(|window, cx| {
        tab.update(cx, |_, cx| cx.notify());
        let start = std::time::Instant::now();
        window.draw(cx).clear(cx);
        start.elapsed().as_nanos()
    })
}

fn measure(
    cx: &mut VisualTestContext,
    tab: &gpui::Entity<ChangesTab>,
    operation: TimedOperation,
    enabled: bool,
) -> Measurement {
    let probe = Probe::default();
    let _scope = enabled.then(|| probe.enter());
    for _ in 0..WARMUP {
        operation(cx, tab);
    }
    probe.reset();
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        samples.push(operation(cx, tab));
    }
    Measurement {
        samples,
        counts: probe.counts(),
    }
}

struct Export {
    directory: PathBuf,
    revision: String,
    host: String,
}

impl Export {
    fn from_env() -> Self {
        let directory = PathBuf::from(
            std::env::var("SIRIO_PERF_SAMPLE_DIR")
                .expect("set SIRIO_PERF_SAMPLE_DIR to an external artifact directory"),
        );
        assert!(
            directory.is_absolute(),
            "artifact directory must be absolute"
        );
        std::fs::create_dir_all(&directory).expect("create artifact directory");
        let revision = std::env::var("SIRIO_PERF_REVISION")
            .expect("set SIRIO_PERF_REVISION to the base SHA plus patch identity");
        let host =
            std::env::var("SIRIO_PERF_HOST").expect("set SIRIO_PERF_HOST to a synthetic host id");
        for id in [&revision, &host] {
            assert!(
                !id.is_empty()
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.+".contains(&b)),
                "CSV identifiers must contain only letters, digits, or -_.+"
            );
        }
        Self {
            directory,
            revision,
            host,
        }
    }

    // Called after timing finishes. Numeric samples only; no paths, diff text,
    // prompts, or user terminal contents. Never overwrite a previous run.
    fn save(
        &self,
        case: &Case,
        repeat: usize,
        enabled: bool,
        metric: &str,
        measured: &Measurement,
    ) {
        use std::io::Write;
        let probe = if enabled { "active" } else { "inactive" };
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let file = self
            .directory
            .join(format!("{}-{repeat}-{probe}-{metric}.csv", case.name));
        let mut out = std::io::BufWriter::new(
            std::fs::File::create_new(file).expect("create new sample file"),
        );
        writeln!(
            out,
            "revision,profile,features,host_id,scenario,repeat,probe,sample,metric,unit,value"
        )
        .unwrap();
        let prefix = format!(
            "{},{profile},default,{},{},{repeat},{probe}",
            self.revision, self.host, case.name
        );
        for (sample, value) in measured.samples.iter().enumerate() {
            writeln!(out, "{prefix},{},{metric},ns,{value}", sample + 1).unwrap();
        }
        // Sample 0 denotes a total over all measured operations, not a frame.
        // Inactive totals are omitted: unobserved is not zero work.
        if enabled {
            for (name, value) in measured.counts.metrics() {
                let unit = if name.ends_with("bytes") {
                    "bytes"
                } else {
                    "count"
                };
                writeln!(out, "{prefix},0,{name},{unit},{value}").unwrap();
            }
        }
        out.flush().unwrap();
        eprintln!(
            "[S1-baseline] {} repeat={repeat} probe={probe} metric={metric} n={} p50_ns={} p95_ns={} p99_ns={} max_ns={} counts={:?}",
            case.name,
            measured.samples.len(),
            percentile_ns(&measured.samples, 50),
            percentile_ns(&measured.samples, 95),
            percentile_ns(&measured.samples, 99),
            percentile_ns(&measured.samples, 100),
            measured.counts
        );
    }
}

fn measure_case(cx: &mut TestAppContext, case: &Case, export: &Export) {
    let window = cx.add_window(|_, _| synthetic_files_tab(case.files, case.lines, case.expanded));
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1200.0), px(800.0)));
    let tab = cx.update(|window, _| window.root::<ChangesTab>().flatten().unwrap());
    draw_frame(&mut cx, &tab);
    if case.hide_header {
        tab.update(&mut cx.cx, |tab, _| {
            tab.list_state.scroll_to_reveal_item(1_000)
        });
    }
    for repeat in 1..=REPEATS {
        // Alternate inactive/active order. This measures collector overhead,
        // not a before/after optimization: both sides are the same R1 code.
        let order = if repeat % 2 == 1 {
            [false, true]
        } else {
            [true, false]
        };
        for enabled in order {
            for (metric, operation) in [
                ("prepare", prepare_rows as TimedOperation),
                ("draw", draw_frame as TimedOperation),
            ] {
                let measured = measure(&mut cx, &tab, operation, enabled);
                if enabled {
                    assert_eq!(measured.counts.section_builds, SAMPLES);
                    assert_eq!(measured.counts.payload_calls, SAMPLES * case.files);
                    if metric == "draw" {
                        assert_eq!(measured.counts.render_body, SAMPLES);
                        if case.hide_header {
                            assert_eq!(measured.counts.draw_payload_copies, 0);
                        } else {
                            assert!(measured.counts.draw_payload_copies >= SAMPLES);
                        }
                    }
                }
                export.save(case, repeat, enabled, metric, &measured);
            }
        }
    }
    cx.update(|window, _| window.remove_window());
}

/// Run serially, in release, with the three SIRIO_PERF_* export variables.
/// This preserves the historical minimum-of-five test unchanged. It measures
/// warm GPUI test draws, NOT display presentation or key-to-photon latency.
#[gpui::test]
#[ignore = "diagnostic: five repeats of 1000 samples; requires external artifact directory"]
async fn changes_hot_baseline(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let export = Export::from_env();
    for case in &CASES {
        measure_case(cx, case, &export);
    }
}
