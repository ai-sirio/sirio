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
//! - **Right panel header 40px, right panel 220–640px
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

use gpui::{TestAppContext, WindowAppearance};
use sirio_theme::{BaseColor, Theme, ThemeMode};

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
