# Simple Icons attribution

The SVG files in this directory are from
[Simple Icons](https://github.com/simple-icons/simple-icons) 16.29.0
(2026-08-29), fetched from the Iconify API on 2026-09-05:

- `opencode.svg` — https://api.iconify.design/simple-icons/opencode.svg
  (Simple Icons source: `anomalyco/opencode`, `packages/identity/mark.svg`
  at `1251a870cb384543c150c4a72fb101b55eec971b`)
- `pi.svg` — https://api.iconify.design/simple-icons/pi.svg
  (Simple Icons source: https://pi.dev/favicon.svg)

One deliberate deviation from the served bytes: the `viewBox` of both files
is `-2 -2 28 28` instead of Iconify's `0 0 24 24`. Simple Icons fill their
whole 24-unit box while Codicons draw inside a padded canvas, so the 2-unit
margin keeps OpenCode and Pi optically the same size as the Claude and OpenAI
marks in `../codicons/`. The path geometry itself is untouched.

Simple Icons are released under CC0 1.0 (`LICENSE.md` alongside this file),
so no attribution is legally required; this file records provenance.

Agent logos are used nominatively to identify their respective products and
remain trademarks of their owners.
