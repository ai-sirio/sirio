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

#[gpui::test]
async fn drawn_frames_count_rebuilds_and_payload_copies_separately(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    cx.update(crate::ely::init);
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
