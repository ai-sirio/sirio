//! The chat transcript and composer, driven by real ACP events.
//!
//! This first Rust implementation owns a linear transcript and one live ACP
//! session, same scope as `Sidebar`'s fixture model: the connection lifecycle
//! and view model live here so the transcript renderer stays deterministic.

use bezel::ui::tooltip::Tooltip;
use gpui::{
    AnyElement, App, BorderStyle, Bounds, ClipboardItem, Context, CursorStyle, DispatchPhase,
    Edges, Element, ElementId, Entity, EventEmitter, ExternalPaths, FocusHandle, Focusable,
    FollowMode, FontWeight, GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId,
    KeyBinding, KeyDownEvent, LayoutId, ListAlignment, ListSizingBehavior, ListState, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder, Pixels, Rgba, SharedString,
    StyledText, Task, Window, actions, canvas, div, list, point, prelude::*, px, quad, rgb,
    transparent_black,
};
use sirio_acp::{
    AcpClient, AcpEvent, AgentCommand, AgentMode, AvailableCommandInfo, ContextUsage, EffortOption,
    ImageAttachment, ModeCatalog, ModelCatalog, ModelOption, ToolCallContentInfo, ToolCallDiff,
    ToolCallLocationInfo,
};
use sirio_git::{GitActions, status as git_status};
use sirio_markdown::{
    Alignment as LegacyAlignment, Block as LegacyBlock, Document as LegacyDocument,
    Inline as LegacyInline, ListKind as LegacyListKind,
};
use sirio_persistence::{
    AppDatabase, ChatEntry, ChatPermissionOption, ChatPermissionOutcome, ChatPlanEntry,
    ChatSessionSummary, ChatToolLocation, ChatTranscript, ChatTurn,
};
use sirio_theme::Theme;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::caret;
use crate::sidebar::icons::{Icon, IconElement, IconSize};

mod composer_view;
mod list_scroll;
mod thought;
mod tool_calls;
mod transcript;
mod turn_rail;
use bezel::ui::input::TextField;
use bezel::ui::popover;
use composer_view::{TokenPopup, assemble_prompt, mention_token, slash_token};

/// F-CORE-FILE-04: overrides a rendered Markdown link's click, used by
/// callers (File Preview) that want to try resolving the link as a local
/// file before falling back to opening it externally. `None` keeps the
/// default of every link opening via `cx.open_url`, which is right for
/// assistant-authored chat prose.
pub(crate) type LinkClickOverride = Rc<dyn Fn(&str, &mut Window, &mut App)>;

fn highlight_markdown_code(
    language: &str,
    code: &str,
) -> Option<Vec<(Range<usize>, bezel::theme::HighlightKind)>> {
    syntax::highlight(code, language)
}

/// Installs the app-owned syntax highlighter used by bezel-markdown code
/// blocks. The renderer remains usable without this registration and simply
/// paints an unknown language as plain code.
pub fn init(cx: &mut App) {
    markdown::set_highlighter(
        cx,
        highlight_markdown_code,
        syntax::lang::LANGS.iter().map(|language| language.name),
    );
}

fn markdown_link_at(document: &markdown::Doc, cursor: markdown::Cursor) -> Option<String> {
    let text = document.blocks.get(cursor.block)?.text_at(cursor.part)?;
    text.marks.iter().rev().find_map(|span| {
        if !span.range.contains(&cursor.offset) {
            return None;
        }
        match &span.mark {
            markdown::Mark::Link(target) | markdown::Mark::Image(target) => Some(target.clone()),
            markdown::Mark::Mention { url, .. } => Some(url.clone()),
            _ => None,
        }
    })
}

fn markdown_block_link_at(document: &markdown::Doc, block: usize) -> Option<String> {
    match &document.blocks.get(block)?.kind {
        markdown::BlockKind::Bookmark { url, .. } | markdown::BlockKind::Image { url, .. } => {
            Some(url.clone())
        }
        _ => None,
    }
}

/// Defers bezel-markdown's build until gpui supplies the `Window` and `App`
/// required by `markdown::render`. File Preview can therefore keep its
/// existing source-only seam while chat entries use the same renderer.
struct MarkdownBody {
    document: markdown::Doc,
    link_click: Option<LinkClickOverride>,
    rendered: Option<AnyElement>,
}

impl MarkdownBody {
    fn new(document: markdown::Doc) -> Self {
        Self {
            document,
            link_click: None,
            rendered: None,
        }
    }

    fn with_link_override(document: markdown::Doc, link_click: LinkClickOverride) -> Self {
        Self {
            document,
            link_click: Some(link_click),
            rendered: None,
        }
    }

    fn build(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        let Some(link_click) = self.link_click.clone() else {
            return markdown::render(&self.document, markdown::Caption::Shown, window, cx);
        };

        let layouts = markdown::BlockLayouts::default();
        let rendered = markdown::render_with_selection(
            &self.document,
            None,
            Some(&layouts),
            None,
            markdown::Caption::Shown,
            window,
            cx,
        );
        let document = self.document.clone();
        let click_layouts = layouts.clone();
        div()
            .w_full()
            .capture_any_mouse_up(move |event, window, cx| {
                if event.button != MouseButton::Left {
                    return;
                }
                let inline = click_layouts
                    .over_text(event.position)
                    .then(|| click_layouts.hit(event.position))
                    .flatten()
                    .and_then(|cursor| markdown_link_at(&document, cursor));
                let block = click_layouts
                    .block_at(event.position)
                    .and_then(|block| markdown_block_link_at(&document, block));
                if let Some(target) = inline.or(block) {
                    cx.stop_propagation();
                    window.prevent_default();
                    link_click(&target, window, cx);
                }
            })
            .child(rendered)
            .into_any_element()
    }
}

impl Element for MarkdownBody {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        if self.rendered.is_none() {
            self.rendered = Some(self.build(window, cx));
        }
        let rendered = self
            .rendered
            .as_mut()
            .expect("markdown body was built during layout");
        (rendered.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.rendered
            .as_mut()
            .expect("markdown body requested layout before prepaint")
            .prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.rendered
            .as_mut()
            .expect("markdown body prepainted before paint")
            .paint(window, cx);
    }
}

impl IntoElement for MarkdownBody {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// The transcript's content column maximum — the Bezel Transcript pattern's
/// 700 (spec §2). Settings and the markdown column keep waku's 720; this one
/// column follows Bezel because the live transcript is what the migration
/// copies.
pub(crate) const TRANSCRIPT_WIDTH: f32 = 700.0;
pub(crate) const CARD_H_PADDING: f32 = 14.0;
/// The tallest the queue's entry list grows before it scrolls (D-CHAT-03):
/// about five rows, Zed's `max_h_40`, so a long queue never pushes the
/// composer card off the pane.
pub(crate) const QUEUE_MAX_HEIGHT: f32 = 160.0;
pub(crate) const CARD_V_PADDING: f32 = 10.0;

/// The user turn's bubble: rounded, right-aligned, capped at the Bezel
/// Activity pattern's 440. The assistant reply has no container at all.
pub(crate) const USER_PILL_MAX_WIDTH: f32 = 440.0;
pub(crate) const USER_PILL_H_PADDING: f32 = 14.0;
pub(crate) const USER_PILL_V_PADDING: f32 = 9.0;
pub(crate) const USER_PILL_TEXT_SIZE: f32 = 13.5;
pub(crate) const TURN_BOTTOM_PADDING: f32 = 28.0;

fn parse_chat_markdown(source: &str) -> markdown::Doc {
    markdown::parse(source)
}

fn bezel_doc_from_legacy(document: LegacyDocument) -> markdown::Doc {
    let mut blocks = Vec::new();
    for block in document.blocks {
        push_legacy_block(block, 0, false, &mut blocks);
    }
    markdown::Doc { blocks }
}

fn push_legacy_block(block: LegacyBlock, indent: u8, quoted: bool, out: &mut Vec<markdown::Block>) {
    let at = |kind| markdown::Block::at(kind, indent);
    match block {
        LegacyBlock::Heading { level, inline } => out.push(at(if quoted {
            markdown::BlockKind::Quote(bezel_text(&inline))
        } else {
            markdown::BlockKind::Heading {
                level,
                text: bezel_text(&inline),
            }
        })),
        LegacyBlock::Paragraph { inline } => out.push(at(if quoted {
            markdown::BlockKind::Quote(bezel_text(&inline))
        } else {
            markdown::BlockKind::Paragraph(bezel_text(&inline))
        })),
        LegacyBlock::List { kind, items, .. } => {
            for (index, item) in items.into_iter().enumerate() {
                let mut item_blocks = item.blocks.into_iter();
                let text = item_blocks.next().map_or_else(
                    || markdown::Text::plain(""),
                    |first| match first {
                        LegacyBlock::Heading { inline, .. } | LegacyBlock::Paragraph { inline } => {
                            bezel_text(&inline)
                        }
                        other => markdown::Text::plain(other.plain_text()),
                    },
                );
                let kind = match item.checked {
                    Some(checked) => markdown::BlockKind::Task { checked, text },
                    None => match kind {
                        LegacyListKind::Bullet => markdown::BlockKind::Bullet(text),
                        LegacyListKind::Ordered { start } => markdown::BlockKind::Ordered {
                            number: start + index as u64,
                            text,
                        },
                    },
                };
                out.push(at(kind));
                for child in item_blocks {
                    push_legacy_block(child, indent.saturating_add(1), quoted, out);
                }
            }
        }
        LegacyBlock::BlockQuote { blocks } => {
            for block in blocks {
                push_legacy_block(block, indent, true, out);
            }
        }
        LegacyBlock::CodeBlock { language, text, .. } => out.push(at(markdown::BlockKind::Code {
            language,
            code: markdown::Text::plain(text.trim_end_matches('\n')),
        })),
        LegacyBlock::Table {
            alignment,
            header,
            rows,
        } => out.push(at(markdown::BlockKind::Table {
            align: alignment
                .into_iter()
                .map(|alignment| match alignment {
                    LegacyAlignment::Center => markdown::Align::Center,
                    LegacyAlignment::Right => markdown::Align::Right,
                    LegacyAlignment::None | LegacyAlignment::Left => markdown::Align::Left,
                })
                .collect(),
            header: header
                .into_iter()
                .map(|cell| bezel_text(&cell.inline))
                .collect(),
            rows: rows
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|cell| bezel_text(&cell.inline))
                        .collect()
                })
                .collect(),
        })),
        LegacyBlock::ThematicBreak => out.push(at(markdown::BlockKind::Rule)),
        LegacyBlock::Html { text } => {
            out.push(at(markdown::BlockKind::Paragraph(markdown::Text::plain(
                text,
            ))));
        }
    }
}

fn bezel_text(inlines: &[LegacyInline]) -> markdown::Text {
    let mut text = markdown::Text::default();
    for inline in inlines {
        push_legacy_inline(inline, &mut text);
    }
    text
}

fn push_marked_inlines(mark: markdown::Mark, inlines: &[LegacyInline], text: &mut markdown::Text) {
    let index = text.marks.len();
    let start = text.text.len();
    text.marks.push(markdown::MarkSpan {
        range: start..start,
        mark,
    });
    for inline in inlines {
        push_legacy_inline(inline, text);
    }
    text.marks[index].range.end = text.text.len();
}

fn push_legacy_inline(inline: &LegacyInline, text: &mut markdown::Text) {
    match inline {
        LegacyInline::Text(value) | LegacyInline::Html(value) => text.text.push_str(value),
        LegacyInline::Emphasis(children) => {
            push_marked_inlines(markdown::Mark::Italic, children, text)
        }
        LegacyInline::Strong(children) => push_marked_inlines(markdown::Mark::Bold, children, text),
        LegacyInline::Code(value) => {
            let start = text.text.len();
            text.text.push_str(value);
            text.marks.push(markdown::MarkSpan {
                range: start..text.text.len(),
                mark: markdown::Mark::Code,
            });
        }
        LegacyInline::Link {
            target, children, ..
        } => push_marked_inlines(markdown::Mark::Link(target.clone()), children, text),
        LegacyInline::Image { target, alt, .. } => {
            let start = text.text.len();
            text.text.push_str(alt);
            text.marks.push(markdown::MarkSpan {
                range: start..text.text.len(),
                mark: markdown::Mark::Image(target.clone()),
            });
        }
        LegacyInline::SoftBreak | LegacyInline::HardBreak => text.text.push('\n'),
    }
}

actions!(
    chat_composer,
    [
        Send,
        Cancel,
        CopyTranscript,
        PopupPrevious,
        PopupNext,
        PopupAccept
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
    duration_ms: Option<u64>,
}

/// One rendered element of the transcript.
#[derive(Clone, Debug)]
enum Entry {
    /// The user's own turn, shown as the gallery's right-aligned raised pill.
    User {
        text: String,
        at: Option<chrono::DateTime<chrono::Local>>,
    },
    /// A streamed assistant reply, grown in place as chunks arrive.
    ///
    /// The parsed tree is updated at ingestion time rather than during
    /// rendering, so a redraw never reparses the entire reply.
    Assistant {
        text: String,
        document: markdown::Doc,
    },
    /// A streamed reasoning chunk, visually distinct from the reply.
    ///
    /// `open` is the reader's say over the body (`widgets::Takeover`): until
    /// they press the header it follows the run — open while the thought
    /// streams, folded once it settles. `started` is view state (the first
    /// chunk's instant); `duration_ms` is what the settling entry stored and
    /// what persists. A restored thought has neither `started` nor a live
    /// run, so it opens closed as before (F-CHAT-21).
    Thought {
        text: String,
        open: bezel::ui::widgets::Takeover,
        started: Option<std::time::Instant>,
        duration_ms: Option<u64>,
    },
    /// A tool call, tracked by protocol id so later updates can patch it.
    ///
    /// `kind`, `content`, `locations`, `raw_input` and `raw_output` are the
    /// protocol's widened tool-call surface (F-CHAT-23/-31/-32) — a diff or
    /// text result, the files touched, and the raw input/output the agent
    /// reported, when it reported them. `expanded` starts `false`, same as
    /// `Thought` (F-CHAT-21): the card renders as one line until the reader
    /// opts in.
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
        duration_ms: Option<u64>,
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
    /// The front of the queue — what sends when the running turn ends.
    /// Kept as a flat string for the `surface.chat.read` readers that
    /// predate the multi-entry queue.
    pub queued_text: String,
    /// The whole queue, front first.
    pub queued: Vec<String>,
    pub transcript: Vec<BTreeMap<String, String>>,
}

impl Entry {
    fn plain_text(&self) -> String {
        match self {
            Self::User { text, .. } => text.clone(),
            Self::Thought { text, .. } => text.clone(),
            Self::Assistant { text, .. } => text.clone(),
            Self::ToolCall {
                title,
                status,
                content,
                locations,
                ..
            } => tool_call_plain_text(title, status, content, locations).text(),
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
        Entry::User { text, at } => Some(ChatEntry::UserMessage {
            text: text.clone(),
            at: at.map(|at| at.timestamp()),
        }),
        Entry::Assistant { text, .. } => Some(ChatEntry::AssistantMessage { text: text.clone() }),
        Entry::Thought {
            text, duration_ms, ..
        } => Some(ChatEntry::Thought {
            text: text.clone(),
            duration_ms: *duration_ms,
        }),
        Entry::ToolCall {
            id,
            title,
            status,
            kind,
            locations,
            duration_ms,
            ..
        } => Some(ChatEntry::ToolCall {
            id: id.clone(),
            title: title.clone(),
            status: status.clone(),
            // #168: the `..` used to drop both of these, so a restored row
            // named neither its tool nor the file it touched.
            kind: Some(kind.clone()),
            locations: locations
                .iter()
                .map(|location| ChatToolLocation {
                    path: location.path.to_string_lossy().into_owned(),
                    line: location.line,
                })
                .collect(),
            duration_ms: *duration_ms,
        }),
        // A subagent task carries neither a tool kind nor file locations, so
        // it stores what it has and restores exactly as it did before #168.
        Entry::SubagentTask {
            id, title, status, ..
        } => Some(ChatEntry::ToolCall {
            id: id.clone(),
            title: title.clone(),
            status: status.clone(),
            kind: None,
            locations: Vec::new(),
            duration_ms: None,
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
        ChatEntry::UserMessage { text, at } => Entry::User {
            text,
            at: at
                .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
                .map(|utc| utc.with_timezone(&chrono::Local)),
        },
        ChatEntry::AssistantMessage { text } => Entry::Assistant {
            document: parse_chat_markdown(&text),
            text,
        },
        ChatEntry::Thought { text, duration_ms } => Entry::Thought {
            text,
            open: Default::default(),
            started: None,
            duration_ms,
        },
        ChatEntry::ToolCall {
            id,
            title,
            status,
            kind,
            locations,
            duration_ms,
        } => Entry::ToolCall {
            id,
            title,
            status,
            // A row stored before #168 carries neither, and restores as the
            // generic label and the empty location list it always did.
            kind: kind.unwrap_or_else(|| "tool".into()),
            content: Vec::new(),
            locations: locations
                .into_iter()
                .map(|location| ToolCallLocationInfo {
                    path: PathBuf::from(location.path),
                    line: location.line,
                })
                .collect(),
            raw_input: None,
            raw_output: None,
            expanded: false,
            duration_ms,
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
    /// The agent's own process is gone — the ACP event channel closed with
    /// no terminal event, whether that happened mid-turn or while idle
    /// (F-CHAT-03; Swift's `ChatController.ChatState.disconnected`, set by
    /// the same "process terminated" observation). Distinct from
    /// `Connection`: the fix here is offered as "Restart agent" rather than
    /// "Retry" because there is no live prompt or request left to retry —
    /// the whole process is gone and the only way back is a fresh one,
    /// which is exactly what `Chat::retry` already does (it re-launches via
    /// `AcpClient::launch` either way; the two labels name the same call by
    /// what it does for each situation, not two different mechanisms).
    Disconnected,
    /// An MCP-configuration-flavored stderr line observed during the turn
    /// (F-CHAT-33) — informational, not a transport failure: the session is
    /// still live, so this is never retryable and never clears the client.
    McpWarning,
    /// The chat's agent cannot be launched on this machine at all — not
    /// installed, nothing published for this platform, or no ACP server
    /// known for it. Distinct from every other kind because nothing here
    /// failed: there is no request to retry and no process to restart, only
    /// a source to acquire, so the box offers Settings and withholds both
    /// Retry and the "OK to dismiss" every other error carries. Dismissing
    /// is the wrong affordance when this row IS the tab's whole content.
    Unavailable,
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
                 (for example, its `login` subcommand), then Retry — Sirio cannot \
                 complete authentication on the agent's behalf."
            ),
            ErrorKind::AuthRequired,
        )
    } else if message.to_ascii_lowercase().contains("transport closed") {
        // F-CHAT-03: this substring is `sirio_acp`'s own wording for a
        // genuinely dead transport, in two shapes — its worker's own
        // end-of-connection cleanup ("ACP transport closed unexpectedly"),
        // and, more commonly, whatever request happened to be in flight
        // when the process actually died, wrapped by that request's own
        // label ("prompt failed: Incoming transport closed: ..." — a real
        // agent killed mid-turn lands exactly here, not in the worker's own
        // cleanup, because the read loop resolves the pending request with
        // the transport error before the connection future itself
        // resolves). Both shapes mean the OS process is gone, unlike a
        // same-shaped-message business rejection ("set mode failed: no such
        // mode") that carries a JSON-RPC error, not a transport one, and
        // leaves the agent alive — those keep the generic Retry treatment.
        (message, ErrorKind::Disconnected)
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

/// The assistant-response copy affordance currently showing its short-lived
/// confirmation. Code blocks own their native bezel-markdown copy state.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CopyTarget {
    Assistant(usize),
}

/// A host-owned action requested by an edit-summary card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatEvent {
    OpenFile(PathBuf),
    /// F-BRW-09: a plain click on an HTTP(S) link in the transcript. The
    /// workspace owns tab creation, so it decides whether to open Sirio's
    /// internal browser tab; the human Cmd+Shift bypass to the system
    /// browser is handled locally (see [`TranscriptSelectableText`]'s mouse
    /// handler) and never reaches this event.
    OpenLink(String),
    /// F-CORE-DOM-07: a turn just finished settling into the transcript
    /// (`AcpEvent::TurnEnded` already handled -- footer pushed, streaming
    /// cleared, transcript persisted). This is the completion signal
    /// `request_auto_rename` needs and that no `ChatEvent` variant used to
    /// provide, so an ACP-hosted chat tab could never be auto-renamed: the
    /// throttle type was tested in isolation but nothing ever asked it a
    /// question. The workspace resolves which pane this chat lives in and
    /// synthesizes the running->done `Transition` `request_auto_rename`
    /// expects.
    TurnEnded,
    /// The Unavailable box's action. The workspace owns the Settings
    /// surface, so the chat states the problem and asks; it does not reach
    /// across and open a window itself.
    OpenSettings,
}

/// Per-tool-call state for the post-turn edited-files summary (F-CHAT-32).
#[derive(Clone, Debug, Default)]
pub(crate) struct EditSummaryState {
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
pub(crate) struct TranscriptInteraction {
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
        paint_wrapped_span(
            self.text.layout(),
            bounds,
            local_start..local_end,
            self.selection_fill,
            window,
            |_| {},
        );
    }
}

/// Shades the byte range `span` of a laid-out `StyledText` that may have
/// wrapped: one quad per visual line — from the span's start to the right
/// edge, full-width for every line in between, and from the left edge to the
/// span's end — so a selection reads as one continuous highlight however the
/// text broke. `on_quad` sees each quad's bounds as it is painted.
fn paint_wrapped_span(
    layout: &gpui::TextLayout,
    bounds: Bounds<Pixels>,
    span: Range<usize>,
    color: Rgba,
    window: &mut Window,
    mut on_quad: impl FnMut(Bounds<Pixels>),
) {
    if span.start >= span.end {
        return;
    }
    let line_height = layout.line_height();
    let start_position = layout
        .position_for_index(span.start)
        .unwrap_or(bounds.origin);
    let end_position = layout
        .position_for_index(span.end)
        .unwrap_or(point(bounds.right(), bounds.bottom() - line_height));
    let mut paint_line = |y: Pixels, left: Pixels, right: Pixels| {
        if right > left {
            let quad_bounds = Bounds::from_corners(
                point(left, y),
                point(right, (y + line_height).min(bounds.bottom())),
            );
            window.paint_quad(quad(
                quad_bounds,
                px(0.0),
                color,
                Edges::default(),
                transparent_black(),
                BorderStyle::default(),
            ));
            on_quad(quad_bounds);
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
                        // Sirio's internal browser and opens the link in
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

/// A chat surface wired to one live [`AcpClient`] session.
pub struct Chat {
    client: Option<AcpClient>,
    /// `None` when there is nothing to launch — see [`Chat::unavailable`].
    agent_command: Option<AgentCommand>,
    /// Display name shown in the empty composer placeholder when known.
    agent_name: Option<String>,
    agent_cwd: PathBuf,
    entries: Vec<Entry>,
    /// The draft, as bezel's field: IME, selection, undo, wrapping and scroll
    /// are its job. `Chat` observes it and reads `content()`/`cursor()` into
    /// `draft`/`draft_caret` on every change — the popups and Send read the
    /// cached pair rather than borrowing the entity mid-render.
    composer_field: Entity<TextField>,
    draft: SharedString,
    draft_caret: usize,
    /// File paths accepted from the `@` picker while their `@path` token is
    /// still in the draft. Lifted into the prompt's mention paths on send;
    /// cleared on send and on `control_compose`.
    accepted_mentions: Vec<String>,
    /// Images attached through the picker or a drop, drawn as a strip above
    /// the field. Not persisted, as before.
    attachments: Vec<ImageAttachment>,
    /// View state for the same-verb folds of a tool run, keyed by the fold's
    /// first entry index. Not persisted; a fold starts closed.
    open_verb_folds: HashSet<usize>,
    /// The placeholder last pushed into the field, so render pushes a new
    /// one only when the state it names changed.
    composer_placeholder_shown: String,
    /// F-CHAT-25: the question answer field (focus, draft, owner request).
    question_answer: QuestionAnswerState,
    /// The answer field's own caret. It lives on `Chat` rather than inside
    /// `QuestionAnswerState` because that struct is cloned once per frame
    /// for the render closure, and blink state must not be duplicated —
    /// one surface, one `Blink`, one timer.
    answer_blink: caret::Blink,
    answer_caret_visible: bool,
    /// The model picker's search row: a real bezel field (`Shape::Line`),
    /// focused while the picker is open; typing filters, Backspace edits,
    /// and none of it touches the composer's draft.
    model_search_field: Entity<TextField>,
    streaming: bool,
    /// Retired with the rotating streaming border: the shared Bezel clock
    /// drives the reasoning header now, so this stays permanently `false`.
    /// Kept as a regression guard — a repaint timer reappearing here would
    /// mean a Chat-owned animation crept back in. Read only by that guard
    /// test, hence the lint allowance.
    #[allow(dead_code)]
    streaming_border_timer_pending: bool,
    /// D-CHAT-03: the drafts committed (Enter) while a turn streams, front
    /// first. Each turn end sends exactly one — the front — so the rest
    /// wait for the turn that send starts. Empty when nothing is queued.
    queue: VecDeque<String>,
    /// Whether the queue block above the composer shows its entries or
    /// only its counting header. Starts unfolded; the reader's toggle
    /// holds for the life of the surface and is not persisted.
    queue_expanded: bool,
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
    /// F-CHAT-16: the model picker's own search query — live in the field,
    /// read from it at render time. Matches `ModelPickerFilter`'s Swift
    /// semantics — trimmed, case-insensitive substring match against
    /// name/id/description, order preserved, empty query keeps every model.
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
    mode_picker_focus: FocusHandle,
    context_popover_focus: FocusHandle,
    transcript_focus: FocusHandle,
    context_usage: Option<ContextUsage>,
    list_state: ListState,
    /// The transcript scrollbar's grab state (bezel's bar over the list).
    transcript_bar: list_scroll::ListScrollbarState,
    transcript_selection: Option<TranscriptSelection>,
    transcript_dragging: bool,
    /// F-CHAT-22, turn half: turns the reader has explicitly re-opened,
    /// keyed by the entry index of the [`Entry::TurnFooter`] that closes
    /// each one — the analogue of Swift's `controller.unfoldedTurns`, whose
    /// key is `divider.id`.
    ///
    /// An index is a safe identity only because entries are otherwise
    /// append-only; the two paths that do remove entries
    /// ([`Self::clear_recovered_connection_errors`] and the two resets) empty
    /// this set as well, rather than leave keys pointing at whatever slid
    /// into their place.
    unfolded_turns: BTreeSet<usize>,
    copied_target: Option<CopyTarget>,
    edit_summaries: BTreeMap<usize, EditSummaryState>,
    /// When each live tool call started, by protocol id, so its first
    /// terminal status can store the elapsed time on the entry. Consumed on
    /// that status; a call that never settles simply leaves its start here
    /// until the chat is dropped.
    tool_started: HashMap<String, std::time::Instant>,
    /// Per-entry scroll state for open thought bodies, keyed by entry index.
    /// Not persisted; created the first time a thought's body is drawn,
    /// cleared with the entries.
    thought_scroll: HashMap<usize, thought::ThoughtScroll>,
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
    /// Ranked views over `available_commands` and `mention_candidates`, with
    /// the keyboard's active row — bezel's own picker state.
    slash_filter: popover::Filter,
    mention_filter: popover::Filter,
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
    /// Launches a real ACP agent from the command the caller resolved.
    ///
    /// There is deliberately no default: a hardcoded `npx …@latest` here
    /// meant every chat tab could start a network fetch before it could say
    /// anything, and silently connected a tab to Claude's server whatever
    /// agent the user picked. `SIRIO_ACP_PROGRAM` stays as a test escape
    /// hatch, because integration tests need one.
    pub fn launch_from_env(cx: &mut Context<Self>) -> Option<Self> {
        let command = std::env::var_os("SIRIO_ACP_PROGRAM")
            .map(PathBuf::from)
            .map(AgentCommand::new)?;
        Some(Self::launch_with_command(command, default_agent_cwd(), cx))
    }

    /// Launches a real ACP agent from an explicit command and returns a
    /// `Chat` wired to its event stream.
    ///
    /// This is the picker's door: the caller resolves the chosen adapter's
    /// launch source (Task 8) and converts it with `agent_command_for`, so
    /// the tab connects to the source the user actually has rather than a
    /// hard-coded default.
    pub fn launch_with_command(
        command: AgentCommand,
        cwd: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut chat = Self::new(Some(command), cwd, cx);
        chat.start_connection(cx);

        chat
    }

    /// Sets the agent display name used by the empty composer placeholder.
    pub fn set_agent_name(&mut self, name: impl Into<String>) {
        self.agent_name = Some(name.into());
    }

    /// What the composer's agent badge shows (#206).
    ///
    /// It names the *agent*, so it reads `agent_name` -- not
    /// `selected_model_name`, whose fallback is the literal "Claude Code".
    /// Reading the model there named the wrong agent for every agent that
    /// is not Claude Code until models arrived, and named an agent at all
    /// for a restored chat whose banner says the agent is unknowable.
    /// `agent_name` is `None` in exactly that case.
    fn agent_badge_name(&self) -> String {
        self.agent_name
            .clone()
            .unwrap_or_else(|| "Unknown agent".into())
    }

    fn default_placeholder(&self) -> String {
        // The gallery's sentence with Sirio's tokens. The agent's name is
        // not spelled out here — the pill and the model chip in the toolbar
        // above the card already name it.
        if self.available_commands.is_empty() {
            "Ask anything, or @ to attach a file".into()
        } else {
            "Ask anything, / for commands, or @ to attach a file".into()
        }
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
        let mut chat = Self::new(Some(command), cwd, cx);
        chat.persistence = Some(ChatPersistence {
            database_path,
            tab_id,
            worktree_id,
        });
        chat.restore_persisted_transcript();
        chat.start_connection(cx);
        chat
    }

    /// [`Self::launch_from_env`] with persistence: the same env-provided
    /// command, wired to save/restore its transcript and to browse the
    /// worktree's Chat History (F-CHAT-34). `None` when no command is set.
    pub fn launch_with_persistence(
        database_path: PathBuf,
        tab_id: String,
        worktree_id: String,
        cx: &mut Context<Self>,
    ) -> Option<Self> {
        let cwd = default_agent_cwd();
        let command = std::env::var_os("SIRIO_ACP_PROGRAM")
            .map(PathBuf::from)
            .map(AgentCommand::new)?;
        Some(Self::launch_with_command_and_persistence(
            command,
            cwd,
            database_path,
            tab_id,
            worktree_id,
            cx,
        ))
    }

    /// `command` is `None` for a chat that has nothing to launch — see
    /// [`Chat::unavailable`]. Storing a placeholder command instead would
    /// reintroduce, in miniature, the exact lie this crate spent a branch
    /// removing: a command that names something nobody verified exists.
    fn new(command: Option<AgentCommand>, cwd: PathBuf, cx: &mut Context<Self>) -> Self {
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

        let model_search_field = cx.new(|cx| {
            TextField::new(cx)
                .with_placeholder("Search models\u{2026}")
                .with_key_context("ChatModelSearch")
        });
        cx.observe(&model_search_field, |_, _, cx| cx.notify())
            .detach();

        let composer_field = cx.new(|cx| {
            TextField::new(cx)
                .with_shape(bezel::ui::input::Shape::Grow { min: 3, max: 12 })
                .with_key_context("ChatComposer")
        });
        // Both content and caret changes notify, and the mention behind the
        // caret changes when either does.
        cx.observe(&composer_field, |chat: &mut Self, _, cx| {
            chat.reread_composer(cx)
        })
        .detach();

        Self {
            client: None,
            agent_command: command,
            agent_name: None,
            agent_cwd: cwd,
            entries: Vec::new(),
            composer_field,
            draft: SharedString::default(),
            draft_caret: 0,
            accepted_mentions: Vec::new(),
            attachments: Vec::new(),
            open_verb_folds: HashSet::new(),
            composer_placeholder_shown: String::new(),
            answer_blink: caret::Blink::new(),
            answer_caret_visible: false,
            model_search_field,
            question_answer: QuestionAnswerState {
                focus: cx.focus_handle().tab_stop(true),
                draft: String::new(),
                for_request: None,
            },
            mode_picker_focus: cx.focus_handle().tab_stop(true),
            context_popover_focus: cx.focus_handle().tab_stop(true),
            overflow_focus: cx.focus_handle().tab_stop(true),
            transcript_focus: cx.focus_handle().tab_stop(false),
            streaming: false,
            streaming_border_timer_pending: false,
            queue: VecDeque::new(),
            queue_expanded: true,
            connecting: false,
            has_completed_turn: false,
            mcp_warnings_shown: 0,
            available_models: Vec::new(),
            model_config_id: None,
            selected_model: None,
            model_picker_open: false,
            mode_catalog: None,
            mode_picker_open: false,
            context_popover_open: false,
            context_usage: None,
            list_state,
            transcript_bar: list_scroll::ListScrollbarState::new(bezel::motion::Painter::of(cx)),
            transcript_selection: None,
            transcript_dragging: false,
            unfolded_turns: BTreeSet::new(),
            copied_target: None,
            edit_summaries: BTreeMap::new(),
            tool_started: HashMap::new(),
            thought_scroll: HashMap::new(),
            persistence: None,
            _event_task: None,
            available_commands: Vec::new(),
            slash_dismissed: false,
            last_slash_token: None,
            slash_filter: popover::Filter::new(Vec::new()),
            mention_candidates: Vec::new(),
            mention_query: None,
            mention_filter: popover::Filter::new(Vec::new()),
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

    /// When each live tool call started, by protocol id, so its first
    /// terminal status can store the elapsed time on the entry. Consumed on
    /// that status; a call that never settles simply leaves its start here
    /// until the chat is dropped.
    fn note_tool_started(&mut self, id: &str) {
        self.tool_started
            .insert(id.to_string(), std::time::Instant::now());
    }

    /// The elapsed time for `id` if `status` is terminal and the call's start
    /// is known; `None` otherwise. Consumes the start.
    fn tool_duration_on(&mut self, id: &str, status: &str) -> Option<u64> {
        if !is_terminal_tool_status(status) {
            return None;
        }
        self.tool_started
            .remove(id)
            .map(|started| started.elapsed().as_millis() as u64)
    }

    /// Add one transcript row and keep the virtualizer's index tree in sync.
    fn push_entry(&mut self, entry: Entry) {
        let index = self.entries.len();
        let following_tail = self.list_state.is_following_tail();
        if !matches!(entry, Entry::Thought { .. }) {
            self.settle_open_thought();
        }
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

    /// F-CHAT-22's fold for one verb inside a tool run. The fold's row is
    /// measured off the run's tail entry, so remeasure that one.
    pub(crate) fn toggle_verb_fold(&mut self, start: usize, cx: &mut Context<Self>) {
        if !self.open_verb_folds.insert(start) {
            self.open_verb_folds.remove(&start);
        }
        // The run's row is measured off its tail entry; the fold changed its height.
        if let Some((_, end)) = tool_call_run_bounds_inclusive(&self.entries, start) {
            self.remeasure_entry(end);
        }
        cx.notify();
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

    /// F-CHAT-22, turn half: flips one older turn between its collapsed
    /// stand-in row and its full contents, in place.
    ///
    /// `turn_id` is the turn's footer index. Every row of the turn changes
    /// height at once, so the whole span is remeasured — remeasuring only
    /// the clicked row would leave the virtualizer holding stale heights for
    /// the rows that just appeared, and the transcript would jump.
    fn toggle_turn_unfolded(&mut self, turn_id: usize, cx: &mut Context<Self>) {
        let turns = segment_turns(&self.entries);
        let Some(turn) = turns.iter().find(|turn| turn.footer == Some(turn_id)) else {
            return;
        };
        if !self.unfolded_turns.remove(&turn_id) {
            self.unfolded_turns.insert(turn_id);
        }
        self.list_state.remeasure_items(turn.start..turn.end + 1);
        cx.notify();
    }

    /// Install the composer keymap in the host application.
    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("enter", Send, Some("ChatComposer")),
            KeyBinding::new("return", Send, Some("ChatComposer")),
            KeyBinding::new(
                "shift-enter",
                bezel::ui::input::InsertNewline,
                Some("ChatComposer"),
            ),
            KeyBinding::new(
                "shift-return",
                bezel::ui::input::InsertNewline,
                Some("ChatComposer"),
            ),
            KeyBinding::new("escape", Cancel, Some("ChatComposer")),
            KeyBinding::new("up", PopupPrevious, Some("ChatComposer")),
            KeyBinding::new("down", PopupNext, Some("ChatComposer")),
            KeyBinding::new("tab", PopupAccept, Some("ChatComposer")),
            KeyBinding::new("ctrl-c", CopyTranscript, Some("ChatTranscript")),
            KeyBinding::new("escape", Cancel, Some("ChatModelPicker")),
            // The model picker's search field owns focus while the picker is
            // open, so Escape has to resolve from its context too.
            KeyBinding::new("escape", Cancel, Some("ChatModelSearch")),
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
                    *document = parse_chat_markdown(existing);
                    self.remeasure_entry(self.entries.len() - 1);
                } else {
                    self.push_entry(Entry::Assistant {
                        document: parse_chat_markdown(&text),
                        text,
                    });
                }
            }
            AcpEvent::ThoughtChunk(text) => {
                if let Some(Entry::Thought { text: existing, .. }) = self.entries.last_mut() {
                    existing.push_str(&text);
                    self.remeasure_entry(self.entries.len() - 1);
                } else {
                    // A new thought re-follows: drop any stale scroll state
                    // for this index so the next draw starts pinned.
                    self.thought_scroll.remove(&self.entries.len());
                    self.push_entry(Entry::Thought {
                        text,
                        open: Default::default(),
                        started: Some(std::time::Instant::now()),
                        duration_ms: None,
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
                self.note_tool_started(&id);
                let duration_ms = self.tool_duration_on(&id, &status);
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
                            duration_ms,
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
                        duration_ms,
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
                let measured = status
                    .as_deref()
                    .and_then(|status| self.tool_duration_on(&id, status));
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
                        if let Some(ms) = measured {
                            call.duration_ms = Some(ms);
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
                    duration_ms: existing_duration_ms,
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
                    if let Some(ms) = measured {
                        *existing_duration_ms = Some(ms);
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
                let measured = self.tool_duration_on(&id, &status);
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
                        if let Some(ms) = measured {
                            call.duration_ms = Some(ms);
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
                    duration_ms: existing_duration_ms,
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
                    if let Some(ms) = measured {
                        *existing_duration_ms = Some(ms);
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
                self.slash_filter = popover::Filter::new(
                    self.available_commands
                        .iter()
                        .map(|command| SharedString::from(command.name.clone()))
                        .collect(),
                );
                if let Some(query) = slash_token(&self.draft) {
                    self.slash_filter.refilter(query);
                }
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
                self.settle_open_thought();
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
                // one rule — drains the queue's front entry as the next
                // turn, exactly once; the rest wait for that turn's end.
                // The footer lands before the queued turn so the transcript
                // reads: stop stated, then the redirect.
                self.send_queued_item(cx);
                // F-CORE-DOM-07: tell the workspace a turn just settled so
                // throttled auto-naming has a signal to react to. Emitted
                // after the transcript is persisted so a subscriber reading
                // `transcript_for_resume()` in response sees this turn's
                // reply included.
                cx.emit(ChatEvent::TurnEnded);
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
            && (!self.draft.trim().is_empty() || !self.attachments.is_empty())
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
        self.draft.to_string()
    }

    /// Replaces the visible composer's plain-text draft through the control
    /// socket route. Attachments deliberately remain a pointer-only concern.
    pub fn control_compose(&mut self, text: &str, cx: &mut Context<Self>) {
        self.accepted_mentions.clear();
        self.attachments.clear();
        self.reset_composer_popups();
        self.set_composer_text(text.to_string(), cx);
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
            composer_text: self.draft.to_string(),
            queued_text: self.queue.front().cloned().unwrap_or_default(),
            queued: self.queue.iter().cloned().collect(),
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
        // F-CHAT-22: `unfolded_turns` is keyed by entry index, so anything
        // that renumbers entries must drop it rather than let a key point at
        // whatever slid into its place.
        self.unfolded_turns.clear();
        self.thought_scroll.clear();
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

    /// The first thing the user actually asked, for naming the session.
    ///
    /// Read from the entries rather than parsed out of `transcript_text`,
    /// because the transcript is prose: "the first line" is the agent's
    /// words, or a tool call, depending on where the session was cut.
    ///
    /// The entries are the reliable source across both restore paths, but
    /// not an identical one. A chat reloaded from the database keeps its
    /// real user turns (`restored_entry` maps `ChatEntry::UserMessage` back
    /// to `Entry::User`), so this finds the original prompt. A chat brought
    /// back through `restore_transcript` -- flat retained text, no structure
    /// -- becomes a single *assistant* entry, and this correctly returns
    /// `None` rather than offering the agent's words as the user's.
    pub fn first_user_prompt(&self) -> Option<String> {
        self.entries.iter().find_map(|entry| match entry {
            Entry::User { text, .. } if !text.trim().is_empty() => Some(text.clone()),
            _ => None,
        })
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
            document: parse_chat_markdown(transcript),
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
                    kind: ErrorKind::Connection | ErrorKind::AuthRequired | ErrorKind::Disconnected,
                    ..
                }
            )
        });
        if self.entries.len() != old_count {
            self.unfolded_turns.clear();
            self.thought_scroll.clear();
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
            self.model_search_field
                .update(cx, |field, cx| field.clear(cx));
            let focus = self.model_search_field.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
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
            // #136: a mode can reach us by either of two wires, and they are
            // not interchangeable. ACP's own session-modes API answers
            // `session/set_mode`; a mode advertised as a `configOptions`
            // select (OpenCode's route, tagged `category: "mode"`) answers
            // `session/set_config_option` and ignores the former — picking
            // the wrong one leaves the picker looking live while changing
            // nothing, which is worse than the bug this fixed.
            match self
                .mode_catalog
                .as_ref()
                .and_then(|catalog| catalog.config_option_id.clone())
            {
                Some(option_id) => {
                    let _ = client.set_config_option(option_id, mode.id.clone());
                }
                None => {
                    let _ = client.set_mode(mode.id.clone());
                }
            }
        }
        if let Some(catalog) = &mut self.mode_catalog {
            catalog.current_id = mode.id;
        }
        self.mode_picker_open = false;
        cx.notify();
    }

    /// #136: whether the mode pill is a control the user can act on. Gated
    /// on the catalog alone, never on `has_completed_turn`: the mode arrives
    /// with `session/new`, before a token is exchanged, and "will this edit
    /// my repo without asking me" is the first question a fresh chat has to
    /// answer — not one it earns by replying once.
    fn mode_selectable(&self) -> bool {
        self.mode_catalog.is_some()
    }

    /// #136's twin for the model selector: offered as soon as the agent has
    /// named models, which `Chat::launch` reads off `AcpClient::model_catalog`
    /// before consuming a single event.
    fn model_control_visible(&self) -> bool {
        !self.available_models.is_empty()
    }

    // --- Slash-command popup (F-CHAT-09) ---

    /// The popup's candidate list: the filter's ranked view, resolved back
    /// to their commands, capped at ten rows like the reference
    /// `slashCandidates`. The filter also ranks substring matches, but the
    /// slash popup's contract is prefix-only, so those are dropped here.
    fn slash_candidates(&self) -> Vec<&AvailableCommandInfo> {
        let Some(query) = slash_token(&self.draft) else {
            return Vec::new();
        };
        let query = query.to_lowercase();
        self.slash_filter
            .filtered()
            .iter()
            .take(10)
            .filter_map(|&item| {
                let name = self.slash_filter.items()[item].as_ref();
                let command = self
                    .available_commands
                    .iter()
                    .find(|command| command.name == name)?;
                let matches = query.is_empty() || command.name.to_lowercase().starts_with(&query);
                matches.then_some(command)
            })
            .collect()
    }

    fn slash_popup_visible(&self) -> bool {
        !self.slash_candidates().is_empty() && !self.slash_dismissed
    }

    fn accept_slash_command(&mut self, name: &str, cx: &mut Context<Self>) {
        self.slash_dismissed = true;
        self.set_composer_text(format!("/{name} "), cx);
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
                    chat.mention_filter = popover::Filter::new(
                        chat.mention_candidates
                            .iter()
                            .map(|path| SharedString::from(path.clone()))
                            .collect(),
                    );
                    cx.notify();
                }
            });
        }));
    }

    fn accept_mention(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some((at, _)) = mention_token(&self.draft, self.draft_caret) else {
            return;
        };
        let caret = self.draft_caret.min(self.draft.len());
        let text = format!("{}@{path} {}", &self.draft[..at], &self.draft[caret..]);
        if !self.accepted_mentions.iter().any(|known| known == path) {
            self.accepted_mentions.push(path.to_string());
        }
        self.mention_candidates.clear();
        self.mention_filter = popover::Filter::new(Vec::new());
        self.set_composer_text(text, cx);
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
        self.attachments.push(ImageAttachment {
            mime_type: mime.to_string(),
            base64_data: base64,
        });
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
                self.attachments.push(ImageAttachment {
                    mime_type: mime.to_string(),
                    base64_data: base64,
                });
            } else {
                // Relative to the worktree when the file lives inside it
                // (matching the `@`-mention chip's own path convention),
                // absolute otherwise.
                let chip_path = path
                    .strip_prefix(&self.agent_cwd)
                    .map(|relative| relative.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| path.display().to_string());
                // A file drop becomes a `@path ` mention token in the draft,
                // the same thing the `@` picker inserts — no chip model any
                // more, just text plus the recorded path.
                let caret = self.draft_caret.min(self.draft.len());
                let text = format!(
                    "{}@{} {}",
                    &self.draft[..caret],
                    chip_path,
                    &self.draft[caret..]
                );
                if !self.accepted_mentions.contains(&chip_path) {
                    self.accepted_mentions.push(chip_path);
                }
                self.set_composer_text(text, cx);
            }
        }
        self.refresh_token_popups(cx);
        if !rejections.is_empty() {
            self.show_attach_error(rejections.join("; "), cx);
        }
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
        self.unfolded_turns.clear();
        self.thought_scroll.clear();
        self.list_state.splice(0..old_count, 0);
        self.accepted_mentions.clear();
        self.attachments.clear();
        self.set_composer_text("", cx);
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
        let (text, mention_paths) = assemble_prompt(&self.draft, &self.accepted_mentions);
        let images = std::mem::take(&mut self.attachments);
        self.accepted_mentions.clear();
        self.reset_composer_popups();
        self.set_composer_text("", cx);
        self.submit_turn(text, mention_paths, images, cx);
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
        self.push_entry(Entry::User {
            text: text.clone(),
            at: Some(chrono::Local::now()),
        });
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

    /// D-CHAT-03: while a turn streams, a send commits the draft as a new
    /// entry at the back of the queue. Mirrors `send`'s consumption of the
    /// composer; an empty draft commits nothing and leaves the queue as
    /// it was.
    fn commit_queued_item(&mut self, cx: &mut Context<Self>) {
        if self.draft.trim().is_empty() && self.attachments.is_empty() {
            return;
        }
        let (text, _) = assemble_prompt(&self.draft, &self.accepted_mentions);
        self.accepted_mentions.clear();
        self.reset_composer_popups();
        self.set_composer_text("", cx);
        self.queue.push_back(text);
        cx.notify();
    }

    /// D-CHAT-03: the ✕ on one entry. That entry is dropped; the others
    /// keep their order.
    fn remove_queued_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        self.queue.remove(index);
        cx.notify();
    }

    /// "Clear all" in the queue header: nothing sends when the turn ends.
    fn clear_queue(&mut self, cx: &mut Context<Self>) {
        self.queue.clear();
        cx.notify();
    }

    /// The queue header's disclosure: folds the entries away or unfolds
    /// them. The counting header stays either way.
    fn toggle_queue_folded(&mut self, cx: &mut Context<Self>) {
        self.queue_expanded = !self.queue_expanded;
        cx.notify();
    }

    /// "Send now" on one entry. The entry jumps to the front and the
    /// running turn is cancelled, so the cancelled turn's end sends it
    /// through the one turn-end rule — the same redirect a stop with a
    /// queued entry already is, with no second send path to keep in step.
    /// With no turn running (a queue that outlived its turn because the
    /// transport died) it sends straight away.
    fn send_queued_entry_now(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(text) = self.queue.remove(index) else {
            return;
        };
        self.queue.push_front(text);
        if self.streaming {
            self.cancel_turn(cx);
        } else {
            self.send_queued_item(cx);
        }
        cx.notify();
    }

    /// D-CHAT-03: drains the front entry as the next user turn. Only the
    /// turn-end path calls this — completed and cancelled alike — so each
    /// entry sends exactly once, and the next waits for this turn's end.
    /// A turn end without a client is a transport death: the queue stays
    /// as it is rather than firing nowhere.
    fn send_queued_item(&mut self, cx: &mut Context<Self>) {
        if self.client.is_none() {
            return;
        }
        let Some(text) = self.queue.pop_front() else {
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
        self.mention_candidates.clear();
        self.mention_query = None;
        self.mention_filter = popover::Filter::new(Vec::new());
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

    fn send_action(&mut self, _: &Send, _: &mut Window, cx: &mut Context<Self>) {
        if self.accept_active_popup_row(cx) {
            cx.notify();
            return;
        }
        self.send(cx);
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
        } else if self.open_token_popup() != TokenPopup::None {
            self.slash_dismissed = true;
            self.mention_candidates.clear();
            self.mention_filter = popover::Filter::new(Vec::new());
            cx.notify();
        } else {
            self.cancel_turn(cx);
        }
    }

    /// Recomputes the token-driven popup state after any edit: the slash
    /// token resets its dismissal/selection, the mention token starts or
    /// cancels its candidate walk.
    fn refresh_token_popups(&mut self, cx: &mut Context<Self>) {
        let slash = slash_token(&self.draft).map(str::to_string);
        if slash != self.last_slash_token {
            self.slash_dismissed = false;
            if let Some(query) = &slash {
                self.slash_filter.refilter(query);
            }
            self.last_slash_token = slash;
        }
        let mention =
            mention_token(&self.draft, self.draft_caret).map(|(_, token)| token.to_string());
        if mention != self.mention_query {
            self.mention_candidates.clear();
            self.mention_filter = popover::Filter::new(Vec::new());
            self.mention_query = mention;
            self.schedule_mention_walk(cx);
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

        let Some(command) = self.agent_command.clone() else {
            // An unavailable chat has no command by construction, so there
            // is nothing to spawn and no failure to report either.
            return;
        };

        self.client.take();
        self.connecting = true;
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
                    // error path — mid-turn or, just as often, while
                    // sitting idle: the composer must never keep offering
                    // Send against a process that is simply gone, so this
                    // fires either way, not only while streaming
                    // (F-CHAT-03). `chat.client` still being `Some` here is
                    // what tells the two apart from an already-handled
                    // `TransportError`/`Timeout`, both of which already took
                    // it — this only runs when nothing else has.
                    let _ = this.update(cx, |chat, cx| {
                        if chat.client.is_some() {
                            chat.client.take();
                            if chat.streaming {
                                chat.expire_unanswered();
                            }
                            chat.push_entry(Entry::Error {
                                message: "Agent disconnected — the process has terminated."
                                    .to_string(),
                                // Restarting is exactly the right response
                                // here, so offer it rather than dead-ending
                                // (rendered as "Restart agent", not "Retry"
                                // — F-CHAT-03).
                                retryable: true,
                                kind: ErrorKind::Disconnected,
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

    /// F-CHAT-33: the "OK to dismiss" affordance the VERIFY clause and the
    /// Swift reference both require (`ChatPaneView.swift:177-192`'s
    /// `promptError`/`mcpWarning` banners, each with `actionTitle: "OK"`
    /// clearing the flag and nothing else). This port keeps errors as
    /// permanent transcript rows rather than Swift's transient bottom
    /// overlay, so the closest equivalent of "clear the banner without
    /// touching the transcript" is removing exactly this one row and
    /// leaving every other entry untouched -- unlike `retry`, which
    /// re-launches the agent, this never does anything but acknowledge.
    /// A stale `entry_index` (the entry already gone, e.g. a double click
    /// racing a re-render) is a no-op rather than panicking or removing the
    /// wrong row.
    fn dismiss_error(&mut self, index: usize, cx: &mut Context<Self>) {
        if !matches!(self.entries.get(index), Some(Entry::Error { .. })) {
            return;
        }
        self.entries.remove(index);
        self.list_state.splice(index..index + 1, 0);
        cx.notify();
    }

    /// A chat that states why it cannot run and never starts anything.
    ///
    /// `reason` is the caller's already-user-facing prose — the same text
    /// the Agents screen renders as its pill — so this method neither
    /// invents wording nor knows about launch sources.
    pub fn unavailable(reason: String, cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        let mut chat = Self::new(None, cwd, cx);
        chat.push_entry(Entry::Error {
            message: reason,
            retryable: false,
            kind: ErrorKind::Unavailable,
        });
        chat
    }

    #[cfg(test)]
    fn from_test_command(command: AgentCommand, cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        let mut chat = Self::new(Some(command), cwd, cx);
        chat.start_connection(cx);
        chat
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
            self.answer_blink.wake();
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
        // F-CHAT-16: the model picker's search is a real TextField — typing,
        // Backspace and Escape are the field's own job; no raw-key redirect
        // lives here any more.
        if event.keystroke.key == "c"
            && event.keystroke.modifiers.control
            && self.transcript_selection.is_some()
        {
            self.copy_transcript(&CopyTranscript, _window, cx);
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
        caret_visible: bool,
    ) -> AnyElement {
        let typography = theme.typography;
        let field_entity = entity.clone();
        let send_entity = entity.clone();
        let cancel_entity = entity.clone();
        let placeholder = input.placeholder();
        let prefill_for_click = input.prefill.clone();
        // The bar always occupies layout, so the answer text does not shift
        // by two pixels every half second as it blinks. An empty field
        // carries it at the placeholder's start (`caret::field_placeholder`,
        // bezel's `TextField` convention), a draft after its last character.
        let answer_caret = || {
            div()
                .debug_selector(|| "question-answer-caret".into())
                .child(caret::bar(
                    typography.body_line_height,
                    theme.text,
                    caret_visible,
                ))
                .into_any_element()
        };

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
                    .bg(theme.surface_raised)
                    .border_1()
                    .border_color(if question_answer.for_request == Some(request_id) {
                        theme.text
                    } else {
                        theme.border
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
                    .flex()
                    .items_center()
                    // A long answer is clipped from the start, so the tail
                    // being typed stays in view (`caret::field_value`); the
                    // placeholder keeps its start (`caret::field_placeholder`).
                    .overflow_hidden()
                    .child(
                        if question_answer.draft.is_empty() {
                            caret::field_placeholder(
                                div().text_color(theme.text_faint).child(placeholder),
                                Some(answer_caret()),
                            )
                        } else {
                            caret::field_value(
                                div()
                                    .text_color(theme.text)
                                    .child(question_answer.draft.clone()),
                            )
                        }
                        .debug_selector(|| "question-answer-text".into()),
                    )
                    .children((!question_answer.draft.is_empty()).then(answer_caret)),
            )
            .child(
                div()
                    .id(("question-answer-send", request_id))
                    .debug_selector(|| "question-answer-send".into())
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(theme.radii.control)
                    .bg(theme.surface_raised)
                    .text_size(typography.footnote)
                    .text_color(theme.text)
                    .hover(|style| style.bg(theme.overlay))
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
                    .text_color(theme.danger)
                    .hover(|style| style.bg(theme.overlay))
                    .on_click(move |_, _, cx| {
                        cancel_entity.update(cx, |chat, cx| {
                            chat.cancel_question(request_id, cx);
                        });
                    })
                    .child("Cancel"),
            )
            .into_any_element()
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
                theme.element_active,
                Vec::new(),
            )
            .into_any_element()
        } else {
            styled.into_any_element()
        }
    }

    /// Shared bezel-markdown renderer used by file tabs. The legacy parsed
    /// tree is converted to bezel's flat Doc at the seam; BlockLayouts then
    /// lets the existing per-render link callback win before bezel's default
    /// external opener runs.
    pub(crate) fn render_markdown_document_with_link_override(
        document: LegacyDocument,
        _theme: &Theme,
        link_click: LinkClickOverride,
    ) -> AnyElement {
        MarkdownBody::with_link_override(bezel_doc_from_legacy(document), link_click)
            .into_any_element()
    }
    /// F-CHAT-31: a diff preview for a tool call that changed a file —
    /// removed lines then added lines at each point of divergence, capped
    /// so one huge rewrite cannot make the transcript unusable.
    ///
    /// Each row carries the line number of the file it belongs to, the way
    /// Swift's `ChatDiffPreviewView.row` leads with `String(format: "%3d",
    /// row.lineNumber)`; the header path is a real control that opens the
    /// file, where Swift puts a `Button` calling `openFileReference`; and
    /// the row text is selectable through the transcript's own selection
    /// mechanism, standing in for Swift's `.textSelection(.enabled)`.
    fn render_tool_diff(
        diff: &ToolCallDiff,
        theme: &Theme,
        context: DiffPreviewContext,
    ) -> AnyElement {
        let typography = theme.typography;
        let lines = diff_preview_lines(diff.old_text.as_deref(), &diff.new_text);
        let total = lines.len();
        let added = lines
            .iter()
            .filter(|line| matches!(line, DiffLine::Added { .. }))
            .count();
        let removed = lines
            .iter()
            .filter(|line| matches!(line, DiffLine::Removed { .. }))
            .count();
        let shown = lines
            .into_iter()
            .take(DIFF_PREVIEW_MAX_LINES)
            .collect::<Vec<_>>();
        let DiffPreviewContext {
            id_prefix,
            entity,
            selection,
        } = context;
        let open_path = diff.path.clone();
        let open_entity = entity.clone();
        let header_id = format!("{id_prefix}-open");
        let header_selector = header_id.clone();
        let scroll_id = format!("{id_prefix}-scroll");
        let mut scroll = div()
            .id(SharedString::from(scroll_id))
            .w_full()
            .overflow_x_scroll()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(8.0));
        for (index, line) in shown.iter().enumerate() {
            // Bezel's gallery diff uses a 10% success/danger wash, not the
            // 12% `VEIL_MID` `sirio_theme::diff_*_background` tokens (those
            // stay theme-wide and out of this task's `chat.rs`-only scope),
            // so the wash is built locally from the same solid colour.
            let (prefix, text_color, background) = match line {
                DiffLine::Context { .. } => (" ", theme.text, None),
                DiffLine::Removed { .. } => (
                    "-",
                    theme.diff_del,
                    Some(Rgba {
                        a: 0.10,
                        ..theme.diff_del
                    }),
                ),
                DiffLine::Added { .. } => (
                    "+",
                    theme.diff_add,
                    Some(Rgba {
                        a: 0.10,
                        ..theme.diff_add
                    }),
                ),
            };
            // The lockstep diff keeps `old_index`/`new_index` in sync at
            // every `Context` line, so its single stored `number` is both
            // columns there; `Removed`/`Added` exist on only one side.
            let (old_number, new_number) = match line {
                DiffLine::Context { number, .. } => (number.to_string(), number.to_string()),
                DiffLine::Removed { number, .. } => (number.to_string(), String::new()),
                DiffLine::Added { number, .. } => (String::new(), number.to_string()),
            };
            let text = line.text().to_string();
            let body = match selection
                .as_ref()
                .and_then(|selection| selection.line_starts.get(index).copied())
            {
                // The transcript's own selection element, not a second
                // mechanism: the range is expressed in `transcript_text`
                // coordinates, so a drag started on assistant prose and
                // ended on a diff row produces one continuous selection and
                // one Ctrl+C.
                Some(start) => TranscriptSelectableText::new(
                    ElementId::Name(SharedString::from(format!("{id_prefix}-text-{index}"))),
                    StyledText::new(text.clone()),
                    start..start + text.len(),
                    selection
                        .as_ref()
                        .expect("selection present in this arm")
                        .interaction
                        .clone(),
                    theme.element_active,
                    Vec::new(),
                )
                .into_any_element(),
                None => div().child(text).into_any_element(),
            };
            let row_selector = format!("{id_prefix}-line-{index}");
            let number_selector = format!("{id_prefix}-number-{index}");
            let text_selector = format!("{id_prefix}-text-{index}");
            let mut row = div()
                .id(SharedString::from(row_selector.clone()))
                .debug_selector(move || row_selector.clone())
                .flex()
                .items_start()
                .min_w_full()
                .px(px(10.0))
                .py(px(1.0))
                .font_family(typography.code_family)
                .text_size(typography.scaled(12.0))
                .line_height(px(18.0))
                .text_color(text_color)
                .child(
                    // One debug selector spans both columns: existing
                    // callers (F-CHAT-31) address a row's gutter as a
                    // single element, and that selector should not have to
                    // change just because the gutter now has two fields.
                    div()
                        .debug_selector(move || number_selector.clone())
                        .flex_none()
                        .flex()
                        .child(
                            div()
                                .w(px(DIFF_GUTTER_WIDTH))
                                .pr(px(4.0))
                                .flex()
                                .justify_end()
                                .text_color(theme.text_faint)
                                .child(old_number),
                        )
                        .child(
                            div()
                                .w(px(DIFF_GUTTER_WIDTH))
                                .pr(px(8.0))
                                .flex()
                                .justify_end()
                                .text_color(theme.text_faint)
                                .child(new_number),
                        ),
                )
                .child(div().flex_none().w(px(12.0)).child(prefix))
                .child(
                    div()
                        .debug_selector(move || text_selector.clone())
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .child(body),
                );
            if let Some(background) = background {
                row = row.bg(background);
            }
            scroll = scroll.child(row);
        }
        if total > DIFF_PREVIEW_MAX_LINES {
            scroll = scroll.child(
                div()
                    .min_w_full()
                    .text_size(typography.caption2)
                    .text_color(theme.text_faint)
                    .px(px(10.0))
                    .child(format!("… {} more lines", total - DIFF_PREVIEW_MAX_LINES)),
            );
        }
        div()
            .w_full()
            // Never the standalone gallery's 760 -- the diff takes the
            // transcript's own width, per `diff_column_width`.
            .max_w(px(diff_column_width(TRANSCRIPT_WIDTH)))
            .flex()
            .flex_col()
            .rounded(theme.radii.code_block)
            .bg(theme.input_bg)
            .py(px(6.0))
            .child(
                div()
                    .id(SharedString::from(header_id))
                    .debug_selector(move || header_selector.clone())
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .text_size(typography.footnote)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.file_link)
                    .cursor(CursorStyle::PointingHand)
                    .hover(|style| style.text_color(theme.text))
                    .px(px(10.0))
                    .pb(px(4.0))
                    .on_click(move |_, _, cx| {
                        open_entity.update(cx, |_, cx| {
                            cx.emit(ChatEvent::OpenFile(open_path.clone()));
                        });
                    })
                    .child(
                        IconElement::new(Icon::File, IconSize::Small).text_color(theme.file_link),
                    )
                    .child(diff.path.display().to_string())
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex()
                            .gap(px(6.0))
                            .child(div().text_color(theme.diff_add).child(format!("+{added}")))
                            .child(
                                div()
                                    .text_color(theme.diff_del)
                                    .child(format!("-{removed}")),
                            ),
                    ),
            )
            .child(scroll)
            .into_any_element()
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
        let typography = theme.typography;
        let mut card = div()
            .id(("edit-summary", entry))
            .debug_selector(move || format!("edit-summary-{entry}"))
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .rounded(theme.radii.code_block)
            .bg(theme.surface_raised)
            .px(px(CARD_H_PADDING))
            .py(px(CARD_V_PADDING))
            .child(
                div()
                    .text_size(typography.footnote)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_muted)
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
                    .text_color(theme.file_link)
                    .cursor(CursorStyle::PointingHand)
                    .hover(|style| style.text_color(theme.text))
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
                        .text_color(theme.text_faint)
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
                            .text_color(theme.danger)
                            .hover(|style| style.bg(theme.overlay))
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
                            .text_color(theme.text_faint)
                            .hover(|style| style.bg(theme.overlay))
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
                            theme.text_faint
                        } else {
                            theme.danger
                        })
                        .hover(|style| style.bg(theme.overlay))
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
                    .text_color(theme.danger)
                    .child(error),
            );
        }
        card.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_entry(
        entry: Entry,
        entry_index: usize,
        theme: &Theme,
        entity: gpui::Entity<Self>,
        transcript_focus: FocusHandle,
        source_start: usize,
        question_answer: &QuestionAnswerState,
        answer_caret_visible: bool,
        copied_target: Option<CopyTarget>,
        edit_summary: Option<EditSummaryState>,
        thought_streaming: bool,
        thought_scroll: &HashMap<usize, thought::ThoughtScroll>,
        day_heading: Option<&str>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let typography = theme.typography;
        let bezel_theme = theme.to_bezel_theme();
        let interaction = TranscriptInteraction {
            chat: entity.clone(),
            focus: transcript_focus,
        };
        match entry {
            Entry::User { text, .. } => {
                let bubble = div()
                    .w_full()
                    .flex()
                    .justify_end()
                    // #173: without this the row's flex child keeps its content
                    // width as a floor, so a message wider than the pane refuses
                    // to shrink and — being end-justified — spills off the *left*
                    // edge, where nothing can scroll to it. `max_w` never binds in
                    // that case, because the pane is already narrower than the cap.
                    .min_w_0()
                    .child(
                        div()
                            .debug_selector(move || format!("user-bubble-{entry_index}"))
                            .min_w_0()
                            .max_w(px(USER_PILL_MAX_WIDTH))
                            .rounded(theme.radii.user_pill)
                            .bg(theme.surface_raised)
                            .px(px(USER_PILL_H_PADDING))
                            .py(px(USER_PILL_V_PADDING))
                            .text_size(typography.scaled(USER_PILL_TEXT_SIZE))
                            .text_color(theme.text)
                            .child(Self::render_plain_text(
                                text,
                                theme,
                                format!("user-entry-{entry_index}"),
                                source_start,
                                Some(&interaction),
                            )),
                    )
                    .into_any_element();
                // The heading row belongs to the `User` entry's row, so no
                // extra list index is needed: it sits above the question.
                match day_heading {
                    Some(label) => div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .child(Chat::render_day_heading(
                            entry_index,
                            label,
                            theme,
                            &bezel_theme,
                        ))
                        .child(bubble)
                        .into_any_element(),
                    None => bubble,
                }
            }
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
                    .bg(theme.surface_raised)
                    .text_size(typography.footnote)
                    .text_color(theme.text_faint)
                    .cursor(CursorStyle::PointingHand)
                    .hover(|style| style.bg(theme.overlay))
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
                // Every prose entry is an answer, whether it closes the turn
                // or sits between tool calls: same markdown, same colour.
                div()
                    .id(("assistant-response", entry_index))
                    .debug_selector(move || format!("assistant-response-{entry_index}"))
                    .relative()
                    .group(hover_group)
                    .w_full()
                    .child(MarkdownBody::new(document))
                    .child(copy)
                    .child(
                        div()
                            .size_0()
                            .debug_selector(move || format!("answer-{entry_index}")),
                    )
                    .into_any_element()
            }
            Entry::Thought {
                text,
                open,
                duration_ms,
                ..
            } => {
                let streaming = thought_streaming;
                let is_open = open.get(streaming);
                let mut column = div().w_full().flex().flex_col().gap(px(4.0)).child(
                    Self::render_thought_header(
                        entry_index,
                        streaming,
                        is_open,
                        duration_ms,
                        theme,
                        &bezel_theme,
                        window,
                        cx,
                        Some(entity.clone()),
                    ),
                );
                if is_open && let Some(scroll) = thought_scroll.get(&entry_index) {
                    column = column.child(Self::render_thought_body(
                        entry_index,
                        &text,
                        source_start,
                        &interaction,
                        scroll,
                        theme,
                        &bezel_theme,
                    ));
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
                duration_ms,
                ..
            } => Self::render_tool_row(
                entry_index,
                None,
                true,
                &title,
                &status,
                &kind,
                duration_ms,
                content,
                locations,
                expanded,
                edit_summary,
                source_start,
                Some(interaction.clone()),
                theme,
                &bezel_theme,
                entity.clone(),
            ),
            Entry::SubagentTask {
                title,
                status,
                tool_calls,
                expanded,
                ..
            } => Self::render_subagent_task(
                entry_index,
                title,
                status,
                tool_calls,
                expanded,
                theme,
                &bezel_theme,
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
                    .bg(theme.surface_raised)
                    .border_l_2()
                    .border_color(theme.warning)
                    .px(px(CARD_H_PADDING))
                    .py(px(CARD_V_PADDING))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(typography.callout)
                            .text_color(theme.text)
                            .child(header),
                    );
                if !prompt.is_empty() {
                    card = card.child(
                        div()
                            .text_size(typography.footnote)
                            .text_color(theme.text_muted)
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
                            .text_color(theme.text_faint)
                            .child(format!("Answered: {choice}")),
                    );
                } else if dismissed {
                    card = card.child(
                        div()
                            .text_size(typography.footnote)
                            .text_color(theme.text_faint)
                            .child("Dismissed — request cancelled"),
                    );
                } else if expired {
                    // F-CHAT-27: the turn ended unanswered; offering the
                    // buttons again would be a lie.
                    card = card.child(
                        div()
                            .text_size(typography.footnote)
                            .text_color(theme.text_faint)
                            .child("No answer — the turn ended"),
                    );
                } else if let Some(input) = text_input {
                    card = card.child(Self::render_question_answer_row(
                        request_id,
                        &input,
                        theme,
                        entity.clone(),
                        question_answer,
                        answer_caret_visible,
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
                                .bg(theme.surface_raised)
                                .text_size(typography.footnote)
                                .text_color(if option.is_rejection {
                                    theme.danger
                                } else {
                                    theme.text
                                })
                                .hover(|style| style.bg(theme.overlay))
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
                            .bg(theme.surface_raised)
                            .text_size(typography.footnote)
                            .text_color(theme.text)
                            .hover(|style| style.bg(theme.overlay))
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
                    .bg(theme.surface_raised)
                    .border_l_2()
                    .border_color(theme.border_strong)
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
                            .child(div().text_color(theme.text).child("Plan"))
                            .child(
                                div()
                                    .text_color(theme.text_faint)
                                    .child(format!("{completed}/{}", entries.len())),
                            ),
                    );
                for row in entries {
                    let (glyph, tint) = match row.status.as_str() {
                        "completed" => ("✓", theme.border_strong),
                        "in_progress" => ("◌", theme.text),
                        _ => ("○", theme.text_faint),
                    };
                    card = card.child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(6.0))
                            .text_size(typography.callout)
                            .child(div().w(px(14.0)).text_color(tint).child(glyph))
                            .child(div().flex_1().text_color(theme.text).child(row.content)),
                    );
                }
                if let Some(approval) = approval {
                    if let Some(choice) = &approval.resolved {
                        card = card.child(
                            div()
                                .text_size(typography.footnote)
                                .text_color(theme.text_faint)
                                .child(format!("Approved: {choice}")),
                        );
                    } else if approval.expired {
                        card = card.child(
                            div()
                                .text_size(typography.footnote)
                                .text_color(theme.text_faint)
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
                                    .bg(theme.surface_raised)
                                    .text_size(typography.footnote)
                                    .text_color(if option.is_rejection {
                                        theme.danger
                                    } else {
                                        theme.text
                                    })
                                    .hover(|style| style.bg(theme.overlay))
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
                .child(div().h(px(1.0)).flex_1().bg(theme.border))
                .child(
                    div()
                        .text_size(typography.footnote)
                        .text_color(theme.text_faint)
                        .child(at.clone()),
                )
                .child(div().h(px(1.0)).flex_1().bg(theme.border))
                .into_any_element(),
            Entry::Error {
                message,
                retryable,
                kind,
            } => {
                let retry_entity = entity.clone();
                let dismiss_entity = entity.clone();
                let is_mcp_warning = kind == ErrorKind::McpWarning;
                // F-CHAT-02: AuthRequired gets its own amber treatment
                // (matching the connecting/working status-dot color already
                // used elsewhere in this file) instead of the generic red
                // connection-failure card — the fix here is "sign in, then
                // retry", not "the network hiccupped, retry", and the card
                // should look like a different kind of problem.
                let is_auth_required = kind == ErrorKind::AuthRequired;
                // F-CHAT-03: the agent's own process is gone — there is no
                // live request left to retry, only a fresh process to
                // start, so this offers "Restart agent" instead of "Retry"
                // (Swift's `ChatState.disconnected` banner names the same
                // distinction; `ChatPaneView.swift:82`).
                let is_disconnected = kind == ErrorKind::Disconnected;
                // Nothing broke, so this must not look like breakage: the
                // agent simply is not available here. It borrows the amber
                // treatment AuthRequired uses for the same reason -- both
                // say "there is an action for you", not "something failed".
                let is_unavailable = kind == ErrorKind::Unavailable;
                let settings_entity = entity.clone();
                let (banner_bg, banner_border, banner_text) = if is_auth_required || is_unavailable
                {
                    (rgb(0xf5a623).opacity(0.12), rgb(0xf5a623), theme.text)
                } else {
                    (theme.diff_del_bg, theme.diff_del, theme.diff_del)
                };
                div()
                    .id(("chat-error-banner", entry_index))
                    .when(is_auth_required, |this| {
                        this.debug_selector(|| "chat-auth-required-banner".into())
                    })
                    .when(is_disconnected, |this| {
                        this.debug_selector(|| "chat-disconnected-banner".into())
                    })
                    .when(is_mcp_warning, |this| {
                        this.debug_selector(|| "chat-mcp-warning-banner".into())
                    })
                    .when(is_unavailable, |this| {
                        this.debug_selector(|| "chat-unavailable-banner".into())
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
                    // F-CHAT-02: a flex child defaults to a min-width of its
                    // own content (same rule as CSS flexbox), so a long
                    // guidance message never shrank below its own text
                    // width — it overflowed the row and pushed the Retry
                    // sibling out past the visible edge instead of wrapping.
                    // `min_w_0()` is the standard fix (zed's own
                    // `ui::components::banner` uses the identical
                    // `.min_w_0().flex_1()` pairing for the same reason).
                    .child(div().flex_1().min_w_0().child(message))
                    .when(retryable, |this| {
                        this.child(
                            div()
                                .id(("retry", entry_index))
                                .debug_selector(move || {
                                    if is_disconnected {
                                        "chat-restart-agent".into()
                                    } else {
                                        "chat-retry".into()
                                    }
                                })
                                // Never shrink: the message above now wraps
                                // and gives up width instead of pushing this
                                // sibling out of the row (F-CHAT-02).
                                .flex_shrink_0()
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded(theme.radii.control)
                                .text_color(theme.text)
                                .bg(theme.surface_raised)
                                .hover(|style| style.bg(theme.overlay))
                                .on_click(move |_, _, cx| {
                                    // Same underlying call as Retry
                                    // (`Chat::retry` -> `start_connection`)
                                    // for the same reason Swift's Restart
                                    // agent button calls the identical
                                    // `ChatController.start()` its Retry
                                    // button does (F-CHAT-03): it always
                                    // spawns a fresh agent process either
                                    // way, so "restart" and "retry" name the
                                    // same act from two different starting
                                    // states rather than two mechanisms.
                                    retry_entity.update(cx, |chat, cx| chat.retry(cx));
                                })
                                .child(if is_disconnected {
                                    "Restart agent"
                                } else {
                                    "Retry"
                                }),
                        )
                    })
                    // The one action that can change the outcome. The chat
                    // does not open Settings itself: the workspace owns that
                    // surface and subscribes to the event, the same way it
                    // already handles OpenFile and OpenLink.
                    .when(is_unavailable, |this| {
                        this.child(
                            div()
                                .id(("open-settings", entry_index))
                                .debug_selector(|| "chat-open-settings".into())
                                .flex_shrink_0()
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded(theme.radii.control)
                                .text_color(theme.text)
                                .bg(theme.surface_raised)
                                .hover(|style| style.bg(theme.overlay))
                                .on_click(move |_, _, cx| {
                                    settings_entity
                                        .update(cx, |_, cx| cx.emit(ChatEvent::OpenSettings));
                                })
                                .child("Open Settings"),
                        )
                    })
                    // F-CHAT-33: "OK to dismiss" -- present for every error,
                    // retryable or not (Swift's `promptError`/`mcpWarning`
                    // banners both carry exactly this one action). It never
                    // retries or restarts anything, only removes this one
                    // row, so it stays available even when Retry/Restart is
                    // also shown above: dismissing without retrying is a
                    // real, distinct choice.
                    //
                    // Withheld for Unavailable alone: there the box is the
                    // tab's entire content, so dismissing would leave a chat
                    // that neither explains itself nor does anything.
                    .when(!is_unavailable, |this| {
                        this.child(
                            div()
                                .id(("dismiss-error", entry_index))
                                .debug_selector(|| "chat-error-ok".into())
                                .flex_shrink_0()
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded(theme.radii.control)
                                .text_color(theme.text)
                                .bg(theme.surface_raised)
                                .hover(|style| style.bg(theme.overlay))
                                .on_click(move |_, _, cx| {
                                    dismiss_entity.update(cx, |chat, cx| {
                                        chat.dismiss_error(entry_index, cx);
                                    });
                                })
                                .child("OK"),
                        )
                    })
                    .into_any_element()
            }
        }
    }

    /// F-CHAT-22, turn half: the single row an older turn collapses to.
    ///
    /// Swift's `TurnFoldRow` — a chevron, `Turn: <label>`, the turn's clock
    /// time pushed to the right, on a quiet rounded plate — and, as there,
    /// the whole row is the control: clicking anywhere on it re-opens the
    /// turn in place.
    fn render_turn_fold_row(
        turn_id: usize,
        label: String,
        at: String,
        theme: &Theme,
        entity: gpui::Entity<Self>,
    ) -> AnyElement {
        let typography = theme.typography;
        div()
            .id(("turn-fold", turn_id))
            .debug_selector(move || format!("turn-fold-{turn_id}"))
            .w_full()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(8.0))
            .py(px(6.0))
            .rounded(theme.radii.code_block)
            .bg(theme.surface_raised)
            .cursor(CursorStyle::PointingHand)
            .hover(|style| style.bg(theme.overlay))
            .on_click(move |_, _, cx| {
                entity.update(cx, |chat, cx| {
                    chat.toggle_turn_unfolded(turn_id, cx);
                });
            })
            .child(
                IconElement::new(Icon::ChevronRight, IconSize::XSmall).text_color(theme.text_faint),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(typography.footnote)
                    .text_color(theme.text_muted)
                    .child(format!("Turn: {label}")),
            )
            .child(
                div()
                    .text_size(typography.caption2)
                    .text_color(theme.text_faint)
                    .child(at),
            )
            .into_any_element()
    }

    /// Blink timer tick for the question answer field's caret.
    fn flip_answer_blink(&mut self, cx: &mut Context<Self>) {
        self.answer_blink.flip();
        cx.notify();
    }

    /// D-CHAT-03: the queue block above the composer card — a header that
    /// counts the entries and folds them, then one row per entry with its
    /// text, "Send now" and ✕. The front entry is the one the running
    /// turn's end sends; its dot is the only bright one. The list caps its
    /// height and scrolls, so a long queue never pushes the card off the
    /// pane. Callers draw it only while the queue is non-empty.
    fn render_queue(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let typography = theme.typography;
        let entity = cx.entity();
        let count = self.queue.len();
        let expanded = self.queue_expanded;
        let title = if count == 1 {
            "1 message queued".to_string()
        } else {
            format!("{count} messages queued")
        };
        let toggle_entity = entity.clone();
        let clear_entity = entity.clone();
        div()
            .id("queue")
            .debug_selector(|| "queue".into())
            .w_full()
            .max_w(px(TRANSCRIPT_WIDTH))
            .mb(px(8.0))
            .rounded(px(bezel::theme::Theme::surface_radius()))
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface_raised)
            .overflow_hidden()
            .flex()
            .flex_col()
            .text_size(typography.footnote)
            .child(
                div()
                    .id("queue-header")
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .pl(px(8.0))
                    .pr(px(4.0))
                    .py(px(4.0))
                    .child(
                        div()
                            .id("queue-toggle")
                            .debug_selector(|| "queue-toggle".into())
                            .flex()
                            .flex_1()
                            .items_center()
                            .gap(px(6.0))
                            .py(px(2.0))
                            .cursor(CursorStyle::PointingHand)
                            .on_click(move |_, _, cx| {
                                toggle_entity.update(cx, |chat, cx| chat.toggle_queue_folded(cx));
                            })
                            .child(
                                IconElement::new(
                                    if expanded {
                                        Icon::ChevronDown
                                    } else {
                                        Icon::ChevronRight
                                    },
                                    IconSize::XSmall,
                                )
                                .text_color(theme.text_faint),
                            )
                            .child(
                                div()
                                    .id("queue-count")
                                    .debug_selector(move || format!("queue-count-{count}"))
                                    .text_color(theme.text_muted)
                                    .child(title),
                            ),
                    )
                    .child(
                        div()
                            .id("queue-clear")
                            .debug_selector(|| "queue-clear".into())
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(theme.radii.control)
                            .text_size(typography.caption2)
                            .text_color(theme.text_faint)
                            .cursor(CursorStyle::PointingHand)
                            .hover(|style| style.bg(theme.overlay))
                            .on_click(move |_, _, cx| {
                                clear_entity.update(cx, |chat, cx| chat.clear_queue(cx));
                            })
                            .child("Clear all"),
                    ),
            )
            .when(expanded, |block| {
                block.child(
                    div()
                        .id("queue-entries")
                        .flex()
                        .flex_col()
                        .max_h(px(QUEUE_MAX_HEIGHT))
                        .overflow_y_scroll()
                        .border_t_1()
                        .border_color(theme.border)
                        .children(self.queue.iter().enumerate().map(|(index, text)| {
                            let send_entity = entity.clone();
                            let remove_entity = entity.clone();
                            let text_for_id = text.clone();
                            let is_next = index == 0;
                            div()
                                .id(("queue-entry", index))
                                .debug_selector(move || format!("queue-entry-{index}"))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .px(px(10.0))
                                .py(px(4.0))
                                .when(index + 1 < count, |row| {
                                    row.border_b_1().border_color(theme.border)
                                })
                                .child(bezel::ui::widgets::status_dot(if is_next {
                                    theme.text.into()
                                } else {
                                    theme.text_faint.into()
                                }))
                                .child(
                                    div()
                                        .id(("queue-text", index))
                                        .debug_selector(move || format!("queue-text-{text_for_id}"))
                                        .flex_1()
                                        .min_w_0()
                                        .text_ellipsis()
                                        .text_color(theme.text)
                                        .child(text.clone()),
                                )
                                .child(
                                    div()
                                        .id(("queue-send", index))
                                        .debug_selector(move || format!("queue-send-{index}"))
                                        .flex_none()
                                        .px(px(8.0))
                                        .py(px(3.0))
                                        .rounded(theme.radii.control)
                                        .text_size(typography.caption2)
                                        .text_color(theme.text_muted)
                                        .cursor(CursorStyle::PointingHand)
                                        .hover(|style| style.bg(theme.overlay))
                                        .on_click(move |_, _, cx| {
                                            send_entity.update(cx, |chat, cx| {
                                                chat.send_queued_entry_now(index, cx);
                                            });
                                        })
                                        .child("Send now"),
                                )
                                .child(
                                    div()
                                        .id(("queue-remove", index))
                                        .debug_selector(move || format!("queue-remove-{index}"))
                                        .flex_none()
                                        .px(px(4.0))
                                        .rounded(px(3.0))
                                        .text_color(theme.text_faint)
                                        .cursor(CursorStyle::PointingHand)
                                        .hover(|style| style.bg(theme.overlay))
                                        .on_click(move |_, _, cx| {
                                            remove_entity.update(cx, |chat, cx| {
                                                chat.remove_queued_entry(index, cx);
                                            });
                                        })
                                        .child("×"),
                                )
                        })),
                )
            })
            .into_any_element()
    }

    fn render_composer(
        &mut self,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let typography = theme.typography;
        let focused = self
            .composer_field
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let bezel_theme = bezel::theme::Theme::of(cx).clone();
        let placeholder = self.composer_placeholder();
        if placeholder != self.composer_placeholder_shown {
            self.composer_placeholder_shown = placeholder.clone();
            self.composer_field
                .update(cx, |field, cx| field.set_placeholder(placeholder, cx));
        }
        let disabled = self.composer_disabled();
        let can_send = self.can_send();
        let entity = cx.entity();

        // Swift's `modePill` (ComposerControlBar.swift) pairs a status dot
        // with the permission mode's name: the dot carries the connection
        // state, the label the mode, and a raw state word ("idle"/"working")
        // only stands in while no mode is known. The port used to let
        // "working" and "connecting" take the label over from the mode, so
        // the permission the user picked was unreadable exactly while a
        // turn ran (`status_pill_content`, `composer_view.rs`).
        let connecting = self.connecting;
        // F-CHAT-15 / #136: a known mode names itself from the first frame,
        // never waiting on `has_completed_turn`.
        let (dot, label) = composer_view::status_pill_content(
            connecting,
            self.streaming,
            self.client.is_some(),
            self.mode_catalog.as_ref(),
        );
        let dot = match dot {
            composer_view::PillDot::Busy => rgb(0xf5a623),
            composer_view::PillDot::Ready => rgb(0x53c653),
            composer_view::PillDot::Offline => rgb(0x8a8d99),
        };
        let mode_selectable = self.mode_selectable();
        let status_pill = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.0))
            .h(px(24.0))
            .px(px(7.0))
            .rounded(theme.radii.control)
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
                this.hover(|style| style.bg(bezel_theme.element_hover))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_mode_picker(window, cx);
                    }))
            })
            .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(dot))
            .child(div().text_color(theme.text).child(label))
            .when(mode_selectable, |this| {
                this.child(picker_chevron(theme))
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
                // Not uppercased any more: it used to be a bare caption that
                // needed to read as chrome, and now it sits beside its own
                // "Effort" label exactly the way the model value sits beside
                // "Model".
                name
            })
        });
        // The picker chip — "Model" in muted chrome text, the value in
        // title text, the effort level when one is selected — opens only
        // once a turn has completed and the agent reported models. Without
        // models the pill degrades to a plain agent badge (F-CHAT-36): no
        // label, no chevron, no picker.
        let model_control = if self.model_control_visible() {
            let model_selection_id = self
                .selected_model
                .as_deref()
                .map(|id| format!("model-selection-{id}"))
                .unwrap_or_else(|| "model-selection-none".into());
            div()
                .id("model-chip")
                .debug_selector(|| "model-chip".into())
                .flex()
                .items_center()
                .gap(px(6.0))
                .h(px(24.0))
                .px(px(7.0))
                .rounded(theme.radii.control)
                .text_size(typography.ui_size)
                // Sized to its content, not to the row. It used to carry
                // `flex_1`, which stretched the pill the whole width of the
                // control row and stranded its own chevron ~200px from the
                // model name, next to the overflow button -- so the chevron
                // read as belonging to nothing and the model value read as a
                // caption rather than a picker. It still shrinks on a tight
                // row -- that is what keeps the name's ellipsis working --
                // but a 56px floor stops it collapsing to nothing: the name
                // ellipsizes while the row wraps around it, never erasing
                // the picker.
                .min_w(px(56.0))
                .hover(|style| style.bg(bezel_theme.element_hover))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.toggle_model_picker(window, cx);
                }))
                .child(div().text_color(theme.text_faint).child("Model"))
                .child(
                    div()
                        .id(model_selection_id.clone())
                        .debug_selector(move || model_selection_id)
                        // No `flex_1`. Inert on its own now that the pill
                        // hugs its content -- there is no slack left inside
                        // to absorb, and removing it alone does not move the
                        // chevron, which the test below confirms. It goes
                        // because the two together are what stranded the
                        // chevron: restore `flex_1` on the pill and this
                        // would push it to the far edge again.
                        // `min_w_0` + `text_ellipsis` do the real work,
                        // truncating a long name when the row is tight.
                        .min_w_0()
                        .text_ellipsis()
                        .text_color(theme.text)
                        .child(selected_model_name.clone()),
                )
                .child(
                    div()
                        .id("model-chip-chevron")
                        .debug_selector(|| "model-chip-chevron".into())
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .child(picker_chevron(theme)),
                )
        } else {
            // #206: this badge names the *agent*, so it reads the agent.
            // It used to render `selected_model_name`, a model variable
            // whose fallback is the literal "Claude Code" -- so it named
            // the wrong agent for every other one until models arrived,
            // and named an agent at all for a chat whose banner two
            // inches above says the agent is unknowable. `agent_name` is
            // `None` in exactly that case, which is the case the banner
            // is about.
            let agent_badge_name = self.agent_badge_name();
            div()
                .id("agent-badge")
                .debug_selector(|| "agent-badge".into())
                .flex()
                .items_center()
                .gap(px(6.0))
                .h(px(24.0))
                .px(px(7.0))
                .rounded(theme.radii.control)
                .text_size(typography.ui_size)
                // Same rule as the chip above: hug the content.
                .min_w_0()
                .child(
                    div()
                        .min_w_0()
                        .text_ellipsis()
                        .text_color(theme.text)
                        .child(agent_badge_name),
                )
        };

        // The effort level is a peer of the model, not a caption inside it:
        // it is changed about as often, so it belongs at the same depth and
        // carries its own label. Drawn only when the agent reports a value
        // AND the picker can actually open, so this is never a click target
        // that leads nowhere. `flex_none` keeps it intact while the model
        // chip beside it absorbs the squeeze on a narrow pane — past that
        // the chip wraps whole to the next line, never over the send disc.
        let effort_control = effort_label
            .filter(|_| self.model_control_visible())
            .map(|label| {
                let effort_entity = entity.clone();
                div()
                    .id("effort-chip")
                    .debug_selector(|| "effort-chip".into())
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(6.0))
                    .h(px(24.0))
                    .px(px(7.0))
                    .rounded(theme.radii.control)
                    .text_size(typography.ui_size)
                    .hover(|style| style.bg(bezel_theme.element_hover))
                    .on_click(move |_, window, cx| {
                        effort_entity.update(cx, |chat, cx| chat.toggle_model_picker(window, cx));
                    })
                    .child(div().text_color(theme.text_faint).child("Effort"))
                    .child(
                        div()
                            .id("model-effort-label")
                            .debug_selector(|| "model-effort-label".into())
                            .text_color(theme.text)
                            .child(label),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .child(picker_chevron(theme)),
                    )
            });

        let view = bezel::motion::Painter::of(cx);
        let model_picker = self.model_picker_open.then(|| {
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
            let query = self.model_search_field.read(cx).content().to_string();
            let filtered_models: Vec<ModelOption> = self
                .available_models
                .iter()
                .filter(|option| model_query_matches(option, &query))
                .cloned()
                .collect();
            let selected_id = self.selected_model.clone();
            popover::anchored_menu_above(
                "model-picker-menu",
                div()
                    .id("model-picker")
                    .debug_selector(|| "model-picker".into())
                    .key_context("ChatModelPicker")
                    .on_action(cx.listener(Self::cancel))
                    .w(px(245.0))
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.model_picker_open = false;
                        cx.notify();
                    }))
                    .child(
                        popover::popover_card(&bezel_theme).child(
                            div()
                                .flex()
                                .flex_col()
                                .when(!self.available_models.is_empty(), |this| {
                                    this.child(
                                        div()
                                            .id("model-search-input")
                                            .debug_selector(|| "model-search-input".into())
                                            .w_full()
                                            .mb(px(6.0))
                                            .child(self.model_search_field.clone()),
                                    )
                                })
                                .when(self.available_models.is_empty(), |this| {
                                    this.child(
                                        div()
                                            .p(px(8.0))
                                            .text_size(typography.footnote)
                                            .text_color(theme.text_faint)
                                            .child(
                                                "The connected agent did not report any models.",
                                            ),
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
                                                .text_color(theme.text_faint)
                                                .child("No models match"),
                                        )
                                    },
                                )
                                .children(filtered_models.iter().cloned().map(|option| {
                                    let option_id = option.id.clone();
                                    let option_name = option.name.clone();
                                    let option_entity = picker_entity.clone();
                                    let is_recommended =
                                        recommended_id.as_deref() == Some(option_id.as_str());
                                    let is_selected =
                                        selected_id.as_deref() == Some(option_id.as_str());
                                    popover::menu_row_nav(
                                        &bezel_theme,
                                        is_selected,
                                        false,
                                        bezel::motion::Fade::new(
                                            view,
                                            format!("model-option-{option_id}"),
                                        ),
                                    )
                                    .id(format!("model-option-{option_id}"))
                                    .debug_selector(move || format!("model-option-{option_id}"))
                                    .on_click(move |_, _, cx| {
                                        option_entity.update(cx, |chat, cx| {
                                            chat.select_model(option.clone(), cx);
                                        });
                                    })
                                    .child(
                                        div().flex_1().min_w_0().text_ellipsis().child(option_name),
                                    )
                                    .when(
                                        is_recommended,
                                        |this| {
                                            this.child(
                                                div()
                                                    .id("model-option-recommended")
                                                    .debug_selector(|| {
                                                        "model-option-recommended".into()
                                                    })
                                                    .flex_shrink_0()
                                                    .px(px(5.0))
                                                    .rounded(px(4.0))
                                                    .text_size(typography.caption2)
                                                    .text_color(theme.text)
                                                    .bg(theme.overlay_strong)
                                                    .child("Recommended"),
                                            )
                                        },
                                    )
                                }))
                                .when_some(self.effort.clone(), |this, effort| {
                                    if effort.choices.is_empty() {
                                        return this;
                                    }
                                    let effort_entity = picker_entity.clone();
                                    let effort_name =
                                        effort.name.clone().unwrap_or_else(|| "Effort".to_string());
                                    let current = effort.current_value.clone();
                                    let choices: Vec<AnyElement> = effort
                                        .choices
                                        .iter()
                                        .map(|choice| {
                                            let choice_value = choice.value.clone();
                                            let choice_name = choice.name.clone();
                                            let is_selected =
                                                current.as_deref() == Some(choice_value.as_str());
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
                                                .text_color(theme.text)
                                                .when(is_selected, |this| {
                                                    this.bg(theme.element_active)
                                                })
                                                .hover(|style| style.bg(theme.overlay))
                                                .on_click(move |_, _, cx| {
                                                    row_entity.update(cx, |chat, cx| {
                                                        chat.select_effort(
                                                            choice_value.clone(),
                                                            cx,
                                                        );
                                                    });
                                                })
                                                .child(choice_name)
                                                .into_any_element()
                                        })
                                        .collect::<Vec<_>>();
                                    this.child(div().h(px(1.0)).w_full().bg(theme.border))
                                        .child(
                                            div()
                                                .w_full()
                                                .pt(px(4.0))
                                                .text_size(typography.caption2)
                                                .text_color(theme.text_faint)
                                                .child(effort_name),
                                        )
                                        .child(
                                            // #233: the choice count is agent-reported
                                            // and not under the picker's control (Claude
                                            // Code advertises six, whose chips plus gaps
                                            // exceed the popup's usable width), so the
                                            // row wraps onto a second line instead of
                                            // painting chips outside the picker border —
                                            // same shape as the colour swatch row's
                                            // `flex_wrap` in `controls::color_picker`
                                            // (F-PRJ-13).
                                            div().flex().flex_wrap().gap(px(4.0)).children(choices),
                                        )
                                }),
                        ),
                    )
                    .into_any_element(),
                None,
            )
        });

        // F-CHAT-15: the session-mode picker, anchored above the status
        // pill the same way `model_picker` anchors above the model chip.
        // No search field — mode lists are small and entirely agent-defined
        // (ask/plan/auto today), so a flat list of rows is enough.
        let mode_picker = self.mode_picker_open.then(|| {
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
            popover::anchored_menu_above(
                "mode-picker-menu",
                div()
                    .id("mode-picker")
                    .debug_selector(|| "mode-picker".into())
                    .key_context("ChatModelPicker")
                    .track_focus(&self.mode_picker_focus)
                    .on_action(cx.listener(Self::cancel))
                    .w(px(200.0))
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.mode_picker_open = false;
                        cx.notify();
                    }))
                    .child(
                        popover::popover_card(&bezel_theme).child(
                            div()
                                .flex()
                                .flex_col()
                                .when(options.is_empty(), |this| {
                                    this.child(
                                        div()
                                            .p(px(8.0))
                                            .text_size(typography.footnote)
                                            .text_color(theme.text_faint)
                                            .child("No modes offered"),
                                    )
                                })
                                .children(options.into_iter().map(|mode| {
                                    let mode_id = mode.id.clone();
                                    let mode_name = mode.name.clone();
                                    let row_entity = mode_entity.clone();
                                    let is_selected = mode.id == current_id;
                                    popover::menu_row_nav(
                                        &bezel_theme,
                                        is_selected,
                                        false,
                                        bezel::motion::Fade::new(
                                            view,
                                            format!("mode-option-{mode_id}"),
                                        ),
                                    )
                                    .id(format!("mode-option-{mode_id}"))
                                    .debug_selector(move || format!("mode-option-{mode_id}"))
                                    .on_click(move |_, _, cx| {
                                        row_entity.update(cx, |chat, cx| {
                                            chat.select_mode(mode.clone(), cx)
                                        });
                                    })
                                    .child(mode_name)
                                })),
                        ),
                    )
                    .into_any_element(),
                None,
            )
        });

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
            .border_color(theme.border)
            .hover(|style| style.bg(theme.overlay))
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
                    .child({
                        // The paint closure is `'static`, so it cannot borrow
                        // `theme`; copy out the two colours it draws with.
                        let ring_danger = theme.danger;
                        let ring_accent = theme.accent;
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
                                            ring_danger
                                        } else {
                                            ring_accent
                                        },
                                    );
                                }
                            },
                        )
                        .absolute()
                        .size_full()
                    }),
            );

        let context_popover = if self.context_popover_open {
            let usage = context_usage.clone();
            Some(popover::anchored_menu_above_end(
                "context-popover-menu",
                div()
                    .id("context-popover")
                    .debug_selector(|| "context-popover".into())
                    .key_context("ChatContextPopover")
                    .track_focus(&self.context_popover_focus)
                    .on_action(cx.listener(Self::cancel))
                    .w(px(285.0))
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
                                .text_color(theme.text)
                                .child(format!("{percent}% of context used")),
                        )
                        .child(
                            div()
                                .mt(px(4.0))
                                .text_size(typography.caption2)
                                .text_color(theme.text_faint)
                                .child(format!("{} / {} tokens", usage.used, usage.size)),
                        )
                        .when_some(cost, |this, cost| {
                            this.child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(typography.caption2)
                                    .text_color(theme.text_faint)
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
                                        .border_color(theme.border)
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.0))
                                        .when_some(usage.input_tokens, |this, tokens| {
                                            this.child(
                                                div()
                                                    .text_size(typography.caption2)
                                                    .text_color(theme.text_faint)
                                                    .child(format!("Input: {tokens} tokens")),
                                            )
                                        })
                                        .when_some(usage.output_tokens, |this, tokens| {
                                            this.child(
                                                div()
                                                    .text_size(typography.caption2)
                                                    .text_color(theme.text_faint)
                                                    .child(format!("Output: {tokens} tokens")),
                                            )
                                        })
                                        .when_some(usage.cached_read_tokens, |this, tokens| {
                                            this.child(
                                                div()
                                                    .text_size(typography.caption2)
                                                    .text_color(theme.text_faint)
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
                                .text_color(theme.text_faint)
                                .child("The agent has not reported context usage yet."),
                        )
                    })
                    .into_any_element(),
                None,
            ))
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
        //
        // Anchored to the composer card's top edge (`bottom: 100%`), not a
        // fixed distance up from its bottom: the card is taller than that
        // distance, so the list used to sit *inside* it — over the input
        // rows, in the card's own `surface_raised` fill, where it read as a
        // transparent veil rather than a menu. The same token paints both
        // on purpose (they are the same step above the page); what makes
        // this a card of its own is that it floats over the page, with the
        // gap below it.
        let slash_popup = if self.slash_popup_visible() {
            let candidates = self.slash_candidates();
            let active = self.slash_filter.active();
            let view = bezel::motion::Painter::of(cx);
            let anchor = self
                .composer_field
                .read(cx)
                .offset_bounds(0, window)
                .map(|row| gpui::point(row.left(), row.top() - px(8.0)));
            anchor.map(|anchor| {
                let rows: Vec<AnyElement> = candidates
                    .into_iter()
                    .enumerate()
                    .map(|(position, command)| {
                        let name = command.name.clone();
                        let tooltip = slash_option_tooltip(&command.description);
                        let row_entity = entity.clone();
                        let accept_name = name.clone();
                        let name_for_id = name.clone();
                        let name_for_label_id = name.clone();
                        // One line per row: the name. The description is
                        // the row's tooltip, so ten rows stay ten lines
                        // and the list does not fill the pane.
                        popover::menu_row(
                            &bezel_theme,
                            Some(position) == active,
                            bezel::motion::Fade::new(view, format!("slash-option-{name}")),
                        )
                        .id(SharedString::from(format!("slash-option-{name}")))
                        .debug_selector(move || format!("slash-option-{name_for_id}"))
                        .when_some(tooltip, |this, text| {
                            this.tooltip(move |window, cx| Tooltip::text(text.clone(), window, cx))
                        })
                        .on_click(move |_, _, cx| {
                            row_entity.update(cx, |chat, cx| {
                                chat.accept_slash_command(&accept_name, cx);
                            });
                        })
                        .child(
                            div()
                                .debug_selector(move || {
                                    format!("slash-option-name-{name_for_label_id}")
                                })
                                .text_size(typography.footnote)
                                .text_color(bezel_theme.text)
                                .child(format!("/{name}")),
                        )
                        .into_any_element()
                    })
                    .collect();
                div()
                    .child(composer_view::menu_above_at(
                        "slash-popup-menu",
                        anchor,
                        popover::popover_card(&bezel_theme)
                            .debug_selector(|| "slash-popup".into())
                            .w(px(280.0))
                            .child(div().flex().flex_col().children(rows))
                            .into_any_element(),
                    ))
                    .into_any_element()
            })
        } else {
            None
        };

        // @ file-mention popup (F-CHAT-10): the bounded filesystem walk's
        // results for the trailing `@token`, anchored above the token. Hidden
        // when the token matches nothing.
        let mention_popup = if mention_token(&self.draft, self.draft_caret).is_some()
            && !self.mention_candidates.is_empty()
        {
            let (at, _) = mention_token(&self.draft, self.draft_caret).expect("token");
            let candidates = self.mention_candidates.clone();
            let active = self.mention_filter.active();
            let view = bezel::motion::Painter::of(cx);
            let anchor = self
                .composer_field
                .read(cx)
                .offset_bounds(at, window)
                .map(|row| gpui::point(row.left(), row.top() - px(8.0)));
            anchor.map(|anchor| {
                let rows: Vec<AnyElement> = candidates
                    .into_iter()
                    .enumerate()
                    .map(|(position, path)| {
                        let row_entity = entity.clone();
                        let path_for_id = path.clone();
                        let path_for_accept = path.clone();
                        popover::menu_row(
                            &bezel_theme,
                            Some(position) == active,
                            bezel::motion::Fade::new(view, format!("mention-option-{path}")),
                        )
                        .id(SharedString::from(format!("mention-option-{path_for_id}")))
                        .debug_selector(move || format!("mention-option-{path_for_id}"))
                        // Pin the row to the card's inner width instead of
                        // trusting cross-axis stretch, so the path below has
                        // a definite box to ellipsize inside.
                        .w_full()
                        .min_w_0()
                        .on_click(move |_, _, cx| {
                            row_entity.update(cx, |chat, cx| {
                                chat.accept_mention(&path_for_accept, cx);
                            });
                        })
                        .child(
                            bezel::ui::icons::icon(bezel::ui::icons::DOCUMENT)
                                .size(px(12.0))
                                .text_color(bezel_theme.text_faint),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(typography.footnote)
                                .text_color(bezel_theme.text)
                                .child(path),
                        )
                        .into_any_element()
                    })
                    .collect();
                div()
                    .child(composer_view::menu_above_at(
                        "mention-popup-menu",
                        anchor,
                        popover::popover_card(&bezel_theme)
                            .id("mention-popup-card")
                            .debug_selector(|| "mention-popup-card".into())
                            .w(px(360.0))
                            .child(div().flex().flex_col().children(rows))
                            .into_any_element(),
                    ))
                    .into_any_element()
            })
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
            Some(popover::anchored_menu_above_end(
                "composer-overflow-menu-menu",
                div()
                    .id("composer-overflow-menu")
                    .debug_selector(|| "composer-overflow-menu".into())
                    .key_context("ChatOverflowMenu")
                    .track_focus(&self.overflow_focus)
                    .on_action(cx.listener(Self::cancel))
                    .w(px(200.0))
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
                            .text_color(theme.text)
                            .hover(|style| style.bg(theme.overlay))
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
                            .text_color(theme.text)
                            .hover(|style| style.bg(theme.overlay))
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
                            .text_color(theme.text)
                            .hover(|style| style.bg(theme.overlay))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_chat_history(window, cx);
                            }))
                            .child("Chat History"),
                    )
                    .into_any_element(),
                None,
            ))
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
                        .text_color(theme.text_muted)
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
                            .hover(|style| style.bg(theme.overlay))
                            .child(
                                div()
                                    .id(SharedString::from(format!("chat-history-open-{tab_id}")))
                                    .flex_1()
                                    .text_size(typography.footnote)
                                    .text_color(theme.text)
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
                                            .text_color(theme.danger)
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
                                            .text_color(theme.text_muted)
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
                                    .text_color(theme.text_muted)
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
            Some(popover::anchored_menu_above_end(
                "chat-history-menu-menu",
                div()
                    .id("chat-history-menu")
                    .debug_selector(|| "chat-history-menu".into())
                    .key_context("ChatHistoryMenu")
                    .track_focus(&self.history_focus)
                    .on_action(cx.listener(Self::cancel))
                    .w(px(260.0))
                    .max_h(px(320.0))
                    .overflow_y_scroll()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.history_open = false;
                        cx.notify();
                    }))
                    .children(rows)
                    .into_any_element(),
                None,
            ))
        } else {
            None
        };

        let attach_button = div()
            .id("attach-image")
            .debug_selector(|| "attach-image".into())
            .w(px(24.0))
            .h(px(24.0))
            .flex_none()
            .rounded(theme.radii.control)
            .flex()
            .items_center()
            .justify_center()
            .hover(|style| style.bg(bezel_theme.element_hover))
            .on_click(move |_, window, cx| {
                attach_entity.update(cx, |chat, cx| chat.attach_image(window, cx));
            })
            .child(
                bezel::ui::icons::icon(bezel::ui::icons::PAPERCLIP)
                    .size(px(14.0))
                    .text_color(bezel_theme.text_faint),
            );

        let overflow_button = div()
            .id("composer-overflow")
            .debug_selector(|| "composer-overflow".into())
            .w(px(24.0))
            .h(px(24.0))
            .flex_none()
            .rounded(theme.radii.control)
            .flex()
            .items_center()
            .justify_center()
            .hover(|style| style.bg(theme.overlay))
            .on_click(move |_, window, cx| {
                overflow_entity.update(cx, |chat, cx| chat.toggle_overflow(window, cx));
            })
            .child(
                div()
                    .text_size(typography.scaled(15.0))
                    .text_color(theme.text_faint)
                    .child("…"),
            );

        let context_percent = context_usage
            .as_ref()
            .filter(|usage| usage.size > 0)
            .map(|usage| ((usage.used as f64 / usage.size as f64) * 100.0).round() as u64)
            .unwrap_or(0);

        // Three looks, one `AnyElement`: the ready arm is `Stateful` (it
        // carries an id), the other two are plain `Div`s.
        let ready = can_send;
        let send_disc = {
            let disc = div()
                .size(px(24.0))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center();
            let disc: AnyElement = if self.streaming {
                // D-CHAT-02: while a turn runs the same control becomes
                // stop -- its click dispatches the same path Escape uses.
                disc.bg(bezel_theme.solid)
                    .cursor_pointer()
                    .child(
                        div()
                            .id("stop-glyph")
                            .debug_selector(|| "stop-glyph".into())
                            .child(
                                bezel::ui::icons::icon(bezel::ui::icons::STOP)
                                    .size(px(12.0))
                                    .text_color(bezel_theme.on_solid),
                            ),
                    )
                    .into_any_element()
            } else if ready {
                disc.id("send-ready")
                    .debug_selector(|| "send-ready".into())
                    .bg(bezel_theme.solid)
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.9))
                    .child(
                        bezel::ui::icons::icon(bezel::ui::icons::ARROW_UP)
                            .size(px(14.0))
                            .text_color(bezel_theme.on_solid),
                    )
                    .into_any_element()
            } else {
                // Present but not pressable: the shape keeps its place, the
                // glyph goes faint, no hover and no pointer (gallery
                // `send_button`).
                disc.bg(bezel::theme::ink(0.06))
                    .child(
                        bezel::ui::icons::icon(bezel::ui::icons::ARROW_UP)
                            .size(px(14.0))
                            .text_color(bezel_theme.text_faint),
                    )
                    .into_any_element()
            };
            div()
                .id("send")
                .debug_selector(|| "send".into())
                .flex_none()
                .cursor_pointer()
                .when(self.streaming, |this| {
                    this.on_click(move |_, _, cx| {
                        stop_entity.update(cx, |chat, cx| chat.cancel_turn(cx));
                    })
                })
                .when(!self.streaming && ready, |this| {
                    this.on_click(move |_, _, cx| {
                        send_entity.update(cx, |chat, cx| chat.send(cx));
                    })
                })
                .child(disc)
        };

        // With no agent configured the chip row holds only the send disc.
        let has_agent = self.agent_command.is_some();

        // The composer is the gallery's `Composer` card: one frosted surface
        // at `surface_radius` carrying the field on top and the chip row
        // ending in the send disc under it.
        let composer_card = div()
            .id("composer")
            .debug_selector(|| "composer".into())
            .relative()
            .w_full()
            .max_w(px(TRANSCRIPT_WIDTH))
            // `Card variant="input"`. #242's rule survives the restyle: the
            // border is always present and only its color reacts to focus,
            // so the card's box never moves while streaming.
            .rounded(px(bezel::theme::Theme::surface_radius()))
            .border_1()
            .border_color(composer_border(focused, &bezel_theme))
            .bg(bezel_theme.card_glass_bg())
            .px(px(4.0))
            .pt(px(4.0))
            .pb(px(6.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.composer_field
                        .read(cx)
                        .focus_handle(cx)
                        .focus(window, cx);
                }),
            )
            .when(!self.attachments.is_empty(), |card| {
                let remove_entity = entity.clone();
                card.child(
                    div()
                        .id("attachment-strip")
                        .debug_selector(|| "attachment-strip".into())
                        .flex()
                        .flex_wrap()
                        .gap(px(6.0))
                        .px(px(4.0))
                        .children(self.attachments.iter().enumerate().map(|(index, _)| {
                            let remove_entity = remove_entity.clone();
                            div()
                                .id(("attachment-chip", index))
                                .debug_selector(move || format!("attachment-chip-{index}"))
                                .h(px(24.0))
                                .px(px(8.0))
                                .rounded(theme.radii.control)
                                .bg(theme.surface_raised)
                                .border_1()
                                .border_color(theme.border)
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(typography.caption2)
                                .child(div().text_color(theme.text_faint).child("▣"))
                                .child(div().text_color(theme.text).child("Image"))
                                .child(
                                    div()
                                        .id(("attachment-remove", index))
                                        .debug_selector(move || {
                                            format!("attachment-remove-{index}")
                                        })
                                        .px(px(2.0))
                                        .rounded(px(2.0))
                                        .text_color(theme.text_faint)
                                        .hover(|style| style.bg(theme.overlay))
                                        .on_click(move |_, window, cx| {
                                            remove_entity.update(cx, |chat, cx| {
                                                chat.remove_attachment(index, cx);
                                                chat.composer_field
                                                    .read(cx)
                                                    .focus_handle(cx)
                                                    .focus(window, cx);
                                            });
                                        })
                                        .child("×"),
                                )
                        })),
                )
            })
            .child(
                div()
                    .id("composer-input")
                    .debug_selector(|| "composer-input".into())
                    .relative()
                    .w_full()
                    .when(disabled, |input| input.opacity(0.6))
                    .child(self.composer_field.clone())
                    .when(focused, |this| {
                        // Covers bezel's own focus ring with the composer's
                        // dark edge: same box, same radius, painted later so
                        // on top. Absolute, so it never moves layout; no
                        // handlers, so clicks still reach the field (and the
                        // card refocuses it anyway).
                        this.child(
                            div()
                                .absolute()
                                .inset_0()
                                .rounded(px(bezel::theme::Theme::button_radius()))
                                .border_1()
                                .border_color(composer_field_edge(&bezel_theme)),
                        )
                    }),
            )
            .when_some(self.attach_error.clone(), |this, message| {
                this.child(
                    div()
                        .id("attach-error")
                        .debug_selector(|| "attach-error".into())
                        .px(px(4.0))
                        .text_size(typography.caption2)
                        .text_color(theme.danger)
                        .child(message),
                )
            })
            .child(
                // The chip row: Sirio's controls on the card's own surface,
                // ending in the send/stop disc. One flat wrapping row —
                // when a line is full the next chip wraps to the next line
                // and the trio follows to the end of whichever line it
                // lands on; nothing is ever clipped, and the disc never
                // paints over a neighbour. With no agent configured the row
                // holds only the send disc.
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.0))
                    .gap_y(px(4.0))
                    .px(px(6.0))
                    .when(has_agent, |row| {
                        row.child(
                            div()
                                .relative()
                                .flex_none()
                                .child(status_pill)
                                .children(mode_picker),
                        )
                        // The one shrinkable child: on a tight line the chip
                        // ellipsizes its name down to its 56px floor, never to
                        // nothing.
                        .child(div().relative().child(model_control).children(model_picker))
                        .children(effort_control)
                        .child(
                            div()
                                .relative()
                                .flex()
                                .flex_none()
                                .items_center()
                                .gap(px(6.0))
                                .h(px(24.0))
                                .px(px(7.0))
                                .rounded(theme.radii.control)
                                .text_size(typography.ui_size)
                                .child(context_ring)
                                .child(
                                    div()
                                        .id("context-label")
                                        .debug_selector(|| "context-label".into())
                                        .text_color(theme.text_faint)
                                        .child("Context"),
                                )
                                .child(
                                    div()
                                        .id("context-percent")
                                        .debug_selector(|| "context-percent".into())
                                        .text_color(theme.text)
                                        .child(format!("{context_percent}%")),
                                )
                                .children(context_popover),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(px(4.0))
                            .ml_auto()
                            .when(has_agent, |trio| {
                                trio.child(attach_button).child(
                                    div()
                                        .relative()
                                        .child(overflow_button)
                                        .children(overflow_menu)
                                        .children(chat_history_menu),
                                )
                            })
                            .child(send_disc),
                    ),
            )
            .children(slash_popup)
            .children(mention_popup);

        composer_card.into_any_element()
    }
}

fn control_entry_row(entry: &Entry) -> BTreeMap<String, String> {
    let mut row = BTreeMap::new();
    match entry {
        Entry::User { text, .. } => {
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
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.composer_field.read(cx).focus_handle(cx)
    }
}

/// The tooltip a command row carries: its description, trimmed, or nothing
/// when the agent published none — an empty tooltip is a blank card that
/// pops up for no reason.
fn slash_option_tooltip(description: &str) -> Option<SharedString> {
    let description = description.trim();
    (!description.is_empty()).then(|| SharedString::from(description.to_owned()))
}

impl Render for Chat {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Fetched fresh every frame from the global, so a change of
        // appearance is picked up without the chat surface holding a stale
        // copy — never `Theme::dark()`, never a field.
        let theme = *Theme::get(cx);
        let transcript_theme = theme;
        let bezel_theme = bezel::theme::Theme::of(cx).clone();
        // The row processor outlives this frame, so it owns a clone; the
        // transient spinner below borrows the original.
        let row_bezel_theme = bezel_theme.clone();
        let entity = cx.entity();
        let entity_for_bar = entity.clone();
        let transcript_ranges = self.transcript_entry_ranges();
        // F-CHAT-22, turn half: resolved once per frame, not once per drawn
        // row — segmenting the transcript is O(entries), and the virtualizer
        // calls its row processor separately for every visible index.
        let turn_roles = turn_row_roles(&self.entries, &self.unfolded_turns);
        let transcript_focus = self.transcript_focus.clone();
        // The answer field's caret, resolved before the tree is built so the
        // card and the composer agree within one frame.
        let answer_focused = self.question_answer.focus.is_focused(window);
        caret::schedule(
            &mut self.answer_blink,
            answer_focused,
            Self::flip_answer_blink,
            cx,
        );
        self.answer_caret_visible = answer_focused && self.answer_blink.visible();
        let answer_caret_visible = self.answer_caret_visible;
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
            .bg(theme.surface)
            .on_action(cx.listener(Self::send_action))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::copy_transcript))
            .on_action(cx.listener(Self::popup_previous))
            .on_action(cx.listener(Self::popup_next))
            .on_action(cx.listener(Self::popup_accept))
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
                    .w_full()
                    .max_w(px(TRANSCRIPT_WIDTH))
                    .px(px(24.0))
                    .py(px(28.0))
                    .flex_1()
                    .flex()
                    // The jump-to-latest disc is laid inside, over the list.
                    .relative()
                    .key_context("ChatTranscript")
                    .track_focus(&self.transcript_focus)
                    .on_action(cx.listener(Self::copy_transcript))
                    .on_key_down(cx.listener(Self::on_transcript_key))
                    .child(
                        list(
                            self.list_state.clone(),
                            cx.processor(move |this, entry_index: usize, window, cx| {
                                // The body's follow pin + scrollbar need per-entry state;
                                // created on first draw so a restored chat pays nothing
                                // until a thought is opened.
                                if let Some(Entry::Thought { open, .. }) =
                                    this.entries.get(entry_index)
                                {
                                    let streaming = this.thought_is_streaming(entry_index);
                                    if open.get(streaming) {
                                        let painter = bezel::motion::Painter::of(cx);
                                        this.thought_scroll.entry(entry_index).or_insert_with(
                                            || thought::ThoughtScroll::new(painter),
                                        );
                                    }
                                }
                                // F-CHAT-22, turn half: an older turn stands
                                // in for itself with one row. Resolved before
                                // the tool-call grouping below, because a
                                // folded turn hides its tool calls too.
                                match turn_roles
                                    .get(entry_index)
                                    .cloned()
                                    .unwrap_or(TurnRowRole::Normal)
                                {
                                    TurnRowRole::Hidden => {
                                        return div()
                                            .id(("chat-entry", entry_index))
                                            .into_any_element();
                                    }
                                    TurnRowRole::Fold { turn_id, label, at } => {
                                        return div()
                                            .id(("chat-entry", entry_index))
                                            .w_full()
                                            .max_w(px(TRANSCRIPT_WIDTH))
                                            .pb(px(TURN_BOTTOM_PADDING))
                                            .child(Chat::render_turn_fold_row(
                                                turn_id,
                                                label,
                                                at,
                                                &transcript_theme,
                                                entity.clone(),
                                            ))
                                            .into_any_element();
                                    }
                                    // The footer of an unfolded older turn
                                    // keeps its hairline-and-timestamp look
                                    // and becomes the way back: the clause is
                                    // "expand *and collapse*", and a fold the
                                    // reader can only ever open once is a
                                    // one-way door.
                                    TurnRowRole::Refoldable { turn_id } => {
                                        let refold_entity = entity.clone();
                                        let source_start = transcript_ranges
                                            .get(entry_index)
                                            .map(|range| range.start)
                                            .unwrap_or(0);
                                        return div()
                                            .id(("chat-entry", entry_index))
                                            .w_full()
                                            .max_w(px(TRANSCRIPT_WIDTH))
                                            .pb(px(TURN_BOTTOM_PADDING))
                                            .child(
                                                div()
                                                    .id(("turn-refold", turn_id))
                                                    .debug_selector(move || {
                                                        format!("turn-refold-{turn_id}")
                                                    })
                                                    .w_full()
                                                    .cursor(CursorStyle::PointingHand)
                                                    .on_click(move |_, _, cx| {
                                                        refold_entity.update(cx, |chat, cx| {
                                                            chat.toggle_turn_unfolded(turn_id, cx);
                                                        });
                                                    })
                                                    .children(
                                                        this.entries.get(entry_index).cloned().map(
                                                            |entry| {
                                                                Chat::render_entry(
                                                                    entry,
                                                                    entry_index,
                                                                    &transcript_theme,
                                                                    entity.clone(),
                                                                    transcript_focus.clone(),
                                                                    source_start,
                                                                    &question_answer,
                                                                    answer_caret_visible,
                                                                    this.copied_target.clone(),
                                                                    None,
                                                                    this.thought_is_streaming(
                                                                        entry_index,
                                                                    ),
                                                                    &this.thought_scroll,
                                                                    None,
                                                                    &mut *window,
                                                                    &mut *cx,
                                                                )
                                                            },
                                                        ),
                                                    ),
                                            )
                                            .into_any_element();
                                    }
                                    TurnRowRole::Normal => {}
                                }
                                // F-CHAT-22: a run of consecutive tool
                                // calls renders as one bordered box, keyed
                                // to the run's last index. Every other
                                // index in that run is "swallowed" — an
                                // empty row — since the list requires one
                                // measured row per index but the box lives
                                // only at the tail. A lone call is its own
                                // box with one row (the gallery's rule),
                                // and consecutive same-verb calls fold.
                                if let Some((start, end)) =
                                    tool_call_run_bounds_inclusive(&this.entries, entry_index)
                                {
                                    if entry_index != end {
                                        return div()
                                            .id(("chat-entry", entry_index))
                                            .into_any_element();
                                    }
                                    let members: Vec<(usize, usize, Entry)> = (start..=end)
                                        .filter_map(|index| {
                                            this.entries.get(index).cloned().map(|entry| {
                                                let source_start = transcript_ranges
                                                    .get(index)
                                                    .map(|range| range.start)
                                                    .unwrap_or(0);
                                                (index, source_start, entry)
                                            })
                                        })
                                        .collect();
                                    // Bezel Transcript pattern §2: a tool
                                    // run sits 8px from the prose that
                                    // follows it, tighter than the 10px
                                    // between turns elsewhere in the list.
                                    let run = this.render_tool_run(
                                        members,
                                        transcript_focus.clone(),
                                        &transcript_theme,
                                        &row_bezel_theme,
                                        entity.clone(),
                                    );
                                    return div()
                                        .id(("chat-entry", entry_index))
                                        .w_full()
                                        .max_w(px(TRANSCRIPT_WIDTH))
                                        .pb(px(8.0))
                                        .child(run)
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
                                        let bottom_padding = if matches!(entry, Entry::TurnFooter(_)) {
                                            TURN_BOTTOM_PADDING
                                        } else {
                                            10.0
                                        };
                                        // The heading is the processor's to
                                        // pass in: it holds `&this.entries`.
                                        let day_heading = match this.entries.get(entry_index) {
                                            Some(Entry::User { .. }) => transcript::heading_for(
                                                &this.entries,
                                                entry_index,
                                                chrono::Local::now(),
                                            ),
                                            _ => None,
                                        };
                                        let body = Chat::render_entry(
                                            entry,
                                            entry_index,
                                            &transcript_theme,
                                            entity.clone(),
                                            transcript_focus.clone(),
                                            source_start,
                                            &question_answer,
                                            answer_caret_visible,
                                            this.copied_target.clone(),
                                            this.edit_summaries.get(&entry_index).cloned(),
                                            this.thought_is_streaming(entry_index),
                                            &this.thought_scroll,
                                            day_heading.as_deref(),
                                            &mut *window,
                                            &mut *cx,
                                        )
                                        .into_any_element();
                                        div()
                                            .id(("chat-entry", entry_index))
                                            .w_full()
                                            .max_w(px(TRANSCRIPT_WIDTH))
                                            .pb(px(bottom_padding))
                                            .child(body)
                                            .into_any_element()
                                    })
                                    .unwrap_or_else(|| div().into_any_element())
                            }),
                        )
                        .with_sizing_behavior(ListSizingBehavior::Auto)
                        .flex_grow_1(),
                    )
                    .child(self.render_jump_to_latest(&bezel_theme, cx)),
            )
            // #239: the generating spinner, transient by construction. It is a
            // sibling of the transcript rather than an entry in it: the list is
            // virtualized off `entries.len()`, so a pseudo-entry would have to
            // be spliced in and out every turn and could be persisted or
            // duplicated. Living outside the list makes "never part of the
            // transcript" structural instead of a rule to maintain.
            .when(self.streaming, |this| {
                this.child(
                    div()
                        .id("chat-generating-spinner")
                        .debug_selector(|| "chat-generating-spinner".into())
                        .w_full()
                        .max_w(px(TRANSCRIPT_WIDTH))
                        .pt(px(6.0))
                        // The transient row is the thought header itself — orb,
                        // `Thinking`, same paddings — so a run in progress has one
                        // shape whether or not a thought has arrived. `usize::MAX`
                        // only feeds the row's marker ids; nothing reads them.
                        .child(Self::render_thought_header(
                            usize::MAX,
                            true,
                            false,
                            None,
                            &theme,
                            &bezel_theme,
                            window,
                            cx,
                            None,
                        )),
                )
            })
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
                        let bar_typography = theme.typography;
                        this.child(
                            div()
                                .id("pending-question-bar")
                                .debug_selector(|| "pending-question-bar".into())
                                .w_full()
                                .max_w(px(TRANSCRIPT_WIDTH))
                                .mb(px(8.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .px(px(10.0))
                                .py(px(6.0))
                                .rounded(theme.radii.control)
                                .bg(theme.surface_raised)
                                .border_1()
                                .border_color(theme.warning)
                                .text_size(bar_typography.footnote)
                                .child(div().text_color(theme.warning).child("?"))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_color(theme.text)
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
                                        .text_color(theme.text_faint)
                                        .hover(|style| style.bg(theme.overlay))
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
                    // #159: nothing is drawn between the composer card and
                    // the bottom of the pane. The working directory used to
                    // sit here as a centred caption, duplicating what the
                    // worktree selection already says — the same chrome #151
                    // dropped from the sidebar rows. `agent_cwd` itself stays:
                    // it is load-bearing for prompt content, mention
                    // resolution and the ACP client's launch directory.
                    //
                    // D-CHAT-03: the queue sits between the transcript and
                    // the card, where Zed keeps its queued messages — what
                    // sends next reads above what is being typed, not
                    // tucked under it. Drawn only while something is queued.
                    .when(!self.queue.is_empty(), |this| {
                        this.child(self.render_queue(&theme, cx))
                    })
                    .child(self.render_composer(&theme, window, cx)),
            )
            .child({
                // F-CHAT-13: the "Drop files to attach" overlay, matching
                // Swift's `ChatPaneView` — invisible by default, revealed by
                // gpui's own `drag_over` style refinement while an
                // `ExternalPaths` drag sits over the pane. Not drawn at all
                // while the composer can't accept input, matching the
                // top-level `on_drop` binding just above.
                let marker = theme.text;
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
                    .border_color(marker)
                    .bg(theme.overlay)
                    .child(
                        div()
                            .id("chat-drop-overlay-label")
                            .debug_selector(|| "chat-drop-overlay-label".into())
                            .text_color(theme.text)
                            .child("Drop files to attach"),
                    );
                if can_accept_drop {
                    overlay.drag_over::<ExternalPaths>(|style, _, _, _| style.visible())
                } else {
                    overlay
                }
            })
            // bezel's scrollbar over the list, on the root's right edge and
            // as tall as the list's viewport (the transcript's own top
            // padding below the root's top edge).
            .child(list_scroll::list_scrollbar(
                "chat-transcript",
                px(28.0),
                &self.list_state,
                &self.transcript_bar,
            ))
            // The turn rail, over the root's left margin; last, so it sits
            // above everything it is laid over.
            .child(self.render_turn_rail(&bezel_theme, cx))
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
                // Mention paths are `@`-token text and agent-side references,
                // so they always use forward slashes regardless of the host
                // filesystem's separator.
                relative_paths.push(relative.to_string_lossy().replace('\\', "/"));
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

fn option_hash(option: &AnswerOption) -> usize {
    option.id.bytes().fold(0usize, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as usize)
    })
}

/// F-CHAT-22: the `[start, end]` bounds (inclusive) of the consecutive run
/// of `Entry::ToolCall` entries that `index` belongs to — `(index, index)`
/// for a lone call. Returns `None` only for an index that isn't a tool call
/// at all.
fn tool_call_run_bounds_inclusive(entries: &[Entry], index: usize) -> Option<(usize, usize)> {
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
    Some((start, end))
}

/// F-CHAT-22, turn half: how many of the most recent turns stay open. Swift's
/// `TimelineBuilder.openTurnCount` — "current + previous".
const OPEN_TURN_COUNT: usize = 2;

/// F-CHAT-22: the fold row's label is the turn's opening question, clipped.
/// Swift's `Turn.label` takes `String(line.prefix(60))`.
const TURN_LABEL_MAX_CHARS: usize = 60;

/// F-CHAT-22: one segmented turn — the contiguous entries between two turn
/// footers. `footer` is `None` for the trailing, still-open turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TurnSegment {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) footer: Option<usize>,
}

impl TurnSegment {
    /// Swift's `!turn.items.isEmpty`: a turn whose only entry is its own
    /// footer has nothing to fold away.
    fn has_body(&self) -> bool {
        self.footer != Some(self.start)
    }
}

/// Splits the transcript into turns at each [`Entry::TurnFooter`], the way
/// Swift's `TimelineBuilder.segment` splits at each `.turnDivider`. The
/// footer belongs to the turn it closes; a trailing run with no footer is
/// the open turn, and is only recorded when it actually has entries.
fn segment_turns(entries: &[Entry]) -> Vec<TurnSegment> {
    let mut turns = Vec::new();
    let mut start = 0usize;
    for (index, entry) in entries.iter().enumerate() {
        if matches!(entry, Entry::TurnFooter(_)) {
            turns.push(TurnSegment {
                start,
                end: index,
                footer: Some(index),
            });
            start = index + 1;
        }
    }
    if start < entries.len() {
        turns.push(TurnSegment {
            start,
            end: entries.len() - 1,
            footer: None,
        });
    }
    turns
}

/// F-CHAT-22's fold POLICY, transcribed from `TimelineBuilder.rows`:
///
/// ```swift
/// let isFoldable = index < turns.count - openTurnCount
///     && turn.divider != nil && !turn.items.isEmpty
/// ```
///
/// A turn folds when it is *closed* (it has a footer — a turn still being
/// answered is never taken away from the reader), when it has a body worth
/// hiding, and when at least `OPEN_TURN_COUNT` newer turns exist. The
/// current turn and the one before it therefore always stay open, and a
/// turn folds by itself the moment a second newer turn closes — nothing
/// folds on a timer or on scrolling away.
///
/// Written as `position + OPEN_TURN_COUNT < turns.len()` rather than Swift's
/// subtraction because `turns.len() - 2` underflows on `usize` for a
/// transcript with fewer than two turns.
fn turn_is_foldable(turns: &[TurnSegment], position: usize) -> bool {
    turns.get(position).is_some_and(|turn| {
        turn.footer.is_some() && turn.has_body() && position + OPEN_TURN_COUNT < turns.len()
    })
}

/// The fold row's label: the first line of the turn's first user message,
/// clipped to [`TURN_LABEL_MAX_CHARS`]. Swift's `Turn.label`, including its
/// `"Turn"` fallback for a turn that opened without one.
fn turn_label(entries: &[Entry], turn: &TurnSegment) -> String {
    for index in turn.start..=turn.end {
        if let Some(Entry::User { text, .. }) = entries.get(index) {
            let line = text.lines().next().unwrap_or(text.as_str());
            return line.chars().take(TURN_LABEL_MAX_CHARS).collect();
        }
    }
    "Turn".to_string()
}

/// F-CHAT-22: what the virtualized transcript list should draw at one entry
/// index once turn folding is applied.
///
/// The list keeps exactly one row per entry, so a folded turn cannot delete
/// rows; it draws its stand-in at the turn's first index and reports every
/// other index as [`Self::Hidden`] — the same trick the tool-call group
/// already uses for a run's non-tail members.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TurnRowRole {
    Normal,
    Hidden,
    /// The collapsed stand-in for a whole turn. `turn_id` is the turn's
    /// footer index, its identity in `unfolded_turns`.
    Fold {
        turn_id: usize,
        label: String,
        at: String,
    },
    /// The footer of a foldable turn the reader has unfolded: clicking it
    /// folds the turn back up.
    Refoldable {
        turn_id: usize,
    },
}

/// One [`TurnRowRole`] per entry, computed once per frame.
fn turn_row_roles(entries: &[Entry], unfolded: &BTreeSet<usize>) -> Vec<TurnRowRole> {
    let turns = segment_turns(entries);
    let mut roles = vec![TurnRowRole::Normal; entries.len()];
    for (position, turn) in turns.iter().enumerate() {
        if !turn_is_foldable(&turns, position) {
            continue;
        }
        let turn_id = turn.end;
        if unfolded.contains(&turn_id) {
            roles[turn_id] = TurnRowRole::Refoldable { turn_id };
            continue;
        }
        let label = turn_label(entries, turn);
        let at = match entries.get(turn_id) {
            Some(Entry::TurnFooter(text)) => text.clone(),
            _ => String::new(),
        };
        for role in &mut roles[turn.start..=turn.end] {
            *role = TurnRowRole::Hidden;
        }
        roles[turn.start] = TurnRowRole::Fold { turn_id, label, at };
    }
    roles
}

/// F-CHAT-31: one line of a diff preview.
///
/// `number` is the line's position in the file it belongs to — the *new*
/// file for context and added lines, the *old* file for removed ones —
/// exactly as Swift's `ChatDiffPreviewRow.lineNumber` is assigned
/// (`newIndex + 1` / `oldIndex + 1`) before `ChatDiffPreviewView.row` prints
/// it with `String(format: "%3d", …)`. Without it a reader has no way to
/// say *where* in the file a proposed change lands.
#[derive(Clone, Debug, PartialEq, Eq)]
enum DiffLine {
    Context { number: usize, text: String },
    Removed { number: usize, text: String },
    Added { number: usize, text: String },
}

impl DiffLine {
    // `render_tool_diff` now reads `number` directly per-variant to split it
    // into old/new columns, so this accessor's only remaining caller is
    // `diff_preview_lines_number_each_side_against_its_own_file` below.
    #[allow(dead_code)]
    fn number(&self) -> usize {
        match self {
            Self::Context { number, .. }
            | Self::Removed { number, .. }
            | Self::Added { number, .. } => *number,
        }
    }

    fn text(&self) -> &str {
        match self {
            Self::Context { text, .. } | Self::Removed { text, .. } | Self::Added { text, .. } => {
                text
            }
        }
    }
}

/// Caps the number of diff lines rendered in the transcript; a full-file
/// rewrite should not make the transcript unusable.
const DIFF_PREVIEW_MAX_LINES: usize = 60;

/// Width of each of a diff preview's two line-number columns (old, then
/// new). Swift reserves 30pt for a `%3d` field plus 8pt of trailing padding;
/// four digits is the realistic worst case in a file this preview would
/// ever show.
const DIFF_GUTTER_WIDTH: f32 = 34.0;

/// The gallery's standalone Diff pattern is drawn at 760. Recorded so the
/// number in the spec has a home in the code, and so the rule below can say
/// what it is *not* doing. Read only by the regression test guarding that
/// rule, hence the lint allowance.
#[allow(dead_code)]
const DIFF_STANDALONE_REFERENCE: f32 = 760.0;

/// A diff inside the transcript uses the width it is given. It never forces
/// the standalone 760 -- the transcript column is narrower, and a diff that
/// overflowed it would scroll the whole turn sideways.
fn diff_column_width(available: f32) -> f32 {
    available
}

/// Everything a drawn diff preview needs beyond the diff itself (F-CHAT-31).
struct DiffPreviewContext {
    /// Stable prefix for this preview's interactive element ids, unique
    /// across the transcript so two previews never collide.
    id_prefix: String,
    /// The chat the header's open-file click emits through.
    entity: Entity<Chat>,
    /// `Some` when this diff belongs to a top-level transcript entry, whose
    /// [`ToolCallPlainText`] projection defines the selection coordinate
    /// space its rows live in. The nested subagent card passes `None`: its
    /// entry's `plain_text` arm lists only child titles and statuses, so
    /// there is no honest offset to anchor a selection at, and inventing one
    /// would make Ctrl+C copy text that is not what the highlight covers.
    selection: Option<DiffPreviewSelection>,
}

struct DiffPreviewSelection {
    interaction: TranscriptInteraction,
    /// Absolute transcript offset of each drawn preview line, in order.
    line_starts: Vec<usize>,
}

/// The plain-text projection of one tool-call entry, kept as lines rather
/// than one string so a caller can address a single line inside it.
///
/// [`Entry::plain_text`] joins these with `"\n"` to contribute this entry's
/// slice of the transcript's global selection coordinate space, and
/// [`Chat::render_tool_diff`] reads `diff_starts` to anchor each *drawn*
/// diff row in that same space (F-CHAT-31). One producer for both is the
/// whole point: a selection highlight painted over a diff row and the text
/// `selected_transcript_text` copies for that highlight cannot drift apart
/// if neither side is allowed its own idea of what the text is.
struct ToolCallPlainText {
    lines: Vec<String>,
    /// Index into `lines` of the first *preview* line (i.e. past the
    /// `diff: <path>` header line) of each diff, in `content` order.
    diff_starts: Vec<usize>,
}

impl ToolCallPlainText {
    fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Byte offset of `lines[index]` within [`Self::text`].
    fn offset_of(&self, index: usize) -> usize {
        self.lines[..index.min(self.lines.len())]
            .iter()
            .map(|line| line.len() + 1)
            .sum()
    }

    /// Absolute transcript offsets of the `ordinal`-th diff's drawn preview
    /// lines, given where this entry starts in the transcript.
    fn diff_line_starts(&self, ordinal: usize, entry_start: usize, count: usize) -> Vec<usize> {
        let Some(first) = self.diff_starts.get(ordinal).copied() else {
            return Vec::new();
        };
        (0..count)
            .map(|offset| entry_start + self.offset_of(first + offset))
            .collect()
    }
}

fn tool_call_plain_text(
    title: &str,
    status: &str,
    content: &[ToolCallContentInfo],
    locations: &[ToolCallLocationInfo],
) -> ToolCallPlainText {
    let mut lines = vec![format!("{title}\n{status}")];
    let mut diff_starts = Vec::new();
    for item in content {
        match item {
            ToolCallContentInfo::Text(text) => lines.push(text.clone()),
            ToolCallContentInfo::Diff(diff) => {
                lines.push(format!("diff: {}", diff.path.to_string_lossy()));
                diff_starts.push(lines.len());
                lines.extend(
                    diff_preview_lines(diff.old_text.as_deref(), &diff.new_text)
                        .into_iter()
                        .take(DIFF_PREVIEW_MAX_LINES)
                        .map(|line| line.text().to_string()),
                );
            }
            ToolCallContentInfo::Other => {}
        }
    }
    for location in locations {
        lines.push(location.path.to_string_lossy().into_owned());
    }
    ToolCallPlainText { lines, diff_starts }
}

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
            rows.push(DiffLine::Context {
                number: new_index + 1,
                text: old_lines[old_index].clone(),
            });
            old_index += 1;
            new_index += 1;
        } else {
            if old_index < old_lines.len() {
                rows.push(DiffLine::Removed {
                    number: old_index + 1,
                    text: old_lines[old_index].clone(),
                });
                old_index += 1;
            }
            if new_index < new_lines.len() {
                rows.push(DiffLine::Added {
                    number: new_index + 1,
                    text: new_lines[new_index].clone(),
                });
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

/// The chevron for the composer's picker chips (mode, model, effort).
///
/// The shared `ChevronDown` SVG rather than the old `⌄` text glyph: a text
/// glyph rides the font baseline and sat low next to its label, while the
/// icon paints from its own centered box, so it sits on the row's optical
/// center beside the text.
fn picker_chevron(theme: &Theme) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .child(IconElement::new(Icon::ChevronDown, IconSize::XSmall).text_color(theme.text_faint))
}

/// The edge drawn over the composer text field while it is focused.
///
/// Bezel's `TextField` paints its own 1px border — `theme.ring` on focus —
/// with no opt-out, so the composer covers it with a border of its own in
/// exactly the same box (see `composer-input`): opaque `surface_card`, the
/// tone the field sits closest to, so the bright ring never shows through.
/// Idle is untouched — the field's own subtle `border` still shows — only
/// the focus flash is replaced by a dark hairline.
fn composer_field_edge(theme: &bezel::theme::Theme) -> gpui::Hsla {
    theme.surface_card
}

/// The composer card's border colour for the given focus state.
///
/// Focus does not brighten the card: the border stays `border` either way,
/// the hairline closest to the background, so the composer never lights up.
/// It used to be the body `text` colour, which on the dark theme painted a
/// solid white frame around the card, and later `border_strong`, still
/// visibly brighter than the surface.
fn composer_border(_focused: bool, theme: &bezel::theme::Theme) -> gpui::Hsla {
    theme.border
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{FileDropEvent, Modifiers, TestAppContext, VisualTestContext, size};
    use sirio_acp::EffortChoice;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    /// The highlighter `init` installs into bezel-markdown is bezel-syntax's
    /// tree-sitter classification: a fenced block's tag reaches a grammar,
    /// so a `rust` block gets keyword spans instead of one plain run. A tag
    /// no grammar answers is `None`, which bezel-markdown paints plain.
    #[test]
    fn markdown_code_blocks_are_classified_by_bezel_syntax() {
        let code = "fn main() {}";
        let spans = highlight_markdown_code("rust", code)
            .expect("bezel-syntax carries a rust grammar by default");
        assert!(
            spans.iter().any(|(range, kind)| {
                matches!(kind, bezel::theme::HighlightKind::Keyword) && &code[range.clone()] == "fn"
            }),
            "`fn` must be classified as a keyword; got {} spans",
            spans.len()
        );
        assert!(
            highlight_markdown_code("no-such-language", code).is_none(),
            "an unknown fence tag falls back to plain code rather than guessing"
        );
    }

    #[test]
    fn composer_field_edge_hides_the_bright_focus_ring() {
        for theme in [bezel::theme::Theme::dark(), bezel::theme::Theme::light()] {
            let edge = composer_field_edge(&theme);
            // Opaque: the bezel ring underneath must not show through.
            assert_eq!(
                edge.a, 1.0,
                "the field edge must be opaque, got {edge:?}"
            );
            // Distinct from the focus ring it covers, in both appearances
            // (white ring on dark, black ring on light).
            let gap = (edge.l - theme.ring.l).abs();
            assert!(
                gap > 0.05,
                "the field edge must read apart from the bezel ring, edge={edge:?} ring={:?}",
                theme.ring
            );
        }
    }

    #[test]
    fn composer_focus_border_is_the_theme_ring_not_body_text() {
        for theme in [bezel::theme::Theme::dark(), bezel::theme::Theme::light()] {
            assert_eq!(composer_border(false, &theme), theme.border);
            let focused = composer_border(true, &theme);
            // Focus never brightens the card: same hairline as idle, close
            // to the background, never the body text colour.
            assert_eq!(focused, theme.border);
            assert_ne!(focused, theme.text);
            assert!(
                focused.a < 1.0,
                "the focus border must be translucent, got {focused:?}"
            );
            assert!(
                focused.a < theme.ring.a,
                "the composer focus must stay darker than the bezel ring, got {focused:?} vs {:?}",
                theme.ring
            );
        }
    }

    /// A typical streamed answer is reparsed as deltas arrive. This deliberately
    /// uses a coarse wall-clock ceiling: it catches an accidental super-linear
    /// parse/build path without pretending to be a benchmark.
    #[test]
    fn a_four_kib_streaming_turn_builds_a_bezel_doc_within_the_frame_budget() {
        let fragment = "## Result\n\n- [x] parsed\n- [ ] rendered\n\n```rust\nlet answer = 42;\n```\n\n| step | state |\n| --- | --- |\n| parse | done |\n\n";
        let source = fragment.repeat(35);
        assert!(
            (4_096..=5_120).contains(&source.len()),
            "fixture must stay close to 4 KiB, got {} bytes",
            source.len()
        );

        let started = std::time::Instant::now();
        let document = parse_chat_markdown(&source);
        let elapsed = started.elapsed();
        let budget = if cfg!(debug_assertions) {
            std::time::Duration::from_millis(50)
        } else {
            std::time::Duration::from_millis(5)
        };

        assert!(
            !document.blocks.is_empty(),
            "parse/build must produce the bezel document rendered by chat"
        );
        assert!(
            elapsed < budget,
            "4 KiB markdown parse/build took {elapsed:?}, budget is {budget:?}"
        );
    }

    fn spinner_test_chat(cx: &mut TestAppContext) -> (Entity<Chat>, &mut VisualTestContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::from_test_command(
                AgentCommand::new("/definitely/missing/sirio-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            chat.has_completed_turn = true;
            chat
        });
        cx.update(|window, _| window.refresh());
        (chat, cx)
    }

    /// #239: the indicator shown while a turn streams is the Activity-derived
    /// reasoning header (Task 6), not a Chat-owned animation.
    #[gpui::test]
    async fn the_running_reasoning_header_shows_the_thinking_indicator(cx: &mut TestAppContext) {
        let (chat, cx) = spinner_test_chat(cx);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        assert!(
            cx.debug_bounds("chat-generating-spinner").is_some(),
            "a streaming turn shows the Activity-derived indicator"
        );
    }

    /// #239: the indicator is shown for exactly as long as a turn is in
    /// flight, driven by the streaming flag the composer's border already
    /// uses — no lifecycle of its own to fall out of step.
    #[gpui::test]
    async fn the_generating_spinner_appears_while_a_turn_streams(cx: &mut TestAppContext) {
        let (chat, cx) = spinner_test_chat(cx);

        assert!(
            cx.debug_bounds("chat-generating-spinner").is_none(),
            "an idle chat shows no spinner"
        );

        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));

        assert!(
            cx.debug_bounds("chat-generating-spinner").is_some(),
            "a streaming turn shows the spinner"
        );
    }

    /// The same assertion covers completion, cancellation and error: all three
    /// clear `streaming`, and the spinner is a pure function of that flag.
    #[gpui::test]
    async fn the_generating_spinner_disappears_when_the_turn_ends(cx: &mut TestAppContext) {
        let (chat, cx) = spinner_test_chat(cx);

        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        assert!(cx.debug_bounds("chat-generating-spinner").is_some());

        chat.update(cx, |chat, cx| {
            chat.streaming = false;
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));

        assert!(
            cx.debug_bounds("chat-generating-spinner").is_none(),
            "the spinner leaves with the turn"
        );
    }

    /// The transient generating spinner is the same row a live thought's
    /// header is: orb, `Thinking`, same paddings — one shape for a run in
    /// progress whether or not a thought has arrived.
    #[gpui::test]
    async fn the_generating_spinner_shares_the_thought_header_shape(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));
        cx.update(|_window, cx| init(cx));
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(AcpEvent::ThoughtChunk("a".into()), cx);
        });
        refresh_frame(cx);
        let header = cx
            .debug_bounds("thought-toggle-0")
            .expect("live thought header");
        chat.update(cx, |chat, cx| {
            chat.handle_event(AcpEvent::AgentMessageChunk("b".into()), cx);
        });
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("chat-generating-spinner").is_some(),
            "the spinner row while the turn streams"
        );
        let spinner_header = cx
            .debug_bounds("thought-toggle-18446744073709551615")
            .expect("the spinner draws the thought header row");
        assert_eq!(
            spinner_header.size.height, header.size.height,
            "one row shape: spinner={spinner_header:?} header={header:?}"
        );
    }

    /// #239 requires the indicator stay transient and never join the
    /// transcript. It is a sibling of the virtualized list rather than an
    /// entry in it, so streaming must not move the entry count at all — this
    /// is what makes "not persisted" and "never duplicated" structural.
    #[gpui::test]
    async fn the_transcript_gains_no_entry_for_the_spinner(cx: &mut TestAppContext) {
        let (chat, cx) = spinner_test_chat(cx);
        let before = chat.read_with(&*cx, |chat, _| chat.entries.len());

        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));

        let during = chat.read_with(&*cx, |chat, _| chat.entries.len());
        assert_eq!(
            before, during,
            "the spinner is not a transcript entry, so streaming adds none"
        );
    }

    /// #239 piece 2: the spinner must leave when a turn *completes*, driven
    /// through the real fixture agent rather than by setting the flag — the
    /// flag is what the earlier tests pin, and a flag can be right while the
    /// path that clears it is not.
    #[gpui::test]
    async fn the_spinner_leaves_when_a_real_turn_completes(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let fixture_dir = dir.0.to_str().expect("fixture dir is utf-8").to_string();
        let (chat, cx) = chat_view(cx, &["staged", &fixture_dir]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "hello");
        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| chat.streaming);
        refresh_frame(cx);

        assert!(
            cx.debug_bounds("chat-generating-spinner").is_some(),
            "the spinner is up as soon as the turn starts"
        );

        std::fs::write(dir.0.join("go"), "go").expect("write go file");
        pump_chat_until(cx, &chat, |chat| !chat.streaming);
        refresh_frame(cx);

        assert!(
            cx.debug_bounds("chat-generating-spinner").is_none(),
            "and gone once the turn has ended"
        );
    }

    /// #239 piece 2: cancellation. Escape reaches `Chat::cancel` through the
    /// real binding, so this exercises the same door a user does.
    #[gpui::test]
    async fn the_spinner_leaves_when_a_turn_is_cancelled(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let fixture_dir = dir.0.to_str().expect("fixture dir is utf-8").to_string();
        let (chat, cx) = chat_view(cx, &["staged", &fixture_dir]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "hello");
        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| chat.streaming);
        refresh_frame(cx);
        assert!(cx.debug_bounds("chat-generating-spinner").is_some());

        cx.simulate_keystrokes("escape");
        pump_chat_until(cx, &chat, |chat| !chat.streaming);
        refresh_frame(cx);

        assert!(
            cx.debug_bounds("chat-generating-spinner").is_none(),
            "cancelling a turn takes the spinner with it"
        );
    }

    /// #239 piece 2: the error path. `TransportError` is handed to
    /// `handle_event` directly — it is the same handler the transport calls,
    /// and killing a live fixture mid-turn from a test would be racing the
    /// very state under assertion.
    #[gpui::test]
    async fn the_spinner_leaves_when_the_transport_fails(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let fixture_dir = dir.0.to_str().expect("fixture dir is utf-8").to_string();
        let (chat, cx) = chat_view(cx, &["staged", &fixture_dir]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "hello");
        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| chat.streaming);
        refresh_frame(cx);
        assert!(cx.debug_bounds("chat-generating-spinner").is_some());

        chat.update(cx, |chat, cx| {
            chat.handle_event(AcpEvent::TransportError("agent went away".into()), cx);
        });
        pump_chat_until(cx, &chat, |chat| !chat.streaming);
        refresh_frame(cx);

        assert!(
            cx.debug_bounds("chat-generating-spinner").is_none(),
            "a failed turn must not leave the spinner running forever"
        );
    }

    /// #239 piece 3, and the acceptance criterion most likely to fail: the
    /// spinner appears in the same column as the composer, so it could push
    /// it down or shrink it. The composer's drawn rectangle must be
    /// bit-identical between idle and streaming.
    #[gpui::test]
    async fn the_spinner_does_not_move_or_resize_the_composer(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let fixture_dir = dir.0.to_str().expect("fixture dir is utf-8").to_string();
        let (chat, cx) = chat_view(cx, &["staged", &fixture_dir]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        let idle = cx.debug_bounds("composer").expect("the composer is drawn");

        focus_and_type(cx, "hello");
        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| chat.streaming);
        refresh_frame(cx);
        assert!(cx.debug_bounds("chat-generating-spinner").is_some());

        let streaming = cx.debug_bounds("composer").expect("the composer is drawn");

        // #242 fixed: the streaming ring no longer participates in layout,
        // so the composer's drawn rectangle is identical either way. It used
        // to move 1px and shrink 2px every time a turn started, because the
        // ring wrapped the card and padded it inward -- the opposite of what
        // `STREAMING_BORDER_WIDTH`'s own comment promised.
        //
        // The spinner is a ~20px row in the same column, so this equality is
        // also what proves the spinner itself displaces nothing.
        assert_eq!(
            idle.size, streaming.size,
            "streaming must not resize the composer: idle {:?} vs streaming {:?}",
            idle.size, streaming.size
        );
        assert_eq!(
            idle.origin, streaming.origin,
            "streaming must not move the composer: idle {:?} vs streaming {:?}",
            idle.origin, streaming.origin
        );
    }

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
                .join(format!("sirio-chat-test-{}-{unique}", std::process::id()));
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
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }

    fn chat_view<'a>(
        cx: &'a mut TestAppContext,
        fixture_args: &[&str],
    ) -> (gpui::Entity<Chat>, &'a mut VisualTestContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        cx.update(init);
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

    /// The card carries the chip row and the send disc: field on top, then
    /// one wrapping row of chips ending in the disc — no toolbar above the
    /// card, no hint text in it.
    #[gpui::test]
    async fn composer_card_carries_the_chip_row_and_the_send_disc(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        chat.update(cx, |chat, _| configure_test_chat(chat));
        chat.update(cx, |chat, _| {
            chat.effort = Some(EffortOption {
                option_id: "effort".into(),
                name: Some("Effort".into()),
                current_value: Some("xhigh".into()),
                choices: vec![EffortChoice {
                    value: "xhigh".into(),
                    name: "Xhigh".into(),
                }],
            });
        });
        refresh_frame(cx);

        let card = cx.debug_bounds("composer").expect("card");
        let input = cx.debug_bounds("composer-input").expect("field");
        let send = cx.debug_bounds("send").expect("send disc");
        let attach = cx.debug_bounds("attach-image").expect("attach");
        let overflow = cx.debug_bounds("composer-overflow").expect("overflow");
        let context = cx.debug_bounds("context-ring").expect("context ring");
        let effort = cx.debug_bounds("effort-chip").expect("effort chip");
        let chip = cx.debug_bounds("model-chip").expect("model chip");
        // Every chip and the disc live inside the card, below the field.
        for (name, bounds) in [
            ("model chip", chip),
            ("effort chip", effort),
            ("context", context),
            ("attach", attach),
            ("overflow", overflow),
            ("send", send),
        ] {
            assert!(
                bounds.left() >= card.left() && bounds.right() <= card.right(),
                "{name} lives inside the card: {name}={bounds:?} card={card:?}"
            );
            assert!(
                bounds.top() >= input.bottom(),
                "{name} sits below the field: {name}={bounds:?} input={input:?}"
            );
        }
        // The trio is one run ending in the disc.
        assert!(
            attach.right() <= overflow.left(),
            "attach precedes overflow: {attach:?} {overflow:?}"
        );
        assert!(
            overflow.right() <= send.left(),
            "overflow precedes the disc: {overflow:?} {send:?}"
        );
        assert!(
            send.right() <= card.right(),
            "the disc stays inside the card: send={send:?} card={card:?}"
        );
        // No toolbar above, no hint text inside.
        assert!(
            cx.debug_bounds("composer-toolbar").is_none(),
            "the toolbar above the card is gone"
        );
        assert!(
            cx.debug_bounds("composer-hint").is_none(),
            "the hint text is gone"
        );
        // The disc is still inert until there is something to send.
        assert!(
            cx.debug_bounds("send-ready").is_none(),
            "an empty draft leaves the disc inert"
        );
        focus_and_type(cx, "go");
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("send-ready").is_some(),
            "a draft arms the disc"
        );
    }

    /// `up`/`down` drive a picker while one is open and are the field's own
    /// vertical motion otherwise — the handlers propagate when no popup is
    /// on screen, so gpui reaches the TextField's binding next.
    #[gpui::test]
    async fn arrows_move_the_caret_when_no_popup_is_open(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);
        focus_and_type(cx, "one");
        cx.simulate_keystrokes("shift-enter");
        cx.simulate_input("two");
        cx.run_until_parked();
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.draft_text()),
            "one
two"
        );
        let end = chat.read_with(&cx.cx, |chat, _| chat.draft_caret);
        cx.simulate_keystrokes("up");
        cx.run_until_parked();
        let moved = chat.read_with(&cx.cx, |chat, _| chat.draft_caret);
        assert!(
            moved < end,
            "up without a popup moves the caret to the first row ({moved} < {end})"
        );
        assert!(chat.read_with(&cx.cx, |chat, _| chat.entries.is_empty()));
    }

    fn focus_and_type(cx: &mut VisualTestContext, text: &str) {
        let composer = cx.debug_bounds("composer").expect("the composer is drawn");
        cx.simulate_click(composer.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input(text);
    }

    struct MarkdownHarness {
        document: markdown::Doc,
    }

    impl Render for MarkdownHarness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                div()
                    .id("assistant-response-0")
                    .debug_selector(|| "assistant-response-0".into())
                    .w_full()
                    .child(MarkdownBody::new(self.document.clone())),
            )
        }
    }

    fn markdown_view(cx: &mut TestAppContext, markdown: String) -> VisualTestContext {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        cx.update(init);
        let window = cx.open_window(size(px(900.0), px(900.0)), move |_, _| MarkdownHarness {
            document: parse_chat_markdown(&markdown),
        });
        VisualTestContext::from_window(window.into(), cx)
    }

    /// #239/Task 7: the rotating streaming border is retired, so the
    /// composer's border is the same footprint and color idle or streaming —
    /// nothing wraps the card, and nothing shifts the draft text inside it.
    #[gpui::test]
    async fn the_composer_has_no_rotating_border_while_streaming(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &[]);
        refresh_frame(cx);

        let idle_card = cx.debug_bounds("composer").expect("the composer is drawn");
        let idle_input = cx
            .debug_bounds("composer-input")
            .expect("the composer input is drawn");

        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            cx.notify();
        });
        refresh_frame(cx);

        let streaming_card = cx.debug_bounds("composer").expect("the composer is drawn");
        let streaming_input = cx
            .debug_bounds("composer-input")
            .expect("the composer input is drawn");
        assert_eq!(
            idle_card, streaming_card,
            "the composer's footprint does not move when a turn starts streaming"
        );
        assert_eq!(
            idle_input, streaming_input,
            "the draft text must not shift when a turn starts streaming"
        );

        chat.update(cx, |chat, cx| {
            chat.streaming = false;
            cx.notify();
        });
        refresh_frame(cx);

        let idle_again = cx.debug_bounds("composer").expect("the composer is drawn");
        assert_eq!(
            idle_card, idle_again,
            "the composer's footprint is unchanged after streaming ends"
        );
    }

    /// A draft longer than the composer is wide must wrap onto a second line
    /// inside the card, not run out past its right border. Every draft text
    /// run is a flex item in the wrapping `composer-input` row, and a flex
    /// item's default `min-width: auto` pins it to its own max-content width
    /// — one unbroken sentence measures far wider than the card and simply
    /// overflows it, taking the caret with it onto a line of its own.
    #[gpui::test]
    async fn a_long_draft_wraps_inside_the_composer_instead_of_overflowing_it(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        // The app's real captured running size, the width the overflow was
        // reported at.
        cx.simulate_resize(size(px(1715.0), px(972.0)));
        refresh_frame(cx);

        focus_and_type(
            cx,
            "Correggi un bug che si presenta nell'editor: il cursore di battitura \
             non lampeggia e non si muove",
        );
        refresh_frame(cx);

        let card = cx
            .debug_bounds("composer")
            .expect("the composer card is drawn");
        let input = cx
            .debug_bounds("composer-input")
            .expect("the composer input row is drawn");
        let idle_height = input.size.height;

        assert!(
            input.origin.x + input.size.width <= card.origin.x + card.size.width,
            "the field must stay inside the composer card, not spill past its              right edge: input={input:?} card={card:?}"
        );

        focus_and_type(cx, &"word ".repeat(200));
        refresh_frame(cx);
        let input = cx
            .debug_bounds("composer-input")
            .expect("the composer input row is drawn");
        assert!(
            input.size.height > idle_height,
            "typing 200 words grows the field: before={idle_height:?} after={input:?}"
        );
    }

    /// The insertion caret marks the exact character position, so it must sit
    /// flush against the character it follows. The draft used to be split
    /// into flex items around the caret (`composer-input` carried a 4px
    /// `gap_x` for chips, and flex gap applies between *every* adjacent
    /// pair), which pushed the bar a phantom space away from the last typed
    /// character. Now one `ComposerText` element paints the bar itself, at
    /// the text layout's own position for the caret index; the headless text
    /// system stubs every glyph at one width (see `conformance.rs`), so this
    /// checks the layout contract — the bar ends where the run ends — and
    /// not glyph-level alignment, which only a screenshot can.
    /// A draft that wraps keeps its caret on the text: the end-of-draft bar
    /// used to be a separate flex item after the text run, and a wrapped
    /// item in a `flex_wrap` row lands on a row of its own — the bar dropped
    /// to an empty third line below the draft. The bar is now painted by the
    /// text element at the layout position of its last character, so it sits
    /// inside the run's bounds on the last wrapped line, and a caret moved
    /// back into the run sits on the first line.
    /// The composer has always *had* a selection — `SelectLeft`,
    /// `SelectRight` and `SelectAll` all mutate it, and `delete_selected`
    /// acts on it — but nothing ever drew it. Select-all followed by one
    /// keystroke therefore replaced the entire draft with no on-screen sign
    /// that anything had been selected: a destructive edit with an
    /// invisible precondition. The fill is painted by the text element, one
    /// quad per wrapped line, behind the text it covers.
    /// F-CHAT-25's answer field takes typed characters through
    /// `on_composer_key`, so it is a text field by every measure except the
    /// one the user checks: it drew no insertion bar at all.
    #[gpui::test]
    async fn the_question_answer_field_draws_a_caret_while_it_holds_focus(cx: &mut TestAppContext) {
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

        let field = cx
            .debug_bounds("question-answer-input")
            .expect("the pending question offers a text answer field");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        refresh_frame(cx);

        assert!(
            cx.debug_bounds("question-answer-caret").is_some(),
            "a field that accepts typing must show where the next character lands"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.answer_caret_visible),
            "and the bar must be lit while the field holds focus, not merely \
             present in layout"
        );
    }

    /// #159/Task 7: the retired animated border used to be the last
    /// transcript-width element laid out at a *fixed* 720px, holding a
    /// narrow pane open. With the border gone, the composer itself must
    /// still track the narrow pane the same way idle or streaming.
    #[gpui::test]
    async fn a_narrow_pane_draws_no_rotating_border(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &[]);
        cx.simulate_resize(size(px(595.0), px(600.0)));
        refresh_frame(cx);

        let idle_card = cx.debug_bounds("composer").expect("the composer is drawn");
        assert!(
            idle_card.size.width < px(TRANSCRIPT_WIDTH),
            "fixture invariant: the pane must be narrower than the transcript \r
             width for this test to exercise anything: {idle_card:?}"
        );

        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            cx.notify();
        });
        refresh_frame(cx);

        let streaming_card = cx.debug_bounds("composer").expect("the composer is drawn");
        assert_eq!(
            streaming_card, idle_card,
            "the composer's border tracks the narrow pane the same way idle \
             or streaming, not held open at the transcript width: \
             idle={idle_card:?} streaming={streaming_card:?}"
        );
    }

    /// #168: a transcript is the record of what an agent did to a repository,
    /// and reading it back after a restart is exactly when someone is
    /// reconstructing that — so a restored tool row has to keep naming the
    /// file it touched, and the card has to keep its kind. Both used to be
    /// dropped on the way into storage, so #119's target vanished at the
    /// first restart.
    #[test]
    fn a_tool_call_round_trips_its_kind_and_locations() {
        let live = Entry::ToolCall {
            id: "tool-1".into(),
            title: "read".into(),
            status: "Completed".into(),
            kind: "Read".into(),
            content: Vec::new(),
            locations: vec![ToolCallLocationInfo {
                path: PathBuf::from("src").join("main.rs"),
                line: Some(42),
            }],
            raw_input: None,
            raw_output: None,
            expanded: false,
            duration_ms: None,
        };

        let stored = persisted_entry(&live).expect("a tool call is persisted");
        let restored = restored_entry(stored);

        let Entry::ToolCall {
            kind, locations, ..
        } = restored
        else {
            panic!("a stored tool call restores as one");
        };
        assert_eq!(kind, "Read", "the card's kind must survive the round trip");
        assert_eq!(
            locations,
            vec![ToolCallLocationInfo {
                path: PathBuf::from("src").join("main.rs"),
                line: Some(42),
            }],
            "the file the call touched must survive the round trip"
        );
    }

    /// A call's duration is the wall clock between its start and its first
    /// terminal status, and it survives a restart; a call restored from a
    /// database written before the field has none.
    #[gpui::test]
    async fn a_tool_call_measures_its_duration_and_persists_it(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));
        chat.update(cx, |chat, cx| {
            chat.handle_event(
                AcpEvent::ToolCallStarted {
                    id: "t1".into(),
                    title: "cargo test".into(),
                    status: "in_progress".into(),
                    kind: "Execute".into(),
                    content: vec![],
                    locations: vec![],
                    raw_input: None,
                    raw_output: None,
                },
                cx,
            );
            assert!(
                matches!(
                    chat.entries.last(),
                    Some(Entry::ToolCall {
                        duration_ms: None,
                        ..
                    })
                ),
                "no duration while the call runs"
            );
            chat.handle_event(
                AcpEvent::ToolCallCompleted {
                    id: "t1".into(),
                    status: "completed".into(),
                    kind: None,
                    content: None,
                    locations: None,
                    raw_input: None,
                    raw_output: None,
                },
                cx,
            );
        });
        let duration = chat.read_with(cx, |chat, _| match chat.entries.last() {
            Some(Entry::ToolCall { duration_ms, .. }) => *duration_ms,
            other => panic!("expected a tool call, got {other:?}"),
        });
        assert!(
            duration.is_some(),
            "a terminal status stores the elapsed time"
        );
        assert!(
            chat.read_with(cx, |chat, _| chat.tool_started.is_empty()),
            "the start is consumed once measured"
        );

        let persisted = chat.read_with(cx, |chat, _| persisted_entry(chat.entries.last().unwrap()));
        assert!(
            matches!(
                persisted,
                Some(ChatEntry::ToolCall {
                    duration_ms: Some(_),
                    ..
                })
            ),
            "the duration is written to the persisted entry"
        );
        let restored = restored_entry(ChatEntry::ToolCall {
            id: "old".into(),
            title: "Read".into(),
            status: "completed".into(),
            kind: Some("Read".into()),
            locations: vec![],
            duration_ms: None,
        });
        assert!(matches!(
            restored,
            Entry::ToolCall {
                duration_ms: None,
                ..
            }
        ));
    }

    /// A row written before #168 carries neither field and must restore
    /// exactly as it always did, rather than failing to deserialize.
    #[test]
    fn a_tool_call_stored_before_the_fields_existed_still_restores() {
        let legacy: ChatEntry = serde_json::from_str(
            r#"{"ToolCall":{"id":"tool-1","title":"read","status":"Completed"}}"#,
        )
        .expect("a pre-#168 row still deserializes");

        let Entry::ToolCall {
            kind, locations, ..
        } = restored_entry(legacy)
        else {
            panic!("a stored tool call restores as one");
        };
        assert_eq!(kind, "tool", "the generic label it always showed");
        assert!(locations.is_empty(), "and no target to name");
    }

    /// A thought measures the wall clock from its first chunk to the entry
    /// that settles it, and the duration survives a restart; a thought
    /// restored from a database written before the field has none.
    #[gpui::test]
    async fn a_thought_measures_its_duration_and_persists_it(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(AcpEvent::ThoughtChunk("weigh the options".into()), cx);
            assert!(
                matches!(
                    chat.entries.last(),
                    Some(Entry::Thought {
                        started: Some(_),
                        duration_ms: None,
                        ..
                    })
                ),
                "the first chunk starts the clock"
            );
            assert!(chat.thought_is_streaming(chat.entries.len() - 1));
            chat.handle_event(
                AcpEvent::AgentMessageChunk("Here is the answer.".into()),
                cx,
            );
        });
        chat.read_with(cx, |chat, _| {
            let thought = chat
                .entries
                .iter()
                .find(|entry| matches!(entry, Entry::Thought { .. }))
                .expect("the thought is still there");
            assert!(
                matches!(
                    thought,
                    Entry::Thought {
                        duration_ms: Some(_),
                        ..
                    }
                ),
                "the answer's first chunk settles the thought: {thought:?}"
            );
            assert!(!chat.thought_is_streaming(0));
            assert!(
                matches!(
                    persisted_entry(thought),
                    Some(ChatEntry::Thought {
                        duration_ms: Some(_),
                        ..
                    })
                ),
                "the duration is written to the persisted entry"
            );
        });
        let restored = restored_entry(ChatEntry::Thought {
            text: "old".into(),
            duration_ms: None,
        });
        assert!(matches!(
            restored,
            Entry::Thought {
                started: None,
                duration_ms: None,
                ..
            }
        ));
    }

    /// The turn's end settles a thought that no answer followed.
    #[gpui::test]
    async fn a_turn_end_settles_a_trailing_thought(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(AcpEvent::ThoughtChunk("…".into()), cx);
            chat.handle_event(
                AcpEvent::TurnEnded {
                    stop_reason: "end_turn".into(),
                },
                cx,
            );
        });
        chat.read_with(cx, |chat, _| {
            assert!(
                chat.entries.iter().any(|entry| matches!(
                    entry,
                    Entry::Thought {
                        duration_ms: Some(_),
                        ..
                    }
                )),
                "the trailing thought settled on turn end"
            );
        });
    }

    /// A pre-field row restores with no duration rather than failing to
    /// deserialize.
    #[test]
    fn a_thought_row_written_before_the_duration_field_restores() {
        let json = r#"{"Thought":{"text":"hmm"}}"#;
        let entry: ChatEntry = serde_json::from_str(json).expect("deserializes");
        assert!(matches!(
            entry,
            ChatEntry::Thought {
                duration_ms: None,
                ..
            }
        ));
    }

    /// The body follows the run until the reader presses the header: open
    /// while the thought streams, folded once it settles, and the reader's
    /// press holds from then on — `widgets::Takeover`.
    #[gpui::test]
    async fn a_live_thought_opens_while_streaming_folds_on_settle_and_obeys_a_press(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));
        cx.update(|_window, cx| init(cx));
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(
                AcpEvent::ThoughtChunk("first, look at the tests".into()),
                cx,
            );
        });
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("thought-streaming-0").is_some(),
            "the header shows the orb"
        );
        assert!(
            cx.debug_bounds("thought-body-0").is_some(),
            "a live thought is open"
        );

        chat.update(cx, |chat, cx| {
            chat.handle_event(AcpEvent::AgentMessageChunk("Done.".into()), cx);
        });
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("thought-settled-0").is_some(),
            "the header shows the chevron"
        );
        assert!(
            cx.debug_bounds("thought-took-0").is_some(),
            "a measured thought says how long"
        );
        assert!(
            cx.debug_bounds("thought-body-0").is_none(),
            "a settled thought folds"
        );

        let header = cx.debug_bounds("thought-toggle-0").expect("header");
        cx.simulate_click(header.center(), Modifiers::none());
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("thought-body-0").is_some(),
            "a press opens it"
        );

        chat.update(cx, |chat, cx| {
            chat.handle_event(
                AcpEvent::TurnEnded {
                    stop_reason: "end_turn".into(),
                },
                cx,
            );
        });
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("thought-body-0").is_some(),
            "the reader's choice holds across later events"
        );
    }

    /// A restored thought opens closed and its header carries no clock.
    #[gpui::test]
    async fn a_restored_thought_opens_closed(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.push_entry(restored_entry(ChatEntry::Thought {
                text: "old reasoning".into(),
                duration_ms: None,
            }));
            chat
        });
        cx.update(|_window, cx| init(cx));
        refresh_frame(cx);
        assert!(cx.debug_bounds("thought-settled-0").is_some());
        assert!(
            cx.debug_bounds("thought-took-0").is_none(),
            "no clock to offer"
        );
        assert!(cx.debug_bounds("thought-body-0").is_none());
    }

    /// An open body is the gallery's reasoning box: a capped scrolling well
    /// with the fade strip along its top and the follow pin + scrollbar laid
    /// over it. A closed thought draws none of it.
    #[gpui::test]
    async fn an_open_thought_body_is_a_capped_well_with_a_fade_strip(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let long: String = (0..60).map(|i| format!("line {i}\n")).collect();
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.push_entry(Entry::Thought {
                text: long,
                open: Default::default(),
                started: None,
                duration_ms: Some(2_000),
            });
            chat
        });
        cx.update(|_window, cx| init(cx));
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("thought-well-0").is_none(),
            "closed: no well"
        );
        assert!(
            cx.debug_bounds("thought-fade-0").is_none(),
            "closed: no fade"
        );

        let header = cx.debug_bounds("thought-toggle-0").expect("header");
        cx.simulate_click(header.center(), Modifiers::none());
        refresh_frame(cx);
        let body = cx.debug_bounds("thought-body-0").expect("open: the body");
        let well = cx.debug_bounds("thought-well-0").expect("open: the well");
        let fade = cx
            .debug_bounds("thought-fade-0")
            .expect("open: the fade strip");
        assert!(
            well.size.height <= px(160.0),
            "the well is capped at 160: {well:?}"
        );
        assert_eq!(fade.size.height, px(20.0));
        assert_eq!(
            fade.top(),
            well.top(),
            "the strip sits on the well's top edge"
        );
        assert!(
            body.left() < well.left(),
            "the well is inset from the border line"
        );
        chat.read_with(cx, |chat, _| {
            assert!(
                chat.thought_scroll.contains_key(&0),
                "scroll state was created on first draw"
            );
        });
    }

    /// #173: a user message longer than the pane must wrap inside it. The
    /// bubble is end-justified, so when it refuses to shrink below its
    /// content width the overflow goes off the *left* edge — off screen,
    /// with nothing to scroll it back. `max_w(USER_PILL_MAX_WIDTH)` does not
    /// save it: the pane is already narrower than that cap.
    #[gpui::test]
    async fn a_long_user_message_wraps_inside_a_narrow_pane(cx: &mut TestAppContext) {
        let message = "Count slowly from 1 to 40, one number per line, nothing else, \r
                       and do not stop until you reach the very end of the list."
            .to_owned();
        let (chat, cx) = chat_view(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.push_entry(Entry::User {
                text: message,
                at: None,
            });
            cx.notify();
        });
        cx.simulate_resize(size(px(420.0), px(600.0)));
        refresh_frame(cx);

        let bubble = (0..8)
            .find_map(|index| {
                let selector: &'static str =
                    Box::leak(format!("user-bubble-{index}").into_boxed_str());
                cx.debug_bounds(selector)
            })
            .expect("the user's message is drawn");
        assert!(
            bubble.left() >= px(0.0),
            "the bubble must wrap inside the pane rather than spill off its \r
             left edge, where nothing can scroll to it: {bubble:?}"
        );
        assert!(
            bubble.size.width <= px(420.0),
            "the bubble must be no wider than the pane it lives in: {bubble:?}"
        );
    }

    /// The turn rail: one tick per user message along the transcript's left
    /// edge, and a click on a tick scrolls the list to that message.
    #[gpui::test]
    async fn the_turn_rail_draws_a_tick_per_user_message_and_jumps_on_click(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = chat_view(cx, &[]);
        chat.update(cx, |chat, cx| {
            for turn in 0..3 {
                chat.push_entry(Entry::User {
                    text: format!("question {turn}"),
                    at: None,
                });
                // Tall enough that three turns overflow a 300px pane, so a
                // jump has somewhere to go.
                let body = (0..12).map(|_| "line").collect::<Vec<_>>().join("\n\n");
                chat.push_entry(Entry::Assistant {
                    document: parse_chat_markdown(&body),
                    text: body,
                });
            }
            cx.notify();
        });
        cx.simulate_resize(size(px(600.0), px(300.0)));
        refresh_frame(cx);

        let transcript = cx.debug_bounds("chat-transcript").expect("transcript");
        let ticks: Vec<_> = (0..3)
            .map(|index| {
                let selector: &'static str =
                    Box::leak(format!("turn-tick-{index}").into_boxed_str());
                cx.debug_bounds(selector)
                    .unwrap_or_else(|| panic!("tick {index} is drawn"))
            })
            .collect();
        assert!(
            cx.debug_bounds("turn-tick-3").is_none(),
            "no tick beyond the last user message"
        );
        for tick in &ticks {
            assert!(
                tick.right() <= transcript.left() + px(24.0),
                "ticks sit in the left margin, not over the prose: {tick:?} vs {transcript:?}"
            );
        }
        assert!(
            ticks[0].top() < ticks[1].top() && ticks[1].top() < ticks[2].top(),
            "ticks follow transcript order top to bottom"
        );

        // The fixture opens with a status entry of its own, so the target
        // index is read off the entries rather than assumed.
        let second_user = chat.read_with(cx, |chat, _| {
            chat.entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| matches!(entry, Entry::User { .. }))
                .nth(1)
                .map(|(index, _)| index)
                .expect("three user messages were pushed")
        });
        cx.simulate_click(ticks[1].center(), Modifiers::none());
        refresh_frame(cx);
        chat.read_with(cx, |chat, _| {
            assert_eq!(
                chat.list_state.logical_scroll_top().item_ix,
                second_user,
                "the second tick scrolls the list to the second user message"
            );
        });
    }

    /// Three tall turns for a 300px pane: enough to overflow the transcript.
    fn push_overflowing_turns(chat: &gpui::Entity<Chat>, cx: &mut VisualTestContext) {
        chat.update(cx, |chat, cx| {
            for turn in 0..3 {
                chat.push_entry(Entry::User {
                    text: format!("question {turn}"),
                    at: None,
                });
                let body = (0..12).map(|_| "line").collect::<Vec<_>>().join("\n\n");
                chat.push_entry(Entry::Assistant {
                    document: parse_chat_markdown(&body),
                    text: body,
                });
            }
            cx.notify();
        });
    }

    /// The transcript's scrollbar: bezel's bar geometry laid over the list.
    /// Nothing is drawn while the content fits; once it overflows the thumb
    /// hugs the root's right edge and travels with the list.
    #[gpui::test]
    async fn the_transcript_scrollbar_appears_on_overflow_and_tracks_the_list(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = chat_view(cx, &[]);
        // Tall enough that the fixture's opening status entry fits.
        cx.simulate_resize(size(px(600.0), px(700.0)));
        refresh_frame(cx);
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("chat-transcript-thumb").is_none(),
            "nothing to scroll: no thumb"
        );

        cx.simulate_resize(size(px(600.0), px(300.0)));
        push_overflowing_turns(&chat, cx);
        // The bar reads the list's geometry as the last frame left it.
        refresh_frame(cx);
        refresh_frame(cx);
        let root = cx.debug_bounds("chat-root").expect("root");
        let viewport = chat.read_with(cx, |chat, _| chat.list_state.viewport_bounds());
        let thumb = cx
            .debug_bounds("chat-transcript-thumb")
            .expect("overflow: the thumb is drawn");
        assert!(
            thumb.right() <= root.right() && thumb.right() >= root.right() - px(12.0),
            "the thumb sits on the root's right edge: {thumb:?} vs {root:?}"
        );
        assert!(
            thumb.size.height < viewport.size.height,
            "the thumb is shorter than the viewport it reports on: {thumb:?} vs {viewport:?}"
        );
        assert!(
            thumb.top() >= viewport.top() && thumb.bottom() <= viewport.bottom() + px(1.0),
            "the track spans the list's viewport: {thumb:?} vs {viewport:?}"
        );

        // Following the tail, the thumb rests at the bottom; scrolling the
        // list back to its first row moves the thumb up.
        let at_tail = thumb.top();
        chat.update(cx, |chat, cx| {
            chat.list_state.scroll_to(gpui::ListOffset {
                item_ix: 0,
                offset_in_item: px(0.0),
            });
            cx.notify();
        });
        refresh_frame(cx);
        refresh_frame(cx);
        let at_top = cx
            .debug_bounds("chat-transcript-thumb")
            .expect("still overflowing")
            .top();
        assert!(
            at_top < at_tail,
            "the thumb travels with the list: tail {at_tail:?}, top {at_top:?}"
        );
    }

    /// The jump-to-latest disc: nothing while the list follows its tail,
    /// a centred disc over the transcript's bottom edge once the reader
    /// scrolls away, and a click that re-pins the transcript.
    #[gpui::test]
    async fn the_jump_to_latest_disc_appears_when_the_reader_leaves_the_tail(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = chat_view(cx, &[]);
        cx.simulate_resize(size(px(600.0), px(300.0)));
        push_overflowing_turns(&chat, cx);
        refresh_frame(cx);
        refresh_frame(cx);
        chat.read_with(cx, |chat, _| {
            assert!(
                chat.list_state.is_following_tail(),
                "a fresh push follows the tail"
            );
        });
        assert!(
            cx.debug_bounds("chat-jump-latest").is_none(),
            "following the tail: no disc"
        );

        chat.update(cx, |chat, cx| {
            chat.list_state.scroll_to(gpui::ListOffset {
                item_ix: 0,
                offset_in_item: px(0.0),
            });
            cx.notify();
        });
        refresh_frame(cx);
        let disc = cx
            .debug_bounds("chat-jump-latest")
            .expect("scrolled away: the disc is drawn");
        let transcript = cx.debug_bounds("chat-transcript").expect("transcript");
        assert!(
            (disc.center().x - transcript.center().x).abs() <= px(1.0),
            "the disc is centred on the transcript: {disc:?} vs {transcript:?}"
        );
        assert!(
            disc.bottom() <= transcript.bottom() && disc.top() >= transcript.top(),
            "the disc floats inside the transcript's bottom edge: {disc:?} vs {transcript:?}"
        );

        cx.simulate_click(disc.center(), Modifiers::none());
        refresh_frame(cx);
        refresh_frame(cx);
        chat.read_with(cx, |chat, _| {
            assert!(
                chat.list_state.is_following_tail(),
                "the click re-pins the transcript to its tail"
            );
        });
        assert!(
            cx.debug_bounds("chat-jump-latest").is_none(),
            "re-pinned: the disc is gone"
        );
    }

    /// #159: nothing is drawn between the composer card and the bottom of
    /// the pane. A centred caption naming the agent's working directory used
    /// to sit there, duplicating what the worktree selection already says.
    /// Measured rather than assumed: the card now ends 18px above the pane's
    /// bottom edge, which is the container's own padding; the caption added
    /// its 8px margin and a caption2 line on top of that, so it pushed the
    /// card roughly 22px higher.
    #[gpui::test]
    async fn nothing_is_drawn_below_the_composer(cx: &mut TestAppContext) {
        const PANE_HEIGHT: f32 = 600.0;
        let (_chat, cx) = chat_view(cx, &[]);
        cx.simulate_resize(size(px(900.0), px(PANE_HEIGHT)));
        refresh_frame(cx);

        let card = cx.debug_bounds("composer").expect("the composer is drawn");
        let below = px(PANE_HEIGHT) - card.bottom();
        assert!(
            below <= px(20.0),
            "only the container's own padding may sit below the composer, but \r
             {below:?} does — something is being drawn under the card again: \r
             card={card:?}"
        );
    }

    #[gpui::test]
    async fn narrow_composer_placeholder_stays_inside_composer_card(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        chat.update(cx, |chat, cx| {
            chat.set_agent_name("OpenCode");
            chat.available_commands.push(AvailableCommandInfo {
                name: "help".into(),
                description: "Show help".into(),
            });
            cx.notify();
        });
        cx.simulate_resize(size(px(595.0), px(600.0)));
        refresh_frame(cx);

        let card = cx
            .debug_bounds("composer")
            .expect("the composer card is drawn");
        let input = cx
            .debug_bounds("composer-input")
            .expect("the composer input is drawn");
        assert!(
            input.left() >= card.left() && input.right() <= card.right(),
            "the field must stay inside the composer card: card={card:?} input={input:?}"
        );
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer_placeholder()),
            chat.read_with(&cx.cx, |chat, _| chat.default_placeholder()),
            "the default placeholder is what the field shows"
        );
    }

    /// At Sirio's real pane width the chip row degrades by wrapping chip
    /// by chip — never by clipping a chip at the pane's edge — and every
    /// essential control stays drawn and reachable inside the card.
    #[gpui::test]
    async fn narrow_control_row_keeps_every_essential_control_reachable(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        chat.update(cx, |chat, _| configure_test_chat(chat));
        chat.update(cx, |chat, _| {
            chat.effort = Some(EffortOption {
                option_id: "effort".into(),
                name: Some("Effort".into()),
                current_value: Some("xhigh".into()),
                choices: vec![EffortChoice {
                    value: "xhigh".into(),
                    name: "Xhigh".into(),
                }],
            });
        });
        // 320 px: narrow enough that the old above-the-card toolbar
        // overflows a chip past its edge (the app's wider labels hit the
        // same wall at ~380 px); the chip row in the card must wrap chip
        // by chip instead.
        cx.simulate_resize(size(px(320.0), px(600.0)));
        refresh_frame(cx);

        let card = cx.debug_bounds("composer").expect("card");
        let send = cx.debug_bounds("send").expect("send");
        let attach = cx.debug_bounds("attach-image").expect("attach");
        let overflow = cx.debug_bounds("composer-overflow").expect("overflow");
        let context = cx.debug_bounds("context-ring").expect("context ring");
        let effort = cx.debug_bounds("effort-chip").expect("effort chip");
        let chip = cx.debug_bounds("model-chip").expect("model chip");
        // Chip by chip, nothing is clipped by the pane: every control
        // stays inside the card's own edges.
        for (name, bounds) in [
            ("model chip", chip),
            ("effort chip", effort),
            ("context", context),
            ("attach", attach),
            ("overflow", overflow),
            ("send", send),
        ] {
            assert!(
                bounds.left() >= card.left() && bounds.right() <= card.right(),
                "{name} is fully inside the card: {name}={bounds:?} card={card:?}"
            );
        }
        assert!(
            send.right() <= card.right(),
            "the send disc stays inside the card: send={send:?} card={card:?}"
        );
        assert!(
            send.left() >= attach.left(),
            "the trio is one run, never painted over a neighbour: attach={attach:?} send={send:?}"
        );
        // Reading order, wrap-aware: effort sits on the model's line to
        // its right, or wrapped onto a later line — at 320 px line 1 holds
        // pill + model at full width and effort correctly drops to line 2.
        assert!(
            effort.top() > chip.top() || effort.left() >= chip.right(),
            "model precedes effort in layout order: {chip:?} {effort:?}"
        );
    }

    async fn narrow_composer_stays_inside_chat_pane_and_keeps_send_reachable(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        cx.simulate_resize(size(px(595.0), px(600.0)));
        refresh_frame(cx);

        let pane = cx
            .debug_bounds("chat-root")
            .expect("the chat pane is drawn");
        let card = cx
            .debug_bounds("composer")
            .expect("the composer card is drawn");
        let send = cx.debug_bounds("send").expect("the send control is drawn");
        assert!(
            card.left() >= pane.left(),
            "the composer must not overflow the pane's left edge: pane={pane:?} card={card:?}"
        );
        assert!(
            card.right() <= pane.right(),
            "the composer must not overflow the pane's right edge: pane={pane:?} card={card:?}"
        );
        assert!(
            send.left() >= card.left() && send.right() <= card.right(),
            "the send control must be fully inside the composer: card={card:?} send={send:?}"
        );
    }

    #[gpui::test]
    async fn wide_composer_remains_capped_at_transcript_maximum(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        cx.simulate_resize(size(px(1140.0), px(600.0)));
        refresh_frame(cx);

        let card = cx
            .debug_bounds("composer")
            .expect("the composer card is drawn");
        assert_eq!(
            card.size.width,
            px(TRANSCRIPT_WIDTH),
            "the composer stays capped below a wider pane: card={card:?}"
        );
        // The card sits where the transcript sits: centred in a wide pane.
        let window_center_x = 1140.0 / 2.0;
        assert!(
            (card.center().x.as_f32() - window_center_x).abs() <= 1.0,
            "the composer card is centred with the transcript: card={card:?}"
        );
    }

    #[gpui::test]
    async fn bezel_markdown_renders_lists_tasks_code_and_tables(cx: &mut TestAppContext) {
        let source = r#"1. first
2. second

- [x] done
- [ ] pending

```rust
let answer = 42;
```

| left | right |
| --- | --- |
| one | two |"#;
        let document = parse_chat_markdown(source);
        assert!(
            document
                .blocks
                .iter()
                .any(|block| matches!(block.kind, markdown::BlockKind::Ordered { .. })),
            "ordered lists stay structured in the bezel Doc"
        );
        assert!(
            document
                .blocks
                .iter()
                .any(|block| matches!(block.kind, markdown::BlockKind::Task { .. })),
            "task lists stay structured in the bezel Doc"
        );
        assert!(
            document
                .blocks
                .iter()
                .any(|block| matches!(block.kind, markdown::BlockKind::Code { .. })),
            "fenced code stays structured in the bezel Doc"
        );
        assert!(
            document
                .blocks
                .iter()
                .any(|block| matches!(block.kind, markdown::BlockKind::Table { .. })),
            "tables stay structured in the bezel Doc"
        );

        let mut cx = markdown_view(cx, source.into());
        refresh_frame(&mut cx);
        assert!(
            cx.debug_bounds("assistant-response-0").is_some(),
            "the mixed markdown document renders as an assistant response"
        );
    }

    #[test]
    fn persisted_transcript_contains_only_completed_turns() {
        let entries = vec![
            Entry::User {
                text: "inspect".into(),
                at: None,
            },
            Entry::Assistant {
                text: "done".into(),
                document: parse_chat_markdown("done"),
            },
            Entry::TurnFooter("12:00".into()),
            Entry::User {
                text: "still streaming".into(),
                at: None,
            },
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
            chat.read_with(&cx.cx, |chat, _| chat.draft_text()),
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
            chat.read_with(&cx.cx, |chat, _| chat.draft_text()),
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
                    .any(|entry| matches!(entry, Entry::User { text, .. } if text == "first"))
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
            chat.read_with(&cx.cx, |chat, _| chat.draft_text()),
            "line one\nline two",
            "Shift+Return inserts a newline into the composer"
        );
        let user_entries = chat.read_with(&cx.cx, |chat, _| {
            chat.entries
                .iter()
                .filter(|entry| matches!(entry, Entry::User { .. }))
                .count()
        });
        assert_eq!(
            user_entries, 1,
            "Shift+Return must not send a second message"
        );

        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(
                |entry| matches!(entry, Entry::User { text, .. } if text == "line one\nline two"),
            ) && chat.has_completed_turn
        });
    }

    /// F-CORE-DOM-07: before this pass `ChatEvent` had exactly two variants
    /// (`OpenFile`, `OpenLink`) and `AcpEvent::TurnEnded`'s handler -- which
    /// already flips `has_completed_turn`, pushes the footer, and persists
    /// the transcript -- never told an outside subscriber a turn had
    /// settled. That is the half of the wiring gap that lives in this
    /// crate: the workspace's `request_auto_rename` (sirio/src/main.rs)
    /// is fully ported and unit-tested, but nothing could ever call it for
    /// a chat pane because no event existed to call it *from*. This drives
    /// a real turn through the real `chat_fixture.py` ACP agent -- the same
    /// production path every other test in this file uses, not a
    /// hand-constructed `AcpEvent` -- and asserts an outside subscriber
    /// (standing in for `SirioWorkspace::bind_chat`) observes
    /// `ChatEvent::TurnEnded` by the time `has_completed_turn` flips.
    #[gpui::test]
    async fn a_completed_turn_emits_chat_event_turn_ended(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());

        let turn_ended = Rc::new(RefCell::new(false));
        let turn_ended_write = turn_ended.clone();
        cx.update(|_, app_cx| {
            app_cx
                .subscribe(&chat, move |_chat, event: &ChatEvent, _cx| {
                    if matches!(event, ChatEvent::TurnEnded) {
                        *turn_ended_write.borrow_mut() = true;
                    }
                })
                .detach();
        });

        focus_and_type(cx, "first");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| chat.has_completed_turn);

        assert!(
            *turn_ended.borrow(),
            "AcpEvent::TurnEnded must also emit ChatEvent::TurnEnded -- on the \
             unfixed tree this assertion is the failure: has_completed_turn flips \
             true (the turn genuinely ended) but no ChatEvent ever reaches a \
             subscriber, which is exactly why request_auto_rename was structurally \
             unreachable from a real chat turn"
        );
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
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::Assistant {
                text: "copy this assistant response".into(),
                document: parse_chat_markdown("copy this assistant response"),
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
        cx.update(bezel::ui::input::init);
        cx.update(init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::Assistant {
                text: "```rust\nlet answer = 42;\n```".into(),
                document: parse_chat_markdown("```rust\nlet answer = 42;\n```"),
            });
            chat
        });
        refresh_frame(cx);

        let response = cx
            .debug_bounds("assistant-response-0")
            .expect("the fenced code response is drawn");
        // bezel-markdown's native copy control is positioned 5px from the
        // block's right edge and 3px from its top. Its stable element id is
        // intentionally not a GPUI debug selector, so exercise its actual hit
        // target relative to the single-block response rather than duplicating
        // the control in Sirio just for test instrumentation.
        cx.simulate_click(
            point(response.right() - px(24.0), response.top() + px(13.0)),
            Modifiers::none(),
        );
        cx.run_until_parked();

        assert_eq!(
            cx.cx.read_from_clipboard().and_then(|item| item.text()),
            Some("let answer = 42;".into()),
            "code-block Copy writes raw code, without the fence or language label"
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
            ["config", "user.name", "Sirio Test"].as_slice(),
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
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
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
                expanded: true,
                duration_ms: None,
            });
            chat.push_entry(Entry::Assistant {
                text: "done".into(),
                document: parse_chat_markdown("done"),
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
                expanded: true,
                duration_ms: None,
            });
            // The expanded rows live inside the zone; the turn streams so
            // it opens by itself.
            chat.streaming = true;
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
        // The task is its own run box, and its header row sits inside it.
        let run = cx
            .debug_bounds("tool-run-1")
            .expect("the task is a run box");
        let task_toggle = cx
            .debug_bounds("subagent-task-toggle-1")
            .expect("subagent task toggle");
        assert!(
            run.left() <= task_toggle.left()
                && task_toggle.right() <= run.right()
                && run.top() <= task_toggle.top()
                && task_toggle.bottom() <= run.bottom(),
            "the task header sits inside its run box: run={run:?} toggle={task_toggle:?}"
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
        let child = cx
            .debug_bounds("subagent-tool-call-toggle-1-0")
            .expect("expanding the task reveals its nested tool call");
        assert!(
            child.left() > task_toggle.left(),
            "a nested call is indented under the task header: child={child:?} header={task_toggle:?}"
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

    /// F-CHAT-23 regression: a finish-line critic reported that dismissing
    /// an unrenderable permission wipes the *entire* transcript to an empty
    /// array, not just the dismissed entry — reproduced (per the critic)
    /// with prior turns already present. This drives exactly that shape:
    /// one turn dismissed and settled, then a second turn's unrenderable
    /// permission dismissed by a real simulated click, and asserts the
    /// first turn's three entries are still there afterwards.
    #[gpui::test]
    async fn dismissing_a_later_permission_does_not_wipe_earlier_turns(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["permission-unrenderable"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        // Turn 1: send, wait for its unrenderable permission, dismiss it.
        focus_and_type(cx, "first");
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
        let dismiss1 = cx
            .debug_bounds("permission-dismiss-1")
            .expect("first dismiss control");
        cx.simulate_click(dismiss1.center(), Modifiers::none());
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| chat.has_completed_turn && !chat.streaming);

        let entries_after_turn_one = chat.read_with(&*cx, |chat, _| chat.entries.len());
        assert_eq!(
            entries_after_turn_one, 3,
            "turn 1 should have settled to user + dismissed-permission + turn-footer"
        );

        // Turn 2: send again, wait for its own unrenderable permission,
        // dismiss THAT one, and check turn 1's entries are still present.
        refresh_frame(cx);
        focus_and_type(cx, "second");
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
        let dismiss2 = cx
            .debug_bounds("permission-dismiss-2")
            .expect("second dismiss control");
        cx.simulate_click(dismiss2.center(), Modifiers::none());
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| chat.has_completed_turn && !chat.streaming);

        chat.read_with(&cx.cx, |chat, _| {
            assert_eq!(
                chat.entries.len(),
                6,
                "dismissing turn 2's permission must not touch turn 1's three \
                 entries: got {:?}",
                chat.entries
            );
            assert!(
                matches!(chat.entries.first(), Some(Entry::User { text, .. }) if text == "first"),
                "turn 1's user message must survive turn 2's dismiss: {:?}",
                chat.entries.first()
            );
        });
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
            }) && chat
                .entries
                .iter()
                .filter(|entry| matches!(entry, Entry::TurnFooter(_)))
                .count()
                == 2
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

        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer_placeholder()),
            "Waiting for permission response…".to_string(),
            "the permission-wait placeholder replaces the ordinary queue placeholder"
        );

        let entries_before = chat.read_with(&*cx, |chat, _| chat.entries.len());
        focus_and_type(cx, "should not appear");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        refresh_frame(cx);

        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.draft.trim().is_empty()
                && chat.attachments.is_empty()),
            "the disabled editor must refuse typed characters entirely"
        );
        assert_eq!(
            chat.read_with(&*cx, |chat, _| chat.entries.len()),
            entries_before,
            "Enter must not send or queue while a permission is pending"
        );
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer_placeholder()),
            "Waiting for permission response…".to_string(),
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
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            Chat::from_test_command(
                AgentCommand::new("/definitely/missing/sirio-acp-agent"),
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

        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer_placeholder()),
            "Agent offline — reconnecting when you send…".to_string(),
            "an empty, disconnected composer must name the offline state"
        );

        let entries_before = chat.read_with(cx, |chat, _| chat.entries.len());
        focus_and_type(cx, "should not appear");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        refresh_frame(cx);

        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.draft.trim().is_empty()
                && chat.attachments.is_empty()),
            "the disabled editor must refuse typed characters entirely while offline"
        );
        assert_eq!(
            chat.read_with(&*cx, |chat, _| chat.entries.len()),
            entries_before,
            "Enter must neither send nor start a fresh reconnect attempt while offline"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| !chat.connecting),
            "a blocked Enter must not itself trigger a new connection attempt"
        );
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer_placeholder()),
            "Agent offline — reconnecting when you send…".to_string(),
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
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            Chat::from_test_command(
                AgentCommand::new("/definitely/missing/sirio-acp-agent"),
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

        chat.update(cx, |chat, cx| {
            chat.set_composer_text("hello offline test", cx);
        });
        refresh_frame(cx);
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.draft_text()),
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
            chat.read_with(&cx.cx, |chat, _| chat.draft_text()),
            "hello offline test",
            "a disabled offline composer must never silently discard or extend the user's draft"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.client.is_none() && !chat.connecting),
            "a blocked Enter must not itself start a new connection attempt"
        );
    }

    /// F-CHAT-25: the "listed option" arm -- a structured question with
    /// populated wire options renders clickable pills instead of a
    /// free-text field; clicking one records the choice on the card,
    /// clears the pending bar, and the agent receives the chosen option id.
    /// Sibling of `a_text_answer_leaves_the_surface_and_clears_the_pending_bar`
    /// below, which drives the text-field arm; this is the arm that was
    /// never re-driven when a later pass touched an unrelated part of the
    /// row.
    #[gpui::test]
    async fn a_listed_option_leaves_the_surface_and_clears_the_pending_bar(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = chat_view(cx, &["question-options"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "which color?");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        // The question is pending: option pills are drawn, not a text
        // field, and the pending bar names the asker.
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
            cx.debug_bounds("question-answer-input").is_none(),
            "populated wire options must render pills, not the text field"
        );
        assert!(
            cx.debug_bounds("permission-option-blue").is_some()
                && cx.debug_bounds("permission-option-green").is_some(),
            "both listed options are drawn as clickable pills"
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

        // Click the Blue pill.
        let blue = cx
            .debug_bounds("permission-option-blue")
            .expect("the Blue pill is drawn");
        cx.simulate_click(blue.center(), Modifiers::none());
        cx.run_until_parked();

        // The choice left the surface: the card records it, the pending
        // bar is gone, and the agent received it (echoed back in the turn).
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
            "clicking a listed option clears the pending state"
        );
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("pending-question-bar").is_none(),
            "the pending-question bar is gone after the click"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| {
                chat.entries.iter().any(|entry| {
                    matches!(
                        entry,
                        Entry::Assistant { text, .. } if text.contains("You picked: blue")
                    )
                })
            }),
            "the agent received the clicked option's id and echoed it back"
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
                .any(|entry| matches!(entry, Entry::User { text, .. } if text == "still alive?"))
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
    /// is the agent's own process going away, not one request being
    /// rejected — the stated error card offers "Restart agent" (not the
    /// generic "Retry"), and clicking it reconnects and completes a later
    /// turn.
    #[gpui::test]
    async fn a_stream_that_dies_mid_reply_states_the_error_and_restart_recovers(
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
        // the death instead of looking like a normal empty reply — and
        // correctly, as the agent's process being gone (F-CHAT-03), not a
        // rejected request (`ErrorKind::Connection`).
        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(
                    entry,
                    Entry::Error {
                        retryable: true,
                        kind: ErrorKind::Disconnected,
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
        assert!(
            cx.debug_bounds("chat-retry").is_none(),
            "the agent's own process died, so this must not offer the \
             generic Retry"
        );
        let restart = cx
            .debug_bounds("chat-restart-agent")
            .expect("the drawn error card offers Restart agent");

        // Restart relaunches the agent; the second fixture invocation
        // behaves, so the disconnected card is cleared and a later turn
        // completes.
        cx.simulate_click(restart.center(), Modifiers::none());
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            chat.client.is_some()
                && !chat.entries.iter().any(|entry| {
                    matches!(
                        entry,
                        Entry::Error {
                            kind: ErrorKind::Disconnected,
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
                    .any(|entry| matches!(entry, Entry::User { text, .. } if text == "again"))
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
        assert!(cx.debug_bounds("stop-glyph").is_none());
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.composer_placeholder())
                != "Type to queue for the next turn…",
            "the idle composer is not queueing"
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
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.composer_placeholder()),
            "Type to queue for the next turn…".to_string(),
            "the empty composer shows the queue placeholder while streaming"
        );

        // Enter during the stream commits the queue; the transcript is not
        // touched by the commit itself.
        focus_and_type(cx, "queued msg");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        let entries = chat.read_with(&cx.cx, |chat, _| chat.entries.len());
        assert_eq!(
            queue_texts(&chat, cx),
            vec!["queued msg".to_string()],
            "Enter during a stream commits the draft as the queued entry"
        );
        assert_eq!(entries, 2, "queueing does not touch the transcript");
        refresh_frame(cx);
        let queue = cx.debug_bounds("queue").expect("the queue block is drawn");
        let card = cx
            .debug_bounds("composer")
            .expect("the composer card is drawn");
        assert!(
            queue.bottom() <= card.top(),
            "the queue sits above the composer card: queue={queue:?} card={card:?}"
        );
        assert!(
            cx.debug_bounds("queue-text-queued msg").is_some(),
            "the queued text is drawn in its entry"
        );
        assert!(
            cx.debug_bounds("queue-count-1").is_some(),
            "the header counts one queued message"
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
            chat.read_with(&cx.cx, |chat, _| chat.draft_text()),
            "half a thought",
            "the composer stays editable while streaming"
        );

        // The turn completes; the queued item sends exactly once as a
        // normal turn and is answered.
        std::fs::write(dir.0.join("go"), "go").expect("write go file");
        pump_chat_until(cx, &chat, |chat| {
            chat.entries
                .iter()
                .filter(|entry| matches!(entry, Entry::User { text, .. } if text == "queued msg"))
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
                    .filter(
                        |entry| matches!(entry, Entry::User { text, .. } if text == "queued msg"),
                    )
                    .count();
                let uncommitted_sent = chat.entries.iter().any(
                    |entry| matches!(entry, Entry::User { text, .. } if text == "half a thought"),
                );
                (
                    queued_count,
                    uncommitted_sent,
                    chat.draft_text(),
                    chat.queue.is_empty(),
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
            queue_texts(&chat, cx),
            vec!["queued msg".to_string()],
            "the ✕ test starts with the committed entry"
        );
        refresh_frame(cx);
        let remove = cx
            .debug_bounds("queue-remove-0")
            .expect("the entry's remove control is drawn");
        cx.simulate_click(remove.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.queue.is_empty()),
            "the ✕ removes the entry"
        );
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("queue").is_none(),
            "the queue block is gone once nothing is queued"
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
                    .all(|entry| !matches!(entry, Entry::User { text, .. } if text == "queued msg"))
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
            queue_texts(&chat, cx),
            vec!["queued msg".to_string()],
            "the stop-with-queue test starts with the committed entry"
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
                .filter(|entry| matches!(entry, Entry::User { text, .. } if text == "queued msg"))
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
                chat.queue.is_empty(),
                chat.has_completed_turn,
            )
        });
        assert!(cancelled_footer, "the cancelled turn's footer states it");
        assert!(queue_drained, "the queue drained into the send");
        assert!(completed, "the queued turn completes");
    }

    /// The queue's entries, front first, as the composer would send them.
    fn queue_texts(chat: &gpui::Entity<Chat>, cx: &VisualTestContext) -> Vec<String> {
        chat.read_with(&cx.cx, |chat, _| chat.queue.iter().cloned().collect())
    }

    /// How many user turns carrying exactly `text` the transcript holds.
    fn user_turn_count(chat: &Chat, text: &str) -> usize {
        chat.entries
            .iter()
            .filter(|entry| matches!(entry, Entry::User { text: sent, .. } if sent == text))
            .count()
    }

    /// The index of the first user turn carrying exactly `text`.
    fn user_turn_position(chat: &Chat, text: &str) -> Option<usize> {
        chat.entries
            .iter()
            .position(|entry| matches!(entry, Entry::User { text: sent, .. } if sent == text))
    }

    /// Sends "hello" against the `cancel` fixture and waits until its
    /// partial reply is streaming, with a fresh frame drawn.
    fn stream_partial_turn(cx: &mut VisualTestContext, chat: &gpui::Entity<Chat>) {
        pump_chat_until(cx, chat, |chat| chat.client.is_some());
        refresh_frame(cx);
        focus_and_type(cx, "hello");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        pump_chat_until(cx, chat, |chat| {
            chat.streaming
                && chat.entries.iter().any(
                    |entry| matches!(entry, Entry::Assistant { text, .. } if text == "partial "),
                )
        });
        refresh_frame(cx);
    }

    /// Types `text` and presses Enter while a turn streams, so it lands in
    /// the queue. Redraws first: each queued entry grows the block above
    /// the card, so the card's last-known bounds would be stale.
    fn queue_entry(cx: &mut VisualTestContext, text: &str) {
        refresh_frame(cx);
        focus_and_type(cx, text);
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
    }

    /// D-CHAT-03, FIFO half: a second Enter during the stream appends a
    /// second entry rather than replacing the first, both are drawn in
    /// order under a header that counts them, and each turn end drains
    /// exactly one entry, front first.
    #[gpui::test]
    async fn a_second_enter_appends_to_the_queue_and_turn_ends_drain_it_in_order(
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

        queue_entry(cx, "first q");
        queue_entry(cx, "second q");
        assert_eq!(
            queue_texts(&chat, cx),
            vec!["first q".to_string(), "second q".to_string()],
            "the second Enter appends behind the first"
        );
        refresh_frame(cx);
        let first = cx
            .debug_bounds("queue-entry-0")
            .expect("the first entry is drawn");
        let second = cx
            .debug_bounds("queue-entry-1")
            .expect("the second entry is drawn");
        assert!(
            first.bottom() <= second.top(),
            "entries are drawn in queue order: first={first:?} second={second:?}"
        );
        assert!(
            cx.debug_bounds("queue-count-2").is_some(),
            "the header counts two queued messages"
        );

        // The turn completes: the front entry sends, its turn end sends the
        // next, and the transcript holds each exactly once in queue order.
        std::fs::write(dir.0.join("go"), "go").expect("write go file");
        pump_chat_until(cx, &chat, |chat| {
            user_turn_count(chat, "second q") == 1 && chat.queue.is_empty() && !chat.streaming
        });
        let (first_count, first_at, second_at) = chat.read_with(&cx.cx, |chat, _| {
            (
                user_turn_count(chat, "first q"),
                user_turn_position(chat, "first q"),
                user_turn_position(chat, "second q"),
            )
        });
        assert_eq!(first_count, 1, "the front entry sends exactly once");
        assert!(
            first_at < second_at,
            "the front entry sends before the one behind it: {first_at:?} {second_at:?}"
        );
    }

    /// The ✕ on one entry removes only that entry; the others keep their
    /// place and their drawn rows renumber from the front.
    #[gpui::test]
    async fn removing_one_entry_keeps_the_others_queued(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["cancel"]);
        stream_partial_turn(cx, &chat);
        queue_entry(cx, "first q");
        queue_entry(cx, "second q");
        refresh_frame(cx);

        let remove = cx
            .debug_bounds("queue-remove-0")
            .expect("the front entry's remove control is drawn");
        cx.simulate_click(remove.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            queue_texts(&chat, cx),
            vec!["second q".to_string()],
            "only the removed entry leaves the queue"
        );
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("queue-text-second q").is_some(),
            "the surviving entry is still drawn"
        );
        assert!(
            cx.debug_bounds("queue-text-first q").is_none(),
            "the removed entry is gone"
        );
        assert!(
            cx.debug_bounds("queue-entry-1").is_none(),
            "the surviving entry moved up to the front row"
        );
    }

    /// "Clear all" in the header empties the queue and takes the block down.
    #[gpui::test]
    async fn clear_all_empties_the_queue(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["cancel"]);
        stream_partial_turn(cx, &chat);
        queue_entry(cx, "first q");
        queue_entry(cx, "second q");
        refresh_frame(cx);

        let clear = cx
            .debug_bounds("queue-clear")
            .expect("the clear-all control is drawn");
        cx.simulate_click(clear.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.queue.is_empty()),
            "clear all empties the queue"
        );
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("queue").is_none(),
            "the queue block is gone once nothing is queued"
        );
    }

    /// The header folds the entries away and unfolds them again; the count
    /// stays visible either way, so a folded queue is never a hidden one.
    #[gpui::test]
    async fn the_queue_header_folds_the_entries_and_keeps_the_count(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["cancel"]);
        stream_partial_turn(cx, &chat);
        queue_entry(cx, "first q");
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("queue-entry-0").is_some(),
            "the queue starts unfolded"
        );

        let toggle = cx
            .debug_bounds("queue-toggle")
            .expect("the header toggle is drawn");
        cx.simulate_click(toggle.center(), Modifiers::none());
        cx.run_until_parked();
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("queue-entry-0").is_none(),
            "folding hides the entries"
        );
        assert!(
            cx.debug_bounds("queue-count-1").is_some(),
            "the folded header still counts the entries"
        );
        assert_eq!(
            queue_texts(&chat, cx),
            vec!["first q".to_string()],
            "folding never touches the queue itself"
        );

        let toggle = cx
            .debug_bounds("queue-toggle")
            .expect("the header toggle is still drawn");
        cx.simulate_click(toggle.center(), Modifiers::none());
        cx.run_until_parked();
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("queue-entry-0").is_some(),
            "unfolding shows the entries again"
        );
    }

    /// "Send now" on a later entry jumps the queue: the running turn is
    /// cancelled, that entry sends as the redirect exactly once, and the
    /// entries ahead of it wait for the next turn end in their old order.
    #[gpui::test]
    async fn send_now_on_a_later_entry_cancels_the_turn_and_sends_that_entry_first(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = chat_view(cx, &["cancel"]);
        stream_partial_turn(cx, &chat);
        queue_entry(cx, "first q");
        queue_entry(cx, "second q");
        refresh_frame(cx);

        let send_now = cx
            .debug_bounds("queue-send-1")
            .expect("the second entry's send-now control is drawn");
        cx.simulate_click(send_now.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            !chat.read_with(&cx.cx, |chat, _| chat.streaming),
            "send now stops the running turn"
        );

        pump_chat_until(cx, &chat, |chat| {
            user_turn_count(chat, "second q") == 1
                && user_turn_count(chat, "first q") == 1
                && chat.queue.is_empty()
                && !chat.streaming
        });
        let (cancelled_at, second_at, first_at) = chat.read_with(&cx.cx, |chat, _| {
            let cancelled_at = chat.entries.iter().position(
                |entry| matches!(entry, Entry::TurnFooter(text) if text.contains("cancelled")),
            );
            (
                cancelled_at,
                user_turn_position(chat, "second q"),
                user_turn_position(chat, "first q"),
            )
        });
        assert!(
            cancelled_at < second_at,
            "the cancelled footer lands before the redirect: {cancelled_at:?} {second_at:?}"
        );
        assert!(
            second_at < first_at,
            "the sent-now entry precedes the one that was ahead of it: {second_at:?} {first_at:?}"
        );
    }

    /// "Send now" with no turn running sends the entry straight away — the
    /// case of a queue that outlived its turn because the transport died.
    #[gpui::test]
    async fn send_now_while_no_turn_runs_sends_the_entry_immediately(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        chat.update(cx, |chat, cx| {
            chat.queue.push_back("late".into());
            cx.notify();
        });
        refresh_frame(cx);

        let send_now = cx
            .debug_bounds("queue-send-0")
            .expect("the entry's send-now control is drawn");
        cx.simulate_click(send_now.center(), Modifiers::none());
        cx.run_until_parked();
        pump_chat_until(cx, &chat, |chat| {
            user_turn_count(chat, "late") == 1
                && chat
                    .entries
                    .iter()
                    .any(|entry| matches!(entry, Entry::Assistant { text, .. } if text == "reply "))
        });
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.queue.is_empty()),
            "the sent entry leaves the queue"
        );
    }

    /// The control snapshot keeps `queued_text` as the front entry for the
    /// existing `surface.chat.read` readers and exposes the whole queue
    /// beside it.
    #[gpui::test]
    async fn the_control_snapshot_exposes_the_whole_queue(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        chat.update(cx, |chat, _| {
            chat.queue.push_back("a".into());
            chat.queue.push_back("b".into());
        });
        let snapshot = chat.read_with(&cx.cx, |chat, _| chat.control_snapshot());
        assert_eq!(snapshot.queued_text, "a", "queued_text is the front entry");
        assert_eq!(
            snapshot.queued,
            vec!["a".to_string(), "b".to_string()],
            "queued is the whole queue, front first"
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
            ..Default::default()
        });
    }

    /// #206: the composer's agent badge must name the agent it is for.
    ///
    /// It rendered `selected_model_name` instead, whose fallback is a
    /// hardcoded "Claude Code", so a Codex chat badged itself "Claude Code"
    /// until Codex reported its models -- naming a *different* agent, not
    /// merely omitting one.
    #[gpui::test]
    fn the_agent_badge_names_the_chats_own_agent(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.set_agent_name("Codex");
            chat
        });

        assert_eq!(
            chat.read_with(cx, |chat, _| chat.agent_badge_name()),
            "Codex"
        );
    }

    /// #206: and when there is no agent to name, it must not invent one.
    ///
    /// This is the case the restored-chat banner is about -- "saved before
    /// Sirio recorded which agent it belonged to, so there is no way to
    /// tell which one to reopen it with" -- which the badge underneath was
    /// flatly contradicting by printing "Claude Code".
    #[gpui::test]
    fn an_unknown_agent_is_not_silently_named_claude_code(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));

        let badge = chat.read_with(cx, |chat, _| chat.agent_badge_name());
        assert_ne!(
            badge, "Claude Code",
            "a chat with no recorded agent must not name one -- the banner              above this badge says the agent is unknowable"
        );
        assert_eq!(badge, "Unknown agent");
    }

    #[gpui::test]
    fn default_placeholder_names_agent_without_commands(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.set_agent_name("OpenCode");
            chat
        });

        // The placeholder is the gallery's sentence; the agent's name lives
        // in the toolbar's pill and model chip, not here.
        assert_eq!(
            chat.read_with(cx, |chat, _| chat.default_placeholder()),
            "Ask anything, or @ to attach a file"
        );
    }

    #[gpui::test]
    fn default_placeholder_names_agent_commands(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.set_agent_name("OpenCode");
            chat.available_commands.push(AvailableCommandInfo {
                name: "help".into(),
                description: "Show help".into(),
            });
            chat
        });

        assert_eq!(
            chat.read_with(cx, |chat, _| chat.default_placeholder()),
            "Ask anything, / for commands, or @ to attach a file"
        );
    }

    #[gpui::test]
    fn default_placeholder_without_agent_keeps_file_affordance(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));

        let placeholder = chat.read_with(cx, |chat, _| chat.default_placeholder());
        assert!(placeholder.starts_with("Ask anything"));
        assert!(placeholder.contains("@ to attach a file"));
    }

    #[test]
    fn default_agent_cwd_follows_the_process_workspace() {
        assert_eq!(default_agent_cwd(), std::env::current_dir().unwrap());
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
                DiffLine::Context {
                    number: 1,
                    text: "one".into()
                },
                DiffLine::Removed {
                    number: 2,
                    text: "two".into()
                },
                DiffLine::Added {
                    number: 2,
                    text: "TWO".into()
                },
                DiffLine::Context {
                    number: 3,
                    text: "three".into()
                },
                DiffLine::Added {
                    number: 4,
                    text: "four".into()
                },
            ]
        );
    }

    #[test]
    fn diff_preview_lines_treats_a_missing_old_text_as_a_pure_addition() {
        let rows = diff_preview_lines(None, "brand new\n");
        assert_eq!(
            rows,
            vec![DiffLine::Added {
                number: 1,
                text: "brand new".into()
            }]
        );
    }

    /// F-CHAT-31: the numbers a reader uses to find the change in the file.
    /// A removed line carries its position in the *old* file and the added
    /// line replacing it carries its position in the *new* one — the same
    /// pair Swift's `ChatDiffPreviewModel.rows` assigns from `oldIndex` and
    /// `newIndex`, which is why both read `2` for a one-line replacement
    /// while a line inserted later shifts only the new side.
    #[test]
    fn diff_preview_lines_number_each_side_against_its_own_file() {
        let rows = diff_preview_lines(Some("a\nb\nc\n"), "a\nB\nc\nd\n");
        assert_eq!(
            rows.iter().map(DiffLine::number).collect::<Vec<_>>(),
            vec![1, 2, 2, 3, 4]
        );

        // A pure insertion in the middle: the new side advances past the old.
        let inserted = diff_preview_lines(Some("a\nc\n"), "a\nb\nc\n");
        assert_eq!(
            inserted
                .iter()
                .map(|line| (line.number(), line.text().to_string()))
                .collect::<Vec<_>>(),
            vec![
                (1, "a".to_string()),
                (2, "c".to_string()),
                (2, "b".to_string()),
                (3, "c".to_string()),
            ]
        );
    }

    pub(crate) fn test_tool_call(id: &str) -> Entry {
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
            duration_ms: None,
        }
    }

    /// A sent message carries the moment it was sent, the moment survives a
    /// restart as unix seconds, and a row written before the field restores
    /// with none.
    #[gpui::test]
    async fn a_user_message_is_stamped_and_the_stamp_persists(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));
        // `send` refuses while offline (no client here), so drive the exact
        // fn the send path uses to push the `User` entry.
        chat.update(cx, |chat, cx| {
            chat.submit_turn("hello".into(), Vec::new(), Vec::new(), cx);
        });
        chat.read_with(cx, |chat, _| {
            let user = chat
                .entries
                .iter()
                .find(|e| matches!(e, Entry::User { .. }))
                .expect("the user entry");
            let Entry::User { at, .. } = user else {
                unreachable!()
            };
            let at = at.as_ref().expect("a sent message is stamped");
            assert!(
                chrono::Local::now()
                    .signed_duration_since(*at)
                    .num_seconds()
                    .abs()
                    < 60
            );
            let persisted = persisted_entry(user).expect("persisted");
            assert!(matches!(
                persisted,
                ChatEntry::UserMessage { at: Some(_), .. }
            ));
        });
        let restored = restored_entry(ChatEntry::UserMessage {
            text: "old".into(),
            at: None,
        });
        assert!(matches!(restored, Entry::User { at: None, .. }));
        let stamped = restored_entry(ChatEntry::UserMessage {
            text: "old".into(),
            at: Some(1_757_000_000),
        });
        assert!(matches!(stamped, Entry::User { at: Some(_), .. }));
    }

    #[test]
    fn a_user_row_written_before_the_stamp_restores() {
        let entry: ChatEntry =
            serde_json::from_str(r#"{"UserMessage":{"text":"hi"}}"#).expect("deserializes");
        assert!(matches!(entry, ChatEntry::UserMessage { at: None, .. }));
    }

    /// Every entry of a turn stays in view once it ends — thought, prose,
    /// tool run — and the prose the model writes between its tool calls
    /// reads with the same weight as its answer: no `Worked · N steps`
    /// header, no muted interim rendering.
    #[gpui::test]
    async fn a_finished_turn_keeps_its_work_in_view_and_its_prose_reads_as_an_answer(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.push_entry(Entry::User {
                text: "q".into(),
                at: None,
            });
            chat.push_entry(Entry::Thought {
                text: "hmm".into(),
                open: Default::default(),
                started: None,
                duration_ms: Some(1000),
            });
            chat.push_entry(Entry::Assistant {
                text: "looking".into(),
                document: parse_chat_markdown("looking"),
            });
            chat.push_entry(test_tool_call("a"));
            chat.push_entry(test_tool_call("b"));
            chat.push_entry(Entry::Assistant {
                text: "the answer".into(),
                document: parse_chat_markdown("the answer"),
            });
            chat.push_entry(Entry::TurnFooter("12:00".into()));
            chat
        });
        cx.update(|_window, cx| init(cx));
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("work-toggle-0").is_none(),
            "no Work header under the question"
        );
        let bubble = cx.debug_bounds("user-bubble-0").expect("bubble");
        let thought = cx
            .debug_bounds("thought-toggle-1")
            .expect("the thought row is drawn");
        assert!(
            cx.debug_bounds("interim-2").is_none(),
            "prose before the last tool call is not drawn muted"
        );
        let prose = cx
            .debug_bounds("assistant-response-2")
            .expect("prose before the last tool call is drawn as an answer");
        let run = cx.debug_bounds("tool-run-3").expect("the run box is drawn");
        assert!(
            cx.debug_bounds("answer-5").is_some(),
            "the closing prose is an answer too"
        );
        assert!(
            thought.top() >= bubble.bottom()
                && prose.top() >= thought.bottom()
                && run.top() >= prose.bottom(),
            "rows keep transcript order"
        );
    }

    /// A turn's tool run is drawn while it streams and stays drawn once the
    /// turn ends: nothing folds away.
    #[gpui::test]
    async fn a_tool_run_stays_in_view_after_the_turn_ends(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.push_entry(Entry::User {
                text: "q".into(),
                at: None,
            });
            chat.push_entry(test_tool_call("a"));
            chat.streaming = true;
            chat
        });
        cx.update(|_window, cx| init(cx));
        refresh_frame(cx);
        assert!(cx.debug_bounds("work-open-0").is_none(), "no zone to open");
        assert!(cx.debug_bounds("tool-run-1").is_some());
        assert!(
            cx.debug_bounds("chat-generating-spinner").is_some(),
            "the generating spinner is a sibling of the list"
        );
        chat.update(cx, |chat, cx| {
            chat.handle_event(AcpEvent::AgentMessageChunk("done".into()), cx);
            chat.handle_event(
                AcpEvent::TurnEnded {
                    stop_reason: "end_turn".into(),
                },
                cx,
            );
        });
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("tool-run-1").is_some(),
            "the run stays in view after the turn ends"
        );
        assert!(cx.debug_bounds("answer-2").is_some());
        assert!(
            cx.debug_bounds("work-toggle-0").is_none(),
            "and no header appears for it"
        );
    }

    #[gpui::test]
    async fn day_headings_are_drawn_above_the_first_question_of_a_day(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let now = chrono::Local::now();
        let yesterday = now - chrono::Duration::days(1);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.push_entry(Entry::User {
                text: "a".into(),
                at: Some(yesterday),
            });
            chat.push_entry(Entry::Assistant {
                text: "x".into(),
                document: parse_chat_markdown("x"),
            });
            chat.push_entry(Entry::TurnFooter("t".into()));
            chat.push_entry(Entry::User {
                text: "b".into(),
                at: Some(now),
            });
            chat
        });
        cx.update(|_window, cx| init(cx));
        refresh_frame(cx);
        let heading = cx.debug_bounds("day-heading-0").expect("Yesterday");
        let bubble = cx.debug_bounds("user-bubble-0").expect("bubble");
        assert!(
            heading.bottom() <= bubble.top(),
            "the heading sits above the question"
        );
        assert!(cx.debug_bounds("day-heading-3").is_some(), "Today");
    }

    #[test]
    fn tool_call_run_bounds_inclusive_singles_a_lone_call_and_spans_runs() {
        let entries = vec![
            Entry::User {
                text: "hi".into(),
                at: None,
            },
            test_tool_call("tool-1"),
            test_tool_call("tool-2"),
            Entry::User {
                text: "bye".into(),
                at: None,
            },
        ];
        assert_eq!(tool_call_run_bounds_inclusive(&entries, 0), None);
        assert_eq!(tool_call_run_bounds_inclusive(&entries, 1), Some((1, 2)));
        assert_eq!(tool_call_run_bounds_inclusive(&entries, 2), Some((1, 2)));
        assert_eq!(tool_call_run_bounds_inclusive(&entries, 3), None);
    }

    #[gpui::test]
    async fn launch_with_command_wires_the_given_command(cx: &mut TestAppContext) {
        // `launch` keeps its default program; the picker path must be able
        // to pass the chosen adapter's command straight through and still
        // start the connection exactly like `launch` does.
        let chat = cx.new(|cx| {
            Chat::launch_with_command(
                AgentCommand::new("/definitely/missing/sirio-acp-agent"),
                std::env::temp_dir(),
                cx,
            )
        });

        cx.executor().allow_parking();
        cx.run_until_parked();

        chat.read_with(cx, |chat, _| {
            assert_eq!(
                chat.agent_command
                    .as_ref()
                    .expect("a launched chat has its command")
                    .program,
                PathBuf::from("/definitely/missing/sirio-acp-agent"),
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
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::from_test_command(
                AgentCommand::new("/definitely/missing/sirio-acp-agent"),
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
    /// fixture shape as `sirio_acp`'s own
    /// `session_creation_auth_required_error_becomes_typed_auth_required`.
    #[cfg(unix)]
    #[gpui::test]
    async fn auth_required_launch_gets_a_dedicated_banner_with_login_guidance(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
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

    /// F-CHAT-02 regression: `retryable: true` alone does not make Retry
    /// reachable — a finish-line critic found the auth banner's message div
    /// had no `min_w_0()`, so at the app's real running width the long
    /// two-paragraph login guidance never shrank/wrapped, overflowed the
    /// row, and clipped Retry out of the visible window at both 1715px and
    /// 2400px. This drives the same real (fixture) auth-failure banner as
    /// `auth_required_launch_gets_a_dedicated_banner_with_login_guidance`,
    /// resizes to the app's own 1715px running width, and asserts the Retry
    /// control's drawn bounds actually sit inside the window — not merely
    /// that it renders somewhere.
    #[gpui::test]
    async fn auth_required_retry_survives_long_guidance_text_at_running_width(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
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
        // The app's real captured running size (see wayland-drive.sh's
        // W1xH1) — the exact width the critic reproduced the clip at.
        cx.simulate_resize(gpui::size(px(1715.0), px(972.0)));
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());

        let banner = cx
            .debug_bounds("chat-auth-required-banner")
            .expect("an auth_required launch failure must render the dedicated banner");
        let window_width = cx.update(|window, _| window.viewport_size().width);

        let retry = cx
            .debug_bounds("chat-retry")
            .expect("retryable:true must draw a Retry control, not just set the flag");
        assert!(
            retry.origin.x + retry.size.width <= window_width,
            "Retry must be reachable inside the window, not clipped past its \
             right edge: retry={retry:?} window_width={window_width:?} \
             banner={banner:?}"
        );
        assert!(
            retry.size.width > px(0.0) && retry.size.height > px(0.0),
            "Retry must have a real, non-zero drawn size: {retry:?}"
        );

        // And the control must actually be clickable where it is drawn, not
        // just present in the layout tree at that position: clicking it
        // must launch a fresh connection attempt. The fixture rejects auth
        // immediately every time, so the observable effect is a second
        // AuthRequired error entry landing in the transcript.
        let entries_before_retry = chat.read_with(&*cx, |chat, _| chat.entries.len());
        cx.simulate_click(retry.center(), Modifiers::none());
        cx.run_until_parked();
        chat.read_with(&cx.cx, |chat, _| {
            assert!(
                chat.entries.len() > entries_before_retry,
                "clicking the reachable Retry must start a new connection \
                 attempt, which lands its own error entry: before={} after={:?}",
                entries_before_retry,
                chat.entries
            );
        });
    }

    /// F-CHAT-03: a `grep -rn "Restart" crates/sirio_ui/src` found zero
    /// matches — the only retry-shaped control anywhere was the generic
    /// `chat.retry`-tied "Retry" button, which is a different, differently
    /// labeled control than a disconnected-agent's "Restart agent" affordance
    /// (`App/Chat/ChatPaneView.swift:82`). This connects an agent
    /// successfully, then breaks the transport itself (a malformed reply
    /// the protocol layer can't parse into any response, so it is the
    /// connection dying, not one request being rejected — the shape that
    /// lands `sirio_acp`'s own "ACP transport closed unexpectedly"
    /// wording, as opposed to a per-request "prompt failed: ..." message
    /// with the agent otherwise still alive) and asserts the dedicated
    /// "Agent disconnected" banner renders with a "Restart agent" (not
    /// "Retry") button that, when clicked, launches a fresh connection.
    #[gpui::test]
    async fn a_disconnected_agent_offers_restart_agent_not_retry(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        // Answers initialize and session/new successfully, then on the
        // first prompt replies with a line the protocol layer cannot parse
        // as a response to anything, and exits — a transport failure, not
        // an answerable request failure.
        let command = AgentCommand::new("/bin/sh").args([
            "-c",
            r#"while IFS= read -r line; do id=$(printf '%s' "$line" | sed -E 's/.*"id":([^,]+),.*/\1/'); case "$line" in *initialize*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"protocolVersion":1,"agentCapabilities":{}}}' ;; *session/new*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"sessionId":"test"}}' ;; *session/prompt*) printf 'not json at all\n'; exit 1 ;; esac; done"#,
        ]);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::from_test_command(command, std::env::temp_dir(), cx);
            configure_test_chat(&mut chat);
            chat
        });

        cx.executor().allow_parking();
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);
        focus_and_type(cx, "hello");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        pump_chat_until(cx, &chat, |chat| {
            chat.entries.iter().any(|entry| {
                matches!(
                    entry,
                    Entry::Error {
                        kind: ErrorKind::Disconnected,
                        ..
                    }
                )
            })
        });
        cx.update(|window, _| window.refresh());

        assert!(
            cx.debug_bounds("chat-disconnected-banner").is_some(),
            "a process that exits while idle must render the dedicated \
             disconnected banner, not the generic connection card"
        );
        chat.read_with(&cx.cx, |chat, _| {
            assert!(
                chat.client.is_none(),
                "a dead process must not leave a stale live client handle behind"
            );
            let disconnected_entry = chat.entries.iter().find(|entry| {
                matches!(
                    entry,
                    Entry::Error {
                        kind: ErrorKind::Disconnected,
                        ..
                    }
                )
            });
            assert!(
                disconnected_entry.is_some(),
                "expected a Disconnected error entry, got {:?}",
                chat.entries
            );
        });

        assert!(
            cx.debug_bounds("chat-retry").is_none(),
            "a disconnected agent must not also draw the generic Retry control"
        );
        let restart = cx
            .debug_bounds("chat-restart-agent")
            .expect("the disconnected banner must offer Restart agent");

        // Clicking Restart agent launches a fresh process (this fixture
        // handles initialize/session/new cleanly on every invocation, so
        // this one succeeds and then just sits idle — no second prompt is
        // sent here). The observable effect is exactly what a successful
        // reconnect always does: a live client again, and the stale
        // disconnected banner cleared rather than left stacking up.
        cx.simulate_click(restart.center(), Modifiers::none());
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        chat.read_with(&cx.cx, |chat, _| {
            assert!(
                !chat.entries.iter().any(|entry| matches!(
                    entry,
                    Entry::Error {
                        kind: ErrorKind::Disconnected,
                        ..
                    }
                )),
                "a successful Restart agent must clear the stale disconnected \
                 banner(s), not leave them behind: {:?}",
                chat.entries
            );
        });
    }

    /// F-CHAT-33: an MCP warning is constructed `retryable: false`
    /// (`surface_mcp_warnings`), so before this fix its banner drew zero
    /// interactive controls at all -- there was no way to acknowledge and
    /// move on. The VERIFY clause (and the Swift reference,
    /// `ChatPaneView.swift:177-192`'s `mcpWarning` banner with
    /// `actionTitle: "OK"`) requires an "OK to dismiss" control here.
    #[gpui::test]
    async fn an_mcp_warning_offers_ok_to_dismiss(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::User {
                text: "earlier turn".into(),
                at: None,
            });
            chat.push_entry(Entry::Error {
                message: "MCP server \"scratch\" failed to start".into(),
                retryable: false,
                kind: ErrorKind::McpWarning,
            });
            chat
        });
        refresh_frame(cx);

        assert!(
            cx.debug_bounds("chat-mcp-warning-banner").is_some(),
            "the MCP warning must render its own banner"
        );
        assert!(
            cx.debug_bounds("chat-retry").is_none(),
            "a non-retryable MCP warning must not offer Retry"
        );
        let ok = cx
            .debug_bounds("chat-error-ok")
            .expect("a non-retryable MCP warning must still offer OK to dismiss");

        cx.simulate_click(ok.center(), Modifiers::none());
        cx.run_until_parked();

        chat.read_with(&cx.cx, |chat, _| {
            assert!(
                !chat
                    .entries
                    .iter()
                    .any(|entry| matches!(entry, Entry::Error { .. })),
                "clicking OK must remove the MCP warning: {:?}",
                chat.entries
            );
            assert!(
                chat.entries.iter().any(
                    |entry| matches!(entry, Entry::User { text, .. } if text == "earlier turn")
                ),
                "dismissing the warning must not touch the rest of the transcript: {:?}",
                chat.entries
            );
        });
    }

    /// F-CHAT-33: a retryable turn error (e.g. a transport timeout) must
    /// offer "OK to dismiss" *alongside* Retry, not instead of it -- Swift's
    /// `promptError` banner's `actionTitle: "OK"` only ever clears the
    /// banner, distinct from any retry mechanism, so dismissing without
    /// retrying has to stay a real, separate choice here too.
    #[gpui::test]
    async fn a_retryable_turn_error_offers_ok_alongside_retry(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::Error {
                message: "prompt timed out after 30.0s".into(),
                retryable: true,
                kind: ErrorKind::Connection,
            });
            chat
        });
        refresh_frame(cx);

        assert!(
            cx.debug_bounds("chat-retry").is_some(),
            "a retryable turn error must still offer Retry"
        );
        let ok = cx
            .debug_bounds("chat-error-ok")
            .expect("a retryable turn error must also offer OK to dismiss");

        cx.simulate_click(ok.center(), Modifiers::none());
        cx.run_until_parked();

        chat.read_with(&cx.cx, |chat, _| {
            assert!(
                !chat
                    .entries
                    .iter()
                    .any(|entry| matches!(entry, Entry::Error { .. })),
                "clicking OK must remove the turn error: {:?}",
                chat.entries
            );
            assert!(
                chat.client.is_none(),
                "OK must only dismiss the banner, never call retry/start_connection \
                 the way the Retry button does"
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
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::from_test_command(
                AgentCommand::new("/definitely/missing/sirio-acp-agent"),
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
                config_option_id: None,
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
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::from_test_command(
                AgentCommand::new("/definitely/missing/sirio-acp-agent"),
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
        // The search row is a real TextField: typing, Backspace and Escape
        // are its own job, and the picker holds focus on it while open.
        let search_focus = chat.read_with(&cx.cx, |chat, cx| {
            chat.model_search_field.read(cx).focus_handle(cx)
        });
        let focused =
            cx.update(|window, app| window.focused(app).is_some_and(|f| f == search_focus));
        assert!(focused, "the open picker focuses its search field");
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
            chat.read_with(&cx.cx, |chat, _| chat.draft.trim().is_empty()
                && chat.attachments.is_empty()),
            "backspace inside the search field must not have eaten composer text"
        );
    }

    /// The model picker's search is a real field: typing filters, Backspace
    /// edits, Escape closes — and none of it reaches the composer's draft.
    #[gpui::test]
    async fn model_search_is_a_text_field_that_never_touches_the_draft(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        chat.update(cx, |chat, _| {
            configure_test_chat(chat);
            chat.available_models.push(ModelOption {
                id: "sonnet".into(),
                name: "Sonnet".into(),
                description: None,
            });
        });
        refresh_frame(cx);
        focus_and_type(cx, "draft stays");
        let chip = cx.debug_bounds("model-chip").expect("model chip");
        cx.simulate_click(chip.center(), Modifiers::none());
        refresh_frame(cx);
        assert!(cx.debug_bounds("model-picker").is_some());
        cx.simulate_input("son");
        refresh_frame(cx);
        assert!(cx.debug_bounds("model-option-sonnet").is_some());
        assert!(
            cx.debug_bounds("model-option-opus").is_none(),
            "the search narrows the list"
        );
        cx.simulate_keystrokes("backspace backspace backspace");
        refresh_frame(cx);
        assert!(cx.debug_bounds("model-option-opus").is_some());
        cx.simulate_keystrokes("escape");
        refresh_frame(cx);
        assert!(cx.debug_bounds("model-picker").is_none());
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.draft_text()),
            "draft stays"
        );
    }

    /// #233: the effort row is a plain flex row inside a fixed 245px popup,
    /// and the choice count is agent-reported (Claude Code advertises six,
    /// whose chips plus gaps exceed the popup's usable width), so the row
    /// must wrap like the colour swatch row in `controls::color_picker`
    /// (F-PRJ-13) instead of painting chips outside the picker border. The
    /// popup is fixed-width regardless of window size, so these bounds
    /// assertions are deterministic.
    #[gpui::test]
    async fn every_effort_chip_stays_inside_the_picker_border(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::from_test_command(
                AgentCommand::new("/definitely/missing/sirio-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            chat.has_completed_turn = true;
            chat.available_models = vec![ModelOption {
                id: "opus".into(),
                name: "Opus".into(),
                description: None,
            }];
            chat.effort = Some(EffortOption {
                option_id: "effort".into(),
                name: Some("Effort".into()),
                current_value: Some("medium".into()),
                choices: [
                    ("default", "Default"),
                    ("low", "Low"),
                    ("medium", "Medium"),
                    ("high", "High"),
                    ("xhigh", "Xhigh"),
                    ("max", "Max"),
                ]
                .iter()
                .map(|(value, name)| EffortChoice {
                    value: (*value).into(),
                    name: (*name).into(),
                })
                .collect(),
            });
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

        let picker = cx
            .debug_bounds("model-picker")
            .expect("model picker is rendered");
        for (selector, name) in [
            ("effort-option-default", "Default"),
            ("effort-option-low", "Low"),
            ("effort-option-medium", "Medium"),
            ("effort-option-high", "High"),
            ("effort-option-xhigh", "Xhigh"),
            ("effort-option-max", "Max"),
        ] {
            let bounds = cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("effort chip {name} ({selector}) is rendered"));
            assert!(
                bounds.right() <= picker.right() && bounds.left() >= picker.left(),
                "effort chip {name} ({selector}) overflows the picker border: \
                 chip [{}, {}] vs picker right {} / picker left {}",
                bounds.right(),
                bounds.left(),
                picker.right(),
                picker.left(),
            );
        }
    }

    /// #233: the model name in an option row never shrinks below
    /// min-content, so a long agent-advertised name pushes the "Recommended"
    /// badge past the fixed picker border. The name must truncate (the
    /// agent-badge pattern: `flex_1` + `min_w_0` + `text_ellipsis`, with a
    /// `flex_shrink_0` badge) instead of overflowing. The name is chosen
    /// far past the popup's usable width so font-metric drift cannot
    /// silently un-reproduce the bug.
    #[gpui::test]
    async fn a_long_model_name_does_not_push_the_recommended_badge_outside(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::from_test_command(
                AgentCommand::new("/definitely/missing/sirio-acp-agent"),
                std::env::temp_dir(),
                cx,
            );
            chat.has_completed_turn = true;
            chat.available_models = vec![ModelOption {
                id: "long".into(),
                name: "claude-sonnet-4-5-20250929-with-a-very-long-suffix-string-0123456789abcdef"
                    .into(),
                description: None,
            }];
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

        let picker = cx
            .debug_bounds("model-picker")
            .expect("model picker is rendered");
        let badge = cx
            .debug_bounds("model-option-recommended")
            .expect("the first-listed model is badged Recommended");
        assert!(
            badge.right() <= picker.right(),
            "Recommended badge overflows the picker border: \
             badge right {} vs picker right {}",
            badge.right(),
            picker.right(),
        );
    }

    #[gpui::test]
    async fn context_ring_shows_reported_usage_and_escape_dismisses_popover(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::from_test_command(
                AgentCommand::new("/definitely/missing/sirio-acp-agent"),
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
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::from_test_command(
                AgentCommand::new("/definitely/missing/sirio-acp-agent"),
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
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::Thought {
                text: "considering the approach".into(),
                open: Default::default(),
                started: None,
                duration_ms: None,
            });
            chat
        });
        cx.update(|window, _| window.refresh());

        fn is_expanded(chat: &Entity<Chat>, cx: &mut VisualTestContext) -> bool {
            chat.read_with(cx, |chat, _| {
                matches!(chat.entries.last(), Some(Entry::Thought { open, .. }) if open.get(false))
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
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
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
                duration_ms,
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
                assert!(duration_ms.is_none(), "a running tool call has no duration");
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
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
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
                duration_ms: None,
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
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
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
                duration_ms: None,
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

    /// A call is one `step_row`: icon and verb from its kind, the title as
    /// the truncating detail, the duration (or the status word) pinned right,
    /// and a chevron only when there is something to open.
    #[gpui::test]
    async fn a_tool_call_is_a_step_row_with_a_chevron_only_when_it_has_a_body(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            let mut bare = test_tool_call("bare");
            if let Entry::ToolCall {
                content,
                locations,
                duration_ms,
                ..
            } = &mut bare
            {
                content.clear();
                locations.clear();
                *duration_ms = Some(1412);
            }
            chat.push_entry(bare);
            chat.push_entry(Entry::Assistant {
                text: "x".into(),
                document: parse_chat_markdown("x"),
            });
            let mut full = test_tool_call("full");
            if let Entry::ToolCall {
                content,
                locations,
                status,
                duration_ms,
                ..
            } = &mut full
            {
                content.push(ToolCallContentInfo::Text("hello from the tool".into()));
                // A path long enough that the link must clip inside the
                // run box rather than run past it.
                locations.push(ToolCallLocationInfo {
                    path: std::path::PathBuf::from(format!("/{}", "a".repeat(300))),
                    line: None,
                });
                *status = "failed".into();
                *duration_ms = None;
            }
            chat.push_entry(full);
            chat
        });
        cx.update(|_window, cx| init(cx));
        refresh_frame(cx);
        assert!(cx.debug_bounds("tool-call-toggle-0").is_some());
        assert!(
            cx.debug_bounds("tool-call-chevron-0").is_none(),
            "nothing to open, no chevron"
        );
        assert!(
            cx.debug_bounds("tool-call-meta-0-1.4s").is_some(),
            "a measured call shows its duration"
        );
        assert!(
            cx.debug_bounds("tool-call-chevron-2").is_some(),
            "text output opens"
        );
        assert!(
            cx.debug_bounds("tool-call-meta-2-failed").is_some(),
            "unmeasured shows the status word"
        );
        assert!(
            cx.debug_bounds("tool-call-failed-2").is_some(),
            "a failed row is flagged"
        );
        assert!(
            cx.debug_bounds("tool-output-2-0").is_none(),
            "closed until clicked"
        );
        let row = cx.debug_bounds("tool-call-toggle-2").expect("row");
        cx.simulate_click(row.center(), Modifiers::none());
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("tool-output-2-0").is_some(),
            "the row opens onto its output"
        );
        let link = cx.debug_bounds("tool-call-location-2-0").expect("the link");
        let run = cx.debug_bounds("tool-run-2").expect("the lone call's box");
        assert!(
            link.right() <= run.right(),
            "a long path ends in an ellipsis inside the box: link={link:?} run={run:?}"
        );
    }

    /// A run of consecutive calls is one bordered box; inside it, consecutive
    /// calls of the same verb fold under a `Verb · N` header that opens on
    /// click — `chunk_by` twice, the gallery's own finding.
    #[gpui::test]
    async fn a_run_is_one_box_and_same_verb_calls_fold(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            for (id, kind) in [
                ("a", "Read"),
                ("b", "Read"),
                ("c", "Read"),
                ("d", "Execute"),
            ] {
                let mut call = test_tool_call(id);
                if let Entry::ToolCall { kind: k, .. } = &mut call {
                    *k = kind.into();
                }
                chat.push_entry(call);
            }
            chat.push_entry(Entry::Assistant {
                text: "done".into(),
                document: parse_chat_markdown("done"),
            });
            chat.push_entry(test_tool_call("e"));
            // Row-level assertions need the rows: the turn streams so the
            // Work zone opens by itself.
            chat.streaming = true;
            chat
        });
        refresh_frame(cx);
        let run = cx
            .debug_bounds("tool-run-0")
            .expect("the four calls share one box");
        assert!(
            cx.debug_bounds("tool-run-5").is_some(),
            "the lone call after the prose is its own box"
        );
        assert!(cx.debug_bounds("tool-run-1").is_none());
        let fold = cx
            .debug_bounds("tool-fold-0")
            .expect("three Reads fold under one header");
        assert!(
            cx.debug_bounds("tool-call-toggle-0").is_none(),
            "folded members are not drawn"
        );
        assert!(
            cx.debug_bounds("tool-call-toggle-3").is_some(),
            "the Execute row stands on its own"
        );
        assert!(
            cx.debug_bounds("tool-fold-3").is_none(),
            "a run of one has no fold header"
        );
        assert!(fold.top() >= run.top() && fold.bottom() <= run.bottom());

        cx.simulate_click(fold.center(), Modifiers::none());
        refresh_frame(cx);
        let member = cx
            .debug_bounds("tool-call-toggle-1")
            .expect("opening the fold draws its members");
        assert!(
            member.left() > fold.left(),
            "members are indented under the header"
        );
        let fold_again = cx
            .debug_bounds("tool-fold-0")
            .expect("fold header persists");
        cx.simulate_click(fold_again.center(), Modifiers::none());
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("tool-call-toggle-1").is_none(),
            "clicking again folds them back"
        );
    }

    /// A long title truncates in the row's detail slot instead of wrapping:
    /// the row stays one line tall.
    #[gpui::test]
    async fn a_tool_row_with_a_long_title_stays_one_line(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.push_entry(test_tool_call("short"));
            let mut long = test_tool_call("long");
            if let Entry::ToolCall { title, kind, .. } = &mut long {
                *title = format!("{}file.rs", "directory ".repeat(80));
                *kind = "Execute".into();
            }
            chat.push_entry(long);
            // The row is the subject, so the turn streams and the zone
            // opens by itself.
            chat.streaming = true;
            chat
        });
        refresh_frame(cx);
        let short = cx.debug_bounds("tool-call-toggle-0").expect("short row");
        let long = cx.debug_bounds("tool-call-toggle-1").expect("long row");
        assert_eq!(
            short.size.height, long.size.height,
            "the detail truncates, the row does not grow"
        );
        assert!(long.right() <= cx.debug_bounds("tool-run-0").unwrap().right());
    }

    /// A title with newlines draws as one truncating line: the row stays the
    /// same height as a single-line title.
    #[gpui::test]
    async fn a_tool_row_with_a_multi_line_title_stays_one_line(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.push_entry(test_tool_call("single"));
            let mut multi = test_tool_call("multi");
            if let Entry::ToolCall { title, kind, .. } = &mut multi {
                *title =
                    "cd /d/Progetti/sirio/sirio && python - <<'EOF'\nimport io\np = 1\nEOF".into();
                *kind = "Execute".into();
            }
            chat.push_entry(multi);
            // The row is the subject, so the turn streams and the zone
            // opens by itself.
            chat.streaming = true;
            chat
        });
        refresh_frame(cx);
        let single = cx.debug_bounds("tool-call-toggle-0").expect("single row");
        let multi = cx.debug_bounds("tool-call-toggle-1").expect("multi row");
        assert_eq!(
            single.size.height, multi.size.height,
            "a multi-line title does not grow the row"
        );
    }

    #[gpui::test]
    async fn transcript_only_lays_out_rows_near_the_viewport(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
                std::env::temp_dir(),
                cx,
            );
            for index in 0..100 {
                chat.push_entry(Entry::User {
                    text: format!("Transcript entry {index}"),
                    at: None,
                });
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
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::User {
                text: "user question".into(),
                at: None,
            });
            chat.push_entry(Entry::Assistant {
                text: "assistant answer".into(),
                document: parse_chat_markdown("assistant answer"),
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
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::User {
                text: "question".into(),
                at: None,
            });
            chat.push_entry(Entry::Assistant {
                text: "answer".into(),
                document: parse_chat_markdown("answer"),
            });
            chat
        });
        let transcript = chat.read_with(cx, |chat, _| chat.transcript_for_resume());

        let (restored, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
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
                AgentCommand::new("/definitely/missing/sirio-acp-agent"),
                cwd.clone(),
                cx,
            )
        });

        // ACP owns a real worker thread and subprocess; permit its wakeups to
        // cross the deterministic test scheduler boundary.
        cx.executor().allow_parking();
        cx.run_until_parked();
        chat.update(cx, |chat, cx| {
            chat.set_composer_text("draft that must survive", cx);
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
            chat.agent_command = Some(AgentCommand::new("/bin/sh").args([
                "-c",
                r#"while IFS= read -r line; do id=$(printf '%s' "$line" | sed -E 's/.*"id":([^,]+),.*/\1/'); case "$line" in *initialize*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"protocolVersion":1,"agentCapabilities":{},"authMethods":[]}}' ;; *session/new*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"sessionId":"test"}}' ;; *session/prompt*) printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"stopReason":"end_turn"}}' ;; esac; done"#,
            ]));
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
                chat.draft_text(),
                "draft that must survive",
                "reconnecting on its own must not touch the still-unsent draft"
            );
            assert!(chat.can_send(), "Send must re-enable once back online");
        });

        // Now that the composer is enabled again, an ordinary Send goes
        // through exactly as it would have while never disconnected.
        chat.update(cx, |chat, cx| {
            chat.set_composer_text("hello", cx);
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
                    .any(|entry| { matches!(entry, Entry::User { text, .. } if text == "hello") })
            );
            assert!(chat.has_completed_turn, "retry prompt should complete");
        });
    }

    #[gpui::test]
    async fn connecting_state_renders_while_startup_is_in_flight(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (_, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
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
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.slash_filter.active().unwrap_or(0)),
            0
        );
        cx.simulate_keystrokes("down");
        cx.run_until_parked();
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.slash_filter.active().unwrap_or(0)),
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
            chat.read_with(&cx.cx, |chat, _| chat.draft_text()),
            "/create-plan ",
            "the accepted command lands as a skill token"
        );

        // The token survives submission: Enter sends it as the /name prefix.
        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| chat.has_completed_turn);
        assert!(chat.read_with(&cx.cx, |chat, _| {
            chat.entries.iter().any(
                |entry| matches!(entry, Entry::User { text, .. } if text.trim() == "/create-plan"),
            )
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
            chat.read_with(&cx.cx, |chat, _| chat.draft_text()),
            "/cr ",
            "clicking a row inserts that command's skill token"
        );
    }

    /// The pickers hang above the token that opened them, not above the
    /// card: as the field grows a row, the menu follows the caret.
    #[gpui::test]
    async fn slash_popup_hangs_above_the_slash_and_steps_with_the_arrows(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["composer"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);
        focus_and_type(cx, "/");
        refresh_frame(cx);
        let popup = cx.debug_bounds("slash-popup").expect("popup");
        let input = cx.debug_bounds("composer-input").expect("field");
        assert!(
            popup.bottom() <= input.top() + px(4.0),
            "the menu opens upward from the token row"
        );
        assert!(
            popup.left() >= input.left() - px(8.0),
            "and starts at the token's column"
        );
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.slash_filter.active()),
            Some(0)
        );
        cx.simulate_keystrokes("down");
        cx.run_until_parked();
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.slash_filter.active()),
            Some(1)
        );
        cx.simulate_keystrokes("up");
        cx.run_until_parked();
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.slash_filter.active()),
            Some(0)
        );
    }

    /// The command popup is a card of its own, floated above the composer.
    /// It used to be anchored 43px up from the composer's *bottom* — inside
    /// the card, in the card's own `surface_raised` fill — so it covered the
    /// input rows and, being the same colour as what it lay on, read as a
    /// transparent veil. A row carries only the command name; the
    /// description is its tooltip, so a row is exactly one line tall.
    #[gpui::test]
    async fn slash_popup_floats_above_the_composer_with_single_line_rows(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["composer"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        focus_and_type(cx, "/");
        refresh_frame(cx);
        let popup = cx
            .debug_bounds("slash-popup")
            .expect("typing / opens the command popup");
        let composer = cx.debug_bounds("composer").expect("the composer is drawn");
        assert!(
            popup.bottom() <= composer.top(),
            "the popup must float above the composer, never over its input rows \
             (popup bottom {:?}, composer top {:?})",
            popup.bottom(),
            composer.top()
        );

        let row = cx
            .debug_bounds("slash-option-cr")
            .expect("a command row is drawn");
        let name = cx
            .debug_bounds("slash-option-name-cr")
            .expect("the row draws the command name");
        // One line: bezel's `menu_row` adds 6px of vertical padding to the
        // name's line box; anything meaningfully taller means a second line
        // — the description — crept back into the row.
        assert!(
            row.size.height <= name.size.height + px(13.0),
            "a row is the command name alone, one line tall \
             (row {:?}, name {:?})",
            row.size.height,
            name.size.height
        );
    }

    #[test]
    fn slash_option_tooltip_carries_the_description_and_skips_a_blank_one() {
        assert_eq!(
            slash_option_tooltip("  Deep research harness.  ").as_deref(),
            Some("Deep research harness.")
        );
        assert_eq!(slash_option_tooltip(""), None);
        assert_eq!(slash_option_tooltip("   "), None);
    }

    /// F-CHAT-10: typing `@` opens the mention popup fed by a real bounded
    /// filesystem walk over the chat's working directory; clicking a listed
    /// file inserts a `@path ` mention token at the token's position, the
    /// token survives further typing, and sending carries the path as a
    /// mention rather than text.
    #[gpui::test]
    async fn at_mention_popup_lists_files_and_inserts_a_mention_token(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        std::fs::create_dir_all(dir.0.join("src")).expect("create src dir");
        std::fs::write(dir.0.join("src/main.rs"), "fn main() {}").expect("write main.rs");
        std::fs::write(dir.0.join("README.md"), "# readme").expect("write readme");
        // A relative path long enough that an unconstrained row would paint
        // the card far past its own 360px width.
        let deep = dir.0.join(
            "rust/target/debug/incremental/o1o2o3o4o5/quirky-uid-slug/a-place-for-building-things/very",
        );
        std::fs::create_dir_all(&deep).expect("create deep dir");
        std::fs::write(deep.join("long-incremental-artifact-name.bin"), "artifact")
            .expect("write long-path file");
        let cwd = dir.0.clone();

        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
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
        let popup = cx
            .debug_bounds("mention-popup-card")
            .expect("typing @ opens the mention popup");
        // The card's width is a cap, not a suggestion: a 90+-character path
        // must ellipsize inside it, never stretch the card. Measured on the
        // card itself — the deferred layer's wrapper is zero-size and would
        // prove nothing.
        assert!(
            popup.size.width <= px(372.0),
            "the mention popup is capped at its 360px card (plus border/shadow allowance): {popup:?}"
        );
        let long_row = cx
            .debug_bounds(
                "mention-option-rust/target/debug/incremental/o1o2o3o4o5/quirky-uid-slug/a-place-for-building-things/very/long-incremental-artifact-name.bin",
            )
            .expect("the long-path row is drawn");
        // The row fills the card's inner width (360 − 2×4 card pad) and the
        // path ellipsizes inside it — it never overflows the card.
        assert!(
            long_row.size.width <= px(352.0),
            "the long-path row stays inside the card and ellipsizes: {long_row:?}"
        );
        assert!(cx.debug_bounds("mention-option-README.md").is_some());
        assert!(cx.debug_bounds("mention-option-src/main.rs").is_some());
        // Same anchor as the command popup: a card above the composer, not a
        // list drawn over its input rows.
        assert!(
            popup.bottom() <= composer.top(),
            "the mention popup must float above the composer              (popup bottom {:?}, composer top {:?})",
            popup.bottom(),
            composer.top()
        );

        let row = cx
            .debug_bounds("mention-option-src/main.rs")
            .expect("the file row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("mention-popup-card").is_none(),
            "choosing a file closes the popup"
        );
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.draft_text()),
            "@src/main.rs ",
            "the accepted file lands as a mention token"
        );

        // The token survives editing after it and serializes as a mention
        // path, not text.
        cx.simulate_input(" check");
        cx.run_until_parked();
        let (text, paths) = chat.read_with(&cx.cx, |chat, _| {
            assemble_prompt(&chat.draft, &chat.accepted_mentions)
        });
        assert_eq!(text.trim(), "check");
        assert_eq!(paths, vec!["src/main.rs".to_string()]);

        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| {
            chat.entries
                .iter()
                .any(|entry| matches!(entry, Entry::User { text, .. } if text.trim() == "check"))
        });
        assert!(
            !chat.read_with(&cx.cx, |chat, _| {
                chat.entries
                    .iter()
                    .any(|entry| matches!(entry, Entry::User { text, .. } if text.contains("src/main.rs")))
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
            cx.debug_bounds("attachment-chip-0").is_some(),
            "a supported image appears as an attachment chip"
        );
        assert!(cx.debug_bounds("attach-error").is_none());
        let (count, mime) = chat.read_with(&cx.cx, |chat, _| {
            (
                chat.attachments.len(),
                chat.attachments[0].mime_type.clone(),
            )
        });
        assert_eq!(count, 1);
        assert_eq!(mime, "image/png");

        // An unsupported file is rejected with a transient message. The
        // chip added above grew the card, which moved the toolbar above it
        // — re-read the attach control's bounds before clicking again.
        chat.update(cx, |chat, _| {
            chat.attach_test_paths = vec![txt.clone()];
        });
        let attach = cx
            .debug_bounds("attach-image")
            .expect("the attach control is drawn");
        cx.simulate_click(attach.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("attach-error").is_some(),
            "an unsupported selection shows the rejection"
        );
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.attachments.len()),
            1,
            "the rejected file adds no chip"
        );
        pump_chat_until(cx, &chat, |chat| chat.attach_error.is_none());

        // A multiple selection is rejected too. Re-read the bounds: the
        // toolbar moved when the card grew.
        chat.update(cx, |chat, _| {
            chat.attach_test_paths = vec![jpeg.clone(), png.clone()];
        });
        let attach = cx
            .debug_bounds("attach-image")
            .expect("the attach control is drawn");
        cx.simulate_click(attach.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("attach-error").is_some(),
            "a multiple selection shows the rejection"
        );
        pump_chat_until(cx, &chat, |chat| chat.attach_error.is_none());

        // F-CHAT-12: the chip's removal control removes it before sending.
        refresh_frame(cx);
        assert!(cx.debug_bounds("attachment-chip-0").is_some());
        let remove = cx
            .debug_bounds("attachment-remove-0")
            .expect("the chip removal control is drawn");
        cx.simulate_click(remove.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("attachment-chip-0").is_none(),
            "removing the chip makes it disappear"
        );
        assert!(chat.read_with(&cx.cx, |chat, _| { chat.attachments.is_empty() }));

        // F-CHAT-12: the × must also re-request composer focus. Type
        // immediately after the click, with no intervening click back into
        // the field — a real user's next gesture — and the keystrokes must
        // land, not vanish into a keyboard-dead composer.
        cx.simulate_input("still here");
        cx.run_until_parked();
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.draft_text()),
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
        cx.update(bezel::ui::input::init);
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
            cx.debug_bounds("attachment-chip-0").is_some(),
            "the supported image becomes an attachment chip"
        );
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.draft_text()),
            "@src/notes.txt ",
            "the non-image file becomes a @-style mention token"
        );
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.attachments.len()),
            1,
            "only the one valid image attaches"
        );
        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| {
                assemble_prompt(&chat.draft, &chat.accepted_mentions).1
            }),
            vec!["src/notes.txt".to_string()],
            "the file token's path is relative to the agent's cwd"
        );
        assert!(
            cx.debug_bounds("attach-error").is_some(),
            "the oversized image is rejected with the transient message"
        );
    }

    /// F-CORE-FILE-03A: dropped item URLs are inserted in the user's drop
    /// order, not re-sorted. `gpui::ExternalPaths` is backed by an ordered
    /// `SmallVec` (`vendor/.../gpui/src/interactive.rs`), and
    /// `drop_external_paths` iterates it with a plain `for path in &paths`
    /// — but that is a claim about the source, and `EVIDENCE-STANDARD.md` is
    /// explicit that reading code proves nothing. Drop BRAVO before ALPHA
    /// (deliberately the reverse of alphabetical) and assert the draft's
    /// `mention_paths` comes back BRAVO-then-ALPHA: if the classify/insert
    /// path ever sorted, deduped-by-BTreeSet, or otherwise lost ordering,
    /// this fails where the earlier attach test above (all-distinct-kinds,
    /// order incidental) could not have caught it.
    #[gpui::test]
    async fn dropping_external_files_preserves_the_drop_order(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let bravo = dir.0.join("BRAVO.txt");
        std::fs::write(&bravo, b"second alphabetically, dropped first").expect("write BRAVO.txt");
        let alpha = dir.0.join("ALPHA.txt");
        std::fs::write(&alpha, b"first alphabetically, dropped second").expect("write ALPHA.txt");
        let cwd = dir.0.clone();

        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let command = AgentCommand::new("python3").args([CHAT_FIXTURE, "plain"]);
            Chat::from_test_command(command, cwd, cx)
        });
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);

        let composer = cx.debug_bounds("composer").expect("the composer is drawn");
        // Literal drop order: BRAVO first, ALPHA second — the reverse of
        // alphabetical, so an accidental sort anywhere in the path would
        // flip this and the assertion below would catch it.
        let paths = ExternalPaths(vec![bravo.clone(), alpha.clone()].into_iter().collect());
        cx.simulate_event(FileDropEvent::Entered {
            position: composer.center(),
            paths,
        });
        cx.simulate_event(FileDropEvent::Submit {
            position: composer.center(),
        });
        cx.run_until_parked();
        refresh_frame(cx);

        assert_eq!(
            chat.read_with(&cx.cx, |chat, _| chat.draft_text()),
            "@BRAVO.txt @ALPHA.txt ",
            "the tokens land in drop order"
        );
        let (text, paths) = chat.read_with(&cx.cx, |chat, _| {
            assemble_prompt(&chat.draft, &chat.accepted_mentions)
        });
        assert_eq!(text, "", "the dropped text files are mentions, not text");
        assert_eq!(
            paths,
            vec!["BRAVO.txt".to_string(), "ALPHA.txt".to_string()],
            "mention_paths must preserve the literal drop order (BRAVO then ALPHA),              not alphabetise to ALPHA-then-BRAVO"
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
        cx.update(bezel::ui::input::init);
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
            cx.debug_bounds("attachment-chip-0").is_none(),
            "a drop during permission-wait must not add a chip"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.attachments.is_empty()),
            "a drop during permission-wait must not touch the draft"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.draft_text().is_empty()),
            "a drop during permission-wait must not insert a mention token"
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
        assert_eq!(chat.read_with(&cx.cx, |chat, _| chat.draft_text()), "draft");
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
            chat.read_with(&cx.cx, |chat, _| chat.unfolded_turns.is_empty()),
            "New Conversation clears the turn fold state"
        );
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.draft.trim().is_empty()
                && chat.attachments.is_empty()),
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
                .any(|entry| matches!(entry, Entry::User { text, .. } if text == "again"))
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
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
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

    /// The chevron has to stay beside the name it qualifies.
    ///
    /// The chip used to carry `flex_1`, stretching the pill across the whole
    /// control row, and the name div carried it too, so the name ate the
    /// slack and the chip's own chevron landed at the row's right edge --
    /// next to the overflow button, ~200px from the model it belonged to. A
    /// blind review of the composer read it as "a lone chevron floating
    /// mid-row, orphaned from whatever it belongs to", and read the model
    /// value as a caption rather than something clickable.
    #[gpui::test]
    async fn the_model_chips_chevron_stays_beside_the_model_name(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["composer"]);
        pump_chat_until(cx, &chat, |chat| {
            chat.effort.is_some() && !chat.available_models.is_empty() && !chat.streaming
        });
        refresh_frame(cx);
        focus_and_type(cx, "hi");
        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| chat.has_completed_turn);
        refresh_frame(cx);

        let chip = cx
            .debug_bounds("model-chip")
            .expect("the model chip is drawn");
        let chevron = cx
            .debug_bounds("model-chip-chevron")
            .expect("the chip's chevron is drawn");

        // The chevron sits inside the pill, near its right edge -- which is
        // only true when the pill is sized to its content.
        let trailing_gap = f32::from(chip.right() - chevron.right());
        assert!(
            trailing_gap < 24.0,
            "the chevron must sit at the pill's own right edge, not be stranded              by a stretched pill: gap {trailing_gap}px"
        );

        // And the pill must not span the control row. The composer is far
        // wider than a model name; a pill claiming most of it is the bug.
        let composer = cx.debug_bounds("composer").expect("the composer is drawn");
        assert!(
            f32::from(chip.size.width) < f32::from(composer.size.width) * 0.6,
            "the pill must hug its content, not the row: pill {}px of {}px",
            f32::from(chip.size.width),
            f32::from(composer.size.width)
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
        // The effort no longer lives inside the model chip -- it is its own
        // labelled pill beside it, so the old wording here would be wrong.
        assert!(
            cx.debug_bounds("model-effort-label").is_some(),
            "the effort pill shows the selected effort"
        );
        assert_effort_is_a_peer_of_the_model(cx);
    }

    /// The effort pill sits beside the model pill, not inside it.
    ///
    /// It used to be an uppercased caption with no label and no click target,
    /// tucked in among the model chip's own children, so changing it meant
    /// opening the model picker and already knowing the effort lived in
    /// there. Both pills are peers now, and each says what it is.
    fn assert_effort_is_a_peer_of_the_model(cx: &mut VisualTestContext) {
        let model = cx
            .debug_bounds("model-chip")
            .expect("the model chip is drawn");
        let effort = cx
            .debug_bounds("effort-chip")
            .expect("the effort pill is drawn");
        assert!(
            f32::from(effort.left()) >= f32::from(model.right()),
            "the effort pill must start at or after the model pill ends, not              be nested inside it: effort left {}px vs model right {}px",
            f32::from(effort.left()),
            f32::from(model.right())
        );
    }

    /// The effort control is reachable as a peer, and it names itself.
    #[gpui::test]
    async fn the_effort_control_is_a_peer_of_the_model_not_a_caption(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["composer"]);
        pump_chat_until(cx, &chat, |chat| {
            chat.effort.is_some() && !chat.available_models.is_empty() && !chat.streaming
        });
        refresh_frame(cx);
        focus_and_type(cx, "hi");
        cx.simulate_keystrokes("enter");
        pump_chat_until(cx, &chat, |chat| chat.has_completed_turn);
        refresh_frame(cx);

        assert_effort_is_a_peer_of_the_model(cx);

        // And it opens the picker on its own, rather than being a label the
        // user has to know is hidden behind the model chip.
        let effort = cx
            .debug_bounds("effort-chip")
            .expect("the effort pill is drawn");
        cx.simulate_click(effort.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("effort-option-high").is_some(),
            "clicking the effort pill must reach the effort choices"
        );
    }

    /// The context meter names what it measures.
    ///
    /// It used to be a ring and a bare percentage. A blind review could read
    /// both and still not know what they measured -- the answer only existed
    /// in the popover, a click away, which spells out "N% of context used".
    /// Every other value in this row carries its field name inline; this one
    /// now does too, between the ring and the number.
    #[gpui::test]
    async fn the_context_meter_names_what_it_measures(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["composer"]);
        pump_chat_until(cx, &chat, |chat| chat.context_usage.is_some());
        refresh_frame(cx);

        let ring = cx
            .debug_bounds("context-ring")
            .expect("the context ring is drawn");
        let label = cx
            .debug_bounds("context-label")
            .expect("the context meter is labelled");
        let percent = cx
            .debug_bounds("context-percent")
            .expect("the context percentage is drawn");

        assert!(
            f32::from(label.left()) >= f32::from(ring.right()),
            "the label follows the ring: label left {}px vs ring right {}px",
            f32::from(label.left()),
            f32::from(ring.right())
        );
        assert!(
            f32::from(percent.left()) >= f32::from(label.right()),
            "the value follows its label, the same order the model and effort              pills use: value left {}px vs label right {}px",
            f32::from(percent.left()),
            f32::from(label.right())
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
                AgentCommand::new("/definitely/missing/sirio-acp-agent"),
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
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::from_test_command(
                AgentCommand::new("/definitely/missing/sirio-acp-agent"),
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

    // ---------------------------------------------------------------
    // F-CHAT-22 (turn half), F-CHAT-23 (locations), F-CHAT-31 (diff)
    // ---------------------------------------------------------------

    /// The fold POLICY, transcribed from Swift's `TimelineBuilder`: the two
    /// most recent turns stay open, a turn still being answered is never
    /// folded, and a turn with nothing but its own footer has nothing to
    /// fold. Mirrors `olderTurnsFoldKeepingLastTwoOpen`.
    #[test]
    fn only_turns_older_than_the_last_two_fold() {
        let entries = vec![
            Entry::User {
                text: "first question".into(),
                at: None,
            },
            Entry::TurnFooter("10:00".into()),
            Entry::User {
                text: "second question".into(),
                at: None,
            },
            Entry::TurnFooter("10:01".into()),
            Entry::User {
                text: "third question".into(),
                at: None,
            },
            Entry::TurnFooter("10:02".into()),
            Entry::User {
                text: "fourth question".into(),
                at: None,
            },
        ];
        let turns = segment_turns(&entries);
        assert_eq!(turns.len(), 4, "three closed turns plus the open one");
        let foldable = (0..turns.len())
            .filter(|position| turn_is_foldable(&turns, *position))
            .collect::<Vec<_>>();
        assert_eq!(foldable, vec![0, 1], "the last two turns stay open");
        assert_eq!(turns[3].footer, None, "the trailing turn is still open");
    }

    /// A transcript with only one or two turns folds nothing — and, because
    /// Swift's `turns.count - openTurnCount` is a `usize` subtraction here,
    /// this is also the case that would panic on underflow if the comparison
    /// were transcribed literally.
    #[test]
    fn a_short_transcript_folds_nothing_and_does_not_underflow() {
        for count in 0..=2usize {
            let mut entries = Vec::new();
            for index in 0..count {
                entries.push(Entry::User {
                    text: format!("question {index}"),
                    at: None,
                });
                entries.push(Entry::TurnFooter(format!("10:0{index}")));
            }
            let turns = segment_turns(&entries);
            assert!(
                (0..turns.len()).all(|position| !turn_is_foldable(&turns, position)),
                "nothing folds with {count} closed turns"
            );
        }
    }

    /// Swift's `Turn.label`: the first line of the turn's first user
    /// message, clipped to 60 characters, with a `"Turn"` fallback.
    #[test]
    fn a_fold_row_is_labelled_by_the_question_that_opened_the_turn() {
        let entries = vec![
            Entry::Assistant {
                text: "leading note".into(),
                document: parse_chat_markdown("leading note"),
            },
            Entry::User {
                text: "what does this do?\nsecond line".into(),
                at: None,
            },
            Entry::TurnFooter("10:00".into()),
        ];
        let turns = segment_turns(&entries);
        assert_eq!(turn_label(&entries, &turns[0]), "what does this do?");

        let long = "x".repeat(100);
        let entries = vec![
            Entry::User {
                text: long,
                at: None,
            },
            Entry::TurnFooter("10:00".into()),
        ];
        let turns = segment_turns(&entries);
        assert_eq!(turn_label(&entries, &turns[0]).chars().count(), 60);

        let entries = vec![
            Entry::Assistant {
                text: "no question here".into(),
                document: parse_chat_markdown("no question here"),
            },
            Entry::TurnFooter("10:00".into()),
        ];
        let turns = segment_turns(&entries);
        assert_eq!(turn_label(&entries, &turns[0]), "Turn");
    }

    /// F-CHAT-22, turn half, driven: a folded turn draws exactly one row in
    /// place of its whole body, a real click on that row puts the body back,
    /// and a real click on the re-exposed footer folds it again — without
    /// moving the rows below it.
    #[gpui::test]
    async fn a_real_click_unfolds_an_older_turn_and_folds_it_back(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
                std::env::temp_dir(),
                cx,
            );
            // 0..=2 first turn, 3..=4 second, 5..=6 third, 7 still open.
            chat.push_entry(Entry::User {
                text: "first question".into(),
                at: None,
            });
            chat.push_entry(test_tool_call("step-one"));
            chat.push_entry(Entry::TurnFooter("10:00".into()));
            chat.push_entry(Entry::User {
                text: "second question".into(),
                at: None,
            });
            chat.push_entry(Entry::TurnFooter("10:01".into()));
            chat.push_entry(Entry::User {
                text: "third question".into(),
                at: None,
            });
            chat.push_entry(Entry::TurnFooter("10:02".into()));
            chat.push_entry(Entry::User {
                text: "fourth question".into(),
                at: None,
            });
            chat
        });
        refresh_frame(cx);

        let fold = cx
            .debug_bounds("turn-fold-2")
            .expect("the oldest turn draws its collapsed stand-in row");
        assert!(
            cx.debug_bounds("turn-fold-4").is_some(),
            "so does the second-oldest"
        );
        assert!(
            cx.debug_bounds("tool-call-toggle-1").is_none(),
            "and the folded turn's contents are not drawn at all"
        );
        let settled_below = cx
            .debug_bounds("turn-fold-4")
            .expect("second fold row is drawn");

        cx.simulate_click(fold.center(), Modifiers::none());
        cx.run_until_parked();
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("tool-call-toggle-1").is_some(),
            "a click on the fold row re-opens the turn in place"
        );
        assert!(
            cx.debug_bounds("turn-fold-2").is_none(),
            "and the stand-in row gives way to the real contents"
        );
        assert_eq!(
            chat.read_with(cx, |chat, _| chat
                .unfolded_turns
                .iter()
                .copied()
                .collect::<Vec<_>>()),
            vec![2],
            "the reopened turn is recorded by its footer index, the way \
             Swift records `divider.id` in `unfoldedTurns`"
        );
        let refold = cx
            .debug_bounds("turn-refold-2")
            .expect("the re-opened turn's footer is the way back");

        cx.simulate_click(refold.center(), Modifiers::none());
        cx.run_until_parked();
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("turn-fold-2").is_some(),
            "clicking the footer folds the turn back up"
        );
        assert!(
            cx.debug_bounds("tool-call-toggle-1").is_none(),
            "and its contents are hidden again"
        );
        assert_eq!(
            cx.debug_bounds("turn-fold-4").map(|bounds| bounds.origin),
            Some(settled_below.origin),
            "an unfold/refold cycle leaves everything below it exactly where \
             it was -- a fold that scrolls the transcript out from under the \
             reader fails the clause as surely as one that cannot re-open"
        );
    }

    /// F-CHAT-23: a tool call's location is a control that opens the file,
    /// not a label that looks like one. Swift makes each one a
    /// `Button { appModel.openFileReference(...) }`.
    #[gpui::test]
    async fn a_real_click_on_a_tool_call_location_opens_the_file(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::ToolCall {
                id: "read".into(),
                title: "Read lib.rs".into(),
                status: "Completed".into(),
                kind: "Read".into(),
                content: vec![],
                locations: vec![
                    ToolCallLocationInfo {
                        path: PathBuf::from("src/lib.rs"),
                        line: Some(42),
                    },
                    ToolCallLocationInfo {
                        path: PathBuf::from("src/other.rs"),
                        line: None,
                    },
                ],
                raw_input: None,
                raw_output: None,
                expanded: true,
                duration_ms: None,
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

        let second = cx
            .debug_bounds("tool-call-location-0-1")
            .expect("a location with no line number is drawn too");
        let first = cx
            .debug_bounds("tool-call-location-0-0")
            .expect("the location link is drawn");
        cx.simulate_click(first.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_click(second.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            opened.borrow().as_slice(),
            [PathBuf::from("src/lib.rs"), PathBuf::from("src/other.rs")],
            "each location opens its own file, not the card's first one"
        );
    }

    #[test]
    fn a_diff_in_the_transcript_takes_the_column_rather_than_the_standalone_760() {
        // The gallery's standalone Diff pattern references 760; inside a 700
        // transcript the diff uses the width it has (spec §3).
        assert!(DIFF_STANDALONE_REFERENCE > TRANSCRIPT_WIDTH);
        assert_eq!(diff_column_width(TRANSCRIPT_WIDTH), TRANSCRIPT_WIDTH);
        assert_eq!(diff_column_width(400.0), 400.0);
    }

    /// F-CHAT-31: the diff preview's header path opens the file, its rows
    /// carry line numbers, and a real drag across a row selects its text
    /// through the transcript's own selection mechanism.
    #[gpui::test]
    async fn a_diff_preview_opens_its_file_and_its_rows_are_numbered_and_selectable(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(
                Some(AgentCommand::new("/definitely/missing/sirio-acp-agent")),
                std::env::temp_dir(),
                cx,
            );
            chat.push_entry(Entry::ToolCall {
                id: "edit".into(),
                title: "Edit lib.rs".into(),
                status: "Completed".into(),
                kind: "Edit".into(),
                content: vec![ToolCallContentInfo::Diff(ToolCallDiff {
                    path: PathBuf::from("src/lib.rs"),
                    old_text: Some("alpha\nbravo\n".into()),
                    new_text: "alpha\nCHARLIE\n".into(),
                })],
                locations: vec![],
                raw_input: None,
                raw_output: None,
                expanded: true,
                duration_ms: None,
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

        // Three rows: one context line, then the removed/added pair.
        for selector in [
            "tool-diff-0-0-number-0",
            "tool-diff-0-0-number-1",
            "tool-diff-0-0-number-2",
        ] {
            assert!(
                cx.debug_bounds(selector).is_some(),
                "{selector} draws a line-number gutter"
            );
        }
        assert!(
            cx.debug_bounds("tool-diff-0-0-number-3").is_none(),
            "and only those three rows exist"
        );

        let header = cx
            .debug_bounds("tool-diff-0-0-open")
            .expect("the preview's header path is drawn");
        cx.simulate_click(header.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            opened.borrow().as_slice(),
            [PathBuf::from("src/lib.rs")],
            "the diff preview's header path opens the file, as Swift's \
             ChatDiffPreviewView header Button does"
        );

        // A real press-drag-release from the removed row into the added one,
        // through the same TranscriptSelectableText element assistant prose
        // uses. Crossing rows is the part that matters: it can only work if
        // both rows are addressed in one transcript-wide coordinate space.
        let from = cx
            .debug_bounds("tool-diff-0-0-text-1")
            .expect("the removed row's text is drawn");
        let to = cx
            .debug_bounds("tool-diff-0-0-text-2")
            .expect("the added row's text is drawn");
        cx.simulate_event(MouseDownEvent {
            position: point(from.left() + px(1.0), from.center().y),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseMoveEvent {
            position: point(to.left() + px(20.0), to.center().y),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(MouseUpEvent {
            position: point(to.left() + px(20.0), to.center().y),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();

        let selected = chat
            .read_with(cx, |chat, _| chat.selected_transcript_text())
            .expect("dragging across the diff selects text");
        // How far into "CHARLIE" 20px lands depends on the measured glyph
        // advance, which is not the claim; that the selection *crossed the
        // row boundary at all* is, because only a shared transcript-wide
        // coordinate space can express it.
        assert!(
            selected.starts_with("bravo\n") && selected.len() > "bravo\n".len(),
            "the selection runs from the removed row into the added one, in \
             one continuous transcript range -- so Ctrl+C copies exactly what \
             the highlight covers; got {selected:?}"
        );
    }

    /// F-CHAT-31: the projection `Entry::plain_text` publishes and the one
    /// `render_tool_diff` anchors its rows in are the same object. If they
    /// ever diverge a selection highlights one string and copies another.
    #[test]
    fn a_tool_call_diffs_rows_are_addressable_in_the_transcript_text() {
        let entry = Entry::ToolCall {
            id: "edit".into(),
            title: "Edit lib.rs".into(),
            status: "Completed".into(),
            kind: "Edit".into(),
            content: vec![ToolCallContentInfo::Diff(ToolCallDiff {
                path: PathBuf::from("src/lib.rs"),
                old_text: Some("alpha\nbravo\n".into()),
                new_text: "alpha\nCHARLIE\n".into(),
            })],
            locations: vec![],
            raw_input: None,
            raw_output: None,
            expanded: true,
            duration_ms: None,
        };
        let Entry::ToolCall {
            title,
            status,
            content,
            locations,
            ..
        } = &entry
        else {
            unreachable!()
        };
        let plain = tool_call_plain_text(title, status, content, locations);
        assert_eq!(plain.text(), entry.plain_text());
        let starts = plain.diff_line_starts(0, 0, 3);
        let text = plain.text();
        assert_eq!(
            starts
                .iter()
                .map(|start| text[*start..].lines().next().unwrap_or_default())
                .collect::<Vec<_>>(),
            vec!["alpha", "bravo", "CHARLIE"],
            "each drawn diff row can name its own offset in the transcript"
        );
        assert!(
            text.contains("diff: src/lib.rs"),
            "the diff still names its file in the copied transcript"
        );
    }

    /// A restored chat whose agent cannot be launched states why, in the
    /// transcript, and offers the one action that fixes it.
    ///
    /// The tab used to be dropped silently during restore: the reason went
    /// to stderr, which a desktop user never sees, so the tab was simply
    /// gone. It comes back disarmed instead — the safety rule the whole
    /// launch-resolution effort exists for is "never connect to another
    /// agent's server", and showing the tab breaks none of it.
    #[gpui::test]
    async fn an_unavailable_chat_states_the_reason_and_never_connects(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            Chat::unavailable(
                "Codex chat cannot start: it is not installed yet — install it \
                 from Settings → Agents."
                    .to_string(),
                std::env::temp_dir(),
                cx,
            )
        });
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("chat-unavailable-banner").is_some(),
            "the reason is stated in the transcript, not on stderr"
        );
        let events = Rc::new(RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&chat, move |_, event: &ChatEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });
        let button = cx
            .debug_bounds("chat-open-settings")
            .expect("the box carries the way to fix it");
        cx.simulate_click(button.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            matches!(events.borrow().as_slice(), [ChatEvent::OpenSettings]),
            "the button asks the workspace to open Settings, rather than \
             merely existing: {:?}",
            events.borrow()
        );
        assert!(
            cx.debug_bounds("chat-retry").is_none(),
            "an unresolvable source has nothing to retry"
        );
        assert!(
            cx.debug_bounds("chat-error-ok").is_none(),
            "this box is the tab's whole content: dismissing it would leave \
             a chat that explains nothing and does nothing"
        );

        chat.read_with(cx, |chat, _| {
            assert!(
                chat.client.is_none() && chat.agent_command.is_none(),
                "no process was started and no command was invented to start one"
            );
            assert!(!chat.can_send(), "the composer stays disabled");
        });
    }

    #[gpui::test]
    fn mode_selectable_is_true_with_catalog_before_first_turn(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));
        chat.update(cx, |chat, _| {
            chat.has_completed_turn = false;
            chat.mode_catalog = Some(ModeCatalog {
                current_id: "build".into(),
                options: vec![
                    AgentMode {
                        id: "build".into(),
                        name: "Build".into(),
                        description: None,
                    },
                    AgentMode {
                        id: "plan".into(),
                        name: "Plan".into(),
                        description: None,
                    },
                ],
                config_option_id: Some("mode".into()),
            });
        });
        assert!(
            chat.read_with(cx, |chat, _| chat.mode_selectable()),
            "mode pill should be selectable on a fresh chat when a mode catalog exists"
        );
        chat.update(cx, |chat, _| {
            chat.mode_catalog = None;
        });
        assert!(
            !chat.read_with(cx, |chat, _| chat.mode_selectable()),
            "mode pill should not be selectable without a catalog"
        );
    }

    #[gpui::test]
    fn model_control_visible_with_models_before_first_turn(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));
        chat.update(cx, |chat, _| {
            chat.has_completed_turn = false;
            chat.available_models = vec![ModelOption {
                id: "opencode/big-pickle".into(),
                name: "Big Pickle".into(),
                description: None,
            }];
        });
        assert!(
            chat.read_with(cx, |chat, _| chat.model_control_visible()),
            "model control should be visible on a fresh chat with models"
        );
        chat.update(cx, |chat, _| {
            chat.available_models.clear();
        });
        assert!(
            !chat.read_with(cx, |chat, _| chat.model_control_visible()),
            "model control should be hidden when no models"
        );
    }
}
