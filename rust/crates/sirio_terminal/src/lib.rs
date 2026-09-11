//! A small GPUI terminal backed by libghostty-vt (VT state) and portable-pty
//! (PTY). Stage 2 of #45: the paint loop consumes grapheme clusters per cell
//! and every SGR attribute gpui can express — bold, italic, underline (+
//! color, styles degraded to gpui's ceiling per #31), strikethrough, inverse,
//! faint, invisible.

use std::{
    collections::HashMap,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    sync::Arc,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};

use futures::channel::mpsc::{TryRecvError, UnboundedReceiver};

use anyhow::{Context as _, Result};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, Bounds, ClipboardItem, ContentMask, Corners, Element, ElementId, EventEmitter, Font,
    FontStyle, FontWeight, GlobalElementId, Hsla, InteractiveElement, IntoElement, KeyDownEvent,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, ParentElement,
    Pixels, Point, RenderImage, ScrollDelta, ScrollWheelEvent, ShapedLine, Size,
    StatefulInteractiveElement, StrikethroughStyle, Style, Styled, TextRun, UnderlineStyle, Window,
    anchored, deferred, div, fill, font, point, px, relative, rgba, size,
};
use image::{Frame, RgbaImage};
use libghostty_vt::alloc::{Allocator, Bytes};
use libghostty_vt::kitty::graphics::{
    DecodePng, DecodedImage, Graphics, ImageFormat, Layer, PlacementIteration, PlacementIterator,
};
use libghostty_vt::{
    Error, RenderState, Terminal, TerminalOptions, key, mouse,
    render::{CellIteration, CellIterator, Colors, RowIterator},
    screen::{CellWide, GridRef},
    selection::{FormatOptions, SelectLineOptions, SelectWordOptions, Selection},
    style::{StyleColor, Underline},
    terminal::{Mode, Point as GhosttyPoint, PointCoordinate, PointSpace, ScrollViewport},
};
use parking_lot::Mutex;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use sirio_theme::Theme;

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
pub use link_router::{opens_terminal_link, resolve_click_cell, url_at_column};

const FONT_SIZE: Pixels = px(13.0);
const LINE_HEIGHT: Pixels = px(18.0);
const MIN_FONT_SIZE: i32 = 12;
const MAX_FONT_SIZE: i32 = 18;

fn line_height_for_font_size(font_size: Pixels) -> Pixels {
    px((f32::from(font_size) * f32::from(LINE_HEIGHT) / f32::from(FONT_SIZE))
        .round()
        .max(1.0))
}
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
/// The shell left behind is the user's own. `exec`ing a hardcoded `/bin/sh`
/// dropped a POSIX-minimal shell on someone whose `$SHELL` is fish or zsh,
/// with none of their history, prompt or aliases, in the middle of resolving
/// a conflict.
///
/// Outstanding Windows port item: the command *grammar* below is POSIX —
/// `;` sequencing, `printf`, `exec`, and [`shell_quote`]'s single-quoting.
/// The shell it runs in is resolved per platform, but cmd.exe would not
/// understand this line. Porting it needs a cmd/PowerShell counterpart, not
/// a different program name.
pub fn conflict_resolution_shell(path: &Path) -> TerminalShell {
    let quoted_path = shell_quote(path);
    let interactive = shell_quote(Path::new(&user_shell_program()));
    let (program, args) = command_shell_invocation(&format!(
        "git diff --cc -- {quoted_path}; printf '\\nResolve conflict at %s\\n' {quoted_path}; exec {interactive} -il"
    ));
    TerminalShell::WithArguments { program, args }
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
    /// Facts the surrounding shell renders independently of terminal
    /// contents. Emitted only when a spawn/restart attempt changes them, so
    /// the host never needs to observe content-driven repaint notifications.
    LifecycleChanged {
        failed: bool,
        exit_status: Option<TerminalExitStatus>,
    },
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

    #[cfg_attr(not(test), allow(dead_code))] // exercised by the exit-status unit test
    fn from_signal(signal: i32) -> Self {
        Self::Signal(signal)
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

/// Events a terminal's owner thread emits to the view's pump. Sirio's own
/// enum — libghostty-vt has no event system of its own; state (title, exit)
/// is read off the `Terminal` and diffed into these.
#[derive(Clone, Debug)]
enum TerminalEvent {
    /// PTY output was fed into the emulator.
    Wakeup,
    /// The viewport moved in response to local scrolling.
    ViewportChanged,
    /// The OSC title changed. An empty string is an OSC title reset.
    Title(String),
    /// The PTY child exited after its final output was drained.
    ChildExit(TerminalExitStatus),
}

/// Codepoints kept inline in one [`SnapshotCell`] before truncation. Flags
/// are 2, ZWJ emoji families reach ~7; anything longer is a curiosity.
const GRAPHEME_INLINE: usize = 8;

/// One cell of the paint-loop snapshot: its full grapheme cluster plus every
/// style attribute the emulator tracks. `inverse` and `invisible` are
/// resolved here (fg/bg swapped; cluster cleared), so the renderer only ever
/// sees final values (#45).
#[derive(Clone)]
struct SnapshotCell {
    cluster: [char; GRAPHEME_INLINE],
    cluster_len: usize,
    bold: bool,
    italic: bool,
    underline: Underline,
    /// Resolved RGB for SGR 58; None paints with the cell's foreground.
    underline_color: Option<SirioColor>,
    strikethrough: bool,
    faint: bool,
    fg: SirioColor,
    bg: SirioColor,
    /// The cell's OSC 8 hyperlink target when the emulator state carries one
    /// (#41). Interned per snapshot by `build_snapshot` — every cell of a link
    /// run shares one `Arc<str>` rather than cloning a String each.
    hyperlink: Option<Arc<str>>,
}

impl SnapshotCell {
    /// The characters this column renders: the full cluster, or a single
    /// space when the cell holds none (blank, or the continuation half of a
    /// wide character).
    ///
    /// SpacerTail pin: libghostty-vt documents SpacerTail as "Do not
    /// render." Sirio pushes a SPACE instead — gpui shapes whole lines, and
    /// only the placeholder keeps following columns aligned. Pinned by
    /// `spacer_tail_renders_as_a_space_so_columns_stay_aligned`.
    fn chars(&self) -> impl Iterator<Item = char> + '_ {
        let cluster = self.cluster;
        let len = self.cluster_len;
        (0..len.max(1)).map(move |i| if i < len { cluster[i] } else { ' ' })
    }

    /// UTF-8 byte length of [`SnapshotCell::chars`] — what a TextRun.len counts.
    fn byte_len(&self) -> usize {
        self.chars().map(char::len_utf8).sum()
    }
}

/// Commands sent to a terminal's dedicated owner thread.
///
/// libghostty-vt types are all `!Send`/`!Sync`, so unlike alacritty's
/// `FairMutex<Term>` there is no shared terminal object at all: each terminal
/// lives on exactly one thread, and every other caller reaches it through
/// this channel. Blocking round-trips (Snapshot/Text) wait on a oneshot-ish
/// reply channel; the owner loop polls with a small sleep so worst-case
/// command latency is bounded by [`EVENT_POLL_INTERVAL`].
/// #259: what a repeated click selects around the cell under the pointer.
///
/// Resolved by libghostty-vt, not here: word boundaries know about wide
/// characters and semantic prompt marks, and a line knows whether it was soft
/// wrapped. Reimplementing either from a cell snapshot gets the easy cases
/// right and the ones that matter wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickSelection {
    /// Double click.
    Word,
    /// Triple click.
    Line,
}

impl ClickSelection {
    /// GPUI counts clicks for us; anything past three repeats the line, which
    /// is what a terminal user expects from a fourth click.
    pub fn for_click_count(count: usize) -> Option<Self> {
        match count {
            0 | 1 => None,
            2 => Some(Self::Word),
            _ => Some(Self::Line),
        }
    }
}

/// #259: the selected region, in viewport grid coordinates, as plain data.
///
/// libghostty-vt owns the real `Selection` -- it is what `format_selection_alloc`
/// reads for Copy -- but that type borrows the terminal and cannot cross to the
/// render thread, and the terminal itself is `!Send`. Painting the highlight
/// through `Selection::contains` would also mean one FFI call per cell per
/// frame across the whole viewport, which is exactly the per-cell cost the
/// Raspberry Pi 5 budget cannot carry.
///
/// So the owner thread snapshots the resolved range into these four numbers and
/// the paint loop does ordinary arithmetic against them, tinting background
/// quads it already emits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedRange {
    /// First selected cell, inclusive.
    pub start: (usize, usize),
    /// Last selected cell, inclusive.
    pub end: (usize, usize),
}

impl SelectedRange {
    /// Builds a range from two endpoints in either order, so a drag upwards or
    /// leftwards selects the same cells as the same drag reversed.
    pub fn between(a: (usize, usize), b: (usize, usize)) -> Self {
        if (a.0, a.1) <= (b.0, b.1) {
            Self { start: a, end: b }
        } else {
            Self { start: b, end: a }
        }
    }

    /// Whether a viewport cell falls inside the selection.
    ///
    /// Linewise, not rectangular: a selection spanning lines takes every cell
    /// after the anchor on the first line, all of each line between, and every
    /// cell up to the focus on the last. Rectangle selection is explicitly out
    /// of scope for #259.
    pub fn contains(&self, line: usize, column: usize) -> bool {
        if line < self.start.0 || line > self.end.0 {
            return false;
        }
        if self.start.0 == self.end.0 {
            return column >= self.start.1 && column <= self.end.1;
        }
        if line == self.start.0 {
            return column >= self.start.1;
        }
        if line == self.end.0 {
            return column <= self.end.1;
        }
        true
    }

    /// Whether the range covers no cells at all -- a click with no drag, which
    /// must clear the highlight rather than tint a single cell.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

struct KeyInput {
    action: key::Action,
    key: key::Key,
    mods: key::Mods,
    consumed_mods: key::Mods,
    utf8: Option<String>,
    unshifted_codepoint: Option<char>,
}

/// Plain-data description of one mouse event (#43), converted from GPUI's
/// window-space events by [`mouse_input`] and encoded against guest-controlled
/// terminal state on the owner thread. Like [`KeyInput`], it carries everything
/// the encoder needs so libghostty-vt types never cross the command channel.
#[derive(Clone, Copy, Debug, PartialEq)]
struct MouseInput {
    action: mouse::Action,
    button: Option<mouse::Button>,
    mods: key::Mods,
    /// Whether any button is held at the moment this event fires — from
    /// GPUI move events' `pressed_button`, true/false around press/release.
    any_button_pressed: bool,
    /// Pane-local surface-space pixels (window position minus pane origin).
    x: f32,
    y: f32,
    /// Geometry snapshot so the owner thread keeps `EncoderSize` current.
    pane_width: f32,
    pane_height: f32,
    cell_width: f32,
    cell_height: f32,
}

/// One persistent libghostty mouse encoder plus the state whose setters clear
/// its internal last-cell motion dedup. We only reapply changed values; calling
/// either setter unconditionally per event in libghostty-vt 0.2.1 would erase
/// the dedup state immediately before every motion encode.
struct MouseEncoderState<'alloc> {
    encoder: mouse::Encoder<'alloc>,
    terminal_modes: Option<[bool; 8]>,
    size: Option<mouse::EncoderSize>,
}

impl MouseEncoderState<'static> {
    fn new() -> std::result::Result<Self, Error> {
        Ok(Self {
            encoder: mouse::Encoder::new()?,
            terminal_modes: None,
            size: None,
        })
    }
}

/// Snapshot cells and cursor position returned by the terminal owner thread.
type SnapshotReply = (Vec<Vec<SnapshotCell>>, (usize, usize));

enum TerminalCommand {
    /// Write bytes to the PTY (file drops and programmatic input).
    Input(Vec<u8>),
    /// Write clipboard bytes to the PTY, using bracketed paste when enabled.
    Paste(Vec<u8>),
    /// Encode a keyboard event against the guest-controlled terminal state.
    Key(KeyInput),
    /// Encode a mouse event against the guest-controlled terminal state.
    Mouse(MouseInput),
    /// Feed bytes straight into the VT parser without touching the PTY
    /// (`replay_scrollback`, `clear_screen`).
    Feed(Vec<u8>),
    Resize(u16, u16, u16, u16),
    Scroll(SirioScroll),
    Snapshot(std::sync::mpsc::Sender<SnapshotReply>),
    /// #301: every visible Kitty placement for this frame as plain owned data
    /// (R2.3 — geometry plus raw pixels; never decoded on the owner thread).
    KittyPlaces(std::sync::mpsc::Sender<KittyPlacementBuckets>),
    Text(std::sync::mpsc::Sender<String>),
    /// #259: the text under a selected range, formatted by the emulator
    /// rather than re-extracted here -- it knows about soft wrapping and
    /// trailing blanks, which slicing a cell snapshot does not.
    SelectionText(SelectedRange, std::sync::mpsc::Sender<String>),
    /// #259: resolve a double or triple click into a range, using the
    /// emulator's own word and line rules.
    ClickSelect(
        (usize, usize),
        ClickSelection,
        std::sync::mpsc::Sender<Option<SelectedRange>>,
    ),
    Shutdown,
}

/// Viewport scroll requests, mirroring the subset of alacritty's `grid::Scroll`
/// the crate used.
#[cfg_attr(not(test), allow(dead_code))] // Top/Bottom constructed from tests
#[derive(Clone, Copy, Debug)]
enum SirioScroll {
    Top,
    Bottom,
    PageUp,
    PageDown,
    /// #259: line-granular scroll, negative upwards. Autoscroll needs one
    /// line at a time; a page per tick would fly past whatever the user was
    /// dragging towards.
    Lines(isize),
}

/// A cheap, clonable reader for a live terminal's retained scrollback.
///
/// It is `Send` and `Sync` because it only holds a clone of the owner
/// thread's command sender and a synchronized pending reply. A held sender
/// clone keeps that command channel alive; terminal shutdown remains explicit,
/// and `Shutdown` makes the owner thread return so later captures fail cleanly
/// when the receiver is dropped.
#[derive(Clone)]
pub struct ScrollbackCapture {
    commands: std::sync::mpsc::Sender<TerminalCommand>,
    pending: Arc<Mutex<Option<std::sync::mpsc::Receiver<String>>>>,
}

impl ScrollbackCapture {
    fn new(commands: std::sync::mpsc::Sender<TerminalCommand>) -> Self {
        Self {
            commands,
            pending: Arc::new(Mutex::new(None)),
        }
    }

    /// Polls normalized retained scrollback from the terminal owner thread.
    /// `None` means the owner is still busy; the pending request is retained so
    /// the next poll can collect its reply without enqueueing duplicate work.
    fn try_capture(&self) -> Option<Vec<u8>> {
        let mut pending = self.pending.lock();
        let pending_result = pending.as_ref().map(|reply_rx| reply_rx.try_recv());
        match pending_result {
            Some(Ok(text)) => {
                pending.take();
                Some(text.into_bytes())
            }
            Some(Err(std::sync::mpsc::TryRecvError::Empty)) => None,
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                pending.take();
                Some(Vec::new())
            }
            None => {
                let (reply_tx, reply_rx) = std::sync::mpsc::channel();
                if self.commands.send(TerminalCommand::Text(reply_tx)).is_err() {
                    return Some(Vec::new());
                }
                match reply_rx.try_recv() {
                    Ok(text) => Some(text.into_bytes()),
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        *pending = Some(reply_rx);
                        None
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Vec::new()),
                }
            }
        }
    }

    /// Captures normalized retained scrollback on the terminal owner thread.
    /// This is the synchronous, fresh-read API used by explicit snapshot and
    /// control-socket consumers. The terminal event pump uses [`Self::try_capture`]
    /// instead, so output settling never waits on this reply.
    pub fn capture(&self) -> Vec<u8> {
        let reply_rx = {
            let mut pending = self.pending.lock();
            if let Some(reply_rx) = pending.take() {
                reply_rx
            } else {
                let (reply_tx, reply_rx) = std::sync::mpsc::channel();
                if self.commands.send(TerminalCommand::Text(reply_tx)).is_err() {
                    return Vec::new();
                }
                reply_rx
            }
        };
        reply_rx
            .recv()
            .map(String::into_bytes)
            .unwrap_or_default()
    }
}

#[derive(Clone)]
struct TerminalHandle {
    /// The terminal's dedicated owner thread. All emulator access funnels
    /// through it (see [`TerminalCommand`]).
    commands: std::sync::mpsc::Sender<TerminalCommand>,
    scrollback: ScrollbackCapture,
    last_size: Arc<Mutex<Option<(u16, u16)>>>,
    shell_pid: u32,
    shutdown_started: Arc<AtomicBool>,
    resize_generation: Arc<AtomicU64>,
    /// The terminal grid's last-painted window-space bounds. `TerminalElement`
    /// (used only inside `Render`, no access to `TerminalView`'s fields)
    /// writes this every `prepaint`; `TerminalView::on_left_mouse_down` reads
    /// it to convert `event.position` (window space) into cell coordinates.
    /// F-TERM-UI-02: without this, mouse clicks anywhere but the window's
    /// top-left corner map to the wrong row/column.
    last_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    /// The terminal grid's last-measured cell width, in the same window-space
    /// pixels `last_bounds` uses. `TerminalElement::prepaint` measures this
    /// from the resolved terminal family's (see
    /// `sirio_theme::terminal_family`) glyph advance every frame
    /// (`window.text_system().advance(..., 'm')`) and writes it here;
    /// `TerminalView::on_left_mouse_down` reads it back to convert a click's
    /// window-space x into a column the same way `prepaint` converted a
    /// column into an x when painting cell rects (`bounds.origin.x +
    /// cell_width * column`, line ~1380 below). Before this field existed,
    /// hit-testing used a hardcoded `8.0` guess instead of the real
    /// measurement `prepaint` already had on hand — a systematic,
    /// column-accumulating error whenever the real glyph advance at
    /// `FONT_SIZE` isn't exactly 8.0px (it generally is not), which grows
    /// with how far right on the line a link sits and reproduces exactly
    /// F-TERM-UI-02's "click on a real terminal URL never opens it" symptom
    /// even though the origin-subtraction half of the same hit-test was
    /// already correct and unit-tested.
    last_cell_width: Arc<Mutex<Option<Pixels>>>,
    /// The terminal grid's last-measured cell height. This follows the
    /// configured font size and keeps hit-testing and mouse reporting in step
    /// with the painted rows.
    last_cell_height: Arc<Mutex<Option<Pixels>>>,
    /// #301/#308 (R4.3): decoded Kitty images keyed `(image_id, generation)`,
    /// with the eviction bookkeeping (last-painted ticks, last placement
    /// walk, mutation stamp) beside them. Lives beside the other per-pane
    /// state — NOT in `TerminalPaintState`, which is rebuilt every prepaint.
    /// Release runs against it every frame (R4.5) and when the pane closes
    /// (its `Drop` parks the images in [`KITTY_DROPPED_IMAGES`]).
    kitty_images: Arc<Mutex<KittyImageCache>>,
    /// #308 R4.4: the owner thread's mutation stamp — bumped once per poll
    /// iteration in which it applied anything to the terminal that could
    /// change the Kitty placement walk or the cell grid (output batch, Feed,
    /// scroll, resize; content transmits and deletes ride the same bump via
    /// `vt_write`). `prepaint` gates both retained products on this stamp, so
    /// a still pane pays one integer comparison per frame.
    mutation_stamp: Arc<AtomicU64>,
    /// Retained cell snapshots and assembled draw products for this pane.
    /// This lives beside the other per-pane state — NOT in
    /// `TerminalPaintState`, which is rebuilt every prepaint.
    grid_render_cache: Arc<Mutex<GridRenderCache>>,
    /// Test-observable count of `Snapshot` builds the owner thread served
    /// for this pane. Per handle rather than process-wide so a test asserting
    /// "a still pane costs zero snapshots" cannot be tripped by the other
    /// tests' panes rendering in parallel in the same process.
    #[cfg_attr(not(test), allow(dead_code))] // read by the retained-grid view tests
    snapshot_builds: Arc<AtomicU64>,
    /// Test-observable count of scrollback extractions (`Text`) the owner
    /// thread served for this pane, read through
    /// [`TerminalHandle::scrollback_captures`].
    ///
    /// Per handle for exactly the reason `snapshot_builds` above is: this
    /// counter was a process-wide `static` until a nightly run caught it
    /// reading 28 where the test had left it at 26. Nothing was wrong with
    /// the pane under test -- a sibling test's pane had captured its own
    /// scrollback in the window between the two loads, in the same test
    /// binary. A global counter cannot answer "did *this* pane get asked",
    /// which is the only question the invariant is about.
    scrollback_captures: Arc<AtomicU64>,
    /// Test-observable count of grid assemblies `prepaint` rebuilt for this
    /// pane (the assembly-key miss path); per handle for the same reason.
    grid_assemblies: Arc<AtomicU64>,
    /// #303 R2.6: latched by the owner-thread PNG decoder so a failed ingest
    /// still produces a visible refusal indicator in the pane. PNG failures
    /// do not create a stored placement for `kitty_refused` to inspect.
    kitty_decode_failed: Arc<AtomicBool>,
    /// #43: refreshed once per owner-thread poll-loop iteration from
    /// `terminal.is_mouse_tracking()`. The view reads it to decide
    /// Sirio-gesture vs encode without a blocking round-trip into the !Send
    /// terminal state.
    mouse_tracking: Arc<AtomicBool>,
    /// #259: the resolved selection, in viewport grid coordinates.
    ///
    /// Written by the main-thread mouse handlers when a gesture changes it,
    /// read by `TerminalElement::prepaint` to tint background quads. Plain data by
    /// design -- see [`SelectedRange`] for why the emulator's own `Selection`
    /// cannot be what the paint loop consults.
    selection: Arc<Mutex<Option<SelectedRange>>>,
    /// The cell a drag started from, same coordinates. Kept beside the
    /// selection so a press that never becomes a drag can clear the highlight
    /// without inventing a zero-width range.
    selection_anchor: Arc<Mutex<Option<(usize, usize)>>>,
    /// F-TERM-03: the PTY master's raw fd number, captured once at spawn
    /// time before the master itself moves to the terminal owner thread (the
    /// fd *number* stays valid for as long as the master is open, so holding
    /// just the number — not the handle — is enough to query it without
    /// contending with that thread's reads/writes).
    /// `tcgetpgrp` on this fd is a read-only terminal-driver ioctl
    /// (`TIOCGPGRP`) and does not consume PTY data.
    ///
    /// Unix-only; `None` when portable-pty could not expose the fd, in which
    /// case [`Self::foreground_command_running`] reports `false`.
    #[cfg(unix)]
    pty_master_fd: Option<std::os::unix::io::RawFd>,
}

/// The shell a `TerminalShell::System` pane falls back to when `$SHELL` is unset,
/// together with the arguments that make it a login shell.
///
/// This is the last OS-specific assumption in the PTY spawn path, so it is gated
/// per platform instead of shared. The reference implementation hardcodes
/// `/bin/zsh` — macOS's default login shell. Inheriting that constant on Linux is
/// not merely an unusual choice: `/bin/zsh` does not exist on a stock box, so the
/// pane fails to spawn and the user gets no shell at all.
///
/// Each platform is named explicitly rather than folded into a single `cfg(unix)`
/// arm, for the reason recorded further down this file — `cfg(unix)` silently
/// hands macOS the Linux branch, which is how that earlier bug happened.
#[cfg(target_os = "macos")]
fn default_system_shell() -> (String, Vec<String>) {
    ("/bin/zsh".to_string(), vec!["-il".to_string()])
}

#[cfg(all(unix, not(target_os = "macos")))]
fn default_system_shell() -> (String, Vec<String>) {
    // POSIX guarantees `/bin/sh`, so it is the floor that can never fail to
    // exist; bash is preferred when present because it is what a login shell on
    // these systems normally is.
    let program = ["/bin/bash", "/bin/sh"]
        .into_iter()
        .find(|candidate| std::path::Path::new(candidate).exists())
        .unwrap_or("/bin/sh");
    (program.to_string(), vec!["-il".to_string()])
}

/// `$SHELL` and the `-il` login convention are POSIX notions, so Windows resolves
/// its own interpreter and passes no login arguments.
#[cfg(windows)]
fn default_system_shell() -> (String, Vec<String>) {
    let program = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
    (program, Vec::new())
}

/// Program and arguments a [`TerminalShell::System`] pane spawns.
///
/// The single source of truth. The spawn path and the UI breadcrumb both read
/// this, so what the user is told cannot drift from what is actually running —
/// the drift is what #230 was.
#[cfg(not(windows))]
fn system_pane_shell() -> (String, Vec<String>) {
    match std::env::var("SHELL") {
        Ok(value) if !value.is_empty() => (value, vec!["-il".to_string()]),
        _ => default_system_shell(),
    }
}

/// `$SHELL` and `-il` are POSIX notions. A value inherited from Git Bash is an
/// MSYS path (`/bin/bash.exe`) that `CreateProcessW` cannot resolve, so
/// honouring it made every pane fail to start with os error 3 (#230). Windows
/// resolves its own interpreter from `COMSPEC` instead.
#[cfg(windows)]
fn system_pane_shell() -> (String, Vec<String>) {
    default_system_shell()
}

/// The display name of the shell a System pane runs: the file stem of
/// [`system_pane_shell`]'s program, for UI that names the shell to the user.
pub fn system_shell_display_name() -> String {
    let (program, _) = system_pane_shell();
    std::path::Path::new(&program)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or(program)
}

/// Program and arguments that run one command string through the user's shell.
///
/// Callers that want to run a command — an agent's launch line, an install
/// command, a summarizer — must go through this rather than spelling a shell
/// and a flag themselves. Both halves are platform-specific: the fallback
/// program, and the flag that means "read the next argument as a command"
/// (`-lc` is POSIX, cmd.exe wants `/C`).
///
/// The macOS and Linux arms are deliberately shared here, unlike
/// [`default_system_shell`]: what differs between them is only the fallback
/// program, and that decision is delegated. Do not "fix" this into a
/// three-way split — the split already happened one call down.
#[cfg(not(windows))]
pub fn command_shell_invocation(command: &str) -> (String, Vec<String>) {
    (
        user_shell_program(),
        vec!["-lc".to_string(), command.to_string()],
    )
}

/// The shell this user runs: `$SHELL` when they set it — they meant it — and
/// the platform's own default only as a floor. Every site that needs to name
/// a shell asks here, so no call site spells a path of its own.
///
/// This used to be two `cfg` arms; the Windows/POSIX split moved one level
/// down into [`system_pane_shell`], whose program each arm already computed
/// exactly, so the function is now shared and ungated.
fn user_shell_program() -> String {
    system_pane_shell().0
}

/// See the POSIX arm. `$SHELL` is not a Windows notion, so the interpreter
/// comes from `COMSPEC` and the command flag is `/C`.
///
/// The command is passed UNQUOTED. An earlier version pre-wrapped it in an
/// outer pair of quotes for `cmd /C`'s quote-stripping rule; that was right
/// for alacritty's raw command line and wrong for portable-pty, whose
/// Windows `CommandBuilder` re-quotes each argument (`append_quoted`) — the
/// pre-wrapped form arrived at cmd as `\"echo …\"`, a quoted program name
/// instead of a command, and the pane failed silently (#39). CommandBuilder's
/// quoting alone satisfies the same `cmd /C` preserve-or-strip rule this
/// wrapper existed for, so the transport owns the quoting now. Consumers
/// must still pass the pieces through VERBATIM (`CommandBuilder::arg`).
#[cfg(windows)]
pub fn command_shell_invocation(command: &str) -> (String, Vec<String>) {
    let (program, _) = default_system_shell();
    // Unquoted: portable-pty's CommandBuilder quotes it (#39).
    (program, vec!["/C".to_string(), command.to_string()])
}

/// Inputs for [`spawn_terminal_thread`]. Everything here is `Send`; the
/// libghostty-vt objects are constructed inside the thread.
struct TerminalThreadInputs {
    cols: u16,
    rows: u16,
    command_rx: std::sync::mpsc::Receiver<TerminalCommand>,
    bytes_rx: std::sync::mpsc::Receiver<Vec<u8>>,
    event_tx: futures::channel::mpsc::UnboundedSender<TerminalEvent>,
    writer: Box<dyn std::io::Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    mouse_tracking: Arc<AtomicBool>,
    /// #308 R4.4: the mutation stamp the view gates its Kitty placement
    /// re-scan and grid render cache on; bumped in the poll loop whenever
    /// this thread mutated the terminal in any way that could change either
    /// product.
    mutation_stamp: Arc<AtomicU64>,
    /// Shared with the handle's `snapshot_builds`: bumped once per served
    /// `Snapshot` so tests can prove a still pane never asks for one.
    snapshot_builds: Arc<AtomicU64>,
    /// Shared with the handle's `scrollback_captures`: bumped once per served
    /// `Text` so tests can prove the render loop never asks this pane's owner
    /// thread for its scrollback.
    scrollback_captures: Arc<AtomicU64>,
    kitty_decode_failed: Arc<AtomicBool>,
}

/// #301: one visible Kitty placement, copied off the owner thread as plain
/// owned data (R2.3, same shape #259 used to keep `Selection` off the paint
/// path). The `!Send` borrows (`Graphics<'t>`, `Image<'t>`,
/// `PlacementIteration`) all die inside [`collect_kitty_placements`]; what
/// survives to `prepaint` is only numbers and a byte buffer.
#[derive(Debug)]
struct KittyPlacement {
    image_id: u32,
    /// The image's store-generation stamp. The render cache keys on
    /// `(image_id, generation)` (R4.1) so a retransmitted image is never
    /// mistaken for the same pixels.
    generation: u64,
    /// Pixel format of `data` as stored. Always one of the raw formats —
    /// never PNG, never compressed: ghostty inflates the transport zlib and
    /// PNG-decodes at ingest (`graphics_image.zig`'s `Image` doc states a
    /// stored image has `compression == .none` and is never `.png`).
    format: ImageFormat,
    /// Image dimensions in pixels; `data.len` must equal
    /// `width * height * bytes-per-pixel(format)` (R2.4's hard bound).
    width: u32,
    height: u32,
    /// The image's raw pixels (RGB, RGBA, Gray or GrayAlpha). Shared across
    /// placements of the same image: copied once per `(image_id, generation)`
    /// per frame, empty on later placements of the same image.
    data: Vec<u8>,
    /// Placement's top-left viewport grid cell — may be negative when
    /// scrolled partly off the top. R3.3 keeps the untruncated geometry and
    /// lets `Window::paint_image` derive the clipped atlas region.
    viewport_col: i32,
    viewport_row: i32,
    /// The placement's rendered pixel size (already aspect-corrected).
    pixel_width: u32,
    pixel_height: u32,
}

/// #302 R3.1: the three z-bands Kitty defines for placement rendering.
/// `All` means "no filter" — it is not a fourth bucket — so prepaint carries
/// exactly these three lists. Bucketing happens in [`collect_kitty_placements`]
/// through the iterator's own `set_layer` filter, one walk per band; the
/// emulator decides membership, the pane never re-classifies (R3.5).
#[derive(Debug, Default)]
struct KittyPlacementBuckets {
    below_bg: Vec<KittyPlacement>,
    below_text: Vec<KittyPlacement>,
    above_text: Vec<KittyPlacement>,
    /// #308: the LIVE placement set — every placement the grid still holds,
    /// visible or scrolled out, as `(image_id, generation)`. A placement
    /// scrolled into the scrollback is still pinned and can be scrolled back
    /// to, so it must NOT be treated as dead (R4.7); it leaves this set only
    /// when the guest deletes it or the emulator evicts it from its store.
    /// The render cache reconciles against this set, not the visible one.
    live: Vec<(u32, u64)>,
}

/// #301 R2.3: copies every visible, non-virtual Kitty placement plus its
/// image's plain pixel bytes out of the emulator — three z-band buckets via
/// the emulator's own `set_layer` filter (#302 R3.1). Runs on the owner
/// thread; nothing here decodes — the decode lands in `prepaint`
/// ([`decode_kitty_image`]), off the thread that drains the PTY.
fn collect_kitty_placements(terminal: &mut Terminal<'_, '_>) -> KittyPlacementBuckets {
    let Ok(graphics) = terminal.kitty_graphics() else {
        return KittyPlacementBuckets::default();
    };
    let Ok(mut iterator) = PlacementIterator::new() else {
        return KittyPlacementBuckets::default();
    };

    let mut buckets = KittyPlacementBuckets::default();
    // Shared across the three z-band walks: image bytes copy at most once
    // per (image_id, generation) per frame, no matter how many placements
    // of the image are visible (R2.3's shape, kept across layers by #302).
    let mut seen: Vec<(u32, u64)> = Vec::new();
    // R3.1: bucketing is the emulator's own `set_layer` filter — a fresh
    // iteration per band, never a re-classification by reading placement
    // fields (R3.5). `All` is "no filter", not a fourth bucket.
    for layer in [Layer::BelowBg, Layer::BelowText, Layer::AboveText] {
        let Ok(mut iteration) = iterator.update(&graphics) else {
            continue;
        };
        if iteration.set_layer(layer).is_err() {
            continue;
        }
        while let Some(placement) = iteration.next() {
            // #308: record liveness BEFORE the visibility filter — a
            // scrolled-into-scrollback placement is skipped by the copy-out
            // but is still pinned and showable, and the cache must know it
            // is alive so scroll-back stays a cache hit (R4.7). An id that
            // leaves this set was deleted by the guest or evicted from the
            // emulator's store: its texture is dead weight (E2).
            if let Ok(id) = placement.image_id()
                && let Some(image) = graphics.image(id)
                && let Ok(generation) = image.generation()
            {
                buckets.live.push((id, generation));
            }
            let Some(place) = copy_kitty_placement(placement, &graphics, terminal, &mut seen)
            else {
                continue;
            };
            match layer {
                Layer::BelowBg => buckets.below_bg.push(place),
                Layer::BelowText => buckets.below_text.push(place),
                Layer::AboveText => buckets.above_text.push(place),
                Layer::All => unreachable!("we never walk without a z-band filter"),
            }
        }
    }
    buckets
}

/// #301 R2.3: copies ONE visible placement out of the emulator as plain
/// owned data. The `!Send` borrows (`Graphics<'t>`, `Image<'t>`,
/// `PlacementIteration`) all die in [`collect_kitty_placements`]; what
/// survives is only numbers and a byte buffer. `seen` is the
/// (image_id, generation) set shared across the three z-band walks (#302):
/// the first walk to meet an image copies its bytes, later walks leave
/// `data` empty and the prepaint cache reuses the key instead (R4.1).
fn copy_kitty_placement(
    placement: &PlacementIteration<'_, '_>,
    graphics: &Graphics<'_>,
    terminal: &Terminal<'_, '_>,
    seen: &mut Vec<(u32, u64)>,
) -> Option<KittyPlacement> {
    let Ok(image_id) = placement.image_id() else {
        return None;
    };
    let image = graphics.image(image_id)?;
    let Ok(generation) = image.generation() else {
        return None;
    };
    // One geometry call per placement, not piecemeal field reads (R3.5).
    let Ok(info) = placement.placement_render_info(&image, terminal) else {
        return None;
    };
    // `viewport_visible` is false for placements fully off-screen AND for
    // virtual (Unicode-placeholder) placements — the placeholder protocol
    // is out of scope, so filtering on it covers both.
    if !info.viewport_visible {
        return None;
    }
    let Ok(format) = image.format() else {
        return None;
    };
    let data = if seen.contains(&(image_id, generation)) {
        Vec::new()
    } else {
        seen.push((image_id, generation));
        match image.data() {
            Ok(data) => data.to_vec(),
            Err(_) => Vec::new(),
        }
    };
    Some(KittyPlacement {
        image_id,
        generation,
        format,
        width: image.width().unwrap_or(0),
        height: image.height().unwrap_or(0),
        viewport_col: info.viewport_col,
        viewport_row: info.viewport_row,
        pixel_width: info.pixel_width,
        pixel_height: info.pixel_height,
        data,
    })
}

/// #302 R3.3: maps a placement's UNTRUNCATED viewport geometry to pane
/// pixels — the origin may be negative when the placement is scrolled
/// partially above the pane's top edge — and `Window::paint_image` derives
/// `visible_bounds` plus the atlas sub-rect from the pane bounds itself.
/// Never clamp here: clamping the origin and shrinking the size in the
/// attempt to "fix" the clip produces a squashed image, not a cropped one.
fn kitty_image_bounds(
    pane: Bounds<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    place: &KittyPlacement,
) -> Bounds<Pixels> {
    Bounds::new(
        point(
            pane.origin.x + cell_width * place.viewport_col as f32,
            pane.origin.y + line_height * place.viewport_row as f32,
        ),
        size(px(place.pixel_width as f32), px(place.pixel_height as f32)),
    )
}

/// #301 R2.5: converts one stored Kitty image into a BGRA `RenderImage`.
///
/// A stored image is always plain pixels (see [`KittyPlacement`]), so this
/// only sees raw RGB / RGBA / Gray / GrayAlpha. The R/B swap happens here,
/// before `RenderImage::new`, because Kitty's `f=24`/`f=32` are R-first while
/// gpui's `RenderImage` is BGRA.
///
/// Runs on the UI thread in `prepaint` — the PTY owner thread only copies the
/// plain bytes (R2.3). Returns `None` for a payload the pane refuses:
/// unexpected formats, or a length that cannot match the announced dimensions
/// (R2.4 — a stored image is never compressed, so the length check IS the
/// decompression ceiling is enforced by [`KittyPngDecoder`]). The caller
/// paints a visible placeholder for `None` (R2.6).
fn decode_kitty_image(
    format: ImageFormat,
    width: u32,
    height: u32,
    data: &[u8],
) -> Option<RenderImage> {
    let pixels = u64::from(width) * u64::from(height);
    let expected = match format {
        ImageFormat::Rgb => pixels * 3,
        ImageFormat::Rgba => pixels * 4,
        ImageFormat::Gray => pixels,
        ImageFormat::GrayAlpha => pixels * 2,
        // PNG never survives ingestion and the enum is #[non_exhaustive];
        // refuse anything else loudly rather than guess a layout.
        _ => return None,
    } as usize;
    if data.len() != expected {
        return None;
    }

    let mut bgra = Vec::with_capacity(pixels as usize * 4);
    match format {
        ImageFormat::Rgb => {
            for px in data.as_chunks::<3>().0 {
                bgra.extend_from_slice(&[px[2], px[1], px[0], 0xFF]);
            }
        }
        ImageFormat::Rgba => {
            for px in data.as_chunks::<4>().0 {
                bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
            }
        }
        ImageFormat::Gray => {
            for &g in data {
                bgra.extend_from_slice(&[g, g, g, 0xFF]);
            }
        }
        ImageFormat::GrayAlpha => {
            for px in data.as_chunks::<2>().0 {
                bgra.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
        }
        _ => return None,
    }
    // `Frame` is gpui's texture-frame type (image::Frame); the buffer it
    // wraps is the swap done above — `RgbaImage` is just bytes with a size.
    let frame = Frame::new(RgbaImage::from_raw(width, height, bgra)?);
    Some(RenderImage::new(vec![frame]))
}

/// #301: `DecodePng` the owner thread registers so PNG transmits (f=100 —
/// what omp emits — measured in the shipped pi-tui's `encodeKittyTransmit`)
/// are stored at all: ghostty refuses a PNG unless a decoder is registered
/// (`graphics_image.zig` `complete`/`decodePng`). R2.1 keeps libghostty's own
/// optional `png` feature off, so this uses the workspace `image` crate — the
/// tree still has exactly one PNG implementation.
struct KittyPngDecoder {
    failure_flag: Arc<AtomicBool>,
}

impl KittyPngDecoder {
    #[cfg(test)]
    fn new() -> Self {
        Self::with_failure_flag(Arc::new(AtomicBool::new(false)))
    }

    fn with_failure_flag(failure_flag: Arc<AtomicBool>) -> Self {
        Self { failure_flag }
    }
}

impl DecodePng for KittyPngDecoder {
    fn decode_png<'alloc>(
        &mut self,
        alloc: &'alloc Allocator<'_>,
        data: &[u8],
    ) -> Option<DecodedImage<'alloc>> {
        let mut reader =
            image::ImageReader::with_format(std::io::Cursor::new(data), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(KITTY_DECOMPRESSED_MAX_BYTES);
        reader.limits(limits);
        let image = match reader.decode() {
            Ok(image) => image.into_rgba8(),
            Err(_) => {
                self.failure_flag.store(true, Ordering::Release);
                return None;
            }
        };
        let (width, height) = image.dimensions();
        // The output buffer must be allocated with ghostty's allocator — the
        // emulator takes ownership and frees it through the same one.
        let Some(mut out) = Bytes::new_with_alloc(alloc, image.as_raw().len()).ok() else {
            self.failure_flag.store(true, Ordering::Release);
            return None;
        };
        out.copy_from_slice(image.as_raw());
        Some(DecodedImage {
            width,
            height,
            data: out,
        })
    }
}

/// #301 R1.3: states the pane's Kitty ingest ceilings explicitly instead of
/// inheriting the emulator's defaults. Applied by the owner thread at spawn
/// and by the headless test harness, so the tested values are the shipped
/// ones.
fn apply_kitty_ingest_limits(terminal: &mut Terminal<'_, '_>) {
    terminal
        .set_apc_max_bytes_kitty(Some(KITTY_APC_MAX_BYTES))
        .expect("set the Kitty APC buffer ceiling");
    terminal
        .set_kitty_image_storage_limit(KITTY_IMAGE_STORAGE_LIMIT)
        .expect("set the Kitty image storage limit");
}

/// #308 (R4.3/R4.7): the decoded-image render cache for one pane, living
/// beside the other per-pane state on [`TerminalHandle`] — NOT in
/// `TerminalPaintState`, which is rebuilt every prepaint. Keys are
/// `(image_id, generation)` (R4.1, never `image_id` alone: guests reuse ids
/// and a replaced image must not show stale pixels).
///
/// Release is the point of this ticket:
/// - **R4.5-E1** a replaced image (new generation for a live id) drops the
///   old key immediately;
/// - **R4.5-E2** an id that leaves the grid (guest delete, store eviction) is
///   dead weight — dropped;
/// - **R4.5-E3** scrollback trims do NOT kill placements (measured), so a
///   scrolled-out image stays alive here until the atlas cap forces it out
///   (R4.6) least-recently-**painted** first (R4.7 — nothing on screen is
///   ever dropped, and the image the user last looked at survives a burst);
/// - **R4.5-E4** pane close: [`Drop`] parks every remaining image in the
///   graveyard [`KITTY_DROPPED_IMAGES`], drained into `Window::drop_image`
///   by the next paint.
#[derive(Default)]
struct KittyImageCache {
    images: HashMap<(u32, u64), Arc<RenderImage>>,
    /// R4.7: last frame tick in which each key was handed to the paint
    /// loop. Eviction orders by this value — least-recently-painted, never
    /// least-recently-added.
    painted: HashMap<(u32, u64), u64>,
    /// The cache's own monotonic frame counter; per-frame tick for `painted`.
    tick: u64,
    /// R4.4: the last owner-thread mutation stamp this cache was synced to.
    /// A still pane (stamp unchanged) skips the owner-thread round trip and
    /// reuses `last_buckets` — one integer comparison per frame, not a
    /// placement walk.
    last_stamp: u64,
    /// The most recent placement walk from the owner thread, kept so a
    /// steady frame can re-render without re-walking.
    last_buckets: KittyPlacementBuckets,
}

/// #308 R4.5-E4: every image a closed pane still held. `Window::drop_image`
/// needs a live `Window`, which a `Drop` impl never has, so the cache parks
/// its images here and `TerminalElement::paint` drains them into the window's
/// atlas each frame. Sirio is single-window, so the atlas that received the
/// uploads is the one that drops them; a multi-window future would need
/// per-window routing. A pane closed as the last thing before the window
/// closes leaks nothing extra: the window's atlas dies with the window.
static KITTY_DROPPED_IMAGES: Mutex<Vec<Arc<RenderImage>>> = Mutex::new(Vec::new());

impl KittyImageCache {
    /// Advances the frame counter and returns this frame's tick. Every
    /// `get_or_decode` in the frame records the tick, so the eviction pass
    /// can tell exactly which images were painted this frame (R4.7).
    fn begin_frame(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }

    /// #301 R4.1/R4.2 + #308: the render-cache lookup for one placement. A
    /// hit reuses the cached `RenderImage` (no re-decode, no rebuild — a
    /// rebuilt image is a new atlas key); a miss decodes and inserts. Either
    /// way the key is marked painted this tick. Returns `None` for a payload
    /// the pane refuses (R2.4) — the caller paints a visible placeholder.
    fn get_or_decode(&mut self, place: &KittyPlacement, tick: u64) -> Option<Arc<RenderImage>> {
        let key = (place.image_id, place.generation);
        if let Some(cached) = self.images.get(&key) {
            self.painted.insert(key, tick);
            return Some(cached.clone());
        }
        let decoded = decode_kitty_image(place.format, place.width, place.height, &place.data)?;
        let decoded = Arc::new(decoded);
        self.images.insert(key, decoded.clone());
        self.painted.insert(key, tick);
        Some(decoded)
    }

    /// #308 R4.5 (E1/E2): keys this frame's LIVE placement set says will
    /// never be shown again:
    ///
    /// - the cached generation of an id that is live with a NEWER generation
    ///   is superseded pixels — the guest retransmitted the id (E1);
    /// - an id that is not live at all was deleted by the guest or evicted
    ///   from the emulator's store (E2). `a=d` (placements only) and `a=D`
    ///   (images too) both land here, as does the store's own storage-limit
    ///   eviction.
    ///
    /// A live id with the same generation is kept even when scrolled out of
    /// view: it can still be scrolled back to, and dropping it would make
    /// that scroll-back re-decode (R4.7).
    fn dead_keys(&self, live: &[(u32, u64)]) -> Vec<(u32, u64)> {
        let mut live_generation: HashMap<u32, u64> = HashMap::new();
        for &(id, generation) in live {
            live_generation.insert(id, generation);
        }
        let mut dead = Vec::new();
        for &(id, generation) in self.images.keys() {
            match live_generation.get(&id) {
                Some(&live_generation) if live_generation != generation => {
                    dead.push((id, generation))
                }
                None => dead.push((id, generation)),
                Some(_) => {}
            }
        }
        dead
    }

    /// #308 R4.6/R4.7: when the cache holds more than `cap` images, the
    /// overflow is evicted least-recently-**painted** first. Keys painted
    /// this frame (on screen right now) are never candidates — an image the
    /// user is looking at outlives a burst that scrolled past it, and
    /// nothing on screen disappears silently (R2.6/#303). This is also the
    /// scrollback-trim release (E3): trims do not remove placements, so a
    /// trimmed-away image only leaves the cache here, as the LRU victim once
    /// newer images crowd it out.
    /// ponytail: when MORE than `cap` images are visible at once (cache full
    /// of this-frame paints), no key is evictable and the cache sits at the
    /// visible count; the 200-image cap makes that state unreachable in
    /// practice. A byte-budgeted variant could evict painted keys instead,
    /// at the cost of next-frame re-uploads.
    fn lru_dead_keys(&self, cap: usize, tick: u64) -> Vec<(u32, u64)> {
        let over = self.images.len().saturating_sub(cap);
        if over == 0 {
            return Vec::new();
        }
        let mut candidates: Vec<(u64, (u32, u64))> = self
            .images
            .keys()
            .filter(|key| self.painted.get(key).copied().unwrap_or(0) != tick)
            .map(|key| (self.painted.get(key).copied().unwrap_or(0), *key))
            .collect();
        candidates.sort_unstable();
        candidates.truncate(over);
        candidates.into_iter().map(|(_, key)| key).collect()
    }

    /// Removes a key from the cache, returning its image so the caller can
    /// hand it to `Window::drop_image`. `None` when the key is already gone.
    fn remove(&mut self, key: &(u32, u64)) -> Option<Arc<RenderImage>> {
        self.painted.remove(key);
        self.images.remove(key)
    }
}

impl Drop for KittyImageCache {
    fn drop(&mut self) {
        // R4.5-E4: when the last per-pane state dies (pane closed), every
        // image it still held is parked for the next paint to release from
        // the window's atlas. Dropping the Arc here without `drop_image`
        // would leave the GPU texture alive for the process lifetime.
        if self.images.is_empty() {
            return;
        }
        KITTY_DROPPED_IMAGES
            .lock()
            .extend(self.images.drain().map(|(_, image)| image));
    }
}

/// Runs a terminal's dedicated owner thread.
///
/// libghostty-vt's `Terminal` (and its render helpers) are all `!Send`, so
/// unlike alacritty's shared `FairMutex<Term>` there is no cross-thread
/// terminal object: this thread owns the emulator, render state, iterators,
/// PTY writer, PTY master, and child process, and serves every other caller
/// through the command channel.
///
/// The loop POLLS rather than blocking on any channel — deliberately, not by
/// inheritance from the alacritty path: a channel waker would fire on the
/// reader thread and break GPUI's deterministic test scheduler (the same
/// reason recorded at the pump below). Worst-case latency for output, replies,
/// commands, title changes and child exits is one [`EVENT_POLL_INTERVAL`] tick.
fn spawn_terminal_thread(inputs: TerminalThreadInputs) {
    std::thread::spawn(move || {
        let TerminalThreadInputs {
            cols,
            rows,
            command_rx,
            bytes_rx,
            event_tx,
            mut writer,
            master,
            mut child,
            mouse_tracking,
            mutation_stamp,
            snapshot_builds,
            scrollback_captures,
            kitty_decode_failed,
        } = inputs;

        // libghostty-vt never writes to the pty itself; it hands the
        // embedder its replies (DSR/DECRQM answers etc.) through this
        // callback. ConPTY opens with ESC[6n and blocks until answered, so
        // dropping these starves the child at its first byte of output —
        // measured in #33 as exactly 4 bytes then silence. The channel
        // exists because `writer` is owned by this thread while the callback
        // borrows the terminal during parsing; replies are drained to the
        // pty right after each parse batch, on THIS thread.
        let (reply_tx, reply_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let mut terminal = Terminal::new(TerminalOptions {
            cols,
            rows,
            max_scrollback: 10_000,
        })
        .expect("terminal owner thread: constructing the VT state");
        // Grapheme clustering (DEC 2027) defaults off upstream; turn it on so
        // ZWJ emoji and flags occupy one cell. herdr needs a vendored patch for
        // this; here the embedder can simply set it. Known gap (#27): a guest
        // ESC c (RIS) resets it back to off.
        terminal
            .set_mode(Mode::GRAPHEME_CLUSTER, true)
            .expect("terminal owner thread: enabling DEC 2027 grapheme clustering");
        terminal
            .on_pty_write(move |_term, data: &[u8]| {
                let _ = reply_tx.send(data.to_vec());
            })
            .expect("terminal owner thread: registering the pty-write callback");

        // #301: state the Kitty ingest ceilings explicitly (R1.3) and register
        // the pane's own PNG decoder — f=100 transmits (what omp emits) are
        // refused by ghostty without one, and R2.1 keeps the crate's own `png`
        // feature off, so the decoder is the workspace `image` crate.
        apply_kitty_ingest_limits(&mut terminal);
        libghostty_vt::kitty::graphics::set_png_decoder(Some(Box::new(
            KittyPngDecoder::with_failure_flag(kitty_decode_failed),
        )))
        .expect("terminal owner thread: registering the Kitty PNG decoder");

        // Allocated once and reused every frame; they live and die on this
        // thread like everything else libghostty-vt owns.
        let mut render = RenderState::new().expect("terminal owner thread: RenderState");
        let mut row_iterator = RowIterator::new().expect("terminal owner thread: RowIterator");
        let mut cell_iterator = CellIterator::new().expect("terminal owner thread: CellIterator");
        let mut key_encoder = key::Encoder::new().expect("terminal owner thread: key encoder");
        let mut mouse_encoder =
            MouseEncoderState::new().expect("terminal owner thread: mouse encoder");

        let mut last_title = String::new();
        let mut child_exit_reported = false;
        // A command that arrived while this thread was parked at the bottom of
        // the loop, carried forward to be served at the top of the next
        // iteration rather than handled out of order down there.
        let mut carried: Option<TerminalCommand> = None;
        loop {
            // 1. Drain whatever the PTY produced into the parser.
            let mut had_output = false;
            while let Ok(chunk) = bytes_rx.try_recv() {
                terminal.vt_write(&chunk);
                had_output = true;
            }
            // vt_write fires on_pty_write synchronously, so the replies owed
            // to the host are flushed after parsing, not before.
            while let Ok(reply) = reply_rx.try_recv() {
                let _ = writer.write_all(&reply);
                let _ = writer.flush();
            }
            if had_output {
                let _ = event_tx.unbounded_send(TerminalEvent::Wakeup);
            }

            // 2. Serve queued commands.
            let mut mutated = false;
            let mut shutdown = false;
            while let Some(command) = carried.take().or_else(|| command_rx.try_recv().ok()) {
                match command {
                    TerminalCommand::Input(bytes) => {
                        let _ = writer.write_all(&bytes);
                        let _ = writer.flush();
                    }
                    TerminalCommand::Paste(bytes) => {
                        if terminal.mode(Mode::BRACKETED_PASTE).unwrap_or(false) {
                            let mut bracketed = Vec::with_capacity(bytes.len() + 12);
                            bracketed.extend_from_slice(b"\x1b[200~");
                            bracketed.extend_from_slice(&bytes);
                            bracketed.extend_from_slice(b"\x1b[201~");
                            let _ = writer.write_all(&bracketed);
                        } else {
                            let _ = writer.write_all(&bytes);
                        }
                        let _ = writer.flush();
                    }
                    TerminalCommand::Key(input) => {
                        if let Ok(bytes) = encode_key_input(&terminal, &mut key_encoder, input) {
                            let _ = writer.write_all(&bytes);
                            let _ = writer.flush();
                        }
                    }
                    TerminalCommand::Mouse(input) => {
                        if let Ok(bytes) = encode_mouse_input(&terminal, &mut mouse_encoder, input)
                        {
                            let _ = writer.write_all(&bytes);
                            let _ = writer.flush();
                        }
                    }
                    TerminalCommand::Feed(bytes) => {
                        terminal.vt_write(&bytes);
                        mutated = true;
                    }
                    TerminalCommand::Resize(columns, lines, cell_width, cell_height) => {
                        let _ = master.resize(PtySize {
                            rows: lines,
                            cols: columns,
                            pixel_width: 0,
                            pixel_height: 0,
                        });
                        let _ = terminal.resize(
                            columns,
                            lines,
                            u32::from(cell_width),
                            u32::from(cell_height),
                        );
                        mutated = true;
                    }
                    TerminalCommand::Scroll(scroll) => {
                        let page = terminal.rows().unwrap_or(rows) as isize;
                        let viewport = match scroll {
                            SirioScroll::Top => ScrollViewport::Top,
                            SirioScroll::Bottom => ScrollViewport::Bottom,
                            SirioScroll::PageUp => ScrollViewport::Delta(-page),
                            SirioScroll::PageDown => ScrollViewport::Delta(page),
                            SirioScroll::Lines(delta) => ScrollViewport::Delta(delta),
                        };
                        terminal.scroll_viewport(viewport);
                        mutated = true;
                        let _ = event_tx.unbounded_send(TerminalEvent::ViewportChanged);
                    }
                    TerminalCommand::Snapshot(reply) => {
                        snapshot_builds.fetch_add(1, Ordering::SeqCst);
                        let frame: SnapshotReply = build_snapshot(
                            &mut terminal,
                            &mut render,
                            &mut row_iterator,
                            &mut cell_iterator,
                        );
                        let _ = reply.send(frame);
                    }
                    TerminalCommand::KittyPlaces(reply) => {
                        let _ = reply.send(collect_kitty_placements(&mut terminal));
                    }
                    TerminalCommand::Text(reply) => {
                        scrollback_captures.fetch_add(1, Ordering::SeqCst);
                        let _ = reply.send(capture_scrollback_text(&mut terminal));
                    }
                    TerminalCommand::ClickSelect(cell, kind, reply) => {
                        let _ = reply.send(click_selection_range(&terminal, cell, kind));
                    }
                    TerminalCommand::SelectionText(range, reply) => {
                        // Built and consumed inside this one iteration:
                        // `Selection` borrows the terminal, so it can never be
                        // held across the loop. The plain range on the handle
                        // is what survives between frames.
                        let text = selection_text(&terminal, range).unwrap_or_default();
                        let _ = reply.send(text);
                    }
                    TerminalCommand::Shutdown => {
                        shutdown = true;
                        break;
                    }
                }
            }
            // #308 R4.4: any mutation that can move placements or change
            // their visibility bumps the stamp the view gates its re-scan
            // and grid render cache on. Content changes
            // (transmits/replaces/placements/deletes) arrive through
            // `vt_write`, so the output batch and `Feed` cover them;
            // Scroll/Resize move placement pins and change which cells are
            // visible. A still pane leaves the stamp untouched and the view
            // pays one integer comparison per frame instead of a placement
            // walk or snapshot.
            if had_output {
                mutated = true;
            }
            if mutated {
                mutation_stamp.fetch_add(1, Ordering::Relaxed);
            }
            if shutdown {
                let _ = child.kill();
                // Dropping the master closes the pty, which unblocks the
                // reader thread's read() so it can end too.
                drop(master);
                drop(writer);
                return;
            }

            // 3. Diff observable terminal state into events.
            if let Ok(title) = terminal.title()
                && title != last_title
            {
                last_title = title.to_string();
                let _ = event_tx.unbounded_send(TerminalEvent::Title(title.to_string()));
            }
            if !child_exit_reported && let Ok(Some(status)) = child.try_wait() {
                child_exit_reported = true;
                // portable-pty reports a signalled exit as a signal
                // *name* string, not a number, so a signalled child
                // degrades to its (nonzero) code rather than Signal(n).
                let status = if status.success() {
                    TerminalExitStatus::Success
                } else {
                    TerminalExitStatus::from_exit_code(status.exit_code() as i32)
                };
                let _ = event_tx.unbounded_send(TerminalEvent::ChildExit(status));
            }

            // #43: refresh the shared tracking gate once per poll-loop
            // iteration so the view can decide Sirio-gesture vs encode from a
            // plain atomic instead of blocking round-trips into the !Send
            // terminal state.
            let tracking = terminal.is_mouse_tracking().unwrap_or(false);
            mouse_tracking.store(tracking, Ordering::Relaxed);

            // 4. Park until a command arrives or the poll tick elapses.
            //
            // This used to be a flat `sleep(EVENT_POLL_INTERVAL)`, which made
            // every blocking round-trip -- especially the `Snapshot` that
            // the renderer needed before the retained-grid cache -- wait for
            // this thread to finish a nap it had only just started. Measured
            // at 5.9ms per frame for a 200x50 pane, of which 5.3ms was this
            // wait and 0.6ms was the grid rebuild: the main thread stalled
            // for most of a frame, per visible pane, doing nothing.
            //
            // Blocking on the channel instead wakes this thread the instant a
            // command lands, while the timeout preserves the tick that drives
            // the PTY drain and the title/exit diffing above.
            match command_rx.recv_timeout(EVENT_POLL_INTERVAL) {
                Ok(command) => carried = Some(command),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                // Every handle is gone; nothing will ever command this
                // terminal again.
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}

/// Builds one paint-loop frame: visible rows of cells plus the cursor
/// position `(row, column)`, or `(usize::MAX, 0)` when scrolled back (the
/// cursor is not in the viewport). Stage 2 (#45): consumes grapheme clusters
/// and the full SGR attribute set, resolving inverse/invisible here so the
/// renderer sees final values.
fn build_snapshot(
    terminal: &mut Terminal<'static, 'static>,
    render: &mut RenderState<'static>,
    row_iterator: &mut RowIterator<'static>,
    cell_iterator: &mut CellIterator<'static>,
) -> (Vec<Vec<SnapshotCell>>, (usize, usize)) {
    let default_fg = SirioColor::Named(NamedColor::Foreground);
    let default_bg = SirioColor::Named(NamedColor::Background);
    let snapshot = match render
        .begin_update(terminal)
        .and_then(|update| update.end())
    {
        Ok(snapshot) => snapshot,
        Err(_) => return (Vec::new(), (usize::MAX, 0)),
    };
    // Palette lookup for SGR 58 palette-indexed underline colors.
    let colors = snapshot.colors().ok();

    let mut rows_out = Vec::new();
    let Ok(mut rows) = row_iterator.update(&snapshot) else {
        return (Vec::new(), (usize::MAX, 0));
    };
    while rows.next().is_some() {
        let mut line = Vec::new();
        let Ok(mut cells) = cell_iterator.update(&rows) else {
            break;
        };
        while cells.next().is_some() {
            let style = cells.style().unwrap_or_default();
            let mut fg = cells
                .fg_color()
                .ok()
                .flatten()
                .map(|rgb| SirioColor::Rgb(rgb.r, rgb.g, rgb.b))
                .unwrap_or(default_fg);
            let mut bg = cells
                .bg_color()
                .ok()
                .flatten()
                .map(|rgb| SirioColor::Rgb(rgb.r, rgb.g, rgb.b))
                .unwrap_or(default_bg);
            if style.inverse {
                std::mem::swap(&mut fg, &mut bg);
            }
            let (cluster, cluster_len) = cell_cluster(&cells);
            line.push(SnapshotCell {
                cluster,
                // invisible: keep the column, drop the glyph — the empty
                // cluster renders as a space via SnapshotCell::chars.
                cluster_len: if style.invisible { 0 } else { cluster_len },
                bold: style.bold,
                italic: style.italic,
                underline: style.underline,
                underline_color: resolve_style_color(style.underline_color, colors.as_ref()),
                strikethrough: style.strikethrough,
                faint: style.faint,
                fg,
                bg,
                hyperlink: None,
            });
        }
        rows_out.push(line);
    }

    // OSC 8 hyperlinks (#41): the render-state cell iterator exposes no
    // hyperlink accessor, so read them straight off the grid — only possible
    // here on the owner thread where the GridRef exists.
    let mut uri_buf: Vec<u8> = Vec::with_capacity(256);
    let mut interned: Vec<Arc<str>> = Vec::new();
    for (row_index, line) in rows_out.iter_mut().enumerate() {
        // Row-level has_hyperlink is a cheap prefilter with FALSE POSITIVES
        // allowed — it may claim links that no cell of the row carries, but
        // never misses a row that does. Skip whole rows; per-cell truth is
        // Cell::has_hyperlink below.
        let row_may_have_links = terminal
            .grid_ref(viewport_point(0, row_index as u32))
            .ok()
            .and_then(|grid_ref| grid_ref.row().ok())
            .and_then(|row| row.has_hyperlink().ok())
            .unwrap_or(false);
        if !row_may_have_links {
            continue;
        }
        for (column, cell) in line.iter_mut().enumerate() {
            let uri = terminal
                .grid_ref(viewport_point(column as u16, row_index as u32))
                .ok()
                .and_then(|grid_ref| {
                    grid_ref.cell().ok().and_then(|cell_ref| {
                        cell_ref
                            .has_hyperlink()
                            .map(|has| if has { Some(grid_ref) } else { None })
                            .unwrap_or(None)
                    })
                })
                .and_then(|grid_ref| grid_ref_hyperlink_uri(grid_ref, &mut uri_buf));
            let Some(uri) = uri else {
                continue;
            };
            // Intern per snapshot: one Arc<str> per distinct URI per frame;
            // link runs are short and URIs per frame are few, so linear scan.
            let shared = match interned.iter().find(|existing| existing.as_ref() == uri) {
                Some(existing) => existing.clone(),
                None => {
                    let arc: Arc<str> = Arc::from(uri.as_str());
                    interned.push(arc.clone());
                    arc
                }
            };
            cell.hyperlink = Some(shared);
        }
    }

    // Hide the cursor when the viewport is scrolled away from the active
    // area — scrollbar offset equals total-len only at the bottom.
    let at_bottom = terminal
        .scrollbar()
        .map(|scrollbar| scrollbar.offset >= scrollbar.total.saturating_sub(scrollbar.len))
        .unwrap_or(true);
    let cursor = if at_bottom {
        (
            usize::from(terminal.cursor_y().unwrap_or(u16::MAX)),
            usize::from(terminal.cursor_x().unwrap_or(0)),
        )
    } else {
        (usize::MAX, 0)
    };
    (rows_out, cursor)
}

/// Reads one cell's grapheme cluster into the inline buffer. Clusters longer
/// than GRAPHEME_INLINE are truncated to their first codepoints.
/// ponytail: truncation ceiling; store oversized clusters in a side table if
/// real content ever hits it.
fn cell_cluster(cells: &CellIteration<'_, '_>) -> ([char; GRAPHEME_INLINE], usize) {
    const NONE: [char; GRAPHEME_INLINE] = ['\0'; GRAPHEME_INLINE];
    let len = match cells.graphemes_len() {
        Ok(len) => len.min(GRAPHEME_INLINE),
        Err(_) => return (NONE, 0),
    };
    if len == 0 {
        return (NONE, 0);
    }
    let mut cluster = NONE;
    if cells.graphemes_buf(&mut cluster[..len]).is_err() {
        return (NONE, 0);
    }
    (cluster, len)
}

/// Resolves an SGR color slot for the snapshot: None means "unset, paint with
/// the default", palette entries are looked up in the render state's active
/// palette.
fn resolve_style_color(color: StyleColor, colors: Option<&Colors>) -> Option<SirioColor> {
    match color {
        StyleColor::None => None,
        StyleColor::Rgb(rgb) => Some(SirioColor::Rgb(rgb.r, rgb.g, rgb.b)),
        StyleColor::Palette(index) => colors
            .map(|colors| colors.palette[index.0 as usize])
            .map(|rgb| SirioColor::Rgb(rgb.r, rgb.g, rgb.b)),
    }
}

/// The working directory handed to a spawned PTY child. Strips the Windows
/// verbatim prefix on Windows: `\\?\C:\a\b` becomes `C:\a\b` and
/// `\\?\UNC\server\share\x` becomes `\\server\share\x`; on every other
/// platform (and for any non-verbatim path) the input passes through
/// untouched.
///
/// This is a **deliberate third copy** of an idiom already carried twice in
/// this repo: `sirio_git::path_arg`/`sirio_git::strip_verbatim_prefix`
/// (`crates/sirio_git/src/git.rs`) strip the prefix for paths leaving as
/// *git arguments*, and `sirio_project::display_absolute_path`
/// (`crates/sirio_project/src/domain.rs`) strips it for *display*. No new
/// dependency is taken and neither existing copy is made public — the copies
/// live independently on purpose. This one exists because a path handed to
/// a **child process** as its working directory must not carry the prefix:
/// `CreateProcess` accepts it, but `cmd.exe` then refuses to keep a `\\?\`
/// directory as its cwd (it reads it as UNC) and silently falls back to the
/// Windows directory — so every command in the terminal runs against the
/// wrong directory (#150).
///
/// [`validate_working_directory`] deliberately keeps the original verbatim
/// path: `std::fs::metadata` handles it fine and keeps its long-path
/// capability. Only the child's cwd is stripped.
fn spawn_cwd(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let string = path.to_string_lossy();
        let rest = string.strip_prefix(r"\\?\");
        if let Some(rest) = rest {
            if let Some(unc) = rest.strip_prefix("UNC\\") {
                return PathBuf::from(format!(r"\\{unc}"));
            }
            return PathBuf::from(rest.to_string());
        }
        path.to_path_buf()
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
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
    ) -> Result<(Self, UnboundedReceiver<TerminalEvent>)> {
        Self::new_with_pane_id(working_directory, shell, None)
    }

    fn new_with_pane_id(
        working_directory: impl AsRef<Path>,
        shell: &TerminalShell,
        pane_id: Option<&str>,
    ) -> Result<(Self, UnboundedReceiver<TerminalEvent>)> {
        // portable-pty swallows a failed chdir in the spawned child (the
        // shell would silently start in the app's cwd, i.e. the wrong
        // project). Validate the directory here so a stale path surfaces as
        // a retryable pane error instead.
        let working_directory = working_directory.as_ref();
        Self::validate_working_directory(working_directory)?;

        const COLS: u16 = 80;
        const ROWS: u16 = 24;

        let (wakeup_tx, wakeup_rx) = futures::channel::mpsc::unbounded();
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: ROWS,
                cols: COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("creating terminal PTY")?;

        let (program, args) = match shell {
            TerminalShell::System => system_pane_shell(),
            TerminalShell::WithArguments { program, args } => (program.clone(), args.clone()),
        };
        let mut command = CommandBuilder::new(program);
        for argument in &args {
            command.arg(argument);
        }
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        // #301 R1.4 (omp half): omp's pi-tui decides its image protocol from
        // PI_FORCE_IMAGE_PROTOCOL before it sends anything — the Kitty support
        // query is answered (R1.1, pinned by test #264) but omp never emits a
        // graphics byte unless this is set (its `detectCapabilities` reads
        // only the environment; measured in #264). The spec calls this one
        // line the milestone (R1.4). It is deliberately NOT a terminal-identity
        // claim: #86 settled Sirio's identity as ai.sirio.Sirio, and this
        // variable only pins the image protocol choice. omp is its only reader
        // in practice — pi's own shipped tree never references it and plain
        // shells ignore it — so exporting it on every pane is the smallest
        // honest surface; scoping it to omp panes would require TerminalShell
        // to carry env, which the next ticket can add if that ever matters.
        command.env("PI_FORCE_IMAGE_PROTOCOL", "kitty");
        if let Some(pane_id) = pane_id {
            command.env("SIRIO_PANE_ID", pane_id);
            // Scripts and agent skills written before the rebrand read the
            // pane id under its old name. Exporting both costs one variable
            // and keeps them working inside a pane they cannot see renamed.
            command.env("TILLER_PANE_ID", pane_id);
        }
        command.cwd(spawn_cwd(working_directory));

        let child = pair
            .slave
            .spawn_command(command)
            .context("spawning PTY child")?;
        drop(pair.slave);
        let shell_pid = child.process_id().unwrap_or(0);

        // Captured before `master` moves to the owner thread below — see the
        // field doc on `pty_master_fd` for why the bare fd number outlives
        // that move.
        #[cfg(unix)]
        let pty_master_fd = pair.master.as_raw_fd();

        let mut reader = pair
            .master
            .try_clone_reader()
            .context("cloning PTY reader")?;
        let writer = pair.master.take_writer().context("taking PTY writer")?;

        // The reader thread only ever sends owned bytes; the emulator itself
        // is never touched off its owner thread (libghostty-vt is !Send).
        let (bytes_tx, bytes_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if bytes_tx.send(buffer[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        let (commands, command_rx) = std::sync::mpsc::channel::<TerminalCommand>();
        let scrollback = ScrollbackCapture::new(commands.clone());
        // Shared before the thread spawns and handed to both the owner loop
        // (writer) and the handle (reader) below.
        let mouse_tracking_flag = Arc::new(AtomicBool::new(false));
        let mutation_stamp = Arc::new(AtomicU64::new(0));
        let snapshot_builds = Arc::new(AtomicU64::new(0));
        let scrollback_captures = Arc::new(AtomicU64::new(0));
        let kitty_decode_failed = Arc::new(AtomicBool::new(false));
        spawn_terminal_thread(TerminalThreadInputs {
            cols: COLS,
            rows: ROWS,
            command_rx,
            bytes_rx,
            event_tx: wakeup_tx,
            writer,
            master: pair.master,
            child,
            mouse_tracking: mouse_tracking_flag.clone(),
            mutation_stamp: mutation_stamp.clone(),
            snapshot_builds: snapshot_builds.clone(),
            scrollback_captures: scrollback_captures.clone(),
            kitty_decode_failed: kitty_decode_failed.clone(),
        });

        Ok((
            Self {
                commands,
                scrollback,
                last_size: Arc::new(Mutex::new(None)),
                shell_pid,
                shutdown_started: Arc::new(AtomicBool::new(false)),
                resize_generation: Arc::new(AtomicU64::new(0)),
                last_bounds: Arc::new(Mutex::new(None)),
                last_cell_width: Arc::new(Mutex::new(None)),
                last_cell_height: Arc::new(Mutex::new(None)),
                kitty_images: Arc::new(Mutex::new(KittyImageCache::default())),
                mutation_stamp,
                grid_render_cache: Arc::new(Mutex::new(GridRenderCache::default())),
                snapshot_builds,
                scrollback_captures,
                grid_assemblies: Arc::new(AtomicU64::new(0)),
                kitty_decode_failed,
                mouse_tracking: mouse_tracking_flag,
                selection: Arc::new(Mutex::new(None)),
                selection_anchor: Arc::new(Mutex::new(None)),
                #[cfg(unix)]
                pty_master_fd,
            },
            wakeup_rx,
        ))
    }

    /// Whether a foreground command other than the shell itself currently
    /// owns the terminal — i.e. whether the pane should show "Running".
    ///
    /// A terminal pane's shell process is always alive from spawn to
    /// teardown, so "a child process exists" cannot distinguish "idle at the
    /// prompt" from "running a command": both have a live shell. The signal
    /// that actually distinguishes them is the PTY's **foreground process
    /// group** (`tcgetpgrp` on the master fd, `TIOCGPGRP` under the hood):
    /// alacritty's PTY setup calls `setsid()` in the child's `pre_exec`
    /// (`tty/unix.rs`), so the shell starts as its own session and process
    /// group leader — `getpgid(shell_pid) == shell_pid`. When an interactive
    /// shell with job control runs a foreground command, it puts that
    /// command in a *new* process group and hands the PTY's foreground
    /// group to it for the duration; the shell reclaims it when the command
    /// exits. So `tcgetpgrp(master_fd) != shell_pid` is true exactly while a
    /// foreground command is running, and false at an idle prompt — this is
    /// the same technique terminal multiplexers use to report pane activity.
    ///
    /// Rejected alternatives:
    /// - **A live child process exists** (`descendant_pids`/libproc): true
    ///   the entire time the shell itself is alive, so it can never report
    ///   "idle" — the exact bug this method exists to fix.
    /// - **The tab's agent-activity dot** (`AgentCatalog`/Layer A-D in
    ///   CLAUDE.md): identity-gated to the five supported agent CLIs. A
    ///   prior pass already confirmed a bare `sleep 20` never moves it, so
    ///   it is silent for exactly the case this pill needs to cover.
    ///
    /// Unix-only: `tcgetpgrp`/process groups are a POSIX job-control notion
    /// with no Windows equivalent, so this always reports `false` there —
    /// the pill simply never shows "Running" on that platform, a known gap
    /// rather than a silent wrong answer.
    fn foreground_command_running(&self) -> bool {
        #[cfg(unix)]
        {
            // SAFETY: `pty_master_fd` is a plain fd number captured while
            // the master itself was alive; the master (owned by the terminal
            // owner thread) keeps the fd open for exactly the
            // `TerminalHandle`'s lifetime, so it is still open here.
            // `tcgetpgrp` is documented to return -1 with `errno` set (e.g.
            // `ENOTTY`, `EBADF`) rather than to invoke UB on any input fd,
            // so a race with teardown is a plain error return, not memory
            // unsafety.
            self.pty_master_fd.is_some_and(|fd| {
                let foreground_pgid = unsafe { libc::tcgetpgrp(fd) };
                foreground_pgid > 0 && foreground_pgid as u32 != self.shell_pid
            })
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    fn write(&self, bytes: Vec<u8>) {
        let _ = self.commands.send(TerminalCommand::Input(bytes));
    }

    fn paste(&self, bytes: Vec<u8>) {
        let _ = self.commands.send(TerminalCommand::Paste(bytes));
    }

    fn write_key(&self, input: KeyInput) {
        let _ = self.commands.send(TerminalCommand::Key(input));
    }

    fn clear_screen(&self) {
        let _ = self
            .commands
            .send(TerminalCommand::Feed(b"\x1b[3J\x1b[2J\x1b[H".to_vec()));
    }

    /// Terminate the PTY's process group(s) and tell the owner thread to kill
    /// the child and drop the PTY. portable-pty only kills its direct child;
    /// signaling the group(s) here is what also reaches the shell's
    /// descendants — including ones that detached into their own process
    /// group after job control forked them (F-PER-06), which a single
    /// `killpg` on the pgid captured at spawn never reaches. A stubborn
    /// process gets SIGKILL after a short grace period so close/quit cannot
    /// leave a live process group behind.
    fn shutdown(&self) {
        if self.shutdown_started.swap(true, Ordering::AcqRel) {
            return;
        }
        terminate_descendant_process_groups(self.shell_pid);
        let _ = self.commands.send(TerminalCommand::Shutdown);
    }

    fn resize(&self, columns: u16, lines: u16, cell_width: u16, cell_height: u16) {
        let mut last_size = self.last_size.lock();
        if *last_size == Some((columns, lines)) {
            return;
        }
        *last_size = Some((columns, lines));
        let generation = self.resize_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let resize_generation = Arc::clone(&self.resize_generation);
        let commands = self.commands.clone();
        std::thread::spawn(move || {
            std::thread::sleep(TERMINAL_RESIZE_DEBOUNCE);
            if resize_generation.load(Ordering::Acquire) == generation {
                let _ = commands.send(TerminalCommand::Resize(
                    columns,
                    lines,
                    cell_width,
                    cell_height,
                ));
            }
        });
    }

    fn scroll_display(&self, scroll: SirioScroll) {
        let _ = self.commands.send(TerminalCommand::Scroll(scroll));
    }

    fn snapshot(&self) -> SnapshotReply {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel::<SnapshotReply>();
        if self
            .commands
            .send(TerminalCommand::Snapshot(reply_tx))
            .is_err()
        {
            return (Vec::new(), (usize::MAX, 0));
        }
        reply_rx.recv().unwrap_or((Vec::new(), (usize::MAX, 0)))
    }

    /// Returns the retained grid, refreshing it from the owner thread when
    /// the owner's mutation stamp has moved.
    ///
    /// Load the stamp before taking the cache lock and before the blocking
    /// `snapshot` round trip. If the owner mutates while that round trip is in
    /// flight, caching newer cells under the loaded stamp is safe because the
    /// next caller observes the newer stamp and refreshes again. Loading the
    /// stamp afterwards could retain stale cells under the current stamp.
    /// Never hold the cache lock across the owner-thread round trip.
    fn current_grid(&self) -> parking_lot::MutexGuard<'_, GridRenderCache> {
        let mutation_stamp = self.mutation_stamp.load(Ordering::Relaxed);
        let mut grid_cache = self.grid_render_cache.lock();
        if grid_cache.grid_stamp != Some(mutation_stamp) {
            drop(grid_cache);
            let (cells, cursor) = self.snapshot();
            grid_cache = self.grid_render_cache.lock();
            grid_cache.grid_stamp = Some(mutation_stamp);
            grid_cache.cells = cells;
            grid_cache.cursor = cursor;
        }
        grid_cache
    }

    /// #301 + #302 R3.1: every visible Kitty placement for this frame,
    /// already bucketed into the three z-bands by the owner thread's
    /// `set_layer` walks, copied as plain data (R2.3). Empty buckets when
    /// the emulator holds no Kitty graphics or the terminal is gone.
    fn kitty_places(&self) -> KittyPlacementBuckets {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        if self
            .commands
            .send(TerminalCommand::KittyPlaces(reply_tx))
            .is_err()
        {
            return KittyPlacementBuckets::default();
        }
        reply_rx.recv().unwrap_or_default()
    }

    /// Resolves the link under a viewport cell from the retained grid: the
    /// cell's OSC 8 hyperlink target when the emulator state carries one (#41),
    /// else the shared regex router over the row's plain text — bare URLs stay
    /// clickable, common in agent output that emits no OSC 8. An OSC 8 region
    /// whose display text also regex-matches prefers its target. The retained
    /// grid is refreshed only when its mutation stamp is stale, which is the
    /// only case that round-trips to the owner thread. Callers are main-thread
    /// mouse handlers and never hold the cache lock. The cache uses a
    /// non-reentrant `parking_lot::Mutex`, so this method never re-enters it.
    fn link_at(&self, row: usize, column: usize) -> Option<String> {
        let grid = self.current_grid();
        resolve_link(&grid.cells, row, column)
    }

    /// Captures the grid as newline-delimited plain text. This intentionally
    /// reads the emulator's complete retained history rather than only the
    /// visible viewport, so a persistence layer can restore what the user
    /// would have found by scrolling up.
    /// #259: the range a double or triple click selects around a cell.
    fn click_select(&self, cell: (usize, usize), kind: ClickSelection) -> Option<SelectedRange> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.commands
            .send(TerminalCommand::ClickSelect(cell, kind, reply_tx))
            .ok()?;
        reply_rx.recv().ok().flatten()
    }

    /// #259: the currently selected text, or `None` when nothing is selected.
    fn selected_text(&self) -> Option<String> {
        let range = (*self.selection.lock())?;
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.commands
            .send(TerminalCommand::SelectionText(range, reply_tx))
            .ok()?;
        let text = reply_rx.recv().ok()?;
        (!text.is_empty()).then_some(text)
    }

    fn capture_scrollback(&self) -> Vec<u8> {
        self.scrollback.capture()
    }

    fn try_capture_scrollback(&self) -> Option<Vec<u8>> {
        self.scrollback.try_capture()
    }

    fn scrollback_source(&self) -> ScrollbackCapture {
        self.scrollback.clone()
    }

    /// Replays captured output directly into the emulator. It does not write
    /// to the child PTY, so restoring history cannot execute restored shell
    /// text or otherwise disturb the fresh process.
    fn replay_scrollback(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let _ = self
            .commands
            .send(TerminalCommand::Feed(normalize_scrollback_for_replay(
                bytes,
            )));
    }
}

/// Captured scrollback is newline-delimited plain text, while the VT parser
/// treats LF as a vertical move without returning to column zero. Restore
/// terminal lines with CRLF semantics, retaining any CR that was already
/// persisted for compatibility with older or externally supplied snapshots.
fn normalize_scrollback_for_replay(bytes: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        if byte == b'\n' && normalized.last() != Some(&b'\r') {
            normalized.push(b'\r');
        }
        normalized.push(byte);
    }
    normalized
}

#[cfg_attr(not(unix), allow(dead_code))] // referenced by unix-only teardown
const TERMINAL_TERMINATE_GRACE: Duration = Duration::from_millis(500);
const TERMINAL_RESIZE_DEBOUNCE: Duration = Duration::from_millis(120);

/// Plain-text capture of a terminal's complete retained grid (history +
/// viewport), newline-delimited, trailing blank rows dropped. The one place
/// grid cells become bytes; shared by the owner thread's Text command and the
/// headless boundary tests, so both always see identical extraction behaviour.
fn capture_scrollback_text(terminal: &mut Terminal<'static, 'static>) -> String {
    let columns = usize::from(terminal.cols().unwrap_or(0));
    let total_rows = terminal.total_rows().unwrap_or(0);
    let mut lines = Vec::with_capacity(total_rows);

    for row in 0..total_rows {
        let mut text = String::with_capacity(columns);
        for column in 0..columns {
            // Screen coordinates span history + viewport; row 0 is the top
            // of the scrollback. This walk is not render-loop work (the doc
            // on Terminal::grid_ref warns it may traverse the page list).
            // Capture happens once per settle/persist or on demand when the
            // control socket's scrollback source is read.
            match terminal.grid_ref(GhosttyPoint::Screen(PointCoordinate {
                x: column as u16,
                y: row as u32,
            })) {
                Ok(grid_ref) => {
                    let spacer = grid_ref.cell().is_ok_and(|cell| {
                        matches!(cell.wide(), Ok(CellWide::SpacerTail | CellWide::SpacerHead))
                    });
                    if spacer {
                        text.push(' ');
                    } else {
                        // The full cluster, not just the base codepoint:
                        // Layer C reads this text, so a ZWJ emoji must
                        // survive capture intact (#40 second half).
                        text.push_str(&grid_ref_cluster(grid_ref));
                    }
                }
                Err(_) => text.push(' '),
            }
        }
        while text.ends_with(' ') {
            text.pop();
        }
        lines.push(text);
    }

    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines.join("\n")
}

/// One column of captured text: the full grapheme cluster, or a space when
/// the cell holds nothing. Retries with a bigger buffer when the cluster
/// exceeds GRAPHEME_INLINE codepoints.
fn grid_ref_cluster(grid_ref: GridRef<'_>) -> String {
    let mut buf = vec!['\0'; GRAPHEME_INLINE];
    let count = loop {
        match grid_ref.graphemes(&mut buf) {
            Ok(count) => break count,
            Err(Error::OutOfSpace { required }) => buf.resize(required, '\0'),
            Err(_) => break 0,
        }
    };
    let mut out: String = buf[..count].iter().collect();
    if out.is_empty() {
        out.push(' ');
    }
    out
}

/// A point in viewport coordinates — row 0 is the top of the visible
/// viewport, matching the order the render-state RowIterator yields rows.
fn viewport_point(column: u16, row: u32) -> GhosttyPoint {
    GhosttyPoint::Viewport(PointCoordinate { x: column, y: row })
}

/// One cell's OSC 8 hyperlink URI, or None when the cell carries no link.
/// `Ok(0)` means no hyperlink; retries with a bigger buffer on OutOfSpace —
/// same pattern as [`grid_ref_cluster`]. Reuses the caller's buffer across
/// cells so a frame allocates once, not per linked cell.
fn grid_ref_hyperlink_uri(grid_ref: GridRef<'_>, buf: &mut Vec<u8>) -> Option<String> {
    if buf.len() < 64 {
        buf.resize(64, 0);
    }
    let len = loop {
        match grid_ref.hyperlink_uri(buf) {
            Ok(len) => break len,
            Err(Error::OutOfSpace { required }) => buf.resize(required, 0),
            Err(_) => return None,
        }
    };
    (len > 0).then(|| String::from_utf8_lossy(&buf[..len]).into_owned())
}

/// Link resolution over a paint-frame snapshot (#41): the cell's OSC 8 target
/// when it carries one, else the regex router over the row's plain text. A
/// free function so headless boundary tests exercise the exact path
/// `TerminalHandle::link_at` serves clicks from.
fn resolve_link(cells: &[Vec<SnapshotCell>], row: usize, column: usize) -> Option<String> {
    let line = cells.get(row)?;
    let cell = line.get(column)?;
    if let Some(uri) = &cell.hyperlink {
        return Some(uri.to_string());
    }
    let text: String = line.iter().flat_map(|c| c.chars()).collect();
    let byte_column = text
        .char_indices()
        .nth(column)
        .map_or(text.len(), |(index, _)| index);
    url_at_column(&text, byte_column)
}

#[cfg(unix)]
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
#[cfg(target_os = "linux")]
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

/// macOS implementation of [`descendant_pids`], using libproc's live child
/// table because macOS has no `/proc` filesystem.
#[cfg(all(unix, not(target_os = "linux")))]
fn descendant_pids(root: libc::pid_t) -> Vec<libc::pid_t> {
    #[cfg(target_os = "macos")]
    {
        let mut discovered = Vec::new();
        let mut seen = std::collections::HashSet::from([root]);
        let mut frontier = vec![root];
        while let Some(pid) = frontier.pop() {
            let Ok(children) = macos_child_pids(pid) else {
                continue;
            };
            for child in children {
                if seen.insert(child) {
                    discovered.push(child);
                    frontier.push(child);
                }
            }
        }
        discovered
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = root;
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
fn macos_child_pids(parent: libc::pid_t) -> std::io::Result<Vec<libc::pid_t>> {
    use std::os::raw::{c_int, c_void};

    const INITIAL_PID_CAPACITY: usize = 64;
    const MAX_PID_CAPACITY: usize = 16_384;

    #[link(name = "proc")]
    unsafe extern "C" {
        fn proc_listchildpids(ppid: c_int, buffer: *mut c_void, buffersize: c_int) -> c_int;
    }

    let mut buffer = vec![0_i32; INITIAL_PID_CAPACITY];
    loop {
        let buffer_size = (buffer.len() * std::mem::size_of::<libc::pid_t>()) as c_int;
        // libproc returns the number of PIDs copied, not a byte count.
        let reported_count =
            unsafe { proc_listchildpids(parent, buffer.as_mut_ptr().cast(), buffer_size) };
        if reported_count < 0 {
            return Err(std::io::Error::last_os_error());
        }

        let count = (reported_count as usize).min(buffer.len());
        if (reported_count as usize) < buffer.len() {
            return Ok(buffer[..count]
                .iter()
                .copied()
                .filter(|pid| *pid > 0)
                .map(|pid| pid as libc::pid_t)
                .collect());
        }

        if buffer.len() >= MAX_PID_CAPACITY {
            return Err(std::io::Error::new(
                std::io::ErrorKind::OutOfMemory,
                "macOS child-process list exceeded safety limit",
            ));
        }
        buffer.resize((buffer.len() * 2).min(MAX_PID_CAPACITY), 0);
    }
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
#[cfg(unix)]
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
#[cfg(unix)]
fn terminate_descendant_process_groups(shell_pid: u32) {
    for group in descendant_process_groups(shell_pid as libc::pid_t) {
        terminate_process_group(group as u32);
    }
}

/// Windows stand-in for [`terminate_descendant_process_groups`]. Unix's
/// `killpg`/`getpgid` walk over `/proc/<pid>/task/<tid>/children` has no
/// Toolhelp32 port here (`CreateToolhelp32Snapshot` + `Process32First`/
/// `Process32Next`, matching `th32ParentProcessID`, would enumerate the
/// descendants; there is no Windows process-group signal to broadcast to
/// once found). The more idiomatic Windows fix is different in kind, not
/// just in API: attach the child to a Job Object created with
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so closing the job handle kills the
/// whole descendant tree atomically — no enumerate-then-kill race at all.
/// `portable-pty`'s `tty/windows/` (ConPTY) backend is the natural
/// place to own that job handle, since it already owns the child's lifetime;
/// duplicating it here would fight that ownership rather than complement it.
/// This is therefore a real no-op, not a partial implementation: the PTY's
/// own child still gets torn down via the `Msg::Shutdown` send right after
/// this call in `shutdown()`, but a descendant that has spawned its own
/// detached process is not reached, and there is no substitute here yet.
#[cfg(not(unix))]
fn terminate_descendant_process_groups(_shell_pid: u32) {}

/// A live PTY-backed terminal view, or a failed pane showing why the PTY
/// could not be started.
pub struct TerminalView {
    terminal: TerminalState,
    spawn: SpawnParams,
    empty_prompt: bool,
    font_size: Pixels,
    focus_handle: gpui::FocusHandle,
    exit_status: Option<TerminalExitStatus>,
    identity: TerminalIdentity,
    context_menu: Option<Point<Pixels>>,
    /// #259: the running autoscroll, armed only while a drag is held outside
    /// the pane. Cancel-on-drop, so releasing the button or coming back
    /// inside stops it -- one timer per surface, the discipline
    /// `caret::schedule` and the composer's streaming border already follow,
    /// rather than a loop that runs whether or not anyone is dragging.
    autoscroll: Option<gpui::Task<()>>,
    /// Live link-hover tooltip (#41): the pointer position plus the resolved
    /// target URI of the cell under the pointer. Set only while the platform
    /// modifier is held over a linked cell; cleared on modifier release or
    /// when the pointer moves off links.
    link_hover: Option<LinkHover>,
    last_dropped_diff: Option<(PathBuf, String)>,
    last_dropped_files: Option<Vec<PathBuf>>,
    /// F-TAB-11 (`SplitDisabledReason::SoleTabInGroup` half): whether this
    /// pane's tab is currently the only tab in its pane group. Only the host
    /// (`sirio::main`) can count tab-group membership, so this is pushed in
    /// as a derived boolean via [`Self::set_sole_tab_in_group`] rather than
    /// computed locally — `sirio_terminal` cannot depend on `sirio`'s tab
    /// machinery.
    sole_tab_in_group: bool,
    /// How many times gpui asked this view to render. Test-observable only:
    /// the host caches pane views, and a still terminal must not render at
    /// all while a spinner elsewhere keeps the window drawing.
    render_count: u64,
    /// F-TERM-PTY-07: this content's own [`TerminalSurfaceHost`], keyed by
    /// the same `terminal_id` its [`TerminalIdentity`] carries. `generation`
    /// bumps on every real respawn after the process is gone (a user
    /// "Restart Terminal", or [`Self::retry`] after a failed spawn) and gates
    /// [`Self::pump_terminal_events`]'s background task: that task is
    /// spawned per-process and outlives a respawn, so without this a late
    /// `ChildExit` from an already-replaced PTY would otherwise overwrite the
    /// *new* process's live state with the old one's exit status.
    host: TerminalSurfaceHost,
}

/// Output is forwarded to the activity model only after this quiet period.
/// This is intentionally longer than the renderer's frame cadence: an agent
/// turn commonly arrives as several PTY writes and must be observed as one
/// settled snapshot.
const OUTPUT_SETTLE_DEBOUNCE: Duration = Duration::from_millis(200);
pub const CONTENT_MATCH_BYTE_LIMIT: usize = 10 * 1024;
pub const CONTENT_MATCH_LINE_LIMIT: usize = 40;

/// A live link-hover tooltip's state (#41). Dumb by design: window-absolute
/// anchor plus the URI string; no interaction.
#[derive(Clone)]
struct LinkHover {
    position: Point<Pixels>,
    uri: String,
}
/// Poll interval for the PTY event channel. The GPUI task owns this timer and
/// polls the channel with `try_recv`; the alacritty reader thread therefore
/// never wakes the deterministic test scheduler directly.
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(4);
/// Cap on events drained in one poll tick, bounding how long the pump can
/// hold the executor before yielding. The *repaint* rate is bounded by
/// [`REPAINT_FRAME_BUDGET`], not by this.
const EVENT_COALESCE_CAP: usize = 100;
/// Floor on the interval between two repaints while output keeps arriving.
/// A PTY that never stops writing must not be allowed to drive repeated grid
/// snapshot/assembly work faster than a frame on a maximised pane. It is a
/// floor and not a delay: output arriving into a quiet pane -- every
/// keystroke a user actually types -- repaints on the very next poll tick.
const REPAINT_FRAME_BUDGET: Duration = Duration::from_millis(16);
/// Once output has settled, keep polling a capture that the owner thread has
/// not served yet. The owner is allowed to finish parsing the burst first,
/// but the view must wake again when that final snapshot can be obtained.
const SCROLLBACK_CAPTURE_RETRY_INTERVAL: Duration = Duration::from_millis(1);

fn terminal_pump_interval(capture_retry_pending: bool) -> Duration {
    if capture_retry_pending {
        SCROLLBACK_CAPTURE_RETRY_INTERVAL
    } else {
        EVENT_POLL_INTERVAL
    }
}

/// #301 R1.3 + #308 R4.6: the Kitty ingest ceilings and the atlas cache cap
/// are ALL decided together and stated as one coordinated pair of numbers —
/// an emulator limit generous enough to retain hundreds of images, paired
/// with a cache that uploads every one of them, would be an atlas policy
/// nobody chose. 32 MiB holds any base64 PNG omp emits in a single APC; the
/// emulator's image store keeps 64 MiB of image bytes (≈200 images at the
/// 320 KiB average the store implies); the pane's atlas cache mirrors that
/// same ≈200-image working set, evicting least-recently-painted first
/// (R4.7) so GPU memory tracks the emulator's own retention instead of
/// growing without limit.
const KITTY_APC_MAX_BYTES: usize = 32 * 1024 * 1024;
const KITTY_IMAGE_STORAGE_LIMIT: u64 = 64 * 1024 * 1024;
/// #303 R2.4: cap image decoder allocations, including the decompressed
/// RGBA output, before a compressed PNG can expand into an unbounded buffer.
const KITTY_DECOMPRESSED_MAX_BYTES: u64 = KITTY_IMAGE_STORAGE_LIMIT;

/// #308 R4.6: the atlas ceiling, coordinated with `KITTY_IMAGE_STORAGE_LIMIT`
/// above (64 MiB ÷ 200 ≈ 320 KiB per image). No agent session renders more
/// than this many Kitty images at once, and the LRU-painted eviction (R4.7)
/// bounds everything else, so GPU memory cannot grow past the configured
/// ceiling however many images a session replaces or scrolls past.
const KITTY_ATLAS_IMAGE_CAP: usize = 200;

/// #301 R2.6: the wash for a placement whose image the pane refused to
/// decode — a silent drop is indistinguishable from the renderer being
/// absent, so a refused image paints this instead of nothing.
const KITTY_REFUSED_COLOR: Hsla = Hsla {
    h: 0.0,
    s: 0.8,
    l: 0.5,
    a: 0.7,
};

/// #303 R2.6: PNG decoding can fail before libghostty stores a placement, so
/// there is no image geometry to use for the normal refusal wash. Keep the
/// indicator small and anchored inside the pane; the content mask clips it.
fn kitty_refused_indicator_bounds(pane: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::new(pane.origin, size(px(16.0), px(16.0)))
}

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

fn scroll_lines_from_wheel_delta(delta: ScrollDelta, line_height: Pixels) -> Option<isize> {
    let (delta_y, line_height) = match delta {
        ScrollDelta::Pixels(pixels) => (f32::from(pixels.y), f32::from(line_height)),
        ScrollDelta::Lines(lines) => (lines.y, 1.0),
    };
    if !delta_y.is_finite() || delta_y == 0.0 {
        return None;
    }

    let lines = (delta_y / line_height).round() as isize;
    Some(if lines == 0 {
        if delta_y > 0.0 { -1 } else { 1 }
    } else {
        -lines
    })
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

        let host = TerminalSurfaceHost::new(identity.terminal_id());
        Ok(Self {
            terminal: TerminalState::Pending,
            spawn,
            empty_prompt: false,
            font_size: FONT_SIZE,
            focus_handle,
            exit_status: None,
            identity,
            context_menu: None,
            autoscroll: None,
            link_hover: None,
            last_dropped_diff: None,
            last_dropped_files: None,
            sole_tab_in_group: false,
            render_count: 0,
            host,
        })
    }

    /// Constructs the unmounted-pane surface. The host can subscribe to
    /// [`TerminalPromptEvent`] and call [`Self::start_from_prompt`] after the
    /// user chooses one of the two actions.
    pub fn empty_prompt(cx: &mut gpui::Context<Self>) -> Self {
        let working_directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let identity = generated_identity();
        let host = TerminalSurfaceHost::new(identity.terminal_id());
        Self {
            terminal: TerminalState::Pending,
            spawn: SpawnParams {
                working_directory,
                shell: TerminalShell::System,
            },
            empty_prompt: true,
            font_size: FONT_SIZE,
            focus_handle: cx.focus_handle(),
            exit_status: None,
            identity,
            context_menu: None,
            autoscroll: None,
            link_hover: None,
            last_dropped_diff: None,
            last_dropped_files: None,
            sole_tab_in_group: false,
            render_count: 0,
            host,
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
        let identity = generated_identity();
        // Starts torn down: there is no live process behind a pane that
        // begins in the failed state, so a subsequent Restart Terminal
        // correctly reads as the *first* relaunch, not a second one.
        let mut host = TerminalSurfaceHost::new(identity.terminal_id());
        host.teardown();
        Self {
            terminal: TerminalState::Failed {
                message: message.into(),
            },
            spawn: SpawnParams {
                working_directory: working_directory.as_ref().to_path_buf(),
                shell,
            },
            empty_prompt: false,
            font_size: FONT_SIZE,
            focus_handle: cx.focus_handle(),
            exit_status: None,
            identity,
            context_menu: None,
            autoscroll: None,
            link_hover: None,
            last_dropped_diff: None,
            last_dropped_files: None,
            sole_tab_in_group: false,
            render_count: 0,
            host,
        }
    }

    /// Replaces the generated identity with the host's stable pane and
    /// terminal IDs. The terminal view uses this target for every delegated
    /// context-menu event, so a menu opened in one split cannot act on another.
    /// The first render is deliberately where the PTY is spawned, so callers
    /// may assign this identity immediately after creating the entity and the
    /// child will inherit `SIRIO_PANE_ID`.
    pub fn set_identity(&mut self, identity: TerminalIdentity) {
        self.identity = identity;
    }

    /// Applies the user-configured terminal font size. The terminal renderer
    /// owns the derived cell metrics, so notifying this entity invalidates the
    /// retained element and causes the next prepaint to resize the grid.
    pub fn set_font_size(&mut self, value: i32, cx: &mut gpui::Context<Self>) {
        let font_size = px(value.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE) as f32);
        if self.font_size == font_size {
            return;
        }
        self.font_size = font_size;
        cx.notify();
    }

    /// F-TAB-11 (`SplitDisabledReason::SoleTabInGroup` half): the host calls
    /// this whenever it re-renders the pane tree, since tab-group membership
    /// can change without this terminal's own state changing. Returns whether
    /// the stored value changed; the host notifies only when it returns true.
    pub fn set_sole_tab_in_group(&mut self, sole: bool) -> bool {
        if self.sole_tab_in_group == sole {
            return false;
        }
        self.sole_tab_in_group = sole;
        true
    }

    /// Test/inspection accessor for [`Self::set_sole_tab_in_group`]'s
    /// current value.
    pub fn sole_tab_in_group(&self) -> bool {
        self.sole_tab_in_group
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

    /// F-TERM-03: whether the pane's status pill should show "Running" right
    /// now — a live PTY, no recorded exit yet, and a foreground command
    /// currently holds the terminal (see
    /// [`TerminalHandle::foreground_command_running`] for the definition and
    /// the alternatives it rejects). Exit takes priority by construction:
    /// once `exit_status` is recorded the PTY is gone, so this is `false`
    /// from then on without needing an explicit check here.
    pub fn is_command_running(&self) -> bool {
        self.running_terminal()
            .is_some_and(TerminalHandle::foreground_command_running)
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
    /// quoted-and-space-joined insertion `sirio_project::terminal_file_drop`
    /// already builds for a `Vec`.
    ///
    /// The drop also *returns focus to the terminal*, which is the second
    /// conjunct of the clause and the half this port was missing. A drop
    /// leaves the paths at the shell's cursor with no trailing newline
    /// precisely so the reader can finish the command themselves — but they
    /// can only do that if their next keystroke reaches the pane. Whatever
    /// held focus when the drag started (in practice a text field elsewhere
    /// in the window: the sidebar Filter, the chat composer) otherwise keeps
    /// it, and the typing lands there. Swift's `PtyTerminalPane.handleDrop`
    /// ends `write(paneId:data:)` with `runtime?.proxy.focus()` for the same
    /// reason.
    fn receive_file_drop(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if paths.is_empty() {
            return;
        }
        let insertion = sirio_project::terminal_file_drop(&paths);
        if insertion.is_empty() {
            return;
        }
        self.last_dropped_files = Some(paths.clone());
        self.input(insertion.into_bytes());
        self.focus_handle.focus(window, cx);
        cx.emit(TerminalDropEvent::Files { paths });
        cx.notify();
    }

    /// The PID of the shell (or direct command) at the root of this pane's
    /// PTY. Layer D walks descendants of this PID on its periodic refresh.
    pub fn shell_pid(&self) -> Option<u32> {
        self.running_terminal().map(|terminal| terminal.shell_pid)
    }

    /// How many times **this pane's** owner thread has been asked for its
    /// scrollback text. The render and activity paths must never move it;
    /// `sirio`'s `render_never_captures_scrollback` reads it either side of
    /// a refresh to prove that.
    ///
    /// Per pane rather than process-wide on purpose — see the field's own
    /// comment. A pending or failed pane has no owner thread and so has
    /// served nothing.
    pub fn scrollback_captures(&self) -> u64 {
        self.running_terminal()
            .map(|terminal| terminal.scrollback_captures.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    /// Captures the renderer's current plain-text history for persistence or
    /// headless verification. Failed panes have no emulator contents.
    pub fn capture_scrollback(&self) -> Vec<u8> {
        match &self.terminal {
            TerminalState::Running(terminal) => terminal.capture_scrollback(),
            TerminalState::Pending | TerminalState::Failed { .. } => Vec::new(),
        }
    }

    /// Returns the live on-demand source used by the control socket to read
    /// this terminal's scrollback. Pending and failed panes have no owner
    /// thread and therefore expose no source.
    pub fn scrollback_source(&self) -> Option<ScrollbackCapture> {
        match &self.terminal {
            TerminalState::Running(terminal) => Some(terminal.scrollback_source()),
            TerminalState::Pending | TerminalState::Failed { .. } => None,
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
    ) -> Result<(TerminalHandle, UnboundedReceiver<TerminalEvent>)> {
        TerminalHandle::new_with_pane_id(&spawn.working_directory, &spawn.shell, Some(pane_id))
    }

    fn ensure_started(&mut self, cx: &mut gpui::Context<Self>) {
        if self.empty_prompt || !matches!(self.terminal, TerminalState::Pending) {
            return;
        }
        match Self::spawn_terminal(&self.spawn, self.identity.pane_id()) {
            Ok((terminal, wakeup_rx)) => {
                Self::pump_terminal_events(terminal.clone(), wakeup_rx, self.host.generation(), cx);
                self.terminal = TerminalState::Running(terminal);
            }
            Err(error) => {
                self.terminal = TerminalState::Failed {
                    message: format!("{error:#}"),
                };
                self.host.teardown();
            }
        }
        self.emit_lifecycle_changed(cx);
    }

    fn emit_lifecycle_changed(&self, cx: &mut gpui::Context<Self>) {
        cx.emit(TerminalActivityEvent::LifecycleChanged {
            failed: self.is_failed(),
            exit_status: self.exit_status,
        });
    }

    /// Re-attempts the spawn after a failure: on success the pane switches
    /// to the live terminal, on failure the message is updated in place.
    /// F-TERM-PTY-07: this is also a relaunch -- the previous attempt never
    /// reached `Running`, so there is no live process this one could race
    /// against, but the generation still bumps so the surface host's count
    /// reflects every real "started a fresh process" event, not just the
    /// ones reached through [`Self::restart`].
    fn retry(&mut self, cx: &mut gpui::Context<Self>) {
        self.host.relaunch();
        match Self::spawn_terminal(&self.spawn, self.identity.pane_id()) {
            Ok((terminal, wakeup_rx)) => {
                Self::pump_terminal_events(terminal.clone(), wakeup_rx, self.host.generation(), cx);
                self.terminal = TerminalState::Running(terminal);
                self.exit_status = None;
            }
            Err(error) => {
                self.terminal = TerminalState::Failed {
                    message: format!("{error:#}"),
                };
                self.host.teardown();
            }
        }
        self.emit_lifecycle_changed(cx);
        cx.notify();
    }

    /// F-TERM-PTY-07 (`TerminalSurfaceHost::relaunch`): tears down the live
    /// process (if any) and starts a fresh one in place, in the *same*
    /// `Entity<TerminalView>` and therefore the same pane/split -- the host
    /// bumps to a new generation first, so the outgoing PTY's own
    /// `pump_terminal_events` task cannot clobber the incoming one's state
    /// (see that function's doc comment). Public: this is the "relaunch it"
    /// half of the row's own VERIFY clause, driven from a context-menu
    /// action the app crate owns.
    pub fn restart(&mut self, cx: &mut gpui::Context<Self>) {
        self.shutdown();
        self.host.relaunch();
        self.terminal = TerminalState::Pending;
        self.exit_status = None;
        self.ensure_started(cx);
        cx.notify();
    }

    /// The surface host's current generation -- bumps by one on every real
    /// respawn ([`Self::restart`], and [`Self::retry`] after a failure).
    /// Exposed for the app crate's tests and any UI that wants to show it.
    pub fn surface_generation(&self) -> u64 {
        self.host.generation()
    }

    /// Whether a live process currently backs this content id. False after
    /// an exit/failure and before the next successful (re)spawn.
    pub fn is_host_mounted(&self) -> bool {
        self.host.is_mounted()
    }

    fn exit_status_from_event(event: &TerminalEvent) -> Option<TerminalExitStatus> {
        match event {
            TerminalEvent::ChildExit(status) => Some(*status),
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
        mut wakeup_rx: UnboundedReceiver<TerminalEvent>,
        // F-TERM-PTY-07: the surface-host generation this specific PTY was
        // spawned under. This task outlives a respawn (nothing cancels it),
        // so every application below is gated on the view's *current*
        // generation still matching -- otherwise a late `ChildExit` from a
        // process a "Restart Terminal" already replaced would overwrite the
        // new process's live state with the old one's exit status.
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            // Output that has been drawn but not yet reported to the activity
            // model, plus how long the channel has been quiet since. The two
            // deadlines are deliberately different: the screen must repaint on
            // the very next poll tick, while the activity model must not see a
            // turn until it has stopped moving.
            let mut unsettled_output = false;
            let mut quiet_for = Duration::ZERO;
            // Output drawn into the emulator that the view has not been told
            // about yet, and how long it has been since the last repaint. A
            // pane that has been quiet accumulates `since_repaint` well past
            // the budget, so the first byte after a pause always redraws at
            // once; only sustained output is held to the frame floor.
            let mut awaiting_repaint = false;
            let mut since_repaint = REPAINT_FRAME_BUDGET;
            // A capture request can be queued while the owner is still
            // parsing the final output batch. Keep this task alive on a short
            // timer until that request replies; otherwise a burst that ends
            // between two PTY events can leave the pane on its penultimate
            // frame forever.
            let mut capture_retry_pending = false;
            loop {
                // Do not await the cross-thread channel. Its waker runs on
                // the PTY reader thread, which violates GPUI's deterministic
                // TestAppContext scheduler. Polling from this task keeps all
                // scheduler interaction on its owning executor.
                let poll_interval = terminal_pump_interval(capture_retry_pending);
                cx.background_executor().timer(poll_interval).await;
                since_repaint = since_repaint.saturating_add(poll_interval);
                let mut exit_status = None;
                let mut osc_title = None;
                let mut viewport_changed = false;
                let mut output_seen = false;
                let mut pending = 0;
                // Coalesce whatever is *already* queued -- never wait for
                // more. Waiting is what used to cost a settle period per
                // keystroke; a quiet channel now costs one poll tick.
                while pending < EVENT_COALESCE_CAP {
                    match wakeup_rx.try_recv() {
                        Ok(event) => {
                            if exit_status.is_none() {
                                exit_status = Self::exit_status_from_event(&event);
                            }
                            if let Some(title) = Self::osc_title_from_event(&event) {
                                osc_title = Some(title);
                            }
                            viewport_changed |=
                                matches!(event, TerminalEvent::ViewportChanged);
                            output_seen |= matches!(event, TerminalEvent::Wakeup);
                            pending += 1;
                        }
                        Err(TryRecvError::Closed) => return,
                        Err(TryRecvError::Empty) => break,
                    }
                }

                if pending > 0 {
                    unsettled_output |= output_seen;
                    awaiting_repaint |= output_seen;
                    quiet_for = Duration::ZERO;
                    // A child exiting or retitling changes what the pane says
                    // about itself, is rare, and cannot wait behind a frame
                    // floor meant for a torrent of ordinary output.
                    let structural = exit_status.is_some() || osc_title.is_some();
                    let repaint =
                        structural || viewport_changed || since_repaint >= REPAINT_FRAME_BUDGET;
                    if this
                        .update(cx, |view, cx| {
                            if view.host.generation() != generation {
                                // Superseded by a later respawn -- this batch
                                // is from a PTY the view has already moved
                                // past.
                                return;
                            }
                            if let Some(exit_status) = exit_status {
                                view.exit_status = Some(exit_status);
                                view.host.teardown();
                                cx.emit(TerminalActivityEvent::ChildExited {
                                    status: exit_status,
                                });
                            }
                            if let Some(title) = osc_title {
                                cx.emit(TerminalActivityEvent::OscTitle(title));
                            }
                            if repaint {
                                sirio_perf::event(
                                    "notify.Terminal.output",
                                    cx.entity_id().as_u64(),
                                );
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        // The view is gone; stop pumping.
                        return;
                    }
                    if repaint {
                        awaiting_repaint = false;
                        since_repaint = Duration::ZERO;
                    }
                    continue;
                }

                // Output that arrived inside the frame floor and was held
                // back: the channel has gone quiet, so nothing later will
                // redraw it. Flush it as soon as the floor allows.
                if awaiting_repaint && since_repaint >= REPAINT_FRAME_BUDGET {
                    if this
                        .update(cx, |view, cx| {
                            if view.host.generation() != generation {
                                return;
                            }
                            sirio_perf::event(
                                "notify.Terminal.deferred_output",
                                cx.entity_id().as_u64(),
                            );
                            cx.notify();
                        })
                        .is_err()
                    {
                        return;
                    }
                    awaiting_repaint = false;
                    since_repaint = Duration::ZERO;
                }

                // Nothing queued. Once the channel has been quiet for a full
                // settle period, hand the turn to the activity model. The
                // scrollback capture lives here, and only here, so a flood
                // costs one capture per turn instead of one per batch.
                quiet_for += poll_interval;
                if unsettled_output && quiet_for >= OUTPUT_SETTLE_DEBOUNCE {
                    if let Some(scrollback) = terminal.try_capture_scrollback() {
                        capture_retry_pending = false;
                        unsettled_output = false;
                        if this
                            .update(cx, |view, cx| {
                                if view.host.generation() != generation {
                                    return;
                                }
                                let scrollback =
                                    recent_content_window(&String::from_utf8_lossy(&scrollback));
                                cx.emit(TerminalActivityEvent::OutputSettled { scrollback });
                                // The capture may complete after the final
                                // PTY event and therefore after the last
                                // output-driven repaint. Schedule the frame
                                // that contains this fresh terminal state.
                                sirio_perf::event(
                                    "notify.Terminal.settled_capture",
                                    cx.entity_id().as_u64(),
                                );
                                cx.notify();
                            })
                            .is_err()
                        {
                            return;
                        }
                    } else {
                        capture_retry_pending = true;
                    }
                }
            }
        })
        .detach();
    }

    fn osc_title_from_event(event: &TerminalEvent) -> Option<String> {
        match event {
            // The owner thread diffs the emulator's title and emits on every
            // change; an empty title is an OSC title reset. An empty title
            // lets the activity model clear a title-owned pane through its
            // normal unmatched-title path without confusing it with SetTitle.
            TerminalEvent::Title(title) => Some(title.clone()),
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

    /// Shared encode-or-drop path for every mouse listener (#43): when the
    /// guest requested tracking (the shared flag the owner thread refreshes
    /// each poll iteration), convert through the tested `mouse_input` path and
    /// fire-and-forget to the owner thread. Shift/right-button gating lives
    /// inside `mouse_input`, so tests drive exactly what production runs.
    fn encode_mouse(
        handle: &TerminalHandle,
        action: mouse::Action,
        button: Option<mouse::Button>,
        pressed_button: Option<MouseButton>,
        event_position: Point<Pixels>,
        modifiers: gpui::Modifiers,
    ) {
        if !handle.mouse_tracking.load(Ordering::Relaxed) {
            return;
        }
        let Some(bounds) = *handle.last_bounds.lock() else {
            return;
        };
        let cell_width = px(handle.last_cell_width.lock().map(f32::from).unwrap_or(8.0));
        let cell_height = px(
            handle
                .last_cell_height
                .lock()
                .map(f32::from)
                .unwrap_or(f32::from(LINE_HEIGHT)),
        );
        if let Some(input) = mouse_input(
            action,
            button,
            pressed_button,
            event_position,
            modifiers,
            bounds,
            cell_width,
            cell_height,
        ) {
            let _ = handle.commands.send(TerminalCommand::Mouse(input));
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

    /// #259: how far to autoscroll for a drag at this height, in lines.
    ///
    /// Negative scrolls towards the scrollback, positive towards the active
    /// area, zero means the drag is comfortably inside and nothing should
    /// move.
    ///
    /// The trigger is a band *just inside* each edge, not the boundary
    /// itself. That is not a nicety: GPUI stops delivering `on_mouse_move` to
    /// an element once the pointer leaves its bounds, so a rule that only
    /// fired outside the pane would never fire at all -- measured, after a
    /// first version that armed on `pointer_y < top` and produced exactly
    /// zero ticks. It is also why libghostty-vt ships an autoscroll tick
    /// event separate from its drag events.
    ///
    /// One line per tick rather than a rate proportional to how far past the
    /// edge the pointer is: a terminal drag aims at a line, and acceleration
    /// is what makes autoscroll overshoot it.
    fn autoscroll_lines(
        pointer_y: f32,
        top: f32,
        bottom: f32,
        line_height: Pixels,
    ) -> isize {
        // One row. Measured: a 12px band left a pointer 3px below it
        // reporting "inside", which is not a distinction a hand at the edge
        // of a pane can make. A row is also the unit being scrolled, so the
        // band and the step agree.
        let edge_band = f32::from(line_height);
        if pointer_y < top + edge_band {
            -1
        } else if pointer_y > bottom - edge_band {
            1
        } else {
            0
        }
    }

    /// #259: whether a left drag makes a host selection rather than going to
    /// the guest.
    ///
    /// The convention xterm, alacritty, iTerm2 and Ghostty's own app share:
    /// while the guest is tracking the mouse it owns bare drags, and `Shift`
    /// takes one back for the host. With tracking off there is nothing to
    /// take it from. It does not collide with the `platform` modifier, which
    /// link opening already claims (`link_router::opens_terminal_link`).
    fn selection_gesture_wanted(mouse_tracking: bool, shift: bool) -> bool {
        !mouse_tracking || shift
    }

    /// The viewport cell under a window-space position.
    ///
    /// Undoes the pane origin and divides by the measured glyph advance --
    /// the same inversion link hit-testing does, kept in one place now that
    /// selection needs it too (F-TERM-UI-02 explains why the origin and the
    /// real `last_cell_width` both matter).
    fn cell_at(terminal: &TerminalHandle, position: gpui::Point<Pixels>) -> (usize, usize) {
        let origin = terminal
            .last_bounds
            .lock()
            .map(|bounds| bounds.origin)
            .unwrap_or_default();
        let cell_width = terminal
            .last_cell_width
            .lock()
            .map(f32::from)
            .unwrap_or(8.0);
        let cell_height = terminal
            .last_cell_height
            .lock()
            .map(f32::from)
            .unwrap_or(f32::from(LINE_HEIGHT));
        link_router::resolve_click_cell(
            f32::from(position.x),
            f32::from(position.y),
            f32::from(origin.x),
            f32::from(origin.y),
            cell_width,
            cell_height,
        )
    }

    fn on_left_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        // #43: Sirio's gestures are decided BEFORE any encoding — left-down
        // still focuses (above) and platform+left-click still opens links
        // below. Whatever remains may belong to the guest: encode the press
        // when it requested reporting and shift isn't bypassing it.
        if let Some(terminal) = self.running_terminal() {
            Self::encode_mouse(
                terminal,
                mouse::Action::Press,
                Some(mouse::Button::Left),
                Some(MouseButton::Left),
                event.position,
                event.modifiers,
            );
        }
        // #259: a press anchors a possible drag and drops the previous
        // highlight. It does not select anything on its own -- a bare click
        // must clear, not select the cell under the pointer.
        //
        // Not while the context menu is open, though. Clicking `Copy` in it
        // is a left press over this same pane, and the press arrives before
        // the item's own handler: clearing here left `copy_text` with nothing
        // to copy, and the highlight vanishing under the click. Found by
        // driving the real app -- the unit tests below cannot see it, because
        // the ordering is GPUI's, not this function's.
        if self.context_menu.is_none()
            && let Some(terminal) = self.running_terminal()
            && Self::selection_gesture_wanted(
                terminal.mouse_tracking.load(Ordering::Relaxed),
                event.modifiers.shift,
            )
        {
            let cell = Self::cell_at(terminal, event.position);
            // A repeated click selects a word or a line outright; a single one
            // only anchors, so a plain click clears rather than selecting the
            // cell under the pointer.
            let clicked = ClickSelection::for_click_count(event.click_count)
                .and_then(|kind| terminal.click_select(cell, kind));
            *terminal.selection_anchor.lock() = Some(cell);
            let mut selection = terminal.selection.lock();
            let previous = *selection;
            *selection = clicked;
            if previous != clicked {
                drop(selection);
                cx.notify();
            }
        }
        if !opens_terminal_link(event.modifiers.platform) {
            return;
        }
        let Some(terminal) = self.running_terminal() else {
            return;
        };
        // event.position arrives in window coordinates; the paint path below
        // (TerminalElement::prepaint) computes cell rects as
        // `bounds.origin + cell_width * column`, so hit-testing must undo
        // that same offset or every pane but one flush against the window's
        // top-left corner maps clicks to the wrong cell (F-TERM-UI-02).
        let origin = terminal
            .last_bounds
            .lock()
            .map(|bounds| bounds.origin)
            .unwrap_or_default();
        // The real measured glyph advance `prepaint` painted cells with, not
        // a hardcoded guess (F-TERM-UI-02) -- see `last_cell_width`'s doc
        // comment for why a mismatch here silently mis-hit-tests every link
        // that isn't in the leftmost few columns.
        let cell_width = terminal
            .last_cell_width
            .lock()
            .map(f32::from)
            .unwrap_or(8.0);
        let cell_height = terminal
            .last_cell_height
            .lock()
            .map(f32::from)
            .unwrap_or(f32::from(LINE_HEIGHT));
        let (row, column) = link_router::resolve_click_cell(
            f32::from(event.position.x),
            f32::from(event.position.y),
            f32::from(origin.x),
            f32::from(origin.y),
            cell_width,
            cell_height,
        );
        let link = terminal.link_at(row, column);
        if std::env::var_os("SIRIO_DEBUG_LINK_CLICK").is_some() {
            eprintln!(
                "SIRIO_DEBUG_LINK_CLICK pos=({:.1},{:.1}) origin=({:.1},{:.1}) cell_width={:.3} row={} column={} link={:?}",
                f32::from(event.position.x),
                f32::from(event.position.y),
                f32::from(origin.x),
                f32::from(origin.y),
                cell_width,
                row,
                column,
                link,
            );
        }
        if let Some(url) = link {
            cx.emit(TerminalLinkEvent {
                target: self.identity.clone(),
                url,
            });
        }
    }

    /// #41: with the platform modifier held, hovering a linked cell shows its
    /// target URI near the pointer — an OSC 8 region's display text may not
    /// reveal the target, so the hover must. Cleared on modifier release or
    /// whenever the pointer moves off a linked cell.
    fn on_link_hover_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // #43: encode guest-bound motion first; the hover logic below is a
        // Sirio-only concern gated on the platform modifier and must not
        // consume the move event.
        if let Some(terminal) = self.running_terminal() {
            let button = event.pressed_button.and_then(gpui_to_ghostty_button);
            Self::encode_mouse(
                terminal,
                mouse::Action::Motion,
                button,
                event.pressed_button,
                event.position,
                event.modifiers,
            );
        }
        // #259: extend the selection while the left button is held from an
        // anchored press. Runs before the hover branch below, which returns
        // early for anything without the platform modifier.
        if event.pressed_button == Some(MouseButton::Left)
            && let Some(terminal) = self.running_terminal().cloned()
            && let Some(anchor) = *terminal.selection_anchor.lock()
        {
            let terminal = &terminal;
            let cell = Self::cell_at(terminal, event.position);
            let range = SelectedRange::between(anchor, cell);
            let range = (!range.is_empty()).then_some(range);
            let mut current = terminal.selection.lock();
            if *current != range {
                *current = range;
                drop(current);
                cx.notify();
            }
            let bounds = *terminal.last_bounds.lock();
            let lines = bounds
                .map(|bounds| {
                    Self::autoscroll_lines(
                        f32::from(event.position.y),
                        f32::from(bounds.origin.y),
                        f32::from(bounds.origin.y + bounds.size.height),
                        px(
                            terminal
                                .last_cell_height
                                .lock()
                                .map(f32::from)
                                .unwrap_or(f32::from(LINE_HEIGHT)),
                        ),
                    )
                })
                .unwrap_or(0);
            if lines == 0 {
                // Back inside the pane: drop the task, which cancels it.
                self.autoscroll = None;
            } else if self.autoscroll.is_none() {
                self.arm_autoscroll(lines, cx);
            }
        }
        if !opens_terminal_link(event.modifiers.platform) {
            if self.link_hover.take().is_some() {
                cx.notify();
            }
            return;
        }
        let Some(terminal) = self.running_terminal() else {
            return;
        };
        // Same hit-test math as `on_left_mouse_down` (F-TERM-UI-02).
        let origin = terminal
            .last_bounds
            .lock()
            .map(|bounds| bounds.origin)
            .unwrap_or_default();
        let cell_width = terminal
            .last_cell_width
            .lock()
            .map(f32::from)
            .unwrap_or(8.0);
        let cell_height = terminal
            .last_cell_height
            .lock()
            .map(f32::from)
            .unwrap_or(f32::from(LINE_HEIGHT));
        let (row, column) = link_router::resolve_click_cell(
            f32::from(event.position.x),
            f32::from(event.position.y),
            f32::from(origin.x),
            f32::from(origin.y),
            cell_width,
            cell_height,
        );
        let uri = terminal.link_at(row, column);
        let next = uri.map(|uri| LinkHover {
            position: event.position,
            uri,
        });
        let changed = match (&self.link_hover, &next) {
            (Some(old), Some(new)) => old.uri != new.uri || old.position != new.position,
            (None, None) => false,
            _ => true,
        };
        self.link_hover = next;
        if changed {
            cx.notify();
        }
    }

    /// #43: left release has no Sirio gesture; it belongs to the guest
    /// whenever tracking was requested.
    /// #259: scrolls one line per tick while a drag is held outside the pane,
    /// extending the selection to the edge it is leaving through.
    ///
    /// Scrolling moves what a viewport row means, so the anchor is shifted by
    /// the opposite amount each tick and stays pinned to the same text. That
    /// holds because this is the only thing scrolling during a drag; a wheel
    /// turn mid-drag would still slip, which is a smaller wrong than an anchor
    /// that walks up the screen on its own.
    fn arm_autoscroll(&mut self, lines: isize, cx: &mut gpui::Context<Self>) {
        const AUTOSCROLL_TICK: Duration = Duration::from_millis(60);
        self.autoscroll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(AUTOSCROLL_TICK).await;
                let keep_going = this.update(cx, |view, cx| {
                    let Some(terminal) = view.running_terminal() else {
                        return false;
                    };
                    let Some(anchor) = *terminal.selection_anchor.lock() else {
                        return false;
                    };
                    terminal.scroll_display(SirioScroll::Lines(lines));
                    // The anchor is in viewport coordinates, and the viewport
                    // just moved under it.
                    let shifted = if lines < 0 {
                        (anchor.0.saturating_add(1), anchor.1)
                    } else {
                        (anchor.0.saturating_sub(1), anchor.1)
                    };
                    *terminal.selection_anchor.lock() = Some(shifted);
                    // Extend to the edge the drag is leaving through.
                    let rows = terminal
                        .last_size
                        .lock()
                        .map(|(_, rows)| rows as usize)
                        .unwrap_or(0);
                    let edge_row = if lines < 0 { 0 } else { rows.saturating_sub(1) };
                    let focus = (edge_row, if lines < 0 { 0 } else { usize::MAX });
                    let range = SelectedRange::between(shifted, focus);
                    *terminal.selection.lock() = (!range.is_empty()).then_some(range);
                    cx.notify();
                    true
                });
                if !matches!(keep_going, Ok(true)) {
                    break;
                }
            }
        }));
    }

    fn on_left_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        _: &mut gpui::Context<Self>,
    ) {
        // #259: the gesture ends; the selection it produced stays until the
        // next press clears it, and the autoscroll stops with the button.
        self.autoscroll = None;
        if let Some(terminal) = self.running_terminal() {
            *terminal.selection_anchor.lock() = None;
        }
        if let Some(terminal) = self.running_terminal() {
            Self::encode_mouse(
                terminal,
                mouse::Action::Release,
                Some(mouse::Button::Left),
                None,
                event.position,
                event.modifiers,
            );
        }
    }

    /// #43: middle button has no Sirio gesture at all; press and release
    /// belong to the guest whenever tracking was requested.
    fn on_middle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        _: &mut gpui::Context<Self>,
    ) {
        if let Some(terminal) = self.running_terminal() {
            Self::encode_mouse(
                terminal,
                mouse::Action::Press,
                Some(mouse::Button::Middle),
                Some(MouseButton::Middle),
                event.position,
                event.modifiers,
            );
        }
    }

    fn on_middle_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        _: &mut gpui::Context<Self>,
    ) {
        if let Some(terminal) = self.running_terminal() {
            Self::encode_mouse(
                terminal,
                mouse::Action::Release,
                Some(mouse::Button::Middle),
                None,
                event.position,
                event.modifiers,
            );
        }
    }

    /// #43: wheel ticks encode Button Four/Five presses while the guest has
    /// tracking on; otherwise the wheel moves Sirio's local viewport.
    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(terminal) = self.running_terminal() else {
            return;
        };
        // GPUI convention (matches Zed's editor): positive y scrolls toward
        // the top, i.e. the wheel rolled up.
        let delta_y = match event.delta {
            ScrollDelta::Pixels(pixels) => f32::from(pixels.y),
            ScrollDelta::Lines(lines) => lines.y,
        };
        if terminal.mouse_tracking.load(Ordering::Relaxed) {
            let button = if delta_y > 0.0 {
                mouse::Button::Four
            } else if delta_y < 0.0 {
                mouse::Button::Five
            } else {
                return;
            };
            Self::encode_mouse(
                terminal,
                mouse::Action::Press,
                Some(button),
                None,
                event.position,
                event.modifiers,
            );
        } else if let Some(lines) = scroll_lines_from_wheel_delta(
            event.delta,
            px(
                terminal
                    .last_cell_height
                    .lock()
                    .map(f32::from)
                    .unwrap_or(f32::from(LINE_HEIGHT)),
            ),
        ) {
            terminal.scroll_display(SirioScroll::Lines(lines));
            cx.notify();
        }
    }

    fn emit_prompt(&mut self, action: TerminalPromptAction, cx: &mut gpui::Context<Self>) {
        cx.emit(TerminalPromptEvent {
            target: self.identity.clone(),
            action,
        });
    }

    fn copy_text(&self, cx: &mut gpui::Context<Self>, _include_context: bool) {
        let Some(terminal) = self.running_terminal() else {
            return;
        };
        // #259: Copy takes the selection, and does nothing without one. It
        // used to copy the entire scrollback, because there was no selection
        // to take -- hundreds of lines where the user had highlighted a word.
        //
        // `capture_scrollback` stays: session save is its other consumer
        // (`sirio/src/main.rs`), and that one really does want everything.
        let Some(text) = terminal.selected_text() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
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
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text())
                    && let Some(terminal) = self.running_terminal()
                {
                    terminal.paste(text.into_bytes());
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
            | TerminalContextAction::RestartTerminal
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
        let is_clipboard_shortcut = {
            #[cfg(target_os = "macos")]
            {
                event.keystroke.modifiers.platform
            }
            #[cfg(not(target_os = "macos"))]
            {
                event.keystroke.modifiers.control && event.keystroke.modifiers.shift
            }
        };
        if is_clipboard_shortcut {
            let action = match key.as_str() {
                "c" => Some(TerminalContextAction::Copy),
                "v" => Some(TerminalContextAction::Paste),
                _ => None,
            };
            if let Some(action) = action {
                self.handle_context_action(action, window, cx);
                return;
            }
        }
        if let TerminalState::Running(terminal) = &self.terminal {
            let scroll = (!event.keystroke.modifiers.modified())
                .then_some(match key.as_str() {
                    "pageup" | "page_up" => Some(SirioScroll::PageUp),
                    "pagedown" | "page_down" => Some(SirioScroll::PageDown),
                    _ => None,
                })
                .flatten();
            if let Some(scroll) = scroll {
                terminal.scroll_display(scroll);
            } else if let Some(input) = key_input(event) {
                terminal.write_key(input);
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
    line_height: Pixels,
    backgrounds: Vec<PaintQuad>,
    lines: Vec<(ShapedLine, gpui::Point<Pixels>)>,
    cursor: Option<PaintQuad>,
    /// #302 R3.1/R3.2: Kitty placements for this frame, bucketed into the
    /// three z-bands Kitty defines (`All` means "no filter", so never a
    /// fourth list — `set_layer` did the bucketing on the owner thread) and
    /// drained at the three insertion points of the existing paint order:
    /// `BelowBg` before `backgrounds`, `BelowText` between `backgrounds` and
    /// `lines`, `AboveText` after `lines`.
    kitty_below_bg: Vec<(Bounds<Pixels>, Arc<RenderImage>)>,
    kitty_below_text: Vec<(Bounds<Pixels>, Arc<RenderImage>)>,
    kitty_above_text: Vec<(Bounds<Pixels>, Arc<RenderImage>)>,
    /// #301 R2.6: placements whose image the pane refused to decode, painted
    /// as a visible placeholder so a dropped image is never
    /// indistinguishable from a missing renderer.
    kitty_refused: Vec<Bounds<Pixels>>,
    /// #308 R4.5: images whose cache entry died this frame (replaced,
    /// deleted, evicted by the atlas cap, or a closed pane's leftovers
    /// parked by [`KittyImageCache`]'s `Drop`). `paint` hands each to
    /// `Window::drop_image` AFTER this frame's own images were painted, so
    /// nothing still on screen is released under its own feet (R4.7).
    kitty_dropped: Vec<Arc<RenderImage>>,
}

#[derive(Clone, Copy, PartialEq)]
struct TerminalPalette {
    background: Hsla,
    foreground: Hsla,
    cursor: Hsla,
    /// #259: the wash painted under selected cells. `ThemeColors::selection`
    /// is the token reserved for text selection, as distinct from the
    /// selected-row fill -- see its doc comment in `sirio_theme`.
    selection: Hsla,
}

/// Retained products for the terminal grid, split into two levels. The grid
/// level keeps the owner-thread `Snapshot` result keyed by the shared
/// mutation stamp. The assembly level keeps shaped lines and absolute
/// background/cursor quads keyed by `(stamp, selection, palette,
/// bounds_origin, bounds_size)`, so a pane move or selection change rebuilds
/// draw products from retained cells without another owner-thread round trip.
///
/// `TerminalHandle::current_grid` refreshes the grid level with the stamp
/// ordering required to keep the retained cells current.
#[derive(Default)]
struct GridRenderCache {
    grid_stamp: Option<u64>,
    cells: Vec<Vec<SnapshotCell>>,
    cursor: (usize, usize),
    assembly_key: Option<GridAssemblyKey>,
    lines: Vec<(ShapedLine, Point<Pixels>)>,
    background_quads: Vec<PaintQuad>,
    cursor_quad: Option<PaintQuad>,
}

#[derive(PartialEq)]
struct GridAssemblyKey {
    stamp: u64,
    selection: Option<SelectedRange>,
    palette: TerminalPalette,
    bounds_origin: Point<Pixels>,
    bounds_size: Size<Pixels>,
    font_size: Pixels,
    cell_width: Pixels,
    line_height: Pixels,
}

impl TerminalPalette {
    fn from_theme(theme: &Theme) -> Self {
        Self {
            background: theme.terminal_surface.into(),
            foreground: theme.text.into(),
            cursor: theme.text.into(),
            selection: theme.selection.into(),
        }
    }
}

struct TerminalElement {
    terminal: TerminalHandle,
    palette: TerminalPalette,
    font_size: Pixels,
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
        let line_height = line_height_for_font_size(self.font_size);
        let _perf = sirio_perf::span("TerminalElement.prepaint", 0);
        *self.terminal.last_bounds.lock() = Some(bounds);
        let terminal_font = font(sirio_theme::terminal_family());
        let font_id = window.text_system().resolve_font(&terminal_font);
        let cell_width = window
            .text_system()
            .advance(font_id, self.font_size, 'm')
            .map(|advance| advance.width)
            .unwrap_or(px(8.0));
        *self.terminal.last_cell_width.lock() = Some(cell_width);
        *self.terminal.last_cell_height.lock() = Some(line_height);
        let columns = (f32::from(bounds.size.width) / f32::from(cell_width))
            .floor()
            .max(1.0) as u16;
        let rows = (f32::from(bounds.size.height) / f32::from(line_height))
            .floor()
            .max(1.0) as u16;
        self.terminal.resize(
            columns,
            rows,
            f32::from(cell_width).round().max(1.0) as u16,
            f32::from(line_height).round().max(1.0) as u16,
        );

        // #259: read once per frame, not per cell. The range is plain numbers
        // precisely so the inner loop stays arithmetic.
        let selection = *self.terminal.selection.lock();
        let mut grid_cache = self.terminal.current_grid();
        let mutation_stamp = grid_cache
            .grid_stamp
            .expect("current_grid always returns a stamped grid");
        let assembly_key = GridAssemblyKey {
            stamp: mutation_stamp,
            selection,
            palette: self.palette,
            bounds_origin: bounds.origin,
            bounds_size: bounds.size,
            font_size: self.font_size,
            cell_width,
            line_height,
        };

        let (backgrounds, lines, cursor) =
            if grid_cache.assembly_key.as_ref() == Some(&assembly_key) {
                (
                    grid_cache.background_quads.clone(),
                    grid_cache.lines.clone(),
                    grid_cache.cursor_quad.clone(),
                )
            } else {
                self.terminal.grid_assemblies.fetch_add(1, Ordering::SeqCst);
                // Quads are per run, not per cell: a row of default background is
                // one quad, and a selection is one contiguous range in reading
                // order, so at most one wash per row.
                let row_count = grid_cache.cells.len();
                let mut backgrounds = Vec::with_capacity(row_count.saturating_mul(4));
                let mut selection_washes =
                    Vec::with_capacity(if selection.is_some() { row_count } else { 0 });
                let mut lines = Vec::with_capacity(row_count);

                for (line, cells) in grid_cache.cells.iter().enumerate() {
                    let mut text = String::with_capacity(cells.len());
                    let mut runs: Vec<TextRun> = Vec::with_capacity(cells.len());
                    let mut background_run: Option<(usize, Hsla)> = None;
                    let mut selection_start = None;
                    let run_bounds = |start_column: usize, column_count: usize| {
                        Bounds::new(
                            point(
                                bounds.origin.x + cell_width * start_column as f32,
                                bounds.origin.y + line_height * line as f32,
                            ),
                            size(cell_width * column_count as f32, line_height),
                        )
                    };

                    for (column, cell) in cells.iter().enumerate() {
                        let foreground = color_to_hsla(cell.fg, self.palette);
                        // SGR 2 faint has no gpui field; halve the alpha instead (#45).
                        let foreground = if cell.faint {
                            Hsla {
                                a: foreground.a * 0.5,
                                ..foreground
                            }
                        } else {
                            foreground
                        };
                        let weight = if cell.bold {
                            FontWeight::BOLD
                        } else {
                            FontWeight::NORMAL
                        };
                        let style = if cell.italic {
                            FontStyle::Italic
                        } else {
                            FontStyle::Normal
                        };
                        let underline = underline_style(
                            cell.underline,
                            cell.underline_color.map(|c| color_to_hsla(c, self.palette)),
                        );
                        let strikethrough = cell.strikethrough.then(StrikethroughStyle::default);
                        let len = cell.byte_len();
                        if let Some(previous) = runs.last_mut()
                            && previous.font.weight == weight
                            && previous.font.style == style
                            && previous.color == foreground
                            && previous.underline == underline
                            && previous.strikethrough == strikethrough
                        {
                            // The terminal font is unchanged across a row, so a
                            // merged run needs no new Font value at all.
                            previous.len += len;
                        } else {
                            runs.push(TextRun {
                                len,
                                color: foreground,
                                background_color: None,
                                font: Font {
                                    weight,
                                    style,
                                    ..terminal_font.clone()
                                },
                                underline,
                                strikethrough,
                            });
                        }
                        text.extend(cell.chars());

                        let background = color_to_hsla(cell.bg, self.palette);
                        match background_run {
                            Some((_start, previous)) if previous == background => {}
                            Some((start, previous)) => {
                                backgrounds.push(fill(run_bounds(start, column - start), previous));
                                background_run = Some((column, background));
                            }
                            None => background_run = Some((column, background)),
                        }

                        // #259: a selected cell is *washed*, not repainted -- a
                        // second translucent quad over the guest's own background
                        // rather than instead of it. Adjacent selected cells share
                        // one wash quad just as adjacent equal backgrounds share
                        // one base quad.
                        if selection.is_some_and(|range| range.contains(line, column)) {
                            if selection_start.is_none() {
                                selection_start = Some(column);
                            }
                        } else if let Some(start) = selection_start.take() {
                            selection_washes.push(fill(
                                run_bounds(start, column - start),
                                self.palette.selection,
                            ));
                        }
                    }

                    if let Some((start, background)) = background_run {
                        backgrounds.push(fill(run_bounds(start, cells.len() - start), background));
                    }
                    if let Some(start) = selection_start {
                        selection_washes.push(fill(
                            run_bounds(start, cells.len() - start),
                            self.palette.selection,
                        ));
                    }

                    let shaped_line =
                        window
                            .text_system()
                            .shape_line(text.into(), self.font_size, &runs, None);
                    lines.push((
                        shaped_line,
                        point(bounds.origin.x, bounds.origin.y + line_height * line as f32),
                    ));
                }

                // Paint every guest background before every translucent selection
                // wash, matching the old per-cell ordering even when a base
                // background run spans selected and unselected cells.
                backgrounds.extend(selection_washes);

                let terminal_cursor = grid_cache.cursor;
                let cursor = (terminal_cursor.0 < rows as usize
                    && terminal_cursor.1 < columns as usize)
                    .then(|| {
                        fill(
                            Bounds::new(
                                point(
                                    bounds.origin.x + cell_width * terminal_cursor.1 as f32,
                                    bounds.origin.y + line_height * terminal_cursor.0 as f32,
                                ),
                                size(cell_width, line_height),
                            ),
                            self.palette.cursor,
                        )
                    });

                grid_cache.assembly_key = Some(assembly_key);
                grid_cache.background_quads = backgrounds.clone();
                grid_cache.lines = lines.clone();
                grid_cache.cursor_quad = cursor.clone();
                (backgrounds, lines, cursor)
            };
        drop(grid_cache);

        // #301 + #302 R3.1/#308 R4.4: Kitty placements arrive pre-bucketed
        // by the emulator's own `set_layer` walks (the three z-bands; `All`
        // is not a bucket), copied as plain pixels + geometry
        // (`kitty_places`, R2.3), plus the LIVE placement set (`live`,
        // #308). Decode and cache happen HERE on the UI thread, never on
        // the PTY owner thread. The cache lives beside the pane's other
        // state (R4.3). The re-scan is GATED on the owner thread's mutation
        // stamp (R4.4): a still pane reuses the last walk — one integer
        // comparison per frame, not a channel round trip plus three walks.
        let mutation_stamp = self.terminal.mutation_stamp.load(Ordering::Relaxed);
        let mut kitty_cache = self.terminal.kitty_images.lock();
        if kitty_cache.last_stamp != mutation_stamp {
            // Never hold the cache lock across the blocking round trip.
            drop(kitty_cache);
            let buckets = self.terminal.kitty_places();
            kitty_cache = self.terminal.kitty_images.lock();
            kitty_cache.last_stamp = mutation_stamp;
            kitty_cache.last_buckets = buckets;
        }
        let kitty_buckets = std::mem::take(&mut kitty_cache.last_buckets);
        let tick = kitty_cache.begin_frame();
        let mut kitty_below_bg = Vec::new();
        let mut kitty_below_text = Vec::new();
        let mut kitty_above_text = Vec::new();
        let mut kitty_refused = Vec::new();
        if self.terminal.kitty_decode_failed.load(Ordering::Acquire) {
            kitty_refused.push(kitty_refused_indicator_bounds(bounds));
        }
        for (layer_bucket, bucket) in [
            (&mut kitty_below_bg, &kitty_buckets.below_bg),
            (&mut kitty_below_text, &kitty_buckets.below_text),
            (&mut kitty_above_text, &kitty_buckets.above_text),
        ] {
            for place in bucket {
                // R3.3: keep the untruncated placement geometry — the row
                // can be negative (scrolled partly off the top) — and let
                // `Window::paint_image` derive the clipped region from the
                // pane bounds at paint time. Clamping here would squash.
                let image_bounds = kitty_image_bounds(bounds, cell_width, line_height, place);
                match kitty_cache.get_or_decode(place, tick) {
                    Some(image) => layer_bucket.push((image_bounds, image)),
                    None => kitty_refused.push(image_bounds),
                }
            }
        }
        // #308 R4.5: reconcile the cache against the emulator AFTER this
        // frame's decodes: a replaced image's old generation and every image
        // that left the grid (guest delete, store eviction) are dead, and
        // over the atlas cap (R4.6) the overflow is evicted
        // least-recently-painted first (R4.7 — never a key painted this
        // frame, so nothing on screen disappears silently, #303 R2.6).
        // Scroll-away and scrollback trim do NOT kill placements (measured
        // against libghostty-vt 0.2.1), so those keep their entries until
        // the cap forces them out.
        let mut kitty_dropped = Vec::new();
        for key in kitty_cache
            .dead_keys(&kitty_buckets.live)
            .into_iter()
            .chain(kitty_cache.lru_dead_keys(KITTY_ATLAS_IMAGE_CAP, tick))
        {
            if let Some(image) = kitty_cache.remove(&key) {
                kitty_dropped.push(image);
            }
        }
        kitty_cache.last_buckets = kitty_buckets;
        drop(kitty_cache);

        TerminalPaintState {
            line_height,
            backgrounds,
            lines,
            cursor,
            kitty_below_bg,
            kitty_below_text,
            kitty_above_text,
            kitty_refused,
            kitty_dropped,
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
            // #302 R3.2: the three insertion points in the existing paint
            // order — BelowBg before `backgrounds`, BelowText between
            // `backgrounds` and `lines`, AboveText after `lines`. No
            // interleave-by-z pass: the emulator's `set_layer` bucketing
            // already fixed each placement's relative order.
            // `paint_image` intersects the full placement bounds with the
            // pane and derives the clipped atlas sub-rect itself (R3.3); the
            // content mask above already clips to the pane. Background quads
            // under a placement still paint (R3.4), since Kitty images carry
            // alpha — suppressing the cells would show the window behind
            // wherever the image is transparent.
            for (image_bounds, image) in state.kitty_below_bg.drain(..) {
                let _ =
                    window.paint_image(bounds, image_bounds, Corners::default(), image, 0, false);
            }
            for background in state.backgrounds.drain(..) {
                window.paint_quad(background);
            }
            for (image_bounds, image) in state.kitty_below_text.drain(..) {
                let _ =
                    window.paint_image(bounds, image_bounds, Corners::default(), image, 0, false);
            }
            for (line, origin) in state.lines.drain(..) {
                let _ = line.paint(
                    origin,
                    state.line_height,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
            for (image_bounds, image) in state.kitty_above_text.drain(..) {
                let _ =
                    window.paint_image(bounds, image_bounds, Corners::default(), image, 0, false);
            }
            // R2.6: a refused format must be visible in the pane, not a
            // silent drop.
            for image_bounds in state.kitty_refused.drain(..) {
                window.paint_quad(fill(image_bounds, KITTY_REFUSED_COLOR));
            }
            // #308 R4.5: release the images that died this frame — and the
            // leftovers of a pane that closed (its cache's `Drop` parked
            // them in the graveyard) — only now, after this frame's
            // placements were already painted, so an on-screen image is
            // never released under its own feet (R4.7). `drop_image` is the
            // only atlas eviction; without it each dropped `Arc` leaks GPU
            // memory for the process lifetime.
            for image in state.kitty_dropped.drain(..) {
                let _ = window.drop_image(image);
            }
            for image in KITTY_DROPPED_IMAGES.lock().drain(..) {
                let _ = window.drop_image(image);
            }
            if let Some(cursor) = state.cursor.take() {
                window.paint_quad(cursor);
            }
        });
    }
}

impl TerminalView {
    /// How many times this view has rendered. Only a test should read it:
    /// it exists so the host can prove a cached, still pane is reused across
    /// frames rather than re-rendered.
    #[doc(hidden)]
    pub fn render_count(&self) -> u64 {
        self.render_count
    }
}

impl gpui::Render for TerminalView {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _perf = sirio_perf::span("TerminalView.render", cx.entity_id().as_u64());
        self.render_count = self.render_count.wrapping_add(1);
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
        // F-TAB-11: the disabled reason is computed from this pane's own
        // live, prepainted size (the same source `on_left_mouse_down` uses
        // for hit-testing), not from any cross-crate pane-tree state --
        // `sirio_terminal` cannot depend on the `sirio` crate that owns
        // the tab/pane tree. See `context_menu::items_with_split_availability`.
        let split_pane_size = self
            .running_terminal()
            .and_then(|terminal| terminal.last_bounds.lock().map(|bounds| bounds.size))
            .map(|size| (f32::from(size.width), f32::from(size.height)));
        let context_menu = self.context_menu.map(|position| {
            let entity = cx.entity();
            let dismiss_entity = entity.clone();
            let mut menu = div()
                .id("terminal-context-menu")
                .debug_selector(|| "terminal-context-menu".to_owned())
                .w(px(220.0))
                .p(px(6.0))
                .rounded(theme.radii.user_pill)
                .border_1()
                .border_color(theme.border)
                .bg(theme.surface_raised)
                .shadow_lg();

            let items = match split_pane_size {
                Some((width, height)) => context_menu::items_with_split_availability(
                    width,
                    height,
                    self.sole_tab_in_group,
                ),
                None => context_menu::items().to_vec(),
            };
            for (index, item) in items.iter().enumerate() {
                let action = item.action;
                let item_entity = entity.clone();
                let selector = format!("terminal-context-item-{index}");
                let debug_selector = selector.clone();
                let disabled_reason = item.disabled_reason.clone();
                let is_disabled = disabled_reason.is_some();
                menu = menu.child(
                    div()
                        .id(selector)
                        .debug_selector(move || debug_selector.clone())
                        .w_full()
                        .min_h(px(29.0))
                        .px(px(10.0))
                        .py(px(5.0))
                        .rounded(theme.radii.control)
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .text_size(theme.typography.footnote)
                        .when(is_disabled, |this| {
                            this.text_color(theme.text_faint).cursor_not_allowed()
                        })
                        .when(!is_disabled, |this| {
                            this.text_color(theme.text)
                                .hover(|style| style.bg(theme.element_hover))
                                .on_click(move |_, window, cx| {
                                    item_entity.update(cx, |terminal, cx| {
                                        terminal.handle_context_action(action, window, cx);
                                    });
                                })
                        })
                        .child(item.label)
                        .when_some(disabled_reason, |this, reason| {
                            this.child(
                                div()
                                    .debug_selector(move || {
                                        format!("terminal-context-item-{index}-reason")
                                    })
                                    .text_size(px(12.0))
                                    .text_color(theme.text_faint)
                                    .child(reason),
                            )
                        }),
                );
            }
            let menu = menu.on_mouse_down_out(move |_, _, cx| {
                dismiss_entity.update(cx, |terminal, cx| {
                    terminal.context_menu = None;
                    cx.notify();
                });
            });
            // `position` is already window-absolute (it comes straight from
            // `MouseDownEvent::position` in `open_context_menu`). A plain
            // `.absolute().left()/.top()` div resolves against this
            // container's own `.relative()` origin, which is *itself*
            // already window-absolute for every pane not flush against the
            // window's top-left corner -- double-counting the offset
            // (P129). `anchored().position(...)` takes a window coordinate
            // as-is, the same idiom already used correctly by
            // `tab_bar.rs`'s "+" menu, `right_panel.rs`, and `sidebar.rs`.
            deferred(anchored().position(position).snap_to_window().child(menu)).priority(1)
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
                            .text_size(px(15.0))
                            .text_color(theme.text)
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
                                    .bg(theme.surface_raised)
                                    .text_color(theme.text)
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
                                    .bg(theme.surface_raised)
                                    .text_color(theme.text)
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
                .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_middle_mouse_down))
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_left_mouse_up))
                .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_middle_mouse_up))
                .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
                .on_mouse_move(cx.listener(Self::on_link_hover_move))
                .on_key_down(cx.listener(Self::on_key_down))
                .on_drop::<(PathBuf, String)>(move |payload: &(PathBuf, String), _, cx| {
                    terminal_entity.update(cx, |terminal, cx| {
                        terminal.receive_diff_drop(payload.clone(), cx);
                    });
                })
                .on_drop::<PathBuf>(move |path: &PathBuf, window, cx| {
                    file_drop_entity.update(cx, |terminal, cx| {
                        terminal.receive_file_drop(vec![path.clone()], window, cx);
                    });
                })
                // F-TERM-PTY-06: the in-app `on_drop::<PathBuf>` above only
                // ever catches GPUI's own typed drag payload (a Files-panel
                // row dragged within the app). A real OS-level file-manager
                // drag arrives as `gpui::ExternalPaths` (GPUI's XDND
                // payload), which can carry more than one path in a single
                // drop; both funnel into the same quoted insertion.
                .on_drop::<gpui::ExternalPaths>(move |paths: &gpui::ExternalPaths, window, cx| {
                    external_drop_entity.update(cx, |terminal, cx| {
                        terminal.receive_file_drop(paths.paths().to_vec(), window, cx);
                    });
                })
                .child(TerminalElement {
                    terminal: terminal.clone(),
                    palette,
                    font_size: self.font_size,
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
                            .bg(theme.surface_raised)
                            .text_size(theme.typography.caption2)
                            .text_color(theme.text)
                            .child(format!("Dropped diff: {path}")),
                    )
                })
                // F-TERM-03: running wins while running, exactly as Swift's
                // `statusChip` orders its four branches (running first, then
                // the exit variants) — mirrored here as an if/else-if
                // instead of two independent `.when`s so the two pills can
                // never both paint. `is_command_running()` is already `false`
                // once `exit_status` is recorded (the PTY is gone by then),
                // but the explicit `else` keeps that ordering true by
                // construction rather than by relying on the callee.
                .map(|this| {
                    if self.is_command_running() {
                        this.child(
                            div()
                                .absolute()
                                .left(px(8.0))
                                .bottom(px(8.0))
                                .px(px(8.0))
                                .py(px(4.0))
                                .bg(theme.surface_raised)
                                .text_size(px(12.0))
                                .text_color(theme.warning)
                                .child("Running"),
                        )
                    } else if let Some(label) = self.exit_status.map(TerminalExitStatus::label) {
                        this.child(
                            div()
                                .absolute()
                                .left(px(8.0))
                                .bottom(px(8.0))
                                .px(px(8.0))
                                .py(px(4.0))
                                .bg(theme.surface_raised)
                                .text_size(px(12.0))
                                .text_color(theme.text_muted)
                                .child(label),
                        )
                    } else {
                        this
                    }
                })
                // #41: the link-hover tooltip — dumb monospace text of the
                // target URI near the pointer, theme colors, no interaction.
                .when_some(self.link_hover.clone(), |this, hover| {
                    this.child(
                        deferred(
                            anchored().position(hover.position).snap_to_window().child(
                                div()
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .max_w(px(560.0))
                                    .rounded(theme.radii.control)
                                    .bg(theme.surface_raised)
                                    .border_1()
                                    .border_color(theme.border)
                                    .font_family(sirio_theme::terminal_family())
                                    .text_size(theme.typography.caption2)
                                    .text_color(theme.text)
                                    .child(hover.uri),
                            ),
                        )
                        .priority(1),
                    )
                })
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
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.warning)
                            .child("Terminal failed to start"),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_size(px(13.0))
                            .text_color(theme.text_muted)
                            .child(message.clone()),
                    )
                    .child(
                        div()
                            .id("terminal-retry")
                            .debug_selector(|| "terminal-retry".to_string())
                            .px(px(14.0))
                            .py(px(6.0))
                            .rounded(px(6.0))
                            .bg(theme.surface_raised)
                            .text_size(px(13.0))
                            .text_color(theme.text)
                            .hover(|style| style.bg(theme.element_hover))
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

/// Maps the emulator's five underline styles onto gpui's ceiling: gpui's
/// UnderlineStyle carries thickness/color/wavy only, so double, dotted and
/// dashed DEGRADE TO SINGLE and curly becomes wavy — #31 decided degrade,
/// never drop; unknown future variants also degrade to single. Custom quad
/// painting is deliberately deferred (the element already paints quads for
/// backgrounds).
fn underline_style(underline: Underline, color: Option<Hsla>) -> Option<UnderlineStyle> {
    match underline {
        Underline::None => None,
        Underline::Curly => Some(UnderlineStyle {
            wavy: true,
            color,
            ..UnderlineStyle::default()
        }),
        _ => Some(UnderlineStyle {
            color,
            ..UnderlineStyle::default()
        }),
    }
}

fn color_to_hsla(color: SirioColor, palette: TerminalPalette) -> Hsla {
    match color {
        SirioColor::Rgb(r, g, b) => rgb_to_hsla((r, g, b)),
        SirioColor::Indexed(index) => rgb_to_hsla(indexed_color(index)),
        SirioColor::Named(name) => named_color(name, palette),
    }
}

/// The subset of the 16+8 ANSI palette plus the four special colors the
/// renderer still consumes. Own enum — no emulator type may leak (#44).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NamedColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Foreground,
    #[allow(dead_code)] // mirrors the emulator's named-colour set; not yet mapped
    BrightForeground,
    Background,
    #[allow(dead_code)] // mirrors the emulator's named-colour set; not yet mapped
    Cursor,
}

/// A resolved cell color: concrete RGB, a 256-palette index, or a special
/// named slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SirioColor {
    Rgb(u8, u8, u8),
    // libghostty resolves palette entries to RGB before we see them; kept so
    // the 256-cube plumbing (and its test) survives into stage 2 (#45).
    #[cfg_attr(not(test), allow(dead_code))]
    Indexed(u8),
    Named(NamedColor),
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

fn key_input(event: &KeyDownEvent) -> Option<KeyInput> {
    let name = event.keystroke.key.to_ascii_lowercase();
    let modifiers = event.keystroke.modifiers;
    if modifiers.platform || matches!(name.as_str(), "shift" | "control" | "alt") {
        return None;
    }

    let mapped = named_key(&name).or_else(|| {
        let mut chars = name.chars();
        let character = chars.next()?;
        chars.next().is_none().then(|| character_key(character))
    });
    let utf8 = event
        .keystroke
        .key_char
        .clone()
        .or_else(|| (name.chars().count() == 1).then(|| event.keystroke.key.clone()));
    let unshifted_codepoint = (name.chars().count() == 1)
        .then(|| name.chars().next())
        .flatten()
        .or_else(|| utf8.as_deref()?.chars().next());

    let mut mods = key::Mods::empty();
    if modifiers.shift {
        mods |= key::Mods::SHIFT;
    }
    if modifiers.alt {
        mods |= key::Mods::ALT;
    }
    if modifiers.control {
        mods |= key::Mods::CTRL;
    }

    Some(KeyInput {
        // GPUI currently delivers key-down only. Key release remains out of
        // scope until a hosted CLI requests Kitty REPORT_EVENTS.
        action: if event.is_held {
            key::Action::Repeat
        } else {
            key::Action::Press
        },
        key: mapped.unwrap_or(key::Key::Unidentified),
        mods,
        consumed_mods: if modifiers.shift && utf8.is_some() {
            key::Mods::SHIFT
        } else {
            key::Mods::empty()
        },
        utf8,
        unshifted_codepoint,
    })
}

fn named_key(name: &str) -> Option<key::Key> {
    Some(match name {
        "enter" | "return" => key::Key::Enter,
        "backspace" => key::Key::Backspace,
        "tab" => key::Key::Tab,
        "escape" => key::Key::Escape,
        "up" => key::Key::ArrowUp,
        "down" => key::Key::ArrowDown,
        "right" => key::Key::ArrowRight,
        "left" => key::Key::ArrowLeft,
        "home" => key::Key::Home,
        "end" => key::Key::End,
        "insert" => key::Key::Insert,
        "delete" => key::Key::Delete,
        "pageup" | "page_up" => key::Key::PageUp,
        "pagedown" | "page_down" => key::Key::PageDown,
        "space" => key::Key::Space,
        "f1" => key::Key::F1,
        "f2" => key::Key::F2,
        "f3" => key::Key::F3,
        "f4" => key::Key::F4,
        "f5" => key::Key::F5,
        "f6" => key::Key::F6,
        "f7" => key::Key::F7,
        "f8" => key::Key::F8,
        "f9" => key::Key::F9,
        "f10" => key::Key::F10,
        "f11" => key::Key::F11,
        "f12" => key::Key::F12,
        _ => return None,
    })
}

fn character_key(character: char) -> key::Key {
    match character {
        'a' => key::Key::A,
        'b' => key::Key::B,
        'c' => key::Key::C,
        'd' => key::Key::D,
        'e' => key::Key::E,
        'f' => key::Key::F,
        'g' => key::Key::G,
        'h' => key::Key::H,
        'i' => key::Key::I,
        'j' => key::Key::J,
        'k' => key::Key::K,
        'l' => key::Key::L,
        'm' => key::Key::M,
        'n' => key::Key::N,
        'o' => key::Key::O,
        'p' => key::Key::P,
        'q' => key::Key::Q,
        'r' => key::Key::R,
        's' => key::Key::S,
        't' => key::Key::T,
        'u' => key::Key::U,
        'v' => key::Key::V,
        'w' => key::Key::W,
        'x' => key::Key::X,
        'y' => key::Key::Y,
        'z' => key::Key::Z,
        '0' => key::Key::Digit0,
        '1' => key::Key::Digit1,
        '2' => key::Key::Digit2,
        '3' => key::Key::Digit3,
        '4' => key::Key::Digit4,
        '5' => key::Key::Digit5,
        '6' => key::Key::Digit6,
        '7' => key::Key::Digit7,
        '8' => key::Key::Digit8,
        '9' => key::Key::Digit9,
        '`' => key::Key::Backquote,
        '\\' => key::Key::Backslash,
        '[' => key::Key::BracketLeft,
        ']' => key::Key::BracketRight,
        ',' => key::Key::Comma,
        '=' => key::Key::Equal,
        '-' => key::Key::Minus,
        '.' => key::Key::Period,
        '\'' => key::Key::Quote,
        ';' => key::Key::Semicolon,
        '/' => key::Key::Slash,
        _ => key::Key::Unidentified,
    }
}

/// #259: the selected text, straight from the emulator.
///
/// Viewport coordinates in, formatted text out. `unwrap` rejoins soft-wrapped
/// lines and `trim` drops the run of blanks a terminal pads every row with --
/// both reasons to ask libghostty-vt rather than slice a cell snapshot, which
/// would copy a screenful of trailing spaces.
fn selection_text(terminal: &Terminal<'_, '_>, range: SelectedRange) -> Option<String> {
    let point = |cell: (usize, usize)| {
        GhosttyPoint::Viewport(PointCoordinate {
            x: u16::try_from(cell.1).unwrap_or(u16::MAX),
            y: u32::try_from(cell.0).unwrap_or(u32::MAX),
        })
    };
    let start = terminal.grid_ref(point(range.start)).ok()?;
    let end = terminal.grid_ref(point(range.end)).ok()?;
    let selection = Selection::new(start, end, false);
    let options = FormatOptions::new()
        .with_selection(&selection)
        .with_unwrap(true)
        .with_trim(true);
    let bytes = terminal.format_selection_alloc(None, options).ok()??;
    Some(String::from_utf8_lossy(bytes.as_ref()).into_owned())
}

/// #259: the range a double or triple click selects.
///
/// `select_word` and `select_line` are the emulator's own, so wide characters,
/// semantic prompt boundaries and soft-wrapped lines behave the way they do in
/// Ghostty rather than the way a hand-written boundary scan would. The result
/// comes back as a `Selection` over `GridRef`s, which
/// `Terminal::point_from_grid_ref` converts to the viewport coordinates the
/// paint loop understands -- the inverse of the lookup that produced them.
///
/// `None` when the click lands somewhere with no word (blank cells) or the
/// range cannot be expressed in the viewport, which is what a click into empty
/// space should do: nothing.
fn click_selection_range(
    terminal: &Terminal<'_, '_>,
    cell: (usize, usize),
    kind: ClickSelection,
) -> Option<SelectedRange> {
    let point = GhosttyPoint::Viewport(PointCoordinate {
        x: u16::try_from(cell.1).unwrap_or(u16::MAX),
        y: u32::try_from(cell.0).unwrap_or(u32::MAX),
    });
    let grid_ref = terminal.grid_ref(point).ok()?;
    let selection = match kind {
        ClickSelection::Word => terminal
            .select_word(SelectWordOptions::new(grid_ref))
            .ok()??,
        ClickSelection::Line => terminal
            .select_line(SelectLineOptions::new(grid_ref))
            .ok()??,
    };
    let viewport = |grid_ref: &GridRef<'_>| {
        terminal
            .point_from_grid_ref(grid_ref, PointSpace::Viewport)
            .ok()
            .flatten()
            .map(|point| (point.y as usize, point.x as usize))
    };
    let start = viewport(&selection.start())?;
    let end = viewport(&selection.end())?;
    Some(SelectedRange::between(start, end))
}

fn encode_key_input(
    terminal: &Terminal<'_, '_>,
    encoder: &mut key::Encoder<'_>,
    input: KeyInput,
) -> std::result::Result<Vec<u8>, Error> {
    let mut event = key::Event::new()?;
    event
        .set_action(input.action)
        .set_key(input.key)
        .set_mods(input.mods)
        .set_consumed_mods(input.consumed_mods)
        .set_utf8(input.utf8);
    if let Some(codepoint) = input.unshifted_codepoint {
        event.set_unshifted_codepoint(codepoint);
    }

    encoder
        .set_options_from_terminal(terminal)
        .set_macos_option_as_alt(key::OptionAsAlt::True);
    let mut bytes = Vec::new();
    encoder.encode_to_vec(&event, &mut bytes)?;
    Ok(bytes)
}

fn encode_mouse_input(
    terminal: &Terminal<'_, '_>,
    state: &mut MouseEncoderState<'_>,
    input: MouseInput,
) -> std::result::Result<Vec<u8>, Error> {
    // Keep TRACKING MODE and FORMAT synchronized with guest state. The mode
    // snapshot avoids resetting libghostty-vt 0.2.1's internal last-cell state
    // when nothing changed; the setter itself clears that state.
    let terminal_modes = [
        terminal.mode(Mode::X10_MOUSE)?,
        terminal.mode(Mode::NORMAL_MOUSE)?,
        terminal.mode(Mode::BUTTON_MOUSE)?,
        terminal.mode(Mode::ANY_MOUSE)?,
        terminal.mode(Mode::UTF8_MOUSE)?,
        terminal.mode(Mode::SGR_MOUSE)?,
        terminal.mode(Mode::URXVT_MOUSE)?,
        terminal.mode(Mode::SGR_PIXELS_MOUSE)?,
    ];
    if state.terminal_modes != Some(terminal_modes) {
        state.encoder.set_options_from_terminal(terminal);
        state.terminal_modes = Some(terminal_modes);
    }

    let size = mouse::EncoderSize {
        screen_width: input.pane_width as u32,
        screen_height: input.pane_height as u32,
        cell_width: input.cell_width.round().max(1.0) as u32,
        cell_height: input.cell_height.round().max(1.0) as u32,
        padding_top: 0,
        padding_bottom: 0,
        padding_right: 0,
        padding_left: 0,
    };
    // Size is likewise current for every event, but only reapplied when the
    // pane geometry changed because this setter also clears last-cell state.
    if state.size != Some(size) {
        state.encoder.set_size(size);
        state.size = Some(size);
    }
    state
        .encoder
        // Per-cell motion dedup inside the encoder — GPUI fires mouse-move per
        // pixel; the encoder drops same-cell motion for us.
        .set_track_last_cell(true)
        .set_any_button_pressed(input.any_button_pressed);
    let mut event = mouse::Event::new()?;
    event
        .set_action(input.action)
        .set_button(input.button)
        .set_mods(input.mods)
        .set_position(mouse::Position {
            x: input.x,
            y: input.y,
        });
    let mut bytes = Vec::new();
    state.encoder.encode_to_vec(&event, &mut bytes)?;
    Ok(bytes)
}

fn gpui_to_ghostty_button(button: MouseButton) -> Option<mouse::Button> {
    Some(match button {
        MouseButton::Left => mouse::Button::Left,
        MouseButton::Middle => mouse::Button::Middle,
        MouseButton::Right => mouse::Button::Right,
        MouseButton::Navigate(_) => return None,
    })
}

/// Converts a GPUI mouse event into a guest-bound [`MouseInput`], deciding
/// Sirio-gesture vs encode BEFORE any encoding (#43):
///
/// * Shift bypasses reporting — the standard terminal override, so any
///   shift-held event stays Sirio's no matter what tracking the guest
///   requested.
/// * The right button stays Sirio's (the context menu); it is never encoded.
///
/// `position` arrives in WINDOW space like every other handler; the pane
/// origin is undone here with the same `last_bounds` math as
/// `on_left_mouse_down` (F-TERM-UI-02).
#[allow(clippy::too_many_arguments)]
fn mouse_input(
    action: mouse::Action,
    button: Option<mouse::Button>,
    pressed_button: Option<MouseButton>,
    position: Point<Pixels>,
    modifiers: gpui::Modifiers,
    bounds: Bounds<Pixels>,
    cell_width: Pixels,
    cell_height: Pixels,
) -> Option<MouseInput> {
    if modifiers.shift {
        return None;
    }
    if button == Some(mouse::Button::Right) || pressed_button == Some(MouseButton::Right) {
        return None;
    }
    // Shift never reaches the encoder (it bypasses reporting above), so only
    // alt/ctrl are reportable.
    let mut mods = key::Mods::empty();
    if modifiers.alt {
        mods |= key::Mods::ALT;
    }
    if modifiers.control {
        mods |= key::Mods::CTRL;
    }
    Some(MouseInput {
        action,
        button,
        mods,
        any_button_pressed: pressed_button.is_some(),
        x: (position.x - bounds.origin.x).into(),
        y: (position.y - bounds.origin.y).into(),
        pane_width: bounds.size.width.into(),
        pane_height: bounds.size.height.into(),
        cell_width: cell_width.into(),
        cell_height: cell_height.into(),
    })
}

/// The deterministic PTY child the Windows arm of the real-PTY tests runs.
/// Its own docstring is the reference for every mode named below.
#[cfg(all(test, not(unix)))]
const PTY_FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/pty_fixture.py");

/// The child a real-PTY test spawns, chosen per platform.
///
/// On unix this stays the real `/bin/sh -c <script>` it has always been,
/// byte for byte. That is deliberate: macOS is the reference release
/// platform, and the tests named `real_pty_*` earn the name by driving a
/// genuine POSIX shell through a genuine PTY — swapping the shell out
/// everywhere would weaken them on exactly the platform that gates a
/// release.
///
/// Windows has no `/bin/sh`, no `sleep`, no `cat` and no `printf`, so the
/// same test drives `tests/fixtures/pty_fixture.py` under `python3`. The
/// shell scripts here only ever ask for two things — stay alive, or emit
/// exact bytes — and the fixture's argv modes cover both. Both halves are
/// named at every call site so the pairing stays visible and reviewable.
#[cfg(test)]
fn pty_fixture_shell(unix_script: &str, windows_fixture_args: &[&str]) -> TerminalShell {
    #[cfg(unix)]
    {
        let _ = windows_fixture_args;
        TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), unix_script.to_string()],
        }
    }
    #[cfg(not(unix))]
    {
        let _ = unix_script;
        TerminalShell::WithArguments {
            program: "python3".to_string(),
            args: std::iter::once(PTY_FIXTURE.to_string())
                .chain(
                    windows_fixture_args
                        .iter()
                        .map(|argument| (*argument).to_string()),
                )
                .collect(),
        }
    }
}

/// A child that puts a prompt on the grid, goes quiet, and speaks again as
/// soon as something is typed at it.
///
/// The drawn tests that use this spawn an interactive `/bin/sh -i`, and lean
/// on exactly two of its behaviours: `settled_drawn_terminal` waits for
/// non-empty scrollback that then stops changing, and the retained-grid
/// tests type at the pane and require output to come back. None of them
/// reads the *result* of a command, so the Windows stand-in prompts and
/// echoes without running anything.
#[cfg(test)]
fn interactive_prompt_shell() -> TerminalShell {
    #[cfg(unix)]
    {
        TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec!["-i".to_string()],
        }
    }
    #[cfg(not(unix))]
    {
        TerminalShell::WithArguments {
            program: "python3".to_string(),
            args: vec![PTY_FIXTURE.to_string(), "prompt".to_string()],
        }
    }
}

/// A child that echoes back whatever is written to the PTY, and nothing
/// else — the tightest echo path there is, with no prompt and no timers of
/// its own.
///
/// On unix that is `cat` on a canonical tty: the line discipline echoes the
/// bytes. A ConPTY only echoes during a cooked read, so the fixture's `cat`
/// mode switches Windows input to raw VT mode and does the echo itself,
/// which is what makes a single keystroke — or a paste with no trailing
/// newline — come straight back.
#[cfg(test)]
fn echo_child_shell() -> TerminalShell {
    #[cfg(unix)]
    {
        TerminalShell::WithArguments {
            program: "/bin/cat".to_string(),
            args: Vec::new(),
        }
    }
    #[cfg(not(unix))]
    {
        TerminalShell::WithArguments {
            program: "python3".to_string(),
            args: vec![PTY_FIXTURE.to_string(), "cat".to_string()],
        }
    }
}

/// A child that reports the PTY's size back through the PTY when something
/// is typed at it.
///
/// On unix that is a real interactive `/bin/sh` answering a real `stty
/// size`. Windows has neither, so the fixture's `size` mode answers any
/// input with `<rows> <columns>` read from its own console — the same
/// observable line, produced by the same round trip through the PTY.
#[cfg(test)]
fn size_probe_shell() -> TerminalShell {
    #[cfg(unix)]
    {
        TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec!["-i".to_string()],
        }
    }
    #[cfg(not(unix))]
    {
        TerminalShell::WithArguments {
            program: "python3".to_string(),
            args: vec![PTY_FIXTURE.to_string(), "size".to_string()],
        }
    }
}

/// A program that is not a shell, printing its own argv and exiting.
///
/// `/bin/echo` on unix; the fixture's `echo` mode on Windows, which is just
/// as much "not a shell" — the point of the test using it is that nothing
/// interposes a shell between the PTY and the named program.
#[cfg(test)]
fn argv_probe_shell(marker: &str) -> TerminalShell {
    #[cfg(unix)]
    {
        TerminalShell::WithArguments {
            program: "/bin/echo".to_string(),
            args: vec![marker.to_string()],
        }
    }
    #[cfg(not(unix))]
    {
        TerminalShell::WithArguments {
            program: "python3".to_string(),
            args: vec![
                PTY_FIXTURE.to_string(),
                "echo".to_string(),
                marker.to_string(),
            ],
        }
    }
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
            .flat_map(|row| row.iter().flat_map(|cell| cell.chars()))
            .collect()
    }

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    fn test_working_directory(name: &str) -> PathBuf {
        let serial = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "sirio-terminal-test-{name}-{}-{serial}",
            std::process::id()
        ))
    }

    /// #259: autoscroll fires only outside the pane, and by one line.
    ///
    /// Inside the pane it must be exactly zero -- a drag that wanders within
    /// the viewport should never move the view under the user. One line per
    /// tick rather than a rate proportional to how far past the edge the
    /// pointer is: a terminal drag aims at a line, and acceleration is what
    /// makes autoscroll overshoot it.
    #[test]
    fn autoscroll_runs_only_near_the_panes_edges() {
        // Comfortably inside: nothing moves.
        assert_eq!(TerminalView::autoscroll_lines(250.0, 100.0, 400.0, LINE_HEIGHT), 0);
        assert_eq!(TerminalView::autoscroll_lines(150.0, 100.0, 400.0, LINE_HEIGHT), 0);
        assert_eq!(TerminalView::autoscroll_lines(350.0, 100.0, 400.0, LINE_HEIGHT), 0);
        // Inside but within a row of the edge -- the case that matters, and
        // the one an "outside the bounds" rule could never see, because GPUI
        // stops delivering moves there.
        assert_eq!(TerminalView::autoscroll_lines(105.0, 100.0, 400.0, LINE_HEIGHT), -1);
        assert_eq!(TerminalView::autoscroll_lines(395.0, 100.0, 400.0, LINE_HEIGHT), 1);
        // Exactly on each edge, and beyond.
        assert_eq!(TerminalView::autoscroll_lines(100.0, 100.0, 400.0, LINE_HEIGHT), -1);
        assert_eq!(TerminalView::autoscroll_lines(400.0, 100.0, 400.0, LINE_HEIGHT), 1);
        assert_eq!(TerminalView::autoscroll_lines(-500.0, 100.0, 400.0, LINE_HEIGHT), -1);
        assert_eq!(TerminalView::autoscroll_lines(5000.0, 100.0, 400.0, LINE_HEIGHT), 1);
    }

    #[test]
    fn wheel_delta_maps_to_terminal_scroll_lines() {
        assert_eq!(
            scroll_lines_from_wheel_delta(ScrollDelta::Lines(point(0.0, 3.0)), LINE_HEIGHT),
            Some(-3)
        );
        assert_eq!(
            scroll_lines_from_wheel_delta(
                ScrollDelta::Pixels(point(px(0.0), px(36.0))),
                LINE_HEIGHT,
            ),
            Some(-2)
        );
        assert_eq!(
            scroll_lines_from_wheel_delta(
                ScrollDelta::Pixels(point(px(0.0), px(-18.0))),
                LINE_HEIGHT,
            ),
            Some(1)
        );
        assert_eq!(
            scroll_lines_from_wheel_delta(ScrollDelta::Lines(point(0.0, 0.0)), LINE_HEIGHT),
            None
        );
    }

    /// #259: how many clicks mean what.
    ///
    /// A single click anchors a drag and selects nothing -- selecting the cell
    /// under the pointer would make every click leave a one-cell highlight.
    /// Past three, a terminal user expects the line again, not a new mode.
    #[test]
    fn click_count_maps_to_word_then_line() {
        assert_eq!(ClickSelection::for_click_count(0), None);
        assert_eq!(ClickSelection::for_click_count(1), None);
        assert_eq!(
            ClickSelection::for_click_count(2),
            Some(ClickSelection::Word)
        );
        assert_eq!(
            ClickSelection::for_click_count(3),
            Some(ClickSelection::Line)
        );
        assert_eq!(
            ClickSelection::for_click_count(4),
            Some(ClickSelection::Line)
        );
    }

    /// #259: who owns a left drag.
    ///
    /// While the guest tracks the mouse it owns bare drags -- a TUI's own
    /// selection, its scrollbars, its panes -- and `Shift` takes one back for
    /// the host. With tracking off there is nothing to take it from, so a bare
    /// drag selects. This is the xterm/alacritty/iTerm2/Ghostty convention.
    #[test]
    fn shift_takes_a_drag_back_from_a_mouse_tracking_guest() {
        // Guest tracking: it owns the bare drag, shift overrides.
        assert!(!TerminalView::selection_gesture_wanted(true, false));
        assert!(TerminalView::selection_gesture_wanted(true, true));
        // No tracking: nothing to override, either way selects.
        assert!(TerminalView::selection_gesture_wanted(false, false));
        assert!(TerminalView::selection_gesture_wanted(false, true));
    }

    /// #264 (part of #263): does libghostty-vt answer the Kitty graphics
    /// query itself, or must the pane compose the reply?
    ///
    /// This is the pivot the whole ticket turns on. Both Pi and omp probe
    /// actively and stay silent until answered, so if the crate replies the
    /// remaining work is only rendering; if it does not, the pane owes a
    /// reply it currently has no idea how to build.
    ///
    /// The query is Kitty's own support probe -- a 1x1 RGB image with
    /// `a=q` (query, do not store) -- and a terminal that supports the
    /// protocol answers `_Gi=<id>;OK`.
    #[test]
    fn kitty_graphics_query_answer_comes_from_the_crate_or_not_at_all() {
        let replies = std::rc::Rc::new(std::cell::RefCell::new(Vec::<u8>::new()));
        let sink = replies.clone();
        let mut term = headless_term(80, 24);
        term.on_pty_write(move |_term, data: &[u8]| {
            sink.borrow_mut().extend_from_slice(data);
        })
        .expect("register the pty-write callback");

        // A control first: DSR-CPR is a query the crate is known to answer,
        // so an empty result below means "no Kitty reply", not "the callback
        // was never wired".
        advance_headless(&mut term, b"[6n");
        let cursor_reply = String::from_utf8_lossy(&replies.borrow()).into_owned();
        assert!(
            cursor_reply.contains('R'),
            "control: the crate answers DSR-CPR, so the callback is live: {cursor_reply:?}"
        );
        replies.borrow_mut().clear();

        advance_headless(&mut term, b"_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\\");
        let answer = String::from_utf8_lossy(&replies.borrow()).into_owned();
        assert_eq!(
            answer, "_Gi=31;OK\\",
            "libghostty-vt answers the Kitty graphics query itself; if this              ever fails, the pane owes the reply and #263's scope grows"
        );
    }

    /// #301 (R2.3's test half): the copy-out the owner thread performs for
    /// every placement — plain owned pixels and geometry, no `!Send` borrows
    /// escape. A raw 1x1 RGB transmit placed at the cursor must come back as
    /// exactly those bytes, and a retransmit of the same id must carry a new
    /// generation (R4.1 keys on both).
    #[test]
    fn kitty_placement_walk_copies_plain_pixels_and_geometry() {
        let mut term = headless_term(80, 24);
        // The real owner thread is resized with cell pixel dimensions before
        // any input; placements are invisible without them (their grid size
        // computes to zero), so the headless harness follows the same shape
        // `resize_headless` already uses.
        resize_headless(&mut term, 80, 24);
        // Base64 of [0xFF, 0x00, 0x00] (a red pixel).
        advance_headless(&mut term, b"\x1b_Ga=t,f=24,s=1,v=1,i=1,q=2;/wAA\x1b\\");
        advance_headless(&mut term, b"\x1b_Ga=p,q=2,i=1,C=1\x1b\\");

        let buckets = collect_kitty_placements(&mut term);
        // Default z is 0, the AboveText band (R3.1).
        assert_eq!(buckets.above_text.len(), 1, "one placement for one image");
        let first = &buckets.above_text[0];
        assert_eq!(first.image_id, 1);
        assert_eq!(
            first.format,
            libghostty_vt::kitty::graphics::ImageFormat::Rgb
        );
        assert_eq!(&first.data, &[0xFF, 0x00, 0x00], "RGB pixels, verbatim");
        assert_eq!((first.width, first.height), (1, 1));
        assert_eq!((first.viewport_col, first.viewport_row), (0, 0));
        assert_eq!((first.pixel_width, first.pixel_height), (1, 1));
        assert!(
            first.generation > 0,
            "a stored image never has generation zero"
        );
        let generation_1 = first.generation;

        // Retransmit the same image id with different pixels: same id, new
        // generation, new bytes — the cache must treat this as a new image.
        advance_headless(&mut term, b"\x1b_Ga=t,f=24,s=1,v=1,i=1,q=2;AAD/\x1b\\");
        let buckets = collect_kitty_placements(&mut term);
        assert_eq!(buckets.above_text.len(), 1);
        let second = &buckets.above_text[0];
        assert_eq!(second.image_id, 1);
        assert_ne!(
            second.generation, generation_1,
            "retransmit bumps the generation"
        );
        assert_eq!(&second.data, &[0x00, 0x00, 0xFF], "blue now");
    }

    /// #302 R3.1: the iterator's own `set_layer` filter does the bucketing —
    /// one walk per Kitty z-band, never a re-classification by the pane.
    /// `All` is "no filter", so the prepaint state carries exactly three
    /// lists (below background / below text / above text).
    #[test]
    fn kitty_layers_bucket_placements_by_z_index() {
        let mut term = headless_term(80, 24);
        resize_headless(&mut term, 80, 24);
        // Three images, three distinct z-bands (kitty spec: default 0 lands
        // above text; z < i32::MIN/2 sits below the cell background).
        for (id, z) in [(1, -2_000_000_000i32), (2, -1), (3, 5)] {
            advance_headless(
                &mut term,
                format!("\x1b_Ga=t,f=24,s=1,v=1,i={id},q=2;/wAA\x1b\\").as_bytes(),
            );
            advance_headless(
                &mut term,
                format!("\x1b_Ga=p,q=2,i={id},C=1,z={z}\x1b\\").as_bytes(),
            );
        }

        let buckets = collect_kitty_placements(&mut term);
        assert_eq!(buckets.below_bg.len(), 1, "z < i32::MIN/2 lands BelowBg");
        assert_eq!(
            buckets.below_text.len(),
            1,
            "i32::MIN/2 <= z < 0 lands BelowText"
        );
        assert_eq!(buckets.above_text.len(), 1, "z >= 0 lands AboveText");
        assert_eq!(buckets.below_bg[0].image_id, 1);
        assert_eq!(buckets.below_text[0].image_id, 2);
        assert_eq!(buckets.above_text[0].image_id, 3);
    }

    /// #302: the copy-once-per-(image_id, generation) contract (#301 R2.3)
    /// survives the bucket split — the three `set_layer` walks share one
    /// `seen` set, so an image placed on two layers still copies its bytes
    /// exactly once, on the first walk that sees it. The later placement
    /// reuses the render cache key instead of re-copying pixels.
    #[test]
    fn kitty_image_bytes_copied_once_across_layers() {
        let mut term = headless_term(80, 24);
        resize_headless(&mut term, 80, 24);
        // One 2x1 RGB image, placed twice: once below text, once above.
        advance_headless(&mut term, b"\x1b_Ga=t,f=24,s=2,v=1,i=4,q=2;/wAAAAD/\x1b\\");
        advance_headless(&mut term, b"\x1b_Ga=p,q=2,i=4,C=1,z=-1\x1b\\");
        advance_headless(&mut term, b"\x1b_Ga=p,q=2,i=4,C=1,z=5\x1b\\");

        let buckets = collect_kitty_placements(&mut term);
        assert_eq!(buckets.below_text.len(), 1, "one placement below text");
        assert_eq!(buckets.above_text.len(), 1, "one placement above text");
        // Walk order (BelowBg, BelowText, AboveText) is also prepaint's
        // decode order, so the first-walked placement carries the bytes.
        assert!(
            !buckets.below_text[0].data.is_empty(),
            "the first walk copies the pixels"
        );
        assert!(
            buckets.above_text[0].data.is_empty(),
            "the same image on another layer reuses the cache key, no second copy"
        );
    }

    /// #302 R3.3 (acceptance, geometry half): an image scrolled so part of
    /// it sticks above the pane's top edge keeps its FULL, untruncated
    /// geometry — negative `viewport_row`, still visible, pixel size intact.
    /// The clip stays with `Window::paint_image` (visible-bounds intersection
    /// plus atlas sub-rect), never a hand-clamped source rect.
    #[test]
    fn kitty_scrolled_half_offscreen_keeps_untruncated_geometry() {
        let mut term = headless_term(80, 24);
        resize_headless(&mut term, 80, 24);
        // An 8x36-px image (one 8px column, two 18px rows — 2x the height
        // of a line) placed at the top row, auto-sized from its aspect.
        let mut pixels = Vec::new();
        for _ in 0..(8 * 36) {
            pixels.extend_from_slice(&[0xFF, 0x00, 0x00]);
        }
        advance_headless(&mut term, b"\x1b[H");
        advance_headless(
            &mut term,
            format!("\x1b_Ga=t,f=24,s=8,v=36,i=9,q=2;{}", base64_encode(&pixels)).as_bytes(),
        );
        advance_headless(&mut term, b"\x1b_Ga=p,q=2,i=9,C=1,z=0\x1b\\");
        // Push one screenful plus one line: the top row of the image enters
        // the scrollback and the placement stands half above the pane's top
        // edge (measured: visible, viewport_row=-1, full 8x36 size).
        for _ in 0..24 {
            advance_headless(&mut term, b"Z\r\n");
        }
        let before = collect_kitty_placements(&mut term);
        // Change the scroll position while the image is still visible.
        term.scroll_viewport(ScrollViewport::Delta(-1));
        let after = collect_kitty_placements(&mut term);

        // The placement surviving the walk proves `viewport_visible` was true
        // (fully-off-screen placements are filtered inside the copy-out), so
        // BEFORE the scroll change it is half off the top with negative row.
        assert_eq!(
            before.above_text.len(),
            1,
            "one placement, half above the pane's top edge"
        );
        let half_out = &before.above_text[0];
        assert!(half_out.viewport_row < 0, "top rows sit above the viewport");
        assert_eq!(
            (half_out.pixel_width, half_out.pixel_height),
            (8, 36),
            "geometry is NOT truncated to the visible part"
        );
        // After the scroll-position change the placement is still collected
        // with the same untruncated geometry.
        assert_eq!(
            after.above_text.len(),
            1,
            "still one placement after scrolling"
        );
        assert_eq!(
            (
                after.above_text[0].pixel_width,
                after.above_text[0].pixel_height
            ),
            (8, 36),
            "scroll change keeps the untruncated geometry"
        );
        // The painter feeds this untruncated placement to `paint_image` with
        // the pane rect as the clip: negative origin, full size.
        let pane = Bounds::new(point(px(10.0), px(20.0)), size(px(640.0), px(432.0)));
        let image_bounds = kitty_image_bounds(pane, px(8.0), LINE_HEIGHT, half_out);
        assert!(
            image_bounds.origin.y < pane.origin.y,
            "origin stays negative"
        );
        assert_eq!(image_bounds.size.height, px(36.0), "height stays full");
        assert_eq!(image_bounds.size.width, px(8.0), "width stays full");
    }

    /// #301 acceptance: the decode produces BGRA pixels with R/B swapped for
    /// byte-faithful RGB (R2.5) and RGBA inputs, passes gray through
    /// unchanged with alpha, and refuses payloads whose length cannot match
    /// the announced dimensions (the R2.4 hard bound — a stored image is
    /// never compressed, so the length check is the ceiling).
    #[test]
    fn kitty_decode_swaps_rgb_and_rgba_into_bgra() {
        let rgb = decode_kitty_image(
            libghostty_vt::kitty::graphics::ImageFormat::Rgb,
            2,
            1,
            &[0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF], // red, blue
        )
        .expect("RGB decodes");
        let pixels = rgb.as_bytes(0).expect("one frame");
        assert_eq!(&pixels[0..4], &[0x00, 0x00, 0xFF, 0xFF], "red -> BGRA");
        assert_eq!(&pixels[4..8], &[0xFF, 0x00, 0x00, 0xFF], "blue -> BGRA");
        assert_eq!(rgb.size(0), gpui::size(2.into(), 1.into()));

        let rgba = decode_kitty_image(
            libghostty_vt::kitty::graphics::ImageFormat::Rgba,
            1,
            1,
            &[0xFF, 0x00, 0x00, 0x80], // red, half alpha
        )
        .expect("RGBA decodes");
        assert_eq!(
            rgba.as_bytes(0).unwrap(),
            &[0x00, 0x00, 0xFF, 0x80],
            "alpha survives the swap"
        );

        let gray = decode_kitty_image(
            libghostty_vt::kitty::graphics::ImageFormat::Gray,
            2,
            1,
            &[0x80, 0x40],
        )
        .expect("gray decodes");
        assert_eq!(
            gray.as_bytes(0).unwrap(),
            &[0x80, 0x80, 0x80, 0xFF, 0x40, 0x40, 0x40, 0xFF],
            "gray replicates into opaque BGRA"
        );

        // R2.4: a payload that cannot match the announced dimensions is
        // refused (and the caller paints a visible placeholder, R2.6) rather
        // than trusting a lying length.
        assert!(
            decode_kitty_image(
                libghostty_vt::kitty::graphics::ImageFormat::Rgb,
                2,
                1,
                &[0xFF],
            )
            .is_none(),
            "length mismatch is refused"
        );
    }

    /// #301 (R4.1): the render cache keys on `(image_id, generation)`, so a
    /// repeated render of the same image reuses the same `RenderImage` —
    /// `Arc::ptr_eq` proves no re-decode and no rebuild — while a replaced
    /// image (new generation, same id) decodes anew.
    #[test]
    fn kitty_render_cache_keys_on_image_id_and_generation() {
        let mut cache = KittyImageCache::default();
        let tick = cache.begin_frame();
        let red = KittyPlacement {
            image_id: 1,
            generation: 3,
            format: libghostty_vt::kitty::graphics::ImageFormat::Rgb,
            width: 1,
            height: 1,
            data: vec![0xFF, 0x00, 0x00],
            viewport_col: 0,
            viewport_row: 0,
            pixel_width: 1,
            pixel_height: 1,
        };
        let first = cache.get_or_decode(&red, tick).expect("decodes");
        let second = cache.get_or_decode(&red, tick).expect("decodes");
        assert!(
            Arc::ptr_eq(&first, &second),
            "same (image_id, generation) must reuse the cached RenderImage"
        );

        // Same id retransmitted: the generation differs, so the cache misses
        // and the new pixels decode.
        let blue = KittyPlacement {
            generation: 4,
            data: vec![0x00, 0x00, 0xFF],
            ..red
        };
        let third = cache.get_or_decode(&blue, tick).expect("decodes");
        assert!(
            !Arc::ptr_eq(&first, &third),
            "new generation must decode anew"
        );
        assert_eq!(&third.as_bytes(0).unwrap()[0..3], &[0xFF, 0x00, 0x00]);
    }

    /// #303 R2.4: a small-on-the-wire PNG must be refused before its full
    /// decompressed pixel buffer can be allocated.
    #[test]
    fn kitty_png_decoder_rejects_compressed_images_over_decompression_limit() {
        let png = image::RgbaImage::from_pixel(4097, 4096, image::Rgba([0x12, 0x34, 0x56, 0xFF]));
        let mut encoded = Vec::new();
        png.write_to(
            &mut std::io::Cursor::new(&mut encoded),
            image::ImageFormat::Png,
        )
        .expect("encode the compressed test image");
        assert!(
            encoded.len() < 1024 * 1024,
            "solid image should be much smaller compressed than decompressed"
        );

        let failed = Arc::new(AtomicBool::new(false));
        let mut decoder = KittyPngDecoder::with_failure_flag(failed.clone());
        assert!(
            decoder
                .decode_png(&libghostty_vt::alloc::Allocator::GLOBAL, &encoded)
                .is_none(),
            "the decompressed image exceeds the Kitty decoder ceiling"
        );
        assert!(
            failed.load(Ordering::Acquire),
            "a refused decode must be available to the pane for visible reporting"
        );
    }

    /// #303 R2.6: a malformed PNG is refused and leaves the visible refusal
    /// indicator that the paint path overlays in the pane.
    #[test]
    fn kitty_png_decode_failure_is_visible_to_the_paint_path() {
        let failed = Arc::new(AtomicBool::new(false));
        let mut decoder = KittyPngDecoder::with_failure_flag(failed.clone());
        assert!(
            decoder
                .decode_png(&libghostty_vt::alloc::Allocator::GLOBAL, b"not a PNG")
                .is_none(),
            "malformed PNG must be refused"
        );
        assert!(failed.load(Ordering::Acquire));

        let pane = Bounds::new(point(px(10.0), px(20.0)), size(px(100.0), px(50.0)));
        let indicator = kitty_refused_indicator_bounds(pane);
        assert_eq!(indicator.origin, pane.origin);
        assert!(indicator.size.width > px(0.0));
        assert!(indicator.size.height > px(0.0));
    }

    /// #301: omp transmits PNG (f=100) — measured in the shipped pi-tui
    /// (`encodeKittyTransmit` emits `a=t,f=100`), and ghostty refuses to
    /// store a PNG unless a decoder is registered (`graphics_image.zig`
    /// `complete`/`decodePng`). The pane's own `DecodePng` uses the
    /// workspace `image` crate (R2.1: no second PNG stack).
    #[test]
    fn kitty_png_ingest_decoder_decodes_to_rgba() {
        let mut png = Vec::new();
        image::RgbaImage::from_raw(2, 1, vec![0xFF, 0x00, 0x00, 0xFF, 0x00, 0x00, 0xFF, 0x80])
            .expect("2x1 buffer")
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("encode PNG");

        let mut decoder = KittyPngDecoder::new();
        let decoded = decoder
            .decode_png(&libghostty_vt::alloc::Allocator::GLOBAL, &png)
            .expect("PNG decodes via the image crate");
        assert_eq!((decoded.width, decoded.height), (2, 1));
        assert_eq!(
            &decoded.data[..],
            &[0xFF, 0x00, 0x00, 0xFF, 0x00, 0x00, 0xFF, 0x80],
            "rgba pixels, verbatim"
        );
    }

    /// #301 R1.3: the ingest ceilings are stated explicitly instead of
    /// inherited from the emulator's defaults — the same function the owner
    /// thread applies (`spawn_terminal_thread`) and the headless harness
    /// (`headless_term`) shares, so what the test reads is what the pane
    /// ships with.
    #[test]
    fn kitty_ingest_limits_are_explicit() {
        let mut term = headless_term(80, 24);
        // `set_apc_max_bytes_kitty` has no getter, so the constants are the
        // readable half of the assertion; the storage getter is live.
        const {
            assert!(KITTY_APC_MAX_BYTES > 0, "APC ceiling explicit");
        }
        const {
            assert!(KITTY_IMAGE_STORAGE_LIMIT > 0, "storage ceiling explicit");
        }
        term.set_apc_max_bytes_kitty(Some(KITTY_APC_MAX_BYTES))
            .expect("re-apply APC override");
        term.set_kitty_image_storage_limit(KITTY_IMAGE_STORAGE_LIMIT)
            .expect("re-apply storage limit");
        assert_eq!(
            term.kitty_image_storage_limit()
                .expect("read storage limit"),
            KITTY_IMAGE_STORAGE_LIMIT
        );
    }

    /// #301 end-to-end: a PNG transmit from omp's own emission shape
    /// (`a=t,f=100,q=2,i=<id>` then `a=p`) is stored as raw RGBA and the
    /// placement walk hands the renderer plain pixels. Base64-encodes a real
    /// PNG to prove the decoder registered on this thread (the same call the
    /// owner thread makes) is what the emulator actually calls.
    #[test]
    fn kitty_png_transmit_reaches_the_placement_walk_as_rgba() {
        libghostty_vt::kitty::graphics::set_png_decoder(Some(Box::new(KittyPngDecoder::new())))
            .expect("register decoder on this thread");
        let mut png = Vec::new();
        image::RgbaImage::from_raw(1, 1, vec![0x12, 0x34, 0x56, 0xFF])
            .expect("1x1 buffer")
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("encode PNG");
        let b64 = base64_encode(&png);

        let mut term = headless_term(80, 24);
        resize_headless(&mut term, 80, 24);
        advance_headless(
            &mut term,
            format!("\x1b_Ga=t,f=100,s=1,v=1,i=7,q=2;{b64}\x1b\\").as_bytes(),
        );
        advance_headless(&mut term, b"\x1b_Ga=p,q=2,i=7,C=1\x1b\\");

        let buckets = collect_kitty_placements(&mut term);
        assert_eq!(
            buckets.above_text.len(),
            1,
            "the PNG image stored and placed"
        );
        let place = &buckets.above_text[0];
        assert_eq!(place.image_id, 7);
        assert_eq!(
            place.format,
            libghostty_vt::kitty::graphics::ImageFormat::Rgba,
            "ghostty stores decoded PNG pixels as RGBA"
        );
        assert_eq!(
            &place.data,
            &[0x12, 0x34, 0x56, 0xFF],
            "the PNG's own pixels, verbatim"
        );
        assert_eq!((place.width, place.height), (1, 1));
    }

    /// #308 test half: a placeable 1x1 RGB image with deterministic pixels.
    fn kitty_pixel_place(image_id: u32, generation: u64) -> KittyPlacement {
        KittyPlacement {
            image_id,
            generation,
            format: libghostty_vt::kitty::graphics::ImageFormat::Rgb,
            width: 1,
            height: 1,
            data: vec![0xFF, 0x00, 0x00],
            viewport_col: 0,
            viewport_row: 0,
            pixel_width: 1,
            pixel_height: 1,
        }
    }

    /// #308 R4.4: the owner-thread mutation stamp that gates the placement
    /// re-scan and grid render cache must stay flat while the pane is idle — a
    /// still pane costs one integer comparison per frame, not a channel round
    /// trip plus three placement walks and a snapshot — and bump the moment
    /// anything is written. Driven
    /// through a real PTY so the owner-thread poll loop is the one under
    /// test, the same shape `pty_output_wakes_the_event_pump` uses.
    #[test]
    fn mutation_stamp_stays_flat_while_idle_and_bumps_on_output() {
        let working_directory =
            std::env::temp_dir().join(format!("sirio-terminal-test-stamp-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).unwrap();
        let (handle, mut wakeup_rx) =
            TerminalHandle::new(&working_directory, &TerminalShell::System).unwrap();

        // Wait for TRUE quiescence: Windows ConPTY shells deliver startup
        // traffic in several chunks with gaps between them, so a single
        // `Empty` drain can return before the banner has fully arrived. Settle
        // only when the stamp is unchanged across a quiet gap — more output
        // would have bumped it — and only once it has moved at all: a stamp
        // still at zero means the shell has not even printed its prompt yet
        // (it starts late when the test binary spawns many PTYs at once), and
        // that prompt would land inside the idle window below.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let before = loop {
            while wakeup_rx.try_recv().is_ok() {}
            let sample = handle.mutation_stamp.load(Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(100));
            if sample > 0 && handle.mutation_stamp.load(Ordering::Relaxed) == sample {
                break sample;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "startup never settled"
            );
        };
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(
            handle.mutation_stamp.load(Ordering::Relaxed),
            before,
            "R4.4: a still pane must not bump the stamp — the re-scan gate is one integer comparison per frame"
        );

        // Any output bumps it; kitty transmits arrive through the same path.
        handle.write(b"printf STAMP_PROBE\n".to_vec());
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut bumped = false;
        while std::time::Instant::now() < deadline {
            if handle.mutation_stamp.load(Ordering::Relaxed) != before {
                bumped = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            bumped,
            "R4.4: output must bump the stamp so prepaint re-scans"
        );
        handle.shutdown();
        let _ = std::fs::remove_dir_all(working_directory);
    }

    /// #308 E1 (R4.5): a retransmitted image id carries a new generation; the
    /// cache reconcile drops the superseded generation the moment the walk
    /// reports the id live at the newer one — the old `RenderImage` is dead
    /// weight that would otherwise stay in the atlas for the process lifetime.
    #[test]
    fn kitty_replaced_image_drops_the_superseded_generation() {
        let mut term = headless_term(80, 24);
        resize_headless(&mut term, 80, 24);
        // Red pixel, id 1, placed at the cursor.
        advance_headless(&mut term, b"\x1b_Ga=t,f=24,s=1,v=1,i=1,q=2;/wAA\x1b\\");
        advance_headless(&mut term, b"\x1b_Ga=p,q=2,i=1,C=1\x1b\\");
        let buckets = collect_kitty_placements(&mut term);
        assert_eq!(buckets.live.len(), 1, "one live placement");
        let old_generation = buckets.live[0].1;

        let mut cache = KittyImageCache::default();
        let tick = cache.begin_frame();
        let first = cache
            .get_or_decode(&buckets.above_text[0], tick)
            .expect("decodes");
        assert_eq!(cache.images.len(), 1);

        // Retransmit the same id with different pixels: same id, new
        // generation (measured: ghostty bumps it on retransmit).
        advance_headless(&mut term, b"\x1b_Ga=t,f=24,s=1,v=1,i=1,q=2;AAD/\x1b\\");
        let buckets = collect_kitty_placements(&mut term);
        let new_generation = buckets.live[0].1;
        assert_ne!(
            old_generation, new_generation,
            "retransmit bumps the generation"
        );

        // The reconcile sees the id live at the NEW generation and names the
        // old key dead; the new pixels decode on the next lookup.
        let dead = cache.dead_keys(&buckets.live);
        assert_eq!(
            dead,
            vec![(1, old_generation)],
            "only the superseded key is dead"
        );
        assert!(cache.remove(&dead[0]).is_some());
        assert!(!cache.images.contains_key(&(1, old_generation)));
        let tick = cache.begin_frame();
        let second = cache
            .get_or_decode(&buckets.above_text[0], tick)
            .expect("redecodes from the new generation");
        assert!(
            !Arc::ptr_eq(&first, &second),
            "the replacement is a fresh texture"
        );
    }

    /// #308 E2 (R4.5): a guest delete removes placements from the grid. The
    /// LIVE placement set (visible or not) is what the cache reconciles
    /// against: an image whose id leaves it — `a=d` deletes placements even
    /// when the image stays in the emulator's store (measured) — is dead
    /// weight and must be released. `a=D` (delete images too) lands here the
    /// same way.
    #[test]
    fn kitty_deleted_placement_releases_the_image() {
        let mut term = headless_term(80, 24);
        resize_headless(&mut term, 80, 24);
        advance_headless(&mut term, b"\x1b_Ga=t,f=24,s=1,v=1,i=1,q=2;/wAA\x1b\\");
        advance_headless(&mut term, b"\x1b_Ga=p,q=2,i=1,C=1\x1b\\");
        let buckets = collect_kitty_placements(&mut term);
        assert_eq!(buckets.live.len(), 1);
        let generation = buckets.live[0].1;

        let mut cache = KittyImageCache::default();
        let tick = cache.begin_frame();
        assert!(cache.get_or_decode(&buckets.above_text[0], tick).is_some());

        // `a=d` (delete placements; the image itself stays in ghostty's
        // store) empties the live set.
        advance_headless(&mut term, b"\x1b_Ga=d,q=2\x1b\\");
        let buckets = collect_kitty_placements(&mut term);
        assert!(buckets.live.is_empty(), "delete empties the live set");

        let dead = cache.dead_keys(&buckets.live);
        assert_eq!(
            dead,
            vec![(1, generation)],
            "the vanished id is the dead key"
        );
        assert!(cache.remove(&dead[0]).is_some(), "its texture is released");
        assert!(cache.images.is_empty());
    }

    /// #308 E2, scroll half (R4.7): scrolling an image OUT of view does NOT
    /// kill its placement — the pin survives in the scrollback (measured
    /// against libghostty-vt 0.2.1) — so the reconcile keeps it cached and a
    /// scroll back is a cache hit instead of a re-decode. Deleting it is
    /// what releases it, not scrolling it away.
    #[test]
    fn kitty_scrolled_away_image_stays_cached_until_deleted() {
        let mut term = headless_term(80, 24);
        resize_headless(&mut term, 80, 24);
        // An 8x144-px image (8 columns x 8 rows) placed at the top.
        let mut pixels = Vec::new();
        for _ in 0..(8 * 144) {
            pixels.extend_from_slice(&[0xFF, 0x00, 0x00]);
        }
        advance_headless(&mut term, b"\x1b[H");
        advance_headless(
            &mut term,
            format!(
                "\x1b_Ga=t,f=24,s=8,v=144,i=4,q=2;{}",
                base64_encode(&pixels)
            )
            .as_bytes(),
        );
        advance_headless(&mut term, b"\x1b_Ga=p,q=2,i=4,C=1,z=0\x1b\\");
        let before = collect_kitty_placements(&mut term);
        assert_eq!(before.live.len(), 1);
        let generation = before.live[0].1;

        let mut cache = KittyImageCache::default();
        let tick = cache.begin_frame();
        assert!(cache.get_or_decode(&before.above_text[0], tick).is_some());

        // Scroll the image fully out of the viewport (into scrollback): 24
        // line-feeds only reach the bottom of the screen — the placement sits
        // one row up, still partially visible (the mirrored half-offscreen
        // test pins that) — so keep pushing until the 8-row image is beyond
        // the top edge entirely.
        for _ in 0..48 {
            advance_headless(&mut term, b"Z\r\n");
        }
        let scrolled = collect_kitty_placements(&mut term);
        assert!(
            scrolled.above_text.is_empty(),
            "fully scrolled-out placements leave the visible walk"
        );
        assert_eq!(
            scrolled.live,
            vec![(4, generation)],
            "but the placement is still pinned in the scrollback — LIVE"
        );
        assert!(
            cache.dead_keys(&scrolled.live).is_empty(),
            "a live, same-generation id must not be released on scroll-away"
        );

        // And it IS still showable: scroll back and it returns.
        term.scroll_viewport(ScrollViewport::Top);
        let back = collect_kitty_placements(&mut term);
        assert_eq!(
            back.above_text.len(),
            1,
            "scroll-back restores the placement"
        );
        let tick = cache.begin_frame();
        let restored = cache
            .get_or_decode(&back.above_text[0], tick)
            .expect("cache hit");
        assert!(
            Arc::ptr_eq(&restored, &cache.images[&(4, generation)]),
            "scroll-back is a cache HIT — the texture survives the scroll away"
        );
    }

    /// #308 E3 (R4.5/R4.6): scrollback trims do NOT remove placements in
    /// libghostty-vt 0.2.1 (measured: pushing 16k lines through a 10k
    /// scrollback leaves the placement live and showable), so there is no
    /// per-trim signal to drop on. The release mechanism is the atlas cap:
    /// when a session replaces/scrolls more images than the cap, the cache
    /// must not grow beyond it, however long the pane lives.
    #[test]
    fn kitty_cache_stays_bounded_by_the_atlas_cap_through_churn() {
        let mut cache = KittyImageCache::default();
        let cap = KITTY_ATLAS_IMAGE_CAP;
        // A session that streams three caps' worth of distinct images past
        // the pane: every image stays live (trimmed placements survive, so
        // the reconcile never fires) and each frame paints only the newest.
        for id in 0..(3 * cap) as u32 {
            let tick = cache.begin_frame();
            cache.get_or_decode(&kitty_pixel_place(id, 1), tick);
            for key in cache.lru_dead_keys(cap, tick) {
                cache.remove(&key);
            }
            assert!(
                cache.images.len() <= cap + 1,
                "the cache overran the atlas cap at image {id}: {} entries",
                cache.images.len()
            );
        }
        assert_eq!(cache.images.len(), cap, "settles exactly at the ceiling");
    }

    /// #308 R4.7: the eviction ORDER is least-recently-*painted*, never
    /// least-recently-added. Image 1 is added first and painted again long
    /// after image 2's only paint — when the cap forces one out, image 2
    /// goes, because 1 is what the user is looking at.
    #[test]
    fn kitty_lru_eviction_tracks_last_painted_not_last_added() {
        let mut cache = KittyImageCache::default();
        let one = kitty_pixel_place(1, 1);
        let two = KittyPlacement {
            image_id: 2,
            data: vec![0x00, 0x00, 0xFF],
            ..one
        };
        // Frame 1: both added and painted.
        let tick = cache.begin_frame();
        cache.get_or_decode(&one, tick);
        cache.get_or_decode(&two, tick);
        // Frames 2..8: a still pane — nothing is painted.
        for _ in 2..9 {
            cache.begin_frame();
        }
        // Frame 9: image 1 is scrolled back into view and painted again.
        let t9 = cache.begin_frame();
        cache.get_or_decode(&one, t9);
        // Frame 10, cap 1: only one may stay.
        let tick = cache.begin_frame();
        let dead = cache.lru_dead_keys(1, tick);
        assert_eq!(
            dead,
            vec![(2, 1)],
            "the least-recently-PAINTED victim is image 2, not the least-recently-added image 1 — 1 was repainted at frame 9"
        );
    }

    /// #308 E4 (R4.5): closing a pane drops the per-pane cache; its `Drop`
    /// must park every remaining image in the graveyard that the next paint
    /// drains into `Window::drop_image`. Dropping the last `Arc` without
    /// `drop_image` would leave the GPU texture alive for the process
    /// lifetime, which is the leak this ticket exists to close.
    #[test]
    fn kitty_pane_close_parks_every_image_for_the_next_paint_to_drop() {
        let mut cache = KittyImageCache::default();
        let tick = cache.begin_frame();
        let first = cache
            .get_or_decode(&kitty_pixel_place(1, 1), tick)
            .expect("decodes");
        let second = cache
            .get_or_decode(&kitty_pixel_place(2, 4), tick)
            .expect("decodes");

        drop(cache); // the pane closed

        // The graveyard is process-global and the harness runs tests in
        // parallel, so sibling tests park their own images concurrently;
        // assert membership of THIS pane's images, not an absolute count.
        let parked = KITTY_DROPPED_IMAGES.lock();
        assert!(
            parked.iter().any(|image| Arc::ptr_eq(image, &first))
                && parked.iter().any(|image| Arc::ptr_eq(image, &second)),
            "both decoded textures of the closed pane reach the next paint's drop list"
        );
    }

    /// Test-only RFC 4648 encoder for the Kitty APC payloads above; the
    /// crate itself never base64-encodes anything.
    fn base64_encode(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let n = (u32::from(chunk[0]) << 16)
                | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
                | u32::from(*chunk.get(2).unwrap_or(&0));
            out.push(ALPHABET[(n >> 18) as usize & 63] as char);
            out.push(ALPHABET[(n >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 {
                ALPHABET[(n >> 6) as usize & 63] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                ALPHABET[n as usize & 63] as char
            } else {
                '='
            });
        }
        out
    }

    /// #259: a selection spanning lines is linewise, not rectangular.
    #[test]
    fn a_multi_line_selection_takes_whole_lines_between_its_ends() {
        let range = SelectedRange::between((1, 5), (3, 2));
        // First line: from the anchor rightwards only.
        assert!(!range.contains(1, 4));
        assert!(range.contains(1, 5));
        assert!(range.contains(1, 99));
        // Middle lines: everything.
        assert!(range.contains(2, 0));
        assert!(range.contains(2, 400));
        // Last line: up to the focus only.
        assert!(range.contains(3, 2));
        assert!(!range.contains(3, 3));
        // Outside entirely.
        assert!(!range.contains(0, 5));
        assert!(!range.contains(4, 0));
    }

    /// A drag that runs backwards selects the same cells as the same drag
    /// forwards -- the user does not have to sweep in the "right" direction.
    #[test]
    fn a_backwards_selection_covers_the_same_cells() {
        let forwards = SelectedRange::between((1, 5), (3, 2));
        let backwards = SelectedRange::between((3, 2), (1, 5));
        assert_eq!(forwards, backwards);

        let right = SelectedRange::between((2, 1), (2, 8));
        let left = SelectedRange::between((2, 8), (2, 1));
        assert_eq!(right, left);
        assert!(right.contains(2, 4));
    }

    /// Within one line the selection is an ordinary column span.
    #[test]
    fn a_single_line_selection_is_a_column_span() {
        let range = SelectedRange::between((7, 3), (7, 6));
        assert!(!range.contains(7, 2));
        assert!(range.contains(7, 3));
        assert!(range.contains(7, 6));
        assert!(!range.contains(7, 7));
        assert!(!range.contains(6, 4));
        assert!(!range.contains(8, 4));
    }

    /// A click without a drag must clear the highlight, not tint one cell.
    #[test]
    fn a_selection_with_no_extent_is_empty() {
        assert!(SelectedRange::between((4, 9), (4, 9)).is_empty());
        assert!(!SelectedRange::between((4, 9), (4, 10)).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn spawn_cwd_strips_the_windows_verbatim_prefix() {
        assert_eq!(
            spawn_cwd(Path::new(r"\\?\C:\Users\me\project")),
            Path::new(r"C:\Users\me\project")
        );
    }

    #[cfg(windows)]
    #[test]
    fn spawn_cwd_converts_the_verbatim_unc_form_to_a_dos_unc_path() {
        assert_eq!(
            spawn_cwd(Path::new(r"\\?\UNC\server\share\x")),
            Path::new(r"\\server\share\x")
        );
    }

    #[test]
    fn spawn_cwd_passes_ordinary_paths_through_unchanged() {
        assert_eq!(
            spawn_cwd(Path::new("/tmp/note.md")),
            Path::new("/tmp/note.md")
        );
        assert_eq!(
            spawn_cwd(Path::new("relative/path")),
            Path::new("relative/path")
        );
    }

    // -----------------------------------------------------------------------
    // Headless boundary harness (#40 invariant half).
    //
    // A bare emulator needs no PTY, so these feed known byte streams straight
    // into a `Terminal` on the test thread through the same parser path the
    // owner thread uses (`vt_write`), and read back through
    // `capture_scrollback_text` — plain text only, never libghostty types.
    // These pin behaviour that must NOT change across the #31 migration;
    // grapheme/wide-char/spacer rendering belongs to the other half.
    // -----------------------------------------------------------------------

    fn headless_term(columns: u16, lines: u16) -> Terminal<'static, 'static> {
        let mut term = Terminal::new(TerminalOptions {
            cols: columns,
            rows: lines,
            max_scrollback: 10_000,
        })
        .expect("headless terminal");
        // Same embedder-side default the owner thread applies at creation:
        // grapheme clustering (DEC 2027) is off upstream.
        term.set_mode(Mode::GRAPHEME_CLUSTER, true)
            .expect("enable DEC 2027 grapheme clustering");
        // #301 R1.3: the headless harness applies the same explicit Kitty
        // ingest ceilings the owner thread does, so the limit test reads the
        // shipped constants through the real setter path.
        apply_kitty_ingest_limits(&mut term);
        term
    }

    fn advance_headless(term: &mut Terminal<'static, 'static>, bytes: &[u8]) {
        term.vt_write(bytes);
    }

    fn key_event(key: &str, key_char: Option<&str>, modifiers: gpui::Modifiers) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: gpui::Keystroke {
                key: key.to_string(),
                key_char: key_char.map(str::to_string),
                modifiers,
            },
            is_held: false,
            prefer_character_input: false,
        }
    }

    fn encoded_key(term: &Terminal<'static, 'static>, event: &KeyDownEvent) -> Vec<u8> {
        let input = key_input(event).expect("guest-bound key");
        let mut encoder = key::Encoder::new().expect("key encoder");
        encode_key_input(term, &mut encoder, input).expect("encode key")
    }

    #[test]
    fn default_keyboard_encoding_preserves_legacy_bytes() {
        let term = headless_term(80, 24);
        let plain = gpui::Modifiers::default();
        let cases = [
            ("enter", None, plain, b"\r".as_slice()),
            ("backspace", None, plain, b"\x7f".as_slice()),
            ("tab", None, plain, b"\t".as_slice()),
            ("escape", None, plain, b"\x1b".as_slice()),
            ("up", None, plain, b"\x1b[A".as_slice()),
            ("down", None, plain, b"\x1b[B".as_slice()),
            ("right", None, plain, b"\x1b[C".as_slice()),
            ("left", None, plain, b"\x1b[D".as_slice()),
            ("home", None, plain, b"\x1b[H".as_slice()),
            ("end", None, plain, b"\x1b[F".as_slice()),
            ("delete", None, plain, b"\x1b[3~".as_slice()),
            ("a", Some("a"), plain, b"a".as_slice()),
            (
                "c",
                Some("c"),
                gpui::Modifiers {
                    control: true,
                    ..plain
                },
                b"\x03".as_slice(),
            ),
            (
                "x",
                Some("x"),
                gpui::Modifiers { alt: true, ..plain },
                b"\x1bx".as_slice(),
            ),
        ];

        for (key, key_char, modifiers, expected) in cases {
            assert_eq!(
                encoded_key(&term, &key_event(key, key_char, modifiers)),
                expected,
                "{key}"
            );
        }
    }

    #[test]
    fn function_keys_emit_terminal_escape_sequences() {
        let term = headless_term(80, 24);
        let cases = [
            ("f1", b"\x1bOP".as_slice()),
            ("f2", b"\x1bOQ".as_slice()),
            ("f3", b"\x1bOR".as_slice()),
            ("f4", b"\x1bOS".as_slice()),
            ("f5", b"\x1b[15~".as_slice()),
            ("f6", b"\x1b[17~".as_slice()),
            ("f7", b"\x1b[18~".as_slice()),
            ("f8", b"\x1b[19~".as_slice()),
            ("f9", b"\x1b[20~".as_slice()),
            ("f10", b"\x1b[21~".as_slice()),
            ("f11", b"\x1b[23~".as_slice()),
            ("f12", b"\x1b[24~".as_slice()),
        ];

        for (key, expected) in cases {
            assert_eq!(
                encoded_key(&term, &key_event(key, None, gpui::Modifiers::default())),
                expected,
                "{key}"
            );
        }
    }

    #[test]
    fn application_cursor_mode_uses_ss3_for_navigation_keys() {
        let mut term = headless_term(80, 24);
        advance_headless(&mut term, b"\x1b[?1h");

        for (key, expected) in [
            ("up", b"\x1bOA".as_slice()),
            ("down", b"\x1bOB".as_slice()),
            ("right", b"\x1bOC".as_slice()),
            ("left", b"\x1bOD".as_slice()),
            ("home", b"\x1bOH".as_slice()),
            ("end", b"\x1bOF".as_slice()),
        ] {
            assert_eq!(
                encoded_key(&term, &key_event(key, None, gpui::Modifiers::default())),
                expected,
                "{key}"
            );
        }
    }

    #[test]
    fn modified_arrows_include_the_modifier_parameter() {
        let term = headless_term(80, 24);
        let modifiers = gpui::Modifiers {
            control: true,
            ..Default::default()
        };

        assert_eq!(
            encoded_key(&term, &key_event("left", None, modifiers)),
            b"\x1b[1;5D"
        );
    }

    #[test]
    fn kitty_disambiguate_distinguishes_escape_and_ctrl_i() {
        let mut term = headless_term(80, 24);
        advance_headless(&mut term, b"\x1b[>1u");
        let control = gpui::Modifiers {
            control: true,
            ..Default::default()
        };

        assert_eq!(
            encoded_key(&term, &key_event("escape", None, Default::default())),
            b"\x1b[27u"
        );
        assert_eq!(
            encoded_key(&term, &key_event("i", Some("i"), control)),
            b"\x1b[105;5u"
        );
    }

    #[test]
    fn modify_other_keys_distinguishes_ctrl_i_from_tab() {
        let mut term = headless_term(80, 24);
        advance_headless(&mut term, b"\x1b[>4;2m");
        let control = gpui::Modifiers {
            control: true,
            ..Default::default()
        };

        assert_eq!(
            encoded_key(&term, &key_event("i", Some("i"), control)),
            b"\x1b[27;5;105~"
        );
    }

    // -----------------------------------------------------------------------
    // Mouse reporting boundary tests (#43): same headless shape as the #42
    // key tests — guest state driven with real bytes via vt_write, then
    // MouseInputs encoded through the exact production path (`encode_mouse_input`)
    // and the bytes asserted. No PTY, no window.
    // -----------------------------------------------------------------------

    /// Geometry matching a 640x360 pane at cell 8x18 — non-trivial cell size so
    /// the pixel→cell conversion is actually proven, not trivially identity.
    fn mouse_input_at(
        action: mouse::Action,
        button: Option<mouse::Button>,
        x: f32,
        y: f32,
        any_button_pressed: bool,
    ) -> MouseInput {
        MouseInput {
            action,
            button,
            mods: key::Mods::empty(),
            any_button_pressed,
            x,
            y,
            pane_width: 640.0,
            pane_height: 360.0,
            cell_width: 8.0,
            cell_height: 18.0,
        }
    }

    fn encoded_mouse(term: &Terminal<'static, 'static>, input: MouseInput) -> Vec<u8> {
        let mut encoder = MouseEncoderState::new().expect("mouse encoder");
        encode_mouse_input(term, &mut encoder, input).expect("encode mouse")
    }

    #[test]
    fn is_mouse_tracking_reflects_guest_requests() {
        let mut term = headless_term(80, 24);
        assert!(!term.is_mouse_tracking().expect("tracking query"));
        advance_headless(&mut term, b"\x1b[?1000h");
        assert!(term.is_mouse_tracking().expect("tracking query"));
        advance_headless(&mut term, b"\x1b[?1000l");
        assert!(!term.is_mouse_tracking().expect("tracking query"));
    }

    #[test]
    fn sgr_press_and_release_encode_cell_coordinates() {
        // opencode's startup request minus its any-event mode.
        let mut term = headless_term(80, 24);
        advance_headless(&mut term, b"\x1b[?1002h\x1b[?1006h");
        // Pane-local (44, 60) px at cell 8x18 lands on 0-based column 5 row 3;
        // SGR output is 1-based, so column 6 row 4.
        let press = encoded_mouse(
            &term,
            mouse_input_at(
                mouse::Action::Press,
                Some(mouse::Button::Left),
                44.0,
                60.0,
                true,
            ),
        );
        assert_eq!(press, b"\x1b[<0;6;4M");
        let release = encoded_mouse(
            &term,
            mouse_input_at(
                mouse::Action::Release,
                Some(mouse::Button::Left),
                44.0,
                60.0,
                false,
            ),
        );
        assert_eq!(release, b"\x1b[<0;6;4m");
    }

    #[test]
    fn motion_with_button_held_encodes_plus_32_form_and_dedups_per_cell() {
        let mut term = headless_term(80, 24);
        advance_headless(&mut term, b"\x1b[?1002h\x1b[?1006h");
        // One shared encoder mirrors the owner thread: per-cell dedup only
        // kicks in across events on the SAME encoder.
        let mut encoder = MouseEncoderState::new().expect("mouse encoder");
        // First move into cell (5, 3) with left held: +32 form of button 0 = 32.
        let first = encode_mouse_input(
            &term,
            &mut encoder,
            mouse_input_at(
                mouse::Action::Motion,
                Some(mouse::Button::Left),
                44.0,
                60.0,
                true,
            ),
        )
        .expect("motion");
        assert_eq!(first, b"\x1b[<32;6;4M");
        // Second move inside the SAME cell encodes nothing (track_last_cell).
        let second = encode_mouse_input(
            &term,
            &mut encoder,
            mouse_input_at(
                mouse::Action::Motion,
                Some(mouse::Button::Left),
                46.0,
                62.0,
                true,
            ),
        )
        .expect("same-cell motion");
        assert_eq!(second, Vec::<u8>::new());
        // Moving into a different cell encodes again.
        let third = encode_mouse_input(
            &term,
            &mut encoder,
            mouse_input_at(
                mouse::Action::Motion,
                Some(mouse::Button::Left),
                88.0,
                96.0,
                true,
            ),
        )
        .expect("next-cell motion");
        assert_eq!(third, b"\x1b[<32;12;6M");
    }

    #[test]
    fn any_event_motion_requires_1003_mode() {
        let mut term = headless_term(80, 24);
        advance_headless(&mut term, b"\x1b[?1003h\x1b[?1006h");
        let with_any_event = encoded_mouse(
            &term,
            mouse_input_at(mouse::Action::Motion, None, 44.0, 60.0, false),
        );
        // No-button motion is the +32 form of button 3 = 35.
        assert_eq!(with_any_event, b"\x1b[<35;6;4M");

        // Button-event tracking alone must not report no-button motion.
        let mut without_any_event = headless_term(80, 24);
        advance_headless(&mut without_any_event, b"\x1b[?1002h\x1b[?1006h");
        let without = encoded_mouse(
            &without_any_event,
            mouse_input_at(mouse::Action::Motion, None, 44.0, 60.0, false),
        );
        assert_eq!(without, Vec::<u8>::new());
    }

    #[test]
    fn wheel_ticks_encode_four_and_five_presses() {
        let mut term = headless_term(80, 24);
        advance_headless(&mut term, b"\x1b[?1002h\x1b[?1006h");
        let up = encoded_mouse(
            &term,
            mouse_input_at(
                mouse::Action::Press,
                Some(mouse::Button::Four),
                44.0,
                60.0,
                false,
            ),
        );
        assert_eq!(up, b"\x1b[<64;6;4M");
        let down = encoded_mouse(
            &term,
            mouse_input_at(
                mouse::Action::Press,
                Some(mouse::Button::Five),
                44.0,
                60.0,
                false,
            ),
        );
        assert_eq!(down, b"\x1b[<65;6;4M");
    }

    #[test]
    fn without_tracking_modes_the_same_inputs_encode_zero_bytes() {
        let term = headless_term(80, 24);
        let inputs = [
            (mouse::Action::Press, Some(mouse::Button::Left)),
            (mouse::Action::Release, Some(mouse::Button::Left)),
            (mouse::Action::Press, Some(mouse::Button::Four)),
        ];
        for (action, button) in inputs {
            let bytes = encoded_mouse(
                &term,
                mouse_input_at(action, button, 44.0, 60.0, button.is_some()),
            );
            assert_eq!(bytes, Vec::<u8>::new());
        }
    }

    #[test]
    fn shift_bypasses_reporting_in_the_conversion() {
        let bounds = Bounds {
            origin: point(px(10.0), px(20.0)),
            size: size(px(640.0), px(360.0)),
        };
        let shift = gpui::Modifiers {
            shift: true,
            ..Default::default()
        };
        // Whatever the tracking state, shift-held events stay Sirio's.
        assert_eq!(
            mouse_input(
                mouse::Action::Press,
                Some(mouse::Button::Left),
                Some(MouseButton::Left),
                point(px(54.0), px(80.0)),
                shift,
                bounds,
                px(8.0),
                px(18.0),
            ),
            None
        );
        // Without shift it converts, undoing the pane origin.
        let converted = mouse_input(
            mouse::Action::Press,
            Some(mouse::Button::Left),
            Some(MouseButton::Left),
            point(px(54.0), px(80.0)),
            gpui::Modifiers::default(),
            bounds,
            px(8.0),
            px(18.0),
        )
        .expect("guest-bound");
        assert_eq!(converted.x, 44.0);
        assert_eq!(converted.y, 60.0);
        assert!(converted.any_button_pressed);
        // The right button stays Sirio's even without shift.
        assert_eq!(
            mouse_input(
                mouse::Action::Press,
                Some(mouse::Button::Right),
                Some(MouseButton::Right),
                point(px(54.0), px(80.0)),
                gpui::Modifiers::default(),
                bounds,
                px(8.0),
                px(18.0),
            ),
            None
        );
    }

    fn resize_headless(term: &mut Terminal<'static, 'static>, columns: u16, lines: u16) {
        term.resize(columns, lines, 8, 18)
            .expect("resize headless terminal");
    }

    // -----------------------------------------------------------------------
    // Headless paint-frame harness (#40 second half, #45): feeds known byte
    // streams into a bare Terminal through vt_write exactly like the owner
    // thread does, then reads back through build_snapshot — the same private
    // function the owner thread serves Snapshot commands with. No PTY, no
    // window, runs on Windows.
    // -----------------------------------------------------------------------

    fn paint_frame(term: &mut Terminal<'static, 'static>) -> Vec<Vec<SnapshotCell>> {
        let mut render = RenderState::new().expect("RenderState");
        let mut rows_iterator = RowIterator::new().expect("RowIterator");
        let mut cells_iterator = CellIterator::new().expect("CellIterator");
        build_snapshot(term, &mut render, &mut rows_iterator, &mut cells_iterator).0
    }

    /// One row's rendered text through SnapshotCell::chars — what gpui shapes.
    fn frame_row_text(row: &[SnapshotCell]) -> String {
        row.iter().flat_map(|cell| cell.chars()).collect()
    }

    // -----------------------------------------------------------------------
    // OSC 8 hyperlink boundary tests (#41): same headless shape as the #40
    // boundary tests above — bytes straight through vt_write, read back
    // through build_snapshot + the free `resolve_link` that serves clicks.
    // -----------------------------------------------------------------------

    #[test]
    fn osc8_display_text_resolves_to_target_uri_not_the_display_text() {
        let mut term = headless_term(80, 24);
        advance_headless(
            &mut term,
            b"\x1b]8;;https://real.test/issue\x07click\x1b]8;;\x07 plain\r\n",
        );
        let frame = paint_frame(&mut term);
        assert_eq!(frame_row_text(&frame[0]).trim_end(), "click plain");
        // Every cell of the display run carries the TARGET; a cell outside
        // the run carries nothing.
        assert_eq!(
            frame[0][0].hyperlink.as_deref(),
            Some("https://real.test/issue"),
            "the linked cell's hyperlink must be the OSC 8 target"
        );
        assert_eq!(
            resolve_link(&frame, 0, 0),
            Some("https://real.test/issue".to_string()),
            "display text 'click' must not leak out as the resolved link"
        );
        assert_eq!(resolve_link(&frame, 0, 7), None);
    }

    #[test]
    fn cells_after_the_osc8_terminator_do_not_carry_the_link() {
        let mut term = headless_term(80, 24);
        advance_headless(
            &mut term,
            b"\x1b]8;;https://real.test/x\x1b\\linked\x1b]8;;\x1b\\after\r\n",
        );
        let frame = paint_frame(&mut term);
        assert_eq!(frame_row_text(&frame[0]).trim_end(), "linkedafter");
        assert_eq!(
            resolve_link(&frame, 0, 3),
            Some("https://real.test/x".to_string())
        );
        for column in 6..11 {
            assert!(
                frame[0][column].hyperlink.is_none(),
                "column {column} is past the OSC 8 terminator and must not carry the link"
            );
            assert_eq!(resolve_link(&frame, 0, column), None);
        }
    }

    #[test]
    fn bare_https_url_without_osc8_still_resolves_via_regex_fallback() {
        let mut term = headless_term(80, 24);
        advance_headless(&mut term, b"see https://example.test/docs end\r\n");
        let frame = paint_frame(&mut term);
        assert!(
            frame
                .iter()
                .all(|row| row.iter().all(|c| c.hyperlink.is_none())),
            "no cell may carry an OSC 8 link when none was emitted"
        );
        assert_eq!(
            resolve_link(&frame, 0, 6),
            Some("https://example.test/docs".to_string()),
            "a bare URL must stay clickable through the regex fallback"
        );
        assert_eq!(resolve_link(&frame, 0, 2), None);
    }

    #[test]
    fn osc8_target_wins_when_display_text_also_regex_matches_a_url() {
        let mut term = headless_term(80, 24);
        advance_headless(
            &mut term,
            b"\x1b]8;;https://target.test/a\x07https://display.test/b\x1b]8;;\x07\r\n",
        );
        let frame = paint_frame(&mut term);
        assert_eq!(
            resolve_link(&frame, 0, 4),
            Some("https://target.test/a".to_string()),
            "when both signals fire on one cell the OSC 8 target must win"
        );
    }

    /// Which sentinels of `B40_LINE_000..NNN` are missing from the text.
    fn missing_lines(text: &str, count: usize) -> Vec<String> {
        (0..count)
            .map(|i| format!("B40_LINE_{i:03}"))
            .filter(|sentinel| !text.contains(sentinel.as_str()))
            .collect()
    }

    #[test]
    fn reflow_narrowing_preserves_content() {
        const LINES: usize = 10;
        let mut term = headless_term(80, 24);
        let feed: String = (0..LINES).map(|i| format!("B40_LINE_{i:03}\r\n")).collect();
        advance_headless(&mut term, feed.as_bytes());

        resize_headless(&mut term, 40, 24);
        let text = capture_scrollback_text(&mut term);
        let lost = missing_lines(&text, LINES);
        assert!(
            lost.is_empty(),
            "after narrowing 80→40 columns these numbered lines were lost from the recovered text: {lost:?}"
        );
    }

    #[test]
    fn reflow_widening_preserves_content() {
        const LINES: usize = 10;
        let mut term = headless_term(40, 24);
        // One line longer than 40 columns forces a wrap that widening must
        // rejoin; the rest are short numbered markers.
        let mut feed = format!("B40_LINE_000 {}\r\n", "x".repeat(60));
        feed.push_str(
            &(1..LINES)
                .map(|i| format!("B40_LINE_{i:03}\r\n"))
                .collect::<String>(),
        );
        advance_headless(&mut term, feed.as_bytes());

        resize_headless(&mut term, 80, 24);
        let text = capture_scrollback_text(&mut term);
        assert!(
            text.contains("B40_LINE_000 xxxxx"),
            "after widening 40→80 columns the wrapped long line did not rejoin; it reads as separate rows instead"
        );
        let lost = missing_lines(&text, LINES);
        assert!(
            lost.is_empty(),
            "after widening 40→80 columns these numbered lines were lost from the recovered text: {lost:?}"
        );
    }

    #[test]
    fn a_line_longer_than_the_width_wraps_and_stays_recoverable() {
        const START: &str = "B40_WRAP_START";
        const END: &str = "B40_WRAP_END";
        let mut term = headless_term(80, 24);
        advance_headless(
            &mut term,
            format!("{START}{}{END}\r\n", "-".repeat(200)).as_bytes(),
        );

        let text = capture_scrollback_text(&mut term);
        assert!(
            text.contains(START) && text.contains(END),
            "a {START}…{END} line longer than 80 columns lost one of its ends when wrapped"
        );

        resize_headless(&mut term, 40, 24);
        let text = capture_scrollback_text(&mut term);
        assert!(
            text.contains(START) && text.contains(END),
            "after wrapping at 80 columns then narrowing to 40, the long line lost one of its ends"
        );
    }

    #[test]
    fn scrollback_retains_oldest_and_newest_beyond_the_viewport() {
        const LINES: usize = 100;
        let mut term = headless_term(80, 24); // viewport holds 24 rows
        let feed: String = (0..LINES).map(|i| format!("B40_LINE_{i:03}\r\n")).collect();
        advance_headless(&mut term, feed.as_bytes());

        let text = capture_scrollback_text(&mut term);
        let lost = missing_lines(&text, LINES);
        assert!(
            lost.is_empty(),
            "100 lines pushed through a 24-row viewport lost these from scrollback: {lost:?}"
        );
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

    // -----------------------------------------------------------------------
    // #40 second half + #45: graphemes and SGR through the paint loop, wide-
    // char width and spacer placement, capture normalisation, reflow on
    // visible TEXT. Written against CORRECT behaviour, never pinning current
    // behaviour (the oracle #35 rejected).
    // -----------------------------------------------------------------------

    const FAMILY_EMOJI: &str = "👨‍👩‍👧‍👦"; // 7 codepoints, 3 ZWJ joins
    const FLAG_EMOJI: &str = "🇮🇹"; // 2 regional indicators

    fn cell_chars(cell: &SnapshotCell) -> Vec<char> {
        cell.chars().collect()
    }

    #[test]
    fn zwj_emoji_and_flag_reach_the_paint_frame_as_single_clusters() {
        let mut term = headless_term(80, 24);
        advance_headless(
            &mut term,
            format!("{FAMILY_EMOJI}{FLAG_EMOJI}END\r\n").as_bytes(),
        );

        let rows = paint_frame(&mut term);
        assert!(!rows.is_empty());
        let row = &rows[0];
        let rendered: Vec<Vec<char>> = row.iter().map(cell_chars).collect();

        assert_eq!(
            rendered[0],
            FAMILY_EMOJI.chars().collect::<Vec<_>>(),
            "the ZWJ family emoji must reach the paint frame as ONE cell holding the full \
             cluster; got {:?} — a one-char-per-cell assumption is dropping combining codepoints",
            rendered[0]
        );
        assert_eq!(
            rendered[1],
            vec![' '],
            "column 1 must be the wide glyph's continuation half rendering as a space; got {:?}",
            rendered[1]
        );
        assert_eq!(
            rendered[2],
            FLAG_EMOJI.chars().collect::<Vec<_>>(),
            "the flag must reach the paint frame as ONE cell holding both regional indicators; got {:?}",
            rendered[2]
        );
        assert_eq!(
            rendered[3],
            vec![' '],
            "column 3 must be the flag's continuation half rendering as a space; got {:?}",
            rendered[3]
        );
        let tail: String = rendered[4..7].iter().map(|c| c[0]).collect();
        assert_eq!(
            tail, "END",
            "text after the wide glyphs must land four columns later, one continuation column each"
        );
    }

    /// PINNED DIVERGENCE: libghostty-vt documents SpacerTail as "Do not
    /// render." Sirio pushes a SPACE instead — gpui shapes whole lines, and
    /// only the placeholder keeps following columns aligned. This test pins
    /// Sirio's model so a port cannot silently erase the distinction.
    #[test]
    fn spacer_tail_renders_as_a_space_so_columns_stay_aligned() {
        let mut term = headless_term(80, 24);
        advance_headless(&mut term, "漢字\r\n".as_bytes());

        let rows = paint_frame(&mut term);
        assert!(!rows.is_empty());
        let text = frame_row_text(&rows[0]);
        assert!(
            text.starts_with("漢 字 "),
            "each wide glyph must be followed by a space placeholder for its SpacerTail cell \
             (libghostty-vt says 'Do not render.', Sirio renders a space to keep gpui's line \
             shaping column-aligned); the row instead reads {text:?}"
        );
    }

    #[test]
    fn invisible_cells_render_blank_but_keep_their_column() {
        let mut term = headless_term(80, 24);
        advance_headless(&mut term, b"\x1b[8mAB\x1b[0mC\r\n");

        let rows = paint_frame(&mut term);
        assert!(!rows.is_empty());
        let text = frame_row_text(&rows[0]);
        assert!(
            text.starts_with("  C"),
            "SGR 8 (invisible) must skip the glyph yet keep both columns so C lands at column 2; \
             the row reads {text:?}"
        );
    }

    #[test]
    fn inverse_swaps_foreground_and_background_in_the_paint_frame() {
        let mut term = headless_term(80, 24);
        advance_headless(&mut term, b"\x1b[7mR\x1b[0mN\r\n");

        let rows = paint_frame(&mut term);
        assert!(!rows.is_empty());
        assert_eq!(
            rows[0][0].bg,
            SirioColor::Named(NamedColor::Foreground),
            "SGR 7 must swap: the reversed cell paints the default FOREGROUND as its background"
        );
        assert_eq!(
            rows[0][0].fg,
            SirioColor::Named(NamedColor::Background),
            "SGR 7 must swap: the reversed cell paints the default BACKGROUND as its foreground"
        );
        assert_eq!(
            rows[0][1].bg,
            SirioColor::Named(NamedColor::Background),
            "the cell after SGR 0 must keep its normal colors"
        );
    }

    #[test]
    fn faint_marks_the_cell_for_alpha_reduction() {
        let mut term = headless_term(80, 24);
        advance_headless(&mut term, b"\x1b[2mF\x1b[0m\r\n");

        let rows = paint_frame(&mut term);
        assert!(!rows.is_empty());
        assert!(
            rows[0][0].faint,
            "SGR 2 (faint) must mark the cell so the renderer halves its alpha"
        );
    }

    #[test]
    fn sgr_attributes_reach_paint_frame_cells() {
        // Bold + italic + strikethrough + underlined + 256-color fg (196) +
        // 256-color underline color (46), all in one SGR.
        let mut term = headless_term(80, 24);
        advance_headless(&mut term, b"\x1b[1;3;9;4;38;5;196;58;5;46mX\r\n");

        let rows = paint_frame(&mut term);
        assert!(!rows.is_empty());
        let cell = &rows[0][0];
        assert!(cell.bold, "SGR 1 bold must reach the paint frame");
        assert!(cell.italic, "SGR 3 italic must reach the paint frame");
        assert!(
            cell.strikethrough,
            "SGR 9 strikethrough must reach the paint frame"
        );
        assert_eq!(
            cell.underline,
            Underline::Single,
            "SGR 4 underline must reach the paint frame as Underline::Single"
        );
        assert_eq!(
            cell.fg,
            SirioColor::Rgb(
                indexed_color(196).0,
                indexed_color(196).1,
                indexed_color(196).2
            ),
            "SGR 38;5;196 foreground must resolve through the palette to RGB"
        );
        assert_eq!(
            cell.underline_color,
            Some(SirioColor::Rgb(
                indexed_color(46).0,
                indexed_color(46).1,
                indexed_color(46).2
            )),
            "SGR 58;5;46 underline color must resolve through the palette to RGB"
        );
    }

    #[test]
    fn curly_underline_keeps_its_color_through_degradation() {
        let mut term = headless_term(80, 24);
        // SGR 4:3 (curly) with SGR 58 palette color 201.
        advance_headless(&mut term, b"\x1b[4:3;58;5;201mW\r\n");

        let rows = paint_frame(&mut term);
        assert!(!rows.is_empty());
        let cell = &rows[0][0];
        assert_eq!(
            cell.underline,
            Underline::Curly,
            "SGR 4:3 must arrive as Underline::Curly before degradation maps it to wavy"
        );
        assert!(
            cell.underline_color.is_some(),
            "an SGR 58 underline color set alongside 4:3 must not be dropped"
        );
    }

    #[test]
    fn underline_styles_degrade_to_gpui_ceiling_never_drop() {
        // #31: double/dotted/dashed degrade to SINGLE; curly stays wavy.
        let mapped = |u: Underline| underline_style(u, None);
        assert!(
            mapped(Underline::None).is_none(),
            "no requested underline must produce no underline"
        );
        assert!(
            !mapped(Underline::Single).unwrap().wavy,
            "single must stay a straight underline"
        );
        for degraded in [Underline::Double, Underline::Dotted, Underline::Dashed] {
            let style = mapped(degraded).unwrap_or_else(|| {
                panic!("{degraded:?} degraded away entirely — #31 says degrade, never drop")
            });
            assert!(
                !style.wavy,
                "{degraded:?} must degrade to a straight single underline"
            );
        }
        assert!(
            mapped(Underline::Curly).unwrap().wavy,
            "curly must stay wavy — it is one of the two styles gpui expresses natively"
        );
        let red = rgb_to_hsla((255, 0, 0));
        assert_eq!(
            underline_style(Underline::Single, Some(red)).unwrap().color,
            Some(red),
            "the SGR 58 underline color must ride along into the gpui style"
        );
    }

    #[test]
    fn capture_scrollback_normalises_known_bytes_with_clusters_intact() {
        let mut term = headless_term(80, 24);
        advance_headless(
            &mut term,
            format!("A{FAMILY_EMOJI}B{FLAG_EMOJI}C   \r\n").as_bytes(),
        );

        let text = capture_scrollback_text(&mut term);
        assert_eq!(
            text.lines().next().unwrap_or(""),
            format!("A{FAMILY_EMOJI} B{FLAG_EMOJI} C"),
            "Layer C reads capture_scrollback_text, so the ZWJ family and the flag must survive \
             capture as complete clusters with no spacer artifacts beyond one space per wide-glyph \
             continuation half, and trailing blanks must be trimmed"
        );
    }

    #[test]
    fn reflow_after_resize_keeps_visible_text_readable_through_paint_frames() {
        const START: &str = "B45_REFL_A";
        const END: &str = "B45_REFL_B";
        // One 70-char logical line wraps at 40 columns into two physical rows.
        let mut term = headless_term(40, 24);
        advance_headless(
            &mut term,
            format!("{START}{}{END}\r\n", "0".repeat(50)).as_bytes(),
        );

        resize_headless(&mut term, 80, 24);
        let rows = paint_frame(&mut term);
        assert!(!rows.is_empty());
        let rejoined = rows.iter().any(|row| {
            let text = frame_row_text(row);
            text.contains(START) && text.contains(END)
        });
        assert!(
            rejoined,
            "after widening 40→80 columns the wrapped line must rejoin into ONE physical row \
             containing both sentinels {START:?} and {END:?}; rows read {:?}",
            rows.iter().map(|r| frame_row_text(r)).collect::<Vec<_>>()
        );

        resize_headless(&mut term, 40, 24);
        let rows = paint_frame(&mut term);
        let visible: String = rows
            .iter()
            .map(|row| frame_row_text(row))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            visible.contains(START) && visible.contains(END),
            "after narrowing back 80→40 columns both sentinels must still be visible in the \
             viewport text; it reads {visible:?}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_descendant_pids_enumerates_a_real_child_process() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("5")
            .spawn()
            .expect("spawn child process");

        let parent_pid = std::process::id() as libc::pid_t;
        let mut descendants = Vec::new();
        for _ in 0..50 {
            descendants = descendant_pids(parent_pid);
            if descendants.contains(&(child.id() as libc::pid_t)) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(
            descendants.contains(&(child.id() as libc::pid_t)),
            "macOS child enumeration must find spawned child pid {} in {:?}",
            child.id(),
            descendants
        );
        child.kill().expect("kill child process");
        child.wait().expect("reap child process");
    }

    fn palette() -> TerminalPalette {
        TerminalPalette::from_theme(&Theme::light())
    }

    #[test]
    fn terminal_child_receives_the_pane_id_environment() {
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-test-pane-env-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = pty_fixture_shell(
            "printf 'pane=%s\\n' \"$SIRIO_PANE_ID\"; exec sleep 0.1",
            &[
                "print",
                "pane=${SIRIO_PANE_ID}\\n",
                "--expand",
                "--sleep",
                "0.1",
            ],
        );
        let (handle, mut events) =
            TerminalHandle::new_with_pane_id(&working_directory, &shell, Some("pane-real-env"))
                .expect("spawn PTY");

        futures::executor::block_on(async {
            while let Some(event) = events.next().await {
                if matches!(event, TerminalEvent::ChildExit(_)) {
                    break;
                }
            }
        });

        assert!(
            String::from_utf8_lossy(&handle.capture_scrollback()).contains("pane=pane-real-env"),
            "the PTY child must inherit SIRIO_PANE_ID"
        );
        handle.shutdown();
        let _ = std::fs::remove_dir_all(working_directory);
    }

    #[test]
    fn scrollback_capture_poll_returns_before_the_owner_replies() {
        let (commands, command_rx) = std::sync::mpsc::channel();
        let source = ScrollbackCapture::new(commands);
        assert!(
            source.try_capture().is_none(),
            "the poll must return while the owner is still busy"
        );

        let command = command_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("capture must enqueue one owner request");
        let TerminalCommand::Text(reply) = command else {
            panic!("capture must enqueue a text request");
        };
        reply
            .send("CAPTURE_READY".to_string())
            .expect("the pending capture reply receiver must stay alive");

        assert_eq!(source.capture(), b"CAPTURE_READY");
    }

    #[test]
    fn missed_scrollback_capture_is_retried_and_produces_a_later_frame() {
        let (commands, command_rx) = std::sync::mpsc::channel();
        let source = ScrollbackCapture::new(commands);

        // The burst has ended, but the owner is still busy. The first poll
        // queues the capture and must return without a frame.
        assert!(source.try_capture().is_none());
        let command = command_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("capture must enqueue one owner request");
        let TerminalCommand::Text(reply) = command else {
            panic!("capture must enqueue a text request");
        };
        assert!(source.try_capture().is_none());

        // A missed capture selects the short retry timer. Once the owner
        // finishes, that later pump can obtain and render the final frame.
        assert_eq!(
            terminal_pump_interval(true),
            SCROLLBACK_CAPTURE_RETRY_INTERVAL
        );
        reply
            .send("FINAL_FRAME".to_string())
            .expect("the pending capture reply receiver must stay alive");
        let mut rendered_frames = Vec::new();
        if let Some(frame) = source.try_capture() {
            rendered_frames.push(frame);
        }

        assert_eq!(rendered_frames, vec![b"FINAL_FRAME".to_vec()]);
        assert_eq!(terminal_pump_interval(false), EVENT_POLL_INTERVAL);
    }

    #[test]
    fn terminal_defaults_follow_the_theme_but_ansi_colors_do_not() {
        let palette = palette();
        assert_eq!(
            color_to_hsla(SirioColor::Named(NamedColor::Background), palette),
            palette.background
        );
        assert_eq!(
            color_to_hsla(SirioColor::Named(NamedColor::Foreground), palette),
            palette.foreground
        );
        assert_ne!(
            color_to_hsla(SirioColor::Named(NamedColor::Red), palette),
            palette.foreground
        );
        assert_ne!(
            color_to_hsla(SirioColor::Indexed(196), palette),
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

    /// F-TERM-03: a shell is alive for the pane's whole life, so "a live
    /// child exists" would report "running" forever — the exact bug this
    /// asserts is fixed. Without `foreground_command_running`'s
    /// `tcgetpgrp`-based definition this test would fail (the method did not
    /// exist before this change; any process-existence stand-in would return
    /// `true` here, since the shell itself is always alive).
    ///
    /// unix only, and genuinely so: the subject is
    /// `TerminalHandle::foreground_command_running`, whose definition *is*
    /// `tcgetpgrp` on the PTY master — POSIX job control, which Windows has
    /// no equivalent of. There the method is `#[cfg(not(unix))] { false }`
    /// by construction (see its own doc comment), so an "idle prompt
    /// reports false" assertion would pass there for a reason that has
    /// nothing to do with what this test is about, and its sibling below
    /// could never pass at all. Gating suppresses no coverage that could
    /// exist today.
    #[cfg(unix)]
    #[test]
    fn foreground_command_running_is_false_at_an_idle_prompt() {
        let working_directory = test_working_directory("idle-prompt");
        std::fs::create_dir_all(&working_directory).unwrap();
        // An interactive shell run directly as the PTY child (no outer shell
        // in between) so job control is active without depending on
        // `$SHELL` in the test environment.
        let shell = TerminalShell::WithArguments {
            program: "/bin/bash".to_string(),
            args: vec![
                "--norc".to_string(),
                "--noprofile".to_string(),
                "-i".to_string(),
            ],
        };
        let (handle, _events) = TerminalHandle::new(&working_directory, &shell).unwrap();

        // Drain bash's own startup (it writes nothing to stdout by default
        // with --norc, but give the fork/exec and setsid a moment to settle
        // before asserting on process-group state).
        std::thread::sleep(Duration::from_millis(500));

        assert!(
            !handle.foreground_command_running(),
            "an interactive shell sitting at its prompt must not report a foreground command"
        );
        handle.shutdown();
        let _ = std::fs::remove_dir_all(working_directory);
    }

    /// F-TERM-03: proves the positive case (`true` while a foreground
    /// command holds the PTY) and that the signal is self-correcting once
    /// the command finishes and the shell reclaims the foreground process
    /// group — not merely a latch that flips on and stays on. Both this and
    /// the previous test exercise `foreground_command_running` directly
    /// because it is `TerminalHandle`-private mechanism; the public,
    /// render-facing surface (`TerminalView::is_command_running`) is covered
    /// by `running_pill_state_is_replaced_by_exit_status_when_the_child_exits`
    /// below, which also proves the exit-state hand-off Swift's
    /// `statusChip` ordering requires.
    ///
    /// unix only for the same reason as its sibling above: `tcgetpgrp` and
    /// the foreground process group it reads have no Windows equivalent, so
    /// `foreground_command_running` is a hardcoded `false` there and the
    /// positive case this test exists for cannot occur.
    #[cfg(unix)]
    #[test]
    fn foreground_command_running_is_true_while_a_command_executes_then_false_again() {
        let working_directory = test_working_directory("running-then-idle");
        std::fs::create_dir_all(&working_directory).unwrap();
        let shell = TerminalShell::WithArguments {
            program: "/bin/bash".to_string(),
            args: vec![
                "--norc".to_string(),
                "--noprofile".to_string(),
                "-i".to_string(),
            ],
        };
        let (handle, _events) = TerminalHandle::new(&working_directory, &shell).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert!(
            !handle.foreground_command_running(),
            "must start out idle before the probe command is sent"
        );

        handle.write(b"cat >/dev/null\n".to_vec());

        let running_deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut observed_running = false;
        while std::time::Instant::now() < running_deadline {
            if handle.foreground_command_running() {
                observed_running = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            observed_running,
            "sleep 3 must be observed as the PTY's foreground process group within 2s"
        );
        handle.write(vec![4]);

        let idle_deadline = std::time::Instant::now() + Duration::from_secs(6);
        let mut observed_idle_again = false;
        while std::time::Instant::now() < idle_deadline {
            if !handle.foreground_command_running() {
                observed_idle_again = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            observed_idle_again,
            "the shell must reclaim the foreground process group once sleep 3 exits"
        );
        handle.shutdown();
        let _ = std::fs::remove_dir_all(working_directory);
    }

    #[test]
    fn scrollback_can_be_viewed_after_output_exceeds_the_viewport() {
        let working_directory = test_working_directory("scroll");
        std::fs::create_dir_all(&working_directory).unwrap();
        let shell = pty_fixture_shell(
            "i=0; while [ \"$i\" -lt 100 ]; do printf 'P4_SCROLL_%03d\\n' \"$i\"; i=$((i + 1)); done",
            &["lines", "P4_SCROLL_", "100"],
        );
        let (handle, mut wakeup_rx) = TerminalHandle::new(&working_directory, &shell).unwrap();
        let output_reached_screen = futures::executor::block_on(async {
            while let Some(event) = wakeup_rx.next().await {
                if matches!(event, TerminalEvent::Wakeup)
                    && screen_text(&handle).contains("P4_SCROLL_099")
                {
                    return true;
                }
                if matches!(event, TerminalEvent::ChildExit(_)) {
                    return screen_text(&handle).contains("P4_SCROLL_099");
                }
            }
            false
        });
        assert!(
            output_reached_screen,
            "the command output never reached the terminal grid"
        );
        let captured_bytes = handle.capture_scrollback();
        let captured = String::from_utf8_lossy(&captured_bytes);
        assert!(
            captured.contains("P4_SCROLL_000") && captured.contains("P4_SCROLL_099"),
            "capture should include both retained history and the newest screen output"
        );

        handle.scroll_display(SirioScroll::Top);
        let viewport_changed = (0..100).any(|_| match wakeup_rx.try_recv() {
            Ok(TerminalEvent::ViewportChanged) => true,
            Ok(_) => false,
            Err(TryRecvError::Empty) => {
                std::thread::sleep(Duration::from_millis(1));
                false
            }
            Err(TryRecvError::Closed) => false,
        });
        assert!(
            viewport_changed,
            "scrolling must notify the view so the new viewport is painted"
        );
        assert!(
            screen_text(&handle).contains("P4_SCROLL_000"),
            "scrolling to the top should expose the oldest retained line"
        );

        handle.scroll_display(SirioScroll::Bottom);
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
            std::env::temp_dir().join(format!("sirio-terminal-test-state-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).unwrap();
        let nonce = format!("P30_SCROLLBACK_NONCE_{}", std::process::id());
        let shell = pty_fixture_shell(
            &format!("printf '%s\\n' '{nonce}'; exec sleep 60"),
            &["print", &format!("{nonce}\\n")],
        );
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
        assert!(
            String::from_utf8_lossy(&captured).contains(&nonce),
            "capture must contain the nonce"
        );
        let (restored, _restored_events) = TerminalHandle::new(
            &working_directory,
            &pty_fixture_shell("exec sleep 60", &["sleep", "inf"]),
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

    #[test]
    fn replayed_scrollback_keeps_saved_lines_left_aligned() {
        let working_directory = test_working_directory("replay-scrollback-columns");
        std::fs::create_dir_all(&working_directory).unwrap();
        let (restored, _restored_events) = TerminalHandle::new(
            &working_directory,
            &pty_fixture_shell("exec sleep 60", &["sleep", "inf"]),
        )
        .unwrap();
        // This is the plain-text format produced by capture_scrollback_text
        // and stored in session state: rows are newline-delimited, without
        // terminal control sequences or PTY input semantics.
        let captured = b"RESTORE_LINE_A\nRESTORE_LINE_B\nRESTORE_LINE_C";
        restored.replay_scrollback(captured);
        let (rows, _) = restored.snapshot();

        for marker in ["RESTORE_LINE_A", "RESTORE_LINE_B", "RESTORE_LINE_C"] {
            let row = rows
                .iter()
                .find(|row| frame_row_text(row).contains(marker))
                .unwrap_or_else(|| panic!("replayed scrollback is missing {marker}"));
            assert_eq!(
                frame_row_text(row).find(marker),
                Some(0),
                "replayed {marker} must start at column zero"
            );
        }

        restored.shutdown();
        let _ = std::fs::remove_dir_all(working_directory);
    }

    /// The event-driven redraw contract, tested at the mechanism level: PTY
    /// output must surface as events on the wakeup channel the view's pump
    /// parks on. This is what lets an idle terminal cost nothing (the pump
    /// waits on the channel instead of polling) while output still wakes the
    /// view promptly.
    #[test]
    fn pty_output_wakes_the_event_pump() {
        let working_directory =
            std::env::temp_dir().join(format!("sirio-terminal-test-{}", std::process::id()));
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
        let working_directory =
            std::env::temp_dir().join(format!("sirio-terminal-test-resize-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).unwrap();
        let shell = size_probe_shell();
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
            std::env::temp_dir().join(format!("sirio-terminal-test-args-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).unwrap();

        let shell = argv_probe_shell("ARGV_PROBE");
        let (handle, mut wakeup_rx) = TerminalHandle::new(&working_directory, &shell).unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match wakeup_rx.try_recv() {
                Ok(_) => {
                    let (cells, _) = handle.snapshot();
                    let screen: String = cells
                        .iter()
                        .flat_map(|row| row.iter().flat_map(|cell| cell.chars()))
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
            "sirio-terminal-test-shutdown-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).unwrap();
        let pid_file = working_directory.join("child.pid");
        let shell = pty_fixture_shell(
            &format!("printf '%s' \"$$\" > {}; exec sleep 60", pid_file.display()),
            &[
                "pid-file",
                &pid_file.display().to_string(),
                "--sleep",
                "inf",
            ],
        );
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

    /// unix only, and genuinely so rather than for convenience: the subject
    /// is POSIX process-group teardown (`getpgid`/`killpg`), which Windows
    /// does not have. `terminate_descendant_process_groups` is a documented
    /// no-op there pending the Job Object design named in its own comment,
    /// so there is no Windows behaviour for this to assert against yet —
    /// gating it suppresses no coverage that could exist today.
    #[cfg(unix)]
    #[test]
    fn shutdown_terminates_the_entire_pty_process_group() {
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-test-process-group-{}",
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
    /// unix only for the same reason as its sibling above: `setsid` and the
    /// separate-process-group behaviour it reproduces are POSIX notions.
    #[cfg(unix)]
    #[test]
    fn shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group() {
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-test-detached-group-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).unwrap();
        let pid_file = working_directory.join("detached.pid");
        let detached_process = if cfg!(target_os = "macos") {
            format!(
                "python3 -c 'exec(\"import os,time\\nchild=os.fork()\\nif child == 0:\\n os.setsid()\\n open(\\\"{}\\\",\\\"w\\\").write(str(os.getpid()))\\n time.sleep(60)\\nelse:\\n os.waitpid(child, 0)\")'",
                pid_file.display()
            )
        } else {
            format!(
                "setsid sh -c 'printf \"%s\" \"$$\" > {}; exec sleep 60' & wait",
                pid_file.display()
            )
        };
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                format!("trap '' HUP; {detached_process} & wait"),
            ],
        };
        let (handle, _wakeup_rx) = TerminalHandle::new(&working_directory, &shell).unwrap();
        let process_group = ProcessGroupGuard(handle.shell_pid);

        let setup_deadline = std::time::Instant::now() + Duration::from_secs(5);
        let detached_pid = loop {
            if let Ok(pid) = std::fs::read_to_string(&pid_file)
                && let Ok(pid) = pid.parse::<i32>()
                && process_is_running(pid)
                && unsafe { libc::getpgid(pid) } != process_group.0 as libc::pid_t
            {
                break pid;
            }
            assert!(
                std::time::Instant::now() < setup_deadline,
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

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
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

    /// Cleanup helper for the two `cfg(unix)` process-group tests above; it
    /// signals a process group, so it has no meaning off unix.
    #[cfg(unix)]
    struct ProcessGroupGuard(u32);

    #[cfg(unix)]
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
    #[cfg(unix)]
    struct PidGuard(i32);

    #[cfg(unix)]
    impl Drop for PidGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = libc::kill(self.0, libc::SIGKILL);
            }
        }
    }

    /// Liveness probe for the `cfg(unix)` process-group tests above. Gated
    /// on `unix` as well as the OS split: every caller is unix-only, so on
    /// Windows this would be a dead `/proc` reader — and `-D warnings`
    /// rejects dead code.
    #[cfg(all(unix, not(target_os = "macos")))]
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

    #[cfg(target_os = "macos")]
    fn process_is_running(pid: i32) -> bool {
        let Ok(output) = std::process::Command::new("ps")
            .args(["-o", "state=", "-p", &pid.to_string()])
            .output()
        else {
            return false;
        };
        let state = String::from_utf8_lossy(&output.stdout);
        state
            .trim()
            .as_bytes()
            .first()
            .is_some_and(|state| *state != b'Z')
    }

    #[cfg(unix)]
    fn process_exists(pid: i32) -> bool {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    /// Windows has no `kill -0`, and the `kill` that a Git Bash install puts
    /// on `PATH` is an MSYS one that speaks MSYS pids — it would answer
    /// about the wrong process, or about none. `tasklist` is the shipped,
    /// always-present equivalent: it filters on the real Win32 pid and
    /// prints one CSV row per match, or an `INFO:` line when nothing
    /// matches. It costs a process spawn per probe, so the callers' polling
    /// loops simply run fewer, slower iterations inside the same deadline.
    #[cfg(not(unix))]
    fn process_exists(pid: i32) -> bool {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .stderr(std::process::Stdio::null())
            .output()
            .is_ok_and(|output| {
                String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
            })
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

    /// P129: every real pane except a single fullscreen one sits behind a
    /// non-zero window origin (the sidebar, the tab strip, sibling split
    /// panes). This fixture reproduces that with a fixed-width spacer to
    /// the terminal's left, standing in for the sidebar, so a context-menu
    /// regression that only shows up away from the window's top-left
    /// corner (like P129's origin-doubling bug) is actually exercised.
    struct NonZeroOriginFixture {
        terminal: gpui::Entity<TerminalView>,
    }

    impl gpui::Render for NonZeroOriginFixture {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            div()
                .size_full()
                .flex()
                .child(
                    div()
                        .id("terminal-test-origin-spacer")
                        .debug_selector(|| "terminal-test-origin-spacer".to_owned())
                        .w(px(280.0))
                        .h_full(),
                )
                .child(self.terminal.clone())
        }
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

    /// F-TERM-PTY-06, focus half. Same drag source as
    /// [`ExternalFileDropFixture`], plus a *competing* focusable field
    /// standing in for the real discriminator the lane uses: the sidebar's
    /// Filter box. Without a second focus target in the window there is
    /// nothing for the terminal to take focus back *from*, and the assertion
    /// would pass on a window whose only focusable element is the terminal.
    struct FocusReturnDropFixture {
        terminal: gpui::Entity<TerminalView>,
        paths: gpui::ExternalPaths,
        decoy_focus: gpui::FocusHandle,
    }

    impl gpui::Render for FocusReturnDropFixture {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            div()
                .size_full()
                .child(
                    div()
                        .id("terminal-focus-drag-source")
                        .debug_selector(|| "terminal-focus-drag-source".to_owned())
                        .h(px(40.0))
                        .on_drag(self.paths.clone(), |_, _, _, cx| cx.new(|_| gpui::Empty))
                        .child("external files source"),
                )
                .child(
                    div()
                        .id("decoy-filter-field")
                        .debug_selector(|| "decoy-filter-field".to_owned())
                        .h(px(40.0))
                        .track_focus(&self.decoy_focus)
                        .child("filter"),
                )
                .child(self.terminal.clone())
        }
    }

    /// F-TERM-PTY-06: the clause's second conjunct — a drop "returns focus to
    /// the terminal". Driven with the same discriminator the Wayland lane
    /// uses on the real compositor: park focus on another field first, drop,
    /// and see where focus ends up.
    #[gpui::test]
    async fn a_file_drop_returns_focus_to_the_terminal(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory =
            std::env::temp_dir().join(format!("sirio-terminal-drop-focus-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).expect("create drop directory");
        let shell = pty_fixture_shell("exec sleep 60", &["sleep", "inf"]);
        let dropped = gpui::ExternalPaths([PathBuf::from("src/one.rs")].into_iter().collect());
        let window = cx.add_window(|_, cx| {
            let terminal = cx.new(|cx| {
                TerminalView::with_shell(&working_directory, shell, cx).expect("spawn terminal")
            });
            FocusReturnDropFixture {
                terminal,
                paths: dropped.clone(),
                decoy_focus: cx.focus_handle(),
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let fixture = cx.update(|window, _| {
            window
                .root::<FocusReturnDropFixture>()
                .flatten()
                .expect("fixture root")
        });
        let terminal = fixture.read_with(&cx.cx, |fixture, _| fixture.terminal.clone());
        let decoy_focus = fixture.read_with(&cx.cx, |fixture, _| fixture.decoy_focus.clone());
        let terminal_focus =
            terminal.read_with(&cx.cx, |terminal, _| terminal.focus_handle.clone());

        // The control: focus starts somewhere else entirely.
        cx.update(|window, app| window.focus(&decoy_focus, app));
        cx.run_until_parked();
        assert!(
            cx.update(|window, _| decoy_focus.is_focused(window)),
            "the decoy field must really hold focus before the drop, or the \
             assertion below proves nothing"
        );
        assert!(
            !cx.update(|window, _| terminal_focus.is_focused(window)),
            "the terminal must not already hold focus before the drop"
        );

        let target = cx
            .debug_bounds("terminal-drop-target")
            .expect("terminal is a drawn drop target");
        let source = cx
            .debug_bounds("terminal-focus-drag-source")
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
            terminal.read_with(&cx.cx, |terminal, _| terminal
                .last_dropped_files()
                .map(<[PathBuf]>::to_vec)),
            Some(dropped.paths().to_vec()),
            "the paths half of the clause still holds"
        );
        assert!(
            cx.update(|window, _| terminal_focus.is_focused(window)),
            "the drop must return focus to the terminal, so the reader's next \
             keystroke completes the command it just pasted"
        );
        assert!(
            !cx.update(|window, _| decoy_focus.is_focused(window)),
            "and must take it away from whatever held it during the drag"
        );
        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
    }

    #[test]
    fn conflict_resolution_shell_quotes_the_exact_repo_relative_path() {
        let shell = conflict_resolution_shell(Path::new("src/conflicted file.txt"));
        let TerminalShell::WithArguments { program, args } = shell else {
            panic!("conflict launch must be a shell command");
        };
        // The user's own shell, not a hardcoded one. This used to assert
        // "/bin/sh", which pinned the defect: whoever resolved a conflict was
        // dropped into a POSIX-minimal shell with none of their own setup.
        //
        // "The user's own shell" is a POSIX notion, and the product says so.
        // `system_pane_shell` reads `$SHELL` on unix and deliberately
        // ignores it on Windows, where the value Git Bash exports
        // (`/bin/bash.exe`) is an MSYS path `CreateProcessW` cannot resolve
        // — honouring it made every pane fail to start (#230). Reading
        // `$SHELL` here without that guard asserted the opposite of what the
        // product promises, and under Git Bash it is always set. The command
        // flag splits the same way (`-lc` is POSIX; cmd.exe wants `/C`), so
        // both halves branch on the platform the product branches on.
        #[cfg(not(windows))]
        {
            if let Ok(configured) = std::env::var("SHELL")
                && !configured.is_empty()
            {
                assert_eq!(program, configured);
            } else {
                assert!(
                    std::path::Path::new(&program).exists(),
                    "the fallback must name a shell present on this platform, got: {program}"
                );
            }
            assert_eq!(args[0], "-lc");
        }
        #[cfg(windows)]
        {
            assert_eq!(
                program,
                std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
                "Windows resolves its interpreter from COMSPEC, never from a \
                 POSIX $SHELL an MSYS environment may have exported (#230)"
            );
            assert_eq!(args[0], "/C");
        }
        assert!(args[1].contains("git diff --cc -- 'src/conflicted file.txt'"));
        assert!(args[1].contains("Resolve conflict at"));
        assert!(
            args[1].contains(&format!("exec '{program}' -il")),
            "the interactive shell left behind must be the same one, got {}",
            args[1]
        );
    }

    /// The drawn terminal created for a conflict keeps the repository root
    /// and exact conflicted path in its launch contract.
    #[gpui::test]
    async fn a_conflict_terminal_is_drawn_with_the_exact_path(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let repo = std::env::temp_dir().join(format!(
            "sirio-terminal-conflict-launch-{}",
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
            "sirio-terminal-missing-{tag}-{}",
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

    /// The host uses the return value to avoid notifying an unchanged
    /// terminal while still notifying when pane-group membership changes.
    #[gpui::test]
    async fn set_sole_tab_in_group_reports_only_changes(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let missing = missing_directory("sole-tab-setter");
        let window = cx.add_window(|_, cx| {
            TerminalView::failed(
                &missing,
                TerminalShell::System,
                "sole-tab setter test",
                cx,
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let terminal = cx.update(|window, _| {
            window
                .root::<TerminalView>()
                .flatten()
                .expect("failed terminal root")
        });

        assert!(!terminal.update(&mut cx.cx, |terminal, _| {
            terminal.set_sole_tab_in_group(false)
        }));
        assert!(terminal.update(&mut cx.cx, |terminal, _| {
            terminal.set_sole_tab_in_group(true)
        }));
        assert!(!terminal.update(&mut cx.cx, |terminal, _| {
            terminal.set_sole_tab_in_group(true)
        }));
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

    /// F-TERM-PTY-04: `TerminalShell::System` prefers `$SHELL`, and falls back to
    /// a shell that actually exists on *this* platform when it is unset.
    ///
    /// The previous version of this test asserted the opposite — that the fallback
    /// names `/bin/zsh` and therefore *fails* to spawn here — using the absence of
    /// `/bin/zsh` on this box as an unfakeable discriminator. The discriminator was
    /// sound; the expectation was not. It enshrined a faithful port of macOS's
    /// default login shell as correct behaviour, when on Linux it meant a pane with
    /// `$SHELL` unset opened no shell at all.
    ///
    /// So the discriminator is kept and inverted. Spawning must now SUCCEED with
    /// `$SHELL` removed, which is false for any build that still hardcodes a
    /// macOS-only path — red on the old code, green on the new.
    #[test]
    fn system_shell_falls_back_to_a_shell_that_exists_on_this_platform() {
        let (program, _args) = default_system_shell();
        assert!(
            std::path::Path::new(&program).exists(),
            "the fallback must name a shell present on this platform, got: {program}"
        );

        let previous = std::env::var_os("SHELL");
        unsafe { std::env::remove_var("SHELL") };

        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-shell-fallback-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create working directory");

        let result = TerminalHandle::new(&working_directory, &TerminalShell::System);

        match previous {
            Some(value) => unsafe { std::env::set_var("SHELL", value) },
            None => unsafe { std::env::remove_var("SHELL") },
        }
        let _ = std::fs::remove_dir_all(&working_directory);

        match result {
            Ok(handle) => drop(handle),
            Err(error) => panic!(
                "with $SHELL unset the System shell must still spawn via the platform \
                 fallback ({program}), but it failed: {error:#}"
            ),
        }
    }

    /// The UI names the System pane's shell via `system_shell_display_name`,
    /// so whatever it returns is shown to the user as if it were a program
    /// name: it must be a bare file name on every platform, never an
    /// unresolvable POSIX path or a Windows path fragment.
    #[test]
    fn system_shell_display_name_is_a_bare_file_name() {
        let name = system_shell_display_name();
        assert!(!name.is_empty(), "the display name must not be empty");
        assert!(
            !name.contains('/') && !name.contains('\\'),
            "the display name must be a bare file name, got: {name:?}"
        );
    }

    /// #230: Git Bash exports `$SHELL=/bin/bash.exe`, an MSYS path that
    /// `CreateProcessW` cannot resolve — every System pane failed with os
    /// error 3. On Windows the POSIX variable must be ignored entirely and the
    /// resolved program must exist on this machine's disk.
    #[cfg(windows)]
    #[test]
    fn a_posix_shell_value_is_ignored_on_windows() {
        const POSIX_VALUE: &str = "/bin/bash.exe";
        let previous = std::env::var_os("SHELL");
        unsafe { std::env::set_var("SHELL", POSIX_VALUE) };

        let (program, _args) = system_pane_shell();

        match previous {
            Some(value) => unsafe { std::env::set_var("SHELL", value) },
            None => unsafe { std::env::remove_var("SHELL") },
        }

        assert_ne!(
            program, POSIX_VALUE,
            "a POSIX $SHELL value must not be honoured on Windows"
        );
        assert!(
            std::path::Path::new(&program).exists(),
            "the resolved program must exist on disk, got: {program}"
        );
    }

    /// `"zsh"` was the hardcoded display fallback of the old breadcrumb code;
    /// on Windows nothing named zsh is running, and naming it lied to the user.
    #[cfg(windows)]
    #[test]
    fn the_display_name_is_not_the_hardcoded_zsh_fallback() {
        assert_ne!(
            system_shell_display_name(),
            "zsh",
            "the display name must come from the shell actually resolved, \
             not the old hardcoded zsh fallback"
        );
    }

    /// Every "run this command" call site funnels through
    /// `command_shell_invocation`, so the guarantee it owes is that the
    /// program it names can actually be executed here and that the command
    /// survives into the arguments.
    ///
    /// This deliberately does not unset `$SHELL`: the sibling test above
    /// already proves the fallback program exists on this platform, and
    /// `command_shell_invocation` delegates to that same function rather than
    /// naming a shell of its own. Mutating the environment a second time would
    /// race that test for no extra coverage.
    #[test]
    fn command_shell_invocation_names_a_runnable_shell_and_keeps_the_command() {
        let (program, args) = command_shell_invocation("printf hello");

        assert!(
            std::path::Path::new(&program).exists() || which_on_path(&program).is_some(),
            "the command shell must be executable here, got: {program}"
        );
        #[cfg(not(windows))]
        {
            assert_eq!(args.first().map(String::as_str), Some("-lc"));
            assert_eq!(
                args.last().map(String::as_str),
                Some("printf hello"),
                "the command must reach the shell verbatim, got {args:?}"
            );
        }
        // Windows passes the command UNQUOTED since #39: the transport is
        // portable-pty's CommandBuilder, whose `append_quoted` re-quotes each
        // argument — a pre-wrapped command arrived at cmd as `\"printf
        // hello\"` (a quoted program name) and never executed. CommandBuilder's
        // own quoting satisfies `cmd /C`'s preserve-or-strip rule.
        #[cfg(windows)]
        {
            assert_eq!(args.first().map(String::as_str), Some("/C"));
            assert_eq!(
                args.last().map(String::as_str),
                Some("printf hello"),
                "the command must reach the shell verbatim and UNQUOTED (CommandBuilder owns quoting), got {args:?}"
            );
        }

        // The whole point is that it runs. A shell that cannot execute the
        // command is the defect this replaced: an ungated `/bin/zsh` opened
        // nothing at all on a Linux box with `$SHELL` unset.
        //
        // Each platform spawns through its REAL transport: Windows goes into
        // a portable-pty CommandBuilder exactly as `new_with_pane_id` does,
        // so the #39 quoting path is exercised end to end; unix keeps the
        // plain std spawn (no PTY needed to prove `-lc` executes).
        #[cfg(windows)]
        {
            // Same effective command line the real transport builds: with
            // the command UNQUOTED, CommandBuilder's `append_quoted` wraps
            // "printf hello" once and cmd /C strips that single pair (#39).
            let output = std::process::Command::new(&program)
                .args(&args)
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap_or_else(|error| panic!("{program} must be spawnable: {error}"));
            assert!(
                output.status.success(),
                "{program} {args:?} exited {:?}; stderr: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        #[cfg(not(windows))]
        {
            let mut spawn = std::process::Command::new(&program);
            spawn.args(&args);
            let status = spawn
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap_or_else(|error| panic!("{program} must be spawnable: {error}"));
            assert!(status.success(), "{program} {args:?} exited {status}");
        }
    }

    /// Minimal `which`, so the assertion above also accepts a `$SHELL` that is
    /// a bare name rather than an absolute path.
    fn which_on_path(program: &str) -> Option<std::path::PathBuf> {
        if program.contains(std::path::MAIN_SEPARATOR) {
            return None;
        }
        std::env::var_os("PATH").and_then(|path| {
            std::env::split_paths(&path)
                .map(|directory| directory.join(program))
                .find(|candidate| candidate.exists())
        })
    }

    #[gpui::test]
    async fn scrollback_source_captures_the_same_bytes_as_direct_capture(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-scrollback-source-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = pty_fixture_shell(
            "printf 'SCROLLBACK_SOURCE_TEST\\n'; exec sleep 1",
            &["print", "SCROLLBACK_SOURCE_TEST\\n", "--sleep", "1"],
        );
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn PTY")
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let captured = loop {
            cx.run_until_parked();
            let source = terminal
                .read_with(&cx.cx, |terminal, _| terminal.scrollback_source())
                .expect("running terminal must expose a source");
            let bytes = source.capture();
            if String::from_utf8_lossy(&bytes).contains("SCROLLBACK_SOURCE_TEST") {
                break bytes;
            }
            if std::time::Instant::now() >= deadline {
                panic!("the source never observed the PTY output");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let direct = terminal.read_with(&cx.cx, |terminal, _| terminal.capture_scrollback());
        assert_eq!(captured, direct);

        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
        let _ = std::fs::remove_dir_all(working_directory);
    }

    #[gpui::test]
    async fn scrollback_source_is_none_for_a_failed_terminal(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let missing = std::env::temp_dir().join(format!(
            "sirio-terminal-failed-scrollback-source-{}",
            std::process::id()
        ));
        let window = cx.add_window(|_, cx| {
            TerminalView::failed(
                &missing,
                TerminalShell::System,
                "failed source test",
                cx,
            )
        });
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let terminal = cx.update(|window, _| {
            window
                .root::<TerminalView>()
                .flatten()
                .expect("failed terminal root")
        });
        assert!(
            terminal
                .read_with(&cx.cx, |terminal, _| terminal.scrollback_source())
                .is_none()
        );
    }

    #[gpui::test]
    async fn scrollback_source_returns_empty_after_shutdown(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-shutdown-scrollback-source-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let window = cx.add_window(|_, cx| {
            TerminalView::with_shell(
                &working_directory,
                pty_fixture_shell("exec sleep 60", &["sleep", "inf"]),
                cx,
            )
            .expect("spawn PTY")
        });
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let terminal = cx.update(|window, _| {
            window
                .root::<TerminalView>()
                .flatten()
                .expect("terminal root")
        });
        let source = terminal
            .read_with(&cx.cx, |terminal, _| terminal.scrollback_source())
            .expect("running terminal must expose a source");
        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();

        let (result_tx, result_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = result_tx.send(source.capture());
        });
        let result = result_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("a source captured after shutdown must not hang");
        assert!(result.is_empty());
        let _ = std::fs::remove_dir_all(working_directory);
    }

    /// A real PTY must expose the shell's OSC title and its settled scrollback
    /// through the terminal entity. This is intentionally a drawn test: the
    /// event pump must be alive, and every scheduler turn is fully drained.
    #[gpui::test]
    async fn real_pty_emits_osc_title_and_settled_output(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-activity-events-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = pty_fixture_shell(
            "sleep 0.1; printf '\\033]0;✳ idle\\007'; printf 'Do you want to proceed?\\n'; exec sleep 1",
            &[
                "print",
                "\\033]0;✳ idle\\007Do you want to proceed?\\n",
                "--delay",
                "0.1",
                "--sleep",
                "1",
            ],
        );
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

    /// Draws a real interactive `/bin/sh`, waits for its prompt output to
    /// settle, and warms the retained grid after the debounced initial resize
    /// has reached the owner thread.
    fn settled_drawn_terminal(
        terminal: &gpui::Entity<TerminalView>,
        cx: &mut gpui::VisualTestContext,
    ) -> TerminalHandle {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut previous = Vec::new();
        let mut stable_samples = 0;
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
            cx.run_until_parked();
            let scrollback =
                terminal.read_with(&cx.cx, |terminal, _| terminal.capture_scrollback());
            if !scrollback.is_empty() && scrollback == previous {
                stable_samples += 1;
            } else {
                stable_samples = 0;
            }
            previous = scrollback;
            if stable_samples >= 3 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            stable_samples >= 3,
            "the /bin/sh prompt output never settled"
        );

        let handle = terminal
            .read_with(&cx.cx, |terminal, _| terminal.running_terminal().cloned())
            .expect("drawn terminal must have a running owner");
        // `prepaint` schedules the initial resize with a debounce. Warm after
        // it has had time to arrive, and repeat until the shared stamp is
        // flat so the samples below describe a genuinely still pane.
        std::thread::sleep(Duration::from_millis(100));
        let warm_deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut previous_stamp = handle.mutation_stamp.load(Ordering::Relaxed);
        let mut stable_stamps = 0;
        while std::time::Instant::now() < warm_deadline {
            terminal.update(&mut cx.cx, |_, cx| cx.notify());
            cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(20));
            cx.run_until_parked();
            let stamp = handle.mutation_stamp.load(Ordering::Relaxed);
            if stamp == previous_stamp {
                stable_stamps += 1;
            } else {
                stable_stamps = 0;
            }
            previous_stamp = stamp;
            if stable_stamps >= 3 {
                break;
            }
        }
        assert!(
            stable_stamps >= 3,
            "the terminal mutation stamp never settled after the initial resize"
        );
        handle
    }

    /// A drawn, idle terminal should reuse both retained levels: three
    /// explicit repaints must neither ask the owner thread for a snapshot nor
    /// assemble the grid again. Real output is the positive control for both
    /// counters.
    #[gpui::test]
    async fn a_still_drawn_terminal_reuses_its_retained_grid(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-retained-grid-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = interactive_prompt_shell();
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn /bin/sh PTY")
        });

        let handle = settled_drawn_terminal(&terminal, cx);
        let before_snapshot_builds = handle.snapshot_builds.load(Ordering::SeqCst);
        let before_assemblies = handle.grid_assemblies.load(Ordering::SeqCst);
        for _ in 0..3 {
            terminal.update(&mut cx.cx, |_, cx| cx.notify());
            cx.run_until_parked();
        }
        assert_eq!(
            handle.snapshot_builds.load(Ordering::SeqCst),
            before_snapshot_builds,
            "an idle pane must not rebuild its owner-thread snapshot"
        );
        assert_eq!(
            handle.grid_assemblies.load(Ordering::SeqCst),
            before_assemblies,
            "an idle pane must not reassemble retained draw products"
        );

        let before_input = (
            handle.snapshot_builds.load(Ordering::SeqCst),
            handle.grid_assemblies.load(Ordering::SeqCst),
        );
        terminal.update(&mut cx.cx, |terminal, _| {
            terminal.input(b"echo X\n".to_vec());
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut both_moved = false;
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
            cx.run_until_parked();
            let snapshot_builds = handle.snapshot_builds.load(Ordering::SeqCst);
            let assemblies = handle.grid_assemblies.load(Ordering::SeqCst);
            if snapshot_builds > before_input.0 && assemblies > before_input.1 {
                both_moved = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            both_moved,
            "terminal output must move both snapshot and assembly counters"
        );
        handle.shutdown();
        cx.run_until_parked();
        let _ = std::fs::remove_dir_all(working_directory);
    }

    /// A Settings font-size update must reach the renderer's cell metrics, not
    /// stop at the persisted Settings snapshot. The line pitch is the
    /// observable metric used by the terminal grid for layout and PTY resize.
    #[gpui::test]
    async fn terminal_font_size_updates_the_drawn_cell_metrics(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-font-size-metrics-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = interactive_prompt_shell();
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn /bin/sh PTY")
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let handle = loop {
            cx.run_until_parked();
            if let Some(handle) =
                terminal.read_with(&cx.cx, |terminal, _| terminal.running_terminal().cloned())
            {
                let cell_height = *handle.last_cell_height.lock();
                if let Some(cell_height) = cell_height {
                    assert_eq!(cell_height, px(18.0));
                    break handle;
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "terminal did not render its initial cell metrics"
            );
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
            std::thread::sleep(Duration::from_millis(10));
        };
        let initial_cell_height =
            (*handle.last_cell_height.lock()).expect("drawn terminal has measured cell height");
        terminal.update(&mut cx.cx, |terminal, cx| {
            terminal.set_font_size(16, cx);
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let updated_cell_height = loop {
            cx.run_until_parked();
            if let Some(cell_height) = *handle.last_cell_height.lock()
                && cell_height > initial_cell_height
            {
                break cell_height;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "font-size update did not reach the drawn terminal metrics"
            );
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
            std::thread::sleep(Duration::from_millis(10));
        };

        assert_eq!(initial_cell_height, px(18.0));
        assert!(
            updated_cell_height > initial_cell_height,
            "font-size change must increase the terminal cell height: {initial_cell_height:?} -> {updated_cell_height:?}"
        );
        handle.shutdown();
        cx.run_until_parked();
        let _ = std::fs::remove_dir_all(working_directory);
    }

    /// A link lookup on a still pane should read the retained grid, while a
    /// lookup after terminal output must refresh it exactly once.
    #[gpui::test]
    async fn link_at_on_a_still_drawn_pane_reuses_the_retained_grid(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-retained-link-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = interactive_prompt_shell();
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn /bin/sh PTY")
        });

        let handle = settled_drawn_terminal(&terminal, cx);
        let before_snapshot_builds = handle.snapshot_builds.load(Ordering::SeqCst);
        for (row, column) in [(0, 0), (0, 1), (1, 0)] {
            let _ = handle.link_at(row, column);
        }
        assert_eq!(handle.link_at(usize::MAX, 0), None);
        assert_eq!(handle.link_at(0, usize::MAX), None);
        assert_eq!(
            handle.snapshot_builds.load(Ordering::SeqCst),
            before_snapshot_builds,
            "link lookups on a still pane must reuse the retained grid"
        );

        let before_stamp = handle.mutation_stamp.load(Ordering::SeqCst);
        terminal.update(&mut cx.cx, |terminal, _| {
            terminal.input(b"echo X\n".to_vec());
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while handle.mutation_stamp.load(Ordering::SeqCst) == before_stamp {
            assert!(
                std::time::Instant::now() < deadline,
                "terminal output never moved the mutation stamp"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        // The retained grid was stamped before the output landed, so one
        // lookup costs exactly one owner-thread round trip: `current_grid`
        // issues at most one `Snapshot` per call, and nothing else asks for
        // one here (no frame is drawn between these calls).
        let before_stale_lookup = handle.snapshot_builds.load(Ordering::SeqCst);
        let _ = handle.link_at(0, 0);
        assert_eq!(
            handle.snapshot_builds.load(Ordering::SeqCst),
            before_stale_lookup + 1,
            "the first lookup after output must refresh the stale retained grid"
        );

        // The shell may still be writing (its echo, the command output and
        // the next prompt can arrive as separate chunks), so judge reuse only
        // inside a window in which the stamp provably held still: sync with
        // one lookup, count the next, and accept the sample only if the stamp
        // read afterwards equals the one read before.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let stamp_before = handle.mutation_stamp.load(Ordering::SeqCst);
            let _ = handle.link_at(0, 0);
            let builds_after_sync = handle.snapshot_builds.load(Ordering::SeqCst);
            let _ = handle.link_at(0, 0);
            let builds_after_reuse = handle.snapshot_builds.load(Ordering::SeqCst);
            if handle.mutation_stamp.load(Ordering::SeqCst) == stamp_before {
                assert_eq!(
                    builds_after_reuse, builds_after_sync,
                    "a lookup under an unchanged stamp must reuse the retained grid"
                );
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the mutation stamp never held still long enough to observe a reuse"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        handle.shutdown();
        cx.run_until_parked();
        let _ = std::fs::remove_dir_all(working_directory);
    }

    /// Selection is an assembly-only invalidation: changing the main-thread
    /// selection must rebuild the wash quads from retained cells without
    /// issuing another owner-thread snapshot.
    #[gpui::test]
    async fn a_selection_change_reassembles_without_a_snapshot(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-retained-selection-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = interactive_prompt_shell();
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn /bin/sh PTY")
        });

        let handle = settled_drawn_terminal(&terminal, cx);
        let before_snapshot_builds = handle.snapshot_builds.load(Ordering::SeqCst);
        let before_assemblies = handle.grid_assemblies.load(Ordering::SeqCst);
        *handle.selection.lock() = Some(SelectedRange::between((0, 0), (0, 1)));
        terminal.update(&mut cx.cx, |_, cx| cx.notify());

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut reassembled = false;
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
            cx.run_until_parked();
            if handle.grid_assemblies.load(Ordering::SeqCst) > before_assemblies {
                reassembled = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            reassembled,
            "changing selection must invalidate the grid assembly"
        );
        assert_eq!(
            handle.snapshot_builds.load(Ordering::SeqCst),
            before_snapshot_builds,
            "changing selection must reuse the retained grid"
        );
        handle.shutdown();
        cx.run_until_parked();
        let _ = std::fs::remove_dir_all(working_directory);
    }

    /// Regression: a keystroke must reach a repaint within one frame.
    ///
    /// Measures the simulated milliseconds between a keystroke reaching the
    /// PTY and the view being notified -- i.e. the earliest moment the screen
    /// is allowed to repaint with the echoed character. This went red at 208ms
    /// when `OUTPUT_SETTLE_DEBOUNCE`, a 200ms quiet period the *activity
    /// model* needs, was also gating the *renderer*; the two deadlines are
    /// separate in `pump_terminal_events` precisely so this stays under a
    /// frame.
    ///
    /// `cat` is the tightest possible echo path: no prompt, no shell start-up
    /// output, and no timers of its own, so the only latency left in the
    /// measurement belongs to Sirio.
    #[gpui::test]
    async fn perf_keystroke_echo_latency(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-perf-echo-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = echo_child_shell();
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn PTY")
        });

        // Let the PTY finish coming up, so start-up churn is not mistaken for
        // the echo we are about to time.
        for _ in 0..100 {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(10));
            cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(2));
        }

        let notified = Rc::new(RefCell::new(false));
        let flag = notified.clone();
        let _subscription = cx.update(|_, app| {
            app.observe(&terminal, move |_, _| {
                *flag.borrow_mut() = true;
            })
        });

        terminal.update(&mut cx.cx, |terminal, _| terminal.input(b"x".to_vec()));

        // Step the simulated clock one millisecond at a time, interleaving a
        // short real sleep so the byte has a chance to travel to `cat` and
        // back on its real PTY thread. The number we report is the *simulated*
        // time, which is the deterministic part.
        let mut simulated_ms = 0u64;
        while simulated_ms < 3000 && !*notified.borrow() {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(1));
            cx.run_until_parked();
            simulated_ms += 1;
            std::thread::sleep(Duration::from_millis(1));
        }

        let echoed = terminal.update(&mut cx.cx, |terminal, _| {
            String::from_utf8_lossy(&terminal.snapshot().scrollback).contains('x')
        });
        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();

        assert!(echoed, "`cat` never echoed the keystroke back; the harness is measuring the wrong thing");
        assert!(
            simulated_ms <= 16,
            "a keystroke took {simulated_ms}ms of simulated time to reach a repaint; one frame is 16ms"
        );
    }

    /// DIAGNOSTIC -- run by hand: `cargo test -p sirio_terminal
    /// perf_idle_terminals_burn_cpu -- --ignored --nocapture`.
    ///
    /// Measures how much CPU the process burns over a fixed wall-clock window
    /// with N terminals open and *producing no output at all*. An idle
    /// terminal should cost approximately nothing.
    ///
    /// Last measured: eight idle terminals cost 19.7ms of CPU over a 2s window
    /// against a 47µs baseline -- about 1% of one core. It was 36.4ms when the
    /// owner thread ended every iteration in a flat
    /// `thread::sleep(EVENT_POLL_INTERVAL)`; parking on the command channel
    /// instead halved it. What remains is the 4ms timeout tick that drives the
    /// PTY drain, one thread per terminal.
    ///
    /// Real, but far too small to have been the app-wide stutter on its own --
    /// that was the per-frame main-thread stall measured by
    /// `perf_grid_rebuild_per_frame`. Ignored because it sleeps for four
    /// seconds and measures wall clock, which makes it a bad citizen in a
    /// loaded workspace run.
    /// Unix-only: the measurement is `getrusage(RUSAGE_SELF)` and the idle
    /// child is `/bin/cat`, neither of which exists on Windows — where the
    /// bare `libc::rusage` reference is a hard compile error, not a warning.
    #[cfg(unix)]
    #[test]
    #[ignore = "diagnostic: sleeps 4s and measures wall-clock CPU"]
    fn perf_idle_terminals_burn_cpu() {
        fn process_cpu() -> Duration {
            let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
            assert_eq!(
                unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) },
                0,
                "getrusage failed"
            );
            let micros = (usage.ru_utime.tv_sec as u64) * 1_000_000
                + usage.ru_utime.tv_usec as u64
                + (usage.ru_stime.tv_sec as u64) * 1_000_000
                + usage.ru_stime.tv_usec as u64;
            Duration::from_micros(micros)
        }

        fn cpu_over_window(terminals: usize) -> Duration {
            // `cat` sits on its PTY forever without writing a single byte, so
            // every cycle measured below is Sirio's own, not the child's.
            let mut handles = Vec::new();
            for index in 0..terminals {
                let working_directory = std::env::temp_dir().join(format!(
                    "sirio-terminal-perf-idle-{}-{index}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&working_directory).expect("create PTY directory");
                let shell = TerminalShell::WithArguments {
                    program: "/bin/cat".to_string(),
                    args: Vec::new(),
                };
                handles.push(
                    TerminalHandle::new(&working_directory, &shell).expect("spawn PTY"),
                );
            }
            // Let the PTYs finish coming up so start-up cost is not counted.
            std::thread::sleep(Duration::from_millis(300));
            let before = process_cpu();
            std::thread::sleep(Duration::from_secs(2));
            let after = process_cpu();
            drop(handles);
            after - before
        }

        // Baseline first: a later measurement cannot be polluted by threads
        // that an earlier one leaked.
        let baseline = cpu_over_window(0);
        let with_terminals = cpu_over_window(8);
        let overhead = with_terminals.saturating_sub(baseline);
        println!(
            "idle CPU over 2s: baseline {baseline:?}, 8 terminals {with_terminals:?}, overhead {overhead:?}"
        );
        assert!(
            overhead < Duration::from_millis(100),
            "8 terminals with zero output burned {overhead:?} of CPU over a 2s wall-clock window (baseline {baseline:?})"
        );
    }

    /// DIAGNOSTIC -- run by hand: `cargo test --release -p sirio_terminal
    /// perf_grid_rebuild_per_frame -- --ignored --nocapture`.
    ///
    /// Times what `TerminalElement::prepaint` waits for on every single frame,
    /// at a realistic maximised-window size. GPUI repaints at display refresh,
    /// so this number has a hard budget: at 120Hz a frame is 8.3ms, and this
    /// happens on the main thread, ahead of everything else the app draws.
    ///
    /// Measured at **5.93ms per frame for 10,000 cells in release**, which was
    /// the app-wide stutter: with a terminal on screen the main thread gave up
    /// most of every frame, per visible pane. Splitting that number was the
    /// whole diagnosis -- only 0.6ms was the grid rebuild, and 5.3ms was the
    /// blocking round-trip waiting on an owner thread that had just started a
    /// flat 4ms nap. It is **354µs** now that the owner thread parks on its
    /// command channel instead (see step 4 of its loop).
    ///
    /// What is left is the rebuild itself: ~35ns per cell, every frame, for
    /// cells that overwhelmingly did not change. Making *that* go away means
    /// the snapshot no longer being rebuilt from scratch -- dirty-region
    /// tracking, or a retained grid the emulator mutates in place -- which is
    /// a real architectural change and deliberately not bundled in here.
    /// Ignored because it measures wall clock, which is exactly the kind of
    /// test this repo already has trouble with under a loaded workspace run.
    #[test]
    #[ignore = "diagnostic: wall-clock measurement of the per-frame snapshot cost"]
    fn perf_grid_rebuild_per_frame() {
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-perf-grid-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                // Fill the scrollback and the screen with real, attributed
                // content rather than blanks -- a screen of spaces is not what
                // an agent session looks like.
                "for i in $(seq 1 4000); do printf 'line %s \\033[1;31mbold red\\033[0m plain text here\\n' \"$i\"; done; exec sleep 60".to_string(),
            ],
        };
        let (handle, _events) = TerminalHandle::new(&working_directory, &shell).expect("spawn PTY");

        // A maximised window on a 16" display, near enough.
        handle.resize(200, 50, 8, 16);
        std::thread::sleep(Duration::from_secs(2));

        // Warm once, so the measurement is steady-state and not first-touch.
        let _ = handle.snapshot();

        const FRAMES: u32 = 100;
        let started = std::time::Instant::now();
        let mut cells = 0usize;
        for _ in 0..FRAMES {
            let (rows, _) = handle.snapshot();
            cells = rows.iter().map(|row| row.len()).sum();
        }
        let per_frame = started.elapsed() / FRAMES;
        println!("grid rebuild: {per_frame:?} per frame over {cells} cells");
        assert!(
            per_frame < Duration::from_micros(2000),
            "rebuilding the grid costs {per_frame:?} per frame ({cells} cells); an 8.3ms frame at 120Hz cannot absorb this plus the rest of the app"
        );
    }

    #[gpui::test]
    async fn stable_identity_is_inherited_by_the_lazily_spawned_pty(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-pane-identity-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = pty_fixture_shell(
            "printf 'pane=%s\\n' \"$SIRIO_PANE_ID\"; exec sleep 1",
            &[
                "print",
                "pane=${SIRIO_PANE_ID}\\n",
                "--expand",
                "--sleep",
                "1",
            ],
        );
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

    /// F-TERM-PTY-07: `restart` bumps `TerminalSurfaceHost`'s generation and
    /// starts a genuinely fresh process, and the *old* process's own
    /// `pump_terminal_events` task -- still alive, still draining its own
    /// channel -- must not be allowed to stamp its late `ChildExit` onto the
    /// new process's state. Both processes are real PTYs; the old one is
    /// killed by `shutdown()` inside `restart`, and this asserts its exit
    /// event, once it does arrive, left `exit_status` alone.
    #[gpui::test]
    async fn restart_bumps_the_generation_and_ignores_the_old_generations_late_exit(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let working_directory =
            std::env::temp_dir().join(format!("sirio-terminal-restart-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = pty_fixture_shell(
            "printf 'gen1\\n'; exec sleep 5",
            &["print", "gen1\\n", "--sleep", "5"],
        );
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn first PTY")
        });

        let wait_for = |cx: &mut gpui::VisualTestContext, needle: &str| {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while std::time::Instant::now() < deadline {
                cx.run_until_parked();
                cx.background_executor
                    .advance_clock(Duration::from_millis(5));
                cx.run_until_parked();
                let snapshot = terminal.update(&mut cx.cx, |terminal, _| terminal.snapshot());
                if String::from_utf8_lossy(&snapshot.scrollback).contains(needle) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("{needle:?} never appeared in scrollback");
        };

        wait_for(cx, "gen1");
        assert_eq!(
            terminal.read_with(&cx.cx, |terminal, _| terminal.surface_generation()),
            1,
            "the first spawn is generation 1"
        );
        assert!(
            terminal.read_with(&cx.cx, |terminal, _| terminal.is_host_mounted()),
            "a live process must report the host as mounted"
        );

        terminal.update(&mut cx.cx, |terminal, cx| {
            terminal.spawn.shell = pty_fixture_shell(
                "printf 'gen2\\n'; exec sleep 5",
                &["print", "gen2\\n", "--sleep", "5"],
            );
            terminal.restart(cx);
        });
        assert_eq!(
            terminal.read_with(&cx.cx, |terminal, _| terminal.surface_generation()),
            2,
            "restart must bump the generation synchronously, not on the PTY's own schedule"
        );

        wait_for(cx, "gen2");
        // Give the killed first process's own pump task every chance to
        // observe and apply its ChildExit before asserting it did not. The
        // PTY-exit watcher runs on a real OS thread outside GPUI's simulated
        // clock, so this needs genuine wall-clock time to pass (like
        // `wait_for` above), not just fast-forwarded timers.
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(50));
            cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            terminal.read_with(&cx.cx, |terminal, _| terminal.exit_status()),
            None,
            "the superseded generation's ChildExit must not clobber the new process's state"
        );
        assert!(
            terminal.read_with(&cx.cx, |terminal, _| terminal.is_host_mounted()),
            "the new process is still live -- restart must not leave the host torn down"
        );
        let snapshot = terminal.update(&mut cx.cx, |terminal, _| terminal.snapshot());
        assert!(
            !String::from_utf8_lossy(&snapshot.scrollback).contains("gen1"),
            "restart starts a genuinely fresh terminal buffer, unlike a pane move"
        );

        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
    }

    /// F-TERM-03: the render-facing surface. `TerminalView::render`'s pill
    /// branch is a direct, one-line function of `is_command_running()` and
    /// `exit_status()` (`if self.is_command_running() { .. } else if let
    /// Some(label) = self.exit_status.map(..) { .. }`), so asserting on
    /// those two getters across one real command's full lifecycle is
    /// equivalent to asserting on the pill it produces without needing a
    /// separate paint-inspection harness (this crate has none for pill
    /// text; the closest existing pattern, `debug_selector`, names an
    /// element for hit-testing, not its text content).
    ///
    /// Drives one PTY through all three pill states in the order Swift's
    /// `statusChip` uses them (running wins first; exit replaces it once the
    /// child is actually gone): idle prompt -> `sleep 2` running in the
    /// foreground -> shell's own `exit 7` tears down the PTY child. Without
    /// this change `is_command_running` does not exist and the running
    /// assertion below has nothing to hold; before the exit hand-off is
    /// respected, a stale `true` could in principle outlive `exit_status`
    /// becoming `Some` -- this test's final assertion catches that.
    ///
    /// unix only, and genuinely so: `is_command_running()` is
    /// `TerminalHandle::foreground_command_running`, whose definition is
    /// `tcgetpgrp` on the PTY master. Windows has no foreground process
    /// group, so that method is `#[cfg(not(unix))] { false }` by
    /// construction and the pill never reports Running there — a known gap,
    /// documented on the method itself. The central assertion of this test
    /// ("the pill must report Running") therefore has no Windows behaviour
    /// to hold on to, and the interactive `/bin/bash` and its `cat
    /// >/dev/null; exit 7` job control have no equivalent either.
    #[cfg(unix)]
    #[gpui::test]
    async fn running_pill_state_is_replaced_by_exit_status_when_the_child_exits(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-running-pill-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        // The shell itself is the PTY's direct child (no wrapping login
        // shell) so this test does not depend on `$SHELL` in the sandbox,
        // but it is still the interactive, job-control-capable shell a real
        // terminal pane runs -- the same shape `TerminalShell::System`
        // spawns.
        let shell = TerminalShell::WithArguments {
            program: "/bin/bash".to_string(),
            args: vec![
                "--norc".to_string(),
                "--noprofile".to_string(),
                "-i".to_string(),
            ],
        };
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn PTY")
        });

        let settle = |cx: &mut gpui::VisualTestContext, extra: Duration| {
            let deadline = std::time::Instant::now() + extra;
            while std::time::Instant::now() < deadline {
                cx.run_until_parked();
                cx.background_executor
                    .advance_clock(Duration::from_millis(5));
                cx.run_until_parked();
                std::thread::sleep(Duration::from_millis(10));
            }
        };

        // Startup settling: real fork/exec/setsid wall-clock time.
        settle(cx, Duration::from_millis(500));
        assert!(
            !terminal.read_with(&cx.cx, |terminal, _| terminal.is_command_running()),
            "a freshly spawned interactive shell sitting at its prompt must not show Running"
        );
        assert_eq!(
            terminal.read_with(&cx.cx, |terminal, _| terminal.exit_status()),
            None
        );

        terminal.update(&mut cx.cx, |terminal, _| {
            terminal.input(b"cat >/dev/null; exit 7\n".to_vec())
        });

        let running_deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut observed_running = false;
        while std::time::Instant::now() < running_deadline {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
            cx.run_until_parked();
            if terminal.read_with(&cx.cx, |terminal, _| terminal.is_command_running()) {
                observed_running = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            observed_running,
            "the pill must report Running while `sleep 2` holds the PTY's foreground process group"
        );
        assert_eq!(
            terminal.read_with(&cx.cx, |terminal, _| terminal.exit_status()),
            None,
            "the running command has not exited yet -- the exit pill must not appear early"
        );
        terminal.update(&mut cx.cx, |terminal, _| terminal.input("\u{4}"));

        let exit_deadline = std::time::Instant::now() + Duration::from_secs(20);
        let mut observed_exit = None;
        while std::time::Instant::now() < exit_deadline {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
            cx.run_until_parked();
            if let Some(status) = terminal.read_with(&cx.cx, |terminal, _| terminal.exit_status()) {
                observed_exit = Some(status);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            observed_exit,
            Some(TerminalExitStatus::Code(7)),
            "the shell's own `exit 7` must surface as this pane's recorded exit status"
        );
        assert!(
            !terminal.read_with(&cx.cx, |terminal, _| terminal.is_command_running()),
            "once the PTY child has exited, Running must not still be reported -- \
             the exit pill replaces it rather than the two ever coexisting"
        );

        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
        let _ = std::fs::remove_dir_all(working_directory);
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
        let working_directory =
            std::env::temp_dir().join(format!("sirio-terminal-link-click-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = pty_fixture_shell(
            "printf 'https://example.test/docs\\n'; exec sleep 60",
            &["print", "https://example.test/docs\\n"],
        );
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

    /// P129: the terminal's own right-click context menu reused the
    /// already-window-absolute mouse-down position as a plain
    /// `.absolute().left()/.top()` value inside a `.relative()` ancestor
    /// whose own window origin is non-zero for every pane but one flush
    /// against the window's top-left corner — double-counting the offset.
    /// This drives the exact same real mouse gesture as
    /// `right_click_resolves_this_terminal_and_draws_all_context_actions`
    /// above, but behind a 280px spacer standing in for the sidebar, which
    /// is the one layout an origin-doubling bug can actually show up in.
    /// It asserts both halves of the regression: the menu paints exactly
    /// at the click point (not click + pane origin), and a click at that
    /// real, painted item position fires the action.
    #[gpui::test]
    async fn context_menu_paints_at_the_click_point_behind_a_non_zero_pane_origin(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let missing = missing_directory("context-menu-non-zero-origin");
        let window = cx.add_window(|_, cx| {
            let terminal = cx.new(|cx| {
                TerminalView::failed(
                    &missing,
                    TerminalShell::System,
                    "test non-zero-origin context-menu terminal",
                    cx,
                )
            });
            NonZeroOriginFixture { terminal }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let fixture = cx.update(|window, _| {
            window
                .root::<NonZeroOriginFixture>()
                .flatten()
                .expect("fixture root")
        });
        let terminal = fixture.read_with(&cx.cx, |fixture, _| fixture.terminal.clone());
        let events = Rc::new(RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&terminal, move |_, event: &TerminalContextEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        let spacer = cx
            .debug_bounds("terminal-test-origin-spacer")
            .expect("spacer must be drawn");
        let pane_origin_x = spacer.origin.x + spacer.size.width;
        assert!(
            pane_origin_x > px(0.0),
            "fixture must place the pane at a non-zero window origin, got {pane_origin_x:?}"
        );

        let click = point(pane_origin_x + px(40.0), spacer.origin.y + px(40.0));
        cx.simulate_mouse_down(click, MouseButton::Right, Modifiers::none());

        let menu = cx
            .debug_bounds("terminal-context-menu")
            .expect("context menu must be drawn");
        // Sub-pixel layout rounding can shift this by a fraction of a
        // pixel; P129's bug shifted it by the pane's entire 280px origin,
        // so a 1px tolerance still cleanly distinguishes "fixed" from
        // "double-counted."
        let delta_x = (menu.origin.x - click.x).abs();
        let delta_y = (menu.origin.y - click.y).abs();
        assert!(
            delta_x < px(1.0) && delta_y < px(1.0),
            "menu must paint at the click point, not click + pane origin (P129): \
             menu.origin={:?}, click={:?}",
            menu.origin,
            click
        );

        let split_left = cx
            .debug_bounds("terminal-context-item-6")
            .expect("Split Left item must be drawn");
        cx.simulate_click(split_left.center(), Modifiers::none());
        let event = events.borrow().last().cloned().expect("split event");
        assert_eq!(event.action, TerminalContextAction::SplitLeft);
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
            "sirio-terminal-clipboard-roundtrip-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = pty_fixture_shell("exec cat", &["cat"]);
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

    fn clipboard_keystroke(key: &str) -> String {
        if cfg!(target_os = "macos") {
            format!("cmd-{key}")
        } else {
            format!("ctrl-shift-{key}")
        }
    }

    #[gpui::test]
    async fn keyboard_paste_round_trips_through_the_terminal(cx: &mut gpui::TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-keyboard-paste-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = pty_fixture_shell(
            "printf '\\033[?2004h'; exec cat",
            &["cat", "--prologue", "\\033[?2004h"],
        );
        let window = cx.add_window(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn PTY")
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let terminal = cx.update(|window, _cx| {
            window
                .root::<TerminalView>()
                .flatten()
                .expect("terminal root")
                .clone()
        });
        let target = cx
            .debug_bounds("terminal-drop-target")
            .expect("terminal is drawn");
        let pasted = "keyboard-paste-351";
        cx.update(|_, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(pasted.to_string()));
        });
        cx.simulate_click(target.center(), Modifiers::none());
        cx.simulate_keystrokes(&clipboard_keystroke("v"));

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut captured = String::new();
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            captured = terminal.read_with(&cx.cx, |terminal, _| {
                String::from_utf8_lossy(&terminal.capture_scrollback()).into_owned()
            });
            if captured.contains(pasted) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            captured.contains(pasted),
            "keyboard paste did not reach the PTY: {captured:?}"
        );

        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
        let _ = std::fs::remove_dir_all(working_directory);
    }

    #[gpui::test]
    async fn keyboard_copy_writes_the_selected_terminal_text_to_clipboard(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-keyboard-copy-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = pty_fixture_shell("printf 'COPY-351'; exec sleep 60", &["print", "COPY-351"]);
        let window = cx.add_window(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn PTY")
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let terminal = cx.update(|window, _cx| {
            window
                .root::<TerminalView>()
                .flatten()
                .expect("terminal root")
                .clone()
        });
        let handle = terminal
            .read_with(&cx.cx, |terminal, _| terminal.running_terminal().cloned())
            .expect("terminal must be running");
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            let captured = terminal.read_with(&cx.cx, |terminal, _| {
                String::from_utf8_lossy(&terminal.capture_scrollback()).into_owned()
            });
            if captured.contains("COPY-351") {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let target = cx
            .debug_bounds("terminal-drop-target")
            .expect("terminal is drawn");
        let cell_width = (*handle.last_cell_width.lock()).expect("terminal cell width");
        let cell_height = (*handle.last_cell_height.lock()).expect("terminal cell height");
        let start = point(
            target.origin.x + px(f32::from(cell_width) * 0.5),
            target.origin.y + px(f32::from(cell_height) * 0.5),
        );
        let end = point(
            target.origin.x + px(f32::from(cell_width) * 7.5),
            target.origin.y + px(f32::from(cell_height) * 0.5),
        );
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
        assert_eq!(handle.selected_text().as_deref(), Some("COPY-351"));

        cx.update(|_, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("sentinel".to_string()));
        });
        cx.simulate_keystrokes(&clipboard_keystroke("c"));
        let copied = cx
            .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
            .expect("keyboard copy must write a clipboard entry");
        assert_eq!(copied, "COPY-351");

        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
        let _ = std::fs::remove_dir_all(working_directory);
    }

    /// F-TAB-11: `TerminalContextItem` had no way to express "unavailable,
    /// and here's why" -- `items()` took no parameters and the render site
    /// attached `.on_click()` to every item unconditionally, so every item
    /// was always clickable even when the action made no sense. This forces
    /// the pane's own live prepainted size below the real split minimum
    /// (mirroring `main.rs`'s `MIN_SPLIT_PANE_SIZE`) and checks both halves
    /// of the fix in one drive: the disabled item shows its reason text and
    /// a click at its own drawn position does not fire an event, while an
    /// unrelated, still-available item in the very same open menu still
    /// works normally.
    #[gpui::test]
    async fn a_too_narrow_pane_disables_split_left_with_a_visible_reason(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "sirio-terminal-disabled-split-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        let shell = pty_fixture_shell("exec cat", &["cat"]);
        // A real, narrow window -- not a faked-up bounds value -- so the
        // pane's own `TerminalElement::prepaint` records a genuinely small
        // size the same way it would behind a real cramped split. Height
        // stays generous so only the horizontal splits are expected to
        // become unavailable.
        let window = cx.open_window(size(px(220.0), px(900.0)), |_, cx| {
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

        let reason_bounds = cx
            .debug_bounds("terminal-context-item-6-reason")
            .expect("Split Left's disabled reason must be drawn");
        assert!(reason_bounds.size.width > px(0.0));

        let split_left = cx
            .debug_bounds("terminal-context-item-6")
            .expect("Split Left item must still be drawn (disabled, not absent)");
        cx.simulate_click(split_left.center(), Modifiers::none());
        assert!(
            events.borrow().is_empty(),
            "a click on a disabled item must not fire its action: {:?}",
            events.borrow()
        );

        // The disabled click landed inside the menu, so it did not dismiss
        // it via `on_mouse_down_out`; an unrelated, available item in the
        // same still-open menu must still work.
        let copy_pane_id = cx
            .debug_bounds("terminal-context-item-4")
            .expect("Copy Pane ID item must still be reachable in the same menu");
        cx.simulate_click(copy_pane_id.center(), Modifiers::none());
        cx.run_until_parked();
        let clipboard_text = cx
            .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
            .expect("an unrelated, enabled item in the same menu must still work");
        assert!(
            clipboard_text.starts_with("pane-"),
            "clipboard held {clipboard_text:?}, not a pane id"
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
            std::env::temp_dir().join(format!("sirio-terminal-diff-drop-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).expect("create drop directory");
        let shell = pty_fixture_shell("exec sleep 60", &["sleep", "inf"]);
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
            "sirio-terminal-keyboard-menu-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create terminal directory");
        let shell = pty_fixture_shell("exec sleep 60", &["sleep", "inf"]);
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
            std::env::temp_dir().join(format!("sirio-terminal-file-drop-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).expect("create drop directory");
        let shell = pty_fixture_shell("exec sleep 60", &["sleep", "inf"]);
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
            "sirio-terminal-external-drop-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create drop directory");
        let shell = pty_fixture_shell("exec sleep 60", &["sleep", "inf"]);
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
