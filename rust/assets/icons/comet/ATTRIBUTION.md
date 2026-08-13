# Icon set — provenance and licence

The 63 SVGs in this directory come from **comet** (<https://github.com/zeronsh/comet>),
`crates/ui/assets/icons/`, imported at the user's explicit request.

**Licence: MIT — Copyright (c) 2026 Wing.** The MIT licence permits use, modification and
redistribution provided the copyright notice and permission notice accompany the work. That is what
this file is for; keep it next to the assets.

## What was imported, and what was not

**Assets only.** No comet source code was copied — the standing rule for this rewrite is that every
line of Rust is written from scratch and transplanted code counts as a gap. Icons are artwork, and
the user asked for them by name.

## Shape of the files

All 63 share one format, which is why they drop into Tiller's existing pipeline unchanged:

- `viewBox="0 0 16 16"`
- `stroke="currentColor"` with `stroke-width="1.25"`, `stroke-linecap="round"` for the line icons
- `fill="currentColor"` for the solid brand marks (`claude-mark`, `openai-mark`, `pi-mark`,
  `cursor-mark`, `grok-mark`, `hermes-mark`)

`currentColor` throughout means every one of them tints through `paint_tinted_svg`
(`tiller_ui/src/icons.rs`) with no edit.

## This is a replacement, not an addition

Tiller's original 22 icons are **Phosphor, "thin" weight** — `caret-down-thin`, `chat-circle-thin`,
`terminal-window-thin`, a hairline aesthetic. These are **Solar** — `alt-arrow-down`, `magnifer`,
`settings-minimalistic`, rounded and slightly heavier.

The two sets do not mix. A titlebar drawn with Solar arrows beside a sidebar drawn with Phosphor
carets reads as an unfinished port, not as a design. Migrating means mapping **every** existing
`Icon::` variant, not adding variants alongside the old ones.
