//! The composer as a band under the chat: it spans the chat column, keeps its
//! content on the transcript's reading column, and holds every control on one
//! line.
use super::composer_row::{HYSTERESIS, RowLayout, row_width};
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
    // The Full row, which is the one that names the effort's level.
    chat.update(cx, |chat, _| {
        configure_full_row(chat);
        without_the_chips_compact_will_move(chat);
    });
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

/// The chips sit in the band's right-aligned group, so each picker hangs from
/// its chip's right edge and grows toward the chat's interior. In a real
/// window the chat has neighbours, and the window's own snapping does not keep
/// a picker off them: here the chat fills the window, so containment would
/// pass either way and the check that carries the test is where each picker
/// hangs from (its chip's right edge), and that the picker's left edge is not
/// past the chat's. The Thinking chip is drawn past a 430px chat's edge until
/// Compact moves it out of the row, so it is checked in a chat wide enough to
/// hold it.
#[gpui::test]
async fn the_chip_pickers_open_inside_a_narrow_chat(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());

    type Toggle = fn(&mut Chat, &mut Window, &mut Context<Chat>);
    // Width, whether Fast/Thinking/tasks are in the row, chip, picker, toggle.
    let pickers: [(f32, bool, &'static str, &'static str, Toggle); 4] = [
        (
            430.0,
            false,
            "chat-status",
            "mode-picker",
            Chat::toggle_mode_picker,
        ),
        (
            430.0,
            false,
            "model-chip",
            "model-picker",
            Chat::toggle_model_picker,
        ),
        (
            430.0,
            false,
            "effort-chip",
            "effort-picker",
            Chat::toggle_effort_picker,
        ),
        (
            1000.0,
            true,
            "thinking-chip",
            "thinking-picker",
            Chat::toggle_thinking_picker,
        ),
    ];
    for (width, with_extras, chip_selector, picker_selector, toggle) in pickers {
        chat.update(cx, |chat, _| {
            configure_full_row(chat);
            if !with_extras {
                without_the_chips_compact_will_move(chat);
            }
        });
        cx.simulate_resize(size(px(width), px(700.0)));
        resize_to(cx, width, 700.0);
        let root = drawn(cx, "chat-root");
        let chip = drawn(cx, chip_selector);
        cx.update(|window, app| chat.update(app, |chat, cx| toggle(chat, window, cx)));
        refresh_frame(cx);
        let picker = drawn(cx, picker_selector);
        assert!(
            near(picker.right(), chip.right(), 0.5),
            "{picker_selector} hangs from its chip's right edge: chip={chip:?} picker={picker:?}"
        );
        assert!(
            picker.left() >= root.left(),
            "{picker_selector} stays inside the chat: root={root:?} picker={picker:?}"
        );
        cx.update(|window, app| chat.update(app, |chat, cx| toggle(chat, window, cx)));
        refresh_frame(cx);
        assert!(
            cx.debug_bounds(picker_selector).is_none(),
            "{picker_selector} closes again"
        );
    }
}

/// Names the mode the chat is in, then lets the frame settle.
fn set_mode(cx: &mut VisualTestContext, chat: &gpui::Entity<Chat>, id: &str) {
    chat.update(cx, |chat, _| {
        chat.mode_catalog
            .as_mut()
            .expect("a mode catalogue")
            .current_id = id.into();
    });
    refresh_frame(cx);
}

/// Each Lucide icon the pill can draw, with the selector it carries.
const MODE_ICON_SELECTORS: [(&str, &str); 6] = [
    ("hand", "mode-icon-hand"),
    ("file-pen", "mode-icon-file-pen"),
    ("clipboard-list", "mode-icon-clipboard-list"),
    ("sparkles", "mode-icon-sparkles"),
    ("shield-off", "mode-icon-shield-off"),
    ("shield", "mode-icon-shield"),
];

/// Each of Claude's five permission modes has its own icon in the pill, and a
/// mode id Claude does not use (an ACP agent's own) still draws one.
#[gpui::test]
async fn each_claude_mode_draws_its_icon(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));

    for (id, icon) in [
        ("default", "hand"),
        ("acceptEdits", "file-pen"),
        ("plan", "clipboard-list"),
        ("auto", "sparkles"),
        ("bypassPermissions", "shield-off"),
        ("ask", "shield"),
    ] {
        set_mode(cx, &chat, id);
        for (name, selector) in MODE_ICON_SELECTORS {
            let drawn = cx.debug_bounds(selector).is_some();
            assert_eq!(
                drawn,
                name == icon,
                "mode {id:?} draws exactly the {icon} icon, and {selector} is {drawn}"
            );
        }
    }
}

/// Bypass permissions is the one mode the pill draws as a warning.
#[gpui::test]
async fn bypass_is_the_only_mode_drawn_in_danger(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));

    for id in ["default", "acceptEdits", "plan", "auto", "ask"] {
        set_mode(cx, &chat, id);
        assert!(
            cx.debug_bounds("mode-chip-danger").is_none(),
            "mode {id:?} is not drawn in danger"
        );
    }
    set_mode(cx, &chat, "bypassPermissions");
    let pill = drawn(cx, "chat-status");
    let danger = drawn(cx, "mode-chip-danger");
    assert!(
        near(danger.left(), pill.left(), 0.5) && near(danger.right(), pill.right(), 0.5),
        "the danger fill is the pill itself: pill={pill:?} danger={danger:?}"
    );
}

/// The connection dot rides the icon's bottom-right corner.
#[gpui::test]
async fn the_connection_dot_sits_on_the_icon_corner(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    refresh_frame(cx);

    let icon = drawn(cx, "mode-icon-file-pen");
    let dot = drawn(cx, "mode-connection-dot");
    let inflated = icon.dilate(px(4.0));
    assert!(
        inflated.contains(&dot.center()),
        "the dot's centre is within 4px of the icon: icon={icon:?} dot={dot:?}"
    );
    assert!(
        dot.center().x > icon.center().x && dot.center().y > icon.center().y,
        "the dot sits on the bottom-right corner: icon={icon:?} dot={dot:?}"
    );
}

/// A chat that has not advertised modes has nothing to draw an icon for: the
/// pill keeps its status dot and the state word.
#[gpui::test]
async fn without_a_catalogue_the_pill_keeps_its_dot_and_word(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| {
        configure_full_row(chat);
        chat.mode_catalog = None;
    });
    refresh_frame(cx);

    for (_, selector) in MODE_ICON_SELECTORS {
        assert!(
            cx.debug_bounds(selector).is_none(),
            "no mode, no {selector}"
        );
    }
    assert!(
        cx.debug_bounds("mode-connection-dot").is_none(),
        "the corner dot belongs to the icon"
    );
    assert!(
        cx.debug_bounds("mode-label").is_some(),
        "the state word stands in for the mode's name"
    );
    assert!(
        cx.debug_bounds("chat-status").is_some(),
        "the pill is drawn"
    );
}

/// The controls on the composer's one line, left to right.
const ROW_CONTROLS: [&str; 10] = [
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
];

/// A model name long enough to matter, short enough that the Full row holding it
/// still fits the widest band.
const LONG_MODEL_NAME: &str = "claude-opus-5-5-preview";

/// Gives the row [`LONG_MODEL_NAME`].
fn use_the_long_model_name(cx: &mut VisualTestContext, chat: &gpui::Entity<Chat>) {
    chat.update(cx, |chat, _| {
        chat.available_models = vec![ModelOption {
            id: "default".into(),
            name: LONG_MODEL_NAME.into(),
            description: None,
        }];
    });
}

/// Takes Fast, Thinking and the background tasks off the full row. The compact
/// layout moves Fast and Thinking out of the row (and the session panel toggle
/// with them), but until it does they are drawn, and with them a narrow row
/// overflows and its chips lose their places: the tests that read where a chip
/// is leave them out.
fn without_the_chips_compact_will_move(chat: &mut Chat) {
    chat.fast_mode = None;
    chat.thinking_display = None;
    chat.background_tasks = 0;
}

fn row_layout_now(cx: &mut VisualTestContext, chat: &gpui::Entity<Chat>) -> RowLayout {
    chat.read_with(cx, |chat, _| chat.composer_row.layout.get())
}

/// Resizes the window to `width`, lets the probe's deferred redraws land.
fn resize_to(cx: &mut VisualTestContext, width: f32, height: f32) {
    cx.simulate_resize(size(px(width), px(height)));
    refresh_frame(cx);
    refresh_frame(cx);
    refresh_frame(cx);
}

fn assert_one_line_inside_the_band(cx: &mut VisualTestContext, controls: &[&'static str]) {
    let composer = drawn(cx, "composer");
    let send = drawn(cx, "send");
    for name in controls.iter().copied() {
        let control = drawn(cx, name);
        assert!(
            near(control.center().y, send.center().y, 2.0),
            "{name} shares the send control's line: {name}={control:?} send={send:?}"
        );
        assert!(
            control.left() >= composer.left() && control.right() <= composer.right(),
            "{name} is inside the band: {name}={control:?} composer={composer:?}"
        );
    }
    assert!(
        send.right() <= composer.right() - px(12.0),
        "send keeps the band's side padding: send={send:?} composer={composer:?}"
    );
}

/// The Full row is chosen from exactly the width it needs plus the hysteresis,
/// holds every control on one line there, and gives way one pixel under what it
/// needs. (With every chip a chat can have and a model name this long the Full row
/// is wider than the band ever is: see the next test.)
#[gpui::test]
async fn full_row_fits_at_the_narrowest_width_it_is_chosen(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| {
        configure_full_row(chat);
        without_the_chips_compact_will_move(chat);
    });
    use_the_long_model_name(cx, &chat);
    refresh_frame(cx);
    let needed = cx.update(|window, app| {
        let theme = *Theme::get(app);
        chat.read(app).full_row_needed(&theme, window)
    });
    assert!(
        needed + HYSTERESIS <= row_width(f32::INFINITY, false),
        "the long model's Full row ({needed}px) can be chosen in the widest band"
    );

    // The band is `needed + HYSTERESIS` wide: the chat adds the band's side padding.
    resize_to(cx, needed + HYSTERESIS + 24.0, 700.0);
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Full,
        "needed={needed}"
    );
    assert_one_line_inside_the_band(
        cx,
        &[
            "attach-image",
            "session-panel-toggle",
            "composer-overflow",
            "chat-status",
            "model-chip",
            "effort-chip",
            "context-ring",
        ],
    );
    assert!(
        drawn(cx, "mode-label").size.width > px(0.0)
            && cx.debug_bounds("effort-chip-level").is_some(),
        "Full names the mode and the effort"
    );

    // Inside the hysteresis band the layout it had stays: Full while it was Full...
    resize_to(cx, needed + HYSTERESIS / 2.0 + 24.0, 700.0);
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Full,
        "inside the band"
    );
    resize_to(cx, needed - 1.0 + 24.0, 700.0);
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Compact,
        "one pixel under what Full needs (needed={needed})"
    );
    // ... and Compact while it was Compact, until the band is a whole step wider.
    resize_to(cx, needed + HYSTERESIS / 2.0 + 24.0, 700.0);
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Compact,
        "inside the band, from Compact"
    );
    resize_to(cx, needed + HYSTERESIS + 24.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Full, "a step wider");
}

/// A chat that has every chip and a long model name needs a Full row wider than
/// the band ever is, so it stays Compact however wide the chat gets, and it
/// does not flip as the frames go by.
#[gpui::test]
async fn a_row_wider_than_the_band_ever_gets_stays_compact(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    use_the_long_model_name(cx, &chat);
    refresh_frame(cx);
    let needed = cx.update(|window, app| {
        let theme = *Theme::get(app);
        chat.read(app).full_row_needed(&theme, window)
    });
    assert!(
        needed > row_width(f32::INFINITY, false),
        "the row needs {needed}px, more than the widest band"
    );
    for width in [1400.0, 900.0, 1400.0] {
        resize_to(cx, width, 700.0);
        assert_eq!(
            row_layout_now(cx, &chat),
            RowLayout::Compact,
            "at {width}px"
        );
        refresh_frame(cx);
        assert_eq!(
            row_layout_now(cx, &chat),
            RowLayout::Compact,
            "and it holds at {width}px"
        );
    }
}

/// Compact keeps the mode's icon and the effort's bars and drops their names.
#[gpui::test]
async fn compact_drops_the_mode_and_effort_names(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    resize_to(cx, 360.0, 700.0);

    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);
    let pill = drawn(cx, "chat-status");
    assert!(
        cx.debug_bounds("mode-icon-file-pen").is_some(),
        "the pill keeps its icon"
    );
    assert!(
        cx.debug_bounds("mode-label").is_none(),
        "the pill's name is in its tooltip, not drawn: {pill:?}"
    );
    let effort = drawn(cx, "effort-chip");
    assert!(
        cx.debug_bounds("effort-chip-bars").is_some(),
        "the effort chip keeps its bars: {effort:?}"
    );
    assert!(
        cx.debug_bounds("effort-chip-level").is_none(),
        "the effort's level is not drawn"
    );
}

/// With the session panel docked beside the band, the band is 300px narrower
/// than the chat, and the row follows the band, not the chat.
#[gpui::test]
async fn a_docked_panel_narrows_the_band_and_the_row_follows_it(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    // A row whose Full layout fits a 700px chat's band, and not what is left of
    // it beside the panel.
    chat.update(cx, |chat, _| {
        configure_full_row(chat);
        without_the_chips_compact_will_move(chat);
    });
    resize_to(cx, 700.0, 700.0);
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Full,
        "a 700px chat with no panel has room for the Full row"
    );

    chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
    for _ in 0..3 {
        refresh_frame(cx);
    }
    let root = drawn(cx, "chat-root");
    let composer = drawn(cx, "composer");
    assert!(
        near(composer.right(), root.right() - px(300.0), 1.0),
        "the panel docks beside the band: root={root:?} composer={composer:?}"
    );
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Compact,
        "the band is {}px wide, short of the Full row",
        f32::from(composer.size.width)
    );
}

/// A chat with no mode catalogue has no icon to draw, and its compact pill is the
/// bare dot; the row still fits at the narrowest pane.
#[gpui::test]
async fn compact_without_a_catalogue_keeps_the_dot_and_fits(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    resize_to(cx, 280.0, 600.0);

    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);
    for (_, selector) in MODE_ICON_SELECTORS {
        assert!(
            cx.debug_bounds(selector).is_none(),
            "no catalogue, no {selector}"
        );
    }
    assert!(
        cx.debug_bounds("mode-label").is_none(),
        "the compact pill is the dot alone"
    );
    assert!(cx.debug_bounds("chat-status").is_some(), "the dot is drawn");
    assert_one_line_inside_the_band(
        cx,
        &[
            "attach-image",
            "session-panel-toggle",
            "composer-overflow",
            "chat-status",
            "context-ring",
        ],
    );
}

/// Each picker hangs from its compact chip's right edge. The chat fills the
/// window here and the window snaps a popup back inside it, so containment alone
/// can never fail; the test asserts both that the picker's right edge is the chip's
/// and that its left edge is not past the chat's, which together fail if the hang
/// is wrong or the picker would cross the chat's left edge. The pickers are as wide
/// as 200 to 280px and hang leftward from a chip, so a chip must sit that far
/// from the chat's left edge: the mode and model pickers hold from a 430px chat,
/// the effort picker from a 320px one.
#[gpui::test]
async fn a_picker_from_a_compact_chip_stays_inside_a_narrow_pane(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| {
        configure_full_row(chat);
        without_the_chips_compact_will_move(chat);
    });

    type Toggle = fn(&mut Chat, &mut Window, &mut Context<Chat>);
    let pickers: [(f32, &'static str, &'static str, Toggle); 3] = [
        (
            430.0,
            "chat-status",
            "mode-picker",
            Chat::toggle_mode_picker,
        ),
        (
            430.0,
            "model-chip",
            "model-picker",
            Chat::toggle_model_picker,
        ),
        (
            320.0,
            "effort-chip",
            "effort-picker",
            Chat::toggle_effort_picker,
        ),
    ];
    for (width, chip_selector, picker_selector, toggle) in pickers {
        resize_to(cx, width, 700.0);
        assert_eq!(
            row_layout_now(cx, &chat),
            RowLayout::Compact,
            "at {width}px"
        );
        let root = drawn(cx, "chat-root");
        let chip = drawn(cx, chip_selector);
        cx.update(|window, app| chat.update(app, |chat, cx| toggle(chat, window, cx)));
        refresh_frame(cx);
        let picker = drawn(cx, picker_selector);
        assert!(
            near(picker.right(), chip.right(), 0.5),
            "{picker_selector} hangs from its chip's right edge at {width}px: chip={chip:?} picker={picker:?}"
        );
        assert!(
            picker.left() >= root.left(),
            "{picker_selector} stays inside the chat at {width}px: root={root:?} picker={picker:?}"
        );
        cx.update(|window, app| chat.update(app, |chat, cx| toggle(chat, window, cx)));
        refresh_frame(cx);
    }
}

/// At 320px every control the row still draws is on the band's one line, and send
/// is inside it.
#[gpui::test]
async fn narrow_control_row_keeps_every_essential_control_reachable(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| {
        configure_test_chat(chat);
        chat.effort = Some(EffortOption {
            option_id: "effort".into(),
            name: Some("Effort".into()),
            current_value: Some("xhigh".into()),
            choices: vec![EffortChoice {
                value: "xhigh".into(),
                name: "Xhigh".into(),
            }],
        });
    });
    resize_to(cx, 320.0, 600.0);

    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);
    assert_one_line_inside_the_band(
        cx,
        &[
            "attach-image",
            "session-panel-toggle",
            "composer-overflow",
            "model-chip",
            "effort-chip",
            "context-ring",
        ],
    );
    let composer = drawn(cx, "composer");
    let send = drawn(cx, "send");
    assert!(send.right() <= composer.right(), "{send:?} in {composer:?}");
}
