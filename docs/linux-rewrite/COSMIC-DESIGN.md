# COSMIC-DESIGN — the token-mapping contract

Written for COSMIC-01 ("the token foundation"). This is the map other builders read before
restyling their own surface, so ten parallel conversions produce one design language instead of
"three greys." It documents what exists, where it lives, the one judgment call the brief asked
for, and what is deliberately not done yet.

Code: `crates/tiller_theme/src/cosmic/`. Reference surface: `crates/tiller_ui/src/titlebar.rs`.

---

## 1. depend vs transcribe — the call, and why

**Decision: transcribe, not depend.** `cosmic-theme` is not a `Cargo.toml` dependency of this
workspace. Its values are hand-transcribed into plain Rust structs, verified against the real
crate's own defaults (see §5).

Why, with evidence, not a guess:

- `cosmic-theme`'s own `Cargo.toml` (checked out locally at
  `~/.cargo/git/checkouts/libcosmic-*/*/cosmic-theme/Cargo.toml`, rev `1f8d8786`) depends on
  `cosmic-config` with its `subscription` feature **turned on unconditionally** — that is
  `cosmic-theme`'s choice, not a downstream default a consumer can opt out of. Cargo feature
  unification means *any* crate anywhere in the dependency graph enabling that feature turns it
  on for everyone; a consumer crate has no lever to strip it back off.
- `subscription` pulls in `iced_futures`, which is Wayland/iced-only. This was not read off a
  changelog — it was built. A scratch probe crate (`Cargo.toml` depending on nothing but
  `cosmic-theme` from git) was compiled end to end and `cargo tree` was run against it; the iced
  chain showed up exactly as expected. Same result for the narrower "depend on `cosmic-config`
  alone, skip `cosmic-theme`" path: it still needs two unpublished git dependencies pulled in
  transitively.
- GPUI is a from-scratch renderer with no iced/Wayland-toolkit surface anywhere else in this
  workspace. Taking that dependency for a data-only need — a handful of colors and numbers —
  would be a large, permanent build-graph and licensing-surface cost for a small, static payload.
- The brief's own framing agrees: `cosmic-theme` is "genuinely portable" as a *data source*, not
  as a crate that is safe to `cargo add`.

**The "prize" was not given up.** The brief calls out that depending is the only way to get real
accent/light-dark pickup from the user's actual COSMIC config — that native code (not
`cosmic-config`) is `crates/tiller_theme/src/cosmic/live.rs`: a dependency-free reader of
COSMIC's on-disk RON config under `$XDG_CONFIG_HOME/cosmic/<config-id>/v<version>/<field>`
(XDG fallback `~/.config`). It reads the same three config IDs `cosmic-config` would
(`com.system76.CosmicTheme.{Dark,Light}` for the resolved theme, `…Mode` for the dark/light
flag), with a narrow hand-rolled RON-subset parser for exactly the shapes COSMIC writes — not a
general RON library. `CosmicTheme::resolve` tries this live reader first and only falls back to
the transcribed stock palette when no live config is present (`cosmic::theme::CosmicTheme::resolve`,
`crates/tiller_theme/src/cosmic/theme.rs:122`).

This was verified executed, not read: `cosmic::live::tests::detects_the_live_cosmic_theme_when_present_or_falls_back_cleanly`
runs against this machine's real COSMIC install and prints
`live COSMIC theme detected: is_dark=true accent.base=rgba(0x63d0dfff)` — the genuine current
system accent, not a stub. **No COSMIC config present → still correct**: when `live::detect()`
finds no config directory (or a malformed one), `resolve()` silently falls through to the
transcribed `CosmicContainers`/`CosmicSemanticColors` `dark()`/`light()` defaults; nothing panics
or renders a default-initialized/zeroed struct. `cosmic::theme::tests::system_mode_never_panics_and_always_resolves`
covers the no-portal-answer case; the fallback path itself is exercised implicitly by every other
`theme` test run in the sandboxed test harness, where no COSMIC config directory exists.

---

## 2. Where the code lives

```
crates/tiller_theme/src/
  lib.rs              # ONE line added: `pub mod cosmic;` — nothing else in this 55KB file changed.
  cosmic/
    mod.rs             # module + re-exports
    hex.rs             # "#RRGGBB"/"#RRGGBBAA" → gpui::Rgba
    spacing.rs          # CosmicSpacing — the 10-step scale
    radii.rs             # CosmicRadii — the 6-step corner-radius scale
    component.rs        # CosmicComponent — one semantic role's interaction states
    container.rs         # CosmicContainer / CosmicContainers — the 3-layer hierarchy
    palette.rs            # CosmicContainers::dark() / ::light() — transcribed stock hierarchy
    semantic.rs            # CosmicSemanticColors::dark() / ::light() — transcribed stock roles
    live.rs                 # dependency-free reader of the user's real COSMIC config
    theme.rs                 # CosmicTheme — the composed Global, install()/get()/set_mode()/...
```

`Theme` (the existing type in `lib.rs`) is untouched: same fields, same 13+ call sites, same
behavior. `CosmicTheme` is a **second, independent `gpui::Global`** — a surface opts in by reading
`CosmicTheme::get(cx)` instead of `Theme::get(cx)`. Both can be installed in the same app at once
(the reference surface does exactly this: `Theme::init` still runs at the app-shell level for
every other surface; `Titlebar` additionally reads `CosmicTheme`). Nothing forces a
whole-app cutover in one step — that is the point of the seam.

`cosmic::theme::CosmicTheme::resolve` calls the *existing* private `ThemeMode::resolve`/
`resolve_system` methods on `lib.rs`'s `ThemeMode` rather than re-implementing "what does System
mean" a second time — Rust's privacy rule that a private item is visible to descendant modules
(not just its own module) makes this possible without changing either method's visibility.

---

## 3. The token catalogue

### 3.1 Container hierarchy

Three layers, each nested in the one before it: `background` → `primary` → `secondary`. Map
Tiller's own nesting onto it directly — **do not invent a fourth grey**:

| COSMIC layer | Tiller equivalent |
|---|---|
| `background` | the window canvas |
| `primary` | chrome that sits on the canvas: titlebar, sidebar, terminal host |
| `secondary` | things that float on `primary`: cards, popovers, menus |

Each `CosmicContainer` carries:

- `base: Rgba` — the layer's own fill
- `on: Rgba` — text/icon color drawn directly on `base`
- `divider: Rgba` — this layer's own rule/hairline color
- `component: CosmicComponent` — colors for widgets that live in this layer and don't have a
  more specific semantic role (see 3.3)

### 3.2 Spacing scale (`CosmicSpacing`, all `u16` logical px)

| step | value |
|---|---|
| `none` | 0 |
| `xxxs` | 4 |
| `xxs` | 8 |
| `xs` | 12 |
| `s` | 16 |
| `m` | 24 |
| `l` | 32 |
| `xl` | 48 |
| `xxl` | 64 |
| `xxxl` | 128 |

Does not vary between light and dark (verified against both `Theme::light_default()` and
`Theme::dark_default()` — identical scale).

### 3.3 Corner radii (`CosmicRadii`, each `[f32; 4]`, uniform per side)

| step | value |
|---|---|
| `radius_0` | 0 |
| `radius_xs` | 4 |
| `radius_s` | 8 |
| `radius_m` | 16 |
| `radius_l` | 32 |
| `radius_xl` | 160 |

GPUI's `div().rounded()` only takes one uniform value, so call sites read `radii.radius_xs[0]`
(etc.) — the `[f32; 4]` shape is kept because upstream's is, so a future per-corner call site
doesn't need a type change.

### 3.4 Semantic component colors (`CosmicSemanticColors`)

Fourteen named roles, each a `CosmicComponent { base, hover, pressed, on, divider, border }`,
plus one standalone `shade: Rgba` (dialog/modal backdrop):

`button`, `accent`, `accent_button`, `success`, `success_button`, `destructive`,
`destructive_button`, `warning`, `warning_button`, `icon_button`, `link_button`, `list_button`,
`text_button`.

Pick the role that matches the control's **function**, not its container:

- Chromeless icon-only toggle (sidebar/panel visibility, window controls) → `icon_button`
- Primary call-to-action → `accent_button`
- Destructive action (delete, discard) → `destructive_button`
- Warning-toned action → `warning_button`
- A plain text link → `link_button`
- A selectable list row → `list_button`
- A text-styled button that isn't a link → `text_button`

`CosmicComponent` upstream carries twelve fields; this transcription keeps the six the brief
calls out (base/hover/pressed/on/divider/border) because nothing here reads the other six
(`selected`, `selected_text`, `focus`, `disabled`, `on_disabled`, `disabled_border`) yet. Adding
them later is additive — it does not change the meaning of the six kept.

One verified quirk worth knowing before you assume all "ghost" roles behave alike:
**`link_button` stays fully transparent (`#00000000`) through base, hover *and* pressed, in both
appearances** — its interaction affordance lives entirely in the `on` (text) color, not a fill
change. `icon_button`, `list_button` and `text_button` do darken on press. This was learned by a
failing test (see `cosmic::semantic::tests::ghost_buttons_still_show_a_pressed_state`'s doc
comment) — do not assume the six-field shape means uniform behavior across roles.

### 3.5 Deliberately not delivered here

The brief flags `accent_text`, `control_tint`, `text_tint`, `window_hint`, `active_hint`, window
gaps and the 14-level frosted-blur alpha map as things to "fetch crate source for detail not
covered here." Status:

- `accent_text`, `control_tint`, `text_tint`, `window_hint`: **omitted, correctly** — verified
  against `Theme::light_default()`/`Theme::dark_default()`'s actual output, all four are `None`
  (unset) in both stock palettes; COSMIC derives a readable-on-accent text color and a
  control/text tint automatically when they're absent. Transcribing a color that does not exist
  by default would have been a fabrication, not a token.
- `active_hint` (default 3), window gaps (default `(0, 8)`), and the frosted-blur alpha map
  (~0.6–0.90 across 14 levels): **not transcribed.** These govern window-manager-level chrome
  (focus ring width, tiling gaps, blur-behind) that nothing in this codebase draws yet — there is
  no consumer for them today, and transcribing untested numbers nobody reads would just be
  unverified surface area. Flagging this explicitly rather than silently dropping it: the next
  builder who needs window-manager-level chrome should treat these as **NOT EXERCISED**, source
  them from `cosmic-theme`'s `CornerRadii`/`Theme` structs the same way §5 describes, and add
  their own transcription + tests rather than assuming this crate already covers it.

---

## 4. How to consume this from a new surface

```rust
use tiller_theme::cosmic::CosmicTheme;

// In your entity's constructor — idempotent, safe to call from every surface that
// might be the first one built:
if !cx.has_global::<CosmicTheme>() {
    CosmicTheme::init(cx); // installs ThemeMode::System, follows the XDG portal on Linux
}

// In render():
let cosmic = *CosmicTheme::get(cx);
let bar = cosmic.containers.primary;          // pick the layer matching your nesting depth
let action = cosmic.semantic.accent_button;    // pick the role matching the control's function
div()
    .bg(bar.base)
    .border_color(bar.divider)
    .rounded(px(cosmic.radii.radius_s[0]))
    .child(/* ... */
        div().bg(action.base).hover(|s| s.bg(action.hover)).text_color(action.on)
    )
```

Context-free variants (`CosmicTheme::light()`, `CosmicTheme::dark()`, `CosmicTheme::for_mode(mode,
appearance)`) exist for tests and don't require a live `App`.

Rules of the road:

1. **Don't hand-pick a new grey.** If a color you need isn't in `containers` or `semantic`,
   that's a sign you're missing a role, not a license to write a hex literal.
2. **Container layer is about visual nesting, not code module.** A dialog rendered from deep
   inside a "primary"-layer file is still visually a `secondary` surface — read its container
   from where it *draws*, not where its code lives.
3. **`Theme` keeps working.** Migrating a surface to `CosmicTheme` is opt-in and file-scoped;
   nothing requires touching every call site at once, and nothing breaks for the surfaces that
   haven't migrated yet.

---

## 5. Where the transcribed numbers came from

Every stock hex/scale value in `palette.rs`/`semantic.rs`/`spacing.rs`/`radii.rs` was sourced by
executing `cosmic_theme::Theme::dark_default()` / `::light_default()` against the real crate in an
isolated scratch binary and reading the values back off the wire — not copied from documentation
or guessed from screenshots. Values were cross-checked against this machine's own **unmodified**
live dark COSMIC config (its Light config on this machine turned out to be user-customized, so it
was excluded from the "what is stock" comparison and the isolated-binary dump was treated as the
source of truth for defaults).

---

## 6. The reference surface: `titlebar.rs`

`crates/tiller_ui/src/titlebar.rs` is the one surface restyled end to end for this task (chosen
because it is self-contained and not owned by any other builder currently active in this
worktree). What changed and what didn't:

- **Colors, hierarchy, radius, one spacing step: now COSMIC.** The bar's fill is
  `cosmic.containers.primary.base`; a 1px `border_b` in `cosmic.containers.primary.divider` marks
  the primary/background seam (COSMIC headerbars carry exactly this hairline); the two icon
  toggles use `cosmic.semantic.icon_button` (`on` for the resting glyph, `hover` for the hover
  fill); corner rounding is `cosmic.radii.radius_xs[0]`; the trailing inset is
  `cosmic.spacing.xs` (12px, replacing a bespoke 14px literal that had no test pinning it).
- **Geometry stayed waku's, on purpose.** `HEIGHT` (48px), `CONTROL_SIZE` (26px), `CONTROL_GAP`
  (6px) and `TRAFFIC_LIGHT_INSET` (14px) are asserted byte-for-byte by
  `crates/tiller_ui/src/conformance.rs`'s `bars_and_rows_use_the_measured_density` test, which is
  a *different*, independently-owned contract (waku's measured physical scale) than the one this
  task is about. Changing them was out of scope and would have broken a passing test that isn't
  mine to rewrite. This is a deliberate scope line, not an oversight: **this surface's geometry
  is waku's; its color/hierarchy/radius language is COSMIC's.** A future task that wants the
  titlebar's own pixel geometry to come from the COSMIC spacing scale too should update
  `conformance.rs`'s pinned values in the same change, not silently drift from them.
- **Both appearances, not just dark.** `CosmicTheme::resolve` is appearance-aware the same way
  `Theme` is; the reference surface reads whichever `CosmicTheme` is installed, so it renders
  correctly under both without a per-surface light/dark branch.

---

## 7. Verification

`cargo test -p tiller_theme --lib` — 33 tests, all passing, including the 21 new ones under
`cosmic::*`: `cosmic::hex::tests::{parses_eight_digit_hex_with_alpha, parses_partial_alpha,
parses_six_digit_hex_as_opaque, rejects_malformed_input}`; `cosmic::spacing::tests::matches_cosmic_themes_verified_defaults`;
`cosmic::radii::tests::matches_cosmic_themes_verified_defaults`; `cosmic::palette::tests::{dark_and_light_are_distinct,
dark_background_is_darker_than_dark_secondary, light_background_is_darker_than_light_secondary}`;
`cosmic::semantic::tests::{dark_and_light_accents_are_distinct, ghost_buttons_still_show_a_pressed_state}`;
`cosmic::live::tests::{detects_the_live_cosmic_theme_when_present_or_falls_back_cleanly,
field_matching_does_not_confuse_border_with_disabled_border, parses_a_real_captured_component_block,
parses_a_real_captured_container_block, read_hex_scalar_strips_the_ron_string_quotes,
splits_container_and_nested_component_text}`; `cosmic::theme::tests::{dark_mode_resolves_to_the_dark_container_hierarchy,
light_mode_resolves_to_the_light_container_hierarchy, system_mode_never_panics_and_always_resolves,
light_and_dark_helpers_do_not_require_an_app_context}`.

`cargo test -p tiller_ui titlebar` — 3 tests, all passing (drawn, UI-tier):
`titlebar::tests::titlebar_controls_emit_shell_visibility_events` (pre-existing, unchanged
behavior verified against the new render path), `titlebar::tests::titlebar_draws_the_cosmic_primary_container_in_dark_mode`,
`titlebar::tests::titlebar_draws_the_cosmic_primary_container_in_light_mode` (both new — installed
`CosmicTheme` explicitly *before* `Titlebar::new` runs, so the assertions only pass if `render`
truly reads the pre-installed global back rather than a hardcoded value; both use
`.debug_bounds(...)` on real drawn elements per the evidence standard's UI tier).

Screenshot evidence (light and dark, both appearances of the reference surface) is tracked
separately in the task's final report, per `EVIDENCE-STANDARD.md`'s appearance tier.
