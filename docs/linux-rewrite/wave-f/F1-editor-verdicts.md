# Wave F slice F1-editor — re-verification verdicts

Independent live re-drive of the 4 PASSED rows in `F1-editor.md`, per
`docs/linux-rewrite/tasks/P125-rows-without-independent-provenance.md`. Instrument: `Scripts/wayland-drive.sh`
(label `f1editor01`) against a freshly built `rust/target/debug/tiller` at HEAD (`559d410`), driving a
throwaway git repo at `/tmp/f1editor-proj` created for this pass.

## `F-EDIT-07` — ledger line 225 — **FAILED — defective**

Opened `/tmp/f1editor-proj/README.md` (a fresh file containing an inline code span and a fenced
` ```bash ` block with a `#` comment line and an `echo "..."` command line) in Preview mode.
Frame `03-readme-double-clicked.png` (cropped 4x in `zoom-codeblock.png`) confirms two of the three
claims and refutes the third: the "Markdown" language badge renders in the header, and the inline
code span (`` `inline code span` ``) gets its own highlighted color/background — both correct. But the
fenced block's comment line and command line render in the exact same flat off-white — no keyword,
comment, or string-literal coloring inside the fenced block at all, contradicting the recorded "comments
coloured apart from commands". Static read confirms why: `Chat::render_markdown_block`'s `CodeBlock` arm
(`chat.rs:3554`) renders fenced content through `render_plain_text` (`chat.rs:3683`), which applies zero
`HighlightStyle`s — unlike `Inline::Code` (`chat.rs:6982`), which does. `file_view.rs`'s per-token
`code_spans` highlighter (the thing that *would* color comments vs commands) only ever fires for the
raw-source Code view of a non-Markdown-language file (its own doc comment says so, `file_view.rs:1353`)
and is never invoked by the Preview/fenced-block render path. The originally-recorded evidence
description does not match what the app actually draws.
