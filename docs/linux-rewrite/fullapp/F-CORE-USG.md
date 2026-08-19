# F-CORE-USG — finish-line critic pass (independent re-drive)

Fresh, independent critic pass over **F-CORE-USG-01..09** (Package tier / TillerCore
domain sub-group: usage/credits domain logic). Run 2026-08-19 on this host, against the
warm binary at `/dev/shm/tt/debug/tiller` (built from the checked-out `linux/gpui-waku`
worktree). I did not write this code and did not simply echo the ledger's existing
verdicts — every row below was re-exercised from scratch this pass, either by running the
actual test suite myself (not trusting a transcript of a previous run) or by driving the
live app through `Scripts/wayland-drive.sh` under my own isolated instance
(label `sweep16usg` / `sweep16usgB` / `sweep16usgC`, outdir `/dev/shm/sweep-16-F-CORE-USG`).

Contract text quoted below is `docs/linux-rewrite/02-inventory-packages.md`'s F-CORE-USG-01
through -09 entries — the ledger's evidence column is not a substitute for the row's own
VERIFY clause, so I went back to that source file rather than re-deriving the contract from
the ledger's prose.

## Setup used

```
cd rust && CARGO_TARGET_DIR=/dev/shm/tt CARGO_PROFILE_DEV_DEBUG=none cargo test -p tiller_usage
cd rust && CARGO_TARGET_DIR=/dev/shm/tt CARGO_PROFILE_DEV_DEBUG=none cargo test -p tiller_ui status_bar::tests
```
Both run **today, by me**, not copied from a prior transcript: 56/56 `tiller_usage` tests
pass (42 lib unit tests + 3 `p99_account_identity` + 1 `p99_codex_locations` + 10
`usage_tests.rs`), and 7/7 `tiller_ui::status_bar::tests` pass.

Live drive fixture: a throwaway garbage Codex OAuth credential file I wrote myself
(`/dev/shm/sweep16usg-codexhome/auth.json`, syntactically valid `tokens.access_token` /
`tokens.refresh_token` / `tokens.account_id`, all nonsense strings, `last_refresh` backdated
to `2026-08-01T00:00:00Z` — 18 days before this run, past the 8-day gate) under an isolated
`CODEX_HOME`, pointed at the real `auth.openai.com` (confirmed real egress before driving:
`curl -sS -o /dev/null -w '%{http_code}' https://auth.openai.com/` → real `403`, not a
network error). The real installed `claude` CLI (2.1.x, logged in as
`e.palmisano@reply.it`) and real installed `codex` CLI were both used unmodified — no
user-global config was written or touched; only the isolated `$CODEX_HOME` (env-scoped to
the driven process) and the worktree-local throwaway git fixture were used.

## Rows

| row | verdict | evidence |
|---|---|---|
| F-CORE-USG-01 | PASSED | `cargo test -p tiller_usage model::tests::` run by me today: `percent_is_clamped_on_construction`, `success_replaces_the_previous_state`, `unavailable_replaces_everything` — 3/3 pass. Read `model.rs:29`'s `from_percent`: routes `!percent.is_finite()` to `0` before `.clamp(0.0, 100.0)`, matching "clamped to 0–100" and the non-finite clause together. |
| F-CORE-USG-02 | PASSED | `claude::tests::` + `usage_tests.rs`, run by me today: `strip_ansi_removes_csi_and_osc`, `label_matching_follows_the_swift_patterns`, `percent_token_handles_used_left_and_bare`, `failure_markers_classify`, `a_well_formed_state_file_parses_into_the_expected_windows` (real captured `/usage` transcript), `a_real_capture_with_carriage_return_line_separators_parses`, `a_truncated_state_file_never_panics_and_loses_nothing_complete` — all pass. Live corroboration: the status bar showed real parsed values that *changed* across independent refreshes on my own instance — "Claude 42% 5h · 75% wk" → (after a refresh click) "Claude 43% 5h · 75% wk" (`/dev/shm/sweep-16-F-CORE-USG/04-03-after-auto-fetch.png`, `06-05-back-statusbar-claude-restored.png`) — a moving percentage rules out a cached/stand-in value. |
| F-CORE-USG-03 | PASSED | `ollama::tests::`, run by me today: `accepts_only_the_expected_usage_percent_field`, `extracts_the_first_unquoted_usage_percent_from_page_text`, `page_percents_above_100_are_clamped`, `garbage_is_not_usage` — pass. This is a parser-level, best-effort clause (the row's own text: "explicitly best-effort") and there is no signed-in Ollama Cloud account on this host to drive the live 200 leg through — same structural gap the ledger already discloses, not new. Live UI corroboration only at the edges: settings screen shows "Ollama Cloud … Show in usage bar: off" by default (`ollamaShowInBar:false` from a fresh `surface.settings.read`), consistent with no cookie present. |
| F-CORE-USG-04 | PASSED | `opencode_go::tests::`, run by me today: `normalizes_bare_and_prefixed_cookies`, `extracts_a_workspace_id_from_the_server_body`, `parses_a_react_flight_usage_page`, `garbage_is_not_usage`, `workspace_override_requires_content_after_trimming` — pass. Same structural note as USG-03: no signed-in OpenCode Go account on this host. Live UI corroboration: settings screen shows "OpenCode Go … Status: Not signed in … Show in usage bar: off" (`/dev/shm/sweep-16-F-CORE-USG/07-06-settings-providers.png`), matching `opencodeShowInBar:false` from socket readback. |
| F-CORE-USG-05 | PASSED | `codex::tests::loads_credentials_from_a_temp_auth_file`, `missing_credentials_are_not_signed_in`, `missing_or_old_refresh_times_need_refresh_after_eight_days`, `saving_refreshed_tokens_merges_and_preserves_other_fields`, `saving_writes_a_parseable_last_refresh_and_keeps_unknown_token_fields`, plus the dedicated `p99_codex_locations.rs::auth_file_precedence_load_refresh_and_missing_at_both_locations` (exercises both `$CODEX_HOME/auth.json` and `~/.codex/auth.json` in precedence order, empty-`$CODEX_HOME` fallback, both-tokens-required, and the 8-day gate at 7d/8d+1s boundaries) — all pass, run by me today. **Live, not just unit-level**: my own garbage-creds drive (`last_refresh` 18 days old) produced the app log line `[codex-usage] proactive token refresh failed: Expired` (`/dev/shm/sweep-16-F-CORE-USG/sweep16usg.log` and `sweep16usgB.log`, both independent invocations) — this is `needs_refresh()`'s 8-day gate actually firing at runtime in the real binary, not just a passing unit test, closing the gap `docs/linux-rewrite/DEAD-MODELS.md` raised on 2026-08-14 (that census predates the fix — the fix's own code comment at `codex.rs:375-377` says so, and I independently confirmed the call site is live). |
| F-CORE-USG-06 | PASSED | Unit: `token_refresh_failure_routes_to_distinct_usage_reasons`, `refresh_401_is_classified_by_its_provider_reason`, `a_real_401_response_is_classified_through_refresh_token_at` (local fixture) — pass today; plus `tiller_ui::status_bar::tests::token_refresh_reasons_render_distinct_text` — pass today. **Live, against the real `auth.openai.com`, driven and observed by me on this host**: garbage OAuth credentials → real 401 → the status bar read **"Codex token expired"** (the distinct copy, not the generic "logged out") — `/dev/shm/sweep-16-F-CORE-USG/zoom-statusbar-boot.png` (cropped zoom of the status bar) and the full frame `04-03-after-auto-fetch.png`. Reproduced twice, independently, across two separate app instances (`sweep16usg` and `sweep16usgB`). |
| F-CORE-USG-07 | PASSED | The ledger's `PASSED` (wave Q, commit `b0f6cc7b`, adding `TILLER_CODEX_USAGE_URL`/`usage_url()` mirroring the pre-existing `token_url()` seam) is real: I ran `codex::tests::a_real_200_response_completes_a_full_fetch_through_codexusagefetcher_fetch` and `a_real_refresh_success_merges_into_the_auth_file_without_losing_unrelated_fields` myself today — both pass, and both exercise the actual `CodexUsageFetcher::fetch()` production function, not a stand-in. This resolves the "half-proven" gap `FINISH-activity-usage.md`'s wave-D pass recorded (it could not reach the valid-credentials/200 leg on its host at all). The other three branches (missing credentials, refresh-needed/rejected-credentials, bearer/account header construction) are covered **live** on my own host by the USG-05/06 drives above: missing → my earlier check of `codex_auth_file_path()`'s resolution and the boot-time "Codex logged out" state in the timeout-lane run (`05-04-timeout-final.png`, real `~/.codex/auth.json` is apikey-mode → `LoggedOut`, no fixture); rejected/refresh-needed → the garbage-creds drive above, which only reaches `chatgpt.com/backend-api/wham/usage` with `Authorization: Bearer …`+`ChatGPT-Account-Id` if the request is well-formed enough to get a 401 back rather than a client-side failure. |
| F-CORE-USG-08 | PASSED | Unit: `usage_tests.rs`'s `not_installed_is_reachable_through_the_real_shell_when_claude_is_absent_from_path`, `logged_out_is_reachable_through_the_real_shell_with_a_fake_claude_on_path`, `error_is_reachable_through_the_real_shell_with_a_fake_claude_on_path`, `the_fetch_is_bounded_and_single_attempts_do_not_hang` — pass today. **Live success, against the real installed `claude` CLI**: "Claude 42% 5h · 75% wk" → "Claude 43% 5h · 75% wk" across refreshes (see USG-02 above). **Live timed-out, against the real `claude` binary, driven by me**: launched with `TILLER_USAGE_CLAUDE_TIMEOUT_MS=1200`, clicked refresh — status bar read **"Claude timed out"** (`/dev/shm/sweep-16-F-CORE-USG/05-04-timeout-final.png`). **PTY spawn/reap independently measured by PID, by me**: sampling `ps --ppid "$APP_PID"` once per second across the refresh click showed a `claude` child PID (`2724733`) present at t+1s/2s/3s and **gone** by t+4s/5s/6s, consistent with the 1.2s bound plus teardown — full transcript in the "children right after refresh click" block of this pass's drive output, reproduced verbatim in the section below. |
| F-CORE-USG-09 | PASSED | Unit: `tiller_usage::tests::provider_catalog_and_preferences_are_stable` (today); `tiller_ui::status_bar::tests::usage_bar_consumes_visibility_and_interval_preferences` and `a_real_timeout_after_a_real_success_dims_the_live_entity` (today, drawn against the production entity method, not the bare reducer). **Live, both conjuncts, driven end-to-end by me**: opened AI Providers, read `claudeShowInBar:"true"` via `ctl surface.settings.read`, clicked the "Show in usage bar" toggle at (1288,186) — visually flipped off (`03-02-providers-after-toggle.png`) and `surface.settings.read` immediately reported `claudeShowInBar:"false"`; navigated back — the Claude segment was genuinely absent from the status bar, leaving only "Codex token expired" (`04-03-back-statusbar-claude-hidden.png`); reopened settings, toggled back on, navigated back — Claude segment restored ("Claude 43% 5h · 75% wk", `06-05-back-statusbar-claude-restored.png`) and `claudeShowInBar` read back `"true"`. Both the socket state and the on-screen segment moved together in both directions. |

## Defects

None found this pass. All 9 rows hold up under independent re-drive; where the ledger's
prior verdict rested on a gap (USG-07's "half-proven" in `FINISH-activity-usage.md`, and
`DEAD-MODELS.md`'s "no live half" concern for USG-05's `needs_refresh`), I found the gaps
already closed by later work (commit `b0f6cc7b`'s URL-override seam; the proactive-refresh
call site at `codex.rs:383`) and reproduced the closure live myself rather than trusting the
ledger's account of it.

One adjacent observation, **not a defect in this section's rows**: the AI Providers screen
shows Codex's account **Status** as "Signed in" (`07-06-settings-providers.png`) at the same
moment the status bar reads "Codex token expired" for my garbage-creds drive. That is a
different signal (`account.rs`'s `codex_credentials_presence_reads_the_auth_file` — file
*presence*, not token *validity*) feeding a different row family (F-CORE-AUTH), and is
consistent with USG-07's own doc comment ("presence, not validity: the tokens may have
expired, the usage fetch answers that"). Flagging it here only so a reader of this file
doesn't mistake the two badges disagreeing for a bug in the usage rows.

## PTY PID transcript (F-CORE-USG-08 raw evidence)

```
children before refresh:
    PID COMMAND
2719428 npm exec @agent
[refresh click]
children right after refresh click (1/sec for 6s):
2719428 npm exec @agent
2724733 claude
2719428 npm exec @agent
2724733 claude
2719428 npm exec @agent
2724733 claude
2719428 npm exec @agent
2719428 npm exec @agent
2719428 npm exec @agent
children after settle:
    PID COMMAND
2719428 npm exec @agent
```
`2719428 npm exec @agent` is an unrelated pre-existing child of the driven app process (not a
usage-fetch PTY) and stays present throughout, which is exactly the point: only the `claude`
usage-fetch child (`2724733`) appears and is reaped, nothing else is disturbed.

## Codex refresh log lines (F-CORE-USG-05/06/07 raw evidence)

```
$ grep -i codex /tmp/sweep16usg.log /tmp/sweep16usgB.log
[codex-usage] proactive token refresh failed: Expired
[codex-usage] token refresh failed: Expired
```
(identical in both independent invocations — saved at
`/dev/shm/sweep-16-F-CORE-USG/sweep16usg.log` and `sweep16usgB.log`)

## What I could not reach, and why

- **USG-03/USG-04's live 200 leg** (a real signed-in Ollama Cloud / OpenCode Go session
  returning real usage data through the UI): this host has no credentials for either
  service. Not exercised live; the row's VERIFY clause itself is phrased at parser level
  ("Feed representative … payloads"), which the unit tests satisfy. Recorded as PASSED on
  that basis, matching the ledger's own prior reasoning (`DEAD-MODULES.md`'s note that
  `F-CORE-USG-03`'s clause is parser-scoped) — not blindly re-asserted, independently
  re-derived from the row's own text.
- One harness hiccup, **not an app defect**: a same-label back-to-back reinvocation
  (`TILLER_WL_KEEP=1` then immediately relaunching under the same label) produced a blank
  first frame with `MESA: error: ZINK: failed to choose pdev` in the app log — GPU/render
  contention from this heavily-loaded host (many concurrent critic instances), not a
  compositor or app fault. Worked around by using a fresh unique label for the next
  invocation, which then ran clean. No row's verdict rests on the failed invocation.

## Artefacts

- Screenshots and logs: `/dev/shm/sweep-16-F-CORE-USG/` (12 PNGs + 2 app logs; this is
  `/dev/shm`, not `/`, per the environment note about the runaway log).
- Fixtures used and left in place for replay: `/dev/shm/sweep16usg-fixture` (throwaway git
  repo), `/dev/shm/sweep16usg-codexhome/auth.json` (garbage OAuth creds).
- No application source was modified. No user-global config (`~/.codex/config.toml`,
  `~/.claude/…`) was touched — only an isolated `$CODEX_HOME` env var scoped to the driven
  process.
