# Wave C slice C-P2 report

## `F-CORE-USG-06` — implemented

Real gap found: `classify_token_refresh_failure`'s Reused/Revoked/Expired classification was
computed in `codex.rs` but had nowhere to go — `UsageReason` (model.rs) had only a generic
`LoggedOut`, so every refresh-failure case rendered as "Codex logged out" regardless of which
kind of failure it was.

Fix:
- `tiller_usage/src/model.rs`: added `UsageReason::TokenReused` / `TokenRevoked` / `TokenExpired`.
- `tiller_usage/src/codex.rs`: added `token_refresh_failure_reason(TokenRefreshFailure) -> UsageReason`
  and wired the reactive-401 refresh-failure branch (previously hard-coded to `UsageReason::LoggedOut`)
  through it. Added `token_refresh_failure_routes_to_distinct_usage_reasons` unit test.
- `tiller_ui/src/status_bar.rs`: `segment_text` now renders "token reused" / "token revoked" /
  "token expired" for the three new reasons. Added `token_refresh_reasons_render_distinct_text` unit
  test.

Commit: `8b7c265` — `feat(usage): route Codex token refresh classification to distinct status-bar copy`

**howToExercise**: Set `CODEX_HOME` to an isolated dir with a synthetic `auth.json` whose
`access_token`/`refresh_token` are backdated so `needs_refresh` fires, and whose refresh POST
target can be made to 401 with a body containing "reuse"/"revok"/"invalid_grant" (the live-run
technique already used in prior passes, e.g. a one-shot local HTTP fixture or a redirected
`TOKEN_URL`), then read the Codex segment in the status bar: it now reads "Codex token reused" /
"Codex token revoked" / "Codex token expired" instead of the generic "Codex logged out". Unit-level:
`cargo test -p tiller_usage token_refresh_failure_routes_to_distinct_usage_reasons` and
`cargo test -p tiller_ui token_refresh_reasons_render_distinct_text` both cover the mapping and the
text without needing network access.

## `F-CORE-USG-05` — already-correct (no code change)

Re-read `codex.rs`'s `needs_refresh` gate and the merge-save path (`save_credentials`) plus their
existing unit tests (`saving_refreshed_tokens_merges_and_preserves_other_fields`,
`saving_writes_a_parseable_last_refresh_and_keeps_unknown_token_fields`,
`a_real_refresh_success_merges_into_the_auth_file_without_losing_unrelated_fields`) — all green,
all real (one drives an actual local HTTP fixture through `refresh_token_at`). The remaining half
recorded as owed ("no real valid Codex creds available") is a live-credentials limitation, not a
code gap: this sandbox has no real signed-in Codex account to drive a genuine 200 refresh response
through the running binary. No code change made.

**howToExercise**: with a real (or a locally faked 200-returning) `TOKEN_URL` fixture, place a
Codex `auth.json` whose `last_refresh` is `>= REFRESH_AFTER` (8 days) old and confirm the proactive
refresh at `codex.rs:342` fires and `save_credentials` merges the new tokens into the file without
losing unrelated fields (diff the file before/after).

## `F-USE-02` — already-correct (no code change)

Read `status_bar.rs`'s tooltip wiring (`provider_segment`, line ~412): `tooltip_text = text.clone()`
where `text` is `Self::segment_text(...)` — the same function for every `ProviderUsageState`,
including `Unavailable`. The tooltip is therefore already correct for the unavailable case by
construction, not just the loaded case; there is no separate vocabulary to add. The recorded gap
("Unavailable-segment tooltip case still unreached") is a live-forcing-a-provider-unavailable
limitation (blocked on F-USE-03/needing a real logged-out or dead provider live), not a code defect.
No code change made.

**howToExercise**: force a provider to `Unavailable` (e.g. empty `CODEX_HOME`, as used for
F-CORE-USG-07's positive control) and hover its status-bar segment; the tooltip text must read the
same "<Provider> logged out"/"not found"/etc. as the segment itself.

## `F-USE-03` — already-correct (no code change)

Read the full pipeline: `status_bar.rs` initializes all four providers to `ProviderUsageState::Loading`
(`StatusBar::new`), `refresh()` resets to `Loading` on every fetch cycle, and `reduce()` (model.rs)
is genuinely called from the live update path (`self.claude = reduce(claude, &self.claude)` etc.,
`status_bar.rs:194-197`) — not a dead function. `reduce`'s `TimedOut` branch already keeps the last
good value as `Stale` (dimmed, same text as `Loaded`) when one exists, confirmed by
`success_replaces_the_previous_state`'s existing unit test. `segment_text`'s `Loading` case renders
`"<Provider> …"`, proven by `unavailable_reasons_render_distinct_text`. No code gap found; the
recorded blocker ("no OCR instrument exists on this lane") is a proof-instrumentation limitation on
the live drive, not a code defect. No code change made.

**howToExercise**: launch the app fresh and `shot` immediately (before the first fetch resolves,
~0-1s) — the Claude/Codex/OpenCode Go/Ollama Cloud segments should all read "<Provider> …". Then let
a fetch time out (e.g. block the Claude PTY fetch's network path) after a prior successful load —
the segment should keep showing the last numbers but visibly dimmed (`segment_dimmed` returns
`true` for `Stale`).

## `F-CORE-ACT-06` — already-correct (no code change)

Read `handle_title_change` (model.rs:187-238): the "unmatched title clears status" branch is gated
on `self.title_owned_panes.contains(pane_id)` — a process-owned pane is never inserted into that
set (`process_identified` inserts into `process_owned_panes` instead), so an unrelated title on a
process-owned pane already falls through with no clear. This exact scenario — process-owned pane,
unrelated title, status survives, then only `process_gone` clears it — is already covered by the
existing integration test `each_ownership_kind_is_cleared_only_by_its_own_condition`
(`tiller_activity/tests/activity_integration.rs:197-231`, not touched — outside my file list, but
already green). No code gap found. The recorded owed half was a *live-gesture* repeat, not a code
defect.

**howToExercise**: on the wayland lane, click a shell pane, `ctl` a foreground process match (or
launch a real agent CLI as the shell's child so Layer D fires and the pane joins
`processOwnedPanes`), confirm its tab dot goes to the running glyph, then `title <unrelated text>`
and screenshot — the glyph must stay unchanged (no clear). Only ending the child process should
clear it.

## `F-CORE-ACT-07` — already-correct (no code change)

Read `title.rs`'s `TITLE_DEBOUNCE` (1500ms) and `should_apply_title_signal`, and its call site in
`handle_title_change` (model.rs:223-229): a Layer-A push updates `last_hook_update_at`
(`notify`, model.rs:111), and `handle_title_change` skips applying a contradicting title status
while `now - last_hook_update_at < TITLE_DEBOUNCE`. This is exercised by the existing
`layer_a_push_suppresses_contradicting_title_inside_debounce_and_stops_after` integration test. No
code gap found; this row was never live-raced (both this pass and the prior one), which the
inventory correctly records as `NOT EXERCISED` rather than a defect claim.

**howToExercise**: on the wayland lane, `ctl notify session=<pane> status=needs-input`, then within
1500ms `title <text that Layer B would read as running>` and screenshot — the glyph must still read
needs-input. After the debounce window elapses, a fresh contradicting title should be free to apply.

## `F-CORE-ACT-11` — already-correct (no code change)

Same integration test as ACT-06 (`each_ownership_kind_is_cleared_only_by_its_own_condition`)
combines all three ownership kinds on three separate panes (`P1` spawn-owned, `P2` title-owned,
`P3` process-owned) in one test and confirms each is cleared only by its own condition and immune
to the others'. That is exactly this row's claim at the model level. What remains owed is the live
version — three real panes of three different ownership kinds simultaneously, each independently
exercised — which needs a spawn-owned pane reachable past the same unreachable-menu blocker
`ACT-11`'s prior evidence already names, plus a real process-owned agent CLI. Both are environment
blockers in this sandbox (no installed `claude`/`codex`/`pi`/`omp` binaries confirmed, and the
spawn-menu blocker predates this pass), not code gaps. No code change made.

**howToExercise**: open three panes in one worktree — one via Tiller's own "new agent" spawn menu
(spawn-owned), one where an unregistered pane's title first identifies it (title-owned), one where
a real agent CLI is the shell's foreground child with no title convention (process-owned, e.g. a
Node-hosted CLI) — then independently clear each (process exit, unmatched title, `process_gone`) and
confirm the other two are unaffected by screenshot at each step.

## Blocked / not attempted

None outright blocked — every row's code path was read and either fixed (USG-06) or confirmed
already correct against its existing unit/integration tests. The five rows left `already-correct`
still owe a **live** gesture per their recorded evidence; that gesture could not be produced this
pass either because the wayland-drive lane hung past its budget (F-USE-03's fresh-launch capture)
or because it needs a real signed-in Codex account / real agent CLI binaries not present in this
sandbox (USG-05, ACT-11).
