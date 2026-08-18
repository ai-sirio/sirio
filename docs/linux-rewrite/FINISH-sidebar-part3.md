# Finish-line critic: sidebar-part3 — the ten half-proven F-SID/F-PRJ rows

Lane: `wf-sid2`. Scope: every `F-SID-*`/`F-PRJ-*` row in `INVENTORY-LEDGER.md` whose verdict is
NOT `PASSED` — ten rows (`F-SID-08`, `F-SID-10`, `F-SID-11`, `F-SID-14`, `F-SID-15`, `F-SID-19`,
`F-PRJ-13`, `F-PRJ-14`, `F-PRJ-17`, `F-PRJ-18`), all currently `half-proven`. Each row's existing
evidence already proves one half; this pass drives only the missing half named in that evidence
cell, per the brief. No half that was already proven is re-proven here except where needed to set
up state for the missing half.

Binary pinned per `ENVIRONMENT.md`:

```
cargo build --manifest-path rust/Cargo.toml
cp rust/target/debug/tiller /tmp/wf-sid2-tiller
export TILLER_WL_BIN=/tmp/wf-sid2-tiller TILLER_WL_LABEL=wf-sid2
```

Fixtures created this pass under `/home/enzopalmisano/`: `wf-sid2-nongit` (plain folder, no
`.git`, for F-SID-08), `wf-sid2-removeproj` (git repo, disposable, for F-SID-10), `wf-sid2-decoy`
and `wf-sid2-main` (git repos, for F-PRJ-13's exact-geometry repro), plus whatever additional
disposable fixtures each row's per-row section names.

## Per-row results

**F-SID-08** — PASSED (upgraded from half-proven). The prior evidence drove Initialize Git
through the Project Settings sheet's button; the missing half — the sidebar row's own
right-click context menu, which this clause/SRC names — was driven live this pass. Fixture:
`/home/enzopalmisano/wf-sid2-nongit`, a plain folder with no `.git`, added via `project.add`.
Right-clicked the `wf-sid2-nongit` project row: menu opened with **Project Settings /
Initialize Git repository / Show in File Manager / Remove Project**
(`reference/linux-progress/wf-sid2/f-sid-08-context-menu-open.png`). Clicked **Initialize Git
repository**: the sidebar row gained a `master` worktree with a branch icon and Primary badge
where before it only showed the bare folder path
(`reference/linux-progress/wf-sid2/f-sid-08-after-initialize-git.png`). Hard discriminator: a
real `.git` directory now exists on disk (`ls -la /home/enzopalmisano/wf-sid2-nongit/.git`) and
`git -C /home/enzopalmisano/wf-sid2-nongit branch --show-current` prints `master` — not just a
UI change, a genuine git init. Both arms of the clause's OR ("confirm the project changes to
Git-backed behavior or shows the initialization error") are now covered: this row proves the
success arm through the named entry point; the error arm was already covered elsewhere (not
re-driven, not this row's missing half).

**F-SID-10** — PASSED (upgraded from half-proven). The prior evidence drove remove-project
through the Project Settings sheet's Remove Project control; the missing half — the sidebar
row's own right-click "Remove Project" item, which this clause specifically names — was driven
live this pass. Fixture: `/home/enzopalmisano/wf-sid2-removeproj`, a disposable git repo.
Right-clicked the `wf-sid2-removeproj` project row: menu opened with **Project Settings /
Initialize Git repository (disabled, "Git is already initialized") / Show in File Manager /
Remove Project** (`reference/linux-progress/wf-sid2/f-sid-10-context-menu-open.png`). Clicked
**Remove Project**, then clicked **Remove from Tiller** on the resulting "Remove project from
Tiller? This only removes the project from Tiller's si[debar]..." confirm dialog, all within one
continuous drive. Hard discriminator: a direct sqlite query against the live on-disk DB
(`select id, root_path from project`) shows exactly 4 rows afterward — `wf-sid2-removeproj` is
gone, the other three fixtures remain — and the row is also gone from the rendered sidebar
(`reference/linux-progress/wf-sid2/f-sid-10-after-remove-row-gone.png`, jumps straight from the
prior project's "New Worktree..." to `wf-sid2-main`). `ls` confirms the on-disk directory still
exists, consistent with the dialog's own "only removes... from Tiller's sidebar" text and with
F-PRJ-11's prior finding for the Settings-sheet entry point.

**Harness note for the next driver**: row Y-coordinates in this sidebar are NOT stable across
app restarts even with identical DB content — the auto-discovered `tiller` project (this
worktree's own real repo, unavoidably picked up by `initial_working_directory()`'s
nearest-git-ancestor walk since `wayland-drive.sh` launches the binary from the repo root) shows
or hides its primary worktree's default Chat/Terminal tab rows depending on session-restore
timing, shifting every row below it by up to 2 row-heights. A rightclick aimed at a stale Y
landed on a neighbouring worktree row instead of a project row once during this pass (opened a
`Codex` tab on `wf-sid2-removeproj`'s own `master` worktree — harmless, but a reminder). Always
re-screenshot immediately before a destructive click in the SAME invocation, never reuse a
coordinate read from an earlier invocation. Also: **never click into the `tiller` project or any
of its worktree rows** (`rust/gpui-rewrite`, `linux/gpui-waku`, `wf-term-clean`, and several
`/tmp`-rooted ones) — these are real linked worktrees of the actual repo this task runs in and
other sibling agents' lanes, not fixtures.

**F-SID-14** — FAILED - defective (downgraded from half-proven; this is a genuine app defect,
not a re-proof gap). The prior evidence drove the empty-state "New Terminal" button; the missing
half — the worktree context menu's New Terminal / agent-panel / New Chat trio, which this clause
specifically names — was driven live this pass across three separate fixture worktrees
(`wf-sid2-main`, `wf-sid2-decoy`, and an earlier one on `wf-sid2-nongit`). Right-click always
opens a 9-item worktree menu at the same fixed screen position regardless of which row was
clicked (Set/Unset Primary, New Terminal, Claude Code, Codex, OpenCode, Pi, Oh-My-Pi, New Chat,
Remove Worktree — `reference/linux-progress/wf-sid2/f-sid-14-worktree-context-menu.png`).

- **New Terminal**: right-clicked `wf-sid2-main`'s `master` worktree, clicked New Terminal — a
  real terminal tab opened with the correct breadcrumb and shell cwd
  (`reference/linux-progress/wf-sid2/f-sid-14-new-terminal-correct-worktree.png`); confirmed via
  direct sqlite read that the new tab row's `worktree_id` genuinely equals
  `wf-sid2-main`'s worktree id. **Correct.**
- **New Chat**: right-clicked `wf-sid2-decoy`'s `master` worktree, clicked New Chat — a Chat tab
  opened with the Files panel correctly showing `/home/enzopalmisano/wf-sid2-decoy`
  (`reference/linux-progress/wf-sid2/f-sid-14-new-chat-and-corrupted-siblings.png`); its
  `worktree_id` genuinely equals `wf-sid2-decoy`'s worktree id. **Correct.**
- **Agent panel (Claude Code, and separately Codex)**: right-clicked `wf-sid2-main`'s `master`
  worktree (confirmed by sidebar highlight and by the Files-panel/status-bar breadcrumb both
  reading `wf-sid2-main` at click time) and clicked **Claude Code** — the tab that opened
  reported **`Accessing workspace: /home/enzopalmisano/wf-sid2-nongit`**, a completely different
  fixture's directory (`reference/linux-progress/wf-sid2/f-sid-14-claude-code-wrong-workspace.png`).
  Reproduced twice more (once from a fresh process's very first interaction, once with **Codex**
  instead of Claude Code from `wf-sid2-removeproj`'s own worktree before it was deleted) — every
  agent-panel click from every tested worktree landed the CLI in `wf-sid2-nongit`, never the
  right-clicked worktree. **Defective.**

**Hard discriminator** (the CLI's own reported cwd, not just sidebar chrome) plus a direct sqlite
proof that the stored `worktree_id` foreign key itself is wrong, not merely a rendering glitch:

```
sqlite> select id, worktree_id, kind, title from tab;
p-05288b005dafa447-wt-0-tab-...-0 | p-a1de4dfcae803386-wt-0 | terminal | Codex
p-be79d7b106999617-wt-0-tab-...-0 | p-be79d7b106999617-wt-0 | terminal | Terminal
p-05288b005dafa447-wt-0-tab-...-1 | p-a1de4dfcae803386-wt-0 | terminal | Claude Code
p-05288b005dafa447-wt-0-tab-...-2 | p-a1de4dfcae803386-wt-0 | terminal | Claude Code
p-a1de4dfcae803386-wt-0-tab-...-3 | p-a1de4dfcae803386-wt-0 | chat     | Chat
```

Note the first three rows: their own `id` is namespaced under `p-05288b...` (`wf-sid2-nongit`,
matching where each was actually rendered when it was created) but their `worktree_id` column —
the actual foreign key the app renders from — reads `p-a1de4dfcae803386-wt-0` (`wf-sid2-decoy`,
the LAST project touched by any worktree action at the time this snapshot was taken). Creating
each new agent-panel tab appears to retroactively rewrite every earlier agent tab's
`worktree_id` to whatever worktree most recently had an agent tab created on it, not just fail to
target the row that was actually clicked. `New Terminal`'s and `New Chat`'s own rows never
exhibit this — their `id` prefix and `worktree_id` agree with each other and with the
right-clicked row every time.

Two of the clause's three named entry points (New Terminal, New Chat) are proven correct through
the exact context-menu path the clause names. The third (agent panel) is proven **broken**
through that same path, with the CLI's own workspace-accessed line as the hard discriminator —
this is not an evidence gap, it is a reproducible defect, so the row is `FAILED - defective`
rather than `half-proven`.

**Safety note**: after F-SID-14's investigation, the auto-discovered `tiller` project (this
worktree's own real repo — see the F-SID-10 note above) had a corrupted agent tab attached to
its real `rust/gpui-rewrite` worktree row, and its row positions kept shifting the sidebar
underneath other in-flight coordinate math, twice nearly causing a click meant for a fixture to
land on a real worktree's own "Remove Worktree" confirm dialog. Rather than keep computing
coordinates around a hazard, `tiller` was removed from **this Tiller instance's own project list**
via the sidebar's Remove Project (proven safe by F-SID-10: DB-only, never touches disk) — verified
before and after via `git worktree list` in the real repo and via `ls` on
`/home/enzopalmisano/Scrivania/Progetti/tiller` that every real worktree, including this one, was
untouched. This stabilized every row position for the rest of the pass.

**F-SID-15** — PASSED (upgraded from half-proven). The prior evidence only triggered the
confirm dialog by mis-click and cancelled it; the missing half — actually confirming and
watching the worktree disappear — was completed live this pass. Fixture: created a second
worktree (`extra`, branch `extra`) on `wf-sid2-main` via the sidebar's own "New Worktree..."
control, confirmed on disk via `git worktree list` showing both `master` and `extra`.
Right-clicked the `extra` worktree row: menu opened with the same 9 items as F-SID-14, target
confirmed correct via the row's own hover-highlight and close (`×`) affordance. Clicked
**Remove Worktree**: the "Remove worktree? This permanently deletes the worktree's dir[ectory]"
confirm dialog opened (`reference/linux-progress/wf-sid2/f-sid-15-remove-worktree-confirm.png`).
Clicked **Remove Worktree** on the dialog itself, all within one continuous drive. Hard
discriminator: `git -C /home/enzopalmisano/wf-sid2-main worktree list` afterward shows only
`master` — `extra` is gone — and `/home/enzopalmisano/wf-sid2-main-extra` no longer exists on
disk at all (`ls` fails with "File o directory non esistente"), confirming the dialog's
"permanently deletes the worktree's directory" text is accurate, not the previously-flagged false
claim. The row is also gone from the rendered sidebar
(`reference/linux-progress/wf-sid2/f-sid-15-after-remove-row-gone.png`, `wf-sid2-main` now shows
only `master` then "New Worktree...").

**F-SID-11** — PASSED (upgraded from half-proven). Branch, folder path, Primary pill and comment
were already proven live; the missing half — the agent-status element on a worktree row — was
driven live this pass. Fixture: `wf-sid2-nongit`'s `master` worktree, with a real registered
control pane (`pane-1`, a live `claude` process, confirmed via `tillerctl panel list` against the
live socket: `pane-1  Claude Code  Claude Code  claude  false`). Captured the row BEFORE
(`reference/linux-progress/wf-sid2/f-sid-11-before-notify.png`: no status glyph next to the
`Primary` badge, status bar's Activity section collapsed/empty). Ran
`TILLER_SOCKET=/tmp/wf-sid2.sock tillerctl notify --session pane-1 --status running` against the
real pane, forced a repaint, and captured AFTER
(`reference/linux-progress/wf-sid2/f-sid-11-after-notify-running.png`): a new orange activity
glyph appears on the `master` worktree row immediately next to the `Primary` badge, and the
bottom status bar now reads "Activity 1 running" where it previously showed nothing. Hard
discriminator: the glyph's appearance is driven purely by the control-socket notify call against
a real registered pane id, not by any UI interaction — before/after is a clean delta.

(remaining rows filled in incrementally below, each followed by a commit)
