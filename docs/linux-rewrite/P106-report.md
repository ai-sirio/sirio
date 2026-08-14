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
