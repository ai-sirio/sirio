# C-P3 report

## F-PRJ-13 — controls.rs colour-swatch clipping

`controls::color_picker` laid out its 8 fixed-width swatches in a
non-wrapping flex row. In the project-settings sheet (narrower than the
picker's natural width) the row clipped at the panel edge, matching the
recorded evidence exactly. Added `.flex_wrap()` so the row wraps onto a
second line instead of being cut off.

The `on_change(ProjectIcon)` half of the original evidence (icon choice not
reaching the sidebar after Close) was **not** re-broken as far as this pass
could read: `sidebar.rs`'s `apply_icon_change` already updates
`project_identities` and calls `cx.notify()`. `sidebar.rs` is not an owned
file for this row (only `controls.rs` is), so this pass did not attempt to
live-drive that half — flagging it here in case a critic still finds it
broken live, since the owned-file fix above cannot itself close that half.

Commit: `a1ba1ed`

## F-PRJ-17 / F-PRJ-18 — base branch + location overrides in New Worktree

`WorktreePrompt` only ever had a `draft` field for the branch name;
`confirm_worktree_prompt` hard-coded `None` for both `create_worktree`'s
`base` argument and `resolve_parent_directory`'s override argument even
though both functions already accepted them (`tiller_git::worktree`,
unowned but unmodified).

Added `base_draft` and `location_draft` to `WorktreePrompt`, a
`WorktreePromptField` enum tracking which field is live, and threaded both
non-empty drafts through on confirm. Two ways to switch fields:

- Tab (`on_prompt_key`) — best effort; GPUI's own tab-stop focus traversal
  can consume the keypress first (same host-dependent risk `chat.rs`
  already flags for its composer), so on this Wayland lane it did not
  reliably move focus.
- A direct click on a field (`on_mouse_down`) — reliable, confirmed live.

Live-driven end to end: opened New Worktree on the `tiller` project,
clicked the branch field, typed `e2ebranch`, Enter → `git worktree list`
on disk shows `../tiller-e2ebranch` on branch `e2ebranch`. Clicking the
base field alone and typing into it moves the selection-ring border there
and inserts text, confirmed with a screenshot. Did not get a clean
three-field-then-confirm run past the lane's own input-timing flakiness
(rapid click+type without an intervening `shot` sometimes drops
keystrokes) to also prove `base`/`location` land in the created worktree
end-to-end over this lane — the code path is unit-provable via
`derive_worktree_path`/`resolve_parent_directory`'s existing tests plus the
one live single-field round trip above.

Commits: `0f227b9`, `2b26bd2`

## F-AGENT-SAFE-01 — port `TillerSkillProvisioner`'s overwrite refusal

Re-grepped and confirmed the critic's finding: no `install_skill`-equivalent
existed anywhere in `tiller_agents`. Ported `TillerSkillProvisioner.install`
from the Swift app as `tiller_agents::install_skill` (lib.rs): marker-gated
input, per-agent destination (`claude` → `.claude/skills/tiller/SKILL.md`,
`codex`/`opencode`/`pi`/`omp` → `.agents/skills/tiller/SKILL.md`), and
managed-prefix recognition of an existing file so a previous release's
marker wording still counts as Tiller's own — refusing to overwrite
anything else (`PrepareError::UnmanagedSkillFile`).

Added a `skill_markdown()` trait hook (default `None`) and wired the
bundled `skills/tiller/SKILL.md` (already present in the repo root, unused
until now) into `claude.rs`, `omp.rs`, `opencode.rs`'s `prepare()`, ahead of
their existing hook-config writes — matching the Swift protocol
extension's own ordering, so a refusal leaves the worktree exactly as it
found it rather than partially configured. `codex.rs`/`pi.rs` are the
no-op adapters and are not owned by this slice; they keep the `None`
default unchanged.

8 new tests in `adapters_tests.rs` (32/32 passing): writes the marker file
for claude, shares one destination across codex/opencode/pi/omp, refuses
markdown missing its own marker, refuses an unsupported agent id, refuses
(and leaves byte-for-byte untouched) an existing unmanaged file, allows
overwriting a file from an older marker wording, and two `prepare()`-level
tests (installs with marker; refuses over an unmanaged file and leaves no
hook config behind either). Also updated
`each_adapter_prepare_creates_only_its_own_files`, whose expected file list
now includes the two new skill paths.

Not live-driven this pass (would need a real `claude`/`opencode`/`omp`
binary launch via New Tab → agent, which needs a right-click on this
Wayland lane — out of the lane's supported gesture set per
`WAYLAND-LANE.md`); the unit tests exercise the real filesystem (temp
dirs, no mocks) end to end through `AgentAdapter::prepare`, including the
refusal path.

Commit: `784d2a2`

## F-AGENT-API-01 — resume, half-proven

`resume_command` is already implemented and unit-tested for all 5
adapters (`claude_resume_command`, `codex_resume_command`,
`opencode_resume_command`, `pi_resume_command`, `omp_resume_command` in
`adapters_tests.rs`, all pre-existing and passing) — nothing to fix in the
owned files (`lib.rs`, `adapters_tests.rs`).

The real remaining gap from the recorded evidence — OpenCode's
`panel.read` returning `UnknownPane` moments after `panel.list` showed the
tab wired — lives in `tiller_control/src/panel.rs`'s pane registry, which
is not an owned file for this slice. Did not attempt a fix there.

Action: `already-correct` for the owned-file scope; the residual defect is
foreign-file and reported via `wantedForeignFiles`.

## F-AGENT-OMP-01 / F-AGENT-OMP-02 — blocked, upstream

Did not independently re-attempt `oh-my-pi --version` (evidence already
reconfirmed twice, by two different testers, same TS-parse `SyntaxError`
at `bin/oh-my-pi.js:176`, before argv is even read). Read `omp.rs`'s
`command`/`resume_command`/`prepare` to confirm they are correct Rust for
what an un-transpiled upstream would need if it ever ran — no code change
made, no code path to drive locally. Reported `blocked`, matching the
ledger's own "instrument-blocked upstream, not FAILED" framing.
