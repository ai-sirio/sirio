# Theme provenance

Where Sirio's colours come from, what vocabulary they are held in, and why
each value is what it is. This document is the rationale moved verbatim out
of `ThemeColors::for_appearance` (`rust/crates/sirio_theme/src/lib.rs`), which
used to derive every token live and now reads the frozen presets instead.

The dump comparison and seeded Linux screenshots are recorded in the
[theme migration evidence](testing/theme-ely-palette/README.md).

## Where the values come from

The values are frozen from `bezel-theme` 0.1.4's `Theme::branded` plus
Sirio's hand-written ladders, on commit `f1b578b3`, by
`Scripts/theme/freeze-presets.py` from `docs/testing/theme-ely-palette/before/`.

`before/` was written on `f1b578b3` plus the dump example alone:

```bash
cd rust && cargo run -p sirio_ui --example theme_dump -- ../docs/testing/theme-ely-palette/before
```

It holds `sirio.tsv` (Sirio's 38 tokens, 28 combinations: 7 bases ×
dark/light × opaque/translucent, as the `Hsla` gpui paints), `ely.tsv` (the
`Palette` in Ely's global, same combinations), `bezel.txt` (bezel-theme's
installed `Theme`, Debug-printed) and `theme.txt` (spacing, radii,
typography and chrome tokens).

The hand-written ladders live in `rust/crates/sirio_theme/src/base_color.rs`:
the four-surface `NOTTE_LADDER` (page `0x0E1016`, surface `0x202127`, raised
`0x2B2F3A`, raised hover `0x313337` — the one place in the theme where
lightness is chosen rather than taken from bezel), and the solid grey ladders
`NEUTRAL_DARK` / `NEUTRAL_LIGHT` (approved shell values, 2026-09-12:
sidebars `#191919`, central panes `#141414`, chat `#232323`, composer
`#313131`, hover `#363636`) and `ONICE_DARK` (Neutral's shape from pure
black). See "Preset ladders" in `base_color.rs`.

## The vocabulary

Sirio's colours are held in Ely's `Palette` vocabulary (`ely-palette`), plus
the 21 `SirioColors` for the colours Ely has no field for, or whose value
differs from the Ely field of the same meaning (spec §3.2). Every colour
token is a `gpui::Hsla`.

The renaming rule (spec §3.1):

> A Sirio token becomes an Ely field only when both its meaning and its value
> coincide with that field's current value (the chat adapter's assignment).
> An alias token, whose own documentation defines it as another token, is
> replaced by the token it aliases. Every other token moves to `SirioColors`.
> As a result, Ely components and Sirio call sites each see today's values.

The rename table is `docs/testing/theme-ely-palette/rename.tsv` (old
`ThemeColors` field → new path):

| Old field | New path |
|---|---|
| `surface` | `ely.bg` |
| `surface_raised` | `ely.surface` |
| `input_bg` | `ely.sunken` |
| `element_hover` | `ely.hover` |
| `element_active` | `ely.active` |
| `border` | `ely.border` |
| `border_opaque` | `ely.border` (alias of `border`) |
| `text` | `ely.fg` |
| `text_muted` | `ely.fg_muted` |
| `text_faint` | `ely.fg_subtle` |
| `git_untracked` | `ely.fg_subtle` (alias: assigned `text_faint`) |
| `file_link` | `ely.link` |
| `selection` | `ely.selection` |
| `success` / `warning` / `danger` | `ely.success` / `ely.warning` / `ely.danger` |
| `favorite` | `ely.warning` (alias: a starred row is the warning hue) |
| `bg` | `sirio.canvas` |
| `frame_surface` | `sirio.frame_surface` |
| `terminal_surface` | `sirio.terminal_surface` |
| `dialog_surface` / `floating_surface` | `sirio.dialog_surface` / `sirio.floating_surface` |
| `overlay` / `overlay_strong` | `sirio.overlay` / `sirio.overlay_strong` |
| `border_strong` | `sirio.border_strong` |
| `ring` | `sirio.ring` |
| `text_dim` | `sirio.text_dim` |
| `danger_muted` | `sirio.danger_muted` |
| `accent` | `sirio.quantity` |
| `solid` / `on_solid` | `sirio.solid` / `sirio.on_solid` |
| `brand_coral` | `sirio.brand_coral` |
| `code_wash` | `sirio.code_wash` |
| `tree_guide` | `sirio.tree_guide` |
| `diff_add`, `diff_add_bg`, `diff_del`, `diff_del_bg` | `sirio.diff_*` |

## What Ely receives for fields Sirio had no token for

Every field of `ely.*` holds a frozen value per preset, equal to what Ely
receives today (spec §3.3):

| Ely field | Today's value |
|---|---|
| `bg`, `surface`, `sunken`, `hover`, `active`, `border`, `fg`, `fg_muted`, `fg_subtle`, `link`, `selection`, `success`, `warning`, `danger` | the Sirio token renamed to it (§3.2) |
| `overlay`, `tooltip_bg` | `surface_raised` |
| `border_strong` | `border` |
| `fg_disabled` | `text_faint` |
| `accent`, `focus`, `tooltip_fg` | `text` |
| `accent_hover` | `text_muted` |
| `on_accent` | `surface` |
| `backdrop` | `overlay` |
| `success_subtle`, `warning_subtle`, `danger_subtle` | the hue at 12% opacity |
| `on_media`, `media_backdrop`, `shimmer`, `glass`, `shadow`, `paper`, `ink`, `info`, `info_subtle`, `chart`, `ansi`, `syntax` | Ely's own default for the appearance, retained |

An earlier draft proposed deriving `syntax` from Sirio's source palette,
`ansi` from the terminal and `chart` from the graph lanes. The vendored chat
already reads `shimmer`, `info`, `chart` and `syntax`, so those rules would
change the chat. In this sub-project, every field the adapter leaves unset
keeps Ely's default. The derivation rules move to the first sub-project that
migrates a surface reading them.

Sirio's own source-code colours stay in the 24-kind
`bezel_theme::SyntaxPalette` returned by `Theme::syntax_palette()`. That is
the highlighting boundary chosen for the migration; Ely's `Syntax` has 13
fields and cannot hold them.

## Per-token reasoning

Moved verbatim from `ThemeColors::for_appearance`. Each heading names the new
token.

The derivation as a whole:

> Every neutral, status and diff token below is bezel's. What stays Sirio's
> is listed in `Group C` of the design doc: the coral, the window-frame
> material, the terminal surface, and the washes that sit between bezel's
> rungs.
>
> The palette is read here rather than through bezel's `wash`/`ink` helpers
> because those resolve against a process-global appearance, and this
> function is called for both appearances in one process. `branded` is the
> same reason the tint arrives as a parameter: a brand global would put back
> exactly the problem those helpers have.

### `sirio.brand_coral`

> Sirio's brand coral. Part measured, part chosen, and the seam between the
> two is the whole point.
>
> Measured: the hue, 24.3°, taken from the warm family the reference frames
> actually render (their inline-code tone, `#E0A882`, agreeing across three
> independent spans). Neither frame contains a coral to sample directly —
> both show one idle chat with no logo, caret, focus ring or activity dot,
> and a search of the whole frame finds zero pixels within 37 units of any
> coral — so hue is as much as looking can settle.
>
> Chosen: saturation 0.70 and lightness 0.60/0.40, against two constraints
> rather than taste. Each variant clears WCAG AA on the surface it is painted
> on (6.68:1 dark, 4.71:1 light against the Neutral ladder — the light step
> was re-derived when the light surface moved to `#E8E8E8`), held by
> `brand_coral_clears_contrast_on_its_own_surface`. And both stay clear of
> every `AgentBrandColor`, held by
> `worktree_activity_colours_name_the_agent_and_never_a_status`: a tab shows
> its coral and its agent's mark side by side, so a coral that lands on a
> brand makes the mark stop meaning anything. Claude's `#D97757` is the near
> one at 22 units, which is also why the obvious shortcut — reusing our own
> Swift's Claude fill for the coral — is the one coral this app cannot have.

### `ely.warning`, `ely.success`, `ely.danger`, `sirio.quantity`

> The state hues are not a fresh design problem: Sirio already shipped them.
> These four are the sRGB components of `App/AppTheme.swift`'s
> `tabNeedsInput`, `tabDone`, `tabError` and `tabFocusAccent`, transcribed
> digit for digit from our own macOS app, where they mark the same four
> things on the same tab strip. They are written as the float triples the
> Swift declares rather than as hex so the two files can be diffed by eye.
>
> Nothing here could have come off the reference frames anyway: both show one
> idle chat session — no error, no progress bar, no starred row, no terminal
> — so there is no pixel of any of these states to sample. Reusing our own is
> the strictly better answer than inventing a second vocabulary for a meaning
> we had already fixed.
>
> `accent` is the exception worth naming: in Swift this blue is the focus
> accent. The Rust brand colour is coral, which freed the blue, and a
> progress bar is the one place left that wants a cool hue.

### `ely.warning` (as `favorite`)

> A starred row is the warning hue, not a fifth colour. Sirio used to turn
> its own warning up to full chroma to make the star the louder of the two;
> bezel's warning already is at full chroma, so there is nothing left to turn
> up and the two are the same value.

### `ely.fg`

> bezel paints body text at full contrast against its page: #E5E5E5 on
> #0D0D0D is 15.4:1, #222222 on #F4F4F4 is 14.5:1. On a surface this dense — a
> sidebar, a tab strip and a file tree all in view — that reads as glare
> rather than as emphasis, so Sirio pulls the primary text one step back
> toward the surface it sits on. The result lands where Sirio's own text was
> before it adopted bezel (12.2:1 dark, 10.8:1 light), which is the target,
> not a taste: both are comfortably past WCAG AAA's 7:1, so nothing is spent.
>
> Only the primary rung moves. `text_muted` and below are already pulled back,
> and softening them too would collapse the ladder.

How far the primary text rung is pulled back toward its surface:

> How far the primary text rung is pulled back toward its surface. Chosen so
> the result lands on the contrast Sirio shipped before adopting bezel; held
> by `body_text_is_softened_off_bezels_full_contrast`.

(`TEXT_SOFTENING` = 0.10.)

### `ely.sunken`

> One step *into* the page, and derived from the measured surface for the
> same reason `sidebar` is: a well is a relationship to the page it is cut
> into, so it should move when the page does. The two factors differ because
> the move is not symmetric — dark has 26 units of headroom below the surface
> and can take a big step, light has 246 and would go grey long before it
> read as a well. Both were picked to make the well legible at a glance and
> neither is a measurement; `the_depth_ladder_reads_as_depth` holds the
> ordering.

### `sirio.terminal_surface`

> A terminal shares the pane surface in dark mode so its empty area cannot
> become a lighter grey than the pane around it. Light mode keeps the
> paper-white terminal surface.

> A grey ladder (Neutral's, Onice's) replaces two veil rules with solid
> fills: the terminal follows the page (Neutral's central panes are
> `#141414`), and both hovers are the ladder's own hover everywhere.

### `sirio.overlay`, `sirio.overlay_strong` (`VEIL_FAINT`, `VEIL_MID`)

> Everything from here to `danger_soft` is a veil off the ladder — see
> [`veil`] for why washes cannot be measured and must come from one rule
> instead.

> A chat row is most of the width of the pane. The same veil a sidebar row
> uses would read as a change of surface at that size, so the large-area
> hover sits one rung lower.
>
> bezel has no rung at 0.05 or 0.12; `wash` is its interactive-state helper
> and takes the alpha directly, so Sirio's faint and mid rungs survive as
> calls rather than as tokens of their own.

The two alphas Sirio still chooses for itself:

> This was a four-rung ladder (0.05 · 0.08 · 0.12 · 0.18), each rung half
> again the one below, because every wash token in the theme was `veil(rung)`
> and the ladder was the only free parameter in the lot. bezel now supplies
> the borders, hovers, selection and code wash, so two rungs have no consumer
> and the "ladder" no longer describes anything: what is left is the faint
> wash and the mid wash, quoted in dark-mode terms the way bezel quotes its
> own.

bezel's interactive-state wash at `alpha`, for a stated appearance:

> A mirror of `bezel::wash`, which exists only in the form that resolves
> against a process-global appearance (`bezel::paint::wash_for` is
> `pub(crate)`). `ThemeColors::for_appearance` builds both palettes in one
> process, so it cannot use the global form: dark and light would come out
> identical. The numbers are bezel's, copied — if bezel changes them this
> mirror has to follow.

### `ely.selection`

> A selection wash sits under its own text, so it has two jobs at once: be
> visible, and not swallow the glyphs. The top rung of the veil ladder is the
> strongest wash that still does both in either appearance;
> `selection_stays_under_its_text` holds the second half.

### `sirio.solid`, `sirio.on_solid`

> An inverted chip — a tooltip, a keycap — is literally the other
> appearance's page, so it is the same measured pair, swapped. No new number,
> and it stays right by construction if either is ever re-measured.

### `sirio.diff_add_bg`, `sirio.diff_del_bg`

> The band under a diff line is the line's own colour turned down, never a
> second green or a second red — see [`softened`].

The same colour at a lower opacity:

> Used for the soft fills that sit *under* text of the same meaning — a
> deleted diff line under red text, a danger banner under a danger label. The
> fill is not a second red to choose; it is the one red already chosen,
> turned down.

Mixes `fraction` of `target` into `color`, opaquely (used for the softened
body text):

> Distinct from [`softened`], which lowers alpha and lets whatever is behind
> show through: this states one opaque colour as a step from another toward a
> named second one, so the result does not depend on what it is drawn over.

### `ely.border` (as `border_opaque`)

> Collapsed onto `border`: bezel draws every seam as a hairline veil, so the
> opaque separator Sirio used to carry has no source any more. Kept as a name
> until phase 3 removes it, so this value change does not also move five call
> sites.

### `ely.hover`

> Measured off the seam itself, which is two frame pixels wide — one logical
> pixel at 2x — and flat at 200/200 in both variants, so these are solid
> values and not a blend of the surfaces either side.

### `sirio.tree_guide`

> A guide is an edge, so it scales with the surround like every other
> hairline rather than holding a fixed alpha.

bezel's hairline ink at `alpha`, for a stated appearance:

> Edges scale opposite to fills: a 1px line needs *more* ink on a bright
> surround, which is what [`bezel::theme::INK_HAIRLINE_SCALE`] carries.

### `sirio.dialog_surface`, `sirio.floating_surface`

> The opaque twins: same values, but `with_translucency_at` leaves them
> alone.

### Direct bezel reads

`ely.bg`, `ely.surface`, `ely.hover` (besides the seam note above),
`ely.active`, `ely.fg_muted`, `ely.fg_subtle`, `ely.link`, `sirio.canvas`
(the frame fallback), `sirio.border_strong`, `sirio.ring`,
`sirio.text_dim`, `sirio.code_wash`, `sirio.danger_muted`, `sirio.diff_add`
and `sirio.diff_del` are bezel's values read straight through
(`Rgba::from(bezel.…)`); `ely.fg_subtle` doubles as `git_untracked`, and
`ely.link` doubles as the file link. What makes the twins separate tokens is
that the translucency fade skips them.

## What bezel receives

`to_bezel_theme` is unchanged and still derives. The inverse adapter installs
today's derivation and writes no Sirio token into it:

> The inverse adapter installs today's derivation and writes no Sirio token
> into it. A frozen Sirio token is bezel's `Hsla` converted to `Rgba` and
> back, which matches the original on screen but not in its last float bits.
> Writing it back would make bezel's installed theme differ from today's in
> exactly those bits. Bezel's tokens and Sirio's agree in this sub-project as
> they agree today, by construction. Sub-project 7 revisits the adapter, when
> `bezel-markdown` is its only reader.

Concretely, `to_bezel_theme` is still `Theme::branded(Brand { tint })` for
the appearance plus the hand-written ladders: the Notte ladder painted onto
the seven surface tokens (`paint_notte_ladder`), and the Neutral/Onice solid
grey ladders (`paint_grey_ladder`). The tint stays `NONE` for the grey
ladders — this moves lightness only, so bezel widgets (via `to_bezel_theme`)
and Sirio tokens see one ladder by construction.

## Changing a colour

Edit `rust/crates/sirio_theme/src/presets.rs` deliberately. It was generated
once from the baseline dump by `Scripts/theme/freeze-presets.py`; after this
commit it is Sirio's source of truth, and the generator documents how it was
first written. There is no re-freeze step: change the value, keep the
per-token reasoning above honest, and re-run the dump comparison
(`Scripts/theme/compare-theme-dumps.py`) to prove nothing else moved.
