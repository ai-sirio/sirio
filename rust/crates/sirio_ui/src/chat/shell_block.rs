//! The block a `!` draft makes in the transcript: the command, its output while it runs and after,
//! and how it ended. The command itself is `sirio_chat::shell::ShellRun`; this is its place in the
//! chat (`Entry::Shell`), the task that carries its output into the block, and the drawing.
//!
//! Output never reaches the UI thread byte by byte. A task on the background executor drains the
//! run's channel, keeps the command's [`ShellOutput`] and, at most every [`REPAINT_EVERY`] (and once
//! when the command ends), hands the block a [`ShellUpdate`]: the output as it stands, and what a
//! frame needs of it (the folded tail and the line counts), already worked out. A frame therefore
//! never normalises output, and a command that writes as fast as it can (`yes`) costs the UI thread
//! one update per tick. Reading is never throttled: a slow reader would hold the command back
//! through the full pipe.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::{FutureExt as _, StreamExt as _, select_biased};
use gpui::{Div, Stateful};
use sirio_chat::shell::context::{CONTEXT_OUTPUT_BUDGET, ShellRecord, cap_output};
use sirio_chat::shell::output::ShellOutput;
use sirio_chat::shell::{ShellEvent, ShellRun};
use sirio_persistence::ChatShellRun;

use super::*;
use ely_gpui_component::buttons::{Button, ButtonVariant};
use ely_gpui_component::motion::Spinner;
use ely_gpui_component::theme::ControlSize;

/// How many lines a block shows folded: the last ones.
pub(super) const FOLD_LINES: usize = 40;

/// The most a block the agent has been given stores of its output: its fold, and no more than this.
pub(super) const STORED_SENT_BYTES: usize = 8 * 1024;

/// What the footer says while the agent has not been given the block.
const CAPTION: &str = "goes with your next message";

/// The least time between two updates of a running block's output on screen.
const REPAINT_EVERY: Duration = Duration::from_millis(100);

/// The tallest the expanded body grows before it scrolls.
const EXPANDED_MAX_HEIGHT: f32 = 360.0;

/// How many lines an expanded block draws while its command still runs: the last ones. Everything kept
/// (up to 1 MiB, a hundred thousand lines or more) is laid out again with every update, which stalls the
/// window; once the command has ended the block draws everything it kept, which is laid out once.
const LIVE_LINES: usize = 1_000;

/// The most characters of a command a tooltip shows.
const TOOLTIP_CHARS: usize = 2_000;

/// What the next process-wide id is.
static NEXT_SHELL_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// A block's identity for as long as the process runs: the entry's index can move, this does not.
pub(super) fn next_shell_id() -> u64 {
    NEXT_SHELL_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Whether `draft` is in command mode: its first character is `!`. Leading whitespace does not
/// count, so ` !important` is an ordinary prompt.
pub(super) fn shell_mode(draft: &str) -> bool {
    draft.starts_with('!')
}

/// The command a command-mode draft runs: what follows the `!`, as typed. `None` when that is
/// blank (a bare `!`) or the draft is not in command mode.
pub(super) fn shell_command(draft: &str) -> Option<&str> {
    let command = draft.strip_prefix('!')?;
    (!command.trim().is_empty()).then_some(command)
}

/// Where a block stands.
#[derive(Clone, Debug)]
pub(super) enum ShellState {
    Running {
        started: Instant,
    },
    Finished {
        /// The shell's exit status; a signal's death is `128 + signal`. `None` for a command that
        /// never started.
        exit: Option<i32>,
        /// The user stopped it (Stop, Esc), or the chat went away under it.
        stopped: bool,
        /// The command could not start (`Could not run: …`).
        failed: bool,
        duration_ms: u64,
    },
}

/// How a command ended, as the task that watches it reports it.
#[derive(Clone, Copy, Debug)]
struct ShellExit {
    code: Option<i32>,
    stopped: bool,
    duration_ms: u64,
}

/// What the block is handed, at most every [`REPAINT_EVERY`] and when the command ends.
pub(super) struct ShellUpdate {
    output: Arc<ShellOutput>,
    /// The last [`FOLD_LINES`] lines.
    folded: String,
    /// The last [`LIVE_LINES`] lines.
    live: String,
    kept_lines: usize,
    total_lines: u64,
    exit: Option<ShellExit>,
}

/// One shell command as the transcript holds it. Cheap to clone: a frame clones the entry it draws.
#[derive(Clone, Debug)]
pub(super) struct ShellBlock {
    pub(super) id: u64,
    /// What the user typed after the `!`, as typed.
    pub(super) command: String,
    /// The output as of the last update. Not drawn directly: see `drawn`.
    output: Arc<ShellOutput>,
    pub(super) state: ShellState,
    /// The agent has been given this block with a prompt (a restored block keeps what it was).
    pub(super) sent: bool,
    pub(super) expanded: bool,
    /// The last [`FOLD_LINES`] lines of `output`, as the last update brought them.
    folded: SharedString,
    /// The last [`LIVE_LINES`] lines, as the last update brought them.
    live: SharedString,
    /// What the body draws: `folded`; while `expanded`, `live` for a command that runs, and every line
    /// kept for one that ended. Worked out when the output or `expanded` changes, never in a frame.
    drawn: SharedString,
    kept_lines: usize,
    total_lines: u64,
    /// The command has been asked to stop (the block's Stop, or Escape): the next Escape kills it at
    /// once instead of asking again.
    stop_asked: bool,
    /// Escape has killed it. A command that survives a SIGKILL (a process in uninterruptible sleep)
    /// does not count as running for the next Escape, which then reaches the turn; the block reads
    /// running until its run really ends.
    killed: bool,
}

impl ShellBlock {
    fn with_output(command: String, output: ShellOutput, state: ShellState) -> Self {
        let folded = SharedString::from(output.tail(FOLD_LINES));
        Self {
            id: next_shell_id(),
            command,
            kept_lines: output.kept_lines(),
            total_lines: output.total_lines(),
            output: Arc::new(output),
            state,
            sent: false,
            expanded: false,
            drawn: folded.clone(),
            live: folded.clone(),
            folded,
            stop_asked: false,
            killed: false,
        }
    }

    /// A command that has just been started and has printed nothing yet.
    pub(super) fn running(command: String) -> Self {
        Self::with_output(
            command,
            ShellOutput::default(),
            ShellState::Running {
                started: Instant::now(),
            },
        )
    }

    /// A command that could not start: a finished block with no exit status, whose only line says
    /// why.
    pub(super) fn could_not_start(command: String, error: &std::io::Error) -> Self {
        Self::with_output(
            command,
            ShellOutput::from_text(&format!("Could not run: {error}"), 1),
            ShellState::Finished {
                exit: None,
                stopped: false,
                failed: true,
                duration_ms: 0,
            },
        )
    }

    pub(super) fn is_finished(&self) -> bool {
        matches!(self.state, ShellState::Finished { .. })
    }

    /// When a running command was started.
    fn started(&self) -> Option<Instant> {
        match self.state {
            ShellState::Running { started } => Some(started),
            ShellState::Finished { .. } => None,
        }
    }

    /// What the body draws, which the read row reports as the block's `output`.
    pub(super) fn drawn_output(&self) -> &str {
        &self.drawn
    }

    /// Every line the command produced (the lines dropped past the memory cap included).
    pub(super) fn total_lines(&self) -> u64 {
        self.total_lines
    }

    /// The shell's exit status, once it has one.
    pub(super) fn exit(&self) -> Option<i32> {
        match self.state {
            ShellState::Finished { exit, .. } => exit,
            ShellState::Running { .. } => None,
        }
    }

    /// `running`, `exited`, `stopped` or `failed`, for the read row.
    pub(super) fn status(&self) -> &'static str {
        match self.state {
            ShellState::Running { .. } => "running",
            ShellState::Finished { failed: true, .. } => "failed",
            ShellState::Finished { stopped: true, .. } => "stopped",
            ShellState::Finished { .. } => "exited",
        }
    }

    /// The label of the control under the body, `None` when the block has nothing more to show.
    /// Compared with saturation: the kept lines can be one more than the command's (a marker, an
    /// unfinished line), and a restored block can have kept fewer than it produced.
    pub(super) fn fold_control(&self) -> Option<String> {
        let dropped = self.total_lines.saturating_sub(self.kept_lines as u64) > 0;
        if self.kept_lines <= FOLD_LINES && !dropped {
            return None;
        }
        Some(if self.expanded {
            "Show less".to_string()
        } else {
            // The kept lines can be one more than the command's (a marker, an unfinished line), and
            // all of them are drawn.
            format!("Show all {} lines", self.total_lines.max(self.kept_lines as u64))
        })
    }

    /// Takes an update from the task that watches the command. Returns whether the command ended.
    pub(super) fn apply(&mut self, update: ShellUpdate) -> bool {
        self.output = update.output;
        self.folded = update.folded.into();
        self.live = update.live.into();
        self.kept_lines = update.kept_lines;
        self.total_lines = update.total_lines;
        let ended = update.exit.is_some();
        if let Some(exit) = update.exit {
            self.state = ShellState::Finished {
                exit: exit.code,
                stopped: exit.stopped,
                failed: false,
                duration_ms: exit.duration_ms,
            };
        }
        self.refresh_drawn();
        ended
    }

    /// Flips between the folded tail and the expanded body: everything kept once the command has
    /// ended, its last [`LIVE_LINES`] lines while it runs.
    pub(super) fn toggle_expanded(&mut self) {
        self.expanded = !self.expanded;
        self.refresh_drawn();
    }

    fn refresh_drawn(&mut self) {
        self.drawn = if !self.expanded {
            self.folded.clone()
        } else if self.is_finished() {
            SharedString::from(self.output.text())
        } else {
            self.live.clone()
        };
    }

    /// `exit 0 · 0.4s`, `exit 3 · 0.4s` or `Stopped · 2.0s`: what the footer says of how it ended.
    /// `None` while it runs and for a command that never started.
    fn footer_text(&self) -> Option<String> {
        let ShellState::Finished {
            exit,
            stopped,
            failed,
            duration_ms,
        } = self.state
        else {
            return None;
        };
        if failed {
            return None;
        }
        let mut parts = Vec::new();
        if stopped {
            parts.push("Stopped".to_string());
        } else if let Some(code) = exit {
            parts.push(format!("exit {code}"));
        }
        parts.push(duration_text(duration_ms));
        Some(parts.join(" · "))
    }

    /// What the footer says after how it ended while the agent has not been given the block. It
    /// is drawn beside the footer's text and is no part of it: not selected, not copied, not in
    /// the text a closed tab keeps. The read row reports it (`caption`), from here.
    pub(super) fn caption(&self) -> Option<&'static str> {
        (self.is_finished() && !self.sent).then_some(CAPTION)
    }

    /// The block as the agent is told of it: the command, everything its output kept, and how it
    /// ended. `None` while the command runs, which is not pending yet. The exit is the runner's
    /// own: `None` only for a command that never started, so `<bash-exit>` reads `not run` for that
    /// one and for no other.
    pub(super) fn record(&self) -> Option<ShellRecord> {
        let ShellState::Finished { exit, stopped, .. } = self.state else {
            return None;
        };
        Some(ShellRecord {
            command: self.command.clone(),
            output: self.output.text(),
            exit,
            stopped,
        })
    }

    /// The block as the transcript stores it (spec 5.6). An unsent block stores what it would
    /// send: its output, cut to the context budget. A sent block stores only its fold, the last
    /// [`FOLD_LINES`] lines and at most [`STORED_SENT_BYTES`]. `None` while the command runs: a
    /// run that started but has not ended has no exit to store, and is saved when it finishes.
    pub(super) fn stored(&self) -> Option<ChatShellRun> {
        let ShellState::Finished {
            exit,
            stopped,
            duration_ms,
            ..
        } = self.state
        else {
            return None;
        };
        let output = if self.sent {
            last_bytes(self.output.tail(FOLD_LINES), STORED_SENT_BYTES)
        } else {
            cap_output(&self.output.text(), CONTEXT_OUTPUT_BUDGET)
        };
        Some(ChatShellRun {
            command: self.command.clone(),
            output,
            total_lines: self.output.total_lines(),
            exit,
            stopped,
            duration_ms,
            sent: self.sent,
        })
    }

    /// What an older build draws in place of the block: `! {command}` and the stored output.
    pub(super) fn fallback_text(run: &ChatShellRun) -> String {
        if run.output.is_empty() {
            format!("! {}", run.command)
        } else {
            format!("! {}\n{}", run.command, run.output)
        }
    }

    /// A block as the transcript stored it: finished, folded, with what it kept of its output. The
    /// stored text is read again as untrusted. A command that had no exit and was not stopped never
    /// started.
    pub(super) fn restored(run: ChatShellRun) -> Self {
        let output = ShellOutput::from_text(&run.output, run.total_lines);
        let mut block = Self::with_output(
            run.command,
            output,
            ShellState::Finished {
                failed: run.exit.is_none() && !run.stopped,
                exit: run.exit,
                stopped: run.stopped,
                duration_ms: run.duration_ms,
            },
        );
        block.sent = run.sent;
        block
    }

    /// The block as the transcript's selection reads it (`Entry::plain_text`): the command, the
    /// body, the footer's words. What a copy takes, and the offset space the drawn text sits in.
    pub(super) fn plain_text(&self) -> String {
        let mut text = format!("! {}", self.command);
        if !self.drawn.is_empty() {
            text.push('\n');
            text.push_str(&self.drawn);
        }
        if let Some(footer) = self.footer_text() {
            text.push('\n');
            text.push_str(&footer);
        }
        text
    }

    /// `plain_text().len()`, for the per-frame sum of the transcript's offsets.
    pub(super) fn plain_len(&self) -> usize {
        let mut len = 2 + self.command.len();
        if !self.drawn.is_empty() {
            len += 1 + self.drawn.len();
        }
        if let Some(footer) = self.footer_text() {
            len += 1 + footer.len();
        }
        len
    }
}

/// `text` cut to its last `max` bytes: on a character boundary, and at a line start when that
/// loses no more than the first, partly cut line.
fn last_bytes(text: String, max: usize) -> String {
    if text.len() <= max {
        return text;
    }
    let mut cut = text.len() - max;
    while !text.is_char_boundary(cut) {
        cut += 1;
    }
    let kept = &text[cut..];
    // The cut may have fallen inside a line: its rest is not a line, unless it is all there is.
    if text.as_bytes()[cut - 1] != b'\n'
        && let Some(newline) = kept.find('\n')
        && newline + 1 < kept.len()
    {
        return kept[newline + 1..].to_string();
    }
    kept.to_string()
}

/// `412ms`, `3.1s`, `5m 02s`.
fn duration_text(ms: u64) -> String {
    if ms < 1_000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let seconds = ms / 1000;
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
}

/// The command on one line, for the header: its lines trimmed and joined by `↵`.
fn header_line(command: &str) -> String {
    command
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ↵ ")
}

/// The command as a tooltip shows it: all of it, within [`TOOLTIP_CHARS`].
fn tooltip_text(command: &str) -> String {
    let mut shown: String = command.chars().take(TOOLTIP_CHARS).collect();
    if shown.len() < command.len() {
        shown.push('…');
    }
    shown
}

/// What stands between a command's output and the block that shows it. Everything here runs on the
/// background executor.
struct Pump {
    output: ShellOutput,
    /// Output has arrived that no update has carried to the block yet.
    dirty: bool,
    /// When the last update went.
    sent: Instant,
    started: Instant,
    exit: Option<ShellExit>,
}

impl Pump {
    fn new(started: Instant) -> Self {
        Self {
            output: ShellOutput::default(),
            dirty: false,
            // The first output is shown at once.
            sent: started.checked_sub(REPAINT_EVERY).unwrap_or(started),
            started,
            exit: None,
        }
    }

    fn take(&mut self, event: ShellEvent) {
        match event {
            ShellEvent::Output(bytes) => self.output.push(&bytes),
            ShellEvent::Exited { code, stopped } => {
                self.output.finish();
                self.exit = Some(ShellExit {
                    code,
                    stopped,
                    duration_ms: self.started.elapsed().as_millis() as u64,
                });
            }
        }
        self.dirty = true;
    }

    /// Whether an update should go now: something is waiting, and the command ended or the last
    /// update is old enough.
    fn due(&self) -> bool {
        self.dirty && (self.exit.is_some() || self.sent.elapsed() >= REPAINT_EVERY)
    }

    /// How long until the output that is waiting is due.
    fn wait(&self) -> Duration {
        REPAINT_EVERY.saturating_sub(self.sent.elapsed())
    }

    fn update(&mut self) -> ShellUpdate {
        self.dirty = false;
        self.sent = Instant::now();
        ShellUpdate {
            folded: self.output.tail(FOLD_LINES),
            live: self.output.tail(LIVE_LINES),
            kept_lines: self.output.kept_lines(),
            total_lines: self.output.total_lines(),
            output: Arc::new(self.output.clone()),
            exit: self.exit,
        }
    }
}

impl Chat {
    /// Enter in command mode: runs what follows the `!` and clears the draft. A bare `!` changes
    /// nothing. Whatever state the agent is in does not matter, and a held quote and attached
    /// images stay for the next prompt.
    pub(super) fn run_shell_draft(&mut self, cx: &mut Context<Self>) {
        let Some(command) = shell_command(&self.draft).map(str::to_owned) else {
            return;
        };
        self.remember_prompt();
        self.accepted_mentions.clear();
        self.reset_composer_popups();
        self.set_composer_text("", cx);
        self.start_shell(command, cx);
    }

    /// Starts `command` in the chat's working directory and puts its block in the transcript. A
    /// command that cannot start is a finished block that says why.
    pub(super) fn start_shell(&mut self, command: String, cx: &mut Context<Self>) {
        let run = match ShellRun::start(&command, &self.agent_cwd) {
            Ok(run) => run,
            Err(error) => {
                self.push_entry(Entry::Shell(ShellBlock::could_not_start(command, &error)));
                // It is finished already: saved like any block that ends.
                self.persist_settled_transcript();
                cx.notify();
                return;
            }
        };
        let block = ShellBlock::running(command);
        let id = block.id;
        let started = block.started().unwrap_or_else(Instant::now);
        let events = run.events();
        self.shell_runs.insert(id, run);
        self.push_entry(Entry::Shell(block));

        let executor = cx.background_executor().clone();
        let (updates, mut arrived) = futures::channel::mpsc::unbounded::<ShellUpdate>();
        let pump = cx.background_spawn(async move {
            let mut pump = Pump::new(started);
            loop {
                // The next event, or, while output waits for its turn, the moment that turn comes.
                let event = if pump.dirty {
                    let next = events.recv().fuse();
                    let turn = executor.timer(pump.wait()).fuse();
                    futures::pin_mut!(next, turn);
                    select_biased! {
                        event = next => Some(event),
                        () = turn => None,
                    }
                } else {
                    Some(events.recv().await)
                };
                match event {
                    Some(Ok(event)) => pump.take(event),
                    // The channel closed with no end event: the command is gone all the same, and the
                    // runner's own "the system could not say how it ended" is -1.
                    Some(Err(_)) => pump.take(ShellEvent::Exited {
                        code: Some(-1),
                        stopped: false,
                    }),
                    None => {}
                }
                // Everything already queued, so the command is never held back by this task. A
                // command that never stops writing still gets its updates: the turn is checked
                // between events.
                while pump.exit.is_none() {
                    if pump.due() && updates.unbounded_send(pump.update()).is_err() {
                        return;
                    }
                    let Ok(event) = events.try_recv() else { break };
                    pump.take(event);
                }
                if pump.due() && updates.unbounded_send(pump.update()).is_err() {
                    return;
                }
                if pump.exit.is_some() {
                    return;
                }
            }
        });
        // Only the weak handle goes in: a closed chat is released, and with it the run.
        cx.spawn(async move |this, cx| {
            let _pump = pump;
            while let Some(mut update) = arrived.next().await {
                // What piled up while the UI was busy: only the newest says where the output stands,
                // and the one that ends the command is always the last.
                while let Ok(newer) = arrived.try_recv() {
                    update = newer;
                }
                let ended = update.exit.is_some();
                if this
                    .update(cx, |chat, cx| chat.apply_shell_update(id, update, cx))
                    .is_err()
                    || ended
                {
                    return;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// The task's update reaches the block. A block that is gone (a new conversation) is ignored.
    fn apply_shell_update(&mut self, id: u64, update: ShellUpdate, cx: &mut Context<Self>) {
        let ended = update.exit.is_some();
        let mut applied = false;
        if let Some(index) = self.shell_index(id)
            && let Entry::Shell(block) = &mut self.entries[index]
        {
            block.apply(update);
            self.remeasure_entry(index);
            self.clamp_transcript_selection();
            applied = true;
        }
        if ended {
            // The run is over: dropping it has nothing left to kill. One that is no longer in the
            // map was killed by the chat itself (a quit, a new conversation): that end is not the
            // user's, and a command that was running when the app quit is not saved (spec 5.6).
            let own_end = self.shell_runs.remove(&id).is_some();
            if applied && own_end {
                self.persist_settled_transcript();
            }
        }
        cx.notify();
    }

    /// Where the block with this id is in the transcript.
    fn shell_index(&self, id: u64) -> Option<usize> {
        self.entries
            .iter()
            .rposition(|entry| matches!(entry, Entry::Shell(block) if block.id == id))
    }

    /// Escape: ends every command of this chat that is still running. Returns whether it acted on
    /// one, so that the turn is left alone. A command that has been asked to stop is running until
    /// its block reads stopped, and that can take 2 s (SIGTERM, then SIGKILL): an Escape that comes
    /// then is for the command, and kills it at once. Once Escape has killed a command it is done
    /// with it, so that one that survives the SIGKILL cannot keep Escape from ever stopping the
    /// turn; its block reads running until its run really ends.
    pub(super) fn stop_running_shells(&mut self, cx: &mut Context<Self>) -> bool {
        let mut acted = false;
        let mut gone = Vec::new();
        for (id, run) in &self.shell_runs {
            let block = self.entries.iter_mut().rev().find_map(|entry| match entry {
                Entry::Shell(block) if block.id == *id => Some(block),
                _ => None,
            });
            match block {
                // Killed already: Escape has nothing more to do for it.
                Some(block) if block.killed => {}
                // The first Escape, or the first since the block's own Stop: SIGTERM, and SIGKILL
                // after 2 s.
                Some(block) if !block.stop_asked => {
                    run.stop();
                    block.stop_asked = true;
                    acted = true;
                }
                // It was asked already and is still here: it ignored the SIGTERM.
                Some(block) => {
                    run.kill();
                    block.killed = true;
                    acted = true;
                }
                // A run with no block to say so: killed, and let go.
                None => {
                    run.kill();
                    gone.push(*id);
                    acted = true;
                }
            }
        }
        for id in gone {
            self.shell_runs.remove(&id);
        }
        if acted {
            cx.notify();
        }
        acted
    }

    /// The Stop button of one block.
    pub(super) fn stop_shell(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(run) = self.shell_runs.get(&id) {
            run.stop();
        }
        if let Some(index) = self.shell_index(id)
            && let Entry::Shell(block) = &mut self.entries[index]
        {
            block.stop_asked = true;
        }
        cx.notify();
    }

    /// Show all / Show less on the block at transcript row `entry`. Returns whether the row has
    /// that control.
    pub(super) fn toggle_shell(&mut self, entry: usize, cx: &mut Context<Self>) -> bool {
        let Some(Entry::Shell(block)) = self.entries.get_mut(entry) else {
            return false;
        };
        if block.fold_control().is_none() {
            return false;
        }
        block.toggle_expanded();
        self.remeasure_entry(entry);
        self.clamp_transcript_selection();
        cx.notify();
        true
    }

    /// The press of the control, which knows the block by its id.
    fn toggle_shell_block(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(index) = self.shell_index(id) {
            self.toggle_shell(index, cx);
        }
    }

    /// The context a prompt typed as `typed` carries (spec 5.4): every block that has finished and
    /// has not been sent, oldest first, in Claude Code's tags. They are marked sent, which takes
    /// their caption away, and the transcript is saved so that a restart does not send them twice.
    /// `None` when nothing is pending, and for a slash command, which the agent reads at the head
    /// of the prompt: those blocks stay pending. A block that still runs is not pending yet.
    pub(super) fn carry_shell_context(&mut self, typed: &str) -> Option<retry::ShellCarry> {
        if quote::is_slash_command(typed) {
            return None;
        }
        let (mut ids, mut records) = (Vec::new(), Vec::new());
        for entry in &self.entries {
            if let Entry::Shell(block) = entry
                && !block.sent
                && let Some(record) = block.record()
            {
                ids.push(block.id);
                records.push(record);
            }
        }
        if ids.is_empty() {
            return None;
        }
        let text = sirio_chat::shell::context::context_for(&records);
        self.mark_shell_sent(&ids, true);
        self.persist_settled_transcript();
        Some(retry::ShellCarry { ids, text })
    }

    /// The prompt that carried these blocks did not reach an agent that has them (a hand-back):
    /// they are pending again, with their caption, and the transcript says so.
    pub(super) fn release_shell_carry(&mut self, carry: &retry::ShellCarry) {
        if self.mark_shell_sent(&carry.ids, false) {
            self.persist_settled_transcript();
        }
    }

    /// Sets `sent` on the blocks with these ids. Returns whether any block changed. The caption
    /// goes or comes back with it, so the rows are measured again (it is no part of the selectable
    /// text, so no selection moves).
    fn mark_shell_sent(&mut self, ids: &[u64], sent: bool) -> bool {
        let mut changed = Vec::new();
        for (index, entry) in self.entries.iter_mut().enumerate() {
            if let Entry::Shell(block) = entry
                && ids.contains(&block.id)
                && block.sent != sent
            {
                block.sent = sent;
                changed.push(index);
            }
        }
        for &index in &changed {
            self.remeasure_entry(index);
        }
        !changed.is_empty()
    }

    /// Kills every command this chat is running. A `ShellRun` that is dropped kills its whole
    /// session and never waits.
    pub(super) fn kill_shell_runs(&mut self) {
        self.shell_runs.clear();
    }
}

/// The key of a block's expanded well in `Chat::tool_output_scroll`.
pub(super) fn well_key(entry: usize) -> String {
    format!("shell-output-{entry}")
}

/// The text of the transcript's selection, from a string that is shared and not copied per frame.
fn selectable(
    text: SharedString,
    id: String,
    source_start: usize,
    interaction: &TranscriptInteraction,
    theme: &Theme,
) -> AnyElement {
    let range = source_start..source_start + text.len();
    TranscriptSelectableText::new(
        ElementId::Name(id.into()),
        StyledText::new(text),
        range,
        interaction.clone(),
        theme.ely.active,
        Vec::new(),
    )
    .into_any_element()
}

/// Draws a block: the header (the command, and while it runs a spinner and Stop), the output in a
/// box, and under it the fold control and the footer. The output goes through the transcript's
/// selection, at the offsets `ShellBlock::plain_text` gives it, and nothing in it is clickable.
pub(super) fn render(
    block: &ShellBlock,
    index: usize,
    source_start: usize,
    interaction: &TranscriptInteraction,
    chat: Entity<Chat>,
    scrolls: &HashMap<String, ScrollHandle>,
    theme: &Theme,
) -> AnyElement {
    let typography = theme.typography;
    let id = block.id;
    let running = !block.is_finished();
    let header_len = 2 + block.command.len();
    let body = block.drawn_output();
    let body_start = source_start + header_len + 1;
    let footer_start = body_start + if body.is_empty() { 0 } else { body.len() + 1 };

    let stop_chat = chat.clone();
    let header = div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .id(("shell-header", index))
                .debug_selector(move || format!("shell-header-{index}"))
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_family(typography.code_family)
                .text_size(typography.footnote)
                .text_color(theme.ely.fg)
                .tooltip(crate::controls::tooltip(tooltip_text(&block.command)))
                .child(format!("! {}", header_line(&block.command))),
        )
        .when(running, |row| {
            row.child(
                Spinner::new(("shell-spinner", index))
                    .size(ely_gpui_component::theme::IconSize::Xs),
            )
            .child(
                div()
                    .flex_none()
                    .debug_selector(move || format!("shell-stop-{index}"))
                    .child(
                        Button::new(("shell-stop", index), "Stop")
                            .variant(ButtonVariant::Secondary)
                            .size(ControlSize::Sm)
                            .on_click(move |_, _, cx| {
                                stop_chat.update(cx, |chat, cx| chat.stop_shell(id, cx));
                            }),
                    ),
            )
        });

    // The text, in the code font, one column per character: a long line scrolls sideways instead
    // of wrapping into the next.
    let text = (!body.is_empty()).then(|| {
        div()
            .id(("shell-body", index))
            .debug_selector(move || format!("shell-body-{index}"))
            .overflow_x_scroll()
            // A vertical wheel over the block keeps scrolling the transcript.
            .restrict_scroll_to_axis()
            .whitespace_nowrap()
            .font_family(typography.code_family)
            .text_size(typography.footnote)
            .text_color(theme.ely.fg_muted)
            .child(selectable(
                block.drawn.clone(),
                format!("shell-text-{index}"),
                body_start,
                interaction,
                theme,
            ))
    });
    let well = text.map(|text| {
        if !block.expanded {
            return text.into_any_element();
        }
        // Everything kept can be a great deal: it scrolls inside a box of its own, which holds the
        // wheel while it has room to move and lets it through at its edges.
        let mut well = div()
            .id(("shell-well", index))
            .max_h(px(EXPANDED_MAX_HEIGHT))
            .overflow_y_scroll()
            .child(text);
        if let Some(handle) = scrolls.get(&well_key(index)) {
            let handle = handle.clone();
            well = well.track_scroll(&handle).on_scroll(move |event, _, cx| {
                let delta_y = event.delta.pixel_delta(px(20.0)).y;
                if tool_calls::tool_output_consumes_scroll(
                    handle.offset().y,
                    handle.max_offset().y,
                    delta_y,
                ) {
                    cx.stop_propagation();
                }
            });
        }
        well.into_any_element()
    });
    let has_body = well.is_some();
    let boxed = (has_body || !running).then(|| {
        div()
            .w_full()
            .rounded(theme.radii.code_block)
            .bg(theme.ely.surface)
            .border_1()
            .border_color(theme.ely.border)
            .px(px(12.0))
            .py(px(8.0))
            .when_some(well, |boxed, well| boxed.child(well))
            .when(!has_body, |boxed| {
                boxed.child(
                    div()
                        .font_family(typography.code_family)
                        .text_size(typography.footnote)
                        .text_color(theme.ely.fg_subtle)
                        .child("(no output)"),
                )
            })
    });

    let control = block.fold_control();
    let footer = block.footer_text();
    let has_footer = footer.is_some();
    let ShellState::Finished {
        exit,
        stopped,
        failed,
        ..
    } = block.state
    else {
        return div()
            .id(("shell-block", index))
            .debug_selector(move || format!("shell-block-{index}"))
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(header)
            .children(boxed)
            .when_some(control, |column, label| {
                column.child(fold_row(index, id, label, chat, None))
            })
            .into_any_element();
    };
    // A command that failed to start or ended non-zero is marked in the danger colour, one that
    // ended well in the success colour, and one that was stopped is neither.
    let (glyph, tone) = if stopped {
        (Icon::XCircle, theme.ely.fg_subtle)
    } else if failed || exit.is_some_and(|code| code != 0) {
        (Icon::XCircle, theme.ely.danger)
    } else {
        (Icon::Check, theme.ely.success)
    };
    let status = div()
        .id(("shell-footer", index))
        .debug_selector(move || format!("shell-footer-{index}"))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(typography.caption2)
        .text_color(theme.ely.fg_subtle)
        .child(IconElement::new(glyph, IconSize::XSmall).text_color(tone))
        .children(footer.map(|footer| {
            selectable(
                SharedString::from(footer),
                format!("shell-footer-text-{index}"),
                footer_start,
                interaction,
                theme,
            )
        }))
        .when_some(block.caption(), |status, caption| {
            // After the words of how it ended, a separator; after the mark alone, nothing.
            status
                .when(has_footer, |status| status.child(div().child("·")))
                .child(div().child(caption))
        });
    div()
        .id(("shell-block", index))
        .debug_selector(move || format!("shell-block-{index}"))
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(header)
        .children(boxed)
        .child(fold_row(index, id, control.unwrap_or_default(), chat, Some(status)))
        .into_any_element()
}

/// The row under the body: the fold control on the left (when there is one), the footer on the
/// right.
fn fold_row(
    index: usize,
    id: u64,
    label: String,
    chat: Entity<Chat>,
    status: Option<Stateful<Div>>,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .child(div().flex_none().debug_selector(move || format!("shell-fold-{index}")).when(
            !label.is_empty(),
            |slot| {
                slot.child(
                    Button::new(("shell-fold", index), label)
                        .variant(ButtonVariant::Ghost)
                        .size(ControlSize::Sm)
                        .on_click(move |_, _, cx| {
                            chat.update(cx, |chat, cx| chat.toggle_shell_block(id, cx));
                        }),
                )
            },
        ))
        .children(status)
        .into_any_element()
}
