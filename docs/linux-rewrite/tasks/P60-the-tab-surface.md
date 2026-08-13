# P60 — The tab surface: one missing menu is holding six rows

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you need.

## P57 landed, and the three judgement calls came back decided

You wired all six `surface.chat.*` methods and answered every question the brief handed over:
unopened sends fail explicitly, reads include live state, cancelled turns persist as stopped. Those
were the three I could not decide for you, and you decided them rather than picking the shape that
made the tests easiest. You also declined to claim PASSED and left the rows `builder-claimed,
unverified`, which is the rule.

Your note that the gate is blocked on `tiller_ui/src/sidebar.rs` formatting drift was correct and it
is not yours — another agent is live in that file.

## The piece: `F-TAB` is the largest untouched family, and it is not 19 problems

Nineteen `F-TAB` rows are non-passing. Nobody has been on them. Read the cluster structure before
you start, because most of the list is one absence repeated:

**Six rows are behind a single missing surface — there is no tab context menu.** `F-TAB-15` (the
clause's context-menu Close Tab route), `F-TAB-17` (close others / close to the right), `F-TAB-14`
(rename), `F-TAB-21` (no Tab menu at all), `F-TAB-12` and `F-TAB-13` (no move-tab UI or empty state).
A *pane* context menu was built in pass 12 and has a green drawn test; a *tab* one does not exist.
Build it, and six rows become reachable at once.

**Three rows are close semantics.** `F-TAB-16` — `close_tab` has no confirmation and no dirty check.
`F-TAB-28` — no ⌘W; ctrl-alt-w closes a pane and no-ops on a single tab. And `F-TAB-01`, which is the
interesting one: the strip renders icon, title and close, but **no dirty indicator anywhere** —
`tab_is_dirty` exists at `main.rs:4726` and feeds only the close confirmation. The state is already
computed and simply never drawn.

**Two are cheap completions of the pane menu you can take while you are in there.** `F-TAB-11` —
`split_disabled_reason` is unit-tested and has **zero UI callers**, so a disabled split never
explains itself; it is confirmed dead by an independent sweep (`Scripts/dead-models.py` lists it at
`tiller/src/panes.rs:207`, out 0, in 0, test 4). `F-TAB-23` — the pane menu carries only right/down
splits, no left/up.

**Leave the rest**: `F-TAB-18`/`24` (tab drag), `F-TAB-02` (All-Tabs overflow), `F-TAB-07`/`08` (the
ACP-agent submenu and its fallback), `F-TAB-09` (Open File — it needs a file-open surface that does
not exist), `F-TAB-25`, `F-TAB-27`. They are their own pieces. Say so rather than half-starting them.

## The judgement call, and it is yours

**What does "dirty" mean for each kind of tab?** A terminal with a live foreground process, a chat
with an unsent draft or a streaming turn, an editor with an unsaved buffer — these are three
different conditions and `tab_is_dirty` already commits to some answer. Read what it actually does,
decide whether it is right for all three kinds, and say what you chose. A close confirmation that
fires on the wrong condition is worse than none, because users learn to dismiss it.

Then decide what the indicator *is* — a dot, a changed close affordance, something else — and make
the strip and the confirmation read the **same** predicate. Two paths that can disagree about whether
a tab is dirty is how "it asked me to confirm but showed nothing" gets built.

## One constraint that is new tonight

The UI is being converted to the **Pop!_OS COSMIC** design language; `sonnet` is building the token
layer in `tiller_theme` right now. So: **take colours, spacing and radii from `tiller_theme::Theme`,
never hardcode them.** Anything you hardcode will have to be found and redone. If the token you need
does not exist yet, use the nearest existing one and name the gap in your report — do not invent a
literal.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. On this project's own ledger, rows judged by
reading ran ~21% false and rows judged by executing ~0%, so: **named drawn tests** using
`TestAppContext` / `VisualTestContext`, elements located with `.debug_selector(id)`, hardened with
the full `run_until_parked()` pump. The pass-12 pane context menu has a green drawn test — copy that
shape.

A menu that renders but whose items are no-ops closes nothing. Every action you add needs a test that
*invokes* it and asserts the effect, not one that asserts the item is present.

The display works (`Scripts/linux-shot.sh` → `1470x833 · 8401 colours`), so a screenshot is welcome
supporting evidence — but it does not replace a drawn test.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. Confirm
  with `git branch --show-current`.
- **Yours: `tiller/src/main.rs`, `tiller_control/**`, and `tiller_ui/src/tab_bar.rs`** — I have
  assigned that last file to you explicitly so nobody else takes it.
- **Do not edit** `tiller_ui/src/settings.rs`, `sidebar.rs`, `chat.rs` (`pi` is live in all three),
  `tiller_terminal/**` (`codex11`), or `tiller_theme/**` (`sonnet`).
- The gate is `Scripts/ci-linux.sh`; it now has an sccache fallback so it runs in your sandbox. It
  may be red for reasons that are not yours — **name those separately** rather than absorbing them,
  exactly as you did in P57.
- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- Mark rows `builder-claimed, unverified`, never `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: the tab context menu and which rows it reaches, your dirty definition per tab
kind and the single predicate both paths now share, whether ⌘W landed, the two pane-menu completions,
any token you needed that `tiller_theme` does not have yet, tests by name, the gate result with
not-yours failures named separately, and the honest remainder.
