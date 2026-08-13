# manifest — 2026-08-13 pass 13 display session (pireview)

Every frame below was captured from a live Tiller instance on `:1` via
`import -window`, with `GPUI_X11_SCALE_FACTOR=1` and a PID- or
single-instance-verified window. Colour counts pass the sweep's MIN_COLORS
guard on every frame. Frames in `visual-sweep/` were produced by
`Scripts/visual-sweep.sh` (project.add + PID-match fixes landed this pass).

App state: fixture projects `fixture-proj` (git, worktrees `main` +
`waku-branch`) and `plain-folder` (non-git) added via `tillerctl project add`
over `$TILLER_SOCKET`; session DB `TILLER_DB=chat.sqlite`,
`XDG_STATE_HOME=/tmp/critic-pass13/state-home`.

| file | evidence for | how produced |
|---|---|---|
| pass13/A2-00-launch-clean.png | D-EMPTY-01 (no-project front door) — sidebar empty, no rows | fresh XDG_STATE_HOME, capture before any action |
| pass13/A2-01-add-git-project.png | F-PRJ-02 | `tillerctl project add fixture-proj` → capture (catalog refresh lands after the second add; see A2-02) |
| pass13/A2-02-add-folder-project.png | F-PRJ-02 (both rows), F-PRJ-03 (non-git add, no prompt — no modal in frame) | second `project add` → capture |
| pass13/A2-03-filter-narrowed.png | F-SID-02 narrowing (filter text + rows reduced: 9→4 text bands measured) | click Filter field, type `waku`, capture |
| pass13/A2-04-filter-cleared.png | F-SID-02 restore (rows back to 9 bands) | backspace ×4, capture |
| pass13/P1-collapsed.png | F-SID-04 collapse on display (9→4 bands) | split press/release click on the project row |
| pass13/E1-expand.png | F-SID-04 expand on display (4→9 bands) | second split click |
| pass13/H2-rc-wt.png | F-SID-12/14 worktree context menu drawn (8-item menu, card_fill block y86–331) | right-click worktree row |
| pass13/S-plus-clicked.png | F-PRJ-01: + opens the platform folder picker — ashpd FileChooser request objects created on the D-Bus (0→2) | split click on +, capture |
| pass13/SET-01-appearance.png | F-SET-19/20 pixels: Appearance page, theme segment card + translucency row + font rows | `surface settings open` + `select Appearance` |
| pass13/SET-02-agents.png | F-SET-22 pixels: Agents page | `surface settings select Agents` |
| pass13/CHG-01-changes-dirty.png | F-CHG-06 pixels: Changes surface, 3 sections (staged/modified/untracked fixture); right-panel files tree shows 3 status dots | dirty fixture + `surface changes open` |
| pass13/CHG-02b-no-worktree.png | F-CHG-02: after close-workspace (`current-workspace` = none) the changes surface still serves the last worktree — no no-worktree explanation state exists | close-workspace + `surface changes open` + capture |
| pass13/TERM-11-running.png / TERM-12-done.png | F-TERM-09: zero-pixel delta between a running `sleep` pane and its finished state — no visible indicator transition | `panel create --cmd "sleep 15"`, captures at +4s and +20s |

Machine transcripts (socket, not pixels) backing the same rows:
`tillerctl surface changes read --json` → sections Staged/Changed/Untracked with
per-file counts; `tillerctl panel state` → `running` while sleep lives;
`tillerctl current-workspace` → `no current workspace` after close-workspace.

`visual-sweep/` (5 frames + 10 waku comparisons) ran to completion this pass;
its transcript lists the stale gap notes (see findings log — the socket has
Changes/Settings methods the sweep claims are missing).
