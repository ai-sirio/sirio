# D5 — The sidebar is a session list, not a filesystem

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.
Fifth in the design series by `fable` (`05-the-design-of-the-program.md`, decision D1). Code and
ledger outrank this file; several D-pieces landed after it was written — read the current
`sidebar.rs` first.

## The tokens landed; this piece is the composition they were for

The last whole-app frame (probe-2, 10:25) showed your theme work holding: the graphite ground, the
row proportions, the neutral selection are genuinely waku's. The same frame showed the composition
is still the old app's — a disclosure tree repeating the same grey `/home/…` path subtitle ten
times in one viewport. Colours changed; information per row did not. This piece finishes what the
tokens started, structurally.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo test -p tiller_ui -p tiller_theme
./Scripts/ci-linux.sh   # strict
```

**`tiller_ui/**` and `tiller_theme/**` are yours.** The context menus and typed sidebar actions
are already contracted in `P43-tab-command-layer-contract.md`; nothing here changes them.

## The piece: `D-SID-01/02/03` — a row is a worktree session

- **`D-SID-01` — the row anatomy.** 51px two-line card: line one, the **branch name at 13.5px**;
  line two at 11.5px, a **project chip · activity state · relative time**. The project's colour
  identity lives in the chip's tint and nowhere else on the row — activity stays glyph-based,
  never a colour fill. Activity comes from the existing read-only status methods (the shell's
  activity wiring); do not invent a second status source.
- **`D-SID-02` — no path text in rows, ever.** Paths move to the row tooltip and the status bar
  (which already shows `worktree · path`). This is an absence claim and drawn frames prove
  absence: across a populated fixture, no sidebar row element contains a `/`-path string.
- **`D-SID-03` — projects are group headers**, in waku's list-section position — **not disclosure
  parents**. Open tabs stop nesting under worktrees; the tab strip owns tabs, the worktree row
  keeps only its activity glyph. **`+ New Worktree` is the sidebar's one primary CTA above the
  fold**, and selection is the existing 6% neutral fill token.
- **Relative time** needs a last-activity timestamp. If no reachable state carries one, render
  the slot empty, name the missing value in your report, and add the need to the contract file —
  do not fake a clock and do not block the piece on it.

## Evidence

Drawn tests over a populated fixture — two projects, three-plus worktrees, one branch name long
enough to truncate (`TestAppContext` + `VisualTestContext`, `.debug_selector(id)`, real clicks,
full `run_until_parked()` hardening):

- a worktree row contains branch, project chip, and time slot; **no element in any row renders
  text containing a path separator**;
- project headers are not disclosure parents: no expand/collapse control in their debug bounds,
  and worktree rows are present without interacting with the header;
- no tab rows exist under worktree rows;
- exactly one `+ New Worktree` CTA in the sidebar's resting frame, and a real click on it
  dispatches the existing typed create path;
- selection still applies the 6% fill token on click (the F-SID-05 regression trap).

Appearance — the 51px rhythm as rendered, chip tinting, truncation, the waku list comparison — is
display-debt; `SHOT-LIST.md` schedules `sidebar-populated` for it. Claim behaviour; never pixels.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  waku's session list is the reference composition; take the anatomy, write the lines.
- Update `D-SID-01/02/03` in `DESIGN-LEDGER.md` and any F-SID rows you touch in
  `INVENTORY-LEDGER.md` as **`builder-claimed, unverified` — never `PASSED`**.
- Keep the gate green. Proceed without asking for design approval; this brief is the design.

## Reporting

Reply in **12 lines or fewer**: the row anatomy as built, the path-absence proof by test name,
what happened to tab nesting and the disclosure tree, where relative time came from (or what is
missing), the gate result, and the honest remainder.
