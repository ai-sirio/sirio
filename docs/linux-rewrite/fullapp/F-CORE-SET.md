# F-CORE-SET — finish-line critic pass

Fresh, independent critic pass on **F-CORE-SET** (2 rows: settings-related domain logic,
`TillerCore`/`tiller_project` tier). Run 2026-08-19 against the warm binary at
`/dev/shm/tt/debug/tiller` (`tillerctl` at `/dev/shm/tt/debug/tillerctl`), driven live through
`Scripts/wayland-drive.sh` under label `critset21-*`. I did not write any of this code. I re-drove
both rows myself rather than trusting the ledger's recorded verdicts, and I disagree with the
ledger's F-CORE-SET-01 verdict — see below.

## Row text used as the contract (not the ledger's evidence column)

The ledger table itself carries no row description, only id/verdict/evidence/judged. The actual
contract text lives in `docs/linux-rewrite/02-inventory-packages.md`:

- **F-CORE-SET-01**: "Settings expose refresh interval clamped to 60–3600 seconds, control-socket
  enablement with `TILLER_SOCKET_ENABLE` environment overrides, resume/autoname/translucency
  options, session retention, mount cap, font sizes, sidebar, and right-panel widths. … VERIFY: Set
  defaults and environment values at each accepted/rejected boundary, restart, and inspect the
  effective settings." (SRC: `AppSettings.swift:3`)
- **F-CORE-SET-02**: "Permission state models macOS TCC permission kinds, reports statuses, and
  exposes refresh and perform-action callbacks. … VERIFY: Inspect each permission state and invoke
  its action, confirming the status is refreshed afterward." (SRC: `PermissionsModel.swift:8`)

The ledger currently scores F-CORE-SET-01 **PASSED** on evidence that only exercises ONE of the
row's eight named boundaries (the control-socket toggle). I drove all eight live this pass.

## Table

| row id | verdict | evidence |
|---|---|---|
| `F-CORE-SET-01` | FAILED — absent | Six of the row's eight named boundaries are robustly live-proven correct (see below); the other two — "sidebar, and right-panel widths" — are named explicitly in the row's own contract text and have **zero** live implementation anywhere: no persistence key, no UI control, no field in the live `surface.settings.read` payload. The only place `sidebar_width`/`right_panel_width` exist in the whole Rust tree is a vestigial, production-dead struct (see Defects). |
| `F-CORE-SET-02` | N/A — platform | Re-confirmed live. `rust/crates/tiller_ui/src/settings.rs:4251-4253`'s own test comment states the design intent explicitly ("the macOS-only TCC rows remain conditionally rendered"); a fresh screenshot of the live Permissions page (`set-general-perms/03-02-permissions.png`) shows only "Browser origin grants" — none of the six TCC kinds (Notifications/Screen Recording/Accessibility/Full Disk Access/Automation/Local Network) the reference app's `10-settings-permissions.png` renders. `grep -rn "TCC\|screenRecording\|fullDiskAccess" rust/crates --include=*.rs` matches nothing outside that one conditional-render comment — no Linux permission model exists to test. Matches the ledger's own verdict on this row (agreement, not a rubber stamp — independently re-driven). |

## What I actually drove (F-CORE-SET-01)

Setup: fixture repo at `/dev/shm/critset21-fixture`; instances under labels `critset21-off`,
`critset21-on`, `critset21-set`, DB `/tmp/critset21-set.sqlite` reused across three separate
process launches sharing that label (each a genuinely fresh process against the same on-disk
state, per `wayland-drive.sh`'s own design).

**1. Control-socket env-var boundary, both directions, fresh today** (not reused from any prior
pass):
- `TILLER_SOCKET_ENABLE=off`: `timeout 60 Scripts/wayland-drive.sh … 8` → exit path hit the
  30s "no control socket" wait (expected — no `.sock` file is ever created), and `$APP_LOG`
  printed `[control] disabled`. No `/tmp/critset21-off.sock` exists afterward (`ls` confirms).
- `TILLER_SOCKET_ENABLE` unset (default on): same binary, same recipe — `ctl system.ping` returned
  `{"ok":true,"result":{"pong":"true"}}` and a real screenshot rendered
  (`on-control/02-01-boot.png`, 4620 colours).

**2. Malformed/out-of-range *persisted* values, restart, inspect effective settings** — this is
the row's actual VERIFY clause, and the part the ledger's PASSED evidence never touched. Baseline
boot (`set-baseline/`) captured defaults via `ctl surface.settings.read`
(`interfaceFontSize:"13"`, `chatRetention:"100"`, `mountedWorktrees:"6"`, `refreshInterval:"5"`,
`theme:"system"`, `translucency:"false"`, `resumeAgentSessions:"true"`, `controlSocketEnabled:"true"`).
I then killed that process and wrote directly into `/tmp/critset21-set.sqlite`'s `setting` table
via Python's `sqlite3` module (table/column names read from `rust/crates/tiller_persistence/src/db.rs`):

| key | malformed value written | expected (from `settings_ranges`/parse fallback) | observed after a fresh relaunch |
|---|---|---|---|
| `appearance.uiFontSize` | `9999` (range 10–20) | clamp → `20` | `interfaceFontSize:"20"` ✓ |
| `appearance.terminalFontSize` | `abc` (unparseable) | fallback → `13` | `terminalFontSize:"13"` ✓ |
| `chat.retentionCount` | `-50` (range 5–500) | clamp → `5` | `chatRetention:"5"` ✓ |
| `worktrees.mountedCount` | `999` (range 2–50) | clamp → `50` | `mountedWorktrees:"50"` ✓ |
| `usage.refreshIntervalMin` | `0` (range 1–60, i.e. 60–3600s) | clamp → `1` | `refreshInterval:"1"` ✓ |
| `controlSocket.enabled` | `banana` (unparseable bool) | fallback → `true` | `controlSocketEnabled:"true"` ✓ |
| `session.resumeAgentSessions` | `nonsense` | fallback → `true` | `resumeAgentSessions:"true"` ✓ |
| `appearance.theme` | `not-a-theme` | fallback → `system` | `theme:"system"` ✓ |
| `appearance.translucency` | `maybe` | fallback → `false` | `translucency:"false"` ✓ |

Full raw JSON is in the transcript; screenshots corroborate visually, not just via the socket:
`set-malformed-restart/02-01-settings-after-malformed-restart.png` (Appearance page: Theme=System,
Translucency=off, Interface font=20pt, Terminal font=13pt — pixel-exact match to the clamped/
fallback numbers above) and `set-general-perms/01-general-malformed.png` (General page: "Resume
agent sessions on launch" ON, "Keep chats per worktree"=5, "Keep mounted"=50, "Control socket" ON —
same match). The app rendered cleanly at every step (4,591–16,955 distinct colours per frame,
never a blank/crashed frame) despite every numeric and boolean field being fed a deliberately
invalid persisted value. **No crash, no garbage passthrough, correct clamp-or-default on every
field I tested.** This is a decisively positive result for six of the row's eight named items —
stronger than the ledger's own PASSED evidence, which tested only the control-socket item.

**3. "sidebar, and right-panel widths"** — the two items the row's own text names alongside the
six above. I could not drive these because there is nothing to drive:
- `grep -rln "sidebar_width\|right_panel_width\|SidebarWidth\|RightPanelWidth\|panel_width" rust/crates`
  matches exactly one file: `rust/crates/tiller_project/src/settings.rs` — the `SettingsPolicy`
  struct's two fields.
- `grep -n "\.from_values()" -r rust/crates` matches exactly one call site: `SettingsPolicy`'s own
  unit test (`settings.rs:77`). `from_values()` — the method that clamps `sidebar_width`/
  `right_panel_width` (and, redundantly, `refresh_interval_secs`/`session_retention_days`/
  `mount_cap`/font sizes, which the *real* clamping in point 2 above does not route through) — is
  never called from `main.rs` or anywhere else in the live app. `SettingsPolicy` is wired into
  production for exactly one thing: `with_environment_override()`, the `TILLER_SOCKET_ENABLE`
  check (`crates/tiller/src/main.rs:11762-11767`).
  `rust/crates/tiller_persistence/src/model.rs`'s `AppSettings` — the struct that actually gets
  read from and written to SQLite (`db.rs::settings()`/`save_settings()`) — has **no**
  `sidebar_width`/`right_panel_width` field, and neither does the live `SettingsSnapshot`/
  `SettingsReport` that `surface.settings.read` serializes: every `ctl surface.settings.read`
  transcript above (three separate live calls) lists every settings key the app actually exposes,
  and none of them is a width. There is also no drag-to-resize affordance for the sidebar or Files
  panel anywhere in the UI (every screenshot this pass and in `reference/shots/` shows both at a
  fixed pixel width) — so this isn't "persisted but not yet exposed in the UI," it is a feature
  with no code path at all, macro to micro.

## Why I disagree with the ledger's PASSED

The ledger's F-CORE-SET-01 evidence ("wave B, x86 box, 2026-08-18") reads: "Closed twice:
direct-binary `TILLER_SOCKET_ENABLE=off/on` … AND the live UI toggle … same boundary." That
sentence is accurate as far as it goes, and I independently reproduced both halves fresh today —
but it only closes ONE of the row's eight named clauses. The row's own text is a conjunction
("refresh interval … control-socket … resume/autoname/translucency … session retention … mount
cap … font sizes … sidebar, and right-panel widths"), and this ledger's own convention elsewhere
(e.g. `F-CORE-FILE-01`, `F-CORE-FILE-08`) is to mark a row FAILED, not PASSED, when a named
sub-clause is demonstrably false or unimplemented even while other named sub-clauses hold. Six of
the eight items pass a real live, malformed-value, restart-and-inspect drive (stronger evidence
than the ledger's own PASSED carries); two are completely unimplemented. Net verdict: **FAILED —
absent**, not PASSED.

This does correct one thing in the other direction: several older documents in this tree
(`STALE-FAILED-CENSUS.md`, `docs/linux-rewrite/pins/INVENTORY-LEDGER.FABLE-07/08.md`,
`P108-reconciliation.md`) recorded this row as "FAILED — absent" on the theory that
resume/autoname/translucency/retention/mount-cap are *also* unpersisted, "report-only" fields.
That claim is now stale: `tiller_persistence::db::settings()`/`save_settings()` has real,
range-correct load/save/clamp logic for every one of those fields today, and I proved it live
above (malformed DB values → correct clamp-or-default on restart, not silent pass-through and not
a crash). Only the sidebar/right-panel width pair remains genuinely absent.

## Defects

1. **`F-CORE-SET-01` half-absent**: `sidebar_width`/`right_panel_width`, named explicitly in the
   row's own contract sentence, have no live implementation — no persisted key in
   `tiller_persistence::model::AppSettings`, no field in the live `SettingsSnapshot`/
   `SettingsReport`/`surface.settings.read` payload, no UI control, no resize affordance. The only
   trace of the concept anywhere in the Rust tree is two dead fields plus a dead clamp method
   (`SettingsPolicy::from_values`, called only from its own unit test) in
   `rust/crates/tiller_project/src/settings.rs:15-17,44-45`. Reproduction: `grep -rn
   "sidebar_width\|right_panel_width" rust/crates --include=*.rs` → one file; `grep -n
   "\.from_values()" -r rust/crates` → one call site (the test itself).

## What I could not reach and why

Nothing in this section was unreachable — both rows were fully driven live, and every named clause
in both rows' contract text was either directly exercised (F-CORE-SET-01's six live boundaries) or
positively confirmed absent by both code inspection and a live control-socket read (the two width
fields, and all six F-CORE-SET-02 TCC kinds). I did not additionally test
`general.autoNaming`/`usage.claudeVisible`-style booleans beyond the two I sampled
(`resumeAgentSessions`, `translucency`) — `parse_bool_setting`'s fallback logic is identical and
shared code for all of them, so I judged the sample sufficient rather than re-running nine
near-identical boolean probes.
