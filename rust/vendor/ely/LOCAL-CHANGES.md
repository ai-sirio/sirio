# Upstream Ely, vendored verbatim

- Source: https://github.com/ELY-Open-Source/Ely-GPUI-Components
- Rev: `8d7723ec` (never moved)
- Crates: `ely-core`, `ely-chat`, `ely-agent`, `ely-editor`, `ely-git`, `ely-documents`,
  `ely-collab`, `ely-charts`, under `crates/`.
- File set taken from each crate: `Cargo.toml`, `src/`, and `icons/`, `assets/` and `LICENSE` where
  present. Never `AGENTS.md` or `examples/`. `LICENSE` at this directory's root is upstream's.
- Nothing in Sirio depends on these crates yet.
- Check: `artifacts/ely-sync/drivers/check-vendor.sh` extracts each crate at the rev, diffs it
  against this directory and fails on any differing path not named below. It prints
  `VENDOR CHECK OK`.

## Changed paths

Every path below is a path the check accepts as different from upstream. Anything else differing
is a failure.

### Manifests (all eight)

`crates/<crate>/Cargo.toml` for each crate:

- `x.workspace = true` becomes upstream's workspace value, literally (package fields, third-party
  dependencies). `[lints]` tables are dropped. `publish = false` is added.
- `gpui` and `gpui_platform` stay `workspace = true`, now Sirio's workspace entries. Each crate gains
  a `[target.'cfg(target_os = "linux")'.dependencies]` section enabling gpui's `wayland` and `x11`,
  the same section the old `ely-gpui-component` carries.
- `ely-core`: `default = []` (was `["fonts"]`), and the `five_icons` `[[example]]` is dropped with
  `examples/`.

- `crates/ely-core/Cargo.toml`
- `crates/ely-chat/Cargo.toml`
- `crates/ely-agent/Cargo.toml`
- `crates/ely-editor/Cargo.toml`
- `crates/ely-git/Cargo.toml`
- `crates/ely-documents/Cargo.toml`
- `crates/ely-collab/Cargo.toml`
- `crates/ely-charts/Cargo.toml`

### Ignored tests

Two `ely-documents` tests of upstream's `e71b644b` need upstream's patched pulldown-cmark. Each
gains `#[ignore = "needs upstream's patched pulldown-cmark; step 2"]` and nothing else.

- `crates/ely-documents/src/markdown/tex/tests.rs`
- `crates/ely-documents/src/tree/tests/inline.rs`

### The palette split (`ely-palette`), rows 58 to 60

`Palette`, `Syntax`, `SyntaxTheme`, `syntax_themes`, `Mix`, `HUE_NAMES`, `color_blind_chart` and
`Mode` (with `From<WindowAppearance>`) now live in the sibling crate `rust/vendor/ely-palette`,
re-extracted from this rev's `ely-core` theme files. `ely-core` depends on it and re-exports the
types at the original `theme::` paths, so `ely_core::theme::{Palette, Syntax, Mode}` are the same
types Sirio's theme crate holds.

- `crates/ely-core/Cargo.toml`: gains `ely-palette.workspace = true`.
- `crates/ely-core/src/theme/mod.rs`: `Mode` and its `From<WindowAppearance>` impl are removed; the
  `palette` and `syntax` modules are gone; `pub use ely_palette::{HUE_NAMES, Mix, Mode, Palette,
  Syntax, SyntaxTheme, syntax_themes}`; the `color_blind_chart` call reads `ely_palette::`.
- `crates/ely-core/src/theme/palette.rs`, `crates/ely-core/src/theme/syntax.rs`: deleted; their
  content moved into `ely-palette`.

In `rust/vendor/ely-palette`: `Palette` and `Syntax` derive `Copy` (all fields are `Hsla`, so
Sirio's `Theme` stays `Copy`), and `color_blind_chart` is `pub` instead of `pub(crate)`. The colour
values are upstream's at this rev (the dark palette and the chart hues moved on since the old
extraction); upstream's `.clone()` calls stay (row 61 is cosmetic and not carried here).

Outcomes:

- Row 58: re-applied (split re-made on this rev's files). Proof: the closure check and
  `cargo nextest run -p sirio_theme`, which compile `Mode`/`Palette` as one type across both.
- Row 59: re-applied. Proof: `sirio_theme`'s `ThemeColors` derives `Copy` over a `Palette` field,
  so it fails to compile without it (`cargo nextest run -p sirio_theme`).
- Row 60: re-applied. Proof: `ely-core` calls `ely_palette::color_blind_chart` across the crate
  boundary; it does not compile with `pub(crate)`.

- `crates/ely-core/src/theme/mod.rs`
- `crates/ely-core/src/theme/palette.rs`
- `crates/ely-core/src/theme/syntax.rs`
