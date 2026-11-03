//! The composer as a band under the chat: it spans the chat column, keeps its
//! content on the transcript's reading column, and holds every control on one
//! line.
use super::composer_row::{BAND_COLUMN, HYSTERESIS, RowLayout, row_width};
use super::controls::{OverflowRow, PickerRow};
use super::tests::{
    chat_view, configure_test_chat, focus_and_type, pump_chat_until, refresh_frame,
};
use super::*;
use gpui::{Bounds, TestAppContext, VisualTestContext, size};
use sirio_chat::EffortChoice;

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

/// A catalogue whose current mode is `default` ("Ask"), the shortest pill a
/// catalogue gives.
fn ask_catalog() -> ModeCatalog {
    ModeCatalog {
        current_id: "default".into(),
        options: vec![AgentMode {
            id: "default".into(),
            name: "Ask".into(),
            description: None,
        }],
        config_option_id: None,
    }
}

/// Starts wide (Full), then holds Full down to exactly what the row says it
/// needs, and asserts that at that width every chip of `chips` is whole inside
/// the clip the chips sit in, the "Context" caption's chip included.
fn assert_full_holds_every_chip_at_its_need(
    cx: &mut VisualTestContext,
    chat: &gpui::Entity<Chat>,
    chips: &[&'static str],
) -> f32 {
    refresh_frame(cx);
    let needed = needed_now(cx, chat);
    assert!(
        needed + HYSTERESIS <= row_width(f32::INFINITY, false),
        "the row ({needed}px) can be chosen in the widest band"
    );
    resize_to(cx, 1920.0, 700.0);
    resize_to(cx, chat_width_for(needed, false), 700.0);
    assert_eq!(
        row_layout_now(cx, chat),
        RowLayout::Full,
        "at exactly what it needs (needed={needed})"
    );
    for name in chips {
        assert!(
            cx.debug_bounds(name).is_some(),
            "{name} is drawn in the Full row"
        );
    }
    let container = drawn(cx, "composer-chips");
    for name in chips {
        let chip = drawn(cx, name);
        assert!(
            chips_fit_whole(cx, &[name]),
            "{name} is cut by the chips' clip at exactly what Full needs ({needed}px): \
             {name}={chip:?} clip={container:?}"
        );
    }
    needed
}

/// The model's chip and the agent's badge are drawn at least 56px wide, however
/// short their text: the Full row counts that floor, not the text. With a short
/// agent badge ("Pi") and no models, a row sized to the text is 26px short and
/// the clip cuts the ring's chip. (The catalogue stays: without one the pill is
/// measured at its widest word and that slack hides the shortfall.)
#[gpui::test]
async fn full_row_counts_the_floor_under_a_short_agent_badge(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| {
        configure_test_chat(chat);
        chat.available_models = Vec::new();
        chat.mode_catalog = Some(ask_catalog());
        chat.agent_name = Some("Pi".into());
        chat.fast_mode = None;
        chat.thinking_display = None;
        chat.background_tasks = 0;
    });
    assert_full_holds_every_chip_at_its_need(
        cx,
        &chat,
        &["chat-status", "agent-badge", "context-chip"],
    );
}

/// The same with a short model name and every chip a Full row can hold: the
/// name "o3" is narrower than the floor, so the chip is drawn at the floor, and
/// the row has to count it.
#[gpui::test]
async fn full_row_counts_the_floor_under_a_short_model_name(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| {
        configure_full_row(chat);
        chat.mode_catalog = Some(ask_catalog());
        chat.available_models = vec![ModelOption {
            id: "default".into(),
            name: "o3".into(),
            description: None,
        }];
        chat.thinking_display = Some(ThinkingDisplay {
            chosen: Some("omitted".into()),
            unsupported: false,
        });
    });
    assert_full_holds_every_chip_at_its_need(
        cx,
        &chat,
        &[
            "chat-status",
            "model-chip",
            "effort-chip",
            "fast-mode-chip",
            "background-tasks-chip",
            "thinking-chip",
            "context-chip",
        ],
    );
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

/// A slot with nothing to draw takes no room: the chips' row puts a gap between
/// the chips it draws and no gap for a slot that is empty. In a Compact row with
/// no background tasks the effort chip and the context chip are neighbours, and
/// the room between them is one `gap_1`, not one more for each empty Fast,
/// task-count and Thinking slot between them.
#[gpui::test]
async fn an_empty_slot_takes_no_gap(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| {
        configure_full_row(chat);
        chat.background_tasks = 0;
    });
    resize_to(cx, 480.0, 700.0);

    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);
    let effort = drawn(cx, "effort-chip");
    let context = drawn(cx, "context-chip");
    let between = f32::from(context.left() - effort.right());
    assert!(
        (between - 4.0).abs() <= 0.5,
        "one gap between the effort chip and the context chip: effort={effort:?} context={context:?}"
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

    for (width, layout) in [(280.0, RowLayout::Narrow), (130.0, RowLayout::Minimal)] {
        resize_to(cx, width, 700.0);
        // Both are under what the Compact row needs with the task count and
        // the ring. 280px holds the Narrow row's three chips whole. 130px is
        // under the Minimal row's own minimum (143px with this fixture's three
        // effort levels), the one width a pane cannot have where the chips
        // overflow, so the clip is what keeps them off send.
        assert_eq!(row_layout_now(cx, &chat), layout, "at {width}px");
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
    "context-chip",
];

/// The chips of the Narrow row: the Compact ones less the task count and the ring,
/// which are rows in ⋯ there.
const NARROW_CHIPS: [&str; 3] = ["chat-status", "model-chip", "effort-chip"];

/// Whether every chip of `chips` is drawn whole inside `composer-chips`, on one
/// line.
fn chips_fit_whole(cx: &mut VisualTestContext, chips: &[&'static str]) -> bool {
    let container = drawn(cx, "composer-chips");
    let send = drawn(cx, "send");
    chips.iter().all(|name| {
        let chip = drawn(cx, name);
        chip.right() <= container.right() + px(0.5) && near(chip.center().y, send.center().y, 2.0)
    })
}

/// The narrowest whole-pixel chat width at which the Compact row holds every chip
/// whole, found by walking down from a roomy Compact row until the layout leaves
/// Compact or a chip is cut. Leaves the chat at that width, in Compact.
fn compact_minimum(cx: &mut VisualTestContext, chat: &gpui::Entity<Chat>) -> u32 {
    resize_to(cx, 480.0, 700.0);
    assert_eq!(row_layout_now(cx, chat), RowLayout::Compact, "at 480px");
    let mut minimum = 480;
    for width in (200..480).rev() {
        resize_to(cx, width as f32, 700.0);
        if row_layout_now(cx, chat) != RowLayout::Compact || !chips_fit_whole(cx, &COMPACT_CHIPS) {
            break;
        }
        minimum = width;
    }
    // The walk ended one pixel under it, in Narrow, which holds until a whole
    // hysteresis step above: back up over that, and down to the minimum.
    resize_to(cx, (minimum + HYSTERESIS as u32) as f32, 700.0);
    resize_to(cx, minimum as f32, 700.0);
    assert_eq!(
        row_layout_now(cx, chat),
        RowLayout::Compact,
        "at {minimum}px"
    );
    minimum
}

/// Below a measured width the Compact row cannot hold every chip whole, and the
/// layout gives way to Narrow, which moves the task count and the ring into ⋯.
/// On the test font, the real minimum with every chip a chat can have (mode, model
/// at its 56px floor, effort, two background tasks, ring) is 345px
/// (7.8px a character; the ring's glyph alone fits from 338px, its chip with the
/// padding it carries from 345px); it is found here by search, walking down from
/// a roomy row.
/// The measure is checked from both sides: at the minimum every chip is whole and the
/// model sits on its 56px floor (a Compact need that came out too high would hand
/// the model more than that), and one pixel under it the row is Narrow (one that
/// came out too low would leave a chip cut in a Compact row).
#[gpui::test]
async fn compact_row_fits_every_chip_from_its_measured_minimum(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    resize_to(cx, 1920.0, 700.0);
    let natural = drawn(cx, "model-chip").size.width;
    assert!(
        natural > px(56.0 + 20.0),
        "the fixture's model chip needs more than its 56px floor: {natural:?}"
    );

    let minimum = compact_minimum(cx, &chat);
    let model = drawn(cx, "model-chip").size.width;
    assert!(
        near(model, px(56.0), 1.0),
        "the model sits on its floor at the minimum: {model:?}"
    );
    // It holds from there up.
    for width in minimum..minimum + 20 {
        resize_to(cx, width as f32, 700.0);
        assert_eq!(
            row_layout_now(cx, &chat),
            RowLayout::Compact,
            "at {width}px"
        );
        assert!(
            chips_fit_whole(cx, &COMPACT_CHIPS),
            "every chip fits whole at {width}px"
        );
        assert_one_line_inside_the_band(cx, &COMPACT_CHIPS);
    }
    // One pixel under it the row is Narrow, and the two chips it cannot hold are
    // not drawn at all rather than cut.
    resize_to(cx, (minimum - 1) as f32, 700.0);
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Narrow,
        "one pixel under the Compact minimum"
    );
    for name in ["background-tasks-chip", "context-chip", "context-ring"] {
        assert!(cx.debug_bounds(name).is_none(), "{name} left the row");
    }
}

/// The Narrow row: the task count and the ring are not in the row and are the
/// last two rows ahead of Follow in ⋯; Fast, Thinking, the panel toggle and the
/// permission name are as absent as in Compact. From Narrow the row holds until a
/// whole hysteresis step above the Compact minimum, and there it is Compact with
/// both chips back and ⋯ without their rows.
#[gpui::test]
async fn narrow_moves_the_task_count_and_the_ring_into_the_overflow_menu(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    let minimum = compact_minimum(cx, &chat) as f32;
    let rows = [
        "overflow-fast",
        "overflow-thinking",
        "overflow-session-panel",
        "overflow-background-tasks",
        "overflow-context",
    ];

    resize_to(cx, minimum - 1.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Narrow);
    for name in [
        "background-tasks-chip",
        "context-chip",
        "context-ring",
        "context-label",
        "fast-mode-chip",
        "thinking-chip",
        "session-panel-toggle",
        "mode-label",
    ] {
        assert!(cx.debug_bounds(name).is_none(), "{name} is not in Narrow");
    }
    assert_one_line_inside_the_band(cx, &NARROW_CHIPS);
    let snapshot = chat.read_with(cx, |chat, _| chat.composer_snapshot());
    assert_eq!(snapshot.layout, "narrow");
    assert_eq!(
        snapshot.overflow,
        [
            "fast",
            "thinking",
            "session-panel",
            "background-tasks",
            "context"
        ]
    );

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
        above.expect("five rows") < follow.top(),
        "the five rows are above Follow Edited Files"
    );
    chat.update(cx, |chat, _| chat.overflow_open = false);

    // A step under the hysteresis the row stays Narrow; at it, it is Compact.
    resize_to(cx, minimum + HYSTERESIS - 1.0, 700.0);
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Narrow,
        "inside the band, from Narrow"
    );
    resize_to(cx, minimum + HYSTERESIS, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);
    for name in ["background-tasks-chip", "context-chip", "context-ring"] {
        drawn(cx, name);
    }
    open_overflow(cx, &chat);
    drawn(cx, "overflow-session-panel");
    for name in ["overflow-background-tasks", "overflow-context"] {
        assert!(
            cx.debug_bounds(name).is_none(),
            "{name} is not in ⋯ while the row is Compact"
        );
    }
    assert_eq!(
        chat.read_with(cx, |chat, _| chat.composer_snapshot().overflow),
        ["fast", "thinking", "session-panel"]
    );
}

/// Choosing the context row in a Narrow ⋯ closes the menu and opens the context
/// popover, which hangs from ⋯ (the ring is not drawn) with the focus in it. The
/// chat fills the window here and the window snaps a popup back inside it, so
/// containment alone could never fail: the popover's left edge on ⋯'s is what
/// proves where it hangs.
#[gpui::test]
async fn the_overflow_context_row_opens_its_popover_inside_the_pane(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    let minimum = compact_minimum(cx, &chat) as f32;
    resize_to(cx, minimum - 1.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Narrow);
    let root = drawn(cx, "chat-root");
    let overflow = drawn(cx, "composer-overflow");

    open_overflow(cx, &chat);
    click_drawn(cx, "overflow-context");

    assert!(
        cx.debug_bounds("composer-overflow-menu").is_none(),
        "choosing Context closes ⋯"
    );
    let popover = drawn(cx, "context-popover");
    assert!(
        popover.left() >= root.left() - px(0.5) && popover.right() <= root.right() + px(0.5),
        "the context popover stays inside the chat: root={root:?} popover={popover:?}"
    );
    assert!(
        near(popover.left(), overflow.left(), 0.5),
        "the popover hangs from ⋯: overflow={overflow:?} popover={popover:?}"
    );
    assert!(
        cx.update(|window, app| chat.read(app).context_popover_focus.is_focused(window)),
        "the popover has the focus"
    );

    // Chosen again with the popover already open, the row opens it, never closes it.
    // By the keys: a click is a mouse-down outside the popover, which closes it
    // before the row is chosen, whatever the row does.
    chat.update(cx, |chat, _| chat.overflow_open = false);
    open_overflow(cx, &chat);
    mark_overflow_row(cx, &chat, OverflowRow::Context);
    assert!(
        chat.read_with(cx, |chat, _| chat.context_popover_open),
        "the popover is open when the row is chosen"
    );
    cx.update(|window, app| {
        chat.update(app, |chat, cx| {
            chat.picker_choose(&PickerChoose, window, cx)
        })
    });
    refresh_frame(cx);
    assert!(
        chat.read_with(cx, |chat, _| chat.context_popover_open),
        "choosing Context with the popover open leaves it open"
    );
    drawn(cx, "context-popover");
}

/// Opens a Narrow ⋯ with every chip present and moves the keyboard cursor to
/// `row` with the keys.
fn narrow_overflow_marked_on(
    cx: &mut VisualTestContext,
    chat: &gpui::Entity<Chat>,
    row: OverflowRow,
) {
    chat.update(cx, |chat, _| configure_full_row(chat));
    resize_to(cx, 300.0, 700.0);
    assert_eq!(row_layout_now(cx, chat), RowLayout::Narrow);
    open_overflow(cx, chat);
    mark_overflow_row(cx, chat, row);
}

/// Moves the open ⋯'s keyboard cursor to `row` with the keys.
fn mark_overflow_row(cx: &mut VisualTestContext, chat: &gpui::Entity<Chat>, row: OverflowRow) {
    for _ in 0..8 {
        if marked_overflow_row(cx, chat) == Some(row) {
            return;
        }
        cx.update(|window, app| {
            chat.update(app, |chat, cx| chat.picker_next(&PickerNext, window, cx))
        });
    }
    panic!("the keys never reached {row:?}");
}

/// The row of ⋯ the keyboard cursor marks, as it is drawn and as Enter takes it.
fn marked_overflow_row(
    cx: &mut VisualTestContext,
    chat: &gpui::Entity<Chat>,
) -> Option<OverflowRow> {
    chat.read_with(cx, |chat, _| {
        let rows = chat.overflow_rows();
        chat.marked_picker_row(rows.len()).map(|at| rows[at])
    })
}

/// The marked row follows its row, not its place. A background task ends while ⋯ is
/// open, so the task count's row is gone and every row under it moves up one:
/// Enter on the row marked Context still opens the context popover, and does not
/// land on Follow, which has taken its place in the list.
#[gpui::test]
async fn the_overflow_cursor_stays_on_its_row_when_a_task_ends(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    narrow_overflow_marked_on(cx, &chat, OverflowRow::Context);
    assert!(!chat.read_with(cx, |chat, _| chat.following_edited_files));

    // The app's own way: the transport's count, read again after an event.
    chat.update(cx, |chat, cx| {
        chat.background_tasks = 0;
        cx.notify();
    });
    refresh_frame(cx);
    assert!(
        !chat
            .read_with(cx, |chat, _| chat.overflow_rows())
            .contains(&OverflowRow::BackgroundTasks),
        "the task count's row is gone"
    );
    assert_eq!(
        marked_overflow_row(cx, &chat),
        Some(OverflowRow::Context),
        "the mark stays on Context"
    );

    cx.update(|window, app| {
        chat.update(app, |chat, cx| {
            chat.picker_choose(&PickerChoose, window, cx)
        })
    });
    refresh_frame(cx);
    assert!(
        chat.read_with(cx, |chat, _| chat.context_popover_open),
        "Enter opened the context popover"
    );
    assert!(
        cx.debug_bounds("composer-overflow-menu").is_none(),
        "choosing Context closes ⋯"
    );
    assert!(
        !chat.read_with(cx, |chat, _| chat.following_edited_files),
        "Follow was not toggled"
    );
}

/// When the marked row itself goes, the mark moves to the nearest row that can be
/// chosen, never onto one that cannot: Thinking and Fast go with ⋯ open, and
/// the place Thinking had now holds the task count's row, which only says how many
/// tasks run.
#[gpui::test]
async fn the_overflow_cursor_moves_to_a_row_that_can_be_chosen_when_its_row_goes(
    cx: &mut TestAppContext,
) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    narrow_overflow_marked_on(cx, &chat, OverflowRow::Thinking);

    chat.update(cx, |chat, cx| {
        chat.fast_mode = None;
        chat.thinking_display = None;
        cx.notify();
    });
    refresh_frame(cx);
    let rows = chat.read_with(cx, |chat, _| chat.overflow_rows());
    assert!(
        !rows.contains(&OverflowRow::Thinking) && !rows.contains(&OverflowRow::Fast),
        "Fast and Thinking are gone: {rows:?}"
    );
    let marked = marked_overflow_row(cx, &chat).expect("a row is marked");
    assert!(
        chat.read_with(cx, |chat, _| chat.picker_row_enabled(&PickerRow::Overflow(marked))),
        "the mark is on a row that can be chosen, not on {marked:?}"
    );
}

/// Compact draws an effort as its bars, and keeps the level's name only for an effort
/// that has no bars to draw (the reset, Ultracode), so that the chip is never a bare
/// chevron.
#[gpui::test]
async fn compact_keeps_the_effort_label_only_where_there_are_no_bars(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| {
        configure_full_row(chat);
        chat.effort
            .as_mut()
            .expect("the full row has an effort")
            .choices
            .push(EffortChoice {
                value: "default".into(),
                name: "Default".into(),
            });
    });
    resize_to(cx, 480.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);

    assert!(
        cx.debug_bounds("effort-chip-bars").is_some(),
        "a barred effort draws its bars"
    );
    assert!(
        cx.debug_bounds("model-effort-label").is_none(),
        "and in Compact leaves its name to the tooltip"
    );

    chat.update(cx, |chat, _| {
        chat.effort.as_mut().unwrap().current_value = Some("default".into());
    });
    refresh_frame(cx);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);
    assert!(
        cx.debug_bounds("effort-chip-bars").is_none(),
        "the reset has no bars"
    );
    assert!(
        cx.debug_bounds("model-effort-label").is_some(),
        "so its name stays, and the chip is not a bare chevron"
    );

    // Ultracode is no level either: Compact keeps its name too.
    chat.update(cx, |chat, _| {
        let effort = chat.effort.as_mut().unwrap();
        effort.choices.push(EffortChoice {
            value: "ultracode".into(),
            name: "Ultracode".into(),
        });
        effort.current_value = Some("ultracode".into());
    });
    refresh_frame(cx);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);
    assert!(
        cx.debug_bounds("effort-chip-bars").is_none(),
        "Ultracode has no bars in Compact"
    );
    assert!(
        cx.debug_bounds("model-effort-label").is_some(),
        "so its name stays"
    );
}

/// In Compact the ring keeps its popover: it hangs from the ring's chip and ends
/// on that chip's right edge, growing leftward. (⋯'s placement is for Narrow.)
/// The chat fills the window and the window snaps a popup back inside it, so it is
/// the right edge on the chip's that proves where it hangs.
#[gpui::test]
async fn the_ring_still_opens_its_popover_from_the_ring_in_compact(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    let minimum = compact_minimum(cx, &chat) as f32;
    resize_to(cx, minimum + 40.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);

    click_drawn(cx, "context-ring");
    let popover = drawn(cx, "context-popover");
    let chip = drawn(cx, "context-chip");
    assert!(
        near(popover.right(), chip.right(), 0.5),
        "in Compact the popover ends on the ring's chip: chip={chip:?} popover={popover:?}"
    );
}

/// Narrow keeps what Compact does for Thinking: the row is in ⋯, and the picker
/// it opens hangs from ⋯.
#[gpui::test]
async fn the_overflow_thinking_row_opens_its_picker_from_a_narrow_row_too(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    resize_to(cx, 300.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Narrow);
    let overflow = drawn(cx, "composer-overflow");

    open_overflow(cx, &chat);
    click_drawn(cx, "overflow-thinking");

    let picker = drawn(cx, "thinking-picker");
    assert!(
        near(picker.left(), overflow.left(), 0.5),
        "the picker hangs from ⋯: overflow={overflow:?} picker={picker:?}"
    );
}

/// The task count in ⋯ is information, not an action: Ely's menu steps over it
/// and will not run it, the keys step over it, and the row has no effect when
/// chosen. An enabled neighbour (Follow) is the control case, so the assertions
/// can tell a disabled row from one that merely does nothing visible.
#[gpui::test]
async fn the_background_tasks_row_is_informational(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    resize_to(cx, 300.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Narrow);
    let rows = chat.read_with(cx, |chat, _| chat.overflow_rows());
    let tasks = rows
        .iter()
        .position(|row| *row == OverflowRow::BackgroundTasks)
        .expect("the task count is a row of ⋯");
    let follow = rows
        .iter()
        .position(|row| *row == OverflowRow::Follow)
        .expect("Follow is a row of ⋯");

    // The menu model.
    let menu = chat.update(cx, |chat, cx| chat.overflow_menu(cx));
    assert_eq!(
        menu.step(Some(tasks - 1), 1),
        Some(tasks + 1),
        "down skips it"
    );
    assert_eq!(
        menu.step(Some(tasks + 1), -1),
        Some(tasks - 1),
        "up skips it"
    );
    assert!(
        !cx.update(|window, app| menu.run(tasks, window, app)),
        "the menu does not run the task count"
    );
    assert!(
        cx.update(|window, app| menu.run(follow, window, app)),
        "the control: an enabled row runs"
    );
    assert!(
        chat.read_with(cx, |chat, _| chat.following_edited_files),
        "the control ran Follow"
    );

    // The chat's own keys.
    assert!(!chat.read_with(cx, |chat, _| {
        chat.picker_row_enabled(&PickerRow::Overflow(OverflowRow::BackgroundTasks))
    }));
    open_overflow(cx, &chat);
    let cursor = |cx: &mut VisualTestContext| chat.read_with(cx, |chat, _| chat.picker_cursor);
    chat.update(cx, |chat, _| chat.picker_cursor = Some(tasks - 1));
    cx.update(|window, app| chat.update(app, |chat, cx| chat.picker_next(&PickerNext, window, cx)));
    assert_eq!(
        cursor(cx),
        Some(tasks + 1),
        "Down steps over the task count"
    );
    cx.update(|window, app| {
        chat.update(app, |chat, cx| {
            chat.picker_previous(&PickerPrevious, window, cx)
        })
    });
    assert_eq!(cursor(cx), Some(tasks - 1), "Up steps over it");

    // Choosing it by any path does nothing.
    chat.update(cx, |chat, _| chat.picker_cursor = Some(tasks));
    cx.update(|window, app| {
        chat.update(app, |chat, cx| {
            chat.picker_choose(&PickerChoose, window, cx)
        })
    });
    assert!(
        chat.read_with(cx, |chat, _| chat.overflow_open
            && !chat.context_popover_open),
        "choosing the task count leaves ⋯ as it was"
    );
}

/// The row's words: the ring's own sentence once the agent has said how full its
/// context is, "Context" until then, and never a zero nobody measured.
#[gpui::test]
async fn the_context_row_reads_as_the_ring_does(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    let label = |cx: &mut VisualTestContext, usage: Option<(u64, u64)>| {
        chat.update(cx, |chat, _| {
            chat.context_usage = usage.map(|(used, size)| ContextUsage {
                used,
                size,
                ..Default::default()
            });
            chat.context_row_label()
        })
    };
    assert_eq!(label(cx, None), "Context");
    assert_eq!(
        label(cx, Some((0, 0))),
        "Context",
        "no window size, no percent"
    );
    assert_eq!(label(cx, Some((45, 100))), "45% of context used");
    assert_eq!(label(cx, Some((2, 3))), "67% of context used", "rounded");
}

/// The chips of the Minimal row: the permission pill and the effort, without
/// chevrons. The model is a row of ⋯ there, and so is attach, which leaves ⋯ alone
/// in the left group.
const MINIMAL_CHIPS: [&str; 2] = ["chat-status", "effort-chip"];

/// The narrowest whole-pixel chat width at which the Narrow row holds its three
/// chips whole, found by walking down from a roomy Narrow row until the layout
/// leaves Narrow or a chip is cut. Leaves the chat at that width, in Narrow.
fn narrow_minimum(cx: &mut VisualTestContext, chat: &gpui::Entity<Chat>) -> u32 {
    resize_to(cx, 300.0, 700.0);
    assert_eq!(row_layout_now(cx, chat), RowLayout::Narrow, "at 300px");
    let mut minimum = 300;
    for width in (160..300).rev() {
        resize_to(cx, width as f32, 700.0);
        if row_layout_now(cx, chat) != RowLayout::Narrow || !chips_fit_whole(cx, &NARROW_CHIPS) {
            break;
        }
        minimum = width;
    }
    // The walk ended one pixel under it, in Minimal, which holds until a whole
    // hysteresis step above: back up over that, and down to the minimum.
    resize_to(cx, (minimum + HYSTERESIS as u32) as f32, 700.0);
    resize_to(cx, minimum as f32, 700.0);
    assert_eq!(
        row_layout_now(cx, chat),
        RowLayout::Narrow,
        "at {minimum}px"
    );
    minimum
}

/// Below its measured minimum the Narrow row gives way to Minimal, which moves
/// the model into ⋯. On the test font, the real minimum with a mode, a model at
/// its 56px floor and an effort is 269px: found by search, walking down from a
/// roomy Narrow row. The measure is checked from both sides: at the minimum every
/// chip is whole and the model sits on its 56px floor (a Narrow need that came out
/// too high would hand the model more than that), and one pixel under it the row is
/// Minimal and the model chip is not drawn (one that came out too low would leave
/// the effort cut in a Narrow row, which the search would have stopped at).
#[gpui::test]
async fn narrow_row_fits_from_its_measured_minimum(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    resize_to(cx, 1920.0, 700.0);
    assert!(
        drawn(cx, "model-chip").size.width > px(56.0 + 20.0),
        "the fixture's model chip needs more than its 56px floor"
    );

    let minimum = narrow_minimum(cx, &chat);
    let model = drawn(cx, "model-chip").size.width;
    assert!(
        near(model, px(56.0), 1.0),
        "the model sits on its floor at the minimum: {model:?}"
    );
    for width in minimum..minimum + 20 {
        resize_to(cx, width as f32, 700.0);
        assert_eq!(row_layout_now(cx, &chat), RowLayout::Narrow, "at {width}px");
        assert!(
            chips_fit_whole(cx, &NARROW_CHIPS),
            "every Narrow chip fits whole at {width}px"
        );
        assert_one_line_inside_the_band(cx, &NARROW_CHIPS);
    }
    resize_to(cx, (minimum - 1) as f32, 700.0);
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Minimal,
        "one pixel under the Narrow minimum"
    );
    assert!(
        cx.debug_bounds("model-chip").is_none(),
        "the model chip left the row rather than being cut"
    );
}

/// The chips are clipped, not merely sized to fit: a chip that does not fit is
/// still laid out past the band's edge, and only the clip keeps it from taking
/// presses there. This test cannot compare boxes for that, because the layout
/// boxes come out the same with and without the clip (Taffy ignores `overflow`
/// once `min_w_0` is set), and child `debug_bounds` are not clipped. What does
/// see the clip is hit-testing, which intersects every hitbox with its content
/// mask: it picks a width where the overflowing effort chip's layout box covers
/// a point in the band's right padding (past send), and clicks that point. The
/// chip's press, opening the effort picker, must not fire. The control case is
/// the same press on the chip in a roomy row, which does. (The effort chip is
/// the last one the Minimal row draws, so it is what overflows first, and only
/// under the Minimal row's measured minimum.)
#[gpui::test]
async fn a_chip_that_does_not_fit_takes_no_press_in_the_band_padding(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    let picker_open =
        |cx: &mut VisualTestContext| chat.read_with(cx, |chat, _| chat.effort_picker_open);

    // Control: the chip opens its picker when it is where the row draws it.
    chat.update(cx, |chat, _| configure_fitting_full_row(chat));
    resize_to(cx, 1920.0, 700.0);
    click_drawn(cx, "effort-chip");
    assert!(
        picker_open(cx),
        "a press on the effort chip opens its picker"
    );
    chat.update(cx, |chat, _| chat.effort_picker_open = false);

    chat.update(cx, |chat, _| configure_full_row(chat));
    // The Narrow row's chips all fit from its minimum up, and the Minimal row's from
    // its own (143px with this fixture's three effort levels). Under that the effort
    // chip is cut, but its box reaches into the band's right padding only once the
    // chat is under about 112px, which a split pane cannot be (160px is its floor).
    // The clip is the same at any width, so the search walks down from the floor to
    // the first width where the box does reach the padding.
    let mut found = None;
    for width in (60..=160).rev() {
        resize_to(cx, width as f32, 700.0);
        assert_ne!(row_layout_now(cx, &chat), RowLayout::Compact, "at {width}px");
        let composer = drawn(cx, "composer");
        let send = drawn(cx, "send");
        let chip = drawn(cx, "effort-chip");
        // A point in the padding between send and the band's edge, inside the
        // chip's layout box, on the row's line.
        let from = f32::from(send.right()).max(f32::from(chip.left())) + 1.0;
        let to = f32::from(composer.right()).min(f32::from(chip.right())) - 1.0;
        if from < to {
            found = Some((width, gpui::point(px((from + to) / 2.0), send.center().y)));
            break;
        }
    }
    let (width, point) = found.expect("a width where the chip's box reaches into the padding");
    let chip = drawn(cx, "effort-chip");
    let composer = drawn(cx, "composer");
    let send = drawn(cx, "send");
    assert!(
        chip.contains(&point) && point.x > send.right() && point.x < composer.right(),
        "the point is in the chip's box and the band's padding at {width}px: \
         point={point:?} chip={chip:?} send={send:?} composer={composer:?}"
    );

    cx.simulate_click(point, gpui::Modifiers::none());
    cx.run_until_parked();
    for _ in 0..3 {
        refresh_frame(cx);
    }
    assert!(
        !picker_open(cx),
        "a chip laid out past the band's edge took a press at {point:?} at {width}px"
    );
}

/// The Compact row gives way to Narrow, from a docked width, without anything else
/// drawing the chat: a single draw across what Compact needs leaves a
/// notification behind, and the next draw lays the row out Narrow. (Docked, so
/// the width stays above the overlay's 640px, where any change of width redraws.)
#[gpui::test]
async fn crossing_into_narrow_asks_for_a_redraw(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
    resize_to(cx, 1200.0, 700.0);
    let (full, compact) = chat.read_with(cx, |chat, _| {
        (
            chat.composer_row.needed.get(),
            chat.composer_row.compact_needed.get(),
        )
    });
    let beside = 300.0 + 24.0;
    assert!(
        compact > 640.0 - beside && compact < full,
        "Compact's need is a docked width: compact={compact} full={full}"
    );

    resize_to(cx, chat_width_for(compact, true), 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact);
    let notified = std::rc::Rc::new(std::cell::Cell::new(0_usize));
    let _count = cx.update(|_, app| {
        let notified = notified.clone();
        app.observe(&chat, move |_, _| notified.set(notified.get() + 1))
    });
    cx.simulate_resize(size(px(chat_width_just_below(compact, true)), px(700.0)));
    cx.update(|window, app| window.draw(app).clear(app));
    cx.run_until_parked();
    assert!(
        notified.get() > 0,
        "the width crossed what Compact needs and nothing asked the chat to redraw"
    );
    cx.update(|window, app| window.draw(app).clear(app));
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Narrow);
}

/// Minimal, under the Narrow row's fit: the model chip and the attach button are
/// not in the row and are the first two rows of ⋯, ahead of Fast, Thinking, the panel toggle, the task count
/// and the ring; and the chips that stay, the permission pill and the effort, lose
/// their chevrons. From Minimal the row holds until a whole hysteresis step above
/// the Narrow minimum, and there it is Narrow with the model chip and every
/// chevron back.
#[gpui::test]
async fn minimal_moves_the_model_into_the_overflow_menu_and_drops_the_chevrons(
    cx: &mut TestAppContext,
) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    let minimum = narrow_minimum(cx, &chat) as f32;
    let chevrons = ["model-chip-chevron", "mode-chip-chevron", "effort-chip-chevron"];

    // Control: the Narrow row draws the model chip and all three chevrons, so
    // their absence below is the layout's doing.
    drawn(cx, "model-chip");
    drawn(cx, "attach-image");
    for name in chevrons {
        drawn(cx, name);
    }

    resize_to(cx, minimum - 1.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Minimal);
    for name in ["model-chip", "agent-badge", "attach-image"]
        .into_iter()
        .chain(chevrons)
    {
        assert!(cx.debug_bounds(name).is_none(), "{name} is not in Minimal");
    }
    // What stays: the pill as its icon, the effort as its bars.
    drawn(cx, "mode-icon-file-pen");
    drawn(cx, "effort-chip-bars");
    assert!(
        cx.debug_bounds("effort-chip-level").is_none(),
        "the effort keeps its bars and no name"
    );
    assert!(chips_fit_whole(cx, &MINIMAL_CHIPS));
    assert_one_line_inside_the_band(cx, &MINIMAL_CHIPS);
    let snapshot = chat.read_with(cx, |chat, _| chat.composer_snapshot());
    assert_eq!(snapshot.layout, "minimal");
    assert_eq!(
        snapshot.overflow,
        [
            "model",
            "attach",
            "fast",
            "thinking",
            "session-panel",
            "background-tasks",
            "context"
        ]
    );

    open_overflow(cx, &chat);
    let follow = drawn(cx, "overflow-follow");
    let mut above = None;
    for name in [
        "overflow-model",
        "overflow-attach",
        "overflow-fast",
        "overflow-thinking",
        "overflow-session-panel",
        "overflow-background-tasks",
        "overflow-context",
    ] {
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
        above.expect("seven rows") < follow.top(),
        "the seven rows are above Follow Edited Files"
    );
    let menu = chat
        .read_with(cx, |chat, _| chat.composer_snapshot().menu)
        .expect("the menu is open");
    assert_eq!(menu[0].id, "model");
    assert!(menu[0].enabled, "the model row can be chosen");
    assert_eq!(menu[1].id, "attach");
    assert!(menu[1].enabled, "the attach row can be chosen");
    chat.update(cx, |chat, _| chat.overflow_open = false);

    // A step under the hysteresis the row stays Minimal; at it, it is Narrow.
    resize_to(cx, minimum + HYSTERESIS - 1.0, 700.0);
    assert_eq!(
        row_layout_now(cx, &chat),
        RowLayout::Minimal,
        "inside the band, from Minimal"
    );
    resize_to(cx, minimum + HYSTERESIS, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Narrow);
    drawn(cx, "model-chip");
    for name in chevrons {
        drawn(cx, name);
    }
    open_overflow(cx, &chat);
    for name in ["overflow-model", "overflow-attach"] {
        assert!(
            cx.debug_bounds(name).is_none(),
            "{name} is not a row of ⋯ while the row is Narrow"
        );
    }
    drawn(cx, "attach-image");
}

/// A chat laid out in a column of fixed width, in a window wider than it: the shape of
/// a split pane in the app, where the window keeps a popup on screen and the pane
/// does not.
struct PaneInWindow {
    chat: gpui::Entity<Chat>,
    width: f32,
}

impl Render for PaneInWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(self.width))
            .h_full()
            .child(self.chat.clone())
    }
}

/// Choosing the model row in a Minimal ⋯ in a 160px pane (`MIN_SPLIT_PANE_SIZE`)
/// closes the menu and opens the model picker with the search field focused, and the
/// picker hangs from ⋯ (the chip is not drawn), as the thinking picker does, at its
/// usual width, which is wider than the pane: the window keeps it on screen. The
/// picker's left edge on ⋯'s is what proves where it hangs (hung from the right, it
/// would be pushed to the window's margin).
#[gpui::test]
async fn the_overflow_model_row_opens_the_model_picker_hung_from_the_overflow_button(
    cx: &mut TestAppContext,
) {
    cx.update(Theme::init);
    cx.update(crate::ely::init);
    cx.update(super::init);
    let (host, cx) = cx.add_window_view(|_, cx| {
        let command = AgentCommand::new("python3")
            .arg(super::tests::CHAT_FIXTURE)
            .arg("plain");
        let chat = cx.new(|cx| {
            Chat::from_test_command(LaunchSpec::Acp(command), std::env::temp_dir(), cx)
        });
        PaneInWindow { chat, width: 160.0 }
    });
    let chat = host.read_with(cx, |host, _| host.chat.clone());
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    cx.simulate_resize(size(px(900.0), px(700.0)));
    refresh_frame(cx);
    refresh_frame(cx);
    refresh_frame(cx);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Minimal);
    assert!(
        near(drawn(cx, "chat-root").size.width, px(160.0), 0.5),
        "the pane is 160px in a wider window"
    );
    let overflow = drawn(cx, "composer-overflow");

    open_overflow(cx, &chat);
    mark_overflow_row(cx, &chat, OverflowRow::Model);
    cx.update(|window, app| {
        chat.update(app, |chat, cx| {
            chat.picker_choose(&PickerChoose, window, cx)
        })
    });
    for _ in 0..3 {
        refresh_frame(cx);
    }

    assert!(
        cx.debug_bounds("composer-overflow-menu").is_none(),
        "choosing Model closes ⋯"
    );
    assert_eq!(
        chat.read_with(cx, |chat, _| chat.composer_snapshot().picker),
        Some("model")
    );
    let picker = drawn(cx, "model-picker");
    let window = cx.update(|window, _| window.bounds());
    assert!(
        picker.left() >= px(0.0) && picker.right() <= window.size.width,
        "the picker is inside the window: picker={picker:?} window={window:?}"
    );
    assert!(
        near(picker.left(), overflow.left(), 0.5),
        "the picker hangs from ⋯: overflow={overflow:?} picker={picker:?}"
    );
    assert!(
        cx.update(|window, app| {
            chat.read(app)
                .model_search_field
                .read(app)
                .focus_handle(app)
                .is_focused(window)
        }),
        "the search field has the focus"
    );
}

/// A row for the agent's badge: with no models there is no model chip, and the
/// badge, which names the agent, is the first row of a Minimal ⋯, as information:
/// the menu's model steps over it and will not run it, and the chat's keys do the
/// same.
#[gpui::test]
async fn a_badge_only_agent_shows_its_name_as_an_informational_row(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| {
        configure_full_row(chat);
        chat.available_models.clear();
        chat.set_agent_name("Codex");
    });
    resize_to(cx, 200.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Minimal);
    assert!(cx.debug_bounds("agent-badge").is_none(), "the badge left the row");
    let snapshot = chat.read_with(cx, |chat, _| chat.composer_snapshot());
    assert_eq!(snapshot.overflow[0], "agent");
    assert!(!snapshot.overflow.contains(&"model"));

    let rows = chat.read_with(cx, |chat, _| chat.overflow_rows());
    assert_eq!(rows[0], OverflowRow::Agent);
    let menu = chat.update(cx, |chat, cx| chat.overflow_menu(cx));
    let last = rows.len() - 1;
    assert_eq!(menu.step(Some(last), 1), Some(1), "down skips it");
    assert_eq!(menu.step(Some(1), -1), Some(last), "up skips it");
    assert!(
        !cx.update(|window, app| menu.run(0, window, app)),
        "the menu does not run the agent's name"
    );

    open_overflow(cx, &chat);
    drawn(cx, "overflow-agent");
    let menu = chat
        .read_with(cx, |chat, _| chat.composer_snapshot().menu)
        .expect("the menu is open");
    assert_eq!(menu[0].id, "agent");
    assert!(!menu[0].enabled);
    assert!(!chat.read_with(cx, |chat, _| {
        chat.picker_row_enabled(&PickerRow::Overflow(OverflowRow::Agent))
    }));
    let cursor = |cx: &mut VisualTestContext| chat.read_with(cx, |chat, _| chat.picker_cursor);
    assert_eq!(cursor(cx), Some(1), "the cursor starts on a row that can be chosen");
    cx.update(|window, app| {
        chat.update(app, |chat, cx| {
            chat.picker_previous(&PickerPrevious, window, cx)
        })
    });
    assert_eq!(cursor(cx), Some(last), "Up steps over it");
    chat.update(cx, |chat, _| chat.picker_cursor = Some(0));
    cx.update(|window, app| {
        chat.update(app, |chat, cx| {
            chat.picker_choose(&PickerChoose, window, cx)
        })
    });
    assert!(
        chat.read_with(cx, |chat, _| chat.overflow_open
            && !chat.model_picker_open
            && !chat.context_popover_open),
        "choosing the agent's name leaves ⋯ as it was"
    );
}

/// A five-level effort ladder, as the real Claude CLI offers (`low`, `medium`,
/// `high`, `xhigh`, `max`), with the highest the current one: five bars, every one
/// drawn: the widest a barred effort chip gets.
fn with_five_effort_levels(chat: &mut Chat) {
    chat.effort = Some(EffortOption {
        option_id: "effort".into(),
        name: Some("Effort".into()),
        current_value: Some("max".into()),
        choices: ["low", "medium", "high", "xhigh", "max"]
            .into_iter()
            .map(|level| EffortChoice {
                value: level.into(),
                name: level[..1].to_uppercase() + &level[1..],
            })
            .collect(),
    });
}

/// The user's goal: down to the 160px a split pane can be (`MIN_SPLIT_PANE_SIZE`)
/// every control is reachable, with the three-level effort of the fixture and with
/// the five levels the real CLI offers. At that width the row is Minimal, the
/// permission pill and the effort chip are whole in the chips' container, send is
/// inside the band and clear of them, and ⋯, alone in the left group, lists the
/// model, attach and every other row the wider layouts draw as chips.
#[gpui::test]
async fn every_control_is_reachable_at_the_pane_floor(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    for levels in [3, 5] {
        chat.update(cx, |chat, _| {
            configure_full_row(chat);
            if levels == 5 {
                with_five_effort_levels(chat);
            }
        });
        chat.update(cx, |chat, _| chat.overflow_open = false);
        resize_to(cx, 1000.0, 700.0);
        resize_to(cx, 160.0, 700.0);

        assert_eq!(
            row_layout_now(cx, &chat),
            RowLayout::Minimal,
            "{levels} levels"
        );
        if levels == 5 {
            assert_eq!(
                chat.read_with(cx, |chat, _| chat.effort_bar_position()),
                Some((5, 5)),
                "every one of five bars is drawn"
            );
        }
        let composer = drawn(cx, "composer");
        let send = drawn(cx, "send");
        let chips = drawn(cx, "composer-chips");
        for name in MINIMAL_CHIPS {
            let chip = drawn(cx, name);
            assert!(
                chip.left() >= chips.left() - px(0.5) && chip.right() <= chips.right() + px(0.5),
                "{name} is whole inside the chips' container with {levels} levels: \
                 {name}={chip:?} chips={chips:?}"
            );
        }
        assert!(
            send.left() >= composer.left() && send.right() <= composer.right(),
            "send is inside the band: send={send:?} composer={composer:?}"
        );
        assert!(
            chips.right() <= send.left() + px(0.5),
            "the chips end before send with {levels} levels: chips={chips:?} send={send:?}"
        );
        assert_one_line_inside_the_band(cx, &MINIMAL_CHIPS);
        assert!(
            cx.debug_bounds("attach-image").is_none(),
            "attach is a row of ⋯, not a button, in Minimal"
        );

        open_overflow(cx, &chat);
        for name in [
            "overflow-model",
            "overflow-attach",
            "overflow-fast",
            "overflow-thinking",
            "overflow-session-panel",
            "overflow-background-tasks",
            "overflow-context",
            "overflow-follow",
        ] {
            drawn(cx, name);
        }
    }
}

/// The narrowest whole-pixel chat width at which the Minimal row holds its chips
/// whole, found by walking down from just under the Narrow minimum, where the row
/// is Minimal. Leaves the chat at that width.
fn minimal_minimum(cx: &mut VisualTestContext, chat: &gpui::Entity<Chat>) -> u32 {
    let narrow = narrow_minimum(cx, chat);
    resize_to(cx, (narrow - 1) as f32, 700.0);
    assert_eq!(row_layout_now(cx, chat), RowLayout::Minimal);
    assert!(
        chips_fit_whole(cx, &MINIMAL_CHIPS),
        "the Minimal chips fit just under the Narrow minimum"
    );
    let mut minimum = narrow - 1;
    for width in (60..narrow - 1).rev() {
        resize_to(cx, width as f32, 700.0);
        assert_eq!(row_layout_now(cx, chat), RowLayout::Minimal, "at {width}px");
        if !chips_fit_whole(cx, &MINIMAL_CHIPS) {
            break;
        }
        minimum = width;
    }
    resize_to(cx, minimum as f32, 700.0);
    minimum
}

/// Below a measured width the Minimal row cannot hold its two chips whole, and the
/// effort, the last of them, is what is cut. Measured with the five-level effort of
/// the real CLI, the widest the chip gets, the real minimum with a mode and an
/// effort is 153px (7.8px a character, though no text is in the row, so
/// it is the icon's, bars' and paddings' own; with three levels it is 143px);
/// found by search, walking down from just under the Narrow minimum.
#[gpui::test]
async fn minimal_row_fits_from_its_measured_minimum(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| {
        configure_full_row(chat);
        with_five_effort_levels(chat);
    });
    let minimum = minimal_minimum(cx, &chat);
    let narrow = narrow_minimum(cx, &chat);
    for width in minimum..narrow {
        resize_to(cx, width as f32, 700.0);
        assert!(
            chips_fit_whole(cx, &MINIMAL_CHIPS),
            "every Minimal chip fits whole at {width}px"
        );
        assert_one_line_inside_the_band(cx, &MINIMAL_CHIPS);
    }
    resize_to(cx, (minimum - 1) as f32, 700.0);
    assert!(
        !chips_fit_whole(cx, &MINIMAL_CHIPS),
        "one pixel under, a chip is cut"
    );
    let chips = drawn(cx, "composer-chips");
    assert!(
        drawn(cx, "effort-chip").right() > chips.right() + px(0.5),
        "the effort, the last chip, is the one cut"
    );
    assert!(
        drawn(cx, "chat-status").right() <= chips.right() + px(0.5),
        "the pill is not cut: the cut falls on the right"
    );
}

/// The marked row follows its row when the layout crosses into Minimal with ⋯
/// open: the model's row is new at the front, every other row moves down one, and
/// Enter on the row marked Context still opens the context popover.
#[gpui::test]
async fn the_overflow_cursor_stays_on_its_row_when_the_layout_crosses_into_minimal(
    cx: &mut TestAppContext,
) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    let minimum = narrow_minimum(cx, &chat) as f32;
    open_overflow(cx, &chat);
    mark_overflow_row(cx, &chat, OverflowRow::Context);
    assert!(
        !chat
            .read_with(cx, |chat, _| chat.overflow_rows())
            .contains(&OverflowRow::Model),
        "Narrow has no model row"
    );

    resize_to(cx, minimum - 1.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Minimal);
    assert!(
        chat.read_with(cx, |chat, _| chat.overflow_open),
        "the layout change leaves ⋯ open"
    );
    assert_eq!(
        marked_overflow_row(cx, &chat),
        Some(OverflowRow::Context),
        "the mark stays on Context"
    );
    cx.update(|window, app| {
        chat.update(app, |chat, cx| {
            chat.picker_choose(&PickerChoose, window, cx)
        })
    });
    refresh_frame(cx);
    assert!(
        chat.read_with(cx, |chat, _| chat.context_popover_open),
        "Enter opened the context popover"
    );
}

/// Choosing the attach row in a Minimal ⋯ closes the menu and runs the action the
/// `+` button runs, `attach_image`: with the test seam holding a PNG's path, as the
/// button's own test does, the image becomes an attachment chip.
#[gpui::test]
async fn the_overflow_attach_row_runs_the_attach_action(cx: &mut TestAppContext) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    resize_to(cx, 160.0, 700.0);
    assert_eq!(row_layout_now(cx, &chat), RowLayout::Minimal);
    let dir = std::env::temp_dir().join(format!("sirio-attach-row-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let png = dir.join("photo.png");
    std::fs::write(&png, b"a png by extension only").expect("write png");
    chat.update(cx, |chat, _| chat.attach_test_paths = vec![png.clone()]);
    assert!(chat.read_with(cx, |chat, _| chat.attachments.is_empty()));

    open_overflow(cx, &chat);
    mark_overflow_row(cx, &chat, OverflowRow::Attach);
    cx.update(|window, app| {
        chat.update(app, |chat, cx| {
            chat.picker_choose(&PickerChoose, window, cx)
        })
    });
    for _ in 0..3 {
        refresh_frame(cx);
    }
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        cx.debug_bounds("composer-overflow-menu").is_none(),
        "choosing Attach closes ⋯"
    );
    assert_eq!(
        chat.read_with(cx, |chat, _| chat.attachments.len()),
        1,
        "the attach action ran: the picked image is an attachment"
    );
    drawn(cx, "attachment-chip-0");
}

/// An effort ladder as the native catalogue offers it for a model with every level:
/// the reset, the five levels, and Ultracode, which is offered where `xhigh` is.
fn with_the_native_effort_catalogue(chat: &mut Chat, current: &str) {
    let choice = |value: &str, name: &str| EffortChoice {
        value: value.into(),
        name: name.into(),
    };
    chat.effort = Some(EffortOption {
        option_id: "effort".into(),
        name: Some("Effort".into()),
        current_value: Some(current.into()),
        choices: vec![
            choice("default", "Default"),
            choice("low", "Low"),
            choice("medium", "Medium"),
            choice("high", "High"),
            choice("xhigh", "Xhigh"),
            choice("max", "Max"),
            choice("ultracode", "Ultracode"),
        ],
    });
}

/// In Minimal an effort with no bars of its own, the reset ("Default") and
/// Ultracode, draws the model's bars, not its word (the user's choice, 2026-10-31),
/// so that the chip is as narrow as a barred level's and 160px holds it whole. The
/// reset's bars are all unfilled and Ultracode's all filled; the level's name is
/// the tooltip's. Compact and Full keep the word.
#[gpui::test]
async fn minimal_draws_bars_for_the_reset_and_ultracode_and_fits_the_pane_floor(
    cx: &mut TestAppContext,
) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    for (current, selector) in [
        ("default", "effort-chip-bars-unfilled"),
        ("ultracode", "effort-chip-bars-filled"),
    ] {
        chat.update(cx, |chat, _| {
            configure_full_row(chat);
            with_the_native_effort_catalogue(chat, current);
            chat.overflow_open = false;
        });
        resize_to(cx, 1000.0, 700.0);
        resize_to(cx, 480.0, 700.0);
        assert_eq!(row_layout_now(cx, &chat), RowLayout::Compact, "{current}");
        drawn(cx, "model-effort-label");
        assert!(
            cx.debug_bounds("effort-chip-bars").is_none(),
            "Compact draws the word of {current}, not bars"
        );

        resize_to(cx, 160.0, 700.0);
        assert_eq!(row_layout_now(cx, &chat), RowLayout::Minimal, "{current}");
        assert!(
            cx.debug_bounds("model-effort-label").is_none(),
            "Minimal draws no word for {current}"
        );
        let bars = drawn(cx, selector);
        assert!(
            near(bars.size.width, px(23.0), 0.5),
            "all five of the model's bars are drawn for {current}: {bars:?}"
        );
        let chips = drawn(cx, "composer-chips");
        let send = drawn(cx, "send");
        for name in MINIMAL_CHIPS {
            let chip = drawn(cx, name);
            assert!(
                chip.right() <= chips.right() + px(0.5),
                "{name} is whole inside the chips' container for {current}: \
                 {name}={chip:?} chips={chips:?}"
            );
        }
        assert!(
            chips.right() <= send.left() + px(0.5),
            "the chips end before send for {current}: chips={chips:?} send={send:?}"
        );
        assert_one_line_inside_the_band(cx, &MINIMAL_CHIPS);
    }
}

/// The mark leaves with its row when the layout leaves Minimal with ⋯ open: the model
/// and attach rows are gone, so a mark on either moves to the nearest row that can be
/// chosen, and Enter runs that row, never the one that is gone (no model picker
/// opens, nothing is attached).
#[gpui::test]
async fn the_overflow_mark_leaves_with_the_model_and_attach_rows_when_the_chat_widens(
    cx: &mut TestAppContext,
) {
    let (chat, cx) = chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |chat| chat.client.is_some());
    chat.update(cx, |chat, _| configure_full_row(chat));
    let dir = std::env::temp_dir().join(format!("sirio-mark-leaves-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let png = dir.join("photo.png");
    std::fs::write(&png, b"a png by extension only").expect("write png");

    // Where the mark lands matters only if the place it held is not a row that can be
    // chosen: with Fast refused, a Narrow ⋯ opens on a disabled Fast row, where the
    // model's row stood; with neither Fast nor Thinking, on the disabled task count,
    // where attach's stood.
    for (gone, expected) in [
        (OverflowRow::Model, OverflowRow::Thinking),
        (OverflowRow::Attach, OverflowRow::Context),
    ] {
        chat.update(cx, |chat, _| {
            configure_full_row(chat);
            chat.overflow_open = false;
            chat.attach_test_paths = vec![png.clone()];
            if gone == OverflowRow::Model {
                chat.fast_mode = Some(FastMode {
                    enabled: false,
                    blocked_by: Some("not on this plan".into()),
                });
            } else {
                chat.fast_mode = None;
                chat.thinking_display = None;
            }
        });
        resize_to(cx, 1000.0, 700.0);
        resize_to(cx, 160.0, 700.0);
        assert_eq!(row_layout_now(cx, &chat), RowLayout::Minimal);
        open_overflow(cx, &chat);
        mark_overflow_row(cx, &chat, gone);

        resize_to(cx, 300.0, 700.0);
        assert_eq!(row_layout_now(cx, &chat), RowLayout::Narrow);
        assert!(
            chat.read_with(cx, |chat, _| chat.overflow_open),
            "the layout change leaves ⋯ open"
        );
        let rows = chat.read_with(cx, |chat, _| chat.overflow_rows());
        assert!(
            !rows.contains(&gone),
            "{gone:?} is not a row of a Narrow ⋯"
        );
        assert_eq!(
            marked_overflow_row(cx, &chat),
            Some(expected),
            "the mark moved to the nearest row that can be chosen ({gone:?} was marked)"
        );

        cx.update(|window, app| {
            chat.update(app, |chat, cx| {
                chat.picker_choose(&PickerChoose, window, cx)
            })
        });
        refresh_frame(cx);
        assert!(
            !chat.read_with(cx, |chat, _| chat.model_picker_open),
            "Enter did not run the model row that is gone ({gone:?} was marked)"
        );
        assert!(
            chat.read_with(cx, |chat, _| chat.attachments.is_empty()),
            "Enter did not run the attach row that is gone ({gone:?} was marked)"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
