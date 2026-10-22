use super::rows::{bar_color, tooltip_lines};
use super::*;
use crate::chat::testing;
use crate::chat::trace::SpanState;
use gpui::{
    AppContext as _, Modifiers, ParentElement, Styled, TestAppContext, VisualTestContext, px,
    size,
};
use sirio_acp::{SpanOutcome, TokenCount, TraceEvent, TurnOutcome};
use std::cell::Cell;
use std::rc::Rc;

/// A chat that never connects, with the tab beside it: the chat is drawn
/// only when a test needs its transcript laid out, the tab only while
/// `show_tab`, which stands in for the tab being hidden.
struct Host {
    chat: Option<Entity<Chat>>,
    tab: Entity<TraceTab>,
    show_chat: bool,
    show_tab: bool,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .children(
                self.chat
                    .clone()
                    .filter(|_| self.show_chat)
                    .map(|chat| div().w(px(420.0)).h_full().child(chat)),
            )
            .children(
                self.show_tab
                    .then(|| div().flex_1().h_full().child(self.tab.clone())),
            )
    }
}

fn host(cx: &mut TestAppContext, show_chat: bool) -> (Entity<Host>, &mut VisualTestContext) {
    testing::prepare(cx);
    cx.update(init);
    let (host, cx) = cx.add_window_view(move |_, cx| {
        let chat = cx.new(testing::offline_chat);
        let tab = cx.new(|cx| TraceTab::new(chat.clone(), cx));
        Host {
            chat: Some(chat),
            tab,
            show_chat,
            show_tab: true,
        }
    });
    cx.simulate_resize(size(px(1100.0), px(500.0)));
    (host, cx)
}

fn chat_of(host: &Entity<Host>, cx: &mut VisualTestContext) -> Entity<Chat> {
    host.read_with(cx, |host, _| host.chat.clone().expect("the chat is alive"))
}

fn tab_of(host: &Entity<Host>, cx: &mut VisualTestContext) -> Entity<TraceTab> {
    host.read_with(cx, |host, _| host.tab.clone())
}

fn feed(host: &Entity<Host>, cx: &mut VisualTestContext, event: TraceEvent) {
    let chat = chat_of(host, cx);
    chat.update(cx, |chat, cx| testing::feed(chat, event, cx));
}

/// The turn shown, the latest, and whether the tab follows it.
fn shown(host: &Entity<Host>, cx: &mut VisualTestContext) -> (usize, usize, bool) {
    let tab = tab_of(host, cx);
    let snapshot = tab.read_with(cx, |tab, cx| tab.snapshot(cx));
    (snapshot.turn, snapshot.turns, snapshot.follow)
}

fn span_ids(host: &Entity<Host>, cx: &mut VisualTestContext) -> Vec<String> {
    let tab = tab_of(host, cx);
    let snapshot = tab.read_with(cx, |tab, cx| tab.snapshot(cx));
    snapshot.spans.iter().map(|span| span["id"].clone()).collect()
}

fn refresh(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }
}

/// A real click at the middle of a drawn element.
fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    refresh(cx);
}

fn selector(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

fn focus_tab(host: &Entity<Host>, cx: &mut VisualTestContext) {
    let tab = tab_of(host, cx);
    cx.update(|window, cx| {
        let focus = tab.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    });
    cx.run_until_parked();
}

fn secs(base: Instant, seconds: f64) -> Instant {
    base + Duration::from_secs_f64(seconds)
}

fn started(at: Instant) -> TraceEvent {
    TraceEvent::TurnStarted { at }
}

fn finished(at: Instant) -> TraceEvent {
    TraceEvent::TurnEnded {
        at,
        outcome: TurnOutcome::Done,
        measured_ms: None,
        cost_usd: None,
        tokens: None,
    }
}

fn span(id: &str, parent: Option<&str>, kind: SpanKind, name: &str, at: Instant) -> TraceEvent {
    TraceEvent::SpanStarted {
        id: id.into(),
        parent: parent.map(str::to_string),
        kind,
        name: name.into(),
        detail: String::new(),
        model: None,
        at,
    }
}

fn span_ended(id: &str, outcome: SpanOutcome, at: Instant) -> TraceEvent {
    TraceEvent::SpanEnded {
        id: id.into(),
        outcome,
        at,
        measured_ms: None,
    }
}

fn done(id: &str, at: Instant) -> TraceEvent {
    span_ended(id, SpanOutcome::Done, at)
}

/// The turn the key and click tests read: a model call, a subagent that
/// read a file, and an edit, all over.
fn an_audit(host: &Entity<Host>, cx: &mut VisualTestContext, base: Instant) -> usize {
    let chat = chat_of(host, cx);
    chat.update(cx, |chat, cx| {
        testing::submit(chat, "audit the crate", cx);
        let events = [
            started(base),
            span("msg_1", None, SpanKind::Llm, "claude", secs(base, 0.5)),
            done("msg_1", secs(base, 2.0)),
            span("toolu_agent", None, SpanKind::Subagent, "Explore", secs(base, 2.0)),
            span("toolu_read", Some("toolu_agent"), SpanKind::Tool, "Read", secs(base, 3.0)),
            done("toolu_read", secs(base, 5.0)),
            done("toolu_agent", secs(base, 8.0)),
            span("toolu_edit", None, SpanKind::Tool, "Edit", secs(base, 8.0)),
            done("toolu_edit", secs(base, 10.0)),
            finished(secs(base, 12.0)),
        ];
        for event in events {
            testing::feed(chat, event, cx);
        }
        testing::answers(chat, 10);
        let agent = testing::agent_row(chat, "toolu_agent", "Explore");
        testing::answers(chat, 40);
        cx.notify();
        agent
    })
}

fn top_entry(host: &Entity<Host>, cx: &mut VisualTestContext) -> usize {
    let chat = chat_of(host, cx);
    chat.read_with(cx, |chat, _| chat.control_snapshot().first_visible_entry)
}

#[gpui::test]
async fn the_tab_follows_the_latest_turn_until_you_go_back(cx: &mut TestAppContext) {
    let (host, cx) = host(cx, false);
    let base = Instant::now() - Duration::from_secs(100);
    for turn in 0..2 {
        let at = secs(base, f64::from(turn) * 10.0);
        feed(&host, cx, started(at));
        feed(&host, cx, finished(secs(at, 4.0)));
    }
    feed(&host, cx, started(secs(base, 20.0)));
    refresh(cx);
    assert_eq!(shown(&host, cx), (3, 3, true), "a tab on a chat's third turn shows it");

    click(cx, "trace-tab-previous");
    assert_eq!(shown(&host, cx), (2, 3, false), "going back turns following off");

    feed(&host, cx, finished(secs(base, 24.0)));
    feed(&host, cx, started(secs(base, 30.0)));
    refresh(cx);
    assert_eq!(
        shown(&host, cx),
        (2, 4, false),
        "a turn that opens while an old one is read leaves the reader there"
    );

    click(cx, "trace-tab-follow");
    assert_eq!(shown(&host, cx), (4, 4, true), "following again jumps to the latest");
}

#[gpui::test]
async fn the_oldest_turn_the_chat_keeps_is_as_far_back_as_the_tab_goes(
    cx: &mut TestAppContext,
) {
    let (host, cx) = host(cx, false);
    let base = Instant::now() - Duration::from_secs(1000);
    let turn = |n: u32| secs(base, f64::from(n) * 10.0);
    for n in 0..22 {
        feed(&host, cx, started(turn(n)));
        feed(&host, cx, finished(secs(turn(n), 4.0)));
    }
    refresh(cx);
    // The chat keeps the last twenty of its twenty-two turns.
    assert_eq!(shown(&host, cx), (22, 22, true));
    let tab = tab_of(&host, cx);
    for _ in 0..30 {
        tab.update(cx, |tab, cx| tab.step_back(cx));
    }
    assert_eq!(shown(&host, cx), (3, 22, false), "turns 1 and 2 are gone, and so is the way to them");

    // A new turn pushes the one the reader is on out of the chat: the
    // oldest that is left is shown.
    feed(&host, cx, started(turn(22)));
    refresh(cx);
    assert_eq!(shown(&host, cx), (4, 23, false));
}

#[gpui::test]
async fn a_trace_that_begins_again_turns_following_back_on(cx: &mut TestAppContext) {
    let (host, cx) = host(cx, false);
    cx.cx.executor().allow_parking();
    let base = Instant::now() - Duration::from_secs(100);
    for n in 0..3 {
        feed(&host, cx, started(secs(base, f64::from(n) * 10.0)));
        feed(&host, cx, finished(secs(base, f64::from(n) * 10.0 + 4.0)));
    }
    click(cx, "trace-tab-previous");
    assert_eq!(shown(&host, cx), (2, 3, false));

    // A new conversation drops the agent that ran these turns, and its
    // trace with it.
    let chat = chat_of(&host, cx);
    chat.update(cx, |chat, cx| testing::new_conversation(chat, cx));
    refresh(cx);
    assert_eq!(shown(&host, cx), (0, 0, true), "nothing to show, and following again");

    feed(&host, cx, started(secs(base, 60.0)));
    refresh(cx);
    assert_eq!(
        shown(&host, cx),
        (1, 1, true),
        "the first turn of the new trace is followed, not left behind at turn 2"
    );
}

#[gpui::test]
async fn the_keys_move_fold_and_reveal(cx: &mut TestAppContext) {
    let (host, cx) = host(cx, true);
    cx.simulate_resize(size(px(1100.0), px(300.0)));
    let base = Instant::now() - Duration::from_secs(60);
    let agent = an_audit(&host, cx, base);
    refresh(cx);
    assert!(top_entry(&host, cx) > agent, "the transcript's tail is far below the agent's row");
    focus_tab(&host, cx);

    // No selection yet: the first Down takes the first row.
    cx.simulate_keystrokes("down down down");
    refresh(cx);
    assert_eq!(
        tab_of(&host, cx).read_with(cx, |tab, _| tab.selected.clone()).as_deref(),
        Some("toolu_agent"),
        "root, the model call, then the subagent"
    );
    assert!(
        top_entry(&host, cx) > agent,
        "moving the selection does not scroll the transcript"
    );

    cx.simulate_keystrokes("left");
    refresh(cx);
    assert_eq!(
        span_ids(&host, cx),
        ["root", "msg_1", "toolu_agent", "toolu_edit"],
        "Left folds the subagent: the read inside it is gone"
    );
    let bar = cx.debug_bounds("trace-bar-toolu_agent").expect("the folded parent's bar is drawn");
    let mark = cx
        .debug_bounds("trace-mark-toolu_agent-0")
        .expect("the folded parent shows where its hidden child began");
    assert!(
        mark.left() >= bar.left() && mark.right() <= bar.right(),
        "the mark {mark:?} sits on the parent's bar {bar:?}"
    );

    cx.simulate_keystrokes("right");
    refresh(cx);
    assert_eq!(
        span_ids(&host, cx),
        ["root", "msg_1", "toolu_agent", "toolu_read", "toolu_edit"],
        "Right unfolds it"
    );
    assert!(cx.debug_bounds("trace-mark-toolu_agent-0").is_none(), "an open parent shows no marks");

    cx.simulate_keystrokes("down");
    refresh(cx);
    assert!(
        top_entry(&host, cx) > agent,
        "Down onto the read did not reveal it either"
    );
    cx.simulate_keystrokes("enter");
    refresh(cx);
    assert_eq!(
        top_entry(&host, cx),
        agent,
        "Enter reveals the Agent row a span inside a subagent belongs to"
    );
}

#[gpui::test]
async fn a_click_on_a_row_selects_it_and_reveals_it(cx: &mut TestAppContext) {
    let (host, cx) = host(cx, true);
    cx.simulate_resize(size(px(1100.0), px(300.0)));
    let agent = an_audit(&host, cx, Instant::now() - Duration::from_secs(60));
    refresh(cx);
    assert!(top_entry(&host, cx) > agent);

    click(cx, "trace-row-toolu_agent");
    assert_eq!(
        tab_of(&host, cx).read_with(cx, |tab, _| tab.selected.clone()).as_deref(),
        Some("toolu_agent")
    );
    assert_eq!(top_entry(&host, cx), agent, "a row reveals what a lane would");
}

#[gpui::test]
async fn a_click_on_a_chevron_folds_without_selecting_or_revealing(cx: &mut TestAppContext) {
    let (host, cx) = host(cx, true);
    cx.simulate_resize(size(px(1100.0), px(300.0)));
    let agent = an_audit(&host, cx, Instant::now() - Duration::from_secs(60));
    refresh(cx);
    assert!(top_entry(&host, cx) > agent);

    click(cx, "trace-chevron-toolu_agent");
    assert_eq!(
        span_ids(&host, cx),
        ["root", "msg_1", "toolu_agent", "toolu_edit"],
        "the chevron folds the subagent"
    );
    assert_eq!(
        tab_of(&host, cx).read_with(cx, |tab, _| tab.selected.clone()),
        None,
        "the row's own click did not also select it"
    );
    assert!(top_entry(&host, cx) > agent, "nor reveal it");

    click(cx, "trace-chevron-toolu_agent");
    assert_eq!(span_ids(&host, cx).len(), 5, "the chevron opens it again");
}

fn row(span: &str, kind: Option<SpanKind>, state: SpanState, start: f64, end: Option<f64>) -> TreeRow {
    TreeRow {
        span: span.into(),
        depth: 1,
        kind,
        name: span.into(),
        detail: String::new(),
        state,
        start: Duration::from_secs_f64(start),
        end: end.map(Duration::from_secs_f64),
        hidden_children: 0,
        child_marks: Vec::new(),
        note: None,
        model: None,
        tokens: None,
        progress_tokens: None,
        tool_uses: None,
        waits: Vec::new(),
    }
}

#[test]
fn the_tooltip_names_kind_name_start_duration_model_tokens_and_status() {
    let lines = |row: &TreeRow| -> Vec<(&'static str, String)> {
        tooltip_lines(row, Duration::from_secs(30))
    };
    let pair = |label: &'static str, value: &str| (label, value.to_string());

    let mut edit = row("Edit", Some(SpanKind::Tool), SpanState::Done, 2.5, Some(6.6));
    edit.name = "Edit".into();
    assert_eq!(
        lines(&edit),
        vec![
            pair("Kind", "Tool"),
            pair("Name", "Edit"),
            pair("Start", "+2.5s"),
            pair("Duration", "4.1s"),
            pair("Status", "Done"),
        ]
    );

    let mut call = row("msg_1", Some(SpanKind::Llm), SpanState::Done, 0.5, Some(0.84));
    call.model = Some("opus".into());
    call.tokens = Some(TokenCount { input: 1_240, output: 340 });
    assert_eq!(
        lines(&call),
        vec![
            pair("Kind", "Model call"),
            pair("Name", "msg_1"),
            pair("Start", "+500ms"),
            pair("Duration", "340ms"),
            pair("Model", "opus"),
            pair("Tokens", "1.2k in · 340 out"),
            pair("Status", "Done"),
        ],
        "a call says its model and what it was given and wrote"
    );

    let mut failed = row("toolu_bash", Some(SpanKind::Tool), SpanState::Failed, 70.0, Some(71.0));
    failed.note = Some("exit status 1".into());
    let failed = lines(&failed);
    assert_eq!(&failed[2..], &[pair("Start", "+1m 10s"), pair("Duration", "1.0s"), pair("Status", "Failed"), pair("Error", "exit status 1")]);

    // A span that is still running has taken as long as the turn has gone
    // on since it began; a subagent says what it has used so far.
    let mut agent = row("toolu_agent", Some(SpanKind::Subagent), SpanState::Running, 10.0, None);
    agent.progress_tokens = Some(48_200);
    let agent = lines(&agent);
    assert_eq!(agent[0], pair("Kind", "Subagent"));
    assert_eq!(agent[3], pair("Duration", "20.0s"));
    assert_eq!(agent[4], pair("Tokens", "48.2k so far"));
    assert_eq!(agent[5], pair("Status", "Running"));

    let root = lines(&row("root", None, SpanState::Done, 0.0, Some(12.0)));
    assert_eq!(root[0], pair("Kind", "Turn"));
}

#[gpui::test]
async fn the_spans_say_who_their_parent_is_and_whether_they_ended(cx: &mut TestAppContext) {
    let (host, cx) = host(cx, false);
    let base = Instant::now() - Duration::from_secs(60);
    for event in [
        started(base),
        span("msg_1", None, SpanKind::Llm, "claude", secs(base, 0.5)),
        done("msg_1", secs(base, 2.0)),
        span("toolu_agent", None, SpanKind::Subagent, "Explore", secs(base, 2.0)),
        span("toolu_read", Some("toolu_agent"), SpanKind::Tool, "Read", secs(base, 3.0)),
        done("toolu_read", secs(base, 5.0)),
        done("toolu_agent", secs(base, 8.0)),
        span("toolu_edit", None, SpanKind::Tool, "Edit", secs(base, 8.0)),
    ] {
        feed(&host, cx, event);
    }
    let tab = tab_of(&host, cx);
    let spans = tab.read_with(cx, |tab, cx| tab.snapshot(cx)).spans;
    let row = |id: &str, parent: &str, kind: &str, state: &str, ended: &str, depth: &str| {
        BTreeMap::from(
            [
                ("id", id),
                ("parent", parent),
                ("kind", kind),
                ("state", state),
                ("ended", ended),
                ("depth", depth),
                ("folded", "0"),
            ]
            .map(|(key, value)| (key.to_string(), value.to_string())),
        )
    };
    assert_eq!(
        spans,
        vec![
            row("root", "", "root", "running", "0", "0"),
            row("msg_1", "root", "llm", "done", "1", "1"),
            row("toolu_agent", "root", "subagent", "done", "1", "1"),
            row("toolu_read", "toolu_agent", "tool", "done", "1", "2"),
            row("toolu_edit", "root", "tool", "running", "0", "1"),
        ]
    );
}

#[gpui::test]
async fn the_tab_moves_only_while_its_turn_runs(cx: &mut TestAppContext) {
    let (host, cx) = host(cx, false);
    let frames = |host: &Entity<Host>, cx: &mut VisualTestContext| {
        tab_of(host, cx).read_with(cx, |tab, _| tab.frames.get())
    };
    let next_frame = |cx: &mut VisualTestContext| {
        cx.cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        refresh(cx);
    };
    let base = Instant::now() - Duration::from_secs(5);
    feed(&host, cx, started(base));
    feed(&host, cx, span("toolu_run", None, SpanKind::Tool, "Bash", secs(base, 1.0)));
    refresh(cx);
    let first = frames(&host, cx);
    assert!(first > 0, "a drawn tab of a running turn animates");
    next_frame(cx);
    assert!(frames(&host, cx) > first, "and goes on animating");

    // The tab goes out of sight: nothing prepaints, so nothing asks.
    host.update(cx, |host, cx| {
        host.show_tab = false;
        cx.notify();
    });
    refresh(cx);
    let hidden = frames(&host, cx);
    for _ in 0..3 {
        next_frame(cx);
    }
    assert_eq!(frames(&host, cx), hidden, "a tab nobody draws asks for no frame");

    host.update(cx, |host, cx| {
        host.show_tab = true;
        cx.notify();
    });
    refresh(cx);
    next_frame(cx);
    assert!(frames(&host, cx) > hidden, "drawn again, it animates again");

    feed(&host, cx, done("toolu_run", Instant::now()));
    feed(&host, cx, finished(Instant::now()));
    refresh(cx);
    let finished = frames(&host, cx);
    for _ in 0..3 {
        next_frame(cx);
    }
    assert_eq!(frames(&host, cx), finished, "a finished turn requests nothing");
}

#[gpui::test]
async fn the_tab_follows_its_shown_turn_not_the_latest_for_its_frames(
    cx: &mut TestAppContext,
) {
    let (host, cx) = host(cx, false);
    let base = Instant::now() - Duration::from_secs(30);
    feed(&host, cx, started(base));
    feed(&host, cx, finished(secs(base, 4.0)));
    feed(&host, cx, started(secs(base, 10.0)));
    feed(&host, cx, span("toolu_run", None, SpanKind::Tool, "Bash", secs(base, 11.0)));
    refresh(cx);
    click(cx, "trace-tab-previous");
    assert_eq!(shown(&host, cx), (1, 2, false));
    let before = tab_of(&host, cx).read_with(cx, |tab, _| tab.frames.get());
    for _ in 0..3 {
        cx.cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        refresh(cx);
    }
    assert_eq!(
        tab_of(&host, cx).read_with(cx, |tab, _| tab.frames.get()),
        before,
        "the turn on screen is over, though the chat's latest runs"
    );
}

#[gpui::test]
async fn the_tab_asks_for_no_frame_but_its_own(cx: &mut TestAppContext) {
    let (host, cx) = host(cx, false);
    let base = Instant::now() - Duration::from_secs(5);
    feed(&host, cx, started(base));
    feed(&host, cx, span("toolu_run", None, SpanKind::Tool, "Bash", secs(base, 1.0)));
    refresh(cx);
    // The test window has no frame loop: the callbacks a draw asks the
    // next frame for wait until they are delivered by hand.
    cx.update(|window, cx| window.simulate_next_frame(cx));
    feed(
        &host,
        cx,
        span("toolu_agent", None, SpanKind::Subagent, "Explore", Instant::now()),
    );
    for _ in 0..6 {
        refresh(cx);
    }
    let asked = cx.update(|window, cx| window.simulate_next_frame(cx));
    assert_eq!(
        asked, 0,
        "every frame the tab causes comes from its own capped loop"
    );
}

#[gpui::test]
async fn a_tab_asks_to_close_when_its_chat_goes(cx: &mut TestAppContext) {
    let (host, cx) = host(cx, false);
    let closed = Rc::new(Cell::new(0));
    let seen = closed.clone();
    let tab = tab_of(&host, cx);
    cx.update(|_, cx| {
        cx.subscribe(&tab, move |_, event: &TraceTabEvent, _| {
            if matches!(event, TraceTabEvent::Close) {
                seen.set(seen.get() + 1);
            }
        })
        .detach();
    });
    refresh(cx);
    assert_eq!(closed.get(), 0);
    host.update(cx, |host, _| host.chat = None);
    cx.run_until_parked();
    assert_eq!(closed.get(), 1, "a tab on a chat that is gone has nothing to show");
}

#[gpui::test]
async fn a_narrow_tab_stacks_each_bar_under_its_name(cx: &mut TestAppContext) {
    let (host, cx) = host(cx, false);
    let base = Instant::now() - Duration::from_secs(10);
    feed(&host, cx, started(base));
    feed(&host, cx, span("toolu_run", None, SpanKind::Tool, "Bash", secs(base, 1.0)));
    feed(&host, cx, done("toolu_run", secs(base, 4.0)));
    feed(&host, cx, finished(secs(base, 5.0)));

    cx.simulate_resize(size(px(900.0), px(400.0)));
    refresh(cx);
    let name = cx.debug_bounds("trace-row-name-toolu_run").expect("the name is drawn");
    let track = cx.debug_bounds("trace-row-track-toolu_run").expect("the bar's track is drawn");
    assert!(
        track.left() >= name.right(),
        "wide: the bars sit beside the names, {track:?} beside {name:?}"
    );

    cx.simulate_resize(size(px(500.0), px(400.0)));
    refresh(cx);
    let name = cx.debug_bounds("trace-row-name-toolu_run").expect("the name is drawn");
    let track = cx.debug_bounds("trace-row-track-toolu_run").expect("the bar's track is drawn");
    assert!(
        track.top() >= name.bottom() && track.left() <= name.left() + px(40.0),
        "narrow: the bar is under its name, {track:?} under {name:?}"
    );
}

#[gpui::test]
async fn the_slowest_leaf_wears_a_tag_a_failure_its_error_and_an_interrupted_bar_an_open_end(
    cx: &mut TestAppContext,
) {
    let (host, cx) = host(cx, false);
    let base = Instant::now() - Duration::from_secs(60);
    for event in [
        started(base),
        span("toolu_slow", None, SpanKind::Tool, "Bash", secs(base, 1.0)),
        done("toolu_slow", secs(base, 9.0)),
        span("toolu_fail", None, SpanKind::Tool, "Bash", secs(base, 9.0)),
        span_ended(
            "toolu_fail",
            SpanOutcome::Failed { note: "exit status 1".into() },
            secs(base, 10.0),
        ),
        span("toolu_hang", None, SpanKind::Tool, "Read", secs(base, 10.0)),
        finished(secs(base, 11.0)),
    ] {
        feed(&host, cx, event);
    }
    refresh(cx);
    let drawn = |cx: &mut VisualTestContext, id: &str| cx.debug_bounds(selector(id.to_string())).is_some();
    assert!(drawn(cx, "trace-slowest-toolu_slow"), "the slowest leaf is tagged");
    assert!(!drawn(cx, "trace-slowest-toolu_fail"), "and only that one");
    assert!(drawn(cx, "trace-note-toolu_fail"), "a failed bar carries its error");
    assert!(!drawn(cx, "trace-note-toolu_slow"));
    assert!(drawn(cx, "trace-open-end-toolu_hang"), "an interrupted bar has an open end");
    assert!(!drawn(cx, "trace-open-end-toolu_slow"), "a finished one is closed");
}

#[gpui::test]
async fn every_bar_shows_its_duration_inside_its_track(cx: &mut TestAppContext) {
    let (host, cx) = host(cx, false);
    let base = Instant::now() - Duration::from_secs(60);
    for event in [
        started(base),
        span("toolu_early", None, SpanKind::Tool, "Read", secs(base, 0.5)),
        done("toolu_early", secs(base, 1.5)),
        span("toolu_late", None, SpanKind::Tool, "Bash", secs(base, 8.5)),
        done("toolu_late", secs(base, 9.5)),
        finished(secs(base, 10.0)),
    ] {
        feed(&host, cx, event);
    }
    for width in [1100.0, 500.0] {
        cx.simulate_resize(size(px(width), px(400.0)));
        refresh(cx);
        for id in ["root", "toolu_early", "toolu_late"] {
            let track = cx
                .debug_bounds(selector(format!("trace-row-track-{id}")))
                .unwrap_or_else(|| panic!("the track of {id} is drawn"));
            let label = cx
                .debug_bounds(selector(format!("trace-label-{id}")))
                .unwrap_or_else(|| panic!("the duration of {id} is drawn"));
            assert!(
                label.left() >= track.left() && label.right() <= track.right(),
                "at {width}px the duration of {id} {label:?} leaves its track {track:?}"
            );
        }
        // The root's bar covers the whole axis: neither side of it has room,
        // so its duration is written on the bar.
        let root_bar = cx.debug_bounds("trace-bar-root").expect("the root's bar is drawn");
        let root_label = cx.debug_bounds("trace-label-root").expect("its duration is drawn");
        assert!(
            root_label.left() >= root_bar.left() && root_label.right() <= root_bar.right(),
            "at {width}px the root's duration {root_label:?} is on its bar {root_bar:?}"
        );
        // A short bar early on has its duration after it.
        let bar = cx.debug_bounds("trace-bar-toolu_early").expect("the bar is drawn");
        let label = cx.debug_bounds("trace-label-toolu_early").expect("its duration is drawn");
        assert!(label.left() >= bar.right(), "{label:?} follows {bar:?}");
    }
}

#[gpui::test]
async fn an_overflowing_tab_draws_a_scrollbar_and_asks_for_no_frame_while_idle(
    cx: &mut TestAppContext,
) {
    let (host, cx) = host(cx, false);
    let base = Instant::now() - Duration::from_secs(120);
    feed(&host, cx, started(base));
    for call in 0..60 {
        let id = format!("toolu_{call}");
        let at = f64::from(call);
        feed(&host, cx, span(&id, None, SpanKind::Tool, "Bash", secs(base, at)));
        feed(&host, cx, done(&id, secs(base, at + 0.5)));
    }
    feed(&host, cx, finished(secs(base, 61.0)));
    cx.simulate_resize(size(px(1100.0), px(300.0)));
    refresh(cx);
    assert!(
        cx.debug_bounds("trace-tab-bar").is_some(),
        "sixty rows do not fit in 300 px: the list has a scrollbar"
    );
    // The bar may ask for a frame while it settles; once it has, an idle tab
    // asks for none.
    for _ in 0..3 {
        cx.update(|window, cx| window.simulate_next_frame(cx));
        refresh(cx);
    }
    let asked = cx.update(|window, cx| window.simulate_next_frame(cx));
    assert_eq!(asked, 0, "a settled, finished tab asks for no frame of its own");
}

#[test]
fn a_running_agent_is_drawn_in_the_running_colour_and_a_finished_one_in_grey() {
    let theme = sirio_theme::Theme::dark();
    let ely = &theme.ely;
    let color = |kind: Option<SpanKind>, state: SpanState, end: Option<f64>| {
        bar_color(&theme, &row("span", kind, state, 0.0, end))
    };
    let running = ely.info.opacity(0.55);
    assert_eq!(color(None, SpanState::Running, None), running, "the root while its turn runs");
    assert_eq!(
        color(Some(SpanKind::Subagent), SpanState::Running, None),
        running,
        "a subagent that runs"
    );
    assert_eq!(
        color(Some(SpanKind::Subagent), SpanState::Waiting, None),
        running,
        "one that waits on a permission is still running"
    );
    assert_eq!(color(None, SpanState::Done, Some(12.0)), ely.border_strong, "the root once it is over");
    assert_eq!(
        color(Some(SpanKind::Subagent), SpanState::Done, Some(8.0)),
        ely.fg_subtle.opacity(0.7),
        "a subagent that finished goes back to the grey ladder"
    );
    assert_eq!(
        color(Some(SpanKind::Subagent), SpanState::Failed, Some(8.0)),
        ely.danger,
        "a failed one keeps the danger colour"
    );
    // Calls keep their kind: their open end and the "now" line say they run.
    assert_eq!(color(Some(SpanKind::Tool), SpanState::Running, None), ely.chart[1]);
    assert_eq!(color(Some(SpanKind::Llm), SpanState::Running, None), ely.chart[4]);
}

#[gpui::test]
async fn folding_away_the_selected_row_selects_the_row_that_folded(cx: &mut TestAppContext) {
    let (host, cx) = host(cx, true);
    cx.simulate_resize(size(px(1100.0), px(300.0)));
    an_audit(&host, cx, Instant::now() - Duration::from_secs(60));
    refresh(cx);
    focus_tab(&host, cx);
    cx.simulate_keystrokes("down down down down");
    let selected = |host: &Entity<Host>, cx: &mut VisualTestContext| {
        tab_of(host, cx).read_with(cx, |tab, _| tab.selected.clone())
    };
    assert_eq!(selected(&host, cx).as_deref(), Some("toolu_read"), "root, call, agent, its read");

    click(cx, "trace-chevron-toolu_agent");
    assert_eq!(span_ids(&host, cx), ["root", "msg_1", "toolu_agent", "toolu_edit"]);
    assert_eq!(
        selected(&host, cx).as_deref(),
        Some("toolu_agent"),
        "the read is folded away: the selection stays on screen, on what hid it"
    );
    focus_tab(&host, cx);
    cx.simulate_keystrokes("down");
    assert_eq!(
        selected(&host, cx).as_deref(),
        Some("toolu_edit"),
        "Down goes on from the folded row"
    );
}

#[gpui::test]
async fn the_folds_and_the_selection_belong_to_their_turn(cx: &mut TestAppContext) {
    let (host, cx) = host(cx, false);
    let base = Instant::now() - Duration::from_secs(100);
    // Two turns that name the same spans.
    for turn in 0..2 {
        let at = secs(base, f64::from(turn) * 30.0);
        for event in [
            started(at),
            span("toolu_agent", None, SpanKind::Subagent, "Explore", secs(at, 1.0)),
            span("toolu_read", Some("toolu_agent"), SpanKind::Tool, "Read", secs(at, 2.0)),
            done("toolu_read", secs(at, 3.0)),
            done("toolu_agent", secs(at, 4.0)),
            finished(secs(at, 5.0)),
        ] {
            feed(&host, cx, event);
        }
    }
    refresh(cx);
    let tab = tab_of(&host, cx);
    click(cx, "trace-row-toolu_agent");
    click(cx, "trace-chevron-toolu_agent");
    assert_eq!(span_ids(&host, cx), ["root", "toolu_agent"], "turn 2: the agent is folded");
    assert_eq!(
        tab.read_with(cx, |tab, _| tab.selected.clone()).as_deref(),
        Some("toolu_agent")
    );

    click(cx, "trace-tab-previous");
    assert_eq!(shown(&host, cx), (1, 2, false));
    assert_eq!(
        span_ids(&host, cx),
        ["root", "toolu_agent", "toolu_read"],
        "turn 1 has nothing folded, though turn 2's agent had the same id"
    );
    assert_eq!(tab.read_with(cx, |tab, _| tab.selected.clone()), None);

    // And the other way: what was done in turn 1 does not follow to turn 2.
    click(cx, "trace-chevron-toolu_agent");
    tab.update(cx, |tab, cx| tab.step_forward(cx));
    refresh(cx);
    assert_eq!(shown(&host, cx), (2, 2, false));
    assert_eq!(span_ids(&host, cx), ["root", "toolu_agent", "toolu_read"]);
}

