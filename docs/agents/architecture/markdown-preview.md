# Markdown: `ely-documents`, the Preview's HTML, Mermaid, PlantUML (`sirio_ui::markdown`, `sirio_markdown::expand_html`, `sirio_diagram`)

Every surface that draws Markdown (the chat transcript, the file view's Preview, change
requests, a subagent's card) draws it with the vendored `ely-documents` renderer through
`sirio_ui/src/markdown/`; `bezel-markdown` is gone. `MarkdownBlocks` parses a text once per
change (`parse_markdown_guarded`, vendored changes L2 and L10: Ely asks the nesting guard about each text it is about
to parse, in that one parse) and rewrites the tree per surface by a
`Policy` (HTML blocks, diagrams, images, file links, `[[toc]]`); an over-nested text (above 32
levels, measured against a 1 MiB release and an 8 MiB debug main thread: see `MAX_NESTING`) is drawn as one
plain paragraph. `view.rs` draws it, `gather.rs` is Sirio's `TextGather`
(selection and link rectangles; `SelectionHost` is `Run`, `Transcript` or `None`, see
`text-selection.md`), `text.rs` the plain text in push order, `code.rs` the code hook (tree-sitter
colours through `bezel_syntax`, memoized, copy button), `sideways.rs` and `cull.rs` the
horizontal scroll area and the chat's draw-only-what-is-near. The vendored changes are L1 to L10 in
`rust/vendor/ely/LOCAL-CHANGES.md`; read them before a sync. Beyond bezel's reader, every surface
now has LaTeX, GitHub alerts, footnotes, emoji shortcodes, strikethrough and bare-URL links;
`[[toc]]` and heading anchors (GitHub slugs) work in the Preview, where the right-click menu
(Markdown items above the file's), an outline panel and the reading progress bar also live. Forge
text never loads a picture and a `file:` or `#` link in it opens nothing. A Preview picture is checked
and delivered as one string: the renderer's own reading of the address (trim, cut at `?`/`#`, decode once)
is applied first, the resulting path is checked (no network path, no DOS device name or name Windows
changes in any component, no non-UTF-8), and it is handed over encoded so that reading gives it back
exactly (`address_of`, `preview_address`).
`docs/superpowers/specs/2026-11-20-ely-markdown-documents-design.md` has the design.

The file view's Preview has no HTML engine. `expand_html` maps a GitHub-style
subset of raw HTML onto the existing `Block`/`Inline` model and reduces the rest
to text; `sirio_ui::markdown` (`preview_blocks`) then reads the source once into Ely's
tree, with pictures resolved against the file and diagram fences replaced by SVG files
in a per-user cache, which Ely's `Node::Image` draws. The file view keeps the tree
(`PreviewCache`) and reads it again only when the text, the theme's palette, the diagram
settings or a diagram's state changes. The Preview's links open through one handler:
`#slug` scrolls to the heading, `http(s)`/`mailto` go to the system, a path or `file:` link
opens in a tab, and any other scheme or an unresolvable path does nothing. gpui rasterises SVG images at 2× on its own, so the
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
