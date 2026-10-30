//! The composer band's measurements. The band spans the chat column; its
//! content keeps to the transcript's reading column, so the field's first
//! character sits on the transcript text's left edge.

use std::cell::Cell;

use gpui::{Bounds, Pixels, Window};
use sirio_theme::Theme;

use super::session_panel::COLUMN_WIDTH;
use super::{Chat, TRANSCRIPT_WIDTH};

/// The band's side padding: what is left between its edge and its content in a
/// pane too narrow for the reading column.
pub(crate) const BAND_PAD_X: f32 = 12.0;

/// How far the transcript pads its text from its own column's edge (the
/// `px(24.0)` on `chat-transcript`).
pub(crate) const TRANSCRIPT_PAD_X: f32 = 24.0;

/// The room between the band's top rule and the field's first row.
pub(crate) const BAND_TOP_PAD: f32 = 10.0;

/// The widest the band's content gets: the transcript's text column, so a wide
/// pane's field lines up with the text above it.
pub(crate) const BAND_COLUMN: f32 = TRANSCRIPT_WIDTH - 2.0 * TRANSCRIPT_PAD_X;

/// How the control row is laid out: every control whole, or the compact set
/// that fits a narrow band on one line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RowLayout {
    #[default]
    Full,
    Compact,
}

/// How much wider than `full_needed` a Compact row has to get before it goes
/// back to Full, so a width sitting on the boundary does not flip each frame.
pub(crate) const HYSTERESIS: f32 = 16.0;

/// The width the row lays its controls in: the band's content column, for a chat
/// `chat_width` wide whose session panel is or is not docked beside the band.
pub(crate) fn row_width(chat_width: f32, docked: bool) -> f32 {
    let beside = if docked { COLUMN_WIDTH } else { 0.0 };
    (chat_width - beside - 2.0 * BAND_PAD_X)
        .min(BAND_COLUMN)
        .max(0.0)
}

/// Full or Compact for a row `row_width` wide whose Full layout needs `full_needed`,
/// from `current`: Full → Compact below `full_needed`; Compact → Full at or above
/// `full_needed + HYSTERESIS`.
pub(crate) fn row_layout(row_width: f32, full_needed: f32, current: RowLayout) -> RowLayout {
    match current {
        RowLayout::Full if row_width < full_needed => RowLayout::Compact,
        RowLayout::Compact if row_width >= full_needed + HYSTERESIS => RowLayout::Full,
        kept => kept,
    }
}

/// The chat width at which `current` gives way, Full strictly below it and Compact at or
/// above it; None when no chat width changes it.
pub(crate) fn flip_width(full_needed: f32, docked: bool, current: RowLayout) -> Option<f32> {
    let beside = if docked { COLUMN_WIDTH } else { 0.0 } + 2.0 * BAND_PAD_X;
    match current {
        // Full gives way for every row narrower than `full_needed`. None of the
        // widest row's width fits more than the column, so beyond that it always does.
        RowLayout::Full if full_needed > BAND_COLUMN => Some(f32::INFINITY),
        RowLayout::Full if full_needed > 0.0 => Some(full_needed + beside),
        RowLayout::Full => None,
        // Compact gives way from `full_needed + HYSTERESIS`, which a row the column
        // caps never reaches when that is wider than the column.
        RowLayout::Compact => {
            let regained = full_needed + HYSTERESIS;
            if regained > BAND_COLUMN {
                None
            } else {
                Some(regained.max(0.0) + beside)
            }
        }
    }
}

/// Shared between render and the chat-width probe.
#[derive(Default)]
pub(crate) struct RowFrame {
    /// The layout the last render chose.
    pub layout: Cell<RowLayout>,
    /// The chat width at which that layout gives way (`flip_width`).
    pub flip: Cell<Option<f32>>,
    /// The chat root's bounds, as the probe last measured them.
    pub root: Cell<Option<Bounds<Pixels>>>,
    /// The band's own bounds, as its probe last measured them.
    pub band: Cell<Option<Bounds<Pixels>>>,
}

/// The control tool group's width: three 24px icon buttons and their 4px gaps.
const TOOLS_WIDTH: f32 = 3.0 * 24.0 + 2.0 * ROW_GAP;
/// The gap between the row's children, `gap_1` everywhere in it.
const ROW_GAP: f32 = 4.0;
/// A chip's padding on each side.
const CHIP_PAD_X: f32 = 7.0;
/// The gap between a chip's parts.
const CHIP_GAP: f32 = 6.0;
/// The permission pill's icon box, and its plain dot without a catalogue.
const MODE_ICON: f32 = 14.0;
const STATUS_DOT: f32 = 6.0;
/// The context ring's diameter.
const CONTEXT_RING: f32 = 16.0;
/// The send control.
const SEND_WIDTH: f32 = 24.0;
/// The children of the chips' row: the pill, model, effort, Fast, background
/// tasks, Thinking and the context chip, each in a wrapper that is drawn
/// whether or not its chip is, so each pays a gap.
const CHIP_SLOTS: f32 = 7.0;

impl Chat {
    /// The width the Full row needs to hold every control it has, whole, on one
    /// line: the tools, the least gap between them and the chips, the chips
    /// (each text measured in the UI font at its own size, plus its padding,
    /// icons and gaps), and send. The model's name counts whole.
    pub(super) fn full_row_needed(&self, theme: &Theme, window: &mut Window) -> f32 {
        let typography = theme.typography;
        let text_system = window.text_system().clone();
        let measure = |text: &str| -> f32 {
            let run = gpui::TextRun {
                len: text.len(),
                font: gpui::font(typography.ui_family),
                color: theme.ely.fg,
                ..Default::default()
            };
            text_system
                .layout_line(text, typography.ui_size, &[run], None)
                .width
                .into()
        };
        let chevron: f32 = crate::sidebar::icons::IconSize::XSmall
            .resolve(typography)
            .into();

        let (_, mode_name) = super::composer_view::status_pill_content(
            self.connecting,
            self.streaming,
            self.client.is_some(),
            self.mode_catalog.as_ref(),
        );
        let lead = if self.mode_catalog.is_some() {
            MODE_ICON
        } else {
            STATUS_DOT
        };
        let mut pill = 2.0 * CHIP_PAD_X + lead + CHIP_GAP + measure(&mode_name);
        if self.mode_selectable() {
            pill += CHIP_GAP + chevron;
        }

        let model_visible = self.model_control_visible();
        let model = if model_visible {
            2.0 * CHIP_PAD_X + measure(&self.selected_model_name()) + CHIP_GAP + chevron
        } else {
            2.0 * CHIP_PAD_X + measure(&self.agent_badge_name())
        };

        let effort = self
            .effort_level_name()
            .filter(|_| model_visible)
            .map_or(0.0, |level| {
                let bars = self.effort_bar_position().map_or(0.0, |(_, total)| {
                    CHIP_GAP + 3.0 * total as f32 + 2.0 * total.saturating_sub(1) as f32
                });
                2.0 * CHIP_PAD_X + bars + measure(&level) + CHIP_GAP + chevron
            });

        let fast = self
            .fast_mode
            .as_ref()
            .filter(|_| model_visible)
            .map_or(0.0, |_| 2.0 * CHIP_PAD_X + measure("Fast"));
        let background = (self.background_tasks > 0)
            .then(|| 2.0 * CHIP_PAD_X + measure(&format!("⌁ {}", self.background_tasks)));
        let thinking = self
            .thinking_display
            .as_ref()
            .filter(|state| !state.unsupported)
            .filter(|_| model_visible)
            .map_or(0.0, |state| {
                let value = state.chosen.as_deref().map_or(0.0, |chosen| {
                    CHIP_GAP + measure(super::thinking_display_name(chosen))
                });
                2.0 * CHIP_PAD_X + measure("Thinking") + value
            });
        let context = 2.0 * CHIP_PAD_X + CONTEXT_RING + CHIP_GAP + measure("Context");

        let chips = pill + model + effort + fast + background.unwrap_or(0.0) + thinking + context;
        // The row: tools, a gap, the least spacer, a gap, the chips and send.
        TOOLS_WIDTH + 2.0 * ROW_GAP + chips + (CHIP_SLOTS - 1.0) * ROW_GAP + ROW_GAP + SEND_WIDTH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_width_cases() {
        assert_eq!(row_width(1920.0, false), 712.0);
        assert_eq!(row_width(736.0, false), 712.0);
        assert_eq!(row_width(735.0, false), 711.0);
        assert_eq!(row_width(280.0, false), 256.0);
        assert_eq!(row_width(640.0, true), 316.0);
        assert_eq!(row_width(10.0, false), 0.0, "never negative");
    }

    #[test]
    fn row_layout_cases() {
        use RowLayout::{Compact, Full};
        assert_eq!(row_layout(500.0, 400.0, Full), Full);
        assert_eq!(row_layout(399.9, 400.0, Full), Compact);
        assert_eq!(row_layout(400.0, 400.0, Compact), Compact);
        assert_eq!(row_layout(415.9, 400.0, Compact), Compact);
        assert_eq!(row_layout(416.0, 400.0, Compact), Full);
        assert_eq!(row_layout(0.0, 400.0, Full), Compact);
        assert_eq!(row_layout(300.0, 0.0, Compact), Full, "nothing to fit");
    }

    #[test]
    fn flip_width_agrees_with_row_layout() {
        for needed in [0.0, 250.0, 400.0, 700.0, 712.0, 800.0] {
            for docked in [false, true] {
                for current in [RowLayout::Full, RowLayout::Compact] {
                    let flip = flip_width(needed, docked, current);
                    for width in 0..=2400 {
                        let width = width as f32;
                        let next = row_layout(row_width(width, docked), needed, current);
                        let predicted = flip.is_some_and(|f| {
                            if current == RowLayout::Full {
                                width < f
                            } else {
                                width >= f
                            }
                        });
                        assert_eq!(
                            next != current,
                            predicted,
                            "needed={needed} docked={docked} current={current:?} width={width} flip={flip:?}"
                        );
                    }
                }
            }
        }
    }
}
