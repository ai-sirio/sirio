# Bundled fonts

## Geist

Geist's five faces, registered first by `sirio_theme::register_ui_fonts`: `Geist.ttf`,
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

## Font pairs

The six pairs `sirio_theme::FontPair` bundles besides Geist, one folder each under
`rust/assets/fonts/<pair-id>/`, registered by `sirio_theme::register_ui_fonts`
(one `add_fonts` call per pair). Each file is copied byte-for-byte from the
source in `THIRD_PARTY_NOTICES.md`, under its upstream name, with its licence
text beside it. All are static TTF. Red Hat Text's SemiBold is weight 632, its
Medium (565) and its Italic are not bundled (spec 2026-11-21-font-setting §2).
`sirio_theme`'s test `bundled_faces_name_their_pair_family_weight_and_style` pins
every file to its pair's family, weight and italic style.

### Atkinson Hyperlegible Next and Mono (`atkinson/`)

| File | SHA-256 |
| --- | --- |
| `AtkinsonHyperlegibleNext-Regular.ttf` | `88ed5c31a71584c7772963b02d04bef1eb7e3d2e9c8b9cb204339b1f82cf432c` |
| `AtkinsonHyperlegibleNext-Medium.ttf` | `dd50b08b3c560846097d23baaaf6a97ffa20dd077115d23c59df68083b9ea05e` |
| `AtkinsonHyperlegibleNext-SemiBold.ttf` | `8e2fff779f35ac25239656bb0f3b2bd35959a2009e3dd21a12141c3b9030362b` |
| `AtkinsonHyperlegibleNext-Bold.ttf` | `994414047df66bb4998d01c1cb1eeb4a2ddc4622d1aa56bbb8adbeca7645b041` |
| `AtkinsonHyperlegibleNext-Italic.ttf` | `b7f8f03ceb28ebadb2de2332b2e869a00e8dbfb5c8e00fdbb06385770c8227e7` |
| `AtkinsonHyperlegibleMono-Regular.ttf` | `2c9a7b9fabebd5c746a808d8a1ef2f52e618850caad6ec4fd9f5c48f56e1c1af` |
| `AtkinsonHyperlegibleMono-Bold.ttf` | `51ad68fafd7ec68051f64983c9c23182a7084268b5cdb0a154dbd53ade079da6` |

### Inter and JetBrains Mono (`inter/`)

| File | SHA-256 |
| --- | --- |
| `Inter-Regular.ttf` | `40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82` |
| `Inter-Medium.ttf` | `97ad806f526e41546d46365bb3a393145f75b7b1568913db74549ad8b8dba872` |
| `Inter-SemiBold.ttf` | `78a843fade9d4612a5567302fb595b56976eb5fcebf4fea5a5912d638bafcde3` |
| `Inter-Bold.ttf` | `288316099b1e0a47a4716d159098005eef7c0066921f34e3200393dbdb01947f` |
| `Inter-Italic.ttf` | `bbc051dd204b5019a1aa0bc0ae2aa8a05ab13e7a3f979fa357631dc7feb6833a` |
| `JetBrainsMono-Regular.ttf` | `a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f` |
| `JetBrainsMono-Bold.ttf` | `5590990c82e097397517f275f430af4546e1c45cff408bde4255dad142479dcb` |

### IBM Plex Sans and Mono (`plex/`)

| File | SHA-256 |
| --- | --- |
| `IBMPlexSans-Regular.ttf` | `b47ebd72b5ce53b60f909033ff84d6fc12358cc147e2981f8c321ad3f36246b6` |
| `IBMPlexSans-Medium.ttf` | `a494d5a69fa094f7071a9ffbc98e86c85337d1a29f3fc1ff7c2e1c46161ddf27` |
| `IBMPlexSans-SemiBold.ttf` | `c51119b25f4a296da2b66edf3201b9aa1735091ed7efc1fca0313346f3851a4d` |
| `IBMPlexSans-Bold.ttf` | `22f59d358e8b9b9fb38ac6ee26a7ea7263ffa1c89a9056c101acff5add91f5d6` |
| `IBMPlexSans-Italic.ttf` | `fbcfd6c96fb6188d3d65498e6f8e9719c2a3c0c2932e09255b01bd6d7c78ca12` |
| `IBMPlexMono-Regular.ttf` | `7c6fbddca4b700be918f5f6183d9bd4464fa427fe435f0b480d77fe2bb8c5a43` |
| `IBMPlexMono-Bold.ttf` | `74e5eedcfa4596497d34e19023cabdabd3a8c852b903007a5654a59591a72ffb` |

### Red Hat Text and Mono (`redhat/`)

| File | SHA-256 |
| --- | --- |
| `RedHatText-Regular.ttf` | `c0989952929fb6427d7b16999678418d52977aea764e66e51b76f9ca221b71b7` |
| `RedHatText-Bold.ttf` | `a44ef4b6cfefb2e90d489247cdf3e269744673ebb68b48c89ce9e37041feccb4` |
| `RedHatMono-Regular.ttf` | `5b884e3130d8785d46c25688a5cc746613e4ce51dfdcfe9ef1e1e9236342570a` |
| `RedHatMono-Bold.ttf` | `c53a1dc7f143d698070c85e5893cd058ce551af5fa4002df847eec29aeb26d56` |
| `RedHatText-SemiBold.ttf` | `d09c9bff0bfde29ff620059b6c0a77275d45f5932d4f609bceef399903751d93` |

### Recursive Sans and Mono (Linear Static) (`recursive/`)

| File | SHA-256 |
| --- | --- |
| `RecursiveSansLnrSt-Regular.ttf` | `ca6aeb615fe2d1c2a97b101ee32c9cad19ffbdfaf3011f0935d6b210d365fb4e` |
| `RecursiveSansLnrSt-Med.ttf` | `8e346dcabc066b68473487a085fd2b9d1c1c00a2a503ee5a1e876eca916fe1ca` |
| `RecursiveSansLnrSt-SemiBold.ttf` | `afbbb94450ca02f92d38faa6d7f24ad12bc7bf1b689c9df9718d37e1794708f9` |
| `RecursiveSansLnrSt-Bold.ttf` | `a8d93b2c5082171c3a841744b5cb4dd427483e9a104911206c5f05fb0d0dc99b` |
| `RecursiveSansLnrSt-Italic.ttf` | `62e4d508318ec18f689c3c528b890577f33b49f65249781fd8bb6be22e424744` |
| `RecursiveMonoLnrSt-Regular.ttf` | `c0f71b949136946bd0b103c79585be4e4005d19fb935e98c6183e5cb44a4f64b` |
| `RecursiveMonoLnrSt-Bold.ttf` | `da1e3c4913fffd14124506827ec0790161056d86e45fe25f5deb211056349f54` |

### Figtree and Fira Code (`figtree/`)

| File | SHA-256 |
| --- | --- |
| `Figtree-Regular.ttf` | `e6f1cae136bc7ea35fec04ffce4617d5e9137ac8844d6a5df619bcd5368ea4ee` |
| `Figtree-Medium.ttf` | `e605259d449b9f0e34dd935abb3d75d4f86759e5b947af6d11d9a7db28c8b3fd` |
| `Figtree-SemiBold.ttf` | `a63306f13cbf3864b092672073864b71df0bfd0cfc2e873c9e3e3a6b075eb574` |
| `Figtree-Bold.ttf` | `2a33fa55aa9411948d2ae776ffa9eaf97d72b8847b34857e7d59d6c8e26c563e` |
| `Figtree-Italic.ttf` | `5983a0320ffe6776f3a7b848a3b8eac72706e23121977787490b80ef06599439` |
| `FiraCode-Regular.ttf` | `5992ab9640e2df491b2f609467b1de60e8bc39b2c28db184342a0592d98f6117` |
| `FiraCode-Bold.ttf` | `41f6554e845e2f5b70adad3950122334b866aac436793b7742ade600067701be` |
