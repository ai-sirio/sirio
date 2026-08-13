# P20 — The pane command layer: splits and navigation that answer the keyboard

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you
need.

## First: your crash work is parked, not discarded, and you were right about `:1`

`Scripts/crash-supervise.py` now classifies runs as VALID TRIAL / NOT A TRIAL with repeated pixel
sampling and 8 regression tests. That is exactly what was asked, and it is the piece of this hunt
that will still be useful in a week.

You reported `:1` unavailable and were **correct** — it died at 09:56, minutes after I told you to
use it. Apologies for the wasted attempt; that was my error, not yours.

The hunt is parked for one concrete reason: **`:2` is now the only display on this machine that
presents, and XTEST input injection is global per display.** The critic is driving `:2` right now
for its live inventory pass — the pass that defines "done" for this project — and two agents
injecting clicks into the same display would corrupt both runs. A driven crash trial therefore
cannot run until that display frees up. Blocked work yields to unblocked work; the hunt resumes the
moment it can produce a valid result.

For the record, so nobody re-derives it: `:2` presents (8820 colours at 1440x833, verified 10:25);
every display created after ~09:55 comes back blank, including with software Vulkan
(`VK_DRIVER_FILES=…/lvp_icd.json`) and with `MESA_VK_WSI_DEBUG=sw`. `/dev/dri` has `card1` and no
`card0`, which is what a GPU device reset leaves behind — the compositor is likely holding a stale
render node, and the real fix is a compositor restart, which is the operator's call.

**This piece is deliberately display-free.** Prove it with tests.

## Where

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust
cargo test -p tiller
cargo build -p tiller -p tiller_control
```

**codex12 is in `tiller/src/main.rs` and `tiller_control/**`; pi is in `tiller_ui/**` and
`tiller_theme/**`.** Build failures there are their work in flight, not yours.

## The defect

The critic found **keyboard splits inert**, and no tab-cycling, tab-jump or pane-focus shortcuts at
all. The inventory section `## Tabs, panes, and navigation` in
`docs/linux-rewrite/01-inventory-app.md` covers this — F-TAB-10, 11, 19, 20, 21, 22, 23, 28 among
others. Each entry reads `- [ ] F-TAB-NN | capability | VERIFY: how to check it | SRC: the Swift
original`; **the VERIFY clause is the specification.**

**The model already exists and it is yours.** `rust/crates/tiller/src/panes.rs` has the whole split
tree: `SplitDirection`, `split_focused`, neighbour resolution for left/right/above/below including
the nested case, ratio adjustment, and node removal that promotes the surviving child. None of it
is reachable from the keyboard.

What is missing is the command layer. This codebase already has the pattern, twice — see
`tiller_ui/src/chat.rs` (`actions!`, `bind_keys`, `KeyBinding::new("enter", Send, Some("ChatComposer"))`)
and `tiller_ui/src/tab_bar.rs`. Follow the house style rather than inventing one.

**On chords: this is Linux.** The inventory names `⌘1`–`⌘9`, `⌘W`, `⌘⌥`+arrows because it was
written from the macOS original. Use the Linux equivalents (Ctrl, Ctrl-Shift, Alt) and say in your
reply which chords you chose. The capability is the contract; the modifier key is not.

## What to build, and where the boundary is

In `panes.rs` — yours, no coordination needed:

- The `actions!` types for: split right, split down, focus the pane left/right/above/below, close
  the focused pane, cycle tabs forward/backward, jump to the Nth tab.
- The key bindings for them.
- The **pure state transitions** for each, with unit tests: splitting the focused pane in each
  direction; focus movement including the nested-split case and the no-neighbour case; closing a
  pane promoting its sibling; tab cycling wrapping at both ends; jump-to-N where N exceeds the tab
  count selecting the last tab (F-TAB-20 says `⌘9` selects the last tab — read the entry).
- The **disabled reasons** F-TAB-11 wants: a split that is ineligible because the pane is too small,
  or is the sole tab in its group, must be able to explain itself rather than silently doing nothing.

Then the boundary. The workspace root renders the pane tree from `tiller/src/main.rs`, which is
**codex12's file and is being edited right now**. Do not touch it. Instead, end your reply with a
short, exact spec of the `on_action` wiring it needs: which action types, which handler on the
workspace, and what each must do.

**Write that spec the way you would want to receive it.** The last handoff of this kind was made
compiler-enforced — a new enum variant made the shell's match non-exhaustive, so the tree would not
build until an arm existed. It worked as designed and still went wrong: the arm was added, the build
went green, and the arm did one of the four things the handoff asked for. Nobody noticed until the
feature was exercised. **A green build is evidence about types, not behaviour.** So state the
behaviour each handler must produce, and state how someone would *check* it from outside — that
sentence is the part that actually transfers.

## Rules that apply to every piece of this project

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Every line of Tiller is written from scratch; transplanted code counts as a gap, always.
- **A capability nobody exercised does not exist.** Tests are what you can produce today; the live
  proof comes when a display frees up, and until then say plainly that it is untested live.
- You own `tiller_terminal/**`, `tiller/src/panes.rs`, and your `Scripts/crash-*` files.

## Reporting

Reply in **12 lines or fewer**: which actions exist, which chords you chose, the test count and what
they cover, the exact `on_action` spec for codex12, and the honest remainder — including that none
of it is live-proven yet.
