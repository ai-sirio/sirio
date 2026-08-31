# Bezel base colours in Settings — design

Date: 2026-08-31
Status: approved, pending implementation plan

## Problem

Bezel's gallery offers five base colours. Sirio ships only bezel's default
neutral and gives the user no say. The request is to expose the five in
Settings → Appearance, and to remove from that page what does not belong.

Two facts, both verified in source rather than taken from the docs, set the
shape of the work.

### The "five themes" are five greys, not five skins

They are `BASE_COLORS` in `bezel-theme-0.1.3/src/brand.rs:39` — Tailwind's five
neutral families at their 500 step:

| Name | oklch hue | chroma |
|---|---|---|
| Neutral | — | 0.0 (`Tint::NONE`) |
| Stone | 58.071 | 0.013 |
| Zinc | 285.938 | 0.016 |
| Gray | 264.364 | 0.027 |
| Slate | 257.417 | 0.046 |

`Brand::apply` rotates the *hue* of 39 palette tokens and nothing else.
Lightness is never a knob — bezel's two palettes were tuned against measured
contrast ratios, and a brand rotates that work rather than replacing it. Tokens
that already carry a hue because they mean something (danger, warning, success)
keep it. So the five differ from each other subtly, by the temperature of the
greys, and every one of them holds the contrast the shipped palette was
verified at.

This is worth stating plainly because "five themes" invites the expectation of
five visibly different skins. It is not that, and the design does not try to
make it that.

### Sirio does not use bezel's brand mechanism

`ThemeColors::for_appearance` (`sirio_theme/src/lib.rs:244`) reads
`bezel::theme::Theme::dark()` / `light()` and builds Sirio's own struct from
them. `bezel::theme::set_brand` and `bezel::theme::Theme::install` are never
called — bezel's gpui globals are not where Sirio's palette lives. The tint has
to be applied on Sirio's side, at the one point where the bezel palette is
read.

## Decisions

### B1 — Two enums, mirroring the appearance pair

`sirio_theme::BaseColor` carries the concept and the tint;
`sirio_persistence::BaseColor` carries the serde contract; `sirio`'s `main.rs`
holds the single conversion between them.

This is not a new pattern. `ThemeMode` / `sirio_persistence::AppearanceMode`
are already exactly this, and CLAUDE.md records the split as deliberate. The
alternative — one enum in `sirio_theme`, read by `sirio_persistence` — would
put bezel back on `sirio_persistence`'s dependency list, which was work
deliberately undone during the theme adoption.

```rust
// sirio_theme
pub enum BaseColor { Neutral, Stone, Zinc, Gray, Slate }
impl BaseColor {
    pub fn tint(self) -> bezel::theme::Tint { … }   // the five constants above
    pub fn title(self) -> &'static str { … }        // "Neutral", "Stone", …
}

// sirio_persistence
pub enum BaseColor { Neutral, Stone, Zinc, Gray, Slate }
impl BaseColor {
    pub fn raw(self) -> &'static str { … }          // "neutral", "stone", …
    pub fn parse(raw: &str) -> Option<Self> { … }
}
```

### B2 — The tint is an explicit parameter, not a global

`ThemeColors::for_appearance` takes the base colour as a parameter and opens
with `bezel::theme::Theme::branded` instead of `::dark()` / `::light()`:

```rust
let bezel = bezel::theme::Theme::branded(
    &bezel::theme::Brand { tint: base.tint(), ..Default::default() },
    match appearance {
        Appearance::Dark => bezel::theme::Appearance::Dark,
        Appearance::Light => bezel::theme::Appearance::Light,
    },
);
```

The appearance is matched rather than converted because no `From` impl exists
between the two enums; `sync_appearance` (`lib.rs:1182`) already spells the
same match out by hand.

`Theme::branded` (`brand.rs:96`) is public and pure: it builds
`for_appearance(appearance)` and applies the brand's hues. It does *not* touch
`radius` or `glass` — those are applied by bezel's `Theme::install`, which
Sirio does not call — so the call rotates colour and only colour.

The parameter, rather than a process-global mirror, for two reasons. First,
`for_appearance` builds both appearances in one process, which is why the file
already avoids bezel's global-resolving `wash`/`ink` helpers; a tint global
would reintroduce the problem the current code works around. Second, a global
needs the tests serialised behind a guard, and the crate already has one
(`lock_appearance`, `lib.rs:1673`) — a second is a cost paid later.

The reach is small and contained. All 13 `for_appearance` call sites live in
`sirio_theme/src/lib.rs` itself — none in any other crate — so the parameter is
threaded through one file: `ThemeColors::for_appearance` (`:244`),
`Theme::for_appearance` (`:1362`), `for_mode` (`:1287`), `for_mode_linux`
(`:1294`), `with_translucency`'s rebuild (`:1342`) and the portal follower
(`:1274`).

`Theme::light()` (`:1299`) and `Theme::dark()` (`:1304`) keep their signatures
as `Neutral` wrappers, which leaves the ~199 test call sites of those two
untouched.

### B3 — `Theme::install` reads the current choice from the global

`Theme::install` recovers the base colour from `cx.try_global::<Self>()`, the
line beside the one that already does this for `translucency`
(`lib.rs:1191`). No new plumbing, and the precedent is one line away from the
new code.

### B4 — `terminal_surface` and `brand_coral` do not rotate

Both are hand-picked hexes and stay that way.

`brand_coral` is Sirio's identity, anchored by two measured constraints
(`docs/THEME-PROVENANCE.md`): it clears WCAG AA on its own surface and is not
any agent's brand colour. Rotating it would break both.

`terminal_surface` is the deliberate exception. THEME-PROVENANCE records that
"the terminal is deliberately independent of the shell's panel hierarchy" — it
is paper-white in light and the pre-shell well in dark. Two further reasons not
to move it: the sixteen ANSI colours are their own vocabulary and read against
that background, and at Slate's chroma (0.046, the strongest of the five) the
seam between a tinted frame and a neutral well is the price of not overturning
a documented decision on no evidence.

`frame_fallback` is `Rgba::from(bezel.bg)` (`lib.rs:309`), so the window frame
follows the tint on its own. So do `text`, the borders and the whole surface
ladder. Nothing extra is needed for them.

### P1 — One new key, no migration

`setting` is a key-value table (`migrations.rs:61`); `AppSettings` maps named
keys, not columns. The new key is `"appearance.baseColor"`, default `neutral`,
loaded beside `"appearance.theme"` in `db.rs:915`. An unparseable value falls
back to the default, as every other enum setting does.

No schema version bump. Removing `"appearance.fileIconTheme"` likewise needs
none: existing rows become inert and are never read again. Deleting them would
be a migration whose only effect is tidiness in a table nobody reads by hand.

### U1 — A third row in the Theme card, not a new section

The picker is `controls::segmented` with the five names, added to the existing
Theme card after Appearance and before Translucency.

The request asked for a new section. A separate card holding one row, beside a
card whose two rows answer the same question — which palette does the app
paint — reads as two topics where there is one. If the separate section is
preferred it is a one-line change.

Segmented text labels rather than the gallery's colour swatches: Sirio has no
swatch primitive, and five greys that differ by hue at chroma 0.013–0.046 are
not distinguishable in a 16px pill anyway. The names are the information.

### R1 — File icons: deleting dead configuration

The file-icon setting is persisted, converted into `SettingsSnapshot`, echoed
on the control socket and drawn as a picker — and read by nothing. `file_glyph`
(`right_panel/files.rs:807`) chooses unconditionally:

```rust
if !is_dir && let Some(asset) = key.material_asset() {
    return Icon::file_type(asset);
}
```

No branch anywhere consults `FileIconChoice`. On macOS a user can select "SF
Symbols" and nothing changes. This is not a feature being removed; it is
configuration that never reached the renderer.

Removed: `FileIconChoice`, `file_icon_choices`, `clamp_file_icons`,
`file_icons_segment`, `SEGMENTED_FILE_ICONS` and the Files section
(`sirio_ui/src/settings.rs`); `file_icon_theme` and `FileIconTheme`
(`sirio_persistence`); the key in `db.rs`; the `"fileIcons"` entry in the
control socket's settings echo (`main.rs:3209`); the conversions at
`main.rs:14903/14936`.

The Material assets stay. They are what the file tree actually renders.

### R2 — Agent colours: the override goes, the table stays

The Agent Colors section is a *user override* of which colour each of the five
agents wears, persisted in a side key-value store because `AppSettings` never
grew a column for it (F-SET-22, `session.rs:153`). Removed:
`render_agent_colors` and `sirio_ui::settings::AgentAccentColor`,
`load_agent_color_ids` / `save_agent_color_id` (`session.rs:1622/1635`), the
`agent_colors` field on the snapshot, the restore loop at `main.rs:15388` and
the save loop at `main.rs:15568`.

`sirio_theme::AgentBrandColor` **stays whole**. It is a separate table
(`lib.rs:1450`), and `for_agent_id` is load-bearing: the sidebar
(`sidebar.rs:60`), the icons (`icons.rs:378`) and the tab strip all read it.
Removing it would strip every agent mark of its colour.

`brand_coral` also stays. It loses one consumer — the picker's Coral entry —
and keeps `loading::bezel_theme`. The sentence in `docs/THEME-PROVENANCE.md`
that cites the picker is updated to match.

### R3 — The control socket contract

`"fileIcons"` disappears from the settings echo. This is a contract facing
`sirioctl` and the agent hooks, and it is the only part of this work visible
outside the app; it was raised during brainstorming and accepted.

`"baseColor"` is deliberately **not** added in its place. Adding a key to the
same response we are trimming would undo the point of trimming it.

## The Appearance page after

| Section | Rows |
|---|---|
| Theme | Appearance (System / Light / Dark) · Base color (Neutral / Stone / Zinc / Gray / Slate) · Translucency |
| Interface | Font size |
| Terminal | Font size |

## What deliberately does not change

- Bezel's pin stays `=0.1.3`. THEME-PROVENANCE's rule holds: a bump restyles
  the app and is reviewed as a visual change.
- `Brand`'s other three knobs — `accent`, `radius`, `glass` — are left at their
  defaults. `radius` in particular would move every measurement the theme
  adoption's decision T2 froze.
- Radii, spacing and font families.
- The semantic hues. `Brand::apply` keeps danger, warning and success where
  they are because they mean something; the tests assert this from Sirio's side
  too, so a bezel bump that changed the rule fails here rather than shipping.

## Testing

1. `raw` / `parse` round-trip over the five variants; an unknown string yields
   the default.
2. **`Neutral` reproduces today's palette exactly.** `Tint::NONE` is the
   shipped neutral, so `dark_palette_comes_from_bezel` and
   `light_palette_comes_from_bezel` become this test with no change to their
   bodies. This is the guard for anyone upgrading.
3. A non-`Neutral` tint moves `bg`, `surface` and `border`, and leaves
   `brand_coral` and `terminal_surface` untouched — decision B4, pinned.
4. The semantic hues do not move under any of the five.
5. `"appearance.baseColor"` round-trips through `AppSettings`, and an absent
   key loads as `Neutral`.
6. Removal coverage: the settings snapshot no longer carries `file_icons` or
   `agent_colors`, and the control-socket echo no longer contains
   `"fileIcons"`.

## Risks

- **The five look alike.** At chroma 0.013–0.046 the difference between Stone
  and Zinc is small on a small surface and clear on a full window. This is
  bezel's design, not a defect, but it is the most likely source of "the
  setting does nothing" reports. The names being Tailwind's is the mitigation:
  they are a vocabulary users may already have.
- **The terminal seam.** Decision B4 accepts a neutral terminal well inside a
  tinted frame. Visible at Slate, negligible at Stone. Revisit only with a
  screenshot, not in the abstract.
- **The socket contract.** R3 removes a key. Anything parsing the settings echo
  for `"fileIcons"` breaks. Nothing in this repo does.
