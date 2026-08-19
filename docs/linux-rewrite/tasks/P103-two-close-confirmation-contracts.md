# P103 — Closing a pane has TWO contracts in Swift, not one

Settled 2026-08-19 by reading the Swift reference directly, after the modal branch
(`worktree-wf_29906a5b-fa0-1`, F-TERM-05 / F-TERM-08 / F-TAB-26) conflicted with
`linux/gpui-waku` on 29 both-sides hunks in `main.rs` and the disagreement turned out to be about
behaviour rather than text.

## The two contracts

**Path A — the Activity row's close button: gated on status.**
`App/RightPanel/ActivitySectionView.swift:111-116`

```swift
private func requestClose(_ row: ActivityRow) {
    if row.status.requiresCloseConfirmation {
        pendingClose = PendingActivityClose(row: row)
    } else {
        close(row)
    }
}
```

with `ActivityStatus.requiresCloseConfirmation` (`TillerCore/ActivityStatus.swift:26`) being
`running`, `needsInput`, `error` → true; `done`, `idle` → false. Its own tests pin all five.

**Path B — the terminal context menu's "Close Terminal…" / "Close Tab…": unconditional.**
`App/SidebarView.swift:720` (and `:592`)

```swift
Button("Close Terminal…", role: .destructive) { confirmingClose = true }
```

No status is consulted anywhere on this path. The alert that follows
(`SidebarView.swift:601`, `:678`) says "Close terminal?" / "The running process will be
terminated." and is raised for **every** close, whatever the pane's activity state.

## Why this matters, and who was right

`linux/gpui-waku` currently routes the *terminal* close through `pane_close_needs_confirmation`,
i.e. it applies Path A's gate to Path B. That is a fidelity defect: an idle or finished terminal
closes silently here and asks first in the original.

The modal branch changes exactly that — "every interactive pane close is held here first" — **and
keeps the Activity-row path on `pane_close_needs_confirmation`**, which is the distinction that
makes it correct rather than merely different. Its conflicting hunks are the right side of this
disagreement.

I initially read `requiresCloseConfirmation` as *the* close contract and concluded the modal
branch contradicted the reference. It does not. One symbol governed one of the two paths, and
finding it first made the other path invisible — the tell was that `ActivitySectionView` is a
right-panel file while F-TERM-08's subject is the terminal context menu.

## How to merge it

Do not resolve those 29 hunks mechanically. A union produces code that compiles and a contract
that lies. Resolve by intent:

- **Take the modal branch's side** for everything about close-confirmation semantics, the Set
  Title prompt, and its tests (F-TERM-05, F-TERM-08, F-TAB-26).
- **Take `linux/gpui-waku`'s side** for everything the modal branch's older base predates:
  F-WIN-06 (`NewBrowser`/`FocusAddressBar`, and `linux_window_shortcuts` returning 8 entries, not
  6), F-TAB-12 (the dead "Move to This Pane" item stays deleted), F-CORE-DOM-01/-02 (the seeded
  worktree defaults), F-CORE-DOM-06 (`numeric_tab_selection`), and F-AGENT-OMP (`omp --print`, not
  `oh-my-pi --print`).

Both sides carry tests asserting their own behaviour, so the merged tree must also drop the
now-wrong assertions from our side — a test that says "a finished pane closes without a prompt"
is asserting the defect.

## The two paths already exist separately in our code — the fix is surgical

Located 2026-08-19, and it shrinks the job considerably. `main.rs` already has one function per
Swift path, and both currently call the same gate:

| our fn | Swift counterpart | today | must become |
|---|---|---|---|
| `request_close_terminal_at` (`main.rs:4823`, gate at `:4841`) | `SidebarView.swift:592`/`:720` | gated on `pane_close_needs_confirmation` | **unconditional** |
| `request_close_activity` (`main.rs:4866`, gate at `:4873`) | `ActivitySectionView.swift:111` | gated | **stays gated** |

So the F-TERM-08 half is not a 29-hunk reconciliation at all: it is removing the gate from
`request_close_terminal_at` and leaving `request_close_activity` untouched. The test
`drawn_activity_row_close_holds_a_running_tab_and_lets_an_idle_one_go` (`:13830`) covers the path
that keeps the gate and must keep passing unchanged — it is the regression guard proving the two
paths did not get collapsed into one.

What the modal branch still carries that this does not: the `modal.rs` primitive itself, F-TERM-05's
Set Title prompt, and F-TAB-26. Those are additive and are the reason to take the branch rather than
hand-write the one-line gate removal.

`pane_close_needs_confirmation` (`main.rs:2835`) keeps both its callers' semantics honest only if its
doc comment stops implying it governs every close; it governs the Activity row. Fix the comment in
the same change, or the next reader re-derives the wrong contract from it — which is how this
started.

## Do not merge the modal branch yet — a second critic refuted the first

A fresh refuter overturned the modal wave's own `PASSED` verdicts for **F-TERM-08** and
**F-TAB-26**, and it is right. The observation is not in dispute; the severity is.

`render_pane_close_confirm` (`main.rs:8370-8416`) builds the "Close terminal?" banner as a plain
absolutely-positioned div. It never calls `.track_focus()` or `window.focus()`, and
`handle_root_key_down` (`main.rs:10211-10244`) only special-cases `self.palette_open`. So the
dialog never takes keyboard focus and nothing routes keys to it.

**The consequence is worse than a missing affordance.** When the confirmation opens while the
terminal holds focus — the ordinary case for anyone who just pressed the documented `ctrl-alt-w`
chord — every subsequent keystroke, Enter included, goes straight into the live PTY underneath
(`TerminalView::on_key_down`/`key_bytes`, `tiller_terminal/src/lib.rs:1501-1534`, `:2144-2176`,
forward anything not matched to a bound action). The user believes a blocking confirmation is up
and is in fact typing into their shell.

The Swift original does not have this exposure: `TerminalContextMenuProvider.showCloseConfirmAlert`
(`App/TerminalContextMenuProvider.swift:58-69`) uses `NSAlert.runModal()`, a real OS-level
application-modal call that synchronously blocks all input until a button is clicked. A hand-rolled
GPUI overlay gets none of that for free, and this one did not add it.

**Why the first critic missed it, which is the transferable part.** It did check that a keypress
does not leak (`03-confirm-ignores-keypress-q.png`) — but captured that check from the
*mouse-driven* right-click path, which likely never put keyboard focus on the terminal at all. The
control passed because it exercised a state the bug cannot occur in. It also pressed the real
`ctrl-alt-w` chord and confirmed the same dialog appeared, but did not re-run the leak check from
*that* entry point. Drive the chord a person would actually press, and re-run the control from the
path where the failure is possible — not the one where it is structurally absent.

Note the same primitive's Set Title variant does claim focus properly (`PendingTitlePrompt::
needs_focus` + `focus.focus(window, cx)`, `main.rs:10926-10938`), so this is an isolated omission in
one variant, not a design choice — which is also why "build the primitive once, use it twice" did
not protect against it.

**Merge blocker.** F-TERM-05 stands (its own focus handling is real and its four keystroke-driven
tests pass). F-TERM-08 and F-TAB-26 do not, until the close-confirm variant claims focus and maps
Escape to cancel. Leave Enter unbound to the destructive action, or bind it only after checking the
Swift alert's own default-button convention.

## Resolved 2026-08-19 — merged as `c0834159`, and the 29-hunk problem never had to be solved

Two things closed here, one of them by discovering the plan above was built on a false premise.

**The focus gap is fixed, in two layers.** A real GPUI focus claim on `PendingPaneClose`
(`focus: FocusHandle` + `needs_focus`, following the Set Title prompt's already-correct pattern,
with `ModalFocus` added to the shared `modal.rs` so both variants use one mechanism), plus a
root-level swallow in `handle_root_key_down` mirroring the existing `palette_open` case. The second
layer is what actually guarantees no leak: it fires before the terminal's own key handler
regardless of what GPUI considers focused at that instant.

**Enter cancels, and the answer came from primary source rather than from caution.** This document
said to leave Enter unbound or bind it only after checking the Swift alert's convention. Checked:
`NSAlert` gives the Return key-equivalent to the **first-added** button, and every close-confirm
alert in the original adds Cancel before Close (`TerminalContextMenuProvider.swift:58`,
`SidebarView.swift:580`, `:627`). So on macOS Enter *and* Escape both cancel, and Close has no
keyboard shortcut at all. `showSetTitleAlert` (`:76`) adds OK first, which is why the two prompts
in this port intentionally end up with opposite Enter behaviour — each follows its own alert's
button order, not one blanket rule.

**The 29-hunk reconciliation this document spends a section on was never performed, because it was
not possible.** The modal branch's `rust/` tree predates the current one *structurally*: its
`main.rs` declares `mod panes;`/`mod session;`/`mod tab_machinery;`/`mod tray;`/
`mod command_palette;`/`mod display_backend;` as external files that exist nowhere in that branch,
and its `tiller_ui/src/` holds only `lib.rs` + `modal.rs` against our ~18 files. `git merge-base`
between the two resolves to a commit with **no `rust/` directory at all**, and an attempted merge
produced *add/add* conflicts rather than a 3-way diff. The per-intent resolution rule above assumes
a mergeable diff exists; there was none.

What was done instead: a fresh worktree off current `linux/gpui-waku`, with the three modal-branch
commits' unique content ported onto it by hand — the `modal.rs` primitive, F-TERM-05's Set Title
prompt, and F-TERM-08's close-confirm fix with focus on top. F-TAB-26 turned out to need nothing
ported; its fix was already on `linux/gpui-waku` independently, which was **verified against the
merged source** rather than accepted as a claim (`main.rs:4961`, the `whole_tab` guard). The
resulting merge conflicted on exactly one line — two module declarations at the same alphabetical
position in `tiller_ui/src/lib.rs` — which is the rare case where a union really is the correct
resolution.

**The surgical table above still describes the landed code**, and that was checked rather than
assumed: `request_close_terminal_at` (`main.rs:4950`) is unconditional, `request_close_activity`
(`:4987`) still calls `pane_close_needs_confirmation`. The two paths did not get collapsed.

**Ledger:** F-TERM-08 → PASSED. F-TERM-05 → half-proven, not because the behaviour is doubted but
because the critic that drove it live deleted its own screenshots and said so; the live half cannot
be replayed. F-TAB-26 → half-proven, because its own reproduction was the right-click context menu
and the drive that cleared F-TERM-08 used the `ctrl-alt-w` chord — and "same code path, therefore
same result" is exactly the reasoning that let F-TERM-08's first critic pass a control from an
entry point the bug could not occur in. That mistake is not being repeated one row over.
