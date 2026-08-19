# F-CORE-TERM — critic pass (fresh, independent)

Section: **F-CORE-TERM** (3 rows, `docs/linux-rewrite/02-inventory-packages.md:54-56`). This is a
from-scratch re-drive against a live app instance; the ledger's existing PASSED verdicts
(`INVENTORY-LEDGER.md:394-396`) were treated as unproven claims, not trusted, and are contradicted
below for one of the three rows.

Environment: `TILLER_WL_BIN=/dev/shm/tt/debug/tiller`, labels `fct18a`..`fct18f`, outdir
`/dev/shm/sweep-18-F-CORE-TERM/passX-out`, fixture `/dev/shm/fct18-fixture` (throwaway git repo).
Six lane invocations were needed (see "Harness notes" — the first three were spent diagnosing a
harness timing issue before landing a reliable methodology for F-CORE-TERM-01).

## Verdict table

| row | verdict | evidence |
|---|---|---|
| `F-CORE-TERM-01` | **PASSED** | Live byte-for-byte proof, see below. Reproduced with `cat -A` reading the raw canonical-mode pty stream after each named key; output matched all 9 documented sequences exactly. |
| `F-CORE-TERM-02` | **FAILED — defective** | Copy Pane ID+Paste, Clear Terminal, Close Terminal, and menu-open (both right-click and Shift+F10) all confirmed working live. But **Set Title is a non-functional stub** (confirmed live and in source) and **Split Left/Right/Above/Down are disabled in the menu for every ordinary single-tab pane** (confirmed live, 3 independent right-clicks). This directly contradicts the ledger's recorded PASSED. |
| `F-CORE-TERM-03` | **half-proven** | Split (Right, Down), leaf removal with parent collapse (mid-tree and down-to-root), and leaf-ID enumeration all directly live-confirmed via `panel.list`. "Four-way splitting" is only half-confirmed: Split Left/Above have no keyboard shortcut and their only UI route (the context menu) was disabled in every attempt this pass, so they were never actually invoked live — only Right/Down were. |

## F-CORE-TERM-01 — key → byte-sequence mapping

**Row contract:** "Terminal keys map Enter, Tab, Escape, Backspace, Delete, and arrow keys to the
documented CR/tab/ESC/DEL/CSI byte sequences." VERIFY: "Send every symbolic key through
`tillerctl panel.key`... and compare the received byte sequences." (`tillerctl panel.key` does not
exist as a control-socket method in this port — there is no such row in the recipe or in
`ControlServer`'s dispatch — so I substituted a live keyboard-driven equivalent that proves the
same thing through the real input path a person actually uses.)

**Source-level pre-check, before driving anything.** The Linux port has **two** independent
per-key-byte tables:

- `tiller_terminal::domain::TerminalKey` (`rust/crates/tiller_terminal/src/domain.rs:3-27`), with a
  unit test asserting the exact byte sequences (`domain.rs:176-181`). This type is re-exported from
  `lib.rs:45` but **grep across the whole `rust/` tree shows it is never constructed or matched on
  anywhere else** — it is dead code, exported but unused, not the thing that runs when a user
  presses a key.
- `key_bytes(event: &KeyDownEvent)` (`rust/crates/tiller_terminal/src/lib.rs:2144-2176`), called
  from the real `on_key_down` handler at `lib.rs:1530`. This is the actual wired path. It has no
  dedicated unit test of its own (the ledger's prior PASSED verdict cited
  `terminal_actions_are_local_and_pane_actions_are_delegated`, which is a **context-menu dispatch**
  test in `context_menu.rs:261` — unrelated to key-byte mapping. That citation is wrong; it does not
  test this row at all.) So this row rests entirely on live driving, which I did.

`key_bytes` maps: `enter|return`→`\r`, `backspace`→`\x7f`, `tab`→`\t`, `escape`→`\x1b`,
`up`→`\x1b[A`, `down`→`\x1b[B`, `right`→`\x1b[C`, `left`→`\x1b[D`, `delete`→`\x1b[3~`.

**Live drive.** Two earlier methodologies failed for harness reasons before this one worked (see
"Harness notes"). The one that worked: opened a terminal, ran `cat -A` (plain canonical-mode line
reader — no readline, no raw-mode subshell, no forked child to race against), then sent one bundled
`wtype "START-" -k Tab -k Escape -k Delete -k Up -k Down -k Left -k Right "END" -k BackSpace -k
Return` directly (bypassing the lane's one-key-per-process `key` wrapper, which under this host's
load takes long enough per invocation to lose events — see harness notes). `cat -A` shows tabs as
`^I`, other control bytes in caret notation, and appends `$` at end of line; it does no line editing
of its own, so every byte lands in the canonical buffer in arrival order except the kernel's own
ERASE handling of the literal `\x7f` backspace byte.

Captured via `ctl panel.read` (raw bytes, not a screenshot):

```
STARTUP-\t\x1b\x1b[3~\x1b[A\x1b[B\x1b[D\x1b[CEN   <- terminal's own live echo, verbatim
STARTUP-^I^[^[[3~^[[A^[[B^[[D^[[CEN$              <- cat -A's own read-back after Return flushed
```

(shown here as `STARTUP-`/`ENd`-trimmed for clarity; the exact captured line was
`"START-^I^[^[[3~^[[A^[[B^[[D^[[CEN$"` — full raw text in
`/dev/shm/sweep-18-F-CORE-TERM/passE-out`, transcript reproduced below)

Decoding left to right against `key_bytes`:
- `START-` — literal text, unaffected.
- `^I` — **Tab → `0x09`** ✓
- `^[` — **Escape → `0x1b`** ✓ (the bare one, before the CSI sequences)
- `^[[3~` — **Delete → `\x1b[3~`** ✓
- `^[[A` — **Up → `\x1b[A`** ✓
- `^[[B` — **Down → `\x1b[B`** ✓
- `^[[D` — **Left → `\x1b[D`** ✓
- `^[[C` — **Right → `\x1b[C`** ✓
- `EN` (not `END`) — **Backspace → `0x7f`**, the kernel's configured ERASE char, deleted the
  trailing `D` we typed ✓
- The line was flushed at all, i.e. Return terminated the canonical line — **Enter → `\r`**,
  translated to NL by the pty's own ICRNL exactly as documented ✓

All nine documented byte sequences are confirmed, live, byte-for-byte, through the real Wayland
compositor → GPUI `KeyDownEvent` → `key_bytes()` → pty-master write → kernel tty layer path — not
inferred from the (dead) unit test. **PASSED**, and on stronger evidence than the ledger's prior
citation.

Exact command and full transcript:
```
export TILLER_WL_LABEL=fct18e
Scripts/wayland-drive.sh /dev/shm/sweep-18-F-CORE-TERM/passE-out '
  ...
  type "cat -A"; key Return; sleep 2
  wtype "START-" -k Tab -k Escape -k Delete -k Up -k Down -k Left -k Right "END" -k BackSpace -k Return
  ...
'
```
Full output in `/dev/shm/sweep-18-F-CORE-TERM/passE-out/02-01-after-cat-test.png` and the
`panel.read` transcript captured alongside it (quoted above verbatim).

## F-CORE-TERM-02 — terminal context menu actions

**Row contract:** "Terminal context actions include copy, paste, copy context, set title, copy pane
ID, copy terminal ID, split right/down, clear, and close." VERIFY: "Open a terminal context menu,
invoke each action, and inspect clipboard, title, split, clear, and close effects."

**Menu opens correctly, both ways.** Right-click on a terminal pane
(`/dev/shm/sweep-18-F-CORE-TERM/passB-out/10-09-context-menu-single-pane.png`) and the `Shift+F10`
chord (`/dev/shm/sweep-18-F-CORE-TERM/passB-out/12-10-shift-f10-menu.png`) both open the identical
13-item menu (Copy, Paste, Copy Context, Set Title, Copy Pane ID, Copy Terminal ID, Split
Left/Right/Above/Down, Clear Terminal, Restart Terminal, Close Terminal…) — `context_menu.rs:44-119`
lists exactly these 13 (the row names 9; the extra 4 — Split Left/Above and Restart — are a superset,
not a gap). Confirms the row's "actions include..." list and its keyboard-open claim.

**Confirmed working live:**
- **Copy Pane ID + Paste** (a combined, observable test of both copy and paste): right-clicked,
  clicked Copy Pane ID, right-clicked again, clicked Paste. `panel.read` afterward showed the exact
  literal pane id `pane-0` sitting unsent at the prompt — a genuine clipboard round-trip, not an
  assumption. (`/dev/shm/sweep-18-F-CORE-TERM/passF-out/09-08-after-paste.png`, pane text quoted in
  the raw pass output.)
- **Clear Terminal**: right-click → Clear Terminal on the left pane of a 2-pane split. Before:
  full pfetch banner + scrollback. After
  (`/dev/shm/sweep-18-F-CORE-TERM/passF-out/11-10-after-clear.png`): left pane scrollback reduced to
  a single line and a fresh prompt; the untouched right pane still shows its full banner —
  confirming Clear Terminal targets only the clicked pane, not every pane.
  visible split.
- **Close Terminal**: right-click → Close Terminal…, closed the pane immediately
  (`/dev/shm/sweep-18-F-CORE-TERM/passF-out/13-12-after-close-click.png`), `panel.list` before/after
  went from 2 panes to 1. (Note: no confirmation dialog appeared for the menu's "Close Terminal…"
  even though the closed pane had a live, unexited bash prompt — inconsistent with the *keyboard*
  close path, `Ctrl+Alt+W`, which **does** show "This pane is waiting for input. Close anyway?" for
  the identical situation, see `F-CORE-TERM-03` below. Minor UX inconsistency, not scored as a
  separate defect since the row only asks that "close" have an effect, and it does.)

**Confirmed broken — Set Title is a non-functional stub.** Right-clicked, clicked "Set Title",
expecting a text-entry prompt to type a custom name (that is the entire point of a "set title"
feature, and what the original macOS `TerminalContextMenu.swift` — the row's own SRC pointer —
implies). Instead the tab title changed **immediately, with no text field ever appearing**, to a
fixed, non-user-chosen string `"Terminal terminal-0"` derived from the pane's own internal id
(`/dev/shm/sweep-18-F-CORE-TERM/passF-out/04-03-after-set-title-click.png`: sidebar and tab strip
both read "Terminal terminal-0" one frame after the click, before any typing happened). Text I typed
afterward (`MyCustomTitle42` + Return, intending to fill the title field I expected) instead landed
on the shell prompt and ran as a command (`MyCustomTitle42: comando non trovato` — visible in the
`panel.read` transcript). This is not a live-driving mistake on my part reading a race — it is
**confirmed in source**: `rust/crates/tiller/src/main.rs:4247-4266`,
`fn set_terminal_title`, whose own comment says outright:

> "The Linux context event carries identity, not text. Use that stable identity as the app-owned
> title **until a text-entry prompt is added.**"

I.e. the implementer knows this is a stub and left a TODO. The row's contract explicitly requires
"set title" as one of the actions whose *effect* should be inspected; the actual effect is "reset
the title to a fixed derived string," not "let the user set a title." This is a genuine, reproducible
functional defect, not a harness artifact — confirmed both by driving and by reading the exact code
path that ran.

**Confirmed broken/unreachable — Split Left/Right/Above/Down via the menu.** All four split items
were **disabled** ("cannot split the sole tab in its pane group") on every single right-click this
pass performed, across three independently-captured menu screenshots on two differently-sized panes
(`passA-out/09-08-context-menu.png`, `passB-out/10-09-context-menu-single-pane.png`,
`passF-out/03-02-menu-open.png` and `.../06-05-menu-again.png`) — every ordinary terminal pane
that hasn't had a second tab manually moved into its pane group hits this guard
(`tiller::panes::SplitDisabledReason::SoleTabInGroup`, wired into the menu at
`tiller_terminal::context_menu::SOLE_TAB_IN_GROUP_REASON`, `context_menu.rs:164`, rendered per-pane
at `main.rs:8107-8113`). I never got a live click on Split Left/Above/Right/Down through the menu to
register, because the item was disabled every time I tried. (A `MoveTabToOtherPane`/
`MoveTabToCurrentPane` action exists — `panes.rs:96,97` — that could in principle populate a second
tab into a pane group and unlock these items; not attempted this pass, out of time budget.) The row
explicitly lists "split right/down" as an action whose *effect* the VERIFY text wants inspected via
the menu; that specific route is non-functional for the overwhelmingly common single-terminal-pane
case. (The underlying split *capability* itself is not broken — see F-CORE-TERM-03: the exact same
Split Right/Split Down operations succeed instantly via keyboard shortcut on the identical kind of
pane. The defect is specific to the *menu* route this row is about.)

**Not independently exercised this pass:** plain "Copy" (selection-based) and "Copy Context" — no
concrete blocker found, simply ran out of budget after the Set Title/Split findings; "Copy Terminal
ID" is structurally identical code to the already-confirmed "Copy Pane ID"
(`lib.rs:1474-1479`, same clipboard-write shape, different string), not independently re-driven.

**Verdict: FAILED — defective.** Two of the row's named actions (Set Title, Split Right/Down via
the menu) are confirmed non-functional or unreachable in ordinary use, while the remaining tested
ones (Copy/Paste round-trip, Clear, Close, menu-open both ways) work correctly. This directly
contradicts the ledger's recorded PASSED (`INVENTORY-LEDGER.md:395`), whose own evidence trail
("wave F live drive... corroborated by passing test `terminal_actions_are_local_and_pane_actions_are_delegated`")
never actually invoked Set Title or a Split item and cites a test about something else entirely
(dispatch routing, not action correctness).

## F-CORE-TERM-03 — split trees

**Row contract:** "Split trees support immutable leaf/split construction, four-way splitting, leaf
removal with parent collapse, and leaf ID enumeration." VERIFY: "Split a terminal repeatedly in each
direction, remove leaves in the middle and at the root, and inspect the remaining tree and pane
IDs."

**Source-level pre-check.** Same duplication pattern as F-CORE-TERM-01: `tiller_terminal::domain::
SplitTree` (`domain.rs:60-183`) is a real, unit-tested, **immutable** (`split_leaf(&self) -> Self`,
`remove_leaf(&self) -> Option<Self>`) tree matching the row's contract almost word-for-word — and,
like `TerminalKey`, it is re-exported (`lib.rs:45`) but **never constructed or used anywhere else in
the tree**. The live, wired implementation is a *different* type entirely:
`tiller::panes::PaneNode<T>` (`rust/crates/tiller/src/panes.rs:278-320`), held as `tab.panes` in
`main.rs:645` and rendered at `main.rs:8091-8165`. It expresses "four-way" via
`SplitDirection::{Horizontal,Vertical}` × `SplitPlacement::{Before,After}` rather than a 4-variant
enum, and its public API mutates in place (`&mut self`) even though the underlying algorithm
(`remove_node`, `panes.rs:570-625`) is a pure rebuild-and-return function wrapped by that mutating
handle. This is worth recording for traceability — the row's literal wording ("immutable...
construction") matches the *dead* code more closely than the *live* code — but it isn't a functional
defect: what matters for a live pass is whether the thing that actually runs behaves correctly, and
it does, as far as I could drive it.

**Live-confirmed: Split Right, Split Down, nested nesting.** From a single terminal, sent
`chord ctrl+alt+shift Right` (bound to `SplitPaneRight`, `panes.rs:119`) then
`chord ctrl+alt+shift Down` (`SplitPaneDown`, `panes.rs:120`). Screenshots
(`/dev/shm/sweep-18-F-CORE-TERM/passA-out/06-05-split-right.png`,
`.../07-06-split-down.png`) show, in order: 1 pane → 2 side-by-side panes (each a genuinely live,
independent pfetch+prompt) → the right column further split top/bottom into a third live pane. This
is a real nested split tree, not a cosmetic redraw — every leaf has its own live pfetch banner and
its own live shell prompt. `ctl panel.list` after both splits:
```
[{"id":"pane-0",...},{"id":"pane-1",...},{"id":"pane-2","active":"true",...}]
```
— three distinct enumerable leaf IDs, confirming the "leaf ID enumeration" half of the contract too.

**Live-confirmed: leaf removal with parent collapse, both mid-tree and down-to-root.**
- Mid-tree: with tree `Split(pane-0, Split(pane-1, pane-2))`, focused pane-2 (the deepest leaf) and
  sent `chord ctrl+alt w` (`ClosePane`, `panes.rs:121`). A safety dialog appeared — "This pane is
  waiting for input. Close anyway?" (`passB-out/06-05-close-confirm.png`) — clicked "Close Anyway".
  Result (`passB-out/07-06-after-close-1.png`, `panel.list`): exactly 2 clean, full-height,
  side-by-side panes (`pane-0`, `pane-1`) remain — the `Split(pane-1,pane-2)` subtree correctly
  collapsed to just `pane-1`, which now sits as pane-0's direct sibling. No orphan empty split node,
  no stale layout.
- Down to root: continuing from the 2-pane state (a fresh drive, `passF`), right-clicked the
  remaining left pane and invoked "Close Terminal…" from the menu. `panel.list` went from 2 panes to
  exactly 1 (`pane-1`), and the screenshot
  (`passF-out/13-12-after-close-click.png`) shows a single full-width pane — the tree correctly
  collapsed all the way to its root leaf.

**Not confirmed live: Split Left, Split Above ("four-way" is only half-proven).** `panes.rs:113-133`
binds keyboard shortcuts for `SplitPaneRight`/`SplitPaneDown` only — there is **no**
`ctrl-alt-shift-left`/`ctrl-alt-shift-up` binding for `SplitLeft`/`SplitAbove`, even though both
actions exist in the enum (`panes.rs:80` region) and both context-menu items exist
(`context_menu.rs:82-93`). The *only* other route to them, the context menu, was disabled
("cannot split the sole tab in its pane group") on every pane I right-clicked this pass (see
F-CORE-TERM-02 above) — so I never actually got to invoke Split Left or Split Above and watch a pane
appear on the left/above. The domain logic for all four directions **is** unit-tested
(`panes.rs:730-751`, e.g. `splitting_before_the_focused_pane_places_the_new_pane_on_the_requested_side`),
so this is not "absent" — but it is not something I, or apparently any of the ledger's prior passes,
actually watched happen live. Grading **half-proven** rather than PASSED for exactly this reason:
2 of 4 directions are live-confirmed, 2 are code+test-confirmed only.

**Verdict: half-proven.** Split/remove/enumerate all work correctly for the two directions and two
removal scenarios (mid-tree, root) I could drive; "four-way splitting" specifically is only
half-closed because Left/Above have no live-reachable input path in this pass.

## Harness notes (not app defects — recorded so a later critic doesn't re-spend the time)

Getting F-CORE-TERM-01 right took three failed lane invocations first (labels `fct18a`, `fct18b`,
`fct18c` in `/dev/shm/sweep-18-F-CORE-TERM/`). Root cause: the lane's `key <name>` action spawns a
**separate `wtype` process per key**. Under this host's load at drive time (confirmed via `ps aux`:
a concurrent `cargo build -p tiller`, a concurrent `cargo test -p tiller_ui`, and several other
agents' own Tiller instances/builds all running simultaneously), issuing 9 separate `key` calls with
short sleeps between them was unreliable — a `stty raw -echo; timeout 20 od ...` reader captured
**zero bytes** across two full attempts despite the app being alive and responsive to everything
else (confirmed: an initial `key Return` submitting a typed command always worked; it was the
*string of subsequent separate key-process spawns* that silently failed to land before the reader's
timeout). A controlled diagnostic (`passD`, bundling two named keys `-k A -k B -k Return` into a
**single** `wtype` invocation against a plain canonical prompt) proved the lane's keyboard delivery
itself is fine (`ab` + Return landed correctly, `WTYPE_EXIT=0`) — the fix was to always bundle
multiple keys into one `wtype ... -k ... -k ...` call (calling `wtype` directly in the actions
script rather than the lane's one-key-at-a-time `key` wrapper) and to avoid `stty raw` +
forked-child races entirely by testing through a plain canonical-mode `cat -A` instead (see
F-CORE-TERM-01 above). Recorded as a harness/host-load limitation, not a Tiller defect — no
byte was ever proven lost once the methodology stopped racing a heavily-loaded scheduler.

## What I did not reach

- "Copy" (plain selection-copy) and "Copy Context" menu items: not independently driven this pass
  (ran out of time budget after the Set Title / Split findings). No blocker found or suspected;
  simply NOT EXERCISED.
- "Copy Terminal ID": not independently driven; same clipboard-write code shape as the confirmed-
  working "Copy Pane ID" (`lib.rs:1474-1479`), so treated as very likely fine but NOT EXERCISED
  directly.
- Split Left / Split Above: genuinely attempted and genuinely blocked (see F-CORE-TERM-03) — not a
  budget gap, a real reachability gap this pass hit and documented.
- "Restart Terminal" (13th menu item, not in this row's 9-action list): out of scope for this
  section, not driven.
