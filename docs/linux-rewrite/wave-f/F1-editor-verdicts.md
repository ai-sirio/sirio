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

## `F-EDIT-10` — ledger line 228 — **FAILED — defective**

Right-clicked the `README.md` row in the Files panel (`/tmp/f1editor-proj`) at its real screen
coordinates and forced a repaint before every capture (trap 2). Across three independent attempts
(`manual-rclick2.png`, `manual-rclick3.png`/`zoom-artifact2.png`, `manual-rclick4.png`) the Open/Reveal
in File Manager/Copy Path menu never renders as a usable control: one attempt drew nothing at all at a
forced-fresh frame, another drew only a ~30px sliver of a rounded, unlabeled shape wedged against the
Files-panel divider — no "Open"/"Reveal"/"Copy Path" text visible anywhere, nowhere near the actual
click point. A blind click inside that sliver (`manual-blindclick.png`) landed on nothing (the menu
closed via `on_mouse_down_out`, no state change). This is the "declared path passes for a control
nobody can reach" failure pattern: `right_panel.rs`'s own headless test (`Copy Path writes the absolute
file path`, ~line 1787) passes because it clicks `cx.debug_bounds("file-context-copy-path")` — the
*true* GPUI layout bounds, known internally — not the pixel position a real user's right-click would
produce. That test proves the data plumbing works; it does not prove the on-screen render is at a
place a human can click, and live re-drive shows it currently is not. The original PASSED evidence's own
caveat ("renders at panel top, not at the pointer") undersold this — it is now unusable, not just
misplaced.

## `F-EDIT-11` — ledger line 229 — **FAILED — defective**

Depends on the same context menu as `F-EDIT-10`. Right-clicked `README.md`, forced a repaint, and
clicked where "Copy Path" should sit inside the visible sliver; read the clipboard immediately after
with a purpose-built Wayland `wl_data_device` client (`/tmp/wl-clip-read.c`, written for this pass since
`xclip`/`wl-paste` are not installed and the app's clipboard on this Wayland lane goes through
`smithay-clipboard`, not X11 selections — `xclip` cannot see it). Result: `FAIL: no selection after
timeout` — nothing was ever written, confirming the click did not land on the real "Copy Path" hitbox.
The row cannot currently be exercised through the interaction it describes (right-click → Copy Path);
the original evidence's `xclip`-based read-back was almost certainly captured on the X11 lane
(`DISPLAY=:1`), not this one, and is not reproducible here regardless of lane.
