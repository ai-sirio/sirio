# Wave G slice G3-gaps-b — report

All three rows re-verified at HEAD (`ec68700`). The critic's diagnosis holds unchanged for each:
no production caller exists, and — per P78's three-case framework — none is satisfied by an
ad-hoc route either. `cargo build -p tiller`, `cargo build -p tiller_terminal`, and
`cargo build -p tiller_project` are green with no changes made; nothing in this slice edits
production code.

## `F-CORE-WSP-04` — blocked

Confirmed: `LayoutCommand`/`classify_layout_command`
(`rust/crates/tiller_project/src/layout.rs:282`/`333`) have 14 references workspace-wide and 0
inside `crates/tiller/src/`. `LayoutNode`/`WorkspaceLayout`/`PaneGroup`/`WorkspaceTab` — the
whole typed layout-tree model this enum mutates — are equally unused by the app.

**Why this isn't a call-site fix.** The app's real split/tab state is a *different*,
independently-built data structure: `OpenTab.panes: PaneNode<TabContent>`
(`rust/crates/tiller/src/main.rs:606`) plus a flat `Vec<OpenTab>` for the tab strip, mutated by
ad-hoc methods (`split_terminal_at_with_placement`, `close_pane`, `select_tab`, …) that predate
`layout.rs` and duplicate its concerns: `PaneNode` vs `LayoutNode`, `OpenTab` vs `WorkspaceTab`,
direct field mutation vs `LayoutCommand` variants. Calling `classify_layout_command` from
`main.rs` on a `LayoutCommand` built just to satisfy the call would be the exact "second dead
control" P78 warns against — the real mutations still happen through the ad-hoc path;
`classify_layout_command`'s `LayoutTransition{structural, focus}` output would drive nothing.

**The real shape of the work** is case 2 from P78: replace the ad-hoc path with the tested one,
not call both. Concretely: `OpenTab.panes` would need to become (or be built from)
`WorkspaceLayout`; every direct mutation site in `main.rs` (`split_terminal_at_with_placement`,
tab close/move/rename, divider-drag handlers) would construct a `LayoutCommand` and apply it
through a to-be-written `WorkspaceLayout::apply(command) -> Result<(), LayoutError>`, using
`classify_layout_command`'s output to decide focus/redraw behavior. That is a full data-model
migration across `main.rs`'s ~600-line pane/tab surface, not a wire-up — it touches every split,
close, move, rename, and divider-drag call site in the file.

- **size**: XL — a data-model migration, not an integration pass.

## `F-CORE-WSP-08` — blocked

Confirmed: `WorkspaceTabViewState` (`rust/crates/tiller_project/src/layout.rs:114`) has 5
references, 0 in `crates/tiller/src/`. The row's pass bar is restart survival ("set state, quit,
relaunch, state is there" — P78), which this type cannot meet regardless of caller count: it is
never constructed by `SessionLayout`/`SessionTab` (`rust/crates/tiller/src/session.rs:288`), the
type that actually gets written to `tiller.sqlite` and read back at launch.

**What the app persists instead (case 2, not case 3).** `SessionTabState`
(`rust/crates/tiller/src/session.rs:79`) already round-trips through the database —
`root_id`, `pane_events`, and bounded `scrollback` — so the row isn't fully unaddressed: pane
*structure* and terminal *scrollback* do survive a restart today. But `WorkspaceTabViewState`'s
fields (`editor_caret`, `editor_selection`, `editor_scroll`, `editor_folds`, `chat_draft`,
`chat_attachments`, `chat_transcript`, `chat_follows_tail`) are a different, richer surface —
editor caret/fold/scroll state and an in-progress chat draft — and none of it is captured by
`SessionTabState` today. Confirmed by grep: `chat_draft`, `editor_caret`, `editor_folds` have no
reader/writer under `crates/tiller/src/` or `crates/tiller_ui/src/chat.rs`/`editor.rs`.

**The real shape of the work**: not "call `WorkspaceTabViewState`'s constructor" (case 1 is
false here — the type would still need to be threaded end-to-end), and not "delete the
duplicate" (case 2's usual resolution) since `SessionTabState` and `WorkspaceTabViewState`
persist genuinely disjoint state, not the same state twice. The honest fix is to extend
`SessionTabState` with the missing fields (or embed `WorkspaceTabViewState` inside it), then:
1. `tiller_ui/src/editor.rs`'s editor entity needs caret/selection/scroll/fold accessors (may
   already partially exist — needs its own audit) wired to write into
   `OpenTab.session_state` on change.
2. `tiller_ui/src/chat.rs`'s composer draft needs the same, plus `chat_transcript`/
   `chat_follows_tail` — `Chat` already exposes `transcript_text()` (per the WSP-04/DOM-07
   report above) so the transcript half is close, but the *draft* (unsent composer text) has no
   existing accessor.
3. `session.rs`'s encode/decode (`SessionTabState::encode`, its decode counterpart) needs the
   new fields added to the schema, and a migration path for rows written before this change.
4. `main.rs`'s restore path needs to feed the decoded state back into the editor/chat entities
   at tab reconstruction, mirroring how `root_id`/`pane_events` already rebuild `PaneNode`.

This is a real, multi-file feature (new persisted fields, two UI-entity accessor sets, a restore
path), not a caller for an existing tested function — the tested function
(`WorkspaceTabViewState` itself) isn't even necessarily the right container; the app may end up
extending `SessionTabState` directly instead of adopting this type verbatim, which is itself a
decision the next slice needs to make deliberately, not by default.

- **size**: L — new persisted state across two UI entities plus the session schema.

## `F-TAB-11` — blocked

Confirmed: `TerminalContextItem` (`rust/crates/tiller_terminal/src/context_menu.rs:30`) carries
only `label`/`action`/`route`; `ITEMS` is a flat `const [TerminalContextItem; 12]`; `items()`
takes no parameters. The real render site is
`TerminalView::render`'s context-menu block (`rust/crates/tiller_terminal/src/lib.rs:1450-1494`)
— every item is always clickable, none is ever shown disabled. `split_disabled_reason`
(`rust/crates/tiller/src/panes.rs:218`, `#[allow(dead_code)]`) is fully tested (sole-tab and
too-small-pane branches) but its only reference outside its own module is its own `#[cfg(test)]`.

**Investigated and deliberately not wired this round.** I traced the plumbing needed —
`TerminalView` (owned by this slice, `tiller_terminal`) has no field for pane geometry or
sibling-tab count today, so making Split Left/Right/Above/Down reflect real disabled state
needs a setter (e.g. `set_split_availability(sole_tab: bool, ...)`) called from `main.rs`
whenever `OpenTab.panes`'s leaf count or a pane's live bounds change, and `context_menu::items()`
changed from a `const` table to a function of that state. That part is buildable within this
slice's files. What stopped me: `split_disabled_reason`'s `tab_count == 1 →
SplitDisabledReason::SoleTabInGroup` branch doesn't have an unambiguous mapping onto the app's
actual split model. `OpenTab.panes: PaneNode<TabContent>` splits the *focused pane* unconditionally
regardless of current leaf count (`split_terminal_at_with_placement`, `main.rs:6079`, has no
leaf-count guard at all today) — so it's not established which runtime quantity `tab_count` is
supposed to observe: the number of panes already in this split tree, or the number of tabs in
the surrounding tab strip (a different, unrelated count). Wiring the wrong one produces
correct-looking grayed-out menu items that disable splitting under the wrong condition — the
"looks right, tests wrong thing" failure mode P78 warns about, and worse than leaving it dead
because it would be a plausible-looking false signal for the next critic pass. The
`PaneTooSmall` branch is unambiguous in meaning but needs live pixel bounds threaded from
`main.rs`'s layout pass into `TerminalView`, which doesn't happen anywhere today either.

**The real shape of the work**: (1) resolve with the original author's intent (or by product
decision) which quantity `SplitDisabledReason::SoleTabInGroup`'s `tab_count` parameter observes;
(2) add a `SplitAvailability`-shaped field + setter to `TerminalView`, called from `main.rs` on
every structural pane change and (for the too-small branch) on layout/resize; (3) change
`context_menu::items()` to take that state and return per-item `enabled`/`disabled_reason`;
(4) style disabled items in the render block (dimmed text, no `on_click`, no hover) and drop the
`#[allow(dead_code)]`s in `panes.rs` once `split_disabled_reason` has a real caller.

- **size**: M — the mechanism is clear, but step (1) is a semantics question this slice
  shouldn't guess at, and guessing wrong is worse than leaving the row open.
