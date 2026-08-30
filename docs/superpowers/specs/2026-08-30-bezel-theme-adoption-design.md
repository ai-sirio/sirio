# Bezel theme adoption — design

Date: 2026-08-30
Status: approved, pending implementation plan

## Problem

Sirio maintains its own theme system (`sirio_theme`, 4357 lines) alongside a
dependency on `bezel`, which ships a complete design system of its own. The two
overlap in colours, radii, spacing and font families, and diverge in naming: a
value that exists in both is called one thing in `ThemeColors` and another in
`bezel::theme::Theme`. Every new component has to be told which of the two to
read from, and the answer is not derivable from the code.

The goal is one source of truth for the look: `bezel::theme` supplies it, and
`sirio_theme` retains only what is Sirio's own domain.

## Scope

Decided during brainstorming, in this order:

- **B3** — colours, typography, spacing and radii all move to bezel, and the
  `cosmic/` module (Pop!_OS COSMIC design tokens, 1243 lines) is removed with
  `titlebar.rs` rebased onto bezel tokens. The cost — losing deliberate Pop!_OS
  adherence on Linux — was raised and accepted.
- **T2** — bezel is the *source* of tokens, not a pixel target. Sirio's own
  layout measurements are preserved, but re-expressed as ratios of `SPACE_*` /
  `BASE_RADIUS` rather than as literals. Heights do not move: `bottom_bar_height`
  stays 40px, written as a multiple instead of `px(40.0)`.
- **A1** — rename first, then swap values, then collapse. See "Phases".

Out of scope: changing any layout measurement, adopting bezel's heights
(`STATUS_STRIP_HEIGHT` 24 vs Sirio's 40), and the still-open macOS CI gate.

## Current state

### Consumers

| Crate | `theme.*` references |
|---|---|
| `sirio_ui` | 1339 |
| `sirio_theme` | 173 |
| `sirio` | 153 |
| `sirio_terminal` | 47 |

That figure counts every `theme.<field>` access. Broken down by what actually
moves:

| Access | Sites | Fate |
|---|---|---|
| `theme.typography` | 205 | untouched |
| `theme.radii` | 157 | untouched (redefined internally) |
| `theme.spacing` | 55 | untouched (redefined internally) |
| colour fields via `Deref` | ~704 | renamed |
| `theme.cosmic` | 59 | rewritten |
| `theme.colors.*` | 21 | lose the `.colors` prefix |

The colour renames concentrate heavily: `title` 160, `meta` 117, `subtitle` 77,
`row_hover` 73, `hairline` 56 — five names are 69% of the 704.

### The Deref that makes this tractable

`sirio_theme::Theme` already implements `Deref<Target = ThemeColors>`
(`lib.rs:1241`). Call sites therefore write `theme.title`, not
`theme.colors.title`. Repointing that `Deref` at `bezel::theme::Theme` leaves
every access whose *name* already matches working untouched.

The facade is kept, not dissolved. `sirio_theme::Theme` remains the single
access point and gains `bezel::ThemeExt`.

### Four appearance enums

| Enum | Location | Role |
|---|---|---|
| `sirio_persistence::AppearanceMode` | `model.rs:334` | persisted; has a serde contract |
| `sirio_project::ui::AppearanceMode` | `ui.rs:3` | duplicate |
| `sirio_theme::ThemeMode` | `lib.rs:46` | the theme's own |
| `bezel::theme::appearance::AppearanceMode` | bezel | `System`/`Light`/`Dark`, already `Serialize`/`Deserialize` |

`main.rs:14639` and `:14676` are two `match` blocks translating between them.

## Target architecture

### Supplied by bezel

| Concern | Source |
|---|---|
| Colours (~40 tokens) | `bezel::theme::Theme` (`Hsla`) |
| Light/Dark + System + persistence | `bezel::theme::appearance::{AppearanceMode, resolve, init, observe_window, apply}` |
| Radius ladder | `BASE_RADIUS` + `surface_radius()` / `panel_radius()` / `button_radius()` / `control_radius()` / `bubble_radius()` / `inset_radius()` |
| Spacing steps | `SPACE_XS/SM/MD/LG` |
| Font families | `Theme::font_sans` / `font_mono` + fallbacks |
| Syntax | `bezel::theme::SyntaxPalette` |
| Glass / translucency | `Brand::glass`, `GLASS_ALPHA`, `set_glass_bevel`, `set_glass_magnify` |
| Tint / accent / radius globals | `Brand { tint, accent, radius, glass }` + `set_brand` |

### Retained in `sirio_theme`

No bezel counterpart exists for these; they are Sirio's domain:

`SirioColors` (see Group C) · `Typography` (scale + `for_base_size`) ·
`AgentBrandColor` · `graph_lane()` · `BrowserChrome` · `WindowsCaption` ·
`TERMINAL_FAMILY_CANDIDATES` + `resolve_terminal_family` · the layout roles bezel
has no concept of (`menu_width`, card shadows, `titlebar_control_frame`,
`titlebar_control_spacing`, `compact_action`, `shell_gap`, `shell_outer_inset`).

Per T2 these keep their current values but are written as ratios of `SPACE_*` /
`BASE_RADIUS`.

### Removed

- `ThemeColors` — absorbed by `bezel::theme::Theme`
- `sirio_theme::ThemeMode` — absorbed by `bezel`'s `AppearanceMode`
- `sirio_project::ui::AppearanceMode` — duplicate
- `cosmic/` (1243 lines), with `titlebar.rs` rebased
- `rust/assets/fonts/` and `sirio::register_fonts`

`sirio_persistence::AppearanceMode` is retained and becomes the sole conversion
point, replacing the two `match` blocks in `main.rs`.

### Fonts

Sirio currently pulls `bezel = { default-features = false }`, which disables
`geist-sans` / `geist-mono` / `geist-weights`, and ships its own copies in
`rust/assets/fonts/` (Geist-Regular, Geist-Medium, GeistMono-Regular).

Enable the three features; delete the local assets and `register_fonts`.

This is a bug fix as well as deduplication. bezel also ships `Geist-SemiBold.ttf`
and `Geist-Bold.ttf`; Sirio does not. Per the comment in
`bezel/crates/ui/src/lib.rs`, gpui's cosmic-text path (Linux) rasterizes variable
fonts at their default instance only and never applies `wght` coordinates — so
with only the variable TTF registered, semibold and bold paint silently at 400.
Sirio's Linux build therefore cannot currently paint semibold or bold, and
nothing reports it. macOS was unaffected: CoreText applies the variable axis
natively.

Consequence: B3 brings Geist to macOS too, so `UI_FAMILY_CANDIDATES` and
`CODE_FAMILY_CANDIDATES` lose their `#[cfg(target_os = "macos")]` branch with
SF Pro / SF Mono, and `macos_keeps_the_apple_faces` is retargeted or removed.

## Token mapping

`ThemeColors` has 65 public fields, but they are **not 65 decisions**. Reading
`ThemeColors::for_appearance` (`lib.rs:307`) shows the fields are aliases over
roughly 28 local bindings. The bindings are where the design lives; the fields
are bookkeeping.

The mapping is therefore two tables: bindings (the decisions) and fields (the
mechanical resolution). Both are derived from the code, not from doc-comments.

### The veil ladder

Sirio's translucent tokens all come from one helper:

```rust
fn veil(alpha: f32, appearance: Appearance) -> Rgba {
    match appearance {
        Appearance::Dark => color(1.0, 1.0, 1.0, alpha),
        Appearance::Light => color(0.0, 0.0, 0.0, alpha),
    }
}

const VEIL_FAINT: f32 = 0.05;
const VEIL_LOW:   f32 = 0.08;
const VEIL_MID:   f32 = 0.12;
const VEIL_HIGH:  f32 = 0.18;
```

Four rungs, each roughly half again the one below — held by
`the_veil_ladder_is_geometric`.

bezel has the same concept as three free functions in `paint.rs`:

| Sirio | bezel | Difference |
|---|---|---|
| `veil(a)` as a fill | `ink(a)` | none — `INK_FILL_SCALE = 1.0`, only the tone flips, exactly as `veil` does |
| `veil(a)` as a border / divider / ring | `hairline(a)` | bezel scales by `INK_HAIRLINE_SCALE = 1.35` in light mode, so a 1px edge survives a bright surround |
| `veil(a)` as a hover / press wash | `wash(a)` | bezel softens short of pure black/white, so plates read as tinted glass |

Two values already agree exactly: Sirio's `border` is `veil(VEIL_LOW)` = 0.08 and
bezel's `border` is `white/0.08`; the same holds for `code_wash`.

The hairline scaling is a behaviour change, not a value copy: Sirio uses the same
alpha in both appearances, bezel brightens edges in light mode. This is a
deliberate improvement and is why `hairline` maps to `hairline(a)` rather than to
`ink(a)`.

### Table 1 — bindings (the decisions)

| Sirio binding | Current value | bezel | Note |
|---|---|---|---|
| `text` | `#CBCDD4` / `#313A40` | `text` | |
| `text_secondary` | `#85888F` / `#667379` | `text_muted` | |
| `text_tertiary` | `#686B71` / `#68757B` | `text_faint` | |
| `text_ghost` | `#575757` / `#A4A4A4` | `text_dim` | |
| `frame_fallback` | `#222427` / `#DCE5E9` | `bg` | |
| `frame_surface` | `softened(frame_fallback, 0.88/0.82)` | **retained in `SirioColors`** | bezel's `band` is a recessed palette/picker header or footer strip, with black at alpha 0.16 in dark mode and 0.045 in light mode, not Sirio's translucent window-frame material. |
| `panel_surface` | `#18191A` / `#F4F7F8` | `surface` | |
| `panel_border` | `#27292D` / `#CCD8DD` | `border` | opaque today, translucent in bezel — a real value change |
| `raised` | `#1D1E21` / `#FBFCFC` | `surface_raised` | |
| `inset` | `scaled(panel_surface, 0.72/0.93)` | `input_bg` | |
| `selected_fill` | `#2D2F34` / `#D7E2E7` | `element_active` | |
| `inverse` | `#F4F7F8` / `#18191A` | `solid` | |
| `on_inverse` | `#313A40` / `#CBCDD4` | `on_solid` | |
| `terminal_surface` | `scaled(SURFACE_DARK, 0.72)` / white | **retained** | bezel has no terminal-surface concept |
| `border` | `veil(0.08)` | `border` | exact match |
| `border_strong` | `veil(0.18)` | `border_strong` | bezel is `white/0.14` — closest rung |
| `row_hover` | `veil(0.08)` | `element_hover` | bezel is `0.11` |
| `overlay` | `veil(0.05)` | `wash(0.05)` | the faint rung; bezel has no token at this level |
| `overlay_strong` | `veil(0.12)` | `wash(0.12)` | between bezel's hover and active |
| `selection` | `veil(0.18)` | `selection` | bezel's is blue, `hsla(0.66, 0.6, 0.55, 0.35)` |
| `code_wash` | `veil(0.08)` | `code_wash` | exact match |
| `warning` | `rgb(0.95,0.72,0.28)` / `rgb(0.67,0.42,0.02)` | `warning` | |
| `success` | `rgb(0.48,0.78,0.57)` / `rgb(0.10,0.45,0.22)` | `success` | |
| `danger` | `rgb(0.94,0.43,0.47)` / `rgb(0.68,0.12,0.17)` | `danger` | |
| `danger_soft` | `softened(danger, 0.12)` | `danger_muted` | |
| `favorite` | `warning` at full chroma | `warning` | doc: "the warning hue at full chroma" |
| `gauge` | `rgb(0.55,0.64,1.00)` / `rgb(0.24,0.38,0.78)` | `accent` | C1 — the blue goes; see "The blue" |
| `accent` | coral, hue 24.3°, sat 0.70, light 0.60/0.40 | **retained**, renamed `brand_coral` | see "The blue" |

**Trivial aliases among the bindings**, collapsing with the fields:
`composer = raised` · `sidebar_border = panel_border` · `code_text = text`

### Table 2 — fields to bindings (mechanical)

Read directly from the `ThemeColors { .. }` literal. Nothing here is a judgement
call; it is the current code.

| Binding | Fields resolving to it |
|---|---|
| `text` | `title`, `title_selected`, `tab_focus_accent`, `selection_ring`, `primary_text_color`, `caret`, `code_text` |
| `text_secondary` | `subtitle`, `panel_focus_ring` |
| `text_tertiary` | `meta` |
| `text_ghost` | `text_ghost` |
| `panel_surface` | `panel_surface`, `background`, `chat_surface`, `chrome_tint`, `sidebar` |
| `panel_border` | `panel_border`, `tab_chip_underline`, `sidebar_border` |
| `frame_fallback` | `frame_fallback`, `canvas` |
| `frame_surface` | `frame_surface` |
| `terminal_surface` | `terminal_surface` |
| `raised` | `raised`, `composer`, `primary_pill_bg`, `card_fill` |
| `inset` | `inset`, `filter_field_bg`, `code_inset_fill` |
| `selected_fill` | `selected_fill`, `selection_fill`, `primary_action_bg` |
| `border` | `border`, `hairline` |
| `border_strong` | `border_strong`, `rail_task`, `rail_edit`, `rail_tool` |
| `row_hover` | `row_hover` |
| `overlay` | `overlay`, `chat_row_hover` |
| `overlay_strong` | `overlay_strong` |
| `selection` | `selection` |
| `code_wash` | `code_wash`, `diff_hunk_background` |
| `warning` | `warning` is not a field; `tab_needs_input`, `git_modified`, `rail_question` |
| `favorite` | `favorite` |
| `success` | `tab_done`, `git_staged`, `diff_addition` |
| `danger` | `tab_error`, `git_conflict`, `diff_deletion` |
| `danger_soft` | `danger_soft` |
| `gauge` | `gauge`, `git_untracked`, `file_link` |
| `accent` | `accent` |
| `inverse` / `on_inverse` | `inverse` / `on_inverse` |
| `veil(VEIL_MID)` inline | `tree_guide` |
| `softened(success, VEIL_MID)` inline | `diff_addition_background` |
| `softened(danger, VEIL_MID)` inline | `diff_deletion_background` |

Total: 65 fields.

### What this changes versus a field-by-field reading

Four rows that a doc-comment reading got wrong, and which the code settles:

| Field | Doc-comment suggested | Code says |
|---|---|---|
| `primary_action_bg` | a gap — "must stay distinguishable from `raised`" | `= selected_fill`; the doc is correct and it is simply not `raised` |
| `primary_pill_bg`, `card_fill` | `solid`, `surface_card` | both `= raised` |
| `rail_task`, `rail_edit`, `rail_tool` | a neutral text step | `= border_strong` |
| `chat_row_hover` | derive at lower alpha | `= overlay`, already the lower rung |

There are no collisions to resolve: `row_hover` (0.08) and `overlay` (0.05) are
different rungs, and `selected_fill` (an opaque hex) and `overlay_strong` (a
veil) are different kinds of value.

`primary_action_bg` was listed as risk R3's confirmed suspect. It is resolved:
it is an alias of `selected_fill`, and needs no judgement.

### Group C — no bezel counterpart (3)

After resolving against the code, only three bindings have no bezel home:

| Sirio | Resolution |
|---|---|
| `frame_surface` | retain in `SirioColors` |
| `terminal_surface` | retain in `SirioColors` |
| `accent` → `brand_coral` | retain, **renamed**. Doc: "no role paints it any more"; only the Coral entry of the agent-colour picker reads it. Carries two invariants (clears AA on its own surface; is not any agent's brand). Renamed because `theme.accent` now resolves through `Deref` to bezel's `accent`, which is a different thing. |

`overlay` and `overlay_strong` sit between bezel's rungs rather than outside
them, and are expressed as `wash(0.05)` and `wash(0.12)` rather than retained as
tokens.

#### The blue

bezel's palette has no blue role for data. Its `accent` is `neutral(0.673)` —
documented as "indigo-400's lightness, no chroma", i.e. the lightness of indigo
without the indigo. Its chromatic hues are red, amber, emerald and pink (`busy`).
Note that bezel's *text selection* is blue (`hsla(0.66, 0.6, 0.55, 0.35)`), so
blue does not vanish from the app — only from data and status.

Sirio uses one blue binding, `gauge`, for three documented meanings: quantity
(`gauge`), clickability (`file_link`), and untracked git status
(`git_untracked`).

**Decision (C1):** drop the blue. `gauge` maps to bezel's `accent` — the neutral
that sits where an accent colour would, which is exactly what a progress bar
needs.

`git_untracked` additionally moves off the binding to `text_faint`, quieter than
the chromatic staged / modified / conflict states.

**This split is structural, not a value change**, because `git_untracked` and
`gauge` are the same binding today. It therefore happens in Phase 1 as an
explicit un-aliasing, not in Phase 2. See Task ordering.

Rejected: keeping a declared Sirio blue (a permanent deviation from the
palette), and mapping `git_untracked` to `busy` (pink reads as error in a git
list).

This satisfies `gauge`'s documented intent rather than contradicting it. The doc
says "a progress bar is never painted in a status hue — a bar filling up is not
an alert"; the requirement was *not a status hue*, not *blue specifically*, and a
neutral meets it.

The real loss is `file_link`: clickability no longer has a colour signal, and
must be carried by underline or hover instead. This is bezel's own convention —
contrast for identity, colour for data — so it is consistent, but it is a
behaviour change to verify in Phase 2's visual review, not just a token swap.

If the coral is ever wanted back as the app accent, the supported route is
`set_brand` with `Brand { accent: Tint::new(<coral hue>, <chroma>), .. }`, which
rotates the whole palette coherently, rather than a bespoke token.

## Phases

### Phase 1 — Rename. Values frozen.

1. Review Table 1 (28 bindings); settle the one **review** row
   (`frame_surface` to `band`) against the current value
2. Un-alias `git_untracked` from `gauge` — a structural split required by C1,
   and the only structural change in the whole migration. Values stay identical
   at this step: `git_untracked` keeps `gauge`'s value under its own binding,
   and only moves to `text_faint` in Phase 2
3. Rename the bindings to their bezel names, values unchanged
4. Rename `ThemeColors` fields; the aliases in Table 2 collapse onto the renamed
   bindings
5. Follow through ~704 call sites; the 21 `.colors` sites lose the prefix
6. Tests change *names*, never numbers

**Gate: no numeric literal changes anywhere in the Phase 1 diff.** Scriptable.
The provenance tests stay green asserting the same values as before — evidence
that nothing moved.

### Phase 2 — Swap. Structure frozen.

1. Repoint `Deref::Target` to `bezel::theme::Theme`; `Theme::for_appearance`
   builds from `Theme::dark()` / `light()` plus `Brand`
2. Group C lands: retained tokens into `SirioColors`, derived tokens computed
3. Fonts: enable the three geist features, delete `rust/assets/fonts/` and
   `register_fonts`, drop the macOS SF branch
4. Rewrite `Spacing` / `Radii` as ratios of `SPACE_*` / `BASE_RADIUS` —
   **identical values** (T2)
5. Retarget the provenance tests to bezel, in a commit that does nothing else
6. Write the new provenance document (see R4)

**Gate: no file outside `sirio_theme` appears in the Phase 2 diff.** Scriptable.
Invariant tests stay green on their own; the rest is visual review against
`cargo run -p gallery`.

### Phase 3 — Collapse.

1. Remove `cosmic/`; rebase `titlebar.rs` onto bezel tokens — 59 sites
2. `sirio_theme::ThemeMode` → bezel's `AppearanceMode`; delete
   `sirio_project::ui::AppearanceMode`; `sirio_persistence::AppearanceMode`
   becomes the sole conversion point, replacing `main.rs:14639` and `:14676`
3. `sirio_theme` reduces to: `SirioColors`, `Typography`, `Spacing`, `Radii`,
   `AgentBrandColor`, `graph_lane`, `BrowserChrome`, `WindowsCaption`, terminal
   families

**Gate:** `Scripts/ci.sh` prints `CI OK`, plus the headless smoke test.

### Why the phases do not merge

Phase 1 can only break *references*, and the compiler catches all of them.
Phase 2 can only break *values*, and it touches no call site where a mistake
could hide. A commit changing names and numbers together has neither property:
a wrong colour is indistinguishable from a wrong rename, and the only way to
find it is to look at screenshots.

## Testing

`sirio_theme` has 51 tests, in two families.

**Provenance-locked** — these encode *which values* Sirio has today, and B3
invalidates them by design. They are retargeted, not deleted:

`dark_palette_matches_recorded_provenance` ·
`light_palette_matches_recorded_provenance` ·
`intellij_shell_palette_matches_the_approved_reference` ·
`the_state_hues_are_the_ones_the_swift_app_shipped` ·
`spacing_and_typography_match_waku` · `radii_match_waku` ·
`macos_keeps_the_apple_faces` ·
`spacing_carries_menu_width_and_hairline_thickness` · the three `*_cosmic_*`
tests

**Invariants** — these encode design rules, cite no hex, and must stay green
throughout:

`shell_body_text_meets_wcag_aa_on_its_panel` · `the_veil_ladder_is_geometric` ·
`the_depth_ladder_reads_as_depth` ·
`accent_clears_contrast_on_its_own_surface` · `accent_is_not_any_agent_brand` ·
`every_adaptive_token_differs_between_light_and_dark` ·
`selection_stays_under_its_text` ·
`graph_lane_colours_cycle_and_stay_distinct` ·
`with_translucency_fades_structural_surfaces_and_is_reversible`

The invariants are a value-independent oracle: if
`shell_body_text_meets_wcag_aa_on_its_panel` is still green after the swap, the
bezel colours are legible on the bezel surfaces without anyone having to look at
them.

Note on the two `accent_*` invariants: they follow the `accent` → `brand_coral`
rename in Phase 1 and continue to test Sirio's coral, not bezel's neutral
`accent`. They are the reason the coral is kept as a token at all — inlining it
as a literal in the agent-colour picker would leave both invariants with nothing
to hold.

### Oracles per phase

| Phase | Automatic | Human |
|---|---|---|
| 1 | compiler + 51 tests green with **unchanged** assertions + "no numeric literal in diff" script | reread the 65-row table against the doc-comments |
| 2 | invariants green + "no file outside `sirio_theme` in diff" script | visual comparison with `cargo run -p gallery` |
| 3 | `Scripts/ci.sh` → `CI OK` + headless smoke test | titlebar on Linux |

Two new scripts, one per gate. Nothing else to write: the invariants that matter
already exist.

### What runs where

`sirio_ui` does not depend on `sirio_terminal` (verified: no reference in its
`Cargo.toml`). Phases 1 and 2 live almost entirely in `sirio_theme` + `sirio_ui`
and are verifiable with `cargo test -p sirio_theme -p sirio_ui`, with no Zig
required.

## Risks

**R1 — The `=0.1.3` pin changes meaning.** Today `bezel` is a component library
and the exact pin protects *API* compatibility. After B3 it is the design system
and the same pin protects *appearance*. A patch bump can recolour the app. Keep
the pin; treat any bump as a visual change to review, not as maintenance.

**R2 — The appearance mirror.** bezel's free functions (`ink`, `wash`,
`hairline`) take no `cx` and read a process-wide mirror that **defaults to
Dark**. Today only `loading.rs` syncs it, for the loaders. After B3 everything
paints through it: a stale mirror means the light theme draws dark hairlines. A
single sync point tied to theme install is required, not scattered calls.

**R3 — Binding choices have no automatic oracle.** Table 2 (fields to bindings)
is read from the code and needs no judgement. Table 1 (bindings to bezel) is the
part no script covers — 28 rows, of which one is marked **review**
(`frame_surface` to `band`).

This risk shrank once the mapping was derived from `for_appearance` rather than
from doc-comments: `primary_action_bg`, previously this risk's confirmed suspect,
turned out to be a plain alias of `selected_fill`. The lesson holds for the rest
— read the constructor, not the comment.

**R4 — Provenance has already been lost once.** `docs/linux-rewrite/` does not
exist: both `THEME-PROVENANCE.md` (cited by `ThemeColors`) and `COSMIC-DESIGN.md`
(cited by `cosmic/mod.rs`) are dead references. Phase 2 must *write* the new
provenance pointing at bezel, not merely retarget the tests — otherwise it is
lost a second time and the values become unexplainable again.

**R5 — Zig 0.15.2 is not installed.** `zig version` reports 0.16.0; only
`0.16.0_1` exists in `/opt/homebrew/Cellar/zig/`, and `Scripts/ci.sh` pins
`ZIG_REQUIRED="0.15.2"` and fails fast. This blocks the Phase 3 gate, not
Phase 1. Separately, per `CLAUDE.md` the macOS gate has never been green
(Phase 0 still open) — independent of this work, but it bounds what "done" can
mean.

## Open questions

None blocking. Table 1's `frame_surface` row is settled during Phase 1: bezel's
`band` is a recessed palette/picker header or footer strip, not Sirio's
translucent window-frame material.
