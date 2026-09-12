# Bundled fonts

## JetBrainsMono Nerd Font Mono

The terminal face `sirio_theme::BUNDLED_TERMINAL_FAMILY` names and
`sirio_theme::register_bundled_terminal_font` registers. JetBrains Mono 2.304
patched by [Nerd Fonts](https://github.com/ryanoasis/nerd-fonts) v3.5.1, the
`Mono` build (every icon and Powerline glyph one cell wide), the four static
faces the agent TUIs need. Copied byte-for-byte from
`patched-fonts/JetBrainsMono/Ligatures/` of the nerd-fonts repository on
2026-09-12:

| File | SHA-256 |
| --- | --- |
| `JetBrainsMonoNerdFontMono-Regular.ttf` | `f2a5ea6cfab397445ffab00c0370927b66d61e560a05db5db271b42006381c1a` |
| `JetBrainsMonoNerdFontMono-Bold.ttf` | `bfcf9a917276ffc058867d87cbc8a5b2f1ab0f4b710e9170dc02763ccb80bd4b` |
| `JetBrainsMonoNerdFontMono-Italic.ttf` | `31efd6ead98746f5b0afa1ee6dba60267ad48db36428360bee327bec10621f97` |
| `JetBrainsMonoNerdFontMono-BoldItalic.ttf` | `9dba502e00e35209f6ed2a151c7376c051657b067cdebbc6e52d06cb9002cf31` |

Licensed under the SIL Open Font License 1.1 (`OFL.txt`, copied from the
JetBrains Mono repository). The glyph sets Nerd Fonts patches in carry their
own licences; the full table is in `THIRD_PARTY_NOTICES.md` at the repository
root.

`sirio_theme`'s test
`bundled_terminal_faces_name_the_family_and_cover_the_agent_glyphs` pins
these files to the family name above and to the Powerline glyphs
(`U+E0B0`, `U+E0B2`, `U+E0B4`, `U+E0B6`) at a one-cell advance. Swap the
files and that test says whether the new ones still qualify.
