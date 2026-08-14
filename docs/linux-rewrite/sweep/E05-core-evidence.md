# E05-core evidence — drive-E05-core

Lane: Wayland only (`TILLER_WL_LABEL=drive-E05-core`). Captures in
`reference/linux-progress/drive-E05-core/`. HEAD under test: `4073297`.

---

## F-CORE-ACT-02 (ledger line 338, half-proven)

**Claim: partially-exercised.** Drove the sidebar half further than the recorded evidence
and it held up; the notification (D-Bus) half is still not observed live.

**Drove:** fresh Wayland instance, `project.add path=<this worktree>` (real git worktree
scan — sidebar populated with `tiller/rust/gpui-rewrite` and `tiller/linux/gpui-waku`, the
latter's default tabs are `Chat` and `Terminal`). Pane ids are assigned by
`next_pane_id()` (`main.rs:2217`, max leaf id + 1), so a fresh worktree's `Chat` tab is
`pane-0` and `Terminal` is `pane-1` — confirmed empirically, not assumed:
- `ctl notify session=pane-1 status=needs-input` (nothing else touched) →
  `reference/linux-progress/drive-E05-core/03-05-notify-needs-input-pane1.png` shows the
  **Terminal** tab label grow a `?` glyph; the **Chat** tab is untouched. This is a
  discriminating result: a plain bash pane never produces this glyph on its own, and the
  glyph appeared on exactly the tab whose leaf id matched the `session` param, on the first
  call, with no prior state.
- `ctl notify session=pane-0 status=needs-input` (issued after the pane-1 call, same
  instance) → `04-06-notify-needs-input-pane0.png`: **Chat**'s tab icon also changes
  (small filled dot next to `Chat`), leaving both tabs marked. Confirms the mapping is
  per-pane-id, not "whichever tab is focused."

This directly exercises the control-socket → `AgentActivityModel` → tab-icon path
(`tab_status` reads `activity.status`) that the ledger's "sidebar half live" clause claims,
with a fresh, id-verified gesture rather than a replayed test.

**Did not drive:** the desktop-notification half (`post_activity_notification` →
`post_desktop_notification` → `notify-send`). Attempted it: `post_activity_notification`
(`main.rs:3738`) early-returns unless `self.activity.agent_id(&pane_id)` is `Some`, i.e. the
pane must already carry an agent identity (from Layer B title match or Layer D process
match) before a status transition will reach `notify-send`. Tried to establish that
identity live by typing `printf "\033]0;\xe2\x9c\xb3 working\007"` into the real Terminal
pane (Claude's idle title grammar, `title.rs:29`) via `wtype`, followed by `key Return`, to
get Layer B to register `claude` as the pane's agent. The typed text did not land as a
clean command — see `02-07-title-set-claude.png`: the printf text appears un-quoted and
wraps mid-string, so it never executed as a single command and no identity was registered.
Ran `dbus-monitor --session "interface='org.freedesktop.Notifications'"` in parallel for
both this attempt and a bare `notify session=pane-1` call with no identity — zero `Notify`
calls captured in either case (`/tmp/act02-dbus.log`, `/tmp/act02-dbus2.log`), consistent
with the code path (no identity → early return) but not a positive proof either way,
because the identity-establishing gesture itself did not land. The notification half stays
unproven, not disproven.

**Captures:** `02-01-initial.png`, `03-05-notify-needs-input-pane1.png`,
`04-06-notify-needs-input-pane0.png`, `02-07-title-set-claude.png` (failed title-injection,
kept as evidence of the blocker), `02-04-initial.png`, `03-08-chat-active.png`,
`04-09-after-notify-hidden.png`.

---

## F-CORE-ACT-06 (ledger line 342, NOT EXERCISED)

**Claim: could-not-reach.**

Owed gesture: identify one pane by title and a separate pane by process, replace each
pane's title with unrelated text, and observe that only the title-owned pane's status
clears.

This requires setting a real OSC title on a live GUI pane's shell (Layer B) via a typed
`printf` command. As recorded under ACT-02 above, the `type`/`key Return` sequence needed to
get that `printf` to execute cleanly did not land in this session — the typed escape
sequence wrapped/split instead of running as one command, so no pane ever acquired a
title-owned identity to test clearing against. The companion process-owned pane (Layer D)
additionally needs a real child process whose `comm` matches an `AgentCatalog` entry, which
this session did not attempt once the title half was already blocked. No live gesture was
completed for this row within the time budget; marking could-not-reach rather than
fabricating a partial reading from the failed injection.

**Captures:** none beyond the shared `02-07-title-set-claude.png` blocker evidence above.

---

## F-CORE-ACT-07 (ledger line 343, NOT EXERCISED)

**Claim: could-not-reach.**

Owed gesture: send a hook status via `notify`, immediately force a contradictory
recognized title (must not override within the debounce window), then repeat after >1.5s
(now it should override). The `notify` half is directly reachable (see ACT-02 above — it
reliably sets tab status via the control socket). The blocking half is the same one as
ACT-06: producing a real, recognized OSC title on the live pane via typed input did not
execute cleanly in this session (see `02-07-title-set-claude.png`), so the "contradictory
recognized title" side of the race could not be produced. Without that, the debounce
window cannot be exercised either way. Marking could-not-reach rather than asserting a
timing result with only half the race staged.

**Captures:** none beyond the shared blocker evidence.

---

## F-CORE-ACT-11 (ledger line 347, NOT EXERCISED)

**Claim: could-not-reach.**

Owed gesture: exercise title-, process-, and spawn-detection ownership on three separate
panes, alter/terminate one signal source at a time, close each pane, and confirm only the
matching ownership state is removed. This needs the same title-injection gesture as
ACT-06/07 (blocked, see above) plus a real spawned child process matching
`AgentCatalog` comm names for the process-owned pane — neither was reachable within budget
once the title gesture failed to land. The `notify`-driven half (spawn-owned, cleared by
watching process exit) is plausibly reachable the same way pane-0/pane-1 were driven above,
but a single ownership class proven alone does not exercise the row's actual claim, which is
about the three classes not clobbering each other. Not attempted further; could-not-reach.

**Captures:** none beyond the shared blocker evidence.

---

## F-CORE-TERM-02 (ledger line 395, half-proven)

**Claim: could-not-reach** (this lane only; the missing half needs a different lane).

Missing half per the ledger: the per-action *effect* of each of the twelve context-menu
items (Copy/Paste/Copy Cmd/Set Title/Copy Path/Copy Text/four Split variants/Clear
Terminal/Close Terminal) — the menu's existence is already proven live.

`WAYLAND-LANE.md` states plainly: "Right-click, button-held drag, modifier chords ...
still require `DISPLAY=:1`" — the Wayland lane's `wayland-drive.sh` exposes only `click`
(left-click), `move`, `type`, and `key`, with no right-click primitive. Opening the
context menu at all is the precondition for testing any of its items, and this lane cannot
open it. Checked for a keyboard-only path to the same menu (a Menu-key binding or
equivalent) — found none in `rust/crates/tiller/src/main.rs`'s keybinding wiring for the
terminal view. Per this task's explicit instruction, `linux-drive.sh`/`DISPLAY=:1` is out of
scope for this agent. Marking could-not-reach for this lane, not attempting the gesture
through a forbidden lane.

**Captures:** none new; the existing `reference/linux-progress/p17-rclick-term.png` (already
on record) still stands as the menu-opened evidence.
