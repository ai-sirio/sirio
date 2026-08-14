# P116 report

## Slice C — headless rows

Headless instance: `env -u DISPLAY -u WAYLAND_DISPLAY TILLER_SOCKET=/tmp/pi.sock TILLER_DB=/tmp/pi.sqlite rust/target/debug/tiller`; it listened on `/tmp/pi.sock`. All captures below are socket/CLI transcripts.

### Batch 1 (rows 1–10)

- `F-CORE-ACT-19` — Drove `session-ref` and attempted an activity `notify` against a live socket-created terminal panel. The CLI rejected the combined agent-status and user-notification arguments as “ambiguous notify modes”; no notification was delivered. The control capability list has `notify` and `notification.*`, but no activity-policy/visibility control. Capture: `/tmp/p116-c2-control.txt`.
- `F-CORE-ACT-20` — The same live panel was used to probe status notification delivery. The attempted working/idle calls were rejected before dispatch because the title/body notification arguments were mixed with `--session/--status`; `list-notifications --json` remained `[]`. Capture: `/tmp/p116-c2-control.txt`.
- `F-CORE-ACT-24` — Queried the live socket’s complete capability list and exercised `session-ref` on `pane-2157148-2`; it returned success. No control method exposes `AgentSessionRestorePlan` inputs or a stable-content-ID restore result, so that plan could not be driven through this socket. Capture: `/tmp/p116-c1-control.txt`, `/tmp/p116-c2-control.txt`.
- `F-CORE-ACT-25` — Added the project and observed both discovered worktrees live; the selected worktree was `p-c1fd7a5bbfd541af-wt-1`. No socket command accepts bootstrap priority/deferred inputs or returns the `BootstrapRestoreOrder` partition. Capture: `/tmp/p116-c1-control.txt`.
- `F-CORE-ACT-26` — Mounted/listed the project worktrees and created/split live panels in the selected worktree. The control contract offers no mount-eviction command or reported eviction decision, so selected/active/unsaved protection was not observable. Capture: `/tmp/p116-c1-control.txt`, `/tmp/p116-c2-control.txt`.
- `F-CORE-FILE-03` — Drove a real PTY panel through `panel write`; the panel scrollback reported the base64 payload for `printf socket-write\r\npanel-ready`. The socket has no file-drop/external-path method, so terminal file-drop encoding could not be sent. Capture: `/tmp/p116-c2-control.txt`.
- `F-CORE-FILE-06` — Opened and read the Changes surface for the selected worktree; it reported zero staged, unstaged, and untracked files. No control method performs a file save or an external modify/delete/rename event, so the clean/dirty external-change cases were not driven. Capture: `/tmp/p116-c2-control.txt`.
- `F-CORE-FILE-08` — The headless socket exposed no file-icon or rendered-tree payload. `surface changes read` returned only counts; no named directory/file glyph mapping was observable without a rendered display. Capture: `/tmp/p116-c2-control.txt`.
- `F-CORE-USG-05` — Queried all live capabilities, opened General then Appearance settings, and read both surfaces. The protocol returns only `settings\t<section>` and has no usage-refresh/status-bar method; no usage refresh request could be issued. Capture: `/tmp/p116-c2-control.txt`.
- `F-CORE-USG-06` — With the same isolated instance, inspected settings and the live capabilities. There is no control request for Codex credential refresh or token-error classification, so the refresh failure branches were not observable through the specified headless interface. Capture: `/tmp/p116-c2-control.txt`.
