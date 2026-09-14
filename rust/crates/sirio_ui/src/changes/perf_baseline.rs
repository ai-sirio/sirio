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
