# P106 report — codex12 slice

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`  
Branch: `linux/gpui-waku`  
Operator: `codex12`  
Lane: nested headless Wayland, with `project.*`, `surface.*`, and `tab.*` control calls. Each `shot` forced a resolution change before capture. The lane has no input devices, so clicks, right-clicks, typing, and drags are recorded as owed when they are part of a row.

This is an observation log, not a verdict sheet. The captures are in `/tmp` and are named below.

## Rows 1–10

### F-PRJ-04

Drove the duplicate insertion path after adding the same non-Git directory once:

```text
ctl project.add path=/tmp/codex12-plain
ctl project.add path=/tmp/codex12-plain
ctl project.list
```

The second call returned:

```json
{"added":"false","projectId":"p-8636772848d55625","worktreeCount":"0"}
```

`project.list` still contained one `codex12-plain` project. The forced capture showed the project row but no visible `sidebar-notice` or “already tracked or nested: …” text: `/tmp/codex12-p1-shots/03-prj04-duplicate.png`. This is the control-socket route; the picker-driven UI insertion gesture remains owed: open `+`, choose `Open Project…`, and submit a duplicate/nested path.

### F-PRJ-07

The Wayland lane has no control call that opens the `Clone Repository…` form. The baseline capture showed the sidebar `+` affordance only: `/tmp/codex12-p1-shots/01-baseline.png`. Owed gesture: click `+`, click `Clone Repository…`, submit a failing clone, then observe the error text and `Retry clone` relabel.

### F-PRJ-10

The Wayland lane has no control call that opens the `Create Project…` form. The same baseline showed the `+` affordance but no form: `/tmp/codex12-p1-shots/01-baseline.png`. Owed gesture: click `+`, click `Create Project…`, submit a failing creation, then observe the error text and `Retry creation` relabel.

### F-CHAT-21

Drove the chat surface and a safe ACP prompt:

```text
ctl project.add path=/home/enzopalmisano/Scrivania/Progetti/tiller-linux
ctl surface.chat.open
ctl tab.select index=1
ctl surface.chat.send surfaceId=default-chat text=pwd
ctl surface.chat.read surfaceId=default-chat
```

The final socket snapshot contained a user entry, one completed tool entry (`text: "pwd"`), an assistant entry, and `EndTurn`; it contained no `Thought` entry. The UI capture after completion remained transcript-empty while the composer was visible: `/tmp/codex12-p4-shots/04-chat-after-pwd.png`. No Thought expand/collapse control was therefore reached. Owed gesture: click the Thought disclosure when a Thought entry is present.

### F-CHAT-22

The same ACP run produced no grouped-step or older-turn group entry—only the user/tool/assistant/turn records described above. The forced capture showed no grouped-step affordance: `/tmp/codex12-p4-shots/04-chat-after-pwd.png`. Owed gesture: exercise the group disclosure against a transcript containing grouped steps or an older turn.

### F-SET-11

The normal-path baseline rendered loaded usage segments in the status bar: Claude showed a percentage/5-hour window and Codex showed a percentage/5-hour window: `/tmp/codex12-p2-shots/01-baseline.png`. I also opened Settings, which returned the full settings state over the socket. I did not produce Loading, Stale, or the four unavailable usage reasons through a control call; those transitions remain unexercised.

### F-SET-24

Drove the permissions surface:

```text
ctl surface.settings.open
ctl surface.settings.select section=permissions
ctl surface.settings.read
```

The response reported `sectionId:"permissions"`. The forced capture rendered “Browser origin grants”, a visible `Revoke all` control, and the empty state “No browser origins have been granted.”: `/tmp/codex12-p2-shots/04-settings-permissions.png`. No origin was present, so per-origin Revoke and an effective Revoke-all action were not exercised. Creating a browser grant is also outside this Wayland lane because browser content does not render there.

### F-PRJ-03

Drove the non-Git add route:

```text
ctl project.add path=/tmp/codex12-plain
ctl project.list
```

The response was `{"added":"true", ... "worktreeCount":"0"}` and `project.list` reported `isGit:"false"`. The forced capture showed `codex12-plain` and `/tmp/codex12-plain` in the sidebar, with no Initialize / Add without Git / Cancel prompt: `/tmp/codex12-p1-shots/02-prj03-after-add.png`. This socket add is not the browse-time picker gesture named by the row. Owed gesture: click `+` → `Open Project…` and choose the non-Git directory.

### F-PRJ-11

The project row was visible after `project.add` in `/tmp/codex12-p3-shots/02-chat-open-before-select.png`. Opening Project Settings requires a project-row context-menu gesture; no Wayland input device was available. Owed gesture: right-click the project row, choose `Project Settings`, and then exercise the sheet’s project removal control. No sheet or removal control was visually reached.

### F-PRJ-12

The same project-row context-menu route was not reachable in this lane. Owed gesture: right-click the project row, choose `Project Settings`, edit the display-name field, and exercise the repository-type switch. The project/sidebar state was visible in `/tmp/codex12-p3-shots/02-chat-open-before-select.png`; no live sheet observation was obtained.

## Rows 11–20

### F-PRJ-17

The project/sidebar capture showed `New Worktree…` but no worktree-location choice surface: `/tmp/codex12-p3-shots/02-chat-open-before-select.png`. There is no socket call in this lane that opens the New Worktree prompt. Owed gesture: click `New Worktree…` and exercise each default-worktree-base option (current, pinned, primary, and no-primary), then observe the resulting parent path.

### F-PRJ-18

No socket-addressable custom worktree-location or default-parent control was reached. The same capture showed the worktree rows and `New Worktree…` entry only: `/tmp/codex12-p3-shots/02-chat-open-before-select.png`. Owed gesture: open `New Worktree…`, look for the location chooser/default-parent control, and exercise it with a custom location.

### F-CHAT-02

The healthy ACP route was driven with:

```text
ctl surface.chat.open
ctl tab.select index=1
ctl surface.chat.send surfaceId=default-chat text=pwd
ctl surface.chat.read surfaceId=default-chat
```

The send completed and returned a completed `pwd` tool entry plus an assistant response; no authentication-required error, auth banner, CLI guidance, or Retry state appeared in the socket response or capture `/tmp/codex12-p4-shots/04-chat-after-pwd.png`.

The Retry half was not triggered because this healthy request did not fail. The authentication half was not reached; the exact auth-failure fixture/condition and its Retry action remain owed.

### F-CHAT-16

The socket-opened Chat surface rendered the model pill `Opus (1M context)` and the composer, but no control call opens the model-picker popover. Captures: `/tmp/codex12-p3-shots/03-chat-selected.png` and `/tmp/codex12-p4-shots/04-chat-after-pwd.png`.

The open/select half was not pointer-exercised. Owed gesture: click the model pill, type a search, observe a no-match result, inspect any Recommended labels, and select a model. The Wayland lane cannot deliver the required click or typing.

### F-CHAT-18

The socket-opened Chat surface showed the `0%` context indicator and `Opus (1M context)` in the composer: `/tmp/codex12-p4-shots/04-chat-after-pwd.png`. No socket call opens the context popover, so the percent/tokens/cost display was not separately reached. Owed gesture: click the context indicator and inspect its percent, token, and cost lines; the input/output/cache breakdown was not observed.

### F-CHAT-23

The final `surface.chat.read surfaceId=default-chat` snapshot contained a completed tool row:

```json
{"id":"toolu_01VqABhxRGxJAWFvrYh2EvPv","kind":"tool","status":"Completed","text":"pwd"}
```

The forced UI capture still showed an empty transcript, so no expand or output-inspection affordance was visually reached: `/tmp/codex12-p4-shots/04-chat-after-pwd.png`.

The expand/output half is therefore only present in the socket snapshot, not visually exercised. No Dismiss control appeared in the snapshot or capture; its gesture remains owed.

### F-CHAT-31

The only tool content generated by the safe ACP probe was `pwd`; it contained no diff lines, locations, or file-open target. The UI capture remained transcript-empty: `/tmp/codex12-p4-shots/04-chat-after-pwd.png`. No diff preview or location-chip/open-file gesture was reached. Owed exercise: produce a safe tool diff, open the preview, and activate a file location.

### F-CHAT-34

`surface.chat.open` returned `surfaceId=default-chat` and an empty transcript, and `tab.select index=1` brought Chat forward. The capture showed the Chat tab and composer but no history browser: `/tmp/codex12-p3-shots/03-chat-selected.png`. No socket call opens an arbitrary chat-history list. Owed gesture: open the chat history menu, browse/open a retained session, and exercise delete with confirmation.

### F-SET-15

The AI Providers section was opened and captured:

```text
ctl surface.settings.open
ctl surface.settings.select section=ai-providers
```

The capture showed Claude Code and Codex each with an `Accounts` area containing `System default`, `This device`, `Active`, and `Add Account`; no second account row or account-selection control was present in the visible state: `/tmp/codex12-p2-shots/06-settings-ai-providers.png`.

The single system-default/active state was observed. The multi-account selection half was not reachable because `Add Account` and selection require pointer interaction.

### F-SET-16

The Agents section was opened and read over the socket:

```text
ctl surface.settings.open
ctl surface.settings.select section=agents
ctl surface.settings.read
```

Normal PATH capture: `/tmp/codex12-p2-shots/03-settings-agents.png`. It visibly contained a `Search agents` field and `Refresh` button. With `PATH=/usr/bin:/bin`, the same surface rendered all five rows as `Not found on PATH`, with the text `Not installed — install the … CLI to use it.`: `/tmp/codex12-p6-shots/02-agents-reduced-path.png`.

The search and refresh controls were visible but not clicked or typed into because Wayland has no input devices. No updated timestamp was visible in either capture. Thus the search/refresh half was state-observed but gesture-owed; the timestamp half was not observed.

## Rows 21–22

### F-SET-18

The reduced-PATH Agents run was driven with:

```text
PATH=/usr/bin:/bin
ctl surface.settings.open
ctl surface.settings.select section=agents
ctl surface.settings.read
```

All five provider rows rendered a red `Not found on PATH` badge and the corresponding text `Not installed — install the … CLI to use it.`: `/tmp/codex12-p6-shots/02-agents-reduced-path.png`. The normal-PATH capture showed available providers and ACP badges: `/tmp/codex12-p2-shots/03-settings-agents.png`.

No Install, installation-progress, Update-to-latest, or failed-Retry control was visible in either state. The availability/status half was exercised; the action controls named by the row were not present in the rendered state.

### F-SET-21

Drove the Appearance section:

```text
ctl surface.settings.open
ctl surface.settings.select section=appearance
ctl surface.settings.read
```

The response reported `fileIcons:"Material"`. The capture rendered a File icons control showing only `Material`; no second selectable icon set was present: `/tmp/codex12-p2-shots/07-settings-appearance.png`. The chooser surface was visible, but no segment click could be delivered and no icon-set change was observed. The second-option/change half remains owed to a lane with pointer input.

## codex11 — core, usage, control, and agents

Operator: `codex11`. The requested Wayland lane was attempted with label `p106-c11`, but sway
could not bind a nested `wayland-N` socket (`Unable to open wayland socket`); it produced no
usable frame. A private X11 app instance on `DISPLAY=:1` was then driven only through
`/tmp/p106-c11.sock`, with `/tmp/p106-c11.sqlite`. No shared-display pointer, keyboard, or
capture was used. The X11 app process was live for the socket exercises below.

### Rows 1–10

### F-BRW-08

Drove:

```text
surface settings open --section permissions
surface settings read
```

Both CLI calls returned `settings\tPermissions`. The raw `surface.settings.open` response was
`{"ok":true}` with `sectionId:"permissions"`, `section:"Permissions"`, and the five available
sections including Permissions. It contained no browser-origin rows or grant list. There is no
control-socket action for per-origin Revoke or Revoke all, and no browser grant was created, so no
revoke effect or prompt reappearance was observed. No capture: the Wayland compositor failed
before the first frame, and this row's browser-content path is outside the headless X11 evidence.

### F-USE-03

Drove:

```text
surface settings open --section ai-providers
surface settings read
```

The raw response reported Claude Code, Codex, OpenCode, Pi, and Oh-My-Pi as `available:"true"`
and `status:"Available"`; it reported `claudeShowInBar:"true"`, `codexShowInBar:"true"`, and
`opencodeShowInBar:"false"`. This is provider/settings state, not a usage-bar readback: the
control socket exposes no usage-state injection or status-bar read method. Loading, stale,
logged-out, and error transitions were not reached live.

The `tiller_usage` package run exercised the existing parser/outcome tests, including
`codex::tests::refresh_401_is_classified_by_its_provider_reason`, credential-file loading, stale
credentials, missing credentials, bounded fetch, and unavailable-state replacement; the package
reported 27 unit, 3 account, 1 location, and 7 usage integration tests passed. A narrow
`tiller_ui` status-bar test could not start because an unrelated concurrent edit in
`rust/crates/tiller_ui/src/chat.rs` does not initialize/match the new `Entry::Permission.dismissed`
field (two compile errors before tests ran). No UI capture was obtained.

### F-CORE-USG-06

The built classification path was exercised by the existing
`refresh_401_is_classified_by_its_provider_reason` test in the `tiller_usage` run. The live app
has no control method for substituting controlled OAuth responses. Static reachability showed
`CodexTokenRefresher`'s production path posts to the fixed `https://auth.openai.com/oauth/token`
through `tiller_usage/src/http.rs::post`, which shells out directly to `curl`; no injectable
transport or test-server URL is available. I did not refresh or rotate the user's real Codex
tokens. Controlled success, reused, revoked, expired, and other-error HTTP observations were
therefore not obtained from the running app. No capture.

### F-CORE-USG-07

The same `tiller_usage` run exercised credential loading, missing/old refresh timestamps, real
usage parsing, bounded fetch, and unavailable/error reducer tests. The private app's Settings
socket response exposed provider availability but no usage-fetch control or outcome injection.
`CodexUsageFetcher::fetch` reaches the same fixed ChatGPT backend through the direct `curl` helper;
there is no controlled credential/backend seam. I did not run a live fetch against the user's
credentials or mutate the auth file. Valid, refresh-needed, missing, and rejected credential
outcomes were not all observed in the running app. No capture.

### F-CTRL-CLI-02

Drove a real Tiller-created shell pane:

```text
panel create --cmd 'printf P106_TILLERCTL_PATH=; command -v tillerctl || true; printf P106_TILLERCTL_SIBLING=; readlink -f /home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust/target/debug/tillerctl'
panel wait pane-491706-1 --timeout-ms 2000
panel read pane-491706-1
```

`panel create` returned `pane-491706-1`; `panel wait` returned `0`. Decoding the base64
`panel read` result produced exactly:

```text
P106_TILLERCTL_PATH=P106_TILLERCTL_SIBLING=/home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust/target/debug/tillerctl
```

The first field is empty: bare `command -v tillerctl` found nothing in the spawned shell's PATH.
The sibling executable resolved by the second probe exists. `panel list --json` showed the new
control pane alongside the restored Chat and Terminal panes. No agent-launch control method was
available to trigger `prepare`/hook execution; the generated-hook install path was not reached
by this socket-only exercise. No capture.

### F-USE-01

Drove:

```text
current-workspace --json
surface settings open --section appearance
surface settings read
```

`current-workspace --json` returned the selected `linux/gpui-waku` worktree at
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`. The Settings response carried
`refreshInterval:"5"`, `socketPath:"/tmp/p106-c11.sock"`, and the current theme/font values.
There is no socket method that reads the bottom usage bar or invokes its refresh control. The
worktree-selection state was reached; the visible gear/refresh/worktree bar and refresh gesture
were not observed. No capture.

### F-USE-02

Drove the AI Providers section and read the same live response. Its provider visibility fields
were `Claude=true`, `Codex=true`, `OpenCode=false`, and all five installed provider paths were
reported Available. This confirms the settings state carried into the app instance, but no
control call returns the rendered provider segments or their unavailable tooltips, and no pointer
input was sent. The segment/tooltip behavior was not observed visually. No capture.

### F-CORE-ACT-24

The built planner was exercised by:

```text
cargo test -p tiller_activity --test activity_domain_integration
```

`f_core_act_24_restore_plan_keys_refs_by_stable_content_not_live_pane` passed. Its observed plan
had one resumable reference for live stable content `terminal-stable` and one prunable reference
for absent content with session ref `session-b`. The missing runtime half was rechecked with:
`rg 'AgentSessionRestorePlan::plan' rust/crates`; the only call is the integration test, not
`crates/tiller/src`. No launch/relaunch restore observation or capture.

### F-CORE-ACT-25

The same activity integration run passed
`f_core_act_25_bootstrap_prioritizes_selected_open_worktree_and_defers_the_rest`: selected `w3`
was first, open `w1` second, and closed/deferred `w2` was deferred. Re-running
`rg 'BootstrapRestoreOrder::partition' rust/crates` found only the integration test, with no
production launch-restoration caller. No multi-worktree relaunch observation or capture.

### F-CORE-ACT-26

The same run passed
`f_core_act_26_mount_policy_evicts_only_safe_oldest_worktrees_until_cap`: with cap 2, selected
`w4`, running `w2`, unsaved error `w3`, and done `w1`, the returned eviction list was exactly
`["w1"]`. Re-running `rg 'WorktreeMountPolicy::ids_to_evict' rust/crates` found only the
integration test, with no production mount/eviction caller. No cap-driven live mount observation
or capture.

### Rows 11–17

### F-CORE-DOM-03

The project/worktree control route was live, with `current-workspace --json` selecting
`linux/gpui-waku`, but no project-creation/default-location control call was available in the
socket API. The built `default_project_base` function was re-read: it selects
`TILLER_PROJECTS_DIR`, then `$XDG_DATA_HOME/Tiller/projects`, then `$HOME/Tiller/projects`, with a
`Tiller/projects` fallback when HOME is absent. Re-running
`rg 'default_project_base' rust/crates` found only the export and its definition; the production
project-creation path does not call it. No proposed default location was observed in the running
app and no capture was available. The environment/default policy and the missing creation-path
consumer are recorded separately here.

### F-CORE-WSP-04

The pure layout package was exercised with:

```text
cargo test -p tiller_project
```

The run reported 46 library, 6 discovery-integration, and 3 naming-throttle tests passed. The
layout tests exercised structural/nonstructural classification, including Insert, divider-fraction,
and Rename examples. The live control capability list contains no workspace-layout command; the
`panel split` calls below exercise the terminal pane tree, not `tiller_project::LayoutCommand`.
Re-running `rg 'classify_layout_command' rust/crates/tiller/src rust/crates/tiller_ui/src` found no
production caller (only the project export, definition, and its own tests). No UI/control exercise
of insert, move, close, activate, view-state, divider, or rename commands was reached and no
capture was available.

### F-CORE-WSP-08

The `tiller_project` run exercised layout serialization and validation, but no live control method
can change editor caret/selection/scroll/folds, chat draft/attachments/transcript/follows-tail,
or terminal viewport state. `WorkspaceTabViewState` still declares all of those fields and is
embedded in `WorkspaceTab`; the rechecked production session path instead persists
`SessionTabState`/`TabStateRecord` and scrollback. `rg 'WorkspaceTabViewState'` found the model,
layout command, export, and test fixture, but no session-store consumer. No close/reopen restore
of these fields was observed and no capture was available.

### F-CORE-FILE-08

The production Files row calls `right_panel.rs::file_glyph`. Re-reading the live implementation
showed directory → `FolderFill`; shell files and `.bash`/`.zsh`/`.profile` → `SquareTerminal`;
`.git*`, `gitignore`, `gitattributes`, and `gitmodules` → `GitBranch`; `.env*`, `.editorconfig`,
`.gitconfig`, and `.npmrc` → `Settings`; all other files → `File`. The representative mapping
test `file_glyph_resolves_known_kinds_from_the_embedded_set` was present, but the narrow
`tiller_ui` test run could not compile because of the unrelated `chat.rs` `dismissed` field errors
recorded under F-USE-03.

No live Files-tree capture was obtained. The remaining clause elements were not reached: exact
filename/extension coverage beyond this small hardcoded set, named directory icons, selected
theme/resource mapping, and SF fallback resolution. `file_glyph` is production-called, but the
full theme/resource behavior was not observed.

### F-CORE-SET-01

The persisted settings half was exercised with:

```text
cargo test -p tiller_persistence --test persistence_integration settings -- --nocapture
```

All four filtered tests passed: defaults for unwritten settings, Linux settings surviving a DB
relaunch with clamps, the project/worktree/settings round trip, and out-of-range/unparseable
fallbacks. The `tiller_project` suite also exercised `SettingsPolicy` clamping and its
`TILLER_SOCKET_ENABLE` override. The private app Settings response returned live values including
`resumeAgentSessions:"true"`, `autoNaming:"false"`, `limitMountedWorktrees:"false"`,
`mountedWorktrees:"6"`, `refreshInterval:"5"`, and `controlSocketEnabled:"true"`.

The missing part was rechecked at the conversion seam: `SettingsSnapshot`/`AppSettings` carry the
persisted contract fields, but the UI's `translucency` state and sidebar/right-panel width policy
fields do not appear in the snapshot or persistence model. The control socket has no settings
mutation command, so no change/relaunch readback for those fields was possible. No capture.

### F-AGENT-API-01

The adapter package was exercised with:

```text
cargo test -p tiller_agents
```

The run reported 11 library, 22 adapter, 1 home-isolation, 5 session-row, and 8 session-source
tests passed. `catalog_has_the_five_adapters_in_order` observed IDs `claude`, `codex`, `opencode`,
`pi`, `omp`, display names `Claude Code`, `Codex`, `OpenCode`, `Pi`, `Oh-My-Pi`, and hook flags
`[true, true, false, false, true]`; the adapter tests also exercised prepare, launch, resume,
availability, and worktree-local hook behavior.

The missing half was rechecked in `AgentAdapter`: it exposes identity, hook flag, prepare,
command, resume, and ACP-program methods, but no optional noninteractive summarizer method or
command. `rg 'summar|summarize' rust/crates/tiller_agents` found no adapter summarizer API. No
agent picker/launch gesture was sent and no capture was available.

### F-TERM-SPLIT-01

Drove the live pane split route:

```text
panel split right --from pane-491706-1 --cmd 'printf P106_SPLIT_RIGHT'
panel split down --from pane-491706-1 --cmd 'printf P106_SPLIT_DOWN'
panel list --json
```

The calls returned `pane-491706-2` and `pane-491706-3`. The final list contained the original
`pane-491706-1`, a `printf (right)` pane, and a `printf (down)` pane, with the down pane active;
the restored Chat and Terminal panes remained present. This exercised real PTY creation and
right/down pane-tree mutation through the control socket.

The current split geometry needles were also re-read: `SEAM_WIDTH` is `6.0` and
`MIN_SPLIT_PANE_SIZE` is `160.0`, with corresponding app tests. `tiller_terminal`'s lifecycle
tests passed all 3 cache-preservation tests. The missing integration half remains visible in
`rg 'TerminalPaneCache|move_within_worktree' rust/crates/tiller/src rust/crates/tiller_ui/src`:
the cache types have no production consumer in the app, only lifecycle tests. No resize-to-limit,
close/prune/focus-restore visual capture was obtained.

---

# P106 report — fable slice

Brief: `tasks/P106-exercise-the-miscalled.md`. Observations only — **no verdicts**; a critic
sets those. Captures live in `reference/linux-progress/p106-fable-*.png` (fable's) and are cited
by filename. Lane for the fable slice: Wayland (`Scripts/wayland-drive.sh`, label `fable`,
display `wayland-3`, socket `/tmp/fable.sock`) — no drive lock, **no synthetic input**, so every
click/drag/chord below is recorded as owed, with the exact gesture.

One lane fact that shaped the evidence: `TILLER_DB=/tmp/fable.sqlite` persists across
`wayland-drive.sh` runs, so this instance **booted with tabs restored from a previous
fable-labelled session** (Chat + Terminal for `linux/gpui-waku`, with an empty project catalog).
Frames 01–03 show that inherited state, not a fresh app.

## fable — window / sidebar / tabs / changes (17 rows, censused by codex12)

### F-WIN-01 — workspace ⇄ Settings routes (census: BUILT)

Drove the state half over the socket:

- `ctl surface.settings.open` → full state reply (five sections: ai-providers, agents, general,
  permissions, appearance, plus values). **Settings replaced the whole workspace** — no sidebar,
  no tab strip; header reads `‹ Back  Settings` — `p106-fable-15-win01-settings-replaces-workspace.png`.
- `ctl tab.select index=2` from inside Settings → reply `ok {}` but the **Settings route stayed
  on top** — `p106-fable-16-win01-back-via-tab-select.png` (frame is fresh: forced repaint at a
  different resolution). No socket method exits Settings; once opened, every later capture is
  occluded until relaunch.
- After `system.quit` + relaunch, the app launched into the **workspace** route (Settings not
  persisted) — `p106-fable-17-win07-relaunch-autorestored.png`.

Chord needle, because the ledger's absence claim aged: `"ctrl-,"` **is bound** at
`tiller/src/main.rs:167` (plus a test that simulates it at :9254). The ledger's pass-12 evidence
"no ctrl-, chord in any crate" no longer describes today's tree.

Owed: gesture — press `ctrl-,` in a live window and confirm Settings opens; click `‹ Back` and
confirm the workspace returns (Escape is a second exit per the F-SET-02 comments).

### F-WIN-07 — restore previous launch (census: PARTIAL — restore built, History menu absent)

Built half, driven twice:

1. Same-launch cycle: `ctl workspace.close workspace=p-c1fd7a5bbfd541af-wt-0` →
   `{"closed":"true","path":".../tiller"}`; `workspace.list` then showed wt-0 `mounted:"false"`.
   `ctl session.restore` → `{"path":".../tiller-linux","restoredCount":"0"}` — and wt-0 **stayed
   unmounted** (`workspace.list` unchanged).
2. Quit/relaunch cycle: `system.quit`, relaunch on the same DB. The launch itself auto-restored
   **everything** — both worktrees `mounted:"true"` (including wt-0, which was unmounted at
   quit), all four tabs, and the Terminal tab's 4-pane split layout with fresh live shells
   (`panel.list` returned 8 panes) — `p106-fable-17-win07-relaunch-autorestored.png`. A second
   explicit `session.restore` again returned `restoredCount:"0"` —
   `p106-fable-18-win07-explicit-restore.png`.

For the critic: `session.restore` answered `ok` with `restoredCount:"0"` in both invocations,
including immediately after an unmount in the same launch, while the *launch path* demonstrably
restores. Whether the snapshot is consumed at boot or the explicit restore is a no-op needs a
reading of `restore_launch_snapshot` (main.rs:3921) against these replies. Not judged here.

Missing half, needle re-run: `grep -rniE "previous launch|history"` over `tiller`/`tiller_ui` →
no History menu, no "Restore Previous Launch" entry, no ⇧⌘O; the only hits are unrelated
(scrollback/chat-retention/browser history vec). Confirmed absent.

### F-WIN-10 — transient toasts (census: PARTIAL — sidebar notice built, toast absent)

Built half — **not reachable over the socket**, and the attempt is itself the observation:

- `ctl project.add path=/tmp/fable-nope` → error reply `cannot add /tmp/fable-nope: No such file
  or directory (os error 2)`; **no sidebar notice rendered** —
  `p106-fable-02-win10-invalid-path.png`.
- `ctl project.add <already-tracked path>` → `ok` with `added:"false"`, silently; no notice —
  `p106-fable-03-win10-duplicate-notice.png`.
- Cause, read not driven: the socket route is `control_add_project` (dispatched at
  main.rs:2653), a separate path from the UI `add_project` whose `Ok(false)`/`Err` arms call
  `sidebar.set_notice` ("already tracked or nested: …", main.rs:3226-3235). Every `set_notice`
  caller in the tree is behind a gesture: Add-Project picker events, sidebar context-menu
  actions (init-git, reveal), agent-launch failure, file-picker/save failure.

Owed: gesture — click the sidebar `+` → Add Project and pick an already-tracked folder (routes
to the duplicate notice), then observe the notice render and whether it ever dismisses.

Missing half, needle re-run: the only "toast" in the tree is the theme radius token asserted at
`tiller_ui/src/conformance.rs:156` — no toast surface, no auto-dismiss. Live corroboration: no
notice/toast element appeared in any of the 18 frames of this pass.

### F-SID-06 — project status badge (census: PARTIAL — worktree dot built, project badge absent)

Built half (worktree dot), driven through the full status cycle on the restored terminal pane:
`ctl notify session=pane-1 status=<s>` for running / needs-input / error / done (each replied
`queued:"true"`):

- `running` → **no dot** — `p106-fable-04-status-running.png`. Deliberate, not a gap:
  `sidebar.rs:2055-2065` maps `ActivityStatus::Running => None` under the comment "A dot only
  appears for a notable status — matching the reference".
- `needs-input` → amber dot — `p106-fable-05-status-needs-input.png`; `error` → red dot —
  `p106-fable-06-status-error.png`; `done` → dot in the `theme.tab_done` colour —
  `p106-fable-07-status-done.png`. (`set_worktree_status` → `status_dot_color` live.)

Missing half: the dot is computed under a `(kind == RowKind::Worktree)` gate — project rows
never get one. Live corroboration: the `tiller` project row shows no badge in any frame while
its child worktree carried every status (04–07). The clause's collapsed-project variant needs a
disclosure click this lane cannot deliver.

Owed: gesture — start activity, click the project row's chevron to collapse it, and record
whether any badge appears on the collapsed project row (per the code gate, none should).

### F-SID-11 — worktree row identity (census: PARTIAL — identity built, comment absent)

Driven: `ctl project.add path=/home/enzopalmisano/Scrivania/Progetti/tiller-linux` →
`{"added":"true","projectId":"p-c1fd7a5bbfd541af","worktreeCount":"2"}`. Rows render:
project `tiller` + path; worktree `rust/gpui-rewrite` + path + **Primary** badge; worktree
`linux/gpui-waku` + path + status dot + nested tab rows (Chat/Terminal/Changes/Browser as they
opened) — `p106-fable-03-…png`, `p106-fable-08-…png`, `p106-fable-17-…png`. Branch names are the
row titles. Agent-status display: the dot cycle under F-SID-06.

The clause's *folder worktree*: `ctl project.add path=/tmp/fable-folder` →
`{"added":"true","worktreeCount":"0"}` — the folder project renders as a **bare project row
with no worktree child at all** (`fable-folder`, `/tmp/fable-folder`,
`p106-fable-17-…png`), so there is no folder-worktree row on which to inspect identity. On this
build the clause's folder-worktree half has no surface to exercise.

Missing half, needle re-run: no comment render anywhere in `sidebar.rs` (sole grep hit is a
doc-comment). Sharpened by the socket: `workspace.list` **does** expose a `comment` field
(`""` for both worktrees) — the model carries a comment; the row render never draws one.

### F-SID-15 — remove worktree from context menu (census: PARTIAL — hover × built, menu item absent)

Built half (the hover × route): not drivable on this lane — it is a hover + click. The control
exists in code: `remove-worktree-{row_id}` at `sidebar.rs:2272-2285` → `remove_worktree_row`
:1438 → `tiller_git::remove_worktree`. Owed: gesture — hover a worktree row, click the ×,
confirm the row disappears, and record whether **any** confirmation prompt appears (the pass-12
ledger note says this route confirms nothing).

Missing half, re-read today: the worktree arm of `context_menu_items` (sidebar.rs ~700-760)
carries Set Primary / Unset Primary and seven New-Tab actions (New Terminal, Claude Code, Codex,
OpenCode, Pi, Oh-My-Pi, New Chat) and ends there — **no Remove Worktree item**. Census needle
re-run agrees (no `remove.?worktree` label among menu items).

### F-SID-16 — reorder projects by dragging (census: BUILT)

The whole clause is a drag; this lane has no synthetic input (WAYLAND-LANE trap 3), so none of
it can be closed here. Driven precondition: **two project rows** exist and render (`tiller`,
`fable-folder`) — `p106-fable-14-sid16-two-projects.png`, `p106-fable-17-…png`.

Owed: gesture — press-drag one project row above/below the other in the projects list and
release; confirm the order changes and survives (census wiring: `Sidebar::row_drag`
sidebar.rs:527, `ReorderScope::Projects`, committed through main.rs:3269).

### F-SID-17 — reorder worktrees by dragging (census: BUILT)

Same lane limit. Driven precondition: project `tiller` lists two worktree rows
(`rust/gpui-rewrite`, `linux/gpui-waku`) — every sidebar frame from
`p106-fable-03-…png` onward.

Owed: gesture — drag `linux/gpui-waku` to `rust/gpui-rewrite`'s position within the same
project; confirm the order changes (census wiring: worktree drag scopes → `reorder_sidebar`
main.rs:3281).

### F-TAB-01 — tab strip decorations (census: BUILT)

Driven four tab kinds and the full status vocabulary:

- Kinds: restored Chat + Terminal; `ctl surface.changes.open` + `tab.select index=3` (Changes);
  `ctl browser.open url=https://example.com` + `tab.select index=4` (Browser). Four distinct
  per-kind icons render (speech bubble / terminal / diff / globe) —
  `p106-fable-08-chg01-changes-tab.png`, `p106-fable-09-tab01-browser-tab.png`.
- Status indicator (same notify cycle as F-SID-06): ○ idle (baseline), **●** amber running
  (`…04…png`), **?** needs-input (`…05…png`, strip), **!** error (`…06…png`, strip), **✓** done
  (`…08/09…png`). Vocabulary matches `tab_status_glyph` (main.rs:2285).
- Close control: renders **on the active tab only** (`.when(active, …)` main.rs:5716) — the ×
  follows the active tab across frames 03/08/09.
- Dirty indicator: red-orange ● rendered beside the Terminal tab — fed not by an edited
  document but by `tab_is_dirty` (main.rs:5766): "live terminals, streaming chats, and unsaved
  editors are dirty"; the restored terminal owns a live PTY. Photographed on the **inactive**
  Terminal tab (`…09…png`). Observation: in frames where Terminal is *active* (05/06) the
  status glyph + × render and **no dirty dot is visible**; the dot appeared only on inactive
  tabs in this pass. Whether active-tab dirty is swapped for the close control by design needs
  the reference; recorded as seen.

Owed: gesture — the clause's document tab (file open is a picker/Files-panel click) and the
modify-a-document dirty route (typing); neither document-kind icon nor unsaved-editor dirty was
exercisable here.

### F-TAB-11 — split disabled reasons (census: PARTIAL — model built test-only, UI absent)

Built conjunct: `split_disabled_reason` (panes.rs:218) **cannot be driven from the running
app** — re-verified today: its only references outside the definition are the `#[cfg(test)]`
module import (panes.rs:610) and tests. No production caller exists, so no gesture or socket
call can make it fire.

Missing conjunct, re-verified at the struct level: `TerminalContextItem` is
`{label, action, route}` (context_menu.rs:29-33) — **no disabled state, no reason field** — and
the menu items are static consts. Nothing renders a disabled split entry or its reason.

Owed: nothing exercisable — both the clause's "resize until ineligible" and "sole tab" trials
dead-end at a menu that has no disabled-reason surface. (The right-click to open the terminal
context menu itself is also input-gated on this lane.)

---

## Batch 2 — F-TAB-18/23/24/28, F-CHG-01/20, F-PER-07

### F-TAB-18 — drag a tab to reorder the strip (census: BUILT)

The clause is a drag end-to-end, and no socket method moves a tab (none of the 53
`system.capabilities` methods is a reorder). What was drivable:

- Precondition staged and captured: a four-tab strip — Chat, Terminal, Changes, Browser —
  visible in `p106-fable-08-chg01-changes-tab.png`, `-09-tab01-browser-tab.png`,
  `-17-win07-relaunch-autorestored.png`.
- Wiring re-read at the render site (not test code): the tab element carries
  `.on_drag(tab_drag, …)` (main.rs:5630) and `.on_drag_move::<RowDrag>` (:5631) which calls
  `preview_tab_reorder` (:5635, defined :3360). Production-wired, matching the census.

Owed: gesture — press-drag a tab to a different strip position and drop; observe the new
order render.

### F-TAB-23 — terminal splits in all four directions (census: BUILT)

Driven — the split machinery itself, all four directions, via `pane.split`:

- `pane.split direction=left` on the Browser tab's pane →
  `p106-fable-10-tab23-split-left-on-browser.png`. **Placement anomaly for the critic:** the
  new terminal pane appeared on the RIGHT half, browser on the left — `direction=left` and
  `direction=right` produced visually identical placement in my frames.
- `pane.split direction=right`, `=up`, `=down` on the Terminal tab →
  `p106-fable-11-tab23-split-right.png`, `-12-tab23-split-up.png`,
  `-13-tab23-split-down.png` — ending in the expected layout of one left pane plus a right
  column of three.

All splits created live panes; verbatim `panel.list` after the four splits (still true at
write time):

```
{"id":"fable","ok":true,"result":{"panels":"[{…\"id\":\"pane-0\",\"tab\":\"Chat\"…},{…\"id\":\"pane-1\",\"tab\":\"Terminal\"…},{…\"id\":\"pane-2\",\"tab\":\"Changes\"…},{…\"id\":\"pane-3\",\"tab\":\"Browser\"…},{…\"id\":\"pane-4\",\"tab\":\"Browser\"…},{…\"id\":\"pane-5\",\"tab\":\"Terminal\"…},{…\"id\":\"pane-6\",\"tab\":\"Terminal\"…},{…\"id\":\"pane-7\",\"tab\":\"Terminal\"…}]"}}
```

The split layout also survived quit + relaunch (`-17-win07-relaunch-autorestored.png`,
reported under F-WIN-07).

The clause's *menu* route: all four items exist with the labels the clause names — "Split
Left" / "Split Right" / "Split Above" / "Split Down" (context_menu.rs:68-84), dispatching
typed `SplitLeft`/`SplitRight`/`SplitAbove`/`SplitDown` actions (:15-18). The ledger's
"only right/down" is stale against this file. Whether the menu route and `pane.split`
converge on identical placement I cannot say from this lane.

Owed: gesture — right-click a terminal pane, click each of the four Split items.

### F-TAB-24 — Escape cancels an in-progress tab drag (census: PARTIAL — drag built, cancel absent)

Built conjunct (the drag): same input bar as F-TAB-18 — nothing socket-reachable; wiring
confirmed at main.rs:5630-5635.

Missing conjunct re-verified — needle `grep -in escape rust/crates/tiller/src/main.rs`:
every production hit is either the F-SET-02 settings-escape (global binding :162, handler
:6866) or the tab-**rename** cancel (:6091). No handler cancels a drag; `grep -n
"drag_cancel\|cancel_drag"` over `rust/crates/tiller/src/` returns nothing. The census's
missing half stands.

Owed: gesture — begin dragging a tab, press Escape before dropping, observe the strip
return to its original order (expected to fail per the re-verified absence — but the trial
is what converts it).

### F-TAB-28 — ⌘W closes the active tab, confirming when dirty (census: BUILT)

State half driven as far as the lane reaches:

- The chord is bound, twice: `KeyBinding::new("cmd-w", CloseTab)` main.rs:2582 and
  `KeyBinding::new("ctrl-w", CloseTab)` :2583, dispatching `handle_close_tab` (:6642). The
  ledger's pass-12 note ("no ⌘W; ctrl-alt-w closes a pane") is stale against these lines.
- No socket method closes a *tab* — `pane.close` exists but closes a pane, a different
  clause — so the close path itself could not be exercised without input.
- The dirty predicate that gates the confirm is live and was driven indirectly under
  F-TAB-01: `tab_is_dirty` (:5766) fed the red-orange dirty dot visible in the strip frames.

Owed: gesture — press ctrl-w on a clean active tab (expect close), then on a dirty one
(expect the confirm; Cancel keeps the tab).

### F-CHG-01 — Changes as a surface, not a right-panel mode (census: PARTIAL)

Driven conjunct — the Changes surface:

- `ctl surface.changes.open` → tab created/selected via `tab.select index=3` →
  `p106-fable-08-chg01-changes-tab.png`: header "Local changes (49)", sections Changed (2)
  and Untracked (47), buttons Stage all / Expand All / Collapse All / Discard all.
- The open reply is the *loading-state* snapshot — verbatim (re-issued at write time):

```
{"id":"fable","ok":true,"result":{"changed":"[]","changedCount":"0","error":"","loading":"true","ready":"false","sections":"[{\"count\":\"0\",\"files\":\"[]\",\"section\":\"Staged\"},{\"count\":\"0\",\"files\":\"[]\",\"section\":\"Changed\"},{\"count\":\"0\",\"files\":\"[]\",\"section\":\"Untracked\"}]","staged":"[]","stagedCount":"0","surfaceId":"changes","tabId":"4","untracked":"[]","untrackedCount":"0","worktree":"/home/enzopalmisano/Scrivania/Progetti/tiller-linux"}}
```

  A `surface.changes.read` two seconds later returned populated sections (Untracked count
  49, per-file additions/deletions) — including the two evidence PNGs this very report
  copied into `reference/linux-progress/` a minute earlier, so the surface reads the
  worktree live. **Note for the critic:** the top-level flags still said
  `"loading":"true","ready":"false"` in that populated read — the flags did not settle even
  though the data had.
- The surface is a tab, corroborating the census's "Changes moved to a Diff tab":
  `surface.changes.open` builds `TabContent::Changes` (main.rs:4205), and the reply above
  carries `"tabId":"4"`.

Missing conjunct re-verified — the right panel has no Files/Changes switch:
`render_header` (right_panel.rs:603) renders a fixed "Files" title plus a close-panel ×
and nothing else; every capture shows the same. The panel-open/close toggle in the
titlebar is click-gated.

Owed: gesture — click the titlebar right-panel toggle (close, reopen), confirming the
panel is Files-only either way.

### F-CHG-20 — Activity running count and empty state (census: BUILT)

Count driven across 0 → 1 → 2 running:

- Nothing running: the Activity header renders with no count —
  `p106-fable-03-win10-duplicate-notice.png`, `-17-win07-relaunch-autorestored.png`.
- `ctl notify session=pane-1 status=running` then `session=pane-5 status=running` — **two
  running panes, both in the same Terminal tab** — header shows "**1 running**"
  (`p106-fable-19-chg20-two-running.png`, crop `crop-19-activity.png` in /tmp/fable-shots).
  The count is per tab surface, not per pane.
- `ctl notify session=pane-4 status=running` (the Browser tab's split terminal pane) →
  "**2 running**" (`p106-fable-20-chg20-second-tab-running.png`).

Empty state: "No activity" (right_panel.rs:837) renders only inside the
`if self.activity_expanded` branch (:821); `activity_expanded` defaults to `false` (:153)
and toggles only on a header click (:754). A unit test at right_panel.rs:1949-1967 asserts
exactly this and names F-CHG-20 in its message.

Owed: gesture — click the Activity header with nothing running; observe the expanded
"No activity" row.

### F-PER-07 — project settings persist across relaunch (census: PARTIAL)

Persistence layer driven (read-only over `/tmp/fable.sqlite`, `mode=ro` URI):

- Schema `user_version` 12; `project` table columns: `id, name, root_path, color_hex,
  display_name, icon_kind, icon_value, avatar_image, default_worktree_base,
  worktree_location_override, order_idx` — every field the clause names has a column.
- Both socket-added projects persisted rows; `display_name`, `default_worktree_base` and
  `worktree_location_override` are NULL in both — nothing populates them without the UI
  door.
- Catalog persistence itself was observed live: the `fable-folder` project survived
  `system.quit` + relaunch (`p106-fable-17-win07-relaunch-autorestored.png`).
- Incidental cross-reference for F-SID-16/17: `order_idx` holds 0 and 1 for my two
  projects — the reorder rows' persistence target exists.

Built conjunct (display name / icon door): `update_project_settings` (main.rs:3240)
persists through the catalog — but its only door is the Project Settings sheet, which is
right-click-gated, and **no socket method updates project settings** (none of the 53).
Could not be driven on this lane.

Missing conjunct re-verified: `default_worktree_base` / `worktree_location_override` have
no UI door at all — needle re-run finds only the persistence layer and the
`tiller_project` model defaults as consumers; zero UI writers. The census's missing half
stands.

Owed: gesture — right-click a project row → Project Settings; change display name and
icon; quit + relaunch; confirm both persisted. While there, confirm the sheet exposes no
field for the two worktree-location columns.

---

## Slice close-out — 17/17 rows exercised

Captures: `reference/linux-progress/p106-fable-01…20-*.png` (crops of status glyphs and
the Activity header remain in `/tmp/fable-shots/crop-*.png`, not committed). The fable
Wayland instance (app + nested sway) was shut down after the last read, env-matched per
WAYLAND-LANE trap 4; `/tmp/fable.sqlite` and `/tmp/fable.log` left in place for the
critic.

**Owed-gesture batch** (everything on this slice that needs the X lock, exact gestures):

1. F-WIN-01 — click the titlebar gear; then exit Settings by a click (no socket door out).
2. F-WIN-10 — add a project via the **sidebar** "+" (invalid path, then duplicate) to see
   the `set_notice` copy; socket `project.add` bypasses every notice.
3. F-SID-06 — hover a worktree row; click the × remove control; observe confirm.
4. F-SID-11 — right-click a worktree row: confirm menu shows Set/Unset Primary + 7
   New-Tab items and **no** Remove Worktree.
5. F-SID-15 — click a folder-project row; observe what a worktree-less project opens.
6. F-SID-16/17 — drag a project row above/below the other; relaunch; confirm order
   (persistence target `order_idx` confirmed present).
7. F-TAB-11 — nothing exercisable (no disabled-reason surface exists — see row).
8. F-TAB-18 — drag a tab to a new strip position and drop.
9. F-TAB-24 — begin a tab drag, press Escape before dropping.
10. F-TAB-28 — ctrl-w on a clean tab, then on a dirty tab (confirm dialog, Cancel).
11. F-CHG-01 — click the titlebar right-panel toggle both ways.
12. F-CHG-20 — click the Activity header with nothing running ("No activity" row).
13. F-PER-07 — Project Settings sheet: change name/icon, relaunch, verify; note absence
    of worktree-location fields.

---

## Orchestrator correction — the chat rows driven over the socket do not count

Added 2026-08-14 after the slices closed. **This corrects my own instruction, not anyone's work.**

`P106` told all three of you to reach chat over the control socket. That route does not drive the
chat you can see. `surface.chat.*` operates on a `chat_sessions` map owned by the control handler
(`tiller/src/main.rs:650`), which is disjoint from the rendered chat view — it even spawns its own
ACP agent against its own database. Verified four ways on the Wayland lane: a completed `pwd` turn
with a real assistant answer left the rendered transcript empty; `compose text=MARKER_ZZ9` left the
visible composer showing its placeholder; `compose surfaceId=pane-0` was refused with `chat surface
is not open: pane-0`; and `panel.list` reports the visible tabs as `pane-0`/`pane-1`, an id space
the chat API does not accept.

`surface.chat.open` returns `surfaceId: "default-chat"`, which *is* the persisted id of the visible
Chat tab (`session.rs:322`) — so the socket answers with the right name for the wrong object. That
is why this looked like it was working.

**Consequence for this report:** every observation here whose only route was `surface.chat.*`
evidences the control API, not the UI, and cannot settle its row. `codex12`'s `F-CHAT-21` and
`F-CHAT-22` entries already say the rendered transcript stayed empty — that reading was correct and
the cause is now known. Treat the whole `F-CHAT` group in this report as **owed: gesture on
`DISPLAY=:1`** until `P107` lands.

Nothing else in the report is affected: the settings, project, sidebar, tab, usage, core and
control rows were driven through routes that do render.
