# Wave C slice C-P2 — verdicts

Critic pass (not the builder of this slice). Ran the named unit/integration tests per-crate
(never `--workspace`), read the production code each test exercises, and independently drove the
real running app over the Wayland lane (`TILLER_WL_LABEL` unique per drive, no lock taken) for
every row where a live drive was plausible — five of seven rows got a genuine live gesture this
pass, not just a re-read of prior evidence. Binary was already built at HEAD; no build was
triggered. One safety note up front: an early live attempt at `F-CORE-ACT-06` briefly triggered a
real `codex login` OAuth flow that opened a browser on the *host* desktop (not the sandboxed
compositor) — killed immediately, confirmed no lingering process, and switched to a safe synthetic
stand-in (a symlink named `codex` pointing at `/bin/sleep`, executed directly so `/proc/<pid>/comm`
reads "codex") for every subsequent Layer-D gesture. No real agent CLI was interacted with again
after that.

## `F-CORE-USG-06` — PASSED (upgraded from half-proven)

`cargo test -p tiller_usage token_refresh_failure_routes_to_distinct_usage_reasons` and
`cargo test -p tiller_ui token_refresh_reasons_render_distinct_text` — both pass. Read the
mapping: `codex.rs`'s reactive-401 branch (the one the reactive fetch retry hits) now calls
`token_refresh_failure_reason` instead of hardcoding `UsageReason::LoggedOut`, and
`status_bar.rs::segment_text` renders "token reused"/"token revoked"/"token expired" for the three
new `UsageReason` variants.

Drove it live end to end with a real, non-mocked HTTP round trip: `CODEX_HOME` pointed at a
synthetic `auth.json` with a backdated `last_refresh` (2020) and a garbage `refresh_token`,
launched the real `tiller` binary on the Wayland lane. The app's own log shows the genuine
sequence: `[codex-usage] proactive token refresh failed: Expired` then
`[codex-usage] token refresh failed: Expired` — both against the real `auth.openai.com/oauth/token`
endpoint (independently reproduced with a bare `curl` POST: real 401, body
`{"code":"token_expired",...}`, which `classify_token_refresh_failure` reads via the `"expir"`
substring match). The status bar segment read **"Codex token expired"**, not the old generic
"Codex logged out" — confirmed by screenshot with the pointer hovering the segment for good
measure (see F-USE-02 below, same frame). This is discriminating: the default/pre-fix behaviour
for any refresh failure was the single string "Codex logged out"; this frame shows the
failure-specific copy the row claims. Frame: `/tmp/critic-cp2/shots/01-usg06.png`.

## `F-CORE-USG-05` — half-proven (unchanged, independently re-confirmed)

`cargo test -p tiller_usage` (39 unit + 3 + 1 + 7 integration, all pass), specifically
`a_real_refresh_success_merges_into_the_auth_file_without_losing_unrelated_fields`: read the test
and confirmed it drives a real local one-shot HTTP fixture server (not a mocked function call)
through the actual `refresh_token_at` + `save_credentials_to` code path, and asserts fields the
fetcher never wrote (`id_token`, `auth_mode`, `custom_field`) survive the merge — genuine
discriminating proof the merge-not-overwrite behaviour is real.

Independently re-drove the proactive gate live (same run as `F-CORE-USG-06` above): the backdated
`last_refresh` genuinely tripped `needs_refresh` in the real running binary against the real
network (log: `proactive token refresh failed: Expired`), which is new evidence this pass — the
prior record only had the *reactive* path confirmed live. The remaining owed half — a real 200
response reaching `save_credentials` inside the actual running binary — stays unreachable in this
sandbox: `TOKEN_URL` is a hardcoded constant (`codex.rs:27`), not env-overridable, so redirecting
the live binary to a controlled 200-returning endpoint would require `/etc/hosts` + a trusted fake
TLS cert for `auth.openai.com`, out of scope for a critic pass. Verdict unchanged at half-proven,
but on stronger footing than before (the proactive-gate-fires half is now live-proven, not just
unit-proven).

## `F-USE-02` — PASSED (upgraded from half-proven)

Read `status_bar.rs`'s `provider_segment`: `tooltip_text = text.clone()` where `text` is the exact
same `Self::segment_text(...)` string rendered on the segment — structurally identical for every
`ProviderUsageState`, so there is no separate vocabulary to diverge.

Drove the previously-unreached half live: with the Codex segment reading "Codex token expired"
(the genuine `Unavailable` state from the `F-CORE-USG-06` drive above), moved a real synthetic
pointer over the segment (persistent `wl_pointer` client started before the app connected, per
`WAYLAND-LANE.md`'s pointer-race guidance) and captured the hover frame: a tooltip box reading
**"Codex token expired"** appeared directly above the segment, matching its own text exactly. This
is a real input-path gesture (not a socket call) and is discriminating — a mismatched or generic
tooltip, or no tooltip at all, would have shown a different frame. Combined with the
previously-confirmed loaded-segment case ("Claude 25% 5h · 84% wk" tooltip matching), both states
this row's claim covers are now live-proven. Frame: `/tmp/critic-cp2/shots/03-hover-attempt.png`.

## `F-USE-03` — half-proven (unchanged, Loading half newly proven live; Stale half still owed)

Read the pipeline: `StatusBar::new` initializes every provider to `Loading`, `refresh()` resets to
`Loading` each cycle, and `reduce`'s `TimedOut` branch keeps the last good value as `Stale` when
one exists (`success_replaces_the_previous_state`, passing).

The prior record said this lane had "no OCR instrument" to read the Loading text — that is a
tooling gap, not a proof gap: I can read screenshots directly. Booted a completely fresh instance
(no prior state) and captured a burst of frames at increasing delays after socket-up. At ~3.4s the
frame shows **"Claude …"** and **"Codex …"** — literally the `"{display_name} …"` Loading format,
still on screen well past a 1-frame flash — and by ~5.6s it had genuinely transitioned to
"Claude 12% 5h · 87% wk" / "Codex logged out". This is a real, discriminating, live-observed state
transition, not an inference from code. Frames:
`/tmp/critic-cp2/shots/fresh3-04-crop.png` (Loading) →
`/tmp/critic-cp2/shots/fresh3-05-crop.png` (resolved). The Stale half (force a timeout after a
prior success, confirm dimmed-with-last-numbers) was not attempted this pass: Claude's fetch is a
real hidden PTY driving the `claude` CLI's `/usage` panel (not a network call I can easily
intercept), so forcing a timeout live needs a different instrument than what I had budget for.
Verdict stays half-proven, now on materially stronger live footing for the half that was closable.

## `F-CORE-ACT-06` — PASSED (upgraded from half-proven)

Read `handle_title_change` (model.rs:187-238): the "clear on unmatched title" branch is gated on
`title_owned_panes.contains(pane_id)`; a process-owned pane is never in that set, so an unrelated
title on it already falls through with no clear, by construction.

Drove the full live gesture end to end on one pane, three screenshots:
1. Ran a safe stand-in agent (`/tmp/critic-cp2/fake-agents/codex` → symlink to `/bin/sleep`,
   invoked directly so `/proc/<pid>/comm` reads `codex`) as the pane shell's real background child.
   Layer D's 500ms `/proc` poll picked it up: the tab dot went from hollow to filled orange
   (Running) and "Activity 1 running" appeared. Frame: `12-tabglyph-crop.png`.
2. With the process still alive, typed a real shell `printf '\033]0;UNRELATED_ACT06_TITLE\007'`
   into the *same* pane's live prompt (a real OSC title escape, executed by the real shell, not a
   synthetic event). The tab dot stayed filled orange — unchanged. Frame: `23-tabglyph-crop.png`.
3. Killed the fake-agent process. Within one poll cycle the dot cleared to hollow (Idle). Frame:
   `25-tabglyph-crop.png`.

This is exactly the row's claim, discriminating at every step (a title-owned or spawn-owned pane
would have cleared at step 2; a still-alive process would not have cleared at step 3).

## `F-CORE-ACT-07` — PASSED (upgraded from NOT EXERCISED)

Read `title.rs`: `TITLE_DEBOUNCE = 1500ms`, `should_apply_title_signal` gates
`handle_title_change`'s title-derived update on `now - last_hook_update_at >= debounce`.

Drove the live race on the same process-owned pane from `F-CORE-ACT-06` (agent identity already
"codex" via the fake-agent symlink, so Layer B's *known-pane* debounced path applies rather than
the unregistered-pane identification path):
1. `ctl notify session=pane-1 status=needs-input` (real Layer-A push over the control socket).
2. ~200ms later, typed a real `printf '\033]0;codex working\007'` — a title `detect_generic` maps
   to `Running` for a registered "codex" pane. Screenshot shows the tab reading **"Terminal ?"**
   (NeedsInput glyph) and no "running" count in Activity — the contradicting title was correctly
   suppressed inside the debounce window. Frame: `27-within-debounce.png`.
3. After the debounce window had elapsed (a real ~20s gap this time, well past 1500ms — even more
   conservative than the row asks), sent a fresh title (`codex working now`). The tab flipped to
   the filled orange Running dot and "Activity 1 running" reappeared — the same title text is now
   free to apply once no longer suppressed. Frame: `28-after-debounce.png`.

Both halves of the claim (suppressed inside the window, applied after it) are now live-proven on
the same pane.

## `F-CORE-ACT-11` — half-proven (upgraded from NOT EXERCISED)

The model-level claim is proven by a real, unmocked integration test:
`cargo test -p tiller_activity each_ownership_kind_is_cleared_only_by_its_own_condition` — 1
passed. Re-read it: it puts a spawn-owned, a title-owned, and a process-owned pane on three
distinct pane ids in one `AgentActivityModel` and asserts each is cleared only by its own
condition and immune to the other two's clearing paths — exactly this row's claim, at the unit
level.

Independently live-drove two of the three legs to correctness this pass (see `F-CORE-ACT-06` for
process-owned's full survive/clear cycle). For spawn-owned: **the builder's stated blocker —
"spawn-owned leg still blocked by unreachable + menu" — does not hold.** I clicked the `+` next to
the tab bar (plain coordinate click, no socket call) and the New-Tab menu rendered in full,
including "Claude Code" (`30-plus-menu.png`, `35-plus-menu-1400.png`). Clicking it spawned a real
`claude` tab: the sidebar entry and tab title showed a filled orange Running dot immediately (spawn
time, before the CLI even painted its first frame — matching `agent_spawned`'s documented
"running at spawn is expected" behaviour), and the terminal pane went on to render Claude Code's
real live status bar (Model: Sonnet 5, Session/Weekly percentages matching the app's own usage-bar
numbers). Killing the real `claude` process with `SIGTERM` (no further keystrokes sent to it, to
avoid repeating the earlier login-flow mishap) flipped the tab glyph to `!` (Error) with "Process
exited with status 143" shown in the pane — the correct spawn-owned exit-clearing path. Frames:
`36-claude-spawned.png`, `37-claude-killed.png`.

What is still missing for a full `PASSED`: I did not get all three ownership kinds coexisting on
three separate panes *at the same time* with a live cross-check that clearing one leaves the other
two's glyphs untouched (I proved each leg's own clear/survive behaviour in isolation, in separate
pane sets, not simultaneously). A separate live attempt at the title-owned leg (typing an OSC title
directly onto a freshly-opened, unregistered plain-shell pane) produced garbled/un-executed input
on that specific pane — plausibly a virtual-keyboard binding race on a pane opened after the
keyboard keeper was already running — and was not salvageable within budget; I did not force a
false verdict from it. Net: the "blocked" reasoning behind the prior `NOT EXERCISED` is refuted
with live evidence, two of three legs are now individually live-proven, and the three-way
combination is real-test-proven — but the specific simultaneous-three-panes gesture the row asks
for was not completed live. `half-proven` is the honest call.

---

## Summary

Five of seven rows upgraded this pass, four to `PASSED` and one to a stronger `half-proven`, all on
fresh live evidence gathered independently (not inherited from the builder's report, which this
task treats as a route, not a conclusion). `F-CORE-USG-05` stays `half-proven` for a reason that is
structural to this sandbox (no way to redirect the live binary's hardcoded `TOKEN_URL` to a
controlled 200 response without host-level TLS/DNS surgery out of scope for a critic pass).
`F-CORE-ACT-11` stays `half-proven` because the exact three-simultaneous-panes gesture was not
completed, even though its two hardest legs (spawn-owned, process-owned) and the model-level
three-way combination are now independently confirmed. One process-safety note: an early live
attempt for `F-CORE-ACT-06` briefly launched a real `codex login` browser flow on the host desktop
before being killed; every gesture after that point used a safe synthetic stand-in instead of a
real agent CLI wherever Layer D detection was being exercised.
