# PI HANDOFF — the visual language, the file map, and what comes next

Written at the end of P9 (sidebar selection), for the builder who continues the UI work.
This is the knowledge that lived in the previous builder's context: the theme vocabulary, the
scales and their rationale, where everything lives, and what remains.

## 1. The theme token vocabulary (`rust/crates/tiller_theme/src/lib.rs`)

The palette is waku's measured values (docs/linux-rewrite/03-visual-bar-and-gpui-patterns.md §A.2),
two palettes (`Theme::dark()` / `Theme::light()`), one `Theme` struct installed as a GPUI global
(`ActiveWakuTheme`-style; `Theme::get(cx)` reads it, `Theme::init(cx)` installs). All colors are
`Rgba`; the hsla→rgb helper `hsla(h,s,l,a)` takes degrees and is unit-tested.

Role names (both palettes, dark/light):
- **Surfaces**: `canvas` #1A1A1A/#F6F5F6 · `background` (same — panel chrome) · `sidebar`
  #181818/#F3F3F3 (opaque choice for Linux — waku is transparent over macOS vibrancy) ·
  `raised` #232323/#ECECEC (cards, chips, popovers) · `composer` #212121/#FFFFFF ·
  `inset` #151515/#E6E6E6 (fields, code wells) · `terminal_surface` #151515/#FFFFFF.
- **Lines**: `hairline` = waku border hsla(220,10%,90%,0.07)/0.08 · `border_strong` 0.14/0.15 ·
  `sidebar_border` #292929-ish / black 12%.
- **Text**: `title` #E2E2E2/#242424 · `title_selected` #F5F5F5/#101010 · `subtitle`
  #A3A3A3/#666666 · `meta` #7D7D7D/#858585 · `text_ghost` #575757/#A4A4A4.
- **Meaning**: `accent` (coral, rare — focus borders, links) #E2795B/#C85F44 · `gauge`
  #3B82F6/#2563EB (meters) · `selection` hsla(211,100%,50%,0.55)/0.35 (text selection ONLY) ·
  `selected_fill` = 6% neutral (selected ROWS — never blue) · `warning/success/danger` +
  `danger_soft` · `code_text` #E0A882/#9A5528 + `code_wash` (inline code) · `inverse/on_inverse`
  (primary buttons) · `favorite`.
- Legacy names kept as aliases: `tab_focus_accent`=accent, `tab_done`=success,
  `tab_needs_input`=warning, `tab_error`=danger, `card_fill`=raised, `filter_field_bg`=inset,
  `git_*`=semantic, `file_link`=gauge, `rail_*` (old app's card rails, kept).

The golden rule (the critic's, quoted): **color is reserved for meaning; selected/hover/pressed
rows are a 6% neutral layer; coral appears on inline code, links, and almost nothing else.**

## 2. Type, radius, density scales and why

- **Type**: body 13.5px @ 21px line-height (~1.56, the "reads like a document" ratio); UI chrome
  11.5px @ 16px; callout 12.5; caption 10.5; code 11.5 @ 17.5 (JetBrains Mono in waku; Tiller's
  code spans still say `SFMono-Regular` — a macOS family, flagged for a fonts pass; on Linux it
  falls back via fontconfig). Headings scale off body: h1 ×1.45, h2 ×1.28, h3 ×1.14, h4 ×1.05,
  line-height ×1.42. Tokens in `Typography` (theme crate).
- **Radius**: 4 (chips-in-rows) / 6 (default controls) / 7 (row cards) / 8 (code blocks, tool
  cards) / 12 (user pill, menus) / 13 (composer card) / full (pills, send, dots).
- **Density**: 48px app bars (titlebar, header); sidebar single-line rows 32px, two-line cards
  51px (7+18+4+15+7, waku's session-card math); file-tree rows 30px; diff lines 20px; hunk
  headers 24px; content column 720px. Rows are `flex_none()` in scroll containers or Taffy
  shrinks them (a real bug I hit twice).
- **Why these numbers**: they are waku's measured values, not invented — the critic judged every
  frame against waku's own screenshots pixel-measured, and the passes came after these scales
  landed. 13.5/21 is the document ratio; 11.5 chrome vs 13.5 body is the hierarchy step that
  replaced boxes-and-color hierarchy.

## 3. File map (rust/crates/tiller_ui/src)

- `theme` (crate tiller_theme): all tokens + the dark-mode portal logic (ashpd, Linux-only,
  dark fallback; skips in test harnesses by thread name).
- `sidebar.rs`: project/worktree/tab tree. Row model `SidebarRow`, two-line cards, hover-revealed
  chrome (chevron, close), host-driven tab rows via `set_worktree_tabs`. P9 (this piece) added
  `SidebarEvent::SelectWorktree(PathBuf)` + `set_selected_worktree` — the host owns selection.
- `chat.rs`: transcript + composer (the P8 surface). User turn = right-aligned raised pill
  (max 540, r12); assistant = plain text; turn footer = centered hairline with time; composer =
  raised card with labelled chips (status, Model, context ring) + circular send; context row
  below the card.
- `changes.rs`: the full-width Changes tab (P13). Own git state, 1s poll, context bands
  (`CONTEXT_BAND_MIN = 4`, `CHANGES_CONTEXT_LINES = 24`), OpenFile event. Dev harness binary:
  `src/bin/changes_preview.rs`.
- `right_panel.rs`: Files tree + activity only (the Changes half moved out). Files toolbar shows
  the repo path; tree refresh = status→changed paths→walk.
- `titlebar.rs` (48px, 2 controls), `tab_bar.rs` (+ menu), `status_bar.rs` (gear + provider
  segments, no refresh button), `controls.rs` (settings primitives), `settings.rs` (shared with
  codex12 — persistence wiring in flight), `file_view.rs` (standalone file tab).

## 4. What remains on the sidebar (P9's second half — shell wiring)

The tiller_ui half is done and tested (event + host-confirmed selection). The shell (main.rs,
codex12's) still needs:
1. `SidebarEvent::SelectWorktree(path)` → a `workspace.select_worktree(path, cx)` that updates the
   control state's current workspace (the source of `tillerctl current-workspace`), re-derives the
   status bar's branch/path, re-homes the open tabs under the right worktree, and answers the
   sidebar with `set_selected_worktree(path, cx)` so the highlight is host-confirmed.
2. The `+` add-project affordance and drag-reorder have no handlers — the sidebar's `start_add_project`
   path exists but the host action is unverified; drag-reorder was never built.

## 5. Capture recipe (already fixed in Scripts/linux-shot.sh)

Xwayland :1 (`-ac -noreset -nolisten tcp`) → `xrandr --output XWAYLAND0 --mode 1920x1080` (the
missing step — the root comes up 640x480 otherwise) → app with `GPUI_X11_SCALE_FACTOR=1` →
`import -window root` + crop at the window's `+X+Y` (`import -window <id>` is always black on this
Xwayland). App launches die if `pkill` runs in the same command as the launch (the pkill matches
the harness's own wrapper) — separate the commands, or use `pkill -f "[t]arget/..."`.

## 6. The critic's axes (how every piece was judged)

Density and type scale before palette; chrome budget (every always-visible control must justify
itself; hover-reveal or delete); accent rarity; rows from the scale, not from habit. The measured
verdicts: P1 dark-mode+palette ✓, P5 proportions+chrome ✓, P8 chat surface ✓, P12 inspector ✓,
P13 changes tab ✓ — each judged against waku's screenshots with exact srgb measurements.

## 7. Display state (P23, ~14:00)

All displays are dead: `:1` died at 09:56, `:2` is wedged (Xwayland alive but xdpyinfo from a fresh
client times out; killing the app did not recover it), and freshly created Xwaylands do not present
(compositor cannot import buffers — see Scripts/linux-shot.sh for the measured history). Work that
needs pixels is NOT EXERCISED, not done: run the app headless instead
(`TILLER_SOCKET=/tmp/<name>.sock env -u DISPLAY -u WAYLAND_DISPLAY rust/target/debug/tiller`) —
the control socket serves real state. The fix is a compositor restart, operator's call.
