# P120 report — the socket is not the only instrument

Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. This
re-drives the 21 rows from `P120-the-socket-is-not-the-only-instrument.md` that Slice C (`P116`)
put in the wrong lane. Each row below names the instrument used, what was actually driven, what it
did, and the capture path. No verdict column — `pireview` sets verdicts — but I say plainly where I
believe a row is `UNREACHABLE` and why.

Captures live under `reference/linux-progress/p120/shots/` (screenshots) and
`reference/linux-progress/p120/logs/` (D-Bus transcripts, hash comparisons, JSON evidence).

Wayland-lane instance used throughout: `TILLER_WL_KEEP=1` sway/tiller pair, `TILLER_SOCKET=/tmp/p120a.sock`,
`TILLER_DB=/tmp/p120a.sqlite`, booted via `Scripts/wayland-drive.sh`, driven interactively afterward
through a helper script sourcing its saved connection env (`/tmp/p120a.env`) so the compositor+app
boot cost is paid once, not per interaction.

## Instrument 4 — a real agent in a real pane

### F-CORE-ACT-19 / F-CORE-ACT-20 (conjunction, split)

The ledger's blocker was real: `pane_agents` identity is written only by `add_agent_tab`, which is a
live UI gesture, not a socket method. So I drove the actual gesture. On the rendered Wayland lane, I
clicked the tab-bar `+` button (`new-tab-button`, confirmed at pixel `(978, 49)` in the 1400×900
frame — two earlier attempts at `(973,48)` and `(715,48)` missed; the first because a `shot()` call's
resolution toggle happened *after* the click and before the capture, visually relocating the already-fired
click's cursor; the second because the coordinate itself was off the button). The menu opened —
`reference/linux-progress/p120/shots/new-tab-menu-try3.png` — with `Claude Code`, `Codex`, `OpenCode`,
`Pi`, `Oh-My-Pi` as direct top-level items (no submenu needed). I clicked `Claude Code`.

This spawned a real pane: sidebar and tab bar both show "Claude Code", the status bar's Activity
segment reads "1 running", and `panel.list` over the control socket confirms
`{"id":"pane-3","agent":"claude","tab":"Claude Code",...}` — identity is registered.
Independently, `/proc/<pid>/environ` for the real `tiller` process (pid 3053396) walks down to a real
`claude` CLI process tree (pid 3054288, `.../claude-agent-sdk-linux-x64/claude --output-format
stream-json ...`), so this is a genuine agent launch, not a stub.
Capture: `reference/linux-progress/p120/shots/act19-claude-code-clicked.png`,
`act19-claude-code-settled.png`.

With identity now live, I drove `notify` through `tillerctl` (not the raw socket, to match the CLI
surface P96/the task doc names) against `pane-3`, alternating the pane's visibility:

- **Background** (`tab.select index=1`, switching the active tab to Chat so `pane-3` is not the
  active tab's pane — confirmed via `panel.list` showing `"active":"false"` for pane-3): sent
  `tillerctl notify --session pane-3 --status done`. A `dbus-monitor` filter on
  `org.freedesktop.Notifications`/`Notify` running throughout captured a real `Notify` method call:
  `app-name="Tiller"`, `title="Claude Code — finished"`, `body="linux/gpui-waku · tiller"`. This is
  `post_desktop_notification` shelling out to the real `notify-send`, landing on the real session bus.
  Capture: `reference/linux-progress/p120/logs/act19-act20-notify-background-fired.log`. A positive
  control (manual `notify-send` while the monitor ran) confirms the monitor itself is not blind:
  `reference/linux-progress/p120/logs/act19-positive-control-dbus.log`.
- **Foreground** (`tab.select index=4`, making `pane-3` the active tab — confirmed
  `"active":"true"`): sent `tillerctl notify --session pane-3 --status error` (a fresh status, so
  `old == Some(new)` doesn't itself explain a no-op). No `Notify` call appeared in five seconds of
  monitoring. This matches `NotificationPolicy::should_notify`'s `!(app_active && pane_visible)` gate
  with `app_active` hardcoded `true` at the call site (`main.rs:3746`) — a visible pane is suppressed.
  Capture: `reference/linux-progress/p120/logs/act20-notify-foreground-suppressed.log`.

Both rows are driven end to end, live, both directions of the suppression gate exercised. My first
attempt at the background case (before I'd verified the monitor was up long enough) produced no
signal and is not included as evidence — the version above was re-run cleanly and is reproducible via
the same commands.

### F-AGENT-SAFE-02

Real agent, real pane (`pane-3`, from the ACT-19/20 drive above). `sha256sum` on
`~/.claude/settings.json` and `~/.codex/config.toml` before the "Claude Code" click and after the
real `claude` process tree was confirmed live via `/proc/<pid>/environ`:

```
BEFORE  b6196e4ced65a12a9362e4bfd7c258498ebeb25b56985850ce2e83fb723195c3  ~/.claude/settings.json
BEFORE  e43c150bd0563862b2af661f3805a10241a64bfbca0d6e45ad7c2d71bb1fb618  ~/.codex/config.toml
AFTER   b6196e4ced65a12a9362e4bfd7c258498ebeb25b56985850ce2e83fb723195c3  ~/.claude/settings.json
AFTER   e43c150bd0563862b2af661f3805a10241a64bfbca0d6e45ad7c2d71bb1fb618  ~/.codex/config.toml
```

Byte-identical. `prepare()` did not touch user-global config. To confirm `prepare()` actually ran
(and this isn't just "nothing happened at all"), I checked what it *did* write:
`<worktree>/.claude/settings.local.json` now contains a real hook block wired to this exact pane —
`tillerctl notify --session pane-3 --status ... --stdin-json` for `Stop`/`Notification`/
`SessionStart`/`UserPromptSubmit`/`SessionEnd` — worktree-local, not global. Capture:
`reference/linux-progress/p120/logs/safe02-sha256-before-after.txt`,
`safe02-worktree-local-hooks.json`.

## Instrument 1 — the rendered Wayland lane

### F-CORE-FILE-08

Incidental to the FILE-06 drive below: the Changes surface and Files panel both render distinct
glyphs per file type (`.md`, `.rs`, `.png`, `.toml`, directories vs. files) in the same capture set —
e.g. `reference/linux-progress/p120/shots/03-changes-open-clean.png` and the Files-panel-open
captures from the ACT-19 drive (`act19-claude-code-clicked.png` shows the Files tree with distinct
icons for `.md`/`.rs`/`Tiller.xcodeproj`/plain files). `file_glyph` (`right_panel.rs:994`) is
live and rendering in the actual running app, not just reachable from tests.

### F-USE-01

Status bar exposed no refresh affordance under hover or click in any capture across this session —
contrasted directly against Settings → AI Providers, which has an explicit "Refresh now" text
control (visible in `1786725407070998351-settings-ai-providers.png`). The status bar segments (git
branch, agent usage meters) have no equivalent control anywhere in the bar itself.

### F-USE-02

Hovered the Codex usage segment in the status bar with the persistent virtual pointer, held position,
and captured: no tooltip appeared, twice, under two different conditions —
`use02-hover-codex-nostretch.png` (real PATH, valid Codex credentials, segment showing live data) and
`use02-hover-nopath-nostretch.png` (PATH stripped of `~/.local/bin`, see USE-03 below). Neither shows
a tooltip. I could not additionally prove the "tooltip on an *unavailable* segment" half specifically,
because stripping PATH did not actually flip the status bar into an unavailable state (see USE-03) —
so this row's "no tooltip, period" claim is driven for the states I could reach, and the
unavailable-segment sub-case stays open pending a way to force that state safely.

### F-USE-03

Relaunched the Wayland-lane instance (`p120c`) with `~/.local/bin` removed from `PATH` so `claude`/
`codex` binaries are not resolvable. Settings → AI Providers correctly reflected this:
`"available":"false"`, `"status":"Not found on PATH"` for the affected providers
(`02-use03-providers-nopath.png`). The bottom status bar, however, kept showing fully live, successful
usage values regardless of the stripped PATH (`1786725734612025838-use03-statusbar-nopath.png`) — its
fetch path is decoupled from Settings' PATH-based availability check. I judged forcing a genuine
status-bar-side "unavailable" state (e.g. by corrupting real OAuth credential files) to be out of
proportion to what this row needs and risked real credential damage, so I stopped at this honest
boundary: the row is driven for the PATH-strip instrument, and it surfaces a real product finding
(Settings and the status bar disagree about availability) rather than the specific tooltip-on-unavailable
behavior. Capture: `02-use03-no-cli-installed.png`, `02-use03-providers-nopath.png`,
`1786725734612025838-use03-statusbar-nopath.png`.

### F-CORE-AUTH-01

Settings → AI Providers → Claude Code → "Add Account". The ledger's premise ("0 app references" to
`parse_claude_json`, paired with "`F-SET-14`'s dead Add Account button") is wrong about the button:
clicking it is not dead. It spawned a real `x-terminal-emulator -e claude auth login` process, which
itself ran `xdg-open` against a genuine `https://claude.com/cai/oauth/authorize?...` PKCE URL in the
real system browser (Brave), for the real signed-in account. Capture:
`1786725460554010974-auth01-add-account-clicked.png`.

I deliberately did not complete this flow — no code was pasted, nothing was authorized — because doing
so would mint a real credential against the user's real account, and the instrument I have (a nested
nested Wayland browser, not visibly separated from the user's real desktop/account state) is not one I
can safely drive to completion for a coverage row. I terminated the whole process group
(`kill -TERM -<pgid>`) and verified nothing remained. So: the button and the OAuth-initiation half are
driven and real. `parse_claude_json` itself — what happens after a real code comes back — remains
unexercised, and I don't think it can be exercised responsibly from this instrument without either a
disposable/sandboxed OAuth client or explicit user participation. Capture:
`1786725547285656214-after-auth01-abort.png` (state after abort, account untouched).

## Instrument 2 — the filesystem

### F-CORE-FILE-06

Opened the Changes surface, then edited the worktree from outside the app entirely, three
transitions, each read back live with no app restart:

1. `echo >> README.md` (external modify) — Changes surface picked it up as a live unstaged change on
   the next read. Capture: `1786725326938102356-file06-external-modify-retry.png`.
2. `mv README.md README.md.bak` (external delete, simulating a real delete since the file no longer
   exists at its tracked path) — Changes surface reflected the deletion live. Capture:
   `1786725371276811386-file06-external-delete.png`.
3. `mv README.md.bak README.md && git checkout -- README.md` (restore to clean) — Changes surface
   returned to zero pending changes, confirmed against `git status --porcelain README.md` before and
   after. Capture: `1786725386644971816-file06-restored-clean.png`.

`FileSystemEventMonitor` → `FileView::poll_file_system_events` → `check_external` is live and reacts
to real out-of-band filesystem changes without requiring the app to have caused them.

## Instrument — code trace confirming the correct instrument, not yet reachable this pass

### F-CORE-FILE-03

`link_router.rs`/`TerminalView::receive_file_drop` aside, this row is specifically about the terminal
accepting a real file drop. The Wayland lane's own documentation (`WAYLAND-LANE.md`) states drag
gestures are not yet exercised there — my persistent virtual pointer only speaks `move`/`click`, not
a press-hold-move-release XDND sequence. This needs either an extended virtual-pointer driver (button
press, motion while held, release, matching the wlr XDND source flow) or the `:1` lane, which was
still lock-held by another pane's session at last check (`pid=1728940 label=sonnet`) past its own
1800s staleness window. I have not yet retried acquisition. **I believe this is likely to stay
`UNREACHABLE` from the Wayland lane specifically** (no drag primitive exists in the current
instrument) even though it should be reachable from the `:1` lane once the lock clears — that attempt
is still pending, tracked below, not concluded.

### F-TERM-UI-02

Confirmed via code trace (`tiller_terminal/src/link_router.rs::opens_terminal_link` requires
`platform_modifier`, gated on GPUI's `platform` modifier — Super, on Linux — held during the click,
`lib.rs:988`) that this needs a modifier-chord click the Wayland lane's virtual pointer/keyboard pair
cannot yet compose (the persistent `wtype` keyboard process and the virtual-pointer FIFO are two
separate uncoordinated input sources; holding Super on one while clicking on the other has not been
attempted and isn't obviously synchronizable). This needs the `:1` lane. Not yet attempted this pass —
tracked below.

## Instrument 3 (database) and remaining rows — not yet driven this pass

The following rows from the assigned 21 have not been driven yet in this pass and are **not being
reported as `UNREACHABLE`** — they simply have not been attempted:

`F-CORE-ACT-24`, `F-CORE-ACT-25`, `F-CORE-ACT-26` (instrument: multi-worktree Wayland session +
restart, watching for observable bootstrap-order/mount-eviction effects), `F-CORE-USG-05`,
`F-CORE-USG-06`, `F-CORE-USG-07` (instrument: Codex token-refresh path — needs a safe way to force a
refresh/failure scenario without mutating real credentials), `F-CORE-DOM-03` (instrument: "+ Add
Project" gesture, confirm native-file-picker behavior headlessly), `F-CORE-SET-01` (instrument:
malformed row written directly into `/tmp/p120a.sqlite`'s settings table, then restart), `F-AGENT-API-01`
(instrument: `system.capabilities` — **partially checked as a side effect of this pass**: the live
53-method list returned by `ctl system.capabilities` against `p120a` has no summarizer-named method;
formal write-up pending), `F-GIT-RUN-02` (instrument: "New Worktree..." click + rapid successive
shots, checking for/against incremental progress UI), `F-CORE-FILE-03` / `F-TERM-UI-02` (see above,
`:1` lane retry pending).

This report will be updated in place as the remaining rows are driven.
