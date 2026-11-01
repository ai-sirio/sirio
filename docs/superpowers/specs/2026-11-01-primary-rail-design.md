# Collapse the Primary half to a rail, and peek from it

**Date:** 2026-11-01
**Status:** approved by Enzo, 2026-11-01
**Builds on:** `2026-10-15-secondary-on-demand-design.md` (the Secondary half
exists only while it holds tabs), `2026-09-22-center-panes-design.md` (the
centre split).
**Mockups:** `2026-11-01-primary-rail-mockups.html` beside this file. Enzo
chose variant **B**. The mockups are illustrative. Where they differ from this
document, the document wins: the peek is as wide as the expanded Primary half
(§1.6, §5), not the mockup's 290 px, and Esc does not close it (§1.8).

## §0 Intent

Enzo wants the editor bigger. More generally, the centre's Secondary half
(editor, Changes, browser) should be able to take most of the width. Today the
Primary half (chats and terminals) always keeps at least 320 px and the split
ratio. The only way to give the Secondary more room is to drag the divider,
and that also shrinks the conversation he is following.

So the Primary half gets a collapse button. Collapsed, it does not go away. It
becomes a narrow **rail** that shows one icon per open chat or terminal, each
with its status dot. A click on an icon opens that chat or terminal as a
floating **peek** over the Secondary. He can answer an agent and go back to
the editor without the editor ever shrinking.

## §1 Decided with Enzo

In the order he decided them:

1. **The button sits right of the `+`** in the Primary strip. Its icon reads
   as "a panel closing to the left".
2. **Collapsing does not close the half.** It becomes a strip with the icons
   of the open chats and terminals and their status dots.
3. **Variant B: a click on a rail icon peeks.** The chat or terminal appears
   in a panel floating over the Secondary. The editor keeps its size. A
   button reopens the half for real.
4. **A `+` at the bottom of the rail** opens the same New menu as the
   strip's `+`.
5. **Anti-goal, in his words: "se le chat smettono di girare quando il pane è
   chiuso".** Collapsing changes what is drawn and nothing else. Every chat
   and terminal keeps running, receiving output and updating its status, as
   if the half were open.
6. **The peek is as wide as the expanded Primary half** (§5). A terminal keeps
   its column count, so an agent TUI does not reflow on every peek, and a
   chat keeps the composer layout it has in the half.
7. **A tab at rest shows a grey dot** (§4), as in the mockup legend ("grigio =
   fermo").
8. **Esc does not close the peek** (§5). Enzo confirmed it after the
   collision with the chat's Cancel and the terminal's PTY was explained,
   and declined an Esc that would close an idle chat's peek only.

## §2 Goals, non-goals, anti-goals

**Goals**

- The Secondary half takes all the centre's width except the rail.
- The open chats and terminals, and which of them want attention, stay
  visible at a glance.
- An agent can be answered from the peek without expanding the half.
- The collapsed state survives a worktree switch and a restart.

**Non-goals.** Nobody asked for these, and they stay out:

- no new keyboard chord, and no command-palette entry;
- no drag and drop onto or off the rail;
- no close, rename or context menu on rail icons;
- no resizing of the peek;
- no rail for the Secondary half;
- no control-socket verb that collapses or peeks in release builds (the
  debug hooks of §8 are for the E2E).

**Anti-goals.** These would make the feature a failure even if it worked:

- a chat or terminal that stops, pauses, detaches or loses output because the
  half is collapsed;
- Esc in a peeked chat or terminal that no longer reaches the chat or the
  program (§5);
- a peek whose content shows the editor through it (§5, opacity).

## §3 Collapsed state and layout

**State.** There is one new per-worktree flag, `primary_pane_collapsed`,
and one setter for it on `SirioWorkspace` that persists it. The setter mirrors
`set_secondary_pane_hidden` (`main.rs:8124`).

**When the rail is drawn**

> drawn as a rail ⇔ `primary_pane_collapsed` **and** `secondary_pane_visible()`

**The flag never outlives the Secondary half.** Every path that makes
`secondary_pane_visible()` false while the flag is set also clears the flag
and persists the clear. The paths are: the last Secondary tab closes,
Ctrl+Shift+B or the strip's `×` hides the half, and a restore in which the
Secondary is not visible. The Primary half then comes back at its normal
width. Opening a file later shows both halves as usual; it does not
re-collapse the Primary by surprise. *(Coordinator's call.)*

**The collapse button**

- **Where.** It sits in the Primary strip, right of the `+`, at the same
  24 px size and hover style as the `+`. Element id `primary-collapse-button`,
  tooltip **Collapse Pane**.
- **When.** It is drawn only while `secondary_pane_visible()`. With nothing
  in the Secondary, collapsing would leave the centre empty.
- **Focus.** Collapsing moves focus to the Secondary's active tab if the
  Primary held it, mirroring what `set_secondary_pane_hidden` does in the
  other direction.

**Widths**

- **Rail.** While collapsed, the Primary half is the rail: a fixed
  `PRIMARY_RAIL_WIDTH` (40 px) wide, with no 320 px floor.
- **Secondary.** It takes the rest of the centre.
- **Divider.** The divider and its drag handle are not drawn.
- **Split ratio.** `appearance.centerSplitRatio` is not touched, so expanding
  returns to the split Enzo had.
- **Where the code changes.** `center_pane_widths` / `resolve_center_split`
  (`panel_layout.rs:63`) and `min_center_width` need the collapsed case.

**Expanding.** Two buttons expand: the rail's top button and the peek's pin
button. Expanding clears the flag and gives focus to the Primary's active tab.

## §4 The rail

The rail sits where the Primary half was, at the left of the centre. It has
an `ely.bg` background and an `ely.border` hairline on its right. Its element
id is `primary-rail`. Its contents, top to bottom:

1. **The expand cell.** It is `TAB_BAR_HEIGHT` tall, so it lines up with the
   Secondary strip, and carries the same hairline under it. Inside is a 24 px
   button, `primary-rail-expand`, with tooltip **Expand Pane**. Its icon is
   the collapse icon mirrored.
2. **One button per Primary tab**, in strip order.
   - **Size and id.** 30 px square, `primary-rail-tab-{tab id}`.
   - **Icon.** The same icon the strip draws for that tab (`tab_icon`, agent
     mark via `tab_agent_mark`).
   - **Status dot.** It is drawn like the sidebar's agent pill
     (`sidebar/row.rs:380`): a 6 px dot at the top-right corner, coloured by
     `status::status_color`. Unlike the pill, it is **always drawn**: Idle is
     grey (`status_color(Idle)`, `ely.fg_subtle`), and a tab for which
     `tab_status` returns no status counts as Idle. Enzo chose the grey dot
     (§1). The strip's bloom glyph stays in the strip.
   - **Status source.** It comes from `tab_status`, the same value the strip
     reads.
   - **Active tab.** The Primary's active tab gets the `ely.active`
     background.
   - **Tooltip.** The tab title, then `tab_status_name` (for example
     "refactor · Needs input").
3. **A `+` pinned to the bottom**, `primary-rail-new`. It opens the same New
   menu as the strip's `+`.

**Overflow.** If the tab buttons overflow the rail's height, they scroll
vertically and the `+` stays pinned.

**Where the rail is drawn from.** `SirioWorkspace` draws the rail, because
`tab_agent_mark` needs `self.activity`.

**Sharing the New menu.** `TabBar` keeps one popup and one anchor cell
(`tab_bar.rs:181-184`). The strip and the rail are never drawn in the same
frame, so the simplest route is to render the same `TabBar` entity in a
"rail" placement: only its `+` button, with the menu hung so that it fits
above the button (`layer` snaps into the window anyway). Two `+` buttons
writing one anchor would be the bug to avoid. The builder chooses the exact
shape.

## §5 The peek

**Opening and closing**

| Action | Result |
|---|---|
| click a rail icon, no peek open | that tab becomes the Primary's active tab and the peek opens on it |
| click a different rail icon | the peek switches to that tab |
| click the same rail icon | the peek closes |
| click anywhere outside the peek and the rail | the peek closes |
| rail `+` → New Terminal / New Chat | the new tab lands in the Primary as today, becomes active, and the peek opens on it |
| rail `+` → Changes / New Browser | opens in the Secondary as today; no peek |
| pin button in the peek header, or the rail's expand button | the half expands showing that tab; the peek is gone |
| worktree switch, or the peeked tab closes | the peek closes |
| any other route that makes a Primary tab active while the rail is drawn: sidebar pill, Ctrl+digit, tab cycling, palette New Terminal / New Chat, Ctrl+Shift+M into the Primary, `surface.tabs.*` over the socket | the peek opens on that tab |

**Other routes.** That last row is a single hook where the Primary's active
tab changes on a user's request, not a list of call sites. Today
`reveal_secondary_for_active_tab` (`main.rs:23181`) does the same job for the
Secondary, and #323 rejected "a tab that exists and is drawn nowhere".
Restoring a layout at boot or on a worktree switch sets the active tab
without peeking.

**On screen, for notifications and unread.** While the peek is open, the
peeked tab is on screen:

- `self.active_tab` is the peeked tab, so `post_activity_notification`
  (`main.rs:12928`) treats it as the tab being looked at.
- A collapsed tab with no peek open is off screen.
- `feat/unread-sessions` (design `2026-11-01-unread-sessions-design.md` §3)
  is in flight. Whichever of the two lands second extends the other: the
  unread rule's "on screen" adds "the peeked tab", and the rail icon carries
  the same unread mark the sidebar pill gets.

**Esc does not close the peek** (Enzo's decision, §1). In a chat, Esc is
bound to `Cancel` in the `ChatComposer` context (`chat/mod.rs:2911`) and
cancels the running turn.
In a terminal, Esc goes to the PTY, which is what vim, Claude Code and others
expect (`sirio_terminal/src/lib.rs:3939`). In the locked gpui, matched
bindings run before key listeners (`bezel-gpui-0.3.8/src/window.rs:5781`), so
any peek-level Esc binding would take Esc from both. The peek therefore
closes only by the clicks in the table.

**Geometry**

- **Anchor.** The peek is anchored to the rail's right edge. It runs from the
  top of the centre to its bottom, with a 6 px inset.
- **Width.** It is the width the Primary half would have if expanded:
  `resolve_center_split` with the current ratio and available width. The chat
  composer band therefore picks the same layout it has in the half
  (`composer_row.rs:30-100`).
- **Terminal size.** A peeked terminal resizes its PTY to the peek's bounds on
  first paint (`lib.rs:4166`). That is an ordinary SIGWINCH.

**Contents**

- **Header.** A 30 px row with the tab's icon, title and status dot, and the
  pin button `primary-peek-pin` (tooltip **Expand Pane**, same icon as the
  rail's expand button).
- **Body.** The tab's own view (`TerminalView` or the chat view) via
  `child_view`. The peek is the **only** place that view is drawn in that
  frame, because two paints would fight over the PTY size. The view is never
  recreated or remounted.

**Opacity.** The card uses an opaque foreground surface (`floating_surface` /
`overlay`), with `ely.border_strong` and a shadow. Under a peeked terminal
sits an opaque backing, because `TerminalView` paints with
`sirio.terminal_surface`, which translucency fades. The rule is the existing
one: translucency is never in the foreground
(`2026-10-25-bezel-ui-off-ely-design.md:172`).

**Focus**

- Opening the peek gives focus to the peeked view. Use the on-next-frame
  refocus pattern (`refocus_focused_pane`), because focus reads the last drawn
  frame.
- While the peek is open, the centre split's focused role is Primary with the
  peeked tab active, so tab-level shortcuts act on what the user is looking
  at.
- Closing the peek returns focus to the Secondary's active tab.

**Native browser.** `overlay_obscures_browsers` (`main.rs:18034`) must
include "peek open" and "rail New menu open". Otherwise a Browser tab in the
Secondary paints over them. Like every overlay today, this unmaps the whole
webview (`browser.rs:2917`), so a Browser tab is blank while a peek is open
and comes back when the peek closes. Clipping the webview to the strip beside
the peek is not part of this work.

**Building blocks.** The closest existing pieces:

- `layer(.., Hang::At(..))` for the floating panel;
- a `Popup` whose trigger is the rail icon (`note_trigger_press`, so the
  click on the icon that closes the peek does not reopen it);
- `leaving` / `reap_popup` for the exit;
- `on_mouse_down_out` for click-outside.

## §6 Why nothing stops running, and how it is proven

**Why.** Collapsing only changes drawing:

- The tabs stay in `self.tabs`. Parking happens only on a worktree switch
  (`park_current_worktree_tabs`), not when a tab is off screen.
- `TerminalPaneCache` keeps strong `Entity` refs, so no `TerminalView` is
  dropped. Its `Drop` would detach.
- Chat entities stay owned by their tabs.
- Activity keeps feeding `tab_status`.

A Secondary half hidden with Ctrl+Shift+B already works this way today: its
tabs are not drawn and keep running.

**What the builder must not add.** No code path may detach, park, pause or
throttle a tab because the Primary half is collapsed. The E2E in §8 proves
it.

## §7 Persistence and the docs it touches

**Persistence**

- **Schema.** A new column, `worktree.primary_pane_collapsed INTEGER NOT NULL
  DEFAULT 0`, in the next migration. On today's main that is `migrate_v25`
  (`migrations.rs:562-587`). `feat/unread-sessions` is in flight, so take
  whatever number is free at implementation time.
- **Code.** Mirror `secondary_pane_hidden` throughout: the record field
  (`model.rs:100`), the SQL in `db.rs`, `session.rs:2173/2195`, and the boot
  and per-selection loads (`main.rs:6668`, `12029`).

**Docs to amend.** "Always shown" stops being true:

- the `PaneRole::Primary` doc comment (`sirio_project/src/tab.rs`);
- `CONTEXT.md`, "Primary pane role";
- `docs/agents/architecture/panes.md`: one paragraph on the rail, the peek
  and §6;
- `docs/agents/e2e-tests.md`: the new E2E entry.

**Version.** This is a `feat:`, so the cycle rules in `CLAUDE.md` apply.
`0.38.1` moves to `0.39.0` unless an earlier `feat:` of the cycle has already
moved it.

## §8 Verification

**E2E.** The new script `Scripts/Tests/test-primary-rail-e2e.sh` is modelled
on `Scripts/Tests/test-open-list-e2e.sh`: private HOME, stub agents, polling
`surface layout read`, and PID-matched captures in `--out-dir`. It prints
`PRIMARY RAIL E2E OK`. Clicks are real X clicks at coordinates the app
reports, as `test-chat-session-panel-e2e.sh` does.

**Reads it needs**

- **Release read.** `surface.layout.read` gains `primaryCollapsed`,
  `peekTab`, and the rail's tabs in order with their status names.
- **Debug only.** The window-relative rects of `primary-collapse-button`,
  `primary-rail-expand`, `primary-rail-tab-*`, `primary-rail-new`,
  `primary-peek` and `primary-peek-pin`. Whatever shape these take, they go
  through `served::served_methods(debug)`.

**Cases**

1. With no Secondary tab, the collapse button is absent.
2. Open a file in the Secondary, then click collapse. `primaryCollapsed=1`,
   the Secondary is wider by the old Primary width minus 40, and the rail
   lists the Primary tabs in order.
3. **The anti-goal.** A stub agent in a Primary chat, and a stub process in a
   Primary terminal, produce output and change status while collapsed
   (Running, then Needs input or Done). The rail's status names follow. Then
   peek each one: `surface.chat.read` and `panel.read` show everything
   produced while collapsed, with no gap.
4. Peek a terminal. `stty size` reports the peek's column count. A key read
   (`read -rsn1`) receives Esc, and the peek is still open.
5. Peek a chat while a turn is running. Esc cancels the turn as it does in the
   half, and the peek stays open.
6. Click outside: the peek closes and focus is back in the Secondary. Click
   the same icon twice: it opens, then closes.
7. Rail `+`, New Terminal: a new Primary tab, with the peek open on it.
8. Pin: the half expands with that tab active, and `primaryCollapsed=0`.
9. Collapse, quit, relaunch: still collapsed. Then close the last Secondary
   tab: the half expands. Relaunch again: still expanded (the flag was
   cleared).
10. Click a Primary tab's pill in the sidebar while collapsed: the peek opens
    on it. Then trigger a status change on the peeked chat: no desktop
    notification is posted for it.
11. Captures, light and dark: the rail; a peeked chat; a peeked terminal; a
    peeked chat with a Browser tab active in the Secondary, where the browser
    is blank while the peek is open and comes back after it closes.

**Tests.** No unit tests are written after the code. If a unit is proven in
isolation (for example, the collapsed-width resolution), its failure modes
are written first.

## §9 Left to the builder

- **Icons.** The collapse and expand icon pair, copied byte-for-byte from
  Zed's catalog like the others in `rust/assets/icons/zed/`, with
  `ATTRIBUTION.md` updated. If Zed has no fitting pair, ask before drawing
  one.
- **Sizing.** Exact pixels within the sizes above, and the peek's entry
  animation (reuse `layer`'s fade).
- **Socket fields.** The shape of the debug rects and of the
  `surface.layout.read` fields.
- **`TabBar`.** How the rail placement of `TabBar` is factored (§4).

One rule applies to every builder: **do not design for rare or future cases
nobody asked for.**

## §10 Coordinator's calls (not decided by Enzo)

- The collapsed flag is cleared whenever the Secondary half disappears (§3).
- Peeking makes that tab the Primary's active tab (§5).
- Chats and terminals created from the rail `+` open straight into the peek.
- The collapse button appears only while the Secondary is visible.
- Every route that makes a Primary tab active while collapsed opens the peek
  on it (§5).
- A peeked tab counts as on screen for notifications and unread marks (§5).
- A Browser tab in the Secondary is blank while a peek is open, as under any
  overlay today (§5).
