//! The Activity surface of the right panel: the agent/terminal rows and
//! their status glyphs.

use super::*;
use gpui::Rgba;

const ACTIVITY_HEADER_HEIGHT: f32 = 27.0;

impl RightPanel {

    fn toggle_activity(&mut self, cx: &mut Context<Self>) {
        self.activity_expanded = !self.activity_expanded;
        cx.notify();
    }

    pub(super) fn render_activity(&self, entity: gpui::Entity<Self>, theme: Theme) -> impl IntoElement {
        let toggle_entity = entity.clone();
        let running_count = self
            .activity
            .iter()
            .filter(|surface| surface.status == ActivityStatus::Running)
            .count();
        let mut section = div()
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .h(px(ACTIVITY_HEADER_HEIGHT
                + if self.activity_expanded {
                    // F-CHG-20: even with zero rows, the expanded section
                    // still renders one "No activity" placeholder row, so
                    // the height must reserve space for at least one row —
                    // otherwise that row is squeezed into near-zero visible
                    // height and its text renders as illegible specks.
                    self.activity.len().max(1) as f32 * ACTIVITY_ROW_HEIGHT
                } else {
                    0.0
                }))
            .flex()
            .flex_col()
            .w_full()
            .flex_none()
            .bg(theme.background)
            .border_t_1()
            .border_color(theme.hairline)
            .child(
                div()
                    .id("activity-header")
                    .debug_selector(|| "activity-header".into())
                    .h(px(ACTIVITY_HEADER_HEIGHT))
                    .w_full()
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .text_size(theme.typography.footnote)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.title)
                    .hover(|style| style.bg(theme.row_hover))
                    .on_click(move |_, _, cx| {
                        toggle_entity.update(cx, |panel, cx| panel.toggle_activity(cx));
                    })
                    .child(if self.activity_expanded {
                        IconElement::new(Icon::ChevronDown, IconSize::XSmall)
                            .text_color(theme.title)
                    } else {
                        IconElement::new(Icon::ChevronRight, IconSize::XSmall)
                            .text_color(theme.title)
                    })
                    .child("Activity")
                    // F-CHG-20: the running count. A quiet meta label beside
                    // the header, present only while something is running,
                    // so a live worktree is visible from the collapsed
                    // header alone.
                    .when(running_count > 0, |this| {
                        this.child(
                            div()
                                .id("activity-running-count")
                                .debug_selector(|| "activity-running-count".into())
                                .text_size(theme.typography.caption2)
                                .text_color(theme.meta)
                                .child(format!("{running_count} running")),
                        )
                    }),
            );
        if self.activity_expanded {
            if self.activity.is_empty() {
                // F-CHG-20: the no-activity empty state, only visible while
                // the section is expanded — the collapsed header stays
                // silent instead of shouting about nothing.
                section = section.child(
                    div()
                        .id("activity-empty")
                        .debug_selector(|| "activity-empty".into())
                        .h(px(ACTIVITY_ROW_HEIGHT))
                        .w_full()
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .text_size(theme.typography.footnote)
                        .text_color(theme.meta)
                        .child("No activity"),
                );
            } else {
                for (index, surface) in self.activity.iter().cloned().enumerate() {
                    section = section.child(Self::render_activity_row(
                        surface,
                        index,
                        entity.clone(),
                        theme,
                    ));
                }
            }
        }
        section
    }

    fn render_activity_row(
        surface: ActivitySurface,
        index: usize,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let status = activity_status(surface.status, theme);
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
                    .text_color(theme.tab_focus_accent)
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
                            .text_color(theme.title)
                            .child(surface.title),
                    )
                    .child(
                        div()
                            .text_size(theme.typography.footnote)
                            .text_color(theme.meta)
                            .child(surface.location),
                    ),
            )
            .child(
                div()
                    .id(status_id.clone())
                    .debug_selector(move || status_id.clone())
                    .text_size(px(11.0))
                    .text_color(status)
                    .child(activity_status_glyph(surface.status)),
            )
            .child(
                div()
                    .id(format!("activity-close-{index}"))
                    .text_size(px(16.0))
                    .text_color(theme.subtitle)
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        close_entity.update(cx, |_, cx| {
                            cx.emit(RightPanelEvent::CloseActivity(index));
                        });
                    })
                    .child(IconElement::new(Icon::Close, IconSize::XSmall).text_color(theme.title)),
            )
    }

}


fn activity_status(status: ActivityStatus, theme: Theme) -> Rgba {
    match status {
        ActivityStatus::Idle => theme.meta,
        ActivityStatus::Running => theme.accent,
        ActivityStatus::NeedsInput => theme.tab_needs_input,
        ActivityStatus::Done => theme.tab_done,
        ActivityStatus::Error => theme.tab_error,
    }
}

fn activity_status_glyph(status: ActivityStatus) -> &'static str {
    match status {
        ActivityStatus::Idle => "○",
        ActivityStatus::Running => "●",
        ActivityStatus::NeedsInput => "?",
        ActivityStatus::Done => "✓",
        ActivityStatus::Error => "!",
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};

    /// F-CHG-20 (empty half): with no activity rows, the expanded section
    /// states No activity instead of showing nothing, and no running count
    /// is offered.
    #[gpui::test]
    async fn activity_section_states_no_activity_when_empty(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx
            .add_window(|_window, _cx| RightPanel::with_activity(std::env::temp_dir(), Vec::new()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let header = cx
            .debug_bounds("activity-header")
            .expect("the activity header is drawn");
        cx.simulate_click(header.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("activity-empty").is_some(),
            "an expanded section with no rows states No activity (F-CHG-20)"
        );
        assert!(
            cx.debug_bounds("activity-running-count").is_none(),
            "nothing runs, so no running count is offered"
        );
    }

    /// F-CHG-20 (running-count half): with a running row in the list, the
    /// header states the count while rows render below it.
    #[gpui::test]
    async fn activity_section_states_the_running_count(cx: &mut TestAppContext) {
        cx.update(Theme::init);
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

        // The count is visible from the collapsed header alone.
        assert!(
            cx.debug_bounds("activity-running-count").is_some(),
            "the running count is stated beside the header"
        );

        let header = cx
            .debug_bounds("activity-header")
            .expect("the activity header is drawn");
        cx.simulate_click(header.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("activity-0").is_some() && cx.debug_bounds("activity-1").is_some(),
            "the activity rows render under the expanded header"
        );
        assert!(
            cx.debug_bounds("activity-empty").is_none(),
            "rows exist, so the empty state is not shown"
        );
        assert!(
            cx.debug_bounds("activity-running-count").is_some(),
            "the running count stays visible with rows present"
        );
    }

    /// F-CHG-22: NeedsInput has its own drawn status marker and is not the
    /// same visual state as Idle.
    #[gpui::test]
    async fn activity_section_draws_needs_input_as_distinct_from_idle(cx: &mut TestAppContext) {
        cx.update(Theme::init);
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
                ],
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let header = cx
            .debug_bounds("activity-header")
            .expect("the activity header is drawn");
        cx.simulate_click(header.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("activity-status-needs-input-0").is_some(),
            "NeedsInput reaches a dedicated status marker in the drawn panel"
        );
        assert_ne!(
            activity_status_glyph(ActivityStatus::NeedsInput),
            activity_status_glyph(ActivityStatus::Idle),
            "NeedsInput is not rendered with Idle's glyph"
        );
    }

}

