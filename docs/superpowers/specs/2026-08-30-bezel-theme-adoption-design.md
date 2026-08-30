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

`ThemeColors` has 65 fields. This table is the reviewable artifact: every row is
grounded in the field's own doc-comment, and rows marked **review** are ones
where the doc-comment and the bezel name do not obviously agree.

### Group A — self-declared aliases, collapse (9)

Each of these documents itself as an alias of another field.

| Sirio | Alias of |
|---|---|
| `background` | `panel_surface` |
| `canvas` | `frame_fallback` |
| `chat_surface` | `panel_surface` |
| `chrome_tint` | `panel_surface` |
| `tab_chip_underline` | `panel_border` |
| `title_selected` | `title` |
| `selection_fill` | `selected_fill` |
| `sidebar` | `panel_surface` |
| `sidebar_border` | `panel_border` |

### Group B — direct bezel correspondence (48)

| Sirio | bezel | Note |
|---|---|---|
| `frame_surface` | `band` | **review** — translucent frame material vs bezel's band |
| `frame_fallback` | `bg` | |
| `panel_surface` | `surface` | |
| `panel_border` | `border` | |
| `panel_focus_ring` | `ring` | |
| `hairline` | `border` | both are a neutral at 7–8% |
| `border_strong` | `border_strong` | |
| `title` | `text` | 160 sites |
| `tab_focus_accent` | `text` | doc: "the full text neutral" |
| `primary_text_color` | `text` | |
| `subtitle` | `text_muted` | 77 sites |
| `meta` | `text_faint` | 117 sites |
| `text_ghost` | `text_dim` | |
| `gauge` | `accent` | bezel's `accent` is `neutral(0.673)` — "indigo-400's lightness, no chroma" |
| `file_link` | `accent` | same neutral; clickability loses its colour signal (see "The blue") |
| `git_untracked` | `text_faint` | quieter than staged/modified/conflict, which stay chromatic |
| `raised` | `surface_raised` | "a step above the surface" |
| `card_fill` | `surface_card` | |
| `composer` | `input_bg` | |
| `filter_field_bg` | `input_bg` | |
| `row_hover` | `element_hover` | 73 sites |
| `overlay` | `element_hover` | doc: "generic hover wash — 5% neutral" |
| `selected_fill` | `element_active` | |
| `overlay_strong` | `element_active` | doc: "pressed wash — 9%" |
| `selection_ring` | `ring` | |
| `selection` | `selection` | text-selection wash, under glyphs |
| `caret` | `caret` | |
| `code_text` | `code_text` | |
| `code_wash` | `code_wash` | |
| `code_inset_fill` | `code_wash` | **review** — recessed fill vs inline wash |
| `inverse` | `solid` | |
| `on_inverse` | `on_solid` | |
| `primary_pill_bg` | `solid` | **review** — collides with `inverse` |
| `tab_done` | `success` | |
| `git_staged` | `success` | |
| `diff_addition` | `diff_add` | |
| `tab_needs_input` | `warning` | |
| `git_modified` | `warning` | |
| `favorite` | `warning` | doc: "the warning hue at full chroma" |
| `rail_question` | `warning` | doc: "the one card kind waiting on the reader" |
| `tab_error` | `danger` | |
| `git_conflict` | `danger` | |
| `diff_deletion` | `diff_del` | |
| `danger_soft` | `danger_muted` | |
| `diff_hunk_background` | `diff_hunk_bg` | |
| `rail_task` | `text_faint` | **review** — doc says neutral, exact step unresolved |
| `rail_edit` | `text_faint` | same neutral as `rail_task` |
| `rail_tool` | `text_faint` | same neutral as `rail_task` |

#### Two collisions to resolve during Phase 1

- `row_hover` and `overlay` both land on `element_hover`
- `selected_fill` and `overlay_strong` both land on `element_active`

Either the pairs are genuinely the same value — in which case they collapse into
Group A — or bezel lacks a distinction Sirio relies on, in which case one of each
pair moves to Group C. Resolved by comparing the current values, not by
argument.

### Group C — no bezel counterpart (8)

| Sirio | Resolution |
|---|---|
| `terminal_surface` | retain in `SirioColors` — bezel has no terminal-surface concept |
| `inset` | derive — "a step below the surface"; bezel has no recessed role |
| `chat_row_hover` | derive — `element_hover` at lower alpha (doc: 5% vs `row_hover`'s 6%) |
| `tree_guide` | derive from `border` |
| `diff_addition_background` | derive — `diff_add` at low alpha, as bezel does for `diff_hunk_bg` |
| `diff_deletion_background` | derive — `diff_del` at low alpha |
| `primary_action_bg` | **review** — doc says "must stay distinguishable from `raised`", so explicitly *not* `surface_raised` |
| `accent` → `brand_coral` | retain, **renamed**. Doc: "no role paints it any more"; only the Coral entry of the agent-colour picker reads it. Carries two invariants (clears AA on its own surface; is not any agent's brand). Renamed because `theme.accent` now resolves through `Deref` to bezel's `accent`, which is a different thing. |

#### The blue

bezel's palette has no blue role. Its `accent` is `neutral(0.673)` — documented
as "indigo-400's lightness, no chroma", i.e. the lightness of indigo without the
indigo. Its chromatic hues are red, amber, emerald and pink (`busy`).

Sirio uses blue for three documented meanings: quantity (`gauge`), clickability
(`file_link`), and untracked git status. This is not a naming gap but an opposite
design choice — bezel reserves colour for data and attention and uses contrast
for identity.

**Decision (C1):** drop the blue. All three tokens take a neutral from the bezel
palette:

- `gauge` → `accent`. bezel's `accent` is the neutral that sits where an accent
  colour would, which is exactly what a progress bar needs.
- `file_link` → `accent`, the same emphasis neutral.
- `git_untracked` → `text_faint`, quieter than the chromatic staged / modified /
  conflict states.

Rejected: keeping a declared Sirio blue (adds a permanent deviation from the
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

Consequence: Sirio's `accent` (brand coral) is renamed `brand_coral`. With
`Deref` pointing at bezel, `theme.accent` resolves to bezel's neutral accent, so
the two cannot share a name. The rename also removes a standing ambiguity — the
field has not been an accent in the design sense since it stopped being painted.

If the coral is ever wanted back as the app accent, the supported route is
`set_brand` with `Brand { accent: Tint::new(<coral hue>, <chroma>), .. }`, which
rotates the whole palette coherently, rather than a bespoke token.

## Phases

### Phase 1 — Rename. Values frozen.

1. Author and review the 65-row mapping table above; resolve the two Group B
   collisions and the four **review** rows against current values
2. Collapse the 9 Group A aliases
3. Rename `ThemeColors` fields to bezel names
4. Follow through ~704 call sites; the 21 `.colors` sites lose the prefix
5. Tests change *names*, never numbers

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

**R3 — Group B has no automatic oracle.** The only part of the work no script
covers. Mitigated by concentration (five names are 69% of sites), but
`primary_action_bg` is already a confirmed suspect: its doc says "must stay
distinguishable from `raised`", so it is explicitly not `surface_raised`.

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

None blocking. The four **review** rows and the two Group B collisions are
resolved during Phase 1 against current values, which is where the table stops
being a proposal and becomes a fact.
