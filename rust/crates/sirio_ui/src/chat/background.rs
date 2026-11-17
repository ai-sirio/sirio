//! The session's background tasks (A3): the registry the CLI's own lines fill,
//! the tail of a shell's output read for the tasks a frame paints, and Stop.
//!
//! A background task is not a trace lane. The CLI says `idle` as soon as a
//! shell starts, so a span that outlived its turn would break the trace's
//! invariants (A3 ruling R1); the task lives here, in a registry that belongs
//! to the connection and ends with it.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use gpui::Context;
use sirio_chat::{BackgroundTaskEvent, TaskOutput};

use super::{Chat, ChatEvent};

/// How often the poll looks for a painted tail.
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// How long an ended shell is still read, for the trailer the CLI appends
/// (`[killed]`, `[exited with code N]`) to the output a moment after the end.
const ENDED_READ_WINDOW: Duration = Duration::from_secs(5);
/// The lines of a tail the registry keeps: the CLI's answer is the last 8 KiB.
const TAIL_KEEP: usize = 100;
/// The CLI's word for a shell or a Monitor, the only tasks `get_task_output`
/// answers for.
const SHELL_TYPE: &str = "local_bash";

/// One background task as the session knows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackgroundEntry {
    pub task_id: String,
    /// The tool call that started it, once a `task_started` said so.
    pub tool_use_id: Option<String>,
    /// The CLI's own word for what runs it, e.g. `local_bash`.
    pub task_type: Option<String>,
    pub description: String,
    /// Whether the CLI's last list of live tasks held it.
    pub live: bool,
    /// The first terminal status the CLI gave it; a later one never replaces it.
    pub outcome: Option<String>,
    /// Stop was pressed and the CLI has not yet said the task ended.
    pub stopping: bool,
    /// The last lines of its output, as the last answer gave them.
    pub tail: Vec<String>,
    /// Why the tail could not be read, when it could not.
    pub tail_note: Option<String>,
}

impl BackgroundEntry {
    fn new(task_id: &str) -> Self {
        Self {
            task_id: task_id.to_string(),
            tool_use_id: None,
            task_type: None,
            description: String::new(),
            live: true,
            outcome: None,
            stopping: false,
            tail: Vec::new(),
            tail_note: None,
        }
    }

    /// The task is over: the CLI's rows no longer hold it, or it has an outcome.
    pub fn ended(&self) -> bool {
        !self.live || self.outcome.is_some()
    }

    /// A shell or a Monitor: what the CLI can read the output of, and so the
    /// only task whose tail a surface draws.
    pub fn is_shell(&self) -> bool {
        self.task_type.as_deref() == Some(SHELL_TYPE)
    }

    /// What every surface says of the task: `running`, `stopping`, or how it
    /// ended: `done` (`completed`), `interrupted` (`killed`, `stopped`),
    /// `failed`, any other outcome `ended`. A task the CLI's rows dropped with
    /// no terminal line is `ended` too. The first terminal status decides, as
    /// the entry keeps it.
    pub fn state_word(&self) -> &'static str {
        match self.outcome.as_deref() {
            Some("completed") => "done",
            Some("killed" | "stopped") => "interrupted",
            Some("failed") => "failed",
            Some(_) => "ended",
            None if !self.live => "ended",
            None if self.stopping => "stopping",
            None => "running",
        }
    }

    /// The word the control read gives for the task's kind.
    pub fn kind(&self) -> &str {
        match self.task_type.as_deref() {
            Some("local_bash") => "shell",
            Some("local_agent") => "agent",
            Some(other) => other,
            None => "",
        }
    }
}

/// What an answer to a read did.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ReadOutcome {
    /// A read of this task was already pending in the worker: nothing changed.
    Busy,
    /// The entry changed, or the answer was one a frame should show.
    Changed,
}

/// Every background task of the current connection, in start order.
#[derive(Default)]
pub struct BackgroundRegistry {
    entries: Vec<BackgroundEntry>,
    /// When each ended task was first seen ended.
    ended_at: HashMap<String, Instant>,
    /// The ended shells whose output has been read through its trailer.
    trailer_read: HashSet<String>,
    /// The tasks with a read in flight.
    in_flight: HashSet<String>,
    /// The tasks a frame painted the tail of since the last tick. Surfaces add
    /// to it (`paint`); the poll is its only reader.
    painted: RefCell<HashSet<String>>,
    /// The CLI refused `get_task_output`: no more reads, and no tail shown.
    output_retired: bool,
}

impl BackgroundRegistry {
    pub fn entries(&self) -> &[BackgroundEntry] {
        &self.entries
    }

    pub fn entry_for_tool_use(&self, tool_use_id: &str) -> Option<&BackgroundEntry> {
        self.entries
            .iter()
            .find(|entry| entry.tool_use_id.as_deref() == Some(tool_use_id))
    }

    /// Whether the CLI refused to read output this session.
    pub fn output_retired(&self) -> bool {
        self.output_retired
    }

    /// A frame painted this task's tail.
    pub fn paint(&self, task_id: &str) {
        self.painted.borrow_mut().insert(task_id.to_string());
    }

    fn entry_mut_or_new(&mut self, task_id: &str) -> &mut BackgroundEntry {
        let at = match self.entries.iter().position(|entry| entry.task_id == task_id) {
            Some(at) => at,
            None => {
                self.entries.push(BackgroundEntry::new(task_id));
                self.entries.len() - 1
            }
        };
        &mut self.entries[at]
    }

    /// Takes in one of the CLI's lines. Whether anything a surface shows changed.
    pub fn apply(&mut self, event: BackgroundTaskEvent, now: Instant) -> bool {
        let before = self.entries.clone();
        match event {
            // The whole list of live tasks: a task in it is live, one that is
            // not has left, whichever of its terminal lines came first.
            BackgroundTaskEvent::Rows(rows) => {
                for entry in &mut self.entries {
                    let held = rows.iter().any(|row| row.task_id == entry.task_id);
                    entry.live = held;
                    if !held {
                        entry.stopping = false;
                    }
                }
                for row in rows {
                    let entry = self.entry_mut_or_new(&row.task_id);
                    entry.live = true;
                    if row.task_type.is_some() {
                        entry.task_type = row.task_type;
                    }
                    if !row.description.is_empty() {
                        entry.description = row.description;
                    }
                }
            }
            BackgroundTaskEvent::Started {
                task_id,
                tool_use_id,
                task_type,
                description,
            } => {
                let entry = self.entry_mut_or_new(&task_id);
                if tool_use_id.is_some() {
                    entry.tool_use_id = tool_use_id;
                }
                if task_type.is_some() {
                    entry.task_type = task_type;
                }
                if !description.is_empty() {
                    entry.description = description;
                }
            }
            // A task the registry never saw start is not invented from its end.
            BackgroundTaskEvent::Ended { task_id, status } => {
                if let Some(entry) = self
                    .entries
                    .iter_mut()
                    .find(|entry| entry.task_id == task_id)
                {
                    // The first terminal status is the outcome: a stop says
                    // `killed` and then `stopped`.
                    entry.outcome.get_or_insert(status);
                    entry.stopping = false;
                }
            }
        }
        for entry in &self.entries {
            if entry.ended() {
                self.ended_at.entry(entry.task_id.clone()).or_insert(now);
            } else {
                // Held by the rows again: its real end starts a window of its own.
                self.ended_at.remove(&entry.task_id);
                self.trailer_read.remove(&entry.task_id);
            }
        }
        if self.output_retired {
            for entry in &mut self.entries {
                entry.tail.clear();
                entry.tail_note = None;
            }
        }
        self.entries != before
    }

    /// Drops the ended entries, with what the poll kept for them: the next
    /// prompt does this, and a task that left is not shown again.
    pub fn drop_ended(&mut self) -> bool {
        let before = self.entries.len();
        let dropped: Vec<String> = self
            .entries
            .iter()
            .filter(|entry| entry.ended())
            .map(|entry| entry.task_id.clone())
            .collect();
        self.entries.retain(|entry| !entry.ended());
        for task_id in &dropped {
            self.ended_at.remove(task_id);
            self.trailer_read.remove(task_id);
            self.painted.borrow_mut().remove(task_id);
        }
        self.entries.len() != before
    }

    /// Forgets everything: the connection the entries belonged to is gone.
    /// Whether anything a surface shows changed.
    pub fn clear(&mut self) -> bool {
        let changed = !self.entries.is_empty() || self.output_retired;
        *self = Self::default();
        changed
    }

    /// Marks a task as being stopped, once. False when there is nothing to
    /// stop: an unknown or ended task, or one already being stopped.
    pub fn begin_stop(&mut self, task_id: &str) -> bool {
        match self
            .entries
            .iter_mut()
            .find(|entry| entry.task_id == task_id)
        {
            Some(entry) if !entry.ended() && !entry.stopping => {
                entry.stopping = true;
                true
            }
            _ => false,
        }
    }

    /// Whether a task is one the poll reads on a tick: a shell or a Monitor
    /// that is live, or one that ended a moment ago and whose trailer has not
    /// been read yet.
    fn is_candidate(&self, entry: &BackgroundEntry, now: Instant) -> bool {
        if self.output_retired || !entry.is_shell() {
            return false;
        }
        if !entry.ended() {
            return true;
        }
        let in_window = self
            .ended_at
            .get(&entry.task_id)
            .is_some_and(|at| now.saturating_duration_since(*at) <= ENDED_READ_WINDOW);
        in_window && !self.trailer_read.contains(&entry.task_id)
    }

    /// Whether any task could still be read: while none can, the poll stops.
    pub fn has_candidate(&self, now: Instant) -> bool {
        self.entries
            .iter()
            .any(|entry| self.is_candidate(entry, now))
    }

    /// One tick: the tasks to read now. A candidate is read only when a frame
    /// painted its tail since the last tick; one whose read is still in flight
    /// is not read again, and keeps its mark so that a slow answer does not
    /// end the polling.
    pub fn take_reads(&mut self, now: Instant) -> Vec<String> {
        let painted = std::mem::take(&mut *self.painted.borrow_mut());
        let mut reads = Vec::new();
        let mut kept = Vec::new();
        for entry in &self.entries {
            if !self.is_candidate(entry, now) || !painted.contains(&entry.task_id) {
                continue;
            }
            if self.in_flight.contains(&entry.task_id) {
                kept.push(entry.task_id.clone());
            } else {
                reads.push(entry.task_id.clone());
            }
        }
        self.in_flight.extend(reads.iter().cloned());
        self.painted.borrow_mut().extend(kept);
        reads
    }

    /// An answer to a read of `task_id`.
    pub(super) fn finish_read(
        &mut self,
        task_id: &str,
        answer: Result<TaskOutput, String>,
    ) -> ReadOutcome {
        self.in_flight.remove(task_id);
        match answer {
            // The worker had a read of this task pending already, and wrote
            // nothing: the tail and its note stay as they were.
            // The tick spent the task's painted mark on this read, so it goes
            // back: no frame may paint again before the CLI answers.
            Err(error) if error == "busy" => {
                self.painted.borrow_mut().insert(task_id.to_string());
                ReadOutcome::Busy
            }
            Err(error) if sirio_chat::is_refusal(&error) => {
                self.output_retired = true;
                for entry in &mut self.entries {
                    entry.tail.clear();
                    entry.tail_note = None;
                }
                ReadOutcome::Changed
            }
            Err(error) => {
                if let Some(entry) = self
                    .entries
                    .iter_mut()
                    .find(|entry| entry.task_id == task_id)
                {
                    let reason = error.lines().next().unwrap_or_default();
                    entry.tail_note = Some(format!("output unavailable: {reason}"));
                }
                ReadOutcome::Changed
            }
            Ok(output) => {
                if self.output_retired {
                    return ReadOutcome::Changed;
                }
                let tail = tail_lines(&output.output, output.truncated, TAIL_KEEP);
                if let Some(entry) = self
                    .entries
                    .iter_mut()
                    .find(|entry| entry.task_id == task_id)
                {
                    entry.tail_note = None;
                    // A tail that has something is never replaced by nothing.
                    if !tail.is_empty() || entry.tail.is_empty() {
                        entry.tail = tail;
                    }
                    let trailer = entry.tail.last().is_some_and(|line| is_trailer(line));
                    if entry.ended() && trailer {
                        self.trailer_read.insert(task_id.to_string());
                    }
                }
                ReadOutcome::Changed
            }
        }
    }
}

/// The CLI ends a shell's output with one of these once it ended.
fn is_trailer(line: &str) -> bool {
    line == "[killed]" || (line.starts_with("[exited with code ") && line.ends_with(']'))
}

/// A tail as lines for a surface to draw: the CLI's last 8 KiB of a task's
/// output with what a terminal would have done to it already done.
///
/// 1. `\r\n` is `\n`.
/// 2. ANSI CSI and OSC sequences go.
/// 3. A line is what its last non-empty piece between `\r`s says (`50%\r`
///    reads `50%`), the way a terminal overwrites.
/// 4. Leading U+FFFDs go: the CLI cuts its 8 KiB window by bytes, which can
///    split a character.
/// 5. The first line goes when the answer is `truncated` and another line
///    follows, because the window can start inside it.
/// 6. Trailing empty lines go.
/// 7. The last `keep` lines come back, trailers (`[killed]`,
///    `[exited with code N]`) included.
pub fn tail_lines(output: &str, truncated: bool, keep: usize) -> Vec<String> {
    let text = output.replace("\r\n", "\n");
    let text = strip_ansi(&text);
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|line| {
            line.split('\r')
                .rfind(|piece| !piece.is_empty())
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    // A cut inside a 3- or 4-byte character leaves one to three of them.
    if let Some(first) = lines.first_mut() {
        *first = first.trim_start_matches('\u{FFFD}').to_string();
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    if truncated && lines.len() > 1 {
        lines.remove(0);
    }
    let skip = lines.len().saturating_sub(keep);
    lines.split_off(skip)
}

/// `text` without its ANSI escape sequences: CSI (`ESC [ ... final`), OSC
/// (`ESC ] ... BEL` or `ESC \`), charset designations and the two-character ones.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' {
                        if chars.peek() == Some(&'\\') {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            // Charset designation (`ESC ( B`, from `tput sgr0`): three bytes.
            Some('(' | ')' | '*' | '+' | '#' | '%') => {
                chars.next();
            }
            _ => {}
        }
    }
    out
}

/// What a chat needs of a connection's task calls. `TaskControl` is the one in
/// production; a test's is a fake.
pub(super) trait TaskSource: Send + Sync {
    fn output(&self, task_id: &str) -> Result<TaskOutput, String>;
    fn stop(&self, task_id: &str);
}

impl TaskSource for sirio_chat::TaskControl {
    fn output(&self, task_id: &str) -> Result<TaskOutput, String> {
        sirio_chat::TaskControl::output(self, task_id)
    }

    fn stop(&self, task_id: &str) {
        sirio_chat::TaskControl::stop(self, task_id);
    }
}

impl Chat {
    /// Every background task of this session, in start order.
    pub fn background_entries(&self) -> &[BackgroundEntry] {
        self.background.entries()
    }

    /// The registry as `surface.chat.read` gives it.
    pub(super) fn background_tasks_json(&self) -> String {
        let tasks: Vec<_> = self
            .background
            .entries()
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "taskId": entry.task_id,
                    "toolUseId": entry.tool_use_id,
                    "kind": entry.kind(),
                    "description": entry.description,
                    "live": entry.live,
                    "outcome": entry.outcome,
                    "stopping": entry.stopping,
                    "tail": entry.tail,
                    "tailNote": entry.tail_note,
                })
            })
            .collect();
        serde_json::to_string(&tasks).expect("the background tasks are serializable")
    }

    /// The background task a tool call started.
    pub fn entry_for_tool_use(&self, tool_use_id: &str) -> Option<&BackgroundEntry> {
        self.background.entry_for_tool_use(tool_use_id)
    }

    /// Whether the CLI refused to read task output: no tail is drawn then.
    pub fn task_output_retired(&self) -> bool {
        self.background.output_retired()
    }

    /// A surface calls this while it paints `task_id`'s tail, every frame: the
    /// poll reads the output of the tasks painted since its last tick and of no
    /// other.
    pub fn paint_tail(&self, task_id: &str) {
        self.background.paint(task_id);
    }

    /// Stop on a background task (R7): fire and forget. The task's own lines
    /// end it; until they do the entry reads `stopping` and a second press
    /// sends nothing.
    pub fn stop_background_task(&mut self, task_id: &str, cx: &mut Context<Self>) {
        let Some(source) = self.task_source() else {
            return;
        };
        if !self.background.begin_stop(task_id) {
            return;
        }
        source.stop(task_id);
        self.background_changed(cx);
    }

    /// The connection's calls on tasks, `None` without one (an ACP agent, or
    /// no connection).
    fn task_source(&self) -> Option<std::sync::Arc<dyn TaskSource>> {
        #[cfg(test)]
        if let Some(source) = &self.task_source_override {
            return Some(source.clone());
        }
        let control = self.client.as_ref()?.task_control()?;
        Some(std::sync::Arc::new(control))
    }

    /// The tasks still running, in start order: what the composer's popover lists.
    pub fn live_background_tasks(&self) -> impl Iterator<Item = &BackgroundEntry> {
        self.background.entries().iter().filter(|entry| !entry.ended())
    }

    /// The registry changed under a surface: it repaints, and the Trace tab
    /// hears of it. The popover lists live tasks, so it goes with the last.
    fn background_changed(&mut self, cx: &mut Context<Self>) {
        if self.live_background_tasks().next().is_none() {
            self.close_background_popover();
        }
        cx.emit(ChatEvent::BackgroundChanged);
        cx.notify();
    }

    /// One of the CLI's background-task lines. A line the worker read before
    /// its client was taken describes a CLI that is gone, and nothing would
    /// clear it again.
    pub(super) fn apply_background_event(
        &mut self,
        event: BackgroundTaskEvent,
        cx: &mut Context<Self>,
    ) {
        if self.client.is_none() {
            return;
        }
        if self.background.apply(event, Instant::now()) {
            self.background_changed(cx);
        }
        self.ensure_background_poll(cx);
    }

    /// The next prompt drops the tasks that ended.
    pub(super) fn drop_ended_background_tasks(&mut self, cx: &mut Context<Self>) {
        if self.background.drop_ended() {
            self.background_changed(cx);
        }
    }

    /// The connection the registry belonged to ended (R6): a transport error, a
    /// timeout, a failed send, a closed event source or a new connection. The
    /// tasks' processes are not ended by this; Sirio just stops showing them.
    pub(super) fn reset_background_tasks(&mut self, cx: &mut Context<Self>) {
        self.background_poll = None;
        self.background_poll_active = false;
        if self.background.clear() {
            self.background_changed(cx);
        }
    }

    /// Starts the poll when a task could be read and no loop is running.
    fn ensure_background_poll(&mut self, cx: &mut Context<Self>) {
        if self.background_poll_active || !self.background.has_candidate(Instant::now()) {
            return;
        }
        self.background_poll_active = true;
        let generation = self.connection_generation;
        self.background_poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let Ok(keep_going) = this.update(cx, |chat, cx| chat.background_tick(generation, cx))
                else {
                    return;
                };
                if !keep_going {
                    return;
                }
            }
        }));
    }

    /// One tick of the poll: starts a read for each task a frame painted, and
    /// says whether the loop goes on. It never waits for an answer.
    pub(super) fn background_tick(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if generation != self.connection_generation || self.client.is_none() {
            self.background_poll_active = false;
            return false;
        }
        let now = Instant::now();
        if let Some(source) = self.task_source() {
            for task_id in self.background.take_reads(now) {
                let source = source.clone();
                let id = task_id.clone();
                cx.spawn(async move |this, cx| {
                    let answer = cx
                        .background_executor()
                        .spawn(async move { source.output(&id) })
                        .await;
                    let _ = this.update(cx, |chat, cx| {
                        chat.background_answer(&task_id, generation, answer, cx);
                    });
                })
                .detach();
            }
        }
        if self.background.has_candidate(now) {
            true
        } else {
            self.background_poll_active = false;
            false
        }
    }

    /// An answer to a read. One from an earlier connection is dropped, and so
    /// is one that arrives with no client: it describes a CLI that is gone.
    pub(super) fn background_answer(
        &mut self,
        task_id: &str,
        generation: u64,
        answer: Result<TaskOutput, String>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.connection_generation || self.client.is_none() {
            return;
        }
        if self.background.finish_read(task_id, answer) == ReadOutcome::Busy {
            return;
        }
        self.background_changed(cx);
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    // Failure list for `tail_lines`, written before the function. It is wrong if:
    //  1. a colour code stays in a line (cur-small's RED-START);
    //  2. a `\r` progress line reads its first or its joined pieces, not its last (`progress 100%`);
    //  3. a last line with no newline is lost or glued to the one before (cur-small);
    //  4. a truncated answer keeps the fragment its window starts inside (cur-special);
    //  5. a truncated answer of one line loses it, or keeps its U+FFFD (cur-utf8);
    //  6. `\r\n` leaves an empty piece that blanks the line, or `50%\r` reads empty;
    //  7. trailing empty lines stay, or interior blank lines go;
    //  8. a trailer is dropped by the trailing-empty rule, or `keep` is off by one;
    //  9. an OSC title (ended by BEL or by ESC \) leaks into a line.

    fn lines(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn small_output_loses_its_colour_and_keeps_the_last_progress_piece() {
        // run-cur-small, verbatim.
        let output = "\u{1b}[31mRED-START\u{1b}[0m\nprogress 10%\rprogress 50%\rprogress 100%\nno-trailing-newline-tail";
        assert_eq!(
            tail_lines(output, false, 10),
            lines(&["RED-START", "progress 100%", "no-trailing-newline-tail"])
        );
    }

    #[test]
    fn special_output_drops_the_fragment_its_window_starts_inside() {
        // run-cur-special: the last 8 KiB starts inside a line of `x`.
        let mut output = String::from("xxxxxxx\n");
        for n in 345..600 {
            output.push_str(&format!("L{n:04}-éééééééé-xxxxxxxx\n"));
        }
        output.push_str("no-trailing-newline-tail");
        let tail = tail_lines(&output, true, 1000);
        assert_eq!(tail.first().unwrap(), "L0345-éééééééé-xxxxxxxx");
        assert_eq!(tail.last().unwrap(), "no-trailing-newline-tail");
        assert_eq!(tail.len(), 255 + 1);
        assert_eq!(
            tail_lines(&output, true, 2),
            lines(&["L0599-éééééééé-xxxxxxxx", "no-trailing-newline-tail"])
        );
    }

    #[test]
    fn a_truncated_single_line_is_kept_without_its_split_character() {
        // run-cur-utf8, as the probe got it: `a`, 5000 x `é`, `z` cut by bytes
        // into 8194 bytes, one line that starts with U+FFFD, 4095 `é` and `z`.
        let output = format!("\u{FFFD}{}z", "é".repeat(4095));
        assert_eq!(output.len(), 8194);
        let tail = tail_lines(&output, true, 10);
        assert_eq!(tail, vec![format!("{}z", "é".repeat(4095))]);
    }

    #[test]
    fn a_cut_inside_a_longer_character_leaves_no_replacement_characters() {
        let output = "\u{FFFD}\u{FFFD}ab\ncd";
        assert_eq!(tail_lines(output, false, 10), lines(&["ab", "cd"]));
    }

    #[test]
    fn special_output_after_a_stop_starts_mid_line_and_ends_with_its_trailer() {
        // run-cur-special's r4 answer (8192 bytes): the window slid 10 bytes
        // and starts at `345-...`, and the CLI added `[killed]`.
        let mut output = String::from("345-éééééééé-xxxxxxxx\n");
        for n in 346..600 {
            output.push_str(&format!("L{n:04}-éééééééé-xxxxxxxx\n"));
        }
        output.push_str("no-trailing-newline-tail\n[killed]\n");
        assert_eq!(output.len(), 8192);
        let tail = tail_lines(&output, true, 1000);
        assert_eq!(tail.first().unwrap(), "L0346-éééééééé-xxxxxxxx");
        assert_eq!(tail.len(), 254 + 2);
        assert_eq!(
            tail_lines(&output, true, 3),
            lines(&["L0599-éééééééé-xxxxxxxx", "no-trailing-newline-tail", "[killed]"])
        );
    }

    #[test]
    fn tputs_charset_reset_leaves_no_stray_letter() {
        // `tput sgr0` writes `ESC ( B` then `ESC [ m`.
        assert_eq!(
            tail_lines("\u{1b}[1mbold\u{1b}(B\u{1b}[m done", false, 10),
            lines(&["bold done"])
        );
    }

    #[test]
    fn crlf_and_a_line_ending_in_a_return_read_as_their_text() {
        assert_eq!(
            tail_lines("one\r\ntwo\r\n50%\r", false, 10),
            lines(&["one", "two", "50%"])
        );
    }

    #[test]
    fn trailing_blank_lines_go_and_interior_ones_stay() {
        assert_eq!(
            tail_lines("a\n\nb\n\n\n", false, 10),
            lines(&["a", "", "b"])
        );
    }

    #[test]
    fn the_trailers_are_kept_and_keep_counts_from_the_end() {
        let killed = "line 1\nline 2\n\n[killed]\n";
        assert_eq!(tail_lines(killed, false, 2), lines(&["", "[killed]"]));
        assert_eq!(
            tail_lines(killed, false, 4),
            lines(&["line 1", "line 2", "", "[killed]"])
        );
        let exited = "quick 1\nquick 2\n\n[exited with code 0]\n";
        assert_eq!(
            tail_lines(exited, false, 1),
            lines(&["[exited with code 0]"])
        );
    }

    #[test]
    fn a_title_sequence_does_not_leak_into_a_line() {
        let output = "\u{1b}]0;title\u{7}one\n\u{1b}]8;;http://x\u{1b}\\two\u{1b}[2K\u{1b}[1;32mgo\u{1b}[0m";
        assert_eq!(tail_lines(output, false, 10), lines(&["one", "twogo"]));
    }

    // Failure list for the registry's life on a connection (R5, R6), written before the code.
    // The chat is wrong if:
    //  1. a read started on a connection answers into the registry of the next one (a
    //     reconnect, a transport error), so a task of the old CLI shows another one's tail;
    //  2. a reset leaves the loop alive, or an entry behind;
    //  3. the answer of a connection that is gone, with no client, is applied;
    //  4. a tick starts a second read for a task whose read has not answered, or throws its
    //     painted mark away while it is pending, so a slow answer ends the polling;
    //  5. an answer does not repaint (success, error, an empty one), or `busy` does;
    //  6. an empty answer replaces a tail that has lines, or an error erases it.

    use std::sync::{Arc, Mutex};

    use gpui::TestAppContext;

    use super::super::tests::{pump_chat_until, silent_state_chat};
    use crate::chat::{Chat, ChatEvent};

    /// The connection's task calls, answered at once and counted.
    pub(in crate::chat) struct Fake {
        reads: Mutex<Vec<String>>,
        stops: Mutex<Vec<String>>,
        answer: Mutex<Result<TaskOutput, String>>,
    }

    impl Fake {
        pub(in crate::chat) fn answering(output: &str) -> Arc<Self> {
            Arc::new(Self {
                reads: Mutex::new(Vec::new()),
                stops: Mutex::new(Vec::new()),
                answer: Mutex::new(Ok(TaskOutput {
                    output: output.to_string(),
                    total_bytes: output.len() as u64,
                    truncated: false,
                })),
            })
        }

        pub(in crate::chat) fn reads(&self) -> usize {
            self.reads.lock().unwrap().len()
        }

        /// The tasks Stop was sent for, in order.
        pub(in crate::chat) fn stops(&self) -> Vec<String> {
            self.stops.lock().unwrap().clone()
        }
    }

    impl TaskSource for Fake {
        fn output(&self, task_id: &str) -> Result<TaskOutput, String> {
            self.reads.lock().unwrap().push(task_id.to_string());
            self.answer.lock().unwrap().clone()
        }

        fn stop(&self, task_id: &str) {
            self.stops.lock().unwrap().push(task_id.to_string());
        }
    }

    fn rows(task_id: &str) -> sirio_chat::ChatEvent {
        sirio_chat::ChatEvent::BackgroundTask(BackgroundTaskEvent::Rows(vec![
            sirio_chat::BackgroundTaskRow {
                task_id: task_id.to_string(),
                task_type: Some("local_bash".to_string()),
                description: "shell40".to_string(),
            },
        ]))
    }

    fn ok(output: &str) -> Result<TaskOutput, String> {
        Ok(TaskOutput {
            output: output.to_string(),
            total_bytes: output.len() as u64,
            truncated: false,
        })
    }

    fn tail_of(chat: &Chat, task_id: &str) -> Vec<String> {
        chat.background_entries()
            .iter()
            .find(|entry| entry.task_id == task_id)
            .map(|entry| entry.tail.clone())
            .unwrap_or_default()
    }

    fn count_changes(
        chat: &gpui::Entity<Chat>,
        cx: &mut gpui::VisualTestContext,
    ) -> std::rc::Rc<std::cell::Cell<usize>> {
        let said = std::rc::Rc::new(std::cell::Cell::new(0usize));
        cx.update(|_, cx| {
            let said = said.clone();
            cx.subscribe(chat, move |_, event: &ChatEvent, _| {
                if matches!(event, ChatEvent::BackgroundChanged) {
                    said.set(said.get() + 1);
                }
            })
            .detach();
        });
        said
    }

    #[gpui::test]
    async fn a_reconnect_empties_the_registry_and_stops_reading(cx: &mut TestAppContext) {
        let (chat, cx) = silent_state_chat(cx);
        let fake = Fake::answering("late\n");
        let old_generation = chat.update(cx, |chat, cx| {
            chat.task_source_override = Some(fake.clone());
            chat.handle_event(rows("bshell001"), cx);
            chat.paint_tail("bshell001");
            let generation = chat.connection_generation;
            assert!(chat.background_tick(generation, cx), "a painted live shell is read");
            generation
        });

        // A new connection: the registry empties and the loop ends at once.
        chat.update(cx, |chat, cx| chat.start_connection(cx));
        chat.read_with(cx, |chat, _| {
            assert!(chat.background_entries().is_empty());
            assert!(chat.background_poll.is_none() && !chat.background_poll_active);
            assert!(chat.connection_generation > old_generation);
        });
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());

        // The new CLI has a task of the same id. The old connection's answer
        // arrives late and is not its tail; the new connection's own is.
        chat.update(cx, |chat, cx| {
            chat.handle_event(rows("bshell001"), cx);
            chat.background_answer("bshell001", old_generation, ok("stale\n"), cx);
        });
        chat.read_with(cx, |chat, _| assert!(tail_of(chat, "bshell001").is_empty()));
        chat.update(cx, |chat, cx| {
            let generation = chat.connection_generation;
            chat.background_answer("bshell001", generation, ok("fresh\n"), cx);
        });
        chat.read_with(cx, |chat, _| {
            assert_eq!(tail_of(chat, "bshell001"), vec!["fresh".to_string()]);
        });
    }

    #[gpui::test]
    async fn a_transport_error_empties_the_registry_and_drops_the_late_answer(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = silent_state_chat(cx);
        let fake = Fake::answering("late\n");
        chat.update(cx, |chat, cx| {
            chat.task_source_override = Some(fake.clone());
            chat.handle_event(rows("bshell001"), cx);
            chat.paint_tail("bshell001");
            // The late answer is a refusal: wrongly applied, it would retire the tail.
            *fake.answer.lock().unwrap() =
                Err("Unsupported control request subtype: get_task_output".to_string());
            let generation = chat.connection_generation;
            assert!(chat.background_tick(generation, cx));
            // The read is in flight when the CLI dies.
            chat.handle_event(sirio_chat::ChatEvent::TransportError("gone".into()), cx);
            assert!(chat.background_entries().is_empty());
            assert!(chat.background_poll.is_none() && !chat.background_poll_active);
            assert!(!chat.background_tick(generation, cx), "a loop of the dead CLI ends");
        });
        cx.run_until_parked();
        chat.read_with(cx, |chat, _| {
            assert!(chat.background_entries().is_empty());
            assert!(!chat.task_output_retired(), "the late refusal was dropped");
        });
        // A line the dead worker still delivers describes nothing.
        chat.update(cx, |chat, cx| chat.handle_event(rows("bshell002"), cx));
        chat.read_with(cx, |chat, _| assert!(chat.background_entries().is_empty()));
    }

    #[gpui::test]
    async fn a_slow_answer_keeps_the_task_painted_and_repaints(cx: &mut TestAppContext) {
        let (chat, cx) = silent_state_chat(cx);
        let said = count_changes(&chat, cx);
        let fake = Fake::answering("\u{1b}[32mline 1\u{1b}[0m\n");
        let generation = chat.update(cx, |chat, cx| {
            chat.task_source_override = Some(fake.clone());
            chat.handle_event(rows("bshell001"), cx);
            chat.paint_tail("bshell001");
            let generation = chat.connection_generation;
            assert!(chat.background_tick(generation, cx));
            // The answer has not come: nothing runs until the executor is pumped.
            // The next tick finds the task painted again and still pending.
            chat.paint_tail("bshell001");
            assert!(chat.background_tick(generation, cx));
            generation
        });
        let before = said.get();
        cx.run_until_parked();
        assert_eq!(fake.reads(), 1, "a pending read is not asked for twice");
        chat.read_with(cx, |chat, _| {
            assert_eq!(tail_of(chat, "bshell001"), vec!["line 1".to_string()]);
        });
        assert!(said.get() > before, "the answer repaints");

        // The second tick found the read pending and kept the task's mark: the
        // tick after the answer serves it, with no new frame in between.
        chat.update(cx, |chat, cx| assert!(chat.background_tick(generation, cx)));
        cx.run_until_parked();
        assert_eq!(fake.reads(), 2, "the mark kept through the pending tick is served next");

        // That mark is spent: a tick with no frame since reads nothing, and the
        // loop goes on while the shell lives.
        chat.update(cx, |chat, cx| assert!(chat.background_tick(generation, cx)));
        cx.run_until_parked();
        assert_eq!(fake.reads(), 2, "an unpainted tick reads nothing");
    }

    #[gpui::test]
    async fn busy_after_a_timeout_keeps_the_polling_going(cx: &mut TestAppContext) {
        let (chat, cx) = silent_state_chat(cx);
        let fake = Fake::answering("");
        *fake.answer.lock().unwrap() = Err("get_task_output timed out after 5s".to_string());
        let generation = chat.update(cx, |chat, cx| {
            chat.task_source_override = Some(fake.clone());
            chat.handle_event(rows("bshell001"), cx);
            chat.paint_tail("bshell001");
            let generation = chat.connection_generation;
            assert!(chat.background_tick(generation, cx));
            generation
        });
        cx.run_until_parked();
        assert_eq!(fake.reads(), 1);
        chat.read_with(cx, |chat, _| {
            assert_eq!(
                chat.background_entries()[0].tail_note.as_deref(),
                Some("output unavailable: get_task_output timed out after 5s")
            );
        });

        // A frame paints, the next tick reads, and the worker, still holding
        // the timed-out read, says busy.
        *fake.answer.lock().unwrap() = Err("busy".to_string());
        chat.update(cx, |chat, cx| {
            chat.paint_tail("bshell001");
            assert!(chat.background_tick(generation, cx));
        });
        cx.run_until_parked();
        assert_eq!(fake.reads(), 2);

        // The CLI recovers. No frame has painted since: the mark must have
        // survived the busy tick for this one to read.
        *fake.answer.lock().unwrap() = ok("one\ntwo\n");
        chat.update(cx, |chat, cx| assert!(chat.background_tick(generation, cx)));
        cx.run_until_parked();
        assert_eq!(fake.reads(), 3, "busy is a skipped tick, not the end of the polling");
        chat.read_with(cx, |chat, _| {
            let entry = &chat.background_entries()[0];
            assert_eq!(entry.tail, vec!["one".to_string(), "two".to_string()]);
            assert_eq!(entry.tail_note, None);
        });
    }

    #[gpui::test]
    async fn every_answer_but_busy_repaints_and_an_empty_one_keeps_the_tail(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = silent_state_chat(cx);
        let said = count_changes(&chat, cx);
        let generation = chat.update(cx, |chat, cx| {
            chat.handle_event(rows("bshell001"), cx);
            chat.connection_generation
        });
        let answer = |cx: &mut gpui::VisualTestContext, answer: Result<TaskOutput, String>| {
            let before = said.get();
            chat.update(cx, |chat, cx| {
                chat.background_answer("bshell001", generation, answer, cx);
            });
            said.get() > before
        };
        assert!(answer(cx, ok("one\ntwo\n")));
        assert!(!answer(cx, Err("busy".to_string())), "busy repaints nothing");
        assert!(answer(cx, Err("get_task_output: broken\nsecond line".to_string())));
        chat.read_with(cx, |chat, _| {
            let entry = &chat.background_entries()[0];
            assert_eq!(entry.tail, vec!["one".to_string(), "two".to_string()]);
            assert_eq!(
                entry.tail_note.as_deref(),
                Some("output unavailable: get_task_output: broken")
            );
        });
        assert!(!answer(cx, Err("busy".to_string())));
        chat.read_with(cx, |chat, _| {
            assert!(
                chat.background_entries()[0].tail_note.is_some(),
                "busy leaves the note as it was"
            );
        });
        assert!(answer(cx, ok("")));
        chat.read_with(cx, |chat, _| {
            let entry = &chat.background_entries()[0];
            assert_eq!(entry.tail, vec!["one".to_string(), "two".to_string()]);
            assert_eq!(entry.tail_note, None);
        });
        assert!(answer(
            cx,
            Err("Unsupported control request subtype: get_task_output".to_string())
        ));
        chat.read_with(cx, |chat, _| {
            assert!(chat.task_output_retired());
            assert!(chat.background_entries()[0].tail.is_empty());
        });
    }
}
