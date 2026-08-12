//! A small GPUI terminal backed by alacritty's terminal emulator and PTY loop.

use std::{borrow::Cow, collections::HashMap, path::Path, sync::Arc, time::Duration};

use futures::channel::mpsc::UnboundedReceiver;
use futures::{FutureExt as _, StreamExt as _};

use alacritty_terminal::{
    event::{Event, EventListener, WindowSize},
    event_loop::{EventLoop, EventLoopSender, Msg},
    grid::Dimensions,
    index::{Column, Line},
    sync::FairMutex,
    term::{Config, Term, cell::Cell, cell::Flags},
    tty::{self, Shell},
    vte::ansi::{Color, NamedColor},
};
use anyhow::{Context as _, Result};
use gpui::{
    App, Bounds, ContentMask, Element, ElementId, Font, FontStyle, FontWeight, GlobalElementId,
    Hsla, InteractiveElement, IntoElement, KeyDownEvent, LayoutId, PaintQuad, ParentElement,
    Pixels, ShapedLine, StatefulInteractiveElement, Style, Styled, TextRun, Window, div, fill,
    font, point, px, relative, rgba, size,
};
use parking_lot::Mutex;
use tiller_theme::Theme;

const FONT_SIZE: Pixels = px(13.0);
const LINE_HEIGHT: Pixels = px(18.0);

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

/// What a [`TerminalView`] is showing: a live PTY, or the failure the PTY
/// could not be started — surfaced INSIDE the pane instead of aborting the
/// process, because forking a PTY fails for ordinary reasons (file-
/// descriptor exhaustion, a directory the user deleted or renamed since the
/// last session, a sandbox denial).
pub enum TerminalState {
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

    fn snapshot(&self) -> (Vec<Vec<Cell>>, (usize, usize)) {
        let term = self.term.lock();
        let grid = term.grid();
        let cells = (0..grid.screen_lines())
            .map(|line| {
                (0..grid.columns())
                    .map(|column| grid[Line(line as i32)][Column(column)].clone())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let cursor = (
            grid.cursor.point.line.0.max(0) as usize,
            grid.cursor.point.column.0,
        );
        (cells, cursor)
    }
}

/// A live PTY-backed terminal view, or a failed pane showing why the PTY
/// could not be started.
pub struct TerminalView {
    terminal: TerminalState,
    spawn: SpawnParams,
    focus_handle: gpui::FocusHandle,
}

/// How long a burst of terminal events may keep the pump draining before it
/// settles on a redraw. Mirrors the 4 ms coalescing window Zed's terminal
/// event loop uses; an idle terminal never reaches this timer, because the
/// pump parks on the event channel instead.
const EVENT_COALESCE_WINDOW: Duration = Duration::from_millis(4);
/// Cap on events drained into one redraw, bounding notify rate under a flood.
const EVENT_COALESCE_CAP: usize = 100;

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
        }
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

    /// Forks the PTY for the given parameters, without any view state.
    fn spawn_terminal(
        spawn: &SpawnParams,
    ) -> Result<(TerminalHandle, UnboundedReceiver<Event>)> {
        TerminalHandle::new(&spawn.working_directory, &spawn.shell)
    }

    /// Re-attempts the spawn after a failure: on success the pane switches
    /// to the live terminal, on failure the message is updated in place.
    fn retry(&mut self, cx: &mut gpui::Context<Self>) {
        match Self::spawn_terminal(&self.spawn) {
            Ok((terminal, wakeup_rx)) => {
                Self::pump_terminal_events(wakeup_rx, cx);
                self.terminal = TerminalState::Running(terminal);
            }
            Err(error) => {
                self.terminal = TerminalState::Failed {
                    message: format!("{error:#}"),
                };
            }
        }
        cx.notify();
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
            while wakeup_rx.next().await.is_some() {
                // Coalesce the burst: keep draining until the channel goes
                // quiet for the window (or the cap is hit), then redraw once.
                let mut quiet = cx.background_executor().timer(EVENT_COALESCE_WINDOW).fuse();
                let mut pending = 1;
                loop {
                    futures::select_biased! {
                        _ = quiet => break,
                        event = wakeup_rx.next() => match event {
                            Some(_) => {
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
                if this.update(cx, |_, cx| cx.notify()).is_err() {
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

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, _: &mut gpui::Context<Self>) {
        if let TerminalState::Running(terminal) = &self.terminal
            && let Some(bytes) = key_bytes(event)
        {
            terminal.write(bytes);
        }
    }
}

impl gpui::Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

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
        match &self.terminal {
            TerminalState::Running(terminal) => div()
                .size_full()
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
                .on_key_down(cx.listener(Self::on_key_down))
                .child(TerminalElement {
                    terminal: terminal.clone(),
                    palette,
                })
                .into_any_element(),
            TerminalState::Failed { message } => {
                let retry_entity = cx.entity();
                div()
                    .size_full()
                    .bg(theme.terminal_surface)
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
    use super::*;

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
        assert_ne!(color_to_hsla(Color::Named(NamedColor::Red), palette), palette.foreground);
        assert_ne!(color_to_hsla(Color::Indexed(196), palette), palette.foreground);
    }

    #[test]
    fn indexed_palette_covers_the_256_color_cube() {
        assert_eq!(indexed_color(16), (0, 0, 0));
        assert_eq!(indexed_color(196), (255, 0, 0));
        assert_eq!(indexed_color(255), (238, 238, 238));
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
            match wakeup_rx.try_next() {
                Ok(Some(_)) => {}
                Ok(None) => panic!("wakeup channel closed while draining startup events"),
                Err(_) => break, // channel empty: startup settled
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
            match wakeup_rx.try_next() {
                Ok(Some(_)) => {
                    // The shell echoed the command and printed WAKEUP_PROBE;
                    // each read batch fired a Wakeup.
                    return;
                }
                Ok(None) => panic!("wakeup channel closed before the probe arrived"),
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        panic!("no wakeup event within 10s after PTY output");
    }

    /// `TerminalShell::WithArguments` must run the given program directly as
    /// the PTY's child — no shell in between. Proven here by running a
    /// program that is not a shell (`/bin/echo`, printing its own argv) and
    /// requiring its output to reach the grid without ever writing a command
    /// through the PTY: nothing sent it one.
    #[test]
    fn with_arguments_runs_the_program_directly_as_the_pty_child() {
        let working_directory = std::env::temp_dir().join(format!(
            "tiller-terminal-test-args-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).unwrap();

        let shell = TerminalShell::WithArguments {
            program: "/bin/echo".to_string(),
            args: vec!["ARGV_PROBE".to_string()],
        };
        let (handle, mut wakeup_rx) = TerminalHandle::new(&working_directory, &shell).unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match wakeup_rx.try_next() {
                Ok(Some(_)) => {
                    let (cells, _) = handle.snapshot();
                    let screen: String = cells
                        .iter()
                        .flat_map(|row| row.iter().map(|cell| cell.c))
                        .collect();
                    if screen.contains("ARGV_PROBE") {
                        return;
                    }
                }
                Ok(None) => panic!("wakeup channel closed before the program's output arrived"),
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        panic!("program's own argv never appeared on the grid within 10s");
    }
}

#[cfg(test)]
mod view_tests {
    use super::*;
    use gpui::{Modifiers, VisualTestContext};

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

        let window = cx.add_window(|window, cx| {
            TerminalView::failed(&missing, TerminalShell::System, message, cx)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

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
        assert!(still_failed, "retry while the directory is missing stays failed");

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
        assert!(recovered, "retry after the directory appears recovers the pane");
    }
}
