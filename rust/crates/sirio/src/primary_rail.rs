//! The Primary half collapsed to a rail of its tabs' icons (spec
//! 2026-11-01-primary-rail §4).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use gpui::{Bounds, Pixels, canvas};

use super::*;

/// The peek's gap to the centre's top and bottom edges.
const PEEK_INSET: f32 = 6.0;

/// Window bounds of the rail's controls from their last paint, for the
/// debug `railRects` read the E2E clicks by.
#[derive(Default)]
pub(crate) struct RailRects {
    pub(crate) expand: Cell<Option<Bounds<Pixels>>>,
    /// Keyed by tab id; cleared at the start of each rail render so a
    /// closed tab's rect does not linger.
    pub(crate) tabs: RefCell<BTreeMap<usize, Bounds<Pixels>>>,
    /// The rail's own bounds; a press inside it does not count as "outside the peek".
    pub(crate) rail: Cell<Option<Bounds<Pixels>>>,
    /// The peek card.
    pub(crate) peek: Cell<Option<Bounds<Pixels>>>,
    /// The peek's pin button.
    pub(crate) pin: Cell<Option<Bounds<Pixels>>>,
}

impl SirioWorkspace {
    /// The rail: the expand button over one button per Primary tab, with the
    /// TabBar's `+` pinned at the foot. Drawn in place of the Primary half
    /// while `primary_rail_drawn`. A click on a tab's button toggles the peek
    /// on that tab (`render_primary_peek`).
    pub(super) fn render_primary_rail(
        &self,
        theme: &Theme,
        entity: Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        // The hover closures below are `'static`, so they take a copy.
        let theme = *theme;
        let rects = self.rail_rects.clone();
        rects.tabs.borrow_mut().clear();

        let expand_rects = rects.clone();
        let expand_entity = entity.clone();
        let expand = div()
            .h(px(TAB_BAR_HEIGHT))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            // Lines up with the Secondary strip's hairline.
            .border_b_1()
            .border_color(theme.ely.border)
            .child(
                div()
                    .id("primary-rail-expand")
                    .debug_selector(|| "primary-rail-expand".to_owned())
                    .w(px(24.0))
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radii.control)
                    .text_color(theme.ely.fg_subtle)
                    .hover(|style| style.bg(theme.ely.hover))
                    .tooltip(sirio_ui::controls::tooltip("Expand Pane"))
                    .on_click(move |_, window, cx| {
                        expand_entity
                            .update(cx, |workspace, cx| workspace.expand_primary(window, cx));
                    })
                    .child(
                        canvas(
                            move |bounds, _, _| expand_rects.expand.set(Some(bounds)),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .child(
                        IconElement::new(Icon::PanelLeftOpen, IconSize::Small)
                            .text_color(theme.ely.fg_subtle),
                    ),
            );

        let active_id = self.center_split.active(PaneRole::Primary);
        let mut tab_list = div()
            .id("primary-rail-tabs")
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.0))
            .py(px(5.0));
        for id in self.center_split.tabs_for(PaneRole::Primary, &self.tabs) {
            let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) else {
                continue;
            };
            let agent = self.tab_agent_mark(tab);
            let file = tab_file_path(tab, cx);
            let icon = tab_icon(tab.kind, file.as_deref(), agent.map(|agent| agent.icon));
            let glyph_color = tab_glyph_color(tab.kind, icon, agent, file.is_some(), theme);
            // Idle is drawn too: a tab with no status counts as Idle.
            let status = self.tab_status(tab, cx).unwrap_or(ActivityStatus::Idle);
            let tab_rects = rects.clone();
            tab_list = tab_list.child(
                div()
                    .id(format!("primary-rail-tab-{id}"))
                    .debug_selector(move || format!("primary-rail-tab-{id}"))
                    .relative()
                    .w(px(30.0))
                    .h(px(30.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radii.control)
                    .hover(|style| style.bg(theme.ely.hover))
                    .when(active_id == Some(id), |this| this.bg(theme.ely.active))
                    .on_click({
                        let entity = entity.clone();
                        move |_, window, cx| {
                            entity.update(cx, |workspace, cx| {
                                workspace.toggle_primary_peek(id, window, cx)
                            });
                        }
                    })
                    .tooltip(sirio_ui::controls::tooltip(format!(
                        "{} · {}",
                        tab.title,
                        tab_status_name(status)
                    )))
                    .child(
                        div()
                            .text_color(glyph_color)
                            .child(IconElement::new(icon, IconSize::Small)),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(2.0))
                            .right(px(2.0))
                            .w(px(6.0))
                            .h(px(6.0))
                            .rounded_full()
                            .bg(sirio_ui::status::status_color(status, theme)),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                tab_rects.tabs.borrow_mut().insert(id, bounds);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    ),
            );
        }

        let rail_rects = rects.clone();
        div()
            .id("primary-rail")
            .debug_selector(|| "primary-rail".into())
            .flex()
            .flex_col()
            .w(px(panel_layout::PRIMARY_RAIL_WIDTH))
            .flex_none()
            .h_full()
            .min_h_0()
            .bg(theme.ely.bg)
            .border_r_1()
            .border_color(theme.ely.border)
            .child(expand)
            .child(tab_list)
            // The TabBar in its rail placement draws the `+` and hangs its
            // menu; outside the scrolling list, so the `+` stays pinned. A
            // cached view is laid out to this box and clipped to it, so the
            // box needs a height of its own: the 30 px `+` and the bar's
            // 5 px above and below.
            .child(
                div()
                    .flex_none()
                    .w_full()
                    .h(px(40.0))
                    .child(self.child_view(self.tab_bar.clone())),
            )
            .child(
                canvas(
                    move |bounds, _, _| rail_rects.rail.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }

    /// The peek (spec 2026-11-01-primary-rail §5): the peeked tab's own pane
    /// tree in a card hung off the rail's right edge, as wide as the expanded
    /// Primary half would be. The only place that view is drawn while the rail
    /// is, so its PTY size has one owner. Opaque: translucency is never in the
    /// foreground. `None` while no peek is open.
    pub(super) fn render_primary_peek(
        &self,
        theme: &Theme,
        entity: Entity<Self>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = *self.primary_peek.get()?;
        if !self.primary_rail_drawn() {
            return None;
        }
        let index = self
            .tabs
            .iter()
            .position(|tab| tab.id == id && tab.pane == PaneRole::Primary)?;
        let room = panel_layout::resolve_collapsed_center(
            self.center_available_width(window, *theme, true),
            self.center_split_ratio,
            CENTER_DIVIDER_WIDTH,
        )
        .secondary;
        // The width comes from the layout the pin would draw: its side panels
        // give up two panes' floors, not the rail and one, so its centre can
        // be wider than this one (spec §5).
        let expanded = panel_layout::resolve_collapsed_center(
            self.center_width_for_floor(
                window,
                *theme,
                panel_layout::min_center_width(true, CENTER_DIVIDER_WIDTH),
            ),
            self.center_split_ratio,
            CENTER_DIVIDER_WIDTH,
        );
        // The card's 1 px border on each side lies outside the half's width,
        // so its body, and a peeked terminal's column count, match the half.
        let width = (expanded.peek + 2.0).min(room);
        // The closures below are `'static`, so they take a copy.
        let theme = *theme;
        let rects = self.rail_rects.clone();

        let tab = &self.tabs[index];
        let agent = self.tab_agent_mark(tab);
        let file = tab_file_path(tab, cx);
        let icon = tab_icon(tab.kind, file.as_deref(), agent.map(|agent| agent.icon));
        let glyph_color = tab_glyph_color(tab.kind, icon, agent, file.is_some(), theme);
        // Idle is drawn too: a tab with no status counts as Idle.
        let status = self.tab_status(tab, cx).unwrap_or(ActivityStatus::Idle);

        let pin_rects = rects.clone();
        let pin_entity = entity.clone();
        let header = div()
            .flex_none()
            .h(px(30.0))
            .w_full()
            .flex()
            .items_center()
            .gap(px(6.0))
            .pl(px(8.0))
            .pr(px(3.0))
            .border_b_1()
            .border_color(theme.ely.border)
            .child(
                div()
                    .flex_none()
                    .text_color(glyph_color)
                    .child(IconElement::new(icon, IconSize::Small)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(theme.typography.scaled(13.0))
                    .child(tab.title.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(6.0))
                    .h(px(6.0))
                    .rounded_full()
                    .bg(sirio_ui::status::status_color(status, theme)),
            )
            .child(
                div()
                    .id("primary-peek-pin")
                    .debug_selector(|| "primary-peek-pin".to_owned())
                    .flex_none()
                    .w(px(24.0))
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radii.control)
                    .text_color(theme.ely.fg_subtle)
                    .hover(|style| style.bg(theme.ely.hover))
                    .tooltip(sirio_ui::controls::tooltip("Expand Pane"))
                    .on_click(move |_, window, cx| {
                        pin_entity.update(cx, |workspace, cx| workspace.expand_primary(window, cx));
                    })
                    .child(
                        canvas(
                            move |bounds, _, _| pin_rects.pin.set(Some(bounds)),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .child(
                        IconElement::new(Icon::PanelLeftOpen, IconSize::Small)
                            .text_color(theme.ely.fg_subtle),
                    ),
            );

        // No padding: a peeked terminal must keep the half's column count.
        let body = div()
            .id("primary-peek-body")
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .child(self.render_pane_tree(
                &self.tabs[index].panes,
                index,
                entity,
                Vec::new(),
                cx,
            ));

        let peek_rects = rects.clone();
        Some(
            div()
                .id("primary-peek")
                .debug_selector(|| "primary-peek".into())
                .absolute()
                .left(px(panel_layout::PRIMARY_RAIL_WIDTH))
                .top(px(PEEK_INSET))
                .bottom(px(PEEK_INSET))
                .w(px(width))
                .flex()
                .flex_col()
                .overflow_hidden()
                .rounded(theme.radii.shell_panel)
                .border_1()
                .border_color(theme.ely.border_strong)
                .bg(theme.sirio.floating_surface)
                .text_color(theme.ely.fg)
                .shadow_lg()
                // A press on the card stays the card's: the root's close on a
                // press outside (`render`) runs only where nothing occludes.
                .occlude()
                .child(
                    canvas(
                        move |bounds, _, _| peek_rects.peek.set(Some(bounds)),
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .child(header)
                .child(body)
                .into_any_element(),
        )
    }
}
