//! The transcript's turn chrome. Every entry of a turn — thought, prose,
//! tool run, closing answer — is drawn where it sits and in one weight:
//! what the model says between its tool calls is an answer like any
//! other, so there is no `Worked · N steps` zone folding interim work
//! behind a header. What is left here is the day heading that marks
//! where the calendar day changes between turns.

use crate::text_selection::selectable_text;
use chrono::{DateTime, Local};
use sirio_theme::Theme;

use super::*;
use ely_gpui_component::buttons::{Button, ButtonVariant};
use ely_gpui_component::chat::{MessageAvatar, MessageBubble, MessageFooter, MessageHeader, Role};
use ely_gpui_component::theme::ControlSize;

/// Sirio's words for a day — bezel carries no clock. `Today`, `Yesterday`,
/// otherwise `Mon 2 Sep`.
pub(crate) fn day_label(at: DateTime<Local>, now: DateTime<Local>) -> String {
    let day = at.date_naive();
    let today = now.date_naive();
    if day == today {
        "Today".to_string()
    } else if today.pred_opt() == Some(day) {
        "Yesterday".to_string()
    } else {
        at.format("%a %-d %b").to_string()
    }
}

/// The day heading for one transcript index: the label when `entries[index]`
/// is a stamped `User` whose calendar day differs from the previous stamped
/// `User`'s, or is the first stamped one; `None` for undated turns, which
/// never break the sequence either.
pub(crate) fn heading_for(entries: &[Entry], index: usize, now: DateTime<Local>) -> Option<String> {
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
    Some(day_label(*at, now))
}

impl Chat {
    /// The day heading above a turn's question: `py 8`, `Subheadline` in
    /// `text_faint`, uppercased with the popover's tracking.
    pub(crate) fn render_day_heading(
        entry_index: usize,
        label: &str,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
    ) -> AnyElement {
        div()
            .debug_selector(move || format!("day-heading-{entry_index}"))
            .py(px(8.0))
            .text_size(theme.typography.footnote)
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(bezel_theme.text_faint)
            .child(SharedString::from(bezel::ui::popover::tracked_upper(label)))
            .into_any_element()
    }
}

pub(super) struct TranscriptRowContext<'a> {
    pub index: usize,
    pub id: ElementId,
    pub chat: Entity<Chat>,
    pub focus: FocusHandle,
    pub source_start: usize,
    pub copied_target: Option<CopyTarget>,
    pub edit_summary: Option<EditSummaryState>,
    pub thought_streaming: bool,
    pub thought_scroll: &'a HashMap<usize, thought::ThoughtScroll>,
    pub tool_output_scroll: &'a HashMap<String, ScrollHandle>,
    pub day_heading: Option<&'a str>,
    pub agent_id: Option<&'a str>,
    pub agent_name: &'a str,
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
    pub(crate) fn render_header(&self, theme: &Theme, _: &Context<Self>) -> AnyElement {
        let name = self.agent_badge_name();
        let mut header = MessageHeader::new("chat-agent-header", name.clone());
        if let Some(model) = self
            .available_models
            .iter()
            .find(|model| Some(&model.id) == self.selected_model.as_ref())
        {
            header = header.model(model.name.clone());
        }
        div()
            .id("chat-header")
            .debug_selector(|| "chat-header".into())
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap(theme.spacing.titlebar_control_spacing)
            .px(theme.spacing.traffic_light_inset)
            .py(theme.spacing.titlebar_control_spacing)
            .border_b_1()
            .border_color(theme.border)
            .child(super::identity::render_mark(
                "chat-header-mark".into(),
                self.agent_id.as_deref(),
                &name,
                theme,
            ))
            .child(header)
            .into_any_element()
    }
    pub(super) fn render_entry(
        entry: Entry,
        row: TranscriptRowContext<'_>,
        theme: &Theme,
        _window: &mut Window,
        _cx: &mut App,
    ) -> AnyElement {
        let TranscriptRowContext {
            index: entry_index,
            id,
            chat: entity,
            focus: transcript_focus,
            source_start,
            copied_target,
            edit_summary,
            thought_streaming,
            thought_scroll,
            tool_output_scroll,
            day_heading,
            agent_id,
            agent_name,
        } = row;
        let _perf = sirio_perf::span("Chat.render_entry", entry_index as u64);
        let typography = theme.typography;
        let bezel_theme = theme.to_bezel_theme();
        let interaction = TranscriptInteraction {
            chat: entity.clone(),
            focus: transcript_focus,
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
                let bubble = div()
                    .w_full()
                    .min_w_0()
                    .debug_selector(move || format!("user-bubble-{entry_index}"))
                    .child(
                        MessageBubble::new(Role::User).child(Self::render_plain_text(
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
                                    .child(MarkdownBody::selectable(
                                        document,
                                        interaction.clone(),
                                        source_start,
                                    )),
                            )
                            .footer(MessageFooter::new().child(copy)),
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
                tool_calls,
                expanded,
                ..
            } => Self::render_subagent_task(
                entry_index,
                id.clone(),
                title,
                status,
                tool_calls,
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
                is_question,
                ..
            } => {
                // The record of a question: what was asked and what became
                // of it. It is answered from the dock above the composer,
                // so it carries no buttons in any state.
                let header = if title.is_empty() {
                    "Permission requested".to_string()
                } else {
                    title
                };
                let status = if let Some(choice) = resolved {
                    format!("Answered: {choice}")
                } else if dismissed {
                    "Dismissed — request cancelled".to_string()
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
                            .text_color(theme.text)
                            .child(selectable_text(header)),
                    )
                    .when(!prompt.is_empty(), |body| {
                        body.child(
                            div()
                                .text_size(typography.footnote)
                                .text_color(theme.text_muted)
                                .child(selectable_text(prompt)),
                        )
                    })
                    .child(
                        div()
                            .text_size(typography.footnote)
                            .text_color(theme.text_faint)
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
                    } else if approval.expired {
                        "No answer — the turn ended".to_string()
                    } else {
                        "Waiting for your approval below".to_string()
                    };
                    card = card.child(
                        div()
                            .text_size(typography.footnote)
                            .text_color(theme.text_faint)
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
                            .text_size(typography.callout)
                            .text_color(theme.text)
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
                .bg(theme.surface_raised)
                .border_l_2()
                .border_color(theme.border_strong)
                .px(px(CARD_H_PADDING))
                .py(px(CARD_V_PADDING))
                .text_size(typography.callout)
                .text_color(theme.text)
                .child(rewind_report_text(
                    &files,
                    insertions,
                    deletions,
                    skipped_links,
                ))
                .into_any_element(),
            Entry::TurnFooter(at) => div()
                .w_full()
                .flex()
                .justify_center()
                .child(MessageFooter::new().fact(at))
                .into_any_element(),
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
                        .child(div().h(px(1.0)).flex_1().bg(theme.border))
                        .child(
                            div()
                                .text_size(typography.footnote)
                                .text_color(theme.text_faint)
                                .child(Self::render_plain_text(
                                    text.clone(),
                                    theme,
                                    format!("notice-entry-{entry_index}"),
                                    source_start,
                                    Some(&interaction),
                                )),
                        )
                        .child(div().h(px(1.0)).flex_1().bg(theme.border))
                        .into_any_element(),
                    NoticeKind::BackgroundTask | NoticeKind::Warning => div()
                        .w_full()
                        .flex()
                        .items_center()
                        .debug_selector(move || selector.into())
                        .text_size(typography.footnote)
                        .text_color(if kind == NoticeKind::Warning {
                            theme.warning
                        } else {
                            theme.text_faint
                        })
                        .child(Self::render_plain_text(
                            text.clone(),
                            theme,
                            format!("notice-entry-{entry_index}"),
                            source_start,
                            Some(&interaction),
                        ))
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
                    theme.text
                } else {
                    theme.danger
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
                                            retry_entity.update(cx, |chat, cx| chat.retry(cx))
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

    #[test]
    fn day_labels_are_sirios_words() {
        let now = Local.with_ymd_and_hms(2026, 9, 4, 22, 0, 0).unwrap();
        assert_eq!(day_label(now, now), "Today");
        assert_eq!(
            day_label(Local.with_ymd_and_hms(2026, 9, 3, 1, 0, 0).unwrap(), now),
            "Yesterday"
        );
        assert_eq!(
            day_label(Local.with_ymd_and_hms(2026, 9, 1, 9, 0, 0).unwrap(), now),
            "Tue 1 Sep"
        );
    }

    #[test]
    fn a_heading_appears_where_the_day_changes_and_never_for_undated_turns() {
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
        assert_eq!(heading_for(&entries, 0, now).as_deref(), Some("Yesterday"));
        assert_eq!(heading_for(&entries, 1, now), None);
        assert_eq!(
            heading_for(&entries, 2, now),
            None,
            "undated: no heading, no break"
        );
        assert_eq!(heading_for(&entries, 3, now).as_deref(), Some("Today"));
    }
}
