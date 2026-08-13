# SHOT-LIST — what to photograph when the compositor comes back

**Executed pass 13** (display restored 19:01): frames, transcript evidence and the
manifest live under `reference/linux-progress/2026-08-13/` — see `manifest.md` there
for filename → ledger-row → how-produced mapping. The sweep ran end-to-end with two
harness fixes landed this pass (`project.add` before `workspace.select`; a warned
geometry fallback for compositor frames without `_NET_WM_PID`). Known remaining gaps
this pass: a light-theme frame, the settings write door, and the Changes-tab states
beyond what the socket reached (see CRITIC-findings-log PASS 13).

Written by `fable` (design loop, FABLE-02). This is the capture plan for the first session with a
working fresh display. It exists because seven inventory rows plus the pixel halves of many
drawn-PASSED rows are **display-blocked**, and because the last valid whole-app frame is
`probe-2.png` (10:25) — everything built since has never been seen. Run the phases in order; the
ordering minimises state churn (fresh-state shots first, theme flip last).

**Ground rules (from `STATE.md`, unchanged):**

- Match the window by `_NET_WM_PID` against the Tiller process you launched; capture with
  `import -window <id>`. Never capture by title alone.
- A capture with ~1 colour is a presentation failure, not a picture of the app. The sweep's
  MIN_COLORS=200 check applies to every frame here.
- **A frame closes nothing by itself.** Each shot's "evidence for" names the ledger rows it bears
  on; the critic (`pireview`, or eyes on the frame) judges — the capture session only collects.
- Store everything under `reference/linux-progress/<YYYY-MM-DD>/`, and write a `manifest.md`
  beside the frames mapping `filename → rows it is evidence for → how it was produced`. A frame
  that isn't in the manifest doesn't exist.

**Drivers:** `tillerctl` over `$TILLER_SOCKET` for everything the socket reaches;
`Scripts/linux-drive.sh` (xdotool click/type/key) for what it doesn't — the sweep's stated gaps
are the Changes tab and Settings sections, and popovers need real keystrokes. Two shots need a
**live `claude` turn** (the only installed CLI that speaks ACP).

---

## Phase 0 — sanity (abort gate)

Launch Tiller on the fresh display, capture one frame, run the colour count. If it fails
MIN_COLORS, **stop — the display debt is not paid**; do not spend the session collecting
single-colour rectangles. If it passes, this frame is `probe-3.png`, the successor to probe-2,
and the first pixel evidence newer than 10:25.

## Phase 1 — the existing sweep, unmodified

Run `Scripts/visual-sweep.sh` end to end. It already covers: the resting frame, workspace
selection, a terminal panel (create/wait/read), tab select and cycle, pane split/focus — with
PID-matched captures and automatic waku dark/light side-by-sides. Do not duplicate those states
in Phase 2. Keep its output directory; add its frames to the manifest.

## Phase 2 — states the sweep cannot reach, in order

Order chosen so state accumulates monotonically: clean app → one project → chat working states →
populated sidebar → panels/settings → palette → theme flip.

| # | shot id | evidence for | how to reach |
|---|---------|--------------|--------------|
| 1 | `launch-clean` | D-EMPTY-01 (no-project front door), D-CHROME-01 resting count | Launch with a fresh `XDG_DATA_HOME`/`XDG_STATE_HOME` (empty temp dirs) so no projects exist. Capture before touching anything. |
| 2 | `worktree-no-tab` | D-EMPTY-02 (Start Claude Code / Open Terminal) | Add the git fixture project (sweep's fixture; socket `workspace.select`), close/avoid any auto-tab so the worktree shows no tab. |
| 3 | `chat-empty` | D-EMPTY-03, **D-CHAT-01 card anatomy, D-CHROME-01/02 resting chrome, D-TAB-01 strip** — the single highest-value frame | Open a chat tab, no turns. One frame carries four D-rows; take it at rest (no popover, no panel). |
| 4 | `composer-slash` | F-CHAT-09 | `linux-drive.sh`: focus composer, type `/`. |
| 5 | `composer-mention` | F-CHAT-10 | Same, type `@`. |
| 6 | `composer-model-effort` | F-CHAT-11, F-CHAT-17 (effort section inside popover) | Click the model chip. |
| 7 | `composer-ring` | F-CHAT-19 pixel half — **locale-formatted numbers** in the popover | Click the context ring after at least one turn (take after shot 9 if empty-state ring shows no numbers). |
| 8 | `chat-streaming` | F-CHAT-07/08 pixel half — the **red square stop** control | Live `claude` turn: send a prompt long enough to stream (see STATE.md's probe prompt), capture while running. |
| 9 | `chat-queued` | D-CHAT-03 pixel half — queued chip + "Type to queue for the next turn…" placeholder | While shot 8 still streams, type a second message and Enter-commit it; capture before the turn ends. |
| 10 | `chat-done` | F-SID activity glyph (done state), transcript resting state | Let the turn finish; capture the whole window so the sidebar glyph is in frame. |
| 11 | `sidebar-populated` | D-SID-01/02/03 as rendered, **F-SID-06 pixel half** (badge) | Add the second fixture project + extra worktrees (sweep fixture supports it); one worktree with a long branch name for truncation. |
| 12 | `changes-empty` | **F-CHG-02** (display-blocked: no-worktree state) | Summon right panel → Changes with no worktree selected — `linux-drive.sh` (no socket method for Changes; the sweep says so). |
| 13 | `changes-dirty` | **F-CHG-06** (status symbols/colours), F-SID-06 cross-check | Dirty the fixture (`echo x >> file` in the worktree), open Changes. |
| 14 | `changes-running` | **F-CHG-20** (running-count row) — builder claims no such element exists; **photograph to settle it** | Open Changes while shot 8's turn runs, or start another turn. |
| 15 | `settings-appearance` | **F-SET-19** (theme page), **F-SET-20** (translucency/font) | Open Settings via its chord/menu — `linux-drive.sh`; capture each section scrolled into view. |
| 16 | `settings-agents` | **F-SET-22** (agent colour swatches), **F-PER-07** (app icon in frame) | Settings → agents section. |
| 17 | `palette-open` | D-CMD-01 as rendered (only if D2 has landed) | `linux-drive.sh`: `ctrl+shift+p` from the chat frame. |
| 18 | `theme-light-repeat` | Light-mode halves of everything above | Flip to light theme (Settings or socket if wired), re-capture **shots 3, 8, 11, 13** minimum — light recolors every surface, so it runs last. |

Socket-drivable: fixture/project/tab plumbing (1–3, 10–11 setup). Needs `linux-drive.sh`: 4–7,
12–17 (popovers, Changes, Settings — the sweep's stated gaps). Needs a live claude turn: 8–10, 14.

## After the session

Update `INVENTORY-LEDGER.md`'s seven display-blocked rows and the pixel halves **only via critic
judgement on the frames** — the manifest is the critic's queue, not a pile of screenshots. Frames
that contradict a drawn-PASSED row (the F-CHAT-08 shape) get flagged in the ledger, not silently
re-judged. probe-2 retires as the reference frame the moment `probe-3` passes Phase 0.
