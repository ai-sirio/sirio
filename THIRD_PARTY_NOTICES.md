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

## Material Icon Theme (file type icons)

The 55 polychrome file-type glyphs in `rust/assets/icons/file-types/` are
from [Material Icon Theme](https://github.com/PKief/vscode-material-icon-theme)
by Material Extensions, used unchanged under the MIT License. Provenance is
recorded in `rust/assets/icons/file-types/SOURCE.md`: they reached this repo
through the retired Swift app's `FileIcons` imagesets at commit `5430d7bf`,
and were restored to the Rust app on 2026-08-21.

This section said the opposite between 2026-08-20 and 2026-09-18 — that the
artwork had left with the Swift project — which stopped being true the day
the icons came back. They are bundled, so the notice below travels with
them:

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

## Trademark notice

The agent marks (Anthropic, OpenAI, Google, xAI, OpenCode, Pi, Oh-My-Pi)
are third-party trademarks and are used nominatively in this application
solely to identify the corresponding products, as the Swift application
did. Nothing in this project grants a license to these marks — the MIT
license on the LobeHub geometry above covers the drawings, not the brands
they depict, and no open-source license conveys trademark rights. They
remain the property of their respective owners.
