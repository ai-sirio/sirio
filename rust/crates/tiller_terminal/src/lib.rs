//! A small GPUI terminal backed by alacritty's terminal emulator and PTY loop.

use std::{
    borrow::Cow,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};

use futures::channel::mpsc::{TryRecvError, UnboundedReceiver};

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
mod lifecycle;
mod link_router;

pub use context_menu::{
    TerminalContextAction, TerminalContextEvent, TerminalContextItem, TerminalContextRoute,
    TerminalIdentity, items as terminal_context_menu_items,
};
pub use domain::{SplitAxis, SplitDirection, SplitTree, TerminalKey};
pub use lifecycle::{CachedTerminalPane, TerminalPaneCache, TerminalSurfaceHost};
pub use link_router::{opens_terminal_link, url_at_column};

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

/// Events emitted when a terminal accepts a typed pane payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalDropEvent {
    /// Paths dropped from the file tree. The terminal inserts the model's
    /// shell-quoted representation without a newline and emits this event so
    /// the owning pane can classify or display the drop.
    Files { paths: Vec<PathBuf> },
    /// A changed-file diff was dropped into this terminal.
    Diff { path: PathBuf, text: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalPromptAction {
    NewTerminal,
    NewTerminalWithCommand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalPromptEvent {
    pub target: TerminalIdentity,
    pub action: TerminalPromptAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalLinkEvent {
    pub target: TerminalIdentity,
    pub url: String,
}

/// Builds the shell used by the Resolve in terminal action.
///
/// The command prints the combined conflict diff for the exact repo-relative
/// path, then leaves an interactive shell in the repository so the user can
/// resolve it with ordinary Git commands. The path is shell-quoted here,
/// before it crosses the PTY boundary.
pub fn conflict_resolution_shell(path: &Path) -> TerminalShell {
    let quoted_path = shell_quote(path);
    TerminalShell::WithArguments {
        program: "/bin/sh".to_string(),
        args: vec![
            "-lc".to_string(),
            format!(
                "git diff --cc -- {quoted_path}; printf '\\nResolve conflict at %s\\n' {quoted_path}; exec /bin/sh -il"
            ),
        ],
    }
}

fn shell_quote(path: &Path) -> String {
    let value = path.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\\''"))
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

/// Signals from the PTY that the app may use for agent-activity detection.
///
/// This is deliberately separate from [`TerminalContextEvent`]: a context
/// menu's `SetTitle` action is a user-set tab label, while `OscTitle` is the
/// title text emitted by the shell or an interactive agent over the PTY.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalActivityEvent {
    /// A title received from an OSC 0/2 sequence in the PTY stream.
    OscTitle(String),
    /// The terminal output burst has been quiet for the coalescing window;
    /// the snapshot is plain text from the emulator's retained scrollback.
    OutputSettled { scrollback: String },
    /// The PTY child exited after its final output was drained.
    ChildExited { status: TerminalExitStatus },
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
    /// The view was created before its host assigned the stable pane ID. The
    /// PTY is intentionally delayed until the first render so that the child
    /// inherits the host identity rather than the generated placeholder.
    Pending,
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
/// delivered. The pump below translates title, output, and child-exit events
/// into the typed [`TerminalActivityEvent`] surface after coalescing a burst.
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
    shell_pid: u32,
    shutdown_started: Arc<AtomicBool>,
    resize_generation: Arc<AtomicU64>,
}

impl TerminalHandle {
    fn validate_working_directory(working_directory: &Path) -> Result<()> {
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
        Ok(())
    }

    #[cfg(test)]
    fn new(
        working_directory: impl AsRef<Path>,
        shell: &TerminalShell,
    ) -> Result<(Self, UnboundedReceiver<Event>)> {
        Self::new_with_pane_id(working_directory, shell, None)
    }

    fn new_with_pane_id(
        working_directory: impl AsRef<Path>,
        shell: &TerminalShell,
        pane_id: Option<&str>,
    ) -> Result<(Self, UnboundedReceiver<Event>)> {
        // alacritty swallows a failed chdir in the forked child (the shell
        // would silently start in the app's cwd, i.e. the wrong project).
        // Validate the directory here so a stale path surfaces as a
        // retryable pane error instead.
        let working_directory = working_directory.as_ref();
        Self::validate_working_directory(working_directory)?;

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
        let mut env = HashMap::from([
            ("TERM".to_string(), "xterm-256color".to_string()),
            ("COLORTERM".to_string(), "truecolor".to_string()),
        ]);
        if let Some(pane_id) = pane_id {
            env.insert("TILLER_PANE_ID".to_string(), pane_id.to_string());
        }
        let options = tty::Options {
            shell: Some(tty_shell),
            working_directory: Some(working_directory.to_path_buf()),
            env,
            ..Default::default()
        };
        let pty = tty::new(&options, size, 0).context("creating terminal PTY")?;
        let shell_pid = pty.child().id();
        let event_loop = EventLoop::new(term.clone(), proxy, pty, true, false)
            .context("creating terminal event loop")?;
        let sender = event_loop.channel();
        event_loop.spawn();

        Ok((
            Self {
                term,
                sender,
                last_size: Arc::new(Mutex::new(None)),
                shell_pid,
                shutdown_started: Arc::new(AtomicBool::new(false)),
                resize_generation: Arc::new(AtomicU64::new(0)),
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

    /// Terminate the PTY's process group(s) and ask alacritty to drop the
    /// PTY. The PTY destructor only signals its direct child; signaling the
    /// group(s) here is what also reaches the shell's descendants —
    /// including ones that detached into their own process group after job
    /// control forked them (F-PER-06), which a single `killpg` on the pgid
    /// captured at spawn never reaches. A stubborn process gets SIGKILL
    /// after a short grace period so close/quit cannot leave a live process
    /// group behind.
    fn shutdown(&self) {
        if self.shutdown_started.swap(true, Ordering::AcqRel) {
            return;
        }
        terminate_descendant_process_groups(self.shell_pid);
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
        let generation = self.resize_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let resize_generation = Arc::clone(&self.resize_generation);
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            std::thread::sleep(TERMINAL_RESIZE_DEBOUNCE);
            if resize_generation.load(Ordering::Acquire) == generation {
                let _ = sender.send(Msg::Resize(size));
            }
        });
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

    fn link_at(&self, row: usize, column: usize) -> Option<String> {
        let term = self.term.lock();
        let grid = term.grid();
        if row >= grid.screen_lines() || column >= grid.columns() {
            return None;
        }
        let display_offset = grid.display_offset() as i32;
        let cell = &grid[Line(row as i32 - display_offset)][Column(column)];
        if let Some(hyperlink) = cell.hyperlink() {
            return Some(hyperlink.uri().to_owned());
        }
        let line = (0..grid.columns())
            .map(|column| grid[Line(row as i32 - display_offset)][Column(column)].c)
            .collect::<String>();
        let byte_column = line
            .char_indices()
            .nth(column)
            .map_or(line.len(), |(index, _)| index);
        url_at_column(&line, byte_column)
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

const TERMINAL_TERMINATE_GRACE: Duration = Duration::from_millis(500);
const TERMINAL_RESIZE_DEBOUNCE: Duration = Duration::from_millis(120);

fn terminate_process_group(process_group: u32) {
    let process_group = process_group as libc::pid_t;
    if process_group <= 0 {
        return;
    }

    unsafe {
        libc::killpg(process_group, libc::SIGTERM);
    }

    std::thread::spawn(move || {
        std::thread::sleep(TERMINAL_TERMINATE_GRACE);
        let group_is_alive = unsafe { libc::killpg(process_group, 0) == 0 };
        if group_is_alive {
            unsafe {
                libc::killpg(process_group, libc::SIGKILL);
            }
        }
    });
}

/// Every process id that is currently a descendant of `root` (not
/// including `root` itself), discovered by walking the kernel's own live
/// parent/child view (F-PER-06).
///
/// `/proc/<pid>/task/<tid>/children` is read fresh per pid rather than
/// cached: a job-control-spawned child (a compound command, a backgrounded
/// job) can detach into its own session/process group at any point after
/// spawn, and a stubborn descendant can itself keep forking, so only a walk
/// done at kill time — not the single pgid captured once at spawn — can
/// find all of it. A process can have multiple threads (`task/<tid>`), and
/// each thread's `children` file only lists the children *that thread*
/// directly spawned, so every tid must be read to see the whole process's
/// children.
fn descendant_pids(root: libc::pid_t) -> Vec<libc::pid_t> {
    let mut discovered = Vec::new();
    let mut seen = std::collections::HashSet::new();
    seen.insert(root);
    let mut frontier = vec![root];
    while let Some(pid) = frontier.pop() {
        let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
            continue;
        };
        for task in tasks.flatten() {
            let Ok(contents) = std::fs::read_to_string(task.path().join("children")) else {
                continue;
            };
            for token in contents.split_whitespace() {
                let Ok(child) = token.parse::<libc::pid_t>() else {
                    continue;
                };
                if seen.insert(child) {
                    discovered.push(child);
                    frontier.push(child);
                }
            }
        }
    }
    discovered
}

/// The distinct process groups spanning the shell (`shell_pid`) and every
/// descendant it has right now, deduplicated. A single `killpg` on the pgid
/// captured once at spawn only ever reaches the shell's *original* group —
/// a job-control-spawned child that detached into a new group (a compound
/// command run under job control, a backgrounded job in an interactive
/// shell) is invisible to it and survives shutdown as an orphan (F-PER-06).
/// Reading `getpgid` per pid at kill time, rather than assuming the shell's
/// own pid is still its pgid, is what makes this correct even if the shell
/// itself has re-grouped.
fn descendant_process_groups(shell_pid: libc::pid_t) -> Vec<libc::pid_t> {
    let mut pids = vec![shell_pid];
    pids.extend(descendant_pids(shell_pid));
    let mut groups: Vec<libc::pid_t> = Vec::new();
    for pid in pids {
        let pgid = unsafe { libc::getpgid(pid) };
        if pgid > 0 && !groups.contains(&pgid) {
            groups.push(pgid);
        }
    }
    groups
}

/// Terminates the shell at `shell_pid` and every process group any of its
/// current descendants live in — not just the group captured at spawn
/// (F-PER-06). Each distinct group gets its own SIGTERM-then-grace-then-
/// SIGKILL handling via [`terminate_process_group`].
fn terminate_descendant_process_groups(shell_pid: u32) {
    for group in descendant_process_groups(shell_pid as libc::pid_t) {
        terminate_process_group(group as u32);
    }
}

/// A live PTY-backed terminal view, or a failed pane showing why the PTY
/// could not be started.
pub struct TerminalView {
    terminal: TerminalState,
    spawn: SpawnParams,
    empty_prompt: bool,
    focus_handle: gpui::FocusHandle,
    exit_status: Option<TerminalExitStatus>,
    identity: TerminalIdentity,
    context_menu: Option<Point<Pixels>>,
    last_dropped_diff: Option<(PathBuf, String)>,
    last_dropped_files: Option<Vec<PathBuf>>,
}

/// Output is forwarded to the activity model only after this quiet period.
/// This is intentionally longer than the renderer's frame cadence: an agent
/// turn commonly arrives as several PTY writes and must be observed as one
/// settled snapshot.
const OUTPUT_SETTLE_DEBOUNCE: Duration = Duration::from_millis(200);
pub const CONTENT_MATCH_BYTE_LIMIT: usize = 10 * 1024;
pub const CONTENT_MATCH_LINE_LIMIT: usize = 40;
/// Poll interval for the PTY event channel. The GPUI task owns this timer and
/// polls the channel with `try_recv`; the alacritty reader thread therefore
/// never wakes the deterministic test scheduler directly.
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(4);
/// Cap on events drained into one redraw, bounding notify rate under a flood.
const EVENT_COALESCE_CAP: usize = 100;

/// Bounds the snapshot sent to the activity matcher. Persistence still keeps
/// the complete scrollback, but matching an agent prompt only needs the last
/// 10 KiB and 40 lines and must not scan an unbounded session history.
pub fn recent_content_window(scrollback: &str) -> String {
    let byte_start = suffix_char_boundary(scrollback, CONTENT_MATCH_BYTE_LIMIT);
    let byte_window = &scrollback[byte_start..];
    let mut lines = byte_window
        .lines()
        .rev()
        .take(CONTENT_MATCH_LINE_LIMIT)
        .collect::<Vec<_>>();
    lines.reverse();
    let joined = lines.join("\n");
    let start = suffix_char_boundary(&joined, CONTENT_MATCH_BYTE_LIMIT);
    joined[start..].to_owned()
}

fn suffix_char_boundary(value: &str, max_bytes: usize) -> usize {
    let mut start = value.len().saturating_sub(max_bytes);
    while start < value.len() && !value.is_char_boundary(start) {
        start += 1;
    }
    start
}

fn generated_identity() -> TerminalIdentity {
    let serial = NEXT_TERMINAL_ID.fetch_add(1, Ordering::Relaxed);
    TerminalIdentity::new(format!("pane-{serial}"), format!("terminal-{serial}"))
}

impl TerminalView {
    /// Starts the user's login shell in `working_directory` on first render.
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
        TerminalHandle::validate_working_directory(&spawn.working_directory)?;
        let focus_handle = cx.focus_handle();
        let identity = generated_identity();

        Ok(Self {
            terminal: TerminalState::Pending,
            spawn,
            empty_prompt: false,
            focus_handle,
            exit_status: None,
            identity,
            context_menu: None,
            last_dropped_diff: None,
            last_dropped_files: None,
        })
    }

    /// Constructs the unmounted-pane surface. The host can subscribe to
    /// [`TerminalPromptEvent`] and call [`Self::start_from_prompt`] after the
    /// user chooses one of the two actions.
    pub fn empty_prompt(cx: &mut gpui::Context<Self>) -> Self {
        let working_directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            terminal: TerminalState::Pending,
            spawn: SpawnParams {
                working_directory,
                shell: TerminalShell::System,
            },
            empty_prompt: true,
            focus_handle: cx.focus_handle(),
            exit_status: None,
            identity: generated_identity(),
            context_menu: None,
            last_dropped_diff: None,
            last_dropped_files: None,
        }
    }

    /// Starts a terminal in `repo_root` prepared to inspect and resolve one
    /// conflicted repo-relative path.
    pub fn for_conflict(
        repo_root: impl AsRef<Path>,
        path: impl AsRef<Path>,
        cx: &mut gpui::Context<Self>,
    ) -> Result<Self> {
        Self::with_shell(repo_root, conflict_resolution_shell(path.as_ref()), cx)
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
            empty_prompt: false,
            focus_handle: cx.focus_handle(),
            exit_status: None,
            identity: generated_identity(),
            context_menu: None,
            last_dropped_diff: None,
            last_dropped_files: None,
        }
    }

    /// Replaces the generated identity with the host's stable pane and
    /// terminal IDs. The terminal view uses this target for every delegated
    /// context-menu event, so a menu opened in one split cannot act on another.
    /// The first render is deliberately where the PTY is spawned, so callers
    /// may assign this identity immediately after creating the entity and the
    /// child will inherit `TILLER_PANE_ID`.
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
            TerminalState::Pending | TerminalState::Running(_) => None,
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

    /// The shell command this pane was created with. This is useful to the
    /// host when it needs to explain or test a specialised launch surface.
    pub fn launch_shell(&self) -> &TerminalShell {
        &self.spawn.shell
    }

    /// The last diff accepted through this pane's drop target, if any.
    pub fn last_dropped_diff(&self) -> Option<&(PathBuf, String)> {
        self.last_dropped_diff.as_ref()
    }

    pub fn last_dropped_files(&self) -> Option<&[PathBuf]> {
        self.last_dropped_files.as_deref()
    }

    /// Lets the pane-composition owner turn the empty surface into a live
    /// terminal without constructing a second view/entity.
    pub fn start_from_prompt(&mut self, shell: TerminalShell, cx: &mut gpui::Context<Self>) {
        self.spawn.shell = shell;
        self.empty_prompt = false;
        self.ensure_started(cx);
        cx.notify();
    }

    fn receive_diff_drop(&mut self, payload: (PathBuf, String), cx: &mut gpui::Context<Self>) {
        self.last_dropped_diff = Some(payload.clone());
        cx.emit(TerminalDropEvent::Diff {
            path: payload.0,
            text: payload.1,
        });
        cx.notify();
    }

    /// Handles both drop payload shapes that land here (F-TERM-PTY-06): the
    /// in-app typed drag (a Files-panel row, always exactly one path) and a
    /// real OS-level `ExternalPaths` drop (an XDND file manager drag, which
    /// can carry several paths in one drop). Both funnel into the same
    /// quoted-and-space-joined insertion `tiller_project::terminal_file_drop`
    /// already builds for a `Vec`.
    fn receive_file_drop(&mut self, paths: Vec<PathBuf>, cx: &mut gpui::Context<Self>) {
        if paths.is_empty() {
            return;
        }
        let insertion = tiller_project::terminal_file_drop(&paths);
        if insertion.is_empty() {
            return;
        }
        self.last_dropped_files = Some(paths.clone());
        self.input(insertion.into_bytes());
        cx.emit(TerminalDropEvent::Files { paths });
        cx.notify();
    }

    /// The PID of the shell (or direct command) at the root of this pane's
    /// PTY. Layer D walks descendants of this PID on its periodic refresh.
    pub fn shell_pid(&self) -> Option<u32> {
        self.running_terminal().map(|terminal| terminal.shell_pid)
    }

    /// Captures the renderer's current plain-text history for persistence or
    /// headless verification. Failed panes have no emulator contents.
    pub fn capture_scrollback(&self) -> Vec<u8> {
        match &self.terminal {
            TerminalState::Running(terminal) => terminal.capture_scrollback(),
            TerminalState::Pending | TerminalState::Failed { .. } => Vec::new(),
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
    fn spawn_terminal(
        spawn: &SpawnParams,
        pane_id: &str,
    ) -> Result<(TerminalHandle, UnboundedReceiver<Event>)> {
        TerminalHandle::new_with_pane_id(&spawn.working_directory, &spawn.shell, Some(pane_id))
    }

    fn ensure_started(&mut self, cx: &mut gpui::Context<Self>) {
        if self.empty_prompt || !matches!(self.terminal, TerminalState::Pending) {
            return;
        }
        match Self::spawn_terminal(&self.spawn, self.identity.pane_id()) {
            Ok((terminal, wakeup_rx)) => {
                Self::pump_terminal_events(terminal.clone(), wakeup_rx, cx);
                self.terminal = TerminalState::Running(terminal);
            }
            Err(error) => {
                self.terminal = TerminalState::Failed {
                    message: format!("{error:#}"),
                };
            }
        }
    }

    /// Re-attempts the spawn after a failure: on success the pane switches
    /// to the live terminal, on failure the message is updated in place.
    fn retry(&mut self, cx: &mut gpui::Context<Self>) {
        match Self::spawn_terminal(&self.spawn, self.identity.pane_id()) {
            Ok((terminal, wakeup_rx)) => {
                Self::pump_terminal_events(terminal.clone(), wakeup_rx, cx);
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
    /// `OUTPUT_SETTLE_DEBOUNCE` or `EVENT_COALESCE_CAP` events — before
    /// requesting a single redraw, so a flood of output does not cost one
    /// notify per byte.
    fn pump_terminal_events(
        terminal: TerminalHandle,
        mut wakeup_rx: UnboundedReceiver<Event>,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            loop {
                // Do not await the cross-thread channel. Its waker runs on
                // the PTY reader thread, which violates GPUI's deterministic
                // TestAppContext scheduler. Polling from this task keeps all
                // scheduler interaction on its owning executor.
                cx.background_executor().timer(EVENT_POLL_INTERVAL).await;
                let first_event = match wakeup_rx.try_recv() {
                    Ok(event) => event,
                    Err(TryRecvError::Closed) => return,
                    Err(TryRecvError::Empty) => continue,
                };
                let mut exit_status = Self::exit_status_from_event(&first_event);
                let mut osc_title = Self::osc_title_from_event(&first_event);
                let mut output_seen = matches!(first_event, Event::Wakeup);
                let mut pending = 1;
                // Coalesce the burst: keep draining after scheduler-owned
                // timer ticks until the channel is quiet or the cap is hit.
                while pending < EVENT_COALESCE_CAP {
                    cx.background_executor().timer(OUTPUT_SETTLE_DEBOUNCE).await;
                    let mut received = false;
                    while let Ok(event) = wakeup_rx.try_recv() {
                        if exit_status.is_none() {
                            exit_status = Self::exit_status_from_event(&event);
                        }
                        if let Some(title) = Self::osc_title_from_event(&event) {
                            osc_title = Some(title);
                        }
                        output_seen |= matches!(event, Event::Wakeup);
                        pending += 1;
                        received = true;
                        if pending >= EVENT_COALESCE_CAP {
                            break;
                        }
                    }
                    if !received {
                        break;
                    }
                }
                if this
                    .update(cx, |view, cx| {
                        if let Some(exit_status) = exit_status {
                            view.exit_status = Some(exit_status);
                            cx.emit(TerminalActivityEvent::ChildExited {
                                status: exit_status,
                            });
                        }
                        if let Some(title) = osc_title {
                            cx.emit(TerminalActivityEvent::OscTitle(title));
                        }
                        if output_seen {
                            let scrollback = recent_content_window(&String::from_utf8_lossy(
                                &terminal.capture_scrollback(),
                            ));
                            cx.emit(TerminalActivityEvent::OutputSettled { scrollback });
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

    fn osc_title_from_event(event: &Event) -> Option<String> {
        match event {
            Event::Title(title) => Some(title.clone()),
            // An OSC reset is observable title state too. An empty title lets
            // the activity model clear a title-owned pane through its normal
            // unmatched-title path without confusing it with SetTitle.
            Event::ResetTitle => Some(String::new()),
            _ => None,
        }
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

    fn on_left_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        if !opens_terminal_link(event.modifiers.platform) {
            return;
        }
        let Some(terminal) = self.running_terminal() else {
            return;
        };
        let column = (f32::from(event.position.x) / 8.0).floor().max(0.0) as usize;
        let row = (f32::from(event.position.y) / f32::from(LINE_HEIGHT))
            .floor()
            .max(0.0) as usize;
        if let Some(url) = terminal.link_at(row, column) {
            cx.emit(TerminalLinkEvent {
                target: self.identity.clone(),
                url,
            });
        }
    }

    fn emit_prompt(&mut self, action: TerminalPromptAction, cx: &mut gpui::Context<Self>) {
        cx.emit(TerminalPromptEvent {
            target: self.identity.clone(),
            action,
        });
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
            TerminalState::Pending | TerminalState::Failed { .. } => None,
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
            | TerminalContextAction::SplitLeft
            | TerminalContextAction::SplitRight
            | TerminalContextAction::SplitAbove
            | TerminalContextAction::SplitDown
            | TerminalContextAction::CloseTerminal => {
                cx.emit(TerminalContextEvent {
                    target: self.identity.clone(),
                    action,
                });
            }
        }
    }

    fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // F-CORE-TERM-02: the context menu previously opened only from
        // MouseButton::Right, with no keyboard-reachable path at all. The
        // conventional keyboard equivalents for "open context menu" are the
        // dedicated Menu key and Shift+F10; either opens the menu anchored
        // near the terminal's origin, since a keyboard event carries no
        // pointer position to anchor to.
        let key = event.keystroke.key.to_ascii_lowercase();
        let is_menu_key = key == "menu" || key == "contextmenu";
        let is_shift_f10 = key == "f10" && event.keystroke.modifiers.shift;
        if is_menu_key || is_shift_f10 {
            self.focus_handle.focus(window, cx);
            self.context_menu = Some(Point::new(px(20.0), px(20.0)));
            cx.notify();
            return;
        }
        if let TerminalState::Running(terminal) = &self.terminal {
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

impl Drop for TerminalView {
    fn drop(&mut self) {
        // GPUI can drop an entity before the host's quit hook runs. The
        // background event pump owns a handle clone, so relying on ordinary
        // field destruction would keep the PTY alive after the view vanished.
        self.shutdown();
    }
}

impl EventEmitter<TerminalContextEvent> for TerminalView {}
impl EventEmitter<TerminalActivityEvent> for TerminalView {}
impl EventEmitter<TerminalPromptEvent> for TerminalView {}
impl EventEmitter<TerminalLinkEvent> for TerminalView {}

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
        self.ensure_started(cx);
        let theme = *Theme::get(cx);
        let palette = TerminalPalette::from_theme(&theme);
        let terminal_entity = cx.entity();
        let file_drop_entity = terminal_entity.clone();
        let external_drop_entity = terminal_entity.clone();
        let dropped_path = self
            .last_dropped_diff
            .as_ref()
            .map(|payload| payload.0.display().to_string());
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
            TerminalState::Pending => {
                if !self.empty_prompt {
                    return div()
                        .size_full()
                        .bg(theme.terminal_surface)
                        .into_any_element();
                }
                div()
                    .size_full()
                    .bg(theme.terminal_surface)
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(theme.title)
                            .child("No terminal in this pane"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .id("terminal-new")
                                    .debug_selector(|| "terminal-new".to_owned())
                                    .px(px(12.0))
                                    .py(px(6.0))
                                    .rounded(px(6.0))
                                    .bg(theme.primary_pill_bg)
                                    .text_color(theme.title)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.emit_prompt(TerminalPromptAction::NewTerminal, cx);
                                    }))
                                    .child("New Terminal"),
                            )
                            .child(
                                div()
                                    .id("terminal-new-command")
                                    .debug_selector(|| "terminal-new-command".to_owned())
                                    .px(px(12.0))
                                    .py(px(6.0))
                                    .rounded(px(6.0))
                                    .bg(theme.card_fill)
                                    .text_color(theme.title)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.emit_prompt(
                                            TerminalPromptAction::NewTerminalWithCommand,
                                            cx,
                                        );
                                    }))
                                    .child("New…"),
                            ),
                    )
                    .when_some(context_menu, |this, menu| this.child(menu))
                    .into_any_element()
            }
            TerminalState::Running(terminal) => div()
                .size_full()
                .relative()
                .debug_selector(|| "terminal-drop-target".to_owned())
                .bg(theme.terminal_surface)
                .key_context("Terminal")
                .track_focus(&self.focus_handle)
                .on_mouse_down(MouseButton::Left, cx.listener(Self::on_left_mouse_down))
                .on_mouse_down(MouseButton::Right, cx.listener(Self::open_context_menu))
                .on_key_down(cx.listener(Self::on_key_down))
                .on_drop::<(PathBuf, String)>(move |payload: &(PathBuf, String), _, cx| {
                    terminal_entity.update(cx, |terminal, cx| {
                        terminal.receive_diff_drop(payload.clone(), cx);
                    });
                })
                .on_drop::<PathBuf>(move |path: &PathBuf, _, cx| {
                    file_drop_entity.update(cx, |terminal, cx| {
                        terminal.receive_file_drop(vec![path.clone()], cx);
                    });
                })
                // F-TERM-PTY-06: the in-app `on_drop::<PathBuf>` above only
                // ever catches GPUI's own typed drag payload (a Files-panel
                // row dragged within the app). A real OS-level file-manager
                // drag arrives as `gpui::ExternalPaths` (GPUI's XDND
                // payload), which can carry more than one path in a single
                // drop; both funnel into the same quoted insertion.
                .on_drop::<gpui::ExternalPaths>(move |paths: &gpui::ExternalPaths, _, cx| {
                    external_drop_entity.update(cx, |terminal, cx| {
                        terminal.receive_file_drop(paths.paths().to_vec(), cx);
                    });
                })
                .child(TerminalElement {
                    terminal: terminal.clone(),
                    palette,
                })
                .when_some(dropped_path, |this, path| {
                    this.child(
                        div()
                            .id("terminal-diff-drop")
                            .debug_selector(|| "terminal-diff-drop".to_owned())
                            .absolute()
                            .top(px(8.0))
                            .right(px(8.0))
                            .px(theme.spacing.titlebar_control_spacing)
                            .py(theme.spacing.titlebar_control_spacing)
                            .rounded(theme.radii.control)
                            .bg(theme.primary_pill_bg)
                            .text_size(theme.typography.caption2)
                            .text_color(theme.title)
                            .child(format!("Dropped diff: {path}")),
                    )
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

impl EventEmitter<TerminalDropEvent> for TerminalView {}

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
    use futures::StreamExt as _;
    use futures::channel::mpsc::TryRecvError;

    use super::*;

    fn screen_text(handle: &TerminalHandle) -> String {
        let (cells, _) = handle.snapshot();
        cells
            .iter()
            .flat_map(|row| row.iter().map(|cell| cell.c))
            .collect()
    }

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    fn test_working_directory(name: &str) -> PathBuf {
        let serial = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "tiller-terminal-test-{name}-{}-{serial}",
            std::process::id()
        ))
    }

    #[test]
    fn activity_content_is_limited_to_the_recent_lines_and_bytes() {
        let input = (0..100)
            .map(|line| format!("line-{line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let window = recent_content_window(&input);
        assert!(window.contains("line-99"));
        assert!(!window.contains("line-0"));
        assert!(window.lines().count() <= CONTENT_MATCH_LINE_LIMIT);
        assert!(window.len() <= CONTENT_MATCH_BYTE_LIMIT);
    }

    #[test]
    fn activity_content_keeps_utf8_boundaries_when_bounded_by_bytes() {
        let input = "é".repeat(CONTENT_MATCH_BYTE_LIMIT);
        let window = recent_content_window(&input);
        assert!(window.is_char_boundary(0));
        assert!(window.len() <= CONTENT_MATCH_BYTE_LIMIT);
        assert!(window.chars().all(|character| character == 'é'));
    }

    fn palette() -> TerminalPalette {
        TerminalPalette::from_theme(&Theme::light())
    }

    #[test]
    fn terminal_child_receives_the_pane_id_environment() {
        let working_directory = std::env::temp_dir().join(format!(
            "tiller-terminal-test-pane-env-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'pane=%s\\n' \"$TILLER_PANE_ID\"; exec sleep 0.1".to_string(),
            ],
        };
        let (handle, mut events) =
            TerminalHandle::new_with_pane_id(&working_directory, &shell, Some("pane-real-env"))
                .expect("spawn PTY");

        futures::executor::block_on(async {
            while let Some(event) = events.next().await {
                if matches!(event, Event::ChildExit(_)) {
                    break;
                }
            }
        });

        assert!(
            String::from_utf8_lossy(&handle.capture_scrollback()).contains("pane=pane-real-env"),
            "the PTY child must inherit TILLER_PANE_ID"
        );
        handle.shutdown();
        let _ = std::fs::remove_dir_all(working_directory);
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
        let working_directory = test_working_directory("scroll");
        std::fs::create_dir_all(&working_directory).unwrap();
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "i=0; while [ \"$i\" -lt 100 ]; do printf 'P4_SCROLL_%03d\\n' \"$i\"; i=$((i + 1)); done"
                    .to_string(),
            ],
        };
        let (handle, mut wakeup_rx) = TerminalHandle::new(&working_directory, &shell).unwrap();
        let output_reached_screen = futures::executor::block_on(async {
            while let Some(event) = wakeup_rx.next().await {
                if matches!(event, Event::Wakeup) && screen_text(&handle).contains("P4_SCROLL_099")
                {
                    return true;
                }
                if matches!(event, Event::ChildExit(_)) {
                    return screen_text(&handle).contains("P4_SCROLL_099");
                }
            }
            false
        });
        assert!(
            output_reached_screen,
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
        handle.shutdown();
        let _ = std::fs::remove_dir_all(working_directory);
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
        std::thread::sleep(Duration::from_millis(150));
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

    #[test]
    fn shutdown_terminates_the_entire_pty_process_group() {
        let working_directory = std::env::temp_dir().join(format!(
            "tiller-terminal-test-process-group-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).unwrap();
        let pid_file = working_directory.join("grandchild.pid");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                format!(
                    "trap '' HUP; sleep 60 & printf '%s' \"$!\" > {}; wait",
                    pid_file.display()
                ),
            ],
        };
        let (handle, _wakeup_rx) = TerminalHandle::new(&working_directory, &shell).unwrap();
        let process_group = ProcessGroupGuard(handle.shell_pid);

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let child_pid = loop {
            if let Ok(pid) = std::fs::read_to_string(&pid_file)
                && let Ok(pid) = pid.parse::<i32>()
            {
                break pid;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "PTY child did not publish its background process pid"
            );
            std::thread::sleep(Duration::from_millis(10));
        };

        assert!(
            process_is_running(child_pid),
            "background process exited before shutdown was exercised"
        );
        handle.shutdown();
        drop(handle);

        while std::time::Instant::now() < deadline && process_is_running(child_pid) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !process_is_running(child_pid),
            "PTY descendant {child_pid} survived shutdown of process group {}",
            process_group.0
        );
    }

    /// F-PER-06: a job-control-spawned child (a compound command, a
    /// backgrounded job under an interactive shell) can detach into its
    /// *own* process group, distinct from the shell's. `setsid` reproduces
    /// that deterministically — it puts its argument into a brand-new
    /// session and process group regardless of whether the parent shell
    /// happens to have job control enabled, which a plain `sleep 60 &`
    /// (the older, weaker test above) does not reliably do under a
    /// non-interactive `-c` shell. The old fix only ever `killpg`'d the
    /// pgid captured once at spawn — the shell's own group — so this
    /// grandchild used to survive shutdown as an orphan.
    #[test]
    fn shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group() {
        let working_directory = std::env::temp_dir().join(format!(
            "tiller-terminal-test-detached-group-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).unwrap();
        let pid_file = working_directory.join("detached.pid");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                format!(
                    "trap '' HUP; setsid sleep 60 & printf '%s' \"$!\" > {}; wait",
                    pid_file.display()
                ),
            ],
        };
        let (handle, _wakeup_rx) = TerminalHandle::new(&working_directory, &shell).unwrap();
        let process_group = ProcessGroupGuard(handle.shell_pid);

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let detached_pid = loop {
            if let Ok(pid) = std::fs::read_to_string(&pid_file)
                && let Ok(pid) = pid.parse::<i32>()
            {
                break pid;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "PTY child did not publish the setsid child's pid"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        let guard = PidGuard(detached_pid);

        assert!(
            process_is_running(detached_pid),
            "the setsid child exited before shutdown was exercised"
        );
        let detached_pgid = unsafe { libc::getpgid(detached_pid) };
        assert_ne!(
            detached_pgid, process_group.0 as libc::pid_t,
            "setsid must actually have put the child in a new process \
             group for this test to prove anything"
        );

        handle.shutdown();
        drop(handle);

        while std::time::Instant::now() < deadline && process_is_running(detached_pid) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !process_is_running(detached_pid),
            "the detached process group {detached_pgid} survived shutdown \
             of the shell's own group {}",
            process_group.0
        );
        drop(guard);
    }

    struct ProcessGroupGuard(u32);

    impl Drop for ProcessGroupGuard {
        fn drop(&mut self) {
            // The PTY creates a dedicated session/process group for its child.
            // This is test cleanup only; the assertion above must prove normal
            // shutdown made this fallback unnecessary.
            unsafe {
                let _ = libc::kill(-(self.0 as libc::pid_t), libc::SIGKILL);
            }
        }
    }

    /// Test cleanup for a single detached pid (its own session/group, so
    /// [`ProcessGroupGuard`]'s `killpg` on the shell's group cannot reach
    /// it). Same role as `ProcessGroupGuard`: a fallback that must prove
    /// unnecessary once the assertion above has run.
    struct PidGuard(i32);

    impl Drop for PidGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = libc::kill(self.0, libc::SIGKILL);
            }
        }
    }

    fn process_is_running(pid: i32) -> bool {
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            return false;
        };
        let Some((_, fields)) = stat.rsplit_once(") ") else {
            return false;
        };
        fields
            .as_bytes()
            .first()
            .is_some_and(|state| *state != b'Z')
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
    use gpui::{AppContext, Modifiers, MouseButton, VisualTestContext, point, px};
    use std::cell::RefCell;
    use std::rc::Rc;

    struct DiffDropFixture {
        terminal: gpui::Entity<TerminalView>,
        payload: (PathBuf, String),
    }

    struct FileDropFixture {
        terminal: gpui::Entity<TerminalView>,
        path: PathBuf,
    }

    /// F-TERM-PTY-06: a real OS-level file-manager drag lands as
    /// `gpui::ExternalPaths` (GPUI's XDND payload), distinct from the
    /// in-app typed drag `FileDropFixture` exercises above. XDND itself is
    /// not exercisable by this harness (no real window manager to drive
    /// it), but the `on_drop::<ExternalPaths>` handler on the terminal does
    /// not care whether the payload arrived from a real XDND drop or an
    /// in-app drag carrying the same type — both dispatch through GPUI's
    /// identical typed-drop matching. Driving that handler with an in-app
    /// `ExternalPaths` drag is therefore a real exercise of the production
    /// code this row was missing, not a simulation of a simulation.
    struct ExternalFileDropFixture {
        terminal: gpui::Entity<TerminalView>,
        paths: gpui::ExternalPaths,
    }

    impl gpui::Render for DiffDropFixture {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .id("terminal-test-drag-source")
                        .debug_selector(|| "terminal-test-drag-source".to_owned())
                        .h(px(40.0))
                        .on_drag(self.payload.clone(), |_, _, _, cx| cx.new(|_| gpui::Empty))
                        .child("diff source"),
                )
                .child(self.terminal.clone())
        }
    }

    impl gpui::Render for FileDropFixture {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            div()
                .size_full()
                .child(
                    div()
                        .id("terminal-file-test-drag-source")
                        .debug_selector(|| "terminal-file-test-drag-source".to_owned())
                        .h(px(40.0))
                        .on_drag(self.path.clone(), |_, _, _, cx| cx.new(|_| gpui::Empty))
                        .child("file source"),
                )
                .child(self.terminal.clone())
        }
    }

    impl gpui::Render for ExternalFileDropFixture {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            div()
                .size_full()
                .child(
                    div()
                        .id("terminal-external-drag-source")
                        .debug_selector(|| "terminal-external-drag-source".to_owned())
                        .h(px(40.0))
                        .on_drag(self.paths.clone(), |_, _, _, cx| cx.new(|_| gpui::Empty))
                        .child("external files source"),
                )
                .child(self.terminal.clone())
        }
    }

    #[test]
    fn conflict_resolution_shell_quotes_the_exact_repo_relative_path() {
        let shell = conflict_resolution_shell(Path::new("src/conflicted file.txt"));
        let TerminalShell::WithArguments { program, args } = shell else {
            panic!("conflict launch must be a shell command");
        };
        assert_eq!(program, "/bin/sh");
        assert_eq!(args[0], "-lc");
        assert!(args[1].contains("git diff --cc -- 'src/conflicted file.txt'"));
        assert!(args[1].contains("Resolve conflict at"));
    }

    /// The drawn terminal created for a conflict keeps the repository root
    /// and exact conflicted path in its launch contract.
    #[gpui::test]
    async fn a_conflict_terminal_is_drawn_with_the_exact_path(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let repo = std::env::temp_dir().join(format!(
            "tiller-terminal-conflict-launch-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&repo).expect("create conflict repo");
        let path = PathBuf::from("src/conflicted file.txt");
        let window = cx.add_window(|_, cx| {
            TerminalView::for_conflict(&repo, &path, cx).expect("create conflict terminal")
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let terminal = cx.update(|window, _| {
            window
                .root::<TerminalView>()
                .flatten()
                .expect("terminal root")
        });

        assert_eq!(
            terminal.read_with(&cx.cx, |terminal, _| terminal
                .working_directory()
                .to_path_buf()),
            repo
        );
        assert!(
            terminal.read_with(&cx.cx, |terminal, _| match terminal.launch_shell() {
                TerminalShell::WithArguments { args, .. } => {
                    args.get(1)
                        .is_some_and(|command| command.contains("'src/conflicted file.txt'"))
                }
                TerminalShell::System => false,
            })
        );
        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
        let _ = std::fs::remove_dir_all(repo);
    }

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

    /// A real PTY must expose the shell's OSC title and its settled scrollback
    /// through the terminal entity. This is intentionally a drawn test: the
    /// event pump must be alive, and every scheduler turn is fully drained.
    #[gpui::test]
    async fn real_pty_emits_osc_title_and_settled_output(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "tiller-terminal-activity-events-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "sleep 0.1; printf '\\033]0;✳ idle\\007'; printf 'Do you want to proceed?\\n'; exec sleep 1"
                    .to_string(),
            ],
        };
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn PTY")
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let collected = events.clone();
        let _subscription = cx.update(|_, app| {
            app.subscribe(&terminal, move |_, event: &TerminalActivityEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
            cx.run_until_parked();
            let events = events.borrow();
            let title_seen = events
                .iter()
                .any(|event| event == &TerminalActivityEvent::OscTitle("✳ idle".to_string()));
            let content_seen = events.iter().any(|event| {
                matches!(event, TerminalActivityEvent::OutputSettled { scrollback }
                    if scrollback.contains("Do you want to proceed?"))
            });
            if title_seen && content_seen {
                break;
            }
            drop(events);
            std::thread::sleep(Duration::from_millis(10));
        }

        let events = events.borrow();
        let snapshot = terminal.update(&mut cx.cx, |terminal, _| terminal.snapshot());
        assert!(
            events
                .iter()
                .any(|event| event == &TerminalActivityEvent::OscTitle("✳ idle".to_string())),
            "OSC title did not cross the PTY listener boundary: {events:?}; scrollback: {:?}",
            String::from_utf8_lossy(&snapshot.scrollback)
        );
        assert!(
            events.iter().any(|event| {
                matches!(event, TerminalActivityEvent::OutputSettled { scrollback }
                    if scrollback.contains("Do you want to proceed?"))
            }),
            "settled PTY scrollback did not reach the activity surface: {events:?}"
        );
        drop(events);
        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
    }

    #[gpui::test]
    async fn stable_identity_is_inherited_by_the_lazily_spawned_pty(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "tiller-terminal-pane-identity-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'pane=%s\\n' \"$TILLER_PANE_ID\"; exec sleep 1".to_string(),
            ],
        };
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            let mut view = TerminalView::with_shell(&working_directory, shell, cx)
                .expect("create lazy terminal");
            view.set_identity(TerminalIdentity::new("pane-stable", "terminal-stable"));
            view
        });
        let scrollback = Rc::new(RefCell::new(Vec::new()));
        let observed = scrollback.clone();
        let _subscription = cx.update(|_, app| {
            app.subscribe(&terminal, move |_, event: &TerminalActivityEvent, _| {
                if let TerminalActivityEvent::OutputSettled { scrollback } = event {
                    observed.borrow_mut().push(scrollback.clone());
                }
            })
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
            cx.run_until_parked();
            if scrollback
                .borrow()
                .iter()
                .any(|text| text.contains("pane=pane-stable"))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            scrollback
                .borrow()
                .iter()
                .any(|text| text.contains("pane=pane-stable")),
            "stable pane identity must reach the PTY child: {:?}",
            scrollback.borrow()
        );
        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
    }

    /// F-TERM-UI-02: a platform-modifier (Super on Linux) left click on a
    /// URL opens the link; a plain left click on the same text does not.
    /// Driven through the real `on_left_mouse_down` dispatch on a live PTY
    /// with a real URL rendered into the emulator grid — the Wayland lane's
    /// pointer driver has no modifier-click primitive, so this is proven as
    /// a unit test instead.
    #[gpui::test]
    async fn platform_modifier_click_opens_a_terminal_link(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "tiller-terminal-link-click-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'https://example.test/docs\\n'; exec sleep 60".to_string(),
            ],
        };
        let window = cx.add_window(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn terminal")
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);

        let terminal = cx.update(|window, _| {
            window
                .root::<TerminalView>()
                .flatten()
                .expect("terminal root")
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, app| {
            app.subscribe(&terminal, move |_, event: &TerminalLinkEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            cx.run_until_parked();
            if terminal.read_with(&cx.cx, |terminal, _| {
                terminal
                    .running_terminal()
                    .and_then(|t| t.link_at(0, 0))
                    .is_some()
            }) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the URL never rendered into the emulator grid"
            );
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
            std::thread::sleep(Duration::from_millis(10));
        }

        let target = cx
            .debug_bounds("terminal-drop-target")
            .expect("terminal is drawn");
        let link_point = point(target.origin.x + px(4.0), target.origin.y + px(9.0));

        // A plain left click on the URL does not open it.
        cx.simulate_mouse_down(link_point, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert!(
            events.borrow().is_empty(),
            "a plain click must not open a terminal link: {:?}",
            events.borrow()
        );

        // The same click held with the platform modifier (Super on Linux)
        // opens it.
        cx.simulate_mouse_down(
            link_point,
            MouseButton::Left,
            Modifiers {
                platform: true,
                ..Modifiers::none()
            },
        );
        cx.run_until_parked();
        assert_eq!(
            events.borrow().last().map(|event| event.url.clone()),
            Some("https://example.test/docs".to_string()),
            "a platform-modifier click on the URL opens it"
        );

        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
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

        for index in 0..12 {
            let selector: &'static str =
                Box::leak(format!("terminal-context-item-{index}").into_boxed_str());
            assert!(
                cx.debug_bounds(selector).is_some(),
                "context menu item {index} must be drawn"
            );
        }

        let split_left = cx
            .debug_bounds("terminal-context-item-6")
            .expect("Split Left item");
        cx.simulate_click(split_left.center(), Modifiers::none());
        let event = events.borrow().last().cloned().expect("split event");
        assert_eq!(event.action, TerminalContextAction::SplitLeft);
        assert!(event.target.pane_id().starts_with("pane-"));
        assert!(event.target.terminal_id().starts_with("terminal-"));

        assert!(cx.update(|_, cx| terminal.read(cx).is_failed()));
    }

    /// F-TERM-06: "Copy Pane ID" and "Paste" were only ever proven by
    /// inspecting the render tree and the clipboard-write call site — never
    /// by actually clicking through the menu and reading back what a real
    /// clipboard held. This drives both context-menu items through the same
    /// real mouse gesture the split test above uses, and closes the loop
    /// through a live `cat` PTY: `Copy Pane ID` must write a real clipboard
    /// entry, and `Paste` must feed that exact entry back into the terminal,
    /// where the shell echoes it into the scrollback.
    #[gpui::test]
    async fn copy_pane_id_then_paste_round_trips_through_the_context_menu(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "tiller-terminal-clipboard-roundtrip-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "exec cat".to_string()],
        };
        let window = cx.add_window(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn PTY")
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

        let click = point(px(40.0), px(40.0));
        cx.simulate_mouse_down(click, MouseButton::Right, Modifiers::none());
        let copy_pane_id = cx
            .debug_bounds("terminal-context-item-4")
            .expect("Copy Pane ID item must be drawn");
        cx.simulate_click(copy_pane_id.center(), Modifiers::none());
        cx.run_until_parked();

        let clipboard_text = cx
            .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
            .expect("Copy Pane ID must write a real clipboard entry");
        assert!(
            clipboard_text.starts_with("pane-"),
            "clipboard held {clipboard_text:?}, not a pane id"
        );

        cx.simulate_mouse_down(click, MouseButton::Right, Modifiers::none());
        let paste = cx
            .debug_bounds("terminal-context-item-1")
            .expect("Paste item must be drawn");
        cx.simulate_click(paste.center(), Modifiers::none());

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut captured = String::new();
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            captured = cx.update(|_, cx| {
                String::from_utf8_lossy(&terminal.read(cx).capture_scrollback()).into_owned()
            });
            if captured.contains(&clipboard_text) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            captured.contains(&clipboard_text),
            "Paste did not feed the clipboard's pane id back into the terminal: {captured:?}"
        );

        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
        let _ = std::fs::remove_dir_all(working_directory);
    }

    /// A dropped diff is delivered through the same real mouse gesture GPUI
    /// uses for payload drags, and the terminal entity records the exact
    /// payload for the host to consume.
    #[gpui::test]
    async fn a_drawn_terminal_receives_a_diff_payload_drop(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory =
            std::env::temp_dir().join(format!("tiller-terminal-diff-drop-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).expect("create drop directory");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "exec sleep 60".to_string()],
        };
        let payload = (
            PathBuf::from("src/conflicted file.txt"),
            "@@ -1 +1 @@\n-old\n+new\n".to_string(),
        );
        let window = cx.add_window(|_, cx| {
            let terminal = cx.new(|cx| {
                TerminalView::with_shell(&working_directory, shell, cx).expect("spawn terminal")
            });
            DiffDropFixture {
                terminal,
                payload: payload.clone(),
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let fixture = cx.update(|window, _| {
            window
                .root::<DiffDropFixture>()
                .flatten()
                .expect("fixture root")
        });
        let terminal = fixture.read_with(&cx.cx, |fixture, _| fixture.terminal.clone());
        let events = Rc::new(RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, app| {
            app.subscribe(&terminal, move |_, event: &TerminalDropEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        let target = cx
            .debug_bounds("terminal-drop-target")
            .expect("terminal is a drawn drop target");
        let source = cx
            .debug_bounds("terminal-test-drag-source")
            .expect("test source is drawn");
        cx.simulate_event(gpui::MouseDownEvent {
            position: source.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(gpui::MouseMoveEvent {
            position: point(source.center().x + px(8.0), source.center().y),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(gpui::MouseMoveEvent {
            position: target.center(),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: target.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();

        assert_eq!(
            events.borrow().as_slice(),
            &[TerminalDropEvent::Diff {
                path: payload.0.clone(),
                text: payload.1.clone(),
            }],
            "the terminal receives the complete diff payload"
        );
        assert_eq!(
            terminal.read_with(&cx.cx, |terminal, _| terminal.last_dropped_diff().cloned()),
            Some(payload),
            "the terminal keeps the dropped diff available to its host"
        );
        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
    }

    /// F-CORE-TERM-02: the context menu must also open from the keyboard,
    /// not only from a right-click. Shift+F10 is the conventional
    /// keyboard-context-menu chord; this drives it through the real
    /// `on_key_down` dispatch path on a focused, drawn terminal.
    #[gpui::test]
    async fn shift_f10_opens_the_context_menu_from_the_keyboard(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "tiller-terminal-keyboard-menu-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create terminal directory");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "exec sleep 60".to_string()],
        };
        let window = cx.add_window(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn terminal")
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let terminal = cx.update(|window, _| {
            window
                .root::<TerminalView>()
                .flatten()
                .expect("terminal root")
        });
        assert!(
            terminal.read_with(&cx.cx, |terminal, _| terminal.context_menu.is_none()),
            "menu starts closed"
        );

        let target = cx
            .debug_bounds("terminal-drop-target")
            .expect("terminal is drawn");
        cx.simulate_click(target.center(), Modifiers::none());
        cx.run_until_parked();

        cx.simulate_keystrokes("shift-f10");
        cx.run_until_parked();

        assert!(
            terminal.read_with(&cx.cx, |terminal, _| terminal.context_menu.is_some()),
            "Shift+F10 opens the context menu with no mouse click involved"
        );
        assert!(
            cx.debug_bounds("terminal-context-item-0").is_some(),
            "the keyboard-opened menu actually draws its items"
        );

        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
    }

    #[gpui::test]
    async fn a_drawn_terminal_inserts_a_quoted_file_drop_without_a_newline(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let working_directory =
            std::env::temp_dir().join(format!("tiller-terminal-file-drop-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).expect("create drop directory");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "exec sleep 60".to_string()],
        };
        let path = PathBuf::from("src/file with spaces.rs");
        let window = cx.add_window(|_, cx| {
            let terminal = cx.new(|cx| {
                TerminalView::with_shell(&working_directory, shell, cx).expect("spawn terminal")
            });
            FileDropFixture {
                terminal,
                path: path.clone(),
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let fixture = cx.update(|window, _| {
            window
                .root::<FileDropFixture>()
                .flatten()
                .expect("fixture root")
        });
        let terminal = fixture.read_with(&cx.cx, |fixture, _| fixture.terminal.clone());
        let events = Rc::new(RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, app| {
            app.subscribe(&terminal, move |_, event: &TerminalDropEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        let target = cx
            .debug_bounds("terminal-drop-target")
            .expect("terminal is a drawn drop target");
        let source = cx
            .debug_bounds("terminal-file-test-drag-source")
            .expect("test source is drawn");
        cx.simulate_event(gpui::MouseDownEvent {
            position: source.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(gpui::MouseMoveEvent {
            position: point(source.center().x + px(8.0), source.center().y),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(gpui::MouseMoveEvent {
            position: target.center(),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: target.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();

        assert_eq!(
            events.borrow().as_slice(),
            &[TerminalDropEvent::Files {
                paths: vec![path.clone()]
            }]
        );
        assert_eq!(
            terminal.read_with(&cx.cx, |terminal, _| terminal
                .last_dropped_files()
                .map(<[PathBuf]>::to_vec)),
            Some(vec![path])
        );
        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
    }

    #[gpui::test]
    async fn a_drawn_terminal_accepts_a_real_external_paths_drop_with_several_files(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "tiller-terminal-external-drop-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create drop directory");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "exec sleep 60".to_string()],
        };
        let dropped = gpui::ExternalPaths(
            [
                PathBuf::from("src/one.rs"),
                PathBuf::from("src/two with spaces.rs"),
            ]
            .into_iter()
            .collect(),
        );
        let window = cx.add_window(|_, cx| {
            let terminal = cx.new(|cx| {
                TerminalView::with_shell(&working_directory, shell, cx).expect("spawn terminal")
            });
            ExternalFileDropFixture {
                terminal,
                paths: dropped.clone(),
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let fixture = cx.update(|window, _| {
            window
                .root::<ExternalFileDropFixture>()
                .flatten()
                .expect("fixture root")
        });
        let terminal = fixture.read_with(&cx.cx, |fixture, _| fixture.terminal.clone());
        let events = Rc::new(RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, app| {
            app.subscribe(&terminal, move |_, event: &TerminalDropEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        let target = cx
            .debug_bounds("terminal-drop-target")
            .expect("terminal is a drawn drop target");
        let source = cx
            .debug_bounds("terminal-external-drag-source")
            .expect("test source is drawn");
        cx.simulate_event(gpui::MouseDownEvent {
            position: source.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(gpui::MouseMoveEvent {
            position: point(source.center().x + px(8.0), source.center().y),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(gpui::MouseMoveEvent {
            position: target.center(),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: target.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();

        assert_eq!(
            events.borrow().as_slice(),
            &[TerminalDropEvent::Files {
                paths: dropped.paths().to_vec()
            }],
            "an ExternalPaths drop with several files reaches the terminal, \
             not just GPUI's in-app single-path typed drag"
        );
        assert_eq!(
            terminal.read_with(&cx.cx, |terminal, _| terminal
                .last_dropped_files()
                .map(<[PathBuf]>::to_vec)),
            Some(dropped.paths().to_vec())
        );
        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
    }
}
