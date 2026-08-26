# Threads view in the sidebar, and Activity removed — Design

Date: 2026-08-27
Branch: `main`
Map: #160 · Children: #162, #163, #164, #165, #170

## Goal

Two changes, one spec, because splitting them risks only the additive half ever
being built (map decision 20):

1. **Add a Threads view** to the left sidebar beside the Projects view, toggled
   from the sidebar header, listing every chat thread across every project.
2. **Remove the Activity view** from the right panel, whose list the sidebar's
   tab rows already render (`sidebar.rs:131` says so in a comment).

A **thread** is a chat tab with a persisted transcript — `chat_turn` rows.
A terminal pane running a CLI agent is not a thread: it has scrollback, not a
structured transcript (decision 1).

## Non-goals

- **Resuming the agent session** via `session_ref`. Opening a thread restores the
  rendered transcript; the agent still restarts with no context (decision 11).
- **Folding live terminal panes into the list.** That would reintroduce, under a
  new name, the duplication removing Activity exists to end.
- **Removing the in-chat history menu** (`chat.rs:2073`). It stays and coexists
  (decision 4). Two surfaces over one dataset is acknowledged debt, recorded here
  rather than silently accepted.

---

# Section 1 — The Threads view

## 1.1 Reconciling the two pieces of reference research

#162 was answered twice, and the two answers appear to conflict. They do not —
they describe **two different lists**.

- The **source-based** reading found a live per-project `ThreadList` *and* a
  separate `Archive`, and reported that **history rows carry no live-activity
  glyph**.
- The **observation-based** reading (Zed v1.17.2 driven on this repo) watched the
  **live** thread list and found its row glyph tracking status: the agent's mark
  when idle, a spinner while running, an amber ⚠ while waiting on the user.

Both are true of their own list. What we are building is the **live list**, not an
archive — so decision 7 survives, and with it the argument that removing Activity
is de-duplication rather than a loss of information. A thread that is running says
so, in the list, without the Activity panel.

**This is load-bearing.** If the Threads view did not carry status, removing
Activity would delete information rather than relocate it, and section 2 would be
a regression.

## 1.2 Row anatomy — settled by prototype (#164), revised by observation

Prototype: branch `proto/164-threads-view`,
`cargo run -p tiller_ui --example threads_view_proto`. Throwaway; not for main.

Two lines, at the sidebar's 280px:

```
[glyph]  Title
         project · branch          time
```

Five rules, each of which the prototype or the reference forced:

1. **Two lines** (decision 6), confirmed at 280px rather than assumed.
2. **The time is pinned right at a fixed width and never enters the truncating
   run.** Composed into one `project · branch · time` string it is what
   disappears first — the prototype rendered
   `tiller-experiments · fix/168-persist-tool-locatio…`, losing the age
   entirely. The age is the field a person scans to find "the one from this
   morning"; the branch is the one they can infer.
3. **The branch is middle-truncated**, keeping both ends
   (`fix/168…tool-locations`). The issue number and the subject are the two
   halves anyone reads.
4. **A degenerate title promotes the first-user-message snippet into the title
   slot** — one line, not a third row (decision 21). Giving it its own line makes
   the list scan as a ragged stack rather than a grid, and nobody needs to read
   the word "Claude Code" on a row whose glyph already says so.
5. **Status lives in the left glyph slot, replacing the agent mark; the right
   edge is reserved for the hover delete and nothing else.** This is the
   revision the reference forced (#164). It dissolves the collision rather than
   arbitrating it: the right edge is permanently free, and hovering can never
   evict the signal that says whether deleting is safe, because that signal is
   nowhere near the pointer.

   *Cost, accepted:* one slot shows one thing, so a running thread's agent
   identity is not visible while it is running. Tiller already makes this trade —
   F-CORE-ACT-17 tints the worktree running-indicator by agent brand rather than
   drawing both.

## 1.3 Buckets

`Today / Yesterday / Last 7 days / Last 30 days / Older` (decision 14), on
local-midnight boundaries. A bucket of one renders with no special casing — it
reads as a date separator, which is what it is (verified in the prototype).

The reference uses `This Week / Past Week` and no 30-day bucket. We keep ours:
its buckets follow ISO-week comparison, which answers "which week" rather than
"how long ago", and a coding session is looked for by age.

## 1.4 The query and its record type

New `Database::chat_sessions_all()` in `tiller_persistence/src/db.rs`, beside
`chat_sessions(worktree_id)` (`db.rs:587`), returning a **new** record:

```rust
pub struct ThreadSummary {
    pub tab_id: String,
    pub title: String,
    pub agent_id: Option<String>,
    pub turn_count: usize,
    pub last_activity: String,
    pub worktree_id: String,
    pub worktree_path: String,
    pub branch: String,
    pub project_name: String,
}
```

`ChatSessionSummary` is **not** widened with `Option` fields it would never use
(decision 17).

`ORDER BY chat_turn.updated_at DESC LIMIT 200`, no pagination, **with the cap
declared in a comment** (decision 15) — a silent cap reads as "all your threads"
when it is not.

## 1.5 The toggle, the header, and the `+`

- Two icons beside the `+`, header label switching `Projects` / `Threads`
  (decision 8). `FolderFill` for Projects, `Thread` for Threads — the icon freed
  by Activity's removal (decision 19), currently at `icons.rs:102`.
- Selected state must be a **filled/unfilled pair**, not a brightness difference.
  The prototype used brightness and it reads weakly.
- **The `+` changes meaning per mode** (decision 12): in Threads mode it creates a
  thread in the selected worktree, and is **disabled with a tooltip** when no
  worktree is selected.
- **The toggle has no side effects** (decision 18): selected worktree, mounted
  panes and live PTYs are untouched by a view switch.
- The existing filter field is reused to search threads **by title** (decision 9).

## 1.6 Mode persistence

A new `AppSettings` field (`model.rs:389`), e.g. `"sidebar.view"`, riding the KV
`setting` table — **no schema migration** (decision 9). `SidebarState`
(`model.rs:517`) holds only expanded projects and the selected worktree; putting
the mode there would need one.

## 1.7 Opening a thread — settled in #165

New `SidebarEvent::OpenThread { tab_id: String, worktree_path: PathBuf }`
(`sidebar.rs:428`), carrying **both**. The sidebar already knows both from the row
it drew; making the host re-derive the worktree from the tab id would put a
database round-trip on a click path. This mirrors `SelectWorktree(PathBuf)`,
which carries a path rather than an id.

Handled by `TillerWorkspace`, in this order:

1. **Select the worktree.** This already mounts it — `select_worktree` sets
   `mounted = true` (`main.rs:1120`), and only `close_worktree` clears it
   (`:1133`). It stays mounted afterwards.
2. **Look for a live tab** whose `OpenTab.persistence_id` (`main.rs:796` — "stable
   database identity, independent of the tab's visible position") equals
   `tab_id`. If found, **activate it and stop.** Never restore a second copy: two
   views of one conversation, one frozen, while the live agent keeps writing to
   the same `chat_turn` rows, is actively misleading mid-turn.
3. **Otherwise** create a chat tab in that worktree and restore the transcript
   into it.

`Chat::open_chat_history_session` (`chat.rs:2146`) is **not** reused and is not
changed. It rewrites the *current* pane's `persistence.tab_id` and swaps entries
in place — right for the in-chat menu, wrong here.

**A project that is collapsed or filtered out**: expand it, clear the filter if it
hides the target, then select. Selecting a destination the user cannot see is the
defect #109 just fixed.

**Failure** — transcript will not load, or the worktree cannot be mounted:
`show_toast` (`main.rs:6535`). Never `eprintln!`, which is what
`open_chat_history_session` does today (`chat.rs:2153`, `:2164`). A sidebar click
that silently does nothing is indistinguishable from a click that missed.

**A worktree whose directory vanished outside Tiller** still has rows (a *deleted*
worktree cascades its threads away — `chat_turn.tab_id → tab → worktree →
project`, every hop `ON DELETE CASCADE`, `migrations.rs:47,55,145`). Attempt it,
toast on failure, and leave the Threads view exactly as it was.

**The Threads view stays on screen after a click.** A thread list is worked
*through*; switching back to Projects charges a toggle per hop. The reference
keeps its threads sidebar open with the thread selected.

## 1.8 Delete — and the one case it must refuse (#170)

Two-click inline confirm, reusing `history_delete_confirm` in `chat.rs`, not a
modal (decision 13).

**If the thread's `tab_id` matches a live `OpenTab.persistence_id`, the delete is
refused** with a toast naming the open tab. Deleting the record of a conversation
that is still happening — while the agent writes to the same `chat_turn` rows —
is not a thing to confirm, it is a thing to decline. With no live tab, two-click
delete proceeds with no prompt.

This keeps one rule per surface: panes close through the close doors, transcripts
delete through the thread row, and neither borrows the other's confirmation.

## 1.9 Empty states

- No threads at all: `No threads yet` plus one muted line of guidance.
- No threads matching the filter: different words, same shape.

## 1.10 Where the code lives

**A new module, `tiller_ui/src/threads.rs`**, with `Sidebar` delegating to it
(decision 16) — `sidebar.rs` is already ~7,250 lines.

`tiller_ui/src/lib.rs` declares modules only and carries an ownership note: *do
not edit it from a piece worktree, the integrator owns it.* Adding `threads.rs` is
therefore **a coordination point**, not a free edit.

---

# Section 2 — Removing Activity

Blast radius as measured in #163.

## 2.1 What goes

- **Deleted**: `tiller_ui/src/right_panel/activity.rs` (241 lines).
- **Edited for behaviour**: `right_panel/mod.rs` (drop `PanelView::Activity`,
  `mod.rs:109`; `ActivitySurface`), `main.rs` — `activity_surfaces()` (`:5841`),
  `set_activity` (`:6583`), the two event handlers (`:4595-4596`),
  `select_worktree` (`:6152`, `:6154`), and the startup path inside `fn main()`
  (`:13970-13992`, `:14138`). **13988 is production code, not a test.**
- **Edited for stale comments**: `sidebar.rs`, `icons.rs`.
- **Rail becomes** Files / Diff / History. `PanelViewSetting` is a GPUI global,
  never persisted, so **no migration** — the rail simply restarts at Files.

Roughly 550–600 lines removed. Five tests die, one changes. No test indexes the
rail positionally (all click `element_id()` strings), so the reshuffle is safe.
`conformance.rs:58` documents "activity rows 48px" but never asserts it — the
comment goes stale, no test breaks.

## 2.2 What survives

`ActivityStatus` **stays**, keeping its home in `right_panel/mod.rs`, with four
call sites: the sidebar worktree dot and collapsed-project rollup, the tab-bar
glyph, the tray-jump ranking, and the pane-close confirmation text.

## 2.3 The close guard — resolved in #170, and it is not a regression

`pane_close_needs_confirmation` (`main.rs:3338`) has exactly one production caller,
`request_close_activity`, which made Activity's close button look like the only UI
path guarding a **background** tab against being closed mid-agent-work.

It is not, and it is the narrower mechanism. `tab_is_dirty` (`main.rs:10041`)
already treats live agent work as dirty — its own comment says *"one predicate
feeds both the strip indicator and every close door"* — and `request_close_tab_by_id`
(`:10062`), the tab bar's ×, gates on it **regardless of focus**:

```rust
TabContent::Terminal { view } => !view.read(cx).is_failed() && view.read(cx).exit_status().is_none(),
TabContent::Chat(chat)       => chat.read(cx).is_streaming(),
```

The case that would have overturned this — a chat **blocked on a permission
prompt**, waiting rather than streaming — is covered too:
`AcpEvent::PermissionRequest` (`chat.rs:1732`) never touches `streaming`, which is
cleared only at turn end (`:1841`), on error (`:1865`), on timeout, on chat reset
(`:2800`) and on connection loss (`:2884`).

**So delete `pane_close_needs_confirmation` with the panel, and record it as
redundant rather than as an accepted regression.**

**The condition on that** — the removal must land **with a test** asserting the
tab-bar close path prompts for an **unfocused** tab whose chat is streaming.
Nothing tests that today. Without it we trade an explicit guard for an implicit
one resting on a single line, and nothing notices when it breaks.

## 2.4 Docs that go stale

`docs/linux-rewrite/01-inventory-app.md:152-155` and
`INVENTORY-LEDGER.md:212-215,359,361` (F-CHG-19..22, F-CORE-ACT-21/23) all cite
the Activity panel as shipped, verified behaviour. They need updating in the same
change, not later.

---

# Tests — written first, per repo convention

**Section 2 (write these first; they gate the deletion):**

1. `tab_is_dirty` is true for a tab whose chat is streaming, and the tab-bar close
   path prompts for it **while the tab is not focused**. This is §2.3's condition
   and the reason the guard may go.
2. The rail renders Files / Diff / History and selects each by `element_id()`.

**Section 1:**

3. `chat_sessions_all()` returns threads across projects, newest first, capped at
   200, against a real temporary database (`tiller_persistence` integration tests
   build these already).
4. Bucketing: a thread at 23:59 yesterday lands in `Yesterday`, not `Today`.
5. Row composition, testable without drawing: time never truncates; the branch
   middle-truncates keeping both ends; a degenerate title yields the snippet in
   the title slot on one line.
6. A drawn test at **280px** that the row's rendered width stays inside the
   sidebar — the shape that caught #117 and #173.
7. Status occupies the glyph slot: a running thread's row shows the running glyph
   and its right edge is unchanged; hovering it still shows the running glyph.
   This is the one that would catch a regression back to the collision.
8. `OpenThread` on a thread whose live tab exists activates that tab and does
   **not** create a second one — assert the tab count is unchanged.
9. `OpenThread` on an unmountable worktree raises a toast and leaves the view
   unchanged.
10. Delete is refused while a live tab holds the same `persistence_id`.

## Verification gate

`Scripts/ci.sh` must print `CI OK`.

Anything that builds `tiller_terminal` needs **Zig exactly 0.15.2** on PATH — a
newer Zig fails too. On Windows, `Scripts/ci.sh` cannot pass: compare failing test
**names** against the machine's known baseline rather than counts.

## Raspberry Pi 5

The list is a single capped query (§1.4) run on view switch and on thread
mutation — not per frame, not on a timer. Status glyphs read the existing
`ActivityStatus` the sidebar already resolves; no new polling. Removing Activity
deletes a per-frame surface, so the net effect is fewer live elements, not more.
