# Markdown Preview: HTML, Mermaid and PlantUML — design

**Date:** 2026-09-23
**Status:** implemented on `worktree/clear-meadow-552a`; the PlantUML sandbox test needs a machine with `plantuml` to run (it SKIPs without one)
**Parent work:** F-EDIT-01 (the file view's Code/Preview split), F-CORE-FILE-04
(Preview links resolve against the open file's directory)
**Scope of this document:** the file view's Markdown **Preview** learns to
render a GitHub-style subset of raw HTML, ` ```mermaid ` fences and
` ```plantuml ` fences. The chat transcript is out of scope: it parses with
bezel's own parser, streams, and is left exactly as it is.

## §0 What the Preview does today

The Preview (`rust/crates/sirio_ui/src/file_view.rs`, the `MarkdownMode::Preview`
branch of the surface render) parses the buffer with `sirio_markdown::parse` —
its only caller — converts the result to bezel's `markdown::Doc` with
`bezel_doc_from_legacy` (`sirio_ui/src/chat/mod.rs`), and hands that to
bezel-markdown's renderer. Three things a reader of a README expects do not
happen:

- **Raw HTML is printed as source.** `Block::Html` becomes a plain paragraph
  of its own text and `Inline::Html` is pushed into the surrounding text, so
  `<p align="center">`, `<br>` and `<!-- comments -->` appear literally.
- **Diagram fences are code.** A ` ```mermaid ` block is a code block.
- **Images are not pictures.** `Inline::Image` becomes alt text carrying
  `Mark::Image`, and bezel paints that mark exactly like `Mark::Link`
  (`bezel-markdown` `render.rs`, the `Mark::Link(url) | Mark::Image(url)` arm).
  Only a `BlockKind::Image` is drawn as a picture, and the converter never
  produces one.

Two platform facts shape the design:

- bezel's `BlockKind::Image` draws a local path through `gpui::img(PathBuf)`,
  and gpui decodes `.svg` files (`ImageFormat::Svg`, resvg). **A diagram
  rendered to an SVG file can therefore reach the screen through the existing
  image block, with no change to bezel** (pinned `=0.1.4`).
- gpui rasterises an SVG image at twice its size itself
  (`SMOOTH_SVG_SCALE_FACTOR = 2` in bezel-gpui's `svg_renderer.rs`). Sirio
  writes the SVG as rendered and passes `width = logical_width`.

## §1 Decisions

| Question | Decision | Rejected, and why |
|---|---|---|
| Where | File Preview only | Chat: streaming fences and a different parser; not needed now |
| Approach | Native: everything becomes blocks of bezel's `Doc` | Rendering the Preview in the existing `wry` webview: full HTML and official mermaid.js, but the Preview would leave GPUI — the #376 z-order/overlay hiding, link routing over IPC, the theme duplicated in CSS, GPUI tests blind to it, a ~3 MB JS bundle to vendor |
| Mermaid | `mermaid-rs-renderer` in-process | mermaid.js needs a DOM; `mermaid-little` is weeks old |
| PlantUML | `plantuml` on PATH, then a user-configured server | `plantuml-little`: its `graphviz-anywhere` dependency `curl`s an unverified prebuilt C archive at **build time** |
| HTML | A GitHub-style subset mapped onto existing blocks and inlines | A full HTML/CSS engine (see Approach) |
| Remote images | Load automatically, as GitHub and VS Code do | Local-only would leave every README badge as text |

A throwaway probe (not in the repo) rendered a flowchart, a sequence diagram
and a class diagram with `mermaid-rs-renderer` 0.3.1 and rasterised them with
resvg: labels present, no `<foreignObject>` (which resvg cannot draw), 25–200 ms
each, and invalid input returns an error rather than panicking.

## §2 Crates

```
sirio_diagram   NEW leaf — no gpui, no bezel
sirio_markdown  + `expand_html`, + html5gum
sirio_persistence + one settings key
sirio_ui        -> sirio_diagram; the Preview conversion, diagram state, Settings
```

`sirio_diagram` sits with the other leaves in `CLAUDE.md`'s crate graph and is
imported only by `sirio_ui`. Its public surface:

- `DiagramKind::{Mermaid, PlantUml}` and `DiagramKind::from_fence(&str) ->
  Option<DiagramKind>`.
- `render(kind, source, &Options) -> Result<Svg, DiagramError>`, blocking —
  the caller decides which thread it runs on.
- `Options { palette: Palette, plantuml_server: Option<String>, working_dir:
  PathBuf }`. `Palette` is a small struct of colour
  strings defined here, so this crate never names bezel.
- `Svg { markup: String, logical_width: u32, logical_height: u32 }`.
- `DiagramError::{Syntax { message, line: Option<u32> }, NotAvailable, Timeout,
  TooLarge, Server { status, message }, Io(String)}`.

**One change to `sirio_markdown`'s public model:** `Inline::Image` gains
`width: Option<u32>`, so `<img width="120">` survives until the Preview can
turn it into `BlockKind::Image { width }`. Adding a field to a public variant
breaks every construction of it outside the crate, so the verification for
that step is `cargo build --workspace --all-targets`, not a per-crate build.

## §3 The diagram pipeline

### Recognition

The first word of the fence's info string, case-insensitively: `mermaid` is
Mermaid; `plantuml`, `puml` and `uml` are PlantUML. A PlantUML source whose
trimmed text does not begin with `@start` is wrapped in `@startuml` /
`@enduml`, as GitHub and GitLab do.

### States, and what the reader sees

| State | Preview shows |
|---|---|
| `Pending` | The code block, unchanged — no empty box, no flash |
| `Ready` | The diagram, as `BlockKind::Image` |
| `Failed` | The code block, then one muted line: *Mermaid diagram is invalid: …*, *PlantUML timed out after 15 s*, *PlantUML server answered 400: …* |
| `NotAvailable` | The code block, then one muted line: *PlantUML is not available: `plantuml` is not on PATH and no server is configured* |

Installing PlantUML takes effect when the file is reopened; configuring a
server takes effect immediately.

A `NotAvailable` note is shown, unlike `sirio_lsp`'s silence about a missing
server. The two cases differ: a missing language server would add a note to
almost every file nobody asked about, while a ` ```plantuml ` fence is the
author asking for a picture. The note names the missing program, as the file
context menu already does for language servers.

### Execution

- **Mermaid** runs on `background_spawn`, diagrams in parallel, inside
  `std::panic::catch_unwind`. A panic in the crate becomes `Failed`, never a
  crash.
- **PlantUML** runs through **one app-wide serial queue** (a gpui `Global`
  owning a single worker). Twenty fences in one file, or the same file in two
  tabs, never start more than one JVM at a time.
- The local command is `plantuml -tsvg -pipe`, source on stdin, SVG on
  stdout, the Markdown file's directory as the working directory. It is found
  on the process `PATH`, which `login_path` has already merged with the login
  shell's at startup. On Windows the lookup follows `PATHEXT` (`.exe`, `.cmd`,
  `.bat`), because package managers install PlantUML as a shim. A spawn that
  fails with `NotFound` means "not installed", as it does for
  `sirio_lsp`.
- Timeouts: **15 s** local (a cold JVM takes 1–3 s), **10 s** server. On
  timeout the child is killed.
- A source over **64 KiB** is not rendered: `TooLarge`.

### Local PlantUML is sandboxed

PlantUML's preprocessor reads files (`!include`), environment variables
(`%getenv`) and URLs (`!includeurl`). The Preview runs it on any Markdown file
the user opens, including one from a repository cloned a minute ago, so an
unsandboxed run lets a hostile README exfiltrate data **because it was
opened**. The local run uses PlantUML's `SANDBOX` profile — no network and no
local file access; `!include` works only for PlantUML's embedded standard
library (`<C4/…>` etc.), because `ALLOWLIST`'s path check is a raw string
prefix that `..` and symlinks escape (verified on 1.2026.8). The child's PATH
drops relative entries and its environment drops
`PLANTUML_INCLUDE_PATH`/`plantuml.include.path`,
`PLANTUML_ALLOWLIST_URL`/`plantuml.allowlist.url`, `JAVA_TOOL_OPTIONS`,
`_JAVA_OPTIONS`, `JDK_JAVA_OPTIONS`; PlantUML older than 1.2023.9 is refused; a timeout kills the whole process tree.

### The server

Consulted when the local command is `NotAvailable` or too old to sandbox, and
`markdown.plantumlServer` is non-empty.

- Request: `GET {server}/svg/{encoded}`, where `encoded` is the source
  compressed with raw DEFLATE and written in PlantUML's own base64 alphabet
  (`0-9A-Za-z-_`). This is the plantuml-server protocol, understood by
  plantuml.com and by any self-hosted instance.
- Accepted: status 200, a body of at most **2 MiB** whose first non-whitespace
  characters are `<svg` or `<?xml`.
- On error, `X-PlantUML-Diagram-Error` and `X-PlantUML-Diagram-Error-Line`
  (when present) become the `Failed` note. Otherwise the note carries the
  status.
- `ureq` is already in the lockfile (via `sirio_registry` and `sirio_update`).

### Theme

`mermaid_rs_renderer::Theme` is a public struct of colours. `sirio_ui` maps
bezel's current palette onto a `sirio_diagram::Palette` (page background,
text, node fill, node border, lines, edge-label background), and
`sirio_diagram` builds the Mermaid theme from it, starting from
`Theme::modern()` for light and `Theme::dark()` for dark. A Mermaid diagram
therefore matches Sirio in either appearance and under any `baseColor`.
PlantUML keeps its own palette and sits on its own light background in both
appearances, as images do on GitHub.

### Crisp on 2× displays

`sirio_diagram` records the SVG root's `width`/`height` (derived from
`viewBox` when absent) as `logical_width`/`logical_height`. gpui rasterises
SVG images at twice their size itself, so Sirio writes the SVG as rendered and
passes `width = logical_width` to `BlockKind::Image`.

### Cache

- **Key:** SHA-256 over the `svg-v2` format salt, kind, a renderer identity
  (crate name and version for Mermaid, `plantuml` for PlantUML), the palette
  fingerprint (Mermaid only) and the source. `sha2` is already in the lockfile.
- **On disk:** `<key>.svg`, written to a temporary name and renamed into
  place so gpui can never read half a file. Directory:
  `$XDG_CACHE_HOME/Sirio/diagrams` (falling back to `~/.cache/Sirio/diagrams`)
  on Linux, `~/Library/Caches/Sirio/diagrams` on macOS,
  `%LOCALAPPDATA%\Sirio\cache\diagrams` on Windows. Content addressing means a
  path's bytes never change, so gpui's own image cache cannot go stale, and
  PlantUML's JVM is not rerun across launches.
- **In memory:** each `FileView` keeps `key -> state`. A render first checks
  the disk (on the background thread); a hit is `Ready` without rendering.
  A completed render updates the map and calls `cx.notify()` on the view.
  `Failed` and `NotAvailable` exist only in memory, never on disk, and are
  dropped when `markdown.plantumlServer` changes. Installing PlantUML takes
  effect when the file is reopened; configuring a server takes effect
  immediately.
- **No pruning in this version.** The files are a few KiB each; this is a
  deliberate omission.

## §4 The HTML subset

`sirio_markdown::expand_html(Document) -> Document` is a pure pass applied
only by the Preview, right after `parse`. `parse` stays a faithful CommonMark
model.

pulldown-cmark has already split the HTML:

- **Inline HTML** arrives as separate tags between text:
  `[Text("Hi "), Html("<b>"), Text("there"), Html("</b>")]`. The pass folds
  every inline list with a tag stack, recursing into `Strong`, `Emphasis` and
  `Link` children. An unclosed or crossed tag is dropped and its text kept.
- **An HTML block** is one `Block::Html` chunk, tokenised with `html5gum`
  (WHATWG-conformant, MIT, decodes entities such as `&nbsp;` and `&amp;`). A
  `<details>` containing Markdown arrives as three blocks — the opening HTML,
  the Markdown, the closing HTML — and each is handled on its own.

| HTML | Becomes |
|---|---|
| `<!-- … -->` | removed |
| `<br>` / `<hr>` | `HardBreak` / `ThematicBreak` |
| `<b>` `<strong>` / `<i>` `<em>` | `Strong` / `Emphasis` |
| `<code>` `<kbd>` `<samp>` `<tt>` | `Code` |
| `<pre>` | `CodeBlock` |
| `<a href>` | `Link` (relative targets open files, as Markdown links do) |
| `<img src alt width>` | `Image { width }` |
| `<picture>` | its fallback `<img>` |
| `<h1>`…`<h6>` / `<blockquote>` | `Heading` / `BlockQuote` |
| `<details>` + `<summary>` | the summary as a bold paragraph; the content always shown (bezel has no collapsible block) |
| `<p>` `<div>` `<span>` `<center>` `<section>` and other containers | transparent: their content stays, `align` is ignored (bezel has no alignment) |
| `<script>` `<style>` `<iframe>` `<object>` `<embed>` `<form>` `<svg>` | removed **with their content** |
| any other tag (`<sub>`, `<sup>`, `<table>`, `<ul>`, …) | the tag removed, its text kept |

The "with their content" row matters: under the general rule, a `<style>`
block would appear as a paragraph of CSS.

## §5 Images and the Preview conversion

A new module, `sirio_ui::markdown_preview`, owns the Preview's conversion from
the legacy `Document` to bezel's `Doc`. It rewrites two things and delegates
everything else to the existing `bezel_doc_from_legacy` machinery (which
becomes `pub(crate)` where needed). The chat's path is untouched.

- **A paragraph whose only content is one image** becomes
  `BlockKind::Image { url, alt, width }`. A relative `url` is resolved against
  the Markdown file's directory, because `gpui::img(PathBuf)` would otherwise
  resolve it against the process's working directory. `http(s)://` URLs are
  passed through: Sirio's app is built with `gpui_platform::application()`,
  which installs the HTTP client bezel needs. An image inside running text
  stays a link, since bezel cannot place a picture inline. The same holds for
  a paragraph of several images — a README's badge row stays a row of
  links — and for a lone image wrapped in `<a>`, the picture is drawn and the
  wrapping link is dropped: clicking it opens the picture.
- **A diagram fence** becomes whatever its state in §3 dictates.

Both rewrites apply wherever the block sits, including inside list items and
quotes (bezel's flat model carries the indent).

## §6 Settings

Settings → General gains a **Markdown preview** section:

- **Local PlantUML**, read-only: *`plantuml` found at /usr/bin/plantuml* or
  *`plantuml` is not on PATH*.
- **PlantUML server**, a text field, empty by default, with the line *When
  `plantuml` is not installed, diagram source is sent to this server.*

The key is `markdown.plantumlServer` (`String`, empty = off) on
`AppSettings`, with a persistence round-trip test.

## §7 Errors and tracing

Nothing in this feature can take the Preview down: every failure degrades one
diagram to its code block plus at most one line. `sirio_perf` names are
content-free `&'static str`s (`"Preview.diagram_render"`,
`"Preview.expand_html"`) — never a diagram's source, a file path or an error
message — per that crate's one hard rule.

## §8 Tests

Written first, per the repo's convention.

**`sirio_diagram`**
- Fence recognition, including case and the three PlantUML spellings.
- `@startuml` wrapping, and no double wrapping of `@startmindmap`.
- PlantUML encoding: the example string on plantuml.com's text-encoding page
  decodes to the text published beside it, and encode∘decode round-trips. The encoder's own bytes are
  not compared, because DEFLATE output legitimately varies between
  implementations.
- `as_rendered`: markup unchanged; logical size read from `width`/`height`,
  from `viewBox` when absent, with `px` accepted.
- Mermaid: a valid source renders; an invalid one is `Syntax`; 64 KiB + 1 is
  `TooLarge`.
- Local PlantUML against a **fake `plantuml`** placed first on a temporary
  `PATH`: one that prints an SVG, one that exits 1 with an error on stderr,
  one that sleeps past the timeout. `cfg(unix)` scripts plus a `.cmd` for
  Windows.
- The server against a **loopback listener**: 200 with an SVG; 400 with the
  two error headers; a body over 2 MiB; a non-SVG body.
- The sandbox test of §3.
- One test against the real `plantuml` that SKIPs when it is absent, like
  the agent conformance tests.

**`sirio_markdown`** — one test per row of §4's table, plus crossed and
unclosed tags, `<details>` split across three blocks, `<script>`/`<style>`
content removed, entity decoding, `<img width>` reaching `Inline::Image`.

**`sirio_ui`**
- `markdown_preview` as a pure function: fence plus each state gives the
  expected blocks; an image-only paragraph gives `BlockKind::Image` with its
  width and a resolved path; an image in running text stays a link; the same
  rewrites inside a list item.
- One drawn-frame test (`TestAppContext`) of a Preview showing a **local**
  image. Test apps have no HTTP client, so a remote URL would only ever show
  the fallback.

**`sirio_persistence`** — the settings key's round trip and default.

## §9 Documentation

- `CLAUDE.md`: `sirio_diagram` among the leaves, the `sirio_ui ->
  sirio_diagram` edge, and one line noting that PlantUML follows
  `sirio_lsp`'s "a program on PATH" model with the §3 difference about
  notes.
- This document's status line, once implemented.

## §10 Deliberately absent

- Rendering in the chat transcript.
- HTML tables, HTML lists, alignment, and a collapsible `<details>`: their
  text stays readable, without structure.
- A dark palette for PlantUML.
- Cache pruning.
- A setting to block remote images.
- Kroki or any server protocol other than plantuml-server's.
