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

/// Whether the chat-width probe asks for a redraw at `chat_width`: the row's
/// own decision, taken again, would change the layout it has. The probe asks the
/// render's question, in the render's arithmetic, so the two never disagree at the
/// boundary: a width one ulp from it cannot redraw on every paint.
pub(crate) fn probe_notifies(
    chat_width: f32,
    docked: bool,
    full_needed: f32,
    current: RowLayout,
) -> bool {
    row_layout(row_width(chat_width, docked), full_needed, current) != current
}

/// Shared between render and the chat-width probe.
#[derive(Default)]
pub(crate) struct RowFrame {
    /// The layout the last render chose.
    pub layout: Cell<RowLayout>,
    /// The Full row's width the last render decided from.
    pub needed: Cell<f32>,
    /// Whether the session panel was docked beside the band in that render.
    pub docked: Cell<bool>,
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
            // The layout rounds a text's box up to whole pixels, so the row needs
            // at least that much.
            let width: f32 = text_system
                .layout_line(text, typography.ui_size, &[run], None)
                .width
                .into();
            width.ceil()
        };
        let chevron: f32 = crate::sidebar::icons::IconSize::XSmall
            .resolve(typography)
            .into();

        // Without a catalogue the pill names the connection's state, which moves
        // with every turn: measured at the widest word it can show, so the layout
        // does not flip with the turn.
        let (lead, mode_width) = match self.mode_catalog.as_ref() {
            Some(_) => {
                let (_, mode_name) = super::composer_view::status_pill_content(
                    self.connecting,
                    self.streaming,
                    self.client.is_some(),
                    self.mode_catalog.as_ref(),
                );
                (MODE_ICON, measure(&mode_name))
            }
            None => (
                STATUS_DOT,
                ["connecting", "working", "idle", "offline"]
                    .into_iter()
                    .map(measure)
                    .fold(0.0, f32::max),
            ),
        };
        let mut pill = 2.0 * CHIP_PAD_X + lead + CHIP_GAP + mode_width;
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
            .fast_state()
            .map_or(0.0, |_| 2.0 * CHIP_PAD_X + measure("Fast"));
        let background = (self.background_tasks > 0)
            .then(|| 2.0 * CHIP_PAD_X + measure(&format!("⌁ {}", self.background_tasks)));
        let thinking = self.thinking_state().map_or(0.0, |state| {
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

    /// A width on the boundary must not redraw on every paint. At every width,
    /// the boundary's f32 neighbours included: one render lands on a layout, and
    /// the probe, asked again at that width with that layout, stays quiet. It asks
    /// from the layout the row had exactly when the render changes it.
    #[test]
    fn the_probe_and_the_render_decide_alike_at_the_boundary() {
        let neededs = [
            0.0,
            250.0,
            400.0,
            491.471_466_064_453_1,
            537.3,
            611.7,
            695.999,
            700.0,
            712.0,
            800.0,
        ];
        for needed in neededs {
            for docked in [false, true] {
                let beside = if docked { 300.0 } else { 0.0 } + 24.0;
                for current in [RowLayout::Full, RowLayout::Compact] {
                    let mut widths = vec![0.0, 1.0, 639.0, 640.0, 2400.0];
                    for edge in [needed, needed + HYSTERESIS] {
                        let mut at = edge + beside;
                        for _ in 0..3 {
                            widths.push(at);
                            at = at.next_up();
                        }
                        let mut at = (edge + beside).next_down();
                        for _ in 0..3 {
                            widths.push(at);
                            at = at.next_down();
                        }
                    }
                    for width in widths {
                        let rendered = row_layout(row_width(width, docked), needed, current);
                        assert!(
                            !probe_notifies(width, docked, needed, rendered),
                            "redraws forever: needed={needed} docked={docked} \
                             current={current:?} rendered={rendered:?} width={width}"
                        );
                    }
                }
            }
        }
    }
}
