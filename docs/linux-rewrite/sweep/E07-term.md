# Drive slice E07-term — 4 rows

Families: F-TERM.

Exercise each row live and record what you saw. **Do not edit the ledger**;
a different agent adjudicates. For a `half-proven` row the evidence column
names the half already proven — drive only the missing half and say which.

| row | ledger line | current verdict | evidence recorded so far |
|---|---|---|---|
| `F-TERM-04` | 322 | half-proven | upgraded from NOT EXERCISED without a new drive: `reference/linux-progress/p17-rclick-term.png` (pass 17, filed to settle a *platform* question) shows the menu open **on a live terminal pane** with `Copy` and `Paste` as its first two items. So the surface is live-observed, not merely drawn-tested. Handlers are real (`lib.rs:718-750`: selection/scrollback → clipboard; clipboa… |
| `F-TERM-06` | 324 | half-proven | upgraded from NOT EXERCISED without a new drive: in `reference/linux-progress/p17-rclick-term.png` the live menu carries both identity items (visible as `Copy P…`/`Copy T…`, truncated by the Files-panel overpaint, not missing). `CopyPaneId`/`CopyTerminalId` write the real strings at `lib.rs:737-742`. **What remains**: click each and read the clipboard back — the string conte… |
| `F-TERM-08` | 326 | half-proven | P82 (codex11) claimed SIGTERM+SIGKILL-fallback shutdown covering all exit routes. Checked live, all three, no display needed: **Quit** (Route A) and **external `kill -TERM $APP_PID`** (Route C, via PTY-hangup SIGHUP to the child's foreground process group) both leave zero survivors in the PTY's process group (`ps -eo pid,pgid,ppid,stat,comm`, CHILD_PID/APP_PID tracked). But … |
| `F-TERM-UI-01` | 535 | half-proven | upgraded from NOT EXERCISED without a new drive: the menu in `reference/linux-progress/p17-rclick-term.png` is open **because a real button-3 event was routed to the terminal pane** — right-click hit testing is live-proven, which is the half a drawn test proves least well. **What remains** is delegation: no item was invoked, so nothing shows the menu forwarding to its handle… |
