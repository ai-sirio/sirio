# Icon set — provenance and licence

> **VERIFIED 2026-08-19 — the user did ask for these, in writing, by URL.**
> Found in the user's own typed-prompt history, `~/.claude/history.jsonl`, session
> `6c13680a-d3bb-4d5b-8c40-6c3bc45a5e73`, project `Scrivania/Progetti/tiller`,
> **2026-08-13 23:23:48** — four minutes before `b753ed69` landed the import at 23:27:53:
>
> > *"Ti chiedo inoltre di prendere le icone da https://github.com/zeronsh/comet.git .
> > Inoltre guarda come è fatta [Image #7] . Vorrei la stessa top bar (la barra dove ci
> > sono i 3 semafori)"*
>
> So the inspiration-only rule does not apply here: the user named this repository and asked
> for its icons directly. The exception is real and this directory stays.
>
> **Why three passes failed to find it, which is the part worth keeping.** The message was
> *queued*, not typed at an idle prompt. A queued message never appears in the session
> transcript as a `type: "user"` turn — it is recorded as `type: "queue-operation"` and again
> inside an `attachment` of type `queued_command`. A search filtered to "genuine user-role
> messages" therefore returns **zero matches** on a message the user unambiguously sent, which
> is exactly what the previous version of this banner reported.
>
> `~/.claude/history.jsonl` is the authoritative record of what the user actually typed,
> across every project and session, and it is one flat file. Search it first, and search it
> with the user's own words — the request is in Italian and contains neither `icon` nor
> `svg`; `semafori` is what finds it.

The 63 SVGs in this directory come from **comet** (<https://github.com/zeronsh/comet>),
`crates/ui/assets/icons/`, imported at the user's explicit request — quoted, dated and
located in the banner above.

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
