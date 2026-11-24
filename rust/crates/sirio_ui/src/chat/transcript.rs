// Ported from bezel-ui 0.1.4 (MIT); see THIRD_PARTY_NOTICES.md.
//! The transcript's turn chrome. Every entry of a turn — thought, prose,
//! tool run, closing answer — is drawn where it sits and in one weight:
//! what the model says between its tool calls is an answer like any
//! other, so there is no `Worked · N steps` zone folding interim work
//! behind a header. What is left here is the day heading that marks
//! where the calendar day changes between turns.

use crate::text_selection::selectable_text;
use chrono::{DateTime, Datelike, Local};
use jiff::civil::Date;
use sirio_theme::Theme;

use super::*;
use ely_gpui_component::buttons::{Button, ButtonVariant};
use ely_gpui_component::chat::{
    DateSeparator, MessageAvatar, MessageBubble, MessageFooter, MessageHeader, QuoteReply, Role,
};
use ely_gpui_component::theme::ControlSize;

/// The calendar day `at` falls on in the reader's zone, as the type Ely's separator takes.
/// `None` for a day `jiff` cannot hold, which no turn has: such a turn gets no separator.
pub(crate) fn civil_day(at: DateTime<Local>) -> Option<Date> {
    Date::new(i16::try_from(at.year()).ok()?, at.month() as i8, at.day() as i8).ok()
}

/// The day heading for one transcript index: the day when `entries[index]` is a stamped `User`
/// whose calendar day differs from the previous stamped `User`'s, or is the first stamped one;
/// `None` for undated turns, which never break the sequence either. The words for the day are
/// Ely's, chosen when the separator is drawn (`render_day_heading`).
pub(crate) fn heading_for(entries: &[Entry], index: usize) -> Option<Date> {
    let Some(Entry::User { at: Some(at), .. }) = entries.get(index) else {
        return None;
    };
    let previous = entries[..index].iter().rev().find_map(|entry| match entry {
        Entry::User { at: Some(at), .. } => Some(*at),
        _ => None,
    });
    if previous.is_some_and(|previous| previous.date_naive() == at.date_naive()) {
        return None;
    }
    civil_day(*at)
}

impl Chat {
    /// The day separator above a turn's question: Ely's `DateSeparator`, which names `day` as
    /// seen from `today`. `today` is read here, at draw time, so a chat left open past midnight
    /// reads its days right on the next frame.
    pub(crate) fn render_day_heading(entry_index: usize, day: Date) -> AnyElement {
        let Some(today) = civil_day(Local::now()) else {
            return div().into_any_element();
        };
        div()
            .debug_selector(move || format!("day-heading-{entry_index}"))
            .w_full()
            .child(DateSeparator::new(day, today))
            .into_any_element()
    }
}

pub(super) struct TranscriptRowContext<'a> {
    pub index: usize,
    pub id: ElementId,
    pub chat: Entity<Chat>,
    pub focus: FocusHandle,
    pub source_start: usize,
    /// What the row draws as actions (`message_actions`), decided from the transcript's own entry:
    /// the entry drawn here has lost its stream. `None` draws none.
    pub actions: Option<message_actions::RowActions>,
    pub edit_summary: Option<EditSummaryState>,
    pub thought_streaming: bool,
    pub thought_scroll: &'a HashMap<usize, thought::ThoughtScroll>,
    pub tool_output_scroll: &'a HashMap<String, ScrollHandle>,
    pub day_heading: Option<Date>,
    pub agent_id: Option<&'a str>,
    pub agent_name: &'a str,
    /// What a live reply draws on this frame instead of its `document`: the blocks its reveal has
    /// come to (`reveal.rs`). `None` draws the document.
    pub drawn: Option<MarkdownBlocks>,
}
impl Chat {
    pub(crate) fn row_id(&self, index: usize, cx: &Context<Self>) -> ElementId {
        ElementId::Name(
            format!(
                "chat-{}-{}-{index}",
                cx.entity_id().as_u64(),
                self.transcript_generation
            )
            .into(),
        )
    }
    pub(super) fn render_entry(
        entry: Entry,
        row: TranscriptRowContext<'_>,
        theme: &Theme,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let TranscriptRowContext {
            index: entry_index,
            id,
            chat: entity,
            focus: transcript_focus,
            source_start,
            actions,
            edit_summary,
            thought_streaming,
            thought_scroll,
            tool_output_scroll,
            day_heading,
            agent_id,
            agent_name,
            drawn,
        } = row;
        let _perf = sirio_perf::span("Chat.render_entry", entry_index as u64);
        let typography = theme.typography;
        let bezel_theme = theme.to_bezel_theme();
        let interaction = TranscriptInteraction {
            chat: entity.clone(),
            focus: transcript_focus,
            entry: entry_index,
            painted: source_start,
        };
        let presentation = match &entry {
            Entry::Error { kind, .. } => Some(
                if matches!(
                    kind,
                    ErrorKind::AuthRequired | ErrorKind::Unavailable | ErrorKind::McpWarning
                ) {
                    ely_gpui_component::primitives::Severity::Warning
                } else {
                    ely_gpui_component::primitives::Severity::Danger
                },
            ),
            _ => None,
        };
        let is_rewind = matches!(
            entry,
            Entry::RewindPreview { .. } | Entry::RewindReport { .. }
        );
        let is_notice = matches!(entry, Entry::Notice { .. });
        let body = match entry {
            Entry::User { text, .. } => {
                // What Copy takes, built once and only for a row that has the action row: one
                // copy of the whole text, made from the borrow.
                let copy_text = matches!(actions, Some(message_actions::RowActions::Message { .. }))
                    .then(|| SharedString::from(text.as_str()));
                // A prompt that starts with a quote block draws it as a card above the rest
                // (`split_quote`, the rule every reader of a user text shares). The row's text in
                // the transcript's offset space is still the whole `text`, block included: the
                // rest is its tail, so it is drawn where its bytes are, and a drag or Select All
                // reads the same bytes it highlights. Copy keeps taking the whole text.
                let (quoted, rest_len) = {
                    let (quoted, rest) = quote::split_quote(&text);
                    (quoted, rest.len())
                };
                let rest_start = text.len() - rest_len;
                let rest_at = source_start + rest_start;
                // The text is moved into its element when no quote leaves it whole; with one, only
                // the tail is copied.
                let rest = if quoted.is_some() {
                    text[rest_start..].to_string()
                } else {
                    text
                };
                // A prompt that is a quote alone (nothing after it, or only whitespace) has no text
                // to draw; any other is drawn, empty or not, as it always was.
                let draws_rest = !(quoted.is_some() && rest.trim().is_empty());
                let card = quoted.map(|quoted| {
                    message_actions::measured(
                        QuoteReply::new(
                            (id.clone(), "quote-card"),
                            quote::READ_BACK_AUTHOR,
                            // One short line: Ely draws the first line it is given. `split_quote`
                            // rebuilt the whole quote for this frame; the card keeps only its head.
                            quote::card_line(&quoted),
                        ),
                        &entity,
                        entry_index,
                        "quoteCard",
                        Some("message-quote-card"),
                    )
                });
                let rest = draws_rest.then(|| {
                    Self::render_plain_text(
                        rest,
                        theme,
                        format!("user-entry-{entry_index}"),
                        rest_at,
                        Some(&interaction),
                    )
                });
                let bubble = div()
                    .w_full()
                    .min_w_0()
                    .debug_selector(move || format!("user-bubble-{entry_index}"))
                    .child(
                        MessageBubble::new(Role::User)
                            .children(card)
                            .children(rest),
                    )
                    .into_any_element();
                // The action row is the bubble's sibling, under it and to the right like the
                // bubble: never inside the selectable text, and the bubble's own box stays the
                // bubble's. The group holds both, so the pointer going from the text to the row
                // keeps it shown.
                let bubble = match (actions, copy_text) {
                    (
                        Some(message_actions::RowActions::Message {
                            shown,
                            continue_here,
                        }),
                        Some(copy_text),
                    ) => {
                        let hover_group = format!("user-message-{entry_index}");
                        div()
                            .id(("user-message", entry_index))
                            .group(hover_group.clone())
                            .w_full()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(bubble)
                            .child(div().w_full().flex().justify_end().child(
                                message_actions::render_message_actions(
                                    id.clone(),
                                    entry_index,
                                    copy_text,
                                    shown,
                                    continue_here,
                                    entity.clone(),
                                    hover_group.into(),
                                ),
                            ))
                            .into_any_element()
                    }
                    _ => bubble,
                };
                // The heading row belongs to the `User` entry's row, so no
                // extra list index is needed: it sits above the question.
                match day_heading {
                    Some(day) => div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .child(Chat::render_day_heading(entry_index, day))
                        .child(bubble)
                        .into_any_element(),
                    None => bubble,
                }
            }
            Entry::Assistant { text, document, .. } => {
                let hover_group = format!("assistant-response-{entry_index}");
                // Under the reply, in the bubble's footer: a sibling of the selectable body.
                let footer = match actions {
                    Some(message_actions::RowActions::Message {
                        shown,
                        continue_here,
                    }) => Some(
                        MessageFooter::new().child(message_actions::render_message_actions(
                            id.clone(),
                            entry_index,
                            text.into(),
                            shown,
                            continue_here,
                            entity.clone(),
                            hover_group.clone().into(),
                        )),
                    ),
                    _ => None,
                };
                div()
                    .id(("assistant-response", entry_index))
                    .group(hover_group)
                    .w_full()
                    .min_w_0()
                    .child(
                        MessageBubble::new(Role::Assistant)
                            .avatar(
                                MessageAvatar::new(
                                    (id.clone(), "avatar"),
                                    Role::Assistant,
                                    agent_name.to_owned(),
                                )
                                .content(
                                    super::identity::render_mark(
                                        (id.clone(), "mark").into(),
                                        agent_id,
                                        agent_name,
                                        theme,
                                    ),
                                ),
                            )
                            .header(MessageHeader::new(
                                (id.clone(), "header"),
                                agent_name.to_owned(),
                            ))
                            .child(
                                div()
                                    .debug_selector(move || {
                                        format!("assistant-response-{entry_index}")
                                    })
                                    .child(super::reply_body(
                                        drawn.as_ref().unwrap_or(&document),
                                        interaction.clone(),
                                        source_start,
                                    )),
                            )
                            .when_some(footer, |bubble, footer| bubble.footer(footer)),
                    )
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
                let toggle_entity = entity.clone();
                let mut block = ely_gpui_component::chat::ThinkingBlock::new(
                    id.clone(),
                    "",
                    streaming,
                    std::time::Duration::from_millis(duration_ms.unwrap_or(0)),
                )
                .header_selector(format!("thought-toggle-{entry_index}"))
                .expanded(is_open, move |desired, _, cx| {
                    toggle_entity.update(cx, |chat, cx| {
                        if let Some(Entry::Thought { open, .. }) = chat.entries.get(entry_index)
                            && open.get(chat.thought_is_streaming(entry_index)) != desired
                        {
                            chat.toggle_thought(entry_index, cx);
                        }
                    })
                });
                if streaming {
                    // Only one thought streams at a time, so one orb id serves.
                    block = block.thinking_glyph(crate::loading::thinking_indicator(
                        "thought-orb",
                        theme,
                        window,
                        cx,
                    ));
                }
                if duration_ms.is_none() {
                    block = block.label("Thought");
                }
                if is_open && let Some(scroll) = thought_scroll.get(&entry_index) {
                    block = block.body(Self::render_thought_body(
                        entry_index,
                        &text,
                        source_start,
                        &interaction,
                        scroll,
                        theme,
                        &bezel_theme,
                    ));
                }
                div()
                    .w_full()
                    .child(block)
                    .child(div().size_0().debug_selector(move || {
                        format!(
                            "thought-{}-{entry_index}",
                            if streaming { "streaming" } else { "settled" }
                        )
                    }))
                    .when(duration_ms.is_some() && !streaming, |row| {
                        row.child(
                            div()
                                .size_0()
                                .debug_selector(move || format!("thought-took-{entry_index}")),
                        )
                    })
                    .into_any_element()
            }
            Entry::ToolCall {
                title,
                status,
                kind,
                content,
                locations,
                expanded,
                duration_ms,
                raw_input,
                raw_output,
                ..
            } => Self::render_tool_row(
                entry_index,
                id.clone(),
                None,
                true,
                &title,
                &status,
                &kind,
                duration_ms,
                project_tool_content(&content, raw_input.as_deref(), raw_output.as_deref()),
                locations,
                expanded,
                edit_summary,
                source_start,
                Some(interaction.clone()),
                theme,
                &bezel_theme,
                entity.clone(),
                tool_output_scroll,
            ),
            Entry::SubagentTask {
                title,
                status,
                parts,
                expanded,
                ..
            } => Self::render_subagent_task(
                entry_index,
                id.clone(),
                title,
                status,
                parts,
                expanded,
                theme,
                &bezel_theme,
                entity.clone(),
                tool_output_scroll,
            ),
            Entry::Permission {
                request_id,
                title,
                prompt,
                resolved,
                expired,
                dismissed,
                withdrawn,
                is_question,
                ..
            } => {
                // The record of a question: what was asked and what became
                // of it. It is answered from the dock above the composer,
                // so it carries no buttons in any state.
                let header = if title.is_empty() {
                    "Permission requested".to_string()
                } else {
                    // The command the user approved, hidden characters shown.
                    super::visible_text::visible_text(&title, true)
                };
                let status = if let Some(choice) = resolved {
                    format!("Answered: {choice}")
                } else if dismissed {
                    "Dismissed — request cancelled".to_string()
                } else if withdrawn {
                    "Withdrawn by the agent".to_string()
                } else if expired {
                    // F-CHAT-27: the turn ended unanswered.
                    "No answer — the turn ended".to_string()
                } else {
                    "Waiting for your answer below".to_string()
                };
                let body = div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(typography.callout)
                            .text_color(theme.ely.fg)
                            .child(selectable_text(header)),
                    )
                    .when(!prompt.is_empty(), |body| {
                        body.child(
                            div()
                                .text_size(typography.footnote)
                                .text_color(theme.ely.fg_muted)
                                .child(selectable_text(prompt)),
                        )
                    })
                    .child(
                        div()
                            .text_size(typography.footnote)
                            .text_color(theme.ely.fg_subtle)
                            .child(status),
                    );
                let shell = if is_question {
                    ely_gpui_component::agent::HumanInputRequest::custom(id.clone(), "Question")
                        .body(body)
                        .into_any_element()
                } else {
                    ely_gpui_component::agent::PermissionPrompt::custom(
                        id.clone(),
                        "Permission requested",
                    )
                    .body(body)
                    .into_any_element()
                };
                div()
                    .id(("permission-card", request_id as usize))
                    .debug_selector(move || format!("permission-card-{request_id}"))
                    .w_full()
                    .child(shell)
                    .into_any_element()
            }
            Entry::Elicitation {
                request_id,
                server,
                message,
                resolved,
                expired,
                timed_out,
                withdrawn,
                completed,
                ..
            } => {
                // The record of a request: who asked and what became of it.
                // It is answered from the dock above the composer, so it
                // carries no buttons in any state.
                let status = if completed {
                    "Completed in the browser"
                } else if withdrawn {
                    "Withdrawn by the agent"
                } else if let Some(resolved) = resolved {
                    match resolved {
                        "cancelled" => "Cancelled",
                        "declined" => "Answered: declined",
                        _ => "Answered: accepted",
                    }
                } else if expired && timed_out {
                    "No answer in time — cancelled"
                } else if expired {
                    "No answer — the turn ended"
                } else {
                    "Waiting for your answer below"
                };
                let body = div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(typography.callout)
                            .text_color(theme.ely.fg)
                            .child(selectable_text(super::question_dock::clean_text(&message))),
                    )
                    .child(
                        div()
                            .text_size(typography.footnote)
                            .text_color(theme.ely.fg_subtle)
                            .child(status),
                    );
                div()
                    .id(("elicitation-card", request_id as usize))
                    .debug_selector(move || format!("elicitation-card-{request_id}"))
                    .w_full()
                    .child(
                        ely_gpui_component::agent::HumanInputRequest::custom(
                            id.clone(),
                            format!("{} asks", super::question_dock::clean_line(&server)),
                        )
                        .body(body),
                    )
                    .into_any_element()
            }
            Entry::Plan { entries, approval } => {
                let mut card = div()
                    .w_full()
                    .px(px(CARD_H_PADDING))
                    .py(px(CARD_V_PADDING))
                    .child(ely_gpui_component::agent::AgentPlan::new(
                        id.clone(),
                        "Plan",
                        entries.into_iter().map(|row| {
                            (row.content, super::tool_calls::activity_state(&row.status))
                        }),
                    ));
                if let Some(approval) = approval {
                    // Approved from the dock above the composer; the card
                    // keeps the plan and says what became of the approval.
                    let status = if let Some(choice) = &approval.resolved {
                        format!("Approved: {choice}")
                    } else if approval.withdrawn {
                        "Withdrawn by the agent".to_string()
                    } else if approval.expired {
                        "No answer — the turn ended".to_string()
                    } else {
                        "Waiting for your approval below".to_string()
                    };
                    card = card.child(
                        div()
                            .text_size(typography.footnote)
                            .text_color(theme.ely.fg_subtle)
                            .child(status),
                    );
                }
                card.into_any_element()
            }
            Entry::RewindPreview {
                files,
                insertions,
                deletions,
                error,
            } => {
                let mut card = div()
                    .id(("rewind-preview", entry_index))
                    .debug_selector(move || format!("rewind-preview-{entry_index}"))
                    .w_full()
                    .rounded(theme.radii.code_block)
                    .bg(theme.ely.surface)
                    .border_l_2()
                    .border_color(theme.sirio.border_strong)
                    .px(px(CARD_H_PADDING))
                    .py(px(CARD_V_PADDING))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(typography.callout)
                            .text_color(theme.ely.fg)
                            .child(rewind_preview_text(&files, insertions, deletions, &error)),
                    );
                if error.is_none() {
                    let confirm_entity = entity.clone();
                    card = card.child(
                        div()
                            .debug_selector(move || format!("rewind-confirm-{entry_index}"))
                            .child(
                                Button::new(("rewind-confirm", entry_index), "Restore files")
                                    .on_click(move |_, _, cx| {
                                        confirm_entity
                                            .update(cx, |chat, cx| chat.confirm_rewind(cx))
                                    }),
                            ),
                    );
                }
                card.into_any_element()
            }
            Entry::RewindReport {
                files,
                insertions,
                deletions,
                skipped_links,
            } => div()
                .id(("rewind-report", entry_index))
                .debug_selector(move || format!("rewind-report-{entry_index}"))
                .w_full()
                .rounded(theme.radii.code_block)
                .bg(theme.ely.surface)
                .border_l_2()
                .border_color(theme.sirio.border_strong)
                .px(px(CARD_H_PADDING))
                .py(px(CARD_V_PADDING))
                .text_size(typography.callout)
                .text_color(theme.ely.fg)
                .child(rewind_report_text(
                    &files,
                    insertions,
                    deletions,
                    skipped_links,
                ))
                .into_any_element(),
            Entry::CommandOutput {
                report: Some(report),
                ..
            } => usage_card::render(&report, entry_index, source_start, theme, &interaction),
            Entry::CommandOutput { command, text, .. } => {
                let label = command_label(&command);
                // The text starts after the label and its line break, so a
                // selection highlights exactly what it copies.
                let text_start = source_start + label.len() + 1;
                div()
                    .id(("command-output", entry_index))
                    .debug_selector(move || format!("command-output-{entry_index}"))
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .font_family(typography.code_family)
                    .text_size(typography.footnote)
                    .child(
                        div().text_color(theme.ely.fg_subtle).child(Self::render_plain_text(
                            label,
                            theme,
                            format!("command-output-label-{entry_index}"),
                            source_start,
                            Some(&interaction),
                        )),
                    )
                    // Its columns are kept: a long line scrolls sideways
                    // instead of wrapping into the next one.
                    .child(
                        div()
                            .id(("command-output-body", entry_index))
                            .overflow_x_scroll()
                            // A vertical wheel over the block keeps scrolling the
                            // transcript instead of being remapped onto it.
                            .restrict_scroll_to_axis()
                            .whitespace_nowrap()
                            .text_color(theme.ely.fg_muted)
                            .child(Self::render_plain_text(
                                text,
                                theme,
                                format!("command-output-text-{entry_index}"),
                                text_start,
                                Some(&interaction),
                            )),
                    )
                    .into_any_element()
            }
            Entry::Shell(block) => shell_block::render(
                &block,
                entry_index,
                source_start,
                &interaction,
                entity.clone(),
                tool_output_scroll,
                theme,
            ),
            Entry::TurnFooter(at) => {
                let facts = div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .child(MessageFooter::new().fact(at));
                match actions {
                    // A turn with no reply to carry Continue (it stopped during a tool call):
                    // the footer draws it alone, under the time.
                    Some(message_actions::RowActions::ContinueOnly) => div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .child(facts)
                        .child(message_actions::render_lone_continue(
                            id.clone(),
                            entry_index,
                            entity.clone(),
                        ))
                        .into_any_element(),
                    _ => facts.into_any_element(),
                }
            }
            Entry::Notice { text, kind } => {
                let selector = kind.selector();
                match kind {
                    // A compaction is drawn across the whole width, like a
                    // turn footer: what it says applies to everything above
                    // it, not to the row beside it.
                    NoticeKind::Compaction => div()
                        .w_full()
                        .h(px(24.0))
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .debug_selector(move || selector.into())
                        .child(div().h(px(1.0)).flex_1().bg(theme.ely.border))
                        .child(
                            div()
                                .text_size(typography.footnote)
                                .text_color(theme.ely.fg_subtle)
                                .child(Self::render_plain_text(
                                    text.clone(),
                                    theme,
                                    format!("notice-entry-{entry_index}"),
                                    source_start,
                                    Some(&interaction),
                                )),
                        )
                        .child(div().h(px(1.0)).flex_1().bg(theme.ely.border))
                        .into_any_element(),
                    NoticeKind::BackgroundTask
                    | NoticeKind::Warning
                    | NoticeKind::Info
                    | NoticeKind::InfoWarning => div()
                        .w_full()
                        .flex()
                        .items_center()
                        .debug_selector(move || selector.into())
                        .text_size(typography.footnote)
                        .text_color(if matches!(kind, NoticeKind::Warning | NoticeKind::InfoWarning) {
                            theme.ely.warning
                        } else {
                            theme.ely.fg_subtle
                        })
                        // The summary can be the agent's whole answer: the
                        // text shrinks to the row and wraps.
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .debug_selector(move || format!("{selector}-text"))
                                .child(Self::render_plain_text(
                                    text.clone(),
                                    theme,
                                    format!("notice-entry-{entry_index}"),
                                    source_start,
                                    Some(&interaction),
                                )),
                        )
                        .into_any_element(),
                }
            }
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
                let banner_text = if is_auth_required || is_unavailable {
                    theme.ely.fg
                } else {
                    theme.ely.danger
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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .debug_selector(move || format!("chat-error-message-{entry_index}"))
                            .child(Self::render_plain_text(
                                message,
                                theme,
                                format!("error-entry-{entry_index}"),
                                source_start,
                                Some(&interaction),
                            )),
                    )
                    .when(retryable, |this| {
                        this.child(
                            div()
                                .flex_none()
                                .debug_selector(move || {
                                    if is_disconnected {
                                        "chat-restart-agent".into()
                                    } else {
                                        "chat-retry".into()
                                    }
                                })
                                .child(
                                    Button::new(
                                        ("retry", entry_index),
                                        if is_disconnected {
                                            "Restart agent"
                                        } else {
                                            "Retry"
                                        },
                                    )
                                    .variant(ButtonVariant::Secondary)
                                    .size(ControlSize::Sm)
                                    .on_click(
                                        move |_, _, cx| {
                                            retry_entity
                                                .update(cx, |chat, cx| chat.retry(entry_index, cx))
                                        },
                                    ),
                                ),
                        )
                    })
                    .when(is_unavailable, |this| {
                        this.child(
                            div()
                                .flex_none()
                                .debug_selector(|| "chat-open-settings".into())
                                .child(
                                    Button::new(("open-settings", entry_index), "Open Settings")
                                        .size(ControlSize::Sm)
                                        .on_click(move |_, _, cx| {
                                            settings_entity.update(cx, |_, cx| {
                                                cx.emit(ChatEvent::OpenSettings)
                                            })
                                        }),
                                ),
                        )
                    })
                    .when(!is_unavailable, |this| {
                        this.child(
                            div()
                                .flex_none()
                                .debug_selector(|| "chat-error-ok".into())
                                .child(
                                    Button::new(("dismiss-error", entry_index), "OK")
                                        .variant(ButtonVariant::Secondary)
                                        .size(ControlSize::Sm)
                                        .on_click(move |_, _, cx| {
                                            dismiss_entity.update(cx, |chat, cx| {
                                                chat.dismiss_error(entry_index, cx)
                                            })
                                        }),
                                ),
                        )
                    })
                    .into_any_element()
            }
        };
        let body = if let Some(severity) = presentation {
            ely_gpui_component::feedback::Alert::new((id.clone(), "error"), severity, "")
                .content(body)
                .into_any_element()
        } else if is_rewind {
            ely_gpui_component::layout::Card::new()
                .child(body)
                .into_any_element()
        } else if is_notice {
            MessageBubble::new(Role::System)
                .child(body)
                .into_any_element()
        } else {
            body
        };
        div()
            .id(id)
            .w_full()
            .min_w_0()
            .child(body)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use jiff::civil::date;

    #[test]
    fn heading_for_gives_the_day_of_each_new_date() {
        let now = Local.with_ymd_and_hms(2026, 9, 4, 22, 0, 0).unwrap();
        let y = Local.with_ymd_and_hms(2026, 9, 3, 9, 0, 0).unwrap();
        let entries = vec![
            Entry::User {
                text: "a".into(),
                at: Some(y),
            },
            Entry::User {
                text: "b".into(),
                at: Some(y),
            },
            Entry::User {
                text: "c".into(),
                at: None,
            },
            Entry::User {
                text: "d".into(),
                at: Some(now),
            },
        ];
        assert_eq!(heading_for(&entries, 0), Some(date(2026, 9, 3)));
        assert_eq!(heading_for(&entries, 1), None);
        assert_eq!(heading_for(&entries, 2), None, "undated: no heading, no break");
        assert_eq!(heading_for(&entries, 3), Some(date(2026, 9, 4)));
    }
}
