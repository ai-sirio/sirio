//! Visual-bar conformance: the components' numbers against waku's frozen
//! measurements (`docs/linux-rewrite/03-visual-bar-and-gpui-patterns.md`
//! §A.2 and the density/type/radius scales in `PI-HANDOFF.md`).
//!
//! # What this proves — and what it does not
//!
//! **This suite proves the components use the frozen numbers, not that the
//! result looks right.** A layout can use every correct value and still be
//! ugly or wrong. Nothing here substitutes for putting the app next to
//! waku's frames when a display exists again (currently
//! `NOT EXERCISED — blocked on display`). A green run means "the measured
//! bar is still in the code", nothing more.
//!
//! It also cannot see everything: text sizes and radii that bypass the
//! theme tokens are invisible to these tests (there is no headless
//! renderer to measure glyphs with). Token-resolved surfaces are pinned by
//! the token assertions below; a fresh inline literal is a review finding,
//! not a test failure.
//!
//! # Do not measure glyphs from a test — the headless text system is a stub
//!
//! The line above says there is no headless renderer to measure glyphs
//! with. That is easy to disbelieve, because `#[gpui::test]` hands you a
//! `TestAppContext`, `window.text_system()` resolves, and
//! `.advance(font_id, size, ch)` returns `Ok`. It looks like a ruler. It is
//! not one. Measured 2026-08-19 under `#[gpui::test]`:
//!
//! ```text
//! family ".SystemUIFont"   n=7.80  m=7.80  i=7.80
//! family "Cantarell"       n=7.80  m=7.80  i=7.80
//! family "DejaVu Sans"     n=7.80  m=7.80  i=7.80
//! family "Liberation Sans" n=7.80  m=7.80  i=7.80   <- not installed at all
//! ```
//!
//! Every glyph, every family, including one that does not exist on the
//! machine, returns the same constant. A proportional font cannot have
//! `i` and `m` the same width; the headless text system is returning a
//! fixed advance rather than reading a face. So a test that "measures" a
//! column's characters-per-line through this API is asserting against a
//! constant, and will happily agree with any number you pick.
//!
//! The only honest ruler for type is a real frame: drive the app under
//! `Scripts/wayland-drive.sh` and measure ink extents in the screenshot.
//! Even then, note that ink extent and advance width are different
//! quantities — spaces carry advance and no ink, and glyphs carry side
//! bearings — so the two disagree by roughly a tenth on a short string, and
//! a derivation that needs advance cannot be fed an ink measurement without
//! reconciling that first.
//!
//! # Departures ledger — where Sirio deliberately leaves the bar
//!
//! A frozen reference is a starting point, not an authority over a
//! decision made for a good reason. These are the recorded ones:
//!
//! - **Tab bar 34px, toolbars 34px** (tab_bar, changes, right panel): waku
//!   has no tab strip and no toolbars — it is one conversation. Sirio's
//!   chrome rows are its own; no waku measurement exists for them.
//! - **Right panel header 40px, activity rows 48px, right panel 220–640px
//!   (default 405)**: Sirio-only surface (waku's right panel is a native
//!   webview, "not part of Sirio's UI"). No waku measurement exists. The
//!   width stopped being a frozen constant when the panel became
//!   user-resizable; 405 survives as the default, not as the geometry.
//! - **`caption2` is 13.0, off the measured scale**: waku's smallest
//!   step is 10.5, and Sirio first sized its dense strips there for fit
//!   — the status bar (three provider segments + worktree context), tab
//!   bar and toolbar labels. 10.5 read too small on those surfaces, so
//!   the token was raised — and the whole UI scale now reads at 12–13
//!   rather than waku's 11.5–12.5. Two consequences, both accepted:
//!   13.0 is not a waku step, and it puts `caption2` ABOVE `footnote`
//!   (12.0), sharing `callout`'s step — so the scale is no longer
//!   monotonic in the order its names imply. The name stays because
//!   every call site already spells it.
//! - **User pill text at body 13.5/21 instead of waku's 14/20**: Sirio
//!   keeps one body size; the pill is distinguished by its container
//!   (max 440, r12, raised, px14/py9 — all pinned below), not by a second
//!   reading size.
//! - **Plain-text file tabs have no column cap**: a code viewer, not
//!   prose; waku has no file view at all.
//! - **Settings cards sit on the 12px step** (`user_pill`): Sirio's own
//!   card design; 12 is in the measured set (menus, edit cards).
//! - **Chat popovers use the 10px toast step**: in the measured set, used
//!   for popover surfaces; not waku's menu radius (12).
//! - **Full circles are spelled as half the box** (6×6 dot at r3, 44×22
//!   swatch at r11, 20px toggle at r10): numerically identical to
//!   `rounded_full`; naming each would invent tokens for one value.
//! - **`DEFAULT_SIDEBAR_WIDTH` 325, sidebar 220–480px**: 325 is inside
//!   waku's resizable 180–420 range, and the reference freezes the range,
//!   not a default. Same course as the right panel above: the width stopped
//!   being a frozen constant when the sidebar became user-resizable, so 325
//!   survives as the default, not as the geometry. Sirio's range is the
//!   wider one because its rows carry a checkout path under the branch name.
//! - **Icons are sized by the iconography scale (9–16px), not the type
//!   scale**: glyph `text_size` calls are not type-scale members.
//! - **The top bar is 38px (`BrowserChrome::bar_height`), not the 48px
//!   `spacing.title_strip_height`** (P76): comet's own measured row, not
//!   waku's. `title_strip_height` is unchanged and still governs the other
//!   48px app bars; the top bar simply is not one of them anymore.
//!
//! # Drift fixed in P32 (was in the code, nobody had decided it)
//!
//! - Settings content column was **704**, now the frozen **720**.
//! - File-view markdown column was **800**, now the frozen **720**.
//! - Swift-scale text sizes (**13.0/12.0/11.0/10.0** from the macOS app's
//!   13pt base) still sat in settings/controls/sidebar/right panel/file
//!   view/tab bar; all moved to the measured steps
//!   (**13.5/12.5/11.5/10.5**) via the `Typography` tokens. The last of
//!   those steps has since been raised — see the departures ledger.
//! - File-view code text was 12pt (Swift's mono size), now the frozen
//!   code size **12** (`typography.code_size`).

use gpui::{FontWeight, Rgba, TestAppContext, WindowAppearance, px};
use sirio_theme::{Appearance, BaseColor, Theme, ThemeMode};

use crate::changes;
use crate::chat::{
    CARD_H_PADDING, CARD_V_PADDING, TRANSCRIPT_WIDTH, TURN_BOTTOM_PADDING, USER_PILL_H_PADDING,
    USER_PILL_MAX_WIDTH, USER_PILL_TEXT_SIZE, USER_PILL_V_PADDING,
};
use crate::file_view::MARKDOWN_COLUMN_WIDTH;
use crate::right_panel;
use crate::settings::CONTENT_WIDTH;
use crate::sidebar::{
    CARD_TWO_LINE_HEIGHT, ROW_GAP, ROW_HEIGHT, ROW_SUB_LINE_HEIGHT, ROW_TITLE_LINE_HEIGHT,
    ROW_V_PADDING,
};
use crate::status_bar;

/// Channel-wise compare against a `0xRRGGBB` opaque hex, tight epsilon.
fn expect_hex(actual: Rgba, hex: u32, label: &str) {
    let expected = (
        ((hex >> 16) & 0xFF) as f32 / 255.0,
        ((hex >> 8) & 0xFF) as f32 / 255.0,
        (hex & 0xFF) as f32 / 255.0,
    );
    for (channel, value, wanted) in [
        ("r", actual.r, expected.0),
        ("g", actual.g, expected.1),
        ("b", actual.b, expected.2),
    ] {
        assert!(
            (value - wanted).abs() < 0.004,
            "{label}.{channel}: {value} != {wanted}"
        );
    }
    assert_eq!(actual.a, 1.0, "{label}.a must be opaque");
}

#[gpui::test]
async fn install_into_bezel_makes_theme_of_return_the_branded_palette(cx: &mut TestAppContext) {
    let _guard = bezel::theme::lock_appearance();
    let theme = Theme::for_mode(ThemeMode::Dark, WindowAppearance::Dark, BaseColor::Slate);
    let expected = theme.to_bezel_theme();
    assert_ne!(expected.bg, bezel::theme::Theme::dark().bg);

    cx.update(|cx| theme.install_into_bezel(cx));
    cx.update(|cx| {
        let installed = bezel::theme::Theme::of(cx);
        assert_eq!(installed.bg, expected.bg);
    });
}

/// The coral is still Sirio's — `#E08B52` dark / `#AD581F` light — and the
/// active chrome is deliberately *not* it.
///
/// The chrome used to be an alias of the coral, and this test existed to stop
/// the two drifting apart. They are different on purpose now: colour is spent
/// on data and on attention, so focus and active chrome are spelled with
/// contrast and resolve to the text neutral instead. What this test guards is
/// therefore the separation — it is what catches a coral creeping back into
/// the shell.
///
/// The text neutral itself is no longer pinned here. It is bezel's, and
/// `sirio_theme`'s `dark_palette_comes_from_bezel` compares it against bezel
/// directly; a hex copy in a second crate would only be a place for the two to
/// disagree. The coral stays pinned because it is Sirio's own — see
/// `docs/THEME-PROVENANCE.md`.
#[test]
fn the_accent_is_the_pickers_coral_and_no_longer_the_chrome() {
    let dark = Theme::dark();
    assert_eq!(dark.appearance, Appearance::Dark);
    expect_hex(dark.brand_coral, 0xE0_8B_52, "dark coral");
    assert_ne!(dark.text, dark.brand_coral);

    let light = Theme::light();
    assert_eq!(light.appearance, Appearance::Light);
    expect_hex(light.brand_coral, 0xAD_58_1F, "light coral");
    assert_ne!(light.text, light.brand_coral);
}

/// The type scale: waku's measured steps with a uniform +1px. Body
/// 14.5/22, UI chrome 13/17, callout 14, code 13/19, and the heading
/// steps 21/18/16/15. These are the values the surfaces resolve through
/// `theme.typography`; a drift here is a drift everywhere.
///
/// The steps are waku's OFFSET, not waku's re-multiplied: a uniform +1
/// necessarily changes every ratio against the body, so the old
/// ×1.45/×1.28/… labels no longer describe what these are and have been
/// dropped rather than quietly recomputed.
///
/// `caption2` shares callout's step — above `footnote` — the raised,
/// still-deliberate off-scale size for dense strips. It is pinned here
/// all the same: a value nobody measured still has to be a value
/// somebody decided.
#[test]
fn type_scale_is_the_measured_one() {
    let typography = Theme::dark().typography;

    assert_eq!(typography.base_size, px(14.5), "body base 14.5");
    assert_eq!(typography.body_line_height, px(22.0), "body 14.5 @ 22");
    assert_eq!(typography.ui_size, px(13.0), "UI chrome 13.0");
    assert_eq!(typography.ui_line_height, px(17.0), "chrome 13 @ 17");
    assert_eq!(typography.callout, px(14.0), "callout 14.0");
    assert_eq!(
        typography.caption2,
        px(14.0),
        "caption2 14.0 — the deliberate off-scale step, above footnote"
    );
    assert_eq!(typography.headline, px(15.0), "headline 15.0");
    assert_eq!(typography.code_size, px(13.0), "code 13.0");
    assert_eq!(typography.code_line_height, px(19.0), "code 13 @ 19");
    assert_eq!(typography.code_weight, FontWeight::NORMAL);
    assert_eq!(typography.large_title, px(21.0), "h1, waku 20 + 1");
    assert_eq!(typography.title, px(18.0), "h2, waku 17 + 1");
    assert_eq!(typography.title2, px(16.0), "h3, waku 15 + 1");
    assert_eq!(typography.title3, px(15.0), "h4, waku 14 + 1");
}

/// Radii resolve through the token set, not fresh literals: the measured
/// steps 4/5/6/7/8/10/12/13 (§A.2), shared by both appearances.
#[test]
fn radii_come_from_the_measured_token_set() {
    let radii = Theme::dark().radii;
    assert_eq!(radii.chip, px(4.0), "chips-in-rows");
    assert_eq!(radii.chip_active, px(5.0), "activity chips, focus proxies");
    assert_eq!(radii.control, px(6.0), "default controls");
    assert_eq!(radii.row_card, px(7.0), "row cards");
    assert_eq!(radii.code_block, px(8.0), "code blocks, tool cards");
    assert_eq!(radii.toast, px(10.0), "toasts, prompt cards");
    assert_eq!(
        radii.user_pill,
        px(12.0),
        "user pills, menus, settings cards"
    );
    assert_eq!(radii.composer, px(13.0), "the composer card");

    assert_eq!(
        Theme::light().radii,
        radii,
        "radii are geometry, not appearance"
    );
}

/// App bars are 48px, the usage strip is 40px (waku's footer), compact sidebar
/// rows are 32px and every worktree card is 51px — the latter exactly
/// waku's session-card math (7 + 18 + 4 + 15 + 7). Changes rows sit on
/// the file-tree (30), hunk-header (24) and diff-line (20) heights from
/// the density scale.
#[test]
fn bars_and_rows_use_the_measured_density() {
    let theme = Theme::dark();
    let spacing = theme.spacing;
    let chrome = theme.browser_chrome;

    assert_eq!(spacing.title_strip_height, px(48.0), "48px app bars");
    assert_eq!(
        chrome.bar_height,
        px(38.0),
        "the top bar is comet's measured 38px row, not waku's 48px (P76)"
    );
    assert_eq!(
        spacing.compact_action,
        px(24.0),
        "cluster buttons are the 24px compact-action token"
    );
    assert_eq!(
        chrome.cluster_button_gap,
        px(2.0),
        "cluster button gap is comet's own CLUSTER_BUTTONS_WIDTH measurement"
    );
    assert_eq!(
        chrome.traffic_light_diameter,
        px(12.0),
        "traffic lights are 12px circles, independently derived from bar_height"
    );
    assert_eq!(
        chrome.traffic_light_gap,
        px(8.0),
        "12+8=20px pitch between lights"
    );
    assert_eq!(
        chrome.traffic_light_inset,
        px(10.0),
        "traffic-light inset reuses comet's own non-macOS cluster-start baseline, not macOS's 14px"
    );
    assert_eq!(
        chrome.cluster_start(),
        px(70.0),
        "cluster start is derived (10 + 3*12 + 2*8 + 8), between comet's own 10px (no lights) and 88px (macOS lights) boundary numbers, not copied from either"
    );

    assert_eq!(spacing.bottom_bar_height, px(40.0), "waku's 40px footer");
    assert_eq!(status_bar::HEIGHT, 40.0, "the usage strip is that footer");

    assert_eq!(ROW_HEIGHT, 32.0, "single-line sidebar rows 32px");
    assert_eq!(ROW_TITLE_LINE_HEIGHT, 18.0, "13.5px title line");
    assert_eq!(ROW_SUB_LINE_HEIGHT, 15.0, "11.5px context line");
    assert_eq!(ROW_V_PADDING, 7.0, "waku's py(7)");
    assert_eq!(ROW_GAP, 4.0, "card title/context gap");
    assert_eq!(
        CARD_TWO_LINE_HEIGHT,
        7.0 + 18.0 + 4.0 + 15.0 + 7.0,
        "two-line cards are waku's session-card math, not a guess"
    );

    assert_eq!(changes::ROW_HEIGHT, 30.0, "file-tree rows 30px");
    assert_eq!(changes::HUNK_ROW_HEIGHT, 24.0, "hunk headers 24px");
    assert_eq!(changes::DIFF_LINE_HEIGHT, 20.0, "diff lines 20px");
    assert_eq!(
        right_panel::ROW_HEIGHT,
        26.0,
        "file tree in the right panel too"
    );
}

/// Settings and the file-tab markdown column keep waku's frozen 720. The chat
/// transcript no longer does: it follows the Bezel Transcript pattern's 700
/// (`docs/superpowers/specs/2026-08-29-bezel-loading-design.md` §2).
#[test]
fn the_non_chat_content_columns_are_the_frozen_720() {
    assert_eq!(CONTENT_WIDTH, 720.0, "settings content column");
    assert_eq!(MARKDOWN_COLUMN_WIDTH, 720.0, "file-tab markdown column");
    assert_eq!(CARD_H_PADDING, 14.0, "composer card px(14)");
    assert_eq!(CARD_V_PADDING, 10.0, "composer card p(10)");
}

#[test]
fn the_live_transcript_follows_the_bezel_transcript_pattern() {
    assert_eq!(
        TRANSCRIPT_WIDTH, 700.0,
        "transcript column (Bezel Transcript)"
    );
    assert_eq!(USER_PILL_MAX_WIDTH, 440.0, "user bubble (Bezel Activity)");
    assert_eq!(USER_PILL_H_PADDING, 14.0, "user bubble horizontal padding");
    assert_eq!(USER_PILL_V_PADDING, 9.0, "user bubble vertical padding");
    assert_eq!(USER_PILL_TEXT_SIZE, 13.5, "user bubble text size");
    assert_eq!(TURN_BOTTOM_PADDING, 28.0, "whole-turn bottom padding");
}

/// The named waku components are the ones the conformance story hinges on;
/// their geometry is pinned above through the constants the components
/// themselves use, so a component that stops referencing the frozen value
/// fails here. The composer's 13px radius and the user pill's 12px radius
/// resolve through the tokens at their call sites (`theme.radii.*`),
/// which the radii test pins.
#[test]
fn named_components_keep_their_frozen_geometry() {
    // The composer card is max-720, r13, p10 (waku composer.rs metrics).
    assert_eq!(Theme::dark().radii.composer, px(13.0));
    assert_eq!(CARD_H_PADDING, 14.0);
    assert_eq!(CARD_V_PADDING, 10.0);

    // The user pill is the Bezel 440, r12, px14/py9, 13.5px composition.
    assert_eq!(Theme::dark().radii.user_pill, px(12.0));
    assert_eq!(USER_PILL_MAX_WIDTH, 440.0);
    assert_eq!(USER_PILL_H_PADDING, 14.0);
    assert_eq!(USER_PILL_V_PADDING, 9.0);
    assert_eq!(USER_PILL_TEXT_SIZE, 13.5);
}
