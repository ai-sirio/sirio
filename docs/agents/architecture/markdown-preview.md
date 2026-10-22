# Markdown Preview: HTML, Mermaid, PlantUML (`sirio_markdown::expand_html`, `sirio_diagram`)

The file view's Preview has no HTML engine. `expand_html` maps a GitHub-style
subset of raw HTML onto the existing `Block`/`Inline` model and reduces the rest
to text; `sirio_ui::markdown_preview` then turns lone images into picture blocks
and diagram fences into SVG files in a per-user cache, which bezel's
`BlockKind::Image` draws. gpui rasterises SVG images at 2× on its own, so the
SVG is cached as rendered. Mermaid renders in-process
(`mermaid-rs-renderer`, pinned `=0.3.1` because its version is part of the cache
key). PlantUML follows `sirio_lsp`'s "a program on PATH" model — `plantuml` is
found, never shipped — with one difference: a fence it cannot render gets a
one-line note, because the author asked for a picture. The local run is
sandboxed (`PLANTUML_SECURITY_PROFILE=SANDBOX`: no network, no local
includes — only the embedded stdlib); PlantUML older than 1.2023.9 is refused
with a note, because its sandbox profile is unsafe, and the optional server
(`markdown.plantumlServer`) is off by default because it sends the source off
the machine.
`docs/superpowers/specs/2026-09-23-markdown-rich-preview-design.md` has the design.
