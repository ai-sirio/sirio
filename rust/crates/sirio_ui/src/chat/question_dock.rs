//! The question dock: the one place an open agent question is answered,
//! drawn above the composer. This file holds the pure projection of the
//! transcript onto what the dock draws and the selection arithmetic the
//! keyboard uses; the design is
//! `docs/superpowers/specs/2026-09-24-question-dock-design.md`.

use crate::text_selection::selectable_text;
use ely_gpui_component::agent::{HumanInputRequest, PermissionPrompt};
use gpui::{AnyElement, Context, Focusable, KeyDownEvent, Window, actions, div, prelude::*, px};
use serde_json::Value;
use sirio_chat::{
    CloseReason, ElicitationAction, ElicitationField, ElicitationFieldKind, ElicitationPrompt,
    RequestKind, elicitation_value,
};
use sirio_theme::Theme;

actions!(
    chat_question_dock,
    [DockPrevious, DockNext, DockConfirm, DockCancel]
);

/// What the debug control verb does to the open question's dock, by the
/// path the matching key takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElicitationAct {
    /// A digit key or a click on the 1-based row.
    Row(usize),
    /// Enter in the free-text row.
    Text(String),
    /// Esc.
    Cancel,
}

use super::visible_text::{is_hidden, strip_escape_sequences, visible_text};
use super::{AnswerOption, AnswerTextInput, Chat, Entry, PlanApproval, TRANSCRIPT_WIDTH};

/// What the dock draws for the open question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct QuestionView {
    /// Handle the answer goes back on.
    pub(super) request_id: u64,
    /// The transcript entry the question is recorded in.
    pub(super) entry_index: usize,
    /// The small line above the question: its header, or what kind of
    /// question it is.
    pub(super) caption: String,
    /// The question itself, whole. Empty when the caption says it all.
    pub(super) body: String,
    /// What the CLI said about the ask, drawn under the body.
    pub(super) details: Vec<DockDetail>,
    /// Whether the digit keys and badges are on. Off for a card the CLI would
    /// rather have refused: a digit answers without looking.
    pub(super) digits: bool,
    /// The answers, in the order they are numbered.
    pub(super) rows: Vec<DockRow>,
    /// The field of a form the dock is on; 0 for everything else. Part of
    /// what a question is: the next field is a new question.
    pub(super) step: usize,
}

/// One line of what the CLI said about a permission ask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DockDetail {
    /// `Reason`, `Path` or `MCP server`; `None` for the description.
    pub(super) label: Option<String>,
    pub(super) text: String,
}

/// One numbered row of the dock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DockRow {
    /// An answer the agent offered.
    Option(AnswerOption),
    /// A typed answer, where the transport can take one.
    FreeText(AnswerTextInput),
    /// The way out of a request that offered nothing to choose.
    Dismiss,
    /// Leave an optional field of a form unanswered.
    Skip,
}

impl DockRow {
    /// What the row says, for the control surface and for tests.
    pub(super) fn label(&self) -> String {
        match self {
            Self::Option(option) => option.label.clone(),
            Self::FreeText(_) => "FreeText".into(),
            Self::Dismiss => "Dismiss".into(),
            Self::Skip => "Skip".into(),
        }
    }
}

/// Server text with its control and bidi characters removed; line breaks stay.
pub(super) fn clean_text(text: &str) -> String {
    text.chars()
        .filter(|character| *character == '\n' || !is_hidden(*character))
        .collect()
}

/// Server text for one line (a title, a label, a URL): `clean_text` without
/// the line breaks too.
pub(super) fn clean_line(text: &str) -> String {
    text.chars().filter(|character| !is_hidden(*character)).collect()
}

/// The host of a web link, lower-cased and without userinfo or port; `None`
/// for anything that is not an `http` or `https` URL with a host. The one
/// gate before a link goes to the system opener, so it reads the URL the
/// way a browser does: `\` ends the authority like `/`, and the host is what
/// follows the last `@`.
pub(super) fn web_url_host(url: &str) -> Option<String> {
    let url = clean_line(url);
    let (scheme, rest) = url.trim().split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let authority = rest.split(['/', '\\', '?', '#']).next()?;
    let host_and_port = authority.rsplit('@').next()?;
    let (host, port) = if host_and_port.starts_with('[') {
        let end = host_and_port.find(']')?;
        let (host, after) = host_and_port.split_at(end + 1);
        let inside = &host[1..end];
        if inside.is_empty() || !inside.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
        {
            return None;
        }
        (host, after.strip_prefix(':').or(after.is_empty().then_some("")))
    } else {
        match host_and_port.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (host_and_port, Some("")),
        }
    };
    if !port?.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let unusable = |c: char| {
        c.is_whitespace() || matches!(c, '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`' | '%' | ':')
    };
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if bare.is_empty() || (!host.starts_with('[') && host.chars().any(unusable)) {
        return None;
    }
    Some(host.to_lowercase())
}

const ROW_ACCEPT: &str = "elicitation-accept";
const ROW_DECLINE: &str = "elicitation-decline";
const ROW_OPEN: &str = "elicitation-open";

fn action_row(id: &str, label: &str, is_rejection: bool) -> DockRow {
    DockRow::Option(AnswerOption {
        id: id.into(),
        label: label.into(),
        description: None,
        is_rejection,
    })
}

/// How many of a field's rows are its own answers (one per choice, Yes and
/// No), before Skip and Decline.
fn answer_row_count(field: &ElicitationField) -> usize {
    match &field.kind {
        ElicitationFieldKind::Choice(choices) => choices.len(),
        ElicitationFieldKind::Boolean => 2,
        _ => 0,
    }
}

/// A given or default value as the text of a field.
fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// A field's value as the review shows it.
fn shown_value(field: &ElicitationField, value: &Value) -> String {
    match (&field.kind, value) {
        (ElicitationFieldKind::Choice(choices), Value::String(wire)) => choices
            .iter()
            .find(|choice| choice.value == *wire)
            .map_or_else(|| clean_line(wire), |choice| clean_line(&choice.label)),
        (ElicitationFieldKind::Boolean, Value::Bool(true)) => "Yes".into(),
        (ElicitationFieldKind::Boolean, Value::Bool(false)) => "No".into(),
        (_, other) => clean_line(&value_text(other)),
    }
}

/// What the dock draws for an elicitation, open or not.
struct ElicitationDock {
    caption: String,
    body: String,
    rows: Vec<DockRow>,
}

fn elicitation_dock(entry: &Entry) -> Option<ElicitationDock> {
    let Entry::Elicitation {
        server,
        message,
        prompt,
        step,
        values,
        ..
    } = entry
    else {
        return None;
    };
    let caption = format!("{} asks", clean_line(server));
    let mut lines = vec![clean_text(message)];
    let mut rows = Vec::new();
    match prompt {
        ElicitationPrompt::Form(fields) => match fields.get(*step) {
            Some(field) => {
                let title = clean_line(&field.title);
                lines.push(if field.required {
                    format!("{title} (required)")
                } else {
                    title.clone()
                });
                lines.extend(field.description.as_deref().map(clean_text));
                if let ElicitationFieldKind::Text {
                    format: Some(format),
                } = &field.kind
                {
                    lines.push(format!("Format: {}", clean_line(format)));
                }
                match &field.kind {
                    ElicitationFieldKind::Choice(choices) => rows.extend(choices.iter().map(
                        |choice| {
                            DockRow::Option(AnswerOption {
                                id: choice.value.clone(),
                                label: clean_line(&choice.label),
                                description: None,
                                is_rejection: false,
                            })
                        },
                    )),
                    ElicitationFieldKind::Boolean => {
                        rows.push(action_row("true", "Yes", false));
                        rows.push(action_row("false", "No", false));
                    }
                    _ => rows.push(DockRow::FreeText(AnswerTextInput {
                        placeholder: Some(title),
                        prefill: values
                            .get(&field.name)
                            .or(field.default.as_ref())
                            .map(|value| clean_line(&value_text(value))),
                    })),
                }
                if !field.required {
                    rows.push(DockRow::Skip);
                }
                rows.push(action_row(ROW_DECLINE, "Decline", true));
            }
            None => {
                lines.extend(fields.iter().filter_map(|field| {
                    values.get(&field.name).map(|value| {
                        format!("{}: {}", clean_line(&field.title), shown_value(field, value))
                    })
                }));
                rows.push(action_row(ROW_ACCEPT, "Accept", false));
                rows.push(action_row(ROW_DECLINE, "Decline", true));
            }
        },
        ElicitationPrompt::Url { url, .. } => {
            let url = clean_line(url);
            let host = web_url_host(&url);
            lines.extend(host.as_ref().map(|host| format!("Host: {host}")));
            lines.push(url.trim().to_string());
            if host.is_some() {
                rows.push(action_row(ROW_OPEN, "Open in browser", false));
            }
            rows.push(action_row(ROW_DECLINE, "Decline", true));
        }
        ElicitationPrompt::Unsupported { reason } => {
            lines.push(format!(
                "Sirio can't show this form: {}",
                clean_line(reason)
            ));
            rows.push(action_row(ROW_DECLINE, "Decline", true));
        }
    }
    lines.retain(|line| !line.is_empty());
    Some(ElicitationDock {
        caption,
        body: lines.join("\n"),
        rows,
    })
}

/// The row a question starts on. A permission the CLI would rather have
/// refused starts on its first rejection (Enter then declines); any other
/// entry starts where `elicitation_start_row` says.
pub(super) fn start_row(entry: &Entry) -> usize {
    match entry {
        Entry::Permission {
            options,
            is_question: false,
            detail: Some(detail),
            ..
        } if detail.default_to_no => options
            .iter()
            .position(|option| option.is_rejection)
            .unwrap_or(0),
        _ => elicitation_start_row(entry),
    }
}

/// The row a step starts on: a boolean's default, the choice already
/// given, otherwise the first.
pub(super) fn elicitation_start_row(entry: &Entry) -> usize {
    let Entry::Elicitation {
        prompt: ElicitationPrompt::Form(fields),
        step,
        values,
        ..
    } = entry
    else {
        return 0;
    };
    let Some(field) = fields.get(*step) else {
        return 0;
    };
    match &field.kind {
        ElicitationFieldKind::Boolean => {
            usize::from(values.get(&field.name).or(field.default.as_ref()) == Some(&Value::Bool(false)))
        }
        ElicitationFieldKind::Choice(choices) => values
            .get(&field.name)
            .or(field.default.as_ref())
            .and_then(Value::as_str)
            .and_then(|given| choices.iter().position(|choice| choice.value == given))
            .unwrap_or(0),
        _ => 0,
    }
}

/// What `control_entry_row` reports of an elicitation, besides its kind and id.
pub(super) fn elicitation_control_row(
    entry: &Entry,
    row: &mut std::collections::BTreeMap<String, String>,
) {
    let Entry::Elicitation {
        server,
        prompt,
        step,
        error,
        resolved,
        expired,
        withdrawn,
        completed,
        ..
    } = entry
    else {
        return;
    };
    let status = if *completed {
        "completed"
    } else if *withdrawn {
        "withdrawn"
    } else if let Some(resolved) = resolved {
        resolved
    } else if *expired {
        "expired"
    } else {
        "pending"
    };
    let (mode, fields, url) = match prompt {
        ElicitationPrompt::Form(fields) => (
            "form",
            fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>()
                .join("|"),
            String::new(),
        ),
        ElicitationPrompt::Url { url, .. } => ("url", String::new(), clean_line(url).trim().into()),
        ElicitationPrompt::Unsupported { .. } => ("unsupported", String::new(), String::new()),
    };
    let host = web_url_host(&url);
    let rows = if is_open(entry) {
        elicitation_dock(entry)
            .map(|dock| {
                dock.rows
                    .iter()
                    .map(DockRow::label)
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .unwrap_or_default()
    } else {
        String::new()
    };
    row.insert("status".into(), status.into());
    row.insert("mode".into(), mode.into());
    row.insert("step".into(), step.to_string());
    row.insert("fields".into(), fields);
    row.insert("rows".into(), rows);
    row.insert("error".into(), error.as_deref().map(clean_line).unwrap_or_default());
    row.insert("openOffered".into(), (mode == "url" && host.is_some() && is_open(entry)).to_string());
    row.insert("host".into(), host.unwrap_or_default());
    row.insert("url".into(), url);
    row.insert("server".into(), clean_line(server));
}

/// Whether an entry is a question still waiting on the user. The one
/// predicate `Chat::pending_question` and [`question_view`] share, so the
/// dock and the disabled composer cannot disagree about whether a question
/// is open.
pub(super) fn is_open(entry: &Entry) -> bool {
    matches!(
        entry,
        Entry::Permission {
            resolved: None,
            expired: false,
            ..
        } | Entry::Plan {
            approval: Some(PlanApproval {
                resolved: None,
                expired: false,
                ..
            }),
            ..
        } | Entry::Elicitation {
            resolved: None,
            expired: false,
            withdrawn: false,
            completed: false,
            ..
        }
    )
}

/// The first open question in the transcript, as the dock draws it.
pub(super) fn question_view(entries: &[Entry]) -> Option<QuestionView> {
    let (entry_index, entry) = entries.iter().enumerate().find(|(_, entry)| is_open(entry))?;
    match entry {
        Entry::Permission {
            request_id,
            title,
            prompt,
            options,
            text_input,
            is_question,
            detail,
            ..
        } => {
            let (mut caption, body) = match (*is_question, prompt.is_empty()) {
                (true, false) => (title.clone(), prompt.clone()),
                // Pi's question repeats its header, so the prompt was
                // blanked at parse time and the title is the question.
                (true, true) => ("Question".to_string(), title.clone()),
                (false, _) => (
                    "Permission requested".to_string(),
                    visible_text(title, true),
                ),
            };
            let mut details = Vec::new();
            let mut digits = true;
            if let Some(detail) = detail.as_ref().filter(|_| !*is_question) {
                if let Some(cli_title) = detail.title.as_deref().filter(|t| !t.trim().is_empty()) {
                    caption = visible_text(cli_title, false);
                }
                let mut push = |label: Option<&str>, text: String| {
                    details.push(DockDetail {
                        label: label.map(str::to_string),
                        text,
                    });
                };
                if let Some(description) = detail
                    .description
                    .as_deref()
                    .map(|text| visible_text(text, true))
                    .filter(|text| !text.trim().is_empty() && text.trim() != body.trim())
                {
                    push(None, description);
                }
                if let Some(reason) = detail
                    .reason
                    .as_deref()
                    .map(|text| visible_text(&strip_escape_sequences(text), true))
                    .filter(|text| !text.trim().is_empty())
                {
                    push(Some("Reason"), reason);
                }
                if let Some(path) = detail.path.as_deref().filter(|p| !p.trim().is_empty()) {
                    push(Some("Path"), visible_text(path, false));
                }
                if let Some(server) = &detail.mcp_server {
                    push(
                        Some("MCP server"),
                        visible_text(&format!("{} ({})", server.name, server.source), false),
                    );
                }
                digits = !detail.default_to_no;
            }
            let mut rows = options
                .iter()
                .cloned()
                .map(DockRow::Option)
                .collect::<Vec<_>>();
            if let Some(input) = text_input {
                rows.push(DockRow::FreeText(input.clone()));
            }
            if rows.is_empty() {
                rows.push(DockRow::Dismiss);
            }
            Some(QuestionView {
                request_id: *request_id,
                entry_index,
                caption,
                body,
                details,
                digits,
                rows,
                step: 0,
            })
        }
        Entry::Elicitation { request_id, step, .. } => {
            let dock = elicitation_dock(entry)?;
            Some(QuestionView {
                request_id: *request_id,
                entry_index,
                caption: dock.caption,
                body: dock.body,
                details: Vec::new(),
                digits: true,
                rows: dock.rows,
                step: *step,
            })
        }
        Entry::Plan {
            approval: Some(approval),
            ..
        } => Some(QuestionView {
            request_id: approval.request_id,
            entry_index,
            caption: "Plan approval".to_string(),
            body: approval.title.clone(),
            details: Vec::new(),
            digits: true,
            rows: approval.options.iter().cloned().map(DockRow::Option).collect(),
            step: 0,
        }),
        _ => None,
    }
}

/// The selected row, kept inside the rows actually drawn.
pub(super) fn clamp_selection(selected: usize, row_count: usize) -> usize {
    selected.min(row_count.saturating_sub(1))
}

/// One step up or down from `selected`. It stops at either end rather than
/// wrapping: Down from the last of three landing on the first reads as a
/// jump, not a step.
pub(super) fn step_selection(selected: usize, row_count: usize, down: bool) -> usize {
    let selected = clamp_selection(selected, row_count);
    if down {
        clamp_selection(selected + 1, row_count)
    } else {
        selected.saturating_sub(1)
    }
}

/// The row a digit key picks: `1` is the first row. `0`, a digit past the
/// last row, and any key that is not a single digit pick nothing.
pub(super) fn row_for_digit(key: &str, row_count: usize) -> Option<usize> {
    let digit = key
        .parse::<usize>()
        .ok()
        .filter(|digit| (1..=9).contains(digit))?;
    (digit <= row_count).then(|| digit - 1)
}

/// The number drawn on row `index`, for the nine rows a digit reaches.
pub(super) fn badge_label(index: usize) -> Option<String> {
    (index < 9).then(|| (index + 1).to_string())
}

/// Tallest the question body grows before it scrolls, so a long command
/// cannot push the composer off the pane.
pub(super) const DOCK_BODY_MAX_HEIGHT: f32 = 160.0;

/// A keystroke or click already in flight when the question appears must not answer it.
pub(super) const DOCK_ARMING_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

impl Chat {
    /// The dock as `surface.chat.read` reports it: `""` when no question is
    /// open, otherwise the JSON of what the dock paints.
    pub(super) fn question_dock_json(&self) -> String {
        let Some(view) = question_view(&self.entries) else {
            return String::new();
        };
        let selected = if self.question_dock.for_request == Some((view.request_id, view.step)) {
            self.question_dock.selected
        } else {
            self.entries.get(view.entry_index).map_or(0, start_row)
        };
        serde_json::json!({
            "caption": view.caption,
            "body": view.body,
            "details": view
                .details
                .iter()
                .map(|detail| serde_json::json!({"label": detail.label, "text": detail.text}))
                .collect::<Vec<_>>(),
            "options": view
                .rows
                .iter()
                .map(|row| match row {
                    DockRow::Option(option) => {
                        serde_json::json!({"id": option.id, "label": option.label})
                    }
                    other => serde_json::json!({"id": "", "label": other.label()}),
                })
                .collect::<Vec<_>>(),
            "selected": clamp_selection(selected, view.rows.len()),
            "digits": view.digits,
        })
        .to_string()
    }

    /// Keeps the dock's selection and focus in step with the open question.
    /// A new question starts on row 1 and takes the keyboard — but only
    /// from inside this chat, never from another pane. When the last
    /// question closes while the dock or its field held the keyboard, the
    /// keyboard goes back to the composer.
    pub(super) fn sync_question_dock(
        &mut self,
        view: Option<&QuestionView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let composer = self.composer_field.read(cx).focus_handle(cx);
        let dock_or_field_focused = self.question_dock.focus.is_focused(window)
            || self.question_answer.focus.is_focused(window);
        match view {
            Some(view)
                if self.question_dock.for_request != Some((view.request_id, view.step)) =>
            {
                self.question_dock.for_request = Some((view.request_id, view.step));
                self.question_dock.shown_at = Some(cx.background_executor().now());
                self.question_dock.selected = self
                    .entries
                    .get(view.entry_index)
                    .map_or(0, start_row);
                let focus_in_chat = dock_or_field_focused
                    || composer.contains_focused(window, cx)
                    || self.transcript_focus.is_focused(window);
                // A message half typed in the composer stays there, and focus
                // with it: handing focus to the dock or the field would let
                // the rest of the message (a digit, an Enter) answer an MCP
                // server. The composer is disabled while a question is
                // pending: it reverts every edit, and no key typed there
                // reaches the dock or the field. The user answers by
                // clicking the dock.
                let typing_a_message = matches!(
                    self.entries.get(view.entry_index),
                    Some(Entry::Elicitation { .. })
                ) && composer.contains_focused(window, cx)
                    && !self.composer_field.read(cx).text().is_empty();
                if focus_in_chat && !typing_a_message {
                    // A free-text step starts on its field: typed characters
                    // are text there, never the dock's row shortcuts.
                    let free_text = match self.entries.get(view.entry_index) {
                        Some(entry @ Entry::Elicitation { .. }) => {
                            match view.rows.get(elicitation_start_row(entry)) {
                                Some(DockRow::FreeText(input)) => Some(input.prefill.clone()),
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    let key = (view.request_id, view.step);
                    if let Some(prefill) = free_text {
                        cx.defer_in(window, move |chat, window, cx| {
                            if chat.question_dock.for_request == Some(key) {
                                chat.focus_question_answer(key.0, prefill, window, cx);
                            }
                        });
                    } else {
                        let dock = self.question_dock.focus.clone();
                        cx.defer_in(window, move |_, window, cx| dock.focus(window, cx));
                    }
                }
            }
            None if self.question_dock.for_request.is_some() => {
                self.question_dock.for_request = None;
                self.question_dock.shown_at = None;
                self.question_dock.selected = 0;
                if dock_or_field_focused {
                    cx.defer_in(window, move |_, window, cx| composer.focus(window, cx));
                }
            }
            _ => {}
        }
    }

    /// Whether the open question's option rows accept activation: the dock
    /// has been drawn for this very request (`sync_question_dock` took it)
    /// and `DOCK_ARMING_DELAY` has passed since. Until then an Enter, digit
    /// or click is ignored — and moves nothing — because it may have been
    /// meant for the question before, or be a key already in flight.
    fn dock_armed(&self, view: &QuestionView, cx: &Context<Self>) -> bool {
        self.question_dock.for_request == Some((view.request_id, view.step))
            && self.question_dock.shown_at.is_some_and(|shown_at| {
                cx.background_executor()
                    .now()
                    .saturating_duration_since(shown_at)
                    >= DOCK_ARMING_DELAY
            })
    }

    /// Enter in the answer field of `request_id`. An open elicitation takes
    /// it only once the dock is armed for the current step: an Enter meant
    /// for the step before must not submit an empty answer here.
    pub(super) fn submit_answer_field(&mut self, request_id: u64, cx: &mut Context<Self>) {
        if let Some(view) = question_view(&self.entries)
            && view.request_id == request_id
            && self.open_elicitation_index(request_id).is_some()
            && !self.dock_armed(&view, cx)
        {
            return;
        }
        let draft = self.question_answer.draft.clone();
        self.answer_question_text(request_id, &draft, cx);
    }

    /// Moves the selection to row `index`, redrawing only on a change — a
    /// pointer resting on a row costs no frames.
    pub(super) fn select_dock_row(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.question_dock.selected != index {
            self.question_dock.selected = index;
            cx.notify();
        }
    }

    fn step_dock(&mut self, down: bool, cx: &mut Context<Self>) {
        if let Some(view) = question_view(&self.entries) {
            let next = step_selection(self.question_dock.selected, view.rows.len(), down);
            self.select_dock_row(next, cx);
        }
    }

    pub(super) fn dock_previous(
        &mut self,
        _: &DockPrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_dock(false, cx);
    }

    pub(super) fn dock_next(
        &mut self,
        _: &DockNext,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_dock(true, cx);
    }

    pub(super) fn dock_confirm(
        &mut self,
        _: &DockConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = question_view(&self.entries) {
            let index = clamp_selection(self.question_dock.selected, view.rows.len());
            self.activate_dock_row(index, window, cx);
        }
    }

    pub(super) fn dock_cancel(
        &mut self,
        _: &DockCancel,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = question_view(&self.entries) {
            self.cancel_question(view.request_id, cx);
        }
    }

    /// Digits `1`–`9` pick their row. Keys typed into the free-text field
    /// bubble through the dock too, and there a digit is text: the field
    /// owns it.
    pub(super) fn on_question_dock_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.question_answer.focus.is_focused(window) || event.keystroke.modifiers.modified() {
            return;
        }
        let Some(view) = question_view(&self.entries) else {
            return;
        };
        if !view.digits {
            return;
        }
        if let Some(index) = row_for_digit(&event.keystroke.key, view.rows.len()) {
            cx.stop_propagation();
            self.activate_dock_row(index, window, cx);
        }
    }

    /// Up from the typed answer: back to the dock, on the last listed
    /// answer.
    pub(super) fn leave_answer_action(
        &mut self,
        _: &super::LeaveAnswer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = question_view(&self.entries) else {
            return;
        };
        self.question_answer.for_request = None;
        self.question_dock.selected = if matches!(
            self.entries.get(view.entry_index),
            Some(Entry::Elicitation { .. })
        ) {
            // The only listed rows of a field are Skip and Decline, below
            // the field: Up goes back onto the field's own row.
            view.rows
                .iter()
                .position(|row| matches!(row, DockRow::FreeText(_)))
                .unwrap_or(0)
        } else {
            view.rows
                .iter()
                .rposition(|row| matches!(row, DockRow::Option(_)))
                .unwrap_or(0)
        };
        self.question_dock.focus.focus(window, cx);
        cx.notify();
    }

    /// Draws the dock for the open question: its caption, the whole
    /// question, and one numbered row per answer.
    pub(super) fn render_question_dock(
        &self,
        view: QuestionView,
        theme: &Theme,
        bezel_theme: &bezel_theme::Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let typography = theme.typography;
        let entity = cx.entity();
        let request_id = view.request_id;
        let step = view.step;
        let error = match self.entries.get(view.entry_index) {
            Some(Entry::Elicitation { error, .. }) => error.as_deref().map(clean_line),
            _ => None,
        };
        let selected = clamp_selection(self.question_dock.selected, view.rows.len());
        let after_options = view
            .rows
            .iter()
            .any(|row| matches!(row, DockRow::Option(_)));
        let digits = view.digits;

        let rows = view
            .rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let highlighted = index == selected;
                let badge = badge_label(index).filter(|_| digits).map(|label| {
                    div()
                        .flex_none()
                        .w(px(20.0))
                        .h(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(theme.radii.control)
                        .border_1()
                        .border_color(bezel_theme.border)
                        .when(highlighted, |badge| badge.bg(theme.sirio.border_strong))
                        .text_size(typography.caption2)
                        .text_color(if highlighted {
                            theme.ely.fg
                        } else {
                            theme.ely.fg_subtle
                        })
                        .child(label)
                });
                let click_entity = entity.clone();
                // Host-owned selection/focus; Ely supplies the request shell.
                let frame = div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .gap(px(8.0))
                    .p(px(8.0))
                    .rounded(theme.radii.control)
                    .bg(if highlighted {
                        theme.ely.active
                    } else {
                        gpui::transparent_black()
                    })
                    .cursor_pointer()
                    .hover(|row| row.bg(theme.ely.hover))
                    .when(!highlighted, |row| row.bg(gpui::transparent_black()))
                    .id(("question-dock-row", index))
                    .items_start()
                    .on_mouse_move({
                        let hover_entity = entity.clone();
                        move |_, _, cx| {
                            hover_entity.update(cx, |chat, cx| chat.select_dock_row(index, cx));
                        }
                    })
                    .on_click(move |_, window, cx| {
                        click_entity.update(cx, |chat, cx| {
                            if question_view(&chat.entries).is_some_and(|view| {
                                view.request_id == request_id && view.step == step
                            }) {
                                chat.activate_dock_row(index, window, cx);
                            }
                        });
                    })
                    .children(badge);
                match row {
                    DockRow::Option(option) => {
                        let option_id = option.id.clone();
                        frame
                            .debug_selector(move || format!("permission-option-{option_id}"))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    // A rejection is not tinted: a red "Reject"
                                    // reads as an error on a legitimate choice.
                                    .child(
                                        div()
                                            .text_size(typography.callout)
                                            .text_color(theme.ely.fg)
                                            .child(option.label),
                                    )
                                    .children(option.description.map(|description| {
                                        div()
                                            .text_size(typography.footnote)
                                            .text_color(theme.ely.fg_muted)
                                            .child(description)
                                    })),
                            )
                            .into_any_element()
                    }
                    DockRow::FreeText(input) => frame
                        .debug_selector(|| "question-dock-other".into())
                        .child(Self::render_question_answer_row(
                            request_id,
                            input.placeholder(after_options),
                            input.prefill.clone(),
                            theme,
                            entity.clone(),
                            &self.question_answer,
                            self.answer_caret_visible,
                        ))
                        .into_any_element(),
                    DockRow::Skip => frame
                        .debug_selector(|| "question-dock-skip".into())
                        .child(
                            div()
                                .flex_1()
                                .text_size(typography.callout)
                                .text_color(theme.ely.fg)
                                .child("Skip"),
                        )
                        .into_any_element(),
                    DockRow::Dismiss => frame
                        .debug_selector(|| "question-dock-dismiss".into())
                        .child(
                            div()
                                .flex_1()
                                .text_size(typography.callout)
                                .text_color(theme.ely.fg)
                                .child("Dismiss"),
                        )
                        .into_any_element(),
                }
            })
            .collect::<Vec<_>>();

        // The details scroll with the body inside the one capped area, so a
        // long reason cannot push the answers off the pane.
        let details = view
            .details
            .into_iter()
            .map(|detail| {
                div()
                    .debug_selector(|| "question-dock-detail".into())
                    .mt(px(4.0))
                    .text_size(typography.footnote)
                    .text_color(theme.ely.fg_muted)
                    .child(selectable_text(match detail.label {
                        Some(label) => format!("{label}: {}", detail.text),
                        None => detail.text,
                    }))
            })
            .collect::<Vec<_>>();
        let body = div()
            .w_full()
            .min_w_0()
            .when(!view.body.is_empty() || !details.is_empty(), |body| {
                body.child(
                    div()
                        .id(("question-dock-body", request_id as usize))
                        .debug_selector(|| "question-dock-body".into())
                        .max_h(px(DOCK_BODY_MAX_HEIGHT))
                        .overflow_y_scroll()
                        .text_size(typography.callout)
                        .text_color(theme.ely.fg)
                        .when(!view.body.is_empty(), |area| {
                            area.child(selectable_text(view.body))
                        })
                        .children(details),
                )
            })
            .children(error.map(|error| {
                div()
                    .debug_selector(|| "question-dock-error".into())
                    .text_size(typography.footnote)
                    .text_color(theme.ely.danger)
                    .child(selectable_text(error))
            }));
        let actions = div()
            .id(("question-dock-options", request_id as usize))
            .w_full()
            .min_w_0()
            .max_h(px(220.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .children(rows)
            .child(
                div()
                    .debug_selector(|| "question-dock-hint".into())
                    .flex()
                    .justify_end()
                    .text_size(typography.caption2)
                    .text_color(theme.ely.fg_subtle)
                    .child("↑↓ select · ⏎ confirm · esc cancel"),
            );
        let id: gpui::ElementId = (
            self.row_id(view.entry_index, cx),
            format!("request-{request_id}"),
        )
            .into();
        let is_question = matches!(
            self.entries.get(view.entry_index),
            Some(Entry::Permission {
                is_question: true,
                ..
            }) | Some(Entry::Elicitation { .. })
        );
        let shell = if is_question {
            HumanInputRequest::custom(id, view.caption)
                .header_selector("question-dock-caption")
                .body(body)
                .action(actions)
                .into_any_element()
        } else {
            PermissionPrompt::custom(id, view.caption)
                .header_selector("question-dock-caption")
                .body(body)
                .action(actions)
                .into_any_element()
        };
        div()
            .id("question-dock")
            .debug_selector(|| "question-dock".into())
            .key_context("ChatQuestionDock")
            .track_focus(&self.question_dock.focus)
            .on_action(cx.listener(Self::dock_previous))
            .on_action(cx.listener(Self::dock_next))
            .on_action(cx.listener(Self::dock_confirm))
            .on_action(cx.listener(Self::dock_cancel))
            .on_key_down(cx.listener(Self::on_question_dock_key))
            .w_full()
            .max_w(px(TRANSCRIPT_WIDTH))
            .mb(px(8.0))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .debug_selector(move || format!("ely-request-{request_id}"))
                    .child(shell),
            )
            .into_any_element()
    }

    /// Acts on row `index` of the open question: answers with an option,
    /// opens the typed answer, or dismisses a request with nothing to
    /// choose.
    pub(super) fn activate_dock_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = question_view(&self.entries) else {
            return;
        };
        let Some(row) = view.rows.get(index).cloned() else {
            return;
        };
        if matches!(row, DockRow::Option(_) | DockRow::Skip) && !self.dock_armed(&view, cx) {
            return;
        }
        self.question_dock.selected = index;
        if matches!(
            self.entries.get(view.entry_index),
            Some(Entry::Elicitation { .. })
        ) {
            self.activate_elicitation_row(view.entry_index, index, row, window, cx);
            return;
        }
        match row {
            DockRow::Option(option) => self.respond_permission(view.request_id, &option, cx),
            DockRow::FreeText(input) => {
                self.focus_question_answer(view.request_id, input.prefill, window, cx)
            }
            DockRow::Dismiss => self.dismiss_permission(view.request_id, cx),
            DockRow::Skip => {}
        }
    }

    /// Debug control verb: does to the open question `request_id` what a
    /// user's key does. Refuses a request that is not the question the dock is
    /// showing, and a row key the dock would still ignore as unarmed.
    pub fn elicitation_act(
        &mut self,
        request_id: u64,
        act: ElicitationAct,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let view = question_view(&self.entries)
            .filter(|view| view.request_id == request_id)
            .ok_or_else(|| format!("request {request_id} is not the open question"))?;
        match act {
            ElicitationAct::Row(number) => {
                let index = number
                    .checked_sub(1)
                    .filter(|index| *index < view.rows.len())
                    .ok_or_else(|| {
                        format!("the dock has {} rows, not row {number}", view.rows.len())
                    })?;
                // A frame shows the dock and starts its arming clock; a run
                // that paints none (E2E --state-only) gets the same from here.
                // On a painted run this is a no-op: the frame did it already.
                self.sync_question_dock(Some(&view), window, cx);
                if matches!(view.rows[index], DockRow::Option(_) | DockRow::Skip)
                    && !self.dock_armed(&view, cx)
                {
                    return Err("the dock is not armed yet; retry".to_string());
                }
                self.activate_dock_row(index, window, cx);
            }
            ElicitationAct::Text(text) => {
                if !view.rows.iter().any(|row| matches!(row, DockRow::FreeText(_))) {
                    return Err("the dock has no free-text row".to_string());
                }
                self.answer_question_text(request_id, &text, cx);
            }
            ElicitationAct::Cancel => self.dock_cancel(&DockCancel, window, cx),
        }
        Ok(())
    }

    /// The transcript index of the open question the dock is on, when its
    /// request id is `request_id`. Answers from the dock act on this entry,
    /// not on the last entry that happens to share the id.
    pub(super) fn dock_entry_index(&self, request_id: u64) -> Option<usize> {
        question_view(&self.entries)
            .filter(|view| view.request_id == request_id)
            .map(|view| view.entry_index)
    }

    /// The transcript index of the open elicitation `request_id`, when it is
    /// the question the dock is on. Typed answers and Esc ask this before
    /// anything else: a permission may share the id.
    pub(super) fn open_elicitation_index(&self, request_id: u64) -> Option<usize> {
        let view = question_view(&self.entries)?;
        (view.request_id == request_id
            && matches!(
                self.entries.get(view.entry_index),
                Some(Entry::Elicitation { .. })
            ))
        .then_some(view.entry_index)
    }

    /// Acts on row `index` of the open elicitation at `entry_index`: an
    /// answer to the current field, Skip, Accept, Decline, or the link.
    fn activate_elicitation_row(
        &mut self,
        entry_index: usize,
        index: usize,
        row: DockRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Entry::Elicitation {
            request_id,
            prompt,
            step,
            values,
            ..
        }) = self.entries.get(entry_index).cloned()
        else {
            return;
        };
        let decline = |chat: &mut Self, cx: &mut Context<Self>| {
            chat.respond_elicitation(request_id, ElicitationAction::Decline, cx)
        };
        let id = match &row {
            DockRow::Option(option) => Some(option.id.as_str()),
            _ => None,
        };
        match &prompt {
            ElicitationPrompt::Form(fields) => match fields.get(step) {
                Some(field) => match row {
                    DockRow::FreeText(input) => {
                        self.focus_question_answer(request_id, input.prefill, window, cx)
                    }
                    DockRow::Skip => self.advance_elicitation(entry_index, Ok(None), cx),
                    DockRow::Option(_) if index < answer_row_count(field) => {
                        // The option's own value, never its label: a label that
                        // equals another option's value must not pick that one.
                        let value = match &field.kind {
                            ElicitationFieldKind::Choice(choices) => {
                                Value::String(choices[index].value.clone())
                            }
                            _ => Value::Bool(index == 0),
                        };
                        self.advance_elicitation(entry_index, Ok(Some(value)), cx);
                    }
                    DockRow::Option(_) if id == Some(ROW_DECLINE) => decline(self, cx),
                    _ => {}
                },
                None => match id {
                    Some(ROW_ACCEPT) => {
                        self.respond_elicitation(request_id, ElicitationAction::Accept(values), cx)
                    }
                    Some(ROW_DECLINE) => decline(self, cx),
                    _ => {}
                },
            },
            ElicitationPrompt::Url { url, .. } => match id {
                Some(ROW_OPEN) => {
                    let url = clean_line(url);
                    // The gate again, at the moment of the click.
                    if web_url_host(&url).is_some() {
                        cx.open_url(url.trim());
                        self.respond_elicitation(
                            request_id,
                            ElicitationAction::Accept(serde_json::Map::new()),
                            cx,
                        );
                    }
                }
                Some(ROW_DECLINE) => decline(self, cx),
                _ => {}
            },
            ElicitationPrompt::Unsupported { .. } => {
                if id == Some(ROW_DECLINE) {
                    decline(self, cx);
                }
            }
        }
    }

    /// Takes a typed answer to the current field of the open elicitation.
    pub(super) fn answer_elicitation_text(
        &mut self,
        request_id: u64,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.open_elicitation_index(request_id) else {
            return;
        };
        let Some(Entry::Elicitation {
            prompt: ElicitationPrompt::Form(fields),
            step,
            ..
        }) = self.entries.get(index)
        else {
            return;
        };
        let Some(field) = fields.get(*step) else {
            return;
        };
        let value = elicitation_value(field, text);
        self.advance_elicitation(index, value, cx);
    }

    /// Records one step's outcome: a value moves on and is kept, an empty
    /// optional moves on, an error stays on the step and says why.
    fn advance_elicitation(
        &mut self,
        index: usize,
        outcome: Result<Option<Value>, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(Entry::Elicitation {
            prompt: ElicitationPrompt::Form(fields),
            step,
            values,
            error,
            ..
        }) = self.entries.get_mut(index)
        else {
            return;
        };
        let Some(field) = fields.get(*step) else {
            return;
        };
        match outcome {
            Err(message) => *error = Some(message),
            Ok(value) => {
                if let Some(value) = value {
                    values.insert(field.name.clone(), value);
                }
                *error = None;
                *step += 1;
                // A draft typed for the last field is not the next one's.
                self.clear_question_answer_focus();
            }
        }
        self.remeasure_entry(index);
        cx.notify();
    }

    /// Answers the open elicitation `request_id`, records what the user
    /// chose on its card and tells the agent. A request that is closed (answered,
    /// expired or withdrawn) takes nothing and sends nothing.
    pub(super) fn respond_elicitation(
        &mut self,
        request_id: u64,
        action: ElicitationAction,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.entries.iter().rposition(|entry| {
            is_open(entry)
                && matches!(entry, Entry::Elicitation { request_id: id, .. } if *id == request_id)
        }) else {
            return;
        };
        self.answer_elicitation_at(index, action, cx);
    }

    /// Answers the open elicitation at `index`: records the answer on its
    /// card and tells the agent. The typed draft is cleared only when this
    /// is the card the dock is on; a server event answering a later card
    /// leaves the draft of the question being answered alone.
    fn answer_elicitation_at(
        &mut self,
        index: usize,
        action: ElicitationAction,
        cx: &mut Context<Self>,
    ) {
        if !is_open(&self.entries[index]) {
            return;
        }
        let was_on_dock = question_view(&self.entries).is_some_and(|view| view.entry_index == index);
        let Entry::Elicitation {
            request_id,
            resolved,
            ..
        } = &mut self.entries[index]
        else {
            return;
        };
        let request_id = *request_id;
        *resolved = Some(match action {
            ElicitationAction::Accept(_) => "accepted",
            ElicitationAction::Decline => "declined",
            ElicitationAction::Cancel => "cancelled",
        });
        self.remeasure_entry(index);
        if let Some(client) = &self.client {
            let _ = client.respond_elicitation(request_id, action);
        }
        if was_on_dock {
            self.clear_question_answer_focus();
        }
        cx.notify();
    }

    /// The agent closed request `request_id` of `kind`: the latest open entry
    /// of that kind with that id is closed, and no other.
    pub(super) fn close_request(
        &mut self,
        request_id: u64,
        kind: RequestKind,
        reason: CloseReason,
    ) {
        let withdrawn = matches!(reason, CloseReason::Withdrawn);
        let position = self.entries.iter().rposition(|entry| match (kind, entry) {
            (RequestKind::Elicitation, Entry::Elicitation { request_id: id, .. }) => {
                *id == request_id && is_open(entry)
            }
            (RequestKind::Permission, Entry::Permission { request_id: id, .. }) => {
                *id == request_id && is_open(entry)
            }
            (
                RequestKind::Permission,
                Entry::Plan {
                    approval: Some(approval),
                    ..
                },
            ) => approval.request_id == request_id && is_open(entry),
            _ => false,
        });
        let Some(index) = position else {
            return;
        };
        let was_on_dock = question_view(&self.entries).is_some_and(|view| view.entry_index == index);
        match &mut self.entries[index] {
            Entry::Elicitation {
                withdrawn: was_withdrawn,
                expired,
                timed_out,
                ..
            } => {
                if withdrawn {
                    *was_withdrawn = true;
                } else {
                    *expired = true;
                    *timed_out = true;
                }
            }
            Entry::Permission {
                withdrawn: was_withdrawn,
                expired,
                ..
            } => {
                *expired = true;
                *was_withdrawn = withdrawn;
            }
            Entry::Plan {
                approval: Some(approval),
                ..
            } => {
                approval.expired = true;
                approval.withdrawn = withdrawn;
            }
            _ => {}
        }
        self.remeasure_entry(index);
        if was_on_dock {
            self.clear_question_answer_focus();
        }
    }

    /// The server finished the page flow of the URL elicitation `elicitation_id`.
    ///
    /// A card still waiting for its answer is accepted first, as a click on
    /// "Open in browser" would: the user finished the flow by hand, the worker
    /// still holds the request, and a URL accept carries no data.
    pub(super) fn complete_elicitation(
        &mut self,
        server: &str,
        elicitation_id: &str,
        cx: &mut Context<Self>,
    ) {
        let position = self.entries.iter().rposition(|entry| {
            matches!(
                entry,
                Entry::Elicitation {
                    server: entry_server,
                    prompt: ElicitationPrompt::Url { elicitation_id: Some(id), .. },
                    ..
                } if entry_server == server && id == elicitation_id
            )
        });
        if let Some(index) = position {
            self.answer_elicitation_at(
                index,
                ElicitationAction::Accept(serde_json::Map::new()),
                cx,
            );
            // Only a card that took the accept (just now, or earlier) reads as
            // completed: a declined, cancelled, expired or withdrawn one got
            // `decline` or `cancel`, and says so.
            if let Entry::Elicitation {
                completed,
                resolved: Some("accepted"),
                ..
            } = &mut self.entries[index]
            {
                *completed = true;
            }
            self.remeasure_entry(index);
            cx.notify();
        }
    }
}

/// Options with the given labels; each id is its label, lower-cased.
#[cfg(test)]
pub(super) fn answer_options(labels: &[&str]) -> Vec<AnswerOption> {
    labels
        .iter()
        .map(|label| AnswerOption {
            id: label.to_lowercase(),
            label: (*label).to_string(),
            description: None,
            is_rejection: false,
        })
        .collect()
}

/// A plain permission, still open.
#[cfg(test)]
pub(super) fn open_permission(request_id: u64, title: &str, labels: &[&str]) -> Entry {
    Entry::Permission {
        request_id,
        title: title.to_string(),
        prompt: String::new(),
        options: answer_options(labels),
        text_input: None,
        is_question: false,
        resolved: None,
        expired: false,
        dismissed: false,
        withdrawn: false,
        detail: None,
    }
}

/// A fresh elicitation, on its first step.
#[cfg(test)]
pub(super) fn open_elicitation(request_id: u64, prompt: sirio_chat::ElicitationPrompt) -> Entry {
    Entry::Elicitation {
        request_id,
        server: "elic".into(),
        message: "Pick a colour for the probe".into(),
        prompt,
        step: 0,
        values: serde_json::Map::new(),
        error: None,
        resolved: None,
        expired: false,
        timed_out: false,
        withdrawn: false,
        completed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::PlanEntryRow;
    use super::*;

    fn question(
        request_id: u64,
        title: &str,
        prompt: &str,
        labels: &[&str],
        text_input: Option<AnswerTextInput>,
    ) -> Entry {
        Entry::Permission {
            request_id,
            title: title.to_string(),
            prompt: prompt.to_string(),
            options: answer_options(labels),
            text_input,
            is_question: true,
            resolved: None,
            expired: false,
            dismissed: false,
            withdrawn: false,
            detail: None,
        }
    }

    #[test]
    fn the_first_open_question_is_the_one_drawn() {
        let mut answered = open_permission(1, "first", &["Allow"]);
        if let Entry::Permission { resolved, .. } = &mut answered {
            *resolved = Some("Allow".into());
        }
        let entries = vec![
            answered,
            open_permission(2, "second", &["Allow"]),
            open_permission(3, "third", &["Allow"]),
        ];
        let view = question_view(&entries).expect("an open question is drawn");
        assert_eq!((view.request_id, view.entry_index), (2, 1));
    }

    #[test]
    fn nothing_is_drawn_when_no_question_is_open() {
        let mut expired = open_permission(1, "expired", &["Allow"]);
        if let Entry::Permission { expired: flag, .. } = &mut expired {
            *flag = true;
        }
        let mut dismissed = open_permission(2, "dismissed", &[]);
        if let Entry::Permission {
            expired, dismissed, ..
        } = &mut dismissed
        {
            *expired = true;
            *dismissed = true;
        }
        let entries = [expired, dismissed, Entry::TurnFooter("12:00".into())];
        assert!(entries.iter().all(|entry| !is_open(entry)));
        assert_eq!(question_view(&entries), None);
    }

    #[test]
    fn a_question_with_a_prompt_is_captioned_by_its_header() {
        let view = question_view(&[question(
            1,
            "Approach",
            "Which layout should the panel use?",
            &["Docked"],
            None,
        )])
        .unwrap();
        assert_eq!(view.caption, "Approach");
        assert_eq!(view.body, "Which layout should the panel use?");
    }

    #[test]
    fn a_question_that_repeats_its_header_is_captioned_question() {
        // Pi: the prompt was blanked because it repeated the header, so the
        // title is the question.
        let view = question_view(&[question(
            1,
            "Which color should the button be?",
            "",
            &["Blue"],
            None,
        )])
        .unwrap();
        assert_eq!(view.caption, "Question");
        assert_eq!(view.body, "Which color should the button be?");
    }

    #[test]
    fn a_plain_permission_is_captioned_permission_requested() {
        let view = question_view(&[open_permission(
            1,
            "/repo/src/main.rs",
            &["Allow", "Reject"],
        )])
        .unwrap();
        assert_eq!(view.caption, "Permission requested");
        assert_eq!(view.body, "/repo/src/main.rs");
        assert_eq!(
            view.rows,
            answer_options(&["Allow", "Reject"])
                .into_iter()
                .map(DockRow::Option)
                .collect::<Vec<_>>(),
            "a plain permission never offers free text"
        );
    }

    #[test]
    fn a_plan_approval_is_drawn_with_its_options() {
        let plan = Entry::Plan {
            entries: vec![PlanEntryRow {
                content: "Read the design".into(),
                status: "pending".into(),
            }],
            approval: Some(PlanApproval {
                request_id: 4,
                title: "Ready to code?".into(),
                options: answer_options(&["Approve", "Keep planning"]),
                resolved: None,
                expired: false,
                withdrawn: false,
            }),
        };
        assert!(is_open(&plan));
        let view = question_view(&[plan]).unwrap();
        assert_eq!(
            (view.request_id, view.caption.as_str(), view.body.as_str()),
            (4, "Plan approval", "Ready to code?")
        );
        assert_eq!(view.rows.len(), 2);
    }

    #[test]
    fn free_text_is_offered_exactly_when_the_question_declares_it() {
        let without = question_view(&[question(1, "Pick", "Which one?", &["A", "B"], None)])
            .unwrap();
        assert!(
            !without
                .rows
                .iter()
                .any(|row| matches!(row, DockRow::FreeText(_))),
            "a question that declared no field gets none"
        );
        let input = AnswerTextInput {
            placeholder: None,
            prefill: None,
        };
        let with = question_view(&[question(
            1,
            "Pick",
            "Which one?",
            &["A", "B"],
            Some(input.clone()),
        )])
        .unwrap();
        assert_eq!(with.rows.len(), 3);
        assert_eq!(
            with.rows.last(),
            Some(&DockRow::FreeText(input)),
            "the typed answer is the last row"
        );
    }

    #[test]
    fn a_request_with_nothing_to_choose_offers_dismiss() {
        let view = question_view(&[open_permission(1, "mystery", &[])]).unwrap();
        assert_eq!(view.rows, vec![DockRow::Dismiss]);
    }

    #[test]
    fn the_selection_stays_on_the_rows_and_stops_at_the_ends() {
        assert_eq!(clamp_selection(7, 3), 2);
        assert_eq!(clamp_selection(0, 0), 0);
        assert_eq!(step_selection(0, 3, true), 1);
        assert_eq!(step_selection(2, 3, true), 2, "down from the last row stays");
        assert_eq!(step_selection(0, 3, false), 0, "up from the first row stays");
        assert_eq!(
            step_selection(9, 3, false),
            1,
            "a stale index is clamped before it moves"
        );
    }

    #[test]
    fn digits_pick_rows_counting_from_one() {
        assert_eq!(row_for_digit("1", 3), Some(0));
        assert_eq!(row_for_digit("3", 3), Some(2));
        assert_eq!(row_for_digit("4", 3), None);
        assert_eq!(row_for_digit("0", 3), None);
        assert_eq!(row_for_digit("a", 3), None);
        assert_eq!(row_for_digit("enter", 3), None);
    }

    #[test]
    fn badges_number_the_nine_rows_a_digit_reaches() {
        assert_eq!(badge_label(0).as_deref(), Some("1"));
        assert_eq!(badge_label(8).as_deref(), Some("9"));
        assert_eq!(badge_label(9), None);
    }

    // Ways the elicitation half of the dock can fail:
    // - an answered, expired or withdrawn request still holds the dock (or a
    //   fresh one does not), so the composer and the dock disagree;
    // - a form is drawn in the wrong order, or a step shows the wrong rows:
    //   a required field offers Skip, an optional one does not, Decline is
    //   missing, a boolean is not Yes/No;
    // - the review step shows nothing the user typed;
    // - a link that is not a web page is offered to open (`javascript:`,
    //   `file:`, a custom scheme, a bare host), or its host is read wrongly
    //   (userinfo, port, case, a backslash that a browser reads as a path);
    // - server text keeps a control or bidi character that disguises the host;
    // - an unsupported form offers anything but a way out;
    // - open requests are walked in any order but the transcript's.

    use sirio_chat::{ElicitationChoice, ElicitationField, ElicitationFieldKind, ElicitationPrompt};

    fn probe_fields() -> Vec<ElicitationField> {
        let field = |name: &str, title: &str, required, kind, default| ElicitationField {
            name: name.into(),
            title: title.into(),
            description: None,
            required,
            kind,
            default,
        };
        let mut note = field(
            "note",
            "note",
            false,
            ElicitationFieldKind::Text {
                format: Some("email".into()),
            },
            None,
        );
        note.description = Some("free text".into());
        vec![
            field(
                "colour",
                "Colour",
                true,
                ElicitationFieldKind::Choice(vec![
                    ElicitationChoice {
                        value: "red".into(),
                        label: "red".into(),
                    },
                    ElicitationChoice {
                        value: "blue".into(),
                        label: "blue".into(),
                    },
                ]),
                None,
            ),
            field("count", "How many", false, ElicitationFieldKind::Integer, None),
            note,
            field(
                "ok",
                "ok",
                false,
                ElicitationFieldKind::Boolean,
                Some(serde_json::Value::Bool(true)),
            ),
        ]
    }

    fn form_at(step: usize) -> Entry {
        let mut entry = open_elicitation(1, ElicitationPrompt::Form(probe_fields()));
        if let Entry::Elicitation { step: at, values, .. } = &mut entry {
            *at = step;
            if step > 0 {
                values.insert("colour".into(), "blue".into());
            }
        }
        entry
    }

    fn labels(view: &QuestionView) -> Vec<String> {
        view.rows.iter().map(DockRow::label).collect()
    }

    #[test]
    fn an_open_elicitation_is_an_open_question() {
        let fresh = form_at(0);
        assert!(is_open(&fresh));
        for close in [
            |e: &mut Entry| {
                if let Entry::Elicitation { resolved, .. } = e {
                    *resolved = Some("declined");
                }
            },
            |e: &mut Entry| {
                if let Entry::Elicitation { expired, .. } = e {
                    *expired = true;
                }
            },
            |e: &mut Entry| {
                if let Entry::Elicitation { withdrawn, .. } = e {
                    *withdrawn = true;
                }
            },
        ] {
            let mut closed = form_at(0);
            close(&mut closed);
            assert!(!is_open(&closed));
            assert_eq!(question_view(&[closed]), None);
        }
    }

    #[test]
    fn a_form_is_walked_one_field_at_a_time() {
        let colour = question_view(&[form_at(0)]).unwrap();
        assert_eq!(colour.caption, "elic asks");
        assert_eq!(labels(&colour), ["red", "blue", "Decline"]);
        assert!(colour.body.contains("(required)"), "{}", colour.body);
        assert!(colour.body.contains("Pick a colour"), "{}", colour.body);

        let count = question_view(&[form_at(1)]).unwrap();
        assert_eq!(count.step, 1);
        assert_eq!(labels(&count), ["FreeText", "Skip", "Decline"]);
        assert!(!count.body.contains("(required)"));

        let note = question_view(&[form_at(2)]).unwrap();
        assert!(note.body.contains("Format: email"), "{}", note.body);
        assert!(note.body.contains("free text"), "{}", note.body);

        let review = question_view(&[form_at(4)]).unwrap();
        assert!(review.body.contains("Colour: blue"), "{}", review.body);
        assert_eq!(labels(&review), ["Accept", "Decline"]);
    }

    #[test]
    fn a_form_with_no_fields_starts_at_its_review() {
        let entry = open_elicitation(1, ElicitationPrompt::Form(Vec::new()));
        let view = question_view(&[entry]).unwrap();
        assert_eq!(labels(&view), ["Accept", "Decline"]);
    }

    #[test]
    fn a_boolean_step_offers_yes_and_no() {
        let view = question_view(&[form_at(3)]).unwrap();
        assert_eq!(labels(&view), ["Yes", "No", "Skip", "Decline"]);
        assert_eq!(elicitation_start_row(&form_at(3)), 0, "the default is true");
        let mut no = form_at(3);
        if let Entry::Elicitation {
            prompt: ElicitationPrompt::Form(fields),
            ..
        } = &mut no
        {
            fields[3].default = Some(serde_json::Value::Bool(false));
        }
        assert_eq!(elicitation_start_row(&no), 1, "the default is false");
    }

    #[test]
    fn a_step_starts_on_the_answer_given_else_the_default() {
        let mut entry = form_at(0);
        if let Entry::Elicitation { values, prompt: ElicitationPrompt::Form(fields), .. } = &mut entry {
            values.insert("colour".into(), "blue".into());
            fields[0].default = Some("red".into());
        }
        assert_eq!(elicitation_start_row(&entry), 1, "the given answer wins over the default");
        if let Entry::Elicitation { values, .. } = &mut entry {
            values.clear();
        }
        assert_eq!(elicitation_start_row(&entry), 0, "the default is red, row 0");
        if let Entry::Elicitation { prompt: ElicitationPrompt::Form(fields), .. } = &mut entry {
            fields[0].default = Some("blue".into());
        }
        assert_eq!(elicitation_start_row(&entry), 1, "a choice's default is its start row");
        assert_eq!(elicitation_start_row(&form_at(1)), 0);
    }

    #[test]
    fn a_completed_link_is_closed_and_offers_nothing() {
        let mut link = open_elicitation(
            2,
            ElicitationPrompt::Url {
                url: "https://example.com/a".into(),
                elicitation_id: Some("el-2".into()),
            },
        );
        assert!(is_open(&link));
        if let Entry::Elicitation { completed, .. } = &mut link {
            *completed = true;
        }
        assert!(!is_open(&link));
        assert_eq!(question_view(&[link.clone()]), None);
        let row = super::super::control_entry_row(&link);
        assert_eq!(row["openOffered"], "false");
        assert_eq!(row["host"], "example.com");
    }

    #[test]
    fn invisible_characters_are_dropped_from_server_text() {
        for hidden in ['\u{200B}', '\u{200F}', '\u{061C}', '\u{2060}', '\u{FEFF}', '\u{202E}'] {
            assert_eq!(clean_line(&format!("a{hidden}b")), "ab");
        }
        let mut entry = form_at(1);
        if let Entry::Elicitation { prompt: ElicitationPrompt::Form(fields), .. } = &mut entry {
            fields[1].kind = ElicitationFieldKind::Text { format: None };
            fields[1].default = Some("x\u{202e}y".into());
        }
        let view = question_view(&[entry]).unwrap();
        assert!(matches!(&view.rows[0], DockRow::FreeText(input) if input.prefill.as_deref() == Some("xy")));
    }

    #[test]
    fn only_web_urls_are_offered_to_open() {
        for (url, host) in [
            ("https://example.com/a", "example.com"),
            ("HTTP://X.y:8080/p", "x.y"),
            ("https://user@evil.com/", "evil.com"),
            ("https://good.com@evil.com/", "evil.com"),
            ("https://evil.com\\@good.com/", "evil.com"),
            ("https://example.com?q=1", "example.com"),
            ("https://[::1]:9/x", "[::1]"),
        ] {
            assert_eq!(web_url_host(url).as_deref(), Some(host), "{url}");
        }
        for url in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "vscode://x",
            "example.com",
            "https://",
            "https:///path",
            "https://:80/",
            "https://ex ample.com/",
            "https://example.com:80x/",
        ] {
            assert_eq!(web_url_host(url), None, "{url}");
        }
        let entry = open_elicitation(
            1,
            ElicitationPrompt::Url {
                url: "https://exa\u{202e}mple.com/\u{0007}a".into(),
                elicitation_id: None,
            },
        );
        let view = question_view(&[entry]).unwrap();
        assert!(!view.body.contains('\u{202e}') && !view.body.contains('\u{0007}'));
        assert!(view.body.contains("Host: example.com"), "{}", view.body);
        assert_eq!(labels(&view), ["Open in browser", "Decline"]);
    }

    #[test]
    fn a_link_that_is_not_a_web_page_offers_only_decline() {
        let entry = open_elicitation(
            1,
            ElicitationPrompt::Url {
                url: "file:///etc/passwd".into(),
                elicitation_id: None,
            },
        );
        let view = question_view(&[entry]).unwrap();
        assert_eq!(labels(&view), ["Decline"]);
        assert!(!view.body.contains("Host:"));
    }

    #[test]
    fn an_unsupported_form_offers_only_decline() {
        let entry = open_elicitation(
            1,
            ElicitationPrompt::Unsupported {
                reason: "`nested` is not a flat field".into(),
            },
        );
        let view = question_view(&[entry]).unwrap();
        assert_eq!(labels(&view), ["Decline"]);
        assert!(
            view.body
                .contains("Sirio can't show this form: `nested` is not a flat field"),
            "{}",
            view.body
        );
    }

    #[test]
    fn the_dock_walks_open_requests_in_transcript_order() {
        let entries = vec![
            open_permission(1, "/repo/a.rs", &["Allow"]),
            form_at(0),
        ];
        let view = question_view(&entries).unwrap();
        assert_eq!(view.entry_index, 0);
        let mut first = entries;
        if let Entry::Permission { resolved, .. } = &mut first[0] {
            *resolved = Some("Allow".into());
        }
        let view = question_view(&first).unwrap();
        assert_eq!((view.entry_index, view.caption.as_str()), (1, "elic asks"));
    }

    #[test]
    fn an_elicitation_row_reports_what_the_dock_draws() {
        let row = super::super::control_entry_row(&form_at(1));
        let get = |key: &str| row.get(key).map(String::as_str);
        assert_eq!(get("kind"), Some("elicitation"));
        assert_eq!(get("id"), Some("1"));
        assert_eq!(get("status"), Some("pending"));
        assert_eq!(get("mode"), Some("form"));
        assert_eq!(get("step"), Some("1"));
        assert_eq!(get("fields"), Some("colour|count|note|ok"));
        assert_eq!(get("rows"), Some("FreeText|Skip|Decline"));
        assert_eq!(get("server"), Some("elic"));
        assert_eq!(get("openOffered"), Some("false"));

        let mut closed = form_at(1);
        if let Entry::Elicitation { resolved, completed, .. } = &mut closed {
            *resolved = Some("accepted");
            *completed = true;
        }
        let row = super::super::control_entry_row(&closed);
        assert_eq!(row["status"], "completed", "completed outranks accepted");
        assert_eq!(row["rows"], "", "a closed request draws no rows");

        let link = open_elicitation(
            2,
            ElicitationPrompt::Url {
                url: "https://User@Example.com/a".into(),
                elicitation_id: Some("el-2".into()),
            },
        );
        let row = super::super::control_entry_row(&link);
        assert_eq!(row["mode"], "url");
        assert_eq!(row["host"], "example.com");
        assert_eq!(row["url"], "https://User@Example.com/a");
        assert_eq!(row["openOffered"], "true");
        assert_eq!(row["rows"], "Open in browser|Decline");
    }
}
