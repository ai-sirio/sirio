# Where every theme token comes from

This document exists because of `GAP-transplanted-theme-tokens.md`. The goal
freezes waku as Tiller's **visual** bar while forbidding its code, and those two
are only compatible if our palette is *measured off rendered frames* rather than
read out of `waku/src/theme.rs`. It previously was read out of the source. This
is the repair, and the audit trail that keeps it honest.

Re-run the measurement yourself:

```
./reference/waku/measure-theme.py
```

Frames: `reference/waku/app-screenshot-{dark,light}.png` — waku's own published
product screenshots (the ones on waku.sh), copied verbatim, 2266x1752 at 2x.

## IntelliJ-inspired shell (2026-08-20)

The source reference is the user-supplied
`Screenshot 2026-08-20 alle 19.39.17.png`. The approved sample extraction and
the corresponding light translation are recorded in
`docs/superpowers/specs/2026-08-20-intellij-inspired-translucent-shell-design.md`.
This shell layer is separate from the older waku-frame measurements below: it
defines translucent frame material, its opaque fallback, and the opaque panels
inside that frame.

The image is intentionally not copied into this repository. Read-only
verification on 2026-08-20 found a SHA-256 of
`ca3f249dd03bca9ea39fdd60235eca048f455c78c46ff4ec692c8b42fc06b662` and PNG
dimensions of **3802 × 2110** pixels (`sips`, `file`, Spotlight metadata, and
Pillow all agreed). This is the actual coordinate space used below; it differs
from the nominal 3840 × 2160 capture size, so the nominal size must not be used
to replay these rectangles.

Sampling used solid interior patches for surfaces and seams, and only opaque
glyph-core pixels for text (not antialiased edge pixels). Rectangles are
`x..x, y..y`, inclusive, in the verified 3802 × 2110 source coordinate space:

| Role | Rectangle(s) | Method |
| --- | --- | --- |
| outer frame | `0..3801, 10..77` | dominant non-control frame pixels |
| title frame | `1000..2500, 12..76` | dominant titlebar interior pixels |
| status frame | `1000..2500, 2048..2109` | dominant statusbar interior pixels |
| left panel interior | `60..930, 90..2000` | dominant flat interior pixels |
| centre panel interior | `990..2510, 150..2000` | dominant flat interior pixels |
| right panel interior | `2580..3730, 240..2000` | dominant flat interior pixels |
| active/selected row | `2580..3725, 160..210` | dominant row-fill pixels, excluding glyphs |
| panel border/seam | `959..966, 90..2000` | one-pixel seam/border runs, excluding corners |
| primary text | `67..160, 100..122` | repeated opaque glyph-core pixels in “Project” |
| meta/disabled text | `1076..1370, 1547..1594` | opaque glyph-core pixels in secondary empty-state copy |

The recorded extraction covers these regions and roles: the outer frame,
integrated titlebar, and status-bar frame; the interiors of the left, centre,
and right panels; an active/selected row; the panel seam/border; primary text;
and meta/disabled text. The resulting approved tokens are:

| Role | Dark | Light |
| --- | --- | --- |
| `frame_fallback` | `#222427` | `#DCE5E9` |
| `frame_surface` | `#222427` at 0.88 alpha | `#DCE5E9` at 0.82 alpha |
| `panel_surface` | `#18191A` | `#F4F7F8` |
| `selected_fill` | `#2D2F34` | `#D7E2E7` |
| `panel_border` | `#27292D` | `#CCD8DD` |
| primary text | `#CBCDD4` | `#313A40` |
| raw sampled meta/disabled text | `#686B71` | `#68757B` |
| final accessible secondary text | `#85888F` | `#667379` |

`background`, `sidebar`, and `chat_surface` deliberately alias
`panel_surface`, preserving compatibility for existing consumers while later
shell work adopts the semantic role directly. `canvas` maps to the opaque
`frame_fallback`; the fallback remains correct when platform translucency is
unavailable. Raised surfaces are `#1D1E21` / `#FBFCFC`, and `composer` aliases
that raised role. Insets remain a derivation of `panel_surface` with the
existing 0.72 dark and 0.93 light factors. The compact shell geometry is a
4px panel gap, 4px outer inset, and 7px shell-panel radius.

## Translucency variant (amended 2026-08-23)

The 2026-08-20 shell design kept panels opaque so the desktop could not
interfere with terminal, code, chat, or editor readability. That rule is
amended: when the translucency toggle is on **and** the resolved window
material is native blur (Windows/macOS), the app installs a theme variant in
which the structural surfaces are faded to **0.85 alpha** — `panel_surface`
and its aliases (`background`, `sidebar`, `chat_surface`, `chrome_tint`),
`raised` and its alias `composer`, `inset`, and `terminal_surface`. The fade
is strong enough to read as real translucency (the Swift-era 0.96 was
imperceptible) yet safe to composite: the frame material behind the panels
is a near-identical grey in both appearances, so a panel at 0.85 alpha
barely shifts in hue and keeps its text readable.

The frame material keeps its designed alphas (0.88 dark / 0.82 light) and
is not faded twice; washes, borders, selection, and text keep full opacity.
The variant is re-derived from `mode` + `appearance` (`Theme::with_translucency`),
so the opaque base is always recoverable and every theme reinstall (`install`,
`set_mode`, the portal follower) preserves the flag by construction. On
platforms without native blur the toggle changes nothing visually — the
spec's opaque-fallback rule still holds.

The raw sampled meta/disabled values are **not** the final secondary-body
values. The final `#85888F` / `#667379` secondary tokens are an explicit
accessibility adjustment: both primary and secondary body roles clear WCAG AA
(4.5:1) against their respective opaque panel surfaces. `panel_focus_ring`
remains the established accent.

This shell change does not retune the terminal surface or terminal ANSI palette,
syntax colours, diff/status hues, agent-brand colours, or activity-status
colours; those meanings stay byte-for-byte stable. The shell palette changes
only structural shell and text roles.

## Historical waku measurements (superseded where noted)

The sections below document the earlier 2266 × 1752 waku screenshots. They are
historical evidence, not the current source of shell values. In particular,
their `surface`, `composer`, `raised`, `text`, `text_tertiary`, and
`sidebar_border` values are superseded by the 2026-08-20 shell roles above.
The dark terminal baseline and inline `code_text` measurement remain active;
the terminal and syntax palettes were deliberately not retuned. Other legacy
wash and semantic discussions remain active only where the current token is
still explicitly derived from them.

## What the measurement can and cannot settle

Both frames are 8-bit palettized PNGs holding 255 distinct colours. Large flat
areas get their own palette entry and survive intact; thin antialiased detail is
an approximation. Every conclusion below rests on regions large enough that
quantization cannot explain the result.

Patches are sampled as *patches*, and the share of the patch the winning colour
occupies is reported. That number is the finding, not bookkeeping: 100% means
the region is genuinely flat and the value is trustworthy; well under 100% means
the region is not flat, and asking why is how two of the surprises below turned
up.

## Historical tokens the waku frames settled

These values describe the prior waku-led palette. The rows identified above as
superseded are retained for audit history, not as current `ThemeColors` values.

| Token | Frame value (dark / light) | Coverage | Sampled at |
|---|---|---|---|
| `surface` | `#1A1A1A` / `#F6F5F6` | 100% / 100% | `80x80+887+351`, transcript above the first bubble |
| `composer` | `#212121` / `#FFFFFF` | 100% / 100% | `80x40+1608+1360`, composer right of its placeholder |
| `raised` | `#232323` / `#EBEBEB` | 100% / 75% | `80x10+1814+290`, user bubble above the cap height |
| `text` | `#E2E2E2` / `#242424` | glyph core | `90x26+700+548`, bold body text "Waku" |
| `text_tertiary` | `#7C7D7D` / `#868686` | glyph core | `180x26+1330+462`, the "Worked for 10 seconds" line |
| `sidebar_border` | `#282828` / `#DCDBDB` | 100% / 100% | seam column `x=662..663` |
| `code_text` | `#E0A882` / `#9A5528` | 3 spans agree | inline code at `+1690+545`, `+720+855`, `+1830+855` |

Two of these historical values moved by one step against what the source had carried:
`text_tertiary` measures `#7C7D7D`/`#868686` where the transplant said
`#7D7D7D`/`#858585`, and light `raised` measures `#EBEBEB` where it said
`#ECECEC`. A one-unit disagreement is what a real measurement looks like. The
measurements remain recorded here even where the current shell intentionally
uses a different role.

The seam is worth its own line. It is exactly two frame pixels wide — one
logical pixel at 2x — and flat at 200/200 in both variants, so `#282828` and
`#DCDBDB` are solid, not a blend:

```
x=661  #21282A  95/200      <- sidebar, not flat (see below)
x=662  #282828  flat        <- the seam
x=663  #282828  flat
x=664  #1A1A1A  flat        <- content
```

That replaces `hsla(126.93, 0.000_000_1, 0.16077, 1.0)`, which was the single
piece of evidence no innocent explanation covered: seven significant digits and
Rust underscore grouping do not come out of a screenshot. For the record it
rendered `#292929`, one step off the measured `#282828`.

## Tokens the frames refuse to settle

This is the part that could not have been reached by re-typing waku's numbers
more carefully, and it is why the remedy is not merely cosmetic.

### `sidebar` — translucent, so it has no fixed value

The sidebar patch came back `#21282A` at only **57%** coverage, with a cyan cast
(G and B above R). The content area, sampled the same way at the same heights,
is neutral and flat at every one of them. Probing both against the desktop
visible beside the window:

| y | sidebar | content | desktop behind the window |
|---|---|---|---|
| 1000 | `#21282A` | `#1A1A1A` | `#88DAF7` |
| 1300 | `#22282A` | `#1A1A1A` | `#F1FAFC` |
| 1560 | `#26292A` | `#1A1A1A` | `#E4F8FF` |

The sidebar tracks the wallpaper and the content does not. The light frame says
it more plainly still — the sidebar climbs `#EAF1F3` → `#F1F3F4` as the desktop
behind it goes `#86DCF9` → `#E5F8FF`, monotonically.

waku's sidebar is a macOS vibrancy layer. **Its rendered colour is a function of
whatever is behind the window**, so no screenshot can yield the constant, and
`0x181818` therefore cannot have been sampled from one — independent
confirmation of the transplant, arrived at by measuring rather than by reading
waku's source.

It also has a consequence beyond provenance: macOS vibrancy does not exist on
Linux or Windows, so the copied constant was a number that **is not a constant
in its own context**. Tiller's sidebar is opaque and must be chosen. We choose
it as a stated derivation from our measured `surface` — recessed a step, because
the reference frames do show a sidebar visually distinct from content, which is
the part a screenshot *can* establish.

### `accent` — not in either frame

`#E2795B` appears **0 times** in the dark frame; `#C85F44` appears **0 times** in
the light one. Nearest neighbours are 37–41 units away, and are identifiable as
other things: `#FF5C5F` is macOS's own traffic-light red, `#A8704B`/`#9A5528` is
inline-code text. That gap is far outside palette-snapping error for any colour
occupying real area, and quantization preferentially *preserves* high-area
colours.

The frames contain no logo, caret, focus ring, or live-activity indicator — no
brand moment at all — so the accent is simply not on display. The warm colour
that *is* on display is inline-code text, consistent across three independent
spans at `#E0A882` (dark) / `#9A5528` (light). That is a syntax colour, not a
brand accent, and pressing it into service as one would be a different mistake.

So the accent is split: **hue measured, saturation and lightness chosen.**

The hue is `24.3°`, taken from the warm family the frames do render — the
inline-code tone above, `#E0A882`, agreeing across three independent spans.
Looking can settle that much and no more.

Saturation `0.70` and lightness `0.60` / `0.40` are ours, and are pinned by two
constraints rather than by taste. Both are tests, because both were nearly got
wrong:

**It must stay legible on its own surface.** The light variant's lightness is
the first step down that clears WCAG AA.

| | value | contrast on its own surface |
|---|---|---|
| dark accent | `#E08B52` on `#1A1A1A` | 6.62:1 |
| light accent | `#AD581F` on `#F6F5F6` | 4.61:1 |
| *the value this replaces* | `#C85F44` on `#F6F5F6` | **3.73:1** |

The transplanted light accent was **below the 4.5:1 line**. Copying a constant
out of another project imports its trade-offs unexamined along with its number,
and this one had a contrast defect in it.
`accent_clears_contrast_on_its_own_surface` holds the rule now.

**It must not be any agent's brand colour.** A tab row paints its accent and its
agent's mark together, so an accent that lands on a brand makes that mark stop
distinguishing anything — every row looks identically tinted whichever agent is
running. This is not hypothetical. The first repair attempted here set the
accent to `#D97757` from our own `App/AgentIcon.swift:68`, reasoning that our
own Swift is unimpeachable provenance. It is — but `#D97757` *is*
`AgentBrandColor::Claude`, and
`worktree_activity_colours_name_the_agent_and_never_a_status` failed
immediately. The tempting shortcut was the one coral this app cannot have.

Distances from the accent as it now stands: Claude 22, Codex 189, OpenCode 88,
Pi 205, Omp 148. `accent_is_not_any_agent_brand` holds it in `tiller_theme`
too, next to the values, rather than only in the app crate that noticed.

### Correction, 2026-08-19: this section used to be a blanket, and the blanket was false

What stood here said the remaining tokens were "ours by choice" and that the
code "says so at each one." A fresh critic checked that claim against
`waku/src/theme.rs` and refuted it
(`docs/linux-rewrite/fullapp/CRITIC-theme-transplant.md`). Sixteen values were
still exact transcriptions — six as hex (`inset`, `warning`, `success`,
`danger`, `gauge`, `favorite`) and ten as identical hsl parameters plus
identical alphas (`row_hover`, `border`, `border_strong`, `overlay`,
`overlay_strong`, `selection`, `code_wash`, `inverse`, `on_inverse`,
`danger_soft`). "Ours by choice" was not a provenance claim at all; it was a
sentence standing where one should have been. The critic was right, and the
strongest form of its argument is the one this document should have made
itself: **a translucent wash cannot be measured from a screenshot in
principle** — compositing has already happened by the time the shutter closes,
so no pixel anywhere carries a wash's rgba back. There was never a "we measured
and it happened to match" story available for those ten. Five of them sharing
one hue, one saturation and one lightness with the reference, each at its
matching alpha, in both appearances, is not a coincidence anyone should be
asked to believe.

All sixteen are now derived. What follows is the whole set, and none of it is a
free number except the four-rung alpha ladder, which is named as such.

### The state hues — from our own macOS app, not from anywhere else

`warning`, `success`, `danger` and `gauge` are the sRGB triples of
`App/AppTheme.swift`'s `tabNeedsInput`, `tabDone`, `tabError` and
`tabFocusAccent`. Tiller already shipped these four, for the same four meanings
on the same tab strip; inventing a second vocabulary for a meaning we had
already fixed would have been the worse answer even if provenance were not at
stake. They are written in Rust as the float triples the Swift declares, not as
hex, so the two files diff by eye.

`gauge` is worth one line: in Swift that blue is the *focus* accent. The Rust
accent is coral, which freed the blue, and a progress bar is the one place left
that wants a cool hue.

This is not a citation in a comment — comments decay.
`the_state_hues_are_the_ones_the_swift_app_ships` reads `App/AppTheme.swift` at
test time, parses the four `dynamic(light:…, dark:…)` declarations, and fails if
the Rust and the Swift ever part company. Perturbing one channel by 0.04 fails
it; that control was run.

| token | dark | light | source |
| --- | --- | --- | --- |
| `warning` | `0.95, 0.72, 0.28` | `0.67, 0.42, 0.02` | Swift `tabNeedsInput` |
| `success` | `0.48, 0.78, 0.57` | `0.10, 0.45, 0.22` | Swift `tabDone` |
| `danger` | `0.94, 0.43, 0.47` | `0.68, 0.12, 0.17` | Swift `tabError` |
| `gauge` | `0.55, 0.64, 1.00` | `0.24, 0.38, 0.78` | Swift `tabFocusAccent` |

`favorite` is not a fifth colour: a starred row is a louder `warning`, so it is
`warning`'s own hue and lightness at full chroma, computed by
`hue_and_lightness` rather than written down. Held by
`favorite_is_the_warning_hue_at_full_chroma`.

### The ten washes — one rule, because measurement is unavailable

Every hairline, hover, overlay and wash is now `veil(rung)`: **white over dark,
black over light, no hue at all.** That is not a workaround for missing
provenance, it is what this theme's own module header already said — *surfaces
step by lightness alone, and colour is spent only where it means something*. A
hairline means nothing; it is shape. The tinted `hsla(220, 10%, …)` values did
not merely lack provenance, they contradicted the rule printed at the top of
the file they lived in.

`no_structural_token_carries_a_hue` holds it, and re-tinting a single veil the
old way fails that test — control run.

The one free parameter is the alpha ladder, deliberately concentrated in four
numbers with a stated relationship instead of scattered across twenty without
one. Each rung is half again the one below (`the_veil_ladder_is_geometric`):

| rung | alpha | used by |
| --- | --- | --- |
| `VEIL_FAINT` | 0.05 | `overlay`, `chat_row_hover` (large areas, must barely register) |
| `VEIL_LOW` | 0.08 | `border`/`hairline`, `row_hover`, `code_wash` |
| `VEIL_MID` | 0.12 | `overlay_strong`, `tree_guide`, the soft fills |
| `VEIL_HIGH` | 0.18 | `border_strong`, `tab_chip_underline` |

### The rest, each stated as a transformation of something measured

| token | rule | held by |
| --- | --- | --- |
| `inset` | the measured `surface` scaled — 0.72 dark, 0.93 light. The factors differ because the move is not symmetric: dark has 26 units of room below the page and can take a big step, light has 246 and would go grey long before it read as a well. | `the_depth_ladder_reads_as_depth` |
| `terminal_surface` | dark: the same well as `inset`. light: paper. | the palette snapshots |
| `inverse` | the other appearance's page — the measured pair, swapped. No new number, and it stays correct by construction if either is ever re-measured. | `inverse_is_the_other_appearances_page` |
| `on_inverse` | likewise, the other appearance's body text. | same |
| `selection` | the accent at 0.45 dark / 0.30 light. Selection *is* focus, and focus is what the accent is for; the alphas are the loudest each appearance takes while the glyphs underneath still clear WCAG AA. | `selection_stays_under_its_text` |
| `danger_soft`, `diff_deletion_background` | `danger` turned down to `VEIL_MID` — the red already chosen, not a second red picked to sit near it. | `soft_fills_are_their_own_meanings_colour` |
| `diff_addition_background` | `success` turned down the same way. | same |

The previous `selection` was `hsla(211, 100%, 50%)`, the browser blue. Dropping
it is a real design change and not only a provenance one: a coral app that
selects text in Chrome's blue is borrowing a convention it does not otherwise
follow.

## Geometry, which has the same problem and less of a cure

The gap report named geometry alongside colour — `conformance.rs` records
replacing Tiller's own 704 and 800 content widths with a `CONTENT_MAX_WIDTH` of
720 read out of `waku/src/app.rs:80`. Same vector, so the same question: what
can a frame settle?

**Measurable, and it confirms what the source said.** The selected session card
fills a flat `#2F3436` from `y=444` to `y=545` at `x=610` — 102 frame pixels,
**51 logical px** at 2x. That is exactly the `7 + 18 + 4 + 15 + 7` the sidebar's
card math claims, arrived at by looking instead of by reading. The
sidebar/content seam measures one logical pixel, likewise (`x=662..663`).

**Not measurable, and the reasons differ.**

- **Bar heights** (the 48px header, the 40px footer). Both sit on the same
  `#1A1A1A` as the content, with no divider: `titlebar`, `surface` and `footer`
  all sample identical at 100% coverage. A boundary that paints nothing cannot
  be found in a picture of it.
- **Sidebar width.** It is user-resizable, so the frame shows one user's choice
  (~251 logical px here), not a constant — the same category of error as the
  sidebar *colour*, for a different reason.
- **The 720 content column.** The content pane in this frame is ~725 logical px
  wide, so a 720 cap would be engaged by five pixels. That cannot be told apart
  from text simply filling the pane, and claiming otherwise would be reading the
  answer into the measurement.

So geometry is a smaller win than colour: one number confirmed, three that a
screenshot provably cannot settle and which therefore have to be ours by
argument. That is the next piece, and it is deliberately *not* claimed here.

## The rule this all turns on

Sampling a pixel from a rendered reference frame and arriving at `#1A1A1A` is
inspiration, and is allowed, even though the number ends up identical. Copying
`0.000_000_1` out of a `.rs` file is not, and no visual argument rescues it.
**Provenance, not the value, is what the contract governs.**

Which is why roughly half this palette kept its numbers and all of it changed
its status.

And it is why the first pass at this document was not enough. Getting the five
loudest tokens right while sixteen quieter ones kept their transcribed values
is not a partial success at the contract; the contract is per-value. The second
pass answers it the only way that scales: **no token in `tiller_theme` is a
free number any more.** Each one is a measurement recorded here, a value our own
Swift app already shipped, or a stated transformation of one of those — with a
test holding the rule rather than the digits. Seven of the eight rules above
have a test named next to them, and the two doing the most work
(`no_structural_token_carries_a_hue`,
`the_state_hues_are_the_ones_the_swift_app_ships`) were checked with positive
controls: perturb the value, watch the test fail, restore.
