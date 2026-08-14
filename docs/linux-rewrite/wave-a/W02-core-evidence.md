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
