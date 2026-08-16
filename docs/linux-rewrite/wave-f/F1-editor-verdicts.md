# Wave F slice F1-editor — re-verification verdicts

Independent live re-drive of the 4 PASSED rows in `F1-editor.md`, per
`docs/linux-rewrite/tasks/P125-rows-without-independent-provenance.md`. Instrument:
`Scripts/wayland-drive.sh` (labels `f1editor01`/`f1editor02`/`f1chg01`/`f1chg03`) against a freshly
built `rust/target/debug/tiller` at HEAD (`559d410`), driving throwaway git repos created for this pass
(`/tmp/f1editor-proj`, `/tmp/f1chg-proj`). Every click below was calibrated by reading pixel coordinates
from a screenshot taken at the *same* resolution the click was then sent at — `wayland-drive.sh`'s
`shot()` alternates the output size on every call (including the implicit baseline shot before the
action block even starts), so a coordinate read from one frame and clicked after an intervening `shot`
silently lands somewhere else in the reflowed layout. This cost a full false-negative cycle on
`F-EDIT-10`/`F-EDIT-11` below before the methodology was corrected — recorded here because the same trap
is likely to catch the next critic who free-hand-composes a multi-step action block.

## `F-EDIT-07` — ledger line 225 — **FAILED — defective**

Opened `/tmp/f1editor-proj/README.md` (a fresh file with an inline code span and a fenced ` ```bash `
block containing a `#` comment line and an `echo "..."` command line) in Preview mode. Two of the three
recorded claims hold: the "Markdown" language badge renders in the header, and the inline code span
gets its own highlighted color/background. The third does not: the fenced block's comment line and
command line render in the exact same flat off-white — no keyword/comment/string coloring inside the
fenced block at all (confirmed at 4x crop, zoomed pixel-by-pixel — every character in both lines is the
identical shade). Static read confirms why: `Chat::render_markdown_block`'s `CodeBlock` arm
(`chat.rs:3554`) renders fenced content through `render_plain_text` (`chat.rs:3683`), which applies zero
`HighlightStyle`s — unlike `Inline::Code` (`chat.rs:6982`), which does. `file_view.rs`'s per-token
`code_spans` highlighter only ever fires for the raw-source Code view of a non-Markdown-language file
(`file_view.rs:1353`) and is never invoked by the Preview/fenced-block render path. The recorded evidence
description does not match what the app draws.

## `F-EDIT-10` — ledger line 228 — **PASSED**

First pass produced a false `FAILED — defective` from a self-inflicted coordinate bug (see header note)
— right-clicks landed in the empty terminal pane, not on the file row, because coordinates read from a
1715-wide frame were sent while the live output was 1400-wide (or vice versa) after an intervening
`shot()` flipped it. Redone with coordinates read and used within the same resolution: right-clicked
`README.md` at its true position and forced a repaint. The Open / Reveal in File Manager / Copy Path
menu draws correctly, fully legible, anchored right next to the pointer (`recalib-rclick.png`). Clicking
"Open" (`recalib-open.png`) closed the menu and left `README.md` open in the editor — Open genuinely
opens the file. The originally-recorded "renders at panel top, not the pointer" defect note does not
reproduce either; the menu now anchors at the click point (`right_panel.rs:437`,
`anchored().position(position).snap_to_window()`). `heldUp: true` — independently re-verified, matches
the prior evidence.

## `F-EDIT-11` — ledger line 229 — **PASSED**

Also false-negatived on the first pass for the same reason as `F-EDIT-10`. Redone with resolution-matched
coordinates: right-clicked `README.md`, clicked "Copy Path" at its true drawn position
(`recalib-rclick2.png`), then read the clipboard with a purpose-built Wayland client using the wlroots
data-control protocol (`/tmp/wl-clip-read2.c`, `zwlr_data_control_manager_v1` — the same
focus-independent protocol `wl-paste` uses; the app's clipboard on this Wayland lane goes through
`smithay-clipboard`, and a plain `wl_data_device` reader only receives selection events while holding
keyboard focus, which a headless CLI reader never has — that gap produced a second false negative
mid-pass before switching protocols). Result: `/tmp/f1editor-proj/README.md` — the exact absolute path of
the file actually clicked, read back independently while the app was still running. `heldUp: true`.

## `F-CHG-16` — ledger line 207 — **PASSED**

Built a second throwaway repo at `/tmp/f1chg-proj` with a genuine unresolved `UU` merge conflict (two
branches editing the same line of `conflict.txt`, merged with `git merge --no-commit` left mid-conflict:
`<<<<<<< HEAD` / branch B's line / `=======` / branch A's line / `>>>>>>> branch-a`). Opened the Changes
tab; `conflict.txt` drew in both Staged and Changed with conflict-red icon styling. Clicking the row
(not the toolbar's "Expand All", which did not visibly respond to clicks in this pass and is unexplored
beyond that) expanded it in place, showing the real conflict-marker diff and a "Resolve in terminal"
action alongside Discard/Stage/Open diff (`manual-row-click.png`). Clicking "Resolve in terminal" opened
and focused a new "Resolve conflict.txt" tab whose terminal showed `diff --cc conflict.txt` with the
actual conflict content, followed by "Resolve conflict at conflict.txt" and a live shell prompt
(`manual-resolve-click.png`) — matching `ChangesTabActionEvent::ResolveInTerminal(PathBuf)`
(`changes.rs:88`) wiring through to `TerminalView::for_conflict`. Independently re-driven this pass
against a conflict I created myself, not read off `P104-report`. `heldUp: true`.
