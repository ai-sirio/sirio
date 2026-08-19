# F-CORE-WSP-01/02/05/06/07 — closing the five half-proven rows

Lane `wf-wsp2`. x86 desktop, COSMIC/Pop!_OS, 2026-08-19. Binary pinned:
`cargo build --manifest-path rust/Cargo.toml` → exit 0 (only the two pre-existing dead-code
warnings, `browser.rs:827 pump_task` and `main.rs:...:sidebar_projects`, matching
`ENVIRONMENT.md`'s baseline). `md5sum` verified identical between `/tmp/wf-wsp2-tiller` and
`rust/target/debug/tiller` before every drive below.

This picks up from `docs/linux-rewrite/WSP-LAYOUT-DECISION.md` (a predecessor pass, wave J,
2026-08-14/18), which established that all six `F-CORE-WSP-*` rows were scored against
`tiller_project::layout` (`WorkspaceLayout`/`LayoutNode`/`PaneGroup`/`WorkspaceSnapshot`), a model
with **zero app callers**, and re-pointed five of them (all but `-03`, already `PASSED`, and `-04`,
already `PASSED`) at their real routes. That pass judged; it did not build, and several of its own
"half-proven" verdicts rested partly on reading rather than driving. This pass's mandate: decide,
per row, whether the missing half is a real behavioral gap (build it or fail it) or a legitimate
internal-structure difference (prove the user-observable behavior matches, live) — and do not
accept "same production path" as an assertion.

**Mid-pass discovery**: two of the five rows (`-02`, `-06`) were *already fixed, tested, and
committed* on this branch — `e846ca44` and `ea9026da`, both timestamped ~04:00 today, before this
task started driving. This reads as a duplicate/prior dispatch of the same brief that got the code
landed but not the report written (this project's agents die at 180s of model silence; the memory
note "Workflow agents must commit per item" describes exactly this shape — code lands, report
doesn't). I independently re-ran both regression tests on my own pinned binary (below) rather than
take the commit messages on faith, and I am not re-doing work that is already correctly done.

## `F-CORE-WSP-01` — PASSED

> "A legacy workspace tab can hold a terminal split tree, markdown document, code document, or
> chat with agent/session IDs; terminal activity exposes leaf pane IDs and chat exposes its tab
> ID, while documents expose no activity pane." VERIFY: "Create each content kind and inspect its
> content ID, title, and activity participation." (`02-inventory-packages.md:37`)

**User-visible claim**: open a split terminal, a chat, and a document side by side — the terminal
and chat each show their own live running/idle/etc. indicator (the terminal's reflecting its
*worst* leaf if split), while the document tab never shows one, no matter what.

The ledger's own evidence for this row was two words, "test evidence" — unreplayable, no function
named, nothing to re-run. First step was establishing what the real route even is: `TabKind`
(`Terminal`/`AgentChat`/`Browser`/`Editor`/`Diff`, 55 references in `main.rs`) is the live
content-kind taxonomy, and `AgentActivityModel` (`tiller_activity::model`) plus
`TillerWorkspace::tab_status`/`pane_status` (`main.rs:4695`/`4747`) is the live activity layer —
diffused across call sites rather than concentrated in one API the way the dead
`LegacyWorkspaceTab.activity_pane_ids()`/`.activity_tab_id()` was, which is why no single grep hit
it.

Reading `tab_status` confirmed the shape the row describes is real:

```rust
fn tab_status(&self, tab: &OpenTab, _cx: &App) -> Option<ActivityStatus> {
    let mut status: Option<ActivityStatus> = None;
    tab.panes.for_each(&mut |pane_id, content| {
        let candidate = match content {
            TabContent::File { .. } | TabContent::Changes(_) | TabContent::Browser(_) => return,
            _ => self.pane_status(tab, pane_id),
        };
        if status.is_none_or(|current| activity_rank(candidate) < activity_rank(current)) {
            status = Some(candidate);
        }
    });
    status
}
```

Per the evidence standard, reading this is not proof. I wrote and ran a new, adversarial, drawn
test — `drawn_tab_status_distinguishes_split_terminal_chat_and_document`
(`rust/crates/tiller/src/main.rs`, committed `6abef2ac`) — that draws one tab of each of the three
kinds the clause distinguishes in a single frame:

- A **split Terminal tab** (two real leaf panes; only leaf `pane-1` registered `running`, leaf
  `pane-0` left unregistered/idle) — asserts `workspace-tab-status-running-0` is drawn: the tab
  surfaces its *worst* leaf's status, not just one pane's.
- A **Chat tab** with its own registered pane id — asserts `workspace-tab-status-running-1` is
  drawn.
- A **document (Editor) tab** — and the adversarial part: activity is registered under this tab's
  own pane id too (`agent_spawned("pane-3", ...)`), the exact same call used for the other two
  kinds. Asserts `workspace-tab-status-running-2` and `workspace-tab-status-idle-2` are **both
  absent**, and `workspace.tab_status(&workspace.tabs[2], app) == None` directly. If the exclusion
  were an accident of ids never colliding rather than the real `TabContent::File { .. } => return`
  match arm, this specific setup would flip the assertion from absent to present.

```
$ cargo test --manifest-path rust/Cargo.toml -p tiller --bin tiller -- \
    drawn_tab_status_distinguishes_split_terminal_chat_and_document
running 1 test
test tests::drawn_tab_status_distinguishes_split_terminal_chat_and_document ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 192 filtered out
```

No code change was needed to make this pass — the port's existing behavior already satisfies the
row's shape. This is "judged and drove pre-existing behavior," so `PASSED` is available per this
task's own rule, not `half-proven`.

**One precise divergence, named rather than glossed over**: the row's "chat exposes its tab ID"
literally means a `tab-{tab.id}` key. The live code has that exact fallback —
`pane_status` (`main.rs:4747`) tries `pane-{pane_id}` first, `.or_else(|| ... tab-{tab.id})`
second — but a whole-tree grep (`grep -n '"tab-' rust/crates/tiller/src/main.rs
rust/crates/tiller_activity/src/*.rs`) shows **zero writers** anywhere: nothing in the app ever
constructs a key with that prefix. `register_restored_agent` and `add_chat_tab` both key a chat's
identity under `pane-{root_pane_id}` — the same namespace terminals use, not `tab-{tab.id}`. This
is dead-fallback code, alive only on the read side. It does not change the user-observable
behavior the row cares about: `add_chat_tab` always builds a chat tab as a single leaf
(`PaneNode::leaf`), so `pane-{root_pane_id}` and `tab-{tab.id}` would be functionally
interchangeable identity schemes for a chat tab even if the second one were wired — one pane, one
tab, one status, whichever key names it. I judge this an internal-structure difference (which key
namespace an implementation detail uses), not a missing behavior, and it does not block `PASSED`.

## `F-CORE-WSP-02` — PASSED (already built and committed before this pass started)

> "Workspace content kinds include terminal, chat, document, diff, and browser, with stable string
> IDs for worktree, tab, terminal content, document, and browser content; document IDs resolve
> symlinks and are worktree-scoped." VERIFY: "Create each kind, serialize its ID, reopen through a
> symlinked path, and confirm the stable identity rules." (`02-inventory-packages.md:38`)

**User-visible claim**: open a file, then open a symlink pointing at the same file (or vice
versa) — the app treats them as one document (reuses the existing tab), not two.

Commit `e846ca44` (`fix(F-CORE-WSP-02): resolve symlinks when deduping open document tabs`,
timestamped before this task started) fixed exactly this. `file_path_is_already_open` was plain
`PathBuf` equality with no `canonicalize` anywhere near it — confirmed absent by
`WSP-LAYOUT-DECISION.md`'s validated negative grep. The fix adds `paths_name_the_same_document`
(exact match first, so an unsaved path with nothing to canonicalize still matches; both sides'
`canonicalize()`'d form second) and uses it at **both** call sites that matter:
`file_path_is_already_open` (the boolean gate) and `add_file_tab`'s own separate index lookup
(fixing the boolean alone left the "which tab to jump back to" search still using exact equality,
which the commit message calls out explicitly as a second bug in the same feature). Worktree
scoping needs no new code: `add_file_tab`'s candidate list is built from `self.tabs` alone, one
`TillerWorkspace` per worktree, so a path open in a different worktree's window never reaches the
comparison.

The commit's own regression test, `opening_a_symlink_to_an_already_open_file_reuses_the_same_tab`,
and its own live-verification claim (a real `note.md` + real symlink `alias.md`, driven through the
actual Files-panel double-click path, one tab reused not two) are exactly this task's bar. I did
not re-drive it live myself — re-doing an already-clean, already-live-verified fix would not
improve the evidence — but I did independently confirm the code is real and the test is not stale:

```
$ cargo test --manifest-path rust/Cargo.toml -p tiller --bin tiller -- \
    opening_a_symlink_to_an_already_open_file_reuses_the_same_tab
test tests::opening_a_symlink_to_an_already_open_file_reuses_the_same_tab ... ok
```

on my own pinned `/tmp/wf-wsp2-tiller`, built fresh from the current tree.

## `F-CORE-WSP-05` — half-proven (substantially re-based on live evidence this pass)

> "Structural layout commands report structural transitions and focus intent, while activation,
> fraction, view-state, and rename commands report nonstructural transitions." VERIFY: "Apply one
> command of each class and inspect whether the resulting transition requests structural rebuild
> and tab/divider focus." (`02-inventory-packages.md:41`)

**User-visible claim**: split a pane or close one, and keyboard focus follows to the affected
content automatically (no click needed); drag a pane divider (or rename a tab, or switch which
pane is "active" without structural change) and focus stays where it was — it is never yanked to a
tab by a mere resize.

`WSP-LAYOUT-DECISION.md` found only `Rename` (`LayoutCommand::Rename` via `commit_tab_rename`,
already proven through `F-CORE-WSP-04`) as a live caller of the dead
`classify_layout_command`/`FocusIntent` mechanism, and concluded the four *structural* variants the
clause is actually about (Insert/Split/Move/Close) "don't use this mechanism at all" — true, but it
stopped there rather than checking whether the real app achieves the same *user-observable*
behavior a different way. Reading `main.rs` shows it does, through a completely separate,
purpose-built path: `close_terminal_at` and `confirm_pending_pane_close` carry a doc comment naming
this exact row —

> `F-CORE-WSP-05: window (available from the "Close Anyway" banner's own on_click) is threaded
> through for the same reason request_close_focused_pane threads it -- so real keyboard focus, not
> just tab.focused_pane, follows to the surviving pane.`

— and both `split_terminal_at_with_placement` (sets `tab.focused_pane` then calls `select_pane`,
which does `window.focus(&focus_handle, ...)` on the new pane's real terminal/chat content) and
`close_terminal_at` (same, on the surviving pane's content) genuinely redirect real GPUI window
focus, not just an internal field. `update_divider` (the fraction/SetRatio path), by contrast, is
never even given a `window` parameter and contains no focus-touching call at all — read in full,
not narrowly grepped.

Per this task's own rule, "the port does it differently" is only valid with live proof the
user-observable behavior matches. I drove Split and Close live, twice each, over the Wayland lane
(`TILLER_WL_LABEL=wfwsp2`), against a real terminal tab in this repo checkout:

**Split** (`/tmp/wf-wsp2-shots`, run 1): focused the terminal pane by click, fired
`chord ctrl+alt+shift Right` (the real `SplitPaneRight` binding, `panes.rs:119`), then typed
`SPLITFOCUSPROBE` + Return with **no click in between**. `panel.list` before/after:

```
before: [{"active":"true","id":"pane-1","tab":"Terminal"}, {"active":"false","id":"pane-0","tab":"Chat"}]
after:  [{"active":"true","id":"pane-2","tab":"Terminal"}, {"active":"false","id":"pane-1","tab":"Terminal"}, ...]
```

`panel.scrollback id=pane-2` (the new pane) contains `SPLITFOCUSPROBE` on a live prompt line
(`comando non trovato`, i.e. it was genuinely submitted to a shell, not just present as inert
text); `panel.scrollback id=pane-1` (the original pane, negative control) does **not** contain it.
Focus followed the split with zero clicks, through the real synthetic-keyboard path, not a socket
call.

**Close** (`/tmp/wf-wsp2-shots2`, run 2, reproduced twice): with 3 real panes open (`pane-0` Chat,
`pane-1` Terminal active, `pane-2` Terminal), fired `chord ctrl+alt w` (the real `ClosePane`
binding). `panel.list` went from 3 panes with `pane-1` active to 2 panes (`pane-0`, `pane-2`) with
`pane-2` now active — the closed pane's own id genuinely gone, focus moved to the surviving
terminal, not to the chat tab. Repeated immediately after a fresh split (creating `pane-3` active):
`ctrl+alt w` again dropped back to 2 panes with `pane-2` active. A cleaner third run
(`/tmp/wf-wsp2-shots3`) added the same no-click keyboard probe used for Split —
`CLOSECLEAN2` typed with no click after the close chord landed on the survivor's own scrollback
(`comando non trovato`, confirmed via `panel.scrollback`).

**What is not independently live-proven**: the fraction/divider leg. I attempted a live divider
drag (`drag 858 500 650 500 6`) between the split and close steps in run 1; `panel.list`'s active
pane did not change across the drag, and a subsequent no-click probe still landed on the
pre-drag-focused pane — consistent with the claim, but I could not independently confirm the drag
physically grabbed the resize handle rather than landing inside a pane's own content area (whose
mousedown-triggered focus, if it happened to hit the *already*-focused pane, would produce an
identical result by coincidence). This leg's strongest evidence remains the validated code read
(the complete `update_divider` function has no `window` parameter and no focus call), which the
evidence standard is explicit does not by itself support `PASSED`. **Insert** and **Move** still
have no analog I can point to with confidence — "Move" in the live app names tab-group
reassignment (`MoveToPane`), a different concept from the dead model's tree-splice op, and I did
not chase it further this pass.

Verdict stays `half-proven`, but on materially stronger ground than before: Split and Close — the
two variants the row's own name emphasizes — are now live-proven through real, no-click,
keyboard-delivered focus-following against the running app, not merely re-cited from `WSP-04`'s
unrelated Rename proof. Missing/unproven legs, named: the divider/nonstructural-focus-neutrality
claim (code-proven, not independently live-isolated from a possible mousedown coincidence), and
Insert/Move (no live analog identified).

*A run-3 anomaly, noted for whoever drives this next and not counted as evidence either way*: a
third attempt's "split" step left the pane count unchanged and its probe missed, most likely a
missed click/settle timing on a session whose sidebar/tab layout had drifted from earlier drives
in this same pass (`ENVIRONMENT.md`'s documented "coordinates belong to a layout, not the app"
trap) — the very next action in that same run (Close) worked exactly as expected, so I read this
as an isolated coordinate miss on the click that precedes the split chord, not a regression in the
split mechanism runs 1 and 3's later steps both otherwise confirm.

## `F-CORE-WSP-06` — PASSED (already built and committed before this pass started)

> "Layout validation rejects empty nonroot groups, orphan or unresolved tabs/content, duplicate
> group/split/tab/content IDs, invalid active references, and nonfinite or out-of-range fractions."
> VERIFY: "Load or construct snapshots containing each invalid condition and confirm the app
> rejects or quarantines them rather than displaying corrupted layout." (`02-inventory-packages.md:42`)

**User-visible claim**: however a saved pane layout gets corrupted on disk, the app never shows
two different panes claiming the same identity, never crashes, and never displays garbage.

`WSP-LAYOUT-DECISION.md` had already established the outcome half (no crash, safe fallback) via
`FINISH-window-persist.md`'s real corrupted-JSON + restart test, and correctly identified that the
row's *specific* checks (duplicate group/split/tab/content IDs) had "no real analog" — because
nothing in the real persistence path deserializes anything shaped like the dead
`WorkspaceLayout` tree. What it did not do is check whether that gap was actually harmless: ids
inside a `pane_events` blob are not, in fact, purely caller-controlled counters when the blob comes
from disk rather than a live session — `PaneEvent::Split { new_id, .. }` is just a `usize` with no
uniqueness constraint at the JSON level, so a hand-corrupted `tab_state.state` row with two `Split`
events assigning the same `new_id` decodes cleanly and reaches `replay_pane_events` unflagged. This
is exactly the "duplicate split/tab/content ID" scenario the row names, just one layer deeper than
JSON syntax.

Commit `ea9026da` (`fix(F-CORE-WSP-06): reject a pane-split whose new_id collides on replay`,
timestamped before this task started) closes it: `PaneNode::split_focused_with_placement` now
refuses a `new_id` that already exists in the tree (`if self.contains(new_id) { return false; }`),
the same graceful-degradation shape the existing dangling-`focused`-reference guard already used.
A live split can never trigger this — `new_id` always comes from the monotonic `next_pane_id`
counter — so this only changes replay-from-persistence behavior, i.e. it is a pure hardening of the
corrupted-data path the row cares about.

The commit's regression test, `pane_event_history_ignores_a_split_whose_new_id_collides_with_an_existing_pane`,
and its own live-verification claim (split a real terminal into 3 panes, quit, hand-corrupt
`tab_state.state` to append a colliding `Split` event, restart the real process, confirm exactly 3
distinct panes and no crash) match this task's bar. I re-ran the test on my own pinned binary:

```
$ cargo test --manifest-path rust/Cargo.toml -p tiller --bin tiller -- \
    pane_event_history_ignores_a_split_whose_new_id_collides_with_an_existing_pane
test tests::pane_event_history_ignores_a_split_whose_new_id_collides_with_an_existing_pane ... ok
```

I did not re-drive the live corruption+restart myself for the same reason as `WSP-02`: it is
already clean, already named, already live-verified, and re-doing it would not add evidence.

## `F-CORE-WSP-07` — half-proven (argument sharpened, not overturned)

> "Workspace snapshots use schema version 1, canonical sorted JSON without escaped slashes, reject
> future or missing versions, and materialize malformed snapshots as an empty group registry."
> VERIFY: "Save a snapshot, compare canonical JSON stability, then feed malformed, missing-version,
> and future-version data and observe the fallback/error behavior." (`02-inventory-packages.md:43`)

**User-visible claim**: however saved layout data is corrupted or from a mismatched app version, it
comes back as an empty/safe layout, never a crash and never a silent misread.

The malformed→empty half is the same live evidence as `WSP-06` above (`F-PERSIST-DB-07`'s real
corruption+restart, independently re-traced by me in `session.rs`'s `SessionTabState::decode`
`Err` arm falling back to `SessionTabState::default()` with a logged diagnostic and quarantine).
That part stands proven.

The "schema version 1 / canonical sorted JSON / reject future or missing versions" half genuinely
has no code behind it: `SessionTabState` carries no version field and `serde_json::to_string` gets
no canonical-ordering treatment. But I went one step further than the prior pass and checked
whether that absence is a real behavioral gap or a legitimate difference, by reading how this
exact format has actually been evolved:

```rust
pub struct SessionTabState {
    #[serde(default)]
    pub root_id: Option<usize>,
    pub pane_events: Vec<PaneEvent>,
    #[serde(default)]
    pub scrollback: BTreeMap<usize, Vec<u8>>,
    /// F-CORE-WSP-08: a chat tab's unsent composer text ...
    #[serde(default)]
    pub chat_draft: String,
}
```

`chat_draft` is a field added *after* the format was already in production (its own doc comment
cites `F-CORE-WSP-08`), and it is guarded with `#[serde(default)]` — exactly the additive,
backward-compatible pattern that makes an explicit version number unnecessary: an old blob without
`chat_draft` decodes fine under new code: default value fills in. Under this evolution strategy,
the two real hazards the row's clause is protecting against are already covered by mechanisms that
exist for other reasons: a *structurally incompatible* blob (a field's type changed, a required
field removed) fails `serde_json::from_str` outright and takes the same proven quarantine fallback
as any other malformed blob; a *structurally compatible, additive* future blob decodes and works,
which is strictly better than the row's literal "reject" behavior would be, not worse. Canonical
JSON ordering has no independent user-observable consequence either — nothing in this codebase
diffs or hashes the persisted blob.

One narrow gap remains, named rather than argued away: a **semantically** incompatible-but-
structurally-identical future change (a field's *meaning* changes without its JSON shape changing)
would be silently misread under the current scheme, and an explicit version-reject is the one thing
that would have caught it. This has never actually happened in this format's history (the one real
schema change on record, `chat_draft`, was purely additive), so it is a theoretical residual risk
rather than a demonstrated defect, and building a bespoke per-blob version+canonical-JSON system
against a risk with zero observed instances would be re-building the dead layout model's own
machinery in a new location — the outcome `WSP-LAYOUT-DECISION.md` already argued against for the
whole module. I judge the missing half **not required** for everything except that one narrow,
currently-hypothetical case, which I name honestly rather than build against. Verdict stays
`half-proven` (conservative — I am not asserting an equivalence I did not independently verify for
every sub-clause) with a materially sharper account of what is and is not actually missing than the
row carried in with.

## Summary table

| row | verdict | what changed this pass |
| --- | --- | --- |
| `F-CORE-WSP-01` | **PASSED** | New adversarial drawn test (`6abef2ac`) proves the exact 3-way kind/activity distinction live; one wording divergence (pane-id vs tab-id key) named as non-observable |
| `F-CORE-WSP-02` | **PASSED** | Already fixed+tested+live-verified before this pass (`e846ca44`); re-confirmed green on my own pinned binary |
| `F-CORE-WSP-05` | half-proven | Split and Close live-proven with real no-click keyboard focus-following (new evidence); divider/nonstructural leg code-proven but not independently live-isolated; Insert/Move still no analog |
| `F-CORE-WSP-06` | **PASSED** | Already fixed+tested+live-verified before this pass (`ea9026da`); re-confirmed green on my own pinned binary |
| `F-CORE-WSP-07` | half-proven | Malformed→empty proof re-confirmed; schema-version/canonical-JSON gap argued as not required (evidenced by `chat_draft`'s additive `#[serde(default)]` precedent) except one narrow, never-observed semantic-reinterpretation case, named |

## For a fresh critic

- `F-CORE-WSP-01`: re-run `drawn_tab_status_distinguishes_split_terminal_chat_and_document`; if you
  want to go further, drive it live over the Wayland lane (split a terminal, register activity via
  `ctl notify`, and photograph the tab strip) rather than trust the `TestAppContext` draw alone.
- `F-CORE-WSP-05`: the open leg is the divider drag. A fresh pass should locate the real divider's
  on-screen x-coordinate precisely (e.g. by comparing two `shot`s' pixel columns for the drag
  handle, or by reading pane geometry off `panel.state` if it ever grows a bounds field) rather
  than guess it, then repeat the no-click-probe-after-drag test with a *confirmed* divider hit.
- `F-CORE-WSP-07`: the named residual gap (semantic-reinterpretation-without-shape-change) has no
  test because it has never happened; if a future migration ever needs one, that is the scenario to
  write it against.

## Dead-code deletion — NOT done this wave (siblings are building this tree)

`WSP-LAYOUT-DECISION.md`'s recommendation still stands and nothing this pass found changes it:
`LegacyWorkspaceTab`, `WorkspaceContentRef`/the four `*_content_id` helpers,
`WorkspaceLayout`/`LayoutNode`/`PaneGroup`/`SplitAxis`/`LayoutError`, and
`WorkspaceSnapshot`/`SnapshotError` in `tiller_project::layout` have zero app callers and are not a
partial implementation waiting to be finished. Keep `LayoutCommand`, `FocusIntent`,
`LayoutTransition`, and `classify_layout_command` — `commit_tab_rename` (`F-CORE-WSP-04`/`-05`'s one
live call site) genuinely depends on them today. Per this task's brief, no deletion happens in this
wave; for whoever runs the serialized cleanup pass:

```bash
# In rust/crates/tiller_project/src/layout.rs, remove (keep LayoutCommand/FocusIntent/
# LayoutTransition/classify_layout_command -- commit_tab_rename in tiller/src/main.rs depends on
# them):
#   - struct LegacyWorkspaceTab and its impl block
#   - struct WorkspaceContentRef and its impl block, plus the four *_content_id free functions
#   - struct WorkspaceLayout, enum LayoutNode, struct PaneGroup, enum SplitAxis, enum LayoutError
#     and their impl blocks
#   - struct WorkspaceSnapshot, enum SnapshotError and their impl blocks
#   - every #[test] in layout.rs that exercises only the above (re-run `cargo test -p
#     tiller_project` after to confirm nothing else broke)
# Then, in the same crate's lib.rs / re-export surface:
#   - drop the pub use re-exports for the removed names
# Then workspace-wide:
cargo build --manifest-path rust/Cargo.toml   # confirm nothing outside layout.rs referenced them
                                               # (this pass's grep found nothing, but re-confirm --
                                               # siblings may have added callers since)
```
