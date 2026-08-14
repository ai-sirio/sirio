// F-TERM-SCR-02 verification — critic2, 2026-08-14.
//
// These two tests were temporarily appended to
// rust/crates/tiller_terminal/src/lib.rs (one into `mod tests`, right after
// `resize_reaches_the_child_pty`; one into `mod view_tests`, at the end),
// run via `cargo test -p tiller_terminal critic2_ -- --nocapture
// --test-threads=1` from rust/, confirmed passing 3x in a row (see
// critic2-scr02-debounce-test.log), then removed again — the working tree
// already had substantial unrelated uncommitted changes in this file
// (someone else's in-progress work), so the tests were never committed and
// were surgically deleted afterward rather than left in or `git checkout`'d.
// Kept here so the exact instrument is replayable.

// --- into `mod tests` (PtyTerminal/TerminalHandle-level, no GPUI harness) ---

#[test]
fn critic2_rapid_resizes_coalesce_to_one_pty_winch() {
    let working_directory = std::env::temp_dir().join(format!(
        "tiller-terminal-critic2-resize-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&working_directory).unwrap();
    let shell = TerminalShell::WithArguments {
        program: "/bin/sh".to_string(),
        args: vec!["-i".to_string()],
    };
    let (handle, _wakeup_rx) = TerminalHandle::new(&working_directory, &shell).unwrap();
    handle.write(b"trap 'echo GOT_WINCH' WINCH\n".to_vec());
    std::thread::sleep(Duration::from_millis(150)); // let the interactive shell register the trap
    // Five resizes, 15ms apart (75ms span), all well under the 120ms
    // TERMINAL_RESIZE_DEBOUNCE. Only the last should ever reach the PTY.
    for cols in [20u16, 30, 40, 50, 60] {
        handle.resize(cols, 20, 8, 18);
        std::thread::sleep(Duration::from_millis(15));
    }
    std::thread::sleep(Duration::from_millis(400)); // let the debounced resize land + WINCH deliver
    handle.write(b"stty size\n".to_vec());

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline && !screen_text(&handle).contains("20 60") {
        std::thread::sleep(Duration::from_millis(10));
    }
    let text = screen_text(&handle);
    // The interactive shell echoes the typed `trap ... GOT_WINCH ...`
    // command itself, so "GOT_WINCH" appears once from that echo plus
    // once per actual WINCH delivery. 5 coalesced resizes -> 1 delivery
    // -> 2 total occurrences; uncoalesced would show 1 + 5 = 6.
    let winch_count = text.matches("GOT_WINCH").count();
    assert_eq!(
        winch_count, 2,
        "5 resizes 15ms apart should coalesce into exactly one PTY WINCH delivery \
         (1 command echo + 1 firing = 2 occurrences of GOT_WINCH), saw {winch_count}: {text:?}"
    );
    assert!(
        text.contains("20 60"),
        "final applied size should be the LAST requested size, not an intermediate one: {text:?}"
    );
}

// --- into `mod view_tests` (through TerminalView + GPUI TestAppContext) ---

#[gpui::test]
async fn critic2_rapid_output_burst_coalesces_far_below_line_count(
    cx: &mut gpui::TestAppContext,
) {
    cx.set_global(Theme::light());
    let working_directory = std::env::temp_dir().join(format!(
        "tiller-terminal-critic2-settle-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&working_directory).expect("create PTY directory");
    let shell = TerminalShell::WithArguments {
        program: "/bin/sh".to_string(),
        args: vec![
            "-c".to_string(),
            // 400 lines across two bursts split by a 50ms pause (well
            // under the 200ms OUTPUT_SETTLE_DEBOUNCE) - a per-line or
            // per-read-chunk implementation would emit many OutputSettled
            // events for this; coalescing should emit very few.
            "for i in $(seq 1 200); do echo L$i; done; sleep 0.05; for i in $(seq 1 200); do echo M$i; done; echo FINAL; exec sleep 2".to_string(),
        ],
    };
    let (terminal, cx) = cx.add_window_view(|_, cx| {
        TerminalView::with_shell(&working_directory, shell, cx).expect("spawn PTY")
    });
    let events = Rc::new(RefCell::new(Vec::new()));
    let collected = events.clone();
    let _subscription = cx.update(|_, app| {
        app.subscribe(&terminal, move |_, event: &TerminalActivityEvent, _| {
            if let TerminalActivityEvent::OutputSettled { scrollback } = event {
                collected.borrow_mut().push(scrollback.clone());
            }
        })
    });

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        cx.run_until_parked();
        cx.background_executor
            .advance_clock(Duration::from_millis(5));
        cx.run_until_parked();
        if events.borrow().iter().any(|s| s.contains("FINAL")) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let events = events.borrow();
    assert!(
        events.iter().any(|s| s.contains("FINAL")),
        "FINAL never settled: {events:?}"
    );
    assert!(
        events.len() <= 5,
        "400 lines across two rapid bursts (50ms apart, under the 200ms debounce) \
         should coalesce into a handful of settle events, not roughly one per line/chunk; \
         saw {} events",
        events.len()
    );
    terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
    cx.run_until_parked();
}
