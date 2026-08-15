# Wave D integration report

Base: `5626555`. Branch: `linux/gpui-waku`. Worktree:
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`.

This session picked up mid-wave: an earlier integrator pass had already applied several of the
`wantedForeignFiles` fixes below (commits `71cb625`, `6eb7e00`, `7517594`, `f7a7189`, `fb86099`,
`329a44e`, `e7d24f4`) and run out of turns before finishing item 4, the cross-slice sweep, or this
report. This report covers the full scope: it re-verifies everything already landed and completes
what was left.

## 1. Build

`cargo build -p tiller` — green. One pre-existing, unrelated warning (`tiller_ui/src/browser.rs`,
`pump_task` field never read; not introduced this wave).

## 2. Tests, per crate (never `--workspace`)

| Crate | Result |
|---|---|
| `tiller` | 143/143 passed |
| `tiller_ui` (`--lib`) | 304/304 passed |
| `tiller_terminal` (`--lib`) | 36/36 passed |
| `tiller_acp` (`--lib`) | 22/22 passed, 1 ignored |
| `tiller_agents` (`--lib`) | 14/14 passed |
| `tiller_usage` | 11/11 passed (10 in `usage_tests.rs` + 1 in `p99_codex_locations.rs`) |

Only crates with files touched since `5626555` were run. No flakes observed in any of the above
run alone.

## 3. Cross-slice sweep

Files touched since `5626555` (via `git diff --name-only 5626555..HEAD -- rust/`):

`main.rs`, `session.rs`, `tab_machinery.rs`, `command_palette.rs` (crate `tiller`); `sidebar.rs`,
`tab_bar.rs`, `settings.rs`, `changes.rs`, `browser.rs`, `chat.rs`, `right_panel.rs`, `titlebar.rs`
(crate `tiller_ui`); `tiller_terminal/src/lib.rs`; `tiller_agents/src/lib.rs`;
`tiller_usage/{claude,codex}.rs`; `tiller_acp/{lib,chat}.rs`; `Cargo.lock`,
`tiller_acp/Cargo.toml`, `tiller/Cargo.toml`.

**Method**: for `main.rs` (touched by all 8 `D-MAIN-*` slices plus the integrator — the highest
collision risk), every commit's row-id tag was cross-referenced against `manifest.json`'s
`ids -> slice` map programmatically. Every id resolved to exactly one owning slice and every
commit's diff was consistent with that slice's own report (or, for `71cb625`/`02fd406`/`6eb7e00`/
`7517594`/`f7a7189`/`fb86099`/`329a44e`, explicitly an integrator commit applying that slice's
documented `wantedForeignFiles` patch). Two `main.rs` commits (`78b2b21`, `6951b1e`) carry no F-id
in the subject; both are individually named and described in `D-MAIN-5-report.md` ("commit
78b2b21", "commit 6951b1e") for `F-SET-18`/`F-SET-04`, so both attribute cleanly. **No commit in
this wave could not be attributed** (contrast with wave C's four unattributed `main.rs` commits).

**Deletion-pattern check** (the wave-B failure mode: a big, unrelated deletion hiding inside a
commit whose subject is about something else): ran `git show --numstat` per commit for every
touched file and flagged any commit whose deletions were large relative to its additions. Every
`main.rs` commit is addition-heavy (worst ratios: `cd6eec7` +39/-17, `274cd28` +30/-14 — both
inspected in full: `cd6eec7` renames `split_terminal_at` into
`split_focused_terminal_with_placement`, deleting only the old function's now-redundant body;
`274cd28` deletes two now-obsolete "still unsupported" test-list entries it also demonstrates are
now supported two lines above — both are ordinary refactors, not reverts). `sidebar.rs`'s four
commits are all net-additive (155/2, 129/5, 97/4, 43/1). Ran the same numstat check across every
other touched file; none showed a deletion-heavy commit. Separately, extracted every `F-`-id
referenced in an added line versus a removed line across `main.rs`'s full wave-D diff — the
removed set is a strict subset of the added set, i.e. no row's marker comment was deleted without
being re-added at its new location. **No genuine revert found anywhere in this wave.**

## 4. Foreign-file fixes applied

Working through the orchestrator's list; six items were already landed by the prior integrator
pass, verified correct against their originating report and left as-is; two were completed this
session.

- **D-P3 -> `right_panel.rs`** (`F-CORE-FILE-03`, commit `329a44e`, pre-existing): drag source
  wired onto Files-panel rows, matching D-P3's exact recipe. Verified.
- **D-P2 -> `main.rs`** (`F-SET-20`, commit `71cb625`, pre-existing): `settings_report_pairs` now
  copies `translucency` into the control-socket payload. Verified.
- **D-P1 -> `main.rs`** (`F-CHAT-14`, commit `71cb625`, pre-existing): `restore_launch_snapshot`
  now binds `ChatEvent::OpenFile` on every restored chat tab. Verified.
- **D-P1 -> `sidebar.rs`** (`F-PRJ-06`/`F-PRJ-09`): **not applied.** D-P1's report is diagnostic
  only ("I could not identify the exact line... not proposing an unverified patch") — no patch was
  described to apply. Left open.
- **D-MAIN-2 -> `changes.rs`** (`F-CHG-02`, `F-CHG-13`, commit `fb86099`, pre-existing): empty-state
  message plus `ChangesTab::focus_path`, matching D-MAIN-2's exact recipe. Verified.
- **D-MAIN-3 -> `browser.rs`**: two separate asks under one row family.
  - `F-CTRL-BROWSER-05` (commit `f7a7189`, pre-existing): resolved *without* touching `browser.rs`
    — `F-CTRL-BROWSER-06` (a later D-MAIN-4 commit) had already made
    `BrowserSurface::evaluate_script` public, so `main.rs`'s `browser.act` arm calls it directly.
    Smaller than D-MAIN-3's proposed patch; verified equivalent behavior.
  - `F-CTRL-BROWSER-04` (screenshot/DOM snapshot): **not applied.** D-MAIN-3's own report calls
    this "genuinely out of scope... not attempted rather than faked" — it needs a real design
    choice among unverified `webkit2gtk`/GTK APIs (`webkit_web_view_get_snapshot` vs. a raw
    `gdk::Window` pixbuf grab), not a described patch. Left open.
- **D-MAIN-5 -> `tiller_persistence/src/model.rs`**: **not applied — optional by design.**
  D-MAIN-5's own report says `wantedForeignFiles: none needed`; the row (`F-SET-22`) is already
  fully fixed via a session-refs KV-table workaround in `session.rs`. The house-rules note flags a
  real `AppSettings.agent_colors` column as a follow-up *replacement* for that workaround, not a
  gap in the shipped fix. Left as a documented future improvement, not attempted.
- **D-MAIN-6 -> `sidebar.rs`**: two rows.
  - `F-SID-15` (commit `6eb7e00`, pre-existing): confirm-gated worktree removal with a real
    "Remove Worktree" menu entry. Verified.
  - `F-SID-11` part 2 (commit `02fd406`, **completed this session**): `SidebarWorktree`/
    `SidebarRow` gained `comment: Option<String>`; `main.rs` gained
    `sidebar_projects_with_comments`, reading the durable `worktree.comment` control-state keyed
    by path (same source the status bar already reads); the worktree row now renders it. This
    diff was sitting uncommitted in the working tree when this session started — verified it
    builds and `cargo test -p tiller` stays green (143/143), then committed with explicit paths.
- **D-MAIN-6 -> `session.rs`** (`F-SID-11` part 1, commit `e7d24f4`, pre-existing): synthesizes a
  primary worktree row for non-git folder projects, with a bare-repo exclusion check. Verified.
- **D-MAIN-6 -> `right_panel.rs`** (`F-TAB-01`): **not applied.** D-MAIN-6's report reproduced the
  click-misrouting-after-collapse bug but explicitly could not root-cause it ("I could not
  root-cause this from source alone... this needs a same-size in-lane repro... I did not have
  budget left") and flagged a lane-repaint confound as a live alternative explanation. No patch was
  described. Left open.
- **D-MAIN-6 -> `tab_bar.rs`** (`F-TAB-08`, commit `7517594`, pre-existing): `TabBar` gained
  `on_open_agent_settings`; `render_chat_empty` now takes `entity: Entity<Self>` and wires
  `.on_click`. Verified.
- **D-MAIN-6 -> `settings.rs`** (`F-TAB-08`): **turned out unnecessary.** The `main.rs` handler for
  the new `WorkspaceAction::OpenAgentSettings` calls the already-existing
  `workspace.open_settings(Some(SettingsCategory::Agents), cx)` — no `settings.rs` change was
  needed to reach the Agents section. D-MAIN-6's report had tentatively guessed a `settings.rs`
  change might be needed; it wasn't.
- **D-MAIN-6 -> `context_menu.rs` / `tiller_terminal::lib.rs` / `panes.rs`** (`F-TAB-11`): **not
  applied.** D-MAIN-6's own report is explicit that this is "a real, multi-file feature — not a
  one-liner" and says it deliberately did not attempt a partial version ("a half-threaded
  eligibility parameter with no real geometry check behind it would be worse than the honest
  absence"). Confirmed the pieces it named are still present and still disconnected: `panes.rs`
  already has a pure, `#[allow(dead_code)]`-annotated `split_disabled_reason(direction, size,
  tab_count) -> Option<SplitDisabledReason>` with zero callers (predates this wave — no commit has
  touched `panes.rs` since `5626555`); `context_menu.rs`'s `TerminalContextItem` still has exactly
  three fields (`label`, `action`, `route`), no `enabled`/`disabled_reason`; `ITEMS` is still a
  flat compile-time `const`. Wiring pane geometry into `TerminalView`'s render path (a
  `tiller_terminal` type with no notion of pane/tab-group state today) needs a genuine new field
  and an update call site in `main.rs` at whatever point learns a pane's rendered size — a design
  decision, not a described patch. Given the row's own author, with focused context on just this
  row, judged a half-implementation worse than no implementation, the integrator made the same
  call rather than force it under a broader, more time-constrained mandate. Left open.

## 5. Binary

Rebuilt after the last commit (`02fd406`). Current binary at
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust/target/debug/tiller` (271 MB, ELF
x86-64, matches `git log -1` at commit time — working tree is clean).

## Summary of what's still open for a future wave

- `F-PRJ-06`/`F-PRJ-09`: sidebar overlay `on_click` down/up mismatch — needs a live-instrumented
  pass on `sidebar.rs`, not described here.
- `F-CTRL-BROWSER-04`: `browser.screenshot`/`browser.snapshot` — needs a `webkit2gtk`/GTK design
  decision in `browser.rs`.
- `F-SET-22` follow-up (optional): replace the session-refs KV workaround with a real
  `AppSettings.agent_colors` column in `tiller_persistence`.
- `F-TAB-01`: click-after-collapse misrouting in the Files panel — root cause still unconfirmed,
  possibly a lane-repaint artifact rather than an app bug.
- `F-TAB-11`: terminal split context-menu items never reflect real split eligibility
  (`panes.rs::split_disabled_reason` exists, unused, predates this wave) — a genuine multi-file
  feature, not a one-line wire-up.
