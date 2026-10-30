//! The composer as a band under the chat: it spans the chat column, keeps its
//! content on the transcript's reading column, and holds every control on one
//! line.
use super::composer_row::{BAND_COLUMN, HYSTERESIS, RowLayout, row_width};
use super::controls::OverflowRow;
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
    // The Full row, which holds every control ROW_CONTROLS names.
    chat.update(cx, |chat, _| {
        configure_fitting_full_row(chat);
        chat.background_tasks = 2;
    });
    assert_fits(cx, &chat);
    resize_to(cx, 1920.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Full);

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
    for name in ROW_CONTROLS {
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

type Toggle = fn(&mut Chat, &mut Window, &mut Context<Chat>);

/// Opens `picker_selector` from `chip_selector` and checks where it hangs, then
/// closes it. The mode picker hangs from its chip's left edge and grows rightward
/// (`from_left`: its right edge must not pass the chat's); the others hang from
/// their chip's right edge and grow leftward (their left edge must not pass the
/// chat's). The chat fills the window here and the window snaps a popup back
/// inside it, so containment alone could never fail: the hang edge is the check
/// that carries each case.
fn check_picker(
    cx: &mut VisualTestContext,
    chat: &gpui::Entity<Chat>,
    chip_selector: &'static str,
    picker_selector: &'static str,
    toggle: Toggle,
    from_left: bool,
) {
    let root = drawn(cx, "chat-root");
    let chip = drawn(cx, chip_selector);
    cx.update(|window, app| chat.update(app, |chat, cx| toggle(chat, window, cx)));
    refresh_frame(cx);
    let picker = drawn(cx, picker_selector);
    if from_left {
        assert!(
            near(picker.left(), chip.left(), 0.5),
            "{picker_selector} hangs from its chip's left edge: chip={chip:?} picker={picker:?}"
        );
        assert!(
            picker.right() <= root.right(),
            "{picker_selector} stays inside the chat: root={root:?} picker={picker:?}"
        );
    } else {
        assert!(
            near(picker.right(), chip.right(), 0.5),
            "{picker_selector} hangs from its chip's right edge: chip={chip:?} picker={picker:?}"
        );
        assert!(
            picker.left() >= root.left(),
            "{picker_selector} stays inside the chat: root={root:?} picker={picker:?}"
        );
    }
    cx.update(|window, app| chat.update(app, |chat, cx| toggle(chat, window, cx)));
    refresh_frame(cx);
    assert!(
        cx.debug_bounds(picker_selector).is_none(),
        "{picker_selector} closes again"
    );
}

/// The mode pill is the first chip of the band's right-aligned group, so its
/// picker hangs from the pill's left edge and grows over the group; the other
/// chips' pickers hang from their right edge and grow toward the chat's interior.
/// Mode and model are checked in the Compact row without the chips that leave it
/// (a narrow row that still draws them overflows leftward over the tools), and
/// the Thinking chip in a Full row that fits.
#[gpui::test]
async fn the_chip_pickers_open_inside_a_narrow_chat(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());

    chat.update(cx, |chat, _| {
        configure_full_row(chat);
        without_the_chips_compact_will_move(chat);
    });
    let compact: [(f32, &'static str, &'static str, Toggle, bool); 3] = [
        (
            320.0,
            "chat-status",
            "mode-picker",
            Chat::toggle_mode_picker,
            true,
        ),
        (
            430.0,
            "model-chip",
            "model-picker",
            Chat::toggle_model_picker,
            false,
        ),
        (
            430.0,
            "effort-chip",
            "effort-picker",
            Chat::toggle_effort_picker,
            false,
        ),
    ];
    for (width, chip, picker, toggle, from_left) in compact {
        resize_to(cx, width, 700.0);
        assert_eq!(
            row_layout_now(cx, &chat),
            RowLayout::Compact,
            "at {width}px"
        );
        check_picker(cx, &chat, chip, picker, toggle, from_left);
    }

    chat.update(cx, |chat, _| configure_fitting_full_row(chat));
    assert_fits(cx, &chat);
    resize_to(cx, 1000.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Full, "at 1000px");
    check_picker(
        cx,
        &chat,
        "thinking-chip",
        "thinking-picker",
        Chat::toggle_thinking_picker,
        false,
    );
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

/// Bypass permissions is the one mode the pill draws as a warning, in the Full
/// row and in the Compact one.
#[gpui::test]
async fn bypass_is_the_only_mode_drawn_in_danger(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_fitting_full_row(chat));
    // The fixture fits with Accept edits; Bypass, the longest name, is checked
    // where it is drawn.
    assert_fits(cx, &chat);

    for (width, layout) in [(1920.0, RowLayout::Full), (360.0, RowLayout::Compact)] {
        resize_to(cx, width, 700.0);
        assert_eq!(row_layout_now(cx, &chat), layout, "at {width}px");
        for id in ["default", "acceptEdits", "plan", "auto", "ask"] {
            set_mode(cx, &chat, id);
            assert!(
                cx.debug_bounds("mode-chip-danger").is_none(),
                "mode {id:?} is not drawn in danger at {width}px"
            );
        }
        set_mode(cx, &chat, "bypassPermissions");
        assert_eq!(
            row_layout_now(cx, &chat),
            layout,
            "Bypass keeps the row {layout:?} at {width}px"
        );
        let pill = drawn(cx, "chat-status");
        let danger = drawn(cx, "mode-chip-danger");
        assert!(
            near(danger.left(), pill.left(), 0.5) && near(danger.right(), pill.right(), 0.5),
            "the danger fill is the pill itself at {width}px: pill={pill:?} danger={danger:?}"
        );
    }
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
        configure_fitting_full_row(chat);
        chat.mode_catalog = None;
    });
    assert_fits(cx, &chat);
    resize_to(cx, 1920.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Full);

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

/// Takes Fast, Thinking and the background tasks off the row. The Full row
/// draws Fast and Thinking, and a row that draws them and does not fit has its
/// chips lose their places: the tests that read where a chip is, in a row they
/// need to be Full, leave them out. (The Compact row moves them into ⋯.)
fn without_the_chips_compact_will_move(chat: &mut Chat) {
    chat.fast_mode = None;
    chat.thinking_display = None;
    chat.background_tasks = 0;
}

/// What [`configure_full_row`] gives, with a row whose Full layout fits the band:
/// a short model name, no background tasks and no chosen Thinking value. The
/// tests that use it open with [`assert_fits`], so a change of font cannot
/// silently turn them Compact. (The test text system is gpui's, at 7.8px a
/// character, whatever the font.)
pub(super) fn configure_fitting_full_row(chat: &mut Chat) {
    configure_full_row(chat);
    chat.available_models = vec![ModelOption {
        id: "default".into(),
        name: "Opus".into(),
        description: None,
    }];
    chat.background_tasks = 0;
    chat.thinking_display = Some(ThinkingDisplay {
        chosen: None,
        unsupported: false,
    });
}

/// The width the Full row needs, read from the chat as it is.
fn needed_now(cx: &mut VisualTestContext, chat: &gpui::Entity<Chat>) -> f32 {
    cx.update(|window, app| {
        let theme = *Theme::get(app);
        chat.read(app).full_row_needed(&theme, window)
    })
}

/// Asserts the Full row fits the band with room to spare, in the current mode.
fn assert_fits(cx: &mut VisualTestContext, chat: &gpui::Entity<Chat>) -> f32 {
    let needed = needed_now(cx, chat);
    assert!(
        needed < BAND_COLUMN - HYSTERESIS,
        "the fitting fixture's Full row needs {needed}px of the {BAND_COLUMN}px band"
    );
    needed
}

/// The narrowest whole-pixel chat width whose row is at least `row_width_wanted`
/// wide. gpui lays the chat out in whole pixels, so this is the width the probe
/// will read; the unit tests cover the f32 neighbours of every boundary.
fn chat_width_for(row_width_wanted: f32, docked: bool) -> f32 {
    let beside = if docked { 300.0 } else { 0.0 } + 24.0;
    let width = (row_width_wanted + beside).ceil();
    assert!(
        row_width(width, docked) >= row_width_wanted
            && row_width(width - 1.0, docked) < row_width_wanted,
        "a chat {width}px wide has a row of {}px, for {row_width_wanted}px wanted",
        row_width(width, docked)
    );
    width
}

/// The widest whole-pixel chat width whose row is narrower than `row_width_wanted`.
fn chat_width_just_below(row_width_wanted: f32, docked: bool) -> f32 {
    chat_width_for(row_width_wanted, docked) - 1.0
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
/// holds every control on one line there, and gives way one whole pixel under what
/// it needs. At exactly what it needs the model's name is still drawn whole, so a
/// measure that came up short would show as an ellipsized name. (With every chip a
/// chat can have and a model name this long the Full row is wider than the band
/// ever is; the unit tests cover that side.)
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
    let needed = needed_now(cx, &chat);
    assert!(
        needed + HYSTERESIS <= row_width(f32::INFINITY, false),
        "the long model's Full row ({needed}px) can be chosen in the widest band"
    );
    resize_to(cx, 1920.0, 700.0);
    let natural = drawn(cx, "model-selection-default").size.width;

    resize_to(cx, chat_width_for(needed + HYSTERESIS, false), 700.0);
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

    // From Full the row holds down to exactly what it needs, and nothing gives.
    resize_to(cx, chat_width_for(needed, false), 700.0);
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Full,
        "at exactly what it needs"
    );
    assert_one_line_inside_the_band(
        cx,
        &["attach-image", "chat-status", "model-chip", "context-ring"],
    );
    let model = drawn(cx, "model-selection-default").size.width;
    assert!(
        near(model, natural, 0.5),
        "the model's name is whole at exactly what the row needs: {model:?} of {natural:?}"
    );
    resize_to(cx, chat_width_just_below(needed, false), 700.0);
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Compact,
        "one pixel under what Full needs (needed={needed})"
    );
    // From Compact it takes a whole hysteresis step to go back.
    resize_to(cx, chat_width_for(needed + HYSTERESIS / 2.0, false), 700.0);
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Compact,
        "inside the band, from Compact"
    );
    resize_to(cx, chat_width_just_below(needed + HYSTERESIS, false), 700.0);
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Compact,
        "one pixel short of it"
    );
    resize_to(cx, chat_width_for(needed + HYSTERESIS, false), 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Full, "a step wider");
}

/// Crossing the threshold asks the chat to redraw by itself: a single draw at the
/// new width, with nothing else drawing it again, leaves a notification behind,
/// and the next draw lays the row out Compact. (Docked, so the width stays above the overlay's 640px,
/// where a change of width redraws anyway.)
#[gpui::test]
async fn crossing_the_threshold_asks_for_a_redraw(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_fitting_full_row(chat));
    let needed = assert_fits(cx, &chat);
    chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
    let beside = 300.0 + 24.0;
    assert!(
        needed > 640.0 - beside,
        "the threshold is a docked width: {needed}"
    );

    resize_to(cx, chat_width_for(needed, true), 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Full);
    let notified = std::rc::Rc::new(std::cell::Cell::new(0_usize));
    let _count = cx.update(|_, app| {
        let notified = notified.clone();
        app.observe(&chat, move |_, _| notified.set(notified.get() + 1))
    });
    cx.simulate_resize(size(px(chat_width_just_below(needed, true)), px(700.0)));
    cx.update(|window, app| window.draw(app).clear(app));
    cx.run_until_parked();
    assert!(
        notified.get() > 0,
        "the width crossed what Full needs and nothing asked the chat to redraw"
    );
    cx.update(|window, app| window.draw(app).clear(app));
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);
}

/// Without a catalogue the pill's word is the connection's state, and a turn
/// changes it from "idle" to "working": the layout must not follow it.
#[gpui::test]
async fn the_state_word_does_not_flip_the_layout(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    refresh_frame(cx);
    let needed = needed_now(cx, &chat);
    resize_to(cx, chat_width_for(needed, false), 700.0);
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Full,
        "idle, in a band as wide as the row needs"
    );
    chat.update(cx, |chat, _| chat.streaming = true);
    resize_to(cx, chat_width_for(needed, false), 700.0);
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Full,
        "working, in the same band"
    );
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
            "composer-overflow",
            "chat-status",
            "context-ring",
        ],
    );
}

/// A picker opened from a Compact chip stays inside a narrow pane: the mode
/// picker hangs rightward from its pill and fits a 320px chat; the effort picker
/// hangs leftward from its chip and fits one too. The model picker is as wide as
/// 280px and hangs leftward from a chip that sits about 190px from the chat's
/// left edge at 320px, so it holds from a 430px chat.
#[gpui::test]
async fn a_picker_from_a_compact_chip_stays_inside_a_narrow_pane(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| {
        configure_full_row(chat);
        without_the_chips_compact_will_move(chat);
    });

    let pickers: [(f32, &'static str, &'static str, Toggle, bool); 3] = [
        (
            320.0,
            "chat-status",
            "mode-picker",
            Chat::toggle_mode_picker,
            true,
        ),
        (
            430.0,
            "model-chip",
            "model-picker",
            Chat::toggle_model_picker,
            false,
        ),
        (
            320.0,
            "effort-chip",
            "effort-picker",
            Chat::toggle_effort_picker,
            false,
        ),
    ];
    for (width, chip, picker, toggle, from_left) in pickers {
        resize_to(cx, width, 700.0);
        assert_eq!(
            row_layout_now(cx, &chat),
            RowLayout::Compact,
            "at {width}px"
        );
        check_picker(cx, &chat, chip, picker, toggle, from_left);
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
            "composer-overflow",
            "chat-status",
            "model-chip",
            "effort-chip",
            "context-ring",
        ],
    );
    // The session panel's toggle left the row, so ⋯ is where it is reached.
    open_overflow(cx, &chat);
    drawn(cx, "overflow-session-panel");
}

/// Opens the ⋯ menu and lets the frame settle.
fn open_overflow(cx: &mut VisualTestContext, chat: &gpui::Entity<Chat>) {
    cx.update(|window, app| chat.update(app, |chat, cx| chat.toggle_overflow(window, cx)));
    refresh_frame(cx);
}

/// Clicks the centre of a drawn control and lets the frames settle.
fn click_drawn(cx: &mut VisualTestContext, name: &'static str) {
    let control = drawn(cx, name);
    cx.simulate_click(control.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    for _ in 0..3 {
        refresh_frame(cx);
    }
}

/// A Full row holds Fast, Thinking and the panel toggle on its one line; a
/// Compact one hands all three to ⋯, as rows above Follow, in that order.
#[gpui::test]
async fn compact_moves_fast_thinking_and_the_panel_into_the_overflow_menu(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    let moved = ["fast-mode-chip", "thinking-chip", "session-panel-toggle"];
    let rows = [
        "overflow-fast",
        "overflow-thinking",
        "overflow-session-panel",
    ];

    // Full: the chips are in the row, and ⋯ does not repeat them.
    chat.update(cx, |chat, _| configure_fitting_full_row(chat));
    assert_fits(cx, &chat);
    resize_to(cx, 1920.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Full);
    for name in moved {
        drawn(cx, name);
    }
    // Every control the row holds, Fast and Thinking among them, is on its one
    // line inside the band.
    assert_one_line_inside_the_band(
        cx,
        &[
            "attach-image",
            "session-panel-toggle",
            "composer-overflow",
            "chat-status",
            "model-chip",
            "effort-chip",
            "fast-mode-chip",
            "thinking-chip",
            "context-ring",
        ],
    );
    open_overflow(cx, &chat);
    drawn(cx, "overflow-follow");
    for name in rows {
        assert!(
            cx.debug_bounds(name).is_none(),
            "{name} is not in ⋯ while the row is Full"
        );
    }

    // Compact: the chips are gone from the row and are rows in ⋯.
    chat.update(cx, |chat, _| {
        chat.overflow_open = false;
        configure_full_row(chat);
    });
    resize_to(cx, 360.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);
    for name in moved {
        assert!(
            cx.debug_bounds(name).is_none(),
            "{name} left the Compact row"
        );
    }
    open_overflow(cx, &chat);
    let follow = drawn(cx, "overflow-follow");
    let mut above = None;
    for name in rows {
        let row = drawn(cx, name);
        if let Some(previous) = above {
            assert!(
                previous < row.top(),
                "{name} sits below the row before it: {previous:?} then {row:?}"
            );
        }
        above = Some(row.top());
    }
    assert!(
        above.expect("three rows") < follow.top(),
        "the three rows are above Follow Edited Files"
    );
}

/// Choosing Thinking in ⋯ closes the menu and opens the same picker, which
/// grows rightward from ⋯ (it is in the left group) and has the
/// focus. The chat fills the window here and the window snaps a popup back
/// inside it, so the containment check alone could never fail: the picker's
/// left edge on ⋯'s is what proves where it hangs.
#[gpui::test]
async fn the_overflow_thinking_row_opens_its_picker_inside_the_pane(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    resize_to(cx, 360.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);
    let root = drawn(cx, "chat-root");
    let overflow = drawn(cx, "composer-overflow");

    open_overflow(cx, &chat);
    click_drawn(cx, "overflow-thinking");

    assert!(
        cx.debug_bounds("composer-overflow-menu").is_none(),
        "choosing Thinking closes ⋯"
    );
    let picker = drawn(cx, "thinking-picker");
    assert!(
        picker.left() >= root.left() - px(0.5) && picker.right() <= root.right() + px(0.5),
        "the thinking picker stays inside the chat: root={root:?} picker={picker:?}"
    );
    assert!(
        near(picker.left(), overflow.left(), 0.5),
        "the picker grows rightward from ⋯: overflow={overflow:?} picker={picker:?}"
    );
    assert!(
        cx.update(|window, app| chat.read(app).thinking_picker_focus.is_focused(window)),
        "the picker keeps the focus"
    );
}

/// Fast mode the CLI has refused is a disabled row in ⋯: Ely's menu model
/// steps over it and will not run it. (Rendering the menu and pressing the row
/// proves nothing here: Ely gives a disabled row no mouse handler, the ACP test
/// client accepts any request, and the check moves only on the CLI's echo. The
/// menu model is what carries the disabling.) An enabled Fast row is the control
/// case, so the assertions can tell the two apart.
#[gpui::test]
async fn a_blocked_fast_row_is_disabled(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    resize_to(cx, 360.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);

    let menu_with = |cx: &mut VisualTestContext, blocked_by: Option<&str>| {
        chat.update(cx, |chat, cx| {
            chat.fast_mode = Some(FastMode {
                enabled: false,
                blocked_by: blocked_by.map(str::to_string),
            });
            chat.overflow_menu(cx)
        })
    };
    let open = menu_with(cx, None);
    assert_eq!(open.step(None, 1), Some(0), "Fast is the menu's first row");
    assert!(cx.update(|window, app| open.run(0, window, app)));

    let blocked = menu_with(cx, Some("sdk_opt_in_required"));
    assert_eq!(
        blocked.step(None, 1),
        Some(1),
        "the keys step over a blocked Fast row"
    );
    assert!(
        !cx.update(|window, app| blocked.run(0, window, app)),
        "a blocked Fast row does not run"
    );
}

/// ⋯ opened from the keyboard starts on the first row that can be chosen, and
/// the arrows step over a blocked Fast row in both directions.
#[gpui::test]
async fn the_keyboard_skips_a_blocked_fast_row(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| {
        configure_full_row(chat);
        chat.fast_mode = Some(FastMode {
            enabled: false,
            blocked_by: Some("sdk_opt_in_required".into()),
        });
    });
    resize_to(cx, 360.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);
    let rows = chat.read_with(cx, |chat, _| chat.overflowed_rows());
    assert_eq!(
        rows,
        [
            OverflowRow::Fast,
            OverflowRow::Thinking,
            OverflowRow::SessionPanel
        ]
    );

    open_overflow(cx, &chat);
    let cursor = |cx: &mut VisualTestContext| chat.read_with(cx, |chat, _| chat.picker_cursor);
    assert_eq!(
        cursor(cx),
        Some(1),
        "the cursor starts on Thinking, not Fast"
    );
    cx.update(|window, app| chat.update(app, |chat, cx| chat.picker_next(&PickerNext, window, cx)));
    assert_eq!(cursor(cx), Some(2), "Down moves to the next enabled row");
    chat.update(cx, |chat, _| chat.picker_cursor = Some(1));
    cx.update(|window, app| {
        chat.update(app, |chat, cx| {
            chat.picker_previous(&PickerPrevious, window, cx)
        })
    });
    let rows = chat.update(cx, |chat, cx| chat.picker_rows(cx).len());
    assert_eq!(
        cursor(cx),
        Some(rows - 1),
        "Up from Thinking wraps past the blocked Fast row"
    );
}

/// ⋯ carries the session panel's toggle once the row is Compact.
#[gpui::test]
async fn the_overflow_panel_row_opens_the_session_panel(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    resize_to(cx, 360.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);
    assert!(!chat.read_with(cx, |chat, _| chat.session_panel_open()));

    open_overflow(cx, &chat);
    click_drawn(cx, "overflow-session-panel");

    assert!(
        chat.read_with(cx, |chat, _| chat.session_panel_open()),
        "the row opens the session panel"
    );
    assert!(
        cx.debug_bounds("composer-overflow-menu").is_none(),
        "choosing the row closes ⋯"
    );
    assert!(
        cx.update(|window, app| {
            chat.read(app)
                .composer_field
                .read(app)
                .focus_handle(app)
                .is_focused(window)
        }),
        "the composer has the focus again"
    );
}

/// With every chip present, nothing in the row covers send or paints outside
/// the band, at 280px and at the 160px a pane can be dragged down to. Child
/// `debug_bounds` are the boxes layout gave the children, not what their parent
/// clips, so the chips are checked through the container that does the clipping
/// (`composer-chips`): it ends before send, inside the band.
#[gpui::test]
async fn the_chips_never_cover_send(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));

    for width in [280.0, 160.0] {
        resize_to(cx, width, 700.0);
        assert_eq!(
            row_layout_now(cx, &chat),
            RowLayout::Compact,
            "at {width}px"
        );
        let composer = drawn(cx, "composer");
        let send = drawn(cx, "send");
        let chips = drawn(cx, "composer-chips");
        assert!(
            send.left() >= composer.left() && send.right() <= composer.right(),
            "send is inside the band at {width}px: send={send:?} composer={composer:?}"
        );
        assert!(
            chips.right() <= send.left() + px(0.5),
            "the chips end before send at {width}px: chips={chips:?} send={send:?}"
        );
        assert!(
            chips.left() >= composer.left() && chips.right() <= composer.right(),
            "the chips are inside the band at {width}px: chips={chips:?} composer={composer:?}"
        );
        let overflow = drawn(cx, "composer-overflow");
        assert!(
            chips.left() >= overflow.right(),
            "the chips never reach the left tools at {width}px: overflow={overflow:?} chips={chips:?}"
        );
    }
}

/// The chips of the Compact row, left to right, that a full chat holds.
const COMPACT_CHIPS: [&str; 5] = [
    "chat-status",
    "model-chip",
    "effort-chip",
    "background-tasks-chip",
    "context-ring",
];

/// Whether every chip is drawn whole inside `composer-chips`, on one line.
fn chips_fit_whole(cx: &mut VisualTestContext) -> bool {
    let chips = drawn(cx, "composer-chips");
    let send = drawn(cx, "send");
    COMPACT_CHIPS.iter().all(|name| {
        let chip = drawn(cx, name);
        chip.right() <= chips.right() + px(0.5) && near(chip.center().y, send.center().y, 2.0)
    })
}

/// Below a measured width the Compact row cannot hold every chip whole, and the
/// rightmost ones (the ring first, then the task count) are what gets cut. The real
/// minimum with every chip a chat can have (mode, model at its 56px floor,
/// effort, two background tasks, ring) is 346px on the test font (7.8px a
/// character); it is found here by search, and the width one pixel under it
/// cuts the last chip, which is the ring.
#[gpui::test]
async fn compact_row_fits_every_chip_from_its_measured_minimum(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));

    let mut minimum = None;
    for width in 280..=480 {
        resize_to(cx, width as f32, 700.0);
        assert_eq!(
            row_layout_now(cx, &chat),
            RowLayout::Compact,
            "at {width}px"
        );
        if chips_fit_whole(cx) {
            minimum = Some(width);
            break;
        }
    }
    let minimum = minimum.expect("some width up to 480px holds every chip");
    assert!(
        (330..=360).contains(&minimum),
        "the measured minimum is about 346px, got {minimum}px"
    );
    // It holds from there up, and one pixel under it the ring is cut.
    for width in minimum..minimum + 20 {
        resize_to(cx, width as f32, 700.0);
        assert!(chips_fit_whole(cx), "every chip fits whole at {width}px");
        assert_one_line_inside_the_band(cx, &COMPACT_CHIPS);
    }
    resize_to(cx, (minimum - 1) as f32, 700.0);
    assert!(!chips_fit_whole(cx), "one pixel under, a chip is cut");
    let chips = drawn(cx, "composer-chips");
    assert!(
        drawn(cx, "context-ring").right() > chips.right() + px(0.5),
        "the ring, the last chip, is the one cut"
    );
    for name in &COMPACT_CHIPS[..4] {
        assert!(
            drawn(cx, name).right() <= chips.right() + px(0.5),
            "{name} is not cut: the cut falls on the right"
        );
    }
}

/// The chips are clipped, not merely sized to fit: a chip that does not fit is
/// still laid out past the band's edge, and only the clip keeps it from taking
/// presses there. This test cannot compare boxes for that, because the layout
/// boxes come out the same with and without the clip (Taffy ignores `overflow`
/// once `min_w_0` is set), and child `debug_bounds` are not clipped. What does
/// see the clip is hit-testing, which intersects every hitbox with its content
/// mask: it picks a width where the overflowing context ring's layout box covers
/// a point in the band's right padding (past send), and clicks that point. The
/// ring's press, opening the context popover, must not fire. The control case
/// is the same press on the ring in a roomy row, which does.
#[gpui::test]
async fn a_chip_that_does_not_fit_takes_no_press_in_the_band_padding(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    let popover_open =
        |cx: &mut VisualTestContext| chat.read_with(cx, |chat, _| chat.context_popover_open);

    // Control: the ring opens the popover when it is where the row draws it.
    chat.update(cx, |chat, _| configure_fitting_full_row(chat));
    resize_to(cx, 1920.0, 700.0);
    click_drawn(cx, "context-ring");
    assert!(popover_open(cx), "a press on the ring opens the popover");
    chat.update(cx, |chat, _| chat.context_popover_open = false);

    chat.update(cx, |chat, _| configure_full_row(chat));
    let mut found = None;
    for width in 281..346 {
        resize_to(cx, width as f32, 700.0);
        let composer = drawn(cx, "composer");
        let send = drawn(cx, "send");
        let ring = drawn(cx, "context-ring");
        // A point in the padding between send and the band's edge, inside the
        // ring's layout box, on the row's line.
        let from = f32::from(send.right()).max(f32::from(ring.left())) + 1.0;
        let to = f32::from(composer.right()).min(f32::from(ring.right())) - 1.0;
        if from < to {
            found = Some((width, gpui::point(px((from + to) / 2.0), send.center().y)));
            break;
        }
    }
    let (width, point) = found.expect("a width where the ring's box reaches into the padding");
    let ring = drawn(cx, "context-ring");
    let composer = drawn(cx, "composer");
    let send = drawn(cx, "send");
    assert!(
        ring.contains(&point) && point.x > send.right() && point.x < composer.right(),
        "the point is in the ring's box and the band's padding at {width}px: \
         point={point:?} ring={ring:?} send={send:?} composer={composer:?}"
    );

    cx.simulate_click(point, gpui::Modifiers::none());
    cx.run_until_parked();
    for _ in 0..3 {
        refresh_frame(cx);
    }
    assert!(
        !popover_open(cx),
        "a ring laid out past the band's edge took a press at {point:?} at {width}px"
    );
}
