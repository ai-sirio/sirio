# D6 — Quiet chrome: the budget, the resting panel, the tab strip

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.
Last in the design series by `fable` (`05-the-design-of-the-program.md`, decisions D5/D6). It runs
**last deliberately**: the count below only means something after the composer, empty-state and
sidebar pieces have landed. Read the current code first; it has moved since this was written.

## Removal is a move this project has already made well

P19 removed drag-to-reorder rather than faking it — an affordance that did nothing, deleted
instead of defended. That is the discipline this piece runs on at window scale: waku's resting
frame shows roughly eight interactive controls; probe-2 showed ours with forty-plus. The
difference is not palette, it is chrome that never rests.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo test -p tiller_ui -p tiller_theme
./Scripts/ci-linux.sh   # strict
```

**`tiller_ui/**` and `tiller_theme/**` are yours.** Shell mount decisions you cannot reach from
`tiller_ui` (the right panel's default) go through `P43-tab-command-layer-contract.md` if the flip
is not already yours to make — check `main.rs`'s mount guard before assuming.

## The piece: `D-CHROME-01`, `D-CHROME-02`, `D-TAB-01`

- **`D-CHROME-01` — the budget, counted.** The resting chat frame — sidebar + chat tab, no panel
  summoned, no popover open — permits **at most 12 visible interactive controls window-wide**.
  The counting method is the debug-bounds map: enumerate every element with an interaction
  handler in the resting frame and **list them by selector in your report** — the list is the
  evidence and the ongoing enforcement artefact. The two title-strip toggles count. The composer
  chip row counts. If the count exceeds 12, trim: each removal names where the capability went
  (palette, context menu, overflow, popover) — capability is never deleted, only its permanent
  chrome.
- **`D-CHROME-02` — the right panel is absent at rest.** Not collapsed: **unmounted** until
  summoned (`Ctrl+Shift+I`, the title-strip toggle, the palette). Files / Activity / Changes stay
  one summon away, and the summoned panel closes again. If the default lives in the shell's mount
  logic, route the flip through the contract file rather than reaching into `main.rs`.
- **`D-TAB-01` — the strip stays quiet.** One row of 11.5px chips; close `✕` revealed on hover
  (the F-TAB-15 pattern); the overflow menu at the strip's end (P43's `OpenAllTabs` already
  exists — draw its affordance if it has none); activity conveyed by the existing glyph states,
  **never by a colour fill**; **no second row; no per-tab paths**.

## Evidence

Drawn tests (`TestAppContext` + `VisualTestContext`, `.debug_selector(id)`, real mouse events,
full `run_until_parked()` hardening):

- the resting-frame enumeration: interactive-element count ≤ 12, asserted, and the selector list
  emitted where the test failure would print it — the assertion is the budget's enforcement;
- no right-panel element in the resting frame's debug map; after the summon action it is present;
  after closing it is absent again;
- tab `✕` absent from a tab's bounds at rest, present under hover, and a real click on it routes
  through the shell's close door (dirty-close prompt intact);
- the overflow affordance exists at the strip's end when tabs overflow, and dispatches
  `OpenAllTabs`;
- no tab element renders path text; no tab draws an activity colour fill (absence assertions).

Appearance — the strip's visual weight, the resting frame against waku — is display-debt;
`SHOT-LIST.md` schedules `chat-empty` and `chat-done` frames for it. Claim behaviour; never
pixels.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- Update `D-CHROME-01/02` and `D-TAB-01` in `DESIGN-LEDGER.md` as **`builder-claimed,
  unverified` — never `PASSED`**. A brief that later adds an always-visible control must name
  what it displaced — put that sentence in the ledger row's evidence cell so it survives you.
- Keep the gate green. Proceed without asking for design approval; this brief is the design.

## Reporting

Reply in **12 lines or fewer**: the final count and the full selector list, what you trimmed and
where each capability went, the panel's resting default and summon proof, the strip assertions by
test name, the gate result, and the honest remainder.
