# Sirio's theme on Ely's palette — design

**Date:** 2026-10-01

**Status:** design approved in conversation on 2026-10-01; written spec awaiting review

**Scope:** sub-project 1 of 7 in migrating Sirio's UI components from bezel
to Ely GPUI Components. This document specifies sub-project 1 only.

## §0 The migration and this sub-project

The user wants a single component library across the app. PR #598 drew the
agent chat with Ely, vendored at `rust/vendor/ely-gpui-component`; everything
else is still drawn with bezel. The migration moves the rest of the app's
widgets to Ely while keeping Sirio's visual identity.

Decisions confirmed in the conversation:

| Item | Decision |
|---|---|
| Perimeter | Migrate `bezel::ui`, `bezel::motion` and theme ownership. `bezel-syntax`/`HighlightKind`, `bezel-markdown` (selection) and the `bezel-gpui` fork stay. `bezel-editor` is removed: only its `init` is called, for key contexts no element uses. |
| Appearance | Sirio's identity on Ely: Sirio's palettes, Geist, Sirio's UI scale and radii. Controls change shape and motion, not colour or type. |
| Delivery | Seven sub-projects, one spec, plan and PR each. `main` stays releasable; bezel and Ely coexist on different surfaces for a few cycles. |
| Vendoring | Extend the vendored closure per sub-project, copying only the needed upstream modules from `e17e31a` (whose `src/` is byte-identical to upstream HEAD `567ab6f` as of 2026-10-01), and record every addition and adaptation in `LOCAL-CHANGES.md`. |
| Theme source (this sub-project) | Ely's `Palette` vocabulary becomes the source of Sirio's colours. The values are Sirio's, frozen from today's tree. |
| Visual change (this sub-project) | None. Where the chat's Ely adapter and a Sirio token of the same meaning disagree, the Ely field keeps today's value and the Sirio token stays in a Sirio extension. |

### Roadmap

| # | Sub-project | Surfaces |
|---|---|---|
| **1** | **Sirio's theme on Ely's palette** (this document) | `sirio_theme`, the Ely adapter, fonts, assets, bootstrap |
| 2 | Overlay primitives | Tooltips, popovers and context menus (sidebar, tab bar, right panel, composer menus), icons, loaders and progress, surfaces |
| 3 | Scroll and motion | Scrollbars (terminal, file view, transcript, thought, horizontal), follow-tail, a Sirio helper replacing `Painter`/`hover_t` |
| 4 | Text input | `TextField` → `forms::TextInput` everywhere: IME, clipboard, popup anchoring |
| 5 | Settings pages | bezel card, button and badge recipes → `settings::*`/`buttons::*`; the effort slider |
| 6 | Navigation | Sidebar `ui::tree` → `lists::Tree`; worktree picker → `forms::Combobox` |
| 7 | Cleanup | Remove `bezel`, `bezel-editor` and the bezel fonts from the manifests; update `CLAUDE.md` |

After sub-project 7, `bezel` (the widget crate) is gone. `bezel-theme`,
`bezel-markdown`, `bezel-syntax` and `bezel-gpui` remain:
`bezel-markdown` and `bezel-syntax` depend on `bezel-theme` and read
`bezel_theme::Theme::of(cx)` when they paint.

## §1 What exists today

`bezel::theme` is the `bezel-theme` crate, re-exported. `sirio_theme`
builds every colour from `bezel_theme::Theme::branded(Brand { tint })`.
For Neutral and Onice in both appearances, and Notte in dark, it then
overwrites the surfaces with hand-written grey ladders (`base_color.rs`).
It keeps 38 colour tokens in `ThemeColors` and installs the same branded
72-field theme into bezel with `install_into_bezel` and `sync_appearance`.
Those two calls must be repeated from every path that swaps the global,
including `follow_portal`.

The chat's adapter (`sirio_ui/src/chat/ely.rs`) observes Sirio's `Theme`
global. On every change it sets Ely's mode and assigns 27 of `Palette`'s
fields from Sirio tokens. The other fields keep Ely's defaults for the mode,
and the vendored chat components read four of them: `shimmer`, `info`,
`chart` and `syntax`. Spacing and radii derive from `BezelTheme::SPACE_*`
and `BASE_RADIUS`. Geist is registered by `bezel::ui::register_fonts`,
whose five TTFs live in `bezel-ui`. `docs/THEME-PROVENANCE.md` is cited by
`sirio_theme` but does not exist.

## §2 Architecture

### `ely-palette`, a vendored leaf crate

Ely's `src/theme/palette.rs` and `src/theme/syntax.rs` depend only on `gpui`
and `Mode`. Extract them, with `Mode` and the `Mix` trait, into
`rust/vendor/ely-palette`. `ely-gpui-component` depends on it and re-exports
it at the original paths, so `ely_gpui_component::theme::Palette` is the same
type. Record the split in `LOCAL-CHANGES.md`. Upstream's inline tests for
those files move with them.

### `sirio_theme`'s colour model

`sirio_theme` depends on `gpui`, `ely-palette` and `bezel-theme`.
`bezel-theme` is used only for `HighlightKind`, `SyntaxPalette` and
`AppearanceMode`. The crate no longer depends on `bezel`, and it does not
depend on `ely-gpui-component`, so `sirio_terminal` and the other
dependants do not compile Ely's components.

```rust
pub struct ThemeColors {
    /// Ely's vocabulary: every field Ely components read, and what Sirio's
    /// own call sites read wherever the meaning and value coincide.
    pub ely: ely_palette::Palette,
    /// The tokens Ely has no field for, or whose value differs from the Ely
    /// field of the same meaning (§3).
    pub sirio: SirioColors,
}
```

`Theme` keeps its `Deref` to `ThemeColors`, so a call site reads
`theme.ely.fg` or `theme.sirio.terminal_surface`. The 38 old names are
removed.

### One source, two consumers

```
Sirio preset (BaseColor × Appearance) ─► Theme { colors: ThemeColors { ely, sirio }, … }   (gpui global)
                                                     │  observed by sirio_ui::ely
                         ┌───────────────────────────┴───────────────────────────┐
     Ely: set_mode_now + palette + metrics + fonts           bezel-theme: inverse adapter
     (every Ely component in the app)                        (bezel-markdown, and bezel widgets
                                                              until sub-project 7)
```

`sirio_ui::chat::ely` moves to `sirio_ui::ely` and is initialised by the host
for the whole app, right after `Theme::init` and before `chat::init`. It
observes the `Theme` global and, on each change:

1. Installs `colors.ely` into Ely with the instant path: no cross-fade,
   because Sirio switches themes instantly today. It also sets the mode,
   `ThemeMetrics` (text sizes and radii, as today), the font families and
   reduced motion.
2. Runs the inverse adapter. It builds `bezel_theme::Theme::branded(Brand { tint })`
   with the preset's tint, overwrites every field that has a Sirio
   equivalent (§3.3), then calls `install_custom` and `set_current_appearance`.

`sirio_theme` loses `to_bezel_theme`, `install_into_bezel` and
`sync_appearance`. Bezel synchronisation becomes a consequence of the
global changing, not a duty of every installer. The fallback install at
`sirio/src/main.rs:16842` goes away. `ChatAssets` becomes
`sirio_ui::ely::AppAssets` with the same namespaces (`ely/`, `sirio-chat/`,
falling back to bezel's icons until sub-project 2).

Effects from `observe_global` are delivered on flush. Validation (§6) reads
consumers after a flush, as a frame does.

## §3 Vocabulary and values

### 3.1 Renaming rule

A Sirio token becomes an Ely field only when both its meaning and its value
coincide with that field's current value (the chat adapter's assignment,
§1). An alias token, whose own documentation defines it as another token,
is replaced by the token it aliases. Every other token moves to
`SirioColors`. As a result, Ely components and Sirio call sites each see
today's values.

### 3.2 The 38 tokens

| Today | After | Reason |
|---|---|---|
| `surface` | `ely.bg` | adapter: `bg ← surface` |
| `surface_raised` | `ely.surface` | adapter: `surface ← surface_raised` |
| `input_bg` | `ely.sunken` | adapter |
| `element_hover` | `ely.hover` | adapter |
| `element_active` | `ely.active` | adapter |
| `border` | `ely.border` | adapter |
| `border_opaque` | `ely.border` | alias of `border` ("kept as a name until phase 3 removes it") |
| `text` | `ely.fg` | adapter |
| `text_muted` | `ely.fg_muted` | adapter |
| `text_faint` | `ely.fg_subtle` | adapter |
| `git_untracked` | `ely.fg_subtle` | alias: assigned `text_faint` |
| `file_link` | `ely.link` | adapter |
| `selection` | `ely.selection` | adapter |
| `success` / `warning` / `danger` | `ely.success` / `ely.warning` / `ely.danger` | adapter |
| `favorite` | `ely.warning` | alias ("a starred row is the warning hue, not a fifth colour") |
| `bg` | `sirio.canvas` | Ely `bg` means the panel surface; this is the opaque fallback behind the shell |
| `frame_surface` | `sirio.frame_surface` | no Ely equivalent |
| `terminal_surface` | `sirio.terminal_surface` | no Ely equivalent |
| `dialog_surface` / `floating_surface` | `sirio.dialog_surface` / `sirio.floating_surface` | opaque twins exempt from translucency |
| `overlay` / `overlay_strong` | `sirio.overlay` / `sirio.overlay_strong` | hover and press washes; Ely's `backdrop` is a scrim |
| `border_strong` | `sirio.border_strong` | value differs from `ely.border_strong` (= `border`) |
| `ring` | `sirio.ring` | value differs from `ely.focus` (= `text`) |
| `text_dim` | `sirio.text_dim` | value differs from `ely.fg_disabled` (= `text_faint`) |
| `danger_muted` | `sirio.danger_muted` | value differs from `ely.danger_subtle` (12% danger) |
| `accent` | `sirio.quantity` | the quantity blue; `ely.accent` is `text` |
| `solid` / `on_solid` | `sirio.solid` / `sirio.on_solid` | value differs from `ely.accent`/`on_accent` |
| `brand_coral` | `sirio.brand_coral` | no Ely equivalent |
| `code_wash` | `sirio.code_wash` | no Ely equivalent |
| `tree_guide` | `sirio.tree_guide` | no Ely equivalent |
| `diff_add`, `diff_add_bg`, `diff_del`, `diff_del_bg` | `sirio.diff_*` | no Ely equivalent |

`SirioColors` therefore holds 21 tokens. Sub-projects 2–6 decide surface by
surface whether a Sirio token converges onto an Ely field. Each convergence
is a visible change and is declared as one.

### 3.3 Ely's fields and where their values come from

Every field of `ely.*` holds a frozen value per preset, equal to what Ely
receives today:

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

**Amendment to the conversational design.** Section 2 proposed deriving
`syntax` from Sirio's source palette, `ansi` from the terminal and `chart`
from the graph lanes. The vendored chat already reads `shimmer`, `info`,
`chart` and `syntax`, so those rules would change the chat. In this
sub-project, every field the adapter leaves unset keeps Ely's default. The
derivation rules move to the first sub-project that migrates a surface
reading them.

Sirio's own source-code colours stay in the 24-kind
`bezel_theme::SyntaxPalette` returned by `Theme::syntax_palette()`. That is
the highlighting boundary chosen for the migration; Ely's `Syntax` has 13
fields and cannot hold them.

The inverse adapter writes the bezel fields with a Sirio equivalent: the
surfaces, text ladder, borders, hover and active, selection, `code_wash`,
`solid`/`on_solid`, the semantic hues, `danger_muted` and `ring`. Other
bezel fields (glass, frost, shadows) keep `branded(tint)`'s values, so the
bezel widgets still alive until sub-project 7 render as today. The tint
is kept in each preset for this purpose only.

### 3.4 Presets

The values are Sirio data, generated once from today's tree and checked
by §6.1. They split along the axis on which they actually vary:

| Block | Varies by | Holds |
|---|---|---|
| Semantic | appearance | success, warning, danger and their subtle forms, diff, coral, the retained Ely defaults |
| Neutral ladder | base colour × appearance (7 × 2) | surfaces, borders, hover and active, the text ladder, selection, sunken, washes |

Neutral, Notte and Onice are already hand-written ladders. Stone, Zinc, Gray
and Slate become ladders of the same form. A future base colour is a new
ladder. `BaseColor` keeps its public API and persisted keys.

### 3.5 Translucency

`with_translucency_at` fades the same surfaces it fades today, under their
new names: `ely.bg`, `ely.surface`, `ely.sunken` and `sirio.terminal_surface`.
Today Ely receives copies taken *after* fading, so the copies are faded
too: `ely.overlay`, `ely.tooltip_bg` and `ely.on_accent`. The opaque twins
(`sirio.dialog_surface`, `sirio.floating_surface`) and `sirio.frame_surface`
are untouched, as now.

## §4 Fonts, metrics, bootstrap

- The five Geist TTFs (Regular, Medium, SemiBold, Bold, Mono) move into
  `sirio_theme`'s assets with their OFL licence and provenance, beside the
  bundled terminal font. `sirio_theme::register_ui_fonts` replaces
  `bezel::ui::register_fonts` and still runs before `Theme::init`. The
  workspace turns off bezel's `geist-*` features so the binary does not
  carry two copies.
- `Spacing` and `Radii` defaults become Sirio numbers, the values the
  `BezelTheme::SPACE_*`/`BASE_RADIUS` expressions produce today.
- `ThemeMode` stays the `bezel-theme` `AppearanceMode` re-export; the
  conversion in `sirio`'s `main.rs` is unchanged.
- Bootstrap order: `register_fonts`, `Theme::init`, `sirio_ui::ely::init`,
  then the existing initialisers.

## §5 Call sites

About 1,900 references to colour tokens, in `sirio_ui`, `sirio` and
`sirio_terminal`, are renamed with the table in §3.2. Renaming is driven by
the compiler: remove the old fields, then translate each reported location
with a script. Do not use textual search and replace, because `.text` and
`.bg` are fields of unrelated types. Behaviour, layout and selectors do not
change.

## §6 Validation

### 6.1 Token identity artefact

The branch's first commit adds `sirio_theme/examples/dump_tokens.rs`. It
prints a TSV of every resolved token for the 14 base × appearance
combinations, with and without translucency, plus the 72 fields installed
into `bezel-theme` and the `Palette` installed into Ely. Each value is
read after a flush. Run on that commit, it records the baseline in today's
vocabulary. After the migration the same example prints the new
vocabulary. A script maps the names with §3.2/§3.3, and the diff must be
empty. The TSVs, the diff and the commands are committed under
`docs/testing/` so anyone can re-run them.

### 6.2 Visual artefact

`Scripts/visual-sweep.sh` captures an isolated real instance in the 14
themes before and after on X11. Native macOS and Windows rendering is not
claimed. Their compilation is covered by `macos-check.yml` and the pull
request's CI.

### 6.3 Tests

- `sirio_theme`'s semantic invariants stay, renamed: WCAG contrast, depth
  ladders, coral distinct from every agent brand, selection under its text,
  translucency.
- `dark_palette_comes_from_bezel` and `light_palette_comes_from_bezel` are
  removed. The behaviour they proved ceases to exist by design.
- `conformance.rs`'s bezel installation test is rewritten against the
  observer.
- `ely_theme_change_preserves_chat_state_and_bezel_palette` keeps its intent.
- A theme change must reach both consumers. Write the failure modes of the
  observer first:
  - a portal swap not reaching bezel;
  - a base change reaching Ely but not bezel;
  - translucency missing a copy.

  Only then write the observer.
- Run `cargo nextest` on `sirio_theme`, `ely-palette`, `sirio_ui` and
  `sirio`, plus `cargo build --workspace --all-targets`: renamed public
  fields break other crates' test targets.
- `Scripts/ci.sh` runs only on the user's request.

## §7 Out of scope

- Any widget change (sub-projects 2–6) and any visible change.
- New base colours, high-contrast modes and new settings. Ely's `high_contrast`,
  `color_blind_safe` and `density` stay at their neutral values.
- `bezel-markdown` and `bezel-syntax`.
- The existing gap where chat code fences colour 7 languages rather than 24
  (noted for a separate change).
- The release version. This sub-project is `refactor:`/`docs:` and does not
  move `[workspace.package] version`.

## §8 Documentation

- Write `docs/THEME-PROVENANCE.md`: the frozen origin (`bezel-theme` 0.1.4 at
  the baseline commit), the vocabulary, §3.2–§3.5 and the retained Ely defaults.
- Update `CLAUDE.md`'s theme paragraph, which says `sirio_theme` "holds no
  palette of its own", and the crate graph entry for `sirio_theme`.
- Add `ely-palette` to `LOCAL-CHANGES.md` and the vendor README.

## References

- [Ely agent chat design](2026-09-30-ely-agent-chat-design.md)
- [Vendored Ely local changes](../../../rust/vendor/ely-gpui-component/LOCAL-CHANGES.md)
- [Ely palette at the pinned revision](https://github.com/ZacharyZhang-NY/Ely-GPUI-Components/blob/e17e31a6890c09ebcfa8b61133d7bc7c625edf69/src/theme/palette.rs)
