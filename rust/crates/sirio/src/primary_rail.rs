//! The Primary half collapsed to a rail of its tabs' icons (spec
//! 2026-11-01-primary-rail §4).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use gpui::{Bounds, Pixels, canvas};

use super::*;

/// Window bounds of the rail's controls from their last paint, for the
/// debug `railRects` read the E2E clicks by.
#[derive(Default)]
pub(crate) struct RailRects {
    pub(crate) expand: Cell<Option<Bounds<Pixels>>>,
    /// Keyed by tab id; cleared at the start of each rail render so a
    /// closed tab's rect does not linger.
    pub(crate) tabs: RefCell<BTreeMap<usize, Bounds<Pixels>>>,
}

impl SirioWorkspace {
    /// The rail: the expand button over one button per Primary tab, with the
    /// TabBar's `+` pinned at the foot. Drawn in place of the Primary half
    /// while `primary_rail_drawn`.
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
                        entity.update(cx, |workspace, cx| workspace.expand_primary(window, cx));
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
            .into_any_element()
    }
}
