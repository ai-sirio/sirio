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
