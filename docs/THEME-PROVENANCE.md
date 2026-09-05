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

The dependency is pinned `=0.1.4` in `rust/Cargo.toml`. **After this work that
pin protects appearance, not just API**: bumping bezel restyles the app. Treat
any bump as a visual change to review, not a dependency chore.

Bezel 0.1.4 also gives frost and glass one surface vocabulary: `SurfaceStyle`
selects the material and `SurfaceSpec` carries the resolved gain, saturation,
edge and shadow parameters. Any surface review must therefore consider both
the frost and glass paths, rather than treating them as unrelated paint APIs.

`dark_palette_comes_from_bezel` and `light_palette_comes_from_bezel` compare
the taken tokens against bezel itself, so a bump that moves a value fails the
suite rather than shipping quietly.

**The greys carry a chosen hue.** Settings → Appearance offers bezel's five base
colours (`BASE_COLORS` in bezel's `brand.rs`: Neutral, Stone, Zinc, Gray,
Slate), and `ThemeColors::for_appearance` builds its bezel palette through
`Theme::branded` rather than `Theme::dark()` / `light()`. Only hue moves —
lightness is never a knob, so every family holds the contrast the shipped
palette was verified at, and the semantic hues (danger, warning, success) keep
their own. `Neutral` is `Tint::NONE`, which reproduces the shipped palette
exactly; it is the default, and it is why the two tests above still compare
against bare `bezel::theme::Theme::dark()` / `light()`.

Three things do not rotate. `brand_coral` is Sirio's identity and is anchored by
its own two measured constraints. `terminal_surface` stays out of it because the
terminal is deliberately independent of the shell's panel hierarchy and the
sixteen ANSI colours read against it. And **the borders do not either** — that
one is bezel's rule, not an omission: `Brand::apply` tints a token only `if
slot.a == 1.0 && slot.s <= f32::EPSILON`, and Sirio's seams have been
translucent veils since `border_opaque` collapsed onto `border`, so a border
reads the tint through compositing instead of carrying it.
`a_tinted_base_moves_the_greys_and_leaves_sirios_own_colours_alone` pins all
three.

**One exception: `text`.** bezel paints body text at full contrast against its
page — `#E5E5E5` on `#0D0D0D` is 15.4:1, `#222222` on `#F4F4F4` is 14.5:1.
Sirio pulls it back by `TEXT_SOFTENING` (10%) toward the surface it sits on,
giving `#CFCFCF` (12.5:1) and `#373737` (10.9:1).

The reason is glare, and the reference is the platform: read off this machine,
macOS's own `labelColor` is white at 85% alpha over `rgb(30,30,30)` in dark —
12.2:1 — and black at 85% over white in light, 14.9:1. bezel's dark rung is the
outlier, mostly because its page (`#0D0D0D`) is far darker than Apple's. The
softened values also land on the contrast Sirio itself shipped before adopting
bezel (12.2 and 10.8), which is the target rather than a taste.

Note the asymmetry that is *not* applied here: because light-on-dark haloes,
platform convention runs dark **lower** than light, and Sirio's symmetric 10%
runs it the other way in light (10.9 against macOS's 14.9). That was reviewed
on screen in both appearances and accepted. `text_muted` and below are
untouched — they are already pulled back, and softening them too would collapse
the ladder; `body_text_is_softened_off_bezels_full_contrast` pins the
relationship, not the numbers, so a bezel bump carries it along.

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
| `brand_coral` | Sirio's brand coral: hue 24.3° measured off the reference frames' inline-code tone, saturation and lightness chosen against two constraints — it clears WCAG AA on its own surface, and it is not any agent's brand (Claude's `#D97757` is the near one, 22 units away). `sirio_ui`'s `loading::bezel_theme` puts it on bezel's `accent` so the loaders keep painting Sirio's colour rather than bezel's grey. Held by `brand_coral_clears_contrast_on_its_own_surface` and `brand_coral_is_not_any_agent_brand`. |
| `frame_surface` | The translucent window-frame material, `frame_fallback` softened to 0.35 (dark) / 0.30 (light). bezel's `band` is a recessed palette header or footer strip, not a window frame. |
| `text` | bezel's, softened 10% toward the surface — see the exception above. Not a hand-picked hex: the rule is one line and follows a bezel bump. |
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
- **A focused text field lights bezel's `ring`, not `text`.** The composer, the
  settings fields, the sidebar's rename and filter fields all framed themselves
  in the body text colour on focus, which on dark is an opaque near-white box.
  `ring` is the hairline every bezel input, select and control uses: a 35% veil
  on the surface's own tone, so the field lifts in the appearance the user
  chose instead of being outlined. Held by `theme_colours_come_from_bezel`.
- **Frost and glass now share a surface model.** bezel 0.1.4 resolves both
  through `SurfaceStyle` / `SurfaceSpec`; the facade's surface behavior is one
  material contract instead of separate frost and glass paint paths.
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
- **The COSMIC tokens are gone** (spec decision B3). The four spacing steps and
  two radii Sirio read from them are bezel's at the same values, so no drawn
  pixel moved; the colours did. COSMIC carried a resting/hover/pressed set per
  semantic colour and bezel carries one value per meaning, so the titlebar's
  traffic lights derive their hover as the 12% darkening COSMIC's own pairs
  described, and its icon buttons read `text` on
  `element_hover`/`element_active`. Untested on Linux, which is where the loss
  of Pop!_OS adherence actually shows.
- **Geist is the face on every platform**, macOS included, registered by
  `bezel::ui::register_fonts`. Sirio's own `assets/fonts` copies are gone, and
  with them a latent Linux bug: they carried no 500/600/700 statics, and gpui's
  cosmic-text path rasterizes a variable font at its default instance only.
