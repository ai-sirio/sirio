//! PROTOTYPE — throwaway. Branch `prototype/ghostty-pane`, issue #33, map #27.
//!
//! # The question
//!
//! Does the chosen foundation — the published `libghostty-vt` crate for VT
//! parsing and state, `portable-pty` for the PTY — put a working terminal pane
//! on screen on Windows, inside this repo's GPUI setup? Specifically: does
//! `libghostty-vt` being `!Send`/`!Sync` force the pane out of the shape a GPUI
//! entity wants, the way `TerminalView` is shaped today?
//!
//! # What this is NOT
//!
//! Not a terminal. No selection, no scrollback UI, no mouse, no IME, no link
//! routing, no context menu, no split tree, no lifecycle cache. It renders
//! rows of styled runs and takes keystrokes. That is enough to answer the
//! question and nothing here should survive into `sirio_terminal`.
//!
//! The paint is deliberately simpler than the real one: `lib.rs` builds
//! `ShapedLine`s through a custom `Element`, this builds flex rows of divs.
//! What is being proven is that cells and their styles arrive, not that they
//! are drawn the production way.
//!
//! # Run
//!
//! ```text
//! cargo run -p sirio_terminal --example prototype_ghostty_pane
//! ```

use std::time::Duration;

use gpui::{
    App, AppContext, Bounds, Context, FontWeight, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, Render, Styled, Window, WindowBounds, WindowOptions, div, px, rgb, size,
};
use gpui_platform::application;

const POLL: Duration = Duration::from_millis(16);
const COLS: u16 = 100;
const ROWS: u16 = 30;

/// Prototype tracing. A GPUI app on Windows has no dependable stderr, so
/// startup steps go to a file next to the executable.
fn trace(step: &str) {
    use std::io::Write as _;
    let path = std::path::Path::new(r"D:\toolchains\prototype_ghostty_pane.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{step}");
        let _ = f.flush();
    }
    println!("[trace] {step}");
}

// ===========================================================================
// The portable half. This is the bit that answers the question and the bit
// that could be lifted into the real crate; it owns no GPUI types and knows
// nothing about how it is drawn.
// ===========================================================================
mod pane {
    use std::io::{Read, Write};
    use std::sync::mpsc::{Receiver, TryRecvError, channel};

    use libghostty_vt::{
        RenderState, Terminal, TerminalOptions,
        render::{CellIterator, RowIterator},
    };
    use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

    /// One rendered run of cells sharing a style. The prototype's own type —
    /// nothing from libghostty-vt escapes this module.
    #[derive(Clone, Debug)]
    pub struct Run {
        pub text: String,
        pub fg: Option<(u8, u8, u8)>,
        pub bg: Option<(u8, u8, u8)>,
        pub bold: bool,
        pub italic: bool,
        pub inverse: bool,
        pub strikethrough: bool,
        pub underline: bool,
    }

    /// Everything the pane owns. Note what is *not* here: no `Arc`, no `Mutex`.
    /// `Terminal` and the three render objects are `!Send`, so this struct is
    /// `!Send` too, and it lives wherever it was built — for this prototype,
    /// inside the GPUI entity on the main thread.
    ///
    /// The only thing crossing a thread boundary is `Vec<u8>` from the PTY
    /// reader, which is plainly `Send`.
    pub struct GhosttyPane {
        terminal: Terminal<'static, 'static>,
        render: RenderState<'static>,
        rows: RowIterator<'static>,
        cells: CellIterator<'static>,
        bytes_rx: Receiver<Vec<u8>>,
        reply_rx: Receiver<Vec<u8>>,
        writer: Box<dyn Write + Send>,
        master: Box<dyn MasterPty + Send>,
        _child: Box<dyn portable_pty::Child + Send + Sync>,
        pub bytes_seen: usize,
        pub cols: u16,
        pub rows_n: u16,
    }

    impl GhosttyPane {
        pub fn new(cols: u16, rows_n: u16) -> anyhow::Result<Self> {
            let pty = native_pty_system();
            let pair = pty.openpty(PtySize {
                rows: rows_n,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })?;

            let shell = std::env::var("PROTOTYPE_SHELL").unwrap_or_else(|_| {
                if cfg!(windows) {
                    "powershell.exe".to_string()
                } else {
                    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
                }
            });
            let cmd = CommandBuilder::new(shell);
            let child = pair.slave.spawn_command(cmd)?;
            drop(pair.slave);

            let mut reader = pair.master.try_clone_reader()?;
            let writer = pair.master.take_writer()?;

            // The reader thread only ever sends owned bytes. It never touches
            // the terminal, which is what keeps `!Send` a non-issue.
            let (tx, bytes_rx) = channel::<Vec<u8>>();
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tx.send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                    }
                }
            });

            let mut terminal = Terminal::new(TerminalOptions {
                cols,
                rows: rows_n,
                max_scrollback: 10_000,
            })?;

            // ConPTY opens with ESC[6n (DSR-CPR) and blocks until the terminal
            // answers; libghostty-vt hands us the reply bytes here. Dropping
            // them deadlocks the child at its first byte of output — fatal on
            // Windows, latent on unix. The channel exists because `Terminal`
            // and the writer both live in `Self`; borrowing one from inside
            // the other cannot compile.
            let (reply_tx, reply_rx) = channel::<Vec<u8>>();
            terminal.on_pty_write(move |_term, data: &[u8]| {
                let _ = reply_tx.send(data.to_vec());
            })?;

            Ok(Self {
                terminal,
                render: RenderState::new()?,
                rows: RowIterator::new()?,
                cells: CellIterator::new()?,
                bytes_rx,
                reply_rx,
                writer,
                master: pair.master,
                _child: child,
                bytes_seen: 0,
                cols,
                rows_n,
            })
        }

        /// Feed bytes straight to the parser, bypassing the pty.
        /// Same entry point real output takes (`pump` calls `vt_write`).
        pub fn feed_vt(&mut self, bytes: &[u8]) {
            self.terminal.vt_write(bytes);
        }

        /// Test-only: like `frame`, but keeps the raw per-cell `Style`, so
        /// measurements can see attributes `Run` flattens away (faint,
        /// invisible, the five underline styles, colour kinds).
        pub fn raw_cells(
            &mut self,
        ) -> anyhow::Result<Vec<Vec<(String, libghostty_vt::style::Style)>>> {
            let snapshot = self.render.begin_update(&self.terminal)?.end()?;
            let mut out: Vec<Vec<(String, libghostty_vt::style::Style)>> = Vec::new();
            let mut row_iteration = self.rows.update(&snapshot)?;
            while let Some(row) = row_iteration.next() {
                let mut line: Vec<(String, libghostty_vt::style::Style)> = Vec::new();
                let mut cell_iteration = self.cells.update(row)?;
                while let Some(cell) = cell_iteration.next() {
                    let graphemes = cell.graphemes().unwrap_or_default();
                    let text: String = if graphemes.is_empty() {
                        " ".to_string()
                    } else {
                        graphemes.into_iter().collect()
                    };
                    let style = cell.style().unwrap_or_default();
                    line.push((text, style));
                }
                out.push(line);
            }
            Ok(out)
        }

        /// Drain whatever the PTY produced and feed it to the parser.
        ///
        /// Polls rather than awaits, for the reason `lib.rs:1444` records about
        /// the alacritty path: the channel's waker would run on the reader
        /// thread and break GPUI's deterministic test scheduler.
        pub fn pump(&mut self) -> bool {
            let mut got = false;
            loop {
                match self.bytes_rx.try_recv() {
                    Ok(bytes) => {
                        self.bytes_seen += bytes.len();
                        self.terminal.vt_write(&bytes);
                        got = true;
                    }
                    Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
                }
            }
            // vt_write fires on_pty_write synchronously, so the replies owed
            // to the host are flushed after parsing, not before.
            while let Ok(reply) = self.reply_rx.try_recv() {
                self.send(&reply);
            }
            got
        }

        pub fn send(&mut self, bytes: &[u8]) {
            let _ = self.writer.write_all(bytes);
            let _ = self.writer.flush();
        }

        pub fn resize(&mut self, cols: u16, rows_n: u16) -> anyhow::Result<()> {
            self.master.resize(PtySize {
                rows: rows_n,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })?;
            // Cell pixel dimensions feed image protocols and size reports; the
            // prototype has no real metrics, so these are plausible constants.
            self.terminal.resize(cols, rows_n, 8, 18)?;
            self.cols = cols;
            self.rows_n = rows_n;
            Ok(())
        }

        pub fn cursor(&self) -> (u16, u16) {
            (
                self.terminal.cursor_x().unwrap_or(0),
                self.terminal.cursor_y().unwrap_or(0),
            )
        }

        pub fn title(&self) -> String {
            self.terminal.title().unwrap_or_default().to_string()
        }

        /// Build one frame: rows of style-grouped runs.
        ///
        /// This is the shape the real paint loop would consume. `begin_update`
        /// is the phase that needs the terminal; everything after `end()` reads
        /// only render-state memory, which is exactly the split that lets a
        /// real implementation hold a lock for the first call alone.
        pub fn frame(&mut self) -> anyhow::Result<Vec<Vec<Run>>> {
            let snapshot = self.render.begin_update(&self.terminal)?.end()?;
            let mut out: Vec<Vec<Run>> = Vec::new();

            let mut row_iteration = self.rows.update(&snapshot)?;
            while let Some(row) = row_iteration.next() {
                let mut line: Vec<Run> = Vec::new();
                let mut cell_iteration = self.cells.update(row)?;
                while let Some(cell) = cell_iteration.next() {
                    let style = cell.style().unwrap_or_default();
                    let graphemes = cell.graphemes().unwrap_or_default();
                    let text: String = if graphemes.is_empty() {
                        " ".to_string()
                    } else {
                        graphemes.into_iter().collect()
                    };
                    let fg = cell.fg_color().ok().flatten().map(|c| (c.r, c.g, c.b));
                    let bg = cell.bg_color().ok().flatten().map(|c| (c.r, c.g, c.b));
                    let run = Run {
                        text,
                        fg,
                        bg,
                        bold: style.bold,
                        italic: style.italic,
                        inverse: style.inverse,
                        strikethrough: style.strikethrough,
                        underline: !matches!(
                            style.underline,
                            libghostty_vt::style::Underline::None
                        ),
                    };
                    match line.last_mut() {
                        Some(prev) if same_style(prev, &run) => prev.text.push_str(&run.text),
                        _ => line.push(run),
                    }
                }
                out.push(line);
            }
            Ok(out)
        }
    }

    fn same_style(a: &Run, b: &Run) -> bool {
        a.fg == b.fg
            && a.bg == b.bg
            && a.bold == b.bold
            && a.italic == b.italic
            && a.inverse == b.inverse
            && a.strikethrough == b.strikethrough
            && a.underline == b.underline
    }
}

// ===========================================================================
// The throwaway half.
// ===========================================================================

struct PrototypeView {
    pane: pane::GhosttyPane,
    frames: usize,
    last_error: Option<String>,
}

impl PrototypeView {
    fn new(cx: &mut Context<Self>) -> anyhow::Result<Self> {
        let pane = pane::GhosttyPane::new(COLS, ROWS)?;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let keep_going = this
                    .update(cx, |view: &mut PrototypeView, cx| {
                        if view.pane.pump() {
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !keep_going {
                    return;
                }
            }
        })
        .detach();
        Ok(Self {
            pane,
            frames: 0,
            last_error: None,
        })
    }
}

fn to_rgb(c: (u8, u8, u8)) -> gpui::Rgba {
    rgb(((c.0 as u32) << 16) | ((c.1 as u32) << 8) | c.2 as u32)
}

impl Render for PrototypeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.frames += 1;
        let frame = match self.pane.frame() {
            Ok(rows) => rows,
            Err(error) => {
                self.last_error = Some(error.to_string());
                Vec::new()
            }
        };
        let (cx_col, cy_row) = self.pane.cursor();
        let status = format!(
            "PROTOTYPE  {}x{}  cursor {},{}  bytes {}  frames {}  title {:?}{}",
            self.pane.cols,
            self.pane.rows_n,
            cx_col,
            cy_row,
            self.pane.bytes_seen,
            self.frames,
            self.pane.title(),
            self.last_error
                .as_ref()
                .map(|e| format!("  ERROR {e}"))
                .unwrap_or_default(),
        );

        div()
            .key_context("prototype")
            .track_focus(&cx.focus_handle())
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, _window, cx| {
                let key = &event.keystroke.key;
                // F5 exercises the resize/reflow path the ticket asks about:
                // the primary screen reflows when wraparound is on.
                if key == "f5" {
                    let narrow = view.pane.cols != COLS;
                    let cols = if narrow { COLS } else { COLS / 2 };
                    if let Err(error) = view.pane.resize(cols, view.pane.rows_n) {
                        view.last_error = Some(format!("resize: {error}"));
                    }
                    cx.notify();
                    return;
                }
                let bytes: Vec<u8> = match key.as_str() {
                    "enter" => vec![b'\r'],
                    "backspace" => vec![0x7f],
                    "tab" => vec![b'\t'],
                    "escape" => vec![0x1b],
                    "up" => b"\x1b[A".to_vec(),
                    "down" => b"\x1b[B".to_vec(),
                    "right" => b"\x1b[C".to_vec(),
                    "left" => b"\x1b[D".to_vec(),
                    other => other.as_bytes().to_vec(),
                };
                view.pane.send(&bytes);
                cx.notify();
            }))
            .size_full()
            .bg(rgb(0x101014))
            .text_color(rgb(0xd0d0d8))
            .font_family("Consolas")
            .text_size(px(13.))
            .flex()
            .flex_col()
            .child(
                div()
                    .bg(rgb(0x202028))
                    .text_color(rgb(0x90f0a0))
                    .px_2()
                    .py_1()
                    .child(status),
            )
            .children(frame.into_iter().map(|line| {
                div()
                    .flex()
                    .flex_row()
                    .children(line.into_iter().map(|run| {
                        let (fg, bg) = if run.inverse {
                            (
                                run.bg.unwrap_or((16, 16, 20)),
                                run.fg.unwrap_or((208, 208, 216)),
                            )
                        } else {
                            (
                                run.fg.unwrap_or((208, 208, 216)),
                                run.bg.unwrap_or((16, 16, 20)),
                            )
                        };
                        let mut d = div()
                            .whitespace_nowrap()
                            .text_color(to_rgb(fg))
                            .bg(to_rgb(bg))
                            .child(run.text);
                        if run.bold {
                            d = d.font_weight(FontWeight::BOLD);
                        }
                        if run.italic {
                            d = d.italic();
                        }
                        if run.underline {
                            d = d.underline();
                        }
                        if run.strikethrough {
                            d = d.line_through();
                        }
                        d
                    }))
            }))
    }
}

fn main() {
    trace("--- run ---");
    application().run(move |cx: &mut App| {
        trace("app.run entered");
        let bounds = Bounds::centered(None, size(px(900.), px(640.)), cx);
        trace("bounds computed");
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |_, cx| {
                trace("window callback entered");
                cx.new(|cx| {
                    trace("building view");
                    let view = PrototypeView::new(cx);
                    trace(&format!("view built ok={}", view.is_ok()));
                    view.expect("start prototype pane")
                })
            },
        );
        trace(&format!("open_window ok={}", opened.is_ok()));
        opened.expect("open prototype window");
        cx.activate(true);
        trace("activated");
    });
    trace("app.run returned");
}

// ===========================================================================
// The headless proof.
//
// The window above answers the question to a human eye. This answers it to a
// machine: no GPUI, no window, no event loop — just PTY -> parser -> cells,
// driven by hand. It exists so the answer to issue #33 survives this branch.
//
// Run: cargo test -p sirio_terminal --example prototype_ghostty_pane \
//          -- --test-threads=1
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::pane::{GhosttyPane, Run};
    use std::time::{Duration, Instant};

    /// A string unlikely to appear in any shell banner, prompt or path.
    const SENTINEL: &str = "SIRIOPROTOOK7391";

    /// How long a PTY round trip is allowed to take before we stop waiting.
    /// A cold PowerShell on Windows is the slow case this has to tolerate.
    const PATIENCE: Duration = Duration::from_secs(15);

    /// Everything known at the moment a wait gave up.
    ///
    /// `bytes_seen` is the load-bearing field: it separates "the PTY delivered
    /// nothing" (portable-pty / ConPTY is at fault, or no shell spawned) from
    /// "bytes arrived but never became cells" (libghostty-vt is at fault).
    /// Without it the two failures are indistinguishable, which is exactly the
    /// mistake the first version of this file made.
    struct Diagnosis {
        bytes_seen: usize,
        last_frame: Vec<String>,
    }

    impl std::fmt::Display for Diagnosis {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let non_blank: Vec<&String> = self
                .last_frame
                .iter()
                .filter(|l| !l.trim().is_empty())
                .collect();
            write!(
                f,
                "{} byte(s) from the pty; {} row(s), {} non-blank: {:?}",
                self.bytes_seen,
                self.last_frame.len(),
                non_blank.len(),
                non_blank,
            )
        }
    }

    /// Flatten a frame to plain text lines; styles are not what these tests
    /// are about.
    fn text_of(frame: &[Vec<Run>]) -> Vec<String> {
        frame
            .iter()
            .map(|line| line.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect()
    }

    /// True when at least one row carries visible text.
    ///
    /// Deliberately not `!frame.is_empty()`: a terminal always has rows, so
    /// that assertion can never fail and proves nothing.
    fn has_text(lines: &[String]) -> bool {
        lines.iter().any(|l| !l.trim().is_empty())
    }

    /// Drive the pane until `wanted` is satisfied, or patience runs out.
    ///
    /// Returns the satisfying frame as `Ok`, or the *last* frame as `Err` —
    /// on timeout that frame is the only evidence there is, so it must not be
    /// thrown away.
    ///
    /// Polls rather than blocks for the same reason the live pane does: the
    /// only thing crossing a thread boundary is `Vec<u8>`, and the terminal is
    /// only ever touched here, on this thread.
    fn pump_until(
        pane: &mut GhosttyPane,
        mut wanted: impl FnMut(&[String]) -> bool,
    ) -> Result<Vec<String>, Vec<String>> {
        let deadline = Instant::now() + PATIENCE;
        let mut last = Vec::new();
        while Instant::now() < deadline {
            pane.pump();
            last = text_of(&pane.frame().expect("a frame always builds"));
            if wanted(&last) {
                return Ok(last);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Err(last)
    }

    /// Called when a PTY wait runs out of patience.
    ///
    /// TODO(enzo): decide the policy.
    ///
    /// Return `true` to let the test pass anyway (read: "this machine could
    /// not answer the question, and that is not the code's fault"), or panic
    /// to fail the build. `diag` carries `bytes_seen`, which is what makes a
    /// hybrid policy expressible rather than a coin flip.
    fn timed_out(waiting_for: &str, diag: Diagnosis) -> bool {
        unimplemented!("timed out waiting for {waiting_for} — {diag}")
    }

    /// Does anything at all come out of the PTY and land in cells?
    ///
    /// Half the catch: shell -> PTY -> parser -> rendered cells. Says nothing
    /// about input.
    #[test]
    fn shell_output_reaches_the_cells() {
        let mut pane = GhosttyPane::new(80, 24).expect("open a pty and a terminal");

        match pump_until(&mut pane, has_text) {
            Ok(_) => assert!(
                pane.bytes_seen > 0,
                "cells carried text but no bytes were counted — the counter lies"
            ),
            Err(last_frame) => {
                let diag = Diagnosis {
                    bytes_seen: pane.bytes_seen,
                    last_frame,
                };
                if !timed_out("any row with visible text", diag) {
                    panic!("the shell never produced a single visible cell");
                }
            }
        }
    }

    /// The full catch: a keystroke goes down the PTY, the shell acts on it,
    /// and the result comes back as cells.
    ///
    /// The sentinel is expected *twice* — once as the shell's echo of the
    /// typed line, once as the output of running it. One occurrence would only
    /// prove the echo, which is the PTY talking to itself.
    #[test]
    fn a_keystroke_round_trips_into_cells() {
        let mut pane = GhosttyPane::new(80, 24).expect("open a pty and a terminal");

        // Let the prompt settle first, or the keystroke races the banner.
        let _ = pump_until(&mut pane, has_text);
        pane.send(format!("echo {SENTINEL}\r").as_bytes());

        // Count real occurrences, not rows containing the sentinel: the
        // echo and the command output can land on the same row.
        let occurrences = |lines: &[String]| lines.concat().matches(SENTINEL).count();

        // Baseline BEFORE the keystroke, so passing means the count actually
        // rose, not merely that it sat at two.
        let settled = pump_until(&mut pane, has_text);
        let baseline = match &settled {
            Ok(lines) => occurrences(lines),
            Err(last) => occurrences(last),
        };
        pane.send(format!("echo {SENTINEL}\r").as_bytes());

        match pump_until(&mut pane, |lines| occurrences(lines) >= baseline + 2) {
            Ok(lines) => {
                let after = occurrences(&lines);
                assert!(
                    after >= baseline + 2,
                    "expected the sentinel to appear twice more ({baseline} -> {after})"
                );
            }
            Err(last_frame) => {
                let diag = Diagnosis {
                    bytes_seen: pane.bytes_seen,
                    last_frame,
                };
                if !timed_out("the sentinel echoed and then executed", diag) {
                    panic!("a keystroke did not round-trip through the shell");
                }
            }
        }
    }

    /// Resizing must reflow rather than blow up — the case the F5 key
    /// exercises by hand in the window.
    ///
    /// Asserts on *text*, not on row count: a terminal always has rows, so
    /// only surviving content proves the reflow did something real.
    #[test]
    fn resize_reflows_without_error() {
        let mut pane = GhosttyPane::new(80, 24).expect("open a pty and a terminal");
        pump_until(&mut pane, has_text).expect("the shell never produced visible text");

        pane.resize(40, 24).expect("narrow the pane");
        let narrow = text_of(&pane.frame().expect("a frame builds after narrowing"));

        pane.resize(120, 30).expect("widen the pane");
        let wide = text_of(&pane.frame().expect("a frame builds after widening"));

        assert!(has_text(&narrow), "narrowing wiped every visible cell");
        assert!(has_text(&wide), "widening wiped every visible cell");
    }

    /// Does `Terminal::title()` see an OSC 2 sequence without any effect
    /// handler wired (`on_title_changed` et al.)? Sirio's activity Layer B
    /// reads this title, so the answer decides whether the real crate needs
    /// the handler or merely wants it. Deliberately NO handler installed:
    /// a failure here is the answer, not something to fix away.
    #[test]
    fn osc_title_reaches_the_terminal_state() {
        let mut pane = GhosttyPane::new(80, 24).expect("open a pty and a terminal");
        pump_until(&mut pane, has_text).expect("the shell never produced visible text");

        // Fed straight down the parser path real output takes.
        pane.feed_vt(b"\x1b]2;SIRIOPROTO-TITLE\x07");

        let title = pane.title();
        assert!(
            title.contains("SIRIOPROTO-TITLE"),
            "osc 2 title did not reach terminal state without a handler; title = {title:?}"
        );
    }

    /// Layer isolation: drive `portable-pty` with no libghostty-vt in sight.
    ///
    /// Runs one process to completion-or-patience and reports what came back.
    ///
    /// The reader lives on its own thread and is deliberately never joined:
    /// `read` on a ConPTY master blocks with no way to cancel it, so a probe
    /// that reads inline can only hang. The first version of this test did
    /// exactly that and had to be killed.
    ///
    /// Note also what is *not* done here: `pair.slave` is kept alive for the
    /// whole probe, unlike `GhosttyPane::new`, which drops it right after
    /// spawning. That difference is one of the things being measured.
    fn probe(argv: &[&str], patience: Duration) -> String {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        use std::io::Read;
        use std::sync::mpsc::{RecvTimeoutError, channel};

        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");

        let mut cmd = CommandBuilder::new(argv[0]);
        for arg in &argv[1..] {
            cmd.arg(arg);
        }
        let mut child = match pair.slave.spawn_command(cmd) {
            Ok(child) => child,
            Err(e) => return format!("{argv:?} -> spawn FAILED: {e}"),
        };

        let mut reader = pair.master.try_clone_reader().expect("clone reader");
        let (tx, rx) = channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        let deadline = Instant::now() + patience;
        let mut total = Vec::new();
        let mut chunks = 0usize;
        let ended;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                ended = "patience ran out";
                break;
            }
            match rx.recv_timeout(left) {
                Ok(bytes) => {
                    chunks += 1;
                    total.extend_from_slice(&bytes);
                    if total.len() > 512 {
                        ended = "enough bytes";
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    ended = "no bytes within patience";
                    break;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    ended = "reader thread ended (eof or read error)";
                    break;
                }
            }
        }

        let alive = match child.try_wait() {
            Ok(None) => "still running".to_string(),
            Ok(Some(status)) => format!("exited {status:?}"),
            Err(e) => format!("try_wait failed: {e}"),
        };
        let hex: String = total.iter().take(24).map(|b| format!("{b:02x} ")).collect();

        format!(
            "{argv:?}
    {} byte(s) in {chunks} chunk(s); child {alive}; ended: {ended}
    hex: {hex}
    text: {:?}",
            total.len(),
            String::from_utf8_lossy(&total[..total.len().min(160)]),
        )
    }

    /// Spawn `argv` through portable-pty and collect raw output until the
    /// sentinel appears or patience runs out.
    ///
    /// Unlike `probe`, this one answers ConPTY's opening `ESC[6n` (DSR) with
    /// `ESC[1;1R` — without that, ConPTY emits exactly four bytes and blocks
    /// forever (the lesson from the first fix in this file), so every probe
    /// below would starve for reasons unrelated to argv quoting.
    #[cfg(windows)]
    fn probe_dsr(argv: &[&str], patience: Duration) -> (bool, bool, String) {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        use std::io::{Read, Write};
        use std::sync::mpsc::{RecvTimeoutError, channel};

        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");

        let mut cmd = CommandBuilder::new(argv[0]);
        for arg in &argv[1..] {
            cmd.arg(arg);
        }
        let mut child = pair.slave.spawn_command(cmd).expect("spawn");
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().expect("clone reader");
        let mut writer = pair.master.take_writer().expect("take writer");
        let (tx, rx) = channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        let deadline = Instant::now() + patience;
        let mut total = Vec::new();
        let mut dsr_answered = 0usize;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            match rx.recv_timeout(left) {
                Ok(bytes) => {
                    if bytes.windows(4).any(|w| w == b"\x1b[6n") {
                        dsr_answered += 1;
                        let _ = writer.write_all(b"\x1b[1;1R");
                        let _ = writer.flush();
                    }
                    total.extend_from_slice(&bytes);
                    if total
                        .windows(SENTINEL.len())
                        .any(|w| w == SENTINEL.as_bytes())
                    {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        let _ = child.kill();

        // Judged on RAW bytes: "executed" = the sentinel came back as its
        // own output line (`SENTINEL\r\n`). A bare contains() would false-
        // positive on the sirio form, where cmd.exe ECHOES the mangled
        // command in its "not recognized" error, sentinel included.
        let mut executed = false;
        for w in total.windows(SENTINEL.len() + 2) {
            if &w[..SENTINEL.len()] == SENTINEL.as_bytes() && &w[SENTINEL.len()..] == b"\r\n" {
                executed = true;
                break;
            }
        }
        // Locale-independent evidence of the quoting bug: the literal bytes
        // `\"echo` in the output mean CommandBuilder escaped the pre-wrapped
        // quotes and cmd.exe saw a quoted program name instead of a command.
        let mangled = total.windows(6).any(|w| w == b"\\\"echo");

        (
            executed,
            mangled,
            format!(
                "{} byte(s); DSR answered {}x; executed={executed}\ntext: {:?}",
                total.len(),
                dsr_answered,
                String::from_utf8_lossy(&total[..total.len().min(200)]),
            ),
        )
    }

    /// Measurement, not hope: does the argv form
    /// `command_shell_invocation` builds (pre-wrapped in quotes) survive
    /// `CommandBuilder`'s re-quoting, or does the plain form win?
    /// See issue #33. Windows-only: the quoting rules under test are
    /// portable-pty's Windows `cmdline` build.
    #[test]
    #[cfg(windows)]
    fn windows_shell_invocation_survives_command_builder() {
        // Form 1 — exactly what sirio_terminal::command_shell_invocation
        // produces: `/C` plus the command pre-wrapped in quotes.
        let (sirio_exec, sirio_mangled, sirio) = probe_dsr(
            &["cmd.exe", "/C", "\"echo SIRIOPROTOOK7391\""],
            Duration::from_secs(10),
        );
        // Form 2 — the same command, unquoted.
        let (plain_exec, _, plain) = probe_dsr(
            &["cmd.exe", "/C", "echo SIRIOPROTOOK7391"],
            Duration::from_secs(10),
        );

        println!("SIRIO FORM (pre-quoted): {sirio}");
        println!("PLAIN FORM:               {plain}");

        // Document reality: plain wins, pre-quoted loses, and the sirio
        // form's capture carries the escaped-quote mangling that explains it.
        assert!(plain_exec, "plain form did not execute: {plain}");
        assert!(
            !sirio_exec,
            "prediction WRONG — pre-quoted form executed too: {sirio}"
        );
        assert!(
            sirio_mangled,
            "expected CommandBuilder to escape the pre-wrapped quotes (\\\"echo …); got: {sirio}"
        );
    }

    /// Measurement for #31: which SGR attributes does libghostty-vt hand the
    /// embedder for free through the ordinary render path?
    ///
    /// Each attribute is wrapped around its own marker character and reset;
    /// the markers are then looked up in the raw cell grid. Read straight off
    /// `cell.style()` — deliberately NOT off `Run`, which flattens underline
    /// to bool and drops faint/invisible entirely.
    #[test]
    fn sgr_attributes_survive_the_render_path() {
        use libghostty_vt::style::{StyleColor, Underline};

        // (marker char, SGR introducing it, attribute label)
        let cases: &[(&str, &str, &str)] = &[
            ("1", "1", "bold"),
            ("2", "2", "dim/faint"),
            ("3", "3", "italic"),
            ("4", "4", "underline single"),
            ("5", "21", "underline double"),
            ("6", "4:3", "underline curly"),
            ("7", "4:4", "underline dotted"),
            ("8", "4:5", "underline dashed"),
            ("9", "7", "inverse"),
            ("h", "8", "hidden/invisible"),
            ("k", "9", "strikethrough"),
            ("p", "38;5;196", "fg 8-bit palette 196"),
            ("t", "38;2;255;128;0", "fg truecolour"),
        ];

        let mut seq = String::new();
        for (marker, sgr, _) in cases {
            seq.push_str(&format!("\x1b[{sgr}m{marker}\x1b[0m"));
        }

        let mut pane = GhosttyPane::new(80, 24).expect("open a pty and a terminal");
        pane.feed_vt(seq.as_bytes());

        let grid = pane.raw_cells().expect("raw cells build");
        let flat: Vec<&(String, _)> = grid.iter().flatten().collect();

        println!(
            "\n{:<22} {:<14} {:<34} {}",
            "attribute", "SGR", "observed on cell.style()", "exposed?"
        );
        println!("{}", "-".repeat(90));
        for (marker, sgr, label) in cases {
            let (_, style) = flat
                .iter()
                .find(|(text, _)| text == *marker)
                .unwrap_or_else(|| panic!("marker {marker:?} ({label}) not found in grid"));
            let observed = format!(
                "bold={} italic={} faint={} inverse={} invisible={} strike={} overline={} blink={} underline={:?} fg={:?}",
                style.bold,
                style.italic,
                style.faint,
                style.inverse,
                style.invisible,
                style.strikethrough,
                style.overline,
                style.blink,
                style.underline,
                match style.fg_color {
                    StyleColor::None => "none".to_string(),
                    StyleColor::Palette(i) => format!("palette({})", i.0),
                    StyleColor::Rgb(c) => format!("rgb({}, {}, {})", c.r, c.g, c.b),
                },
            );
            println!("{label:<22} {sgr:<14} {observed:<34}");
        }

        // Assert what was measured, not what was hoped. These document the
        // observed state; if libghostty-vt changes, this test flags it.
        let style_of = |m: &str| {
            &flat
                .iter()
                .find(|(text, _)| text == m)
                .expect("marker present")
                .1
        };
        assert!(style_of("1").bold);
        assert!(style_of("2").faint);
        assert!(style_of("3").italic);
        assert_eq!(style_of("4").underline, Underline::Single);
        assert_eq!(style_of("5").underline, Underline::Double);
        assert_eq!(style_of("6").underline, Underline::Curly);
        assert_eq!(style_of("7").underline, Underline::Dotted);
        assert_eq!(style_of("8").underline, Underline::Dashed);
        assert!(style_of("9").inverse);
        assert!(style_of("h").invisible);
        assert!(style_of("k").strikethrough);
        assert!(matches!(style_of("p").fg_color, StyleColor::Palette(i) if i.0 == 196));
        assert!(
            matches!(style_of("t").fg_color, StyleColor::Rgb(c) if (c.r, c.g, c.b) == (255, 128, 0))
        );
    }

    /// Always panics — it is a probe, not an assertion. The report is the point.
    ///
    /// Three processes, cheapest first. `cmd.exe /c echo` certainly writes and
    /// exits; if even that starves, the fault is the pty layer and neither the
    /// shell nor the parser is in question.
    #[test]
    #[ignore = "diagnostic probe: run explicitly with --ignored"]
    fn pty_alone_delivers_bytes() {
        let report = [
            probe(&["cmd.exe", "/c", "echo", SENTINEL], Duration::from_secs(6)),
            probe(
                &[
                    "powershell.exe",
                    "-NoProfile",
                    "-Command",
                    "Write-Output 42",
                ],
                Duration::from_secs(10),
            ),
            probe(&["powershell.exe", "-NoProfile"], Duration::from_secs(10)),
        ]
        .join(
            "
",
        );
        panic!(
            "PTY-ONLY REPORT
{report}"
        );
    }
}
