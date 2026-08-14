# Drive slice E10-persist+sid+agent+per — evidence log

Driven on the Wayland lane only (`TILLER_WL_LABEL=drive-E10-persist+sid+agent+per`), one
row at a time, committed after each. HEAD under test 4073297, worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. No `rust/`
source read or edited.

## F-SID-19 (ledger line 88)

**Precondition owed by the ledger note**: reach a selected worktree with **zero tabs** (the
"no-terminal empty state"), not just "existing tabs" (P104's gap). **Gesture**: press
ctrl-t and confirm the empty state is replaced by a terminal tab.

Route: `project.add` a scratch git fixture, click the worktree row (auto-opens Chat +
Terminal), then close each tab via its header `x` button and confirm the "Close dirty tab?"
platform prompt (both tabs are backed by a live PTY/agent process, so both are "dirty").
Closing the *last* tab renders the real empty state — icon, "No Terminals" heading, "Open a
new terminal to get started.", and a "New Terminal" button (`empty-worktree-new-terminal` in
the source) — captured at
`reference/linux-progress/drive-E10-persist+sid+agent+per/03-empty-retest.png`. This is a
genuine, code-level empty state, not a stale/absent one — contrary to the older F-SID-18
"no empty state" note, which this drive did not re-adjudicate but visibly contradicts.

**Instrument caveat that cost most of the drive time**: `wayland-drive.sh`'s virtual keyboard
is only started when the literal word `type` or `key` appears as a token in the action
script; calling `wtype` directly (to get a `ctrl+t` chord, which the `key()` wrapper can't
express since it takes one bare key name) silently no-ops if that trigger word is absent —
the ad-hoc wtype client races the app's `wl_keyboard` bind and loses. Confirmed with a
positive control first: clicking "New Terminal" by mouse *did* replace the empty state
(`04-after-real-click-newterminal` region of
`reference/linux-progress/drive-E10-persist+sid+agent+per/02-after-real-click-newterminal.png`),
proving the empty state and its button both work, while two prior `ctrl+t` attempts with no
keyboard-trigger word in the action script produced byte-for-byte unchanged captures. Once a
harmless `key Escape` was added earlier in the same action script (starting the virtual
keyboard for real), the identical `wtype -M ctrl -k t -m ctrl` chord worked immediately.

**Result**: with the virtual keyboard actually live, `ctrl-t` from the confirmed empty state
(`03-empty-retest.png`) opened a fresh Terminal tab and replaced the empty state
(`04-ctrlt-with-keyboard-init.png` — tab strip now reads "Terminal", live shell prompt
visible, sidebar shows one child tab under the worktree). This is exactly the clause's ask
(`01-inventory-app.md:36` — "Select a worktree with no tabs, press ⌘T, and confirm the empty
state is replaced by a terminal tab"; Linux binds ctrl-t for the same
`WindowCommand::NewTerminalTab`, per `P94`'s vocabulary-row note already on this ledger row).

Both halves driven live: the precondition (real empty state, confirmed reachable and
photographed) and the replacement gesture (ctrl-t, confirmed to replace it). Discriminating:
the empty state does not spontaneously contain a terminal, and the two "before" and "after"
captures are pixel-different only after the correctly-instrumented keystroke.

Captures: `reference/linux-progress/drive-E10-persist+sid+agent+per/03-empty-retest.png`,
`reference/linux-progress/drive-E10-persist+sid+agent+per/04-ctrlt-with-keyboard-init.png`,
`reference/linux-progress/drive-E10-persist+sid+agent+per/02-after-real-click-newterminal.png`
(positive control).
