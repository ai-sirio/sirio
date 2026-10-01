# Bundled fonts

## Geist

The five UI faces registered by `sirio_theme::register_ui_fonts`: `Geist.ttf`,
`Geist-Medium.ttf`, `Geist-SemiBold.ttf`, `Geist-Bold.ttf`, and `GeistMono.ttf`.
Copied byte-for-byte from `bezel-ui` 0.1.4's `assets/fonts`; upstream is
[vercel/geist-font](https://github.com/vercel/geist-font). Licensed under the
SIL Open Font License 1.1 in `Geist-OFL.txt`.

| File | SHA-256 |
| --- | --- |
| `Geist.ttf` | `73894e0448cae90a92b6c2f8732b7bb9acb7b94c418bff559dad4a18e1de9659` |
| `Geist-Medium.ttf` | `0090e004725f6f64b841715b4167920580f883fcf9b67fc6d744089103fec101` |
| `Geist-SemiBold.ttf` | `612ec98df33935354f39e81e54101656961ab6e5549f64b63eb57868ba7bab8d` |
| `Geist-Bold.ttf` | `e866b423b755233cae8bce6a37519f6fe630be9772fa08fc3114bff15bc8580f` |
| `GeistMono.ttf` | `87c2aff9723544a9adaea19d92e42a33705c9723624801b6e0224c2206a6af0d` |

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
