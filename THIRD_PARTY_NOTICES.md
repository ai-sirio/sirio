# Third-Party Notices

## Zed icons

The UI icons in `rust/assets/icons/zed/` were copied byte-for-byte from
[`zed-industries/zed`](https://github.com/zed-industries/zed) commit
`875e2a1c458ffcf2bfe90be841eb9893f399b4ff`. Full file-level provenance is
recorded in `rust/assets/icons/zed/ATTRIBUTION.md`; the upstream icon license
is copied in `rust/assets/icons/zed/LICENSES`.

> Lucide License
>
> ISC License
>
> Copyright (c) for portions of Lucide are held by Cole Bemis 2013-2022 as
> part of Feather (MIT). All other copyright (c) for Lucide are held by
> Lucide Contributors 2022.
>
> Permission to use, copy, modify, and/or distribute this software for any
> purpose with or without fee is hereby granted, provided that the above
> copyright notice and this permission notice appear in all copies.
>
> THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
> WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
> MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
> SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
> WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
> OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
> CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

## Lucide icons

The two panel icons in `rust/assets/icons/lucide/` (`panel-left-close.svg`,
`panel-left-open.svg`) were copied byte-for-byte from
[`lucide-icons/lucide`](https://github.com/lucide-icons/lucide) release tag
`1.66.0`. Per-file provenance is recorded in
`rust/assets/icons/lucide/ATTRIBUTION.md`, and the license file of that tag is
copied in `rust/assets/icons/lucide/LICENSE`. It is the ISC license quoted
under Zed icons above, with this tag's copyright line:

> Copyright (c) 2026 Lucide Icons and Contributors

## Material Icon Theme (file and folder icons)

The 1112 polychrome file and folder icons in `rust/crates/sirio_icons/assets/`
are the artwork of [Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme)
by Material Extensions (formerly PKief/vscode-material-icon-theme), release
v5.29.0, used unchanged under the MIT License. They were vendored on
2026-09-22 from [`zed-extensions/material-icon-theme`](https://github.com/zed-extensions/material-icon-theme)
commit `5ec848638409e4578d9e8c8478041fcab1df15f8` by
`Scripts/vendor-material-icons.py`; provenance is recorded in
`rust/crates/sirio_icons/UPSTREAM.md` and `ATTRIBUTION.md`, and the notice
below is copied in `rust/crates/sirio_icons/LICENSE-MIT`.

They replace a 55-glyph subset of the same artwork that lived in
`rust/assets/icons/file-types/` from 2026-08-21, when it came back from the
retired Swift app's `FileIcons` imagesets (commit `5430d7bf`), until 2026-09-22.
Between 2026-08-20 and 2026-09-18 this section said the artwork had left with
the Swift project, which stopped being true the day the icons came back.

The table that maps a file or directory name to an icon,
`rust/crates/sirio_icons/src/generated.rs`, is generated from that
repository's `icon_themes/material-icon-theme.json`. The mapping is the Zed
extension's own work and is licensed separately, under the Apache License 2.0;
its full text is `rust/crates/sirio_icons/LICENSE-APACHE`.

The artwork is bundled, so its notice travels with it:

> The MIT License (MIT)
> Copyright (c) 2025 Material Extensions
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

## LobeHub lobe-icons (agent marks)

The agent brand marks in `rust/assets/icons/lobehub/` — Claude, Codex,
Gemini, Grok, OpenCode and Pi — are from LobeHub's
[lobe-icons](https://github.com/lobehub/lobe-icons), npm package
`@lobehub/icons-static-svg` 1.95.0, fetched byte-for-byte from unpkg on
2026-09-18. Per-file provenance is recorded in
`rust/assets/icons/lobehub/ATTRIBUTION.md`; the license is copied in
`rust/assets/icons/lobehub/LICENSE` and reproduced here:

> MIT License
>
> Copyright (c) 2023 LobeHub
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

That license covers the geometry only; see the trademark notice below.

## Codicons and Simple Icons (no longer bundled)

**These icons are not distributed with this project any more.** From
2026-09-05 to 2026-09-18 the Claude and OpenAI marks came from
[Codicons](https://github.com/microsoft/vscode-codicons) by Microsoft
Corporation (CC BY 4.0) and the OpenCode and Pi marks from
[Simple Icons](https://github.com/simple-icons/simple-icons) 16.29.0
(CC0 1.0 Universal), both obtained via [Iconify](https://iconify.design).
They were replaced by the single LobeHub set above, which carries every
mark on one grid. This section is retained only because the artwork was
distributed in past releases.

## Agent marks

The Oh-My-Pi mark in `rust/assets/icons/agent-omp.svg` is a project-owned
SVG translation of the vector coordinates formerly stored in
`App/AgentIcon.swift`, readable at commit `5430d7bf`; no icon library
carries an Oh My Pi mark. Every other mark comes from the LobeHub set
above. Before it, from 2026-09-05, they came from Codicons and Simple
Icons; before that Claude, OpenAI and OpenCode were the Zed catalog's
`ai_*.svg` assets and Pi a project-owned monogram, all since removed.

## Geist (UI fonts)

The five UI faces in `rust/assets/fonts/` — `Geist.ttf`,
`Geist-Medium.ttf`, `Geist-SemiBold.ttf`, `Geist-Bold.ttf` and `GeistMono.ttf`
— were copied byte-for-byte from `bezel-ui` 0.1.4's `assets/fonts`. Geist is
from [vercel/geist-font](https://github.com/vercel/geist-font) and is licensed
under the SIL Open Font License 1.1. The license is copied to
`rust/assets/fonts/Geist-OFL.txt`; SHA-256 sums for each face are in
`rust/assets/fonts/README.md`.

## JetBrainsMono Nerd Font Mono (terminal font)

The terminal face in `rust/assets/fonts/` is
[JetBrains Mono](https://github.com/JetBrains/JetBrainsMono) 2.304 as patched
by [Nerd Fonts](https://github.com/ryanoasis/nerd-fonts) v3.5.1 — the four
static `JetBrainsMonoNerdFontMono-{Regular,Bold,Italic,BoldItalic}.ttf`
faces, copied byte-for-byte from the nerd-fonts repository's
`patched-fonts/JetBrainsMono/Ligatures/` on 2026-09-12; SHA-256 sums are in
`rust/assets/fonts/README.md`. JetBrains Mono is licensed under the SIL Open
Font License 1.1, copied in `rust/assets/fonts/OFL.txt`; the Nerd Fonts
patcher itself is MIT. The glyph sets the patch adds carry their own
licences, as listed by the nerd-fonts project for this font:

| Glyph set | Source | Licence |
| --- | --- | --- |
| Codicons | https://github.com/microsoft/vscode-codicons | CC BY 4.0 |
| Devicons | https://github.com/devicons/devicon | MIT |
| extraglyphs | https://github.com/source-foundry/Hack | MIT |
| Font Awesome | https://github.com/FortAwesome/Font-Awesome | CC BY 4.0 |
| Font Awesome Extension | https://github.com/AndreLZGava/font-awesome-extension | MIT |
| Font Logos | https://github.com/lukas-w/font-logos | unlicensed |
| Material Design | https://github.com/Templarian/MaterialDesign-Font | Apache 2.0 |
| Octicons | https://github.com/primer/octicons | MIT |
| Seti and original | https://github.com/jesseweed/seti-ui | MIT |
| Pomicons | https://github.com/gabrielelana/pomicons | OFL 1.1 RFN |
| Powerline Extra | https://github.com/ryanoasis/powerline-extra-symbols | MIT |
| Powerline Symbols | https://github.com/powerline/powerline | MIT |
| Power Symbols IEC | https://github.com/jloughry/Unicode | MIT |
| Weather Icons | https://github.com/erikflowers/weather-icons | OFL 1.1 |

## bezel-ui and bezel-motion (ported code)

Code ported into Sirio from [`bezel-ui`](https://crates.io/crates/bezel-ui) and
[`bezel-motion`](https://crates.io/crates/bezel-motion) 0.1.4 carries a header
naming the crate it came from. In `rust/crates/sirio_ui/src/` that is:

- `motion/`: the frame clock, the hover fades, the cubic-bezier curves and the
  loaders' phase maths;
- `loading.rs`: the orbs, the mini spinner and the redacted skeleton rows;
- `popup.rs`: the popup and filter state and the menu's close lifecycle;
- `scroll.rs`: the scrollbar's thumb maths, its drag payload and the follow-the-tail state;
- `chat/thought.rs`: `Takeover`;
- `chat/transcript.rs`: `tracked_upper`, the day heading's letter tracking;
- `combobox.rs`: the type-to-filter select, and the closed face of a select (`select_trigger`);
- `sidebar/tree_keys.rs`: the tree's arrow-key bindings and cursor step;
- `settings/scaffold.rs`: the settings page skeleton (headline, section card, rows, meta line) and its
  empty state and status dot.

The vendored Ely tree (`rust/vendor/ely/crates/ely-core/`) holds ported `bezel-ui` code too, each piece
marked by a comment naming bezel-ui 0.1.4's `src/input.rs`:

- `crates/ely-core/src/forms/text/actions.rs`: the `DeleteWordRight`, `DeleteToLineStart` and `DeleteToLineEnd` handlers;
- `crates/ely-core/src/forms/text/edit.rs`: `is_word`, the word unit that word motion and deletion share.

Both crates are by clearloop ([crabtalk/bezel](https://github.com/crabtalk/bezel))
and are licensed under the MIT licence below, which is the `LICENSE` of the repository's `v0.1.4` tag
(the published crates do not ship it). The second
copyright line is bezel-ui's own, for the portions it extracted from
zeronsh/comet; `bezel-motion`'s manifest names the same MIT licence and the same
author.

> MIT License
>
> Copyright (c) 2026 clearloop
> Copyright (c) 2026 Wing (portions extracted from zeronsh/comet)
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

## Ely GPUI Components (vendored crates)

`rust/vendor/ely/crates/` holds the eight crates of
[Ely GPUI Components](https://github.com/ELY-Open-Source/Ely-GPUI-Components) at rev `8d7723ec`
(`ely-core`, `ely-chat`, `ely-agent`, `ely-editor`, `ely-git`, `ely-documents`, `ely-collab`,
`ely-charts`), plus the colour tables of its theme in `rust/vendor/ely-palette`. They are licensed
under the MIT licence below, which is the `LICENSE` at `rust/vendor/ely/LICENSE`. Sirio's changes to
them are listed in `rust/vendor/ely/LOCAL-CHANGES.md`.

> MIT License
>
> Copyright (c) 2026 Ely GPUI Component contributors
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

## IBM Plex Sans and the KaTeX fonts (embedded by Ely's crates)

Two font sets are compiled into Ely's crates as `include_bytes!` data, and the release binary may
carry them (whether the linker keeps them depends on code paths Sirio does not control):

- IBM Plex Sans Regular, in `ely-core` (`crates/ely-core/assets/plex/`): Copyright (c) 2017 IBM Corp.
  with Reserved Font Name "Plex". Licensed under the SIL Open Font License 1.1 below; the in-tree copy
  is `rust/vendor/ely/crates/ely-core/assets/plex/IBMPlexSans-LICENSE.txt`.
- The KaTeX fonts (`KaTeX_*.ttf`), from the `ratex-katex-fonts` crate, which `ely-documents` enables
  through `ratex-svg`'s `embed-fonts` feature. They come from the [KaTeX](https://github.com/KaTeX/KaTeX)
  project and are licensed under the SIL Open Font License 1.1, whose text the crate ships in
  `fonts/OFL.txt` (the same text as below, with no copyright line of its own). The `ratex-*` crates
  themselves are MIT.

Inter and JetBrains Mono, which `ely-core` embeds behind its `fonts` feature, are not in a release
build: Sirio leaves the feature off, and only `ely-git`'s Linux dev-dependency turns it on for test builds.

> SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
> -----------------------------------------------------------
>
> PREAMBLE
> The goals of the Open Font License (OFL) are to stimulate worldwide
> development of collaborative font projects, to support the font creation
> efforts of academic and linguistic communities, and to provide a free and
> open framework in which fonts may be shared and improved in partnership
> with others.
>
> The OFL allows the licensed fonts to be used, studied, modified and
> redistributed freely as long as they are not sold by themselves. The
> fonts, including any derivative works, can be bundled, embedded,
> redistributed and/or sold with any software provided that any reserved
> names are not used by derivative works. The fonts and derivatives,
> however, cannot be released under any other type of license. The
> requirement for fonts to remain under this license does not apply
> to any document created using the fonts or their derivatives.
>
> DEFINITIONS
> "Font Software" refers to the set of files released by the Copyright
> Holder(s) under this license and clearly marked as such. This may
> include source files, build scripts and documentation.
>
> "Reserved Font Name" refers to any names specified as such after the
> copyright statement(s).
>
> "Original Version" refers to the collection of Font Software components as
> distributed by the Copyright Holder(s).
>
> "Modified Version" refers to any derivative made by adding to, deleting,
> or substituting -- in part or in whole -- any of the components of the
> Original Version, by changing formats or by porting the Font Software to a
> new environment.
>
> "Author" refers to any designer, engineer, programmer, technical
> writer or other person who contributed to the Font Software.
>
> PERMISSION & CONDITIONS
> Permission is hereby granted, free of charge, to any person obtaining
> a copy of the Font Software, to use, study, copy, merge, embed, modify,
> redistribute, and sell modified and unmodified copies of the Font
> Software, subject to the following conditions:
>
> 1) Neither the Font Software nor any of its individual components,
> in Original or Modified Versions, may be sold by itself.
>
> 2) Original or Modified Versions of the Font Software may be bundled,
> redistributed and/or sold with any software, provided that each copy
> contains the above copyright notice and this license. These can be
> included either as stand-alone text files, human-readable headers or
> in the appropriate machine-readable metadata fields within text or
> binary files as long as those fields can be easily viewed by the user.
>
> 3) No Modified Version of the Font Software may use the Reserved Font
> Name(s) unless explicit written permission is granted by the corresponding
> Copyright Holder. This restriction only applies to the primary font name as
> presented to the users.
>
> 4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
> Software shall not be used to promote, endorse or advertise any
> Modified Version, except to acknowledge the contribution(s) of the
> Copyright Holder(s) and the Author(s) or with their explicit written
> permission.
>
> 5) The Font Software, modified or unmodified, in part or in whole,
> must be distributed entirely under this license, and must not be
> distributed under any other license. The requirement for fonts to
> remain under this license does not apply to any document created
> using the Font Software.
>
> TERMINATION
> This license becomes null and void if any of the above conditions are
> not met.
>
> DISCLAIMER
> THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
> EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
> MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
> OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
> COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
> INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
> DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
> FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
> OTHER DEALINGS IN THE FONT SOFTWARE.

## Trademark notice

The agent marks (Anthropic, OpenAI, Google, xAI, OpenCode, Pi, Oh-My-Pi)
are third-party trademarks and are used nominatively in this application
solely to identify the corresponding products, as the Swift application
did. Nothing in this project grants a license to these marks — the MIT
license on the LobeHub geometry above covers the drawings, not the brands
they depict, and no open-source license conveys trademark rights. They
remain the property of their respective owners.
