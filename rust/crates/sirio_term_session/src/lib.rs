//! The terminal session engine runs PTYs, a resident VT, and session byte
//! streams without GPUI; it is used by `sirio_host` and by the app's in-process
//! fallback.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;

use libghostty_vt::Terminal;
use libghostty_vt::fmt::{Format as VtFormat, Formatter, FormatterOptions};
use libghostty_vt::screen::CellWide;
use libghostty_vt::style::{Style, StyleColor, Underline};
use libghostty_vt::terminal::{Point, PointCoordinate};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

pub mod fanout;
pub mod journal;
pub mod terminate;
mod keys;
mod owner;

pub use owner::{
    KITTY_APC_MAX_BYTES, KITTY_DECOMPRESSED_MAX_BYTES, KITTY_IMAGE_STORAGE_LIMIT,
    KittyPngDecoder, apply_kitty_ingest_limits,
};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const JOURNAL_BYTES: usize = 4 << 20;
pub(crate) const STREAM_QUEUE_BYTES: usize = 4 << 20;

fn wait_for_input_reply(
    receiver: mpsc::Receiver<Result<(), InputError>>,
    timeout: Duration,
) -> Result<(), InputError> {
    match receiver.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(()),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(InputError::Gone),
    }
}

/// The information needed to create one terminal PTY and its resident VT.
pub struct SpawnSpec {
    pub id: String,
    pub cwd: PathBuf,
    pub program: Option<(String, Vec<String>)>,
    pub env: BTreeMap<String, String>,
    pub cols: u16,
    pub rows: u16,
    pub prelude: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpawnError {
    InvalidCwd(String),
    Spawn(String),
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCwd(kind) => write!(f, "invalid working directory ({kind})"),
            Self::Spawn(reason) => write!(f, "could not spawn terminal session ({reason})"),
        }
    }
}

impl std::error::Error for SpawnError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputError {
    Exited,
    Gone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyError {
    Session(InputError),
    InvalidKey(String),
    TimedOut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Exit {
    Success,
    Code(i32),
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Title(String),
    Foreground(bool),
    Exited(Exit),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Text,
    Raw,
}

#[derive(Clone, Debug)]
pub struct Info {
    pub pid: u32,
    pub cwd: PathBuf,
    pub title: String,
    pub cols: u16,
    pub rows: u16,
    pub exit: Option<Exit>,
    pub foreground: bool,
    pub attachments: usize,
    /// Windows job-adoption error kind; the host logs it as
    /// `session.no_job <kind>` (Task 6). A missing process handle reports
    /// `Unsupported`.
    pub tree_kill_error: Option<String>,
}

pub struct Attach {
    pub size: (u16, u16),
    pub resumed: bool,
    pub attachment: fanout::Attachment,
}

/// A child process, a VT that lives as long as it does, and its attachments.
pub struct Session {
    commands: mpsc::Sender<owner::Command>,
    info: Arc<Mutex<Info>>,
    wait_state: Arc<(Mutex<Option<Exit>>, Condvar)>,
    closed: AtomicBool,
}

impl Session {
    pub fn spawn(spec: SpawnSpec, events: mpsc::Sender<Event>) -> Result<Self, SpawnError> {
        validate_cwd(&spec.cwd)?;

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: spec.rows,
                cols: spec.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| SpawnError::Spawn(error.to_string()))?;

        #[cfg(unix)]
        terminate::drop_acquired_controlling_terminal();

        let (program, args) = spec
            .program
            .clone()
            .unwrap_or_else(|| session_login_shell(&spec.env));
        let mut command = CommandBuilder::new(program);
        for argument in &args {
            command.arg(argument);
        }
        command.env_clear();
        for (key, value) in &spec.env {
            command.env(key, value);
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
        command.env("SIRIO_PANE_ID", &spec.id);
        // Scripts and agent skills written before the rebrand read the pane id
        // under its old name. Exporting both costs one variable and keeps them
        // working inside a pane they cannot see renamed.
        command.env("TILLER_PANE_ID", &spec.id);
        command.cwd(spawn_cwd(&spec.cwd));

        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| SpawnError::Spawn(error.to_string()))?;
        drop(pair.slave);
        let pid = child.process_id().unwrap_or(0);

        #[cfg(windows)]
        let (kill_on_close, tree_kill_error) = match child.as_raw_handle() {
            Some(handle) => match terminate::KillOnClose::adopt(handle) {
                Ok(job) => (Some(job), None),
                Err(error) => (None, Some(format!("{:?}", error.kind()))),
            },
            None => (
                None,
                Some(format!("{:?}", std::io::ErrorKind::Unsupported)),
            ),
        };
        #[cfg(not(windows))]
        let tree_kill_error = None;

        #[cfg(unix)]
        let master_fd = pair.master.as_raw_fd();
        let mut reader = pair.master.try_clone_reader().map_err(|error| {
            let _ = child.kill();
            SpawnError::Spawn(error.to_string())
        })?;
        let writer = pair.master.take_writer().map_err(|error| {
            let _ = child.kill();
            SpawnError::Spawn(error.to_string())
        })?;

        let info = Arc::new(Mutex::new(Info {
            pid,
            cwd: spec.cwd.clone(),
            title: String::new(),
            cols: spec.cols,
            rows: spec.rows,
            exit: None,
            foreground: false,
            attachments: 0,
            tree_kill_error,
        }));
        let wait_state = Arc::new((Mutex::new(None), Condvar::new()));
        let (commands, command_rx) = mpsc::channel();
        let (bytes_tx, bytes_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();

        owner::start(owner::Inputs {
            cols: spec.cols,
            rows: spec.rows,
            prelude: spec.prelude,
            command_rx,
            bytes_rx,
            events,
            writer,
            master: pair.master,
            child,
            info: Arc::clone(&info),
            wait_state: Arc::clone(&wait_state),
            ready: ready_tx,
            #[cfg(unix)]
            master_fd,
            #[cfg(windows)]
            kill_on_close,
        });

        if ready_rx.recv_timeout(RESPONSE_TIMEOUT).is_err() {
            return Err(SpawnError::Spawn("session owner did not start".into()));
        }

        // The owner applies the prelude through its Feed path before it signals
        // readiness, so the reader cannot enqueue child output ahead of it.
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

        Ok(Self {
            commands,
            info,
            wait_state,
            closed: AtomicBool::new(false),
        })
    }

    pub fn attach(&self, resume_from: Option<u64>) -> Attach {
        let (reply, receiver) = mpsc::channel();
        if self
            .commands
            .send(owner::Command::Attach(resume_from, reply))
            .is_ok()
            && let Ok(attach) = receiver.recv_timeout(RESPONSE_TIMEOUT)
        {
            return attach;
        }

        let cached = self.cached_info();
        let (_, attachment) = fanout::Fanout::new(0).add();
        Attach {
            size: (cached.cols, cached.rows),
            resumed: false,
            attachment,
        }
    }

    pub fn detach(&self, id: fanout::AttachmentId) {
        let _ = self.commands.send(owner::Command::Detach(id));
    }

    pub fn input(&self, bytes: Vec<u8>) -> Result<(), InputError> {
        let (reply, receiver) = mpsc::channel();
        self.commands
            .send(owner::Command::Input(bytes, reply))
            .map_err(|_| InputError::Gone)?;
        wait_for_input_reply(receiver, RESPONSE_TIMEOUT)
    }

    pub fn key(&self, name: &str) -> Result<(), KeyError> {
        let (reply, receiver) = mpsc::channel();
        self.commands
            .send(owner::Command::Key(name.to_string(), reply))
            .map_err(|_| KeyError::Session(InputError::Gone))?;
        receiver.recv_timeout(RESPONSE_TIMEOUT).unwrap_or_else(|error| {
            Err(match error {
                mpsc::RecvTimeoutError::Timeout => KeyError::TimedOut,
                mpsc::RecvTimeoutError::Disconnected => {
                    KeyError::Session(InputError::Gone)
                }
            })
        })
    }

    pub fn feed(&self, bytes: Vec<u8>) {
        let _ = self.commands.send(owner::Command::Feed(bytes));
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.commands.send(owner::Command::Resize(cols, rows));
    }

    pub fn read(&self, format: Format, max_bytes: Option<usize>) -> Vec<u8> {
        let (reply, receiver) = mpsc::channel();
        if self
            .commands
            .send(owner::Command::Read(format, max_bytes, reply))
            .is_err()
        {
            return Vec::new();
        }
        receiver.recv_timeout(RESPONSE_TIMEOUT).unwrap_or_default()
    }

    pub fn info(&self) -> Info {
        let (reply, receiver) = mpsc::channel();
        if self.commands.send(owner::Command::Info(reply)).is_ok()
            && let Ok(info) = receiver.recv_timeout(RESPONSE_TIMEOUT)
        {
            return info;
        }
        self.cached_info()
    }

    pub fn wait(&self, timeout: Duration) -> Option<Exit> {
        let (lock, changed) = &*self.wait_state;
        let result = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if result.is_some() || timeout.is_zero() {
            return result.clone();
        }
        let (result, _) = changed
            .wait_timeout_while(result, timeout, |result| result.is_none())
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        result.clone()
    }

    pub fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        let (reply, receiver) = mpsc::channel();
        if self.commands.send(owner::Command::Close(reply)).is_ok() {
            let _ = receiver.recv_timeout(RESPONSE_TIMEOUT);
        }
    }

    fn cached_info(&self) -> Info {
        self.info
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{Style, StyleColor, Underline, push_sgr};
    use super::wait_for_input_reply;
    use libghostty_vt::style::{PaletteIndex, RgbColor};
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn input_timeout_is_accepted() {
        let (_reply, receiver) = mpsc::channel();
        assert_eq!(wait_for_input_reply(receiver, Duration::ZERO), Ok(()));
    }

    fn sgr(style: Style) -> String {
        let mut out = Vec::new();
        push_sgr(&style, &mut out);
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn sgr_starts_from_reset_and_covers_colors_and_attributes() {
        assert_eq!(sgr(Style::default()), "\x1b[0m");
        let mut red_bold = Style::default();
        red_bold.bold = true;
        red_bold.fg_color = StyleColor::Palette(PaletteIndex(1));
        assert_eq!(sgr(red_bold), "\x1b[0;1;31m");
        let mut extended = Style::default();
        extended.bg_color = StyleColor::Palette(PaletteIndex(200));
        assert_eq!(sgr(extended), "\x1b[0;48;5;200m");
        let mut under = Style::default();
        under.fg_color = StyleColor::Rgb(RgbColor { r: 1, g: 2, b: 3 });
        under.underline = Underline::Curly;
        under.underline_color = StyleColor::Palette(PaletteIndex(4));
        assert_eq!(sgr(under), "\x1b[0;4:3;38;2;1;2;3;58;5;4m");
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}

fn validate_cwd(cwd: &Path) -> Result<(), SpawnError> {
    let metadata = std::fs::metadata(cwd)
        .map_err(|error| SpawnError::InvalidCwd(format!("{:?}", error.kind())))?;
    if !metadata.is_dir() {
        return Err(SpawnError::InvalidCwd("NotADirectory".into()));
    }
    Ok(())
}

/// Program and arguments for this process's login shell. A session the host
/// owns takes its shell from its own environment instead: see
/// [`session_login_shell`].
#[cfg(not(windows))]
pub fn login_shell() -> (String, Vec<String>) {
    match std::env::var("SHELL") {
        Ok(value) if !value.is_empty() => (value, vec!["-il".into()]),
        _ => default_login_shell(),
    }
}

/// Windows resolves its shell from `COMSPEC`, never the POSIX `$SHELL` which
/// may contain an MSYS path that CreateProcess cannot resolve.
#[cfg(windows)]
pub fn login_shell() -> (String, Vec<String>) {
    default_login_shell()
}

/// The login shell a session runs when it names no program: the `SHELL` in
/// the session's own environment. A session built from a client's environment
/// (`env_exact`, docs/superpowers/specs/2026-10-17-ade-programme-design.md
/// §5.3) runs that client's shell, never the host's.
#[cfg(not(windows))]
pub fn session_login_shell(env: &BTreeMap<String, String>) -> (String, Vec<String>) {
    match env.get("SHELL") {
        Some(value) if !value.is_empty() => (value.clone(), vec!["-il".into()]),
        _ => default_login_shell(),
    }
}

/// Windows ignores `SHELL` for the same reason [`login_shell`] does.
#[cfg(windows)]
pub fn session_login_shell(_env: &BTreeMap<String, String>) -> (String, Vec<String>) {
    default_login_shell()
}

#[cfg(target_os = "macos")]
fn default_login_shell() -> (String, Vec<String>) {
    ("/bin/zsh".into(), vec!["-il".into()])
}

#[cfg(all(unix, not(target_os = "macos")))]
fn default_login_shell() -> (String, Vec<String>) {
    let program = ["/bin/bash", "/bin/sh"]
        .into_iter()
        .find(|candidate| Path::new(candidate).exists())
        .unwrap_or("/bin/sh");
    (program.into(), vec!["-il".into()])
}

#[cfg(windows)]
fn default_login_shell() -> (String, Vec<String>) {
    (
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into()),
        Vec::new(),
    )
}

/// Prepare paths for a child process. Windows removes its verbatim prefix
/// because `cmd.exe` otherwise silently changes to the Windows directory.
pub fn spawn_cwd(path: &Path) -> PathBuf {
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

/// The text of the active screen and its scrollback, one line per row, trailing
/// blanks trimmed — the form `term.read text` returns and the snapshot test
/// compares. Public so a replica (the app's, a test's) renders it the same way.
pub fn render_text(terminal: &Terminal<'_, '_>) -> String {
    let options = FormatterOptions::new()
        .with_format(VtFormat::Plain)
        .with_unwrap(false)
        .with_trim(true);
    match Formatter::new(terminal, options)
        .and_then(|mut formatter| formatter.format_alloc(None).map(|bytes| bytes.as_ref().to_vec()))
    {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    }
}

/// The VT bytes that rebuild `terminal`'s active screen on a fresh, equally
/// sized terminal: content with styles and hyperlinks, modes, scrolling
/// region, tab stops, keyboard flags, charsets, cursor — and not the palette
/// or the working directory, which belong to the client (`vt_state_snapshot`).
pub fn render_snapshot(terminal: &Terminal<'_, '_>) -> Vec<u8> {
    let options = FormatterOptions::new()
        .with_format(VtFormat::Vt)
        .with_unwrap(true)
        .with_trim(true)
        .with_palette(false)
        .with_pwd(false)
        .with_modes(true)
        .with_scrolling_region(true)
        .with_tabstops(true)
        .with_keyboard(true)
        .with_cursor(true)
        .with_style(true)
        .with_hyperlink(true)
        .with_protection(true)
        .with_kitty_keyboard(true)
        .with_charsets(true);
    let mut bytes = Formatter::new(terminal, options)
        .and_then(|mut formatter| formatter.format_alloc(None).map(|bytes| bytes.as_ref().to_vec()))
        .unwrap_or_default();
    restore_cursor(terminal, &mut bytes);
    bytes
}

/// The formatter's trailing setup (tab stops) leaves the cursor wherever it
/// ends, not where the live cursor is; a replica fed the snapshot would
/// continue there, so the first following bytes can land on the wrong row.
/// Worse, the formatter trims trailing blank rows, so when the live cursor
/// sits below the last content row the replica has fewer rows: restore the
/// run of blanks with line feeds (each scrolls content into scrollback, which
/// is what the live screen did to get there — row counts stay identical),
/// then put the live cursor back with CUP (1-based). Best-effort throughout:
/// a snapshot must never fail because the cursor could not be read.
fn restore_cursor(terminal: &Terminal<'_, '_>, bytes: &mut Vec<u8>) {
    let (Ok(x), Ok(y)) = (terminal.cursor_x(), terminal.cursor_y()) else {
        return;
    };
    for _ in 0..trailing_blank_rows(terminal, y) {
        bytes.extend_from_slice(b"\n");
    }
    bytes.extend_from_slice(format!("\x1b[{};{}H", y as u32 + 1, x as u32 + 1).as_bytes());
    if terminal.is_cursor_pending_wrap().unwrap_or(false) {
        restore_pending_wrap(terminal, bytes, x, y);
    }
}

/// How many blank rows the formatter trimmed below the live cursor: the run
/// of rows with neither text nor styling ending at the cursor row. Zero when
/// the cursor sits on content (or when nothing could be read — never recreate
/// rows over unknown state).
fn trailing_blank_rows(terminal: &Terminal<'_, '_>, cursor_y: u16) -> u16 {
    let Ok(cols) = terminal.cols() else {
        return 0;
    };
    let mut blanks = 0u16;
    for row in (0..=cursor_y).rev() {
        if !row_is_blank(terminal, cols, row) {
            break;
        }
        blanks += 1;
    }
    // All rows down to the top are blank: the content, if any, is below the
    // cursor (or there is none) — a CUP alone puts the replica right.
    if blanks > cursor_y {
        return 0;
    }
    blanks
}

fn row_is_blank(terminal: &Terminal<'_, '_>, cols: u16, row: u16) -> bool {
    for col in 0..cols {
        let Ok(grid) = terminal.grid_ref(Point::Active(PointCoordinate {
            x: col,
            y: row as u32,
        })) else {
            return false;
        };
        let Ok(cell) = grid.cell() else {
            return false;
        };
        if cell.has_text().unwrap_or(true) || cell.has_styling().unwrap_or(true) {
            return false;
        }
    }
    true
}

/// Re-establish a pending wrap: move to the last column and re-emit the cell
/// there, so the next printable wraps exactly like the live screen. The cell
/// is overwritten with its own content and style, which is a no-op visually;
/// anything unreadable (wide spans, missing graphemes) skips the restore and
/// keeps today's behaviour instead of guessing.
fn restore_pending_wrap(terminal: &Terminal<'_, '_>, bytes: &mut Vec<u8>, x: u16, y: u16) {
    let Ok(cols) = terminal.cols() else {
        return;
    };
    // Pending wrap lives on the last column; anywhere else the CUP above is
    // already the whole truth.
    if cols == 0 || x != cols - 1 {
        return;
    }
    let Ok(grid) = terminal.grid_ref(Point::Active(PointCoordinate {
        x: cols - 1,
        y: y as u32,
    })) else {
        return;
    };
    let Ok(cell) = grid.cell() else {
        return;
    };
    if !matches!(cell.wide(), Ok(CellWide::Narrow)) {
        return;
    }
    let mut cluster = vec!['\0'; 16];
    let mut read = grid.graphemes(&mut cluster);
    if read.is_err() {
        cluster = vec!['\0'; 128];
        read = grid.graphemes(&mut cluster);
    }
    let Ok(count) = read else {
        return;
    };
    if count == 0 {
        return;
    }
    bytes.extend_from_slice(format!("\x1b[{};{}H", y as u32 + 1, cols as u32).as_bytes());
    push_sgr(&grid.style().unwrap_or_default(), bytes);
    let text: String = cluster[..count].iter().collect();
    bytes.extend_from_slice(text.as_bytes());
    match terminal.cursor_style() {
        Ok(current) => push_sgr(&current, bytes),
        Err(_) => bytes.extend_from_slice(b"\x1b[0m"),
    }
}

/// An SGR sequence for `style`, always starting from a reset so it is
/// self-contained: the replica's current style ends up exactly `style`.
fn push_sgr(style: &Style, out: &mut Vec<u8>) {
    let mut params: Vec<String> = vec!["0".to_string()];
    if style.bold {
        params.push("1".to_string());
    }
    if style.faint {
        params.push("2".to_string());
    }
    if style.italic {
        params.push("3".to_string());
    }
    match style.underline {
        Underline::None => {}
        Underline::Single => params.push("4".to_string()),
        Underline::Double => params.push("4:2".to_string()),
        Underline::Curly => params.push("4:3".to_string()),
        Underline::Dotted => params.push("4:4".to_string()),
        Underline::Dashed => params.push("4:5".to_string()),
        _ => {}
    }
    if style.blink {
        params.push("5".to_string());
    }
    if style.inverse {
        params.push("7".to_string());
    }
    if style.invisible {
        params.push("8".to_string());
    }
    if style.strikethrough {
        params.push("9".to_string());
    }
    if style.overline {
        params.push("53".to_string());
    }
    if let Some(param) = color_param(&style.fg_color, 30, 90, 38) {
        params.push(param);
    }
    if let Some(param) = color_param(&style.bg_color, 40, 100, 48) {
        params.push(param);
    }
    if !matches!(style.underline, Underline::None)
        && let Some(param) = underline_color_param(&style.underline_color)
    {
        params.push(param);
    }
    out.extend_from_slice(format!("\x1b[{}m", params.join(";")).as_bytes());
}

fn color_param(color: &StyleColor, plain: u32, bright: u32, eight_bit: u32) -> Option<String> {
    match color {
        StyleColor::None => None,
        StyleColor::Palette(index) => {
            let index = index.0 as u32;
            Some(if index < 8 {
                (plain + index).to_string()
            } else if index < 16 {
                (bright + index - 8).to_string()
            } else {
                format!("{eight_bit};5;{index}")
            })
        }
        StyleColor::Rgb(rgb) => Some(format!(
            "{eight_bit};2;{};{};{}",
            rgb.r,
            rgb.g,
            rgb.b
        )),
    }
}

fn underline_color_param(color: &StyleColor) -> Option<String> {
    match color {
        StyleColor::None => None,
        StyleColor::Palette(index) => Some(format!("58;5;{}", index.0)),
        StyleColor::Rgb(rgb) => Some(format!("58;2;{};{};{}", rgb.r, rgb.g, rgb.b)),
    }
}

pub(crate) fn initialize_terminal(cols: u16, rows: u16) -> Terminal<'static, 'static> {
    let mut terminal = Terminal::new(libghostty_vt::TerminalOptions {
        cols,
        rows,
        max_scrollback: 10_000,
    })
    .expect("terminal session: construct the VT");
    terminal
        .set_mode(libghostty_vt::terminal::Mode::GRAPHEME_CLUSTER, true)
        .expect("terminal session: enable DEC 2027 grapheme clustering");
    terminal
}
