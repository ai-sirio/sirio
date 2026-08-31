//! The Activity surface of the right panel: the agent/terminal rows and
//! their status glyphs.

use super::*;
use crate::loading;

impl RightPanel {
    pub(super) fn render_activity(
        &self,
        entity: gpui::Entity<Self>,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        if self.activity.is_empty() {
            // F-CHG-20: the empty activity view states the absence instead
            // of drawing nothing at all.
            div()
                .id("activity-empty")
                .debug_selector(|| "activity-empty".into())
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(theme.typography.footnote)
                .text_color(theme.text_faint)
                .child("No activity")
                .into_any_element()
        } else {
            let mut list = div().flex_1().min_h(px(0.0)).flex().flex_col();
            for (index, surface) in self.activity.iter().cloned().enumerate() {
                list = list.child(Self::render_activity_row(
                    surface,
                    index,
                    entity.clone(),
                    theme,
                    window,
                    cx,
                ));
            }
            list.into_any_element()
        }
    }

    fn render_activity_row(
        surface: ActivitySurface,
        index: usize,
        entity: gpui::Entity<Self>,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let status = super::status_color(surface.status, theme);
        let status_name = match surface.status {
            ActivityStatus::Idle => "idle",
            ActivityStatus::Running => "running",
            ActivityStatus::NeedsInput => "needs-input",
            ActivityStatus::Done => "done",
            ActivityStatus::Error => "error",
        };
        let status_id = format!("activity-status-{status_name}-{index}");
        let select_entity = entity.clone();
        let close_entity = entity;
        div()
            .id(format!("activity-{index}"))
            .debug_selector(move || format!("activity-{index}"))
            .h(px(ACTIVITY_ROW_HEIGHT))
            .w_full()
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .hover(|style| style.bg(theme.row_hover))
            .on_click(move |_, _, cx| {
                select_entity.update(cx, |_, cx| {
                    cx.emit(RightPanelEvent::SelectActivity(index));
                });
            })
            .child(
                div()
                    .w(px(15.0))
                    .text_color(theme.text)
                    .child(IconElement::new(surface.icon, IconSize::Small)),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(theme.typography.headline)
                            .text_color(theme.text)
                            .child(surface.title),
                    )
                    .child(
                        div()
                            .text_size(theme.typography.footnote)
                            .text_color(theme.text_faint)
                            .child(surface.location),
                    ),
            )
            .child(
                div()
                    .id(status_id.clone())
                    .debug_selector(move || status_id.clone())
                    .text_size(px(12.0))
                    .text_color(status)
                    .child(activity_status_glyph(surface.status, index, window, cx)),
            )
            .child(
                div()
                    .id(format!("activity-close-{index}"))
                    .text_size(px(17.0))
                    .text_color(theme.text_muted)
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        close_entity.update(cx, |_, cx| {
                            cx.emit(RightPanelEvent::CloseActivity(index));
                        });
                    })
                    .child(IconElement::new(Icon::Close, IconSize::XSmall).text_color(theme.text)),
            )
    }
}

fn activity_status_glyph(
    status: ActivityStatus,
    index: usize,
    window: &mut Window,
    cx: &mut Context<RightPanel>,
) -> gpui::AnyElement {
    let status_name = match status {
        ActivityStatus::Idle => "idle",
        ActivityStatus::Running => "running",
        ActivityStatus::NeedsInput => "needs-input",
        ActivityStatus::Done => "done",
        ActivityStatus::Error => "error",
    };
    let glyph_id = if status == ActivityStatus::Running {
        format!("activity-running-spinner-{index}")
    } else {
        format!("activity-status-glyph-{status_name}-{index}")
    };
    match status {
        ActivityStatus::Running => div()
            .id(("activity-running-spinner", index))
            .debug_selector(move || glyph_id.clone())
            .child(loading::compact("activity-running-spinner", window, cx))
            .into_any_element(),
        ActivityStatus::Idle => div()
            .id(glyph_id.clone())
            .debug_selector(move || glyph_id.clone())
            .child("○")
            .into_any_element(),
        ActivityStatus::NeedsInput => div()
            .id(glyph_id.clone())
            .debug_selector(move || glyph_id.clone())
            .child("?")
            .into_any_element(),
        ActivityStatus::Done => div()
            .id(glyph_id.clone())
            .debug_selector(move || glyph_id.clone())
            .child("✓")
            .into_any_element(),
        ActivityStatus::Error => div()
            .id(glyph_id.clone())
            .debug_selector(move || glyph_id)
            .child("!")
            .into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext};

    /// F-CHG-20 (empty half): with no activity rows, the Activity view
    /// states No activity instead of showing nothing.
    #[gpui::test]
    async fn activity_section_states_no_activity_when_empty(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(|cx| PanelView::set(PanelView::Activity, cx));
        let window = cx
            .add_window(|_window, _cx| RightPanel::with_activity(std::env::temp_dir(), Vec::new()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("activity-empty").is_some(),
            "an empty activity view states No activity (F-CHG-20)"
        );
        assert!(
            cx.debug_bounds("activity-0").is_none(),
            "nothing runs, so no row is drawn"
        );
    }

    /// The collapsible footer is gone: with rows present, the Activity view
    /// draws every row as a full-height list, and the header is no longer a
    /// separate element to click open.
    #[gpui::test]
    async fn activity_view_lists_rows_at_full_height(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(|cx| PanelView::set(PanelView::Activity, cx));
        let window = cx.add_window(|_window, _cx| {
            RightPanel::with_activity(
                std::env::temp_dir(),
                vec![
                    ActivitySurface::new(
                        Icon::MessageSquare,
                        "Chat",
                        "/repo",
                        ActivityStatus::Running,
                    ),
                    ActivitySurface::new(Icon::File, "Changes", "/repo", ActivityStatus::Done),
                ],
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("activity-0").is_some() && cx.debug_bounds("activity-1").is_some(),
            "the activity rows render as a full-height list"
        );
        assert!(
            cx.debug_bounds("activity-empty").is_none(),
            "rows exist, so the empty state is not shown"
        );
        assert!(
            cx.debug_bounds("activity-header").is_none(),
            "the collapsible header is gone"
        );
    }

    /// F-CHG-22: NeedsInput has its own drawn status marker and is not the
    /// same visual state as Idle.
    #[gpui::test]
    async fn activity_section_draws_needs_input_as_distinct_from_idle(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(|cx| PanelView::set(PanelView::Activity, cx));
        let window = cx.add_window(|_window, _cx| {
            RightPanel::with_activity(
                std::env::temp_dir(),
                vec![
                    ActivitySurface::new(
                        Icon::MessageSquare,
                        "Waiting agent",
                        "/repo",
                        ActivityStatus::NeedsInput,
                    ),
                    ActivitySurface::new(Icon::File, "Idle surface", "/repo", ActivityStatus::Idle),
                    ActivitySurface::new(
                        Icon::MessageSquare,
                        "Running agent",
                        "/repo",
                        ActivityStatus::Running,
                    ),
                ],
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("activity-status-needs-input-0").is_some(),
            "NeedsInput reaches a dedicated status marker in the drawn panel"
        );
        assert!(
            cx.debug_bounds("activity-status-glyph-needs-input-0")
                .is_some(),
            "NeedsInput draws its own glyph instead of Idle's glyph"
        );
        assert!(
            cx.debug_bounds("activity-status-glyph-idle-1").is_some(),
            "Idle draws its own glyph instead of NeedsInput's glyph"
        );
        assert!(
            cx.debug_bounds("activity-running-spinner-2").is_some(),
            "Running draws the compact spinner in its status slot"
        );
        assert!(
            cx.debug_bounds("activity-running-spinner-1").is_none(),
            "Idle does not draw the running spinner"
        );
    }
}
