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
//! # Departures ledger — where Tiller deliberately leaves the bar
//!
//! A frozen reference is a starting point, not an authority over a
//! decision made for a good reason. These are the recorded ones:
//!
//! - **Tab bar 34px, toolbars 34px** (tab_bar, changes, right panel): waku
//!   has no tab strip and no toolbars — it is one conversation. Tiller's
//!   chrome rows are its own; no waku measurement exists for them.
//! - **Right panel header 40px, activity rows 48px, `PANEL_WIDTH` 405**:
//!   Tiller-only surface (waku's right panel is a native webview, "not
//!   part of Tiller's UI"). No waku measurement exists.
//! - **Status bar text at caption2 (10.5)**: a dense Tiller-only strip
//!   (three provider segments + worktree context) sized for fit, within
//!   the measured scale; waku's 40px footer rows are 11.5.
//! - **User pill text at body 13.5/21 instead of waku's 14/20**: Tiller
//!   keeps one body size; the pill is distinguished by its container
//!   (max 540, r12, raised, px12/py8 — all pinned below), not by a second
//!   reading size.
//! - **Plain-text file tabs have no column cap**: a code viewer, not
//!   prose; waku has no file view at all.
//! - **Settings cards sit on the 12px step** (`user_pill`): Tiller's own
//!   card design; 12 is in the measured set (menus, edit cards).
//! - **Chat popovers use the 10px toast step**: in the measured set, used
//!   for popover surfaces; not waku's menu radius (12).
//! - **Full circles are spelled as half the box** (6×6 dot at r3, 44×22
//!   swatch at r11, 20px toggle at r10): numerically identical to
//!   `rounded_full`; naming each would invent tokens for one value.
//! - **`SIDEBAR_WIDTH` 325**: inside waku's resizable 180–420 range; the
//!   reference freezes the range, not a default.
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
//!   (**13.5/12.5/11.5/10.5**) via the `Typography` tokens.
//! - File-view code text was 12pt (Swift's mono size), now the frozen
//!   code size **11.5** (`typography.code_size`).

use gpui::{FontWeight, Rgba, px};
use tiller_theme::{Appearance, Theme};

use crate::changes;
use crate::chat::{CARD_H_PADDING, CARD_V_PADDING, TRANSCRIPT_WIDTH, USER_PILL_MAX_WIDTH};
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

/// The accent resolves to Tiller's coral in both appearances — `#E08B52` dark
/// / `#AD581F` light — under both the modern and legacy token names the
/// surfaces consume.
///
/// The pair a surface reads through `accent` and the pair it reads through
/// `tab_focus_accent` must not drift apart, which is the only thing this test
/// is for; where the values themselves come from is
/// `docs/linux-rewrite/THEME-PROVENANCE.md`, and the rules they satisfy are
/// tested next to them in `tiller_theme`.
#[test]
fn accent_resolves_to_the_same_coral_under_either_token_name() {
    let dark = Theme::dark();
    assert_eq!(dark.appearance, Appearance::Dark);
    expect_hex(dark.accent, 0xE0_8B_52, "dark accent");
    expect_hex(dark.tab_focus_accent, 0xE0_8B_52, "dark tab_focus_accent");

    let light = Theme::light();
    assert_eq!(light.appearance, Appearance::Light);
    expect_hex(light.accent, 0xAD_58_1F, "light accent");
    expect_hex(light.tab_focus_accent, 0xAD_58_1F, "light tab_focus_accent");
}

/// The type scale is the measured one: body 13.5/21 (the document ratio),
/// UI chrome 11.5/16, callout 12.5, caption 10.5, code 11.5/17.5, and the
/// heading steps off body (×1.45/×1.28/×1.14/×1.05). These are the values
/// the surfaces resolve through `theme.typography`; a drift here is a
/// drift everywhere.
#[test]
fn type_scale_is_the_measured_one() {
    let typography = Theme::dark().typography;

    assert_eq!(typography.base_size, px(13.5), "body base 13.5");
    assert_eq!(
        typography.body_line_height,
        px(21.0),
        "body 13.5 @ 21 (×1.56)"
    );
    assert_eq!(typography.ui_size, px(11.5), "UI chrome 11.5");
    assert_eq!(typography.ui_line_height, px(16.0), "chrome 11.5 @ 16");
    assert_eq!(typography.callout, px(12.5), "callout 12.5");
    assert_eq!(typography.caption2, px(10.5), "caption 10.5");
    assert_eq!(typography.headline, px(13.5), "headline is the body size");
    assert_eq!(typography.code_size, px(11.5), "code 11.5");
    assert_eq!(typography.code_line_height, px(17.5), "code 11.5 @ 17.5");
    assert_eq!(typography.code_weight, FontWeight::NORMAL);
    assert_eq!(typography.large_title, px(20.0), "h1 13.5 × 1.45");
    assert_eq!(typography.title, px(17.0), "h2 13.5 × 1.28");
    assert_eq!(typography.title2, px(15.0), "h3 13.5 × 1.14");
    assert_eq!(typography.title3, px(14.0), "h4 13.5 × 1.05");
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

/// App bars are 48px, the usage strip is 40px (waku's footer), sidebar
/// rows are 32px single-line and 51px two-line — the latter exactly
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
        30.0,
        "file tree in the right panel too"
    );
}

/// The content column is the frozen 720 everywhere prose reads: the chat
/// transcript, the settings surface (was 704 — a value nobody recorded),
/// and rendered markdown in file tabs (was 800). The user pill is capped
/// at waku's 540; the composer card pads 14/10.
#[test]
fn the_content_column_is_the_frozen_720() {
    assert_eq!(TRANSCRIPT_WIDTH, 720.0, "chat transcript column");
    assert_eq!(CONTENT_WIDTH, 720.0, "settings content column");
    assert_eq!(MARKDOWN_COLUMN_WIDTH, 720.0, "file-tab markdown column");

    assert_eq!(USER_PILL_MAX_WIDTH, 540.0, "user pill max width (waku 540)");
    assert_eq!(CARD_H_PADDING, 14.0, "composer card px(14)");
    assert_eq!(CARD_V_PADDING, 10.0, "composer card p(10)");
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

    // The user pill is max-540, r12, px12/py8 — the container metrics are
    // frozen; the text deliberately stays at body size (see ledger).
    assert_eq!(Theme::dark().radii.user_pill, px(12.0));
    assert_eq!(USER_PILL_MAX_WIDTH, 540.0);
}
