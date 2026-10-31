//! The composer band's measurements. The band spans the chat column; its
//! content keeps to the transcript's reading column, so the field's first
//! character sits on the transcript text's left edge.

use std::cell::{Cell, RefCell};

use gpui::{Bounds, Pixels, Window};
use sirio_theme::Theme;

use super::controls::OverflowRow;
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

/// How the control row is laid out, widest first (the variants are ordered, so
/// `Full < Compact < Narrow < Minimal`): every control whole; the compact set that
/// fits a narrow band on one line; below that, the compact set without the
/// background-task count and the context ring, which are rows in ⋯ then; and, under
/// that, the model chip (or the agent's badge) and the attach button are rows of ⋯ too
/// and the chips that stay, the permission pill and the effort, lose their chevrons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RowLayout {
    #[default]
    Full,
    Compact,
    Narrow,
    Minimal,
}

impl RowLayout {
    /// Narrow or narrower: the task count and the ring are rows of ⋯.
    pub(crate) fn is_narrow(self) -> bool {
        self >= RowLayout::Narrow
    }

    /// The narrowest layout: the model is a row of ⋯ and the chips that stay have
    /// no chevrons.
    pub(crate) fn is_minimal(self) -> bool {
        self == RowLayout::Minimal
    }
}

/// How much wider than its need a row has to get before it goes back to a
/// wider layout (from Minimal to Narrow, Compact or Full, from Narrow to Compact or
/// Full, from Compact to Full), so a width sitting on the boundary does not flip
/// each frame.
pub(crate) const HYSTERESIS: f32 = 16.0;

/// The width the row lays its controls in: the band's content column, for a chat
/// `chat_width` wide whose session panel is or is not docked beside the band.
pub(crate) fn row_width(chat_width: f32, docked: bool) -> f32 {
    let beside = if docked { COLUMN_WIDTH } else { 0.0 };
    (chat_width - beside - 2.0 * BAND_PAD_X)
        .min(BAND_COLUMN)
        .max(0.0)
}

/// The layout for a row `row_width` wide, from `current`: the widest of Full,
/// Compact, Narrow and Minimal whose need the row meets. A layout wider than
/// `current` needs a hysteresis step more than its own need, so that a width on
/// the boundary does not flip each frame; `current` and any narrower layout need
/// only their own. Full needs `full_needed`, Compact `compact_needed` and Narrow
/// `narrow_needed`, each held to the need above it (a larger figure is lowered to
/// it, so no band is unreachable); Minimal needs nothing, so it is the layout of
/// every width the others do not fit.
pub(crate) fn row_layout(
    row_width: f32,
    full_needed: f32,
    compact_needed: f32,
    narrow_needed: f32,
    current: RowLayout,
) -> RowLayout {
    let compact_needed = compact_needed.min(full_needed);
    let narrow_needed = narrow_needed.min(compact_needed);
    [
        (RowLayout::Full, full_needed),
        (RowLayout::Compact, compact_needed),
        (RowLayout::Narrow, narrow_needed),
        (RowLayout::Minimal, 0.0),
    ]
    .into_iter()
    .find(|(layout, needed)| {
        let wider = *layout < current;
        row_width >= if wider { needed + HYSTERESIS } else { *needed }
    })
    .map_or(RowLayout::Minimal, |(layout, _)| layout)
}

/// Whether the chat-width probe asks for a redraw at `chat_width`: the row's
/// own decision, taken again, would change the layout it has. The probe asks the
/// render's question, in the render's arithmetic, so the two never disagree at the
/// boundary: a width one ulp from it cannot redraw on every paint.
pub(crate) fn probe_notifies(
    chat_width: f32,
    docked: bool,
    full_needed: f32,
    compact_needed: f32,
    narrow_needed: f32,
    current: RowLayout,
) -> bool {
    row_layout(
        row_width(chat_width, docked),
        full_needed,
        compact_needed,
        narrow_needed,
        current,
    ) != current
}

/// Shared between render and the chat-width probe.
#[derive(Default)]
pub(crate) struct RowFrame {
    /// The layout the last render chose.
    pub layout: Cell<RowLayout>,
    /// The Full row's width the last render decided from.
    pub needed: Cell<f32>,
    /// The Compact row's width the last render decided from.
    pub compact_needed: Cell<f32>,
    /// The Narrow row's width the last render decided from.
    pub narrow_needed: Cell<f32>,
    /// Whether the session panel was docked beside the band in that render.
    pub docked: Cell<bool>,
    /// The chat root's bounds, as the probe last measured them.
    pub root: Cell<Option<Bounds<Pixels>>>,
    /// The band's own bounds, as its probe last measured them.
    pub band: Cell<Option<Bounds<Pixels>>>,
    /// The send control's bounds, as the band's send probe last measured them.
    pub send: Cell<Option<Bounds<Pixels>>>,
    /// The bounds of the clipping container the chips sit in, as its probe last
    /// measured them.
    pub chips: Cell<Option<Bounds<Pixels>>>,
    /// The ⋯ trigger's bounds, as its probe last measured them.
    pub overflow_button: Cell<Option<Bounds<Pixels>>>,
    /// The Lucide name of the icon the last render drew in the permission pill;
    /// `None` when it drew none.
    pub mode_icon: Cell<Option<&'static str>>,
    /// Whether the last render wrapped the pill in the bypass warning.
    pub mode_danger: Cell<bool>,
    /// The rows of ⋯ the last render built, while the menu is open.
    pub menu: RefCell<Option<Vec<MenuRow>>>,
}

/// One row of ⋯ as the render built it.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct MenuRow {
    /// `model`, `agent`, `attach`, `fast`, `thinking`, `session-panel`, `background-tasks`,
    /// `context`, `follow`, `new-conversation` or `history`.
    pub id: String,
    /// Whether a press on the row does anything.
    pub enabled: bool,
    /// The row's tooltip: on Fast, the reason the CLI refused it.
    pub tooltip: Option<String>,
}

/// The composer band as the control socket reports it (`composerRow` of
/// `surface.chat.read`): which layout the row took, where the band and its
/// parts were last laid out and what the layout moved into ⋯. Boxes are
/// `[x, y, width, height]` in window pixels.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerSnapshot {
    /// `full`, `compact`, `narrow` or `minimal`.
    pub layout: &'static str,
    pub band: Option<[f32; 4]>,
    /// The chat root, whose width the layout is decided from.
    pub root: Option<[f32; 4]>,
    /// The send control.
    pub send: Option<[f32; 4]>,
    /// The chips' clipping container.
    pub chips: Option<[f32; 4]>,
    /// Whether the session panel is docked beside the band.
    pub docked: bool,
    /// `model`, `agent`, `attach`, `fast`, `thinking`, `session-panel`, `background-tasks`
    /// and `context`: what the layout moved into ⋯.
    pub overflow: Vec<&'static str>,
    /// The Lucide name of the icon the permission pill drew; `None` when it drew
    /// none, which is without a catalogue, where the pill is a dot.
    pub mode_icon: Option<&'static str>,
    /// Whether the pill was drawn as the bypass warning.
    pub mode_danger: bool,
    /// The ⋯ trigger.
    pub overflow_button: Option<[f32; 4]>,
    /// The rows ⋯ drew, in order; `None` while it is closed.
    pub menu: Option<Vec<MenuRow>>,
    /// The composer picker that is open: `mode`, `model`, `effort`, `thinking`,
    /// `overflow`, `history` or `context`; `None` when none is.
    pub picker: Option<&'static str>,
}

fn boxed(bounds: Option<Bounds<Pixels>>) -> Option<[f32; 4]> {
    bounds.map(|b| {
        [
            b.origin.x.into(),
            b.origin.y.into(),
            b.size.width.into(),
            b.size.height.into(),
        ]
    })
}

/// The room a popup hung from ⋯ has in a Minimal row, between ⋯'s left edge and the
/// right edge of a chat `chat_width` wide, less the margin the layer keeps from the
/// window's edge. ⋯ is alone in the left group there, so its left edge is the band's
/// padding.
pub(crate) fn overflow_popup_room(chat_width: f32) -> f32 {
    (chat_width - BAND_PAD_X - POPUP_MARGIN).max(0.0)
}

/// How far `layer` keeps a popup from the window's edge (`snap_to_window_with_margin`).
const POPUP_MARGIN: f32 = 8.0;

/// One of the tool group's 24px icon buttons.
const TOOL_WIDTH: f32 = 24.0;
/// The Full row's tool group: three icon buttons and their gaps.
const TOOLS_WIDTH: f32 = 3.0 * TOOL_WIDTH + 2.0 * ROW_GAP;
/// The gap between the row's children, `gap_1` everywhere in it.
const ROW_GAP: f32 = 4.0;
/// A chip's padding on each side.
pub(crate) const CHIP_PAD_X: f32 = 7.0;
/// The gap between a chip's parts.
pub(crate) const CHIP_GAP: f32 = 6.0;
/// The permission pill's icon box, and its plain dot without a catalogue.
const MODE_ICON: f32 = 14.0;
const STATUS_DOT: f32 = 6.0;
/// The context ring's diameter.
const CONTEXT_RING: f32 = 16.0;
/// The send control.
const SEND_WIDTH: f32 = 24.0;
/// The least width the model's chip, or the agent's badge, is drawn at, whatever
/// its text (`min_w` on both the chip and its wrapper).
pub(crate) const MODEL_FLOOR: f32 = 56.0;
/// Measures a text in the UI font at its own size, as the row lays it out. The
/// layout rounds a text's box up to whole pixels, so the row needs at least
/// that much.
fn text_measurer(theme: &Theme, window: &mut Window) -> impl Fn(&str) -> f32 {
    let typography = theme.typography;
    let text_system = window.text_system().clone();
    let color = theme.ely.fg;
    move |text: &str| {
        let run = gpui::TextRun {
            len: text.len(),
            font: gpui::font(typography.ui_family),
            color,
            ..Default::default()
        };
        let width: f32 = text_system
            .layout_line(text, typography.ui_size, &[run], None)
            .width
            .into();
        width.ceil()
    }
}

/// A picker chevron's width.
fn chevron_width(theme: &Theme) -> f32 {
    crate::sidebar::icons::IconSize::XSmall
        .resolve(theme.typography)
        .into()
}

/// The effort's bars: `total` 3px steps with 2px between them.
fn bars_width(total: usize) -> f32 {
    3.0 * total as f32 + 2.0 * total.saturating_sub(1) as f32
}

/// The row around `chips`: the tools, a gap, the least spacer, a gap, the chips
/// (`slots` of them in the chips' row, a gap between each pair: a slot that draws
/// nothing is not in the row and pays none) and send.
fn row_total(tools: f32, chips: f32, slots: usize) -> f32 {
    tools + 2.0 * ROW_GAP + chips + slots.saturating_sub(1) as f32 * ROW_GAP + ROW_GAP + SEND_WIDTH
}

impl Chat {
    /// The composer band's state for the control socket.
    pub(super) fn composer_snapshot(&self) -> ComposerSnapshot {
        let frame = &self.composer_row;
        ComposerSnapshot {
            layout: match frame.layout.get() {
                RowLayout::Full => "full",
                RowLayout::Compact => "compact",
                RowLayout::Narrow => "narrow",
                RowLayout::Minimal => "minimal",
            },
            band: boxed(frame.band.get()),
            root: boxed(frame.root.get()),
            send: boxed(frame.send.get()),
            chips: boxed(frame.chips.get()),
            docked: frame.docked.get(),
            overflow: self
                .overflowed_rows()
                .into_iter()
                .filter_map(|row| match row {
                    OverflowRow::Model => Some("model"),
                    OverflowRow::Agent => Some("agent"),
                    OverflowRow::Attach => Some("attach"),
                    OverflowRow::Fast => Some("fast"),
                    OverflowRow::Thinking => Some("thinking"),
                    OverflowRow::SessionPanel => Some("session-panel"),
                    OverflowRow::BackgroundTasks => Some("background-tasks"),
                    OverflowRow::Context => Some("context"),
                    OverflowRow::Follow | OverflowRow::NewConversation | OverflowRow::History => {
                        None
                    }
                })
                .collect(),
            mode_icon: frame.mode_icon.get(),
            mode_danger: frame.mode_danger.get(),
            overflow_button: boxed(frame.overflow_button.get()),
            menu: frame.menu.borrow().clone(),
            picker: self.open_picker_name(),
        }
    }

    /// Which composer picker is open, by the flags the render reads.
    fn open_picker_name(&self) -> Option<&'static str> {
        [
            (self.mode_picker_open, "mode"),
            (self.model_picker_open, "model"),
            (self.effort_picker_open, "effort"),
            (self.thinking_picker_open, "thinking"),
            (self.overflow_open, "overflow"),
            (self.history_open, "history"),
            (self.context_popover_open, "context"),
        ]
        .into_iter()
        .find_map(|(open, name)| open.then_some(name))
    }

    /// The width the Full row needs to hold every control it has, whole, on one
    /// line: the tools, the least gap between them and the chips, the chips
    /// (each text measured in the UI font at its own size, plus its padding,
    /// icons and gaps), and send. The model's name counts whole.
    pub(super) fn full_row_needed(&self, theme: &Theme, window: &mut Window) -> f32 {
        let measure = text_measurer(theme, window);
        let chevron = chevron_width(theme);

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
                    .map(&measure)
                    .fold(0.0, f32::max),
            ),
        };
        let mut pill = 2.0 * CHIP_PAD_X + lead + CHIP_GAP + mode_width;
        if self.mode_selectable() {
            pill += CHIP_GAP + chevron;
        }

        let model_visible = self.model_control_visible();
        // Drawn at least `MODEL_FLOOR` wide, however short its text: the row
        // counts the floor, or the overflow lands in the chips' clip.
        let model = if model_visible {
            2.0 * CHIP_PAD_X + measure(&self.selected_model_name()) + CHIP_GAP + chevron
        } else {
            2.0 * CHIP_PAD_X + measure(&self.agent_badge_name())
        }
        .max(MODEL_FLOOR);

        let effort = self
            .effort_level_name()
            .filter(|_| model_visible)
            .map_or(0.0, |level| {
                let bars = self
                    .effort_bar_position()
                    .map_or(0.0, |(_, total)| CHIP_GAP + bars_width(total));
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
        // The pill, the model's or the badge's slot and the context chip are always
        // drawn; the rest are slots when `render_agent_controls` draws them.
        let slots = 3
            + usize::from(self.effort_chip_drawn())
            + usize::from(self.fast_state().is_some())
            + usize::from(self.background_tasks > 0)
            + usize::from(self.thinking_state().is_some());
        row_total(TOOLS_WIDTH, chips, slots)
    }

    /// The width the Compact row needs to hold every chip it draws, whole, on
    /// one line, with the model at its floor: the tools without the panel's
    /// toggle, which is a row in ⋯ there, the pill as its icon and chevron, the
    /// effort as its bars and chevron (its level only where there are no bars),
    /// the task count, and the ring with no caption. Fast and Thinking are rows in
    /// ⋯. Summed from the constants `full_row_needed` uses, each text rounded up
    /// the same way.
    pub(super) fn compact_row_needed(&self, theme: &Theme, window: &mut Window) -> f32 {
        let measure = text_measurer(theme, window);
        let chevron = chevron_width(theme);

        let lead = match self.mode_catalog.as_ref() {
            Some(_) => MODE_ICON,
            None => STATUS_DOT,
        };
        let mut pill = 2.0 * CHIP_PAD_X + lead;
        if self.mode_selectable() {
            pill += CHIP_GAP + chevron;
        }

        // The model's chip, or the agent's badge, is the one that gives.
        let model = MODEL_FLOOR;

        let effort = self
            .effort_level_name()
            .filter(|_| self.model_control_visible())
            .map_or(0.0, |level| match self.effort_bar_position() {
                Some((_, total)) => 2.0 * CHIP_PAD_X + bars_width(total) + CHIP_GAP + chevron,
                None => 2.0 * CHIP_PAD_X + measure(&level) + CHIP_GAP + chevron,
            });

        let background = (self.background_tasks > 0)
            .then(|| 2.0 * CHIP_PAD_X + measure(&format!("⌁ {}", self.background_tasks)));
        let context = 2.0 * CHIP_PAD_X + CONTEXT_RING;

        let chips = pill + model + effort + background.unwrap_or(0.0) + context;
        // The pill, the model's or the badge's slot and the ring are always drawn;
        // the effort and the task count are slots when `render_agent_controls`
        // draws them.
        let slots =
            3 + usize::from(self.effort_chip_drawn()) + usize::from(self.background_tasks > 0);
        // Attach and ⋯: two buttons and the gap between them.
        row_total(2.0 * TOOL_WIDTH + ROW_GAP, chips, slots)
    }

    /// The width the Narrow row needs to hold the chips it draws, whole, on one
    /// line, with the model at its floor: the Compact row's tools, pill and
    /// effort, and nothing else but the model's (or the badge's) slot; the task
    /// count and the ring are rows in ⋯. Summed like `compact_row_needed`, each
    /// text rounded up the same way. Below it the row is Minimal.
    pub(super) fn narrow_row_needed(&self, theme: &Theme, window: &mut Window) -> f32 {
        let measure = text_measurer(theme, window);
        let chevron = chevron_width(theme);

        let lead = match self.mode_catalog.as_ref() {
            Some(_) => MODE_ICON,
            None => STATUS_DOT,
        };
        let mut pill = 2.0 * CHIP_PAD_X + lead;
        if self.mode_selectable() {
            pill += CHIP_GAP + chevron;
        }

        // The model's chip, or the agent's badge, is the one that gives.
        let model = MODEL_FLOOR;

        let effort = self
            .effort_level_name()
            .filter(|_| self.effort_chip_drawn())
            .map_or(0.0, |level| match self.effort_bar_position() {
                Some((_, total)) => 2.0 * CHIP_PAD_X + bars_width(total) + CHIP_GAP + chevron,
                None => 2.0 * CHIP_PAD_X + measure(&level) + CHIP_GAP + chevron,
            });

        let chips = pill + model + effort;
        // The pill and the model's or the badge's slot are always drawn; the
        // effort is a slot when `render_agent_controls` draws it.
        let slots = 2 + usize::from(self.effort_chip_drawn());
        // Attach and ⋯: two buttons and the gap between them.
        row_total(2.0 * TOOL_WIDTH + ROW_GAP, chips, slots)
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
        use RowLayout::{Compact, Full, Minimal, Narrow};
        // Full needs 400, Compact 300; the hysteresis is 16. Narrow needs nothing
        // here, which is the row as it was before Minimal: never below Narrow.
        let at = |row: f32, current| row_layout(row, 400.0, 300.0, 0.0, current);

        // From Full: whole down to what it needs, Compact down to what Compact
        // needs, Narrow under that, and straight to Narrow in one step.
        assert_eq!(at(500.0, Full), Full);
        assert_eq!(at(400.0, Full), Full);
        assert_eq!(at(399.9, Full), Compact);
        assert_eq!(at(300.0, Full), Compact);
        assert_eq!(at(299.9, Full), Narrow);
        assert_eq!(at(0.0, Full), Narrow);

        // From Compact: Full a hysteresis step above Full's need, Narrow under
        // Compact's need, and nothing in between.
        assert_eq!(at(400.0, Compact), Compact);
        assert_eq!(at(415.9, Compact), Compact);
        assert_eq!(at(416.0, Compact), Full);
        assert_eq!(at(310.0, Compact), Compact, "inside Compact's own band");
        assert_eq!(at(300.0, Compact), Compact);
        assert_eq!(at(299.9, Compact), Narrow);
        assert_eq!(at(0.0, Compact), Narrow);

        // From Narrow: Compact a hysteresis step above Compact's need, and Full
        // a step above Full's, which skips Compact.
        assert_eq!(at(299.9, Narrow), Narrow);
        assert_eq!(at(310.0, Narrow), Narrow, "inside Compact's band");
        assert_eq!(at(315.9, Narrow), Narrow);
        assert_eq!(at(316.0, Narrow), Compact);
        assert_eq!(at(399.9, Narrow), Compact);
        assert_eq!(at(415.9, Narrow), Compact);
        assert_eq!(at(416.0, Narrow), Full, "Narrow goes straight to Full");
        assert_eq!(at(2000.0, Narrow), Full);

        // A row nothing needs room for is whole at any width.
        assert_eq!(
            row_layout(300.0, 0.0, 0.0, 0.0, Compact),
            Full,
            "nothing to fit"
        );
        assert_eq!(
            row_layout(300.0, 0.0, 0.0, 0.0, Narrow),
            Full,
            "nothing to fit"
        );
        assert_eq!(
            row_layout(10.0, 400.0, 0.0, 0.0, Full),
            Compact,
            "Compact needs 0"
        );
        // Compact never needs more than Full: a larger figure is held to it, so
        // the Compact band cannot be unreachable.
        assert_eq!(row_layout(310.0, 300.0, 400.0, 0.0, Compact), Compact);
        assert_eq!(row_layout(320.0, 300.0, 400.0, 0.0, Narrow), Full);
        assert_eq!(row_layout(310.0, 300.0, 400.0, 0.0, Narrow), Narrow);
        assert_eq!(row_layout(299.9, 300.0, 400.0, 0.0, Full), Narrow);
        // Equal needs leave no Compact range at all.
        assert_eq!(row_layout(399.0, 400.0, 400.0, 0.0, Full), Narrow);
        assert_eq!(row_layout(399.0, 400.0, 400.0, 0.0, Compact), Narrow);

        // Minimal, the layout below Narrow's fit: Full needs 400, Compact 300 and
        // Narrow 200. Minimal itself needs nothing, so it holds at any width.
        let at = |row: f32, current| row_layout(row, 400.0, 300.0, 200.0, current);
        // From Full, down in one step to whichever layout the row still fits.
        assert_eq!(at(399.9, Full), Compact);
        assert_eq!(at(299.9, Full), Narrow);
        assert_eq!(at(200.0, Full), Narrow);
        assert_eq!(at(199.9, Full), Minimal, "Full to Minimal in one step");
        assert_eq!(at(0.0, Full), Minimal);
        // From Compact and Narrow, under Narrow's need.
        assert_eq!(at(200.0, Compact), Narrow);
        assert_eq!(at(199.9, Compact), Minimal);
        assert_eq!(at(200.0, Narrow), Narrow);
        assert_eq!(at(199.9, Narrow), Minimal);
        assert_eq!(at(0.0, Narrow), Minimal);
        // From Minimal: Narrow a hysteresis step above Narrow's need, Compact a step
        // above Compact's and Full a step above Full's, each skipping what is
        // between; nothing in between a need and its step.
        assert_eq!(at(0.0, Minimal), Minimal);
        assert_eq!(
            at(200.0, Minimal),
            Minimal,
            "Narrow's need alone is not enough"
        );
        assert_eq!(at(215.9, Minimal), Minimal);
        assert_eq!(at(216.0, Minimal), Narrow);
        assert_eq!(at(299.9, Minimal), Narrow);
        assert_eq!(at(315.9, Minimal), Narrow);
        assert_eq!(
            at(316.0, Minimal),
            Compact,
            "Minimal to Compact in one step"
        );
        assert_eq!(at(415.9, Minimal), Compact);
        assert_eq!(at(416.0, Minimal), Full, "Minimal to Full in one step");
        assert_eq!(at(2000.0, Minimal), Full);
        // Narrow inside Compact's band holds, as it did before Minimal.
        assert_eq!(at(310.0, Narrow), Narrow);
        assert_eq!(at(316.0, Narrow), Compact);

        // Each need is held to the one above it, so no band is unreachable: a
        // Narrow need over Compact's is Compact's, over Full's too.
        assert_eq!(row_layout(300.0, 400.0, 300.0, 350.0, Narrow), Narrow);
        assert_eq!(row_layout(299.9, 400.0, 300.0, 350.0, Narrow), Minimal);
        assert_eq!(row_layout(316.0, 400.0, 300.0, 350.0, Minimal), Compact);
        // The held need leaves Narrow no range of its own: from Minimal it is
        // Compact's step or nothing.
        assert_eq!(row_layout(315.9, 400.0, 300.0, 350.0, Minimal), Minimal);
        assert_eq!(row_layout(250.0, 300.0, 400.0, 500.0, Full), Minimal);
        assert_eq!(row_layout(299.9, 300.0, 400.0, 500.0, Narrow), Minimal);
        // Equal needs leave no Narrow range at all.
        assert_eq!(row_layout(299.0, 300.0, 300.0, 300.0, Full), Minimal);
        assert_eq!(row_layout(299.0, 300.0, 300.0, 300.0, Narrow), Minimal);
        // A row nothing needs room for is whole at any width.
        assert_eq!(
            row_layout(300.0, 0.0, 0.0, 0.0, Minimal),
            Full,
            "nothing to fit"
        );
    }

    /// A width on the boundary must not redraw on every paint. At every width,
    /// the boundary's f32 neighbours included: one render lands on a layout, and
    /// the probe, asked again at that width with that layout, stays quiet. It asks
    /// from the layout the row had exactly when the render changes it. Swept over
    /// (Full, Compact, Narrow) needs with fractions, equal needs, zero needs, and
    /// a Compact or Narrow need the one above it is meant to hold to.
    #[test]
    fn the_probe_and_the_render_decide_alike_at_the_boundary() {
        let needs = [
            (0.0, 0.0, 0.0),
            (250.0, 0.0, 0.0),
            (250.0, 180.5, 0.0),
            (250.0, 180.5, 120.25),
            (400.0, 300.0, 200.0),
            (400.0, 399.999, 399.998),
            (491.471_466_064_453_1, 380.25, 268.9),
            (537.3, 537.3, 537.3),
            (537.3, 537.3, 211.7),
            (300.0, 400.0, 500.0),
            (611.7, 322.7, 322.7),
            (611.7, 322.7, 400.0),
            (695.999, 611.7, 301.3),
            (712.0, 321.999_97, 261.000_03),
            (800.0, 0.4, 0.2),
            (800.0, 0.4, 0.9),
        ];
        let currents = [
            RowLayout::Full,
            RowLayout::Compact,
            RowLayout::Narrow,
            RowLayout::Minimal,
        ];
        for (full, compact, narrow) in needs {
            let held = f32::min(compact, full);
            let held_narrow = f32::min(narrow, held);
            for docked in [false, true] {
                let beside = if docked { 300.0 } else { 0.0 } + 24.0;
                for current in currents {
                    let mut widths = vec![0.0, 1.0, 639.0, 640.0, 2400.0];
                    for edge in [
                        full,
                        full + HYSTERESIS,
                        compact,
                        compact + HYSTERESIS,
                        held,
                        held + HYSTERESIS,
                        narrow,
                        narrow + HYSTERESIS,
                        held_narrow,
                        held_narrow + HYSTERESIS,
                    ] {
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
                        let rendered =
                            row_layout(row_width(width, docked), full, compact, narrow, current);
                        assert!(
                            !probe_notifies(width, docked, full, compact, narrow, rendered),
                            "redraws forever: full={full} compact={compact} narrow={narrow} \
                             docked={docked} current={current:?} rendered={rendered:?} \
                             width={width}"
                        );
                    }
                }
            }
        }
    }
}
