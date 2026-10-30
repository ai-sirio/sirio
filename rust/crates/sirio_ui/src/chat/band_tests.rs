//! The composer as a band under the chat: it spans the chat column, keeps its
//! content on the transcript's reading column, and holds every control on one
//! line.
use super::tests::{
    chat_view, configure_test_chat, focus_and_type, pump_chat_until, refresh_frame,
};
use super::*;
use gpui::{Bounds, TestAppContext, VisualTestContext, size};
use sirio_acp::EffortChoice;

/// What the composer shows with a full complement of controls: Claude's five
/// permission modes (current `acceptEdits`), Fast, Thinking, two background
/// tasks and an effort catalogue of `low`/`medium`/`high` (current `high`),
/// on top of what [`configure_test_chat`] gives every chat.
pub(super) fn configure_full_row(chat: &mut Chat) {
    configure_test_chat(chat);
    let mode = |id: &str, name: &str| AgentMode {
        id: id.into(),
        name: name.into(),
        description: None,
    };
    chat.mode_catalog = Some(ModeCatalog {
        current_id: "acceptEdits".into(),
        options: vec![
            mode("default", "Ask"),
            mode("acceptEdits", "Accept edits"),
            mode("plan", "Plan"),
            mode("auto", "Auto"),
            mode("bypassPermissions", "Bypass permissions"),
        ],
        config_option_id: None,
    });
    chat.fast_mode = Some(FastMode {
        enabled: false,
        blocked_by: None,
    });
    chat.thinking_display = Some(ThinkingDisplay {
        chosen: Some("summarized".into()),
        unsupported: false,
    });
    chat.background_tasks = 2;
    chat.effort = Some(EffortOption {
        option_id: "effort".into(),
        name: Some("Effort".into()),
        current_value: Some("high".into()),
        choices: ["low", "medium", "high"]
            .into_iter()
            .map(|level| EffortChoice {
                value: level.into(),
                name: level[..1].to_uppercase() + &level[1..],
            })
            .collect(),
    });
}

fn drawn(cx: &mut VisualTestContext, name: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(name)
        .unwrap_or_else(|| panic!("{name} is drawn in the row"))
}

fn near(a: Pixels, b: Pixels, tolerance: f32) -> bool {
    (f32::from(a) - f32::from(b)).abs() <= tolerance
}

/// The band runs from the chat column's left edge to its right edge, sits flush
/// on its bottom edge, and every control of the row shares one line.
#[gpui::test]
async fn composer_band_spans_the_chat_column_on_one_row(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    refresh_frame(cx);

    let root = cx.debug_bounds("chat-root").expect("the chat pane");
    let composer = cx.debug_bounds("composer").expect("the band");
    let shell = cx
        .debug_bounds("ely-prompt-input")
        .expect("the band's shell");
    assert!(
        near(composer.left(), root.left(), 0.5) && near(composer.right(), root.right(), 0.5),
        "the band spans the chat: root={root:?} composer={composer:?}"
    );
    assert!(
        near(composer.bottom(), root.bottom(), 0.5),
        "the band is flush with the pane's bottom: root={root:?} composer={composer:?}"
    );
    assert!(
        near(shell.left(), composer.left(), 0.5) && near(shell.right(), composer.right(), 0.5),
        "the shell is the band: composer={composer:?} shell={shell:?}"
    );

    let send = drawn(cx, "send");
    for name in [
        "attach-image",
        "session-panel-toggle",
        "composer-overflow",
        "chat-status",
        "model-chip",
        "effort-chip",
        "fast-mode-chip",
        "thinking-chip",
        "background-tasks-chip",
        "context-ring",
    ] {
        let control = drawn(cx, name);
        assert!(
            near(control.center().y, send.center().y, 2.0),
            "{name} shares the send control's line: {name}={control:?} send={send:?}"
        );
    }
    let (attach, panel, overflow, status) = (
        drawn(cx, "attach-image"),
        drawn(cx, "session-panel-toggle"),
        drawn(cx, "composer-overflow"),
        drawn(cx, "chat-status"),
    );
    assert!(attach.right() <= panel.left(), "{attach:?} then {panel:?}");
    assert!(
        panel.right() <= overflow.left(),
        "{panel:?} then {overflow:?}"
    );
    assert!(
        overflow.right() < status.left(),
        "the tools stay left of the chips: {overflow:?} {status:?}"
    );
    assert!(
        send.right() <= composer.right() - px(12.0),
        "send keeps the band's side padding: send={send:?} composer={composer:?}"
    );

    assert!(
        cx.debug_bounds("send-ready").is_none(),
        "an empty draft leaves the disc inert"
    );
    focus_and_type(cx, "go");
    refresh_frame(cx);
    assert!(
        cx.debug_bounds("send-ready").is_some(),
        "a draft arms the disc"
    );
}

/// In a wide pane the band's content keeps to the transcript's reading column:
/// the field's first character sits on the transcript text's left edge.
#[gpui::test]
async fn the_band_field_lines_up_with_the_transcript_text(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    refresh_frame(cx);

    let transcript = cx.debug_bounds("chat-transcript").expect("the transcript");
    let field = cx.debug_bounds("composer-input").expect("the field");
    assert!(
        near(field.left(), transcript.left() + px(24.0), 1.0),
        "the field starts on the transcript text's edge: transcript={transcript:?} field={field:?}"
    );
}

/// A narrow pane keeps the band inside it and send reachable.
#[gpui::test]
async fn narrow_band_stays_inside_the_chat_and_keeps_send_reachable(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    cx.simulate_resize(size(px(595.0), px(600.0)));
    refresh_frame(cx);

    let root = cx.debug_bounds("chat-root").expect("the chat pane");
    let composer = cx.debug_bounds("composer").expect("the band");
    let send = cx.debug_bounds("send").expect("send is drawn");
    assert!(
        composer.left() >= root.left() && composer.right() <= root.right(),
        "the band stays inside the pane: root={root:?} composer={composer:?}"
    );
    assert!(
        near(composer.bottom(), root.bottom(), 0.5),
        "the band is flush with the pane's bottom: root={root:?} composer={composer:?}"
    );
    assert!(
        send.left() >= composer.left() && send.right() <= composer.right(),
        "send is inside the band: composer={composer:?} send={send:?}"
    );
}

/// "Model" and "Effort" are not written any more: the value speaks for itself.
/// The key labels have no selector, so this reads where the value starts: the
/// chip's own padding from its edge, with nothing in front of it, and the
/// effort's bars, new in the chip, ahead of the level.
#[gpui::test]
async fn the_model_and_effort_chips_carry_no_key_label(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    refresh_frame(cx);

    let model = cx.debug_bounds("model-chip").expect("model chip");
    let selection = cx
        .debug_bounds("model-selection-default")
        .expect("the model's name");
    assert!(
        near(selection.left(), model.left() + px(7.0), 1.0),
        "the model's name opens its chip: chip={model:?} name={selection:?}"
    );

    let effort = cx.debug_bounds("effort-chip").expect("effort chip");
    let bars = cx
        .debug_bounds("effort-chip-bars")
        .expect("the effort's bars are in its chip");
    let level = cx
        .debug_bounds("effort-chip-level")
        .expect("the effort's level is in its chip");
    assert!(
        near(bars.left(), effort.left() + px(7.0), 1.0),
        "the bars open the effort chip: chip={effort:?} bars={bars:?}"
    );
    assert!(
        bars.right() <= level.left() && level.right() <= effort.right(),
        "bars, then the level, inside the chip: chip={effort:?} bars={bars:?} level={level:?}"
    );
}

/// ⋯ sits in the band's left group, so its menu and the Chat History popover
/// must grow rightward from it into the chat. In a real window the chat has
/// neighbours, and the window's own snapping does not keep a menu off them:
/// here the chat fills the window, so the check is where the menu hangs from
/// (⋯'s left edge) as well as that it stays inside the chat.
#[gpui::test]
async fn the_overflow_menu_and_chat_history_open_inside_a_narrow_chat(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    cx.simulate_resize(size(px(430.0), px(700.0)));
    refresh_frame(cx);
    let root = drawn(cx, "chat-root");
    let overflow = drawn(cx, "composer-overflow");

    cx.update(|window, app| chat.update(app, |chat, cx| chat.toggle_overflow(window, cx)));
    refresh_frame(cx);
    let menu = drawn(cx, "composer-overflow-menu");
    assert!(
        menu.left() >= root.left() - px(0.5) && menu.right() <= root.right() + px(0.5),
        "the overflow menu stays inside the chat: root={root:?} menu={menu:?}"
    );
    assert!(
        near(menu.left(), overflow.left(), 0.5),
        "the overflow menu grows rightward from the control: overflow={overflow:?} menu={menu:?}"
    );

    cx.update(|window, app| chat.update(app, |chat, cx| chat.toggle_chat_history(window, cx)));
    refresh_frame(cx);
    let history = drawn(cx, "chat-history-menu");
    assert!(
        history.left() >= root.left() - px(0.5) && history.right() <= root.right() + px(0.5),
        "the chat history stays inside the chat: root={root:?} history={history:?}"
    );
    assert!(
        near(history.left(), overflow.left(), 0.5),
        "the chat history grows rightward from the control: overflow={overflow:?} history={history:?}"
    );
}
