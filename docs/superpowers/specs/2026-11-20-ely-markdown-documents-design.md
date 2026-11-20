# Markdown on `ely-documents` (programme step 2)

Status: decisions delegated by Enzo (2026-11-20 13:02, via t2s); each call is recorded in §9 with its
cost · 2026-11-20 · branch `feat/ely-markdown-documents` (from `main` `f0a3fdaa`, version 0.46.2)

## 1. Why

Sirio draws Markdown in three places: the chat transcript, the file view's Markdown Preview, and
change requests (description, timeline, review threads). All three paint through `bezel_markdown`
0.1.4, a vendored crate from the bezel family that Sirio is otherwise leaving. Since step 1b (#823)
Sirio vendors upstream Ely's `ely-documents`, a Markdown reader and renderer that already ships
LaTeX, GitHub alerts, footnotes, emoji shortcodes, a contents block and heading anchors.

This step moves every Markdown surface onto `ely-documents`, so that:

- `bezel_markdown` leaves Sirio entirely (the last bezel UI crate besides `bezel_theme` and
  `bezel_syntax`, which stay);
- the renderer's capabilities reach Sirio's readers;
- step 3 (chat features: streaming Markdown, message actions, …) builds on the same renderer.

### 1.1 The programme

Agreed with Enzo on 2026-11-16. Step 1 is done: 1a #806 (engine swap to `gpui-pre =0.3.12`), 1b #823
(vendored Ely onto upstream's crates at `8d7723ec`, shipped in 0.46.1).

2. **Markdown on `ely-documents` (this document).** Enzo's scope, verbatim from the programme:
   - `bezel-markdown` fully out: chat, Markdown Preview, file view, change requests,
     `selectable_markdown`.
   - Keep **every** current Preview feature: `expand_html`, images, the Mermaid cache, sandboxed
     PlantUML (server off by default), selection and Ctrl+C, the Code/Preview toggle, scroll bars.
   - Add every renderer capability: LaTeX, GitHub alerts, footnotes, emoji shortcodes, `[[toc]]`,
     heading anchors, a right-click menu; plus an outline panel and reading progress. Nothing else.
   - Out: word count, split view, zen, print, EPUB, block editor, history, backlinks, templates,
     thumbnails, rich text, covers.
   - Step 1b left one decision here: upstream's patched `pulldown-cmark` (`e71b644b`) and the two
     `ely-documents` tests it `#[ignore]`d.
3. **Chat features from Ely**, one delivery and PR at a time (not this step).

## 2. Goals, non-goals, anti-goals

**Goals**

1. No crate in Sirio's build depends on `bezel-markdown`; `rust/vendor/bezel_markdown` is deleted.
2. Every surface that draws Markdown today draws it through `ely_documents::MarkdownRenderer`.
3. Every kept behaviour in §5.8's table still holds, proven by a test or an E2E named there.
4. The added capabilities work where §5.7 says, proven by an E2E with a re-runnable artifact.
5. Each surface parses once per change of its text, not once per frame (today the Preview reparses
   and rebuilds every frame, `file_view.rs:1896`).

**Non-goals**

- Chat feature work of step 3: streaming reveal (`StreamingMarkdown`), date separators, message
  actions, … The chat keeps re-parsing a growing reply per chunk, as today.
- Diagrams in chat or change requests (they show as code today and keep doing so).
- Anything on Enzo's out-list, even where `ely-documents` offers it.

**Anti-goals** (a failure even if everything renders)

- A terminal's Ctrl+C stops being SIGINT, or a selection's Ctrl+C stops reaching the clipboard.
- A plain click in a chat reply moves focus out of the composer.
- Forge text (change requests) gains a way to read local files or reach the network that it does not
  have today.
- PlantUML runs without its sandbox, or the PlantUML server turns on by default.

## 3. Constraints

- Version: this is the cycle's first `feat:`; the PR moves the workspace to `0.47.0` with
  `Scripts/set-workspace-version.sh 0.47.0` and `(cd rust && CARGO_SCOPE=0 cargo update --workspace --offline)`.
- Vendored Ely stays a mechanical diff against upstream `8d7723ec`: every Sirio change inside
  `rust/vendor/ely/crates` is recorded in `rust/vendor/ely/LOCAL-CHANGES.md` (its `## Differing paths`
  section and a prose entry) and `check-vendor.sh` prints `VENDOR CHECK OK`. Local changes are
  additive and opt-in: upstream callers inside the closure keep their behaviour.
- One gpui (`gpui-pre =0.3.12`), one copy of each Ely type, reached through the
  `ely-gpui-component` facade.
- Testing is end to end (CLAUDE.md): every new E2E ends in a re-runnable artifact; a `#[test]` only
  for a unit proven in isolation, failure modes written first.
- Machine rules: `CARGO_BUILD_JOBS=4`, at most two builds machine-wide, a private target per
  worktree removed by absolute path at the end; no `Scripts/ci.sh`, `ci-linux.sh`, `cargo fmt`,
  `rustfmt` or `clippy`; E2E only under a private Xvfb (`:120`–`:139`) with TMPDIR on disk; no
  `pkill`/`killall` by name; no bare `sirioctl` or installed `sirio`.
- One PR, several deliveries (§5.10); push when a delivery is ready, not per commit.

## 4. Approaches considered

**A. Sirio owns the tree and the selection; Ely draws (chosen).** Sirio parses each text once with
Ely's own reader, rewrites the resulting `Node` tree per surface (diagrams, HTML blocks, file links,
image policy, `[[toc]]`), and draws it with `MarkdownRenderer::blocks(id, Rc<[Node]>)`. Selection,
Ctrl+C and the right-click menu stay Sirio's (`text_selection` and the chat transcript), fed by a new
opt-in hook through which the renderer hands each drawn text and its layout to the host. Code blocks
go through the renderer's existing `code` hook so they keep tree-sitter colours, the copy button and
their sideways scroll bar.
- For: every kept feature survives; parse once per change; untrusted text is filtered before it is
  drawn; Ely's reader decides syntax, so math, alerts, footnotes, emoji and `[toc]` come from
  upstream unchanged.
- Against: eight small local changes in the vendored crates (§5.2) to carry on each sync.

**B. Ely owns everything (`MarkdownRenderer::new(source).selectable(..)`).** Almost no Sirio code.
Rejected: Ely's selection is per body, takes focus on press, handles Ctrl+C in its own key handler,
clears on every text change and skips code blocks drawn by the host; the chat's transcript-wide
selection, the focus rule and the Ctrl+C sink would all be lost. It also drops the Mermaid disk
cache, PlantUML, the HTML subset (Ely shows block HTML as text, so a README's `<img>` vanishes),
and lets forge text name any local image path.

**C. Keep Sirio's parser (`sirio_markdown`) and convert to Ely's `Node`.** No parser change.
Rejected: alerts, footnotes, emoji, math and `[toc]` would be re-implemented in the converter,
diverging from upstream's reader on every sync.

## 5. Design

### 5.1 Where things live

```text
rust/vendor/
├── bezel_markdown/                  # deleted
├── pulldown-cmark/                  # new: upstream Ely's patched copy (§5.3)
└── ely/crates/ely-documents, ely-core   # local changes L1–L8 (§5.2)
rust/crates/sirio_ui/src/
├── markdown/                        # new home of all Markdown drawing
│   ├── mod.rs        # MarkdownBlocks: parse once + rewrite; per-surface Policy
│   ├── rewrite.rs    # HTML blocks, diagrams, images, file links, [[toc]], anchors
│   ├── text.rs       # the plain text of a tree, in the renderer's push order
│   ├── gather.rs     # host TextGather: highlights in, layouts and link rects out
│   ├── code.rs       # the code hook: bezel_syntax colours, copy button, sideways bar
│   └── menu.rs       # right-click menu: Copy, Select all, Open link, Copy link address
├── selectable_markdown.rs           # Preview and change requests, on markdown/
├── markdown_preview.rs              # diagram pipeline + HTML expansion, on markdown/
└── chat/mod.rs, chat/file_links.rs  # MarkdownBody on markdown/
```

`sirio_markdown` stays (the editor's file I/O, and `expand_html` for HTML blocks, §5.5).
`sirio_diagram` is unchanged. The facade `ely-gpui-component` gains `pub use ely_documents as documents;`
(and its manifest an `ely-documents` dependency), so Sirio writes `ely_gpui_component::documents::…`.
The module split above is a proposal; the builder may merge files, not responsibilities.

### 5.2 Local changes in the vendored crates

Each is opt-in or additive, recorded in `LOCAL-CHANGES.md`, and covered by an upstream-style test in
the crate where the crate's own tests can show it.

| # | Crate, file | Change | Why |
|---|---|---|---|
| L1 | `ely-documents` `render.rs`, `render/select.rs` | A public trait `TextGather` with the three calls drawing already makes on its private `Gather` (`over(text) -> highlights`, `push(text, layout, links)`, `lead(marker)`); `Gather` implements it; `MarkdownRenderer::gather(Rc<dyn TextGather>)` installs a host one. With a host gather the body gets no Ely selection handlers, focus or key handling. A right press on a picture reaches Ely's menu only with Ely's own gather. | Sirio keeps its selection model (§5.6) while Ely draws. |
| L2 | `ely-documents` `tree/mod.rs`, `lib.rs` | `pub fn parse_markdown(source) -> Vec<Node>` (today's crate-private `parse`) and `parse_markdown_with(source, html_block)` where `html_block: &dyn Fn(&str) -> Option<Vec<Node>>` replaces a block of raw HTML (at any depth) when it returns `Some`. | One parse per change, outside the draw; the HTML subset (§5.5). |
| L3 | `ely-documents` `tree/mod.rs`, `picture.rs`, constructors | `Node::Image` gains `width: Option<f32>` (logical pixels); `Picture` draws at that width when set, still capped to the column. Upstream's reader always sets `None`. | `<img width=…>` in README HTML keeps its size (a kept Preview behaviour). |
| L4 | `ely-documents` `render.rs`, `picture.rs` | `MarkdownRenderer::web_pictures(bool)`, default `false`: when `true`, an `http(s)://` picture loads through gpui's image loader (`Resource::Uri`, the app's HTTP client) instead of the "Not loaded from the web" note. | Remote images in the Preview and chat, as bezel loaded them (`test-markdown-remote-image-e2e.sh`). |
| L5 | `ely-documents` `render.rs` | `MarkdownRenderer::diagrams(bool)`, default `true`: when `false`, a `mermaid` fence goes to the `code` hook like any other fence. | Sirio keeps its own diagram pipeline in the Preview and code-only fences elsewhere. |
| L6 | `ely-core` `navigation/toc.rs` | `Sections::reveal(&mut self, value, …)`: scrolls the section's spot to the top, the same arithmetic `TableOfContents` uses on a click. | `#heading` links and the `[[toc]]` list jump within the Preview. |
| L7 | workspace `Cargo.toml`, `rust/vendor/pulldown-cmark` | §5.3. | Upstream's footnote-in-link fix; restores two ignored tests. |
| L8 | `ely-documents` `render.rs`, `render/table.rs` | `MarkdownRenderer::table_frame(f)` where `f: Fn(ElementId, AnyElement) -> AnyElement`: when set, a table is laid at its natural width (columns do not shrink or wrap below their content) and handed to `f`, which places it. Unset, upstream's wrapping table is unchanged. | Sirio wraps each table in its own horizontal scroll area with `scroll::horizontal_bar`, as fences are, on all three surfaces: tables keep their sideways scroll bar (Enzo, #649; `test-markdown-scroll-probe.sh`). |

Names are proposals; the builder may rename, recording it. Anything else found necessary is a new
row, recorded, never a silent edit.

### 5.3 The patched `pulldown-cmark` (decided)

Take it. Vendor upstream Ely's `vendor/pulldown-cmark` (0.13.5 plus the one-line `e71b644b` change:
a link may hold a footnote reference, as GFM reads it) at `rust/vendor/pulldown-cmark`, wire it with
`[patch.crates-io]`, exclude it from the workspace members, record it in `rust/vendor/README.md`,
and remove the two `#[ignore]`s (`a_note_in_a_markdown_link_frees_the_math_after_it`,
`a_link_around_a_footnote_reference_stays_a_link`) with their LOCAL-CHANGES entries.

Effect on `sirio_markdown`: none observable. It parses with `TABLES | TASKLISTS` only, and the
change touches footnote references, which it never enables. Its tests must stay green as proof.
Cost: one more vendored crate that `check-vendor.sh` does not cover, recorded as such.

### 5.4 Parsing once: `MarkdownBlocks`

`MarkdownBlocks` is Sirio's parsed document: `Rc<[Node]>` plus what each surface needs alongside
(the plain text and part offsets of §5.6, the heading slugs of §5.7, the outline entries). It is built
from `(source, Policy)` by `parse_markdown_with` and then the rewrites of §5.5, and is rebuilt only
when the source or the policy's inputs change:

- **Preview:** on buffer change, file change, or a diagram becoming ready or failed (today's
  per-frame parse goes away). The 256 KiB lock and "Render preview" button stay as they are.
- **Chat:** on each streamed chunk (as today) and on restore.
- **Change requests:** on header refresh and thread change (as today).

**Nesting guard.** Ely's reader and drawing recurse once per nested quote or list, with no depth
limit (only its HTML has one, `tree/html_block.rs:447`), and chat and forge text are untrusted.
Before parsing, Sirio walks the same source with `pulldown-cmark`'s event stream (iterative, same
options as Ely's reader) and measures container depth; above 128 (the cap `sirio_markdown` already
uses, `parse.rs:12-18`) the text is drawn as one plain paragraph of its source, still selectable.
This is a Sirio-side check, not a vendored change, and gets a failure-first test (a 10 000-deep `>`
run renders without overflowing the stack).

### 5.5 Rewrites, by surface (`Policy`)

| Rewrite | Preview | Chat | Change requests |
|---|---|---|---|
| Block HTML via `expand_html` (L2 hook: `sirio_markdown` model → `Node`) | yes | yes | yes |
| Mermaid/PlantUML fences → cached SVG `Node::Image`, or fence + note (§5.5.1) | yes | no, code | no, code |
| Relative image path | → file's folder | → worktree, then the absolute-path rule (today's `local_image_path`) | alt text |
| Absolute or `file:` image | kept | kept only if a regular file ≤ 10 MiB (`local_image_path`), else alt text | alt text |
| Web image | loads (L4) | loads (L4) | alt text, plus a link to it |
| `data:` image | kept | kept | alt text |
| Paths to files in the worktree → file links (`link_file_paths`) | — | yes | — |
| `[[toc]]` and `[toc]` paragraph → a list of `#slug` links | yes | yes, links inert | yes, links inert |
| Ely `diagrams(false)`, `web_pictures` | false, true | false, true | false, false |
| Tables in a sideways scroll area (L8) | yes | yes | yes |

- **HTML.** Inline HTML is Ely's (its reader maps `u`, `b`, `i`, `s`, `code`, `kbd`, `mark`, `br`,
  `a`). Block HTML goes through the L2 hook to `expand_html` (the GitHub subset it maps today:
  headings, paragraphs and `div` with `align`, `img` with `width`, `a`, `details`/`summary`,
  `blockquote`, `pre`, `hr`, removed `script`/`style`/`iframe`/`object`/`embed`/`form`/`svg`), and
  the resulting model converts to `Node` (the converter replaces `bezel_doc_from_legacy`). A block
  `expand_html` reduces to nothing yields no nodes.
- **Images in forge text** never reach Ely's loader: the change-request policy turns every picture
  into text before drawing, as `images_as_text` does today.
- **Links.** Every renderer gets an `on_link`, so Ely's default system opener never runs:
  - Preview: `#slug` → reveal (§5.7); a diagram target is ignored; `resolve_file_link` → open the
    file; else `cx.open_url`.
  - Chat: file links → `ChatEvent::OpenFileAt`; web → `cx.open_url`; `#slug` ignored.
  - Change requests: web → `cx.open_url`; `file:` and `#slug` ignored. This is a fix, not kept
    behaviour: today `open_links` (`change_request_tab.rs:1330`) hands every address, `file:`
    included, to `cx.open_url`, because `selectable_markdown` handles the release before bezel's own
    `file:` guard runs (D7).
  - Everywhere: a link press that ends a drag, or that lands while a selection is live in that body,
    does not follow the link. Today that holds in the Preview and change requests
    (`a_drag_across_a_link_selects_it_without_following_it`); in the chat a drag ending on a web link
    opens it, which changes (D8).

#### 5.5.1 Diagrams in the Preview

Unchanged pipeline, new output type. `sirio_diagram` renders Mermaid in process and PlantUML
sandboxed, through the same per-user SVG cache, `PlantUmlQueue` and settings
(`markdown.plantumlServer` off by default). In the tree: `Ready` → `Node::Image` of the cached SVG
path at its logical width (L3); `Failed` → the fence as code plus a muted note paragraph; `Pending`
or no settings → the fence as code. A diagram becoming ready rebuilds `MarkdownBlocks`.

### 5.6 Selection, copy and the right-click menu

Sirio keeps today's model (`docs/agents/architecture/text-selection.md`): one selection at a time,
selecting moves focus to the root sink, Ctrl/Cmd+C goes through `text_selection::on_copy` (scoped
`!Terminal`, declines when there is nothing to copy), and the chat transcript has one document-wide
offset space.

- **Gather (L1).** Each body installs a Sirio gather. During the draw it receives every drawn text in
  push order with its `TextLayout` and link ranges, returns the selection's highlight for that text,
  and keeps the layouts for hit-testing (press, drag, double- and triple-click), link rectangles
  (the chat's `record_links` for the E2E's `transcriptLinks`) and the caret rule.
- **Plain text (`text.rs`).** A walk of the final tree yields every text the reader shows, in drawing
  order, as parts of one offset space, so a body's plain text and part offsets are known without
  drawing; the chat needs this for off-screen entries (`transcript_entry_ranges`). Two kinds of part:
  - *drawn* parts, the texts the renderer pushes to the gather (with the same list markers);
  - *silent* parts, texts Ely draws without pushing: a paragraph holding inline math and display math
    (as the TeX written), an alert's title, a footnote's label. A silent part cannot be hit or
    highlighted, but a selection that spans it, and Select All, copy it; so Select All and Copy in
    a chat reply never drops a paragraph because it holds `$x$`.

  A test asserts that the drawn parts are exactly the gather's pushes, in order, on a fixture
  covering every node kind.
- **Code blocks** are drawn by Sirio's `code` hook (§5.1 `code.rs`): `bezel_syntax` colours, the copy
  button, the sideways scroll bar, the language label. The hook pushes the code text into the same
  gather, so code stays selectable and copyable.
- **Not hit or highlighted** (renderer limits, D4): the silent parts above. Pictures (Mermaid,
  PlantUML, images) are not parts at all, and copy skips them.
- **Right-click menu (`menu.rs`)**, Sirio's popup, on every Markdown body:
  - Change requests: Copy (enabled with a selection), Select all (this body), and over a link Open
    link and Copy link address.
  - Preview: today a right press anywhere in it opens the file view's menu (`file_view.rs:2075`,
    items from `file_context_menu.rs`). The two merge: the Markdown items above come first, then a
    separator, then the file items unchanged. The file view's existing menu tests stay green.
  - Chat: the transcript's existing menu (Copy, Select All) gains Open link and Copy link address
    over a link.

### 5.7 Added capabilities and where they show

| Capability | Source | Preview | Chat | Change requests |
|---|---|---|---|---|
| LaTeX (`$…$`, `$$…$$`, `\(…\)`, `\[…\]`) | Ely reader + ratex | yes | yes | yes |
| GitHub alerts (`> [!NOTE]` …, top level) | Ely | yes | yes | yes |
| Footnotes (references, definitions drawn in place) | Ely + L7 | yes | yes | yes |
| Emoji shortcodes (`:tada:`) | Ely | yes | yes | yes |
| Strikethrough, task boxes, bare-URL links | Ely | yes (new in Preview) | yes | yes (new) |
| `[[toc]]` / `[toc]` → list of heading links | Sirio rewrite | jumps | drawn, inert | drawn, inert |
| Heading anchors: GitHub slugs, `#slug` links jump | Sirio slugs + `sections` + L6 | yes | — | — |
| Right-click menu | Sirio (§5.6) | yes | extended | yes |
| Outline panel | Sirio's heading walk + Ely `TableOfContents` | yes | — | — |
| Reading progress | Ely `ReadingProgress` | yes | — | — |

- **Slugs** follow GitHub: lowercase, punctuation other than `-` and `_` dropped, spaces to `-`,
  repeats suffixed `-1`, `-2`, in document order. A slug maps to the renderer's `heading-N` ordinal,
  counted in the renderer's drawing order over the final tree (alerts included), so the outline and
  the anchors cannot drift (the recon found `outline()` skips alert bodies; Sirio does not use it on
  the source for that reason).
- **Outline panel.** A toggle in the Preview header beside Code/Preview (icon button, tooltip
  "Outline"). Shows a column at the Preview's right edge listing the headings (`TableOfContents`,
  which follows the section in view and jumps on click). Hidden by default, remembered per file view
  for the session; the toggle is disabled for a document with no headings (`TableOfContents` asserts
  a non-empty list).
- **Reading progress.** `ReadingProgress` on the Preview's scroll handle, a thin bar along the top
  edge of the Preview column, drawn only when the document scrolls.

### 5.8 Kept behaviour and its proof

| Behaviour | Today's proof | After |
|---|---|---|
| Code/Preview toggle, 256 KiB lock | `file_view.rs` tests, `test-editor-e2e.sh` `scenario_markdown` | same, green |
| Preview selection, double/triple click, Ctrl+C, link click vs drag | `selectable_markdown.rs` gpui tests, `test-markdown-selection-probe.sh`, `test-text-selection-e2e.sh` | ported to Ely; probe passes |
| Preview scroll bars (vertical, horizontal page, fence) | `test-markdown-scroll-probe.sh` | passes for page, fences and tables (L8) |
| Remote image | `test-markdown-remote-image-e2e.sh` | passes (L4) |
| Local/relative images | `file_view.rs` local image test | green |
| HTML subset | `sirio_markdown` `html.rs` 34 tests + `markdown_preview.rs` tests | green; new tree-level tests where the converter needs them |
| Mermaid cache, dedupe, PlantUML queue, settings change | `file_view.rs:5463-5587`, `sirio_diagram` tests | green |
| Font follows the interface size | `rendered_markdown_follows_the_interface_font_size` | green |
| Chat transcript selection, copy, code copy, file links | `chat/mod.rs` tests (§recon), `ely_tests.rs:169`, `test-chat-session-panel-e2e.sh` `scenario_file_links` | green |
| Change-request description selected and copied, images as text | `change_request_tab.rs` tests | green |
| Ctrl+C in settings and main shell | `text_selected_in_settings_is_copied_by_the_real_shell`, `…main_shell…` | green |

### 5.9 Visible differences (for Enzo's hand checks)

Expected, recorded in the PR's "Visible changes" section and in the `differences.md` of the capture
comparison (§6.3); each is accepted by this spec unless the comparison shows it worse than described.

- **D1 Font.** Markdown text moves from bezel's Geist names to Ely's theme families, i.e. Sirio's
  resolved UI and code fonts; Ely's layout (line height 1.6, heading steps 2.25/1.75/1.5/1.25).
- **D2 Chat-only bezel syntax goes.** `<url>` chips, lone-URL bookmark cards and `![alt|480]` widths
  render as plain links and natural-size pictures.
- **D3** (withdrawn by Enzo via t2s, 2026-11-20: tables keep their sideways scroll, L8).
- **D4 Not highlighted:** math, alert titles and footnote labels are copied but cannot be hit or
  highlighted; diagrams and pictures are neither (§5.6).
- **D5 Gained syntax in Preview and change requests:** strikethrough and bare-URL links, which
  `sirio_markdown`'s options did not read.
- **D6 Inline HTML** follows Ely's mapping (`<u>`, `<mark>` gain styles; an inline `<img>` in a
  Markdown paragraph is no longer drawn).
- **D7 Forge links:** a `file:` or `#…` link in change-request text no longer reaches the system
  opener (a fix, §5.5).
- **D8 Chat link after a drag:** a drag that ends on a web link in a reply selects instead of opening
  it, as in the Preview.
- **D9 Block HTML in chat replies** is expanded (`expand_html`) instead of shown as bezel drew it.
- **D10 A picture inside a sentence** becomes its own block between the text before and after it
  (Ely's reader splits a paragraph at each picture). `an_image_in_running_text_stays_a_paragraph`
  (`markdown_preview.rs:487`) is rewritten to the new behaviour, recorded.
- **D11 The Preview's right-click menu** gains Markdown items above the file items (§5.6).

### 5.10 Order of work (one PR)

0. **Baseline.** On the untouched branch head, build and capture the Preview, a chat reply and a
   change request from the one fixture of §6.2, so differences can be listed. Nothing is changed
   before this exists.
1. **Vendor seams.** L1–L8, upstream suites green with `--features test-support`, `check-vendor.sh`
   OK, LOCAL-CHANGES updated, facade re-export.
2. **`sirio_ui::markdown`.** `MarkdownBlocks`, the rewrites, the plain-text walk, the gather, the code
   hook, the menu. Unit tests where a unit needs isolation, failure modes first.
3. **Preview** on it, with the diagram pipeline, HTML, images, anchors, `[[toc]]`, outline panel and
   reading progress; the two probes ported.
4. **Change requests** (description, timeline, threads).
5. **Chat** (`MarkdownBody`, file links, transcript selection, restore).
6. **Removal.** `bezel_markdown` out of manifests and `rust/vendor`, `TextOffset` gone, docs
   updated (`docs/agents/architecture/{markdown-preview,text-selection,claude-chat,crates}.md`,
   `rust/vendor/README.md`, `docs/THEME-PROVENANCE.md`, `docs/agents/e2e-tests.md`, CLAUDE.md's
   bezel/Ely lines), version 0.47.0.
7. **E2E and captures** (§6), then the PR.

## 6. Verification

### 6.1 Build and tests

- `cargo build -p sirio`, `cargo build -p sirio_ui --examples`.
- `cargo nextest run` for `sirio_ui`, `sirio`, `sirio_markdown`, `sirio_diagram`, `sirio_theme`.
- `ely-documents` and `ely-core` with `--features test-support`, including the two restored tests.
- `rust/vendor/ely/check-vendor.sh` with `ELY_UPSTREAM` set → `VENDOR CHECK OK`; the ignored-files
  check `git status --ignored --porcelain rust/vendor | grep -v /target/` prints nothing.
- `cargo tree -i bezel-markdown` finds nothing.

### 6.2 New E2E: `Scripts/Tests/test-markdown-documents-e2e.sh`

The real app, isolated, under a private Xvfb, opening one committed fixture
(`Scripts/Tests/fixtures/markdown-documents/`) that holds every kept and added feature: headings
with repeated titles, `[[toc]]`, a `#slug` link, inline and display math, the five alert kinds,
footnotes, emoji shortcodes, a task list, a table, a fenced code block, a Mermaid fence, a PlantUML
fence, README-style HTML with `<img width>`, `<details>`, a relative image and a remote one (local
HTTP server), and a long tail so the document scrolls. It asserts, through the app's debug state and
real X input:

- the Preview draws (math, alert, footnote and picture elements present by debug selector);
- a `[[toc]]` entry click, a `#slug` click and an outline-panel click each bring their heading to the
  top (scroll offset and heading bounds);
- the reading-progress bar grows after scrolling;
- a drag selection plus Ctrl+C, and the right-click menu's Copy, put the expected text on the
  clipboard (emoji turned, footnote marker present); Ctrl+C in a terminal still reaches the PTY;
- the Mermaid picture comes from the cache dir; PlantUML renders when `plantuml` is on PATH, else the
  run prints a SKIP line for that check only;
- `<img width=120>` draws 120 logical pixels wide.

It ends with `MARKDOWN DOCUMENTS E2E OK (…)` and leaves captures plus a JSON transcript under
`--out-dir`. A `--mutate` flag breaks one check (for example, no `sections`) and must fail.

### 6.3 Before/after captures

The same fixture, opened in the Preview, pasted as a chat reply through the fake-agent chat E2E
harness, and shown as a change-request description through the forge fixtures; captured at step 0
and after step 6 with the same display settings, compared with `Scripts/capture-diff.py`. Every
differing pair is attributed to D1–D11 or to an added capability in `differences.md` (gitignored
evidence) and summarised in the PR; an unattributed difference blocks the PR.

### 6.4 Existing suites

Every suite in §5.8, plus `test-ely-chat-ui-e2e.py`, `test-forge-e2e.sh`, `test-forge-diff-e2e.sh`,
`test-child-env-e2e.sh`, `test-prompts-e2e.sh`. A red one is compared against `main`'s run before it
is called pre-existing.

## 7. Risks

- **Gather order drift.** If the plain-text walk and the renderer's pushes disagree, chat selection
  copies the wrong text. Mitigated by the walk == pushes test over every node kind.
- **Link click after a drag.** Ely draws links with gpui `InteractiveText`, whose click may fire when
  a drag starts and ends on one link; the `on_link` guard (§5.5) owns that case.
- **Streaming cost.** Ely's reader is heavier than bezel's; a long reply re-parses per chunk. Measure
  a 40 KiB reply streamed in 200 chunks before and after; more than 2× is a finding to report, not to
  fix here (step 3 brings `StreamingMarkdown`).
- **Untrusted math.** ratex on forge text: a parse failure draws Ely's error note; a panic would be a
  finding. The E2E fixture includes malformed TeX.
- **Upstream sync cost.** L1–L8 must be re-applied on each sync; they are additive and listed.

## 8. Left to the builder

File layout inside `sirio_ui::markdown`; exact names of L1–L6 APIs (recorded); the outline column's
width and the progress bar's thickness (Ely's defaults first); how the E2E reaches each assertion
(existing debug-state methods first, a new debug-only one if needed, recorded); the converter's
handling of `expand_html` output the tree has no node for (falls back to a paragraph of its text).

## 9. Calls made while writing this (for Enzo to check)

Taken as the recommended option, per Enzo's delegation; each with what it costs if wrong.

1. **Approach A** (Sirio owns tree and selection). Wrong if a lighter integration was wanted: the
   cost is eight local changes carried on each sync.
2. **Patched `pulldown-cmark` taken** (§5.3). Wrong: one vendored crate to drop and two tests to
   re-ignore.
3. **Diagrams stay Preview-only;** chat and change requests show fences as code, as today. Wrong:
   switch `diagrams` on per surface later (one line each, but forge text would then reach the Mermaid
   renderer).
4. **Web pictures load in Preview and chat** (as bezel did), never in change requests. Wrong: one
   policy flag.
5. **Tables keep their sideways scroll bar** through L8 on every surface. Decided by Enzo via t2s
   (2026-11-20), reversing this document's first call (tables wrapping, D3): every current Preview
   feature is kept, scroll bars included, and the chat's table bar was Enzo's request (#649).
6. **Chat-only bezel syntax dropped** (D2). Wrong: a rewrite pass restores `|480` widths (L3 already
   carries width); chips and bookmark cards would need new drawing.
7. **`[[toc]]` and `[toc]` both accepted**, drawn as a list of links that jump in the Preview only.
8. **Outline panel hidden by default**, toggled from the Preview header; reading progress shown
   whenever the document scrolls.
9. **Anchors use GitHub slugs**, so links written for GitHub READMEs work.
10. **Math, alerts, footnotes and emoji everywhere**, including change requests (forge text): it is
    drawing only, with no file or network access.
11. **Preview right-click: one merged menu**, Markdown items first, file items after a separator.
    Wrong: show only one of the two depending on where the press lands.
12. **Nesting guard in Sirio** (depth 128, plain text beyond), not a vendored change. Wrong: a local
    change in Ely's reader instead.
13. **Silent parts copied** (math as TeX, alert titles, footnote labels) though not highlighted.
    Wrong: they drop out of copies, the renderer's own behaviour.
14. **Forge `file:` links stop opening** (D7), a fix of a current hole, taken without asking.
