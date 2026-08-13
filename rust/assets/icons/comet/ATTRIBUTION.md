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

An earlier version of this file claimed all 63 share one format. **They do not** — measured across
the directory, there are three, and the difference is optically visible once they are scaled into a
common 16px box:

| group | count | viewBox | stroke |
|---|---|---|---|
| line icons, house style | 52 | `0 0 24 24` | `1.5` |
| line icons, small grid | 4 | `0 0 16 16` | `1.25` |
| marks and solids | 7 | arbitrary — see below | fill, no stroke |

The 16×16 four are `check`, `close`, `plus`, `terminal` (`grok-mark` is 16×16 but a solid). A stroke
of `1.5` on a 24-unit grid renders at `1.5 × 16/24 = 1.0px`; a stroke of `1.25` on a 16-unit grid
renders at `1.25px`. **The small-grid icons therefore come out about 25% heavier than the other 52.**
Setting their `stroke-width` to `1.0` matches the majority without touching a path.

Three of the marks are **not square** — `claude-mark` 256×257, `openai-mark` 256×260,
`cursor-mark` 466.73×532.09, `comet-logo` 820×940 — so how they sit in a square icon box depends on
aspect handling, and should be checked visually rather than assumed. `pi-mark` (800×800) and `stop`
(10×10) are square.

Do not infer the family from the paint: `key-minimalistic` is fill-only despite being an ordinary
24×24 icon.

`currentColor` is the one thing that genuinely is universal here, so every file tints through
`paint_tinted_svg` (`tiller_ui/src/icons.rs`) without an edit.

Modifying the artwork is permitted — the MIT licence above covers modification, provided this notice
stays with it.

## This is a replacement, not an addition

Tiller's original 22 icons are **Phosphor, "thin" weight** — `caret-down-thin`, `chat-circle-thin`,
`terminal-window-thin`, a hairline aesthetic. These are **Solar** — `alt-arrow-down`, `magnifer`,
`settings-minimalistic`, rounded and slightly heavier.

The two sets do not mix. A titlebar drawn with Solar arrows beside a sidebar drawn with Phosphor
carets reads as an unfinished port, not as a design. Migrating means mapping **every** existing
`Icon::` variant, not adding variants alongside the old ones.
