//! The card `/usage` is drawn as when the CLI sends its `usage_report`: this
//! chat's totals, one line per model, the plan's limit rows under one heading
//! per group, and the extra usage.
//!
//! The card is built once, as `CardLine`s; the drawn card, the text a copy
//! takes (`plain_text`) and the read row (`card`, `headline`) all come from
//! them, so the three agree. Every string the CLI sent (a scope name, a kind,
//! a group, a model name, a currency code) goes through `visible_text` here
//! and nowhere else.

use chrono::{DateTime, Local, TimeZone};
use sirio_chat::{ExtraUsage, LimitRow, PlanLimits, SessionTotals, UsageReport, whole_percent};
use std::fmt::Display;

use super::limit_notice::reset_time;
use super::visible_text::visible_text;
use super::*;

/// The title the card is drawn and copied under.
const TITLE: &str = "Usage";

/// How a row's bar is coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Severity {
    Normal,
    Warning,
    Critical,
}

/// One line of the card, with what the drawing needs to style it.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum CardLine {
    /// The totals and the model lines.
    Muted(String),
    /// A group's name.
    Heading(String),
    /// A limit row: `name` and `rest` are adjacent, the bar sits between.
    Row {
        name: String,
        rest: String,
        percent: Option<u8>,
        severity: Severity,
        /// The first active row, whose name is drawn bold.
        bold: bool,
    },
    /// Plan limits that are unavailable or empty, and the extra usage.
    Plain(String),
}

impl CardLine {
    fn text(&self) -> String {
        match self {
            Self::Muted(text) | Self::Heading(text) | Self::Plain(text) => text.clone(),
            Self::Row { name, rest, .. } => format!("{name}{rest}"),
        }
    }
}

/// A duration in whole seconds, rounded down: `42s`, `3m 5s`, `1h 2m`.
fn duration(ms: u64) -> String {
    let seconds = ms / 1_000;
    match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3_599 => format!("{}m {}s", seconds / 60, seconds % 60),
        _ => format!("{}h {}m", seconds / 3_600, seconds % 3_600 / 60),
    }
}

/// Joins the parts of a line, or `None` when there are none.
fn joined(parts: &[String]) -> Option<String> {
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn totals_line(totals: &SessionTotals) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(cost) = totals.cost_usd {
        parts.push(format!("${cost:.2}"));
    }
    if let Some(api) = totals.api_ms {
        parts.push(format!("API {}", duration(api)));
    }
    if let Some(wall) = totals.wall_ms {
        parts.push(format!("total {}", duration(wall)));
    }
    match (totals.lines_added, totals.lines_removed) {
        (Some(added), Some(removed)) => {
            parts.push(format!("{added} lines added, {removed} removed"));
        }
        (Some(added), None) => parts.push(format!("{added} lines added")),
        (None, Some(removed)) => parts.push(format!("{removed} lines removed")),
        (None, None) => {}
    }
    joined(&parts).map(|line| format!("This chat: {line}"))
}

fn model_lines(totals: &SessionTotals) -> Vec<String> {
    totals
        .models
        .iter()
        .filter_map(|(model, tokens)| {
            let mut parts = Vec::new();
            for (count, label) in [
                (tokens.input, "in"),
                (tokens.output, "out"),
                (tokens.cache_read, "cache read"),
                (tokens.cache_write, "cache write"),
            ] {
                if let Some(count) = count {
                    parts.push(format!("{} {label}", thousands(count)));
                }
            }
            joined(&parts).map(|line| format!("{}: {line}", visible_text(model, false)))
        })
        .collect()
}

/// A group's heading: the two known groups by name, any other as sent.
fn heading(group: &str) -> String {
    match group {
        "session" => "Session".to_string(),
        "weekly" => "Weekly".to_string(),
        other => visible_text(other, false),
    }
}

/// The name a row is shown under.
fn row_name(row: &LimitRow) -> String {
    match (&row.scope_name, row.kind.as_str()) {
        (Some(scope), _) => visible_text(scope, false),
        (None, "session") => "Current session".to_string(),
        (None, "weekly_all") => "All models".to_string(),
        (None, kind) => visible_text(kind, false),
    }
}

/// One row, or `None` when it has no part at all (no name, percent, reset or
/// severity word): a line with nothing on it is dropped.
fn row_line<Tz: TimeZone>(row: &LimitRow, bold: bool, now: &DateTime<Tz>) -> Option<CardLine>
where
    Tz::Offset: Display,
{
    let name = row_name(row);
    let percent = row.percent.map(whole_percent);
    let mut parts = Vec::new();
    if let Some(percent) = percent {
        parts.push(format!("{percent}%"));
    }
    // A reset that does not parse leaves its part out.
    if let Some(at) = row
        .resets_at
        .as_deref()
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
    {
        parts.push(format!(
            "resets at {}",
            reset_time(&at.with_timezone(&now.timezone()), now)
        ));
    }
    let severity = match row.severity.as_str() {
        "warning" => {
            parts.push("warning".to_string());
            Severity::Warning
        }
        "critical" => {
            parts.push("critical".to_string());
            Severity::Critical
        }
        _ => Severity::Normal,
    };
    if name.is_empty() && parts.is_empty() {
        return None;
    }
    // The separators sit between parts that exist: with no name there is
    // nothing for the first one to follow.
    let rest = if name.is_empty() {
        parts.join(" · ")
    } else {
        parts.iter().map(|part| format!(" · {part}")).collect()
    };
    Some(CardLine::Row {
        name,
        rest,
        percent,
        severity,
        bold,
    })
}

/// An amount in minor units: `$12.34`, `12.34 EUR`, or `12.34` with no code.
fn amount(minor: f64, currency: Option<&str>) -> String {
    let major = minor / 100.0;
    match currency.filter(|code| !code.is_empty()) {
        Some("USD") => format!("${major:.2}"),
        Some(code) => format!("{major:.2} {}", visible_text(code, false)),
        None => format!("{major:.2}"),
    }
}

fn extra_line(extra: &ExtraUsage) -> String {
    if !extra.enabled {
        return "Extra usage: off".to_string();
    }
    let currency = extra.currency.as_deref();
    match (extra.used, extra.monthly_limit) {
        (Some(used), Some(limit)) => format!(
            "Extra usage: {} of {} this month",
            amount(used, currency),
            amount(limit, currency)
        ),
        (Some(used), None) => format!("Extra usage: {} this month", amount(used, currency)),
        // A limit alone says nothing about how much was used.
        _ => "Extra usage: on".to_string(),
    }
}

/// The card, line by line, in the order it is drawn.
pub(super) fn card_parts<Tz: TimeZone>(report: &UsageReport, now: &DateTime<Tz>) -> Vec<CardLine>
where
    Tz::Offset: Display,
{
    let mut lines = Vec::new();
    if let Some(totals) = &report.totals {
        lines.extend(totals_line(totals).map(CardLine::Muted));
        lines.extend(model_lines(totals).into_iter().map(CardLine::Muted));
    }
    match &report.limits {
        PlanLimits::Unavailable => lines.push(CardLine::Plain("Plan limits: unavailable".into())),
        PlanLimits::Rows(rows) if rows.is_empty() => {
            lines.push(CardLine::Plain("Plan limits: none reported".into()));
        }
        PlanLimits::Rows(rows) => {
            // One heading per group, in the order the groups first appear.
            let mut groups: Vec<(&str, Vec<&LimitRow>)> = Vec::new();
            for row in rows {
                match groups.iter_mut().find(|(group, _)| *group == row.group) {
                    Some((_, members)) => members.push(row),
                    None => groups.push((row.group.as_str(), vec![row])),
                }
            }
            let mut first_active = rows.iter().position(|row| row.is_active);
            let mut drawn = 0;
            for (group, members) in groups {
                let mut group_lines = Vec::new();
                for row in members {
                    // Groups reorder the rows, so the first active row is
                    // found by identity, not by its place in the group.
                    let is_first_active = first_active
                        .is_some_and(|index| std::ptr::eq(row, &rows[index]));
                    if is_first_active {
                        first_active = None;
                    }
                    group_lines.extend(row_line(row, is_first_active, now));
                }
                // A heading stands over rows that were drawn.
                let title = heading(group);
                if !group_lines.is_empty() && !title.is_empty() {
                    lines.push(CardLine::Heading(title));
                }
                drawn += group_lines.len();
                lines.extend(group_lines);
            }
            // Rows that held nothing to draw leave the same line as no rows.
            if drawn == 0 {
                lines.push(CardLine::Plain("Plan limits: none reported".into()));
            }
        }
    }
    lines.extend(report.extra.as_ref().map(|extra| CardLine::Plain(extra_line(extra))));
    lines
}

/// The drawn lines of the card, as they are copied and read back.
#[must_use]
pub(super) fn card_lines<Tz: TimeZone>(report: &UsageReport, now: &DateTime<Tz>) -> Vec<String>
where
    Tz::Offset: Display,
{
    card_parts(report, now).iter().map(CardLine::text).collect()
}

/// The first active row's name, or `""`.
pub(super) fn headline<Tz: TimeZone>(report: &UsageReport, now: &DateTime<Tz>) -> String
where
    Tz::Offset: Display,
{
    card_parts(report, now)
        .into_iter()
        .find_map(|line| match line {
            CardLine::Row { name, bold: true, .. } => Some(name),
            _ => None,
        })
        .unwrap_or_default()
}

/// What a copy of the card takes: the title, then the lines.
pub(super) fn plain_text(report: &UsageReport) -> String {
    let mut text = format!("{TITLE}\n");
    text.push_str(&card_lines(report, &Local::now()).join("\n"));
    text
}

/// The card, drawn. `source_start` is where the entry's `plain_text` starts in
/// the transcript, so each piece of text highlights exactly what it copies.
pub(super) fn render(
    report: &UsageReport,
    entry_index: usize,
    source_start: usize,
    theme: &Theme,
    interaction: &TranscriptInteraction,
) -> AnyElement {
    let typography = theme.typography;
    let start = Cell::new(source_start);
    let text = |label: String, id: String, color: Hsla, interaction: &TranscriptInteraction| {
        let at = start.get();
        start.set(at + label.len());
        div().text_color(color).child(Chat::render_plain_text(
            label,
            theme,
            id,
            at,
            Some(interaction),
        ))
    };
    let mut card = div()
        .id(("usage-card", entry_index))
        .debug_selector(move || format!("usage-card-{entry_index}"))
        .w_full()
        .rounded(theme.radii.code_block)
        .bg(theme.ely.surface)
        .border_l_2()
        .border_color(theme.sirio.border_strong)
        .px(px(CARD_H_PADDING))
        .py(px(CARD_V_PADDING))
        .flex()
        .flex_col()
        .gap(px(3.0))
        .text_size(typography.callout)
        .text_color(theme.ely.fg);
    let title = text(
        TITLE.to_string(),
        format!("usage-card-{entry_index}-title"),
        theme.ely.fg,
        interaction,
    );
    card = card.child(title.font_weight(gpui::FontWeight::MEDIUM));
    // Past the title and its line break.
    start.set(start.get() + 1);
    for (n, line) in card_parts(report, &Local::now()).into_iter().enumerate() {
        let id = format!("usage-card-{entry_index}-{n}");
        let element = match line {
            CardLine::Muted(label) => text(label, id, theme.ely.fg_muted, interaction)
                .text_size(typography.footnote)
                .into_any_element(),
            CardLine::Heading(label) => text(label, id, theme.ely.fg_subtle, interaction)
                .mt(px(6.0))
                .text_size(typography.footnote)
                .into_any_element(),
            CardLine::Plain(label) => text(label, id, theme.ely.fg, interaction).into_any_element(),
            CardLine::Row {
                name,
                rest,
                percent,
                severity,
                bold,
            } => {
                let fill = match severity {
                    Severity::Normal => theme.ely.accent,
                    Severity::Warning => theme.ely.warning,
                    Severity::Critical => theme.ely.danger,
                };
                // A shared minimum width lines the bars up under each other.
                // An empty name draws no text, only the space that keeps
                // the bars aligned.
                let name = if name.is_empty() {
                    div().flex_none().min_w(px(112.0))
                } else {
                    text(name, format!("{id}-name"), theme.ely.fg, interaction)
                        .flex_none()
                        .min_w(px(112.0))
                };
                let name = if bold {
                    name.font_weight(gpui::FontWeight::BOLD)
                } else {
                    name
                };
                let mut row = div().w_full().flex().items_center().gap(px(8.0)).child(name);
                if let Some(percent) = percent {
                    row = row.child(
                        div()
                            .flex_none()
                            .w(px(56.0))
                            .h(px(5.0))
                            .rounded(px(3.0))
                            .bg(theme.ely.border)
                            .child(
                                div()
                                    .h_full()
                                    .rounded(px(3.0))
                                    .bg(fill)
                                    .w(relative(f32::from(percent) / 100.0)),
                            ),
                    );
                }
                // The rest takes what is left of the row and wraps in it.
                row.child(
                    text(rest, format!("{id}-rest"), theme.ely.fg_muted, interaction)
                        .flex_1()
                        .min_w_0(),
                )
                .into_any_element()
            }
        };
        card = card.child(element);
        // The line break after every line.
        start.set(start.get() + 1);
    }
    card.into_any_element()
}
