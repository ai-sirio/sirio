# FINISH-sweep-tail — thirteen half-proven rows nobody else owns (lane wf-sweep)

Fresh critic pass, 2026-08-18, this host (x86_64 desktop, COSMIC/Wayland, per `ENVIRONMENT.md`'s
2026-08-18 section). Evidence standard: `EVIDENCE-STANDARD.md` — a verdict without a named,
replayable transcript is not a verdict. Lane: `Scripts/wayland-drive.sh`, binary pinned per
`ENVIRONMENT.md`:

```bash
cargo build --manifest-path rust/Cargo.toml
cp rust/target/debug/tiller /tmp/wf-sweep-tiller && export TILLER_WL_BIN=/tmp/wf-sweep-tiller
```

`TILLER_WL_LABEL=wf-sweep`, `TILLER_DB=/tmp/wf-sweep.sqlite`. Screenshots referenced below are
committed under `reference/linux-progress/wf-sweep/`.

## A trap this pass hit and is recording for the next one

Every row here needed a right-click context menu, then a click on one of its items. The FIRST
attempt at every such gesture, done as `rightclick <x> <y>` immediately followed by `click <x> <y>`
with nothing between them, silently failed: the click visibly landed on the correct item (hover
highlight showed in the screenshot) but the menu never closed and no side effect occurred — the
click was accepted by the compositor and delivered, but arrived before the freshly-opened menu's
subtree was truly hit-testable (the same "~2 real frames before linking" issue `wayland-drive.sh`'s
own comments document for `tab_bar.rs`'s `deferred(...)` menus, evidently shared by the sidebar's
own context menu). A visible `sleep 1` between `rightclick` and the item `click` fixed it
consistently for every row below. **Do not trust a bare `rightclick`+`click` pair with nothing
between them; the menu opening and the item being clickable are not the same frame.**

Also hit repeatedly this pass: `MESA: error: ZINK: failed to choose pdev` / `Io error: Broken pipe`
at app startup — `ENVIRONMENT.md`'s documented GPU/compositor contention from ~5-6 concurrent
sibling lanes sharing `/dev/dri/renderD128`. Fully environmental (confirmed via `ps aux` showing
`wf-chg`, `wf-tab`, `wf-rest`, `wf-rest2`, `wf-act` all alive at once); resolved by a clean
kill+retry loop, never a code concern.

## Rows

### F-SID-08 — PASSED (upgraded)

Clause: right-click a non-Git project, choose **Initialize repository**, confirm the project
changes to Git-backed behavior. The half already proven (wave F) was the underlying action via the
Project Settings sheet's button; the context-menu entry point itself was unproven.

Live drive: added a real non-git folder project (`/home/enzopalmisano/wf-sweep-nongit`, confirmed
empty, no `.git`, via `ls -la` before). Right-clicked its sidebar row — menu showed **Initialize
Git repository** enabled (a sibling git-backed project's menu in the same drive showed it disabled
with reason "Git is already initialized", confirming the menu reads real per-project git state).
Clicked it (`sleep 1` between open and click — see trap above). Result, both halves of a hard
discriminator:

- **Disk**: `/home/enzopalmisano/wf-sweep-nongit/.git` now exists (`find` before: nothing under the
  folder; `find` after: `.git` present) — a real `git init` ran.
- **UI**: the row transformed from a plain folder project (no branch child) into an expandable
  git-backed project with a `master` worktree row and a `Primary` badge, matching the sibling
  project's own shape — `reference/linux-progress/wf-sweep/f-sid-08-context-menu-init-git.png`.

Both "confirm the project changes to Git-backed behavior" and the specific context-menu entry
point (not the Project Settings sheet) are now driven.

