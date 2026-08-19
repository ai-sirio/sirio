# 03 — Visual bar (waku) and GPUI implementation patterns

**Phase 1 reference freeze.** Sources read for this document (all READ-ONLY):

- `waku` checkout @ `9c21576` ("Share one resident opencode serve...") — primary visual reference. ~85.5K lines of Rust (143 files), GPL-3.0. Builds with a **fork of zed's GPUI** (`egoist/zed`, branch `waku-webview`, = upstream main + PR #61945 "layered scene rendering", which composites menus/tooltips above native child views — see `waku/Cargo.toml`). waku is macOS-first; its non-macOS code is `#[cfg(not(target_os = "macos"))]` no-ops. Its visual language is the target; its macOS-only chrome (vibrancy, traffic lights, native menus) must be re-derived for Linux per Part C.
- `zed` checkout @ `c05e346` (upstream main, ~Nov 2025) — `crates/gpui`, `crates/gpui_platform`, `crates/gpui_linux`, `crates/theme`. This is the API surface Tiller will build against (the waku fork only adds the layered-scene patch; everything cited below exists in this checkout).

> **Correction, 2026-08-19.** This document used to open by claiming "nothing
> in this document is copied code". That was not true as written, and the
> sentence itself pointed at why: the numbers below were read out of the
> checkouts' **source files** — this document cites them by file and line — not
> measured off rendered frames. For a palette, a table of values *is* the code;
> transcribing `waku/src/theme.rs` into prose and then into Rust is a transplant
> with an extra step. See `GAP-transplanted-theme-tokens.md` for the finding and
> `THEME-PROVENANCE.md` for the repair.
>
> What holds: the **mechanism and pattern** notes here are descriptions of how
> something works, written from reading, and those are inspiration in the
> ordinary sense. What does not: the §A.2 colour tables, which have been
> superseded. Tiller's palette now comes from `reference/waku/measure-theme.py`
> sampling waku's published *screenshots*, with every value's origin recorded —
> including the ones the frames turn out to be unable to settle, which are ours
> by choice and say so. Where a value in §A.2 disagrees with
> `THEME-PROVENANCE.md`, the provenance document wins.

---

# Part A — The visual bar (from waku)

## A.1 Asset inventory

### Raster images (9 files)

| Path (in waku checkout) | What it is |
|---|---|
| `resources/AppIcon.icns` (1.4M) | macOS app icon bundle (shipped build) |
| `resources/AppIconDev.icns` (1.3M) | macOS app icon bundle (dev build) |
| `resources/computer-use/menubar-cursor.png` (672B) | Small cursor glyph used by the computer-use overlay |
| `website/public/favicon.png` (1.8K), `apple-touch-icon.png` (35K), `og-icon.png` (231K), `app-icon.png` (63K) | Website/web assets derived from the app icon |
| `website/public/app-screenshot-light.png` (639K, 2266×1752), `app-screenshot-dark.png` (605K, 2266×1752) | Marketing screenshots of the app in light and dark mode. These are the only photographic evidence of the visual system; the palette below is extracted from source, not from these |

### SVGs (180 files)

- **`assets/icons/` — 72 monochrome UI icons** (16×16 viewBox class, ~200-800 B each, mostly single-path, stroke/fill via currentColor). They are the *only* icon vocabulary in the app. Full list: alert, appearance, arrow-down, arrow-left, arrow-right, arrow-up, arrow-up-right, block, bot, case-sensitive, chart-column, check, changes, chevron-down, chevron-right, chevron-up, chevrons-up-down, cloud-upload, compose, copy, cursor-spark, download, external-link, file, folder, folder-new, file-diff, fork, gauge, git-branch, git-commit-horizontal, github, globe, hexagon, info, laptop, list, loader-circle, lock, lock-open, package, panel-left, panel-right, pencil, plus, regex, replace, replace-all, rewind, rotate-cw, search, settings, slash, sparkle, star, star-filled, stop, stop-filled, terminal, terminal-square, trash, whole-word, wrench, x, zap. Plus **7 provider marks**: provider-amp (red "&"), provider-claude (Anthropic starburst), provider-cursor, provider-grok, provider-openai, provider-opencode, provider-pi.
- **`assets/icons/file-types/` — 100 polychrome file-type icons** (ruby, rust, python, react, typescript, javascript, go, json, yaml, docker, kubernetes, git, markdown, html, css, etc.). `file-types/SOURCE.md` states they are a *selected subset of the Material Icon Theme* (MIT, upstream: PKief/vscode-material-icon-theme), shipped only for extensions waku actually maps. **License note for Tiller: reuse of these icons is MIT-licensed via Material Icon Theme; copying them is allowed, unlike app code.**
- `website/public/providers/` — 7 provider SVG logos (same marks as the in-app provider icons, used by the website).
- `resources/computer-use/overlay-cursor.svg` — cursor image for the computer-use overlay.

**How assets are embedded** (`waku/src/assets.rs`): every icon is `include_bytes!`'d into a static table `(&str path, &[u8])`; `Assets` implements `gpui::AssetSource { load(path), list(path) }`. A unit test (`src/ui/mod.rs` tests) asserts every icon path referenced anywhere in the app resolves — the app fails its test suite if a renderer references a missing icon. Fonts: `JetBrainsMono-Regular/Bold/Italic/BoldItalic.ttf` embedded and registered via `cx.text_system().add_fonts(...)` (`assets.rs:180-206`); `SymbolsNerdFontMono-Regular.ttf` is registered *only on macOS* via CoreText for glyph fallback.

## A.2 The visual system, precisely

### Positioning statement (from `src/theme.rs:16-20`, verbatim intent)

> "Waku's visual language, take two: neutral graphite surfaces in the spirit of Cursor — **color is reserved for meaning**. ... Selected, hovered, and pressed rows remain a 6% neutral layer."

That is the whole design contract: a graphite shell, an accent used only for brand moments, semantic hues for status, and every interactive row state expressed as a neutral alpha wash rather than a color.

### Layout skeleton (root render, `src/app/render.rs:129-189`)

One window, one root entity. The root is a horizontal flex:

```
┌────────────────────────────────────────────────────────────────┐
│ sidebar (fixed px width, user-resizable)  │ main column (flex_1)│
│ ┌─ titlebar 48px (drag regions, toggle,  │ ┌─ header 48px      │
│ │  history buttons)                      │ │  (drag region,    │
│ ├─ new-session row 32px (+10 gap)        │ │   title, actions) │
│ ├─ virtualized session list (flex_1)     │ ├─ transcript or    │
│ │  group headers 22px, cards 51+1px      │ │  empty state      │
│ │  overlay scrollbar                     │ │  (flex_1, virtual-│
│ ├─ footer 40px (settings, updater pill)  │ │  ized, max_w 720) │
│ │                                        │ ├─ composer card    │
│ │                                        │ │  (max_w 720,      │
│ │                                        │ │  rounded 13, p10) │
│ │                                        │ └─ footer strip     │
├────────────────────────────────────────────────────────────────┤
│ overlays (float above everything): command palette, commit     │
│ dialog, image preview, toast layer (top-centered), permission  │
│ prompt, computer-use overlay, right panel (native webview)     │
└────────────────────────────────────────────────────────────────┘
```

- Root div: `key_context("Waku")`, ~20 `on_action` handlers, `size_full().relative().flex()`, `text_color(theme.text)`, `font_family(".SystemUIFont")` (render.rs:145-184).
- Sidebar shown conditionally; main column is `flex_1().h_full().min_w_0().flex().flex_col().bg(theme.surface)` with `border_l_1().border_color(theme.sidebar_border)` when the sidebar is visible (render.rs:165-172). Main column children in order: header, transcript-or-empty-state, permission prompt, queued messages, composer, workspace footer.
- Right panel (a WKWebView browser surface on macOS — **not part of Tiller's UI**; Tiller's equivalent panels are plain GPUI) is a sibling after the main column, `flex_none`, fixed width.
- Settings is a whole-window takeover (`render.rs:88-98`): when `settings_page.is_some()`, the root renders only settings (its own sidebar + content) plus palette/dialog overlays.
- **Chat content is centered**: every transcript row is `px(20)` + `justify_center` wrapping a `max_w(720)` column (`src/app/transcript_view.rs:1049-1060`); the composer is the same `px(20)` + `max_w(720).mx_auto()` (`src/app/composer.rs:1788-1793`).
- Column constants (`src/app.rs:75-103`): `CONTENT_MAX_WIDTH 720`, sidebar min/max 180/420, right panel min/max 280/1000, file tree 140-360 (default 184), nav rail 44, `FOLLOWUP_TURN_TOP_GAP 48`.

### Spacing scale

No named spacing tokens — **raw pixel constants, and the de-facto scale is 2/4/6/8/10/12/14/20**. Verified usages:

- 2: history-button gap (sidebar.rs:390), activity-row gap (transcript_view.rs:1498)
- 4: sidebar row padding-x, group gap, composer footer gap, tooltip offset, session-card column gap (composer.rs:1891)
- 5: sidebar group gap, activity chip gap
- 6: chip gaps, titlebar row gaps, header gap-8 primary (sidebar.rs:368, 383)
- 7: chip horizontal padding, toast padding-y
- 8: session card padding-x, message bubble padding, header gap (sidebar.rs:881; components.rs:548)
- 10: sidebar page padding-x, composer card padding, code-block padding-x, toast padding-x, list gap-10 headers, settings nav gap (composer.rs:1793; md/render.rs:1170)
- 12: user-bubble padding-x, message edit padding
- 14: header pl/pr, button padding-x (sidebar.rs:1200)
- 20: outer page margins px(20), empty-state CTA top margin

Padding inside cards: `p(10)` on the composer card; `px(12).py(8)` on user bubbles; `px(8).py(7)` on sidebar session cards; `px(10).py(8)` on code blocks. Row separation in the virtualized list: `py(8)` per transcript row, first row `pt(22)`, last `pb(22)`, follow-up turns get `pt(48)` (transcript_view.rs:1046-1062).

### Corner radii (de-facto scale: 4/6/7/8/10/12/13, plus full)

| Radius | Used for |
|---|---|
| 4 | group-header toggles, code wash, rename field, nav-rail hover chips |
| 5 | activity chips, find-bar match chips, focus ring proxies |
| 6 | **default control radius**: icon buttons, chips, text fields, tooltips, image corners, code-wash quads |
| 7 | sidebar action rows, session cards, settings nav rows, secondary buttons, changed-files rows |
| 8 | code blocks, tables, settings nav, activity disclosure blocks |
| 10 | toast card |
| 12 | user message bubbles, edit card |
| 13 | composer card |
| `rounded_full` | primary CTA, send/stop button, scroll-to-bottom pill, updater pill, meters, FPS dot, avatars |

### Colour roles — full palette (`src/theme.rs:98-185`, both modes)

All values verbatim. `rgb(0x...)` = opaque; `hsla(h,s,l,a)` as written. Dark first, light second.

**Surfaces** — graphite ramp:
- `canvas` (settings background): `#1A1A1A` / `#F6F5F6`
- `sidebar`: `transparent_black()` **in both modes** (macOS: vibrancy shows through; Linux must decide — see C.4)
- `sidebar_drag_background` (while resizing): `#181818` / `#F3F3F3`
- `sidebar_item_background` (selected/hover rows): `hsla(0,0,0.941,0.06)` (6% white) / `hsla(0,0,0.078,0.06)` (6% black)
- `surface` (main column): `#1A1A1A` / `#F6F5F6`
- `raised` (cards, bubbles, tooltips, toasts): `#232323` / `#ECECEC`
- `composer` (input card): `#212121` / `#FFFFFF`
- `inset` (code blocks, text fields): `#151515` / `#E6E6E6`
- `terminal`: `#151515` (paper-white `#FFFFFF` in light)
- `overlay` / `overlay_strong` (hover/pressed wash): `hsla(220,10%,90%,0.05)` / `0.09`; light `hsla(220,10%,12%,0.05)` / `0.09`

**Borders**: `border` `hsla(220,10%,90%,0.07)` / light `0.08`; `border_strong` `0.14` / `0.15`; `sidebar_border` `hsla(126.93, ~0, 0.16077, 1.0)` (near-black `#292929`; light `hsla(0,0,0.078,0.12)`).

**Text ramp** (4 steps): `text #E2E2E2` / `#242424`; `text_secondary #A3A3A3` / `#666666`; `text_tertiary #7D7D7D` / `#858585`; `text_ghost #575757` / `#A4A4A4`. Ghost is used for disabled/hints; tertiary for metadata.

**Accent & functional**:
- `accent` (brand coral — logo, caret, live-activity pulses, focus rings, "nothing structural"): `#E2795B` / `#C85F44`
- `resize_handle` and `gauge` (quota meters): `#3B82F6` / `#2563EB` (blue)
- `selection` (text-selection wash, deliberately browser-blue not brand): `hsla(211,100%,50%,0.55)` / `0.35`, painted under glyphs
- `code_text` (inline code): `#E0A882` / `#9A5528`; `code_wash` `hsla(220,10%,90%,0.08)` / `0.07`
- `inverse` / `on_inverse` (primary buttons — light fill, dark glyph): `#E7E9EC`/`#17181C`; light `#202227`/`#F8F8F9`
- `warning #E0B36A`/`#A66B20`; `success #62C987`/`#2F8F52`; `favorite #EAB308`/`#CA8A04` (yellow star); `danger #E2726A`/`#C64A42`; `danger_soft hsla(4,55%,63%,0.10)`/`hsla(4,55%,52%,0.10)` (stop-button hover)

**Semantic mapping** (`src/ui/mod.rs:75-83`): session status → idle = `text_ghost`, connecting/working = `accent`, waiting = `warning`, failed = `danger`. Provider brand colors (`ui/mod.rs:48-64`): Amp `#F34E3F`, Claude `#D97757`, Codex/Cursor/OpenCode/Grok/Pi = graphite `#F3F3F3` (dark) / `#34363B` (light) — non-branded CLIs deliberately do not get colors.

**Markdown token colors** (`src/md/render.rs:150-178`) — deliberately restrained: keyword `#C98BC0`/`#9A4B92`, literal+number `#D9A05B`/`#9A6019`, string `#94C08A`/`#3F7A36`, type+function `#8FB8D9`/`#2F6690`, comment = `text_ghost`. "three hues plus muted comments, so a code block still reads as part of a graphite transcript."

### Typography scale

- **System sans everywhere**: `".SystemUIFont"` is the app-wide family (root div, render.rs:153). No embedded text font. UI sizes are all small:
  - 9-9.5px: chips/badges (activity counter chips, status mini-labels, plan-meter labels) — SEMIBOLD/MEDIUM
  - 10-10.5px: code-block language labels, find-bar labels, FPS counter, "Esc" hint
  - 11-11.5px: **default UI text** — chips, buttons, footer rows, tooltips, group headers, toasts, activity rows
  - 12-12.5px: secondary CTAs, empty-state description, settings rows
  - 13-13.5px: **body text** — sidebar session titles, header title, transcript body, fields
  - 14px: user message bubble text
  - 20px: empty-state headline (MEDIUM)
- Line heights: transcript body 13.5/21; UI rows 11.5/14-16; compact (toasts) 11.5/17.
- **Markdown body metrics** (`src/md/render.rs:61-99`): BODY = 13.5px/21px, code 11.5px/17.5px; COMPACT = 11.5/17, code 10.5/16. Headings scale off body: h1 ×1.45 BOLD, h2 ×1.28 BOLD, h3 ×1.14 SEMIBOLD, h4 ×1.05 SEMIBOLD; line-height = size ×1.42 (render.rs:102-109).
- **Mono**: `"JetBrains Mono"` (embedded, 4 faces) for code blocks, inline code, FPS counter.
- Weights: NORMAL default; SEMIBOLD for labels/buttons/headings h3+; MEDIUM for titles, chips, code labels, nav labels; BOLD only inside markdown headings.

### Iconography

- All UI icons are **monochrome, 13-16px** (14px typical in headers, 11-12px inline with 13px text, 9-10px carets), rendered through GPUI's `svg()` element which treats an SVG as an **alpha mask tinted by `text_color`** (`ui/mod.rs:20-26`) — so one icon file serves every hue. Tinting follows text roles: `text_tertiary` idle, `text_ghost` disabled.
- **Polychrome icons** (file types, provider marks) go through `img()` (`ui/mod.rs:38-43`) — GPUI's `img` element preserves authored colors. The two paths are deliberate and non-interchangeable.
- Icon buttons: 22-26px hit targets (`size(22)` ghost buttons, `size(26)` in headers/footers), radius 6, icon 13-14px, hover `overlay` / active `overlay_strong` (ui/mod.rs:32-45).
- Icons denote *activity kinds* in the transcript (sparkle=reasoning, terminal=command, pencil=file edit, search=search, folder=file list, list=plan, wrench=tool, file=file read) (`ui/mod.rs:85-99`).

### Density & component inventory

The app is **dense**: 11.5px UI text, 22-26px controls, 32px action rows, 51px session cards, 48px bars. Components with exact metrics (all verified in source):

- **Icon button** 22×22, r6, icon 13 (ui/mod.rs:32)
- **Chip (menu trigger)** 24px tall (outlined 30), px7 (10), r6 (7), gap6, text 11.5/14, icon 10.5, caret 9; selected = `overlay` fill; disabled = opacity 0.7 (ui/mod.rs:139-215)
- **Text field** 28px, px8, r6, border `border_strong`→`accent` when focused, bg `inset`, text 11.5/16, optional leading icon (ui/text_field.rs:63-86)
- **Action row (sidebar)** 32px, px4, r7, gap10, icon 16 in 20px box, label 13px secondary (sidebar.rs:456-490)
- **Session card** 51px (+1px gap = 52px rows), px8/py7, r7, gap4; title 13.5 (clamp 1), status spinner 12px (900ms linear rotation), project row 11.5/15 with 11px folder icon, relative time right-aligned (sidebar.rs:874-980)
- **Primary CTA** 32px, px14, r-full, bg `inverse`/text `on_inverse`, 12.5 SEMIBOLD, hover opacity 0.9, active 0.8 (sidebar.rs:1307-1331)
- **Secondary CTA** 30px, px12, r-full, ghost text, hover `overlay` (sidebar.rs:1333-1351)
- **Small buttons** (edit card): 26px, px10-11, r7; cancel = border+`overlay` bg; send = `inverse` when enabled, `overlay_strong` + ghost text when not (components.rs:520-585)
- **Composer card** max_w 720, r13, border, bg `composer`, p10; input min_h 24, 13.5/22; footer row mt8 gap4 text 11.5/14 with provider chip, model traits, access control, spacer, and the send control (composer.rs:1788-1937)
- **Send/stop control** 26px r-full; idle/stop: bg `overlay_strong`, hover `danger_soft` when stopping; "Esc" hint inside the circle (10px SEMIBOLD) when escape-stop is armed; preparing: 15px spinner on `overlay_strong` (composer.rs:1899-1975)
- **Header** 48px, gap8, pl/pr14, session title 13px MEDIUM centered-left, right side: background-work summary, optional FPS counter, right-panel toggle (sidebar.rs:1190-1278)
- **Toast** top-centered at `top(56)` (below the 48px header), max_w 560, px10/py7, r10, border_strong, bg `raised`, `shadow_lg`, 11.5/16, status icon 14 (alert/check), dismiss 26px; enters with 150ms `ease_out_quint` (top 48→56, opacity 0.4→1) (render.rs:213-313)
- **Tooltip** px7/py4, r6, border_strong, bg `raised`, `shadow_md`, 11/15, text_secondary (ui/tooltip.rs:44-63)
- **Overlay scrollbar** track 11px, thumb 5px→8px hovered, min 28px, inset 2, hold 900ms + fade 350ms (ui/scrollbar.rs:16-22)
- **Quota meter** 3px tall r-full track `overlay_strong`, fill `gauge`; ≥80% warning, ≥95% danger; ≥1% keeps a 1.5% sliver (usage_meter.rs:584-610)
- **Updater pill** 20px r-full bg `gauge`, expands 20→58px in 150ms `ease_out_quint` (sidebar.rs:545-690)
- **Turn fold** "Worked for Ns" — 24px row, 1px hairline `border` lines on both sides, label 11.5/16 MEDIUM tertiary, 10px chevron (transcript_view.rs:1351-1402)
- **Scroll-to-bottom pill** 32px r-full, border_strong, bg `composer`, `shadow_xs`, bottom(8), arrow-down 16px (transcript_view.rs:149-181)
- **Navigation rail** 44px column at transcript's left edge; 32×2px ticks per turn, gap 10, inactive opacity 0.45, hover expands to 36px-wide chip r4; shows turn positions, active turn highlight, click = jump (transcript_view.rs:341-500; app.rs:95-103)
- **Code block** r8, border, bg `inset`, overflow hidden; 24px language header (10px MEDIUM ghost, border-b); code px10/py8, nowrap, horizontal scroll (md/render.rs:1104-1180)
- **User bubble** max_w 540, r12, bg `raised`, px12/py8, text 14/20; right-aligned (components.rs:497-560)
- **Empty state** centered column: accent sparkle 24px, headline 20px MEDIUM, description 12.5/19 tertiary max_w 380, CTA pair (primary + secondary), `pb(46-52)` (sidebar.rs:1284-1405)

### Chrome: present vs deliberately absent

**Present**: 48px custom header/sidebar titlebar (invisible on macOS behind traffic lights — the window is `appears_transparent` + `is_movable: false`, waku draws all titlebar gestures itself, main.rs:319-336); toasts; tooltips; floating pills; command palette overlay; native menu bar on macOS via `cx.set_menus` (main.rs:344+); context menus on right-click and Shift+F10 (sidebar.rs:952-956); panel resize handles (2px blue strip, 10px hit area straddling the edge; sidebar.rs/render.rs:4-21).

**Deliberately absent**: no tabs, no breadcrumbs, no toolbar buttons in the chat column besides the header trio; no window chrome of its own (relies on the OS); no borders around the main column except the 1px sidebar separator; no card chrome around markdown content (only code blocks/tables get containers); no scrollbar until you scroll; no animations except functional ones (spinners, pill expansion, toast enter, meter pulse, nav-rail tick transition, scrollbar fade); the FPS counter is a hidden debug affordance (default off, `cmd-alt-shift-f`).

### Motion & interaction feel

- **Spinners**: `loader-circle.svg` rotating with `Animation::new(900ms).repeat().with_easing(linear)` via `Transformation::rotate(percentage(delta))` (sidebar.rs:604-615, 909-919).
- **Micro-animations**: toast enter 150ms `ease_out_quint`; updater pill expand 150ms `ease_out_quint`; nav-rail tick width transition 300ms; plan-usage skeleton pulse `pulsating_between(0.45, 0.9)` at 1400ms; working-wave dots.
- **Reduce motion is honored**: waku sets `cx.set_reduce_motion(platform::reduce_motion_enabled())` at startup (main.rs:294); GPUI's animation path resolves instantly under reduce-motion (render.rs:305 comment).
- **Hover/press contract**: hover = `overlay` (5% white), pressed = `overlay_strong` (9%); primary buttons hover = opacity 0.9, active = 0.8; rows (sidebar items) use `sidebar_item_background` for hover/selected/active alike — no color, only the 6% wash.
- **Focus contract**: every keyboard-operable element gets `tab_index(0)` + `focus_visible` accent border + `cursor_default`; Enter/Space activate (rows, buttons, toggles); Shift+F10 opens context menus; Escape cancels/backs out.
- **Text selection** is the familiar browser blue (`selection`), painted *under* glyphs; selections are click-drag anywhere in the transcript, with copy via Cmd/Ctrl-C fallback from the composer (transcript_view.rs:283-325).
- **Streaming presentation**: presentation pacing of ~24ms frame interval while chunks stream (app.rs:100-101), grapheme-batched (12-256/frame) with tail remeasure (app.rs:112-115).
- **Scroll behavior**: transcript is anchor-following — sticks to bottom during streaming unless the user scrolls up; a "scroll to bottom" pill appears then; the overlay scrollbar reveals on movement, holds 900ms, fades 350ms, widens under the pointer (transcript_view.rs:117-148; ui/scrollbar.rs).

---

# Part B — GPUI implementation patterns

How a GPUI app of this kind is actually built. Citations are zed `crates/gpui/...` unless prefixed `waku`.

## B.1 App entry and window creation

1. Build the platform: `gpui_platform::application()` — on Linux this is `gpui::Application::with_platform(gpui_linux::current_platform(false))`; `gpui_linux::current_platform` picks Wayland/X11/Headless via `gpui::guess_compositor()` (env: `WAYLAND_DISPLAY` then `DISPLAY`; `ZED_HEADLESS` forces headless) (`crates/gpui_platform/src/gpui_platform.rs:31-53`; `crates/gpui_linux/src/linux.rs:47-70`; `crates/gpui/src/platform.rs:97-120`).
2. `Application::with_assets(asset_source)` (app.rs:200), then `.run(|cx: &mut App| { ... })` (app.rs:226) — the callback receives `&mut App`, the root context, once the platform loop starts. On Linux, `Platform::run` blocks for the app's lifetime.
3. In the callback: register fonts (`cx.text_system().add_fonts(...)`, text_system.rs:102), install globals and key bindings (`cx.bind_keys(...)`, app.rs:2164), then `cx.open_window(WindowOptions, |window, cx| -> Entity<V>)` (app.rs:1236). The builder gets `&mut Window` and must return the root `Entity<V>` (a `Render` view). `open_window` draws one frame before returning.
4. `WindowOptions` (platform.rs:1794-1865): `window_bounds` (see `WindowBounds::Windowed`/`Maximized`/`Fullscreen`, platform.rs:1927), `titlebar: Option<TitlebarOptions>` (title, `appears_transparent` **macOS+Windows only**, `traffic_light_position` macOS only; platform.rs:1989-2000), `focus`, `show`, `kind: WindowKind` (`Normal | PopUp | AnchoredPopup(popup options) | Floating | LayerShell (wayland-only) | Dialog`, platform.rs:2003-2030), `is_movable`, `app_owns_titlebar_drag` (macOS only), `is_resizable`, `is_minimizable`, `display_id`, `window_background: WindowBackgroundAppearance` (`Opaque | Transparent | Blurred | MicaBackdrop`, platform.rs:2063-2090), `app_id`, `window_min_size`, **`window_decorations: Option<WindowDecorations>` (`Server` default | `Client`) — the Linux knob** (platform.rs:500-507), `icon` (X11 only).
5. waku's concrete call (main.rs:319-336): centered 1380×880, min 980×680, transparent titlebar, `is_movable: false`, `window_background: Blurred`.
6. Window-level: `window.refresh()` (window.rs:2166) requests a redraw; `window.request_animation_frame()` (window.rs:2509) keeps frames coming for animations; `window.on_next_frame(cb)` (window.rs:2490) and `window.defer(cx, cb)` (window.rs:2387) defer work to the next frame; `window.viewport_size()` (window.rs:2627), `window.bounds()` (window.rs:2584).
7. `cx.set_menus(...)` (app.rs:2349) builds a **native menu bar — macOS only**; on Linux menus are stored in memory and never shown (B.11, C.3).

## B.2 The Render trait and element tree

- A "view" is an `Entity<T>` where `T: Render` — `trait Render { fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement; }` (element.rs:163-172). Render is called every frame; you build a fresh element tree from state each time (waku rebuilds everything per frame — rows, chips, menus — and pays for it only for visible list rows).
- `IntoElement` (element.rs:145) / `RenderOnce` (element.rs:179) / `ParentElement::child/extend` (element.rs:188) are the composition traits. **Components** are plain structs deriving `IntoElement` (waku's `MenuChip`, `ProjectNameSelector`, `TextField` in `src/ui/mod.rs`/`ui/text_field.rs` all implement `RenderOnce` with a `base: Stateful<Div>` and forward `Styled`/`InteractiveElement`/`ParentElement` to it — that is the reusable-component idiom).
- The workhorse element is `Div` (elements/div.rs, 203K): a flexbox div with style + interactivity. Builder functions: `div()`, `flex()` (row) / `flex_col()`, `items_center()`, `justify_center()`/`justify_between()`, `gap(px)`, `p/px/py/pt/pb`, `m/mx/my/ml/...`, `w/h/size/min_w/max_w/min_h/max_h`, `flex_1()`, `flex_none()`, `min_w_0()`, `truncate()`, `line_clamp(n)`, `whitespace_nowrap()`, `overflow_hidden()`/`overflow_x_scroll()`, `rounded(px)`/`rounded_full()`, `border_1()`/`border_l_1()`/`border_b_1()` + `border_color()`, `bg(color)`, `text_color()`, `text_size(px)`, `line_height(px)`, `font_weight()`, `font_family()`, `opacity()`, `shadow_xs/md/lg()`, `absolute()/relative()` + `top/left/right/bottom(px)` + `inset_0()`, `size_full()`, `cursor_default()/IBeam()/col_resize()`, `key_context(...)`, `track_focus(&FocusHandle)`, `tab_index()`, `tab_group()`, `focus_visible(...)`, `hover(...)`, `active(...)`, `group(name)` + `group_hover(name, ...)`, `when(cond, ...)`, `when_some(...)`, `id(ElementId)` — all generated by `gpui_macros::style_helpers!()` from the `Styled` trait (styled.rs:22-60) and `InteractiveElement` (div.rs:732+).
- **Interactivity** (div.rs): `on_click` (:566), `on_action` (:431), `on_hover(&bool)` (:651), `on_mouse_down/up/move`, `on_mouse_down_out`, `on_mouse_up_out` (:259, :279), `capture_any_mouse_down` (:142), `on_key_down`, `drag_over::<T>()` + `on_drop` (used by waku for file-drop staging), `tooltip(builder)` (GPUI owns tooltip timing/placement; the builder returns an `AnyView` to render — waku's `ui/tooltip.rs` is exactly that view).
- **Low-level drawing**: `canvas(prepaint, paint)` element (elements/canvas.rs:20-46) — callbacks get `Bounds<Pixels>`; inside paint you use `window.paint_quad(quad(...))` (window.rs:4192), `window.paint_path(path, color)` (window.rs:4263), and mouse listeners via `window.on_mouse_event(...)` (window.rs:4937). This is how waku draws its overlay scrollbar, selection washes, the dashed project-name underline, and the caret. `quad()` = `quad(bounds, corner_radius, fill, border_width, border_color, border_style)`.
- **Caching**: `.cached(StyleRefinement)` (waku's nav rail) keeps an element subtree from re-rendering across frames; `occlude()` marks layers that need their own scene surface (toast layer in waku).
- `list(state, render_item_fn)` is an element like any other (B.5).

## B.3 State ownership: Entity / Context / observers

Read `crates/gpui/src/_ownership_and_data_flow.rs` (the official doc, 7K):

- **The `App` owns everything.** `Entity<T>` is a reference-counted handle + type tag; it does *not* give you the state — you need an `App` (or `Context`) to `entity.read(cx)` / `entity.update(cx, |t, cx| ...)` (entity_map.rs:414-490; `read` :464, `update` :476, `downgrade` :449 → `WeakEntity` :740). Handles are cloneable; `WeakEntity` breaks cycles for background tasks.
- `Context<'a, T>` (app/context.rs:20) is the per-entity view of the app: `cx.entity()` (:50) to get your own handle, `cx.new(|cx| T)` to create children, `cx.observe(&entity, |this, cx| ...)` (:63) to be notified when another entity notifies (subscriptions are owned — waku stores them in `_subscriptions` or calls `.detach()`), `cx.notify()` (:229) marks yourself dirty → observers fire → next frame renders, `cx.spawn(async move |this: WeakEntity<Self>, cx| ...)` (:237) for async work (returns `Task`, `.detach()`), `cx.listener(move |this, event, window, cx| ...)` (:252) to build event handlers that borrow the entity, `cx.on_focus(&handle, window, ...)` (:547) / `cx.on_blur` (:596), `cx.focus_handle()` (:763), `cx.observe_window_appearance(window, ...)` and `cx.observe_window_activation(window, ...)` (waku app.rs:1816, 1825).
- `App` (app.rs:692) additionally has `cx.new`, `cx.set_global/has_global/global/global_mut` (app.rs:1940-1984) for app-wide singletons, `cx.window_appearance()` (app.rs:1343), `cx.bind_keys`, `cx.quit()`, `cx.activate`, clipboard, `cx.on_action` (global action handler).
- **waku's actual shape** (app.rs:798-950): one giant root entity `Waku` holding *everything* — persisted state, sub-`Entity`s for each text input (`composer: Entity<ComposerInput>`, search fields), `ListState`s, `FocusHandle`s, crossbeam channels for off-thread work (providers, probes, usage scans), caches (`Rc<Cell<f32>>` for animation-driven values, `Rc<RefCell<...>>` for per-frame borrows like the row-kinds cache). Sub-entities are updated via `cx.observe` chains or polled during render. Background work: channels + `cx.spawn` loops; frames only read snapshots. **For Tiller: a root app entity per window + small sub-entities for self-contained widgets (text fields, menus, navigation rail) is the proven shape.**
- Data flow rule from the source: render methods must be cheap and allocation-light for visible items only; expensive derivations happen on `notify`/settle boundaries and are cached (waku's `sync_transcript_rows` cache, `message_markdown` per-message cache with 512KB source cap, app.rs:140-146).

## B.4 Styling API

- The `Styled` trait (styled.rs:22) is the single styling surface; all methods take pixels via `px(f32)` (a `Pixels` newtype). Layout is **Taffy flexbox** (crates/gpui/src/taffy.rs). There is no CSS, no stylesheet system, no cascade — styles are set inline per element, per branch (`.when(cond, |e| ...)`), or via `.cached(style)`.
- Colors: `Hsla`, `rgb(0xRRGGBB).into()`, `hsla(h,s,l,a)`, `transparent_black()`, `.opacity(f)` (color.rs). Every color type implements `Into<AnyColor>`/`Into<Background>`.
- The theme is passed as plain data: every render reads `Theme::current(cx)` — a struct from an app `Global` (see B.9). Components take `&Theme` as an explicit parameter (waku's pattern: `fn render_x(&self, theme: &Theme, cx)`).
- Sizes: `Pixels` (px), `relative(f32)` (fraction of parent), `percentage(f32)`, `em`. `Bounds<Pixels>`, `point(px,px)`, `size(px,px)` for geometry (geometry.rs).
- Focus/hover/active states are *style refinements applied conditionally*: `.hover(|style| style.bg(theme.overlay))`, `.active(...)`, `.focus_visible(...)`, `.group_hover("group-name", ...)`.

## B.5 Lists and virtualized scrolling

- `list(state: ListState, render_item: impl FnMut(usize, &mut Window, &mut App) -> AnyElement)` (elements/list.rs:24-35) renders only visible items. `ListState(Rc<RefCell<StateInner>>)` (:54) is a **cloneable handle the view owns as a field**; the list element mutates it during layout/paint.
- `ListState::new(item_count, alignment, overdraw_px)` (:314) — `ListAlignment::Top` (history lists) or `Bottom` (chat transcripts, anchored to the tail); overdraw in px (waku uses 2048 for transcripts, 256 for sidebar, 512 for the diff list, app.rs:1751-1755).
- Mutation: `items_changed(ix, new_count)` (mark dirty range), `set_visible`, `flush_cache`; scrolling: `scroll_to(ListOffset{item_ix, offset_in_item})` (:660), `scroll_to_end()` (:603), `scroll_to_item(ix, align)`; queries: `item_count()` (:478), `logical_scroll_top()` (:560), `viewport_bounds()` (:799), `bounds_for_item(ix)` (:711), `max_offset_for_scrollbar()` (:772), `scroll_px_offset_for_scrollbar()` (:781) — the last two are exactly what waku's custom overlay scrollbar consumes through its `Scrollable` trait (ui/scrollbar.rs:100-120).
- Row index → element mapping: waku precomputes a flat `Vec<SidebarRow>`/row-kinds cache per frame (sidebar.rs:748-812; transcript_view.rs:975-995) so the per-row closure is a cheap match + lookup, not a scan.
- **Follow-scroll**: waku's transcript pins to the bottom via `scroll_to(ListOffset { item_ix: item_count(), offset_in_item: 0 })` whenever content is appended while anchored, and measures `anchor_end_space` to stop following when the user scrolls up (transcript_view.rs:117-148). `FollowState` also exists inside ListState for zed-style followed panes.
- The scrollbar is **not** a GPUI builtin: waku paints one with `canvas` + `paint_quad` from list geometry (see A.2, ui/scrollbar.rs) — that is the pattern to copy (AppKit-style overlay behavior, one element, zero layout children).

## B.6 Text input

Two layers exist, and waku uses the *low* one:

1. **`EntityInputHandler` trait + `ElementInputHandler<V>`** (input.rs:10-132): implement the trait on your entity (`text_for_range`, `selected_text_range`, `marked_text_range`, `unmark_text`, `replace_text_in_range`, `replace_and_mark_text_in_range` (IME), `bounds_for_range`, `character_index_for_point`, `set_selected_text_range`, `text_length_utf16`, `accepts_text_input`); during your element's paint call `window.handle_input(&focus_handle, ElementInputHandler::new(bounds, entity), cx)` (window.rs:4916). This routes all platform text events (typing, IME composition, clipboard) to your entity.
2. **Text shaping/painting**: `window.text_system()` (window.rs:2250) → `WindowTextSystem::shape_line(text, font_size, runs, force_width)` / `shape_text(...)` (text_system.rs:397, :509) with per-range `TextRun { len, font, color, background_color, underline, strikethrough }`; `TextLayout` (text_system.rs:365) caches shaped lines and paints via `layout.paint(...)`.

waku's `ComposerInput` (src/input.rs:497-2182) is a full custom editor built on layer 2: owns `content`, `selected_range`, `marked_range` (IME), an `EditHistory` (undo/redo), `vertical_navigation` (goal-x for soft-wrapped Up/Down), `scroll_offset` for single-line fields, caret blink via a separate `Entity<BlinkCursor>` observed for notify; render is a `div().key_context("ComposerInput").track_focus(...)` with ~24 `on_action` handlers (backspace/delete/left/right/up/down/select_*/home/end/word-moves/paste/cut/copy/undo/redo/enter/newline) plus mouse handlers, containing a custom `InputElement` (a leaf `Element` with request_layout/prepaint/paint) that paints shaped text, the selection wash, and the caret quad, and registers the input handler (input.rs:2160-2210). Context menu is a custom popover. A search-mode variant keeps text on one line with `whitespace_nowrap().overflow_hidden()`.

**Tiller guidance**: for a composer + search fields, either reimplement this (proven, full control — waku did) or check whether zed's richer editor elements (crates/editor) can be slimmed; GPUI core itself ships no editable-text widget — the field is yours either way. The essential trio is: `ElementInputHandler` registration in paint, `shape_line` + `TextRun`s, and `focus_handle`-routed actions.

## B.7 Focus management

- `FocusHandle` (window.rs:504-590): `cx.focus_handle()` creates one; `handle.focus(window, cx)` (:578) moves focus; `handle.is_focused(window)` (:583) queries. Views keep handles as fields (`composer_focus`, `settings_focus`, per-row focus in waku).
- A focusable element is `div().id(...).track_focus(&handle).tab_index(0)` — `track_focus` (div.rs:752) wires the element to the handle; `Focusable` trait (window.rs:678) gives `focus_handle(&self, cx)` for widgets.
- `window.focus(&handle, cx)` (window.rs:2185) programmatically moves focus (waku defers it to next frame after opening rename fields: `window.on_next_frame(move |w, cx| w.focus(&focus, cx))`, sidebar.rs:734).
- Focus is a tree: `on_focus`/`on_blur` (context.rs:547/596) notify entities when their handle gains/loses focus (waku uses this to stop/start caret blink, input.rs:547-558).
- Keyboard tabbing: `tab_index(0)` + `tab_group()` + `tab_stop(false)` manage the tab order (tab_stop.rs); waku marks row groups so Tab moves between logical groups.
- **Focus + actions**: focused elements establish key context (B.8); `focus_visible` styles render the accent focus ring.

## B.8 Keybindings and actions

- Declare actions: `actions!(namespace, [Quit, NewSession, ...])` macro (action.rs:24-52) — generates unit structs deriving `gpui::Action`; `Action` is also derivable for data-carrying actions (e.g. `SelectNextEntry(usize)`).
- Bind: `cx.bind_keys([KeyBinding::new("cmd-k", ToggleCommandPalette, None), ...])` — third arg is the **key context** (`Option<&str>`; `None` = global) (keymap/binding.rs:10-48; app.rs:2164). waku binds ~45 bindings at startup with contexts `"Waku"`, `"ComposerInput"`, `"MenuPanel"`, `"FileEditorPane"`, `"FindBar"`, `"Browser"`, `"BrowserAddress"` (main.rs:300-342).
- Dispatch: elements declare `.key_context("Waku")` (a stack — inner contexts shadow outer; waku's composer card pushes `"ComposerAutocomplete"` only while the autocomplete popup is open, so Enter submits vs accepts, composer.rs:1820-1840) and `.on_action(cx.listener(Self::handler))` (div.rs:431). Dispatch walks focused-context → parent contexts; `cx.stop_propagation()` / `cx.propagate()` control fall-through (waku's copy-selection fallback chain, transcript_view.rs:283-325).
- Keybinding strings: `"cmd-q"`, `"shift-enter"`, `"escape"`, `"f10"`, `"cmd-alt-shift-f"` (modifier order cmd/alt/ctrl/shift). `cmd` maps to Ctrl on non-macOS? — **not automatic: see C.6.**

## B.9 Theming

- **GPUI core has no theme system** (no Theme type in `crates/gpui`; zed's theming lives in a separate `crates/theme` crate built on GPUI primitives — `Theme { id, name, appearance, styles: ThemeStyles }`, theme.rs:233 — with schema/registry/fallback machinery Tiller does not need).
- The platform gives you **appearance only**: `WindowAppearance { Light, VibrantLight, Dark, VibrantDark }` (platform.rs:2063-2073); `cx.window_appearance()` (app.rs:1343); `cx.observe_window_appearance(window, ...)` for changes. On macOS it's `NSAppearance`; **on Linux it comes from the XDG desktop portal `color-scheme` setting** (gpui_linux xdg_desktop_portal.rs:185 `window_appearance_from_color_scheme`).
- waku's theme pattern (src/theme.rs, 231 lines — the whole thing): a plain `struct Theme { is_dark, ~35 Hsla fields }` with two constructors `dark()` / `light()`; stored as an app `Global` (`ActiveWakuTheme(Theme)`, `cx.set_global`); `Theme::current(cx)` reads it (fallback dark); `init(cx)` resolves the startup palette from `window_appearance`; `apply_theme_preference(pref, window, cx)` (System/Light/Dark, settings.rs:603) re-sets the global, forces the native window appearance where possible, and calls `window.refresh()`. A `ThemePreference` enum is persisted in state; `observe_window_appearance` re-applies when the system flips and the preference is System (app.rs:1816-1822).
- Everything downstream derives a `Palette`/`MarkdownPalette` from the Theme per render (md/render.rs:128-148) — components never hardcode colors.
- **This is the pattern to copy exactly**: one Global, one struct of Hsla, two constructors, read per frame.

## B.10 Rendering the app: what the platform loop does (Linux)

- wgpu (Vulkan/GL) via `gpui_wgpu` (pulled in by both Linux features); macOS uses Metal. `WindowBackgroundAppearance::Opaque` windows skip drawing behind content (opaque-region optimization, wayland/window.rs:1991-1997).
- Text: font-kit + cosmic-text? — the `font-kit` feature of gpui_macos is for macOS; Linux text uses fontconfig through `gpui_linux` (text_system.rs). waku builds with `features = ["font-kit"]` on gpui_platform (Cargo.toml). Embedded fonts are added via `text_system().add_fonts(bytes)` — works on all platforms; on Linux the app must also rely on system fonts for `.SystemUIFont`-equivalent families (see C.7).
- Rendering is immediate-mode-ish: every frame, the `Render` tree is rebuilt, layout via Taffy, paint via the scene (scene.rs); `window.refresh()`/`notify` schedules frames. There is no retained DOM; cheap per-frame construction is the intended style.

## B.11 Backends and feature flags (what matters on Linux)

- `gpui_platform` features (gpui_platform/Cargo.toml): `wayland`, `x11` (map to `gpui_linux/wayland`, `gpui_linux/x11`), `font-kit` (macOS), `test-support`, `screen-capture`, `runtime_shaders` (macOS). **Tiller should build with both `wayland` and `x11` (gpui_linux defaults) or pick one; `test-support` for UI tests.**
- `gpui_linux` features pull: wayland → wayland-client, wayland-protocols (+plasma for KDE blur, +wlr for layer-shell), xkbcommon/wayland, `ashpd` (XDG portal), open; x11 → x11rb, xkbcommon/x11, as-raw-xcb-connection, ashpd (gpui_linux/Cargo.toml).
- Runtime selection: `guess_compositor()` (B.1) — `WAYLAND_DISPLAY` wins if the `wayland` feature is on, else `DISPLAY`; `ZED_HEADLESS` → headless (great for tests/CI).
- X11 specifics: XIM input-method handler (x11/xim_handler.rs), `_NET_WM_SYNC_REQUEST` resize sync (x11/window.rs:640-663), XI2 gesture events when supported (x11/window.rs:677-698), `_MOTIF_WM_HINTS` for decorations (x11/window.rs:1879).
- Wayland specifics: xdg-toplevel + xdg-decoration (wayland/window.rs:270-299), `zwp_text_input_v3` IME (wayland/client.rs:49-53, 139-150), layer-shell for overlays, fractional scaling protocol (wayland/window.rs:26).
- Headless: `current_platform(true)` / `headless()` lets logic and even windows run without a display (used by waku's tests via `test-support`).

---

# Part C — Linux windowing notes (what Tiller can and cannot assume)

## C.1 Window decorations

- **There is no hidden-native-titlebar trick on Linux.** `TitlebarOptions.appears_transparent` is macOS/Windows-only (platform.rs:1990-1993). On Linux the knob is `WindowOptions::window_decorations: Option<WindowDecorations>` (`Server` default | `Client`) (platform.rs:500-507, 1860-1862).
  - **X11**: requests are honored via `_MOTIF_WM_HINTS` (x11/window.rs:1879-1886); the window manager may ignore them (comment at :727-728 — CSD "seem to work on X11 even without `true`"). `client_side_decorations_supported` is probed (x11/window.rs:283, 421).
  - **Wayland**: uses `zxdg_toplevel_decoration_v1` (xdg-decoration protocol) through the decoration manager (wayland/window.rs:270-299); whether you get CSD depends on the compositor's preference — GPUI requests per `window_decorations`, the compositor can override.
- **Consequence for Tiller**: the app must look right both with an OS-drawn titlebar (SSD) and without one (CSD). waku's 48px header with custom drag regions works under both *if* Tiller draws it; under SSD the OS bar adds its own strip above it (so reserve nothing, treat the header as in-window chrome — same as waku under a transparent titlebar). Under CSD, `window.set_client_inset(px)` tells GPUI the width of the invisible decoration edge (window.rs:2669-2674) so hit-testing/shadows account for it.

## C.2 Title bar and window dragging

- No `titlebar_double_click` (macOS-only, window.rs:6092). Instead:
  - `window.start_window_move()` (window.rs:2665-2667) — "Handle window movement for Linux and macOS. Tells the compositor to take control" (works on Wayland and X11).
  - `window.show_window_menu(position)` (window.rs:2655-2657) — opens the **native titlebar context menu** (the "close/minimize/move" menu) at a point; this is the CSD idiom for a right-click on a custom titlebar.
- waku's drag idiom ports directly: mouse-down arms a flag, first mouse-move calls `start_window_move`; double-click is the only piece that needs re-mapping (macOS: `titlebar_double_click` → zoom/minimize; Linux: either ignore or call `show_window_menu`/toggle-maximize). Note `is_movable: false` + `app_owns_titlebar_drag` are macOS-only semantics (platform.rs:1813-1831).

## C.3 Menus

- `cx.set_menus(...)` produces a **native menu bar on macOS only**; on Linux, `set_menus` just stores the menu model in memory — nothing is shown (gpui_linux platform.rs:616-624). Key equivalents registered through menus still dispatch via the keymap on macOS; **on Linux, only `bind_keys` matters.**
- **Consequence**: Tiller must render its own menu affordances in-window (command palette is the waku answer; a hamburger/menu-bar row is another). Context menus and dropdowns are in-app popovers either way — waku's `ui/menu.rs` (custom popover with `MenuItem { Entry(label, icon, selected, disabled, on_click), Custom, Header, Separator }`, keyboard nav via bound actions in a `"MenuPanel"` key context) is the pattern.
- Native popup windows: `WindowKind::AnchoredPopup` exists on both Linux backends (wayland: xdg_popup with seat grab, wayland/window.rs:197-239; x11 supported too) for menus that must escape the window — but waku never uses it; it draws everything in-window. Tiller can ignore native popups.

## C.4 Transparency and blur

- `WindowBackgroundAppearance` (platform.rs:2063-2090):
  - `Opaque` (default) — full paint, compositor skips background (opaque-region hint on Wayland, wayland/window.rs:1991-1997).
  - `Transparent` — alpha compositing; works on both backends.
  - `Blurred` — **"Not always supported."** On Wayland it is implemented **only via the KDE KWin blur protocol** (`org_kde_kwin_blur`, wayland-protocols-plasma; wayland/window.rs:105-107, 1999-2005) — i.e. KDE Plasma gets a blurred backdrop, GNOME/sway/etc. get plain transparency or nothing. **X11 has no blur support at all** (only the Opaque check at x11/window.rs:293).
  - `MicaBackdrop` — Windows 11 only.
- **Consequence for Tiller**: waku's macOS look leans on `Blurred` window background + `NSVisualEffectView` sidebar vibrancy (platform.rs `configure_sidebar_material` — macOS-only; the theme's `sidebar: transparent_black()` in both modes is the giveaway). On Linux, a transparent sidebar shows whatever the compositor puts behind the window — usually noise. **Tiller must pick a real sidebar color for Linux** (e.g. promote `sidebar_drag_background` (#181818/#F3F3F3) to the resting sidebar fill) and keep blur as a KDE-only enhancement. Rounded window corners: GPUI draws none; under SSD the WM rounds the frame, under CSD the window is rectangular — do not paint corner masks on the window itself (the wayland opaque-region comment at :1989 notes rounded CSD corners break that API).
- The 6%-wash hover/selection system is alpha-on-top-of-our-own-surface — fully portable; only the *sidebar base* needs the platform decision above.

## C.5 Dark mode detection

- On Linux, `WindowAppearance` is derived from the XDG desktop portal `org.freedesktop.appearance color-scheme` (xdg_desktop_portal.rs:36-121, 185) — works on GNOME/KDE/etc. when a portal is present; without one, appearance stays `Light` (the `Default`). `observe_window_appearance` fires on portal changes (x11/client.rs:487-490, wayland/client.rs:799-806).
- **Consequence**: `ThemePreference::System` behaves on Linux but is only as good as the portal; Tiller's settings toggle (System/Light/Dark, waku theme.rs:6-36) should still be offered, and `apply_theme_preference`'s `native_override` is macOS-only (platform.rs — `set_window_appearance` has no Linux effect; on Linux GPUI's window appearance is not settable per-window).

## C.6 Keyboard conventions

- Keybinding strings use `cmd` (macOS ⌘). **On Linux, does `cmd` map to Ctrl?** — in zed's keymap, `cmd` and `ctrl` are distinct; zed ships per-platform default keymaps that bind the same actions to `cmd-...` on macOS and `ctrl-...` on Linux. waku hardcodes `cmd-` bindings (main.rs:300-342) and runs macOS-only. **Tiller must decide: either bind both (`ctrl-` on Linux) or normalize `cmd`→`ctrl` at keymap-parse time** — check `KeyBinding::new`/keystroke mapping (keymap/binding.rs:33-48) when implementing; do not assume ⌘ semantics arrive as Ctrl events.
- Escape is `escape`, F10 is `f10` — portable.

## C.7 Fonts

- `.SystemUIFont` is a macOS pseudo-family (resolved by AppKit). On Linux it resolves via fontconfig fallback (GPUI's default font family for the platform). **Tiller should pick an explicit UI font stack** (e.g. a system sans via fontconfig names) and **embed JetBrains Mono** (or another mono) exactly the way waku does (`include_bytes!` + `add_fonts`), since mono coverage varies wildly across Linux distros. Symbols/icon fonts: waku registers a Nerd Font symbols face on macOS only — on Linux, avoid glyph-fallback dependence or ship the font (add_fonts works everywhere).

## C.8 What Tiller can assume vs must verify (summary)

| Capability | Linux status |
|---|---|
| Opaque windows, full rendering | ✅ both backends |
| Transparent windows | ✅ both backends |
| Blurred window background | ⚠️ Wayland + KDE only (KWin blur protocol); not on X11 |
| CSD (own titlebar) | ✅ Wayland via xdg-decoration (compositor may override), ✅/⚠️ X11 via _MOTIF_WM_HINTS (WM may ignore) |
| `start_window_move` | ✅ both (compositor-driven) |
| Native titlebar context menu | ✅ `show_window_menu` both |
| Native menu bar | ❌ none — in-app menus/command palette required |
| Native titlebar double-click action | ❌ `titlebar_double_click` is macOS-only |
| Dark-mode detection | ⚠️ XDG portal color-scheme; defaults Light without portal |
| IME | ✅ X11 via XIM, Wayland via text-input-v3 (marked text in the input handler) |
| System notifications | ⚠️ via `ashpd` (xdg portal) — gpui_linux/system_notifications.rs |
| Fractional scaling | ✅ Wayland (wp-fractional-scale); X11 via XRandR/scale |
| Per-window appearance override | ❌ macOS-only; theme preference must be app-internal |

---

# Part D — Build-order implications for Tiller (derived, opinionated)

1. **Theme module first** (the 35-color struct + Global + preference) — everything else reads it.
2. **UI kit** (icon helper, icon-button, chip, text-field shell, tooltip view, overlay scrollbar) before any screen — waku's own dependency order in `src/ui/`.
3. **Root shell** (sidebar + header + main column + resize handles) with real drag regions, then the virtualized transcript, then the composer + custom text field (the hardest single component), then overlays (palette/menus/toasts).
4. Keep the **per-frame construction + cheap visible-only rows + canvas-painted chrome** discipline; do not try to out-smart GPUI with retained widgets.
5. The visual bar is measurable: 11.5px UI text, 13.5px body, 6% neutral washes, coral accent `#E2795B` (dark) / `#C85F44` (light), 720px content column, 48px bars, radius scale 4/6/7/8/12/13/full — deviations should be deliberate.

## Known gaps / uncertainties (explicit)

- The waku checkout's screenshots (2266×1752) could not be visually inspected in this session (model without image support); the palette and layout above are 100% source-derived, but *rendering* details (shadow softness, exact anti-aliasing feel) are unverified against pixels.
- The zed checkout is upstream main @ `c05e346`; waku builds against a fork one patch ahead (layered scene rendering, PR #61945). If Tiller tracks upstream, verify `list()`/`canvas`/layering behavior on the pinned commit — APIs cited here exist in this checkout.
- `cmd`→`ctrl` mapping on Linux was not traced end-to-end (zed ships per-platform keymaps outside crates/gpui); flagged as a decision point, not a fact.
- The Material Icon Theme subset (100 file-type icons) is MIT-licensed — copyable — but the provider marks (Claude, OpenAI, Grok, Cursor, Pi, OpenCode, Amp) are third-party trademarks; Tiller should source its own provider marks for any providers it ships.
- `window_decorations: Client` behavior under GNOME (mutter) CSD overrides was not empirically tested — the code path exists (xdg-decoration), the compositor's word is final.
