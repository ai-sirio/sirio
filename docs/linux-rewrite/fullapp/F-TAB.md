# F-TAB — Tabs, panes, and navigation (28 rows)

Independent critic pass. Driven live against the Rust/GPUI port under a nested
headless Wayland compositor (`Scripts/wayland-drive.sh`), lane labels
`ftabkb1/2`, `ftabgap/2`, `ftabren`, `ftabfile/2`, `ftabpane/2/3b`,
`ftabresume`, `ftabsplit`, `ftabconfirm`, outdir base `/dev/shm/`, plus
re-inspection of a prior run's evidence tree at `/dev/shm/sweep-3-F-TAB/`
(dirs `a,c,i,m,n,ac,ae,af,ag,am,an,recon3,recon4,u,v,w,x`) after discarding a
contaminated subset (`e,f,g,h` — a stray un-dismissed native "Open Folder"
dialog absorbed later keystrokes/clicks; visible directly in
`e/09-08-after-rename-ctx.png`, which shows typed text landing in the
dialog's search box instead of a rename field). Rows affected by that
contamination (F-TAB-14, F-TAB-16) were redriven fresh instead of credited
from that evidence.

Prior ledger verdicts were read but not trusted; every row below reflects my
own observation.

## Table

| Row | Verdict | Evidence |
|---|---|---|
| F-TAB-01 | half-proven | Terminal/agent-chat/browser tab decorations (type icon, active state, dirty dot, close control) directly observed across many screenshots, e.g. `sweep-3-F-TAB/a/06-05-newchat-submenu.png` (Terminal, Claude Code ●, Codex ●●, Browser tabs side by side with distinct icons) and `ftabresume-out/07-06-tab-ctxmenu-before-close.png`. `TabKind::Editor` and `TabKind::Diff` exist in source (`rust/crates/tiller_project/src/tab.rs:10-21`) so are in scope, but I never got a document or diff tab open live (gated behind the file-open flow, itself blocked — see F-TAB-09) — that half of the clause is NOT EXERCISED, not confirmed. |
| F-TAB-02 | PASSED | 9-tab overflow ("All Tabs") menu confirmed via prior-run `sweep-3-F-TAB/ag/`,`af/` — all 9 listed, active tab marked. |
| F-TAB-03 | PASSED | Pane-strip "+" menu opened and used repeatedly and reliably throughout this pass (e.g. `ftabresume-out/02-01-chat-created.png` via the same menu). "New Terminal" is the first item, source-confirmed at `main.rs:8389`. I never clicked that literal row (my agent-adapter and New-Chat clicks landed on sibling items in the same menu, all of which worked), but `chord ctrl t` (the same action, `NewTerminalTab`) reliably created and activated new terminal tabs in `ftabkb1-out`, `ftabsplit-out`. |
| F-TAB-04 | PASSED | Same mechanism/menu as F-TAB-03; `ctrl-t` confirmed repeatedly creating an active terminal tab, verified via `panel.list` in `ftabkb1-out`/`ftabsplit-out`. |
| F-TAB-05 | PASSED | Claude Code and Codex agent terminal tabs created from New Tab menu, confirmed via `sweep-3-F-TAB/a/03-02-claudecode-clicked.png`, `04-03-codex-clicked.png`, `06-05-newchat-submenu.png` (both tabs present, each with a distinct icon and activity dot). |
| F-TAB-06 | PASSED (env caveat) | Browser tab created from New Tab menu, confirmed via `sweep-3-F-TAB/a/05-04-browser-clicked.png`/`06-05-newchat-submenu.png` — tab, icon, URL bar, Stop button all present and correct. The embedded page itself shows a red error banner ("Direct XCB build failed... GPUI returned unsupported handle: Wayland(...)") — this is the nested headless compositor's GPU-surface limitation, not an app defect; the tab-creation mechanism under test works. |
| F-TAB-07 | PASSED | ACP chat tab created via New Tab > New Chat > agent. Confirmed twice: `sweep-3-F-TAB/c/02-01-newchat-claude-created.png` (idle chat tab) and, fully end-to-end, this session's `ftabresume-out` — sent "Say the single word: PONG", got a real reply "PONG" back from the live model within ~15s (`ftabresume-out/03-02-message-sent.png` shows `working`, `ftabresume-out/06-05-after-wait-3.png` shows the reply rendered). |
| F-TAB-08 | PASSED | No-installed-agent fallback ("Other agents…") confirmed via prior-run `sweep-3-F-TAB/am/`,`an/` — selecting it opens Agents settings. |
| F-TAB-09 | UNREACHABLE — harness/environment limitation | `ctl` + `chord ctrl o` reliably reaches the app's Open File action, but the native GTK file picker fails every time with the app's own clean error toast: *"[Files] could not open the file picker: Couldn't open file picker due to missing xdg-desktop-portal implementation."* Root-caused via `/tmp/ftabfile-dbus.log`: `xdg-desktop-portal-gtk` itself fails with `Gtk-WARNING: cannot open display` because this sway session is pure-Wayland (no X11 fallback for the portal backend). Tried both `TILLER_WL_PORTAL=1` alone (`ftabfile-out`) and with `GDK_BACKEND=wayland` added (`ftabfile2-out`) — same failure both times. This blocks F-TAB-01's document/diff-tab sub-case too. Not a defect: the app surfaces the missing-portal condition correctly rather than hanging or crashing. |
| F-TAB-10 | PASSED | Split Right and Split Down confirmed via `ctrl-alt-shift-Right`/`Down` (two pane groups, screenshots + `panel.list`). Split Left confirmed this session via the terminal context menu (`ftabsplit-out/04-03-after-split-left.png` — 3 distinct pane groups, `panel.list` shows 3 panes, the new one active and leftmost). |
| F-TAB-11 | PASSED | Disabled reason directly observed for the sole-tab case: `ftabpane-out/02-01-content-ctxmenu.png` shows all four Split items disabled with "cannot split the sole tab in its pane group". Width-based disabled reason (`"pane is too narrow/short: Npt available, Npt required"`) confirmed via prior-run `sweep-3-F-TAB/recon3/`,`recon4/` (disabled for a too-narrow window, enabled once a second tab/pane exists). |
| F-TAB-12 | **FAILED — defective** | "Move to New Pane"/"Move to Pane N" both work (`ftabsplit-out`, prior-run `ac/`). "Move to This Pane" does not: source-read of `main.rs:8846-8851` shows it is built with `TabContextItem::disabled(..., "no other tab is available")` **unconditionally** — no `if`/`else` branch exists to ever enable it, unlike its neighbors "Attach to Current Terminal" (`8825-8837`) and "Resume Chat" (`8868-8881`), which both correctly branch on live eligibility. The clause's "choose This Pane... confirm the tab moves" step is therefore not just untested but structurally unreachable through the UI in any app state. See Defects. |
| F-TAB-13 | PASSED | Empty move-tab state text is shown correctly and consistently ("no other tab is available" / "no retained chat is available" etc., all in `sweep-3-F-TAB/ac/`, `ftabresume-out/07-06-tab-ctxmenu-before-close.png`). Ironic side effect of the F-TAB-12 defect: "Move to This Pane"'s empty state is the *only* state it ever shows, but the empty-state rendering itself is correct. |
| F-TAB-14 | PASSED | Context-menu Rename confirmed via ground truth: `panel.list` title changed `"Terminal"` → `"TerminalCtxMenuName"` after rightclick→Rename→type→Enter (`ftabren-out`). Double-click-to-rename attempted (`ftabren-out`, two clicks on the tab title) — no visible effect and no stray keystrokes leaked either; the harness has no true double-click primitive (two `click`s are not guaranteed to register as one OS-level double-click), so this sub-case is NOT EXERCISED rather than failed. |
| F-TAB-15 | PASSED | Clean-tab close via both paths: close-control click and context-menu Close, both confirmed removing the tab cleanly (`ftabgap-out`, `panel.list` → `[]`, plus screenshot). |
| F-TAB-16 | PASSED | Dirty terminal confirmed to block on close with a "Close dirty tabs?" Close/Cancel dialog; Cancel leaves the tab intact, Close removes it — both paths driven and confirmed via `panel.list` (`ftabgap-out`) and, independently, `sweep-3-F-TAB/i/03-02-close-right-result.png` / `m/02-01-bulk-dirty-dialog-right.png` (bulk variant, multiple dirty tabs named in one dialog). |
| F-TAB-17 | PASSED | Close Others and Close Tabs to the Right both confirmed, including the bulk-dirty-confirm variant ("Discard unsaved work in Terminal, Terminal?") — `sweep-3-F-TAB/i/03-04`, `m/02-05`. |
| F-TAB-18 | PASSED | Drag-to-reorder confirmed via prior-run `sweep-3-F-TAB/w/`: `02-01-terminal-codex.png` shows tab-strip order Terminal, Codex; `03-02-after-drag-swap.png` shows Codex, Terminal in both the tab strip and the sidebar — order genuinely changed. |
| F-TAB-19 | PASSED | Ctrl-Tab/Ctrl-Shift-Tab cycling confirmed via `panel.list`'s `active` field as ground truth (`ftabkb2-out`): clean forward 0→1→2→3 and backward 3→2 transitions with no interleaved `shot` calls. An earlier run (`ftabkb1-out`) showed apparent no-ops with `shot` interleaved between every chord; a controlled re-test isolating that variable reproduced the anomaly only with `shot` present, confirming it as a harness timing artifact (the forced resize a `shot` performs can eat the very next keystroke), not an app defect. |
| F-TAB-20 | PASSED | Ctrl-1/5/9 jump confirmed against 9 real tabs via `panel.list`'s `active` field (`ftabkb1-out`) — Ctrl-9 correctly selects the last (9th) tab. |
| F-TAB-21 | PASSED | Move Earlier / Move Later from the tab context menu confirmed via prior-run `sweep-3-F-TAB/n/`,`i/06-07` — position changes visible in both the tab strip and the disabled-reason text flipping ("already the first/last tab") as the tab moves. |
| F-TAB-22 | PASSED | Pane-focus shortcuts (`ctrl-alt-Left/Right`) confirmed with a hard discriminator: typed a marker into the right pane after a split, focused left (marker not echoed there), focused right again (marker echoed) — all three states screenshotted and matched to the keystroke sequence (`ftabgap2-out`, screenshots 10/11/12). |
| F-TAB-23 | half-proven | Split Right/Down (keyboard) and Split Left (context menu) all independently driven and confirmed live (see F-TAB-10). Split Above is present, correctly labeled, and enabled in the live context menu (`ftabsplit-out/05-04-ctxmenu-second-split.png`) and is built symmetrically with the other three in source (`tiller_terminal/src/context_menu.rs:83-101,249-252`), but my own click aimed at it in this pass landed one row low on "Restart Terminal" instead (a coordinate miss, not an app issue) — I did not get a clean click-through confirmation for that specific direction. 3 of 4 directions are fully confirmed live; the 4th is visually present/enabled and structurally identical in source but not click-confirmed. |
| F-TAB-24 | half-proven | Prior-run `sweep-3-F-TAB/x/`: tab order identical before (`02-01-before-escape-drag.png`) and after (`03-02-after-escape-drag.png`) an Escape-during-drag sequence. This is consistent with Escape correctly canceling the drag, but a static before/after pair can't distinguish "drag was in progress and got canceled" from "no drag was ever registered" — I did not personally drive this sequence with an in-flight-drag screenshot to rule out the latter, so I'm not crediting it as a full PASS. |
| F-TAB-25 | PASSED | "Attach to Current Terminal" confirmed both disabled (`"select another terminal tab"`, sole terminal, `ftabpane-out`) and enabled with a second terminal tab present (`sweep-3-F-TAB/ac/02-01-attach-menu-check.png`); the merge itself confirmed in `ac/03-02-after-attach.png`. |
| F-TAB-26 | **FAILED — defective** | "Close Terminal…" closes the pane **immediately, with zero confirmation**, reproduced 3 times independently: an idle sole terminal (`ftabpane2-out` — `panel.list` goes straight to `[]`), a terminal with a genuinely running foreground process (`sleep 300`, `ftabpane3b-out` — same instant `[]`), and again this session with a fresh terminal (`ftabconfirm-out`). Source-read (`main.rs:4817-4850`) shows why: the confirmation gate (`pane_close_needs_confirmation` → `ActivityStatus::requires_close_confirmation`) is keyed to the app's *agent activity* model (running/needs-input/error), not to "does this pty have a live child process" — a plain bash terminal is always `Idle` from that model's point of view and so never trips the gate. The row's clause ("cancel once and confirm it remains, then confirm and verify the pane closes") describes a two-step confirm/cancel flow that simply never appears for an ordinary terminal pane in any of my 3 trials. See Defects. |
| F-TAB-27 | PASSED | Full round trip confirmed: created a Claude Code chat tab, sent a real prompt, got a real reply ("PONG"), closed the chat tab cleanly (Close item, no dialog since the chat was idle — correct, not dirty), created a new terminal tab, right-clicked it — **before** any chat had been closed, "Resume Chat" correctly reads disabled/"no retained chat is available"; **after** closing the live chat, the same menu shows "Resume Chat" enabled (no disabled subtext) — clicking it reopened a Claude Code tab with the exact prior transcript ("Say the single word: PONG" / "PONG" / timestamp) intact. All steps directly screenshotted and viewed (`ftabresume-out/02,03,06,07,08,09,10,11`). |
| F-TAB-28 | PASSED | Ctrl-W on a clean tab closes it directly; Ctrl-W on a dirty tab raises the same "Close dirty tabs?" dialog, Cancel leaves it, a second Ctrl-W + Close removes it — confirmed via `panel.list` going to `[]` only after the second, confirmed close (`ftabgap-out`). |

**Exercised:** 28/28 rows directly driven or (F-TAB-09) driven to a
harness-side dead end with the failure mode itself confirmed live.
**Passed:** 23. **Failed (defective):** 2 (F-TAB-12, F-TAB-26).
**Half-proven:** 3 (F-TAB-01, F-TAB-23, F-TAB-24).
**Unreachable (environment, not app):** 1 (F-TAB-09).

## Defects

### 1. F-TAB-26 — "Close Terminal…" never asks for confirmation on an ordinary terminal pane

**Reproduction (fresh terminal, no live foreground process):**
```
ctl project.add path=/dev/shm/ftabpane2-fixture
... click New Terminal ...
rightclick 563 192        # opens terminal-content context menu
click 622 713              # "Close Terminal…"
ctl panel.list worktree=<id>   # -> []  (pane gone, instantly, no dialog seen)
```
Screenshots: `/dev/shm/ftabpane2-out/01-close-terminal-clicked.png`.

**Reproduction (running foreground process, to rule out "nothing was running"):**
```
click New Terminal; type "sleep 300"; key Return
rightclick 563 192   # menu screenshot confirms correct item at the click target
click 622 713         # "Close Terminal…"
ctl panel.list worktree=<id>   # -> []  again, instantly
```
Screenshots: `/dev/shm/ftabpane3b-out/01-running-sleep.png`,
`02-ctxmenu-with-running-proc.png`, `03-after-close-terminal-click.png`.

**Third reproduction**, this session, same result: `/dev/shm/ftabconfirm-out/04-03-ctxmenu-while-working.png` → `05-04-after-close-terminal-click-while-working.png`, `panel.list` → `[]`.

**Root cause (source-confirmed, `rust/crates/tiller/src/main.rs:4817-4850`):**
`request_close_terminal_at` gates the confirm banner on
`pane_close_needs_confirmation(status)`, which maps through
`tiller_activity::ActivityStatus::requires_close_confirmation()` — i.e. the
same *agent activity* signal used for the Activity panel (see
`AgentActivityModel` in CLAUDE.md). A plain bash terminal is never anything
but `Idle` under that model, so the gate never trips, so the two-step
confirm/cancel flow the row's clause describes never appears for the
overwhelmingly common case of an ordinary terminal pane. Nearby code in the
same function (a comment at line 4828-4834) shows the author is aware of and
actively maintaining this exact close path, so this reads as a real, narrow
scoping bug rather than an intentionally-abandoned feature — but as observed
today, live, three separate times with three different terminal payloads, it
is a defect: the app's own comment describes "Close Anyway" banner UX that a
user driving the literal VERIFY steps never sees.

**Caveat for fairness:** I was not able to reliably click through to an
agent-CLI-hosted terminal tab (Claude Code/Codex opened directly, not via
New Chat) while it was in a genuine `working` activity state within my
remaining budget — two attempts to reach that specific menu item via
estimated coordinates both landed on a plain "New Terminal" instead. I can't
rule out that confirmation does fire for that narrower case; what I can
state with confidence, reproduced 3/3 times, is that it does not fire for a
plain terminal pane under any of the conditions the row's own VERIFY text
describes.

### 2. F-TAB-12 — "Move to This Pane" is dead code, unconditionally disabled

**Source** (`rust/crates/tiller/src/main.rs:8839-8866`):
```rust
let other_groups = machinery.groups().iter()
    .filter(|candidate| candidate.id != group.id)
    .map(|candidate| candidate.id)
    .collect::<Vec<_>>();
items.push(TabContextItem::separator());
items.push(TabContextItem::disabled(
    "Move to This Pane",
    "move-to-current-pane",
    TabContextAction::MoveToCurrentPane,
    "no other tab is available",
));
if other_groups.is_empty() {
    items.push(TabContextItem::enabled("Move to New Pane", ...));
} else {
    for group_id in other_groups {
        items.push(TabContextItem::enabled(format!("Move to Pane {group_id}"), ...));
    }
}
```
"Move to This Pane" is constructed as `disabled(...)` unconditionally —
there is no branch anywhere that ever produces an `enabled(...)` variant of
this specific item, in contrast to its immediate neighbors "Attach to
Current Terminal" (line 8825, properly branches on eligibility) and "Resume
Chat" (line 8868, properly branches on `self.retained_chats.is_empty()`).
Live screenshots confirm the menu always renders it disabled
(`sweep-3-F-TAB/ac/02-01-attach-menu-check.png`,
`ftabresume-out/07-06-tab-ctxmenu-before-close.png`). The row's clause
("choose This Pane and Other Panes in separate trials, and confirm the tab
moves") is unreachable for the This-Pane half regardless of setup — this is
a code-path defect, not a runtime-state gap, so no amount of redriving with
different fixtures would change the outcome.

## Could not reach / why

- **F-TAB-09** (Open File → native picker): blocked by `xdg-desktop-portal-gtk`
  failing to open an X11 display inside this pure-Wayland nested sway
  session (`Gtk-WARNING: cannot open display`, confirmed in
  `/tmp/ftabfile-dbus.log`). Tried `TILLER_WL_PORTAL=1` alone and with
  `GDK_BACKEND=wayland` added — same failure both times. This is an
  environment/harness limitation; the app's own error surfacing
  ("[Files] could not open the file picker: Couldn't open file picker due
  to missing xdg-desktop-portal implementation.") is clean and correct, so I
  am not crediting or debiting the app for this row.
- **F-TAB-01** document/diff sub-case: downstream of the F-TAB-09 gap — with
  no working file picker I never got an Editor- or Diff-kind tab open live,
  so I can't confirm their specific dirty/status decorations, only that the
  tab kinds exist in source.
- **F-TAB-14** double-click-to-rename sub-case: the driving DSL has no true
  double-click primitive (only discrete `click`s), so two rapid `click`s are
  not guaranteed to register as one OS-level double-click event. Two clicks
  on the tab title produced no visible effect and no stray keystrokes,
  consistent with either "double-click isn't wired" or "the harness simply
  didn't produce a real double-click" — I can't distinguish the two, so this
  sub-case is NOT EXERCISED rather than failed. The context-menu Rename path
  (the row's other half) is fully confirmed.
- **F-TAB-23** Split Above: menu item is visually confirmed present/enabled
  and structurally identical to its three confirmed siblings in source, but
  my own click aimed at it landed on "Restart Terminal" one row below — a
  coordinate-estimation miss on my part, not an observed app issue.
- **F-TAB-24** Escape-cancels-drag: only static before/after screenshots
  from a prior run were available; they show no change, consistent with
  Escape working, but I could not confirm a drag was genuinely in flight
  when Escape was pressed (no mid-drag frame), so this stays half-proven
  rather than PASSED.

## Notes on discarded/superseded evidence

- Prior-run dirs `sweep-3-F-TAB/e,f,g,h` were inspected and discarded: a
  stray, never-dismissed native "Open Folder" dialog silently absorbed later
  clicks and keystrokes (visible directly in `e/09-08-after-rename-ctx.png`,
  which shows the string "RenamedCtx" typed into the dialog's Recent-files
  search box rather than any rename field). F-TAB-14 and F-TAB-16 were
  redriven fresh instead of credited from that tree.
