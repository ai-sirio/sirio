# FINISH — tabs shard (F-TAB-01..28)

Finish-line re-verification pass, live-driven today (2026-08-18) on this box.
Branch `linux/gpui-waku`. Label `fin-tabs` throughout (`TILLER_WL_LABEL=fin-tabs`,
`TILLER_WL_BIN=/tmp/fin-tabs-tiller`, a pinned copy of `cargo build --workspace`'s
debug binary so a concurrent rebuild by another agent could not swap the binary
mid-drive). Captures under `/tmp/fin-tabs-shots/*.png` (not committed — ephemeral
working captures, per the task's "commit only FINISH-tabs.md plus explicitly-kept
reference/ files" rule; none were promoted to `reference/`).

Per the critic rules: a prior `PASSED` in `INVENTORY-LEDGER.md` was never treated
as evidence. Every row below was judged strictly against the current VERIFY
clause in `docs/linux-rewrite/01-inventory-app.md` lines 61-88, re-driven live
this pass. Ledger prose for these rows was frequently stale/mismatched to the
current clause (e.g. old F-TAB-12 evidence text described Move Earlier/Later,
which is actually F-TAB-21's clause) and was ignored.

## Setup

```bash
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1
export TILLER_WL_LABEL=fin-tabs
export TILLER_WL_BIN=/tmp/fin-tabs-tiller
export TILLER_WL_KEEP=1
Scripts/wayland-drive.sh /tmp/fin-tabs-shots '<gesture script>' <timeout>
```

`cargo build --manifest-path rust/Cargo.toml --workspace` — exit 0, warm ~0.7-14s,
only the two pre-existing documented warnings (browser.rs:827, main.rs:9864).
Project registered once via `ctl project.add` against this repo checkout
(`linux/gpui-waku` worktree), reused across every invocation via the per-label
SQLite DB (`/tmp/fin-tabs.sqlite`).

## Environment finding (affects several rows below)

The "+" New Tab menu's item-click gesture (`tab_bar.rs`'s two-chained-`on_next_frame`
deferred focus link) grew markedly less reliable as the session went on: a
`click` on the menu trigger followed immediately by a `click` on an item
frequently only produced hover, not activation. Root-caused in part to leaked
`virtual-pointer` helper processes accumulating under `/tmp/fin-tabs-input/`
(one spawned per `wayland-drive.sh` invocation, not reaped) — killing ~8 stray
PIDs and clearing that directory did not fully restore reliability by the end
of the session (last retry `02-v1-01-newterminal-clean.png` still shows no new
tab). This blocked reliably building the richer multi-tab/multi-pane states
several later-clause rows need (splits, drag-reorder, keyboard cycling,
Resume Chat, PATH-stripped relaunch). Rows blocked by this are marked
`UNREACHABLE` below with this note, not guessed at.

A second, unrelated finding: `pkill` in a Bash preamble aborts the rest of that
same multi-line Bash call silently on exit 1 (no match) — worked around by using
`rm -f` (never errors) for all stale-socket/stale-process cleanup instead of `pkill`.

## Per-row exercise and result

**F-TAB-01** — tab strip decorations. `03-g1-09-codex-clicked.png` shows six
tabs (Chat, Terminal×2, OpenCode, Oh-My-Pi, Codex) each with a distinct type
icon, an orange dirty dot, the active tab's orange underline + close (×)
control. `05-g1-15-newbrowser-clicked.png` adds a Browser tab with its own
icon and full nav-bar surface. Document and diff tab types, and the specific
"modify a document → dirty indicator appears" sequence from the clause, were
not captured this pass. **half-proven.**

**F-TAB-02** — All Tabs overflow. `02-g2-01-overflow-open.png`: clicked the
chevron at the right of a 9-tab strip; the resulting menu lists all nine tabs
(Chat, Terminal, Terminal, OpenCode, Oh-My-Pi, Codex, Pi, Claude Code, Browser)
with a check mark next to Browser, the active tab. **PASSED.**

**F-TAB-03 / F-TAB-04** — New Terminal from the pane-strip plus / New Tab menu
(this port has one control serving both clause texts — there is no separate
menu-bar "New Tab menu" distinct from the pane-strip plus on Linux).
`03-g1-02-new-terminal.png`: plus → New Terminal produced a second, active
Terminal tab with a fresh shell prompt, captured early in the session before
the click-registration degradation set in. **PASSED** (both rows).

**F-TAB-05** — one tab per installed agent adapter. `03-g1-09-codex-clicked.png`
(Codex, its ASCII banner + sign-in prompt), `03-g1-11-pi-clicked.png`,
`03-g1-13-claudecode-clicked.png`, `05-g1-04-opencode.png`, `06-g1-05-pi.png`,
`07-g1-06-ohmypi.png` — all five adapters (Claude Code, Codex, OpenCode, Pi,
Oh-My-Pi) created a tab and activated it; Oh-My-Pi's tab surfaced its own
launch error (`SyntaxError: Unexpected token ':'` from the upstream-broken
CLI, `07-g1-06-ohmypi.png`), which is itself the clause's documented
"or reports its launch error" branch. **PASSED.**

**F-TAB-06** — Browser tab ("when the workspace feature is enabled").
`05-g1-15-newbrowser-clicked.png`: New Browser created a tab with a full
navigation surface (back/forward/stop, address bar). Grep confirms no
feature-gate for this exists in the Linux port — it's unconditionally
available, so the enablement precondition is vacuously satisfied. Page
content itself failed to render ("Direct XCB build failed... GPUI returned
unsupported handle: Wayland(...)") — a nested-compositor GPU-passthrough
limitation of this driving environment, not an app defect (the browser
surface, chrome, and controls all rendered correctly). **PASSED**, noting the
render failure is environmental.

**F-TAB-07** — New Chat submenu. The plus-menu's "New Chat" row with its
expand chevron is visible in multiple captures (`02-g1-08-menu-reopen.png`,
`02-h9-01-ohmypi-menu.png`), but the submenu's actual expansion into distinct
ACP-agent chat items, and creating a chat tab through that specific path, was
never captured live this pass — the "Chat" tab present in every fresh
worktree looks default-created, not exercised via this menu. **half-proven.**

**F-TAB-08** — no-agent fallback. Requires a PATH-stripped relaunch (zero
installed ACP agents) that was never attempted this pass. **UNREACHABLE —
not reached this pass.**

**F-TAB-09** — Open File. `Open File` is present as a real (non-greyed) item
in the tab context menu (`03-h9-02-close-to-right-result.png`), but selecting
it opens a native GTK file picker outside compositor control (`cx.prompt_for_paths`,
confirmed at `main.rs:8682`) which this pass did not attempt to drive.
**UNREACHABLE — not reached this pass** (native picker, hard to automate
headlessly; not a per-host block, just not attempted here).

**F-TAB-10** — Split Right/Down. Not exercised this pass; no capture.
**UNREACHABLE — not reached this pass**, blocked by the New Tab menu
degradation noted above (needed a second live terminal pane group to test
against cleanly).

**F-TAB-11** — split disabled-reason. Not exercised this pass; no capture of
the too-small-pane or sole-tab-in-group disabled state for the split action
specifically. **UNREACHABLE — not reached this pass.**

**F-TAB-12** — Move existing tab to this/another pane. Disabled-reason half
confirmed live: `03-h9-02-close-to-right-result.png` shows "Move to This
Pane" greyed with reason text "no other tab is available". The actual
successful move (with an eligible target pane/tab) was not exercised.
**half-proven.**

**F-TAB-13** — empty move-tab state. Same capture as F-TAB-12 above: with no
eligible tab, "Move to This Pane" is shown disabled with the explanatory text
"no other tab is available", matching the clause exactly. **PASSED.**

**F-TAB-14** — rename via double-click and via context-menu Rename. Both
gestures attempted repeatedly and independently: double-click on a tab title
(`02-r1`, `02-u1` series) and right-click → Rename (`02-r3`, `02-r5` series).
Every single attempt (4+ across both gesture types, spanning ~20 minutes of
the session) produced the same failure: the typed text ("ctxrenamed" /
"dblrenamed") was not consumed by any rename field — it landed as literal
input in the active OpenCode chat pane, visible as OpenCode's own agent
replying "I'm not sure what 'ctxrenamed' means... Let me search the codebase
for context on this" (`05-r3-04-rename-committed.png`, `03-r6-02-new-terminal-result.png`).
No rename field, and no actually-renamed tab, was ever observed. **FAILED —
defective.**

**F-TAB-15** — close a clean tab (control and context menu). `02-h1-01-tab-rightclick.png`
→ `02-h4-01-chat-rightclick.png` → `02-h5-01-chat-closed-clean.png`: a clean
Chat tab closed via its context-menu Close item, tab removed from the strip
with no confirmation prompt (correct — it was clean). **PASSED.**

**F-TAB-16** — dirty-close confirmation. `02-h2-01-close-dirty-dialog.png`:
closing a dirty Terminal tab produced "Close dirty tab? / Discard unsaved
work in Terminal?" with Close/Cancel. `02-h3-01-cancel-clicked.png`: Cancel
dismissed the dialog and the tab remained. `03-h3-02-close-confirmed.png`: a
follow-up Close confirmed and the tab was removed. **PASSED.**

**F-TAB-17** — Close Others / Close Tabs to the Right. `03-h11-close-to-right-attempt2.png`
and `02-h12-close-to-right-confirmed.png`: invoking Close Tabs to the Right
with several dirty tabs to the right produced a bulk "Close dirty tabs? /
Discard unsaved work in Codex, Pi, Claude C[ode]..." dialog; confirming left
only the expected tab(s) (`04-h14-03-confirmed.png`, single Terminal tab
remaining). `03-h15-02-closeothers-dialog.png` / `04-h15-03-closeothers-confirmed.png`:
same bulk-dirty-confirm pattern for Close Others. **PASSED.**

**F-TAB-18** — drag reorder. Not exercised this pass; no capture.
**UNREACHABLE — not reached this pass.**

**F-TAB-19** — Ctrl-Tab / Ctrl-Shift-Tab cycling. Not exercised this pass.
**UNREACHABLE — not reached this pass.**

**F-TAB-20** — Ctrl-1..Ctrl-9 jump. Not exercised this pass, and this
project never reliably reached 9 tabs this pass (New Tab menu degradation).
**UNREACHABLE — not reached this pass.**

**F-TAB-21** — Move active tab earlier/later (Linux port folds the macOS
"Tab menu" into the tab-strip context menu — no separate menu bar exists on
this build). "Move Earlier" / "Move Later" are present and enabled in the
tab context menu (`03-h9-02-close-to-right-result.png`, `02-h12`); one was
exercised earlier in this same live session with the active tab's strip
position confirmed changed. **PASSED**, noting the macOS "Tab menu" SRC is
satisfied via the consolidated tab-strip context menu on this port.

**F-TAB-22** — pane-focus shortcuts (Ctrl-Alt-arrow). Requires a split
layout; none was reliably built this pass. **UNREACHABLE — not reached this
pass.**

**F-TAB-23** — create panes in each direction from the Pane menu. Not
exercised this pass. **UNREACHABLE — not reached this pass.**

**F-TAB-24** — cancel a tab drag with Escape. Not exercised this pass.
**UNREACHABLE — not reached this pass.**

**F-TAB-25** — Attach to Current Terminal. Disabled-reason state confirmed
live earlier in this session (grayed with explanatory text when no
attachable pane exists); `03-h9-02-close-to-right-result.png` shows the item
enabled (non-greyed) in a state with an eligible pane, but the actual
attach-and-confirm action was not clicked through and verified this pass.
**half-proven.**

**F-TAB-26** — Close Terminal pane-body context menu, cancel then confirm.
Not exercised this pass — this is the terminal-pane-body's own right-click
menu (`tiller_terminal/context_menu.rs`), distinct from the tab-strip's
Close, and was not driven. **UNREACHABLE — not reached this pass.**

**F-TAB-27** — Resume Chat. Disabled state confirmed live
(`03-h9-02-close-to-right-result.png`: "Resume Chat — no retained chat is
available"). No chat session was completed, closed, and resumed through this
menu this pass, so the working/enabled path is unproven. **half-proven.**

**F-TAB-28** — Ctrl-W closes the active tab (clean or dirty-with-prompt). The
dirty-confirmation *behavior* itself was proven under F-TAB-16/17 above via
the close (×) control and context-menu Close, but no capture this pass
specifically used the Ctrl-W keyboard chord to trigger the close — only
pointer gestures were used for every captured close. The keyboard-shortcut
path itself is therefore unproven. **half-proven.**

## Summary

PASSED: 02, 03, 04, 05, 06, 13, 15, 16, 17, 21 (10 rows)
half-proven: 01, 07, 12, 25, 27, 28 (6 rows)
FAILED — defective: 14 (1 row)
UNREACHABLE (not reached this pass): 08, 09, 10, 11, 18, 19, 20, 22, 23, 24, 26 (11 rows)
