//! DIAGNOSTIC -- run by hand:
//! `cargo test -p sirio_terminal --test still_frame_cost -- --ignored --nocapture`
//!
//! Times what a window pays to draw a terminal pane whose content is not
//! changing: every frame of a still pane, end to end through gpui's test
//! window, at the size `add_window_view` gives it. This is the number the
//! retained grid (`GridRenderCache`) is meant to drive towards "one integer
//! comparison per frame"; `perf_grid_rebuild_per_frame` in the crate's own
//! tests times the snapshot round trip itself and cannot see whether the
//! frame still asks for one.
//!
//! An integration test on purpose: it uses only the crate's public surface,
//! so the same file runs unchanged against an older revision of the crate in
//! a worktree, which is how the before/after numbers are produced. Ignored
//! because it measures wall clock.
//!
//! Measured 2026-09-01 (Linux, debug profile, 200 frames, the same box for
//! both): **393.7 ms per frame** at `6bf69f65`, the last commit before the
//! fluidity stack, where every frame paid the owner-thread snapshot round
//! trip plus a per-cell rebuild of runs and quads; **9.09 ms per frame** at
//! `b9fb9bc3`, with the retained grid answering a still frame from cache.
//! Debug numbers overstate the absolute cost (the old path's per-cell work is
//! what debug builds punish most); the ratio is the point.

use std::time::{Duration, Instant};

use sirio_terminal::{TerminalShell, TerminalView};
use sirio_theme::Theme;

#[gpui::test]
#[ignore = "diagnostic: wall-clock measurement of a still pane's per-frame draw cost"]
async fn still_pane_frame_cost(cx: &mut gpui::TestAppContext) {
    cx.set_global(Theme::light());
    let working_directory =
        std::env::temp_dir().join(format!("sirio-terminal-still-frame-{}", std::process::id()));
    std::fs::create_dir_all(&working_directory).expect("create PTY directory");
    let shell = TerminalShell::WithArguments {
        program: "/bin/sh".to_string(),
        args: vec![
            "-c".to_string(),
            // Real, attributed content on every visible row, then silence:
            // the pane must be still while the frames are timed.
            "for i in $(seq 1 4000); do printf 'line %s \\033[1;31mbold red\\033[0m plain text here\\n' \"$i\"; done; exec sleep 60".to_string(),
        ],
    };
    let (terminal, cx) = cx.add_window_view(|_, cx| {
        TerminalView::with_shell(&working_directory, shell, cx).expect("spawn PTY")
    });

    // Let the output land and settle: the last line must be present and the
    // captured scrollback unchanged across three consecutive samples.
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut previous = Vec::new();
    let mut stable_samples = 0;
    while Instant::now() < deadline {
        cx.run_until_parked();
        cx.background_executor
            .advance_clock(Duration::from_millis(5));
        cx.run_until_parked();
        let scrollback = terminal.read_with(&cx.cx, |terminal, _| terminal.capture_scrollback());
        let complete = String::from_utf8_lossy(&scrollback).contains("line 4000");
        if complete && scrollback == previous {
            stable_samples += 1;
        } else {
            stable_samples = 0;
        }
        previous = scrollback;
        if stable_samples >= 3 {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(stable_samples >= 3, "the pane content never settled");
    // The initial resize is debounced; give it time to reach the owner
    // thread so the timed frames see a pane whose geometry is final too.
    std::thread::sleep(Duration::from_millis(200));
    cx.run_until_parked();

    const WARM_FRAMES: u32 = 10;
    const FRAMES: u32 = 200;
    for _ in 0..WARM_FRAMES {
        terminal.update(&mut cx.cx, |_, cx| cx.notify());
        cx.run_until_parked();
    }
    let started = Instant::now();
    for _ in 0..FRAMES {
        terminal.update(&mut cx.cx, |_, cx| cx.notify());
        cx.run_until_parked();
    }
    let per_frame = started.elapsed() / FRAMES;
    println!("still pane: {per_frame:?} per frame over {FRAMES} frames");

    terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
    cx.run_until_parked();
    let _ = std::fs::remove_dir_all(working_directory);
}
