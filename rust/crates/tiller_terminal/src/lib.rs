//! A small GPUI terminal backed by alacritty's terminal emulator and PTY loop.

use std::{
    borrow::Cow,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use futures::channel::mpsc::UnboundedReceiver;
use futures::{FutureExt as _, StreamExt as _};

use alacritty_terminal::{
    event::{Event, EventListener, WindowSize},
    event_loop::{EventLoop, EventLoopSender, Msg},
    grid::{Dimensions, Scroll},
    index::{Column, Line},
    sync::FairMutex,
    term::{Config, Term, cell::Cell, cell::Flags},
    tty::{self, Shell},
    vte::ansi::{Color, NamedColor, Processor},
};
use anyhow::{Context as _, Result};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, Bounds, ClipboardItem, ContentMask, Element, ElementId, EventEmitter, Font, FontStyle,
    FontWeight, GlobalElementId, Hsla, InteractiveElement, IntoElement, KeyDownEvent, LayoutId,
    MouseButton, MouseDownEvent, PaintQuad, ParentElement, Pixels, Point, ShapedLine,
    StatefulInteractiveElement, Style, Styled, TextRun, Window, div, fill, font, point, px,
    relative, rgba, size,
};
use parking_lot::Mutex;
use tiller_theme::Theme;

mod context_menu;
mod domain;

pub use context_menu::{
    TerminalContextAction, TerminalContextEvent, TerminalContextItem, TerminalContextRoute,
    TerminalIdentity, items as terminal_context_menu_items,
};
pub use domain::{SplitAxis, SplitDirection, SplitTree, TerminalKey};

const FONT_SIZE: Pixels = px(13.0);
const LINE_HEIGHT: Pixels = px(18.0);
static NEXT_TERMINAL_ID: AtomicU64 = AtomicU64::new(1);

/// What a terminal's PTY runs, mirroring the shape of Zed's own `Shell`
/// (`util::shell::Shell`): a terminal is constructed with its task already
/// decided, never mutated into running a command after the fact.
#[derive(Clone, Debug)]
pub enum TerminalShell {
    /// The user's interactive login shell (`$SHELL -il`).
    System,
    /// A specific program with arguments, run directly as the PTY's child —
    /// no shell in between, so the very first thing the pane shows is that
    /// program's own interface, not a shell prompt.
    WithArguments { program: String, args: Vec<String> },
}

/// The terminal child's final lifecycle result.
///
/// Keeping this separate from the PTY implementation lets callers render a
/// stable status after the event loop has drained the child's final output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalExitStatus {
    Success,
    Code(i32),
    Signal(i32),
    Unknown,
}

/// The renderer-owned state that persistence and headless verification can
/// observe without knowing about GPUI or alacritty's grid types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalStateSnapshot {
    /// The directory supplied to the PTY when this terminal was spawned.
    pub working_directory: PathBuf,
    /// Plain-text terminal contents, including retained scrollback and the
    /// visible screen. The persistence owner applies its own byte bound.
    pub scrollback: Vec<u8>,
    /// The child's final status, once the PTY reports that it exited.
    pub exit_status: Option<TerminalExitStatus>,
}

impl TerminalExitStatus {
    fn from_exit_code(code: i32) -> Self {
        if code == 0 {
            Self::Success
        } else {
            Self::Code(code)
        }
    }

    fn from_signal(signal: i32) -> Self {
        Self::Signal(signal)
    }

    fn from_process_status(status: &std::process::ExitStatus) -> Self {
        if let Some(code) = status.code() {
            return Self::from_exit_code(code);
        }

        #[cfg(unix)]
        if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(status) {
            return Self::from_signal(signal);
        }

        Self::Unknown
    }

    fn label(self) -> String {
        match self {
            Self::Success => "Process exited successfully".to_string(),
            Self::Code(code) => format!("Process exited with status {code}"),
            Self::Signal(signal) => format!("Process terminated by signal {signal}"),
            Self::Unknown => "Process exited with an unknown status".to_string(),
        }
    }
}

/// What a [`TerminalView`] is showing: a live PTY, or the failure the PTY
/// could not be started — surfaced INSIDE the pane instead of aborting the
/// process, because forking a PTY fails for ordinary reasons (file-
/// descriptor exhaustion, a directory the user deleted or renamed since the
/// last session, a sandbox denial).
enum TerminalState {
    /// A live PTY, pumping events.
    Running(TerminalHandle),
    /// The last spawn attempt failed; the pane renders the message and a
    /// retry button.
    Failed { message: String },
}

/// The parameters a spawn (or retry) needs, kept so a failed pane can
/// re-attempt with the same inputs.
#[derive(Clone, Debug)]
struct SpawnParams {
    working_directory: std::path::PathBuf,
    shell: TerminalShell,
}

type SharedTerm = Arc<FairMutex<Term<TermEventProxy>>>;

struct TerminalDimensions {
    columns: usize,
    screen_lines: usize,
}

impl Dimensions for TerminalDimensions {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// Bridges the alacritty event-loop thread to the view's event pump.
///
/// The grid is already mutated by the event-loop thread before an event is
/// delivered, so every event means "the screen may have changed". Forwarding
/// the full event (rather than a bare wakeup) keeps the door open for
/// distinguishing Bell, title changes, etc. later, mirroring how Zed forwards
/// `TerminalBackendEvent`s through an unbounded channel.
#[derive(Clone)]
struct TermEventProxy {
    wakeup: futures::channel::mpsc::UnboundedSender<Event>,
}

impl EventListener for TermEventProxy {
    fn send_event(&self, event: Event) {
        let _ = self.wakeup.unbounded_send(event);
    }
}

#[derive(Clone)]
struct TerminalHandle {
    term: SharedTerm,
    sender: EventLoopSender,
    last_size: Arc<Mutex<Option<(u16, u16)>>>,
}

impl TerminalHandle {
    fn new(
        working_directory: impl AsRef<Path>,
        shell: &TerminalShell,
    ) -> Result<(Self, UnboundedReceiver<Event>)> {
        // alacritty swallows a failed chdir in the forked child (the shell
        // would silently start in the app's cwd, i.e. the wrong project).
        // Validate the directory here so a stale path surfaces as a
        // retryable pane error instead.
        let working_directory = working_directory.as_ref();
        let metadata = std::fs::metadata(working_directory).with_context(|| {
            format!(
                "working directory '{}' does not exist",
                working_directory.display()
            )
        })?;
        if !metadata.is_dir() {
            anyhow::bail!(
                "working directory '{}' is not a directory",
                working_directory.display()
            );
        }

        let (wakeup_tx, wakeup_rx) = futures::channel::mpsc::unbounded();
        let proxy = TermEventProxy { wakeup: wakeup_tx };
        let size = WindowSize {
            num_lines: 24,
            num_cols: 80,
            cell_width: 8,
            cell_height: 18,
        };
        let term = Arc::new(FairMutex::new(Term::new(
            Config::default(),
            &TerminalDimensions {
                columns: size.num_cols as usize,
                screen_lines: size.num_lines as usize,
            },
            proxy.clone(),
        )));

        let tty_shell = match shell {
            TerminalShell::System => {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
                Shell::new(shell, vec!["-il".to_string()])
            }
            TerminalShell::WithArguments { program, args } => {
                Shell::new(program.clone(), args.clone())
            }
        };
        let options = tty::Options {
            shell: Some(tty_shell),
            working_directory: Some(working_directory.to_path_buf()),
            env: HashMap::from([
                ("TERM".to_string(), "xterm-256color".to_string()),
                ("COLORTERM".to_string(), "truecolor".to_string()),
            ]),
            ..Default::default()
        };
        let pty = tty::new(&options, size, 0).context("creating terminal PTY")?;
        let event_loop = EventLoop::new(term.clone(), proxy, pty, true, false)
            .context("creating terminal event loop")?;
        let sender = event_loop.channel();
        event_loop.spawn();

        Ok((
            Self {
                term,
                sender,
                last_size: Arc::new(Mutex::new(None)),
            },
            wakeup_rx,
        ))
    }

    fn write(&self, bytes: Vec<u8>) {
        let _ = self.sender.send(Msg::Input(Cow::Owned(bytes)));
    }

    fn selection_text(&self) -> Option<String> {
        self.term.lock().selection_to_string()
    }

    fn clear_screen(&self) {
        let mut term = self.term.lock();
        let mut processor = Processor::<alacritty_terminal::vte::ansi::StdSyncHandler>::new();
        processor.advance(&mut *term, b"\x1b[3J\x1b[2J\x1b[H");
    }

    /// Ask alacritty's event loop to drop the PTY. Its Unix PTY destructor
    /// sends SIGHUP to the child and waits for it, so a terminal entity can
    /// never leave its shell orphaned when the app quits.
    fn shutdown(&self) {
        let _ = self.sender.send(Msg::Shutdown);
    }

    fn resize(&self, columns: u16, lines: u16, cell_width: u16, cell_height: u16) {
        let mut last_size = self.last_size.lock();
        if *last_size == Some((columns, lines)) {
            return;
        }
        *last_size = Some((columns, lines));
        let size = WindowSize {
            num_cols: columns,
            num_lines: lines,
            cell_width,
            cell_height,
        };
        self.term.lock().resize(TerminalDimensions {
            columns: columns as usize,
            screen_lines: lines as usize,
        });
        let _ = self.sender.send(Msg::Resize(size));
    }

    fn scroll_display(&self, scroll: Scroll) {
        self.term.lock().scroll_display(scroll);
    }

    fn snapshot(&self) -> (Vec<Vec<Cell>>, (usize, usize)) {
        let term = self.term.lock();
        let grid = term.grid();
        let display_offset = grid.display_offset() as i32;
        let cells = (0..grid.screen_lines())
            .map(|line| {
                (0..grid.columns())
                    .map(|column| grid[Line(line as i32 - display_offset)][Column(column)].clone())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let cursor = if display_offset == 0 {
            (
                grid.cursor.point.line.0.max(0) as usize,
                grid.cursor.point.column.0,
            )
        } else {
            (usize::MAX, 0)
        };
        (cells, cursor)
    }

    /// Captures the grid as newline-delimited plain text. This intentionally
    /// reads the emulator's complete retained history rather than only the
    /// visible viewport, so a persistence layer can restore what the user
    /// would have found by scrolling up.
    fn capture_scrollback(&self) -> Vec<u8> {
        let term = self.term.lock();
        let grid = term.grid();
        let history_size = grid.history_size();
        let mut lines = Vec::with_capacity(grid.total_lines());

        for line in -(history_size as i32)..(grid.screen_lines() as i32) {
            let mut text = String::with_capacity(grid.columns());
            for column in 0..grid.columns() {
                let cell = &grid[Line(line)][Column(column)];
                text.push(if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    ' '
                } else {
                    cell.c
                });
            }
            while text.ends_with(' ') {
                text.pop();
            }
            lines.push(text);
        }

        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines.join("\n").into_bytes()
    }

    /// Replays captured output directly into the emulator. It does not write
    /// to the child PTY, so restoring history cannot execute restored shell
    /// text or otherwise disturb the fresh process.
    fn replay_scrollback(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut term = self.term.lock();
        let mut processor = Processor::<alacritty_terminal::vte::ansi::StdSyncHandler>::new();
        processor.advance(&mut *term, bytes);
    }
}

/// A live PTY-backed terminal view, or a failed pane showing why the PTY
/// could not be started.
pub struct TerminalView {
    terminal: TerminalState,
    spawn: SpawnParams,
    focus_handle: gpui::FocusHandle,
    exit_status: Option<TerminalExitStatus>,
    identity: TerminalIdentity,
    context_menu: Option<Point<Pixels>>,
}

/// How long a burst of terminal events may keep the pump draining before it
/// settles on a redraw. Mirrors the 4 ms coalescing window Zed's terminal
/// event loop uses; an idle terminal never reaches this timer, because the
/// pump parks on the event channel instead.
const EVENT_COALESCE_WINDOW: Duration = Duration::from_millis(4);
/// Cap on events drained into one redraw, bounding notify rate under a flood.
const EVENT_COALESCE_CAP: usize = 100;

fn generated_identity() -> TerminalIdentity {
    let serial = NEXT_TERMINAL_ID.fetch_add(1, Ordering::Relaxed);
    TerminalIdentity::new(format!("pane-{serial}"), format!("terminal-{serial}"))
}

impl TerminalView {
    /// Starts the user's login shell in `working_directory`.
    ///
    /// Returns an error when the PTY cannot be forked — the caller renders
    /// the failure inside the pane (see [`Self::failed`]) rather than
    /// aborting, because spawn failures are ordinary (fd exhaustion, a
    /// deleted directory, a sandbox denial).
    pub fn new(working_directory: impl AsRef<Path>, cx: &mut gpui::Context<Self>) -> Result<Self> {
        Self::with_shell(working_directory, TerminalShell::System, cx)
    }

    /// Starts `shell`'s program in `working_directory`. Use this instead of
    /// [`Self::new`] plus a later write when the pane should run one command
    /// (an agent CLI, a task) rather than an interactive shell — the command
    /// is the PTY's child process from the start, so it is the first and
    /// only thing the pane ever shows.
    pub fn with_shell(
        working_directory: impl AsRef<Path>,
        shell: TerminalShell,
        cx: &mut gpui::Context<Self>,
    ) -> Result<Self> {
        let spawn = SpawnParams {
            working_directory: working_directory.as_ref().to_path_buf(),
            shell,
        };
        let (terminal, wakeup_rx) = Self::spawn_terminal(&spawn)?;
        let focus_handle = cx.focus_handle();

        Self::pump_terminal_events(wakeup_rx, cx);

        Ok(Self {
            terminal: TerminalState::Running(terminal),
            spawn,
            focus_handle,
            exit_status: None,
            identity: generated_identity(),
            context_menu: None,
        })
    }

    /// A pane in the failed state: renders `message` where the terminal
    /// would be, with a retry button. The caller decides the message (the
    /// error from a failed [`Self::new`]); the app uses this when starting a
    /// terminal tab or restoring a session item that cannot be spawned — a
    /// bad item must never abort the whole app.
    pub fn failed(
        working_directory: impl AsRef<Path>,
        shell: TerminalShell,
        message: impl Into<String>,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        Self {
            terminal: TerminalState::Failed {
                message: message.into(),
            },
            spawn: SpawnParams {
                working_directory: working_directory.as_ref().to_path_buf(),
                shell,
            },
            focus_handle: cx.focus_handle(),
            exit_status: None,
            identity: generated_identity(),
            context_menu: None,
        }
    }

    /// Replaces the generated identity with the host's stable pane and
    /// terminal IDs. The terminal view uses this target for every delegated
    /// context-menu event, so a menu opened in one split cannot act on another.
    pub fn set_identity(&mut self, identity: TerminalIdentity) {
        self.identity = identity;
    }

    pub fn identity(&self) -> &TerminalIdentity {
        &self.identity
    }

    /// Whether this pane is in the failed state (a live PTY could not be
    /// started). The host may use this to show a distinct tab glyph.
    pub fn is_failed(&self) -> bool {
        matches!(self.terminal, TerminalState::Failed { .. })
    }

    /// The message of a failed pane, if failed.
    pub fn failure_message(&self) -> Option<&str> {
        match &self.terminal {
            TerminalState::Failed { message } => Some(message),
            TerminalState::Running(_) => None,
        }
    }

    /// The child's final status once the PTY has reported `ChildExit`.
    pub fn exit_status(&self) -> Option<TerminalExitStatus> {
        self.exit_status
    }

    /// The working directory used to spawn this pane.
    pub fn working_directory(&self) -> &Path {
        &self.spawn.working_directory
    }

    /// Captures the renderer's current plain-text history for persistence or
    /// headless verification. Failed panes have no emulator contents.
    pub fn capture_scrollback(&self) -> Vec<u8> {
        match &self.terminal {
            TerminalState::Running(terminal) => terminal.capture_scrollback(),
            TerminalState::Failed { .. } => Vec::new(),
        }
    }

    /// Returns the terminal facts that can be checked without rendering a
    /// window. The caller owns any persistence-size limit.
    pub fn snapshot(&self) -> TerminalStateSnapshot {
        TerminalStateSnapshot {
            working_directory: self.spawn.working_directory.clone(),
            scrollback: self.capture_scrollback(),
            exit_status: self.exit_status,
        }
    }

    /// Replays stored terminal output into the emulator without sending it to
    /// the child process. The app uses this on a freshly spawned pane while
    /// restoring session state.
    pub fn replay_scrollback(&self, bytes: &[u8]) {
        if let TerminalState::Running(terminal) = &self.terminal {
            terminal.replay_scrollback(bytes);
        }
    }

    /// Forks the PTY for the given parameters, without any view state.
    fn spawn_terminal(spawn: &SpawnParams) -> Result<(TerminalHandle, UnboundedReceiver<Event>)> {
        TerminalHandle::new(&spawn.working_directory, &spawn.shell)
    }

    /// Re-attempts the spawn after a failure: on success the pane switches
    /// to the live terminal, on failure the message is updated in place.
    fn retry(&mut self, cx: &mut gpui::Context<Self>) {
        match Self::spawn_terminal(&self.spawn) {
            Ok((terminal, wakeup_rx)) => {
                Self::pump_terminal_events(wakeup_rx, cx);
                self.terminal = TerminalState::Running(terminal);
                self.exit_status = None;
            }
            Err(error) => {
                self.terminal = TerminalState::Failed {
                    message: format!("{error:#}"),
                };
            }
        }
        cx.notify();
    }

    fn exit_status_from_event(event: &Event) -> Option<TerminalExitStatus> {
        match event {
            Event::ChildExit(status) => Some(TerminalExitStatus::from_process_status(status)),
            _ => None,
        }
    }

    /// Bridges terminal event-loop activity onto the GPUI executor, the same
    /// shape Zed's `TerminalBuilder::subscribe` uses: the task blocks on the
    /// event channel while the terminal is idle (no timer, no redraws), and
    /// on activity it coalesces the burst — draining for up to
    /// `EVENT_COALESCE_WINDOW` or `EVENT_COALESCE_CAP` events — before
    /// requesting a single redraw, so a flood of output does not cost one
    /// notify per byte.
    fn pump_terminal_events(mut wakeup_rx: UnboundedReceiver<Event>, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            while let Some(first_event) = wakeup_rx.next().await {
                let mut exit_status = Self::exit_status_from_event(&first_event);
                // Coalesce the burst: keep draining until the channel goes
                // quiet for the window (or the cap is hit), then redraw once.
                let mut quiet = cx.background_executor().timer(EVENT_COALESCE_WINDOW).fuse();
                let mut pending = 1;
                loop {
                    futures::select_biased! {
                        _ = quiet => break,
                        event = wakeup_rx.next() => match event {
                            Some(event) => {
                                if exit_status.is_none() {
                                    exit_status = Self::exit_status_from_event(&event);
                                }
                                pending += 1;
                                if pending >= EVENT_COALESCE_CAP {
                                    break;
                                }
                            }
                            // All senders dropped: the alacritty event loop
                            // went away, so no more redraws can be needed.
                            None => return,
                        },
                    }
                }
                if this
                    .update(cx, |view, cx| {
                        if let Some(exit_status) = exit_status {
                            view.exit_status = Some(exit_status);
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    // The view is gone; stop pumping.
                    return;
                }
            }
        })
        .detach();
    }

    /// Writes bytes to the live PTY, the same path keystrokes use. Callers
    /// send bytes, never a raw file descriptor. A failed pane has no PTY and
    /// silently drops input (it renders a retry button instead).
    pub fn input(&self, bytes: impl Into<Vec<u8>>) {
        if let TerminalState::Running(terminal) = &self.terminal {
            terminal.write(bytes.into());
        }
    }

    /// Stops the PTY event loop and lets its Unix PTY destructor terminate the
    /// child. The app calls this from its quit hook while the GPUI entity is
    /// still alive, avoiding background-test scheduler activity during drop.
    pub fn shutdown(&self) {
        if let TerminalState::Running(terminal) = &self.terminal {
            terminal.shutdown();
        }
    }

    fn open_context_menu(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.context_menu = Some(event.position);
        cx.notify();
    }

    fn copy_text(&self, cx: &mut gpui::Context<Self>, include_context: bool) {
        let Some(terminal) = self.running_terminal() else {
            return;
        };
        let text = if include_context {
            terminal.capture_scrollback()
        } else {
            terminal
                .selection_text()
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| {
                    String::from_utf8_lossy(&terminal.capture_scrollback()).into_owned()
                })
                .into_bytes()
        };
        cx.write_to_clipboard(ClipboardItem::new_string(
            String::from_utf8_lossy(&text).into(),
        ));
    }

    fn running_terminal(&self) -> Option<&TerminalHandle> {
        match &self.terminal {
            TerminalState::Running(terminal) => Some(terminal),
            TerminalState::Failed { .. } => None,
        }
    }

    fn handle_context_action(
        &mut self,
        action: TerminalContextAction,
        _: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.context_menu = None;
        match action {
            TerminalContextAction::Copy => self.copy_text(cx, false),
            TerminalContextAction::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.input(text.into_bytes());
                }
            }
            TerminalContextAction::CopyContext => self.copy_text(cx, true),
            TerminalContextAction::CopyPaneId => cx.write_to_clipboard(ClipboardItem::new_string(
                self.identity.pane_id().to_owned(),
            )),
            TerminalContextAction::CopyTerminalId => cx.write_to_clipboard(
                ClipboardItem::new_string(self.identity.terminal_id().to_owned()),
            ),
            TerminalContextAction::ClearTerminal => {
                if let Some(terminal) = self.running_terminal() {
                    terminal.clear_screen();
                    cx.notify();
                }
            }
            TerminalContextAction::SetTitle
            | TerminalContextAction::SplitRight
            | TerminalContextAction::SplitDown
            | TerminalContextAction::CloseTerminal => {
                cx.emit(TerminalContextEvent {
                    target: self.identity.clone(),
                    action,
                });
            }
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, _: &mut gpui::Context<Self>) {
        if let TerminalState::Running(terminal) = &self.terminal {
            let key = event.keystroke.key.to_ascii_lowercase();
            let scroll = match key.as_str() {
                "pageup" | "page_up" => Some(Scroll::PageUp),
                "pagedown" | "page_down" => Some(Scroll::PageDown),
                _ => None,
            };
            if let Some(scroll) = scroll {
                terminal.scroll_display(scroll);
            } else if let Some(bytes) = key_bytes(event) {
                terminal.write(bytes);
            }
        }
    }
}

impl gpui::Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<TerminalContextEvent> for TerminalView {}

struct TerminalPaintState {
    backgrounds: Vec<PaintQuad>,
    lines: Vec<(ShapedLine, gpui::Point<Pixels>)>,
    cursor: Option<PaintQuad>,
}

#[derive(Clone, Copy)]
struct TerminalPalette {
    background: Hsla,
    foreground: Hsla,
    cursor: Hsla,
}

impl TerminalPalette {
    fn from_theme(theme: &Theme) -> Self {
        Self {
            background: theme.terminal_surface.into(),
            foreground: theme.primary_text_color.into(),
            cursor: theme.primary_text_color.into(),
        }
    }
}

struct TerminalElement {
    terminal: TerminalHandle,
    palette: TerminalPalette,
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = TerminalPaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        _: &mut App,
    ) -> Self::PrepaintState {
        let terminal_font = font("MesloLGS Nerd Font Mono");
        let font_id = window.text_system().resolve_font(&terminal_font);
        let cell_width = window
            .text_system()
            .advance(font_id, FONT_SIZE, 'm')
            .map(|advance| advance.width)
            .unwrap_or(px(8.0));
        let columns = (f32::from(bounds.size.width) / f32::from(cell_width))
            .floor()
            .max(1.0) as u16;
        let rows = (f32::from(bounds.size.height) / f32::from(LINE_HEIGHT))
            .floor()
            .max(1.0) as u16;
        self.terminal.resize(
            columns,
            rows,
            f32::from(cell_width).round().max(1.0) as u16,
            f32::from(LINE_HEIGHT).round().max(1.0) as u16,
        );

        let (cells, cursor) = self.terminal.snapshot();
        let mut backgrounds = Vec::new();
        let mut lines = Vec::with_capacity(cells.len());

        for (line, cells) in cells.into_iter().enumerate() {
            let mut text = String::new();
            let mut runs: Vec<TextRun> = Vec::with_capacity(cells.len());
            for (column, cell) in cells.into_iter().enumerate() {
                let character = if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    ' '
                } else {
                    cell.c
                };
                let foreground = color_to_hsla(cell.fg, self.palette);
                let run = TextRun {
                    len: character.len_utf8(),
                    color: foreground,
                    background_color: None,
                    font: Font {
                        weight: if cell.flags.contains(Flags::BOLD) {
                            FontWeight::BOLD
                        } else {
                            FontWeight::NORMAL
                        },
                        style: if cell.flags.contains(Flags::ITALIC) {
                            FontStyle::Italic
                        } else {
                            FontStyle::Normal
                        },
                        ..terminal_font.clone()
                    },
                    underline: None,
                    strikethrough: None,
                };
                if let Some(previous) = runs.last_mut()
                    && previous.font == run.font
                    && previous.color == run.color
                {
                    previous.len += run.len;
                } else {
                    runs.push(run);
                }
                text.push(character);

                backgrounds.push(fill(
                    Bounds::new(
                        point(
                            bounds.origin.x + cell_width * column as f32,
                            bounds.origin.y + LINE_HEIGHT * line as f32,
                        ),
                        size(cell_width, LINE_HEIGHT),
                    ),
                    color_to_hsla(cell.bg, self.palette),
                ));
            }

            let shaped_line = window
                .text_system()
                .shape_line(text.into(), FONT_SIZE, &runs, None);
            lines.push((
                shaped_line,
                point(bounds.origin.x, bounds.origin.y + LINE_HEIGHT * line as f32),
            ));
        }

        let cursor = (cursor.0 < rows as usize && cursor.1 < columns as usize).then(|| {
            fill(
                Bounds::new(
                    point(
                        bounds.origin.x + cell_width * cursor.1 as f32,
                        bounds.origin.y + LINE_HEIGHT * cursor.0 as f32,
                    ),
                    size(cell_width, LINE_HEIGHT),
                ),
                self.palette.cursor,
            )
        });

        TerminalPaintState {
            backgrounds,
            lines,
            cursor,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        state: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for background in state.backgrounds.drain(..) {
                window.paint_quad(background);
            }
            for (line, origin) in state.lines.drain(..) {
                let _ = line.paint(origin, LINE_HEIGHT, gpui::TextAlign::Left, None, window, cx);
            }
            if let Some(cursor) = state.cursor.take() {
                window.paint_quad(cursor);
            }
        });
    }
}

impl gpui::Render for TerminalView {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let palette = TerminalPalette::from_theme(&theme);
        let context_menu = self.context_menu.map(|position| {
            let entity = cx.entity();
            let dismiss_entity = entity.clone();
            let mut menu = div()
                .id("terminal-context-menu")
                .debug_selector(|| "terminal-context-menu".to_owned())
                .absolute()
                .left(position.x)
                .top(position.y)
                .w(px(220.0))
                .p(px(6.0))
                .rounded(theme.radii.user_pill)
                .border_1()
                .border_color(theme.hairline)
                .bg(theme.card_fill)
                .shadow_lg();

            for (index, item) in context_menu::items().iter().enumerate() {
                let action = item.action;
                let item_entity = entity.clone();
                let selector = format!("terminal-context-item-{index}");
                let debug_selector = selector.clone();
                menu = menu.child(
                    div()
                        .id(selector)
                        .debug_selector(move || debug_selector.clone())
                        .w_full()
                        .min_h(px(29.0))
                        .px(px(10.0))
                        .py(px(5.0))
                        .rounded(theme.radii.control)
                        .text_size(theme.typography.footnote)
                        .text_color(theme.title)
                        .hover(|style| style.bg(theme.row_hover))
                        .on_click(move |_, window, cx| {
                            item_entity.update(cx, |terminal, cx| {
                                terminal.handle_context_action(action, window, cx);
                            });
                        })
                        .child(item.label),
                );
            }
            menu.on_mouse_down_out(move |_, _, cx| {
                dismiss_entity.update(cx, |terminal, cx| {
                    terminal.context_menu = None;
                    cx.notify();
                });
            })
        });
        match &self.terminal {
            TerminalState::Running(terminal) => div()
                .size_full()
                .relative()
                .bg(theme.terminal_surface)
                .key_context("Terminal")
                .track_focus(&self.focus_handle)
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _: &gpui::MouseDownEvent, window, cx| {
                        // Click-to-focus, like Zed's terminal view: a terminal
                        // only takes keyboard input once focused.
                        this.focus_handle.focus(window, cx);
                    }),
                )
                .on_mouse_down(MouseButton::Right, cx.listener(Self::open_context_menu))
                .on_key_down(cx.listener(Self::on_key_down))
                .child(TerminalElement {
                    terminal: terminal.clone(),
                    palette,
                })
                .when_some(
                    self.exit_status.map(TerminalExitStatus::label),
                    |this, label| {
                        this.child(
                            div()
                                .absolute()
                                .left(px(8.0))
                                .bottom(px(8.0))
                                .px(px(8.0))
                                .py(px(4.0))
                                .bg(theme.primary_pill_bg)
                                .text_size(px(11.0))
                                .text_color(theme.subtitle)
                                .child(label),
                        )
                    },
                )
                .when_some(context_menu, |this, menu| this.child(menu))
                .into_any_element(),
            TerminalState::Failed { message } => {
                let retry_entity = cx.entity();
                div()
                    .size_full()
                    .relative()
                    .bg(theme.terminal_surface)
                    .on_mouse_down(MouseButton::Right, cx.listener(Self::open_context_menu))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(12.0))
                    .px(px(24.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.tab_needs_input)
                            .child("Terminal failed to start"),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_size(px(12.0))
                            .text_color(theme.subtitle)
                            .child(message.clone()),
                    )
                    .child(
                        div()
                            .id("terminal-retry")
                            .debug_selector(|| "terminal-retry".to_string())
                            .px(px(14.0))
                            .py(px(6.0))
                            .rounded(px(6.0))
                            .bg(theme.primary_pill_bg)
                            .text_size(px(12.0))
                            .text_color(theme.title)
                            .hover(|style| style.bg(theme.row_hover))
                            .cursor(gpui::CursorStyle::PointingHand)
                            .on_click(move |_, _, cx| {
                                retry_entity.update(cx, |view, cx| view.retry(cx));
                            })
                            .child("Retry"),
                    )
                    .when_some(context_menu, |this, menu| this.child(menu))
                    .into_any_element()
            }
        }
    }
}

fn color_to_hsla(color: Color, palette: TerminalPalette) -> Hsla {
    let (r, g, b) = match color {
        Color::Spec(rgb) => (rgb.r, rgb.g, rgb.b),
        Color::Indexed(index) => indexed_color(index),
        Color::Named(name) => return named_color(name, palette),
    };
    rgb_to_hsla((r, g, b))
}

fn named_color(color: NamedColor, palette: TerminalPalette) -> Hsla {
    match color {
        NamedColor::Black
        | NamedColor::Red
        | NamedColor::Green
        | NamedColor::Yellow
        | NamedColor::Blue
        | NamedColor::Magenta
        | NamedColor::Cyan
        | NamedColor::White
        | NamedColor::BrightBlack
        | NamedColor::BrightRed
        | NamedColor::BrightGreen
        | NamedColor::BrightYellow
        | NamedColor::BrightBlue
        | NamedColor::BrightMagenta
        | NamedColor::BrightCyan
        | NamedColor::BrightWhite => rgb_to_hsla(ansi_named_rgb(color)),
        NamedColor::Foreground | NamedColor::BrightForeground => palette.foreground,
        NamedColor::Background => palette.background,
        NamedColor::Cursor => palette.cursor,
        _ => rgba(0x808080ff).into(),
    }
}

fn rgb_to_hsla((r, g, b): (u8, u8, u8)) -> Hsla {
    rgba(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | 0xff).into()
}

fn ansi_named_rgb(color: NamedColor) -> (u8, u8, u8) {
    const COLORS: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (205, 49, 49),
        (13, 188, 121),
        (229, 229, 16),
        (36, 114, 200),
        (188, 63, 188),
        (17, 168, 205),
        (229, 229, 229),
        (102, 102, 102),
        (241, 76, 76),
        (35, 209, 139),
        (245, 245, 67),
        (59, 142, 234),
        (214, 112, 214),
        (41, 184, 219),
        (255, 255, 255),
    ];
    COLORS[color as usize]
}

fn indexed_color(index: u8) -> (u8, u8, u8) {
    if index < 16 {
        let color = match index {
            0 => NamedColor::Black,
            1 => NamedColor::Red,
            2 => NamedColor::Green,
            3 => NamedColor::Yellow,
            4 => NamedColor::Blue,
            5 => NamedColor::Magenta,
            6 => NamedColor::Cyan,
            7 => NamedColor::White,
            8 => NamedColor::BrightBlack,
            9 => NamedColor::BrightRed,
            10 => NamedColor::BrightGreen,
            11 => NamedColor::BrightYellow,
            12 => NamedColor::BrightBlue,
            13 => NamedColor::BrightMagenta,
            14 => NamedColor::BrightCyan,
            _ => NamedColor::BrightWhite,
        };
        return ansi_named_rgb(color);
    }
    if index >= 232 {
        let value = 8 + (index - 232) * 10;
        return (value, value, value);
    }
    let index = index - 16;
    let component = |value: u8| if value == 0 { 0 } else { 55 + value * 40 };
    (
        component(index / 36),
        component((index / 6) % 6),
        component(index % 6),
    )
}

fn key_bytes(event: &KeyDownEvent) -> Option<Vec<u8>> {
    let key = event.keystroke.key.to_ascii_lowercase();
    let modifiers = event.keystroke.modifiers;
    if modifiers.platform || key == "shift" || key == "control" || key == "alt" {
        return None;
    }
    let mut bytes = match key.as_str() {
        "enter" | "return" => b"\r".to_vec(),
        "backspace" => b"\x7f".to_vec(),
        "tab" => b"\t".to_vec(),
        "escape" => b"\x1b".to_vec(),
        "up" => b"\x1b[A".to_vec(),
        "down" => b"\x1b[B".to_vec(),
        "right" => b"\x1b[C".to_vec(),
        "left" => b"\x1b[D".to_vec(),
        "home" => b"\x1b[H".to_vec(),
        "end" => b"\x1b[F".to_vec(),
        "delete" => b"\x1b[3~".to_vec(),
        _ => event
            .keystroke
            .key_char
            .as_deref()
            .unwrap_or(&event.keystroke.key)
            .as_bytes()
            .to_vec(),
    };
    if modifiers.control && bytes.len() == 1 {
        bytes[0] &= 0x1f;
    } else if modifiers.alt {
        bytes.insert(0, 0x1b);
    }
    (!bytes.is_empty()).then_some(bytes)
}

#[cfg(test)]
mod tests {
    use futures::channel::mpsc::TryRecvError;

    use super::*;

    fn screen_text(handle: &TerminalHandle) -> String {
        let (cells, _) = handle.snapshot();
        cells
            .iter()
            .flat_map(|row| row.iter().map(|cell| cell.c))
            .collect()
    }

    fn palette() -> TerminalPalette {
        TerminalPalette::from_theme(&Theme::light())
    }

    #[test]
    fn terminal_defaults_follow_the_theme_but_ansi_colors_do_not() {
        let palette = palette();
        assert_eq!(
            color_to_hsla(Color::Named(NamedColor::Background), palette),
            palette.background
        );
        assert_eq!(
            color_to_hsla(Color::Named(NamedColor::Foreground), palette),
            palette.foreground
        );
        assert_ne!(
            color_to_hsla(Color::Named(NamedColor::Red), palette),
            palette.foreground
        );
        assert_ne!(
            color_to_hsla(Color::Indexed(196), palette),
            palette.foreground
        );
    }

    #[test]
    fn indexed_palette_covers_the_256_color_cube() {
        assert_eq!(indexed_color(16), (0, 0, 0));
        assert_eq!(indexed_color(196), (255, 0, 0));
        assert_eq!(indexed_color(255), (238, 238, 238));
    }

    #[test]
    fn child_exit_status_preserves_normal_and_signal_termination() {
        assert_eq!(
            TerminalExitStatus::from_exit_code(0),
            TerminalExitStatus::Success
        );
        assert_eq!(
            TerminalExitStatus::from_exit_code(7),
            TerminalExitStatus::Code(7)
        );
        assert_eq!(
            TerminalExitStatus::from_signal(15),
            TerminalExitStatus::Signal(15)
        );
    }

    #[test]
    fn scrollback_can_be_viewed_after_output_exceeds_the_viewport() {
        let working_directory = std::env::temp_dir().join(format!(
            "tiller-terminal-test-scroll-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).unwrap();
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "i=0; while [ \"$i\" -lt 100 ]; do printf 'P4_SCROLL_%03d\\n' \"$i\"; i=$((i + 1)); done"
                    .to_string(),
            ],
        };
        let (handle, _wakeup_rx) = TerminalHandle::new(&working_directory, &shell).unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline
            && !screen_text(&handle).contains("P4_SCROLL_099")
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            screen_text(&handle).contains("P4_SCROLL_099"),
            "the command output never reached the terminal grid"
        );
        let captured = String::from_utf8_lossy(&handle.capture_scrollback()).into_owned();
        assert!(
            captured.contains("P4_SCROLL_000") && captured.contains("P4_SCROLL_099"),
            "capture should include both retained history and the newest screen output"
        );

        handle.scroll_display(Scroll::Top);
        assert!(
            screen_text(&handle).contains("P4_SCROLL_000"),
            "scrolling to the top should expose the oldest retained line"
        );

        handle.scroll_display(Scroll::Bottom);
        assert!(
            screen_text(&handle).contains("P4_SCROLL_099"),
            "scrolling back to the bottom should restore the newest output"
        );
    }

    #[test]
    fn terminal_state_can_capture_and_replay_a_nonce_without_writing_to_the_child() {
        let working_directory =
            std::env::temp_dir().join(format!("tiller-terminal-test-state-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).unwrap();
        let nonce = format!("P30_SCROLLBACK_NONCE_{}", std::process::id());
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                format!("printf '%s\\n' '{nonce}'; exec sleep 60"),
            ],
        };
        let (source, _source_events) = TerminalHandle::new(&working_directory, &shell).unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline && !screen_text(&source).contains(&nonce) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            screen_text(&source).contains(&nonce),
            "the source terminal never displayed the nonce"
        );

        let captured = source.capture_scrollback();
        assert!(!captured.is_empty(), "capture must contain terminal state");
        assert!(
            String::from_utf8_lossy(&captured).contains(&nonce),
            "capture must contain the nonce"
        );
        let (restored, _restored_events) = TerminalHandle::new(
            &working_directory,
            &TerminalShell::WithArguments {
                program: "/bin/sh".to_string(),
                args: vec!["-c".to_string(), "exec sleep 60".to_string()],
            },
        )
        .unwrap();
        restored.replay_scrollback(&captured);

        assert!(
            screen_text(&restored).contains(&nonce),
            "replayed terminal state must contain the nonce"
        );
        source.shutdown();
        restored.shutdown();
    }

    /// The event-driven redraw contract, tested at the mechanism level: PTY
    /// output must surface as events on the wakeup channel the view's pump
    /// parks on. This is what lets an idle terminal cost nothing (the pump
    /// waits on the channel instead of polling) while output still wakes the
    /// view promptly.
    #[test]
    fn pty_output_wakes_the_event_pump() {
        let working_directory =
            std::env::temp_dir().join(format!("tiller-terminal-test-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).unwrap();

        let (handle, mut wakeup_rx) =
            TerminalHandle::new(&working_directory, &TerminalShell::System).unwrap();

        // Drain the startup traffic (login shell banner, prompt) so the
        // assertion below can only be satisfied by the probe we send.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match wakeup_rx.try_recv() {
                Ok(_) => {}
                Err(TryRecvError::Closed) => {
                    panic!("wakeup channel closed while draining startup events")
                }
                Err(TryRecvError::Empty) => break, // channel empty: startup settled
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            std::time::Instant::now() < deadline,
            "startup events kept arriving for 10s; event loop never settles"
        );

        // Send a command through the PTY and require an event to come back.
        handle.write(b"printf WAKEUP_PROBE\n".to_vec());
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match wakeup_rx.try_recv() {
                Ok(_) => {
                    // The shell echoed the command and printed WAKEUP_PROBE;
                    // each read batch fired a Wakeup.
                    return;
                }
                Err(TryRecvError::Closed) => {
                    panic!("wakeup channel closed before the probe arrived")
                }
                Err(TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        panic!("no wakeup event within 10s after PTY output");
    }

    #[test]
    fn resize_reaches_the_child_pty() {
        let working_directory = std::env::temp_dir().join(format!(
            "tiller-terminal-test-resize-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).unwrap();
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec!["-i".to_string()],
        };
        let (handle, _wakeup_rx) = TerminalHandle::new(&working_directory, &shell).unwrap();
        handle.resize(37, 11, 8, 18);
        handle.write(b"stty size\n".to_vec());

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline && !screen_text(&handle).contains("11 37") {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            screen_text(&handle).contains("11 37"),
            "the child did not observe the requested PTY size"
        );
    }

    /// `TerminalShell::WithArguments` must run the given program directly as
    /// the PTY's child — no shell in between. Proven here by running a
    /// program that is not a shell (`/bin/echo`, printing its own argv) and
    /// requiring its output to reach the grid without ever writing a command
    /// through the PTY: nothing sent it one.
    #[test]
    fn with_arguments_runs_the_program_directly_as_the_pty_child() {
        let working_directory =
            std::env::temp_dir().join(format!("tiller-terminal-test-args-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).unwrap();

        let shell = TerminalShell::WithArguments {
            program: "/bin/echo".to_string(),
            args: vec!["ARGV_PROBE".to_string()],
        };
        let (handle, mut wakeup_rx) = TerminalHandle::new(&working_directory, &shell).unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match wakeup_rx.try_recv() {
                Ok(_) => {
                    let (cells, _) = handle.snapshot();
                    let screen: String = cells
                        .iter()
                        .flat_map(|row| row.iter().map(|cell| cell.c))
                        .collect();
                    if screen.contains("ARGV_PROBE") {
                        return;
                    }
                }
                Err(TryRecvError::Closed) => {
                    panic!("wakeup channel closed before the program's output arrived")
                }
                Err(TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        panic!("program's own argv never appeared on the grid within 10s");
    }

    #[test]
    fn shutdown_sends_the_pty_child_a_termination_request() {
        let working_directory = std::env::temp_dir().join(format!(
            "tiller-terminal-test-shutdown-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).unwrap();
        let pid_file = working_directory.join("child.pid");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                format!("printf '%s' \"$$\" > {}; exec sleep 60", pid_file.display()),
            ],
        };
        let (handle, mut wakeup_rx) = TerminalHandle::new(&working_directory, &shell).unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let pid = loop {
            if let Ok(pid) = std::fs::read_to_string(&pid_file)
                && let Ok(pid) = pid.parse::<i32>()
            {
                break pid;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "PTY child did not publish its pid"
            );
            let _ = wakeup_rx.try_recv();
            std::thread::sleep(Duration::from_millis(10));
        };

        handle.shutdown();
        while std::time::Instant::now() < deadline {
            if wakeup_rx.try_recv().is_ok() && !process_exists(pid) {
                return;
            }
            if !process_exists(pid) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("PTY child {pid} was still running after terminal shutdown");
    }

    fn process_exists(pid: i32) -> bool {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
}

#[cfg(test)]
mod view_tests {
    use super::*;
    use gpui::{Modifiers, MouseButton, VisualTestContext, point, px};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn missing_directory(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "tiller-terminal-missing-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    /// Spawning a terminal into a directory that does not exist must return
    /// an error the caller can render, never panic.
    #[test]
    fn spawning_into_a_missing_directory_returns_an_error() {
        let missing = missing_directory("spawn");
        let result = TerminalHandle::new(&missing, &TerminalShell::System);
        assert!(
            result.is_err(),
            "the caller must receive the failure, not a panic"
        );
    }

    /// A failed pane renders its message and a retry button, and the retry
    /// recovers once the directory exists — the pane never aborts the
    /// process and never requires a restart.
    #[gpui::test]
    async fn failed_pane_renders_and_retry_recovers(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let missing = missing_directory("retry");
        let message = "cannot fork PTY".to_string();

        let window = cx.add_window(|_window, cx| {
            TerminalView::failed(&missing, TerminalShell::System, message, cx)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let snapshot = cx.update(|window, cx| {
            window
                .root::<TerminalView>()
                .flatten()
                .expect("root")
                .read(cx)
                .snapshot()
        });
        assert_eq!(snapshot.working_directory, missing);
        assert!(snapshot.scrollback.is_empty());
        assert_eq!(snapshot.exit_status, None);

        let retry_bounds = cx
            .debug_bounds("terminal-retry")
            .expect("the failed pane renders a retry button");

        // Retry while the directory is still missing: still failed, with a
        // fresh message in place.
        cx.simulate_click(retry_bounds.center(), Modifiers::none());
        cx.run_until_parked();
        let still_failed = cx.update(|window, cx| {
            window
                .root::<TerminalView>()
                .flatten()
                .expect("root")
                .read(cx)
                .is_failed()
        });
        assert!(
            still_failed,
            "retry while the directory is missing stays failed"
        );

        // Create the directory and retry: the pane comes alive.
        std::fs::create_dir_all(&missing).expect("create directory");
        cx.simulate_click(retry_bounds.center(), Modifiers::none());
        cx.run_until_parked();
        let recovered = cx.update(|window, cx| {
            !window
                .root::<TerminalView>()
                .flatten()
                .expect("root")
                .read(cx)
                .is_failed()
        });
        assert!(
            recovered,
            "retry after the directory appears recovers the pane"
        );
    }

    #[gpui::test]
    async fn right_click_resolves_this_terminal_and_draws_all_context_actions(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let missing = missing_directory("context-menu");
        let window = cx.add_window(|_, cx| {
            TerminalView::failed(
                &missing,
                TerminalShell::System,
                "test context-menu terminal",
                cx,
            )
        });
        let window_handle = window.into();
        let mut cx = VisualTestContext::from_window(window_handle, cx);
        cx.run_until_parked();

        let terminal = cx.update(|window, _cx| {
            window
                .root::<TerminalView>()
                .flatten()
                .expect("terminal root")
                .clone()
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&terminal, move |_, event: &TerminalContextEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        let click = point(px(40.0), px(40.0));
        cx.simulate_mouse_down(click, MouseButton::Right, Modifiers::none());

        for index in 0..10 {
            let selector: &'static str =
                Box::leak(format!("terminal-context-item-{index}").into_boxed_str());
            assert!(
                cx.debug_bounds(selector).is_some(),
                "context menu item {index} must be drawn"
            );
        }

        let split_right = cx
            .debug_bounds("terminal-context-item-6")
            .expect("Split Right item");
        cx.simulate_click(split_right.center(), Modifiers::none());
        let event = events.borrow().last().cloned().expect("split event");
        assert_eq!(event.action, TerminalContextAction::SplitRight);
        assert!(event.target.pane_id().starts_with("pane-"));
        assert!(event.target.terminal_id().starts_with("terminal-"));

        assert!(cx.update(|_, cx| terminal.read(cx).is_failed()));
    }
}
