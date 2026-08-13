# The construction backlog, cut into dispatchable pieces (FABLE-11)

This doc cuts every truly-absent ledger row into pieces sized for one builder and one critic
verdict. It changes no verdicts and edits no source. Owners are derived from the files a piece
must touch, under the ownership map current tonight — not from the `F-` prefix and not from old
briefs.

## Derivation — measured at cut time, not copied from the brief

The ledger moved while this was written; every number below was re-measured against the live
`INVENTORY-LEDGER.md`, and the census cross-checked against its own declared counts
(`STALE-FAILED-CENSUS.md`: 141 rows, **39 built** — parsed 141, parsed 39, both match).

- Live `FAILED — absent`: **127** (the brief said 131). Live `FAILED — defective`: **10** (the
  brief said 8). Pass 16 is consuming the FABLE-09/10 recipes in real time.
- Of the census's 39 built rows, **14 have already left** `FAILED — absent` (8 PASSED,
  5 half-proven, 1 now defective: `F-SET-09`). The remaining **25 built rows still sit in the
  127** — they need the critic, not a builder, and are **not in this cut**.
- Two census-absent rows also left without construction: `F-SET-08` and `F-TAB-09`, both PASSED —
  the census's class-3 (symptom-stated-as-cause) reading of them was vindicated.
- **Target = 127 − 25 = 102 rows**: 100 census-absent survivors plus two rows born after the pin
  (`F-AGENT-OMP-03`, `F-AGENT-OPENCODE-03`). The brief's arithmetic (131 − 39 = 92) assumed the
  39 were all still inside the 131; fourteen already weren't.

The cut: **102 rows → 52 construction pieces (99 rows) + 3 no-piece rows**, plus **4 defect
pieces** covering the 10 defective rows (4 of those 10 are absorbed by construction pieces).

## Ownership map used (current tonight) — and the drift found

| owner | files |
|---|---|
| `pi` | `chat.rs`, `sidebar.rs`, `status_bar.rs` |
| `codex11` | `changes.rs`, `right_panel.rs`, `editor.rs`, `file_view.rs`, `browser.rs`, `tiller_git/**`, `tiller_terminal/**`, `tiller_acp/**`, `tiller_agents/**` |
| `codex12` | `main.rs`, `session.rs`, `tiller_control/**`, `tab_bar.rs` |
| `sonnet` | `titlebar.rs`, `controls.rs`, `composer.rs`, `settings.rs`, `icons.rs`, `tiller_theme/**` |
| unowned | `tiller_project/**`, `tiller_usage/**`, `tiller_markdown/**`, `tiller_persistence/**` |

Drift between briefs/reality and this map, found while cutting — each is a two-agents-one-file
accident waiting:

1. **`settings.rs` = `sonnet`** (P75). Stale in P69, P68, P62, P18 (all say `pi`; P16 even says
   `codex12`).
2. **P69 hands `pi` `tiller_agents/**`** ("Yours: chat.rs, settings.rs, sidebar.rs,
   status_bar.rs, tiller_agents/**") — the map says `codex11`. P69 is exactly the
   questions/auth territory of pieces B-04/B-10 below; the halves split differently depending on
   who is right. Needs an orchestrator ruling before either piece dispatches.
3. **`tiller_persistence` is unowned in the map, but P58 — a persistence job — is `codex11`'s
   standing handoff.** Either the map gains a line or P58 is an acknowledged exception.
4. **`composer.rs` is `sonnet`'s, but the chat composer renders from `chat.rs`
   (`render_composer`, verified today) — `pi`'s file.** Any composer-area piece must name its
   file explicitly or the two collide.
5. **`F-WIN-06` carries a user ruling (2026-08-13): the browser is IN scope** — "niente webview"
   bans an Electron-style shell, not a web engine behind the surface. The census's "deliberate
   absence" note on the nine `F-BRW` rows is dead; `browser.rs` is already being written (the
   fmt gate is red on it), P72 compositing spike queued.

## The pieces

Size legend: S ≈ a sitting, M ≈ a day-piece, L ≈ needs its own brief. "seam" = two owners, cut
P71-style into named halves. Unless a *depends* is stated, the piece dispatches today.

### codex11

| piece | rows it closes | size — what drives it | depends on |
|---|---|---|---|
| B-01 · the summarizer command | `F-AGENT-API-01` `F-AGENT-OMP-03` `F-AGENT-OPENCODE-03` | S/M — one trait method + per-adapter command builders | nothing; gives `F-SET-05`'s rename half its missing consumer |
| B-02 · the browser surface | `F-BRW-01`…`F-BRW-09` | **L — the largest piece on the board**; a web engine behind a GPUI surface | P72 spike outcome; already in flight |
| B-03 · changes-surface gaps | `F-CHG-01` `F-CHG-02` `F-CHG-20` | S/M — a no-worktree state, an Activity section, a panel-tab mount | nothing (`F-CHG-01` is a design divergence — flag to orchestrator at dispatch) |
| B-05 · file drag & drop | `F-CORE-FILE-03` `F-TERM-PTY-06` `F-EDIT-12` | M — right-panel drag-out + terminal drop-in; the quoting fn exists, unused | nothing |
| B-06 · wire the file watcher | `F-CORE-FILE-06` | M — `file_view.rs` consumes the inotify monitor (which lives in unowned `tiller_markdown` — consumption only) | nothing |
| B-07 · split ergonomics | `F-TAB-11` `F-TAB-23` `F-TERM-SPLIT-01` | M — left/up splits, disabled-reasons on the menu (`split_disabled_reason` exists, unrendered), 6px divider + 160px minimum | nothing |
| B-08 · empty-pane prompt | `F-TERM-02` | S | nothing |
| B-09 · pane host lifecycle | `F-TERM-PTY-07` `F-TERM-PTY-08` | L — generation/teardown abstraction + pane cache | nothing |
| B-11 · settle + resize debounce | `F-TERM-SCR-02` | S — two documented constants + tests | nothing |
| B-12 · the terminal link router | `F-TERM-UI-02` (+ heals defective `F-CORE-FILE-04`) | M — OSC 8 + URL + file links; `resolve_file_link` exists with zero callers | nothing |
| B-13 · P58, the settings contract persists | `F-CORE-SET-01` | M — SettingsPolicy keys into the persisted schema | in flight (standing handoff); unblocks five already-recipe'd DB halves (`F-SET-04..07,10`) |

### codex12

| piece | rows it closes | size — what drives it | depends on |
|---|---|---|---|
| B-20 · notification delivery | `F-AUTO-06` `F-USE-06` (+ heals defective `F-CORE-ACT-19` `F-CORE-ACT-20`; completes the blocked halves of `F-CORE-ACT-02` and `F-CTRL-NOTIFY-03`) | M — call `should_notify`/`build_payload` on activity transitions + a zbus/notify-send poster. **Six ledger entries for one piece — the best leverage on the board** | nothing |
| B-21 · the two-line notice | `F-PRJ-04` | S — insertion-failure `eprintln` → the existing `set_notice` route | nothing |
| B-22 · the missing chords | `F-TAB-28` `F-WIN-01` | S — bind `ctrl-w` (today the shipped chord is literally `"cmd-w"`) and `ctrl-,` | nothing |
| B-23 · the status cell tells the truth | `F-TAB-01` `F-TERM-03` (+ retires `F-TERM-09`'s missing pixel half) | S/M — render Running/Error/Idle and exit/signal status; today the cell shows only Done | nothing |
| B-24 · launch remount planning | `F-CORE-ACT-24` `F-CORE-ACT-25` `F-CORE-ACT-26` | M — wire `bootstrap::partition`/order/`ids_to_evict` into startup; eviction side effects | nothing hard; pairs with B-13's mount-cap key |
| B-25 · clone from URL | `F-PRJ-01` `F-PRJ-05` `F-PRJ-06` `F-PRJ-07` | seam, M — **Half A `codex11`**: `tiller_git` clone + error mapping (S, unit-testable alone). **Half B `codex12`**: the + fork and the clone form with guards and failure/retry | Half B on Half A |
| B-26 · the create-new-project form | `F-PRJ-08` `F-PRJ-09` `F-PRJ-10` (+ folds `F-CORE-DOM-03`: `default_project_base` becomes the default directory) | S/M | nothing |
| B-27 · the non-git add prompt | `F-PRJ-03` | S — Initialize-Git / Add-without-Git / Cancel on plain-folder add; the git-init backend already exists (F-SID-08) | nothing |
| B-28 · layout commands wired | `F-CORE-WSP-04` | S — the enum is dead code today | nothing |
| B-29 · persist view_state | `F-CORE-WSP-08` | S/M | pattern from B-13, not blocked |
| B-30 · tillerctl install shim | `F-CTRL-CLI-02` | S | nothing |
| B-31 · tab drag reorder | `F-TAB-18` (folds `F-TAB-24`: cancel is part of the drag) | M — the strip's only drag today is the pane divider | nothing |
| B-32 · no-worktree empty state | `F-TERM-11` | S | nothing |
| B-33 · history without a menu bar | `F-WIN-07` | S — a palette/History surface; a drawn test pins "no in-window menu bar" by design, so the clause's menu route needs an orchestrator reading | nothing |
| B-34 · toasts | `F-WIN-10` | M — only a theme radius token exists | nothing |

### pi

| piece | rows it closes | size — what drives it | depends on |
|---|---|---|---|
| B-40 · status-bar truth | `F-USE-01` `F-USE-02` `F-USE-03` | M — render the refresh control (`on_refresh` is a dead builder API), tooltips + visibility coupling, distinct Unavailable reasons | nothing; one file, zero seams |
| B-41 · model-picker search | `F-CHAT-16` | S — search + "Recommended" + no-match in the existing picker. **In `chat.rs`, not `composer.rs`** (drift item 4) | nothing |
| B-42 · context-ring breakdown | `F-CHAT-18` | S — input/output/cache rows in the existing popover; ACP data availability decides the ceiling | nothing |
| B-43 · thought + grouped steps | `F-CHAT-21` `F-CHAT-22` | M — expand/collapse affordances on entries that already render | nothing |
| B-44 · the tool-call card | `F-CHAT-23` `F-CHAT-31` | **L — the biggest chat piece**: click/expand, output preview, diff/location links, Dismiss | nothing; uses the existing `tiller_git` diff API read-only |
| B-45 · the edit summary | `F-CHAT-32` | S | nothing |
| B-46 · subagent task cards | `F-CHAT-28` | M — new entry kind; ACP must surface the data (note at dispatch) | nothing |
| B-47 · the history menu | `F-CHAT-34` `F-CHAT-35` | S/M — lists the retained chats codex12's shell already holds (read-only, no seam) | nothing |
| B-48 · sidebar small states | `F-SID-06` `F-SID-18` | S — project-row badge + "No Terminals" empty state | nothing |

### sonnet

| piece | rows it closes | size — what drives it | depends on |
|---|---|---|---|
| B-60 · agents page alive | `F-SET-16` | S — a real search input + wire the no-op Refresh to the existing `discovered()` re-run | nothing |
| B-61 · reasons on the providers page | `F-SET-11` | S — four Unavailable reasons render one "—" today; twin of B-40's reason work | nothing |
| B-62 · cookie auth | `F-SET-12` `F-SET-13` | M — settings UI half; the fetch half lands in `tiller_usage` | backend half `codex12` (ruled 2026-08-13) |
| B-63 · account actions | `F-SET-14` `F-SET-15` | M — the shipped Add Account button is a dead no-op; `F-SET-15` is design-pinned single-account (comment in code) — orchestrator note at dispatch | nothing |
| B-64 · agent colour choice | `F-SET-22` | S — `color_swatch` gains a click; value persists via B-13 | nothing for the UI |

### Seam pieces (two owners, P71-style halves)

| piece | rows it closes | halves | size | depends on |
|---|---|---|---|---|
| B-04 · ACP auth | `F-CHAT-02` | **A `codex11`**: `tiller_acp` authenticate flow · **B `pi`**: auth banner/flow in `chat.rs` | M | ruled 2026-08-13: `tiller_agents/**` is `codex11`'s, P69 stale — dispatchable |
| B-10 · questions beyond buttons | `F-CHAT-24` `F-CHAT-25` `F-CHAT-26` `F-CHAT-27` | **A `codex11`**: ACP text-answer/cancel/expiry · **B `pi`**: Plan card, text answers, pending bar, expired state | M+M | ruled 2026-08-13: same ruling — dispatchable |
| B-50 · agent registry + install | `F-SET-17` `F-SET-18` | **A `codex11`**: registry + install/update in `tiller_agents` · **B `sonnet`**: page wiring | L | B on A |
| B-51 · per-file icons | `F-CORE-FILE-08` `F-SET-21` | **A `sonnet`**: `icons.rs` extension lookup (the comet set just landed) + settings choice · **B `codex11`**: tree consumption | M | B on A |
| B-52 · project identity, catalog half | `F-PRJ-12` `F-PER-07` | **`codex12`**: editable name/repo-type fields in the `session.rs` catalog + persistence | M | nothing |
| B-53 · project identity, sheet half | `F-PRJ-11` | **`pi`**: sheet gains edit controls + the trash-with-confirm route | M | B-52 |
| B-54 · the icon picker | `F-PRJ-13` `F-PRJ-15` `F-PRJ-16` | **A `sonnet`**: glyph/emoji catalog in `icons.rs` · **B `pi`**: picker UI in the sheet | M/L | B-52 |
| B-55 · remote avatars | `F-PRJ-14` | **`pi`** + network fetch/caching | L | B-52; **recommend deferring — worst value/size on the board** |
| B-56 · the worktree comment | `F-SID-11` (+ heals defective `F-CTRL-WORK-01`) | **A `codex12`**: comment column + socket persist · **B `pi`**: row render/edit | S+S | B on A |
| B-57 · Remove Worktree, the honest route | `F-SID-15` | **A `pi`**: menu item · **B `codex12`**: action arm + confirmation | S | nothing |
| B-58 · worktree location choice | `F-PRJ-17` `F-PRJ-18` | **A `codex11`**: `tiller_git` base override · **B `codex12`**: prompt control | S+S | nothing |
| B-59 · attach to terminal | `F-TAB-25` | **A `codex12`**: adopt-pane on the model · **B `pi`**: the sidebar menu entry that calls it | S+S | nothing — orchestrator ruling 2026-08-13: the reference has it (`App/SidebarView.swift:717`, "Attach to Current Terminal" → `model.adoptPane`); P65's refusal was a builder's own scope call, overruled |

### Formerly unowned — ruled 2026-08-13: `tiller_usage/**` is `codex12`'s

| piece | rows it closes | size | note |
|---|---|---|---|
| B-70 · usage transport seam | `F-CORE-USG-06` `F-CORE-USG-07` | S — a trait over `http.rs` + injected tests; pure refactor | `codex12` (usage is session-scoped; also takes B-62's backend half) |

## The 10 defective rows — separate list, separate economics

Diagnosing is cheaper than building; four of the ten are healed by construction pieces above,
the rest are four small pieces:

| piece | rows | owner | size |
|---|---|---|---|
| D-1 · the oh-my-pi rename | `F-AGENT-OMP-01` `F-AGENT-OMP-02` | `codex11` | S — the adapter seeks `omp`, the distribution ships `oh-my-pi`; OMP-02 falls with OMP-01 |
| D-2 · process-group hygiene | `F-PER-06` `F-TERM-08` | `codex11` | M — kill process groups on close/quit + the close confirmation |
| D-3 · wire Install Skill | `F-SET-09` | `sonnet` | S — the button's handler is the literal no-op; the provisioner exists and is tested |
| D-4 · an honest New Browser entry | `F-WIN-06` (interim) | `codex12` | S — disable-with-reason or feedback until B-02 lands; the full heal is B-02 (`codex11`) |
| — absorbed | `F-CORE-ACT-19` `F-CORE-ACT-20` → B-20 · `F-CORE-FILE-04` → B-12 · `F-CTRL-WORK-01` → B-56 | | |

## Rows that got no piece — and why

- `F-SID-16` `F-SID-17` — sidebar drag reorder is **removed by design** and a no-reorder test
  pins it. That is a verdict question for the orchestrator/`pireview`, not a construction job.
- ~~`F-TAB-25`~~ — resolved: the orchestrator verified the reference has the feature
  (`SidebarView.swift:717`) and overruled P65's self-made refusal → now seam piece **B-59**.

Folded rather than pieced: `F-CORE-DOM-03` → B-26, `F-TAB-24` → B-31, `F-PER-07` → B-52,
`F-AGENT-OMP-02` → D-1.

## Dispatch order — who takes what next, and why

- **`codex12` → B-20 (notification delivery)** — six ledger entries for one wiring piece, no
  dependencies; nothing else on the board comes close. On the way in, land B-21 + B-22 in the
  same sitting (both are minutes, both are the "two-line" class). Then B-23.
- **`codex11` → finish B-13 (P58)** — it is already his, and five settings DB-halves are sitting
  in `pireview`'s recipe queue waiting on it; every day it stays open, converted rows stay
  half-proven. Then D-1 (the rename is two lines and un-bricks two agents), then B-07. B-02
  continues as spiked by P72.
- **`pi` → B-40 (status-bar truth)** — three rows, one file `pi` owns outright, zero seams, no
  dependency — dispatchable this minute while drift item 2 (P69 vs the map on `tiller_agents`)
  gets ruled. Once ruled, B-10 Half B is the natural next: it is P69's own territory. Then B-44.
- **`sonnet` → D-3 (wire Install Skill) then B-60 (agents page alive)** — both tiny, both on the
  file `sonnet` just inherited, and B-60 kills the page's two dead controls in one pass. Then
  B-61, then B-51 Half A (the comet icons are already in his tree).

Both rulings this order needed have landed (2026-08-13 23:53): `tiller_agents/**` → `codex11`
(P69 stale; adapters and transport are one subsystem), `tiller_usage/**` → `codex12` (usage is
session-scoped). B-04, B-10, B-70 and B-62's backend half are all dispatchable.

## Honest remainder

- The numbers moved while cutting (131→127 absent, 8→10 defective); this doc measures the tree
  and ledger as of 2026-08-13 late evening. Builders are landing pieces tonight — `browser.rs`
  visibly in flight — so some rows here may be built before this is read. The census pattern
  says that staleness concentrates in assigned rows: re-read the row before dispatching a piece.
- Sizes are read from row evidence and code shape, not from prototyping; an L may hide an M and
  vice versa. The 25 built-still-absent rows and the recipe'd half-proven set are deliberately
  not here — they are the critic's queue, not the builders'.
