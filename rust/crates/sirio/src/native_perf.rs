//! Opt-in export of GPUI's bounded foreground journal. Never records input text.

use gpui::profiler::journal::{
    ForegroundEvent, ForegroundJournalEntry, FrameStateChange, IntervalBoundary,
};
use perf_scheduler::Instant;
use serde_json::{Value, json};
use std::io::{self, BufWriter, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn relative_ns(time: Instant, origin: Instant) -> i128 {
    if time >= origin {
        time.duration_since(origin).as_nanos() as i128
    } else {
        -(origin.duration_since(time).as_nanos() as i128)
    }
}

fn encode_entries(entries: &[ForegroundJournalEntry], origin: Instant) -> Vec<Value> {
    let ns = |time| relative_ns(time, origin);
    entries.iter().map(|entry| match entry {
        ForegroundJournalEntry::Event(event) => {
            let mut row = json!({"start_ns": ns(event.start_time()), "end_ns": ns(event.end_time())});
            match event {
                ForegroundEvent::Input(input) => {
                    row["kind"] = json!("input");
                    row["input_kind"] = json!(input.kind);
                    row["invalidated"] = json!(input.caused_invalidation);
                }
                ForegroundEvent::TaskPoll(task) => {
                    row["kind"] = json!("task");
                    row["location"] = json!(format!("{}:{}:{}", task.location.file(), task.location.line(), task.location.column()));
                }
                ForegroundEvent::Action(_) => row["kind"] = json!("action"),
                ForegroundEvent::Draw(frame) => {
                    row["kind"] = json!("draw");
                    row["window"] = json!(frame.window_id.as_u64());
                    row["dirty_ns"] = json!(frame.dirty_at.map(ns));
                    row["invalidations"] = json!(frame.invalidations);
                }
                ForegroundEvent::Present(present) => {
                    row["kind"] = json!("present");
                    row["window"] = json!(present.window_id.as_u64());
                }
                ForegroundEvent::SmallPolls(polls) => {
                    row["kind"] = json!("small_polls");
                    row["count"] = json!(polls.summary.count);
                    row["total_ns"] = json!(polls.summary.total.as_nanos());
                }
            }
            row
        }
        ForegroundJournalEntry::Discontinuity { lost } => json!({"kind": "lost", "count": lost}),
        ForegroundJournalEntry::Boundary(IntervalBoundary::Idle { ended_at }) => json!({"kind": "idle", "at_ns": ns(*ended_at)}),
        ForegroundJournalEntry::Boundary(IntervalBoundary::Presented(presented)) => json!({
            "kind": "presented", "window": presented.frame.window_id.as_u64(),
            "dirty_ns": presented.frame.dirty_at.map(ns), "invalidations": presented.frame.invalidations,
            "draw_start_ns": ns(presented.frame.draw_start), "draw_end_ns": ns(presented.frame.draw_end),
            "present_start_ns": ns(presented.presentation.present_start), "present_end_ns": ns(presented.presentation.present_end),
        }),
        ForegroundJournalEntry::FrameState(FrameStateChange::Pending { window_id, dirty_at }) => json!({"kind": "pending", "window": window_id.as_u64(), "at_ns": ns(*dirty_at)}),
        ForegroundJournalEntry::FrameState(FrameStateChange::Closed { window_id, at }) => json!({"kind": "closed", "window": window_id.as_u64(), "at_ns": ns(*at)}),
    }).collect()
}

fn write_row(writer: &mut impl Write, row: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, row)?;
    writer.write_all(b"\n")
}

fn encode_task_snapshot(
    timings: Vec<gpui::ThreadTaskTimings>,
    foreground: std::thread::ThreadId,
    origin: Instant,
    started: Instant,
    finished: Instant,
) -> Value {
    let mut snapshot = json!({
        "capture_start_ns": relative_ns(started, origin),
        "capture_end_ns": relative_ns(finished, origin),
        "foreground_found": false,
        "tasks": [],
    });
    if let Some(thread) = timings
        .into_iter()
        .find(|thread| thread.thread_id == foreground)
    {
        snapshot["foreground_found"] = json!(true);
        snapshot["total_pushed"] = json!(thread.total_pushed);
        snapshot["retained"] = json!(thread.timings.len());
        snapshot["lost"] = json!(
            thread
                .total_pushed
                .saturating_sub(thread.timings.len() as u64)
        );
        snapshot["tasks"] = json!(thread.timings.iter().map(|task| json!({
            "start_ns": relative_ns(task.start, origin),
            "end_ns": relative_ns(task.end.0, origin),
            "location": format!("{}:{}:{}", task.location.file(), task.location.line(), task.location.column()),
        })).collect::<Vec<_>>());
    }
    snapshot
}

fn capture_task_snapshot(
    path: &std::path::Path,
    foreground: std::thread::ThreadId,
    origin: Instant,
    epoch_ns: u128,
) -> io::Result<()> {
    let _perf = sirio_perf::span("profiler.task_snapshot", 0);
    let started = Instant::now();
    let timings = gpui::profiler::get_all_timings(gpui::TasksIncluded::OnlyCompleted);
    let finished = Instant::now();
    let mut snapshot = encode_task_snapshot(timings, foreground, origin, started, finished);
    snapshot["schema"] = json!("sirio-tasks-v1");
    snapshot["pid"] = json!(std::process::id());
    snapshot["epoch_ns"] = json!(epoch_ns);
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let mut writer = BufWriter::new(file);
    write_row(&mut writer, &snapshot)?;
    writer.flush()
}

/// No foreground timer, observer, or redraw. The reader drains the existing
/// bounded ring on a separate thread. A health row after a measurement's end
/// is required: abrupt process exit is not a promise of a complete final drain.
pub fn init(cx: &gpui::App) {
    if !sirio_perf::enabled() {
        return;
    }
    let Some(path) = std::env::var_os("SIRIO_PERF_NATIVE_TRACE") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let task_snapshot_path = (std::env::var("SIRIO_PERF_TASK_SNAPSHOT").as_deref() == Ok("1"))
        .then(|| path.with_extension("tasks.json"));
    let foreground = std::thread::current().id();
    let start = || -> io::Result<()> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        let origin = Instant::now();
        let epoch_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut collector = cx.foreground_journal().collector();
        let mut writer = BufWriter::new(file);
        write_row(
            &mut writer,
            &json!({"kind": "header", "schema": "sirio-native-v1", "pid": std::process::id(), "epoch_ns": epoch_ns}),
        )?;
        writer.flush()?;
        std::thread::Builder::new().name("sirio-native-perf".into()).spawn(move || {
            let mut task_snapshot_path = task_snapshot_path;
            let mut sequence = 0_u64;
            let mut lost = 0_u64;
            let mut drain = || -> io::Result<()> {
                // Time captured before the drain is a conservative coverage bound.
                let at_ns = relative_ns(Instant::now(), origin);
                let batch = collector.collect_unseen();
                lost += batch.lost;
                for mut row in encode_entries(&batch.entries, origin) {
                    row["seq"] = json!(sequence);
                    sequence += 1;
                    write_row(&mut writer, &row)?;
                }
                write_row(&mut writer, &json!({"kind": "health", "at_ns": at_ns, "lost": lost, "entries": sequence}))?;
                writer.flush()
            };
            loop {
                if let Err(error) = drain() {
                    eprintln!("[perf-native] export failed: {error}");
                    break;
                }
                // A one-shot snapshot requested after the measurement, never a UI task.
                if let Some(path) = task_snapshot_path.as_ref()
                    && path.with_extension("request").is_file()
                {
                    if let Err(error) = capture_task_snapshot(path, foreground, origin, epoch_ns) {
                        eprintln!("[perf-native] task snapshot failed: {error}");
                    }
                    task_snapshot_path = None;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })?;
        gpui::profiler::set_trace_enabled(true);
        Ok(())
    };
    if let Err(error) = start() {
        eprintln!("[perf-native] startup failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::profiler::journal::{ForegroundEvent, InputTiming, IntervalBoundary};
    use std::time::Duration;

    #[test]
    fn task_snapshot_keeps_short_foreground_polls_and_reports_overflow() {
        let origin = Instant::now();
        let foreground = std::thread::current().id();
        let other = std::thread::spawn(|| std::thread::current().id())
            .join()
            .unwrap();
        let mut short = gpui::TaskTiming::placeholder();
        short.location = std::panic::Location::caller();
        short.start = origin - Duration::from_nanos(10);
        short.end = gpui::YieldTime(origin + Duration::from_nanos(69_790));
        let snapshot = encode_task_snapshot(
            vec![
                gpui::ThreadTaskTimings {
                    thread_name: Some("private-background-name".into()),
                    thread_id: other,
                    timings: vec![short; 2],
                    stats: Default::default(),
                    total_pushed: 2,
                },
                gpui::ThreadTaskTimings {
                    thread_name: Some("private-foreground-name".into()),
                    thread_id: foreground,
                    timings: vec![short],
                    stats: Default::default(),
                    total_pushed: 3,
                },
            ],
            foreground,
            origin,
            origin + Duration::from_millis(1),
            origin + Duration::from_millis(2),
        );
        assert_eq!(snapshot["tasks"].as_array().unwrap().len(), 1);
        assert_eq!(snapshot["foreground_found"], true);
        assert_eq!(snapshot["total_pushed"], 3);
        assert_eq!(snapshot["retained"], 1);
        assert_eq!(snapshot["lost"], 2);
        assert_eq!(snapshot["capture_start_ns"], 1_000_000);
        assert_eq!(snapshot["capture_end_ns"], 2_000_000);
        assert_eq!(snapshot["tasks"][0]["start_ns"], -10);
        assert_eq!(snapshot["tasks"][0]["end_ns"], 69_790);
        assert_eq!(
            snapshot["tasks"][0]["location"],
            format!(
                "{}:{}:{}",
                short.location.file(),
                short.location.line(),
                short.location.column()
            )
        );
        assert!(!snapshot.to_string().contains("private-"));
    }

    #[test]
    fn task_snapshot_reads_the_live_profiler_and_refuses_overwrite() {
        const CHILD: &str = "SIRIO_PERF_TASK_TEST_CHILD";
        if let Some(path) = std::env::var_os(CHILD) {
            let origin = Instant::now();
            gpui::profiler::set_trace_enabled(true);
            gpui::profiler::update_running_task(
                perf_scheduler::SpawnTime(origin),
                std::panic::Location::caller(),
            );
            gpui::profiler::save_task_timing();
            let path = std::path::Path::new(&path);
            capture_task_snapshot(path, std::thread::current().id(), origin, 100).unwrap();
            assert_eq!(
                capture_task_snapshot(path, std::thread::current().id(), origin, 100)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::AlreadyExists
            );
            return;
        }
        let path = std::env::temp_dir().join(format!(
            "sirio-task-snapshot-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native_perf::tests::task_snapshot_reads_the_live_profiler_and_refuses_overwrite",
                "--nocapture",
            ])
            .env(CHILD, &path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let snapshot: Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(snapshot["schema"], "sirio-tasks-v1");
        assert_eq!(snapshot["foreground_found"], true);
        assert_eq!(snapshot["lost"], 0);
        assert_eq!(snapshot["retained"], 1);
        assert_eq!(snapshot["tasks"].as_array().unwrap().len(), 1);
        assert!(
            snapshot["tasks"][0]["location"]
                .as_str()
                .unwrap()
                .contains("native_perf.rs:")
        );
    }

    #[test]
    fn journal_export_preserves_completion_order_and_loss_boundaries() {
        let origin = Instant::now();
        let mut outer = gpui::TaskTiming::placeholder();
        outer.start = origin + Duration::from_nanos(10);
        outer.end = gpui::YieldTime(origin + Duration::from_nanos(40));
        let entries = [
            ForegroundJournalEntry::Event(ForegroundEvent::Input(InputTiming {
                kind: "KeyDown",
                start: origin + Duration::from_nanos(20),
                end: origin + Duration::from_nanos(30),
                caused_invalidation: true,
            })),
            ForegroundJournalEntry::Discontinuity { lost: 7 },
            ForegroundJournalEntry::Boundary(IntervalBoundary::Idle {
                ended_at: origin + Duration::from_nanos(35),
            }),
            ForegroundJournalEntry::Event(ForegroundEvent::TaskPoll(outer)),
        ];
        let rows = encode_entries(&entries, origin);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0]["kind"], "input");
        assert_eq!(rows[0]["start_ns"], 20);
        assert_eq!(rows[0]["end_ns"], 30);
        assert_eq!(rows[0]["invalidated"], true);
        assert_eq!(rows[1]["kind"], "lost");
        assert_eq!(rows[1]["count"], 7);
        assert_eq!(rows[2]["kind"], "idle");
        assert_eq!(rows[3]["start_ns"], 10);
        assert_eq!(rows[3]["end_ns"], 40);
        let mut keys: Vec<_> = rows[0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["end_ns", "input_kind", "invalidated", "kind", "start_ns"]
        );
    }

    #[test]
    fn journal_export_keeps_matched_draw_and_present_metadata() {
        use gpui::profiler::journal::PresentedFrame;
        let origin = Instant::now();
        let frame = gpui::FrameTiming {
            window_id: 1.into(),
            dirty_at: Some(origin - Duration::from_nanos(5)),
            invalidations: 3,
            draw_start: origin + Duration::from_nanos(10),
            draw_end: origin + Duration::from_nanos(20),
        };
        let presentation = gpui::PresentTiming {
            window_id: frame.window_id,
            present_start: origin + Duration::from_nanos(30),
            present_end: origin + Duration::from_nanos(40),
            animation_interval: None,
        };
        let rows = encode_entries(
            &[ForegroundJournalEntry::Boundary(
                IntervalBoundary::Presented(PresentedFrame {
                    frame,
                    presentation,
                }),
            )],
            origin,
        );
        assert_eq!(rows[0]["kind"], "presented");
        assert_eq!(rows[0]["dirty_ns"], -5);
        assert_eq!(rows[0]["invalidations"], 3);
        assert_eq!(rows[0]["draw_start_ns"], 10);
        assert_eq!(rows[0]["present_end_ns"], 40);
    }

    #[test]
    fn journal_write_failure_is_not_reported_as_success() {
        let error = write_row(
            &mut io::Cursor::new(&mut [0_u8; 1][..]),
            &json!({"kind": "health"}),
        );
        assert!(error.is_err());
    }
}
