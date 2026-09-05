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

## Material Icon Theme (no longer bundled)

**These icons are not distributed with this project any more.** They lived in
the macOS app's asset catalogue, which was removed with the Swift project on
2026-08-20; the current app renders its generic file glyphs from the pinned
Zed set above instead. This section is retained only because the artwork was
distributed in past releases. The icons were from
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

## Codicons (agent marks)

The Claude and OpenAI marks in `rust/assets/icons/codicons/` are from
[Codicons](https://github.com/microsoft/vscode-codicons) by Microsoft
Corporation, obtained via [Iconify](https://iconify.design) (`codicon` set),
and are used under the Creative Commons Attribution 4.0 International
license. Provenance is recorded in `rust/assets/icons/codicons/ATTRIBUTION.md`;
the full license text is copied in `rust/assets/icons/codicons/LICENSE`.

## Simple Icons (agent marks)

The OpenCode and Pi marks in `rust/assets/icons/simple-icons/` are from
[Simple Icons](https://github.com/simple-icons/simple-icons) 16.29.0,
obtained via [Iconify](https://iconify.design) (`simple-icons` set), and are
released under CC0 1.0 Universal (`rust/assets/icons/simple-icons/LICENSE.md`).
Provenance, and the one deliberate `viewBox` change, are recorded in
`rust/assets/icons/simple-icons/ATTRIBUTION.md`.

## Agent marks

The Oh-My-Pi mark in `rust/assets/icons/agent-omp.svg` is a project-owned
SVG translation of the vector coordinates formerly stored in
`App/AgentIcon.swift`, readable at commit `5430d7bf`. The Claude, OpenAI,
OpenCode and Pi marks come from the Codicons and Simple Icons sets above;
until 2026-09-05 Claude, OpenAI and OpenCode were the Zed catalog's
`ai_*.svg` assets and Pi a project-owned monogram, both since removed.

## Trademark notice

The agent marks (Anthropic, OpenAI, OpenCode, Pi, Oh-My-Pi) are third-party
trademarks and are used nominatively in this application solely to identify
the corresponding products, as the Swift application did. Nothing
in this project grants a license to these marks; they remain the property
of their respective owners.
