# Theme provenance

Where every value in `sirio_theme` comes from, and what moves it.

This replaces `docs/linux-rewrite/THEME-PROVENANCE.md`, which the tests and the
`ThemeColors` doc-comment referenced long after the file itself had been
deleted. The record was already lost once; that is why it is being written
again rather than only pointed at.

Written 2026-08-31, when the theme was rebased onto
[bezel](https://github.com/crabtalk/bezel) (spec decision B3 in
`docs/superpowers/specs/2026-08-30-bezel-theme-adoption-design.md`).

## Three categories, three different answers

### 1. Colours — bezel's, at the pinned version

`ThemeColors::for_appearance` reads `bezel::theme::Theme::dark()` /
`light()` and takes its neutrals, status hues, accent and diff colours
directly. Nothing is sampled, measured or transcribed any more.

The dependency is pinned `=0.1.3` in `rust/Cargo.toml`. **After this work that
pin protects appearance, not just API**: bumping bezel restyles the app. Treat
any bump as a visual change to review, not a dependency chore.

`dark_palette_comes_from_bezel` and `light_palette_comes_from_bezel` compare
all 23 taken tokens against bezel itself, so a bump that moves a value fails the
suite rather than shipping quietly.

### 2. Radii and spacing — bezel's constants, Sirio's measurements

Per spec decision T2 the measurements do **not** move; only their derivation
does. `Radii::default` and `Spacing::default` are ratios of
`bezel::theme::Theme::BASE_RADIUS` (8.0) and `SPACE_XS/SM/MD/LG`
(4/8/12/16) chosen to reproduce the numbers exactly, each with the resulting
value in a trailing comment.

Ratios that do not land on one of bezel's five named corners carry an explicit
multiplier rather than being rounded to the nearest named one — rounding would
move the UI, which T2 forbids. `radii_match_waku` and
`spacing_and_typography_match_waku` still assert the raw numbers and are the
guard on that.

The numbers themselves are waku's measured de-facto scale, recorded in
`docs/linux-rewrite/03-visual-bar-and-gpui-patterns.md` §A.2 — also a deleted
path, which is why the values are restated in the trailing comments in
`lib.rs` rather than only cited.

### 3. Sirio's own — the three bezel has no answer for

| Token | Why it is not bezel's |
|---|---|
| `brand_coral` | Sirio's brand coral: hue 24.3° measured off the reference frames' inline-code tone, saturation and lightness chosen against two constraints — it clears WCAG AA on its own surface, and it is not any agent's brand (Claude's `#D97757` is the near one, 22 units away). The Coral entry of the agent-colour picker reads it, and `sirio_ui`'s `loading::bezel_theme` puts it on bezel's `accent` so the loaders keep painting Sirio's colour rather than bezel's grey. Held by `brand_coral_clears_contrast_on_its_own_surface` and `brand_coral_is_not_any_agent_brand`. |
| `frame_surface` | The translucent window-frame material, `frame_fallback` softened to 0.88 (dark) / 0.82 (light). bezel's `band` is a recessed palette header or footer strip, not a window frame. |
| `terminal_surface` | Paper-white in light, the pre-shell dark well in dark. bezel has no terminal-surface concept, and the terminal is deliberately independent of the shell's panel hierarchy. |

Two more values are Sirio's choice but derived rather than measured:
`VEIL_FAINT` (0.05) and `VEIL_MID` (0.12), the alphas behind `overlay`,
`overlay_strong`, `tree_guide` and the two diff washes. They are fed through
`wash` and `hairline`, which are hand-copied from bezel's own two paint rules —
bezel exports only the forms that resolve against a process-global appearance,
and `ThemeColors::for_appearance` builds both palettes in one process, so it
cannot use them. `washes_follow_bezels_two_rules` is the guard on that copy.

## What changed in appearance, and was accepted

Recorded because no test covers any of it:

- **The blue is gone** (spec decision C1). `accent` was a quantity blue; it is
  now bezel's neutral grey. `file_link` and `git_untracked` followed it —
  `git_untracked` onto `text_faint`, so an untracked file reads as quieter than
  the chromatic staged / modified / conflict states rather than as a fourth
  status.
- **Panel borders went translucent.** `border_opaque` collapsed onto `border`:
  bezel draws every seam as a hairline veil, so the opaque separator has no
  source any more.
- **Hairlines are scaled up on light** by `INK_HAIRLINE_SCALE` (1.35), where
  Sirio scaled fills and edges the same.
- **The depth ladder inverted.** Sirio cut a well by darkening the page; bezel
  lightens it — in dark it is a translucent white veil, in light it is pure
  white on a grey page.
- **A starred row is exactly the warning hue.** Sirio turned warning up to full
  chroma to make the star louder; bezel's warning is already at full chroma, so
  there is nothing left to turn up. The sixth commit-graph lane moved off
  `favorite` onto `brand_coral` because of it — the two had become the same
  colour, and two graph lanes that cannot be told apart is a bug.
- **`solid` / `on_solid` stopped being the mirrored page.** They are bezel's own
  pair now; what is still asserted is the reason the mirror existed, that an
  inverted chip stays legible.
- **Geist is the face on every platform**, macOS included, registered by
  `bezel::ui::register_fonts`. Sirio's own `assets/fonts` copies are gone, and
  with them a latent Linux bug: they carried no 500/600/700 statics, and gpui's
  cosmic-text path rasterizes a variable font at its default instance only.
