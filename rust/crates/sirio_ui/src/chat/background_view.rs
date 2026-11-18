//! The background tasks' small surfaces (A3): the popover the composer's task
//! chip opens, and the Stop button every surface of them draws.
//!
//! The popover lists the live tasks, each with its kind and a Stop, and has no
//! tail (A3 ruling R2). Whether it is open stays with `Chat`, which closes it
//! when no task is left (`background_changed`); Escape closes it before it
//! would cancel the turn (`Chat::cancel`).

use super::controls::popup_width;
use super::*;
use ely_gpui_component::menus::{Hang, layer, panel_surface};
use gpui::{CursorStyle, StatefulInteractiveElement as _};

/// The Stop control of a background task: `Stop`, or `Stopping…` once pressed,
/// when it is disabled and takes no click (R7: one press sends one `stop_task`).
/// `selector` names it for the tests; `press` runs on a click.
pub(crate) fn stop_button(
    selector: String,
    stopping: bool,
    theme: &Theme,
    press: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let button = div()
        .id(SharedString::from(selector.clone()))
        .debug_selector(move || selector.clone())
        .flex_none()
        .px(px(6.0))
        .py(px(1.0))
        .rounded(theme.radii.chip)
        .border_1()
        .border_color(theme.ely.border_strong)
        .text_size(theme.typography.footnote)
        .child(if stopping { "Stopping…" } else { "Stop" });
    if stopping {
        button.text_color(theme.ely.fg_subtle).into_any_element()
    } else {
        button
            .text_color(theme.ely.fg)
            .cursor(CursorStyle::PointingHand)
            .hover(|style| style.bg(theme.ely.hover))
            .on_click(move |_, window, cx| press(window, cx))
            .into_any_element()
    }
}

impl Chat {
    /// Whether the popover is drawn: open, and in a row that has the chip (the
    /// Narrow row keeps the count as a row of ⋯ and has no popover).
    pub(super) fn background_popover_visible(&self) -> bool {
        self.background_popover_open
            && self.composer_row.layout.get() < composer_row::RowLayout::Narrow
    }

    /// Closes the popover by whatever means but Escape. The keyboard it may hold goes back to
    /// the composer at the next render, which has the window this has not.
    pub(super) fn close_background_popover(&mut self) {
        if self.background_popover_open {
            self.background_popover_open = false;
            self.background_focus_return = true;
        }
    }

    /// Run at the top of every render: a popover the row no longer draws (it went Narrow) is
    /// closed, and one that closed hands the keyboard back, when it held it.
    pub(super) fn settle_background_popover(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.background_popover_open && !self.background_popover_visible() {
            self.close_background_popover();
        }
        if std::mem::take(&mut self.background_focus_return)
            && self.background_popover_focus.is_focused(window)
        {
            self.focus_composer(window, cx);
        }
    }

    /// The chip was pressed. A chip with no live task has nothing to list, so
    /// it opens nothing.
    pub(super) fn toggle_background_popover(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.background_popover_open {
            self.close_background_popover();
        } else if self.live_background_tasks().next().is_some() {
            self.model_picker_open = false;
            self.mode_picker_open = false;
            self.effort_picker_open = false;
            self.thinking_picker_open = false;
            self.context_popover_open = false;
            self.background_popover_open = true;
            let focus = self.background_popover_focus.clone();
            window.focus(&focus, cx);
            window.on_next_frame(move |window, _| {
                window.on_next_frame(move |window, cx| window.focus(&focus, cx));
            });
        }
        cx.notify();
    }

    /// The popover while it is open, hung from the chip.
    pub(super) fn render_background_popover(
        &self,
        theme: &Theme,
        hang: Hang,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if !self.background_popover_visible() {
            return None;
        }
        let footnote = theme.typography.footnote;
        let rows = self.live_background_tasks().map(|entry| {
            let task_id = entry.task_id.clone();
            let entity = cx.entity();
            div()
                .flex()
                .items_center()
                .gap_2()
                .py(px(3.0))
                .debug_selector({
                    let selector = format!("background-task-{task_id}");
                    move || selector.clone()
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(footnote)
                        .text_color(theme.ely.fg)
                        .child(entry.description.clone()),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(theme.typography.caption2)
                        .text_color(theme.ely.fg_subtle)
                        .child(entry.kind().to_string()),
                )
                .child(stop_button(
                    format!("background-task-stop-{task_id}"),
                    entry.stopping,
                    theme,
                    move |_, cx| {
                        entity.update(cx, |chat, cx| chat.stop_background_task(&task_id, cx));
                    },
                ))
        });
        Some(layer(
            "background-popover-menu",
            hang,
            div()
                .id("background-tasks-popover")
                .debug_selector(|| "background-tasks-popover".into())
                .key_context("ChatBackgroundPopover")
                .track_focus(&self.background_popover_focus)
                .on_action(cx.listener(Self::cancel))
                .w(popup_width(theme, 285.0))
                .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    // A press on the chip is the click that toggles: closing here would
                    // have that click open the popover again.
                    let on_chip = this
                        .composer_row
                        .background_chip
                        .get()
                        .is_some_and(|chip| chip.contains(&event.position));
                    if !on_chip {
                        this.close_background_popover();
                        cx.notify();
                    }
                }))
                .child(
                    panel_surface("background-popover-surface", cx)
                        .p_2()
                        .flex()
                        .flex_col()
                        .children(rows),
                ),
            cx,
        ))
    }
}

#[cfg(test)]
mod tests {
    // Failure list for the surfaces (A3 Task 4). It was drafted from the brief, and these tests
    // were written after the code they cover; each one is kept only because a named mutation of
    // the production code makes it fail (recorded in task-4-report.md). The chat is wrong if:
    //  1. Escape with the popover open also cancels the turn, or Escape with no popover drawn
    //     (a Narrow row hides it) stops cancelling;
    //  2. the chip opens a popover that lists no live task, or one is left open when the last task
    //     ends, or a second click on the chip does not close it (the outside-click that closes it
    //     must not be the click that reopens it);
    //  3. a popover that closes by itself (the last task ended, the row went Narrow) leaves the
    //     keyboard on a view that is no longer drawn, so typing goes nowhere;
    //  4. a Stop button sends no `stop_task`, one for another task, or a second one when pressed
    //     again (R7), or one surface's Stop does nothing (popover, card row, agent lane, tab strip);
    //  5. the Stop on an agent lane also reveals the lane's transcript row (it sits over the
    //     clickable header and must occlude it);
    //  6. a chat that is not drawn, a card that is folded, a tab that is not drawn, or a read of
    //     the control snapshots keeps the tail poll reading. The control of each is that the same
    //     surface, drawn, does read.

    use super::super::background::tests::Fake;
    use super::super::band_tests::configure_fitting_full_row;
    use super::super::composer_row::RowLayout;
    use super::super::tests::{
        CHAT_FIXTURE, pump_chat_until, push_answers, refresh_frame, top_entry,
    };
    use super::*;
    use crate::trace_tab::TraceTab;
    use gpui::{AppContext as _, Modifiers, Styled as _, TestAppContext, VisualTestContext, size};
    use sirio_chat::{BackgroundTaskEvent, BackgroundTaskRow, SpanKind, TraceEvent};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// The chat and the Trace tab beside it: each is drawn only while its flag says so,
    /// which stands in for it being hidden behind another tab.
    struct Host {
        chat: Entity<Chat>,
        tab: Entity<TraceTab>,
        show_chat: bool,
        show_tab: bool,
        /// Mount the chat cached, as the app does (see `setup_cached`).
        cached: bool,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .children(self.show_chat.then(|| {
                    if self.cached {
                        // As the app mounts a chat: a cached view draws again only when it is
                        // notified or resized.
                        self.chat
                            .clone()
                            .cached(gpui::StyleRefinement::default().flex_1().min_w_0().h_full())
                            .into_any_element()
                    } else {
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(self.chat.clone())
                            .into_any_element()
                    }
                }))
                .children(
                    self.show_tab
                        .then(|| div().w(px(600.0)).h_full().child(self.tab.clone())),
                )
        }
    }

    /// A connected chat with a Full composer row (the task chip drawn) in a wide window, its
    /// task source a fake that records what it is asked.
    fn setup(
        cx: &mut TestAppContext,
    ) -> (
        Entity<Host>,
        Entity<Chat>,
        Arc<Fake>,
        &mut VisualTestContext,
    ) {
        setup_mounted(cx, false)
    }

    /// `setup` with the chat mounted cached, as the app mounts it. The harness drops the debug
    /// bounds of a view a frame reuses, so a test that clicks selectors cannot use this host:
    /// the tests that read bounds use `setup`, and `the_popover...no_longer_draws...` needs none.
    fn setup_cached(
        cx: &mut TestAppContext,
    ) -> (
        Entity<Host>,
        Entity<Chat>,
        Arc<Fake>,
        &mut VisualTestContext,
    ) {
        setup_mounted(cx, true)
    }

    fn setup_mounted(
        cx: &mut TestAppContext,
        cached: bool,
    ) -> (
        Entity<Host>,
        Entity<Chat>,
        Arc<Fake>,
        &mut VisualTestContext,
    ) {
        cx.update(Theme::init);
        cx.update(crate::ely::init);
        cx.update(super::super::init);
        cx.update(crate::trace_tab::init);
        cx.executor().allow_parking();
        let (host, cx) = cx.add_window_view(|_, cx| {
            let command = AgentCommand::new("python3").arg(CHAT_FIXTURE).arg("plain");
            let chat = cx.new(|cx| {
                Chat::from_test_command(LaunchSpec::Acp(command), std::env::temp_dir(), cx)
            });
            let tab = cx.new(|cx| TraceTab::new(chat.clone(), cx));
            Host {
                chat,
                tab,
                show_chat: true,
                show_tab: false,
                cached,
            }
        });
        let chat = host.read_with(cx, |host, _| host.chat.clone());
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        let fake = Fake::answering("");
        chat.update(cx, |chat, cx| {
            configure_fitting_full_row(chat);
            // The chip is drawn from the CLI's own count.
            chat.background_tasks = 2;
            chat.task_source_override = Some(fake.clone());
            // A cached view draws again only when it is notified.
            cx.notify();
        });
        cx.simulate_resize(size(px(1920.0), px(1000.0)));
        settle(cx);
        (host, chat, fake, cx)
    }

    fn settle(cx: &mut VisualTestContext) {
        for _ in 0..4 {
            refresh_frame(cx);
        }
    }

    /// The same frames: spelled apart where a test means that nobody notified the chat.
    fn settle_quiet(cx: &mut VisualTestContext) {
        settle(cx);
    }

    /// The CLI says `event`. Every event re-reads the chip's count from the client, which has
    /// none to give a fixture chat, so the test puts back what the CLI would count: the live rows.
    fn feed(chat: &Entity<Chat>, cx: &mut VisualTestContext, event: BackgroundTaskEvent) {
        chat.update(cx, |chat, cx| {
            chat.handle_event(sirio_chat::ChatEvent::BackgroundTask(event), cx);
            chat.background_tasks = chat.live_background_tasks().count();
            cx.notify();
        });
        settle(cx);
    }

    fn row(task_id: &str, task_type: &str, description: &str) -> BackgroundTaskRow {
        BackgroundTaskRow {
            task_id: task_id.into(),
            task_type: Some(task_type.into()),
            description: description.into(),
        }
    }

    /// `bshell001` and `bshell002` shells, and `aagent001` started by the Agent call `toolu_a1`.
    fn start_tasks(chat: &Entity<Chat>, cx: &mut VisualTestContext) {
        feed(
            chat,
            cx,
            BackgroundTaskEvent::Rows(vec![
                row("bshell001", "local_bash", "first shell"),
                row("bshell002", "local_bash", "second shell"),
                row("aagent001", "local_agent", "the reviewer"),
            ]),
        );
        feed(
            chat,
            cx,
            BackgroundTaskEvent::Started {
                task_id: "aagent001".into(),
                tool_use_id: Some("toolu_a1".into()),
                task_type: Some("local_agent".into()),
                description: "the reviewer".into(),
            },
        );
    }

    /// A turn with the Agent call `toolu_a1` as a lane: the card and the tab have something to show.
    fn start_turn(chat: &Entity<Chat>, cx: &mut VisualTestContext) {
        chat.update(cx, |chat, cx| {
            let now = Instant::now() - Duration::from_secs(5);
            chat.handle_event(
                sirio_chat::ChatEvent::Trace(TraceEvent::TurnStarted { at: now }),
                cx,
            );
            chat.handle_event(
                sirio_chat::ChatEvent::Trace(TraceEvent::SpanStarted {
                    id: "toolu_a1".into(),
                    parent: None,
                    kind: SpanKind::Subagent,
                    name: "Reviewer".into(),
                    detail: "".into(),
                    model: None,
                    at: now + Duration::from_secs(1),
                }),
                cx,
            );
            chat.background_tasks = chat.live_background_tasks().count();
            cx.notify();
        });
        settle(cx);
    }

    /// The session panel opens with the Trace card unfolded.
    fn open_card(chat: &Entity<Chat>, cx: &mut VisualTestContext) {
        start_turn(chat, cx);
        chat.update_in(cx, |chat, window, cx| {
            chat.set_session_panel_open(true, cx);
            chat.session_panel_act(
                super::super::SessionPanelAction::Toggle("trace".into()),
                window,
                cx,
            )
            .expect("the card unfolds");
        });
        settle(cx);
    }

    fn drawn(cx: &mut VisualTestContext, selector: &str) -> gpui::Bounds<gpui::Pixels> {
        cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
            .unwrap_or_else(|| panic!("{selector} is drawn"))
    }

    fn is_drawn(cx: &mut VisualTestContext, selector: &str) -> bool {
        cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
            .is_some()
    }

    /// A real click at the middle of a drawn element.
    fn click(cx: &mut VisualTestContext, selector: &str) {
        let at = drawn(cx, selector).center();
        cx.simulate_click(at, Modifiers::none());
        cx.run_until_parked();
        settle(cx);
    }

    fn composer_is_focused(chat: &Entity<Chat>, cx: &mut VisualTestContext) -> bool {
        cx.update(|window, app| {
            chat.read(app)
                .composer_field
                .read(app)
                .focus_handle(app)
                .is_focused(window)
        })
    }

    fn popover_reported(chat: &Entity<Chat>, cx: &mut VisualTestContext) -> bool {
        chat.read_with(cx, |chat, _| chat.composer_snapshot().background_popover)
    }

    #[gpui::test]
    async fn the_chip_opens_a_popover_listing_each_live_task(cx: &mut TestAppContext) {
        let (_, chat, _, cx) = setup(cx);
        start_tasks(&chat, cx);
        assert!(!is_drawn(cx, "background-tasks-popover"));
        assert!(!popover_reported(&chat, cx));

        click(cx, "background-tasks-chip");
        let popover = drawn(cx, "background-tasks-popover");
        for task in ["bshell001", "bshell002", "aagent001"] {
            let task_row = drawn(cx, &format!("background-task-{task}"));
            let stop = drawn(cx, &format!("background-task-stop-{task}"));
            assert!(
                popover.contains(&task_row.center()) && task_row.contains(&stop.center()),
                "{task}'s row, with its Stop, is inside the popover"
            );
        }
        assert!(
            popover_reported(&chat, cx),
            "the read says the popover is open"
        );

        // The same chip closes it: the mouse-down that is outside the popover closes it, and the
        // click that follows must not open it again.
        click(cx, "background-tasks-chip");
        assert!(!is_drawn(cx, "background-tasks-popover"));
        assert!(!popover_reported(&chat, cx));

        // A click outside closes it too.
        click(cx, "background-tasks-chip");
        assert!(is_drawn(cx, "background-tasks-popover"));
        let root = drawn(cx, "chat-root");
        cx.simulate_click(
            root.origin + gpui::point(px(40.0), px(40.0)),
            Modifiers::none(),
        );
        cx.run_until_parked();
        settle(cx);
        assert!(!is_drawn(cx, "background-tasks-popover"));
    }

    #[gpui::test]
    async fn escape_closes_the_popover_and_keeps_the_turn(cx: &mut TestAppContext) {
        let (_, chat, _, cx) = setup(cx);
        start_tasks(&chat, cx);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            cx.notify();
        });
        click(cx, "background-tasks-chip");
        assert!(is_drawn(cx, "background-tasks-popover"));
        assert!(
            cx.update(|window, app| chat.read(app).background_popover_focus.is_focused(window)),
            "the popover has the keyboard, so Escape reaches it"
        );

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        settle(cx);
        assert!(
            !is_drawn(cx, "background-tasks-popover"),
            "Escape closes the popover"
        );
        assert!(
            chat.read_with(cx, |chat, _| chat.streaming),
            "and the turn it was pressed over runs on"
        );

        // With nothing open, Escape is the turn's own Stop again.
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(
            !chat.read_with(cx, |chat, _| chat.streaming),
            "the next Escape cancels the turn"
        );
    }

    #[gpui::test]
    async fn a_popover_the_row_no_longer_draws_is_closed_and_escape_cancels_the_turn(
        cx: &mut TestAppContext,
    ) {
        let (_, chat, _, cx) = setup_cached(cx);
        start_tasks(&chat, cx);
        chat.update_in(cx, |chat, window, cx| {
            chat.streaming = true;
            chat.toggle_background_popover(window, cx);
        });
        settle(cx);
        assert!(chat.read_with(cx, |chat, _| chat.background_popover_open));
        assert!(
            cx.update(|window, app| chat.read(app).background_popover_focus.is_focused(window)),
            "the popover holds the keyboard"
        );

        // The window narrows with no click in it: the Narrow row has no chip and no popover.
        // One step, as a window manager's snap or a maximize toggle makes it: the chat renders
        // once at the new width and nothing renders it again.
        cx.simulate_resize(size(px(160.0), px(800.0)));
        settle_quiet(cx);
        assert!(
            chat.read_with(cx, |chat, _| chat.composer_row.layout.get()) >= RowLayout::Narrow,
            "the window is narrow enough for the Narrow row"
        );
        assert!(
            !chat.read_with(cx, |chat, _| chat.background_popover_open),
            "the flag does not outlive the drawing, with no later render to close it"
        );
        assert!(
            composer_is_focused(&chat, cx),
            "the keyboard is the composer's again"
        );

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(
            !chat.read_with(cx, |chat, _| chat.streaming),
            "with no popover drawn, Escape cancels the turn"
        );

        // Widening brings nothing back unbidden.
        cx.simulate_resize(size(px(1920.0), px(1000.0)));
        settle(cx);
        // The cached host drops a reused view's debug bounds, so `is_drawn` would prove nothing.
        assert!(
            !popover_reported(&chat, cx),
            "widening brings nothing back unbidden"
        );
    }

    #[gpui::test]
    async fn the_popover_closes_when_no_task_is_left(cx: &mut TestAppContext) {
        let (_, chat, _, cx) = setup(cx);
        start_tasks(&chat, cx);
        click(cx, "background-tasks-chip");
        assert!(is_drawn(cx, "background-tasks-popover"));

        assert!(
            cx.update(|window, app| chat.read(app).background_popover_focus.is_focused(window)),
            "the popover holds the keyboard"
        );

        // The CLI's list drains: no task is left to list.
        feed(&chat, cx, BackgroundTaskEvent::Rows(Vec::new()));
        assert!(
            composer_is_focused(&chat, cx),
            "the keyboard went back to the composer, not to a view that is not drawn"
        );
        assert!(
            !is_drawn(cx, "background-tasks-popover"),
            "nothing is left to list"
        );
        assert!(
            !popover_reported(&chat, cx),
            "and the read does not say it is open"
        );

        // The chip is drawn from the CLI's count, so it can show with no task
        // running, and then it opens nothing. When the connection that gave
        // the count ends, the chip goes with it.
        chat.update(cx, |chat, cx| {
            chat.background_tasks = 2;
            cx.notify();
        });
        settle(cx);
        assert!(is_drawn(cx, "background-tasks-chip"));
        click(cx, "background-tasks-chip");
        assert!(
            !is_drawn(cx, "background-tasks-popover"),
            "a chip with no running task opens nothing"
        );
        chat.update(cx, |chat, cx| chat.reset_background_tasks(cx));
        settle(cx);
        assert!(
            !is_drawn(cx, "background-tasks-chip"),
            "a reset leaves no chip count"
        );
        assert!(!popover_reported(&chat, cx));
    }

    #[gpui::test]
    async fn each_stop_button_stops_its_task(cx: &mut TestAppContext) {
        let (host, chat, fake, cx) = setup(cx);
        start_tasks(&chat, cx);
        open_card(&chat, cx);

        // The popover's.
        click(cx, "background-tasks-chip");
        click(cx, "background-task-stop-bshell001");
        assert_eq!(fake.stops(), ["bshell001"]);
        // Pressed, it reads `Stopping…`, is disabled, and a second press sends nothing.
        assert!(chat.read_with(cx, |chat, _| chat.background_entries()[0].stopping));
        click(cx, "background-task-stop-bshell001");
        assert_eq!(fake.stops(), ["bshell001"], "a second press sends nothing");
        click(cx, "background-tasks-chip");
        assert!(!is_drawn(cx, "background-tasks-popover"));

        // The card's group row.
        click(cx, "trace-background-stop-bshell002");
        assert_eq!(fake.stops(), ["bshell001", "bshell002"]);

        // The agent's lane: its task has no group row, the lane carries the Stop.
        assert!(!is_drawn(cx, "trace-background-aagent001"));
        click(cx, "trace-lane-stop-aagent001");
        assert_eq!(fake.stops(), ["bshell001", "bshell002", "aagent001"]);

        // The tab's strip. Every task was stopped by now but one: a task of its own.
        feed(
            &chat,
            cx,
            BackgroundTaskEvent::Rows(vec![
                row("bshell001", "local_bash", "first shell"),
                row("bshell002", "local_bash", "second shell"),
                row("aagent001", "local_agent", "the reviewer"),
                row("bshell003", "local_bash", "third shell"),
            ]),
        );
        host.update(cx, |host, cx| {
            host.show_tab = true;
            cx.notify();
        });
        settle(cx);
        click(cx, "trace-tab-stop-bshell003");
        assert_eq!(
            fake.stops(),
            ["bshell001", "bshell002", "aagent001", "bshell003"]
        );
    }

    #[gpui::test]
    async fn a_lane_stop_does_not_reveal(cx: &mut TestAppContext) {
        let (_, chat, fake, cx) = setup(cx);
        start_tasks(&chat, cx);
        let agent = chat.update(cx, |chat, cx| {
            push_answers(chat, 10);
            chat.push_entry(Entry::SubagentTask {
                id: "toolu_a1".into(),
                title: "Reviewer".into(),
                status: "Running".into(),
                parts: Vec::new(),
                expanded: false,
                by_parent: false,
                duration_ms: None,
            });
            let agent = chat.entries.len() - 1;
            push_answers(chat, 60);
            cx.notify();
            agent
        });
        open_card(&chat, cx);
        let before = top_entry(&chat, cx);
        assert!(
            before > agent,
            "the transcript's tail is far below the agent's row"
        );

        click(cx, "trace-lane-stop-aagent001");
        assert_eq!(fake.stops(), ["aagent001"]);
        assert_eq!(
            top_entry(&chat, cx),
            before,
            "Stop did not scroll to the agent's row"
        );

        // The control: the lane's own click is the one that reveals.
        click(cx, "trace-lane-toolu_a1");
        assert_eq!(
            top_entry(&chat, cx),
            agent,
            "the lane's click reveals its row"
        );
    }

    /// One tick of the tail poll, and the reads it started.
    fn tick(chat: &Entity<Chat>, cx: &mut VisualTestContext, fake: &Fake) -> usize {
        let before = fake.reads();
        chat.update(cx, |chat, cx| {
            let generation = chat.connection_generation;
            chat.background_tick(generation, cx);
        });
        cx.run_until_parked();
        fake.reads() - before
    }

    #[gpui::test]
    async fn a_hidden_chat_paints_no_tail(cx: &mut TestAppContext) {
        let (host, chat, fake, cx) = setup(cx);
        start_tasks(&chat, cx);
        // The card is folded and the column closed: nothing paints a tail.
        assert_eq!(
            tick(&chat, cx, &fake),
            0,
            "a chat showing no tail is not read"
        );

        open_card(&chat, cx);
        assert_eq!(
            tick(&chat, cx, &fake),
            2,
            "the control: the drawn, unfolded card reads both shells and no agent"
        );

        // The chat is not drawn (another chat is selected): no frame paints the card.
        host.update(cx, |host, cx| {
            host.show_chat = false;
            cx.notify();
        });
        settle(cx);

        // The answers to the last reads repainted the card a moment before it was hidden, and
        // that last frame is served by the next tick. After it nothing paints.
        tick(&chat, cx, &fake);
        settle(cx);
        // Reading the chat's state, which builds the card's rows too, is not a paint.
        chat.read_with(cx, |chat, _| chat.control_snapshot());
        assert_eq!(tick(&chat, cx, &fake), 0, "a hidden chat is not read");
        assert_eq!(tick(&chat, cx, &fake), 0);
    }

    #[gpui::test]
    async fn a_hidden_tab_paints_no_tail(cx: &mut TestAppContext) {
        let (host, chat, fake, cx) = setup(cx);
        start_tasks(&chat, cx);
        start_turn(&chat, cx);
        // The chat is hidden behind the tab: only the tab's strip can paint.
        host.update(cx, |host, cx| {
            host.show_chat = false;
            host.show_tab = true;
            cx.notify();
        });
        settle(cx);
        assert_eq!(
            tick(&chat, cx, &fake),
            2,
            "the control: the drawn strip reads both shells"
        );

        host.update(cx, |host, cx| {
            host.show_tab = false;
            cx.notify();
        });
        settle(cx);
        // The answers repainted the strip just before it was hidden: that frame is served by the
        // next tick, and after it nothing paints.
        tick(&chat, cx, &fake);
        settle(cx);
        assert_eq!(
            tick(&chat, cx, &fake),
            0,
            "a tab that is not drawn is not read"
        );
        assert_eq!(tick(&chat, cx, &fake), 0);
    }

    #[gpui::test]
    async fn reading_the_tabs_snapshot_paints_no_tail(cx: &mut TestAppContext) {
        let (host, chat, fake, cx) = setup(cx);
        start_tasks(&chat, cx);
        start_turn(&chat, cx);
        host.update(cx, |host, cx| {
            host.show_chat = false;
            cx.notify();
        });
        settle(cx);
        let tab = host.read_with(cx, |host, _| host.tab.clone());
        // The tab is not drawn, and `surface.trace.read` reads it.
        let strip = tab.read_with(cx, |tab, app| tab.snapshot(app).background);
        assert_eq!(strip.len(), 3, "the read does list the tasks");
        assert_eq!(
            tick(&chat, cx, &fake),
            0,
            "a read of the tab's state is not a frame"
        );
        tab.read_with(cx, |tab, app| tab.snapshot(app));
        assert_eq!(tick(&chat, cx, &fake), 0);
    }
}
