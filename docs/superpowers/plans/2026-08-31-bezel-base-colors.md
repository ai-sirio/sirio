# Bezel base colours in Settings — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user pick one of bezel's five base colours in Settings → Appearance, and delete the file-icon and agent-colour sections from that page.

**Architecture:** A `BaseColor` enum in `sirio_theme` maps each of the five names to a `bezel::theme::Tint`. `ThemeColors::for_appearance` takes it as a parameter and builds its bezel palette with `bezel::theme::Theme::branded` instead of `::dark()`/`::light()`, so only the hue of bezel-derived tokens rotates. The choice is carried on the installed `Theme` (the way `mode` and `appearance` already are), persisted under a new key in the existing key-value `setting` table, and mirrored by a serde-only twin enum in `sirio_persistence`.

**Tech Stack:** Rust 2024, gpui, `bezel` pinned `=0.1.3`, rusqlite.

**Spec:** `docs/superpowers/specs/2026-08-31-bezel-base-colors-design.md`

## Global Constraints

- `bezel` stays pinned `=0.1.3` in `rust/Cargo.toml`. Do not bump it.
- **Zig exactly 0.15.2 must be on PATH** for anything that builds `sirio_terminal` — that means `-p sirio` and any workspace-wide command. On this machine: `export PATH="/opt/homebrew/opt/zig@0.15/bin:$PATH"`. `-p sirio_theme`, `-p sirio_persistence` and `-p sirio_ui` do not need it.
- `Scripts/ci.sh` must print `CI OK` before the work is considered done.
- Commit messages: Conventional Commits, lower-case imperative subject (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`).
- `sirio_persistence` must **not** gain a dependency on `bezel` or `sirio_theme`. It carries the serde contract only.
- Tests first. Every task writes the failing test before the implementation.
- All shell commands below run from the repo root unless they say `cd rust`.

---

### Task 1: `sirio_theme::BaseColor`

The enum that names the five and hands back bezel's tint. Nothing reads it yet.

**Files:**
- Create: `rust/crates/sirio_theme/src/base_color.rs`
- Modify: `rust/crates/sirio_theme/src/lib.rs` (add `mod base_color;` and the re-export beside the other `pub use` lines near line 47)
- Test: `rust/crates/sirio_theme/src/base_color.rs` (inline `#[cfg(test)] mod tests`, matching the crate's existing style)

**Interfaces:**
- Consumes: `bezel::theme::Tint` (`bezel-theme-0.1.3/src/brand.rs:19`), constructed with `Tint::new(hue, chroma)` and `Tint::NONE`.
- Produces: `sirio_theme::BaseColor` with variants `Neutral`, `Stone`, `Zinc`, `Gray`, `Slate`; `BaseColor::ALL: [BaseColor; 5]`; `fn tint(self) -> bezel::theme::Tint`; `fn title(self) -> &'static str`; `impl Default` returning `Neutral`. Tasks 2, 3, 5 and 6 rely on these exact names.

- [ ] **Step 1: Write the failing test**

Create `rust/crates/sirio_theme/src/base_color.rs` with the test module only, so the file fails to compile for the right reason:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_is_bezels_shipped_grey() {
        // `Tint::NONE` is what `Brand::default` carries, and bezel documents
        // that it reproduces the built-in palette exactly. Anything else here
        // would silently restyle every existing install on upgrade.
        assert_eq!(BaseColor::Neutral.tint(), bezel::theme::Tint::NONE);
        assert_eq!(BaseColor::default(), BaseColor::Neutral);
    }

    #[test]
    fn the_four_tinted_families_carry_bezels_own_numbers() {
        // Transcribed from `BASE_COLORS` in bezel's `brand.rs`. Written out
        // rather than read from that array so a bezel bump that moves a hue
        // fails here instead of restyling the app quietly — the same contract
        // `dark_palette_comes_from_bezel` holds for the palette itself.
        let expected = [
            (BaseColor::Stone, 58.071_f32, 0.013_f32),
            (BaseColor::Zinc, 285.938, 0.016),
            (BaseColor::Gray, 264.364, 0.027),
            (BaseColor::Slate, 257.417, 0.046),
        ];
        for (base, hue, chroma) in expected {
            let tint = base.tint();
            assert_eq!(tint.hue, hue, "{base:?} hue");
            assert_eq!(tint.chroma, chroma, "{base:?} chroma");
        }
    }

    #[test]
    fn all_lists_every_variant_in_display_order() {
        assert_eq!(
            BaseColor::ALL,
            [
                BaseColor::Neutral,
                BaseColor::Stone,
                BaseColor::Zinc,
                BaseColor::Gray,
                BaseColor::Slate,
            ]
        );
        assert_eq!(
            BaseColor::ALL.map(BaseColor::title),
            ["Neutral", "Stone", "Zinc", "Gray", "Slate"]
        );
    }
}
```

Add to `rust/crates/sirio_theme/src/lib.rs`, immediately above the existing `pub use bezel::theme::appearance::AppearanceMode as ThemeMode;` (line 47):

```rust
mod base_color;

pub use base_color::BaseColor;
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd rust && cargo test -p sirio_theme base_color
```

Expected: FAIL — `cannot find type BaseColor in this scope`.

- [ ] **Step 3: Write minimal implementation**

Put this at the top of `rust/crates/sirio_theme/src/base_color.rs`, above the test module:

```rust
//! The neutral family the palette's greys are tinted with.
//!
//! bezel ships five, quoted from Tailwind's neutral families at their 500
//! step (`BASE_COLORS` in bezel's `brand.rs`) — the same list shadcn offers
//! as its base colour. They are hues, not palettes: `Brand::apply` rotates
//! the hue of every grey and leaves lightness alone, because bezel's two
//! palettes were tuned against measured contrast ratios and a brand rotates
//! that work rather than replacing it.

use bezel::theme::Tint;

/// One of bezel's five base colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BaseColor {
    /// bezel's shipped grey, carrying no hue at all.
    #[default]
    Neutral,
    Stone,
    Zinc,
    Gray,
    Slate,
}

impl BaseColor {
    /// Every variant, in the order the picker offers them — bezel's own
    /// order in `BASE_COLORS`, neutral first.
    pub const ALL: [Self; 5] = [
        Self::Neutral,
        Self::Stone,
        Self::Zinc,
        Self::Gray,
        Self::Slate,
    ];

    /// The oklch hue and chroma this family tints the greys with.
    pub fn tint(self) -> Tint {
        match self {
            Self::Neutral => Tint::NONE,
            Self::Stone => Tint::new(58.071, 0.013),
            Self::Zinc => Tint::new(285.938, 0.016),
            Self::Gray => Tint::new(264.364, 0.027),
            Self::Slate => Tint::new(257.417, 0.046),
        }
    }

    /// The user-visible name. Tailwind's, because a user who has met these
    /// names anywhere else has met exactly these colours.
    pub fn title(self) -> &'static str {
        match self {
            Self::Neutral => "Neutral",
            Self::Stone => "Stone",
            Self::Zinc => "Zinc",
            Self::Gray => "Gray",
            Self::Slate => "Slate",
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd rust && cargo test -p sirio_theme base_color
```

Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_theme/src/base_color.rs rust/crates/sirio_theme/src/lib.rs
git commit -m "feat: add bezel's five base colours as a theme token"
```

---

### Task 2: Thread the base colour through the palette

`ThemeColors::for_appearance` gains the parameter and builds from `Theme::branded`. `Theme` gains a `base_color` field so a reinstall preserves it.

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs:244` (`ThemeColors::for_appearance`), `:1102-1130` (the `Theme` struct), `:1274` (portal follower), `:1287` (`for_mode`), `:1294` (`for_mode_linux`), `:1299` (`light`), `:1304` (`dark`), `:1342` (`with_translucency`), `:1362` (`Theme::for_appearance`), and the four test call sites at `:1714`, `:1765`, `:1844`, `:1857`
- Test: `rust/crates/sirio_theme/src/lib.rs` (the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `BaseColor::tint` from Task 1.
- Produces:
  - `ThemeColors::for_appearance(appearance: Appearance, base: BaseColor) -> Self`
  - `Theme::for_appearance(mode: ThemeMode, appearance: Appearance, base: BaseColor) -> Self`
  - `Theme::for_mode(mode: ThemeMode, system_appearance: WindowAppearance, base: BaseColor) -> Self`
  - `Theme.base_color: BaseColor` — a public field, read by Task 3
  - `Theme::light()` / `Theme::dark()` keep their zero-argument signatures and mean `BaseColor::Neutral`

Note on why the field is required rather than merely convenient: `with_translucency` (`:1342`) rebuilds the palette from `self.mode` and `self.appearance`. Without `self.base_color` the rebuild would silently reset the user's choice to Neutral every time the translucency toggle is flipped.

- [ ] **Step 1: Write the failing test**

Add to the existing `mod tests` in `rust/crates/sirio_theme/src/lib.rs`:

```rust
    #[test]
    fn neutral_reproduces_the_palette_shipped_before_base_colours() {
        // The upgrade guard. `Tint::NONE` is bezel's shipped grey, so an
        // install that has never touched the new setting must paint exactly
        // what it painted yesterday — every token, both appearances.
        for appearance in [Appearance::Light, Appearance::Dark] {
            let neutral = ThemeColors::for_appearance(appearance, BaseColor::Neutral);
            let bezel = match appearance {
                Appearance::Dark => bezel::theme::Theme::dark(),
                Appearance::Light => bezel::theme::Theme::light(),
            };
            assert_eq!(
                neutral.bg,
                Rgba::from(bezel.bg),
                "{appearance:?} bg is bezel's untinted page"
            );
            assert_eq!(neutral.surface, Rgba::from(bezel.surface));
            assert_eq!(neutral.border, Rgba::from(bezel.border));
        }
    }

    #[test]
    fn a_tinted_base_moves_the_greys_and_leaves_sirios_own_colours_alone() {
        // Decision B4: the coral is Sirio's identity, anchored by two
        // measured constraints, and the terminal well is deliberately
        // independent of the shell's panel hierarchy. Neither rotates.
        for appearance in [Appearance::Light, Appearance::Dark] {
            let neutral = ThemeColors::for_appearance(appearance, BaseColor::Neutral);
            let slate = ThemeColors::for_appearance(appearance, BaseColor::Slate);

            assert_ne!(slate.bg, neutral.bg, "{appearance:?} page takes the tint");
            assert_ne!(slate.surface, neutral.surface);
            assert_ne!(slate.border, neutral.border);

            assert_eq!(
                slate.brand_coral, neutral.brand_coral,
                "{appearance:?} coral is Sirio's, not bezel's to rotate"
            );
            assert_eq!(
                slate.terminal_surface, neutral.terminal_surface,
                "{appearance:?} terminal well stays out of the panel hierarchy"
            );
        }
    }

    #[test]
    fn the_semantic_hues_hold_under_every_base_colour() {
        // `Brand::apply` keeps danger, warning and success where they are
        // because they mean something. Asserted from Sirio's side too, so a
        // bezel bump that changed the rule fails here rather than shipping a
        // status colour nobody chose.
        for appearance in [Appearance::Light, Appearance::Dark] {
            let neutral = ThemeColors::for_appearance(appearance, BaseColor::Neutral);
            for base in BaseColor::ALL {
                let tinted = ThemeColors::for_appearance(appearance, base);
                assert_eq!(tinted.danger, neutral.danger, "{base:?} danger");
                assert_eq!(tinted.warning, neutral.warning, "{base:?} warning");
                assert_eq!(tinted.success, neutral.success, "{base:?} success");
            }
        }
    }

    #[test]
    fn translucency_preserves_the_chosen_base_colour() {
        // `with_translucency` rebuilds the palette from the theme's own
        // fields; the base colour has to be one of them or the toggle
        // silently resets the user's choice.
        let theme = Theme::for_appearance(ThemeMode::Dark, Appearance::Dark, BaseColor::Slate);
        let translucent = theme.with_translucency(true);
        assert_eq!(translucent.base_color, BaseColor::Slate);
        assert_eq!(
            translucent.colors.border,
            theme.colors.border,
            "a non-faded token keeps the tinted value"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd rust && cargo test -p sirio_theme
```

Expected: FAIL to compile — `this function takes 1 argument but 2 arguments were supplied` at `ThemeColors::for_appearance`, and `no field base_color on type Theme`.

- [ ] **Step 3: Write minimal implementation**

3a. `ThemeColors::for_appearance` at `:244` — change the signature and the palette read. Everything after the `let bezel = …` block is untouched:

```rust
    fn for_appearance(appearance: Appearance, base: BaseColor) -> Self {
        // Every neutral, status and diff token below is bezel's. What stays
        // Sirio's is listed in `Group C` of the design doc: the coral, the
        // window-frame material, the terminal surface, and the washes that sit
        // between bezel's rungs.
        //
        // The palette is read here rather than through bezel's `wash`/`ink`
        // helpers because those resolve against a process-global appearance,
        // and this function is called for both appearances in one process.
        // `branded` is the same reason the tint arrives as a parameter: a
        // brand global would put back exactly the problem those helpers have.
        let bezel = bezel::theme::Theme::branded(
            &bezel::theme::Brand {
                tint: base.tint(),
                ..Default::default()
            },
            match appearance {
                Appearance::Dark => bezel::theme::Appearance::Dark,
                Appearance::Light => bezel::theme::Appearance::Light,
            },
        );
```

3b. `Theme` struct at `:1102` — add the field after `appearance`:

```rust
    /// The active resolved appearance.
    pub appearance: Appearance,
    /// The neutral family the greys are tinted with. Carried on the theme so
    /// every reinstall (`install`, `set_mode`, `with_translucency`, the
    /// portal follower) preserves it by construction — the same reason
    /// `translucency_enabled` lives here.
    pub base_color: BaseColor,
```

3c. `Theme::for_appearance` at `:1362`:

```rust
    fn for_appearance(mode: ThemeMode, appearance: Appearance, base: BaseColor) -> Self {
        let colors = ThemeColors::for_appearance(appearance, base);
        Self {
            mode,
            appearance,
            base_color: base,
            colors,
            spacing: Spacing::default(),
            radii: Radii::default(),
            browser_chrome: BrowserChrome::default(),
            windows_caption: WindowsCaption::default(),
            typography: Typography::default(),
            translucent_surface_opacity: Self::surface_opacity(true),
            translucency_enabled: false,
        }
    }
```

3d. The callers, in order of line number:

```rust
    // :1274, inside follow_portal
                    let next = Theme::for_appearance(
                        ThemeMode::System,
                        preference,
                        cx.global::<Theme>().base_color,
                    )
                    .with_translucency(cx.global::<Theme>().translucency_enabled);

    // :1285, for_mode
    pub fn for_mode(
        mode: ThemeMode,
        system_appearance: WindowAppearance,
        base: BaseColor,
    ) -> Self {
        let appearance = resolve_mode(mode, system_appearance);
        Self::for_appearance(mode, appearance, base)
    }

    // :1292, for_mode_linux
    #[cfg(target_os = "linux")]
    fn for_mode_linux(
        mode: ThemeMode,
        system_appearance: WindowAppearance,
        base: BaseColor,
    ) -> Self {
        let appearance = resolve_mode_linux(mode, system_appearance);
        Self::for_appearance(mode, appearance, base)
    }

    // :1299 and :1304 — the zero-argument helpers stay zero-argument
    pub fn light() -> Self {
        Self::for_appearance(ThemeMode::Light, Appearance::Light, BaseColor::Neutral)
    }

    pub fn dark() -> Self {
        Self::for_appearance(ThemeMode::Dark, Appearance::Dark, BaseColor::Neutral)
    }

    // :1342, inside with_translucency
        let mut theme = Self::for_appearance(self.mode, self.appearance, self.base_color);
```

3e. The four existing test call sites at `:1714`, `:1765`, `:1844` and `:1857` all read `ThemeColors::for_appearance(appearance)`. Add the second argument to each:

```rust
        let sirio = ThemeColors::for_appearance(appearance, BaseColor::Neutral);
```

(at `:1857` the binding is named `c`, not `sirio` — change only the call, not the name.)

Also check the system-mode helper immediately after `dark()` (the one whose doc-comment begins "Returns a system-mode theme resolved against the supplied appearance"): if it calls `Self::for_appearance`, give it a `base: BaseColor` parameter and forward it, exactly like `for_mode`.

- [ ] **Step 4: Run test to verify it passes**

```bash
cd rust && cargo test -p sirio_theme
```

Expected: PASS. The pre-existing `dark_palette_comes_from_bezel` and `light_palette_comes_from_bezel` must pass **unchanged** — if either fails, `Neutral` is not reproducing the shipped palette and the implementation is wrong, not the test.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_theme/src/lib.rs
git commit -m "feat: build the palette from a chosen bezel base colour"
```

---

### Task 3: Install and switch the base colour

`Theme::install` recovers the current choice from the global; `Theme::set_base_color` swaps it.

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs:1189` (`Theme::install`) and the `set_mode` immediately below it
- Test: `rust/crates/sirio_theme/src/lib.rs` (the existing `mod tests`)

**Interfaces:**
- Consumes: `Theme.base_color`, `Theme::for_mode` from Task 2.
- Produces: `Theme::install(mode: ThemeMode, cx: &mut App)` — signature unchanged; `Theme::set_base_color(base: BaseColor, cx: &mut App)`; `Theme::current_base_color(cx: &App) -> BaseColor`. Task 6 calls `set_base_color`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `rust/crates/sirio_theme/src/lib.rs`:

```rust
    #[test]
    fn the_base_colour_defaults_to_neutral_when_nothing_is_installed() {
        // A pure test of the recovery rule; no gpui context involved.
        assert_eq!(BaseColor::default(), BaseColor::Neutral);
        assert_eq!(Theme::dark().base_color, BaseColor::Neutral);
    }

    #[test]
    fn a_reinstall_keeps_the_base_colour_the_way_it_keeps_translucency() {
        // `install` recovers both from the previously installed theme. This
        // is the pure half of that contract: rebuilding for a new mode from
        // an existing theme's fields must carry the choice across.
        let installed =
            Theme::for_appearance(ThemeMode::Dark, Appearance::Dark, BaseColor::Zinc);
        let next = Theme::for_appearance(
            ThemeMode::Light,
            Appearance::Light,
            installed.base_color,
        );
        assert_eq!(next.base_color, BaseColor::Zinc);
        assert_ne!(
            next.colors.bg,
            ThemeColors::for_appearance(Appearance::Light, BaseColor::Neutral).bg,
            "the light rebuild is still tinted"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd rust && cargo test -p sirio_theme base_colour
```

Expected: FAIL to compile — `Theme::for_appearance` is private to the crate but reachable from `mod tests`; the failure is the missing `base_color` field only if Task 2 was skipped. If Task 2 is in place these two tests pass immediately; that is expected and fine — they are the regression guard for Step 3, which changes `install`. Proceed to Step 3 and re-run.

- [ ] **Step 3: Write minimal implementation**

Replace `Theme::install` (`:1189`) and add the two new functions after `set_mode`:

```rust
    pub fn install(mode: ThemeMode, cx: &mut App) {
        Self::resolve_font_families(cx);
        // Both the base colour and the translucency flag are recovered from
        // whatever is already installed: `install` is called from mode
        // switches and from the portal follower, neither of which knows or
        // should know about the other two axes.
        let previous = cx.try_global::<Self>();
        let base = previous.map_or_else(BaseColor::default, |theme| theme.base_color);
        let translucency = previous.is_some_and(|theme| theme.translucency_enabled);
        #[cfg(target_os = "linux")]
        let theme = Self::for_mode_linux(mode, cx.window_appearance(), base);
        #[cfg(not(target_os = "linux"))]
        let theme = Self::for_mode(mode, cx.window_appearance(), base);
        let theme = theme.with_translucency(translucency);
        theme.sync_appearance();
        cx.set_global(theme);
    }

    /// The base colour the installed theme is painting with, or the default
    /// when no theme is installed yet.
    pub fn current_base_color(cx: &App) -> BaseColor {
        cx.try_global::<Self>()
            .map_or_else(BaseColor::default, |theme| theme.base_color)
    }

    /// Installs a replacement base colour, keeping the current mode.
    ///
    /// The mode is read back rather than passed in because the picker that
    /// calls this changes one axis and must not restate the other — a caller
    /// that guessed `System` here would undo an explicit Light or Dark.
    pub fn set_base_color(base: BaseColor, cx: &mut App) {
        let mode = cx
            .try_global::<Self>()
            .map_or(ThemeMode::System, |theme| theme.mode);
        let translucency = cx
            .try_global::<Self>()
            .is_some_and(|theme| theme.translucency_enabled);
        #[cfg(target_os = "linux")]
        let theme = Self::for_mode_linux(mode, cx.window_appearance(), base);
        #[cfg(not(target_os = "linux"))]
        let theme = Self::for_mode(mode, cx.window_appearance(), base);
        let theme = theme.with_translucency(translucency);
        theme.sync_appearance();
        cx.set_global(theme);
    }
```

Note: `cx.try_global::<Self>()` is called twice in `set_base_color` because the borrow from the first call ends before `cx` is used mutably. Keep it that way rather than binding once and holding the borrow.

- [ ] **Step 4: Run test to verify it passes**

```bash
cd rust && cargo test -p sirio_theme
```

Expected: PASS, whole crate.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_theme/src/lib.rs
git commit -m "feat: install and switch the theme base colour"
```

---

### Task 4: Persist the choice

The serde-only twin enum, its key, and the loader branch.

**Files:**
- Modify: `rust/crates/sirio_persistence/src/model.rs:344-392` (beside `AppearanceMode` and `FileIconTheme`), `:399` (`AppSettings`), `:463` (`Default`), `:496` (`settings_keys`)
- Modify: `rust/crates/sirio_persistence/src/db.rs:912` (`settings`) and the corresponding writer
- Test: `rust/crates/sirio_persistence/src/model.rs` (inline tests) and `rust/crates/sirio_persistence/tests/persistence_integration.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks. This crate must stay free of `bezel` and `sirio_theme`.
- Produces: `sirio_persistence::BaseColor` with the same five variants, `raw()`, `parse()`, `Default = Neutral`; `AppSettings.base_color: BaseColor`; `settings_keys::BASE_COLOR = "appearance.baseColor"`. Task 5 converts between this and `sirio_theme::BaseColor`.

- [ ] **Step 1: Write the failing test**

Add to the inline test module in `rust/crates/sirio_persistence/src/model.rs`:

```rust
    #[test]
    fn base_colour_round_trips_through_its_raw_value() {
        for base in [
            BaseColor::Neutral,
            BaseColor::Stone,
            BaseColor::Zinc,
            BaseColor::Gray,
            BaseColor::Slate,
        ] {
            assert_eq!(BaseColor::parse(base.raw()), Some(base), "{base:?}");
        }
        assert_eq!(
            BaseColor::raw(BaseColor::Neutral),
            "neutral",
            "the raw values are lower-case, like every other enum setting"
        );
    }

    #[test]
    fn an_unknown_base_colour_is_not_parsed() {
        // The loader turns None into the default; parse itself does not
        // guess, matching AppearanceMode::parse.
        assert_eq!(BaseColor::parse("cerulean"), None);
        assert_eq!(BaseColor::parse(""), None);
    }

    #[test]
    fn settings_default_to_the_neutral_base_colour() {
        assert_eq!(AppSettings::default().base_color, BaseColor::Neutral);
    }
```

Add to `rust/crates/sirio_persistence/tests/persistence_integration.rs`. The
`TempDir` / open / drop / reopen shape is the one the file's other settings
tests already use (see the test around `:471`):

```rust
#[test]
fn base_colour_survives_a_relaunch() {
    let dir = TempDir::new();
    let database_path = dir.0.join("sirio.sqlite3");

    {
        let db = Database::open(&database_path).expect("open");
        let mut settings = db.settings().expect("load defaults");
        assert_eq!(
            settings.base_color,
            BaseColor::Neutral,
            "an install that never wrote the key reads as neutral"
        );
        settings.base_color = BaseColor::Slate;
        db.save_settings(&settings).expect("save settings");
    }
    // Drop closes the connection; reopen simulates a relaunch.
    let db = Database::open(&database_path).expect("reopen");
    assert_eq!(db.settings().expect("load settings").base_color, BaseColor::Slate);
}
```

Use the same opening call and type name the neighbouring tests use — copy the
first two lines of the test at `:460` rather than trusting `Database::open`
above — and add `BaseColor` to the file's existing `use sirio_persistence::{…}`
list.

- [ ] **Step 2: Run test to verify it fails**

```bash
cd rust && cargo test -p sirio_persistence base_colour
```

Expected: FAIL — `cannot find type BaseColor in this scope`.

- [ ] **Step 3: Write minimal implementation**

3a. In `rust/crates/sirio_persistence/src/model.rs`, immediately after the `FileIconTheme` impl block (which ends at `:392`):

```rust
/// The bezel base colour the palette's greys are tinted with. The serde
/// half of `sirio_theme::BaseColor`, kept here for the same reason
/// `AppearanceMode` is: this crate carries the storage contract and must not
/// depend on the theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BaseColor {
    #[default]
    Neutral,
    Stone,
    Zinc,
    Gray,
    Slate,
}

impl BaseColor {
    pub fn raw(self) -> &'static str {
        match self {
            BaseColor::Neutral => "neutral",
            BaseColor::Stone => "stone",
            BaseColor::Zinc => "zinc",
            BaseColor::Gray => "gray",
            BaseColor::Slate => "slate",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "neutral" => Some(BaseColor::Neutral),
            "stone" => Some(BaseColor::Stone),
            "zinc" => Some(BaseColor::Zinc),
            "gray" => Some(BaseColor::Gray),
            "slate" => Some(BaseColor::Slate),
            _ => None,
        }
    }
}
```

3b. In `AppSettings` (`:399`), after `terminal_font_size`:

```rust
    /// "appearance.baseColor" — default: neutral.
    pub base_color: BaseColor,
```

3c. In `impl Default for AppSettings` (`:463`), after `terminal_font_size: 13,`:

```rust
            base_color: BaseColor::Neutral,
```

3d. In `settings_keys` (`:496`), after `TERMINAL_FONT_SIZE`:

```rust
    pub const BASE_COLOR: &str = "appearance.baseColor";
```

3e. In `db.rs`, in `settings()` after the `TERMINAL_FONT_SIZE` branch:

```rust
        if let Some(value) = self.setting_value(settings_keys::BASE_COLOR)? {
            defaults.base_color = BaseColor::parse(&value).unwrap_or(BaseColor::Neutral);
        }
```

Import `BaseColor` alongside the existing `FileIconTheme` import at the top of `db.rs`.

3f. The writer is `save_settings` at `db.rs:1015`, a run of `set_setting(...)`
calls — the `FILE_ICON_THEME` one is at `:1032-1035`. Add a matching call
after the `TERMINAL_FONT_SIZE` write, copying the exact argument shape of its
neighbours (they pass the transaction, the key and the value):

```rust
        set_setting(
            &transaction,
            settings_keys::BASE_COLOR,
            settings.base_color.raw(),
        )?;
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd rust && cargo test -p sirio_persistence
```

Expected: PASS, whole crate. Other crates will not compile yet — `AppSettings` gained a field and `main.rs` builds it with a struct literal. That is Task 5.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_persistence/src
git add rust/crates/sirio_persistence/tests/persistence_integration.rs
git commit -m "feat: persist the theme base colour"
```

---

### Task 5: Wire the choice through the app

The single conversion between the two enums, plus the snapshot field.

**Files:**
- Modify: `rust/crates/sirio_ui/src/settings.rs:335` (`SettingsSnapshot`), `:390` (`Default`), `:940` (the surface's own fields), `:1209` (construction from `initial`), `:1656` (the snapshot it emits)
- Modify: `rust/crates/sirio/src/main.rs:14899` (`settings_snapshot_from_app_settings`), `:14930` (`app_settings_from_snapshot`), and wherever `theme_mode`/`persisted_appearance` are defined — the two new converters go beside them
- Test: `rust/crates/sirio/src/main.rs` (`mod tests`)

**Interfaces:**
- Consumes: `sirio_theme::BaseColor` (Task 1), `sirio_persistence::BaseColor` (Task 4).
- Produces: `SettingsSnapshot.base_color: sirio_theme::BaseColor`; free functions `theme_base_color(sirio_persistence::BaseColor) -> sirio_theme::BaseColor` and `persisted_base_color(sirio_theme::BaseColor) -> sirio_persistence::BaseColor` in `main.rs`. Task 6 renders from the snapshot field.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `rust/crates/sirio/src/main.rs`:

```rust
    #[test]
    fn the_base_colour_converts_both_ways_across_the_crate_boundary() {
        // CLAUDE.md's rule: persistence owns the serde contract, the theme
        // owns the concept, and main.rs holds the single conversion. Every
        // variant has to survive the trip or a persisted choice silently
        // becomes a different colour.
        let pairs = [
            (
                sirio_persistence::BaseColor::Neutral,
                sirio_theme::BaseColor::Neutral,
            ),
            (
                sirio_persistence::BaseColor::Stone,
                sirio_theme::BaseColor::Stone,
            ),
            (
                sirio_persistence::BaseColor::Zinc,
                sirio_theme::BaseColor::Zinc,
            ),
            (
                sirio_persistence::BaseColor::Gray,
                sirio_theme::BaseColor::Gray,
            ),
            (
                sirio_persistence::BaseColor::Slate,
                sirio_theme::BaseColor::Slate,
            ),
        ];
        for (persisted, theme) in pairs {
            assert_eq!(theme_base_color(persisted), theme, "{persisted:?} inbound");
            assert_eq!(
                persisted_base_color(theme),
                persisted,
                "{theme:?} outbound"
            );
        }
    }

    #[test]
    fn the_settings_snapshot_carries_the_persisted_base_colour() {
        let mut persisted = AppSettings::default();
        persisted.base_color = sirio_persistence::BaseColor::Gray;

        let snapshot = settings_snapshot_from_app_settings(persisted);
        assert_eq!(snapshot.base_color, sirio_theme::BaseColor::Gray);

        let back = app_settings_from_snapshot(snapshot);
        assert_eq!(back.base_color, sirio_persistence::BaseColor::Gray);
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd rust && export PATH="/opt/homebrew/opt/zig@0.15/bin:$PATH" && cargo test -p sirio base_colour
```

Expected: FAIL to compile — `cannot find function theme_base_color`, and `AppSettings` missing-field errors from the existing struct literals.

- [ ] **Step 3: Write minimal implementation**

3a. In `rust/crates/sirio_ui/src/settings.rs`, add to `SettingsSnapshot` (`:335`) after `terminal_font_size`:

```rust
    /// The bezel base colour the greys are tinted with (Appearance → Theme).
    pub base_color: sirio_theme::BaseColor,
```

and to its `Default` (`:390`), after `terminal_font_size: 13,`:

```rust
            base_color: sirio_theme::BaseColor::Neutral,
```

3b. In the settings surface's own field list (`:940`), after `terminal_font_size: i32,`:

```rust
    base_color: sirio_theme::BaseColor,
```

At `:1209`, in the block that builds those fields from `initial`, add beside the neighbouring lines:

```rust
            base_color: initial.base_color,
```

At `:1656`, in the `SettingsSnapshot` the surface emits, add beside `terminal_font_size: self.terminal_font_size,`:

```rust
            base_color: self.base_color,
```

3c. In `rust/crates/sirio/src/main.rs`, beside the existing `theme_mode` / `persisted_appearance` converters:

```rust
/// The persisted base colour as the theme's own enum. The single conversion
/// between the two, per CLAUDE.md: `sirio_persistence` carries the serde
/// contract and never depends on `sirio_theme`.
fn theme_base_color(base: sirio_persistence::BaseColor) -> sirio_theme::BaseColor {
    match base {
        sirio_persistence::BaseColor::Neutral => sirio_theme::BaseColor::Neutral,
        sirio_persistence::BaseColor::Stone => sirio_theme::BaseColor::Stone,
        sirio_persistence::BaseColor::Zinc => sirio_theme::BaseColor::Zinc,
        sirio_persistence::BaseColor::Gray => sirio_theme::BaseColor::Gray,
        sirio_persistence::BaseColor::Slate => sirio_theme::BaseColor::Slate,
    }
}

/// The inverse of [`theme_base_color`].
fn persisted_base_color(base: sirio_theme::BaseColor) -> sirio_persistence::BaseColor {
    match base {
        sirio_theme::BaseColor::Neutral => sirio_persistence::BaseColor::Neutral,
        sirio_theme::BaseColor::Stone => sirio_persistence::BaseColor::Stone,
        sirio_theme::BaseColor::Zinc => sirio_persistence::BaseColor::Zinc,
        sirio_theme::BaseColor::Gray => sirio_persistence::BaseColor::Gray,
        sirio_theme::BaseColor::Slate => sirio_persistence::BaseColor::Slate,
    }
}
```

3d. In `settings_snapshot_from_app_settings` (`:14899`), after `terminal_font_size:`:

```rust
        base_color: theme_base_color(settings.base_color),
```

3e. In `app_settings_from_snapshot` (`:14930`), after `terminal_font_size:`:

```rust
        base_color: persisted_base_color(snapshot.base_color),
```

3f. At startup the loaded choice has to reach the installed theme. The site is
`main.rs:15270`, where `saved_settings` is already in hand and the persisted
mode is applied:

```rust
        let saved_settings = app_settings_with_environment_override(session_store.load_settings());
        // The base colour first: `Theme::set_mode` recovers it from the
        // installed theme, so setting the mode before the colour would build
        // one frame with the default and then throw it away.
        Theme::set_base_color(theme_base_color(saved_settings.base_color), cx);
        Theme::set_mode(
            settings_snapshot_from_app_settings(saved_settings.clone()).theme,
            cx,
        );
```

`Theme::init(cx)` at `:15217` runs earlier and installs the neutral default;
that is correct and stays — this pair replaces it once the database has been
read.

- [ ] **Step 4: Run test to verify it passes**

```bash
cd rust && export PATH="/opt/homebrew/opt/zig@0.15/bin:$PATH" && cargo test -p sirio base_colour && cargo test -p sirio_ui
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/settings.rs rust/crates/sirio/src/main.rs
git commit -m "feat: carry the base colour from settings to the theme"
```

---

### Task 6: The Base color row

A third row in the existing Theme card.

**Files:**
- Modify: `rust/crates/sirio_ui/src/settings.rs:45` (the segmented labels), `:1746` area (a new setter beside `set_theme_mode`), `:2472` (`render_appearance`)
- Test: `rust/crates/sirio_ui/src/settings.rs` (`mod tests`)

**Interfaces:**
- Consumes: `SettingsSnapshot.base_color` (Task 5), `sirio_theme::Theme::set_base_color` (Task 3), `sirio_theme::BaseColor::ALL` / `title` (Task 1).
- Produces: `SEGMENTED_BASE_COLOR: &[&str]`; `fn base_color_segment(base: sirio_theme::BaseColor) -> usize`; `fn set_base_color(&mut self, index: usize, cx: &mut Context<Self>)`. Nothing later depends on these.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `rust/crates/sirio_ui/src/settings.rs`:

```rust
    #[test]
    fn the_base_colour_segments_are_bezels_five_in_order() {
        assert_eq!(
            SEGMENTED_BASE_COLOR,
            &["Neutral", "Stone", "Zinc", "Gray", "Slate"]
        );
    }

    #[test]
    fn every_base_colour_maps_to_its_own_segment() {
        for (index, base) in sirio_theme::BaseColor::ALL.into_iter().enumerate() {
            assert_eq!(base_color_segment(base), index, "{base:?}");
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd rust && cargo test -p sirio_ui base_colour
```

Expected: FAIL — `cannot find value SEGMENTED_BASE_COLOR`.

- [ ] **Step 3: Write minimal implementation**

3a. Beside `SEGMENTED_THEME` (`:45`):

```rust
/// bezel's five base colours, in its own order. Text rather than swatches:
/// the five differ by hue at chroma 0.013–0.046, which a 16px pill cannot
/// show, and the names are Tailwind's — a vocabulary a user may already have.
const SEGMENTED_BASE_COLOR: &[&str] = &["Neutral", "Stone", "Zinc", "Gray", "Slate"];

/// The segment index of a base colour. `BaseColor::ALL` is the display order,
/// so the index is its position in that array.
fn base_color_segment(base: sirio_theme::BaseColor) -> usize {
    sirio_theme::BaseColor::ALL
        .iter()
        .position(|candidate| *candidate == base)
        .unwrap_or(0)
}
```

3b. A setter, immediately after `set_theme_mode` (`:1717`):

```rust
    fn set_base_color(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(base) = sirio_theme::BaseColor::ALL.get(index).copied() else {
            return;
        };
        self.base_color = base;
        Theme::set_base_color(base, cx);
        // Same reason as set_theme_mode: Theme is a GPUI global, so every
        // mounted surface repaints from the new palette on its next render.
        cx.refresh_windows();
        self.changed();
        cx.notify();
    }
```

3c. In `render_appearance` (`:2472`), build the control next to `theme_control` and insert the row between Appearance and Translucency:

```rust
        let base_entity = entity.clone();
        let base_control = controls::segmented(
            "appearance-base-color",
            SEGMENTED_BASE_COLOR,
            base_color_segment(self.base_color),
            theme,
            move |index, cx| {
                base_entity.update(cx, |this, cx| this.set_base_color(index, cx));
            },
        );
```

and, in the `theme_card` chain, after the Appearance row's `controls::separator(theme)` and before the Translucency row:

```rust
        theme_card = theme_card
            .child(
                div()
                    .id("settings-appearance-base-color-row")
                    .child(controls::row("Base color", None, base_control, theme)),
            )
            .child(controls::separator(theme));
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd rust && cargo test -p sirio_ui
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/settings.rs
git commit -m "feat: add the base colour picker to Appearance"
```

---

### Task 7: Remove the file-icon setting

Dead configuration: persisted, plumbed, echoed on the socket, drawn as a picker, read by nothing.

**Files:**
- Modify: `rust/crates/sirio_ui/src/settings.rs` — delete `SEGMENTED_FILE_ICONS` (`:49-52`), `FileIconChoice` and its impl (`:140-166`), `file_icon_choices` (`:172`), `clamp_file_icons` (`:186`), `file_icons_segment` (`:197`), the snapshot field (`:344`) and its default (`:395`), the surface field (`:948`), the construction (`:1209`), the emit (`:1656`), `set_file_icons` (`:1746`), the Files card and its section child (`:2550-2558` and the `settings_section("Files", …)` line in `render_appearance`), and the tests at `:4794`, `:4844`, `:4848`, `:4859`, `:4863`, `:4866`, `:4879`
- Modify: `rust/crates/sirio_persistence/src/model.rs` — delete `FileIconTheme` (`:370-392`), the `AppSettings` field (`:407`), its default (`:469`), `settings_keys::FILE_ICON_THEME` (`:500`)
- Modify: `rust/crates/sirio_persistence/src/db.rs` — delete the loader branch (`:929`) and the matching writer line
- Modify: `rust/crates/sirio/src/main.rs` — delete the `"fileIcons"` socket entry (`:3208-3211`), the two conversions (`:14903`, `:14936`), and the assertion at `:21995`
- Test: the existing suites; this task removes tests rather than adding them

**Interfaces:**
- Consumes: nothing.
- Produces: nothing. `FileIconChoice`, `FileIconTheme` and `"appearance.fileIconTheme"` cease to exist.

Do **not** touch: `FileIconKey` (`sirio_project`), `Icon::file_type`, `key.material_asset()`, the Material assets, or `file_glyph` in `right_panel/files.rs`. Those are what the file tree actually renders. Do **not** delete `controls::color_picker` — `project_identity.rs:720` uses it.

- [ ] **Step 1: Establish the red check**

This task adds no behaviour, so there is no unit test to write. The compiler
covers the typed removals — `snapshot.file_icons` stops existing, and every
consumer becomes an error. What the compiler cannot catch is a surviving
hardcoded `"fileIcons"` string in the socket echo, so that is the check:

```bash
cd rust && grep -rn '"fileIcons"\|file_icon\|FileIconChoice\|FileIconTheme' crates --include='*.rs'
```

The echo entry lives in `settings_report_pairs` (`main.rs:3156`), whose only
caller is `main.rs:9336`; it is built from a `SettingsReport` produced by a
gpui entity, which is why this is a grep and not a fixture.

- [ ] **Step 2: Run the check to see it fail**

Run the grep above. Expected: matches, including `main.rs:3209` `"fileIcons"`.
Record the count — this is the list Step 3 drives to zero.

- [ ] **Step 3: Write minimal implementation**

Delete every site listed under **Files** above. Work outward from the leaves so the compiler names the next site for you:

```bash
cd rust
export PATH="/opt/homebrew/opt/zig@0.15/bin:$PATH"
cargo build -p sirio_persistence   # then sirio_ui, then sirio
```

Two sites need judgement rather than deletion:

- `settings_snapshot_from_app_settings` and `app_settings_from_snapshot` (`main.rs:14903`, `:14936`): remove the whole `file_icons:` / `file_icon_theme:` entry, not just its body.
- `render_appearance`: remove both the `files_card` binding and the `.child(settings_section("Files", files_card, theme))` line. Leaving one behind compiles and renders an empty section.

- [ ] **Step 4: Run test to verify it passes**

```bash
cd rust && export PATH="/opt/homebrew/opt/zig@0.15/bin:$PATH" && cargo test -p sirio_persistence && cargo test -p sirio_ui && cargo test -p sirio
```

Expected: PASS, three crates, with no `unused` warnings from the removal.

Then re-run the Step 1 grep. Expected: **no matches**. Any survivor is either a
missed deletion or a `FileIconKey` false positive — `FileIconKey` is
`sirio_project`'s and stays, so check the crate before deleting anything the
grep names.

- [ ] **Step 5: Commit**

```bash
git add rust/crates
git commit -m "refactor: drop the file-icon setting the renderer never read"
```

---

### Task 8: Remove the agent-colour override

The picker and its side store go; the brand table stays.

**Files:**
- Modify: `rust/crates/sirio_ui/src/settings.rs` — delete `AgentAccentColor` and its impl (`:263-330` — the enum, `ALL`, `id`, `resolve`, `parse`), the snapshot field (`:384`) and its default (`:416`), the surface field (`:1125`), the construction (`:1267`), the emit (`:1672`), `set_agent_color` (`:1762`), `render_agent_colors` (`:2573-2643`), the `agent_card` binding and the `settings_section("Agent Colors", …)` child in `render_appearance` (`:2560`), and the tests covering them
- Modify: `rust/crates/sirio/src/session.rs` — delete `load_agent_color_ids` (`:1622`), `save_agent_color_id` (`:1635`) and the doc-comment at `:153` that explains them
- Modify: `rust/crates/sirio/src/main.rs` — delete the snapshot default at `:14926`, the restore block at `:15383-15392`, and the save loop at `:15566-15570`
- Test: the existing suites; this task removes tests rather than adding them

**Interfaces:**
- Consumes: nothing.
- Produces: nothing.

Do **not** touch `sirio_theme::AgentBrandColor` (`lib.rs:1450`) or `for_agent_id`. The sidebar (`sidebar.rs:60`), the icons (`icons.rs:378`) and the tab strip read it; removing it strips every agent mark of its colour. Do **not** remove `brand_coral` — it loses the picker's Coral entry and keeps `loading::bezel_theme`. Do **not** remove `controls::color_picker`; `project_identity.rs:720` uses it.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `rust/crates/sirio/src/main.rs`:

```rust
    #[test]
    fn agent_marks_still_have_their_brand_colours_without_the_picker() {
        // The picker was a user override of a table that stands on its own.
        // Removing the override must leave the table — the sidebar, the icons
        // and the tab strip all resolve a mark's colour through it.
        for (id, expected) in [
            ("claude", AgentBrandColor::Claude),
            ("codex", AgentBrandColor::Codex),
            ("opencode", AgentBrandColor::OpenCode),
            ("pi", AgentBrandColor::Pi),
            ("omp", AgentBrandColor::Omp),
        ] {
            assert_eq!(AgentBrandColor::for_agent_id(id), expected);
        }
        assert_eq!(
            AgentBrandColor::for_agent_id("something-new"),
            AgentBrandColor::Unknown
        );
    }
```

This test passes before the removal too. That is deliberate: it is the guard that the removal does not go one step too far, and it must still pass after Step 3.

- [ ] **Step 2: Run test to verify it fails**

```bash
cd rust && export PATH="/opt/homebrew/opt/zig@0.15/bin:$PATH" && cargo test -p sirio agent_marks_still
```

Expected: PASS. This is the one step in the plan with no red phase — there is no behaviour to add, and the test's job is to fail *after* an over-eager deletion, not before it. Note the result and continue.

- [ ] **Step 3: Write minimal implementation**

Delete every site listed under **Files** above, leaves first:

```bash
cd rust
export PATH="/opt/homebrew/opt/zig@0.15/bin:$PATH"
cargo build -p sirio_ui   # then sirio
```

Watch for two things the compiler will not catch:

- `render_appearance` must lose both the `agent_card` binding and its `settings_section` child.
- `session.rs:153`'s doc-comment references `SessionStore::load_agent_color_ids` from a type that survives. A dangling doc-link to a deleted method is a rustdoc warning and a lie; delete the sentence.

- [ ] **Step 4: Run test to verify it passes**

```bash
cd rust && export PATH="/opt/homebrew/opt/zig@0.15/bin:$PATH" && cargo test -p sirio_ui && cargo test -p sirio
```

Expected: PASS, including `agent_marks_still_have_their_brand_colours_without_the_picker`.

- [ ] **Step 5: Commit**

```bash
git add rust/crates
git commit -m "refactor: drop the per-agent accent override"
```

---

### Task 9: Update the provenance record and close the gate

**Files:**
- Modify: `docs/THEME-PROVENANCE.md` (the `brand_coral` row of the "Sirio's own" table, and the colours section)
- Test: `Scripts/ci.sh`

**Interfaces:**
- Consumes: everything above.
- Produces: nothing.

- [ ] **Step 1: Update the `brand_coral` row**

In `docs/THEME-PROVENANCE.md`, the `brand_coral` row currently reads "The Coral entry of the agent-colour picker reads it, and `sirio_ui`'s `loading::bezel_theme` puts it on bezel's `accent` …". The picker is gone. Replace that clause with:

```
`sirio_ui`'s `loading::bezel_theme` puts it on bezel's `accent` so the loaders keep painting Sirio's colour rather than bezel's grey.
```

- [ ] **Step 2: Record the base colour in the colours section**

Append to the "### 1. Colours — bezel's, at the pinned version" section, after the paragraph about `dark_palette_comes_from_bezel`:

```markdown
**The greys carry a chosen hue.** Settings → Appearance offers bezel's five
base colours (`BASE_COLORS` in bezel's `brand.rs`: Neutral, Stone, Zinc, Gray,
Slate), and `ThemeColors::for_appearance` builds its bezel palette through
`Theme::branded` rather than `Theme::dark()` / `light()`. Only hue moves —
lightness is never a knob, so every family holds the contrast the shipped
palette was verified at, and the semantic hues (danger, warning, success) keep
their own. `Neutral` is `Tint::NONE`, which reproduces the shipped palette
exactly; it is the default, and it is why the two palette tests above still
compare against bare `bezel::theme::Theme::dark()` / `light()`.

Two tokens do not rotate. `brand_coral` is Sirio's identity and is anchored by
its own two measured constraints. `terminal_surface` stays out of it because
the terminal is deliberately independent of the shell's panel hierarchy and
the sixteen ANSI colours read against it — see
`a_tinted_base_moves_the_greys_and_leaves_sirios_own_colours_alone`.
```

- [ ] **Step 3: Run the full gate**

```bash
export PATH="/opt/homebrew/opt/zig@0.15/bin:$PATH"
Scripts/ci.sh
```

Expected: `CI OK`.

If the workspace-wide run flakes, re-run the two timing-sensitive tests per-crate as `Scripts/ci-linux.sh`'s `WORKSPACE_CRATES` comment describes, rather than treating the flake as a regression.

- [ ] **Step 4: Verify the Appearance page by eye**

```bash
Scripts/build-dev.sh
```

Open Settings → Appearance and confirm: three sections (Theme, Interface, Terminal); the Theme card has Appearance, Base color, Translucency; switching to Slate re-tints the window frame and panels while the terminal well and any coral mark stay put; the choice survives a restart.

- [ ] **Step 5: Commit**

```bash
git add docs/THEME-PROVENANCE.md
git commit -m "docs: record the base colour in the theme provenance"
```

---

## Notes for the executor

- **`Theme::light()` and `Theme::dark()` keep their signatures on purpose.** Around 199 test call sites use them. If you find yourself editing them one by one, you have taken a wrong turn in Task 2.
- **`Neutral` must be a no-op.** The two pre-existing tests `dark_palette_comes_from_bezel` and `light_palette_comes_from_bezel` are the guard. If you need to change either one, the implementation is wrong.
- **The two removal tasks delete more lines than the six feature tasks add.** That is the expected shape; if a removal is growing new code, re-read the "Do not touch" list for that task.
