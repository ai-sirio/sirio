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
