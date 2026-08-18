# FINISH — activity/usage shard (F-CORE-USG-*, F-USE-*)

Fresh finish-line critic pass, run **2026-08-18** on the box described in `ENVIRONMENT.md`'s
2026-08-18 top section (x86_64, 12 cores, COSMIC/wayland-1, AMD GPU). I built none of this. Every
row below was re-driven today under label `wd-actb`, live, against the real installed `claude`
2.1.234 CLI (logged in as e.palmisano@reply.it) and the real `codex` 0.147.0 CLI (logged out).
A prior `PASSED` is the hypothesis under test, not evidence — every row was re-exercised from
scratch; where I found a genuinely new discriminator I used it instead of repeating the old one.

## Setup

```bash
export TILLER_WL_LABEL=wd-actb
cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo build --manifest-path rust/Cargo.toml --workspace   # exit 0 after a transient race with a
                                                            # concurrent builder's own build in the
                                                            # shared rust/target self-resolved on retry
cp rust/target/debug/tiller /tmp/wd-actb-tiller
export TILLER_WL_BIN=/tmp/wd-actb-tiller
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1
```

DB is `/tmp/wd-actb.sqlite`; `wayland-drive.sh` reuses it across invocations sharing the label, so
settings toggled in one invocation (e.g. AI Providers visibility) are read back by the next.

## A lane trap this pass found and worked around: `shot()`'s resize dance clamps the pointer above y≈900

`Scripts/wayland-drive.sh`'s `shot()` forces a repaint by shrinking the output to `1400x900` and
back to `1715x972` before every capture (documented, intentional — see the "Repaint is lazy"
comment). What is **not** documented: shrinking the output clamps the persistent virtual pointer's
absolute position into the smaller extent, and growing back does **not** re-issue a motion event to
restore it — the compositor leaves the cursor wherever it got clamped. Measured precisely with
pixel-exact cursor-tip detection (`convert ... txt:-`, topmost white pixel): `move 135 952` followed
by `shot` renders the cursor at real `y≈901` every time, and any requested `y` from 900 through 972
lands at the same ~901; `y≤900` passes through unclamped (measured accurate to ±2px). The status
bar's usage segments sit at `y≈928–955` — inside the clamped-away band — so **no on-screen hover
state below y≈900 survives a `shot()` call**, which made F-USE-02's tooltip unreachable through the
documented `move`/`shot` vocabulary alone.

**Click is unaffected** — a `click`'s button event fires immediately at the true requested
coordinates, before any later `shot()` resize; only the pointer's *visual* position at the next
capture is clamped. Confirmed independently: `click 58 952` genuinely triggered the refresh action
(content changed) even though the following screenshot showed the cursor sprite at y≈901.

**Workaround used throughout this pass**: `move` to the real target, then capture with a raw
`grim -o HEADLESS-1 "$OUTDIR/name.png"` issued directly inside the action block (both `$OUTDIR` and
the app's own Wayland connection stay in scope there), skipping `shot()`'s clamp-inducing dance
entirely. Confirmed the manual capture is not a stale cached frame (F-USE-02 evidence below shows
content that could only come from a fresh, correctly-timed frame: a tooltip with the *current*
percentage, and a second tooltip with different text on a different segment).

## Rows

### F-CORE-USG-01 — PASSED
VERIFY: clamp 0–100, non-finite→0, `has_any`, reducer states.
`cargo test -p tiller_usage model::tests::` today: `percent_is_clamped_on_construction`,
`success_replaces_the_previous_state`, `unavailable_replaces_everything` — 3/3 pass. Source
(`model.rs:29`) shows `from_percent` explicitly routes `!percent.is_finite()` to `0` before
clamping, matching the clause.

### F-CORE-USG-02 — PASSED
VERIFY: ANSI-stripped session/weekly/Fable label parsing, percent-left inversion, malformed input.
`cargo test -p tiller_usage claude::tests::` + `usage_tests.rs`: `strip_ansi_removes_csi_and_osc`,
`label_matching_follows_the_swift_patterns`, `percent_token_handles_used_left_and_bare`,
`failure_markers_classify`, `a_well_formed_state_file_parses_into_the_expected_windows` (against a
real captured `/usage` transcript from this machine), `a_real_capture_with_carriage_return_line_separators_parses`,
`a_truncated_state_file_never_panics_and_loses_nothing_complete`,
`a_state_file_with_unexpected_json_shape_is_handled_not_parsed` — all pass today.

### F-CORE-USG-03 — PASSED
VERIFY: Ollama Cloud best-effort single field.
`ollama::tests::accepts_only_the_expected_usage_percent_field`,
`extracts_the_first_unquoted_usage_percent_from_page_text`, `page_percents_above_100_are_clamped`,
`garbage_is_not_usage` — pass today.

### F-CORE-USG-04 — PASSED
VERIFY: cookie normalization, workspace id, rolling/weekly/monthly, reset-seconds conversion.
`opencode_go::tests::normalizes_bare_and_prefixed_cookies`,
`extracts_a_workspace_id_from_the_server_body`, `parses_a_react_flight_usage_page`,
`garbage_is_not_usage`, `workspace_override_requires_content_after_trimming` — pass today.

### F-CORE-USG-05 — PASSED
VERIFY: load from `$CODEX_HOME`/`~/.codex`, require both tokens, merge-save, 8-day refresh gate.
PLATFORM clause (XDG-compatible Linux location) covered by the same precedence test.
`codex::tests::loads_credentials_from_a_temp_auth_file`, `missing_credentials_are_not_signed_in`,
`missing_or_old_refresh_times_need_refresh_after_eight_days`,
`saving_refreshed_tokens_merges_and_preserves_other_fields`,
`saving_writes_a_parseable_last_refresh_and_keeps_unknown_token_fields`, and the dedicated
`p99_codex_locations.rs::auth_file_precedence_load_refresh_and_missing_at_both_locations` (both
`$CODEX_HOME/auth.json` and `~/.codex/auth.json`, in precedence order) — all pass today. Also
exercised live today as a side effect of F-CORE-USG-06/07 below: `codex_auth_file_path()` correctly
resolved `$CODEX_HOME` in the real running binary.

### F-CORE-USG-06 — PASSED (independently re-driven live today, not just unit tests)
VERIFY: token refresh 401→reused/revoked/expired classification.
Unit: `token_refresh_failure_routes_to_distinct_usage_reasons`, `refresh_401_is_classified_by_its_provider_reason`,
`a_real_401_response_is_classified_through_refresh_token_at` (local fixture) — pass today. Plus
`tiller_ui`'s `status_bar::tests::token_refresh_reasons_render_distinct_text` (UI-tier, drawn) —
pass today.

**Live, against the real `auth.openai.com`** (not a fixture — confirmed network egress reaches it:
`curl auth.openai.com` → real `403`): wrote a syntactically-valid but garbage OAuth credential file
(`access_token`/`refresh_token`/`account_id` all nonsense, `last_refresh` backdated to
2026-08-01) to an isolated `CODEX_HOME=/tmp/wd-actb-codexhome`, launched the pinned binary with that
env, clicked refresh. The real ChatGPT backend rejected the garbage access token, the app's reactive
refresh then hit the real `auth.openai.com` with the garbage refresh token, got a real 401, and the
status bar showed **"Codex token expired"** — the distinct copy, not the generic "logged out".
Screenshot crop: `/tmp/wd-actb-shots/codex-401-zoom2.png`. This is today's own independent
reproduction of the prior sweep's claim, through the unmodified production path.

### F-CORE-USG-07 — half-proven
VERIFY: full fetch — load, refresh-as-needed, call backend with bearer/account headers, parse,
expose logged-out/error.

**Proven live today, through the real unmodified `CodexUsageFetcher::fetch()`, not a stand-in:**
- *missing credentials*: this machine's real `~/.codex/auth.json` is `apikey`-mode (no `tokens`
  object Tiller's OAuth loader needs) → `load_credentials()` fails → the live app genuinely shows
  "Codex logged out" on first boot, no fixture involved.
- *rejected credentials* (the reactive-refresh branch): the F-CORE-USG-06 drive above exercises this
  exact branch of `fetch()` end-to-end against the real backend — `fetch_usage` gets a real 401,
  triggers `refresh_token`, which also gets a real 401, correctly classified and rendered.
- *bearer/account headers built correctly*: confirmed by reading `fetch_usage` (`codex.rs:210`) —
  `Authorization: Bearer <token>` and `ChatGPT-Account-Id` are both present in the real request that
  produced the real 401 above (a malformed header would not have reached OpenAI's auth layer to
  produce a 401 at all — it would have failed as a client-side error instead).
- *parsing*: `codex::tests::parses_the_real_wham_response` and `parses_a_secondary_window_when_present`
  run today against a real captured wham response body.

**Not reachable this pass**: the *valid credentials → `Success`* branch. `USAGE_URL`
(`chatgpt.com/backend-api/wham/usage`) has no test-seam override analogous to `token_url()`'s
`TILLER_CODEX_TOKEN_URL`, and this environment's Codex CLI is genuinely logged out (no working
account to authenticate a real request). This is a structural gap in the row, not a budget one: the
200-response path through `fetch()` cannot be driven without either a real signed-in Codex account
or a code change adding a URL override, and I did not make that change (out of scope for a critic
pass). Naming it precisely: 3 of 4 named branches (missing / refresh-needed·rejected / header
construction) are proven live through the real function; the `Success` branch is unreachable in
this environment.

### F-CORE-USG-08 — PASSED
VERIFY: hidden login-shell PTY, 2s settle, `/usage`, palette confirmation, 25s-bounded
success/not-installed/logged-out/error/timed-out. PLATFORM clause (Linux PTY + shell strategy,
not macOS `/bin/zsh`) — source (`claude.rs:488` `login_shell()`) reads `$SHELL` first, falls back
through `/bin/bash`/`/bin/sh`/`/usr/bin/bash`; this machine's real `claude` PTY spawn used `bash`
(confirmed in the "Terminal: wd-actb-tiller" panes captured this pass, which all show `bash`).

- *not-installed / logged-out / error*: `usage_tests.rs::not_installed_is_reachable_through_the_real_shell_when_claude_is_absent_from_path`,
  `logged_out_is_reachable_through_the_real_shell_with_a_fake_claude_on_path`,
  `error_is_reachable_through_the_real_shell_with_a_fake_claude_on_path` — real shell, fake `claude`
  on `$PATH`, pass today.
- *bounded/timed-out*: `the_fetch_is_bounded_and_single_attempts_do_not_hang` (real shell, `TILLER_USAGE_NO_DOTFILES`) pass today.
- *success, live, against the real installed `claude` 2.1.234, logged in*: the running app's status
  bar showed real fetched values that changed between independent refreshes — "Claude 36% 5h · 32%
  wk" → after a manual refresh-icon click, "Claude … / Codex …" (loading) → "Claude 36% 5h · 33% wk"
  → later "Claude 37–40% 5h · 33% wk" across further refreshes. Percentages moving between fetches
  rules out a cached/stand-in value; this is the real 25s-bounded PTY path completing well inside
  budget against a real Claude Max account.
- *timed-out, live, against the real `claude` binary* (not a fake PTY — using the documented
  `TILLER_USAGE_CLAUDE_TIMEOUT_MS` instrument, legitimate per this pass's brief): launched fresh with
  `TILLER_USAGE_CLAUDE_TIMEOUT_MS=1200`; the real `claude` PTY could not render its prompt and
  respond inside the shortened budget, and the status bar read **"Claude timed out"**
  (`/tmp/wd-actb-shots/timeout-check-zoom.png`).
- *hidden PTY termination on completion and on timeout, live, both measured by PID*: watched the
  pinned `tiller` process's direct children (`ps --ppid <pid>`) across a real successful fetch — a
  `claude` child PID appeared, and 6s later was **gone** (confirmed both `ps -p <pid>` empty and the
  process absent from the children list), while a separate, genuinely long-lived interactive
  "Claude Code" agent tab's own `claude` child PID stayed alive throughout, proving the fetcher's
  `Drop for Pty` (`child.kill()` + `child.wait()`) actually reaps the *right* process and does not
  touch unrelated panes.

### F-CORE-USG-09 — PASSED
VERIFY: Claude/Codex/OpenCode Go/Ollama Cloud catalog with per-provider prefs; stale-on-timeout
reducer.
`tiller_usage::tests::provider_catalog_and_preferences_are_stable` pass today. `tiller_ui`'s
`status_bar::tests::usage_bar_consumes_visibility_and_interval_preferences` (drawn, UI-tier) and
`a_real_timeout_after_a_real_success_dims_the_live_entity` (drawn, real `Loaded`→`Stale` transition
through the production entity method, dimming flag asserted) pass today. Live today: toggled
Claude's "Show in usage bar" off in AI Providers (`ctl surface.settings.read` before/after showed
`claudeShowInBar` flip `true`→`false`), navigated back, and the Claude segment was genuinely absent
from the bar (only "Codex logged out" remained) — then toggled back on and confirmed restored.

## F-USE rows

### F-USE-01 — PASSED
VERIFY: settings gear, refresh action, current worktree info in the bottom bar.
Live: baseline screenshot shows the gear icon, the refresh (circular-arrow) icon, and (bottom-right)
"linux/gpui-waku · ~/Scrivania/Progetti/tiller-linux" — the open worktree's branch and path.
Clicked the refresh icon at (58,952): the very next frame showed both provider segments flip to
"Claude …" / "Codex …" (loading, dimmed) — a real, immediate effect, not a static icon.

### F-USE-02 — PASSED
VERIFY: configure provider visibility in AI Providers, return to workspace, confirm enabled
segments and unavailable tooltips match settings/status.
Live, both conjuncts exercised:
- **Visibility**: see F-CORE-USG-09 above — toggled in AI Providers, confirmed via socket readback
  *and* visually in the bar.
- **Tooltips, both a loaded and an unavailable segment** (using the raw-`grim` workaround above to
  dodge the pointer-clamp trap): hovering the "Claude 38% 5h · 33% wk" segment produced a tooltip
  reading the identical text "Claude 38% 5h · 33% wk"
  (`/tmp/wd-actb-shots/manual-hover-claude-zoom.png`); hovering the "Codex logged out" segment (a
  genuinely `Unavailable(LoggedOut)` state, this machine's real Codex CLI) produced a tooltip
  reading "Codex logged out" (`/tmp/wd-actb-shots/manual-hover-codex2-zoom.png`) — two different
  tooltips over two different segments, each matching its own segment's own text, rules out a
  static/stuck tooltip.

### F-USE-03 — PASSED
VERIFY: configured, loading, stale, logged-out, failed display states.
- *configured* (Loaded): live, "Claude 36–40% 5h · 32–34% wk" across several real refreshes (USG-08
  above).
- *loading*: live, refresh click → "Claude … / Codex …" (dimmed ellipsis) next frame.
- *logged-out*: live, real Codex CLI, "Codex logged out".
- *timed-out*: live, `TILLER_USAGE_CLAUDE_TIMEOUT_MS=1200` → "Claude timed out" (USG-08 above), plus
  the drawn `a_real_timeout_after_a_real_success_dims_the_live_entity` test (production entity
  method, `Loaded`→`Stale` with the dimming flag asserted, not just the reducer function in
  isolation).
- *error/other unavailable reasons render distinct text*: `unavailable_reasons_render_distinct_text`
  (pure-function, all 4 reasons + Loaded + Stale checked against exact expected strings) pass today.

### F-USE-04 — PASSED (independently re-driven against the real StatusNotifierWatcher)
VERIFY: click the tray item, popover lists active agents or "No active agents", includes "Quit
Tiller".
Live: found the real registered service `org.kde.StatusNotifierItem-<pid>-1` via
`busctl --user list` against this machine's actual `org.kde.StatusNotifierWatcher`
(`cosmic-applet-s...`, PID 3152 — the real COSMIC panel's tray host, not a stand-in). After putting
one real "Claude Code" agent pane into `needs-input` via a genuine transition,
`busctl --user call ... /MenuBar com.canonical.dbusmenu GetLayout` returned a real menu: item 1
`"linux/gpui-waku — tiller (needs input)"`, item 2 separator, item 3 `"Quit Tiller"` — the live
roster row reflects the real status, not a placeholder.

### F-USE-05 — PASSED (via the real dbusmenu click, not the `ctl tray.jump` shortcut)
VERIFY: click a roster row, Tiller reopens/selects that worktree and activates the relevant tab.
Live, stronger proof than a prior sweep's `ctl tray.jump` drive: switched the app to a **different**
worktree (`wl-proof-branch`) over the control socket, then sent a genuine
`com.canonical.dbusmenu.Event(id=1, "clicked", ...)` D-Bus call directly at the real registered
StatusNotifierItem service — the exact call a real desktop tray sends on a real click, with no `ctl`
convenience method involved. `workspace.current` afterward reported back on `linux/gpui-waku`, and
`panel.list` showed the `needs-input` "Claude Code" pane as `active:true` — the jump landed on the
worst-status tab of the target worktree, through the production `select_worktree_and_jump` path,
driven from the actual menu-click entry point.

### F-USE-06 — PASSED
VERIFY: start an agent, hide/switch away from its pane, cause a status transition, confirm a user
notification is delivered.
Live: opened a real "Claude Code" agent tab (spawn-owned, `agent_spawned` registers `agent_id`),
switched the active tab to "Chat" (making the Claude Code pane genuinely not visible), then sent
`ctl notify session=<pane> status=needs-input`. Watched the real session D-Bus bus with
`dbus-monitor --session "interface='org.freedesktop.Notifications',member='Notify'"` (sanity-checked
first with a manual `notify-send` call, which the monitor caught) and captured a genuine method call
from **app-name `"Tiller"`**, title `"Claude Code — tiller/linux/gpui-waku"`, body
`"linux/gpui-waku · tiller"` — exactly the production `post_desktop_notification` payload shape
(`<agent> — <worktree label>` / `<branch> · <project>`), fired through the real `notify-send`
binary at the real desktop notification daemon, not a log line or a stubbed call.

Earlier attempts against a **plain terminal pane** (no registered `agent_id`) correctly produced
*no* notification — `post_activity_notification` early-returns when `self.activity.agent_id(pane)`
is `None`. This is expected: a bare terminal is not an agent pane, and the guard is doing its job,
not failing silently. Re-driving against a real agent-spawned pane produced the notification
immediately once the transition was genuinely non-visible.

## The open cross-cutting question from another critic today

Another critic's note: chat context-usage never reported above single digits, leaving unsettled
whether Tiller fails to populate the breakdown or the ACP bridge never reports it. **This machinery
is not what my rows cover.** `F-CORE-USG-*`/`F-USE-*` are the *provider* usage bar
(`ProviderUsage`/`UsageBarView` — Claude/Codex/OpenCode Go/Ollama Cloud session-and-weekly
percentages, fetched by the mechanisms above), which is a different system from the *chat pane's*
per-conversation context-window breakdown that critic is asking about. I did not exercise or touch
that second system this pass. One incidental data point, not a settlement: while driving F-USE-05 I
had a real interactive `claude` CLI open in a terminal pane, and *its own* statusline (the raw CLI's
own rendering, not Tiller's chat UI) showed `Context: [...] 0/1.0M (0%)` — i.e. the CLI itself
reports near-zero context on a fresh session, which is expected and unrelated to whether Tiller's
own chat-pane breakdown UI populates correctly. Someone driving the F-CHAT rows would need to check
that separately.
