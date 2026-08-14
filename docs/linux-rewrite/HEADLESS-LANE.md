# The headless lane — verify without the display lock

**Found 2026-08-14 04:15 by the orchestrator, while failing at something else.**

Four agents share one X pointer, so every drive serializes behind
`/tmp/tiller-drive-1.lockd`. This lane removes that constraint for a large class of rows.
**You can run your own app instance, concurrently with everyone else's, and exercise it through
the control socket.**

## What it is

An app instance on the virtual display `:2` (`Xvfb`) renders **nothing** — GPUI's Vulkan renderer
cannot present there, and the window captures as a single flat colour. That was the failed
experiment. But the process is otherwise **completely alive**: PTYs spawn, git is read, the ACP
client connects, and all 54 control-socket methods answer.

So: **no pixels, full behaviour.**

## What it can and cannot prove

This boundary is the whole discipline of the lane. Cross it and you will file a `PASSED` that a
display critic then has to reverse.

| provable here | not provable here |
|---|---|
| A control changes real app state | Where a menu appears |
| A message reaches the agent and streams back | Colours, spacing, fonts, COSMIC/comet parity |
| A file stages, unstages, discards | Whether a widget is visible or occluded |
| A tab opens, closes, renames, reorders | Z-order, overlap, clipping |
| An event has a subscriber that acts | That a label reads correctly on screen |
| A path is carried, or discarded (`F-CHG-13`) | Anything in the visual bar |

The lane is *stronger* than a screenshot for the two defects that cost us rows on 2026-08-13/14:

- **`F-CHAT-14`** — *Follow Edited Files* flipped a bool only its own label read. A screenshot shows
  the toggle move. `surface.chat.read` shows that nothing downstream changed.
- **`F-TERM-UI-02`** — `TerminalLinkEvent` is emitted with no subscriber. The screen looks identical
  whether or not a handler exists; the state readback does not.

It is *weaker* for anything the user sees. **A row whose clause describes appearance still needs the
display lock.** Say which lane produced your evidence, always.

## Standing it up

```bash
# once per machine — the display already exists if `xdpyinfo -display :2` answers
Xvfb :2 -screen 0 1715x972x24 &

# per agent — YOUR OWN db and socket, or you will fight another agent for state
env -u WAYLAND_DISPLAY DISPLAY=:2 \
    VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
    TILLER_DB=/tmp/<you>.sqlite TILLER_SOCKET=/tmp/<you>.sock \
    nohup rust/target/debug/tiller > /tmp/<you>.log 2>&1 &

export TILLER_SOCKET=/tmp/<you>.sock
rust/target/debug/tillerctl ping     # -> pong
```

Two traps in that snippet, both cost me a cycle:

- **The socket path must be under 108 bytes.** That is a kernel limit on `sockaddr_un`, not a
  Tiller limit. The scratchpad path is 118 bytes and fails with
  `[control] failed to start: socket path too long`. Use `/tmp/`.
- `VK_ICD_FILENAMES` (lavapipe) suppresses the fatal
  `vulkan: No DRI3 support detected - required for presentation`. The window still never paints, but
  the process starts clean. Keep it; the two remaining `libEGL … DRI3` lines are harmless noise.

## The rule that will otherwise cost you a false defect

**A first read can be a race, not a finding.**

`surface changes read` returned `0 0 0` against a worktree with 177 modified files. That looks
exactly like a dead surface. It was async load latency — three seconds later the same command
returned `0 36 141 true`, matching `git status --short` (36 modified + 141 untracked = 177) and the
shell prompt's own `?141 ~36`.

**Read twice, a beat apart, before you believe a zero.** This is the same shape as the `head -5`
trap in `QUEUE.md`: an instrument's first answer looked like evidence of absence and was evidence of
nothing.

## Verified working, 2026-08-14

Each of these was run, not assumed. A `capabilities` listing is a *static list* — it names methods,
it does not prove them, which is failure-mechanism (g) in `QUEUE.md`. Exercise, don't trust.

| command | observed |
|---|---|
| `ping` | `pong` |
| `capabilities --json` | 54 methods |
| `project add <path> --json` | `{"added":"true","worktreeCount":"2"}` |
| `list-workspaces --json` | both worktrees, correct branches, correct `selected` |
| `panel create` | `pane-<pid>-1` |
| `panel write --input … --enter` + `panel read` | shell ran `$((6*7))`, returned `HEADLESS_PTY_OK_42` |
| `surface changes open` / `read` | `0 36 141 true`, agreeing with `git status` |

`panel read` returns **base64**. To read it:

```bash
tillerctl panel read <id> | tr -d '\n' | base64 -d | sed 's/\x1b\[[0-9;]*m//g'
```

## `surface.chat.*` is NOT the chat you see — read this before using it

The socket advertises `surface.chat.open|send|compose|permission|stop|read` and the server really
implements them (`main.rs:1230`–`1265`, with genuine parameter validation). **`tillerctl` has no
`chat` subcommand**, so they are reachable only as raw line-delimited JSON — `ControlRequest` is
`{"method": …, "params": {…}}` with **string-valued params** (`surfaceId`, `text`, `requestId`,
`optionId`).

But there are **two independent ACP paths in production**, and they are not the same object:

| path | entry point | who drives it |
|---|---|---|
| the chat the user sees | `tiller_ui::chat::ChatView::launch_with_command` (`chat.rs:607`) | the UI |
| the chat the socket drives | `tiller_acp::ChatSession::launch` (`main.rs:905`) | `surface.chat.*` |

`tiller_ui/src/chat.rs` contains **zero references to `ChatSession`**; the only production caller of
`ChatSession::launch` is the socket handler, everything else is tests. The two share the lower-level
`tiller_acp` crate, not their session state.

**So `surface.chat.*` proves the ACP protocol layer and the control-socket chat path. It does not
prove the chat surface.** A row whose clause describes what the user types, sees streaming, or
approves still needs the display lock and the UI. Closing chat-UI rows with socket evidence would be
failure-mechanism (g) — *a channel that reports success for work it never does* — applied at scale.

Still untried and still worth an hour: `browser.*`, `notification.*`, `session.restore`, `tab.*`,
`pane.*`, `surface.settings.*`.

## Cleaning up

Kill only **your own** process. Match on the environment, never on the name — an agent driving the
real display has a `tiller` process too, and killing it destroys a live drive:

```bash
for p in $(pgrep -x tiller); do
  tr '\0' '\n' < /proc/$p/environ | grep -q '^TILLER_SOCKET=/tmp/<you>.sock' && kill $p
done
```
