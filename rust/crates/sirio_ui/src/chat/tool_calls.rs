//! Tool calls as the gallery draws them: one `step_row` per call, a bordered
//! box per run of consecutive calls, and a `Verb · N` fold for consecutive
//! calls of the same verb — `crabtalk/bezel` tag `v0.1.4`,
//! `apps/gallery/src/patterns/agent.rs` (the `ToolCalls` section).
//!
//! The library never learns what a tool is: `step_row` takes strings, and the
//! grouping is `slice::chunk_by`. What Sirio adds is the mapping from the
//! protocol's `kind` to an icon and a verb, and a clock for the duration.

use crate::text_selection::selectable_text;
use ely_gpui_component::{
    agent::{ToolCallCard, ToolCallGroup},
    chat::StepState,
};
use gpui::{
    AnyElement, Div, ElementId, Entity, FocusHandle, ScrollHandle, SharedString, div, prelude::*,
    px,
};
use std::collections::HashMap;

use super::{
    Chat, DIFF_PREVIEW_MAX_LINES, DiffPreviewContext, DiffPreviewSelection, EditSummaryState,
    Entry, SubagentToolCall, TranscriptInteraction, diff_preview_lines, project_tool_content,
    tool_call_plain_text,
};
use sirio_acp::{ToolCallContentInfo, ToolCallLocationInfo};
use sirio_theme::Theme;

// bezel's `step_output` metrics (`bezel-ui` `widgets/status.rs`), private there.
const OUTPUT_PAD_X: f32 = 10.0;
const OUTPUT_PAD_Y: f32 = 6.0;
const OUTPUT_MAX_HEIGHT: f32 = 256.0;

/// bezel's `step_output` well, with a body that can be selected.
///
/// `step_output` hangs its text on the well as a plain string child, and a
/// `Div` cannot give a child back, so the only way to a selectable body is to
/// draw the well here. It must stay indistinguishable from bezel's — same
/// cap, border, padding, face and colour — which is what
/// `the_selectable_output_well_lays_out_like_bezels` holds it to, so a bezel
/// bump that moves `step_output` shows up there rather than as a well that
/// quietly no longer matches the rest of the row.
pub(crate) fn output_well(
    theme: &bezel::theme::Theme,
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .max_h(px(OUTPUT_MAX_HEIGHT))
        .overflow_y_scroll()
        .border_t_1()
        .border_color(theme.border)
        .px(px(OUTPUT_PAD_X))
        .py(px(OUTPUT_PAD_Y))
        .font_family(theme.font_mono.clone())
        .text_size(px(12.0))
        .text_color(theme.text_muted)
        .child(selectable_text(text))
}

/// The verb a row leads with: the kind word, capitalised, snake_case read
/// as two words; `Tool` when the protocol gave none (an empty kind, the
/// generic `tool` a pre-#168 database restores, or `Other` — no kind of
/// tool at all).
pub(crate) fn tool_verb(kind: &str) -> String {
    let kind = kind.trim();
    if kind.is_empty() || kind.eq_ignore_ascii_case("tool") || kind.eq_ignore_ascii_case("other") {
        return "Tool".to_string();
    }
    let mut out = String::with_capacity(kind.len());
    let mut first = true;
    for c in kind.chars() {
        if c == '_' {
            out.push(' ');
        } else if first {
            out.extend(c.to_uppercase());
            first = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// A title as one line: every line break (and the indentation that follows
/// it) becomes a single space, so the row's detail slot truncates with an
/// ellipsis instead of wrapping.
pub(crate) fn one_line_title(title: &str) -> String {
    title
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
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

/// Whether a wheel over a capped tool output should be consumed by the
/// output itself instead of bubbling to the transcript list.
///
/// GPUI: `offset_y` negativo verso il basso, `max_y` overflow (>=0),
/// `delta_y` negativo = giù, positivo = su. Trattiene solo se l'inner può
/// ancora scorrere in quella direzione; ai bordi lascia passare (chaining
/// nativo), altrimenti l'outer resterebbe intrappolato.
pub(crate) fn tool_output_consumes_scroll(
    offset_y: gpui::Pixels,
    max_y: gpui::Pixels,
    delta_y: gpui::Pixels,
) -> bool {
    if max_y <= px(0.0) || delta_y == px(0.0) {
        return false;
    }
    let travelled = offset_y.clamp(-max_y, px(0.0)).abs();
    if delta_y > px(0.0) {
        // Su: c'è ancora strada verso la cima.
        travelled > px(0.5)
    } else {
        // Giù: c'è ancora strada verso il fondo.
        max_y - travelled > px(0.5)
    }
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

pub(super) fn activity_state(status: &str) -> StepState {
    match status.to_ascii_lowercase().as_str() {
        "completed" | "done" => StepState::Done,
        "failed" | "error" | "cancelled" | "canceled" => StepState::Failed,
        "in_progress" | "inprogress" | "running" => StepState::Working,
        _ => StepState::Waiting,
    }
}

impl Chat {
    /// Assicura un `ScrollHandle` persistente per ogni output testuale
    /// dell'entry: senza handle la well non può trattenere la wheel e
    /// l'evento ribolle anche alla lista (doppio scroll).
    pub(crate) fn ensure_tool_output_scroll_for_entry(&mut self, entry_index: usize) {
        let Some(entry) = self.entries.get(entry_index).cloned() else {
            return;
        };
        match entry {
            Entry::ToolCall {
                content,
                raw_input,
                raw_output,
                ..
            } => {
                let content =
                    project_tool_content(&content, raw_input.as_deref(), raw_output.as_deref());
                let mut ordinal = 0usize;
                for item in &content {
                    if matches!(item, ToolCallContentInfo::Text(_)) {
                        let key = format!("tool-output-{entry_index}-{ordinal}");
                        self.tool_output_scroll
                            .entry(key)
                            .or_insert_with(ScrollHandle::new);
                        ordinal += 1;
                    }
                }
            }
            Entry::SubagentTask { tool_calls, .. } => {
                for (child_index, call) in tool_calls.iter().enumerate() {
                    let mut ordinal = 0usize;
                    for item in &call.content {
                        if matches!(item, ToolCallContentInfo::Text(_)) {
                            let key = format!(
                                "subagent-tool-output-{entry_index}-{child_index}-{ordinal}"
                            );
                            self.tool_output_scroll
                                .entry(key)
                                .or_insert_with(ScrollHandle::new);
                            ordinal += 1;
                        }
                    }
                }
            }
            _ => {}
        }
    }

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
    /// hairline above, so a run box needs no divider of its own. `nested`
    /// addresses a call inside a subagent task rather than a transcript
    /// entry: its own toggle ids, and a body without the edit summary or
    /// transcript selection.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_tool_row(
        entry_index: usize,
        row_id: ElementId,
        nested: Option<(usize, usize)>,
        _first: bool,
        title: &str,
        status: &str,
        kind: &str,
        duration_ms: Option<u64>,
        content: Vec<ToolCallContentInfo>,
        locations: Vec<ToolCallLocationInfo>,
        expanded: bool,
        edit_summary: Option<EditSummaryState>,
        source_start: usize,
        interaction: Option<TranscriptInteraction>,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
        entity: Entity<Chat>,
        output_scrolls: &HashMap<String, ScrollHandle>,
    ) -> AnyElement {
        let has_body = Self::tool_has_body(&content, &locations);
        let meta = Self::tool_row_meta(status, duration_ms);
        let marker = nested
            .map(|(task, child)| format!("{task}-{child}"))
            .unwrap_or_else(|| entry_index.to_string());
        let selector = nested
            .map(|(task, child)| format!("subagent-tool-call-toggle-{task}-{child}"))
            .unwrap_or_else(|| format!("tool-call-toggle-{entry_index}"));
        let toggle_entity = entity.clone();
        let mut card = ToolCallCard::new(row_id, tool_verb(kind), activity_state(status))
            .summary(one_line_title(title))
            .header_selector(selector)
            .expanded(expanded, move |desired, _, cx| toggle_entity.update(cx, |chat, cx| {
                let current = match nested {
                    None => matches!(chat.entries.get(entry_index), Some(Entry::ToolCall { expanded: true, .. })),
                    Some((task, child)) => matches!(chat.entries.get(task), Some(Entry::SubagentTask { tool_calls, .. }) if tool_calls.get(child).is_some_and(|call| call.expanded)),
                };
                if current != desired { match nested {
                    None => chat.toggle_tool_call_expanded(entry_index, cx),
                    Some((task, child)) => chat.toggle_subagent_tool_call_expanded(task, child, cx),
                } }
            }));
        if let Some(ms) = duration_ms {
            card = card.took(std::time::Duration::from_millis(ms));
        }
        // A closed custom body is an empty element: it still gives the card a
        // disclosure, without mounting scroll or animation work offscreen.
        if has_body {
            card = card.body(if expanded {
                Self::render_tool_body(
                    entry_index,
                    nested,
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
                    output_scrolls,
                )
            } else {
                div().into_any_element()
            });
        }
        div()
            .w_full()
            .child(card)
            .child(div().size_0().debug_selector({
                let marker = marker.clone();
                move || format!("tool-call-meta-{marker}-{meta}")
            }))
            .when(has_body, |row| {
                row.child(div().size_0().debug_selector({
                    let marker = marker.clone();
                    move || format!("tool-call-chevron-{marker}")
                }))
            })
            .when(is_failed_status(status), |row| {
                row.child(
                    div()
                        .size_0()
                        .debug_selector(move || format!("tool-call-failed-{marker}")),
                )
            })
            .into_any_element()
    }

    /// The body under an open row: text output through bezel's capped
    /// scrolling well; diffs, file links and the edit summary keep Sirio's
    /// renderers, stacked in the gallery's gap/padding frame.
    #[allow(clippy::too_many_arguments)]
    fn render_tool_body(
        entry_index: usize,
        nested: Option<(usize, usize)>,
        content: Vec<ToolCallContentInfo>,
        locations: Vec<ToolCallLocationInfo>,
        edit_summary: Option<EditSummaryState>,
        source_start: usize,
        interaction: Option<TranscriptInteraction>,
        title: &str,
        status: &str,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
        entity: Entity<Chat>,
        output_scrolls: &HashMap<String, ScrollHandle>,
    ) -> AnyElement {
        let typography = theme.typography;
        // F-CHAT-31: the same projection `Entry::plain_text` contributes to
        // the transcript, so every diff row drawn below can name its own
        // offset in the global selection coordinate space. A nested call
        // contributes no text of its own, so it gets no selection either.
        let plain = tool_call_plain_text(title, status, &content, &locations);
        // Top-level rows key their ids off the transcript entry; nested rows
        // off the subagent task and the call's position in it.
        let (output_base, diff_base, location_base) = match nested {
            None => (
                format!("tool-output-{entry_index}"),
                format!("tool-diff-{entry_index}"),
                format!("tool-call-location-{entry_index}"),
            ),
            Some((task, child)) => (
                format!("subagent-tool-output-{task}-{child}"),
                format!("subagent-diff-{task}-{child}"),
                format!("subagent-tool-call-location-{task}-{child}"),
            ),
        };
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
                    let output_id = format!("{output_base}-{output_ordinal}");
                    let debug_id = output_id.clone();
                    let mut well = output_well(
                        bezel_theme,
                        SharedString::from(output_id.clone()),
                        text.clone(),
                    );
                    // La well interna deve isolare la wheel dalla lista
                    // esterna: GPUI fa ribollire lo stesso evento a tutti gli
                    // hitbox sotto il mouse, quindi senza stop entrambi
                    // scrollano (il bug). Con handle persistente + stop
                    // edge-aware: trattiene solo finché ha corsa, ai bordi
                    // lascia passare (chaining nativo).
                    if let Some(handle) = output_scrolls.get(&output_id) {
                        let scroll_handle = handle.clone();
                        well = well.track_scroll(&scroll_handle).on_scroll_wheel(
                            move |event, _, cx| {
                                let delta_y = event.delta.pixel_delta(px(20.0)).y;
                                if tool_output_consumes_scroll(
                                    scroll_handle.offset().y,
                                    scroll_handle.max_offset().y,
                                    delta_y,
                                ) {
                                    cx.stop_propagation();
                                }
                            },
                        );
                    }
                    body = body.child(well.debug_selector(move || debug_id.clone()));
                    output_ordinal += 1;
                }
                ToolCallContentInfo::Diff(diff) => {
                    let drawn = diff_preview_lines(diff.old_text.as_deref(), &diff.new_text)
                        .len()
                        .min(DIFF_PREVIEW_MAX_LINES);
                    let selection = interaction
                        .as_ref()
                        .map(|interaction| DiffPreviewSelection {
                            interaction: interaction.clone(),
                            line_starts: plain.diff_line_starts(diff_ordinal, source_start, drawn),
                        });
                    body = body.child(Self::render_tool_diff(
                        diff,
                        theme,
                        DiffPreviewContext {
                            id_prefix: format!("{diff_base}-{diff_ordinal}"),
                            entity: entity.clone(),
                            selection,
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
                    let selector = format!("{location_base}-{index}");
                    let element_id = selector.clone();
                    div()
                        .id(SharedString::from(element_id))
                        .debug_selector(move || selector.clone())
                        .text_size(typography.footnote)
                        .text_color(theme.file_link)
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
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
        if !diffs.is_empty() && nested.is_none() {
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

    /// Destructures one tool-call entry and draws it as a row in its run.
    pub(super) fn tool_row_from_entry(
        &self,
        entry_index: usize,
        first: bool,
        source_start: usize,
        entry: &Entry,
        transcript_focus: FocusHandle,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
        entity: Entity<Chat>,
    ) -> AnyElement {
        let Entry::ToolCall {
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
        } = entry
        else {
            return div().into_any_element();
        };
        Self::render_tool_row(
            entry_index,
            ElementId::Name(
                format!(
                    "chat-{}-{}-{entry_index}",
                    entity.entity_id().as_u64(),
                    self.transcript_generation
                )
                .into(),
            ),
            None,
            first,
            title,
            status,
            kind,
            *duration_ms,
            project_tool_content(content, raw_input.as_deref(), raw_output.as_deref()),
            locations.clone(),
            *expanded,
            self.edit_summaries.get(&entry_index).cloned(),
            source_start,
            Some(TranscriptInteraction {
                chat: entity.clone(),
                focus: transcript_focus,
            }),
            theme,
            bezel_theme,
            entity,
            &self.tool_output_scroll,
        )
    }

    /// The protocol Task call: one run box whose header reads "Task" with
    /// the agent's title as its detail, and whose nested tool calls are
    /// step rows indented under it while it is open.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_subagent_task(
        task_index: usize,
        row_id: ElementId,
        title: String,
        status: String,
        tool_calls: Vec<SubagentToolCall>,
        expanded: bool,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
        entity: Entity<Chat>,
        output_scrolls: &HashMap<String, ScrollHandle>,
    ) -> AnyElement {
        let toggle_entity = entity.clone();
        let mut group = ToolCallGroup::new(row_id.clone(), format!("Task · {title}"))
            .status(activity_state(&status))
            .header_selector(format!("subagent-task-toggle-{task_index}"))
            .expanded(expanded, move |desired, _, cx| toggle_entity.update(cx, |chat, cx| {
                if matches!(chat.entries.get(task_index), Some(Entry::SubagentTask { expanded, .. }) if *expanded != desired) { chat.toggle_subagent_task_expanded(task_index, cx); }
            }));
        if expanded {
            for (child_index, call) in tool_calls.into_iter().enumerate() {
                group = group.child(Self::render_tool_row(
                    task_index,
                    (row_id.clone(), format!("child-{child_index}")).into(),
                    Some((task_index, child_index)),
                    child_index == 0,
                    &call.title,
                    &call.status,
                    &call.kind,
                    call.duration_ms,
                    call.content,
                    call.locations,
                    call.expanded,
                    None,
                    0,
                    None,
                    theme,
                    bezel_theme,
                    entity.clone(),
                    output_scrolls,
                ));
            }
        }
        Self::run_box(bezel_theme)
            .id(("tool-run", task_index))
            .debug_selector(move || format!("tool-run-{task_index}"))
            .child(group)
            .into_any_element()
    }

    /// The box a run shares: rounded, bordered, clipping whatever it holds.
    fn run_box(bezel_theme: &bezel::theme::Theme) -> Div {
        div()
            .w_full()
            .rounded(px(bezel::theme::Theme::panel_radius()))
            .border_1()
            .border_color(bezel_theme.border)
            .overflow_hidden()
            .flex()
            .flex_col()
    }

    /// A run of consecutive calls, `members` in transcript order as
    /// `(entry index, source start, entry)`: one row per call, and one
    /// `Verb · N` header for each consecutive run of the same verb with two
    /// or more calls, its members drawn under it only while the fold is open.
    pub(super) fn render_tool_run(
        &self,
        members: Vec<(usize, usize, Entry)>,
        transcript_focus: FocusHandle,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
        entity: Entity<Chat>,
    ) -> AnyElement {
        let Some((run_start, _, _)) = members.first() else {
            return div().into_any_element();
        };
        let run_start = *run_start;
        let kinds: Vec<&str> = members
            .iter()
            .map(|(_, _, entry)| match entry {
                Entry::ToolCall { kind, .. } => kind.as_str(),
                _ => "",
            })
            .collect();
        let mut children: Vec<AnyElement> = Vec::new();
        let mut first_in_box = true;
        for (offset, len) in verb_folds(&kinds) {
            let slice = &members[offset..offset + len];
            if len == 1 {
                let (index, source_start, entry) = &slice[0];
                children.push(self.tool_row_from_entry(
                    *index,
                    first_in_box,
                    *source_start,
                    entry,
                    transcript_focus.clone(),
                    theme,
                    bezel_theme,
                    entity.clone(),
                ));
                first_in_box = false;
                continue;
            }
            let fold_start = slice[0].0;
            let open = self.open_verb_folds.contains(&fold_start);
            let state = slice
                .iter()
                .map(|(_, _, entry)| match entry {
                    Entry::ToolCall { status, .. } => activity_state(status),
                    _ => StepState::Waiting,
                })
                .fold(StepState::Done, |a, b| match (a, b) {
                    (StepState::Failed, _) | (_, StepState::Failed) => StepState::Failed,
                    (StepState::Working, _) | (_, StepState::Working) => StepState::Working,
                    (StepState::Waiting, _) | (_, StepState::Waiting) => StepState::Waiting,
                    _ => StepState::Done,
                });
            let working = slice.iter().filter(|(_, _, entry)| matches!(entry, Entry::ToolCall { status, .. } if activity_state(status) == StepState::Working)).count();
            let failed = slice.iter().filter(|(_, _, entry)| matches!(entry, Entry::ToolCall { status, .. } if activity_state(status) == StepState::Failed)).count();
            let mut label = format!("{} · {len}", tool_verb(kinds[offset]));
            if working > 0 {
                label.push_str(&format!(" · {working} running"));
            }
            if failed > 0 {
                label.push_str(&format!(" · {failed} failed"));
            }
            let toggle_entity = entity.clone();
            let mut fold = ToolCallGroup::new(
                ElementId::Name(
                    format!(
                        "chat-{}-{}-{fold_start}-fold",
                        entity.entity_id().as_u64(),
                        self.transcript_generation
                    )
                    .into(),
                ),
                label,
            )
            .status(state)
            .header_selector(format!("tool-fold-{fold_start}"))
            .expanded(open, move |desired, _, cx| {
                toggle_entity.update(cx, |chat, cx| {
                    if chat.open_verb_folds.contains(&fold_start) != desired {
                        chat.toggle_verb_fold(fold_start, cx);
                    }
                })
            });
            if open {
                for (position, (index, source_start, entry)) in slice.iter().enumerate() {
                    fold = fold.child(self.tool_row_from_entry(
                        *index,
                        position == 0,
                        *source_start,
                        entry,
                        transcript_focus.clone(),
                        theme,
                        bezel_theme,
                        entity.clone(),
                    ));
                }
            }
            children.push(fold.into_any_element());
            first_in_box = false;
        }
        Self::run_box(bezel_theme)
            .id(("tool-run", run_start))
            .debug_selector(move || format!("tool-run-{run_start}"))
            .children(children)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel::ui::widgets::Status as _;

    /// The well is a copy of bezel's `step_output` with a selectable body;
    /// this is what keeps the copy honest. Drawn side by side at the same
    /// width, on text that wraps and text that does not, the two must occupy
    /// the same box.
    #[gpui::test]
    async fn the_selectable_output_well_lays_out_like_bezels(cx: &mut gpui::TestAppContext) {
        use gpui::{Context, Render, Window, size};

        struct Wells(String);
        impl Render for Wells {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let theme = bezel::theme::Theme::of(cx).clone();
                div()
                    .w(px(220.0))
                    .child(
                        theme
                            .step_output("bezel-well", self.0.clone())
                            .debug_selector(|| "bezel-well".into()),
                    )
                    .child(
                        output_well(&theme, "sirio-well", self.0.clone())
                            .debug_selector(|| "sirio-well".into()),
                    )
            }
        }

        cx.update(sirio_theme::Theme::init);
        for text in [
            "one short line".to_string(),
            "error[E0308]: mismatched types in a line long enough to wrap\nsecond line".to_string(),
            (0..80).map(|i| format!("line {i}\n")).collect(),
        ] {
            let (_, vcx) = cx.add_window_view(|_, _| Wells(text.clone()));
            vcx.simulate_resize(size(px(400.0), px(800.0)));
            vcx.run_until_parked();
            let bezel = vcx
                .debug_bounds("bezel-well")
                .expect("bezel's well renders");
            let sirio = vcx
                .debug_bounds("sirio-well")
                .expect("the selectable well renders");
            assert_eq!(sirio.size, bezel.size, "for {text:?}");
        }
    }

    #[test]
    fn titles_are_flattened_to_one_line() {
        assert_eq!(one_line_title("cargo test"), "cargo test");
        assert_eq!(
            one_line_title("cd x && python - <<'EOF'\nimport io\n  p = 1\nEOF"),
            "cd x && python - <<'EOF' import io p = 1 EOF"
        );
        assert_eq!(one_line_title("a\r\n\r\nb"), "a b");
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

    #[test]
    fn inner_output_consumes_wheel_until_its_edge() {
        // GPUI: offset negativo verso il basso, max_overflow >= 0.
        // delta_y negativo = giù, positivo = su.
        // Senza overflow l'inner non deve mai trattenere la wheel.
        assert!(!tool_output_consumes_scroll(px(0.0), px(0.0), px(-10.0)));
        assert!(!tool_output_consumes_scroll(px(0.0), px(0.0), px(10.0)));
        // In mezzo: trattiene in entrambe le direzioni.
        assert!(tool_output_consumes_scroll(px(-50.0), px(100.0), px(-10.0)));
        assert!(tool_output_consumes_scroll(px(-50.0), px(100.0), px(10.0)));
        // In cima: solo giù.
        assert!(tool_output_consumes_scroll(px(0.0), px(100.0), px(-10.0)));
        assert!(!tool_output_consumes_scroll(px(0.0), px(100.0), px(10.0)));
        // In fondo: solo su.
        assert!(!tool_output_consumes_scroll(
            px(-100.0),
            px(100.0),
            px(-10.0)
        ));
        assert!(tool_output_consumes_scroll(px(-100.0), px(100.0), px(10.0)));
        // Delta nullo: mai.
        assert!(!tool_output_consumes_scroll(px(-50.0), px(100.0), px(0.0)));
    }
}
