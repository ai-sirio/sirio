# F-CORE-WSP layout model — decision, not just a re-score

Lane label `wf-wsp`. x86 desktop, COSMIC/Pop!_OS, 2026-08-18. Binary pinned:
`cargo build --manifest-path rust/Cargo.toml` → exit 0, only the two pre-existing dead-code
warnings (`browser.rs:827 pump_task`, `main.rs:9864 sidebar_projects`) — matches
`ENVIRONMENT.md`'s baseline. `md5sum` verified identical between `/tmp/wfwsp-tiller` and
`rust/target/debug/tiller` before driving.

This is not a drive-and-score pass over six rows. It is an investigation into whether
`tiller_project::layout`'s dead domain model (`WorkspaceLayout`, `LayoutNode`, `PaneGroup`,
`WorkspaceSnapshot`, `LegacyWorkspaceTab`, `WorkspaceContentRef`) should be wired, deleted, or
left alone, with the six `half-proven` `F-CORE-WSP` rows re-judged against whatever the shipped
app actually does instead.

## Method

For each row: (1) quote the clause and restate it in user-visible terms, (2) grep+read the real
app crates for an equivalent live mechanism, (3) where practical, **drive** that mechanism live
under the Wayland lane rather than trust the reading, (4) verdict on the evidence actually
produced, using the exact vocabulary from `EVIDENCE-STANDARD.md`.

Live drive: `Scripts/wayland-drive.sh` under `TILLER_WL_LABEL=wfwsp`/`wfwsp2`/`wfwsp3` (the first
two labels' nested compositors died mid-drive from host load — `uptime` read `load average: 46,
17` and `35, 21` on a 12-core box during those attempts, matching `ENVIRONMENT.md`'s documented
danger zone; `wfwsp3` succeeded). `ctl project.add path=/home/enzopalmisano/Scrivania/Progetti/
tiller-linux` registered this checkout as a project against a private `TILLER_DB`; a real terminal
tab was already open and selected. Screenshots live under `/tmp/wfwsp-shots/` (not committed —
`FINISH-window-persist.md`, the prior same-day pass over this exact area, set the precedent of
committing only the report, not its `/tmp` captures).

## Grep re-confirmation (my own, independent of the brief's claim)

```
$ grep -rn "WorkspaceLayout\|LayoutNode\|PaneGroup\|WorkspaceSnapshot\|LegacyWorkspaceTab\|
  WorkspaceContentRef\|WorkspaceTabViewState\|LayoutCommand\|classify_layout_command\|
  WorkspaceLayoutTransition\|FocusIntent" --include=*.rs rust/crates/tiller rust/crates/tiller_ui
  | grep -v "rust/crates/tiller_project"

rust/crates/tiller/src/main.rs:8634:  let command = tiller_project::LayoutCommand::Rename { ... };
rust/crates/tiller/src/main.rs:8638:  if tiller_project::classify_layout_command(&command).focus
rust/crates/tiller/src/main.rs:8639:      == tiller_project::FocusIntent::Tab
```

Confirms the brief's premise exactly: the only live call site anywhere outside `tiller_project`
is `commit_tab_rename`'s `LayoutCommand::Rename` + `classify_layout_command` check, which drives
`F-CORE-WSP-04` (already `PASSED`, not one of the six). A second grep, `LayoutCommand::` across
`main.rs`/`tiller_ui`, shows `Rename` is the **only** variant ever constructed — `Insert`, `Split`,
`Move`, `Close`, `Activate`, `SetDividerFraction`, `UpdateViewState` are constructed nowhere but
`layout.rs`'s own tests. `WorkspaceTabViewState`, `WorkspaceSnapshot`, `WorkspaceContentRef`,
`LegacyWorkspaceTab` and the four `*_content_id` helpers: zero hits anywhere outside `layout.rs`.

## The real parallel implementations

- **Splitting/closing panes**: `PaneNode<T>` in `crates/tiller/src/panes.rs` — a binary
  `Leaf`/`Split` tree with `SplitDirection::{Horizontal,Vertical}` and `ratio: f32`. Bound to
  live keys (`ctrl-alt-shift-Right`/`-Down` → `SplitPaneRight`/`SplitPaneDown` →
  `split_focused_terminal`) and to the command palette. `set_ratio` clamps to `[0.1, 0.9]`
  (tested: `ratios_are_clamped_and_survive_nested_splits`). `remove`/`take`/`remove_node` collapse
  a split into its surviving sibling on close, so an "empty non-root group" is unreachable **by
  construction**, not rejected by a validator.
- **Content kinds**: `TabKind` in `crates/tiller_project/src/tab.rs` (`Terminal`, `AgentChat`,
  `Browser`, `Editor`, `Diff`) — genuinely wired, 55 references in `main.rs`, imported at
  `main.rs:31`. This is the real, live analog of `layout.rs`'s dead `ContentKind`.
  `AgentActivityModel` (already critic-closed as `F-CORE-ACT`, 27 rows) is the real activity
  layer; documents/browser tabs are never registered in it, terminals are keyed by pane id.
- **Stable content identity**: `TerminalContentId` in `crates/tiller_activity/src/session.rs` — a
  live, wired (`main.rs:10430`/`10435`) opaque string wrapper keyed by pane id, used only to
  decide which saved agent-session refs survive a restore. It has no symlink-resolution or
  worktree-scoping logic — it is not `document_content_id`'s replacement, it solves a narrower,
  different problem (session-ref pruning, not general tab identity).
- **Document identity / dedup**: `add_file_tab` (`main.rs:6441`) calls
  `file_path_is_already_open`, which is `open_paths.iter().any(|p| p == path)` — **exact
  `PathBuf` equality, no `canonicalize`, no symlink resolution**. Grepping `rust/crates/tiller`
  and `tiller_ui` for `canonicalize` near file-open code returns nothing. So a symlink and its
  target open as **two separate tabs** in the shipped app — the opposite of what
  `document_content_id`'s own test (`document_identity_is_worktree_scoped_and_resolves_symlinks`)
  proves for the dead type.
- **Persistence**: `PaneEvent`/`SessionTabState` in `crates/tiller/src/session.rs` — an
  **event-log replay**, not a serialized tree snapshot. `PaneEvent::{Split,SetRatio,Close}` is
  written per-tab into the `tab_state.state` JSON column and replayed through the real `PaneNode`
  API on restore. `SessionTabState::decode` returns `Result`; its `Err` arm
  (`session.rs:1158-1167`) falls back to `SessionTabState::default()` with a logged diagnostic —
  the real analog of `WorkspaceSnapshot::decode_or_empty`'s malformed→empty fallback, but for a
  fundamentally different persistence strategy (no `schema_version` field, no canonical-JSON
  requirement, no future-version rejection — the DB's own `user_version` migration counter
  versions the whole schema, not this per-tab blob).
- **File-drop classification** (the smaller twin, see below): `drop_external_paths` in
  `crates/tiller_ui/src/chat.rs:2517`.

## Rows

### `F-CORE-WSP-01` — half-proven

> "A legacy workspace tab can hold a terminal split tree, markdown document, code document, or
> chat with agent/session IDs; terminal activity exposes leaf pane IDs and chat exposes its tab
> ID, while documents expose no activity pane."

User-visible claim: different tab kinds exist, and only some of them (terminal, chat) show up in
agent-activity tracking; plain documents never do.

The *shape* of this is real and live: `TabKind` (`Terminal`/`AgentChat`/`Browser`/`Editor`/
`Diff`, wired 55× in `main.rs`) is the shipped content-kind enum — a **superset** of `layout.rs`'s
dead `ContentKind` (it also covers `Browser`, which `LegacyWorkspaceTab` has no variant for at
all). `AgentActivityModel` (F-CORE-ACT, already critic-closed elsewhere in this ledger) is the
real, live activity layer, and by construction only terminal panes and chat tabs ever register
with it — editor/diff/browser tabs never spawn an agent and are never given a pane id to track.

What I did **not** drive this pass, and am not claiming: the precise per-kind *identity* contract
the row describes (`activity_pane_ids()` returning every leaf pane id for a split terminal tree
vs. `activity_tab_id()` returning one tab id for chat vs. neither for documents). I read the real
registration call sites enough to be confident the *shape* holds, but I did not exercise a split
terminal tab's multi-pane activity reporting against a chat tab's single-id reporting side by
side this session, and `layout.rs`'s dead type expresses this as a first-class API
(`LegacyWorkspaceTab::activity_pane_ids`/`activity_tab_id`) that has no direct successor by that
name — the real mechanism is diffused across `AgentActivityModel`'s registration call sites rather
than concentrated in one place I can point to and say "there, verified." Missing half: the exact
per-kind identity/activity **contract**, as opposed to the general kind-taxonomy-plus-activity
shape, which I did not personally exercise live.

### `F-CORE-WSP-02` — half-proven

> "Workspace content kinds include terminal, chat, document, diff, and browser, with stable
> string IDs for worktree, tab, terminal content, document, and browser content; document IDs
> resolve symlinks and are worktree-scoped."

User-visible claim, second half: open a file through a symlink and through its real path, and the
app treats them as the same open document (one tab, not two).

First half — the five content kinds — is live and real: exactly `TabKind`'s five variants,
confirmed by grep above. Second half — the stable, symlink-resolving, worktree-scoped ID scheme
— **does not exist anywhere in the shipped app**. Read `add_file_tab` (`main.rs:6441`): its dedup
check is `file_path_is_already_open`, `open_paths.iter().any(|p| p == path)` — plain `PathBuf`
equality. A `grep -rn canonicalize rust/crates/tiller/src rust/crates/tiller_ui/src` near any
file-open code returns nothing relevant. This is a validated negative, not a narrow grep: the
same search finds real `canonicalize` calls elsewhere in `main.rs` (project-path resolution,
`tillerctl` binary discovery) so the pattern is proven to hit when the behavior exists — it just
doesn't exist for document identity. Concretely: opening `alias.md` (a symlink to `note.md`) and
then opening `note.md` directly would open **two tabs** in the real app, not one — the opposite of
`document_content_id`'s own passing test. Missing half: symlink-resolving, worktree-scoped
document identity. `TerminalContentId` (the one real "stable content id" type that is live and
wired) only covers terminal-content identity for a narrower purpose (session-ref restore
pruning), not the general five-kind ID scheme this clause describes.

### `F-CORE-WSP-03` — PASSED — lives in `crates/tiller/src/panes.rs`'s `PaneNode<T>`

> "Workspace layouts are binary groups along horizontal or vertical axes; a valid empty layout
> has one empty group, and each split stores a fraction between 0 and 1."

User-visible claim: a pane region can be split along an axis, and the split has an adjustable,
bounded proportion. **Driven live this pass**, hard discriminator: focused a live terminal
(`click 700 500`), fired the real split gesture (`chord ctrl+alt+shift Right`, bound to
`SplitPaneRight`/`split_focused_terminal`), and the frame changed from one terminal to **two
side-by-side, independently-running PTYs** — confirmed genuinely distinct (not a redraw artifact)
by their live `neofetch` banners disagreeing on `Memory:`/`Swap:` at capture time (28%→31%
memory used on the two sides in successive captures — two different processes being sampled, not
one buffer copied), and by typing `echo WFWSP_SPLIT_MARKER_42` into the new (right) pane and
watching it land on that pane's own prompt line, not the original's. Screenshots:
`/tmp/wfwsp-shots/03-after-split.png`, `04-after-drag-resize.png` (this run's label `wfwsp3`).
Divider fraction is a real, bounded `f32` (`set_ratio` clamps to `[0.1, 0.9]`, narrower than the
dead type's `(0, 1)` — the real app never lets a pane shrink to nothing, an intentional
difference, not a bug) — tested live by `ratios_are_clamped_and_survive_nested_splits`, and the
divider-drag gesture I sent (`drag 810 500 600 500 6`) did not produce an unambiguous
before/after pixel delta I'd stake a claim on (the two captures differ mainly in how much
`neofetch` output had printed by shot time), so I am **not** claiming the drag itself as proven
live — the split and its bounded ratio are what carry this verdict.

One clause detail has no real analog and is worth naming rather than glossing over: the dead
type's "a valid empty layout has one empty group" describes a `PaneGroup` that can hold zero tabs
and still be legal. The real `PaneNode<T>` has no equivalent persistent state — its own doc
comment is explicit that `content: None` exists "only while an in-place tree operation temporarily
owns the node" and "a rendered tree always contains `Some`." This is a data-modeling invariant of
the dead type, not a user-observable behavior (nothing a person could see or trigger), so it does
not block the PASSED verdict above, but it means the two models are not simply the same shape
wearing different names — the real one deliberately cannot express "empty."

### `F-CORE-WSP-05` — half-proven

> "Structural layout commands report structural transitions and focus intent, while activation,
> fraction, view-state, and rename commands report nonstructural transitions."

User-visible claim: some operations (split/close/move/insert) should shift keyboard focus to a
tab in a structured way; others (resize, activate, rename) should not, or should focus a divider
instead of a tab.

One of eight command variants is genuinely live: `Rename` — this is exactly `F-CORE-WSP-04`'s
proof (already `PASSED`): `commit_tab_rename` (`main.rs:8634`) builds a real
`LayoutCommand::Rename`, calls `classify_layout_command`, and gates `focus_tab_content` on the
result being `FocusIntent::Tab` — live-verified (this host, same day, `FINISH-window-persist.md`):
renaming a tab and immediately typing into the chat composer landed the keystrokes correctly,
discriminating against the exact stranded-focus regression this classification exists to prevent.
I independently re-read this call site this pass and confirm it matches that report's claim.

The other seven variants — critically, **all four "structural" ones the clause is really about**
(`Insert`/`Split`/`Move`/`Close`) — are constructed nowhere outside `layout.rs`'s own tests
(confirmed by the `LayoutCommand::` grep above). The real split/close code
(`split_terminal_at_with_placement`, `request_close_focused_pane`) does not call
`classify_layout_command` or consult a `FocusIntent` at all — it has its own, separate focus
logic. So the *nonstructural* half of this row is live-proven (via Rename), but the *structural*
half — the part the row's own name emphasizes — is not exercised through this mechanism by the
shipped app at all; whatever focus behavior real split/close has, it does not go through the code
this row describes. Missing half: structural-command focus reporting (Insert/Split/Move/Close),
which the real app implements, if at all, through completely different, unaudited-by-this-row
code in `panes.rs`/`main.rs`.

### `F-CORE-WSP-06` — half-proven

> "Layout validation rejects empty nonroot groups, orphan or unresolved tabs/content, duplicate
> group/split/tab/content IDs, invalid active references, and nonfinite or out-of-range
> fractions."

User-visible claim: the app never lets a corrupted or self-contradictory pane layout reach the
screen — it validates and refuses/quarantines bad data instead.

The real app achieves the **outcome** this row cares about (garbage never corrupts the visible
pane tree) through a different strategy than a `validate()` function: (1) most of the listed
failure modes are unreachable **by construction** — `remove_node` always collapses a split into
its remaining sibling, so an empty non-root group cannot occur; ids are caller-assigned `usize`
counters from live tree operations, not deserialized data, so duplicate/orphan ids would require
a caller bug rather than a corrupted-input scenario; (2) for the one real external-data path —
restoring `SessionTabState` from the `tab_state.state` JSON column — `SessionTabState::decode`
returns `Result`, and its `Err` arm falls back to `SessionTabState::default()`
(`session.rs:1158-1167`) with a logged diagnostic. This fallback path is **live-proven**, though
not by me this pass — `FINISH-window-persist.md`'s `F-PERSIST-DB-07`, same host, same day,
directly corrupted a real `tab_state.state` row to `'{not valid json'` on disk, restarted the
process for real, and confirmed the app did not crash and moved the corrupted bytes into
`quarantine_record` with a genuine diagnostic reason. I re-read the exact code that report
describes (`session.rs`'s `decode`/`default` fallback, `db.rs`'s `quarantine_rows`) and confirm it
is real and matches the claim — this is not "same production path as row X" by assertion, it is
the same code, traced by me independently of that report's narrative.

What is **not** proven, live or otherwise: none of the row's *specific* checks — duplicate
group/split/tab/content-ID rejection, invalid-active-group/tab rejection — have a real analog to
exercise, because the real app never deserializes anything shaped like a `WorkspaceLayout` tree in
the first place. The malformed-data *outcome* (no crash, safe fallback) is proven; the specific
*validation rules* this row lists are not applicable to the real persistence shape and so were
never, and could never be, exercised against it. Missing half: the specific duplicate-ID /
invalid-reference checks, which have no real-app equivalent to test.

### `F-CORE-WSP-07` — half-proven

> "Workspace snapshots use schema version 1, canonical sorted JSON without escaped slashes,
> reject future or missing versions, and materialize malformed snapshots as an empty group
> registry."

User-visible claim: saved layout data is versioned so old/new builds don't silently misread each
other, and corrupted saved data comes back as an empty layout rather than a crash.

The "malformed data comes back empty" half is live-proven — same evidence as `F-CORE-WSP-06`
above (`F-PERSIST-DB-07`'s real corruption + restart test, `session.rs`'s `decode`/`default`
fallback, independently re-traced by me). The "schema version 1 / canonical sorted JSON / reject
future or missing versions" half has no real analog: `SessionTabState` (the thing actually
persisted per tab) has no `schema_version` field, no future-version rejection, and
`serde_json::to_string` on it is given no canonical-ordering treatment beyond what `BTreeMap`
happens to provide for the `scrollback` field alone — `pane_events`/`chat_draft` are plain
`Vec`/`String` fields with no stability guarantee, and there is no version number in the blob to
reject a "future" one against. The database's own `user_version` migration counter
(`tiller_persistence`'s `migrate_v1..v13`) is a real, live, tested schema-versioning mechanism —
but it versions the **whole SQLite schema**, not this per-tab JSON blob, so it is a different
mechanism solving a related-but-not-identical problem. Missing half: per-blob schema versioning
and canonical-JSON-without-escaped-slashes, which nothing in the real persistence path provides.

## Recommendation on the dead code

**Delete `LegacyWorkspaceTab`, `WorkspaceContentRef`/the four `*_content_id` helpers,
`WorkspaceLayout`/`LayoutNode`/`PaneGroup`/`SplitAxis`/`LayoutError`, and `WorkspaceSnapshot`/
`SnapshotError` from `tiller_project::layout` — but keep `LayoutCommand`, `FocusIntent`,
`LayoutTransition`, and `classify_layout_command`, since `F-CORE-WSP-04`/`-05`'s one live call site
(`commit_tab_rename`) genuinely depends on them today.** The rest of the module is not a partial
implementation waiting to be finished — it is a second, structurally-similar-but-independently-
maintained pane/tab/persistence model that the app already replaced, cleanly, with three separate
real mechanisms (`panes.rs::PaneNode<T>` for splits, `tiller_project::tab::TabKind` for content
kinds, `session.rs::PaneEvent`/`SessionTabState` for persistence) that between them deliver
everything the six rows above describe in user-visible terms, at least as well and in one case
(document dedup by exact path, not symlink-resolved) *differently on purpose*. Wiring the dead
model instead would mean either running two parallel split-tree implementations that must be kept
in lockstep forever, or a real migration that rewrites `panes.rs` and `session.rs` to be backed by
it — neither is "finish a gap," both are a rewrite of working code for no behavioral gain I found
evidence for. Nothing here would drop real behavior if deleted: every user-visible capability the
six rows describe already has a live owner elsewhere in the tree (WSP-01/02's precise identity
contracts are the one place real capability is actually *missing* from the app — that is a
`half-proven`/gap finding about the **app**, not a reason to keep the dead **code**, since the
dead code was never wired to provide it either).

## `F-CORE-FILE-02` twin — confirmed independently

`classify_file_drop` (`crates/tiller_project/src/file.rs:66`) has zero callers outside its own
module: `grep -rn classify_file_drop rust/crates` returns only `file.rs`'s own `pub use`
re-export and its four unit tests. The real, live drop-handling code is
`drop_external_paths` in `crates/tiller_ui/src/chat.rs:2517`. I read both implementations in
full: they are **independently written, not shared code**, but strikingly close in behavior —
same accepted extensions (`png`/`jpg`/`jpeg`/`gif`/`webp`), the same 10 MB cap
(`classify_file_drop`'s `MAX_DROPPED_IMAGE_BYTES` vs. `drop_external_paths`'s inline
`MAX_IMAGE_BYTES`, both `10 * 1024 * 1024`), and the same relative-vs-absolute path distinction
(`classify_file_drop`'s `strip_prefix` vs. `drop_external_paths`'s in-tree/outside-tree chip
logic). `F-CORE-FILE-02` is already `PASSED` in the ledger on the strength of a real live XDND
drive (image accept, oversized reject with the exact message, in/out-of-tree chips) — that drive
was exercising `chat.rs`, not `file.rs`, and this pass confirms that attribution is correct:
the dead `classify_file_drop` is not what a user's drop ever reaches.

## Summary table

| row | verdict | real route |
| --- | --- | --- |
| `F-CORE-WSP-01` | half-proven | `TabKind` + `AgentActivityModel` (shape proven, exact per-kind identity contract not driven) |
| `F-CORE-WSP-02` | half-proven | `TabKind` (kinds, live); document symlink/worktree-scoped identity absent app-wide |
| `F-CORE-WSP-03` | **PASSED** | `panes.rs::PaneNode<T>` — live-driven real split this pass |
| `F-CORE-WSP-05` | half-proven | `LayoutCommand::Rename` path (already proven via WSP-04) only; structural commands (Insert/Split/Move/Close) don't use this mechanism at all |
| `F-CORE-WSP-06` | half-proven | outcome (no crash, safe fallback) proven via `SessionTabState::decode`/quarantine; specific duplicate-ID/reference checks have no real analog |
| `F-CORE-WSP-07` | half-proven | malformed→empty fallback proven (same evidence as WSP-06); schema-version/canonical-JSON half absent |
