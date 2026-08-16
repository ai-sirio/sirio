//! The chat transcript and composer, driven by real ACP events.
//!
//! This first Rust implementation owns a linear transcript and one live ACP
//! session, same scope as `Sidebar`'s fixture model: the connection lifecycle
//! and view model live here so the transcript renderer stays deterministic.

use gpui::{
    AnyElement, App, BorderStyle, Bounds, ClipboardItem, Context, CursorStyle, DispatchPhase,
    Edges, Element, ElementId, Entity, EventEmitter, ExternalPaths, FocusHandle, Focusable,
    FollowMode, FontStyle, FontWeight, GlobalElementId, HighlightStyle, Hitbox, HitboxBehavior,
    InspectorElementId, InteractiveText, KeyBinding, KeyDownEvent, LayoutId, ListAlignment,
    ListSizingBehavior, ListState, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PathBuilder, Pixels, Rgba, SharedString, StyledText, Task, UnderlineStyle, Window, actions,
    canvas, div, list, point, prelude::*, px, quad, rgb, transparent_black,
};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tiller_acp::{
    AcpClient, AcpEvent, AgentCommand, AgentMode, AvailableCommandInfo, ContextUsage, EffortOption,
    ImageAttachment, ModeCatalog, ModelCatalog, ModelOption, ToolCallContentInfo, ToolCallDiff,
    ToolCallLocationInfo,
};
use tiller_git::{GitActions, status as git_status};
use tiller_markdown::{Alignment, Block, Document, Inline, ListItem, ListKind, parse};
use tiller_persistence::{
    AppDatabase, ChatEntry, ChatPermissionOption, ChatPermissionOutcome, ChatPlanEntry,
    ChatSessionSummary, ChatTranscript, ChatTurn,
};
use tiller_theme::Theme;

use crate::composer::{Composer, ComposerChip, ComposerPart};
use crate::editor::Language;
use crate::file_view::{CodeSpanKind, code_spans};
use crate::sidebar::icons::{Icon, IconElement};

/// F-CORE-FILE-04: overrides a rendered Markdown link's click, used by
/// callers (File Preview) that want to try resolving the link as a local
/// file before falling back to opening it externally. `None` keeps the
/// default of every link opening via `cx.open_url`, which is right for
/// assistant-authored chat prose.
pub(crate) type LinkClickOverride = Rc<dyn Fn(&str, &mut Window, &mut App)>;

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

actions!(chat_question_answer, [SendAnswer, CancelAnswer]);

/// One option rendered on a question or plan-approval card.
#[derive(Clone, Debug, PartialEq, Eq)]
struct AnswerOption {
    /// Protocol option id sent back to the agent.
    id: String,
    /// Human-readable label.
    label: String,
    /// Whether the option declines the request, tinted distinctly.
    is_rejection: bool,
}

/// Free-text input offered on a question card.
#[derive(Clone, Debug, PartialEq, Eq)]
struct AnswerTextInput {
    /// Placeholder shown in the empty field.
    placeholder: Option<String>,
    /// Text prefilled into the field.
    prefill: Option<String>,
}

impl AnswerTextInput {
    fn placeholder(&self) -> String {
        self.placeholder
            .clone()
            .unwrap_or_else(|| "Type an answer".into())
    }
}

/// One row of the agent's plan, as rendered on the Plan card.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PlanEntryRow {
    /// Human-readable description of the task.
    content: String,
    /// Wire status: `pending`, `in_progress`, or `completed`.
    status: String,
}

/// A pending approval attached to the Plan card (F-CHAT-24).
#[derive(Clone, Debug, PartialEq, Eq)]
struct PlanApproval {
    /// Handle passed to [`AcpClient::respond_permission`].
    request_id: u64,
    /// Title of the tool asking for approval.
    title: String,
    /// Offered choices.
    options: Vec<AnswerOption>,
    /// Chosen option label, once answered.
    resolved: Option<String>,
    /// No longer answerable.
    expired: bool,
}

/// F-CHAT-25: the live state of the question-card answer field — which
/// request's field owns focus, and its draft text. One field can be focused
/// at a time, so the state is a single slot rather than a map.
#[derive(Clone, Debug, PartialEq, Eq)]
struct QuestionAnswerState {
    /// Focus handle of the answer field.
    focus: FocusHandle,
    /// Draft text of the focused answer field.
    draft: String,
    /// The request whose answer field currently holds focus.
    for_request: Option<u64>,
}

/// A tool call nested inside a subagent task card.
#[derive(Clone, Debug)]
struct SubagentToolCall {
    id: String,
    title: String,
    status: String,
    kind: String,
    content: Vec<ToolCallContentInfo>,
    locations: Vec<ToolCallLocationInfo>,
    expanded: bool,
}

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
    ///
    /// `expanded` starts `false` (F-CHAT-21): thinking renders collapsed to
    /// a one-line summary until the reader opts in, live or historical.
    Thought { text: String, expanded: bool },
    /// A tool call, tracked by protocol id so later updates can patch it.
    ///
    /// `kind`, `content`, `locations`, `raw_input` and `raw_output` are the
    /// protocol's widened tool-call surface (F-CHAT-23/-31/-32) — a diff or
    /// text result, the files touched, and the raw input/output the agent
    /// reported, when it reported them. `expanded` starts `false`, same as
    /// `Thought` (F-CHAT-21): the card renders as one line until the reader
    /// opts in.
    ///
    /// `group_expanded` (F-CHAT-22) is read only on the *last* entry of a
    /// consecutive run of tool calls — that is the entry the transcript
    /// renders the "N steps" toggle against, since a run has no separate
    /// grouping record of its own. It is meaningless, and ignored, on any
    /// entry that is not currently a run's tail.
    ToolCall {
        id: String,
        title: String,
        status: String,
        kind: String,
        content: Vec<ToolCallContentInfo>,
        locations: Vec<ToolCallLocationInfo>,
        raw_input: Option<String>,
        raw_output: Option<String>,
        expanded: bool,
        group_expanded: bool,
    },
    /// A Task/dispatch tool call whose following live calls are presented as
    /// the child agent's work. ACP v1/v2 carry no parent-child relation, so
    /// this is deliberately a conservative presentation grouping based on
    /// the parent tool's title/input and its in-progress lifetime.
    SubagentTask {
        id: String,
        title: String,
        status: String,
        tool_calls: Vec<SubagentToolCall>,
        expanded: bool,
    },
    /// A question the agent put to the user, answered in place (F-CHAT-25).
    /// `resolved` is the recorded answer once one is chosen or typed;
    /// `expired` means the turn ended and the card is no longer answerable.
    Permission {
        request_id: u64,
        /// Tool title naming the asker.
        title: String,
        /// Structured question body; empty for plain permission gates.
        prompt: String,
        options: Vec<AnswerOption>,
        /// Free-text input when the question offers one.
        text_input: Option<AnswerTextInput>,
        resolved: Option<String>,
        expired: bool,
        /// The user dismissed an unrenderable request; distinct from denial.
        dismissed: bool,
    },
    /// The agent's execution plan (F-CHAT-24). Replaced in place as entries
    /// advance; a pending approval attaches its option buttons to the card.
    Plan {
        entries: Vec<PlanEntryRow>,
        approval: Option<PlanApproval>,
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

/// A socket-safe projection of the one chat entity that GPUI renders.
///
/// The shell owns protocol encoding; this type keeps the UI independent of
/// the control transport while making the rendered state observable there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatControlSnapshot {
    pub status: String,
    pub composer_text: String,
    pub queued_text: String,
    pub transcript: Vec<BTreeMap<String, String>>,
}

impl Entry {
    fn plain_text(&self) -> String {
        match self {
            Self::User(text) => text.clone(),
            Self::Thought { text, .. } => text.clone(),
            Self::Assistant { document, .. } => document.plain_text(),
            Self::ToolCall {
                title,
                status,
                content,
                locations,
                ..
            } => {
                let mut lines = vec![format!("{title}\n{status}")];
                for item in content {
                    match item {
                        ToolCallContentInfo::Text(text) => lines.push(text.clone()),
                        ToolCallContentInfo::Diff(diff) => {
                            lines.push(format!("diff: {}", diff.path.to_string_lossy()))
                        }
                        ToolCallContentInfo::Other => {}
                    }
                }
                for location in locations {
                    lines.push(location.path.to_string_lossy().into_owned());
                }
                lines.join("\n")
            }
            Self::SubagentTask {
                title,
                status,
                tool_calls,
                ..
            } => {
                let mut lines = vec![format!("Subagent: {title}\n{status}")];
                lines.extend(
                    tool_calls
                        .iter()
                        .map(|call| format!("{}\n{}", call.title, call.status)),
                );
                lines.join("\n")
            }
            Self::Permission {
                options, resolved, ..
            } => resolved.clone().unwrap_or_else(|| {
                options
                    .iter()
                    .map(|option| option.label.as_str())
                    .collect::<Vec<_>>()
                    .join(" / ")
            }),
            Self::Plan { entries, .. } => entries
                .iter()
                .map(|entry| format!("{} · {}", entry.status, entry.content))
                .collect::<Vec<_>>()
                .join("\n"),
            Self::TurnFooter(text) => text.clone(),
            Self::Error { message, .. } => message.clone(),
        }
    }
}

fn persisted_entry(entry: &Entry) -> Option<ChatEntry> {
    match entry {
        Entry::User(text) => Some(ChatEntry::UserMessage { text: text.clone() }),
        Entry::Assistant { text, .. } => Some(ChatEntry::AssistantMessage { text: text.clone() }),
        Entry::Thought { text, .. } => Some(ChatEntry::Thought { text: text.clone() }),
        Entry::ToolCall {
            id, title, status, ..
        } => Some(ChatEntry::ToolCall {
            id: id.clone(),
            title: title.clone(),
            status: status.clone(),
        }),
        Entry::SubagentTask {
            id, title, status, ..
        } => Some(ChatEntry::ToolCall {
            id: id.clone(),
            title: title.clone(),
            status: status.clone(),
        }),
        Entry::Permission {
            request_id,
            title,
            options,
            resolved,
            expired,
            dismissed,
            ..
        } => {
            let options = options
                .iter()
                .map(|option| ChatPermissionOption {
                    id: option.id.clone(),
                    name: option.label.clone(),
                    kind: if option.is_rejection {
                        "reject".into()
                    } else {
                        "allow".into()
                    },
                })
                .collect::<Vec<_>>();
            let outcome = if *dismissed {
                ChatPermissionOutcome::Cancelled
            } else {
                match (resolved, expired) {
                    (Some(label), _) => ChatPermissionOutcome::Selected {
                        option_id: options
                            .iter()
                            .find(|option| option.name == *label)
                            .map(|option| option.id.clone())
                            .unwrap_or_default(),
                        label: label.clone(),
                    },
                    (None, true) => ChatPermissionOutcome::Expired,
                    (None, false) => ChatPermissionOutcome::Pending,
                }
            };
            Some(ChatEntry::Permission {
                request_id: *request_id,
                title: title.clone(),
                options,
                outcome,
            })
        }
        Entry::Plan { entries, .. } => Some(ChatEntry::Plan {
            entries: entries
                .iter()
                .map(|entry| ChatPlanEntry {
                    content: entry.content.clone(),
                    status: entry.status.clone(),
                })
                .collect(),
        }),
        Entry::TurnFooter(text) => Some(ChatEntry::TurnFooter { text: text.clone() }),
        Entry::Error {
            message, retryable, ..
        } => Some(ChatEntry::Error {
            message: message.clone(),
            retryable: *retryable,
        }),
    }
}

fn is_terminal_tool_status(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "completed" | "failed" | "cancelled" | "canceled"
    )
}

/// ACP does not identify subagents or parent tool calls. These are the
/// observable names used by the installed ACP adapters; the raw-input check
/// is only a second signal for adapters that title the call generically.
fn is_subagent_tool_call(title: &str, raw_input: Option<&str>) -> bool {
    let title = title.trim().to_ascii_lowercase();
    title == "task"
        || title.contains("subagent")
        || title.contains("dispatch")
        || title.contains("spawn")
        || raw_input.is_some_and(|input| {
            let input = input.to_ascii_lowercase();
            input.contains("\"subagent_type\"") || input.contains("\"agent_type\"")
        })
}

/// Revert the live worktree change for one ACP-reported path. ACP adapters
/// may report a path relative to their cwd or absolute beneath it; git's
/// status model uses the relative spelling, so normalize at the seam.
fn discard_edited_path(repo: &Path, reported_path: &Path) -> Result<(), String> {
    let path = reported_path.strip_prefix(repo).unwrap_or(reported_path);
    let snapshot = git_status(repo).map_err(|error| error.to_string())?;
    let entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == path)
        .cloned()
        .ok_or_else(|| format!("No changes to revert for {}.", path.display()))?;
    if entry.is_untracked() {
        GitActions::discard_untracked(repo, &[entry]).map_err(|error| error.to_string())
    } else {
        GitActions::discard_changes(repo, &[entry]).map_err(|error| error.to_string())
    }
}

fn restored_entry(entry: ChatEntry) -> Entry {
    match entry {
        ChatEntry::UserMessage { text } => Entry::User(text),
        ChatEntry::AssistantMessage { text } => Entry::Assistant {
            document: parse(&text),
            text,
        },
        ChatEntry::Thought { text } => Entry::Thought {
            text,
            expanded: false,
        },
        ChatEntry::ToolCall { id, title, status } => Entry::ToolCall {
            id,
            title,
            status,
            kind: "tool".into(),
            content: Vec::new(),
            locations: Vec::new(),
            raw_input: None,
            raw_output: None,
            expanded: false,
            group_expanded: false,
        },
        ChatEntry::Permission {
            request_id,
            title,
            options,
            outcome,
        } => {
            let expired = matches!(
                &outcome,
                ChatPermissionOutcome::Cancelled
                    | ChatPermissionOutcome::TimedOut
                    | ChatPermissionOutcome::Expired
            );
            let dismissed = matches!(&outcome, ChatPermissionOutcome::Cancelled);
            let resolved = match outcome {
                ChatPermissionOutcome::Selected { label, .. } => Some(label),
                ChatPermissionOutcome::Cancelled => Some("Cancelled".into()),
                ChatPermissionOutcome::TimedOut => Some("Timed out".into()),
                ChatPermissionOutcome::Expired => Some("Expired".into()),
                ChatPermissionOutcome::Pending => None,
            };
            Entry::Permission {
                request_id,
                title,
                prompt: String::new(),
                options: options
                    .into_iter()
                    .map(|option| AnswerOption {
                        is_rejection: option.kind == "reject",
                        id: option.id,
                        label: option.name,
                    })
                    .collect(),
                text_input: None,
                resolved,
                expired,
                dismissed,
            }
        }
        ChatEntry::Plan { entries } => Entry::Plan {
            entries: entries
                .into_iter()
                .map(|entry| PlanEntryRow {
                    content: entry.content,
                    status: entry.status,
                })
                .collect(),
            approval: None,
        },
        ChatEntry::TurnFooter { text } => Entry::TurnFooter(text),
        ChatEntry::Error { message, retryable } => Entry::Error {
            message,
            retryable,
            kind: ErrorKind::Connection,
        },
    }
}

/// Whether an error describes the live connection or belongs permanently in
/// the transcript. Connection errors are removed when a later retry recovers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ErrorKind {
    Connection,
    /// The agent rejected launch or a mid-session request with ACP's
    /// `auth_required` error (F-CHAT-02) — distinct from a generic
    /// connection failure because the fix isn't "retry", it's "sign in out
    /// of band, then retry": the agent has already exited by the time this
    /// reaches the caller, so there is no live prompt to authenticate on.
    AuthRequired,
    /// An MCP-configuration-flavored stderr line observed during the turn
    /// (F-CHAT-33) — informational, not a transport failure: the session is
    /// still live, so this is never retryable and never clears the client.
    McpWarning,
}

/// A mid-session `TransportError` never carries the typed `AcpError` that
/// caused it — the ACP layer folds it to a plain `String` before it crosses
/// the crate boundary (`AcpEvent::TransportError`'s own doc comment: "a
/// mid-session request rejected with ACP's `auth_required` error... arrives
/// here rather than as its own variant... `AcpError::AuthRequired`'s
/// `Display` renders the same auth-guidance text `AcpClient::launch` uses
/// for a startup failure, so the message string alone still carries it").
/// This is the one place both the launch-failure and mid-session paths
/// converge on that string convention to recover the distinction
/// (F-CHAT-02): a dedicated banner and CLI-login guidance instead of the
/// generic "retry the connection" card.
fn classify_connection_error(message: String) -> (String, ErrorKind) {
    if message.contains("requires authentication") {
        (
            format!(
                "{message}\n\nSign in from a terminal using this agent's own CLI \
                 (for example, its `login` subcommand), then Retry — Tiller cannot \
                 complete authentication on the agent's behalf."
            ),
            ErrorKind::AuthRequired,
        )
    } else {
        (message, ErrorKind::Connection)
    }
}

struct ChatPersistence {
    database_path: PathBuf,
    tab_id: String,
    /// F-CHAT-34: the worktree this tab's transcripts are scoped to, so the
    /// Chat History menu can list every past chat in the worktree, not just
    /// this one tab's own transcript.
    worktree_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TranscriptSelection {
    anchor: usize,
    head: usize,
}

/// The one copy affordance currently showing its short-lived confirmation.
/// A target, rather than a boolean, ensures one response's acknowledgement
/// never leaks onto another response or a nested code block.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CopyTarget {
    Assistant(usize),
    CodeBlock { entry: usize, block: String },
}

/// A host-owned action requested by an edit-summary card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatEvent {
    OpenFile(PathBuf),
    /// F-BRW-09: a plain click on an HTTP(S) link in the transcript. The
    /// workspace owns tab creation, so it decides whether to open Tiller's
    /// internal browser tab; the human Cmd+Shift bypass to the system
    /// browser is handled locally (see [`TranscriptSelectableText`]'s mouse
    /// handler) and never reaches this event.
    OpenLink(String),
}

/// Per-tool-call state for the post-turn edited-files summary (F-CHAT-32).
#[derive(Clone, Debug, Default)]
struct EditSummaryState {
    confirming_path: Option<PathBuf>,
    reverted_paths: Vec<PathBuf>,
    revert_error: Option<String>,
    reverting_path: Option<PathBuf>,
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
    /// The owning assistant entry, if this markdown is part of the chat
    /// transcript rather than the read-only file preview renderer.
    entry_index: Option<usize>,
    copied_target: Option<CopyTarget>,
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
                        // F-BRW-09: the human Cmd+Shift chord bypasses
                        // Tiller's internal browser and opens the link in
                        // the system browser directly; a plain click routes
                        // through ChatEvent::OpenLink so the workspace can
                        // open it in an internal browser tab instead.
                        if event.modifiers.platform && event.modifiers.shift {
                            cx.open_url(target);
                        } else {
                            let target = target.clone();
                            interaction.chat.update(cx, |_, cx| {
                                cx.emit(ChatEvent::OpenLink(target));
                            });
                        }
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

/// Converts an adapter's ACP program description into the concrete
/// [`AgentCommand`] the chat spawns.
///
/// The conversion is mechanical on purpose: the per-adapter mapping lives
/// in `tiller_agents` (each adapter answers its own ACP server), so a
/// Codex tab launches Codex's server and a Claude tab Claude's — never a
/// silently-downgraded default.
pub fn acp_agent_command(program: tiller_agents::AcpProgram) -> AgentCommand {
    AgentCommand::new(program.program).args(program.args.iter().copied())
}

/// A chat surface wired to one live [`AcpClient`] session.
pub struct Chat {
    client: Option<AcpClient>,
    agent_command: AgentCommand,
    agent_cwd: PathBuf,
    entries: Vec<Entry>,
    composer: Composer,
    composer_focus: FocusHandle,
    /// F-CHAT-25: the question answer field (focus, draft, owner request).
    question_answer: QuestionAnswerState,
    streaming: bool,
    /// D-CHAT-03: the draft committed (Enter) while a turn streams, to be
    /// sent as the next user turn when the turn ends — one slot, latest
    /// commit wins. `None` when nothing is queued.
    queued_item: Option<String>,
    connecting: bool,
    has_completed_turn: bool,
    /// F-CHAT-33: how many of `AcpClient::mcp_warnings()` have already been
    /// surfaced as transcript entries. `mcp_warnings()` returns the whole
    /// running list each call (it doesn't drain), so this is the cursor
    /// that keeps a warning from being re-posted on every later turn.
    mcp_warnings_shown: usize,
    available_models: Vec<ModelOption>,
    model_config_id: Option<String>,
    selected_model: Option<String>,
    model_picker_open: bool,
    /// F-CHAT-16: the model picker's own search query, reset each time the
    /// picker opens. Matches `ModelPickerFilter`'s Swift semantics — trimmed,
    /// case-insensitive substring match against name/id/description, order
    /// preserved, empty query keeps every model.
    model_search: String,
    /// F-CHAT-15: the session-mode selector (ask/plan/auto, entirely
    /// agent-defined), re-read from `AcpClient::mode_catalog` on connect and
    /// after every event since the wire only pushes mode changes as an
    /// untyped `CurrentModeUpdate` the ACP layer folds into that live cell
    /// rather than a discrete event. `None` when the agent never advertised
    /// `modes` — most ACP agents today don't, so the pill stays a plain
    /// status readout exactly as before this row.
    mode_catalog: Option<ModeCatalog>,
    mode_picker_open: bool,
    context_popover_open: bool,
    model_picker_focus: FocusHandle,
    mode_picker_focus: FocusHandle,
    context_popover_focus: FocusHandle,
    transcript_focus: FocusHandle,
    context_usage: Option<ContextUsage>,
    list_state: ListState,
    transcript_selection: Option<TranscriptSelection>,
    transcript_dragging: bool,
    copied_target: Option<CopyTarget>,
    edit_summaries: BTreeMap<usize, EditSummaryState>,
    persistence: Option<ChatPersistence>,
    _event_task: Option<Task<()>>,
    // --- Composer popups and attachments (F-CHAT-09/10/11/12/14/17/19) ---
    /// Slash commands advertised by the agent over ACP.
    available_commands: Vec<AvailableCommandInfo>,
    /// The slash popup was dismissed for the current `/token`.
    slash_dismissed: bool,
    /// The `/token` the popup state (dismissal, selection) belongs to; any
    /// change resets both.
    last_slash_token: Option<String>,
    /// Keyboard selection index into the slash candidates.
    slash_selected: usize,
    /// File-mention candidates for the current `@token`.
    mention_candidates: Vec<String>,
    /// The `@token` the in-flight candidate walk was started for.
    mention_query: Option<String>,
    /// In-flight candidate walk.
    mention_task: Option<Task<()>>,
    /// Transient attachment rejection message, e.g. an unsupported file.
    attach_error: Option<String>,
    /// In-flight native file picker.
    attach_task: Option<Task<()>>,
    /// Overflow menu (Follow Edited Files / New Conversation / Chat History).
    overflow_open: bool,
    overflow_focus: FocusHandle,
    /// F-CHAT-34/35: the Chat History popover — every persisted session in
    /// this tab's worktree, loaded on open. `None` renders "No past chats"
    /// (F-CHAT-35). `history_open` gates rendering; `history_delete_confirm`
    /// holds the tab_id awaiting a second click before it is really deleted.
    history_open: bool,
    history_sessions: Vec<ChatSessionSummary>,
    history_delete_confirm: Option<String>,
    history_focus: FocusHandle,
    /// Follow Edited Files toggle state.
    following_edited_files: bool,
    /// F-CHAT-14: throttles `maybe_follow_location` the same 500ms window
    /// as the Swift original's `followThrottle`, so a burst of location
    /// patches on one tool call doesn't open the same file repeatedly.
    last_follow_at: Option<std::time::Instant>,
    /// Effort-level selector advertised by the session.
    effort: Option<EffortOption>,
    /// Test seam: paths handed to the attach control instead of the native
    /// file picker (never set outside `#[cfg(test)]`).
    #[cfg(test)]
    attach_test_paths: Vec<PathBuf>,
}

impl EventEmitter<ChatEvent> for Chat {}

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

        Self::launch_with_command(command, cwd, cx)
    }

    /// Launches a real ACP agent from an explicit command and returns a
    /// `Chat` wired to its event stream.
    ///
    /// This is the picker's door: the caller resolves the chosen adapter's
    /// [`tiller_agents::AcpProgram`] (via `AgentAdapter::acp_program` or
    /// `AgentAvailability::acp_program`) and converts it with
    /// [`acp_agent_command`], so the tab connects to the agent the user
    /// picked rather than a hard-coded default.
    pub fn launch_with_command(
        command: AgentCommand,
        cwd: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut chat = Self::new(command, cwd, cx);
        chat.start_connection(cx);

        chat
    }

    /// Launches an ACP chat whose completed turns are restored and saved in
    /// the durable transcript owned by its shell tab.
    pub fn launch_with_command_and_persistence(
        command: AgentCommand,
        cwd: PathBuf,
        database_path: PathBuf,
        tab_id: String,
        worktree_id: String,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut chat = Self::new(command, cwd, cx);
        chat.persistence = Some(ChatPersistence {
            database_path,
            tab_id,
            worktree_id,
        });
        chat.restore_persisted_transcript();
        chat.start_connection(cx);
        chat
    }

    /// [`Self::launch`] with persistence: the same default-agent command,
    /// but wired to save/restore its transcript and to browse the
    /// worktree's Chat History (F-CHAT-34).
    pub fn launch_with_persistence(
        database_path: PathBuf,
        tab_id: String,
        worktree_id: String,
        cx: &mut Context<Self>,
    ) -> Self {
        let cwd = default_agent_cwd();
        let command = std::env::var_os("TILLER_ACP_PROGRAM")
            .map(PathBuf::from)
            .map(AgentCommand::new)
            .unwrap_or_else(|| {
                AgentCommand::new("npx")
                    .args(["-y", "@agentclientprotocol/claude-agent-acp@latest"])
            });
        Self::launch_with_command_and_persistence(
            command,
            cwd,
            database_path,
            tab_id,
            worktree_id,
            cx,
        )
    }

    fn new(command: AgentCommand, cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        Self::bind_keys(cx);
        let list_state = ListState::new(0, ListAlignment::Top, px(2048.0));
        list_state.set_follow_mode(FollowMode::Tail);
        // F-CHAT-20: scrolling away from the tail mid-stream (a real
        // GPUI `ScrollWheelEvent`, not this app's code) already flips
        // `FollowMode` off inside `ListState::scroll` — that half needed no
        // fix. But nothing ever turned it back on, so once a user scrolled
        // away the transcript stayed pinned to that spot forever, even after
        // manually scrolling back down to the last entry — there was no way
        // to "re-pin". Re-enable Tail-follow whenever a scroll leaves the
        // list sitting exactly at its end.
        //
        // Must `cx.defer`: the scroll handler runs while `ListState`'s own
        // `RefCell` is still mutably borrowed inside `scroll()`, so calling
        // `set_follow_mode` (which borrows it again) from here directly
        // would double-borrow and panic.
        let scroll_list_state = list_state.clone();
        list_state.set_scroll_handler(move |_event, _window, cx| {
            let list_state = scroll_list_state.clone();
            cx.defer(move |_cx| {
                if list_state.is_scrolled_to_end() == Some(true) {
                    list_state.set_follow_mode(FollowMode::Tail);
                }
            });
        });

        Self {
            client: None,
            agent_command: command,
            agent_cwd: cwd,
            entries: Vec::new(),
            composer: Composer::new(),
            composer_focus: cx.focus_handle().tab_stop(true),
            question_answer: QuestionAnswerState {
                focus: cx.focus_handle().tab_stop(true),
                draft: String::new(),
                for_request: None,
            },
            model_picker_focus: cx.focus_handle().tab_stop(true),
            mode_picker_focus: cx.focus_handle().tab_stop(true),
            context_popover_focus: cx.focus_handle().tab_stop(true),
            overflow_focus: cx.focus_handle().tab_stop(true),
            transcript_focus: cx.focus_handle().tab_stop(false),
            streaming: false,
            queued_item: None,
            connecting: false,
            has_completed_turn: false,
            mcp_warnings_shown: 0,
            available_models: Vec::new(),
            model_config_id: None,
            selected_model: None,
            model_picker_open: false,
            model_search: String::new(),
            mode_catalog: None,
            mode_picker_open: false,
            context_popover_open: false,
            context_usage: None,
            list_state,
            transcript_selection: None,
            transcript_dragging: false,
            copied_target: None,
            edit_summaries: BTreeMap::new(),
            persistence: None,
            _event_task: None,
            available_commands: Vec::new(),
            slash_dismissed: false,
            last_slash_token: None,
            slash_selected: 0,
            mention_candidates: Vec::new(),
            mention_query: None,
            mention_task: None,
            attach_error: None,
            attach_task: None,
            overflow_open: false,
            history_open: false,
            history_sessions: Vec::new(),
            history_delete_confirm: None,
            history_focus: cx.focus_handle().tab_stop(true),
            following_edited_files: false,
            last_follow_at: None,
            effort: None,
            #[cfg(test)]
            attach_test_paths: Vec::new(),
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

    /// F-CHAT-33: append any MCP-configuration-flavored stderr lines the
    /// live client has observed since the last time this ran, as
    /// non-retryable transcript errors. Called once per turn end so a
    /// misconfigured MCP server the agent silently ignored isn't invisible
    /// to the user just because the turn itself "succeeded".
    fn surface_mcp_warnings(&mut self) {
        let Some(client) = &self.client else {
            return;
        };
        let warnings = client.mcp_warnings();
        if warnings.len() <= self.mcp_warnings_shown {
            return;
        }
        for warning in &warnings[self.mcp_warnings_shown..] {
            self.push_entry(Entry::Error {
                message: warning.clone(),
                retryable: false,
                kind: ErrorKind::McpWarning,
            });
        }
        self.mcp_warnings_shown = warnings.len();
    }

    fn remeasure_entry(&self, index: usize) {
        self.list_state.remeasure_items(index..index + 1);
    }

    fn subagent_task_position(&self, id: &str) -> Option<usize> {
        self.entries.iter().rposition(
            |entry| matches!(entry, Entry::SubagentTask { id: task_id, .. } if task_id == id),
        )
    }

    fn nested_tool_call_position(&self, id: &str) -> Option<(usize, usize)> {
        self.entries
            .iter()
            .enumerate()
            .rev()
            .find_map(|(task_index, entry)| {
                let Entry::SubagentTask { tool_calls, .. } = entry else {
                    return None;
                };
                tool_calls
                    .iter()
                    .position(|call| call.id == id)
                    .map(|child_index| (task_index, child_index))
            })
    }

    /// F-CHAT-21: flips one thought entry's expand/collapse state.
    fn toggle_thought_expanded(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(Entry::Thought { expanded, .. }) = self.entries.get_mut(index) {
            *expanded = !*expanded;
            self.remeasure_entry(index);
        }
        cx.notify();
    }

    /// F-CHAT-23: flips one tool call entry's expand/collapse state.
    fn toggle_tool_call_expanded(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(Entry::ToolCall { expanded, .. }) = self.entries.get_mut(index) {
            *expanded = !*expanded;
            self.remeasure_entry(index);
        }
        cx.notify();
    }

    /// Flips the outer subagent task card between its one-line summary and
    /// the child tool calls reported while its task call was in progress.
    fn toggle_subagent_task_expanded(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(Entry::SubagentTask { expanded, .. }) = self.entries.get_mut(index) {
            *expanded = !*expanded;
            self.remeasure_entry(index);
        }
        cx.notify();
    }

    /// Flips one nested tool call without changing the parent task card.
    fn toggle_subagent_tool_call_expanded(
        &mut self,
        task_index: usize,
        child_index: usize,
        cx: &mut Context<Self>,
    ) {
        if let Some(Entry::SubagentTask { tool_calls, .. }) = self.entries.get_mut(task_index)
            && let Some(call) = tool_calls.get_mut(child_index)
        {
            call.expanded = !call.expanded;
            self.remeasure_entry(task_index);
        }
        cx.notify();
    }

    /// F-CHAT-22: flips a run of tool calls between its compact "N steps"
    /// summary and every step shown as its own full card. `index` is the
    /// run's last entry — the one the group's header renders against.
    fn toggle_tool_call_group_expanded(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(Entry::ToolCall { group_expanded, .. }) = self.entries.get_mut(index) {
            *group_expanded = !*group_expanded;
            self.remeasure_entry(index);
        }
        cx.notify();
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
            // Linux spelling of the platform modifier (Super on Linux):
            // Ctrl+A and Ctrl+C are what a Linux user presses; the mac
            // `cmd-` forms would bind Super and be unreachable.
            KeyBinding::new("ctrl-a", SelectAll, Some("ChatComposer")),
            KeyBinding::new("ctrl-c", CopyTranscript, Some("ChatTranscript")),
            KeyBinding::new("ctrl-c", CopyTranscript, Some("ChatComposer")),
            KeyBinding::new("home", Home, Some("ChatComposer")),
            KeyBinding::new("end", End, Some("ChatComposer")),
            // No `cmd-left`/`cmd-right` here: Home/End above cover the same
            // movement, and on Linux the Super variants are intercepted by
            // the desktop's window tiling before the app ever sees them.
            KeyBinding::new("escape", Cancel, Some("ChatModelPicker")),
            KeyBinding::new("escape", Cancel, Some("ChatContextPopover")),
            // F-CHAT-25: the question answer field owns Enter (send the
            // answer) and Escape (cancel the question) while it has focus.
            KeyBinding::new("enter", SendAnswer, Some("ChatQuestionAnswer")),
            KeyBinding::new("return", SendAnswer, Some("ChatQuestionAnswer")),
            KeyBinding::new("escape", CancelAnswer, Some("ChatQuestionAnswer")),
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
                if let Some(Entry::Thought { text: existing, .. }) = self.entries.last_mut() {
                    existing.push_str(&text);
                    self.remeasure_entry(self.entries.len() - 1);
                } else {
                    self.push_entry(Entry::Thought {
                        text,
                        expanded: false,
                    });
                }
            }
            AcpEvent::ToolCallStarted {
                id,
                title,
                status,
                kind,
                content,
                locations,
                raw_input,
                raw_output,
            } => {
                self.maybe_follow_location(&locations, cx);
                if is_subagent_tool_call(&title, raw_input.as_deref()) {
                    self.push_entry(Entry::SubagentTask {
                        id,
                        title,
                        status,
                        tool_calls: Vec::new(),
                        expanded: false,
                    });
                } else if let Some(index) = self.entries.iter().rposition(|entry| {
                    matches!(
                        entry,
                        Entry::SubagentTask { status, .. }
                            if !is_terminal_tool_status(status)
                    )
                }) {
                    if let Some(Entry::SubagentTask { tool_calls, .. }) =
                        self.entries.get_mut(index)
                    {
                        tool_calls.push(SubagentToolCall {
                            id,
                            title,
                            status,
                            kind,
                            content,
                            locations,
                            expanded: false,
                        });
                        self.remeasure_entry(index);
                    }
                } else {
                    self.push_entry(Entry::ToolCall {
                        id,
                        title,
                        status,
                        kind,
                        content,
                        locations,
                        raw_input,
                        raw_output,
                        expanded: false,
                        group_expanded: false,
                    });
                }
            }
            AcpEvent::ToolCallUpdated {
                id,
                title,
                status,
                kind,
                content,
                locations,
                raw_input,
                raw_output,
            } => {
                if let Some(locations) = &locations {
                    self.maybe_follow_location(locations, cx);
                }
                if let Some(task_index) = self.subagent_task_position(&id) {
                    if let Some(Entry::SubagentTask {
                        title: existing_title,
                        status: existing_status,
                        ..
                    }) = self.entries.get_mut(task_index)
                    {
                        if let Some(title) = title {
                            *existing_title = title;
                        }
                        if let Some(status) = status {
                            *existing_status = status;
                        }
                        self.remeasure_entry(task_index);
                    }
                } else if let Some((task_index, child_index)) = self.nested_tool_call_position(&id) {
                    if let Some(Entry::SubagentTask { tool_calls, .. }) =
                        self.entries.get_mut(task_index)
                        && let Some(call) = tool_calls.get_mut(child_index)
                    {
                        if let Some(title) = title {
                            call.title = title;
                        }
                        if let Some(status) = status {
                            call.status = status;
                        }
                        if let Some(kind) = kind {
                            call.kind = kind;
                        }
                        if let Some(content) = content {
                            call.content = content;
                        }
                        if let Some(locations) = locations {
                            call.locations = locations;
                        }
                        self.remeasure_entry(task_index);
                    }
                } else if let Some((index, Entry::ToolCall {
                    title: existing_title,
                    status: existing_status,
                    kind: existing_kind,
                    content: existing_content,
                    locations: existing_locations,
                    raw_input: existing_raw_input,
                    raw_output: existing_raw_output,
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
                    if let Some(kind) = kind {
                        *existing_kind = kind;
                    }
                    if let Some(content) = content {
                        *existing_content = content;
                    }
                    if let Some(locations) = locations {
                        *existing_locations = locations;
                    }
                    if let Some(raw_input) = raw_input {
                        *existing_raw_input = Some(raw_input);
                    }
                    if let Some(raw_output) = raw_output {
                        *existing_raw_output = Some(raw_output);
                    }
                    self.remeasure_entry(self.entries.len() - 1 - index);
                }
            }
            AcpEvent::ToolCallCompleted {
                id,
                status,
                kind,
                content,
                locations,
                raw_input,
                raw_output,
            } => {
                if let Some(locations) = &locations {
                    self.maybe_follow_location(locations, cx);
                }
                if let Some(task_index) = self.subagent_task_position(&id) {
                    if let Some(Entry::SubagentTask { status: existing_status, .. }) =
                        self.entries.get_mut(task_index)
                    {
                        *existing_status = status;
                        self.remeasure_entry(task_index);
                    }
                } else if let Some((task_index, child_index)) = self.nested_tool_call_position(&id) {
                    if let Some(Entry::SubagentTask { tool_calls, .. }) =
                        self.entries.get_mut(task_index)
                        && let Some(call) = tool_calls.get_mut(child_index)
                    {
                        call.status = status;
                        if let Some(kind) = kind {
                            call.kind = kind;
                        }
                        if let Some(content) = content {
                            call.content = content;
                        }
                        if let Some(locations) = locations {
                            call.locations = locations;
                        }
                        self.remeasure_entry(task_index);
                    }
                } else if let Some((index, Entry::ToolCall {
                    status: existing_status,
                    kind: existing_kind,
                    content: existing_content,
                    locations: existing_locations,
                    raw_input: existing_raw_input,
                    raw_output: existing_raw_output,
                    ..
                })) = self
                    .entries
                    .iter_mut()
                    .rev()
                    .enumerate()
                    .find(|(_, entry)| matches!(entry, Entry::ToolCall { id: entry_id, .. } if *entry_id == id))
                {
                    *existing_status = status;
                    if let Some(kind) = kind {
                        *existing_kind = kind;
                    }
                    if let Some(content) = content {
                        *existing_content = content;
                    }
                    if let Some(locations) = locations {
                        *existing_locations = locations;
                    }
                    if let Some(raw_input) = raw_input {
                        *existing_raw_input = Some(raw_input);
                    }
                    if let Some(raw_output) = raw_output {
                        *existing_raw_output = Some(raw_output);
                    }
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
            AcpEvent::AvailableCommands(commands) => {
                self.available_commands = commands;
            }
            AcpEvent::Effort(effort) => {
                self.effort = Some(effort);
            }
            AcpEvent::ContextUsage(usage) => {
                self.context_usage = Some(usage);
            }
            // F-CHAT-18: the breakdown arrives on a separate wire event
            // (PromptResponse.usage, gated behind the ACP-agent-side
            // `unstable_end_turn_token_usage` extension) from the
            // used/size/cost triple on `ContextUsage`, so it merges into
            // whatever context usage is already known rather than
            // replacing it -- an agent that reports one without the other
            // must not blank out the other's fields.
            AcpEvent::TokenUsageBreakdown {
                input_tokens,
                output_tokens,
                cached_read_tokens,
            } => {
                let usage = self.context_usage.get_or_insert_with(ContextUsage::default);
                usage.input_tokens = Some(input_tokens);
                usage.output_tokens = Some(output_tokens);
                usage.cached_read_tokens = cached_read_tokens;
            }
            AcpEvent::PermissionRequest {
                request_id,
                title,
                options,
                question,
                ..
            } => {
                let answer_options = options
                    .into_iter()
                    .map(|option| AnswerOption {
                        is_rejection: option.kind.contains("Reject"),
                        id: option.id,
                        label: option.name,
                    })
                    .collect::<Vec<_>>();
                let is_plan_approval = title.to_ascii_lowercase().contains("plan")
                    && matches!(
                        self.entries.last(),
                        Some(Entry::Plan { approval: None, .. })
                    );
                if is_plan_approval {
                    // F-CHAT-24: approval of a plan attaches its option
                    // buttons to the Plan card instead of a separate card.
                    if let Some(Entry::Plan { approval, .. }) = self.entries.last_mut() {
                        *approval = Some(PlanApproval {
                            request_id,
                            title,
                            options: answer_options,
                            resolved: None,
                            expired: false,
                        });
                    }
                } else {
                    let has_structured_question = question.is_some();
                    let structured_prompt = question
                        .as_ref()
                        .map(|question| question.prompt.clone())
                        .unwrap_or_default();
                    let text_input = question
                        .and_then(|question| question.text_input)
                        .map(|input| AnswerTextInput {
                            placeholder: input.placeholder,
                            prefill: input.prefill,
                        })
                        .or_else(|| {
                            // Structured questions with no choices are still
                            // answerable through the existing text field. A
                            // plain permission with no choices is different:
                            // its wire request has no renderable answer, so it
                            // gets Dismiss below instead of a fake option.
                            (has_structured_question && answer_options.is_empty()).then_some(
                                AnswerTextInput {
                                    placeholder: None,
                                    prefill: None,
                                },
                            )
                        });
                    self.push_entry(Entry::Permission {
                        request_id,
                        title,
                        prompt: structured_prompt,
                        options: answer_options,
                        text_input,
                        resolved: None,
                        expired: false,
                        dismissed: false,
                    });
                }
            }
            AcpEvent::PlanUpdate { entries } => {
                let entries = entries
                    .into_iter()
                    .map(|entry| PlanEntryRow {
                        content: entry.content,
                        status: entry.status,
                    })
                    .collect::<Vec<_>>();
                // A plan is a running state: each update replaces the
                // previous Plan card, so entries advance in place.
                if let Some(Entry::Plan {
                    entries: existing, ..
                }) = self
                    .entries
                    .iter_mut()
                    .rev()
                    .find(|entry| matches!(entry, Entry::Plan { .. }))
                {
                    *existing = entries;
                } else {
                    self.push_entry(Entry::Plan {
                        entries,
                        approval: None,
                    });
                }
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
                self.expire_unanswered();
                self.surface_mcp_warnings();
                self.push_entry(Entry::TurnFooter(label));
                self.streaming = false;
                self.has_completed_turn = true;
                self.persist_settled_transcript();
                // D-CHAT-03: a turn ended — completed or cancelled alike,
                // one rule — drains the queued item as the next turn,
                // exactly once. The footer lands before the queued turn so
                // the transcript reads: stop stated, then the redirect.
                self.send_queued_item(cx);
            }
            AcpEvent::TransportError(message) => {
                self.client.take();
                self.expire_unanswered();
                let (message, kind) = classify_connection_error(message);
                self.push_entry(Entry::Error {
                    message,
                    retryable: true,
                    kind,
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
                self.expire_unanswered();
                self.push_entry(Entry::Error {
                    message: format!(
                        "{operation:?} timed out after {:.1}s",
                        duration.as_secs_f32()
                    ),
                    retryable: true,
                    kind: ErrorKind::Connection,
                });
                self.streaming = false;
            }
            AcpEvent::OtherSessionUpdate { .. } => {}
        }
        // F-CHAT-15: a `CurrentModeUpdate` (agent-initiated mode switch, or
        // this chat's own `set_mode` resolving) only reaches the wire as an
        // untyped `OtherSessionUpdate` today — `AcpClient` folds it into its
        // live `mode_catalog` cell for every kind of event, so re-reading it
        // here after any event picks up the change without a dedicated
        // event variant.
        if let Some(client) = &self.client {
            self.mode_catalog = client.mode_catalog();
        }
        cx.notify();
    }

    /// F-CHAT-05: true exactly when the composer's own "offline" placeholder
    /// is showing (see the `composer_parts` match in `render`) — mirrors the
    /// Swift reference's `ChatController.ChatState.disconnected`, which
    /// `ChatComposerView.canInteract` explicitly excludes
    /// (`ChatComposerView.swift:25-28`). Every composer-mutating entry point
    /// below checks this the same way it already checks `pending_question`,
    /// so the whole editor goes out of service while disconnected, not just
    /// Send.
    fn is_offline(&self) -> bool {
        self.client.is_none() && !self.connecting
    }

    fn can_send(&self) -> bool {
        // F-CHAT-05: an unresolved permission/plan question must block Send
        // in its own right, not merely ride along with `streaming` (a
        // permission request always arrives mid-turn today, so the two
        // happen to coincide, but the check must name its real reason —
        // future non-streaming question types must not slip through). The
        // offline check is the same rule applied to the other half of the
        // same contract row: the reference disables Send while disconnected
        // too, it just never needed a separate flag for it because
        // `.disabled(!canInteract)` covers the whole editor at once.
        !self.streaming
            && !self.connecting
            && !self.is_offline()
            && !self.composer.is_empty()
            && self.pending_question().is_none()
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

    /// Converts rendered entries into the durable format, retaining only
    /// turns closed by a footer. A partially streamed tail is deliberately
    /// omitted so a relaunch never presents an unfinished answer as settled.
    fn transcript_from_entries(tab_id: &str, entries: &[Entry]) -> ChatTranscript {
        let mut turns = Vec::new();
        let mut current = Vec::new();
        for entry in entries {
            if let Some(entry) = persisted_entry(entry) {
                let is_footer = matches!(entry, ChatEntry::TurnFooter { .. });
                current.push(entry);
                if is_footer {
                    turns.push(ChatTurn { entries: current });
                    current = Vec::new();
                }
            }
        }
        ChatTranscript {
            tab_id: tab_id.to_string(),
            turns,
        }
    }

    /// Returns the settled transcript for callers that need to inspect the
    /// persistence seam without exposing the internal Entry model.
    pub fn persisted_transcript(&self) -> Option<ChatTranscript> {
        let persistence = self.persistence.as_ref()?;
        let transcript = Self::transcript_from_entries(&persistence.tab_id, &self.entries);
        (!transcript.turns.is_empty()).then_some(transcript)
    }

    /// The composer's current unsent draft text — F-CORE-WSP-08's
    /// persistence seam. Read live at session-save time (mirrors how a
    /// terminal pane's scrollback is captured fresh from the live view
    /// rather than tracked incrementally) and pushed back through
    /// [`Self::control_compose`] on restore.
    pub fn draft_text(&self) -> String {
        self.composer.text()
    }

    /// Replaces the visible composer's plain-text draft through the control
    /// socket route. Attachments deliberately remain a pointer-only concern.
    pub fn control_compose(&mut self, text: &str, cx: &mut Context<Self>) {
        self.composer = Composer::new();
        self.composer.insert_text(text);
        self.reset_composer_popups();
        self.refresh_token_popups(cx);
        cx.notify();
    }

    /// Sends exactly as the rendered Send control does, after replacing the
    /// visible composer with the socket request's text.
    pub fn control_send(&mut self, text: &str, cx: &mut Context<Self>) {
        self.control_compose(text, cx);
        self.send(cx);
    }

    /// Resolves a rendered permission option by its protocol id and sends the
    /// answer through the same path used by the card's buttons.
    pub fn control_permission(
        &mut self,
        request_id: u64,
        option_id: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let option = self.entries.iter().rev().find_map(|entry| match entry {
            Entry::Permission {
                request_id: id,
                options,
                ..
            } if *id == request_id => options
                .iter()
                .find(|option| option.id == option_id)
                .cloned(),
            Entry::Plan {
                approval: Some(approval),
                ..
            } if approval.request_id == request_id => approval
                .options
                .iter()
                .find(|option| option.id == option_id)
                .cloned(),
            _ => None,
        });
        let Some(option) = option else {
            return Err(format!("unknown chat permission option: {option_id}"));
        };
        self.respond_permission(request_id, &option, cx);
        Ok(())
    }

    /// Stops the rendered turn. It is intentionally a no-op while idle,
    /// matching the visible Stop affordance.
    pub fn control_stop(&mut self, cx: &mut Context<Self>) {
        self.cancel_turn(cx);
    }

    /// Produces the control API's read model from the entity that owns the
    /// composer and transcript on screen.
    pub fn control_snapshot(&self) -> ChatControlSnapshot {
        let status = if self.streaming {
            "streaming"
        } else if self.connecting {
            "connecting"
        } else if self.has_completed_turn {
            "completed"
        } else {
            "idle"
        };
        ChatControlSnapshot {
            status: status.to_string(),
            composer_text: self.composer.text(),
            queued_text: self.queued_item.clone().unwrap_or_default(),
            transcript: self.entries.iter().map(control_entry_row).collect(),
        }
    }

    fn restore_persisted_transcript(&mut self) {
        let Some(persistence) = self.persistence.as_ref() else {
            return;
        };
        let database = match AppDatabase::open(&persistence.database_path) {
            Ok(database) => database,
            Err(error) => {
                eprintln!(
                    "[chat] failed to open transcript database {}: {error}",
                    persistence.database_path.display()
                );
                return;
            }
        };
        let transcript = match database.load_chat_transcript(&persistence.tab_id) {
            Ok(Some(transcript)) => transcript,
            Ok(None) => return,
            Err(error) => {
                eprintln!("[chat] failed to load transcript: {error}");
                return;
            }
        };
        for turn in transcript.turns {
            for entry in turn.entries {
                self.push_entry(restored_entry(entry));
            }
        }
        self.has_completed_turn = !self.entries.is_empty();
    }

    /// F-CHAT-34: toggles the Chat History popover, loading every persisted
    /// session in this tab's worktree from the durable database on open.
    /// A tab with no persistence (never launched with a database/tab_id, or
    /// the DB failed to open) still opens the popover so F-CHAT-35's "No
    /// past chats" empty state is reachable rather than the control
    /// silently doing nothing.
    fn toggle_chat_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overflow_open = false;
        self.history_open = !self.history_open;
        self.history_delete_confirm = None;
        if self.history_open {
            self.history_sessions = self.load_chat_history();
            let focus = self.history_focus.clone();
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    fn load_chat_history(&self) -> Vec<ChatSessionSummary> {
        let Some(persistence) = self.persistence.as_ref() else {
            return Vec::new();
        };
        let database = match AppDatabase::open(&persistence.database_path) {
            Ok(database) => database,
            Err(error) => {
                eprintln!(
                    "[chat] failed to open transcript database {}: {error}",
                    persistence.database_path.display()
                );
                return Vec::new();
            }
        };
        match database.chat_sessions(&persistence.worktree_id) {
            Ok(sessions) => sessions,
            Err(error) => {
                eprintln!("[chat] failed to list chat sessions: {error}");
                Vec::new()
            }
        }
    }

    /// Opens a past session into this tab: loads its transcript from the
    /// database, replaces the live entries with it, and repoints this tab's
    /// own persistence at the opened session so a new turn appends there
    /// rather than silently writing back into the tab the popover was
    /// opened from.
    fn open_chat_history_session(&mut self, tab_id: String, cx: &mut Context<Self>) {
        let Some(persistence) = self.persistence.as_mut() else {
            return;
        };
        let database = match AppDatabase::open(&persistence.database_path) {
            Ok(database) => database,
            Err(error) => {
                eprintln!(
                    "[chat] failed to open transcript database {}: {error}",
                    persistence.database_path.display()
                );
                return;
            }
        };
        let transcript = match database.load_chat_transcript(&tab_id) {
            Ok(Some(transcript)) => transcript,
            Ok(None) => return,
            Err(error) => {
                eprintln!("[chat] failed to load transcript: {error}");
                return;
            }
        };
        persistence.tab_id = tab_id;
        self.entries.clear();
        for turn in transcript.turns {
            for entry in turn.entries {
                self.push_entry(restored_entry(entry));
            }
        }
        self.has_completed_turn = !self.entries.is_empty();
        self.history_open = false;
        cx.notify();
    }

    /// F-CHAT-34's delete-with-confirmation half: the first click arms
    /// `history_delete_confirm`; a second click on the same row's Confirm
    /// control actually removes the transcript and drops it from the
    /// visible list.
    fn request_delete_chat_session(&mut self, tab_id: String, cx: &mut Context<Self>) {
        self.history_delete_confirm = Some(tab_id);
        cx.notify();
    }

    fn confirm_delete_chat_session(&mut self, tab_id: String, cx: &mut Context<Self>) {
        self.history_delete_confirm = None;
        let Some(persistence) = self.persistence.as_ref() else {
            return;
        };
        let database = match AppDatabase::open(&persistence.database_path) {
            Ok(database) => database,
            Err(error) => {
                eprintln!(
                    "[chat] failed to open transcript database {}: {error}",
                    persistence.database_path.display()
                );
                return;
            }
        };
        if let Err(error) = database.delete_chat_session(&tab_id) {
            eprintln!("[chat] failed to delete chat session: {error}");
            return;
        }
        self.history_sessions
            .retain(|session| session.tab_id != tab_id);
        cx.notify();
    }

    fn cancel_delete_chat_session(&mut self, cx: &mut Context<Self>) {
        self.history_delete_confirm = None;
        cx.notify();
    }

    fn persist_settled_transcript(&self) {
        let Some(persistence) = self.persistence.as_ref() else {
            return;
        };
        let database = match AppDatabase::open(&persistence.database_path) {
            Ok(database) => database,
            Err(error) => {
                eprintln!("[chat] failed to open transcript database: {error}");
                return;
            }
        };
        let transcript = Self::transcript_from_entries(&persistence.tab_id, &self.entries);
        if let Err(error) = database.save_chat_transcript(&transcript) {
            eprintln!("[chat] failed to save transcript: {error}");
        }
    }

    fn clear_persisted_transcript(&self) {
        let Some(persistence) = self.persistence.as_ref() else {
            return;
        };
        let Ok(database) = AppDatabase::open(&persistence.database_path) else {
            return;
        };
        let empty = ChatTranscript {
            tab_id: persistence.tab_id.clone(),
            turns: Vec::new(),
        };
        if let Err(error) = database.save_chat_transcript(&empty) {
            eprintln!("[chat] failed to clear transcript: {error}");
        }
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

    /// Copies a local transcript fragment and gives just that control a
    /// brief acknowledgement. The delayed clear is deliberately keyed to
    /// the target, so a second click cannot have its confirmation cleared by
    /// the first click's already-scheduled timer.
    fn copy_local_text(&mut self, target: CopyTarget, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied_target = Some(target.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            let _ = this.update(cx, |chat, cx| {
                if chat.copied_target.as_ref() == Some(&target) {
                    chat.copied_target = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn request_edit_revert(&mut self, entry: usize, path: PathBuf, cx: &mut Context<Self>) {
        let state = self.edit_summaries.entry(entry).or_default();
        state.confirming_path = Some(path);
        state.revert_error = None;
        cx.notify();
    }

    fn cancel_edit_revert(&mut self, entry: usize, cx: &mut Context<Self>) {
        if let Some(state) = self.edit_summaries.get_mut(&entry) {
            state.confirming_path = None;
        }
        cx.notify();
    }

    fn confirm_edit_revert(&mut self, entry: usize, path: PathBuf, cx: &mut Context<Self>) {
        let state = self.edit_summaries.entry(entry).or_default();
        state.confirming_path = None;
        state.revert_error = None;
        state.reverting_path = Some(path.clone());
        let repo = self.agent_cwd.clone();
        let revert_path = path.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { discard_edited_path(&repo, &revert_path) })
                .await;
            let _ = this.update(cx, |chat, cx| {
                let state = chat.edit_summaries.entry(entry).or_default();
                state.reverting_path = None;
                match result {
                    Ok(()) => state.reverted_paths.push(path),
                    Err(error) => state.revert_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn on_transcript_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "c" && event.keystroke.modifiers.control {
            self.copy_transcript(&CopyTranscript, window, cx);
        }
    }

    fn clear_recovered_connection_errors(&mut self) {
        let old_count = self.entries.len();
        self.entries.retain(|entry| {
            !matches!(
                entry,
                Entry::Error {
                    kind: ErrorKind::Connection | ErrorKind::AuthRequired,
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
            // F-CHAT-16: a fresh search every time the picker opens, same as
            // Swift's `@State private var query` starting blank each time
            // the popover view is recreated.
            self.model_search.clear();
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

    /// F-CHAT-15: opens the session-mode picker anchored to the status
    /// pill. A no-op when the agent never advertised `modes` — the pill has
    /// no chevron and no click handler in that case (mirrors the model
    /// chip's degrade-to-badge rule, F-CHAT-36).
    fn toggle_mode_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode_catalog.is_none() {
            return;
        }
        self.model_picker_open = false;
        self.context_popover_open = false;
        self.mode_picker_open = !self.mode_picker_open;
        if self.mode_picker_open {
            let focus = self.mode_picker_focus.clone();
            window.focus(&focus, cx);
            window.on_next_frame(move |window, _| {
                window.on_next_frame(move |window, cx| window.focus(&focus, cx));
            });
        }
        cx.notify();
    }

    /// F-CHAT-15: asks the live agent to switch session mode, then
    /// optimistically reflects the choice as `current_id` so the pill label
    /// updates immediately rather than waiting on the round-trip
    /// `CurrentModeUpdate`/`set_mode` confirmation.
    fn select_mode(&mut self, mode: AgentMode, cx: &mut Context<Self>) {
        if let Some(client) = &self.client {
            let _ = client.set_mode(mode.id.clone());
        }
        if let Some(catalog) = &mut self.mode_catalog {
            catalog.current_id = mode.id;
        }
        self.mode_picker_open = false;
        cx.notify();
    }

    // --- Slash-command popup (F-CHAT-09) ---

    /// The popup's candidate list: all commands while the query is empty,
    /// then the case-insensitive prefix matches — capped at ten rows like
    /// the reference `slashCandidates`.
    fn slash_candidates(&self) -> Vec<&AvailableCommandInfo> {
        let Some(query) = self.composer.slash_token() else {
            return Vec::new();
        };
        let query = query.to_lowercase();
        let matching = |command: &&AvailableCommandInfo| {
            query.is_empty() || command.name.to_lowercase().starts_with(&query)
        };
        let all: Vec<&AvailableCommandInfo> = self.available_commands.iter().collect();
        all.into_iter().filter(matching).take(10).collect()
    }

    fn slash_popup_visible(&self) -> bool {
        !self.slash_candidates().is_empty() && !self.slash_dismissed
    }

    /// Inserts the selected command as a skill token. The popup closes
    /// because the draft is no longer a single `/token`.
    fn accept_slash_selection(&mut self, cx: &mut Context<Self>) {
        let candidates = self.slash_candidates();
        if candidates.is_empty() {
            return;
        }
        let selected = self.slash_selected.min(candidates.len() - 1);
        let name = candidates[selected].name.clone();
        self.composer.replace_slash_token(&name);
        self.slash_dismissed = true;
        self.refresh_token_popups(cx);
    }

    fn accept_slash_command(&mut self, name: &str, cx: &mut Context<Self>) {
        self.composer.replace_slash_token(name);
        self.slash_dismissed = true;
        self.refresh_token_popups(cx);
    }

    // --- @ file mentions (F-CHAT-10) ---

    /// Starts the bounded filesystem walk for the current mention token. The
    /// result is dropped when the token changed while the walk ran — the
    /// same staleness guard the reference `refreshMentionCandidates` uses.
    fn schedule_mention_walk(&mut self, cx: &mut Context<Self>) {
        let Some(query) = self.mention_query.clone() else {
            self.mention_task.take();
            return;
        };
        let cwd = self.agent_cwd.clone();
        self.mention_task = Some(cx.spawn(async move |this, cx| {
            let walk_query = query.clone();
            let hits = cx
                .background_executor()
                .spawn(async move { mention_candidates_on_disk(&cwd, &walk_query) })
                .await;
            let _ = this.update(cx, |chat, cx| {
                if chat.mention_query.as_deref() == Some(query.as_str()) {
                    chat.mention_candidates = hits;
                    cx.notify();
                }
            });
        }));
    }

    fn accept_mention(&mut self, path: &str, cx: &mut Context<Self>) {
        self.composer.accept_mention(path);
        self.mention_candidates.clear();
        self.refresh_token_popups(cx);
    }

    // --- Attachments (F-CHAT-11 / F-CHAT-12) ---

    /// Opens the image picker and inserts the chosen image as a chip.
    /// `multiple` is off and only PNG/JPEG are accepted, so an unsupported
    /// choice is impossible in the picker itself; whatever the picker (or
    /// the test seam) returns is still validated here.
    fn attach_image(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(test)]
        if !self.attach_test_paths.is_empty() {
            let paths = std::mem::take(&mut self.attach_test_paths);
            self.apply_attached_paths(paths, cx);
            return;
        }
        let options = gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Attach image".into()),
        };
        let receiver = cx.prompt_for_paths(options);
        self.attach_task = Some(cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |chat, cx| match result {
                Ok(Ok(Some(paths))) => chat.apply_attached_paths(paths, cx),
                Ok(Ok(None)) => {}
                Ok(Err(_)) | Err(_) => {
                    chat.show_attach_error("Could not open the file picker.".to_string(), cx)
                }
            });
        }));
    }

    /// Validates picker results and inserts image chips. Anything that is
    /// not exactly one PNG/JPEG file is rejected with a transient message.
    fn apply_attached_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if paths.len() != 1 {
            self.show_attach_error("Only one image can be attached at a time.".to_string(), cx);
            return;
        }
        let path = &paths[0];
        let extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let mime = match extension.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            _ => {
                self.show_attach_error("Only PNG and JPEG images can be attached.".to_string(), cx);
                return;
            }
        };
        let Ok(bytes) = std::fs::read(path) else {
            self.show_attach_error("The image could not be read.".to_string(), cx);
            return;
        };
        use base64::Engine as _;
        let base64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        self.composer.insert_chip_at_cursor(ComposerChip::Image {
            mime: mime.to_string(),
            base64,
        });
        self.refresh_token_popups(cx);
        cx.notify();
    }

    /// Shows the transient rejection message and schedules its dismissal.
    fn show_attach_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.attach_error = Some(message);
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(3))
                .await;
            let _ = this.update(cx, |chat, cx| {
                chat.attach_error = None;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    // --- File drop (F-CHAT-13) ---

    /// Mirrors F-CHAT-05's rule for typed input: a composer that cannot
    /// accept a keystroke must not quietly accumulate chips from a drop
    /// either. Matches Swift's `ChatPaneView.canAcceptDrop`, which excludes
    /// `.disconnected` the same way `canInteract` does
    /// (`ChatPaneView.swift:153-157`).
    fn can_accept_drop(&self) -> bool {
        self.pending_question().is_none() && !self.is_offline()
    }

    /// The whole chat pane is the drop target for files dragged in from
    /// outside the app (the desktop file manager), matching the Swift
    /// original's `.onDrop(of: [.fileURL])` on `ChatPaneView` + `FileDrop`.
    /// A recognized image extension becomes an attachment chip exactly like
    /// the "+" picker, just with the drop path's wider format list; anything
    /// else becomes a `@`-style file chip; anything unreadable or oversized
    /// is rejected through the same transient-message path as
    /// `apply_attached_paths`, one message per rejected item.
    fn drop_external_paths(
        &mut self,
        paths: &ExternalPaths,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_accept_drop() {
            return;
        }
        let paths = paths.paths().to_vec();
        if paths.is_empty() {
            return;
        }
        // Ten megabytes: past this point a stray drop would stall a turn
        // (base64 costs about a third more than the file) instead of
        // enriching it — the same cap as the Swift original's `FileDrop`.
        const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
        fn image_mime(path: &Path) -> Option<&'static str> {
            let extension = path
                .extension()
                .map(|extension| extension.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            match extension.as_str() {
                "png" => Some("image/png"),
                "jpg" | "jpeg" => Some("image/jpeg"),
                "gif" => Some("image/gif"),
                "webp" => Some("image/webp"),
                _ => None,
            }
        }
        let mut rejections: Vec<String> = Vec::new();
        for path in &paths {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            if let Some(mime) = image_mime(path) {
                match std::fs::metadata(path) {
                    Ok(meta) if meta.len() > MAX_IMAGE_BYTES => {
                        rejections.push(format!("{name} is too large (max 10 MB)"));
                        continue;
                    }
                    Err(_) => {
                        rejections.push(format!("Couldn't read {name}"));
                        continue;
                    }
                    _ => {}
                }
                let Ok(bytes) = std::fs::read(path) else {
                    rejections.push(format!("Couldn't read {name}"));
                    continue;
                };
                use base64::Engine as _;
                let base64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                self.composer.insert_chip_at_cursor(ComposerChip::Image {
                    mime: mime.to_string(),
                    base64,
                });
            } else {
                // Relative to the worktree when the file lives inside it
                // (matching the `@`-mention chip's own path convention),
                // absolute otherwise.
                let chip_path = path
                    .strip_prefix(&self.agent_cwd)
                    .map(|relative| relative.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| path.display().to_string());
                self.composer
                    .insert_chip_at_cursor(ComposerChip::File { path: chip_path });
            }
        }
        self.refresh_token_popups(cx);
        if !rejections.is_empty() {
            self.show_attach_error(rejections.join("; "), cx);
        }
        cx.notify();
    }

    /// Removes one chip at the given part index (its × control).
    fn remove_composer_chip(&mut self, part_index: usize, cx: &mut Context<Self>) {
        self.composer.remove_chip(part_index);
        self.refresh_token_popups(cx);
        cx.notify();
    }

    // --- Overflow menu (F-CHAT-14) ---

    /// The toggle's other half: while Follow Edited Files is on, the most
    /// recent location a tool call reports is opened the same way an
    /// edit-summary card's own "open" link does — `ChatEvent::OpenFile`,
    /// which the host already routes to a file tab (`Workspace::bind_chat`).
    /// Reusing that event means Follow needs no new host-side wiring: it
    /// rides the same seam F-CHAT-32 already built. Throttled 500ms, same
    /// as the Swift original's `followThrottle`, so a burst of location
    /// patches on one tool call doesn't reopen the same file repeatedly.
    fn maybe_follow_location(
        &mut self,
        locations: &[ToolCallLocationInfo],
        cx: &mut Context<Self>,
    ) {
        if !self.following_edited_files {
            return;
        }
        let Some(location) = locations.last() else {
            return;
        };
        const FOLLOW_THROTTLE: std::time::Duration = std::time::Duration::from_millis(500);
        let now = std::time::Instant::now();
        if let Some(last) = self.last_follow_at
            && now.duration_since(last) < FOLLOW_THROTTLE
        {
            return;
        }
        self.last_follow_at = Some(now);
        cx.emit(ChatEvent::OpenFile(location.path.clone()));
    }

    fn toggle_overflow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overflow_open = !self.overflow_open;
        if self.overflow_open {
            let focus = self.overflow_focus.clone();
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    /// Resets the composer and transcript to a brand-new chat: every entry
    /// is dropped and the ACP session is relaunched.
    fn new_conversation(&mut self, cx: &mut Context<Self>) {
        let old_count = self.entries.len();
        self.entries.clear();
        self.list_state.splice(0..old_count, 0);
        self.composer = Composer::new();
        self.reset_composer_popups();
        self.overflow_open = false;
        self.streaming = false;
        self.has_completed_turn = false;
        self.transcript_selection = None;
        self.clear_persisted_transcript();
        self.start_connection(cx);
        cx.notify();
    }

    // --- Effort levels (F-CHAT-17) ---

    /// Optimistic like `select_model`: the picker reflects the choice
    /// immediately and reverts when the agent rejects it.
    fn select_effort(&mut self, value: String, cx: &mut Context<Self>) {
        let Some(effort) = &mut self.effort else {
            return;
        };
        let previous = effort.current_value.clone();
        effort.current_value = Some(value.clone());
        if let Some(client) = &self.client
            && let Err(_error) = client.set_config_option(effort.option_id.clone(), value.clone())
        {
            effort.current_value = previous;
        }
        cx.notify();
    }

    fn send(&mut self, cx: &mut Context<Self>) {
        // F-CHAT-05: an unresolved permission/plan question takes the
        // composer out of service entirely — no send, no queueing — until
        // it is answered from its own card in the transcript.
        if self.pending_question().is_some() {
            return;
        }
        // D-CHAT-03: Enter while a turn runs queues the draft for the next
        // turn instead of starting a second turn.
        if self.streaming {
            self.commit_queued_item(cx);
            return;
        }
        // F-CHAT-05: `can_send` already excludes the offline state, so this
        // is the last line of defense, not the primary guard — mirrors the
        // Swift reference, whose `.disabled(!canInteract)` disables the
        // whole editor while `.disconnected` rather than letting Send retry
        // the connection. An earlier revision here did the opposite (an
        // offline Send silently reconnected and resent), on the theory that
        // was a deliberate UX improvement over the contract's wording; a
        // wave-I critic checked that against `ChatController.swift` and
        // found no such feature in the reference, only a disabled composer.
        // Reconnecting is now only ever explicit — the transcript's own
        // retryable-error "Retry" button (`chat.retry`) — never an implicit
        // side effect of Send. Whatever the reason `can_send` said no, this
        // must still never touch `self.composer`: a typed draft must survive
        // untouched. See `offline_enter_never_discards_the_typed_draft`.
        if !self.can_send() {
            return;
        }
        let draft = self.composer.draft();
        self.composer = Composer::new();
        self.reset_composer_popups();
        self.submit_turn(draft.text, draft.mention_paths, draft.images, cx);
    }

    /// Pushes the user turn into the transcript and sends it to the live
    /// client. Shared by the normal send path and the queued-item drain, so
    /// both turns take the same wire path.
    fn submit_turn(
        &mut self,
        text: String,
        mention_paths: Vec<String>,
        images: Vec<ImageAttachment>,
        cx: &mut Context<Self>,
    ) {
        self.push_entry(Entry::User(text.clone()));
        self.streaming = true;
        if let Some(client) = &self.client
            && let Err(error) =
                client.prompt_content(text, mention_paths, images, self.agent_cwd.clone())
        {
            self.client.take();
            self.push_entry(Entry::Error {
                message: format!("send failed: {error:#}"),
                retryable: true,
                kind: ErrorKind::Connection,
            });
            self.streaming = false;
        }
        cx.notify();
    }

    /// D-CHAT-03: while a turn streams, a send commits the draft as the
    /// queued item — one slot, latest commit wins. Mirrors `send`'s
    /// consumption of the composer; an empty draft commits nothing and
    /// leaves any previous item in place.
    fn commit_queued_item(&mut self, cx: &mut Context<Self>) {
        if self.composer.is_empty() {
            return;
        }
        let draft = self.composer.draft();
        self.composer = Composer::new();
        self.reset_composer_popups();
        self.queued_item = Some(draft.text);
        cx.notify();
    }

    /// D-CHAT-03: the ✕ on the queued row. The item is dropped; nothing
    /// sends when the turn ends.
    fn remove_queued_item(&mut self, cx: &mut Context<Self>) {
        self.queued_item = None;
        cx.notify();
    }

    /// D-CHAT-03: drains the queued item as the next user turn. Only the
    /// turn-end path calls this — completed and cancelled alike — so a
    /// committed item sends exactly once. A turn end without a client is a
    /// transport death: the item stays queued rather than firing nowhere.
    fn send_queued_item(&mut self, cx: &mut Context<Self>) {
        if self.client.is_none() {
            return;
        }
        let Some(text) = self.queued_item.take() else {
            return;
        };
        self.submit_turn(text, Vec::new(), Vec::new(), cx);
    }

    /// Closes every popup that belongs to a specific draft token or state:
    /// the slash and mention popups track tokens, the attach rejection is a
    /// transient event, and the pickers/menu are one-shot surfaces.
    fn reset_composer_popups(&mut self) {
        self.slash_dismissed = false;
        self.last_slash_token = None;
        self.slash_selected = 0;
        self.mention_candidates.clear();
        self.mention_query = None;
        self.attach_error = None;
    }

    fn respond_permission(
        &mut self,
        request_id: u64,
        option: &AnswerOption,
        cx: &mut Context<Self>,
    ) {
        if let Some((index, entry)) = self
            .entries
            .iter_mut()
            .enumerate()
            .rev()
            .find(|(_, entry)| matches!(entry, Entry::Permission { request_id: id, .. } if *id == request_id))
        {
            let Entry::Permission { resolved, .. } = entry else {
                unreachable!()
            };
            *resolved = Some(option.label.clone());
            self.remeasure_entry(index);
        } else if let Some((index, Entry::Plan { approval: Some(approval), .. })) = self
            .entries
            .iter_mut()
            .enumerate()
            .rev()
            .find(|(_, entry)| {
                matches!(
                    entry,
                    Entry::Plan {
                        approval: Some(PlanApproval {
                            request_id: id,
                            ..
                        }),
                        ..
                    } if *id == request_id
                )
            })
        {
            approval.resolved = Some(option.label.clone());
            self.remeasure_entry(index);
        }
        if let Some(client) = &self.client {
            let _ = client.respond_permission(request_id, option.id.clone());
        }
        cx.notify();
    }

    /// F-CHAT-25: submits the typed answer to a pending question. The text
    /// rides the selected-option channel; the transcript records it as the
    /// answer, and the pending state ends.
    fn answer_question_text(&mut self, request_id: u64, text: &str, cx: &mut Context<Self>) {
        let answer = text.trim().to_string();
        if answer.is_empty() {
            return;
        }
        let mut answered = false;
        if let Some((index, Entry::Permission {
            resolved, expired, ..
        })) = self
            .entries
            .iter_mut()
            .enumerate()
            .rev()
            .find(|(_, entry)| matches!(entry, Entry::Permission { request_id: id, .. } if *id == request_id))
            && resolved.is_none()
            && !*expired
        {
            *resolved = Some(answer.clone());
            self.remeasure_entry(index);
            answered = true;
        }
        if answered && let Some(client) = &self.client {
            let _ = client.respond_permission(request_id, answer);
        }
        self.clear_question_answer_focus();
        cx.notify();
    }

    /// F-CHAT-25: withdraws a pending question without choosing. The card
    /// reads as no longer answerable; the agent is told the decision was
    /// cancelled.
    fn cancel_question(&mut self, request_id: u64, cx: &mut Context<Self>) {
        let mut cancelled = false;
        if let Some((index, entry)) = self
            .entries
            .iter_mut()
            .enumerate()
            .rev()
            .find(|(_, entry)| matches!(entry, Entry::Permission { request_id: id, .. } if *id == request_id))
        {
            let Entry::Permission { expired, .. } = entry else {
                unreachable!()
            };
            if !*expired {
                *expired = true;
                self.remeasure_entry(index);
                cancelled = true;
            }
        } else if let Some((index, Entry::Plan {
            approval: Some(approval),
            ..
        })) = self
            .entries
            .iter_mut()
            .enumerate()
            .rev()
            .find(|(_, entry)| {
                matches!(
                    entry,
                    Entry::Plan {
                        approval: Some(PlanApproval {
                            request_id: id,
                            ..
                        }),
                        ..
                    } if *id == request_id
                )
            })
            && !approval.expired
        {
            approval.expired = true;
            self.remeasure_entry(index);
            cancelled = true;
        }
        if cancelled && let Some(client) = &self.client {
            let _ = client.cancel_permission(request_id);
        }
        self.clear_question_answer_focus();
        cx.notify();
    }

    /// Dismisses a permission request that has no renderable answer. ACP has
    /// no `dismissed` outcome, so the wire response is explicitly Cancelled;
    /// it is never mapped to a rejection option.
    fn dismiss_permission(&mut self, request_id: u64, cx: &mut Context<Self>) {
        let mut dismissed = false;
        if let Some((
            index,
            Entry::Permission {
                expired,
                dismissed: was_dismissed,
                resolved,
                ..
            },
        )) = self
            .entries
            .iter_mut()
            .enumerate()
            .rev()
            .find(|(_, entry)| {
                matches!(
                    entry,
                    Entry::Permission {
                        request_id: id,
                        resolved: None,
                        expired: false,
                        ..
                    } if *id == request_id
                )
            })
            && resolved.is_none()
        {
            *expired = true;
            *was_dismissed = true;
            self.remeasure_entry(index);
            dismissed = true;
        }
        if dismissed && let Some(client) = &self.client {
            let _ = client.cancel_permission(request_id);
        }
        self.clear_question_answer_focus();
        cx.notify();
    }

    /// F-CHAT-27: a finished turn (or a dead transport) leaves every
    /// unanswered question permanently unanswerable, so the surface never
    /// sits in a wait nothing can resolve.
    fn expire_unanswered(&mut self) {
        for entry in &mut self.entries {
            match entry {
                Entry::Permission {
                    resolved: None,
                    expired,
                    ..
                } => *expired = true,
                Entry::Plan {
                    approval:
                        Some(PlanApproval {
                            resolved: None,
                            expired,
                            ..
                        }),
                    ..
                } => *expired = true,
                _ => {}
            }
        }
        self.clear_question_answer_focus();
    }

    /// The first unanswered question in the transcript, with its entry
    /// index — the pending bar lives exactly while this is Some (F-CHAT-26).
    fn pending_question(&self) -> Option<(usize, String)> {
        self.entries
            .iter()
            .enumerate()
            .find_map(|(index, entry)| match entry {
                Entry::Permission {
                    title,
                    resolved: None,
                    expired: false,
                    ..
                } => Some((index, title.clone())),
                Entry::Plan {
                    approval:
                        Some(PlanApproval {
                            title,
                            resolved: None,
                            expired: false,
                            ..
                        }),
                    ..
                } => Some((index, title.clone())),
                _ => None,
            })
    }

    fn clear_question_answer_focus(&mut self) {
        self.question_answer.draft.clear();
        self.question_answer.for_request = None;
    }

    fn insert_text(&mut self, text: &str, cx: &mut Context<Self>) {
        // F-CHAT-05: the composer is out of service while a permission/plan
        // question is unanswered — the whole editor is disabled, not just
        // Send, mirroring the Swift original's `.disabled(!canInteract)`.
        if self.pending_question().is_some() {
            return;
        }
        // F-CHAT-05: same rule, offline half — `canInteract` excludes
        // `.disconnected` too, so a disconnected composer refuses typed
        // characters exactly like it refuses them during permission-wait.
        // Returning here before ever touching `self.composer` is what keeps
        // this safe for a draft typed *before* the connection dropped: see
        // `offline_enter_never_discards_the_typed_draft`.
        if self.is_offline() {
            return;
        }
        // F-CHAT-16: the only remaining path into the composer while the
        // model picker is open is Shift+Enter's bound `Newline` action
        // (plain typing is already redirected to `model_search` in
        // `on_composer_key`, before it ever reaches here).
        if self.model_picker_open {
            return;
        }
        self.composer.insert_text(text);
        self.refresh_token_popups(cx);
    }

    /// Recomputes the token-driven popup state after any edit: the slash
    /// token resets its dismissal/selection, the mention token starts or
    /// cancels its candidate walk.
    fn refresh_token_popups(&mut self, cx: &mut Context<Self>) {
        let slash_token = self.composer.slash_token();
        if slash_token != self.last_slash_token {
            self.last_slash_token = slash_token;
            self.slash_dismissed = false;
            self.slash_selected = 0;
        }
        let mention_token = self.composer.mention_token();
        if mention_token != self.mention_query {
            self.mention_candidates.clear();
            self.mention_query = mention_token;
            self.schedule_mention_walk(cx);
        }
    }

    fn send_action(&mut self, _: &Send, _: &mut Window, cx: &mut Context<Self>) {
        // F-CHAT-16: Enter is bound to `Send` for the whole "ChatComposer"
        // context, which the model picker's search field inherits (it has
        // no keybinding of its own to shadow it) — without this, pressing
        // Enter while typing a search would actually send the composer's
        // draft.
        if self.model_picker_open {
            return;
        }
        if self.slash_popup_visible() {
            self.accept_slash_selection(cx);
            cx.notify();
            return;
        }
        self.send(cx);
    }

    fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        self.insert_text("\n", cx);
        cx.notify();
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        if self.model_picker_open {
            self.model_picker_open = false;
            cx.notify();
        } else if self.mode_picker_open {
            self.mode_picker_open = false;
            cx.notify();
        } else if self.context_popover_open {
            self.context_popover_open = false;
            cx.notify();
        } else if self.overflow_open {
            self.overflow_open = false;
            cx.notify();
        } else if self.slash_popup_visible() {
            self.slash_dismissed = true;
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

    fn start_connection(&mut self, cx: &mut Context<Self>) {
        if self.connecting {
            return;
        }

        self.client.take();
        self.connecting = true;
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
                    let initial_mode_catalog = client.mode_catalog();
                    let _ = this.update(cx, |chat, _| {
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
                        chat.mode_catalog = initial_mode_catalog;
                        chat.connecting = false;
                    });

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
                            chat.expire_unanswered();
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
                    let (message, kind) =
                        classify_connection_error(format!("could not launch ACP agent: {error:#}"));
                    let _ = this.update(cx, |chat, cx| {
                        chat.connecting = false;
                        chat.client = None;
                        chat.streaming = false;
                        chat.push_entry(Entry::Error {
                            message,
                            retryable: true,
                            kind,
                        });
                        cx.notify();
                    });
                }
            }
        }));
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        self.start_connection(cx);
    }

    #[cfg(test)]
    fn from_test_command(command: AgentCommand, cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        let mut chat = Self::new(command, cwd, cx);
        chat.start_connection(cx);
        chat
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        // F-CHAT-16: Backspace is a bound action (chat-root's own
        // `.on_action(Backspace)`), not a raw key `on_composer_key` ever
        // sees, so its model-picker redirect has to live here instead —
        // otherwise it would silently eat a character from the composer's
        // draft while the user thinks they're correcting a search typo.
        if self.model_picker_open {
            self.model_search.pop();
            cx.notify();
            return;
        }
        // F-CHAT-05: see `insert_text` — the editor is fully disabled while
        // a permission/plan question is unanswered, or while disconnected.
        if self.pending_question().is_some() || self.is_offline() {
            return;
        }
        self.composer.backspace();
        self.refresh_token_popups(cx);
        cx.notify();
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        // F-CHAT-05: see `insert_text` — the editor is fully disabled while
        // a permission/plan question is unanswered, or while disconnected.
        if self.pending_question().is_some() || self.is_offline() {
            return;
        }
        self.composer.delete_forward();
        self.refresh_token_popups(cx);
        cx.notify();
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.composer.move_left(false);
        cx.notify();
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.composer.move_right(false);
        cx.notify();
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.composer.move_left(true);
        cx.notify();
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.composer.move_right(true);
        cx.notify();
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.composer.select_all();
        cx.notify();
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.composer.move_home(false);
        cx.notify();
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.composer.move_end(false);
        cx.notify();
    }

    fn on_composer_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // F-CHAT-25: while a question answer field holds focus, every key
        // belongs to the draft — Enter sends the answer, Escape cancels the
        // question, printable characters type. This mirrors the composer's
        // own fallback handling below for hosts without the keymap.
        if let Some(request_id) = self.question_answer.for_request {
            if matches!(event.keystroke.key.as_str(), "enter" | "return") {
                let draft = self.question_answer.draft.clone();
                self.answer_question_text(request_id, &draft, cx);
            } else if event.keystroke.key == "escape" {
                self.cancel_question(request_id, cx);
            } else if let Some(character) = event.keystroke.key_char.as_deref()
                && !event.keystroke.modifiers.platform
                && !event.keystroke.modifiers.control
                && character != "\n"
            {
                self.question_answer.draft.push_str(character);
                cx.notify();
            }
            return;
        }
        // F-CHAT-16: while the model picker is open, every key belongs to
        // its search field, not the composer — printable characters type
        // (Backspace has its own guard, since it's a bound action rather
        // than a raw key this handler ever sees), Escape closes the picker
        // the same way its own `Cancel` action binding does.
        if self.model_picker_open {
            if event.keystroke.key == "escape" {
                self.model_picker_open = false;
                cx.notify();
            } else if let Some(character) = event.keystroke.key_char.as_deref()
                && !event.keystroke.modifiers.platform
                && !event.keystroke.modifiers.control
                && character != "\n"
            {
                self.model_search.push_str(character);
                cx.notify();
            }
            return;
        }
        if event.keystroke.key == "c"
            && event.keystroke.modifiers.control
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
                self.insert_text("\n", cx);
                cx.notify();
            } else if self.slash_popup_visible() {
                self.accept_slash_selection(cx);
                cx.notify();
            } else {
                self.send(cx);
            }
            return;
        }
        if event.keystroke.key == "escape" {
            if self.slash_popup_visible() {
                self.slash_dismissed = true;
                cx.notify();
            } else {
                self.cancel_turn(cx);
            }
            return;
        }
        // Slash-popup keyboard navigation: the popup keeps the composer's
        // focus so typing keeps filtering, and up/down/tab steer the
        // selection. Tab may be consumed by focus traversal in some hosts;
        // Enter (bound to `Send`, handled above) accepts there too.
        if self.slash_popup_visible() && !event.keystroke.modifiers.shift {
            match event.keystroke.key.as_str() {
                "up" => {
                    self.slash_selected = self.slash_selected.saturating_sub(1);
                    cx.notify();
                    return;
                }
                "down" => {
                    let last = self.slash_candidates().len().saturating_sub(1);
                    self.slash_selected = (self.slash_selected + 1).min(last);
                    cx.notify();
                    return;
                }
                "tab" => {
                    self.accept_slash_selection(cx);
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }

        if let Some(character) = event.keystroke.key_char.as_deref()
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
            && character != "\n"
        {
            self.insert_text(character, cx);
            cx.notify();
        }
    }

    /// F-CHAT-25: the keymap path for Enter while the answer field is
    /// focused (the raw-key fallback lives in `on_composer_key`).
    fn send_answer_action(&mut self, _: &SendAnswer, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(request_id) = self.question_answer.for_request {
            let draft = self.question_answer.draft.clone();
            self.answer_question_text(request_id, &draft, cx);
        }
        cx.notify();
    }

    /// F-CHAT-25: the keymap path for Escape while the answer field is
    /// focused.
    fn cancel_answer_action(&mut self, _: &CancelAnswer, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(request_id) = self.question_answer.for_request {
            self.cancel_question(request_id, cx);
        }
        cx.notify();
    }

    /// F-CHAT-25: the answer row of a question card — a text field with the
    /// question's draft, a Send control, and a Cancel control. The field
    /// owns the `ChatQuestionAnswer` key context while focused.
    fn render_question_answer_row(
        request_id: u64,
        input: &AnswerTextInput,
        theme: &Theme,
        entity: Entity<Self>,
        question_answer: &QuestionAnswerState,
    ) -> AnyElement {
        let colors = theme.colors;
        let typography = theme.typography;
        let field_entity = entity.clone();
        let send_entity = entity.clone();
        let cancel_entity = entity.clone();
        let placeholder = input.placeholder();
        let prefill_for_click = input.prefill.clone();

        // The field is a display of `question_draft` while it owns focus;
        // clicking it seeds the draft from the declared prefill when
        // nothing has been typed yet.
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .id(("question-answer-input", request_id))
                    .debug_selector(|| "question-answer-input".into())
                    .key_context("ChatQuestionAnswer")
                    .track_focus(&question_answer.focus)
                    .px(px(8.0))
                    .py(px(5.0))
                    .flex_1()
                    .rounded(theme.radii.control)
                    .bg(colors.raised)
                    .border_1()
                    .border_color(if question_answer.for_request == Some(request_id) {
                        colors.accent
                    } else {
                        colors.hairline
                    })
                    .text_size(typography.headline)
                    .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                        let prefill = prefill_for_click.clone();
                        field_entity.update(cx, |chat, cx| {
                            chat.question_answer.focus.focus(window, cx);
                            if chat.question_answer.for_request != Some(request_id) {
                                chat.question_answer.for_request = Some(request_id);
                                if chat.question_answer.draft.is_empty()
                                    && let Some(prefill) = prefill
                                {
                                    chat.question_answer.draft = prefill;
                                }
                                cx.notify();
                            }
                        });
                    })
                    .child(if question_answer.draft.is_empty() {
                        div()
                            .text_color(colors.meta)
                            .child(placeholder)
                            .into_any_element()
                    } else {
                        div()
                            .text_color(colors.title)
                            .child(question_answer.draft.clone())
                            .into_any_element()
                    }),
            )
            .child(
                div()
                    .id(("question-answer-send", request_id))
                    .debug_selector(|| "question-answer-send".into())
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(theme.radii.control)
                    .bg(colors.primary_pill_bg)
                    .text_size(typography.footnote)
                    .text_color(colors.title)
                    .hover(|style| style.bg(colors.chat_row_hover))
                    .on_click(move |_, _, cx| {
                        send_entity.update(cx, |chat, cx| {
                            let draft = chat.question_answer.draft.clone();
                            chat.answer_question_text(request_id, &draft, cx);
                        });
                    })
                    .child("Send"),
            )
            .child(
                div()
                    .id(("question-answer-cancel", request_id))
                    .debug_selector(|| "question-answer-cancel".into())
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(theme.radii.control)
                    .text_size(typography.footnote)
                    .text_color(colors.git_conflict)
                    .hover(|style| style.bg(colors.chat_row_hover))
                    .on_click(move |_, _, cx| {
                        cancel_entity.update(cx, |chat, cx| {
                            chat.cancel_question(request_id, cx);
                        });
                    })
                    .child("Cancel"),
            )
            .into_any_element()
    }

    fn render_markdown(
        document: Document,
        theme: &Theme,
        interaction: Option<TranscriptInteraction>,
        source_start: usize,
        link_click: Option<LinkClickOverride>,
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
                        link_click.clone(),
                    );
                    *block_start += block.plain_text().len() + 2;
                    Some(rendered)
                },
            ))
            .into_any_element()
    }

    /// Shared markdown renderer used by file tabs. Keeping this entry point
    /// on `Chat` means file tabs use the same structured `tiller_markdown`
    /// tree and styling as the transcript rather than growing a second
    /// markdown renderer.
    ///
    /// Every rendered link's click is routed through `link_click` instead of
    /// the transcript's default `cx.open_url` (F-CORE-FILE-04). File Preview
    /// mode uses this so a relative link resolves against the open file's
    /// directory and opens as a tab, the same way the Code-mode +
    /// platform-click path already does — Preview is the file view's
    /// default mode, so it must not fall back to unconditionally handing
    /// every link to `cx.open_url`.
    pub(crate) fn render_markdown_document_with_link_override(
        document: Document,
        theme: &Theme,
        link_click: LinkClickOverride,
    ) -> AnyElement {
        Self::render_markdown(document, theme, None, 0, Some(link_click))
    }

    fn render_markdown_block(
        block: Block,
        theme: &Theme,
        id: String,
        interaction: Option<&TranscriptInteraction>,
        source_start: usize,
        link_click: Option<LinkClickOverride>,
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
                    link_click.clone(),
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
                    link_click.clone(),
                ))
                .into_any_element(),
            Block::List { kind, items, .. } => Self::render_markdown_list(
                kind,
                items,
                theme,
                id,
                0,
                source_start,
                interaction,
                link_click.clone(),
            ),
            Block::BlockQuote { blocks } => div()
                .w_full()
                .flex()
                .border_l_2()
                .border_color(colors.subtitle)
                .pl(px(12.0))
                .child(div().w_full().flex().flex_col().gap(px(7.0)).children(
                    blocks.into_iter().enumerate().scan(
                        source_start,
                        move |block_start, (index, block)| {
                            let rendered = Self::render_markdown_block(
                                block.clone(),
                                theme,
                                format!("{id}-quote-{index}"),
                                interaction,
                                *block_start,
                                link_click.clone(),
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
                let fence_language = language
                    .as_deref()
                    .map(Self::language_from_fence_tag)
                    .unwrap_or(Language::PlainText);
                let language_label = language.unwrap_or_else(|| "code".to_string());
                let language_label = if open {
                    format!("{language_label} · streaming")
                } else {
                    language_label
                };
                let code_copy = interaction.and_then(|interaction| {
                    let entry = interaction.entry_index?;
                    let target = CopyTarget::CodeBlock {
                        entry,
                        block: id.clone(),
                    };
                    let copied = interaction.copied_target.as_ref() == Some(&target);
                    let selector = format!("code-block-copy-{entry}-{id}");
                    let confirmation_selector = format!("code-block-copy-confirmed-{entry}-{id}");
                    let copy_entity = interaction.chat.clone();
                    let copy_target = target.clone();
                    let copy_text = text.clone();
                    let mut button = div()
                        .id(selector.clone())
                        .debug_selector(move || selector.clone())
                        .px(px(7.0))
                        .py(px(4.0))
                        .rounded(theme.radii.control)
                        .text_size(typography.footnote)
                        .text_color(colors.meta)
                        .cursor(CursorStyle::PointingHand)
                        .hover(|style| style.bg(colors.chat_row_hover))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            copy_entity.update(cx, |chat, cx| {
                                chat.copy_local_text(copy_target.clone(), copy_text.clone(), cx);
                            });
                        });
                    if copied {
                        button = button.child(
                            div()
                                .id(confirmation_selector.clone())
                                .debug_selector(move || confirmation_selector.clone())
                                .child("Copied ✓"),
                        );
                    } else {
                        button = button.child("Copy");
                    }
                    Some(button.into_any_element())
                });
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
                            .flex()
                            .items_center()
                            .text_size(typography.footnote)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.meta)
                            .child(div().flex_1().child(Self::render_plain_text(
                                language_label.clone(),
                                theme,
                                format!("{id}-language"),
                                source_start,
                                interaction,
                            )))
                            .children(code_copy),
                    )
                    .child(
                        div()
                            .font_family(typography.code_family)
                            .text_size(typography.code_size)
                            .line_height(typography.code_line_height)
                            .text_color(colors.primary_text_color)
                            .child(Self::render_highlighted_code(
                                text,
                                fence_language,
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
                link_click,
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

    /// F-EDIT-07: a fenced code block's rendered text, but with per-token
    /// highlighting — the same `code_spans` pass the editor's own code
    /// surface (`file_view.rs`) uses, so keywords/literals/comments in a
    /// Markdown preview's fenced block no longer render as flat plain text
    /// (the defect: `render_plain_text` applies zero `HighlightStyle`s).
    fn render_highlighted_code(
        text: String,
        language: Language,
        theme: &Theme,
        id: String,
        source_start: usize,
        interaction: Option<&TranscriptInteraction>,
    ) -> AnyElement {
        let mut highlights: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
        let mut offset = 0usize;
        for line in text.split_inclusive('\n') {
            let trimmed = line.strip_suffix('\n').unwrap_or(line);
            for span in code_spans(language, trimmed) {
                let color = match span.kind {
                    CodeSpanKind::Keyword => theme.colors.accent,
                    CodeSpanKind::Literal => theme.colors.diff_addition,
                    CodeSpanKind::Comment => theme.colors.meta,
                };
                highlights.push((
                    offset + span.range.start..offset + span.range.end,
                    HighlightStyle {
                        color: Some(color.into()),
                        ..Default::default()
                    },
                ));
            }
            offset += line.len();
        }
        highlights.sort_by_key(|(range, _)| range.start);
        let styled = StyledText::new(text.clone()).with_highlights(highlights);
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

    /// Maps a fenced code block's info-string language tag (as written
    /// after the opening ` ``` `) to the editor's [`Language`] so the
    /// preview's highlighter picks the right keyword vocabulary. Unknown or
    /// absent tags fall back to `PlainText`, same as the editor.
    fn language_from_fence_tag(tag: &str) -> Language {
        match tag.trim().to_ascii_lowercase().as_str() {
            "rust" | "rs" => Language::Rust,
            "python" | "py" => Language::Python,
            "javascript" | "js" | "jsx" | "mjs" | "cjs" => Language::JavaScript,
            "typescript" | "ts" | "tsx" => Language::TypeScript,
            "bash" | "sh" | "shell" | "zsh" | "fish" | "console" => Language::Shell,
            "json" => Language::Json,
            "yaml" | "yml" => Language::Yaml,
            "toml" => Language::Toml,
            "c" | "h" => Language::C,
            "cpp" | "c++" | "cc" | "cxx" | "hpp" => Language::Cpp,
            "go" | "golang" => Language::Go,
            "swift" => Language::Swift,
            "kotlin" | "kt" => Language::Kotlin,
            "java" => Language::Java,
            "ruby" | "rb" => Language::Ruby,
            "php" => Language::Php,
            "html" | "htm" => Language::Html,
            "css" => Language::Css,
            "sql" => Language::Sql,
            "xml" => Language::Xml,
            "lua" => Language::Lua,
            "zig" => Language::Zig,
            "markdown" | "md" => Language::Markdown,
            _ => Language::PlainText,
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
        link_click: Option<LinkClickOverride>,
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
                                    link_click.clone(),
                                ),
                                block => Self::render_markdown_block(
                                    block,
                                    theme,
                                    format!("{id}-{index}-{block_index}"),
                                    interaction,
                                    *block_start,
                                    link_click.clone(),
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
        link_click: Option<LinkClickOverride>,
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
                            link_click.clone(),
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
        link_click: Option<LinkClickOverride>,
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
                .on_click(link_ranges, move |index, window, cx| {
                    if let Some(target) = link_targets.get(index) {
                        // F-CORE-FILE-04: File Preview supplies an override
                        // that tries to resolve the link as another local
                        // file first; assistant-authored prose (no override)
                        // keeps opening every link externally.
                        match &link_click {
                            Some(handler) => handler(target, window, cx),
                            None => cx.open_url(target),
                        }
                    }
                })
                .into_any_element()
        };
        div().w_full().child(text).into_any_element()
    }

    /// F-CHAT-23: a tool call's text output, tail-truncated (large MCP
    /// results in particular can run long, and the tail is where the
    /// result usually lands) rather than shown in full.
    fn render_tool_output_text(text: &str, theme: &Theme) -> AnyElement {
        let colors = theme.colors;
        let typography = theme.typography;
        let (shown, truncated) = truncate_tool_output(text);
        let mut column = div()
            .w_full()
            .flex()
            .flex_col()
            .rounded(theme.radii.code_block)
            .bg(colors.code_inset_fill)
            .px(px(10.0))
            .py(px(6.0))
            .gap(px(4.0));
        if truncated {
            column = column.child(
                div()
                    .text_size(typography.caption2)
                    .text_color(colors.meta)
                    .child(format!("Showing last {TOOL_OUTPUT_MAX_CHARS} characters")),
            );
        }
        column
            .child(
                div()
                    .font_family(typography.code_family)
                    .text_size(typography.code_size)
                    .line_height(typography.code_line_height)
                    .text_color(colors.primary_text_color)
                    .child(shown),
            )
            .into_any_element()
    }

    /// F-CHAT-31: a diff preview for a tool call that changed a file —
    /// removed lines then added lines at each point of divergence, capped
    /// so one huge rewrite cannot make the transcript unusable.
    fn render_tool_diff(diff: &ToolCallDiff, theme: &Theme) -> AnyElement {
        let colors = theme.colors;
        let typography = theme.typography;
        let lines = diff_preview_lines(diff.old_text.as_deref(), &diff.new_text);
        let total = lines.len();
        let shown = lines.into_iter().take(DIFF_PREVIEW_MAX_LINES);
        let mut column = div()
            .w_full()
            .flex()
            .flex_col()
            .rounded(theme.radii.code_block)
            .bg(colors.code_inset_fill)
            .py(px(6.0))
            .child(
                div()
                    .text_size(typography.footnote)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors.meta)
                    .px(px(10.0))
                    .pb(px(4.0))
                    .child(diff.path.display().to_string()),
            );
        for line in shown {
            let (prefix, text, text_color, background) = match line {
                DiffLine::Context(text) => (" ", text, colors.primary_text_color, None),
                DiffLine::Removed(text) => (
                    "-",
                    text,
                    colors.diff_deletion,
                    Some(colors.diff_deletion_background),
                ),
                DiffLine::Added(text) => (
                    "+",
                    text,
                    colors.diff_addition,
                    Some(colors.diff_addition_background),
                ),
            };
            let mut row = div()
                .flex()
                .px(px(10.0))
                .font_family(typography.code_family)
                .text_size(typography.code_size)
                .line_height(typography.code_line_height)
                .text_color(text_color)
                .child(format!("{prefix} {text}"));
            if let Some(background) = background {
                row = row.bg(background);
            }
            column = column.child(row);
        }
        if total > DIFF_PREVIEW_MAX_LINES {
            column = column.child(
                div()
                    .text_size(typography.caption2)
                    .text_color(colors.meta)
                    .px(px(10.0))
                    .pt(px(4.0))
                    .child(format!("… {} more lines", total - DIFF_PREVIEW_MAX_LINES)),
            );
        }
        column.into_any_element()
    }

    /// F-CHAT-32: an edit tool call ends with an actionable file summary.
    /// The diff remains available above it; this card is the post-hoc path
    /// for opening the file or deliberately discarding the reported change.
    fn render_edit_summary(
        entry: usize,
        diffs: Vec<ToolCallDiff>,
        state: EditSummaryState,
        theme: &Theme,
        entity: Entity<Self>,
    ) -> AnyElement {
        let colors = theme.colors;
        let typography = theme.typography;
        let mut card = div()
            .id(("edit-summary", entry))
            .debug_selector(move || format!("edit-summary-{entry}"))
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .rounded(theme.radii.code_block)
            .bg(colors.raised)
            .px(px(CARD_H_PADDING))
            .py(px(CARD_V_PADDING))
            .child(
                div()
                    .text_size(typography.footnote)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors.subtitle)
                    .child(if diffs.len() == 1 {
                        "1 file changed".to_string()
                    } else {
                        format!("{} files changed", diffs.len())
                    }),
            );
        for (index, diff) in diffs.into_iter().enumerate() {
            let path = diff.path;
            let open_path = path.clone();
            let request_path = path.clone();
            let confirm_path = path.clone();
            let reverted = state.reverted_paths.contains(&path);
            let confirming = state.confirming_path.as_ref() == Some(&path);
            let reverting = state.reverting_path.as_ref() == Some(&path);
            let open_entity = entity.clone();
            let request_entity = entity.clone();
            let confirm_entity = entity.clone();
            let cancel_entity = entity.clone();
            let mut row = div().flex().items_center().gap(px(8.0)).child(
                div()
                    .id(format!("edit-summary-open-{entry}-{index}"))
                    .debug_selector(move || format!("edit-summary-open-{entry}-{index}"))
                    .flex_1()
                    .text_size(typography.footnote)
                    .text_color(colors.accent)
                    .cursor(CursorStyle::PointingHand)
                    .hover(|style| style.text_color(colors.title))
                    .on_click(move |_, _, cx| {
                        open_entity.update(cx, |_, cx| {
                            cx.emit(ChatEvent::OpenFile(open_path.clone()));
                        });
                    })
                    .child(path.display().to_string()),
            );
            if reverted {
                row = row.child(
                    div()
                        .id(format!("edit-summary-reverted-{entry}-{index}"))
                        .debug_selector(move || format!("edit-summary-reverted-{entry}-{index}"))
                        .text_size(typography.caption2)
                        .text_color(colors.meta)
                        .child("reverted"),
                );
            } else if confirming {
                row = row
                    .child(
                        div()
                            .id(format!("edit-summary-confirm-{entry}-{index}"))
                            .debug_selector(move || format!("edit-summary-confirm-{entry}-{index}"))
                            .px(px(7.0))
                            .py(px(4.0))
                            .rounded(theme.radii.control)
                            .text_size(typography.footnote)
                            .text_color(colors.git_conflict)
                            .hover(|style| style.bg(colors.chat_row_hover))
                            .on_click(move |_, _, cx| {
                                confirm_entity.update(cx, |chat, cx| {
                                    chat.confirm_edit_revert(entry, confirm_path.clone(), cx);
                                });
                            })
                            .child("Confirm Revert"),
                    )
                    .child(
                        div()
                            .id(format!("edit-summary-cancel-{entry}-{index}"))
                            .debug_selector(move || format!("edit-summary-cancel-{entry}-{index}"))
                            .px(px(7.0))
                            .py(px(4.0))
                            .rounded(theme.radii.control)
                            .text_size(typography.footnote)
                            .text_color(colors.meta)
                            .hover(|style| style.bg(colors.chat_row_hover))
                            .on_click(move |_, _, cx| {
                                cancel_entity.update(cx, |chat, cx| {
                                    chat.cancel_edit_revert(entry, cx);
                                });
                            })
                            .child("Cancel"),
                    );
            } else {
                row = row.child(
                    div()
                        .id(format!("edit-summary-revert-{entry}-{index}"))
                        .debug_selector(move || format!("edit-summary-revert-{entry}-{index}"))
                        .px(px(7.0))
                        .py(px(4.0))
                        .rounded(theme.radii.control)
                        .text_size(typography.footnote)
                        .text_color(if reverting {
                            colors.meta
                        } else {
                            colors.git_conflict
                        })
                        .hover(|style| style.bg(colors.chat_row_hover))
                        .on_click(move |_, _, cx| {
                            if !reverting {
                                request_entity.update(cx, |chat, cx| {
                                    chat.request_edit_revert(entry, request_path.clone(), cx);
                                });
                            }
                        })
                        .child(if reverting { "Reverting…" } else { "Revert" }),
                );
            }
            card = card.child(row);
        }
        if let Some(error) = state.revert_error {
            card = card.child(
                div()
                    .id(("edit-summary-error", entry))
                    .debug_selector(move || format!("edit-summary-error-{entry}"))
                    .text_size(typography.caption2)
                    .text_color(colors.git_conflict)
                    .child(error),
            );
        }
        card.into_any_element()
    }

    fn render_entry(
        entry: Entry,
        entry_index: usize,
        theme: &Theme,
        entity: gpui::Entity<Self>,
        transcript_focus: FocusHandle,
        source_start: usize,
        question_answer: &QuestionAnswerState,
        copied_target: Option<CopyTarget>,
        edit_summary: Option<EditSummaryState>,
    ) -> impl IntoElement {
        let colors = theme.colors;
        let typography = theme.typography;
        let interaction = TranscriptInteraction {
            chat: entity.clone(),
            focus: transcript_focus,
            entry_index: Some(entry_index),
            copied_target: copied_target.clone(),
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
            Entry::Assistant { text, document } => {
                let target = CopyTarget::Assistant(entry_index);
                let copied = copied_target.as_ref() == Some(&target);
                let hover_group = format!("assistant-response-{entry_index}");
                let copy_entity = entity.clone();
                let copy_target = target.clone();
                let copy_text = text;
                let mut copy = div()
                    .id(("assistant-copy", entry_index))
                    .debug_selector(move || format!("assistant-copy-{entry_index}"))
                    .absolute()
                    .top(px(0.0))
                    .right(px(0.0))
                    .px(px(7.0))
                    .py(px(4.0))
                    .rounded(theme.radii.control)
                    .bg(colors.raised)
                    .text_size(typography.footnote)
                    .text_color(colors.meta)
                    .cursor(CursorStyle::PointingHand)
                    .hover(|style| style.bg(colors.chat_row_hover))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        copy_entity.update(cx, |chat, cx| {
                            chat.copy_local_text(copy_target.clone(), copy_text.clone(), cx);
                        });
                    });
                if copied {
                    copy = copy.child(
                        div()
                            .id(("assistant-copy-confirmed", entry_index))
                            .debug_selector(move || {
                                format!("assistant-copy-confirmed-{entry_index}")
                            })
                            .child("Copied ✓"),
                    );
                } else {
                    copy = copy
                        .invisible()
                        .group_hover(hover_group.clone(), |style| style.visible())
                        .child("Copy");
                }
                div()
                    .id(("assistant-response", entry_index))
                    .debug_selector(move || format!("assistant-response-{entry_index}"))
                    .relative()
                    .group(hover_group)
                    .w_full()
                    .child(Self::render_markdown(
                        document,
                        theme,
                        Some(interaction),
                        source_start,
                        None,
                    ))
                    .child(copy)
                    .into_any_element()
            }
            Entry::Thought { text, expanded } => {
                let toggle_entity = entity.clone();
                let mut column = div().w_full().flex().flex_col().gap(px(4.0)).child(
                    div()
                        .id(("thought-toggle", entry_index))
                        .debug_selector(move || format!("thought-toggle-{entry_index}"))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .cursor(CursorStyle::PointingHand)
                        .hover(|style| style.text_color(colors.title))
                        .child(
                            IconElement::new(
                                if expanded {
                                    Icon::ChevronDown
                                } else {
                                    Icon::ChevronRight
                                },
                                px(10.0),
                            )
                            .text_color(colors.meta),
                        )
                        .child(
                            div()
                                .text_size(typography.callout)
                                .line_height(px(19.0))
                                .text_color(colors.subtitle)
                                .italic()
                                .child(if expanded {
                                    "Thinking".to_string()
                                } else {
                                    thought_summary(&text)
                                }),
                        )
                        .on_click(move |_, _, cx| {
                            toggle_entity.update(cx, |chat, cx| {
                                chat.toggle_thought_expanded(entry_index, cx);
                            });
                        }),
                );
                if expanded {
                    column = column.child(
                        div()
                            .pl(px(14.0))
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
                            )),
                    );
                }
                column.into_any_element()
            }
            Entry::ToolCall {
                title,
                status,
                kind,
                content,
                locations,
                expanded,
                ..
            } => Self::render_tool_call_card(
                entry_index,
                title,
                status,
                kind,
                content,
                locations,
                expanded,
                edit_summary,
                theme,
                entity.clone(),
            ),
            Entry::SubagentTask {
                title,
                status,
                tool_calls,
                expanded,
                ..
            } => Self::render_subagent_task_card(
                entry_index,
                title,
                status,
                tool_calls,
                expanded,
                theme,
                entity.clone(),
            ),
            Entry::Permission {
                request_id,
                title,
                prompt,
                options,
                text_input,
                resolved,
                expired,
                dismissed,
            } => {
                let header = if title.is_empty() {
                    "Permission requested".to_string()
                } else {
                    title
                };
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
                            .child(header),
                    );
                if !prompt.is_empty() {
                    card = card.child(
                        div()
                            .text_size(typography.footnote)
                            .text_color(colors.subtitle)
                            .child(prompt),
                    );
                }
                let unrenderable = options.is_empty()
                    && text_input.is_none()
                    && resolved.is_none()
                    && !expired
                    && !dismissed;
                if let Some(choice) = resolved {
                    card = card.child(
                        div()
                            .text_size(typography.footnote)
                            .text_color(colors.meta)
                            .child(format!("Answered: {choice}")),
                    );
                } else if dismissed {
                    card = card.child(
                        div()
                            .text_size(typography.footnote)
                            .text_color(colors.meta)
                            .child("Dismissed — request cancelled"),
                    );
                } else if expired {
                    // F-CHAT-27: the turn ended unanswered; offering the
                    // buttons again would be a lie.
                    card = card.child(
                        div()
                            .text_size(typography.footnote)
                            .text_color(colors.meta)
                            .child("No answer — the turn ended"),
                    );
                } else if let Some(input) = text_input {
                    card = card.child(Self::render_question_answer_row(
                        request_id,
                        &input,
                        theme,
                        entity.clone(),
                        question_answer,
                    ));
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
                                .text_color(if option.is_rejection {
                                    colors.git_conflict
                                } else {
                                    colors.title
                                })
                                .hover(|style| style.bg(colors.chat_row_hover))
                                .on_click(move |_, _, cx| {
                                    entity.update(cx, |chat, cx| {
                                        chat.respond_permission(request_id, &option_for_click, cx);
                                    });
                                })
                                .child(option.label.clone()),
                        );
                    }
                    card = card.child(row);
                }
                if unrenderable {
                    let dismiss_entity = entity.clone();
                    card = card.child(
                        div()
                            .id(("permission-dismiss", request_id as usize))
                            .debug_selector(move || format!("permission-dismiss-{request_id}"))
                            .px(px(10.0))
                            .py(px(5.0))
                            .rounded(theme.radii.control)
                            .bg(colors.primary_pill_bg)
                            .text_size(typography.footnote)
                            .text_color(colors.title)
                            .hover(|style| style.bg(colors.chat_row_hover))
                            .on_click(move |_, _, cx| {
                                dismiss_entity.update(cx, |chat, cx| {
                                    chat.dismiss_permission(request_id, cx);
                                });
                            })
                            .child("Dismiss"),
                    );
                }
                card.into_any_element()
            }
            Entry::Plan { entries, approval } => {
                let completed = entries
                    .iter()
                    .filter(|entry| entry.status == "completed")
                    .count();
                let mut card = div()
                    .w_full()
                    .rounded(theme.radii.code_block)
                    .bg(colors.card_fill)
                    .border_l_2()
                    .border_color(colors.rail_task)
                    .px(px(CARD_H_PADDING))
                    .py(px(CARD_V_PADDING))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(typography.caption2)
                            .child(div().text_color(colors.title).child("Plan"))
                            .child(
                                div()
                                    .text_color(colors.meta)
                                    .child(format!("{completed}/{}", entries.len())),
                            ),
                    );
                for row in entries {
                    let (glyph, tint) = match row.status.as_str() {
                        "completed" => ("✓", colors.rail_edit),
                        "in_progress" => ("◌", colors.title),
                        _ => ("○", colors.meta),
                    };
                    card = card.child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(6.0))
                            .text_size(typography.callout)
                            .child(div().w(px(14.0)).text_color(tint).child(glyph))
                            .child(div().flex_1().text_color(colors.title).child(row.content)),
                    );
                }
                if let Some(approval) = approval {
                    if let Some(choice) = &approval.resolved {
                        card = card.child(
                            div()
                                .text_size(typography.footnote)
                                .text_color(colors.meta)
                                .child(format!("Approved: {choice}")),
                        );
                    } else if approval.expired {
                        card = card.child(
                            div()
                                .text_size(typography.footnote)
                                .text_color(colors.meta)
                                .child("No answer — the turn ended"),
                        );
                    } else {
                        let mut row = div().flex().gap(px(8.0));
                        for option in &approval.options {
                            let entity = entity.clone();
                            let option_for_click = option.clone();
                            let option_id = option.id.clone();
                            let request_id = approval.request_id;
                            row = row.child(
                                div()
                                    .id((
                                        "permission-option",
                                        request_id as usize ^ option_hash(option),
                                    ))
                                    .debug_selector(move || {
                                        format!("permission-option-{option_id}")
                                    })
                                    .px(px(10.0))
                                    .py(px(5.0))
                                    .rounded(theme.radii.control)
                                    .bg(colors.primary_pill_bg)
                                    .text_size(typography.footnote)
                                    .text_color(if option.is_rejection {
                                        colors.git_conflict
                                    } else {
                                        colors.title
                                    })
                                    .hover(|style| style.bg(colors.chat_row_hover))
                                    .on_click(move |_, _, cx| {
                                        entity.update(cx, |chat, cx| {
                                            chat.respond_permission(
                                                request_id,
                                                &option_for_click,
                                                cx,
                                            );
                                        });
                                    })
                                    .child(option.label.clone()),
                            );
                        }
                        card = card.child(row);
                    }
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
                message,
                retryable,
                kind,
            } => {
                let retry_entity = entity.clone();
                // F-CHAT-02: AuthRequired gets its own amber treatment
                // (matching the connecting/working status-dot color already
                // used elsewhere in this file) instead of the generic red
                // connection-failure card — the fix here is "sign in, then
                // retry", not "the network hiccupped, retry", and the card
                // should look like a different kind of problem.
                let is_auth_required = kind == ErrorKind::AuthRequired;
                let (banner_bg, banner_border, banner_text) = if is_auth_required {
                    (rgb(0xf5a623).opacity(0.12), rgb(0xf5a623), colors.title)
                } else {
                    (
                        colors.diff_deletion_background,
                        colors.diff_deletion,
                        colors.diff_deletion,
                    )
                };
                div()
                    .id(("chat-error-banner", entry_index))
                    .when(is_auth_required, |this| {
                        this.debug_selector(|| "chat-auth-required-banner".into())
                    })
                    .w_full()
                    .rounded(theme.radii.code_block)
                    .bg(banner_bg)
                    .border_l_2()
                    .border_color(banner_border)
                    .px(px(CARD_H_PADDING))
                    .py(px(CARD_V_PADDING))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .text_size(typography.callout)
                    .text_color(banner_text)
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

    fn render_subagent_task_card(
        task_index: usize,
        title: String,
        status: String,
        tool_calls: Vec<SubagentToolCall>,
        expanded: bool,
        theme: &Theme,
        entity: gpui::Entity<Self>,
    ) -> AnyElement {
        let colors = theme.colors;
        let typography = theme.typography;
        let toggle_entity = entity.clone();
        let header = div()
            .id(("subagent-task-toggle", task_index))
            .debug_selector(move || format!("subagent-task-toggle-{task_index}"))
            .px(px(CARD_H_PADDING))
            .py(px(8.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor(CursorStyle::PointingHand)
            .child(
                IconElement::new(
                    if expanded {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    },
                    px(10.0),
                )
                .text_color(colors.meta),
            )
            .child(
                div()
                    .text_size(typography.footnote)
                    .text_color(colors.rail_task)
                    .child("Subagent"),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(typography.callout)
                    .text_color(colors.title)
                    .child(title),
            )
            .child(
                div()
                    .text_size(typography.footnote)
                    .text_color(colors.meta)
                    .child(status),
            )
            .on_click(move |_, _, cx| {
                toggle_entity.update(cx, |chat, cx| {
                    chat.toggle_subagent_task_expanded(task_index, cx);
                });
            });
        let mut card = div()
            .w_full()
            .flex()
            .flex_col()
            .rounded(theme.radii.code_block)
            .bg(colors.card_fill)
            .border_l_2()
            .border_color(colors.rail_task)
            .child(header);
        if expanded {
            for (child_index, call) in tool_calls.into_iter().enumerate() {
                card = card.child(Self::render_subagent_tool_call_card(
                    task_index,
                    child_index,
                    call,
                    theme,
                    entity.clone(),
                ));
            }
        }
        card.into_any_element()
    }

    fn render_subagent_tool_call_card(
        task_index: usize,
        child_index: usize,
        call: SubagentToolCall,
        theme: &Theme,
        entity: gpui::Entity<Self>,
    ) -> AnyElement {
        let colors = theme.colors;
        let typography = theme.typography;
        let SubagentToolCall {
            title,
            status,
            kind,
            content,
            locations,
            expanded,
            ..
        } = call;
        let toggle_entity = entity.clone();
        let header = div()
            .id(format!(
                "subagent-tool-call-toggle-{task_index}-{child_index}"
            ))
            .debug_selector(move || format!("subagent-tool-call-toggle-{task_index}-{child_index}"))
            .pl(px(CARD_H_PADDING + 10.0))
            .pr(px(CARD_H_PADDING))
            .py(px(6.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor(CursorStyle::PointingHand)
            .child(
                IconElement::new(
                    if expanded {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    },
                    px(10.0),
                )
                .text_color(colors.meta),
            )
            .child(
                div()
                    .text_size(typography.footnote)
                    .text_color(colors.meta)
                    .child(kind),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(typography.callout)
                    .text_color(colors.title)
                    .child(title),
            )
            .child(
                div()
                    .text_size(typography.footnote)
                    .text_color(colors.meta)
                    .child(status),
            )
            .on_click(move |_, _, cx| {
                toggle_entity.update(cx, |chat, cx| {
                    chat.toggle_subagent_tool_call_expanded(task_index, child_index, cx);
                });
            });
        let mut card = div().w_full().flex().flex_col().child(header);
        if expanded {
            let mut body = div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .pl(px(CARD_H_PADDING + 26.0))
                .pr(px(CARD_H_PADDING))
                .pb(px(CARD_V_PADDING));
            for item in &content {
                match item {
                    ToolCallContentInfo::Text(text) => {
                        body = body.child(Self::render_tool_output_text(text, theme));
                    }
                    ToolCallContentInfo::Diff(diff) => {
                        body = body.child(Self::render_tool_diff(diff, theme));
                    }
                    ToolCallContentInfo::Other => {}
                }
            }
            if !locations.is_empty() {
                body = body.child(div().flex().flex_wrap().gap(px(8.0)).children(
                    locations.iter().map(|location| {
                        let label = match location.line {
                            Some(line) => format!("{}:{line}", location.path.display()),
                            None => location.path.display().to_string(),
                        };
                        div()
                            .text_size(typography.footnote)
                            .text_color(colors.meta)
                            .child(label)
                    }),
                ));
            }
            card = card.child(body);
        }
        card.into_any_element()
    }

    /// One tool call's card: a chevron-toggle header (kind, title, status)
    /// and, when `expanded`, its content/diff/location detail. Shared by
    /// the single-entry render path and by `render_tool_call_group`'s
    /// expanded view, so a run's members look identical whether they are
    /// standing alone or inside a group.
    fn render_tool_call_card(
        entry_index: usize,
        title: String,
        status: String,
        kind: String,
        content: Vec<ToolCallContentInfo>,
        locations: Vec<ToolCallLocationInfo>,
        expanded: bool,
        edit_summary: Option<EditSummaryState>,
        theme: &Theme,
        entity: gpui::Entity<Self>,
    ) -> AnyElement {
        let colors = theme.colors;
        let typography = theme.typography;
        let toggle_entity = entity.clone();
        let header = div()
            .id(("tool-call-toggle", entry_index))
            .debug_selector(move || format!("tool-call-toggle-{entry_index}"))
            .flex_1()
            .px(px(CARD_H_PADDING))
            .py(px(8.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor(CursorStyle::PointingHand)
            .child(
                IconElement::new(
                    if expanded {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    },
                    px(10.0),
                )
                .text_color(colors.meta),
            )
            .child(
                div()
                    .text_size(typography.footnote)
                    .text_color(colors.meta)
                    .child(kind),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(typography.callout)
                    .text_color(colors.title)
                    .child(title),
            )
            .child(
                div()
                    .text_size(typography.footnote)
                    .text_color(colors.meta)
                    .child(status),
            )
            .on_click(move |_, _, cx| {
                toggle_entity.update(cx, |chat, cx| {
                    chat.toggle_tool_call_expanded(entry_index, cx);
                });
            });
        let mut card = div()
            .w_full()
            .flex()
            .flex_col()
            .rounded(theme.radii.code_block)
            .bg(colors.card_fill)
            .border_l_2()
            .border_color(colors.rail_tool)
            .child(header);
        if expanded {
            let mut body = div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .px(px(CARD_H_PADDING))
                .pb(px(CARD_V_PADDING));
            for item in &content {
                match item {
                    ToolCallContentInfo::Text(text) => {
                        body = body.child(Self::render_tool_output_text(text, theme));
                    }
                    ToolCallContentInfo::Diff(diff) => {
                        body = body.child(Self::render_tool_diff(diff, theme));
                    }
                    ToolCallContentInfo::Other => {}
                }
            }
            if !locations.is_empty() {
                body = body.child(div().flex().flex_wrap().gap(px(8.0)).children(
                    locations.iter().map(|location| {
                        let label = match location.line {
                            Some(line) => {
                                format!("{}:{line}", location.path.display())
                            }
                            None => location.path.display().to_string(),
                        };
                        div()
                            .text_size(typography.footnote)
                            .text_color(colors.meta)
                            .child(label)
                    }),
                ));
            }
            card = card.child(body);
        }
        let diffs = content
            .iter()
            .filter_map(|content| match content {
                ToolCallContentInfo::Diff(diff) => Some(diff.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        if !diffs.is_empty() {
            card = card.child(Self::render_edit_summary(
                entry_index,
                diffs,
                edit_summary.unwrap_or_default(),
                theme,
                entity.clone(),
            ));
        }
        card.into_any_element()
    }

    /// F-CHAT-22: a consecutive run of tool calls, collapsed by default to
    /// one "N steps" header. `members` is every entry in the run in order;
    /// `group_index` is the run's last entry, the only index the transcript
    /// list actually draws a row for (see `tool_call_run_bounds`), so the
    /// header's toggle and its `group_expanded` state both key off it.
    /// Collapsed, each member shows as a compact one-line status row;
    /// expanded, each renders through `render_tool_call_card` exactly as it
    /// would standing alone, keyed by its own transcript index so its own
    /// F-CHAT-23 detail toggle still works independently.
    fn render_tool_call_group(
        members: Vec<(usize, Entry)>,
        group_index: usize,
        group_expanded: bool,
        theme: &Theme,
        entity: gpui::Entity<Self>,
    ) -> AnyElement {
        let colors = theme.colors;
        let typography = theme.typography;
        let count = members.len();
        let toggle_entity = entity.clone();
        let header = div()
            .id(("tool-call-group-toggle", group_index))
            .debug_selector(move || format!("tool-call-group-toggle-{group_index}"))
            .px(px(CARD_H_PADDING))
            .py(px(6.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor(CursorStyle::PointingHand)
            .hover(|style| style.text_color(colors.title))
            .child(
                IconElement::new(
                    if group_expanded {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    },
                    px(10.0),
                )
                .text_color(colors.meta),
            )
            .child(
                div()
                    .text_size(typography.callout)
                    .text_color(colors.subtitle)
                    .italic()
                    .child(format!("{count} steps")),
            )
            .on_click(move |_, _, cx| {
                toggle_entity.update(cx, |chat, cx| {
                    chat.toggle_tool_call_group_expanded(group_index, cx);
                });
            });
        let mut column = div().w_full().flex().flex_col().gap(px(2.0)).child(header);
        if group_expanded {
            for (member_index, member) in members {
                if let Entry::ToolCall {
                    title,
                    status,
                    kind,
                    content,
                    locations,
                    expanded,
                    ..
                } = member
                {
                    column = column.child(Self::render_tool_call_card(
                        member_index,
                        title,
                        status,
                        kind,
                        content,
                        locations,
                        expanded,
                        None,
                        theme,
                        entity.clone(),
                    ));
                }
            }
        } else {
            for (_, member) in members {
                if let Entry::ToolCall { title, status, .. } = member {
                    column = column.child(
                        div()
                            .pl(px(CARD_H_PADDING + 16.0))
                            .text_size(typography.footnote)
                            .text_color(colors.meta)
                            .child(format!("{status} · {title}")),
                    );
                }
            }
        }
        column.into_any_element()
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
        // F-CHAT-15: once a live mode catalog exists, the "Ask" fallback
        // gives way to the agent's own current-mode name — the pill was a
        // hardcoded word before this row, now it reflects what
        // `AcpClient::set_mode` would actually change.
        let live_mode_name = self.mode_catalog.as_ref().and_then(|catalog| {
            catalog
                .options
                .iter()
                .find(|mode| mode.id == catalog.current_id)
                .map(|mode| mode.name.clone())
        });
        let (dot, label) = if connecting {
            (rgb(0xf5a623), "connecting".to_string())
        } else if self.streaming {
            (rgb(0xf5a623), "working".to_string())
        } else if self.has_completed_turn {
            (
                rgb(0x53c653),
                live_mode_name.unwrap_or_else(|| "Ask".into()),
            )
        } else if self.client.is_some() {
            (rgb(0x53c653), "idle".to_string())
        } else {
            (rgb(0x8a8d99), "offline".to_string())
        };
        let mode_selectable = self.has_completed_turn && self.mode_catalog.is_some();
        let status_pill = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(24.0))
            .px(px(7.0))
            .rounded(theme.radii.control)
            .bg(colors.raised)
            .text_size(typography.ui_size);
        let status_pill = if connecting {
            status_pill
                .id("chat-connecting")
                .debug_selector(|| "chat-connecting".into())
        } else {
            status_pill
                .id("chat-status")
                .debug_selector(|| "chat-status".into())
        };
        let status_pill = status_pill
            .when(mode_selectable, |this| {
                this.hover(|style| style.bg(colors.chat_row_hover))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_mode_picker(window, cx);
                    }))
            })
            .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(dot))
            .child(div().text_color(colors.title).child(label))
            .when(self.has_completed_turn, |this| {
                this.child(div().text_color(colors.meta).child("⌄"))
            });

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
        let effort_label = self.effort.as_ref().and_then(|effort| {
            effort.current_value.as_ref().map(|current| {
                let name = effort
                    .choices
                    .iter()
                    .find(|choice| choice.value == *current)
                    .map(|choice| choice.name.clone())
                    .unwrap_or_else(|| current.clone());
                name.to_uppercase()
            })
        });
        // The picker chip — "Model" in muted chrome text, the value in
        // title text, the effort level when one is selected — opens only
        // once a turn has completed and the agent reported models. Without
        // models the pill degrades to a plain agent badge (F-CHAT-36): no
        // label, no chevron, no picker.
        let model_control = if self.has_completed_turn && !self.available_models.is_empty() {
            let effort_for_chip = effort_label.clone();
            div()
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
                .on_click(cx.listener(|this, _, window, cx| {
                    this.toggle_model_picker(window, cx);
                }))
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
                .when_some(effort_for_chip, |this, label| {
                    this.child(
                        div()
                            .id("model-effort-label")
                            .debug_selector(|| "model-effort-label".into())
                            .text_size(typography.caption2)
                            .text_color(colors.meta)
                            .child(label),
                    )
                })
                .child(div().text_color(colors.meta).child("⌄"))
        } else {
            div()
                .id("agent-badge")
                .debug_selector(|| "agent-badge".into())
                .flex()
                .items_center()
                .gap(px(6.0))
                .h(px(24.0))
                .px(px(7.0))
                .rounded(theme.radii.control)
                .bg(colors.raised)
                .text_size(typography.ui_size)
                .child(div().text_color(colors.title).child(selected_model_name))
        };

        let model_picker = if self.model_picker_open {
            let picker_entity = model_entity.clone();
            // F-CHAT-16: "Recommended" is not a protocol flag — `ModelOption`
            // has none, and the ACP layer never carries one — it is purely
            // the driver's own first-listed choice, matching Swift's
            // `recommendedId = models.first?.modelId` exactly. The search
            // filter runs over the full, unfiltered list order (`filter`,
            // not `sort`) so a query never reorders results.
            let recommended_id = self
                .available_models
                .first()
                .map(|option| option.id.clone());
            let filtered_models: Vec<ModelOption> = self
                .available_models
                .iter()
                .filter(|option| model_query_matches(option, &self.model_search))
                .cloned()
                .collect();
            let search_placeholder = self.model_search.is_empty();
            let search_text = self.model_search.clone();
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
                    .when(!self.available_models.is_empty(), |this| {
                        this.child(
                            div()
                                .id("model-search-input")
                                .debug_selector(|| "model-search-input".into())
                                .w_full()
                                .mb(px(6.0))
                                .px(px(8.0))
                                .py(px(5.0))
                                .rounded(theme.radii.control)
                                .bg(colors.raised)
                                .border_1()
                                .border_color(colors.hairline)
                                .text_size(typography.footnote)
                                .child(if search_placeholder {
                                    div()
                                        .text_color(colors.meta)
                                        .child("Search models…")
                                        .into_any_element()
                                } else {
                                    div()
                                        .text_color(colors.title)
                                        .child(search_text)
                                        .into_any_element()
                                }),
                        )
                    })
                    .when(self.available_models.is_empty(), |this| {
                        this.child(
                            div()
                                .p(px(8.0))
                                .text_size(typography.footnote)
                                .text_color(colors.meta)
                                .child("The connected agent did not report any models."),
                        )
                    })
                    .when(
                        !self.available_models.is_empty() && filtered_models.is_empty(),
                        |this| {
                            this.child(
                                div()
                                    .id("model-picker-no-match")
                                    .debug_selector(|| "model-picker-no-match".into())
                                    .p(px(8.0))
                                    .text_size(typography.footnote)
                                    .text_color(colors.meta)
                                    .child("No models match"),
                            )
                        },
                    )
                    .children(filtered_models.iter().cloned().map(|option| {
                        let option_id = option.id.clone();
                        let option_name = option.name.clone();
                        let option_entity = picker_entity.clone();
                        let is_recommended = recommended_id.as_deref() == Some(option_id.as_str());
                        div()
                            .id(format!("model-option-{option_id}"))
                            .debug_selector(move || format!("model-option-{option_id}"))
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
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
                            .child(div().flex_1().child(option_name))
                            .when(is_recommended, |this| {
                                this.child(
                                    div()
                                        .id("model-option-recommended")
                                        .debug_selector(|| "model-option-recommended".into())
                                        .px(px(5.0))
                                        .rounded(px(4.0))
                                        .text_size(typography.caption2)
                                        .text_color(colors.accent)
                                        .bg(colors.accent.opacity(0.15))
                                        .child("Recommended"),
                                )
                            })
                    }))
                    .when_some(self.effort.clone(), |this, effort| {
                        if effort.choices.is_empty() {
                            return this;
                        }
                        let effort_entity = picker_entity.clone();
                        let effort_name =
                            effort.name.clone().unwrap_or_else(|| "Effort".to_string());
                        let current = effort.current_value.clone();
                        let children: Vec<AnyElement> = vec![
                            div()
                                .w_full()
                                .px(px(8.0))
                                .pt(px(4.0))
                                .text_size(typography.caption2)
                                .text_color(colors.meta)
                                .child(effort_name)
                                .into_any_element(),
                        ];
                        let choices: Vec<AnyElement> = effort
                            .choices
                            .iter()
                            .map(|choice| {
                                let choice_value = choice.value.clone();
                                let choice_name = choice.name.clone();
                                let is_selected = current.as_deref() == Some(choice_value.as_str());
                                let row_entity = effort_entity.clone();
                                let choice_value_for_id = choice_value.clone();
                                div()
                                    .id(format!("effort-option-{}", choice_value))
                                    .debug_selector(move || {
                                        format!("effort-option-{}", choice_value_for_id)
                                    })
                                    .h(px(22.0))
                                    .px(px(8.0))
                                    .rounded(theme.radii.control)
                                    .flex()
                                    .items_center()
                                    .text_size(typography.caption2)
                                    .text_color(colors.title)
                                    .when(is_selected, |this| this.bg(colors.selection_fill))
                                    .hover(|style| style.bg(colors.chat_row_hover))
                                    .on_click(move |_, _, cx| {
                                        row_entity.update(cx, |chat, cx| {
                                            chat.select_effort(choice_value.clone(), cx);
                                        });
                                    })
                                    .child(choice_name)
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>();
                        this.child(div().h(px(1.0)).w_full().bg(colors.hairline))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .children(children)
                                    .child(div().flex().gap(px(4.0)).children(choices)),
                            )
                    }),
            )
        } else {
            None
        };

        // F-CHAT-15: the session-mode picker, anchored above the status
        // pill the same way `model_picker` anchors above the model chip.
        // No search field — mode lists are small and entirely agent-defined
        // (ask/plan/auto today), so a flat list of rows is enough.
        let mode_picker = if self.mode_picker_open {
            let mode_entity = entity.clone();
            let current_id = self
                .mode_catalog
                .as_ref()
                .map(|catalog| catalog.current_id.clone())
                .unwrap_or_default();
            let options: Vec<AgentMode> = self
                .mode_catalog
                .as_ref()
                .map(|catalog| catalog.options.clone())
                .unwrap_or_default();
            Some(
                div()
                    .id("mode-picker")
                    .debug_selector(|| "mode-picker".into())
                    .key_context("ChatModelPicker")
                    .track_focus(&self.mode_picker_focus)
                    .on_action(cx.listener(Self::cancel))
                    .absolute()
                    .left(px(0.0))
                    .bottom(px(43.0))
                    .w(px(200.0))
                    .p(px(6.0))
                    .rounded(theme.radii.toast)
                    .bg(colors.card_fill)
                    .border_1()
                    .border_color(colors.hairline)
                    .shadow_lg()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.mode_picker_open = false;
                        cx.notify();
                    }))
                    .when(options.is_empty(), |this| {
                        this.child(
                            div()
                                .p(px(8.0))
                                .text_size(typography.footnote)
                                .text_color(colors.meta)
                                .child("No modes offered"),
                        )
                    })
                    .children(options.into_iter().map(|mode| {
                        let mode_id = mode.id.clone();
                        let mode_name = mode.name.clone();
                        let row_entity = mode_entity.clone();
                        let is_selected = mode.id == current_id;
                        div()
                            .id(format!("mode-option-{mode_id}"))
                            .debug_selector(move || format!("mode-option-{mode_id}"))
                            .w_full()
                            .flex()
                            .items_center()
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(theme.radii.control)
                            .text_size(typography.footnote)
                            .text_color(colors.title)
                            .when(is_selected, |this| this.bg(colors.selection_fill))
                            .hover(|style| style.bg(colors.chat_row_hover))
                            .on_click(move |_, _, cx| {
                                row_entity
                                    .update(cx, |chat, cx| chat.select_mode(mode.clone(), cx));
                            })
                            .child(mode_name)
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
        // Above the 80% warning threshold the arc switches from the gauge
        // blue to the shared danger token, the same rule as the reference
        // `contextRingColor`. The warning wrapper element exists only in
        // that state, so drawn tests can see the colour decision without
        // reading pixels.
        let context_warning = context_amount > 0.8;
        let warning_element = div()
            .id("context-ring-warning")
            .debug_selector(|| "context-ring-warning".into());
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
            .when(context_warning, |this| this.child(warning_element))
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
                                    window.paint_path(
                                        path,
                                        if context_warning {
                                            colors.git_conflict
                                        } else {
                                            colors.gauge
                                        },
                                    );
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
                        // F-CHAT-18: input/output/cache breakdown, present
                        // only for agents that report end-of-turn usage
                        // (`unstable_end_turn_token_usage`) -- absent for
                        // every other agent, so the rows are opt-in rather
                        // than showing zeros.
                        .when(
                            usage.input_tokens.is_some()
                                || usage.output_tokens.is_some()
                                || usage.cached_read_tokens.is_some(),
                            |this| {
                                this.child(
                                    div()
                                        .id("context-usage-breakdown")
                                        .debug_selector(|| "context-usage-breakdown".into())
                                        .mt(px(6.0))
                                        .pt(px(6.0))
                                        .border_t_1()
                                        .border_color(colors.hairline)
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.0))
                                        .when_some(usage.input_tokens, |this, tokens| {
                                            this.child(
                                                div()
                                                    .text_size(typography.caption2)
                                                    .text_color(colors.meta)
                                                    .child(format!("Input: {tokens} tokens")),
                                            )
                                        })
                                        .when_some(usage.output_tokens, |this, tokens| {
                                            this.child(
                                                div()
                                                    .text_size(typography.caption2)
                                                    .text_color(colors.meta)
                                                    .child(format!("Output: {tokens} tokens")),
                                            )
                                        })
                                        .when_some(usage.cached_read_tokens, |this, tokens| {
                                            this.child(
                                                div()
                                                    .text_size(typography.caption2)
                                                    .text_color(colors.meta)
                                                    .child(format!("Cache read: {tokens} tokens")),
                                            )
                                        }),
                                )
                            },
                        )
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
        let stop_entity = entity.clone();
        let attach_entity = entity.clone();
        let overflow_entity = entity.clone();

        // Slash-command popup (F-CHAT-09): a filtered list over the input,
        // opened by the leading `/token`, closed the moment the token is no
        // longer a single unbroken prefix. Keyboard selection comes from the
        // composer key path (up/down/tab, enter accepts via `Send`); click
        // accepts directly.
        let slash_popup =
            if self.slash_popup_visible() {
                let candidates = self.slash_candidates();
                let selected = self.slash_selected.min(candidates.len().saturating_sub(1));
                let slash_entity = entity.clone();
                Some(
                    div()
                        .id("slash-popup")
                        .debug_selector(|| "slash-popup".into())
                        .absolute()
                        .left(px(16.0))
                        .bottom(px(43.0))
                        .w(px(360.0))
                        .p(px(6.0))
                        .rounded(theme.radii.toast)
                        .bg(colors.card_fill)
                        .border_1()
                        .border_color(colors.hairline)
                        .shadow_lg()
                        .children(candidates.into_iter().enumerate().map(
                            move |(index, command)| {
                                let name = command.name.clone();
                                let description = command.description.clone();
                                let row_entity = slash_entity.clone();
                                let is_selected = index == selected;
                                let name_for_id = name.clone();
                                let accept_name = name.clone();
                                div()
                                    .id(format!("slash-option-{name}"))
                                    .debug_selector(move || format!("slash-option-{name_for_id}"))
                                    .w_full()
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .rounded(theme.radii.control)
                                    .flex()
                                    .flex_col()
                                    .when(is_selected, |this| this.bg(colors.selection_fill))
                                    .on_click(move |_, _, cx| {
                                        row_entity.update(cx, |chat, cx| {
                                            chat.accept_slash_command(&accept_name, cx);
                                        });
                                    })
                                    .child(
                                        div()
                                            .text_size(typography.footnote)
                                            .text_color(colors.title)
                                            .child(format!("/{name}")),
                                    )
                                    .child(
                                        div()
                                            .text_size(typography.caption2)
                                            .text_color(colors.meta)
                                            .child(description),
                                    )
                            },
                        )),
                )
            } else {
                None
            };

        // @ file-mention popup (F-CHAT-10): the bounded filesystem walk's
        // results for the trailing `@token`. Hidden when the token matches
        // nothing.
        let mention_popup =
            if self.composer.mention_token().is_some() && !self.mention_candidates.is_empty() {
                let mention_entity = entity.clone();
                let candidates = self.mention_candidates.clone();
                Some(
                    div()
                        .id("mention-popup")
                        .debug_selector(|| "mention-popup".into())
                        .absolute()
                        .left(px(16.0))
                        .bottom(px(43.0))
                        .w(px(360.0))
                        .p(px(6.0))
                        .rounded(theme.radii.toast)
                        .bg(colors.card_fill)
                        .border_1()
                        .border_color(colors.hairline)
                        .shadow_lg()
                        .children(candidates.into_iter().map(move |path| {
                            let row_entity = mention_entity.clone();
                            let path_for_id = path.clone();
                            let path_for_accept = path.clone();
                            div()
                                .id(format!("mention-option-{path_for_id}"))
                                .debug_selector(move || format!("mention-option-{path_for_id}"))
                                .w_full()
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded(theme.radii.control)
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .hover(|style| style.bg(colors.chat_row_hover))
                                .on_click(move |_, _, cx| {
                                    row_entity.update(cx, |chat, cx| {
                                        chat.accept_mention(&path_for_accept, cx);
                                    });
                                })
                                .child(
                                    div()
                                        .text_size(typography.caption2)
                                        .text_color(colors.meta)
                                        .child("▤"),
                                )
                                .child(
                                    div()
                                        .text_size(typography.footnote)
                                        .text_color(colors.title)
                                        .child(path),
                                )
                        })),
                )
            } else {
                None
            };

        // Overflow menu (F-CHAT-14): Follow Edited Files toggle and New
        // Conversation, the two secondary composer actions the control row
        // does not carry inline.
        let overflow_menu = if self.overflow_open {
            let follow_label = if self.following_edited_files {
                "Stop Following"
            } else {
                "Follow Edited Files"
            };
            Some(
                div()
                    .id("composer-overflow-menu")
                    .debug_selector(|| "composer-overflow-menu".into())
                    .key_context("ChatOverflowMenu")
                    .track_focus(&self.overflow_focus)
                    .on_action(cx.listener(Self::cancel))
                    .absolute()
                    .right(px(60.0))
                    .bottom(px(43.0))
                    .w(px(200.0))
                    .p(px(6.0))
                    .rounded(theme.radii.toast)
                    .bg(colors.card_fill)
                    .border_1()
                    .border_color(colors.hairline)
                    .shadow_lg()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.overflow_open = false;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .id("overflow-follow")
                            .debug_selector(|| "overflow-follow".into())
                            .w_full()
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(theme.radii.control)
                            .text_size(typography.footnote)
                            .text_color(colors.title)
                            .hover(|style| style.bg(colors.chat_row_hover))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.following_edited_files = !this.following_edited_files;
                                cx.notify();
                            }))
                            .child(follow_label),
                    )
                    .child(
                        div()
                            .id("overflow-new-conversation")
                            .debug_selector(|| "overflow-new-conversation".into())
                            .w_full()
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(theme.radii.control)
                            .text_size(typography.footnote)
                            .text_color(colors.title)
                            .hover(|style| style.bg(colors.chat_row_hover))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.new_conversation(cx);
                            }))
                            .child("New Conversation"),
                    )
                    .child(
                        div()
                            .id("overflow-chat-history")
                            .debug_selector(|| "overflow-chat-history".into())
                            .w_full()
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(theme.radii.control)
                            .text_size(typography.footnote)
                            .text_color(colors.title)
                            .hover(|style| style.bg(colors.chat_row_hover))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_chat_history(window, cx);
                            }))
                            .child("Chat History"),
                    ),
            )
        } else {
            None
        };

        // F-CHAT-34/35: the Chat History popover — a session list with
        // Open/Delete per row, or the "No past chats" empty state.
        let chat_history_menu = if self.history_open {
            let rows: Vec<AnyElement> = if self.history_sessions.is_empty() {
                vec![
                    div()
                        .id("chat-history-empty")
                        .debug_selector(|| "chat-history-empty".into())
                        .px(px(8.0))
                        .py(px(10.0))
                        .text_size(typography.footnote)
                        .text_color(colors.subtitle)
                        .child("No past chats")
                        .into_any_element(),
                ]
            } else {
                self.history_sessions
                    .iter()
                    .map(|session| {
                        let tab_id = session.tab_id.clone();
                        let confirming = self.history_delete_confirm.as_deref() == Some(&tab_id);
                        let open_tab_id = tab_id.clone();
                        let row_id = SharedString::from(format!("chat-history-row-{tab_id}"));
                        let title = if session.title.is_empty() {
                            "Untitled chat".to_string()
                        } else {
                            session.title.clone()
                        };
                        div()
                            .id(row_id)
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(6.0))
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(theme.radii.control)
                            .hover(|style| style.bg(colors.chat_row_hover))
                            .child(
                                div()
                                    .id(SharedString::from(format!("chat-history-open-{tab_id}")))
                                    .flex_1()
                                    .text_size(typography.footnote)
                                    .text_color(colors.title)
                                    .child(title)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_chat_history_session(open_tab_id.clone(), cx);
                                    })),
                            )
                            .child(if confirming {
                                let confirm_tab_id = tab_id.clone();
                                div()
                                    .id(SharedString::from(format!(
                                        "chat-history-confirm-{tab_id}"
                                    )))
                                    .flex()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "chat-history-confirm-delete-{tab_id}"
                                            )))
                                            .text_size(typography.footnote)
                                            .text_color(colors.tab_error)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.confirm_delete_chat_session(
                                                    confirm_tab_id.clone(),
                                                    cx,
                                                );
                                            }))
                                            .child("Confirm"),
                                    )
                                    .child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "chat-history-cancel-delete-{tab_id}"
                                            )))
                                            .text_size(typography.footnote)
                                            .text_color(colors.subtitle)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.cancel_delete_chat_session(cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .into_any_element()
                            } else {
                                let delete_tab_id = tab_id.clone();
                                div()
                                    .id(SharedString::from(format!("chat-history-delete-{tab_id}")))
                                    .text_size(typography.footnote)
                                    .text_color(colors.subtitle)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.request_delete_chat_session(delete_tab_id.clone(), cx);
                                    }))
                                    .child("Delete")
                                    .into_any_element()
                            })
                            .into_any_element()
                    })
                    .collect()
            };
            Some(
                div()
                    .id("chat-history-menu")
                    .debug_selector(|| "chat-history-menu".into())
                    .key_context("ChatHistoryMenu")
                    .track_focus(&self.history_focus)
                    .on_action(cx.listener(Self::cancel))
                    .absolute()
                    .right(px(60.0))
                    .bottom(px(43.0))
                    .w(px(260.0))
                    .max_h(px(320.0))
                    .overflow_y_scroll()
                    .p(px(6.0))
                    .rounded(theme.radii.toast)
                    .bg(colors.card_fill)
                    .border_1()
                    .border_color(colors.hairline)
                    .shadow_lg()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.history_open = false;
                        cx.notify();
                    }))
                    .children(rows),
            )
        } else {
            None
        };

        let attach_button = div()
            .id("attach-image")
            .debug_selector(|| "attach-image".into())
            .w(px(24.0))
            .h(px(24.0))
            .rounded(theme.radii.control)
            .flex()
            .items_center()
            .justify_center()
            .hover(|style| style.bg(colors.chat_row_hover))
            .on_click(move |_, window, cx| {
                attach_entity.update(cx, |chat, cx| chat.attach_image(window, cx));
            })
            .child(IconElement::new(Icon::Plus, px(12.0)).text_color(colors.meta));

        let overflow_button = div()
            .id("composer-overflow")
            .debug_selector(|| "composer-overflow".into())
            .w(px(24.0))
            .h(px(24.0))
            .rounded(theme.radii.control)
            .flex()
            .items_center()
            .justify_center()
            .hover(|style| style.bg(colors.chat_row_hover))
            .on_click(move |_, window, cx| {
                overflow_entity.update(cx, |chat, cx| chat.toggle_overflow(window, cx));
            })
            .child(div().text_size(px(14.0)).text_color(colors.meta).child("…"));

        let context_percent = context_usage
            .as_ref()
            .filter(|usage| usage.size > 0)
            .map(|usage| ((usage.used as f64 / usage.size as f64) * 100.0).round() as u64)
            .unwrap_or(0);

        // The composer text area renders the draft part by part: text runs
        // inline, chips as removable tokens. The placeholder shows only when
        // the whole draft is empty, so a chip-only draft still reads as
        // content.
        let composer_parts: Vec<AnyElement> = if self.composer.is_empty() {
            // D-CHAT-03 / F-CHAT-05: the empty composer's placeholder names
            // what state it's actually in — permission-wait is not ordinary
            // mid-turn queueing, so it gets its own text, distinct selector,
            // and (per `insert_text`/`backspace`/`delete`/`send` above)
            // actually refuses input rather than merely describing itself
            // that way.
            if self.pending_question().is_some() {
                vec![
                    div()
                        .id("permission-wait-placeholder")
                        .debug_selector(|| "permission-wait-placeholder".into())
                        .text_color(colors.meta)
                        .child("Waiting for permission response…")
                        .into_any_element(),
                ]
            } else if self.streaming {
                vec![
                    div()
                        .id("queue-placeholder")
                        .debug_selector(|| "queue-placeholder".into())
                        .text_color(colors.meta)
                        .child("Type to queue for the next turn…")
                        .into_any_element(),
                ]
            } else if self.client.is_none() && !self.connecting {
                // F-CHAT-05: sweep E03 drove the composer from the real
                // `● offline` state (client died / never connected) and
                // found typing + Enter genuinely inert -- `send()` routes an
                // offline Enter into a silent reconnect-and-retry instead of
                // submitting -- but nothing on screen said why nothing
                // happened; the generic "Message…" placeholder gave no
                // signal the agent was unreachable. Name the actual state,
                // matching the permission-wait and queue placeholders above.
                vec![
                    div()
                        .id("offline-placeholder")
                        .debug_selector(|| "offline-placeholder".into())
                        .text_color(colors.meta)
                        .child("Agent offline — reconnecting when you send…")
                        .into_any_element(),
                ]
            } else {
                vec![
                    div()
                        .text_color(colors.meta)
                        .child("Message...")
                        .into_any_element(),
                ]
            }
        } else {
            self.composer
                .parts()
                .iter()
                .enumerate()
                .map(|(index, part)| match part {
                    ComposerPart::Text(text) => div()
                        .text_color(colors.primary_text_color)
                        .child(text.clone())
                        .into_any_element(),
                    ComposerPart::Chip(chip) => {
                        let remove_entity = entity.clone();
                        let (glyph, kind) = match chip {
                            ComposerChip::Skill { .. } => ("✦", "skill"),
                            ComposerChip::File { .. } => ("▤", "file"),
                            ComposerChip::Image { .. } => ("▣", "image"),
                        };
                        let label = chip.label();
                        div()
                            .id(format!("composer-chip-{kind}"))
                            .debug_selector(move || format!("composer-chip-{kind}"))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(5.0))
                            .bg(colors.card_fill)
                            .border_1()
                            .border_color(colors.hairline)
                            .text_size(typography.caption2)
                            .child(div().text_color(colors.meta).child(glyph))
                            .child(div().text_color(colors.title).child(label))
                            .child(
                                div()
                                    .id(format!("chip-remove-{index}"))
                                    .debug_selector(move || format!("chip-remove-{index}"))
                                    .px(px(2.0))
                                    .rounded(px(2.0))
                                    .text_size(typography.caption2)
                                    .text_color(colors.meta)
                                    .hover(|style| style.bg(colors.chat_row_hover))
                                    .on_click(move |_, window, cx| {
                                        // F-CHAT-12: the × removes the chip
                                        // from the model correctly on its
                                        // own, but nothing else in the click
                                        // path re-requests composer focus —
                                        // the ancestor container only grabs
                                        // it on its own on_mouse_down, which
                                        // this click never reaches (the chip
                                        // stops propagation). Without this,
                                        // the composer is left keyboard-dead
                                        // until the user clicks the text
                                        // area again.
                                        remove_entity.update(cx, |chat, cx| {
                                            chat.remove_composer_chip(index, cx);
                                            chat.composer_focus.focus(window, cx);
                                        });
                                    })
                                    .child("×"),
                            )
                            .into_any_element()
                    }
                })
                .collect()
        };

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
                    .id("composer-input")
                    .debug_selector(|| "composer-input".into())
                    .px(px(4.0))
                    .pt(px(2.0))
                    .min_h(px(44.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x(px(4.0))
                    .gap_y(px(4.0))
                    .text_size(typography.headline)
                    .line_height(typography.body_line_height)
                    .children(composer_parts),
            )
            .when_some(self.queued_item.clone(), |this, queued| {
                // D-CHAT-03: the committed next-turn item, its text and a
                // remove ✕. One slot: the latest commit replaces the row.
                let remove_entity = entity.clone();
                let queued_for_id = queued.clone();
                this.child(
                    div()
                        .id("queued-item")
                        .debug_selector(|| "queued-item".into())
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded(theme.radii.control)
                        .bg(colors.raised)
                        .text_size(typography.footnote)
                        .child(div().text_color(colors.meta).child("Queued:"))
                        .child(
                            div()
                                .id("queued-text")
                                .debug_selector(move || format!("queued-text-{queued_for_id}"))
                                .text_color(colors.title)
                                .child(queued),
                        )
                        .child(
                            div()
                                .id("queued-remove")
                                .debug_selector(|| "queued-remove".into())
                                .px(px(4.0))
                                .rounded(px(3.0))
                                .text_color(colors.meta)
                                .hover(|style| style.bg(colors.chat_row_hover))
                                .on_click(move |_, _, cx| {
                                    remove_entity.update(cx, |chat, cx| {
                                        chat.remove_queued_item(cx);
                                    });
                                })
                                .child("×"),
                        ),
                )
            })
            .when_some(self.attach_error.clone(), |this, message| {
                this.child(
                    div()
                        .id("attach-error")
                        .debug_selector(|| "attach-error".into())
                        .px(px(4.0))
                        .text_size(typography.caption2)
                        .text_color(colors.git_conflict)
                        .child(message),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(attach_button)
                    .child(status_pill)
                    .child(model_control)
                    .child(div().flex_1())
                    .child(overflow_button)
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
                            .bg(if self.streaming || can_send {
                                colors.overlay_strong
                            } else {
                                colors.overlay
                            })
                            .text_color(if self.streaming || can_send {
                                colors.title
                            } else {
                                colors.text_ghost
                            })
                            .hover(|style| style.bg(colors.raised))
                            .when(self.streaming, |this| {
                                // D-CHAT-02: while a turn runs the same
                                // control becomes stop — a filled square in
                                // theme tokens — and its click dispatches the
                                // same path Escape uses. The selector stays
                                // `"send"`; tests target the control, not
                                // the glyph.
                                this.on_click(move |_, _, cx| {
                                    stop_entity.update(cx, |chat, cx| chat.cancel_turn(cx));
                                })
                                .child(
                                    div()
                                        .id("stop-glyph")
                                        .debug_selector(|| "stop-glyph".into())
                                        .w(px(9.0))
                                        .h(px(9.0))
                                        .rounded(px(2.0))
                                        .bg(colors.title),
                                )
                            })
                            .when(!self.streaming, |this| {
                                this.when(can_send, |this| {
                                    this.on_click(move |_, _, cx| {
                                        send_entity.update(cx, |chat, cx| chat.send(cx));
                                    })
                                })
                                .child("↑")
                            }),
                    ),
            )
            .children(slash_popup)
            .children(mention_popup)
            .children(overflow_menu)
            .children(chat_history_menu)
            .children(model_picker)
            .children(mode_picker)
            .children(context_popover)
    }
}

fn control_entry_row(entry: &Entry) -> BTreeMap<String, String> {
    let mut row = BTreeMap::new();
    match entry {
        Entry::User(text) => {
            row.insert("kind".into(), "user".into());
            row.insert("text".into(), text.clone());
        }
        Entry::Assistant { text, .. } => {
            row.insert("kind".into(), "assistant".into());
            row.insert("text".into(), text.clone());
        }
        Entry::Thought { text, .. } => {
            row.insert("kind".into(), "thought".into());
            row.insert("text".into(), text.clone());
        }
        Entry::ToolCall {
            id, title, status, ..
        }
        | Entry::SubagentTask {
            id, title, status, ..
        } => {
            row.insert("kind".into(), "tool".into());
            row.insert("id".into(), id.clone());
            row.insert("text".into(), title.clone());
            row.insert("status".into(), status.clone());
        }
        Entry::Permission {
            request_id,
            resolved,
            expired,
            dismissed,
            ..
        } => {
            row.insert("kind".into(), "permission".into());
            row.insert("id".into(), request_id.to_string());
            row.insert(
                "status".into(),
                if *dismissed {
                    "cancelled".into()
                } else if *expired {
                    "expired".into()
                } else if resolved.is_some() {
                    "selected".into()
                } else {
                    "pending".into()
                },
            );
        }
        Entry::Plan { entries, .. } => {
            row.insert("kind".into(), "plan".into());
            row.insert(
                "text".into(),
                entries
                    .iter()
                    .map(|entry| format!("{} · {}", entry.status, entry.content))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        Entry::TurnFooter(text) => {
            row.insert("kind".into(), "turn".into());
            row.insert("text".into(), text.clone());
        }
        Entry::Error { message, .. } => {
            row.insert("kind".into(), "error".into());
            row.insert("text".into(), message.clone());
        }
    }
    row
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
        let entity_for_bar = entity.clone();
        let transcript_ranges = self.transcript_entry_ranges();
        let transcript_focus = self.transcript_focus.clone();
        let question_answer = self.question_answer.clone();
        // F-CHAT-13: captured once per render, same as Swift's `canAcceptDrop`
        // — a permission-wait that starts mid-drag simply means the next
        // render (the composer disabling itself already forces one) stops
        // offering the drop target.
        let can_accept_drop = self.can_accept_drop();

        div()
            .id("chat-root")
            .debug_selector(|| "chat-root".into())
            .relative()
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
            .on_action(cx.listener(Self::send_answer_action))
            .on_action(cx.listener(Self::cancel_answer_action))
            .on_key_down(cx.listener(Self::on_composer_key))
            .when(can_accept_drop, |this| {
                this.on_drop(cx.listener(Self::drop_external_paths))
            })
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
                                // F-CHAT-22: a run of consecutive tool calls
                                // renders as one group, keyed to the run's
                                // last index. Every other index in that run
                                // is "swallowed" — an empty row — since the
                                // list requires one measured row per index
                                // but the group's card lives only at the tail.
                                if let Some((start, end)) =
                                    tool_call_run_bounds(&this.entries, entry_index)
                                {
                                    if entry_index != end {
                                        return div()
                                            .id(("chat-entry", entry_index))
                                            .into_any_element();
                                    }
                                    let members: Vec<(usize, Entry)> = (start..=end)
                                        .filter_map(|index| {
                                            this.entries
                                                .get(index)
                                                .cloned()
                                                .map(|entry| (index, entry))
                                        })
                                        .collect();
                                    let group_expanded = matches!(
                                        this.entries.get(end),
                                        Some(Entry::ToolCall {
                                            group_expanded: true,
                                            ..
                                        })
                                    );
                                    return div()
                                        .id(("chat-entry", entry_index))
                                        .w(px(TRANSCRIPT_WIDTH))
                                        .pb(px(8.0))
                                        .child(Chat::render_tool_call_group(
                                            members,
                                            end,
                                            group_expanded,
                                            &transcript_theme,
                                            entity.clone(),
                                        ))
                                        .into_any_element();
                                }
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
                                                &question_answer,
                                                this.copied_target.clone(),
                                                this.edit_summaries.get(&entry_index).cloned(),
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
                    .when_some(self.pending_question(), |this, (index, title)| {
                        // F-CHAT-26: the persistent "the agent is waiting on
                        // you" bar above the composer. It exists exactly
                        // while a question is unanswered; Show scrolls the
                        // transcript to the question card.
                        let show_entity = entity_for_bar.clone();
                        let bar_colors = theme.colors;
                        let bar_typography = theme.typography;
                        this.child(
                            div()
                                .id("pending-question-bar")
                                .debug_selector(|| "pending-question-bar".into())
                                .w(px(TRANSCRIPT_WIDTH))
                                .mb(px(8.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .px(px(10.0))
                                .py(px(6.0))
                                .rounded(theme.radii.control)
                                .bg(bar_colors.card_fill)
                                .border_1()
                                .border_color(bar_colors.rail_question)
                                .text_size(bar_typography.footnote)
                                .child(div().text_color(bar_colors.rail_question).child("?"))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_color(bar_colors.title)
                                        .child(format!("Question waiting · {title}")),
                                )
                                .child(
                                    div()
                                        .id("pending-question-show")
                                        .debug_selector(|| "pending-question-show".into())
                                        .px(px(8.0))
                                        .py(px(3.0))
                                        .rounded(theme.radii.control)
                                        .text_size(bar_typography.caption2)
                                        .text_color(bar_colors.meta)
                                        .hover(|style| style.bg(bar_colors.chat_row_hover))
                                        .on_click(move |_, _, cx| {
                                            show_entity.update(cx, |chat, cx| {
                                                chat.list_state.scroll_to_reveal_item(index);
                                                cx.notify();
                                            });
                                        })
                                        .child("Show"),
                                ),
                        )
                    })
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
            .child({
                // F-CHAT-13: the "Drop files to attach" overlay, matching
                // Swift's `ChatPaneView` — invisible by default, revealed by
                // gpui's own `drag_over` style refinement while an
                // `ExternalPaths` drag sits over the pane. Not drawn at all
                // while the composer can't accept input, matching the
                // top-level `on_drop` binding just above.
                let accent = theme.colors.accent;
                let overlay = div()
                    .id("chat-drop-overlay")
                    .debug_selector(|| "chat-drop-overlay".into())
                    .invisible()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radii.composer)
                    .border_1()
                    .border_color(accent)
                    .bg(accent.opacity(0.08))
                    .child(
                        div()
                            .id("chat-drop-overlay-label")
                            .debug_selector(|| "chat-drop-overlay-label".into())
                            .text_color(theme.colors.title)
                            .child("Drop files to attach"),
                    );
                if can_accept_drop {
                    overlay.drag_over::<ExternalPaths>(|style, _, _, _| style.visible())
                } else {
                    overlay
                }
            })
    }
}

fn now_hhmm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

/// F-CHAT-16: the model picker's search predicate — trimmed, lowercased,
/// substring match against name/id/description; matches Swift's
/// `ModelPickerFilter.filter` exactly (order-preserving, pure, an empty
/// query keeps every model).
fn model_query_matches(option: &ModelOption, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    option.name.to_lowercase().contains(&query)
        || option.id.to_lowercase().contains(&query)
        || option
            .description
            .as_deref()
            .is_some_and(|description| description.to_lowercase().contains(&query))
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

/// The reference `FileMentionIndex`: a synchronous, capped filesystem walk
/// over the chat's working directory. Hidden files and the usual noise
/// directories are skipped; the query is a case-insensitive substring match;
/// basename-prefix matches rank first; the result is capped at 8 paths.
fn mention_candidates_on_disk(cwd: &Path, query: &str) -> Vec<String> {
    const SCAN_CAP: usize = 5000;
    const IGNORED_DIRECTORIES: [&str; 4] = [".git", "node_modules", ".build", "DerivedData"];

    let mut relative_paths = Vec::new();
    let mut scanned = 0;
    let mut stack: Vec<PathBuf> = vec![cwd.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            scanned += 1;
            if scanned > SCAN_CAP {
                return finish_mention_matches(relative_paths, query);
            }
            let path = entry.path();
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            let is_hidden = name.starts_with('.');
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if is_hidden || IGNORED_DIRECTORIES.contains(&name.as_ref()) {
                    continue;
                }
                stack.push(path);
            } else if file_type.is_file()
                && !is_hidden
                && let Ok(relative) = path.strip_prefix(cwd)
            {
                relative_paths.push(relative.to_string_lossy().into_owned());
            }
        }
    }
    finish_mention_matches(relative_paths, query)
}

fn finish_mention_matches(mut relative_paths: Vec<String>, query: &str) -> Vec<String> {
    let lowered = query.to_lowercase();
    let matches: Vec<String> = relative_paths
        .drain(..)
        .filter(|path| lowered.is_empty() || path.to_lowercase().contains(&lowered))
        .collect();
    let mut ranked = matches;
    ranked.sort_by(|a, b| {
        let a_prefix = a
            .rsplit('/')
            .next()
            .unwrap_or(a)
            .to_lowercase()
            .starts_with(&lowered);
        let b_prefix = b
            .rsplit('/')
            .next()
            .unwrap_or(b)
            .to_lowercase()
            .starts_with(&lowered);
        b_prefix.cmp(&a_prefix).then_with(|| a.cmp(b))
    });
    ranked.truncate(8);
    ranked
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

fn option_hash(option: &AnswerOption) -> usize {
    option.id.bytes().fold(0usize, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as usize)
    })
}

/// F-CHAT-21: the one-line summary shown on a collapsed thought — text
/// flattened to a single line and capped so it never wraps the header row.
const THOUGHT_SUMMARY_MAX_CHARS: usize = 72;

fn thought_summary(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return "Thinking…".to_string();
    }
    if flat.chars().count() <= THOUGHT_SUMMARY_MAX_CHARS {
        flat
    } else {
        let truncated: String = flat.chars().take(THOUGHT_SUMMARY_MAX_CHARS).collect();
        format!("{truncated}…")
    }
}

/// F-CHAT-22: the `[start, end]` bounds (inclusive) of the consecutive run
/// of `Entry::ToolCall` entries that `index` belongs to, when that run has
/// more than one member. Returns `None` for a lone tool call or an index
/// that isn't a tool call at all — the caller then falls back to the
/// ordinary single-entry render path instead of grouping.
fn tool_call_run_bounds(entries: &[Entry], index: usize) -> Option<(usize, usize)> {
    if !matches!(entries.get(index), Some(Entry::ToolCall { .. })) {
        return None;
    }
    let mut start = index;
    while start > 0 && matches!(entries.get(start - 1), Some(Entry::ToolCall { .. })) {
        start -= 1;
    }
    let mut end = index;
    while matches!(entries.get(end + 1), Some(Entry::ToolCall { .. })) {
        end += 1;
    }
    (end > start).then_some((start, end))
}

/// F-CHAT-23: caps a tool call's rendered text output. Kept as the tail
/// rather than the head — a long run's result or error is usually at the
/// end, not the start.
const TOOL_OUTPUT_MAX_CHARS: usize = 2000;

fn truncate_tool_output(text: &str) -> (String, bool) {
    let char_count = text.chars().count();
    if char_count <= TOOL_OUTPUT_MAX_CHARS {
        (text.to_string(), false)
    } else {
        let skip = char_count - TOOL_OUTPUT_MAX_CHARS;
        (text.chars().skip(skip).collect(), true)
    }
}

/// F-CHAT-31: one line of a diff preview.
#[derive(Debug, PartialEq, Eq)]
enum DiffLine {
    Context(String),
    Removed(String),
    Added(String),
}

/// Caps the number of diff lines rendered in the transcript; a full-file
/// rewrite should not make the transcript unusable.
const DIFF_PREVIEW_MAX_LINES: usize = 60;

/// Builds a readable diff preview without a full LCS diff: matching lines
/// stay in context, a mismatch emits the old line then the new line at the
/// point where the two texts diverge.
fn diff_preview_lines(old_text: Option<&str>, new_text: &str) -> Vec<DiffLine> {
    let old_lines = split_diff_lines(old_text.unwrap_or(""));
    let new_lines = split_diff_lines(new_text);
    let mut rows = Vec::new();
    let mut old_index = 0;
    let mut new_index = 0;
    while old_index < old_lines.len() || new_index < new_lines.len() {
        if old_index < old_lines.len()
            && new_index < new_lines.len()
            && old_lines[old_index] == new_lines[new_index]
        {
            rows.push(DiffLine::Context(old_lines[old_index].clone()));
            old_index += 1;
            new_index += 1;
        } else {
            if old_index < old_lines.len() {
                rows.push(DiffLine::Removed(old_lines[old_index].clone()));
                old_index += 1;
            }
            if new_index < new_lines.len() {
                rows.push(DiffLine::Added(new_lines[new_index].clone()));
                new_index += 1;
            }
        }
    }
    rows
}

fn split_diff_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{FileDropEvent, Modifiers, TestAppContext, VisualTestContext};
    use std::cell::RefCell;
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

    #[test]
    fn persisted_transcript_contains_only_completed_turns() {
        let entries = vec![
            Entry::User("inspect".into()),
            Entry::Assistant {
                text: "done".into(),
                document: parse("done"),
            },
            Entry::TurnFooter("12:00".into()),
            Entry::User("still streaming".into()),
        ];
        let transcript = Chat::transcript_from_entries("tab-chat", &entries);
        assert_eq!(transcript.turns.len(), 1);
        assert_eq!(transcript.turns[0].entries.len(), 3);
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
            chat.read_with(&cx.cx, |chat, _| chat.composer.text()),
            "hello",
            "typing fills the composer"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.entries.is_empty()),
            "typing alone must not send anything"
        );
    }

    /// The control socket must edit this entity's composer, rather than an
    /// unrendered ACP session with a coincidentally matching tab id.
    #[gpui::test]
    async fn control_compose_changes_the_rendered_composer(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());

        chat.update(&mut cx.cx, |chat, cx| {
            chat.control_compose("MARKER_P107", cx);
        });
        refresh_frame(cx);

        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer.text()),
            "MARKER_P107",
            "the visible composer owns socket-driven draft text"
        );
        assert!(
            cx.debug_bounds("composer-input").is_some(),
            "the edited composer remains mounted in the rendered chat"
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
            chat.read_with(&cx.cx, |chat, _| chat.composer.text()),
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
                    Entry::Thought { text, .. } => Some(text.clone()),
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
                cost: None,
                ..Default::default()
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

    /// F-CHAT-29: hovering an assistant response reveals a local Copy control;
    /// its click writes the response and replaces its label with a checkmark.
    #[gpui::test]
    async fn assistant_response_copy_writes_text_and_confirms(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::Assistant {
                text: "copy this assistant response".into(),
                document: parse("copy this assistant response"),
            });
            chat
        });
        refresh_frame(cx);

        let response = cx
            .debug_bounds("assistant-response-0")
            .expect("assistant response is drawn");
        cx.simulate_mouse_move(response.center(), None, Modifiers::none());
        cx.run_until_parked();

        let copy = cx
            .debug_bounds("assistant-copy-0")
            .expect("hovering an assistant response reveals Copy");
        cx.simulate_click(copy.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            cx.cx.read_from_clipboard().and_then(|item| item.text()),
            Some("copy this assistant response".into()),
            "Copy writes only the assistant response"
        );
        assert!(
            cx.debug_bounds("assistant-copy-confirmed-0").is_some(),
            "the clicked Copy control visibly acknowledges success"
        );
        let _ = chat;
    }

    /// F-CHAT-30: a fenced code block exposes a local Copy control whose
    /// acknowledgement belongs to that block, not the surrounding response.
    #[gpui::test]
    async fn code_block_copy_writes_code_and_confirms(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::Assistant {
                text: "```rust\nlet answer = 42;\n```".into(),
                document: parse("```rust\nlet answer = 42;\n```"),
            });
            chat
        });
        refresh_frame(cx);

        let copy = cx
            .debug_bounds("code-block-copy-0-assistant-block-0")
            .expect("a code block exposes its own Copy control");
        cx.simulate_click(copy.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            cx.cx.read_from_clipboard().and_then(|item| item.text()),
            Some("let answer = 42;\n".into()),
            "code-block Copy writes raw code, without the fence or language label"
        );
        assert!(
            cx.debug_bounds("code-block-copy-confirmed-0-assistant-block-0")
                .is_some(),
            "the clicked code-block control visibly acknowledges success"
        );
        let _ = chat;
    }

    /// F-CHAT-32: ACP diffs render an edited-files summary. Its Open action
    /// is host-routed, and each Revert passes through confirmation before the
    /// card reports either the real git result or the real failure.
    #[gpui::test]
    async fn edit_summary_opens_and_reports_revert_success_or_error(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        for args in [
            ["init"].as_slice(),
            ["config", "user.email", "test@example.invalid"].as_slice(),
            ["config", "user.name", "Tiller Test"].as_slice(),
        ] {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(&dir.0)
                    .status()
                    .expect("run git setup")
                    .success()
            );
        }
        std::fs::write(dir.0.join("edited.rs"), "old\n").expect("seed tracked file");
        assert!(
            std::process::Command::new("git")
                .args(["add", "edited.rs"])
                .current_dir(&dir.0)
                .status()
                .expect("stage seed file")
                .success()
        );
        assert!(
            std::process::Command::new("git")
                .args(["commit", "-m", "seed"])
                .current_dir(&dir.0)
                .status()
                .expect("commit seed file")
                .success()
        );
        std::fs::write(dir.0.join("edited.rs"), "new\n").expect("modify tracked file");

        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                dir.0.clone(),
                cx,
            );
            let diff = |path: &str| {
                ToolCallContentInfo::Diff(ToolCallDiff {
                    path: PathBuf::from(path),
                    old_text: Some("old\n".into()),
                    new_text: "new\n".into(),
                })
            };
            chat.push_entry(Entry::ToolCall {
                id: "edit-ok".into(),
                title: "Edit file".into(),
                status: "Completed".into(),
                kind: "Edit".into(),
                content: vec![diff("edited.rs")],
                locations: vec![],
                raw_input: None,
                raw_output: None,
                expanded: false,
                group_expanded: false,
            });
            chat.push_entry(Entry::Assistant {
                text: "done".into(),
                document: parse("done"),
            });
            chat.push_entry(Entry::ToolCall {
                id: "edit-stale".into(),
                title: "Edit missing file".into(),
                status: "Completed".into(),
                kind: "Edit".into(),
                content: vec![diff("missing.rs")],
                locations: vec![],
                raw_input: None,
                raw_output: None,
                expanded: false,
                group_expanded: false,
            });
            chat
        });
        let opened = Rc::new(std::cell::RefCell::new(Vec::new()));
        let opened_events = opened.clone();
        cx.update(|_, cx| {
            cx.subscribe(&chat, move |_, event: &ChatEvent, _| {
                if let ChatEvent::OpenFile(path) = event {
                    opened_events.borrow_mut().push(path.clone());
                }
            })
            .detach();
        });
        refresh_frame(cx);

        let open = cx
            .debug_bounds("edit-summary-open-0-0")
            .expect("Open file is drawn");
        cx.simulate_click(open.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(opened.borrow().as_slice(), [PathBuf::from("edited.rs")]);

        let revert = cx
            .debug_bounds("edit-summary-revert-0-0")
            .expect("Revert is drawn");
        cx.simulate_click(revert.center(), Modifiers::none());
        cx.run_until_parked();
        let confirm = cx
            .debug_bounds("edit-summary-confirm-0-0")
            .expect("Revert requires confirmation");
        cx.simulate_click(confirm.center(), Modifiers::none());
        pump_chat_until(cx, &chat, |chat| {
            chat.edit_summaries
                .get(&0)
                .is_some_and(|state| !state.reverted_paths.is_empty())
        });
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("edit-summary-reverted-0-0").is_some(),
            "successful git discard is shown as reverted"
        );

        let revert = cx
            .debug_bounds("edit-summary-revert-2-0")
            .expect("second Revert is drawn");
        cx.simulate_click(revert.center(), Modifiers::none());
        cx.run_until_parked();
        let confirm = cx
            .debug_bounds("edit-summary-confirm-2-0")
            .expect("stale Revert also requires confirmation");
        cx.simulate_click(confirm.center(), Modifiers::none());
        pump_chat_until(cx, &chat, |chat| {
            chat.edit_summaries
                .get(&2)
                .is_some_and(|state| state.revert_error.is_some())
        });
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("edit-summary-error-2").is_some(),
            "failed git discard is shown on the card"
        );
    }

    /// F-CHAT-28: a protocol Task tool call becomes a subagent card; its
    /// following tool call is nested under the card and keeps its own
    /// expand/collapse control.
    #[gpui::test]
    async fn a_subagent_task_card_expands_nested_tool_calls(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["subagent"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);
        focus_and_type(cx, "inspect the transcript");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(
                    entry,
                    Entry::SubagentTask {
                        tool_calls,
                        status,
                        ..
                    } if status == "Completed" && !tool_calls.is_empty()
                )
            })
        });
        refresh_frame(cx);

        assert!(
            cx.debug_bounds("subagent-task-toggle-1").is_some(),
            "the subagent task card is drawn"
        );
        assert!(
            cx.debug_bounds("subagent-tool-call-toggle-1-0").is_none(),
            "nested tool calls stay collapsed with their parent card"
        );

        let task_toggle = cx
            .debug_bounds("subagent-task-toggle-1")
            .expect("subagent task toggle");
        cx.simulate_click(task_toggle.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("subagent-tool-call-toggle-1-0").is_some(),
            "expanding the task reveals its nested tool call"
        );

        let child_toggle = cx
            .debug_bounds("subagent-tool-call-toggle-1-0")
            .expect("nested tool call toggle");
        cx.simulate_click(child_toggle.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            chat.read_with(&cx.cx, |chat, _| {
                matches!(
                    chat.entries.get(1),
                    Some(Entry::SubagentTask { tool_calls, .. })
                        if tool_calls.first().is_some_and(|call| call.expanded)
                )
            }),
            "the nested tool call expands independently"
        );
    }

    /// F-CHAT-23: a permission request with no renderable choice is not
    /// silently stranded; Dismiss is drawn and answers ACP with cancellation,
    /// not with a rejection option.
    #[gpui::test]
    async fn an_unrenderable_permission_can_be_dismissed(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["permission-unrenderable"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);
        focus_and_type(cx, "run the unknown permission");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(
                    entry,
                    Entry::Permission {
                        options,
                        text_input: None,
                        resolved: None,
                        expired: false,
                        ..
                    } if options.is_empty()
                )
            })
        });
        refresh_frame(cx);

        assert!(
            cx.debug_bounds("permission-dismiss-1").is_some(),
            "unrenderable permissions expose Dismiss"
        );
        assert!(
            cx.debug_bounds("permission-option-allow").is_none(),
            "unrenderable permissions do not invent an option button"
        );

        let dismiss = cx
            .debug_bounds("permission-dismiss-1")
            .expect("dismiss control");
        cx.simulate_click(dismiss.center(), Modifiers::none());
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(
                    entry,
                    Entry::Permission {
                        dismissed: true,
                        resolved: None,
                        expired: true,
                        ..
                    }
                )
            }) && chat.has_completed_turn
        });
        assert!(
            !chat.read_with(&cx.cx, |chat, _| chat.pending_question().is_some()),
            "dismissing the request clears the pending state"
        );
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

    /// F-CHAT-05: a genuinely unresolved permission request takes the whole
    /// composer out of service — its own distinct placeholder, not the
    /// ordinary mid-turn "queue-placeholder", and typed characters/Enter are
    /// refused outright rather than silently queued.
    #[gpui::test]
    async fn permission_wait_disables_the_composer_and_shows_its_own_placeholder(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = chat_view(cx, &["permission"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "may I?");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.entries
                .iter()
                .any(|entry| matches!(entry, Entry::Permission { resolved: None, .. }))
        });
        refresh_frame(cx);

        assert!(
            cx.debug_bounds("permission-wait-placeholder").is_some(),
            "the permission-wait placeholder replaces the ordinary queue placeholder"
        );
        assert!(
            cx.debug_bounds("queue-placeholder").is_none(),
            "permission-wait must not read as ordinary mid-turn queueing"
        );

        let entries_before = chat.read_with(&cx.cx, |chat, _| chat.entries.len());
        focus_and_type(cx, "should not appear");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        refresh_frame(cx);

        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.composer.is_empty()),
            "the disabled editor must refuse typed characters entirely"
        );
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.entries.len()),
            entries_before,
            "Enter must not send or queue while a permission is pending"
        );
        assert!(
            cx.debug_bounds("permission-wait-placeholder").is_some(),
            "the placeholder survives the blocked keystrokes"
        );
    }

    /// F-CHAT-05: sweep E03 drove a real offline composer (agent process
    /// never came up) and found typing + Enter genuinely inert, but nothing
    /// on screen said why — the empty composer fell back to the same
    /// generic "Message…" placeholder used once connected. This is one half
    /// of the fix: a distinct placeholder names the offline state, the way
    /// permission-wait and queueing above already do. The other half —
    /// typed characters and Enter actually being refused, not merely
    /// looking refused — is asserted directly here too, mirroring
    /// `permission_wait_disables_the_composer_and_shows_its_own_placeholder`
    /// above: a wave-I critic checked the row's cited `SRC`
    /// (`ChatComposerView.swift:25-28`) against the real reference and found
    /// `canInteract` excludes `.disconnected` exactly like it excludes a
    /// pending permission, so the editor must be equally inert in both
    /// states, not just visually different.
    #[gpui::test]
    async fn offline_composer_shows_its_own_placeholder(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            Chat::from_test_command(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            )
        });
        // ACP owns a real worker thread and subprocess; permit its wakeups
        // to cross the deterministic test scheduler boundary (same as
        // `failed_launch_can_retry_and_complete` above).
        cx.executor().allow_parking();
        cx.run_until_parked();
        chat.read_with(cx, |chat, _| {
            assert!(
                chat.client.is_none(),
                "the missing binary must fail to launch"
            );
        });
        refresh_frame(cx);

        assert!(
            cx.debug_bounds("offline-placeholder").is_some(),
            "an empty, disconnected composer must name the offline state"
        );
        assert!(
            cx.debug_bounds("queue-placeholder").is_none(),
            "offline must not read as ordinary mid-turn queueing"
        );
        assert!(
            cx.debug_bounds("permission-wait-placeholder").is_none(),
            "offline must not read as a pending permission"
        );

        let entries_before = chat.read_with(cx, |chat, _| chat.entries.len());
        focus_and_type(cx, "should not appear");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        refresh_frame(cx);

        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.composer.is_empty()),
            "the disabled editor must refuse typed characters entirely while offline"
        );
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.entries.len()),
            entries_before,
            "Enter must neither send nor start a fresh reconnect attempt while offline"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| !chat.connecting),
            "a blocked Enter must not itself trigger a new connection attempt"
        );
        assert!(
            cx.debug_bounds("offline-placeholder").is_some(),
            "the placeholder survives the blocked keystrokes"
        );
    }

    /// F-CHAT-05 (H5-drive): a wave-G critic live-drove a genuinely offline
    /// agent (Codex ACP, refuses to launch without auth) and reported that,
    /// while the distinct offline placeholder correctly appeared before
    /// typing, pressing Return after typing "silently cleared the draft"
    /// instead of visibly disabling the field. That reading did not
    /// reproduce structurally at the time, and a wave-I critic later live-
    /// drove the disabled-vs-enabled question directly and confirmed the
    /// reference (`ChatComposerView.swift`) genuinely disables the editor
    /// for `.disconnected` — the composer here now matches that. What must
    /// keep holding regardless of *how* the editor is taken out of service:
    /// a draft typed before the connection dropped is never touched by
    /// going offline. The draft is seeded directly (bypassing the now-
    /// disabled `insert_text` path) to model exactly that "already there
    /// when disconnect happened" case, since real timing can't reliably
    /// land a keystroke inside the fleeting `connecting` window before the
    /// missing-binary launch fails.
    #[gpui::test]
    async fn offline_enter_never_discards_the_typed_draft(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            Chat::from_test_command(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            )
        });
        cx.executor().allow_parking();
        cx.run_until_parked();
        chat.read_with(cx, |chat, _| {
            assert!(
                chat.client.is_none(),
                "the missing binary must fail to launch"
            );
        });

        chat.update(cx, |chat, _| {
            chat.composer.insert_text("hello offline test");
        });
        refresh_frame(cx);
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer.text()),
            "hello offline test",
            "a draft already in the composer when disconnect happened must render untouched"
        );

        // Further typing must be refused outright — the editor is disabled,
        // not merely "won't send" — and Enter must be equally inert.
        focus_and_type(cx, " more");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        refresh_frame(cx);
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer.text()),
            "hello offline test",
            "a disabled offline composer must never silently discard or extend the user's draft"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.client.is_none() && !chat.connecting),
            "a blocked Enter must not itself start a new connection attempt"
        );
    }

    /// F-CHAT-25 + F-CHAT-26: a structured question renders with a free-text
    /// field while the pending-question bar sits above the composer; typing
    /// an answer and pressing Enter records it on the card, clears the bar,
    /// and the agent echoes the text back — the answer leaves the surface
    /// end to end.
    #[gpui::test]
    async fn a_text_answer_leaves_the_surface_and_clears_the_pending_bar(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["question"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "which color?");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        // The question is pending: the card offers the text field, and the
        // pending bar names the asker.
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(
                    entry,
                    Entry::Permission {
                        resolved: None,
                        expired: false,
                        ..
                    }
                )
            })
        });
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("question-answer-input").is_some(),
            "the pending question offers a text answer field"
        );
        assert!(
            cx.debug_bounds("question-answer-send").is_some()
                && cx.debug_bounds("question-answer-cancel").is_some(),
            "Send and Cancel controls are drawn with the field"
        );
        assert!(
            cx.debug_bounds("pending-question-bar").is_some(),
            "the pending-question bar is drawn while the question is open"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| {
                chat.pending_question()
                    .is_some_and(|(_, title)| title == "Ask user question")
            }),
            "the pending bar names the asking tool"
        );

        // Focus the field, type the answer, send it with Enter.
        let field = cx
            .debug_bounds("question-answer-input")
            .expect("the answer field is drawn");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("Blue");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        // The answer left the surface: the card records it, the pending bar
        // is gone, and the agent received it (echoed back in the turn).
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(
                    entry,
                    Entry::Permission {
                        resolved: Some(choice),
                        ..
                    } if choice == "Blue"
                )
            }) && chat.has_completed_turn
        });
        assert!(
            !chat.read_with(&cx.cx, |chat, _| chat.pending_question().is_some()),
            "answering the question clears the pending state"
        );
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("pending-question-bar").is_none(),
            "the pending-question bar is gone after the answer"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| {
                chat.entries.iter().any(|entry| {
                    matches!(
                        entry,
                        Entry::Assistant { text, .. } if text.contains("You chose: Blue")
                    )
                })
            }),
            "the agent received the typed answer and echoed it back"
        );
    }

    /// F-CHAT-25 (Cancel): withdrawing a pending question marks the card
    /// no longer answerable and clears the pending state; the turn still
    /// completes.
    #[gpui::test]
    async fn cancel_on_a_question_closes_it_without_an_answer(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["question"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "which color?");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(
                    entry,
                    Entry::Permission {
                        resolved: None,
                        expired: false,
                        ..
                    }
                )
            })
        });
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("pending-question-bar").is_some(),
            "the pending bar shows before the cancel"
        );

        let cancel = cx
            .debug_bounds("question-answer-cancel")
            .expect("the cancel control is drawn");
        cx.simulate_click(cancel.center(), Modifiers::none());
        cx.run_until_parked();

        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(
                    entry,
                    Entry::Permission {
                        resolved: None,
                        expired: true,
                        ..
                    }
                )
            }) && chat.has_completed_turn
        });
        assert!(
            !chat.read_with(&cx.cx, |chat, _| chat.pending_question().is_some()),
            "cancelling the question clears the pending state"
        );
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("pending-question-bar").is_none(),
            "the pending bar is gone after the cancel"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| {
                chat.entries.iter().any(|entry| {
                    matches!(
                        entry,
                        Entry::Permission {
                            expired: true,
                            resolved: None,
                            ..
                        }
                    )
                })
            }),
            "the cancelled card stays in the transcript as no-longer-answerable"
        );
    }

    /// F-CHAT-27: cancelling the turn while its question is unanswered
    /// expires the card — the transcript says so, the pending state ends,
    /// and the composer is usable again instead of waiting forever.
    #[gpui::test]
    async fn a_question_whose_turn_ends_unanswered_expires_instead_of_waiting(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = chat_view(cx, &["question-expire"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "which color?");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(
                    entry,
                    Entry::Permission {
                        resolved: None,
                        expired: false,
                        ..
                    }
                )
            })
        });
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("pending-question-bar").is_some(),
            "the pending bar is up while the question is open"
        );

        // Escape cancels the turn; the protocol answers the pending
        // permission with `cancelled`, and the card must expire instead of
        // leaving the surface waiting on a decision nothing can deliver.
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(
                    entry,
                    Entry::Permission {
                        resolved: None,
                        expired: true,
                        ..
                    }
                )
            }) && chat.has_completed_turn
        });
        assert!(
            !chat.read_with(&cx.cx, |chat, _| chat.pending_question().is_some()),
            "an expired question is no longer pending"
        );
        assert!(
            !chat.read_with(&cx.cx, |chat, _| chat.streaming),
            "the turn ended; the surface is not stuck streaming"
        );
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("pending-question-bar").is_none(),
            "the pending bar clears when the question expires"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| {
                chat.entries.iter().any(|entry| {
                    matches!(
                        entry,
                        Entry::Permission {
                            expired: true,
                            resolved: None,
                            ..
                        }
                    )
                })
            }),
            "the expired card stays in the transcript"
        );

        // The composer is usable again: a fresh turn completes.
        focus_and_type(cx, "still alive?");
        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| {
            let footers = chat
                .entries
                .iter()
                .filter(|entry| matches!(entry, Entry::TurnFooter(_)))
                .count();
            chat.entries
                .iter()
                .any(|entry| matches!(entry, Entry::User(text) if text == "still alive?"))
                && footers >= 2
        });
    }

    /// F-CHAT-24: a plan renders as a named Plan card with its entries; a
    /// pending approval attaches its option buttons to the card, and the
    /// approved plan advances in place.
    #[gpui::test]
    async fn a_plan_renders_approval_attaches_and_the_plan_advances(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plan"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "plan this");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        // The plan card is up with both entries pending and its approval
        // buttons attached (the permission named the plan tool).
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(
                    entry,
                    Entry::Plan {
                        entries,
                        approval:
                            Some(PlanApproval {
                                resolved: None,
                                expired: false,
                                ..
                            }),
                    } if entries.len() == 2
                        && entries.iter().all(|row| row.status == "pending")
                )
            })
        });
        assert!(
            chat.read_with(&cx.cx, |chat, _| {
                chat.pending_question()
                    .is_some_and(|(_, title)| title == "Exit plan mode")
            }),
            "a pending plan approval counts as the pending question"
        );
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("permission-option-approve").is_some()
                && cx.debug_bounds("permission-option-keep").is_some(),
            "the plan approval renders both decision buttons"
        );
        assert!(
            cx.debug_bounds("pending-question-bar").is_some(),
            "the pending bar covers the plan approval"
        );

        // Approve: the decision leaves the surface, the plan advances.
        let approve = cx
            .debug_bounds("permission-option-approve")
            .expect("approve button");
        cx.simulate_click(approve.center(), Modifiers::none());
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(
                    entry,
                    Entry::Plan {
                        entries,
                        approval:
                            Some(PlanApproval {
                                resolved: Some(choice),
                                ..
                            }),
                    } if choice == "Approve plan"
                        && entries.iter().any(|row| {
                            row.content == "Read the design" && row.status == "completed"
                        })
                        && entries.iter().any(|row| {
                            row.content == "Implement it" && row.status == "in_progress"
                        })
                )
            }) && chat.has_completed_turn
        });
        assert!(
            !chat.read_with(&cx.cx, |chat, _| chat.pending_question().is_some()),
            "approving the plan clears the pending state"
        );
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("pending-question-bar").is_none(),
            "the pending bar clears after the approval"
        );
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

    /// D-CHAT-02: while a turn streams, the send control keeps its `"send"`
    /// selector but draws the stop state, and a real click on it cancels
    /// through the same path as Escape — the footer states the cancellation
    /// and the control returns to send afterwards.
    #[gpui::test]
    async fn stop_click_cancels_the_stream_and_the_transcript_states_it(cx: &mut TestAppContext) {
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
        refresh_frame(cx);

        // The control is the same `"send"` element, drawn as stop.
        assert!(
            cx.debug_bounds("send").is_some(),
            "the control keeps its send selector while streaming"
        );
        assert!(
            cx.debug_bounds("stop-glyph").is_some(),
            "the stop state is drawn while the turn runs"
        );

        let stop = cx.debug_bounds("send").expect("the stop control is drawn");
        cx.simulate_click(stop.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            !chat.read_with(&cx.cx, |chat, _| chat.streaming),
            "a real click on the control stops the streaming turn"
        );
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("stop-glyph").is_none(),
            "the stop state is gone once the turn is stopped"
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

        // The composer is usable again, with the plain placeholder, and a
        // normal next turn completes unmarked.
        focus_and_type(cx, "again");
        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(
                |entry| matches!(entry, Entry::TurnFooter(text) if !text.contains("cancelled")),
            )
        });
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("stop-glyph").is_none()
                && cx.debug_bounds("queue-placeholder").is_none(),
            "the idle composer is neither stop nor queueing"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.has_completed_turn),
            "the later turn completes normally"
        );
    }

    /// D-CHAT-03, completion half: while a turn streams the composer stays
    /// editable, its placeholder says the next Enter queues, Enter commits
    /// the draft as the queued item, and when the turn completes the queued
    /// item arrives as the next user turn exactly once. Text left
    /// uncommitted at turn end stays in the composer and never auto-sends.
    #[gpui::test]
    async fn enter_during_a_stream_queues_and_the_turn_end_sends_it_exactly_once(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        let fixture_dir = dir.0.to_str().expect("fixture dir is utf-8").to_string();
        let (chat, cx) = chat_view(cx, &["staged", &fixture_dir]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "hello");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.streaming
                && matches!(
                    chat.entries.last(),
                    Some(Entry::Assistant { text, .. }) if text == "first "
                )
        });
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("queue-placeholder").is_some(),
            "the empty composer shows the queue placeholder while streaming"
        );

        // Enter during the stream commits the queue; the transcript is not
        // touched by the commit itself.
        focus_and_type(cx, "queued msg");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        let (queued, entries) = chat.read_with(&cx.cx, |chat, _| {
            (chat.queued_item.clone(), chat.entries.len())
        });
        assert_eq!(
            queued.as_deref(),
            Some("queued msg"),
            "Enter during a stream commits the draft as the queued item"
        );
        assert_eq!(entries, 2, "queueing does not touch the transcript");
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("queued-item").is_some(),
            "the queued row is drawn in the composer"
        );
        assert!(
            cx.debug_bounds("queued-text-queued msg").is_some(),
            "the queued text is drawn in the row"
        );

        // Text typed after the commit stays in the composer (mid-sentence
        // rule): it must not fire when the turn ends.
        let input = cx
            .debug_bounds("composer-input")
            .expect("the composer input is drawn");
        cx.simulate_click(input.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("half a thought");
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer.text()),
            "half a thought",
            "the composer stays editable while streaming"
        );

        // The turn completes; the queued item sends exactly once as a
        // normal turn and is answered.
        std::fs::write(dir.0.join("go"), "go").expect("write go file");
        pump_chat_until(cx, &chat, |chat| {
            chat.entries
                .iter()
                .filter(|entry| matches!(entry, Entry::User(text) if text == "queued msg"))
                .count()
                == 1
                && chat
                    .entries
                    .iter()
                    .any(|entry| matches!(entry, Entry::Assistant { text, .. } if text == "reply "))
        });
        let (queued_count, uncommitted_sent, composer_text, queue_drained, completed) = chat
            .read_with(&cx.cx, |chat, _| {
                let queued_count = chat
                    .entries
                    .iter()
                    .filter(|entry| matches!(entry, Entry::User(text) if text == "queued msg"))
                    .count();
                let uncommitted_sent = chat
                    .entries
                    .iter()
                    .any(|entry| matches!(entry, Entry::User(text) if text == "half a thought"));
                (
                    queued_count,
                    uncommitted_sent,
                    chat.composer.text(),
                    chat.queued_item.is_none(),
                    chat.has_completed_turn,
                )
            });
        assert_eq!(queued_count, 1, "the queued item sends exactly once");
        assert!(!uncommitted_sent, "uncommitted text never auto-sends");
        assert_eq!(
            composer_text, "half a thought",
            "uncommitted text stays in the composer"
        );
        assert!(queue_drained, "the queue is drained into the send");
        assert!(completed, "the queued turn completes like any other");
    }

    /// D-CHAT-03, ✕ half: removing the queued item clears the drawn row,
    /// and nothing sends when the turn ends.
    #[gpui::test]
    async fn removing_the_queued_item_means_nothing_sends_when_the_turn_ends(
        cx: &mut TestAppContext,
    ) {
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
        refresh_frame(cx);

        focus_and_type(cx, "queued msg");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.queued_item.clone()),
            Some("queued msg".to_string()),
            "the ✕ row starts with the committed item"
        );
        refresh_frame(cx);
        let remove = cx
            .debug_bounds("queued-remove")
            .expect("the queued row's remove control is drawn");
        cx.simulate_click(remove.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.queued_item.is_none()),
            "the ✕ clears the queued item"
        );
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("queued-item").is_none(),
            "the queued row is gone after the ✕"
        );

        // The turn ends (cancelled); the removed item must not send.
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.entries
                .iter()
                .any(|entry| matches!(entry, Entry::TurnFooter(text) if text.contains("cancelled")))
        });
        // Settle past any turn-end work, then verify nothing extra arrived.
        cx.cx
            .executor()
            .advance_clock(std::time::Duration::from_secs(1));
        cx.cx.run_until_parked();
        assert!(
            chat.read_with(&cx.cx, |chat, _| {
                chat.entries
                    .iter()
                    .all(|entry| !matches!(entry, Entry::User(text) if text == "queued msg"))
            }),
            "nothing sends when the queued item was removed"
        );
    }

    /// D-CHAT-03, stop-with-queue half: queue-then-stop is the redirect
    /// gesture — a queued item survives a stop via the control's click, and
    /// the cancelled turn's end sends it exactly once.
    #[gpui::test]
    async fn stopping_via_click_with_a_queued_item_still_sends_it(cx: &mut TestAppContext) {
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
        refresh_frame(cx);

        focus_and_type(cx, "queued msg");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.queued_item.clone()),
            Some("queued msg".to_string()),
            "the stop-with-queue test starts with the committed item"
        );
        refresh_frame(cx);

        // Stop via a real click on the control, with the item queued.
        let stop = cx.debug_bounds("send").expect("the stop control is drawn");
        cx.simulate_click(stop.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            !chat.read_with(&cx.cx, |chat, _| chat.streaming),
            "the stop click ends the streaming turn"
        );

        // The cancelled turn ends; the queued item still sends, once, and
        // is answered as a normal turn.
        pump_chat_until(cx, &chat, |chat| {
            chat.entries
                .iter()
                .filter(|entry| matches!(entry, Entry::User(text) if text == "queued msg"))
                .count()
                == 1
                && chat
                    .entries
                    .iter()
                    .any(|entry| matches!(entry, Entry::Assistant { text, .. } if text == "done "))
        });
        let (cancelled_footer, queue_drained, completed) = chat.read_with(&cx.cx, |chat, _| {
            let cancelled_footer = chat.entries.iter().any(
                |entry| matches!(entry, Entry::TurnFooter(text) if text.contains("cancelled")),
            );
            (
                cancelled_footer,
                chat.queued_item.is_none(),
                chat.has_completed_turn,
            )
        });
        assert!(cancelled_footer, "the cancelled turn's footer states it");
        assert!(queue_drained, "the queue drained into the send");
        assert!(completed, "the queued turn completes");
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
            ..Default::default()
        });
    }

    #[test]
    fn default_agent_cwd_follows_the_process_workspace() {
        assert_eq!(default_agent_cwd(), std::env::current_dir().unwrap());
    }

    #[test]
    fn thought_summary_flattens_and_caps_long_text() {
        assert_eq!(thought_summary(""), "Thinking…");
        assert_eq!(thought_summary("short thought"), "short thought");
        assert_eq!(
            thought_summary("line one\nline two"),
            "line one line two",
            "collapsed summary flattens newlines to one line"
        );
        let long = "word ".repeat(30);
        let summary = thought_summary(&long);
        assert!(
            summary.chars().count() <= THOUGHT_SUMMARY_MAX_CHARS + 1,
            "summary must stay within the cap plus the ellipsis: {summary:?}"
        );
        assert!(
            summary.ends_with('…'),
            "truncated summary keeps the ellipsis marker"
        );
    }

    /// F-CHAT-23: text output past the cap is truncated to its tail, not
    /// its head — a long run's result or error usually lands at the end.
    #[test]
    fn truncate_tool_output_keeps_the_tail_past_the_cap() {
        let (shown, truncated) = truncate_tool_output("short output");
        assert_eq!(shown, "short output");
        assert!(!truncated);

        let long: String = (0..(TOOL_OUTPUT_MAX_CHARS + 500))
            .map(|index| char::from(b'a' + (index % 26) as u8))
            .collect();
        let (shown, truncated) = truncate_tool_output(&long);
        assert!(truncated);
        assert_eq!(shown.chars().count(), TOOL_OUTPUT_MAX_CHARS);
        assert_eq!(shown, &long[long.len() - TOOL_OUTPUT_MAX_CHARS..]);
    }

    /// F-CHAT-31: matching lines stay context; a changed line emits the old
    /// text as `Removed` then the new text as `Added` at the point the two
    /// texts diverge, and a pure addition/deletion needs no counterpart.
    #[test]
    fn diff_preview_lines_marks_context_removed_and_added() {
        let rows = diff_preview_lines(Some("one\ntwo\nthree\n"), "one\nTWO\nthree\nfour\n");
        assert_eq!(
            rows,
            vec![
                DiffLine::Context("one".into()),
                DiffLine::Removed("two".into()),
                DiffLine::Added("TWO".into()),
                DiffLine::Context("three".into()),
                DiffLine::Added("four".into()),
            ]
        );
    }

    #[test]
    fn diff_preview_lines_treats_a_missing_old_text_as_a_pure_addition() {
        let rows = diff_preview_lines(None, "brand new\n");
        assert_eq!(rows, vec![DiffLine::Added("brand new".into())]);
    }

    fn test_tool_call(id: &str) -> Entry {
        Entry::ToolCall {
            id: id.into(),
            title: format!("{id} title"),
            status: "Completed".into(),
            kind: "Edit".into(),
            content: vec![],
            locations: vec![],
            raw_input: None,
            raw_output: None,
            expanded: false,
            group_expanded: false,
        }
    }

    #[test]
    fn tool_call_run_bounds_is_none_for_a_non_tool_call_entry() {
        let entries = vec![Entry::User("hi".into())];
        assert_eq!(tool_call_run_bounds(&entries, 0), None);
    }

    #[test]
    fn tool_call_run_bounds_is_none_for_a_lone_tool_call() {
        let entries = vec![
            Entry::User("hi".into()),
            test_tool_call("tool-1"),
            Entry::User("bye".into()),
        ];
        assert_eq!(
            tool_call_run_bounds(&entries, 1),
            None,
            "a run of one is not a group"
        );
    }

    #[test]
    fn tool_call_run_bounds_spans_a_consecutive_run_and_ignores_neighbors() {
        let entries = vec![
            Entry::User("hi".into()),
            test_tool_call("tool-1"),
            test_tool_call("tool-2"),
            test_tool_call("tool-3"),
            Entry::User("bye".into()),
        ];
        assert_eq!(tool_call_run_bounds(&entries, 1), Some((1, 3)));
        assert_eq!(tool_call_run_bounds(&entries, 2), Some((1, 3)));
        assert_eq!(tool_call_run_bounds(&entries, 3), Some((1, 3)));
    }

    #[test]
    fn tool_call_run_bounds_treats_two_separate_runs_independently() {
        let entries = vec![
            test_tool_call("tool-1"),
            test_tool_call("tool-2"),
            Entry::User("in between".into()),
            test_tool_call("tool-3"),
            test_tool_call("tool-4"),
        ];
        assert_eq!(tool_call_run_bounds(&entries, 0), Some((0, 1)));
        assert_eq!(tool_call_run_bounds(&entries, 1), Some((0, 1)));
        assert_eq!(tool_call_run_bounds(&entries, 3), Some((3, 4)));
        assert_eq!(tool_call_run_bounds(&entries, 4), Some((3, 4)));
    }

    #[test]
    fn acp_program_converts_to_the_adapters_own_agent_command() {
        // The picker contract: the chosen adapter's `AcpProgram` converts
        // to the `AgentCommand` that launches THAT agent — never another's.
        // This is the assertion the old acceptance test laundered:
        // connecting successfully proves nothing; the command differing by
        // adapter does.
        use tiller_agents::AgentAdapter as _;

        let codex = acp_agent_command(
            tiller_agents::CodexAdapter
                .acp_program()
                .expect("codex ships an ACP server"),
        );
        assert_eq!(codex.program, PathBuf::from("npx"));
        assert_eq!(
            codex.args,
            ["-y", "@agentclientprotocol/codex-acp@latest"],
            "a Codex tab must launch Codex's ACP server, not Claude's"
        );

        let claude = acp_agent_command(
            tiller_agents::ClaudeCodeAdapter
                .acp_program()
                .expect("claude ships an ACP server"),
        );
        assert_eq!(
            claude.args,
            ["-y", "@agentclientprotocol/claude-agent-acp@latest"]
        );
        assert_ne!(codex.args, claude.args, "the command differs by adapter");
    }

    #[gpui::test]
    async fn launch_with_command_wires_the_given_command(cx: &mut TestAppContext) {
        // `launch` keeps its default program; the picker path must be able
        // to pass the chosen adapter's command straight through and still
        // start the connection exactly like `launch` does.
        let chat = cx.new(|cx| {
            Chat::launch_with_command(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            )
        });

        cx.executor().allow_parking();
        cx.run_until_parked();

        chat.read_with(cx, |chat, _| {
            assert_eq!(
                chat.agent_command.program,
                PathBuf::from("/definitely/missing/tiller-acp-agent"),
                "the command given to the constructor is the command wired"
            );
            assert!(
                chat.entries.iter().any(|entry| matches!(
                    entry,
                    Entry::Error {
                        retryable: true,
                        ..
                    }
                )),
                "launch_with_command starts the connection like launch does"
            );
        });
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

    /// F-CHAT-02: a launch that fails with ACP's `auth_required` error gets
    /// a distinct banner (not the generic connection-failure card) with CLI
    /// login guidance — driven end-to-end against a real (fixture) agent
    /// process that rejects `session/new` with wire code -32000, same
    /// fixture shape as `tiller_acp`'s own
    /// `session_creation_auth_required_error_becomes_typed_auth_required`.
    #[cfg(unix)]
    #[gpui::test]
    async fn auth_required_launch_gets_a_dedicated_banner_with_login_guidance(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let command = AgentCommand::new("/bin/sh").args([
            "-c",
            r#"while IFS= read -r line; do id=$(printf '%s' "$line" | sed -E 's/.*"id":([^,]+),.*/\1/'); case "$line" in *initialize*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"protocolVersion":1,"agentCapabilities":{},"authMethods":[{"id":"login","name":"Login","description":"agent auth login"}]}}' ;; *session/new*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"error":{"code":-32000,"message":"Authentication required"}}' ;; esac; done"#,
        ]);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::from_test_command(command, std::env::temp_dir(), cx);
            configure_test_chat(&mut chat);
            chat
        });

        cx.executor().allow_parking();
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());

        assert!(
            cx.debug_bounds("chat-auth-required-banner").is_some(),
            "an auth_required launch failure must render the dedicated banner, not the generic connection card"
        );
        chat.read_with(cx, |chat, _| {
            let auth_entry = chat.entries.iter().find(|entry| {
                matches!(
                    entry,
                    Entry::Error {
                        kind: ErrorKind::AuthRequired,
                        ..
                    }
                )
            });
            let Some(Entry::Error { message, .. }) = auth_entry else {
                panic!(
                    "expected an AuthRequired error entry, got {:?}",
                    chat.entries
                );
            };
            assert!(
                message.contains("Login"),
                "banner must name the agent-advertised login method: {message}"
            );
            assert!(
                message.to_ascii_lowercase().contains("cli") || message.contains("login"),
                "banner must carry CLI login guidance, not just the raw protocol message: {message}"
            );
        });
    }

    /// F-CHAT-15: the session-mode pill opens a picker over the agent's
    /// advertised modes, selecting one calls through to the live
    /// `AcpClient::set_mode` seam and updates the pill label in place —
    /// this was a purely cosmetic 4-way string flip before this row wired
    /// it to `ModeCatalog`.
    #[gpui::test]
    async fn mode_picker_selects_an_agent_advertised_mode_and_updates_the_pill(
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
            chat.mode_catalog = Some(ModeCatalog {
                current_id: "ask".into(),
                options: vec![
                    AgentMode {
                        id: "ask".into(),
                        name: "Ask".into(),
                        description: None,
                    },
                    AgentMode {
                        id: "plan".into(),
                        name: "Plan".into(),
                        description: None,
                    },
                ],
            });
            chat
        });
        cx.update(|window, _| window.refresh());

        let pill = cx
            .debug_bounds("chat-status")
            .expect("status pill is rendered");
        cx.simulate_click(pill.center(), Modifiers::none());
        cx.run_until_parked();
        let option = cx
            .debug_bounds("mode-option-plan")
            .expect("agent mode option is rendered");
        cx.simulate_click(option.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("mode-picker").is_none(),
            "selecting a mode closes the picker"
        );
        _chat.read_with(cx, |chat, _| {
            assert_eq!(
                chat.mode_catalog
                    .as_ref()
                    .map(|catalog| catalog.current_id.as_str()),
                Some("plan"),
                "selecting a mode updates the live catalog's current id"
            );
        });
    }

    /// F-CHAT-16: the model picker's search field filters the driver's own
    /// list (order preserved, case-insensitive substring match), the
    /// driver's first-listed model carries the "Recommended" badge exactly
    /// while it's in the filtered results, and a query with no matches
    /// shows "No models match" instead of an empty list. Backspace is a
    /// bound action, not a raw key — it has to reach the search field
    /// through its own guard, not the composer's.
    #[gpui::test]
    async fn model_picker_search_filters_and_badges_the_recommended_model(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::from_test_command(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            chat.has_completed_turn = true;
            chat.available_models = vec![
                ModelOption {
                    id: "opus".into(),
                    name: "Opus".into(),
                    description: None,
                },
                ModelOption {
                    id: "sonnet".into(),
                    name: "Sonnet".into(),
                    description: None,
                },
                ModelOption {
                    id: "haiku".into(),
                    name: "Haiku".into(),
                    description: None,
                },
            ];
            chat
        });
        cx.update(|window, _| window.refresh());

        let chip = cx
            .debug_bounds("model-chip")
            .expect("model chip is rendered");
        cx.simulate_click(chip.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(cx.debug_bounds("model-search-input").is_some());
        assert!(
            cx.debug_bounds("model-option-recommended").is_some(),
            "the driver's first-listed model (opus) is badged Recommended"
        );
        assert!(cx.debug_bounds("model-option-opus").is_some());
        assert!(cx.debug_bounds("model-option-sonnet").is_some());
        assert!(cx.debug_bounds("model-option-haiku").is_some());

        // Typing routes to the search field, not the composer.
        cx.simulate_input("son");
        cx.run_until_parked();
        assert!(cx.debug_bounds("model-option-sonnet").is_some());
        assert!(
            cx.debug_bounds("model-option-opus").is_none(),
            "opus no longer matches \"son\""
        );
        assert!(cx.debug_bounds("model-option-haiku").is_none());
        assert!(
            cx.debug_bounds("model-option-recommended").is_none(),
            "the recommended model is filtered out, so its badge is gone too"
        );
        assert!(cx.debug_bounds("model-picker-no-match").is_none());

        // A query with no matches shows the empty state, not an empty list.
        cx.simulate_input("zzz");
        cx.run_until_parked();
        assert!(cx.debug_bounds("model-option-sonnet").is_none());
        assert!(cx.debug_bounds("model-picker-no-match").is_some());

        // Backspace is a bound action — it must still reach the search
        // field, not silently corrupt the composer's own draft.
        cx.simulate_keystrokes("backspace backspace backspace backspace backspace backspace");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("model-option-opus").is_some(),
            "clearing the query back to \"son\" then to empty restores every model"
        );
        assert!(cx.debug_bounds("model-option-sonnet").is_some());
        assert!(cx.debug_bounds("model-option-haiku").is_some());
        assert!(cx.debug_bounds("model-picker-no-match").is_none());
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.composer.is_empty()),
            "backspace inside the search field must not have eaten composer text"
        );
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

    /// F-CHAT-18: `TokenUsageBreakdown` (the wire event carrying
    /// PromptResponse.usage) merges into whatever `ContextUsage` is
    /// already known -- the breakdown rows appear without disturbing the
    /// used/size percent that arrived over the separate `UsageUpdate`
    /// event, and stay absent for an agent that never reports it.
    #[gpui::test]
    async fn context_popover_shows_token_breakdown_when_the_agent_reports_it(
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
        cx.simulate_click(ring.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("context-usage-breakdown").is_none(),
            "no breakdown rows before the agent ever reports token usage"
        );

        chat.update(cx, |chat, cx| {
            chat.handle_event(
                AcpEvent::TokenUsageBreakdown {
                    input_tokens: 40,
                    output_tokens: 12,
                    cached_read_tokens: Some(8),
                },
                cx,
            );
        });
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        assert!(cx.debug_bounds("context-usage-breakdown").is_some());
        assert!(
            cx.debug_bounds("context-usage-25-of-100").is_some(),
            "the used/size percent from the earlier UsageUpdate survives the merge"
        );
    }

    #[gpui::test]
    async fn thought_starts_collapsed_and_toggles_on_click(cx: &mut TestAppContext) {
        // F-CHAT-21: thinking renders collapsed to a summary until clicked,
        // live or historical, and clicking again folds it back.
        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::Thought {
                text: "considering the approach".into(),
                expanded: false,
            });
            chat
        });
        cx.update(|window, _| window.refresh());

        fn is_expanded(chat: &Entity<Chat>, cx: &mut VisualTestContext) -> bool {
            chat.read_with(cx, |chat, _| {
                matches!(chat.entries.last(), Some(Entry::Thought { expanded, .. }) if *expanded)
            })
        }
        assert!(!is_expanded(&chat, cx), "a new thought starts collapsed");

        let toggle = cx
            .debug_bounds("thought-toggle-0")
            .expect("thought toggle is rendered");
        cx.simulate_click(toggle.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            is_expanded(&chat, cx),
            "clicking the toggle expands the thought"
        );

        let toggle = cx
            .debug_bounds("thought-toggle-0")
            .expect("thought toggle stays rendered while expanded");
        cx.simulate_click(toggle.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(!is_expanded(&chat, cx), "clicking again collapses it back");
    }

    /// P91 part 2: `ToolCallStarted` carries the widened fields straight
    /// into `Entry::ToolCall`, and the card starts collapsed like a
    /// thought — the reader opts into the detail, live or historical.
    #[gpui::test]
    async fn tool_call_started_carries_widened_fields_and_starts_collapsed(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            Chat::new(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            )
        });
        chat.update(cx, |chat, cx| {
            chat.handle_event(
                AcpEvent::ToolCallStarted {
                    id: "tool-1".into(),
                    title: "Edit file".into(),
                    status: "InProgress".into(),
                    kind: "Edit".into(),
                    content: vec![ToolCallContentInfo::Diff(ToolCallDiff {
                        path: PathBuf::from("src/lib.rs"),
                        old_text: Some("old\n".into()),
                        new_text: "new\n".into(),
                    })],
                    locations: vec![ToolCallLocationInfo {
                        path: PathBuf::from("src/lib.rs"),
                        line: Some(3),
                    }],
                    raw_input: Some("{\"path\":\"src/lib.rs\"}".into()),
                    raw_output: Some("{\"bytesWritten\":12}".into()),
                },
                cx,
            );
        });
        chat.read_with(cx, |chat, _| match chat.entries.last() {
            Some(Entry::ToolCall {
                id,
                title,
                status,
                kind,
                content,
                locations,
                raw_input,
                raw_output,
                expanded,
                group_expanded,
            }) => {
                assert_eq!(id, "tool-1");
                assert_eq!(title, "Edit file");
                assert_eq!(status, "InProgress");
                assert_eq!(kind, "Edit");
                assert_eq!(content.len(), 1);
                assert_eq!(locations.len(), 1);
                assert!(raw_input.is_some());
                assert!(raw_output.is_some());
                assert!(!expanded, "a new tool call starts collapsed");
                assert!(!group_expanded, "a new tool call starts group-collapsed");
            }
            other => panic!("expected a widened ToolCall entry, got {other:?}"),
        });
    }

    /// A status-only `ToolCallUpdated` (the common case — a spinner or
    /// status flip with no new content) must not blank out the detail an
    /// earlier update already attached.
    #[gpui::test]
    async fn tool_call_update_merges_widened_fields_and_preserves_the_rest(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::ToolCall {
                id: "tool-1".into(),
                title: "Edit file".into(),
                status: "InProgress".into(),
                kind: "Edit".into(),
                content: vec![ToolCallContentInfo::Text("partial output".into())],
                locations: vec![],
                raw_input: None,
                raw_output: None,
                expanded: false,
                group_expanded: false,
            });
            chat
        });
        chat.update(cx, |chat, cx| {
            chat.handle_event(
                AcpEvent::ToolCallUpdated {
                    id: "tool-1".into(),
                    title: None,
                    status: Some("Completed".into()),
                    kind: None,
                    content: None,
                    locations: None,
                    raw_input: None,
                    raw_output: None,
                },
                cx,
            );
        });
        chat.read_with(cx, |chat, _| match chat.entries.last() {
            Some(Entry::ToolCall {
                title,
                status,
                kind,
                content,
                ..
            }) => {
                assert_eq!(
                    title, "Edit file",
                    "an unset update field keeps the old value"
                );
                assert_eq!(status, "Completed");
                assert_eq!(kind, "Edit");
                assert_eq!(
                    content,
                    &vec![ToolCallContentInfo::Text("partial output".into())],
                    "a status-only update must not blank out earlier content"
                );
            }
            other => panic!("expected the patched ToolCall entry, got {other:?}"),
        });
    }

    #[gpui::test]
    async fn tool_call_starts_collapsed_and_toggles_on_click(cx: &mut TestAppContext) {
        // F-CHAT-23: a tool call's detail (kind, content, locations) is
        // hidden until the reader clicks it open, same control as F-CHAT-21.
        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::ToolCall {
                id: "tool-1".into(),
                title: "Edit file".into(),
                status: "Completed".into(),
                kind: "Edit".into(),
                content: vec![ToolCallContentInfo::Text("the tool's output".into())],
                locations: vec![],
                raw_input: None,
                raw_output: None,
                expanded: false,
                group_expanded: false,
            });
            chat
        });
        cx.update(|window, _| window.refresh());

        fn is_expanded(chat: &Entity<Chat>, cx: &mut VisualTestContext) -> bool {
            chat.read_with(cx, |chat, _| {
                matches!(chat.entries.last(), Some(Entry::ToolCall { expanded, .. }) if *expanded)
            })
        }
        assert!(!is_expanded(&chat, cx), "a new tool call starts collapsed");

        let toggle = cx
            .debug_bounds("tool-call-toggle-0")
            .expect("tool call toggle is rendered");
        cx.simulate_click(toggle.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            is_expanded(&chat, cx),
            "clicking the toggle expands the tool call"
        );

        let toggle = cx
            .debug_bounds("tool-call-toggle-0")
            .expect("tool call toggle stays rendered while expanded");
        cx.simulate_click(toggle.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(!is_expanded(&chat, cx), "clicking again collapses it back");
    }

    #[gpui::test]
    async fn consecutive_tool_calls_group_under_one_toggle_and_expand_to_full_cards(
        cx: &mut TestAppContext,
    ) {
        // F-CHAT-22: three tool calls run back-to-back land as one "3 steps"
        // group, not three separate cards — and expanding it reveals every
        // member as its own full card, keyed by its own transcript index.
        cx.update(Theme::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            for id in ["tool-1", "tool-2", "tool-3"] {
                chat.push_entry(Entry::ToolCall {
                    id: id.into(),
                    title: format!("{id} title"),
                    status: "Completed".into(),
                    kind: "Edit".into(),
                    content: vec![],
                    locations: vec![],
                    raw_input: None,
                    raw_output: None,
                    expanded: false,
                    group_expanded: false,
                });
            }
            chat
        });
        cx.update(|window, _| window.refresh());

        assert!(
            cx.debug_bounds("tool-call-group-toggle-2").is_some(),
            "three consecutive tool calls collapse under one group toggle"
        );
        assert!(
            cx.debug_bounds("tool-call-toggle-0").is_none(),
            "a collapsed group does not render its members' own toggles"
        );

        let group_toggle = cx
            .debug_bounds("tool-call-group-toggle-2")
            .expect("group toggle is rendered");
        cx.simulate_click(group_toggle.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("tool-call-toggle-0").is_some(),
            "expanding the group renders the first member as its own full card"
        );
        assert!(
            cx.debug_bounds("tool-call-toggle-1").is_some(),
            "expanding the group renders the second member as its own full card"
        );
        assert!(
            cx.debug_bounds("tool-call-toggle-2").is_some(),
            "expanding the group renders the third member as its own full card"
        );

        let group_toggle = cx
            .debug_bounds("tool-call-group-toggle-2")
            .expect("group toggle stays rendered while expanded");
        cx.simulate_click(group_toggle.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("tool-call-toggle-0").is_none(),
            "clicking again collapses the group back"
        );
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

    /// F-CHAT-05: a failed launch must leave the transcript's own retryable
    /// error banner as the *only* way back online — an offline Send used to
    /// silently reconnect-and-resend, but a wave-I critic found the
    /// reference (`ChatComposerView.swift`/`ChatController.swift`) has no
    /// such feature, only a disabled editor. This drives the explicit
    /// `chat.retry` path (what the banner's "Retry" control calls) instead,
    /// and confirms an ordinary Send works again once that reconnect lands.
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
            chat.composer.insert_text("draft that must survive");
        });
        chat.read_with(cx, |chat, _| {
            assert!(
                !chat.can_send(),
                "F-CHAT-05: Send must stay disabled while disconnected, even with a draft present"
            );
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

        // The only path back online: the transcript error banner's explicit
        // Retry control, not a side effect of a disabled Send.
        chat.update(cx, |chat, cx| {
            chat.agent_command = AgentCommand::new("/bin/sh").args([
                "-c",
                r#"while IFS= read -r line; do id=$(printf '%s' "$line" | sed -E 's/.*"id":([^,]+),.*/\1/'); case "$line" in *initialize*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"protocolVersion":1,"agentCapabilities":{},"authMethods":[]}}' ;; *session/new*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"sessionId":"test"}}' ;; *session/prompt*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"stopReason":"end_turn"}}' ;; esac; done"#,
            ]);
            chat.retry(cx);
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
            assert_eq!(
                chat.composer.text(),
                "draft that must survive",
                "reconnecting on its own must not touch the still-unsent draft"
            );
            assert!(chat.can_send(), "Send must re-enable once back online");
        });

        // Now that the composer is enabled again, an ordinary Send goes
        // through exactly as it would have while never disconnected.
        chat.update(cx, |chat, cx| {
            chat.composer = Composer::new();
            chat.composer.insert_text("hello");
            chat.send(cx);
        });

        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(25));
            cx.run_until_parked();
        }
        chat.read_with(cx, |chat, _| {
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

    /// F-CHAT-09: typing `/` through the real keystroke path opens the slash
    /// popup fed by the agent's advertised commands; a filter prefix narrows
    /// it; up/down move the keyboard selection; Enter accepts the selected
    /// command into a skill token that survives editing and submission; a
    /// filter that matches nothing closes the popup; clicking a row accepts
    /// it too.
    #[gpui::test]
    async fn slash_popup_filters_and_inserts_a_skill_token(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["composer"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        let composer = cx.debug_bounds("composer").expect("the composer is drawn");
        cx.simulate_click(composer.center(), Modifiers::none());
        cx.run_until_parked();

        cx.simulate_keystrokes("/");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("slash-popup").is_some(),
            "typing / opens the command popup"
        );
        assert!(cx.debug_bounds("slash-option-cr").is_some());
        assert!(cx.debug_bounds("slash-option-create-plan").is_some());
        assert!(cx.debug_bounds("slash-option-research").is_some());

        // A filter prefix narrows the list; a prefix that matches nothing
        // hides the popup entirely.
        cx.simulate_keystrokes("c");
        cx.run_until_parked();
        assert!(cx.debug_bounds("slash-option-cr").is_some());
        assert!(cx.debug_bounds("slash-option-create-plan").is_some());
        assert!(
            cx.debug_bounds("slash-option-research").is_none(),
            "non-matching commands disappear from the popup"
        );
        cx.simulate_keystrokes("z");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("slash-popup").is_none(),
            "a filter that matches nothing closes the popup"
        );

        // Back to a matching prefix; keyboard selection + Enter accepts.
        cx.simulate_keystrokes("backspace");
        cx.run_until_parked();
        assert!(cx.debug_bounds("slash-popup").is_some());
        assert_eq!(chat.read_with(&cx.cx, |chat, _| chat.slash_selected), 0);
        cx.simulate_keystrokes("down");
        cx.run_until_parked();
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.slash_selected),
            1,
            "down moves the keyboard selection"
        );
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("slash-popup").is_none(),
            "accepting the selection closes the popup"
        );
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer.draft().text),
            "/create-plan  ",
            "the accepted command lands as a skill token"
        );

        // The token survives submission: Enter sends it as the /name prefix.
        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| chat.has_completed_turn);
        assert!(chat.read_with(&cx.cx, |chat, _| {
            chat.entries
                .iter()
                .any(|entry| matches!(entry, Entry::User(text) if text.trim() == "/create-plan"))
        }));

        // Clicking a row accepts directly.
        focus_and_type(cx, "/");
        refresh_frame(cx);
        assert!(cx.debug_bounds("slash-popup").is_some());
        let row = cx
            .debug_bounds("slash-option-cr")
            .expect("the click target is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("slash-popup").is_none());
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer.draft().text),
            "/cr  ",
            "clicking a row inserts that command's skill token"
        );
    }

    /// F-CHAT-10: typing `@` opens the mention popup fed by a real bounded
    /// filesystem walk over the chat's working directory; clicking a listed
    /// file inserts a file chip at the token's position, the chip survives
    /// further typing, and sending carries the path as a mention rather than
    /// text.
    #[gpui::test]
    async fn at_mention_popup_lists_files_and_inserts_a_file_chip(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        std::fs::create_dir_all(dir.0.join("src")).expect("create src dir");
        std::fs::write(dir.0.join("src/main.rs"), "fn main() {}").expect("write main.rs");
        std::fs::write(dir.0.join("README.md"), "# readme").expect("write readme");
        let cwd = dir.0.clone();

        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let command = AgentCommand::new("python3").args([CHAT_FIXTURE, "plain"]);
            Chat::from_test_command(command, cwd, cx)
        });
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        let composer = cx.debug_bounds("composer").expect("the composer is drawn");
        cx.simulate_click(composer.center(), Modifiers::none());
        cx.run_until_parked();

        cx.simulate_keystrokes("@");
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| !chat.mention_candidates.is_empty());
        refresh_frame(cx);
        assert!(cx.debug_bounds("mention-popup").is_some());
        assert!(cx.debug_bounds("mention-option-README.md").is_some());
        assert!(cx.debug_bounds("mention-option-src/main.rs").is_some());

        let row = cx
            .debug_bounds("mention-option-src/main.rs")
            .expect("the file row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("mention-popup").is_none(),
            "choosing a file closes the popup"
        );
        assert!(
            cx.debug_bounds("composer-chip-file").is_some(),
            "a file chip is rendered in the composer"
        );

        // The chip survives editing after it and serializes as a mention
        // path, not text.
        cx.simulate_input(" check");
        cx.run_until_parked();
        let draft = chat.read_with(&cx.cx, |chat, _| chat.composer.draft());
        assert_eq!(draft.text, " check");
        assert_eq!(draft.mention_paths, vec!["src/main.rs".to_string()]);

        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| {
            chat.entries
                .iter()
                .any(|entry| matches!(entry, Entry::User(text) if text == " check"))
        });
        assert!(
            !chat.read_with(&cx.cx, |chat, _| {
                chat.entries
                    .iter()
                    .any(|entry| matches!(entry, Entry::User(text) if text.contains("src/main.rs")))
            }),
            "the mention path must not leak into the user bubble text"
        );
    }

    /// F-CHAT-11 + F-CHAT-12: the attach control accepts exactly one PNG or
    /// JPEG as an image chip; an unsupported file and a multiple selection
    /// are rejected with a transient message; the chip's removal control
    /// takes it back out before sending.
    #[gpui::test]
    async fn attach_control_accepts_one_image_and_rejects_the_rest(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let png = dir.0.join("photo.png");
        std::fs::write(
            &png,
            b"not really a png but the type check is extension-based",
        )
        .expect("write png");
        let jpeg = dir.0.join("shot.jpeg");
        std::fs::write(&jpeg, b"jpeg bytes").expect("write jpeg");
        let txt = dir.0.join("notes.txt");
        std::fs::write(&txt, b"text").expect("write txt");

        // F-CHAT-05: attach/chip mechanics are exercised here, not
        // connectivity, so this needs a real connected client rather than
        // the permanently-missing-binary fixture other tests use — once
        // offline disables the whole composer (see the `offline_*` tests
        // above), the final "type right after chip removal" assertion below
        // would be exercising the disabled-editor path instead of the
        // chip-removal focus behavior it's meant to prove.
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        chat.update(cx, |chat, _| configure_test_chat(chat));
        cx.update(|window, _| window.refresh());

        let attach = cx
            .debug_bounds("attach-image")
            .expect("the attach control is drawn");

        // A supported image becomes a chip.
        chat.update(cx, |chat, _| {
            chat.attach_test_paths = vec![png.clone()];
        });
        cx.simulate_click(attach.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("composer-chip-image").is_some(),
            "a supported image appears as an attachment chip"
        );
        assert!(cx.debug_bounds("attach-error").is_none());
        let draft = chat.read_with(&cx.cx, |chat, _| chat.composer.draft());
        assert_eq!(draft.images.len(), 1);
        assert_eq!(draft.images[0].mime_type, "image/png");

        // An unsupported file is rejected with a transient message.
        chat.update(cx, |chat, _| {
            chat.attach_test_paths = vec![txt.clone()];
        });
        cx.simulate_click(attach.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("attach-error").is_some(),
            "an unsupported selection shows the rejection"
        );
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer.draft().images.len()),
            1,
            "the rejected file adds no chip"
        );
        pump_chat_until(cx, &chat, |chat| chat.attach_error.is_none());

        // A multiple selection is rejected too.
        chat.update(cx, |chat, _| {
            chat.attach_test_paths = vec![jpeg.clone(), png.clone()];
        });
        cx.simulate_click(attach.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("attach-error").is_some(),
            "a multiple selection shows the rejection"
        );
        pump_chat_until(cx, &chat, |chat| chat.attach_error.is_none());

        // F-CHAT-12: the chip's removal control removes it before sending.
        refresh_frame(cx);
        assert!(cx.debug_bounds("composer-chip-image").is_some());
        let remove = cx
            .debug_bounds("chip-remove-0")
            .expect("the chip removal control is drawn");
        cx.simulate_click(remove.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("composer-chip-image").is_none(),
            "removing the chip makes it disappear"
        );
        assert!(chat.read_with(&cx.cx, |chat, _| {
            chat.composer.draft().images.is_empty()
        }));

        // F-CHAT-12: the × must also re-request composer focus. Type
        // immediately after the click, with no intervening click back into
        // the field — a real user's next gesture — and the keystrokes must
        // land, not vanish into a keyboard-dead composer.
        cx.simulate_input("still here");
        cx.run_until_parked();
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer.draft().text),
            "still here",
            "the composer must accept keystrokes right after chip removal, with no re-click"
        );
    }

    /// F-CHAT-13: dropping files from outside the app (an `ExternalPaths`
    /// drag, the platform's stand-in for a real desktop-file-manager drop)
    /// onto the chat pane attaches a supported image as an image chip, a
    /// non-image file as a `@`-style file chip with a worktree-relative
    /// path, and rejects an oversized image with a transient message —
    /// mirroring Swift's `FileDrop.classify` + `ComposerDropApplier.apply`.
    #[gpui::test]
    async fn dropping_external_files_attaches_chips_and_rejects_the_oversized_one(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        let png = dir.0.join("photo.png");
        std::fs::write(
            &png,
            b"not really a png but the type check is extension-based",
        )
        .expect("write png");
        std::fs::create_dir_all(dir.0.join("src")).expect("create src dir");
        let txt = dir.0.join("src/notes.txt");
        std::fs::write(&txt, b"todo").expect("write txt");
        let huge = dir.0.join("huge.png");
        std::fs::write(&huge, vec![0u8; 10 * 1024 * 1024 + 1]).expect("write huge png");
        let cwd = dir.0.clone();

        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let command = AgentCommand::new("python3").args([CHAT_FIXTURE, "plain"]);
            Chat::from_test_command(command, cwd, cx)
        });
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        let composer = cx.debug_bounds("composer").expect("the composer is drawn");
        assert!(
            cx.debug_bounds("chat-drop-overlay").is_some(),
            "the drop target exists (invisibly) whenever the composer can accept input"
        );

        let paths = ExternalPaths(
            vec![png.clone(), txt.clone(), huge.clone()]
                .into_iter()
                .collect(),
        );
        cx.simulate_event(FileDropEvent::Entered {
            position: composer.center(),
            paths,
        });
        cx.simulate_event(FileDropEvent::Submit {
            position: composer.center(),
        });
        cx.run_until_parked();
        refresh_frame(cx);

        assert!(
            cx.debug_bounds("composer-chip-image").is_some(),
            "the supported image becomes an attachment chip"
        );
        assert!(
            cx.debug_bounds("composer-chip-file").is_some(),
            "the non-image file becomes a @-style file chip"
        );
        let draft = chat.read_with(&cx.cx, |chat, _| chat.composer.draft());
        assert_eq!(draft.images.len(), 1, "only the one valid image attaches");
        assert_eq!(
            draft.mention_paths,
            vec!["src/notes.txt".to_string()],
            "the file chip's path is relative to the agent's cwd"
        );
        assert!(
            cx.debug_bounds("attach-error").is_some(),
            "the oversized image is rejected with the transient message"
        );
    }

    /// F-CHAT-13 + F-CHAT-05: a drop that arrives while a permission is
    /// pending must be refused outright — no chip, no message — the same
    /// rule `insert_text`/`send` already enforce for the keyboard.
    #[gpui::test]
    async fn dropping_external_files_is_refused_during_permission_wait(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let png = dir.0.join("photo.png");
        std::fs::write(
            &png,
            b"not really a png but the type check is extension-based",
        )
        .expect("write png");
        let cwd = dir.0.clone();

        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let command = AgentCommand::new("python3").args([CHAT_FIXTURE, "permission"]);
            Chat::from_test_command(command, cwd, cx)
        });
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "may I?");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.entries
                .iter()
                .any(|entry| matches!(entry, Entry::Permission { resolved: None, .. }))
        });
        refresh_frame(cx);

        // The same Entered+Submit sequence the happy-path test drives: with
        // `can_accept_drop` false, `chat-root` never chained `.on_drop` this
        // render (the overlay div is still drawn for layout purposes, just
        // permanently `.invisible()` with no `.drag_over`/`.on_drop` bound),
        // so gpui has nothing registered to call — the drop is a silent
        // no-op, not a caught rejection.
        let composer = cx.debug_bounds("composer").expect("the composer is drawn");
        let paths = ExternalPaths(vec![png.clone()].into_iter().collect());
        cx.simulate_event(FileDropEvent::Entered {
            position: composer.center(),
            paths,
        });
        cx.simulate_event(FileDropEvent::Submit {
            position: composer.center(),
        });
        cx.run_until_parked();
        refresh_frame(cx);

        assert!(
            cx.debug_bounds("composer-chip-image").is_none(),
            "a drop during permission-wait must not add a chip"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.composer.draft().images.is_empty()),
            "a drop during permission-wait must not touch the draft"
        );
    }

    /// F-CHAT-14: the overflow menu toggles Follow Edited Files on and off,
    /// and New Conversation resets the composer and transcript to a brand
    /// new chat with a relaunched agent session.
    #[gpui::test]
    async fn overflow_menu_toggles_follow_and_resets_to_a_new_conversation(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        // Open the overflow menu.
        let overflow = cx
            .debug_bounds("composer-overflow")
            .expect("the overflow control is drawn");
        cx.simulate_click(overflow.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("composer-overflow-menu").is_some());
        assert!(cx.debug_bounds("overflow-follow").is_some());
        assert!(cx.debug_bounds("overflow-new-conversation").is_some());

        // Toggle Follow Edited Files on, then off.
        let follow = cx
            .debug_bounds("overflow-follow")
            .expect("the follow row is drawn");
        cx.simulate_click(follow.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.following_edited_files),
            "the follow toggle turns on"
        );
        let follow = cx
            .debug_bounds("overflow-follow")
            .expect("the follow row is still drawn");
        cx.simulate_click(follow.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            !chat.read_with(&cx.cx, |chat, _| chat.following_edited_files),
            "the follow toggle turns back off"
        );

        // Build a completed turn and a draft, then reset everything.
        focus_and_type(cx, "hello");
        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| chat.has_completed_turn);
        focus_and_type(cx, "draft");
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer.text()),
            "draft"
        );

        let overflow = cx
            .debug_bounds("composer-overflow")
            .expect("the overflow control is drawn again");
        cx.simulate_click(overflow.center(), Modifiers::none());
        cx.run_until_parked();
        let new_conversation = cx
            .debug_bounds("overflow-new-conversation")
            .expect("the new-conversation row is drawn");
        cx.simulate_click(new_conversation.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.entries.is_empty()),
            "New Conversation clears the transcript"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.composer.is_empty()),
            "New Conversation clears the composer"
        );
        assert!(
            !chat.read_with(&cx.cx, |chat, _| chat.has_completed_turn),
            "New Conversation forgets the completed turn"
        );
        pump_chat_until(cx, &chat, |chat| chat.client.is_some() && !chat.connecting);

        // The relaunched session takes a new turn.
        focus_and_type(cx, "again");
        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| {
            chat.entries
                .iter()
                .any(|entry| matches!(entry, Entry::User(text) if text == "again"))
                && chat.has_completed_turn
        });
    }

    /// F-CHAT-14: the toggle's other half — while Follow Edited Files is
    /// on, a tool call reporting a location emits `ChatEvent::OpenFile`
    /// for it (the same event an edit-summary card's own "open" link
    /// uses, already wired by the host to a file tab); while it's off,
    /// the same location never fires the event. A second location inside
    /// the 500ms throttle window is dropped.
    #[gpui::test]
    async fn following_edited_files_opens_the_tool_calls_reported_location(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            Chat::new(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            )
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&chat, move |_, event: &ChatEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        // Off by default: a reported location opens nothing.
        chat.update(cx, |chat, cx| {
            chat.handle_event(
                AcpEvent::ToolCallStarted {
                    id: "tool-1".into(),
                    title: "Edit file".into(),
                    status: "InProgress".into(),
                    kind: "Edit".into(),
                    content: vec![],
                    locations: vec![ToolCallLocationInfo {
                        path: PathBuf::from("src/lib.rs"),
                        line: Some(3),
                    }],
                    raw_input: None,
                    raw_output: None,
                },
                cx,
            );
        });
        assert!(
            events.borrow().is_empty(),
            "no follow while the toggle is off"
        );

        // On: the next reported location opens.
        chat.update(cx, |chat, _| chat.following_edited_files = true);
        chat.update(cx, |chat, cx| {
            chat.handle_event(
                AcpEvent::ToolCallUpdated {
                    id: "tool-1".into(),
                    title: None,
                    status: None,
                    kind: None,
                    content: None,
                    locations: Some(vec![ToolCallLocationInfo {
                        path: PathBuf::from("src/other.rs"),
                        line: None,
                    }]),
                    raw_input: None,
                    raw_output: None,
                },
                cx,
            );
        });
        assert_eq!(
            events.borrow().as_slice(),
            &[ChatEvent::OpenFile(PathBuf::from("src/other.rs"))],
            "a location reported while following opens that file"
        );

        // Throttled: a second location right after does not refollow.
        chat.update(cx, |chat, cx| {
            chat.handle_event(
                AcpEvent::ToolCallUpdated {
                    id: "tool-1".into(),
                    title: None,
                    status: None,
                    kind: None,
                    content: None,
                    locations: Some(vec![ToolCallLocationInfo {
                        path: PathBuf::from("src/third.rs"),
                        line: None,
                    }]),
                    raw_input: None,
                    raw_output: None,
                },
                cx,
            );
        });
        assert_eq!(
            events.borrow().len(),
            1,
            "a second location inside the throttle window does not refollow"
        );
    }

    /// F-CHAT-17: the model picker offers the agent's advertised effort
    /// levels, the current one is marked, and choosing one updates the
    /// selection and the effort label on the model chip.
    #[gpui::test]
    async fn model_picker_offers_effort_levels_and_updates_the_selection(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["composer"]);
        pump_chat_until(cx, &chat, |chat| {
            chat.effort.is_some() && !chat.available_models.is_empty() && !chat.streaming
        });
        refresh_frame(cx);

        // Complete a turn so the picker chip becomes interactive.
        focus_and_type(cx, "hi");
        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| chat.has_completed_turn);
        refresh_frame(cx);

        let chip = cx
            .debug_bounds("model-chip")
            .expect("the model chip is drawn");
        cx.simulate_click(chip.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("model-picker").is_some());
        assert!(cx.debug_bounds("effort-option-low").is_some());
        assert!(cx.debug_bounds("effort-option-medium").is_some());
        assert!(cx.debug_bounds("effort-option-high").is_some());
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| {
                chat.effort.as_ref().and_then(|e| e.current_value.clone())
            }),
            Some("medium".to_string()),
            "the fixture reported the current effort"
        );

        let high = cx
            .debug_bounds("effort-option-high")
            .expect("the high effort row is drawn");
        cx.simulate_click(high.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| {
                chat.effort.as_ref().and_then(|e| e.current_value.clone())
            }),
            Some("high".to_string()),
            "choosing an effort updates the selection"
        );
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("model-effort-label").is_some(),
            "the model chip shows the selected effort"
        );
    }

    /// F-CHAT-19: above the 80% threshold the context ring renders its
    /// warning element; at 25% it does not.
    #[gpui::test]
    async fn context_ring_warns_above_eighty_percent(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["composer"]);
        pump_chat_until(cx, &chat, |chat| chat.context_usage.is_some());
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("context-ring-warning").is_some(),
            "85% usage renders the warning ring"
        );
        assert!(cx.debug_bounds("context-ring-progress").is_some());

        // The 25% fixture state renders no warning.
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
        assert!(
            cx.debug_bounds("context-ring-warning").is_none(),
            "25% usage stays on the calm ring"
        );
        assert!(cx.debug_bounds("context-ring-progress").is_some());
    }

    /// F-CHAT-36: an agent that exposes no models renders a plain agent
    /// badge instead of the model picker chip, and the badge opens nothing.
    #[gpui::test]
    async fn no_models_fallback_shows_a_plain_agent_badge(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::from_test_command(
                AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            chat.has_completed_turn = true;
            chat.available_models = Vec::new();
            chat
        });
        cx.update(|window, _| window.refresh());

        assert!(
            cx.debug_bounds("agent-badge").is_some(),
            "no models renders the plain agent badge"
        );
        assert!(
            cx.debug_bounds("model-chip").is_none(),
            "no models renders no picker chip"
        );
        let badge = cx.debug_bounds("agent-badge").expect("the badge is drawn");
        cx.simulate_click(badge.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("model-picker").is_none(),
            "the badge opens no picker"
        );
    }

    /// F-EDIT-07: a fenced code block's info-string tag resolves to the
    /// same `Language` the editor uses, so the preview highlighter picks
    /// the right keyword vocabulary instead of always falling back to
    /// `PlainText` (which would produce zero highlights, reproducing the
    /// original flat-color defect).
    #[test]
    fn fence_tag_resolves_to_editor_language() {
        assert_eq!(Chat::language_from_fence_tag("bash"), Language::Shell);
        assert_eq!(Chat::language_from_fence_tag("Rust"), Language::Rust);
        assert_eq!(Chat::language_from_fence_tag("py"), Language::Python);
        assert_eq!(
            Chat::language_from_fence_tag("not-a-real-language"),
            Language::PlainText
        );
    }

    /// F-EDIT-07: the fenced `CodeBlock` render path now runs the same
    /// `code_spans` pass the editor's code surface uses, producing at
    /// least one highlighted keyword span for a comment-and-command shell
    /// block — the exact case the critic's pixel inspection caught
    /// rendering as flat, unhighlighted text.
    #[test]
    fn shell_code_block_produces_keyword_and_comment_highlights() {
        let language = Chat::language_from_fence_tag("bash");
        let text = "# comment\nif [ -f x ]; then\n  echo hi\nfi";
        let mut saw_comment = false;
        let mut saw_keyword = false;
        for line in text.split('\n') {
            for span in code_spans(language, line) {
                match span.kind {
                    CodeSpanKind::Comment => saw_comment = true,
                    CodeSpanKind::Keyword => saw_keyword = true,
                    CodeSpanKind::Literal => {}
                }
            }
        }
        assert!(
            saw_comment,
            "the comment line should highlight as a comment"
        );
        assert!(saw_keyword, "if/then/fi should highlight as keywords");
    }
}
