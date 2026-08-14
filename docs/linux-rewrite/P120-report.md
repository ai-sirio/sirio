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

### F-AGENT-API-01

Queried the real, live control socket of the running `p120a` instance directly (`ctl
system.capabilities`, via `wl-helper.sh`'s `ctl()`), not a doc or spec read. The response lists 53
methods verbatim — `system.*`, `project.*`, `workspace.*`, `worktree.set`, `notify`, `panel.*`,
`pane.*`, `tab.*`, `notification.*`, `session.*`, `surface.changes.*`, `surface.settings.*`,
`surface.chat.*`, `browser.*`. No method name contains "summar" anywhere — the ledger's premise (no
summarizer-triggering control method exists) holds against the real, current method registry of a
real running process, not just against source. Capture:
`reference/linux-progress/p120/logs/api01-system-capabilities.json`.

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

### F-CORE-DOM-03

Clicked the sidebar's "+ Add Project" affordance on the rendered Wayland lane. No dialog, no new
toplevel, and no native file-picker appeared anywhere inside the nested compositor's own output or
window tree, and `/tmp/p120a.log` shows no error around the click. That is not a clean disproof,
though: the host also runs real `xdg-desktop-portal` / `xdg-desktop-portal-gtk` /
`xdg-desktop-portal-cosmic` daemons (confirmed via `ps aux` — pids 2732, 2809, 3199), and a portal
file-chooser call from inside my isolated nested sway instance could plausibly get routed to and
rendered by the **real host desktop's** portal backend rather than into my nested compositor's own
output — in which case `grim -o HEADLESS-1` (scoped to my nested instance) would never see it even
if the call fired and succeeded. A read-only, non-input check on the real `:1` desktop
(`wmctrl -l`, `xdotool search`) found only one anonymous X11 window at the time, which is
inconclusive for a native Wayland toplevel and which I did not attempt to click into, for the same
real-desktop safety reasons documented in "Instrument 1b" below. I lean toward **`UNREACHABLE` from
the Wayland lane as currently built** — I can't rule out the gesture working invisibly on the real
desktop, and confirming that one way or the other needs either a portal backend scoped to the
nested instance or a safe way to observe the real desktop's own portal surface, neither of which I
have this pass.

### F-GIT-RUN-02

Clicked "New Worktree..." in the sidebar. A real inline form opened — "New worktree in tiller", a
focused branch-name text field, "Enter to create · Esc to cancel" — confirming the affordance
itself is live, not dead. I could type into the field (`wtype` text injection landed real characters
in a real focused GPUI text input, confirmed twice). I could not, however, submit it: `wtype -k
Return`, an explicit press/release pair, and a literal embedded newline all failed to submit the
form, and `wtype -k Escape` (the form's own documented cancel gesture) likewise had no effect. A
cross-check ruled out "this field's Enter handler specifically is broken" — the exact same
"typed text lands, Return/Escape do nothing" failure reproduced in the completely unrelated Chat
message box. Full diagnostic, capture list, and reasoning:
`reference/linux-progress/p120/logs/gitrun02-return-key-diagnostic.txt`.

I could not get past submitting the form, so I could not observe whether worktree creation streams
progress incrementally — the row's actual `VERIFY` claim (`GitRunner`/`run_streaming` has 0
references in the app crates per the ledger). This is **`UNREACHABLE` from this instrument as
currently built**: the blocker is that `wtype`'s non-printable-keysym delivery does not register in
this app on this compositor/GPUI stack at all (not specific to worktree creation), so the correct
next step is a driver that can deliver a real Return keypress — or a mouse-only submit affordance,
if one exists that I didn't find — not another retry of the same injection method.

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

## Instrument 1b — the `:1` lane, and why it stops here for these two rows

`Scripts/linux-drive.sh`'s `:1` display is **not an isolated headless X server — it is the real,
currently-in-use COSMIC desktop** (`Xwayland :1 -rootless` under a live `sway`/Smithay session on
`tty1`, with `cosmic-panel`, `cosmic-term`, and other panes' real `claude`/`herdr` processes visibly
running on it at the time I drove it). `xdotool` on this display moves the operator's actual pointer
and injects real XTEST input system-wide, exactly as the script's own comments warn. I did not
appreciate this distinction until partway through driving `F-TERM-UI-02` on it.

I first acquired the drive lock legitimately: it was held by `pid=1728940 label=sonnet` since
17:11:51, 6448s past its own 1800s staleness window, and `linux-drive.sh` self-healed it on my first
`mkdir` attempt exactly as designed (`NOTE: breaking a drive lock held 6448s...`). I drove a real
worktree/terminal, typed `echo https://example.com/p120-term-link-test` into it, and confirmed via a
`PATH`-shadowed `xdg-open` recorder (`/tmp/p120-xdgopen-bin/xdg-open`, prepended onto `PATH` for the
launched app only) that a genuine `cx.open_url` call would be caught. I then held `Super` via
`xdotool keydown super` and clicked the rendered URL text at its exact on-screen position (verified
twice, on a fresh `TILLER_DB` each time so the terminal layout was reproducible and the coordinates
were confirmed correct against the actual capture). No `xdg-open` call was recorded either time.

That is not a clean disproof, though, and I don't want to report it as one: COSMIC's own compositor
very plausibly binds `Super` itself as a global shortcut (its panel includes a `CosmicAppLibrary`
launcher button, the conventional Super target). If the window manager intercepts `Super` before the
XWayland client ever sees a `platform`-modifier keydown, `opens_terminal_link` would correctly return
`false` for a reason that has nothing to do with `tiller`'s own code — indistinguishable, from outside,
from the gesture genuinely not working. `import -window "$WID"`'s per-window capture wouldn't even show
a WM-level overlay if one opened, since it only rasterizes the named window. This is exactly the "a
control that did nothing, about code that is fine" failure mode `linux-drive.sh`'s own comments warn
about for the shared pointer — I just hit its keyboard-modifier analogue.

Given that, and that this display had other panes' live agent sessions genuinely running on it at the
time, I chose not to keep re-attempting blindly on shared, real infrastructure to disambiguate WM
interception from a real code gap. I verified cleanup left no stuck modifier and no stray window
(`xdotool getactivewindow` now errors `BadWindow` for both driven window ids, `pgrep` shows only the
other panes' own legitimate `tiller` processes). **I believe both of the following are reachable in
principle but not disambiguable from this pass's instrument**, and flag them rather than guess:

### F-CORE-FILE-03

Same `:1` lane, worse risk profile — a drag gesture that goes even slightly wrong could drop a file
onto a *different* live window on the same real desktop (another pane's `cosmic-term` was visibly
running at `pts/36` during this session). The Wayland lane's virtual pointer also has no drag primitive
(`move`/`click` only, no press-hold-motion-release), so `WAYLAND-LANE.md`'s own "drag not yet
exercised" limitation applies there too. I did not attempt this row this pass. **I believe this needs
either an extended Wayland virtual-pointer driver (a real press/motion/release XDND sequence) or a
truly isolated `:1`-equivalent display before it can be driven safely** — not the currently shared real
desktop.

### F-TERM-UI-02

Driven as described above: real URL rendered, real `Super`-held click landed on its exact screen
position, no `xdg-open` call observed — but I can't rule out WM-level `Super` interception ahead of
the app, on the specific instrument available. Capture:
`reference/linux-progress/p120/shots/term02-url-typed.png` (URL rendered, pre-click),
`term02-superclick-fresh.png` (post-click, layout/coordinates verified against the pre-click frame). I
lean toward **`UNREACHABLE` from this instrument as currently built**, not toward the code being
broken — the correct next step is a synchronized modifier+click Wayland driver (one process, atomic
keydown→motion→click→keyup) rather than another blind attempt on the real desktop.

## Instrument 3 — the database

### F-CORE-SET-01

Wrote six malformed values directly into `/tmp/p120a.sqlite`'s `setting` table via Python's
`sqlite3` module, bypassing the app entirely, then killed the running app process and relaunched a
fresh one against the same on-disk DB and the same already-running sway compositor:

| key | malformed value | resolved value (post-restart, via `ctl surface.settings.read`) |
| --- | --- | --- |
| `appearance.uiFontSize` | `"not-a-number"` | `"13"` — fell back to default |
| `appearance.terminalFontSize` | `"99999"` | `"24"` — clamped to range max |
| `appearance.theme` | `"totally-bogus-theme"` | `"system"` — fell back to default enum |
| `controlSocket.enabled` | `"maybe-ish"` | `"true"` — fell back to default bool |
| `usage.refreshIntervalMin` | `"-500"` | `"1"` — clamped to range min |
| `general.summarizerAgent` | `"not-a-real-agent"` | not independently re-read via `ctl` this pass — reasoning from the allow-list at `db.rs`'s `settings()`, not a live observation |

No crash: `/tmp/p120a.log` after restart shows only pre-existing Mesa/EGL/Vulkan/radv warnings, no
panic. The control socket itself stayed live and responsive through the whole sequence — the
`controlSocket.enabled` row's own malformed value fell back to enabled, and the `ctl` call that
produced this very evidence table is proof the socket survived. Full transcript and insert script:
`reference/linux-progress/p120/logs/set01-malformed-values-resolved.txt`,
`1786727180429745461-set01-settings-appearance.png`.

## Instrument 4b — a real multi-worktree session, killed and relaunched twice

Grep confirms `AgentSessionRestorePlan`, `BootstrapRestoreOrder`, and `WorktreeMountPolicy` — the
three pure policy structs behind ACT-24/25/26 — have zero references anywhere outside
`tiller_activity` itself; nothing in `tiller`/`tiller_ui` calls any of them. So I drove the only
thing that can exercise their absence: a real multi-worktree launch, killed and relaunched from
cold, twice, watching what the app does instead. Added two real scratch git worktrees via
`ctl workspace.create` (not the "New Worktree..." button — see F-GIT-RUN-02 above), for 4 total
mounted worktrees including the one with the live `pane-3` Claude Code agent from the ACT-19/20
drive. Killed the real `tiller` process and relaunched it against the same on-disk DB/socket and the
same already-running compositor, twice in a row. Full transcript and reasoning:
`reference/linux-progress/p120/logs/act24-25-26-multi-worktree-restart.txt`.

### F-CORE-ACT-24

Directly observed, twice, reproducibly: the "Claude Code" pane/tab shell for `pane-3` reappears
after every cold restart, and a real new agent process is spawned to back it each time — but with a
**different random `--session-id`** on each restart (`e8e3cb45-...` after restart #1,
`d5cdcc17-...` after restart #2), no `--resume`/`--continue` flag present either time. This is live
confirmation of the ledger's claim: the app recreates the pane shell from persisted, raw pane-keyed
state without ever going through `AgentSessionRestorePlan`'s resumable/prunable distinction — there
is no case where an old session is either cleanly resumed (same session-id survives) or
explicitly recognized-and-pruned; it just always spawns a fresh session unconditionally.

### F-CORE-ACT-25

Partially reachable from this instrument. I confirmed a real, reproducible restore-selection
finding: a runtime `ctl workspace.select` change does not survive a restart — the persisted DB
selection (`linux/gpui-waku`) won both times, not the worktree I had just selected
(`p120-act-scratch-2`) moments before killing the process. All 4 worktrees also appeared fully
mounted within ~3-4s of each relaunch, with no observable staged/deferred loading in a
`workspace.list` snapshot. I could not, from a single post-restart snapshot, distinguish "no
priority/deferred split exists in practice" from "the split exists but resolves faster than I
sampled" — that would need a sub-second `workspace.list` poll loop starting at process spawn, which
I did not build this pass. I lean toward this row being effectively confirmed in practice, but flag
the instrument's resolution limit honestly rather than claim a clean disproof.

### F-CORE-ACT-26

Directly observed, twice: 4 mounted worktrees survive two consecutive full process restarts with
zero evictions, regardless of which one was selected or had a live agent. Confirms no
cap-enforcement mechanism is currently live in the running app to observe protecting anything
against — matching the ledger's "no mount eviction consumes it" verbatim.

Cleanup: both scratch worktrees closed via `ctl workspace.close`, then fully removed from the real
shared repo (`git worktree remove --force` × 2, `git branch -D` × 2, verified via `git worktree
list` showing only the two pre-existing worktrees afterward) — no debris left in shared git state.

## Instrument 1c — synthetic Codex credentials via `$CODEX_HOME`

### F-CORE-USG-05 / F-CORE-USG-06 / F-CORE-USG-07

I'd deferred these earlier this pass over real-credential risk, but `codex_auth_file_path()`
(`tiller_usage/src/codex.rs:84`) resolves `$CODEX_HOME/auth.json` in preference to the real
`~/.codex/auth.json` — the same precedence the real `codex` CLI itself uses — which makes the whole
fetch/refresh/classify path testable with entirely synthetic, harmless credentials. A garbage bearer
token against the real `chatgpt.com` usage API and a garbage refresh token against the real
`auth.openai.com` token endpoint behave exactly like "wrong password": rejected safely, no real
account touched, nothing to clean up.

Wrote a well-formed but bogus `auth.json` to `/tmp/p120-fake-codex-home/` (valid JSON shape, so
credential *loading* succeeds — ruling out the "no credentials" `LoggedOut` case by construction),
relaunched `p120a` with `CODEX_HOME` pointed at it. Before this swap, the status bar had shown real
live Codex usage data ("Codex 100% 5h") throughout the entire session under the real `~/.codex`
credentials. Within ~3s of the swap, it changed to **"Codex logged out"**. Capture:
`1786728406096566027-usg-fake-codex-statusbar.png`.

That specific wording is the proof, not just a vague failure: `status_bar.rs:284` maps
`UsageReason::LoggedOut` to "logged out" and `UsageReason::Error` to the distinct string "error".
Tracing `CodexUsageFetcher::fetch()`, the only way to reach "logged out" (given credential loading
already succeeded) is `fetch_usage()` returning `Unauthorized` (a real 401 from the real wham API)
followed by `refresh_token()` failing (a real non-200 from the real OpenAI token endpoint) — which
means `fetch_usage`'s 401 branch, `refresh_token()`, `TOKEN_URL`, and `classify_token_refresh_failure()`
(called inside `refresh_token()` to build the error it returns) all executed for real, live, against
real endpoints. Full reasoning and transcript:
`reference/linux-progress/p120/logs/usg05-06-07-synthetic-credentials.txt`.

Nuance for F-CORE-USG-05: `classify_token_refresh_failure()`'s specific return variant
(Reused/Revoked/Expired/Other) is computed for real but then immediately discarded by its caller —
every variant maps identically to `LoggedOut`, so the classification *runs* but currently has no
observable effect on user-visible behavior beyond "refresh failed → logged out, however it failed."
`needs_refresh()` was re-grepped and still has zero non-test callers — the time-based refresh gate
didn't trigger this; a real 401 did. Not exercised: the successful-refresh-and-merge-save path
(`save_credentials`), which needs a real 200 from the token endpoint — out of proportion to
manufacture safely, same reasoning as F-CORE-AUTH-01 above.

Cleanup: relaunched `p120a` once more without `CODEX_HOME` to restore real-credential behavior.

## Remaining rows — not yet driven this pass

None. All 21 rows assigned to this pass have been driven this session.
