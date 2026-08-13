# P58 → codex11 handoff: the persisted settings schema extension

**From pi (P58).** The settings surface now carries eleven values beyond the original
five Swift-parity keys; every one flows through `SettingsSnapshot` and reaches the host's
`on_change`. The `setting` key-value table is the right store and needs **no migration**
(new keys are new rows). What is missing is the typed contract in `tiller_persistence`:
`AppSettings` still has only the five Swift keys, so `settings()`/`save_settings()` cannot
carry the new values and a relaunch still drops them.

## The schema (decided once, in P58)

Extend `AppSettings` with these fields, read/write them in `settings()`/`save_settings()`
exactly like the existing five (unparseable stored value → default; values clamped into the
UI's own ranges so a value written by any version can always be rendered):

| field | key | type | default | clamp |
|---|---|---|---|---|
| `resume_agent_sessions` | `session.resumeAgentSessions` | bool | true | — |
| `auto_naming` | `general.autoNaming` | bool | false | — |
| `limit_chat_history` | `chat.limitHistory` | bool | true | — |
| `chat_retention` | `chat.retentionCount` | i64 | 100 | 5..=500 |
| `limit_mounted_worktrees` | `worktrees.limitMounted` | bool | false | — |
| `mounted_worktrees` | `worktrees.mountedCount` | i64 | 6 | 2..=50 |
| `summarizer_agent` | `general.summarizerAgent` | String | "claude" | one of `claude`/`codex`/`opencode`/`pi`/`omp` |
| `claude_show_in_bar` | `usage.claudeVisible` | bool | true | — |
| `codex_show_in_bar` | `usage.codexVisible` | bool | true | — |
| `opencode_show_in_bar` | `usage.opencodeVisible` | bool | false | — |
| `refresh_interval_min` | `usage.refreshIntervalMin` | i64 | 5 | 1..=60 |

Defaults are exactly what the surface draws today, so first-launch behavior is unchanged.
These keys are Linux-rewrite keys (the Swift app has no such settings), so the Swift-parity
tests on the original five keys stay untouched.

## The mapping that follows (in `tiller/src/main.rs`, pi's part of the handoff)

`settings_snapshot_from_app_settings` currently fills the new snapshot fields with
`..Default::default()`; `app_settings_from_snapshot` ignores them. Once the fields exist,
both functions map all eleven in both directions (the tests
`persisted_settings_map_to_the_ui_snapshot_and_back` in main.rs and
`settings_snapshot_defaults_match_the_persisted_contract` in tiller_ui already pin the
defaults). The `summarizer_agent` string ↔ `tiller_ui::settings::SummarizerChoice` mapping
uses `SummarizerChoice::id()` / `SummarizerChoice::parse`.

## Evidence P58 can already show

- `general_surface_settings_flow_into_the_persistence_contract` (tiller_ui, drawn): each
  toggle/stepper lands in the snapshot and reaches `on_change`.
- `summarizer_picker_is_gated_on_auto_naming_and_selects` (tiller_ui, drawn): picker gated
  on the auto-naming toggle; choice persists; Escape closes the menu.
- `usage_bar_consumes_visibility_and_interval_preferences` (tiller_ui, drawn): the bar
  consumes visibility + interval from the contract.
- `settings_visibility_toggles_reach_the_usage_bar` (tiller bin, drawn): end to end through
  the workspace observe link.

The relaunch round-trip (set → quit → relaunch → read back) is the remaining evidence and
cannot pass until this lands.
