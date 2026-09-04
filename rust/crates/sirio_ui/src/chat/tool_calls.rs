//! Tool calls as the gallery draws them: one `step_row` per call, a bordered
//! box per run of consecutive calls, and a `Verb · N` fold for consecutive
//! calls of the same verb — `crabtalk/bezel` tag `v0.1.4`,
//! `apps/gallery/src/patterns/agent.rs` (the `ToolCalls` section).
//!
//! The library never learns what a tool is: `step_row` takes strings, and the
//! grouping is `slice::chunk_by`. What Sirio adds is the mapping from the
//! protocol's `kind` to an icon and a verb, and a clock for the duration.

use bezel::ui::icons;
use bezel::ui::widgets::{Status as _, step_row_hover};
use gpui::{AnyElement, Entity, SharedString, div, prelude::*, px};

use super::{
    Chat, DIFF_PREVIEW_MAX_LINES, DiffPreviewContext, DiffPreviewSelection, EditSummaryState,
    TranscriptInteraction, diff_preview_lines, tool_call_plain_text,
};
use sirio_acp::{ToolCallContentInfo, ToolCallLocationInfo};
use sirio_theme::Theme;

/// The icon for a protocol tool kind (`Read`, `Edit`, `Execute`, …).
pub(crate) fn tool_icon(kind: &str) -> &'static str {
    match kind.to_ascii_lowercase().as_str() {
        "read" => icons::BOOK,
        "edit" => icons::PEN,
        "execute" => icons::TERMINAL,
        "search" => icons::MAGNIFER,
        "fetch" => icons::DOWNLOAD,
        "think" => icons::CPU,
        "delete" => icons::TRASH_BIN_MINIMALISTIC,
        "move" => icons::ARROW_RIGHT,
        _ => icons::WIDGET,
    }
}

/// The verb a row leads with: the kind word, capitalised; `Tool` when the
/// protocol gave none (an empty kind, or the generic `tool` a pre-#168
/// database restores).
pub(crate) fn tool_verb(kind: &str) -> String {
    let kind = kind.trim();
    if kind.is_empty() || kind.eq_ignore_ascii_case("tool") {
        return "Tool".to_string();
    }
    let mut chars = kind.chars();
    let first = chars
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    format!("{first}{}", chars.as_str())
}

/// `412ms` under a second, `1.4s` over it — a figure you read at a glance
/// rather than count digits in (gallery `took`).
pub(crate) fn took(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f32 / 1000.0)
    }
}

/// Whether a status reads as a failure — tinted red on the row.
pub(crate) fn is_failed_status(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "failed" | "cancelled" | "canceled"
    )
}

/// Consecutive same-verb runs as `(start, len)` — the gallery's grouping,
/// straight out of std.
pub(crate) fn verb_folds(kinds: &[&str]) -> Vec<(usize, usize)> {
    let mut folds = Vec::new();
    let mut start = 0;
    for run in kinds.chunk_by(|a, b| a.eq_ignore_ascii_case(b)) {
        folds.push((start, run.len()));
        start += run.len();
    }
    folds
}

impl Chat {
    /// The right-aligned figure: how long the call took when this process
    /// measured it, otherwise the status word (`running`, `failed`, …) — a
    /// restored transcript has no clock to offer.
    pub(crate) fn tool_row_meta(status: &str, duration_ms: Option<u64>) -> SharedString {
        match duration_ms {
            Some(ms) => took(ms).into(),
            None => status.to_ascii_lowercase().replace('_', " ").into(),
        }
    }

    pub(crate) fn tool_has_body(
        content: &[ToolCallContentInfo],
        locations: &[ToolCallLocationInfo],
    ) -> bool {
        content
            .iter()
            .any(|item| !matches!(item, ToolCallContentInfo::Other))
            || !locations.is_empty()
    }

    /// One call: the row, then — when open — its body. `first` skips the
    /// hairline above, so a run box needs no divider of its own.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_tool_row(
        entry_index: usize,
        first: bool,
        title: &str,
        status: &str,
        kind: &str,
        duration_ms: Option<u64>,
        content: Vec<ToolCallContentInfo>,
        locations: Vec<ToolCallLocationInfo>,
        expanded: bool,
        edit_summary: Option<EditSummaryState>,
        source_start: usize,
        interaction: TranscriptInteraction,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
        entity: Entity<Chat>,
    ) -> AnyElement {
        let failed = is_failed_status(status);
        let has_body = Self::tool_has_body(&content, &locations);
        let meta = Self::tool_row_meta(status, duration_ms);
        let meta_for_id = meta.clone();
        let toggle_entity = entity.clone();
        let row = bezel_theme
            .step_row(
                tool_icon(kind),
                tool_verb(kind),
                Some(SharedString::from(title.to_string())),
                Some(meta),
                failed,
                has_body.then_some(expanded),
            )
            .id(("tool-call-toggle", entry_index))
            .debug_selector(move || format!("tool-call-toggle-{entry_index}"))
            .hover(step_row_hover)
            .on_click(move |_, _, cx| {
                toggle_entity.update(cx, |chat, cx| {
                    chat.toggle_tool_call_expanded(entry_index, cx)
                });
            })
            // Zero-size markers: what the row *means* is testable without
            // reading pixels.
            .child(
                div()
                    .size_0()
                    .debug_selector(move || format!("tool-call-meta-{entry_index}-{meta_for_id}")),
            )
            .when(has_body, |row| {
                row.child(
                    div()
                        .size_0()
                        .debug_selector(move || format!("tool-call-chevron-{entry_index}")),
                )
            })
            .when(failed, |row| {
                row.child(
                    div()
                        .size_0()
                        .debug_selector(move || format!("tool-call-failed-{entry_index}")),
                )
            });

        let mut item = div()
            .w_full()
            .flex()
            .flex_col()
            .when(!first, |item| {
                item.border_t_1().border_color(bezel_theme.border)
            })
            .child(row);
        if expanded && has_body {
            item = item.child(Self::render_tool_body(
                entry_index,
                content,
                locations,
                edit_summary,
                source_start,
                interaction,
                title,
                status,
                theme,
                bezel_theme,
                entity,
            ));
        }
        item.into_any_element()
    }

    /// The body under an open row: text output through bezel's capped
    /// scrolling well; diffs, file links and the edit summary keep Sirio's
    /// renderers, stacked in the gallery's gap/padding frame.
    #[allow(clippy::too_many_arguments)]
    fn render_tool_body(
        entry_index: usize,
        content: Vec<ToolCallContentInfo>,
        locations: Vec<ToolCallLocationInfo>,
        edit_summary: Option<EditSummaryState>,
        source_start: usize,
        interaction: TranscriptInteraction,
        title: &str,
        status: &str,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
        entity: Entity<Chat>,
    ) -> AnyElement {
        let typography = theme.typography;
        // F-CHAT-31: the same projection `Entry::plain_text` contributes to
        // the transcript, so every diff row drawn below can name its own
        // offset in the global selection coordinate space.
        let plain = tool_call_plain_text(title, status, &content, &locations);
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(12.0))
            .pb(px(8.0));
        let mut diff_ordinal = 0usize;
        let mut output_ordinal = 0usize;
        for item in &content {
            match item {
                ToolCallContentInfo::Text(text) => {
                    body = body.child(
                        bezel_theme
                            .step_output(
                                SharedString::from(format!(
                                    "tool-output-{entry_index}-{output_ordinal}"
                                )),
                                text.clone(),
                            )
                            .debug_selector(move || {
                                format!("tool-output-{entry_index}-{output_ordinal}")
                            }),
                    );
                    output_ordinal += 1;
                }
                ToolCallContentInfo::Diff(diff) => {
                    let drawn = diff_preview_lines(diff.old_text.as_deref(), &diff.new_text)
                        .len()
                        .min(DIFF_PREVIEW_MAX_LINES);
                    body = body.child(Self::render_tool_diff(
                        diff,
                        theme,
                        DiffPreviewContext {
                            id_prefix: format!("tool-diff-{entry_index}-{diff_ordinal}"),
                            entity: entity.clone(),
                            selection: Some(DiffPreviewSelection {
                                interaction: interaction.clone(),
                                line_starts: plain.diff_line_starts(
                                    diff_ordinal,
                                    source_start,
                                    drawn,
                                ),
                            }),
                        },
                    ));
                    diff_ordinal += 1;
                }
                ToolCallContentInfo::Other => {}
            }
        }
        if !locations.is_empty() {
            // F-CHAT-23: a location is a *link*, not a label. Swift makes
            // each one a `Button { appModel.openFileReference(...) }`;
            // the equivalent here is the same `ChatEvent::OpenFile` the
            // edit-summary card already opens an editor tab with, so
            // there is one door into the file, not two.
            body = body.child(div().flex().flex_wrap().gap(px(8.0)).children(
                locations.iter().enumerate().map(|(index, location)| {
                    let label = match location.line {
                        Some(line) => {
                            format!("{}:{line}", location.path.display())
                        }
                        None => location.path.display().to_string(),
                    };
                    let open_path = location.path.clone();
                    let open_entity = entity.clone();
                    let selector = format!("tool-call-location-{entry_index}-{index}");
                    let element_id = selector.clone();
                    div()
                        .id(SharedString::from(element_id))
                        .debug_selector(move || selector.clone())
                        .text_size(typography.footnote)
                        .text_color(theme.file_link)
                        .cursor(gpui::CursorStyle::PointingHand)
                        .hover(|style| style.text_color(theme.text))
                        .on_click(move |_, _, cx| {
                            open_entity.update(cx, |_, cx| {
                                cx.emit(super::ChatEvent::OpenFile(open_path.clone()));
                            });
                        })
                        .child(label)
                }),
            ));
        }
        let diffs = content
            .iter()
            .filter_map(|content| match content {
                ToolCallContentInfo::Diff(diff) => Some(diff.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        if !diffs.is_empty() {
            body = body.child(Self::render_edit_summary(
                entry_index,
                diffs,
                edit_summary.unwrap_or_default(),
                theme,
                entity.clone(),
            ));
        }
        body.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_follow_the_kind_and_fall_back_to_widget() {
        assert_eq!(tool_icon("Read"), icons::BOOK);
        assert_eq!(tool_icon("edit"), icons::PEN);
        assert_eq!(tool_icon("Execute"), icons::TERMINAL);
        assert_eq!(tool_icon("Search"), icons::MAGNIFER);
        assert_eq!(tool_icon("Fetch"), icons::DOWNLOAD);
        assert_eq!(tool_icon("Think"), icons::CPU);
        assert_eq!(tool_icon("Delete"), icons::TRASH_BIN_MINIMALISTIC);
        assert_eq!(tool_icon("Move"), icons::ARROW_RIGHT);
        assert_eq!(tool_icon("Other"), icons::WIDGET);
        assert_eq!(tool_icon(""), icons::WIDGET);
    }

    #[test]
    fn verbs_are_the_kind_word_or_tool() {
        assert_eq!(tool_verb("Read"), "Read");
        assert_eq!(tool_verb("execute"), "Execute");
        assert_eq!(tool_verb("tool"), "Tool");
        assert_eq!(tool_verb(""), "Tool");
    }

    #[test]
    fn took_reads_at_a_glance() {
        assert_eq!(took(0), "0ms");
        assert_eq!(took(412), "412ms");
        assert_eq!(took(999), "999ms");
        assert_eq!(took(1000), "1.0s");
        assert_eq!(took(1412), "1.4s");
        assert_eq!(took(61_000), "61.0s");
    }

    #[test]
    fn failed_statuses_are_failed_and_cancelled() {
        assert!(is_failed_status("failed"));
        assert!(is_failed_status("Cancelled"));
        assert!(is_failed_status("canceled"));
        assert!(!is_failed_status("completed"));
        assert!(!is_failed_status("in_progress"));
    }

    #[test]
    fn verb_folds_are_consecutive_same_kind_runs() {
        assert_eq!(verb_folds(&[]), vec![]);
        assert_eq!(verb_folds(&["Read"]), vec![(0, 1)]);
        assert_eq!(
            verb_folds(&["Read", "Read", "read", "Execute", "Read"]),
            vec![(0, 3), (3, 1), (4, 1)]
        );
    }
}
