# P100 — the notification that never arrives

**Owner: the next builder to free up.** Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

Three rows, two defects, one seam. **The desktop notifier works and is correctly written. Nothing
can reach it.**

| row | clause | now |
|---|---|---|
| `F-CTRL-NOTIFY-03` | `notification.create` requires title and body **and posts a user notification**; `list` returns rows; `clear` clears them | `FAILED — defective` |
| `F-CORE-ACT-19` | an agent transition posts a desktop notification | `NOT EXERCISED` (blocked here) |
| `F-CORE-ACT-20` | the visible / active / unchanged-status suppression gates | `NOT EXERCISED` (blocked here) |

`notify-send` is **no longer the blocker** — `libnotify-bin` was installed on this machine on
2026-08-14 and posts fine from a shell. Every earlier note that says "no desktop notifier available"
is stale. Do not re-diagnose that.

## Defect 1 — `create` reports success for a post it never makes

`notification.create` → `record_notification` pushes a `ControlNotification` onto an in-memory
`Vec` and returns `ok:true`. That is the whole implementation. The only caller of
`post_desktop_notification` is `post_activity_notification`, which is reachable **only** from an
activity `Transition` — never from the socket's create path. `notify --title/--body` funnels into
the same store-only function.

Proven twice, so you do not have to take it on faith:

- **By construction** — grep `post_desktop_notification`; it has exactly two mentions, its
  definition and one call site, and that call site is in the activity path.
- **Live** — with `notify-send` installed and working, a capture of
  `dbus-monitor --session "interface='org.freedesktop.Notifications',member='Notify'"` across two
  `notification.create` calls was **empty**. Both calls returned `ok:true`; `notification.list`
  returned both rows intact; **nothing reached the session bus.**

This is QUEUE mechanism **(g)**: *a channel that reports success for work it never does.* The four
other conjuncts of the clause are already proven live — validation of a missing title, validation of
a missing body, `list` round-tripping exact title/body, and `clear` emptying it. **Only the posting
conjunct is owed.**

## Defect 2 — after a restart, no agent pane can notify at all

This is the larger one, and it is why `F-CORE-ACT-19/20` cannot be exercised.

`post_activity_notification` opens with an early return:

```rust
let Some(agent_id) = self.activity.agent_id(&transition.pane_id) else {
    return;
};
```

`agent_id()` reads `pane_agents`, and `pane_agents` is written by **`agent_spawned` only** — called
from exactly two live gestures, `add_agent_tab` and `split_focused_agent`. Meanwhile
`register_agent_id` exists, is tested, and its own doc comment names precisely the case it was
written for:

> *Registers a restored pane's agent identity without claiming a status. Unlike `agent_spawned`,
> does NOT set `.running` — a restored chat tab may be idle, and the real status arrives later via
> `notify`.*

**It has zero production callers.** Only `activity_integration.rs:331` calls it.

`restore_tabs` and `restore_tabs_in_workspace` both read `tab.agent_id` out of `RestoredSession` and
use it — for the tab's icon and the tab model. Neither tells the activity model. The identity
survives the database round-trip (`session.rs` restores `agent_id` per tab, and its own test asserts
it) and is then dropped on the floor at the seam.

**The user-visible consequence:** quit Tiller with agent tabs open and relaunch. The tabs come back
with the right icons. From that moment, **no agent tab in that process ever produces a desktop
notification again** — not until you manually open a *new* agent tab. The tab looks like an agent
tab and behaves like a plain shell, which is exactly why four passes did not catch it.

QUEUE mechanism **(a)**: built, tested, never wired.

## The trap

**Two different fields are called `agent_id` and only one of them changes behaviour.**
`OpenTab.agent_id` selects the icon; `AgentActivityModel.pane_agents` gates notification,
attention-sorting and status glyphs. Restore populates the first. A grep for `agent_id` returns both
and reads as if the seam were connected. Scope your greps to the crate and check which map you are
looking at — this substring shape has cost this project a false verdict before.

## Order of work

1. **Wire restore.** `restore_tabs` / `restore_tabs_in_workspace` call `register_agent_id` for every
   restored tab that carries an `agent_id`. Say which of the two you changed — **both restore
   paths exist and a fix to one leaves the other dead.** If they share a helper, say so.
2. **Route `create` to the notifier.** Decide, and write down, whether `notification.create` should
   post unconditionally or go through `NotificationPolicy`. My reading: `create` is an explicit
   request to notify, not an observed transition, so the transition-shaped gate does not apply —
   `should_notify` drops `new == Running` and `old == Some(new)`, neither of which means anything
   for a caller-supplied title and body. **If you disagree, argue it in the commit message rather
   than silently picking the other one.**
3. **Do not make posting fatal.** `notify-send` is absent on plenty of Linux systems. A failed post
   must not fail the socket call or panic the app; the existing site already logs and continues —
   keep that shape.

## How to prove it, and what does not count

**A green test does not count.** Both defects are already covered by green tests — that is how they
survived. The proofs are live:

- **Defect 1** — run the `dbus-monitor` capture above while calling `notification.create`, and show
  a `Notify` member call on the wire with your title and body in it. The headless lane
  (`HEADLESS-LANE.md`) is enough; no display needed.
- **Defect 2** — the judgeable gesture is a **real process restart**: open an agent tab, quit,
  relaunch, drive a status transition on the restored pane, and see the notification. Note the
  suppression gate while you test — `should_notify` suppresses when `app_active && pane_visible`,
  and `app_active` is hardcoded `true` at the call site, so **the pane must be in a background tab
  or you will prove nothing and conclude the fix failed.**

Report what you saw for each. `F-CORE-ACT-20` specifically wants the gates *exercised*, not read —
a suppressed case and a delivered case, side by side.

## The rules

- **Inspiration, never code.** waku, Zed, orca and comet are to look at. Transplanted code is a gap.
- **Code plus a green test is `NOT EXERCISED`, never `PASSED`.**
- **Do not edit `INVENTORY-LEDGER.md`.** Report what you built and what you exercised; a critic sets
  verdicts, and it will not be you.
- Commit path-scoped, never `git add -A`. `main.rs` is shared — follow `ENVIRONMENT.md`
  §"A shared file is not a reason to leave work uncommitted".
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
