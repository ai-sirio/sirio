//! The hover card a worktree card opens beside the sidebar: its branch, how
//! long ago it last moved, its project and one line per tab. Read-only; it
//! never takes focus and has no click handlers.

use std::time::Duration;

use ely_gpui_component::{
    primitives::{Icon as EIcon, IconName},
    theme::IconSize as EIconSize,
};
use gpui::{
    AnyElement, Bounds, Context, FontWeight, Pixels, Point, Task, div, point, prelude::*,
    px,
};
use sirio_project::TabKind;
use sirio_theme::Theme;

use super::icons::{Icon, IconElement, IconSize};
use super::{Sidebar, SidebarPill, SidebarRow, sessions};
use crate::loading;
use crate::status::{ActivityStatus, status_color};

/// Width of the card.
pub(super) const HOVER_CARD_WIDTH: f32 = 290.0;
/// How long the pointer rests on a worktree card before the hover card opens.
pub(super) const OPEN_DELAY: Duration = Duration::from_millis(500);
/// How long the hover card stays after the pointer leaves.
pub(super) const LINGER: Duration = Duration::from_millis(200);
/// Space between the sidebar's right edge and the card.
pub(super) const GAP: f32 = 4.0;
/// The panel's own border, which the sidebar's bounds exclude: the gap is
/// measured from the visible edge, border included.
pub(super) const PANEL_BORDER: f32 = 1.0;
/// A sidebar narrower than this never shows the card.
pub(super) const MIN_SIDEBAR_WIDTH: f32 = 200.0;

/// Whether the window is wide enough to hold the card beside the sidebar. A
/// narrower one would push the card left over the row it is about, covering
/// its star and cross and taking their clicks, so the card does not open.
pub(super) fn fits(sidebar_right: Pixels, window_width: Pixels) -> bool {
    sidebar_right + px(GAP) + px(HOVER_CARD_WIDTH) <= window_width
}

/// Where the card's top-left corner goes: past the sidebar's right edge, level
/// with the row, never above the window. Its bottom edge is the anchored
/// layer's to fit, once the card's real height is laid out. Callers check
/// [`fits`] first.
pub(super) fn card_origin(row: Bounds<Pixels>, sidebar_right: Pixels) -> Point<Pixels> {
    point(sidebar_right + px(GAP), row.origin.y.max(px(0.0)))
}

/// What the hover card is doing: which worktree card it is about, where that
/// card is, whether it is open, and the timer that opens or closes it.
#[derive(Default)]
pub(super) struct HoverCardState {
    pub(super) row_id: Option<usize>,
    pub(super) row_bounds: Option<Bounds<Pixels>>,
    pub(super) shown: bool,
    /// The wall clock when the card opened; its times are read from this.
    pub(super) opened_at_ms: i64,
    /// Where the list was scrolled when the card was asked for; a different
    /// offset closes it.
    pub(super) scroll_y: Pixels,
    /// The pointer is on the open card itself.
    on_card: bool,
    timer: Option<Task<()>>,
}

impl HoverCardState {
    /// The pointer is on a worktree card. An open card moves to it at once; a
    /// closed one opens after [`OPEN_DELAY`].
    pub(super) fn hover(
        &mut self,
        row_id: usize,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Sidebar>,
    ) {
        self.row_id = Some(row_id);
        self.row_bounds = Some(bounds);
        self.on_card = false;
        if self.shown {
            self.timer = None;
            self.opened_at_ms = sessions::unix_now_ms();
            cx.notify();
            return;
        }
        self.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(OPEN_DELAY).await;
            this.update(cx, |sidebar, cx| {
                let state = &mut sidebar.hover_card;
                if state.row_id == Some(row_id) && !state.shown {
                    state.shown = true;
                    state.opened_at_ms = sessions::unix_now_ms();
                    state.timer = None;
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// The pointer is on the open card itself: it stays, and a worktree card's
    /// leave that arrives after this one (a move straight from the card onto
    /// the hover card is one event) does not start the linger.
    pub(super) fn keep(&mut self) {
        if self.shown {
            self.on_card = true;
            self.timer = None;
        }
    }

    /// The pointer left the open hover card.
    pub(super) fn leave_card(&mut self, cx: &mut Context<Sidebar>) {
        self.on_card = false;
        self.linger(cx);
    }

    /// The pointer left worktree card `row_id`. Ignored when the card is about
    /// another row (a move from one card to the next reports the new row's
    /// enter first) or when the pointer is on the hover card. A card not yet
    /// open never opens; an open one stays for [`LINGER`].
    pub(super) fn leave(&mut self, row_id: usize, cx: &mut Context<Sidebar>) {
        if self.row_id != Some(row_id) || self.on_card {
            return;
        }
        if !self.shown {
            self.dismiss();
            return;
        }
        self.linger(cx);
    }

    fn linger(&mut self, cx: &mut Context<Sidebar>) {
        if !self.shown {
            return;
        }
        self.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LINGER).await;
            this.update(cx, |sidebar, cx| {
                sidebar.hover_card.dismiss();
                cx.notify();
            })
            .ok();
        }));
    }

    /// Closes the card and cancels whatever was about to open or close it.
    pub(super) fn dismiss(&mut self) {
        self.row_id = None;
        self.row_bounds = None;
        self.shown = false;
        self.on_card = false;
        self.timer = None;
    }
}

/// What a tab's line starts with.
#[derive(Debug, PartialEq)]
pub(super) enum Lead {
    Unread,
    Bloom(ActivityStatus),
    Terminal,
    Nothing,
}

pub(super) fn lead_kind(pill: &SidebarPill) -> Lead {
    if pill.unread {
        return Lead::Unread;
    }
    match pill.status {
        Some(status) if status != ActivityStatus::Idle => Lead::Bloom(status),
        _ if pill.kind == TabKind::Terminal => Lead::Terminal,
        _ => Lead::Nothing,
    }
}

/// What a tab's line starts with: its unread dot, or its status bloom, or the
/// terminal glyph.
fn lead(pill: &SidebarPill, theme: &Theme) -> AnyElement {
    let slot = div().w(px(14.0)).h(px(14.0)).flex_none().flex().items_center().justify_center();
    match lead_kind(pill) {
        Lead::Unread => slot
            .child(div().size(px(7.0)).rounded_full().bg(theme.ely.info))
            .into_any_element(),
        Lead::Bloom(status) => slot
            .child(loading::settled_bloom(
                "sidebar-hover-card-bloom",
                loading::BLOOM_GLYPH,
                status_color(status, *theme),
            ))
            .into_any_element(),
        Lead::Terminal => slot
            .child(IconElement::new(Icon::SquareTerminal, IconSize::XSmall).text_color(theme.ely.fg_subtle))
            .into_any_element(),
        Lead::Nothing => slot.into_any_element(),
    }
}

/// The card's content: branch and time, project, a rule, one line per tab.
pub(super) fn render_card(
    row: &SidebarRow,
    project_name: &str,
    pills: &[SidebarPill],
    now_ms: i64,
    theme: &Theme,
) -> AnyElement {
    let last_event = pills.iter().filter_map(|pill| pill.last_event_at).max();
    div()
        .w(px(HOVER_CARD_WIDTH))
        .p(px(12.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(theme.ely.border)
        .bg(theme.ely.overlay)
        .text_size(theme.typography.scaled(13.0))
        .text_color(theme.ely.fg)
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(8.0))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .line_clamp(2)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(row.title.clone()),
                )
                .when_some(last_event, |this, at| {
                    this.child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .text_size(theme.typography.scaled(12.0))
                            .text_color(theme.ely.fg_subtle)
                            .child(EIcon::new(IconName::Clock).size(EIconSize::Xs).color(theme.ely.fg_subtle))
                            .child(sessions::relative_time(now_ms, at)),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_color(theme.ely.fg_muted)
                .debug_selector({
                    let name = project_name.to_owned();
                    move || format!("sidebar-hover-card-project-{name}")
                })
                .child(EIcon::new(IconName::Folder).size(EIconSize::Sm).color(theme.ely.fg_muted))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(project_name.to_owned()),
                ),
        )
        .when(!pills.is_empty(), |this| {
            this.child(div().h(px(1.0)).w_full().bg(theme.ely.border)).children(
                pills.iter().map(|pill| {
                    let title = pill.title.clone();
                    div()
                        .debug_selector(move || format!("sidebar-hover-card-tab-{title}"))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .h(px(14.0))
                        .child(lead(pill, theme))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(pill.title.clone()),
                        )
                }),
            )
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    fn row_at(y: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(8.0), px(y)), size(px(284.0), px(53.0)))
    }

    fn origin(row_y: f32) -> Point<Pixels> {
        card_origin(row_at(row_y), px(300.0))
    }

    fn pill(kind: TabKind, status: Option<ActivityStatus>, unread: bool) -> SidebarPill {
        SidebarPill {
            tab_id: Some(1),
            parked_tab: None,
            title: "t".to_owned(),
            icon: Icon::SquareTerminal,
            brand: None,
            status,
            selected: false,
            kind,
            persistence_id: String::new(),
            last_event_at: None,
            unread,
        }
    }

    #[test]
    fn an_idle_agent_tab_leads_with_nothing_and_a_terminal_with_its_glyph() {
        assert_eq!(lead_kind(&pill(TabKind::AgentChat, None, false)), Lead::Nothing);
        assert_eq!(lead_kind(&pill(TabKind::AgentChat, Some(ActivityStatus::Idle), false)), Lead::Nothing);
        assert_eq!(lead_kind(&pill(TabKind::Terminal, None, false)), Lead::Terminal);
    }

    #[test]
    fn a_tab_with_news_leads_with_its_dot_or_bloom_whatever_its_kind() {
        for kind in [TabKind::AgentChat, TabKind::Terminal] {
            assert_eq!(lead_kind(&pill(kind, None, true)), Lead::Unread);
            assert_eq!(
                lead_kind(&pill(kind, Some(ActivityStatus::Running), false)),
                Lead::Bloom(ActivityStatus::Running)
            );
        }
    }

    #[test]
    fn the_card_sits_beside_the_sidebar_at_the_row_top() {
        assert_eq!(origin(250.0), point(px(304.0), px(250.0)));
    }

    #[test]
    fn a_row_partly_above_the_window_clamps_to_zero() {
        assert_eq!(origin(-20.0).y, px(0.0));
    }
}
