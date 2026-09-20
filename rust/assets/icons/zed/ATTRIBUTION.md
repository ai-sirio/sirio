# Zed icon attribution

The SVG files in this directory were copied byte-for-byte from
`zed-industries/zed` commit
`875e2a1c458ffcf2bfe90be841eb9893f399b4ff`:

https://github.com/zed-industries/zed/tree/875e2a1c458ffcf2bfe90be841eb9893f399b4ff/assets/icons

The accompanying `LICENSES` file is the license notice distributed by Zed
for this icon directory. The icon geometry must remain unchanged; update the
commit and provenance together when refreshing these files.

The agent marks (`ai_claude.svg`, `ai_open_ai.svg`, `ai_open_code.svg`) that
used to live here were replaced on 2026-09-05, and since 2026-09-18 every
mark comes from one set, `../lobehub/`; this directory now holds generic UI
glyphs only.

`folder_open.svg` was added on 2026-09-18, after the initial vendoring, from
that same commit and by the same byte-for-byte rule: the file tree pairs it
with `folder.svg` to show whether a directory is expanded, in place of a
disclosure arrow. Take any further icon from the commit named above rather
than from elsewhere, so this directory stays one provenance.

`eye.svg` and `code.svg` were added on 2026-09-20 under the same rule, from
that same commit: the file editor's Markdown row names its two modes with
them once Preview and Code stopped being text labels. `code.svg` carries
width and height but no `viewBox`, exactly as upstream serves it — the same
shape `public.svg`, `rotate_cw.svg` and `sparkle.svg` already have here.
