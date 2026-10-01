# Theme on Ely's palette — evidence

Proof that sub-project 1 of the bezel → Ely migration
(`docs/superpowers/specs/2026-10-01-sirio-theme-on-ely-palette-design.md`)
changes no colour any consumer reads.

## Baseline

`before/` was written on `f1b578b3` plus the dump example alone:

    cd rust && cargo run -p sirio_ui --example theme_dump -- ../docs/testing/theme-ely-palette/before

| File | Holds |
|---|---|
| `sirio.tsv` | Sirio's 38 tokens, 28 combinations (7 bases × dark/light × opaque/translucent), as the `Hsla` gpui paints |
| `ely.tsv` | the `Palette` in Ely's global, same combinations |
| `bezel.txt` | bezel-theme's installed `Theme`, Debug-printed |
| `theme.txt` | Spacing, radii, typography and chrome tokens |
