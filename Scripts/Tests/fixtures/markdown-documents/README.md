# markdown-documents fixture

`showcase.md` is the one document every Markdown surface is compared on. Task 0 of the
"Markdown on ely-documents" plan recorded how the removed bezel-markdown drew it; `capture-markdown-surfaces.sh`
opens it in the file Preview, as a chat reply and as a change request description.
`pixel.png` is a 64x64 solid magenta (250, 40, 190) PNG, a colour the app's palette never draws.

`{{REMOTE_PORT}}` in the remote image address is replaced by the script with the port of its loopback server.

| Section (in order) | What it exercises |
| --- | --- |
| H1, two H2 titled "Second section" | heading slugs: the second one is `second-section-1` |
| `[[toc]]` | the table of contents block |
| `[second section](#second-section)` | an in-document anchor link |
| paragraph | bold, emphasis, strikethrough, inline code, a bare URL, emoji shortcodes |
| inline and display math | `$...$`, `$$...$$` and a malformed `$$\frac{1}{$$` that must not break the page |
| five alerts | NOTE, TIP, IMPORTANT, WARNING, CAUTION |
| footnote | a reference, its definition, and a link wrapped around a reference |
| task list | checked and unchecked items |
| 9-column table | wider than 900 px, so it needs its horizontal scroll bar |
| `rust` fence | one line of 200+ columns, the code block's horizontal scroll |
| `mermaid`, `plantuml` fences | diagrams (PlantUML draws a note when `plantuml` is not installed) |
| README HTML | a centred `<img>` and a `<details>` block |
| `![local](pixel.png)`, `![remote](http://127.0.0.1:{{REMOTE_PORT}}/pixel.png)` | a picture beside the file and one over loopback HTTP |
| 60 numbered lines | a tail long enough that every surface scrolls |
