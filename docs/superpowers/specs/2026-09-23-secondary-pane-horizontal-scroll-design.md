# Horizontal scrolling for Secondary content — design

**Date:** 2026-09-23

**Status:** approved in chat; implementation pending

## Intent and scope

The user wants a horizontal scrollbar **when content does not fit** in the second centre pane, so long lines can be reached rather than clipped. They confirmed that this applies to both text files and the Changes diff; in a side-by-side diff the left and right columns must scroll **independently**. This is content scrolling, not scrolling the tab strip. Bars disappear when the content fits.

The tab bar, empty launcher, Browser (its native webview scrolls its own page), Project Settings and Markdown preview are unchanged. No scrollbar is wrapped around the whole Secondary pane: child surfaces already own their layout, and the parent cannot recover text clipped inside them. The controls live in the affected surfaces, so Changes retains the same behavior wherever it is hosted.

## Existing rendering and constraints

- `rust/crates/sirio/src/main.rs` renders `secondary-surface` as a bounded `overflow_hidden` host. Its `render_pane_tree` mounts `FileView` and `ChangesTab` as separate full-size surfaces. Do not change pane widths, the splitter or tab strip.
- `rust/crates/sirio_ui/src/file_view.rs` uses a vertically virtualized `uniform_list` with `ListHorizontalSizingBehavior::Unconstrained`, a longest-line width reference and a `UniformListScrollHandle` whose `base_handle` already carries horizontal offset and overflow. The source view has a vertical bezel scrollbar but no visible horizontal bar.
- `rust/crates/sirio_ui/src/changes.rs` uses a single vertically virtualized `gpui::list`. Unified code lines have `text_ellipsis`; side-by-side cells pin two equal-width halves and clip each line internally. Those clips prevent an outer scrollbar from seeing or reaching the overflow. The list must remain viewport-width: making its width follow the longest line previously pushed the other column and toolbar off screen.
- `bezel::ui::scroll::scrollbar` is vertical only. Reuse its `thumb` / `offset_for_thumb` geometry and theme ink; add a small horizontal control only where needed, not a new dependency or a replacement for bezel's vertical bar.

## Content behavior

### Text file source

In `file_view.rs`, overlay a horizontal thumb at the bottom of the source-code viewport, bound to the list's existing base scroll handle (X axis). Show it only if the laid-out content width exceeds the viewport; hide it for short lines, loading/empty states and Markdown preview. Dragging updates only X and preserves the vertical offset, editor selection, caret and vertical scrollbar. The viewport height and file toolbar do not change. The bar may need a follow-up paint after first layout, as the existing vertical bar does, because scroll geometry comes from the previous frame.

### Changes diff

Keep **one** `ListState` and its vertical scroll/row virtualization. Keep the toolbar, file/section headers, numeric gutters and split separator fixed. Only the code text in a diff line moves horizontally inside a clipped code viewport. Remove the ellipsis/width cap from the scrollable code text itself; keep clipping at the viewport boundary, not at the text's intrinsic width. A long line must not change the size of the list, column, toolbar or centre pane.

- **Unified:** one X position shared by all rendered diff code lines; one horizontal bar across the available code viewport below the list. The two line numbers and change marker stay in place.
- **Split:** two X positions and two bars, one in each half's code viewport. Each half retains its own line number and change marker, and their rows remain vertically aligned. Scrolling the left code never moves the right code or either gutter.
- A bar appears only for a code region whose current rendered diff has text wider than that region. Empty, loading and git-error states have no bar. A diff without expanded code lines needs no bar.

Changes owns per-tab horizontal state (unified/left/right offsets and extents); it does not own a second vertical list. Determine each region's overflow using the loaded, expanded diff lines, **not just currently visible rows**. Measure with the existing code typography/layout so non-ASCII or variable glyph widths do not silently clip the end of a line; update extents when diff data/expansion/view mode changes and when pane width changes. Clamp offsets to the new limits, including zero when overflow disappears. A bar should not jump in length merely because the vertical viewport has scrolled to a different row. Dragging maps thumb travel to that region's X offset, with clamping at both ends; release ends the drag. The current vertical wheel and keyboard navigation remain unchanged.

The horizontal bars occupy an overlay at the bottom of their own viewport(s) and do not resize or shift the diff rows when appearing/disappearing. Use bezel's color/geometry vocabulary and existing GPUI drag conventions. Do not add a general-purpose scrollbar API unless the two call sites actually share a small implementation.

## Failure cases and verification

No persistence or database changes: horizontal positions belong to live tab views. Replaced diff data, a narrower/wider pane, a collapsed diff or a switch between Unified and Split cannot leave offsets outside their valid range. Source load failures and Git errors keep their current messages and do not draw a misleading bar.

Add drawn-frame tests in `file_view.rs` for long versus fitting text and a thumb drag reaching the end; in `changes.rs`, test fitting/long Unified lines, Split overflow on one side only, independent left/right drags and intact vertical alignment. Add/extend a shell-boundary test if needed to verify the Changes surface, toolbar and both columns remain inside the Secondary pane on a narrow window. Run targeted `cargo test -p sirio_ui` and any affected `sirio` test, plus LSP diagnostics on changed files. Do **not** run `Scripts/ci.sh` or `Scripts/ci-linux.sh` without the user's explicit request.

## Rejected alternative

Two separately virtualized vertical columns would provide native independent X scrolling but require synchronizing two vertical lists and their row heights. A single outer pane scrollbar would be smaller code but cannot expose content already clipped by `ChangesTab` and might scroll the whole layout/webview. Keep the existing list and scroll just the code regions.
