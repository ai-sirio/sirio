# Wave C slice C-P3 — verdicts

Critic pass, independent of the builder. Driven live on two lanes: the Wayland lane
(`TILLER_WL_LABEL` unused this pass — its no-right-click limit ruled it out for three of the
seven rows) and the X11 lane (`Scripts/linux-drive.sh`, `DISPLAY=:1`, which does support
`rclick`), against a disposable scratch git repo
(`/home/enzopalmisano/Scrivania/Progetti/tiller-linux-verify-scratch/cp3-proj`) and a
throwaway `TILLER_DB`/`TILLER_SOCKET` pair so nothing here touches the real worktree's git
state or session DB. `git status --porcelain` on the worktree is clean after this pass.

## F-PRJ-13 — PASSED

Builder claim: *fixed* (`a1ba1ed`, `.flex_wrap()` on the swatch row), with the sidebar half
("does the picked icon reach the sidebar row") explicitly flagged as unverified because
`sidebar.rs` isn't an owned file for this row.

Live-drove the full round trip past what the builder itself verified: `rclick` the project
row -> Project Settings -> the Colour row now shows all 8 swatches wrapped onto two lines (6
+ 2) inside the narrow settings panel, not clipped at the edge. Then picked the green
git-branch icon glyph inside the picker, clicked Close, and re-screenshotted the sidebar: the
project row's icon is now the green git-branch glyph, not the original orange folder. Default
is the orange folder, so this is a real, discriminating state change, not a frame that would
look the same either way. Both the clipping defect and the previously-suspected
`on_change(ProjectIcon)`-unwired defect are confirmed fixed live.

Instrument: X11 lane screenshots, before/after the picker interaction and after Close.

## F-PRJ-17 — PASSED

Builder claim: *implemented* (`0f227b9`), single-field live click+type only ("did not get a
clean three-field-then-confirm run past the lane's own input-timing flakiness").

Live-drove all three fields together: created a second branch (`altbase`) with a
distinguishing commit (`MARKER.txt`) off `main` in the scratch repo, then in the app clicked
New Worktree -> typed a branch name into the branch field -> clicked the base field, typed
`altbase` -> clicked the location field, typed a custom absolute path -> Enter. `git worktree
list` shows the new worktree at the typed path; `git log` inside it has `MARKER.txt`'s commit
as HEAD, which only exists on `altbase`, not on `main`/HEAD — proving the typed base branch,
not HEAD, was actually used as the git ancestor. This is stronger, and end-to-end, compared
to the builder's own single-field evidence.

Instrument: real git repo, `git worktree list` + `git log` read after the drive.

## F-PRJ-18 — PASSED

Same drive as F-PRJ-17 (one three-field submission proves both rows at once). The created
worktree landed at the literal typed path (`/tmp/cp3-custom-location/cp3-proj-critic-branch`),
not the project's default sibling directory — confirmed both in the sidebar's path label and
by `git worktree list` reading the real filesystem location.

Instrument: same as F-PRJ-17.

## F-AGENT-SAFE-01 — PASSED

Builder claim: *implemented* (`784d2a2`), 8 new unit tests passing (reran: 10/10 including
the pre-existing 2), but explicitly *not* live-driven because the route needs a right-click
and the builder's Wayland lane doesn't support one.

The X11 lane does support `rclick`, so this pass drove the actual route: `rclick` a worktree
row -> New Tab -> Claude Code, on the scratch repo. Confirmed `.claude/skills/tiller/SKILL.md`
was written carrying the `<!-- Machine-managed by Tiller...` marker, and
`.claude/settings.local.json` (worktree-local, not `~/.claude/settings.json`) picked up the
five notify hooks. Then overwrote `SKILL.md` with unmarked content and repeated the identical
gesture (right-click -> New Tab -> Claude Code again): read the file back afterward and it was
byte-for-byte unchanged ("unmanaged content, not a tiller marker") — the app's own log shows
`PrepareError::UnmanagedSkillFile` fired for exactly that path, and `settings.local.json`'s
mtime did not advance either, confirming the `?`-propagated refusal left hook config untouched
too, matching the row's actual VERIFY clause (`02-inventory-packages.md`: "refuses to
overwrite an existing unmanaged skill file... compare success/error behavior").

One caveat worth recording though not a reason to fail this row: the builder's own predicted
route said the refusal "should make the tab launch fail to prepare (**visible in the
pane/console**)". Live-driving it, that is not what happens — `main.rs`'s three `prepare()`
call sites only `eprintln!` the error to the app's own stdout/stderr log; the terminal tab
still launches the real `claude` CLI normally (trust-prompt and all), with no notice in the
sidebar or the pane. A user hitting this refusal gets a normal-looking Claude Code tab that
silently has no Tiller hooks wired (since hook setup is skipped by the same early return) and
no on-screen indication why. That's a real UX gap, but it's a different, unfiled clause from
what this row's own VERIFY text asks for (the refusal itself, and worktree-local-only writes),
which is fully proven.

Instrument: real `claude` CLI process spawned twice via the actual right-click gesture on
`DISPLAY=:1`; file reads and app-log grep between the two runs.

## F-AGENT-API-01 — PASSED

Builder claim: *already-correct* for owned files (resume_command, reran 5/5 passing), residual
defect (OpenCode `panel.read` racing `panel.list`) filed as foreign-file. `INTEGRATION.md`
records the integrator applying the actual fix on the builder's behalf outside owned files
(`tiller_control/src/panel.rs`, commit `3fb90ec`): `PaneRegistry::read` now checks the
renderer-owned `external` map as well as the control-owned one, mirroring `state()`/
`scrollback()`. `cargo test -p tiller_control` reran clean (46/46, including the new
regression test named in `INTEGRATION.md`).

Live-drove the exact route the row names: opened a real OpenCode tab (right-click -> New Tab
-> OpenCode) over the scratch repo, `panel.list` showed it wired (`agent":"opencode"`,
`id":"pane-2"`), and an immediate `panel.read id=pane-2` returned `{"ok":true,"result":
{"data":""}}` — not the `UnknownPane`/"unknown pane" error the row's evidence had previously
reproduced at this exact race window. Confirms the foreign-file fix live, not just via test.

Instrument: control-socket calls against a live app instance, immediately sequential
(`panel.list` then `panel.read`, same drive script invocation, no intervening delay).

## F-AGENT-OMP-01 — UNREACHABLE

Builder claim: *blocked*, `oh-my-pi --version` reproduces an upstream un-transpiled-TS
`SyntaxError` at `bin/oh-my-pi.js:176` before argv is read.

Independently reran `oh-my-pi --version` in this environment: identical `SyntaxError:
Unexpected token ':'` at the same file/line, before any Tiller-authored code could run. No
session can ever come to exist to exercise this row's actual behaviour — the block is
upstream of Tiller and this pass's own gestures can't route around it. Matches the ledger's
existing framing exactly; recording as `UNREACHABLE` rather than `NOT EXERCISED` since the
behaviour genuinely cannot be reached (this wasn't a time-budget skip).

Instrument: direct shell invocation of the installed `oh-my-pi` binary.

## F-AGENT-OMP-02 — UNREACHABLE

Same shared upstream blocker as OMP-01 (no session can exist to fire any hook event).
Independently reconfirmed via the same `oh-my-pi --version` run. `omp.rs`'s
`command`/`resume_command`/`prepare` were read and are plausible Rust for what a working
upstream would need, but there is no way to drive them locally.

Instrument: same as OMP-01.
