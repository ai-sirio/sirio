# W13-git+per+persist+win evidence

## `F-GIT-RUN-02` — ledger line 484 — exercised-working

Prior evidence (P120) said wtype could not deliver Return/Escape at all on this compositor.
That was true only because the field click happened before the newly-opened form's layout had
settled — the same "repair must be forced" trap documented in WAYLAND-LANE.md trap 2, applied
to keyboard focus rather than a screenshot. Once a `shot` (forced repaint) was inserted between
opening the Clone-repository form and clicking its URL field, a real `key Return` worked, proven
twice: once creating a worktree via the branch-name prompt, and once submitting the Clone
Repository form and driving `GitClone::clone` -> `GitRunner::run_streaming` end to end.

Drive: `project.add path=/tmp/w13-testrepo` (a real local git repo with one commit) -> clicked
"+" next to Projects -> "Clone Repository..." -> **settled with a `shot`** -> clicked the
Repository-URL field -> `type /tmp/w13-testrepo` -> **`key Return`** -> the form closed and a new
project `w13-testrepo` appeared in the sidebar at `/home/enzopalmisano/w13-testrepo` with a
`main` worktree marked `Primary`. Verified on disk, not just in the UI: `/home/enzopalmisano/w13-testrepo/.git`
existed with `git log` showing the cloned commit (`8c0a29e init`), then removed as scratch
cleanup. Because the source path exists on disk, `clone.rs` adds `--no-local`, forcing git
through the real object-receiving path rather than the local hardlink optimization, so this is a
genuine exercise of `run_streaming`'s line-by-line stderr forwarding, not a no-op.

As a second, independent confirmation of Return delivery: the New Worktree branch-name prompt
(`click` the "New Worktree..." row -> `type w13-newbranch` -> `key Return`) produced a new
sidebar row `w13-newbranch` at `/tmp/w13-testrepo-w13-newbranch`, and the directory was created
on disk by `git worktree add` (a sibling caller of `run_accepting`, not `run_streaming`, but
proof the same Return-keypress mechanism is not the blocker triage evidence claimed).

Captures: `02-01-baseline.png` (project added), `03-02-form-open.png`/`04-03-typed.png`/
`05-04-after-return.png` (worktree prompt sequence), `06-add-menu.png`..`28-clone-c` under
`reference/linux-progress/wavea-W13-git+per+persist+win/` (clone-form sequence; `27-clone-b.png`
is the discriminating frame showing the new project+worktree after a genuine clone).

**Correction to the manifest's approach note**: an X11/xdotool lane is not needed. The lane's own
`key Return` (wtype against the persistent virtual keyboard, per WAYLAND-LANE.md trap 3) delivers
Return correctly once the target field's frame has settled before the click. The earlier
"wtype cannot deliver Return" finding was an artifact of clicking a field before its container's
layout had rendered, not an instrument limit.
