# P96 — headless-lane report

Date: 2026-08-14
Lane: Xvfb `:2`, isolated `TILLER_DB=/tmp/p96-codex12.sqlite`, isolated `TILLER_SOCKET=/tmp/p96-codex12.sock`
Binary: `rust/target/debug/tiller` / `tillerctl`
P90 handoff: commit `988d9e9` (`fix: make browser control responses honest`)

## Build and lane

`cargo check -p tiller` and `cargo build -p tiller_control --bins` passed before the P90 handoff. The isolated app answered `tillerctl ping` with `pong`; capabilities advertised notification, tab, settings, and browser control methods. The app was quit and relaunched against the same DB; the socket came back and `ping` again returned `pong`.

## Seven judgeable rows

| row | verdict | headless evidence |
|---|---|---|
| `F-CORE-ACT-19` | NOT EXERCISED | A control-owned pane accepted `notify --session <pane> --status running` and `needs-input`, but `list-notifications --json` did not gain a desktop notification. The pane has no agent identity, so this did not create the required agent transition. The direct user-notification path did round-trip the exact title/subtitle/body payload through `list-notifications`, but that is a different in-memory list and is not proof that the activity payload was built and delivered. The Linux notifier path uses `notify-send`, which is not installed in this lane. |
| `F-CORE-ACT-20` | NOT EXERCISED | The same control-owned-pane probe produced no activity notification. No agent-owned pane with a real transition was available, and there was no desktop notifier with which to observe the suppression/allow result. The visible-window and unchanged-status gates therefore were not tested as the row requires. |
| `F-AGENT-OMP-01` | NOT EXERCISED | Tiller launched the corrected distribution name with `panel create --cmd "oh-my-pi --hook '/home/enzopalmisano/Scrivania/Progetti/tiller-linux/.tiller/omp-hook.ts'"`. The pane exited `code:1` before a session, with `SyntaxError: Unexpected token ':'` in the installed `oh-my-pi` JavaScript entrypoint (`function checkFile(path: string, label: string)`). Source evidence confirms `id() == "omp"`, `executable_name() == "oh-my-pi"`, and both command builders use `oh-my-pi`; a live OMP session could not be reached. |
| `F-AGENT-OMP-02` | NOT EXERCISED | Because the OMP process failed before session startup, no `session_start`, `turn_start`, `turn_end`, or `session_shutdown` hook events were observed. A follow-up `notify --session <pane> --status needs-input --agent-session p96-omp-ref` did not add a notification. The stale pre-existing `.tiller/omp-hook.ts` was not modified or treated as freshly generated evidence. |
| `F-BRW-04` | FAILED — defective | Raw socket `browser.open` with `{"url":"https://"}` returned `ok:true` and silently substituted `https://example.com`. Raw `browser.navigate` to `http://127.0.0.1:9/p96-unreachable` and `http://does-not-exist.invalid/p96-unreachable` both returned `ok:true` with the requested URL and blank title, with no error field. Invalid-address and unreachable-host behavior therefore does not expose the required visible error through the available browser control path. |
| `F-PER-08` | NOT EXERCISED | Read-only SQLite inspection before relaunch showed schema version `11`, a `browser_origin_grant` table, and zero rows in both `browser_origin_grant` and `setting`. The settings socket could open/read General and Permissions but exposed no grant and has no mutation path for the required setting/grant change. After quit/relaunch against the same DB, both tables remained empty. This verifies the restart probe, not a grant/settings persistence roundtrip. |
| `F-BRW-07` | NOT EXERCISED | `browser_origin_grant` was empty before and after relaunch. The raw `browser.open`/`browser.navigate` methods do not trigger an Allow-origin prompt, and the control capabilities expose no permission-grant action, so no origin could be allowed and retriggered. |

The only verdict delta from the pre-P96 ledger is `F-BRW-04`: `NOT EXERCISED` → `FAILED — defective`. The other six rows remain `NOT EXERCISED`, with the instrument limitations and live outputs above.

## Tab rows: reachability only

P96 does not judge `F-TAB-12`, `F-TAB-15`, or `F-TAB-17` because the control socket cannot right-click a tab or invoke its context menu.

- `tab select` and `tab cycle forward/backward` were reachable and returned exit 0 (four successful navigation calls; an out-of-range `tab select 99` also returned exit 0, with no readback).
- `F-TAB-12`: Move Existing Tab via context menu — 0/1 actions reachable; no verdict change.
- `F-TAB-15`: clean close control or context-menu Close — 0/1 actions reachable; no verdict change.
- `F-TAB-17`: Close Others / Close Tabs Right — 0/2 actions reachable; no verdict change.

## Scope and cleanup

No user-global configuration was modified. Only the P90 `main.rs` handoff was committed separately, as required by the task; the report and ledger update are scoped to P96. Unrelated concurrent worktree changes and pre-existing untracked artifacts were left untouched.
