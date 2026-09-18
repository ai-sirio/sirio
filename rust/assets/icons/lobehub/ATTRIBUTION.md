# LobeHub icons attribution

The SVG files in this directory are LobeHub's
[lobe-icons](https://github.com/lobehub/lobe-icons), npm package
`@lobehub/icons-static-svg` version 1.95.0 — the latest release on
2026-09-18 — fetched byte-for-byte from unpkg on 2026-09-18:

- `claude-color.svg` — https://unpkg.com/@lobehub/icons-static-svg@1.95.0/icons/claude-color.svg
- `codex.svg` — https://unpkg.com/@lobehub/icons-static-svg@1.95.0/icons/codex.svg
- `gemini-color.svg` — https://unpkg.com/@lobehub/icons-static-svg@1.95.0/icons/gemini-color.svg
- `grok.svg` — https://unpkg.com/@lobehub/icons-static-svg@1.95.0/icons/grok.svg
- `opencode.svg` — https://unpkg.com/@lobehub/icons-static-svg@1.95.0/icons/opencode.svg
- `pi.svg` — https://unpkg.com/@lobehub/icons-static-svg@1.95.0/icons/pi.svg

One set, not several, is the whole point: lobe-icons is the only collection
carrying every mark Sirio needs already on the same grid — `viewBox="0 0 24
24"`, `width="1em" height="1em"` — so the marks keep one optical weight
without per-file surgery. Mixing sets is what made the previous vendoring
(Codicons for Claude and OpenAI, Simple Icons for OpenCode and Pi) invent a
`-2 -2 28 28` viewBox for two of its four marks; that deviation is gone, and
this directory carries none.

## Which file, and why not its neighbour

- `codex.svg`, **not** `codex-color.svg`: the colour variant paints a white
  plate behind a gradient mark — illegible in dark mode and muddy at 15px.
- `claude-color.svg`, **not** `claudecode-color.svg`: both are LobeHub's and
  both are `#D97757`, but the starburst is the shape Sirio has always shown
  for this agent, so the swap changes the source without changing the
  picture. The alternative is Claude Code's own pixel mark.
- No Oh My Pi mark exists in any library, so omp keeps Sirio's own
  `../agent-omp.svg` and its baked gradient.
- Gemini and Grok have no adapter behind them in `sirio_agents::ALL`.
  They are vendored because `Icon::for_agent_id` maps their ids: an agent
  published under `gemini` or `grok` in the registry draws its real mark
  instead of falling back to the generic sparkle.

## Three things that look like they want cleaning up, and must not be

The files are upstream's bytes, unedited:

- `width="1em" height="1em"` and `style="flex:none;line-height:1"` are
  LobeHub's React sizing convention. GPUI sizes the element itself and usvg
  ignores the CSS, so both are inert here.
- `<title>` is inert too: nothing in this app reads an SVG's title.
- gemini's gradient ids carry a React-generated suffix
  (`lobe-icons-gemini-0-_R_0_`). In a browser, two copies of one icon in a
  single DOM would collide on those ids and the second would take the
  first's gradient. Sirio rasterizes each file as its own resvg document,
  where ids are scoped per icon, so the collision cannot happen and the
  rename upstream consumers need is not needed here.

`claude-color.svg` is the one file with a colour baked in (`fill="#D97757"`).
It still rides the tinted path rather than the full-colour raster, because
that hex *is* `AgentBrandColor::Claude`: same pixels, one less cache entry.
See `rust/crates/sirio_ui/src/icons.rs`.

The icon geometry must remain unchanged; update the version, the fetch date
and this provenance together when refreshing these files. Should a mark ever
need an optical-size adjustment, change its `viewBox` and record the
deviation here — never the path data.

lobe-icons is distributed under the MIT License (`LICENSE` alongside this
file), Copyright (c) 2023 LobeHub. That licence covers the geometry only.

Agent logos are used nominatively to identify their respective products and
remain trademarks of their owners.
