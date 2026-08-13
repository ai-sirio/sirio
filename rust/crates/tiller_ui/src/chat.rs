//! The chat transcript and composer, driven by real ACP events.
//!
//! This first Rust implementation owns a linear transcript and one live ACP
//! session, same scope as `Sidebar`'s fixture model: the connection lifecycle
//! and view model live here so the transcript renderer stays deterministic.

use gpui::{
    AnyElement, App, BorderStyle, Bounds, ClipboardItem, Context, CursorStyle, DispatchPhase,
    Edges, Element, ElementId, Entity, FocusHandle, Focusable, FollowMode, FontStyle, FontWeight,
    GlobalElementId, HighlightStyle, Hitbox, HitboxBehavior, InspectorElementId, InteractiveText,
    KeyBinding, KeyDownEvent, LayoutId, ListAlignment, ListSizingBehavior, ListState, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder, Pixels, Rgba, SharedString,
    StyledText, Task, UnderlineStyle, Window, actions, canvas, div, list, point, prelude::*, px,
    quad, rgb, transparent_black,
};
use std::cell::Cell;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use tiller_acp::{
    AcpClient, AcpEvent, AgentCommand, ContextUsage, ModelCatalog, ModelOption, PermissionOption,
};
use tiller_markdown::{Alignment, Block, Document, Inline, ListItem, ListKind, parse};
use tiller_theme::Theme;

/// The transcript's content column — waku's measured `CONTENT_MAX_WIDTH`
/// 720 (`docs/linux-rewrite/03-visual-bar-and-gpui-patterns.md` §A.2).
pub(crate) const TRANSCRIPT_WIDTH: f32 = 720.0;
pub(crate) const CARD_H_PADDING: f32 = 14.0;
pub(crate) const CARD_V_PADDING: f32 = 10.0;

/// The user turn's pill: rounded, right-aligned, capped at waku's bubble
/// width. The assistant reply has no container at all.
pub(crate) const USER_PILL_MAX_WIDTH: f32 = 540.0;

actions!(
    chat_composer,
    [
        Send,
        Newline,
        Cancel,
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        CopyTranscript,
        Home,
        End,
    ]
);

/// One rendered element of the transcript.
#[derive(Clone, Debug)]
enum Entry {
    /// The user's own turn, shown as a full-width card.
    User(String),
    /// A streamed assistant reply, grown in place as chunks arrive.
    ///
    /// The parsed tree is updated at ingestion time rather than during
    /// rendering, so a redraw never reparses the entire reply.
    Assistant { text: String, document: Document },
    /// A streamed reasoning chunk, visually distinct from the reply.
    Thought(String),
    /// A tool call, tracked by protocol id so later updates can patch it.
    ToolCall {
        id: String,
        title: String,
        status: String,
    },
    /// An inline permission prompt; `resolved` is filled in once answered.
    Permission {
        request_id: u64,
        options: Vec<PermissionOption>,
        resolved: Option<String>,
    },
    /// The muted timestamp/rule footer closing out one completed turn.
    TurnFooter(String),
    /// A transport failure, surfaced instead of silently doing nothing.
    Error {
        message: String,
        retryable: bool,
        kind: ErrorKind,
    },
}

impl Entry {
    fn plain_text(&self) -> String {
        match self {
            Self::User(text) | Self::Thought(text) => text.clone(),
            Self::Assistant { document, .. } => document.plain_text(),
            Self::ToolCall { title, status, .. } => format!("{title}\n{status}"),
            Self::Permission {
                options, resolved, ..
            } => resolved.clone().unwrap_or_else(|| {
                options
                    .iter()
                    .map(|option| option.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" / ")
            }),
            Self::TurnFooter(text) => text.clone(),
            Self::Error { message, .. } => message.clone(),
        }
    }
}

/// Whether an error describes the live connection or belongs permanently in
/// the transcript. Connection errors are removed when a later retry recovers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ErrorKind {
    Connection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TranscriptSelection {
    anchor: usize,
    head: usize,
}

impl TranscriptSelection {
    fn range(&self) -> Range<usize> {
        if self.anchor <= self.head {
            self.anchor..self.head
        } else {
            self.head..self.anchor
        }
    }
}

#[derive(Clone)]
struct TranscriptInteraction {
    chat: Entity<Chat>,
    focus: FocusHandle,
}

/// A styled text run whose selection range is anchored in the complete
/// transcript rather than in the currently mounted virtualized row.
struct TranscriptSelectableText {
    id: ElementId,
    text: StyledText,
    source_range: Range<usize>,
    interaction: TranscriptInteraction,
    selection_fill: Rgba,
    links: Vec<(Range<usize>, String)>,
    pressed_index: Rc<Cell<Option<usize>>>,
}

impl TranscriptSelectableText {
    fn new(
        id: impl Into<ElementId>,
        text: StyledText,
        source_range: Range<usize>,
        interaction: TranscriptInteraction,
        selection_fill: Rgba,
        links: Vec<(Range<usize>, String)>,
    ) -> Self {
        Self {
            id: id.into(),
            text,
            source_range,
            interaction,
            selection_fill,
            links,
            pressed_index: Rc::new(Cell::new(None)),
        }
    }

    fn paint_selection(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let Some(selection) = self
            .interaction
            .chat
            .read(cx)
            .transcript_selection
            .as_ref()
            .map(TranscriptSelection::range)
        else {
            return;
        };
        let start = selection.start.max(self.source_range.start);
        let end = selection.end.min(self.source_range.end);
        if start >= end {
            return;
        }

        let local_start = start - self.source_range.start;
        let local_end = end - self.source_range.start;
        let layout = self.text.layout();
        let line_height = layout.line_height();
        let start_position = layout
            .position_for_index(local_start)
            .unwrap_or(bounds.origin);
        let end_position = layout
            .position_for_index(local_end)
            .unwrap_or(point(bounds.right(), bounds.bottom() - line_height));
        let color = self.selection_fill;
        let mut paint_line = |y: Pixels, left: Pixels, right: Pixels| {
            if right > left {
                window.paint_quad(quad(
                    Bounds::from_corners(
                        point(left, y),
                        point(right, (y + line_height).min(bounds.bottom())),
                    ),
                    px(0.0),
                    color,
                    Edges::default(),
                    transparent_black(),
                    BorderStyle::default(),
                ));
            }
        };

        if start_position.y == end_position.y {
            paint_line(start_position.y, start_position.x, end_position.x);
        } else {
            paint_line(start_position.y, start_position.x, bounds.right());
            let mut y = start_position.y + line_height;
            while y < end_position.y {
                paint_line(y, bounds.left(), bounds.right());
                y += line_height;
            }
            paint_line(end_position.y, bounds.left(), end_position.x);
        }
    }
}

impl Element for TranscriptSelectableText {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.text.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.text
            .prepaint(id, inspector_id, bounds, state, window, cx);
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = self.text.layout().clone();
        self.paint_selection(bounds, window, cx);
        window.set_cursor_style(CursorStyle::IBeam, hitbox);

        let source_range = self.source_range.clone();
        let interaction = self.interaction.clone();
        let pressed_index = self.pressed_index.clone();
        let hitbox_for_down = hitbox.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Left
                && hitbox_for_down.is_hovered(window)
            {
                let index = match layout.index_for_position(event.position) {
                    Ok(index) | Err(index) => index.min(source_range.len()),
                };
                pressed_index.set(Some(index));
                interaction.chat.update(cx, |chat, cx| {
                    let position = source_range.start + index;
                    chat.transcript_selection = Some(TranscriptSelection {
                        anchor: position,
                        head: position,
                    });
                    chat.transcript_dragging = true;
                    cx.notify();
                });
                window.focus(&interaction.focus, cx);
                window.prevent_default();
            }
        });

        let source_range = self.source_range.clone();
        let interaction = self.interaction.clone();
        let hitbox_for_move = hitbox.clone();
        let layout_for_move = self.text.layout().clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.dragging()
                && hitbox_for_move.is_hovered(window)
                && interaction.chat.read(cx).transcript_dragging
            {
                let index = match layout_for_move.index_for_position(event.position) {
                    Ok(index) | Err(index) => index.min(source_range.len()),
                };
                interaction.chat.update(cx, |chat, cx| {
                    if let Some(selection) = &mut chat.transcript_selection {
                        selection.head = source_range.start + index;
                    }
                    cx.notify();
                });
            }
        });

        let source_range = self.source_range.clone();
        let interaction = self.interaction.clone();
        let links = self.links.clone();
        let pressed_index_for_up = self.pressed_index.clone();
        let hitbox_for_up = hitbox.clone();
        let layout_for_up = self.text.layout().clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Left
                && hitbox_for_up.is_hovered(window)
            {
                if let Some(index) = pressed_index_for_up.take() {
                    let up = match layout_for_up.index_for_position(event.position) {
                        Ok(index) | Err(index) => index.min(source_range.len()),
                    };
                    if index == up
                        && let Some((_, target)) =
                            links.iter().find(|(range, _)| range.contains(&index))
                    {
                        cx.open_url(target);
                    }
                }
                interaction.chat.update(cx, |chat, cx| {
                    chat.transcript_dragging = false;
                    cx.notify();
                });
                window.prevent_default();
            }
        });

        self.text
            .paint(id, inspector_id, bounds, state, &mut (), window, cx);
    }
}

impl IntoElement for TranscriptSelectableText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// A chat surface wired to one live [`AcpClient`] session.
pub struct Chat {
    client: Option<AcpClient>,
    agent_command: AgentCommand,
    agent_cwd: PathBuf,
    entries: Vec<Entry>,
    composer_text: String,
    composer_cursor: usize,
    composer_selection_anchor: Option<usize>,
    composer_focus: FocusHandle,
    streaming: bool,
    connecting: bool,
    retry_pending_send: bool,
    has_completed_turn: bool,
    available_models: Vec<ModelOption>,
    model_config_id: Option<String>,
    selected_model: Option<String>,
    model_picker_open: bool,
    context_popover_open: bool,
    model_picker_focus: FocusHandle,
    context_popover_focus: FocusHandle,
    transcript_focus: FocusHandle,
    context_usage: Option<ContextUsage>,
    list_state: ListState,
    transcript_selection: Option<TranscriptSelection>,
    transcript_dragging: bool,
    _event_task: Option<Task<()>>,
}

impl Chat {
    /// Launches a real ACP agent and returns a `Chat` wired to its event
    /// stream. The command defaults to the same `npx` agent used by
    /// `tiller_acp`'s own smoke test, overridable with `TILLER_ACP_PROGRAM`.
    pub fn launch(cx: &mut Context<Self>) -> Self {
        let cwd = default_agent_cwd();

        let command = std::env::var_os("TILLER_ACP_PROGRAM")
            .map(PathBuf::from)
            .map(AgentCommand::new)
            .unwrap_or_else(|| {
                AgentCommand::new("npx")
                    .args(["-y", "@agentclientprotocol/claude-agent-acp@latest"])
            });

        let mut chat = Self::new(command, cwd, cx);
        chat.start_connection(cx, false);

        chat
    }

    fn new(command: AgentCommand, cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        Self::bind_keys(cx);
        let list_state = ListState::new(0, ListAlignment::Top, px(2048.0));
        list_state.set_follow_mode(FollowMode::Tail);

        Self {
            client: None,
            agent_command: command,
            agent_cwd: cwd,
            entries: Vec::new(),
            composer_text: String::new(),
            composer_cursor: 0,
            composer_selection_anchor: None,
            composer_focus: cx.focus_handle().tab_stop(true),
            model_picker_focus: cx.focus_handle().tab_stop(true),
            context_popover_focus: cx.focus_handle().tab_stop(true),
            transcript_focus: cx.focus_handle().tab_stop(false),
            streaming: false,
            connecting: false,
            retry_pending_send: false,
            has_completed_turn: false,
            available_models: Vec::new(),
            model_config_id: None,
            selected_model: None,
            model_picker_open: false,
            context_popover_open: false,
            context_usage: None,
            list_state,
            transcript_selection: None,
            transcript_dragging: false,
            _event_task: None,
        }
    }

    /// Add one transcript row and keep the virtualizer's index tree in sync.
    fn push_entry(&mut self, entry: Entry) {
        let index = self.entries.len();
        let following_tail = self.list_state.is_following_tail();
        self.entries.push(entry);
        self.list_state.splice(index..index, 1);
        if following_tail {
            self.list_state.set_follow_mode(FollowMode::Tail);
        }
    }

    fn remeasure_entry(&self, index: usize) {
        self.list_state.remeasure_items(index..index + 1);
    }

    /// Install the composer keymap in the host application.
    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("enter", Send, Some("ChatComposer")),
            KeyBinding::new("return", Send, Some("ChatComposer")),
            KeyBinding::new("shift-enter", Newline, Some("ChatComposer")),
            KeyBinding::new("shift-return", Newline, Some("ChatComposer")),
            KeyBinding::new("escape", Cancel, Some("ChatComposer")),
            KeyBinding::new("backspace", Backspace, Some("ChatComposer")),
            KeyBinding::new("delete", Delete, Some("ChatComposer")),
            KeyBinding::new("left", Left, Some("ChatComposer")),
            KeyBinding::new("right", Right, Some("ChatComposer")),
            KeyBinding::new("shift-left", SelectLeft, Some("ChatComposer")),
            KeyBinding::new("shift-right", SelectRight, Some("ChatComposer")),
            KeyBinding::new("cmd-a", SelectAll, Some("ChatComposer")),
            KeyBinding::new("cmd-c", CopyTranscript, Some("ChatTranscript")),
            KeyBinding::new("cmd-c", CopyTranscript, Some("ChatComposer")),
            KeyBinding::new("home", Home, Some("ChatComposer")),
            KeyBinding::new("end", End, Some("ChatComposer")),
            KeyBinding::new("cmd-left", Home, Some("ChatComposer")),
            KeyBinding::new("cmd-right", End, Some("ChatComposer")),
            KeyBinding::new("escape", Cancel, Some("ChatModelPicker")),
            KeyBinding::new("escape", Cancel, Some("ChatContextPopover")),
        ]);
    }

    fn handle_event(&mut self, event: AcpEvent, cx: &mut Context<Self>) {
        match event {
            AcpEvent::AgentMessageChunk(text) => {
                if let Some(Entry::Assistant {
                    text: existing,
                    document,
                }) = self.entries.last_mut()
                {
                    existing.push_str(&text);
                    *document = parse(existing);
                    self.remeasure_entry(self.entries.len() - 1);
                } else {
                    self.push_entry(Entry::Assistant {
                        document: parse(&text),
                        text,
                    });
                }
            }
            AcpEvent::ThoughtChunk(text) => {
                if let Some(Entry::Thought(existing)) = self.entries.last_mut() {
                    existing.push_str(&text);
                    self.remeasure_entry(self.entries.len() - 1);
                } else {
                    self.push_entry(Entry::Thought(text));
                }
            }
            AcpEvent::ToolCallStarted { id, title, status } => {
                self.push_entry(Entry::ToolCall { id, title, status });
            }
            AcpEvent::ToolCallUpdated { id, title, status } => {
                if let Some((index, Entry::ToolCall {
                    title: existing_title,
                    status: existing_status,
                    ..
                })) = self
                    .entries
                    .iter_mut()
                    .rev()
                    .enumerate()
                    .find(|(_, entry)| matches!(entry, Entry::ToolCall { id: entry_id, .. } if *entry_id == id))
                {
                    if let Some(title) = title {
                        *existing_title = title;
                    }
                    if let Some(status) = status {
                        *existing_status = status;
                    }
                    self.remeasure_entry(self.entries.len() - 1 - index);
                }
            }
            AcpEvent::ToolCallCompleted { id, status } => {
                if let Some((index, Entry::ToolCall {
                    status: existing_status,
                    ..
                })) = self
                    .entries
                    .iter_mut()
                    .rev()
                    .enumerate()
                    .find(|(_, entry)| matches!(entry, Entry::ToolCall { id: entry_id, .. } if *entry_id == id))
                {
                    *existing_status = status;
                    self.remeasure_entry(self.entries.len() - 1 - index);
                }
            }
            AcpEvent::ModelCatalog(ModelCatalog {
                config_id,
                options,
                selected_id,
            }) => {
                self.model_config_id = Some(config_id);
                self.available_models = options;
                self.selected_model = Some(selected_id);
            }
            AcpEvent::ContextUsage(usage) => {
                self.context_usage = Some(usage);
            }
            AcpEvent::PermissionRequest {
                request_id,
                options,
                ..
            } => {
                self.push_entry(Entry::Permission {
                    request_id,
                    options,
                    resolved: None,
                });
            }
            AcpEvent::TurnEnded { stop_reason } => {
                // The footer must state *why* the turn stopped. A cancelled
                // or refused turn that ends in a plain timestamp looks like an
                // ordinary completion, and that is the exact failure class
                // this surface is responsible for: the transcript is where a
                // user notices a hidden state least.
                let time = now_hhmm();
                let label = match stop_reason.as_str() {
                    "EndTurn" => time,
                    reason => format!("{time} · {}", turn_end_label(reason)),
                };
                self.push_entry(Entry::TurnFooter(label));
                self.streaming = false;
                self.has_completed_turn = true;
            }
            AcpEvent::TransportError(message) => {
                self.client.take();
                self.push_entry(Entry::Error {
                    message,
                    retryable: true,
                    kind: ErrorKind::Connection,
                });
                self.streaming = false;
            }
            AcpEvent::Timeout {
                operation,
                duration,
            } => {
                // A deadline expiring is the agent failing to answer, so it
                // belongs in the transcript like any other error rather than
                // leaving the composer stuck in its streaming state.
                self.client.take();
                self.push_entry(Entry::Error {
                    message: format!("{operation:?} timed out after {:.1}s", duration.as_secs_f32()),
                    retryable: true,
                    kind: ErrorKind::Connection,
                });
                self.streaming = false;
            }
            AcpEvent::OtherSessionUpdate { .. } => {}
        }
        cx.notify();
    }

    fn can_send(&self) -> bool {
        !self.streaming && !self.connecting && !self.composer_text.trim().is_empty()
    }

    fn transcript_entry_ranges(&self) -> Vec<Range<usize>> {
        let mut offset = 0;
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let length = entry.plain_text().len();
                let range = offset..offset + length;
                offset += length;
                if index + 1 < self.entries.len() {
                    offset += 2;
                }
                range
            })
            .collect()
    }

    fn transcript_text(&self) -> String {
        self.entries
            .iter()
            .map(Entry::plain_text)
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Returns the plain transcript retained by the shell when a chat tab is
    /// closed. The shell persists the opaque string, not ACP implementation
    /// details, so a later Resume Chat action can reconstruct visible history
    /// even when the original transport is gone.
    pub fn transcript_for_resume(&self) -> String {
        self.transcript_text()
    }

    /// Restores retained transcript text into a fresh chat surface. It is
    /// intentionally rendered as one historical assistant entry: preserving
    /// the exact visible transcript is more important than pretending the
    /// ACP event boundaries survived after the session was retained.
    pub fn restore_transcript(&mut self, transcript: &str, cx: &mut Context<Self>) {
        if transcript.is_empty() {
            return;
        }
        self.push_entry(Entry::Assistant {
            text: transcript.to_string(),
            document: parse(transcript),
        });
        self.has_completed_turn = true;
        cx.notify();
    }

    fn selected_transcript_text(&self) -> Option<String> {
        let selection = self.transcript_selection.as_ref()?.range();
        if selection.start >= selection.end {
            return None;
        }
        self.transcript_text().get(selection).map(ToOwned::to_owned)
    }

    fn copy_transcript(&mut self, _: &CopyTranscript, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(selected) = self.selected_transcript_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(selected));
        }
    }

    fn on_transcript_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "c" && event.keystroke.modifiers.platform {
            self.copy_transcript(&CopyTranscript, window, cx);
        }
    }

    fn clear_recovered_connection_errors(&mut self) {
        let old_count = self.entries.len();
        self.entries.retain(|entry| {
            !matches!(
                entry,
                Entry::Error {
                    kind: ErrorKind::Connection,
                    ..
                }
            )
        });
        if self.entries.len() != old_count {
            self.list_state.splice(0..old_count, self.entries.len());
        }
    }

    fn toggle_model_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.context_popover_open = false;
        self.model_picker_open = !self.model_picker_open;
        if self.model_picker_open {
            let focus = self.model_picker_focus.clone();
            window.focus(&focus, cx);
            window.on_next_frame(move |window, _| {
                window.on_next_frame(move |window, cx| window.focus(&focus, cx));
            });
        }
        cx.notify();
    }

    fn toggle_context_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.model_picker_open = false;
        self.context_popover_open = !self.context_popover_open;
        if self.context_popover_open {
            let focus = self.context_popover_focus.clone();
            window.focus(&focus, cx);
            window.on_next_frame(move |window, _| {
                window.on_next_frame(move |window, cx| window.focus(&focus, cx));
            });
        }
        cx.notify();
    }

    fn select_model(&mut self, option: ModelOption, cx: &mut Context<Self>) {
        if let (Some(client), Some(config_id)) = (&self.client, &self.model_config_id) {
            let _ = client.set_model(config_id.clone(), option.id.clone());
        }
        self.selected_model = Some(option.id);
        self.model_picker_open = false;
        cx.notify();
    }

    fn send(&mut self, cx: &mut Context<Self>) {
        if !self.can_send() {
            return;
        }
        if self.client.is_none() {
            self.retry_pending_send = true;
            self.start_connection(cx, true);
            return;
        }
        let text = std::mem::take(&mut self.composer_text);
        self.composer_cursor = 0;
        self.composer_selection_anchor = None;
        self.push_entry(Entry::User(text.clone()));
        self.streaming = true;
        if let Some(client) = &self.client
            && let Err(error) = client.prompt(&text)
        {
            self.client.take();
            self.composer_text = text;
            self.composer_cursor = self.composer_text.len();
            self.push_entry(Entry::Error {
                message: format!("send failed: {error:#}"),
                retryable: true,
                kind: ErrorKind::Connection,
            });
            self.streaming = false;
        }
        cx.notify();
    }

    fn respond_permission(
        &mut self,
        request_id: u64,
        option: &PermissionOption,
        cx: &mut Context<Self>,
    ) {
        if let Some((index, Entry::Permission { resolved, .. })) = self.entries.iter_mut().enumerate().rev().find(
            |(_, entry)| matches!(entry, Entry::Permission { request_id: id, .. } if *id == request_id),
        ) {
            *resolved = Some(option.name.clone());
            self.remeasure_entry(index);
        }
        if let Some(client) = &self.client {
            let _ = client.respond_permission(request_id, option.id.clone());
        }
        cx.notify();
    }

    fn selected_range(&self) -> Option<Range<usize>> {
        let anchor = self.composer_selection_anchor?;
        (anchor != self.composer_cursor).then(|| {
            let (start, end) = if anchor < self.composer_cursor {
                (anchor, self.composer_cursor)
            } else {
                (self.composer_cursor, anchor)
            };
            start..end
        })
    }

    fn delete_selected(&mut self) -> bool {
        let Some(range) = self.selected_range() else {
            return false;
        };
        self.composer_text.replace_range(range.clone(), "");
        self.composer_cursor = range.start;
        self.composer_selection_anchor = None;
        true
    }

    fn insert_text(&mut self, text: &str) {
        self.delete_selected();
        self.composer_text.insert_str(self.composer_cursor, text);
        self.composer_cursor += text.len();
    }

    fn previous_boundary(&self) -> usize {
        self.composer_text[..self.composer_cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index)
    }

    fn next_boundary(&self) -> usize {
        self.composer_text[self.composer_cursor..]
            .char_indices()
            .nth(1)
            .map_or(self.composer_text.len(), |(index, _)| {
                self.composer_cursor + index
            })
    }

    fn move_cursor(&mut self, cursor: usize, extend_selection: bool) {
        if extend_selection {
            if self.composer_selection_anchor.is_none() {
                self.composer_selection_anchor = Some(self.composer_cursor);
            }
        } else {
            self.composer_selection_anchor = None;
        }
        self.composer_cursor = cursor;
    }

    fn send_action(&mut self, _: &Send, _: &mut Window, cx: &mut Context<Self>) {
        self.send(cx);
    }

    fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        self.insert_text("\n");
        cx.notify();
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        if self.model_picker_open {
            self.model_picker_open = false;
            cx.notify();
        } else if self.context_popover_open {
            self.context_popover_open = false;
            cx.notify();
        } else {
            self.cancel_turn(cx);
        }
    }

    /// Cancels a streaming turn: asks the ACP client to stop, returns the
    /// composer to its idle state, and leaves the transcript showing what
    /// had already arrived. Safe to call when nothing is streaming — it is
    /// a no-op then.
    fn cancel_turn(&mut self, cx: &mut Context<Self>) {
        if self.streaming {
            if let Some(client) = &self.client {
                let _ = client.cancel();
            }
            self.streaming = false;
            cx.notify();
        }
    }

    fn start_connection(&mut self, cx: &mut Context<Self>, retry_pending_send: bool) {
        if self.connecting {
            return;
        }

        self.client.take();
        self.connecting = true;
        self.retry_pending_send = retry_pending_send;
        let command = self.agent_command.clone();
        let cwd = self.agent_cwd.clone();
        self._event_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { AcpClient::launch(command, cwd) })
                .await;

            match result {
                Ok((client, events)) => {
                    let initial_catalog = client.model_catalog().cloned();
                    let should_send = this
                        .update(cx, |chat, _| {
                            chat.clear_recovered_connection_errors();
                            chat.client = Some(client);
                            if let Some(ModelCatalog {
                                config_id,
                                options,
                                selected_id,
                            }) = initial_catalog
                            {
                                chat.model_config_id = Some(config_id);
                                chat.available_models = options;
                                chat.selected_model = Some(selected_id);
                            }
                            chat.connecting = false;
                            std::mem::take(&mut chat.retry_pending_send)
                        })
                        .unwrap_or(false);

                    if should_send {
                        let _ = this.update(cx, |chat, cx| chat.send(cx));
                    }

                    while let Ok(event) = events.recv().await {
                        if this
                            .update(cx, |chat, cx| chat.handle_event(event, cx))
                            .is_err()
                        {
                            break;
                        }
                    }

                    // The event source closed without a terminal event — a
                    // worker crash, or a transport death that skipped the
                    // error path. The composer must never be left working by
                    // a state only a happy turn-ending event can leave, so
                    // say what was lost and come back idle.
                    let _ = this.update(cx, |chat, cx| {
                        if chat.streaming {
                            chat.push_entry(Entry::Error {
                                message: "agent transport closed".to_string(),
                                // Relaunching is exactly the right response
                                // here, so offer it rather than dead-ending.
                                retryable: true,
                                kind: ErrorKind::Connection,
                            });
                            chat.streaming = false;
                            cx.notify();
                        }
                    });
                }
                Err(error) => {
                    let _ = this.update(cx, |chat, cx| {
                        chat.connecting = false;
                        chat.client = None;
                        chat.streaming = false;
                        chat.push_entry(Entry::Error {
                            message: format!("could not launch ACP agent: {error:#}"),
                            retryable: true,
                            kind: ErrorKind::Connection,
                        });
                        cx.notify();
                    });
                }
            }
        }));
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        self.start_connection(cx, false);
    }

    #[cfg(test)]
    fn from_test_command(command: AgentCommand, cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        let mut chat = Self::new(command, cwd, cx);
        chat.start_connection(cx, false);
        chat
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if !self.delete_selected() && self.composer_cursor > 0 {
            let previous = self.previous_boundary();
            self.composer_text.drain(previous..self.composer_cursor);
            self.composer_cursor = previous;
        }
        cx.notify();
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if !self.delete_selected() && self.composer_cursor < self.composer_text.len() {
            let next = self.next_boundary();
            self.composer_text.drain(self.composer_cursor..next);
        }
        cx.notify();
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(range) = self.selected_range() {
            self.move_cursor(range.start, false);
        } else {
            self.move_cursor(self.previous_boundary(), false);
        }
        cx.notify();
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(range) = self.selected_range() {
            self.move_cursor(range.end, false);
        } else {
            self.move_cursor(self.next_boundary(), false);
        }
        cx.notify();
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(self.previous_boundary(), true);
        cx.notify();
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(self.next_boundary(), true);
        cx.notify();
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.composer_selection_anchor = Some(0);
        self.composer_cursor = self.composer_text.len();
        cx.notify();
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(0, false);
        cx.notify();
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(self.composer_text.len(), false);
        cx.notify();
    }

    fn on_composer_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "c"
            && event.keystroke.modifiers.platform
            && self.transcript_selection.is_some()
        {
            self.copy_transcript(&CopyTranscript, _window, cx);
            return;
        }
        // Bound keys (enter, shift-enter, escape, …) are consumed by the
        // keymap actions first — gpui stops propagation once an action
        // listener fires — so these branches are fallbacks for hosts that
        // never installed the keymap via [`Chat::bind_keys`]. Escape in
        // particular must always cancel a streaming turn, whether it arrives
        // as the `Cancel` action or as a raw key event.
        if matches!(event.keystroke.key.as_str(), "enter" | "return") {
            if event.keystroke.modifiers.shift {
                self.insert_text("\n");
                cx.notify();
            } else {
                self.send(cx);
            }
            return;
        }
        if event.keystroke.key == "escape" {
            self.cancel_turn(cx);
            return;
        }

        if let Some(character) = event.keystroke.key_char.as_deref()
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
            && character != "\n"
        {
            self.insert_text(character);
            cx.notify();
        }
    }

    fn render_markdown(
        document: Document,
        theme: &Theme,
        interaction: Option<TranscriptInteraction>,
        source_start: usize,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .children(document.blocks.into_iter().enumerate().scan(
                source_start,
                move |block_start, (index, block)| {
                    let rendered = Self::render_markdown_block(
                        block.clone(),
                        theme,
                        format!("assistant-block-{index}"),
                        interaction.as_ref(),
                        *block_start,
                    );
                    *block_start += block.plain_text().len() + 2;
                    Some(rendered)
                },
            ))
            .into_any_element()
    }

    /// Shared markdown renderer used by both assistant replies and file tabs.
    /// Keeping this entry point on `Chat` means file tabs use the same
    /// structured `tiller_markdown` tree and styling as the transcript rather
    /// than growing a second markdown renderer.
    pub(crate) fn render_markdown_document(document: Document, theme: &Theme) -> AnyElement {
        Self::render_markdown(document, theme, None, 0)
    }

    fn render_markdown_block(
        block: Block,
        theme: &Theme,
        id: String,
        interaction: Option<&TranscriptInteraction>,
        source_start: usize,
    ) -> AnyElement {
        let colors = theme.colors;
        let typography = theme.typography;
        match block {
            Block::Heading { level, inline } => div()
                .w_full()
                .text_size(markdown_heading_size(level, typography))
                .line_height(px(
                    f32::from(markdown_heading_size(level, typography)) * 1.42
                ))
                .font_weight(FontWeight::BOLD)
                .text_color(colors.title)
                .child(Self::render_inline(
                    inline,
                    theme,
                    id,
                    interaction,
                    source_start,
                ))
                .into_any_element(),
            Block::Paragraph { inline } => div()
                .w_full()
                .text_size(typography.headline)
                .line_height(typography.body_line_height)
                .text_color(colors.title)
                .child(Self::render_inline(
                    inline,
                    theme,
                    id,
                    interaction,
                    source_start,
                ))
                .into_any_element(),
            Block::List { kind, items, .. } => {
                Self::render_markdown_list(kind, items, theme, id, 0, source_start, interaction)
            }
            Block::BlockQuote { blocks } => div()
                .w_full()
                .flex()
                .border_l_2()
                .border_color(colors.subtitle)
                .pl(px(12.0))
                .child(div().w_full().flex().flex_col().gap(px(7.0)).children(
                    blocks.into_iter().enumerate().scan(
                        source_start,
                        |block_start, (index, block)| {
                            let rendered = Self::render_markdown_block(
                                block.clone(),
                                theme,
                                format!("{id}-quote-{index}"),
                                interaction,
                                *block_start,
                            );
                            *block_start += block.plain_text().len() + 2;
                            Some(rendered)
                        },
                    ),
                ))
                .into_any_element(),
            Block::CodeBlock {
                language,
                text,
                open,
            } => {
                let language_label = language.unwrap_or_else(|| "code".to_string());
                let language_label = if open {
                    format!("{language_label} · streaming")
                } else {
                    language_label
                };
                div()
                    .w_full()
                    .rounded(theme.radii.code_block)
                    .bg(colors.code_inset_fill)
                    .px(px(12.0))
                    .py(px(9.0))
                    .flex()
                    .flex_col()
                    .gap(px(7.0))
                    .child(
                        div()
                            .text_size(typography.footnote)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.meta)
                            .child(Self::render_plain_text(
                                language_label.clone(),
                                theme,
                                format!("{id}-language"),
                                source_start,
                                interaction,
                            )),
                    )
                    .child(
                        div()
                            .font_family(typography.code_family)
                            .text_size(typography.code_size)
                            .line_height(typography.code_line_height)
                            .text_color(colors.primary_text_color)
                            .child(Self::render_plain_text(
                                text,
                                theme,
                                format!("{id}-code"),
                                source_start + language_label.len() + 1,
                                interaction,
                            )),
                    )
                    .into_any_element()
            }
            Block::Table {
                alignment,
                header,
                rows,
            } => Self::render_markdown_table(
                alignment,
                header,
                rows,
                theme,
                id,
                interaction,
                source_start,
            ),
            Block::ThematicBreak => div()
                .w_full()
                .h(px(1.0))
                .my(px(4.0))
                .bg(colors.hairline)
                .into_any_element(),
            Block::Html { text } => div()
                .w_full()
                .text_size(typography.headline)
                .text_color(colors.title)
                .child(Self::render_plain_text(
                    text,
                    theme,
                    id,
                    source_start,
                    interaction,
                ))
                .into_any_element(),
        }
    }

    fn render_plain_text(
        text: String,
        theme: &Theme,
        id: String,
        source_start: usize,
        interaction: Option<&TranscriptInteraction>,
    ) -> AnyElement {
        let styled = StyledText::new(text.clone());
        if let Some(interaction) = interaction {
            TranscriptSelectableText::new(
                ElementId::Name(id.into()),
                styled,
                source_start..source_start + text.len(),
                interaction.clone(),
                theme.colors.selection_fill,
                Vec::new(),
            )
            .into_any_element()
        } else {
            styled.into_any_element()
        }
    }

    fn render_markdown_list(
        kind: ListKind,
        items: Vec<ListItem>,
        theme: &Theme,
        id: String,
        depth: usize,
        source_start: usize,
        interaction: Option<&TranscriptInteraction>,
    ) -> AnyElement {
        let colors = theme.colors;
        let typography = theme.typography;
        div()
            .w_full()
            .pl(px(18.0 * depth as f32))
            .flex()
            .flex_col()
            .gap(px(5.0))
            .children(items.into_iter().enumerate().scan(
                source_start,
                move |item_start, (index, item)| {
                    let marker = match kind {
                        ListKind::Bullet => "•".to_string(),
                        ListKind::Ordered { start } => format!("{}.", start + index as u64),
                    };
                    let marker = item
                        .checked
                        .map(|checked| if checked { "☑" } else { "☐" })
                        .unwrap_or(&marker)
                        .to_string();
                    let block_source_start = *item_start + marker.len() + 1;
                    let item_text = item
                        .blocks
                        .iter()
                        .map(Block::plain_text)
                        .collect::<Vec<_>>()
                        .join("\n");
                    let item_length = marker.len() + 1 + item_text.len();
                    let children = item.blocks.into_iter().enumerate().scan(
                        block_source_start,
                        |block_start, (block_index, block)| {
                            let rendered = match block.clone() {
                                Block::List { kind, items, .. } => Self::render_markdown_list(
                                    kind,
                                    items,
                                    theme,
                                    format!("{id}-{index}-{block_index}"),
                                    depth + 1,
                                    *block_start,
                                    interaction,
                                ),
                                block => Self::render_markdown_block(
                                    block,
                                    theme,
                                    format!("{id}-{index}-{block_index}"),
                                    interaction,
                                    *block_start,
                                ),
                            };
                            *block_start += block.plain_text().len() + 1;
                            Some(rendered)
                        },
                    );
                    let rendered = div()
                        .w_full()
                        .flex()
                        .items_start()
                        .gap(px(8.0))
                        .child(
                            div()
                                .w(px(18.0))
                                .text_size(typography.headline)
                                .text_color(colors.file_link)
                                .child(Self::render_plain_text(
                                    marker,
                                    theme,
                                    format!("{id}-{index}-marker"),
                                    *item_start,
                                    interaction,
                                )),
                        )
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(5.0))
                                .children(children),
                        );
                    *item_start += item_length + 1;
                    Some(rendered)
                },
            ))
            .into_any_element()
    }

    fn render_markdown_table(
        alignment: Vec<Alignment>,
        header: Vec<tiller_markdown::TableCell>,
        rows: Vec<Vec<tiller_markdown::TableCell>>,
        theme: &Theme,
        id: String,
        interaction: Option<&TranscriptInteraction>,
        source_start: usize,
    ) -> AnyElement {
        let colors = theme.colors;
        let typography = theme.typography;
        let render_row = |cells: Vec<tiller_markdown::TableCell>,
                          row_id: String,
                          header_row: bool,
                          row_start: usize| {
            div()
                .w_full()
                .flex()
                .border_b_1()
                .border_color(colors.hairline)
                .when(header_row, |this| this.bg(colors.card_fill))
                .children(cells.into_iter().enumerate().scan(
                    row_start,
                    |cell_start, (index, cell)| {
                        let alignment = alignment.get(index).copied().unwrap_or(Alignment::None);
                        let cell_text = tiller_markdown::Inline::plain_text_all(&cell.inline);
                        let mut cell_view = div()
                            .flex_1()
                            .px(px(8.0))
                            .py(px(5.0))
                            .flex()
                            .text_size(typography.callout)
                            .text_color(colors.title);
                        cell_view = match alignment {
                            Alignment::Center => cell_view.justify_center(),
                            Alignment::Right => cell_view.justify_end(),
                            Alignment::None | Alignment::Left => cell_view.justify_start(),
                        };
                        cell_view = cell_view.child(Self::render_inline(
                            cell.inline,
                            theme,
                            format!("{row_id}-cell-{index}"),
                            interaction,
                            *cell_start,
                        ));
                        *cell_start += cell_text.len() + 1;
                        Some(cell_view.into_any_element())
                    },
                ))
        };

        let header_text_len = header
            .iter()
            .map(|cell| tiller_markdown::Inline::plain_text_all(&cell.inline).len() + 1)
            .sum::<usize>();

        div()
            .w_full()
            .flex()
            .flex_col()
            .border_1()
            .border_color(colors.hairline)
            .rounded(theme.radii.control)
            .overflow_hidden()
            .child(render_row(
                header,
                format!("{id}-header"),
                true,
                source_start,
            ))
            .children(rows.into_iter().enumerate().scan(
                source_start + header_text_len,
                |row_start, (index, row)| {
                    let row_text_len = row
                        .iter()
                        .map(|cell| tiller_markdown::Inline::plain_text_all(&cell.inline).len() + 1)
                        .sum::<usize>();
                    let rendered = render_row(row, format!("{id}-row-{index}"), false, *row_start);
                    *row_start += row_text_len + 1;
                    Some(rendered)
                },
            ))
            .into_any_element()
    }

    fn render_inline(
        inline: Vec<Inline>,
        theme: &Theme,
        id: String,
        interaction: Option<&TranscriptInteraction>,
        source_start: usize,
    ) -> AnyElement {
        let mut builder = InlineBuilder::default();
        for inline in &inline {
            builder.append(inline, theme);
        }

        let links = builder.links.clone();
        let rendered_len = builder.text.len();
        let link_ranges = links
            .iter()
            .map(|(range, _)| range.clone())
            .collect::<Vec<_>>();
        let link_targets = links
            .clone()
            .into_iter()
            .map(|(_, target)| target)
            .collect::<Vec<_>>();
        let styled = StyledText::new(builder.text)
            .with_highlights(builder.highlights)
            .with_font_family_overrides(builder.font_overrides);
        let text = if let Some(interaction) = interaction {
            TranscriptSelectableText::new(
                ElementId::Name(id.into()),
                styled,
                source_start..source_start + rendered_len,
                interaction.clone(),
                theme.colors.selection_fill,
                links,
            )
            .into_any_element()
        } else if link_ranges.is_empty() {
            styled.into_any_element()
        } else {
            InteractiveText::new(ElementId::Name(id.into()), styled)
                .on_click(link_ranges, move |index, _, cx| {
                    if let Some(target) = link_targets.get(index) {
                        cx.open_url(target);
                    }
                })
                .into_any_element()
        };
        div().w_full().child(text).into_any_element()
    }

    fn render_entry(
        entry: Entry,
        entry_index: usize,
        theme: &Theme,
        entity: gpui::Entity<Self>,
        transcript_focus: FocusHandle,
        source_start: usize,
    ) -> impl IntoElement {
        let colors = theme.colors;
        let typography = theme.typography;
        let interaction = TranscriptInteraction {
            chat: entity.clone(),
            focus: transcript_focus,
        };
        match entry {
            Entry::User(text) => div()
                .w_full()
                .flex()
                .justify_end()
                .child(
                    div()
                        .max_w(px(USER_PILL_MAX_WIDTH))
                        .rounded(theme.radii.user_pill)
                        .bg(colors.raised)
                        .px(px(12.0))
                        .py(px(8.0))
                        .text_size(typography.headline)
                        .line_height(typography.body_line_height)
                        .text_color(colors.title)
                        .child(Self::render_plain_text(
                            text,
                            theme,
                            format!("user-entry-{entry_index}"),
                            source_start,
                            Some(&interaction),
                        )),
                )
                .into_any_element(),
            Entry::Assistant { document, .. } => {
                Self::render_markdown(document, theme, Some(interaction), source_start)
            }
            Entry::Thought(text) => div()
                .w_full()
                .text_size(typography.callout)
                .line_height(px(19.0))
                .text_color(colors.subtitle)
                .italic()
                .child(Self::render_plain_text(
                    text,
                    theme,
                    format!("thought-entry-{entry_index}"),
                    source_start,
                    Some(&interaction),
                ))
                .into_any_element(),
            Entry::ToolCall { title, status, .. } => div()
                .w_full()
                .flex()
                .rounded(theme.radii.code_block)
                .bg(colors.card_fill)
                .border_l_2()
                .border_color(colors.rail_tool)
                .child(
                    div()
                        .flex_1()
                        .px(px(CARD_H_PADDING))
                        .py(px(8.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(typography.callout)
                                .text_color(colors.title)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(typography.footnote)
                                .text_color(colors.meta)
                                .child(status),
                        ),
                )
                .into_any_element(),
            Entry::Permission {
                request_id,
                options,
                resolved,
            } => {
                let mut card = div()
                    .w_full()
                    .rounded(theme.radii.code_block)
                    .bg(colors.card_fill)
                    .border_l_2()
                    .border_color(colors.rail_question)
                    .px(px(CARD_H_PADDING))
                    .py(px(CARD_V_PADDING))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(typography.callout)
                            .text_color(colors.title)
                            .child("Permission requested"),
                    );
                if let Some(choice) = resolved {
                    card = card.child(
                        div()
                            .text_size(typography.footnote)
                            .text_color(colors.meta)
                            .child(format!("Answered: {choice}")),
                    );
                } else {
                    let mut row = div().flex().gap(px(8.0));
                    for option in options {
                        let entity = entity.clone();
                        let option_for_click = option.clone();
                        let option_id = option.id.clone();
                        row = row.child(
                            div()
                                .id((
                                    "permission-option",
                                    request_id as usize ^ option_hash(&option),
                                ))
                                .debug_selector(move || format!("permission-option-{option_id}"))
                                .px(px(10.0))
                                .py(px(5.0))
                                .rounded(theme.radii.control)
                                .bg(colors.primary_pill_bg)
                                .text_size(typography.footnote)
                                .text_color(colors.title)
                                .hover(|style| style.bg(colors.chat_row_hover))
                                .on_click(move |_, _, cx| {
                                    entity.update(cx, |chat, cx| {
                                        chat.respond_permission(request_id, &option_for_click, cx);
                                    });
                                })
                                .child(option.name.clone()),
                        );
                    }
                    card = card.child(row);
                }
                card.into_any_element()
            }
            Entry::TurnFooter(at) => div()
                .w_full()
                .h(px(24.0))
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(div().h(px(1.0)).flex_1().bg(colors.hairline))
                .child(
                    div()
                        .text_size(typography.footnote)
                        .text_color(colors.meta)
                        .child(at.clone()),
                )
                .child(div().h(px(1.0)).flex_1().bg(colors.hairline))
                .into_any_element(),
            Entry::Error {
                message, retryable, ..
            } => {
                let retry_entity = entity.clone();
                div()
                    .w_full()
                    .rounded(theme.radii.code_block)
                    .bg(colors.diff_deletion_background)
                    .border_l_2()
                    .border_color(colors.diff_deletion)
                    .px(px(CARD_H_PADDING))
                    .py(px(CARD_V_PADDING))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .text_size(typography.callout)
                    .text_color(colors.diff_deletion)
                    .child(div().flex_1().child(message))
                    .when(retryable, |this| {
                        this.child(
                            div()
                                .id(("retry", entry_index))
                                .debug_selector(|| "chat-retry".into())
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded(theme.radii.control)
                                .text_color(colors.title)
                                .bg(colors.card_fill)
                                .hover(|style| style.bg(colors.chat_row_hover))
                                .on_click(move |_, _, cx| {
                                    retry_entity.update(cx, |chat, cx| chat.retry(cx));
                                })
                                .child("Retry"),
                        )
                    })
                    .into_any_element()
            }
        }
    }

    fn render_composer(
        &self,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = theme.colors;
        let typography = theme.typography;
        let focused = self.composer_focus.is_focused(window);
        let can_send = self.can_send();
        let entity = cx.entity();
        let entity_for_focus = entity.clone();

        // Swift's `modePill` (ComposerControlBar.swift) always pairs a
        // status dot with a label, whether that label is a raw state word
        // ("idle"/"working") or a mode name ("Ask") — the dot survives the
        // switch to the post-turn mode dropdown, it never disappears.
        let connecting = self.connecting;
        let (dot, label) = if connecting {
            (rgb(0xf5a623), "connecting")
        } else if self.streaming {
            (rgb(0xf5a623), "working")
        } else if self.has_completed_turn {
            (rgb(0x53c653), "Ask")
        } else if self.client.is_some() {
            (rgb(0x53c653), "idle")
        } else {
            (rgb(0x8a8d99), "offline")
        };
        let status_pill = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(24.0))
            .px(px(7.0))
            .rounded(theme.radii.control)
            .bg(colors.raised)
            .text_size(typography.ui_size)
            .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(dot))
            .child(div().text_color(colors.title).child(label))
            .when(self.has_completed_turn, |this| {
                this.child(div().text_color(colors.meta).child("⌄"))
            });
        let status_pill = if connecting {
            status_pill
                .id("chat-connecting")
                .debug_selector(|| "chat-connecting".into())
        } else {
            status_pill
                .id("chat-status")
                .debug_selector(|| "chat-status".into())
        };

        let selected_model_name = self
            .selected_model
            .as_deref()
            .and_then(|selected| {
                self.available_models
                    .iter()
                    .find(|option| option.id == selected)
                    .map(|option| option.name.clone())
            })
            .or_else(|| self.selected_model.clone())
            .unwrap_or_else(|| "Claude Code".into());
        let model_entity = entity.clone();
        // A labelled chip — "Model" in muted chrome text, the value in
        // title text — waku's composer-chip anatomy. The picker opens only
        // once a turn has completed and the agent reported models; before
        // that the chip is a static agent name.
        let model_chip = div()
            .id("model-chip")
            .debug_selector(|| "model-chip".into())
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(24.0))
            .px(px(7.0))
            .rounded(theme.radii.control)
            .bg(colors.raised)
            .text_size(typography.ui_size)
            .hover(|style| style.bg(colors.chat_row_hover))
            .when(self.has_completed_turn, |this| {
                this.on_click(cx.listener(|this, _, window, cx| {
                    this.toggle_model_picker(window, cx);
                }))
            })
            .child(div().text_color(colors.meta).child("Model"))
            .child(
                div()
                    .id(self
                        .selected_model
                        .as_deref()
                        .map(|id| format!("model-selection-{id}"))
                        .unwrap_or_else(|| "model-selection-none".into()))
                    .debug_selector(move || {
                        self.selected_model
                            .as_deref()
                            .map(|id| format!("model-selection-{id}"))
                            .unwrap_or_else(|| "model-selection-none".into())
                    })
                    .text_color(colors.title)
                    .child(selected_model_name.clone()),
            )
            .when(self.has_completed_turn, |this| {
                this.child(div().text_color(colors.meta).child("⌄"))
            });

        let model_picker = if self.model_picker_open {
            let picker_entity = model_entity.clone();
            Some(
                div()
                    .id("model-picker")
                    .debug_selector(|| "model-picker".into())
                    .key_context("ChatModelPicker")
                    .track_focus(&self.model_picker_focus)
                    .on_action(cx.listener(Self::cancel))
                    .absolute()
                    .right(px(42.0))
                    .bottom(px(43.0))
                    .w(px(245.0))
                    .p(px(8.0))
                    .rounded(theme.radii.toast)
                    .bg(colors.card_fill)
                    .border_1()
                    .border_color(colors.hairline)
                    .shadow_lg()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.model_picker_open = false;
                        cx.notify();
                    }))
                    .when(self.available_models.is_empty(), |this| {
                        this.child(
                            div()
                                .p(px(8.0))
                                .text_size(typography.footnote)
                                .text_color(colors.meta)
                                .child("The connected agent did not report any models."),
                        )
                    })
                    .children(self.available_models.iter().cloned().map(move |option| {
                        let option_id = option.id.clone();
                        let option_name = option.name.clone();
                        let option_entity = picker_entity.clone();
                        div()
                            .id(format!("model-option-{option_id}"))
                            .debug_selector(move || format!("model-option-{option_id}"))
                            .w_full()
                            .px(px(8.0))
                            .py(px(7.0))
                            .rounded(theme.radii.control)
                            .text_size(typography.footnote)
                            .text_color(colors.title)
                            .hover(|style| style.bg(colors.chat_row_hover))
                            .on_click(move |_, _, cx| {
                                option_entity
                                    .update(cx, |chat, cx| chat.select_model(option.clone(), cx));
                            })
                            .child(option_name)
                    })),
            )
        } else {
            None
        };

        let context_usage = self.context_usage.clone();
        let context_amount = context_usage
            .as_ref()
            .filter(|usage| usage.size > 0)
            .map(|usage| (usage.used as f32 / usage.size as f32).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let context_ring_entity = entity.clone();
        let context_ring = div()
            .id("context-ring")
            .debug_selector(|| "context-ring".into())
            .relative()
            .w(px(16.0))
            .h(px(16.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(colors.hairline)
            .hover(|style| style.bg(colors.chat_row_hover))
            .on_click(move |_, window, cx| {
                context_ring_entity.update(cx, |chat, cx| chat.toggle_context_popover(window, cx));
            })
            .child(
                div()
                    .id("context-ring-progress")
                    .debug_selector(|| "context-ring-progress".into())
                    .absolute()
                    .inset_0()
                    .child(
                        canvas(
                            move |_, _, _| {},
                            move |bounds, _, window, _| {
                                if context_amount <= 0.0 {
                                    return;
                                }
                                let origin_x: f32 = bounds.origin.x.into();
                                let origin_y: f32 = bounds.origin.y.into();
                                let width: f32 = bounds.size.width.into();
                                let center =
                                    point(px(origin_x + width / 2.0), px(origin_y + width / 2.0));
                                let radius = width / 2.0 - 2.0;
                                let start = point(center.x, px(origin_y + 1.0));
                                let angle = -std::f32::consts::FRAC_PI_2
                                    + context_amount * std::f32::consts::TAU;
                                let end = point(
                                    px(origin_x + width / 2.0 + radius * angle.cos()),
                                    px(origin_y + width / 2.0 + radius * angle.sin()),
                                );
                                let mut path = PathBuilder::stroke(px(2.0));
                                path.move_to(start);
                                if context_amount >= 1.0 {
                                    path.arc_to(
                                        point(px(radius), px(radius)),
                                        px(0.0),
                                        false,
                                        true,
                                        point(center.x, px(origin_y + width - 1.0)),
                                    );
                                    path.arc_to(
                                        point(px(radius), px(radius)),
                                        px(0.0),
                                        false,
                                        true,
                                        start,
                                    );
                                } else if context_amount > 0.0 {
                                    path.arc_to(
                                        point(px(radius), px(radius)),
                                        px(0.0),
                                        context_amount > 0.5,
                                        true,
                                        end,
                                    );
                                }
                                if let Ok(path) = path.build() {
                                    window.paint_path(path, colors.gauge);
                                }
                            },
                        )
                        .absolute()
                        .size_full(),
                    ),
            );

        let context_popover = if self.context_popover_open {
            let usage = context_usage.clone();
            Some(
                div()
                    .id("context-popover")
                    .debug_selector(|| "context-popover".into())
                    .key_context("ChatContextPopover")
                    .track_focus(&self.context_popover_focus)
                    .on_action(cx.listener(Self::cancel))
                    .absolute()
                    .right(px(16.0))
                    .bottom(px(43.0))
                    .w(px(285.0))
                    .p(px(12.0))
                    .rounded(theme.radii.toast)
                    .bg(colors.card_fill)
                    .border_1()
                    .border_color(colors.hairline)
                    .shadow_lg()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.context_popover_open = false;
                        cx.notify();
                    }))
                    .when_some(usage, |this, usage| {
                        let percent = if usage.size == 0 {
                            0
                        } else {
                            ((usage.used as f64 / usage.size as f64) * 100.0).round() as u64
                        };
                        let cost = usage
                            .cost
                            .map(|cost| format!("Cost: {:.2} {}", cost.amount, cost.currency));
                        this.child(
                            div()
                                .id(format!("context-usage-{}-of-{}", usage.used, usage.size))
                                .debug_selector(move || {
                                    format!("context-usage-{}-of-{}", usage.used, usage.size)
                                })
                                .text_size(typography.footnote)
                                .text_color(colors.title)
                                .child(format!("{percent}% of context used")),
                        )
                        .child(
                            div()
                                .mt(px(4.0))
                                .text_size(typography.caption2)
                                .text_color(colors.meta)
                                .child(format!("{} / {} tokens", usage.used, usage.size)),
                        )
                        .when_some(cost, |this, cost| {
                            this.child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(typography.caption2)
                                    .text_color(colors.meta)
                                    .child(cost),
                            )
                        })
                    })
                    .when(self.context_usage.is_none(), |this| {
                        this.child(
                            div()
                                .text_size(typography.footnote)
                                .text_color(colors.meta)
                                .child("The agent has not reported context usage yet."),
                        )
                    }),
            )
        } else {
            None
        };

        let send_entity = entity.clone();
        let context_percent = context_usage
            .as_ref()
            .filter(|usage| usage.size > 0)
            .map(|usage| ((usage.used as f64 / usage.size as f64) * 100.0).round() as u64)
            .unwrap_or(0);

        // The composer is the visual anchor: a raised card with a roomy
        // input and one row of labelled chips — status, model, context —
        // ending in the circular send control. The card is waku's: max
        // 720px, 13px radius, `composer` fill, a hairline border that turns
        // coral while focused.
        div()
            .id("composer")
            .debug_selector(|| "composer".into())
            .relative()
            .w(px(TRANSCRIPT_WIDTH))
            .rounded(theme.radii.composer)
            .bg(colors.composer)
            .border_1()
            .border_color(if focused {
                colors.accent
            } else {
                colors.hairline
            })
            .p(px(10.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.composer_focus.focus(window, cx);
                    let _ = &entity_for_focus;
                }),
            )
            .child(
                div()
                    .px(px(4.0))
                    .pt(px(2.0))
                    .min_h(px(44.0))
                    .text_size(typography.headline)
                    .line_height(typography.body_line_height)
                    .text_color(if self.composer_text.is_empty() {
                        colors.meta
                    } else {
                        colors.primary_text_color
                    })
                    .child(if self.composer_text.is_empty() {
                        "Message...".to_owned()
                    } else {
                        self.composer_text.clone()
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(status_pill)
                    .child(model_chip)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .h(px(24.0))
                            .px(px(7.0))
                            .rounded(theme.radii.control)
                            .bg(colors.raised)
                            .text_size(typography.ui_size)
                            .child(context_ring)
                            .child(
                                div()
                                    .text_color(colors.title)
                                    .child(format!("{context_percent}%")),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("send")
                            .debug_selector(|| "send".into())
                            .w(px(26.0))
                            .h(px(26.0))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(14.0))
                            .bg(if can_send {
                                colors.overlay_strong
                            } else {
                                colors.overlay
                            })
                            .text_color(if can_send {
                                colors.title
                            } else {
                                colors.text_ghost
                            })
                            .hover(|style| style.bg(colors.raised))
                            .when(can_send, |this| {
                                this.on_click(move |_, _, cx| {
                                    send_entity.update(cx, |chat, cx| chat.send(cx));
                                })
                            })
                            .child("↑"),
                    ),
            )
            .children(model_picker)
            .children(context_popover)
    }
}

impl Chat {
    // --- Read-only status, for the shell's activity indicators (sidebar
    // dot / tab checkmark / Activity row) — P28. Added at the end of this
    // impl block, touching no existing line, so it stays out of the way of
    // whatever else is in flight here. The shell polls these fresh every
    // render, the same as it does `Theme::get(cx)`; nothing is cached.

    /// Whether a turn is currently streaming in.
    pub fn is_streaming(&self) -> bool {
        self.streaming
    }

    /// Whether at least one turn has completed.
    pub fn has_completed_turn(&self) -> bool {
        self.has_completed_turn
    }
}

impl Focusable for Chat {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.composer_focus.clone()
    }
}

impl Render for Chat {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Fetched fresh every frame from the global, so a change of
        // appearance is picked up without the chat surface holding a stale
        // copy — never `Theme::dark()`, never a field.
        let theme = *Theme::get(cx);
        let transcript_theme = theme;
        let entity = cx.entity();
        let transcript_ranges = self.transcript_entry_ranges();
        let transcript_focus = self.transcript_focus.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .bg(theme.colors.chat_surface)
            .key_context("ChatComposer")
            .track_focus(&self.composer_focus)
            .on_action(cx.listener(Self::send_action))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::copy_transcript))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_key_down(cx.listener(Self::on_composer_key))
            .child(
                div()
                    .id("chat-transcript")
                    .debug_selector(|| "chat-transcript".into())
                    .w(px(TRANSCRIPT_WIDTH))
                    .pt(px(22.0))
                    .flex_1()
                    .flex()
                    .key_context("ChatTranscript")
                    .track_focus(&self.transcript_focus)
                    .on_action(cx.listener(Self::copy_transcript))
                    .on_key_down(cx.listener(Self::on_transcript_key))
                    .child(
                        list(
                            self.list_state.clone(),
                            cx.processor(move |this, entry_index: usize, _window, _cx| {
                                let source_start = transcript_ranges
                                    .get(entry_index)
                                    .map(|range| range.start)
                                    .unwrap_or(0);
                                this.entries
                                    .get(entry_index)
                                    .cloned()
                                    .map(|entry| {
                                        div()
                                            .id(("chat-entry", entry_index))
                                            .w(px(TRANSCRIPT_WIDTH))
                                            .pb(px(8.0))
                                            .child(Chat::render_entry(
                                                entry,
                                                entry_index,
                                                &transcript_theme,
                                                entity.clone(),
                                                transcript_focus.clone(),
                                                source_start,
                                            ))
                                            .into_any_element()
                                    })
                                    .unwrap_or_else(|| div().into_any_element())
                            }),
                        )
                        .with_sizing_behavior(ListSizingBehavior::Auto)
                        .flex_grow_1(),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .pb(px(18.0))
                    .child(self.render_composer(&theme, window, cx))
                    .child(
                        // A quiet context line under the card: the agent's
                        // working directory. Chrome below the fold.
                        div()
                            .mt(px(8.0))
                            .text_size(theme.typography.caption2)
                            .text_color(theme.colors.meta)
                            .child(self.agent_cwd.display().to_string()),
                    ),
            )
    }
}

fn now_hhmm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

/// Human label for a non-`EndTurn` stop reason, stated in the turn footer so
/// an abnormal end never reads as an ordinary completion. The protocol's own
/// Debug strings are "Cancelled", "Refusal", "MaxTokens", "MaxTurnRequests".
fn turn_end_label(reason: &str) -> &'static str {
    match reason {
        "Cancelled" => "cancelled",
        "Refusal" => "refused to continue",
        "MaxTokens" => "stopped at the token limit",
        "MaxTurnRequests" => "stopped at the turn-request limit",
        _ => "ended",
    }
}

fn default_agent_cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir())
}

fn markdown_heading_size(level: u8, typography: tiller_theme::Typography) -> gpui::Pixels {
    match level {
        1 => typography.large_title,
        2 => typography.title,
        3 => typography.title2,
        4 => typography.title3,
        5 => typography.headline,
        _ => typography.callout,
    }
}

#[derive(Default)]
struct InlineBuilder {
    text: String,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    font_overrides: Vec<(Range<usize>, SharedString)>,
    links: Vec<(Range<usize>, String)>,
}

impl InlineBuilder {
    fn append(&mut self, inline: &Inline, theme: &Theme) {
        self.append_with_style(inline, theme, false, false, false, None);
    }

    fn append_with_style(
        &mut self,
        inline: &Inline,
        theme: &Theme,
        strong: bool,
        emphasis: bool,
        code: bool,
        link_target: Option<&str>,
    ) {
        match inline {
            Inline::Text(text) => {
                let start = self.text.len();
                self.text.push_str(text);
                let end = self.text.len();
                if start == end {
                    return;
                }

                let mut highlight = HighlightStyle {
                    font_weight: strong.then_some(FontWeight::BOLD),
                    font_style: emphasis.then_some(FontStyle::Italic),
                    ..Default::default()
                };
                if code {
                    // waku spends its one saturated colour on inline code:
                    // the warm `code_text` on a faint `code_wash` ground.
                    highlight.color = Some(theme.colors.code_text.into());
                    highlight.background_color = Some(theme.colors.code_wash.into());
                    self.font_overrides
                        .push((start..end, theme.typography.code_family.into()));
                }
                if let Some(target) = link_target {
                    highlight.color = Some(theme.colors.accent.into());
                    highlight.underline = Some(UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(theme.colors.accent.into()),
                        wavy: false,
                    });
                    self.links.push((start..end, target.to_string()));
                }
                if highlight != HighlightStyle::default() {
                    self.highlights.push((start..end, highlight));
                }
            }
            Inline::Emphasis(children) => {
                for child in children {
                    self.append_with_style(child, theme, strong, true, code, link_target);
                }
            }
            Inline::Strong(children) => {
                for child in children {
                    self.append_with_style(child, theme, true, emphasis, code, link_target);
                }
            }
            Inline::Code(value) => {
                let start = self.text.len();
                self.text.push_str(value);
                let end = self.text.len();
                if start < end {
                    let mut highlight = HighlightStyle {
                        color: Some(theme.colors.code_text.into()),
                        background_color: Some(theme.colors.code_wash.into()),
                        ..Default::default()
                    };
                    highlight.font_weight = strong.then_some(FontWeight::BOLD);
                    highlight.font_style = emphasis.then_some(FontStyle::Italic);
                    if let Some(target) = link_target {
                        highlight.color = Some(theme.colors.accent.into());
                        self.links.push((start..end, target.to_string()));
                    }
                    self.highlights.push((start..end, highlight));
                    self.font_overrides
                        .push((start..end, theme.typography.code_family.into()));
                }
            }
            Inline::Link {
                target, children, ..
            } => {
                for child in children {
                    self.append_with_style(child, theme, strong, emphasis, code, Some(target));
                }
            }
            Inline::Image { alt, .. } => {
                let image = Inline::Text(alt.clone());
                self.append_with_style(&image, theme, strong, emphasis, code, link_target);
            }
            Inline::SoftBreak | Inline::HardBreak => self.text.push('\n'),
            Inline::Html(html) => {
                let html = Inline::Text(html.clone());
                self.append_with_style(&html, theme, strong, emphasis, code, link_target);
            }
        }
    }
}

fn option_hash(option: &PermissionOption) -> usize {
    option.id.bytes().fold(0usize, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as usize)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    const CHAT_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/chat_fixture.py"
    );

    /// Scratch directory shared between a test and the fixture subprocess;
    /// removed when the test finishes.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("tiller-chat-test-{}-{unique}", std::process::id()));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Pumps both executors — with real sleeps, virtual-clock advances, and
    /// `run_until_parked` — until `condition` holds over the chat's own
    /// state, or the budget is exhausted. A drawn test that skips this loop
    /// can pass on timing luck; every test below goes through it.
    fn pump_chat_until(
        cx: &VisualTestContext,
        chat: &gpui::Entity<Chat>,
        mut condition: impl FnMut(&Chat) -> bool,
    ) {
        cx.cx.executor().allow_parking();
        for _ in 0..600 {
            if chat.read_with(&cx.cx, |chat, _| condition(chat)) {
                return;
            }
            cx.cx
                .executor()
                .advance_clock(std::time::Duration::from_secs(1));
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.cx.run_until_parked();
        }
        let summary = chat.read_with(&cx.cx, |chat, _| {
            let entries = chat
                .entries
                .iter()
                .map(|entry| match entry {
                    Entry::Error { message, .. } => format!("Error({message})"),
                    other => format!("{other:?}"),
                })
                .collect::<Vec<_>>()
                .join(" | ");
            format!(
                "client={} streaming={} connecting={} entries=[{entries}]",
                chat.client.is_some(),
                chat.streaming,
                chat.connecting
            )
        });
        panic!("chat condition never became true within the pump budget: {summary}");
    }

    /// A freshly drawn frame, so `debug_bounds` reads state that actually
    /// rendered rather than the last stale frame.
    fn refresh_frame(cx: &mut VisualTestContext) {
        cx.update(|window, _| window.refresh());
        cx.cx.run_until_parked();
    }

    fn chat_view<'a>(
        cx: &'a mut TestAppContext,
        fixture_args: &[&str],
    ) -> (gpui::Entity<Chat>, &'a mut VisualTestContext) {
        cx.update(Theme::init);
        let args = fixture_args.to_vec();
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut command = AgentCommand::new("python3").arg(CHAT_FIXTURE);
            for arg in &args {
                command = command.arg(*arg);
            }
            Chat::from_test_command(command, std::env::temp_dir(), cx)
        });
        (chat, cx)
    }

    fn focus_and_type(cx: &mut VisualTestContext, text: &str) {
        let composer = cx.debug_bounds("composer").expect("the composer is drawn");
        cx.simulate_click(composer.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input(text);
    }

    /// F-CHAT-37: a fresh chat shows an empty transcript with the composer
    /// and its controls, and typing does not send.
    #[gpui::test]
    async fn empty_chat_renders_composer_and_typing_does_not_send(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        assert!(
            chat.read_with(&cx.cx, |chat, _| {
                chat.entries.is_empty() && chat.list_state.item_count() == 0
            }),
            "before any message there are no transcript items"
        );
        assert!(
            cx.debug_bounds("composer").is_some(),
            "the composer card is drawn"
        );
        assert!(
            cx.debug_bounds("send").is_some(),
            "the send control is drawn"
        );
        assert!(
            cx.debug_bounds("chat-status").is_some(),
            "the status pill is drawn"
        );
        assert!(
            cx.debug_bounds("chat-transcript").is_some(),
            "the transcript surface is drawn"
        );

        focus_and_type(cx, "hello");
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer_text.clone()),
            "hello",
            "typing fills the composer"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.entries.is_empty()),
            "typing alone must not send anything"
        );
    }

    /// F-CHAT-04: Return sends; Shift+Return inserts a newline without
    /// sending. Both halves are driven through the real key-dispatch path in
    /// a drawn frame.
    #[gpui::test]
    async fn enter_sends_and_shift_return_inserts_a_newline(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "first");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert!(
            chat.read_with(&cx.cx, |chat, _| {
                chat.entries
                    .iter()
                    .any(|entry| matches!(entry, Entry::User(text) if text == "first"))
            }),
            "Return sends the composed message"
        );
        pump_chat_until(cx, &chat, |chat| chat.has_completed_turn);

        // A completed turn leaves the composer usable for the Shift+Return
        // half.
        focus_and_type(cx, "line one");
        cx.simulate_keystrokes("shift-return");
        cx.simulate_input("line two");
        cx.run_until_parked();
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer_text.clone()),
            "line one\nline two",
            "Shift+Return inserts a newline into the composer"
        );
        let user_entries = chat.read_with(&cx.cx, |chat, _| {
            chat.entries
                .iter()
                .filter(|entry| matches!(entry, Entry::User(_)))
                .count()
        });
        assert_eq!(
            user_entries, 1,
            "Shift+Return must not send a second message"
        );

        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| {
            chat.entries
                .iter()
                .any(|entry| matches!(entry, Entry::User(text) if text == "line one\nline two"))
                && chat.has_completed_turn
        });
    }

    /// F-CHAT-01 + F-CHAT-23 + F-CHAT-18: one streamed turn renders as a
    /// user pill, an assistant reply that grows in place, a thought, a tool
    /// call card, a live usage update, and the turn footer. The fixture's
    /// go-file gate makes the in-place growth assertion deterministic.
    #[gpui::test]
    async fn a_streamed_reply_grows_one_entry_with_thought_tool_and_usage(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let fixture_dir = dir.0.to_str().expect("fixture dir is utf-8").to_string();
        let (chat, cx) = chat_view(cx, &["staged", &fixture_dir]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "hello");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        // Chunk one arrives and the fixture blocks on the go-file, so the
        // transcript must be mid-growth here: one Assistant entry holding
        // "first " and nothing after it, with the turn still streaming.
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.len() == 2
                && matches!(
                    chat.entries.last(),
                    Some(Entry::Assistant { text, .. }) if text == "first "
                )
        });
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.streaming),
            "Enter starts a streaming turn that stays active until the agent ends it"
        );
        let assistant_count = chat.read_with(&cx.cx, |chat, _| {
            chat.entries
                .iter()
                .filter(|entry| matches!(entry, Entry::Assistant { .. }))
                .count()
        });
        assert_eq!(
            assistant_count, 1,
            "the reply grows one entry in place, not one entry per chunk"
        );

        // Release the rest of the turn.
        std::fs::write(dir.0.join("go"), "go").expect("write go file");
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.len() == 5 && matches!(chat.entries.last(), Some(Entry::TurnFooter(_)))
        });

        let (assistant, thought, tool, usage, completed) = chat.read_with(&cx.cx, |chat, _| {
            let assistant = chat
                .entries
                .iter()
                .find_map(|entry| match entry {
                    Entry::Assistant { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .expect("assistant reply");
            let thought = chat
                .entries
                .iter()
                .find_map(|entry| match entry {
                    Entry::Thought(text) => Some(text.clone()),
                    _ => None,
                })
                .expect("thought chunk");
            let tool = chat
                .entries
                .iter()
                .find_map(|entry| match entry {
                    Entry::ToolCall { title, status, .. } => Some((title.clone(), status.clone())),
                    _ => None,
                })
                .expect("tool call card");
            (
                assistant,
                thought,
                tool,
                chat.context_usage.clone(),
                chat.has_completed_turn,
            )
        });
        assert_eq!(
            assistant, "first streamed",
            "both chunks land in the same growing entry"
        );
        assert_eq!(thought, "thinking hard");
        assert_eq!(tool, ("write nonce".into(), "Completed".into()));
        assert_eq!(
            usage,
            Some(ContextUsage {
                used: 53_000,
                size: 200_000,
                cost: None
            })
        );
        assert!(completed, "the turn completes with a footer");

        // The drawn transcript laid out the footer and the composer is back
        // to its post-turn state.
        refresh_frame(cx);
        assert!(
            chat.read_with(&cx.cx, |chat, _| {
                chat.list_state.bounds_for_item(4).is_some()
            }),
            "the turn footer is laid out in the drawn transcript"
        );
        assert!(cx.debug_bounds("chat-status").is_some());
    }

    /// F-CHAT-25 (option answers): a permission prompt renders both option
    /// buttons in the drawn frame and each answer is recorded on the card.
    #[gpui::test]
    async fn a_permission_prompt_answers_both_ways(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["permission"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        let send = |cx: &mut VisualTestContext, text: &str| {
            focus_and_type(cx, text);
            cx.simulate_keystrokes("enter");
            cx.run_until_parked();
        };

        // First prompt: answer Allow.
        send(cx, "may I?");
        pump_chat_until(cx, &chat, |chat| {
            chat.entries
                .iter()
                .any(|entry| matches!(entry, Entry::Permission { resolved: None, .. }))
        });
        assert!(
            !chat.read_with(&cx.cx, |chat, _| chat.can_send()),
            "the composer cannot send while a permission is pending (F-CHAT-05)"
        );
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("permission-option-allow").is_some()
                && cx.debug_bounds("permission-option-deny").is_some(),
            "both permission answers are drawn as buttons"
        );
        let allow = cx
            .debug_bounds("permission-option-allow")
            .expect("allow button");
        cx.simulate_click(allow.center(), Modifiers::none());
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(entry, Entry::Permission { resolved: Some(choice), .. } if choice == "Allow once")
            }) && chat.has_completed_turn
        });

        // Second prompt: answer Deny against the same connection.
        send(cx, "second?");
        pump_chat_until(cx, &chat, |chat| {
            chat.entries
                .iter()
                .any(|entry| matches!(entry, Entry::Permission { resolved: None, .. }))
        });
        refresh_frame(cx);
        let deny = cx
            .debug_bounds("permission-option-deny")
            .expect("deny button");
        cx.simulate_click(deny.center(), Modifiers::none());
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(entry, Entry::Permission { resolved: Some(choice), .. } if choice == "Deny once")
            })
        });
        let (answered, footers) = chat.read_with(&cx.cx, |chat, _| {
            let answered = chat
                .entries
                .iter()
                .filter_map(|entry| match entry {
                    Entry::Permission {
                        resolved: Some(choice),
                        ..
                    } => Some(choice.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let footers = chat
                .entries
                .iter()
                .filter(|entry| matches!(entry, Entry::TurnFooter(_)))
                .count();
            (answered, footers)
        });
        assert_eq!(answered, vec!["Allow once", "Deny once"]);
        assert_eq!(footers, 2, "both answered turns complete");
    }

    /// F-CHAT-03 + the stream-death seam: a transport that dies mid-reply
    /// leaves a stated error card with a working Retry, and Retry reconnects
    /// and completes a later turn.
    #[gpui::test]
    async fn a_stream_that_dies_mid_reply_states_the_error_and_retry_recovers(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        let fixture_dir = dir.0.to_str().expect("fixture dir is utf-8").to_string();
        let (chat, cx) = chat_view(cx, &["death-then-ok", &fixture_dir]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "hello");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        // The fixture streams "partial" and dies; the transcript must state
        // the death instead of looking like a normal empty reply.
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(
                    entry,
                    Entry::Error {
                        retryable: true,
                        kind: ErrorKind::Connection,
                        ..
                    }
                )
            }) && !chat.streaming
        });
        assert!(
            chat.read_with(&cx.cx, |chat, _| {
                chat.entries.iter().any(
                    |entry| matches!(entry, Entry::Assistant { text, .. } if text == "partial "),
                )
            }),
            "what arrived before the death stays in the transcript"
        );
        refresh_frame(cx);
        let retry = cx
            .debug_bounds("chat-retry")
            .expect("the drawn error card offers Retry");

        // Retry relaunches the agent; the second fixture invocation behaves,
        // so the connection error card is cleared and a later turn completes.
        cx.simulate_click(retry.center(), Modifiers::none());
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.client.is_some()
                && !chat.entries.iter().any(|entry| {
                    matches!(
                        entry,
                        Entry::Error {
                            kind: ErrorKind::Connection,
                            ..
                        }
                    )
                })
        });
        assert!(
            !chat.read_with(&cx.cx, |chat, _| chat.connecting),
            "a recovered chat is no longer connecting"
        );

        focus_and_type(cx, "again");
        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| {
            chat.entries
                .iter()
                .any(|entry| matches!(entry, Entry::Assistant { text, .. } if text == "alive "))
                && chat.has_completed_turn
        });
        assert!(
            chat.read_with(&cx.cx, |chat, _| {
                chat.entries
                    .iter()
                    .any(|entry| matches!(entry, Entry::User(text) if text == "again"))
            }),
            "the recovered connection takes a new turn"
        );
    }

    /// F-CHAT-07 + the cancelled-request seam: Escape stops a streaming turn
    /// and the transcript footer states the cancellation instead of looking
    /// like an ordinary completion. A later, normally-ended turn stays plain.
    #[gpui::test]
    async fn escape_cancels_the_stream_and_the_transcript_states_it(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["cancel"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "hello");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.streaming
                && chat.entries.iter().any(
                    |entry| matches!(entry, Entry::Assistant { text, .. } if text == "partial "),
                )
        });

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(
            !chat.read_with(&cx.cx, |chat, _| chat.streaming),
            "Escape immediately stops the streaming turn"
        );

        // The agent answers the cancelled prompt; the footer states it.
        pump_chat_until(cx, &chat, |chat| {
            chat.entries
                .iter()
                .any(|entry| matches!(entry, Entry::TurnFooter(text) if text.contains("cancelled")))
        });
        assert!(
            chat.read_with(&cx.cx, |chat, _| {
                chat.entries.iter().any(
                    |entry| matches!(entry, Entry::Assistant { text, .. } if text == "partial "),
                )
            }),
            "the partial reply stays visible under the cancelled footer"
        );

        // The composer is usable again; a normal next turn is not marked.
        focus_and_type(cx, "again");
        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(
                |entry| matches!(entry, Entry::TurnFooter(text) if !text.contains("cancelled")),
            )
        });
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.has_completed_turn),
            "a later turn completes normally"
        );
    }

    fn configure_test_chat(chat: &mut Chat) {
        chat.has_completed_turn = true;
        chat.available_models = vec![ModelOption {
            id: "opus".into(),
            name: "Opus".into(),
            description: Some("Highest quality".into()),
        }];
        chat.model_config_id = Some("model".into());
        chat.selected_model = Some("default".into());
        chat.context_usage = Some(ContextUsage {
            used: 25,
            size: 100,
            cost: None,
        });
    }

    #[test]
    fn default_agent_cwd_follows_the_process_workspace() {
        assert_eq!(default_agent_cwd(), std::env::current_dir().unwrap());
    }

    #[gpui::test]
    async fn model_picker_selects_an_agent_advertised_model_and_escape_dismisses(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::from_test_command(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            configure_test_chat(&mut chat);
            chat
        });
        cx.update(|window, _| window.refresh());

        let chip = cx
            .debug_bounds("model-chip")
            .expect("model chip is rendered");
        cx.simulate_click(chip.center(), Modifiers::none());
        cx.run_until_parked();
        let option = cx
            .debug_bounds("model-option-opus")
            .expect("agent model option is rendered");
        cx.simulate_click(option.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("model-picker").is_none());
        assert!(cx.debug_bounds("model-selection-opus").is_some());

        cx.simulate_click(chip.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("model-picker").is_some());
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(cx.debug_bounds("model-picker").is_none());

        cx.simulate_click(chip.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("model-picker").is_some());
        cx.simulate_click(point(px(10.0), px(10.0)), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("model-picker").is_none());
    }

    #[gpui::test]
    async fn context_ring_shows_reported_usage_and_escape_dismisses_popover(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::from_test_command(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            configure_test_chat(&mut chat);
            chat
        });
        cx.update(|window, _| window.refresh());

        let ring = cx
            .debug_bounds("context-ring")
            .expect("context ring is rendered");
        assert!(cx.debug_bounds("context-ring-progress").is_some());
        cx.simulate_click(ring.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("context-popover").is_some());
        assert!(cx.debug_bounds("context-usage-25-of-100").is_some());
        let context_focus = chat.read_with(cx, |chat, _| chat.context_popover_focus.clone());
        let context_focus_is_active = cx.update(|window, app| {
            window
                .focused(app)
                .is_some_and(|focused| focused == context_focus)
        });
        assert!(context_focus_is_active, "context popover owns focus");
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(cx.debug_bounds("context-popover").is_none());

        cx.simulate_click(ring.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("context-popover").is_some());
        cx.simulate_click(point(px(10.0), px(10.0)), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("context-popover").is_none());
    }

    #[gpui::test]
    async fn transcript_only_lays_out_rows_near_the_viewport(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            for index in 0..100 {
                chat.push_entry(Entry::User(format!("Transcript entry {index}")));
            }
            chat
        });
        cx.update(|window, _| window.refresh());

        let (entry_count, first_bounds, last_bounds) = chat.read_with(cx, |chat, _| {
            (
                chat.list_state.item_count(),
                chat.list_state.bounds_for_item(0),
                chat.list_state.bounds_for_item(99),
            )
        });
        assert_eq!(entry_count, 100);
        assert!(
            first_bounds.is_none(),
            "rows outside the viewport should not be laid out"
        );
        assert!(last_bounds.is_some(), "the tail should be laid out");
    }

    #[gpui::test]
    async fn transcript_selection_copies_across_entries_from_global_offsets(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::User("user question".into()));
            chat.push_entry(Entry::Assistant {
                text: "assistant answer".into(),
                document: parse("assistant answer"),
            });
            chat
        });

        let (start, end, expected) = chat.read_with(cx, |chat, _| {
            let ranges = chat.transcript_entry_ranges();
            let start = ranges[0].start + "user ".len();
            let end = ranges[1].start + "assistant answer".len();
            (start, end, chat.transcript_text()[start..end].to_string())
        });
        chat.update(cx, |chat, _| {
            chat.transcript_selection = Some(TranscriptSelection {
                anchor: start,
                head: end,
            });
            assert_eq!(
                chat.selected_transcript_text().as_deref(),
                Some(expected.as_str())
            );
        });
    }

    #[gpui::test]
    async fn a_retained_transcript_can_be_restored_as_visible_chat_history(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::User("question".into()));
            chat.push_entry(Entry::Assistant {
                text: "answer".into(),
                document: parse("answer"),
            });
            chat
        });
        let transcript = chat.read_with(cx, |chat, _| chat.transcript_for_resume());

        let (restored, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            chat.restore_transcript(&transcript, cx);
            chat
        });
        assert_eq!(
            restored.read_with(cx, |chat, _| chat.transcript_for_resume()),
            transcript
        );
    }

    #[gpui::test]
    async fn failed_launch_can_retry_and_complete(cx: &mut TestAppContext) {
        let cwd = std::env::temp_dir();
        let chat = cx.new(|cx| {
            Chat::from_test_command(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                cwd.clone(),
                cx,
            )
        });

        // ACP owns a real worker thread and subprocess; permit its wakeups to
        // cross the deterministic test scheduler boundary.
        cx.executor().allow_parking();
        cx.run_until_parked();
        chat.update(cx, |chat, _| {
            chat.composer_text = "retry".into();
            chat.composer_cursor = chat.composer_text.len();
        });
        chat.read_with(cx, |chat, _| {
            assert!(chat.can_send(), "a failed launch must leave Send usable");
            assert!(chat.entries.iter().any(|entry| {
                matches!(
                    entry,
                    Entry::Error {
                        retryable: true,
                        ..
                    }
                )
            }));
        });

        chat.update(cx, |chat, cx| {
            chat.agent_command = AgentCommand::new("/bin/sh").args([
                "-c",
                r#"while IFS= read -r line; do id=$(printf '%s' "$line" | sed -E 's/.*"id":([^,]+),.*/\1/'); case "$line" in *initialize*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"protocolVersion":1,"agentCapabilities":{},"authMethods":[]}}' ;; *session/new*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"sessionId":"test"}}' ;; *session/prompt*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"stopReason":"end_turn"}}' ;; esac; done"#,
            ]);
            chat.composer_text = "hello".into();
            chat.composer_cursor = chat.composer_text.len();
            chat.send(cx);
        });

        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(25));
            cx.run_until_parked();
        }
        chat.read_with(cx, |chat, _| {
            assert!(chat.client.is_some(), "retry should establish a client");
            assert!(
                !chat
                    .entries
                    .iter()
                    .any(|entry| matches!(entry, Entry::Error { .. })),
                "a recovered connection must not retain its startup error"
            );
            assert!(
                chat.entries
                    .iter()
                    .any(|entry| { matches!(entry, Entry::User(text) if text == "hello") })
            );
            assert!(chat.has_completed_turn, "retry prompt should complete");
        });
    }

    #[gpui::test]
    async fn connecting_state_renders_while_startup_is_in_flight(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let (_, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            chat.connecting = true;
            chat
        });
        cx.update(|window, _| window.refresh());
        assert!(cx.debug_bounds("chat-connecting").is_some());
    }
}
