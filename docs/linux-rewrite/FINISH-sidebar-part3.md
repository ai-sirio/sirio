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

(remaining rows filled in incrementally below, each followed by a commit)
