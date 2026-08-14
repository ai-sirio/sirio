# W02-core evidence log

Slice: `docs/linux-rewrite/wave-a/W02-core.md`. One section per row, appended as each row
finishes (never batched). Lane: Wayland only (`TILLER_WL_LABEL=wavea-W02-core`).

## `F-CORE-AUTH-01`

**Claim:** could-not-reach.

**Drove:** Reviewed `AgentAccountIdentity::parse_claude_json` (tiller_usage/src/account.rs:52)
and its caller at `tiller_ui/src/settings.rs:548`. Confirmed the prior pass's half-proof
(spawn + PKCE URL) is real per the code path — `settings.rs:548` genuinely feeds
`claude`'s real stdout into the parser, no mock in between. Did not attempt to carry the
OAuth flow to completion: doing so requires a real `claude` account, a real browser
completing the PKCE authorize redirect, and pasting back a real authorization code — none
of which a scripted Wayland-lane drive can produce without live credentials belonging to a
real account. Forcing it through with throwaway credentials risks writing bogus
`~/.claude` state that could affect the shared machine outside this worktree, which the
task charter's "record what you saw" instruction does not license.

**Observed:** No new capture. The half-proof already on record (PKCE URL confirmed live)
stands; `parse_claude_json`'s parse of genuine completed-login JSON remains undriven.

**Verdict left as:** half-proven, unchanged — this pass could not add the missing half.

## `F-CORE-DOM-03`

**Claim:** could-not-reach.

**Drove:** Clicked the sidebar's "+" Add Project affordance (`add-project` at sidebar
top-right, `sidebar.rs:2419`), then clicked "Open Project…" in the menu that renders at
`top:30 right:12` (`sidebar.rs:1746`), which calls `start_open_project` ->
`cx.prompt_for_paths` (`project_identity.rs:491`). Captured before/after
(`06-before-open.png`, `07-after-open-click.png`) and additionally queried
`swaymsg -t get_tree` on this lane's own nested compositor immediately after the click.

**Observed:** The nested compositor's window tree shows exactly one `con` (the Tiller
toplevel itself, `focused=true`) both before and after the click — no second toplevel, no
portal dialog window appeared anywhere in this instance's own tree. This reproduces
P120's finding on the current HEAD (`4073297`) rather than an old build: the button is
real (menu opens, item is clickable, `start_open_project` genuinely calls
`cx.prompt_for_paths`), but a real `xdg-desktop-portal` file-chooser call, per
`ENVIRONMENT.md:77`, can plausibly be routed to and rendered by the **host desktop's**
portal backend rather than this lane's own isolated compositor — in which case it would
never appear in this lane's tree or `grim` capture even if it fired and opened
successfully. This lane cannot distinguish "the portal never opened" from "the portal
opened invisibly on the host desktop", and the task charter restricts this slice to the
Wayland lane only (no `DISPLAY=:1`, no host-desktop interaction) — so resolving the
ambiguity is out of reach from here, not a defect finding.

**Captures:** `reference/linux-progress/wavea-W02-core/02-06-before-open.png`,
`03-07-after-open-click.png`, `02-08-tree-check.png`.

**Verdict left as:** NOT EXERCISED, unchanged — instrument ambiguity persists on current HEAD.
