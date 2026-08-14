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
