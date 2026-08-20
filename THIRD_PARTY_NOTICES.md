# Third-Party Notices

## comet (icon set in use today)

The 63 generic surface icons in `rust/assets/icons/comet/` are from
[comet](https://github.com/zeronsh/comet), **MIT, Copyright (c) 2026 Wing**.
Full provenance, including why this set replaced the vendored Phosphor *thin*
icons rather than joining them, is in `rust/assets/icons/comet/ATTRIBUTION.md`.

> The MIT License (MIT)
>
> Copyright (c) 2026 Wing
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in
> all copies or substantial portions of the Software.

## Material Icon Theme (no longer bundled)

**These icons are not distributed with this project any more.** They lived in
the macOS app's asset catalogue, which was removed with the Swift project on
2026-08-20; the Linux app renders file-type icons from the comet set above
instead. This section is retained only because the artwork was distributed in
past releases. The icons were from
[Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme)
by Material Extensions, obtained via [Iconify](https://iconify.design)
(`material-icon-theme` set), and are used under the MIT License:

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

## Phosphor Icons

The surface icons in `rust/assets/icons/` are from
[Phosphor Icons](https://phosphoricons.com), obtained through
[Iconify](https://iconify.design), and are used under the MIT License:

> MIT License
> Copyright (c) 2020 Phosphor Icons
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

## Simple Icons

The Claude Code and OpenCode marks in `rust/assets/icons/agent-*.svg` are from
[Simple Icons](https://simpleicons.org), obtained through Iconify, and are
dedicated to the public domain under CC0 1.0.

The Codex/OpenAI mark was copied from the macOS app's template asset at
`App/Assets.xcassets/agent-codex.imageset/openai.svg`. The Pi and Oh-My-Pi
marks are project-owned SVG translations of the vector coordinates in
`App/AgentIcon.swift`. Both of those paths were removed with the Swift project
and are readable at commit `5430d7bf` (`git show 5430d7bf:App/AgentIcon.swift`).

## SVG Logos

The Anthropic and OpenAI marks in `rust/assets/icons/agent-claude.svg` and
`rust/assets/icons/agent-codex.svg` are from the [SVG Logos
collection](https://github.com/gilbarbara/logos) ("logos" set), obtained via
Iconify, and are dedicated to the public domain under CC0 1.0:

> CC0 1.0 Universal — to the extent possible under law, the authors have
> waived all copyright and related or neighboring rights to this work.

Each file is cropped from the collection's logo-plus-wordmark artwork: the
Claude file keeps only the sunburst glyph (the wordmark paths are dropped),
and the Codex file keeps only the knot glyph.

The Oh-My-Pi mark's three-stop gradient (#ED4ABF → #9B4DFF → #5AD8E6) and
the Pi and Oh-My-Pi vector shapes are project-owned translations of
`App/AgentIcon.swift`, readable at commit `5430d7bf`.

## Trademark notice

The agent marks (Anthropic, OpenAI, OpenCode, Pi, Oh-My-Pi) are third-party
trademarks and are used nominatively in this application solely to identify
the corresponding products, as the Swift application did. Nothing
in this project grants a license to these marks; they remain the property
of their respective owners.
